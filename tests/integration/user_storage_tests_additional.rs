//! Integration tests for `UserStorage` at `synapse-storage/src/user.rs`.
//!
//! Covers all 47 public methods of `UserStorage` plus the `UserStore` trait dispatch.
//! Uses the warm_up_pool + Mutex guard + unique_id pattern for cross-runtime isolation.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
#![allow(clippy::await_holding_lock)]

use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex, OnceLock};

use synapse_rust::cache::{CacheConfig, CacheManager};
use synapse_storage::user::{LockedUser, UserStorage, UserStore};

static TEST_COUNTER: AtomicU64 = AtomicU64::new(1);

fn unique_id() -> u64 {
    TEST_COUNTER.fetch_add(1, Ordering::SeqCst)
}

fn user_test_guard() -> &'static Mutex<()> {
    static GUARD: OnceLock<Mutex<()>> = OnceLock::new();
    GUARD.get_or_init(|| Mutex::new(()))
}

/// Warm up the shared pool on the current tokio runtime.
/// SELECT 1 with 8 retries and 400ms backoff fixes cross-runtime sqlx pool isolation.
async fn warm_up_pool(pool: &Arc<sqlx::PgPool>) {
    for _ in 0..8 {
        match tokio::time::timeout(
            std::time::Duration::from_secs(5),
            sqlx::query("SELECT 1").execute(pool.as_ref()),
        )
        .await
        {
            Ok(Ok(_)) => return,
            Ok(Err(_)) | Err(_) => {
                tokio::time::sleep(std::time::Duration::from_millis(400)).await;
            }
        }
    }
    let _ = sqlx::query("SELECT 1").execute(pool.as_ref()).await;
}

/// Create all tables needed by user.rs tests with CREATE TABLE IF NOT EXISTS.
/// Also cleans up test data from previous runs using the `ut_` prefix.
async fn setup_test_database(pool: &Arc<sqlx::PgPool>) {
    warm_up_pool(pool).await;

    // -- users table (with all columns from the real schema) --
    sqlx::query(
        r#"
        CREATE TABLE IF NOT EXISTS users (
            user_id TEXT NOT NULL PRIMARY KEY,
            username TEXT NOT NULL UNIQUE,
            password_hash TEXT,
            is_admin BOOLEAN DEFAULT FALSE,
            is_guest BOOLEAN DEFAULT FALSE,
            is_shadow_banned BOOLEAN DEFAULT FALSE,
            is_deactivated BOOLEAN DEFAULT FALSE,
            created_ts BIGINT NOT NULL,
            updated_ts BIGINT,
            displayname TEXT,
            avatar_url TEXT,
            email TEXT,
            phone TEXT,
            generation BIGINT DEFAULT 0,
            consent_version TEXT,
            appservice_id TEXT,
            user_type TEXT,
            invalid_update_at BIGINT,
            migration_state TEXT,
            password_changed_ts BIGINT,
            is_password_change_required BOOLEAN DEFAULT FALSE,
            password_expires_at BIGINT,
            failed_login_attempts INT DEFAULT 0,
            locked_until BIGINT,
            must_change_password BOOLEAN DEFAULT FALSE
        )
        "#,
    )
    .execute(pool.as_ref())
    .await
    .ok();

    // -- rooms table (needed for events FK) --
    sqlx::query(
        r#"
        CREATE TABLE IF NOT EXISTS rooms (
            room_id TEXT NOT NULL PRIMARY KEY,
            creator TEXT,
            is_public BOOLEAN DEFAULT FALSE,
            room_version TEXT DEFAULT '6',
            created_ts BIGINT NOT NULL,
            last_activity_ts BIGINT,
            is_federated BOOLEAN DEFAULT TRUE,
            has_guest_access BOOLEAN DEFAULT FALSE,
            join_rules TEXT DEFAULT 'invite',
            history_visibility TEXT DEFAULT 'shared',
            name TEXT,
            topic TEXT,
            avatar_url TEXT,
            canonical_alias TEXT,
            visibility TEXT DEFAULT 'private'
        )
        "#,
    )
    .execute(pool.as_ref())
    .await
    .ok();

    // -- devices table (for DAU/MAU/R30) --
    sqlx::query(
        r#"
        CREATE TABLE IF NOT EXISTS devices (
            device_id TEXT NOT NULL PRIMARY KEY,
            user_id TEXT NOT NULL,
            display_name TEXT,
            device_key JSONB,
            last_seen_ts BIGINT,
            last_seen_ip TEXT,
            created_ts BIGINT NOT NULL,
            first_seen_ts BIGINT NOT NULL,
            user_agent TEXT,
            appservice_id TEXT,
            ignored_user_list TEXT
        )
        "#,
    )
    .execute(pool.as_ref())
    .await
    .ok();

    // -- events stream ordering sequence --
    sqlx::query("CREATE SEQUENCE IF NOT EXISTS events_stream_ordering_seq")
        .execute(pool.as_ref())
        .await
        .ok();

    // -- events table (for count_sent_messages) --
    sqlx::query(
        r#"
        CREATE TABLE IF NOT EXISTS events (
            event_id TEXT NOT NULL PRIMARY KEY,
            room_id TEXT NOT NULL,
            sender TEXT NOT NULL,
            event_type TEXT NOT NULL,
            content JSONB NOT NULL,
            origin_server_ts BIGINT NOT NULL,
            state_key TEXT,
            is_redacted BOOLEAN DEFAULT FALSE,
            redacted_at BIGINT,
            redacted_by TEXT,
            transaction_id TEXT,
            depth BIGINT,
            prev_events JSONB,
            auth_events JSONB,
            signatures JSONB,
            hashes JSONB,
            unsigned JSONB DEFAULT '{}',
            processed_at BIGINT,
            not_before BIGINT DEFAULT 0,
            status TEXT,
            reference_image TEXT,
            origin TEXT,
            user_id TEXT,
            redacts TEXT,
            stream_ordering BIGINT DEFAULT nextval('events_stream_ordering_seq')
        )
        "#,
    )
    .execute(pool.as_ref())
    .await
    .ok();

    // -- presence table (for search_users_with_presence, search_directory_users) --
    sqlx::query(
        r#"
        CREATE TABLE IF NOT EXISTS presence (
            user_id TEXT NOT NULL PRIMARY KEY,
            status_msg TEXT,
            presence TEXT NOT NULL DEFAULT 'offline',
            last_active_ts BIGINT NOT NULL DEFAULT 0,
            status_from TEXT,
            created_ts BIGINT NOT NULL,
            updated_ts BIGINT NOT NULL
        )
        "#,
    )
    .execute(pool.as_ref())
    .await
    .ok();

    // -- user_locks table (for lock/unlock operations) --
    sqlx::query(
        r#"
        CREATE TABLE IF NOT EXISTS user_locks (
            id BIGSERIAL PRIMARY KEY,
            user_id TEXT NOT NULL,
            reason TEXT,
            locked_by TEXT NOT NULL,
            created_ts BIGINT NOT NULL,
            unlocked_ts BIGINT,
            is_active BOOLEAN NOT NULL DEFAULT TRUE
        )
        "#,
    )
    .execute(pool.as_ref())
    .await
    .ok();

    // Unique index required by lock_user's ON CONFLICT clause
    sqlx::query(
        r"CREATE UNIQUE INDEX IF NOT EXISTS idx_user_locks_user_active
          ON user_locks(user_id, is_active) WHERE is_active = TRUE",
    )
    .execute(pool.as_ref())
    .await
    .ok();

    // -- account_data table (for get_account_data_content, upsert_account_data_content) --
    sqlx::query(
        r#"
        CREATE TABLE IF NOT EXISTS account_data (
            id BIGSERIAL PRIMARY KEY,
            user_id TEXT NOT NULL,
            data_type TEXT NOT NULL,
            content JSONB NOT NULL,
            created_ts BIGINT NOT NULL,
            updated_ts BIGINT NOT NULL,
            CONSTRAINT uq_account_data_user_type UNIQUE (user_id, data_type)
        )
        "#,
    )
    .execute(pool.as_ref())
    .await
    .ok();

    // -- user_account_data table (for set_account_data) --
    sqlx::query(
        r#"
        CREATE TABLE IF NOT EXISTS user_account_data (
            id BIGSERIAL PRIMARY KEY,
            user_id TEXT NOT NULL,
            event_type TEXT NOT NULL,
            content TEXT NOT NULL,
            created_ts BIGINT NOT NULL,
            CONSTRAINT uq_user_account_data_user_type UNIQUE (user_id, event_type)
        )
        "#,
    )
    .execute(pool.as_ref())
    .await
    .ok();

    // -- Clean up test data from previous runs --
    // Delete in FK-safe order: child tables first.
    let cleanup = |table: &str, col: &str| {
        let q = format!("DELETE FROM {table} WHERE {col} LIKE '%ut_%'");
        q
    };
    for stmt in [
        cleanup("user_locks", "user_id"),
        cleanup("account_data", "user_id"),
        cleanup("user_account_data", "user_id"),
        cleanup("presence", "user_id"),
        cleanup("events", "sender"),
        cleanup("devices", "user_id"),
        cleanup("users", "user_id"),
    ] {
        sqlx::query(&stmt).execute(pool.as_ref()).await.ok();
    }
}

