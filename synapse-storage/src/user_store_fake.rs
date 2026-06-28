use async_trait::async_trait;
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::RwLock;

use crate::user::{
    LockedUser, User, UserDirectorySearchResult, UserProfile, UserSearchResult, UserStatsSummary, UserStore,
};

/// In-memory adapter for UserStore — used in unit tests.
/// Stores locked users in a Vec behind RwLock.
#[derive(Clone, Default)]
pub struct FakeUserStore {
    locked_users: Arc<RwLock<Vec<LockedUser>>>,
}

impl FakeUserStore {
    pub fn new() -> Self {
        Self { locked_users: Arc::new(RwLock::new(Vec::new())) }
    }
}

#[async_trait]
impl UserStore for FakeUserStore {
    fn pool(&self) -> &Arc<sqlx::PgPool> {
        unimplemented!("FakeUserStore does not provide a database pool")
    }

    async fn lock_user(
        &self,
        user_id: &str,
        reason: Option<&str>,
        locked_by: &str,
        now_ts: i64,
    ) -> Result<LockedUser, sqlx::Error> {
        let mut users = self.locked_users.write().await;
        // Deactivate any existing active lock for this user
        for u in users.iter_mut() {
            if u.user_id == user_id {
                u.is_active = false;
            }
        }
        let locked = LockedUser {
            id: users.len() as i64 + 1,
            user_id: user_id.to_string(),
            reason: reason.map(|s| s.to_string()),
            locked_by: locked_by.to_string(),
            created_ts: now_ts,
            unlocked_ts: None,
            is_active: true,
        };
        users.push(locked.clone());
        Ok(locked)
    }

    async fn unlock_user(&self, user_id: &str, now_ts: i64) -> Result<(), sqlx::Error> {
        let mut users = self.locked_users.write().await;
        for u in users.iter_mut() {
            if u.user_id == user_id && u.is_active {
                u.is_active = false;
                u.unlocked_ts = Some(now_ts);
            }
        }
        Ok(())
    }

    async fn is_user_locked(&self, user_id: &str) -> Result<bool, sqlx::Error> {
        let users = self.locked_users.read().await;
        Ok(users.iter().any(|u| u.user_id == user_id && u.is_active))
    }

    async fn get_active_user_lock(&self, user_id: &str) -> Result<Option<LockedUser>, sqlx::Error> {
        let users = self.locked_users.read().await;
        Ok(users.iter().find(|u| u.user_id == user_id && u.is_active).cloned())
    }

    async fn get_locked_users(&self, limit: i64, offset: i64) -> Result<Vec<LockedUser>, sqlx::Error> {
        let users = self.locked_users.read().await;
        let active: Vec<_> = users.iter().filter(|u| u.is_active).cloned().collect();
        if offset as usize >= active.len() {
            return Ok(vec![]);
        }
        let start = offset as usize;
        let end = (offset + limit).min(active.len() as i64) as usize;
        Ok(active[start..end.min(active.len())].to_vec())
    }

    // ---- query methods (stubs) ----

    async fn get_user_by_id(&self, _user_id: &str) -> Result<Option<User>, sqlx::Error> {
        Ok(None)
    }

    async fn get_user_by_username(&self, _username: &str) -> Result<Option<User>, sqlx::Error> {
        Ok(None)
    }

    async fn get_user_by_email(&self, _email: &str) -> Result<Option<User>, sqlx::Error> {
        Ok(None)
    }

    async fn get_user_by_identifier(&self, _identifier: &str) -> Result<Option<User>, sqlx::Error> {
        Ok(None)
    }

    async fn get_users_paginated(
        &self,
        _limit: i64,
        _since_ts: Option<i64>,
        _since_user_id: Option<&str>,
    ) -> Result<Vec<User>, sqlx::Error> {
        Ok(vec![])
    }

    async fn list_users(
        &self,
        _limit: i64,
        _from_ts: Option<i64>,
        _from_user_id: Option<&str>,
        _name_filter: Option<&str>,
    ) -> Result<Vec<User>, sqlx::Error> {
        Ok(vec![])
    }

    async fn user_exists(&self, _user_id: &str) -> Result<bool, sqlx::Error> {
        Ok(false)
    }

    async fn filter_existing_users(&self, _user_ids: &[String]) -> Result<Vec<String>, sqlx::Error> {
        Ok(vec![])
    }

    async fn get_user_count(&self) -> Result<i64, sqlx::Error> {
        Ok(0)
    }

