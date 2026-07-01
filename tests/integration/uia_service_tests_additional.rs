//! Integration tests for `synapse-services/src/uia_service.rs`.
//!
//! These tests complement `uia_service_tests_migrated.rs` by focusing on
//! DB-backed coverage of methods that the migrated file does not exercise:
//!
//! - `verify_email_identity_stage` — full credential-validation matrix and
//!   verified-email checks against `ThreepidStorage`.
//! - `verify_msisdn_stage` — full credential-validation matrix and
//!   verified-phone checks against `ThreepidStorage`.
//! - `verify_password_stage` — success path and identifier-resolution variants
//!   (localpart, `user`, `user_id` fields) using a real `AuthService`.
//! - `verify_token_stage` — success path, cross-user rejection, and
//!   invalid-token rejection using a real `AuthService`.
//! - `require_uia` — end-to-end orchestration across password/token/email/
//!   msisdn/unsupported auth types.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex, OnceLock};

use serde_json::json;
use synapse_rust::auth::AuthService;
use synapse_rust::cache::{CacheConfig, CacheManager};
use synapse_rust::common::config::SecurityConfig;
use synapse_rust::common::metrics::MetricsCollector;
use synapse_services::uia_service::{UiaFlow, UiaService};
use synapse_storage::threepid::ThreepidStorage;

static TEST_COUNTER: AtomicU64 = AtomicU64::new(1);

fn unique_id() -> u64 {
    TEST_COUNTER.fetch_add(1, Ordering::SeqCst)
}

fn uia_test_guard() -> &'static Mutex<()> {
    static GUARD: OnceLock<Mutex<()>> = OnceLock::new();
    GUARD.get_or_init(|| Mutex::new(()))
}

const TEST_PASSWORD: &str = "TestPass@123";

/// Warm up the shared pool on the current tokio runtime to work around
/// cross-runtime sqlx pool isolation issues.
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

/// Clean tables used by these tests. Users are left in place (each test uses
/// unique user IDs), but `user_threepids` is cleared to keep the shared
/// schema tidy.
async fn setup(pool: &Arc<sqlx::PgPool>) {
    warm_up_pool(pool).await;
    sqlx::query("DELETE FROM user_threepids").execute(pool.as_ref()).await.ok();
}

fn create_service() -> UiaService {
    let cache = Arc::new(CacheManager::new(&CacheConfig::default()));
    UiaService::new(cache, 3600)
}

fn create_security_config() -> SecurityConfig {
    SecurityConfig {
        secret: "test_secret".to_string(),
        expiry_time: 3600,
        refresh_token_expiry: 604800,
        argon2_m_cost: 2048,
        argon2_t_cost: 1,
        argon2_p_cost: 1,
        allow_legacy_hashes: false,
        login_failure_lockout_threshold: 5,
        login_lockout_duration_seconds: 900,
        admin_mfa_required: false,
        admin_mfa_shared_secret: String::new(),
        admin_mfa_allowed_drift_steps: 1,
        admin_rbac_enabled: true,
        ui_auth_session_timeout: 900,
        ..Default::default()
    }
}

fn create_auth_service(pool: &Arc<sqlx::PgPool>) -> Arc<dyn synapse_rust::auth::Auth> {
    let cache = Arc::new(CacheManager::new(&CacheConfig::default()));
    let metrics = Arc::new(MetricsCollector::new());
    let security = create_security_config();
    Arc::new(AuthService::new(pool, cache, metrics, &security, "localhost"))
}

fn create_threepid_storage(pool: &Arc<sqlx::PgPool>) -> ThreepidStorage {
    ThreepidStorage::new(pool.as_ref())
}

/// Insert a bare user row (no password) for threepid-only tests.
async fn insert_user(pool: &sqlx::PgPool, user_id: &str, username: &str) {
    let now = chrono::Utc::now().timestamp_millis();
    sqlx::query("INSERT INTO users (user_id, username, created_ts) VALUES ($1, $2, $3) ON CONFLICT DO NOTHING")
        .bind(user_id)
        .bind(username)
        .bind(now)
        .execute(pool)
        .await
        .expect("Failed to insert test user");
}

/// Register a test user via AuthService and return `(user_id, access_token)`.
async fn register_test_user(
    auth_service: &Arc<dyn synapse_rust::auth::Auth>,
    suffix: u64,
) -> (String, String) {
    let username = format!("uiauser{suffix}");
    let (user, access_token, _refresh, _device) = auth_service
        .register(&username, TEST_PASSWORD, false, None)
        .await
        .expect("Failed to register test user");
    (user.user_id, access_token)
}

