#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

//! NEW-09 第五波 2/3 —— `sliding_sync` 端点族的**真 HTTP 往返**。
//!
//! `tests/unit/sliding_sync_shape_only_tests.rs` 只断言自己构造的字面量；此前这一族只有
//! `sliding_sync_service_tests_migrated` / `sliding_sync_storage_tests_migrated`（service 级、
//! storage 级），**没有任何一条走过 HTTP**。本文件打真 router：
//!
//! * `POST /_matrix/client/v4/sync`（MSC4186 稳定路径）
//! * `POST /_matrix/client/unstable/org.matrix.msc3575/sync`（MSC3575）
//! * `POST /_matrix/client/unstable/org.matrix.simplified_msc3575/sync`
//! * `POST /_matrix/client/v1/sync`
//!
//! ⚠️ **NEW-17（实测）**：`rooms` 返回的是扁平 `room_id → 房间对象`，而规范要求
//! `rooms: {join, invite, leave}` ⇒ 按规范读 `rooms.join` 的客户端会看到空房间列表。
//!
//! 四条路径在台账里都有，但**名字对不等于接通**：本文件用同一份请求体依次打四条，
//! 任何一条落到 404/405 就会红（这正是"路由注册了但 handler 没接上"的典型症状）。

use axum::{
    body::Body,
    http::{Request, StatusCode},
};
use serde_json::{json, Value};
use tower::ServiceExt;

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
    let bytes = axum::body::to_bytes(response.into_body(), 256 * 1024).await.unwrap();
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

async fn create_room(app: &axum::Router, token: &str, body: &Value) -> String {
    let request = Request::builder()
        .method("POST")
        .uri("/_matrix/client/v3/createRoom")
        .header("Authorization", format!("Bearer {token}"))
        .header("Content-Type", "application/json")
        .body(Body::from(body.to_string()))
        .unwrap();
    let (status, value) = call(app, request).await;
    assert_eq!(status, StatusCode::OK, "建房必须 200：{value}");
    value["room_id"].as_str().expect("建房必须返回 room_id").to_string()
}

fn sync_body() -> Value {
    json!({
        "lists": {
            "main": {
                "ranges": [[0, 20]],
                "required_state": [["m.room.name", ""]],
                "timeline_limit": 10
            }
        },
        "room_subscriptions": {}
    })
}

/// 主干：一次滑动同步必须返回 `pos`（客户端靠它做增量）+ `lists.main` + `rooms`。
#[tokio::test]
async fn sliding_sync_returns_pos_lists_and_rooms() {
    let Some(app) = super::setup_fresh_test_app().await else {
        return;
    };
    let token = register(&app, "slidingsync_user").await;

    let (status, value) = call(&app, post_json("/_matrix/client/v4/sync", &token, &sync_body())).await;
    assert_eq!(status, StatusCode::OK, "滑动同步必须 200：{value}");

    let pos = value["pos"].as_str().unwrap_or_else(|| panic!("必须返回 pos（增量游标）：{value}"));
    assert!(!pos.is_empty(), "pos 不能是空串：{value}");

    let lists = value["lists"].as_object().unwrap_or_else(|| panic!("lists 必须是对象：{value}"));
    assert!(lists.contains_key("main"), "请求里的 main 列表必须回显（客户端按键取结果）：{value}");

    assert!(value["rooms"].is_object(), "rooms 必须是对象（{{join,invite,leave}}）：{value}");
}

