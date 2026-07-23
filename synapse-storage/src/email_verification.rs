use async_trait::async_trait;
use serde_json::Value;
use sqlx::{Pool, Postgres};
use std::sync::Arc;
use synapse_common::current_timestamp_millis;
use synapse_common::error::ApiError;

#[derive(Debug, Clone, sqlx::FromRow)]
pub struct EmailVerificationToken {
    pub id: i64,
    pub user_id: Option<String>,
    pub email: String,
    pub token: String,
    pub expires_at: Option<i64>,
    pub created_ts: i64,
    pub is_used: bool,
    pub session_data: Option<serde_json::Value>,
}

// ── Trait ───────────────────────────────────────────────────────────────

#[async_trait]
pub trait EmailVerificationStoreApi: Send + Sync {
    async fn create_verification_token(
        &self,
        email: &str,
        token: &str,
        expires_in_seconds: i64,
        user_id: Option<&str>,
        session_data: Option<serde_json::Value>,
    ) -> Result<i64, sqlx::Error>;
    async fn verify_token(&self, email: &str, token: &str) -> Result<Option<EmailVerificationToken>, sqlx::Error>;
    async fn mark_token_used(&self, token_id: i64) -> Result<(), sqlx::Error>;
    async fn validate_and_consume_token(
        &self,
        token_id: i64,
        submitted_token: &str,
        client_secret: &str,
    ) -> Result<EmailVerificationToken, ApiError>;
    async fn get_verification_token_by_id(&self, token_id: i64) -> Result<Option<EmailVerificationToken>, sqlx::Error>;
    async fn delete_token_by_id(&self, token_id: i64) -> Result<(), sqlx::Error>;
    async fn claim_used_token(&self, token_id: i64) -> Result<Option<EmailVerificationToken>, sqlx::Error>;
    async fn cleanup_expired_tokens(&self) -> Result<i64, sqlx::Error>;
    async fn get_token_by_email(&self, email: &str) -> Result<Option<EmailVerificationToken>, sqlx::Error>;
}

// ── Postgres implementation ─────────────────────────────────────────────

#[derive(Clone)]
pub struct EmailVerificationStorage {
    pub pool: Arc<Pool<Postgres>>,
}

impl EmailVerificationStorage {
    pub fn new(pool: &Arc<Pool<Postgres>>) -> Self {
        Self { pool: pool.clone() }
    }

    pub async fn create_verification_token(
        &self,
        email: &str,
        token: &str,
        expires_in_seconds: i64,
        user_id: Option<&str>,
        session_data: Option<serde_json::Value>,
    ) -> Result<i64, sqlx::Error> {
        let now = current_timestamp_millis();
        let expires_at = now + expires_in_seconds * 1000;

        let row = sqlx::query_as::<_, TokenIdRow>(
            r"
            INSERT INTO email_verification_tokens (email, token, expires_at, created_ts, is_used, user_id, session_data)
            VALUES ($1, $2, $3, $4, FALSE, $5, $6)
            RETURNING id
            ",
        )
        .bind(email)
        .bind(token)
        .bind(expires_at)
        .bind(now)
        .bind(user_id)
        .bind(session_data)
        .fetch_one(&*self.pool)
        .await?;

        Ok(row.id)
    }

    pub async fn verify_token(&self, email: &str, token: &str) -> Result<Option<EmailVerificationToken>, sqlx::Error> {
        let now = current_timestamp_millis();

        let token_record = sqlx::query_as::<_, EmailVerificationToken>(
            r"
            SELECT id, user_id, email, token, expires_at, created_ts, is_used, session_data
            FROM email_verification_tokens
            WHERE email = $1 AND token = $2 AND is_used = FALSE AND expires_at > $3
            ",
        )
        .bind(email)
        .bind(token)
        .bind(now)
        .fetch_optional(&*self.pool)
        .await?;

        Ok(token_record)
    }

    pub async fn mark_token_used(&self, token_id: i64) -> Result<(), sqlx::Error> {
        sqlx::query(
            r"
            UPDATE email_verification_tokens SET is_used = TRUE WHERE id = $1
            ",
        )
        .bind(token_id)
        .execute(&*self.pool)
        .await?;
        Ok(())
    }

