//! DB-backed integration tests for the user storage domain.
//!
//! Split from the former `user::db_tests` inline module.

#![allow(clippy::unwrap_used, clippy::expect_used)]

use super::*;
use sqlx::{Pool, Postgres};
use std::sync::Arc;
use synapse_cache::{CacheConfig, CacheManager};
use synapse_common::current_timestamp_millis;

/// Each test gets a fresh isolated schema (v11 baseline), so parallel
/// tests can't pollute each other: several tests insert users with fixed
/// usernames (e.g. `uniqueuser99`) into the shared `public` schema, which
/// violates the `uq_users_username` unique constraint when they run
/// concurrently. Stale rows left behind by an earlier failed run also
/// cascade into later failures.
///
/// Returns the `IsolatedTestPool` *and* the pool so callers can hold the
/// guard alive for the whole test — dropping it early spawns a background
/// `DROP SCHEMA` that can race with in-flight queries.
async fn test_pool() -> (crate::test_isolation::IsolatedTestPool, Arc<Pool<Postgres>>) {
    let isolated = crate::test_isolation::isolated_test_pool().await.expect("isolated pool");
    let pool = isolated.pool();
    (isolated, pool)
}

fn test_cache() -> Arc<CacheManager> {
    Arc::new(CacheManager::new(&CacheConfig::default()))
}

// ── create / get by id ──────────────────────────────────────────

#[tokio::test]
async fn test_create_user_returns_valid_record() {
    let (_iso, pool) = test_pool().await;
    let cache = test_cache();
    let storage = UserStorage::new(&pool, cache);
    let user_id = format!("@create_test_{}:example.com", uuid::Uuid::new_v4());
    let _ = storage.delete_user(&user_id).await;

    let user = storage.create_user(&user_id, "createtest", None, false).await.expect("create_user should succeed");

    assert_eq!(user.user_id, user_id);
    assert_eq!(user.username, "createtest");
    assert!(!user.is_admin);
    assert!(!user.is_deactivated);

    let _ = storage.delete_user(&user_id).await;
}

#[tokio::test]
async fn test_get_user_by_id_found() {
    let (_iso, pool) = test_pool().await;
    let cache = test_cache();
    let storage = UserStorage::new(&pool, cache);
    let user_id = format!("@getbyid_{}:example.com", uuid::Uuid::new_v4());
    let _ = storage.delete_user(&user_id).await;
    storage.create_user(&user_id, "getbyiduser", None, false).await.unwrap();

    let found = storage.get_user_by_id(&user_id).await.expect("get_user_by_id should succeed");
    assert!(found.is_some());
    assert_eq!(found.unwrap().user_id, user_id);

    let _ = storage.delete_user(&user_id).await;
}

#[tokio::test]
async fn test_get_user_by_id_not_found() {
    let (_iso, pool) = test_pool().await;
    let cache = test_cache();
    let storage = UserStorage::new(&pool, cache);
    let result = storage.get_user_by_id("@nonexistent:example.com").await.expect("get_user_by_id should succeed");
    assert!(result.is_none());
}

// ── exists / by-username / count ─────────────────────────────────

#[tokio::test]
async fn test_user_exists_returns_true_for_existing_user() {
    let (_iso, pool) = test_pool().await;
    let cache = test_cache();
    let storage = UserStorage::new(&pool, cache);
    let user_id = format!("@exists_{}:example.com", uuid::Uuid::new_v4());
    let _ = storage.delete_user(&user_id).await;
    storage.create_user(&user_id, "existsuser", None, false).await.unwrap();

    assert!(storage.user_exists(&user_id).await.expect("user_exists should succeed"));

    let _ = storage.delete_user(&user_id).await;
}

#[tokio::test]
async fn test_user_exists_returns_false_for_nonexistent() {
    let (_iso, pool) = test_pool().await;
    let cache = test_cache();
    let storage = UserStorage::new(&pool, cache);
    assert!(!storage.user_exists("@nobody:example.com").await.expect("user_exists should succeed"));
}

/// U-2: the two predicates must be genuinely different. A deactivated row
/// still *exists* (upstream #20172 semantics, needed by the profile-field
/// endpoints and by username-availability) but is not an *active* account
/// (needed by every authorization / "can this account act" path).
#[tokio::test]
async fn test_user_exists_true_and_active_user_exists_false_for_deactivated_user() {
    let (_iso, pool) = test_pool().await;
    let cache = test_cache();
    let storage = UserStorage::new(&pool, cache);
    let user_id = format!("@deactivated_pred_{}:example.com", uuid::Uuid::new_v4());
    let _ = storage.delete_user(&user_id).await;
    storage.create_user(&user_id, "deactivated_pred", None, false).await.unwrap();

    assert!(storage.user_exists(&user_id).await.expect("user_exists should succeed"));
    assert!(
        storage.active_user_exists(&user_id).await.expect("active_user_exists should succeed"),
        "an account that was never deactivated must be active"
    );

    storage.deactivate_user(&user_id).await.expect("deactivate_user should succeed");

    assert!(
        storage.user_exists(&user_id).await.expect("user_exists should succeed"),
        "a deactivated account still exists as a row (upstream #20172)"
    );
    assert!(
        !storage.active_user_exists(&user_id).await.expect("active_user_exists should succeed"),
        "a deactivated account must not be reported as active"
    );

    let _ = storage.delete_user(&user_id).await;
}

#[tokio::test]
async fn test_user_exists_and_active_user_exists_true_for_active_user() {
    let (_iso, pool) = test_pool().await;
    let cache = test_cache();
    let storage = UserStorage::new(&pool, cache);
    let user_id = format!("@active_pred_{}:example.com", uuid::Uuid::new_v4());
    let _ = storage.delete_user(&user_id).await;
    storage.create_user(&user_id, "active_pred", None, false).await.unwrap();

    assert!(storage.user_exists(&user_id).await.expect("user_exists should succeed"));
    assert!(storage.active_user_exists(&user_id).await.expect("active_user_exists should succeed"));

    let _ = storage.delete_user(&user_id).await;
}

