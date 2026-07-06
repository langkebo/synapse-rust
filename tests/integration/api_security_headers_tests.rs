use axum::{
    body::Body,
    http::{Request, StatusCode},
};
use tower::ServiceExt;

async fn setup_test_app() -> Option<axum::Router> {
    super::setup_fresh_test_app().await
}

/// Task 2.1: X-Content-Type-Options: nosniff on API responses
#[tokio::test]
async fn test_security_header_x_content_type_options_nosniff() {
    let app = setup_test_app().await.expect("test app should start");

    let request = Request::builder().method("GET").uri("/health").body(Body::empty()).unwrap();

    let response = ServiceExt::<Request<Body>>::oneshot(app, request).await.unwrap();
    assert_eq!(response.status(), StatusCode::OK);

    let header_value = response
        .headers()
        .get("x-content-type-options")
        .and_then(|v| v.to_str().ok())
        .expect("X-Content-Type-Options header must be present");
    assert_eq!(header_value, "nosniff");
}

/// Task 2.2: Referrer-Policy: strict-origin-when-cross-origin on API responses
#[tokio::test]
async fn test_security_header_referrer_policy() {
    let app = setup_test_app().await.expect("test app should start");

    let request = Request::builder().method("GET").uri("/health").body(Body::empty()).unwrap();

    let response = ServiceExt::<Request<Body>>::oneshot(app, request).await.unwrap();
    assert_eq!(response.status(), StatusCode::OK);

    let header_value = response
        .headers()
        .get("referrer-policy")
        .and_then(|v| v.to_str().ok())
        .expect("Referrer-Policy header must be present");
    assert_eq!(header_value, "strict-origin-when-cross-origin");
}

/// Task 2.3: Full security header set on API responses
#[tokio::test]
async fn test_security_header_full_set() {
    let app = setup_test_app().await.expect("test app should start");

    let request = Request::builder().method("GET").uri("/health").body(Body::empty()).unwrap();

    let response = ServiceExt::<Request<Body>>::oneshot(app, request).await.unwrap();
    assert_eq!(response.status(), StatusCode::OK);

    let headers = response.headers();

    // Content-Security-Policy
    let csp = headers
        .get("content-security-policy")
        .and_then(|v| v.to_str().ok())
        .expect("Content-Security-Policy header must be present");
    assert!(csp.contains("default-src 'none'"));
    assert!(csp.contains("frame-src 'none'"));

    // Permissions-Policy
    let permissions = headers
        .get("permissions-policy")
        .and_then(|v| v.to_str().ok())
        .expect("Permissions-Policy header must be present");
    assert!(permissions.contains("camera=()"));
    assert!(permissions.contains("microphone=()"));

    // X-Content-Type-Options
    let nosniff = headers
        .get("x-content-type-options")
        .and_then(|v| v.to_str().ok())
        .expect("X-Content-Type-Options header must be present");
    assert_eq!(nosniff, "nosniff");

    // Referrer-Policy
    let referrer =
        headers.get("referrer-policy").and_then(|v| v.to_str().ok()).expect("Referrer-Policy header must be present");
    assert_eq!(referrer, "strict-origin-when-cross-origin");

    // HSTS (enabled by default with max-age=31536000)
    let hsts = headers
        .get("strict-transport-security")
        .and_then(|v| v.to_str().ok())
        .expect("Strict-Transport-Security header must be present");
    assert!(hsts.contains("max-age=31536000"));
}
