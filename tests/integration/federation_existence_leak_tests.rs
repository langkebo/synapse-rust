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
use synapse_federation::signing::{compute_event_content_hash, signature_material_bytes};
use synapse_storage::event::EventStorage;
use synapse_web::federation::signing::canonical_federation_request_bytes;
use synapse_web::routes::federation::pdu::{apply_stored_signature_material, state_pdu, PduCompleteness};
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
    // The key rotation manager is built *inside* `ServiceContainer::new`, so the
    // config overrides above never reach it: seed it directly, otherwise every
    // PDU re-signing path silently no-ops for want of a current key.
    //
    // `initialize` installs the in-memory key *before* its at-rest persistence
    // policy check, which refuses plaintext when no master key is configured —
    // so the key is usable even though that check may fail. Assert on the
    // installed key rather than on the (deliberately unpersisted) result.
    let init_result = container.federation.key_rotation_manager.initialize(&signing_key_b64, key_id).await;
    assert!(
        container.federation.key_rotation_manager.get_current_key().await.expect("key read").is_some(),
        "the deterministic test signing key must be installed for PDU signing (initialize returned {init_result:?})"
    );
    let cache = Arc::new(synapse_rust::cache::CacheManager::new(&synapse_rust::cache::CacheConfig::default()));
    let state = synapse_web::routes::state::AppState::new(container, cache.clone());
    let app = synapse_web::create_router(state);
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

/// Build a v2 invite request body: `{"event": …, "room_version": …}`.
///
/// The v2 body is **not** the bare event — upstream `FederationV2InviteServlet`
/// reads `content["event"]` and `content["room_version"]`, and a v3+ PDU carries
/// neither a version nor an `event_id` of its own (the receiver derives the ID).
///
/// The event deliberately carries no `depth`/`prev_events`/`auth_events`: it is
/// accepted far enough to prove the request cleared the existence gate, then
/// rejected by PDU-integrity validation.
fn build_invite_event_body(room_id: &str, sender: &str, invitee: &str, origin: &str, room_version: &str) -> Value {
    json!({
        "event": {
            "type": "m.room.member",
            "content": { "membership": "invite" },
            "sender": sender,
            "state_key": invitee,
            "room_id": room_id,
            "origin": origin,
            "origin_server_ts": chrono::Utc::now().timestamp_millis()
        },
        "room_version": room_version
    })
}

/// OPT-017's observability rule applies only to rooms we host.
///
/// A federated invite is how the invitee's server *first learns* a room exists,
/// so it holds no membership rows to check. Applying the rule unconditionally
/// would reject the very first cross-server invite to every room with
/// `M_NOT_FOUND`, making federated invites unusable. A room we do host still
/// refuses an origin with no non-banned member.
///
/// The consequence is that the two cases no longer share a status code: a hosted
/// private room returns 404, while an unknown room clears the gate and is then
/// rejected by PDU-integrity validation — 400 `M_BAD_JSON`, because the body
/// above carries no DAG fields. That 400, not a 404, is the evidence the
/// existence gate was passed.
#[tokio::test]
async fn invite_v2_observability_applies_only_to_hosted_rooms() {
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

    // 2. Remote server attempts invite_v2 on the private room it has no member
    //    in. We host it, so `validate_federation_origin_can_observe_room`
    //    refuses with 404 instead of leaking the room's contents.
    let inviter = "@inviter:remote.example";
    let invitee = "@invitee:localhost";
    let event_id = "$invite_evt_001:remote.example";
    let body = build_invite_event_body(&private_room_id, inviter, invitee, remote_origin, "12");
    let uri = format!("/_matrix/federation/v2/invite/{}/{}", private_room_id, event_id);
    let request =
        signed_fed_request_as("PUT", &uri, remote_origin, "localhost", remote_key_id, &remote_signing_key, Some(&body));

    let response = ServiceExt::<Request<Body>>::oneshot(app.clone(), request).await.unwrap();
    let private_room_status = response.status();

    // 3. Remote server attempts invite_v2 on a room we have never seen. This is
    //    the normal first-contact case for federated invites, so it must clear
    //    the existence gate.
    let nonexistent_room = "!nonexistent_room:localhost";
    let event_id_2 = "$invite_evt_002:remote.example";
    let body_2 = build_invite_event_body(nonexistent_room, inviter, invitee, remote_origin, "12");
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

    // 4. A room we host must still refuse an origin with no member.
    assert_eq!(
        private_room_status,
        StatusCode::NOT_FOUND,
        "invite_v2 for a hosted room without a member from the origin must return 404, not {}",
        private_room_status
    );

    // 5. An unknown room must clear the existence gate. The body intentionally
    //    lacks DAG fields, so the next check rejects it with 400 — never 404.
    assert_eq!(
        nonexistent_status,
        StatusCode::BAD_REQUEST,
        "invite_v2 for an unknown room must clear the existence gate and be rejected by \
         PDU validation with 400, not blocked as if enumerating (got {})",
        nonexistent_status
    );
}