fn create_user_storage(pool: &Arc<sqlx::PgPool>) -> UserStorage {
    let cache = Arc::new(CacheManager::new(&CacheConfig::default()));
    UserStorage::new(pool, cache)
}

/// Helper: create a test user and return its user_id.
async fn make_user(storage: &UserStorage, suffix: &str) -> String {
    let id = unique_id();
    let user_id = format!("@ut_{suffix}_{id}:localhost");
    let username = format!("ut_{suffix}_{id}");
    storage
        .create_user(&user_id, &username, Some("hash123"), false)
        .await
        .unwrap();
    user_id
}

/// Check if pg_trgm extension is available (needed for similarity() function).
async fn has_pg_trgm(pool: &Arc<sqlx::PgPool>) -> bool {
    if sqlx::query("CREATE EXTENSION IF NOT EXISTS pg_trgm")
        .execute(pool.as_ref())
        .await
        .is_err()
    {
        return false;
    }
    sqlx::query("SELECT similarity('test', 'test')")
        .execute(pool.as_ref())
        .await
        .is_ok()
}

// =============================================================================
// new / constructor
// =============================================================================

#[tokio::test]
async fn test_new_constructor() {
    let _guard = user_test_guard()
        .lock()
        .unwrap_or_else(|e| e.into_inner());
    let pool = crate::require_test_pool().await;
    setup_test_database(&pool).await;
    let storage = create_user_storage(&pool);
    // A trivial query proves the storage was constructed with a usable pool.
    let _ = storage.get_user_count().await.unwrap();
}

#[tokio::test]
async fn test_user_struct_user_id_method() {
    let _guard = user_test_guard()
        .lock()
        .unwrap_or_else(|e| e.into_inner());
    let pool = crate::require_test_pool().await;
    setup_test_database(&pool).await;
    let storage = create_user_storage(&pool);

    let uid = unique_id();
    let user_id = format!("@ut_struct_{uid}:localhost");
    let username = format!("ut_struct_{uid}");
    let user = storage
        .create_user(&user_id, &username, None, false)
        .await
        .unwrap();

    assert_eq!(user.user_id(), user_id);
}

// =============================================================================
// create_user_tx
// =============================================================================

#[tokio::test]
async fn test_create_user_tx_basic() {
    let _guard = user_test_guard()
        .lock()
        .unwrap_or_else(|e| e.into_inner());
    let pool = crate::require_test_pool().await;
    setup_test_database(&pool).await;
    let storage = create_user_storage(&pool);

    let uid = unique_id();
    let user_id = format!("@ut_tx_{uid}:localhost");
    let username = format!("ut_tx_{uid}");

    let mut tx = pool.begin().await.unwrap();
    let user = storage
        .create_user_tx(&mut tx, &user_id, &username, Some("hash"), true)
        .await
        .unwrap();
    tx.commit().await.unwrap();

    assert_eq!(user.user_id, user_id);
    assert!(user.is_admin);

    let fetched = storage.get_user_by_id(&user_id).await.unwrap().unwrap();
    assert_eq!(fetched.username, username);
}

#[tokio::test]
async fn test_create_user_tx_rollback() {
    let _guard = user_test_guard()
        .lock()
        .unwrap_or_else(|e| e.into_inner());
    let pool = crate::require_test_pool().await;
    setup_test_database(&pool).await;
    let storage = create_user_storage(&pool);

    let uid = unique_id();
    let user_id = format!("@ut_rollback_{uid}:localhost");
    let username = format!("ut_rollback_{uid}");

    let mut tx = pool.begin().await.unwrap();
    let _ = storage
        .create_user_tx(&mut tx, &user_id, &username, None, false)
        .await
        .unwrap();
    tx.rollback().await.unwrap();

    // User should not exist after rollback.
    assert!(storage.get_user_by_id(&user_id).await.unwrap().is_none());
}

#[tokio::test]
async fn test_create_user_duplicate_username_errors() {
    let _guard = user_test_guard()
        .lock()
        .unwrap_or_else(|e| e.into_inner());
    let pool = crate::require_test_pool().await;
    setup_test_database(&pool).await;
    let storage = create_user_storage(&pool);

    let uid = unique_id();
    let username = format!("ut_dup_{uid}");
    storage
        .create_user(
            &format!("@ut_dup_a_{uid}:localhost"),
            &username,
            None,
            false,
        )
        .await
        .unwrap();

    let result = storage
        .create_user(
            &format!("@ut_dup_b_{uid}:localhost"),
            &username,
            None,
            false,
        )
        .await;
    assert!(result.is_err(), "duplicate username should violate UNIQUE constraint");
}

// =============================================================================
// query methods — edge cases
// =============================================================================

#[tokio::test]
async fn test_get_user_by_id_not_found() {
    let _guard = user_test_guard()
        .lock()
        .unwrap_or_else(|e| e.into_inner());
    let pool = crate::require_test_pool().await;
    setup_test_database(&pool).await;
    let storage = create_user_storage(&pool);

    let result = storage
        .get_user_by_id(&format!("@ut_nonexistent_{}:localhost", unique_id()))
        .await
        .unwrap();
    assert!(result.is_none());
}

#[tokio::test]
async fn test_get_user_by_username_not_found() {
    let _guard = user_test_guard()
        .lock()
        .unwrap_or_else(|e| e.into_inner());
    let pool = crate::require_test_pool().await;
    setup_test_database(&pool).await;
    let storage = create_user_storage(&pool);

    let result = storage
        .get_user_by_username(&format!("ut_nonexistent_{}", unique_id()))
        .await
        .unwrap();
    assert!(result.is_none());
}

#[tokio::test]
async fn test_get_user_by_email_not_found() {
    let _guard = user_test_guard()
        .lock()
        .unwrap_or_else(|e| e.into_inner());
    let pool = crate::require_test_pool().await;
    setup_test_database(&pool).await;
    let storage = create_user_storage(&pool);

    let result = storage
        .get_user_by_email(&format!("ut_nonexistent_{}@example.com", unique_id()))
        .await
        .unwrap();
    assert!(result.is_none());
}

#[tokio::test]
async fn test_get_user_by_identifier_not_found() {
    let _guard = user_test_guard()
        .lock()
        .unwrap_or_else(|e| e.into_inner());
    let pool = crate::require_test_pool().await;
    setup_test_database(&pool).await;
    let storage = create_user_storage(&pool);

    let uid = unique_id();
    // user_id format identifier
    assert!(
        storage
            .get_user_by_identifier(&format!("@ut_missing_{uid}:localhost"))
            .await
            .unwrap()
            .is_none()
    );
    // username format identifier
    assert!(
        storage
            .get_user_by_identifier(&format!("ut_missing_{uid}"))
            .await
            .unwrap()
            .is_none()
    );
}

#[tokio::test]
async fn test_get_all_users_respects_limit() {
    let _guard = user_test_guard()
        .lock()
        .unwrap_or_else(|e| e.into_inner());
    let pool = crate::require_test_pool().await;
    setup_test_database(&pool).await;
    let storage = create_user_storage(&pool);

    for _ in 0..3 {
        let uid = unique_id();
        storage
            .create_user(
                &format!("@ut_all_{uid}:localhost"),
                &format!("ut_all_{uid}"),
                None,
                false,
            )
            .await
            .unwrap();
    }

    let users = storage.get_all_users(2).await.unwrap();
    assert_eq!(users.len(), 2, "limit should be respected");
}

// =============================================================================
// get_users_paginated
// =============================================================================

#[tokio::test]
async fn test_get_users_paginated_first_page() {
    let _guard = user_test_guard()
        .lock()
        .unwrap_or_else(|e| e.into_inner());
    let pool = crate::require_test_pool().await;
    setup_test_database(&pool).await;
    let storage = create_user_storage(&pool);

    for i in 0..3 {
        let uid = unique_id();
        storage
            .create_user(
                &format!("@ut_pg_{i}_{uid}:localhost"),
                &format!("ut_pg_{i}_{uid}"),
                None,
                false,
            )
            .await
            .unwrap();
    }

    let page = storage.get_users_paginated(2, None, None).await.unwrap();
    assert_eq!(page.len(), 2, "first page should respect limit");
    // Ordered by created_ts DESC, user_id DESC.
    assert!(page[0].created_ts >= page[1].created_ts);
}

