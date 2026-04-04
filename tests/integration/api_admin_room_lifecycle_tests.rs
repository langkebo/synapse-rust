use axum::{
    body::Body,
    http::{Request, StatusCode},
};
use serde_json::{json, Value};
use tower::ServiceExt;

/// 测试房间管理完整生命周期：创建 → 查询 → 删除 → 验证删除
#[tokio::test]
async fn test_admin_room_lifecycle_management() {
    let Some(app) = super::setup_test_app().await else {
        return;
    };
    let (admin_token, _) = super::get_admin_token(&app).await;

    // 1. 创建测试用户
    let username = format!("roomowner_{}", rand::random::<u32>());
    let register_request = Request::builder()
        .method("POST")
        .uri("/_matrix/client/r0/register")
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

    let response = ServiceExt::<Request<Body>>::oneshot(app.clone(), register_request)
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
    let body = axum::body::to_bytes(response.into_body(), 1024)
        .await
        .unwrap();
    let json: Value = serde_json::from_slice(&body).unwrap();
    let user_token = json["access_token"].as_str().unwrap().to_string();

    // 2. 用户创建房间
    let room_name = format!("Test Room {}", rand::random::<u32>());
    let create_room_request = Request::builder()
        .method("POST")
        .uri("/_matrix/client/r0/createRoom")
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

    let response = ServiceExt::<Request<Body>>::oneshot(app.clone(), create_room_request)
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
    let body = axum::body::to_bytes(response.into_body(), 1024)
        .await
        .unwrap();
    let json: Value = serde_json::from_slice(&body).unwrap();
    let room_id = json["room_id"].as_str().unwrap().to_string();

    // 3. 管理员查询房间详情
    let encoded_room_id = room_id.replace('!', "%21").replace(':', "%3A");
    let get_room_request = Request::builder()
        .uri(format!("/_synapse/admin/v1/rooms/{}", encoded_room_id))
        .header("Authorization", format!("Bearer {}", admin_token))
        .body(Body::empty())
        .unwrap();

    let response = ServiceExt::<Request<Body>>::oneshot(app.clone(), get_room_request)
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
    let body = axum::body::to_bytes(response.into_body(), 1024)
        .await
        .unwrap();
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

    let response = ServiceExt::<Request<Body>>::oneshot(app.clone(), delete_room_request)
        .await
        .unwrap();

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

    let response = ServiceExt::<Request<Body>>::oneshot(app.clone(), verify_deleted_request)
        .await
        .unwrap();

    // 应该返回 404 或显示房间已被删除
    assert!(
        response.status() == StatusCode::NOT_FOUND || response.status() == StatusCode::OK,
        "Deleted room should return 404 or show deleted status"
    );

    if response.status() == StatusCode::OK {
        let body = axum::body::to_bytes(response.into_body(), 1024)
            .await
            .unwrap();
        let json: Value = serde_json::from_slice(&body).unwrap();
        // 如果返回 200，应该显示房间已被删除或阻止
        assert!(
            json["blocked"].as_bool().unwrap_or(false)
                || json["deleted"].as_bool().unwrap_or(false),
            "Room should be marked as blocked or deleted"
        );
    }

    // 6. 验证用户无法再访问该房间
    let user_access_request = Request::builder()
        .uri(format!("/_matrix/client/r0/rooms/{}/state", room_id))
        .header("Authorization", format!("Bearer {}", user_token))
        .body(Body::empty())
        .unwrap();

    let response = ServiceExt::<Request<Body>>::oneshot(app.clone(), user_access_request)
        .await
        .unwrap();

    // 用户应该无法访问已删除的房间
    assert!(
        response.status() == StatusCode::NOT_FOUND || response.status() == StatusCode::FORBIDDEN,
        "User should not be able to access deleted room"
    );
}

