use async_trait::async_trait;
use std::sync::Arc;

use sqlx::PgPool;

/// The `RateLimitRecord` struct.
#[derive(Debug, Clone, sqlx::FromRow)]
pub struct RateLimitRecord {
    /// The `messages_per_second` field.
    pub messages_per_second: Option<f64>,
    /// The `burst_count` field.
    pub burst_count: Option<i32>,
}

// ── Trait ───────────────────────────────────────────────────────────────

/// The `RateLimitStoreApi` trait.
#[async_trait]
pub trait RateLimitStoreApi: Send + Sync {
    /// See [`get_user_rate_limit`].
    async fn get_user_rate_limit(&self, user_id: &str) -> Result<Option<RateLimitRecord>, sqlx::Error>;
    /// See [`upsert_user_rate_limit`].
    async fn upsert_user_rate_limit(
        &self,
        user_id: &str,
        messages_per_second: f64,
        burst_count: i32,
    ) -> Result<(), sqlx::Error>;
    /// See [`delete_user_rate_limit`].
    async fn delete_user_rate_limit(&self, user_id: &str) -> Result<(), sqlx::Error>;
}

// ── Postgres implementation ─────────────────────────────────────────────

/// The `RateLimitStorage` struct.
#[derive(Clone)]
pub struct RateLimitStorage {
    pool: Arc<PgPool>,
}

impl RateLimitStorage {
    /// See [`new`].
    pub fn new(pool: &Arc<PgPool>) -> Self {
        Self { pool: pool.clone() }
    }

    /// See [`get_user_rate_limit`].
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

    /// See [`upsert_user_rate_limit`].
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

    /// See [`delete_user_rate_limit`].
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
    use std::sync::Arc;

    /// 每个测试一个从迁移 baseline 克隆出来的独立 schema（返回 guard 与 pool）。
    ///
    /// 2026-09-21：原先用 `connect_shared_test_pool()`（共享 `public`）。共享池的两个问题：
    /// 测试结果取决于环境里 `public` 的残渣（本地 `public` 落后于迁移 baseline 时会直接
    /// 42P01），且并行测试互相影响。按铁律 7 消除共享状态：从模板克隆的 per-test schema
    /// 保证表一定存在、行数从 0 开始（同 `admin_federation.rs` 与 `event_report/db_tests.rs`
    /// 的迁移方式）。
    async fn test_pool() -> (crate::test_isolation::IsolatedTestPool, Arc<sqlx::PgPool>) {
        let isolated = crate::test_isolation::isolated_test_pool().await.expect("isolated test pool");
        let pool = isolated.pool();
        (isolated, pool)
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
        .expect("test fixture: insert must succeed — a swallowed error here surfaces later as an unrelated failure");
    }

    fn make_suffix() -> String {
        uuid::Uuid::new_v4().simple().to_string()
    }

    #[tokio::test]
    async fn get_user_rate_limit_none_for_missing_user() {
        let (_isolated, pool) = test_pool().await;
        let storage = RateLimitStorage::new(&pool);
        let suffix = make_suffix();
        let user_id = format!("@ratelimit_missing_{suffix}:test");
        assert!(storage.get_user_rate_limit(&user_id).await.unwrap().is_none());
    }

    #[tokio::test]
    async fn upsert_user_rate_limit_inserts_new_record() {
        let (_isolated, pool) = test_pool().await;
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
        let (_isolated, pool) = test_pool().await;
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
        let (_isolated, pool) = test_pool().await;
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