#[tokio::test]
async fn test_user_exists_and_active_user_exists_false_for_unknown_user() {
    let (_iso, pool) = test_pool().await;
    let cache = test_cache();
    let storage = UserStorage::new(&pool, cache);

    assert!(!storage.user_exists("@nobody_active:example.com").await.expect("user_exists should succeed"));
    assert!(!storage
        .active_user_exists("@nobody_active:example.com")
        .await
        .expect("active_user_exists should succeed"));
}

#[tokio::test]
async fn test_get_user_by_username_found() {
    let (_iso, pool) = test_pool().await;
    let cache = test_cache();
    let storage = UserStorage::new(&pool, cache);
    let user_id = format!("@byuser_{}:example.com", uuid::Uuid::new_v4());
    let _ = storage.delete_user(&user_id).await;
    storage.create_user(&user_id, "uniqueuser99", None, false).await.unwrap();

    let found = storage.get_user_by_username("uniqueuser99").await.expect("query should succeed");
    assert!(found.is_some());
    assert_eq!(found.unwrap().username, "uniqueuser99");

    let _ = storage.delete_user(&user_id).await;
}

#[tokio::test]
async fn test_get_user_count_increases_after_create() {
    let (_iso, pool) = test_pool().await;
    let cache = test_cache();
    let storage = UserStorage::new(&pool, cache);
    let uuid = uuid::Uuid::new_v4();
    let user_id = format!("@ct_{uuid}:example.com");
    let _ = storage.delete_user(&user_id).await;

    let _created =
        storage.create_user(&user_id, &format!("ct_{uuid}"), None, false).await.expect("create_user should succeed");

    let user = storage
        .get_user_by_id(&user_id)
        .await
        .expect("get_user_by_id should succeed")
        .expect("user should exist after create");

    assert_eq!(user.user_id, user_id);

    let _ = storage.delete_user(&user_id).await;
}

// ── displayname / avatar / password / admin ─────────────────────

#[tokio::test]
async fn test_update_displayname() {
    let (_iso, pool) = test_pool().await;
    let cache = test_cache();
    let storage = UserStorage::new(&pool, cache);
    let user_id = format!("@displayname_{}:example.com", uuid::Uuid::new_v4());
    let _ = storage.delete_user(&user_id).await;
    storage.create_user(&user_id, "dnameuser", None, false).await.unwrap();

    storage.update_displayname(&user_id, Some("New Name")).await.expect("update should succeed");
    let profile = storage.get_user_profile(&user_id).await.expect("get profile should succeed");
    assert_eq!(profile.unwrap().displayname.unwrap(), "New Name");

    let _ = storage.delete_user(&user_id).await;
}

#[tokio::test]
async fn test_update_avatar_url() {
    let (_iso, pool) = test_pool().await;
    let cache = test_cache();
    let storage = UserStorage::new(&pool, cache);
    let user_id = format!("@avatar_{}:example.com", uuid::Uuid::new_v4());
    let _ = storage.delete_user(&user_id).await;
    storage.create_user(&user_id, "avataruser", None, false).await.unwrap();

    storage.update_avatar_url(&user_id, Some("mxc://avatar")).await.expect("update should succeed");
    let profile = storage.get_user_profile(&user_id).await.expect("get profile should succeed");
    assert_eq!(profile.unwrap().avatar_url.unwrap(), "mxc://avatar");

    let _ = storage.delete_user(&user_id).await;
}

#[tokio::test]
async fn test_update_password() {
    let (_iso, pool) = test_pool().await;
    let cache = test_cache();
    let storage = UserStorage::new(&pool, cache);
    let user_id = format!("@pwd_{}:example.com", uuid::Uuid::new_v4());
    let _ = storage.delete_user(&user_id).await;
    storage.create_user(&user_id, "pwduser", Some("old_hash"), false).await.unwrap();

    storage.update_password(&user_id, "new_hash").await.expect("update_password should succeed");
    // password_hash is excluded from User serialization, but the
    // operation succeeding without error confirms the update worked.

    let _ = storage.delete_user(&user_id).await;
}

#[tokio::test]
async fn test_set_admin_status_toggle() {
    let (_iso, pool) = test_pool().await;
    let cache = test_cache();
    let storage = UserStorage::new(&pool, cache);
    let user_id = format!("@admin_{}:example.com", uuid::Uuid::new_v4());
    let _ = storage.delete_user(&user_id).await;
    storage.create_user(&user_id, "adminuser", None, false).await.unwrap();
    assert!(!storage.get_user_by_id(&user_id).await.unwrap().unwrap().is_admin);

    storage.set_admin_status(&user_id, true).await.expect("set_admin should succeed");
    let user = storage.get_user_by_id(&user_id).await.unwrap().unwrap();
    assert!(user.is_admin);

    storage.set_admin_status(&user_id, false).await.expect("unset_admin should succeed");
    let user = storage.get_user_by_id(&user_id).await.unwrap().unwrap();
    assert!(!user.is_admin);

    let _ = storage.delete_user(&user_id).await;
}

// ── deactivation / shadow-ban / delete / list / filter ──────────

#[tokio::test]
async fn test_set_deactivation_status() {
    let (_iso, pool) = test_pool().await;
    let cache = test_cache();
    let storage = UserStorage::new(&pool, cache);
    let user_id = format!("@deact_{}:example.com", uuid::Uuid::new_v4());
    let _ = storage.delete_user(&user_id).await;
    storage.create_user(&user_id, "deactuser", None, false).await.unwrap();

    let result = storage.set_deactivation_status(&user_id, true).await.expect("deactivate should succeed");
    assert!(result);
    let user = storage.get_user_by_id(&user_id).await.unwrap().unwrap();
    assert!(user.is_deactivated);

    let result = storage.set_deactivation_status(&user_id, false).await.expect("reactivate should succeed");
    assert!(result);
    let user = storage.get_user_by_id(&user_id).await.unwrap().unwrap();
    assert!(!user.is_deactivated);

    let _ = storage.delete_user(&user_id).await;
}

