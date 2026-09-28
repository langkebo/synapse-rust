//! Federation send_transaction integration tests (P2 GAP-03).
//!
//! Covers the inbound PDU hot path: signature auth → PDU validation → persistence.
//! Tests 401 rejection for missing/invalid signatures and 200 response shapes
//! for empty and invalid PDU payloads.

use axum::{
    body::Body,
    http::{Request, StatusCode},
};
use base64::engine::general_purpose::STANDARD_NO_PAD;
use base64::Engine as _;
use ed25519_dalek::Signer;
use serde_json::{json, Value};
use sqlx::Row;
use std::sync::Arc;
use synapse_common::event_id::resolve_received_event_id;
use synapse_common::pdu::{build_pdu, PduParts};
use synapse_common::test_isolation::IsolatedTestPool;
use synapse_federation::event_finalize::finalize_local_pdu;
use synapse_federation::signing::{compute_event_content_hash, sign_and_hash_event};
use synapse_storage::event::EventStorage;
use synapse_web::federation::signing::canonical_federation_request_bytes;
use tower::ServiceExt;

/// Workspace migration baseline (v12), compiled in for the shared
/// [`IsolatedTestPool`]: new DB coverage clones this real baseline instead of
/// building its own schema (R9). The constant must be byte-identical to the other
/// callers' copies — `tests/unit/test_isolation_unification_tests.rs` pins it.
const BASELINE_SQL: &str = include_str!("../../migrations/00000000_unified_schema_v12.sql");

async fn setup_federation_txn_test_app(
    key_id: &str,
    signing_key_b64: &str,
) -> Option<(axum::Router, Arc<sqlx::PgPool>)> {
    let pool = super::require_test_pool().await;
    let app = build_federation_txn_app(pool.clone(), key_id, signing_key_b64).await;
    Some((app, pool))
}

/// Build the full router over `pool` with the federation wiring these tests need.
///
/// Split out of [`setup_federation_txn_test_app`] so the signature-assertion
/// tests below can drive the same configuration from an [`IsolatedTestPool`].
async fn build_federation_txn_app(pool: Arc<sqlx::PgPool>, key_id: &str, signing_key_b64: &str) -> axum::Router {
    let mut container = synapse_services::ServiceContainer::new_test_with_pool(pool).await;
    super::config_mut(&mut container).server.name = "localhost".to_string();
    container.core.server_name = "localhost".to_string();
    super::config_mut(&mut container).federation.enabled = true;
    super::config_mut(&mut container).federation.allow_ingress = true;
    super::config_mut(&mut container).federation.server_name = "localhost".to_string();
    super::config_mut(&mut container).federation.key_id = Some(key_id.to_string());
    super::config_mut(&mut container).federation.signing_key = Some(signing_key_b64.to_string());
    // EDU ingress is off by default (`process_inbound_edus` / `process_inbound_presence_edus`
    // both default to `false`), so the presence-EDU test below would silently assert nothing.
    super::config_mut(&mut container).federation.process_inbound_edus = true;
    super::config_mut(&mut container).federation.process_inbound_presence_edus = true;
    // The `KeyRotationManager` is constructed *inside* `ServiceContainer::new`, so the
    // `federation.signing_key` override above never reaches it — every PDU signing path
    // silently no-ops without this. `initialize` installs the in-memory key *before* its
    // at-rest persistence policy check (which refuses plaintext when no master key is
    // configured), so assert on the installed key, not on the (deliberately
    // unpersisted) result.
    let init_result = container.federation.key_rotation_manager.initialize(signing_key_b64, key_id).await;
    assert!(
        container.federation.key_rotation_manager.get_current_key().await.expect("key read").is_some(),
        "the deterministic test signing key must be installed for PDU signing (initialize returned {init_result:?})"
    );
    let cache =
        std::sync::Arc::new(synapse_rust::cache::CacheManager::new(&synapse_rust::cache::CacheConfig::default()));
    let state = synapse_web::routes::state::AppState::new(container, cache);
    synapse_web::create_router(state)
}

