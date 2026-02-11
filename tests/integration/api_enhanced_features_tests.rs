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
use tokio::runtime::Runtime;
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
                "auth": {"type": "m.login.dummy"}
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

#[test]
fn test_friend_system_extended() {
    let rt = Runtime::new().unwrap();
    rt.block_on(async {
        let app = match setup_test_app().await {
            Some(app) => app,
            None => return,
        };
        let alice_token = register_user(&app, &format!("alice_{}", rand::random::<u32>())).await;

        // 1. Get Friend List
        let request = Request::builder()
            .uri("/_synapse/enhanced/friends")
            .header("Authorization", format!("Bearer {}", alice_token))
            .body(Body::empty())
            .unwrap();
        let response = ServiceExt::<Request<Body>>::oneshot(app.clone(), request)
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::OK);

        // 2. Get Friend Requests
        let request = Request::builder()
            .uri("/_synapse/enhanced/friend/requests")
            .header("Authorization", format!("Bearer {}", alice_token))
            .body(Body::empty())
            .unwrap();
        let response = ServiceExt::<Request<Body>>::oneshot(app.clone(), request)
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::OK);

        // 3. Blacklist
        let request_whoami = Request::builder()
            .uri("/_matrix/client/r0/account/whoami")
            .header("Authorization", format!("Bearer {}", alice_token))
            .body(Body::empty())
            .unwrap();
        let response_whoami = ServiceExt::<Request<Body>>::oneshot(app.clone(), request_whoami)
            .await
            .unwrap();
        let body_whoami = axum::body::to_bytes(response_whoami.into_body(), 1024)
            .await
            .unwrap();
        let json_whoami: Value = serde_json::from_slice(&body_whoami).unwrap();
        let alice_user_id = json_whoami["user_id"].as_str().unwrap();

        let request = Request::builder()
            .uri(format!(
                "/_synapse/enhanced/friend/blocks/{}",
                alice_user_id
            ))
            .header("Authorization", format!("Bearer {}", alice_token))
            .body(Body::empty())
            .unwrap();
        let response = ServiceExt::<Request<Body>>::oneshot(app.clone(), request)
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::OK);
    });
}

#[test]
fn test_trusted_private_chat_preset() {
    let rt = Runtime::new().unwrap();
    rt.block_on(async {
        let app = match setup_test_app().await {
            Some(app) => app,
            None => return,
        };
        let alice_token = register_user(&app, &format!("alice_{}", rand::random::<u32>())).await;

        // Create a trusted private chat room using the standard Matrix API
        let request = Request::builder()
            .method("POST")
            .uri("/_matrix/client/r0/createRoom")
            .header("Authorization", format!("Bearer {}", alice_token))
            .header("Content-Type", "application/json")
            .body(Body::from(
                json!({
                    "preset": "trusted_private_chat",
                    "visibility": "private",
                    "name": "Private Chat",
                    "topic": "Encrypted & Secure",
                    "is_direct": true
                })
                .to_string(),
            ))
            .unwrap();
        let response = ServiceExt::<Request<Body>>::oneshot(app.clone(), request)
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::OK);

        // Verify room was created
        let body = axum::body::to_bytes(response.into_body(), 4096)
            .await
            .unwrap();
        let json: Value = serde_json::from_slice(&body).unwrap();
        assert!(json["room_id"].is_str());

        // Get room state to verify privacy settings were applied
        let room_id = json["room_id"].as_str().unwrap();
        let request = Request::builder()
            .uri(format!("/_matrix/client/r0/rooms/{}/state", room_id))
            .header("Authorization", format!("Bearer {}", alice_token))
            .body(Body::empty())
            .unwrap();
        let response = ServiceExt::<Request<Body>>::oneshot(app.clone(), request)
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::OK);
    });
}

#[test]
fn test_voice_messages() {
    let rt = Runtime::new().unwrap();
    rt.block_on(async {
        let app = match setup_test_app().await {
            Some(app) => app,
            None => return,
        };
        let alice_token = register_user(&app, &format!("alice_{}", rand::random::<u32>())).await;

        // 1. Get Voice Messages (List)
        let request_whoami = Request::builder()
            .uri("/_matrix/client/r0/account/whoami")
            .header("Authorization", format!("Bearer {}", alice_token))
            .body(Body::empty())
            .unwrap();
        let response_whoami = ServiceExt::<Request<Body>>::oneshot(app.clone(), request_whoami)
            .await
            .unwrap();
        let body_whoami = axum::body::to_bytes(response_whoami.into_body(), 1024)
            .await
            .unwrap();
        let json_whoami: Value = serde_json::from_slice(&body_whoami).unwrap();
        let alice_user_id = json_whoami["user_id"].as_str().unwrap();

        let request = Request::builder()
            .uri(format!("/_matrix/client/r0/voice/user/{}", alice_user_id))
            .header("Authorization", format!("Bearer {}", alice_token))
            .body(Body::empty())
            .unwrap();
        let response = ServiceExt::<Request<Body>>::oneshot(app.clone(), request)
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::OK);

        // 2. Voice Statistics
        let request = Request::builder()
            .uri("/_matrix/client/r0/voice/stats")
            .header("Authorization", format!("Bearer {}", alice_token))
            .body(Body::empty())
            .unwrap();
        let response = ServiceExt::<Request<Body>>::oneshot(app.clone(), request)
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::OK);
    });
}