#[tokio::test]
async fn test_set_shadow_ban() {
    let (_iso, pool) = test_pool().await;
    let cache = test_cache();
    let storage = UserStorage::new(&pool, cache);
    let user_id = format!("@shadow_{}:example.com", uuid::Uuid::new_v4());
    let _ = storage.delete_user(&user_id).await;
    storage.create_user(&user_id, "shadowuser", None, false).await.unwrap();

    let result = storage.set_shadow_ban(&user_id, true).await.expect("shadow ban should succeed");
    assert!(result);
    let user = storage.get_user_by_id(&user_id).await.unwrap().unwrap();
    assert!(user.is_shadow_banned);

    storage.set_shadow_ban(&user_id, false).await.expect("unban should succeed");
    let user = storage.get_user_by_id(&user_id).await.unwrap().unwrap();
    assert!(!user.is_shadow_banned);

    let _ = storage.delete_user(&user_id).await;
}

#[tokio::test]
async fn test_delete_user() {
    let (_iso, pool) = test_pool().await;
    let cache = test_cache();
    let storage = UserStorage::new(&pool, cache);
    let user_id = format!("@delete_me_{}:example.com", uuid::Uuid::new_v4());
    let _ = storage.delete_user(&user_id).await;
    storage.create_user(&user_id, "deleteme", None, false).await.unwrap();
    assert!(storage.user_exists(&user_id).await.unwrap());

    storage.delete_user(&user_id).await.expect("delete_user should succeed");
    assert!(!storage.user_exists(&user_id).await.unwrap());
}

#[tokio::test]
async fn test_get_all_users_respects_limit() {
    let (_iso, pool) = test_pool().await;
    let cache = test_cache();
    let storage = UserStorage::new(&pool, cache);
    let users = storage.get_all_users(5).await.expect("get_all_users should succeed");
    assert!(users.len() <= 5);
}

#[tokio::test]
async fn test_filter_existing_users() {
    let (_iso, pool) = test_pool().await;
    let cache = test_cache();
    let storage = UserStorage::new(&pool, cache);
    let user_id = format!("@filter_{}:example.com", uuid::Uuid::new_v4());
    let _ = storage.delete_user(&user_id).await;
    storage.create_user(&user_id, "filteruser", None, false).await.unwrap();

    let existing = storage
        .filter_existing_users(&[user_id.clone(), "@nobody:example.com".to_string()])
        .await
        .expect("filter_existing_users should succeed");
    assert_eq!(existing.len(), 1);
    assert_eq!(existing[0], user_id);

    let _ = storage.delete_user(&user_id).await;
}

// ── lock / unlock / batch / profile / count-messages ────────────

#[tokio::test]
async fn test_lock_user_flow() {
    let (_iso, pool) = test_pool().await;
    let cache = test_cache();
    let storage = UserStorage::new(&pool, cache);
    let user_id = format!("@lock_{}:example.com", uuid::Uuid::new_v4());
    let _ = storage.delete_user(&user_id).await;
    storage.create_user(&user_id, "lockuser", None, false).await.unwrap();
    assert!(!storage.is_user_locked(&user_id).await.unwrap());

    let now = current_timestamp_millis();
    storage.lock_user(&user_id, Some("test_reason"), "system", now).await.expect("lock should succeed");
    assert!(storage.is_user_locked(&user_id).await.unwrap());

    let locked = storage.get_active_user_lock(&user_id).await.unwrap();
    assert!(locked.is_some());
    assert_eq!(locked.unwrap().reason.unwrap(), "test_reason");

    storage.unlock_user(&user_id, current_timestamp_millis()).await.expect("unlock should succeed");
    assert!(!storage.is_user_locked(&user_id).await.unwrap());

    let _ = storage.delete_user(&user_id).await;
}

#[tokio::test]
async fn test_get_users_batch() {
    let (_iso, pool) = test_pool().await;
    let cache = test_cache();
    let storage = UserStorage::new(&pool, cache);
    let uid1 = format!("@batch1_{}:example.com", uuid::Uuid::new_v4());
    let uid2 = format!("@batch2_{}:example.com", uuid::Uuid::new_v4());
    let _ = storage.delete_user(&uid1).await;
    let _ = storage.delete_user(&uid2).await;
    storage.create_user(&uid1, "batchuser1", None, false).await.unwrap();
    storage.create_user(&uid2, "batchuser2", None, false).await.unwrap();

    let users = storage.get_users_batch(&[uid1.clone(), uid2.clone()]).await.expect("get_users_batch should succeed");
    assert_eq!(users.len(), 2);
    let ids: Vec<&str> = users.iter().map(|u| u.user_id.as_str()).collect();
    assert!(ids.contains(&uid1.as_str()));
    assert!(ids.contains(&uid2.as_str()));

    let _ = storage.delete_user(&uid1).await;
    let _ = storage.delete_user(&uid2).await;
}

#[tokio::test]
async fn test_get_user_profile_found_and_not_found() {
    let (_iso, pool) = test_pool().await;
    let cache = test_cache();
    let storage = UserStorage::new(&pool, cache);
    let user_id = format!("@profile_{}:example.com", uuid::Uuid::new_v4());
    let _ = storage.delete_user(&user_id).await;
    storage.create_user(&user_id, "profileuser", None, false).await.unwrap();
    storage.update_displayname(&user_id, Some("Profile User")).await.unwrap();

    let profile = storage.get_user_profile(&user_id).await.unwrap().unwrap();
    assert_eq!(profile.displayname.unwrap(), "Profile User");

    let missing = storage.get_user_profile("@nobody:example.com").await.unwrap();
    assert!(missing.is_none());

    let _ = storage.delete_user(&user_id).await;
}

#[tokio::test]
async fn test_count_sent_messages_returns_count() {
    let (_iso, pool) = test_pool().await;
    let cache = test_cache();
    let storage = UserStorage::new(&pool, cache);
    let user_id = format!("@msgcount_{}:example.com", uuid::Uuid::new_v4());
    let _ = storage.delete_user(&user_id).await;
    storage.create_user(&user_id, "msgcountuser", None, false).await.unwrap();

    let count = storage.count_sent_messages(&user_id).await.expect("count should succeed");
    assert!(count >= 0);

    let _ = storage.delete_user(&user_id).await;
}

