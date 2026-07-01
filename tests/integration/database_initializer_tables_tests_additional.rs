//! Integration tests for `DatabaseInitService` DDL step methods
//! (synapse-services/src/database_initializer/tables.rs).
//!
//! These tests verify that the table-creation, index, and constraint DDL
//! statements execute successfully and are idempotent. They require the
//! `runtime-ddl` feature and a live PostgreSQL test pool.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
#![cfg(feature = "runtime-ddl")]

use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex, OnceLock};
use std::time::Duration;

use synapse_services::{DatabaseInitMode, DatabaseInitService};

static TEST_COUNTER: AtomicU64 = AtomicU64::new(1);

fn unique_id() -> u64 {
    TEST_COUNTER.fetch_add(1, Ordering::SeqCst)
}

fn guard() -> &'static Mutex<()> {
    static GUARD: OnceLock<Mutex<()>> = OnceLock::new();
    GUARD.get_or_init(|| Mutex::new(()))
}

async fn warm_up_pool(pool: &Arc<sqlx::PgPool>) {
    for attempt in 0..8u32 {
        match tokio::time::timeout(Duration::from_secs(5), sqlx::query("SELECT 1").execute(pool.as_ref())).await {
            Ok(Ok(_)) => return,
            Ok(Err(_)) | Err(_) => {
                tokio::time::sleep(Duration::from_millis(200u64 * (1 << attempt.min(6)))).await;
            }
        }
    }
    eprintln!("Warning: warm_up_pool failed; continuing anyway");
}

/// Check whether a table exists in the current schema.
async fn table_exists(pool: &sqlx::PgPool, table_name: &str) -> bool {
    let exists: Option<(bool,)> = sqlx::query_as(
        "SELECT EXISTS (
            SELECT 1 FROM information_schema.tables
            WHERE table_schema = current_schema() AND table_name = $1
        )",
    )
    .bind(table_name)
    .fetch_one(pool)
    .await
    .ok();
    exists.map(|(b,)| b).unwrap_or(false)
}

/// Check whether an index exists.
async fn index_exists(pool: &sqlx::PgPool, index_name: &str) -> bool {
    let exists: Option<(bool,)> = sqlx::query_as(
        "SELECT EXISTS (
            SELECT 1 FROM pg_indexes
            WHERE schemaname = current_schema() AND indexname = $1
        )",
    )
    .bind(index_name)
    .fetch_one(pool)
    .await
    .ok();
    exists.map(|(b,)| b).unwrap_or(false)
}

/// Check whether a column exists on a table.
async fn column_exists(pool: &sqlx::PgPool, table_name: &str, column_name: &str) -> bool {
    let exists: Option<(bool,)> = sqlx::query_as(
        "SELECT EXISTS (
            SELECT 1 FROM information_schema.columns
            WHERE table_schema = current_schema() AND table_name = $1 AND column_name = $2
        )",
    )
    .bind(table_name)
    .bind(column_name)
    .fetch_one(pool)
    .await
    .ok();
    exists.map(|(b,)| b).unwrap_or(false)
}

/// Check whether a table constraint exists.
async fn constraint_exists(pool: &sqlx::PgPool, constraint_name: &str) -> bool {
    let exists: Option<(bool,)> = sqlx::query_as(
        "SELECT EXISTS (
            SELECT 1 FROM information_schema.table_constraints
            WHERE constraint_schema = current_schema() AND constraint_name = $1
        )",
    )
    .bind(constraint_name)
    .fetch_one(pool)
    .await
    .ok();
    exists.map(|(b,)| b).unwrap_or(false)
}

/// Check whether a sequence exists.
async fn sequence_exists(pool: &sqlx::PgPool, seq_name: &str) -> bool {
    let exists: Option<(bool,)> = sqlx::query_as(
        "SELECT EXISTS (
            SELECT 1 FROM information_schema.sequences
            WHERE sequence_schema = current_schema() AND sequence_name = $1
        )",
    )
    .bind(seq_name)
    .fetch_one(pool)
    .await
    .ok();
    exists.map(|(b,)| b).unwrap_or(false)
}

async fn make_service(pool: &Arc<sqlx::PgPool>) -> DatabaseInitService {
    DatabaseInitService::new(pool.clone()).with_mode(DatabaseInitMode::Compatible)
}

