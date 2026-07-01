//! Integration tests for `AdminUserService` at `synapse-services/src/admin_user_service.rs`.
//!
//! Covers all public methods of `AdminUserService` (21 methods) plus the
//! free cursor functions and the `From<&User> for AdminUserProfile` impl.
//!
//! Uses the warm_up_pool + Mutex guard + unique_id pattern for cross-runtime
//! isolation. Test data is prefixed with `aus_` and cleaned up after each run
//! to avoid disturbing other integration tests running in parallel.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
#![allow(clippy::await_holding_lock)]

use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex, OnceLock};

use synapse_rust::cache::{CacheConfig, CacheManager};
use synapse_services::admin_user_service::{
    decode_user_cursor, encode_user_cursor, AdminUserCursor, AdminUserService,
};
use synapse_storage::device::DeviceStorage;
use synapse_storage::membership::RoomMemberStorage;
use synapse_storage::room::RoomStorage;
use synapse_storage::RoomMemberRepository;
use synapse_storage::user::{UserStorage, UserStore};

static TEST_COUNTER: AtomicU64 = AtomicU64::new(1);

fn unique_id() -> u64 {
    TEST_COUNTER.fetch_add(1, Ordering::SeqCst)
}

fn admin_user_test_guard() -> &'static Mutex<()> {
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

/// Create all tables needed by admin_user_service.rs tests with CREATE TABLE IF
/// NOT EXISTS, then clean up leftover `aus_`-prefixed data from previous runs.
async fn setup_test_database(pool: &Arc<sqlx::PgPool>) {
    warm_up_pool(pool).await;

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

    sqlx::query(
        r#"
        CREATE TABLE IF NOT EXISTS room_memberships (
            id BIGSERIAL PRIMARY KEY,
            room_id TEXT NOT NULL,
            user_id TEXT NOT NULL,
            membership TEXT NOT NULL,
            joined_ts BIGINT,
            invited_ts BIGINT,
            left_ts BIGINT,
            banned_ts BIGINT,
            sender TEXT,
            reason TEXT,
            event_id TEXT,
            event_type TEXT,
            display_name TEXT,
            avatar_url TEXT,
            is_banned BOOLEAN DEFAULT FALSE,
            invite_token TEXT,
            updated_ts BIGINT,
            join_reason TEXT,
            banned_by TEXT,
            ban_reason TEXT,
            CONSTRAINT uq_room_memberships_room_user UNIQUE (room_id, user_id)
        )
        "#,
    )
    .execute(pool.as_ref())
    .await
    .ok();

    sqlx::query(
        r#"
        CREATE TABLE IF NOT EXISTS room_summaries (
            id BIGSERIAL PRIMARY KEY,
            room_id TEXT NOT NULL UNIQUE,
            room_type TEXT,
            name TEXT,
            topic TEXT,
            avatar_url TEXT,
            canonical_alias TEXT,
            join_rules TEXT NOT NULL DEFAULT 'invite',
            history_visibility TEXT NOT NULL DEFAULT 'shared',
            guest_access TEXT NOT NULL DEFAULT 'forbidden',
            is_direct BOOLEAN NOT NULL DEFAULT FALSE,
            is_space BOOLEAN NOT NULL DEFAULT FALSE,
            is_encrypted BOOLEAN NOT NULL DEFAULT FALSE,
            member_count BIGINT DEFAULT 0,
            joined_member_count BIGINT DEFAULT 0,
            invited_member_count BIGINT DEFAULT 0,
            hero_users JSONB NOT NULL DEFAULT '[]',
            last_event_id TEXT,
            last_event_ts BIGINT,
            last_message_ts BIGINT,
            unread_notifications BIGINT NOT NULL DEFAULT 0
        )
        "#,
    )
    .execute(pool.as_ref())
    .await
    .ok();

    sqlx::query("CREATE SEQUENCE IF NOT EXISTS events_stream_ordering_seq")
        .execute(pool.as_ref())
        .await
        .ok();

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
            prevEvents JSONB,
            authEvents JSONB,
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

    // -- Clean up test data from previous runs (FK-safe order) --
    let cleanup = |table: &str, col: &str| format!("DELETE FROM {table} WHERE {col} LIKE '%aus_%'");
    for stmt in [
        cleanup("events", "sender"),
        cleanup("devices", "user_id"),
        cleanup("room_memberships", "user_id"),
        cleanup("room_summaries", "room_id"),
        cleanup("rooms", "room_id"),
        cleanup("users", "user_id"),
    ] {
        sqlx::query(&stmt).execute(pool.as_ref()).await.ok();
    }
}

const SERVER_NAME: &str = "localhost";

/// Build an `AdminUserService` wired to real storages backed by `pool`.
fn make_service(pool: &Arc<sqlx::PgPool>) -> AdminUserService {
    let cache = Arc::new(CacheManager::new(&CacheConfig::default()));
    let user_storage: Arc<dyn UserStore> = Arc::new(UserStorage::new(pool, cache));
    let device_storage = DeviceStorage::new(pool);
    let room_storage = RoomStorage::new(pool);
    let member_storage: Arc<dyn RoomMemberRepository> =
        Arc::new(RoomMemberStorage::new(pool, SERVER_NAME));
    AdminUserService::new(
        pool.clone(),
        user_storage,
        device_storage,
        room_storage,
        member_storage,
        SERVER_NAME.to_string(),
    )
}