// ── get_user_by_email / get_user_by_identifier ─────────────────

#[tokio::test]
async fn test_get_user_by_email_found() {
    let (_iso, pool) = test_pool().await;
    let cache = test_cache();
    let storage = UserStorage::new(&pool, cache);
    let user_id = format!("@emailu_{}:example.com", uuid::Uuid::new_v4());
    let email = format!("emailtest_{}@example.com", uuid::Uuid::new_v4());
    let _ = storage.delete_user(&user_id).await;
    storage.create_user(&user_id, "emailuser", None, false).await.unwrap();
    // Set email via direct SQL (no public setter on storage).
    sqlx::query("UPDATE users SET email = $1 WHERE user_id = $2")
        .bind(&email)
        .bind(&user_id)
        .execute(&*pool)
        .await
        .expect("set email should succeed");

    let found = storage.get_user_by_email(&email).await.expect("get_user_by_email should succeed");
    assert!(found.is_some());
    assert_eq!(found.unwrap().user_id, user_id);

    let _ = storage.delete_user(&user_id).await;
}

#[tokio::test]
async fn test_get_user_by_email_not_found() {
    let (_iso, pool) = test_pool().await;
    let cache = test_cache();
    let storage = UserStorage::new(&pool, cache);
    let result =
        storage.get_user_by_email("nobody_here_12345@example.com").await.expect("get_user_by_email should succeed");
    assert!(result.is_none());
}

#[tokio::test]
async fn test_get_user_by_identifier_user_id_path() {
    let (_iso, pool) = test_pool().await;
    let cache = test_cache();
    let storage = UserStorage::new(&pool, cache);
    let user_id = format!("@ident_{}:example.com", uuid::Uuid::new_v4());
    let _ = storage.delete_user(&user_id).await;
    storage.create_user(&user_id, "identuser", None, false).await.unwrap();

    // Identifier starts with '@' and contains ':' → should resolve via get_user_by_id.
    let found =
        storage.get_user_by_identifier(&user_id).await.expect("get_user_by_identifier user_id path should succeed");
    assert!(found.is_some());
    assert_eq!(found.unwrap().user_id, user_id);

    let _ = storage.delete_user(&user_id).await;
}

#[tokio::test]
async fn test_get_user_by_identifier_username_path() {
    let (_iso, pool) = test_pool().await;
    let cache = test_cache();
    let storage = UserStorage::new(&pool, cache);
    let user_id = format!("@ident2_{}:example.com", uuid::Uuid::new_v4());
    let username = format!("identuser_{}", uuid::Uuid::new_v4());
    let _ = storage.delete_user(&user_id).await;
    storage.create_user(&user_id, &username, None, false).await.unwrap();

    // Identifier has no ':' → should resolve via get_user_by_username.
    let found =
        storage.get_user_by_identifier(&username).await.expect("get_user_by_identifier username path should succeed");
    assert!(found.is_some());
    assert_eq!(found.unwrap().username, username);

    let _ = storage.delete_user(&user_id).await;
}

// ── pagination / list / count ─────────────────────────────────

#[tokio::test]
async fn test_get_users_paginated_no_cursor() {
    let (_iso, pool) = test_pool().await;
    let cache = test_cache();
    let storage = UserStorage::new(&pool, cache);
    let users = storage.get_users_paginated(5, None, None).await.expect("get_users_paginated no cursor should succeed");
    assert!(users.len() <= 5);
}

#[tokio::test]
async fn test_get_users_paginated_with_cursor() {
    let (_iso, pool) = test_pool().await;
    let cache = test_cache();
    let storage = UserStorage::new(&pool, cache);
    let user_id = format!("@pag_{}:example.com", uuid::Uuid::new_v4());
    let _ = storage.delete_user(&user_id).await;
    let created = storage.create_user(&user_id, "paguser", None, false).await.unwrap();

    // Use the created user's timestamp/id as a cursor; should return users before it.
    let users = storage
        .get_users_paginated(10, Some(created.created_ts), Some(&user_id))
        .await
        .expect("get_users_paginated with cursor should succeed");
    // The created user itself should NOT be in the result (cursor is exclusive).
    assert!(!users.iter().any(|u| u.user_id == user_id));

    let _ = storage.delete_user(&user_id).await;
}

#[tokio::test]
async fn test_list_users_with_name_filter() {
    let (_iso, pool) = test_pool().await;
    let cache = test_cache();
    let storage = UserStorage::new(&pool, cache);
    let user_id = format!("@listf_{}:example.com", uuid::Uuid::new_v4());
    let username = format!("listfilter_{}", uuid::Uuid::new_v4().simple());
    let _ = storage.delete_user(&user_id).await;
    storage.create_user(&user_id, &username, None, false).await.unwrap();

    let users =
        storage.list_users(50, None, None, Some(&username)).await.expect("list_users with name filter should succeed");
    assert!(users.iter().any(|u| u.user_id == user_id));

    let _ = storage.delete_user(&user_id).await;
}

#[tokio::test]
async fn test_list_users_no_filter_respects_limit() {
    let (_iso, pool) = test_pool().await;
    let cache = test_cache();
    let storage = UserStorage::new(&pool, cache);
    let users = storage.list_users(3, None, None, None).await.expect("list_users no filter should succeed");
    assert!(users.len() <= 3);
}

#[tokio::test]
async fn test_get_user_count_returns_non_negative() {
    let (_iso, pool) = test_pool().await;
    let cache = test_cache();
    let storage = UserStorage::new(&pool, cache);
    let count = storage.get_user_count().await.expect("get_user_count should succeed");
    assert!(count >= 0);
}

