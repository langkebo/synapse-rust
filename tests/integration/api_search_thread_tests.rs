use axum::{
    body::Body,
    http::{Request, StatusCode},
};
use serde_json::{json, Value};
use tower::ServiceExt;

async fn create_room(app: &axum::Router, token: &str) -> String {
    let request = Request::builder()
        .method("POST")
        .uri("/_matrix/client/v3/createRoom")
        .header("Authorization", format!("Bearer {}", token))
        .header("Content-Type", "application/json")
        .body(Body::from(json!({ "name": "Thread Test Room" }).to_string()))
        .unwrap();

    let response = ServiceExt::<Request<Body>>::oneshot(app.clone(), request).await.unwrap();
    assert_eq!(response.status(), StatusCode::OK);

    let body = axum::body::to_bytes(response.into_body(), 1024).await.unwrap();
    let json: Value = serde_json::from_slice(&body).unwrap();
    json["room_id"].as_str().unwrap().to_string()
}

async fn send_message(app: &axum::Router, token: &str, room_id: &str, txn_id: &str) -> String {
    let request = Request::builder()
        .method("PUT")
        .uri(format!("/_matrix/client/v3/rooms/{}/send/m.room.message/{}", room_id, txn_id))
        .header("Authorization", format!("Bearer {}", token))
        .header("Content-Type", "application/json")
        .body(Body::from(
            json!({
                "msgtype": "m.text",
                "body": "thread root"
            })
            .to_string(),
        ))
        .unwrap();

    let response = ServiceExt::<Request<Body>>::oneshot(app.clone(), request).await.unwrap();
    assert_eq!(response.status(), StatusCode::OK);

    let body = axum::body::to_bytes(response.into_body(), 1024).await.unwrap();
    let json: Value = serde_json::from_slice(&body).unwrap();
    json["event_id"].as_str().unwrap().to_string()
}

async fn create_thread(app: &axum::Router, token: &str, room_id: &str, root_event_id: &str) -> String {
    let request = Request::builder()
        .method("POST")
        .uri(format!("/_matrix/vendor/v1/rooms/{}/threads", room_id))
        .header("Authorization", format!("Bearer {}", token))
        .header("Content-Type", "application/json")
        .body(Body::from(
            // Thread creation takes the content from the root event; a non-empty
            // `content` here is rejected on purpose (audit I-07).
            json!({ "root_event_id": root_event_id }).to_string(),
        ))
        .unwrap();

    let response = ServiceExt::<Request<Body>>::oneshot(app.clone(), request).await.unwrap();
    assert_eq!(response.status(), StatusCode::OK);

    let body = axum::body::to_bytes(response.into_body(), 1024).await.unwrap();
    let json: Value = serde_json::from_slice(&body).unwrap();
    json["thread_id"].as_str().unwrap().to_string()
}

/// 发一条线程回复。请求体不带 `event_id` / `root_event_id`：回复事件的 id 由
/// 服务端写入口定稿，关联目标由 `thread_id` 决定（见 W3 / ARCH-01）。
async fn add_reply(app: &axum::Router, token: &str, room_id: &str, thread_id: &str) {
    let request = Request::builder()
        .method("POST")
        .uri(format!("/_matrix/client/v1/rooms/{}/threads/{}/replies", room_id, thread_id))
        .header("Authorization", format!("Bearer {}", token))
        .header("Content-Type", "application/json")
        .body(Body::from(
            json!({
                "content": {
                    "msgtype": "m.text",
                    "body": "thread reply"
                }
            })
            .to_string(),
        ))
        .unwrap();

    let response = ServiceExt::<Request<Body>>::oneshot(app.clone(), request).await.unwrap();
    assert_eq!(response.status(), StatusCode::OK);
}

async fn get_thread_response(
    app: &axum::Router,
    token: &str,
    room_id: &str,
    thread_id: &str,
) -> axum::response::Response {
    let request = Request::builder()
        .method("GET")
        .uri(format!("/_matrix/client/v1/rooms/{}/threads/{}?include_replies=true", room_id, thread_id))
        .header("Authorization", format!("Bearer {}", token))
        .body(Body::empty())
        .unwrap();

    ServiceExt::<Request<Body>>::oneshot(app.clone(), request).await.unwrap()
}

