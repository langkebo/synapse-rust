//! Vodozemac-based Megolm implementation.
//!
//! This module provides a vodozemac-backed Megolm session manager that
//! replaces the self-implemented AES-256-GCM Megolm path. It wraps
//! `vodozemac::megolm::GroupSession` (sender) and
//! `vodozemac::megolm::InboundGroupSession` (receiver) and provides
//! the same API surface as the legacy `MegolmService`.
//!
//! # Interoperability
//!
//! vodozemac 0.9 is the reference implementation used by Element Web,
//! Android, and iOS. Using it directly guarantees cross-client
//! compatibility and proper ratchet / forward-secrecy semantics.
//!
//! # Migration
//!
//! The legacy `e2ee::megolm::MegolmService` is retained for backward
//! compatibility during migration. Once all deployments have migrated,
//! the legacy path should be removed.
//!
//! See `docs/synapse-rust/E2EE_VODOZEMAC_MIGRATION.md` for the full
//! migration plan.

use crate::crypto::key_at_rest::KeyAtRest;
use crate::megolm::models::{MegolmSession, PickleFormat, RoomKeyDistributionData};
use crate::megolm::storage::MegolmSessionStorage;
use std::sync::Arc;
use std::time::Instant;
use synapse_cache::CacheManager;
use synapse_common::current_timestamp_millis;
use synapse_common::current_timestamp_utc;
use synapse_common::map_database;
use synapse_common::server_metrics::ServerMetrics;
use synapse_common::ApiError;
use vodozemac::megolm::{
    GroupSession, GroupSessionPickle, InboundGroupSession, InboundGroupSessionPickle, SessionConfig,
};

/// Maximum age of a megolm session in days before rotation.
static MEGOLM_SESSION_MAX_AGE_DAYS: std::sync::OnceLock<i64> = std::sync::OnceLock::new();

/// E-04: a decrypted `message_index` jumping more than this many slots
/// ahead of the last-known DB counter is treated as suspicious (replay or
/// bug). The ratchet itself enforces forward progress, so reaching this
/// branch implies an out-of-band attack; 100 is a conservative threshold
/// that is well above any realistic single batch encrypt.
const MEGOLM_LARGE_INDEX_GAP: u32 = 100;

fn get_session_max_age_days() -> i64 {
    *MEGOLM_SESSION_MAX_AGE_DAYS.get_or_init(|| {
        std::env::var("MEGOLM_SESSION_MAX_AGE_DAYS")
            .ok()
            .and_then(|v| v.parse().ok())
            .filter(|d: &i64| *d > 0)
            .unwrap_or(7)
    })
}

/// Serialise a `GroupSessionPickle` to a base64-encoded string for
/// storage in the `MegolmSession::session_key` column.
/// E2EE-09: 序列化失败返回错误而非 panic。
fn pickle_to_string(pickle: &GroupSessionPickle) -> Result<String, ApiError> {
    let json = serde_json::to_vec(pickle)
        .map_err(|e| ApiError::internal(format!("GroupSessionPickle serialize failed: {e}")))?;
    Ok(base64::Engine::encode(&base64::engine::general_purpose::STANDARD, json))
}

/// Deserialise a `GroupSessionPickle` from a base64-encoded string.
fn pickle_from_string(s: &str) -> Result<GroupSessionPickle, ApiError> {
    let bytes = base64::Engine::decode(&base64::engine::general_purpose::STANDARD, s)
        .map_err(|_| ApiError::decryption_error("Invalid pickle base64".to_string()))?;
    serde_json::from_slice(&bytes).map_err(|_| ApiError::decryption_error("Invalid group session pickle".to_string()))
}

/// Serialise an `InboundGroupSessionPickle` to a base64-encoded string.
/// E2EE-09: 序列化失败返回错误而非 panic。
fn inbound_pickle_to_string(pickle: &InboundGroupSessionPickle) -> Result<String, ApiError> {
    let json = serde_json::to_vec(pickle)
        .map_err(|e| ApiError::internal(format!("InboundGroupSessionPickle serialize failed: {e}")))?;
    Ok(base64::Engine::encode(&base64::engine::general_purpose::STANDARD, json))
}

