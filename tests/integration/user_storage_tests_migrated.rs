#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
use std::collections::HashSet;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use synapse_common::current_timestamp_millis;
use synapse_rust::cache::{CacheConfig, CacheManager};
use synapse_storage::user::UserStorage;

static TEST_COUNTER: AtomicU64 = AtomicU64::new(1);

fn unique_id() -> u64 {
    TEST_COUNTER.fetch_add(1, Ordering::SeqCst)
}

fn create_user_storage(pool: &Arc<sqlx::PgPool>) -> UserStorage {
    let cache = Arc::new(CacheManager::new(&CacheConfig::default()));
    UserStorage::new(pool, cache)
}

#[tokio::test]
async fn test_create_user_and_get_by_id() {
    let pool = crate::require_test_pool().await;
    let storage = create_user_storage(&pool);
    let id = unique_id();
    let user_id = format!("@testuser_{id}:localhost");
    let username = format!("testuser_{id}");

    let user = storage.create_user(&user_id, &username, Some("hash123"), false).await.unwrap();

    assert_eq!(user.user_id, user_id);
    assert_eq!(user.username, username);
    assert_eq!(user.password_hash, Some("hash123".to_string()));
    assert!(!user.is_admin);
    assert!(!user.is_guest);
    assert!(!user.is_deactivated);
    assert!(user.created_ts > 0);

    let fetched = storage.get_user_by_id(&user_id).await.unwrap().unwrap();
    assert_eq!(fetched.user_id, user_id);
    assert_eq!(fetched.username, username);
}

#[tokio::test]
async fn test_get_user_by_id_not_found() {
    let pool = crate::require_test_pool().await;
    let storage = create_user_storage(&pool);

    let result = storage.get_user_by_id("@nonexistent:localhost").await.unwrap();
    assert!(result.is_none());
}

#[tokio::test]
async fn test_get_user_by_username() {
    let pool = crate::require_test_pool().await;
    let storage = create_user_storage(&pool);
    let id = unique_id();
    let user_id = format!("@username_user_{id}:localhost");
    let username = format!("username_user_{id}");

    storage.create_user(&user_id, &username, None, false).await.unwrap();

    let fetched = storage.get_user_by_username(&username).await.unwrap().unwrap();
    assert_eq!(fetched.user_id, user_id);
    assert_eq!(fetched.username, username);
}

#[tokio::test]
async fn test_get_user_by_username_not_found() {
    let pool = crate::require_test_pool().await;
    let storage = create_user_storage(&pool);

    let result = storage.get_user_by_username("nonexistent_user").await.unwrap();
    assert!(result.is_none());
}

#[tokio::test]
async fn test_get_user_by_email() {
    let pool = crate::require_test_pool().await;
    let id = unique_id();
    let user_id = format!("@email_user_{id}:localhost");
    let username = format!("email_user_{id}");
    let email = format!("email_{id}@example.com");

    sqlx::query(
        r#"
            INSERT INTO users (user_id, username, email, created_ts, generation)
            VALUES ($1, $2, $3, $4, $5)
            "#,
    )
    .bind(&user_id)
    .bind(&username)
    .bind(&email)
    .bind(current_timestamp_millis())
    .bind(current_timestamp_millis())
    .execute(pool.as_ref())
    .await
    .unwrap();

    let storage = create_user_storage(&pool);
    let fetched = storage.get_user_by_email(&email).await.unwrap().unwrap();
    assert_eq!(fetched.user_id, user_id);
    assert_eq!(fetched.email, Some(email));
}

#[tokio::test]
async fn test_get_user_by_email_deactivated_excluded() {
    let pool = crate::require_test_pool().await;
    let id = unique_id();
    let user_id = format!("@deactivated_email_{id}:localhost");
    let username = format!("deactivated_email_{id}");
    let email = format!("deactivated_{id}@example.com");

    sqlx::query(
        r#"
            INSERT INTO users (user_id, username, email, is_deactivated, created_ts, generation)
            VALUES ($1, $2, $3, TRUE, $4, $5)
            "#,
    )
    .bind(&user_id)
    .bind(&username)
    .bind(&email)
    .bind(current_timestamp_millis())
    .bind(current_timestamp_millis())
    .execute(pool.as_ref())
    .await
    .unwrap();

    let storage = create_user_storage(&pool);
    let result = storage.get_user_by_email(&email).await.unwrap();
    assert!(result.is_none());
}

