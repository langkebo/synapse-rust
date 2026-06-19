use std::sync::Arc;
use synapse_cache::CacheManager;
use synapse_common::ApiError;
use synapse_storage::{RateLimitStorage, UserStorage};
use tracing::instrument;

#[derive(Debug, Clone)]
pub struct UserRateLimit {
    pub messages_per_second: f64,
    pub burst_count: i32,
}

pub struct AdminSecurityService {
    user_storage: UserStorage,
    rate_limit_storage: RateLimitStorage,
    cache: Arc<CacheManager>,
}

impl AdminSecurityService {
    pub fn new(user_storage: UserStorage, cache: Arc<CacheManager>) -> Self {
        Self { user_storage, rate_limit_storage: RateLimitStorage::new(), cache }
    }

    #[instrument(skip(self))]
    pub async fn set_shadow_ban(&self, user_id: &str, is_shadow_banned: bool) -> Result<(), ApiError> {
        let updated = self
            .user_storage
            .set_shadow_ban(user_id, is_shadow_banned)
            .await
            .map_err(|e| ApiError::internal_with_log("Database error", &e))?;

        if !updated {
            return Err(ApiError::not_found("User not found".to_string()));
        }

        self.cache.delete(&format!("user:shadow_banned:{user_id}")).await;
        Ok(())
    }

    #[instrument(skip(self))]
    pub async fn get_user_rate_limit(&self, user_id: &str) -> Result<UserRateLimit, ApiError> {
        let limit = self
            .rate_limit_storage
            .get_user_rate_limit(&self.user_storage.pool, user_id)
            .await
            .map_err(|e| ApiError::internal_with_log("Database error", &e))?;

        Ok(match limit {
            Some(row) => UserRateLimit {
                messages_per_second: row.messages_per_second.unwrap_or(5.0_f64),
                burst_count: row.burst_count.unwrap_or(10_i32),
            },
            None => UserRateLimit { messages_per_second: 5.0_f64, burst_count: 10_i32 },
        })
    }

    #[instrument(skip(self))]
    pub async fn set_user_rate_limit(
        &self,
        user_id: &str,
        messages_per_second: f64,
        burst_count: i32,
    ) -> Result<UserRateLimit, ApiError> {
        self.rate_limit_storage
            .upsert_user_rate_limit(&self.user_storage.pool, user_id, messages_per_second, burst_count)
            .await
            .map_err(|e| ApiError::internal_with_log("Database error", &e))?;

        Ok(UserRateLimit { messages_per_second, burst_count })
    }

    #[instrument(skip(self))]
    pub async fn delete_user_rate_limit(&self, user_id: &str) -> Result<(), ApiError> {
        self.rate_limit_storage
            .delete_user_rate_limit(&self.user_storage.pool, user_id)
            .await
            .map_err(|e| ApiError::internal_with_log("Database error", &e))?;
        Ok(())
    }
}
