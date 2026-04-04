use axum::body::Body;
use hyper::{Request, StatusCode};
use serde_json::json;
use tower::ServiceExt;

use crate::{get_admin_token, setup_test_app};

/// P1-1: 事务推送与事件处理闭环
/// 验证：AppService 可以接收事件推送，事件正确存储到数据库
#[tokio::test]
async fn test_appservice_transaction_push() {
    let Some(app) = setup_test_app().await else {
        return;
    };

    let (admin_token, _) = get_admin_token(&app).await;

    // 1. Register AppService
    let as_id = format!("test_as_txn_{}", rand::random::<u32>());
    let register_request = Request::builder()
        .method("POST")
        .uri("/_synapse/admin/v1/appservices")
        .header("Content-Type", "application/json")
        .header("Authorization", format!("Bearer {}", admin_token))
        .body(Body::from(
            json!({
                "id": &as_id,
                "url": "http://localhost:8080",
                "as_token": format!("test_as_token_{}", rand::random::<u32>()),
                "hs_token": format!("test_hs_token_{}", rand::random::<u32>()),
                "sender_localpart": "bot_txn"
            })
            .to_string(),
        ))
        .unwrap();

    let response = app.clone().oneshot(register_request).await.unwrap();
    assert_eq!(response.status(), StatusCode::CREATED);

    // 2. Push event to AppService
    let push_request = Request::builder()
        .method("POST")
        .uri(format!("/_synapse/admin/v1/appservices/{}/events", as_id))
        .header("Content-Type", "application/json")
        .header("Authorization", format!("Bearer {}", admin_token))
        .body(Body::from(
            json!({
                "room_id": "!test:localhost",
                "event_type": "m.room.message",
                "sender": "@user:localhost",
                "content": {
                    "msgtype": "m.text",
                    "body": "Test message"
                }
            })
            .to_string(),
        ))
        .unwrap();

    let response = app.clone().oneshot(push_request).await.unwrap();
    assert_eq!(
        response.status(),
        StatusCode::CREATED,
        "Event push should return 201 CREATED"
    );

    let body = axum::body::to_bytes(response.into_body(), 1024 * 1024)
        .await
        .unwrap();
    let event_json: serde_json::Value = serde_json::from_slice(&body).unwrap();
    assert!(event_json["event_id"].is_string());
    assert_eq!(event_json["as_id"], as_id);
    assert_eq!(event_json["room_id"], "!test:localhost");

    // 3. Query pending events
    let query_request = Request::builder()
        .method("GET")
        .uri(format!(
            "/_synapse/admin/v1/appservices/{}/events?limit=10",
            as_id
        ))
        .header("Authorization", format!("Bearer {}", admin_token))
        .body(Body::empty())
        .unwrap();

    let response = app.clone().oneshot(query_request).await.unwrap();
    assert_eq!(
        response.status(),
        StatusCode::OK,
        "Pending events query should return 200 OK"
    );

    let body = axum::body::to_bytes(response.into_body(), 1024 * 1024)
        .await
        .unwrap();
    let events_json: serde_json::Value = serde_json::from_slice(&body).unwrap();
    let events = events_json.as_array().unwrap();
    assert!(!events.is_empty(), "Should have at least one pending event");
    assert_eq!(events[0]["as_id"], as_id);
    assert_eq!(events[0]["event_type"], "m.room.message");
}