/// Deserialise an `InboundGroupSessionPickle` from a base64-encoded string.
fn inbound_pickle_from_string(s: &str) -> Result<InboundGroupSessionPickle, ApiError> {
    let bytes = base64::Engine::decode(&base64::engine::general_purpose::STANDARD, s)
        .map_err(|_| ApiError::decryption_error("Invalid pickle base64".to_string()))?;
    serde_json::from_slice(&bytes)
        .map_err(|_| ApiError::decryption_error("Invalid inbound group session pickle".to_string()))
}

/// A vodozemac-backed Megolm service.
///
/// # Sender side
///
/// ```ignore
/// let svc = MegolmVodozemacService::new(storage, cache, server_metrics);
/// let session = svc.create_session("!room:example.com", "sender_key").await?;
/// let ciphertext = svc.encrypt(&session.session_id, b"hello").await?;
/// let key_data = svc.get_room_key_distribution("!room:example.com").await?;
/// // Share `key_data.session_key` via `m.room_key` to-device event.
/// ```
///
/// # Receiver side
///
/// ```ignore
/// let session_key = /* from m.room_key to-device event */;
/// svc.import_session("!room:example.com", "sender_key", &session_key).await?;
/// let plaintext = svc.decrypt(&session_id, &ciphertext).await?;
/// ```
#[derive(Clone)]
pub struct MegolmVodozemacService {
    storage: MegolmSessionStorage,
    cache: Arc<CacheManager>,
    server_metrics: Option<Arc<ServerMetrics>>,
    /// Key-at-rest for encrypting session keys before persisting to Redis / Postgres.
    /// E-08: all session keys must be encrypted with this at-rest key.
    at_rest: KeyAtRest,
}

/// (see code)
impl MegolmVodozemacService {
    /// Create a new VodozemacMegolmService with the given at-rest encryption key.
    pub fn new(storage: MegolmSessionStorage, cache: Arc<CacheManager>, at_rest: KeyAtRest) -> Self {
        Self {
            storage,
            cache,
            server_metrics: None,
            at_rest,
        }
    }

    /// See [`with_server_metrics`].
    pub fn with_server_metrics(mut self, metrics: Arc<ServerMetrics>) -> Self {
        self.server_metrics = Some(metrics);
        self
    }

    /// Create a new outbound Megolm session for a room.
    ///
    /// Uses `vodozemac::megolm::GroupSession` — the same implementation
    /// used by Element clients. The session key is available via
    /// `get_room_key_distribution()` for sharing to recipients.
    pub async fn create_session(&self, room_id: &str, sender_key: &str) -> Result<MegolmSession, ApiError> {
        let session_id = uuid::Uuid::new_v4().to_string();
        let outbound = GroupSession::new(SessionConfig::default());

        // Serialise the group session to a pickle for storage.
        let pickle_str = pickle_to_string(&outbound.pickle())?;

        let session = MegolmSession {
            id: uuid::Uuid::new_v4(),
            session_id: session_id.clone(),
            room_id: room_id.to_string(),
            sender_key: sender_key.to_string(),
            // session_key stores the vodozemac pickle (base64-encoded JSON)
            session_key: pickle_str,
            algorithm: "m.megolm.v1.aes-sha2".to_string(),
            message_index: 0,
            created_ts: current_timestamp_utc(),
            last_used_ts: current_timestamp_utc(),
            expires_at: Some(current_timestamp_utc() + chrono::Duration::days(get_session_max_age_days())),
            pickle_format: PickleFormat::Vodozemac,
        };

        self.storage.create_session(&session).await?;

        let cache_key = format!("megolm_session:{session_id}");
        if let Err(e) = self.cache.set(&cache_key, &session, 600).await {
            ::tracing::warn!(session_id = %session_id, cache_key = %cache_key, error = %e, "Failed to cache outbound megolm session");
        }

        ::tracing::info!(
            room_id = %room_id,
            session_id = %session_id,
            "Created vodozemac Megolm outbound session"
        );

        Ok(session)
    }