async fn cleanup_user(pool: &sqlx::PgPool, user_id: &str) {
    // ON DELETE CASCADE propagates to user_threepids, devices, access_tokens, refresh_tokens.
    sqlx::query("DELETE FROM users WHERE user_id = $1").bind(user_id).execute(pool).await.ok();
}

/// A valid `threepidCreds` array with one entry.
fn valid_threepid_creds() -> serde_json::Value {
    json!([{"sid": "sid-123", "client_secret": "secret-abc"}])
}

// =============================================================================
// verify_email_identity_stage
// =============================================================================

#[tokio::test]
async fn test_email_identity_missing_threepid_creds() {
    let _guard = uia_test_guard().lock().unwrap_or_else(|e| e.into_inner());
    let pool = crate::require_test_pool().await;
    setup(&pool).await;
    let service = create_service();
    let threepid_storage = create_threepid_storage(&pool);
    let uid = unique_id();
    let user_id = format!("@uiaemail{uid}:localhost");
    insert_user(&pool, &user_id, &format!("uiaemail{uid}")).await;

    let auth = json!({"type": "m.login.email.identity"});
    let result = service.verify_email_identity_stage(&auth, &user_id, &threepid_storage).await;
    assert!(result.is_err());
    let e = result.unwrap_err();
    assert!(e.is_bad_request(), "expected bad_request, got: {:?}", e);
    assert!(e.internal_message().contains("threepidCreds array required"));

    cleanup_user(&pool, &user_id).await;
}

#[tokio::test]
async fn test_email_identity_threepid_creds_not_array() {
    let _guard = uia_test_guard().lock().unwrap_or_else(|e| e.into_inner());
    let pool = crate::require_test_pool().await;
    setup(&pool).await;
    let service = create_service();
    let threepid_storage = create_threepid_storage(&pool);
    let uid = unique_id();
    let user_id = format!("@uiaemail{uid}:localhost");
    insert_user(&pool, &user_id, &format!("uiaemail{uid}")).await;

    let auth = json!({"type": "m.login.email.identity", "threepidCreds": "not-an-array"});
    let result = service.verify_email_identity_stage(&auth, &user_id, &threepid_storage).await;
    assert!(result.is_err());
    let e = result.unwrap_err();
    assert!(e.is_bad_request());
    assert!(e.internal_message().contains("threepidCreds array required"));

    cleanup_user(&pool, &user_id).await;
}

#[tokio::test]
async fn test_email_identity_missing_sid() {
    let _guard = uia_test_guard().lock().unwrap_or_else(|e| e.into_inner());
    let pool = crate::require_test_pool().await;
    setup(&pool).await;
    let service = create_service();
    let threepid_storage = create_threepid_storage(&pool);
    let uid = unique_id();
    let user_id = format!("@uiaemail{uid}:localhost");
    insert_user(&pool, &user_id, &format!("uiaemail{uid}")).await;

    let auth = json!({"threepidCreds": [{"client_secret": "cs"}]});
    let result = service.verify_email_identity_stage(&auth, &user_id, &threepid_storage).await;
    assert!(result.is_err());
    let e = result.unwrap_err();
    assert!(e.is_bad_request());
    assert!(e.internal_message().contains("sid required"));

    cleanup_user(&pool, &user_id).await;
}

#[tokio::test]
async fn test_email_identity_missing_client_secret() {
    let _guard = uia_test_guard().lock().unwrap_or_else(|e| e.into_inner());
    let pool = crate::require_test_pool().await;
    setup(&pool).await;
    let service = create_service();
    let threepid_storage = create_threepid_storage(&pool);
    let uid = unique_id();
    let user_id = format!("@uiaemail{uid}:localhost");
    insert_user(&pool, &user_id, &format!("uiaemail{uid}")).await;

    let auth = json!({"threepidCreds": [{"sid": "s1"}]});
    let result = service.verify_email_identity_stage(&auth, &user_id, &threepid_storage).await;
    assert!(result.is_err());
    let e = result.unwrap_err();
    assert!(e.is_bad_request());
    assert!(e.internal_message().contains("client_secret required"));

    cleanup_user(&pool, &user_id).await;
}

