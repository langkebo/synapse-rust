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

async fn register_user(app: &axum::Router, username: &str) -> String {
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

    let response = ServiceExt::<Request<Body>>::oneshot(app.clone(), request)
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);

    let body = axum::body::to_bytes(response.into_body(), 1024)
        .await
        .unwrap();
    let json: Value = serde_json::from_slice(&body).unwrap();
    json["access_token"].as_str().unwrap().to_string()
}

async fn create_room(app: &axum::Router, token: &str) -> String {
    let request = Request::builder()
        .method("POST")
        .uri("/_matrix/client/v3/createRoom")
        .header("Authorization", format!("Bearer {}", token))
        .header("Content-Type", "application/json")
        .body(Body::from(json!({"name": "Widget Room"}).to_string()))
        .unwrap();

    let response = ServiceExt::<Request<Body>>::oneshot(app.clone(), request)
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);

    let body = axum::body::to_bytes(response.into_body(), 1024)
        .await
        .unwrap();
    let json: Value = serde_json::from_slice(&body).unwrap();
    json["room_id"].as_str().unwrap().to_string()
}

#[tokio::test]
async fn test_create_widget_succeeds_for_existing_room() {
    let Some(app) = setup_test_app().await else {
        return;
    };

    let token = register_user(&app, &format!("widget_user_{}", rand::random::<u32>())).await;
    let room_id = create_room(&app, &token).await;

    let create_widget_request = Request::builder()
        .method("POST")
        .uri("/_matrix/client/v1/widgets")
        .header("Authorization", format!("Bearer {}", token))
        .header("Content-Type", "application/json")
        .body(Body::from(
            json!({
                "room_id": room_id,
                "widget_type": "m.custom",
                "url": "https://example.com/widget",
                "name": "Test Widget",
                "data": { "source": "integration-test" }
            })
            .to_string(),
        ))
        .unwrap();

    let create_widget_response =
        ServiceExt::<Request<Body>>::oneshot(app.clone(), create_widget_request)
            .await
            .unwrap();
    assert_eq!(create_widget_response.status(), StatusCode::OK);

    let body = axum::body::to_bytes(create_widget_response.into_body(), 2048)
        .await
        .unwrap();
    let json: Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(json["widget"]["room_id"], room_id);
    assert_eq!(json["widget"]["name"], "Test Widget");
}

#[tokio::test]
async fn test_create_widget_returns_not_found_for_missing_room() {
    let Some(app) = setup_test_app().await else {
        return;
    };

    let token = register_user(&app, &format!("widget_missing_{}", rand::random::<u32>())).await;

    let create_widget_request = Request::builder()
        .method("POST")
        .uri("/_matrix/client/v1/widgets")
        .header("Authorization", format!("Bearer {}", token))
        .header("Content-Type", "application/json")
        .body(Body::from(
            json!({
                "room_id": "!missing:localhost",
                "widget_type": "m.custom",
                "url": "https://example.com/widget",
                "name": "Broken Widget"
            })
            .to_string(),
        ))
        .unwrap();

    let create_widget_response =
        ServiceExt::<Request<Body>>::oneshot(app, create_widget_request)
            .await
            .unwrap();
    assert_eq!(create_widget_response.status(), StatusCode::NOT_FOUND);
}