/// Directly insert a user row and return its user_id.
async fn insert_user(pool: &Arc<sqlx::PgPool>, suffix: &str) -> String {
    let id = unique_id();
    let user_id = format!("@aus_{suffix}_{id}:{SERVER_NAME}");
    let username = format!("aus_{suffix}_{id}");
    let now = chrono::Utc::now().timestamp_millis();
    sqlx::query(
        r#"INSERT INTO users (user_id, username, password_hash, is_admin, is_guest, is_shadow_banned,
           is_deactivated, created_ts, generation)
           VALUES ($1, $2, 'hash', FALSE, FALSE, FALSE, FALSE, $3, $3)"#,
    )
    .bind(&user_id)
    .bind(&username)
    .bind(now)
    .execute(pool.as_ref())
    .await
    .unwrap();
    user_id
}

/// Directly insert a room row.
async fn insert_room(pool: &Arc<sqlx::PgPool>, room_id: &str) {
    let now = chrono::Utc::now().timestamp_millis();
    sqlx::query(
        r#"INSERT INTO rooms (room_id, creator, created_ts) VALUES ($1, 'aus_creator', $2)
           ON CONFLICT (room_id) DO NOTHING"#,
    )
    .bind(room_id)
    .bind(now)
    .execute(pool.as_ref())
    .await
    .ok();
}

/// Directly insert a room_memberships row.
async fn insert_membership(pool: &Arc<sqlx::PgPool>, room_id: &str, user_id: &str, membership: &str) {
    let now = chrono::Utc::now().timestamp_millis();
    let joined_ts = if membership == "join" { Some(now) } else { None };
    sqlx::query(
        r#"INSERT INTO room_memberships (room_id, user_id, membership, joined_ts)
           VALUES ($1, $2, $3, $4)
           ON CONFLICT (room_id, user_id) DO UPDATE SET membership = EXCLUDED.membership,
              joined_ts = EXCLUDED.joined_ts"#,
    )
    .bind(room_id)
    .bind(user_id)
    .bind(membership)
    .bind(joined_ts)
    .execute(pool.as_ref())
    .await
    .unwrap();
}

/// Directly insert a room_summaries row.
async fn insert_room_summary(pool: &Arc<sqlx::PgPool>, room_id: &str, member_count: i64) {
    let now = chrono::Utc::now().timestamp_millis();
    sqlx::query(
        r#"INSERT INTO room_summaries (room_id, member_count, joined_member_count, updated_ts)
           VALUES ($1, $2, $2, $3)
           ON CONFLICT (room_id) DO UPDATE SET member_count = EXCLUDED.member_count,
              joined_member_count = EXCLUDED.joined_member_count"#,
    )
    .bind(room_id)
    .bind(member_count)
    .bind(now)
    .execute(pool.as_ref())
    .await
    .ok();
}

/// Directly insert a device row.
async fn insert_device(pool: &Arc<sqlx::PgPool>, device_id: &str, user_id: &str, last_seen_ts: Option<i64>) {
    let now = chrono::Utc::now().timestamp_millis();
    sqlx::query(
        r#"INSERT INTO devices (device_id, user_id, display_name, last_seen_ts, created_ts, first_seen_ts)
           VALUES ($1, $2, 'dev', $3, $4, $4)"#,
    )
    .bind(device_id)
    .bind(user_id)
    .bind(last_seen_ts)
    .bind(now)
    .execute(pool.as_ref())
    .await
    .unwrap();
}

/// Directly insert a message event row (requires room to exist for FK).
async fn insert_message_event(pool: &Arc<sqlx::PgPool>, event_id: &str, room_id: &str, sender: &str) {
    let now = chrono::Utc::now().timestamp_millis();
    sqlx::query(
        r#"INSERT INTO events (event_id, room_id, sender, event_type, content, origin_server_ts, is_redacted)
           VALUES ($1, $2, $3, 'm.room.message', '{}'::JSONB, $4, FALSE)"#,
    )
    .bind(event_id)
    .bind(room_id)
    .bind(sender)
    .bind(now)
    .execute(pool.as_ref())
    .await
    .unwrap();
}

// =============================================================================
// Cursor encode / decode (free functions)
// =============================================================================

#[test]
fn test_encode_decode_cursor_round_trip() {
    let cursor = AdminUserCursor { created_ts: 1_700_000_000_000, user_id: "@alice:example.com".to_string() };
    let encoded = encode_user_cursor(&cursor);
    let decoded = decode_user_cursor(Some(&encoded)).expect("cursor should decode");
    assert_eq!(decoded, cursor);
}

#[test]
fn test_decode_cursor_none_returns_none() {
    assert_eq!(decode_user_cursor(None), None);
}

#[test]
fn test_decode_cursor_rejects_invalid_inputs() {
    // Missing delimiter.
    assert_eq!(decode_user_cursor(Some("bad-cursor")), None);
    // Empty user_id after delimiter.
    assert_eq!(decode_user_cursor(Some("123|")), None);
    // Non-numeric timestamp.
    assert_eq!(decode_user_cursor(Some("abc|@user:srv")), None);
}

#[test]
fn test_encode_cursor_format() {
    let cursor = AdminUserCursor { created_ts: 42, user_id: "@bob:srv".to_string() };
    assert_eq!(encode_user_cursor(&cursor), "42|@bob:srv");
}

// =============================================================================
// new (constructor)
// =============================================================================

#[tokio::test]
async fn test_new_constructor_builds_service() {
    let _guard = admin_user_test_guard().lock().unwrap_or_else(|e| e.into_inner());
    let pool = crate::require_test_pool().await;
    setup_test_database(&pool).await;
    let service = make_service(&pool);
    // A trivial operation proves the service was constructed with usable storages.
    let stats = service.get_user_stats().await.unwrap();
    assert!(stats.total_users >= 0);
}

// =============================================================================
// get_user_by_identifier
// =============================================================================

