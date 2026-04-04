use axum::body::Body;
use hyper::{Request, StatusCode};
use serde_json::json;
use tower::ServiceExt;

use crate::setup_test_app;

#[tokio::test]
async fn test_appservice_routes_exist() {
    let Some(app) = setup_test_app().await else {
        return;
    };

    // Test that the list endpoint exists (will fail auth but route should exist)
    let list_request = Request::builder()
        .method("GET")
        .uri("/_synapse/admin/v1/appservices")
        .body(Body::empty())
        .unwrap();

    let response = app.clone().oneshot(list_request).await.unwrap();
    // Should get 401 Unauthorized, not 404 Not Found
    assert_ne!(
        response.status(),
        StatusCode::NOT_FOUND,
        "AppService list route should exist"
    );
}

#[tokio::test]
async fn test_appservice_register_requires_auth() {
    let Some(app) = setup_test_app().await else {
        return;
    };

    let as_id = format!("test_as_{}", rand::random::<u32>());
    let register_request = Request::builder()
        .method("POST")
        .uri("/_synapse/admin/v1/appservices")
        .header("Content-Type", "application/json")
        .body(Body::from(
            json!({
                "id": &as_id,
                "url": "http://localhost:8080",
                "as_token": "test_token",
                "hs_token": "test_hs_token",
                "sender_localpart": "bot_test"
            })
            .to_string(),
        ))
        .unwrap();

    let response = app.clone().oneshot(register_request).await.unwrap();
    // Should require authentication
    assert_eq!(
        response.status(),
        StatusCode::UNAUTHORIZED,
        "AppService registration should require authentication"
    );
}
