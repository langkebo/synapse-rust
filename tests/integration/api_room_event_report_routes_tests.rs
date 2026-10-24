#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

//! NEW-09 批次 2/3 —— `event_report` 端点族的**真 HTTP 往返**。
//!
//! `tests/unit/event_report_shape_only_tests.rs` 只断言自己构造的 JSON 字面量，
//! 改坏 `report_event` 也不会红。本文件走真链路：
//! `POST /_matrix/client/v3/rooms/{room_id}/report/{event_id}`，造真房间 + 真消息取 event_id。
//!
//! 锁住三条语义（都是改坏 handler 就会红的）：
//! * **成功返回 `report_id`** —— 客户端拿它去轮询处理状态；
//! * **`reason` 可选** —— 不带 reason 也必须 200（规范里 reason 是 optional）；
//! * **事件必须属于该房间** —— 房间对了但 event_id 不存在 ⇒ 404 `M_NOT_FOUND`；
//! * **必须先是房间成员** —— 非成员举报他人房间的事件 ⇒ 403 `M_FORBIDDEN`
//!   （这条是访问控制，不是参数校验）。

use axum::{
    body::Body,
    http::{Request, StatusCode},
};
use serde_json::{json, Value};
use tower::ServiceExt;

fn bearer(builder: axum::http::request::Builder, token: Option<&str>) -> axum::http::request::Builder {
    match token {
        Some(token) => builder.header("Authorization", format!("Bearer {token}")),
        None => builder,
    }
}

fn post_json(uri: &str, token: Option<&str>, body: &Value) -> Request<Body> {
    let builder = Request::builder().method("POST").uri(uri).header("Content-Type", "application/json");
    bearer(builder, token).body(Body::from(body.to_string())).unwrap()
}

fn put_json(uri: &str, token: &str, body: &Value) -> Request<Body> {
    let builder = Request::builder().method("PUT").uri(uri).header("Content-Type", "application/json");
    bearer(builder, Some(token)).body(Body::from(body.to_string())).unwrap()
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

async fn create_room(app: &axum::Router, token: &str) -> String {
    let (status, value) =
        call(app, post_json("/_matrix/client/v3/createRoom", Some(token), &json!({ "name": "Report Room" }))).await;
    assert_eq!(status, StatusCode::OK, "建房必须是 200：{value}");
    value["room_id"].as_str().expect("建房必须返回 room_id").to_string()
}

async fn send_message(app: &axum::Router, token: &str, room_id: &str, txn: &str) -> String {
    let uri = format!("/_matrix/client/v3/rooms/{room_id}/send/m.room.message/{txn}");
    let body = json!({ "msgtype": "m.text", "body": "report me" });
    let (status, value) = call(app, put_json(&uri, token, &body)).await;
    assert_eq!(status, StatusCode::OK, "发消息必须是 200：{value}");
    value["event_id"].as_str().expect("发消息必须返回 event_id").to_string()
}

/// 主干：举报成功，返回可用的 `report_id`。
#[tokio::test]
async fn report_event_returns_report_id() {
    let Some(app) = super::setup_fresh_test_app().await else {
        return;
    };
    let token = register(&app, "report_main").await;
    let room_id = create_room(&app, &token).await;
    let event_id = send_message(&app, &token, &room_id, "txn-report-1").await;

    let uri = format!("/_matrix/client/v3/rooms/{room_id}/report/{event_id}");
    let (status, value) = call(&app, post_json(&uri, Some(&token), &json!({ "reason": "spam" }))).await;
    assert_eq!(status, StatusCode::OK, "举报必须是 200：{value}");
    assert!(value["report_id"].as_i64().is_some(), "响应必须带数值 report_id（客户端据此轮询处理状态）：{value}");
}

/// `reason` 是可选的 —— 不带也必须成功（规范里是 optional，缺了不该 400）。
#[tokio::test]
async fn report_event_without_reason_is_accepted() {
    let Some(app) = super::setup_fresh_test_app().await else {
        return;
    };
    let token = register(&app, "report_no_reason").await;
    let room_id = create_room(&app, &token).await;
    let event_id = send_message(&app, &token, &room_id, "txn-report-2").await;

    let uri = format!("/_matrix/client/v3/rooms/{room_id}/report/{event_id}");
    let (status, value) = call(&app, post_json(&uri, Some(&token), &json!({}))).await;
    assert_eq!(status, StatusCode::OK, "不带 reason 也必须 200：{value}");
    assert!(value["report_id"].as_i64().is_some(), "仍然要返回 report_id：{value}");
}

/// 房间对但 event_id 不存在 ⇒ 404 `M_NOT_FOUND`（不能 200 假装记下了）。
#[tokio::test]
async fn report_event_for_unknown_event_is_not_found() {
    let Some(app) = super::setup_fresh_test_app().await else {
        return;
    };
    let token = register(&app, "report_unknown").await;
    let room_id = create_room(&app, &token).await;

    let uri = format!("/_matrix/client/v3/rooms/{room_id}/report/$notarealevent:localhost");
    let (status, value) = call(&app, post_json(&uri, Some(&token), &json!({ "reason": "spam" }))).await;
    assert_eq!(status, StatusCode::NOT_FOUND, "不存在的事件必须 404：{value}");
    assert_eq!(value["errcode"], "M_NOT_FOUND", "errcode：{value}");
}

/// 非成员举报他人房间的事件 ⇒ 403 `M_FORBIDDEN`（这是访问控制，不是参数问题）。
#[tokio::test]
async fn report_event_by_non_member_is_forbidden() {
    let Some(app) = super::setup_fresh_test_app().await else {
        return;
    };
    let owner_token = register(&app, "report_owner").await;
    let outsider_token = register(&app, "report_outsider").await;
    let room_id = create_room(&app, &owner_token).await;
    let event_id = send_message(&app, &owner_token, &room_id, "txn-report-3").await;

    let uri = format!("/_matrix/client/v3/rooms/{room_id}/report/{event_id}");
    let (status, value) = call(&app, post_json(&uri, Some(&outsider_token), &json!({ "reason": "spam" }))).await;
    assert_eq!(status, StatusCode::FORBIDDEN, "非成员必须 403：{value}");
    assert_eq!(value["errcode"], "M_FORBIDDEN", "errcode：{value}");
}