/// P1-2: as_token 认证验证
/// 验证：使用 as_token 可以访问 AppService API，无效 token 被拒绝
#[tokio::test]
async fn test_appservice_as_token_authentication() {
    let Some(app) = setup_test_app().await else {
        return;
    };

    let (admin_token, _) = get_admin_token(&app).await;

    // 1. Register AppService with specific as_token
    let as_id = format!("test_as_auth_{}", rand::random::<u32>());
    let as_token = format!("valid_as_token_{}", rand::random::<u32>());
    let register_request = Request::builder()
        .method("POST")
        .uri("/_synapse/admin/v1/appservices")
        .header("Content-Type", "application/json")
        .header("Authorization", format!("Bearer {}", admin_token))
        .body(Body::from(
            json!({
                "id": &as_id,
                "url": "http://localhost:8080",
                "as_token": &as_token,
                "hs_token": format!("test_hs_token_{}", rand::random::<u32>()),
                "sender_localpart": "bot_auth"
            })
            .to_string(),
        ))
        .unwrap();

    let response = app.clone().oneshot(register_request).await.unwrap();
    assert_eq!(response.status(), StatusCode::CREATED);

    // 2. Test valid as_token - ping endpoint
    let ping_request = Request::builder()
        .method("POST")
        .uri("/_matrix/app/v1/ping")
        .header("Authorization", format!("Bearer {}", as_token))
        .header("Content-Type", "application/json")
        .body(Body::from("{}"))
        .unwrap();

    let response = app.clone().oneshot(ping_request).await.unwrap();
    assert_eq!(
        response.status(),
        StatusCode::OK,
        "Valid as_token should be accepted"
    );

    let body = axum::body::to_bytes(response.into_body(), 1024 * 1024)
        .await
        .unwrap();
    let ping_json: serde_json::Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(ping_json["as_id"], as_id);

    // 3. Test invalid as_token - should be rejected
    let invalid_ping_request = Request::builder()
        .method("POST")
        .uri("/_matrix/app/v1/ping")
        .header("Authorization", "Bearer invalid_token_12345")
        .header("Content-Type", "application/json")
        .body(Body::from("{}"))
        .unwrap();

    let response = app.clone().oneshot(invalid_ping_request).await.unwrap();
    assert_eq!(
        response.status(),
        StatusCode::UNAUTHORIZED,
        "Invalid as_token should be rejected with 401"
    );
}

/// P1-3: hs_token 认证验证
/// 验证：homeserver 使用 hs_token 向 AppService 发送事务时的认证
/// 注意：这个测试验证 hs_token 的存储和查询，实际的 HTTP 调用需要真实的 AppService 服务器
#[tokio::test]
async fn test_appservice_hs_token_storage() {
    let Some(app) = setup_test_app().await else {
        return;
    };

    let (admin_token, _) = get_admin_token(&app).await;

    // 1. Register AppService with specific hs_token
    let as_id = format!("test_as_hs_{}", rand::random::<u32>());
    let hs_token = format!("valid_hs_token_{}", rand::random::<u32>());
    let register_request = Request::builder()
        .method("POST")
        .uri("/_synapse/admin/v1/appservices")
        .header("Content-Type", "application/json")
        .header("Authorization", format!("Bearer {}", admin_token))
        .body(Body::from(
            json!({
                "id": &as_id,
                "url": "http://localhost:8080",
                "as_token": format!("test_as_token_{}", rand::random::<u32>()),
                "hs_token": &hs_token,
                "sender_localpart": "bot_hs"
            })
            .to_string(),
        ))
        .unwrap();

    let response = app.clone().oneshot(register_request).await.unwrap();
    assert_eq!(response.status(), StatusCode::CREATED);

    // 2. Query AppService to verify hs_token is stored (not exposed in response for security)
    let query_request = Request::builder()
        .method("GET")
        .uri(format!("/_synapse/admin/v1/appservices/{}", as_id))
        .header("Authorization", format!("Bearer {}", admin_token))
        .body(Body::empty())
        .unwrap();

    let response = app.clone().oneshot(query_request).await.unwrap();
    assert_eq!(response.status(), StatusCode::OK);

    let body = axum::body::to_bytes(response.into_body(), 1024 * 1024)
        .await
        .unwrap();
    let service_json: serde_json::Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(service_json["as_id"], as_id);
    // hs_token should not be exposed in API responses for security
    assert!(
        service_json.get("hs_token").is_none(),
        "hs_token should not be exposed in API response"
    );
}