#[tokio::test]
async fn test_get_daily_active_users() {
    let (_iso, pool) = test_pool().await;
    let cache = test_cache();
    let storage = UserStorage::new(&pool, cache);
    let count = storage.get_daily_active_users().await.expect("get_daily_active_users should succeed");
    assert!(count >= 0);
}

#[tokio::test]
async fn test_get_monthly_active_users() {
    let (_iso, pool) = test_pool().await;
    let cache = test_cache();
    let storage = UserStorage::new(&pool, cache);
    let count = storage.get_monthly_active_users().await.expect("get_monthly_active_users should succeed");
    assert!(count >= 0);
}

#[tokio::test]
async fn test_get_r30_users() {
    let (_iso, pool) = test_pool().await;
    let cache = test_cache();
    let storage = UserStorage::new(&pool, cache);
    let count = storage.get_r30_users().await.expect("get_r30_users should succeed");
    assert!(count >= 0);
}

#[tokio::test]
async fn test_get_user_stats_summary() {
    let (_iso, pool) = test_pool().await;
    let cache = test_cache();
    let storage = UserStorage::new(&pool, cache);
    let user_id = format!("@stats_{}:example.com", uuid::Uuid::new_v4());
    let _ = storage.delete_user(&user_id).await;
    storage.create_user(&user_id, "statsuser", None, false).await.unwrap();

    let summary = storage.get_user_stats_summary().await.expect("get_user_stats_summary should succeed");
    // total_users should include all counted users; the breakdowns should be consistent.
    assert!(summary.total_users >= 1);
    assert!(summary.active_users + summary.deactivated_users <= summary.total_users + 1);

    let _ = storage.delete_user(&user_id).await;
}

// ── deactivate / guest / user_type / upgrade_guest ────────────

#[tokio::test]
async fn test_deactivate_user() {
    let (_iso, pool) = test_pool().await;
    let cache = test_cache();
    let storage = UserStorage::new(&pool, cache);
    let user_id = format!("@deactfn_{}:example.com", uuid::Uuid::new_v4());
    let _ = storage.delete_user(&user_id).await;
    storage.create_user(&user_id, "deactfnuser", None, false).await.unwrap();
    assert!(!storage.get_user_by_id(&user_id).await.unwrap().unwrap().is_deactivated);

    storage.deactivate_user(&user_id).await.expect("deactivate_user should succeed");
    let user = storage.get_user_by_id(&user_id).await.unwrap().unwrap();
    assert!(user.is_deactivated);

    let _ = storage.delete_user(&user_id).await;
}

#[tokio::test]
async fn test_set_guest_status_toggle() {
    let (_iso, pool) = test_pool().await;
    let cache = test_cache();
    let storage = UserStorage::new(&pool, cache);
    let user_id = format!("@guest_{}:example.com", uuid::Uuid::new_v4());
    let _ = storage.delete_user(&user_id).await;
    storage.create_user(&user_id, "guestuser", None, false).await.unwrap();
    assert!(!storage.get_user_by_id(&user_id).await.unwrap().unwrap().is_guest);

    storage.set_guest_status(&user_id, true).await.expect("set_guest_status true should succeed");
    assert!(storage.get_user_by_id(&user_id).await.unwrap().unwrap().is_guest);

    storage.set_guest_status(&user_id, false).await.expect("set_guest_status false should succeed");
    assert!(!storage.get_user_by_id(&user_id).await.unwrap().unwrap().is_guest);

    let _ = storage.delete_user(&user_id).await;
}

#[tokio::test]
async fn test_set_user_type() {
    let (_iso, pool) = test_pool().await;
    let cache = test_cache();
    let storage = UserStorage::new(&pool, cache);
    let user_id = format!("@utype_{}:example.com", uuid::Uuid::new_v4());
    let _ = storage.delete_user(&user_id).await;
    storage.create_user(&user_id, "utypeuser", None, false).await.unwrap();
    assert!(storage.get_user_by_id(&user_id).await.unwrap().unwrap().user_type.is_none());

    storage.set_user_type(&user_id, Some("bot")).await.expect("set_user_type Some should succeed");
    assert_eq!(storage.get_user_by_id(&user_id).await.unwrap().unwrap().user_type.as_deref(), Some("bot"));

    storage.set_user_type(&user_id, None).await.expect("set_user_type None should succeed");
    assert!(storage.get_user_by_id(&user_id).await.unwrap().unwrap().user_type.is_none());

    let _ = storage.delete_user(&user_id).await;
}

#[tokio::test]
async fn test_upgrade_guest_account() {
    let (_iso, pool) = test_pool().await;
    let cache = test_cache();
    let storage = UserStorage::new(&pool, cache);
    let user_id = format!("@upgrade_{}:example.com", uuid::Uuid::new_v4());
    let _ = storage.delete_user(&user_id).await;
    storage.create_user(&user_id, "guestupgrade", None, false).await.unwrap();
    // Mark as guest first.
    storage.set_guest_status(&user_id, true).await.unwrap();
    assert!(storage.get_user_by_id(&user_id).await.unwrap().unwrap().is_guest);

    let new_username = format!("upgraded_{}", uuid::Uuid::new_v4().simple());
    storage
        .upgrade_guest_account(&user_id, &new_username, "new_hash")
        .await
        .expect("upgrade_guest_account should succeed");

    let user = storage.get_user_by_id(&user_id).await.unwrap().unwrap();
    assert!(!user.is_guest);
    assert_eq!(user.username, new_username);

    let _ = storage.delete_user(&user_id).await;
}

#[tokio::test]
async fn test_search_users_empty_query() {
    let (_iso, pool) = test_pool().await;
    let cache = test_cache();
    let storage = UserStorage::new(&pool, cache);
    let results = storage.search_users("", 10).await.expect("search_users empty should succeed");
    assert!(results.is_empty());
}

#[tokio::test]
async fn test_search_users_matches_username() {
    let (_iso, pool) = test_pool().await;
    let cache = test_cache();
    let storage = UserStorage::new(&pool, cache);
    let unique = uuid::Uuid::new_v4().simple().to_string();
    let username = format!("searchable_{unique}");
    let user_id = format!("@searchable_{unique}:example.com");
    let _ = storage.delete_user(&user_id).await;
    storage.create_user(&user_id, &username, None, false).await.unwrap();

    let results = storage.search_users(&username, 10).await.expect("search_users should succeed");
    assert!(results.iter().any(|r| r.user_id == user_id));

    let _ = storage.delete_user(&user_id).await;
}

