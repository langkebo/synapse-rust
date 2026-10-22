use axum::body::Body;
use hyper::{Request, StatusCode};
use serde_json::json;
use tower::ServiceExt;

use crate::{get_super_admin_token, setup_fresh_test_app};

/// P1-1: 事务推送与事件处理闭环
/// 验证：AppService 可以接收事件推送，事件正确存储到数据库
#[tokio::test]
async fn test_appservice_transaction_push() {
    let Some(app) = setup_fresh_test_app().await else {
        return;
    };

    let (admin_token, _) = get_super_admin_token(&app).await;

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
                "sender_localpart": "bot_txn",
                "namespaces": {
                    "users": [{"regex": "^@user:localhost$", "exclusive": false}],
                    "aliases": [],
                    "rooms": []
                }
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
    assert_eq!(response.status(), StatusCode::CREATED, "Event push should return 201 CREATED");

    let body = axum::body::to_bytes(response.into_body(), 1024 * 1024).await.unwrap();
    let event_json: serde_json::Value = serde_json::from_slice(&body).unwrap();
    assert!(event_json["event_id"].is_string());
    assert_eq!(event_json["as_id"], as_id);
    assert_eq!(event_json["room_id"], "!test:localhost");

    // 3. Query pending events
    let query_request = Request::builder()
        .method("GET")
        .uri(format!("/_synapse/admin/v1/appservices/{}/events?limit=10", as_id))
        .header("Authorization", format!("Bearer {}", admin_token))
        .body(Body::empty())
        .unwrap();

    let response = app.clone().oneshot(query_request).await.unwrap();
    assert_eq!(response.status(), StatusCode::OK, "Pending events query should return 200 OK");

    let body = axum::body::to_bytes(response.into_body(), 1024 * 1024).await.unwrap();
    let events_json: serde_json::Value = serde_json::from_slice(&body).unwrap();
    let events = events_json["events"].as_array().unwrap();
    assert!(!events.is_empty(), "Should have at least one pending event");
    assert_eq!(events[0]["as_id"], as_id);
    assert_eq!(events[0]["event_type"], "m.room.message");
    assert!(events_json["next_token"].is_null(), "a page shorter than the limit has no next_token");
}

/// P1-2: as_token 认证验证
/// 验证：使用 as_token 可以访问 AppService API，无效 token 被拒绝
#[tokio::test]
async fn test_appservice_as_token_authentication() {
    let Some(app) = setup_fresh_test_app().await else {
        return;
    };

    let (admin_token, _) = get_super_admin_token(&app).await;

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
    assert_eq!(response.status(), StatusCode::OK, "Valid as_token should be accepted");

    let body = axum::body::to_bytes(response.into_body(), 1024 * 1024).await.unwrap();
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
    assert_eq!(response.status(), StatusCode::UNAUTHORIZED, "Invalid as_token should be rejected with 401");
}

/// P1-3: hs_token 认证验证
/// 验证：homeserver 使用 hs_token 向 AppService 发送事务时的认证
/// 注意：这个测试验证 hs_token 的存储和查询，实际的 HTTP 调用需要真实的 AppService 服务器
#[tokio::test]
async fn test_appservice_hs_token_storage() {
    let Some(app) = setup_fresh_test_app().await else {
        return;
    };

    let (admin_token, _) = get_super_admin_token(&app).await;

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

    let body = axum::body::to_bytes(response.into_body(), 1024 * 1024).await.unwrap();
    let service_json: serde_json::Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(service_json["as_id"], as_id);
    // hs_token should not be exposed in API responses for security
    assert!(service_json.get("hs_token").is_none(), "hs_token should not be exposed in API response");
}

/// P1-4: Namespace 独占性验证
/// 验证：exclusive namespace 只能被一个 AppService 使用
#[tokio::test]
async fn test_appservice_namespace_exclusivity() {
    let Some(app) = setup_fresh_test_app().await else {
        return;
    };

    let (admin_token, _) = get_super_admin_token(&app).await;

    // 1. Register first AppService with exclusive namespace
    let as_id_1 = format!("test_as_ns1_{}", rand::random::<u32>());
    let exclusive_prefix = rand::random::<u32>();
    let namespace_pattern = format!("@bot_exclusive_{}.*:localhost", exclusive_prefix);
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
    assert_eq!(response.status(), StatusCode::CREATED, "First AppService with exclusive namespace should be created");

    let as_id_2 = format!("test_as_ns2_{}", rand::random::<u32>());
    let register_request_2 = Request::builder()
        .method("POST")
        .uri("/_synapse/admin/v1/appservices")
        .header("Content-Type", "application/json")
        .header("Authorization", format!("Bearer {}", admin_token))
        .body(Body::from(
            json!({
                "id": &as_id_2,
                "url": "http://localhost:8081",
                "as_token": format!("test_as_token_{}", rand::random::<u32>()),
                "hs_token": format!("test_hs_token_{}", rand::random::<u32>()),
                "sender_localpart": "bot_ns2",
                "namespaces": {
                    "users": [{"regex": &namespace_pattern, "exclusive": true}]
                }
            })
            .to_string(),
        ))
        .unwrap();

    let response = app.clone().oneshot(register_request_2).await.unwrap();
    assert_eq!(response.status(), StatusCode::CONFLICT, "Duplicate exclusive namespace should be rejected");

    // 2. Query namespaces to verify exclusive flag
    let query_ns_request = Request::builder()
        .method("GET")
        .uri(format!("/_synapse/admin/v1/appservices/{}/namespaces", as_id_1))
        .header("Authorization", format!("Bearer {}", admin_token))
        .body(Body::empty())
        .unwrap();

    let response = app.clone().oneshot(query_ns_request).await.unwrap();
    assert_eq!(response.status(), StatusCode::OK);

    let body = axum::body::to_bytes(response.into_body(), 1024 * 1024).await.unwrap();
    let ns_json: serde_json::Value = serde_json::from_slice(&body).unwrap();
    assert!(ns_json["users"].is_array());
    let users_ns = ns_json["users"].as_array().unwrap();
    assert!(!users_ns.is_empty(), "Should have user namespace");
    assert_eq!(users_ns[0]["is_exclusive"], true, "Namespace should be marked as exclusive");
    assert_eq!(users_ns[0]["namespace_pattern"], namespace_pattern);

    // 3. Register virtual user within exclusive namespace
    let virtual_user_id = format!("@bot_exclusive_{}_test:localhost", exclusive_prefix);
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
    assert_eq!(response.status(), StatusCode::CREATED, "Virtual user in exclusive namespace should be created");

    // 4. Query user namespace to verify it belongs to as_id_1
    let query_user_request = Request::builder()
        .method("GET")
        .uri(format!("/_synapse/admin/v1/appservices/query/user?user_id={}", urlencoding::encode(&virtual_user_id)))
        .header("Authorization", format!("Bearer {}", admin_token))
        .body(Body::empty())
        .unwrap();

    let response = app.clone().oneshot(query_user_request).await.unwrap();
    assert_eq!(response.status(), StatusCode::OK);

    let body = axum::body::to_bytes(response.into_body(), 1024 * 1024).await.unwrap();
    let query_json: serde_json::Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(query_json["user_id"], virtual_user_id);
    // Verify the user is associated with the correct AppService
    if let Some(as_id_value) = query_json["application_service"].as_str() {
        assert_eq!(as_id_value, as_id_1, "User should belong to the first AppService");
    }
}

