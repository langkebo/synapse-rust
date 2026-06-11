#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use synapse_rust::storage::token::AccessTokenStorage;
static TEST_COUNTER: AtomicU64 = AtomicU64::new(1);

fn unique_id() -> u64 {
    TEST_COUNTER.fetch_add(1, Ordering::SeqCst)
}

async fn setup_test_database(pool: &Arc<sqlx::PgPool>) {

    sqlx::query(
        r#"
        CREATE TABLE users (
            user_id TEXT NOT NULL PRIMARY KEY,
            username TEXT NOT NULL UNIQUE,
            password_hash TEXT,
            is_admin BOOLEAN DEFAULT FALSE,
            is_guest BOOLEAN DEFAULT FALSE,
            creation_ts BIGINT NOT NULL,
            deactivated BOOLEAN DEFAULT FALSE,
            displayname TEXT,
            avatar_url TEXT
        )
        "#,
    )
    .execute(pool.as_ref())
    .await
    .expect("Failed to create users table");

    sqlx::query(
        r#"
        CREATE TABLE access_tokens (
            id BIGSERIAL PRIMARY KEY,
            token_hash TEXT NOT NULL UNIQUE,
            token TEXT,
            user_id TEXT NOT NULL,
            device_id TEXT,
            created_ts BIGINT NOT NULL,
            expires_at BIGINT,
            last_used_ts BIGINT,
            user_agent TEXT,
            ip_address TEXT,
            is_revoked BOOLEAN DEFAULT FALSE
        )
        "#,
    )
    .execute(pool.as_ref())
    .await
    .expect("Failed to create access_tokens table");

    sqlx::query(
        r#"
        CREATE TABLE token_blacklist (
            id BIGSERIAL PRIMARY KEY,
            token_hash TEXT NOT NULL UNIQUE,
            token TEXT,
            token_type TEXT DEFAULT 'access',
            user_id TEXT,
            is_revoked BOOLEAN DEFAULT TRUE,
            reason TEXT,
            expires_at BIGINT
        )
        "#,
    )
    .execute(pool.as_ref())
    .await
    .expect("Failed to create token_blacklist table");
}

async fn insert_test_user(pool: &Arc<sqlx::PgPool>, user_id: &str, suffix: u64) {
    sqlx::query("INSERT INTO users (user_id, username, creation_ts) VALUES ($1, $2, $3)")
        .bind(user_id)
        .bind(format!("atuser{suffix}"))
        .bind(chrono::Utc::now().timestamp_millis())
        .execute(pool.as_ref())
        .await
        .unwrap();
}

#[tokio::test]
async fn test_create_token_with_device() {
    let pool = crate::require_test_pool().await;
    setup_test_database(&pool).await;

        let storage = AccessTokenStorage::new(&pool);
        let suffix = unique_id();
        let user_id = format!("@at_user_{suffix}:localhost");
        insert_test_user(&pool, &user_id, suffix).await;

        let token_str = format!("syt_token_{suffix}");
        let device_id = format!("DEVICE_{suffix}");
        let future_ts = chrono::Utc::now().timestamp_millis() + 3_600_000;

        let token = storage.create_token(&token_str, &user_id, Some(&device_id), Some(future_ts)).await.unwrap();

        assert!(token.id > 0);
        assert_eq!(token.user_id, user_id);
        assert_eq!(token.device_id, Some(device_id));
        assert!(!token.is_revoked);
        assert!(token.created_ts > 0);
        assert_eq!(token.expires_at, Some(future_ts));
}

#[tokio::test]
async fn test_create_token_without_device() {
    let pool = crate::require_test_pool().await;
    setup_test_database(&pool).await;

        let storage = AccessTokenStorage::new(&pool);
        let suffix = unique_id();
        let user_id = format!("@at_user_{suffix}:localhost");
        insert_test_user(&pool, &user_id, suffix).await;

        let token_str = format!("syt_nodevice_{suffix}");

        let token = storage.create_token(&token_str, &user_id, None, None).await.unwrap();

        assert!(token.id > 0);
        assert!(token.device_id.is_none());
        assert!(token.expires_at.is_none());
        assert!(!token.is_revoked);
}