#[tokio::test]
async fn test_get_user_by_identifier_existing() {
    let _guard = admin_user_test_guard().lock().unwrap_or_else(|e| e.into_inner());
    let pool = crate::require_test_pool().await;
    setup_test_database(&pool).await;
    let service = make_service(&pool);

    let user_id = insert_user(&pool, "byid").await;
    let user = service.get_user_by_identifier(&user_id).await.unwrap().expect("user should exist");
    assert_eq!(user.user_id, user_id);
    assert!(!user.is_admin);
}

#[tokio::test]
async fn test_get_user_by_identifier_by_username() {
    let _guard = admin_user_test_guard().lock().unwrap_or_else(|e| e.into_inner());
    let pool = crate::require_test_pool().await;
    setup_test_database(&pool).await;
    let service = make_service(&pool);

    let user_id = insert_user(&pool, "byname").await;
    let username = user_id.strip_prefix('@').and_then(|u| u.split(':').next()).unwrap();
    let user = service.get_user_by_identifier(username).await.unwrap().expect("user should exist");
    assert_eq!(user.user_id, user_id);
}

#[tokio::test]
async fn test_get_user_by_identifier_missing_returns_none() {
    let _guard = admin_user_test_guard().lock().unwrap_or_else(|e| e.into_inner());
    let pool = crate::require_test_pool().await;
    setup_test_database(&pool).await;
    let service = make_service(&pool);

    let result = service.get_user_by_identifier(&format!("@aus_missing_{}:x", unique_id())).await;
    assert!(result.is_ok());
    assert!(result.unwrap().is_none());
}

// =============================================================================
// get_user_or_not_found
// =============================================================================

#[tokio::test]
async fn test_get_user_or_not_found_returns_user() {
    let _guard = admin_user_test_guard().lock().unwrap_or_else(|e| e.into_inner());
    let pool = crate::require_test_pool().await;
    setup_test_database(&pool).await;
    let service = make_service(&pool);

    let user_id = insert_user(&pool, "ornf").await;
    let user = service.get_user_or_not_found(&user_id).await.unwrap();
    assert_eq!(user.user_id, user_id);
}

#[tokio::test]
async fn test_get_user_or_not_found_errors_for_missing() {
    let _guard = admin_user_test_guard().lock().unwrap_or_else(|e| e.into_inner());
    let pool = crate::require_test_pool().await;
    setup_test_database(&pool).await;
    let service = make_service(&pool);

    let result = service.get_user_or_not_found(&format!("@aus_nope_{}:x", unique_id())).await;
    assert!(result.is_err());
}

// =============================================================================
// list_users_legacy
// =============================================================================

#[tokio::test]
async fn test_list_users_legacy_returns_users_and_total() {
    let _guard = admin_user_test_guard().lock().unwrap_or_else(|e| e.into_inner());
    let pool = crate::require_test_pool().await;
    setup_test_database(&pool).await;
    let service = make_service(&pool);

    let before = service.list_users_legacy(1000, None, None).await.unwrap().total;
    let _u1 = insert_user(&pool, "legacy1").await;
    let _u2 = insert_user(&pool, "legacy2").await;

    let page = service.list_users_legacy(1000, None, None).await.unwrap();
    assert!(page.total >= before + 2);
    assert!(page.users.iter().any(|u| u.user_id.starts_with("@aus_legacy1_")));
    assert!(page.users.iter().any(|u| u.user_id.starts_with("@aus_legacy2_")));
}

#[tokio::test]
async fn test_list_users_legacy_respects_limit() {
    let _guard = admin_user_test_guard().lock().unwrap_or_else(|e| e.into_inner());
    let pool = crate::require_test_pool().await;
    setup_test_database(&pool).await;
    let service = make_service(&pool);

    for i in 0..3 {
        insert_user(&pool, &format!("lim{i}")).await;
    }
    let page = service.list_users_legacy(2, None, None).await.unwrap();
    assert_eq!(page.users.len(), 2, "limit should be respected");
}

// =============================================================================
// delete_user
// =============================================================================

#[tokio::test]
async fn test_delete_user_removes_user() {
    let _guard = admin_user_test_guard().lock().unwrap_or_else(|e| e.into_inner());
    let pool = crate::require_test_pool().await;
    setup_test_database(&pool).await;
    let service = make_service(&pool);

    let user_id = insert_user(&pool, "del").await;
    assert!(service.get_user_by_identifier(&user_id).await.unwrap().is_some());

    service.delete_user(&user_id).await.unwrap();
    assert!(service.get_user_by_identifier(&user_id).await.unwrap().is_none());
}

#[tokio::test]
async fn test_delete_user_nonexistent_succeeds() {
    let _guard = admin_user_test_guard().lock().unwrap_or_else(|e| e.into_inner());
    let pool = crate::require_test_pool().await;
    setup_test_database(&pool).await;
    let service = make_service(&pool);

    // delete_user is idempotent: deleting a nonexistent user does not error.
    let result = service.delete_user(&format!("@aus_ghost_{}:x", unique_id())).await;
    assert!(result.is_ok());
}

// =============================================================================
// set_admin_status
// =============================================================================

#[tokio::test]
async fn test_set_admin_status_promotes_and_demotes() {
    let _guard = admin_user_test_guard().lock().unwrap_or_else(|e| e.into_inner());
    let pool = crate::require_test_pool().await;
    setup_test_database(&pool).await;
    let service = make_service(&pool);

    let user_id = insert_user(&pool, "admin").await;
    assert!(!service.get_user_by_identifier(&user_id).await.unwrap().unwrap().is_admin);

    service.set_admin_status(&user_id, true).await.unwrap();
    assert!(service.get_user_by_identifier(&user_id).await.unwrap().unwrap().is_admin);

    service.set_admin_status(&user_id, false).await.unwrap();
    assert!(!service.get_user_by_identifier(&user_id).await.unwrap().unwrap().is_admin);
}

