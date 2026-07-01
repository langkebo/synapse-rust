//! Integration tests for `SecureBackupService` (synapse-e2ee/src/secure_backup/service.rs).
//! Covers every public service method:
//!   - create_backup (passphrase-based) / create_backup_with_data (custom algorithm + auth_data)
//!   - store_session_keys (empty / insert / upsert / count increment)
//!   - restore_backup (round trip / wrong passphrase / room filter / nonexistent)
//!   - verify_passphrase (valid / invalid)
//!   - get_backup_info (found / not found)
//!   - list_backups (multiple / empty)
//!   - delete_backup (removes backup + session keys)
//!
//! Note: `secure_key_backups` has FK to `users(user_id) ON DELETE CASCADE` and
//! `secure_backup_session_keys` has FK to `secure_key_backups(user_id, backup_id)
//! ON DELETE CASCADE`; tests insert a test user first.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex, OnceLock};
use synapse_rust::e2ee::secure_backup::models::SessionKeyData;
use synapse_rust::e2ee::SecureBackupService;

static TEST_COUNTER: AtomicU64 = AtomicU64::new(1);

fn unique_id() -> u64 {
    TEST_COUNTER.fetch_add(1, Ordering::SeqCst)
}

fn sb_guard() -> &'static Mutex<()> {
    static GUARD: OnceLock<Mutex<()>> = OnceLock::new();
    GUARD.get_or_init(|| Mutex::new(()))
}

/// Warm up the shared pool on the current tokio runtime (the test pool can be
/// created on a different runtime; first query on a fresh runtime may fail).
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

/// Clear secure_backup tables. Order matters: child tables first due to FK.
async fn setup(pool: &Arc<sqlx::PgPool>) {
    warm_up_pool(pool).await;
    // Child first (FK to secure_key_backups)
    sqlx::query("DELETE FROM secure_backup_session_keys").execute(pool.as_ref()).await.ok();
    sqlx::query("DELETE FROM secure_key_backups").execute(pool.as_ref()).await.ok();
}

async fn teardown(pool: &sqlx::PgPool) {
    sqlx::query("DELETE FROM secure_backup_session_keys").execute(pool).await.ok();
    sqlx::query("DELETE FROM secure_key_backups").execute(pool).await.ok();
}

fn new_service(pool: &Arc<sqlx::PgPool>) -> SecureBackupService {
    SecureBackupService::new(pool)
}

fn unique_user_id() -> String {
    format!("@sbuser_{}:localhost", unique_id())
}

fn unique_room_id() -> String {
    format!("!sbroom_{}:localhost", unique_id())
}

fn unique_session_id() -> String {
    format!("sb_session_{}", unique_id())
}

/// Insert a row into `users` to satisfy FK constraints. Idempotent.
async fn ensure_user(pool: &sqlx::PgPool, user_id: &str) {
    let now = chrono::Utc::now().timestamp_millis();
    sqlx::query("INSERT INTO users (user_id, username, created_ts) VALUES ($1, $2, $3) ON CONFLICT (user_id) DO NOTHING")
        .bind(user_id)
        .bind(user_id.trim_start_matches('@').replace(':', "_"))
        .bind(now)
        .execute(pool)
        .await
        .ok();
}

async fn cleanup_user(pool: &sqlx::PgPool, user_id: &str) {
    // ON DELETE CASCADE on secure_backup_session_keys and secure_key_backups
    // will remove dependent rows when the user is deleted.
    sqlx::query("DELETE FROM users WHERE user_id = $1").bind(user_id).execute(pool).await.ok();
}

fn make_session_key(room_id: &str, session_id: &str, key_data: &str) -> SessionKeyData {
    SessionKeyData {
        room_id: room_id.to_string(),
        session_id: session_id.to_string(),
        first_message_index: 0,
        forwarded_count: 0,
        is_verified: true,
        session_key: key_data.to_string(),
    }
}

// ===========================================================================
// create_backup
// ===========================================================================

