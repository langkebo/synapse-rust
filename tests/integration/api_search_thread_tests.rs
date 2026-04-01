use axum::{
    body::Body,
    http::{Request, StatusCode},
};
use serde_json::{json, Value};
use tower::ServiceExt;

#[tokio::test]
async fn test_v3_search_validation_is_preserved_after_router_refactor() {
    let Some(app) = super::setup_test_app().await else {
        return;
    };
    let token = super::create_test_user(&app).await;

    let request = Request::builder()
        .method("POST")
        .uri("/_matrix/client/v3/search")
        .header("Authorization", format!("Bearer {}", token))
        .header("Content-Type", "application/json")
        .body(Body::from(
            json!({
                "search_categories": {
                    "room_events": {
                        "search_term": "   ",
                        "keys": ["content.body"]
                    }
                }
            })
            .to_string(),
        ))
        .unwrap();

    let response = ServiceExt::<Request<Body>>::oneshot(app.clone(), request)
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::BAD_REQUEST);

    let body = axum::body::to_bytes(response.into_body(), 1024)
        .await
        .unwrap();
    let json: Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(json["error"], "Search term cannot be empty");
}

#[tokio::test]
async fn test_r0_search_recipients_route_still_works_after_nesting() {
    let Some(app) = super::setup_test_app().await else {
        return;
    };
    let token = super::create_test_user(&app).await;

    let request = Request::builder()
        .method("POST")
        .uri("/_matrix/client/r0/search_recipients")
        .header("Authorization", format!("Bearer {}", token))
        .header("Content-Type", "application/json")
        .body(Body::from(
            json!({
                "search_term": "   "
            })
            .to_string(),
        ))
        .unwrap();

    let response = ServiceExt::<Request<Body>>::oneshot(app.clone(), request)
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::BAD_REQUEST);

    let body = axum::body::to_bytes(response.into_body(), 1024)
        .await
        .unwrap();
    let json: Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(json["error"], "Search term cannot be empty");
}

#[tokio::test]
async fn test_thread_compat_route_is_available_from_thread_module() {
    let Some(app) = super::setup_test_app().await else {
        return;
    };

    let request = Request::builder()
        .method("GET")
        .uri("/_matrix/client/v3/user/@alice:localhost/rooms/!room:localhost/threads")
        .body(Body::empty())
        .unwrap();

    let response = ServiceExt::<Request<Body>>::oneshot(app, request)
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
}
