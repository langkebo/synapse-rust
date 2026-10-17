// Key Rotation Service
// E2EE Phase 2: Automatic key rotation for enhanced security

use crate::megolm::{MegolmProvider, MegolmSession};
use async_trait::async_trait;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use synapse_common::current_timestamp_millis;
use synapse_common::map_database;
use synapse_common::ApiError;

/// The `KeyRotationService` type.
pub struct KeyRotationService {
    megolm_service: Arc<MegolmProvider>,
    storage: Arc<KeyRotationStorage>,
    config: Arc<tokio::sync::RwLock<KeyRotationConfig>>,
}

const DEFAULT_OLM_ROTATION_DAYS: i64 = 7;
const DEFAULT_MEGOLM_ROTATION_MESSAGES: i64 = 100;
const DEFAULT_MAX_SESSION_AGE_DAYS: i64 = 90;

/// M-04: make the persisted rotation config the single source of truth for
/// megolm session lifetime. `vodozemac_megolm` reads this when stamping
/// `expires_at` on a new session; the env var only covers the window before
/// the config has ever been loaded.
fn apply_session_max_age(config: &KeyRotationConfig) {
    crate::vodozemac_megolm::set_session_max_age_days(config.max_session_age_days);
}

#[derive(Clone, Debug)]
/// The `KeyRotationConfig` type.
pub struct KeyRotationConfig {
    /// The `olm_rotation_days` field.
    /// The `megolm_rotation_messages` field.
    /// The `max_session_age_days` field.
    /// The `enable_auto_rotation` field.
    pub olm_rotation_days: i64,
    /// The `megolm_rotation_messages` field.
    /// The `max_session_age_days` field.
    /// The `enable_auto_rotation` field.
    pub megolm_rotation_messages: i64,
    /// The `max_session_age_days` field.
    /// The `enable_auto_rotation` field.
    pub max_session_age_days: i64,
    /// The `enable_auto_rotation` field.
    pub enable_auto_rotation: bool,
}

/// Default implementation for [`KeyRotationConfig`].
impl Default for KeyRotationConfig {
    fn default() -> Self {
        Self {
            olm_rotation_days: DEFAULT_OLM_ROTATION_DAYS,
            megolm_rotation_messages: DEFAULT_MEGOLM_ROTATION_MESSAGES,
            max_session_age_days: DEFAULT_MAX_SESSION_AGE_DAYS,
            enable_auto_rotation: true,
        }
    }
}

/// Implementation of [`KeyRotationConfig`] methods.
impl KeyRotationConfig {
    /// See [`load_from_storage`].
    pub async fn load_from_storage(storage: &KeyRotationStorage) -> Result<Self, ApiError> {
        let olm_rotation_days: i64 =
            storage.get_rotation_config("olm_rotation_days").await?.and_then(|v| v.parse().ok()).unwrap_or_else(|| {
                tracing::warn!("Failed to parse olm_rotation_days, using default");
                DEFAULT_OLM_ROTATION_DAYS
            });

        let megolm_rotation_messages: i64 =
            storage.get_rotation_config("megolm_rotation_messages").await?.and_then(|v| v.parse().ok()).unwrap_or_else(
                || {
                    tracing::warn!("Failed to parse megolm_rotation_messages, using default");
                    DEFAULT_MEGOLM_ROTATION_MESSAGES
                },
            );

        let max_session_age_days: i64 =
            storage.get_rotation_config("max_session_age_days").await?.and_then(|v| v.parse().ok()).unwrap_or_else(
                || {
                    tracing::warn!("Failed to parse max_session_age_days, using default");
                    DEFAULT_MAX_SESSION_AGE_DAYS
                },
            );

        let enable_auto_rotation: bool =
            storage.get_rotation_config("enable_auto_rotation").await?.and_then(|v| v.parse().ok()).unwrap_or_else(
                || {
                    tracing::warn!("Failed to parse enable_auto_rotation, using default");
                    true
                },
            );

        Ok(Self { olm_rotation_days, megolm_rotation_messages, max_session_age_days, enable_auto_rotation })
    }