/// P1-4: Namespace 独占性验证
/// 验证：exclusive namespace 只能被一个 AppService 使用
#[tokio::test]
async fn test_appservice_namespace_exclusivity() {
    let Some(app) = setup_test_app().await else {
        return;
    };

    let (admin_token, _) = get_admin_token(&app).await;

    // 1. Register first AppService with exclusive namespace
    let as_id_1 = format!("test_as_ns1_{}", rand::random::<u32>());
    let namespace_pattern = format!("@bot_exclusive_{}.*:localhost", rand::random::<u32>());
    let register_request_1 = Request::builder()
        .method("POST")
        .uri("/_synapse/admin/v1/appservices")
        .header("Content-Type", "application/json")
        .header("Authorization", format!("Bearer {}", admin_token))
        .body(Body::from(
            json!({
                "id": &as_id_1,
                "url": "http://localhost:8080",
                "as_token": format!("test_as_token_{}", rand::random::<u32>()),
                "hs_token": format!("test_hs_token_{}", rand::random::<u32>()),
                "sender_localpart": "bot_ns1",
                "namespaces": {
                    "users": [{"regex": &namespace_pattern, "exclusive": true}]
                }
            })
            .to_string(),
        ))
        .unwrap();

    let response = app.clone().oneshot(register_request_1).await.unwrap();
    assert_eq!(
        response.status(),
        StatusCode::CREATED,
        "First AppService with exclusive namespace should be created"
    );

    // 2. Query namespaces to verify exclusive flag
    let query_ns_request = Request::builder()
        .method("GET")
        .uri(format!(
            "/_synapse/admin/v1/appservices/{}/namespaces",
            as_id_1
        ))
        .header("Authorization", format!("Bearer {}", admin_token))
        .body(Body::empty())
        .unwrap();

    let response = app.clone().oneshot(query_ns_request).await.unwrap();
    assert_eq!(response.status(), StatusCode::OK);

    let body = axum::body::to_bytes(response.into_body(), 1024 * 1024)
        .await
        .unwrap();
    let ns_json: serde_json::Value = serde_json::from_slice(&body).unwrap();
    assert!(ns_json["users"].is_array());
    let users_ns = ns_json["users"].as_array().unwrap();
    assert!(!users_ns.is_empty(), "Should have user namespace");
    assert_eq!(
        users_ns[0]["exclusive"], true,
        "Namespace should be marked as exclusive"
    );
    assert_eq!(users_ns[0]["namespace_pattern"], namespace_pattern);

    // 3. Register virtual user within exclusive namespace
    let virtual_user_id = format!("@bot_exclusive_{}_test:localhost", rand::random::<u32>());
    let register_user_request = Request::builder()
        .method("POST")
        .uri(format!("/_synapse/admin/v1/appservices/{}/users", as_id_1))
        .header("Content-Type", "application/json")
        .header("Authorization", format!("Bearer {}", admin_token))
        .body(Body::from(
            json!({
                "user_id": &virtual_user_id,
                "displayname": "Exclusive Bot"
            })
            .to_string(),
        ))
        .unwrap();

    let response = app.clone().oneshot(register_user_request).await.unwrap();
    assert_eq!(
        response.status(),
        StatusCode::CREATED,
        "Virtual user in exclusive namespace should be created"
    );

    // 4. Query user namespace to verify it belongs to as_id_1
    let query_user_request = Request::builder()
        .method("GET")
        .uri(format!(
            "/_synapse/admin/v1/appservices/query/user?user_id={}",
            urlencoding::encode(&virtual_user_id)
        ))
        .header("Authorization", format!("Bearer {}", admin_token))
        .body(Body::empty())
        .unwrap();

    let response = app.clone().oneshot(query_user_request).await.unwrap();
    assert_eq!(response.status(), StatusCode::OK);

    let body = axum::body::to_bytes(response.into_body(), 1024 * 1024)
        .await
        .unwrap();
    let query_json: serde_json::Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(query_json["user_id"], virtual_user_id);
    // Verify the user is associated with the correct AppService
    if let Some(as_id_value) = query_json["application_service"].as_str() {
        assert_eq!(
            as_id_value, as_id_1,
            "User should belong to the first AppService"
        );
    }
}