/// 刚创建的房间必须出现在 `rooms.join` 里 —— 列表不是空的，且房间归属正确。
#[tokio::test]
async fn sliding_sync_lists_joined_room() {
    let Some(app) = super::setup_fresh_test_app().await else {
        return;
    };
    let token = register(&app, "slidingsync_room_user").await;
    let room_id = create_room(&app, &token, &json!({ "name": "sliding sync probe", "preset": "private_chat" })).await;

    let (status, value) = call(&app, post_json("/_matrix/client/v4/sync", &token, &sync_body())).await;
    assert_eq!(status, StatusCode::OK, "滑动同步必须 200：{value}");

    // ① 规范里"有哪些房间"的唯一权威来源是 `lists.<key>.ops[].room_ids` —— 这条必须是通的
    let ops = value["lists"]["main"]["ops"].as_array().unwrap_or_else(|| panic!("lists.main.ops 必须是数组：{value}"));
    let listed: Vec<&str> =
        ops.iter().filter_map(|op| op["room_ids"].as_array()).flatten().filter_map(|id| id.as_str()).collect();
    assert!(
        listed.contains(&room_id.as_str()),
        "刚建的房间 {room_id} 必须出现在 lists.main.ops[].room_ids 里：{value}"
    );

    // ② ⚠️ NEW-17：`rooms` 目前是**扁平**的 `room_id → 房间对象`，
    //    而 MSC3575/MSC4186 规定是 `rooms: {join: {...}, invite: {...}, leave: {...}}`。
    //    构造处 `sliding_sync_service::filters::build_rooms_response` 里是
    //    `rooms_json.insert(room_id, payload)`，没有按 membership 分组 ⇒
    //    **按规范读 `rooms.join` 的客户端会拿到 undefined，房间列表显示为空**。
    //    这里断言**实际形状**（不是理想形状）：修好后 `rooms[room_id]` 会消失、本用例会红，
    //    那时按 `rooms.join[room_id]` 改断言即可。
    let rooms = value["rooms"].as_object().unwrap_or_else(|| panic!("rooms 必须是对象：{value}"));
    assert!(
        rooms.contains_key(&room_id),
        "当前 rooms 是扁平 map，房间 {room_id} 必须直接作为键出现（见 NEW-17）：{value}"
    );
}

/// 四条路径必须**都接通**（台账里有名字 ≠ handler 接上了）。
#[tokio::test]
async fn all_four_sync_paths_are_wired() {
    let Some(app) = super::setup_fresh_test_app().await else {
        return;
    };
    let token = register(&app, "slidingsync_paths_user").await;

    for path in [
        "/_matrix/client/v1/sync",
        "/_matrix/client/v4/sync",
        "/_matrix/client/unstable/org.matrix.msc3575/sync",
        "/_matrix/client/unstable/org.matrix.simplified_msc3575/sync",
    ] {
        let (status, value) = call(&app, post_json(path, &token, &sync_body())).await;
        assert_eq!(status, StatusCode::OK, "{path} 必须 200（注册了但没接上会 404/405）：{value}");
        assert!(value["pos"].as_str().is_some_and(|pos| !pos.is_empty()), "{path} 必须返回 pos：{value}");
    }
}

/// 带 `pos` 的增量请求必须被接受（客户端第二轮就靠它，500 会让客户端反复全量同步）。
#[tokio::test]
async fn incremental_sync_with_pos_is_accepted() {
    let Some(app) = super::setup_fresh_test_app().await else {
        return;
    };
    let token = register(&app, "slidingsync_incr_user").await;

    let (status, first) = call(&app, post_json("/_matrix/client/v4/sync", &token, &sync_body())).await;
    assert_eq!(status, StatusCode::OK, "首轮必须 200：{first}");
    let pos = first["pos"].as_str().expect("必须返回 pos").to_string();

    let mut second_body = sync_body();
    second_body["pos"] = json!(pos);
    let (status, second) = call(&app, post_json("/_matrix/client/v4/sync", &token, &second_body)).await;
    assert_eq!(status, StatusCode::OK, "带 pos 的增量请求必须 200：{second}");
    assert!(second["pos"].as_str().is_some_and(|next| !next.is_empty()), "增量请求也必须返回新的 pos：{second}");
}

/// 未认证 ⇒ 401。
#[tokio::test]
async fn sliding_sync_requires_authentication() {
    let Some(app) = super::setup_fresh_test_app().await else {
        return;
    };

    let request = Request::builder()
        .method("POST")
        .uri("/_matrix/client/v4/sync")
        .header("Content-Type", "application/json")
        .body(Body::from(sync_body().to_string()))
        .unwrap();
    let (status, value) = call(&app, request).await;
    assert_eq!(status, StatusCode::UNAUTHORIZED, "未认证必须 401：{value}");
    let errcode = value["errcode"].as_str().unwrap_or_default();
    assert!(errcode.starts_with("M_"), "必须带 Matrix errcode：{value}");
}