#[tokio::test]
async fn test_get_user_by_identifier_user_id() {
    let pool = crate::require_test_pool().await;
    let storage = create_user_storage(&pool);
    let id = unique_id();
    let user_id = format!("@ident_user_{id}:localhost");
    let username = format!("ident_user_{id}");

    storage.create_user(&user_id, &username, None, false).await.unwrap();

    let fetched = storage.get_user_by_identifier(&user_id).await.unwrap().unwrap();
    assert_eq!(fetched.user_id, user_id);
}

#[tokio::test]
async fn test_get_user_by_identifier_username() {
    let pool = crate::require_test_pool().await;
    let storage = create_user_storage(&pool);
    let id = unique_id();
    let user_id = format!("@ident_name_{id}:localhost");
    let username = format!("ident_name_{id}");

    storage.create_user(&user_id, &username, None, false).await.unwrap();

    let fetched = storage.get_user_by_identifier(&username).await.unwrap().unwrap();
    assert_eq!(fetched.username, username);
}

#[tokio::test]
async fn test_get_all_users() {
    let pool = crate::require_test_pool().await;
    let storage = create_user_storage(&pool);

    for i in 0..3 {
        let id = unique_id();
        let user_id = format!("@all_user_{id}:localhost");
        let username = format!("all_user_{id}");
        storage.create_user(&user_id, &username, None, false).await.unwrap();
        let _ = i;
    }

    let users = storage.get_all_users(10).await.unwrap();
    assert!(users.len() >= 3);
}

#[tokio::test]
async fn test_get_all_users_limit() {
    let pool = crate::require_test_pool().await;
    let storage = create_user_storage(&pool);

    for _ in 0..5 {
        let id = unique_id();
        let user_id = format!("@limit_user_{id}:localhost");
        let username = format!("limit_user_{id}");
        storage.create_user(&user_id, &username, None, false).await.unwrap();
    }

    let users = storage.get_all_users(2).await.unwrap();
    assert_eq!(users.len(), 2);
}

#[tokio::test]
async fn test_get_user_count() {
    let pool = crate::require_test_pool().await;
    let storage = create_user_storage(&pool);

    let count_before = storage.get_user_count().await.unwrap();

    let id = unique_id();
    let user_id = format!("@count_user_{id}:localhost");
    let username = format!("count_user_{id}");
    storage.create_user(&user_id, &username, None, false).await.unwrap();

    let count_after = storage.get_user_count().await.unwrap();
    assert_eq!(count_after, count_before + 1);
}

#[tokio::test]
async fn test_user_exists() {
    let pool = crate::require_test_pool().await;
    let storage = create_user_storage(&pool);
    let id = unique_id();
    let user_id = format!("@exists_user_{id}:localhost");
    let username = format!("exists_user_{id}");

    storage.create_user(&user_id, &username, None, false).await.unwrap();

    assert!(storage.user_exists(&user_id).await.unwrap());
    assert!(!storage.user_exists("@no_such_user:localhost").await.unwrap());
}

#[tokio::test]
async fn test_user_exists_true_and_active_user_exists_false_for_deactivated() {
    let pool = crate::require_test_pool().await;
    let storage = create_user_storage(&pool);
    let id = unique_id();
    let user_id = format!("@deactivated_exists_{id}:localhost");
    let username = format!("deactivated_exists_{id}");

    storage.create_user(&user_id, &username, None, false).await.unwrap();
    assert!(storage.active_user_exists(&user_id).await.unwrap(), "a fresh account is active");

    storage.deactivate_user(&user_id).await.unwrap();

    // U-2 / upstream #20172: the row still *exists* (profile-field endpoints and
    // username availability must keep seeing it) …
    assert!(storage.user_exists(&user_id).await.unwrap());
    // … but the account may no longer *act*, which is what `active_user_exists`
    // is for. This used to assert `!user_exists(..)`, the conflated behaviour.
    assert!(!storage.active_user_exists(&user_id).await.unwrap());
}

#[tokio::test]
async fn test_update_password() {
    let pool = crate::require_test_pool().await;
    let storage = create_user_storage(&pool);
    let id = unique_id();
    let user_id = format!("@pwd_user_{id}:localhost");
    let username = format!("pwd_user_{id}");

    storage.create_user(&user_id, &username, Some("old_hash"), false).await.unwrap();

    storage.update_password(&user_id, "new_hash").await.unwrap();

    let user = storage.get_user_by_id(&user_id).await.unwrap().unwrap();
    assert_eq!(user.password_hash, Some("new_hash".to_string()));
    assert!(user.password_changed_ts.is_some());
}

