//! Integration tests for `synapse_services::e2ee_audit` service layer.
//!
//! Background: the storage layer (`synapse-storage/src/e2ee_audit.rs`) has its
//! own `db_tests` covering the seven SQL paths (log/select/filter/cleanup).
//! This file targets the **service layer** in
//! `synapse-services/src/e2ee_audit/audit_service.rs` which was previously 0%
//! covered.
//!
//! The cross_signing signature path seeds the *storage* tables
//! `cross_signing_keys` and `device_signatures` directly (no cryptographic
//! primitives) so the test does not pull in the full e2ee device-bootstrap
//! pipeline. This keeps tests fast (no key generation, no vodozemac) while
//! still exercising the business logic that joins the four storage tables.

#![cfg(feature = "test-utils")]

use std::sync::Arc;

use synapse_common::current_timestamp_millis;
use synapse_services::e2ee_audit::{CrossSigningVerificationService, E2eeAuditService, KeyEvent};
use synapse_storage::e2ee_audit::E2eeAuditStorage;
use synapse_storage::DeviceStorage;

use crate::require_test_pool;

/// Build a unique `@user:example.com` Matrix id from a tag.
fn unique_user_id(tag: &str) -> String {
    format!("@{}_{}:example.com", tag, uuid::Uuid::new_v4().simple())
}

/// Insert a minimal row into `users` so that FKs from `devices`,
/// `device_trust_status`, etc. are satisfied. The user is not password-hashed
/// because none of the audit paths need to authenticate; this is purely a
/// fixture to satisfy foreign key constraints.
async fn ensure_user_row(pool: &sqlx::PgPool, user_id: &str) {
    let suffix = uuid::Uuid::new_v4().simple().to_string();
    let localpart = user_id.split(':').next().unwrap_or(user_id).trim_start_matches('@');
    sqlx::query(
        r"INSERT INTO users (user_id, username, created_ts, password_hash, is_deactivated, appservice_id)
         VALUES ($1, $2, $3, 'placeholder', false, NULL)
         ON CONFLICT (user_id) DO NOTHING",
    )
    .bind(user_id)
    .bind(localpart)
    .bind(current_timestamp_millis())
    .execute(pool)
    .await
    .unwrap_or_else(|e| panic!("ensure_user_row({user_id}, suffix={suffix}) failed: {e}"));
}

/// Insert a key event directly via storage to set up baseline state.
async fn seed_audit_log(storage: &E2eeAuditStorage, user_id: &str, operation: &str, device_id: &str, ts: i64) {
    let event = KeyEvent {
        user_id: user_id.to_string(),
        device_id: Some(device_id.to_string()),
        operation: operation.to_string(),
        key_id: Some(format!("ed25519:{device_id}")),
        room_id: None,
        details: Some(serde_json::json!({"seed": true})),
        ip_address: Some("127.0.0.1".to_string()),
        timestamp: ts,
    };
    storage.log_key_operation(&event).await.expect("seed log");
}

// ─────────────────────────────────────────────────────────────────────────────
// E2eeAuditService — 6 entry points, each must round-trip the DB.
// ─────────────────────────────────────────────────────────────────────────────

#[tokio::test]
async fn audit_service_log_key_operation_round_trip() {
    let pool = require_test_pool().await;
    let svc = E2eeAuditService::new(pool.clone());
    let user_id = unique_user_id("log");

    let event = KeyEvent {
        user_id: user_id.clone(),
        device_id: Some("DEV_LOG".to_string()),
        operation: "upload_keys".to_string(),
        key_id: Some("ed25519:master".to_string()),
        room_id: Some("!room_log:example.com".to_string()),
        details: Some(serde_json::json!({"algorithm": "ed25519"})),
        ip_address: Some("10.0.0.1".to_string()),
        timestamp: current_timestamp_millis(),
    };

    svc.log_key_operation(event).await.expect("log_key_operation service");

    let storage = E2eeAuditStorage::new(&pool);
    let history = storage.get_key_history(&user_id).await.expect("get_key_history");
    assert_eq!(history.len(), 1, "service should have persisted exactly one entry");
    assert_eq!(history[0].operation, "upload_keys");
    assert_eq!(history[0].device_id.as_deref(), Some("DEV_LOG"));
}