/// P1-4b: virtual user 注册必须落在本地 exclusive user namespace 内
#[tokio::test]
async fn test_appservice_virtual_user_requires_exclusive_local_namespace() {
    let Some(app) = setup_fresh_test_app().await else {
        return;
    };

    let (admin_token, _) = get_super_admin_token(&app).await;
    let as_id = format!("test_as_virtual_guard_{}", rand::random::<u32>());
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
                "sender_localpart": "bot_guard",
                "namespaces": {
                    "users": [{"regex": "^@guarded_bot_[0-9]+:localhost$", "exclusive": true}],
                    "aliases": [],
                    "rooms": []
                }
            })
            .to_string(),
        ))
        .unwrap();

    let response = app.clone().oneshot(register_request).await.unwrap();
    assert_eq!(response.status(), StatusCode::CREATED);

    let outside_namespace_request = Request::builder()
        .method("POST")
        .uri(format!("/_synapse/admin/v1/appservices/{}/users", as_id))
        .header("Content-Type", "application/json")
        .header("Authorization", format!("Bearer {}", admin_token))
        .body(Body::from(
            json!({
                "user_id": format!("@outside_bot_{}:localhost", rand::random::<u32>()),
                "displayname": "Outside Bot"
            })
            .to_string(),
        ))
        .unwrap();

    let response = app.clone().oneshot(outside_namespace_request).await.unwrap();
    assert_eq!(response.status(), StatusCode::FORBIDDEN, "Virtual user outside exclusive namespace should be rejected");

    let foreign_domain_request = Request::builder()
        .method("POST")
        .uri(format!("/_synapse/admin/v1/appservices/{}/users", as_id))
        .header("Content-Type", "application/json")
        .header("Authorization", format!("Bearer {}", admin_token))
        .body(Body::from(
            json!({
                "user_id": format!("@guarded_bot_{}:example.com", rand::random::<u32>()),
                "displayname": "Foreign Bot"
            })
            .to_string(),
        ))
        .unwrap();

    let response = app.oneshot(foreign_domain_request).await.unwrap();
    assert_eq!(response.status(), StatusCode::BAD_REQUEST, "Foreign-domain virtual user should be rejected");
}

/// P1-4c: 管理面显式写入事件必须落在 AppService namespace 内
#[tokio::test]
async fn test_appservice_admin_push_event_requires_namespace_ownership() {
    let Some(app) = setup_fresh_test_app().await else {
        return;
    };

    // 2026-10-21：`POST /appservices` 与 `…/events` 等同后门，已收紧到 super_admin。
    let (admin_token, _) = get_super_admin_token(&app).await;
    let as_id = format!("test_as_push_guard_{}", rand::random::<u32>());
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
                "sender_localpart": "bot_push_guard",
                "namespaces": {
                    "users": [{"regex": "^@owned_sender:localhost$", "exclusive": false}],
                    "aliases": [],
                    "rooms": [{"regex": "^!owned_room:localhost$", "exclusive": true}]
                }
            })
            .to_string(),
        ))
        .unwrap();

    let response = app.clone().oneshot(register_request).await.unwrap();
    assert_eq!(response.status(), StatusCode::CREATED);

    let forbidden_push_request = Request::builder()
        .method("POST")
        .uri(format!("/_synapse/admin/v1/appservices/{}/events", as_id))
        .header("Content-Type", "application/json")
        .header("Authorization", format!("Bearer {}", admin_token))
        .body(Body::from(
            json!({
                "room_id": "!foreign_room:localhost",
                "event_type": "m.room.message",
                "sender": "@foreign_sender:localhost",
                "content": {
                    "msgtype": "m.text",
                    "body": "out of namespace"
                }
            })
            .to_string(),
        ))
        .unwrap();

    let response = app.clone().oneshot(forbidden_push_request).await.unwrap();
    assert_eq!(response.status(), StatusCode::FORBIDDEN, "Admin push_event should reject out-of-namespace writes");

    let allowed_push_request = Request::builder()
        .method("POST")
        .uri(format!("/_synapse/admin/v1/appservices/{}/events", as_id))
        .header("Content-Type", "application/json")
        .header("Authorization", format!("Bearer {}", admin_token))
        .body(Body::from(
            json!({
                "room_id": "!owned_room:localhost",
                "event_type": "m.room.message",
                "sender": "@foreign_sender:localhost",
                "content": {
                    "msgtype": "m.text",
                    "body": "owned room write"
                }
            })
            .to_string(),
        ))
        .unwrap();

    let response = app.oneshot(allowed_push_request).await.unwrap();
    assert_eq!(response.status(), StatusCode::CREATED, "Admin push_event should still allow owned namespace writes");
}

