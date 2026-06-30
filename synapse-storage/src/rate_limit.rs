use std::sync::Arc;

use sqlx::PgPool;

#[derive(Debug, Clone, sqlx::FromRow)]
pub struct RateLimitRecord {
    pub messages_per_second: Option<f64>,
    pub burst_count: Option<i32>,
}

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