    async fn get_daily_active_users(&self) -> Result<i64, sqlx::Error> {
        Ok(0)
    }

    async fn get_monthly_active_users(&self) -> Result<i64, sqlx::Error> {
        Ok(0)
    }

    async fn get_r30_users(&self) -> Result<i64, sqlx::Error> {
        Ok(0)
    }

    // ---- mutation methods (stubs) ----

    async fn create_user(
        &self,
        _user_id: &str,
        _username: &str,
        _password_hash: Option<&str>,
        _is_admin: bool,
    ) -> Result<User, sqlx::Error> {
        Err(sqlx::Error::WorkerCrashed)
    }

    async fn update_password(&self, _user_id: &str, _password_hash: &str) -> Result<(), sqlx::Error> {
        Ok(())
    }

    async fn update_displayname(&self, _user_id: &str, _displayname: Option<&str>) -> Result<(), sqlx::Error> {
        Ok(())
    }

    async fn update_avatar_url(&self, _user_id: &str, _avatar_url: Option<&str>) -> Result<(), sqlx::Error> {
        Ok(())
    }

    async fn set_deactivation_status(&self, _user_id: &str, _is_deactivated: bool) -> Result<bool, sqlx::Error> {
        Ok(true)
    }

    async fn set_admin_status(&self, _user_id: &str, _is_admin: bool) -> Result<(), sqlx::Error> {
        Ok(())
    }

    async fn set_shadow_ban(&self, _user_id: &str, _is_shadow_banned: bool) -> Result<bool, sqlx::Error> {
        Ok(true)
    }

    async fn delete_user(&self, _user_id: &str) -> Result<(), sqlx::Error> {
        Ok(())
    }

    async fn set_guest_status(&self, _user_id: &str, _is_guest: bool) -> Result<(), sqlx::Error> {
        Ok(())
    }

    async fn set_user_type(&self, _user_id: &str, _user_type: Option<&str>) -> Result<(), sqlx::Error> {
        Ok(())
    }

    async fn upgrade_guest_account(
        &self,
        _user_id: &str,
        _username: &str,
        _password_hash: &str,
    ) -> Result<(), sqlx::Error> {
        Ok(())
    }

    // ---- stats / search methods (stubs) ----

    async fn get_user_stats_summary(&self) -> Result<UserStatsSummary, sqlx::Error> {
        Ok(UserStatsSummary { total_users: 0, active_users: 0, admin_users: 0, deactivated_users: 0, guest_users: 0 })
    }

    async fn count_sent_messages(&self, _user_id: &str) -> Result<i64, sqlx::Error> {
        Ok(0)
    }

    async fn search_users(&self, _query: &str, _limit: i64) -> Result<Vec<UserSearchResult>, sqlx::Error> {
        Ok(vec![])
    }

    async fn search_directory_users(
        &self,
        _query: &str,
        _limit: i64,
        _exact_only: bool,
    ) -> Result<Vec<UserDirectorySearchResult>, sqlx::Error> {
        Ok(vec![])
    }

    async fn get_user_profile(&self, _user_id: &str) -> Result<Option<UserProfile>, sqlx::Error> {
        Ok(None)
    }

    async fn get_user_profiles_batch(&self, _user_ids: &[String]) -> Result<Vec<UserProfile>, sqlx::Error> {
        Ok(vec![])
    }

    async fn get_user_profiles_map(&self, _user_ids: &[String]) -> Result<HashMap<String, UserProfile>, sqlx::Error> {
        Ok(HashMap::new())
    }

    async fn get_users_batch(&self, _user_ids: &[String]) -> Result<Vec<User>, sqlx::Error> {
        Ok(vec![])
    }

    async fn get_users_map(&self, _user_ids: &[String]) -> Result<HashMap<String, User>, sqlx::Error> {
        Ok(HashMap::new())
    }

    async fn get_account_data_content(
        &self,
        _user_id: &str,
        _data_type: &str,
    ) -> Result<Option<serde_json::Value>, sqlx::Error> {
        Ok(None)
    }