/// P0-02: 管理面统计接口需要显式暴露 scheduler 聚合状态
#[tokio::test]
async fn test_appservice_statistics_expose_scheduler_summary() {
    let Some(app) = setup_fresh_test_app().await else {
        return;
    };

    let (admin_token, _) = get_super_admin_token(&app).await;
    let as_id = format!("test_as_scheduler_stats_{}", rand::random::<u32>());
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
                "sender_localpart": "bot_scheduler_stats",
                "namespaces": {
                    "users": [{"regex": "^@sched_stats:localhost$", "exclusive": false}],
                    "aliases": [],
                    "rooms": []
                }
            })
            .to_string(),
        ))
        .unwrap();

    let response = app.clone().oneshot(register_request).await.unwrap();
    assert_eq!(response.status(), StatusCode::CREATED);

    for (state_key, state_value) in [
        ("scheduler_last_result", "backoff"),
        ("scheduler_transaction_state", "retry_backoff"),
        ("scheduler_total_backoff_count", "1"),
        ("scheduler_last_elapsed_ms", "42"),
    ] {
        let set_state_request = Request::builder()
            .method("POST")
            .uri(format!("/_synapse/admin/v1/appservices/{}/state", as_id))
            .header("Content-Type", "application/json")
            .header("Authorization", format!("Bearer {}", admin_token))
            .body(Body::from(
                json!({
                    "state_key": state_key,
                    "state_value": state_value
                })
                .to_string(),
            ))
            .unwrap();

        let response = app.clone().oneshot(set_state_request).await.unwrap();
        assert_eq!(response.status(), StatusCode::OK);
    }

    let statistics_request = Request::builder()
        .method("GET")
        .uri("/_synapse/admin/v1/appservices/statistics")
        .header("Authorization", format!("Bearer {}", admin_token))
        .body(Body::empty())
        .unwrap();

    let response = app.clone().oneshot(statistics_request).await.unwrap();
    assert_eq!(response.status(), StatusCode::OK);

    let body = axum::body::to_bytes(response.into_body(), 1024 * 1024).await.unwrap();
    let statistics_json: serde_json::Value = serde_json::from_slice(&body).unwrap();
    let statistics = statistics_json.as_array().expect("statistics should be an array");
    let service = statistics
        .iter()
        .find(|entry| entry["as_id"] == as_id)
        .expect("registered appservice should be present in statistics");

    assert_eq!(service["pending_event_count"], 0);
    assert_eq!(service["pending_transaction_count"], 0);
    assert_eq!(service["scheduler"]["available"], true);
    assert_eq!(service["scheduler"]["last_result"], "backoff");
    assert_eq!(service["scheduler"]["transaction_state"], "retry_backoff");
    assert_eq!(service["scheduler"]["total_backoff_count"], 1);
    assert_eq!(service["scheduler"]["last_elapsed_ms"], 42);
    assert!(service["scheduler"]["total_success_count"].is_null());
}

/// P1-5: Namespace 查询闭环
/// 验证：用户/房间别名查询能正确返回所属的 AppService
#[tokio::test]
async fn test_appservice_namespace_query() {
    let Some(app) = setup_fresh_test_app().await else {
        return;
    };

    let (admin_token, _) = get_super_admin_token(&app).await;

    // 1. Register AppService with user and alias namespaces
    let as_id = format!("test_as_query_{}", rand::random::<u32>());
    // Use a single random suffix for the user namespace so the pattern
    // and the virtual user ID stay consistent.
    let user_suffix = rand::random::<u32>();
    let user_pattern = format!("@query_bot_{}.*:localhost", user_suffix);
    let alias_suffix = rand::random::<u32>();
    let alias_pattern = format!("#query_room_{}.*:localhost", alias_suffix);

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
    let virtual_user_id = format!("@query_bot_{}_user:localhost", user_suffix);
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
        .uri(format!("/_synapse/admin/v1/appservices/query/user?user_id={}", urlencoding::encode(&virtual_user_id)))
        .header("Authorization", format!("Bearer {}", admin_token))
        .body(Body::empty())
        .unwrap();

    let response = app.clone().oneshot(query_user_request).await.unwrap();
    assert_eq!(response.status(), StatusCode::OK, "User namespace query should succeed");

    let body = axum::body::to_bytes(response.into_body(), 1024 * 1024).await.unwrap();
    let user_query_json: serde_json::Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(user_query_json["user_id"], virtual_user_id);

    // 4. Query room alias namespace
    let test_alias = format!("#query_room_{}_test:localhost", alias_suffix);
    let query_alias_request = Request::builder()
        .method("GET")
        .uri(format!("/_synapse/admin/v1/appservices/query/alias?alias={}", urlencoding::encode(&test_alias)))
        .header("Authorization", format!("Bearer {}", admin_token))
        .body(Body::empty())
        .unwrap();

    let response = app.clone().oneshot(query_alias_request).await.unwrap();
    assert_eq!(response.status(), StatusCode::OK, "Alias namespace query should succeed");

    let body = axum::body::to_bytes(response.into_body(), 1024 * 1024).await.unwrap();
    let alias_query_json: serde_json::Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(alias_query_json["alias"], test_alias);
}

