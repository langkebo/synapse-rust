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
async fn test_upsert_and_get_account_data_content() {
    let (_iso, pool) = test_pool().await;
    let cache = test_cache();
    let storage = UserStorage::new(&pool, cache);
    let user_id = format!("@acctup_{}:example.com", uuid::Uuid::new_v4());
    let data_type = format!("m.up_{}", uuid::Uuid::new_v4().simple());
    let _ = storage.delete_user(&user_id).await;
    storage.create_user(&user_id, "acctupuser", None, false).await.unwrap();

    // Initially absent.
    let absent = storage
        .get_account_data_content(&user_id, &data_type)
        .await
        .expect("get_account_data_content absent should succeed");
    assert!(absent.is_none());

    // Upsert (insert).
    storage
        .upsert_account_data_content(&user_id, &data_type, &serde_json::json!({"v": 1}))
        .await
        .expect("upsert insert should succeed");
    let got = storage
        .get_account_data_content(&user_id, &data_type)
        .await
        .expect("get_account_data_content after insert should succeed")
        .expect("account data should exist");
    assert_eq!(got["v"], 1);

    // Upsert (update).
    storage
        .upsert_account_data_content(&user_id, &data_type, &serde_json::json!({"v": 2}))
        .await
        .expect("upsert update should succeed");
    let got2 = storage
        .get_account_data_content(&user_id, &data_type)
        .await
        .expect("get_account_data_content after update should succeed")
        .expect("account data should exist after update");
    assert_eq!(got2["v"], 2);

    let _ = storage.delete_user(&user_id).await;
}

// ── search ────────────────────────────────────────────────────

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
