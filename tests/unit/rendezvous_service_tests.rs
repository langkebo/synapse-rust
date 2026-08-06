// Rendezvous service-layer tests
//
// The rendezvous feature's "service" surface today lives in
// `synapse-storage::rendezvous` (the `RendezvousStoreApi` trait + the domain
// DTOs that `synapse-services/src/rendezvous_service.rs` re-exports). Because
// the production `RendezvousStorage` requires a live Postgres pool, these
// tests validate the MSC4108 storage contract through an in-memory
// `MockRendezvousStore` that implements the same `RendezvousStoreApi` trait
// the route layer depends on (`ctx.rendezvous_storage`).
//
// The mock mirrors the real storage semantics line-for-line:
//   * ETag  = `"<millis>"`  (quoted timestamp)
//   * get   returns None when the session is missing OR expired (`expires_at > now`)
//   * update with a matching `if_match` succeeds; a mismatch / missing / expired
//     session returns None (which the route maps to 400 Bad Request)
//   * delete is fire-and-forget (Ok(()) regardless of rows affected)
//
// This exercises real trait behavior (create → get → update → delete lifecycle,
// not-found, expiry, and conditional-update precondition checks) without a DB.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use async_trait::async_trait;
use synapse_common::current_timestamp_millis;
use synapse_storage::rendezvous::{
    CreateRendezvousSessionParams, RendezvousIntent, RendezvousMessage, RendezvousTransport,
    RendezvousStoreApi,
};
use tokio::sync::Mutex;

/// Mirrors `MSC4108_TTL_MS` in `src/web/routes/msc4108_rendezvous.rs` (5 minutes).
const MSC4108_TTL_MS: i64 = 5 * 60 * 1000;

// ── In-memory MockRendezvousStore ──────────────────────────────────────────

/// One MSC4108 session row stored in memory.
#[derive(Debug, Clone)]
struct Msc4108Row {
    data: String,
    /// ETag stored WITHOUT the surrounding quotes (the raw timestamp string),
    /// matching how the real storage compares `updated_ts::TEXT`.
    etag_raw: String,
    expires_at: i64,
}

/// In-memory `RendezvousStoreApi` implementation.
///
/// Only the MSC4108 methods carry real logic; the legacy rendezvous methods
/// (create_session / get_session / store_message / ...) return empty/Ok defaults
/// because the MSC4108 route module never invokes them.
///
/// A monotonic internal clock (`clock`) advances by 1 ms on every "now" read,
/// so each create/update produces a strictly-increasing ETag — this mirrors
/// the real storage's `updated_ts` semantics without wall-clock granularity
/// races (the real storage also uses millis, and two writes in the same ms
/// would collide).
struct MockRendezvousStore {
    msc4108: Mutex<std::collections::HashMap<String, Msc4108Row>>,
    clock: Mutex<i64>,
}

impl MockRendezvousStore {
    fn new() -> Self {
        Self { msc4108: Mutex::new(std::collections::HashMap::new()), clock: Mutex::new(current_timestamp_millis()) }
    }

    /// Return the next mock timestamp (strictly increasing across calls).
    async fn now(&self) -> i64 {
        let mut c = self.clock.lock().await;
        *c += 1;
        *c
    }
}

#[async_trait]
impl RendezvousStoreApi for MockRendezvousStore {
    async fn create_session(
        &self,
        _params: CreateRendezvousSessionParams,
    ) -> Result<synapse_storage::rendezvous::RendezvousSession, sqlx::Error> {
        // Not used by MSC4108; return a minimal placeholder.
        Ok(synapse_storage::rendezvous::RendezvousSession {
            id: 0,
            session_id: String::new(),
            user_id: None,
            device_id: None,
            intent: None,
            transport: None,
            transport_data: None,
            key: None,
            created_ts: 0,
            expires_at: 0,
            status: None,
        })
    }

    async fn get_session(
        &self,
        _session_id: &str,
    ) -> Result<Option<synapse_storage::rendezvous::RendezvousSession>, sqlx::Error> {
        Ok(None)
    }

    async fn update_session_status(&self, _session_id: &str, _status: &str) -> Result<(), sqlx::Error> {
        Ok(())
    }

    async fn bind_user_to_session(
        &self,
        _session_id: &str,
        _user_id: &str,
        _device_id: &str,
    ) -> Result<(), sqlx::Error> {
        Ok(())
    }

    async fn complete_session(&self, _session_id: &str) -> Result<(), sqlx::Error> {
        Ok(())
    }

    async fn delete_session(&self, _session_id: &str) -> Result<(), sqlx::Error> {
        Ok(())
    }

    async fn cleanup_expired_sessions(&self) -> Result<u64, sqlx::Error> {
        Ok(0)
    }

    async fn store_message(
        &self,
        _session_id: &str,
        _direction: &str,
        _message: &RendezvousMessage,
    ) -> Result<(), sqlx::Error> {
        Ok(())
    }

    async fn get_messages(
        &self,
        _session_id: &str,
        _after_id: Option<i64>,
    ) -> Result<Vec<synapse_storage::rendezvous::StoredRendezvousMessage>, sqlx::Error> {
        Ok(Vec::new())
    }