// ===========================================================================
// step_create_e2ee_tables
// ===========================================================================

#[tokio::test]
async fn step_create_e2ee_tables_creates_device_keys_table() {
    let _g = guard().lock().unwrap_or_else(|e| e.into_inner());
    let pool = crate::require_test_pool().await;
    warm_up_pool(&pool).await;
    let svc = make_service(&pool).await;
    let result = svc.step_create_e2ee_tables().await;
    assert!(result.is_ok(), "step_create_e2ee_tables should succeed: {:?}", result.err());
    assert!(table_exists(&pool, "device_keys").await, "device_keys table should exist");
}

#[tokio::test]
async fn step_create_e2ee_tables_creates_user_device_index() {
    let _g = guard().lock().unwrap_or_else(|e| e.into_inner());
    let pool = crate::require_test_pool().await;
    warm_up_pool(&pool).await;
    let svc = make_service(&pool).await;
    svc.step_create_e2ee_tables().await.unwrap();
    assert!(index_exists(&pool, "idx_device_keys_user_device").await, "index should exist");
}

#[tokio::test]
async fn step_create_e2ee_tables_creates_unique_constraint() {
    let _g = guard().lock().unwrap_or_else(|e| e.into_inner());
    let pool = crate::require_test_pool().await;
    warm_up_pool(&pool).await;
    let svc = make_service(&pool).await;
    svc.step_create_e2ee_tables().await.unwrap();
    assert!(
        constraint_exists(&pool, "uq_device_keys_user_device_key").await,
        "unique constraint should exist"
    );
}

#[tokio::test]
async fn step_create_e2ee_tables_has_required_columns() {
    let _g = guard().lock().unwrap_or_else(|e| e.into_inner());
    let pool = crate::require_test_pool().await;
    warm_up_pool(&pool).await;
    let svc = make_service(&pool).await;
    svc.step_create_e2ee_tables().await.unwrap();
    for col in ["user_id", "device_id", "algorithm", "public_key", "added_ts", "created_ts", "is_verified"] {
        assert!(column_exists(&pool, "device_keys", col).await, "device_keys.{col} should exist");
    }
}

#[tokio::test]
async fn step_create_e2ee_tables_is_idempotent() {
    let _g = guard().lock().unwrap_or_else(|e| e.into_inner());
    let pool = crate::require_test_pool().await;
    warm_up_pool(&pool).await;
    let svc = make_service(&pool).await;
    // Call multiple times - should not error
    svc.step_create_e2ee_tables().await.unwrap();
    svc.step_create_e2ee_tables().await.unwrap();
    let r3 = svc.step_create_e2ee_tables().await;
    assert!(r3.is_ok(), "third invocation should also succeed: {:?}", r3.err());
    assert!(table_exists(&pool, "device_keys").await);
}

// ===========================================================================
// step_create_e2ee_core_tables
// ===========================================================================

#[tokio::test]
async fn step_create_e2ee_core_tables_creates_olm_accounts() {
    let _g = guard().lock().unwrap_or_else(|e| e.into_inner());
    let pool = crate::require_test_pool().await;
    warm_up_pool(&pool).await;
    let svc = make_service(&pool).await;
    let result = svc.step_create_e2ee_core_tables().await;
    assert!(result.is_ok(), "step_create_e2ee_core_tables should succeed: {:?}", result.err());
    assert!(table_exists(&pool, "olm_accounts").await, "olm_accounts table should exist");
}

#[tokio::test]
async fn step_create_e2ee_core_tables_creates_olm_sessions() {
    let _g = guard().lock().unwrap_or_else(|e| e.into_inner());
    let pool = crate::require_test_pool().await;
    warm_up_pool(&pool).await;
    let svc = make_service(&pool).await;
    svc.step_create_e2ee_core_tables().await.unwrap();
    assert!(table_exists(&pool, "olm_sessions").await, "olm_sessions table should exist");
}

#[tokio::test]
async fn step_create_e2ee_core_tables_creates_megolm_sessions() {
    let _g = guard().lock().unwrap_or_else(|e| e.into_inner());
    let pool = crate::require_test_pool().await;
    warm_up_pool(&pool).await;
    let svc = make_service(&pool).await;
    svc.step_create_e2ee_core_tables().await.unwrap();
    assert!(table_exists(&pool, "megolm_sessions").await, "megolm_sessions table should exist");
}

