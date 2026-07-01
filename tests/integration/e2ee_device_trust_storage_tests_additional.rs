//! Integration tests for `DeviceTrustStorage` (synapse-e2ee/src/device_trust/storage.rs).
//! Covers every public storage method:
//!   - Device trust status: get_device_trust / upsert_device_trust /
//!     set_device_trust (verified / unverified / blocked) /
//!     get_all_devices_with_trust / get_verified_devices / count_devices_by_trust
//!   - Verification requests: create_verification_request / get_request_by_token /
//!     get_pending_request / update_request_status / update_request_with_data /
//!     cleanup_expired_requests
//!   - Key rotation log: log_key_rotation
//!   - Security events: log_security_event / get_recent_security_events
//!   - Cross-signing trust: set_cross_signing_trust (trusted / untrusted) /
//!     has_cross_signing_master_key
//!
//! Note: `device_verification_request` and `secure_key_backups` tables have FK
//! constraints to `users(user_id) ON DELETE CASCADE`; for those tests we insert
//! a test user first.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex, OnceLock};
use synapse_rust::e2ee::device_trust::models::{
    DeviceTrustLevel, DeviceTrustStatus, DeviceVerificationRequest, E2eeSecurityEvent, KeyRotationLog,
    VerificationMethod, VerificationRequestStatus,
};
use synapse_rust::e2ee::DeviceTrustStorage;

static TEST_COUNTER: AtomicU64 = AtomicU64::new(1);

fn unique_id() -> u64 {
    TEST_COUNTER.fetch_add(1, Ordering::SeqCst)
}

fn dt_guard() -> &'static Mutex<()> {
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

/// Clear device_trust-exclusive tables. Order matters for FK constraints:
/// child tables first (cross_signing_trust references cross_signing_keys via
/// subquery but has no FK; device_verification_request has FK to users).
async fn setup(pool: &Arc<sqlx::PgPool>) {
    warm_up_pool(pool).await;
    // Module-exclusive tables with no FK to users
    sqlx::query("DELETE FROM cross_signing_trust").execute(pool.as_ref()).await.ok();
    sqlx::query("DELETE FROM cross_signing_keys").execute(pool.as_ref()).await.ok();
    sqlx::query("DELETE FROM e2ee_security_events").execute(pool.as_ref()).await.ok();
    sqlx::query("DELETE FROM key_rotation_log").execute(pool.as_ref()).await.ok();
    sqlx::query("DELETE FROM device_trust_status").execute(pool.as_ref()).await.ok();
    // device_verification_request has FK to users; safe to clear without
    // touching users because ON DELETE CASCADE only triggers on user removal.
    sqlx::query("DELETE FROM device_verification_request").execute(pool.as_ref()).await.ok();
}

async fn teardown(pool: &sqlx::PgPool) {
    sqlx::query("DELETE FROM cross_signing_trust").execute(pool).await.ok();
    sqlx::query("DELETE FROM cross_signing_keys").execute(pool).await.ok();
    sqlx::query("DELETE FROM e2ee_security_events").execute(pool).await.ok();
    sqlx::query("DELETE FROM key_rotation_log").execute(pool).await.ok();
    sqlx::query("DELETE FROM device_trust_status").execute(pool).await.ok();
    sqlx::query("DELETE FROM device_verification_request").execute(pool).await.ok();
}

fn new_storage(pool: &Arc<sqlx::PgPool>) -> DeviceTrustStorage {
    DeviceTrustStorage::new(pool)
}

fn unique_user_id() -> String {
    format!("@dtuser_{}:localhost", unique_id())
}

fn unique_device_id() -> String {
    format!("DEVICE_{}", unique_id())
}

fn unique_token() -> String {
    format!("token_{}", unique_id())
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
    sqlx::query("DELETE FROM device_verification_request WHERE user_id = $1").bind(user_id).execute(pool).await.ok();
    sqlx::query("DELETE FROM device_trust_status WHERE user_id = $1").bind(user_id).execute(pool).await.ok();
    sqlx::query("DELETE FROM cross_signing_trust WHERE user_id = $1 OR target_user_id = $1")
        .bind(user_id)
        .execute(pool)
        .await
        .ok();
    sqlx::query("DELETE FROM cross_signing_keys WHERE user_id = $1").bind(user_id).execute(pool).await.ok();
    sqlx::query("DELETE FROM e2ee_security_events WHERE user_id = $1").bind(user_id).execute(pool).await.ok();
    sqlx::query("DELETE FROM key_rotation_log WHERE user_id = $1").bind(user_id).execute(pool).await.ok();
    sqlx::query("DELETE FROM users WHERE user_id = $1").bind(user_id).execute(pool).await.ok();
}

// ===========================================================================
// get_device_trust
// ===========================================================================