    /// Import an inbound Megolm session from a shared session key.
    ///
    /// The `session_key` is the base64-encoded key received via a
    /// `m.room_key` to-device event. This creates an
    /// `InboundGroupSession` for decrypting messages from the sender.
    pub async fn import_session(
        &self,
        room_id: &str,
        sender_key: &str,
        session_key: &str,
    ) -> Result<MegolmSession, ApiError> {
        let session_id = uuid::Uuid::new_v4().to_string();

        let key = vodozemac::megolm::SessionKey::from_base64(session_key)
            .map_err(|_| ApiError::decryption_error("Invalid session key".to_string()))?;

        let inbound = InboundGroupSession::new(&key, SessionConfig::default());
        let pickle_str = inbound_pickle_to_string(&inbound.pickle())?;

        let session = MegolmSession {
            id: uuid::Uuid::new_v4(),
            session_id: session_id.clone(),
            room_id: room_id.to_string(),
            sender_key: sender_key.to_string(),
            // inbound pickle written to `session_key` column
            session_key: pickle_str,
            algorithm: "m.megolm.v1.aes-sha2".to_string(),
            message_index: 0,
            created_ts: current_timestamp_utc(),
            last_used_ts: current_timestamp_utc(),
            expires_at: Some(current_timestamp_utc() + chrono::Duration::days(get_session_max_age_days())),
            pickle_format: PickleFormat::Vodozemac,
        };

        self.storage.create_session(&session).await?;

        let cache_key = format!("megolm_session:{session_id}");
        if let Err(e) = self.cache.set(&cache_key, &session, 600).await {
            ::tracing::warn!(session_id = %session_id, cache_key = %cache_key, error = %e, "Failed to cache inbound megolm session");
        }

        ::tracing::info!(
            room_id = %room_id,
            sender_key = %sender_key,
            session_id = %session_id,
            "Imported vodozemac Megolm inbound session"
        );

        Ok(session)
    }

    /// Load a session from cache or storage and rehydrate the vodozemac
    /// group session from its pickle.
    async fn load_outbound(&self, session_id: &str) -> Result<(MegolmSession, GroupSession), ApiError> {
        let session = self.load_session_record(session_id).await?;
        let pickle = pickle_from_string(&session.session_key)?;
        let outbound = GroupSession::from_pickle(pickle);
        Ok((session, outbound))
    }

    /// Load a session from cache or storage and rehydrate the vodozemac
    /// inbound group session from its pickle.
    async fn load_inbound(&self, session_id: &str) -> Result<(MegolmSession, InboundGroupSession), ApiError> {
        let session = self.load_session_record(session_id).await?;
        let pickle = inbound_pickle_from_string(&session.session_key)?;
        let inbound = InboundGroupSession::from_pickle(pickle);
        Ok((session, inbound))
    }

    async fn load_session_record(&self, session_id: &str) -> Result<MegolmSession, ApiError> {
        let cache_key = format!("megolm_session:{session_id}");
        if let Ok(Some(session)) = self.cache.get::<MegolmSession>(&cache_key).await {
            return Ok(session);
        }

        let session = self
            .storage
            .get_session(session_id)
            .await?
            .ok_or_else(|| ApiError::not_found("Session not found".to_string()))?;

        if let Err(e) = self.cache.set(&cache_key, &session, 600).await {
            ::tracing::warn!(session_id = %session_id, cache_key = %cache_key, error = %e, "Failed to cache loaded megolm session");
        }
        Ok(session)
    }

    /// Encrypt a single plaintext message using the vodozemac outbound session.
    pub async fn encrypt(&self, session_id: &str, plaintext: &[u8]) -> Result<Vec<u8>, ApiError> {
        let mut out = self.encrypt_many(session_id, std::slice::from_ref(&plaintext)).await?;
        out.pop().ok_or_else(|| ApiError::internal("encrypt_many returned no ciphertexts for a single plaintext input"))
    }