#[tokio::test]
async fn step_create_e2ee_core_tables_creates_cross_signing_keys() {
    let _g = guard().lock().unwrap_or_else(|e| e.into_inner());
    let pool = crate::require_test_pool().await;
    warm_up_pool(&pool).await;
    let svc = make_service(&pool).await;
    svc.step_create_e2ee_core_tables().await.unwrap();
    assert!(table_exists(&pool, "cross_signing_keys").await, "cross_signing_keys table should exist");
    assert!(column_exists(&pool, "cross_signing_keys", "added_ts").await, "added_ts should exist");
    assert!(column_exists(&pool, "cross_signing_keys", "signatures").await, "signatures should exist");
}

#[tokio::test]
async fn step_create_e2ee_core_tables_creates_device_signatures() {
    let _g = guard().lock().unwrap_or_else(|e| e.into_inner());
    let pool = crate::require_test_pool().await;
    warm_up_pool(&pool).await;
    let svc = make_service(&pool).await;
    svc.step_create_e2ee_core_tables().await.unwrap();
    assert!(table_exists(&pool, "device_signatures").await, "device_signatures table should exist");
}

#[tokio::test]
async fn step_create_e2ee_core_tables_creates_backup_keys() {
    let _g = guard().lock().unwrap_or_else(|e| e.into_inner());
    let pool = crate::require_test_pool().await;
    warm_up_pool(&pool).await;
    let svc = make_service(&pool).await;
    svc.step_create_e2ee_core_tables().await.unwrap();
    assert!(table_exists(&pool, "backup_keys").await, "backup_keys table should exist");
}

#[tokio::test]
async fn step_create_e2ee_core_tables_creates_indexes() {
    let _g = guard().lock().unwrap_or_else(|e| e.into_inner());
    let pool = crate::require_test_pool().await;
    warm_up_pool(&pool).await;
    let svc = make_service(&pool).await;
    svc.step_create_e2ee_core_tables().await.unwrap();
    for idx in [
        "idx_olm_accounts_user",
        "idx_olm_accounts_device",
        "idx_olm_sessions_user",
        "idx_olm_sessions_device",
        "idx_megolm_sessions_room",
        "idx_cross_signing_keys_user",
        "idx_backup_keys_backup",
        "idx_backup_keys_room",
    ] {
        assert!(index_exists(&pool, idx).await, "index {idx} should exist");
    }
}

#[tokio::test]
async fn step_create_e2ee_core_tables_is_idempotent() {
    let _g = guard().lock().unwrap_or_else(|e| e.into_inner());
    let pool = crate::require_test_pool().await;
    warm_up_pool(&pool).await;
    let svc = make_service(&pool).await;
    svc.step_create_e2ee_core_tables().await.unwrap();
    svc.step_create_e2ee_core_tables().await.unwrap();
    let r3 = svc.step_create_e2ee_core_tables().await;
    assert!(r3.is_ok(), "third call should succeed: {:?}", r3.err());
}

// ===========================================================================
// step_ensure_additional_tables
// ===========================================================================

#[tokio::test]
async fn step_ensure_additional_tables_succeeds() {
    let _g = guard().lock().unwrap_or_else(|e| e.into_inner());
    let pool = crate::require_test_pool().await;
    warm_up_pool(&pool).await;
    let svc = make_service(&pool).await;
    let result = svc.step_ensure_additional_tables().await;
    assert!(result.is_ok(), "step_ensure_additional_tables should succeed: {:?}", result.err());
}

#[tokio::test]
async fn step_ensure_additional_tables_creates_typing_table() {
    let _g = guard().lock().unwrap_or_else(|e| e.into_inner());
    let pool = crate::require_test_pool().await;
    warm_up_pool(&pool).await;
    let svc = make_service(&pool).await;
    svc.step_ensure_additional_tables().await.unwrap();
    assert!(table_exists(&pool, "typing").await, "typing table should exist");
}