    async fn upsert_account_data_content(
        &self,
        _user_id: &str,
        _data_type: &str,
        _content: &serde_json::Value,
    ) -> Result<(), sqlx::Error> {
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_lock_and_check_user() {
        let store = FakeUserStore::new();
        let now = 1000;

        assert!(!store.is_user_locked("@alice:example.com").await.unwrap());

        let locked = store.lock_user("@alice:example.com", Some("spam"), "admin", now).await.unwrap();
        assert!(locked.is_active);
        assert_eq!(locked.user_id, "@alice:example.com");
        assert_eq!(locked.reason, Some("spam".to_string()));

        assert!(store.is_user_locked("@alice:example.com").await.unwrap());
    }

    #[tokio::test]
    async fn test_unlock_user() {
        let store = FakeUserStore::new();
        let now = 1000;

        store.lock_user("@alice:example.com", None, "admin", now).await.unwrap();
        assert!(store.is_user_locked("@alice:example.com").await.unwrap());

        store.unlock_user("@alice:example.com", now + 100).await.unwrap();
        assert!(!store.is_user_locked("@alice:example.com").await.unwrap());
    }

    #[tokio::test]
    async fn test_get_active_user_lock_returns_none_after_unlock() {
        let store = FakeUserStore::new();
        let now = 1000;

        store.lock_user("@bob:example.com", None, "admin", now).await.unwrap();
        let lock = store.get_active_user_lock("@bob:example.com").await.unwrap();
        assert!(lock.is_some());

        store.unlock_user("@bob:example.com", now + 1).await.unwrap();
        let lock = store.get_active_user_lock("@bob:example.com").await.unwrap();
        assert!(lock.is_none());
    }

    #[tokio::test]
    async fn test_get_locked_users_pagination() {
        let store = FakeUserStore::new();
        let now = 1000;

        store.lock_user("@a:example.com", None, "admin", now).await.unwrap();
        store.lock_user("@b:example.com", None, "admin", now).await.unwrap();
        store.lock_user("@c:example.com", None, "admin", now).await.unwrap();

        let page = store.get_locked_users(2, 0).await.unwrap();
        assert_eq!(page.len(), 2);

        let page2 = store.get_locked_users(2, 2).await.unwrap();
        assert_eq!(page2.len(), 1);
    }

    #[tokio::test]
    async fn test_get_locked_users_empty_when_offset_beyond_range() {
        let store = FakeUserStore::new();
        let now = 1000;

        store.lock_user("@a:example.com", None, "admin", now).await.unwrap();

        // offset >= active.len() should return empty, not panic
        let result = store.get_locked_users(10, 1).await.unwrap();
        assert!(result.is_empty());

        let result = store.get_locked_users(10, 100).await.unwrap();
        assert!(result.is_empty());
    }

    #[test]
    fn test_locked_user_fields() {
        let user = LockedUser {
            id: 42,
            user_id: "@alice:example.com".to_string(),
            reason: Some("spam".to_string()),
            locked_by: "admin".to_string(),
            created_ts: 1000,
            unlocked_ts: None,
            is_active: true,
        };
        assert_eq!(user.id, 42);
        assert_eq!(user.user_id, "@alice:example.com");
        assert_eq!(user.reason, Some("spam".to_string()));
        assert_eq!(user.locked_by, "admin");
        assert_eq!(user.created_ts, 1000);
        assert_eq!(user.unlocked_ts, None);
        assert!(user.is_active);
    }

    #[test]
    fn test_locked_user_clone() {
        let user = LockedUser {
            id: 1,
            user_id: "@bob:example.com".to_string(),
            reason: None,
            locked_by: "mod".to_string(),
            created_ts: 2000,
            unlocked_ts: None,
            is_active: true,
        };
        let cloned = user.clone();
        assert_eq!(cloned.id, user.id);
        assert_eq!(cloned.user_id, user.user_id);
        assert_eq!(cloned.reason, user.reason);
        assert_eq!(cloned.locked_by, user.locked_by);
        assert_eq!(cloned.created_ts, user.created_ts);
        assert_eq!(cloned.unlocked_ts, user.unlocked_ts);
        assert_eq!(cloned.is_active, user.is_active);
    }

    #[test]
    fn test_locked_user_unlocked_state() {
        let user = LockedUser {
            id: 99,
            user_id: "@carol:example.com".to_string(),
            reason: Some("inactive".to_string()),
            locked_by: "system".to_string(),
            created_ts: 500,
            unlocked_ts: Some(600),
            is_active: false,
        };
        assert_eq!(user.id, 99);
        assert!(!user.is_active);
        assert_eq!(user.unlocked_ts, Some(600));
        // unlocked_ts is set when is_active is false (post-unlock state)
        assert!(user.unlocked_ts.unwrap() > user.created_ts);
    }
}
