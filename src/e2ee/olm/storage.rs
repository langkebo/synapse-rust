use super::models::{OlmAccountData, OlmSessionData};
use crate::error::ApiError;
use sqlx::PgPool;
use std::sync::Arc;

/// Internal row struct for `olm_sessions` (matches DB column types exactly,
/// including `i32` for `message_index` which the public model widens to `u32`).
#[derive(Debug, Clone, sqlx::FromRow)]
pub struct OlmSessionRow {
    pub session_id: String,
    pub user_id: String,
    pub device_id: String,
    pub sender_key: String,
    pub receiver_key: String,
    pub serialized_state: String,
    pub message_index: i32,
    pub created_ts: i64,
    pub last_used_ts: i64,
    pub expires_at: Option<i64>,
}

impl From<OlmSessionRow> for OlmSessionData {
    fn from(row: OlmSessionRow) -> Self {
        OlmSessionData {
            session_id: row.session_id,
            user_id: row.user_id,
            device_id: row.device_id,
            sender_key: row.sender_key,
            receiver_key: row.receiver_key,
            serialized_state: row.serialized_state,
            message_index: row.message_index as u32,
            created_ts: row.created_ts,
            last_used_ts: row.last_used_ts,
            expires_at: row.expires_at,
        }
    }
}

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
            r"
            CREATE TABLE IF NOT EXISTS olm_accounts (
                id BIGSERIAL PRIMARY KEY,
                user_id VARCHAR(255) NOT NULL,
                device_id VARCHAR(255) NOT NULL,
                identity_key VARCHAR(255) NOT NULL,
                serialized_account TEXT NOT NULL,
                is_one_time_keys_published BOOLEAN DEFAULT FALSE,
                is_fallback_key_published BOOLEAN DEFAULT FALSE,
                created_ts BIGINT NOT NULL,
                updated_ts BIGINT NOT NULL,
                UNIQUE(user_id, device_id)
            )
            ",
        )
        .execute(&*self.pool)
        .await?;

        sqlx::query(
            r"
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
                expires_at BIGINT
            )
            ",
        )
        .execute(&*self.pool)
        .await?;

        sqlx::query(
            r"
            CREATE INDEX IF NOT EXISTS idx_olm_sessions_user_device ON olm_sessions(user_id, device_id)
            ",
        )
        .execute(&*self.pool)
        .await?;

        sqlx::query(
            r"
            CREATE INDEX IF NOT EXISTS idx_olm_sessions_sender_key ON olm_sessions(sender_key)
            ",
        )
        .execute(&*self.pool)
        .await?;

        Ok(())
    }

    pub async fn save_account(&self, account: &OlmAccountData) -> Result<(), ApiError> {
        let now = chrono::Utc::now().timestamp_millis();

        sqlx::query(
            r"
            INSERT INTO olm_accounts (
                user_id, device_id, identity_key, serialized_account,
                is_one_time_keys_published, is_fallback_key_published, created_ts, updated_ts
            )
            VALUES ($1, $2, $3, $4, $5, $6, $7, $8)
            ON CONFLICT (user_id, device_id) DO UPDATE SET
                identity_key = EXCLUDED.identity_key,
                serialized_account = EXCLUDED.serialized_account,
                is_one_time_keys_published = EXCLUDED.is_one_time_keys_published,
                is_fallback_key_published = EXCLUDED.is_fallback_key_published,
                updated_ts = EXCLUDED.updated_ts
            ",
        )
        .bind(&account.user_id)
        .bind(&account.device_id)
        .bind(&account.identity_key)
        .bind(&account.serialized_account)
        .bind(account.has_published_one_time_keys)
        .bind(account.has_published_fallback_key)
        .bind(now)
        .bind(now)
        .execute(&*self.pool)
        .await
        .map_err(|e| {
            tracing::error!("Failed to save olm account: {e}");
            ApiError::database("A database error occurred".to_string())
        })?;

        Ok(())
    }

    pub async fn load_account(&self, user_id: &str, device_id: &str) -> Result<Option<OlmAccountData>, ApiError> {
        let row: Option<OlmAccountRow> = sqlx::query_as::<_, OlmAccountRow>(
            r#"
            SELECT
                user_id,
                device_id,
                identity_key,
                serialized_account,
                is_one_time_keys_published,
                is_fallback_key_published
            FROM olm_accounts
            WHERE user_id = $1 AND device_id = $2
            "#,
        )
        .bind(user_id)
        .bind(device_id)
        .fetch_optional(&*self.pool)
        .await
        .map_err(|e| {
            tracing::error!("Failed to load olm account: {e}");
            ApiError::database("A database error occurred".to_string())
        })?;

        Ok(row.map(|r| OlmAccountData {
            user_id: r.user_id,
            device_id: r.device_id,
            identity_key: r.identity_key,
            serialized_account: r.serialized_account,
            has_published_one_time_keys: r.is_one_time_keys_published.unwrap_or(false),
            has_published_fallback_key: r.is_fallback_key_published.unwrap_or(false),
        }))
    }

    pub async fn delete_account(&self, user_id: &str, device_id: &str) -> Result<(), ApiError> {
        sqlx::query(
            r"
            DELETE FROM olm_accounts
            WHERE user_id = $1 AND device_id = $2
            ",
        )
        .bind(user_id)
        .bind(device_id)
        .execute(&*self.pool)
        .await
        .map_err(|e| {
            tracing::error!("Failed to delete olm account: {e}");
            ApiError::database("A database error occurred".to_string())
        })?;

        self.delete_sessions_for_device(user_id, device_id).await?;

        Ok(())
    }

    pub async fn save_session(&self, session: &OlmSessionData) -> Result<(), ApiError> {
        sqlx::query(
            r"
            INSERT INTO olm_sessions (
                user_id, device_id, session_id, sender_key, receiver_key,
                serialized_state, message_index, created_ts, last_used_ts, expires_at
            )
            VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10)
            ON CONFLICT (session_id) DO UPDATE SET
                serialized_state = EXCLUDED.serialized_state,
                message_index = EXCLUDED.message_index,
                last_used_ts = EXCLUDED.last_used_ts,
                expires_at = EXCLUDED.expires_at
            ",
        )
        .bind(&session.user_id)
        .bind(&session.device_id)
        .bind(&session.session_id)
        .bind(&session.sender_key)
        .bind(&session.receiver_key)
        .bind(&session.serialized_state)
        .bind(session.message_index as i32)
        .bind(session.created_ts)
        .bind(session.last_used_ts)
        .bind(session.expires_at)
        .execute(&*self.pool)
        .await
        .map_err(|e| {
            tracing::error!("Failed to save olm session: {e}");
            ApiError::database("A database error occurred".to_string())
        })?;

        Ok(())
    }

    pub async fn load_sessions(&self, user_id: &str, device_id: &str) -> Result<Vec<OlmSessionData>, ApiError> {
        let rows: Vec<OlmSessionRow> = sqlx::query_as::<_, OlmSessionRow>(
            r#"
            SELECT
                session_id,
                user_id,
                device_id,
                sender_key,
                receiver_key,
                serialized_state,
                message_index,
                created_ts,
                last_used_ts,
                expires_at
            FROM olm_sessions
            WHERE user_id = $1 AND device_id = $2
            ORDER BY last_used_ts DESC
            "#,
        )
        .bind(user_id)
        .bind(device_id)
        .fetch_all(&*self.pool)
        .await
        .map_err(|e| {
            tracing::error!("Failed to load olm sessions: {e}");
            ApiError::database("A database error occurred".to_string())
        })?;

        Ok(rows.into_iter().map(Into::into).collect())
    }

    pub async fn load_session(&self, session_id: &str) -> Result<Option<OlmSessionData>, ApiError> {
        let row: Option<OlmSessionRow> = sqlx::query_as::<_, OlmSessionRow>(
            r#"
            SELECT
                session_id,
                user_id,
                device_id,
                sender_key,
                receiver_key,
                serialized_state,
                message_index,
                created_ts,
                last_used_ts,
                expires_at
            FROM olm_sessions
            WHERE session_id = $1
            "#,
        )
        .bind(session_id)
        .fetch_optional(&*self.pool)
        .await
        .map_err(|e| {
            tracing::error!("Failed to load olm session: {e}");
            ApiError::database("A database error occurred".to_string())
        })?;

        Ok(row.map(Into::into))
    }

    pub async fn load_session_by_sender_key(
        &self,
        user_id: &str,
        device_id: &str,
        sender_key: &str,
    ) -> Result<Option<OlmSessionData>, ApiError> {
        let row: Option<OlmSessionRow> = sqlx::query_as::<_, OlmSessionRow>(
            r#"
            SELECT
                session_id,
                user_id,
                device_id,
                sender_key,
                receiver_key,
                serialized_state,
                message_index,
                created_ts,
                last_used_ts,
                expires_at
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
            tracing::error!("Failed to load olm session by sender key: {e}");
            ApiError::database("A database error occurred".to_string())
        })?;

        Ok(row.map(Into::into))
    }

    pub async fn delete_session(&self, session_id: &str) -> Result<(), ApiError> {
        sqlx::query(
            r"
            DELETE FROM olm_sessions
            WHERE session_id = $1
            ",
        )
        .bind(session_id)
        .execute(&*self.pool)
        .await
        .map_err(|e| {
            tracing::error!("Failed to delete olm session: {e}");
            ApiError::database("A database error occurred".to_string())
        })?;

        Ok(())
    }

    pub async fn delete_sessions_for_device(&self, user_id: &str, device_id: &str) -> Result<(), ApiError> {
        sqlx::query(
            r"
            DELETE FROM olm_sessions
            WHERE user_id = $1 AND device_id = $2
            ",
        )
        .bind(user_id)
        .bind(device_id)
        .execute(&*self.pool)
        .await
        .map_err(|e| {
            tracing::error!("Failed to delete olm sessions: {e}");
            ApiError::database("A database error occurred".to_string())
        })?;

        Ok(())
    }

    pub async fn delete_expired_sessions(&self) -> Result<u64, ApiError> {
        let now = chrono::Utc::now().timestamp_millis();

        let result = sqlx::query(
            r"
            DELETE FROM olm_sessions
            WHERE expires_at IS NOT NULL AND expires_at < $1
            ",
        )
        .bind(now)
        .execute(&*self.pool)
        .await
        .map_err(|e| {
            tracing::error!("Failed to delete expired sessions: {e}");
            ApiError::database("A database error occurred".to_string())
        })?;

        Ok(result.rows_affected())
    }

    pub async fn update_session_last_used(&self, session_id: &str) -> Result<(), ApiError> {
        let now = chrono::Utc::now().timestamp_millis();

        sqlx::query(
            r"
            UPDATE olm_sessions
            SET last_used_ts = $1
            WHERE session_id = $2
            ",
        )
        .bind(now)
        .bind(session_id)
        .execute(&*self.pool)
        .await
        .map_err(|e| {
            tracing::error!("Failed to update session last used: {e}");
            ApiError::database("A database error occurred".to_string())
        })?;

        Ok(())
    }

    pub async fn get_session_count(&self, user_id: &str, device_id: &str) -> Result<i64, ApiError> {
        let count: i64 = sqlx::query_scalar::<_, i64>(
            r#"
            SELECT COUNT(*)
            FROM olm_sessions
            WHERE user_id = $1 AND device_id = $2
            "#,
        )
        .bind(user_id)
        .bind(device_id)
        .fetch_one(&*self.pool)
        .await
        .map_err(|e| {
            tracing::error!("Failed to get session count: {e}");
            ApiError::database("A database error occurred".to_string())
        })?;

        Ok(count)
    }
}

#[derive(Debug, Clone, sqlx::FromRow)]
struct OlmAccountRow {
    user_id: String,
    device_id: String,
    identity_key: String,
    serialized_account: String,
    is_one_time_keys_published: Option<bool>,
    is_fallback_key_published: Option<bool>,
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
            has_published_one_time_keys: false,
            has_published_fallback_key: false,
        };

        assert_eq!(account.user_id, "@test:example.com");
        assert_eq!(account.device_id, "DEVICE123");
        assert!(!account.has_published_one_time_keys);
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
            created_ts: 1234567890,
            last_used_ts: 1234567900,
            expires_at: Some(1234568000),
        };

        assert_eq!(session.session_id, "session_123");
        assert_eq!(session.message_index, 5);
        assert!(session.expires_at.is_some());
    }
}