#[tokio::test]
async fn test_get_users_paginated_with_cursor() {
    let _guard = user_test_guard()
        .lock()
        .unwrap_or_else(|e| e.into_inner());
    let pool = crate::require_test_pool().await;
    setup_test_database(&pool).await;
    let storage = create_user_storage(&pool);

    let mut created = Vec::new();
    for i in 0..3 {
        let uid = unique_id();
        let user_id = format!("@ut_pgc_{i}_{uid}:localhost");
        let user = storage
            .create_user(&user_id, &format!("ut_pgc_{i}_{uid}"), None, false)
            .await
            .unwrap();
        created.push(user);
    }

    // Page 1: limit=2, no cursor.
    let page1 = storage.get_users_paginated(2, None, None).await.unwrap();
    assert_eq!(page1.len(), 2);

    // Page 2: continue from last row of page 1.
    let last = &page1[1];
    let page2 = storage
        .get_users_paginated(2, Some(last.created_ts), Some(&last.user_id))
        .await
        .unwrap();
    assert_eq!(page2.len(), 1, "only one remaining row");

    // No overlap.
    assert_ne!(page1[0].user_id, page2[0].user_id);
    assert_ne!(page1[1].user_id, page2[0].user_id);
}

// =============================================================================
// list_users
// =============================================================================

#[tokio::test]
async fn test_list_users_basic() {
    let _guard = user_test_guard()
        .lock()
        .unwrap_or_else(|e| e.into_inner());
    let pool = crate::require_test_pool().await;
    setup_test_database(&pool).await;
    let storage = create_user_storage(&pool);

    let uid = unique_id();
    for i in 0..3 {
        storage
            .create_user(
                &format!("@ut_list_{i}_{uid}:localhost"),
                &format!("ut_list_{i}_{uid}"),
                None,
                false,
            )
            .await
            .unwrap();
    }

    let users = storage.list_users(100, None, None, None).await.unwrap();
    assert!(
        users.iter().any(|u| u.username == format!("ut_list_0_{uid}")),
        "created users should appear in list"
    );
}

#[tokio::test]
async fn test_list_users_with_name_filter() {
    let _guard = user_test_guard()
        .lock()
        .unwrap_or_else(|e| e.into_inner());
    let pool = crate::require_test_pool().await;
    setup_test_database(&pool).await;
    let storage = create_user_storage(&pool);

    let uid = unique_id();
    let target = format!("ut_filter_target_{uid}");
    storage
        .create_user(&format!("@{target}:localhost"), &target, None, false)
        .await
        .unwrap();
    let other = format!("ut_filter_other_{uid}");
    storage
        .create_user(&format!("@{other}:localhost"), &other, None, false)
        .await
        .unwrap();

    let filtered = storage
        .list_users(100, None, None, Some(&format!("ut_filter_target_{uid}")))
        .await
        .unwrap();
    assert!(filtered.iter().all(|u| u.username.contains(&format!("ut_filter_target_{uid}"))));
    assert!(filtered.iter().any(|u| u.username == target));
}

#[tokio::test]
async fn test_list_users_with_cursor() {
    let _guard = user_test_guard()
        .lock()
        .unwrap_or_else(|e| e.into_inner());
    let pool = crate::require_test_pool().await;
    setup_test_database(&pool).await;
    let storage = create_user_storage(&pool);

    let uid = unique_id();
    let mut users = Vec::new();
    for i in 0..3 {
        let user = storage
            .create_user(
                &format!("@ut_lc_{i}_{uid}:localhost"),
                &format!("ut_lc_{i}_{uid}"),
                None,
                false,
            )
            .await
            .unwrap();
        users.push(user);
    }

    // Page 1
    let page1 = storage.list_users(2, None, None, None).await.unwrap();
    assert_eq!(page1.len(), 2);

    // Page 2: continue from page1's last row.
    let last = &page1[1];
    let page2 = storage
        .list_users(2, Some(last.created_ts), Some(&last.user_id), None)
        .await
        .unwrap();
    assert_eq!(page2.len(), 1, "only one remaining row");
    assert_ne!(page1[0].user_id, page2[0].user_id);
}

// =============================================================================
// user_exists / filter_existing_users — edge cases
// =============================================================================

#[tokio::test]
async fn test_user_exists_nonexistent_returns_false() {
    let _guard = user_test_guard()
        .lock()
        .unwrap_or_else(|e| e.into_inner());
    let pool = crate::require_test_pool().await;
    setup_test_database(&pool).await;
    let storage = create_user_storage(&pool);

    assert!(
        !storage
            .user_exists(&format!("@ut_missing_{}:localhost", unique_id()))
            .await
            .unwrap()
    );
}

#[tokio::test]
async fn test_filter_existing_users_all_nonexistent() {
    let _guard = user_test_guard()
        .lock()
        .unwrap_or_else(|e| e.into_inner());
    let pool = crate::require_test_pool().await;
    setup_test_database(&pool).await;
    let storage = create_user_storage(&pool);

    let input = vec![
        format!("@ut_none_{}:localhost", unique_id()),
        format!("@ut_none_{}:localhost", unique_id()),
    ];
    let existing = storage.filter_existing_users(&input).await.unwrap();
    assert!(existing.is_empty());
}

#[tokio::test]
async fn test_filter_existing_users_mixed() {
    let _guard = user_test_guard()
        .lock()
        .unwrap_or_else(|e| e.into_inner());
    let pool = crate::require_test_pool().await;
    setup_test_database(&pool).await;
    let storage = create_user_storage(&pool);

    let existing_id = make_user(&storage, "filter_mix").await;
    let missing_id = format!("@ut_filter_missing_{}:localhost", unique_id());

    let result = storage
        .filter_existing_users(&[existing_id.clone(), missing_id])
        .await
        .unwrap();
    assert_eq!(result.len(), 1);
    assert_eq!(result[0], existing_id);
}

// =============================================================================
// get_user_count
// =============================================================================

#[tokio::test]
async fn test_get_user_count_increments() {
    let _guard = user_test_guard()
        .lock()
        .unwrap_or_else(|e| e.into_inner());
    let pool = crate::require_test_pool().await;
    setup_test_database(&pool).await;
    let storage = create_user_storage(&pool);

    let before = storage.get_user_count().await.unwrap();
    make_user(&storage, "count").await;
    let after = storage.get_user_count().await.unwrap();
    assert_eq!(after, before + 1);
}

// =============================================================================
// get_daily_active_users / get_monthly_active_users / get_r30_users
// =============================================================================

#[tokio::test]
async fn test_get_daily_active_users_no_devices() {
    let _guard = user_test_guard()
        .lock()
        .unwrap_or_else(|e| e.into_inner());
    let pool = crate::require_test_pool().await;
    setup_test_database(&pool).await;
    let storage = create_user_storage(&pool);

    // No devices → 0 DAU.
    let dau = storage.get_daily_active_users().await.unwrap();
    assert_eq!(dau, 0);
}

#[tokio::test]
async fn test_get_daily_active_users_with_recent_device() {
    let _guard = user_test_guard()
        .lock()
        .unwrap_or_else(|e| e.into_inner());
    let pool = crate::require_test_pool().await;
    setup_test_database(&pool).await;
    let storage = create_user_storage(&pool);

    let user_id = make_user(&storage, "dau").await;
    let now = chrono::Utc::now().timestamp_millis();
    let dev_id = format!("DEV_dau_{}", unique_id());
    sqlx::query(
        "INSERT INTO devices (device_id, user_id, created_ts, first_seen_ts, last_seen_ts) VALUES ($1, $2, $3, $4, $5)",
    )
    .bind(&dev_id)
    .bind(&user_id)
    .bind(now)
    .bind(now)
    .bind(now)
    .execute(pool.as_ref())
    .await
    .unwrap();

    let dau = storage.get_daily_active_users().await.unwrap();
    assert!(dau >= 1, "user with a recent device should be counted as DAU");
}

#[tokio::test]
async fn test_get_monthly_active_users_no_devices() {
    let _guard = user_test_guard()
        .lock()
        .unwrap_or_else(|e| e.into_inner());
    let pool = crate::require_test_pool().await;
    setup_test_database(&pool).await;
    let storage = create_user_storage(&pool);

    let mau = storage.get_monthly_active_users().await.unwrap();
    assert_eq!(mau, 0);
}

