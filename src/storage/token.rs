use sqlx::{Pool, Postgres};
use std::sync::Arc;

#[derive(Debug, Clone, sqlx::FromRow)]
pub struct AccessToken {
    pub id: i64,
    pub token: String,
    pub user_id: String,
    pub device_id: Option<String>,
    pub created_ts: i64,
    pub expires_ts: i64,
    pub invalidated_ts: Option<i64>,
}

#[derive(Debug, Clone, sqlx::FromRow)]
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
        let row = sqlx::query_as::<_, AccessToken>(
            r#"
            INSERT INTO access_tokens (token, user_id, device_id, created_ts, expires_ts)
            VALUES ($1, $2, $3, $4, $5)
            RETURNING id, token, user_id, device_id, created_ts, expires_ts, invalidated_ts
            "#,
        )
        .bind(token)
        .bind(user_id)
        .bind(device_id)
        .bind(now)
        .bind(expires_ts)
        .fetch_one(&*self.pool)
        .await?;
        Ok(row)
    }

    pub async fn get_token(&self, token: &str) -> Result<Option<AccessToken>, sqlx::Error> {
        let row = sqlx::query_as::<_, AccessToken>(
            r#"
            SELECT id, token, user_id, device_id, created_ts, expires_ts, invalidated_ts
            FROM access_tokens WHERE token = $1
            "#,
        )
        .bind(token)
        .fetch_optional(&*self.pool)
        .await?;
        Ok(row)
    }

    pub async fn get_user_tokens(&self, user_id: &str) -> Result<Vec<AccessToken>, sqlx::Error> {
        let rows = sqlx::query_as::<_, AccessToken>(
            r#"
            SELECT id, token, user_id, device_id, created_ts, expires_ts, invalidated_ts
            FROM access_tokens WHERE user_id = $1
            "#,
        )
        .bind(user_id)
        .fetch_all(&*self.pool)
        .await?;
        Ok(rows)
    }

    pub async fn delete_token(&self, token: &str) -> Result<(), sqlx::Error> {
        sqlx::query(
            r#"
            DELETE FROM access_tokens WHERE token = $1
            "#,
        )
        .bind(token)
        .execute(&*self.pool)
        .await?;
        Ok(())
    }

    pub async fn delete_user_tokens(&self, user_id: &str) -> Result<(), sqlx::Error> {
        sqlx::query(
            r#"
            DELETE FROM access_tokens WHERE user_id = $1
            "#,
        )
        .bind(user_id)
        .execute(&*self.pool)
        .await?;
        Ok(())
    }

    pub async fn delete_device_tokens(&self, device_id: &str) -> Result<(), sqlx::Error> {
        sqlx::query(
            r#"
            DELETE FROM access_tokens WHERE device_id = $1
            "#,
        )
        .bind(device_id)
        .execute(&*self.pool)
        .await?;
        Ok(())
    }

    pub async fn token_exists(&self, token: &str) -> Result<bool, sqlx::Error> {
        let result = sqlx::query_scalar::<_, i32>(
            r#"
            SELECT 1 AS "exists" FROM access_tokens WHERE token = $1 LIMIT 1
            "#,
        )
        .bind(token)
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
        let row = sqlx::query_as::<_, RefreshToken>(
            r#"
            INSERT INTO refresh_tokens (token, user_id, device_id, created_ts, expires_ts)
            VALUES ($1, $2, $3, $4, $5)
            RETURNING id, token, user_id, device_id, created_ts, expires_ts, invalidated_ts
            "#,
        )
        .bind(token)
        .bind(user_id)
        .bind(device_id)
        .bind(now)
        .bind(expires_ts)
        .fetch_one(&*self.pool)
        .await?;
        Ok(row)
    }

    pub async fn get_refresh_token(
        &self,
        token: &str,
    ) -> Result<Option<RefreshToken>, sqlx::Error> {
        let row = sqlx::query_as::<_, RefreshToken>(
            r#"
            SELECT id, token, user_id, device_id, created_ts, expires_ts, invalidated_ts
            FROM refresh_tokens WHERE token = $1
            "#,
        )
        .bind(token)
        .fetch_optional(&*self.pool)
        .await?;
        Ok(row)
    }

    pub async fn delete_refresh_token(&self, token: &str) -> Result<(), sqlx::Error> {
        sqlx::query(
            r#"
            DELETE FROM refresh_tokens WHERE token = $1
            "#,
        )
        .bind(token)
        .execute(&*self.pool)
        .await?;
        Ok(())
    }
}