#[tokio::test]
async fn test_email_identity_empty_sid() {
    let _guard = uia_test_guard().lock().unwrap_or_else(|e| e.into_inner());
    let pool = crate::require_test_pool().await;
    setup(&pool).await;
    let service = create_service();
    let threepid_storage = create_threepid_storage(&pool);
    let uid = unique_id();
    let user_id = format!("@uiaemail{uid}:localhost");
    insert_user(&pool, &user_id, &format!("uiaemail{uid}")).await;

    let auth = json!({"threepidCreds": [{"sid": "", "client_secret": "cs"}]});
    let result = service.verify_email_identity_stage(&auth, &user_id, &threepid_storage).await;
    assert!(result.is_err());
    let e = result.unwrap_err();
    assert!(e.is_bad_request());
    assert!(e.internal_message().contains("must not be empty"));

    cleanup_user(&pool, &user_id).await;
}

#[tokio::test]
async fn test_email_identity_empty_client_secret() {
    let _guard = uia_test_guard().lock().unwrap_or_else(|e| e.into_inner());
    let pool = crate::require_test_pool().await;
    setup(&pool).await;
    let service = create_service();
    let threepid_storage = create_threepid_storage(&pool);
    let uid = unique_id();
    let user_id = format!("@uiaemail{uid}:localhost");
    insert_user(&pool, &user_id, &format!("uiaemail{uid}")).await;

    let auth = json!({"threepidCreds": [{"sid": "s1", "client_secret": ""}]});
    let result = service.verify_email_identity_stage(&auth, &user_id, &threepid_storage).await;
    assert!(result.is_err());
    let e = result.unwrap_err();
    assert!(e.is_bad_request());
    assert!(e.internal_message().contains("must not be empty"));

    cleanup_user(&pool, &user_id).await;
}

#[tokio::test]
async fn test_email_identity_no_verified_email() {
    let _guard = uia_test_guard().lock().unwrap_or_else(|e| e.into_inner());
    let pool = crate::require_test_pool().await;
    setup(&pool).await;
    let service = create_service();
    let threepid_storage = create_threepid_storage(&pool);
    let uid = unique_id();
    let user_id = format!("@uiaemail{uid}:localhost");
    insert_user(&pool, &user_id, &format!("uiaemail{uid}")).await;
    // No threepid added — user has no verified email.

    let auth = json!({"threepidCreds": valid_threepid_creds()});
    let result = service.verify_email_identity_stage(&auth, &user_id, &threepid_storage).await;
    assert!(result.is_err());
    let e = result.unwrap_err();
    assert!(e.is_forbidden());
    assert!(e.internal_message().contains("No verified email"));

    cleanup_user(&pool, &user_id).await;
}

#[tokio::test]
async fn test_email_identity_success_with_verified_email() {
    let _guard = uia_test_guard().lock().unwrap_or_else(|e| e.into_inner());
    let pool = crate::require_test_pool().await;
    setup(&pool).await;
    let service = create_service();
    let threepid_storage = create_threepid_storage(&pool);
    let uid = unique_id();
    let user_id = format!("@uiaemail{uid}:localhost");
    insert_user(&pool, &user_id, &format!("uiaemail{uid}")).await;

    let now = chrono::Utc::now().timestamp_millis();
    threepid_storage
        .add_verified_threepid(&user_id, "email", &format!("user{uid}@example.com"), now, now)
        .await
        .expect("add_verified_threepid should succeed");

    let auth = json!({"threepidCreds": valid_threepid_creds()});
    let result = service.verify_email_identity_stage(&auth, &user_id, &threepid_storage).await;
    assert!(result.is_ok(), "expected success with verified email, got: {:?}", result);

    cleanup_user(&pool, &user_id).await;
}

#[tokio::test]
async fn test_email_identity_snake_case_creds_key() {
    let _guard = uia_test_guard().lock().unwrap_or_else(|e| e.into_inner());
    let pool = crate::require_test_pool().await;
    setup(&pool).await;
    let service = create_service();
    let threepid_storage = create_threepid_storage(&pool);
    let uid = unique_id();
    let user_id = format!("@uiaemail{uid}:localhost");
    insert_user(&pool, &user_id, &format!("uiaemail{uid}")).await;

    let now = chrono::Utc::now().timestamp_millis();
    threepid_storage
        .add_verified_threepid(&user_id, "email", &format!("user{uid}@example.com"), now, now)
        .await
        .unwrap();

    // Use snake_case key `threepid_creds` instead of camelCase `threepidCreds`.
    let auth = json!({"threepid_creds": valid_threepid_creds()});
    let result = service.verify_email_identity_stage(&auth, &user_id, &threepid_storage).await;
    assert!(result.is_ok(), "snake_case threepid_creds should be accepted");

    cleanup_user(&pool, &user_id).await;
}