async fn get_user_id(app: &axum::Router, token: &str) -> String {
    let request = Request::builder()
        .method("GET")
        .uri("/_matrix/client/v3/account/whoami")
        .header("Authorization", format!("Bearer {}", token))
        .body(Body::empty())
        .unwrap();

    let response = ServiceExt::<Request<Body>>::oneshot(app.clone(), request).await.unwrap();
    assert_eq!(response.status(), StatusCode::OK);

    let body = axum::body::to_bytes(response.into_body(), 1024).await.unwrap();
    let json: Value = serde_json::from_slice(&body).unwrap();
    json["user_id"].as_str().unwrap().to_string()
}

async fn invite_user(app: &axum::Router, token: &str, room_id: &str, user_id: &str) {
    let request = Request::builder()
        .method("POST")
        .uri(format!("/_matrix/client/v3/rooms/{}/invite", room_id))
        .header("Authorization", format!("Bearer {}", token))
        .header("Content-Type", "application/json")
        .body(Body::from(json!({ "user_id": user_id }).to_string()))
        .unwrap();

    let response = ServiceExt::<Request<Body>>::oneshot(app.clone(), request).await.unwrap();
    assert_eq!(response.status(), StatusCode::OK);
}

async fn join_room(app: &axum::Router, token: &str, room_id: &str) {
    let request = Request::builder()
        .method("POST")
        .uri(format!("/_matrix/client/v3/rooms/{}/join", room_id))
        .header("Authorization", format!("Bearer {}", token))
        .body(Body::empty())
        .unwrap();

    let response = ServiceExt::<Request<Body>>::oneshot(app.clone(), request).await.unwrap();
    assert_eq!(response.status(), StatusCode::OK);
}

#[tokio::test]
async fn test_v3_search_validation_is_preserved_after_router_refactor() {
    let Some(app) = super::setup_fresh_test_app().await else {
        return;
    };
    let token = super::create_test_user(&app).await;

    let request = Request::builder()
        .method("POST")
        .uri("/_matrix/client/v3/search")
        .header("Authorization", format!("Bearer {}", token))
        .header("Content-Type", "application/json")
        .body(Body::from(
            json!({
                "search_categories": {
                    "room_events": {
                        "search_term": "   ",
                        "keys": ["content.body"]
                    }
                }
            })
            .to_string(),
        ))
        .unwrap();

    let response = ServiceExt::<Request<Body>>::oneshot(app.clone(), request).await.unwrap();

    assert_eq!(response.status(), StatusCode::BAD_REQUEST);

    let body = axum::body::to_bytes(response.into_body(), 1024).await.unwrap();
    let json: Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(json["error"], "Search term cannot be empty");
}

#[tokio::test]
async fn test_search_recipients_route_still_works_under_the_vendor_nest() {
    let Some(app) = super::setup_fresh_test_app().await else {
        return;
    };
    let token = super::create_test_user(&app).await;

    // `/_matrix/vendor/v1/search_recipients` 是私有端点的唯一规范位置
    // （`assembly.rs` 的 `nest("/_matrix/vendor/v1", create_vendor_router())`）。
    // 旧的 `/_matrix/client/v3/search_recipients` 别名已按铁律 1 删除。
    let request = Request::builder()
        .method("POST")
        .uri("/_matrix/vendor/v1/search_recipients")
        .header("Authorization", format!("Bearer {}", token))
        .header("Content-Type", "application/json")
        .body(Body::from(
            json!({
                "search_term": "   "
            })
            .to_string(),
        ))
        .unwrap();

    let response = ServiceExt::<Request<Body>>::oneshot(app.clone(), request).await.unwrap();

    assert_eq!(response.status(), StatusCode::BAD_REQUEST);

    let body = axum::body::to_bytes(response.into_body(), 1024).await.unwrap();
    let json: Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(json["error"], "Search term cannot be empty");
}