#[allow(clippy::await_holding_lock)]
#[tokio::test]
async fn test_create_backup_creates_backup_row() {
    let _guard = sb_guard().lock().unwrap();
    let pool = crate::require_test_pool().await;
    setup(&pool).await;
    let service = new_service(&pool);

    let user_id = unique_user_id();
    ensure_user(pool.as_ref(), &user_id).await;

    let response = service.create_backup(&user_id, "correct horse battery staple").await.unwrap();

    assert!(!response.backup_id.is_empty());
    assert!(!response.version.is_empty());
    assert_eq!(response.algorithm, "m.megolm_backup.v1.secure");
    assert_eq!(response.key_count, 0);
    assert!(!response.auth_data.salt.is_empty());
    assert_eq!(response.auth_data.iterations, 500000);
    assert_eq!(response.auth_data.backup_id, response.backup_id);

    let count: i64 =
        sqlx::query_scalar("SELECT COUNT(*) FROM secure_key_backups WHERE user_id = $1 AND backup_id = $2")
            .bind(&user_id)
            .bind(&response.backup_id)
            .fetch_one(pool.as_ref())
            .await
            .unwrap();
    assert_eq!(count, 1);

    cleanup_user(pool.as_ref(), &user_id).await;
    teardown(pool.as_ref()).await;
}

#[allow(clippy::await_holding_lock)]
#[tokio::test]
async fn test_create_backup_returns_distinct_backup_ids() {
    let _guard = sb_guard().lock().unwrap();
    let pool = crate::require_test_pool().await;
    setup(&pool).await;
    let service = new_service(&pool);

    let user_id = unique_user_id();
    ensure_user(pool.as_ref(), &user_id).await;

    let r1 = service.create_backup(&user_id, "passphrase1").await.unwrap();
    let r2 = service.create_backup(&user_id, "passphrase2").await.unwrap();

    assert_ne!(r1.backup_id, r2.backup_id, "Each create_backup call should produce a unique backup_id");

    cleanup_user(pool.as_ref(), &user_id).await;
    teardown(pool.as_ref()).await;
}

// ===========================================================================
// create_backup_with_data
// ===========================================================================

#[allow(clippy::await_holding_lock)]
#[tokio::test]
async fn test_create_backup_with_data_custom_algorithm() {
    let _guard = sb_guard().lock().unwrap();
    let pool = crate::require_test_pool().await;
    setup(&pool).await;
    let service = new_service(&pool);

    let user_id = unique_user_id();
    ensure_user(pool.as_ref(), &user_id).await;

    let auth_data = serde_json::json!({
        "salt": "custom_salt_value",
        "iterations": 100000,
        "public_key": "base64_pubkey_here"
    });
    let response = service.create_backup_with_data(&user_id, "m.megolm_backup.v1.curve25519-aes-sha2", &auth_data).await.unwrap();

    assert_eq!(response.algorithm, "m.megolm_backup.v1.curve25519-aes-sha2");
    assert_eq!(response.auth_data.salt, "custom_salt_value");
    assert_eq!(response.auth_data.iterations, 100000);
    assert_eq!(response.auth_data.public_key.as_deref(), Some("base64_pubkey_here"));
    assert_eq!(response.key_count, 0);

    cleanup_user(pool.as_ref(), &user_id).await;
    teardown(pool.as_ref()).await;
}

#[allow(clippy::await_holding_lock)]
#[tokio::test]
async fn test_create_backup_with_data_defaults_missing_fields() {
    let _guard = sb_guard().lock().unwrap();
    let pool = crate::require_test_pool().await;
    setup(&pool).await;
    let service = new_service(&pool);

    let user_id = unique_user_id();
    ensure_user(pool.as_ref(), &user_id).await;

    // Empty auth_data object — service should default salt="" iterations=0 public_key=None
    let auth_data = serde_json::json!({});
    let response = service.create_backup_with_data(&user_id, "m.megolm_backup.v1", &auth_data).await.unwrap();

    assert_eq!(response.auth_data.salt, "");
    assert_eq!(response.auth_data.iterations, 0);
    assert!(response.auth_data.public_key.is_none());

    cleanup_user(pool.as_ref(), &user_id).await;
    teardown(pool.as_ref()).await;
}

// ===========================================================================
// store_session_keys
// ===========================================================================