// =============================================================================
// get_user_rooms_paginated
// =============================================================================

#[tokio::test]
async fn test_get_user_rooms_paginated_no_cursor() {
    let _guard = admin_user_test_guard().lock().unwrap_or_else(|e| e.into_inner());
    let pool = crate::require_test_pool().await;
    setup_test_database(&pool).await;
    let service = make_service(&pool);

    let user_id = insert_user(&pool, "rooms1").await;
    let r1 = format!("!aus_roomA_{}:{}", unique_id(), SERVER_NAME);
    let r2 = format!("!aus_roomB_{}:{}", unique_id(), SERVER_NAME);
    insert_membership(&pool, &r1, &user_id, "join").await;
    insert_membership(&pool, &r2, &user_id, "join").await;

    let rooms = service.get_user_rooms_paginated(&user_id, 10, None).await.unwrap();
    assert_eq!(rooms.len(), 2);
    assert!(rooms.contains(&r1));
    assert!(rooms.contains(&r2));
}

#[tokio::test]
async fn test_get_user_rooms_paginated_with_cursor() {
    let _guard = admin_user_test_guard().lock().unwrap_or_else(|e| e.into_inner());
    let pool = crate::require_test_pool().await;
    setup_test_database(&pool).await;
    let service = make_service(&pool);

    let user_id = insert_user(&pool, "rooms2").await;
    let r1 = format!("!aus_alpha_{}:{}", unique_id(), SERVER_NAME);
    let r2 = format!("!aus_beta_{}:{}", unique_id(), SERVER_NAME);
    // r2 > r1 lexicographically; insert r1 first but ordering is by room_id ASC.
    insert_membership(&pool, &r1, &user_id, "join").await;
    insert_membership(&pool, &r2, &user_id, "join").await;

    // Page 1: limit 1 returns the smaller room_id (r1).
    let page1 = service.get_user_rooms_paginated(&user_id, 1, None).await.unwrap();
    assert_eq!(page1.len(), 1);

    // Page 2: cursor after page1[0] returns the remaining room (r2 if r2 > page1[0]).
    let page2 = service.get_user_rooms_paginated(&user_id, 10, Some(page1[0].as_str())).await.unwrap();
    assert_eq!(page2.len(), 1);
    assert_ne!(page1[0], page2[0]);
}

#[tokio::test]
async fn test_get_user_rooms_paginated_excludes_non_join() {
    let _guard = admin_user_test_guard().lock().unwrap_or_else(|e| e.into_inner());
    let pool = crate::require_test_pool().await;
    setup_test_database(&pool).await;
    let service = make_service(&pool);

    let user_id = insert_user(&pool, "rooms3").await;
    let r = format!("!aus_leave_{}:{}", unique_id(), SERVER_NAME);
    insert_membership(&pool, &r, &user_id, "leave").await;

    let rooms = service.get_user_rooms_paginated(&user_id, 10, None).await.unwrap();
    assert!(rooms.is_empty(), "only 'join' memberships should be returned");
}

// =============================================================================
// get_user_devices
// =============================================================================

#[tokio::test]
async fn test_get_user_devices_returns_devices() {
    let _guard = admin_user_test_guard().lock().unwrap_or_else(|e| e.into_inner());
    let pool = crate::require_test_pool().await;
    setup_test_database(&pool).await;
    let service = make_service(&pool);

    let user_id = insert_user(&pool, "devs").await;
    let dev1 = format!("aus_dev1_{}", unique_id());
    let dev2 = format!("aus_dev2_{}", unique_id());
    insert_device(&pool, &dev1, &user_id, Some(1000)).await;
    insert_device(&pool, &dev2, &user_id, Some(2000)).await;

    let devices = service.get_user_devices(&user_id).await.unwrap();
    assert_eq!(devices.len(), 2);
    assert!(devices.iter().any(|d| d.device_id == dev1));
    assert!(devices.iter().any(|d| d.device_id == dev2));
}

#[tokio::test]
async fn test_get_user_devices_empty_for_user_without_devices() {
    let _guard = admin_user_test_guard().lock().unwrap_or_else(|e| e.into_inner());
    let pool = crate::require_test_pool().await;
    setup_test_database(&pool).await;
    let service = make_service(&pool);

    let user_id = insert_user(&pool, "nodev").await;
    let devices = service.get_user_devices(&user_id).await.unwrap();
    assert!(devices.is_empty());
}

// =============================================================================
// get_user_device_count
// =============================================================================

#[tokio::test]
async fn test_get_user_device_count() {
    let _guard = admin_user_test_guard().lock().unwrap_or_else(|e| e.into_inner());
    let pool = crate::require_test_pool().await;
    setup_test_database(&pool).await;
    let service = make_service(&pool);

    let user_id = insert_user(&pool, "dcount").await;
    insert_device(&pool, &format!("aus_dc1_{}", unique_id()), &user_id, None).await;
    insert_device(&pool, &format!("aus_dc2_{}", unique_id()), &user_id, None).await;

    let count = service.get_user_device_count(&user_id).await.unwrap();
    assert_eq!(count, 2);
}

#[tokio::test]
async fn test_get_user_device_count_zero() {
    let _guard = admin_user_test_guard().lock().unwrap_or_else(|e| e.into_inner());
    let pool = crate::require_test_pool().await;
    setup_test_database(&pool).await;
    let service = make_service(&pool);

    let user_id = insert_user(&pool, "dczero").await;
    let count = service.get_user_device_count(&user_id).await.unwrap();
    assert_eq!(count, 0);
}

// =============================================================================
// get_joined_room_count
// =============================================================================