#[tokio::test]
async fn test_room_context_rejects_non_member_and_admin_override() {
    let Some(app) = super::setup_fresh_test_app().await else {
        return;
    };
    let owner_token = super::create_test_user(&app).await;
    let guest_token = super::create_test_user(&app).await;
    let (admin_token, _) = super::get_admin_token(&app).await;

    let room_id = create_room(&app, &owner_token).await;
    let event_id = send_message(&app, &owner_token, &room_id, "context_guard_txn").await;

    let guest_request = Request::builder()
        .method("GET")
        .uri(format!("/_matrix/client/v3/rooms/{}/context/{}", room_id, event_id))
        .header("Authorization", format!("Bearer {}", guest_token))
        .body(Body::empty())
        .unwrap();
    let guest_response = ServiceExt::<Request<Body>>::oneshot(app.clone(), guest_request).await.unwrap();
    assert_eq!(guest_response.status(), StatusCode::FORBIDDEN);

    let admin_request = Request::builder()
        .method("GET")
        .uri(format!("/_matrix/client/v3/rooms/{}/context/{}", room_id, event_id))
        .header("Authorization", format!("Bearer {}", admin_token))
        .body(Body::empty())
        .unwrap();
    let admin_response = ServiceExt::<Request<Body>>::oneshot(app.clone(), admin_request).await.unwrap();
    assert_eq!(admin_response.status(), StatusCode::FORBIDDEN);

    let owner_request = Request::builder()
        .method("GET")
        .uri(format!("/_matrix/client/v3/rooms/{}/context/{}", room_id, event_id))
        .header("Authorization", format!("Bearer {}", owner_token))
        .body(Body::empty())
        .unwrap();
    let owner_response = ServiceExt::<Request<Body>>::oneshot(app, owner_request).await.unwrap();
    assert_eq!(owner_response.status(), StatusCode::OK);
}

#[tokio::test]
async fn test_user_level_thread_route_is_served_under_vendor_prefix() {
    let Some(app) = super::setup_fresh_test_app().await else {
        return;
    };

    // ISSUE-13：用户级线程列表是私有扩展，唯一规范位置是 `/_matrix/vendor/v1`；
    // client 前缀下的旧路径已按铁律 1 删除（不再可达）。
    let request = Request::builder()
        .method("GET")
        .uri("/_matrix/vendor/v1/user/@alice:localhost/rooms/!room:localhost/threads")
        .body(Body::empty())
        .unwrap();

    let response = ServiceExt::<Request<Body>>::oneshot(app, request).await.unwrap();

    // 无 token ⇒ 认证提取器先失败（说明路由**存在**且挂上了鉴权层）
    assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
}

/// W3 / ARCH-01（方案 A：打通）门禁 —— 线程回复必须落一条**规范** `m.thread` 事件。
///
/// 影子表（`thread_replies`）只能是对规范事件的索引：它回指的 `event_id` 必须在
/// `events` 表里真实存在，且该事件的 `content.m.relates_to` 指向线程根。
/// 修复前 `ThreadService::add_reply` 只写影子表，`events` 里没有任何事件 ⇒ 这条用例必红。
#[tokio::test]
async fn test_thread_reply_writes_canonical_m_thread_event() {
    let Some((app, pool, _cache)) = super::setup_fresh_test_app_with_pool().await else {
        return;
    };
    let token = super::create_test_user(&app).await;
    let room_id = create_room(&app, &token).await;
    let root_event_id = send_message(&app, &token, &room_id, "canonical_thread_root_txn").await;
    let thread_id = create_thread(&app, &token, &room_id, &root_event_id).await;

    add_reply(&app, &token, &room_id, &thread_id).await;

    let indexed_event_id: String =
        sqlx::query_scalar("SELECT event_id FROM thread_replies WHERE room_id = $1 AND thread_id = $2")
            .bind(&room_id)
            .bind(&thread_id)
            .fetch_one(&*pool)
            .await
            .expect("线程回复必须被索引到 thread_replies");

    let content: Option<Value> = sqlx::query_scalar("SELECT content FROM events WHERE event_id = $1")
        .bind(&indexed_event_id)
        .fetch_optional(&*pool)
        .await
        .expect("查询 events 失败");

    let content = content.unwrap_or_else(|| {
        panic!(
            "thread_replies.event_id = {indexed_event_id} 在 events 表里不存在 —— \
             影子表只是规范事件的索引，不能凭空造事件"
        )
    });

    assert_eq!(content["m.relates_to"]["rel_type"], "m.thread", "回复必须是规范 m.thread 事件");
    assert_eq!(content["m.relates_to"]["event_id"], root_event_id, "m.thread 必须指向线程根");
}

