use axum::{
    body::Body,
    http::{Request, StatusCode},
};
use serde_json::{json, Value};
use sqlx::Row;
use synapse_common::current_timestamp_millis;
use tower::ServiceExt;

/// 测试房间管理完整生命周期：创建 → 查询 → 删除 → 验证删除
#[tokio::test]
async fn test_admin_room_lifecycle_management() {
    let Some(app) = super::setup_fresh_test_app().await else {
        return;
    };
    let (admin_token, _) = super::get_admin_token(&app).await;

    // 1. 创建测试用户
    let username = format!("roomowner_{}", rand::random::<u32>());
    let register_request = Request::builder()
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

    let response = ServiceExt::<Request<Body>>::oneshot(app.clone(), register_request).await.unwrap();

    assert_eq!(response.status(), StatusCode::OK);
    let body = axum::body::to_bytes(response.into_body(), 1024).await.unwrap();
    let json: Value = serde_json::from_slice(&body).unwrap();
    let user_token = json["access_token"].as_str().unwrap().to_string();

    // 2. 用户创建房间
    let room_name = format!("Test Room {}", rand::random::<u32>());
    let create_room_request = Request::builder()
        .method("POST")
        .uri("/_matrix/client/v3/createRoom")
        .header("Authorization", format!("Bearer {}", user_token))
        .header("Content-Type", "application/json")
        .body(Body::from(
            json!({
                "name": room_name,
                "preset": "private_chat"
            })
            .to_string(),
        ))
        .unwrap();

    let response = ServiceExt::<Request<Body>>::oneshot(app.clone(), create_room_request).await.unwrap();

    assert_eq!(response.status(), StatusCode::OK);
    let body = axum::body::to_bytes(response.into_body(), 1024).await.unwrap();
    let json: Value = serde_json::from_slice(&body).unwrap();
    let room_id = json["room_id"].as_str().unwrap().to_string();

    // 3. 管理员查询房间详情
    let encoded_room_id = room_id.replace('!', "%21").replace(':', "%3A");
    let get_room_request = Request::builder()
        .uri(format!("/_synapse/admin/v1/rooms/{}", encoded_room_id))
        .header("Authorization", format!("Bearer {}", admin_token))
        .body(Body::empty())
        .unwrap();

    let response = ServiceExt::<Request<Body>>::oneshot(app.clone(), get_room_request).await.unwrap();

    assert_eq!(response.status(), StatusCode::OK);
    let body = axum::body::to_bytes(response.into_body(), 1024).await.unwrap();
    let json: Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(json["room_id"], room_id);
    assert_eq!(json["name"], room_name);

    // 4. 管理员删除房间
    let delete_room_request = Request::builder()
        .method("DELETE")
        .uri(format!("/_synapse/admin/v1/rooms/{}", encoded_room_id))
        .header("Authorization", format!("Bearer {}", admin_token))
        .header("Content-Type", "application/json")
        .body(Body::from(
            json!({
                "block": true,
                "purge": true
            })
            .to_string(),
        ))
        .unwrap();

    let response = ServiceExt::<Request<Body>>::oneshot(app.clone(), delete_room_request).await.unwrap();

    // 删除应该返回 200 或 202（异步删除）
    assert!(
        response.status() == StatusCode::OK || response.status() == StatusCode::ACCEPTED,
        "Room deletion should succeed with status 200 or 202"
    );

    // 5. 验证房间已被删除（查询返回 404 或显示已删除状态）
    let verify_deleted_request = Request::builder()
        .uri(format!("/_synapse/admin/v1/rooms/{}", encoded_room_id))
        .header("Authorization", format!("Bearer {}", admin_token))
        .body(Body::empty())
        .unwrap();

    let response = ServiceExt::<Request<Body>>::oneshot(app.clone(), verify_deleted_request).await.unwrap();

    // 应该返回 404 或显示房间已被删除
    assert!(
        response.status() == StatusCode::NOT_FOUND || response.status() == StatusCode::OK,
        "Deleted room should return 404 or show deleted status"
    );

    if response.status() == StatusCode::OK {
        let body = axum::body::to_bytes(response.into_body(), 1024).await.unwrap();
        let json: Value = serde_json::from_slice(&body).unwrap();
        // 如果返回 200，应该显示房间已被删除或阻止
        assert!(
            json["blocked"].as_bool().unwrap_or(false) || json["deleted"].as_bool().unwrap_or(false),
            "Room should be marked as blocked or deleted"
        );
    }

    // 6. 验证用户无法再访问该房间
    let user_access_request = Request::builder()
        .uri(format!("/_matrix/client/v3/rooms/{}/state", room_id))
        .header("Authorization", format!("Bearer {}", user_token))
        .body(Body::empty())
        .unwrap();

    let response = ServiceExt::<Request<Body>>::oneshot(app.clone(), user_access_request).await.unwrap();

    // 用户应该无法访问已删除的房间
    assert!(
        response.status() == StatusCode::NOT_FOUND || response.status() == StatusCode::FORBIDDEN,
        "User should not be able to access deleted room"
    );
}

