use sqlx::{Pool, Postgres};
use std::sync::Arc;

#[derive(Debug, Clone, sqlx::FromRow)]
pub struct EmailVerificationToken {
    pub id: i64,
    pub email: String,
    pub token: String,
    pub expires_at: i64,
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
        session_data: Option<serde_json::Value>,
    ) -> Result<i64, sqlx::Error> {
        let now = chrono::Utc::now().timestamp();
        let expires_at = now + expires_in_seconds;
        let created_ts = now;

        let row = sqlx::query_as::<_, TokenIdRow>(
            r#"
            INSERT INTO email_verification_tokens (email, token, expires_at, created_ts, used, session_data)
            VALUES ($1, $2, $3, $4, FALSE, $5)
            RETURNING id
            "#,
        )
        .bind(email)
        .bind(token)
        .bind(expires_at)
        .bind(created_ts)
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
            SELECT id, email, token, expires_at, created_ts, used, session_data
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
            SELECT id, email, token, expires_at, created_ts, used, session_data
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
            DELETE FROM email_verification_tokens WHERE expires_at < $1
            "#,
        )
        .bind(now)
        .execute(&*self.pool)
        .await?;
        Ok(result.rows_affected() as i64)
    }
}

#[derive(Debug, Clone, sqlx::FromRow)]
struct TokenIdRow {
    pub id: i64,
}