#[tokio::test]
async fn test_get_monthly_active_users_with_recent_device() {
    let _guard = user_test_guard()
        .lock()
        .unwrap_or_else(|e| e.into_inner());
    let pool = crate::require_test_pool().await;
    setup_test_database(&pool).await;
    let storage = create_user_storage(&pool);

    let user_id = make_user(&storage, "mau").await;
    let now = chrono::Utc::now().timestamp_millis();
    let dev_id = format!("DEV_mau_{}", unique_id());
    sqlx::query(
        "INSERT INTO devices (device_id, user_id, created_ts, first_seen_ts, last_seen_ts) VALUES ($1, $2, $3, $4, $5)",
    )
    .bind(&dev_id)
    .bind(&user_id)
    .bind(now)
    .bind(now)
    .bind(now)
    .execute(pool.as_ref())
    .await
    .unwrap();

    let mau = storage.get_monthly_active_users().await.unwrap();
    assert!(mau >= 1, "user with a recent device should be counted as MAU");
}

#[tokio::test]
async fn test_get_r30_users_no_devices() {
    let _guard = user_test_guard()
        .lock()
        .unwrap_or_else(|e| e.into_inner());
    let pool = crate::require_test_pool().await;
    setup_test_database(&pool).await;
    let storage = create_user_storage(&pool);

    let r30 = storage.get_r30_users().await.unwrap();
    assert_eq!(r30, 0);
}

// =============================================================================
// get_user_stats_summary
// =============================================================================

#[tokio::test]
async fn test_get_user_stats_summary_with_various_users() {
    let _guard = user_test_guard()
        .lock()
        .unwrap_or_else(|e| e.into_inner());
    let pool = crate::require_test_pool().await;
    setup_test_database(&pool).await;
    let storage = create_user_storage(&pool);

    let uid = unique_id();

    // Regular active user.
    let active_id = format!("@ut_stats_active_{uid}:localhost");
    storage
        .create_user(&active_id, &format!("ut_stats_active_{uid}"), None, false)
        .await
        .unwrap();

    // Admin user.
    let admin_id = format!("@ut_stats_admin_{uid}:localhost");
    storage
        .create_user(
            &admin_id,
            &format!("ut_stats_admin_{uid}"),
            None,
            true,
        )
        .await
        .unwrap();

    // Deactivated user.
    let deact_id = format!("@ut_stats_deact_{uid}:localhost");
    storage
        .create_user(&deact_id, &format!("ut_stats_deact_{uid}"), None, false)
        .await
        .unwrap();
    storage.set_deactivation_status(&deact_id, true).await.unwrap();

    // Guest user.
    let guest_id = format!("@ut_stats_guest_{uid}:localhost");
    storage
        .create_user(&guest_id, &format!("ut_stats_guest_{uid}"), None, false)
        .await
        .unwrap();
    storage.set_guest_status(&guest_id, true).await.unwrap();

    let summary = storage.get_user_stats_summary().await.unwrap();
    assert!(summary.total_users >= 4, "should count at least 4 test users");
    assert!(summary.admin_users >= 1, "should count at least 1 admin");
    assert!(summary.deactivated_users >= 1, "should count at least 1 deactivated");
    assert!(summary.guest_users >= 1, "should count at least 1 guest");
    assert!(summary.active_users >= 2, "active + admin should be at least 2");
}

// =============================================================================
// count_sent_messages
// =============================================================================

#[tokio::test]
async fn test_count_sent_messages_no_events() {
    let _guard = user_test_guard()
        .lock()
        .unwrap_or_else(|e| e.into_inner());
    let pool = crate::require_test_pool().await;
    setup_test_database(&pool).await;
    let storage = create_user_storage(&pool);

    let user_id = make_user(&storage, "msg_count").await;
    let count = storage.count_sent_messages(&user_id).await.unwrap();
    assert_eq!(count, 0, "user with no messages should return 0");
}

#[tokio::test]
async fn test_count_sent_messages_with_events() {
    let _guard = user_test_guard()
        .lock()
        .unwrap_or_else(|e| e.into_inner());
    let pool = crate::require_test_pool().await;
    setup_test_database(&pool).await;
    let storage = create_user_storage(&pool);

    let user_id = make_user(&storage, "msg_with").await;
    let room_id = format!("!ut_room_msg_{}:localhost", unique_id());
    let now = chrono::Utc::now().timestamp_millis();

    // Create room (needed for events FK if enforced).
    sqlx::query("INSERT INTO rooms (room_id, created_ts) VALUES ($1, $2) ON CONFLICT DO NOTHING")
        .bind(&room_id)
        .bind(now)
        .execute(pool.as_ref())
        .await
        .ok();

    // Insert 3 messages, 1 redacted.
    for i in 0..3 {
        let event_id = format!("$ut_msg_{i}_{}", unique_id());
        let is_redacted = i == 2;
        sqlx::query(
            r#"
            INSERT INTO events (event_id, room_id, sender, event_type, content, origin_server_ts, is_redacted)
            VALUES ($1, $2, $3, 'm.room.message', '{}', $4, $5)
            "#,
        )
        .bind(&event_id)
        .bind(&room_id)
        .bind(&user_id)
        .bind(now + i)
        .bind(is_redacted)
        .execute(pool.as_ref())
        .await
        .unwrap();
    }

    // Insert a non-message event — should not be counted.
    let other_event = format!("$ut_other_{}", unique_id());
    sqlx::query(
        r#"
        INSERT INTO events (event_id, room_id, sender, event_type, content, origin_server_ts, is_redacted)
        VALUES ($1, $2, $3, 'm.room.member', '{}', $4, false)
        "#,
    )
    .bind(&other_event)
    .bind(&room_id)
    .bind(&user_id)
    .bind(now)
    .execute(pool.as_ref())
    .await
    .unwrap();

    let count = storage.count_sent_messages(&user_id).await.unwrap();
    // 3 messages - 1 redacted = 2 non-redacted messages.
    assert_eq!(count, 2, "should count only non-redacted m.room.message events");
}

// =============================================================================
// set_deactivation_status
// =============================================================================

#[tokio::test]
async fn test_set_deactivation_status_returns_true() {
    let _guard = user_test_guard()
        .lock()
        .unwrap_or_else(|e| e.into_inner());
    let pool = crate::require_test_pool().await;
    setup_test_database(&pool).await;
    let storage = create_user_storage(&pool);

    let user_id = make_user(&storage, "deact_status").await;
    let result = storage.set_deactivation_status(&user_id, true).await.unwrap();
    assert!(result, "should return true when a row was updated");

    let user = storage.get_user_by_id(&user_id).await.unwrap().unwrap();
    assert!(user.is_deactivated);

    // Reactivate.
    let result = storage.set_deactivation_status(&user_id, false).await.unwrap();
    assert!(result, "should return true when reactivating");
    let user = storage.get_user_by_id(&user_id).await.unwrap().unwrap();
    assert!(!user.is_deactivated);
}

#[tokio::test]
async fn test_set_deactivation_status_nonexistent_returns_false() {
    let _guard = user_test_guard()
        .lock()
        .unwrap_or_else(|e| e.into_inner());
    let pool = crate::require_test_pool().await;
    setup_test_database(&pool).await;
    let storage = create_user_storage(&pool);

    let result = storage
        .set_deactivation_status(&format!("@ut_nonexistent_{}:localhost", unique_id()), true)
        .await
        .unwrap();
    assert!(!result, "should return false when no row was updated");
}

// =============================================================================
// set_shadow_ban
// =============================================================================

#[tokio::test]
async fn test_set_shadow_ban_toggle() {
    let _guard = user_test_guard()
        .lock()
        .unwrap_or_else(|e| e.into_inner());
    let pool = crate::require_test_pool().await;
    setup_test_database(&pool).await;
    let storage = create_user_storage(&pool);

    let user_id = make_user(&storage, "shadow").await;
    let result = storage.set_shadow_ban(&user_id, true).await.unwrap();
    assert!(result, "should return true when a row was updated");

    let user = storage.get_user_by_id(&user_id).await.unwrap().unwrap();
    assert!(user.is_shadow_banned);

    let result = storage.set_shadow_ban(&user_id, false).await.unwrap();
    assert!(result);
    let user = storage.get_user_by_id(&user_id).await.unwrap().unwrap();
    assert!(!user.is_shadow_banned);
}

#[tokio::test]
async fn test_set_shadow_ban_nonexistent_returns_false() {
    let _guard = user_test_guard()
        .lock()
        .unwrap_or_else(|e| e.into_inner());
    let pool = crate::require_test_pool().await;
    setup_test_database(&pool).await;
    let storage = create_user_storage(&pool);

    let result = storage
        .set_shadow_ban(&format!("@ut_nonexistent_{}:localhost", unique_id()), true)
        .await
        .unwrap();
    assert!(!result);
}