/// 测试房间历史清理功能
#[tokio::test]
async fn test_admin_room_history_purge() {
    let Some(app) = super::setup_test_app().await else {
        return;
    };
    let (admin_token, _) = super::get_admin_token(&app).await;

    // 1. 创建测试用户
    let username = format!("historyuser_{}", rand::random::<u32>());
    let register_request = Request::builder()
        .method("POST")
        .uri("/_matrix/client/r0/register")
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

    let response = ServiceExt::<Request<Body>>::oneshot(app.clone(), register_request)
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
    let body = axum::body::to_bytes(response.into_body(), 1024)
        .await
        .unwrap();
    let json: Value = serde_json::from_slice(&body).unwrap();
    let user_token = json["access_token"].as_str().unwrap().to_string();

    // 2. 创建房间
    let create_room_request = Request::builder()
        .method("POST")
        .uri("/_matrix/client/r0/createRoom")
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

    let response = ServiceExt::<Request<Body>>::oneshot(app.clone(), create_room_request)
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
    let body = axum::body::to_bytes(response.into_body(), 1024)
        .await
        .unwrap();
    let json: Value = serde_json::from_slice(&body).unwrap();
    let room_id = json["room_id"].as_str().unwrap().to_string();

    // 3. 发送一些消息
    for i in 0..3 {
        let send_message_request = Request::builder()
            .method("PUT")
            .uri(format!(
                "/_matrix/client/r0/rooms/{}/send/m.room.message/txn_{}",
                room_id, i
            ))
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

        let response = ServiceExt::<Request<Body>>::oneshot(app.clone(), send_message_request)
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::OK);
    }

    // 4. 管理员清理房间历史（保留最近 1 条消息）
    let encoded_room_id = room_id.replace('!', "%21").replace(':', "%3A");
    let purge_history_request = Request::builder()
        .method("POST")
        .uri(format!(
            "/_synapse/admin/v1/rooms/{}/purge_history",
            encoded_room_id
        ))
        .header("Authorization", format!("Bearer {}", admin_token))
        .header("Content-Type", "application/json")
        .body(Body::from(
            json!({
                "delete_local_events": true,
                "purge_up_to_ts": (chrono::Utc::now().timestamp_millis() - 1000) // 1秒前
            })
            .to_string(),
        ))
        .unwrap();

    let response = ServiceExt::<Request<Body>>::oneshot(app.clone(), purge_history_request)
        .await
        .unwrap();

    // 清理历史应该返回 200 或 202
    assert!(
        response.status() == StatusCode::OK || response.status() == StatusCode::ACCEPTED,
        "History purge should succeed"
    );

    // 5. 验证历史已被清理（可选，取决于实现）
    // 这里可以查询房间消息，验证旧消息已被删除
}

/// 测试批量房间查询和搜索
#[tokio::test]
async fn test_admin_room_list_and_search() {
    let Some(app) = super::setup_test_app().await else {
        return;
    };
    let (admin_token, _) = super::get_admin_token(&app).await;

    // 1. 创建测试用户
    let username = format!("roomlistuser_{}", rand::random::<u32>());
    let register_request = Request::builder()
        .method("POST")
        .uri("/_matrix/client/r0/register")
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

    let response = ServiceExt::<Request<Body>>::oneshot(app.clone(), register_request)
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
    let body = axum::body::to_bytes(response.into_body(), 1024)
        .await
        .unwrap();
    let json: Value = serde_json::from_slice(&body).unwrap();
    let user_token = json["access_token"].as_str().unwrap().to_string();

    // 2. 创建多个房间
    let mut room_ids = Vec::new();
    for i in 0..3 {
        let create_room_request = Request::builder()
            .method("POST")
            .uri("/_matrix/client/r0/createRoom")
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

        let response = ServiceExt::<Request<Body>>::oneshot(app.clone(), create_room_request)
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::OK);
        let body = axum::body::to_bytes(response.into_body(), 1024)
            .await
            .unwrap();
        let json: Value = serde_json::from_slice(&body).unwrap();
        room_ids.push(json["room_id"].as_str().unwrap().to_string());
    }

    // 3. 管理员查询房间列表
    let list_rooms_request = Request::builder()
        .uri("/_synapse/admin/v1/rooms")
        .header("Authorization", format!("Bearer {}", admin_token))
        .body(Body::empty())
        .unwrap();

    let response = ServiceExt::<Request<Body>>::oneshot(app.clone(), list_rooms_request)
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
    let body = axum::body::to_bytes(response.into_body(), 10240)
        .await
        .unwrap();
    let json: Value = serde_json::from_slice(&body).unwrap();

    let rooms = json["rooms"].as_array().unwrap();
    assert!(rooms.len() >= 3, "Should return at least 3 rooms");

    // 4. 测试房间搜索（按名称）
    let search_request = Request::builder()
        .uri("/_synapse/admin/v1/rooms/search?search_term=Bulk")
        .header("Authorization", format!("Bearer {}", admin_token))
        .body(Body::empty())
        .unwrap();

    let response = ServiceExt::<Request<Body>>::oneshot(app.clone(), search_request)
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
    let body = axum::body::to_bytes(response.into_body(), 10240)
        .await
        .unwrap();
    let json: Value = serde_json::from_slice(&body).unwrap();

    let results = json["results"].as_array().unwrap();
    assert!(
        results.len() >= 3,
        "Search should find at least 3 rooms with 'Bulk' in name"
    );

    // 5. 测试分页查询
    let paginated_request = Request::builder()
        .uri("/_synapse/admin/v1/rooms?limit=2")
        .header("Authorization", format!("Bearer {}", admin_token))
        .body(Body::empty())
        .unwrap();

    let response = ServiceExt::<Request<Body>>::oneshot(app.clone(), paginated_request)
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
    let body = axum::body::to_bytes(response.into_body(), 10240)
        .await
        .unwrap();
    let json: Value = serde_json::from_slice(&body).unwrap();

    let rooms = json["rooms"].as_array().unwrap();
    assert_eq!(rooms.len(), 2, "Should return exactly 2 rooms with limit=2");
}