/// W3 / ARCH-01（方案 A：打通）门禁 —— 线程根必须是房间里**真实存在**的事件。
///
/// 线程根本身就是那条规范事件（客户端不需要再发一条），所以「影子表降级为索引」
/// 对建线程的要求是：`thread_roots.root_event_id` 必须能在 `events` 里找到。
/// 修复前 `create_thread` 不校验根事件 ⇒ 传一个不存在的 id 也能建出影子行，必红。
#[tokio::test]
async fn test_create_thread_rejects_unknown_root_event() {
    let Some(app) = super::setup_fresh_test_app().await else {
        return;
    };
    let token = super::create_test_user(&app).await;
    let room_id = create_room(&app, &token).await;

    let request = Request::builder()
        .method("POST")
        .uri(format!("/_matrix/vendor/v1/rooms/{}/threads", room_id))
        .header("Authorization", format!("Bearer {}", token))
        .header("Content-Type", "application/json")
        .body(Body::from(json!({ "root_event_id": "$does_not_exist:localhost" }).to_string()))
        .unwrap();

    let response = ServiceExt::<Request<Body>>::oneshot(app, request).await.unwrap();

    assert_eq!(
        response.status(),
        StatusCode::NOT_FOUND,
        "线程根事件不存在时必须 404，而不是在影子表里造一行指向空气的线程"
    );
}

#[tokio::test]
async fn test_get_thread_returns_real_thread_details() {
    let Some(app) = super::setup_fresh_test_app().await else {
        return;
    };
    let token = super::create_test_user(&app).await;
    let room_id = create_room(&app, &token).await;
    let root_event_id = send_message(&app, &token, &room_id, "thread_root_txn").await;
    let thread_id = create_thread(&app, &token, &room_id, &root_event_id).await;

    add_reply(&app, &token, &room_id, &thread_id).await;

    let response = get_thread_response(&app, &token, &room_id, &thread_id).await;
    assert_eq!(response.status(), StatusCode::OK);

    let body = axum::body::to_bytes(response.into_body(), 4096).await.unwrap();
    let json: Value = serde_json::from_slice(&body).unwrap();

    assert_eq!(json["root"]["thread_id"], thread_id);
    assert_eq!(json["root"]["root_event_id"], root_event_id);
    assert_eq!(json["reply_count"], 1);
    assert_eq!(json["replies"].as_array().unwrap().len(), 1);
    assert_eq!(json["replies"][0]["thread_id"], thread_id);
}

#[tokio::test]
async fn test_get_thread_forbids_non_members() {
    let Some(app) = super::setup_fresh_test_app().await else {
        return;
    };
    let owner_token = super::create_test_user(&app).await;
    let outsider_token = super::create_test_user(&app).await;
    let room_id = create_room(&app, &owner_token).await;
    let root_event_id = send_message(&app, &owner_token, &room_id, "thread_forbidden_txn").await;
    let thread_id = create_thread(&app, &owner_token, &room_id, &root_event_id).await;

    let response = get_thread_response(&app, &outsider_token, &room_id, &thread_id).await;
    assert_eq!(response.status(), StatusCode::FORBIDDEN);
}