#[tokio::test]
async fn test_get_token_after_create() {
    let pool = crate::require_test_pool().await;
    setup_test_database(&pool).await;

        let storage = AccessTokenStorage::new(&pool);
        let suffix = unique_id();
        let user_id = format!("@at_user_{suffix}:localhost");
        insert_test_user(&pool, &user_id, suffix).await;

        let token_str = format!("syt_gettest_{suffix}");
        let device_id = format!("DEV_{suffix}");

        storage.create_token(&token_str, &user_id, Some(&device_id), None).await.unwrap();

        let fetched = storage.get_token(&token_str).await.unwrap();
        assert!(fetched.is_some());
        let fetched = fetched.unwrap();
        assert_eq!(fetched.user_id, user_id);
        assert_eq!(fetched.device_id, Some(device_id));
        assert!(!fetched.is_revoked);
}

#[tokio::test]
async fn test_get_token_returns_none_for_missing() {
    let pool = crate::require_test_pool().await;
    setup_test_database(&pool).await;

        let storage = AccessTokenStorage::new(&pool);
        let result = storage.get_token("nonexistent_token_value").await.unwrap();
        assert!(result.is_none());
}

#[tokio::test]
async fn test_get_token_excludes_revoked() {
    let pool = crate::require_test_pool().await;
    setup_test_database(&pool).await;

        let storage = AccessTokenStorage::new(&pool);
        let suffix = unique_id();
        let user_id = format!("@at_user_{suffix}:localhost");
        insert_test_user(&pool, &user_id, suffix).await;

        let token_str = format!("syt_revoked_{suffix}");
        storage.create_token(&token_str, &user_id, None, None).await.unwrap();

        storage.delete_token(&token_str).await.unwrap();

        let fetched = storage.get_token(&token_str).await.unwrap();
        assert!(fetched.is_none());
}

#[tokio::test]
async fn test_get_user_tokens() {
    let pool = crate::require_test_pool().await;
    setup_test_database(&pool).await;

        let storage = AccessTokenStorage::new(&pool);
        let suffix = unique_id();
        let user_id = format!("@at_user_{suffix}:localhost");
        insert_test_user(&pool, &user_id, suffix).await;

        for i in 0..3 {
            let token_str = format!("syt_multi_{suffix}_{i}");
            let device_id = format!("DEV_{suffix}_{i}");
            storage.create_token(&token_str, &user_id, Some(&device_id), None).await.unwrap();
        }

        let tokens = storage.get_user_tokens(&user_id).await.unwrap();
        assert_eq!(tokens.len(), 3);
        assert!(tokens.iter().all(|t| t.user_id == user_id));
}

#[tokio::test]
async fn test_get_user_tokens_returns_empty_for_unknown_user() {
    let pool = crate::require_test_pool().await;
    setup_test_database(&pool).await;

        let storage = AccessTokenStorage::new(&pool);
        let tokens = storage.get_user_tokens("@nonexistent:localhost").await.unwrap();
        assert!(tokens.is_empty());
}

#[tokio::test]
async fn test_delete_token() {
    let pool = crate::require_test_pool().await;
    setup_test_database(&pool).await;

        let storage = AccessTokenStorage::new(&pool);
        let suffix = unique_id();
        let user_id = format!("@at_user_{suffix}:localhost");
        insert_test_user(&pool, &user_id, suffix).await;

        let token_str = format!("syt_delete_{suffix}");
        storage.create_token(&token_str, &user_id, None, None).await.unwrap();

        let before = storage.get_token(&token_str).await.unwrap();
        assert!(before.is_some());

        storage.delete_token(&token_str).await.unwrap();

        let after = storage.get_token(&token_str).await.unwrap();
        assert!(after.is_none());
}

