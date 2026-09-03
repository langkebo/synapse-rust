// User service unit tests — exercises `synapse_services::user_service::UserService`.
//
// `UserService` is a thin convenience layer over `UserStore` that maps
// `sqlx::Error` → `ApiError` and bundles common multi-step patterns. These
// tests cover:
//   * Happy-path delegation for lookup methods (get_user, get_user_by_email,
//     user_exists, etc.).
//   * `get_user_or_not_found` returns `Ok(user)` when found, `Err(not_found)`
//     when missing.
//   * `ensure_user_exists` returns `Ok(())` when found, `Err(not_found)` when
//     missing.
//   * `get_profile` builds the expected JSON shape.
//   * `get_profiles_batch` maps storage profiles to JSON.
//   * `update_displayname` / `update_avatar_url` map "too long" errors to
//     `bad_request` and other errors to `internal`.
//   * `update_profile` delegates to both setters.
//   * Error-path: every method that returns `Err` from storage surfaces as
//     `ApiError::internal`.
//
// Mock: `FakeUserStore` from `synapse_storage` provides an in-memory
// `UserStore` implementation. We extend it with a custom wrapper that can
// inject errors for the error-path tests.

use std::collections::{HashMap, HashSet};
use std::sync::{Arc, Mutex};

use async_trait::async_trait;

use synapse_services::user_service::UserService;
use synapse_storage::user::{
    LockedUser, User, UserDirectorySearchResult, UserProfile, UserSearchResult, UserStatsSummary, UserStore,
};

// ─────────────────────────────────────────────────────────────────────────────
// Mock UserStore
// ─────────────────────────────────────────────────────────────────────────────

/// Configurable in-memory `UserStore` fake. Each method can be set to return
/// an error via `set_fail_all(true)`, and `update_displayname` / `update_avatar_url`
/// can be set to return a "too long" error to exercise the bad_request mapping.
#[derive(Default, Clone)]
struct MockUserStore {
    users: Arc<Mutex<HashMap<String, User>>>,
    fail_all: Arc<Mutex<bool>>,
    fail_with_too_long: Arc<Mutex<bool>>,
}

impl MockUserStore {
    fn new() -> Self {
        Self::default()
    }

    /// Build a mock with a single seeded user.
    fn with_user(user: User) -> Self {
        let store = Self::new();
        store.users.lock().unwrap().insert(user.user_id.clone(), user);
        store
    }

    fn set_fail_all(&self, fail: bool) {
        *self.fail_all.lock().unwrap() = fail;
    }

    fn set_fail_with_too_long(&self, fail: bool) {
        *self.fail_with_too_long.lock().unwrap() = fail;
    }

    fn fail_all_check(&self) -> Result<(), sqlx::Error> {
        if *self.fail_all.lock().unwrap() {
            Err(sqlx::Error::PoolClosed)
        } else {
            Ok(())
        }
    }
}

fn make_user(user_id: &str, displayname: Option<&str>, avatar_url: Option<&str>) -> User {
    User {
        user_id: user_id.to_string(),
        username: user_id.trim_start_matches('@').to_string(),
        password_hash: None,
        is_admin: false,
        is_guest: false,
        is_shadow_banned: false,
        is_deactivated: false,
        created_ts: 1_700_000_000_000,
        updated_ts: None,
        displayname: displayname.map(|s| s.to_string()),
        avatar_url: avatar_url.map(|s| s.to_string()),
        email: None,
        phone: None,
        generation: None,
        consent_version: None,
        appservice_id: None,
        user_type: None,
        invalid_update_at: None,
        migration_state: None,
        password_changed_ts: None,
        is_password_change_required: false,
        password_expires_at: None,
        failed_login_attempts: 0,
        locked_until: None,
        must_change_password: false,
    }
}

#[async_trait]
impl UserStore for MockUserStore {
    #[allow(clippy::unimplemented)]
    fn pool(&self) -> &Arc<sqlx::PgPool> {
        // UserService never calls this; return a leaked static reference would
        // be unsafe, so we panic to surface any accidental call.
        unimplemented!("MockUserStore does not provide a database pool")
    }

