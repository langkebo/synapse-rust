//! 管理端注册的**门禁分支**：IP 白名单 / captcha / 人工审批 / 未启用 / 验签失败。
//!
//! ## 为什么单独一个文件
//!
//! `synapse-web/src/routes/admin/register.rs` 的 handler 里有 186 行可执行代码，
//! 2026-10-28 的完整报告只命中 126 行（67.7%）。逐行读 lcov 的 `DA:` 行后发现缺口
//! **全部落在"非默认配置"的分支**上：
//!
//! * `ensure_admin_registration_ip_policy` 的 135–152 行 —— 默认 `ip_whitelist` 为空，
//!   函数在第 132 行就 `return Ok(())`，白名单逻辑一行都没跑；
//! * `verify_additional_registration_controls` 的 captcha（175–192）与人工审批（196–203）
//!   —— 默认两个开关都是 `false`；
//! * `get_nonce` 的"未启用"（220）/ "缺少 shared_secret"（225）、`register` 的
//!   校验失败（248）与"未启用"（255）；
//! * `map_admin_register_service_error`（83–95）整个函数。
//!
//! 这些不是"覆盖率数字不够好看"，而是**授权控制**：IP 白名单与人工审批令牌配错时
//! 的表现是"任何人只要知道 nonce 流程就能注册管理员"。默认配置里它们关着，
//! 所以永远没有用例走到 —— 而一旦运维打开它们，正是最需要有人验证的时候。
//!
//! ## 与既有用例的分工
//!
//! `api_input_validation_tests.rs` 里的两条 `x-forwarded-for` 用例其实测的是
//! **`LocalhostGuard`**（消息是 "Admin registration is only available from localhost"），
//! 不是 handler 的 IP 白名单。本文件把 `allow_external_access` 打开让 guard 放行，
//! 从而真正打到 handler 的那一层（消息是 "Admin registration source IP is not allowed"）——
//! 两层的拒绝消息不同，断言里直接区分，避免"绿了但测的是另一层"。

use axum::{
    body::Body,
    http::{Request, StatusCode},
};
use serde_json::{json, Value};
use tower::ServiceExt;

use super::{config_mut, with_local_connect_info};

const NONCE_URI: &str = "/_synapse/admin/v1/register/nonce";
const REGISTER_URI: &str = "/_synapse/admin/v1/register";

async fn send(app: axum::Router, request: Request<Body>) -> (StatusCode, Value) {
    let response = app.oneshot(with_local_connect_info(request)).await.expect("router must respond");
    let status = response.status();
    let body = axum::body::to_bytes(response.into_body(), 64 * 1024).await.expect("body must be readable");
    (status, serde_json::from_slice(&body).unwrap_or(Value::Null))
}

fn get_nonce_request(x_forwarded_for: Option<&str>) -> Request<Body> {
    let mut builder = Request::builder().method("GET").uri(NONCE_URI);
    if let Some(ip) = x_forwarded_for {
        builder = builder.header("x-forwarded-for", ip);
    }
    builder.body(Body::empty()).expect("valid request")
}

/// 注册请求的最小合法骨架（各用例只改要测的那一处）。
fn register_body(overrides: &Value) -> Body {
    let mut payload = json!({
        "nonce": "not-a-real-nonce",
        "username": format!("admin_probe_{}", rand::random::<u32>()),
        "password": "AdminTest@123",
        "admin": true,
        "mac": "00",
    });
    for (key, value) in overrides.as_object().expect("overrides must be an object") {
        payload[key] = value.clone();
    }
    Body::from(payload.to_string())
}

fn post_register(body: Body) -> Request<Body> {
    Request::builder()
        .method("POST")
        .uri(REGISTER_URI)
        .header("Content-Type", "application/json")
        .body(body)
        .expect("valid request")
}

/// `enabled = false` 时 nonce 端点必须明确拒绝，而不是发一个"之后也用不了"的 nonce。
#[tokio::test]
async fn nonce_is_refused_when_admin_registration_is_disabled() {
    let Some((app, _state)) = super::setup_fresh_test_app_with_config(|container| {
        config_mut(container).admin_registration.enabled = false;
    })
    .await
    else {
        return;
    };

    let (status, body) = send(app, get_nonce_request(None)).await;

    assert_eq!(status, StatusCode::BAD_REQUEST, "未启用时必须拒绝：{body}");
    assert_eq!(body["error"], "Admin registration is not enabled");
}