#[tokio::test]
async fn test_thread_management_requires_room_creator() {
    let Some(app) = super::setup_fresh_test_app().await else {
        return;
    };
    let owner_token = super::create_test_user(&app).await;
    let member_token = super::create_test_user(&app).await;
    let member_user_id = get_user_id(&app, &member_token).await;
    let room_id = create_room(&app, &owner_token).await;
    invite_user(&app, &owner_token, &room_id, &member_user_id).await;
    join_room(&app, &member_token, &room_id).await;
    let root_event_id = send_message(&app, &owner_token, &room_id, "thread_manage_txn").await;
    let thread_id = create_thread(&app, &owner_token, &room_id, &root_event_id).await;

    let freeze_request = Request::builder()
        .method("POST")
        .uri(format!("/_matrix/vendor/v1/rooms/{}/threads/{}/freeze", room_id, thread_id))
        .header("Authorization", format!("Bearer {}", member_token))
        .body(Body::empty())
        .unwrap();
    let response = ServiceExt::<Request<Body>>::oneshot(app.clone(), freeze_request).await.unwrap();
    assert_eq!(response.status(), StatusCode::FORBIDDEN);

    let unfreeze_request = Request::builder()
        .method("POST")
        .uri(format!("/_matrix/vendor/v1/rooms/{}/threads/{}/unfreeze", room_id, thread_id))
        .header("Authorization", format!("Bearer {}", member_token))
        .body(Body::empty())
        .unwrap();
    let response = ServiceExt::<Request<Body>>::oneshot(app.clone(), unfreeze_request).await.unwrap();
    assert_eq!(response.status(), StatusCode::FORBIDDEN);

    let delete_request = Request::builder()
        .method("DELETE")
        .uri(format!("/_matrix/vendor/v1/rooms/{}/threads/{}", room_id, thread_id))
        .header("Authorization", format!("Bearer {}", member_token))
        .body(Body::empty())
        .unwrap();
    let response = ServiceExt::<Request<Body>>::oneshot(app.clone(), delete_request).await.unwrap();
    assert_eq!(response.status(), StatusCode::FORBIDDEN);

    let owner_freeze_request = Request::builder()
        .method("POST")
        .uri(format!("/_matrix/vendor/v1/rooms/{}/threads/{}/freeze", room_id, thread_id))
        .header("Authorization", format!("Bearer {}", owner_token))
        .body(Body::empty())
        .unwrap();
    let response = ServiceExt::<Request<Body>>::oneshot(app.clone(), owner_freeze_request).await.unwrap();
    assert_eq!(response.status(), StatusCode::OK);

    let owner_unfreeze_request = Request::builder()
        .method("POST")
        .uri(format!("/_matrix/vendor/v1/rooms/{}/threads/{}/unfreeze", room_id, thread_id))
        .header("Authorization", format!("Bearer {}", owner_token))
        .body(Body::empty())
        .unwrap();
    let response = ServiceExt::<Request<Body>>::oneshot(app.clone(), owner_unfreeze_request).await.unwrap();
    assert_eq!(response.status(), StatusCode::OK);

    let owner_delete_request = Request::builder()
        .method("DELETE")
        .uri(format!("/_matrix/vendor/v1/rooms/{}/threads/{}", room_id, thread_id))
        .header("Authorization", format!("Bearer {}", owner_token))
        .body(Body::empty())
        .unwrap();
    let response = ServiceExt::<Request<Body>>::oneshot(app, owner_delete_request).await.unwrap();
    assert_eq!(response.status(), StatusCode::NO_CONTENT);
}

