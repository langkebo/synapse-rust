//! MSC4108 QR 登录 token 持久化存储
//!
//! 已登录设备通过 `POST /v1/login/qr_token` 生成短时 login token（60s TTL），
//! 经 MSC4108 安全通道传给新设备，新设备用 `m.login.token` 兑换 access token。
//! token 单次使用，消费即删除（原子 DELETE ... RETURNING，防重放）。

use async_trait::async_trait;
use sqlx::{FromRow, PgPool};
use std::sync::Arc;
use synapse_common::current_timestamp_millis;

/// The `LoginToken` struct.
#[derive(Debug, Clone, FromRow)]
pub struct LoginToken {
    /// The `id` field.
    pub id: i64,
    /// The `token` field.
    pub token: String,
    /// The `user_id` field.
    pub user_id: String,
    /// The `device_id` field.
    pub device_id: Option<String>,
    /// The `created_ts` field.
    pub created_ts: i64,
    /// The `expires_at` field.
    pub expires_at: i64,
}

/// The `LoginTokenStoreApi` trait.
#[async_trait]
pub trait LoginTokenStoreApi: Send + Sync {
    /// See [`create_login_token`].
    async fn create_login_token(
        &self,
        token: &str,
        user_id: &str,
        device_id: Option<&str>,
        expires_at: i64,
    ) -> Result<(), sqlx::Error>;
    /// See [`consume_login_token`].
    async fn consume_login_token(&self, token: &str) -> Result<Option<LoginToken>, sqlx::Error>;
    /// See [`cleanup_expired_tokens`].
    async fn cleanup_expired_tokens(&self, now_ts: i64) -> Result<u64, sqlx::Error>;
}

/// The `LoginTokenStorage` struct.
#[derive(Clone)]
pub struct LoginTokenStorage {
    pool: Arc<PgPool>,
}

impl LoginTokenStorage {
    /// See [`new`].
    pub fn new(pool: &Arc<PgPool>) -> Self {
        Self { pool: pool.clone() }
    }

    /// See [`create_login_token`].
    pub async fn create_login_token(
        &self,
        token: &str,
        user_id: &str,
        device_id: Option<&str>,
        expires_at: i64,
    ) -> Result<(), sqlx::Error> {
        let now = current_timestamp_millis();
        sqlx::query(
            r#"
            INSERT INTO login_tokens (token, user_id, device_id, created_ts, expires_at)
            VALUES ($1, $2, $3, $4, $5)
            "#,
        )
        .bind(token)
        .bind(user_id)
        .bind(device_id)
        .bind(now)
        .bind(expires_at)
        .execute(&*self.pool)
        .await?;
        Ok(())
    }

    /// 原子消费：仅当 token 存在且未过期时返回并删除（单次使用 + 过期检查一体）。
    pub async fn consume_login_token(&self, token: &str) -> Result<Option<LoginToken>, sqlx::Error> {
        let now = current_timestamp_millis();
        let row = sqlx::query_as::<_, LoginToken>(
            r#"
            DELETE FROM login_tokens
            WHERE token = $1 AND expires_at > $2
            RETURNING id, token, user_id, device_id, created_ts, expires_at
            "#,
        )
        .bind(token)
        .bind(now)
        .fetch_optional(&*self.pool)
        .await?;
        Ok(row)
    }

    /// See [`cleanup_expired_tokens`].
    pub async fn cleanup_expired_tokens(&self, now_ts: i64) -> Result<u64, sqlx::Error> {
        let result =
            sqlx::query("DELETE FROM login_tokens WHERE expires_at < $1").bind(now_ts).execute(&*self.pool).await?;
        Ok(result.rows_affected())
    }
}

#[async_trait]
impl LoginTokenStoreApi for LoginTokenStorage {
    async fn create_login_token(
        &self,
        token: &str,
        user_id: &str,
        device_id: Option<&str>,
        expires_at: i64,
    ) -> Result<(), sqlx::Error> {
        self.create_login_token(token, user_id, device_id, expires_at).await
    }
    async fn consume_login_token(&self, token: &str) -> Result<Option<LoginToken>, sqlx::Error> {
        self.consume_login_token(token).await
    }
    async fn cleanup_expired_tokens(&self, now_ts: i64) -> Result<u64, sqlx::Error> {
        self.cleanup_expired_tokens(now_ts).await
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_login_token_structure() {
        let token = LoginToken {
            id: 1,
            token: "abc123".to_string(),
            user_id: "@user:localhost".to_string(),
            device_id: Some("DEVICE123".to_string()),
            created_ts: 1700000000000,
            expires_at: 1700000060000,
        };
        assert_eq!(token.token, "abc123");
        assert_eq!(token.user_id, "@user:localhost");
        assert!(token.device_id.is_some());
        assert!(token.expires_at > token.created_ts);
    }

    #[test]
    fn test_login_token_expiry_ttl() {
        let created_ts = 1700000000000i64;
        let ttl_ms = 60_000;
        let expires_at = created_ts + ttl_ms;
        assert_eq!(expires_at, 1700000060000);
    }
}

#[cfg(test)]
mod db_tests {
    #![allow(clippy::unwrap_used, clippy::expect_used)]