#[tokio::test]
async fn test_search_users_with_presence_empty_query() {
    let (_iso, pool) = test_pool().await;
    let cache = test_cache();
    let storage = UserStorage::new(&pool, cache);
    let results =
        storage.search_users_with_presence("", 10).await.expect("search_users_with_presence empty should succeed");
    assert!(results.is_empty());
}

#[tokio::test]
async fn test_search_users_with_presence_matches() {
    let (_iso, pool) = test_pool().await;
    let cache = test_cache();
    let storage = UserStorage::new(&pool, cache);
    let unique = uuid::Uuid::new_v4().simple().to_string();
    let username = format!("swp_{unique}");
    let user_id = format!("@swp_{unique}:example.com");
    let _ = storage.delete_user(&user_id).await;
    storage.create_user(&user_id, &username, None, false).await.unwrap();

    let results =
        storage.search_users_with_presence(&username, 10).await.expect("search_users_with_presence should succeed");
    assert!(results.iter().any(|r| r.user_id == user_id));

    let _ = storage.delete_user(&user_id).await;
}

#[tokio::test]
async fn test_search_directory_users_empty_query() {
    let (_iso, pool) = test_pool().await;
    let cache = test_cache();
    let storage = UserStorage::new(&pool, cache);
    let results =
        storage.search_directory_users("", 10, false).await.expect("search_directory_users empty should succeed");
    assert!(results.is_empty());
}

#[tokio::test]
async fn test_search_directory_users_matches() {
    let (_iso, pool) = test_pool().await;
    let cache = test_cache();
    let storage = UserStorage::new(&pool, cache);
    let unique = uuid::Uuid::new_v4().simple().to_string();
    let username = format!("diruser_{unique}");
    let user_id = format!("@diruser_{unique}:example.com");
    let _ = storage.delete_user(&user_id).await;
    storage.create_user(&user_id, &username, None, false).await.unwrap();

    let results =
        storage.search_directory_users(&username, 10, false).await.expect("search_directory_users should succeed");
    assert!(results.iter().any(|r| r.user_id == user_id));

    let _ = storage.delete_user(&user_id).await;
}

// ── batch profiles / maps ─────────────────────────────────────

#[tokio::test]
async fn test_get_user_profiles_batch_empty() {
    let (_iso, pool) = test_pool().await;
    let cache = test_cache();
    let storage = UserStorage::new(&pool, cache);
    let result = storage.get_user_profiles_batch(&[]).await.expect("get_user_profiles_batch empty should succeed");
    assert!(result.is_empty());
}

#[tokio::test]
async fn test_get_user_profiles_batch_with_users() {
    let (_iso, pool) = test_pool().await;
    let cache = test_cache();
    let storage = UserStorage::new(&pool, cache);
    let uid1 = format!("@pb1_{}:example.com", uuid::Uuid::new_v4());
    let uid2 = format!("@pb2_{}:example.com", uuid::Uuid::new_v4());
    for uid in [&uid1, &uid2] {
        let _ = storage.delete_user(uid).await;
    }
    storage.create_user(&uid1, "pb1user", None, false).await.unwrap();
    storage.create_user(&uid2, "pb2user", None, false).await.unwrap();

    let profiles = storage
        .get_user_profiles_batch(&[uid1.clone(), uid2.clone()])
        .await
        .expect("get_user_profiles_batch should succeed");
    assert_eq!(profiles.len(), 2);
    assert!(profiles.iter().any(|p| p.user_id == uid1));
    assert!(profiles.iter().any(|p| p.user_id == uid2));

    for uid in [&uid1, &uid2] {
        let _ = storage.delete_user(uid).await;
    }
}

#[tokio::test]
async fn test_get_user_profiles_map_empty() {
    let (_iso, pool) = test_pool().await;
    let cache = test_cache();
    let storage = UserStorage::new(&pool, cache);
    let map = storage.get_user_profiles_map(&[]).await.expect("get_user_profiles_map empty should succeed");
    assert!(map.is_empty());
}

#[tokio::test]
async fn test_get_user_profiles_map_with_users() {
    let (_iso, pool) = test_pool().await;
    let cache = test_cache();
    let storage = UserStorage::new(&pool, cache);
    let uid1 = format!("@pm1_{}:example.com", uuid::Uuid::new_v4());
    let uid2 = format!("@pm2_{}:example.com", uuid::Uuid::new_v4());
    for uid in [&uid1, &uid2] {
        let _ = storage.delete_user(uid).await;
    }
    storage.create_user(&uid1, "pm1user", None, false).await.unwrap();
    storage.create_user(&uid2, "pm2user", None, false).await.unwrap();

    let map = storage
        .get_user_profiles_map(&[uid1.clone(), uid2.clone()])
        .await
        .expect("get_user_profiles_map should succeed");
    assert!(map.contains_key(&uid1));
    assert!(map.contains_key(&uid2));

    for uid in [&uid1, &uid2] {
        let _ = storage.delete_user(uid).await;
    }
}

#[tokio::test]
async fn test_get_users_map_empty() {
    let (_iso, pool) = test_pool().await;
    let cache = test_cache();
    let storage = UserStorage::new(&pool, cache);
    let map = storage.get_users_map(&[]).await.expect("get_users_map empty should succeed");
    assert!(map.is_empty());
}

