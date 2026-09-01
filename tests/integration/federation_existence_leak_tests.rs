//! Federation Existence Leak Prevention Tests (Audit 06 §10, OPT-017)
//!
//! Tests that federation endpoints do not leak room/user existence via
//! distinguishable HTTP status codes (404 vs 403). A private room that
//! exists but denies access must return the same status as a non-existent
//! room, so an attacker cannot determine whether a room exists.

use axum::{
    body::Body,
    http::{Request, StatusCode},
};
use base64::engine::general_purpose::STANDARD_NO_PAD;
use base64::Engine as _;
use ed25519_dalek::Signer;
use serde_json::{json, Value};
use std::sync::Arc;
use synapse_rust::federation::signing::canonical_federation_request_bytes;
use tower::ServiceExt;

// ---------------------------------------------------------------------------
// Test helpers
// ---------------------------------------------------------------------------

async fn setup_federation_app() -> Option<(
    axum::Router,
    Arc<sqlx::PgPool>,
    String,
    String,
    ed25519_dalek::SigningKey,
    Arc<synapse_rust::cache::CacheManager>,
)> {
    let key_id = "ed25519:test";
    let signing_key_seed = [17u8; 32];
    let signing_key_b64 = STANDARD_NO_PAD.encode(signing_key_seed);
    let signing_key = ed25519_dalek::SigningKey::from_bytes(&signing_key_seed);

    let pool = super::require_test_pool().await;
    let mut container = synapse_services::ServiceContainer::new_test_with_pool(pool.clone()).await;
    super::config_mut(&mut container).server.name = "localhost".to_string();
    container.core.server_name = "localhost".to_string();
    super::config_mut(&mut container).federation.enabled = true;
    super::config_mut(&mut container).federation.allow_ingress = true;
    super::config_mut(&mut container).federation.server_name = "localhost".to_string();
    super::config_mut(&mut container).federation.key_id = Some(key_id.to_string());
    super::config_mut(&mut container).federation.signing_key = Some(signing_key_b64.clone());
    let cache = Arc::new(synapse_rust::cache::CacheManager::new(&synapse_rust::cache::CacheConfig::default()));
    let state = synapse_rust::web::routes::state::AppState::new(container, cache.clone());
    let app = synapse_rust::web::create_router(state);
    Some((app, pool, key_id.to_string(), signing_key_b64, signing_key, cache))
}

/// Register a remote server's verify key in the federation auth cache so that
/// signed requests from that origin pass authentication without network calls.
async fn register_remote_verify_key(
    cache: &synapse_rust::cache::CacheManager,
    origin: &str,
    key_id: &str,
    signing_key: &ed25519_dalek::SigningKey,
) {
    let public_key_b64 = STANDARD_NO_PAD.encode(signing_key.verifying_key().to_bytes());
    let cache_key = format!("federation:verify_key:{origin}:{key_id}");
    let _ = cache.set::<String>(&cache_key, public_key_b64, 3600).await;
}

/// Build a signed federation request where origin != destination (remote server
/// sending to the local server).
fn signed_fed_request_as(
    method: &str,
    uri: &str,
    origin: &str,
    destination: &str,
    key_id: &str,
    signing_key: &ed25519_dalek::SigningKey,
    content: Option<&Value>,
) -> Request<Body> {
    let signed_bytes = canonical_federation_request_bytes(method, uri, origin, destination, content).unwrap();
    let sig = signing_key.sign(&signed_bytes);
    let sig_b64 = STANDARD_NO_PAD.encode(sig.to_bytes());

    let mut builder = Request::builder().method(method).uri(uri).header(
        "Authorization",
        format!(
            "X-Matrix origin=\"{}\",destination=\"{}\",key=\"{}\",sig=\"{}\"",
            origin, destination, key_id, sig_b64
        ),
    );

    if content.is_some() {
        builder = builder.header("Content-Type", "application/json");
    }

    builder.body(Body::from(content.map(Value::to_string).unwrap_or_default())).unwrap()
}