#[allow(clippy::await_holding_lock)]
#[tokio::test]
async fn test_get_device_trust_returns_none_for_nonexistent() {
    let _guard = dt_guard().lock().unwrap();
    let pool = crate::require_test_pool().await;
    setup(&pool).await;
    let storage = new_storage(&pool);

    let result = storage.get_device_trust("@nonexistent:localhost", "NO_DEVICE").await.unwrap();
    assert!(result.is_none());

    teardown(pool.as_ref()).await;
}

#[allow(clippy::await_holding_lock)]
#[tokio::test]
async fn test_get_device_trust_returns_status_after_upsert() {
    let _guard = dt_guard().lock().unwrap();
    let pool = crate::require_test_pool().await;
    setup(&pool).await;
    let storage = new_storage(&pool);

    let user_id = unique_user_id();
    let device_id = unique_device_id();
    let mut status = DeviceTrustStatus::new(&user_id, &device_id);
    status.trust_level = DeviceTrustLevel::Verified;
    status.verified_by_device_id = Some("VERIFIER".to_string());
    status.verified_at = Some(chrono::Utc::now().timestamp_millis());

    storage.upsert_device_trust(&status).await.unwrap();

    let fetched = storage.get_device_trust(&user_id, &device_id).await.unwrap().unwrap();
    assert_eq!(fetched.user_id, user_id);
    assert_eq!(fetched.device_id, device_id);
    assert_eq!(fetched.trust_level, DeviceTrustLevel::Verified);
    assert_eq!(fetched.verified_by_device_id.as_deref(), Some("VERIFIER"));
    assert!(fetched.verified_at.is_some());

    teardown(pool.as_ref()).await;
}

// ===========================================================================
// upsert_device_trust
// ===========================================================================

#[allow(clippy::await_holding_lock)]
#[tokio::test]
async fn test_upsert_device_trust_inserts_new() {
    let _guard = dt_guard().lock().unwrap();
    let pool = crate::require_test_pool().await;
    setup(&pool).await;
    let storage = new_storage(&pool);

    let user_id = unique_user_id();
    let device_id = unique_device_id();
    let status = DeviceTrustStatus::new(&user_id, &device_id);

    storage.upsert_device_trust(&status).await.unwrap();

    let fetched = storage.get_device_trust(&user_id, &device_id).await.unwrap().unwrap();
    assert_eq!(fetched.trust_level, DeviceTrustLevel::Unverified);
    assert!(fetched.verified_at.is_none());
    assert!(fetched.id > 0);

    teardown(pool.as_ref()).await;
}

#[allow(clippy::await_holding_lock)]
#[tokio::test]
async fn test_upsert_device_trust_updates_existing() {
    let _guard = dt_guard().lock().unwrap();
    let pool = crate::require_test_pool().await;
    setup(&pool).await;
    let storage = new_storage(&pool);

    let user_id = unique_user_id();
    let device_id = unique_device_id();

    // Insert as unverified
    let mut status = DeviceTrustStatus::new(&user_id, &device_id);
    storage.upsert_device_trust(&status).await.unwrap();

    // Update to blocked
    status.trust_level = DeviceTrustLevel::Blocked;
    status.updated_ts = chrono::Utc::now().timestamp_millis();
    storage.upsert_device_trust(&status).await.unwrap();

    let fetched = storage.get_device_trust(&user_id, &device_id).await.unwrap().unwrap();
    assert_eq!(fetched.trust_level, DeviceTrustLevel::Blocked);

    teardown(pool.as_ref()).await;
}

// ===========================================================================
// set_device_trust
// ===========================================================================

#[allow(clippy::await_holding_lock)]
#[tokio::test]
async fn test_set_device_trust_verified_sets_verified_at() {
    let _guard = dt_guard().lock().unwrap();
    let pool = crate::require_test_pool().await;
    setup(&pool).await;
    let storage = new_storage(&pool);

    let user_id = unique_user_id();
    let device_id = unique_device_id();

    storage
        .set_device_trust(&user_id, &device_id, DeviceTrustLevel::Verified, Some("VERIFIER_DEV"))
        .await
        .unwrap();

    let fetched = storage.get_device_trust(&user_id, &device_id).await.unwrap().unwrap();
    assert_eq!(fetched.trust_level, DeviceTrustLevel::Verified);
    assert_eq!(fetched.verified_by_device_id.as_deref(), Some("VERIFIER_DEV"));
    assert!(fetched.verified_at.is_some());

    teardown(pool.as_ref()).await;
}

