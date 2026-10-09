use super::setup_fresh_test_app;
use axum::{
    body::Body,
    http::{Request, StatusCode},
};
use serde_json::{json, Value};
use tower::ServiceExt;

async fn register_user(app: &axum::Router, username: &str) -> String {
    let request = Request::builder()
        .method("POST")
        .uri("/_matrix/client/v3/register")
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

    let response = ServiceExt::<Request<Body>>::oneshot(app.clone(), request).await.unwrap();
    assert_eq!(response.status(), StatusCode::OK);

    let body = axum::body::to_bytes(response.into_body(), 1024).await.unwrap();
    let json: Value = serde_json::from_slice(&body).unwrap();
    json["access_token"].as_str().unwrap().to_string()
}

async fn create_room(app: &axum::Router, token: &str, name: &str) -> String {
    let request = Request::builder()
        .method("POST")
        .uri("/_matrix/client/v3/createRoom")
        .header("Authorization", format!("Bearer {}", token))
        .header("Content-Type", "application/json")
        .body(Body::from(json!({ "name": name }).to_string()))
        .unwrap();

    let response = ServiceExt::<Request<Body>>::oneshot(app.clone(), request).await.unwrap();
    assert_eq!(response.status(), StatusCode::OK);

    let body = axum::body::to_bytes(response.into_body(), 1024).await.unwrap();
    let json: Value = serde_json::from_slice(&body).unwrap();
    json["room_id"].as_str().unwrap().to_string()
}

fn encode_room_id(room_id: &str) -> String {
    room_id.replace('!', "%21").replace(':', "%3A")
}

#[tokio::test]
async fn test_room_sync_requires_membership() {
    let Some(app) = setup_fresh_test_app().await else {
        return;
    };

    let alice = format!("alice_{}", rand::random::<u32>());
    let bob = format!("bob_{}", rand::random::<u32>());
    let alice_token = register_user(&app, &alice).await;
    let bob_token = register_user(&app, &bob).await;

    let room_id = create_room(&app, &alice_token, "room_sync_test").await;
    let encoded_room_id = encode_room_id(&room_id);

    let request = Request::builder()
        .method("GET")
        .uri(format!("/_matrix/vendor/v1/rooms/{}/sync", encoded_room_id))
        .header("Authorization", format!("Bearer {}", bob_token))
        .body(Body::empty())
        .unwrap();

    let response = ServiceExt::<Request<Body>>::oneshot(app.clone(), request).await.unwrap();
    assert_eq!(response.status(), StatusCode::FORBIDDEN);
}

#[tokio::test]
async fn test_room_sync_returns_minimal_shape() {
    let Some(app) = setup_fresh_test_app().await else {
        return;
    };

    let alice = format!("alice_{}", rand::random::<u32>());
    let alice_token = register_user(&app, &alice).await;

    let room_id = create_room(&app, &alice_token, "room_sync_shape").await;
    let encoded_room_id = encode_room_id(&room_id);

    let request = Request::builder()
        .method("GET")
        .uri(format!("/_matrix/vendor/v1/rooms/{}/sync", encoded_room_id))
        .header("Authorization", format!("Bearer {}", alice_token))
        .body(Body::empty())
        .unwrap();

    let response = ServiceExt::<Request<Body>>::oneshot(app.clone(), request).await.unwrap();
    assert_eq!(response.status(), StatusCode::OK);

    let body = axum::body::to_bytes(response.into_body(), 64 * 1024).await.unwrap();
    let json: Value = serde_json::from_slice(&body).unwrap();

    assert!(json.get("next_batch").and_then(|v| v.as_str()).is_some());
    assert!(json.get("timeline").and_then(|v| v.as_object()).is_some());
    assert!(json.get("timeline").and_then(|v| v.get("events")).and_then(|v| v.as_array()).is_some());
    assert!(json.get("state").and_then(|v| v.get("events")).and_then(|v| v.as_array()).is_some());
    assert!(json
        .get("unread_notifications")
        .and_then(|v| v.get("notification_count"))
        .and_then(|v| v.as_i64())
        .is_some());
}