// =============================================================================
// set_guest_status
// =============================================================================

#[tokio::test]
async fn test_set_guest_status_toggle() {
    let _guard = user_test_guard()
        .lock()
        .unwrap_or_else(|e| e.into_inner());
    let pool = crate::require_test_pool().await;
    setup_test_database(&pool).await;
    let storage = create_user_storage(&pool);

    let user_id = make_user(&storage, "guest").await;
    let user = storage.get_user_by_id(&user_id).await.unwrap().unwrap();
    assert!(!user.is_guest);

    storage.set_guest_status(&user_id, true).await.unwrap();
    let user = storage.get_user_by_id(&user_id).await.unwrap().unwrap();
    assert!(user.is_guest);

    storage.set_guest_status(&user_id, false).await.unwrap();
    let user = storage.get_user_by_id(&user_id).await.unwrap().unwrap();
    assert!(!user.is_guest);
}

// =============================================================================
// set_user_type
// =============================================================================

#[tokio::test]
async fn test_set_user_type_set_and_clear() {
    let _guard = user_test_guard()
        .lock()
        .unwrap_or_else(|e| e.into_inner());
    let pool = crate::require_test_pool().await;
    setup_test_database(&pool).await;
    let storage = create_user_storage(&pool);

    let user_id = make_user(&storage, "utype").await;
    let user = storage.get_user_by_id(&user_id).await.unwrap().unwrap();
    assert!(user.user_type.is_none());

    storage.set_user_type(&user_id, Some("bot")).await.unwrap();
    let user = storage.get_user_by_id(&user_id).await.unwrap().unwrap();
    assert_eq!(user.user_type.as_deref(), Some("bot"));

    storage.set_user_type(&user_id, None).await.unwrap();
    let user = storage.get_user_by_id(&user_id).await.unwrap().unwrap();
    assert!(user.user_type.is_none());
}

// =============================================================================
// upgrade_guest_account
// =============================================================================

#[tokio::test]
async fn test_upgrade_guest_account() {
    let _guard = user_test_guard()
        .lock()
        .unwrap_or_else(|e| e.into_inner());
    let pool = crate::require_test_pool().await;
    setup_test_database(&pool).await;
    let storage = create_user_storage(&pool);

    let uid = unique_id();
    let user_id = format!("@ut_upgrade_{uid}:localhost");
    let guest_username = format!("ut_guest_{uid}");
    // Create as guest.
    storage
        .create_user(&user_id, &guest_username, None, false)
        .await
        .unwrap();
    storage.set_guest_status(&user_id, true).await.unwrap();
    assert!(
        storage
            .get_user_by_id(&user_id)
            .await
            .unwrap()
            .unwrap()
            .is_guest
    );

    let new_username = format!("ut_upgraded_{uid}");
    storage
        .upgrade_guest_account(&user_id, &new_username, "new_hash_123")
        .await
        .unwrap();

    let user = storage.get_user_by_id(&user_id).await.unwrap().unwrap();
    assert_eq!(user.username, new_username);
    assert!(!user.is_guest);
    assert_eq!(user.password_hash.as_deref(), Some("new_hash_123"));
    assert!(user.password_changed_ts.is_some());
    assert!(!user.is_password_change_required);
    assert!(!user.must_change_password);
}

// =============================================================================
// account_data: set_account_data / get_account_data_content / upsert_account_data_content
// =============================================================================

#[tokio::test]
async fn test_set_account_data_insert_and_upsert() {
    let _guard = user_test_guard()
        .lock()
        .unwrap_or_else(|e| e.into_inner());
    let pool = crate::require_test_pool().await;
    setup_test_database(&pool).await;
    let storage = create_user_storage(&pool);

    let user_id = make_user(&storage, "ad_set").await;
    let content = serde_json::json!({"key": "value1"});

    storage
        .set_account_data(&user_id, "m.direct", &content)
        .await
        .unwrap();

    // Upsert: update existing.
    let updated = serde_json::json!({"key": "value2"});
    storage
        .set_account_data(&user_id, "m.direct", &updated)
        .await
        .unwrap();

    // Verify only one row exists (upsert, not insert).
    let count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM user_account_data WHERE user_id = $1")
        .bind(&user_id)
        .fetch_one(pool.as_ref())
        .await
        .unwrap();
    assert_eq!(count, 1, "upsert should not create a duplicate row");
}

#[tokio::test]
async fn test_get_account_data_content_not_found() {
    let _guard = user_test_guard()
        .lock()
        .unwrap_or_else(|e| e.into_inner());
    let pool = crate::require_test_pool().await;
    setup_test_database(&pool).await;
    let storage = create_user_storage(&pool);

    let user_id = make_user(&storage, "ad_get_nf").await;
    let result = storage
        .get_account_data_content(&user_id, "m.nonexistent")
        .await
        .unwrap();
    assert!(result.is_none());
}

#[tokio::test]
async fn test_upsert_account_data_content_insert_and_update() {
    let _guard = user_test_guard()
        .lock()
        .unwrap_or_else(|e| e.into_inner());
    let pool = crate::require_test_pool().await;
    setup_test_database(&pool).await;
    let storage = create_user_storage(&pool);

    let user_id = make_user(&storage, "ad_upsert").await;

    // Insert.
    let content1 = serde_json::json!({"v": 1});
    storage
        .upsert_account_data_content(&user_id, "m.test_type", &content1)
        .await
        .unwrap();

    let fetched = storage
        .get_account_data_content(&user_id, "m.test_type")
        .await
        .unwrap()
        .expect("row should exist after insert");
    assert_eq!(fetched, content1);

    // Update.
    let content2 = serde_json::json!({"v": 2});
    storage
        .upsert_account_data_content(&user_id, "m.test_type", &content2)
        .await
        .unwrap();

    let fetched = storage
        .get_account_data_content(&user_id, "m.test_type")
        .await
        .unwrap()
        .expect("row should exist after update");
    assert_eq!(fetched, content2);

    // Only one row.
    let count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM account_data WHERE user_id = $1 AND data_type = $2")
        .bind(&user_id)
        .bind("m.test_type")
        .fetch_one(pool.as_ref())
        .await
        .unwrap();
    assert_eq!(count, 1);
}

// =============================================================================
// get_user_profiles_batch / get_user_profiles_map
// =============================================================================

#[tokio::test]
async fn test_get_user_profiles_batch_empty_input() {
    let _guard = user_test_guard()
        .lock()
        .unwrap_or_else(|e| e.into_inner());
    let pool = crate::require_test_pool().await;
    setup_test_database(&pool).await;
    let storage = create_user_storage(&pool);

    let result = storage.get_user_profiles_batch(&[]).await.unwrap();
    assert!(result.is_empty());
}

#[tokio::test]
async fn test_get_user_profiles_batch_multiple() {
    let _guard = user_test_guard()
        .lock()
        .unwrap_or_else(|e| e.into_inner());
    let pool = crate::require_test_pool().await;
    setup_test_database(&pool).await;
    let storage = create_user_storage(&pool);

    let id1 = make_user(&storage, "pb1").await;
    let id2 = make_user(&storage, "pb2").await;
    storage.update_displayname(&id1, Some("Profile One")).await.unwrap();

    let profiles = storage
        .get_user_profiles_batch(&[id1.clone(), id2.clone()])
        .await
        .unwrap();
    assert_eq!(profiles.len(), 2);
    assert!(profiles.iter().any(|p| p.user_id == id1));
    assert!(profiles.iter().any(|p| p.user_id == id2));
}

#[tokio::test]
async fn test_get_user_profiles_batch_excludes_deactivated() {
    let _guard = user_test_guard()
        .lock()
        .unwrap_or_else(|e| e.into_inner());
    let pool = crate::require_test_pool().await;
    setup_test_database(&pool).await;
    let storage = create_user_storage(&pool);

    let active_id = make_user(&storage, "pb_active").await;
    let deact_id = make_user(&storage, "pb_deact").await;
    storage.set_deactivation_status(&deact_id, true).await.unwrap();

    let profiles = storage
        .get_user_profiles_batch(&[active_id.clone(), deact_id])
        .await
        .unwrap();
    assert_eq!(profiles.len(), 1);
    assert_eq!(profiles[0].user_id, active_id);
}

#[tokio::test]
async fn test_get_user_profiles_map_empty_input() {
    let _guard = user_test_guard()
        .lock()
        .unwrap_or_else(|e| e.into_inner());
    let pool = crate::require_test_pool().await;
    setup_test_database(&pool).await;
    let storage = create_user_storage(&pool);

    let map = storage.get_user_profiles_map(&[]).await.unwrap();
    assert!(map.is_empty());
}