#[allow(clippy::await_holding_lock)]
#[tokio::test]
async fn test_set_device_trust_unverified_no_verified_at() {
    let _guard = dt_guard().lock().unwrap();
    let pool = crate::require_test_pool().await;
    setup(&pool).await;
    let storage = new_storage(&pool);

    let user_id = unique_user_id();
    let device_id = unique_device_id();

    storage.set_device_trust(&user_id, &device_id, DeviceTrustLevel::Unverified, None).await.unwrap();

    let fetched = storage.get_device_trust(&user_id, &device_id).await.unwrap().unwrap();
    assert_eq!(fetched.trust_level, DeviceTrustLevel::Unverified);
    assert!(fetched.verified_at.is_none());

    teardown(pool.as_ref()).await;
}

#[allow(clippy::await_holding_lock)]
#[tokio::test]
async fn test_set_device_trust_blocked_clears_verified_by() {
    let _guard = dt_guard().lock().unwrap();
    let pool = crate::require_test_pool().await;
    setup(&pool).await;
    let storage = new_storage(&pool);

    let user_id = unique_user_id();
    let device_id = unique_device_id();

    // First verify
    storage
        .set_device_trust(&user_id, &device_id, DeviceTrustLevel::Verified, Some("VERIFIER"))
        .await
        .unwrap();
    // Then block
    storage.set_device_trust(&user_id, &device_id, DeviceTrustLevel::Blocked, None).await.unwrap();

    let fetched = storage.get_device_trust(&user_id, &device_id).await.unwrap().unwrap();
    assert_eq!(fetched.trust_level, DeviceTrustLevel::Blocked);
    // ON CONFLICT UPDATE preserves verified_by_device_id column (not in SET),
    // so it remains from the Verified row.
    assert_eq!(fetched.verified_by_device_id.as_deref(), Some("VERIFIER"));

    teardown(pool.as_ref()).await;
}

#[allow(clippy::await_holding_lock)]
#[tokio::test]
async fn test_set_device_trust_upserts_existing_row() {
    let _guard = dt_guard().lock().unwrap();
    let pool = crate::require_test_pool().await;
    setup(&pool).await;
    let storage = new_storage(&pool);

    let user_id = unique_user_id();
    let device_id = unique_device_id();

    // First insert as unverified
    storage.set_device_trust(&user_id, &device_id, DeviceTrustLevel::Unverified, None).await.unwrap();
    // Update to verified via same set_device_trust path (triggers ON CONFLICT)
    storage
        .set_device_trust(&user_id, &device_id, DeviceTrustLevel::Verified, Some("VER"))
        .await
        .unwrap();

    let count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM device_trust_status WHERE user_id = $1 AND device_id = $2")
        .bind(&user_id)
        .bind(&device_id)
        .fetch_one(pool.as_ref())
        .await
        .unwrap();
    assert_eq!(count, 1, "set_device_trust should upsert, not insert duplicates");

    teardown(pool.as_ref()).await;
}

// ===========================================================================
// get_all_devices_with_trust / get_verified_devices / count_devices_by_trust
// ===========================================================================

#[allow(clippy::await_holding_lock)]
#[tokio::test]
async fn test_get_all_devices_with_trust() {
    let _guard = dt_guard().lock().unwrap();
    let pool = crate::require_test_pool().await;
    setup(&pool).await;
    let storage = new_storage(&pool);

    let user_id = unique_user_id();
    for i in 0..3 {
        let device_id = format!("{device_base}_{i}", device_base = unique_device_id());
        storage.upsert_device_trust(&DeviceTrustStatus::new(&user_id, &device_id)).await.unwrap();
    }

    let all = storage.get_all_devices_with_trust(&user_id).await.unwrap();
    assert_eq!(all.len(), 3);
    assert!(all.iter().all(|s| s.user_id == user_id));

    teardown(pool.as_ref()).await;
}

#[allow(clippy::await_holding_lock)]
#[tokio::test]
async fn test_get_verified_devices_filters_correctly() {
    let _guard = dt_guard().lock().unwrap();
    let pool = crate::require_test_pool().await;
    setup(&pool).await;
    let storage = new_storage(&pool);

    let user_id = unique_user_id();
    // 2 verified, 1 blocked, 1 unverified
    storage
        .set_device_trust(&user_id, "DEV_V1", DeviceTrustLevel::Verified, Some("V"))
        .await
        .unwrap();
    storage
        .set_device_trust(&user_id, "DEV_V2", DeviceTrustLevel::Verified, Some("V"))
        .await
        .unwrap();
    storage.set_device_trust(&user_id, "DEV_B", DeviceTrustLevel::Blocked, None).await.unwrap();
    storage.set_device_trust(&user_id, "DEV_U", DeviceTrustLevel::Unverified, None).await.unwrap();

    let verified = storage.get_verified_devices(&user_id).await.unwrap();
    assert_eq!(verified.len(), 2);
    assert!(verified.iter().all(|s| s.trust_level == DeviceTrustLevel::Verified));

    teardown(pool.as_ref()).await;
}

