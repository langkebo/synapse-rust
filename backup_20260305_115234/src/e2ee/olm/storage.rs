use super::models::{OlmAccountData, OlmSessionData};
use crate::error::ApiError;
use sqlx::{PgPool, Row};
use std::sync::Arc;

#[derive(Clone)]
pub struct OlmStorage {
    pool: Arc<PgPool>,
}

impl OlmStorage {
    pub fn new(pool: &Arc<PgPool>) -> Self {
        Self { pool: pool.clone() }
    }

    pub async fn create_tables(&self) -> Result<(), sqlx::Error> {
        sqlx::query(
            r#"
            CREATE TABLE IF NOT EXISTS olm_accounts (
                id BIGSERIAL PRIMARY KEY,
                user_id VARCHAR(255) NOT NULL,
                device_id VARCHAR(255) NOT NULL,
                identity_key VARCHAR(255) NOT NULL,
                serialized_account TEXT NOT NULL,
                one_time_keys_published BOOLEAN DEFAULT FALSE,
                fallback_key_published BOOLEAN DEFAULT FALSE,
                created_ts BIGINT NOT NULL,
                updated_ts BIGINT NOT NULL,
                UNIQUE(user_id, device_id)
            )
            "#,
        )
        .execute(&*self.pool)
        .await?;

        sqlx::query(
            r#"
            CREATE TABLE IF NOT EXISTS olm_sessions (
                id BIGSERIAL PRIMARY KEY,
                user_id VARCHAR(255) NOT NULL,
                device_id VARCHAR(255) NOT NULL,
                session_id VARCHAR(255) NOT NULL UNIQUE,
                sender_key VARCHAR(255) NOT NULL,
                receiver_key VARCHAR(255) NOT NULL,
                serialized_state TEXT NOT NULL,
                message_index INTEGER DEFAULT 0,
                created_ts BIGINT NOT NULL,
                last_used_ts BIGINT NOT NULL,
                expires_ts BIGINT
            )
            "#,
        )
        .execute(&*self.pool)
        .await?;

        sqlx::query(
            r#"
            CREATE INDEX IF NOT EXISTS idx_olm_sessions_user_device ON olm_sessions(user_id, device_id)
            "#,
        )
        .execute(&*self.pool)
        .await?;

        sqlx::query(
            r#"
            CREATE INDEX IF NOT EXISTS idx_olm_sessions_sender_key ON olm_sessions(sender_key)
            "#,
        )
        .execute(&*self.pool)
        .await?;

        Ok(())
    }

    pub async fn save_account(&self, account: &OlmAccountData) -> Result<(), ApiError> {
        let now = chrono::Utc::now().timestamp_millis();

        sqlx::query(
            r#"
            INSERT INTO olm_accounts (
                user_id, device_id, identity_key, serialized_account,
                one_time_keys_published, fallback_key_published, created_ts, updated_ts
            )
            VALUES ($1, $2, $3, $4, $5, $6, $7, $8)
            ON CONFLICT (user_id, device_id) DO UPDATE SET
                identity_key = EXCLUDED.identity_key,
                serialized_account = EXCLUDED.serialized_account,
                one_time_keys_published = EXCLUDED.one_time_keys_published,
                fallback_key_published = EXCLUDED.fallback_key_published,
                updated_ts = EXCLUDED.updated_ts
            "#,
        )
        .bind(&account.user_id)
        .bind(&account.device_id)
        .bind(&account.identity_key)
        .bind(&account.serialized_account)
        .bind(account.one_time_keys_published)
        .bind(account.fallback_key_published)
        .bind(now)
        .bind(now)
        .execute(&*self.pool)
        .await
        .map_err(|e| ApiError::internal(format!("Failed to save olm account: {}", e)))?;

        Ok(())
    }

    pub async fn load_account(
        &self,
        user_id: &str,
        device_id: &str,
    ) -> Result<Option<OlmAccountData>, ApiError> {
        let row = sqlx::query(
            r#"
            SELECT user_id, device_id, identity_key, serialized_account,
                   one_time_keys_published, fallback_key_published
            FROM olm_accounts
            WHERE user_id = $1 AND device_id = $2
            "#,
        )
        .bind(user_id)
        .bind(device_id)
        .fetch_optional(&*self.pool)
        .await
        .map_err(|e| ApiError::internal(format!("Failed to load olm account: {}", e)))?;

        Ok(row.map(|r| OlmAccountData {
            user_id: r.get("user_id"),
            device_id: r.get("device_id"),
            identity_key: r.get("identity_key"),
            serialized_account: r.get("serialized_account"),
            one_time_keys_published: r.get("one_time_keys_published"),
            fallback_key_published: r.get("fallback_key_published"),
        }))
    }