// =============================================================================
// verify_msisdn_stage
// =============================================================================

#[tokio::test]
async fn test_msisdn_missing_threepid_creds() {
    let _guard = uia_test_guard().lock().unwrap_or_else(|e| e.into_inner());
    let pool = crate::require_test_pool().await;
    setup(&pool).await;
    let service = create_service();
    let threepid_storage = create_threepid_storage(&pool);
    let uid = unique_id();
    let user_id = format!("@uiamsisdn{uid}:localhost");
    insert_user(&pool, &user_id, &format!("uiamsisdn{uid}")).await;

    let auth = json!({"type": "m.login.msisdn"});
    let result = service.verify_msisdn_stage(&auth, &user_id, &threepid_storage).await;
    assert!(result.is_err());
    let e = result.unwrap_err();
    assert!(e.is_bad_request());
    assert!(e.internal_message().contains("threepidCreds array required"));

    cleanup_user(&pool, &user_id).await;
}

#[tokio::test]
async fn test_msisdn_missing_sid() {
    let _guard = uia_test_guard().lock().unwrap_or_else(|e| e.into_inner());
    let pool = crate::require_test_pool().await;
    setup(&pool).await;
    let service = create_service();
    let threepid_storage = create_threepid_storage(&pool);
    let uid = unique_id();
    let user_id = format!("@uiamsisdn{uid}:localhost");
    insert_user(&pool, &user_id, &format!("uiamsisdn{uid}")).await;

    let auth = json!({"threepidCreds": [{"client_secret": "cs"}]});
    let result = service.verify_msisdn_stage(&auth, &user_id, &threepid_storage).await;
    assert!(result.is_err());
    let e = result.unwrap_err();
    assert!(e.is_bad_request());
    assert!(e.internal_message().contains("sid required"));

    cleanup_user(&pool, &user_id).await;
}

#[tokio::test]
async fn test_msisdn_missing_client_secret() {
    let _guard = uia_test_guard().lock().unwrap_or_else(|e| e.into_inner());
    let pool = crate::require_test_pool().await;
    setup(&pool).await;
    let service = create_service();
    let threepid_storage = create_threepid_storage(&pool);
    let uid = unique_id();
    let user_id = format!("@uiamsisdn{uid}:localhost");
    insert_user(&pool, &user_id, &format!("uiamsisdn{uid}")).await;

    let auth = json!({"threepidCreds": [{"sid": "s1"}]});
    let result = service.verify_msisdn_stage(&auth, &user_id, &threepid_storage).await;
    assert!(result.is_err());
    let e = result.unwrap_err();
    assert!(e.is_bad_request());
    assert!(e.internal_message().contains("client_secret required"));

    cleanup_user(&pool, &user_id).await;
}

#[tokio::test]
async fn test_msisdn_empty_values() {
    let _guard = uia_test_guard().lock().unwrap_or_else(|e| e.into_inner());
    let pool = crate::require_test_pool().await;
    setup(&pool).await;
    let service = create_service();
    let threepid_storage = create_threepid_storage(&pool);
    let uid = unique_id();
    let user_id = format!("@uiamsisdn{uid}:localhost");
    insert_user(&pool, &user_id, &format!("uiamsisdn{uid}")).await;

    let auth = json!({"threepidCreds": [{"sid": "", "client_secret": ""}]});
    let result = service.verify_msisdn_stage(&auth, &user_id, &threepid_storage).await;
    assert!(result.is_err());
    let e = result.unwrap_err();
    assert!(e.is_bad_request());
    assert!(e.internal_message().contains("must not be empty"));

    cleanup_user(&pool, &user_id).await;
}

#[tokio::test]
async fn test_msisdn_no_verified_phone() {
    let _guard = uia_test_guard().lock().unwrap_or_else(|e| e.into_inner());
    let pool = crate::require_test_pool().await;
    setup(&pool).await;
    let service = create_service();
    let threepid_storage = create_threepid_storage(&pool);
    let uid = unique_id();
    let user_id = format!("@uiamsisdn{uid}:localhost");
    insert_user(&pool, &user_id, &format!("uiamsisdn{uid}")).await;
    // No msisdn threepid added.

    let auth = json!({"threepidCreds": valid_threepid_creds()});
    let result = service.verify_msisdn_stage(&auth, &user_id, &threepid_storage).await;
    assert!(result.is_err());
    let e = result.unwrap_err();
    assert!(e.is_forbidden());
    assert!(e.internal_message().contains("No verified phone number"));

    cleanup_user(&pool, &user_id).await;
}

