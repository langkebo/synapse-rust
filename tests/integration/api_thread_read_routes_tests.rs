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
//! **NEW-13（已修）**：
//! ① 标准 `/send` 带 `m.relates_to.rel_type=m.thread` 现在会**补登记线程根**
//!    （`send_message` 里 best-effort 调 `ensure_thread_root_for_reply`，幂等）；
//! ② 读侧支持用**根事件 id** 当 `thread_id`（`get_thread` 查不到就按 `root_event_id` 回退），
//!    因为 MSC3440 客户端就是拿根事件 id 查的。
//! 因此主干用例仍走 vendor 登记（覆盖那条写入路径），另加一条 `threaded_send_alone_is_readable`
//! 走**纯 `/send`**，把 ①+② 一起钉住。
//!
//! **NEW-19（已修）**：同一处还补写 `thread_replies` 索引 —— 写它需要 `content` 与
//! `origin_server_ts`，后者从**刚落库的事件**回查（不用本地时钟顶替）。所以纯 `/send`
//! 的线程现在**既能读到根、`replies` 里也有那条回复**；`threaded_send_txn_retry_does_not_double_index`
//! 钉住"重试同一 txn 不会把 `reply_count` 累加两次"。
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

/// NEW-13①+② + NEW-19：**只用标准 `/send`**（不碰 vendor 端点）也要能读到线程 ——
/// 根消息 + 带 `m.thread` 的回复发完，`GET /threads/{根事件 id}` 必须 200，
/// 且 `replies` 里必须真有那条回复（NEW-19 之前这里是空的）。
#[tokio::test]
async fn threaded_send_alone_is_readable() {
    let Some(app) = super::setup_fresh_test_app().await else {
        return;
    };
    let token = register(&app, "thread_send_only_user").await;
    let room_id = create_room(&app, &token).await;

    let root = send(&app, &token, &room_id, "txn-send-root", &json!({ "msgtype": "m.text", "body": "root" })).await;

    // 关键：不调 vendor 登记，只发一条带 m.thread 的回复
    let reply_event = send(
        &app,
        &token,
        &room_id,
        "txn-send-reply",
        &json!({
            "msgtype": "m.text",
            "body": "reply in thread",
            "m.relates_to": {
                "rel_type": "m.thread",
                "event_id": root,
                "is_falling_back": true,
                "m.in_reply_to": { "event_id": root }
            }
        }),
    )
    .await;

    // ② 读侧用根事件 id 当 thread_id（MSC3440 客户端的行为）
    let uri = format!("/_matrix/client/v3/rooms/{room_id}/threads/{root}");
    let (status, value) = call(&app, get(&uri, Some(&token))).await;
    assert_eq!(status, StatusCode::OK, "只发 /send 之后线程必须能按根事件 id 读到（NEW-13）：{value}");
    assert_eq!(value["root"]["root_event_id"], root.as_str(), "root 必须指向那条根事件：{value}");

    assert!(reply_event.starts_with('$'), "发消息必须返回事件 id：{reply_event}");

    // NEW-19：回复本体必须进 `thread_replies` 索引（此前恒为空）
    let replies = value["replies"].as_array().unwrap_or_else(|| panic!("replies 必须是数组：{value}"));
    let indexed = replies
        .iter()
        .find(|event| event["event_id"] == reply_event.as_str())
        .unwrap_or_else(|| panic!("纯 /send 的回复必须出现在 replies 里（reply={reply_event}）：{value}"));
    assert_eq!(value["reply_count"], json!(1), "reply_count 必须与索引一致：{value}");
    assert_eq!(
        indexed["root_event_id"],
        root.as_str(),
        "索引行的 root_event_id 必须是根事件（写错会让回复挂到别的线程）：{indexed}"
    );
    assert_eq!(
        indexed["in_reply_to_event_id"],
        root.as_str(),
        "`m.relates_to.m.in_reply_to.event_id` 必须抄进索引：{indexed}"
    );
    assert_eq!(indexed["content"]["body"], "reply in thread", "索引行必须抄落库后的 content（不是空壳）：{indexed}");

    // 时间戳必须是**事件本体**上的那个值，不能是本地时钟：线程内排序/翻页都按它算，
    // 抄错会让线程顺序与房间时间线不一致。拿单事件端点对照。
    let event_uri = format!("/_matrix/client/v3/rooms/{room_id}/event/{reply_event}");
    let (status, event) = call(&app, get(&event_uri, Some(&token))).await;
    assert_eq!(status, StatusCode::OK, "读单事件必须 200：{event}");
    assert_eq!(
        indexed["origin_server_ts"], event["origin_server_ts"],
        "索引行的 origin_server_ts 必须等于事件本体的时间戳（indexed={indexed} / event={event}）"
    );
}

/// NEW-19 幂等：客户端**重试同一 txn** 时 `/send` 返回同一个 `event_id`，
/// 索引补写会再走一遍 —— 不能把 `reply_count` 累加两次，也不能让重试变成 5xx。
#[tokio::test]
async fn threaded_send_txn_retry_does_not_double_index() {
    let Some(app) = super::setup_fresh_test_app().await else {
        return;
    };
    let token = register(&app, "thread_retry_user").await;
    let room_id = create_room(&app, &token).await;

    let root = send(&app, &token, &room_id, "txn-retry-root", &json!({ "msgtype": "m.text", "body": "root" })).await;
    let body = json!({
        "msgtype": "m.text",
        "body": "reply once",
        "m.relates_to": { "rel_type": "m.thread", "event_id": root }
    });

    // 同一 txn 发两次：第二次被 `room_event_txn_dedup` 挡下，返回同一个 event_id
    let first = send(&app, &token, &room_id, "txn-retry-reply", &body).await;
    let second = send(&app, &token, &room_id, "txn-retry-reply", &body).await;
    assert_eq!(first, second, "同一 txn 的重试必须返回同一个 event_id");

    let uri = format!("/_matrix/client/v3/rooms/{room_id}/threads/{root}");
    let (status, value) = call(&app, get(&uri, Some(&token))).await;
    assert_eq!(status, StatusCode::OK, "读线程详情必须 200：{value}");
    assert_eq!(value["reply_count"], json!(1), "重试同一 txn 不得让 reply_count 变成 2：{value}");
    let replies = value["replies"].as_array().unwrap_or_else(|| panic!("replies 必须是数组：{value}"));
    assert_eq!(replies.len(), 1, "重试同一 txn 不得在 replies 里出现两条：{value}");
    assert_eq!(replies[0]["event_id"], first.as_str(), "留下的必须是那条事件：{value}");
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
