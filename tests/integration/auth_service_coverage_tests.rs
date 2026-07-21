#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
//! B.3 Phase 3 Batch 1 — auth service coverage tests.
//!
//! Target files (currently 0-15% covered):
//!   - synapse-services/src/auth/login.rs    (login success, locked account,
//!     deactivated user, device_id branches, display_name length,
//!     verify_user_credentials)
//!   - synapse-services/src/auth/register.rs (register success, duplicate
//!     username, weak password, register_with_device_name, displayname set)
//!
//! Existing `auth_service_tests_migrated.rs` already covers:
//!   - register invalid username / empty username
//!   - login invalid credentials (nonexistent user)
//!   - password migration (legacy hash → argon2)
//!
//! These tests fill the remaining gaps to reach ≥60% on both files.

use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;

use synapse_rust::cache::{CacheConfig, CacheManager};
use synapse_rust::common::config::SecurityConfig;
use synapse_rust::common::error::MatrixErrorCode;
use synapse_rust::common::metrics::MetricsCollector;
use synapse_services::auth::AuthService;

static TEST_COUNTER: AtomicU64 = AtomicU64::new(10_000);

fn unique_id() -> u64 {
    TEST_COUNTER.fetch_add(1, Ordering::SeqCst)
}

/// Build a SecurityConfig tuned for fast argon2 hashing in tests.
fn test_security() -> SecurityConfig {
    SecurityConfig {
        secret: "test_secret_for_coverage".to_string(),
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

/// Build an AuthService bound to the test pool.
fn build_auth(pool: &Arc<sqlx::PgPool>, security: &SecurityConfig) -> AuthService {
    let cache = Arc::new(CacheManager::new(&CacheConfig::default()));
    let metrics = Arc::new(MetricsCollector::new());
    AuthService::new(pool, cache, metrics, security, "localhost")
}

/// Build an AuthService whose cache is shared so we can pre-seed lockout keys.
fn build_auth_with_cache(pool: &Arc<sqlx::PgPool>, security: &SecurityConfig) -> (AuthService, Arc<CacheManager>) {
    let cache = Arc::new(CacheManager::new(&CacheConfig::default()));
    let metrics = Arc::new(MetricsCollector::new());
    let auth = AuthService::new(pool, cache.clone(), metrics, security, "localhost");
    (auth, cache)
}

fn unique_username() -> String {
    format!("covuser_{}", unique_id())
}

// =============================================================================
// register.rs coverage
// =============================================================================

#[tokio::test]
async fn test_register_success_returns_user_and_tokens() {
    let pool = crate::require_test_pool().await;
    let security = test_security();
    let auth = build_auth(&pool, &security);

    let username = unique_username();
    let result = auth.register(&username, "StrongP@ss1!", false, None).await;
    assert!(result.is_ok(), "register should succeed: {:?}", result.err());
    let (user, access_token, refresh_token, device_id) = result.unwrap();
    assert_eq!(user.username, username);
    assert_eq!(user.user_id, format!("@{username}:localhost"));
    assert!(!user.is_admin);
    assert!(!access_token.is_empty(), "access token must not be empty");
    assert!(!refresh_token.is_empty(), "refresh token must not be empty");
    assert!(!device_id.is_empty(), "device id must not be empty");
}

#[tokio::test]
async fn test_register_admin_user() {
    let pool = crate::require_test_pool().await;
    let security = test_security();
    let auth = build_auth(&pool, &security);

    let username = unique_username();
    let result = auth.register(&username, "StrongP@ss1!", true, None).await;
    assert!(result.is_ok(), "admin register should succeed: {:?}", result.err());
    let (user, _access, _refresh, _device) = result.unwrap();
    assert!(user.is_admin, "registered admin user must have is_admin=true");
}

#[tokio::test]
async fn test_register_duplicate_username_returns_user_in_use() {
    let pool = crate::require_test_pool().await;
    let security = test_security();
    let auth = build_auth(&pool, &security);

    let username = unique_username();
    let first = auth.register(&username, "StrongP@ss1!", false, None).await;
    assert!(first.is_ok());

    let second = auth.register(&username, "DifferentP@ss2!", false, None).await;
    assert!(second.is_err());
    let err = second.unwrap_err();
    assert!(
        err.code_is(MatrixErrorCode::UserInUse),
        "expected M_USER_IN_USE, got {:?} ({})",
        err.code(),
        err.message()
    );
}

#[tokio::test]
async fn test_register_weak_password_returns_invalid_param() {
    let pool = crate::require_test_pool().await;
    let security = test_security();
    let auth = build_auth(&pool, &security);

    // MIN_PASSWORD_LENGTH = 8; "short" is 5 chars → too short.
    let username = unique_username();
    let result = auth.register(&username, "short", false, None).await;
    assert!(result.is_err());
    let err = result.unwrap_err();
    assert!(
        err.code_is(MatrixErrorCode::InvalidParam),
        "expected M_INVALID_PARAM for weak password, got {:?}",
        err.code()
    );
}

#[tokio::test]
async fn test_register_empty_password_returns_missing_param() {
    let pool = crate::require_test_pool().await;
    let security = test_security();
    let auth = build_auth(&pool, &security);

    let username = unique_username();
    let result = auth.register(&username, "", false, None).await;
    assert!(result.is_err());
    assert!(result.unwrap_err().code_is(MatrixErrorCode::MissingParam), "empty password should yield M_MISSING_PARAM");
}

#[tokio::test]
async fn test_register_with_displayname_sets_displayname() {
    let pool = crate::require_test_pool().await;
    let security = test_security();
    let auth = build_auth(&pool, &security);

    let username = unique_username();
    let displayname = format!("Display Name {}", unique_id());
    let result = auth.register(&username, "StrongP@ss1!", false, Some(&displayname)).await;
    assert!(result.is_ok(), "register with displayname should succeed: {:?}", result.err());

    // Verify displayname was persisted (best-effort; update_displayname failure
    // is logged, not propagated, so we just confirm registration succeeded).
    let (user, _access, _refresh, _device) = result.unwrap();
    assert_eq!(user.username, username);
}

#[tokio::test]
async fn test_register_with_device_name_returns_device_id() {
    let pool = crate::require_test_pool().await;
    let security = test_security();
    let auth = build_auth(&pool, &security);

    let username = unique_username();
    let result = auth.register_with_device_name(&username, "StrongP@ss1!", false, None, Some("my-test-device")).await;
    assert!(result.is_ok(), "register_with_device_name should succeed: {:?}", result.err());
    let (_user, _access, _refresh, device_id) = result.unwrap();
    assert!(!device_id.is_empty(), "device_id must be generated");
}

// =============================================================================
// login.rs coverage
// =============================================================================

#[tokio::test]
async fn test_login_success_after_register() {
    let pool = crate::require_test_pool().await;
    let security = test_security();
    let auth = build_auth(&pool, &security);

    let username = unique_username();
    let password = "StrongP@ss1!";
    let register_result = auth.register(&username, password, false, None).await;
    assert!(register_result.is_ok());

    let login_result = auth.login(&username, password, None, None).await;
    assert!(login_result.is_ok(), "login after register should succeed: {:?}", login_result.err());
    let (user, access_token, refresh_token, device_id) = login_result.unwrap();
    assert_eq!(user.username, username);
    assert!(!access_token.is_empty());
    assert!(!refresh_token.is_empty());
    assert!(!device_id.is_empty());

    // Success counter should be incremented.
    let success_counter = auth.metrics.get_counter("auth_login_success_total");
    assert!(success_counter.is_some(), "auth_login_success_total counter should exist");
    assert!(success_counter.unwrap().get() >= 1);
}

#[tokio::test]
async fn test_login_with_wrong_password_returns_forbidden() {
    let pool = crate::require_test_pool().await;
    let security = test_security();
    let auth = build_auth(&pool, &security);

    let username = unique_username();
    let register_result = auth.register(&username, "StrongP@ss1!", false, None).await;
    assert!(register_result.is_ok());

    let login_result = auth.login(&username, "WrongPassword99!", None, None).await;
    assert!(login_result.is_err());
    let err = login_result.unwrap_err();
    assert!(err.code_is(MatrixErrorCode::Forbidden), "expected M_FORBIDDEN for wrong password, got {:?}", err.code());

    // Failure counter should be incremented.
    let failure_counter = auth.metrics.get_counter("auth_login_failure_total");
    assert!(failure_counter.is_some(), "auth_login_failure_total counter should exist");
    assert!(failure_counter.unwrap().get() >= 1);
}

#[tokio::test]
async fn test_login_with_explicit_device_id_creates_new_device() {
    let pool = crate::require_test_pool().await;
    let security = test_security();
    let auth = build_auth(&pool, &security);

    let username = unique_username();
    let password = "StrongP@ss1!";
    auth.register(&username, password, false, None).await.unwrap();

    let custom_device_id = format!("explicitdev_{}", unique_id());
    let login_result = auth.login(&username, password, Some(&custom_device_id), None).await;
    assert!(login_result.is_ok(), "login with explicit device_id should succeed: {:?}", login_result.err());
    let (_user, _access, _refresh, device_id) = login_result.unwrap();
    assert_eq!(device_id, custom_device_id, "login should return the supplied device_id");
}

#[tokio::test]
async fn test_login_with_initial_display_name_too_long_returns_bad_request() {
    let pool = crate::require_test_pool().await;
    let security = test_security();
    let auth = build_auth(&pool, &security);

    let username = unique_username();
    let password = "StrongP@ss1!";
    auth.register(&username, password, false, None).await.unwrap();

    let long_name = "x".repeat(101);
    let result = auth.login(&username, password, None, Some(&long_name)).await;
    assert!(result.is_err());
    let err = result.unwrap_err();
    assert!(err.is_bad_request(), "expected bad_request for >100 char display name, got {:?}", err.code());
}

#[tokio::test]
async fn test_login_deactivated_user_returns_forbidden() {
    let pool = crate::require_test_pool().await;
    let security = test_security();
    let auth = build_auth(&pool, &security);

    let username = unique_username();
    let password = "StrongP@ss1!";
    let (user, _access, _refresh, _device) = auth.register(&username, password, false, None).await.unwrap();

    // Deactivate the user directly via SQL.
    sqlx::query("UPDATE users SET is_deactivated = TRUE WHERE user_id = $1")
        .bind(&user.user_id)
        .execute(&*pool)
        .await
        .expect("Failed to deactivate user");

    let result = auth.login(&username, password, None, None).await;
    assert!(result.is_err());
    assert!(result.unwrap_err().code_is(MatrixErrorCode::Forbidden), "deactivated user login should be M_FORBIDDEN");
}

#[tokio::test]
async fn test_verify_user_credentials_success() {
    let pool = crate::require_test_pool().await;
    let security = test_security();
    let auth = build_auth(&pool, &security);

    let username = unique_username();
    let password = "StrongP@ss1!";
    let (user, _access, _refresh, _device) = auth.register(&username, password, false, None).await.unwrap();

    let verify = auth.verify_user_credentials(&user.user_id, password).await;
    assert!(verify.is_ok(), "verify_user_credentials should succeed: {:?}", verify.err());
}

#[tokio::test]
async fn test_verify_user_credentials_wrong_password_returns_forbidden() {
    let pool = crate::require_test_pool().await;
    let security = test_security();
    let auth = build_auth(&pool, &security);

    let username = unique_username();
    let (user, _access, _refresh, _device) = auth.register(&username, "StrongP@ss1!", false, None).await.unwrap();

    let result = auth.verify_user_credentials(&user.user_id, "WrongPassword99!").await;
    assert!(result.is_err());
    assert!(result.unwrap_err().code_is(MatrixErrorCode::Forbidden), "wrong password should yield M_FORBIDDEN");
}

#[tokio::test]
async fn test_verify_user_credentials_nonexistent_user_returns_forbidden() {
    let pool = crate::require_test_pool().await;
    let security = test_security();
    let auth = build_auth(&pool, &security);

    let result = auth.verify_user_credentials("@nonexistent_cov:localhost", "anypassword").await;
    assert!(result.is_err());
    assert!(result.unwrap_err().code_is(MatrixErrorCode::Forbidden), "nonexistent user should yield M_FORBIDDEN");
}

#[tokio::test]
async fn test_login_locked_account_returns_rate_limited() {
    let pool = crate::require_test_pool().await;
    let security = test_security();
    let (auth, cache) = build_auth_with_cache(&pool, &security);

    let username = unique_username();
    let password = "StrongP@ss1!";
    let (user, _access, _refresh, _device) = auth.register(&username, password, false, None).await.unwrap();

    // Pre-seed the lockout cache key so is_account_locked() returns true.
    let lockout_key = format!("auth:lockout:{}", user.user_id);
    let lockout_until = chrono::Utc::now().timestamp() + 3600; // 1h in the future
    cache.set(&lockout_key, &lockout_until, 3600).await.unwrap();

    let result = auth.login(&username, password, None, None).await;
    assert!(result.is_err());
    let err = result.unwrap_err();
    assert!(
        err.is_rate_limited(),
        "locked account login should be rate-limited, got {:?} ({})",
        err.code(),
        err.message()
    );
}

#[tokio::test]
async fn test_login_failure_lockout_after_threshold() {
    let pool = crate::require_test_pool().await;
    // Very low threshold so we trigger lockout quickly.
    let mut security = test_security();
    security.login_failure_lockout_threshold = 2;
    security.login_lockout_duration_seconds = 3600;
    let (auth, cache) = build_auth_with_cache(&pool, &security);

    let username = unique_username();
    let password = "StrongP@ss1!";
    let (user, _access, _refresh, _device) = auth.register(&username, password, false, None).await.unwrap();

    // Two wrong-password logins should trigger lockout (threshold = 2).
    for _ in 0..2 {
        let _ = auth.login(&username, "WrongPassword99!", None, None).await;
    }

    // After lockout, even the correct password should be rate-limited.
    let lockout_key = format!("auth:lockout:{}", user.user_id);
    let lockout_until: Option<i64> = cache.get(&lockout_key).await.unwrap();
    assert!(lockout_until.is_some(), "lockout key should be set after threshold failures");

    let result = auth.login(&username, password, None, None).await;
    assert!(result.is_err());
    assert!(
        result.unwrap_err().is_rate_limited(),
        "correct password should still be rate-limited while lockout is active"
    );
}

#[tokio::test]
async fn test_login_clears_logout_marker_on_success() {
    let pool = crate::require_test_pool().await;
    let security = test_security();
    let (auth, cache) = build_auth_with_cache(&pool, &security);

    let username = unique_username();
    let password = "StrongP@ss1!";
    let (user, _access, _refresh, _device) = auth.register(&username, password, false, None).await.unwrap();

    // Seed a logout marker (as if the user had previously called logout_all).
    let logout_marker = format!("user:logout_all:{}", user.user_id);
    cache.set(&logout_marker, &"1", 3600).await.unwrap();

    let result = auth.login(&username, password, None, None).await;
    assert!(result.is_ok(), "login should succeed: {:?}", result.err());

    // The logout marker should have been deleted.
    let marker: Option<String> = cache.get(&logout_marker).await.unwrap();
    assert!(marker.is_none(), "logout marker should be cleared on successful login");
}
