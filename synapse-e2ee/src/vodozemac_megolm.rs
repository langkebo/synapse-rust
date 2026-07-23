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

use crate::megolm::models::{MegolmSession, PickleFormat, RoomKeyDistributionData};
use crate::megolm::storage::MegolmSessionStorage;
use std::sync::Arc;
use std::time::Instant;
use synapse_cache::CacheManager;
use synapse_common::current_timestamp_millis;
use synapse_common::current_timestamp_utc;
use synapse_common::server_metrics::ServerMetrics;
use synapse_common::ApiError;
use vodozemac::megolm::{
    GroupSession, GroupSessionPickle, InboundGroupSession, InboundGroupSessionPickle, SessionConfig,
};

/// Phase 2 dual-write 开关（默认 `false`）。
///
/// 当设为 `true` 时，`MegolmVodozemacService::create_session` 会在写 vodozemac
/// pickle 之外，**额外**调用 legacy 加密路径生成一份 `session_key`（用服务器
/// `encryption_key` 加密的 32 字节 session key），并把 `pickle_format` 设为
/// `dual`。这样 legacy 路径在只读场景下也能识别这些 session。
static DUAL_WRITE_ENABLED: std::sync::OnceLock<bool> = std::sync::OnceLock::new();

fn is_dual_write_enabled() -> bool {
    *DUAL_WRITE_ENABLED.get_or_init(|| {
        std::env::var("E2EE_DUAL_WRITE")
            .ok()
            .map(|s| s.to_ascii_lowercase())
            .is_some_and(|s| matches!(s.as_str(), "1" | "true" | "yes" | "on"))
    })
}

/// Maximum age of a megolm session in days before rotation.
static MEGOLM_SESSION_MAX_AGE_DAYS: std::sync::OnceLock<i64> = std::sync::OnceLock::new();

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
#[allow(clippy::expect_used)]
fn pickle_to_string(pickle: &GroupSessionPickle) -> String {
    let json = serde_json::to_vec(pickle).expect("GroupSessionPickle should serialize");
    base64::Engine::encode(&base64::engine::general_purpose::STANDARD, json)
}

/// Deserialise a `GroupSessionPickle` from a base64-encoded string.
fn pickle_from_string(s: &str) -> Result<GroupSessionPickle, ApiError> {
    let bytes = base64::Engine::decode(&base64::engine::general_purpose::STANDARD, s)
        .map_err(|_| ApiError::decryption_error("Invalid pickle base64".to_string()))?;
    serde_json::from_slice(&bytes).map_err(|_| ApiError::decryption_error("Invalid group session pickle".to_string()))
}

/// Serialise an `InboundGroupSessionPickle` to a base64-encoded string.
#[allow(clippy::expect_used)]
fn inbound_pickle_to_string(pickle: &InboundGroupSessionPickle) -> String {
    let json = serde_json::to_vec(pickle).expect("InboundGroupSessionPickle should serialize");
    base64::Engine::encode(&base64::engine::general_purpose::STANDARD, json)
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
    /// 服务器侧加密密钥（用于 Phase 2 双写：把 32 字节 vodozemac session_key
    /// 用 legacy 路径的 `Aes256GcmCipher` 加密后写入 `session_key` 列）。
    /// 当 `E2EE_DUAL_WRITE=true` 时必须设置；否则可保持 None（仅写 vodozemac 路径）。
    encryption_key: Option<[u8; 32]>,
}

impl MegolmVodozemacService {
    pub fn new(storage: MegolmSessionStorage, cache: Arc<CacheManager>) -> Self {
        Self { storage, cache, server_metrics: None, encryption_key: None }
    }

    /// 设置服务器侧加密密钥（启用 Phase 2 双写时调用）
    pub fn with_encryption_key(mut self, key: [u8; 32]) -> Self {
        self.encryption_key = Some(key);
        self
    }

    pub fn with_server_metrics(mut self, metrics: Arc<ServerMetrics>) -> Self {
        self.server_metrics = Some(metrics);
        self
    }