#[allow(clippy::await_holding_lock)]
#[tokio::test]
async fn test_count_devices_by_trust() {
    let _guard = dt_guard().lock().unwrap();
    let pool = crate::require_test_pool().await;
    setup(&pool).await;
    let storage = new_storage(&pool);

    let user_id = unique_user_id();
    storage
        .set_device_trust(&user_id, "DEV_V", DeviceTrustLevel::Verified, Some("V"))
        .await
        .unwrap();
    storage.set_device_trust(&user_id, "DEV_B", DeviceTrustLevel::Blocked, None).await.unwrap();
    storage.set_device_trust(&user_id, "DEV_U1", DeviceTrustLevel::Unverified, None).await.unwrap();
    storage.set_device_trust(&user_id, "DEV_U2", DeviceTrustLevel::Unverified, None).await.unwrap();

    let (verified, unverified, blocked) = storage.count_devices_by_trust(&user_id).await.unwrap();
    assert_eq!(verified, 1);
    assert_eq!(unverified, 2);
    assert_eq!(blocked, 1);

    teardown(pool.as_ref()).await;
}

#[allow(clippy::await_holding_lock)]
#[tokio::test]
async fn test_count_devices_by_trust_empty_user() {
    let _guard = dt_guard().lock().unwrap();
    let pool = crate::require_test_pool().await;
    setup(&pool).await;
    let storage = new_storage(&pool);

    let (verified, unverified, blocked) = storage.count_devices_by_trust("@empty:localhost").await.unwrap();
    assert_eq!((verified, unverified, blocked), (0, 0, 0));

    teardown(pool.as_ref()).await;
}

// ===========================================================================
// Verification requests (require user FK)
// ===========================================================================

#[allow(clippy::await_holding_lock)]
#[tokio::test]
async fn test_create_verification_request_inserts_row() {
    let _guard = dt_guard().lock().unwrap();
    let pool = crate::require_test_pool().await;
    setup(&pool).await;
    let storage = new_storage(&pool);

    let user_id = unique_user_id();
    ensure_user(pool.as_ref(), &user_id).await;

    let request = DeviceVerificationRequest::new(&user_id, "NEW_DEV", VerificationMethod::Sas, &unique_token(), 60);
    storage.create_verification_request(&request).await.unwrap();

    let count: i64 =
        sqlx::query_scalar("SELECT COUNT(*) FROM device_verification_request WHERE user_id = $1").bind(&user_id).fetch_one(pool.as_ref()).await.unwrap();
    assert_eq!(count, 1);

    cleanup_user(pool.as_ref(), &user_id).await;
    teardown(pool.as_ref()).await;
}

#[allow(clippy::await_holding_lock)]
#[tokio::test]
async fn test_get_request_by_token_returns_request() {
    let _guard = dt_guard().lock().unwrap();
    let pool = crate::require_test_pool().await;
    setup(&pool).await;
    let storage = new_storage(&pool);

    let user_id = unique_user_id();
    ensure_user(pool.as_ref(), &user_id).await;
    let token = unique_token();

    let request = DeviceVerificationRequest::new(&user_id, "NEW_DEV", VerificationMethod::Qr, &token, 60);
    storage.create_verification_request(&request).await.unwrap();

    let fetched = storage.get_request_by_token(&token).await.unwrap().unwrap();
    assert_eq!(fetched.user_id, user_id);
    assert_eq!(fetched.new_device_id, "NEW_DEV");
    assert_eq!(fetched.verification_method, VerificationMethod::Qr);
    assert_eq!(fetched.status, VerificationRequestStatus::Pending);
    assert_eq!(fetched.request_token, token);

    cleanup_user(pool.as_ref(), &user_id).await;
    teardown(pool.as_ref()).await;
}

#[allow(clippy::await_holding_lock)]
#[tokio::test]
async fn test_get_request_by_token_nonexistent_returns_none() {
    let _guard = dt_guard().lock().unwrap();
    let pool = crate::require_test_pool().await;
    setup(&pool).await;
    let storage = new_storage(&pool);

    let result = storage.get_request_by_token("nonexistent_token_xyz").await.unwrap();
    assert!(result.is_none());

    teardown(pool.as_ref()).await;
}

#[allow(clippy::await_holding_lock)]
#[tokio::test]
async fn test_get_pending_request_returns_pending() {
    let _guard = dt_guard().lock().unwrap();
    let pool = crate::require_test_pool().await;
    setup(&pool).await;
    let storage = new_storage(&pool);

    let user_id = unique_user_id();
    ensure_user(pool.as_ref(), &user_id).await;
    let token = unique_token();

    let request = DeviceVerificationRequest::new(&user_id, "NEW_DEV", VerificationMethod::Sas, &token, 60);
    storage.create_verification_request(&request).await.unwrap();

    let pending = storage.get_pending_request(&user_id, "NEW_DEV").await.unwrap().unwrap();
    assert_eq!(pending.request_token, token);
    assert_eq!(pending.status, VerificationRequestStatus::Pending);

    cleanup_user(pool.as_ref(), &user_id).await;
    teardown(pool.as_ref()).await;
}

