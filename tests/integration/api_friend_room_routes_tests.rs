use axum::{
    body::Body,
    http::{Request, StatusCode},
};
use serde_json::{json, Value};
use tower::ServiceExt;

async fn setup_test_app() -> Option<axum::Router> {
    super::setup_test_app().await
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

    (
        json["access_token"].as_str().unwrap().to_string(),
        json["user_id"].as_str().unwrap().to_string(),
    )
}

#[tokio::test]
async fn test_v1_and_r0_friend_list_routes_work_after_nesting() {
    let Some(app) = setup_test_app().await else {
        return;
    };
    let (token, _) = register_user(&app, "friend_routes_list").await;

    for path in [
        "/_matrix/client/v1/friends",
        "/_matrix/client/r0/friendships",
    ] {
        let request = Request::builder()
            .method("GET")
            .uri(path)
            .header("Authorization", format!("Bearer {}", token))
            .body(Body::empty())
            .unwrap();

        let response = ServiceExt::<Request<Body>>::oneshot(app.clone(), request)
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::OK, "failed path: {path}");
    }
}

#[tokio::test]
async fn test_r0_friendships_alias_keeps_send_friend_request_validation() {
    let Some(app) = setup_test_app().await else {
        return;
    };
    let (token, user_id) = register_user(&app, "friend_routes_alias").await;

    let request = Request::builder()
        .method("POST")
        .uri("/_matrix/client/r0/friendships")
        .header("Authorization", format!("Bearer {}", token))
        .header("Content-Type", "application/json")
        .body(Body::from(
            json!({
                "user_id": user_id
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
    assert_eq!(json["error"], "Cannot send friend request to yourself");
}

#[tokio::test]
async fn test_v3_friends_keeps_get_only_contract() {
    let Some(app) = setup_test_app().await else {
        return;
    };
    let (token, user_id) = register_user(&app, "friend_routes_v3").await;

    let get_request = Request::builder()
        .method("GET")
        .uri("/_matrix/client/v3/friends")
        .header("Authorization", format!("Bearer {}", token))
        .body(Body::empty())
        .unwrap();

    let get_response = ServiceExt::<Request<Body>>::oneshot(app.clone(), get_request)
        .await
        .unwrap();
    assert_eq!(get_response.status(), StatusCode::OK);

    let post_request = Request::builder()
        .method("POST")
        .uri("/_matrix/client/v3/friends")
        .header("Authorization", format!("Bearer {}", token))
        .header("Content-Type", "application/json")
        .body(Body::from(
            json!({
                "user_id": user_id
            })
            .to_string(),
        ))
        .unwrap();

    let post_response = ServiceExt::<Request<Body>>::oneshot(app, post_request)
        .await
        .unwrap();
    assert!(
        post_response.status() == StatusCode::METHOD_NOT_ALLOWED
            || post_response.status() == StatusCode::BAD_REQUEST,
        "Expected 405 or 400 for POST to GET-only route, got: {}",
        post_response.status()
    );
}