fn signed_fed_request(
    method: &str,
    uri: &str,
    origin: &str,
    key_id: &str,
    signing_key: &ed25519_dalek::SigningKey,
    content: Option<&Value>,
) -> Request<Body> {
    let signed_bytes = canonical_federation_request_bytes(method, uri, origin, origin, content).unwrap();
    let sig = signing_key.sign(&signed_bytes);
    let sig_b64 = STANDARD_NO_PAD.encode(sig.to_bytes());

    let mut builder = Request::builder()
        .method(method)
        .uri(uri)
        .header("Authorization", format!("X-Matrix origin=\"{}\",key=\"{}\",sig=\"{}\"", origin, key_id, sig_b64));

    if content.is_some() {
        builder = builder.header("Content-Type", "application/json");
    }

    builder.body(Body::from(content.map(Value::to_string).unwrap_or_default())).unwrap()
}

async fn register_user(app: &axum::Router, username: &str) -> (String, String) {
    let request = Request::builder()
        .method("POST")
        .uri("/_matrix/client/v3/register")
        .header("Content-Type", "application/json")
        .body(Body::from(
            serde_json::json!({
                "username": format!("{}_{}", username, rand::random::<u32>()),
                "password": "Password123!",
                "auth": { "type": "m.login.dummy" }
            })
            .to_string(),
        ))
        .unwrap();

    let response = ServiceExt::<Request<Body>>::oneshot(app.clone(), request).await.unwrap();
    assert_eq!(response.status(), StatusCode::OK);

    let body = axum::body::to_bytes(response.into_body(), 2048).await.unwrap();
    let json: Value = serde_json::from_slice(&body).unwrap();
    (json["access_token"].as_str().unwrap().to_string(), json["user_id"].as_str().unwrap().to_string())
}

async fn create_private_room(app: &axum::Router, token: &str) -> String {
    // Create a room with invite join_rule (private) — the default.
    let request = Request::builder()
        .method("POST")
        .uri("/_matrix/client/v3/createRoom")
        .header("Authorization", format!("Bearer {}", token))
        .header("Content-Type", "application/json")
        .body(Body::from(
            json!({
                "name": "Private Test Room",
                "preset": "private_chat"
            })
            .to_string(),
        ))
        .unwrap();

    let response = ServiceExt::<Request<Body>>::oneshot(app.clone(), request).await.unwrap();
    assert_eq!(response.status(), StatusCode::OK);

    let body = axum::body::to_bytes(response.into_body(), 2048).await.unwrap();
    let json: Value = serde_json::from_slice(&body).unwrap();
    json["room_id"].as_str().unwrap().to_string()
}

/// Build a valid m.room.member join event body for send_join_v2.
fn build_join_event_body(room_id: &str, event_id: &str, sender: &str) -> Value {
    json!({
        "type": "m.room.member",
        "content": { "membership": "join" },
        "sender": sender,
        "state_key": sender,
        "room_id": room_id,
        "event_id": event_id,
        "origin": "localhost",
        "origin_server_ts": chrono::Utc::now().timestamp_millis()
    })
}

// ---------------------------------------------------------------------------
// Tests: send_join_v2 existence leak
// ---------------------------------------------------------------------------