#[tokio::test]
async fn test_msisdn_success_with_verified_phone() {
    let _guard = uia_test_guard().lock().unwrap_or_else(|e| e.into_inner());
    let pool = crate::require_test_pool().await;
    setup(&pool).await;
    let service = create_service();
    let threepid_storage = create_threepid_storage(&pool);
    let uid = unique_id();
    let user_id = format!("@uiamsisdn{uid}:localhost");
    insert_user(&pool, &user_id, &format!("uiamsisdn{uid}")).await;

    let now = chrono::Utc::now().timestamp_millis();
    threepid_storage
        .add_verified_threepid(&user_id, "msisdn", &format!("+1555000{uid}"), now, now)
        .await
        .expect("add_verified_threepid should succeed");

    let auth = json!({"threepidCreds": valid_threepid_creds()});
    let result = service.verify_msisdn_stage(&auth, &user_id, &threepid_storage).await;
    assert!(result.is_ok(), "expected success with verified msisdn, got: {:?}", result);

    cleanup_user(&pool, &user_id).await;
}

#[tokio::test]
async fn test_msisdn_verified_email_not_accepted() {
    let _guard = uia_test_guard().lock().unwrap_or_else(|e| e.into_inner());
    let pool = crate::require_test_pool().await;
    setup(&pool).await;
    let service = create_service();
    let threepid_storage = create_threepid_storage(&pool);
    let uid = unique_id();
    let user_id = format!("@uiamsisdn{uid}:localhost");
    insert_user(&pool, &user_id, &format!("uiamsisdn{uid}")).await;

    // User has a verified EMAIL but no verified MSISDN — msisdn stage must still fail.
    let now = chrono::Utc::now().timestamp_millis();
    threepid_storage
        .add_verified_threepid(&user_id, "email", &format!("user{uid}@example.com"), now, now)
        .await
        .unwrap();

    let auth = json!({"threepidCreds": valid_threepid_creds()});
    let result = service.verify_msisdn_stage(&auth, &user_id, &threepid_storage).await;
    assert!(result.is_err(), "verified email must not satisfy msisdn stage");
    let e = result.unwrap_err();
    assert!(e.is_forbidden());
    assert!(e.internal_message().contains("No verified phone number"));

    cleanup_user(&pool, &user_id).await;
}

// =============================================================================
// verify_password_stage — success path and identifier resolution
// =============================================================================

#[tokio::test]
async fn test_password_stage_success() {
    let _guard = uia_test_guard().lock().unwrap_or_else(|e| e.into_inner());
    let pool = crate::require_test_pool().await;
    setup(&pool).await;
    let service = create_service();
    let auth_service = create_auth_service(&pool);
    let uid = unique_id();
    let (user_id, _token) = register_test_user(&auth_service, uid).await;

    let auth = json!({
        "password": TEST_PASSWORD,
        "identifier": {"user": user_id}
    });
    let result = service.verify_password_stage(&auth, &user_id, &auth_service).await;
    assert!(result.is_ok(), "valid password should succeed, got: {:?}", result);

    cleanup_user(&pool, &user_id).await;
}

#[tokio::test]
async fn test_password_stage_invalid_password() {
    let _guard = uia_test_guard().lock().unwrap_or_else(|e| e.into_inner());
    let pool = crate::require_test_pool().await;
    setup(&pool).await;
    let service = create_service();
    let auth_service = create_auth_service(&pool);
    let uid = unique_id();
    let (user_id, _token) = register_test_user(&auth_service, uid).await;

    let auth = json!({
        "password": "WrongPass@456",
        "identifier": {"user": user_id}
    });
    let result = service.verify_password_stage(&auth, &user_id, &auth_service).await;
    assert!(result.is_err());
    let e = result.unwrap_err();
    assert!(e.is_forbidden());
    assert!(e.internal_message().contains("Invalid password"));

    cleanup_user(&pool, &user_id).await;
}

#[tokio::test]
async fn test_password_stage_localpart_resolution() {
    let _guard = uia_test_guard().lock().unwrap_or_else(|e| e.into_inner());
    let pool = crate::require_test_pool().await;
    setup(&pool).await;
    let service = create_service();
    let auth_service = create_auth_service(&pool);
    let uid = unique_id();
    let (user_id, _token) = register_test_user(&auth_service, uid).await;
    // user_id is "@uiauser{uid}:localhost"; pass the localpart only.
    let localpart = &user_id[1..user_id.find(':').unwrap_or(user_id.len())];

    let auth = json!({
        "password": TEST_PASSWORD,
        "identifier": {"user": localpart}
    });
    let result = service.verify_password_stage(&auth, &user_id, &auth_service).await;
    assert!(result.is_ok(), "localpart should resolve to full MXID, got: {:?}", result);

    cleanup_user(&pool, &user_id).await;
}