#[allow(clippy::await_holding_lock)]
#[tokio::test]
async fn test_store_session_keys_empty_returns_zero() {
    let _guard = sb_guard().lock().unwrap();
    let pool = crate::require_test_pool().await;
    setup(&pool).await;
    let service = new_service(&pool);

    let user_id = unique_user_id();
    ensure_user(pool.as_ref(), &user_id).await;
    let response = service.create_backup(&user_id, "passphrase").await.unwrap();

    let count = service.store_session_keys(&user_id, &response.backup_id, "passphrase", vec![]).await.unwrap();
    assert_eq!(count, 0);

    cleanup_user(pool.as_ref(), &user_id).await;
    teardown(pool.as_ref()).await;
}

#[allow(clippy::await_holding_lock)]
#[tokio::test]
async fn test_store_session_keys_inserts_encrypted_keys() {
    let _guard = sb_guard().lock().unwrap();
    let pool = crate::require_test_pool().await;
    setup(&pool).await;
    let service = new_service(&pool);

    let user_id = unique_user_id();
    ensure_user(pool.as_ref(), &user_id).await;
    let response = service.create_backup(&user_id, "passphrase").await.unwrap();

    let room1 = unique_room_id();
    let room2 = unique_room_id();
    let keys = vec![
        make_session_key(&room1, &unique_session_id(), "plaintext_key_1"),
        make_session_key(&room2, &unique_session_id(), "plaintext_key_2"),
    ];

    let count = service.store_session_keys(&user_id, &response.backup_id, "passphrase", keys).await.unwrap();
    assert_eq!(count, 2);

    let db_count: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM secure_backup_session_keys WHERE user_id = $1 AND backup_id = $2",
    )
    .bind(&user_id)
    .bind(&response.backup_id)
    .fetch_one(pool.as_ref())
    .await
    .unwrap();
    assert_eq!(db_count, 2);

    // Verify backup key_count was incremented
    let key_count: i64 = sqlx::query_scalar("SELECT key_count FROM secure_key_backups WHERE user_id = $1 AND backup_id = $2")
        .bind(&user_id)
        .bind(&response.backup_id)
        .fetch_one(pool.as_ref())
        .await
        .unwrap();
    assert_eq!(key_count, 2);

    cleanup_user(pool.as_ref(), &user_id).await;
    teardown(pool.as_ref()).await;
}

#[allow(clippy::await_holding_lock)]
#[tokio::test]
async fn test_store_session_keys_upserts_existing_key() {
    let _guard = sb_guard().lock().unwrap();
    let pool = crate::require_test_pool().await;
    setup(&pool).await;
    let service = new_service(&pool);

    let user_id = unique_user_id();
    ensure_user(pool.as_ref(), &user_id).await;
    let response = service.create_backup(&user_id, "passphrase").await.unwrap();

    let room = unique_room_id();
    let session = unique_session_id();

    // First store
    let keys1 = vec![make_session_key(&room, &session, "original_key")];
    let count1 = service.store_session_keys(&user_id, &response.backup_id, "passphrase", keys1).await.unwrap();
    assert_eq!(count1, 1);

    // Upsert with new key
    let keys2 = vec![make_session_key(&room, &session, "updated_key")];
    let count2 = service.store_session_keys(&user_id, &response.backup_id, "passphrase", keys2).await.unwrap();
    assert_eq!(count2, 1);

    // Should still only have 1 row (ON CONFLICT upsert)
    let db_count: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM secure_backup_session_keys WHERE user_id = $1 AND backup_id = $2 AND room_id = $3 AND session_id = $4",
    )
    .bind(&user_id)
    .bind(&response.backup_id)
    .bind(&room)
    .bind(&session)
    .fetch_one(pool.as_ref())
    .await
    .unwrap();
    assert_eq!(db_count, 1, "store_session_keys should upsert, not insert duplicates");

    cleanup_user(pool.as_ref(), &user_id).await;
    teardown(pool.as_ref()).await;
}