    // ── MSC4108 methods (real in-memory logic) ──

    async fn create_msc4108_session(
        &self,
        initial_data: &str,
        ttl_ms: i64,
    ) -> Result<(String, String, i64), sqlx::Error> {
        let now = self.now().await;
        let session_id = uuid::Uuid::new_v4().simple().to_string();
        let expires_at = now + ttl_ms;
        let etag_raw = now.to_string();

        self.msc4108.lock().await.insert(
            session_id.clone(),
            Msc4108Row { data: initial_data.to_string(), etag_raw: etag_raw.clone(), expires_at },
        );

        Ok((session_id, format!("\"{etag_raw}\""), expires_at))
    }

    async fn get_msc4108_data(&self, session_id: &str) -> Result<Option<(String, String)>, sqlx::Error> {
        let now = self.now().await;
        let guard = self.msc4108.lock().await;
        match guard.get(session_id) {
            Some(row) if row.expires_at > now => {
                Ok(Some((row.data.clone(), format!("\"{}\"", row.etag_raw))))
            }
            _ => Ok(None),
        }
    }

    async fn update_msc4108_data(
        &self,
        session_id: &str,
        data: &str,
        if_match: Option<&str>,
    ) -> Result<Option<String>, sqlx::Error> {
        let now = self.now().await;
        let mut guard = self.msc4108.lock().await;

        // Conditional update: verify the precondition first.
        if let Some(expected_etag) = if_match {
            let expected_raw = expected_etag.trim_matches('"').to_string();
            let exists = guard
                .get(session_id)
                .map(|row| row.etag_raw == expected_raw && row.expires_at > now)
                .unwrap_or(false);
            if !exists {
                return Ok(None); // ETag mismatch / not found / expired
            }
        }

        match guard.get_mut(session_id) {
            Some(row) if row.expires_at > now => {
                let new_etag_raw = now.to_string();
                row.data = data.to_string();
                row.etag_raw = new_etag_raw.clone();
                Ok(Some(format!("\"{new_etag_raw}\"")))
            }
            _ => Ok(None),
        }
    }

    async fn delete_msc4108_session(&self, session_id: &str) -> Result<(), sqlx::Error> {
        self.msc4108.lock().await.remove(session_id);
        Ok(())
    }
}

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
    let store = MockRendezvousStore::new();
    let (session_id, etag, expires_at) = store.create_msc4108_session("initial", MSC4108_TTL_MS).await.unwrap();

    assert!(!session_id.is_empty(), "session id must be non-empty");
    assert!(etag.starts_with('"') && etag.ends_with('"'), "etag must be quoted, got: {etag}");
    assert!(expires_at > current_timestamp_millis(), "expiry must be in the future");
}

#[tokio::test]
async fn get_after_create_returns_initial_data() {
    let store = MockRendezvousStore::new();
    let (session_id, _etag, _expires) = store.create_msc4108_session("hello-world", MSC4108_TTL_MS).await.unwrap();

    let result = store.get_msc4108_data(&session_id).await.unwrap();
    let (data, etag) = result.expect("session must exist right after creation");
    assert_eq!(data, "hello-world");
    assert!(etag.starts_with('"') && etag.ends_with('"'));
}

#[tokio::test]
async fn get_unknown_session_returns_none() {
    let store = MockRendezvousStore::new();
    let result = store.get_msc4108_data("does-not-exist").await.unwrap();
    assert!(result.is_none(), "missing session must resolve to None");
}

#[tokio::test]
async fn expired_session_returns_none_on_get() {
    let store = MockRendezvousStore::new();
    // ttl=0 ⇒ expires_at == now ⇒ `expires_at > now` is false ⇒ treated as expired.
    let (session_id, _etag, _expires) = store.create_msc4108_session("temp", 0).await.unwrap();

    let result = store.get_msc4108_data(&session_id).await.unwrap();
    assert!(result.is_none(), "expired session must resolve to None");
}

#[tokio::test]
async fn update_returns_new_etag_and_advances_data() {
    let store = MockRendezvousStore::new();
    let (session_id, _etag, _expires) = store.create_msc4108_session("v1", MSC4108_TTL_MS).await.unwrap();

    let new_etag = store.update_msc4108_data(&session_id, "v2", None).await.unwrap();
    let new_etag = new_etag.expect("unconditional update must succeed");
    assert!(new_etag.starts_with('"') && new_etag.ends_with('"'));

    let (data, etag) = store.get_msc4108_data(&session_id).await.unwrap().expect("session still present");
    assert_eq!(data, "v2");
    assert_eq!(etag, new_etag, "get must reflect the etag returned by update");
}

#[tokio::test]
async fn update_unknown_session_returns_none() {
    let store = MockRendezvousStore::new();
    let result = store.update_msc4108_data("ghost", "data", None).await.unwrap();
    assert!(result.is_none(), "updating a missing session must return None");
}

#[tokio::test]
async fn update_with_matching_if_match_succeeds() {
    let store = MockRendezvousStore::new();
    let (session_id, etag, _expires) = store.create_msc4108_session("v1", MSC4108_TTL_MS).await.unwrap();

    let updated = store.update_msc4108_data(&session_id, "v2", Some(&etag)).await.unwrap();
    assert!(updated.is_some(), "update with the current etag must succeed");
}

