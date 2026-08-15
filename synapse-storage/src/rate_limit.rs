use async_trait::async_trait;
use std::sync::Arc;

use sqlx::PgPool;

#[derive(Debug, Clone, sqlx::FromRow)]
pub struct RateLimitRecord {
    pub messages_per_second: Option<f64>,
    pub burst_count: Option<i32>,
}

// ── Trait ───────────────────────────────────────────────────────────────

#[async_trait]
pub trait RateLimitStoreApi: Send + Sync {
    async fn get_user_rate_limit(&self, user_id: &str) -> Result<Option<RateLimitRecord>, sqlx::Error>;
    async fn upsert_user_rate_limit(
        &self,
        user_id: &str,
        messages_per_second: f64,
        burst_count: i32,
    ) -> Result<(), sqlx::Error>;
    async fn delete_user_rate_limit(&self, user_id: &str) -> Result<(), sqlx::Error>;
}

// ── Postgres implementation ─────────────────────────────────────────────

#[derive(Clone)]
pub struct RateLimitStorage {
    pool: Arc<PgPool>,
}

impl RateLimitStorage {
    pub fn new(pool: &Arc<PgPool>) -> Self {
        Self { pool: pool.clone() }
    }

    pub async fn get_user_rate_limit(&self, user_id: &str) -> Result<Option<RateLimitRecord>, sqlx::Error> {
        sqlx::query_as::<_, RateLimitRecord>(
            r"
            SELECT messages_per_second, burst_count
            FROM rate_limits
            WHERE user_id = $1
            ",
        )
        .bind(user_id)
        .fetch_optional(self.pool.as_ref())
        .await
    }

    pub async fn upsert_user_rate_limit(
        &self,
        user_id: &str,
        messages_per_second: f64,
        burst_count: i32,
    ) -> Result<(), sqlx::Error> {
        sqlx::query(
            r"
            INSERT INTO rate_limits (user_id, messages_per_second, burst_count)
            VALUES ($1, $2, $3)
            ON CONFLICT (user_id) DO UPDATE
            SET messages_per_second = EXCLUDED.messages_per_second,
                burst_count = EXCLUDED.burst_count
            ",
        )
        .bind(user_id)
        .bind(messages_per_second)
        .bind(burst_count)
        .execute(self.pool.as_ref())
        .await?;
        Ok(())
    }

    pub async fn delete_user_rate_limit(&self, user_id: &str) -> Result<(), sqlx::Error> {
        sqlx::query(
            r"
            DELETE FROM rate_limits
            WHERE user_id = $1
            ",
        )
        .bind(user_id)
        .execute(self.pool.as_ref())
        .await?;
        Ok(())
    }
}

// ── Trait delegation ────────────────────────────────────────────────────

#[async_trait]
impl RateLimitStoreApi for RateLimitStorage {
    async fn get_user_rate_limit(&self, user_id: &str) -> Result<Option<RateLimitRecord>, sqlx::Error> {
        self.get_user_rate_limit(user_id).await
    }

    async fn upsert_user_rate_limit(
        &self,
        user_id: &str,
        messages_per_second: f64,
        burst_count: i32,
    ) -> Result<(), sqlx::Error> {
        self.upsert_user_rate_limit(user_id, messages_per_second, burst_count).await
    }

    async fn delete_user_rate_limit(&self, user_id: &str) -> Result<(), sqlx::Error> {
        self.delete_user_rate_limit(user_id).await
    }
}

// ── Tests ──────────────────────────────────────────────────────────────

#[cfg(test)]
mod db_tests {
    #![allow(clippy::unwrap_used, clippy::expect_used)]

    use super::*;
    use sqlx::postgres::PgPoolOptions;
    use std::env;

