use crate::cache::CacheManager;
use crate::common::ApiError;
use sqlx::PgPool;
use std::sync::Arc;
use tracing::instrument;

#[derive(Debug, Clone)]
pub struct UserRateLimit {
    pub messages_per_second: f64,
    pub burst_count: i32,
}

pub struct AdminSecurityService {
    pool: Arc<PgPool>,
    cache: Arc<CacheManager>,
}

impl AdminSecurityService {
    pub fn new(pool: Arc<PgPool>, cache: Arc<CacheManager>) -> Self {
        Self { pool, cache }
    }

    #[instrument(skip(self))]
    pub async fn set_shadow_ban(&self, user_id: &str, is_shadow_banned: bool) -> Result<(), ApiError> {
        let result =
            sqlx::query!("UPDATE users SET is_shadow_banned = $2 WHERE user_id = $1", user_id, is_shadow_banned,)
                .execute(&*self.pool)
                .await
                .map_err(|e| ApiError::internal_with_log("Database error", &e))?;

        if result.rows_affected() == 0 {
            return Err(ApiError::not_found("User not found".to_string()));
        }

        self.cache.delete(&format!("user:shadow_banned:{user_id}")).await;
        Ok(())
    }

    #[instrument(skip(self))]
    pub async fn get_user_rate_limit(&self, user_id: &str) -> Result<UserRateLimit, ApiError> {
        let limit =
            sqlx::query!("SELECT messages_per_second, burst_count FROM rate_limits WHERE user_id = $1", user_id,)
                .fetch_optional(&*self.pool)
                .await
                .map_err(|e| ApiError::internal_with_log("Database error", &e))?;

        Ok(match limit {
            Some(row) => UserRateLimit {
                messages_per_second: row.messages_per_second.unwrap_or(5.0),
                burst_count: row.burst_count.unwrap_or(10),
            },
            None => UserRateLimit { messages_per_second: 5.0, burst_count: 10 },
        })
    }

    #[instrument(skip(self))]
    pub async fn set_user_rate_limit(
        &self,
        user_id: &str,
        messages_per_second: f64,
        burst_count: i32,
    ) -> Result<UserRateLimit, ApiError> {
        sqlx::query!(
            "INSERT INTO rate_limits (user_id, messages_per_second, burst_count) VALUES ($1, $2, $3) ON CONFLICT (user_id) DO UPDATE SET messages_per_second = $2, burst_count = $3",
            user_id,
            messages_per_second,
            burst_count
        )
        .execute(&*self.pool)
        .await
        .map_err(|e| ApiError::internal_with_log("Database error", &e))?;

        Ok(UserRateLimit { messages_per_second, burst_count })
    }

    #[instrument(skip(self))]
    pub async fn delete_user_rate_limit(&self, user_id: &str) -> Result<(), ApiError> {
        sqlx::query!("DELETE FROM rate_limits WHERE user_id = $1", user_id)
            .execute(&*self.pool)
            .await
            .map_err(|e| ApiError::internal_with_log("Database error", &e))?;
        Ok(())
    }
}
