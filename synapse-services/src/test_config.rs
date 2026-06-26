//! Test configuration utilities
//!
//! Provides centralized configuration for test environments,
//! eliminating hardcoded connection strings and paths.

#![cfg(any(test, feature = "test-utils"))]

use synapse_common::config::Config;
use synapse_common::config::{
    AdminRegistrationConfig, CorsConfig, DatabaseConfig, FederationConfig, FederationRateLimitConfig,
    PostgresFtsConfig, RateLimitConfig, RedisConfig, SearchConfig, SecurityConfig, ServerConfig, SmtpConfig,
    WorkerConfig,
};

/// Returns the test database URL from environment or default
///
/// Reads from TEST_DATABASE_URL environment variable.
/// Default: postgres://synapse:synapse@localhost:5432/synapse_test
pub fn test_database_url() -> String {
    std::env::var("TEST_DATABASE_URL")
        .unwrap_or_else(|_| "postgres://synapse:synapse@localhost:5432/synapse_test".to_string())
}

/// Returns the test Redis URL from environment or default
///
/// Reads from TEST_REDIS_URL environment variable.
/// Default: redis://localhost:6379
pub fn test_redis_url() -> String {
    std::env::var("TEST_REDIS_URL").unwrap_or_else(|_| "redis://localhost:6379".to_string())
}

