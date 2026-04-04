use axum::body::Body;
use hyper::{Request, StatusCode};
use serde_json::json;
use tower::ServiceExt;

use crate::{get_admin_token, setup_test_app};

#[tokio::test]
async fn test_appservice_list_empty() {
    let Some(app) = setup_test_app().await else {
        return;
    };

    let (admin_token, _) = get_admin_token(&app).await;

    // List AppServices (should be empty initially)
    let list_request = Request::builder()
        .method("GET")
        .uri("/_synapse/admin/v1/appservices")
        .header("Authorization", format!("Bearer {}", admin_token))
        .body(Body::empty())
        .unwrap();

    let response = app.clone().oneshot(list_request).await.unwrap();
    assert_eq!(
        response.status(),
        StatusCode::OK,
        "AppService list should return 200 OK"
    );

    let body = axum::body::to_bytes(response.into_body(), 1024 * 1024)
        .await
        .unwrap();
    let list_json: serde_json::Value = serde_json::from_slice(&body).unwrap();
    assert!(list_json.is_array(), "Response should be an array");
}

#[tokio::test]
async fn test_appservice_register_and_query() {
    let Some(app) = setup_test_app().await else {
        return;
    };

    let (admin_token, _) = get_admin_token(&app).await;

    // 1. Register AppService
    let as_id = format!("test_as_{}", rand::random::<u32>());
    let register_request = Request::builder()
        .method("POST")
        .uri("/_synapse/admin/v1/appservices")
        .header("Content-Type", "application/json")
        .header("Authorization", format!("Bearer {}", admin_token))
        .body(Body::from(
            json!({
                "id": &as_id,
                "url": "http://localhost:8080",
                "as_token": "test_as_token_123",
                "hs_token": "test_hs_token_456",
                "sender_localpart": "bot_test",
                "description": "Test AppService",
                "rate_limited": false,
                "protocols": ["test_protocol"]
            })
            .to_string(),
        ))
        .unwrap();

    let response = app.clone().oneshot(register_request).await.unwrap();
    assert_eq!(
        response.status(),
        StatusCode::CREATED,
        "AppService registration should return 201 CREATED"
    );

    let body = axum::body::to_bytes(response.into_body(), 1024 * 1024)
        .await
        .unwrap();
    let register_json: serde_json::Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(register_json["as_id"], as_id);
    assert_eq!(register_json["url"], "http://localhost:8080");

    // 2. Query AppService
    let query_request = Request::builder()
        .method("GET")
        .uri(format!("/_synapse/admin/v1/appservices/{}", as_id))
        .header("Authorization", format!("Bearer {}", admin_token))
        .body(Body::empty())
        .unwrap();

    let response = app.clone().oneshot(query_request).await.unwrap();
    assert_eq!(
        response.status(),
        StatusCode::OK,
        "AppService query should return 200 OK"
    );

    let body = axum::body::to_bytes(response.into_body(), 1024 * 1024)
        .await
        .unwrap();
    let query_json: serde_json::Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(query_json["as_id"], as_id);
    assert_eq!(query_json["url"], "http://localhost:8080");
    assert_eq!(query_json["sender"], "bot_test");
}

#[tokio::test]
async fn test_appservice_virtual_user() {
    let Some(app) = setup_test_app().await else {
        return;
    };

    let (admin_token, _) = get_admin_token(&app).await;

    // 1. Register AppService
    let as_id = format!("test_as_{}", rand::random::<u32>());
    let register_request = Request::builder()
        .method("POST")
        .uri("/_synapse/admin/v1/appservices")
        .header("Content-Type", "application/json")
        .header("Authorization", format!("Bearer {}", admin_token))
        .body(Body::from(
            json!({
                "id": &as_id,
                "url": "http://localhost:8080",
                "as_token": "test_as_token_789",
                "hs_token": "test_hs_token_012",
                "sender_localpart": "bot_virtual",
                "namespaces": {
                    "users": [{"regex": "@bot_.*:localhost", "exclusive": true}]
                }
            })
            .to_string(),
        ))
        .unwrap();

    let response = app.clone().oneshot(register_request).await.unwrap();
    assert_eq!(response.status(), StatusCode::CREATED);

    // 2. Register virtual user
    let virtual_user_id = format!("@bot_test_{}:localhost", rand::random::<u32>());
    let register_user_request = Request::builder()
        .method("POST")
        .uri(format!("/_synapse/admin/v1/appservices/{}/users", as_id))
        .header("Content-Type", "application/json")
        .header("Authorization", format!("Bearer {}", admin_token))
        .body(Body::from(
            json!({
                "user_id": &virtual_user_id,
                "displayname": "Test Bot",
                "avatar_url": "mxc://localhost/avatar123"
            })
            .to_string(),
        ))
        .unwrap();

    let response = app.clone().oneshot(register_user_request).await.unwrap();
    assert_eq!(
        response.status(),
        StatusCode::CREATED,
        "Virtual user registration should return 201 CREATED"
    );

    let body = axum::body::to_bytes(response.into_body(), 1024 * 1024)
        .await
        .unwrap();
    let user_json: serde_json::Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(user_json["user_id"], virtual_user_id);
    assert_eq!(user_json["as_id"], as_id);

    // 3. Query virtual users
    let query_users_request = Request::builder()
        .method("GET")
        .uri(format!("/_synapse/admin/v1/appservices/{}/users", as_id))
        .header("Authorization", format!("Bearer {}", admin_token))
        .body(Body::empty())
        .unwrap();

    let response = app.clone().oneshot(query_users_request).await.unwrap();
    assert_eq!(
        response.status(),
        StatusCode::OK,
        "Virtual users query should return 200 OK"
    );

    let body = axum::body::to_bytes(response.into_body(), 1024 * 1024)
        .await
        .unwrap();
    let users_json: serde_json::Value = serde_json::from_slice(&body).unwrap();
    let users = users_json.as_array().unwrap();
    assert!(
        users.iter().any(|u| u["user_id"] == virtual_user_id),
        "Virtual user should be in the list"
    );
}