#[tokio::test]
async fn test_update_displayname() {
    let pool = crate::require_test_pool().await;
    let storage = create_user_storage(&pool);
    let id = unique_id();
    let user_id = format!("@display_user_{id}:localhost");
    let username = format!("display_user_{id}");

    storage.create_user(&user_id, &username, None, false).await.unwrap();

    storage.update_displayname(&user_id, Some("Display Name")).await.unwrap();

    let user = storage.get_user_by_id(&user_id).await.unwrap().unwrap();
    assert_eq!(user.displayname, Some("Display Name".to_string()));

    storage.update_displayname(&user_id, None).await.unwrap();

    let user = storage.get_user_by_id(&user_id).await.unwrap().unwrap();
    assert_eq!(user.displayname, None);
}

#[tokio::test]
async fn test_update_avatar_url() {
    let pool = crate::require_test_pool().await;
    let storage = create_user_storage(&pool);
    let id = unique_id();
    let user_id = format!("@avatar_user_{id}:localhost");
    let username = format!("avatar_user_{id}");

    storage.create_user(&user_id, &username, None, false).await.unwrap();

    storage.update_avatar_url(&user_id, Some("mxc://localhost/avatar123")).await.unwrap();

    let user = storage.get_user_by_id(&user_id).await.unwrap().unwrap();
    assert_eq!(user.avatar_url, Some("mxc://localhost/avatar123".to_string()));

    storage.update_avatar_url(&user_id, None).await.unwrap();

    let user = storage.get_user_by_id(&user_id).await.unwrap().unwrap();
    assert_eq!(user.avatar_url, None);
}

#[tokio::test]
async fn test_deactivate_user() {
    let pool = crate::require_test_pool().await;
    let storage = create_user_storage(&pool);
    let id = unique_id();
    let user_id = format!("@deact_user_{id}:localhost");
    let username = format!("deact_user_{id}");

    storage.create_user(&user_id, &username, None, false).await.unwrap();

    let user = storage.get_user_by_id(&user_id).await.unwrap().unwrap();
    assert!(!user.is_deactivated);

    storage.deactivate_user(&user_id).await.unwrap();

    let user = storage.get_user_by_id(&user_id).await.unwrap().unwrap();
    assert!(user.is_deactivated);
}

#[tokio::test]
async fn test_set_admin_status() {
    let pool = crate::require_test_pool().await;
    let storage = create_user_storage(&pool);
    let id = unique_id();
    let user_id = format!("@admin_user_{id}:localhost");
    let username = format!("admin_user_{id}");

    storage.create_user(&user_id, &username, None, false).await.unwrap();

    let user = storage.get_user_by_id(&user_id).await.unwrap().unwrap();
    assert!(!user.is_admin);

    storage.set_admin_status(&user_id, true).await.unwrap();

    let user = storage.get_user_by_id(&user_id).await.unwrap().unwrap();
    assert!(user.is_admin);

    storage.set_admin_status(&user_id, false).await.unwrap();

    let user = storage.get_user_by_id(&user_id).await.unwrap().unwrap();
    assert!(!user.is_admin);
}

#[tokio::test]
async fn test_delete_user() {
    let pool = crate::require_test_pool().await;
    let storage = create_user_storage(&pool);
    let id = unique_id();
    let user_id = format!("@delete_user_{id}:localhost");
    let username = format!("delete_user_{id}");

    storage.create_user(&user_id, &username, None, false).await.unwrap();

    assert!(storage.get_user_by_id(&user_id).await.unwrap().is_some());

    storage.delete_user(&user_id).await.unwrap();

    assert!(storage.get_user_by_id(&user_id).await.unwrap().is_none());
}

#[tokio::test]
async fn test_search_users_by_username() {
    let pool = crate::require_test_pool().await;
    if sqlx::query("CREATE EXTENSION IF NOT EXISTS pg_trgm").execute(pool.as_ref()).await.is_err() {
        eprintln!("Skipping test_search_users_by_username: pg_trgm not available");
        return;
    }
    let has_trgm = sqlx::query("SELECT similarity('test', 'test')").execute(pool.as_ref()).await.is_ok();
    if !has_trgm {
        eprintln!("Skipping test_search_users_by_username: similarity() not available in search_path");
        return;
    }
    let storage = create_user_storage(&pool);
    let id = unique_id();
    let user_id = format!("@search_user_{id}:localhost");
    let username = format!("search_user_{id}");

    storage.create_user(&user_id, &username, None, false).await.unwrap();

    let results = storage.search_users(&username, 10).await.unwrap();
    assert!(!results.is_empty());
    assert_eq!(results[0].user_id, user_id);
}

