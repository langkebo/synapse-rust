use crate::UserService;
use std::sync::Arc;
use synapse_cache::CacheManager;
use synapse_common::ApiError;
use synapse_storage::rate_limit::RateLimitStoreApi;
use synapse_storage::UserStore;
use tracing::instrument;

#[derive(Debug, Clone)]
pub struct UserRateLimit {
    pub messages_per_second: f64,
    pub burst_count: i32,
}

pub struct AdminSecurityService {
    user_storage: Arc<dyn UserStore>,
    #[allow(dead_code)]
    user_service: Arc<UserService>,
    rate_limit_storage: Arc<dyn RateLimitStoreApi>,
    cache: Arc<CacheManager>,
}

impl AdminSecurityService {
    pub fn new(
        user_storage: Arc<dyn UserStore>,
        user_service: Arc<UserService>,
        rate_limit_storage: Arc<dyn RateLimitStoreApi>,
        cache: Arc<CacheManager>,
    ) -> Self {
        Self { user_storage, user_service, rate_limit_storage, cache }
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
            .get_user_rate_limit(user_id)
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
            .upsert_user_rate_limit(user_id, messages_per_second, burst_count)
            .await
            .map_err(|e| ApiError::internal_with_log("Database error", &e))?;

        Ok(UserRateLimit { messages_per_second, burst_count })
    }

    #[instrument(skip(self))]
    pub async fn delete_user_rate_limit(&self, user_id: &str) -> Result<(), ApiError> {
        self.rate_limit_storage
            .delete_user_rate_limit(user_id)
            .await
            .map_err(|e| ApiError::internal_with_log("Database error", &e))?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use synapse_storage::test_mocks::{InMemoryRateLimitStore, SharedFakeUserStore};

    fn fake_user_store() -> SharedFakeUserStore {
        Arc::new(synapse_storage::test_mocks::FakeUserStore::new())
    }

    fn fake_cache() -> Arc<CacheManager> {
        Arc::new(CacheManager::new(&synapse_cache::CacheConfig::default()))
    }

    fn test_service() -> AdminSecurityService {
        let user_store = fake_user_store();
        let user_service = Arc::new(crate::UserService::new(user_store.clone()));
        AdminSecurityService::new(user_store, user_service, Arc::new(InMemoryRateLimitStore::new()), fake_cache())
    }

    #[tokio::test]
    async fn get_rate_limit_returns_defaults_for_unknown_user() {
        let svc = test_service();
        let limit = svc.get_user_rate_limit("@unknown:example.com").await.unwrap();
        assert_eq!(limit.messages_per_second, 5.0);
        assert_eq!(limit.burst_count, 10);
    }

    #[tokio::test]
    async fn set_and_get_rate_limit() {
        let svc = test_service();
        let set = svc.set_user_rate_limit("@alice:example.com", 20.0, 15).await.unwrap();
        assert_eq!(set.messages_per_second, 20.0);
        assert_eq!(set.burst_count, 15);

        let got = svc.get_user_rate_limit("@alice:example.com").await.unwrap();
        assert_eq!(got.messages_per_second, 20.0);
        assert_eq!(got.burst_count, 15);
    }

    #[tokio::test]
    async fn delete_rate_limit_resets_to_defaults() {
        let svc = test_service();
        svc.set_user_rate_limit("@alice:example.com", 20.0, 15).await.unwrap();
        svc.delete_user_rate_limit("@alice:example.com").await.unwrap();
        let limit = svc.get_user_rate_limit("@alice:example.com").await.unwrap();
        assert_eq!(limit.messages_per_second, 5.0);
        assert_eq!(limit.burst_count, 10);
    }

    #[tokio::test]
    async fn set_shadow_ban_updates_user() {
        let svc = test_service();
        // Initially not shadow banned
        let user = svc.user_service.get_user_or_not_found("@alice:example.com").await.unwrap();
        assert!(!user.is_shadow_banned);

        svc.set_shadow_ban("@alice:example.com", true).await.unwrap();

        let updated = svc.user_service.get_user_or_not_found("@alice:example.com").await.unwrap();
        assert!(updated.is_shadow_banned);
    }

    #[tokio::test]
    async fn set_shadow_ban_on_nonexistent_user_returns_not_found() {
        let svc = test_service();
        let result = svc.set_shadow_ban("@nonexistent:example.com", true).await;
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("not found"));
    }

    #[tokio::test]
    async fn set_shadow_ban_false_removes_ban() {
        let svc = test_service();
        svc.set_shadow_ban("@alice:example.com", true).await.unwrap();
        svc.set_shadow_ban("@alice:example.com", false).await.unwrap();

        let user = svc.user_service.get_user_or_not_found("@alice:example.com").await.unwrap();
        assert!(!user.is_shadow_banned);
    }
}