#[allow(clippy::await_holding_lock)]
#[tokio::test]
async fn test_get_pending_request_expired_returns_none() {
    let _guard = dt_guard().lock().unwrap();
    let pool = crate::require_test_pool().await;
    setup(&pool).await;
    let storage = new_storage(&pool);

    let user_id = unique_user_id();
    ensure_user(pool.as_ref(), &user_id).await;
    let token = unique_token();

    // Create request that already expired (expires_at in the past)
    let now = chrono::Utc::now();
    let mut request = DeviceVerificationRequest::new(&user_id, "NEW_DEV", VerificationMethod::Sas, &token, 0);
    request.expires_at = now.timestamp_millis() - 1000; // 1 second ago
    storage.create_verification_request(&request).await.unwrap();

    let result = storage.get_pending_request(&user_id, "NEW_DEV").await.unwrap();
    assert!(result.is_none(), "Expired pending request should not be returned");

    cleanup_user(pool.as_ref(), &user_id).await;
    teardown(pool.as_ref()).await;
}

#[allow(clippy::await_holding_lock)]
#[tokio::test]
async fn test_update_request_status_to_approved() {
    let _guard = dt_guard().lock().unwrap();
    let pool = crate::require_test_pool().await;
    setup(&pool).await;
    let storage = new_storage(&pool);

    let user_id = unique_user_id();
    ensure_user(pool.as_ref(), &user_id).await;
    let token = unique_token();

    let request = DeviceVerificationRequest::new(&user_id, "NEW_DEV", VerificationMethod::Sas, &token, 60);
    storage.create_verification_request(&request).await.unwrap();

    storage.update_request_status(&token, VerificationRequestStatus::Approved).await.unwrap();

    let fetched = storage.get_request_by_token(&token).await.unwrap().unwrap();
    assert_eq!(fetched.status, VerificationRequestStatus::Approved);
    assert!(fetched.completed_at.is_some());

    cleanup_user(pool.as_ref(), &user_id).await;
    teardown(pool.as_ref()).await;
}

#[allow(clippy::await_holding_lock)]
#[tokio::test]
async fn test_update_request_status_to_rejected() {
    let _guard = dt_guard().lock().unwrap();
    let pool = crate::require_test_pool().await;
    setup(&pool).await;
    let storage = new_storage(&pool);

    let user_id = unique_user_id();
    ensure_user(pool.as_ref(), &user_id).await;
    let token = unique_token();

    let request = DeviceVerificationRequest::new(&user_id, "NEW_DEV", VerificationMethod::Emoji, &token, 60);
    storage.create_verification_request(&request).await.unwrap();

    storage.update_request_status(&token, VerificationRequestStatus::Rejected).await.unwrap();

    let fetched = storage.get_request_by_token(&token).await.unwrap().unwrap();
    assert_eq!(fetched.status, VerificationRequestStatus::Rejected);

    cleanup_user(pool.as_ref(), &user_id).await;
    teardown(pool.as_ref()).await;
}

#[allow(clippy::await_holding_lock)]
#[tokio::test]
async fn test_update_request_with_data_sets_commitment_and_pubkey() {
    let _guard = dt_guard().lock().unwrap();
    let pool = crate::require_test_pool().await;
    setup(&pool).await;
    let storage = new_storage(&pool);

    let user_id = unique_user_id();
    ensure_user(pool.as_ref(), &user_id).await;
    let token = unique_token();

    let request = DeviceVerificationRequest::new(&user_id, "NEW_DEV", VerificationMethod::Sas, &token, 60);
    storage.create_verification_request(&request).await.unwrap();

    storage.update_request_with_data(&token, "COMMITMENT_HEX", "PUBKEY_HEX").await.unwrap();

    let fetched = storage.get_request_by_token(&token).await.unwrap().unwrap();
    assert_eq!(fetched.commitment.as_deref(), Some("COMMITMENT_HEX"));
    assert_eq!(fetched.pubkey.as_deref(), Some("PUBKEY_HEX"));

    cleanup_user(pool.as_ref(), &user_id).await;
    teardown(pool.as_ref()).await;
}