/// REGRESSION (S9): `GET /_matrix/app/v1/{as_id}` must require the AS token.
///
/// It was the only endpoint in this module without the `extract_as_token` +
/// `validate_token` pair its siblings all have, so anyone could read an
/// application service's `url`, `sender_localpart`, `description` and
/// `protocols` by guessing an `as_id`.
#[tokio::test]
async fn test_appservice_query_requires_as_token() {
    let Some(app) = setup_fresh_test_app().await else {
        return;
    };
    let (admin_token, _) = get_super_admin_token(&app).await;

    let as_id = format!("test_as_query_{}", rand::random::<u32>());
    let as_token = format!("test_as_query_token_{}", rand::random::<u32>());
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
                "hs_token": format!("test_hs_query_token_{}", rand::random::<u32>()),
                "sender_localpart": "bot_query",
                "namespaces": { "users": [], "aliases": [], "rooms": [] }
            })
            .to_string(),
        ))
        .unwrap();
    let response = app.clone().oneshot(register_request).await.unwrap();
    assert_eq!(response.status(), StatusCode::CREATED);

    // 1. No credential at all -> rejected (this is the regression: it used to answer 200).
    let unauthenticated =
        Request::builder().method("GET").uri(format!("/_matrix/app/v1/{as_id}")).body(Body::empty()).unwrap();
    let response = app.clone().oneshot(unauthenticated).await.unwrap();
    assert_eq!(
        response.status(),
        StatusCode::UNAUTHORIZED,
        "an anonymous request must not be able to read application-service configuration"
    );

    // 2. The AS's own token -> allowed.
    let authenticated = Request::builder()
        .method("GET")
        .uri(format!("/_matrix/app/v1/{as_id}"))
        .header("Authorization", format!("Bearer {as_token}"))
        .body(Body::empty())
        .unwrap();
    let response = app.clone().oneshot(authenticated).await.unwrap();
    assert_eq!(response.status(), StatusCode::OK, "the AS's own token must be accepted");
    let body = axum::body::to_bytes(response.into_body(), 4096).await.unwrap();
    let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(json["id"], as_id);

    // 3. A different AS's token -> forbidden (a token may only describe its own AS).
    let other_as_id = format!("test_as_query_other_{}", rand::random::<u32>());
    let other_as_token = format!("test_as_query_other_token_{}", rand::random::<u32>());
    let register_other = Request::builder()
        .method("POST")
        .uri("/_synapse/admin/v1/appservices")
        .header("Content-Type", "application/json")
        .header("Authorization", format!("Bearer {}", admin_token))
        .body(Body::from(
            json!({
                "id": &other_as_id,
                "url": "http://localhost:8081",
                "as_token": &other_as_token,
                "hs_token": format!("test_hs_query_other_{}", rand::random::<u32>()),
                "sender_localpart": "bot_query_other",
                "namespaces": { "users": [], "aliases": [], "rooms": [] }
            })
            .to_string(),
        ))
        .unwrap();
    let response = app.clone().oneshot(register_other).await.unwrap();
    assert_eq!(response.status(), StatusCode::CREATED);
    let _ = other_as_id;

    let cross_as = Request::builder()
        .method("GET")
        .uri(format!("/_matrix/app/v1/{as_id}"))
        .header("Authorization", format!("Bearer {other_as_token}"))
        .body(Body::empty())
        .unwrap();
    let response = app.clone().oneshot(cross_as).await.unwrap();
    assert_eq!(response.status(), StatusCode::FORBIDDEN, "a token must not describe another application service");
}