#[tokio::test]
async fn test_search_users_empty_query() {
    let pool = crate::require_test_pool().await;
    let storage = create_user_storage(&pool);

    let results = storage.search_users("", 10).await.unwrap();
    assert!(results.is_empty());

    let results = storage.search_users("   ", 10).await.unwrap();
    assert!(results.is_empty());
}

#[tokio::test]
async fn test_search_users_excludes_deactivated() {
    let pool = crate::require_test_pool().await;
    if sqlx::query("CREATE EXTENSION IF NOT EXISTS pg_trgm").execute(pool.as_ref()).await.is_err() {
        eprintln!("Skipping test_search_users_excludes_deactivated: pg_trgm not available");
        return;
    }
    let has_trgm = sqlx::query("SELECT similarity('test', 'test')").execute(pool.as_ref()).await.is_ok();
    if !has_trgm {
        eprintln!("Skipping test_search_users_excludes_deactivated: similarity() not available in search_path");
        return;
    }
    let storage = create_user_storage(&pool);
    let id = unique_id();
    let user_id = format!("@deact_search_{id}:localhost");
    let username = format!("deact_search_{id}");

    storage.create_user(&user_id, &username, None, false).await.unwrap();

    storage.deactivate_user(&user_id).await.unwrap();

    let results = storage.search_users(&username, 10).await.unwrap();
    let found = results.iter().any(|r| r.user_id == user_id);
    assert!(!found);
}

#[tokio::test]
async fn test_filter_existing_users() {
    let pool = crate::require_test_pool().await;
    let storage = create_user_storage(&pool);
    let id = unique_id();
    let user_id1 = format!("@filter_user_a_{id}:localhost");
    let username1 = format!("filter_user_a_{id}");
    let user_id2 = format!("@filter_user_b_{id}:localhost");
    let username2 = format!("filter_user_b_{id}");

    storage.create_user(&user_id1, &username1, None, false).await.unwrap();
    storage.create_user(&user_id2, &username2, None, false).await.unwrap();

    let input = vec![user_id1.clone(), "@nonexistent:localhost".to_string(), user_id2.clone()];
    let existing = storage.filter_existing_users(&input).await.unwrap();
    assert_eq!(existing.len(), 2);
    assert!(existing.contains(&user_id1));
    assert!(existing.contains(&user_id2));
}

#[tokio::test]
async fn test_filter_existing_users_empty_input() {
    let pool = crate::require_test_pool().await;
    let storage = create_user_storage(&pool);

    let existing = storage.filter_existing_users(&[]).await.unwrap();
    assert!(existing.is_empty());
}

#[tokio::test]
async fn test_filter_existing_users_excludes_deactivated() {
    let pool = crate::require_test_pool().await;
    let storage = create_user_storage(&pool);
    let id = unique_id();
    let user_id = format!("@filter_deact_{id}:localhost");
    let username = format!("filter_deact_{id}");

    storage.create_user(&user_id, &username, None, false).await.unwrap();
    storage.deactivate_user(&user_id).await.unwrap();

    let existing = storage.filter_existing_users(std::slice::from_ref(&user_id)).await.unwrap();
    assert!(!existing.contains(&user_id));
}

#[tokio::test]
async fn test_get_user_profile() {
    let pool = crate::require_test_pool().await;
    let storage = create_user_storage(&pool);
    let id = unique_id();
    let user_id = format!("@profile_user_{id}:localhost");
    let username = format!("profile_user_{id}");

    storage.create_user(&user_id, &username, None, false).await.unwrap();

    storage.update_displayname(&user_id, Some("Profile Name")).await.unwrap();

    let profile = storage.get_user_profile(&user_id).await.unwrap().unwrap();
    assert_eq!(profile.user_id, user_id);
    assert_eq!(profile.username, username);
    assert_eq!(profile.displayname, Some("Profile Name".to_string()));
}

#[tokio::test]
async fn test_get_user_profile_not_found() {
    let pool = crate::require_test_pool().await;
    let storage = create_user_storage(&pool);

    let result = storage.get_user_profile("@noprofile:localhost").await.unwrap();
    assert!(result.is_none());
}