    async fn lock_user(
        &self,
        _user_id: &str,
        _reason: Option<&str>,
        _locked_by: &str,
        _now_ts: i64,
    ) -> Result<LockedUser, sqlx::Error> {
        self.fail_all_check()?;
        Err(sqlx::Error::WorkerCrashed)
    }

    async fn unlock_user(&self, _user_id: &str, _now_ts: i64) -> Result<(), sqlx::Error> {
        self.fail_all_check()?;
        Ok(())
    }

    async fn is_user_locked(&self, _user_id: &str) -> Result<bool, sqlx::Error> {
        self.fail_all_check()?;
        Ok(false)
    }

    async fn get_active_user_lock(&self, _user_id: &str) -> Result<Option<LockedUser>, sqlx::Error> {
        self.fail_all_check()?;
        Ok(None)
    }

    async fn get_locked_users(&self, _limit: i64, _offset: i64) -> Result<Vec<LockedUser>, sqlx::Error> {
        self.fail_all_check()?;
        Ok(Vec::new())
    }

    async fn get_user_by_id(&self, user_id: &str) -> Result<Option<User>, sqlx::Error> {
        self.fail_all_check()?;
        Ok(self.users.lock().unwrap().get(user_id).cloned())
    }

    async fn get_user_by_username(&self, _username: &str) -> Result<Option<User>, sqlx::Error> {
        self.fail_all_check()?;
        Ok(None)
    }

    async fn get_user_by_email(&self, _email: &str) -> Result<Option<User>, sqlx::Error> {
        self.fail_all_check()?;
        Ok(None)
    }

    async fn get_user_by_identifier(&self, identifier: &str) -> Result<Option<User>, sqlx::Error> {
        self.fail_all_check()?;
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
        self.fail_all_check()?;
        Ok(self.users.lock().unwrap().values().cloned().collect())
    }

    async fn list_users(
        &self,
        _limit: i64,
        _from_ts: Option<i64>,
        _from_user_id: Option<&str>,
        _name_filter: Option<&str>,
    ) -> Result<Vec<User>, sqlx::Error> {
        self.fail_all_check()?;
        Ok(self.users.lock().unwrap().values().cloned().collect())
    }

    async fn user_exists(&self, user_id: &str) -> Result<bool, sqlx::Error> {
        self.fail_all_check()?;
        Ok(self.users.lock().unwrap().contains_key(user_id))
    }

    async fn filter_existing_users(&self, user_ids: &[String]) -> Result<Vec<String>, sqlx::Error> {
        self.fail_all_check()?;
        let users = self.users.lock().unwrap();
        Ok(user_ids.iter().filter(|id| users.contains_key(*id)).cloned().collect())
    }

    async fn get_user_count(&self) -> Result<i64, sqlx::Error> {
        self.fail_all_check()?;
        Ok(self.users.lock().unwrap().len() as i64)
    }

    async fn count_non_deactivated_users(&self) -> Result<i64, sqlx::Error> {
        self.fail_all_check()?;
        Ok(self.users.lock().unwrap().values().filter(|u| !u.is_deactivated).count() as i64)
    }

    async fn count_non_deactivated_users_by_app_service(&self) -> Result<HashMap<String, i64>, sqlx::Error> {
        self.fail_all_check()?;
        Ok(HashMap::new())
    }

    async fn get_daily_active_users(&self) -> Result<i64, sqlx::Error> {
        self.fail_all_check()?;
        Ok(0)
    }

    async fn get_monthly_active_users(&self) -> Result<i64, sqlx::Error> {
        self.fail_all_check()?;
        Ok(0)
    }

    async fn get_r30_users(&self) -> Result<i64, sqlx::Error> {
        self.fail_all_check()?;
        Ok(0)
    }

    async fn create_user(
        &self,
        _user_id: &str,
        _username: &str,
        _password_hash: Option<&str>,
        _is_admin: bool,
    ) -> Result<User, sqlx::Error> {
        self.fail_all_check()?;
        Err(sqlx::Error::WorkerCrashed)
    }