    pub async fn delete_account(&self, user_id: &str, device_id: &str) -> Result<(), ApiError> {
        sqlx::query(
            r#"
            DELETE FROM olm_accounts
            WHERE user_id = $1 AND device_id = $2
            "#,
        )
        .bind(user_id)
        .bind(device_id)
        .execute(&*self.pool)
        .await
        .map_err(|e| ApiError::internal(format!("Failed to delete olm account: {}", e)))?;

        self.delete_sessions_for_device(user_id, device_id).await?;

        Ok(())
    }

    pub async fn save_session(&self, session: &OlmSessionData) -> Result<(), ApiError> {
        sqlx::query(
            r#"
            INSERT INTO olm_sessions (
                user_id, device_id, session_id, sender_key, receiver_key,
                serialized_state, message_index, created_ts, last_used_ts, expires_ts
            )
            VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10)
            ON CONFLICT (session_id) DO UPDATE SET
                serialized_state = EXCLUDED.serialized_state,
                message_index = EXCLUDED.message_index,
                last_used_ts = EXCLUDED.last_used_ts,
                expires_ts = EXCLUDED.expires_ts
            "#,
        )
        .bind(&session.user_id)
        .bind(&session.device_id)
        .bind(&session.session_id)
        .bind(&session.sender_key)
        .bind(&session.receiver_key)
        .bind(&session.serialized_state)
        .bind(session.message_index as i32)
        .bind(session.created_at)
        .bind(session.last_used_at)
        .bind(session.expires_at)
        .execute(&*self.pool)
        .await
        .map_err(|e| ApiError::internal(format!("Failed to save olm session: {}", e)))?;