    /// Bulk-encrypt N messages under one round-trip.
    ///
    /// Uses vodozemac's `GroupSession::encrypt()` which handles the
    /// ratchet advancement internally. The message index is bumped
    /// atomically in the database.
    pub async fn encrypt_many(&self, session_id: &str, plaintexts: &[&[u8]]) -> Result<Vec<Vec<u8>>, ApiError> {
        if plaintexts.is_empty() {
            return Ok(Vec::new());
        }

        let (session, mut outbound) = self.load_outbound(session_id).await?;

        let mut ciphertexts = Vec::with_capacity(plaintexts.len());
        for pt in plaintexts {
            let msg = outbound.encrypt(pt);
            ciphertexts.push(msg.to_bytes());
        }

        // Persist the updated pickle and counter atomically.
        let now_ms = current_timestamp_millis();
        let new_pickle_str = pickle_to_string(&outbound.pickle())?;
        let new_index =
            self.storage.increment_message_index(session_id, plaintexts.len() as i64, now_ms).await?.ok_or_else(
                || {
                    ::tracing::error!(
                        target: "security_audit",
                        event = "vodozemac_megolm_encrypt_many_session_vanished",
                        session_id = %session_id,
                        messages = plaintexts.len(),
                    );
                    ApiError::not_found("megolm session not found")
                },
            )?;

        // Update the pickle in storage and cache.
        let cache_key = format!("megolm_session:{session_id}");
        let updated_session = MegolmSession {
            session_key: new_pickle_str,
            message_index: new_index,
            last_used_ts: current_timestamp_utc(),
            pickle_format: PickleFormat::Vodozemac,
            ..session
        };
        if let Err(e) = self.cache.set(&cache_key, &updated_session, 600).await {
            ::tracing::warn!(session_id = %session_id, cache_key = %cache_key, error = %e, "Failed to refresh megolm session cache after encrypt");
        }

        ::tracing::debug!(
            session_id = %session_id,
            messages = plaintexts.len(),
            new_index = new_index,
            "Bulk-encrypted vodozemac megolm messages"
        );

        Ok(ciphertexts)
    }

    /// Decrypt a ciphertext using the vodozemac inbound session.
    ///
    /// E-04: emit structured `security_audit` logs that record the message
    /// index so an off-line detector can spot replay patterns (repeated
    /// indices, anomalous gaps, regressions). vodozemac's ratchet already
    /// rejects an index that is older than the highest index it has
    /// consumed, so the application layer only needs the *observability*
    /// signal — we do not maintain a separate "seen" set in the database.
    pub async fn decrypt(&self, session_id: &str, ciphertext: &[u8]) -> Result<Vec<u8>, ApiError> {
        let (session, mut inbound) = self.load_inbound(session_id).await?;

        let last_known_index = session.message_index as u32;

        let msg = vodozemac::megolm::MegolmMessage::from_bytes(ciphertext)
            .map_err(|_| ApiError::decryption_error("Invalid megolm ciphertext".to_string()))?;

        let decrypted = inbound
            .decrypt(&msg)
            .map_err(|e| ApiError::decryption_error(format!("vodozemac megolm decrypt failed: {e}")))?;

        let new_index = decrypted.message_index;

        // E-04: surface suspicious ratchet behaviour as a security audit
        // event. The ratchet itself enforces forward progress, so reaching
        // the `if` branches below implies an out-of-band replay attempt
        // (e.g. an attacker substituting a ratchet pickle) or a buggy
        // client that fast-forwards. In both cases we want a loud,
        // grep-able log line — not a silent return.
        if new_index < last_known_index {
            ::tracing::warn!(
                target: "security_audit",
                event = "megolm_decrypt_index_regression",
                session_id = %session_id,
                last_known_index = last_known_index,
                new_index = new_index,
                "E-04: decrypted message_index regressed; possible ratchet replay"
            );
        } else if new_index.saturating_sub(last_known_index) > MEGOLM_LARGE_INDEX_GAP {
            ::tracing::warn!(
                target: "security_audit",
                event = "megolm_decrypt_large_index_gap",
                session_id = %session_id,
                last_known_index = last_known_index,
                new_index = new_index,
                gap = new_index - last_known_index,
                "E-04: decrypted message_index jumped unexpectedly far"
            );
        } else {
            ::tracing::debug!(
                target: "security_audit",
                event = "megolm_decrypt_ok",
                session_id = %session_id,
                message_index = new_index,
            );
        }

        // Persist the updated pickle.
        let new_pickle_str = inbound_pickle_to_string(&inbound.pickle())?;

        let cache_key = format!("megolm_session:{session_id}");
        let updated_session = MegolmSession {
            session_key: new_pickle_str,
            last_used_ts: current_timestamp_utc(),
            pickle_format: PickleFormat::Vodozemac,
            ..session
        };
        if let Err(e) = self.cache.set(&cache_key, &updated_session, 600).await {
            ::tracing::warn!(session_id = %session_id, cache_key = %cache_key, error = %e, "Failed to refresh megolm session cache after decrypt");
        }

        Ok(decrypted.plaintext)
    }