#[tokio::test]
async fn test_global_thread_routes_return_real_data() {
    let Some(app) = super::setup_fresh_test_app().await else {
        return;
    };
    let token = super::create_test_user(&app).await;
    let room_id = create_room(&app, &token).await;
    let root_event_id = send_message(&app, &token, &room_id, "global_thread_root_txn").await;

    let create_request = Request::builder()
        .method("POST")
        .uri("/_matrix/vendor/v1/threads")
        .header("Authorization", format!("Bearer {}", token))
        .header("Content-Type", "application/json")
        .body(Body::from(
            // Same contract as the per-room route: content comes from the root
            // event, so a non-empty `content` is rejected (audit I-07).
            json!({
                "room_id": room_id,
                "root_event_id": root_event_id,
            })
            .to_string(),
        ))
        .unwrap();

    let create_response = ServiceExt::<Request<Body>>::oneshot(app.clone(), create_request).await.unwrap();
    assert_eq!(create_response.status(), StatusCode::OK);

    let body = axum::body::to_bytes(create_response.into_body(), 2048).await.unwrap();
    let create_json: Value = serde_json::from_slice(&body).unwrap();
    let thread_id = create_json["thread_id"].as_str().unwrap().to_string();

    let list_request = Request::builder()
        .method("GET")
        .uri("/_matrix/vendor/v1/threads")
        .header("Authorization", format!("Bearer {}", token))
        .body(Body::empty())
        .unwrap();

    let list_response = ServiceExt::<Request<Body>>::oneshot(app.clone(), list_request).await.unwrap();
    assert_eq!(list_response.status(), StatusCode::OK);

    let body = axum::body::to_bytes(list_response.into_body(), 4096).await.unwrap();
    let list_json: Value = serde_json::from_slice(&body).unwrap();
    let threads = list_json["threads"].as_array().unwrap();

    assert!(threads.iter().any(|thread| thread["thread_id"] == thread_id && thread["room_id"] == room_id));
    assert!(list_json["total"].as_i64().unwrap_or_default() >= 1);

    let subscribe_request = Request::builder()
        .method("POST")
        .uri(format!("/_matrix/client/v1/rooms/{}/threads/{}/subscribe", room_id, thread_id))
        .header("Authorization", format!("Bearer {}", token))
        .header("Content-Type", "application/json")
        .body(Body::from(
            json!({
                "notification_level": "all"
            })
            .to_string(),
        ))
        .unwrap();

    let subscribe_response = ServiceExt::<Request<Body>>::oneshot(app.clone(), subscribe_request).await.unwrap();
    assert_eq!(subscribe_response.status(), StatusCode::OK);

    let subscribed_request = Request::builder()
        .method("GET")
        .uri("/_matrix/vendor/v1/threads/subscribed")
        .header("Authorization", format!("Bearer {}", token))
        .body(Body::empty())
        .unwrap();

    let subscribed_response = ServiceExt::<Request<Body>>::oneshot(app, subscribed_request).await.unwrap();
    assert_eq!(subscribed_response.status(), StatusCode::OK);

    let body = axum::body::to_bytes(subscribed_response.into_body(), 4096).await.unwrap();
    let subscribed_json: Value = serde_json::from_slice(&body).unwrap();

    assert!(subscribed_json["threads"].as_array().unwrap().iter().any(|thread| thread["thread_id"] == thread_id));
    assert!(subscribed_json["subscribed"]
        .as_array()
        .unwrap()
        .iter()
        .any(|subscription| { subscription["thread_id"] == thread_id && subscription["notification_level"] == "all" }));
}

#[tokio::test]
async fn test_global_threads_only_include_joined_rooms() {
    let Some(app) = super::setup_fresh_test_app().await else {
        return;
    };
    let owner_token = super::create_test_user(&app).await;
    let outsider_token = super::create_test_user(&app).await;
    let room_id = create_room(&app, &owner_token).await;
    let root_event_id = send_message(&app, &owner_token, &room_id, "global_scope_root_txn").await;
    let thread_id = create_thread(&app, &owner_token, &room_id, &root_event_id).await;

    let request = Request::builder()
        .method("GET")
        .uri("/_matrix/vendor/v1/threads")
        .header("Authorization", format!("Bearer {}", outsider_token))
        .body(Body::empty())
        .unwrap();

    let response = ServiceExt::<Request<Body>>::oneshot(app, request).await.unwrap();
    assert_eq!(response.status(), StatusCode::OK);

    let body = axum::body::to_bytes(response.into_body(), 4096).await.unwrap();
    let json: Value = serde_json::from_slice(&body).unwrap();
    let threads = json["threads"].as_array().unwrap();
    assert!(!threads.iter().any(|thread| thread["thread_id"] == thread_id));
    assert_eq!(json["total"], 0);
}

