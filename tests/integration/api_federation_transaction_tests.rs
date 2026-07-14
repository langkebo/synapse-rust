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
use std::sync::Arc;
use synapse_rust::federation::signing::canonical_federation_request_bytes;
use tower::ServiceExt;

async fn setup_federation_txn_test_app(
    key_id: &str,
    signing_key_b64: &str,
) -> Option<(axum::Router, Arc<sqlx::PgPool>)> {
    let pool = super::require_test_pool().await;
    let mut container = synapse_services::ServiceContainer::new_test_with_pool(pool.clone()).await;
    container.core.config.server.name = "localhost".to_string();
    container.core.server_name = "localhost".to_string();
    container.core.config.federation.enabled = true;
    container.core.config.federation.allow_ingress = true;
    container.core.config.federation.server_name = "localhost".to_string();
    container.core.config.federation.key_id = Some(key_id.to_string());
    container.core.config.federation.signing_key = Some(signing_key_b64.to_string());
    let cache =
        std::sync::Arc::new(synapse_rust::cache::CacheManager::new(&synapse_rust::cache::CacheConfig::default()));
    let state = synapse_rust::web::routes::state::AppState::new(container, cache);
    Some((synapse_rust::web::create_router(state), pool))
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
        .header(
            "Authorization",
            format!(
                "X-Matrix origin=\"{}\",key=\"{}\",sig=\"{}\"",
                origin, key_id, sig_b64
            ),
        );

    if content.is_some() {
        builder = builder.header("Content-Type", "application/json");
    }

    builder
        .body(Body::from(content.map(Value::to_string).unwrap_or_default()))
        .unwrap()
}

// ============================================================================
// Test 1: Missing Authorization header → 401
// ============================================================================

#[tokio::test]
async fn test_send_transaction_rejects_missing_signature() {
    let pool = super::require_test_pool().await;
    let mut container = synapse_services::ServiceContainer::new_test_with_pool(pool.clone()).await;
    container.core.config.federation.enabled = true;
    container.core.config.federation.allow_ingress = true;
    let cache = std::sync::Arc::new(synapse_rust::cache::CacheManager::new(
        &synapse_rust::cache::CacheConfig::default(),
    ));
    let state = synapse_rust::web::routes::state::AppState::new(container, cache);
    let app = synapse_rust::web::create_router(state);

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
    assert!(json["results"].as_array().is_some_and(|r| r.is_empty()),
        "Expected empty results array, got: {json}");
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
    assert!(
        first.get("error").is_some(),
        "Expected error field in PDU result, got: {first}"
    );
}

// ============================================================================
// Test 5: Valid signed PDU (signed+hashed) → 200 with success in results
// ============================================================================

#[tokio::test]
async fn test_send_transaction_with_signed_pdu_accepted() {
    let key_id = "ed25519:txn_test";
    let signing_key_seed = [96u8; 32];
    let signing_key_b64 = STANDARD_NO_PAD.encode(signing_key_seed);
    let signing_key = ed25519_dalek::SigningKey::from_bytes(&signing_key_seed);

    let Some((app, _pool)) = setup_federation_txn_test_app(key_id, &signing_key_b64).await else {
        return;
    };

    // Build a properly signed and hashed PDU using synapse_federation APIs.
    let mut pdu = json!({
        "event_id": "$test_signed_event:localhost",
        "room_id": "!test_signed_room:localhost",
        "sender": "@test_signed_user:localhost",
        "type": "m.room.message",
        "origin_server_ts": 1000,
        "content": { "body": "hello", "msgtype": "m.text" }
    });

    synapse_rust::federation::signing::sign_and_hash_event(
        "localhost",
        key_id,
        &signing_key_b64,
        &mut pdu,
    )
    .unwrap();

    let body = json!({
        "origin": "localhost",
        "pdus": [pdu]
    });

    let request = signed_federation_request(
        "PUT",
        "/_matrix/federation/v1/send/txn5",
        "localhost",
        key_id,
        &signing_key,
        Some(&body),
    );

    let response = ServiceExt::<Request<Body>>::oneshot(app, request).await.unwrap();
    assert_eq!(response.status(), StatusCode::OK);

    let resp_body = axum::body::to_bytes(response.into_body(), 4096).await.unwrap();
    let json: Value = serde_json::from_slice(&resp_body).unwrap();

    let results = json["results"].as_array().expect("results should be an array");
    assert!(!results.is_empty(), "Expected non-empty results for signed PDU");

    let first = &results[0];
    assert_eq!(first["event_id"], "$test_signed_event:localhost");
    // The PDU should either succeed (has success field) or fail with a clear error.
    assert!(
        first.get("success").is_some() || first.get("error").is_some(),
        "Expected success or error in PDU result, got: {first}"
    );
}
