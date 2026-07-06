// Key Rotation Service
// E2EE Phase 2: Automatic key rotation for enhanced security

use crate::megolm::{MegolmProvider, MegolmSession};
use crate::olm::OlmService;
use async_trait::async_trait;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use synapse_common::ApiError;

pub struct KeyRotationService {
    #[allow(dead_code)]
    olm_service: Arc<OlmService>,
    megolm_service: Arc<MegolmProvider>,
    storage: Arc<KeyRotationStorage>,
    config: Arc<tokio::sync::RwLock<KeyRotationConfig>>,
}

const DEFAULT_OLM_ROTATION_DAYS: i64 = 7;
const DEFAULT_MEGOLM_ROTATION_MESSAGES: i64 = 100;
const DEFAULT_MAX_SESSION_AGE_DAYS: i64 = 90;

#[derive(Clone, Debug)]
pub struct KeyRotationConfig {
    pub olm_rotation_days: i64,
    pub megolm_rotation_messages: i64,
    pub max_session_age_days: i64,
    pub enable_auto_rotation: bool,
}

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

impl KeyRotationConfig {
    pub async fn load_from_storage(storage: &KeyRotationStorage) -> Result<Self, ApiError> {
        let olm_rotation_days: i64 = storage
            .get_rotation_config("olm_rotation_days")
            .await?
            .and_then(|v| v.parse().ok())
            .unwrap_or(DEFAULT_OLM_ROTATION_DAYS);

        let megolm_rotation_messages: i64 = storage
            .get_rotation_config("megolm_rotation_messages")
            .await?
            .and_then(|v| v.parse().ok())
            .unwrap_or(DEFAULT_MEGOLM_ROTATION_MESSAGES);

        let max_session_age_days: i64 = storage
            .get_rotation_config("max_session_age_days")
            .await?
            .and_then(|v| v.parse().ok())
            .unwrap_or(DEFAULT_MAX_SESSION_AGE_DAYS);

        let enable_auto_rotation: bool =
            storage.get_rotation_config("enable_auto_rotation").await?.and_then(|v| v.parse().ok()).unwrap_or(true);

        Ok(Self { olm_rotation_days, megolm_rotation_messages, max_session_age_days, enable_auto_rotation })
    }