    async fn update_password(&self, _user_id: &str, _password_hash: &str) -> Result<(), sqlx::Error> {
        self.fail_all_check()?;
        Ok(())
    }

    async fn update_displayname(&self, _user_id: &str, _displayname: Option<&str>) -> Result<(), sqlx::Error> {
        self.fail_all_check()?;
        if *self.fail_with_too_long.lock().unwrap() {
            // Mimic the storage layer's "too long" validation error so the
            // service can map it to bad_request. The service checks
            // `e.to_string().contains("too long")`.
            return Err(sqlx::Error::Decode(Box::new(std::io::Error::new(
                std::io::ErrorKind::InvalidData,
                "value too long for column displayname (max 255)",
            ))));
        }
        Ok(())
    }

    async fn update_avatar_url(&self, _user_id: &str, _avatar_url: Option<&str>) -> Result<(), sqlx::Error> {
        self.fail_all_check()?;
        if *self.fail_with_too_long.lock().unwrap() {
            return Err(sqlx::Error::Decode(Box::new(std::io::Error::new(
                std::io::ErrorKind::InvalidData,
                "value too long for column avatar_url (max 255)",
            ))));
        }
        Ok(())
    }

    async fn set_deactivation_status(&self, _user_id: &str, _is_deactivated: bool) -> Result<bool, sqlx::Error> {
        self.fail_all_check()?;
        Ok(true)
    }

    async fn set_deactivation_status_batch(
        &self,
        user_ids: &[String],
        is_deactivated: bool,
    ) -> Result<HashSet<String>, sqlx::Error> {
        self.fail_all_check()?;
        let mut users = self.users.lock().unwrap();
        let mut changed = HashSet::new();
        for id in user_ids {
            if let Some(u) = users.get_mut(id) {
                if u.is_deactivated != is_deactivated {
                    u.is_deactivated = is_deactivated;
                    changed.insert(id.clone());
                }
            }
        }
        Ok(changed)
    }

    async fn set_admin_status(&self, _user_id: &str, _is_admin: bool) -> Result<(), sqlx::Error> {
        self.fail_all_check()?;
        Ok(())
    }

    async fn set_shadow_ban(&self, _user_id: &str, _is_shadow_banned: bool) -> Result<bool, sqlx::Error> {
        self.fail_all_check()?;
        Ok(true)
    }

    async fn delete_user(&self, _user_id: &str) -> Result<(), sqlx::Error> {
        self.fail_all_check()?;
        Ok(())
    }

    async fn set_guest_status(&self, _user_id: &str, _is_guest: bool) -> Result<(), sqlx::Error> {
        self.fail_all_check()?;
        Ok(())
    }

    async fn set_user_type(&self, _user_id: &str, _user_type: Option<&str>) -> Result<(), sqlx::Error> {
        self.fail_all_check()?;
        Ok(())
    }

    async fn upgrade_guest_account(
        &self,
        _user_id: &str,
        _username: &str,
        _password_hash: &str,
    ) -> Result<(), sqlx::Error> {
        self.fail_all_check()?;
        Ok(())
    }

    async fn get_user_stats_summary(&self) -> Result<UserStatsSummary, sqlx::Error> {
        self.fail_all_check()?;
        Ok(UserStatsSummary { total_users: 0, active_users: 0, admin_users: 0, deactivated_users: 0, guest_users: 0 })
    }

    async fn count_sent_messages(&self, _user_id: &str) -> Result<i64, sqlx::Error> {
        self.fail_all_check()?;
        Ok(0)
    }

    async fn search_users(&self, _query: &str, _limit: i64) -> Result<Vec<UserSearchResult>, sqlx::Error> {
        self.fail_all_check()?;
        Ok(Vec::new())
    }

    async fn search_directory_users(
        &self,
        _query: &str,
        _limit: i64,
        _exact_only: bool,
    ) -> Result<Vec<UserDirectorySearchResult>, sqlx::Error> {
        self.fail_all_check()?;
        Ok(Vec::new())
    }

