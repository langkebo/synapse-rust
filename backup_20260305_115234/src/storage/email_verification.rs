use sqlx::{Pool, Postgres};
use std::sync::Arc;

#[derive(Debug, Clone, sqlx::FromRow)]
pub struct EmailVerificationToken {
    pub id: i64,
    pub user_id: Option<String>,
    pub email: String,
    pub token: String,
    pub expires_ts: i64,
    pub created_ts: i64,
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
        let now = chrono::Utc::now().timestamp();
        let expires_ts = now + expires_in_seconds;
        let created_ts = now;

        let row = sqlx::query_as::<_, TokenIdRow>(
            r#"
            INSERT INTO email_verification_tokens (email, token, expires_ts, created_ts, used, user_id, session_data)
            VALUES ($1, $2, $3, $4, FALSE, $5, $6)
            RETURNING id
            "#,
        )
        .bind(email)
        .bind(token)
        .bind(expires_ts)
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
        let now = chrono::Utc::now().timestamp();

        let token_record = sqlx::query_as::<_, EmailVerificationToken>(
            r#"
            SELECT id, user_id, email, token, expires_ts, created_ts, used, session_data
            FROM email_verification_tokens
            WHERE email = $1 AND token = $2 AND used = FALSE AND expires_ts > $3
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
            SELECT id, user_id, email, token, expires_ts, created_ts, used, session_data
            FROM email_verification_tokens
            WHERE id = $1
            "#,
        )
        .bind(token_id)
        .fetch_optional(&*self.pool)
        .await?;

        Ok(token_record)
    }

    pub async fn cleanup_expired_tokens(&self) -> Result<i64, sqlx::Error> {
        let now = chrono::Utc::now().timestamp();
        let result = sqlx::query(
            r#"
            DELETE FROM email_verification_tokens WHERE expires_ts < $1
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
        let now = chrono::Utc::now().timestamp();

        let token_record = sqlx::query_as::<_, EmailVerificationToken>(
            r#"
            SELECT id, user_id, email, token, expires_ts, created_ts, used, session_data
            FROM email_verification_tokens
            WHERE email = $1 AND used = FALSE AND expires_ts > $2
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
            expires_ts: 1234567890,
            created_ts: 1234560000,
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
            expires_ts: 1234567890,
            created_ts: 1234560000,
            used: false,
            session_data: Some(serde_json::json!({"key": "value"})),
        };
        assert!(token.session_data.is_some());
    }

    #[test]
    fn test_email_verification_token_expired() {
        let current_ts = chrono::Utc::now().timestamp();
        let token = EmailVerificationToken {
            id: 3,
            user_id: Some("@expired:example.com".to_string()),
            email: "expired@example.com".to_string(),
            token: "expired_token".to_string(),
            expires_ts: current_ts - 3600,
            created_ts: current_ts - 7200,
            used: false,
            session_data: None,
        };
        assert!(token.expires_ts < current_ts);
    }

    #[test]
    fn test_email_verification_token_already_used() {
        let token = EmailVerificationToken {
            id: 4,
            user_id: Some("@used:example.com".to_string()),
            email: "used@example.com".to_string(),
            token: "used_token".to_string(),
            expires_ts: 1234567890,
            created_ts: 1234560000,
            used: true,
            session_data: None,
        };
        assert!(token.used);
    }
}