#[tokio::test]
async fn send_join_v2_no_existence_leak_private_room_vs_nonexistent() {
    let Some((app, _pool, key_id, _key_b64, signing_key, _cache)) = setup_federation_app().await else {
        return;
    };

    // 1. Create a private room via client API.
    let (token, _creator_id) = register_user(&app, "creator").await;
    let private_room_id = create_private_room(&app, &token).await;

    // 2. Attempt send_join_v2 to the private room as a non-member.
    //    Currently: federatable_room_version passes (room exists) →
    //    validate_federation_join_access returns 403 → response is 403 (LEAK).
    //    After fix: access check first, forbidden→404 → response is 404.
    let joiner = "@joiner:localhost";
    let event_id = "$join_evt_001:localhost";
    let body = build_join_event_body(&private_room_id, event_id, joiner);
    let uri = format!("/_matrix/federation/v2/send_join/{}/{}", private_room_id, event_id);
    let request = signed_fed_request("PUT", &uri, "localhost", &key_id, &signing_key, Some(&body));

    let response = ServiceExt::<Request<Body>>::oneshot(app.clone(), request).await.unwrap();
    let private_room_status = response.status();

    // 3. Attempt send_join_v2 to a room that does not exist.
    let nonexistent_room = "!nonexistent_room:localhost";
    let event_id_2 = "$join_evt_002:localhost";
    let body_2 = build_join_event_body(nonexistent_room, event_id_2, joiner);
    let uri_2 = format!("/_matrix/federation/v2/send_join/{}/{}", nonexistent_room, event_id_2);
    let request_2 = signed_fed_request("PUT", &uri_2, "localhost", &key_id, &signing_key, Some(&body_2));

    let response_2 = ServiceExt::<Request<Body>>::oneshot(app, request_2).await.unwrap();
    let nonexistent_status = response_2.status();

    // 4. Both must return the same status code — no existence leak.
    assert_eq!(
        private_room_status, nonexistent_status,
        "send_join_v2 leaks room existence: private room returned {}, non-existent room returned {}. \
         Both must return the same status to prevent existence enumeration.",
        private_room_status, nonexistent_status
    );

    // 5. Both must be 404 (not 403) after the fix.
    assert_eq!(
        private_room_status,
        StatusCode::NOT_FOUND,
        "send_join_v2 for a private room without access must return 404, not {}",
        private_room_status
    );
}

// ---------------------------------------------------------------------------
// Tests: send_leave_v2 existence leak
// ---------------------------------------------------------------------------

/// Build a valid m.room.member leave event body for send_leave_v2.
fn build_leave_event_body(room_id: &str, event_id: &str, sender: &str, origin: &str) -> Value {
    json!({
        "type": "m.room.member",
        "content": { "membership": "leave" },
        "sender": sender,
        "state_key": sender,
        "room_id": room_id,
        "event_id": event_id,
        "origin": origin,
        "origin_server_ts": chrono::Utc::now().timestamp_millis()
    })
}

#[tokio::test]
async fn send_leave_v2_no_existence_leak_remote_server() {
    let Some((app, _pool, _local_key_id, _local_key_b64, _local_signing_key, cache)) = setup_federation_app().await
    else {
        return;
    };

    // 0. Register a remote server signing key in the cache so federation auth passes.
    let remote_origin = "remote.example";
    let remote_key_id = "ed25519:remote_test";
    let remote_signing_key_seed = [99u8; 32];
    let remote_signing_key = ed25519_dalek::SigningKey::from_bytes(&remote_signing_key_seed);
    register_remote_verify_key(&cache, remote_origin, remote_key_id, &remote_signing_key).await;

    // 1. Create a private room via client API (by a localhost user).
    let (token, _creator_id) = register_user(&app, "creator").await;
    let private_room_id = create_private_room(&app, &token).await;

    // 2. Remote server attempts send_leave_v2 on the private room.
    //    The remote server has NO members in this room.
    //    Currently: federatable_room_version passes (room exists) → code
    //    proceeds to create event → returns 200 (LEAK).
    //    After fix: validate_federation_origin_can_observe_room returns 404
    //    (no members from remote.example) → response is 404.
    let leaver = "@leaver:remote.example";
    let event_id = "$leave_evt_001:remote.example";
    let body = build_leave_event_body(&private_room_id, event_id, leaver, remote_origin);
    let uri = format!("/_matrix/federation/v2/send_leave/{}/{}", private_room_id, event_id);
    let request =
        signed_fed_request_as("PUT", &uri, remote_origin, "localhost", remote_key_id, &remote_signing_key, Some(&body));

    let response = ServiceExt::<Request<Body>>::oneshot(app.clone(), request).await.unwrap();
    let private_room_status = response.status();

    // 3. Remote server attempts send_leave_v2 on a non-existent room.
    let nonexistent_room = "!nonexistent_room:localhost";
    let event_id_2 = "$leave_evt_002:remote.example";
    let body_2 = build_leave_event_body(nonexistent_room, event_id_2, leaver, remote_origin);
    let uri_2 = format!("/_matrix/federation/v2/send_leave/{}/{}", nonexistent_room, event_id_2);
    let request_2 = signed_fed_request_as(
        "PUT",
        &uri_2,
        remote_origin,
        "localhost",
        remote_key_id,
        &remote_signing_key,
        Some(&body_2),
    );

    let response_2 = ServiceExt::<Request<Body>>::oneshot(app, request_2).await.unwrap();
    let nonexistent_status = response_2.status();

    // 4. Both must return the same status code — no existence leak.
    assert_eq!(
        private_room_status, nonexistent_status,
        "send_leave_v2 leaks room existence: private room returned {}, non-existent room returned {}. \
         Both must return the same status to prevent existence enumeration.",
        private_room_status, nonexistent_status
    );

    // 5. Both must be 404 after the fix.
    assert_eq!(
        private_room_status,
        StatusCode::NOT_FOUND,
        "send_leave_v2 for a private room without access must return 404, not {}",
        private_room_status
    );
}