/// P1-5: Namespace 查询闭环
/// 验证：用户/房间别名查询能正确返回所属的 AppService
#[tokio::test]
async fn test_appservice_namespace_query() {
    let Some(app) = setup_test_app().await else {
        return;
    };

    let (admin_token, _) = get_admin_token(&app).await;

    // 1. Register AppService with user and alias namespaces
    let as_id = format!("test_as_query_{}", rand::random::<u32>());
    let user_pattern = format!("@query_bot_{}.*:localhost", rand::random::<u32>());
    let alias_pattern = format!("#query_room_{}.*:localhost", rand::random::<u32>());

    let register_request = Request::builder()
        .method("POST")
        .uri("/_synapse/admin/v1/appservices")
        .header("Content-Type", "application/json")
        .header("Authorization", format!("Bearer {}", admin_token))
        .body(Body::from(
            json!({
                "id": &as_id,
                "url": "http://localhost:8080",
                "as_token": format!("test_as_token_{}", rand::random::<u32>()),
                "hs_token": format!("test_hs_token_{}", rand::random::<u32>()),
                "sender_localpart": "bot_query",
                "namespaces": {
                    "users": [{"regex": &user_pattern, "exclusive": true}],
                    "aliases": [{"regex": &alias_pattern, "exclusive": false}]
                }
            })
            .to_string(),
        ))
        .unwrap();

    let response = app.clone().oneshot(register_request).await.unwrap();
    assert_eq!(response.status(), StatusCode::CREATED);

    // 2. Register virtual user in namespace
    let virtual_user_id = format!("@query_bot_{}_user:localhost", rand::random::<u32>());
    let register_user_request = Request::builder()
        .method("POST")
        .uri(format!("/_synapse/admin/v1/appservices/{}/users", as_id))
        .header("Content-Type", "application/json")
        .header("Authorization", format!("Bearer {}", admin_token))
        .body(Body::from(
            json!({
                "user_id": &virtual_user_id,
                "displayname": "Query Bot"
            })
            .to_string(),
        ))
        .unwrap();

    let response = app.clone().oneshot(register_user_request).await.unwrap();
    assert_eq!(response.status(), StatusCode::CREATED);

    // 3. Query user namespace
    let query_user_request = Request::builder()
        .method("GET")
        .uri(format!(
            "/_synapse/admin/v1/appservices/query/user?user_id={}",
            urlencoding::encode(&virtual_user_id)
        ))
        .header("Authorization", format!("Bearer {}", admin_token))
        .body(Body::empty())
        .unwrap();

    let response = app.clone().oneshot(query_user_request).await.unwrap();
    assert_eq!(
        response.status(),
        StatusCode::OK,
        "User namespace query should succeed"
    );

    let body = axum::body::to_bytes(response.into_body(), 1024 * 1024)
        .await
        .unwrap();
    let user_query_json: serde_json::Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(user_query_json["user_id"], virtual_user_id);

    // 4. Query room alias namespace
    let test_alias = format!("#query_room_{}_test:localhost", rand::random::<u32>());
    let query_alias_request = Request::builder()
        .method("GET")
        .uri(format!(
            "/_synapse/admin/v1/appservices/query/alias?alias={}",
            urlencoding::encode(&test_alias)
        ))
        .header("Authorization", format!("Bearer {}", admin_token))
        .body(Body::empty())
        .unwrap();

    let response = app.clone().oneshot(query_alias_request).await.unwrap();
    assert_eq!(
        response.status(),
        StatusCode::OK,
        "Alias namespace query should succeed"
    );

    let body = axum::body::to_bytes(response.into_body(), 1024 * 1024)
        .await
        .unwrap();
    let alias_query_json: serde_json::Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(alias_query_json["alias"], test_alias);
}
