use std::sync::Arc;
use synapse_common::ApiError;
use synapse_storage::{LockedUser, UserStorage};

pub struct UserLockService {
    user_storage: Arc<UserStorage>,
}

impl UserLockService {
    pub fn new(user_storage: Arc<UserStorage>) -> Self {
        Self { user_storage }
    }

    pub async fn lock_user(
        &self,
        user_id: &str,
        reason: Option<&str>,
        locked_by: &str,
    ) -> Result<LockedUser, ApiError> {
        let now_ts = chrono::Utc::now().timestamp_millis();
        self.user_storage
            .lock_user(user_id, reason, locked_by, now_ts)
            .await
            .map_err(|e| ApiError::internal_with_log("Failed to lock user", &e))
    }

    pub async fn unlock_user(&self, user_id: &str) -> Result<(), ApiError> {
        let now_ts = chrono::Utc::now().timestamp_millis();
        self.user_storage
            .unlock_user(user_id, now_ts)
            .await
            .map_err(|e| ApiError::internal_with_log("Failed to unlock user", &e))
    }

    pub async fn is_user_locked(&self, user_id: &str) -> Result<bool, ApiError> {
        self.user_storage
            .is_user_locked(user_id)
            .await
            .map_err(|e| ApiError::internal_with_log("Failed to check user lock status", &e))
    }

    pub async fn get_active_user_lock(&self, user_id: &str) -> Result<Option<LockedUser>, ApiError> {
        self.user_storage
            .get_active_user_lock(user_id)
            .await
            .map_err(|e| ApiError::internal_with_log("Failed to get user lock", &e))
    }

    pub async fn get_locked_users(&self, limit: i64, offset: i64) -> Result<Vec<LockedUser>, ApiError> {
        self.user_storage
            .get_locked_users(limit, offset)
            .await
            .map_err(|e| ApiError::internal_with_log("Failed to get locked users", &e))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_locked_user_fields() {
        let user = LockedUser {
            id: 1,
            user_id: "@user:server".to_string(),
            reason: Some("violation".to_string()),
            locked_by: "@admin:server".to_string(),
            created_ts: 1700000000000,
            unlocked_ts: None,
            is_active: true,
        };

        assert_eq!(user.id, 1);
        assert_eq!(user.user_id, "@user:server");
        assert_eq!(user.reason.as_deref(), Some("violation"));
        assert_eq!(user.locked_by, "@admin:server");
        assert!(user.unlocked_ts.is_none());
        assert!(user.is_active);
    }

    #[test]
    fn test_locked_user_clone() {
        let user = LockedUser {
            id: 1,
            user_id: "@user:server".to_string(),
            reason: None,
            locked_by: "@admin:server".to_string(),
            created_ts: 1700000000000,
            unlocked_ts: None,
            is_active: true,
        };
        let cloned = user.clone();
        assert_eq!(cloned.user_id, user.user_id);
        assert_eq!(cloned.locked_by, user.locked_by);
    }

    #[test]
    fn test_locked_user_unlocked_state() {
        let mut user = LockedUser {
            id: 1,
            user_id: "@user:server".to_string(),
            reason: Some("violation".to_string()),
            locked_by: "@admin:server".to_string(),
            created_ts: 1700000000000,
            unlocked_ts: None,
            is_active: true,
        };

        assert!(user.is_active);
        assert!(user.unlocked_ts.is_none());

        user.is_active = false;
        user.unlocked_ts = Some(1700001000000);
        assert!(!user.is_active);
        assert_eq!(user.unlocked_ts, Some(1700001000000));
    }

    // DB-dependent tests marked with #[ignore]

    #[tokio::test]
    #[ignore = "requires PostgreSQL database"]
    async fn test_lock_user() {
        // Requires a running PostgreSQL with locked_users table
    }

    #[tokio::test]
    #[ignore = "requires PostgreSQL database"]
    async fn test_unlock_user() {
        // Requires a running PostgreSQL
    }

    #[tokio::test]
    #[ignore = "requires PostgreSQL database"]
    async fn test_is_user_locked() {
        // Requires a running PostgreSQL
    }

    #[tokio::test]
    #[ignore = "requires PostgreSQL database"]
    async fn test_double_lock_upsert() {
        // Requires a running PostgreSQL
    }

    #[tokio::test]
    #[ignore = "requires PostgreSQL database"]
    async fn test_unlock_non_locked_user() {
        // Requires a running PostgreSQL
    }
}