#[tokio::test]
async fn test_room_sync_incremental_omits_state() {
    let Some(app) = setup_fresh_test_app().await else {
        return;
    };

    let alice = format!("alice_{}", rand::random::<u32>());
    let alice_token = register_user(&app, &alice).await;

    let room_id = create_room(&app, &alice_token, "room_sync_incremental").await;
    let encoded_room_id = encode_room_id(&room_id);

    let first_request = Request::builder()
        .method("GET")
        .uri(format!("/_matrix/vendor/v1/rooms/{}/sync", encoded_room_id))
        .header("Authorization", format!("Bearer {}", alice_token))
        .body(Body::empty())
        .unwrap();

    let first_response = ServiceExt::<Request<Body>>::oneshot(app.clone(), first_request).await.unwrap();
    assert_eq!(first_response.status(), StatusCode::OK);

    let first_body = axum::body::to_bytes(first_response.into_body(), 64 * 1024).await.unwrap();
    let first_json: Value = serde_json::from_slice(&first_body).unwrap();
    let since = first_json["next_batch"].as_str().unwrap().to_string();

    let second_request = Request::builder()
        .method("GET")
        .uri(format!("/_matrix/vendor/v1/rooms/{}/sync?since={}", encoded_room_id, since))
        .header("Authorization", format!("Bearer {}", alice_token))
        .body(Body::empty())
        .unwrap();

    let second_response = ServiceExt::<Request<Body>>::oneshot(app.clone(), second_request).await.unwrap();
    assert_eq!(second_response.status(), StatusCode::OK);

    let second_body = axum::body::to_bytes(second_response.into_body(), 64 * 1024).await.unwrap();
    let second_json: Value = serde_json::from_slice(&second_body).unwrap();
    let state_events = second_json["state"]["events"].as_array().unwrap();
    assert!(state_events.is_empty());
}

/// Like [`register_user`], but also hands back the generated user ID (the invite
/// target).
async fn register_user_with_id(app: &axum::Router, username: &str) -> (String, String) {
    let request = Request::builder()
        .method("POST")
        .uri("/_matrix/client/v3/register")
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

    let response = ServiceExt::<Request<Body>>::oneshot(app.clone(), request).await.unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    let body = axum::body::to_bytes(response.into_body(), 1024).await.unwrap();
    let json: Value = serde_json::from_slice(&body).unwrap();
    (json["access_token"].as_str().unwrap().to_string(), json["user_id"].as_str().unwrap().to_string())
}

/// An invited user must see the room under `rooms.invite`.
///
/// `get_sync_rooms` is the **only** producer of the membership list `/sync`
/// buckets into sections, so filtering `invite` out (`membership IN
/// ('join','leave')`) made `/sync` blind to every invite: the whole
/// `invite_state` rendering pipeline existed but was unreachable, and an invited
/// user simply saw nothing. Found by the cross-implementation V-2b run
/// (2026-10-03) after the invite itself started being accepted.
#[tokio::test]
async fn test_global_sync_shows_an_invited_room() {
    let Some(app) = setup_fresh_test_app().await else {
        return;
    };

    let alice_token = register_user(&app, &format!("sync_inv_alice_{}", rand::random::<u32>())).await;
    let (bob_token, bob_id) = register_user_with_id(&app, &format!("sync_inv_bob_{}", rand::random::<u32>())).await;

    let room_id = create_room(&app, &alice_token, "sync_invite").await;

    let invite_request = Request::builder()
        .method("POST")
        .uri(format!("/_matrix/client/v3/rooms/{}/invite", encode_room_id(&room_id)))
        .header("Authorization", format!("Bearer {alice_token}"))
        .header("Content-Type", "application/json")
        .body(Body::from(json!({ "user_id": bob_id }).to_string()))
        .unwrap();
    let invite_response = ServiceExt::<Request<Body>>::oneshot(app.clone(), invite_request).await.unwrap();
    assert_eq!(invite_response.status(), StatusCode::OK, "the local invite must be accepted");

    let sync_request = Request::builder()
        .method("GET")
        .uri("/_matrix/client/v3/sync?timeout=0")
        .header("Authorization", format!("Bearer {bob_token}"))
        .body(Body::empty())
        .unwrap();
    let sync_response = ServiceExt::<Request<Body>>::oneshot(app.clone(), sync_request).await.unwrap();
    assert_eq!(sync_response.status(), StatusCode::OK);
    let sync_body = axum::body::to_bytes(sync_response.into_body(), 256 * 1024).await.unwrap();
    let sync_json: Value = serde_json::from_slice(&sync_body).unwrap();

    let invite = &sync_json["rooms"]["invite"][&room_id];
    assert!(!invite.is_null(), "the invited user must see `rooms.invite[{room_id}]`: {sync_json}");
    // The invitee renders the invite from stripped state, which must therefore
    // carry the room's `m.room.create` (MSC4311: no create ⇒ no room version).
    let stripped_types: Vec<&str> = invite["invite_state"]["events"]
        .as_array()
        .expect("invite_state.events must be an array")
        .iter()
        .filter_map(|event| event["type"].as_str())
        .collect();
    assert!(stripped_types.contains(&"m.room.create"), "invite_state must carry m.room.create, got {stripped_types:?}");
}