/// A federated invite is how the invitee's server first learns the room exists,
/// so the invite's `prev_events` name events it has never seen. Persisting the
/// PDU through the normal graph path would try to write `event_edges` rows
/// pointing at those foreign parents and fail the foreign key onto `events`;
/// the invite must be stored as an outlier (graph columns kept, no edges) and
/// the invitee's membership must be recorded — otherwise `/sync` and
/// `/rooms/{roomId}/state` never show the invite.
#[tokio::test]
async fn invite_v2_persists_unknown_room_invite_as_outlier_and_records_membership() {
    let Some((app, pool, _local_key_id, _local_key_b64, _local_signing_key, cache)) = setup_federation_app().await
    else {
        return;
    };

    // 0. Register the remote server's signing key so federation auth passes.
    let remote_origin = "remote.example";
    let remote_key_id = "ed25519:remote_test";
    let remote_signing_key = ed25519_dalek::SigningKey::from_bytes(&[99u8; 32]);
    register_remote_verify_key(&cache, remote_origin, remote_key_id, &remote_signing_key).await;

    // 1. The invitee is a user we host — that is why the invite is addressed to us.
    let (_invitee_token, invitee_id) = register_user(&app, "invitee").await;
    let inviter = "@inviter:remote.example";

    // 2. The room is unknown to us, and the invite's parents live on the
    //    inviting server. Room version "1" is used so the PDU carries its own
    //    `event_id` (v3+ derive it, which would need the reference hash here).
    let room_id = format!("!outlier_invite_room_{}:localhost", rand::random::<u32>());
    let event_id = "$outlier_invite_evt_001:remote.example";
    let body = json!({
        "event": {
            "type": "m.room.member",
            "content": { "membership": "invite" },
            "sender": inviter,
            "state_key": invitee_id,
            "room_id": room_id,
            "event_id": event_id,
            "origin": remote_origin,
            "origin_server_ts": chrono::Utc::now().timestamp_millis(),
            "depth": 7,
            "prev_events": ["$parent_we_never_saw:remote.example"],
            "auth_events": ["$create_we_never_saw:remote.example"]
        },
        "room_version": "1"
    });
    let uri = format!("/_matrix/federation/v2/invite/{}/{}", room_id, event_id);
    let request =
        signed_fed_request_as("PUT", &uri, remote_origin, "localhost", remote_key_id, &remote_signing_key, Some(&body));

    let response = ServiceExt::<Request<Body>>::oneshot(app, request).await.unwrap();
    let status = response.status();
    let response_body = axum::body::to_bytes(response.into_body(), 8192).await.unwrap();
    let response_json: Value = serde_json::from_slice(&response_body).unwrap();

    assert_eq!(
        status,
        StatusCode::OK,
        "an invite for an unknown room whose parents we do not hold must be accepted as an outlier, got {status}: {response_json}"
    );

    // 3. The spec answer is `{"event": <signed PDU>}` — the inviting server
    //    verifies the event it just handed us, so the key is mandatory.
    assert!(
        response_json.get("event").is_some_and(Value::is_object),
        "invite_v2 must answer with {{\"event\": <PDU>}}, got {response_json}"
    );

    // 4. The invitee's membership is recorded: nothing else writes it, because
    //    the persisted event is the remote PDU and the graph path is bypassed.
    let membership: String =
        sqlx::query_scalar("SELECT membership FROM room_memberships WHERE room_id = $1 AND user_id = $2")
            .bind(&room_id)
            .bind(&invitee_id)
            .fetch_one(pool.as_ref())
            .await
            .expect("the invitee's room_memberships row must exist after an inbound federated invite");
    assert_eq!(membership, "invite", "the recorded membership must be `invite`");

    // 5. `sender` keeps the remote inviter, so clients can report who invited.
    let recorded_sender: String =
        sqlx::query_scalar("SELECT sender FROM room_memberships WHERE room_id = $1 AND user_id = $2")
            .bind(&room_id)
            .bind(&invitee_id)
            .fetch_one(pool.as_ref())
            .await
            .expect("the recorded membership must carry the inviter as sender");
    assert_eq!(recorded_sender, inviter);

    // 6. The event itself is persisted with the sender's ID and graph position.
    let stored_event_id: String = sqlx::query_scalar("SELECT event_id FROM events WHERE event_id = $1")
        .bind(event_id)
        .fetch_one(pool.as_ref())
        .await
        .expect("the invite PDU must be persisted");
    assert_eq!(stored_event_id, event_id);

    // 7. Outlier shape: no `event_edges` rows may point at the foreign parents.
    let edge_count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM event_edges WHERE event_id = $1")
        .bind(event_id)
        .fetch_one(pool.as_ref())
        .await
        .unwrap();
    assert_eq!(edge_count, 0, "an outlier must not write event_edges rows: the parents are not events we hold");
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

// ---------------------------------------------------------------------------
// Tests: F-03 / U-13-R7 — the persisted signature must cover the PDU a peer
// actually receives (the projected row), not a hand-assembled partial dict
// ---------------------------------------------------------------------------

/// Create a room with an explicit room version via the client API.
async fn create_room_with_version(app: &axum::Router, token: &str, room_version: &str) -> String {
    let request = Request::builder()
        .method("POST")
        .uri("/_matrix/client/v3/createRoom")
        .header("Authorization", format!("Bearer {}", token))
        .header("Content-Type", "application/json")
        .body(Body::from(json!({ "preset": "public_chat", "room_version": room_version }).to_string()))
        .unwrap();

    let response = ServiceExt::<Request<Body>>::oneshot(app.clone(), request).await.unwrap();
    assert_eq!(response.status(), StatusCode::OK);

    let body = axum::body::to_bytes(response.into_body(), 2048).await.unwrap();
    let json: Value = serde_json::from_slice(&body).unwrap();
    json["room_id"].as_str().unwrap().to_string()
}

/// F-03: `re_sign_pdu_locally` must sign the **projected** event row.
///
/// The defect: the six membership call sites handed it a hand-assembled dict
/// holding only `event_id, room_id, sender, type, state_key, origin_server_ts,
/// origin, content`.  That dict is missing `depth` / `prev_events` /
/// `auth_events`, so the persisted `hashes.sha256` and `signatures` describe
/// bytes no peer can reproduce when the row is re-emitted as a full PDU — every
/// verifying server rejects the signature.
///
/// This drives `/invite` v2 (which persists the graph fields it was given, so
/// the projection is complete and the re-sign path actually runs), then checks
/// the persisted material against the projection of the same row:
/// the stored `hashes.sha256` must equal the projected content hash, and the
/// stored ed25519 signature must verify over the projected signature material.
#[tokio::test]
async fn invite_v2_stored_signature_covers_the_projected_pdu() {
    let Some((app, pool, key_id, _key_b64, signing_key, _cache)) = setup_federation_app().await else {
        return;
    };

    // Room version 11 is v3+: `event_id` is a reference hash and is not carried.
    let (token, creator_id) = register_user(&app, "creator").await;
    let room_id = create_room_with_version(&app, &token, "12").await;

    // The PDU's graph fields must reference a real persisted event.
    let create_event_id: String =
        sqlx::query_scalar("SELECT event_id FROM events WHERE room_id = $1 AND event_type = 'm.room.create' LIMIT 1")
            .bind(&room_id)
            .fetch_one(&*pool)
            .await
            .expect("the new room must have an m.room.create event");

    let invitee = "@invitee:localhost";
    let body = json!({
        "event": {
            "type": "m.room.member",
            "content": { "membership": "invite" },
            "sender": creator_id,
            "state_key": invitee,
            "room_id": room_id,
            "origin": "localhost",
            "origin_server_ts": chrono::Utc::now().timestamp_millis(),
            "depth": 3,
            "prev_events": [create_event_id.clone()],
            "auth_events": [create_event_id.clone()],
        },
        "room_version": "12"
    });
    let uri = format!("/_matrix/federation/v2/invite/{}/$path_event_id", room_id);
    let request = signed_fed_request("PUT", &uri, "localhost", &key_id, &signing_key, Some(&body));

    let response = ServiceExt::<Request<Body>>::oneshot(app.clone(), request).await.unwrap();
    assert_eq!(response.status(), StatusCode::OK, "invite_v2 must accept and persist the PDU");
    let response_body = axum::body::to_bytes(response.into_body(), 8192).await.unwrap();
    let response_json: Value = serde_json::from_slice(&response_body).unwrap();

    // The spec answer is `{"event": <signed PDU>}` — the inviting server verifies
    // the event it just handed us. The v12 PDU carries no `event_id` of its own
    // (the receiver derives it), so the persisted row is located by its
    // membership coordinates instead of by an ID echoed in the response.
    assert!(
        response_json["event"]["signatures"]["localhost"][&key_id].is_string(),
        "invite_v2 must answer with {{\"event\": <PDU>}} carrying the local signature under {key_id}: {response_json}"
    );
    let event_id: String = sqlx::query_scalar(
        "SELECT event_id FROM events WHERE room_id = $1 AND event_type = 'm.room.member' AND state_key = $2",
    )
    .bind(&room_id)
    .bind(invitee)
    .fetch_one(&*pool)
    .await
    .expect("the invite row must be persisted");

    // Read the persisted row back the same way the federation emitters do.
    let storage = EventStorage::new(&pool, "localhost".to_string());
    let records = storage.get_state_events(&room_id).await.expect("state rows must be readable");
    let record = records.iter().find(|r| r.event_id == event_id).expect("the invite row must be persisted");

    // The material must cover exactly the projected PDU a peer will receive.
    let (projected, completeness) = state_pdu("localhost", record, Some("12"));
    assert_eq!(completeness, PduCompleteness::Complete, "invite_v2 persisted depth/prev_events/auth_events");

    let stored_hashes = record.hashes.clone().expect("re_sign_pdu_locally must persist hashes");
    let stored_signatures = record.signatures.clone().expect("re_sign_pdu_locally must persist signatures");

    let expected_hash = compute_event_content_hash(&projected).expect("projected PDU must hash");
    assert_eq!(
        stored_hashes["sha256"].as_str(),
        Some(expected_hash.as_str()),
        "stored hashes.sha256 must hash the projected PDU, not a hand-built partial dict"
    );

    let signature_b64 = stored_signatures["localhost"][&key_id]
        .as_str()
        .unwrap_or_else(|| panic!("no local signature under {key_id}: {stored_signatures}"));
    let signature_bytes = STANDARD_NO_PAD.decode(signature_b64).expect("signature must be unpadded base64");
    let signature = ed25519_dalek::Signature::from_slice(&signature_bytes).expect("signature must be 64 bytes");

    // The signed bytes are the projection *with* its `hashes` attached:
    // redaction keeps `hashes` (only `signatures`, `unsigned` and `age_ts` are
    // stripped), so a verifier recomputes the material from the full PDU a peer
    // receives — exactly what this rebuilds via the production helper.
    let mut pdu_as_received = projected;
    assert!(apply_stored_signature_material(record, &mut pdu_as_received), "stored material must attach");
    let material = signature_material_bytes("12", &pdu_as_received).expect("projected signature material");
    signing_key
        .verifying_key()
        .verify_strict(&material, &signature)
        .expect("the stored signature must verify over the projected PDU a peer receives");
}

/// Build and sign a v12 `m.room.member` PDU as a **foreign** origin would, and
/// derive the event ID the receiver computes for it.
///
/// `invite_room_state` is passed through verbatim (the caller decides whether the
/// body carries it at all).
#[allow(clippy::too_many_arguments)]
fn signed_foreign_member_pdu(
    remote_origin: &str,
    remote_key_id: &str,
    remote_key_b64: &str,
    room_id: &str,
    sender: &str,
    state_key: &str,
    invite_room_state: Option<Value>,
) -> (Value, String, Value) {
    let content = json!({ "membership": "invite" });
    let prev_events = vec!["$parent:remote.example".to_string()];
    let auth_events = vec!["$create:remote.example".to_string()];
    let parts = synapse_common::pdu::PduParts {
        room_version: "12",
        event_id: None,
        room_id,
        sender,
        event_type: "m.room.member",
        content: &content,
        state_key: Some(state_key),
        origin_server_ts: 1_750_000_000_000,
        origin: remote_origin,
        depth: 7,
        prev_events: &prev_events,
        auth_events: &auth_events,
        redacts: None,
    };
    let mut pdu = synapse_common::pdu::build_pdu(&parts);
    let finalized =
        synapse_federation::event_finalize::finalize_local_pdu(&parts).expect("the origin's PDU must finalize");
    pdu.as_object_mut().expect("a PDU is a JSON object").insert("hashes".to_string(), finalized.hashes);
    synapse_federation::signing::sign_and_hash_event("12", remote_origin, remote_key_id, remote_key_b64, &mut pdu)
        .expect("the origin's PDU must sign");
    let event_id =
        synapse_common::event_id::resolve_received_event_id("12", &pdu).expect("the receiver must derive the event id");

    let mut body = json!({ "event": pdu, "room_version": "12" });
    if let Some(stripped) = invite_room_state {
        body.as_object_mut().expect("the body is an object").insert("invite_room_state".to_string(), stripped);
    }
    (body, event_id, content)
}

/// P-18: the stripped state the sender supplied must survive ingest, because a
/// first-contact invite leaves us **no** room state to project from.
///
/// Upstream stores it on the membership event's `unsigned.invite_room_state`
/// (`FederationHandler.on_invite_request` → `event.unsigned[...]`) and renders
/// `rooms.invite[*].invite_state` from it. Our sync path instead projects the
/// room's *own* state and fails closed without an `m.room.create` — for a room we
/// have never seen, that means the invite is persisted, the membership row
/// exists, and the invitee still sees nothing in `/sync`.
#[tokio::test]
async fn invite_v2_supplied_stripped_state_reaches_the_invitees_sync() {
    let Some((app, _pool, _key_id, _key_b64, _signing_key, cache)) = setup_federation_app().await else {
        return;
    };

    let remote_origin = "remote.example";
    let remote_key_id = "ed25519:remote_stripped_state";
    let remote_seed = [88u8; 32];
    let remote_signing_key = ed25519_dalek::SigningKey::from_bytes(&remote_seed);
    let remote_key_b64 = STANDARD_NO_PAD.encode(remote_seed);
    register_remote_verify_key(&cache, remote_origin, remote_key_id, &remote_signing_key).await;

    let (invitee_token, invitee_id) = register_user(&app, "stripped_invitee").await;
    let inviter = format!("@inviter:{remote_origin}");
    let room_id = format!("!stripped_invite_{}:localhost", rand::random::<u32>());

    let invite_room_state = json!([
        {
            "type": "m.room.create",
            "state_key": "",
            "content": { "creator": inviter, "room_version": "12" },
            "sender": inviter
        },
        {
            "type": "m.room.name",
            "state_key": "",
            "content": { "name": "Remote Room" },
            "sender": inviter
        }
    ]);
    let (body, event_id, _content) = signed_foreign_member_pdu(
        remote_origin,
        remote_key_id,
        &remote_key_b64,
        &room_id,
        &inviter,
        &invitee_id,
        Some(invite_room_state),
    );

    let uri = format!("/_matrix/federation/v2/invite/{room_id}/{event_id}");
    let request =
        signed_fed_request_as("PUT", &uri, remote_origin, "localhost", remote_key_id, &remote_signing_key, Some(&body));
    let response = ServiceExt::<Request<Body>>::oneshot(app.clone(), request).await.unwrap();
    let status = response.status();
    let response_body = axum::body::to_bytes(response.into_body(), 16384).await.unwrap();
    let response_json: Value = serde_json::from_slice(&response_body).unwrap();
    assert_eq!(status, StatusCode::OK, "the invite must be accepted, got {status}: {response_json}");

    // The invitee must see the invite, rendered from the state the sender gave us.
    let sync_request = Request::builder()
        .method("GET")
        .uri("/_matrix/client/v3/sync?timeout=0")
        .header("Authorization", format!("Bearer {invitee_token}"))
        .body(Body::empty())
        .unwrap();
    let sync_response = ServiceExt::<Request<Body>>::oneshot(app.clone(), sync_request).await.unwrap();
    assert_eq!(sync_response.status(), StatusCode::OK);
    let sync_body = axum::body::to_bytes(sync_response.into_body(), 256 * 1024).await.unwrap();
    let sync_json: Value = serde_json::from_slice(&sync_body).unwrap();

    let invite = &sync_json["rooms"]["invite"][&room_id];
    assert!(!invite.is_null(), "the invitee must see `rooms.invite[{room_id}]`: {sync_json}");
    let stripped_types: Vec<&str> = invite["invite_state"]["events"]
        .as_array()
        .unwrap_or_else(|| panic!("invite_state.events must be an array: {invite}"))
        .iter()
        .filter_map(|event| event["type"].as_str())
        .collect();
    assert!(
        stripped_types.contains(&"m.room.create") && stripped_types.contains(&"m.room.name"),
        "invite_state must carry the sender's stripped state, got {stripped_types:?}"
    );
}

/// P-15 / V-2b: an invite that arrives **signed by its origin** must keep that
/// signature — in the row *and* in the `{"event": …}` response.
///
/// The inviting server re-checks the event we hand back
/// (`synapse/federation/federation_client.py::send_invite` →
/// `_check_sigs_and_hash` → `_check_sigs_on_pdu`), which verifies the signature
/// of the **sender domain**. If we replace the origin's signature with our own,
/// every inbound invite dies with `403 … Not signed by <sender>`, even though we
/// accepted and persisted it.
///
/// This is the same invariant `/send` already upholds
/// (`transaction.rs`: "Persist the **origin server's** signature/hash pair …
/// the same post-insert mechanism the local signing path and the inbound
/// membership path use"). The invite path skipped it.
#[tokio::test]
async fn invite_v2_keeps_the_origin_signature_in_the_row_and_the_response() {
    let Some((app, pool, _key_id, _key_b64, _signing_key, cache)) = setup_federation_app().await else {
        return;
    };

    let remote_origin = "remote.example";
    let remote_key_id = "ed25519:remote_invite_sig";
    let remote_seed = [77u8; 32];
    let remote_signing_key = ed25519_dalek::SigningKey::from_bytes(&remote_seed);
    let remote_key_b64 = STANDARD_NO_PAD.encode(remote_seed);
    register_remote_verify_key(&cache, remote_origin, remote_key_id, &remote_signing_key).await;

    let (_invitee_token, invitee_id) = register_user(&app, "invitee").await;
    let inviter = format!("@inviter:{remote_origin}");

    // A room we do not host: this is the first-contact case, so the invite is
    // persisted as an outlier exactly like a real cross-implementation invite.
    let room_id = format!("!origin_sig_invite_{}:localhost", rand::random::<u32>());
    let (body, event_id, _content) =
        signed_foreign_member_pdu(remote_origin, remote_key_id, &remote_key_b64, &room_id, &inviter, &invitee_id, None);
    let uri = format!("/_matrix/federation/v2/invite/{room_id}/{event_id}");
    let request =
        signed_fed_request_as("PUT", &uri, remote_origin, "localhost", remote_key_id, &remote_signing_key, Some(&body));

    let response = ServiceExt::<Request<Body>>::oneshot(app, request).await.unwrap();
    let status = response.status();
    let response_body = axum::body::to_bytes(response.into_body(), 16384).await.unwrap();
    let response_json: Value = serde_json::from_slice(&response_body).unwrap();
    assert_eq!(status, StatusCode::OK, "a signed invite must be accepted, got {status}: {response_json}");

    // 1. The row keeps the origin's signature next to ours (or instead of it —
    //    what matters is that the origin's block survives).
    let stored: Value = sqlx::query_scalar("SELECT signatures FROM events WHERE event_id = $1")
        .bind(&event_id)
        .fetch_one(pool.as_ref())
        .await
        .expect("the invite row must be persisted with signature material");
    assert!(
        stored[remote_origin][remote_key_id].is_string(),
        "the persisted row must keep the origin's signature, got {stored}"
    );

    // 2. The response echoes it…
    let echoed = response_json.get("event").expect("v2 invite answers with an event");
    let sig_b64 = echoed["signatures"][remote_origin][remote_key_id]
        .as_str()
        .unwrap_or_else(|| panic!("the echoed PDU must carry the origin's signature, got {echoed}"));
    assert_eq!(
        sig_b64,
        stored[remote_origin][remote_key_id].as_str().expect("stored signature is a string"),
        "the echoed signature must be the one we stored, byte for byte"
    );

    // 3. …and it verifies over the bytes we actually send, which is exactly the
    //    check the inviting server performs before accepting its own invite.
    let material = signature_material_bytes("12", echoed).expect("the echoed PDU must have signature material");
    let signature_bytes = STANDARD_NO_PAD.decode(sig_b64).expect("signature must be unpadded base64");
    let signature = ed25519_dalek::Signature::from_slice(&signature_bytes).expect("signature must be 64 bytes");
    remote_signing_key
        .verifying_key()
        .verify_strict(&material, &signature)
        .expect("the sender must be able to verify the PDU we echo back");

    // 4. …and the sender's *second* check: the content hash
    //    (`federation_base._check_sigs_and_hash` → `check_event_content_hash`,
    //    computed over the unredacted PDU). A field we add that the origin never
    //    put on the wire changes that hash, and the peer then treats its own
    //    invite as tampered and silently redacts it.
    synapse_federation::signing::verify_event_content_hash(echoed)
        .expect("the echoed PDU must reproduce the origin's content hash");
}

// ---------------------------------------------------------------------------
// Tests: U-13-R9 — the v≤11 write path must persist `depth` / `prev_events` /
// `auth_events`, not only the v12 `create_event_with_pdu` path
// ---------------------------------------------------------------------------

/// Read one persisted row back through the same column list the federation
/// projectors use. Dynamic SQL on purpose: macros inside `tests/` are not part
/// of the `.sqlx` offline cache (rule R9 / D-13).
async fn read_persisted_event(pool: &sqlx::PgPool, event_id: &str) -> synapse_storage::event::StateEvent {
    sqlx::query_as(
        "SELECT event_id, room_id, COALESCE(sender, user_id) as sender, event_type, content, state_key, \
         unsigned, is_redacted, origin_server_ts, depth, NULL::BIGINT as processed_at, not_before, status, origin, \
         user_id, stream_ordering, prev_events, auth_events, signatures, hashes \
         FROM events WHERE event_id = $1",
    )
    .bind(event_id)
    .fetch_one(pool)
    .await
    .expect("the persisted event row must be readable")
}

/// U-13-R9: a client message in a v12 room must persist its graph metadata.
///
/// `MessagingService::send_message` (DB-03-a) is the one locally-producing
/// caller that writes its event inside a caller-managed transaction. The
/// write-path decorator used to skip graph resolution for **every**
/// `tx = Some(..)` call, so a v≤11 message landed with `depth` /
/// `prev_events` / `auth_events` SQL `NULL` even though the v12 branch of
/// `MessagingService::create_event` supplied them explicitly. Such a row
/// projects as `PduCompleteness::MissingGraphMetadata`: it is emitted unsigned
/// and the broadcast path refuses to build its outbound PDU at all.
#[tokio::test]
async fn client_message_in_v12_room_persists_graph_metadata() {
    let Some((app, pool, _key_id, _key_b64, _signing_key, _cache)) = setup_federation_app().await else {
        return;
    };

    for room_version in ["12"] {
        let (token, _creator_id) = register_user(&app, "creator").await;
        let room_id = create_room_with_version(&app, &token, room_version).await;

        let txn_id = format!("txn_{}", rand::random::<u32>());
        let request = Request::builder()
            .method("PUT")
            .uri(format!("/_matrix/client/v3/rooms/{room_id}/send/m.room.message/{txn_id}"))
            .header("Authorization", format!("Bearer {token}"))
            .header("Content-Type", "application/json")
            .body(Body::from(json!({ "msgtype": "m.text", "body": "hello" }).to_string()))
            .unwrap();
        let response = ServiceExt::<Request<Body>>::oneshot(app.clone(), request).await.unwrap();
        assert_eq!(response.status(), StatusCode::OK, "v{room_version}: the message must be accepted");
        let body = axum::body::to_bytes(response.into_body(), 4096).await.unwrap();
        let event_id = serde_json::from_slice::<Value>(&body).unwrap()["event_id"]
            .as_str()
            .expect("send must return the persisted event id")
            .to_string();

        let record = read_persisted_event(&pool, &event_id).await;

        assert!(record.depth.is_some(), "v{room_version}: `depth` must be persisted for a locally-created event");
        assert!(
            record.prev_events.as_ref().and_then(Value::as_array).is_some_and(|parents| !parents.is_empty()),
            "v{room_version}: `prev_events` must name the room's forward extremities, got {:?}",
            record.prev_events
        );
        assert!(
            record.auth_events.as_ref().and_then(Value::as_array).is_some_and(|auth| !auth.is_empty()),
            "v{room_version}: `auth_events` must be persisted, got {:?}",
            record.auth_events
        );

        let (pdu, completeness) = state_pdu("localhost", &record, Some(room_version));
        assert_eq!(
            completeness,
            PduCompleteness::Complete,
            "v{room_version}: a locally-created event must project Complete, got {completeness:?}: {pdu}"
        );
        assert_eq!(record.depth, pdu.get("depth").and_then(Value::as_i64));
    }
}

/// U-13-R9 (membership path): a `/send_join` v2 into a v12 room must persist the
/// graph metadata, leave a local signature on the stored row, **and** echo the
/// signed PDU back as `event` (FED-01 — the spec requires it, and it is the only
/// copy the joining server can verify).
///
/// The join event is created locally by the resident server, so it exercises
/// the same write path as the client message above; `project_and_sign_pdu_locally`
/// only signs a projection that is `Complete`.
#[tokio::test]
async fn send_join_v2_in_v12_room_persists_graph_metadata_and_signs_the_member_event() {
    let Some((app, pool, key_id, _key_b64, _signing_key, _cache)) = setup_federation_app().await else {
        return;
    };

    let (token, _creator_id) = register_user(&app, "creator").await;
    let room_id = create_room_with_version(&app, &token, "12").await;

    let joiner = "@joiner:localhost";
    // `room_memberships.user_id` has an FK onto `users`: the resident server
    // records the joining user's membership, so a federated user needs its local
    // shadow row first (same shape the other federation tests seed).
    sqlx::query(
        "INSERT INTO users (user_id, username, created_ts) VALUES ($1, $2, $3) ON CONFLICT (user_id) DO NOTHING",
    )
    .bind(joiner)
    .bind("joiner")
    .bind(chrono::Utc::now().timestamp_millis())
    .execute(&*pool)
    .await
    .expect("the joining user must be seedable as a local shadow row");
    let path_event_id = format!("$join_{}:localhost", rand::random::<u32>());
    let body = build_join_event_body(&room_id, &path_event_id, joiner);
    let uri = format!("/_matrix/federation/v2/send_join/{room_id}/{path_event_id}");
    let request = signed_fed_request("PUT", &uri, "localhost", &key_id, &_signing_key, Some(&body));

    let response = ServiceExt::<Request<Body>>::oneshot(app.clone(), request).await.unwrap();
    assert_eq!(response.status(), StatusCode::OK, "a public-room send_join must succeed");

    // FED-01: the response must carry the signed join PDU as `event` — without
    // it the joining server has nothing it can verify against the resident
    // server's key.
    let body_bytes = axum::body::to_bytes(response.into_body(), 64 * 1024).await.unwrap();
    let response_body: Value = serde_json::from_slice(&body_bytes).expect("send_join v2 must answer with JSON");
    let echoed_event =
        response_body.get("event").expect("FED-01: send_join v2 must return the signed join PDU as `event`");
    assert_eq!(
        echoed_event.get("type").and_then(Value::as_str),
        Some("m.room.member"),
        "the echoed `event` must be the join PDU, got {echoed_event}"
    );
    assert_eq!(
        echoed_event.get("state_key").and_then(Value::as_str),
        Some(joiner),
        "the echoed `event` must be the joining user's member event, got {echoed_event}"
    );
    assert!(
        echoed_event.get("hashes").is_some(),
        "FED-01: the echoed join PDU must carry `hashes`, got {echoed_event}"
    );
    assert!(
        echoed_event.get("signatures").and_then(Value::as_object).is_some_and(|sigs| !sigs.is_empty()),
        "FED-01: the echoed join PDU must carry a non-empty `signatures`, got {echoed_event}"
    );

    let storage = EventStorage::new(&pool, "localhost".to_string());
    let records = storage.get_state_events(&room_id).await.expect("state rows must be readable");
    let record = records
        .iter()
        .find(|candidate| candidate.state_key.as_deref() == Some(joiner))
        .expect("the join event must be persisted as room state");

    assert!(record.depth.is_some(), "send_join must persist `depth` for the locally-created member event");
    assert!(
        record.prev_events.as_ref().and_then(Value::as_array).is_some_and(|parents| !parents.is_empty()),
        "send_join must persist `prev_events`, got {:?}",
        record.prev_events
    );
    assert!(
        record.auth_events.as_ref().and_then(Value::as_array).is_some_and(|auth| !auth.is_empty()),
        "send_join must persist `auth_events`, got {:?}",
        record.auth_events
    );

    let (_, completeness) = state_pdu("localhost", record, Some("12"));
    assert_eq!(completeness, PduCompleteness::Complete, "the persisted join event must project Complete");

    assert!(record.hashes.is_some(), "project_and_sign_pdu_locally must persist hashes on the join event");
    assert!(record.signatures.is_some(), "project_and_sign_pdu_locally must persist signatures on the join event");
}

// ---------------------------------------------------------------------------
// Tests: the graph-aware write path must persist `event_edges`, not only the
// graph columns
// ---------------------------------------------------------------------------

/// `event_edges` rows written for `event_id`, i.e. the parents it points at.
///
/// Dynamic SQL on purpose: macros inside `tests/` are not part of the `.sqlx`
/// offline cache (rule R9 / D-13).
async fn persisted_prev_event_ids(pool: &sqlx::PgPool, event_id: &str) -> Vec<String> {
    sqlx::query_scalar("SELECT prev_event_id FROM event_edges WHERE event_id = $1")
        .bind(event_id)
        .fetch_all(pool)
        .await
        .expect("the event's DAG edges must be readable")
}

/// `PUT /rooms/{room}/state/{type}` — the client state route writes through
/// `MessagingService::create_event(.., None)`, i.e. the **auto-commit** path.
async fn send_state_event(app: &axum::Router, token: &str, room_id: &str, event_type: &str, content: Value) -> String {
    let request = Request::builder()
        .method("PUT")
        .uri(format!("/_matrix/client/v3/rooms/{room_id}/state/{event_type}"))
        .header("Authorization", format!("Bearer {token}"))
        .header("Content-Type", "application/json")
        .body(Body::from(content.to_string()))
        .unwrap();
    let response = ServiceExt::<Request<Body>>::oneshot(app.clone(), request).await.unwrap();
    assert_eq!(response.status(), StatusCode::OK, "{event_type} must be accepted");
    let body = axum::body::to_bytes(response.into_body(), 4096).await.unwrap();
    serde_json::from_slice::<Value>(&body).unwrap()["event_id"]
        .as_str()
        .expect("a state write must return the persisted event id")
        .to_string()
}

/// `PUT /rooms/{room}/send/m.room.message/{txn}` — `MessagingService::send_message`
/// writes inside a **caller-managed transaction** (`DB-03-a`).
async fn send_client_message(app: &axum::Router, token: &str, room_id: &str) -> String {
    let txn_id = format!("txn_{}", rand::random::<u32>());
    let request = Request::builder()
        .method("PUT")
        .uri(format!("/_matrix/client/v3/rooms/{room_id}/send/m.room.message/{txn_id}"))
        .header("Authorization", format!("Bearer {token}"))
        .header("Content-Type", "application/json")
        .body(Body::from(json!({ "msgtype": "m.text", "body": "hello" }).to_string()))
        .unwrap();
    let response = ServiceExt::<Request<Body>>::oneshot(app.clone(), request).await.unwrap();
    assert_eq!(response.status(), StatusCode::OK, "the message must be accepted");
    let body = axum::body::to_bytes(response.into_body(), 4096).await.unwrap();
    serde_json::from_slice::<Value>(&body).unwrap()["event_id"]
        .as_str()
        .expect("send must return the persisted event id")
        .to_string()
}

/// U-13-R9 follow-up: writing the graph **columns** is not enough — the write
/// path must also persist `event_edges`.
///
/// `create_event_with_pdu` inserts `depth` / `prev_events` / `auth_events` but
/// never inserted into `event_edges`. After `11bf5455d` every known room version
/// routes through that method, so **every** locally-created event had no edge.
/// `get_forward_extremities_in_room` derives the room's tips solely from
/// `event_edges` (`NOT EXISTS (SELECT 1 FROM event_edges g WHERE g.prev_event_id
/// = e.event_id)`), so each local event stayed a forward extremity for ever:
/// `prev_events` grew without bound and `/get_missing_events` could not walk
/// back through the DAG.
///
/// Both locally-producing call shapes are driven end to end:
///   * client state events (`MessagingService::create_event(.., None)`) — the
///     auto-commit path;
///   * `/send` (`MessagingService::send_message`) — the caller-transaction path.
#[tokio::test]
async fn local_events_persist_event_edges_on_both_write_shapes() {
    let Some((app, pool, _key_id, _key_b64, _signing_key, _cache)) = setup_federation_app().await else {
        return;
    };

    let (token, _creator_id) = register_user(&app, "creator").await;
    let room_id = create_room_with_version(&app, &token, "12").await;
    let storage = EventStorage::new(&pool, "localhost".to_string());

    // Auto-commit local writes (`tx = None`): two client state events.
    let first = send_state_event(&app, &token, &room_id, "m.room.topic", json!({ "topic": "edges" })).await;
    let second = send_state_event(&app, &token, &room_id, "m.room.name", json!({ "name": "edges" })).await;

    let edges = persisted_prev_event_ids(&pool, &second).await;
    assert!(
        edges.contains(&first),
        "the auto-commit local write must record the DAG edge {second} -> {first}, got {edges:?}"
    );

    let extremities =
        storage.get_forward_extremities_in_room(&room_id, 10).await.expect("extremities must be readable");
    assert!(
        !extremities.contains(&first),
        "an event a later local write points at must stop being a forward extremity, got {extremities:?}"
    );

    // Caller-transaction local write (`tx = Some(..)`): `send_message`.
    let third = send_client_message(&app, &token, &room_id).await;

    let edges = persisted_prev_event_ids(&pool, &third).await;
    assert!(
        edges.contains(&second),
        "the caller-transaction local write must record the DAG edge {third} -> {second}, got {edges:?}"
    );

    let extremities =
        storage.get_forward_extremities_in_room(&room_id, 10).await.expect("extremities must be readable");
    assert!(
        !extremities.contains(&second),
        "send_message's graph-aware write must stop reporting its parent as a tip, got {extremities:?}"
    );
}

// ---------------------------------------------------------------------------
// Tests: U-13-R10 — the federation knock must answer with the ID it persisted
// ---------------------------------------------------------------------------

/// Defect B (U-13-R10): pre-fix `/knock` minted
/// `format!("${}", generate_event_id(..))` — a `$$…` placeholder — called
/// `create_event(.., None)` and **discarded** the returned `RoomEvent`, then
/// answered with the placeholder. The write entry had already replaced that
/// placeholder with the v3+ reference hash, so the response named an event that
/// was never persisted (decision §4.1: the caller consumes the write entry's ID).
#[tokio::test]
async fn knock_room_returns_the_id_of_the_persisted_row() {
    let Some((app, pool, _local_key_id, _local_key_b64, _local_signing_key, cache)) = setup_federation_app().await
    else {
        return;
    };

    let remote_origin = "remote.example";
    let remote_key_id = "ed25519:remote_knock";
    let remote_signing_key = ed25519_dalek::SigningKey::from_bytes(&[101u8; 32]);
    register_remote_verify_key(&cache, remote_origin, remote_key_id, &remote_signing_key).await;

    // The remote server needs a non-banned member in the room for
    // `validate_federation_origin_can_observe_room` to let the knock through to
    // the event write. Seed the invite the same way the send_join test seeds its
    // shadow user (there is no outbound federation transport in this sandbox).
    let (token, creator_id) = register_user(&app, "knockcreator").await;
    let room_id = create_private_room(&app, &token).await;
    let invitee = "@invitee:remote.example";
    sqlx::query(
        "INSERT INTO users (user_id, username, created_ts) VALUES ($1, $2, $3) ON CONFLICT (user_id) DO NOTHING",
    )
    .bind(invitee)
    .bind("invitee_remote")
    .bind(chrono::Utc::now().timestamp_millis())
    .execute(&*pool)
    .await
    .expect("the remote invitee must be seedable as a local shadow row");
    sqlx::query(
        "INSERT INTO room_memberships (room_id, user_id, membership, sender, event_type, updated_ts) \
         VALUES ($1, $2, 'invite', $3, 'm.room.member', $4)",
    )
    .bind(&room_id)
    .bind(invitee)
    .bind(&creator_id)
    .bind(chrono::Utc::now().timestamp_millis())
    .execute(&*pool)
    .await
    .expect("the invite membership must be seedable");

    let knocker = "@knocker:remote.example";
    let body = build_knock_event_body(&room_id, knocker, remote_origin);
    let uri = format!("/_matrix/federation/v1/knock/{room_id}/{knocker}");
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
    assert_eq!(response.status(), StatusCode::OK, "the knock must reach the event write");
    let bytes = axum::body::to_bytes(response.into_body(), 4096).await.unwrap();
    let json: Value = serde_json::from_slice(&bytes).unwrap();
    let event_id =
        json["event"]["event_id"].as_str().expect("the knock response must carry event.event_id").to_string();

    assert!(!event_id.starts_with("$$"), "the placeholder must not carry a double `$`: {event_id}");
    let stored: Option<String> = sqlx::query_scalar("SELECT event_id FROM events WHERE event_id = $1")
        .bind(&event_id)
        .fetch_optional(&*pool)
        .await
        .expect("the events table must be queryable");
    assert_eq!(
        stored.as_deref(),
        Some(event_id.as_str()),
        "the knock must answer with the ID of the row it persisted, not a placeholder"
    );
    assert!(
        !event_id.contains(":localhost"),
        "a v3+ knock event ID is a reference hash with no origin suffix: {event_id}"
    );
}