#[tokio::test]
async fn test_password_stage_user_field_fallback() {
    let _guard = uia_test_guard().lock().unwrap_or_else(|e| e.into_inner());
    let pool = crate::require_test_pool().await;
    setup(&pool).await;
    let service = create_service();
    let auth_service = create_auth_service(&pool);
    let uid = unique_id();
    let (user_id, _token) = register_test_user(&auth_service, uid).await;

    // No `identifier` block — fall back to the top-level `user` field.
    let auth = json!({
        "password": TEST_PASSWORD,
        "user": user_id
    });
    let result = service.verify_password_stage(&auth, &user_id, &auth_service).await;
    assert!(result.is_ok(), "`user` field fallback should work, got: {:?}", result);

    cleanup_user(&pool, &user_id).await;
}

#[tokio::test]
async fn test_password_stage_user_id_field_fallback() {
    let _guard = uia_test_guard().lock().unwrap_or_else(|e| e.into_inner());
    let pool = crate::require_test_pool().await;
    setup(&pool).await;
    let service = create_service();
    let auth_service = create_auth_service(&pool);
    let uid = unique_id();
    let (user_id, _token) = register_test_user(&auth_service, uid).await;

    // Neither `identifier` nor `user` — fall back to `user_id` field.
    let auth = json!({
        "password": TEST_PASSWORD,
        "user_id": user_id
    });
    let result = service.verify_password_stage(&auth, &user_id, &auth_service).await;
    assert!(result.is_ok(), "`user_id` field fallback should work, got: {:?}", result);

    cleanup_user(&pool, &user_id).await;
}

#[tokio::test]
async fn test_password_stage_non_string_password() {
    let _guard = uia_test_guard().lock().unwrap_or_else(|e| e.into_inner());
    let pool = crate::require_test_pool().await;
    setup(&pool).await;
    let service = create_service();
    let auth_service = create_auth_service(&pool);
    let uid = unique_id();
    let (user_id, _token) = register_test_user(&auth_service, uid).await;

    // password is a number, not a string — as_str() returns None.
    let auth = json!({
        "password": 12345,
        "identifier": {"user": user_id}
    });
    let result = service.verify_password_stage(&auth, &user_id, &auth_service).await;
    assert!(result.is_err());
    let e = result.unwrap_err();
    assert!(e.is_bad_request());
    assert!(e.internal_message().contains("Password required"));

    cleanup_user(&pool, &user_id).await;
}

// =============================================================================
// verify_token_stage — success path, cross-user, invalid token
// =============================================================================

#[tokio::test]
async fn test_token_stage_success() {
    let _guard = uia_test_guard().lock().unwrap_or_else(|e| e.into_inner());
    let pool = crate::require_test_pool().await;
    setup(&pool).await;
    let service = create_service();
    let auth_service = create_auth_service(&pool);
    let uid = unique_id();
    let (user_id, access_token) = register_test_user(&auth_service, uid).await;

    let auth = json!({
        "token": access_token,
        "txn_id": "txn-success-001"
    });
    let result = service.verify_token_stage(&auth, &user_id, &auth_service).await;
    assert!(result.is_ok(), "valid token should succeed, got: {:?}", result);

    cleanup_user(&pool, &user_id).await;
}

#[tokio::test]
async fn test_token_stage_token_different_user() {
    let _guard = uia_test_guard().lock().unwrap_or_else(|e| e.into_inner());
    let pool = crate::require_test_pool().await;
    setup(&pool).await;
    let service = create_service();
    let auth_service = create_auth_service(&pool);
    let uid_a = unique_id();
    let uid_b = unique_id();
    let (user_id_a, _token_a) = register_test_user(&auth_service, uid_a).await;
    let (user_id_b, token_b) = register_test_user(&auth_service, uid_b).await;

    // Token belongs to user B but we claim to be user A.
    let auth = json!({
        "token": token_b,
        "txn_id": "txn-cross-user-001"
    });
    let result = service.verify_token_stage(&auth, &user_id_a, &auth_service).await;
    assert!(result.is_err());
    let e = result.unwrap_err();
    assert!(e.is_forbidden());
    assert!(e.internal_message().contains("Token belongs to a different user"));

    cleanup_user(&pool, &user_id_a).await;
    cleanup_user(&pool, &user_id_b).await;
}