#[tokio::test]
async fn audit_service_get_key_history_filters_by_user() {
    let pool = require_test_pool().await;
    let storage = E2eeAuditStorage::new(&pool);
    let svc = E2eeAuditService::new(pool.clone());

    let user_a = unique_user_id("hist_a");
    let user_b = unique_user_id("hist_b");

    let now = current_timestamp_millis();
    seed_audit_log(&storage, &user_a, "op_a1", "DEV_A", now).await;
    seed_audit_log(&storage, &user_a, "op_a2", "DEV_A", now + 1).await;
    seed_audit_log(&storage, &user_b, "op_b1", "DEV_B", now + 2).await;

    let hist_a = svc.get_key_history(&user_a).await.expect("get_key_history a");
    assert_eq!(hist_a.len(), 2, "user_a should have 2 entries");
    assert!(hist_a.iter().all(|e| e.user_id == user_a), "user_a must only see own entries");

    let hist_b = svc.get_key_history(&user_b).await.expect("get_key_history b");
    assert_eq!(hist_b.len(), 1, "user_b should have 1 entry");
    assert_eq!(hist_b[0].user_id, user_b);
}

#[tokio::test]
async fn audit_service_get_key_history_paginated_walks_pages() {
    let pool = require_test_pool().await;
    let storage = E2eeAuditStorage::new(&pool);
    let svc = E2eeAuditService::new(pool.clone());

    let user_id = unique_user_id("paged");
    let base_ts = current_timestamp_millis();
    for i in 0..5 {
        seed_audit_log(&storage, &user_id, "query_keys", &format!("DEV_{i}"), base_ts + i * 1000).await;
    }

    let page1 = svc.get_key_history_paginated(&user_id, 2, None, None).await.expect("page1");
    assert_eq!(page1.len(), 2);

    let last = page1.last().expect("non-empty page1");
    let page2 = svc.get_key_history_paginated(&user_id, 2, Some(last.created_ts), Some(last.id)).await.expect("page2");
    assert_eq!(page2.len(), 2);
    assert_ne!(page1[0].id, page2[0].id, "pages must not overlap");

    let last2 = page2.last().expect("non-empty page2");
    let page3 =
        svc.get_key_history_paginated(&user_id, 2, Some(last2.created_ts), Some(last2.id)).await.expect("page3");
    assert_eq!(page3.len(), 1, "page 3 should have the remaining 1 entry");
}

#[tokio::test]
async fn audit_service_get_operations_by_type_filters() {
    let pool = require_test_pool().await;
    let storage = E2eeAuditStorage::new(&pool);
    let svc = E2eeAuditService::new(pool.clone());

    let user_id = unique_user_id("optype");
    let suffix = uuid::Uuid::new_v4().simple().to_string();
    let op_upload = format!("upload_dev_keys_{suffix}");
    let op_sign = format!("sign_{suffix}");

    let now = current_timestamp_millis();
    seed_audit_log(&storage, &user_id, &op_upload, "DEV_A", now).await;
    seed_audit_log(&storage, &user_id, &op_sign, "DEV_A", now + 1).await;
    seed_audit_log(&storage, &user_id, &op_upload, "DEV_A", now + 2).await;

    let uploads = svc.get_operations_by_type(&op_upload, 50).await.expect("uploads");
    assert_eq!(uploads.len(), 2, "must return only the 2 upload events");
    assert!(uploads.iter().all(|e| e.operation == op_upload));

    let signs = svc.get_operations_by_type(&op_sign, 50).await.expect("signs");
    assert_eq!(signs.len(), 1);
    assert_eq!(signs[0].operation, op_sign);
}

