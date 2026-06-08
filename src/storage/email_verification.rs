use chrono::Utc;
use sqlx::{Pool, Postgres};
use std::sync::Arc;

#[derive(Debug, Clone, sqlx::FromRow)]
pub struct EmailVerificationToken {
    pub id: i64,
    pub user_id: Option<String>,
    pub email: String,
    pub token: String,
    pub expires_at: i64,
    pub created_ts: i64,
    pub is_used: bool,
    pub session_data: Option<serde_json::Value>,
}

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
        let now = Utc::now().timestamp_millis();
        let expires_at = now + (expires_in_seconds * 1000);

        let row = sqlx::query_as!(
            TokenIdRow,
            r#"INSERT INTO email_verification_tokens (email, token, expires_at, created_ts, is_used, user_id, session_data)
            VALUES ($1, $2, $3, $4, FALSE, $5, $6)
            RETURNING id"#,
            email,
            token,
            expires_at,
            now,
            user_id,
            session_data,
        )
        .fetch_one(&*self.pool)
        .await?;

        Ok(row.id)
    }

    pub async fn verify_token(&self, email: &str, token: &str) -> Result<Option<EmailVerificationToken>, sqlx::Error> {
        let now = Utc::now().timestamp_millis();

        let token_record = sqlx::query_as!(
            EmailVerificationToken,
            r#"SELECT id, user_id, email, token, expires_at, created_ts, is_used AS "is_used!: bool", session_data
            FROM email_verification_tokens
            WHERE email = $1 AND token = $2 AND is_used = FALSE AND expires_at > $3"#,
            email,
            token,
            now,
        )
        .fetch_optional(&*self.pool)
        .await?;

        Ok(token_record)
    }

    pub async fn mark_token_used(&self, token_id: i64) -> Result<(), sqlx::Error> {
        sqlx::query!(
            r#"UPDATE email_verification_tokens SET is_used = TRUE WHERE id = $1"#,
            token_id,
        )
        .execute(&*self.pool)
        .await?;
        Ok(())
    }

    pub async fn get_verification_token_by_id(
        &self,
        token_id: i64,
    ) -> Result<Option<EmailVerificationToken>, sqlx::Error> {
        let token_record = sqlx::query_as!(
            EmailVerificationToken,
            r#"SELECT id, user_id, email, token, expires_at, created_ts, is_used AS "is_used!: bool", session_data
            FROM email_verification_tokens WHERE id = $1"#,
            token_id,
        )
        .fetch_optional(&*self.pool)
        .await?;

        Ok(token_record)
    }

    pub async fn delete_token_by_id(&self, token_id: i64) -> Result<(), sqlx::Error> {
        sqlx::query!(
            r#"DELETE FROM email_verification_tokens WHERE id = $1"#,
            token_id,
        )
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
        let now = Utc::now().timestamp_millis();
        let row = sqlx::query_as!(
            EmailVerificationToken,
            r#"DELETE FROM email_verification_tokens
            WHERE id = $1 AND is_used = TRUE AND expires_at > $2
            RETURNING id, user_id, email, token, expires_at, created_ts, is_used AS "is_used!: bool", session_data"#,
            token_id,
            now,
        )
        .fetch_optional(&*self.pool)
        .await?;

        Ok(row)
    }

    pub async fn cleanup_expired_tokens(&self) -> Result<i64, sqlx::Error> {
        let now = Utc::now().timestamp_millis();
        let result = sqlx::query!(
            r#"DELETE FROM email_verification_tokens WHERE expires_at < $1"#,
            now,
        )
        .execute(&*self.pool)
        .await?;
        Ok(result.rows_affected() as i64)
    }

    pub async fn get_token_by_email(&self, email: &str) -> Result<Option<EmailVerificationToken>, sqlx::Error> {
        let now = Utc::now().timestamp_millis();

        let token_record = sqlx::query_as!(
            EmailVerificationToken,
            r#"SELECT id, user_id, email, token, expires_at, created_ts, is_used AS "is_used!: bool", session_data
            FROM email_verification_tokens
            WHERE email = $1 AND is_used = FALSE AND expires_at > $2
            ORDER BY created_ts DESC
            LIMIT 1"#,
            email,
            now,
        )
        .fetch_optional(&*self.pool)
        .await?;

        Ok(token_record)
    }
}

#[derive(Debug, Clone, sqlx::FromRow)]
struct TokenIdRow {
    pub id: i64,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_email_verification_token_struct() {
        let now = Utc::now().timestamp_millis();
        let token = EmailVerificationToken {
            id: 1,
            user_id: Some("@test:example.com".to_string()),
            email: "test@example.com".to_string(),
            token: "abc123".to_string(),
            expires_at: now + 3600000,
            created_ts: now,
            is_used: false,
            session_data: None,
        };

        assert_eq!(token.id, 1);
        assert_eq!(token.email, "test@example.com");
        assert!(!token.is_used);
    }

    #[test]
    fn test_email_verification_token_with_session_data() {
        let now = Utc::now().timestamp_millis();
        let token = EmailVerificationToken {
            id: 2,
            user_id: Some("@user:example.com".to_string()),
            email: "user@example.com".to_string(),
            token: "token456".to_string(),
            expires_at: now + 3600000,
            created_ts: now,
            is_used: false,
            session_data: Some(serde_json::json!({"key": "value"})),
        };
        assert!(token.session_data.is_some());
    }

    #[test]
    fn test_email_verification_token_expired() {
        let now = Utc::now().timestamp_millis();
        let token = EmailVerificationToken {
            id: 3,
            user_id: Some("@expired:example.com".to_string()),
            email: "expired@example.com".to_string(),
            token: "expired_token".to_string(),
            expires_at: now - 3600000,
            created_ts: now - 7200000,
            is_used: false,
            session_data: None,
        };
        assert!(token.expires_at < now);
    }

    #[test]
    fn test_email_verification_token_already_used() {
        let now = Utc::now().timestamp_millis();
        let token = EmailVerificationToken {
            id: 4,
            user_id: Some("@used:example.com".to_string()),
            email: "used@example.com".to_string(),
            token: "used_token".to_string(),
            expires_at: now + 3600000,
            created_ts: now,
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
                expires_at TIMESTAMPTZ NOT NULL,
                created_ts TIMESTAMPTZ NOT NULL,
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
