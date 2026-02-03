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

async fn setup_test_app() -> axum::Router {
    let container = ServiceContainer::new_test();
    let cache = Arc::new(CacheManager::new(CacheConfig::default()));
    let state = AppState::new(container, cache);
    create_router(state)
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

    let status = response.status();
    if status != StatusCode::OK {
        let body = axum::body::to_bytes(response.into_body(), 1024)
            .await
            .unwrap();
        panic!(
            "Registration failed with status {}: {:?}",
            status,
            String::from_utf8_lossy(&body)
        );
    }

    let body = axum::body::to_bytes(response.into_body(), 1024)
        .await
        .unwrap();
    let json: Value = serde_json::from_slice(&body).unwrap();
    json["access_token"].as_str().unwrap().to_string()
}

#[tokio::test]
async fn test_room_lifecycle() {
    let app = setup_test_app().await;
    let alice_token = register_user(&app, &format!("alice_{}", rand::random::<u32>())).await;
    let bob_token = register_user(&app, &format!("bob_{}", rand::random::<u32>())).await;

    // 1. Create Room
    let request = Request::builder()
        .method("POST")
        .uri("/_matrix/client/r0/createRoom")
        .header("Authorization", format!("Bearer {}", alice_token))
        .header("Content-Type", "application/json")
        .body(Body::from(
            json!({
                "name": "Alice's Room",
                "topic": "Testing room lifecycle",
                "visibility": "public"
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
    let room_id = json["room_id"].as_str().unwrap().to_string();

    // 2. Invite Bob
    let request_whoami = Request::builder()
        .uri("/_matrix/client/r0/account/whoami")
        .header("Authorization", format!("Bearer {}", bob_token))
        .body(Body::empty())
        .unwrap();
    let response_whoami = ServiceExt::<Request<Body>>::oneshot(app.clone(), request_whoami)
        .await
        .unwrap();
    let body_whoami = axum::body::to_bytes(response_whoami.into_body(), 1024)
        .await
        .unwrap();
    let json_whoami: Value = serde_json::from_slice(&body_whoami).unwrap();
    let bob_user_id = json_whoami["user_id"].as_str().unwrap();

    let request = Request::builder()
        .method("POST")
        .uri(format!("/_matrix/client/r0/rooms/{}/invite", room_id))
        .header("Authorization", format!("Bearer {}", alice_token))
        .header("Content-Type", "application/json")
        .body(Body::from(
            json!({
                "user_id": bob_user_id
            })
            .to_string(),
        ))
        .unwrap();

    let response = ServiceExt::<Request<Body>>::oneshot(app.clone(), request)
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);

    // 3. Bob Joins Room
    let request = Request::builder()
        .method("POST")
        .uri(format!("/_matrix/client/r0/rooms/{}/join", room_id))
        .header("Authorization", format!("Bearer {}", bob_token))
        .body(Body::empty())
        .unwrap();

    let response = ServiceExt::<Request<Body>>::oneshot(app.clone(), request)
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);

    // 4. Send Message
    let request = Request::builder()
        .method("PUT")
        .uri(format!(
            "/_matrix/client/r0/rooms/{}/send/m.room.message/txn1",
            room_id
        ))
        .header("Authorization", format!("Bearer {}", bob_token))
        .header("Content-Type", "application/json")
        .body(Body::from(
            json!({
                "msgtype": "m.text",
                "body": "Hello Alice!"
            })
            .to_string(),
        ))
        .unwrap();

    let response = ServiceExt::<Request<Body>>::oneshot(app.clone(), request)
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);

    // 5. Get Members
    let request = Request::builder()
        .uri(format!("/_matrix/client/r0/rooms/{}/members", room_id))
        .header("Authorization", format!("Bearer {}", alice_token))
        .body(Body::empty())
        .unwrap();

    let response = ServiceExt::<Request<Body>>::oneshot(app.clone(), request)
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    let body = axum::body::to_bytes(response.into_body(), 10240)
        .await
        .unwrap();
    let json: Value = serde_json::from_slice(&body).unwrap();
    assert!(json["chunk"].as_array().unwrap().len() >= 2);

    // 6. Get Messages
    let request = Request::builder()
        .uri(format!(
            "/_matrix/client/r0/rooms/{}/messages?limit=10",
            room_id
        ))
        .header("Authorization", format!("Bearer {}", alice_token))
        .body(Body::empty())
        .unwrap();

    let response = ServiceExt::<Request<Body>>::oneshot(app.clone(), request)
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    let body = axum::body::to_bytes(response.into_body(), 10240)
        .await
        .unwrap();
    let json: Value = serde_json::from_slice(&body).unwrap();
    assert!(!json["chunk"].as_array().unwrap().is_empty());

    // 7. Leave Room
    let request = Request::builder()
        .method("POST")
        .uri(format!("/_matrix/client/r0/rooms/{}/leave", room_id))
        .header("Authorization", format!("Bearer {}", bob_token))
        .body(Body::empty())
        .unwrap();

    let response = ServiceExt::<Request<Body>>::oneshot(app.clone(), request)
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);
}