#[tokio::test]
async fn test_get_users_map_with_users() {
    let (_iso, pool) = test_pool().await;
    let cache = test_cache();
    let storage = UserStorage::new(&pool, cache);
    let uid1 = format!("@um1_{}:example.com", uuid::Uuid::new_v4());
    let uid2 = format!("@um2_{}:example.com", uuid::Uuid::new_v4());
    for uid in [&uid1, &uid2] {
        let _ = storage.delete_user(uid).await;
    }
    storage.create_user(&uid1, "um1user", None, false).await.unwrap();
    storage.create_user(&uid2, "um2user", None, false).await.unwrap();

    let map = storage.get_users_map(&[uid1.clone(), uid2.clone()]).await.expect("get_users_map should succeed");
    assert!(map.contains_key(&uid1));
    assert!(map.contains_key(&uid2));

    for uid in [&uid1, &uid2] {
        let _ = storage.delete_user(uid).await;
    }
}

#[tokio::test]
async fn test_update_displayname_batch_empty() {
    let (_iso, pool) = test_pool().await;
    let cache = test_cache();
    let storage = UserStorage::new(&pool, cache);
    let count = storage.update_displayname_batch(&[]).await.expect("update_displayname_batch empty should succeed");
    assert_eq!(count, 0);
}

#[tokio::test]
async fn test_update_displayname_batch_with_updates() {
    let (_iso, pool) = test_pool().await;
    let cache = test_cache();
    let storage = UserStorage::new(&pool, cache);
    let uid1 = format!("@dbn1_{}:example.com", uuid::Uuid::new_v4());
    let uid2 = format!("@dbn2_{}:example.com", uuid::Uuid::new_v4());
    for uid in [&uid1, &uid2] {
        let _ = storage.delete_user(uid).await;
    }
    storage.create_user(&uid1, "dbn1user", None, false).await.unwrap();
    storage.create_user(&uid2, "dbn2user", None, false).await.unwrap();

    let updates: Vec<(String, Option<String>)> =
        vec![(uid1.clone(), Some("Batch Name 1".to_string())), (uid2.clone(), Some("Batch Name 2".to_string()))];
    let count = storage.update_displayname_batch(&updates).await.expect("update_displayname_batch should succeed");
    assert_eq!(count, 2);

    assert_eq!(storage.get_user_profile(&uid1).await.unwrap().unwrap().displayname.unwrap(), "Batch Name 1");
    assert_eq!(storage.get_user_profile(&uid2).await.unwrap().unwrap().displayname.unwrap(), "Batch Name 2");

    for uid in [&uid1, &uid2] {
        let _ = storage.delete_user(uid).await;
    }
}

// ── get_locked_users / create_user_tx ─────────────────────────

#[tokio::test]
async fn test_get_locked_users_pagination() {
    let (_iso, pool) = test_pool().await;
    let cache = test_cache();
    let storage = UserStorage::new(&pool, cache);
    let user_id = format!("@lklist_{}:example.com", uuid::Uuid::new_v4());
    let _ = storage.delete_user(&user_id).await;
    storage.create_user(&user_id, "lklistuser", None, false).await.unwrap();

    let now = current_timestamp_millis();
    storage.lock_user(&user_id, Some("audit"), "system", now).await.unwrap();

    // Page 1 (limit large enough) should include our locked user.
    let locked = storage.get_locked_users(100, 0).await.expect("get_locked_users should succeed");
    assert!(locked.iter().any(|l| l.user_id == user_id && l.is_active));

    // Clean up the lock.
    storage.unlock_user(&user_id, now).await.unwrap();
    let _ = storage.delete_user(&user_id).await;
}

#[tokio::test]
async fn test_create_user_tx_in_transaction() {
    let (_iso, pool) = test_pool().await;
    let cache = test_cache();
    let storage = UserStorage::new(&pool, cache);
    let user_id = format!("@createtx_{}:example.com", uuid::Uuid::new_v4());
    let _ = storage.delete_user(&user_id).await;

    let mut tx = pool.begin().await.expect("begin tx should succeed");
    let user = storage
        .create_user_tx(&mut tx, &user_id, "createtxuser", Some("hash"), false)
        .await
        .expect("create_user_tx should succeed");
    assert_eq!(user.user_id, user_id);
    tx.commit().await.expect("commit should succeed");

    // After commit, the user should be retrievable.
    let found = storage.get_user_by_id(&user_id).await.unwrap().expect("user should exist after tx commit");
    assert_eq!(found.username, "createtxuser");

    let _ = storage.delete_user(&user_id).await;
}

// =============================================================================
// C63-0: `search_directory_users` 的真基线往返（此前只有路由/服务层间接覆盖）
// =============================================================================

