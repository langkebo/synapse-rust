use async_trait::async_trait;
use std::collections::{HashMap, HashSet};
use std::sync::Arc;
use tokio::sync::RwLock;

use crate::user::{
    NewRegistration, User, UserDirectorySearchResult, UserProfile, UserSearchResult, UserStatsSummary, UserStore,
};

/// In-memory adapter for UserStore — used in unit tests.
#[derive(Clone)]
pub struct FakeUserStore {
    users: Arc<RwLock<HashMap<String, User>>>,
    pool: Arc<sqlx::PgPool>,
}

/// 满足 [`UserStore::pool`] 签名的**惰性、永不连接**的连接池（CODE-05）。
///
/// [`UserStore::pool`] 返回 `&Arc<PgPool>` 而不是 `Result`，所以「本适配器没有数据库」
/// 只能这样表达：真正 `acquire()` 时得到 `sqlx::Error`，而不是让调用方 panic。
///
/// `max_lifetime` / `idle_timeout` 必须显式关掉：`PoolInner::new_arc` 只有在
/// `max_lifetime`、`idle_timeout` 至少一个有值（默认都是 `Some`），或
/// `min_connections > 0` 时才会 `rt::spawn` 后台维护任务，而
/// `FakeUserStore::new()` 常在**没有 Tokio runtime** 的同步上下文里被调用。
fn lazy_unconnected_pool() -> Arc<sqlx::PgPool> {
    let pool = sqlx::postgres::PgPoolOptions::new()
        .max_lifetime(None)
        .idle_timeout(None)
        .connect_lazy_with(sqlx::postgres::PgConnectOptions::new_without_pgpass());
    Arc::new(pool)
}

impl Default for FakeUserStore {
    /// 与 `#[derive(Default)]` 时的行为一致：空 store，不预置任何用户。
    fn default() -> Self {
        Self { users: Arc::new(RwLock::new(HashMap::new())), pool: lazy_unconnected_pool() }
    }
}

impl FakeUserStore {
    /// See [`new`].
    pub fn new() -> Self {
        let mut users = HashMap::new();
        // Seed a default test user so admin/security tests have a target.
        users.insert(
            "@alice:example.com".to_string(),
            User {
                user_id: "@alice:example.com".to_string(),
                username: "alice".to_string(),
                password_hash: None,
                is_admin: false,
                is_guest: false,
                is_shadow_banned: false,
                is_deactivated: false,
                created_ts: 0,
                updated_ts: None,
                displayname: None,
                avatar_url: None,
                email: None,
                phone: None,
                generation: None,
                consent_version: None,
                appservice_id: None,
                user_type: None,
                invalid_update_at: None,
                migration_state: None,
                password_changed_ts: None,
                failed_login_attempts: 0,
                locked_until: None,
            },
        );
        Self { users: Arc::new(RwLock::new(users)), pool: lazy_unconnected_pool() }
    }

    /// Insert (or replace) a user in the in-memory store for test setup.
    pub async fn seed_user(&self, user: User) {
        self.users.write().await.insert(user.user_id.clone(), user);
    }
}

#[async_trait]
impl UserStore for FakeUserStore {
    fn pool(&self) -> &Arc<sqlx::PgPool> {
        &self.pool
    }

    // ---- query methods (stubs) ----

    async fn get_user_by_id(&self, user_id: &str) -> Result<Option<User>, sqlx::Error> {
        Ok(self.users.read().await.get(user_id).cloned())
    }

    async fn get_user_by_username(&self, _username: &str) -> Result<Option<User>, sqlx::Error> {
        Ok(None)
    }

    async fn get_user_by_email(&self, _email: &str) -> Result<Option<User>, sqlx::Error> {
        Ok(None)
    }