#[allow(clippy::await_holding_lock)]
#[tokio::test]
async fn test_store_session_keys_nonexistent_backup_returns_error() {
    let _guard = sb_guard().lock().unwrap();
    let pool = crate::require_test_pool().await;
    setup(&pool).await;
    let service = new_service(&pool);

    let user_id = unique_user_id();
    ensure_user(pool.as_ref(), &user_id).await;

    let result = service
        .store_session_keys(&user_id, "nonexistent_backup_id", "passphrase", vec![make_session_key("!room:localhost", "session", "key")])
        .await;
    assert!(result.is_err(), "store_session_keys should fail when backup does not exist");

    cleanup_user(pool.as_ref(), &user_id).await;
    teardown(pool.as_ref()).await;
}

// ===========================================================================
// restore_backup
// ===========================================================================

#[allow(clippy::await_holding_lock)]
#[tokio::test]
async fn test_restore_backup_no_keys_returns_zero() {
    let _guard = sb_guard().lock().unwrap();
    let pool = crate::require_test_pool().await;
    setup(&pool).await;
    let service = new_service(&pool);

    let user_id = unique_user_id();
    ensure_user(pool.as_ref(), &user_id).await;
    let response = service.create_backup(&user_id, "passphrase").await.unwrap();

    let result = service.restore_backup(&user_id, &response.backup_id, "passphrase", None).await.unwrap();
    assert_eq!(result.recovered_keys, 0);
    assert_eq!(result.total_keys, 0);

    cleanup_user(pool.as_ref(), &user_id).await;
    teardown(pool.as_ref()).await;
}

#[allow(clippy::await_holding_lock)]
#[tokio::test]
async fn test_restore_backup_with_keys_decrypts_correctly() {
    let _guard = sb_guard().lock().unwrap();
    let pool = crate::require_test_pool().await;
    setup(&pool).await;
    let service = new_service(&pool);

    let user_id = unique_user_id();
    ensure_user(pool.as_ref(), &user_id).await;
    let response = service.create_backup(&user_id, "correct passphrase").await.unwrap();

    let room1 = unique_room_id();
    let room2 = unique_room_id();
    let keys = vec![
        make_session_key(&room1, &unique_session_id(), "session_key_data_1"),
        make_session_key(&room2, &unique_session_id(), "session_key_data_2"),
        make_session_key(&room1, &unique_session_id(), "session_key_data_3"),
    ];
    service.store_session_keys(&user_id, &response.backup_id, "correct passphrase", keys).await.unwrap();

    let result = service.restore_backup(&user_id, &response.backup_id, "correct passphrase", None).await.unwrap();
    assert_eq!(result.recovered_keys, 3, "All 3 keys should decrypt successfully");
    assert_eq!(result.total_keys, 3);

    cleanup_user(pool.as_ref(), &user_id).await;
    teardown(pool.as_ref()).await;
}

#[allow(clippy::await_holding_lock)]
#[tokio::test]
async fn test_restore_backup_wrong_passphrase_returns_zero() {
    let _guard = sb_guard().lock().unwrap();
    let pool = crate::require_test_pool().await;
    setup(&pool).await;
    let service = new_service(&pool);

    let user_id = unique_user_id();
    ensure_user(pool.as_ref(), &user_id).await;
    let response = service.create_backup(&user_id, "correct passphrase").await.unwrap();

    let keys = vec![make_session_key(&unique_room_id(), &unique_session_id(), "session_key_data")];
    service.store_session_keys(&user_id, &response.backup_id, "correct passphrase", keys).await.unwrap();

    let result = service.restore_backup(&user_id, &response.backup_id, "wrong passphrase", None).await.unwrap();
    assert_eq!(result.recovered_keys, 0, "Decryption should fail with wrong passphrase");
    assert_eq!(result.total_keys, 1);

    cleanup_user(pool.as_ref(), &user_id).await;
    teardown(pool.as_ref()).await;
}