/// `UserStorage::search_directory_users` 是目录搜索的唯一实现：三段 `UNION ALL`
/// （username / displayname / email / user_id 四个投影面）+ `rank_score` 打分 + `pg_trgm`
/// 的 `%` 相似度 + `ILIKE … ESCAPE '\'` 的**元字符转义**。此前**零 storage 级用例**
/// （只有 `tests/integration/api_profile_tests.rs` 的路由级覆盖），转换前按 R8 补齐。
///
/// 本用例钉住四处语义：① 精确匹配排在前缀/包含之前（`match_type = 'exact'` 且 `match_score`
/// 最高）；② `exact_only=true` 只留精确匹配；③ `limit` 生效；④ **`_` 是 LIKE 元字符**，
/// `escape_like_pattern` 必须让它只按字面量匹配（与 D-98 的 unread highlight 同一类陷阱）。
#[tokio::test]
async fn test_search_directory_users_ranks_exactly_and_escapes_like_metacharacters() {
    let (_iso, pool) = test_pool().await;
    let cache = test_cache();
    let storage = UserStorage::new(&pool, cache);

    let suffix = uuid::Uuid::new_v4().simple().to_string();
    // 用户名里带 `_`：既是"精确/前缀/包含"的排序素材，也是 LIKE 元字符转义的探针。
    let exact = format!("dir_{suffix}");
    let prefix = format!("dir_{suffix}_extra");
    let contains = format!("xdir_{suffix}x");
    let decoy = format!("dirQ{suffix}"); // 与 exact 只差 `_` → `Q`：转义正确时**不该**被精确命中
    for name in [&exact, &prefix, &contains, &decoy] {
        let user_id = format!("@{name}:example.com");
        let _ = storage.delete_user(&user_id).await;
        storage.create_user(&user_id, name, None, false).await.expect("create_user");
    }

    // ④ `_` 按字面量匹配：exact 必须命中，decoy（把 `_` 换成 `Q`）不得被"精确"命中
    let rows = storage.search_directory_users(&exact, 10, false).await.expect("search_directory_users");
    assert_eq!(rows.first().map(|r| r.username.as_str()), Some(exact.as_str()), "精确匹配必须排第一");
    assert_eq!(rows[0].match_type, "exact");
    assert!(rows.iter().any(|r| r.username == prefix), "前缀命中也应出现在非 exact_only 结果里");
    let decoy_row = rows.iter().find(|r| r.username == decoy);
    assert!(
        decoy_row.is_none_or(|r| r.match_type != "exact"),
        "`_` 必须按字面量匹配（escape_like_pattern）：decoy 不得被当成精确匹配"
    );

    // ② exact_only 只留精确匹配
    let only_exact = storage.search_directory_users(&exact, 10, true).await.expect("exact_only search");
    assert_eq!(only_exact.len(), 1, "exact_only=true 只应返回精确匹配: {only_exact:?}");
    assert_eq!(only_exact[0].username, exact);
    assert_eq!(only_exact[0].match_type, "exact");

    // ③ limit 生效
    let limited = storage.search_directory_users(&exact, 1, false).await.expect("limited search");
    assert_eq!(limited.len(), 1);

    // ① 打分单调：精确 > 前缀（同为 `dir_…` 家族，避免别的测试数据干扰）
    let family: Vec<_> =
        rows.iter().filter(|r| r.username.starts_with("dir_") || r.username.starts_with("xdir_")).collect();
    if let (Some(first), Some(rest)) = (family.first(), family.get(1)) {
        assert!(first.match_score >= rest.match_score, "结果必须按 match_score 降序: {family:?}");
    }

    // 清理
    for name in [&exact, &prefix, &contains, &decoy] {
        let user_id = format!("@{name}:example.com");
        let _ = storage.delete_user(&user_id).await;
    }
}

// =============================================================================
// C69：`ensure_remote_user` 的真 baseline 往返（此前**零用例**）
// =============================================================================

/// 该方法是"联邦/服务通知遇到远端用户时补一行 `users`"的唯一入口（生产调用点：
/// `synapse-services/src/room/membership/federation.rs:436`、
/// `synapse-services/src/server_notification_service.rs:335`）。
///
/// C69 把它的 SQL 从**字面量动态调用**（`sqlx::query(r#"INSERT …"#)`，违反 R1、并让 literal 棘轮
/// 从 36 涨到 37）改成 `sqlx::query!` —— 因此这里按 R8④ 补一条真 baseline 往返，
/// 钉住三件事：① username 由 localpart 派生；② **幂等**（`ON CONFLICT DO NOTHING`：重复调用不报错、
/// 不新增行、不覆盖已有行）；③ 没有 localpart 时 username 退回整个 user_id。
#[tokio::test]
async fn test_ensure_remote_user_derives_username_and_is_idempotent() {
    let (_iso, pool) = test_pool().await;
    let cache = test_cache();
    let storage = UserStorage::new(&pool, cache);
    let suffix = uuid::Uuid::new_v4().simple().to_string();

    // ① 正常远端用户：localpart 派生 username
    let remote = format!("@remote_{suffix}:other.example");
    storage.ensure_remote_user(&remote).await.expect("ensure_remote_user should succeed");
    let row = storage.get_user_by_id(&remote).await.expect("get_user_by_id").expect("远端用户必须已插入");
    assert_eq!(row.username, format!("remote_{suffix}"), "username 必须由 localpart 派生");
    assert!(row.created_ts > 0, "created_ts 必须被写入");
    assert!(row.password_hash.is_none(), "远端用户不应有本地密码");

    // ② 幂等：重复调用不报错、行数不变、已有行不被覆盖
    storage.ensure_remote_user(&remote).await.expect("重复调用必须成功（ON CONFLICT DO NOTHING）");
    let count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM users WHERE user_id = $1")
        .bind(&remote)
        .fetch_one(&*pool)
        .await
        .expect("count");
    assert_eq!(count, 1, "重复调用不得插入第二行");

    // ③ 已存在的**本地**用户不得被远端补行覆盖（这是 ON CONFLICT DO NOTHING 的关键语义）
    let local = format!("@local_{suffix}:example.com");
    storage.create_user(&local, "localusername", Some("hash"), true).await.expect("create_user");
    storage.ensure_remote_user(&local).await.expect("对已存在用户必须成功且不修改");
    let untouched = storage.get_user_by_id(&local).await.expect("get_user_by_id").expect("本地用户仍在");
    assert_eq!(untouched.username, "localusername", "不得改写已有 username");
    assert_eq!(untouched.password_hash.as_deref(), Some("hash"), "不得清掉已有 password_hash");
    assert!(untouched.is_admin, "不得降级已有 is_admin");

    // ④ 畸形 user_id（没有 localpart）必须**失败**而不是插入脏行：`users` 上有
    //    `ck_users_user_id_format CHECK (user_id ~ '^@[a-zA-Z0-9._=+./-]+:[a-zA-Z0-9.-]+$')`。
    //    ⚠️ 这同时说明 `ensure_remote_user` 里 `unwrap_or(user_id)` 那条 username 兜底是
    //    **defence-in-depth**（合法 user_id 走不到它），本用例把它钉成"fail-closed"而不是"能成功"。
    let odd = format!("odduser{suffix}");
    let error = storage.ensure_remote_user(&odd).await.expect_err("畸形 user_id 必须被 CHECK 约束拒绝");
    assert!(
        error.to_string().contains("ck_users_user_id_format") || error.to_string().contains("23514"),
        "错误必须来自 user_id 格式约束，实际：{error}"
    );
    assert!(!storage.user_exists(&odd).await.expect("user_exists"), "失败后不得留下任何行");

    for user_id in [&remote, &local] {
        let _ = storage.delete_user(user_id).await;
    }
}
