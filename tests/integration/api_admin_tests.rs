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

    assert_eq!(response.status(), StatusCode::OK);
    let body = axum::body::to_bytes(response.into_body(), 1024)
        .await
        .unwrap();
    let json: Value = serde_json::from_slice(&body).unwrap();
    let nonce = json["nonce"].as_str().unwrap().to_string();

    // 2. Register admin
    let username = format!("admin_{}", rand::random::<u32>());
    let password = "password123";
    let shared_secret = "test_shared_secret"; // From ServiceContainer::new_test

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

    assert_eq!(response.status(), StatusCode::OK);
    let body = axum::body::to_bytes(response.into_body(), 1024)
        .await
        .unwrap();
    let json: Value = serde_json::from_slice(&body).unwrap();
    (json["access_token"].as_str().unwrap().to_string(), username)
}

#[tokio::test]
async fn test_admin_flow() {
    let Some(app) = setup_test_app().await else {
        return;
    };
    let (token, username) = get_admin_token(&app).await;

    // Test server version
    let request = Request::builder()
        .uri("/_synapse/admin/v1/server_version")
        .header("Authorization", format!("Bearer {}", token))
        .body(Body::empty())
        .unwrap();

    let response = ServiceExt::<Request<Body>>::oneshot(app.clone(), request)
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);

    // Test status
    let request = Request::builder()
        .uri("/_synapse/admin/v1/status")
        .header("Authorization", format!("Bearer {}", token))
        .body(Body::empty())
        .unwrap();

    let response = ServiceExt::<Request<Body>>::oneshot(app.clone(), request)
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);

    // Test users list
    let request = Request::builder()
        .uri("/_synapse/admin/v1/users")
        .header("Authorization", format!("Bearer {}", token))
        .body(Body::empty())
        .unwrap();

    let response = ServiceExt::<Request<Body>>::oneshot(app.clone(), request)
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);

    // Test specific user
    let request = Request::builder()
        .uri(format!("/_synapse/admin/v1/users/{}", username))
        .header("Authorization", format!("Bearer {}", token))
        .body(Body::empty())
        .unwrap();
    let response = ServiceExt::<Request<Body>>::oneshot(app.clone(), request)
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);

    // Test IP blocking
    let request = Request::builder()
        .method("POST")
        .uri("/_synapse/admin/v1/security/ip/block")
        .header("Authorization", format!("Bearer {}", token))
        .header("Content-Type", "application/json")
        .body(Body::from(
            json!({
                "ip_address": "1.2.3.4",
                "reason": "Spamming"
            })
            .to_string(),
        ))
        .unwrap();
    let response = ServiceExt::<Request<Body>>::oneshot(app.clone(), request)
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);

    let request = Request::builder()
        .uri("/_synapse/admin/v1/security/ip/blocks")
        .header("Authorization", format!("Bearer {}", token))
        .body(Body::empty())
        .unwrap();
    let response = ServiceExt::<Request<Body>>::oneshot(app.clone(), request)
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);

    // Test rooms list
    let request = Request::builder()
        .uri("/_synapse/admin/v1/rooms")
        .header("Authorization", format!("Bearer {}", token))
        .body(Body::empty())
        .unwrap();
    let response = ServiceExt::<Request<Body>>::oneshot(app.clone(), request)
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);
}