/// B13: `m.login.application_service` — App Service 用 `as_token` 冒充其**排他**
/// 命名空间内的本地用户；命名空间外的用户必须被拒；无效 as_token 必须 401。
#[tokio::test]
async fn test_appservice_login_impersonates_exclusive_namespace_user() {
    let Some(app) = setup_fresh_test_app().await else {
        return;
    };
    let (admin_token, _) = get_super_admin_token(&app).await;

    let suffix = rand::random::<u32>();
    let username = format!("aslogin{suffix}");
    let user_id = format!("@{username}:localhost");
    let as_token = format!("as_login_token_{suffix}");
    let as_id = format!("test_as_login_{suffix}");

    // 1. 注册带**排他** users 命名空间的 AS
    let register_request = Request::builder()
        .method("POST")
        .uri("/_synapse/admin/v1/appservices")
        .header("Content-Type", "application/json")
        .header("Authorization", format!("Bearer {admin_token}"))
        .body(Body::from(
            json!({
                "id": as_id,
                "url": "http://localhost:8080",
                "as_token": as_token,
                "hs_token": format!("hs_login_{suffix}"),
                "sender_localpart": format!("botlogin{suffix}"),
                "namespaces": {
                    "users": [{"regex": format!("^@{username}:localhost$"), "exclusive": true}],
                    "aliases": [],
                    "rooms": []
                }
            })
            .to_string(),
        ))
        .unwrap();
    let response = app.clone().oneshot(register_request).await.unwrap();
    assert_eq!(response.status(), StatusCode::CREATED, "AppService 注册应成功");

    // 2. 创建命名空间内的用户
    let register_user = Request::builder()
        .method("POST")
        .uri("/_matrix/client/v3/register")
        .header("Content-Type", "application/json")
        .body(Body::from(
            json!({
                "username": username,
                "password": "AsLoginPass123!",
                "auth": {"type": "m.login.dummy"}
            })
            .to_string(),
        ))
        .unwrap();
    let register_user_response = app.clone().oneshot(register_user).await.unwrap();
    if register_user_response.status() != StatusCode::OK {
        eprintln!("Skipping AS login test: user registration disabled in this config");
        return;
    }

    // 3. AS 登录其命名空间内的用户 → 200 + 令牌
    let login_request = Request::builder()
        .method("POST")
        .uri("/_matrix/client/v3/login")
        .header("Content-Type", "application/json")
        .header("Authorization", format!("Bearer {as_token}"))
        .body(Body::from(
            json!({
                "type": "m.login.application_service",
                "identifier": {"type": "m.id.user", "user": user_id},
                "device_id": "ASDEV"
            })
            .to_string(),
        ))
        .unwrap();
    let login_response = app.clone().oneshot(login_request).await.unwrap();
    let login_status = login_response.status();
    let body = axum::body::to_bytes(login_response.into_body(), 8192).await.unwrap();
    if login_status != StatusCode::OK {
        panic!("命名空间内的 AS 登录应成功，实际 {login_status}，body={}", String::from_utf8_lossy(&body));
    }
    let value: serde_json::Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(value["user_id"].as_str(), Some(user_id.as_str()));
    assert_eq!(value["device_id"].as_str(), Some("ASDEV"));
    assert!(value["access_token"].as_str().is_some_and(|token| !token.is_empty()), "必须返回 access_token: {value}");

    // 4. 命名空间外的用户 → 403
    let outsider = format!("@outsider{suffix}:localhost");
    let register_outsider = Request::builder()
        .method("POST")
        .uri("/_matrix/client/v3/register")
        .header("Content-Type", "application/json")
        .body(Body::from(
            json!({
                "username": format!("outsider{suffix}"),
                "password": "AsLoginPass123!",
                "auth": {"type": "m.login.dummy"}
            })
            .to_string(),
        ))
        .unwrap();
    let _ = app.clone().oneshot(register_outsider).await.unwrap();

    let login_outsider = Request::builder()
        .method("POST")
        .uri("/_matrix/client/v3/login")
        .header("Content-Type", "application/json")
        .header("Authorization", format!("Bearer {as_token}"))
        .body(Body::from(
            json!({
                "type": "m.login.application_service",
                "identifier": {"type": "m.id.user", "user": outsider}
            })
            .to_string(),
        ))
        .unwrap();
    let outsider_response = app.clone().oneshot(login_outsider).await.unwrap();
    assert_eq!(outsider_response.status(), StatusCode::FORBIDDEN, "AS 不得冒充命名空间外的用户");

    // 5. 无效 as_token → 401
    let bad_token_login = Request::builder()
        .method("POST")
        .uri("/_matrix/client/v3/login")
        .header("Content-Type", "application/json")
        .header("Authorization", "Bearer definitely-not-a-valid-as-token")
        .body(Body::from(
            json!({
                "type": "m.login.application_service",
                "identifier": {"type": "m.id.user", "user": user_id}
            })
            .to_string(),
        ))
        .unwrap();
    let bad_token_response = app.clone().oneshot(bad_token_login).await.unwrap();
    assert_eq!(bad_token_response.status(), StatusCode::UNAUTHORIZED);
}