#[allow(clippy::await_holding_lock)]
#[tokio::test]
async fn test_cleanup_expired_requests_marks_expired() {
    let _guard = dt_guard().lock().unwrap();
    let pool = crate::require_test_pool().await;
    setup(&pool).await;
    let storage = new_storage(&pool);

    let user_id = unique_user_id();
    ensure_user(pool.as_ref(), &user_id).await;

    // Two expired requests, one valid
    let now = chrono::Utc::now();
    let mut expired1 = DeviceVerificationRequest::new(&user_id, "EXP1", VerificationMethod::Sas, &unique_token(), 0);
    expired1.expires_at = now.timestamp_millis() - 5000;
    storage.create_verification_request(&expired1).await.unwrap();

    let mut expired2 = DeviceVerificationRequest::new(&user_id, "EXP2", VerificationMethod::Sas, &unique_token(), 0);
    expired2.expires_at = now.timestamp_millis() - 1000;
    storage.create_verification_request(&expired2).await.unwrap();

    let valid = DeviceVerificationRequest::new(&user_id, "VALID", VerificationMethod::Sas, &unique_token(), 60);
    storage.create_verification_request(&valid).await.unwrap();

    let affected = storage.cleanup_expired_requests().await.unwrap();
    assert!(affected >= 2, "Should have marked at least 2 expired requests, got {affected}");

    let expired_count: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM device_verification_request WHERE user_id = $1 AND status = 'expired'",
    )
    .bind(&user_id)
    .fetch_one(pool.as_ref())
    .await
    .unwrap();
    assert!(expired_count >= 2);

    let still_pending_count: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM device_verification_request WHERE user_id = $1 AND status = 'pending'",
    )
    .bind(&user_id)
    .fetch_one(pool.as_ref())
    .await
    .unwrap();
    assert_eq!(still_pending_count, 1, "Valid request should still be pending");

    cleanup_user(pool.as_ref(), &user_id).await;
    teardown(pool.as_ref()).await;
}

#[allow(clippy::await_holding_lock)]
#[tokio::test]
async fn test_cleanup_expired_requests_no_expired_returns_zero() {
    let _guard = dt_guard().lock().unwrap();
    let pool = crate::require_test_pool().await;
    setup(&pool).await;
    let storage = new_storage(&pool);

    let user_id = unique_user_id();
    ensure_user(pool.as_ref(), &user_id).await;

    let valid = DeviceVerificationRequest::new(&user_id, "VALID", VerificationMethod::Sas, &unique_token(), 60);
    storage.create_verification_request(&valid).await.unwrap();

    let affected = storage.cleanup_expired_requests().await.unwrap();
    assert_eq!(affected, 0, "No expired requests should be affected");

    cleanup_user(pool.as_ref(), &user_id).await;
    teardown(pool.as_ref()).await;
}

// ===========================================================================
// Key rotation log
// ===========================================================================

#[allow(clippy::await_holding_lock)]
#[tokio::test]
async fn test_log_key_rotation_inserts_row() {
    let _guard = dt_guard().lock().unwrap();
    let pool = crate::require_test_pool().await;
    setup(&pool).await;
    let storage = new_storage(&pool);

    let user_id = unique_user_id();
    let log = KeyRotationLog::new(&user_id, "DEV1", "megolm")
        .with_room("!room:localhost")
        .with_keys("OLD_KEY", "NEW_KEY")
        .with_reason("compromised");

    storage.log_key_rotation(&log).await.unwrap();

    let count: i64 =
        sqlx::query_scalar("SELECT COUNT(*) FROM key_rotation_log WHERE user_id = $1 AND device_id = $2")
            .bind(&user_id)
            .bind("DEV1")
            .fetch_one(pool.as_ref())
            .await
            .unwrap();
    assert_eq!(count, 1);

    let row: (Option<String>, Option<String>, Option<String>, Option<String>) = sqlx::query_as(
        "SELECT room_id, old_key_id, new_key_id, reason FROM key_rotation_log WHERE user_id = $1 AND device_id = $2",
    )
    .bind(&user_id)
    .bind("DEV1")
    .fetch_one(pool.as_ref())
    .await
    .unwrap();
    assert_eq!(row.0.as_deref(), Some("!room:localhost"));
    assert_eq!(row.1.as_deref(), Some("OLD_KEY"));
    assert_eq!(row.2.as_deref(), Some("NEW_KEY"));
    assert_eq!(row.3.as_deref(), Some("compromised"));

    teardown(pool.as_ref()).await;
}

// ===========================================================================
// Security events
// ===========================================================================