#[tokio::test]
async fn test_get_joined_room_count() {
    let _guard = admin_user_test_guard().lock().unwrap_or_else(|e| e.into_inner());
    let pool = crate::require_test_pool().await;
    setup_test_database(&pool).await;
    let service = make_service(&pool);

    let user_id = insert_user(&pool, "jrcount").await;
    insert_membership(&pool, &format!("!aus_j1_{}:{}", unique_id(), SERVER_NAME), &user_id, "join").await;
    insert_membership(&pool, &format!("!aus_j2_{}:{}", unique_id(), SERVER_NAME), &user_id, "join").await;
    insert_membership(&pool, &format!("!aus_l1_{}:{}", unique_id(), SERVER_NAME), &user_id, "leave").await;

    let count = service.get_joined_room_count(&user_id).await.unwrap();
    assert_eq!(count, 2, "only 'join' memberships should be counted");
}

// =============================================================================
// evict_user_from_joined_rooms
// =============================================================================

#[tokio::test]
async fn test_evict_user_from_joined_rooms_success() {
    let _guard = admin_user_test_guard().lock().unwrap_or_else(|e| e.into_inner());
    let pool = crate::require_test_pool().await;
    setup_test_database(&pool).await;
    let service = make_service(&pool);

    let user_id = insert_user(&pool, "evict").await;
    let r1 = format!("!aus_ev1_{}:{}", unique_id(), SERVER_NAME);
    let r2 = format!("!aus_ev2_{}:{}", unique_id(), SERVER_NAME);
    insert_membership(&pool, &r1, &user_id, "join").await;
    insert_membership(&pool, &r2, &user_id, "join").await;
    insert_room_summary(&pool, &r1, 5).await;
    insert_room_summary(&pool, &r2, 3).await;

    let result = service.evict_user_from_joined_rooms(&user_id).await.unwrap();
    assert_eq!(result.joined_rooms.len(), 2);
    assert!(result.failures.is_empty(), "eviction should not fail: {result:?}");

    // After eviction, the user should have no joined rooms.
    let count = service.get_joined_room_count(&user_id).await.unwrap();
    assert_eq!(count, 0);
}

#[tokio::test]
async fn test_evict_user_with_no_joined_rooms() {
    let _guard = admin_user_test_guard().lock().unwrap_or_else(|e| e.into_inner());
    let pool = crate::require_test_pool().await;
    setup_test_database(&pool).await;
    let service = make_service(&pool);

    let user_id = insert_user(&pool, "evictempty").await;
    let result = service.evict_user_from_joined_rooms(&user_id).await.unwrap();
    assert!(result.joined_rooms.is_empty());
    assert!(result.failures.is_empty());
}

// =============================================================================
// list_users_v2
// =============================================================================

#[tokio::test]
async fn test_list_users_v2_first_page() {
    let _guard = admin_user_test_guard().lock().unwrap_or_else(|e| e.into_inner());
    let pool = crate::require_test_pool().await;
    setup_test_database(&pool).await;
    let service = make_service(&pool);

    let before = service.list_users_v2(1000, None, None).await.unwrap().total;
    let _u1 = insert_user(&pool, "v2a").await;
    let _u2 = insert_user(&pool, "v2b").await;

    let page = service.list_users_v2(1000, None, None).await.unwrap();
    assert!(page.total >= before + 2);
    assert!(page.users.iter().any(|u| u.user_id.contains("aus_v2a_")));
    // next_token is None when fewer than `limit` rows are returned.
    assert!(page.next_token.is_none());
}

#[tokio::test]
async fn test_list_users_v2_full_page_yields_next_token() {
    let _guard = admin_user_test_guard().lock().unwrap_or_else(|e| e.into_inner());
    let pool = crate::require_test_pool().await;
    setup_test_database(&pool).await;
    let service = make_service(&pool);

    for i in 0..3 {
        insert_user(&pool, &format!("v2pg{i}")).await;
    }
    // A full page (limit == rows.len()) yields a next_token.
    let page = service.list_users_v2(1, None, None).await.unwrap();
    assert_eq!(page.users.len(), 1);
    assert!(page.next_token.is_some(), "full page should yield a next_token");
}

#[tokio::test]
async fn test_list_users_v2_with_name_filter() {
    let _guard = admin_user_test_guard().lock().unwrap_or_else(|e| e.into_inner());
    let pool = crate::require_test_pool().await;
    setup_test_database(&pool).await;
    let service = make_service(&pool);

    let _u = insert_user(&pool, "filterme").await;
    let page = service.list_users_v2(1000, None, Some("filterme")).await.unwrap();
    // AdminUserListItem has no `username` field; assert via user_id which
    // embeds the same prefix used for the username.
    assert!(page.users.iter().all(|u| u.user_id.contains("filterme")));
    assert!(page.users.iter().any(|u| u.user_id.contains("aus_filterme_")));
}

#[tokio::test]
async fn test_list_users_v2_with_cursor_continues() {
    let _guard = admin_user_test_guard().lock().unwrap_or_else(|e| e.into_inner());
    let pool = crate::require_test_pool().await;
    setup_test_database(&pool).await;
    let service = make_service(&pool);

    let _u1 = insert_user(&pool, "cur1").await;
    let _u2 = insert_user(&pool, "cur2").await;

    let page1 = service.list_users_v2(1, None, None).await.unwrap();
    assert!(page1.next_token.is_some());
    let cursor = decode_user_cursor(page1.next_token.as_deref()).expect("cursor decodes");
    let page2 = service.list_users_v2(1000, Some(cursor), None).await.unwrap();
    // page2 should not contain page1's user.
    assert!(!page2.users.iter().any(|u| u.user_id == page1.users[0].user_id));
}