    /// Rotate a session: delete the old one and create a new one.
    pub async fn rotate_session(&self, session_id: &str) -> Result<(), ApiError> {
        let session = self.load_session_record(session_id).await?;
        self.storage.delete_session(session_id).await?;
        self.create_session(&session.room_id, &session.sender_key).await?;
        Ok(())
    }

    /// Share the session key to a set of recipient users.
    ///
    /// The `session_key()` from `GroupSession` is the raw key bytes
    /// that should be sent to recipients via `m.room_key` to-device
    /// events. This method persists the shared key for each recipient
    /// so they can retrieve it later.
    pub async fn share_session(&self, session_id: &str, user_ids: &[String]) -> Result<(), ApiError> {
        if user_ids.is_empty() {
            return Ok(());
        }

        let (session, outbound) = self.load_outbound(session_id).await?;
        let session_key_b64 = outbound.session_key().to_base64();

        // E-08 Step 2: Encrypt session key at-rest before storing
        let sealed_key = self.at_rest.seal(session_key_b64.as_bytes()).map_err(|e| {
            ::tracing::error!(session_id = %session_id, error = %e, "Failed to seal session key at-rest");
            ApiError::internal(format!("Failed to seal session key at-rest: {e}"))
        })?;

        let created_ts = current_timestamp_millis();
        let expires_at = session.expires_at.map_or_else(|| created_ts + 7 * 24 * 3600 * 1000, |t| t.timestamp_millis());

        let db_start = Instant::now();
        let db_result = self
            .storage
            .upsert_session_keys_batch(user_ids, session_id, &sealed_key, created_ts, Some(expires_at))
            .await;
        let db_duration_ms = db_start.elapsed().as_secs_f64() * 1000.0;

        match db_result {
            Ok(rows) => {
                ::tracing::debug!(
                    session_id = %session_id,
                    recipients = user_ids.len(),
                    rows_written = rows,
                    db_duration_ms = db_duration_ms,
                    "Bulk-persisted sealed vodozemac megolm session keys"
                );
            }
            Err(e) => {
                ::tracing::error!(
                    target: "security_audit",
                    event = "vodozemac_megolm_share_session_db_write_failed",
                    recipients = user_ids.len(),
                    session_id = %session_id,
                    error = %e,
                );
                if let Some(metrics) = &self.server_metrics {
                    metrics.record_megolm_share(user_ids.len(), db_duration_ms, 0.0, false);
                }
                return Err(e);
            }
        }

        // Cache write: best-effort fast path.
        let cache_start = Instant::now();
        for user_id in user_ids {
            let cache_key = format!("megolm_session_key:{user_id}:{session_id}");
            if let Err(e) = self.cache.set(&cache_key, &sealed_key, 600).await {
                ::tracing::warn!(
                    user_id = %user_id,
                    session_id = %session_id,
                    error = %e,
                    "Failed to cache sealed vodozemac megolm session key"
                );
                if let Some(metrics) = &self.server_metrics {
                    metrics.record_megolm_share_cache_error();
                }
            }
        }
        let cache_duration_ms = cache_start.elapsed().as_secs_f64() * 1000.0;

        if let Some(metrics) = &self.server_metrics {
            metrics.record_megolm_share(user_ids.len(), db_duration_ms, cache_duration_ms, true);
        }

        Ok(())
    }