#[allow(clippy::await_holding_lock)]
#[tokio::test]
async fn test_restore_backup_filters_by_rooms() {
    let _guard = sb_guard().lock().unwrap();
    let pool = crate::require_test_pool().await;
    setup(&pool).await;
    let service = new_service(&pool);

    let user_id = unique_user_id();
    ensure_user(pool.as_ref(), &user_id).await;
    let response = service.create_backup(&user_id, "passphrase").await.unwrap();

    let room_a = unique_room_id();
    let room_b = unique_room_id();
    let room_c = unique_room_id();
    let keys = vec![
        make_session_key(&room_a, &unique_session_id(), "key_a"),
        make_session_key(&room_b, &unique_session_id(), "key_b"),
        make_session_key(&room_c, &unique_session_id(), "key_c"),
    ];
    service.store_session_keys(&user_id, &response.backup_id, "passphrase", keys).await.unwrap();

    // Only restore room_a and room_c
    let result = service
        .restore_backup(&user_id, &response.backup_id, "passphrase", Some(vec![room_a.clone(), room_c.clone()]))
        .await
        .unwrap();
    assert_eq!(result.recovered_keys, 2, "Should only restore keys for filtered rooms");
    assert_eq!(result.total_keys, 3, "total_keys should reflect all stored keys");

    cleanup_user(pool.as_ref(), &user_id).await;
    teardown(pool.as_ref()).await;
}

#[allow(clippy::await_holding_lock)]
#[tokio::test]
async fn test_restore_backup_nonexistent_returns_error() {
    let _guard = sb_guard().lock().unwrap();
    let pool = crate::require_test_pool().await;
    setup(&pool).await;
    let service = new_service(&pool);

    let user_id = unique_user_id();
    ensure_user(pool.as_ref(), &user_id).await;

    let result = service.restore_backup(&user_id, "nonexistent_backup_id", "passphrase", None).await;
    assert!(result.is_err(), "restore_backup should fail when backup does not exist");

    cleanup_user(pool.as_ref(), &user_id).await;
    teardown(pool.as_ref()).await;
}

// ===========================================================================
// verify_passphrase
// ===========================================================================

#[allow(clippy::await_holding_lock)]
#[tokio::test]
async fn test_verify_passphrase_valid_returns_true() {
    let _guard = sb_guard().lock().unwrap();
    let pool = crate::require_test_pool().await;
    setup(&pool).await;
    let service = new_service(&pool);

    let user_id = unique_user_id();
    ensure_user(pool.as_ref(), &user_id).await;
    let response = service.create_backup(&user_id, "valid passphrase").await.unwrap();

    // Store at least one key so verify_passphrase has something to decrypt
    let keys = vec![make_session_key(&unique_room_id(), &unique_session_id(), "key_data")];
    service.store_session_keys(&user_id, &response.backup_id, "valid passphrase", keys).await.unwrap();

    let is_valid = service.verify_passphrase(&user_id, &response.backup_id, "valid passphrase").await.unwrap();
    assert!(is_valid, "Correct passphrase should verify as true");

    cleanup_user(pool.as_ref(), &user_id).await;
    teardown(pool.as_ref()).await;
}

#[allow(clippy::await_holding_lock)]
#[tokio::test]
async fn test_verify_passphrase_invalid_returns_false() {
    let _guard = sb_guard().lock().unwrap();
    let pool = crate::require_test_pool().await;
    setup(&pool).await;
    let service = new_service(&pool);

    let user_id = unique_user_id();
    ensure_user(pool.as_ref(), &user_id).await;
    let response = service.create_backup(&user_id, "valid passphrase").await.unwrap();

    let keys = vec![make_session_key(&unique_room_id(), &unique_session_id(), "key_data")];
    service.store_session_keys(&user_id, &response.backup_id, "valid passphrase", keys).await.unwrap();

    let is_valid = service.verify_passphrase(&user_id, &response.backup_id, "wrong passphrase").await.unwrap();
    assert!(!is_valid, "Wrong passphrase should verify as false");

    cleanup_user(pool.as_ref(), &user_id).await;
    teardown(pool.as_ref()).await;
}