    async fn get_user_profile(&self, _user_id: &str) -> Result<Option<UserProfile>, sqlx::Error> {
        self.fail_all_check()?;
        Ok(None)
    }

    async fn get_user_profiles_batch(&self, user_ids: &[String]) -> Result<Vec<UserProfile>, sqlx::Error> {
        self.fail_all_check()?;
        let users = self.users.lock().unwrap();
        let mut profiles = Vec::new();
        for id in user_ids {
            if let Some(u) = users.get(id) {
                profiles.push(UserProfile {
                    user_id: u.user_id.clone(),
                    username: u.username.clone(),
                    displayname: u.displayname.clone(),
                    avatar_url: u.avatar_url.clone(),
                    created_ts: u.created_ts,
                });
            }
        }
        Ok(profiles)
    }

    async fn get_user_profiles_map(&self, _user_ids: &[String]) -> Result<HashMap<String, UserProfile>, sqlx::Error> {
        self.fail_all_check()?;
        Ok(HashMap::new())
    }

    async fn get_users_batch(&self, _user_ids: &[String]) -> Result<Vec<User>, sqlx::Error> {
        self.fail_all_check()?;
        Ok(Vec::new())
    }

    async fn get_users_map(&self, _user_ids: &[String]) -> Result<HashMap<String, User>, sqlx::Error> {
        self.fail_all_check()?;
        Ok(HashMap::new())
    }

    async fn get_account_data_content(
        &self,
        _user_id: &str,
        _data_type: &str,
    ) -> Result<Option<serde_json::Value>, sqlx::Error> {
        self.fail_all_check()?;
        Ok(None)
    }