// =============================================================================
// get_user_v2
// =============================================================================

#[tokio::test]
async fn test_get_user_v2_returns_details_with_devices() {
    let _guard = admin_user_test_guard().lock().unwrap_or_else(|e| e.into_inner());
    let pool = crate::require_test_pool().await;
    setup_test_database(&pool).await;
    let service = make_service(&pool);

    let user_id = insert_user(&pool, "v2get").await;
    let dev = format!("aus_v2dev_{}", unique_id());
    insert_device(&pool, &dev, &user_id, Some(12345)).await;

    let details = service.get_user_v2(&user_id).await.unwrap().expect("details should exist");
    assert_eq!(details.user.user_id, user_id);
    assert_eq!(details.devices.len(), 1);
    assert_eq!(details.devices[0].device_id, dev);
    assert_eq!(details.devices[0].last_seen_ts, Some(12345));
}

#[tokio::test]
async fn test_get_user_v2_returns_none_for_missing() {
    let _guard = admin_user_test_guard().lock().unwrap_or_else(|e| e.into_inner());
    let pool = crate::require_test_pool().await;
    setup_test_database(&pool).await;
    let service = make_service(&pool);

    let result = service.get_user_v2(&format!("@aus_missing_{}:x", unique_id())).await;
    assert!(result.is_ok());
    assert!(result.unwrap().is_none());
}

#[test]
fn test_admin_user_profile_from_user() {
    use synapse_storage::user::User;
    let now = chrono::Utc::now().timestamp_millis();
    let user = User {
        user_id: "@aus_conv:x".to_string(),
        username: "aus_conv".to_string(),
        password_hash: None,
        is_admin: true,
        is_guest: false,
        is_shadow_banned: false,
        is_deactivated: false,
        created_ts: now,
        updated_ts: None,
        displayname: Some("Aus Conv".to_string()),
        avatar_url: Some("mxc://x/a".to_string()),
        email: None,
        phone: None,
        generation: now,
        consent_version: None,
        appservice_id: None,
        user_type: Some("bot".to_string()),
        invalid_update_at: None,
        migration_state: None,
        password_changed_ts: None,
        is_password_change_required: false,
        password_expires_at: None,
        failed_login_attempts: 0,
        locked_until: None,
        must_change_password: false,
    };
    let profile = synapse_services::admin_user_service::AdminUserProfile::from(&user);
    assert_eq!(profile.user_id, "@aus_conv:x");
    assert_eq!(profile.username, "aus_conv");
    assert!(profile.is_admin);
    assert!(!profile.is_deactivated);
    assert_eq!(profile.displayname.as_deref(), Some("Aus Conv"));
    assert_eq!(profile.user_type.as_deref(), Some("bot"));
}

// =============================================================================
// create_or_update_user_v2
// =============================================================================

#[tokio::test]
async fn test_create_or_update_user_v2_creates_new() {
    let _guard = admin_user_test_guard().lock().unwrap_or_else(|e| e.into_inner());
    let pool = crate::require_test_pool().await;
    setup_test_database(&pool).await;
    let service = make_service(&pool);

    let ident = format!("aus_new_{}", unique_id());
    service
        .create_or_update_user_v2(
            &ident,
            Some("Display Name"),
            Some("mxc://avatar"),
            Some(true),
            Some(false),
            Some("bot"),
            Some("secret123"),
        )
        .await
        .unwrap();

    let user_id = format!("@{ident}:{SERVER_NAME}");
    let user = service.get_user_by_identifier(&user_id).await.unwrap().expect("user should be created");
    assert_eq!(user.displayname.as_deref(), Some("Display Name"));
    assert_eq!(user.avatar_url.as_deref(), Some("mxc://avatar"));
    assert!(user.is_admin);
    assert!(!user.is_deactivated);
    assert_eq!(user.user_type.as_deref(), Some("bot"));
    assert!(user.password_hash.is_some());
}

#[tokio::test]
async fn test_create_or_update_user_v2_with_full_identifier() {
    let _guard = admin_user_test_guard().lock().unwrap_or_else(|e| e.into_inner());
    let pool = crate::require_test_pool().await;
    setup_test_database(&pool).await;
    let service = make_service(&pool);

    // Identifier starting with '@' is used verbatim.
    let id = unique_id();
    let full_id = format!("@aus_full_{id}:explicit.server");
    service
        .create_or_update_user_v2(&full_id, None, None, None, None, None, Some("pw"))
        .await
        .unwrap();

    let user = service.get_user_by_identifier(&full_id).await.unwrap().expect("user should be created");
    assert_eq!(user.user_id, full_id);
    // username is derived by stripping '@' and taking the part before ':'.
    assert_eq!(user.username, format!("aus_full_{id}"));
}

#[tokio::test]
async fn test_create_or_update_user_v2_updates_existing() {
    let _guard = admin_user_test_guard().lock().unwrap_or_else(|e| e.into_inner());
    let pool = crate::require_test_pool().await;
    setup_test_database(&pool).await;
    let service = make_service(&pool);

    let user_id = insert_user(&pool, "upd").await;
    // Re-fetch the username form for the identifier.
    let username = user_id.strip_prefix('@').and_then(|u| u.split(':').next()).unwrap().to_string();

    service
        .create_or_update_user_v2(
            &username,
            Some("Updated"),
            Some("mxc://new"),
            Some(true),
            Some(true),
            Some("service"),
            Some("newpw"),
        )
        .await
        .unwrap();

    let user = service.get_user_by_identifier(&user_id).await.unwrap().expect("user should still exist");
    assert_eq!(user.displayname.as_deref(), Some("Updated"));
    assert_eq!(user.avatar_url.as_deref(), Some("mxc://new"));
    assert!(user.is_admin);
    assert!(user.is_deactivated);
    assert_eq!(user.user_type.as_deref(), Some("service"));
}