    async fn test_pool() -> Arc<PgPool> {
        let db_url = env::var("TEST_DATABASE_URL")
            .unwrap_or_else(|_| "postgres://synapse:synapse@localhost:15432/synapse_test".to_string());
        let pool = PgPoolOptions::new().max_connections(2).connect(&db_url).await.expect("Failed to connect to test database");
        Arc::new(pool)
    }

    async fn ensure_test_user(pool: &PgPool, user_id: &str) {
        let username = user_id.strip_prefix('@').and_then(|u| u.split(':').next()).unwrap_or("testuser");
        sqlx::query(
            "INSERT INTO users (user_id, username, created_ts) VALUES ($1, $2, EXTRACT(EPOCH FROM NOW()) * 1000) ON CONFLICT (user_id) DO NOTHING",
        )
        .bind(user_id)
        .bind(username)
        .execute(pool)
        .await
        .ok();
    }

    fn make_suffix() -> String {
        uuid::Uuid::new_v4().to_string().replace('-', "")
    }

    #[tokio::test]
    async fn get_user_rate_limit_none_for_missing_user() {
        let pool = test_pool().await;
        let storage = RateLimitStorage::new(&pool);
        let suffix = make_suffix();
        let user_id = format!("@ratelimit_missing_{suffix}:test");
        assert!(storage.get_user_rate_limit(&user_id).await.unwrap().is_none());
    }

    #[tokio::test]
    async fn upsert_user_rate_limit_inserts_new_record() {
        let pool = test_pool().await;
        let storage = RateLimitStorage::new(&pool);
        let suffix = make_suffix();
        let user_id = format!("@ratelimit_insert_{suffix}:test");
        ensure_test_user(&pool, &user_id).await;

        storage.upsert_user_rate_limit(&user_id, 5.0, 10).await.unwrap();
        let record = storage.get_user_rate_limit(&user_id).await.unwrap().unwrap();
        assert_eq!(record.messages_per_second, Some(5.0));
        assert_eq!(record.burst_count, Some(10));

        let _ = sqlx::query("DELETE FROM rate_limits WHERE user_id = $1").bind(&user_id).execute(pool.as_ref()).await;
        let _ = sqlx::query("DELETE FROM users WHERE user_id = $1").bind(&user_id).execute(pool.as_ref()).await;
    }

    #[tokio::test]
    async fn upsert_user_rate_limit_updates_existing_record() {
        let pool = test_pool().await;
        let storage = RateLimitStorage::new(&pool);
        let suffix = make_suffix();
        let user_id = format!("@ratelimit_update_{suffix}:test");
        ensure_test_user(&pool, &user_id).await;

        storage.upsert_user_rate_limit(&user_id, 1.0, 1).await.unwrap();
        storage.upsert_user_rate_limit(&user_id, 9.0, 99).await.unwrap();
        let record = storage.get_user_rate_limit(&user_id).await.unwrap().unwrap();
        assert_eq!(record.messages_per_second, Some(9.0));
        assert_eq!(record.burst_count, Some(99));

        let _ = sqlx::query("DELETE FROM rate_limits WHERE user_id = $1").bind(&user_id).execute(pool.as_ref()).await;
        let _ = sqlx::query("DELETE FROM users WHERE user_id = $1").bind(&user_id).execute(pool.as_ref()).await;
    }

    #[tokio::test]
    async fn delete_user_rate_limit_removes_record() {
        let pool = test_pool().await;
        let storage = RateLimitStorage::new(&pool);
        let suffix = make_suffix();
        let user_id = format!("@ratelimit_delete_{suffix}:test");
        ensure_test_user(&pool, &user_id).await;

        storage.upsert_user_rate_limit(&user_id, 3.0, 30).await.unwrap();
        storage.delete_user_rate_limit(&user_id).await.unwrap();
        assert!(storage.get_user_rate_limit(&user_id).await.unwrap().is_none());

        let _ = sqlx::query("DELETE FROM users WHERE user_id = $1").bind(&user_id).execute(pool.as_ref()).await;
    }
}
