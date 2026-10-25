#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

//! NEW-09 第六波 1/1 —— `captcha` 端点族的**真 HTTP 往返**。
//!
//! `tests/unit/captcha_shape_only_tests.rs` 只断言自己构造的字面量；`captcha_tests_migrated` 是
//! service 级（0 个 `oneshot`）。本文件打真 router：
//!
//! * `POST /_matrix/client/v3/register/captcha/send` —— `{captcha_type, target}` ⇒ `{captcha_id, expires_in, captcha_type}`
//! * `GET  /_matrix/client/v3/register/captcha/status?captcha_id=` —— `{captcha_id, status, attempt_count, max_attempts, …}`
//! * `POST /_matrix/client/v3/register/captcha/verify` —— `{captcha_id, code}` ⇒ `{verified}`
//! * `POST /_synapse/admin/v1/captcha/cleanup` —— `{cleaned_count, message}`
//!
//! 这一族挂在**注册流程**上，所以 `/_matrix/client/v3/register/captcha/*` 四个端点在源码里是
//! `public_routes`（注册前拿不到 token，匿名可用是设计要求）；只有 `/_synapse/admin/v1/captcha/cleanup`
//! 走 `AdminUser`。
//!
//! ⚠️ **`send` 需要预置 captcha 模板**：实测即便 `captcha_type: "image"`，未配模板时服务端返回
//! `400 M_BAD_JSON`「No captcha template configured for this type」（`captcha_service::send_captcha`
//! 先取模板再生成码）。
//!
//! **本轮已补齐完整状态机**（原 §0.1 记的"唯一遗留"）：`captcha_full_state_machine` 与
//! `captcha_exhausts_after_max_attempts` 先往 `captcha_template` 种一条 **`image` 类型的默认模板**
//! —— `image` 的投递实现是 `Ok(())`（`captcha_service::send_captcha_via_provider`），
//! 因此不需要 SMTP / SMS 就能走完整条链路；验证码本身不回给客户端（只落
//! `registration_captcha.code`），测试直接从库里读，顺带证明码**没有**泄漏进任何响应体。
//!
//! 其余用例覆盖的是路由接通、`send` 的前置条件与错误文案、非法类型 400、未知 id 404、清理端点的 RBAC。

use axum::{
    body::Body,
    http::{Request, StatusCode},
};
use serde_json::{json, Value};
use tower::ServiceExt;

fn get(uri: &str, token: Option<&str>) -> Request<Body> {
    let mut builder = Request::builder().method("GET").uri(uri);
    if let Some(token) = token {
        builder = builder.header("Authorization", format!("Bearer {token}"));
    }
    builder.body(Body::empty()).unwrap()
}

fn post_json(uri: &str, token: Option<&str>, body: &Value) -> Request<Body> {
    let mut builder = Request::builder().method("POST").uri(uri).header("Content-Type", "application/json");
    if let Some(token) = token {
        builder = builder.header("Authorization", format!("Bearer {token}"));
    }
    builder.body(Body::from(body.to_string())).unwrap()
}

async fn call(app: &axum::Router, request: Request<Body>) -> (StatusCode, Value) {
    let response = app.clone().oneshot(super::with_local_connect_info(request)).await.unwrap();
    let status = response.status();
    let bytes = axum::body::to_bytes(response.into_body(), 64 * 1024).await.unwrap();
    let value = serde_json::from_slice(&bytes).unwrap_or(Value::Null);
    (status, value)
}

async fn register(app: &axum::Router, username: &str) -> String {
    let request = Request::builder()
        .method("POST")
        .uri("/_matrix/client/v3/register")
        .header("Content-Type", "application/json")
        .body(Body::from(
            json!({
                "username": username,
                "password": "Password123!",
                "auth": { "type": "m.login.dummy" }
            })
            .to_string(),
        ))
        .unwrap();
    let (status, value) = call(app, request).await;
    assert_eq!(status, StatusCode::OK, "注册必须是 200：{value}");
    value["access_token"].as_str().expect("注册必须返回 access_token").to_string()
}

