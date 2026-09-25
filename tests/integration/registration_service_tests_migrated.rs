#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use synapse_rust::cache::{CacheConfig, CacheManager};
use synapse_rust::common::config::SecurityConfig;
use synapse_rust::common::metrics::MetricsCollector;
use synapse_services::account::UserService;
use synapse_services::auth::{AuthService, CredentialAuth, TokenAuth};
use synapse_services::registration_service::RegistrationService;
use synapse_storage::user::UserStorage;
use synapse_storage::user::UserStore;

static TEST_COUNTER: AtomicU64 = AtomicU64::new(1);

fn unique_id() -> u64 {
    TEST_COUNTER.fetch_add(1, Ordering::SeqCst)
}

#[tokio::test]
async fn test_register_user_success() {
    let pool = crate::require_test_pool().await;

    let security = SecurityConfig {
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
    };
    let cache = Arc::new(CacheManager::new(&CacheConfig::default()));
    let canonical_cache = cache.clone();
    let metrics = Arc::new(MetricsCollector::new());
    let auth_service = AuthService::new(&pool, canonical_cache.clone(), metrics.clone(), &security, "localhost");
    let user_store: Arc<dyn UserStore> = Arc::new(UserStorage::new(&pool, canonical_cache));
    let user_service = Arc::new(UserService::new(user_store.clone()));
    let auth_arc = Arc::new(auth_service);
    let token_auth: Arc<dyn TokenAuth> = auth_arc.clone();
    let credential_auth: Arc<dyn CredentialAuth> = auth_arc;
    let registration_service =
        RegistrationService::new(user_service, token_auth, credential_auth, metrics, "localhost", true, None);

    let id = unique_id();
    let username = format!("alice_{}", id);
    let result = registration_service.register_user(&username, "Password123!", Some("Alice"), None).await;
    assert!(result.is_ok());
    let val = result.unwrap();
    assert_eq!(val["user_id"], format!("@{}:localhost", username));
    assert!(val["access_token"].is_string());
}

#[tokio::test]
async fn test_login_success() {
    let pool = crate::require_test_pool().await;

    let security = SecurityConfig {
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
    };
    let cache = Arc::new(CacheManager::new(&CacheConfig::default()));
    let canonical_cache = cache.clone();
    let metrics = Arc::new(MetricsCollector::new());
    let auth_service = AuthService::new(&pool, canonical_cache.clone(), metrics.clone(), &security, "localhost");
    let user_store: Arc<dyn UserStore> = Arc::new(UserStorage::new(&pool, canonical_cache));
    let user_service = Arc::new(UserService::new(user_store.clone()));
    let auth_arc = Arc::new(auth_service);
    let token_auth: Arc<dyn TokenAuth> = auth_arc.clone();
    let credential_auth: Arc<dyn CredentialAuth> = auth_arc;
    let registration_service =
        RegistrationService::new(user_service, token_auth, credential_auth, metrics, "localhost", true, None);

    let id = unique_id();
    let username = format!("alice_{}", id);
    registration_service.register_user(&username, "Password123!", None, None).await.unwrap();

    let result = registration_service.login(&username, "Password123!", None, None).await;
    assert!(result.is_ok());
    let val = result.unwrap();
    assert_eq!(val["user_id"], format!("@{}:localhost", username));
}

#[tokio::test]
async fn test_get_profile_success() {
    let pool = crate::require_test_pool().await;

    let cache = Arc::new(CacheManager::new(&CacheConfig::default()));
    let canonical_cache = cache.clone();
    let id = unique_id();
    let user_id = format!("@alice_{}:localhost", id);
    let username = format!("alice_{}", id);

    let user_storage: Arc<dyn UserStore> = Arc::new(UserStorage::new(&pool, canonical_cache.clone()));
    let user_service = Arc::new(UserService::new(user_storage.clone()));
    user_storage.create_user(&user_id, &username, None, false).await.unwrap();
    user_storage.update_displayname(&user_id, Some("Alice")).await.unwrap();

    let security = SecurityConfig {
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
    };
    let metrics = Arc::new(MetricsCollector::new());
    let auth_service = AuthService::new(&pool, canonical_cache.clone(), metrics.clone(), &security, "localhost");
    let auth_arc = Arc::new(auth_service);
    let token_auth: Arc<dyn TokenAuth> = auth_arc.clone();
    let credential_auth: Arc<dyn CredentialAuth> = auth_arc;
    let registration_service =
        RegistrationService::new(user_service, token_auth, credential_auth, metrics, "localhost", true, None);

    let result = registration_service.get_profile(&user_id).await;
    assert!(result.is_ok());
    let val = result.unwrap();
    assert_eq!(val["displayname"], "Alice");
}