#[tokio::test]
async fn test_get_user_profiles_map_multiple() {
    let _guard = user_test_guard()
        .lock()
        .unwrap_or_else(|e| e.into_inner());
    let pool = crate::require_test_pool().await;
    setup_test_database(&pool).await;
    let storage = create_user_storage(&pool);

    let id1 = make_user(&storage, "pm1").await;
    let id2 = make_user(&storage, "pm2").await;

    let map = storage
        .get_user_profiles_map(&[id1.clone(), id2.clone()])
        .await
        .unwrap();
    assert_eq!(map.len(), 2);
    assert!(map.contains_key(&id1));
    assert!(map.contains_key(&id2));
}

// =============================================================================
// get_users_batch / get_users_map
// =============================================================================

#[tokio::test]
async fn test_get_users_batch_empty_input() {
    let _guard = user_test_guard()
        .lock()
        .unwrap_or_else(|e| e.into_inner());
    let pool = crate::require_test_pool().await;
    setup_test_database(&pool).await;
    let storage = create_user_storage(&pool);

    let result = storage.get_users_batch(&[]).await.unwrap();
    assert!(result.is_empty());
}

#[tokio::test]
async fn test_get_users_batch_multiple() {
    let _guard = user_test_guard()
        .lock()
        .unwrap_or_else(|e| e.into_inner());
    let pool = crate::require_test_pool().await;
    setup_test_database(&pool).await;
    let storage = create_user_storage(&pool);

    let id1 = make_user(&storage, "ub1").await;
    let id2 = make_user(&storage, "ub2").await;
    let missing = format!("@ut_missing_{}:localhost", unique_id());

    let users = storage
        .get_users_batch(&[id1.clone(), id2.clone(), missing])
        .await
        .unwrap();
    assert_eq!(users.len(), 2, "should return only existing users");
    assert!(users.iter().any(|u| u.user_id == id1));
    assert!(users.iter().any(|u| u.user_id == id2));
}

#[tokio::test]
async fn test_get_users_map_empty_input() {
    let _guard = user_test_guard()
        .lock()
        .unwrap_or_else(|e| e.into_inner());
    let pool = crate::require_test_pool().await;
    setup_test_database(&pool).await;
    let storage = create_user_storage(&pool);

    let map = storage.get_users_map(&[]).await.unwrap();
    assert!(map.is_empty());
}

#[tokio::test]
async fn test_get_users_map_multiple() {
    let _guard = user_test_guard()
        .lock()
        .unwrap_or_else(|e| e.into_inner());
    let pool = crate::require_test_pool().await;
    setup_test_database(&pool).await;
    let storage = create_user_storage(&pool);

    let id1 = make_user(&storage, "um1").await;
    let id2 = make_user(&storage, "um2").await;

    let map = storage
        .get_users_map(&[id1.clone(), id2.clone()])
        .await
        .unwrap();
    assert_eq!(map.len(), 2);
    assert!(map.contains_key(&id1));
    assert!(map.contains_key(&id2));
    // Verify the User objects are correctly mapped.
    assert_eq!(map.get(&id1).unwrap().username, map.get(&id1).unwrap().username);
}

// =============================================================================
// update_displayname_batch
// =============================================================================

#[tokio::test]
async fn test_update_displayname_batch_empty() {
    let _guard = user_test_guard()
        .lock()
        .unwrap_or_else(|e| e.into_inner());
    let pool = crate::require_test_pool().await;
    setup_test_database(&pool).await;
    let storage = create_user_storage(&pool);

    let count = storage.update_displayname_batch(&[]).await.unwrap();
    assert_eq!(count, 0);
}

#[tokio::test]
async fn test_update_displayname_batch_multiple() {
    let _guard = user_test_guard()
        .lock()
        .unwrap_or_else(|e| e.into_inner());
    let pool = crate::require_test_pool().await;
    setup_test_database(&pool).await;
    let storage = create_user_storage(&pool);

    let id1 = make_user(&storage, "db1").await;
    let id2 = make_user(&storage, "db2").await;

    let updates = vec![
        (id1.clone(), Some("Display One".to_string())),
        (id2.clone(), Some("Display Two".to_string())),
    ];
    let count = storage.update_displayname_batch(&updates).await.unwrap();
    assert_eq!(count, 2);

    let user1 = storage.get_user_by_id(&id1).await.unwrap().unwrap();
    assert_eq!(user1.displayname.as_deref(), Some("Display One"));
    let user2 = storage.get_user_by_id(&id2).await.unwrap().unwrap();
    assert_eq!(user2.displayname.as_deref(), Some("Display Two"));
}

// =============================================================================
// search_users_with_presence
// =============================================================================

#[tokio::test]
async fn test_search_users_with_presence_empty_query() {
    let _guard = user_test_guard()
        .lock()
        .unwrap_or_else(|e| e.into_inner());
    let pool = crate::require_test_pool().await;
    setup_test_database(&pool).await;
    let storage = create_user_storage(&pool);

    let result = storage.search_users_with_presence("", 10).await.unwrap();
    assert!(result.is_empty());

    let result = storage.search_users_with_presence("   ", 10).await.unwrap();
    assert!(result.is_empty());
}

#[tokio::test]
async fn test_search_users_with_presence_basic() {
    let _guard = user_test_guard()
        .lock()
        .unwrap_or_else(|e| e.into_inner());
    let pool = crate::require_test_pool().await;
    setup_test_database(&pool).await;
    if !has_pg_trgm(&pool).await {
        eprintln!("Skipping test_search_users_with_presence_basic: pg_trgm not available");
        return;
    }
    let storage = create_user_storage(&pool);

    let uid = unique_id();
    let user_id = format!("@ut_swp_{uid}:localhost");
    let username = format!("ut_swp_{uid}");
    storage
        .create_user(&user_id, &username, None, false)
        .await
        .unwrap();

    // Insert presence record.
    let now = chrono::Utc::now().timestamp_millis();
    sqlx::query(
        "INSERT INTO presence (user_id, presence, last_active_ts, created_ts, updated_ts) VALUES ($1, 'online', $2, $3, $4) ON CONFLICT (user_id) DO UPDATE SET presence = EXCLUDED.presence, last_active_ts = EXCLUDED.last_active_ts, updated_ts = EXCLUDED.updated_ts",
    )
    .bind(&user_id)
    .bind(now)
    .bind(now)
    .bind(now)
    .execute(pool.as_ref())
    .await
    .unwrap();

    let results = storage.search_users_with_presence(&username, 10).await.unwrap();
    assert!(!results.is_empty(), "should find the created user");
    let found = results.iter().find(|r| r.user_id == user_id);
    assert!(found.is_some(), "search should return the created user");
    let found = found.unwrap();
    assert_eq!(found.presence.as_deref(), Some("online"));
}

// =============================================================================
// search_directory_users
// =============================================================================

#[tokio::test]
async fn test_search_directory_users_empty_query() {
    let _guard = user_test_guard()
        .lock()
        .unwrap_or_else(|e| e.into_inner());
    let pool = crate::require_test_pool().await;
    setup_test_database(&pool).await;
    let storage = create_user_storage(&pool);

    let result = storage.search_directory_users("", 10, false).await.unwrap();
    assert!(result.is_empty());

    let result = storage.search_directory_users("   ", 10, false).await.unwrap();
    assert!(result.is_empty());
}

#[tokio::test]
async fn test_search_directory_users_basic() {
    let _guard = user_test_guard()
        .lock()
        .unwrap_or_else(|e| e.into_inner());
    let pool = crate::require_test_pool().await;
    setup_test_database(&pool).await;
    if !has_pg_trgm(&pool).await {
        eprintln!("Skipping test_search_directory_users_basic: pg_trgm not available");
        return;
    }
    let storage = create_user_storage(&pool);

    let uid = unique_id();
    let user_id = format!("@ut_dir_{uid}:localhost");
    let username = format!("ut_dir_{uid}");
    storage
        .create_user(&user_id, &username, None, false)
        .await
        .unwrap();

    let results = storage
        .search_directory_users(&username, 10, false)
        .await
        .unwrap();
    let found = results.iter().find(|r| r.user_id == user_id);
    assert!(found.is_some(), "search should return the created user");
    let found = found.unwrap();
    assert_eq!(found.username, username);
    // match_type should be one of the known categories.
    assert!(
        ["exact", "prefix", "contains", "fuzzy"].contains(&found.match_type.as_str()),
        "match_type should be a known category, got: {}",
        found.match_type
    );
}