/// 测试房间历史清理功能
#[tokio::test]
async fn test_admin_room_history_purge() {
    let Some(app) = super::setup_fresh_test_app().await else {
        return;
    };
    let (admin_token, _) = super::get_super_admin_token(&app).await;

    // 1. 创建测试用户
    let username = format!("historyuser_{}", rand::random::<u32>());
    let register_request = Request::builder()
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

    let response = ServiceExt::<Request<Body>>::oneshot(app.clone(), register_request).await.unwrap();

    assert_eq!(response.status(), StatusCode::OK);
    let body = axum::body::to_bytes(response.into_body(), 1024).await.unwrap();
    let json: Value = serde_json::from_slice(&body).unwrap();
    let user_token = json["access_token"].as_str().unwrap().to_string();

    // 2. 创建房间
    let create_room_request = Request::builder()
        .method("POST")
        .uri("/_matrix/client/v3/createRoom")
        .header("Authorization", format!("Bearer {}", user_token))
        .header("Content-Type", "application/json")
        .body(Body::from(
            json!({
                "name": "History Test Room",
                "preset": "private_chat"
            })
            .to_string(),
        ))
        .unwrap();

    let response = ServiceExt::<Request<Body>>::oneshot(app.clone(), create_room_request).await.unwrap();

    assert_eq!(response.status(), StatusCode::OK);
    let body = axum::body::to_bytes(response.into_body(), 1024).await.unwrap();
    let json: Value = serde_json::from_slice(&body).unwrap();
    let room_id = json["room_id"].as_str().unwrap().to_string();

    // 3. 发送一些消息
    for i in 0..3 {
        let send_message_request = Request::builder()
            .method("PUT")
            .uri(format!("/_matrix/client/v3/rooms/{}/send/m.room.message/txn_{}", room_id, i))
            .header("Authorization", format!("Bearer {}", user_token))
            .header("Content-Type", "application/json")
            .body(Body::from(
                json!({
                    "msgtype": "m.text",
                    "body": format!("Test message {}", i)
                })
                .to_string(),
            ))
            .unwrap();

        let response = ServiceExt::<Request<Body>>::oneshot(app.clone(), send_message_request).await.unwrap();

        assert_eq!(response.status(), StatusCode::OK);
    }

    // 4. 管理员清理房间历史（保留最近 1 条消息）
    let encoded_room_id = room_id.replace('!', "%21").replace(':', "%3A");
    let purge_history_request = Request::builder()
        .method("POST")
        .uri(format!("/_synapse/admin/v1/rooms/{}/purge_history", encoded_room_id))
        .header("Authorization", format!("Bearer {}", admin_token))
        .header("Content-Type", "application/json")
        .body(Body::from(
            json!({
                "delete_local_events": true,
                "purge_up_to_ts": (current_timestamp_millis() - 1000) // 1秒前
            })
            .to_string(),
        ))
        .unwrap();

    let response = ServiceExt::<Request<Body>>::oneshot(app.clone(), purge_history_request).await.unwrap();

    // 清理历史应该返回 200 或 202
    assert!(
        response.status() == StatusCode::OK || response.status() == StatusCode::ACCEPTED,
        "History purge should succeed"
    );

    // 5. 验证历史已被清理（可选，取决于实现）
    // 这里可以查询房间消息，验证旧消息已被删除
}