    /// See [`persist_to_storage`].
    pub async fn persist_to_storage(&self, storage: &KeyRotationStorage) -> Result<(), ApiError> {
        storage.set_rotation_config("olm_rotation_days", &self.olm_rotation_days.to_string()).await?;
        storage.set_rotation_config("megolm_rotation_messages", &self.megolm_rotation_messages.to_string()).await?;
        storage.set_rotation_config("max_session_age_days", &self.max_session_age_days.to_string()).await?;
        storage.set_rotation_config("enable_auto_rotation", &self.enable_auto_rotation.to_string()).await?;
        Ok(())
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
/// The `KeyRotationLog` type.
pub struct KeyRotationLog {
    /// The `id` field.
    /// The `user_id` field.
    /// The `device_id` field.
    /// The `room_id` field.
    /// The `rotation_type` field.
    /// The `old_key_id` field.
    /// The `new_key_id` field.
    /// The `reason` field.
    /// The `rotated_at` field.
    pub id: i64,
    /// The `user_id` field.
    /// The `device_id` field.
    /// The `room_id` field.
    /// The `rotation_type` field.
    /// The `old_key_id` field.
    /// The `new_key_id` field.
    /// The `reason` field.
    /// The `rotated_at` field.
    pub user_id: String,
    /// The `device_id` field.
    /// The `room_id` field.
    /// The `rotation_type` field.
    /// The `old_key_id` field.
    /// The `new_key_id` field.
    /// The `reason` field.
    /// The `rotated_at` field.
    pub device_id: String,
    /// The `room_id` field.
    /// The `rotation_type` field.
    /// The `old_key_id` field.
    /// The `new_key_id` field.
    /// The `reason` field.
    /// The `rotated_at` field.
    pub room_id: Option<String>,
    /// The `rotation_type` field.
    /// The `old_key_id` field.
    /// The `new_key_id` field.
    /// The `reason` field.
    /// The `rotated_at` field.
    pub rotation_type: String,
    /// The `old_key_id` field.
    /// The `new_key_id` field.
    /// The `reason` field.
    /// The `rotated_at` field.
    pub old_key_id: Option<String>,
    /// The `new_key_id` field.
    /// The `reason` field.
    /// The `rotated_at` field.
    pub new_key_id: String,
    /// The `reason` field.
    /// The `rotated_at` field.
    pub reason: Option<String>,
    /// The `rotated_at` field.
    pub rotated_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
/// The `RotationStatus` type.
pub struct RotationStatus {
    /// The `total_sessions` field.
    /// The `rotated_sessions` field.
    /// The `last_rotation` field.
    pub total_sessions: i64,
    /// The `rotated_sessions` field.
    /// The `last_rotation` field.
    pub rotated_sessions: i64,
    /// The `last_rotation` field.
    pub last_rotation: Option<DateTime<Utc>>,
}

/// `get_rotation_status` 的行结构：`last_rotation` 在 SQL 里由 BIGINT 毫秒转成
/// timestamptz（D-44），因此这里用 `DateTime<Utc>` 而不是 `i64`。
#[derive(sqlx::FromRow)]
struct RotationStatusRow {
    total_sessions: i64,
    rotated_sessions: i64,
    last_rotation: Option<chrono::DateTime<chrono::Utc>>,
}

/// 一天的毫秒数 —— 把 `olm_rotation_days` 折算成 `list_rotatable_sessions` 需要的时间戳边界。
const MS_PER_DAY: i64 = 86_400_000;

/// 一条**服务端自有**、已达轮换阈值的 megolm 会话（`megolm_sessions` 的一行）。
///
/// **语义约定（有守卫钉住）**：`sender_key` 在本仓**唯一**的生产写入路径下就等于归属
/// **user id** —— `KeyRotationService::rotate_megolm_session(room_id, user_id, device_id)`
/// 把 `user_id` 当 `sender_key` 传给 `create_session(room_id, sender_key)`。因此本结构可以
/// 直接喂回 `rotate_megolm_session`，并由 `log_rotation` 记成该用户的轮换。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RotatableSession {
    /// The `room_id` field.
    pub room_id: String,
    /// `megolm_sessions.sender_key` —— 见类型文档：等于归属 user id。
    pub sender_key: String,
}

/// 一次轮换扫描的结果（定时执行器每轮返回一份，用于日志与测试断言）。
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct RotationSweep {
    /// 命中的候选会话数。
    pub candidates: usize,
    /// 实际完成轮换的会话数。
    pub rotated: usize,
    /// 轮换失败的会话数（逐个 best-effort，失败不中断整轮）。
    pub failed: usize,
}

/// Implementation of [`KeyRotationService`] methods.
impl KeyRotationService {
    /// See [`new`].
    pub fn new(
        megolm_service: Arc<MegolmProvider>,
        storage: Arc<KeyRotationStorage>,
        config: KeyRotationConfig,
    ) -> Self {
        apply_session_max_age(&config);
        Self { megolm_service, storage, config: Arc::new(tokio::sync::RwLock::new(config)) }
    }

    /// See [`new_with_db_config`].
    pub async fn new_with_db_config(
        megolm_service: Arc<MegolmProvider>,
        storage: Arc<KeyRotationStorage>,
        fallback_config: KeyRotationConfig,
    ) -> Self {
        let config = KeyRotationConfig::load_from_storage(&storage).await.unwrap_or_else(|e| {
            tracing::warn!("Failed to load KeyRotationConfig from database, using defaults: {e}");
            fallback_config
        });

        apply_session_max_age(&config);
        Self { megolm_service, storage, config: Arc::new(tokio::sync::RwLock::new(config)) }
    }

    /// See [`reload_config`].
    pub async fn reload_config(&self) -> Result<(), ApiError> {
        let new_config = KeyRotationConfig::load_from_storage(&self.storage).await?;
        apply_session_max_age(&new_config);
        *self.config.write().await = new_config;
        Ok(())
    }

    /// See [`update_config`].
    pub async fn update_config(&self, new_config: KeyRotationConfig) -> Result<(), ApiError> {
        new_config.persist_to_storage(&self.storage).await?;
        apply_session_max_age(&new_config);
        *self.config.write().await = new_config;
        Ok(())
    }

    /// See [`get_config`].
    pub async fn get_config(&self) -> KeyRotationConfig {
        self.config.read().await.clone()
    }

    /// See [`should_rotate`].
    pub async fn should_rotate(&self, session: &MegolmSession) -> Result<bool, ApiError> {
        let config = self.config.read().await;
        let age_days = (Utc::now() - session.last_used_ts).num_days();
        if age_days >= config.olm_rotation_days {
            return Ok(true);
        }

        if session.message_index >= config.megolm_rotation_messages {
            return Ok(true);
        }

        if let Some(expires_at) = session.expires_at {
            if Utc::now() >= expires_at {
                return Ok(true);
            }
        }

        // Reaching this point is the **normal** outcome: no trigger fired. A
        // `warn!` claiming "timestamp conversion failed or expired_at is invalid"
        // used to sit here, which meant it fired for every healthy session — log
        // spam plus a false diagnosis. Timestamp conversion happens exactly once,
        // at the row→model boundary (`megolm/storage.rs`), and reports its own
        // failures there, so this path has nothing to say.
        Ok(false)
    }

    /// See [`rotate_megolm_session`].
    ///
    /// `device_id` is required because a rotation is performed **by a device**: the
    /// audit row it writes is keyed by one (`key_rotation_log.device_id` is
    /// `NOT NULL`) and the only reader, `get_device_rotation_history` — behind
    /// `GET /_matrix/vendor/v1/keys/rotation/history/{device_id}` — filters on it.
    pub async fn rotate_megolm_session(
        &self,
        room_id: &str,
        user_id: &str,
        device_id: &str,
    ) -> Result<MegolmSession, ApiError> {
        let new_session = self.megolm_service.create_session(room_id, user_id).await?;

        self.storage.log_rotation(user_id, device_id, room_id, "megolm").await?;

        self.share_new_key(room_id, &new_session, user_id).await?;

        self.mark_session_as_rotated(room_id, user_id).await?;

        Ok(new_session)
    }

    /// See [`rotate_all_user_sessions`].
    pub async fn rotate_all_user_sessions(&self, user_id: &str, device_id: &str) -> Result<Vec<String>, ApiError> {
        let mut rotated_rooms = Vec::new();

        let rooms = self.storage.get_encrypted_rooms(user_id).await?;

        for room_id in rooms {
            if self.should_rotate_for_room(user_id, &room_id).await? {
                self.rotate_megolm_session(&room_id, user_id, device_id).await?;
                rotated_rooms.push(room_id);
            }
        }

        Ok(rotated_rooms)
    }

    /// See [`cleanup_expired_sessions`].
    pub async fn cleanup_expired_sessions(&self) -> Result<i64, ApiError> {
        let count = self.storage.delete_expired_sessions().await?;
        Ok(count)
    }

    /// See [`get_rotation_status`].
    pub async fn get_rotation_status(&self, user_id: &str) -> Result<RotationStatus, ApiError> {
        self.storage.get_rotation_status(user_id).await
    }

    /// 对**服务端自有**的 megolm 会话做一次轮换扫描 —— 定时执行器的入口。
    ///
    /// `device_id` 是"服务器作为一台设备"的身份，来自 `server.megolm_device_id`：
    /// `key_rotation_log.device_id` 是 `NOT NULL`，而本仓拒绝写 `unknown` 这类合成值
    /// （审计 §11.15.3），所以调用方**必须显式提供**。
    ///
    /// `enable_auto_rotation == false` 时**直接返回、不查库** —— 这是
    /// [`KeyRotationConfig::enable_auto_rotation`] 的第一个也是唯一读者（在本轮之前它是个死旋钮）。
    ///
    /// 逐个 best-effort：单个会话轮换失败只记 `warn!` 并计入 `failed`，不中断整轮
    /// （中断会让后面的房间被整轮跳过，而调度器下轮还要等一个周期）。
    pub async fn sweep_rotations(&self, device_id: &str) -> Result<RotationSweep, ApiError> {
        let config = self.config.read().await.clone();
        if !config.enable_auto_rotation {
            return Ok(RotationSweep::default());
        }

        let now_ms = current_timestamp_millis();
        let max_idle_ms = config.olm_rotation_days.saturating_mul(MS_PER_DAY);
        let candidates =
            self.storage.list_rotatable_sessions(config.megolm_rotation_messages, max_idle_ms, now_ms).await?;

        let mut sweep = RotationSweep { candidates: candidates.len(), rotated: 0, failed: 0 };
        for candidate in &candidates {
            match self.rotate_megolm_session(&candidate.room_id, &candidate.sender_key, device_id).await {
                Ok(_) => sweep.rotated += 1,
                Err(error) => {
                    sweep.failed += 1;
                    tracing::warn!(
                        room_id = %candidate.room_id,
                        owner = %candidate.sender_key,
                        %error,
                        "Server-side megolm rotation failed for this session; continuing the sweep"
                    );
                }
            }
        }

        if sweep.rotated > 0 || sweep.failed > 0 {
            tracing::info!(
                candidates = sweep.candidates,
                rotated = sweep.rotated,
                failed = sweep.failed,
                %device_id,
                "Server-side megolm rotation sweep finished"
            );
        }

        Ok(sweep)
    }

    async fn share_new_key(
        &self,
        room_id: &str,
        session: &MegolmSession,
        recipient_user_id: &str,
    ) -> Result<(), ApiError> {
        tracing::info!("Sharing new megolm key for room {}, session {}", room_id, session.session_id);
        self.storage
            .record_key_share(room_id, &session.session_id, recipient_user_id, "rotated")
            .await
            .map_err(map_database!("Failed to record key share for rotation"))
    }

    async fn mark_session_as_rotated(&self, room_id: &str, user_id: &str) -> Result<(), ApiError> {
        tracing::info!("Marking session as rotated for user {} in room {}", user_id, room_id);
        self.storage.mark_rotated(user_id, room_id).await.map_err(map_database!("Failed to mark session as rotated"))
    }

    async fn should_rotate_for_room(&self, user_id: &str, room_id: &str) -> Result<bool, ApiError> {
        let sessions = self.megolm_service.get_room_sessions(room_id).await?;
        for session in &sessions {
            if self.should_rotate(session).await? {
                return Ok(true);
            }
        }
        let needs_rotation = self.storage.check_needs_rotation(user_id, room_id).await?;
        Ok(needs_rotation)
    }

    /// See [`notify_member_left_encrypted_room`].
    pub async fn notify_member_left_encrypted_room(
        &self,
        room_id: &str,
        leaving_user_id: &str,
    ) -> Result<Vec<String>, ApiError> {
        let remaining_members = self.storage.get_encrypted_room_members(room_id).await?;

        let remaining: Vec<String> = remaining_members.into_iter().filter(|uid| uid != leaving_user_id).collect();

        self.storage.mark_key_rotation_needed(room_id, leaving_user_id).await?;

        tracing::info!(
            "Marked key rotation needed for room {} after user {} left ({} remaining members)",
            room_id,
            leaving_user_id,
            remaining.len()
        );

        Ok(remaining)
    }

    /// See [`forward_keys_for_new_member`].
    pub async fn forward_keys_for_new_member(&self, room_id: &str, new_user_id: &str) -> Result<(), ApiError> {
        let sessions = self.megolm_service.get_room_sessions(room_id).await?;

        for session in &sessions {
            // E-07: dedup at per-recipient granularity. A row exists in
            // `megolm_key_shares` only if we already shipped this session's
            // key to `new_user_id`. Repeat forwards (e.g. re-join) are
            // suppressed per-recipient, while other members remain unaffected.
            let already_shared = self
                .storage
                .key_share_exists(room_id, &session.session_id, new_user_id)
                .await
                .map_err(map_database!("Failed to check existing key shares"))?;
            if already_shared {
                tracing::debug!(
                    "Skipping duplicate key forward for room {room_id} session {} user {new_user_id}: \
                     already shared (E-07 dedup)",
                    session.session_id
                );
                continue;
            }

            self.megolm_service.share_session(&session.session_id, &[new_user_id.to_string()]).await?;

            self.storage
                .record_key_share(room_id, &session.session_id, new_user_id, "new_member")
                .await
                .map_err(map_database!("Failed to record key share for new member"))?;
        }

        tracing::info!("Forwarded {} session keys to new member {} in room {}", sessions.len(), new_user_id, room_id);

        Ok(())
    }

    /// See [`get_rooms_needing_key_rotation`].
    pub async fn get_rooms_needing_key_rotation(&self, user_id: &str) -> Result<Vec<String>, ApiError> {
        self.storage.get_rooms_needing_key_rotation(user_id).await
    }
}

#[async_trait]
/// The `KeyRotationStorageApi` trait.
pub trait KeyRotationStorageApi: Send + Sync {
    /// Get the last key-rotation timestamp for a user (millis since epoch).
    async fn get_user_last_rotation_ts(&self, user_id: &str) -> Result<Option<i64>, ApiError>;
    /// Get the rotation history for a single device.
    async fn get_device_rotation_history(
        &self,
        user_id: &str,
        device_id: &str,
    ) -> Result<Vec<(Option<String>, Option<i64>)>, ApiError>;
    /// Set a rotation configuration value.
    async fn set_rotation_config(&self, key: &str, value: &str) -> Result<(), ApiError>;
    /// Get a rotation configuration value.
    async fn get_rotation_config(&self, key: &str) -> Result<Option<String>, ApiError>;
    /// Get the last rotation timestamp for a specific key.
    async fn get_last_rotation_for_key(&self, user_id: &str, key_id: &str) -> Result<Option<i64>, ApiError>;
    /// Get the latest rotation timestamp across all keys for a user.
    async fn get_max_rotation_ts(&self, user_id: &str) -> Result<i64, ApiError>;
    /// Mark a key rotation as needed when a user leaves a room.
    async fn mark_key_rotation_needed(&self, room_id: &str, leaving_user_id: &str) -> Result<(), ApiError>;
}

#[derive(Clone)]
/// The `KeyRotationStorage` type.
pub struct KeyRotationStorage {
    pool: Arc<sqlx::PgPool>,
}

/// Implementation of [`KeyRotationStorage`] methods.
impl KeyRotationStorage {
    /// See [`new`].
    pub fn new(pool: Arc<sqlx::PgPool>) -> Self {
        Self { pool }
    }

    /// See [`log_rotation`].
    ///
    /// `device_id` is **required**, not `Option`: `key_rotation_log.device_id` is
    /// `NOT NULL`, so a caller binding SQL NULL aborts the whole rotation with
    /// `null value in column "device_id" violates not-null constraint`. That is
    /// exactly what this used to do (`None::<String>`), and it went unnoticed
    /// because the only caller, `rotate_megolm_session`, is not wired into
    /// production. A mandatory parameter removes the failure by construction
    /// rather than papering over it with a placeholder id — a synthetic id would
    /// also be indistinguishable from a real one in the audit trail.
    pub async fn log_rotation(
        &self,
        user_id: &str,
        device_id: &str,
        room_id: &str,
        rotation_type: &str,
    ) -> Result<(), ApiError> {
        let now = current_timestamp_millis();
        let new_key_id = uuid::Uuid::new_v4().to_string();

        tracing::debug!(
            user_id = %user_id,
            device_id = %device_id,
            room_id = %room_id,
            rotation_type = %rotation_type,
            "Logging key rotation"
        );

        sqlx::query!(
            "INSERT INTO key_rotation_log
             (user_id, device_id, room_id, rotation_type, old_key_id, new_key_id, reason, rotated_at)
             VALUES ($1, $2, $3, $4, $5, $6, $7, $8)",
            user_id,
            device_id,
            room_id,
            rotation_type,
            None::<String>,
            &new_key_id,
            None::<String>,
            now,
        )
        .execute(&*self.pool)
        .await
        .map_err(map_database!("log_rotation"))?;

        Ok(())
    }

    /// See [`get_encrypted_rooms`].
    pub async fn get_encrypted_rooms(&self, user_id: &str) -> Result<Vec<String>, ApiError> {
        let rows = sqlx::query_scalar!(
            r"
            SELECT DISTINCT r.room_id
            FROM rooms r
            INNER JOIN room_memberships rm ON r.room_id = rm.room_id
            INNER JOIN events e ON r.room_id = e.room_id
            WHERE rm.user_id = $1
              AND rm.membership = 'join'
              AND e.event_type = 'm.room.encryption'
              AND e.state_key IS NOT NULL
            ",
            user_id
        )
        .fetch_all(&*self.pool)
        .await
        .map_err(map_database!("get_encrypted_rooms"))?;

        Ok(rows)
    }

    /// See [`record_key_share`].
    pub async fn record_key_share(
        &self,
        room_id: &str,
        session_id: &str,
        recipient_user_id: &str,
        share_reason: &str,
    ) -> Result<(), ApiError> {
        let now = current_timestamp_millis();
        sqlx::query!(
            r"
            INSERT INTO megolm_key_shares (room_id, session_id, recipient_user_id, share_reason, shared_at)
            VALUES ($1, $2, $3, $4, $5)
            ON CONFLICT (room_id, session_id, recipient_user_id)
            DO UPDATE SET share_reason = $4, shared_at = $5
            ",
            room_id,
            session_id,
            recipient_user_id,
            share_reason,
            now,
        )
        .execute(&*self.pool)
        .await
        .map_err(map_database!("record_key_share"))?;

        Ok(())
    }

    /// E-07 dedup helper. Returns `true` if a `(room_id, session_id, recipient_user_id)`
    /// row already exists in `megolm_key_shares`. Used by
    /// `KeyRotationService::forward_keys_for_new_member` to suppress
    /// duplicate to-device `m.room_key` messages for the same recipient.
    pub async fn key_share_exists(
        &self,
        room_id: &str,
        session_id: &str,
        recipient_user_id: &str,
    ) -> Result<bool, ApiError> {
        // `query_scalar!` 只取单列、不需要给列起名，因此 `SELECT 1` 不再触发
        // "`?column?` is not a valid Rust identifier"（那是 `query!` 的问题）。
        let row = sqlx::query_scalar!(
            r"
            SELECT 1
            FROM megolm_key_shares
            WHERE room_id = $1 AND session_id = $2 AND recipient_user_id = $3
            LIMIT 1
            ",
            room_id,
            session_id,
            recipient_user_id,
        )
        .fetch_optional(&*self.pool)
        .await
        .map_err(map_database!("key_share_exists"))?;

        Ok(row.is_some())
    }

    /// See [`mark_rotated`].
    pub async fn mark_rotated(&self, user_id: &str, room_id: &str) -> Result<(), ApiError> {
        let now = current_timestamp_millis();
        // D-43: 这里曾写 `rotation_count` / `last_rotation_ts` 两列 —— 它们在
        // `key_rotation_state`（`PRIMARY KEY (user_id, room_id)`）里**都不存在**（表里是
        // `is_rotated BOOLEAN` + `rotated_at BIGINT`），所以这条 INSERT 在真 schema 下
        // 必然 42703；`rotation_count` 全仓也没有任何读取方（写-only 死数据），
        // 故按既有列重写而不是加列。
        sqlx::query!(
            r"
            INSERT INTO key_rotation_state (user_id, room_id, is_rotated, rotated_at)
            VALUES ($1, $2, TRUE, $3)
            ON CONFLICT (user_id, room_id) DO UPDATE SET
                is_rotated = TRUE,
                rotated_at = EXCLUDED.rotated_at
            ",
            user_id,
            room_id,
            now,
        )
        .execute(&*self.pool)
        .await
        .map_err(map_database!("mark_rotated"))?;

        Ok(())
    }

    /// See [`check_needs_rotation`].
    pub async fn check_needs_rotation(&self, user_id: &str, room_id: &str) -> Result<bool, ApiError> {
        // D-43（同族第二处）：这里同样引用了不存在的 `rotation_count`，真列是
        // `is_rotated BOOLEAN`；`COALESCE(bool, 0) > 0` 也是无意义表达式。
        let row = sqlx::query_scalar!(
            r"
            SELECT is_rotated FROM key_rotation_state
            WHERE user_id = $1 AND room_id = $2
            ",
            user_id,
            room_id
        )
        .fetch_optional(&*self.pool)
        .await
        .map_err(map_database!("check_needs_rotation"))?;

        Ok(row.is_none_or(|is_rotated| !is_rotated))
    }

    /// See [`delete_expired_sessions`].
    pub async fn delete_expired_sessions(&self) -> Result<i64, ApiError> {
        let result =
            sqlx::query!("DELETE FROM megolm_sessions WHERE expires_at < (EXTRACT(EPOCH FROM NOW())::BIGINT * 1000)",)
                .execute(&*self.pool)
                .await
                .map_err(map_database!("delete_expired_sessions"))?;

        Ok(result.rows_affected() as i64)
    }

    /// 列出**需要轮换**的服务端自有 megolm 会话。
    ///
    /// 判据与 [`KeyRotationService::should_rotate`] **同源**（三个阈值：消息数上限、
    /// 会话最长寿命、空闲时长上限），但在 SQL 里先做粗筛：`megolm_sessions` 可能很大，
    /// 而 `should_rotate` 要先把行 rehydrate 成 `MegolmSession`（含 at-rest 解封）——
    /// 为筛掉绝大多数不该动的行而解封每一行是纯浪费。两处实现的一致性由
    /// `key_rotation/service/tests.rs` 的**一致性守卫**钉住（同一策略、两个站点、且被判据约束）。
    ///
    /// **每个 `(room_id, sender_key)` 只看最新那一行**（`created_ts` 最新、`session_id` 破平）：
    /// 轮换只**新增**一行而**不删**旧行（旧行要留着解历史消息），所以若不过滤，旧行会永远
    /// 满足"已过期" ⇒ 每轮都再轮换一次 ⇒ 会话行数无界增长。
    ///
    /// `last_used_ts` 允许为 NULL；row→model 层把它回退成 `created_ts`（`megolm/storage.rs`），
    /// 故这里的 `COALESCE` 是同一口径，不是第二套规则。
    pub async fn list_rotatable_sessions(
        &self,
        max_messages: i64,
        max_idle_ms: i64,
        now_ms: i64,
    ) -> Result<Vec<RotatableSession>, ApiError> {
        let idle_cutoff_ms = now_ms.saturating_sub(max_idle_ms);
        let rows = sqlx::query!(
            r"
            SELECT latest.room_id, latest.sender_key
            FROM (
                SELECT DISTINCT ON (room_id, sender_key)
                       room_id, sender_key, message_index, created_ts, last_used_ts, expires_at
                FROM megolm_sessions
                ORDER BY room_id, sender_key, created_ts DESC, session_id DESC
            ) AS latest
            WHERE latest.message_index >= $1
               OR (latest.expires_at IS NOT NULL AND latest.expires_at <= $3)
               OR (COALESCE(latest.last_used_ts, latest.created_ts) <= $2)
            ORDER BY latest.room_id, latest.sender_key
            ",
            max_messages,
            idle_cutoff_ms,
            now_ms,
        )
        .fetch_all(&*self.pool)
        .await
        .map_err(map_database!("list_rotatable_sessions"))?;

        Ok(rows.into_iter().map(|row| RotatableSession { room_id: row.room_id, sender_key: row.sender_key }).collect())
    }

    /// See [`get_rotation_status`].
    pub async fn get_rotation_status(&self, user_id: &str) -> Result<RotationStatus, ApiError> {
        let seven_days_ago_ms = current_timestamp_millis() - 7 * 24 * 3600 * 1000;
        // D-44: 同 D-43 —— `last_rotation_ts` 不存在，真列名是 `rotated_at`。
        // 该列是 BIGINT 毫秒，而 `RotationStatus.last_rotation` 是 `DateTime<Utc>`，
        // 因此在 SQL 里显式转成 timestamptz（否则是第二处类型不符）。
        let row = sqlx::query_as!(
            RotationStatusRow,
            r#"
            SELECT
             COUNT(*) AS "total_sessions!",
             COUNT(CASE WHEN rotated_at > $2 THEN 1 END) AS "rotated_sessions!",
             to_timestamp(MAX(rotated_at)::double precision / 1000.0) AS "last_rotation"
            FROM key_rotation_state
            WHERE user_id = $1
            "#,
            user_id,
            seven_days_ago_ms
        )
        .fetch_one(&*self.pool)
        .await
        .map_err(map_database!("get_rotation_status"))?;

        Ok(RotationStatus {
            total_sessions: row.total_sessions,
            rotated_sessions: row.rotated_sessions,
            last_rotation: row.last_rotation,
        })
    }

    /// See [`get_encrypted_room_members`].
    pub async fn get_encrypted_room_members(&self, room_id: &str) -> Result<Vec<String>, ApiError> {
        let rows = sqlx::query_scalar!(
            r"
            SELECT rm.user_id
            FROM room_memberships rm
            INNER JOIN events e ON rm.room_id = e.room_id
            WHERE rm.room_id = $1
              AND rm.membership = 'join'
              AND e.event_type = 'm.room.encryption'
              AND e.state_key IS NOT NULL
            ",
            room_id
        )
        .fetch_all(&*self.pool)
        .await
        .map_err(map_database!("get_encrypted_room_members"))?;

        Ok(rows)
    }

    /// See [`get_rooms_needing_key_rotation`].
    pub async fn get_rooms_needing_key_rotation(&self, user_id: &str) -> Result<Vec<String>, ApiError> {
        let rows = sqlx::query_scalar!(
            r"
            SELECT DISTINCT krp.room_id
            FROM key_rotation_pending krp
            INNER JOIN room_memberships rm ON krp.room_id = rm.room_id
            WHERE rm.user_id = $1
              AND rm.membership = 'join'
            ",
            user_id
        )
        .fetch_all(&*self.pool)
        .await
        .map_err(map_database!("get_rooms_needing_key_rotation"))?;

        Ok(rows)
    }

    /// See [`clear_key_rotation_needed`].
    pub async fn clear_key_rotation_needed(&self, room_id: &str) -> Result<(), ApiError> {
        sqlx::query!(
            r"
            DELETE FROM key_rotation_pending WHERE room_id = $1
            ",
            room_id,
        )
        .execute(&*self.pool)
        .await
        .map_err(map_database!("clear_key_rotation_needed"))?;

        Ok(())
    }

    /// Get the timestamp of the most recent rotation for a user.
    pub async fn get_user_last_rotation_ts(&self, user_id: &str) -> Result<Option<i64>, ApiError> {
        let result: Option<i64> =
            sqlx::query_scalar!(r#"SELECT MAX(rotated_at) AS "max" FROM key_rotation_log WHERE user_id = $1"#, user_id)
                .fetch_one(&*self.pool)
                .await
                .map_err(map_database!("Failed to query key rotation log"))?;

        Ok(result)
    }

    /// Get the rotation history for a specific user and device, limited to the
    /// most recent 10 entries.
    pub async fn get_device_rotation_history(
        &self,
        user_id: &str,
        device_id: &str,
    ) -> Result<Vec<(Option<String>, Option<i64>)>, ApiError> {
        let rows = sqlx::query!(
            r"
            SELECT new_key_id AS key_id, rotated_at AS rotated_ts
            FROM key_rotation_log
            WHERE user_id = $1 AND device_id = $2
            ORDER BY rotated_at DESC
            LIMIT 10
            ",
            user_id,
            device_id,
        )
        .fetch_all(&*self.pool)
        .await
        .map_err(map_database!("Failed to get rotation history"))?;

        Ok(rows.into_iter().map(|row| (row.key_id, Some(row.rotated_ts))).collect())
    }

    /// Get the last rotation timestamp for a specific key id.
    ///
    /// `rotated_at` is stored as a BIGINT millisecond timestamp, so it is
    /// returned as-is (no `EXTRACT(EPOCH ...)` conversion, which would fail
    /// with `function extract(unknown, bigint) does not exist`).
    pub async fn get_last_rotation_for_key(&self, user_id: &str, key_id: &str) -> Result<Option<i64>, ApiError> {
        let result: Option<i64> = sqlx::query_scalar!(
            r#"
            SELECT rotated_at
            FROM key_rotation_log
            WHERE user_id = $1 AND (new_key_id = $2 OR old_key_id = $2)
            ORDER BY rotated_at DESC LIMIT 1
            "#,
            user_id,
            key_id
        )
        .fetch_optional(&*self.pool)
        .await
        .map_err(map_database!("Failed to query rotation log by key_id"))?;

        Ok(result)
    }

    /// Get the maximum rotation timestamp for a user (returns 0 if no
    /// rotations exist).
    ///
    /// `rotated_at` is a BIGINT millisecond timestamp — no epoch conversion
    /// is applied (see `get_last_rotation_for_key`).
    pub async fn get_max_rotation_ts(&self, user_id: &str) -> Result<i64, ApiError> {
        // `COALESCE(..., 0)` 使 NULL 不可能，但宏按 `MAX(...)` 推断为可空 ⇒
        // 显式收口（用 `unwrap_or(0)` 而不是 `unwrap()`，本 crate 禁止后者）。
        let result: i64 = sqlx::query_scalar!(
            r#"
            SELECT COALESCE(MAX(rotated_at), 0) AS "last_rotation"
            FROM key_rotation_log
            WHERE user_id = $1
            "#,
            user_id
        )
        .fetch_one(&*self.pool)
        .await
        .map_err(map_database!("Failed to query rotation log"))?
        // `COALESCE(..., 0)` 保证非空，但宏按 `MAX(...)` 推断为可空 ⇒ 显式收口
        // （用 `unwrap_or(0)` 而不是 `unwrap()`，本 crate 禁止后者）。
        .unwrap_or(0);

        Ok(result)
    }

    /// Persist a key-value pair in the key_rotation_config table.
    pub async fn set_rotation_config(&self, key: &str, value: &str) -> Result<(), ApiError> {
        sqlx::query!(
            r"
            INSERT INTO key_rotation_config (key, value)
            VALUES ($1, $2)
            ON CONFLICT (key) DO UPDATE SET value = $2
            ",
            key,
            value,
        )
        .execute(&*self.pool)
        .await
        .map_err(map_database!("Failed to persist key rotation config"))?;

        Ok(())
    }

    /// Read a value from the key_rotation_config table.
    pub async fn get_rotation_config(&self, key: &str) -> Result<Option<String>, ApiError> {
        let result: Option<String> =
            sqlx::query_scalar!(r#"SELECT value FROM key_rotation_config WHERE key = $1"#, key)
                .fetch_optional(&*self.pool)
                .await
                .map_err(map_database!("Failed to query key rotation config"))?;

        Ok(result)
    }
}

#[async_trait]
/// Implementation of [`KeyRotationStorageApi`] methods.
impl KeyRotationStorageApi for KeyRotationStorage {
    async fn get_user_last_rotation_ts(&self, user_id: &str) -> Result<Option<i64>, ApiError> {
        self.get_user_last_rotation_ts(user_id).await
    }

    async fn get_device_rotation_history(
        &self,
        user_id: &str,
        device_id: &str,
    ) -> Result<Vec<(Option<String>, Option<i64>)>, ApiError> {
        self.get_device_rotation_history(user_id, device_id).await
    }

    async fn set_rotation_config(&self, key: &str, value: &str) -> Result<(), ApiError> {
        self.set_rotation_config(key, value).await
    }

    async fn get_rotation_config(&self, key: &str) -> Result<Option<String>, ApiError> {
        self.get_rotation_config(key).await
    }

    async fn get_last_rotation_for_key(&self, user_id: &str, key_id: &str) -> Result<Option<i64>, ApiError> {
        self.get_last_rotation_for_key(user_id, key_id).await
    }

    async fn get_max_rotation_ts(&self, user_id: &str) -> Result<i64, ApiError> {
        self.get_max_rotation_ts(user_id).await
    }

    async fn mark_key_rotation_needed(&self, room_id: &str, leaving_user_id: &str) -> Result<(), ApiError> {
        let now = current_timestamp_millis();
        sqlx::query!(
            r"
            INSERT INTO key_rotation_pending (room_id, reason, triggered_by_user_id, created_ts)
            VALUES ($1, 'member_left', $2, $3)
            ON CONFLICT (room_id, triggered_by_user_id) DO UPDATE SET created_ts = $3
            ",
            room_id,
            leaving_user_id,
            now,
        )
        .execute(&*self.pool)
        .await
        .map_err(map_database!("mark_key_rotation_needed"))?;

        Ok(())
    }
}

#[cfg(test)]
mod db_tests;
#[cfg(test)]
mod tests;