#[tokio::test]
async fn test_token_stage_invalid_token() {
    let _guard = uia_test_guard().lock().unwrap_or_else(|e| e.into_inner());
    let pool = crate::require_test_pool().await;
    setup(&pool).await;
    let service = create_service();
    let auth_service = create_auth_service(&pool);
    let uid = unique_id();
    let (user_id, _token) = register_test_user(&auth_service, uid).await;

    let auth = json!({
        "token": "this-is-not-a-valid-jwt",
        "txn_id": "txn-invalid-001"
    });
    let result = service.verify_token_stage(&auth, &user_id, &auth_service).await;
    assert!(result.is_err());
    let e = result.unwrap_err();
    assert!(e.is_forbidden());
    assert!(e.internal_message().contains("Invalid or expired token"));

    cleanup_user(&pool, &user_id).await;
}

// =============================================================================
// require_uia — end-to-end orchestration
// =============================================================================

#[tokio::test]
async fn test_require_uia_no_auth() {
    let _guard = uia_test_guard().lock().unwrap_or_else(|e| e.into_inner());
    let pool = crate::require_test_pool().await;
    setup(&pool).await;
    let service = create_service();
    let auth_service = create_auth_service(&pool);
    let threepid_storage = create_threepid_storage(&pool);
    let uid = unique_id();
    let user_id = format!("@uiauser{uid}:localhost");
    insert_user(&pool, &user_id, &format!("uiauser{uid}")).await;

    let flows = UiaService::get_default_flows();
    let result = service.require_uia(None, &user_id, flows, &auth_service, &threepid_storage).await;
    assert!(result.is_err());
    let err = result.unwrap_err();
    assert_eq!(err["errcode"], "M_UIA_REQUIRED");
    assert!(err["session"].is_string());
    assert!(err["flows"].is_array());

    cleanup_user(&pool, &user_id).await;
}

#[tokio::test]
async fn test_require_uia_password_success() {
    let _guard = uia_test_guard().lock().unwrap_or_else(|e| e.into_inner());
    let pool = crate::require_test_pool().await;
    setup(&pool).await;
    let service = create_service();
    let auth_service = create_auth_service(&pool);
    let threepid_storage = create_threepid_storage(&pool);
    let uid = unique_id();
    let (user_id, _token) = register_test_user(&auth_service, uid).await;

    let flows = vec![UiaFlow { stages: vec!["m.login.password".to_string()] }];
    let session = service.create_session(&user_id, flows.clone()).await;
    let auth = json!({
        "type": "m.login.password",
        "session": session.session_id,
        "password": TEST_PASSWORD
    });
    let result = service
        .require_uia(Some(&auth), &user_id, flows, &auth_service, &threepid_storage)
        .await;
    assert!(result.is_ok(), "password flow should complete, got: {:?}", result);

    cleanup_user(&pool, &user_id).await;
}

#[tokio::test]
async fn test_require_uia_password_failure() {
    let _guard = uia_test_guard().lock().unwrap_or_else(|e| e.into_inner());
    let pool = crate::require_test_pool().await;
    setup(&pool).await;
    let service = create_service();
    let auth_service = create_auth_service(&pool);
    let threepid_storage = create_threepid_storage(&pool);
    let uid = unique_id();
    let (user_id, _token) = register_test_user(&auth_service, uid).await;

    let flows = vec![UiaFlow { stages: vec!["m.login.password".to_string()] }];
    let session = service.create_session(&user_id, flows.clone()).await;
    let auth = json!({
        "type": "m.login.password",
        "session": session.session_id,
        "password": "WrongPass@456"
    });
    let result = service
        .require_uia(Some(&auth), &user_id, flows, &auth_service, &threepid_storage)
        .await;
    assert!(result.is_err());
    let err = result.unwrap_err();
    assert_eq!(err["errcode"], "M_FORBIDDEN");
    // A new session is created for the retry.
    assert!(err["session"].is_string());

    cleanup_user(&pool, &user_id).await;
}