#[tokio::test]
async fn test_room_directory_and_public_rooms() {
    let app = setup_test_app().await;
    let alice_token = register_user(&app, &format!("alice_{}", rand::random::<u32>())).await;

    // 1. Create Public Room
    let request = Request::builder()
        .method("POST")
        .uri("/_matrix/client/r0/createRoom")
        .header("Authorization", format!("Bearer {}", alice_token))
        .header("Content-Type", "application/json")
        .body(Body::from(
            json!({
                "name": "Public Room",
                "visibility": "public"
            })
            .to_string(),
        ))
        .unwrap();

    let response = ServiceExt::<Request<Body>>::oneshot(app.clone(), request)
        .await
        .unwrap();
    let body = axum::body::to_bytes(response.into_body(), 1024)
        .await
        .unwrap();
    let json: Value = serde_json::from_slice(&body).unwrap();
    let room_id = json["room_id"].as_str().unwrap().to_string();

    // 2. Get Public Rooms
    let request = Request::builder()
        .uri("/_matrix/client/r0/publicRooms")
        .header("Authorization", format!("Bearer {}", alice_token))
        .body(Body::empty())
        .unwrap();

    let response = ServiceExt::<Request<Body>>::oneshot(app.clone(), request)
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    let body = axum::body::to_bytes(response.into_body(), 10240)
        .await
        .unwrap();
    let json: Value = serde_json::from_slice(&body).unwrap();
    assert!(json["chunk"]
        .as_array()
        .unwrap()
        .iter()
        .any(|r| r["room_id"] == room_id));

    // 3. Get Room Info (Directory)
    let request = Request::builder()
        .uri(format!("/_matrix/client/r0/directory/room/{}", room_id))
        .header("Authorization", format!("Bearer {}", alice_token))
        .body(Body::empty())
        .unwrap();

    let response = ServiceExt::<Request<Body>>::oneshot(app.clone(), request)
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);
}

#[tokio::test]
async fn test_room_state_and_redaction() {
    let app = setup_test_app().await;
    let alice_token = register_user(&app, &format!("alice_{}", rand::random::<u32>())).await;

    let request = Request::builder()
        .method("POST")
        .uri("/_matrix/client/r0/createRoom")
        .header("Authorization", format!("Bearer {}", alice_token))
        .header("Content-Type", "application/json")
        .body(Body::from(json!({"name": "State Room"}).to_string()))
        .unwrap();
    let response = ServiceExt::<Request<Body>>::oneshot(app.clone(), request)
        .await
        .unwrap();
    let body = axum::body::to_bytes(response.into_body(), 1024)
        .await
        .unwrap();
    let json: Value = serde_json::from_slice(&body).unwrap();
    let room_id = json["room_id"].as_str().unwrap().to_string();

    // 1. Get Room State
    let request = Request::builder()
        .uri(format!("/_matrix/client/r0/rooms/{}/state", room_id))
        .header("Authorization", format!("Bearer {}", alice_token))
        .body(Body::empty())
        .unwrap();
    let response = ServiceExt::<Request<Body>>::oneshot(app.clone(), request)
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);

    // 2. Send Message and then Redact it
    let request = Request::builder()
        .method("PUT")
        .uri(format!(
            "/_matrix/client/r0/rooms/{}/send/m.room.message/txn1",
            room_id
        ))
        .header("Authorization", format!("Bearer {}", alice_token))
        .header("Content-Type", "application/json")
        .body(Body::from(
            json!({"msgtype": "m.text", "body": "To be redacted"}).to_string(),
        ))
        .unwrap();
    let response = ServiceExt::<Request<Body>>::oneshot(app.clone(), request)
        .await
        .unwrap();
    let body = axum::body::to_bytes(response.into_body(), 1024)
        .await
        .unwrap();
    let json: Value = serde_json::from_slice(&body).unwrap();
    let event_id = json["event_id"].as_str().unwrap().to_string();

    let request = Request::builder()
        .method("PUT")
        .uri(format!(
            "/_matrix/client/r0/rooms/{}/redact/{}",
            room_id, event_id
        ))
        .header("Authorization", format!("Bearer {}", alice_token))
        .header("Content-Type", "application/json")
        .body(Body::from(json!({"reason": "Test redaction"}).to_string()))
        .unwrap();
    let response = ServiceExt::<Request<Body>>::oneshot(app.clone(), request)
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);
}