#[tokio::test]
async fn update_with_stale_if_match_returns_none() {
    let store = MockRendezvousStore::new();
    let (session_id, etag_v1, _expires) = store.create_msc4108_session("v1", MSC4108_TTL_MS).await.unwrap();

    // First update bumps the etag to v2.
    let etag_v2 = store.update_msc4108_data(&session_id, "v2", None).await.unwrap().unwrap();

    // Now retry with the stale v1 etag — must fail.
    let stale_result = store.update_msc4108_data(&session_id, "v3", Some(&etag_v1)).await.unwrap();
    assert!(stale_result.is_none(), "update with a stale etag must return None");

    // The current v2 etag still works.
    let ok_result = store.update_msc4108_data(&session_id, "v3", Some(&etag_v2)).await.unwrap();
    assert!(ok_result.is_some(), "update with the current etag must succeed");
}

#[tokio::test]
async fn update_with_if_match_on_missing_session_returns_none() {
    let store = MockRendezvousStore::new();
    let result = store.update_msc4108_data("missing", "data", Some("\"1700000000000\"")).await.unwrap();
    assert!(result.is_none(), "conditional update on a missing session must return None");
}

#[tokio::test]
async fn delete_removes_session() {
    let store = MockRendezvousStore::new();
    let (session_id, _etag, _expires) = store.create_msc4108_session("doomed", MSC4108_TTL_MS).await.unwrap();

    store.delete_msc4108_session(&session_id).await.unwrap();

    let result = store.get_msc4108_data(&session_id).await.unwrap();
    assert!(result.is_none(), "get after delete must return None");
}

#[tokio::test]
async fn delete_unknown_session_is_idempotent() {
    let store = MockRendezvousStore::new();
    // delete_msc4108_session is fire-and-forget — never errors on unknown ids.
    let result = store.delete_msc4108_session("never-existed").await;
    assert!(result.is_ok(), "delete of unknown session must succeed (fire-and-forget)");
}

#[tokio::test]
async fn full_lifecycle_create_get_update_get_delete() {
    let store = MockRendezvousStore::new();

    // create
    let (session_id, etag_v1, expires) = store.create_msc4108_session("payload-1", MSC4108_TTL_MS).await.unwrap();
    assert!(expires > current_timestamp_millis());

    // get → initial payload + etag
    let (data, etag) = store.get_msc4108_data(&session_id).await.unwrap().unwrap();
    assert_eq!(data, "payload-1");
    assert_eq!(etag, etag_v1);

    // conditional update with the current etag
    let etag_v2 = store.update_msc4108_data(&session_id, "payload-2", Some(&etag)).await.unwrap().unwrap();
    assert_ne!(etag_v2, etag_v1, "a successful update must produce a new etag");

    // get reflects the update
    let (data, etag) = store.get_msc4108_data(&session_id).await.unwrap().unwrap();
    assert_eq!(data, "payload-2");
    assert_eq!(etag, etag_v2);

    // delete
    store.delete_msc4108_session(&session_id).await.unwrap();
    assert!(store.get_msc4108_data(&session_id).await.unwrap().is_none());
}

#[tokio::test]
async fn multiple_sessions_are_independent() {
    let store = MockRendezvousStore::new();
    let (id_a, _, _) = store.create_msc4108_session("a", MSC4108_TTL_MS).await.unwrap();
    let (id_b, _, _) = store.create_msc4108_session("b", MSC4108_TTL_MS).await.unwrap();

    assert_ne!(id_a, id_b, "each session gets a unique id");

    let (data_a, _) = store.get_msc4108_data(&id_a).await.unwrap().unwrap();
    let (data_b, _) = store.get_msc4108_data(&id_b).await.unwrap().unwrap();
    assert_eq!(data_a, "a");
    assert_eq!(data_b, "b");

    // Deleting one does not affect the other.
    store.delete_msc4108_session(&id_a).await.unwrap();
    assert!(store.get_msc4108_data(&id_a).await.unwrap().is_none());
    assert!(store.get_msc4108_data(&id_b).await.unwrap().is_some());
}

#[tokio::test]
async fn unconditional_update_overwrites_stale_etag_without_check() {
    let store = MockRendezvousStore::new();
    let (session_id, etag_v1, _) = store.create_msc4108_session("v1", MSC4108_TTL_MS).await.unwrap();

    // Unconditional update (if_match=None) succeeds even though we never read v1's etag.
    let new_etag = store.update_msc4108_data(&session_id, "v2", None).await.unwrap().unwrap();
    assert_ne!(new_etag, etag_v1);
}

#[tokio::test]
async fn etag_format_is_quoted_timestamp() {
    let store = MockRendezvousStore::new();
    let (_session_id, etag, _) = store.create_msc4108_session("x", MSC4108_TTL_MS).await.unwrap();

    let raw = etag.trim_matches('"');
    assert!(raw.parse::<i64>().is_ok(), "etag payload must be a numeric timestamp, got: {raw}");
}