#[tokio::test]
async fn audit_service_get_user_device_history_isolates_per_device() {
    let pool = require_test_pool().await;
    let storage = E2eeAuditStorage::new(&pool);
    let svc = E2eeAuditService::new(pool.clone());

    let user_id = unique_user_id("devhist");
    let now = current_timestamp_millis();
    seed_audit_log(&storage, &user_id, "op", "PHONE", now).await;
    seed_audit_log(&storage, &user_id, "op", "PHONE", now + 1).await;
    seed_audit_log(&storage, &user_id, "op", "LAPTOP", now + 2).await;

    let phone = svc.get_user_device_history(&user_id, "PHONE").await.expect("phone");
    assert_eq!(phone.len(), 2);
    assert!(phone.iter().all(|e| e.device_id.as_deref() == Some("PHONE")));

    let laptop = svc.get_user_device_history(&user_id, "LAPTOP").await.expect("laptop");
    assert_eq!(laptop.len(), 1);

    let ghost = svc.get_user_device_history(&user_id, "GHOST").await.expect("ghost");
    assert!(ghost.is_empty(), "non-existent device must return empty vec");
}

#[tokio::test]
async fn audit_service_cleanup_old_logs_returns_zero_for_fresh_data() {
    let pool = require_test_pool().await;
    let storage = E2eeAuditStorage::new(&pool);
    let svc = E2eeAuditService::new(pool.clone());

    let user_id = unique_user_id("cleanup_fresh");
    let one_hour_ago = current_timestamp_millis() - 3600 * 1000;
    seed_audit_log(&storage, &user_id, "op", "DEV", one_hour_ago).await;

    let deleted = svc.cleanup_old_logs(365).await.expect("cleanup");
    assert_eq!(deleted, 0, "fresh entries must not be deleted");

    let history = svc.get_key_history(&user_id).await.expect("get history");
    assert_eq!(history.len(), 1, "entry should still be present");
}

#[tokio::test]
async fn audit_service_cleanup_old_logs_removes_old_entries() {
    let pool = require_test_pool().await;
    let storage = E2eeAuditStorage::new(&pool);
    let svc = E2eeAuditService::new(pool.clone());

    let user_id = unique_user_id("cleanup_old");
    let now = current_timestamp_millis();
    let day_ms: i64 = 24 * 60 * 60 * 1000;

    seed_audit_log(&storage, &user_id, "old", "OLD_DEV", now - 40 * day_ms).await;
    seed_audit_log(&storage, &user_id, "recent", "NEW_DEV", now - day_ms).await;

    let deleted = svc.cleanup_old_logs(7).await.expect("cleanup");
    assert!(deleted >= 1, "should have deleted at least the old entry");

    let history = svc.get_key_history(&user_id).await.expect("get history");
    assert_eq!(history.len(), 1, "only the recent entry must remain");
    assert_eq!(history[0].operation, "recent");
}

// ─────────────────────────────────────────────────────────────────────────────
// CrossSigningVerificationService — branch coverage on the core paths.
// ─────────────────────────────────────────────────────────────────────────────

#[tokio::test]
async fn cross_signing_verify_user_devices_returns_empty_report_for_no_devices() {
    // Critical branch: when get_user_devices returns empty the service must
    // early-return a default `DeviceVerificationReport` without touching
    // cross_signing tables.
    let pool = require_test_pool().await;
    let audit = Arc::new(E2eeAuditService::new(pool.clone()));
    let svc = CrossSigningVerificationService::new(pool.clone(), audit.clone());

    let user_id = unique_user_id("empty");

    let report = svc.verify_user_devices(&user_id).await.expect("verify_user_devices");

    assert_eq!(report.user_id, user_id);
    assert!(report.devices.is_empty());
    assert!(report.all_verified, "all_verified should be true for empty device list");
    assert!(!report.cross_signing_setup, "no cross-signing when no devices exist");
    assert_eq!(report.verified_count, 0);
    assert_eq!(report.unverified_count, 0);
}