#[allow(clippy::await_holding_lock)]
#[tokio::test]
async fn test_verify_passphrase_no_keys_returns_false() {
    let _guard = sb_guard().lock().unwrap();
    let pool = crate::require_test_pool().await;
    setup(&pool).await;
    let service = new_service(&pool);

    let user_id = unique_user_id();
    ensure_user(pool.as_ref(), &user_id).await;
    let response = service.create_backup(&user_id, "passphrase").await.unwrap();

    // No keys stored — verify_passphrase returns false because recovered_keys == 0
    let is_valid = service.verify_passphrase(&user_id, &response.backup_id, "passphrase").await.unwrap();
    assert!(!is_valid, "verify_passphrase should return false when no keys exist to decrypt");

    cleanup_user(pool.as_ref(), &user_id).await;
    teardown(pool.as_ref()).await;
}

// ===========================================================================
// get_backup_info
// ===========================================================================

#[allow(clippy::await_holding_lock)]
#[tokio::test]
async fn test_get_backup_info_returns_backup() {
    let _guard = sb_guard().lock().unwrap();
    let pool = crate::require_test_pool().await;
    setup(&pool).await;
    let service = new_service(&pool);

    let user_id = unique_user_id();
    ensure_user(pool.as_ref(), &user_id).await;
    let created = service.create_backup(&user_id, "passphrase").await.unwrap();

    let info = service.get_backup_info(&user_id, &created.backup_id).await.unwrap().unwrap();
    assert_eq!(info.backup_id, created.backup_id);
    assert_eq!(info.version, created.version);
    assert_eq!(info.algorithm, created.algorithm);
    assert_eq!(info.key_count, 0);
    assert_eq!(info.auth_data.salt, created.auth_data.salt);

    cleanup_user(pool.as_ref(), &user_id).await;
    teardown(pool.as_ref()).await;
}

#[allow(clippy::await_holding_lock)]
#[tokio::test]
async fn test_get_backup_info_nonexistent_returns_none() {
    let _guard = sb_guard().lock().unwrap();
    let pool = crate::require_test_pool().await;
    setup(&pool).await;
    let service = new_service(&pool);

    let result = service.get_backup_info("@nobody:localhost", "nonexistent_backup_id").await.unwrap();
    assert!(result.is_none());

    teardown(pool.as_ref()).await;
}

// ===========================================================================
// list_backups
// ===========================================================================

#[allow(clippy::await_holding_lock)]
#[tokio::test]
async fn test_list_backups_returns_all_user_backups() {
    let _guard = sb_guard().lock().unwrap();
    let pool = crate::require_test_pool().await;
    setup(&pool).await;
    let service = new_service(&pool);

    let user_id = unique_user_id();
    ensure_user(pool.as_ref(), &user_id).await;

    let mut backup_ids = Vec::new();
    for i in 0..3 {
        // Small delay so created_ts differs between backups for ordering
        if i > 0 {
            tokio::time::sleep(std::time::Duration::from_millis(10)).await;
        }
        let r = service.create_backup(&user_id, &format!("passphrase_{i}")).await.unwrap();
        backup_ids.push(r.backup_id);
    }

    let backups = service.list_backups(&user_id).await.unwrap();
    assert_eq!(backups.len(), 3, "Should return all 3 backups for the user");

    let returned_ids: Vec<_> = backups.iter().map(|b| b.backup_id.clone()).collect();
    for id in &backup_ids {
        assert!(returned_ids.contains(id), "Backup {id} should be in list_backups result");
    }

    cleanup_user(pool.as_ref(), &user_id).await;
    teardown(pool.as_ref()).await;
}

#[allow(clippy::await_holding_lock)]
#[tokio::test]
async fn test_list_backups_empty_returns_empty_vec() {
    let _guard = sb_guard().lock().unwrap();
    let pool = crate::require_test_pool().await;
    setup(&pool).await;
    let service = new_service(&pool);

    let user_id = unique_user_id();
    ensure_user(pool.as_ref(), &user_id).await;

    let backups = service.list_backups(&user_id).await.unwrap();
    assert!(backups.is_empty(), "User with no backups should get empty list");

    cleanup_user(pool.as_ref(), &user_id).await;
    teardown(pool.as_ref()).await;
}