/// `enabled` 但 `shared_secret` 为空：nonce 能发，却永远算不出合法 HMAC。
///
/// 这条是"配了一半"的典型形态 —— 此时最容易被误读成"注册接口坏了"。
#[tokio::test]
async fn nonce_is_refused_when_the_shared_secret_is_missing() {
    let Some((app, _state)) = super::setup_fresh_test_app_with_config(|container| {
        let config = config_mut(container);
        config.admin_registration.enabled = true;
        config.admin_registration.shared_secret = String::new();
    })
    .await
    else {
        return;
    };

    let (status, body) = send(app, get_nonce_request(None)).await;

    assert_eq!(status, StatusCode::BAD_REQUEST, "{body}");
    assert_eq!(body["error"], "Shared secret is not configured");
}

/// 白名单不含回环地址 ⇒ 直接 403。
///
/// 断言 message 而不是只断言 403：`LocalhostGuard` 也会返回 403（另一句话），
/// 只判状态码会把"guard 拦住了"误当成"白名单生效了"。
#[tokio::test]
async fn nonce_is_refused_when_the_peer_is_outside_the_ip_whitelist() {
    let Some((app, _state)) = super::setup_fresh_test_app_with_config(|container| {
        let config = config_mut(container);
        config.admin_registration.enabled = true;
        // 回环地址不在这个网段里 ⇒ 只可能是 handler 的白名单判据拒绝的。
        config.admin_registration.ip_whitelist = vec!["10.0.0.0/8".to_string()];
    })
    .await
    else {
        return;
    };

    let (status, body) = send(app, get_nonce_request(None)).await;

    assert_eq!(status, StatusCode::FORBIDDEN, "{body}");
    assert_eq!(
        body["error"], "Admin registration source IP is not allowed",
        "必须是 handler 的白名单判据拒绝的（不是 LocalhostGuard 的 localhost 判据）：{body}"
    );
}

/// 白名单包含回环地址、却带着外部 `x-forwarded-for` ⇒ 仍然 403。
///
/// 这是"反向代理后面"的真实形态：直连看起来是 127.0.0.1，但真实客户端在头里。
/// 把 `allow_external_access` 打开让 guard 放行，才测得到 handler 的转发头判据。
#[tokio::test]
async fn nonce_is_refused_when_the_forwarded_client_is_outside_the_ip_whitelist() {
    let Some((app, _state)) = super::setup_fresh_test_app_with_config(|container| {
        let config = config_mut(container);
        config.admin_registration.enabled = true;
        config.admin_registration.ip_whitelist = vec!["127.0.0.1".to_string()];
        // guard 完全放行，于是只剩 handler 的判据在把关。
        config.admin_registration.allow_external_access = true;
    })
    .await
    else {
        return;
    };

    let (status, body) = send(app.clone(), get_nonce_request(Some("8.8.8.8"))).await;

    assert_eq!(status, StatusCode::FORBIDDEN, "{body}");
    assert_eq!(body["error"], "Admin registration source IP is not allowed");

    // 反向对照：同一个 app、同一个白名单，转发头也在白名单内 ⇒ 放行。
    // 没有这一半，上一条断言在"白名单整个坏掉"时也会绿。
    let (status, body) = send(app, get_nonce_request(Some("127.0.0.1"))).await;
    assert_eq!(status, StatusCode::OK, "白名单内的转发地址必须放行：{body}");
    assert!(body["nonce"].is_string(), "放行时应当给出 nonce：{body}");
}

/// `require_captcha = true` 但请求里没带 `captcha_id` ⇒ 400（不是 500，也不是放行）。
#[tokio::test]
async fn register_demands_a_captcha_id_when_captcha_is_required() {
    let Some((app, _state)) = super::setup_fresh_test_app_with_config(|container| {
        let config = config_mut(container);
        config.admin_registration.enabled = true;
        config.admin_registration.require_captcha = true;
    })
    .await
    else {
        return;
    };

    let (status, body) = send(app, post_register(register_body(&json!({})))).await;

    assert_eq!(status, StatusCode::BAD_REQUEST, "{body}");
    assert_eq!(body["errcode"], "M_INVALID_PARAM");
    assert_eq!(body["error"], "captcha_id is required");
}

/// 给了 `captcha_id` 却没给 `captcha_code` ⇒ 400，且错误必须点名缺的是哪一个。
#[tokio::test]
async fn register_demands_a_captcha_code_when_the_id_is_present() {
    let Some((app, _state)) = super::setup_fresh_test_app_with_config(|container| {
        let config = config_mut(container);
        config.admin_registration.enabled = true;
        config.admin_registration.require_captcha = true;
    })
    .await
    else {
        return;
    };

    let (status, body) = send(app, post_register(register_body(&json!({ "captcha_id": "probe-captcha" })))).await;

    assert_eq!(status, StatusCode::BAD_REQUEST, "{body}");
    assert_eq!(body["error"], "captcha_code is required");
}