#[tokio::test]
async fn test_room_moderation() {
    let app = setup_test_app().await;
    let alice_token = register_user(&app, &format!("alice_{}", rand::random::<u32>())).await;
    let bob_token = register_user(&app, &format!("bob_{}", rand::random::<u32>())).await;

    // 1. Create Room
    let request = Request::builder()
        .method("POST")
        .uri("/_matrix/client/r0/createRoom")
        .header("Authorization", format!("Bearer {}", alice_token))
        .header("Content-Type", "application/json")
        .body(Body::from(json!({"name": "Moderation Room"}).to_string()))
        .unwrap();
    let response = ServiceExt::<Request<Body>>::oneshot(app.clone(), request)
        .await
        .unwrap();
    let body = axum::body::to_bytes(response.into_body(), 1024)
        .await
        .unwrap();
    let json: Value = serde_json::from_slice(&body).unwrap();
    let room_id = json["room_id"].as_str().unwrap().to_string();

    // 2. Get Bob's user_id
    let request_whoami = Request::builder()
        .uri("/_matrix/client/r0/account/whoami")
        .header("Authorization", format!("Bearer {}", bob_token))
        .body(Body::empty())
        .unwrap();
    let response_whoami = ServiceExt::<Request<Body>>::oneshot(app.clone(), request_whoami)
        .await
        .unwrap();
    let body_whoami = axum::body::to_bytes(response_whoami.into_body(), 1024)
        .await
        .unwrap();
    let json_whoami: Value = serde_json::from_slice(&body_whoami).unwrap();
    let bob_user_id = json_whoami["user_id"].as_str().unwrap();

    // 3. Bob Joins Room (public or invited - here we just join)
    let request = Request::builder()
        .method("POST")
        .uri(format!("/_matrix/client/r0/rooms/{}/join", room_id))
        .header("Authorization", format!("Bearer {}", bob_token))
        .body(Body::empty())
        .unwrap();
    let response = ServiceExt::<Request<Body>>::oneshot(app.clone(), request)
        .await
        .unwrap();
    // Might fail if not public, but let's assume it works for now or invite him
    if response.status() != StatusCode::OK {
        // Invite first
        let request = Request::builder()
            .method("POST")
            .uri(format!("/_matrix/client/r0/rooms/{}/invite", room_id))
            .header("Authorization", format!("Bearer {}", alice_token))
            .header("Content-Type", "application/json")
            .body(Body::from(json!({"user_id": bob_user_id}).to_string()))
            .unwrap();
        ServiceExt::<Request<Body>>::oneshot(app.clone(), request)
            .await
            .unwrap();

        let request = Request::builder()
            .method("POST")
            .uri(format!("/_matrix/client/r0/rooms/{}/join", room_id))
            .header("Authorization", format!("Bearer {}", bob_token))
            .body(Body::empty())
            .unwrap();
        ServiceExt::<Request<Body>>::oneshot(app.clone(), request)
            .await
            .unwrap();
    }

    // 4. Alice kicks Bob
    let request = Request::builder()
        .method("POST")
        .uri(format!("/_matrix/client/r0/rooms/{}/kick", room_id))
        .header("Authorization", format!("Bearer {}", alice_token))
        .header("Content-Type", "application/json")
        .body(Body::from(
            json!({"user_id": bob_user_id, "reason": "Behave!"}).to_string(),
        ))
        .unwrap();
    let response = ServiceExt::<Request<Body>>::oneshot(app.clone(), request)
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);

    // 5. Alice bans Bob
    let request = Request::builder()
        .method("POST")
        .uri(format!("/_matrix/client/r0/rooms/{}/ban", room_id))
        .header("Authorization", format!("Bearer {}", alice_token))
        .header("Content-Type", "application/json")
        .body(Body::from(
            json!({"user_id": bob_user_id, "reason": "Banned!"}).to_string(),
        ))
        .unwrap();
    let response = ServiceExt::<Request<Body>>::oneshot(app.clone(), request)
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);

    // 6. Alice unbans Bob
    let request = Request::builder()
        .method("POST")
        .uri(format!("/_matrix/client/r0/rooms/{}/unban", room_id))
        .header("Authorization", format!("Bearer {}", alice_token))
        .header("Content-Type", "application/json")
        .body(Body::from(json!({"user_id": bob_user_id}).to_string()))
        .unwrap();
    let response = ServiceExt::<Request<Body>>::oneshot(app.clone(), request)
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);
}