    /// Shared validation for submitToken: checks existence, usage, expiration,
    /// token match, and client_secret match against session_data. Marks the
    /// token as used on success.
    pub async fn validate_and_consume_token(
        &self,
        token_id: i64,
        submitted_token: &str,
        client_secret: &str,
    ) -> Result<EmailVerificationToken, ApiError> {
        let verification_token = self
            .get_verification_token_by_id(token_id)
            .await
            .map_err(|e| ApiError::internal_with_log("Failed to get verification token", &e))?;

        let verification_token = verification_token
            .ok_or_else(|| ApiError::bad_request("Invalid session ID or session not found".to_string()))?;

        if verification_token.is_used {
            return Err(ApiError::bad_request("Verification token has already been used".to_string()));
        }

        let now = current_timestamp_millis();
        if verification_token.expires_at.is_none_or(|expires_at| expires_at < now) {
            return Err(ApiError::bad_request("Verification token has expired".to_string()));
        }

        if verification_token.token != submitted_token {
            return Err(ApiError::bad_request("Invalid verification token".to_string()));
        }

        // Mirror the legacy session_data->client_secret check.
        let stored_secret = match verification_token.session_data.as_ref() {
            Some(Value::String(s)) => Some(s.as_str()),
            Some(Value::Object(map)) => map.get("client_secret").and_then(|v| v.as_str()),
            _ => None,
        };
        if stored_secret != Some(client_secret) {
            return Err(ApiError::bad_request("Client secret mismatch".to_string()));
        }

        self.mark_token_used(token_id)
            .await
            .map_err(|e| ApiError::internal_with_log("Failed to mark token as used", &e))?;
        Ok(verification_token)
    }

    pub async fn get_verification_token_by_id(
        &self,
        token_id: i64,
    ) -> Result<Option<EmailVerificationToken>, sqlx::Error> {
        let token_record = sqlx::query_as::<_, EmailVerificationToken>(
            r"
            SELECT id, user_id, email, token, expires_at, created_ts, is_used, session_data
            FROM email_verification_tokens
            WHERE id = $1
            ",
        )
        .bind(token_id)
        .fetch_optional(&*self.pool)
        .await?;

        Ok(token_record)
    }

    pub async fn delete_token_by_id(&self, token_id: i64) -> Result<(), sqlx::Error> {
        sqlx::query(
            r"
            DELETE FROM email_verification_tokens WHERE id = $1
            ",
        )
        .bind(token_id)
        .execute(&*self.pool)
        .await?;
        Ok(())
    }

    /// 原子地"消费"一次已校验的会话：DELETE ... RETURNING 在单条 SQL 中
    /// 完成"取出 + 删除"，保证两个并发请求里只有一个能拿到行，另一个
    /// 拿到 `Ok(None)`。配合 `expires_at > now` 与 `is_used = TRUE`
    /// 一并放在 WHERE 里，避免单独 SELECT/UPDATE 之间的 TOCTOU 窗口。
    ///
    /// 仅返回 `email`、`user_id`、`session_data`，调用方据此完成业务校验
    /// （client_secret、purpose 等）。一旦此函数返回 `Some`，行就已物理
    /// 删除，无法被重放。
    pub async fn claim_used_token(&self, token_id: i64) -> Result<Option<EmailVerificationToken>, sqlx::Error> {
        let now = current_timestamp_millis();
        let row = sqlx::query_as::<_, EmailVerificationToken>(
            r"
            DELETE FROM email_verification_tokens
            WHERE id = $1 AND is_used = TRUE AND expires_at > $2
            RETURNING id, user_id, email, token, expires_at, created_ts, is_used, session_data
            ",
        )
        .bind(token_id)
        .bind(now)
        .fetch_optional(&*self.pool)
        .await?;

        Ok(row)
    }

    pub async fn cleanup_expired_tokens(&self) -> Result<i64, sqlx::Error> {
        let now = current_timestamp_millis();
        let result = sqlx::query(
            r"
            DELETE FROM email_verification_tokens WHERE expires_at < $1
            ",
        )
        .bind(now)
        .execute(&*self.pool)
        .await?;
        Ok(result.rows_affected() as i64)
    }

    pub async fn get_token_by_email(&self, email: &str) -> Result<Option<EmailVerificationToken>, sqlx::Error> {
        let now = current_timestamp_millis();

        let token_record = sqlx::query_as::<_, EmailVerificationToken>(
            r"
            SELECT id, user_id, email, token, expires_at, created_ts, is_used, session_data
            FROM email_verification_tokens
            WHERE email = $1 AND is_used = FALSE AND expires_at > $2
            ORDER BY created_ts DESC
            LIMIT 1
            ",
        )
        .bind(email)
        .bind(now)
        .fetch_optional(&*self.pool)
        .await?;

        Ok(token_record)
    }
}

#[derive(Debug, Clone, sqlx::FromRow)]
struct TokenIdRow {
    pub id: i64,
}

// ── Delegation impl ─────────────────────────────────────────────────────

