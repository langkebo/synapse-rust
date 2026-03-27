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

#[tokio::test]
async fn test_room_summary_read_routes_share_across_versions() {
    let Some(app) = setup_test_app().await else {
        return;
    };
    let token = register_user(&app, "room_summary_routes_shared").await;
    let room_id = format!("!room-summary-shared-{}:localhost", rand::random::<u32>());

    let create_request = Request::builder()
        .method("POST")
        .uri(format!("/_matrix/client/v3/rooms/{}/summary", room_id))
        .header("Authorization", format!("Bearer {}", token))
        .header("Content-Type", "application/json")
        .body(Body::from(
            json!({
                "room_id": room_id,
                "name": "Shared summary room",
                "topic": "room summary route test",
                "is_space": false
            })
            .to_string(),
        ))
        .unwrap();

    let create_response = ServiceExt::<Request<Body>>::oneshot(app.clone(), create_request)
        .await
        .unwrap();
    assert_eq!(create_response.status(), StatusCode::CREATED);

    let v3_get_request = Request::builder()
        .method("GET")
        .uri(format!("/_matrix/client/v3/rooms/{}/summary", room_id))
        .body(Body::empty())
        .unwrap();
    let v3_get_response = ServiceExt::<Request<Body>>::oneshot(app.clone(), v3_get_request)
        .await
        .unwrap();
    assert_eq!(v3_get_response.status(), StatusCode::OK);

    let body = axum::body::to_bytes(v3_get_response.into_body(), 2048)
        .await
        .unwrap();
    let json: Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(json["room_id"], room_id);
    assert_eq!(json["name"], "Shared summary room");

    let r0_get_request = Request::builder()
        .method("GET")
        .uri(format!("/_matrix/client/r0/rooms/{}/summary", room_id))
        .body(Body::empty())
        .unwrap();
    let r0_get_response = ServiceExt::<Request<Body>>::oneshot(app, r0_get_request)
        .await
        .unwrap();
    assert_eq!(r0_get_response.status(), StatusCode::OK);

    let body = axum::body::to_bytes(r0_get_response.into_body(), 2048)
        .await
        .unwrap();
    let json: Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(json["room_id"], room_id);
    assert_eq!(json["topic"], "room summary route test");
}

#[tokio::test]
async fn test_room_summary_route_boundaries_are_preserved() {
    let Some(app) = setup_test_app().await else {
        return;
    };
    let room_id = "!room-summary-boundary:localhost";

    let r0_write_request = Request::builder()
        .method("POST")
        .uri(format!("/_matrix/client/r0/rooms/{}/summary", room_id))
        .header("Content-Type", "application/json")
        .body(Body::from(
            json!({
                "room_id": room_id,
                "name": "should not exist on r0"
            })
            .to_string(),
        ))
        .unwrap();
    let r0_write_response = ServiceExt::<Request<Body>>::oneshot(app.clone(), r0_write_request)
        .await
        .unwrap();
    assert_eq!(r0_write_response.status(), StatusCode::METHOD_NOT_ALLOWED);

    let v3_unread_request = Request::builder()
        .method("POST")
        .uri(format!(
            "/_matrix/client/v3/rooms/{}/summary/unread/clear",
            room_id
        ))
        .body(Body::empty())
        .unwrap();
    let v3_unread_response = ServiceExt::<Request<Body>>::oneshot(app.clone(), v3_unread_request)
        .await
        .unwrap();
    assert_eq!(v3_unread_response.status(), StatusCode::UNAUTHORIZED);

    let r0_unread_request = Request::builder()
        .method("POST")
        .uri(format!(
            "/_matrix/client/r0/rooms/{}/summary/unread/clear",
            room_id
        ))
        .body(Body::empty())
        .unwrap();
    let r0_unread_response = ServiceExt::<Request<Body>>::oneshot(app.clone(), r0_unread_request)
        .await
        .unwrap();
    assert_eq!(r0_unread_response.status(), StatusCode::NOT_FOUND);

    let synapse_request = Request::builder()
        .method("GET")
        .uri("/_synapse/room_summary/v1/summaries")
        .body(Body::empty())
        .unwrap();
    let synapse_response = ServiceExt::<Request<Body>>::oneshot(app, synapse_request)
        .await
        .unwrap();
    assert_eq!(synapse_response.status(), StatusCode::UNAUTHORIZED);
}
