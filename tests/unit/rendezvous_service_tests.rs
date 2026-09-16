// Rendezvous service-layer tests
//
// The HTTP layer no longer touches `synapse_storage` directly: it calls
// `synapse_services::rendezvous_service::RendezvousService`, which owns the
// MSC4108 / legacy session contract and the storage-error mapping. These tests
// exercise that contract through the shared in-memory double
// `synapse_storage::test_mocks::rendezvous::InMemoryRendezvousStore` (the
// production `RendezvousStorage` needs a live Postgres pool).
//
// The double mirrors the real storage semantics where they are observable:
//   * ETag  = `"<millis>"`  (quoted timestamp)
//   * get   returns None when the session is missing OR expired (`expires_at > now`)
//   * update with a matching `if_match` succeeds; a mismatch / missing / expired
//     session returns None (which the service maps to 404 / 412 per MSC4108)
//   * delete is fire-and-forget (Ok(()) regardless of rows affected)
//
// This exercises real trait behavior (create → get → update → delete lifecycle,
// not-found, expiry, and conditional-update precondition checks) without a DB.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use synapse_common::current_timestamp_millis;
use synapse_storage::rendezvous::{
    CreateRendezvousSessionParams, Msc4108UpdateOutcome, RendezvousIntent, RendezvousMessage, RendezvousStoreApi,
    RendezvousTransport,
};
use synapse_storage::test_mocks::rendezvous::InMemoryRendezvousStore;

/// Mirrors the service's MSC4108 TTL (`RendezvousService`, 5 minutes): these
/// tests drive the store directly, so they pin the same lifetime the service
/// would pass in.
const MSC4108_TTL_MS: i64 = 5 * 60 * 1000;

// ── Domain model tests (the re-exported service surface) ───────────────────

#[test]
fn rendezvous_intent_variants_round_trip() {
    assert_eq!(RendezvousIntent::LoginReciprocate.as_str(), "login.reciprocate");
    assert_eq!(RendezvousIntent::LoginStart.as_str(), "login.start");
}

#[test]
fn rendezvous_transport_variants_round_trip() {
    assert_eq!(RendezvousTransport::HttpV1.as_str(), "http.v1");
    assert_eq!(RendezvousTransport::HttpV2.as_str(), "http.v2");
}

#[test]
fn create_params_carries_intent_and_transport() {
    let params = CreateRendezvousSessionParams {
        intent: RendezvousIntent::LoginStart,
        transport: RendezvousTransport::HttpV2,
        transport_data: Some(serde_json::json!({"uri": "https://example.com"})),
        expires_in_ms: Some(120_000),
    };
    assert_eq!(params.intent.as_str(), "login.start");
    assert_eq!(params.transport.as_str(), "http.v2");
    assert_eq!(params.expires_in_ms, Some(120_000));
    assert!(params.transport_data.is_some());
}

#[test]
fn rendezvous_message_serializes_type_field() {
    let message = RendezvousMessage {
        message_type: "m.login.start".to_string(),
        content: serde_json::json!({"homeserver": "https://matrix.example.com"}),
    };
    let json = serde_json::to_value(&message).unwrap();
    assert_eq!(json["type"], "m.login.start");
    assert_eq!(json["content"]["homeserver"], "https://matrix.example.com");
}

// ── MSC4108 storage contract (in-memory, via the trait) ───────────────────

#[tokio::test]
async fn create_session_returns_id_etag_and_expiry() {
    let store = InMemoryRendezvousStore::new();
    let (session_id, etag, _created_ts, expires_at) =
        store.create_msc4108_session("initial", MSC4108_TTL_MS).await.unwrap();

    assert!(!session_id.is_empty(), "session id must be non-empty");
    assert!(etag.starts_with('"') && etag.ends_with('"'), "etag must be quoted, got: {etag}");
    assert!(expires_at > current_timestamp_millis(), "expiry must be in the future");
}

#[tokio::test]
async fn get_after_create_returns_initial_data() {
    let store = InMemoryRendezvousStore::new();
    let (session_id, _etag, _created_ts, _expires) =
        store.create_msc4108_session("hello-world", MSC4108_TTL_MS).await.unwrap();

    let result = store.get_msc4108_data(&session_id).await.unwrap();
    let (data, etag, _updated_ts, _expires) = result.expect("session must exist right after creation");
    assert_eq!(data, "hello-world");
    assert!(etag.starts_with('"') && etag.ends_with('"'));
}