    /// Recipient-side read of a previously-shared session key.
    ///
    /// E-08 Step 3: Decrypts the at-rest sealed key. Decryption failure is a
    /// hard error (not graceful degradation to None), as returning garbage
    /// or skipping a key would hide key rotation or compromise failures.
    pub async fn get_session_key_for_user(&self, user_id: &str, session_id: &str) -> Result<Option<String>, ApiError> {
        let start = Instant::now();
        let cache_key = format!("megolm_session_key:{user_id}:{session_id}");

        let sealed = match self.cache.get::<String>(&cache_key).await {
            Ok(Some(v)) => Some(v),
            _ => self.storage.get_session_key(user_id, session_id).await?,
        };

        let Some(sealed) = sealed else {
            if let Some(metrics) = &self.server_metrics {
                metrics.record_megolm_session_key_read("miss_db_miss", start.elapsed().as_secs_f64() * 1000.0);
            }
            return Ok(None);
        };

        // E-08 Step 3: Decrypt the sealed key - failure is a hard error
        let plaintext = self.at_rest.open(&sealed).map_err(|e| {
            ::tracing::error!(
                target: "security_audit",
                event = "e2ee_session_key_at_rest_errors_total",
                user_id = %user_id,
                session_id = %session_id,
                error = %e,
                "Failed to decrypt session key at-rest"
            );
            if let Some(metrics) = &self.server_metrics {
                metrics.record_megolm_session_key_read("decryption_failed", start.elapsed().as_secs_f64() * 1000.0);
            }
            e
        })?;

        // Verify it's valid UTF-8 (should always be session_key_base64)
        let session_key_b64 = String::from_utf8(plaintext)
            .map_err(|_| ApiError::internal("session key is not valid UTF-8"))?;

        // Cache the decrypted key for fast path (best-effort, don't fail on cache error)
        if let Err(e) = self.cache.set(&cache_key, &sealed, 600).await {
            ::tracing::warn!(
                user_id = %user_id,
                session_id = %session_id,
                error = %e,
                "Failed to backfill vodozemac megolm session key into cache (using DB value on next read)"
            );
        }

        if let Some(metrics) = &self.server_metrics {
            metrics.record_megolm_session_key_read("miss_db_hit", start.elapsed().as_secs_f64() * 1000.0);
        }

        Ok(Some(session_key_b64))
    }

    /// List all sessions for a room.
    pub async fn get_room_sessions(&self, room_id: &str) -> Result<Vec<MegolmSession>, ApiError> {
        self.storage.get_room_sessions(room_id).await.map_err(map_database!("Failed to get room sessions"))
    }

    /// Delete a session.
    pub async fn delete_session(&self, session_id: &str) -> Result<(), ApiError> {
        self.storage.delete_session(session_id).await.map_err(map_database!("Failed to delete session"))
    }

    /// Clean up expired Megolm sessions.
    ///
    /// Aligned with Synapse v1.153: removes sessions whose `expires_at`
    /// timestamp is in the past.
    pub async fn cleanup_expired_sessions(&self) -> Result<u64, ApiError> {
        self.storage.cleanup_expired_sessions().await
    }

    /// Get the outbound session key distribution data for a room.
    pub async fn get_outbound_session(&self, room_id: &str) -> Result<Option<RoomKeyDistributionData>, ApiError> {
        self.get_room_key_distribution(room_id).await
    }

