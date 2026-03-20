// Key Rotation Service
// E2EE Phase 2: Automatic key rotation for enhanced security

use crate::e2ee::megolm::{MegolmService, MegolmSession};
use crate::e2ee::olm::OlmService;
use crate::error::ApiError;
use chrono::{DateTime, Duration, Utc};
use serde::{Deserialize, Serialize};
use std::sync::Arc;

pub struct KeyRotationService {
    olm_service: Arc<OlmService>,
    megolm_service: Arc<MegolmService>,
    storage: Arc<KeyRotationStorage>,
    config: KeyRotationConfig,
}

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
            olm_rotation_days: 7,
            megolm_rotation_messages: 100,
            max_session_age_days: 90,
            enable_auto_rotation: true,
        }
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
    pub pending_rotations: i64,
    pub last_rotation: Option<DateTime<Utc>>,
}

impl KeyRotationService {
    pub fn new(
        olm_service: Arc<OlmService>,
        megolm_service: Arc<MegolmService>,
        storage: Arc<KeyRotationStorage>,
        config: KeyRotationConfig,
    ) -> Self {
        Self {
            olm_service,
            megolm_service,
            storage,
            config,
        }
    }

    pub async fn should_rotate(&self, session: &MegolmSession) -> Result<bool, ApiError> {
        let age_days = (Utc::now() - session.last_used_ts).num_days();
        if age_days >= self.config.olm_rotation_days {
            return Ok(true);
        }

        if session.message_index >= self.config.megolm_rotation_messages {
            return Ok(true);
        }

        if let Some(expires_at) = session.expires_at {
            if Utc::now() >= expires_at {
                return Ok(true);
            }
        }

        Ok(false)
    }

    pub async fn rotate_megolm_session(
        &self,
        room_id: &str,
        user_id: &str,
    ) -> Result<MegolmSession, ApiError> {
        let new_session = self.megolm_service.create_session(room_id, user_id).await?;

        self.storage.log_rotation(user_id, room_id, "megolm").await?;

        self.share_new_key(room_id, &new_session).await?;

        self.mark_session_as_rotated(room_id, user_id).await?;

        Ok(new_session)
    }

    pub async fn rotate_all_user_sessions(
        &self,
        user_id: &str,
    ) -> Result<Vec<String>, ApiError> {
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

    async fn share_new_key(&self, _room_id: &str, _session: &MegolmSession) -> Result<(), ApiError> {
        Ok(())
    }

    async fn mark_session_as_rotated(&self, _room_id: &str, _user_id: &str) -> Result<(), ApiError> {
        Ok(())
    }

    async fn should_rotate_for_room(&self, _user_id: &str, _room_id: &str) -> Result<bool, ApiError> {
        Ok(true)
    }
}

pub struct KeyRotationStorage {
    pool: Arc<sqlx::PgPool>,
}

impl KeyRotationStorage {
    pub fn new(pool: Arc<sqlx::PgPool>) -> Self {
        Self { pool }
    }

    pub async fn log_rotation(
        &self,
        user_id: &str,
        room_id: &str,
        rotation_type: &str,
    ) -> Result<(), ApiError> {
        let now = Utc::now();
        let new_key_id = uuid::Uuid::new_v4().to_string();
        
        sqlx::query(
            "INSERT INTO key_rotation_log 
             (user_id, device_id, room_id, rotation_type, old_key_id, new_key_id, reason, rotated_at)
             VALUES ($1, $2, $3, $4, $5, $6, $7, $8)"
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
        .map_err(|e| ApiError::internal(format!("Database error: {}", e)))?;

        Ok(())
    }

    pub async fn get_encrypted_rooms(&self, _user_id: &str) -> Result<Vec<String>, ApiError> {
        Ok(Vec::new())
    }

    pub async fn delete_expired_sessions(&self) -> Result<i64, ApiError> {
        let result = sqlx::query(
            "DELETE FROM megolm_sessions WHERE expires_at < NOW()"
        )
        .execute(&*self.pool)
        .await
        .map_err(|e| ApiError::internal(format!("Database error: {}", e)))?;

        Ok(result.rows_affected() as i64)
    }

    pub async fn get_rotation_status(&self, user_id: &str) -> Result<RotationStatus, ApiError> {
        let row = sqlx::query(
            "SELECT 
             COUNT(*) as total_sessions,
             COUNT(CASE WHEN rotated_at > NOW() - INTERVAL '7 days' THEN 1 END) as rotated_sessions,
             COUNT(CASE WHEN last_used_ts < NOW() - INTERVAL '7 days' THEN 1 END) as pending_rotations,
             MAX(rotated_at) as last_rotation
             FROM key_rotation_log 
             WHERE user_id = $1"
        )
        .bind(user_id)
        .fetch_one(&*self.pool)
        .await
        .map_err(|e| ApiError::internal(format!("Database error: {}", e)))?;

        use sqlx::Row;
        Ok(RotationStatus {
            total_sessions: row.get("total_sessions"),
            rotated_sessions: row.get("rotated_sessions"),
            pending_rotations: row.get("pending_rotations"),
            last_rotation: row.get("last_rotation"),
        })
    }
}