#[tokio::test]
async fn get_unknown_session_returns_none() {
    let store = InMemoryRendezvousStore::new();
    let result = store.get_msc4108_data("does-not-exist").await.unwrap();
    assert!(result.is_none(), "missing session must resolve to None");
}

#[tokio::test]
async fn expired_session_returns_none_on_get() {
    let store = InMemoryRendezvousStore::new();
    // ttl=0 ⇒ expires_at == now ⇒ `expires_at > now` is false ⇒ treated as expired.
    let (session_id, _etag, _created_ts, _expires) = store.create_msc4108_session("temp", 0).await.unwrap();

    let result = store.get_msc4108_data(&session_id).await.unwrap();
    assert!(result.is_none(), "expired session must resolve to None");
}

#[tokio::test]
async fn update_returns_new_etag_and_advances_data() {
    let store = InMemoryRendezvousStore::new();
    let (session_id, _etag, _created_ts, _expires) = store.create_msc4108_session("v1", MSC4108_TTL_MS).await.unwrap();

    let outcome = store.update_msc4108_data(&session_id, "v2", None).await.unwrap();
    let new_etag = match outcome {
        Msc4108UpdateOutcome::Updated { new_etag, .. } => new_etag,
        other => panic!("expected Updated, got {:?}", other),
    };
    assert!(new_etag.starts_with('"') && new_etag.ends_with('"'));

    let (data, etag, _updated_ts, _expires) =
        store.get_msc4108_data(&session_id).await.unwrap().expect("session still present");
    assert_eq!(data, "v2");
    assert_eq!(etag, new_etag, "get must reflect the etag returned by update");
}

#[tokio::test]
async fn update_unknown_session_returns_not_found() {
    let store = InMemoryRendezvousStore::new();
    let result = store.update_msc4108_data("ghost", "data", None).await.unwrap();
    assert!(matches!(result, Msc4108UpdateOutcome::NotFound), "updating a missing session must return NotFound");
}

#[tokio::test]
async fn update_with_matching_if_match_succeeds() {
    let store = InMemoryRendezvousStore::new();
    let (session_id, etag, _created_ts, _expires) = store.create_msc4108_session("v1", MSC4108_TTL_MS).await.unwrap();

    let outcome = store.update_msc4108_data(&session_id, "v2", Some(&etag)).await.unwrap();
    assert!(matches!(outcome, Msc4108UpdateOutcome::Updated { .. }), "update with the current etag must succeed");
}

#[tokio::test]
async fn update_with_stale_if_match_returns_precondition_failed() {
    let store = InMemoryRendezvousStore::new();
    let (session_id, etag_v1, _created_ts, _expires) =
        store.create_msc4108_session("v1", MSC4108_TTL_MS).await.unwrap();

    // First update bumps the etag to v2.
    let outcome_v2 = store.update_msc4108_data(&session_id, "v2", None).await.unwrap();
    let etag_v2 = match outcome_v2 {
        Msc4108UpdateOutcome::Updated { new_etag, .. } => new_etag,
        _ => panic!(),
    };

    // Now retry with the stale v1 etag — must fail with PreconditionFailed.
    let stale_result = store.update_msc4108_data(&session_id, "v3", Some(&etag_v1)).await.unwrap();
    assert!(
        matches!(stale_result, Msc4108UpdateOutcome::PreconditionFailed { .. }),
        "update with a stale etag must return PreconditionFailed"
    );

    // The current v2 etag still works.
    let ok_result = store.update_msc4108_data(&session_id, "v3", Some(&etag_v2)).await.unwrap();
    assert!(matches!(ok_result, Msc4108UpdateOutcome::Updated { .. }), "update with the current etag must succeed");
}

#[tokio::test]
async fn update_with_if_match_on_missing_session_returns_not_found() {
    let store = InMemoryRendezvousStore::new();
    let result = store.update_msc4108_data("missing", "data", Some("\"1700000000000\"")).await.unwrap();
    assert!(
        matches!(result, Msc4108UpdateOutcome::NotFound),
        "conditional update on a missing session must return NotFound"
    );
}

