#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

//! NEW-09 第三波 2/3 —— `thread` 端点族的**真 HTTP 往返**。
//!
//! `tests/unit/thread_shape_only_tests.rs` 只断言自己构造的字面量，改坏
//! `handlers/room/management/metadata.rs::get_room_thread_by_id` 也不会红。本文件造真线程：
//!
//! 1. 发根消息（`/send`）；
//! 2. **登记线程根**（`POST /_matrix/vendor/v1/threads {room_id, root_event_id}`）——
//!    本仓的 `thread_roots` 只由这条 vendor 写入路径维护；
//! 3. 加一条线程回复（`POST /_matrix/client/v1/rooms/{r}/threads/{t}/replies`）；
//! 4. 读线程详情（`GET /_matrix/client/v3/rooms/{r}/threads/{t}`）。
//!
//! ⚠️ 实测发现（已登记 NEW-13）：**标准 `/send` 带 `m.relates_to.rel_type=m.thread` 不会
//! 登记线程根** —— 事件写进去了，但 `GET /threads/{root}` 回 404 `Thread not found`。
//! 所以本文件的主干用例走 vendor 登记 + client/v1 回复这条**受支持**的写入路径；
//! 不要把它改成「只发 `/send` 然后期待能读到」，那是另一个缺陷（NEW-13），不是本文件的契约。
//!
//! 另加三条边界：非成员 403、房间不存在 404、`thread_id` 不是 `$…` 形状 400 `M_INVALID_PARAM`。

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

fn get(uri: &str, token: Option<&str>) -> Request<Body> {
    bearer(Request::builder().method("GET").uri(uri), token).body(Body::empty()).unwrap()
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
        call(app, post_json("/_matrix/client/v3/createRoom", Some(token), &json!({ "name": "Thread Room" }))).await;
    assert_eq!(status, StatusCode::OK, "建房必须是 200：{value}");
    value["room_id"].as_str().expect("建房必须返回 room_id").to_string()
}

async fn send(app: &axum::Router, token: &str, room_id: &str, txn: &str, body: &Value) -> String {
    let uri = format!("/_matrix/client/v3/rooms/{room_id}/send/m.room.message/{txn}");
    let (status, value) = call(app, put_json(&uri, token, body)).await;
    assert_eq!(status, StatusCode::OK, "发消息必须是 200：{value}");
    value["event_id"].as_str().expect("发消息必须返回 event_id").to_string()
}

/// 主干：登记线程根 → 加回复 → 读详情，`thread_id`/`root`/`replies`/`reply_count` 自洽。
#[tokio::test]
async fn thread_read_returns_root_and_replies() {
    let Some(app) = super::setup_fresh_test_app().await else {
        return;
    };
    let token = register(&app, "thread_owner").await;
    let room_id = create_room(&app, &token).await;

    let root = send(&app, &token, &room_id, "txn-thread-root", &json!({ "msgtype": "m.text", "body": "root" })).await;

    // ① 登记线程根（本仓唯一的生产写入路径）
    let (status, registered) = call(
        &app,
        post_json("/_matrix/vendor/v1/threads", Some(&token), &json!({ "room_id": room_id, "root_event_id": root })),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "登记线程根必须 200：{registered}");
    assert_eq!(registered["root_event_id"], root.as_str(), "{registered}");
    assert_eq!(registered["reply_count"], 0, "刚登记的线程回复数必须是 0：{registered}");

    // ⚠️ 本仓的 `thread_id` 是服务端生成的 `$<uuid>`，**不是**根事件 id。
    // MSC3440 里两者相同（客户端用根事件 id 当 thread_id）；这里必须用响应里的值，
    // 否则后续读/写都 404 —— 见审计 NEW-13（标准客户端会踩这个坎）。
    let thread_id = registered["thread_id"]
        .as_str()
        .unwrap_or_else(|| panic!("登记线程必须返回 thread_id（本仓是 `$<uuid>`，不是根事件 id）：{registered}"))
        .to_string();
    assert!(thread_id.starts_with('$'), "thread_id 形状：{thread_id}");
    assert_ne!(thread_id, root, "记录现状：本仓 thread_id ≠ 根事件 id（MSC3440 偏差，见 NEW-13）");

    // ② 加一条线程回复（稳定端点 `/_matrix/client/v1`）
    let replies_uri = format!("/_matrix/client/v1/rooms/{room_id}/threads/{thread_id}/replies");
    let (status, reply) = call(
        &app,
        post_json(&replies_uri, Some(&token), &json!({ "content": { "msgtype": "m.text", "body": "reply" } })),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "加线程回复必须 200：{reply}");
    let reply_event_id = reply["event_id"].as_str().unwrap_or_else(|| panic!("回复必须返回 event_id：{reply}"));
    assert_eq!(reply["thread_id"], thread_id.as_str(), "回复必须挂在该线程上：{reply}");

    // ③ 读线程详情
    let uri = format!("/_matrix/client/v3/rooms/{room_id}/threads/{thread_id}");
    let (status, value) = call(&app, get(&uri, Some(&token))).await;
    assert_eq!(status, StatusCode::OK, "读线程详情必须 200：{value}");

    assert_eq!(value["thread_id"], thread_id.as_str(), "thread_id 必须回同一个值：{value}");
    assert_eq!(value["room_id"], room_id.as_str(), "{value}");
    // `root` 是 `thread_roots` 的**影子行**（不是事件 JSON）：字段名是 `root_event_id`。
    assert_eq!(value["root"]["root_event_id"], root.as_str(), "root 影子行必须指向根事件：{value}");
    assert_eq!(value["root"]["thread_id"], thread_id.as_str(), "根影子行的 thread_id 必须一致：{value}");

    let replies = value["replies"].as_array().unwrap_or_else(|| panic!("replies 必须是数组：{value}"));
    assert!(
        replies.iter().any(|event| event["event_id"] == reply_event_id),
        "刚加的回复必须出现在 replies 里（reply={reply_event_id}）：{value}"
    );
    assert_eq!(
        value["reply_count"],
        json!(replies.len()),
        "reply_count 必须等于 replies 的实际条数（两者不一致会让客户端角标错）：{value}"
    );
}

