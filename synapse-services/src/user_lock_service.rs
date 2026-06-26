use std::sync::Arc;
use synapse_common::ApiError;
use synapse_storage::user::{LockedUser, UserStore};

#[derive(Clone)]
pub struct UserLockService {
    user_store: Arc<dyn UserStore>,
}

impl UserLockService {
    pub fn new(user_store: Arc<dyn UserStore>) -> Self {
        Self { user_store }
    }

    pub async fn lock_user(
        &self,
        user_id: &str,
        reason: Option<&str>,
        locked_by: &str,
        now_ts: i64,
    ) -> Result<LockedUser, ApiError> {
        self.user_store
            .lock_user(user_id, reason, locked_by, now_ts)
            .await
            .map_err(|e| ApiError::internal_with_log("Failed to lock user", &e))
    }

    pub async fn unlock_user(&self, user_id: &str, now_ts: i64) -> Result<(), ApiError> {
        self.user_store
            .unlock_user(user_id, now_ts)
            .await
            .map_err(|e| ApiError::internal_with_log("Failed to unlock user", &e))
    }

    pub async fn is_user_locked(&self, user_id: &str) -> Result<bool, ApiError> {
        self.user_store
            .is_user_locked(user_id)
            .await
            .map_err(|e| ApiError::internal_with_log("Failed to check user lock status", &e))
    }

    pub async fn get_active_user_lock(&self, user_id: &str) -> Result<Option<LockedUser>, ApiError> {
        self.user_store
            .get_active_user_lock(user_id)
            .await
            .map_err(|e| ApiError::internal_with_log("Failed to get active user lock", &e))
    }

    pub async fn get_locked_users(&self, limit: i64, offset: i64) -> Result<Vec<LockedUser>, ApiError> {
        self.user_store
            .get_locked_users(limit, offset)
            .await
            .map_err(|e| ApiError::internal_with_log("Failed to get locked users", &e))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use synapse_storage::FakeUserStore;

    #[tokio::test]
    async fn test_lock_user_service() {
        let store = Arc::new(FakeUserStore::new());
        let service = UserLockService::new(store);

        let locked = service.lock_user("@alice:example.com", Some("spam"), "admin", 1000).await.unwrap();
        assert_eq!(locked.user_id, "@alice:example.com");
        assert!(locked.is_active);

        assert!(service.is_user_locked("@alice:example.com").await.unwrap());
    }

    #[tokio::test]
    async fn test_unlock_user_service() {
        let store = Arc::new(FakeUserStore::new());
        let service = UserLockService::new(store);

        service.lock_user("@alice:example.com", None, "admin", 1000).await.unwrap();
        service.unlock_user("@alice:example.com", 1001).await.unwrap();
        assert!(!service.is_user_locked("@alice:example.com").await.unwrap());
    }

    #[tokio::test]
    async fn test_get_locked_users_service() {
        let store = Arc::new(FakeUserStore::new());
        let service = UserLockService::new(store);

        service.lock_user("@a:example.com", None, "admin", 1000).await.unwrap();
        service.lock_user("@b:example.com", None, "admin", 1000).await.unwrap();

        let users = service.get_locked_users(10, 0).await.unwrap();
        assert_eq!(users.len(), 2);
    }
}