#[tokio::test]
async fn test_delete_token_is_soft_delete() {
    let pool = crate::require_test_pool().await;
    setup_test_database(&pool).await;

        let storage = AccessTokenStorage::new(&pool);
        let suffix = unique_id();
        let user_id = format!("@at_user_{suffix}:localhost");
        insert_test_user(&pool, &user_id, suffix).await;

        let token_str = format!("syt_softdel_{suffix}");
        storage.create_token(&token_str, &user_id, None, None).await.unwrap();

        storage.delete_token(&token_str).await.unwrap();

        let tokens = storage.get_user_tokens(&user_id).await.unwrap();
        assert_eq!(tokens.len(), 1);
        assert!(tokens[0].is_revoked);
}

#[tokio::test]
async fn test_delete_user_tokens() {
    let pool = crate::require_test_pool().await;
    setup_test_database(&pool).await;

        let storage = AccessTokenStorage::new(&pool);
        let suffix = unique_id();
        let user_id = format!("@at_user_{suffix}:localhost");
        insert_test_user(&pool, &user_id, suffix).await;

        for i in 0..3 {
            let token_str = format!("syt_userdel_{suffix}_{i}");
            storage.create_token(&token_str, &user_id, None, None).await.unwrap();
        }

        storage.delete_user_tokens(&user_id).await.unwrap();

        let tokens = storage.get_user_tokens(&user_id).await.unwrap();
        assert!(tokens.iter().all(|t| t.is_revoked));
}

#[tokio::test]
async fn test_delete_user_tokens_skips_already_revoked() {
    let pool = crate::require_test_pool().await;
    setup_test_database(&pool).await;

        let storage = AccessTokenStorage::new(&pool);
        let suffix = unique_id();
        let user_id = format!("@at_user_{suffix}:localhost");
        insert_test_user(&pool, &user_id, suffix).await;

        let token_str_a = format!("syt_skipdel_{suffix}_a");
        let token_str_b = format!("syt_skipdel_{suffix}_b");
        storage.create_token(&token_str_a, &user_id, None, None).await.unwrap();
        storage.create_token(&token_str_b, &user_id, None, None).await.unwrap();

        storage.delete_token(&token_str_a).await.unwrap();

        storage.delete_user_tokens(&user_id).await.unwrap();

        let tokens = storage.get_user_tokens(&user_id).await.unwrap();
        assert_eq!(tokens.len(), 2);
        assert!(tokens.iter().all(|t| t.is_revoked));
}

#[tokio::test]
async fn test_delete_device_tokens() {
    let pool = crate::require_test_pool().await;
    setup_test_database(&pool).await;

        let storage = AccessTokenStorage::new(&pool);
        let suffix = unique_id();
        let user_id = format!("@at_user_{suffix}:localhost");
        insert_test_user(&pool, &user_id, suffix).await;

        let device_a = format!("device_a_{suffix}");
        let device_b = format!("device_b_{suffix}");

        let token_str_a = format!("syt_devdel_{suffix}_a");
        let token_str_b = format!("syt_devdel_{suffix}_b");
        storage.create_token(&token_str_a, &user_id, Some(&device_a), None).await.unwrap();
        storage.create_token(&token_str_b, &user_id, Some(&device_b), None).await.unwrap();

        storage.delete_device_tokens(&device_a).await.unwrap();

        let tokens = storage.get_user_tokens(&user_id).await.unwrap();
        let device_a_tokens: Vec<_> =
            tokens.iter().filter(|t| t.device_id.as_deref() == Some(device_a.as_str())).collect();
        let device_b_tokens: Vec<_> =
            tokens.iter().filter(|t| t.device_id.as_deref() == Some(device_b.as_str())).collect();

        assert!(device_a_tokens.iter().all(|t| t.is_revoked));
        assert!(device_b_tokens.iter().all(|t| !t.is_revoked));
}

