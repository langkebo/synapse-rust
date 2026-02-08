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

// Simplified setup_test_app similar to api_admin_tests.rs
async fn setup_test_app() -> Option<axum::Router> {
    if !super::init_test_database().await {
        return None;
    }
    let container = ServiceContainer::new_test();
    let cache = Arc::new(CacheManager::new(CacheConfig::default()));
    let state = AppState::new(container, cache);
    Some(create_router(state))
}

// Helper to register a test user and get token
async fn create_test_user(app: &axum::Router) -> String {
    let username = format!("user_{}", rand::random::<u32>());
    let password = "password123";

    // Register
    let request = Request::builder()
        .method("POST")
        .uri("/_matrix/client/r0/register")
        .header("Content-Type", "application/json")
        .body(Body::from(json!({
            "username": username,
            "password": password,
            "auth": { "type": "m.login.dummy" }
        }).to_string()))
        .unwrap();

    let response = ServiceExt::<Request<Body>>::oneshot(app.clone(), request).await.unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    
    let body = axum::body::to_bytes(response.into_body(), usize::MAX).await.unwrap();
    let json: Value = serde_json::from_slice(&body).unwrap();
    json["access_token"].as_str().unwrap().to_string()
}

#[tokio::test]
async fn test_voice_config_endpoint() {
    let Some(app) = setup_test_app().await else { return };

    let response = app.clone()
        .oneshot(
            Request::builder()
                .uri("/_matrix/client/r0/voice/config")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);

    let body = axum::body::to_bytes(response.into_body(), usize::MAX).await.unwrap();
    let json: Value = serde_json::from_slice(&body).unwrap();

    assert!(json.get("supported_formats").is_some());
    assert_eq!(json["default_sample_rate"], 48000);
}

#[tokio::test]
async fn test_voice_convert_endpoint() {
    let Some(app) = setup_test_app().await else { return };
    let token = create_test_user(&app).await;

    let response = app.clone()
        .oneshot(
            Request::builder()
                .uri("/_matrix/client/r0/voice/convert")
                .method("POST")
                .header("Authorization", format!("Bearer {}", token))
                .header("Content-Type", "application/json")
                .body(Body::from(json!({
                    "content": "base64data",
                    "target_format": "mp3"
                }).to_string()))
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
    
    let body = axum::body::to_bytes(response.into_body(), usize::MAX).await.unwrap();
    let json: Value = serde_json::from_slice(&body).unwrap();
    
    assert_eq!(json["status"], "success");
}

#[tokio::test]
async fn test_voice_optimize_endpoint() {
    let Some(app) = setup_test_app().await else { return };
    let token = create_test_user(&app).await;

    let response = app.clone()
        .oneshot(
            Request::builder()
                .uri("/_matrix/client/r0/voice/optimize")
                .method("POST")
                .header("Authorization", format!("Bearer {}", token))
                .header("Content-Type", "application/json")
                .body(Body::from(json!({
                    "content": "base64data"
                }).to_string()))
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
    
    let body = axum::body::to_bytes(response.into_body(), usize::MAX).await.unwrap();
    let json: Value = serde_json::from_slice(&body).unwrap();
    
    assert_eq!(json["status"], "success");
}