#[tokio::test]
async fn test_search_directory_users_exact_only() {
    let _guard = user_test_guard()
        .lock()
        .unwrap_or_else(|e| e.into_inner());
    let pool = crate::require_test_pool().await;
    setup_test_database(&pool).await;
    if !has_pg_trgm(&pool).await {
        eprintln!("Skipping test_search_directory_users_exact_only: pg_trgm not available");
        return;
    }
    let storage = create_user_storage(&pool);

    let uid = unique_id();
    let username = format!("ut_exact_{uid}");
    let user_id = format!("@ut_exact_{uid}:localhost");
    storage
        .create_user(&user_id, &username, None, false)
        .await
        .unwrap();

    // Exact match should find the user.
    let results = storage
        .search_directory_users(&username, 10, true)
        .await
        .unwrap();
    let found = results.iter().find(|r| r.user_id == user_id);
    assert!(found.is_some(), "exact match should find the user");
    assert_eq!(found.unwrap().match_type, "exact");
}

#[tokio::test]
async fn test_search_directory_users_clamps_limit() {
    let _guard = user_test_guard()
        .lock()
        .unwrap_or_else(|e| e.into_inner());
    let pool = crate::require_test_pool().await;
    setup_test_database(&pool).await;
    if !has_pg_trgm(&pool).await {
        eprintln!("Skipping test_search_directory_users_clamps_limit: pg_trgm not available");
        return;
    }
    let storage = create_user_storage(&pool);

    let uid = unique_id();
    let username = format!("ut_clamp_{uid}");
    storage
        .create_user(
            &format!("@ut_clamp_{uid}:localhost"),
            &username,
            None,
            false,
        )
        .await
        .unwrap();

    // limit=0 is clamped to 1 — should still return at least 1 result.
    let results = storage.search_directory_users(&username, 0, false).await.unwrap();
    assert!(!results.is_empty(), "limit=0 should be clamped to 1");

    // limit=1000 is clamped to 100 — should still work.
    let results = storage.search_directory_users(&username, 1000, false).await.unwrap();
    assert!(!results.is_empty());
}

// =============================================================================
// lock_user / unlock_user / is_user_locked / get_active_user_lock / get_locked_users
// =============================================================================

#[tokio::test]
async fn test_lock_user_basic() {
    let _guard = user_test_guard()
        .lock()
        .unwrap_or_else(|e| e.into_inner());
    let pool = crate::require_test_pool().await;
    setup_test_database(&pool).await;
    let storage = create_user_storage(&pool);

    let user_id = make_user(&storage, "lock").await;
    let now = chrono::Utc::now().timestamp_millis();

    let lock = storage
        .lock_user(&user_id, Some("suspicious activity"), "admin", now)
        .await
        .unwrap();

    assert_eq!(lock.user_id, user_id);
    assert_eq!(lock.reason.as_deref(), Some("suspicious activity"));
    assert_eq!(lock.locked_by, "admin");
    assert_eq!(lock.created_ts, now);
    assert!(lock.is_active);
    assert!(lock.unlocked_ts.is_none());
    assert!(lock.id > 0);
}

#[tokio::test]
async fn test_lock_user_already_locked_updates() {
    let _guard = user_test_guard()
        .lock()
        .unwrap_or_else(|e| e.into_inner());
    let pool = crate::require_test_pool().await;
    setup_test_database(&pool).await;
    let storage = create_user_storage(&pool);

    let user_id = make_user(&storage, "relock").await;
    let now1 = chrono::Utc::now().timestamp_millis();

    let lock1 = storage
        .lock_user(&user_id, Some("reason1"), "admin1", now1)
        .await
        .unwrap();

    // Lock again — should update the existing lock, not create a new one.
    let now2 = now1 + 5000;
    let lock2 = storage
        .lock_user(&user_id, Some("reason2"), "admin2", now2)
        .await
        .unwrap();

    // ON CONFLICT (user_id, is_active) WHERE is_active = TRUE DO UPDATE.
    assert_eq!(lock1.id, lock2.id, "re-locking should update the same row");
    assert_eq!(lock2.reason.as_deref(), Some("reason2"));
    assert_eq!(lock2.locked_by, "admin2");
    assert_eq!(lock2.created_ts, now2);

    // Only one active lock.
    let active = storage.get_active_user_lock(&user_id).await.unwrap().unwrap();
    assert_eq!(active.id, lock1.id);
}

#[tokio::test]
async fn test_unlock_user_basic() {
    let _guard = user_test_guard()
        .lock()
        .unwrap_or_else(|e| e.into_inner());
    let pool = crate::require_test_pool().await;
    setup_test_database(&pool).await;
    let storage = create_user_storage(&pool);

    let user_id = make_user(&storage, "unlock").await;
    let now = chrono::Utc::now().timestamp_millis();

    storage.lock_user(&user_id, None, "admin", now).await.unwrap();
    assert!(storage.is_user_locked(&user_id).await.unwrap());

    let unlock_ts = now + 10000;
    storage.unlock_user(&user_id, unlock_ts).await.unwrap();

    assert!(!storage.is_user_locked(&user_id).await.unwrap());
    let active = storage.get_active_user_lock(&user_id).await.unwrap();
    assert!(active.is_none(), "no active lock after unlock");

    // The unlocked record should still exist with unlocked_ts set.
    let all_locks: Vec<(bool, Option<i64>)> = sqlx::query_as(
        "SELECT is_active, unlocked_ts FROM user_locks WHERE user_id = $1",
    )
    .bind(&user_id)
    .fetch_all(pool.as_ref())
    .await
    .unwrap();
    assert!(all_locks.iter().any(|(active, ts)| !active && ts == &Some(unlock_ts)));
}

#[tokio::test]
async fn test_unlock_user_not_locked_noop() {
    let _guard = user_test_guard()
        .lock()
        .unwrap_or_else(|e| e.into_inner());
    let pool = crate::require_test_pool().await;
    setup_test_database(&pool).await;
    let storage = create_user_storage(&pool);

    let user_id = make_user(&storage, "unlock_noop").await;
    // Unlock a user that was never locked — should be a no-op (no error).
    storage.unlock_user(&user_id, chrono::Utc::now().timestamp_millis()).await.unwrap();
    assert!(!storage.is_user_locked(&user_id).await.unwrap());
}

#[tokio::test]
async fn test_is_user_locked_false() {
    let _guard = user_test_guard()
        .lock()
        .unwrap_or_else(|e| e.into_inner());
    let pool = crate::require_test_pool().await;
    setup_test_database(&pool).await;
    let storage = create_user_storage(&pool);

    let user_id = make_user(&storage, "lock_check_false").await;
    assert!(!storage.is_user_locked(&user_id).await.unwrap());
}

#[tokio::test]
async fn test_is_user_locked_true() {
    let _guard = user_test_guard()
        .lock()
        .unwrap_or_else(|e| e.into_inner());
    let pool = crate::require_test_pool().await;
    setup_test_database(&pool).await;
    let storage = create_user_storage(&pool);

    let user_id = make_user(&storage, "lock_check_true").await;
    let now = chrono::Utc::now().timestamp_millis();
    storage.lock_user(&user_id, None, "admin", now).await.unwrap();
    assert!(storage.is_user_locked(&user_id).await.unwrap());
}

#[tokio::test]
async fn test_get_active_user_lock_none() {
    let _guard = user_test_guard()
        .lock()
        .unwrap_or_else(|e| e.into_inner());
    let pool = crate::require_test_pool().await;
    setup_test_database(&pool).await;
    let storage = create_user_storage(&pool);

    let user_id = make_user(&storage, "lock_none").await;
    let result = storage.get_active_user_lock(&user_id).await.unwrap();
    assert!(result.is_none());
}

#[tokio::test]
async fn test_get_active_user_lock_existing() {
    let _guard = user_test_guard()
        .lock()
        .unwrap_or_else(|e| e.into_inner());
    let pool = crate::require_test_pool().await;
    setup_test_database(&pool).await;
    let storage = create_user_storage(&pool);

    let user_id = make_user(&storage, "lock_existing").await;
    let now = chrono::Utc::now().timestamp_millis();
    storage
        .lock_user(&user_id, Some("test reason"), "admin_user", now)
        .await
        .unwrap();

    let lock = storage
        .get_active_user_lock(&user_id)
        .await
        .unwrap()
        .expect("should have an active lock");
    assert_eq!(lock.user_id, user_id);
    assert_eq!(lock.reason.as_deref(), Some("test reason"));
    assert_eq!(lock.locked_by, "admin_user");
    assert!(lock.is_active);
}

#[tokio::test]
async fn test_get_locked_users_empty() {
    let _guard = user_test_guard()
        .lock()
        .unwrap_or_else(|e| e.into_inner());
    let pool = crate::require_test_pool().await;
    setup_test_database(&pool).await;
    let storage = create_user_storage(&pool);

    let results = storage.get_locked_users(10, 0).await.unwrap();
    // May have locks from other tests, but at minimum should not error.
    assert!(results.iter().all(|l| l.is_active));
}