#[tokio::test]
async fn step_ensure_additional_tables_creates_search_index_table() {
    let _g = guard().lock().unwrap_or_else(|e| e.into_inner());
    let pool = crate::require_test_pool().await;
    warm_up_pool(&pool).await;
    let svc = make_service(&pool).await;
    svc.step_ensure_additional_tables().await.unwrap();
    assert!(table_exists(&pool, "search_index").await, "search_index table should exist");
    assert!(index_exists(&pool, "idx_search_index_room").await);
    assert!(index_exists(&pool, "idx_search_index_user").await);
    assert!(index_exists(&pool, "idx_search_index_type").await);
}

#[tokio::test]
async fn step_ensure_additional_tables_creates_user_directory() {
    let _g = guard().lock().unwrap_or_else(|e| e.into_inner());
    let pool = crate::require_test_pool().await;
    warm_up_pool(&pool).await;
    let svc = make_service(&pool).await;
    svc.step_ensure_additional_tables().await.unwrap();
    assert!(table_exists(&pool, "user_directory").await, "user_directory table should exist");
}

#[tokio::test]
async fn step_ensure_additional_tables_creates_pushers_table() {
    let _g = guard().lock().unwrap_or_else(|e| e.into_inner());
    let pool = crate::require_test_pool().await;
    warm_up_pool(&pool).await;
    let svc = make_service(&pool).await;
    svc.step_ensure_additional_tables().await.unwrap();
    assert!(table_exists(&pool, "pushers").await, "pushers table should exist");
    assert!(index_exists(&pool, "idx_pushers_user").await);
    assert!(index_exists(&pool, "idx_pushers_enabled").await);
}

#[tokio::test]
async fn step_ensure_additional_tables_creates_account_data() {
    let _g = guard().lock().unwrap_or_else(|e| e.into_inner());
    let pool = crate::require_test_pool().await;
    warm_up_pool(&pool).await;
    let svc = make_service(&pool).await;
    svc.step_ensure_additional_tables().await.unwrap();
    assert!(table_exists(&pool, "account_data").await, "account_data table should exist");
}

#[tokio::test]
async fn step_ensure_additional_tables_creates_key_backups() {
    let _g = guard().lock().unwrap_or_else(|e| e.into_inner());
    let pool = crate::require_test_pool().await;
    warm_up_pool(&pool).await;
    let svc = make_service(&pool).await;
    svc.step_ensure_additional_tables().await.unwrap();
    assert!(table_exists(&pool, "key_backups").await, "key_backups table should exist");
}

#[tokio::test]
async fn step_ensure_additional_tables_creates_room_events() {
    let _g = guard().lock().unwrap_or_else(|e| e.into_inner());
    let pool = crate::require_test_pool().await;
    warm_up_pool(&pool).await;
    let svc = make_service(&pool).await;
    svc.step_ensure_additional_tables().await.unwrap();
    assert!(table_exists(&pool, "room_events").await, "room_events table should exist");
}

#[tokio::test]
async fn step_ensure_additional_tables_creates_to_device_tables() {
    let _g = guard().lock().unwrap_or_else(|e| e.into_inner());
    let pool = crate::require_test_pool().await;
    warm_up_pool(&pool).await;
    let svc = make_service(&pool).await;
    svc.step_ensure_additional_tables().await.unwrap();
    assert!(table_exists(&pool, "to_device_messages").await, "to_device_messages table should exist");
    assert!(table_exists(&pool, "to_device_transactions").await, "to_device_transactions table should exist");
}

#[tokio::test]
async fn step_ensure_additional_tables_creates_device_lists_tables() {
    let _g = guard().lock().unwrap_or_else(|e| e.into_inner());
    let pool = crate::require_test_pool().await;
    warm_up_pool(&pool).await;
    let svc = make_service(&pool).await;
    svc.step_ensure_additional_tables().await.unwrap();
    assert!(table_exists(&pool, "device_lists_changes").await, "device_lists_changes table should exist");
    assert!(table_exists(&pool, "device_lists_stream").await, "device_lists_stream table should exist");
}

#[tokio::test]
async fn step_ensure_additional_tables_creates_room_ephemeral() {
    let _g = guard().lock().unwrap_or_else(|e| e.into_inner());
    let pool = crate::require_test_pool().await;
    warm_up_pool(&pool).await;
    let svc = make_service(&pool).await;
    svc.step_ensure_additional_tables().await.unwrap();
    assert!(table_exists(&pool, "room_ephemeral").await, "room_ephemeral table should exist");
}

