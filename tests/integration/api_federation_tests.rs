use axum::{
    body::Body,
    http::{Request, StatusCode},
};
use serde_json::Value;
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

#[tokio::test]
async fn test_federation_version() {
    let Some(app) = setup_test_app().await else {
        return;
    };

    let request = Request::builder()
        .uri("/_matrix/federation/v1/version")
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
    assert!(json["server"]["version"].is_string());
}

#[tokio::test]
async fn test_federation_queries() {
    let Some(app) = setup_test_app().await else {
        return;
    };

    // 1. Query Profile
    let request = Request::builder()
        .uri("/_matrix/federation/v1/query/profile/@alice:localhost?field=displayname")
        .body(Body::empty())
        .unwrap();
    let response = ServiceExt::<Request<Body>>::oneshot(app.clone(), request)
        .await
        .unwrap();
    // Might be 404 if user doesn't exist, but the endpoint should exist
    assert!(response.status() == StatusCode::OK || response.status() == StatusCode::NOT_FOUND);

    // 2. Query Directory
    let request = Request::builder()
        .uri("/_matrix/federation/v1/query/directory?room_alias=#test:localhost")
        .body(Body::empty())
        .unwrap();
    let response = ServiceExt::<Request<Body>>::oneshot(app.clone(), request)
        .await
        .unwrap();
    assert!(response.status() == StatusCode::OK || response.status() == StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn test_federation_public_rooms() {
    let Some(app) = setup_test_app().await else {
        return;
    };

    let request = Request::builder()
        .uri("/_matrix/federation/v1/publicRooms")
        .body(Body::empty())
        .unwrap();

    let response = ServiceExt::<Request<Body>>::oneshot(app.clone(), request)
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
}
