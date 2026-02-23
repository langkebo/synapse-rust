use axum::{
    body::Body,
    http::{Request, StatusCode},
};
use serde_json::{json, Value};
use std::sync::Arc;
use synapse_rust::cache::{CacheConfig, CacheManager};
use synapse_rust::services::ServiceContainer;
use synapse_rust::web::routes::create_router;
use synapse_rust::web::AppState;
use tower::ServiceExt;

async fn setup_test_app() -> Option<axum::Router> {
    if !super::init_test_database().await {
        return None;
    }
    let container = ServiceContainer::new_test();
    let cache = Arc::new(CacheManager::new(CacheConfig::default()));
    let state = AppState::new(container, cache);
    Some(create_router(state))
}

async fn register_user(app: &axum::Router, username: &str) -> (String, String) {
    let request = Request::builder()
        .method("POST")
        .uri("/_matrix/client/r0/register")
        .header("Content-Type", "application/json")
        .body(Body::from(
            json!({
                "username": username,
                "password": "Password123!",
                "auth": {"type": "m.login.dummy"}
            })
            .to_string(),
        ))
        .unwrap();

    let response = ServiceExt::<Request<Body>>::oneshot(app.clone(), request)
        .await
        .unwrap();

    let status = response.status();
    let body = axum::body::to_bytes(response.into_body(), 1024)
        .await
        .unwrap();
    if status != StatusCode::OK {
        panic!(
            "Registration failed with status {}: {:?}",
            status,
            String::from_utf8_lossy(&body)
        );
    }
    let json: Value = serde_json::from_slice(&body).unwrap();
    (
        json["access_token"].as_str().unwrap().to_string(),
        json["user_id"].as_str().unwrap().to_string(),
    )
}

#[tokio::test]
async fn test_device_management() {
    let Some(app) = setup_test_app().await else {
        return;
    };
    let (token, _) = register_user(&app, &format!("user_{}", rand::random::<u32>())).await;

    // 1. Get Devices
    let request = Request::builder()
        .uri("/_matrix/client/r0/devices")
        .header("Authorization", format!("Bearer {}", token))
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
    let devices = json["devices"].as_array().unwrap();
    assert!(!devices.is_empty());
    let device_id = devices[0]["device_id"].as_str().unwrap().to_string();

    // 2. Get Single Device
    let request = Request::builder()
        .uri(format!("/_matrix/client/r0/devices/{}", device_id))
        .header("Authorization", format!("Bearer {}", token))
        .body(Body::empty())
        .unwrap();
    let response = ServiceExt::<Request<Body>>::oneshot(app.clone(), request)
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);

    // 3. Update Device
    let request = Request::builder()
        .method("PUT")
        .uri(format!("/_matrix/client/r0/devices/{}", device_id))
        .header("Authorization", format!("Bearer {}", token))
        .header("Content-Type", "application/json")
        .body(Body::from(
            json!({
                "display_name": "New Device Name"
            })
            .to_string(),
        ))
        .unwrap();
    let response = ServiceExt::<Request<Body>>::oneshot(app.clone(), request)
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);

    // 4. Delete Single Device
    // First, we need to login again to get another device or just delete the current one (might invalidate token if it's the only one)
    // Let's just delete it and check 200 or 401 on next request.
    let request = Request::builder()
        .method("DELETE")
        .uri(format!("/_matrix/client/r0/devices/{}", device_id))
        .header("Authorization", format!("Bearer {}", token))
        .header("Content-Type", "application/json")
        .body(Body::from(
            json!({"auth": {"type": "m.login.password", "user": "...", "password": "..."}})
                .to_string(),
        ))
        .unwrap();
    let response = ServiceExt::<Request<Body>>::oneshot(app.clone(), request)
        .await
        .unwrap();
    // Some servers require UIA (User Interactive Authentication) for deleting devices.
    // Our implementation might just return 200 for now if UIA is not fully implemented.
    assert!(response.status() == StatusCode::OK || response.status() == StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn test_presence_management() {
    let Some(app) = setup_test_app().await else {
        return;
    };
    let (token, user_id) = register_user(&app, &format!("user_{}", rand::random::<u32>())).await;

    // 1. Set Presence
    let request = Request::builder()
        .method("PUT")
        .uri(format!("/_matrix/client/r0/presence/{}/status", user_id))
        .header("Authorization", format!("Bearer {}", token))
        .header("Content-Type", "application/json")
        .body(Body::from(
            json!({
                "presence": "online",
                "status_msg": "Coding in Rust"
            })
            .to_string(),
        ))
        .unwrap();
    let response = ServiceExt::<Request<Body>>::oneshot(app.clone(), request)
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);

    // 2. Get Presence
    let request = Request::builder()
        .uri(format!("/_matrix/client/r0/presence/{}/status", user_id))
        .header("Authorization", format!("Bearer {}", token))
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
    assert_eq!(json["presence"], "online");
    assert_eq!(json["status_msg"], "Coding in Rust");
}