#[tokio::test]
async fn test_create_or_update_user_v2_no_password_generates_random() {
    let _guard = admin_user_test_guard().lock().unwrap_or_else(|e| e.into_inner());
    let pool = crate::require_test_pool().await;
    setup_test_database(&pool).await;
    let service = make_service(&pool);

    let ident = format!("aus_nopw_{}", unique_id());
    service.create_or_update_user_v2(&ident, None, None, None, None, None, None).await.unwrap();

    let user_id = format!("@{ident}:{SERVER_NAME}");
    let user = service.get_user_by_identifier(&user_id).await.unwrap().expect("user created");
    // A random password hash should have been generated.
    assert!(user.password_hash.is_some());
}

// =============================================================================
// get_user_stats
// =============================================================================

#[tokio::test]
async fn test_get_user_stats() {
    let _guard = admin_user_test_guard().lock().unwrap_or_else(|e| e.into_inner());
    let pool = crate::require_test_pool().await;
    setup_test_database(&pool).await;
    let service = make_service(&pool);

    let before = service.get_user_stats().await.unwrap();
    let user_id = insert_user(&pool, "stat").await;
    // Promote to admin.
    service.set_admin_status(&user_id, true).await.unwrap();

    let after = service.get_user_stats().await.unwrap();
    assert!(after.total_users >= before.total_users + 1);
    assert!(after.admin_users >= before.admin_users + 1);
    // average_rooms_per_user is a non-negative finite number.
    assert!(after.average_rooms_per_user >= 0.0);
    assert!(after.average_rooms_per_user.is_finite());
}

#[tokio::test]
async fn test_get_user_stats_with_no_users_average_is_zero() {
    let _guard = admin_user_test_guard().lock().unwrap_or_else(|e| e.into_inner());
    let pool = crate::require_test_pool().await;
    setup_test_database(&pool).await;
    let service = make_service(&pool);

    // snapshot total; if there are zero users the average must be 0.0.
    let stats = service.get_user_stats().await.unwrap();
    if stats.total_users == 0 {
        assert_eq!(stats.average_rooms_per_user, 0.0);
    }
    // Always populated (smoke check).
    assert!(stats.total_users >= 0);
    assert!(stats.active_users >= 0);
    assert!(stats.deactivated_users >= 0);
    assert!(stats.guest_users >= 0);
}

// =============================================================================
// get_single_user_stats
// =============================================================================

#[tokio::test]
async fn test_get_single_user_stats() {
    let _guard = admin_user_test_guard().lock().unwrap_or_else(|e| e.into_inner());
    let pool = crate::require_test_pool().await;
    setup_test_database(&pool).await;
    let service = make_service(&pool);

    let user_id = insert_user(&pool, "single").await;
    let room = format!("!aus_sng_{}:{}", unique_id(), SERVER_NAME);
    insert_room(&pool, &room).await;
    insert_membership(&pool, &room, &user_id, "join").await;
    let dev = format!("aus_sngdev_{}", unique_id());
    insert_device(&pool, &dev, &user_id, Some(99999)).await;
    let evt = format!("$aus_evt_{}", unique_id());
    insert_message_event(&pool, &evt, &room, &user_id).await;

    let stats = service.get_single_user_stats(&user_id).await.unwrap();
    assert_eq!(stats.user.user_id, user_id);
    assert_eq!(stats.rooms_joined, 1);
    assert_eq!(stats.messages_sent, 1);
    assert_eq!(stats.last_seen_ts, Some(99999));
}

#[tokio::test]
async fn test_get_single_user_stats_errors_for_missing() {
    let _guard = admin_user_test_guard().lock().unwrap_or_else(|e| e.into_inner());
    let pool = crate::require_test_pool().await;
    setup_test_database(&pool).await;
    let service = make_service(&pool);

    let result = service.get_single_user_stats(&format!("@aus_nope_{}:x", unique_id())).await;
    assert!(result.is_err());
}

// =============================================================================
// batch_create_users
// =============================================================================

#[tokio::test]
async fn test_batch_create_users_success() {
    let _guard = admin_user_test_guard().lock().unwrap_or_else(|e| e.into_inner());
    let pool = crate::require_test_pool().await;
    setup_test_database(&pool).await;
    let service = make_service(&pool);

    let id = unique_id();
    let users = vec![
        (format!("aus_bc1_{id}"), "pw1".to_string(), Some("Disp1".to_string()), false),
        (format!("aus_bc2_{id}"), "pw2".to_string(), None, true),
    ];
    let result = service.batch_create_users(&users).await.unwrap();
    assert_eq!(result.succeeded.len(), 2);
    assert!(result.failed.is_empty());

    // Both users exist.
    let u1 = format!("@aus_bc1_{id}:{SERVER_NAME}");
    let u2 = format!("@aus_bc2_{id}:{SERVER_NAME}");
    assert!(service.get_user_by_identifier(&u1).await.unwrap().is_some());
    assert!(service.get_user_by_identifier(&u2).await.unwrap().is_some());
    // displayname was set for the first user.
    let fetched = service.get_user_by_identifier(&u1).await.unwrap().unwrap();
    assert_eq!(fetched.displayname.as_deref(), Some("Disp1"));
}

