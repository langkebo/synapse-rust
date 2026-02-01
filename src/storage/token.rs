use sqlx::{Pool, Postgres};
use std::sync::Arc;

#[derive(Debug, Clone)]
pub struct AccessToken {
    pub id: i64,
    pub token: String,
    pub user_id: String,
    pub device_id: Option<String>,
    pub created_ts: i64,
    pub expires_ts: i64,
    pub invalidated_ts: Option<i64>,
}

#[derive(Debug, Clone)]
pub struct RefreshToken {
    pub id: i64,
    pub token: String,
    pub user_id: String,
    pub device_id: String,
    pub created_ts: i64,
    pub expires_ts: i64,
    pub invalidated_ts: Option<i64>,
}

#[derive(Clone)]
pub struct AccessTokenStorage {
    pub pool: Arc<Pool<Postgres>>,
}

impl AccessTokenStorage {
    pub fn new(pool: &Arc<Pool<Postgres>>) -> Self {
        Self { pool: pool.clone() }
    }

    pub async fn create_token(
        &self,
        token: &str,
        user_id: &str,
        device_id: Option<&str>,
        expires_ts: Option<i64>,
    ) -> Result<AccessToken, sqlx::Error> {
        let now = chrono::Utc::now().timestamp();
        let row = sqlx::query!(
            r#"
            INSERT INTO access_tokens (token, user_id, device_id, created_ts, expires_ts)
            VALUES ($1, $2, $3, $4, $5)
            RETURNING id, token, user_id, device_id, created_ts, expires_ts, invalidated_ts
            "#,
            token,
            user_id,
            device_id,
            now,
            expires_ts
        )
        .fetch_one(&*self.pool)
        .await?;
        Ok(AccessToken {
            id: row.id,
            token: row.token,
            user_id: row.user_id,
            device_id: row.device_id,
            created_ts: row.created_ts,
            expires_ts: row.expires_ts,
            invalidated_ts: row.invalidated_ts,
        })
    }

    pub async fn get_token(&self, token: &str) -> Result<Option<AccessToken>, sqlx::Error> {
        let row = sqlx::query!(
            r#"
            SELECT id, token, user_id, device_id, created_ts, expires_ts, invalidated_ts
            FROM access_tokens WHERE token = $1
            "#,
            token
        )
        .fetch_optional(&*self.pool)
        .await?;
        if let Some(row) = row {
            Ok(Some(AccessToken {
                id: row.id,
                token: row.token,
                user_id: row.user_id,
                device_id: row.device_id,
                created_ts: row.created_ts,
                expires_ts: row.expires_ts,
                invalidated_ts: row.invalidated_ts,
            }))
        } else {
            Ok(None)
        }
    }

    pub async fn get_user_tokens(&self, user_id: &str) -> Result<Vec<AccessToken>, sqlx::Error> {
        let rows = sqlx::query_as!(
            AccessToken,
            r#"
            SELECT id, token, user_id, device_id, created_ts, expires_ts, invalidated_ts
            FROM access_tokens WHERE user_id = $1
            "#,
            user_id
        )
        .fetch_all(&*self.pool)
        .await?;
        Ok(rows)
    }

    pub async fn delete_token(&self, token: &str) -> Result<(), sqlx::Error> {
        sqlx::query!(
            r#"
            DELETE FROM access_tokens WHERE token = $1
            "#,
            token
        )
        .execute(&*self.pool)
        .await?;
        Ok(())
    }

    pub async fn delete_user_tokens(&self, user_id: &str) -> Result<(), sqlx::Error> {
        sqlx::query!(
            r#"
            DELETE FROM access_tokens WHERE user_id = $1
            "#,
            user_id
        )
        .execute(&*self.pool)
        .await?;
        Ok(())
    }

    pub async fn delete_device_tokens(&self, device_id: &str) -> Result<(), sqlx::Error> {
        sqlx::query!(
            r#"
            DELETE FROM access_tokens WHERE device_id = $1
            "#,
            device_id
        )
        .execute(&*self.pool)
        .await?;
        Ok(())
    }

    pub async fn token_exists(&self, token: &str) -> Result<bool, sqlx::Error> {
        let result = sqlx::query!(
            r#"
            SELECT 1 AS exists FROM access_tokens WHERE token = $1 LIMIT 1
            "#,
            token
        )
        .fetch_optional(&*self.pool)
        .await?;
        Ok(result.is_some())
    }
}

#[derive(Clone)]
pub struct RefreshTokenStorage {
    pub pool: Arc<Pool<Postgres>>,
}

impl RefreshTokenStorage {
    pub fn new(pool: &Arc<Pool<Postgres>>) -> Self {
        Self { pool: pool.clone() }
    }

    pub async fn create_refresh_token(
        &self,
        token: &str,
        user_id: &str,
        device_id: &str,
        expires_ts: Option<i64>,
    ) -> Result<RefreshToken, sqlx::Error> {
        let now = chrono::Utc::now().timestamp();
        let row = sqlx::query!(
            r#"
            INSERT INTO refresh_tokens (token, user_id, device_id, created_ts, expires_ts)
            VALUES ($1, $2, $3, $4, $5)
            RETURNING id, token, user_id, device_id, created_ts, expires_ts, invalidated_ts
            "#,
            token,
            user_id,
            device_id,
            now,
            expires_ts
        )
        .fetch_one(&*self.pool)
        .await?;
        Ok(RefreshToken {
            id: row.id,
            token: row.token,
            user_id: row.user_id,
            device_id: row.device_id,
            created_ts: row.created_ts,
            expires_ts: row.expires_ts,
            invalidated_ts: row.invalidated_ts,
        })
    }

    pub async fn get_refresh_token(
        &self,
        token: &str,
    ) -> Result<Option<RefreshToken>, sqlx::Error> {
        let row = sqlx::query!(
            r#"
            SELECT id, token, user_id, device_id, created_ts, expires_ts, invalidated_ts
            FROM refresh_tokens WHERE token = $1
            "#,
            token
        )
        .fetch_optional(&*self.pool)
        .await?;
        if let Some(row) = row {
            Ok(Some(RefreshToken {
                id: row.id,
                token: row.token,
                user_id: row.user_id,
                device_id: row.device_id,
                created_ts: row.created_ts,
                expires_ts: row.expires_ts,
                invalidated_ts: row.invalidated_ts,
            }))
        } else {
            Ok(None)
        }
    }

    pub async fn delete_refresh_token(&self, token: &str) -> Result<(), sqlx::Error> {
        sqlx::query!(
            r#"
            DELETE FROM refresh_tokens WHERE token = $1
            "#,
            token
        )
        .execute(&*self.pool)
        .await?;
        Ok(())
    }
}