#[tokio::test]
async fn test_admin_purge_history_requires_existing_room() {
    let Some(app) = super::setup_fresh_test_app().await else {
        return;
    };
    let (admin_token, _) = super::get_super_admin_token(&app).await;

    let missing_room_id = format!("!missingpurge{}:localhost", rand::random::<u32>());
    let encoded_room_id = missing_room_id.replace('!', "%21").replace(':', "%3A");

    let purge_history_request = Request::builder()
        .method("POST")
        .uri(format!("/_synapse/admin/v1/rooms/{}/purge_history", encoded_room_id))
        .header("Authorization", format!("Bearer {}", admin_token))
        .header("Content-Type", "application/json")
        .body(Body::from(
            json!({
                "delete_local_events": true,
                "purge_up_to_ts": current_timestamp_millis()
            })
            .to_string(),
        ))
        .unwrap();

    let response = ServiceExt::<Request<Body>>::oneshot(app, purge_history_request).await.unwrap();

    assert_eq!(response.status(), StatusCode::NOT_FOUND);
}

/// 测试批量房间查询和搜索
#[tokio::test]
async fn test_admin_room_list_and_search() {
    let Some(app) = super::setup_fresh_test_app().await else {
        return;
    };
    let (admin_token, _) = super::get_admin_token(&app).await;

    // 1. 创建测试用户
    let username = format!("roomlistuser_{}", rand::random::<u32>());
    let register_request = Request::builder()
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

    let response = ServiceExt::<Request<Body>>::oneshot(app.clone(), register_request).await.unwrap();

    assert_eq!(response.status(), StatusCode::OK);
    let body = axum::body::to_bytes(response.into_body(), 1024).await.unwrap();
    let json: Value = serde_json::from_slice(&body).unwrap();
    let user_token = json["access_token"].as_str().unwrap().to_string();

    // 2. 创建多个房间
    let mut room_ids = Vec::new();
    for i in 0..3 {
        let create_room_request = Request::builder()
            .method("POST")
            .uri("/_matrix/client/v3/createRoom")
            .header("Authorization", format!("Bearer {}", user_token))
            .header("Content-Type", "application/json")
            .body(Body::from(
                json!({
                    "name": format!("Bulk Test Room {}", i),
                    "preset": "private_chat"
                })
                .to_string(),
            ))
            .unwrap();

        let response = ServiceExt::<Request<Body>>::oneshot(app.clone(), create_room_request).await.unwrap();

        assert_eq!(response.status(), StatusCode::OK);
        let body = axum::body::to_bytes(response.into_body(), 1024).await.unwrap();
        let json: Value = serde_json::from_slice(&body).unwrap();
        room_ids.push(json["room_id"].as_str().unwrap().to_string());
    }

    // 3. 管理员查询房间列表
    let list_rooms_request = Request::builder()
        .uri("/_synapse/admin/v1/rooms")
        .header("Authorization", format!("Bearer {}", admin_token))
        .body(Body::empty())
        .unwrap();

    let response = ServiceExt::<Request<Body>>::oneshot(app.clone(), list_rooms_request).await.unwrap();

    assert_eq!(response.status(), StatusCode::OK);
    let body = axum::body::to_bytes(response.into_body(), 10240).await.unwrap();
    let json: Value = serde_json::from_slice(&body).unwrap();

    let rooms = json["rooms"].as_array().unwrap();
    assert!(rooms.len() >= 3, "Should return at least 3 rooms");

    // 4. 测试房间搜索（按名称）
    let search_request = Request::builder()
        .uri("/_synapse/admin/v1/rooms/search?search_term=Bulk")
        .header("Authorization", format!("Bearer {}", admin_token))
        .body(Body::empty())
        .unwrap();

    let response = ServiceExt::<Request<Body>>::oneshot(app.clone(), search_request).await.unwrap();

    assert_eq!(response.status(), StatusCode::OK);
    let body = axum::body::to_bytes(response.into_body(), 10240).await.unwrap();
    let json: Value = serde_json::from_slice(&body).unwrap();

    let results = json["results"].as_array().unwrap();
    assert!(results.len() >= 3, "Search should find at least 3 rooms with 'Bulk' in name");

    // 5. 测试分页查询
    let paginated_request = Request::builder()
        .uri("/_synapse/admin/v1/rooms?limit=2")
        .header("Authorization", format!("Bearer {}", admin_token))
        .body(Body::empty())
        .unwrap();

    let response = ServiceExt::<Request<Body>>::oneshot(app.clone(), paginated_request).await.unwrap();

    assert_eq!(response.status(), StatusCode::OK);
    let body = axum::body::to_bytes(response.into_body(), 10240).await.unwrap();
    let json: Value = serde_json::from_slice(&body).unwrap();

    let rooms = json["rooms"].as_array().unwrap();
    assert_eq!(rooms.len(), 2, "Should return exactly 2 rooms with limit=2");
}