#[tokio::test]
async fn test_delete_user_device_tokens() {
    let pool = crate::require_test_pool().await;
    setup_test_database(&pool).await;

        let storage = AccessTokenStorage::new(&pool);
        let suffix = unique_id();
        let user_id = format!("@at_user_{suffix}:localhost");
        insert_test_user(&pool, &user_id, suffix).await;

        let device_a = format!("device_a_{suffix}");
        let device_b = format!("device_b_{suffix}");

        let token_str_a = format!("syt_uddevdel_{suffix}_a");
        let token_str_b = format!("syt_uddevdel_{suffix}_b");
        storage.create_token(&token_str_a, &user_id, Some(&device_a), None).await.unwrap();
        storage.create_token(&token_str_b, &user_id, Some(&device_b), None).await.unwrap();

        storage.delete_user_device_tokens(&user_id, &device_a).await.unwrap();

        let tokens = storage.get_user_tokens(&user_id).await.unwrap();
        let device_a_tokens: Vec<_> =
            tokens.iter().filter(|t| t.device_id.as_deref() == Some(device_a.as_str())).collect();
        let device_b_tokens: Vec<_> =
            tokens.iter().filter(|t| t.device_id.as_deref() == Some(device_b.as_str())).collect();

        assert!(device_a_tokens.iter().all(|t| t.is_revoked));
        assert!(device_b_tokens.iter().all(|t| !t.is_revoked));
}

#[tokio::test]
async fn test_delete_user_tokens_except_device() {
    let pool = crate::require_test_pool().await;
    setup_test_database(&pool).await;

        let storage = AccessTokenStorage::new(&pool);
        let suffix = unique_id();
        let user_id = format!("@at_user_{suffix}:localhost");
        insert_test_user(&pool, &user_id, suffix).await;

        let device_keep = format!("device_keep_{suffix}");
        let device_revoke = format!("device_revoke_{suffix}");

        let token_str_keep = format!("syt_except_{suffix}_keep");
        let token_str_revoke = format!("syt_except_{suffix}_revoke");
        storage.create_token(&token_str_keep, &user_id, Some(&device_keep), None).await.unwrap();
        storage.create_token(&token_str_revoke, &user_id, Some(&device_revoke), None).await.unwrap();

        storage.delete_user_tokens_except_device(&user_id, &device_keep).await.unwrap();

        let tokens = storage.get_user_tokens(&user_id).await.unwrap();
        let keep_tokens: Vec<_> =
            tokens.iter().filter(|t| t.device_id.as_deref() == Some(device_keep.as_str())).collect();
        let revoke_tokens: Vec<_> =
            tokens.iter().filter(|t| t.device_id.as_deref() == Some(device_revoke.as_str())).collect();

        assert!(keep_tokens.iter().all(|t| !t.is_revoked));
        assert!(revoke_tokens.iter().all(|t| t.is_revoked));
}

#[tokio::test]
async fn test_token_exists_true() {
    let pool = crate::require_test_pool().await;
    setup_test_database(&pool).await;

        let storage = AccessTokenStorage::new(&pool);
        let suffix = unique_id();
        let user_id = format!("@at_user_{suffix}:localhost");
        insert_test_user(&pool, &user_id, suffix).await;

        let token_str = format!("syt_exists_{suffix}");
        storage.create_token(&token_str, &user_id, None, None).await.unwrap();

        let exists = storage.token_exists(&token_str).await.unwrap();
        assert!(exists);
}

#[tokio::test]
async fn test_token_exists_false_for_missing() {
    let pool = crate::require_test_pool().await;
    setup_test_database(&pool).await;

        let storage = AccessTokenStorage::new(&pool);
        let exists = storage.token_exists("nonexistent_token").await.unwrap();
        assert!(!exists);
}