#[tokio::test]
async fn test_require_uia_token_success() {
    let _guard = uia_test_guard().lock().unwrap_or_else(|e| e.into_inner());
    let pool = crate::require_test_pool().await;
    setup(&pool).await;
    let service = create_service();
    let auth_service = create_auth_service(&pool);
    let threepid_storage = create_threepid_storage(&pool);
    let uid = unique_id();
    let (user_id, access_token) = register_test_user(&auth_service, uid).await;

    let flows = vec![UiaFlow { stages: vec!["m.login.token".to_string()] }];
    let session = service.create_session(&user_id, flows.clone()).await;
    let auth = json!({
        "type": "m.login.token",
        "session": session.session_id,
        "token": access_token,
        "txn_id": "require-uia-token-001"
    });
    let result = service
        .require_uia(Some(&auth), &user_id, flows, &auth_service, &threepid_storage)
        .await;
    assert!(result.is_ok(), "token flow should complete, got: {:?}", result);

    cleanup_user(&pool, &user_id).await;
}

#[tokio::test]
async fn test_require_uia_email_identity_failure() {
    let _guard = uia_test_guard().lock().unwrap_or_else(|e| e.into_inner());
    let pool = crate::require_test_pool().await;
    setup(&pool).await;
    let service = create_service();
    let auth_service = create_auth_service(&pool);
    let threepid_storage = create_threepid_storage(&pool);
    let uid = unique_id();
    let user_id = format!("@uiauser{uid}:localhost");
    insert_user(&pool, &user_id, &format!("uiauser{uid}")).await;
    // No verified email — the email identity stage will fail.

    let flows = vec![UiaFlow { stages: vec!["m.login.email.identity".to_string()] }];
    let session = service.create_session(&user_id, flows.clone()).await;
    let auth = json!({
        "type": "m.login.email.identity",
        "session": session.session_id,
        "threepidCreds": valid_threepid_creds()
    });
    let result = service
        .require_uia(Some(&auth), &user_id, flows, &auth_service, &threepid_storage)
        .await;
    assert!(result.is_err());
    let err = result.unwrap_err();
    assert_eq!(err["errcode"], "M_FORBIDDEN");
    assert!(err["error"].as_str().unwrap().contains("No verified email"));

    cleanup_user(&pool, &user_id).await;
}

#[tokio::test]
async fn test_require_uia_msisdn_failure() {
    let _guard = uia_test_guard().lock().unwrap_or_else(|e| e.into_inner());
    let pool = crate::require_test_pool().await;
    setup(&pool).await;
    let service = create_service();
    let auth_service = create_auth_service(&pool);
    let threepid_storage = create_threepid_storage(&pool);
    let uid = unique_id();
    let user_id = format!("@uiauser{uid}:localhost");
    insert_user(&pool, &user_id, &format!("uiauser{uid}")).await;
    // No verified phone — the msisdn stage will fail.

    let flows = vec![UiaFlow { stages: vec!["m.login.msisdn".to_string()] }];
    let session = service.create_session(&user_id, flows.clone()).await;
    let auth = json!({
        "type": "m.login.msisdn",
        "session": session.session_id,
        "threepidCreds": valid_threepid_creds()
    });
    let result = service
        .require_uia(Some(&auth), &user_id, flows, &auth_service, &threepid_storage)
        .await;
    assert!(result.is_err());
    let err = result.unwrap_err();
    assert_eq!(err["errcode"], "M_FORBIDDEN");
    assert!(err["error"].as_str().unwrap().contains("No verified phone number"));

    cleanup_user(&pool, &user_id).await;
}

#[tokio::test]
async fn test_require_uia_unsupported_type() {
    let _guard = uia_test_guard().lock().unwrap_or_else(|e| e.into_inner());
    let pool = crate::require_test_pool().await;
    setup(&pool).await;
    let service = create_service();
    let auth_service = create_auth_service(&pool);
    let threepid_storage = create_threepid_storage(&pool);
    let uid = unique_id();
    let user_id = format!("@uiauser{uid}:localhost");
    insert_user(&pool, &user_id, &format!("uiauser{uid}")).await;

    // Flow only allows password, but the client sends m.login.dummy.
    let flows = vec![UiaFlow { stages: vec!["m.login.password".to_string()] }];
    let session = service.create_session(&user_id, flows.clone()).await;
    let auth = json!({
        "type": "m.login.dummy",
        "session": session.session_id
    });
    let result = service
        .require_uia(Some(&auth), &user_id, flows, &auth_service, &threepid_storage)
        .await;
    assert!(result.is_err());
    let err = result.unwrap_err();
    // validate_auth rejects the unsupported stage with M_INVALID_PARAM.
    assert_eq!(err["errcode"], "M_INVALID_PARAM");
    assert!(err["error"].as_str().unwrap().contains("Unsupported auth type"));

    cleanup_user(&pool, &user_id).await;
}