#[tokio::test]
async fn cross_signing_verify_user_devices_unverified_when_no_cross_signing_key() {
    // Branch: devices exist but no cross_signing key for the user -> every
    // device should be reported unverified, cross_signing_setup false,
    // and an audit "verify_all_devices" event should be logged.
    let pool = require_test_pool().await;
    let audit = Arc::new(E2eeAuditService::new(pool.clone()));
    let svc = CrossSigningVerificationService::new(pool.clone(), audit.clone());

    let user_id = unique_user_id("nodevices");
    let device_storage = DeviceStorage::new(&pool);
    ensure_user_row(&pool, &user_id).await;
    device_storage.create_device("UNVERIFIED_DEV", &user_id, Some("My Phone")).await.expect("create device");

    let report = svc.verify_user_devices(&user_id).await.expect("verify_user_devices");

    assert_eq!(report.devices.len(), 1, "should report the single device");
    assert!(!report.all_verified, "all_verified must be false");
    assert!(!report.cross_signing_setup, "cross_signing_setup must be false");
    assert_eq!(report.verified_count, 0);
    assert_eq!(report.unverified_count, 1);
    assert!(!report.devices[0].is_verified);
    assert!(!report.devices[0].is_cross_signed);
    assert!(!report.devices[0].signature_valid);

    // The service must have written an audit "verify_all_devices" entry.
    let history = audit.get_key_history(&user_id).await.expect("audit history");
    let has_summary = history.iter().any(|e| e.operation == "verify_all_devices");
    assert!(has_summary, "verify_all_devices audit entry should be written");
}

#[tokio::test]
async fn cross_signing_verify_user_devices_marks_verified_when_signature_present() {
    // Branch: device has a signature AND a self_signing cross-signing key ->
    // marked verified, all_verified=true, cross_signing_setup=true.
    //
    // We seed both tables directly (no real crypto required) so the service
    // path is exercised end-to-end.
    let pool = require_test_pool().await;
    let audit = Arc::new(E2eeAuditService::new(pool.clone()));
    let svc = CrossSigningVerificationService::new(pool.clone(), audit.clone());

    let user_id = unique_user_id("withsig");
    let device_id = "SIGNED_DEV";
    let device_storage = DeviceStorage::new(&pool);
    ensure_user_row(&pool, &user_id).await;
    device_storage.create_device(device_id, &user_id, Some("Signed Device")).await.expect("create device");

    let now = current_timestamp_millis();
    sqlx::query(
        r"INSERT INTO cross_signing_keys (user_id, key_type, key_data, added_ts)
         VALUES ($1, 'self_signing', $2, $3)
         ON CONFLICT (user_id, key_type) DO NOTHING",
    )
    .bind(&user_id)
    .bind(serde_json::json!({"ed25519:AA": "fakebase64keydata"}))
    .bind(now)
    .execute(&*pool)
    .await
    .expect("seed cross_signing_keys");

    sqlx::query(
        r"INSERT INTO device_signatures (user_id, device_id, target_user_id, target_device_id, algorithm, signature, created_ts)
         VALUES ($1, $2, $1, $2, 'ed25519', $3, $4)",
    )
    .bind(&user_id)
    .bind(device_id)
    .bind(serde_json::json!({"ed25519:AA": "sigbytes"}))
    .bind(now)
    .execute(&*pool)
    .await
    .expect("seed device_signatures");

    let report = svc.verify_user_devices(&user_id).await.expect("verify_user_devices");

    assert_eq!(report.devices.len(), 1);
    let dev = &report.devices[0];
    assert!(dev.signature_valid, "signature_valid must be true after seeding");
    assert!(dev.is_cross_signed, "is_cross_signed must reflect seeded self_signing key");
    assert!(dev.is_verified, "is_verified = signature_valid && cross_signing_setup");
    assert!(report.all_verified, "single verified device => all_verified");
    assert!(report.cross_signing_setup);
    assert_eq!(report.verified_count, 1);
    assert_eq!(report.unverified_count, 0);
}