/// MSC4190: an App Service may manage the devices of the virtual user it
/// impersonates **without** user-interactive auth, and `PUT /devices/{deviceId}`
/// upserts (201 on create, 200 on update). Mirrors upstream
/// `synapse/rest/client/devices.py`, which short-circuits UIA when
/// `requester.app_service_id` is set.
#[tokio::test]
async fn test_appservice_device_management_skips_uia_msc4190() {
    let Some(app) = setup_fresh_test_app().await else {
        return;
    };
    let (admin_token, _) = get_super_admin_token(&app).await;

    let suffix = rand::random::<u32>();
    let username = format!("asdev{suffix}");
    let user_id = format!("@{username}:localhost");
    let as_token = format!("as_dev_token_{suffix}");
    let as_id = format!("test_as_dev_{suffix}");

    // 1. 注册带排他 users 命名空间的 AS
    let register_request = Request::builder()
        .method("POST")
        .uri("/_synapse/admin/v1/appservices")
        .header("Content-Type", "application/json")
        .header("Authorization", format!("Bearer {admin_token}"))
        .body(Body::from(
            json!({
                "id": as_id,
                "url": "http://localhost:8080",
                "as_token": as_token,
                "hs_token": format!("hs_dev_{suffix}"),
                "sender_localpart": format!("botdev{suffix}"),
                "namespaces": {
                    "users": [{"regex": format!("^@{username}:localhost$"), "exclusive": true}],
                    "aliases": [],
                    "rooms": []
                }
            })
            .to_string(),
        ))
        .unwrap();
    let response = app.clone().oneshot(register_request).await.unwrap();
    assert_eq!(response.status(), StatusCode::CREATED, "AppService 注册应成功");

    // 2. 创建命名空间内的用户
    let register_user = Request::builder()
        .method("POST")
        .uri("/_matrix/client/v3/register")
        .header("Content-Type", "application/json")
        .body(Body::from(
            json!({
                "username": username,
                "password": "AsDevPass123!",
                "auth": {"type": "m.login.dummy"}
            })
            .to_string(),
        ))
        .unwrap();
    let register_user_response = app.clone().oneshot(register_user).await.unwrap();
    if register_user_response.status() != StatusCode::OK {
        eprintln!("Skipping MSC4190 device management test: user registration disabled in this config");
        return;
    }

    // 3. AS 登录命名空间内的用户，拿到带 appservice_id 的访问令牌
    let login_request = Request::builder()
        .method("POST")
        .uri("/_matrix/client/v3/login")
        .header("Content-Type", "application/json")
        .header("Authorization", format!("Bearer {as_token}"))
        .body(Body::from(
            json!({
                "type": "m.login.application_service",
                "identifier": {"type": "m.id.user", "user": user_id},
                "device_id": "ASDEV"
            })
            .to_string(),
        ))
        .unwrap();
    let login_response = app.clone().oneshot(login_request).await.unwrap();
    assert_eq!(login_response.status(), StatusCode::OK, "AS 登录应成功");
    let body = axum::body::to_bytes(login_response.into_body(), 8192).await.unwrap();
    let value: serde_json::Value = serde_json::from_slice(&body).unwrap();
    let as_access_token = value["access_token"].as_str().unwrap().to_string();

    // 4. PUT 新设备 → 201 Created（AS upsert 的创建分支）
    let put_new = Request::builder()
        .method("PUT")
        .uri("/_matrix/client/v3/devices/ASNEW")
        .header("Content-Type", "application/json")
        .header("Authorization", format!("Bearer {as_access_token}"))
        .body(Body::from(json!({"display_name": "AS device"}).to_string()))
        .unwrap();
    let put_new_response = app.clone().oneshot(put_new).await.unwrap();
    assert_eq!(put_new_response.status(), StatusCode::CREATED, "AS PUT 新设备应返回 201（MSC4190 upsert 创建分支）");

    // 5. 再次 PUT 同一设备 → 200 OK（AS upsert 的更新分支）
    let put_existing = Request::builder()
        .method("PUT")
        .uri("/_matrix/client/v3/devices/ASNEW")
        .header("Content-Type", "application/json")
        .header("Authorization", format!("Bearer {as_access_token}"))
        .body(Body::from(json!({"display_name": "AS device v2"}).to_string()))
        .unwrap();
    let put_existing_response = app.clone().oneshot(put_existing).await.unwrap();
    assert_eq!(
        put_existing_response.status(),
        StatusCode::OK,
        "AS PUT 已存在设备应返回 200（MSC4190 upsert 更新分支）"
    );

    // 6. DELETE 单设备（不带 UIA auth）→ 200，证明 AS 免 UIA
    let delete_one = Request::builder()
        .method("DELETE")
        .uri("/_matrix/client/v3/devices/ASNEW")
        .header("Content-Type", "application/json")
        .header("Authorization", format!("Bearer {as_access_token}"))
        .body(Body::from("{}"))
        .unwrap();
    let delete_one_response = app.clone().oneshot(delete_one).await.unwrap();
    assert_eq!(delete_one_response.status(), StatusCode::OK, "AS DELETE /devices/{{id}} 应免 UIA 直接成功");

    // 7. PUT 另一台设备后用 POST /delete_devices 批量删除（同样免 UIA）
    let put_batch = Request::builder()
        .method("PUT")
        .uri("/_matrix/client/v3/devices/ASBATCH")
        .header("Content-Type", "application/json")
        .header("Authorization", format!("Bearer {as_access_token}"))
        .body(Body::from(json!({}).to_string()))
        .unwrap();
    let put_batch_response = app.clone().oneshot(put_batch).await.unwrap();
    assert_eq!(put_batch_response.status(), StatusCode::CREATED);

    let delete_batch = Request::builder()
        .method("POST")
        .uri("/_matrix/client/v3/delete_devices")
        .header("Content-Type", "application/json")
        .header("Authorization", format!("Bearer {as_access_token}"))
        .body(Body::from(json!({"devices": ["ASBATCH"]}).to_string()))
        .unwrap();
    let delete_batch_response = app.clone().oneshot(delete_batch).await.unwrap();
    assert_eq!(delete_batch_response.status(), StatusCode::OK, "AS POST /delete_devices 应免 UIA 直接成功");

    // 8. 安全边界：AS 不得 upsert 属于其他用户的设备
    let register_other = Request::builder()
        .method("POST")
        .uri("/_matrix/client/v3/register")
        .header("Content-Type", "application/json")
        .body(Body::from(
            json!({
                "username": format!("otherdev{suffix}"),
                "password": "AsDevPass123!",
                "auth": {"type": "m.login.dummy"}
            })
            .to_string(),
        ))
        .unwrap();
    let register_other_response = app.clone().oneshot(register_other).await.unwrap();
    assert_eq!(register_other_response.status(), StatusCode::OK);
    let other_body = axum::body::to_bytes(register_other_response.into_body(), 8192).await.unwrap();
    let other_value: serde_json::Value = serde_json::from_slice(&other_body).unwrap();
    let other_device_id = other_value["device_id"].as_str().unwrap().to_string();

    let put_foreign = Request::builder()
        .method("PUT")
        .uri(format!("/_matrix/client/v3/devices/{other_device_id}"))
        .header("Content-Type", "application/json")
        .header("Authorization", format!("Bearer {as_access_token}"))
        .body(Body::from(json!({"display_name": "hijack"}).to_string()))
        .unwrap();
    let put_foreign_response = app.clone().oneshot(put_foreign).await.unwrap();
    assert_eq!(
        put_foreign_response.status(),
        StatusCode::NOT_FOUND,
        "AS 不得 upsert 属于其他用户的设备（应返回 404，与「无此设备」不可区分）"
    );
}