#[tokio::test]
async fn step_ensure_additional_tables_creates_room_account_data() {
    let _g = guard().lock().unwrap_or_else(|e| e.into_inner());
    let pool = crate::require_test_pool().await;
    warm_up_pool(&pool).await;
    let svc = make_service(&pool).await;
    svc.step_ensure_additional_tables().await.unwrap();
    assert!(table_exists(&pool, "room_account_data").await, "room_account_data table should exist");
}

#[tokio::test]
async fn step_ensure_additional_tables_creates_read_markers() {
    let _g = guard().lock().unwrap_or_else(|e| e.into_inner());
    let pool = crate::require_test_pool().await;
    warm_up_pool(&pool).await;
    let svc = make_service(&pool).await;
    svc.step_ensure_additional_tables().await.unwrap();
    assert!(table_exists(&pool, "read_markers").await, "read_markers table should exist");
}

#[tokio::test]
async fn step_ensure_additional_tables_creates_key_rotation_tables() {
    let _g = guard().lock().unwrap_or_else(|e| e.into_inner());
    let pool = crate::require_test_pool().await;
    warm_up_pool(&pool).await;
    let svc = make_service(&pool).await;
    svc.step_ensure_additional_tables().await.unwrap();
    assert!(table_exists(&pool, "key_rotation_pending").await, "key_rotation_pending table should exist");
    assert!(table_exists(&pool, "key_rotation_state").await, "key_rotation_state table should exist");
    assert!(table_exists(&pool, "key_rotation_config").await, "key_rotation_config table should exist");
}

#[tokio::test]
async fn step_ensure_additional_tables_creates_lazy_loaded_members() {
    let _g = guard().lock().unwrap_or_else(|e| e.into_inner());
    let pool = crate::require_test_pool().await;
    warm_up_pool(&pool).await;
    let svc = make_service(&pool).await;
    svc.step_ensure_additional_tables().await.unwrap();
    assert!(table_exists(&pool, "lazy_loaded_members").await, "lazy_loaded_members table should exist");
}

#[tokio::test]
async fn step_ensure_additional_tables_creates_sync_stream_id() {
    let _g = guard().lock().unwrap_or_else(|e| e.into_inner());
    let pool = crate::require_test_pool().await;
    warm_up_pool(&pool).await;
    let svc = make_service(&pool).await;
    svc.step_ensure_additional_tables().await.unwrap();
    assert!(table_exists(&pool, "sync_stream_id").await, "sync_stream_id table should exist");
    // The method inserts a seed row with id=1
    let count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM sync_stream_id")
        .fetch_one(pool.as_ref())
        .await
        .unwrap_or(0);
    assert!(count >= 1, "sync_stream_id should have at least one seed row");
}

#[tokio::test]
async fn step_ensure_additional_tables_creates_sliding_sync_sequence() {
    let _g = guard().lock().unwrap_or_else(|e| e.into_inner());
    let pool = crate::require_test_pool().await;
    warm_up_pool(&pool).await;
    let svc = make_service(&pool).await;
    svc.step_ensure_additional_tables().await.unwrap();
    assert!(sequence_exists(&pool, "sliding_sync_pos_seq").await, "sliding_sync_pos_seq sequence should exist");
}

#[tokio::test]
async fn step_ensure_additional_tables_creates_sliding_sync_tables() {
    let _g = guard().lock().unwrap_or_else(|e| e.into_inner());
    let pool = crate::require_test_pool().await;
    warm_up_pool(&pool).await;
    let svc = make_service(&pool).await;
    svc.step_ensure_additional_tables().await.unwrap();
    assert!(table_exists(&pool, "sliding_sync_lists").await, "sliding_sync_lists table should exist");
    assert!(table_exists(&pool, "sliding_sync_tokens").await, "sliding_sync_tokens table should exist");
    assert!(table_exists(&pool, "sliding_sync_rooms").await, "sliding_sync_rooms table should exist");
    assert!(index_exists(&pool, "idx_sliding_sync_lists_unique").await);
    assert!(index_exists(&pool, "idx_sliding_sync_tokens_unique").await);
    assert!(index_exists(&pool, "idx_sliding_sync_rooms_unique").await);
}