/// 登记线程根时 `room_id` 必填 —— 缺了必须 400（不能靠 root_event_id 反查房间）。
#[tokio::test]
async fn register_thread_without_room_id_is_rejected() {
    let Some(app) = super::setup_fresh_test_app().await else {
        return;
    };
    let token = register(&app, "thread_no_room").await;
    let room_id = create_room(&app, &token).await;
    let root = send(&app, &token, &room_id, "txn-thread-root-3", &json!({ "msgtype": "m.text", "body": "root" })).await;

    let (status, value) =
        call(&app, post_json("/_matrix/vendor/v1/threads", Some(&token), &json!({ "root_event_id": root }))).await;
    assert_eq!(status, StatusCode::BAD_REQUEST, "缺 room_id 必须 400：{value}");
    assert!(value["error"].as_str().unwrap_or_default().contains("room_id"), "错误信息要点名缺的字段：{value}");
}

/// 非成员 ⇒ 403（线程详情按房间可见性做门禁，不是「知道 event_id 就能读」）。
#[tokio::test]
async fn thread_read_by_non_member_is_forbidden() {
    let Some(app) = super::setup_fresh_test_app().await else {
        return;
    };
    let owner_token = register(&app, "thread_member").await;
    let outsider_token = register(&app, "thread_outsider").await;
    let room_id = create_room(&app, &owner_token).await;
    let root =
        send(&app, &owner_token, &room_id, "txn-thread-root-2", &json!({ "msgtype": "m.text", "body": "root" })).await;

    let uri = format!("/_matrix/client/v3/rooms/{room_id}/threads/{root}");
    let (status, value) = call(&app, get(&uri, Some(&outsider_token))).await;
    assert_eq!(status, StatusCode::FORBIDDEN, "非成员必须 403：{value}");
    assert_eq!(value["errcode"], "M_FORBIDDEN", "errcode：{value}");
}

/// 房间不存在 ⇒ 404（先判房间存在性，再判可见性）。
#[tokio::test]
async fn thread_read_unknown_room_is_not_found() {
    let Some(app) = super::setup_fresh_test_app().await else {
        return;
    };
    let token = register(&app, "thread_missing_room").await;

    let uri = format!("/_matrix/client/v3/rooms/%21nonexistent%3Alocalhost/threads/$someevent%3Alocalhost");
    let (status, value) = call(&app, get(&uri, Some(&token))).await;
    assert_eq!(status, StatusCode::NOT_FOUND, "房间不存在必须 404：{value}");
    assert_eq!(value["errcode"], "M_NOT_FOUND", "errcode：{value}");
}

/// `thread_id` 不是 `$` 开头 ⇒ 400 `M_INVALID_PARAM`（不是 404 —— 参数就不合法）。
#[tokio::test]
async fn thread_read_invalid_thread_id_is_invalid_param() {
    let Some(app) = super::setup_fresh_test_app().await else {
        return;
    };
    let token = register(&app, "thread_bad_id").await;
    let room_id = create_room(&app, &token).await;

    let uri = format!("/_matrix/client/v3/rooms/{room_id}/threads/notaneventid");
    let (status, value) = call(&app, get(&uri, Some(&token))).await;
    assert_eq!(status, StatusCode::BAD_REQUEST, "非法 thread_id 必须 400：{value}");
    assert_eq!(value["errcode"], "M_INVALID_PARAM", "errcode：{value}");
}