// ---------------------------------------------------------------------------
// Tests: invite_v2 existence leak
// ---------------------------------------------------------------------------

/// Build a valid m.room.member invite event body for invite_v2.
fn build_invite_event_body(room_id: &str, event_id: &str, sender: &str, invitee: &str, origin: &str) -> Value {
    json!({
        "type": "m.room.member",
        "content": { "membership": "invite" },
        "sender": sender,
        "state_key": invitee,
        "room_id": room_id,
        "event_id": event_id,
        "origin": origin,
        "origin_server_ts": chrono::Utc::now().timestamp_millis()
    })
}

#[tokio::test]
async fn invite_v2_no_existence_leak_remote_server() {
    let Some((app, _pool, _local_key_id, _local_key_b64, _local_signing_key, cache)) = setup_federation_app().await
    else {
        return;
    };

    // 0. Register a remote server signing key in the cache.
    let remote_origin = "remote.example";
    let remote_key_id = "ed25519:remote_test";
    let remote_signing_key_seed = [99u8; 32];
    let remote_signing_key = ed25519_dalek::SigningKey::from_bytes(&remote_signing_key_seed);
    register_remote_verify_key(&cache, remote_origin, remote_key_id, &remote_signing_key).await;

    // 1. Create a private room via client API (by a localhost user).
    let (token, _creator_id) = register_user(&app, "creator").await;
    let private_room_id = create_private_room(&app, &token).await;

    // 2. Remote server attempts invite_v2 on the private room.
    //    The remote server has NO members in this room.
    //    Currently: federatable_room_version passes (room exists) → code
    //    proceeds to create event → returns 200 (LEAK).
    //    After fix: validate_federation_origin_can_observe_room returns 404
    //    (no members from remote.example) → response is 404.
    let inviter = "@inviter:remote.example";
    let invitee = "@invitee:localhost";
    let event_id = "$invite_evt_001:remote.example";
    let body = build_invite_event_body(&private_room_id, event_id, inviter, invitee, remote_origin);
    let uri = format!("/_matrix/federation/v2/invite/{}/{}", private_room_id, event_id);
    let request =
        signed_fed_request_as("PUT", &uri, remote_origin, "localhost", remote_key_id, &remote_signing_key, Some(&body));

    let response = ServiceExt::<Request<Body>>::oneshot(app.clone(), request).await.unwrap();
    let private_room_status = response.status();

    // 3. Remote server attempts invite_v2 on a non-existent room.
    let nonexistent_room = "!nonexistent_room:localhost";
    let event_id_2 = "$invite_evt_002:remote.example";
    let body_2 = build_invite_event_body(nonexistent_room, event_id_2, inviter, invitee, remote_origin);
    let uri_2 = format!("/_matrix/federation/v2/invite/{}/{}", nonexistent_room, event_id_2);
    let request_2 = signed_fed_request_as(
        "PUT",
        &uri_2,
        remote_origin,
        "localhost",
        remote_key_id,
        &remote_signing_key,
        Some(&body_2),
    );

    let response_2 = ServiceExt::<Request<Body>>::oneshot(app, request_2).await.unwrap();
    let nonexistent_status = response_2.status();

    // 4. Both must return the same status code — no existence leak.
    assert_eq!(
        private_room_status, nonexistent_status,
        "invite_v2 leaks room existence: private room returned {}, non-existent room returned {}. \
         Both must return the same status to prevent existence enumeration.",
        private_room_status, nonexistent_status
    );

    // 5. Both must be 404 after the fix.
    assert_eq!(
        private_room_status,
        StatusCode::NOT_FOUND,
        "invite_v2 for a private room without access must return 404, not {}",
        private_room_status
    );
}