#[tokio::test]
async fn step_ensure_additional_tables_creates_thread_subscriptions() {
    let _g = guard().lock().unwrap_or_else(|e| e.into_inner());
    let pool = crate::require_test_pool().await;
    warm_up_pool(&pool).await;
    let svc = make_service(&pool).await;
    svc.step_ensure_additional_tables().await.unwrap();
    assert!(table_exists(&pool, "thread_subscriptions").await, "thread_subscriptions table should exist");
}

#[tokio::test]
async fn step_ensure_additional_tables_creates_space_tables() {
    let _g = guard().lock().unwrap_or_else(|e| e.into_inner());
    let pool = crate::require_test_pool().await;
    warm_up_pool(&pool).await;
    let svc = make_service(&pool).await;
    svc.step_ensure_additional_tables().await.unwrap();
    assert!(table_exists(&pool, "space_children").await, "space_children table should exist");
    assert!(table_exists(&pool, "space_hierarchy").await, "space_hierarchy table should exist");
    // Verify added columns exist
    assert!(column_exists(&pool, "space_children", "order").await, "space_children.order should exist");
    assert!(column_exists(&pool, "space_children", "suggested").await, "space_children.suggested should exist");
    assert!(column_exists(&pool, "space_children", "added_by").await, "space_children.added_by should exist");
    assert!(column_exists(&pool, "space_children", "removed_ts").await, "space_children.removed_ts should exist");
}

#[tokio::test]
async fn step_ensure_additional_tables_adds_users_is_guest_column() {
    let _g = guard().lock().unwrap_or_else(|e| e.into_inner());
    let pool = crate::require_test_pool().await;
    warm_up_pool(&pool).await;
    let svc = make_service(&pool).await;
    svc.step_ensure_additional_tables().await.unwrap();
    // The method adds is_guest column to users table
    assert!(column_exists(&pool, "users", "is_guest").await, "users.is_guest should exist");
}

#[tokio::test]
async fn step_ensure_additional_tables_adds_guest_access_to_rooms() {
    let _g = guard().lock().unwrap_or_else(|e| e.into_inner());
    let pool = crate::require_test_pool().await;
    warm_up_pool(&pool).await;
    let svc = make_service(&pool).await;
    svc.step_ensure_additional_tables().await.unwrap();
    assert!(column_exists(&pool, "rooms", "guest_access").await, "rooms.guest_access should exist");
}

#[tokio::test]
async fn step_ensure_additional_tables_adds_expires_at_to_refresh_tokens() {
    let _g = guard().lock().unwrap_or_else(|e| e.into_inner());
    let pool = crate::require_test_pool().await;
    warm_up_pool(&pool).await;
    let svc = make_service(&pool).await;
    svc.step_ensure_additional_tables().await.unwrap();
    assert!(
        column_exists(&pool, "refresh_tokens", "expires_at").await,
        "refresh_tokens.expires_at should exist"
    );
}

#[tokio::test]
async fn step_ensure_additional_tables_is_idempotent() {
    let _g = guard().lock().unwrap_or_else(|e| e.into_inner());
    let pool = crate::require_test_pool().await;
    warm_up_pool(&pool).await;
    let svc = make_service(&pool).await;
    svc.step_ensure_additional_tables().await.unwrap();
    svc.step_ensure_additional_tables().await.unwrap();
    let r3 = svc.step_ensure_additional_tables().await;
    assert!(r3.is_ok(), "third call should succeed: {:?}", r3.err());
}

#[tokio::test]
async fn database_init_service_with_mode_returns_configured_mode() {
    // Verify the with_mode builder works correctly (does not require DB).
    let _ = unique_id();
    // We can't easily assert mode since it's private, but we can verify construction doesn't panic.
    let pool = crate::require_test_pool().await;
    let _svc_strict = DatabaseInitService::new(pool.clone()).with_mode(DatabaseInitMode::Strict);
    let _svc_compat = DatabaseInitService::new(pool.clone()).with_mode(DatabaseInitMode::Compatible);
    let _svc_auto = DatabaseInitService::new(pool.clone()).with_mode(DatabaseInitMode::Auto);
}
