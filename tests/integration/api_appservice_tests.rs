use axum::body::Body;
use hyper::{Request, StatusCode};
use serde_json::json;
use tower::ServiceExt;

use crate::{get_admin_token, setup_test_app};

async fn register_user(app: &axum::Router, username: &str) -> (String, String) {
    let request = Request::builder()
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
    let response = app.clone().oneshot(request).await.unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    let body = axum::body::to_bytes(response.into_body(), 4096)
        .await
        .unwrap();
    let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
    (
        json["access_token"].as_str().unwrap().to_string(),
        json["user_id"].as_str().unwrap().to_string(),
    )
}

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

#[tokio::test]
async fn test_user_appservice_endpoint_is_self_only_for_non_admins() {
    let Some(app) = setup_test_app().await else {
        return;
    };

    let (alice_token, alice_id) = register_user(
        &app,
        &format!("appservice_self_alice_{}", rand::random::<u32>()),
    )
    .await;
    let (_, bob_id) = register_user(
        &app,
        &format!("appservice_self_bob_{}", rand::random::<u32>()),
    )
    .await;
    let (admin_token, _) = get_admin_token(&app).await;

    let own_request = Request::builder()
        .method("GET")
        .uri(format!("/_matrix/client/v1/user/{}/appservice", alice_id))
        .header("Authorization", format!("Bearer {}", alice_token))
        .body(Body::empty())
        .unwrap();
    let response = app.clone().oneshot(own_request).await.unwrap();
    assert_eq!(response.status(), StatusCode::OK);

    let cross_user_request = Request::builder()
        .method("GET")
        .uri(format!("/_matrix/client/v1/user/{}/appservice", bob_id))
        .header("Authorization", format!("Bearer {}", alice_token))
        .body(Body::empty())
        .unwrap();
    let response = app.clone().oneshot(cross_user_request).await.unwrap();
    assert_eq!(response.status(), StatusCode::FORBIDDEN);

    let admin_request = Request::builder()
        .method("GET")
        .uri(format!("/_matrix/client/v1/user/{}/appservice", bob_id))
        .header("Authorization", format!("Bearer {}", admin_token))
        .body(Body::empty())
        .unwrap();
    let response = app.oneshot(admin_request).await.unwrap();
    assert_eq!(response.status(), StatusCode::OK);
}
