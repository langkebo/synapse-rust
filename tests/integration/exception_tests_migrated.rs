#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
use std::sync::Arc;
use synapse_rust::auth::AuthService;
use synapse_rust::cache::{CacheConfig, CacheManager};
use synapse_rust::common::config::SecurityConfig;
use synapse_rust::common::metrics::MetricsCollector;

#[tokio::test]
async fn test_invalid_jwt() {
    let pool = crate::require_test_pool().await;
    let cache = Arc::new(CacheManager::new(&CacheConfig::default()).to_synapse_cache_manager());
    let metrics = Arc::new(MetricsCollector::new());
    let security_config = SecurityConfig {
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
    };
    let auth_service = AuthService::new(&pool, cache, metrics, &security_config, "localhost");

    let result = auth_service.validate_token("invalid_token").await;
    assert!(result.is_err());
    match result {
        Err(ref e) => {
            assert!(e.is_unauthorized(), "Expected Unauthorized error, got: {:?}", e);
            assert!(e.message.contains("Invalid token"));
        }
        _ => panic!("Expected Unauthorized error"),
    }
}

#[tokio::test]
async fn test_database_connection_failure() {
    let result = sqlx::PgPool::connect("postgres://synapse:secret@localhost:5433/synapse_test").await;
    assert!(result.is_err());
}