#[tokio::test]
async fn test_get_user_profile_deactivated_returns_none() {
    let pool = crate::require_test_pool().await;
    let storage = create_user_storage(&pool);
    let id = unique_id();
    let user_id = format!("@deact_profile_{id}:localhost");
    let username = format!("deact_profile_{id}");

    storage.create_user(&user_id, &username, None, false).await.unwrap();
    storage.deactivate_user(&user_id).await.unwrap();

    let result = storage.get_user_profile(&user_id).await.unwrap();
    assert!(result.is_none());
}

#[tokio::test]
async fn test_create_user_as_admin() {
    let pool = crate::require_test_pool().await;
    let storage = create_user_storage(&pool);
    let id = unique_id();
    let user_id = format!("@admin_create_{id}:localhost");
    let username = format!("admin_create_{id}");

    let user = storage.create_user(&user_id, &username, Some("adminhash"), true).await.unwrap();

    assert!(user.is_admin);
    assert_eq!(user.password_hash, Some("adminhash".to_string()));
}

#[tokio::test]
async fn test_create_user_no_password() {
    let pool = crate::require_test_pool().await;
    let storage = create_user_storage(&pool);
    let id = unique_id();
    let user_id = format!("@nopwd_user_{id}:localhost");
    let username = format!("nopwd_user_{id}");

    let user = storage.create_user(&user_id, &username, None, false).await.unwrap();

    assert_eq!(user.password_hash, None);
}

// ─────────────────────────────────────────────────────────────────────────────
// B-1.2 批量停用（`set_deactivation_status_batch`）与按 appservice 分组计数。
//
// 这两条路径此前没有任何测试：批量停用是管理面一次点 N 个用户的唯一实现，
// 分组计数是 appservice 配额/统计的取数口径。断言的落点都是"别多做"——
// 返回集合多一个 id 代表幽灵用户被当成停用成功，UPDATE 漏 WHERE 代表全库被停用。
// ─────────────────────────────────────────────────────────────────────────────

#[tokio::test]
async fn test_set_deactivation_status_batch_returns_only_existing_users() {
    let pool = crate::require_test_pool().await;
    let storage = create_user_storage(&pool);
    let id = unique_id();

    let first = format!("@batch_deact_a_{id}:localhost");
    let second = format!("@batch_deact_b_{id}:localhost");
    let bystander = format!("@batch_deact_keep_{id}:localhost");
    let ghost = format!("@batch_deact_missing_{id}:localhost");

    storage.create_user(&first, &format!("batch_deact_a_{id}"), None, false).await.unwrap();
    storage.create_user(&second, &format!("batch_deact_b_{id}"), None, false).await.unwrap();
    storage.create_user(&bystander, &format!("batch_deact_keep_{id}"), None, false).await.unwrap();

    let updated = storage.set_deactivation_status_batch(&[first.clone(), second.clone(), ghost], true).await.unwrap();

    // 返回集合必须恰好是"库里真实存在"的那两个 id；service 层靠
    // `input - returned` 推导 failed，多返回一个就会把幽灵用户算成成功。
    let expected: HashSet<String> = [first.clone(), second].into_iter().collect();
    assert_eq!(updated, expected, "batch update must return exactly the ids that exist");

    assert!(storage.get_user_by_id(&first).await.unwrap().unwrap().is_deactivated);

    // 反向断言：不在入参里的用户必须毫发无损（防 UPDATE 漏 WHERE / WHERE 过宽）。
    assert!(
        !storage.get_user_by_id(&bystander).await.unwrap().unwrap().is_deactivated,
        "users outside the batch must not be touched"
    );
}

#[tokio::test]
async fn test_set_deactivation_status_batch_empty_input_touches_nothing() {
    let pool = crate::require_test_pool().await;
    let storage = create_user_storage(&pool);
    let id = unique_id();
    let user_id = format!("@batch_deact_empty_{id}:localhost");

    storage.create_user(&user_id, &format!("batch_deact_empty_{id}"), None, false).await.unwrap();

    let updated = storage.set_deactivation_status_batch(&[], true).await.unwrap();

    assert!(updated.is_empty(), "empty input must yield an empty result set");
    // 反向断言：空入参不得退化成"全库停用"（`user_id = ANY('{}')` 恰好是安全的，
    // 这条用例锁住它别被改成不带谓词的 UPDATE）。
    assert!(!storage.get_user_by_id(&user_id).await.unwrap().unwrap().is_deactivated);
}