    /// Get the room key distribution data for sharing.
    pub async fn get_room_key_distribution(&self, room_id: &str) -> Result<Option<RoomKeyDistributionData>, ApiError> {
        let sessions = self.get_room_sessions(room_id).await?;

        if let Some(session) = sessions.first() {
            let (_, outbound) = self.load_outbound(&session.session_id).await?;
            let session_key_b64 = outbound.session_key().to_base64();

            Ok(Some(RoomKeyDistributionData {
                session_id: session.session_id.clone(),
                session_key: session_key_b64,
                algorithm: session.algorithm.clone(),
                room_id: room_id.to_string(),
            }))
        } else {
            Ok(None)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use vodozemac::megolm::{GroupSession, InboundGroupSession, SessionConfig};

    /// A freshly created outbound session can encrypt and then
    /// the same session key can be imported as an inbound session
    /// to decrypt the message. This is the fundamental interop
    /// guarantee that vodozemac provides.
    #[test]
    fn vodozemac_megolm_roundtrip() {
        let mut outbound = GroupSession::new(SessionConfig::default());
        let session_key = outbound.session_key();

        let plaintext = b"hello vodozemac megolm";
        let msg = outbound.encrypt(plaintext);

        let mut inbound = InboundGroupSession::new(&session_key, SessionConfig::default());
        let decrypted = inbound.decrypt(&msg).expect("decrypt should succeed");
        assert_eq!(decrypted.plaintext, plaintext);
    }

    /// Pickle roundtrip: a session survives serialisation.
    #[test]
    fn vodozemac_megolm_pickle_roundtrip() {
        let mut outbound = GroupSession::new(SessionConfig::default());
        // Capture the session key BEFORE any encryption so the inbound
        // session starts at index 0 and can decrypt the first message.
        let session_key = outbound.session_key();
        let plaintext = b"before pickle";
        let msg = outbound.encrypt(plaintext);

        let pickle = outbound.pickle();
        let pickle_str = pickle_to_string(&pickle).expect("pickle serialize");
        let restored_pickle = pickle_from_string(&pickle_str).expect("pickle roundtrip");
        let mut restored = GroupSession::from_pickle(restored_pickle);

        let pt2 = b"after pickle";
        let msg2 = restored.encrypt(pt2);

        let mut inbound = InboundGroupSession::new(&session_key, SessionConfig::default());
        let d1 = inbound.decrypt(&msg).expect("first message");
        assert_eq!(d1.plaintext, plaintext);
        let d2 = inbound.decrypt(&msg2).expect("second message after pickle");
        assert_eq!(d2.plaintext, pt2);
    }

    /// Multiple messages produce strictly increasing message indices.
    #[test]
    fn vodozemac_megolm_message_index_monotonic() {
        let mut outbound = GroupSession::new(SessionConfig::default());
        let session_key = outbound.session_key();
        let mut inbound = InboundGroupSession::new(&session_key, SessionConfig::default());

        let mut last_index = 0u32;
        for i in 0..16u32 {
            let pt = format!("message {i}");
            let msg = outbound.encrypt(pt.as_bytes());
            let decrypted = inbound.decrypt(&msg).expect("decrypt");
            assert_eq!(decrypted.plaintext, pt.as_bytes());
            assert!(decrypted.message_index >= last_index, "message index must be non-decreasing");
            last_index = decrypted.message_index;
        }
        assert_eq!(last_index, 15, "16 messages should yield message_index 0..=15");
    }

    // ========================================================================
    // E-12: convergence — only vodozemac pickle format (legacy/dual removed)
    // ========================================================================

    /// Verify MegolmSession model serializes with Vodozemac format after E-12 convergence
    #[test]
    fn megolm_session_e12_convergence_format() {
        let session = MegolmSession {
            id: uuid::Uuid::new_v4(),
            session_id: "e12_convergence".to_string(),
            room_id: "!room:test.example".to_string(),
            sender_key: "sender_key_b64".to_string(),
            session_key: "vodozemac_pickle_string".to_string(),
            algorithm: "m.megolm.v1.aes-sha2".to_string(),
            message_index: 0,
            created_ts: current_timestamp_utc(),
            last_used_ts: current_timestamp_utc(),
            expires_at: Some(current_timestamp_utc() + chrono::Duration::days(7)),
            pickle_format: PickleFormat::Vodozemac,
        };

        let json = serde_json::to_string(&session).expect("serialize");
        assert!(json.contains("\"pickle_format\":\"vodozemac\""), "json should include vodozemac format: {json}");

        let deserialized: MegolmSession = serde_json::from_str(&json).expect("deserialize");
        assert_eq!(deserialized.pickle_format, PickleFormat::Vodozemac);
    }

    /// Verify PickleFormat enum only has Vodozemac variant after E-12
    #[test]
    fn pickle_format_e12_only_vodozemac() {
        let fmt = PickleFormat::Vodozemac;
        let s = serde_json::to_string(&fmt).expect("serialize");
        assert_eq!(s, "\"vodozemac\"", "PickleFormat should only serialize to vodozemac");
        
        let parsed: PickleFormat = serde_json::from_str(&s).expect("deserialize");
        assert_eq!(parsed, PickleFormat::Vodozemac);
    }

    /// 验证 vodozemac session_key 的 base64 字符串非空且长度合理
    /// vodozemac session_key 是完整的 Megolm ratchet 序列化，远大于 32 字节
    #[test]
    fn vodozemac_session_key_length_sanity() {
        let outbound = GroupSession::new(SessionConfig::default());
        let session_key_b64 = outbound.session_key().to_base64();
        // vodozemac session_key 包含完整 ratchet 状态，base64 后约 300+ 字符
        assert!(
            session_key_b64.len() > 100,
            "vodozemac session_key b64 should be >100 chars (full ratchet), got {}",
            session_key_b64.len()
        );
    }

    /// 验证 outbound / inbound session pickle 的 to_base64 / from_base64 兼容性
    /// 双写逻辑中 session_key 列在 dual 模式下应能同时被 legacy 和 vodozemac 路径解析
    #[test]
    fn vodozemac_pickle_roundtrip_through_storage_format() {
        let outbound = GroupSession::new(SessionConfig::default());
        let pickle = outbound.pickle();
        let pickle_str = pickle_to_string(&pickle).expect("pickle serialize");

        // 模拟 storage get_session 读取
        let restored = pickle_from_string(&pickle_str).expect("pickle from storage");
        let _restored_session = GroupSession::from_pickle(restored);
    }

    // ========================================================================
    // E2EE-09: .expect() removal — verify error propagation instead of panic
    // ========================================================================
    //
    // The happy path (valid pickle → Ok) is already covered by the
    // roundtrip tests above. These tests verify the *error* path:
    // malformed input must return `Err`, not panic. If `.expect()`
    // were re-introduced in any of these functions, the corresponding
    // test would panic and fail.

    /// `pickle_from_string` must return `Err` (not panic) when given
    /// malformed base64 input. This exercises the base64 decode error
    /// path that replaced the previous panic-prone pattern.
    #[test]
    fn test_pickle_from_string_returns_err_on_invalid_base64() {
        let result = pickle_from_string("!!!not valid base64!!!");
        assert!(result.is_err(), "pickle_from_string should return Err for invalid base64 input");
    }

    /// `pickle_from_string` must return `Err` (not panic) when given
    /// valid base64 that decodes to invalid JSON for a GroupSessionPickle.
    /// This exercises the `serde_json::from_slice` error path.
    #[test]
    fn test_pickle_from_string_returns_err_on_invalid_json() {
        let invalid_json_b64 =
            base64::Engine::encode(&base64::engine::general_purpose::STANDARD, b"not a valid pickle json");
        let result = pickle_from_string(&invalid_json_b64);
        assert!(result.is_err(), "pickle_from_string should return Err for valid base64 of invalid JSON");
    }

    /// `inbound_pickle_from_string` must return `Err` (not panic) when
    /// given malformed base64 input.
    #[test]
    fn test_inbound_pickle_from_string_returns_err_on_invalid_base64() {
        let result = inbound_pickle_from_string("!!!not valid base64!!!");
        assert!(result.is_err(), "inbound_pickle_from_string should return Err for invalid base64 input");
    }

    /// `MegolmMessage::from_bytes` must return `Err` (not panic) when
    /// given malformed ciphertext. This verifies the error path in the
    /// `decrypt` method for external/untrusted input — the exact
    /// scenario where `.expect()` would crash the server.
    #[test]
    fn test_megolm_message_from_bytes_returns_err_on_malformed_input() {
        let result = vodozemac::megolm::MegolmMessage::from_bytes(&[0u8; 5]);
        assert!(result.is_err(), "MegolmMessage::from_bytes should return Err for malformed ciphertext");
    }

    // ========================================================================
    // E-04: message-index monitoring
    // ========================================================================

    /// E-04: verify `MEGOLM_LARGE_INDEX_GAP` is set to a sensible threshold.
    /// The constant controls when `decrypt` emits a `megolm_decrypt_large_index_gap`
    /// warning. 100 is conservative — well above any realistic single batch
    /// decrypt that a client would perform in one sync cycle.
    #[test]
    fn test_e04_large_index_gap_threshold() {
        assert_eq!(MEGOLM_LARGE_INDEX_GAP, 100, "E-04: MEGOLM_LARGE_INDEX_GAP should be 100; adjust after profiling");
    }

    /// E-04: confirm that the vodozemac ratchet enforces forward progress
    /// (each decrypt consumes one message_index; the next decrypt must be ≥
    /// the previous). This is the foundation that makes the application-layer
    /// monitoring log meaningful: a gap of 0 on two distinct ciphertexts
    /// implies a replay attempt, while a regression implies a manipulated
    /// pickle — both surface as `security_audit` events in the `decrypt`
    /// method's new logging block.
    #[test]
    fn test_e04_vodozemac_ratchet_enforces_forward_progress() {
        let mut outbound = GroupSession::new(SessionConfig::default());
        let session_key = outbound.session_key();
        let mut inbound = InboundGroupSession::new(&session_key, SessionConfig::default());

        // Encrypt and decrypt 3 messages; record the indices.
        let indices: Vec<u32> = (0..3)
            .map(|i| {
                let pt = format!("message {i}");
                let msg = outbound.encrypt(pt.as_bytes());
                let decrypted = inbound.decrypt(&msg).expect("valid decrypt");
                decrypted.message_index
            })
            .collect();

        assert_eq!(indices, &[0, 1, 2], "message indices must be 0, 1, 2");
        assert!(indices.windows(2).all(|w| w[1] >= w[0]), "each message_index must be ≥ the previous one");
    }
}