#[tokio::test]
async fn cross_signing_verify_user_devices_mixed_verified_and_unverified() {
    // Branch: 2 devices, only one has a signature -> verified_count=1,
    // unverified_count=1, all_verified=false.
    let pool = require_test_pool().await;
    let audit = Arc::new(E2eeAuditService::new(pool.clone()));
    let svc = CrossSigningVerificationService::new(pool.clone(), audit.clone());

    let user_id = unique_user_id("mixed");
    let device_storage = DeviceStorage::new(&pool);
    ensure_user_row(&pool, &user_id).await;
    device_storage
        .create_device("DEV_VERIFIED", &user_id, Some("Verified Device"))
        .await
        .expect("create verified device");
    device_storage
        .create_device("DEV_UNVERIFIED", &user_id, Some("Unverified Device"))
        .await
        .expect("create unverified device");

    let now = current_timestamp_millis();
    sqlx::query(
        r"INSERT INTO cross_signing_keys (user_id, key_type, key_data, added_ts)
         VALUES ($1, 'self_signing', $2, $3)
         ON CONFLICT (user_id, key_type) DO NOTHING",
    )
    .bind(&user_id)
    .bind(serde_json::json!({"ed25519:AA": "fakekey"}))
    .bind(now)
    .execute(&*pool)
    .await
    .expect("seed cross_signing_keys");

    // Only DEV_VERIFIED gets a signature.
    sqlx::query(
        r"INSERT INTO device_signatures (user_id, device_id, target_user_id, target_device_id, algorithm, signature, created_ts)
         VALUES ($1, $2, $1, $2, 'ed25519', $3, $4)",
    )
    .bind(&user_id)
    .bind("DEV_VERIFIED")
    .bind(serde_json::json!({"ed25519:AA": "sigbytes"}))
    .bind(now)
    .execute(&*pool)
    .await
    .expect("seed device_signatures");

    let report = svc.verify_user_devices(&user_id).await.expect("verify_user_devices");

    assert_eq!(report.devices.len(), 2);
    let verified = report.devices.iter().find(|d| d.device_id == "DEV_VERIFIED").expect("DEV_VERIFIED present");
    let unverified = report.devices.iter().find(|d| d.device_id == "DEV_UNVERIFIED").expect("DEV_UNVERIFIED present");

    assert!(verified.is_verified, "DEV_VERIFIED must be verified");
    assert!(verified.signature_valid);
    assert!(verified.is_cross_signed);

    assert!(!unverified.is_verified, "DEV_UNVERIFIED must be unverified");
    assert!(!unverified.signature_valid, "no signature seeded for DEV_UNVERIFIED");
    assert!(unverified.is_cross_signed, "cross_signing_setup is per-user, not per-device");

    assert!(!report.all_verified, "mixed set => all_verified=false");
    assert_eq!(report.verified_count, 1);
    assert_eq!(report.unverified_count, 1);
    assert!(report.cross_signing_setup, "self_signing key exists => cross_signing_setup true");
}

#[tokio::test]
async fn cross_signing_mark_device_verified_writes_audit_log_and_trust_row() {
    // Branch: mark_device_verified -> writes to device_trust_status table and
    // logs a "mark_verified" audit event.
    let pool = require_test_pool().await;
    let audit = Arc::new(E2eeAuditService::new(pool.clone()));
    let svc = CrossSigningVerificationService::new(pool.clone(), audit.clone());

    let user_id = unique_user_id("markver");
    let device_storage = DeviceStorage::new(&pool);
    ensure_user_row(&pool, &user_id).await;
    device_storage.create_device("TO_VERIFY", &user_id, Some("To Be Verified")).await.expect("create device");

    svc.mark_device_verified(&user_id, "TO_VERIFY", "qr_code_scan").await.expect("mark_device_verified");

    // Verify trust row was written.
    let trust_count: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM device_trust_status WHERE user_id = $1 AND device_id = $2 AND trust_level = 'verified'",
    )
    .bind(&user_id)
    .bind("TO_VERIFY")
    .fetch_one(&*pool)
    .await
    .expect("query trust");
    assert_eq!(trust_count, 1, "verified trust row must be written");

    // Verify audit log entry was written.
    let history = audit.get_key_history(&user_id).await.expect("audit history");
    let mark_entry = history.iter().find(|e| e.operation == "mark_verified");
    assert!(mark_entry.is_some(), "mark_verified audit entry must exist");
    let details = mark_entry.unwrap().details.clone().expect("details present");
    assert_eq!(details["method"], "qr_code_scan", "method must be recorded in details");
}