#[tokio::test]
async fn delete_removes_session() {
    let store = InMemoryRendezvousStore::new();
    let (session_id, _etag, _created_ts, _expires) =
        store.create_msc4108_session("doomed", MSC4108_TTL_MS).await.unwrap();

    let removed = store.delete_msc4108_session(&session_id).await.unwrap();
    assert!(removed, "delete must report whether a row existed");

    let result = store.get_msc4108_data(&session_id).await.unwrap();
    assert!(result.is_none(), "get after delete must return None");
}

#[tokio::test]
async fn delete_unknown_session_returns_false() {
    let store = InMemoryRendezvousStore::new();
    // delete_msc4108_session returns false when no row existed (route turns this into 404).
    let removed = store.delete_msc4108_session("never-existed").await.unwrap();
    assert!(!removed, "delete of unknown session must return false");
}

#[tokio::test]
async fn full_lifecycle_create_get_update_get_delete() {
    let store = InMemoryRendezvousStore::new();

    // create
    let (session_id, etag_v1, _created_ts, expires) =
        store.create_msc4108_session("payload-1", MSC4108_TTL_MS).await.unwrap();
    assert!(expires > current_timestamp_millis());

    // get → initial payload + etag
    let (data, etag, _updated_ts, _expires) = store.get_msc4108_data(&session_id).await.unwrap().unwrap();
    assert_eq!(data, "payload-1");
    assert_eq!(etag, etag_v1);

    // conditional update with the current etag
    let etag_v2 = match store.update_msc4108_data(&session_id, "payload-2", Some(&etag)).await.unwrap() {
        Msc4108UpdateOutcome::Updated { new_etag, .. } => new_etag,
        other => panic!("expected Updated, got {:?}", other),
    };
    assert_ne!(etag_v2, etag_v1, "a successful update must produce a new etag");

    // get reflects the update
    let (data, etag, _updated_ts, _expires) = store.get_msc4108_data(&session_id).await.unwrap().unwrap();
    assert_eq!(data, "payload-2");
    assert_eq!(etag, etag_v2);

    // delete
    store.delete_msc4108_session(&session_id).await.unwrap();
    assert!(store.get_msc4108_data(&session_id).await.unwrap().is_none());
}

#[tokio::test]
async fn multiple_sessions_are_independent() {
    let store = InMemoryRendezvousStore::new();
    let (id_a, _, _, _) = store.create_msc4108_session("a", MSC4108_TTL_MS).await.unwrap();
    let (id_b, _, _, _) = store.create_msc4108_session("b", MSC4108_TTL_MS).await.unwrap();

    assert_ne!(id_a, id_b, "each session gets a unique id");

    let (data_a, _etag_a, _, _) = store.get_msc4108_data(&id_a).await.unwrap().unwrap();
    let (data_b, _etag_b, _, _) = store.get_msc4108_data(&id_b).await.unwrap().unwrap();
    assert_eq!(data_a, "a");
    assert_eq!(data_b, "b");

    // Deleting one does not affect the other.
    store.delete_msc4108_session(&id_a).await.unwrap();
    assert!(store.get_msc4108_data(&id_a).await.unwrap().is_none());
    assert!(store.get_msc4108_data(&id_b).await.unwrap().is_some());
}

#[tokio::test]
async fn unconditional_update_overwrites_stale_etag_without_check() {
    let store = InMemoryRendezvousStore::new();
    let (session_id, etag_v1, _created_ts, _expires) =
        store.create_msc4108_session("v1", MSC4108_TTL_MS).await.unwrap();

    // Unconditional update (if_match=None) succeeds even though we never read v1's etag.
    let new_etag = match store.update_msc4108_data(&session_id, "v2", None).await.unwrap() {
        Msc4108UpdateOutcome::Updated { new_etag, .. } => new_etag,
        other => panic!("expected Updated, got {:?}", other),
    };
    assert_ne!(new_etag, etag_v1);
}

#[tokio::test]
async fn etag_format_is_quoted_timestamp() {
    let store = InMemoryRendezvousStore::new();
    let (_session_id, etag, _created_ts, _expires) = store.create_msc4108_session("x", MSC4108_TTL_MS).await.unwrap();

    let raw = etag.trim_matches('"');
    assert!(raw.parse::<i64>().is_ok(), "etag payload must be a numeric timestamp, got: {raw}");
}