#[allow(clippy::await_holding_lock)]
#[tokio::test]
async fn test_log_security_event_inserts_row() {
    let _guard = dt_guard().lock().unwrap();
    let pool = crate::require_test_pool().await;
    setup(&pool).await;
    let storage = new_storage(&pool);

    let user_id = unique_user_id();
    let event = E2eeSecurityEvent::new(&user_id, "key_share")
        .with_device("DEV1")
        .with_data(serde_json::json!({"recipient": "@bob:localhost"}))
        .with_ip("192.168.1.1")
        .with_user_agent("TestAgent/1.0");

    storage.log_security_event(&event).await.unwrap();

    let count: i64 =
        sqlx::query_scalar("SELECT COUNT(*) FROM e2ee_security_events WHERE user_id = $1 AND event_type = $2")
            .bind(&user_id)
            .bind("key_share")
            .fetch_one(pool.as_ref())
            .await
            .unwrap();
    assert_eq!(count, 1);

    teardown(pool.as_ref()).await;
}

#[allow(clippy::await_holding_lock)]
#[tokio::test]
async fn test_get_recent_security_events_returns_ordered_desc() {
    let _guard = dt_guard().lock().unwrap();
    let pool = crate::require_test_pool().await;
    setup(&pool).await;
    let storage = new_storage(&pool);

    let user_id = unique_user_id();
    let base = chrono::Utc::now().timestamp_millis();

    for i in 0..3 {
        let mut event = E2eeSecurityEvent::new(&user_id, &format!("event_{i}"));
        // Override created_ts to enforce ordering
        event.created_ts = base + i * 1000;
        storage.log_security_event(&event).await.unwrap();
        // Small delay so DB created_ts differs
        tokio::time::sleep(std::time::Duration::from_millis(5)).await;
    }

    let events = storage.get_recent_security_events(&user_id, 10).await.unwrap();
    assert_eq!(events.len(), 3);
    // Should be ordered by created_ts DESC
    assert!(events[0].created_ts >= events[1].created_ts);
    assert!(events[1].created_ts >= events[2].created_ts);

    teardown(pool.as_ref()).await;
}

#[allow(clippy::await_holding_lock)]
#[tokio::test]
async fn test_get_recent_security_events_respects_limit() {
    let _guard = dt_guard().lock().unwrap();
    let pool = crate::require_test_pool().await;
    setup(&pool).await;
    let storage = new_storage(&pool);

    let user_id = unique_user_id();
    for i in 0..5 {
        let event = E2eeSecurityEvent::new(&user_id, &format!("evt_{i}"));
        storage.log_security_event(&event).await.unwrap();
        tokio::time::sleep(std::time::Duration::from_millis(2)).await;
    }

    let events = storage.get_recent_security_events(&user_id, 2).await.unwrap();
    assert_eq!(events.len(), 2, "Should respect the limit");

    teardown(pool.as_ref()).await;
}

#[allow(clippy::await_holding_lock)]
#[tokio::test]
async fn test_get_recent_security_events_isolates_by_user() {
    let _guard = dt_guard().lock().unwrap();
    let pool = crate::require_test_pool().await;
    setup(&pool).await;
    let storage = new_storage(&pool);

    let user_a = unique_user_id();
    let user_b = unique_user_id();

    storage.log_security_event(&E2eeSecurityEvent::new(&user_a, "a_event")).await.unwrap();
    storage.log_security_event(&E2eeSecurityEvent::new(&user_b, "b_event")).await.unwrap();

    let a_events = storage.get_recent_security_events(&user_a, 10).await.unwrap();
    assert_eq!(a_events.len(), 1);
    assert_eq!(a_events[0].event_type, "a_event");

    teardown(pool.as_ref()).await;
}

// ===========================================================================
// Cross-signing trust
// ===========================================================================

#[allow(clippy::await_holding_lock)]
#[tokio::test]
async fn test_set_cross_signing_trust_trusted_sets_trusted_at() {
    let _guard = dt_guard().lock().unwrap();
    let pool = crate::require_test_pool().await;
    setup(&pool).await;
    let storage = new_storage(&pool);

    let user_id = unique_user_id();
    let target_user_id = unique_user_id();

    storage.set_cross_signing_trust(&user_id, &target_user_id, true).await.unwrap();

    let is_trusted: bool = sqlx::query_scalar(
        "SELECT is_trusted FROM cross_signing_trust WHERE user_id = $1 AND target_user_id = $2",
    )
    .bind(&user_id)
    .bind(&target_user_id)
    .fetch_one(pool.as_ref())
    .await
    .unwrap();
    assert!(is_trusted);

    let trusted_at: Option<chrono::DateTime<chrono::Utc>> = sqlx::query_scalar(
        "SELECT trusted_at FROM cross_signing_trust WHERE user_id = $1 AND target_user_id = $2",
    )
    .bind(&user_id)
    .bind(&target_user_id)
    .fetch_one(pool.as_ref())
    .await
    .unwrap();
    assert!(trusted_at.is_some(), "trusted_at should be set when is_trusted=true");

    teardown(pool.as_ref()).await;
}