const SEND: &str = "/_matrix/client/v3/register/captcha/send";
const STATUS: &str = "/_matrix/client/v3/register/captcha/status";
const VERIFY: &str = "/_matrix/client/v3/register/captcha/verify";

/// `send` 的前置条件：未配模板 ⇒ 400 且**文案要点名模板**（不是 500、也不是静默成功）。
#[tokio::test]
async fn captcha_send_without_configured_template_reports_error() {
    let Some(app) = super::setup_fresh_test_app().await else {
        return;
    };

    let (status, sent) =
        call(&app, post_json(SEND, None, &json!({ "captcha_type": "image", "target": "unit-probe" }))).await;
    assert_eq!(status, StatusCode::BAD_REQUEST, "未配模板时发送必须 400：{sent}");
    assert_eq!(sent["errcode"], "M_BAD_JSON", "errcode：{sent}");
    let message = sent["error"].as_str().unwrap_or_default();
    assert!(
        message.contains("template") || message.contains("模板"),
        "错误文案必须点名「模板」这个前置条件（否则运维只能猜）：{sent}"
    );
}

/// 未知 `captcha_id` 验码：**绝不能被当作通过**（200 时 `verified` 必须是 false，否则必须带 errcode）。
#[tokio::test]
async fn verify_unknown_captcha_id_is_never_verified() {
    let Some(app) = super::setup_fresh_test_app().await else {
        return;
    };

    let (status, value) =
        call(&app, post_json(VERIFY, None, &json!({ "captcha_id": "does-not-exist", "code": "123456" }))).await;
    if status == StatusCode::OK {
        assert_eq!(value["verified"], false, "未知 captcha_id 不能验证通过：{value}");
    } else {
        assert!(status.is_client_error(), "未知 captcha_id 必须是客户端错误：{status} {value}");
        let errcode = value["errcode"].as_str().unwrap_or_default();
        assert!(errcode.starts_with("M_"), "必须带 Matrix errcode：{value}");
    }
}

/// 未知 `captcha_id` 查状态 ⇒ 404（不能返回空对象让客户端以为"待验证"）。
#[tokio::test]
async fn captcha_status_for_unknown_id_is_not_found() {
    let Some(app) = super::setup_fresh_test_app().await else {
        return;
    };

    let (status, value) = call(&app, get(&format!("{STATUS}?captcha_id=does-not-exist"), None)).await;
    assert_eq!(status, StatusCode::NOT_FOUND, "未知 captcha_id 必须 404：{value}");
    let errcode = value["errcode"].as_str().unwrap_or_default();
    assert!(errcode.starts_with("M_"), "必须带 Matrix errcode：{value}");
}

/// 非法 `captcha_type` ⇒ 400（不能让未知类型悄悄建出记录）。
#[tokio::test]
async fn captcha_send_with_invalid_type_is_bad_request() {
    let Some(app) = super::setup_fresh_test_app().await else {
        return;
    };

    let (status, value) =
        call(&app, post_json(SEND, None, &json!({ "captcha_type": "carrier-pigeon", "target": "x" }))).await;
    assert_eq!(status, StatusCode::BAD_REQUEST, "非法类型必须 400：{value}");
    let errcode = value["errcode"].as_str().unwrap_or_default();
    assert!(errcode.starts_with("M_"), "必须带 Matrix errcode：{value}");
}

/// 管理员清理端点：普通用户 403，管理员 200 且带 `cleaned_count`。
#[tokio::test]
async fn captcha_cleanup_requires_admin() {
    let Some(app) = super::setup_fresh_test_app().await else {
        return;
    };
    const CLEANUP: &str = "/_synapse/admin/v1/captcha/cleanup";

    let user_token = register(&app, "captcha_plain_user").await;
    let (status, denied) = call(&app, post_json(CLEANUP, Some(&user_token), &json!({}))).await;
    assert_eq!(status, StatusCode::FORBIDDEN, "普通用户调用清理必须 403：{denied}");
    assert_eq!(denied["errcode"], "M_FORBIDDEN", "errcode：{denied}");

    let (admin_token, _) = super::get_admin_token(&app).await;
    let (status, cleaned) = call(&app, post_json(CLEANUP, Some(&admin_token), &json!({}))).await;
    assert_eq!(status, StatusCode::OK, "管理员调用清理必须 200：{cleaned}");
    assert!(
        cleaned["cleaned_count"].as_i64().is_some(),
        "必须返回数值 cleaned_count（运维据此判断是否清到东西）：{cleaned}"
    );
}