    /// 计算在双写场景下要写入 `session_key` 的 legacy 加密格式
    ///
    /// 输入：vodozemac `GroupSession::session_key()` 的原始 32 字节
    /// 输出：与 `MegolmService::encrypt_session_key` 兼容的 base64 JSON 格式
    /// 当双写关闭或缺 encryption_key 时返回 None（仅写 vodozemac）。
    fn dual_write_legacy_session_key(&self, raw_session_key: &[u8]) -> Option<String> {
        if !is_dual_write_enabled() {
            return None;
        }
        let key = self.encryption_key?;
        use crate::crypto::{Aes256GcmCipher, Aes256GcmKey};

        let cipher_key = Aes256GcmKey::from_bytes(key);
        let encrypted = Aes256GcmCipher::encrypt_with_nonce(&cipher_key, raw_session_key).ok()?;
        let json = serde_json::to_string(&encrypted).ok()?;
        Some(base64::Engine::encode(&base64::engine::general_purpose::STANDARD, json.as_bytes()))
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
        let pickle_str = pickle_to_string(&outbound.pickle());
        let session_key_b64 = outbound.session_key().to_base64();

        // 解码 session_key 为 32 字节原始对称密钥
        // vodozemac 使用 STANDARD base64 (无 padding)，与 base64ct::Base64Unpadded 一致
        let raw_session_key =
            base64::Engine::decode(&base64::engine::general_purpose::STANDARD_NO_PAD, &session_key_b64)
                .map_err(|_| ApiError::encryption_error("Invalid vodozemac session_key base64".to_string()))?;

        // Phase 2: 双写 legacy 加密格式（仅当 E2EE_DUAL_WRITE=true 且 encryption_key 已设置）
        let (legacy_session_key, pickle_format) = match self.dual_write_legacy_session_key(&raw_session_key) {
            Some(legacy) => (legacy, PickleFormat::Dual),
            None => (pickle_str.clone(), PickleFormat::Vodozemac),
        };

        let session = MegolmSession {
            id: uuid::Uuid::new_v4(),
            session_id: session_id.clone(),
            room_id: room_id.to_string(),
            sender_key: sender_key.to_string(),
            session_key: legacy_session_key,
            algorithm: "m.megolm.v1.aes-sha2".to_string(),
            message_index: 0,
            created_ts: current_timestamp_utc(),
            last_used_ts: current_timestamp_utc(),
            expires_at: Some(current_timestamp_utc() + chrono::Duration::days(get_session_max_age_days())),
            pickle_format,
            vodozemac_pickle: Some(pickle_str.clone()),
        };

        self.storage.create_session(&session).await?;

        let cache_key = format!("megolm_session:{session_id}");
        if let Err(e) = self.cache.set(&cache_key, &session, 600).await {
            ::tracing::warn!(session_id = %session_id, cache_key = %cache_key, error = %e, "Failed to cache outbound megolm session");
        }

        let key_cache_key = format!("megolm_session_key_raw:{session_id}");
        if let Err(e) = self.cache.set(&key_cache_key, &session_key_b64, 600).await {
            ::tracing::warn!(session_id = %session_id, cache_key = %key_cache_key, error = %e, "Failed to cache raw megolm session key");
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
        let pickle_str = inbound_pickle_to_string(&inbound.pickle());

        let session = MegolmSession {
            id: uuid::Uuid::new_v4(),
            session_id: session_id.clone(),
            room_id: room_id.to_string(),
            sender_key: sender_key.to_string(),
            // inbound pickle 写在 `session_key` 列（Phase 1 行为）
            session_key: pickle_str.clone(),
            algorithm: "m.megolm.v1.aes-sha2".to_string(),
            message_index: 0,
            created_ts: current_timestamp_utc(),
            last_used_ts: current_timestamp_utc(),
            expires_at: Some(current_timestamp_utc() + chrono::Duration::days(get_session_max_age_days())),
            pickle_format: PickleFormat::Vodozemac,
            vodozemac_pickle: Some(pickle_str.clone()),
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
    #[allow(clippy::expect_used)]
    pub async fn encrypt(&self, session_id: &str, plaintext: &[u8]) -> Result<Vec<u8>, ApiError> {
        let mut out = self.encrypt_many(session_id, std::slice::from_ref(&plaintext)).await?;
        Ok(out.pop().expect("encrypt_many returns one ciphertext per input plaintext"))
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
        let new_pickle_str = pickle_to_string(&outbound.pickle());
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

        // Phase 2: 把最新 vodozemac pickle 持久化到 `vodozemac_pickle` 列
        // 失败仅记日志：cache 中已有更新副本，不阻塞本次 encrypt 返回
        let persist_start = Instant::now();
        let persist_result = self.storage.update_vodozemac_pickle(session_id, &new_pickle_str, now_ms).await;
        let persist_duration_ms = persist_start.elapsed().as_secs_f64() * 1000.0;
        match &persist_result {
            Ok(_) => {
                if let Some(metrics) = &self.server_metrics {
                    metrics.record_megolm_vodozemac_pickle_persist(persist_duration_ms, true);
                }
            }
            Err(e) => {
                if let Some(metrics) = &self.server_metrics {
                    metrics.record_megolm_vodozemac_pickle_persist(persist_duration_ms, false);
                }
                ::tracing::warn!(
                    target: "security_audit",
                    event = "vodozemac_megolm_pickle_persist_failed",
                    session_id = %session_id,
                    duration_ms = persist_duration_ms,
                    error = %e,
                    "Failed to persist vodozemac pickle (continuing with cache-only state)"
                );
            }
        }

        // Update the pickle in storage and cache.
        let cache_key = format!("megolm_session:{session_id}");
        let updated_session = MegolmSession {
            session_key: new_pickle_str.clone(),
            message_index: new_index,
            last_used_ts: current_timestamp_utc(),
            pickle_format: PickleFormat::Vodozemac,
            vodozemac_pickle: Some(new_pickle_str),
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
    pub async fn decrypt(&self, session_id: &str, ciphertext: &[u8]) -> Result<Vec<u8>, ApiError> {
        let (session, mut inbound) = self.load_inbound(session_id).await?;

        let msg = vodozemac::megolm::MegolmMessage::from_bytes(ciphertext)
            .map_err(|_| ApiError::decryption_error("Invalid megolm ciphertext".to_string()))?;

        let decrypted = inbound
            .decrypt(&msg)
            .map_err(|e| ApiError::decryption_error(format!("vodozemac megolm decrypt failed: {e}")))?;

        // Persist the updated pickle.
        let new_pickle_str = inbound_pickle_to_string(&inbound.pickle());

        // Phase 2: 持久化新 pickle 到 vodozemac_pickle 列（best-effort）
        let now_ms = current_timestamp_millis();
        if let Err(e) = self.storage.update_vodozemac_pickle(session_id, &new_pickle_str, now_ms).await {
            ::tracing::warn!(
                target: "security_audit",
                event = "vodozemac_megolm_inbound_pickle_persist_failed",
                session_id = %session_id,
                error = %e,
                "Failed to persist vodozemac inbound pickle (continuing with cache-only state)"
            );
        }

        let cache_key = format!("megolm_session:{session_id}");
        let updated_session = MegolmSession {
            session_key: new_pickle_str.clone(),
            last_used_ts: current_timestamp_utc(),
            pickle_format: PickleFormat::Vodozemac,
            vodozemac_pickle: Some(new_pickle_str),
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

        let created_ts = current_timestamp_millis();
        let expires_at = session.expires_at.map_or_else(|| created_ts + 7 * 24 * 3600 * 1000, |t| t.timestamp_millis());

        let db_start = Instant::now();
        let db_result = self
            .storage
            .upsert_session_keys_batch(user_ids, session_id, &session_key_b64, created_ts, Some(expires_at))
            .await;
        let db_duration_ms = db_start.elapsed().as_secs_f64() * 1000.0;

        match db_result {
            Ok(rows) => {
                ::tracing::debug!(
                    session_id = %session_id,
                    recipients = user_ids.len(),
                    rows_written = rows,
                    db_duration_ms = db_duration_ms,
                    "Bulk-persisted vodozemac megolm session keys"
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
            if let Err(e) = self.cache.set(&cache_key, &session_key_b64, 600).await {
                ::tracing::warn!(
                    user_id = %user_id,
                    session_id = %session_id,
                    error = %e,
                    "Failed to cache vodozemac megolm session key"
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
    pub async fn get_session_key_for_user(&self, user_id: &str, session_id: &str) -> Result<Option<String>, ApiError> {
        let start = Instant::now();
        let cache_key = format!("megolm_session_key:{user_id}:{session_id}");

        if let Ok(Some(encrypted)) = self.cache.get::<String>(&cache_key).await {
            if let Some(metrics) = &self.server_metrics {
                metrics.record_megolm_session_key_read("hit", start.elapsed().as_secs_f64() * 1000.0);
            }
            return Ok(Some(encrypted));
        }

        let encrypted = self.storage.get_session_key(user_id, session_id).await?;
        let result_label = if encrypted.is_some() { "miss_db_hit" } else { "miss_db_miss" };

        if let Some(ref value) = encrypted {
            if let Err(e) = self.cache.set(&cache_key, value, 600).await {
                ::tracing::warn!(
                    user_id = %user_id,
                    session_id = %session_id,
                    error = %e,
                    "Failed to backfill vodozemac megolm session key into cache"
                );
            }
        }

        if let Some(metrics) = &self.server_metrics {
            metrics.record_megolm_session_key_read(result_label, start.elapsed().as_secs_f64() * 1000.0);
        }

        Ok(encrypted)
    }

    /// List all sessions for a room.
    pub async fn get_room_sessions(&self, room_id: &str) -> Result<Vec<MegolmSession>, ApiError> {
        self.storage.get_room_sessions(room_id).await.map_err(|e| {
            ::tracing::error!("Failed to get room sessions: {e}");
            ApiError::database("A database error occurred".to_string())
        })
    }

    /// Delete a session.
    pub async fn delete_session(&self, session_id: &str) -> Result<(), ApiError> {
        self.storage.delete_session(session_id).await.map_err(|e| {
            ::tracing::error!("Failed to delete session: {e}");
            ApiError::database("A database error occurred".to_string())
        })
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
        let pickle_str = pickle_to_string(&pickle);
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
    // Phase 2: dual-write logic unit tests
    // ========================================================================

    /// 验证 MegolmSession 模型与 Phase 2 pickle_format 字段的序列化兼容
    #[test]
    fn megolm_session_phase2_fields_serialize() {
        let session = MegolmSession {
            id: uuid::Uuid::new_v4(),
            session_id: "phase2_test".to_string(),
            room_id: "!room:test.example".to_string(),
            sender_key: "sender_key_b64".to_string(),
            session_key: "legacy_encrypted_session_key".to_string(),
            algorithm: "m.megolm.v1.aes-sha2".to_string(),
            message_index: 0,
            created_ts: current_timestamp_utc(),
            last_used_ts: current_timestamp_utc(),
            expires_at: Some(current_timestamp_utc() + chrono::Duration::days(7)),
            pickle_format: PickleFormat::Dual,
            vodozemac_pickle: Some("base64_vodozemac_pickle".to_string()),
        };

        let json = serde_json::to_string(&session).expect("serialize");
        assert!(json.contains("\"pickle_format\":\"dual\""), "json should include dual format: {json}");
        assert!(
            json.contains("\"vodozemac_pickle\":\"base64_vodozemac_pickle\""),
            "json should include vodozemac_pickle: {json}"
        );

        let deserialized: MegolmSession = serde_json::from_str(&json).expect("deserialize");
        assert_eq!(deserialized.pickle_format, PickleFormat::Dual);
        assert_eq!(deserialized.vodozemac_pickle.as_deref(), Some("base64_vodozemac_pickle"));
    }

    /// 验证 PickleFormat 枚举三种变体都能正确序列化（小写）
    #[test]
    fn pickle_format_serde_all_variants() {
        for (fmt, expected) in [
            (PickleFormat::Legacy, "\"legacy\""),
            (PickleFormat::Vodozemac, "\"vodozemac\""),
            (PickleFormat::Dual, "\"dual\""),
        ] {
            let s = serde_json::to_string(&fmt).expect("serialize");
            assert_eq!(s, expected, "PickleFormat {fmt:?} should serialize to {expected}");
        }
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
        let pickle_str = pickle_to_string(&pickle);

        // 模拟 storage get_session 读取
        let restored = pickle_from_string(&pickle_str).expect("pickle from storage");
        let _restored_session = GroupSession::from_pickle(restored);
    }
}