/// Build a `Config` suitable for `ServiceContainer::new_test*` flows.
///
/// Reads `DATABASE_HOST` / `DATABASE_PORT` / `DATABASE_USER` /
/// `DATABASE_PASSWORD` / `DATABASE_NAME` env vars (defaulting to
/// `localhost:5432` / `synapse` / `synapse` / `synapse`) and wires up
/// sensible test defaults for every other sub-config.
pub fn build_test_config() -> Config {
    let host = std::env::var("DATABASE_HOST").unwrap_or_else(|_| "localhost".to_string());
    let port: u16 = std::env::var("DATABASE_PORT").ok().and_then(|p| p.parse().ok()).unwrap_or(5432);
    let user = std::env::var("DATABASE_USER").unwrap_or_else(|_| "synapse".to_string());
    let pass = std::env::var("DATABASE_PASSWORD").unwrap_or_else(|_| "synapse".to_string());
    let name = std::env::var("DATABASE_NAME").unwrap_or_else(|_| "synapse".to_string());
    let test_pool_max_connections = crate::test_utils::configured_test_pool_max_connections();
    let test_pool_min_connections = crate::test_utils::configured_test_pool_min_connections();

    Config {
        server: ServerConfig {
            name: "localhost".to_string(),
            host: "0.0.0.0".to_string(),
            port: 8008,
            public_baseurl: None,
            signing_key_path: None,
            macaroon_secret_key: None,
            form_secret: None,
            server_name: None,
            suppress_key_server_warning: false,
            serve_server_wellknown: false,
            soft_file_limit: 0,
            user_agent_suffix: None,
            web_client_location: None,
            registration_shared_secret: None,
            admin_contact: None,
            max_upload_size: 1000000,
            max_image_resolution: 1000000,
            remote_media_lifetime: 2592000,
            local_media_lifetime: 0,
            enable_registration: true,
            enable_registration_captcha: false,
            background_tasks_interval: 60,
            dehydrated_device_cleanup_interval_secs: 3600,
            expire_access_token: true,
            expire_access_token_lifetime: 3600,
            refresh_token_lifetime: 604800,
            refresh_token_sliding_window_size: 1000,
            session_duration: 86400,
            warmup_pool: true,
            allow_public_rooms_without_auth: false,
            allow_public_rooms_over_federation: true,
            auto_join_rooms: vec![],
            autocreate_auto_join_rooms: true,
            encryption_enabled_by_default_for_room_type: None,
            app_service_config_files: vec![],
            presence_enabled: true,
            media_path: "./data/media".to_string(),
            megolm_encryption_key_path: None,
            enable_burn_after_read_processor: true,
            refresh_token_ttl_secs: 2_592_000,
        },
        database: DatabaseConfig {
            host,
            port,
            username: user,
            password: pass,
            name,
            pool_size: test_pool_max_connections,
            max_size: test_pool_max_connections,
            min_idle: Some(test_pool_min_connections),
            connection_timeout: crate::test_utils::configured_test_pool_acquire_timeout().as_secs(),
        },
        redis: RedisConfig {
            host: "localhost".to_string(),
            port: 6379,
            password: None,
            key_prefix: "test:".to_string(),
            pool_size: 10,
            enabled: false,
            connection_timeout_ms: 5000,
            command_timeout_ms: 3000,
            circuit_breaker: synapse_common::config::CircuitBreakerConfig::default(),
        },
        logging: synapse_common::config::LoggingConfig {
            level: "info".to_string(),
            format: "json".to_string(),
            log_file: None,
            log_dir: None,
        },
        federation: FederationConfig {
            enabled: true,
            allow_ingress: false,
            server_name: "test.example.com".to_string(),
            federation_port: 8448,
            connection_pool_size: 10,
            max_transaction_payload: 50000,
            ca_file: None,
            client_ca_file: None,
            signing_key: None,
            key_id: None,
            trusted_key_servers: vec![],
            key_refresh_interval: 86400,
            suppress_key_server_warning: false,
            signature_cache_ttl: 3600,
            key_cache_ttl: 3600,
            key_rotation_grace_period_ms: 60_0000,
            key_fetch_max_concurrency: 32,
            key_fetch_timeout_ms: 5000,
            process_inbound_edus: false,
            inbound_edus_max_per_txn: 100,
            inbound_edu_max_concurrency: 8,
            inbound_edu_acquire_timeout_ms: 250,
            inbound_edu_per_origin_max_concurrency: 2,
            process_inbound_presence_edus: false,
            inbound_presence_updates_max_per_txn: 50,
            inbound_presence_backoff_ms: 3000,
            join_max_concurrency: 16,
            join_acquire_timeout_ms: 750,
            admission_mode: false,
            signing_key_master_key: None,
            event_broadcast_batch_size: 100,
            rate_limit: FederationRateLimitConfig::default(),
        },
        security: SecurityConfig {
            secret: "test_secret".to_string(),
            expiry_time: 3600,
            refresh_token_expiry: 604800,
            argon2_m_cost: 65536,
            argon2_t_cost: 3,
            argon2_p_cost: 1,
            allow_legacy_hashes: false,
            login_failure_lockout_threshold: 5,
            login_lockout_duration_seconds: 900,
            admin_mfa_required: false,
            admin_mfa_shared_secret: String::new(),
            admin_mfa_allowed_drift_steps: 1,
            admin_rbac_enabled: true,
            ui_auth_session_timeout: 900,
            csrf_secret: String::new(),
        },
        search: SearchConfig {
            enabled: false,
            elasticsearch_url: "http://localhost:9200".to_string(),
            postgres_fts: PostgresFtsConfig { enabled: true, weights: Default::default() },
            provider: "postgres".to_string(),
            search_index_name: "synapse_search".to_string(),
        },
        rate_limit: RateLimitConfig::default(),
        admin_registration: AdminRegistrationConfig {
            enabled: true,
            shared_secret: "test_shared_secret".to_string(),
            nonce_timeout_seconds: 60,
            allow_external_access: false,
            production_only: true,
            ip_whitelist: Vec::new(),
            require_captcha: false,
            require_manual_approval: false,
            approval_tokens: Vec::new(),
        },
        builtin_oidc: synapse_common::config::BuiltinOidcConfig::default(),
        worker: WorkerConfig::default(),
        cors: CorsConfig::default(),
        smtp: SmtpConfig::default(),
        sms: synapse_common::config::SmsConfig::default(),
        voip: synapse_common::config::VoipConfig::default(),
        livekit: synapse_common::config::LivekitConfig::default(),
        push: synapse_common::config::PushConfig::default(),
        url_preview: synapse_common::config::UrlPreviewConfig::default(),
        oidc: synapse_common::config::OidcConfig::default(),
        saml: synapse_common::config::SamlConfig::default(),
        retention: synapse_common::config::RetentionConfig::default(),
        telemetry: synapse_common::telemetry_config::OpenTelemetryConfig::default(),
        prometheus: synapse_common::telemetry_config::PrometheusConfig::default(),
        performance: synapse_common::config::PerformanceConfig::default(),
        experimental: synapse_common::config::ExperimentalConfig::default(),
        identity: synapse_common::config::IdentityConfig::default(),
        translate: synapse_common::config::TranslateConfig::default(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_database_url_default() {
        std::env::remove_var("TEST_DATABASE_URL");
        assert_eq!(test_database_url(), "postgres://synapse:synapse@localhost:5432/synapse_test");
    }

    #[test]
    fn test_database_url_from_env() {
        std::env::set_var("TEST_DATABASE_URL", "postgres://custom:custom@localhost:5432/custom");
        assert_eq!(test_database_url(), "postgres://custom:custom@localhost:5432/custom");
        std::env::remove_var("TEST_DATABASE_URL");
    }
}