/// 调 `GET …/rooms/{room_id}/threads/search`，返回 (状态码, JSON)。
async fn room_thread_search(
    app: &axum::Router,
    token: &str,
    room_id: &str,
    term: &str,
    limit: Option<u32>,
    from: Option<&str>,
) -> (StatusCode, Value) {
    let mut uri = format!("/_matrix/vendor/v1/rooms/{}/threads/search?q={}", room_id, term);
    if let Some(limit) = limit {
        uri.push_str(&format!("&limit={limit}"));
    }
    if let Some(from) = from {
        uri.push_str(&format!("&from={from}"));
    }

    let request = Request::builder()
        .method("GET")
        .uri(uri)
        .header("Authorization", format!("Bearer {}", token))
        .body(Body::empty())
        .unwrap();

    let response = ServiceExt::<Request<Body>>::oneshot(app.clone(), request).await.unwrap();
    let status = response.status();
    let body = axum::body::to_bytes(response.into_body(), 8192).await.unwrap();
    let json: Value = serde_json::from_slice(&body).unwrap_or(Value::Null);
    (status, json)
}

/// 话题搜索必须与同族 `list_threads` **同形**（信封 + `next_batch` 游标），且游标能续读。
///
/// 交接单 §6 Q5：修复前返回**裸数组**且 `from` 被静默忽略 ⇒ 对接的 SDK 按
/// `IThreadListResponse { threads; next_batch? }` 读 ⇒ `result.threads` 恒为 `undefined`、
/// 结果集被 `limit` 截断后无法续读（该方法完全不可用）。这条测试同时钉住
/// ① 信封形状、② `limit` 截断时必给 `next_batch`、③ 相邻页不重复、④ 畸形游标 400。
#[tokio::test]
async fn test_room_thread_search_returns_envelope_and_paginates_by_cursor() {
    let Some(app) = super::setup_fresh_test_app().await else {
        return;
    };
    let token = super::create_test_user(&app).await;
    let room_id = create_room(&app, &token).await;

    // 造 2 条命中：命中来自**最新回复**的 body（`thread_replies.content`）。
    let mut expected = Vec::new();
    for i in 0..2 {
        let root_event_id = send_message(&app, &token, &room_id, &format!("search_root_txn_{i}")).await;
        let thread_id = create_thread(&app, &token, &room_id, &root_event_id).await;
        add_reply(&app, &token, &room_id, &thread_id).await;
        expected.push(thread_id);
    }
    expected.sort();

    // ① 未截断：信封对象（不是裸数组）、没有 next_batch
    let (status, full) = room_thread_search(&app, &token, &room_id, "thread", None, None).await;
    assert_eq!(status, StatusCode::OK);
    assert!(full.is_object(), "响应必须是信封对象 —— SDK 按 IThreadListResponse 读 `threads`");
    assert_eq!(full["threads"].as_array().expect("`threads` 必须是数组").len(), 2);
    assert!(full["next_batch"].is_null(), "未截断时不应给 next_batch");

    // ② limit=1 截断：必须给出 next_batch
    let (status, page1) = room_thread_search(&app, &token, &room_id, "thread", Some(1), None).await;
    assert_eq!(status, StatusCode::OK);
    let page1_threads = page1["threads"].as_array().expect("`threads` 必须是数组");
    assert_eq!(page1_threads.len(), 1);
    let cursor = page1["next_batch"].as_str().expect("被截断的页必须带 next_batch").to_string();

    // ③ 第二页给出**另一条**，且没有更多
    let (status, page2) = room_thread_search(&app, &token, &room_id, "thread", Some(1), Some(&cursor)).await;
    assert_eq!(status, StatusCode::OK);
    let page2_threads = page2["threads"].as_array().expect("`threads` 必须是数组");
    assert_eq!(page2_threads.len(), 1);
    assert_ne!(page2_threads[0]["thread_id"], page1_threads[0]["thread_id"], "相邻页不得重复同一行");
    assert!(page2["next_batch"].is_null(), "已是最后一页");

    // 两页并集 == 全部命中（游标**不漏行**）
    let mut seen = vec![
        page1_threads[0]["thread_id"].as_str().expect("thread_id").to_string(),
        page2_threads[0]["thread_id"].as_str().expect("thread_id").to_string(),
    ];
    seen.sort();
    assert_eq!(seen, expected);

    // ④ 畸形游标：必须 400 —— 静默降级成"第一页"会让客户端拿到错的一页却不报错
    let (status, _) = room_thread_search(&app, &token, &room_id, "thread", Some(1), Some("not-a-cursor")).await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
}