#[tokio::test]
async fn test_token_exists_false_for_revoked() {
    let pool = crate::require_test_pool().await;
    setup_test_database(&pool).await;

        let storage = AccessTokenStorage::new(&pool);
        let suffix = unique_id();
        let user_id = format!("@at_user_{suffix}:localhost");
        insert_test_user(&pool, &user_id, suffix).await;

        let token_str = format!("syt_existsrev_{suffix}");
        storage.create_token(&token_str, &user_id, None, None).await.unwrap();

        storage.delete_token(&token_str).await.unwrap();

        let exists = storage.token_exists(&token_str).await.unwrap();
        assert!(!exists);
}

#[tokio::test]
async fn test_is_token_revoked_true() {
    let pool = crate::require_test_pool().await;
    setup_test_database(&pool).await;

        let storage = AccessTokenStorage::new(&pool);
        let suffix = unique_id();
        let user_id = format!("@at_user_{suffix}:localhost");
        insert_test_user(&pool, &user_id, suffix).await;

        let token_str = format!("syt_revcheck_{suffix}");
        storage.create_token(&token_str, &user_id, None, None).await.unwrap();

        storage.delete_token(&token_str).await.unwrap();

        let revoked = storage.is_token_revoked(&token_str).await.unwrap();
        assert!(revoked);
}

#[tokio::test]
async fn test_is_token_revoked_false_for_active() {
    let pool = crate::require_test_pool().await;
    setup_test_database(&pool).await;

        let storage = AccessTokenStorage::new(&pool);
        let suffix = unique_id();
        let user_id = format!("@at_user_{suffix}:localhost");
        insert_test_user(&pool, &user_id, suffix).await;

        let token_str = format!("syt_activecheck_{suffix}");
        storage.create_token(&token_str, &user_id, None, None).await.unwrap();

        let revoked = storage.is_token_revoked(&token_str).await.unwrap();
        assert!(!revoked);
}

#[tokio::test]
async fn test_is_token_revoked_false_for_missing() {
    let pool = crate::require_test_pool().await;
    setup_test_database(&pool).await;

        let storage = AccessTokenStorage::new(&pool);
        let revoked = storage.is_token_revoked("missing_token").await.unwrap();
        assert!(!revoked);
}

#[tokio::test]
async fn test_add_to_blacklist_and_is_in_blacklist() {
    let pool = crate::require_test_pool().await;
    setup_test_database(&pool).await;

        let storage = AccessTokenStorage::new(&pool);
        let suffix = unique_id();
        let token_str = format!("syt_blacklist_{suffix}");
        let user_id = format!("@at_user_{suffix}:localhost");

        let blacklisted = storage.is_in_blacklist(&token_str).await.unwrap();
        assert!(!blacklisted);

        storage.add_to_blacklist(&token_str, &user_id, Some("compromised")).await.unwrap();

        let blacklisted = storage.is_in_blacklist(&token_str).await.unwrap();
        assert!(blacklisted);
}

#[tokio::test]
async fn test_add_to_blacklist_without_reason() {
    let pool = crate::require_test_pool().await;
    setup_test_database(&pool).await;

        let storage = AccessTokenStorage::new(&pool);
        let suffix = unique_id();
        let token_str = format!("syt_blacklist_noreason_{suffix}");
        let user_id = format!("@at_user_{suffix}:localhost");

        storage.add_to_blacklist(&token_str, &user_id, None).await.unwrap();

        let blacklisted = storage.is_in_blacklist(&token_str).await.unwrap();
        assert!(blacklisted);
}

#[tokio::test]
async fn test_add_hash_to_blacklist() {
    let pool = crate::require_test_pool().await;
    setup_test_database(&pool).await;

        let storage = AccessTokenStorage::new(&pool);
        let suffix = unique_id();
        let hash = format!("direct_hash_{suffix}");
        let user_id = format!("@at_user_{suffix}:localhost");

        storage.add_hash_to_blacklist(&hash, &user_id, Some("direct hash insert")).await.unwrap();

        let count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM token_blacklist WHERE token_hash = $1")
            .bind(&hash)
            .fetch_one(pool.as_ref())
            .await
            .unwrap();
        assert_eq!(count, 1);
}