#[async_trait]
impl EmailVerificationStoreApi for EmailVerificationStorage {
    async fn create_verification_token(
        &self,
        email: &str,
        token: &str,
        expires_in_seconds: i64,
        user_id: Option<&str>,
        session_data: Option<serde_json::Value>,
    ) -> Result<i64, sqlx::Error> {
        self.create_verification_token(email, token, expires_in_seconds, user_id, session_data).await
    }
    async fn verify_token(&self, email: &str, token: &str) -> Result<Option<EmailVerificationToken>, sqlx::Error> {
        self.verify_token(email, token).await
    }
    async fn mark_token_used(&self, token_id: i64) -> Result<(), sqlx::Error> {
        self.mark_token_used(token_id).await
    }
    async fn validate_and_consume_token(
        &self,
        token_id: i64,
        submitted_token: &str,
        client_secret: &str,
    ) -> Result<EmailVerificationToken, ApiError> {
        self.validate_and_consume_token(token_id, submitted_token, client_secret).await
    }
    async fn get_verification_token_by_id(&self, token_id: i64) -> Result<Option<EmailVerificationToken>, sqlx::Error> {
        self.get_verification_token_by_id(token_id).await
    }
    async fn delete_token_by_id(&self, token_id: i64) -> Result<(), sqlx::Error> {
        self.delete_token_by_id(token_id).await
    }
    async fn claim_used_token(&self, token_id: i64) -> Result<Option<EmailVerificationToken>, sqlx::Error> {
        self.claim_used_token(token_id).await
    }
    async fn cleanup_expired_tokens(&self) -> Result<i64, sqlx::Error> {
        self.cleanup_expired_tokens().await
    }
    async fn get_token_by_email(&self, email: &str) -> Result<Option<EmailVerificationToken>, sqlx::Error> {
        self.get_token_by_email(email).await
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_email_verification_token_struct() {
        let token = EmailVerificationToken {
            id: 1,
            user_id: Some("@test:example.com".to_string()),
            email: "test@example.com".to_string(),
            token: "abc123".to_string(),
            expires_at: Some(current_timestamp_millis() + 3600000),
            created_ts: current_timestamp_millis(),
            is_used: false,
            session_data: None,
        };

        assert_eq!(token.id, 1);
        assert_eq!(token.email, "test@example.com");
        assert!(!token.is_used);
    }

    #[test]
    fn test_email_verification_token_with_session_data() {
        let token = EmailVerificationToken {
            id: 2,
            user_id: Some("@user:example.com".to_string()),
            email: "user@example.com".to_string(),
            token: "token456".to_string(),
            expires_at: Some(current_timestamp_millis() + 3600000),
            created_ts: current_timestamp_millis(),
            is_used: false,
            session_data: Some(serde_json::json!({"key": "value"})),
        };
        assert!(token.session_data.is_some());
    }

    #[test]
    fn test_email_verification_token_expired() {
        let current_ts = current_timestamp_millis();
        let token = EmailVerificationToken {
            id: 3,
            user_id: Some("@expired:example.com".to_string()),
            email: "expired@example.com".to_string(),
            token: "expired_token".to_string(),
            expires_at: Some(current_ts - 3600000),
            created_ts: current_ts - 7200000,
            is_used: false,
            session_data: None,
        };
        assert!(token.expires_at.unwrap() < current_ts);
    }

    #[test]
    fn test_email_verification_token_already_used() {
        let token = EmailVerificationToken {
            id: 4,
            user_id: Some("@used:example.com".to_string()),
            email: "used@example.com".to_string(),
            token: "used_token".to_string(),
            expires_at: Some(current_timestamp_millis() + 3600000),
            created_ts: current_timestamp_millis(),
            is_used: true,
            session_data: None,
        };
        assert!(token.is_used);
    }

    #[tokio::test]
    async fn test_delete_token_by_id_removes_verification_session() {
        let pool = match crate::test_utils::prepare_empty_isolated_test_pool().await {
            Ok(pool) => pool,
            Err(error) => {
                tracing::warn!(
                    "Skipping email verification delete-token test because test database is unavailable: {error}"
                );
                return;
            }
        };

        sqlx::query(
            r#"
            CREATE TABLE email_verification_tokens (
                id BIGSERIAL PRIMARY KEY,
                user_id TEXT,
                email TEXT NOT NULL,
                token TEXT NOT NULL,
                expires_at BIGINT NOT NULL,
                created_ts BIGINT NOT NULL,
                is_used BOOLEAN NOT NULL DEFAULT FALSE,
                session_data JSONB
            )
            "#,
        )
        .execute(&*pool)
        .await
        .expect("Failed to create email_verification_tokens table");

        let storage = EmailVerificationStorage::new(&pool);
        let token_id = storage
            .create_verification_token(
                "delete-me@example.com",
                "test-token",
                3600,
                Some("@delete-me:example.com"),
                Some(serde_json::json!({
                    "client_secret": "secret",
                    "purpose": "password_reset"
                })),
            )
            .await
            .expect("Failed to create verification token");

        let before_delete = storage
            .get_verification_token_by_id(token_id)
            .await
            .expect("Failed to fetch verification token before delete");
        assert!(before_delete.is_some());

        storage.delete_token_by_id(token_id).await.expect("Failed to delete verification token");

        let after_delete = storage
            .get_verification_token_by_id(token_id)
            .await
            .expect("Failed to fetch verification token after delete");
        assert!(after_delete.is_none());
    }
}
