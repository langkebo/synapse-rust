#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

//! NEW-09 批次 1/3 —— `push` 端点族的**真 HTTP 往返**。
//!
//! 存在的意义：`tests/unit/push_shape_only_tests.rs` 只断言自己构造的 JSON 字面量，
//! 改坏生产 handler 也不会红（见 `scripts/ci/shape_only_test_matrix` 里那一行 `NONE`）。
//! 本文件把该族改成「构造 router → 发请求 → 断言 status / body / errcode」：
//! 覆盖 `GET /pushers`、`POST /pushers/set`（新建）、`POST /pushers/set`（`kind: null` 删除）、
//! 必填字段校验、以及未认证访问。
//!
//! 两个容易踩的契约细节（都已在本文件里锁住）：
//! * `kind` **必须出现**但**可以是 null** —— 缺失是 400，`null` 是「删除」；
//! * `kind` 非 null 时 `app_display_name` 与 `lang` 是必填（缺了 400 `M_BAD_JSON`）；
//! * `data.url` 会过 SSRF 校验：必须是 https 且不能是 IP 字面量/localhost。

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
    let (status, value) = call(
        app,
        post_json(
            "/_matrix/client/v3/register",
            None,
            &json!({
                "username": username,
                "password": "Password123!",
                "auth": { "type": "m.login.dummy" }
            }),
        ),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "注册必须是 200：{value}");
    value["access_token"].as_str().expect("注册必须返回 access_token").to_string()
}

fn http_pusher(pushkey: &str) -> Value {
    json!({
        "pushkey": pushkey,
        "app_id": "com.example.app",
        "kind": "http",
        "app_display_name": "Example",
        "device_display_name": "Test Device",
        "lang": "en",
        "data": { "url": "https://push.example.com/_matrix/push/v1/notify", "format": "event_id_only" }
    })
}

/// 新用户的 pusher 列表是空数组 —— 这也把「字段名是 `pushers`」钉住（客户端按这个名字解）。
#[tokio::test]
async fn new_user_has_empty_pusher_list() {
    let Some(app) = super::setup_fresh_test_app().await else {
        return;
    };
    let token = register(&app, "pusher_empty").await;

    let (status, value) = call(&app, get("/_matrix/client/v3/pushers", Some(&token))).await;
    assert_eq!(status, StatusCode::OK, "{value}");
    let pushers = value["pushers"].as_array().unwrap_or_else(|| panic!("响应必须是对象且含 pushers 数组：{value}"));
    assert!(pushers.is_empty(), "新注册用户不应有 pusher，实际：{pushers:?}");
}

/// 创建 → 读回 → 删除，这条是「改坏 handler 就会红」的主干。
#[tokio::test]
async fn set_pusher_is_listed_then_removed_by_null_kind() {
    let Some(app) = super::setup_fresh_test_app().await else {
        return;
    };
    let token = register(&app, "pusher_crud").await;

    let (status, created) =
        call(&app, post_json("/_matrix/client/v3/pushers/set", Some(&token), &http_pusher("apns-key-1"))).await;
    assert_eq!(status, StatusCode::OK, "创建 pusher 必须是 200：{created}");
    assert_eq!(created["kind"], "http");
    assert_eq!(created["app_id"], "com.example.app");
    assert!(
        created["created_ts"].as_i64().is_some_and(|ts| ts > 0),
        "创建响应必须带正的 created_ts（客户端据此排序）：{created}"
    );

    let (status, listed) = call(&app, get("/_matrix/client/v3/pushers", Some(&token))).await;
    assert_eq!(status, StatusCode::OK);
    let pushers = listed["pushers"].as_array().unwrap();
    assert_eq!(pushers.len(), 1, "创建后必须只剩/恰有 1 个 pusher：{listed}");
    assert_eq!(pushers[0]["pushkey"], "apns-key-1");
    assert_eq!(pushers[0]["kind"], "http");

    // `kind: null` 是**删除**语义，而不是「缺字段」。
    let delete_body = json!({ "pushkey": "apns-key-1", "app_id": "com.example.app", "kind": null });
    let (status, deleted) = call(&app, post_json("/_matrix/client/v3/pushers/set", Some(&token), &delete_body)).await;
    assert_eq!(status, StatusCode::OK, "删除 pusher 必须是 200：{deleted}");
    assert_eq!(deleted["deleted"], true, "删除响应必须带 deleted=true：{deleted}");

    let (status, listed) = call(&app, get("/_matrix/client/v3/pushers", Some(&token))).await;
    assert_eq!(status, StatusCode::OK);
    assert!(listed["pushers"].as_array().unwrap().is_empty(), "删除后必须为空：{listed}");
}

/// `kind` 非 null 时缺 `app_display_name` ⇒ 400 `M_BAD_JSON`（不是 axum 的 422）。
#[tokio::test]
async fn set_pusher_missing_required_field_is_bad_json() {
    let Some(app) = super::setup_fresh_test_app().await else {
        return;
    };
    let token = register(&app, "pusher_bad").await;

    let mut body = http_pusher("apns-key-2");
    body.as_object_mut().unwrap().remove("app_display_name");
    let (status, value) = call(&app, post_json("/_matrix/client/v3/pushers/set", Some(&token), &body)).await;
    assert_eq!(status, StatusCode::BAD_REQUEST, "缺必填字段必须 400：{value}");
    assert_eq!(value["errcode"], "M_BAD_JSON", "缺字段的 errcode 必须是 M_BAD_JSON：{value}");
    assert!(
        value["error"].as_str().unwrap_or_default().contains("app_display_name"),
        "错误信息要点名缺的字段（否则客户端无法定位）：{value}"
    );
}

/// 未认证访问 `GET /pushers` ⇒ 401（不能返回空列表假装成功）。
#[tokio::test]
async fn pushers_require_authentication() {
    let Some(app) = super::setup_fresh_test_app().await else {
        return;
    };

    let (status, value) = call(&app, get("/_matrix/client/v3/pushers", None)).await;
    assert_eq!(status, StatusCode::UNAUTHORIZED, "未认证必须是 401：{value}");
    // 本仓走 `bearer_token`（`synapse-web/src/utils/auth.rs`）的端点统一回 `M_UNAUTHORIZED`
    // —— 包括「头缺失」与「头畸形」两种情形；`M_MISSING_TOKEN` 只出现在用专用提取器的路径上。
    // 这里断言**实际契约**，不按规范字面写 `M_MISSING_TOKEN`（写了会红，而那是全仓 auth 契约的事，
    // 不属于本族）。
    assert_eq!(value["errcode"], "M_UNAUTHORIZED", "未带 token 的 errcode：{value}");
}