    async fn upsert_account_data_content(
        &self,
        _user_id: &str,
        _data_type: &str,
        _content: &serde_json::Value,
    ) -> Result<(), sqlx::Error> {
        self.fail_all_check()?;
        Ok(())
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// Helpers
// ─────────────────────────────────────────────────────────────────────────────

fn build_service(store: MockUserStore) -> UserService {
    UserService::new(Arc::new(store) as Arc<dyn UserStore>)
}

fn build_service_with_user(user: User) -> UserService {
    build_service(MockUserStore::with_user(user))
}

// ─────────────────────────────────────────────────────────────────────────────
// get_user — happy + error path
// ─────────────────────────────────────────────────────────────────────────────

#[tokio::test]
async fn get_user_returns_some_when_user_exists() {
    let user = make_user("@alice:example.com", Some("Alice"), None);
    let svc = build_service_with_user(user);

    let result = svc.get_user("@alice:example.com").await.expect("should succeed");
    assert!(result.is_some());
    assert_eq!(result.unwrap().user_id, "@alice:example.com");
}

#[tokio::test]
async fn get_user_returns_none_when_user_missing() {
    let svc = build_service(MockUserStore::new());

    let result = svc.get_user("@nobody:example.com").await.expect("should succeed");
    assert!(result.is_none());
}

#[tokio::test]
async fn get_user_maps_storage_error_to_internal() {
    let store = MockUserStore::new();
    store.set_fail_all(true);
    let svc = build_service(store);

    let err = svc.get_user("@alice:example.com").await.expect_err("should propagate error");
    assert!(err.is_internal(), "storage error must surface as ApiError::internal");
}

// ─────────────────────────────────────────────────────────────────────────────
// get_user_by_identifier / username / email
// ─────────────────────────────────────────────────────────────────────────────

#[tokio::test]
async fn get_user_by_identifier_returns_user_for_user_id_format() {
    let user = make_user("@alice:example.com", None, None);
    let svc = build_service_with_user(user);

    let result = svc.get_user_by_identifier("@alice:example.com").await.expect("should succeed");
    assert!(result.is_some());
}

#[tokio::test]
async fn get_user_by_identifier_returns_none_for_username_format() {
    // MockUserStore::get_user_by_username always returns None.
    let svc = build_service(MockUserStore::new());
    let result = svc.get_user_by_identifier("alice").await.expect("should succeed");
    assert!(result.is_none());
}

#[tokio::test]
async fn get_user_by_username_returns_none_when_not_found() {
    let svc = build_service(MockUserStore::new());
    let result = svc.get_user_by_username("alice").await.expect("should succeed");
    assert!(result.is_none());
}

#[tokio::test]
async fn get_user_by_email_returns_none_when_not_found() {
    let svc = build_service(MockUserStore::new());
    let result = svc.get_user_by_email("alice@example.com").await.expect("should succeed");
    assert!(result.is_none());
}

#[tokio::test]
async fn get_user_by_identifier_maps_storage_error_to_internal() {
    let store = MockUserStore::new();
    store.set_fail_all(true);
    let svc = build_service(store);

    let err = svc.get_user_by_identifier("@alice:example.com").await.expect_err("should error");
    assert!(err.is_internal());
}

// ─────────────────────────────────────────────────────────────────────────────
// user_exists
// ─────────────────────────────────────────────────────────────────────────────

#[tokio::test]
async fn user_exists_returns_true_when_user_seeded() {
    let user = make_user("@alice:example.com", None, None);
    let svc = build_service_with_user(user);

    assert!(svc.user_exists("@alice:example.com").await.expect("should succeed"));
}

#[tokio::test]
async fn user_exists_returns_false_when_user_missing() {
    let svc = build_service(MockUserStore::new());

    assert!(!svc.user_exists("@nobody:example.com").await.expect("should succeed"));
}

#[tokio::test]
async fn user_exists_maps_storage_error_to_internal() {
    let store = MockUserStore::new();
    store.set_fail_all(true);
    let svc = build_service(store);

    let err = svc.user_exists("@alice:example.com").await.expect_err("should error");
    assert!(err.is_internal());
}

// ─────────────────────────────────────────────────────────────────────────────
// get_user_or_not_found
// ─────────────────────────────────────────────────────────────────────────────

#[tokio::test]
async fn get_user_or_not_found_returns_user_when_found() {
    let user = make_user("@alice:example.com", Some("Alice"), None);
    let svc = build_service_with_user(user);

    let result = svc.get_user_or_not_found("@alice:example.com").await.expect("should succeed");
    assert_eq!(result.user_id, "@alice:example.com");
    assert_eq!(result.displayname.as_deref(), Some("Alice"));
}

#[tokio::test]
async fn get_user_or_not_found_returns_not_found_error_when_missing() {
    let svc = build_service(MockUserStore::new());

    let err = svc.get_user_or_not_found("@nobody:example.com").await.expect_err("should be not_found");
    assert!(err.is_not_found(), "missing user must surface as not_found, got: {:?}", err);
}

// ─────────────────────────────────────────────────────────────────────────────
// ensure_user_exists
// ─────────────────────────────────────────────────────────────────────────────

#[tokio::test]
async fn ensure_user_exists_returns_ok_when_user_seeded() {
    let user = make_user("@alice:example.com", None, None);
    let svc = build_service_with_user(user);

    svc.ensure_user_exists("@alice:example.com").await.expect("should succeed");
}

#[tokio::test]
async fn ensure_user_exists_returns_not_found_when_missing() {
    let svc = build_service(MockUserStore::new());

    let err = svc.ensure_user_exists("@nobody:example.com").await.expect_err("should be not_found");
    assert!(err.is_not_found());
}

// ─────────────────────────────────────────────────────────────────────────────
// get_profile
// ─────────────────────────────────────────────────────────────────────────────

#[tokio::test]
async fn get_profile_returns_json_with_expected_fields() {
    let user = make_user("@alice:example.com", Some("Alice"), Some("mxc://example.com/avatar"));
    let svc = build_service_with_user(user);

    let profile = svc.get_profile("@alice:example.com").await.expect("should succeed");
    assert_eq!(profile["user_id"], "@alice:example.com");
    assert_eq!(profile["displayname"], "Alice");
    assert_eq!(profile["avatar_url"], "mxc://example.com/avatar");
}

#[tokio::test]
async fn get_profile_returns_none_fields_when_unset() {
    let user = make_user("@alice:example.com", None, None);
    let svc = build_service_with_user(user);

    let profile = svc.get_profile("@alice:example.com").await.expect("should succeed");
    assert_eq!(profile["user_id"], "@alice:example.com");
    assert!(profile["displayname"].is_null());
    assert!(profile["avatar_url"].is_null());
}

#[tokio::test]
async fn get_profile_returns_not_found_when_user_missing() {
    let svc = build_service(MockUserStore::new());

    let err = svc.get_profile("@nobody:example.com").await.expect_err("should be not_found");
    assert!(err.is_not_found());
}

// ─────────────────────────────────────────────────────────────────────────────
// get_profiles_batch
// ─────────────────────────────────────────────────────────────────────────────

#[tokio::test]
async fn get_profiles_batch_returns_profiles_for_seeded_users() {
    let store = MockUserStore::new();
    store
        .users
        .lock()
        .unwrap()
        .insert("@alice:example.com".to_string(), make_user("@alice:example.com", Some("Alice"), None));
    store
        .users
        .lock()
        .unwrap()
        .insert("@bob:example.com".to_string(), make_user("@bob:example.com", Some("Bob"), None));
    let svc = build_service(store);

    let profiles = svc
        .get_profiles_batch(&[
            "@alice:example.com".to_string(),
            "@bob:example.com".to_string(),
            "@missing:example.com".to_string(),
        ])
        .await
        .expect("should succeed");

    assert_eq!(profiles.len(), 2, "only seeded users should produce profiles");
    let ids: Vec<&str> = profiles.iter().map(|p| p["user_id"].as_str().unwrap()).collect();
    assert!(ids.contains(&"@alice:example.com"));
    assert!(ids.contains(&"@bob:example.com"));
}

#[tokio::test]
async fn get_profiles_batch_returns_empty_for_no_matches() {
    let svc = build_service(MockUserStore::new());

    let profiles = svc.get_profiles_batch(&["@nobody:example.com".to_string()]).await.expect("should succeed");

    assert!(profiles.is_empty());
}

#[tokio::test]
async fn get_profiles_batch_maps_storage_error_to_internal() {
    let store = MockUserStore::new();
    store.set_fail_all(true);
    let svc = build_service(store);

    let err = svc.get_profiles_batch(&["@alice:example.com".to_string()]).await.expect_err("should error");
    assert!(err.is_internal());
}

// ─────────────────────────────────────────────────────────────────────────────
// update_displayname / update_avatar_url — error mapping
// ─────────────────────────────────────────────────────────────────────────────

#[tokio::test]
async fn update_displayname_succeeds_when_storage_accepts() {
    let svc = build_service(MockUserStore::new());
    svc.update_displayname("@alice:example.com", Some("Alice")).await.expect("should succeed");
}

#[tokio::test]
async fn update_displayname_maps_too_long_error_to_bad_request() {
    let store = MockUserStore::new();
    store.set_fail_with_too_long(true);
    let svc = build_service(store);

    let long = "x".repeat(300);
    let err = svc.update_displayname("@alice:example.com", Some(&long)).await.expect_err("should error");
    assert!(err.is_bad_request(), "too-long displayname must surface as bad_request");
}

#[tokio::test]
async fn update_displayname_maps_generic_error_to_internal() {
    let store = MockUserStore::new();
    store.set_fail_all(true);
    let svc = build_service(store);

    let err = svc.update_displayname("@alice:example.com", Some("Alice")).await.expect_err("should error");
    assert!(err.is_internal());
}

#[tokio::test]
async fn update_avatar_url_succeeds_when_storage_accepts() {
    let svc = build_service(MockUserStore::new());
    svc.update_avatar_url("@alice:example.com", Some("mxc://example.com/x")).await.expect("should succeed");
}

#[tokio::test]
async fn update_avatar_url_maps_too_long_error_to_bad_request() {
    let store = MockUserStore::new();
    store.set_fail_with_too_long(true);
    let svc = build_service(store);

    let long = "x".repeat(300);
    let err = svc.update_avatar_url("@alice:example.com", Some(&long)).await.expect_err("should error");
    assert!(err.is_bad_request(), "too-long avatar_url must surface as bad_request");
}

#[tokio::test]
async fn update_avatar_url_maps_generic_error_to_internal() {
    let store = MockUserStore::new();
    store.set_fail_all(true);
    let svc = build_service(store);

    let err = svc.update_avatar_url("@alice:example.com", Some("mxc://x")).await.expect_err("should error");
    assert!(err.is_internal());
}

// ─────────────────────────────────────────────────────────────────────────────
// update_profile — delegates to both setters
// ─────────────────────────────────────────────────────────────────────────────

#[tokio::test]
async fn update_profile_with_both_fields_succeeds() {
    let svc = build_service(MockUserStore::new());
    svc.update_profile("@alice:example.com", Some("Alice"), Some("mxc://example.com/a")).await.expect("should succeed");
}

#[tokio::test]
async fn update_profile_with_only_displayname_succeeds() {
    let svc = build_service(MockUserStore::new());
    svc.update_profile("@alice:example.com", Some("Alice"), None).await.expect("should succeed");
}

#[tokio::test]
async fn update_profile_with_only_avatar_url_succeeds() {
    let svc = build_service(MockUserStore::new());
    svc.update_profile("@alice:example.com", None, Some("mxc://example.com/a")).await.expect("should succeed");
}

#[tokio::test]
async fn update_profile_with_neither_field_succeeds_as_noop() {
    let svc = build_service(MockUserStore::new());
    svc.update_profile("@alice:example.com", None, None).await.expect("should succeed");
}

#[tokio::test]
async fn update_profile_propagates_displayname_error() {
    let store = MockUserStore::new();
    store.set_fail_with_too_long(true);
    let svc = build_service(store);

    let long = "x".repeat(300);
    let err = svc.update_profile("@alice:example.com", Some(&long), None).await.expect_err("should error");
    assert!(err.is_bad_request());
}

#[tokio::test]
async fn update_profile_propagates_avatar_url_error() {
    let store = MockUserStore::new();
    store.set_fail_with_too_long(true);
    let svc = build_service(store);

    let long = "x".repeat(300);
    let err = svc.update_profile("@alice:example.com", None, Some(&long)).await.expect_err("should error");
    assert!(err.is_bad_request());
}

// ─────────────────────────────────────────────────────────────────────────────
// search_users / search_directory_users
// ─────────────────────────────────────────────────────────────────────────────

#[tokio::test]
async fn search_users_returns_empty_vec_when_no_matches() {
    let svc = build_service(MockUserStore::new());
    let result = svc.search_users("alice", 10).await.expect("should succeed");
    assert!(result.is_empty());
}

#[tokio::test]
async fn search_users_maps_storage_error_to_internal() {
    let store = MockUserStore::new();
    store.set_fail_all(true);
    let svc = build_service(store);
    let err = svc.search_users("alice", 10).await.expect_err("should error");
    assert!(err.is_internal());
}

#[tokio::test]
async fn search_directory_users_returns_empty_vec_when_no_matches() {
    let svc = build_service(MockUserStore::new());
    let result = svc.search_directory_users("alice", 10, false).await.expect("should succeed");
    assert!(result.is_empty());
}

#[tokio::test]
async fn search_directory_users_maps_storage_error_to_internal() {
    let store = MockUserStore::new();
    store.set_fail_all(true);
    let svc = build_service(store);
    let err = svc.search_directory_users("alice", 10, false).await.expect_err("should error");
    assert!(err.is_internal());
}

// ─────────────────────────────────────────────────────────────────────────────
// get_users_paginated / get_user_count / get_non_deactivated_user_count
// ─────────────────────────────────────────────────────────────────────────────

#[tokio::test]
async fn get_users_paginated_returns_seeded_users() {
    let store = MockUserStore::new();
    store.users.lock().unwrap().insert("@alice:example.com".to_string(), make_user("@alice:example.com", None, None));
    let svc = build_service(store);

    let users = svc.get_users_paginated(10, None, None).await.expect("should succeed");
    assert_eq!(users.len(), 1);
    assert_eq!(users[0].user_id, "@alice:example.com");
}

#[tokio::test]
async fn get_users_paginated_maps_storage_error_to_internal() {
    let store = MockUserStore::new();
    store.set_fail_all(true);
    let svc = build_service(store);
    let err = svc.get_users_paginated(10, None, None).await.expect_err("should error");
    assert!(err.is_internal());
}

#[tokio::test]
async fn get_user_count_returns_seeded_count() {
    let store = MockUserStore::new();
    store.users.lock().unwrap().insert("@alice:example.com".to_string(), make_user("@alice:example.com", None, None));
    store.users.lock().unwrap().insert("@bob:example.com".to_string(), make_user("@bob:example.com", None, None));
    let svc = build_service(store);

    let count = svc.get_user_count().await.expect("should succeed");
    assert_eq!(count, 2);
}

#[tokio::test]
async fn get_user_count_returns_zero_for_empty_store() {
    let svc = build_service(MockUserStore::new());
    let count = svc.get_user_count().await.expect("should succeed");
    assert_eq!(count, 0);
}

#[tokio::test]
async fn get_non_deactivated_user_count_excludes_deactivated() {
    let store = MockUserStore::new();
    let mut active = make_user("@alice:example.com", None, None);
    active.is_deactivated = false;
    let mut deactivated = make_user("@bob:example.com", None, None);
    deactivated.is_deactivated = true;
    store.users.lock().unwrap().insert(active.user_id.clone(), active);
    store.users.lock().unwrap().insert(deactivated.user_id.clone(), deactivated);
    let svc = build_service(store);

    let count = svc.get_non_deactivated_user_count().await.expect("should succeed");
    assert_eq!(count, 1, "only the non-deactivated user should be counted");
}

#[tokio::test]
async fn get_non_deactivated_user_count_maps_storage_error_to_internal() {
    let store = MockUserStore::new();
    store.set_fail_all(true);
    let svc = build_service(store);
    let err = svc.get_non_deactivated_user_count().await.expect_err("should error");
    assert!(err.is_internal());
}

#[tokio::test]
async fn get_non_deactivated_user_count_by_app_service_maps_storage_error_to_internal() {
    let store = MockUserStore::new();
    store.set_fail_all(true);
    let svc = build_service(store);
    let err = svc.get_non_deactivated_user_count_by_app_service().await.expect_err("should error");
    assert!(err.is_internal());
}

// ─────────────────────────────────────────────────────────────────────────────
// store() accessor
// ─────────────────────────────────────────────────────────────────────────────

#[tokio::test]
async fn store_accessor_returns_underlying_storage() {
    // The delegated accessor should return the same Arc<dyn UserStore>.
    let store = MockUserStore::new();
    let svc = build_service(store);
    let storage: &Arc<dyn UserStore> = svc.store();
    // Verify the returned store is functional by calling a method through it.
    let exists = storage.user_exists("@nobody:example.com").await.unwrap_or(false);
    assert!(!exists);
}

// ─────────────────────────────────────────────────────────────────────────────
// Edge cases — empty input
// ─────────────────────────────────────────────────────────────────────────────

#[tokio::test]
async fn get_profiles_batch_with_empty_input_returns_empty_vec() {
    let svc = build_service(MockUserStore::new());
    let profiles = svc.get_profiles_batch(&[]).await.expect("should succeed");
    assert!(profiles.is_empty());
}

#[tokio::test]
async fn get_users_paginated_with_zero_limit_returns_seeded_users() {
    // MockUserStore ignores the limit and returns all seeded users. The
    // service is a pure delegator, so we just verify it doesn't panic.
    let store = MockUserStore::new();
    store.users.lock().unwrap().insert("@alice:example.com".to_string(), make_user("@alice:example.com", None, None));
    let svc = build_service(store);
    let users = svc.get_users_paginated(0, None, None).await.expect("should succeed");
    assert!(!users.is_empty());
}