#[allow(clippy::await_holding_lock)]
#[tokio::test]
async fn test_list_backups_isolates_by_user() {
    let _guard = sb_guard().lock().unwrap();
    let pool = crate::require_test_pool().await;
    setup(&pool).await;
    let service = new_service(&pool);

    let user_a = unique_user_id();
    let user_b = unique_user_id();
    ensure_user(pool.as_ref(), &user_a).await;
    ensure_user(pool.as_ref(), &user_b).await;

    service.create_backup(&user_a, "pass_a").await.unwrap();
    service.create_backup(&user_b, "pass_b").await.unwrap();
    service.create_backup(&user_b, "pass_b2").await.unwrap();

    let a_backups = service.list_backups(&user_a).await.unwrap();
    let b_backups = service.list_backups(&user_b).await.unwrap();
    assert_eq!(a_backups.len(), 1, "User A should only see their own backup");
    assert_eq!(b_backups.len(), 2, "User B should only see their own 2 backups");

    cleanup_user(pool.as_ref(), &user_a).await;
    cleanup_user(pool.as_ref(), &user_b).await;
    teardown(pool.as_ref()).await;
}

// ===========================================================================
// delete_backup
// ===========================================================================

#[allow(clippy::await_holding_lock)]
#[tokio::test]
async fn test_delete_backup_removes_backup_and_keys() {
    let _guard = sb_guard().lock().unwrap();
    let pool = crate::require_test_pool().await;
    setup(&pool).await;
    let service = new_service(&pool);

    let user_id = unique_user_id();
    ensure_user(pool.as_ref(), &user_id).await;
    let response = service.create_backup(&user_id, "passphrase").await.unwrap();

    let keys = vec![
        make_session_key(&unique_room_id(), &unique_session_id(), "key1"),
        make_session_key(&unique_room_id(), &unique_session_id(), "key2"),
    ];
    service.store_session_keys(&user_id, &response.backup_id, "passphrase", keys).await.unwrap();

    service.delete_backup(&user_id, &response.backup_id).await.unwrap();

    let backup_count: i64 =
        sqlx::query_scalar("SELECT COUNT(*) FROM secure_key_backups WHERE user_id = $1 AND backup_id = $2")
            .bind(&user_id)
            .bind(&response.backup_id)
            .fetch_one(pool.as_ref())
            .await
            .unwrap();
    assert_eq!(backup_count, 0, "Backup should be deleted");

    let key_count: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM secure_backup_session_keys WHERE user_id = $1 AND backup_id = $2",
    )
    .bind(&user_id)
    .bind(&response.backup_id)
    .fetch_one(pool.as_ref())
    .await
    .unwrap();
    assert_eq!(key_count, 0, "Session keys should be cascade-deleted with backup");

    cleanup_user(pool.as_ref(), &user_id).await;
    teardown(pool.as_ref()).await;
}

#[allow(clippy::await_holding_lock)]
#[tokio::test]
async fn test_delete_backup_nonexistent_is_noop() {
    let _guard = sb_guard().lock().unwrap();
    let pool = crate::require_test_pool().await;
    setup(&pool).await;
    let service = new_service(&pool);

    let user_id = unique_user_id();
    ensure_user(pool.as_ref(), &user_id).await;

    // Deleting a nonexistent backup should succeed (idempotent)
    let result = service.delete_backup(&user_id, "nonexistent_backup_id").await;
    assert!(result.is_ok(), "delete_backup on nonexistent backup should be a no-op, not an error");

    cleanup_user(pool.as_ref(), &user_id).await;
    teardown(pool.as_ref()).await;
}

#[allow(clippy::await_holding_lock)]
#[tokio::test]
async fn test_delete_backup_then_get_returns_none() {
    let _guard = sb_guard().lock().unwrap();
    let pool = crate::require_test_pool().await;
    setup(&pool).await;
    let service = new_service(&pool);

    let user_id = unique_user_id();
    ensure_user(pool.as_ref(), &user_id).await;
    let response = service.create_backup(&user_id, "passphrase").await.unwrap();

    service.delete_backup(&user_id, &response.backup_id).await.unwrap();

    let info = service.get_backup_info(&user_id, &response.backup_id).await.unwrap();
    assert!(info.is_none(), "After delete, get_backup_info should return None");

    cleanup_user(pool.as_ref(), &user_id).await;
    teardown(pool.as_ref()).await;
}