// ---------------------------------------------------------------------------
// Tests: knock_room existence leak
// ---------------------------------------------------------------------------

/// Build a valid knock event body for the federation knock endpoint.
fn build_knock_event_body(room_id: &str, sender: &str, origin: &str) -> Value {
    json!({
        "type": "m.room.member",
        "content": { "membership": "knock" },
        "sender": sender,
        "state_key": sender,
        "room_id": room_id,
        "origin": origin,
        "origin_server_ts": chrono::Utc::now().timestamp_millis()
    })
}

#[tokio::test]
async fn knock_room_no_existence_leak_remote_server() {
    let Some((app, _pool, _local_key_id, _local_key_b64, _local_signing_key, cache)) = setup_federation_app().await
    else {
        return;
    };

    // 0. Register a remote server signing key in the cache.
    let remote_origin = "remote.example";
    let remote_key_id = "ed25519:remote_test";
    let remote_signing_key_seed = [99u8; 32];
    let remote_signing_key = ed25519_dalek::SigningKey::from_bytes(&remote_signing_key_seed);
    register_remote_verify_key(&cache, remote_origin, remote_key_id, &remote_signing_key).await;

    // 1. Create a private room via client API (by a localhost user).
    let (token, _creator_id) = register_user(&app, "creator").await;
    let private_room_id = create_private_room(&app, &token).await;

    // 2. Remote server attempts knock on the private room.
    //    The remote server has NO members in this room.
    //    Currently: federatable_room_version passes (room exists) → code
    //    proceeds to create event → returns 200 (LEAK).
    //    After fix: validate_federation_origin_can_observe_room returns 404
    //    (no members from remote.example) → response is 404.
    let knocker = "@knocker:remote.example";
    let body = build_knock_event_body(&private_room_id, knocker, remote_origin);
    let uri = format!("/_matrix/federation/v1/knock/{}/{}", private_room_id, knocker);
    let request = signed_fed_request_as(
        "POST",
        &uri,
        remote_origin,
        "localhost",
        remote_key_id,
        &remote_signing_key,
        Some(&body),
    );

    let response = ServiceExt::<Request<Body>>::oneshot(app.clone(), request).await.unwrap();
    let private_room_status = response.status();

    // 3. Remote server attempts knock on a non-existent room.
    let nonexistent_room = "!nonexistent_room:localhost";
    let body_2 = build_knock_event_body(nonexistent_room, knocker, remote_origin);
    let uri_2 = format!("/_matrix/federation/v1/knock/{}/{}", nonexistent_room, knocker);
    let request_2 = signed_fed_request_as(
        "POST",
        &uri_2,
        remote_origin,
        "localhost",
        remote_key_id,
        &remote_signing_key,
        Some(&body_2),
    );

    let response_2 = ServiceExt::<Request<Body>>::oneshot(app, request_2).await.unwrap();
    let nonexistent_status = response_2.status();

    // 4. Both must return the same status code — no existence leak.
    assert_eq!(
        private_room_status, nonexistent_status,
        "knock_room leaks room existence: private room returned {}, non-existent room returned {}. \
         Both must return the same status to prevent existence enumeration.",
        private_room_status, nonexistent_status
    );

    // 5. Both must be 404 after the fix.
    assert_eq!(
        private_room_status,
        StatusCode::NOT_FOUND,
        "knock_room for a private room without access must return 404, not {}",
        private_room_status
    );
}