#[tokio::test]
async fn test_add_to_blacklist_idempotent() {
    let pool = crate::require_test_pool().await;
    setup_test_database(&pool).await;

        let storage = AccessTokenStorage::new(&pool);
        let suffix = unique_id();
        let token_str = format!("syt_idempotent_{suffix}");
        let user_id = format!("@at_user_{suffix}:localhost");

        storage.add_to_blacklist(&token_str, &user_id, None).await.unwrap();

        storage.add_to_blacklist(&token_str, &user_id, None).await.unwrap();

        let blacklisted = storage.is_in_blacklist(&token_str).await.unwrap();
        assert!(blacklisted);
}

#[tokio::test]
async fn test_is_in_blacklist_false_for_missing() {
    let pool = crate::require_test_pool().await;
    setup_test_database(&pool).await;

        let storage = AccessTokenStorage::new(&pool);
        let blacklisted = storage.is_in_blacklist("not_blacklisted_token").await.unwrap();
        assert!(!blacklisted);
}

#[tokio::test]
async fn test_cleanup_expired_blacklist_entries() {
    let pool = crate::require_test_pool().await;
    setup_test_database(&pool).await;

        let storage = AccessTokenStorage::new(&pool);
        let suffix = unique_id();
        let user_id = format!("@at_user_{suffix}:localhost");
        let past_ts = chrono::Utc::now().timestamp_millis() - 3_600_000;

        sqlx::query(
            r#"
            INSERT INTO token_blacklist (token_hash, token, token_type, user_id, is_revoked, reason, expires_at)
            VALUES ($1, NULL, 'access', $2, TRUE, 'expired entry', $3)
            "#,
        )
        .bind(format!("expired_bl_{suffix}"))
        .bind(&user_id)
        .bind(past_ts)
        .execute(pool.as_ref())
        .await
        .unwrap();

        let deleted = storage.cleanup_expired_blacklist_entries(1800).await.unwrap();
        assert_eq!(deleted, 1);
}

#[tokio::test]
async fn test_cleanup_expired_blacklist_keeps_valid_entries() {
    let pool = crate::require_test_pool().await;
    setup_test_database(&pool).await;

        let storage = AccessTokenStorage::new(&pool);
        let suffix = unique_id();
        let user_id = format!("@at_user_{suffix}:localhost");
        let future_ts = chrono::Utc::now().timestamp_millis() + 3_600_000;

        sqlx::query(
            r#"
            INSERT INTO token_blacklist (token_hash, token, token_type, user_id, is_revoked, reason, expires_at)
            VALUES ($1, NULL, 'access', $2, TRUE, 'valid entry', $3)
            "#,
        )
        .bind(format!("valid_bl_{suffix}"))
        .bind(&user_id)
        .bind(future_ts)
        .execute(pool.as_ref())
        .await
        .unwrap();

        let deleted = storage.cleanup_expired_blacklist_entries(1800).await.unwrap();
        assert_eq!(deleted, 0);
}

#[tokio::test]
async fn test_cleanup_expired_tokens() {
    let pool = crate::require_test_pool().await;
    setup_test_database(&pool).await;

        let storage = AccessTokenStorage::new(&pool);
        let suffix = unique_id();
        let user_id = format!("@at_user_{suffix}:localhost");
        insert_test_user(&pool, &user_id, suffix).await;

        let past_ts = chrono::Utc::now().timestamp_millis() - 3_600_000;
        let future_ts = chrono::Utc::now().timestamp_millis() + 3_600_000;

        let expired_token = format!("syt_expired_{suffix}");
        let active_token = format!("syt_active_{suffix}");

        storage.create_token(&expired_token, &user_id, None, Some(past_ts)).await.unwrap();
        storage.create_token(&active_token, &user_id, None, Some(future_ts)).await.unwrap();

        let deleted = storage.cleanup_expired_tokens().await.unwrap();
        assert_eq!(deleted, 1);

        let tokens = storage.get_user_tokens(&user_id).await.unwrap();
        assert_eq!(tokens.len(), 1);
}