/// `POST /_synapse/admin/v1/rooms/{room_id}/backfill` returns 404 for a
/// room that does not exist locally.  This locks the contract that the
/// endpoint validates room existence before attempting any federation
/// traffic.
#[tokio::test]
async fn test_admin_backfill_requires_existing_room() {
    let Some(app) = super::setup_fresh_test_app().await else {
        return;
    };
    let (admin_token, _) = super::get_super_admin_token(&app).await;

    let missing_room_id = format!("!missingbackfill{}:localhost", rand::random::<u32>());
    let encoded_room_id = missing_room_id.replace('!', "%21").replace(':', "%3A");

    let backfill_request = Request::builder()
        .method("POST")
        .uri(format!("/_synapse/admin/v1/rooms/{}/backfill", encoded_room_id))
        .header("Authorization", format!("Bearer {}", admin_token))
        .header("Content-Type", "application/json")
        .body(Body::from(json!({ "limit": 50 }).to_string()))
        .unwrap();

    let response = ServiceExt::<Request<Body>>::oneshot(app, backfill_request).await.unwrap();
    assert_eq!(response.status(), StatusCode::NOT_FOUND);
}

// ============================================================================
// Admin redaction: `events.redacted_by` is a self-referential FK to
// `events.event_id` (`fk_events_redacted_by`), NOT a user id.
//
// Admin cascade/batch redaction are operator actions: no `m.room.redaction`
// event is ever persisted (the admin need not even be a room member, so
// synthesising one would fail room auth). The attribution record is the
// structured `admin.cascade_redact` / `admin.redact_room_events` audit log
// entry, which carries `admin_user_id`. `redacted_by` must therefore be SQL
// NULL — passing `admin.user_id` violated the FK and made the endpoints 500.
// ============================================================================

/// Register a client user, returning `(access_token, user_id)`.
async fn admin_room_register_user(app: &axum::Router, prefix: &str) -> (String, String) {
    let username = format!("{prefix}_{}", rand::random::<u32>());
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
    let response = ServiceExt::<Request<Body>>::oneshot(app.clone(), request).await.unwrap();
    assert_eq!(response.status(), StatusCode::OK, "registration must succeed");
    let body = axum::body::to_bytes(response.into_body(), 4096).await.unwrap();
    let json: Value = serde_json::from_slice(&body).unwrap();
    (
        json["access_token"].as_str().expect("access_token").to_string(),
        json["user_id"].as_str().expect("user_id").to_string(),
    )
}

/// Create a room owned by `token` and return its id.
async fn admin_room_create_room(app: &axum::Router, token: &str, name: &str) -> String {
    let request = Request::builder()
        .method("POST")
        .uri("/_matrix/client/v3/createRoom")
        .header("Authorization", format!("Bearer {token}"))
        .header("Content-Type", "application/json")
        .body(Body::from(json!({ "name": name, "preset": "private_chat" }).to_string()))
        .unwrap();
    let response = ServiceExt::<Request<Body>>::oneshot(app.clone(), request).await.unwrap();
    assert_eq!(response.status(), StatusCode::OK, "createRoom must succeed");
    let body = axum::body::to_bytes(response.into_body(), 4096).await.unwrap();
    let json: Value = serde_json::from_slice(&body).unwrap();
    json["room_id"].as_str().expect("room_id").to_string()
}

