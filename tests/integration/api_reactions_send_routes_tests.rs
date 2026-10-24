#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

//! NEW-09 批次 3/3 —— `reactions` 端点族的**真 HTTP 往返**。
//!
//! `tests/unit/reactions_shape_only_tests.rs` 只断言自己构造的字面量（外加路由台账），
//! 改坏 `add_reaction` 也不会红。本文件走真链路：
//! `PUT /_matrix/client/v3/rooms/{room_id}/send/m.reaction/{txn_id}`，造真房间 + 真消息当目标事件。
//!
//! 锁住四条语义：
//! * **成功返回新事件的 `event_id`**（不是被反应的那个 event_id —— 弄混了客户端会去重错）；
//! * **缺 `m.relates_to` ⇒ 400 `M_BAD_JSON`**；
//! * **`rel_type` 必须是 `m.annotation`**（发别的 rel_type 走这个端点 ⇒ 400）；
//! * **非成员 ⇒ 403 `M_FORBIDDEN`** —— 这条是访问控制：历史上该端点只查 `room_exists`，
//!   任何登录用户都能往别人的私密房间注入 `m.annotation`（跨房写入）。

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
        call(app, post_json("/_matrix/client/v3/createRoom", Some(token), &json!({ "name": "Reaction Room" }))).await;
    assert_eq!(status, StatusCode::OK, "建房必须是 200：{value}");
    value["room_id"].as_str().expect("建房必须返回 room_id").to_string()
}

async fn send_message(app: &axum::Router, token: &str, room_id: &str, txn: &str) -> String {
    let uri = format!("/_matrix/client/v3/rooms/{room_id}/send/m.room.message/{txn}");
    let body = json!({ "msgtype": "m.text", "body": "react to me" });
    let (status, value) = call(app, put_json(&uri, token, &body)).await;
    assert_eq!(status, StatusCode::OK, "发消息必须是 200：{value}");
    value["event_id"].as_str().expect("发消息必须返回 event_id").to_string()
}

fn annotation(target_event_id: &str, key: &str) -> Value {
    json!({
        "m.relates_to": {
            "rel_type": "m.annotation",
            "event_id": target_event_id,
            "key": key
        }
    })
}

/// 主干：加反应成功，且返回的是**新事件**的 event_id。
#[tokio::test]
async fn add_reaction_returns_new_event_id() {
    let Some(app) = super::setup_fresh_test_app().await else {
        return;
    };
    let token = register(&app, "reaction_main").await;
    let room_id = create_room(&app, &token).await;
    let target = send_message(&app, &token, &room_id, "txn-react-target").await;

    let uri = format!("/_matrix/client/v3/rooms/{room_id}/send/m.reaction/txn-react-1");
    let (status, value) = call(&app, put_json(&uri, &token, &annotation(&target, "👍"))).await;
    assert_eq!(status, StatusCode::OK, "加反应必须是 200：{value}");

    let reaction_event_id = value["event_id"].as_str().unwrap_or_else(|| panic!("必须返回 event_id：{value}"));
    assert!(reaction_event_id.starts_with('$'), "event_id 形状不对：{reaction_event_id}");
    assert_ne!(
        reaction_event_id, target,
        "返回的必须是新反应事件的 event_id，不是被反应的那条（弄混会让客户端去重错）"
    );
}

/// 缺 `m.relates_to` ⇒ 400 `M_BAD_JSON`。
#[tokio::test]
async fn add_reaction_without_relates_to_is_bad_json() {
    let Some(app) = super::setup_fresh_test_app().await else {
        return;
    };
    let token = register(&app, "reaction_missing").await;
    let room_id = create_room(&app, &token).await;

    let uri = format!("/_matrix/client/v3/rooms/{room_id}/send/m.reaction/txn-react-2");
    let (status, value) = call(&app, put_json(&uri, &token, &json!({}))).await;
    assert_eq!(status, StatusCode::BAD_REQUEST, "缺 m.relates_to 必须 400：{value}");
    assert_eq!(value["errcode"], "M_BAD_JSON", "errcode：{value}");
}

/// `rel_type` 不是 `m.annotation` ⇒ 400（这个端点只收反应；别的 relation 走 `/send/m.<type>`）。
#[tokio::test]
async fn add_reaction_with_wrong_rel_type_is_bad_json() {
    let Some(app) = super::setup_fresh_test_app().await else {
        return;
    };
    let token = register(&app, "reaction_reltype").await;
    let room_id = create_room(&app, &token).await;
    let target = send_message(&app, &token, &room_id, "txn-react-target-2").await;

    let uri = format!("/_matrix/client/v3/rooms/{room_id}/send/m.reaction/txn-react-3");
    let body = json!({
        "m.relates_to": { "rel_type": "m.thread", "event_id": target, "key": "👍" }
    });
    let (status, value) = call(&app, put_json(&uri, &token, &body)).await;
    assert_eq!(status, StatusCode::BAD_REQUEST, "rel_type 不对必须 400：{value}");
    assert_eq!(value["errcode"], "M_BAD_JSON", "errcode：{value}");
}

/// 非成员 ⇒ 403（不是 404「房间不存在」—— 房间是存在的，拒绝原因是没权限）。
#[tokio::test]
async fn add_reaction_by_non_member_is_forbidden() {
    let Some(app) = super::setup_fresh_test_app().await else {
        return;
    };
    let owner_token = register(&app, "reaction_owner").await;
    let outsider_token = register(&app, "reaction_outsider").await;
    let room_id = create_room(&app, &owner_token).await;
    let target = send_message(&app, &owner_token, &room_id, "txn-react-target-3").await;

    let uri = format!("/_matrix/client/v3/rooms/{room_id}/send/m.reaction/txn-react-4");
    let (status, value) = call(&app, put_json(&uri, &outsider_token, &annotation(&target, "👍"))).await;
    assert_eq!(status, StatusCode::FORBIDDEN, "非成员必须 403：{value}");
    assert_eq!(value["errcode"], "M_FORBIDDEN", "errcode：{value}");
}
