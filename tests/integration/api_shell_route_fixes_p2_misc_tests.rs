// Integration tests for P2 shell route fixes - DM, Invite control, Rendezvous
// Tests verify remaining P2 operations return confirmation data

use axum::{
    body::Body,
    http::{Request, StatusCode},
};
use serde_json::{json, Value};
use tower::ServiceExt;

async fn setup_test_app() -> Option<axum::Router> {
    super::setup_test_app().await
}

async fn register_user(app: &axum::Router, username: &str) -> (String, String, String) {
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
        json["device_id"].as_str().unwrap().to_string(),
    )
}

async fn create_room(app: &axum::Router, access_token: &str, name: &str) -> String {
    let request = Request::builder()
        .method("POST")
        .uri("/_matrix/client/r0/createRoom")
        .header("Content-Type", "application/json")
        .header("Authorization", format!("Bearer {}", access_token))
        .body(Body::from(
            json!({
                "name": name,
                "preset": "private_chat"
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

    json["room_id"].as_str().unwrap().to_string()
}

async fn create_dm_room(app: &axum::Router, access_token: &str, target_user_id: &str) -> String {
    let request = Request::builder()
        .method("POST")
        .uri("/_matrix/client/r0/createRoom")
        .header("Content-Type", "application/json")
        .header("Authorization", format!("Bearer {}", access_token))
        .body(Body::from(
            json!({
                "is_direct": true,
                "invite": [target_user_id],
                "preset": "trusted_private_chat"
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

    json["room_id"].as_str().unwrap().to_string()
}

#[tokio::test]
async fn test_update_dm_room_returns_confirmation() {
    let Some(app) = setup_test_app().await else {
        return;
    };
    let (alice_token, _alice_id, _) = register_user(&app, "alice").await;
    let (_bob_token, bob_id, _) = register_user(&app, "bob").await;

    // Create DM room
    let room_id = create_dm_room(&app, &alice_token, &bob_id).await;

    // Update DM room mapping
    let request = Request::builder()
        .method("PUT")
        .uri(format!("/_matrix/client/v3/direct/{}", room_id))
        .header("Content-Type", "application/json")
        .header("Authorization", format!("Bearer {}", alice_token))
        .body(Body::from(
            json!({
                "users": [bob_id]
            })
            .to_string(),
        ))
        .unwrap();

    let response = ServiceExt::<Request<Body>>::oneshot(app.clone(), request)
        .await
        .expect("Failed to update DM room");

    assert_eq!(response.status(), StatusCode::OK);

    let body = axum::body::to_bytes(response.into_body(), 1024)
        .await
        .expect("Failed to read response body");
    let body: Value = serde_json::from_slice(&body).expect("Failed to parse response");

    // Verify response contains confirmation data
    assert!(
        body.get("room_id").is_some(),
        "Response should contain room_id"
    );
    assert_eq!(body["room_id"], room_id);
    assert!(body.get("users").is_some(), "Response should contain users");
    assert!(body["users"].is_array(), "users should be an array");
    assert!(
        body.get("updated_ts").is_some(),
        "Response should contain updated_ts"
    );
    assert!(
        body["updated_ts"].is_number(),
        "updated_ts should be a number"
    );
}

#[tokio::test]
async fn test_set_invite_blocklist_returns_confirmation() {
    let Some(app) = setup_test_app().await else {
        return;
    };
    let (token, _user_id, _) = register_user(&app, "charlie").await;
    let room_id = create_room(&app, &token, "Test Room").await;

    // Set invite blocklist
    let request = Request::builder()
        .method("POST")
        .uri(format!(
            "/_matrix/client/v3/rooms/{}/invite_blocklist",
            room_id
        ))
        .header("Content-Type", "application/json")
        .header("Authorization", format!("Bearer {}", token))
        .body(Body::from(
            json!({
                "user_ids": ["@blocked1:example.com", "@blocked2:example.com"]
            })
            .to_string(),
        ))
        .unwrap();

    let response = ServiceExt::<Request<Body>>::oneshot(app.clone(), request)
        .await
        .expect("Failed to set invite blocklist");

    let status = response.status();
    let body = axum::body::to_bytes(response.into_body(), 4096)
        .await
        .expect("Failed to read response body");
    if status != StatusCode::OK {
        panic!(
            "set invite blocklist expected 200, got {} body={}",
            status.as_u16(),
            String::from_utf8_lossy(&body)
        );
    }
    let body: Value = serde_json::from_slice(&body).expect("Failed to parse response");

    // Verify response contains confirmation data
    assert!(
        body.get("room_id").is_some(),
        "Response should contain room_id"
    );
    assert_eq!(body["room_id"], room_id);
    assert!(
        body.get("blocklist").is_some(),
        "Response should contain blocklist"
    );
    assert!(body["blocklist"].is_array(), "blocklist should be an array");
    assert_eq!(body["blocklist"].as_array().unwrap().len(), 2);
    assert!(
        body.get("updated_ts").is_some(),
        "Response should contain updated_ts"
    );
    assert!(
        body["updated_ts"].is_number(),
        "updated_ts should be a number"
    );
}

#[tokio::test]
async fn test_set_invite_allowlist_returns_confirmation() {
    let Some(app) = setup_test_app().await else {
        return;
    };
    let (token, _user_id, _) = register_user(&app, "dave").await;
    let room_id = create_room(&app, &token, "Test Room").await;

    // Set invite allowlist
    let request = Request::builder()
        .method("POST")
        .uri(format!(
            "/_matrix/client/v3/rooms/{}/invite_allowlist",
            room_id
        ))
        .header("Content-Type", "application/json")
        .header("Authorization", format!("Bearer {}", token))
        .body(Body::from(
            json!({
                "user_ids": ["@allowed1:example.com", "@allowed2:example.com"]
            })
            .to_string(),
        ))
        .unwrap();

    let response = ServiceExt::<Request<Body>>::oneshot(app.clone(), request)
        .await
        .expect("Failed to set invite allowlist");

    let status = response.status();
    let body = axum::body::to_bytes(response.into_body(), 4096)
        .await
        .expect("Failed to read response body");
    if status != StatusCode::OK {
        panic!(
            "set invite allowlist expected 200, got {} body={}",
            status.as_u16(),
            String::from_utf8_lossy(&body)
        );
    }
    let body: Value = serde_json::from_slice(&body).expect("Failed to parse response");

    // Verify response contains confirmation data
    assert!(
        body.get("room_id").is_some(),
        "Response should contain room_id"
    );
    assert_eq!(body["room_id"], room_id);
    assert!(
        body.get("allowlist").is_some(),
        "Response should contain allowlist"
    );
    assert!(body["allowlist"].is_array(), "allowlist should be an array");
    assert_eq!(body["allowlist"].as_array().unwrap().len(), 2);
    assert!(
        body.get("updated_ts").is_some(),
        "Response should contain updated_ts"
    );
    assert!(
        body["updated_ts"].is_number(),
        "updated_ts should be a number"
    );
}

#[tokio::test]
async fn test_send_rendezvous_message_returns_confirmation() {
    let Some(app) = setup_test_app().await else {
        return;
    };

    // Create rendezvous session
    let request = Request::builder()
        .method("POST")
        .uri("/_matrix/client/v1/rendezvous")
        .header("Content-Type", "application/json")
        .body(Body::from(
            json!({
                "intent": "login.start",
                "transport": "http.v1"
            })
            .to_string(),
        ))
        .unwrap();

    let response = ServiceExt::<Request<Body>>::oneshot(app.clone(), request)
        .await
        .expect("Failed to create rendezvous session");

    let body = axum::body::to_bytes(response.into_body(), 1024)
        .await
        .expect("Failed to read response body");
    let create_body: Value = serde_json::from_slice(&body).expect("Failed to parse response");
    let session_id = create_body["session_id"].as_str().unwrap();
    let session_key = create_body["key"].as_str().unwrap();

    // Send message
    let request = Request::builder()
        .method("POST")
        .uri(format!(
            "/_matrix/client/v1/rendezvous/{}/messages",
            session_id
        ))
        .header("X-Matrix-Rendezvous-Key", session_key)
        .header("Content-Type", "application/json")
        .body(Body::from(
            json!({
                "type": "m.login.progress",
                "content": {
                    "stage": "waiting"
                }
            })
            .to_string(),
        ))
        .unwrap();

    let response = ServiceExt::<Request<Body>>::oneshot(app.clone(), request)
        .await
        .expect("Failed to send rendezvous message");

    assert_eq!(response.status(), StatusCode::OK);

    let body = axum::body::to_bytes(response.into_body(), 1024)
        .await
        .expect("Failed to read response body");
    let body: Value = serde_json::from_slice(&body).expect("Failed to parse response");

    // Verify response contains confirmation data
    assert!(
        body.get("session_id").is_some(),
        "Response should contain session_id"
    );
    assert_eq!(body["session_id"], session_id);
    assert!(
        body.get("message_id").is_some(),
        "Response should contain message_id"
    );
    assert!(
        body["message_id"].is_string(),
        "message_id should be a string"
    );
    assert!(
        body.get("sent_ts").is_some(),
        "Response should contain sent_ts"
    );
    assert!(body["sent_ts"].is_number(), "sent_ts should be a number");
}

#[tokio::test]
async fn test_empty_blocklist_returns_confirmation() {
    let Some(app) = setup_test_app().await else {
        return;
    };
    let (token, _user_id, _) = register_user(&app, "eve").await;
    let room_id = create_room(&app, &token, "Test Room").await;

    // Set empty blocklist (clear blocklist)
    let request = Request::builder()
        .method("POST")
        .uri(format!(
            "/_matrix/client/v3/rooms/{}/invite_blocklist",
            room_id
        ))
        .header("Content-Type", "application/json")
        .header("Authorization", format!("Bearer {}", token))
        .body(Body::from(
            json!({
                "user_ids": []
            })
            .to_string(),
        ))
        .unwrap();

    let response = ServiceExt::<Request<Body>>::oneshot(app.clone(), request)
        .await
        .expect("Failed to set invite blocklist");

    assert_eq!(response.status(), StatusCode::OK);

    let body = axum::body::to_bytes(response.into_body(), 1024)
        .await
        .expect("Failed to read response body");
    let body: Value = serde_json::from_slice(&body).expect("Failed to parse response");

    // Verify response contains confirmation data even for empty list
    assert!(
        body.get("room_id").is_some(),
        "Response should contain room_id"
    );
    assert!(
        body.get("blocklist").is_some(),
        "Response should contain blocklist"
    );
    assert!(body["blocklist"].is_array(), "blocklist should be an array");
    assert_eq!(body["blocklist"].as_array().unwrap().len(), 0);
    assert!(
        body.get("updated_ts").is_some(),
        "Response should contain updated_ts"
    );
}

#[tokio::test]
async fn test_update_dm_with_content_returns_confirmation() {
    let Some(app) = setup_test_app().await else {
        return;
    };
    let (alice_token, _alice_id, _) = register_user(&app, "frank").await;
    let (_bob_token, bob_id, _) = register_user(&app, "george").await;

    // Create DM room
    let room_id = create_dm_room(&app, &alice_token, &bob_id).await;

    // Update DM room with content format
    let request = Request::builder()
        .method("PUT")
        .uri(format!("/_matrix/client/v3/direct/{}", room_id))
        .header("Content-Type", "application/json")
        .header("Authorization", format!("Bearer {}", alice_token))
        .body(Body::from(
            json!({
                "content": {
                    "user_id": bob_id
                }
            })
            .to_string(),
        ))
        .unwrap();

    let response = ServiceExt::<Request<Body>>::oneshot(app.clone(), request)
        .await
        .expect("Failed to update DM room");

    assert_eq!(response.status(), StatusCode::OK);

    let body = axum::body::to_bytes(response.into_body(), 1024)
        .await
        .expect("Failed to read response body");
    let body: Value = serde_json::from_slice(&body).expect("Failed to parse response");

    // Verify response contains confirmation data
    assert!(
        body.get("room_id").is_some(),
        "Response should contain room_id"
    );
    assert!(body.get("users").is_some(), "Response should contain users");
    assert!(
        body.get("updated_ts").is_some(),
        "Response should contain updated_ts"
    );
}