#[tokio::test]
async fn test_batch_create_users_with_duplicate_failure() {
    let _guard = admin_user_test_guard().lock().unwrap_or_else(|e| e.into_inner());
    let pool = crate::require_test_pool().await;
    setup_test_database(&pool).await;
    let service = make_service(&pool);

    let id = unique_id();
    // Pre-create a user with the same username to force a failure.
    let existing = format!("@aus_dup_{id}:{SERVER_NAME}");
    {
        let cache = Arc::new(CacheManager::new(&CacheConfig::default()));
        let us = UserStorage::new(&pool, cache);
        us.create_user(&existing, &format!("aus_dup_{id}"), Some("hash"), false).await.unwrap();
    }

    let users = vec![
        (format!("aus_dup_{id}"), "pw".to_string(), None, false), // duplicate -> fails
        (format!("aus_ok_{id}"), "pw".to_string(), None, false),  // succeeds
    ];
    let result = service.batch_create_users(&users).await.unwrap();
    assert_eq!(result.succeeded.len(), 1);
    assert_eq!(result.failed.len(), 1);
    assert_eq!(result.failed[0], format!("aus_dup_{id}"));
}

#[tokio::test]
async fn test_batch_create_users_empty_input() {
    let _guard = admin_user_test_guard().lock().unwrap_or_else(|e| e.into_inner());
    let pool = crate::require_test_pool().await;
    setup_test_database(&pool).await;
    let service = make_service(&pool);

    let result = service.batch_create_users(&[]).await.unwrap();
    assert!(result.succeeded.is_empty());
    assert!(result.failed.is_empty());
}

// =============================================================================
// batch_deactivate_users
// =============================================================================

#[tokio::test]
async fn test_batch_deactivate_users_success() {
    let _guard = admin_user_test_guard().lock().unwrap_or_else(|e| e.into_inner());
    let pool = crate::require_test_pool().await;
    setup_test_database(&pool).await;
    let service = make_service(&pool);

    let u1 = insert_user(&pool, "bd1").await;
    let u2 = insert_user(&pool, "bd2").await;
    let result = service.batch_deactivate_users(&[u1.clone(), u2.clone()]).await.unwrap();
    assert_eq!(result.succeeded.len(), 2);
    assert!(result.failed.is_empty());

    assert!(service.get_user_by_identifier(&u1).await.unwrap().unwrap().is_deactivated);
    assert!(service.get_user_by_identifier(&u2).await.unwrap().unwrap().is_deactivated);
}

#[tokio::test]
async fn test_batch_deactivate_users_rejects_invalid_format() {
    let _guard = admin_user_test_guard().lock().unwrap_or_else(|e| e.into_inner());
    let pool = crate::require_test_pool().await;
    setup_test_database(&pool).await;
    let service = make_service(&pool);

    // "nocolon" lacks ':' and "@" prefix; "noatsign" lacks '@'.
    let result = service
        .batch_deactivate_users(&["nocolon".to_string(), "noatsign:x".to_string()])
        .await
        .unwrap();
    assert_eq!(result.succeeded.len(), 0);
    assert_eq!(result.failed.len(), 2);
}

#[tokio::test]
async fn test_batch_deactivate_users_nonexistent_marked_failed() {
    let _guard = admin_user_test_guard().lock().unwrap_or_else(|e| e.into_inner());
    let pool = crate::require_test_pool().await;
    setup_test_database(&pool).await;
    let service = make_service(&pool);

    // Valid format but does not exist -> set_deactivation_status returns false -> failed.
    let missing = format!("@aus_ghost_{}:{SERVER_NAME}", unique_id());
    let result = service.batch_deactivate_users(&[missing.clone()]).await.unwrap();
    assert_eq!(result.succeeded.len(), 0);
    assert_eq!(result.failed.len(), 1);
    assert_eq!(result.failed[0], missing);
}

// =============================================================================
// update_account
// =============================================================================

#[tokio::test]
async fn test_update_account_updates_fields() {
    let _guard = admin_user_test_guard().lock().unwrap_or_else(|e| e.into_inner());
    let pool = crate::require_test_pool().await;
    setup_test_database(&pool).await;
    let service = make_service(&pool);

    let user_id = insert_user(&pool, "acct").await;
    service
        .update_account(&user_id, Some("New Display"), Some("mxc://avatar2"), Some(true))
        .await
        .unwrap();

    let user = service.get_user_by_identifier(&user_id).await.unwrap().unwrap();
    assert_eq!(user.displayname.as_deref(), Some("New Display"));
    assert_eq!(user.avatar_url.as_deref(), Some("mxc://avatar2"));
    assert!(user.is_admin);
}

#[tokio::test]
async fn test_update_account_no_changes_is_noop() {
    let _guard = admin_user_test_guard().lock().unwrap_or_else(|e| e.into_inner());
    let pool = crate::require_test_pool().await;
    setup_test_database(&pool).await;
    let service = make_service(&pool);

    let user_id = insert_user(&pool, "noop").await;
    // All-None update should succeed and change nothing.
    service.update_account(&user_id, None, None, None).await.unwrap();
    let user = service.get_user_by_identifier(&user_id).await.unwrap().unwrap();
    assert!(user.displayname.is_none());
    assert!(user.avatar_url.is_none());
    assert!(!user.is_admin);
}

#[tokio::test]
async fn test_update_account_partial_update_only_displayname() {
    let _guard = admin_user_test_guard().lock().unwrap_or_else(|e| e.into_inner());
    let pool = crate::require_test_pool().await;
    setup_test_database(&pool).await;
    let service = make_service(&pool);

    let user_id = insert_user(&pool, "partial").await;
    service.update_account(&user_id, Some("Only Name"), None, None).await.unwrap();

    let user = service.get_user_by_identifier(&user_id).await.unwrap().unwrap();
    assert_eq!(user.displayname.as_deref(), Some("Only Name"));
    assert!(user.avatar_url.is_none(), "avatar_url should be untouched");
    assert!(!user.is_admin, "is_admin should be untouched");
}