    pub async fn persist_to_storage(&self, storage: &KeyRotationStorage) -> Result<(), ApiError> {
        storage.set_rotation_config("olm_rotation_days", &self.olm_rotation_days.to_string()).await?;
        storage.set_rotation_config("megolm_rotation_messages", &self.megolm_rotation_messages.to_string()).await?;
        storage.set_rotation_config("max_session_age_days", &self.max_session_age_days.to_string()).await?;
        storage.set_rotation_config("enable_auto_rotation", &self.enable_auto_rotation.to_string()).await?;
        Ok(())
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct KeyRotationLog {
    pub id: i64,
    pub user_id: String,
    pub device_id: String,
    pub room_id: Option<String>,
    pub rotation_type: String,
    pub old_key_id: Option<String>,
    pub new_key_id: String,
    pub reason: Option<String>,
    pub rotated_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RotationStatus {
    pub total_sessions: i64,
    pub rotated_sessions: i64,
    pub last_rotation: Option<DateTime<Utc>>,
}

impl KeyRotationService {
    pub fn new(
        olm_service: Arc<OlmService>,
        megolm_service: Arc<MegolmProvider>,
        storage: Arc<KeyRotationStorage>,
        config: KeyRotationConfig,
    ) -> Self {
        Self { olm_service, megolm_service, storage, config: Arc::new(tokio::sync::RwLock::new(config)) }
    }

    pub async fn new_with_db_config(
        olm_service: Arc<OlmService>,
        megolm_service: Arc<MegolmProvider>,
        storage: Arc<KeyRotationStorage>,
        fallback_config: KeyRotationConfig,
    ) -> Self {
        let config = KeyRotationConfig::load_from_storage(&storage).await.unwrap_or_else(|e| {
            tracing::warn!("Failed to load KeyRotationConfig from database, using defaults: {e}");
            fallback_config
        });

        Self { olm_service, megolm_service, storage, config: Arc::new(tokio::sync::RwLock::new(config)) }
    }

    pub async fn reload_config(&self) -> Result<(), ApiError> {
        let new_config = KeyRotationConfig::load_from_storage(&self.storage).await?;
        *self.config.write().await = new_config;
        Ok(())
    }

    pub async fn update_config(&self, new_config: KeyRotationConfig) -> Result<(), ApiError> {
        new_config.persist_to_storage(&self.storage).await?;
        *self.config.write().await = new_config;
        Ok(())
    }

    pub async fn get_config(&self) -> KeyRotationConfig {
        self.config.read().await.clone()
    }

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

        Ok(false)
    }

    pub async fn rotate_megolm_session(&self, room_id: &str, user_id: &str) -> Result<MegolmSession, ApiError> {
        let new_session = self.megolm_service.create_session(room_id, user_id).await?;

        self.storage.log_rotation(user_id, room_id, "megolm").await?;

        self.share_new_key(room_id, &new_session).await?;

        self.mark_session_as_rotated(room_id, user_id).await?;

        Ok(new_session)
    }

    pub async fn rotate_all_user_sessions(&self, user_id: &str) -> Result<Vec<String>, ApiError> {
        let mut rotated_rooms = Vec::new();

        let rooms = self.storage.get_encrypted_rooms(user_id).await?;

        for room_id in rooms {
            if self.should_rotate_for_room(user_id, &room_id).await? {
                self.rotate_megolm_session(&room_id, user_id).await?;
                rotated_rooms.push(room_id);
            }
        }

        Ok(rotated_rooms)
    }

    pub async fn cleanup_expired_sessions(&self) -> Result<i64, ApiError> {
        let count = self.storage.delete_expired_sessions().await?;
        Ok(count)
    }

    pub async fn get_rotation_status(&self, user_id: &str) -> Result<RotationStatus, ApiError> {
        self.storage.get_rotation_status(user_id).await
    }

    async fn share_new_key(&self, room_id: &str, session: &MegolmSession) -> Result<(), ApiError> {
        tracing::info!("Sharing new megolm key for room {}, session {}", room_id, session.session_id);
        self.storage.record_key_share(room_id, &session.session_id, "rotated").await.map_err(|e| {
            tracing::warn!("Failed to record key share for rotation: {e}");
            ApiError::database("A database error occurred".to_string())
        })
    }

    async fn mark_session_as_rotated(&self, room_id: &str, user_id: &str) -> Result<(), ApiError> {
        tracing::info!("Marking session as rotated for user {} in room {}", user_id, room_id);
        self.storage.mark_rotated(user_id, room_id).await.map_err(|e| {
            tracing::warn!("Failed to mark session as rotated: {e}");
            ApiError::database("A database error occurred".to_string())
        })
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

    pub async fn forward_keys_for_new_member(&self, room_id: &str, new_user_id: &str) -> Result<(), ApiError> {
        let sessions = self.megolm_service.get_room_sessions(room_id).await?;

        for session in &sessions {
            self.megolm_service.share_session(&session.session_id, &[new_user_id.to_string()]).await?;

            self.storage.record_key_share(room_id, &session.session_id, "new_member").await.map_err(|e| {
                tracing::warn!("Failed to record key share for new member: {e}");
                ApiError::database("A database error occurred".to_string())
            })?;
        }

        tracing::info!("Forwarded {} session keys to new member {} in room {}", sessions.len(), new_user_id, room_id);

        Ok(())
    }

    pub async fn get_rooms_needing_key_rotation(&self, user_id: &str) -> Result<Vec<String>, ApiError> {
        self.storage.get_rooms_needing_key_rotation(user_id).await
    }
}

#[async_trait]
pub trait KeyRotationStorageApi: Send + Sync {
    async fn get_user_last_rotation_ts(&self, user_id: &str) -> Result<Option<i64>, ApiError>;
    async fn get_device_rotation_history(
        &self,
        user_id: &str,
        device_id: &str,
    ) -> Result<Vec<(Option<String>, Option<i64>)>, ApiError>;
    async fn set_rotation_config(&self, key: &str, value: &str) -> Result<(), ApiError>;
    async fn get_rotation_config(&self, key: &str) -> Result<Option<String>, ApiError>;
    async fn get_last_rotation_for_key(&self, user_id: &str, key_id: &str) -> Result<Option<i64>, ApiError>;
    async fn get_max_rotation_ts(&self, user_id: &str) -> Result<i64, ApiError>;
}

#[derive(Clone)]
pub struct KeyRotationStorage {
    pool: Arc<sqlx::PgPool>,
}

impl KeyRotationStorage {
    pub fn new(pool: Arc<sqlx::PgPool>) -> Self {
        Self { pool }
    }

    pub async fn log_rotation(&self, user_id: &str, room_id: &str, rotation_type: &str) -> Result<(), ApiError> {
        let now = Utc::now();
        let new_key_id = uuid::Uuid::new_v4().to_string();

        sqlx::query(
            "INSERT INTO key_rotation_log
             (user_id, device_id, room_id, rotation_type, old_key_id, new_key_id, reason, rotated_at)
             VALUES ($1, $2, $3, $4, $5, $6, $7, $8)",
        )
        .bind(user_id)
        .bind("")
        .bind(room_id)
        .bind(rotation_type)
        .bind(None::<String>)
        .bind(&new_key_id)
        .bind(None::<String>)
        .bind(now)
        .execute(&*self.pool)
        .await
        .map_err(|e| {
            tracing::error!("Database error: {e}");
            ApiError::database("A database error occurred".to_string())
        })?;

        Ok(())
    }

    pub async fn get_encrypted_rooms(&self, user_id: &str) -> Result<Vec<String>, ApiError> {
        let rows = sqlx::query_as::<_, (String,)>(
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
        )
        .bind(user_id)
        .fetch_all(&*self.pool)
        .await
        .map_err(|e| {
            tracing::error!("Database error: {e}");
            ApiError::database("A database error occurred".to_string())
        })?;

        Ok(rows.into_iter().map(|r| r.0).collect())
    }

    pub async fn record_key_share(&self, room_id: &str, session_id: &str, share_reason: &str) -> Result<(), ApiError> {
        let now = chrono::Utc::now().timestamp_millis();
        sqlx::query(
            r"
            INSERT INTO megolm_key_shares (room_id, session_id, share_reason, shared_at)
            VALUES ($1, $2, $3, $4)
            ON CONFLICT (room_id, session_id) DO UPDATE SET share_reason = $3, shared_at = $4
            ",
        )
        .bind(room_id)
        .bind(session_id)
        .bind(share_reason)
        .bind(now)
        .execute(&*self.pool)
        .await
        .map_err(|e| {
            tracing::error!("Database error: {e}");
            ApiError::database("A database error occurred".to_string())
        })?;

        Ok(())
    }

    pub async fn mark_rotated(&self, user_id: &str, room_id: &str) -> Result<(), ApiError> {
        let now = chrono::Utc::now().timestamp_millis();
        sqlx::query(
            r"
            INSERT INTO key_rotation_state (user_id, room_id, rotation_count, last_rotation_ts)
            VALUES ($1, $2, 1, $3)
            ON CONFLICT (user_id, room_id) DO UPDATE SET rotation_count = key_rotation_state.rotation_count + 1, last_rotation_ts = $3
            ",
        )
        .bind(user_id)
        .bind(room_id)
        .bind(now)
        .execute(&*self.pool)
        .await
        .map_err(|e| {
            tracing::error!("Database error: {e}");
            ApiError::database("A database error occurred".to_string())
        })?;

        Ok(())
    }

    pub async fn check_needs_rotation(&self, user_id: &str, room_id: &str) -> Result<bool, ApiError> {
        let row = sqlx::query_as::<_, (bool,)>(
            r"
            SELECT COALESCE(rotation_count, 0) > 0 FROM key_rotation_state
            WHERE user_id = $1 AND room_id = $2
            ",
        )
        .bind(user_id)
        .bind(room_id)
        .fetch_optional(&*self.pool)
        .await
        .map_err(|e| {
            tracing::error!("Database error: {e}");
            ApiError::database("A database error occurred".to_string())
        })?;

        Ok(row.as_ref().is_none_or(|r| !r.0))
    }

    pub async fn delete_expired_sessions(&self) -> Result<i64, ApiError> {
        let result =
            sqlx::query("DELETE FROM megolm_sessions WHERE expires_at < (EXTRACT(EPOCH FROM NOW())::BIGINT * 1000)")
                .execute(&*self.pool)
                .await
                .map_err(|e| {
                    tracing::error!("Database error: {e}");
                    ApiError::database("A database error occurred".to_string())
                })?;

        Ok(result.rows_affected() as i64)
    }

    pub async fn get_rotation_status(&self, user_id: &str) -> Result<RotationStatus, ApiError> {
        let seven_days_ago_ms = chrono::Utc::now().timestamp_millis() - 7 * 24 * 3600 * 1000;
        let row = sqlx::query(
            "SELECT
             COUNT(*) as total_sessions,
             COUNT(CASE WHEN last_rotation_ts > $2 THEN 1 END) as rotated_sessions,
             MAX(last_rotation_ts) as last_rotation
             FROM key_rotation_state
             WHERE user_id = $1",
        )
        .bind(user_id)
        .bind(seven_days_ago_ms)
        .fetch_one(&*self.pool)
        .await
        .map_err(|e| {
            tracing::error!("Database error: {e}");
            ApiError::database("A database error occurred".to_string())
        })?;

        use sqlx::Row;
        Ok(RotationStatus {
            total_sessions: row.get("total_sessions"),
            rotated_sessions: row.get("rotated_sessions"),
            last_rotation: row.get("last_rotation"),
        })
    }

    pub async fn get_encrypted_room_members(&self, room_id: &str) -> Result<Vec<String>, ApiError> {
        let rows = sqlx::query_as::<_, (String,)>(
            r"
            SELECT rm.user_id
            FROM room_memberships rm
            INNER JOIN events e ON rm.room_id = e.room_id
            WHERE rm.room_id = $1
              AND rm.membership = 'join'
              AND e.event_type = 'm.room.encryption'
              AND e.state_key IS NOT NULL
            ",
        )
        .bind(room_id)
        .fetch_all(&*self.pool)
        .await
        .map_err(|e| {
            tracing::error!("Database error: {e}");
            ApiError::database("A database error occurred".to_string())
        })?;

        Ok(rows.into_iter().map(|r| r.0).collect())
    }

    pub async fn mark_key_rotation_needed(&self, room_id: &str, leaving_user_id: &str) -> Result<(), ApiError> {
        let now = chrono::Utc::now().timestamp_millis();
        sqlx::query(
            r"
            INSERT INTO key_rotation_pending (room_id, reason, triggered_by_user_id, created_ts)
            VALUES ($1, 'member_left', $2, $3)
            ON CONFLICT (room_id, triggered_by_user_id) DO UPDATE SET created_ts = $3
            ",
        )
        .bind(room_id)
        .bind(leaving_user_id)
        .bind(now)
        .execute(&*self.pool)
        .await
        .map_err(|e| {
            tracing::error!("Database error: {e}");
            ApiError::database("A database error occurred".to_string())
        })?;

        Ok(())
    }

    pub async fn get_rooms_needing_key_rotation(&self, user_id: &str) -> Result<Vec<String>, ApiError> {
        let rows = sqlx::query_as::<_, (String,)>(
            r"
            SELECT DISTINCT krp.room_id
            FROM key_rotation_pending krp
            INNER JOIN room_memberships rm ON krp.room_id = rm.room_id
            WHERE rm.user_id = $1
              AND rm.membership = 'join'
            ",
        )
        .bind(user_id)
        .fetch_all(&*self.pool)
        .await
        .map_err(|e| {
            tracing::error!("Database error: {e}");
            ApiError::database("A database error occurred".to_string())
        })?;

        Ok(rows.into_iter().map(|r| r.0).collect())
    }

    pub async fn clear_key_rotation_needed(&self, room_id: &str) -> Result<(), ApiError> {
        sqlx::query(
            r"
            DELETE FROM key_rotation_pending WHERE room_id = $1
            ",
        )
        .bind(room_id)
        .execute(&*self.pool)
        .await
        .map_err(|e| {
            tracing::error!("Database error: {e}");
            ApiError::database("A database error occurred".to_string())
        })?;

        Ok(())
    }

    /// Get the timestamp of the most recent rotation for a user.
    pub async fn get_user_last_rotation_ts(&self, user_id: &str) -> Result<Option<i64>, ApiError> {
        let result: Option<i64> =
            sqlx::query_scalar(r"SELECT MAX(rotated_at) FROM key_rotation_log WHERE user_id = $1")
                .bind(user_id)
                .fetch_one(&*self.pool)
                .await
                .map_err(|e| {
                    tracing::error!("Failed to query key rotation log: {e}");
                    ApiError::database("A database error occurred".to_string())
                })?;

        Ok(result)
    }

    /// Get the rotation history for a specific user and device, limited to the
    /// most recent 10 entries.
    pub async fn get_device_rotation_history(
        &self,
        user_id: &str,
        device_id: &str,
    ) -> Result<Vec<(Option<String>, Option<i64>)>, ApiError> {
        let rows = sqlx::query(
            r"
            SELECT new_key_id AS key_id, rotated_at AS rotated_ts
            FROM key_rotation_log
            WHERE user_id = $1 AND device_id = $2
            ORDER BY rotated_at DESC
            LIMIT 10
            ",
        )
        .bind(user_id)
        .bind(device_id)
        .fetch_all(&*self.pool)
        .await
        .map_err(|e| {
            tracing::error!("Failed to get rotation history: {e}");
            ApiError::database("A database error occurred".to_string())
        })?;

        Ok(rows
            .iter()
            .map(|row| {
                use sqlx::Row;
                (row.get::<Option<String>, _>("key_id"), row.get::<Option<i64>, _>("rotated_ts"))
            })
            .collect())
    }

    /// Get the last rotation timestamp for a specific key id.
    pub async fn get_last_rotation_for_key(&self, user_id: &str, key_id: &str) -> Result<Option<i64>, ApiError> {
        let result: Option<i64> = sqlx::query_scalar(
            r"
            SELECT EXTRACT(EPOCH FROM rotated_at) * 1000
            FROM key_rotation_log
            WHERE user_id = $1 AND (new_key_id = $2 OR old_key_id = $2)
            ORDER BY rotated_at DESC LIMIT 1
            ",
        )
        .bind(user_id)
        .bind(key_id)
        .fetch_optional(&*self.pool)
        .await
        .map_err(|e| {
            tracing::error!("Failed to query rotation log by key_id: {e}");
            ApiError::database("A database error occurred".to_string())
        })?
        .flatten();

        Ok(result)
    }

    /// Get the maximum rotation timestamp for a user (returns 0 if no
    /// rotations exist).
    pub async fn get_max_rotation_ts(&self, user_id: &str) -> Result<i64, ApiError> {
        let result: i64 = sqlx::query_scalar(
            r"
            SELECT COALESCE(EXTRACT(EPOCH FROM MAX(rotated_at)) * 1000, 0)::bigint
            FROM key_rotation_log
            WHERE user_id = $1
            ",
        )
        .bind(user_id)
        .fetch_one(&*self.pool)
        .await
        .map_err(|e| {
            tracing::error!("Failed to query rotation log: {e}");
            ApiError::database("A database error occurred".to_string())
        })?;

        Ok(result)
    }

    /// Persist a key-value pair in the key_rotation_config table.
    pub async fn set_rotation_config(&self, key: &str, value: &str) -> Result<(), ApiError> {
        sqlx::query(
            r"
            INSERT INTO key_rotation_config (key, value)
            VALUES ($1, $2)
            ON CONFLICT (key) DO UPDATE SET value = $2
            ",
        )
        .bind(key)
        .bind(value)
        .execute(&*self.pool)
        .await
        .map_err(|e| {
            tracing::error!("Failed to persist key rotation config: {e}");
            ApiError::database("A database error occurred".to_string())
        })?;

        Ok(())
    }

    /// Read a value from the key_rotation_config table.
    pub async fn get_rotation_config(&self, key: &str) -> Result<Option<String>, ApiError> {
        let result: Option<String> = sqlx::query_scalar(r"SELECT value FROM key_rotation_config WHERE key = $1")
            .bind(key)
            .fetch_optional(&*self.pool)
            .await
            .map_err(|e| {
                tracing::error!("Failed to query key rotation config: {e}");
                ApiError::database("A database error occurred".to_string())
            })?
            .flatten();

        Ok(result)
    }
}

#[async_trait]
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
}