#[tokio::test]
async fn test_cleanup_expired_tokens_keeps_no_expiry() {
    let pool = crate::require_test_pool().await;
    setup_test_database(&pool).await;

        let storage = AccessTokenStorage::new(&pool);
        let suffix = unique_id();
        let user_id = format!("@at_user_{suffix}:localhost");
        insert_test_user(&pool, &user_id, suffix).await;

        let no_expiry_token = format!("syt_noexpiry_{suffix}");
        storage.create_token(&no_expiry_token, &user_id, None, None).await.unwrap();

        let deleted = storage.cleanup_expired_tokens().await.unwrap();
        assert_eq!(deleted, 0);

        let tokens = storage.get_user_tokens(&user_id).await.unwrap();
        assert_eq!(tokens.len(), 1);
}

#[tokio::test]
async fn test_delete_user_tokens_does_not_affect_other_users() {
    let pool = crate::require_test_pool().await;
    setup_test_database(&pool).await;

        let storage = AccessTokenStorage::new(&pool);
        let suffix_a = unique_id();
        let suffix_b = unique_id();
        let user_a = format!("@at_user_{suffix_a}:localhost");
        let user_b = format!("@at_user_{suffix_b}:localhost");
        insert_test_user(&pool, &user_a, suffix_a).await;
        insert_test_user(&pool, &user_b, suffix_b).await;

        let token_a = format!("syt_usera_{suffix_a}");
        let token_b = format!("syt_userb_{suffix_b}");
        storage.create_token(&token_a, &user_a, None, None).await.unwrap();
        storage.create_token(&token_b, &user_b, None, None).await.unwrap();

        storage.delete_user_tokens(&user_a).await.unwrap();

        let tokens_b = storage.get_user_tokens(&user_b).await.unwrap();
        assert_eq!(tokens_b.len(), 1);
        assert!(!tokens_b[0].is_revoked);
}

#[tokio::test]
async fn test_delete_user_tokens_except_device_with_no_other_devices() {
    let pool = crate::require_test_pool().await;
    setup_test_database(&pool).await;

        let storage = AccessTokenStorage::new(&pool);
        let suffix = unique_id();
        let user_id = format!("@at_user_{suffix}:localhost");
        insert_test_user(&pool, &user_id, suffix).await;

        let device_only = format!("device_only_{suffix}");
        let token_str = format!("syt_onlydev_{suffix}");
        storage.create_token(&token_str, &user_id, Some(&device_only), None).await.unwrap();

        storage.delete_user_tokens_except_device(&user_id, &device_only).await.unwrap();

        let tokens = storage.get_user_tokens(&user_id).await.unwrap();
        assert_eq!(tokens.len(), 1);
        assert!(!tokens[0].is_revoked);
}

#[tokio::test]
async fn test_multiple_tokens_same_device() {
    let pool = crate::require_test_pool().await;
    setup_test_database(&pool).await;

        let storage = AccessTokenStorage::new(&pool);
        let suffix = unique_id();
        let user_id = format!("@at_user_{suffix}:localhost");
        insert_test_user(&pool, &user_id, suffix).await;

        let device_id = format!("shared_device_{suffix}");

        for i in 0..2 {
            let token_str = format!("syt_shared_{suffix}_{i}");
            storage.create_token(&token_str, &user_id, Some(&device_id), None).await.unwrap();
        }

        storage.delete_device_tokens(&device_id).await.unwrap();

        let tokens = storage.get_user_tokens(&user_id).await.unwrap();
        assert!(tokens.iter().all(|t| t.is_revoked));
}