    use super::*;
    use sqlx::postgres::PgPoolOptions;
    use std::env;
    use std::sync::Arc;
    use std::time::Duration;

    async fn test_pool() -> Arc<PgPool> {
        let db_url = env::var("TEST_DATABASE_URL")
            .unwrap_or_else(|_| "postgres://synapse:synapse@localhost:5432/synapse_test".to_string());
        let pool = PgPoolOptions::new()
            .max_connections(2)
            .acquire_timeout(Duration::from_secs(30))
            .connect(&db_url)
            .await
            .expect("Failed to connect to test database");
        Arc::new(pool)
    }

    fn make_suffix() -> String {
        uuid::Uuid::new_v4().simple().to_string()
    }

    #[tokio::test]
    async fn create_login_token_then_consume_returns_token() {
        let pool = test_pool().await;
        let storage = LoginTokenStorage::new(&pool);
        let suffix = make_suffix();
        let token = format!("qr_token_{suffix}");
        let user_id = format!("@qrcode_{suffix}:test");
        let expires_at = current_timestamp_millis() + 60_000;

        storage.create_login_token(&token, &user_id, Some("DEVICE1"), expires_at).await.unwrap();
        let consumed = storage.consume_login_token(&token).await.unwrap().unwrap();
        assert_eq!(consumed.token, token);
        assert_eq!(consumed.user_id, user_id);
        assert_eq!(consumed.device_id.as_deref(), Some("DEVICE1"));
    }

    #[tokio::test]
    async fn consume_login_token_expired_returns_none() {
        let pool = test_pool().await;
        let storage = LoginTokenStorage::new(&pool);
        let suffix = make_suffix();
        let token = format!("qr_expired_{suffix}");
        let user_id = format!("@qrexpired_{suffix}:test");
        let expires_at = current_timestamp_millis() - 1000;

        storage.create_login_token(&token, &user_id, None, expires_at).await.unwrap();
        assert!(storage.consume_login_token(&token).await.unwrap().is_none());

        let _ = sqlx::query("DELETE FROM login_tokens WHERE token = $1").bind(&token).execute(pool.as_ref()).await;
    }

    #[tokio::test]
    async fn consume_login_token_second_time_returns_none() {
        let pool = test_pool().await;
        let storage = LoginTokenStorage::new(&pool);
        let suffix = make_suffix();
        let token = format!("qr_single_{suffix}");
        let user_id = format!("@qrsingle_{suffix}:test");
        let expires_at = current_timestamp_millis() + 60_000;

        storage.create_login_token(&token, &user_id, None, expires_at).await.unwrap();
        assert!(storage.consume_login_token(&token).await.unwrap().is_some());
        // 单次使用：第二次消费返回 None（防重放）
        assert!(storage.consume_login_token(&token).await.unwrap().is_none());
    }

    #[tokio::test]
    async fn cleanup_expired_tokens_removes_expired_only() {
        let pool = test_pool().await;
        let storage = LoginTokenStorage::new(&pool);
        let suffix = make_suffix();
        let expired_token = format!("qr_cleanup_exp_{suffix}");
        let valid_token = format!("qr_cleanup_val_{suffix}");
        let user_id = format!("@qrcleanup_{suffix}:test");
        let now = current_timestamp_millis();

        storage.create_login_token(&expired_token, &user_id, None, now - 1000).await.unwrap();
        storage.create_login_token(&valid_token, &user_id, None, now + 60_000).await.unwrap();

        let removed = storage.cleanup_expired_tokens(now).await.unwrap();
        // 共享 public schema 下可能有其它测试残留的过期 token，故只断言「至少删除
        // 我们自己的过期 token」，核心语义是「不误删有效 token」。
        assert!(removed >= 1, "cleanup 应至少删除我们插入的过期 token，实际 removed={removed}");

        // 有效 token 仍可消费（cleanup 未误删）
        assert!(storage.consume_login_token(&valid_token).await.unwrap().is_some());
    }
}