/// 种一条 **`image` 类型的默认模板**并返回 shell 之外需要的 pool。
///
/// `image` 是最省事的选择：`send_captcha_via_provider` 对它的投递就是 `Ok(())`，
/// 不需要 SMTP / SMS / 后台任务队列，因此整条状态机能在默认测试配置下跑通。
async fn seed_image_template(pool: &sqlx::PgPool, template_name: &str) {
    sqlx::query(
        r#"
        INSERT INTO captcha_template (template_name, captcha_type, subject, content, is_default, is_enabled, created_ts)
        VALUES ($1, 'image', '验证码', '你的验证码是 {{code}}，{{expiry_minutes}} 分钟内有效', TRUE, TRUE, $2)
        "#,
    )
    .bind(template_name)
    .bind(synapse_common::current_timestamp_millis())
    .execute(pool)
    .await
    .expect("种 captcha 模板失败");
}

/// 从库里读某条 captcha 的验证码 —— 服务端**故意**不把它放进任何响应体。
async fn captcha_code(pool: &sqlx::PgPool, captcha_id: &str) -> String {
    sqlx::query_scalar::<_, String>("SELECT code FROM registration_captcha WHERE captcha_id = $1")
        .bind(captcha_id)
        .fetch_one(pool)
        .await
        .expect("读 captcha code 失败")
}

async fn captcha_status(app: &axum::Router, captcha_id: &str) -> Value {
    let (status, value) = call(app, get(&format!("{STATUS}?captcha_id={captcha_id}"), None)).await;
    assert_eq!(status, StatusCode::OK, "查状态必须 200：{value}");
    value
}

/// NEW-09 遗留收口：**完整状态机** —— 发码 → 查状态 → 错码 → 对码 → 复用。
#[tokio::test]
async fn captcha_full_state_machine() {
    let Some((app, pool, _cache)) = super::setup_fresh_test_app_with_pool().await else {
        return;
    };
    seed_image_template(&pool, "image-default-state-machine").await;

    // ① 发码
    let target = "state-machine@example.com";
    let (status, sent) = call(&app, post_json(SEND, None, &json!({ "captcha_type": "image", "target": target }))).await;
    assert_eq!(status, StatusCode::OK, "配好模板后发码必须 200：{sent}");
    let captcha_id = sent["captcha_id"].as_str().expect("发码必须返回 captcha_id").to_string();
    assert_eq!(sent["captcha_type"], "image", "{sent}");
    assert_eq!(sent["expires_in"], json!(300), "image 类默认 5 分钟有效：{sent}");

    // ② 码绝不能出现在响应里（否则"验证"就是自欺）
    let code = captcha_code(&pool, &captcha_id).await;
    assert_eq!(code.len(), 4, "image 类默认 4 位码：{code}");
    assert!(!sent.to_string().contains(&code), "发码响应不得包含验证码本身：{sent}");

    // ③ 查状态：待验证、0 次尝试、上限 3
    let pending = captcha_status(&app, &captcha_id).await;
    assert_eq!(pending["status"], "pending", "{pending}");
    assert_eq!(pending["target"], target, "{pending}");
    assert_eq!(pending["attempt_count"], json!(0), "{pending}");
    assert_eq!(pending["max_attempts"], json!(3), "image 类默认 3 次尝试：{pending}");
    assert!(!pending.to_string().contains(&code), "状态响应不得包含验证码：{pending}");

    // ④ 错码：verified=false，且**消耗一次尝试**（不能静默不计数）
    let wrong = if code == "0000" { "1111" } else { "0000" };
    let (status, bad) = call(&app, post_json(VERIFY, None, &json!({ "captcha_id": captcha_id, "code": wrong }))).await;
    assert_eq!(status, StatusCode::OK, "验码接口对错码也回 200 + verified=false：{bad}");
    assert_eq!(bad["verified"], false, "错码必须 verified=false：{bad}");
    let after_wrong = captcha_status(&app, &captcha_id).await;
    assert_eq!(after_wrong["attempt_count"], json!(1), "错码必须消耗一次尝试：{after_wrong}");
    assert_eq!(after_wrong["status"], "pending", "还没到上限，状态仍是 pending：{after_wrong}");

    // ⑤ 对码：verified=true，状态转 verified
    let (status, ok) = call(&app, post_json(VERIFY, None, &json!({ "captcha_id": captcha_id, "code": code }))).await;
    assert_eq!(status, StatusCode::OK, "{ok}");
    assert_eq!(ok["verified"], true, "对码必须 verified=true：{ok}");
    let verified = captcha_status(&app, &captcha_id).await;
    assert_eq!(verified["status"], "verified", "验证通过后状态必须转 verified：{verified}");

    // ⑥ 复用同一个码：**单次有效**，不能再通过
    let (status, reused) =
        call(&app, post_json(VERIFY, None, &json!({ "captcha_id": captcha_id, "code": code }))).await;
    assert_eq!(status, StatusCode::OK, "{reused}");
    assert_eq!(reused["verified"], false, "已验证的 captcha 不能复用：{reused}");
}

