use sqlx::{Pool, Postgres};
use crate::common::*;

#[derive(Debug, Clone)]
pub struct AccessToken {
    pub token: String,
    pub user_id: String,
    pub device_id: Option<String>,
    pub created_ts: chrono::DateTime<chrono::Utc>,
    pub expired_ts: Option<chrono::DateTime<chrono::Utc>>,
}

#[derive(Debug, Clone)]
pub struct RefreshToken {
    pub token: String,
    pub user_id: String,
    pub device_id: Option<String>,
    pub created_ts: chrono::DateTime<chrono::Utc>,
    pub expired_ts: Option<chrono::DateTime<chrono::Utc>>,
}

pub struct AccessTokenStorage<'a> {
    pool: &'a Pool<Postgres>,
}

impl<'a> AccessTokenStorage<'a> {
    pub fn new(pool: &'a Pool<Postgres>) -> Self {
        Self { pool }
    }

    pub async fn create_token(
        &self,
        token: &str,
        user_id: &str,
        device_id: Option<&str>,
        expiry_ts: Option<i64>,
    ) -> Result<AccessToken, sqlx::Error> {
        let now = chrono::Utc::now().timestamp();
        sqlx::query_as!(
            AccessToken,
            r#"
            INSERT INTO access_tokens (token, user_id, device_id, created_ts, expired_ts)
            VALUES ($1, $2, $3, $4, $5)
            RETURNING *
            "#,
            token,
            user_id,
            device_id,
            now,
            expiry_ts
        ).fetch_one(self.pool).await
    }

    pub async fn get_token(&self, token: &str) -> Result<Option<AccessToken>, sqlx::Error> {
        sqlx::query_as!(
            AccessToken,
            r#"
            SELECT * FROM access_tokens WHERE token = $1
            "#,
            token
        ).fetch_optional(self.pool).await
    }

    pub async fn get_user_tokens(&self, user_id: &str) -> Result<Vec<AccessToken>, sqlx::Error> {
        sqlx::query_as!(
            AccessToken,
            r#"
            SELECT * FROM access_tokens WHERE user_id = $1
            "#,
            user_id
        ).fetch_all(self.pool).await
    }

    pub async fn delete_token(&self, token: &str) -> Result<(), sqlx::Error> {
        sqlx::query!(
            r#"
            DELETE FROM access_tokens WHERE token = $1
            "#,
            token
        ).execute(self.pool).await?;
        Ok(())
    }

    pub async fn delete_user_tokens(&self, user_id: &str) -> Result<(), sqlx::Error> {
        sqlx::query!(
            r#"
            DELETE FROM access_tokens WHERE user_id = $1
            "#,
            user_id
        ).execute(self.pool).await?;
        Ok(())
    }

    pub async fn delete_device_tokens(&self, device_id: &str) -> Result<(), sqlx::Error> {
        sqlx::query!(
            r#"
            DELETE FROM access_tokens WHERE device_id = $1
            "#,
            device_id
        ).execute(self.pool).await?;
        Ok(())
    }

    pub async fn create_refresh_token(
        &self,
        token: &str,
        user_id: &str,
        device_id: Option<&str>,
        expiry_ts: Option<i64>,
    ) -> Result<RefreshToken, sqlx::Error> {
        let now = chrono::Utc::now().timestamp();
        sqlx::query_as!(
            RefreshToken,
            r#"
            INSERT INTO refresh_tokens (token, user_id, device_id, created_ts, expired_ts)
            VALUES ($1, $2, $3, $4, $5)
            RETURNING *
            "#,
            token,
            user_id,
            device_id,
            now,
            expiry_ts
        ).fetch_one(self.pool).await
    }

    pub async fn get_refresh_token(&self, token: &str) -> Result<Option<RefreshToken>, sqlx::Error> {
        sqlx::query_as!(
            RefreshToken,
            r#"
            SELECT * FROM refresh_tokens WHERE token = $1
            "#,
            token
        ).fetch_optional(self.pool).await
    }

    pub async fn delete_refresh_token(&self, token: &str) -> Result<(), sqlx::Error> {
        sqlx::query!(
            r#"
            DELETE FROM refresh_tokens WHERE token = $1
            "#,
            token
        ).execute(self.pool).await?;
        Ok(())
    }

    pub async fn token_exists(&self, token: &str) -> Result<bool, sqlx::Error> {
        let result = sqlx::query!(
            r#"
            SELECT 1 FROM access_tokens WHERE token = $1 LIMIT 1
            "#,
            token
        ).fetch_optional(self.pool).await?;
        Ok(result.is_some())
    }
}