/// Send a plain message and return its event id.
async fn admin_room_send_message(app: &axum::Router, token: &str, room_id: &str) -> String {
    let txn = format!("txn_{}", rand::random::<u32>());
    let request = Request::builder()
        .method("PUT")
        .uri(format!("/_matrix/client/v3/rooms/{room_id}/send/m.room.message/{txn}"))
        .header("Authorization", format!("Bearer {token}"))
        .header("Content-Type", "application/json")
        .body(Body::from(json!({ "msgtype": "m.text", "body": "hello" }).to_string()))
        .unwrap();
    let response = ServiceExt::<Request<Body>>::oneshot(app.clone(), request).await.unwrap();
    assert_eq!(response.status(), StatusCode::OK, "send must succeed");
    let body = axum::body::to_bytes(response.into_body(), 4096).await.unwrap();
    let json: Value = serde_json::from_slice(&body).unwrap();
    json["event_id"].as_str().expect("event_id").to_string()
}

/// Add an `m.annotation` child (the cascade target) via the generic send route,
/// so the row lands in `events` where `find_related_events` can see it.
async fn admin_room_send_annotation(app: &axum::Router, token: &str, room_id: &str, target: &str) -> String {
    let txn = format!("txn_annot_{}", rand::random::<u32>());
    let request = Request::builder()
        .method("PUT")
        .uri(format!("/_matrix/client/v3/rooms/{room_id}/send/m.room.message/{txn}"))
        .header("Authorization", format!("Bearer {token}"))
        .header("Content-Type", "application/json")
        .body(Body::from(
            json!({
                "msgtype": "m.text",
                "body": "reaction",
                "m.relates_to": { "rel_type": "m.annotation", "event_id": target, "key": "👍" }
            })
            .to_string(),
        ))
        .unwrap();
    let response = ServiceExt::<Request<Body>>::oneshot(app.clone(), request).await.unwrap();
    assert_eq!(response.status(), StatusCode::OK, "annotation send must succeed");
    let body = axum::body::to_bytes(response.into_body(), 4096).await.unwrap();
    let json: Value = serde_json::from_slice(&body).unwrap();
    json["event_id"].as_str().expect("event_id").to_string()
}

/// `(is_redacted, redacted_by)` for one event.
async fn admin_room_redaction_state(pool: &sqlx::PgPool, event_id: &str) -> (bool, Option<String>) {
    let row = sqlx::query("SELECT is_redacted, redacted_by FROM events WHERE event_id = $1")
        .bind(event_id)
        .fetch_one(pool)
        .await
        .expect("the event must exist");
    (row.get::<bool, _>("is_redacted"), row.get::<Option<String>, _>("redacted_by"))
}