#[tokio::test]
async fn cross_signing_mark_device_unverified_writes_audit_log_and_trust_row() {
    // Branch: mark_device_unverified -> trust_level='unverified', no
    // verified_by_device_id, audit op="mark_unverified".
    let pool = require_test_pool().await;
    let audit = Arc::new(E2eeAuditService::new(pool.clone()));
    let svc = CrossSigningVerificationService::new(pool.clone(), audit.clone());

    let user_id = unique_user_id("markunver");
    let device_storage = DeviceStorage::new(&pool);
    ensure_user_row(&pool, &user_id).await;
    device_storage.create_device("TO_UNVERIFY", &user_id, Some("To Be Unverified")).await.expect("create device");

    svc.mark_device_unverified(&user_id, "TO_UNVERIFY", "user_revoked").await.expect("mark_device_unverified");

    let trust_count: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM device_trust_status WHERE user_id = $1 AND device_id = $2 AND trust_level = 'unverified'",
    )
    .bind(&user_id)
    .bind("TO_UNVERIFY")
    .fetch_one(&*pool)
    .await
    .expect("query trust");
    assert_eq!(trust_count, 1, "unverified trust row must be written");

    let history = audit.get_key_history(&user_id).await.expect("audit history");
    let mark_entry = history.iter().find(|e| e.operation == "mark_unverified");
    assert!(mark_entry.is_some(), "mark_unverified audit entry must exist");
    let details = mark_entry.unwrap().details.clone().expect("details present");
    assert_eq!(details["reason"], "user_revoked", "reason must be recorded in details");
}

#[tokio::test]
async fn cross_signing_mark_verified_then_unverified_updates_trust_level() {
    // Branch: trust_level must transition verified -> unverified when
    // mark_device_unverified is called after mark_device_verified.
    let pool = require_test_pool().await;
    let audit = Arc::new(E2eeAuditService::new(pool.clone()));
    let svc = CrossSigningVerificationService::new(pool.clone(), audit.clone());

    let user_id = unique_user_id("transition");
    let device_storage = DeviceStorage::new(&pool);
    ensure_user_row(&pool, &user_id).await;
    device_storage.create_device("TRANSITION_DEV", &user_id, Some("Transition Device")).await.expect("create device");

    svc.mark_device_verified(&user_id, "TRANSITION_DEV", "emoji_verify").await.expect("first verify");
    svc.mark_device_unverified(&user_id, "TRANSITION_DEV", "key_reset").await.expect("then unverify");

    // Latest trust_level must be 'unverified'.
    let trust_level: String =
        sqlx::query_scalar("SELECT trust_level FROM device_trust_status WHERE user_id = $1 AND device_id = $2")
            .bind(&user_id)
            .bind("TRANSITION_DEV")
            .fetch_one(&*pool)
            .await
            .expect("query trust level");
    assert_eq!(trust_level, "unverified", "trust level must reflect latest action");

    // Both audit events should be present in the log.
    let history = audit.get_key_history(&user_id).await.expect("audit history");
    assert!(history.iter().any(|e| e.operation == "mark_verified"));
    assert!(history.iter().any(|e| e.operation == "mark_unverified"));
}