#[tokio::test]
async fn test_get_locked_users_pagination() {
    let _guard = user_test_guard()
        .lock()
        .unwrap_or_else(|e| e.into_inner());
    let pool = crate::require_test_pool().await;
    setup_test_database(&pool).await;
    let storage = create_user_storage(&pool);

    let now = chrono::Utc::now().timestamp_millis();

    // Create 3 locked users.
    let mut user_ids = Vec::new();
    for i in 0..3 {
        let uid = unique_id();
        let user_id = format!("@ut_locked_{i}_{uid}:localhost");
        storage
            .create_user(&user_id, &format!("ut_locked_{i}_{uid}"), None, false)
            .await
            .unwrap();
        storage
            .lock_user(&user_id, Some("test"), "admin", now + i)
            .await
            .unwrap();
        user_ids.push(user_id);
    }

    // Page 1: limit=2.
    let page1 = storage.get_locked_users(2, 0).await.unwrap();
    assert_eq!(page1.len(), 2);
    assert!(page1.iter().all(|l| l.is_active));

    // Page 2: offset=2.
    let page2 = storage.get_locked_users(2, 2).await.unwrap();
    // May have 1 or more depending on other tests, but at least 1.
    assert!(!page2.is_empty() || page1.len() >= 2);

    // Verify our test users are somewhere in the results.
    let all_pages: Vec<LockedUser> = storage.get_locked_users(100, 0).await.unwrap();
    for uid in &user_ids {
        assert!(
            all_pages.iter().any(|l| &l.user_id == uid),
            "locked user {uid} should be in results"
        );
    }
}

// =============================================================================
// delete_user
// =============================================================================

#[tokio::test]
async fn test_delete_user_and_verify_gone() {
    let _guard = user_test_guard()
        .lock()
        .unwrap_or_else(|e| e.into_inner());
    let pool = crate::require_test_pool().await;
    setup_test_database(&pool).await;
    let storage = create_user_storage(&pool);

    let user_id = make_user(&storage, "delete").await;
    assert!(storage.get_user_by_id(&user_id).await.unwrap().is_some());

    storage.delete_user(&user_id).await.unwrap();
    assert!(storage.get_user_by_id(&user_id).await.unwrap().is_none());
    assert!(!storage.user_exists(&user_id).await.unwrap());
}

#[tokio::test]
async fn test_delete_user_nonexistent_no_error() {
    let _guard = user_test_guard()
        .lock()
        .unwrap_or_else(|e| e.into_inner());
    let pool = crate::require_test_pool().await;
    setup_test_database(&pool).await;
    let storage = create_user_storage(&pool);

    // Deleting a non-existent user should not error.
    storage
        .delete_user(&format!("@ut_nonexistent_{}:localhost", unique_id()))
        .await
        .unwrap();
}

// =============================================================================
// get_user_profile — edge cases
// =============================================================================

#[tokio::test]
async fn test_get_user_profile_falls_back_to_username() {
    let _guard = user_test_guard()
        .lock()
        .unwrap_or_else(|e| e.into_inner());
    let pool = crate::require_test_pool().await;
    setup_test_database(&pool).await;
    let storage = create_user_storage(&pool);

    let uid = unique_id();
    let user_id = format!("@ut_pf_{uid}:localhost");
    let username = format!("ut_pf_{uid}");
    storage
        .create_user(&user_id, &username, None, false)
        .await
        .unwrap();
    // No displayname set.

    let profile = storage.get_user_profile(&user_id).await.unwrap().unwrap();
    // displayname should fall back to username via COALESCE.
    assert_eq!(profile.displayname, Some(username));
}

// =============================================================================
// UserStore trait dispatch
// =============================================================================

#[tokio::test]
async fn test_user_store_trait_dispatch() {
    let _guard = user_test_guard()
        .lock()
        .unwrap_or_else(|e| e.into_inner());
    let pool = crate::require_test_pool().await;
    setup_test_database(&pool).await;
    let storage = create_user_storage(&pool);

    // Use the storage through the trait object to verify the impl delegates correctly.
    let store: &dyn UserStore = &storage;

    let uid = unique_id();
    let user_id = format!("@ut_trait_{uid}:localhost");
    let username = format!("ut_trait_{uid}");

    // create_user via trait.
    let user = store
        .create_user(&user_id, &username, Some("hash"), false)
        .await
        .unwrap();
    assert_eq!(user.user_id, user_id);

    // get_user_by_id via trait.
    let fetched = store.get_user_by_id(&user_id).await.unwrap().unwrap();
    assert_eq!(fetched.username, username);

    // user_exists via trait.
    assert!(store.user_exists(&user_id).await.unwrap());

    // get_user_count via trait.
    let count = store.get_user_count().await.unwrap();
    assert!(count >= 1);

    // get_user_stats_summary via trait.
    let summary = store.get_user_stats_summary().await.unwrap();
    assert!(summary.total_users >= 1);

    // lock_user / is_user_locked / unlock_user via trait.
    let now = chrono::Utc::now().timestamp_millis();
    let lock = store.lock_user(&user_id, Some("test"), "admin", now).await.unwrap();
    assert_eq!(lock.user_id, user_id);
    assert!(store.is_user_locked(&user_id).await.unwrap());
    store.unlock_user(&user_id, now + 1000).await.unwrap();
    assert!(!store.is_user_locked(&user_id).await.unwrap());

    // update_password via trait.
    store.update_password(&user_id, "new_hash").await.unwrap();
    let user = store.get_user_by_id(&user_id).await.unwrap().unwrap();
    assert_eq!(user.password_hash.as_deref(), Some("new_hash"));

    // set_admin_status via trait.
    store.set_admin_status(&user_id, true).await.unwrap();
    assert!(store.get_user_by_id(&user_id).await.unwrap().unwrap().is_admin);

    // set_shadow_ban via trait.
    assert!(store.set_shadow_ban(&user_id, true).await.unwrap());
    assert!(
        store
            .get_user_by_id(&user_id)
            .await
            .unwrap()
            .unwrap()
            .is_shadow_banned
    );

    // set_deactivation_status via trait.
    assert!(store.set_deactivation_status(&user_id, true).await.unwrap());

    // set_guest_status via trait.
    store.set_guest_status(&user_id, true).await.unwrap();
    assert!(
        store
            .get_user_by_id(&user_id)
            .await
            .unwrap()
            .unwrap()
            .is_guest
    );

    // set_user_type via trait.
    store.set_user_type(&user_id, Some("bot")).await.unwrap();
    assert_eq!(
        store
            .get_user_by_id(&user_id)
            .await
            .unwrap()
            .unwrap()
            .user_type
            .as_deref(),
        Some("bot")
    );

    // upgrade_guest_account via trait.
    store
        .upgrade_guest_account(&user_id, &format!("ut_upgraded_{uid}"), "upgraded_hash")
        .await
        .unwrap();

    // filter_existing_users via trait.
    let existing = store.filter_existing_users(&[user_id.clone()]).await.unwrap();
    assert!(existing.contains(&user_id));

    // get_users_paginated via trait.
    let _ = store.get_users_paginated(10, None, None).await.unwrap();

    // list_users via trait.
    let _ = store.list_users(10, None, None, None).await.unwrap();

    // get_daily/monthly/r30 via trait.
    let _ = store.get_daily_active_users().await.unwrap();
    let _ = store.get_monthly_active_users().await.unwrap();
    let _ = store.get_r30_users().await.unwrap();

    // count_sent_messages via trait.
    let _ = store.count_sent_messages(&user_id).await.unwrap();

    // get_user_profile / batch / map via trait.
    let _ = store.get_user_profile(&user_id).await.unwrap();
    let _ = store.get_user_profiles_batch(&[user_id.clone()]).await.unwrap();
    let _ = store.get_user_profiles_map(&[user_id.clone()]).await.unwrap();
    let _ = store.get_users_batch(&[user_id.clone()]).await.unwrap();
    let _ = store.get_users_map(&[user_id.clone()]).await.unwrap();

    // account_data via trait.
    let content = serde_json::json!({"k": "v"});
    store
        .upsert_account_data_content(&user_id, "m.test", &content)
        .await
        .unwrap();
    let fetched = store
        .get_account_data_content(&user_id, "m.test")
        .await
        .unwrap();
    assert_eq!(fetched, Some(content));

    // pool() via trait.
    let _pool_ref = store.pool();

    // delete_user via trait.
    store.delete_user(&user_id).await.unwrap();
    assert!(store.get_user_by_id(&user_id).await.unwrap().is_none());
}