/// `require_manual_approval = true`：缺 token ⇒ 400；错 token ⇒ 403。
///
/// 两种失败必须**可区分**：缺参数是调用方写错了，令牌不对是鉴权失败 —— 合成同一个
/// 状态码会让运维无法判断该改客户端还是改配置。
#[tokio::test]
async fn register_requires_an_approval_token_that_actually_matches() {
    let Some((app, _state)) = super::setup_fresh_test_app_with_config(|container| {
        let config = config_mut(container);
        config.admin_registration.enabled = true;
        config.admin_registration.require_manual_approval = true;
        config.admin_registration.approval_tokens = vec!["expected-approval-token".to_string()];
    })
    .await
    else {
        return;
    };

    let (status, body) = send(app.clone(), post_register(register_body(&json!({})))).await;
    assert_eq!(status, StatusCode::BAD_REQUEST, "缺参数应当是 400：{body}");
    assert_eq!(body["error"], "approval_token is required");

    let (status, body) =
        send(app.clone(), post_register(register_body(&json!({ "approval_token": "wrong-approval-token" })))).await;
    assert_eq!(status, StatusCode::FORBIDDEN, "令牌不对应当是 403：{body}");
    assert_eq!(body["error"], "Manual approval token is invalid");

    // 前缀也不能过 —— `secure_compare` 要求长度相等，这里把它钉在 HTTP 层。
    let (status, body) =
        send(app, post_register(register_body(&json!({ "approval_token": "expected-approval-toke" })))).await;
    assert_eq!(status, StatusCode::FORBIDDEN, "前缀不得通过：{body}");
}

/// 字段校验失败必须是 400 + `M_INVALID_PARAM`，而不是掉进服务层变成 500。
#[tokio::test]
async fn register_rejects_a_too_short_password_before_touching_the_service() {
    let Some((app, _state)) = super::setup_fresh_test_app_with_config(|container| {
        config_mut(container).admin_registration.enabled = true;
    })
    .await
    else {
        return;
    };

    let (status, body) = send(app, post_register(register_body(&json!({ "password": "short" })))).await;

    assert_eq!(status, StatusCode::BAD_REQUEST, "{body}");
    assert_eq!(body["errcode"], "M_INVALID_PARAM");
    let message = body["error"].as_str().unwrap_or_default();
    assert!(message.starts_with("Validation error:"), "错误必须说明是校验失败：{body}");
}

/// 无效 nonce / 错误 HMAC 走 `map_admin_register_service_error`，必须是 400 而不是 500。
///
/// 这个映射函数此前**一行都没被执行过**：默认配置下要打到它，得先拿到合法 nonce、
/// 再用一个错的 mac 提交。500 会让人以为服务端崩了，实际是客户端算错了签名。
#[tokio::test]
async fn a_wrong_hmac_is_a_client_error_not_a_server_error() {
    let Some((app, _state)) = super::setup_fresh_test_app_with_config(|container| {
        config_mut(container).admin_registration.enabled = true;
    })
    .await
    else {
        return;
    };

    // 先拿一个真 nonce，确保失败原因是 HMAC 而不是 nonce。
    let (status, nonce_body) = send(app.clone(), get_nonce_request(None)).await;
    assert_eq!(status, StatusCode::OK, "前置条件：nonce 必须能取到：{nonce_body}");
    let nonce = nonce_body["nonce"].as_str().expect("nonce must be a string").to_string();

    let (status, body) = send(
        app,
        post_register(register_body(&json!({
            "nonce": nonce,
            "username": format!("admin_probe_{}", rand::random::<u32>()),
            "password": "AdminTest@123",
            "admin": true,
            // 长度合法的十六进制，但和 nonce/用户名/密码算出来的 HMAC 对不上。
            "mac": "00112233445566778899aabbccddeeff00112233445566778899aabbccddeeff",
        }))),
    )
    .await;

    assert_eq!(status, StatusCode::BAD_REQUEST, "签名错误是客户端错误：{body}");
    assert_eq!(body["errcode"], "M_UNKNOWN");
    assert!(
        body["error"].as_str().unwrap_or_default().contains("HMAC"),
        "错误必须点名是 HMAC 不对，否则调用方只能猜：{body}"
    );
}