/// App Service pusher 面：上游 Matrix 的 Application Service API **没有**专属 pusher
/// 端点（见 spec「Client-Server API Extensions → Identity assertion」）—— AS 通过
/// `m.login.application_service` 登录拿到带 `appservice_id` 的访问令牌后，直接调用
/// 通用 C-S pusher 端点（`POST /pushers/set`、`GET /pushers`）即可管理其虚拟用户的
/// pusher。本用例锁定这一链路不被后续改动破坏，与
/// `test_appservice_device_management_skips_uia_msc4190` 同构。
#[tokio::test]
async fn test_appservice_pusher_management_via_login() {
    let Some(app) = setup_fresh_test_app().await else {
        return;
    };
    let (admin_token, _) = get_super_admin_token(&app).await;

    let suffix = rand::random::<u32>();
    let username = format!("aspush{suffix}");
    let user_id = format!("@{username}:localhost");
    let as_token = format!("as_push_token_{suffix}");
    let as_id = format!("test_as_push_{suffix}");

    // 1. 注册带排他 users 命名空间的 AS
    let register_request = Request::builder()
        .method("POST")
        .uri("/_synapse/admin/v1/appservices")
        .header("Content-Type", "application/json")
        .header("Authorization", format!("Bearer {admin_token}"))
        .body(Body::from(
            json!({
                "id": as_id,
                "url": "http://localhost:8080",
                "as_token": as_token,
                "hs_token": format!("hs_push_{suffix}"),
                "sender_localpart": format!("botpush{suffix}"),
                "namespaces": {
                    "users": [{"regex": format!("^@{username}:localhost$"), "exclusive": true}],
                    "aliases": [],
                    "rooms": []
                }
            })
            .to_string(),
        ))
        .unwrap();
    let response = app.clone().oneshot(register_request).await.unwrap();
    assert_eq!(response.status(), StatusCode::CREATED, "AppService 注册应成功");

    // 2. 创建命名空间内的用户
    let register_user = Request::builder()
        .method("POST")
        .uri("/_matrix/client/v3/register")
        .header("Content-Type", "application/json")
        .body(Body::from(
            json!({
                "username": username,
                "password": "AsPushPass123!",
                "auth": {"type": "m.login.dummy"}
            })
            .to_string(),
        ))
        .unwrap();
    let register_user_response = app.clone().oneshot(register_user).await.unwrap();
    if register_user_response.status() != StatusCode::OK {
        eprintln!("Skipping AS pusher test: user registration disabled in this config");
        return;
    }

    // 3. AS 登录命名空间内的用户，拿到带 appservice_id 的访问令牌
    let login_request = Request::builder()
        .method("POST")
        .uri("/_matrix/client/v3/login")
        .header("Content-Type", "application/json")
        .header("Authorization", format!("Bearer {as_token}"))
        .body(Body::from(
            json!({
                "type": "m.login.application_service",
                "identifier": {"type": "m.id.user", "user": user_id},
                "device_id": "ASPUSH"
            })
            .to_string(),
        ))
        .unwrap();
    let login_response = app.clone().oneshot(login_request).await.unwrap();
    assert_eq!(login_response.status(), StatusCode::OK, "AS 登录应成功");
    let body = axum::body::to_bytes(login_response.into_body(), 8192).await.unwrap();
    let value: serde_json::Value = serde_json::from_slice(&body).unwrap();
    let as_access_token = value["access_token"].as_str().unwrap().to_string();

    // 4. AS 通过通用 C-S 端点创建 pusher → 200
    let set_pusher = Request::builder()
        .method("POST")
        .uri("/_matrix/client/v3/pushers/set")
        .header("Content-Type", "application/json")
        .header("Authorization", format!("Bearer {as_access_token}"))
        .body(Body::from(
            json!({
                "pushkey": "as-pushkey-1",
                "kind": "http",
                "app_id": "com.example.bridge",
                "app_display_name": "Bridge Push",
                "device_display_name": "AS Device",
                "lang": "en",
                "data": {"url": "https://push.example.test"}
            })
            .to_string(),
        ))
        .unwrap();
    let set_pusher_response = app.clone().oneshot(set_pusher).await.unwrap();
    assert_eq!(set_pusher_response.status(), StatusCode::OK, "AS 应能为其虚拟用户设置 pusher");

    // 5. GET /pushers 能看到刚设置的 pusher
    let get_pushers = Request::builder()
        .method("GET")
        .uri("/_matrix/client/v3/pushers")
        .header("Authorization", format!("Bearer {as_access_token}"))
        .body(Body::empty())
        .unwrap();
    let get_pushers_response = app.clone().oneshot(get_pushers).await.unwrap();
    assert_eq!(get_pushers_response.status(), StatusCode::OK);
    let body = axum::body::to_bytes(get_pushers_response.into_body(), 8192).await.unwrap();
    let pushers_json: serde_json::Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(pushers_json["pushers"][0]["pushkey"], json!("as-pushkey-1"));

    // 6. kind=null 删除该 pusher → 200，GET 变空
    let delete_pusher = Request::builder()
        .method("POST")
        .uri("/_matrix/client/v3/pushers/set")
        .header("Content-Type", "application/json")
        .header("Authorization", format!("Bearer {as_access_token}"))
        .body(Body::from(
            json!({
                "pushkey": "as-pushkey-1",
                "kind": serde_json::Value::Null,
                "app_id": "com.example.bridge"
            })
            .to_string(),
        ))
        .unwrap();
    let delete_pusher_response = app.clone().oneshot(delete_pusher).await.unwrap();
    assert_eq!(delete_pusher_response.status(), StatusCode::OK, "AS 应能删除其虚拟用户的 pusher");

    let get_pushers_after = Request::builder()
        .method("GET")
        .uri("/_matrix/client/v3/pushers")
        .header("Authorization", format!("Bearer {as_access_token}"))
        .body(Body::empty())
        .unwrap();
    let get_pushers_after_response = app.clone().oneshot(get_pushers_after).await.unwrap();
    assert_eq!(get_pushers_after_response.status(), StatusCode::OK);
    let body = axum::body::to_bytes(get_pushers_after_response.into_body(), 8192).await.unwrap();
    let pushers_after_json: serde_json::Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(pushers_after_json["pushers"], json!([]), "删除后 pusher 列表应为空");
}