#[tokio::test]
async fn test_set_deactivation_status_batch_is_symmetric_for_reactivation() {
    let pool = crate::require_test_pool().await;
    let storage = create_user_storage(&pool);
    let id = unique_id();
    let user_id = format!("@batch_reactivate_{id}:localhost");

    storage.create_user(&user_id, &format!("batch_reactivate_{id}"), None, false).await.unwrap();
    let ids = vec![user_id.clone()];

    let deactivated = storage.set_deactivation_status_batch(&ids, true).await.unwrap();
    assert_eq!(deactivated.len(), 1);
    assert!(storage.get_user_by_id(&user_id).await.unwrap().unwrap().is_deactivated);

    let restored = storage.set_deactivation_status_batch(&ids, false).await.unwrap();
    assert_eq!(restored.len(), 1, "reactivation must still report the row as updated");
    // 反向断言：值必须真的跟着参数走（防参数被硬编码成 TRUE）。
    assert!(!storage.get_user_by_id(&user_id).await.unwrap().unwrap().is_deactivated);
}

/// 播种一条带 `appservice_id` 的用户行。
///
/// `create_user` 不暴露 `appservice_id`，而 `is_deactivated` 需要能显式写 NULL
/// 才能覆盖 `COALESCE(is_deactivated, FALSE)` 那一支，所以这里直接写表。
async fn seed_appservice_user(
    pool: &Arc<sqlx::PgPool>,
    user_id: &str,
    username: &str,
    appservice_id: &str,
    is_deactivated: Option<bool>,
) {
    sqlx::query(
        r#"
            INSERT INTO users (user_id, username, appservice_id, is_deactivated, created_ts, generation)
            VALUES ($1, $2, $3, $4, $5, $6)
            "#,
    )
    .bind(user_id)
    .bind(username)
    .bind(appservice_id)
    .bind(is_deactivated)
    .bind(current_timestamp_millis())
    .bind(current_timestamp_millis())
    .execute(pool.as_ref())
    .await
    .unwrap();
}

#[tokio::test]
async fn test_count_non_deactivated_users_by_app_service_groups_and_excludes_deactivated() {
    let pool = crate::require_test_pool().await;
    let storage = create_user_storage(&pool);
    let id = unique_id();
    // 用本次唯一的 appservice_id 做分组键，这样并发跑的其它用例不会污染计数。
    let live_service = format!("as_live_{id}");
    let dead_service = format!("as_dead_{id}");

    let seed = |suffix: &str, service: &str, deactivated: Option<bool>| {
        let user_id = format!("@as_{suffix}_{id}:localhost");
        let username = format!("as_{suffix}_{id}");
        (user_id, username, service.to_string(), deactivated)
    };

    for (user_id, username, service, deactivated) in [
        seed("live_a", &live_service, Some(false)),
        seed("live_b", &live_service, Some(false)),
        seed("gone", &live_service, Some(true)),
        // is_deactivated 是 NULL：COALESCE 必须把它当"未停用"。
        seed("nullflag", &live_service, None),
        // 整组都被停用 ⇒ GROUP BY 之后这一组不存在，而不是报 0。
        seed("dead", &dead_service, Some(true)),
    ] {
        seed_appservice_user(&pool, &user_id, &username, &service, deactivated).await;
    }

    let counts = storage.count_non_deactivated_users_by_app_service().await.unwrap();

    assert_eq!(
        counts.get(&live_service),
        Some(&3),
        "2 个显式 FALSE + 1 个 NULL(is_deactivated) 都算未停用；已停用的那个不能计入"
    );
    assert!(!counts.contains_key(&dead_service), "全组停用的分组必须整组消失（GROUP BY 后无行），不能以 0 出现");
    assert!(!counts.contains_key(&format!("as_absent_{id}")), "没有任何用户的 appservice_id 不能凭空出现");
}

#[tokio::test]
async fn test_count_non_deactivated_users_by_app_service_maps_local_users_to_empty_string() {
    let pool = crate::require_test_pool().await;
    let storage = create_user_storage(&pool);
    let id = unique_id();
    let user_id = format!("@local_null_as_{id}:localhost");

    storage.create_user(&user_id, &format!("local_null_as_{id}"), None, false).await.unwrap();

    let counts = storage.count_non_deactivated_users_by_app_service().await.unwrap();

    // 本机用户 appservice_id IS NULL，口径要求落在空串键下。
    // "" 这一组是全局的（并行用例也在建本地用户），所以只能断言存在且至少含本用例这个，
    // 断言精确值会变成 flaky。
    assert!(counts.contains_key(""), "本机用户（appservice_id IS NULL）必须落在空串键下");
    assert!(counts[""] >= 1, "空串组至少要含本用例刚创建的那个未停用用户");
}