/// `POST /_synapse/admin/v1/rooms/{room_id}/cascade_redact` is an operator
/// action with no `m.room.redaction` event, so `redacted_by` must stay NULL.
///
/// Pre-fix this endpoint passed `admin.user_id` into the self-referential FK and
/// failed with a foreign-key violation (HTTP 500) before redacting anything.
#[tokio::test]
async fn test_admin_cascade_redact_succeeds_and_records_no_redaction_event() {
    let Some((app, pool, _cache)) = super::setup_fresh_test_app_with_pool().await else {
        return;
    };
    let (admin_token, _) = super::get_admin_token(&app).await;
    let (user_token, _user_id) = admin_room_register_user(&app, "admincascade").await;
    let room_id = admin_room_create_room(&app, &user_token, "Admin cascade redact").await;

    let target = admin_room_send_message(&app, &user_token, &room_id).await;
    let child = admin_room_send_annotation(&app, &user_token, &room_id, &target).await;

    let encoded_room_id = room_id.replace('!', "%21").replace(':', "%3A");
    let request = Request::builder()
        .method("POST")
        .uri(format!("/_synapse/admin/v1/rooms/{encoded_room_id}/cascade_redact"))
        .header("Authorization", format!("Bearer {admin_token}"))
        .header("Content-Type", "application/json")
        .body(Body::from(json!({ "event_id": target, "max_depth": 5 }).to_string()))
        .unwrap();
    let response = ServiceExt::<Request<Body>>::oneshot(app.clone(), request).await.unwrap();
    assert_eq!(
        response.status(),
        StatusCode::OK,
        "admin cascade redact must not fail on the events.redacted_by foreign key"
    );
    let body = axum::body::to_bytes(response.into_body(), 4096).await.unwrap();
    let json: Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(json["redacted_count"], json!(2), "target + annotation must both be redacted: {json}");

    let (target_redacted, target_redacted_by) = admin_room_redaction_state(&pool, &target).await;
    let (child_redacted, child_redacted_by) = admin_room_redaction_state(&pool, &child).await;
    assert!(target_redacted, "the cascade root must be redacted");
    assert!(child_redacted, "the cascade target must be redacted");
    assert_eq!(
        target_redacted_by, None,
        "an operator cascade has no m.room.redaction event, so redacted_by must be NULL"
    );
    assert_eq!(
        child_redacted_by, None,
        "an operator cascade has no m.room.redaction event, so redacted_by must be NULL"
    );
}

/// `POST /_matrix/client/v3/admin/room/{room_id}/redact` (batch redact) has the
/// same FK contract as the cascade endpoint: NULL `redacted_by`.
///
/// This Synapse-compat path is RBAC-restricted to `super_admin` (the `admin`
/// role is only allow-listed for `/_synapse/admin/v1/rooms…`), hence the
/// super-admin token.
#[tokio::test]
async fn test_admin_batch_redact_succeeds_and_records_no_redaction_event() {
    let Some((app, pool, _cache)) = super::setup_fresh_test_app_with_pool().await else {
        return;
    };
    let (admin_token, _) = super::get_super_admin_token(&app).await;
    let (user_token, _user_id) = admin_room_register_user(&app, "adminbatch").await;
    let room_id = admin_room_create_room(&app, &user_token, "Admin batch redact").await;

    let first = admin_room_send_message(&app, &user_token, &room_id).await;
    let second = admin_room_send_message(&app, &user_token, &room_id).await;

    // `before_ts` in the future selects both messages (and only them: the room
    // create/member events are excluded by the query).
    let encoded_room_id = room_id.replace('!', "%21").replace(':', "%3A");
    let request = Request::builder()
        .method("POST")
        .uri(format!("/_matrix/client/v3/admin/room/{encoded_room_id}/redact"))
        .header("Authorization", format!("Bearer {admin_token}"))
        .header("Content-Type", "application/json")
        .body(Body::from(json!({ "before_ts": current_timestamp_millis() + 60_000 }).to_string()))
        .unwrap();
    let response = ServiceExt::<Request<Body>>::oneshot(app.clone(), request).await.unwrap();
    let status = response.status();
    let body = axum::body::to_bytes(response.into_body(), 4096).await.unwrap();
    assert_eq!(
        status,
        StatusCode::OK,
        "admin batch redact must not fail on the events.redacted_by foreign key: {}",
        String::from_utf8_lossy(&body)
    );
    let json: Value = serde_json::from_slice(&body).unwrap();
    // The endpoint redacts every non-`m.room.create` event in the window, so the
    // exact count varies (room state/member events are included); what matters is
    // that both messages are among them.
    let redacted = json["redacted"].as_u64().expect("`redacted` must be a count");
    assert!(redacted >= 2, "both messages must be redacted, got: {json}");

    for event_id in [&first, &second] {
        let (redacted, redacted_by) = admin_room_redaction_state(&pool, event_id).await;
        assert!(redacted, "the batch target {event_id} must be redacted");
        assert_eq!(
            redacted_by, None,
            "an operator batch redaction has no m.room.redaction event, so redacted_by must be NULL"
        );
    }
}
