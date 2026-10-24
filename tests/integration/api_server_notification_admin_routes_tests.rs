#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

//! NEW-09 第二波 3/3 —— `server_notification` 端点族的**真 HTTP 往返**。
//!
//! `tests/unit/server_notification_shape_only_tests.rs` 只断言自己构造的字面量，改坏
//! `routes/admin/notification.rs` 或 `server_notification_service` 都不会红。本文件打真 router
//! （`/_synapse/admin/v1/notifications`），覆盖管理员侧完整 CRUD + 两条边界：
//!
//! * 创建 → 详情 → 列表 → 删除 → 再取 404（一条链把「写进去了、查得到、删掉了」全钉住）；
//! * 列表形状稳定：`{notifications: [...], next_batch: ...}`（客户端按这两个键分页）；
//! * 未知 id ⇒ 404 `M_NOT_FOUND`（删除不存在的 id 也必须 404，不能假装成功）；
//! * 非管理员 ⇒ 403 `M_FORBIDDEN`。
//!
//! 这一族挂在 `AdminUser` 提取器下、且受 `server-notifications` feature 门控 ——
//! 本文件的 `#[tokio::test]` 在未启用该 feature 时不会编译进目标（与路由一致）。

use axum::{
    body::Body,
    http::{Request, StatusCode},
};
use serde_json::{json, Value};
use tower::ServiceExt;

fn get(uri: &str, token: &str) -> Request<Body> {
    Request::builder()
        .method("GET")
        .uri(uri)
        .header("Authorization", format!("Bearer {token}"))
        .body(Body::empty())
        .unwrap()
}

fn delete(uri: &str, token: &str) -> Request<Body> {
    Request::builder()
        .method("DELETE")
        .uri(uri)
        .header("Authorization", format!("Bearer {token}"))
        .body(Body::empty())
        .unwrap()
}

fn post_json(uri: &str, token: &str, body: &Value) -> Request<Body> {
    Request::builder()
        .method("POST")
        .uri(uri)
        .header("Authorization", format!("Bearer {token}"))
        .header("Content-Type", "application/json")
        .body(Body::from(body.to_string()))
        .unwrap()
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

/// 主干：创建 → 详情 → 列表 → 删除 → 再取 404。
#[tokio::test]
async fn admin_notification_crud_round_trip() {
    let Some(app) = super::setup_fresh_test_app().await else {
        return;
    };
    let (admin_token, _) = super::get_admin_token(&app).await;

    let created = json!({
        "title": "Scheduled maintenance",
        "content": "The server will restart at 03:00 UTC.",
        "notification_type": "maintenance",
        "priority": 5,
        "target_audience": "all"
    });
    let (status, value) = call(&app, post_json("/_synapse/admin/v1/notifications", &admin_token, &created)).await;
    assert_eq!(status, StatusCode::OK, "创建通知必须是 200：{value}");
    let id = value["id"].as_i64().unwrap_or_else(|| panic!("创建响应必须带数值 id：{value}"));

    // 详情：标题/内容必须原样回来（不是被截断或默认值覆盖）
    let uri = format!("/_synapse/admin/v1/notifications/{id}");
    let (status, detail) = call(&app, get(&uri, &admin_token)).await;
    assert_eq!(status, StatusCode::OK, "详情必须 200：{detail}");
    assert_eq!(detail["title"], "Scheduled maintenance", "标题必须原样返回：{detail}");
    assert_eq!(detail["content"], "The server will restart at 03:00 UTC.", "内容必须原样返回：{detail}");

    // 列表：必须包含刚创建的那条
    let (status, list) = call(&app, get("/_synapse/admin/v1/notifications", &admin_token)).await;
    assert_eq!(status, StatusCode::OK, "列表必须 200：{list}");
    let notifications =
        list["notifications"].as_array().unwrap_or_else(|| panic!("列表必须是 {{notifications: [...]}}：{list}"));
    assert!(notifications.iter().any(|n| n["id"] == id), "列表里必须能找到刚创建的 id={id}：{list}");

    // 删除：成功返回 200，再取变 404
    let (status, deleted) = call(&app, delete(&uri, &admin_token)).await;
    assert_eq!(status, StatusCode::OK, "删除必须 200：{deleted}");
    let (status, after) = call(&app, get(&uri, &admin_token)).await;
    assert_eq!(status, StatusCode::NOT_FOUND, "删除后详情必须 404：{after}");
    assert_eq!(after["errcode"], "M_NOT_FOUND", "errcode：{after}");
}

/// 列表形状必须稳定：`notifications` 是数组、`next_batch` 键存在（可为 null）。
#[tokio::test]
async fn admin_notification_list_shape_is_stable() {
    let Some(app) = super::setup_fresh_test_app().await else {
        return;
    };
    let (admin_token, _) = super::get_admin_token(&app).await;

    let (status, list) = call(&app, get("/_synapse/admin/v1/notifications?limit=10", &admin_token)).await;
    assert_eq!(status, StatusCode::OK, "{list}");
    assert!(list["notifications"].is_array(), "notifications 必须是数组（空态也是 []）：{list}");
    assert!(list.get("next_batch").is_some(), "必须带 next_batch 键（客户端据此翻页）：{list}");
}

/// 删除不存在的 id ⇒ 404（不能返回 200 假装删掉了）。
#[tokio::test]
async fn delete_unknown_notification_is_not_found() {
    let Some(app) = super::setup_fresh_test_app().await else {
        return;
    };
    let (admin_token, _) = super::get_admin_token(&app).await;

    let (status, value) =
        call(&app, delete("/_synapse/admin/v1/notifications/9223372036854775000", &admin_token)).await;
    assert_eq!(status, StatusCode::NOT_FOUND, "删除不存在的通知必须 404：{value}");
    assert_eq!(value["errcode"], "M_NOT_FOUND", "errcode：{value}");
}

/// 非管理员 ⇒ 403（这一族整体要求管理员，普通用户连列表都不该看到）。
#[tokio::test]
async fn admin_notification_requires_admin() {
    let Some(app) = super::setup_fresh_test_app().await else {
        return;
    };
    let token = register(&app, "notification_plain_user").await;

    let (status, value) = call(&app, get("/_synapse/admin/v1/notifications", &token)).await;
    assert_eq!(status, StatusCode::FORBIDDEN, "普通用户必须 403：{value}");
    assert_eq!(value["errcode"], "M_FORBIDDEN", "errcode：{value}");
}