/// 尝试次数上限：第 4 次（`max_attempts = 3` 用尽后）必须把状态打成 `exhausted`，
/// 而不是让攻击者无限次猜。
#[tokio::test]
async fn captcha_exhausts_after_max_attempts() {
    let Some((app, pool, _cache)) = super::setup_fresh_test_app_with_pool().await else {
        return;
    };
    seed_image_template(&pool, "image-default-exhaustion").await;

    let (status, sent) =
        call(&app, post_json(SEND, None, &json!({ "captcha_type": "image", "target": "exhaust@example.com" }))).await;
    assert_eq!(status, StatusCode::OK, "发码必须 200：{sent}");
    let captcha_id = sent["captcha_id"].as_str().expect("captcha_id").to_string();
    let code = captcha_code(&pool, &captcha_id).await;
    let wrong = if code == "0000" { "1111" } else { "0000" };

    // 前 3 次错码：都回 false，状态仍 pending（attempt_count 依次 1/2/3）
    for expected_attempts in 1..=3 {
        let (status, value) =
            call(&app, post_json(VERIFY, None, &json!({ "captcha_id": captcha_id, "code": wrong }))).await;
        assert_eq!(status, StatusCode::OK, "{value}");
        assert_eq!(value["verified"], false, "错码必须 false：{value}");
        let snapshot = captcha_status(&app, &captcha_id).await;
        assert_eq!(snapshot["attempt_count"], json!(expected_attempts), "尝试次数必须递增：{snapshot}");
    }

    // 第 4 次：上限已用尽 ⇒ false 且状态打 exhausted
    let (status, blocked) =
        call(&app, post_json(VERIFY, None, &json!({ "captcha_id": captcha_id, "code": wrong }))).await;
    assert_eq!(status, StatusCode::OK, "{blocked}");
    assert_eq!(blocked["verified"], false, "用尽后必须拒绝：{blocked}");
    let exhausted = captcha_status(&app, &captcha_id).await;
    assert_eq!(exhausted["status"], "exhausted", "超限后状态必须打 exhausted：{exhausted}");

    // 即使此时给出**正确**的码，也不能通过（状态已不是 pending）
    let (status, late) = call(&app, post_json(VERIFY, None, &json!({ "captcha_id": captcha_id, "code": code }))).await;
    assert_eq!(status, StatusCode::OK, "{late}");
    assert_eq!(late["verified"], false, "已 exhausted 的 captcha 即使码对也不能通过：{late}");
}
