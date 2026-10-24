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
//! 先取模板再生成码）。默认测试库没有模板 ⇒ **完整状态机（发码→查状态→验码）在本轮测不了**，
//! 需要先种一行模板数据（属独立一笔，得先摸清模板表结构与默认值）。
//! 因此本文件覆盖的是**能在真实 router 上验证的部分**：路由接通、`send` 的前置条件与错误文案、
//! 非法类型 400、未知 id 404、以及清理端点的 RBAC。

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