    async fn get_user_by_identifier(&self, identifier: &str) -> Result<Option<User>, sqlx::Error> {
        if identifier.starts_with('@') && identifier.contains(':') {
            self.get_user_by_id(identifier).await
        } else {
            self.get_user_by_username(identifier).await
        }
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

    async fn user_exists(&self, user_id: &str) -> Result<bool, sqlx::Error> {
        Ok(self.users.read().await.contains_key(user_id))
    }

    async fn active_user_exists(&self, user_id: &str) -> Result<bool, sqlx::Error> {
        Ok(self.users.read().await.get(user_id).is_some_and(|user| !user.is_deactivated))
    }

    async fn filter_existing_users(&self, _user_ids: &[String]) -> Result<Vec<String>, sqlx::Error> {
        Ok(vec![])
    }

    async fn get_user_count(&self) -> Result<i64, sqlx::Error> {
        Ok(0)
    }

    async fn count_users_matching(&self, name_filter: Option<&str>) -> Result<i64, sqlx::Error> {
        // B-6 mock: mirror the Postgres LIKE '%...%' semantics against the
        // in-memory map.  Tests that want exact parity can rely on this.
        let users = self.users.read().await;
        if let Some(pat) = name_filter {
            Ok(users.values().filter(|u| u.username.contains(pat)).count() as i64)
        } else {
            Ok(users.len() as i64)
        }
    }

    async fn count_non_deactivated_users(&self) -> Result<i64, sqlx::Error> {
        let users = self.users.read().await;
        Ok(users.values().filter(|u| !u.is_deactivated).count() as i64)
    }

    async fn count_non_deactivated_users_by_app_service(&self) -> Result<HashMap<String, i64>, sqlx::Error> {
        let users = self.users.read().await;
        let mut map: HashMap<String, i64> = HashMap::new();
        for user in users.values() {
            if user.is_deactivated {
                continue;
            }
            let key = user.appservice_id.clone().unwrap_or_default();
            *map.entry(key).or_insert(0) += 1;
        }
        Ok(map)
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
        user_id: &str,
        username: &str,
        password_hash: Option<&str>,
        is_admin: bool,
    ) -> Result<User, sqlx::Error> {
        // 检测用户名重复（与 Postgres 唯一约束行为一致），供 register 的
        // M_USER_IN_USE 路径测试使用。
        {
            let users = self.users.read().await;
            if users.values().any(|u| u.username == username) {
                return Err(sqlx::Error::Protocol(
                    "duplicate key value violates unique constraint \"users_username_key\"".to_string(),
                ));
            }
        }

        let user = User {
            user_id: user_id.to_string(),
            username: username.to_string(),
            password_hash: password_hash.map(|s| s.to_string()),
            is_admin,
            is_guest: false,
            is_shadow_banned: false,
            is_deactivated: false,
            created_ts: 0,
            updated_ts: None,
            displayname: None,
            avatar_url: None,
            email: None,
            phone: None,
            generation: None,
            consent_version: None,
            appservice_id: None,
            user_type: None,
            invalid_update_at: None,
            migration_state: None,
            password_changed_ts: None,
            failed_login_attempts: 0,
            locked_until: None,
        };
        self.users.write().await.insert(user_id.to_string(), user.clone());
        Ok(user)
    }

    async fn create_registration(&self, registration: NewRegistration) -> Result<User, sqlx::Error> {
        // 内存实现没有事务语义：按序写入，仅供单测使用。
        let mut user = self
            .create_user(
                &registration.user_id,
                &registration.username,
                registration.password_hash.as_deref(),
                registration.is_admin,
            )
            .await?;
        if let Some(name) = registration.displayname {
            user.displayname = Some(name);
            self.users.write().await.insert(user.user_id.clone(), user.clone());
        }
        Ok(user)
    }

    async fn ensure_remote_user(&self, user_id: &str) -> Result<(), sqlx::Error> {
        if !self.users.read().await.contains_key(user_id) {
            self.create_user(
                user_id,
                user_id.strip_prefix('@').and_then(|u| u.split(':').next()).unwrap_or(user_id),
                None,
                false,
            )
            .await?;
        }
        Ok(())
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

    /// MSC4262 mock: upsert profile into the in-memory store, mirroring Postgres UPDATE-only semantics.
    /// Returns `true` if the user existed and was updated, `false` otherwise.
    async fn apply_profile_update_from_federation(
        &self,
        user_id: &str,
        displayname: Option<&str>,
        avatar_url: Option<&str>,
    ) -> Result<bool, sqlx::Error> {
        let mut users = self.users.write().await;
        if let Some(user) = users.get_mut(user_id) {
            if let Some(name) = displayname {
                user.displayname = Some(name.to_string());
            }
            if let Some(url) = avatar_url {
                user.avatar_url = Some(url.to_string());
            }
            Ok(true)
        } else {
            // Unknown remote user — no placeholder, consistent with Postgres UPDATE-only.
            Ok(false)
        }
    }

    async fn set_deactivation_status(&self, user_id: &str, is_deactivated: bool) -> Result<bool, sqlx::Error> {
        // Mirror real Postgres semantics: update the in-memory flag and report
        // whether a row was affected. Keeps deactivate_user() tests honest.
        let mut users = self.users.write().await;
        if let Some(user) = users.get_mut(user_id) {
            user.is_deactivated = is_deactivated;
            Ok(true)
        } else {
            Ok(false)
        }
    }

    async fn set_deactivation_status_batch(
        &self,
        user_ids: &[String],
        _is_deactivated: bool,
    ) -> Result<HashSet<String>, sqlx::Error> {
        // The fake always succeeds for all provided user IDs.
        Ok(user_ids.iter().cloned().collect())
    }

    async fn set_admin_status(&self, _user_id: &str, _is_admin: bool) -> Result<(), sqlx::Error> {
        Ok(())
    }

    async fn set_shadow_ban(&self, user_id: &str, is_shadow_banned: bool) -> Result<bool, sqlx::Error> {
        let mut users = self.users.write().await;
        if let Some(user) = users.get_mut(user_id) {
            user.is_shadow_banned = is_shadow_banned;
            Ok(true)
        } else {
            Ok(false)
        }
    }

    async fn delete_user(&self, user_id: &str) -> Result<(), sqlx::Error> {
        // Mirror real Postgres semantics: remove the row so callers that rely
        // on a subsequent get_user_by_id returning None (e.g. validate_token's
        // user-not-found path) can be exercised in unit tests.
        self.users.write().await.remove(user_id);
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

    async fn get_user_profiles_updated_since(
        &self,
        _user_ids: &[String],
        _since_ts: i64,
    ) -> Result<HashMap<String, UserProfile>, sqlx::Error> {
        Ok(HashMap::new())
    }

    async fn get_users_batch(&self, user_ids: &[String]) -> Result<Vec<User>, sqlx::Error> {
        let users = self.users.read().await;
        Ok(user_ids.iter().filter_map(|user_id| users.get(user_id).cloned()).collect())
    }

    async fn get_users_map(&self, _user_ids: &[String]) -> Result<HashMap<String, User>, sqlx::Error> {
        Ok(HashMap::new())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::user::UserStore;

    fn make_user(user_id: &str, is_deactivated: bool) -> User {
        User {
            user_id: user_id.to_string(),
            username: user_id.trim_start_matches('@').to_string(),
            password_hash: None,
            is_admin: false,
            is_guest: false,
            is_shadow_banned: false,
            is_deactivated,
            created_ts: 0,
            updated_ts: None,
            displayname: None,
            avatar_url: None,
            email: None,
            phone: None,
            generation: None,
            consent_version: None,
            appservice_id: None,
            user_type: None,
            invalid_update_at: None,
            migration_state: None,
            password_changed_ts: None,
            failed_login_attempts: 0,
            locked_until: None,
        }
    }

    #[tokio::test]
    async fn count_non_deactivated_users_returns_only_active_count() {
        let store = FakeUserStore::new();
        // FakeUserStore::new seeds @alice:example.com (not deactivated) → 1 active.
        store.seed_user(make_user("@bob:example.com", false)).await;
        store.seed_user(make_user("@carol:example.com", true)).await;
        store.seed_user(make_user("@dave:example.com", true)).await;

        let count = store.count_non_deactivated_users().await.expect("count should succeed");
        assert_eq!(count, 2, "only non-deactivated users should be counted (alice + bob)");
    }

    #[tokio::test]
    async fn count_non_deactivated_users_zero_when_all_deactivated() {
        let store = FakeUserStore::new();
        // Override the seeded alice to be deactivated.
        store.seed_user(make_user("@alice:example.com", true)).await;

        let count = store.count_non_deactivated_users().await.expect("count should succeed");
        assert_eq!(count, 0);
    }

    fn make_user_with_appservice(user_id: &str, is_deactivated: bool, appservice_id: Option<&str>) -> User {
        let mut user = make_user(user_id, is_deactivated);
        user.appservice_id = appservice_id.map(|s| s.to_string());
        user
    }

    #[tokio::test]
    async fn count_non_deactivated_users_by_app_service_groups_correctly() {
        let store = FakeUserStore::new();
        // FakeUserStore::new seeds @alice:example.com (not deactivated, appservice_id=None) → 1 local.
        // AS "as1": 2 active, 1 deactivated → 2 counted
        store.seed_user(make_user_with_appservice("@bob:example.com", false, Some("as1"))).await;
        store.seed_user(make_user_with_appservice("@carol:example.com", false, Some("as1"))).await;
        store.seed_user(make_user_with_appservice("@dave:example.com", true, Some("as1"))).await;
        // AS "as2": 1 active, 1 deactivated → 1 counted
        store.seed_user(make_user_with_appservice("@eve:example.com", false, Some("as2"))).await;
        store.seed_user(make_user_with_appservice("@frank:example.com", true, Some("as2"))).await;
        // Local (appservice_id=None): alice (active) + gina (active) → 2 counted
        store.seed_user(make_user_with_appservice("@gina:example.com", false, None)).await;

        let counts =
            store.count_non_deactivated_users_by_app_service().await.expect("count by app service should succeed");

        // Local users (appservice_id=NULL) are grouped under empty string key.
        assert_eq!(counts.get("").copied(), Some(2), "local non-deactivated: alice + gina");
        assert_eq!(counts.get("as1").copied(), Some(2), "as1 non-deactivated: bob + carol");
        assert_eq!(counts.get("as2").copied(), Some(1), "as2 non-deactivated: eve");
        // Total should match count_non_deactivated_users.
        let total: i64 = counts.values().sum();
        assert_eq!(total, 5, "sum of by-app-service counts should equal total non-deactivated users");
    }

    #[tokio::test]
    async fn count_non_deactivated_users_by_app_service_empty_when_all_deactivated() {
        let store = FakeUserStore::new();
        // Override seeded alice to be deactivated.
        store.seed_user(make_user_with_appservice("@alice:example.com", true, None)).await;
        store.seed_user(make_user_with_appservice("@bob:example.com", true, Some("as1"))).await;

        let counts =
            store.count_non_deactivated_users_by_app_service().await.expect("count by app service should succeed");

        // No groups should be present because all users are deactivated.
        assert!(counts.is_empty(), "no non-deactivated users → empty map");
    }

    // ── B-6: count_users_matching coverage ────────────────────────────────

    #[tokio::test]
    async fn b6_count_users_matching_none_returns_full_table_count() {
        let store = FakeUserStore::new();
        // FakeUserStore::new seeds @alice:example.com.
        store.seed_user(make_user("@bob:example.com", false)).await;
        store.seed_user(make_user("@carol:example.com", false)).await;

        let count = store.count_users_matching(None).await.expect("count should succeed");
        assert_eq!(count, 3, "no filter ⇒ all users counted (alice + bob + carol)");
    }

    #[tokio::test]
    async fn b6_count_users_matching_substring_filters_by_username() {
        let store = FakeUserStore::new();
        // Override the seeded alice to control the test fully.
        store.seed_user(make_user("@alice:example.com", false)).await;
        store.seed_user(make_user("@alicia:example.com", false)).await;
        store.seed_user(make_user("@bob:example.com", false)).await;
        store.seed_user(make_user("@carol:example.com", false)).await;

        // Substring "ali" must match both alice and alicia, and nothing else.
        let count = store.count_users_matching(Some("ali")).await.expect("count should succeed");
        assert_eq!(count, 2, "substring 'ali' matches alice + alicia, not bob/carol");
    }

    #[tokio::test]
    async fn b6_count_users_matching_no_match_returns_zero() {
        let store = FakeUserStore::new();
        store.seed_user(make_user("@alice:example.com", false)).await;

        let count = store.count_users_matching(Some("zzz")).await.expect("count should succeed");
        assert_eq!(count, 0, "no match ⇒ zero");
    }
}