fn signed_federation_request(
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

// ============================================================================
// Test 1: Missing Authorization header → 401
// ============================================================================

#[tokio::test]
async fn test_send_transaction_rejects_missing_signature() {
    let pool = super::require_test_pool().await;
    let mut container = synapse_services::ServiceContainer::new_test_with_pool(pool.clone()).await;
    super::config_mut(&mut container).federation.enabled = true;
    super::config_mut(&mut container).federation.allow_ingress = true;
    let cache =
        std::sync::Arc::new(synapse_rust::cache::CacheManager::new(&synapse_rust::cache::CacheConfig::default()));
    let state = synapse_web::routes::state::AppState::new(container, cache);
    let app = synapse_web::create_router(state);

    let body = json!({
        "origin": "localhost",
        "pdus": []
    });

    let request = Request::builder()
        .method("PUT")
        .uri("/_matrix/federation/v1/send/txn1")
        .header("Content-Type", "application/json")
        .body(Body::from(body.to_string()))
        .unwrap();

    let response = ServiceExt::<Request<Body>>::oneshot(app, request).await.unwrap();
    assert_eq!(response.status(), StatusCode::UNAUTHORIZED);

    let resp_body = axum::body::to_bytes(response.into_body(), 2048).await.unwrap();
    let json: Value = serde_json::from_slice(&resp_body).unwrap();
    assert_eq!(json["errcode"], "M_UNAUTHORIZED");
}

// ============================================================================
// Test 2: Invalid signature (wrong origin) → 401
// ============================================================================

#[tokio::test]
async fn test_send_transaction_rejects_invalid_signature() {
    let key_id = "ed25519:txn_test";
    let signing_key_seed = [99u8; 32];
    let signing_key_b64 = STANDARD_NO_PAD.encode(signing_key_seed);
    let _signing_key = ed25519_dalek::SigningKey::from_bytes(&signing_key_seed);

    // Use a different key for signing — will fail signature verification.
    let wrong_key_seed = [88u8; 32];
    let wrong_key = ed25519_dalek::SigningKey::from_bytes(&wrong_key_seed);

    let Some((app, _pool)) = setup_federation_txn_test_app(key_id, &signing_key_b64).await else {
        return;
    };

    let body = json!({
        "origin": "localhost",
        "pdus": []
    });

    let request = signed_federation_request(
        "PUT",
        "/_matrix/federation/v1/send/txn2",
        "localhost",
        key_id,
        &wrong_key,
        Some(&body),
    );

    let response = ServiceExt::<Request<Body>>::oneshot(app, request).await.unwrap();
    assert_eq!(response.status(), StatusCode::UNAUTHORIZED);

    let resp_body = axum::body::to_bytes(response.into_body(), 2048).await.unwrap();
    let json: Value = serde_json::from_slice(&resp_body).unwrap();
    assert_eq!(json["errcode"], "M_UNAUTHORIZED");
}

// ============================================================================
// Test 3: Empty PDUs with valid signature → 200 with empty results
// ============================================================================

#[tokio::test]
async fn test_send_transaction_with_empty_pdus_returns_ok() {
    let key_id = "ed25519:txn_test";
    let signing_key_seed = [98u8; 32];
    let signing_key_b64 = STANDARD_NO_PAD.encode(signing_key_seed);
    let signing_key = ed25519_dalek::SigningKey::from_bytes(&signing_key_seed);

    let Some((app, _pool)) = setup_federation_txn_test_app(key_id, &signing_key_b64).await else {
        return;
    };

    let body = json!({
        "origin": "localhost",
        "pdus": []
    });

    let request = signed_federation_request(
        "PUT",
        "/_matrix/federation/v1/send/txn3",
        "localhost",
        key_id,
        &signing_key,
        Some(&body),
    );

    let response = ServiceExt::<Request<Body>>::oneshot(app, request).await.unwrap();
    assert_eq!(response.status(), StatusCode::OK);

    let resp_body = axum::body::to_bytes(response.into_body(), 2048).await.unwrap();
    let json: Value = serde_json::from_slice(&resp_body).unwrap();
    // Empty PDUs → empty results array.
    assert!(json["results"].as_array().is_some_and(|r| r.is_empty()), "Expected empty results array, got: {json}");
}

// ============================================================================
// Test 4: Valid signature + invalid PDU → 200 with error in results
// ============================================================================

#[tokio::test]
async fn test_send_transaction_with_invalid_pdu_returns_result_error() {
    let key_id = "ed25519:txn_test";
    let signing_key_seed = [97u8; 32];
    let signing_key_b64 = STANDARD_NO_PAD.encode(signing_key_seed);
    let signing_key = ed25519_dalek::SigningKey::from_bytes(&signing_key_seed);

    let Some((app, _pool)) = setup_federation_txn_test_app(key_id, &signing_key_b64).await else {
        return;
    };

    // PDU with no hashes/signatures will fail content hash verification.
    let invalid_pdu = json!({
        "event_id": "$test_event:localhost",
        "room_id": "!test_room:localhost",
        "sender": "@test_user:localhost",
        "type": "m.room.message",
        "origin": "localhost",
        "origin_server_ts": 999,
        "content": { "body": "test", "msgtype": "m.text" }
    });

    let body = json!({
        "origin": "localhost",
        "pdus": [invalid_pdu]
    });

    let request = signed_federation_request(
        "PUT",
        "/_matrix/federation/v1/send/txn4",
        "localhost",
        key_id,
        &signing_key,
        Some(&body),
    );

    let response = ServiceExt::<Request<Body>>::oneshot(app, request).await.unwrap();
    // Handler returns 200 even when PDUs fail — errors are in the results array.
    assert_eq!(response.status(), StatusCode::OK);

    let resp_body = axum::body::to_bytes(response.into_body(), 4096).await.unwrap();
    let json: Value = serde_json::from_slice(&resp_body).unwrap();

    let results = json["results"].as_array().expect("results should be an array");
    assert!(!results.is_empty(), "Expected non-empty results for invalid PDU");

    let first = &results[0];
    assert_eq!(first["event_id"], "$test_event:localhost");
    assert!(first.get("error").is_some(), "Expected error field in PDU result, got: {first}");
}

// ============================================================================
// Test 5: Valid signed PDU → 200 with `success`, persisted under the derived ID
// ============================================================================
//
// The previous version of this test sent a PDU for a room that did not exist,
// signed it with a room version the receiver could not resolve, and accepted
// *either* `success` or `error` — so the entire inbound verification half was
// untestable.  This version creates a real v11 room (v3+: the ID is a reference
// hash), builds the PDU through the production pipeline
// (`build_pdu` → `finalize_local_pdu` → `sign_and_hash_event`), and asserts the
// per-PDU `success` plus the persisted row: its derived ID, `hashes` and
// `signatures`.

/// Room version 11 is v3+: the PDU carries no `event_id` and the receiver must
/// derive it from the reference hash (spec room v3 "Event format").
// G-1: only room v12 is creatable, so the shared fixture room is v12. The
// assertions in this file are about signature material and event identity, which
// are identical for v11 and v12 outside the create event (whose `room_id` is
// omitted in v12 — pinned separately by MSC4307_ROOM_VERSION below).
const SIG_ASSERT_ROOM_VERSION: &str = "12";

/// One v11 room plus a fully signed PDU ready to PUT to `/send/{txnId}`.
struct SignedPduFixture {
    app: axum::Router,
    pool: Arc<sqlx::PgPool>,
    key_id: String,
    signing_key: ed25519_dalek::SigningKey,
    /// Unpadded Base64 of the same secret, for re-signing a deliberately
    /// malformed PDU ([`sign_and_hash_event`] takes the Base64 form).
    signing_key_b64: String,
    room_id: String,
    /// The PDU as the sending server would emit it (no `event_id`).
    pdu: Value,
    /// The ID the sender finalized; the receiver must derive the same one.
    derived_event_id: String,
}

impl SignedPduFixture {
    /// PUT `pdu` to `/_matrix/federation/v1/send/{txn_id}` and return the body.
    async fn send(&self, txn_id: &str, pdu: &Value) -> Value {
        let body = json!({ "origin": "localhost", "pdus": [pdu] });
        let request = signed_federation_request(
            "PUT",
            &format!("/_matrix/federation/v1/send/{txn_id}"),
            "localhost",
            &self.key_id,
            &self.signing_key,
            Some(&body),
        );
        let response = ServiceExt::<Request<Body>>::oneshot(self.app.clone(), request).await.unwrap();
        assert_eq!(
            response.status(),
            StatusCode::OK,
            "the transaction endpoint answers 200 even for rejected PDUs; the verdict is in `results`"
        );
        let bytes = axum::body::to_bytes(response.into_body(), 8192).await.unwrap();
        serde_json::from_slice(&bytes).expect("the response body must be JSON")
    }

    /// The single per-PDU result entry (one PDU in → one result out).
    fn single_result(response: &Value) -> &Value {
        let results = response["results"].as_array().expect("results must be an array");
        assert_eq!(results.len(), 1, "one PDU in → one result out: {response}");
        &results[0]
    }
}

/// Register a user through the client API, returning `(access_token, user_id)`.
async fn register_user_via_client(app: &axum::Router, prefix: &str) -> (String, String) {
    let username = format!("{prefix}_{}", uuid::Uuid::new_v4().as_simple());
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
    assert_eq!(response.status(), StatusCode::OK, "test user registration must succeed");
    let bytes = axum::body::to_bytes(response.into_body(), 4096).await.unwrap();
    let json: Value = serde_json::from_slice(&bytes).unwrap();
    (
        json["access_token"].as_str().expect("registration must return an access token").to_string(),
        json["user_id"].as_str().expect("registration must return a user id").to_string(),
    )
}

/// Create a room of `room_version` through the client API so the receiver can
/// resolve the room version from the `rooms` row (`inbound_pdu_room_version`
/// never guesses).
async fn create_room_with_version(app: &axum::Router, token: &str, room_version: &str) -> String {
    let request = Request::builder()
        .method("POST")
        .uri("/_matrix/client/v3/createRoom")
        .header("Authorization", format!("Bearer {token}"))
        .header("Content-Type", "application/json")
        .body(Body::from(json!({ "preset": "public_chat", "room_version": room_version }).to_string()))
        .unwrap();

    let response = ServiceExt::<Request<Body>>::oneshot(app.clone(), request).await.unwrap();
    assert_eq!(response.status(), StatusCode::OK, "creating a v{room_version} room must succeed");
    let bytes = axum::body::to_bytes(response.into_body(), 4096).await.unwrap();
    let json: Value = serde_json::from_slice(&bytes).unwrap();
    json["room_id"].as_str().expect("createRoom must return a room_id").to_string()
}

/// Assemble and sign one `m.room.topic` PDU through the production pipeline:
/// `build_pdu` → `finalize_local_pdu` → `sign_and_hash_event`.
///
/// Returns the emitted PDU and the finalized event ID. `sign_and_hash_event`
/// signs the same bytes the receiver's `verify_pdu_sender_signature` recomputes
/// (`signature_material_bytes`), which is what makes the round-trip assertable.
fn build_signed_topic_pdu(
    room_version: &str,
    room_id: &str,
    sender: &str,
    create_event_id: &str,
    key_id: &str,
    signing_key_b64: &str,
) -> (Value, String) {
    build_signed_state_pdu(
        room_version,
        room_id,
        sender,
        &[create_event_id.to_string()],
        &[create_event_id.to_string()],
        key_id,
        signing_key_b64,
    )
}

/// Assemble and sign one `m.room.topic` state PDU with explicit DAG references.
fn build_signed_state_pdu(
    room_version: &str,
    room_id: &str,
    sender: &str,
    prev_events: &[String],
    auth_events: &[String],
    key_id: &str,
    signing_key_b64: &str,
) -> (Value, String) {
    let content = json!({ "topic": "U-8 federation signature round-trip" });
    let parts = PduParts {
        room_version,
        // v3+ has no `event_id` field; identity is the reference hash.
        event_id: None,
        room_id,
        sender,
        event_type: "m.room.topic",
        content: &content,
        state_key: Some(""),
        origin_server_ts: 1_750_000_000_000,
        origin: "localhost",
        depth: 2,
        prev_events,
        auth_events,
        redacts: None,
    };

    let mut pdu = build_pdu(&parts);
    let finalized = finalize_local_pdu(&parts).expect("the assembled PDU must be finalizable");
    pdu.as_object_mut().expect("a PDU is a JSON object").insert("hashes".to_string(), finalized.hashes.clone());
    sign_and_hash_event(room_version, "localhost", key_id, signing_key_b64, &mut pdu)
        .expect("sign_and_hash_event must sign our own PDU");

    // Item 2 of the task: the emitted PDU must be a complete, v3+-shaped PDU.
    assert!(pdu.get("event_id").is_none(), "v3+ PDUs must not carry event_id: {pdu}");
    for field in ["depth", "prev_events", "auth_events", "hashes", "signatures"] {
        assert!(pdu.get(field).is_some(), "the emitted PDU must carry `{field}`: {pdu}");
    }
    assert_eq!(pdu["hashes"], finalized.hashes, "signer and finalizer must agree on the content hash");
    assert_eq!(
        resolve_received_event_id(room_version, &pdu).expect("the receiver must derive the ID"),
        finalized.event_id,
        "the ID the receiver derives must equal the ID the sender finalized"
    );

    (pdu, finalized.event_id)
}

/// Fresh v12-baseline schema + real v11 room + production-built signed PDU.
///
/// The [`IsolatedTestPool`] handle is returned so the schema outlives the test
/// body (R9: new DB coverage clones the shared real baseline).
async fn signed_pdu_fixture(
    key_id: &str,
    seed: [u8; 32],
    prefix: &str,
) -> Option<(IsolatedTestPool, SignedPduFixture)> {
    let isolated = match IsolatedTestPool::new(BASELINE_SQL).await {
        Ok(isolated) => isolated,
        Err(error) => {
            eprintln!(
                "Skipping federation signature round-trip test because the test database is unavailable: {error}"
            );
            return None;
        }
    };
    let pool = isolated.pool();
    let signing_key_b64 = STANDARD_NO_PAD.encode(seed);
    let signing_key = ed25519_dalek::SigningKey::from_bytes(&seed);
    let app = build_federation_txn_app(pool.clone(), key_id, &signing_key_b64).await;

    let (token, creator) = register_user_via_client(&app, prefix).await;
    let room_id = create_room_with_version(&app, &token, SIG_ASSERT_ROOM_VERSION).await;
    let create_event_id: String = sqlx::query_scalar(
        "SELECT event_id FROM events WHERE room_id = $1 AND event_type = 'm.room.create' \
         ORDER BY origin_server_ts ASC LIMIT 1",
    )
    .bind(&room_id)
    .fetch_one(&*pool)
    .await
    .expect("the new v11 room must have an m.room.create event");

    let (pdu, derived_event_id) =
        build_signed_topic_pdu(SIG_ASSERT_ROOM_VERSION, &room_id, &creator, &create_event_id, key_id, &signing_key_b64);
    let fixture = SignedPduFixture {
        app,
        pool,
        key_id: key_id.to_string(),
        signing_key,
        signing_key_b64,
        room_id,
        pdu,
        derived_event_id,
    };
    Some((isolated, fixture))
}

#[tokio::test]
async fn test_send_transaction_with_signed_pdu_accepted() {
    let key_id = "ed25519:u8_accept";
    let Some((_isolated, fixture)) = signed_pdu_fixture(key_id, [96u8; 32], "u8_accept").await else {
        return;
    };

    // A reference-hash ID carries no origin suffix — this is what distinguishes
    // it from the previous `$…:localhost` fabricated IDs.
    assert!(
        !fixture.derived_event_id.contains(':'),
        "a v3+ event ID is a bare reference hash: {}",
        fixture.derived_event_id
    );

    let response = fixture.send("u8_accept_1", &fixture.pdu).await;
    let first = SignedPduFixture::single_result(&response);

    // Do NOT accept either/or: the PDU must be accepted, and `error` must be absent.
    assert_eq!(first["success"], json!(true), "the signed PDU must be accepted: {first}");
    assert!(first.get("error").is_none(), "an accepted PDU must not carry an error: {first}");
    assert_eq!(first["event_id"], json!(fixture.derived_event_id), "the result must name the derived ID: {first}");

    // Read the persisted row back through the storage layer.
    let storage = EventStorage::new(&fixture.pool, "localhost".to_string());
    let stored = storage
        .get_state_event(&fixture.room_id, "m.room.topic", "")
        .await
        .expect("the events table must be readable")
        .expect("the accepted PDU must be persisted");
    assert_eq!(
        stored.event_id, fixture.derived_event_id,
        "the event must be persisted under the derived reference-hash ID, not a fabricated one"
    );

    let hashes = stored.hashes.as_ref().expect("the inbound PDU's content hash must be persisted");
    assert!(
        hashes.as_object().is_some_and(|map| !map.is_empty()),
        "stored hashes must be a non-empty object: {hashes}"
    );
    let expected_hash = compute_event_content_hash(&fixture.pdu).expect("the sent PDU must hash");
    assert_eq!(
        hashes["sha256"].as_str(),
        Some(expected_hash.as_str()),
        "the stored hashes.sha256 must equal the content hash of the PDU we sent"
    );

    let signatures = stored.signatures.as_ref().expect("the inbound PDU's signatures must be persisted");
    let server_signatures = signatures["localhost"]
        .as_object()
        .unwrap_or_else(|| panic!("stored signatures must contain the sender server: {signatures}"));
    assert!(!server_signatures.is_empty(), "stored signatures for the sender server must be non-empty: {signatures}");
    let stored_signature = server_signatures[key_id].as_str().expect("the sender's ed25519 signature must be stored");
    assert!(!stored_signature.is_empty(), "the stored ed25519 signature must not be empty");
    assert_eq!(
        Some(stored_signature),
        fixture.pdu["signatures"]["localhost"][key_id].as_str(),
        "the persisted signature must be the origin's, verbatim"
    );
}

/// Tampering a field that redaction **retains** must be rejected by the sender
/// signature check.
///
/// The content hash is deliberately recomputed after the tamper so
/// `verify_event_content_hash` passes: this makes the sender-signature
/// verification the only remaining check, i.e. it pins the half U-13 fixed
/// (before it, the signed bytes were the raw un-redacted PDU and this test's
/// premise did not hold).
#[tokio::test]
async fn test_send_transaction_rejects_pdu_with_tampered_retained_field() {
    let key_id = "ed25519:u8_tamper";
    let Some((_isolated, fixture)) = signed_pdu_fixture(key_id, [95u8; 32], "u8_tamper").await else {
        return;
    };

    let mut tampered = fixture.pdu.clone();
    let original_ts = tampered["origin_server_ts"].as_i64().expect("the PDU carries origin_server_ts");
    tampered["origin_server_ts"] = json!(original_ts + 1_000);
    // Redaction retains `origin_server_ts`, so the *signature* covers it; the
    // content hash does not (`hashes` is stripped before hashing), so refresh it
    // to guarantee the rejection comes from signature verification.
    let recomputed = compute_event_content_hash(&tampered).expect("the tampered PDU must still hash");
    tampered["hashes"]["sha256"] = json!(recomputed);

    let response = fixture.send("u8_tamper_1", &tampered).await;
    let first = SignedPduFixture::single_result(&response);

    assert!(first.get("success").is_none(), "a tampered PDU must not report success: {first}");
    let error = first["error"].as_str().unwrap_or_else(|| panic!("a tampered PDU must report an error: {first}"));
    assert!(
        error.starts_with("Invalid PDU signature"),
        "the retained-field tamper must be caught by the sender-signature check, got: {error}"
    );

    let storage = EventStorage::new(&fixture.pool, "localhost".to_string());
    assert!(
        storage
            .get_state_event(&fixture.room_id, "m.room.topic", "")
            .await
            .expect("the events table must be readable")
            .is_none(),
        "a rejected PDU must not be persisted"
    );
}

/// A v3+ PDU that carries an explicit `event_id` must be rejected outright:
/// the receiver derives the identity, it never trusts a sender-supplied one.
#[tokio::test]
async fn test_send_transaction_rejects_v3_pdu_carrying_explicit_event_id() {
    let key_id = "ed25519:u8_explicit";
    let Some((_isolated, fixture)) = signed_pdu_fixture(key_id, [94u8; 32], "u8_explicit").await else {
        return;
    };

    let explicit_event_id = "$u8_explicit_event:localhost";
    let mut pdu = fixture.pdu.clone();
    pdu["event_id"] = json!(explicit_event_id);
    // The explicit `event_id` is the *only* defect this PDU may carry, or some
    // other gate would reject it and mask whether the v3+ format rule fired:
    // adding a field changes the content hash, and `hashes` is itself covered by
    // the signature.  Re-hash and re-sign so the hash and sender-signature checks
    // both pass.  That is also the realistic shape of the attack: v3+ signature
    // material strips `event_id`, so the carried identity is unauthenticated and
    // a sender can emit a fully signed PDU that still violates the format.
    sign_and_hash_event(SIG_ASSERT_ROOM_VERSION, "localhost", key_id, &fixture.signing_key_b64, &mut pdu)
        .expect("the malformed PDU must still be signable");

    let response = fixture.send("u8_explicit_1", &pdu).await;
    let first = SignedPduFixture::single_result(&response);

    assert!(first.get("success").is_none(), "a PDU with an explicit event_id must not succeed: {first}");
    let error = first["error"].as_str().unwrap_or_else(|| panic!("the PDU must report an error: {first}"));
    assert!(
        error.contains("must not carry an explicit event_id"),
        "resolve_received_event_id must enforce the v3+ format rule, got: {error}"
    );
    assert_eq!(first["event_id"], json!(explicit_event_id), "the rejection must echo the carried ID: {first}");

    let storage = EventStorage::new(&fixture.pool, "localhost".to_string());
    assert!(
        storage.get_event(explicit_event_id).await.expect("the events table must be readable").is_none(),
        "the fabricated event_id must not be persisted"
    );
    assert!(
        storage
            .get_state_event(&fixture.room_id, "m.room.topic", "")
            .await
            .expect("the events table must be readable")
            .is_none(),
        "the rejected PDU must not be persisted"
    );
}

// ============================================================================
// Test 6: one `m.presence` EDU carrying several updates persists all of them (D-32)
// ============================================================================
//
// The handler used to call `PresenceService::set_presence` once per entry — N upserts plus
// N federation broadcasts for a single EDU. It now collects the updates that pass
// validation and writes them with the batched `set_presence_batch` (one `UNNEST` statement).
// This test drives the real HTTP route (signed `PUT /send`) so the wiring itself is covered,
// not just the storage-level batch method.
#[tokio::test]
async fn test_send_transaction_persists_every_presence_update_in_one_edu() {
    let key_id = "ed25519:presence_batch";
    let signing_key = ed25519_dalek::SigningKey::from_bytes(&[77u8; 32]);
    let signing_key_b64 = STANDARD_NO_PAD.encode(signing_key.to_bytes());
    let Some((app, pool)) = setup_federation_txn_test_app(key_id, &signing_key_b64).await else {
        return;
    };

    let uuid = uuid::Uuid::new_v4();
    let suffix = uuid.as_simple();
    let user_a = format!("@pres_a_{suffix}:localhost");
    let user_b = format!("@pres_b_{suffix}:localhost");
    // `validate_presence_update` only accepts user_ids belonging to the sending origin, and
    // the handler drops updates for users this server does not know.
    for user in [&user_a, &user_b] {
        super::ensure_test_user(&pool, user).await;
    }

    let body = json!({
        "origin": "localhost",
        "pdus": [],
        "edus": [{
            "edu_type": "m.presence",
            "content": {
                "push": [
                    { "user_id": user_a, "presence": "online", "status_msg": "at work" },
                    { "user_id": user_b, "presence": "away" }
                ]
            }
        }]
    });

    let request = signed_federation_request(
        "PUT",
        "/_matrix/federation/v1/send/presence_batch_1",
        "localhost",
        key_id,
        &signing_key,
        Some(&body),
    );

    let response = ServiceExt::<Request<Body>>::oneshot(app, request).await.unwrap();
    assert_eq!(response.status(), StatusCode::OK);

    let rows: Vec<(String, String, Option<String>)> =
        sqlx::query_as("SELECT user_id, presence, status_msg FROM presence WHERE user_id = ANY($1) ORDER BY user_id")
            .bind(vec![user_a.clone(), user_b.clone()])
            .fetch_all(&*pool)
            .await
            .expect("querying the presence table must succeed");

    assert_eq!(rows.len(), 2, "both updates of the single m.presence EDU must be persisted, got: {rows:?}");
    assert_eq!(rows[0], (user_a.clone(), "online".to_string(), Some("at work".to_string())));
    // `PresenceState` normalises the wire value: the EDU's "away" is stored as
    // "unavailable" (the canonical local spelling).
    assert_eq!(rows[1], (user_b.clone(), "unavailable".to_string(), None));

    for user in [&user_a, &user_b] {
        let _ = sqlx::query("DELETE FROM presence WHERE user_id = $1").bind(user).execute(&*pool).await;
    }
}

// ============================================================================
// Test 7: the EDU's updates are written as ONE statement (all-or-nothing) — D-32
// ============================================================================
//
// This is what makes the wiring observable rather than merely "the rows appear either way":
// with the old per-entry loop, a rejected update left the *earlier* users of the same EDU
// committed; with the batched `UNNEST` statement the whole EDU is one write, so a rejection
// rolls back every update in it. Reverting `handle_presence_edu` to the per-entry loop makes
// this test fail with `user_a` present (RED evidence for D-32).
#[tokio::test]
async fn test_presence_edu_updates_are_written_as_a_single_batch() {
    let key_id = "ed25519:presence_batch_atomic";
    let signing_key = ed25519_dalek::SigningKey::from_bytes(&[78u8; 32]);
    let signing_key_b64 = STANDARD_NO_PAD.encode(signing_key.to_bytes());
    let Some((app, pool)) = setup_federation_txn_test_app(key_id, &signing_key_b64).await else {
        return;
    };

    let uuid = uuid::Uuid::new_v4();
    let suffix = uuid.as_simple();
    let user_a = format!("@pres_atomic_a_{suffix}:localhost");
    let user_b = format!("@pres_atomic_b_{suffix}:localhost");
    for user in [&user_a, &user_b] {
        super::ensure_test_user(&pool, user).await;
    }

    // Failure injection: reject exactly one of the two rows, so the batch write fails.
    // A CHECK constraint (rather than dropping the table) is required because the clone's
    // `search_path` falls back to `public` for unqualified names.
    sqlx::query(&format!("ALTER TABLE presence ADD CONSTRAINT probe_reject_presence_b CHECK (user_id <> '{user_b}')"))
        .execute(&*pool)
        .await
        .expect("failure injection: the probe constraint must be added to the per-test table");

    let body = json!({
        "origin": "localhost",
        "pdus": [],
        "edus": [{
            "edu_type": "m.presence",
            "content": {
                "push": [
                    { "user_id": user_a, "presence": "online" },
                    { "user_id": user_b, "presence": "online" }
                ]
            }
        }]
    });

    let request = signed_federation_request(
        "PUT",
        "/_matrix/federation/v1/send/presence_batch_atomic_1",
        "localhost",
        key_id,
        &signing_key,
        Some(&body),
    );

    let response = ServiceExt::<Request<Body>>::oneshot(app, request).await.unwrap();
    assert_eq!(response.status(), StatusCode::OK);

    let rows: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM presence WHERE user_id = ANY($1)")
        .bind(vec![user_a.clone(), user_b.clone()])
        .fetch_one(&*pool)
        .await
        .expect("count must succeed");

    assert_eq!(
        rows, 0,
        "the EDU's updates are one batched statement, so the rejected entry must roll back the \
         whole EDU; seeing {rows} row(s) means the per-entry loop is back (user_a committed \
         before user_b was rejected)"
    );

    for user in [&user_a, &user_b] {
        let _ = sqlx::query("DELETE FROM presence WHERE user_id = $1").bind(user).execute(&*pool).await;
    }
}

// ============================================================================
// Inbound federation redaction: `events.redacted_by` must record the redaction
// EVENT's id, not the sending user.
// ============================================================================
//
// `events.redacted_by` is a self-referential FK to `events.event_id`
// (`fk_events_redacted_by`). The inbound redaction path already has the correct
// value in scope — the redaction PDU's own (derived) event id, which it logged
// as `redaction_event_id` right next to the buggy call — but passed the sender's
// user id into the FK instead, so the content redaction always failed with a
// foreign-key violation and the target stayed un-redacted while the sender got
// an HTTP 200 `success` back.

/// Seed one `m.room.message` row directly as the redaction target.
///
/// The isolated test pool caps connections at 2, so driving the full client
/// `send` pipeline here exhausts it; this test is about the *inbound* redaction
/// path, so seeding the row through the real storage writer is sufficient.
async fn seed_redaction_target(pool: &Arc<sqlx::PgPool>, room_id: &str, sender: &str) -> String {
    let storage = EventStorage::new(pool, "localhost".to_string());
    let event_id = format!("$redact_target_{}:localhost", uuid::Uuid::new_v4().as_simple());
    storage
        .create_event(
            synapse_storage::event::CreateEventParams {
                event_id: event_id.clone(),
                room_id: room_id.to_string(),
                user_id: sender.to_string(),
                event_type: "m.room.message".to_string(),
                content: json!({ "msgtype": "m.text", "body": "redact me" }),
                state_key: None,
                origin_server_ts: 1_750_000_000_050,
                redacts: None,
            },
            None,
        )
        .await
        .expect("the redaction target must be persisted");
    event_id
}

/// Assemble and sign one v11 `m.room.redaction` PDU through the production
/// pipeline. Room version 11 carries the target in `content.redacts`
/// (MSC2174/MSC3820), which is what `extract_redacts` reads.
fn build_signed_redaction_pdu(
    room_id: &str,
    sender: &str,
    target_event_id: &str,
    create_event_id: &str,
    key_id: &str,
    signing_key_b64: &str,
) -> (Value, String) {
    let content = json!({ "redacts": target_event_id, "reason": "inbound federation redaction" });
    let prev_events = vec![create_event_id.to_string()];
    let auth_events = vec![create_event_id.to_string()];
    let parts = PduParts {
        room_version: SIG_ASSERT_ROOM_VERSION,
        event_id: None,
        room_id,
        sender,
        event_type: "m.room.redaction",
        content: &content,
        state_key: None,
        origin_server_ts: 1_750_000_000_100,
        origin: "localhost",
        depth: 3,
        prev_events: &prev_events,
        auth_events: &auth_events,
        redacts: Some(target_event_id),
    };

    let mut pdu = build_pdu(&parts);
    let finalized = finalize_local_pdu(&parts).expect("the assembled redaction PDU must be finalizable");
    pdu.as_object_mut().expect("a PDU is a JSON object").insert("hashes".to_string(), finalized.hashes.clone());
    sign_and_hash_event(SIG_ASSERT_ROOM_VERSION, "localhost", key_id, signing_key_b64, &mut pdu)
        .expect("sign_and_hash_event must sign our own PDU");

    (pdu, finalized.event_id)
}

#[tokio::test]
async fn test_inbound_redaction_records_redaction_event_id_in_redacted_by() {
    let key_id = "ed25519:u8_redact_fk";
    let Some((_isolated, fixture)) = signed_pdu_fixture(key_id, [93u8; 32], "u8_redact_fk").await else {
        return;
    };

    // A real target event in the room (the redaction PDU must have something to
    // strip; `redact_event_content` is a no-op for an unknown event).
    let sender = fixture
        .pdu
        .get("sender")
        .and_then(Value::as_str)
        .expect("the fixture PDU carries the creator as sender")
        .to_string();
    let target = seed_redaction_target(&fixture.pool, &fixture.room_id, &sender).await;
    let create_event_id: String = sqlx::query_scalar(
        "SELECT event_id FROM events WHERE room_id = $1 AND event_type = 'm.room.create' \
         ORDER BY origin_server_ts ASC LIMIT 1",
    )
    .bind(&fixture.room_id)
    .fetch_one(&*fixture.pool)
    .await
    .expect("the room must have an m.room.create event");

    let (pdu, redaction_event_id) = build_signed_redaction_pdu(
        &fixture.room_id,
        // The PDU is signed by `localhost`, so the sender must be a local user.
        &sender,
        &target,
        &create_event_id,
        key_id,
        &fixture.signing_key_b64,
    );

    let response = fixture.send("u8_redact_fk_1", &pdu).await;
    let result = SignedPduFixture::single_result(&response);
    assert_eq!(result["success"], json!(true), "the signed redaction PDU must be accepted: {result}");
    assert_eq!(result["event_id"], json!(redaction_event_id), "the persisted redaction event id: {result}");

    let row = sqlx::query("SELECT is_redacted, redacted_by FROM events WHERE event_id = $1")
        .bind(&target)
        .fetch_one(&*fixture.pool)
        .await
        .expect("the target event must still exist");
    assert!(
        row.get::<bool, _>("is_redacted"),
        "the inbound redaction PDU must strip the target's content, not fail on the redacted_by FK"
    );
    assert_eq!(
        row.get::<Option<String>, _>("redacted_by").as_deref(),
        Some(redaction_event_id.as_str()),
        "redacted_by must be the redaction event's own id, not the sender's user id"
    );
}

// ============================================================================
// Test 8: MSC4307 / room version 12 rule 3.5 — `auth_events` must belong to
// the event's room
// ============================================================================
//
// Spec source: `matrix-spec` `content/rooms/v12.md` rule 3.5 (MSC4307):
// "If any event in `auth_events` has a `room_id` which does not match that of
// the event being authorised, reject."
//
// This is the first rule wired through the inbound authorisation seam
// (`synapse_federation::event_auth::check_inbound_event_auth`); the seam itself
// is what makes rules 1.2 / 2 / 2.5 / 3.1 / 3.2 / 10.4 addable without
// re-plumbing the transaction handler.

/// Room version 12 is the version that carries rule 3.5 (MSC4304 defines v12
/// as including MSC4307). v1–v11 do not have this rule, so the check is gated
/// on the version — see the v11 scope test below.
const MSC4307_ROOM_VERSION: &str = "12";

/// Two v12 rooms created by the same local user, plus a signed-PDU emitter.
struct CrossRoomAuthEventsFixture {
    app: axum::Router,
    pool: Arc<sqlx::PgPool>,
    key_id: String,
    signing_key_b64: String,
    creator: String,
    /// The room the event under test claims to be in.
    room_a: String,
    /// `m.room.create` of `room_a` — the well-formed auth event.
    room_a_create: String,
    /// `m.room.create` of another room — the mismatching auth event.
    room_b_create: String,
}

impl CrossRoomAuthEventsFixture {
    async fn setup(key_id: &str, seed: [u8; 32], prefix: &str) -> Option<(IsolatedTestPool, Self)> {
        let isolated = match IsolatedTestPool::new(BASELINE_SQL).await {
            Ok(isolated) => isolated,
            Err(error) => {
                eprintln!("Skipping MSC4307 test because the test database is unavailable: {error}");
                return None;
            }
        };
        let pool = isolated.pool();
        let signing_key_b64 = STANDARD_NO_PAD.encode(seed);
        let app = build_federation_txn_app(pool.clone(), key_id, &signing_key_b64).await;

        let (token, creator) = register_user_via_client(&app, prefix).await;
        let room_a = create_room_with_version(&app, &token, MSC4307_ROOM_VERSION).await;
        let room_b = create_room_with_version(&app, &token, MSC4307_ROOM_VERSION).await;
        let room_a_create = create_event_id_of(&pool, &room_a).await;
        let room_b_create = create_event_id_of(&pool, &room_b).await;

        // Guard the fixture itself: the two create events must genuinely live in
        // different rooms, or the "mismatch" probe below would assert nothing.
        assert_ne!(room_a, room_b, "the fixture must create two distinct rooms");

        Some((
            isolated,
            Self {
                app,
                pool,
                key_id: key_id.to_string(),
                signing_key_b64,
                creator,
                room_a,
                room_a_create,
                room_b_create,
            },
        ))
    }

    fn signing_key(&self) -> ed25519_dalek::SigningKey {
        let seed: [u8; 32] = STANDARD_NO_PAD
            .decode(&self.signing_key_b64)
            .expect("seed decodes")
            .as_slice()
            .try_into()
            .expect("32 bytes");
        ed25519_dalek::SigningKey::from_bytes(&seed)
    }

    /// PUT one `m.room.topic` PDU for `room_a` whose `auth_events` is exactly
    /// `auth_events`.
    async fn send_topic_with_auth_events(&self, txn_id: &str, auth_events: &[String]) -> Value {
        let (pdu, _derived) = build_signed_state_pdu(
            MSC4307_ROOM_VERSION,
            &self.room_a,
            &self.creator,
            std::slice::from_ref(&self.room_a_create),
            auth_events,
            &self.key_id,
            &self.signing_key_b64,
        );
        let body = json!({ "origin": "localhost", "pdus": [pdu] });
        let request = signed_federation_request(
            "PUT",
            &format!("/_matrix/federation/v1/send/{txn_id}"),
            "localhost",
            &self.key_id,
            &self.signing_key(),
            Some(&body),
        );
        let response = ServiceExt::<Request<Body>>::oneshot(self.app.clone(), request).await.unwrap();
        assert_eq!(response.status(), StatusCode::OK, "the verdict is in `results`, not the status");
        let bytes = axum::body::to_bytes(response.into_body(), 8192).await.unwrap();
        serde_json::from_slice(&bytes).expect("the response body must be JSON")
    }

    fn single_result(response: &Value) -> &Value {
        let results = response["results"].as_array().expect("results must be an array");
        assert_eq!(results.len(), 1, "one PDU in → one result out: {response}");
        &results[0]
    }

    async fn topic_persisted(&self) -> bool {
        EventStorage::new(&self.pool, "localhost".to_string())
            .get_state_event(&self.room_a, "m.room.topic", "")
            .await
            .expect("the events table must be readable")
            .is_some()
    }
}

/// The `m.room.create` event ID of `room_id`, read back from the real schema.
async fn create_event_id_of(pool: &sqlx::PgPool, room_id: &str) -> String {
    sqlx::query_scalar(
        "SELECT event_id FROM events WHERE room_id = $1 AND event_type = 'm.room.create' \
         ORDER BY origin_server_ts ASC LIMIT 1",
    )
    .bind(room_id)
    .fetch_one(pool)
    .await
    .expect("every created room must have an m.room.create event")
}

/// MSC4307 / rule 3.5: an inbound v12 event whose `auth_events` names an event
/// from a *different* room must be rejected and must not be persisted.
///
/// RED evidence: before the seam existed this PDU was accepted (the same
/// fixture with matching auth events is the positive control below).
#[tokio::test]
async fn test_send_transaction_rejects_auth_event_from_another_room_v12() {
    let key_id = "ed25519:msc4307_cross_room";
    let Some((_isolated, fixture)) = CrossRoomAuthEventsFixture::setup(key_id, [93u8; 32], "msc4307_cross").await
    else {
        return;
    };

    let response =
        fixture.send_topic_with_auth_events("msc4307_cross_1", std::slice::from_ref(&fixture.room_b_create)).await;
    let first = CrossRoomAuthEventsFixture::single_result(&response);

    assert!(first.get("success").is_none(), "a cross-room auth event must not be accepted: {first}");
    let error =
        first["error"].as_str().unwrap_or_else(|| panic!("the cross-room auth event must produce an error: {first}"));
    assert!(
        error.contains("auth_events entry"),
        "the rejection must name the offending auth_events entry, got: {error}"
    );
    assert!(
        error.contains(&fixture.room_b_create),
        "the rejection must name the foreign event {}, got: {error}",
        fixture.room_b_create
    );
    assert!(
        error.contains("MSC4307"),
        "the rejection must cite the rule it enforced (MSC4307 / v12 rule 3.5), got: {error}"
    );
    assert!(!fixture.topic_persisted().await, "a PDU rejected by rule 3.5 must not be persisted into the room");
}

/// Positive control: the *same* PDU shape with an `auth_events` entry from the
/// event's own room must still be accepted. Without this, the cross-room test
/// could pass by rejecting everything.
#[tokio::test]
async fn test_send_transaction_accepts_auth_event_from_same_room_v12() {
    let key_id = "ed25519:msc4307_same_room";
    let Some((_isolated, fixture)) = CrossRoomAuthEventsFixture::setup(key_id, [92u8; 32], "msc4307_same").await else {
        return;
    };

    let response =
        fixture.send_topic_with_auth_events("msc4307_same_1", std::slice::from_ref(&fixture.room_a_create)).await;
    let first = CrossRoomAuthEventsFixture::single_result(&response);

    assert_eq!(
        first["success"],
        json!(true),
        "the same-room auth event must be accepted (positive control for rule 3.5): {first}"
    );
    assert!(first.get("error").is_none(), "an accepted PDU must not carry an error: {first}");
    assert!(fixture.topic_persisted().await, "the accepted PDU must be persisted into the room");
}

/// An `auth_events` entry the server cannot resolve is rejected, never waved
/// through: the room of an unavailable event is unknown, so "does not match"
/// cannot be decided and the fail-closed answer is "reject".
#[tokio::test]
async fn test_send_transaction_rejects_unresolvable_auth_event_v12() {
    let key_id = "ed25519:msc4307_missing";
    let Some((_isolated, fixture)) = CrossRoomAuthEventsFixture::setup(key_id, [91u8; 32], "msc4307_missing_ev").await
    else {
        return;
    };

    let phantom = format!("$msc4307_phantom_{}:localhost", uuid::Uuid::new_v4().as_simple());
    let response = fixture.send_topic_with_auth_events("msc4307_missing_1", std::slice::from_ref(&phantom)).await;
    let first = CrossRoomAuthEventsFixture::single_result(&response);

    assert!(first.get("success").is_none(), "an unresolvable auth event must not be accepted: {first}");
    let error =
        first["error"].as_str().unwrap_or_else(|| panic!("an unresolvable auth event must produce an error: {first}"));
    assert!(error.contains(&phantom), "the rejection must name the unresolvable entry, got: {error}");
    assert!(!fixture.topic_persisted().await, "a fail-closed rejection must not persist the PDU");
}
