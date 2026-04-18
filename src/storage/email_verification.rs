use chrono::{DateTime, Duration, Utc};
use sqlx::{Pool, Postgres};
use std::sync::Arc;

#[derive(Debug, Clone, sqlx::FromRow)]
pub struct EmailVerificationToken {
    pub id: i64,
    pub user_id: Option<String>,
    pub email: String,
    pub token: String,
    pub expires_at: DateTime<Utc>,
    pub created_ts: DateTime<Utc>,
    pub used: bool,
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
        let now = Utc::now();
        let expires_at = now + Duration::seconds(expires_in_seconds);
        let created_ts = now;

        let row = sqlx::query_as::<_, TokenIdRow>(
            r#"
            INSERT INTO email_verification_tokens (email, token, expires_at, created_ts, used, user_id, session_data)
            VALUES ($1, $2, $3, $4, FALSE, $5, $6)
            RETURNING id
            "#,
        )
        .bind(email)
        .bind(token)
        .bind(expires_at)
        .bind(created_ts)
        .bind(user_id)
        .bind(session_data)
        .fetch_one(&*self.pool)
        .await?;

        Ok(row.id)
    }

    pub async fn verify_token(
        &self,
        email: &str,
        token: &str,
    ) -> Result<Option<EmailVerificationToken>, sqlx::Error> {
        let now = Utc::now();

        let token_record = sqlx::query_as::<_, EmailVerificationToken>(
            r#"
            SELECT id, user_id, email, token, expires_at, created_ts, used, session_data
            FROM email_verification_tokens
            WHERE email = $1 AND token = $2 AND used = FALSE AND expires_at > $3
            "#,
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
            r#"
            UPDATE email_verification_tokens SET used = TRUE WHERE id = $1
            "#,
        )
        .bind(token_id)
        .execute(&*self.pool)
        .await?;
        Ok(())
    }

    pub async fn get_verification_token_by_id(
        &self,
        token_id: i64,
    ) -> Result<Option<EmailVerificationToken>, sqlx::Error> {
        let token_record = sqlx::query_as::<_, EmailVerificationToken>(
            r#"
            SELECT id, user_id, email, token, expires_at, created_ts, used, session_data
            FROM email_verification_tokens
            WHERE id = $1
            "#,
        )
        .bind(token_id)
        .fetch_optional(&*self.pool)
        .await?;

        Ok(token_record)
    }

    pub async fn delete_token_by_id(&self, token_id: i64) -> Result<(), sqlx::Error> {
        sqlx::query(
            r#"
            DELETE FROM email_verification_tokens WHERE id = $1
            "#,
        )
        .bind(token_id)
        .execute(&*self.pool)
        .await?;
        Ok(())
    }

    pub async fn cleanup_expired_tokens(&self) -> Result<i64, sqlx::Error> {
        let now = Utc::now();
        let result = sqlx::query(
            r#"
            DELETE FROM email_verification_tokens WHERE expires_at < $1
            "#,
        )
        .bind(now)
        .execute(&*self.pool)
        .await?;
        Ok(result.rows_affected() as i64)
    }

    pub async fn get_token_by_email(
        &self,
        email: &str,
    ) -> Result<Option<EmailVerificationToken>, sqlx::Error> {
        let now = Utc::now();

        let token_record = sqlx::query_as::<_, EmailVerificationToken>(
            r#"
            SELECT id, user_id, email, token, expires_at, created_ts, used, session_data
            FROM email_verification_tokens
            WHERE email = $1 AND used = FALSE AND expires_at > $2
            ORDER BY created_ts DESC
            LIMIT 1
            "#,
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
            expires_at: Utc::now(),
            created_ts: Utc::now(),
            used: false,
            session_data: None,
        };

        assert_eq!(token.id, 1);
        assert_eq!(token.email, "test@example.com");
        assert!(!token.used);
    }

    #[test]
    fn test_email_verification_token_with_session_data() {
        let token = EmailVerificationToken {
            id: 2,
            user_id: Some("@user:example.com".to_string()),
            email: "user@example.com".to_string(),
            token: "token456".to_string(),
            expires_at: Utc::now(),
            created_ts: Utc::now(),
            used: false,
            session_data: Some(serde_json::json!({"key": "value"})),
        };
        assert!(token.session_data.is_some());
    }

    #[test]
    fn test_email_verification_token_expired() {
        let current_ts = Utc::now();
        let token = EmailVerificationToken {
            id: 3,
            user_id: Some("@expired:example.com".to_string()),
            email: "expired@example.com".to_string(),
            token: "expired_token".to_string(),
            expires_at: current_ts - Duration::seconds(3600),
            created_ts: current_ts - Duration::seconds(7200),
            used: false,
            session_data: None,
        };
        assert!(token.expires_at < current_ts);
    }

    #[test]
    fn test_email_verification_token_already_used() {
        let token = EmailVerificationToken {
            id: 4,
            user_id: Some("@used:example.com".to_string()),
            email: "used@example.com".to_string(),
            token: "used_token".to_string(),
            expires_at: Utc::now(),
            created_ts: Utc::now(),
            used: true,
            session_data: None,
        };
        assert!(token.used);
    }

    #[tokio::test]
    async fn test_delete_token_by_id_removes_verification_session() {
        let pool = match crate::test_utils::prepare_empty_isolated_test_pool().await {
            Ok(pool) => pool,
            Err(error) => {
                eprintln!(
                    "Skipping email verification delete-token test because test database is unavailable: {}",
                    error
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
                used BOOLEAN NOT NULL DEFAULT FALSE,
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

        storage
            .delete_token_by_id(token_id)
            .await
            .expect("Failed to delete verification token");

        let after_delete = storage
            .get_verification_token_by_id(token_id)
            .await
            .expect("Failed to fetch verification token after delete");
        assert!(after_delete.is_none());
    }
}