// ============================================================================
// MSC4190：应用服务**注册**虚拟用户（`POST /register`）
//
// 这条面此前整体缺失：`/register` 完全不认 AS token，只接受 `auth.type` 里的
// `m.login.demo/password` 之类，`inhibit_login` 全仓零命中 —— 于是
// `M_APPSERVICE_LOGIN_UNSUPPORTED` 这个码也就没有存在的地方。
//
// 三条判据（与上游同口径）：
// 1. AS token + `inhibit_login: true` ⇒ 200，**只回 user_id**（无 access_token / device_id）；
// 2. 缺 `inhibit_login`（或 false）⇒ 400 `M_APPSERVICE_LOGIN_UNSUPPORTED`；
// 3. 用户不在该 AS 的 `users` 命名空间 ⇒ 403。
// ============================================================================

/// 注册一个带 `users` 命名空间的 AS，返回 `(as_token, 命名空间前缀)`。
async fn register_appservice_with_user_namespace(app: &axum::Router) -> (String, String) {
    let (admin_token, _) = get_super_admin_token(app).await;
    let suffix = rand::random::<u32>();
    let as_token = format!("as_token_register_{suffix}");
    let namespace_pattern = format!("@asbot_{suffix}.*:localhost");

    let request = Request::builder()
        .method("POST")
        .uri("/_synapse/admin/v1/appservices")
        .header("Content-Type", "application/json")
        .header("Authorization", format!("Bearer {admin_token}"))
        .body(Body::from(
            json!({
                "id": format!("test_as_register_{suffix}"),
                "url": "http://localhost:8080",
                "as_token": &as_token,
                "hs_token": format!("hs_token_register_{suffix}"),
                "sender_localpart": format!("bot_register_{suffix}"),
                "namespaces": {
                    "users": [{ "regex": &namespace_pattern, "exclusive": true }]
                }
            })
            .to_string(),
        ))
        .unwrap();

    let response = app.clone().oneshot(request).await.unwrap();
    assert_eq!(response.status(), StatusCode::CREATED, "AS registration fixture must succeed");
    (as_token, format!("asbot_{suffix}"))
}

async fn register_as_user(
    app: &axum::Router,
    as_token: &str,
    username: &str,
    body: serde_json::Value,
) -> (StatusCode, serde_json::Value) {
    let request = Request::builder()
        .method("POST")
        .uri("/_matrix/client/v3/register")
        .header("Content-Type", "application/json")
        .header("Authorization", format!("Bearer {as_token}"))
        .body(Body::from(body.to_string()))
        .unwrap();

    let response = app.clone().oneshot(request).await.unwrap();
    let status = response.status();
    let bytes = axum::body::to_bytes(response.into_body(), 1024 * 1024).await.unwrap();
    let json: serde_json::Value = serde_json::from_slice(&bytes).unwrap_or(serde_json::Value::Null);
    let _ = username;
    (status, json)
}

/// 正向：`inhibit_login: true` ⇒ 建用户，**不签发会话**。
#[tokio::test]
async fn test_appservice_register_user_without_login() {
    let Some(app) = setup_fresh_test_app().await else {
        return;
    };
    let (as_token, prefix) = register_appservice_with_user_namespace(&app).await;
    let username = format!("{prefix}_alice");

    let (status, json) = register_as_user(
        &app,
        &as_token,
        &username,
        json!({ "username": &username, "inhibit_login": true, "displayname": "AS Alice" }),
    )
    .await;

    assert_eq!(status, StatusCode::OK, "AS 注册必须成功，响应: {json}");
    assert_eq!(json["user_id"], format!("@{username}:localhost"), "必须回本地用户 id");
    assert!(
        json.get("access_token").is_none() && json.get("device_id").is_none(),
        "`inhibit_login: true` 下不得签发会话，实得: {json}"
    );

    // 用户真的建出来了：公开 profile 能读到，且 displayname 生效。
    let profile_request = Request::builder()
        .uri(format!("/_matrix/client/v3/profile/@{username}:localhost"))
        .body(Body::empty())
        .unwrap();
    let response = app.clone().oneshot(profile_request).await.unwrap();
    assert_eq!(response.status(), StatusCode::OK, "注册出的用户必须可查");
    let bytes = axum::body::to_bytes(response.into_body(), 1024 * 1024).await.unwrap();
    let profile: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
    assert_eq!(profile["displayname"], "AS Alice", "displayname 必须落库，实得: {profile}");
}

/// 负控：缺 `inhibit_login` ⇒ 400 `M_APPSERVICE_LOGIN_UNSUPPORTED`（该码此前全仓零命中）。
#[tokio::test]
async fn test_appservice_register_requires_inhibit_login() {
    let Some(app) = setup_fresh_test_app().await else {
        return;
    };
    let (as_token, prefix) = register_appservice_with_user_namespace(&app).await;

    for body in [
        json!({ "username": format!("{prefix}_noflag") }),
        json!({ "username": format!("{prefix}_false"), "inhibit_login": false }),
    ] {
        let (status, json) = register_as_user(&app, &as_token, "ignored", body).await;
        assert_eq!(status, StatusCode::BAD_REQUEST, "缺/假 inhibit_login 必须 400，实得: {json}");
        assert_eq!(
            json["errcode"], "M_APPSERVICE_LOGIN_UNSUPPORTED",
            "必须是 MSC4190 的稳定码（不是泛化的 M_BAD_JSON），实得: {json}"
        );
    }
}

/// 负控：用户不在 AS 的 `users` 命名空间 ⇒ 403，且**不建用户**。
#[tokio::test]
async fn test_appservice_register_rejects_user_outside_namespace() {
    let Some(app) = setup_fresh_test_app().await else {
        return;
    };
    let (as_token, _prefix) = register_appservice_with_user_namespace(&app).await;
    let username = format!("outsider_{}", rand::random::<u32>());

    let (status, json) =
        register_as_user(&app, &as_token, &username, json!({ "username": &username, "inhibit_login": true })).await;
    assert_eq!(status, StatusCode::FORBIDDEN, "命名空间外的用户必须被拒，实得: {json}");
}
