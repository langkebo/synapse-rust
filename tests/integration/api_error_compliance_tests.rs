use axum::{
    body::Body,
    http::{Request, StatusCode},
};
use serde_json::Value;
use tower::ServiceExt;

async fn setup_test_app() -> Option<axum::Router> {
    super::setup_test_app().await
}

/// Task 1.1: Unknown path → 404 JSON with M_UNRECOGNIZED
#[tokio::test]
async fn test_fallback_unknown_path_returns_404_json() {
    let app = setup_test_app().await.expect("test app should start");

    let request = Request::builder()
        .method("GET")
        .uri("/_matrix/client/v3/this-path-does-not-exist-12345")
        .body(Body::empty())
        .unwrap();

    let response = ServiceExt::<Request<Body>>::oneshot(app, request).await.unwrap();

    assert_eq!(response.status(), StatusCode::NOT_FOUND);

    let content_type = response
        .headers()
        .get("content-type")
        .and_then(|v| v.to_str().ok())
        .unwrap_or("");
    assert!(content_type.contains("application/json"), "expected application/json, got: {content_type}");

    let body = axum::body::to_bytes(response.into_body(), 1024).await.unwrap();
    let json: Value = serde_json::from_slice(&body).expect("body should be valid JSON");
    assert_eq!(json["errcode"], "M_UNRECOGNIZED");
}

/// Task 1.4: Method-not-allowed → 405 JSON with M_UNRECOGNIZED
#[tokio::test]
async fn test_method_not_allowed_returns_405_json() {
    let app = setup_test_app().await.expect("test app should start");

    // /health only accepts GET; POST should trigger 405
    let request = Request::builder()
        .method("POST")
        .uri("/health")
        .header("content-type", "application/json")
        .body(Body::from("{}"))
        .unwrap();

    let response = ServiceExt::<Request<Body>>::oneshot(app, request).await.unwrap();

    assert_eq!(response.status(), StatusCode::METHOD_NOT_ALLOWED);

    let content_type = response
        .headers()
        .get("content-type")
        .and_then(|v| v.to_str().ok())
        .unwrap_or("");
    assert!(content_type.contains("application/json"), "expected application/json, got: {content_type}");

    let body = axum::body::to_bytes(response.into_body(), 1024).await.unwrap();
    let json: Value = serde_json::from_slice(&body).expect("body should be valid JSON");
    assert_eq!(json["errcode"], "M_UNRECOGNIZED");
    assert!(json["error"].as_str().unwrap_or("").contains("Method not allowed"),
        "error message should mention method not allowed");
}
