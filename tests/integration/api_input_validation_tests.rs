use axum::{
    body::Body,
    http::{Request, StatusCode},
};
use hmac::{Hmac, Mac};
use serde_json::{json, Value};
use sha2::Sha256;
use std::sync::Arc;
use synapse_rust::cache::{CacheConfig, CacheManager};
use synapse_rust::services::ServiceContainer;
use synapse_rust::web::routes::create_router;
use synapse_rust::web::AppState;
use tower::ServiceExt;

type HmacSha256 = Hmac<Sha256>;

async fn setup_test_app() -> Option<axum::Router> {
    if !super::init_test_database().await {
        return None;
    }
    let container = ServiceContainer::new_test();
    let cache = Arc::new(CacheManager::new(CacheConfig::default()));
    let state = AppState::new(container, cache);
    Some(create_router(state))
}

async fn get_admin_token(app: &axum::Router) -> (String, String) {
    // 1. Get nonce
    let request = Request::builder()
        .uri("/_synapse/admin/v1/register/nonce")
        .body(Body::empty())
        .unwrap();

    let response = ServiceExt::<Request<Body>>::oneshot(app.clone(), request)
        .await
        .unwrap();

    let body = axum::body::to_bytes(response.into_body(), 1024)
        .await
        .unwrap();
    let json: Value = serde_json::from_slice(&body).unwrap();
    let nonce = json["nonce"].as_str().unwrap().to_string();

    // 2. Register admin
    let username = format!("admin_{}", rand::random::<u32>());
    let password = "password123";
    let shared_secret = "test_shared_secret";

    let mut mac = HmacSha256::new_from_slice(shared_secret.as_bytes()).unwrap();
    mac.update(nonce.as_bytes());
    mac.update(b"\0");
    mac.update(username.as_bytes());
    mac.update(b"\0");
    mac.update(password.as_bytes());
    mac.update(b"\0");
    mac.update(b"admin");

    let expected_mac = mac.finalize().into_bytes();
    let mac_hex = expected_mac
        .iter()
        .map(|b| format!("{:02x}", b))
        .collect::<String>();

    let request = Request::builder()
        .method("POST")
        .uri("/_synapse/admin/v1/register")
        .header("Content-Type", "application/json")
        .body(Body::from(
            json!({
                "nonce": nonce,
                "username": username,
                "password": password,
                "admin": true,
                "mac": mac_hex
            })
            .to_string(),
        ))
        .unwrap();

    let response = ServiceExt::<Request<Body>>::oneshot(app.clone(), request)
        .await
        .unwrap();
    
    let body = axum::body::to_bytes(response.into_body(), 1024)
        .await
        .unwrap();
    let json: Value = serde_json::from_slice(&body).unwrap();
    (json["access_token"].as_str().unwrap().to_string(), username)
}

#[tokio::test]
async fn test_admin_input_validation() {
    let Some(app) = setup_test_app().await else {
        return;
    };
    let (token, _username) = get_admin_token(&app).await;

    // 1. Block invalid IP
    let request = Request::builder()
        .method("POST")
        .uri("/_synapse/admin/v1/security/ip/block")
        .header("Authorization", format!("Bearer {}", token))
        .header("Content-Type", "application/json")
        .body(Body::from(
            json!({
                "ip_address": "invalid-ip",
                "reason": "Test"
            })
            .to_string(),
        ))
        .unwrap();
    let response = ServiceExt::<Request<Body>>::oneshot(app.clone(), request)
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::BAD_REQUEST);

    // 2. Set admin for non-existent user
    let request = Request::builder()
        .method("PUT")
        .uri("/_synapse/admin/v1/users/@nonexistent:localhost/admin")
        .header("Authorization", format!("Bearer {}", token))
        .header("Content-Type", "application/json")
        .body(Body::from(json!({"admin": true}).to_string()))
        .unwrap();
    let response = ServiceExt::<Request<Body>>::oneshot(app.clone(), request)
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::NOT_FOUND);

    // 3. Shutdown non-existent room
    let request = Request::builder()
        .method("POST")
        .uri("/_synapse/admin/v1/shutdown_room")
        .header("Authorization", format!("Bearer {}", token))
        .header("Content-Type", "application/json")
        .body(Body::from(json!({"room_id": "!nonexistent:localhost"}).to_string()))
        .unwrap();
    let response = ServiceExt::<Request<Body>>::oneshot(app.clone(), request)
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn test_client_input_validation() {
    let Some(app) = setup_test_app().await else {
        return;
    };
    let (token, _username) = get_admin_token(&app).await;

    // 1. Register with too long username
    let long_username = "a".repeat(256);
    let request = Request::builder()
        .method("POST")
        .uri("/_matrix/client/r0/register")
        .header("Content-Type", "application/json")
        .body(Body::from(
            json!({
                "username": long_username,
                "password": "password123",
                "auth": { "type": "m.login.dummy" }
            })
            .to_string(),
        ))
        .unwrap();
    let response = ServiceExt::<Request<Body>>::oneshot(app.clone(), request)
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::BAD_REQUEST);

    // 2. Create room with too long name
    let long_name = "a".repeat(256);
    let request = Request::builder()
        .method("POST")
        .uri("/_matrix/client/r0/createRoom")
        .header("Authorization", format!("Bearer {}", token))
        .header("Content-Type", "application/json")
        .body(Body::from(
            json!({
                "name": long_name
            })
            .to_string(),
        ))
        .unwrap();
    let response = ServiceExt::<Request<Body>>::oneshot(app.clone(), request)
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::BAD_REQUEST);

    // 3. Invite non-existent user (need a valid room first)
    // Create a valid room first
    let request = Request::builder()
        .method("POST")
        .uri("/_matrix/client/r0/createRoom")
        .header("Authorization", format!("Bearer {}", token))
        .header("Content-Type", "application/json")
        .body(Body::from(json!({}).to_string()))
        .unwrap();
    let response = ServiceExt::<Request<Body>>::oneshot(app.clone(), request)
        .await
        .unwrap();
    let body = axum::body::to_bytes(response.into_body(), 1024).await.unwrap();
    let json: Value = serde_json::from_slice(&body).unwrap();
    let room_id = json["room_id"].as_str().unwrap();

    let request = Request::builder()
        .method("POST")
        .uri(format!("/_matrix/client/r0/rooms/{}/invite", room_id))
        .header("Authorization", format!("Bearer {}", token))
        .header("Content-Type", "application/json")
        .body(Body::from(json!({"user_id": "@nonexistent:localhost"}).to_string()))
        .unwrap();
    let response = ServiceExt::<Request<Body>>::oneshot(app.clone(), request)
        .await
        .unwrap();
    // This depends on whether validate_user_id or user_exists check comes first.
    // validate_user_id checks format. @nonexistent:localhost is valid format.
    // user_exists check fails -> Not Found.
    // Wait, invite_user handler now has validate_user_id check.
    // Then service.invite_user checks existence -> Not Found.
    assert_eq!(response.status(), StatusCode::NOT_FOUND);
}