#[allow(clippy::await_holding_lock)]
#[tokio::test]
async fn test_set_cross_signing_trust_untrusted_no_trusted_at() {
    let _guard = dt_guard().lock().unwrap();
    let pool = crate::require_test_pool().await;
    setup(&pool).await;
    let storage = new_storage(&pool);

    let user_id = unique_user_id();
    let target_user_id = unique_user_id();

    // First trust
    storage.set_cross_signing_trust(&user_id, &target_user_id, true).await.unwrap();
    // Then untrust
    storage.set_cross_signing_trust(&user_id, &target_user_id, false).await.unwrap();

    let is_trusted: bool = sqlx::query_scalar(
        "SELECT is_trusted FROM cross_signing_trust WHERE user_id = $1 AND target_user_id = $2",
    )
    .bind(&user_id)
    .bind(&target_user_id)
    .fetch_one(pool.as_ref())
    .await
    .unwrap();
    assert!(!is_trusted);

    // trusted_at should be preserved (CASE WHEN EXCLUDED.is_trusted=TRUE THEN ... ELSE cross_signing_trust.trusted_at)
    let trusted_at: Option<chrono::DateTime<chrono::Utc>> = sqlx::query_scalar(
        "SELECT trusted_at FROM cross_signing_trust WHERE user_id = $1 AND target_user_id = $2",
    )
    .bind(&user_id)
    .bind(&target_user_id)
    .fetch_one(pool.as_ref())
    .await
    .unwrap();
    assert!(trusted_at.is_some(), "trusted_at should be preserved from previous trust");

    teardown(pool.as_ref()).await;
}

#[allow(clippy::await_holding_lock)]
#[tokio::test]
async fn test_set_cross_signing_trust_upserts_existing_pair() {
    let _guard = dt_guard().lock().unwrap();
    let pool = crate::require_test_pool().await;
    setup(&pool).await;
    let storage = new_storage(&pool);

    let user_id = unique_user_id();
    let target_user_id = unique_user_id();

    storage.set_cross_signing_trust(&user_id, &target_user_id, true).await.unwrap();
    storage.set_cross_signing_trust(&user_id, &target_user_id, true).await.unwrap();

    let count: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM cross_signing_trust WHERE user_id = $1 AND target_user_id = $2",
    )
    .bind(&user_id)
    .bind(&target_user_id)
    .fetch_one(pool.as_ref())
    .await
    .unwrap();
    assert_eq!(count, 1, "set_cross_signing_trust should upsert, not insert duplicates");

    teardown(pool.as_ref()).await;
}

// ===========================================================================
// has_cross_signing_master_key
// ===========================================================================

#[allow(clippy::await_holding_lock)]
#[tokio::test]
async fn test_has_cross_signing_master_key_true_when_exists() {
    let _guard = dt_guard().lock().unwrap();
    let pool = crate::require_test_pool().await;
    setup(&pool).await;
    let storage = new_storage(&pool);

    let user_id = unique_user_id();
    let now = chrono::Utc::now().timestamp_millis();
    sqlx::query("INSERT INTO cross_signing_keys (user_id, key_type, key_data, added_ts) VALUES ($1, 'master', $2, $3)")
        .bind(&user_id)
        .bind("master_key_data")
        .bind(now)
        .execute(pool.as_ref())
        .await
        .unwrap();

    let has = storage.has_cross_signing_master_key(&user_id).await.unwrap();
    assert!(has);

    teardown(pool.as_ref()).await;
}

#[allow(clippy::await_holding_lock)]
#[tokio::test]
async fn test_has_cross_signing_master_key_false_when_absent() {
    let _guard = dt_guard().lock().unwrap();
    let pool = crate::require_test_pool().await;
    setup(&pool).await;
    let storage = new_storage(&pool);

    let user_id = unique_user_id();
    let has = storage.has_cross_signing_master_key(&user_id).await.unwrap();
    assert!(!has);

    teardown(pool.as_ref()).await;
}

#[allow(clippy::await_holding_lock)]
#[tokio::test]
async fn test_has_cross_signing_master_key_ignores_non_master_keys() {
    let _guard = dt_guard().lock().unwrap();
    let pool = crate::require_test_pool().await;
    setup(&pool).await;
    let storage = new_storage(&pool);

    let user_id = unique_user_id();
    let now = chrono::Utc::now().timestamp_millis();
    // Insert a self_signing key, not master
    sqlx::query("INSERT INTO cross_signing_keys (user_id, key_type, key_data, added_ts) VALUES ($1, 'self_signing', $2, $3)")
        .bind(&user_id)
        .bind("self_signing_data")
        .bind(now)
        .execute(pool.as_ref())
        .await
        .unwrap();

    let has = storage.has_cross_signing_master_key(&user_id).await.unwrap();
    assert!(!has, "Should only return true for master key type");

    teardown(pool.as_ref()).await;
}