        Ok(())
    }

    pub async fn load_sessions(
        &self,
        user_id: &str,
        device_id: &str,
    ) -> Result<Vec<OlmSessionData>, ApiError> {
        let rows = sqlx::query(
            r#"
            SELECT session_id, user_id, device_id, sender_key, receiver_key,
                   serialized_state, message_index, created_ts, last_used_ts, expires_ts
            FROM olm_sessions
            WHERE user_id = $1 AND device_id = $2
            ORDER BY last_used_ts DESC
            "#,
        )
        .bind(user_id)
        .bind(device_id)
        .fetch_all(&*self.pool)
        .await
        .map_err(|e| ApiError::internal(format!("Failed to load olm sessions: {}", e)))?;

        Ok(rows
            .into_iter()
            .map(|r| OlmSessionData {
                session_id: r.get("session_id"),
                user_id: r.get("user_id"),
                device_id: r.get("device_id"),
                sender_key: r.get("sender_key"),
                receiver_key: r.get("receiver_key"),
                serialized_state: r.get("serialized_state"),
                message_index: r.get::<i32, _>("message_index") as u32,
                created_at: r.get("created_ts"),
                last_used_at: r.get("last_used_ts"),
                expires_at: r.get("expires_ts"),
            })
            .collect())
    }

    pub async fn load_session(&self, session_id: &str) -> Result<Option<OlmSessionData>, ApiError> {
        let row = sqlx::query(
            r#"
            SELECT session_id, user_id, device_id, sender_key, receiver_key,
                   serialized_state, message_index, created_ts, last_used_ts, expires_ts
            FROM olm_sessions
            WHERE session_id = $1
            "#,
        )
        .bind(session_id)
        .fetch_optional(&*self.pool)
        .await
        .map_err(|e| ApiError::internal(format!("Failed to load olm session: {}", e)))?;

        Ok(row.map(|r| OlmSessionData {
            session_id: r.get("session_id"),
            user_id: r.get("user_id"),
            device_id: r.get("device_id"),
            sender_key: r.get("sender_key"),
            receiver_key: r.get("receiver_key"),
            serialized_state: r.get("serialized_state"),
            message_index: r.get::<i32, _>("message_index") as u32,
            created_at: r.get("created_ts"),
            last_used_at: r.get("last_used_ts"),
            expires_at: r.get("expires_ts"),
        }))
    }

    pub async fn load_session_by_sender_key(
        &self,
        user_id: &str,
        device_id: &str,
        sender_key: &str,
    ) -> Result<Option<OlmSessionData>, ApiError> {
        let row = sqlx::query(
            r#"
            SELECT session_id, user_id, device_id, sender_key, receiver_key,
                   serialized_state, message_index, created_ts, last_used_ts, expires_ts
            FROM olm_sessions
            WHERE user_id = $1 AND device_id = $2 AND sender_key = $3
            ORDER BY last_used_ts DESC
            LIMIT 1
            "#,
        )
        .bind(user_id)
        .bind(device_id)
        .bind(sender_key)
        .fetch_optional(&*self.pool)
        .await
        .map_err(|e| {
            ApiError::internal(format!("Failed to load olm session by sender key: {}", e))
        })?;

        Ok(row.map(|r| OlmSessionData {
            session_id: r.get("session_id"),
            user_id: r.get("user_id"),
            device_id: r.get("device_id"),
            sender_key: r.get("sender_key"),
            receiver_key: r.get("receiver_key"),
            serialized_state: r.get("serialized_state"),
            message_index: r.get::<i32, _>("message_index") as u32,
            created_at: r.get("created_ts"),
            last_used_at: r.get("last_used_ts"),
            expires_at: r.get("expires_ts"),
        }))
    }

    pub async fn delete_session(&self, session_id: &str) -> Result<(), ApiError> {
        sqlx::query(
            r#"
            DELETE FROM olm_sessions
            WHERE session_id = $1
            "#,
        )
        .bind(session_id)
        .execute(&*self.pool)
        .await
        .map_err(|e| ApiError::internal(format!("Failed to delete olm session: {}", e)))?;

        Ok(())
    }

    pub async fn delete_sessions_for_device(
        &self,
        user_id: &str,
        device_id: &str,
    ) -> Result<(), ApiError> {
        sqlx::query(
            r#"
            DELETE FROM olm_sessions
            WHERE user_id = $1 AND device_id = $2
            "#,
        )
        .bind(user_id)
        .bind(device_id)
        .execute(&*self.pool)
        .await
        .map_err(|e| ApiError::internal(format!("Failed to delete olm sessions: {}", e)))?;

        Ok(())
    }

    pub async fn delete_expired_sessions(&self) -> Result<u64, ApiError> {
        let now = chrono::Utc::now().timestamp_millis();

        let result = sqlx::query(
            r#"
            DELETE FROM olm_sessions
            WHERE expires_ts IS NOT NULL AND expires_ts < $1
            "#,
        )
        .bind(now)
        .execute(&*self.pool)
        .await
        .map_err(|e| ApiError::internal(format!("Failed to delete expired sessions: {}", e)))?;

        Ok(result.rows_affected())
    }

    pub async fn update_session_last_used(&self, session_id: &str) -> Result<(), ApiError> {
        let now = chrono::Utc::now().timestamp_millis();

        sqlx::query(
            r#"
            UPDATE olm_sessions
            SET last_used_ts = $1
            WHERE session_id = $2
            "#,
        )
        .bind(now)
        .bind(session_id)
        .execute(&*self.pool)
        .await
        .map_err(|e| ApiError::internal(format!("Failed to update session last used: {}", e)))?;

        Ok(())
    }

    pub async fn get_session_count(&self, user_id: &str, device_id: &str) -> Result<i64, ApiError> {
        let row = sqlx::query(
            r#"
            SELECT COUNT(*) as count
            FROM olm_sessions
            WHERE user_id = $1 AND device_id = $2
            "#,
        )
        .bind(user_id)
        .bind(device_id)
        .fetch_one(&*self.pool)
        .await
        .map_err(|e| ApiError::internal(format!("Failed to get session count: {}", e)))?;

        Ok(row.get("count"))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_olm_account_data_serialization() {
        let account = OlmAccountData {
            user_id: "@test:example.com".to_string(),
            device_id: "DEVICE123".to_string(),
            identity_key: "test_identity_key".to_string(),
            serialized_account: "serialized_data".to_string(),
            one_time_keys_published: false,
            fallback_key_published: false,
        };

        assert_eq!(account.user_id, "@test:example.com");
        assert_eq!(account.device_id, "DEVICE123");
        assert!(!account.one_time_keys_published);
    }

    #[test]
    fn test_olm_session_data_serialization() {
        let session = OlmSessionData {
            session_id: "session_123".to_string(),
            user_id: "@test:example.com".to_string(),
            device_id: "DEVICE123".to_string(),
            sender_key: "sender_key".to_string(),
            receiver_key: "receiver_key".to_string(),
            serialized_state: "state_data".to_string(),
            message_index: 5,
            created_at: 1234567890,
            last_used_at: 1234567900,
            expires_at: Some(1234568000),
        };

        assert_eq!(session.session_id, "session_123");
        assert_eq!(session.message_index, 5);
        assert!(session.expires_at.is_some());
    }
}
