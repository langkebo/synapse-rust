use serde::Deserialize;
#[cfg(test)]
use std::path::PathBuf;

mod loader;
mod manager;
mod validation;

/// Re-exported item.
pub use manager::ConfigManager;

// ============================================================================
// Sub-module declarations
// ============================================================================

/// Module `auth`.
pub mod auth;
/// Module `builtin_oidc`.
pub mod builtin_oidc;
/// Module `database`.
pub mod database;
/// Module `error`.
pub mod error;
/// Module `experimental`.
pub mod experimental;
/// Module `federation`.
pub mod federation;
/// Module `identity`.
pub mod identity;
/// Module `logging`.
pub mod logging;
/// Module `mas`.
pub mod mas;
/// Module `performance`.
pub mod performance;
/// Module `policy_server`.
pub mod policy_server;
/// Module `push`.
pub mod push;
/// Module `rate_limit`.
pub mod rate_limit;
/// Module `retention`.
pub mod retention;
/// Module `search`.
pub mod search;
/// Module `security`.
pub mod security;
/// Module `server`.
pub mod server;
/// Module `sms`.
pub mod sms;
/// Module `smtp`.
pub mod smtp;
/// Module `translate`.
pub mod translate;
/// Module `voip`.
pub mod voip;
/// Module `worker`.
pub mod worker;

// ============================================================================
// Re-exports for backward compatibility
// ============================================================================

/// Re-exported item.
pub use auth::{OidcAttributeMapping, OidcConfig, SamlAttributeMapping, SamlConfig};
/// Re-exported item.
pub use builtin_oidc::{BuiltinOidcConfig, BuiltinOidcUser};
/// Re-exported item.
pub use database::{CircuitBreakerConfig, DatabaseConfig, RedisConfig};
/// Re-exported item.
pub use error::ConfigError;
/// Re-exported item.
pub use experimental::ExperimentalConfig;
/// Re-exported item.
pub use federation::{FederationConfig, FederationRateLimitConfig, TrustedKeyServer};
/// Re-exported item.
pub use identity::IdentityConfig;
/// Re-exported item.
pub use logging::LoggingConfig;
/// Re-exported item.
pub use mas::MasConfig;
/// Re-exported item.
pub use performance::PerformanceConfig;
/// Re-exported item.
pub use policy_server::PolicyServerConfig;
/// Re-exported item.
pub use rate_limit::{RateLimitConfig, RateLimitEndpointRule, RateLimitMatchType, RateLimitRule, SyncRateLimitConfig};
/// Re-exported item.
pub use retention::{RetentionConfig, RetentionPolicy, RetentionPurgeJob};
/// Re-exported item.
pub use search::{PostgresFtsConfig, PostgresFtsWeights, SearchConfig};
/// Re-exported item.
pub use security::{AdminRegistrationConfig, CorsConfig, SecurityConfig};
/// Re-exported item.
pub use server::ServerConfig;
/// Re-exported item.
pub use sms::SmsConfig;
/// Re-exported item.
pub use smtp::{SmtpConfig, SmtpRateLimitConfig};
/// Re-exported item.
pub use translate::TranslateConfig;
/// Re-exported item.
pub use voip::{
    ApnsConfig, FcmConfig, LivekitConfig, PushConfig, UrlBlacklistRule, UrlPreviewConfig, VoipConfig, WebPushConfig,
};
/// Re-exported item.
pub use worker::{InstanceLocationConfig, ReplicationConfig, ReplicationHttpConfig, StreamWriters, WorkerConfig};

// Re-export helper functions used in tests and serde defaults
/// Re-exported item.
pub use security::{
    default_admin_mfa_allowed_drift_steps, default_admin_rbac_enabled, default_allowed_headers,
    default_allowed_methods, default_cors_max_age, default_ui_auth_session_timeout,
};
/// Re-exported item.
pub use server::default_dehydrated_device_cleanup_interval_secs;

/// Main configuration class for the Matrix Homeserver, containing all configuration sub-items.
/// Loaded via environment variables or configuration file.
///
/// `deny_unknown_fields` 使未知顶层配置键在加载时直接报错，而非被 serde
/// 静默丢弃，避免运维配置了不存在的字段（如历史遗留的 `registration:`、
/// `identity_server_url:`、`app_service:`）却误以为生效（审查 #11）。
#[derive(Debug, Clone, Deserialize, Default)]
#[serde(deny_unknown_fields)]
/// Represents Config.
pub struct Config {
    /// Server configuration
    pub server: ServerConfig,
    /// Database configuration
    pub database: DatabaseConfig,
    /// Redis configuration
    pub redis: RedisConfig,
    /// Logging configuration
    pub logging: LoggingConfig,
    /// Federation configuration
    pub federation: FederationConfig,
    /// Security configuration
    pub security: SecurityConfig,
    /// Search configuration
    pub search: SearchConfig,
    /// Rate limiting configuration
    #[serde(default)]
    /// `rate_limit` field.
    pub rate_limit: RateLimitConfig,
    /// Admin registration configuration
    #[serde(default)]
    /// `admin_registration` field.
    pub admin_registration: AdminRegistrationConfig,
    /// Worker node configuration
    #[serde(default)]
    /// `worker` field.
    pub worker: WorkerConfig,
    /// CORS configuration
    #[serde(default)]
    /// `cors` field.
    pub cors: CorsConfig,
    /// SMTP email configuration
    #[serde(default)]
    /// `smtp` field.
    pub smtp: SmtpConfig,
    /// SMS provider configuration
    #[serde(default)]
    /// `sms` field.
    pub sms: SmsConfig,
    /// VoIP/TURN configuration
    #[serde(default)]
    /// `voip` field.
    pub voip: VoipConfig,
    /// Livekit SFU configuration
    #[serde(default)]
    /// `livekit` field.
    pub livekit: LivekitConfig,
    /// Push notification configuration
    #[serde(default)]
    /// `push` field.
    pub push: PushConfig,
    /// URL preview configuration
    #[serde(default)]
    /// `url_preview` field.
    pub url_preview: UrlPreviewConfig,
    /// OIDC single sign-on configuration (external Provider)
    #[serde(default)]
    /// `oidc` field.
    pub oidc: OidcConfig,
    /// Built-in OIDC Provider configuration
    #[serde(default)]
    /// `builtin_oidc` field.
    pub builtin_oidc: BuiltinOidcConfig,
    /// SAML single sign-on configuration
    #[serde(default)]
    /// `saml` field.
    pub saml: SamlConfig,
    /// Message retention policy configuration
    #[serde(default)]
    /// `retention` field.
    pub retention: RetentionConfig,
    /// MSC4284 — Policy server configuration for room/user/content moderation.
    #[serde(default)]
    /// `policy_server` field.
    pub policy_server: PolicyServerConfig,
    /// MSC3861 — Matrix Authentication Service (MAS) configuration.
    /// When enabled, the homeserver delegates auth to MAS and validates
    /// MAS-issued access tokens (JWTs) instead of local HS256 tokens.
    #[serde(default)]
    /// `mas` field.
    pub mas: MasConfig,
    /// OpenTelemetry configuration
    #[serde(default)]
    /// `telemetry` field.
    pub telemetry: crate::telemetry_config::OpenTelemetryConfig,
    /// Prometheus configuration
    #[serde(default)]
    /// `prometheus` field.
    pub prometheus: crate::telemetry_config::PrometheusConfig,
    /// Performance optimization configuration
    #[serde(default)]
    /// `performance` field.
    pub performance: PerformanceConfig,
    /// 实验性功能配置
    #[serde(default)]
    /// `experimental` field.
    pub experimental: ExperimentalConfig,
    /// Identity Server 配置
    #[serde(default)]
    /// `identity` field.
    pub identity: IdentityConfig,
    /// Translation service configuration
    #[serde(default)]
    /// `translate` field.
    pub translate: TranslateConfig,
    /// Allowed redirect URL prefixes for SSO post-login redirects.
    /// If empty, only same-origin paths (starting with `/`) are permitted.
    /// Example: `["https://app.example.com/"]`
    #[serde(default)]
    /// `sso_redirect_allowlist` field.
    pub sso_redirect_allowlist: Vec<String>,
}

impl Config {
    /// Databases the url.
    pub fn database_url(&self) -> String {
        format!(
            "postgres://{}:{}@{}:{}/{}",
            self.database.username, self.database.password, self.database.host, self.database.port, self.database.name
        )
    }

    /// Rediss the url.
    pub fn redis_url(&self) -> String {
        self.redis.connection_url()
    }

    /// Canonical access-token lifetime in seconds.
    ///
    /// `homeserver.yaml` historically exposes two settings that both look like
    /// they control this:
    ///   - `server.expire_access_token_lifetime` (Synapse-style, e.g. 86400)
    ///   - `security.expiry_time` (legacy short name, default 3600)
    ///
    /// Only `security.expiry_time` was actually wired into `AuthService`,
    /// causing tokens to expire in 1 hour even when operators configured
    /// 24 hours via the more obvious server-level field. This helper picks
    /// a single value with the priority:
    ///
    ///   1. `server.expire_access_token_lifetime` if it has been explicitly
    ///      set to something other than the legacy default of 3600.
    ///   2. `security.expiry_time` otherwise.
    ///   3. 3600 as a final fallback.
    pub fn access_token_lifetime_seconds(&self) -> i64 {
        let server_lifetime = self.server.expire_access_token_lifetime;
        let security_lifetime = self.security.expiry_time;
        if server_lifetime > 0 && server_lifetime != 3600 {
            if security_lifetime > 0 && security_lifetime != 3600 && security_lifetime != server_lifetime {
                tracing::warn!(
                    "Both server.expire_access_token_lifetime ({}) and security.expiry_time ({}) are set and differ. \
                     Using server.expire_access_token_lifetime as the canonical value.",
                    server_lifetime,
                    security_lifetime,
                );
            }
            server_lifetime
        } else if security_lifetime > 0 {
            security_lifetime
        } else {
            3600
        }
    }

    /// Canonical refresh-token lifetime in seconds.
    ///
    /// 历史上 refresh token 生命周期存在三处配置、默认值互相矛盾：
    ///   - `server.refresh_token_lifetime`（7 天，死字段，从未被消费）
    ///   - `server.refresh_token_ttl_secs`（30 天，仅喂给 RefreshTokenService）
    ///   - `security.refresh_token_expiry`（7 天，实际生效于 AuthService）
    ///
    /// 现已删除前两个冗余字段，`security.refresh_token_expiry` 成为单一权威
    /// 来源；为 0 时回退到 `DEFAULT_REFRESH_TOKEN_EXPIRY_SECS`（7 天）。
    pub fn refresh_token_lifetime_seconds(&self) -> i64 {
        if self.security.refresh_token_expiry > 0 {
            self.security.refresh_token_expiry
        } else {
            crate::DEFAULT_REFRESH_TOKEN_EXPIRY_SECS
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_config_rejects_unknown_top_level_key() {
        // deny_unknown_fields：未知顶层配置键应在反序列化时报 "unknown field"，
        // 而非被 serde 静默丢弃（审查 #11，防止运维配置了不存在的字段却误以为生效）。
        let json = serde_json::json!({"unknown_key": "x"});
        let err = serde_json::from_value::<Config>(json).unwrap_err();
        let msg = err.to_string();
        assert!(msg.contains("unknown field"), "应报 unknown field，实际：{msg}");
    }

    #[test]
    fn test_config_database_url() {
        let config = Config {
            server: ServerConfig {
                name: "test".to_string(),
                host: "127.0.0.1".to_string(),
                port: 8000,
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
                refresh_token_sliding_window_size: 1000,
                warmup_pool: true,
                allow_public_rooms_without_auth: false,
                allow_public_rooms_over_federation: true,
                auto_join_rooms: vec![],
                autocreate_auto_join_rooms: true,
                encryption_enabled_by_default_for_room_type: None,
                app_service_config_files: vec![],
                presence_enabled: true,
                ..Default::default()
            },
            database: DatabaseConfig {
                host: "localhost".to_string(),
                port: 5432,
                username: "testuser".to_string(),
                password: "testpass".to_string(),
                name: "testdb".to_string(),
                pool_size: 10,
                max_size: 20,
                min_idle: Some(5),
                connection_timeout: 30,
                ..Default::default()
            },
            redis: RedisConfig {
                host: "localhost".to_string(),
                port: 6379,
                password: None,
                key_prefix: "test:".to_string(),
                pool_size: 10,
                enabled: true,
                connection_timeout_ms: 500,
                command_timeout_ms: 500,
                circuit_breaker: CircuitBreakerConfig::default(),
            },
            logging: LoggingConfig {
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
                signing_key: Some("test_signing_key".to_string()),
                key_id: Some("ed25519:test_key".to_string()),
                trusted_key_servers: vec![],
                key_refresh_interval: 86400,
                suppress_key_server_warning: false,
                signature_cache_ttl: 3600,
                key_cache_ttl: 3600,
                key_rotation_grace_period_ms: 60_0000,
                key_fetch_max_concurrency: 32,
                key_fetch_timeout_ms: 5000,
                allow_http_key_fetch: false,
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
                ..Default::default()
            },
            security: SecurityConfig {
                secret: "test_secret".to_string(),
                expiry_time: 3600,
                refresh_token_expiry: 604800,
                argon2_m_cost: 4096,
                argon2_t_cost: 3,
                argon2_p_cost: 1,
                allow_legacy_hashes: false,
                login_failure_lockout_threshold: 5,
                login_lockout_duration_seconds: 900,
                login_lockout_fail_open_on_redis_error: true,
                admin_mfa_required: false,
                admin_mfa_shared_secret: String::new(),
                admin_mfa_allowed_drift_steps: default_admin_mfa_allowed_drift_steps(),
                admin_rbac_enabled: default_admin_rbac_enabled(),
                ui_auth_session_timeout: default_ui_auth_session_timeout(),
                csrf_secret: String::new(),
                ..Default::default()
            },
            search: SearchConfig {
                elasticsearch_url: "http://localhost:9200".to_string(),
                enabled: false,
                postgres_fts: PostgresFtsConfig::default(),
                provider: "elasticsearch".to_string(),
                ..Default::default()
            },
            rate_limit: RateLimitConfig::default(),
            admin_registration: AdminRegistrationConfig::default(),
            worker: WorkerConfig::default(),
            cors: CorsConfig {
                allowed_origins: vec!["*".to_string()],
                allow_credentials: false,
                allowed_methods: default_allowed_methods(),
                allowed_headers: default_allowed_headers(),
                max_age_seconds: default_cors_max_age(),
            },
            smtp: SmtpConfig::default(),
            sms: SmsConfig::default(),
            livekit: LivekitConfig::default(),
            voip: VoipConfig::default(),
            push: PushConfig::default(),
            url_preview: UrlPreviewConfig::default(),
            oidc: OidcConfig::default(),
            builtin_oidc: BuiltinOidcConfig::default(),
            saml: SamlConfig::default(),
            retention: RetentionConfig::default(),
            telemetry: crate::telemetry_config::OpenTelemetryConfig::default(),
            prometheus: crate::telemetry_config::PrometheusConfig::default(),
            performance: PerformanceConfig::default(),
            experimental: ExperimentalConfig::default(),
            identity: IdentityConfig::default(),
            translate: TranslateConfig::default(),
            sso_redirect_allowlist: vec![],
            ..Config::default()
        };

        let url = config.database_url();
        assert_eq!(url, "postgres://testuser:testpass@localhost:5432/testdb");
    }

    #[test]
    fn test_config_redis_url() {
        let config = Config {
            server: ServerConfig {
                name: "test".to_string(),
                host: "127.0.0.1".to_string(),
                port: 8000,
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
                refresh_token_sliding_window_size: 1000,
                warmup_pool: true,
                allow_public_rooms_without_auth: false,
                allow_public_rooms_over_federation: true,
                auto_join_rooms: vec![],
                autocreate_auto_join_rooms: true,
                encryption_enabled_by_default_for_room_type: None,
                app_service_config_files: vec![],
                presence_enabled: true,
                ..Default::default()
            },
            database: DatabaseConfig {
                host: "localhost".to_string(),
                port: 5432,
                username: "testuser".to_string(),
                password: "testpass".to_string(),
                name: "testdb".to_string(),
                pool_size: 10,
                max_size: 20,
                min_idle: Some(5),
                connection_timeout: 30,
                ..Default::default()
            },
            redis: RedisConfig {
                host: "redis.example.com".to_string(),
                port: 6380,
                password: Some("secret".to_string()),
                key_prefix: "prod:".to_string(),
                pool_size: 20,
                enabled: true,
                connection_timeout_ms: 500,
                command_timeout_ms: 500,
                circuit_breaker: CircuitBreakerConfig::default(),
            },
            logging: LoggingConfig {
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
                signing_key: Some("test_signing_key".to_string()),
                key_id: Some("ed25519:test_key".to_string()),
                trusted_key_servers: vec![],
                key_refresh_interval: 86400,
                suppress_key_server_warning: false,
                signature_cache_ttl: 3600,
                key_cache_ttl: 3600,
                key_rotation_grace_period_ms: 60_0000,
                key_fetch_max_concurrency: 32,
                key_fetch_timeout_ms: 5000,
                allow_http_key_fetch: false,
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
                ..Default::default()
            },
            security: SecurityConfig {
                secret: "test_secret".to_string(),
                expiry_time: 3600,
                refresh_token_expiry: 604800,
                argon2_m_cost: 4096,
                argon2_t_cost: 3,
                argon2_p_cost: 1,
                allow_legacy_hashes: false,
                login_failure_lockout_threshold: 5,
                login_lockout_duration_seconds: 900,
                login_lockout_fail_open_on_redis_error: true,
                admin_mfa_required: false,
                admin_mfa_shared_secret: String::new(),
                admin_mfa_allowed_drift_steps: default_admin_mfa_allowed_drift_steps(),
                admin_rbac_enabled: default_admin_rbac_enabled(),
                ui_auth_session_timeout: default_ui_auth_session_timeout(),
                csrf_secret: String::new(),
                ..Default::default()
            },
            search: SearchConfig {
                elasticsearch_url: "http://localhost:9200".to_string(),
                enabled: false,
                postgres_fts: PostgresFtsConfig::default(),
                provider: "elasticsearch".to_string(),
                ..Default::default()
            },
            rate_limit: RateLimitConfig::default(),
            admin_registration: AdminRegistrationConfig::default(),
            worker: WorkerConfig::default(),
            cors: CorsConfig {
                allowed_origins: vec!["*".to_string()],
                allow_credentials: false,
                allowed_methods: default_allowed_methods(),
                allowed_headers: default_allowed_headers(),
                max_age_seconds: default_cors_max_age(),
            },
            smtp: SmtpConfig::default(),
            sms: SmsConfig::default(),
            livekit: LivekitConfig::default(),
            voip: VoipConfig::default(),
            push: PushConfig::default(),
            url_preview: UrlPreviewConfig::default(),
            oidc: OidcConfig::default(),
            saml: SamlConfig::default(),
            retention: RetentionConfig::default(),
            telemetry: crate::telemetry_config::OpenTelemetryConfig::default(),
            prometheus: crate::telemetry_config::PrometheusConfig::default(),
            performance: PerformanceConfig::default(),
            experimental: ExperimentalConfig::default(),
            identity: IdentityConfig::default(),
            ..Config::default()
        };

        let url = config.redis_url();
        assert_eq!(url, "redis://:secret@redis.example.com:6380/");
    }

    #[test]
    fn test_server_config_defaults() {
        let config = ServerConfig {
            name: "test".to_string(),
            host: "0.0.0.0".to_string(),
            port: 8080,
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
            registration_shared_secret: Some("secret".to_string()),
            admin_contact: Some("admin@example.com".to_string()),
            max_upload_size: 50000000,
            max_image_resolution: 8000000,
            remote_media_lifetime: 2592000,
            local_media_lifetime: 0,
            enable_registration: true,
            enable_registration_captcha: true,
            background_tasks_interval: 30,
            dehydrated_device_cleanup_interval_secs: 3600,
            expire_access_token: true,
            expire_access_token_lifetime: 86400,
            refresh_token_sliding_window_size: 5000,
            warmup_pool: true,
            allow_public_rooms_without_auth: false,
            allow_public_rooms_over_federation: true,
            auto_join_rooms: vec![],
            autocreate_auto_join_rooms: true,
            encryption_enabled_by_default_for_room_type: None,
            app_service_config_files: vec![],
            presence_enabled: true,
            ..Default::default()
        };

        assert_eq!(config.name, "test");
        assert_eq!(config.port, 8080);
        assert!(config.enable_registration);
        assert!(config.registration_shared_secret.is_some());
    }

    #[test]
    fn test_dehydrated_device_cleanup_interval_default() {
        assert_eq!(default_dehydrated_device_cleanup_interval_secs(), 3600);
    }

    #[test]
    fn test_database_config_defaults() {
        let config = DatabaseConfig {
            host: "db.example.com".to_string(),
            port: 5432,
            username: "synapse".to_string(),
            password: "secure_password".to_string(),
            name: "synapse".to_string(),
            pool_size: 10,
            max_size: 20,
            min_idle: None,
            connection_timeout: 60,
            ..Default::default()
        };

        assert_eq!(config.host, "db.example.com");
        assert_eq!(config.port, 5432);
        assert!(config.min_idle.is_none());
    }

    #[test]
    fn test_redis_config_defaults() {
        let config = RedisConfig {
            host: "127.0.0.1".to_string(),
            port: 6379,
            password: None,
            key_prefix: "synapse:".to_string(),
            pool_size: 16,
            enabled: true,
            connection_timeout_ms: 500,
            command_timeout_ms: 500,
            circuit_breaker: CircuitBreakerConfig::default(),
        };

        assert_eq!(config.host, "127.0.0.1");
        assert_eq!(config.port, 6379);
        assert!(config.enabled);
        assert_eq!(config.connection_timeout_ms, 500);
        assert_eq!(config.command_timeout_ms, 500);
        assert!(config.circuit_breaker.enabled);
        assert_eq!(config.connection_url(), "redis://127.0.0.1:6379/");
    }

    #[test]
    fn test_redis_config_connection_url_with_password() {
        let config = RedisConfig {
            host: "redis".to_string(),
            port: 6379,
            password: Some("secret".to_string()),
            key_prefix: "synapse:".to_string(),
            pool_size: 16,
            enabled: true,
            connection_timeout_ms: 500,
            command_timeout_ms: 500,
            circuit_breaker: CircuitBreakerConfig::default(),
        };

        assert_eq!(config.connection_url(), "redis://:secret@redis:6379/");
    }

    #[test]
    fn test_resolve_env_variables_resolves_redis_password() -> Result<(), String> {
        unsafe {
            std::env::set_var("TEST_REDIS_PASSWORD", "resolved-secret");
        }

        let mut config = Config {
            server: ServerConfig {
                name: "test".to_string(),
                host: "127.0.0.1".to_string(),
                port: 8000,
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
                refresh_token_sliding_window_size: 1000,
                warmup_pool: true,
                allow_public_rooms_without_auth: false,
                allow_public_rooms_over_federation: true,
                auto_join_rooms: vec![],
                autocreate_auto_join_rooms: true,
                encryption_enabled_by_default_for_room_type: None,
                app_service_config_files: vec![],
                presence_enabled: true,
                ..Default::default()
            },
            database: DatabaseConfig {
                host: "localhost".to_string(),
                port: 5432,
                username: "testuser".to_string(),
                password: "testpass".to_string(),
                name: "testdb".to_string(),
                pool_size: 10,
                max_size: 20,
                min_idle: Some(5),
                connection_timeout: 30,
                ..Default::default()
            },
            redis: RedisConfig {
                host: "localhost".to_string(),
                port: 6379,
                password: Some("${TEST_REDIS_PASSWORD:?missing}".to_string()),
                key_prefix: "test:".to_string(),
                pool_size: 10,
                enabled: true,
                connection_timeout_ms: 500,
                command_timeout_ms: 500,
                circuit_breaker: CircuitBreakerConfig::default(),
            },
            logging: LoggingConfig {
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
                signing_key: Some("test_signing_key".to_string()),
                key_id: Some("ed25519:test_key".to_string()),
                trusted_key_servers: vec![],
                key_refresh_interval: 86400,
                suppress_key_server_warning: false,
                signature_cache_ttl: 3600,
                key_cache_ttl: 3600,
                key_rotation_grace_period_ms: 60_0000,
                key_fetch_max_concurrency: 32,
                key_fetch_timeout_ms: 5000,
                allow_http_key_fetch: false,
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
                ..Default::default()
            },
            security: SecurityConfig {
                secret: "test_secret".to_string(),
                expiry_time: 3600,
                refresh_token_expiry: 604800,
                argon2_m_cost: 4096,
                argon2_t_cost: 3,
                argon2_p_cost: 1,
                allow_legacy_hashes: false,
                login_failure_lockout_threshold: 5,
                login_lockout_duration_seconds: 900,
                login_lockout_fail_open_on_redis_error: true,
                admin_mfa_required: false,
                admin_mfa_shared_secret: String::new(),
                admin_mfa_allowed_drift_steps: default_admin_mfa_allowed_drift_steps(),
                admin_rbac_enabled: default_admin_rbac_enabled(),
                ui_auth_session_timeout: default_ui_auth_session_timeout(),
                csrf_secret: String::new(),
                ..Default::default()
            },
            search: SearchConfig {
                elasticsearch_url: "http://localhost:9200".to_string(),
                enabled: false,
                postgres_fts: PostgresFtsConfig::default(),
                provider: "elasticsearch".to_string(),
                ..Default::default()
            },
            rate_limit: RateLimitConfig::default(),
            admin_registration: AdminRegistrationConfig::default(),
            worker: WorkerConfig::default(),
            cors: CorsConfig {
                allowed_origins: vec!["*".to_string()],
                allow_credentials: false,
                allowed_methods: default_allowed_methods(),
                allowed_headers: default_allowed_headers(),
                max_age_seconds: default_cors_max_age(),
            },
            smtp: SmtpConfig::default(),
            sms: SmsConfig::default(),
            livekit: LivekitConfig::default(),
            voip: VoipConfig::default(),
            push: PushConfig::default(),
            url_preview: UrlPreviewConfig::default(),
            oidc: OidcConfig::default(),
            builtin_oidc: BuiltinOidcConfig::default(),
            saml: SamlConfig::default(),
            retention: RetentionConfig::default(),
            telemetry: crate::telemetry_config::OpenTelemetryConfig::default(),
            prometheus: crate::telemetry_config::PrometheusConfig::default(),
            performance: PerformanceConfig::default(),
            experimental: ExperimentalConfig::default(),
            identity: IdentityConfig::default(),
            translate: TranslateConfig::default(),
            sso_redirect_allowlist: vec![],
            ..Config::default()
        };

        config.resolve_env_variables()?;

        assert_eq!(config.redis.password.as_deref(), Some("resolved-secret"));

        unsafe {
            std::env::remove_var("TEST_REDIS_PASSWORD");
        }

        Ok(())
    }

    // 部署实测（2026）：`homeserver.yaml` 里
    // `signing_key_master_key: "${FEDERATION_MASTER_KEY:-}"` 在变量未设置时
    // 解析为**空字符串**，而不是"未配置"。`Some("")` 会让密钥管理器走加密分支，
    // 用空的 HKDF 输入派生 AES 密钥（HKDF 接受空 IKM），于是联邦签名私钥以 `enc:`
    // 前缀入库 —— 看似加密、实则任何拿到库导出的人都能解开，同时 fail-closed
    // 分支（`None` → 拒绝持久化）永远走不到。归一化必须发生在配置边界这一处。
    #[test]
    fn test_resolve_env_variables_normalizes_blank_signing_key_master_key() -> Result<(), String> {
        for blank in ["", "   ", "\n"] {
            let mut config = Config::default();
            config.federation.signing_key_master_key = Some(blank.to_string());

            config.resolve_env_variables()?;

            assert_eq!(
                config.federation.signing_key_master_key, None,
                "blank master key {blank:?} must normalize to None, not stay as an empty string"
            );
        }
        Ok(())
    }

    #[test]
    fn test_resolve_env_variables_keeps_a_real_signing_key_master_key() -> Result<(), String> {
        let mut config = Config::default();
        let key = "k".repeat(crate::key_encryption::MIN_MASTER_KEY_LEN);
        config.federation.signing_key_master_key = Some(key.clone());

        config.resolve_env_variables()?;

        assert_eq!(config.federation.signing_key_master_key.as_deref(), Some(key.as_str()));
        Ok(())
    }

    #[test]
    fn test_circuit_breaker_config_defaults() {
        let config = CircuitBreakerConfig::default();

        assert!(config.enabled);
        assert_eq!(config.failure_threshold, 10);
        assert_eq!(config.success_threshold, 3);
        assert_eq!(config.timeout_ms, 60_000);
        assert_eq!(config.window_size_seconds, 120);
    }

    #[test]
    fn test_logging_config_with_file() {
        let config = LoggingConfig {
            level: "debug".to_string(),
            format: "text".to_string(),
            log_file: Some("/var/log/synapse.log".to_string()),
            log_dir: Some("/var/log".to_string()),
        };

        assert_eq!(config.level, "debug");
        assert!(config.log_file.is_some());
        assert!(config.log_dir.is_some());
    }

    #[test]
    fn test_federation_config_defaults() {
        let config = FederationConfig {
            enabled: true,
            allow_ingress: true,
            server_name: "federation.example.com".to_string(),
            federation_port: 8448,
            connection_pool_size: 50,
            max_transaction_payload: 100000,
            ca_file: Some(PathBuf::from("/etc/synapse/ca.crt")),
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
            allow_http_key_fetch: false,
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
            ..Default::default()
        };

        assert!(config.enabled);
        assert!(config.allow_ingress);
        assert!(config.ca_file.is_some());
    }

    #[test]
    fn test_security_config_defaults() {
        let config = SecurityConfig {
            secret: "very_secure_secret_key".to_string(),
            expiry_time: 3600,
            refresh_token_expiry: 604800,
            argon2_m_cost: 4096,
            argon2_t_cost: 3,
            argon2_p_cost: 1,
            allow_legacy_hashes: false,
            login_failure_lockout_threshold: 5,
            login_lockout_duration_seconds: 900,
            login_lockout_fail_open_on_redis_error: true,
            admin_mfa_required: false,
            admin_mfa_shared_secret: String::new(),
            admin_mfa_allowed_drift_steps: default_admin_mfa_allowed_drift_steps(),
            admin_rbac_enabled: default_admin_rbac_enabled(),
            ui_auth_session_timeout: default_ui_auth_session_timeout(),
            csrf_secret: String::new(),
            ..Default::default()
        };

        assert!(config.secret.len() > 16);
        assert_eq!(config.argon2_m_cost, 4096);
    }

    #[test]
    fn test_refresh_token_lifetime_seconds_uses_security_expiry() {
        let config = Config {
            security: SecurityConfig { refresh_token_expiry: 2_592_000, ..Default::default() },
            ..Default::default()
        };
        assert_eq!(config.refresh_token_lifetime_seconds(), 2_592_000);
    }

    #[test]
    fn test_refresh_token_lifetime_seconds_falls_back_to_default_when_zero() {
        // SecurityConfig::default() 的 refresh_token_expiry 为 0，应回退到
        // DEFAULT_REFRESH_TOKEN_EXPIRY_SECS（7 天）。
        let config = Config::default();
        assert_eq!(config.refresh_token_lifetime_seconds(), crate::DEFAULT_REFRESH_TOKEN_EXPIRY_SECS);
    }
}

// ============================================================================
// 官方 Synapse 配置模块（未实现）
// 以下配置模块参考官方 Synapse 配置文档，使用注释标记暂未实现
// 文档: https://matrix-org.github.io/synapse/latest/usage/configuration/config_documentation.html
// ============================================================================

/*
/// 媒体存储配置。
///
/// 官方 Synapse 配置文档: <https://matrix-org.github.io/synapse/latest/usage/configuration/config_documentation.html#media_store>
///
/// 配置媒体文件（图片、视频等）的存储位置和访问方式。
///
/// # 待实现功能
/// - 媒体文件上传 API: `POST /_matrix/media/v3/upload`
/// - 媒体文件下载 API: `GET /_matrix/media/v3/download/{serverName}/{mediaId}`
/// - 缩略图生成: `GET /_matrix/media/v3/thumbnail/{serverName}/{mediaId}`
/// - URL 预览: `GET /_matrix/media/v3/preview_url`
/// - 二级存储提供者（S3, Azure Blob 等）
///
/// # 配置示例
/// ```yaml
/// media_store:
///   enabled: true
///   storage_path: "/var/lib/synapse/media"
///   max_upload_size: 104857600  # 字节；权威字段为 server.max_upload_size（G-1 统一）
///   url_preview_enabled: true
///   max_thumbnail_size: "10M"
///   min_thumbnail_size: "10K"
/// ```
#[derive(Debug, Clone, Deserialize)]
/// Represents MediaStoreConfig.
pub struct MediaStoreConfig {
    /// 是否启用媒体存储功能
    #[serde(default)]
    /// `enabled` field.
    pub enabled: bool,

    /// 媒体文件存储路径
    pub storage_path: String,

    /// 是否启用 URL 预览功能
    #[serde(default)]
    /// `url_preview_enabled` field.
    pub url_preview_enabled: bool,

    /// 缩略图配置
    #[serde(default)]
    /// `thumbnails` field.
    pub thumbnails: ThumbnailConfig,

    /// 二级存储提供者（S3, Azure 等）
    #[serde(default)]
    /// `storage_providers` field.
    pub storage_providers: Vec<StorageProviderConfig>,
}

#[derive(Debug, Clone, Deserialize)]
/// Represents ThumbnailConfig.
pub struct ThumbnailConfig {
    /// 最大缩略图大小
    #[serde(default = "default_max_thumbnail_size")]
    /// `max_size` field.
    pub max_size: String,

    /// 最小缩略图大小
    #[serde(default = "default_min_thumbnail_size")]
    /// `min_size` field.
    pub min_size: String,

    /// 支持的缩略图尺寸列表
    #[serde(default)]
    /// `sizes` field.
    pub sizes: Vec<ThumbnailSize>,
}

#[derive(Debug, Clone, Deserialize)]
/// Represents ThumbnailSize.
pub struct ThumbnailSize {
    /// `width` field.
    pub width: u32,
    /// `height` field.
    pub height: u32,
    /// `method` field.
    pub method: String, // "crop", "scale", "fit"
}

#[derive(Debug, Clone, Deserialize)]
/// Represents StorageProviderConfig.
pub struct StorageProviderConfig {
    /// `provider` field.
    pub provider: String, // "s3", "azure", "gcs"
    /// `bucket` field.
    pub bucket: String,
    /// `region` field.
    pub region: Option<String>,
    /// `endpoint_url` field.
    pub endpoint_url: Option<String>,
    /// `access_key_id` field.
    pub access_key_id: Option<String>,
    /// `secret_access_key` field.
    pub secret_access_key: Option<String>,
}
*/

/*
/// 监听器配置。
///
/// 官方 Synapse 配置文档: <https://matrix-org.github.io/synapse/latest/usage/configuration/config_documentation.html#listeners>
///
/// 配置多个监听器，每个监听器可以监听不同的端口并提供不同的资源。
///
/// # 待实现功能
/// - 多端口监听支持（当前只有单一 host:port）
/// - 按资源类型分离监听器（client, federation, metrics）
/// - TLS/HTTPS 支持
/// - X-Forwarded-For 处理
/// - 资源访问控制
///
/// # 配置示例
/// ```yaml
/// listeners:
///   - type: http
///     port: 8008
///     tls: false
///     x_forwarded: true
///     resources:
///       - names: [client, federation]
///         compress: true
///   - type: metrics
///     port: 9148
///     resources:
///       - names: [metrics]
/// ```
#[derive(Debug, Clone, Deserialize)]
/// Represents ListenersConfig.
pub struct ListenersConfig {
    #[serde(default)]
    /// `listeners` field.
    pub listeners: Vec<ListenerConfig>,
}

#[derive(Debug, Clone, Deserialize)]
/// Represents ListenerConfig.
pub struct ListenerConfig {
    /// 监听器类型: http, https, metrics, manhole
    #[serde(default)]
    pub r#type: String,

    /// 监听端口
    pub port: u16,

    /// 监听地址
    #[serde(default = "default_listen_host")]
    /// `host` field.
    pub host: String,

    /// 是否启用 TLS
    #[serde(default)]
    /// `tls` field.
    pub tls: bool,

    /// TLS 证书路径
    pub tls_certificate_path: Option<String>,

    /// TLS 私钥路径
    pub tls_private_key_path: Option<String>,

    /// 是否处理 X-Forwarded-For 头
    #[serde(default = "default_x_forwarded")]
    /// `x_forwarded` field.
    pub x_forwarded: bool,

    /// 资源配置
    #[serde(default)]
    /// `resources` field.
    pub resources: Vec<ListenerResource>,
}

#[derive(Debug, Clone, Deserialize)]
/// Represents ListenerResource.
pub struct ListenerResource {
    /// 资源名称列表: client, federation, metrics, static
    pub names: Vec<String>,

    /// 是否压缩响应
    #[serde(default = "default_compress")]
    /// `compress` field.
    pub compress: bool,
}

fn default_listen_host() -> String {
    "::".to_string()
}

fn default_x_forwarded() -> bool {
    false
}

fn default_compress() -> bool {
    false
}
*/

/*
/// 限制配置。
///
/// 官方 Synapse 配置文档: <https://matrix-org.github.io/synapse/latest/usage/configuration/config_documentation.html#limits>
///
/// 配置各种资源限制，防止资源滥用。
///
/// # 待实现功能
/// - 上传大小限制（已在 ServerConfig 中有基础实现）
/// - 房间加入限制
/// - 事件内容大小限制
/// - 联邦限制
/// - 速率限制（已实现 RateLimitConfig）
///
/// # 配置示例
/// ```yaml
/// limits:
///   max_upload_size: 104857600  # 字节；权威字段为 server.max_upload_size（G-1 统一）
///   room_join_complexity_limit: 10000
///   event_fields_size_limit: "65536"
/// federation:
///   event_size_limit: "10M"
///   batch_size_limit: 50
/// ```
#[derive(Debug, Clone, Deserialize)]
/// Represents LimitsConfig.
pub struct LimitsConfig {
    /// 房间加入复杂度限制
    #[serde(default = "default_room_join_complexity")]
    /// `room_join_complexity_limit` field.
    pub room_join_complexity_limit: u64,

    /// 事件字段大小限制
    #[serde(default = "default_event_fields_size")]
    /// `event_fields_size_limit` field.
    pub event_fields_size_limit: u64,

    /// 联邦限制配置
    #[serde(default)]
    /// `federation` field.
    pub federation: FederationLimitsConfig,
}

#[derive(Debug, Clone, Deserialize)]
/// Represents FederationLimitsConfig.
pub struct FederationLimitsConfig {
    /// 单个事件大小限制
    #[serde(default = "default_federation_event_size")]
    /// `event_size_limit` field.
    pub event_size_limit: String,

    /// 批量事件数量限制
    #[serde(default = "default_batch_size")]
    /// `batch_size_limit` field.
    pub batch_size_limit: u64,
}

fn default_room_join_complexity() -> u64 {
    10000
}

fn default_event_fields_size() -> u64 {
    65536
}

fn default_federation_event_size() -> String {
    "10M".to_string()
}

fn default_batch_size() -> u64 {
    50
}
*/

/*
/// 密码配置。
///
/// 官方 Synapse 配置文档: <https://matrix-org.github.io/synapse/latest/usage/configuration/config_documentation.html#password_config>
///
/// 配置密码策略和认证模块。
///
/// # 待实现功能
/// - 密码 pepper（全局盐值）
/// - 多认证模块支持（bcrypt, argon2, custom）
/// - 密码复杂度要求
/// - 密码重用检查
/// - 密码过期策略
///
/// # 配置示例
/// ```yaml
/// password_config:
///   enabled: true
///   pepper: "YOUR_PEPPER_SECRET"
///   minimum_length: 8
///   require_digit: true
///   require_symbol: true
///   modules:
///     - module: "argon2"
///     - module: "bcrypt"
/// ```
#[derive(Debug, Clone, Deserialize)]
/// Represents PasswordConfig.
pub struct PasswordConfig {
    /// 是否启用密码认证
    #[serde(default = "default_password_enabled")]
    /// `enabled` field.
    pub enabled: bool,

    /// 密码 pepper（全局盐值，对所有密码哈希添加额外安全性）
    pub pepper: Option<String>>

    /// 最小密码长度
    #[serde(default = "default_min_password_length")]
    /// `minimum_length` field.
    pub minimum_length: u32,

    /// 是否要求数字
    #[serde(default)]
    /// `require_digit` field.
    pub require_digit: bool,

    /// 是否要求符号
    #[serde(default)]
    /// `require_symbol` field.
    pub require_symbol: bool,

    /// 是否要求大写字母
    #[serde(default)]
    /// `require_uppercase` field.
    pub require_uppercase: bool,

    /// 是否要求小写字母
    #[serde(default)]
    /// `require_lowercase` field.
    pub require_lowercase: bool,

    /// 认证模块列表
    #[serde(default)]
    /// `modules` field.
    pub modules: Vec<PasswordAuthModule>,
}

#[derive(Debug, Clone, Deserialize)]
/// Represents PasswordAuthModule.
pub struct PasswordAuthModule {
    /// `module` field.
    pub module: String, // "argon2", "bcrypt", "custom"
    /// `config` field.
    pub config: Option<serde_json::Value>,
}

fn default_password_enabled() -> bool {
    true
}

fn default_min_password_length() -> u32 {
    8
}
*/

/*
/// 账户有效性配置。
///
/// 官方 Synapse 配置文档: <https://matrix-org.github.io/synapse/latest/usage/configuration/config_documentation.html#account_validity>
///
/// 配置临时账户功能。
///
/// # 待实现功能
/// - 账户有效期设置
/// - 账户续期 API
/// - 过期账户自动停用
/// - 续期邮件发送
///
/// # 配置示例
/// ```yaml
/// account_validity:
///   enabled: true
///   period: "30d"
///   renew_at: "7d"
///   renewal_email_subject: "Renew your account"
/// ```
#[derive(Debug, Clone, Deserialize)]
/// Represents AccountValidityConfig.
pub struct AccountValidityConfig {
    /// 是否启用账户有效性
    #[serde(default)]
    /// `enabled` field.
    pub enabled: bool,

    /// 账户有效期
    #[serde(default = "default_validity_period")]
    /// `period` field.
    pub period: String,

    /// 续期提醒时间
    #[serde(default = "default_renew_at")]
    /// `renew_at` field.
    pub renew_at: String,

    /// 续期邮件主题
    pub renewal_email_subject: Option<String>,

    /// 续期邮件模板
    pub renewal_email_template: Option<String>,
}

fn default_validity_period() -> String {
    "30d".to_string()
}

fn default_renew_at() -> String {
    "7d".to_string()
}
*/

/*
/// CAS 认证配置。
///
/// 官方 Synapse 配置文档: <https://matrix-org.github.io/synapse/latest/usage/configuration/config_documentation.html#cas_config>
///
/// 配置 CAS (Central Authentication Service) 单点登录。
///
/// # 待实现功能
/// - CAS 认证流程
/// - 属性获取
/// - 用户属性映射
///
/// # 配置示例
/// ```yaml
/// cas:
///   enabled: true
///   server_url: "https://cas.example.com"
///   service_url: "https://matrix.example.com"
/// ```
#[derive(Debug, Clone, Deserialize)]
/// Represents CasConfig.
pub struct CasConfig {
    /// 是否启用 CAS
    #[serde(default)]
    /// `enabled` field.
    pub enabled: bool,

    /// CAS 服务器 URL
    pub server_url: String,

    /// 服务 URL
    pub service_url: String,
}
*/

/*
/// SAML2 认证配置。
///
/// 官方 Synapse 配置文档: <https://matrix-org.github.io/synapse/latest/usage/configuration/config_documentation.html#saml2_config>
///
/// 配置 SAML2 单点登录（企业级 SSO）。
///
/// # 待实现功能
/// - SAML2 认证流程
/// - 元数据配置
/// - 属性映射
/// - 多 IdP 支持
///
/// # 配置示例
/// ```yaml
/// saml2:
///   enabled: true
///   sp_config:
///     endpoint:
///       - "https://matrix.example.com/_matrix/saml2/authn_response"
///     cert_file: "/path/to/cert.pem"
///     key_file: "/path/to/key.pem"
///   idp_metadata:
///     - url: "https://idp.example.com/metadata"
///   attribute_mapping:
///     uid: "name-id"
///     displayname: "displayName"
///     email: "emailAddress"
/// ```
#[derive(Debug, Clone, Deserialize)]
/// Represents Saml2Config.
pub struct Saml2Config {
    /// 是否启用 SAML2
    #[serde(default)]
    /// `enabled` field.
    pub enabled: bool,

    /// 服务提供者配置
    #[serde(default)]
    /// `sp_config` field.
    pub sp_config: Option<SamlSpConfig>,

    /// 身份提供者元数据
    #[serde(default)]
    /// `idp_metadata` field.
    pub idp_metadata: Vec<SamlIdpMetadata>,

    /// 属性映射
    #[serde(default)]
    /// `attribute_mapping` field.
    pub attribute_mapping: SamlAttributeMapping,
}

#[derive(Debug, Clone, Deserialize)]
/// Represents SamlSpConfig.
pub struct SamlSpConfig {
    /// `endpoint` field.
    pub endpoint: Vec<String>,
    /// `cert_file` field.
    pub cert_file: String,
    /// `key_file` field.
    pub key_file: String,
}

#[derive(Debug, Clone, Deserialize)]
/// Represents SamlIdpMetadata.
pub struct SamlIdpMetadata {
    /// `url` field.
    pub url: Option<String>,
    /// `file` field.
    pub file: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
/// Represents SamlAttributeMapping.
pub struct SamlAttributeMapping {
    /// `uid` field.
    pub uid: String,
    /// `displayname` field.
    pub displayname: Option<String>,
    /// `email` field.
    pub email: Option<String>,
}

impl Default for SamlAttributeMapping {
    fn default() -> Self {
        Self {
            uid: "name-id".to_string(),
            displayname: None,
            email: None,
        }
    }
}
*/

/*
/// UI 认证配置。
///
/// 官方 Synapse 配置文档: <https://matrix-org.github.io/synapse/latest/usage/configuration/config_documentation.html#ui_auth>
///
/// 配置用户交互认证（UIAA）会话参数。
///
/// # 待实现功能
/// - 会话超时配置
/// - 认证流程配置
/// - 重试策略
///
/// # 配置示例
/// ```yaml
/// ui_auth:
///   session_timeout: "15m"
///   maximum_sessions: 100
/// ```
#[derive(Debug, Clone, Deserialize)]
/// Represents UiAuthConfig.
pub struct UiAuthConfig {
    /// 会话超时时间
    #[serde(default = "default_ui_auth_session_timeout")]
    /// `session_timeout` field.
    pub session_timeout: String,

    /// 最大会话数
    #[serde(default = "default_max_ui_auth_sessions")]
    /// `maximum_sessions` field.
    pub maximum_sessions: u32,
}

fn default_ui_auth_session_timeout() -> String {
    "15m".to_string()
}

fn default_max_ui_auth_sessions() -> u32 {
    100
}
*/

/*
/// 房间配置。
///
/// 官方 Synapse 配置文档: <https://matrix-org.github.io/synapse/latest/usage/configuration/config_documentation.html#rooms>
///
/// 配置房间默认参数和行为。
///
/// # 待实现功能
/// - 默认房间版本
/// - 房间导出配置
/// - 房间加入规则
/// - 房间状态事件限制
///
/// # 配置示例
/// ```yaml
/// rooms:
///   default_room_version: "10"
///   filter_room_lists: true
///   export_metrics: false
///   state_event_limit: 1000
/// ```
#[derive(Debug, Clone, Deserialize)]
/// Represents RoomsConfig.
pub struct RoomsConfig {
    /// 默认房间版本
    #[serde(default = "default_room_version")]
    /// `default_room_version` field.
    pub default_room_version: String,

    /// 是否过滤房间列表
    #[serde(default)]
    /// `filter_room_lists` field.
    pub filter_room_lists: bool,

    /// 是否导出指标
    #[serde(default)]
    /// `export_metrics` field.
    pub export_metrics: bool,

    /// 状态事件数量限制
    #[serde(default = "default_state_event_limit")]
    /// `state_event_limit` field.
    pub state_event_limit: u64,
}

fn default_room_version() -> String {
    // Single source of truth: synapse_common::room_versions::DEFAULT_ROOM_VERSION.
    // Hard-coding "11" (or "10") here as well is how this project ended up with
    // two contradictory defaults in the same capabilities response.
    crate::room_versions::DEFAULT_ROOM_VERSION.to_string()
}

fn default_state_event_limit() -> u64 {
    1000
}
*/

/*
/// 用户目录配置。
///
/// 官方 Synapse 配置文档: <https://matrix-org.github.io/synapse/latest/usage/configuration/config_documentation.html#user_directory>
///
/// 配置用户搜索目录行为。
///
/// # 待实现功能
/// - 搜索所有用户开关
/// - 用户索引更新频率
/// - 优先用户列表
/// - 搜索结果显示数量限制
///
/// # 配置示例
/// ```yaml
/// user_directory:
///   enabled: true
///   search_all_users: false
///   prefer_local_users: true
///   indexing_interval: "1h"
/// ```
#[derive(Debug, Clone, Deserialize)]
/// Represents UserDirectoryConfig.
pub struct UserDirectoryConfig {
    /// 是否启用用户目录
    #[serde(default = "default_user_directory_enabled")]
    /// `enabled` field.
    pub enabled: bool,

    /// 是否搜索所有用户（包括非共享房间的用户）
    #[serde(default)]
    /// `search_all_users` field.
    pub search_all_users: bool,

    /// 是否优先显示本地用户
    #[serde(default = "default_prefer_local_users")]
    /// `prefer_local_users` field.
    pub prefer_local_users: bool,

    /// 索引更新间隔
    #[serde(default = "default_indexing_interval")]
    /// `indexing_interval` field.
    pub indexing_interval: String,
}

fn default_user_directory_enabled() -> bool {
    true
}

fn default_prefer_local_users() -> bool {
    true
}

fn default_indexing_interval() -> String {
    "1h".to_string()
}
*/

/*
/// 性能指标配置。
///
/// 官方 Synapse 配置文档: <https://matrix-org.github.io/synapse/latest/usage/configuration/config_documentation.html#metrics>
///
/// 配置 Prometheus 性能指标导出。
///
/// # 待实现功能
/// - Prometheus 端点
/// - 指标标签配置
/// - 自定义指标
/// - OpenTelemetry 支持
///
/// # 配置示例
/// ```yaml
/// metrics:
///   enabled: true
///   port: 9148
///   labels:
///     - "instance:production"
/// ```
#[derive(Debug, Clone, Deserialize)]
/// Represents MetricsConfig.
pub struct MetricsConfig {
    /// 是否启用指标
    #[serde(default)]
    /// `enabled` field.
    pub enabled: bool,

    /// 指标端口
    #[serde(default = "default_metrics_port")]
    /// `port` field.
    pub port: u16,

    /// 额外的标签
    #[serde(default)]
    /// `labels` field.
    pub labels: Vec<String>,
}

fn default_metrics_port() -> u16 {
    9148
}
*/

/*
/// 客户端配置。
///
/// 官方 Synapse 配置文档: <https://matrix-org.github.io/synapse/latest/usage/configuration/config_documentation.html#client>
///
/// 配置客户端行为参数。
///
/// # 待实现功能
/// - 最大请求大小
/// - 同步响应配置
/// - 事件获取限制
/// - Well-known 配置
///
/// # 配置示例
/// ```yaml
/// client:
///   max_request_size: "10M"
///   max_sync_events: 100
///   well_known:
///     client_name: "Synapse (Rust)"
///     client_url: "https://github.com/element-hq/synapse"
/// ```
#[derive(Debug, Clone, Deserialize)]
/// Represents ClientConfig.
pub struct ClientConfig {
    /// 最大请求大小
    #[serde(default = "default_client_max_request_size")]
    /// `max_request_size` field.
    pub max_request_size: String,

    /// 最大同步事件数量
    #[serde(default = "default_max_sync_events")]
    /// `max_sync_events` field.
    pub max_sync_events: u64,

    /// Well-known 配置
    #[serde(default)]
    /// `well_known` field.
    pub well_known: Option<WellKnownConfig>,
}

#[derive(Debug, Clone, Deserialize)]
/// Represents WellKnownConfig.
pub struct WellKnownConfig {
    /// `client_name` field.
    pub client_name: Option<String>,
    /// `client_url` field.
    pub client_url: Option<String>,
}

fn default_client_max_request_size() -> String {
    "10M".to_string()
}

fn default_max_sync_events() -> u64 {
    100
}
*/

/*
/// 服务器通知配置。
///
/// 官方 Synapse 配置文档: <https://matrix-org.github.io/synapse/latest/usage/configuration/config_documentation.html#server_notices>
///
/// 配置服务器通知系统（用于向用户发送系统消息）。
///
/// # 待实现功能
/// - 系统通知房间配置
/// - 通知发送 API
/// - 通知模板
///
/// # 配置示例
/// ```yaml
/// server_notices:
///   system_mxid_localpart: "notices"
///   system_display_name: "Server Notices"
///   server_notices_room: "!notices:example.com"
/// ```
#[derive(Debug, Clone, Deserialize)]
/// Represents ServerNoticesConfig.
pub struct ServerNoticesConfig {
    /// 系统通知用户的 MXID 本地部分
    pub system_mxid_localpart: String,

    /// 系统通知用户的显示名称
    pub system_display_name: Option<String>,

    /// 系统通知房间 ID
    pub server_notices_room: Option<String>,
}
*/

/*
/// 第三方协议规则配置。
///
/// 官方 Synapse 配置文档: <https://matrix-org.github.io/synapse/latest/usage/configuration/config_documentation.html#third_party_rules>
///
/// 配置第三方协议桥接规则。
///
/// # 待实现功能
/// - 协议列表
/// - 网络字段
/// - 匹配规则
///
/// # 配置示例
/// ```yaml
/// third_party_rules:
///   - protocol: "irc"
///     fields:
///       - network: "freenode"
/// ```
#[derive(Debug, Clone, Deserialize)]
/// Represents ThirdPartyRulesConfig.
pub struct ThirdPartyRulesConfig {
    /// `rules` field.
    pub rules: Vec<ThirdPartyRule>,
}

#[derive(Debug, Clone, Deserialize)]
/// Represents ThirdPartyRule.
pub struct ThirdPartyRule {
    /// `protocol` field.
    pub protocol: String,
    /// `fields` field.
    pub fields: Vec<ThirdPartyField>,
}

#[derive(Debug, Clone, Deserialize)]
/// Represents ThirdPartyField.
pub struct ThirdPartyField {
    /// `network` field.
    pub network: String,
}
*/

/*
/// Sentry 错误追踪配置。
///
/// 官方 Synapse 配置文档: <https://matrix-org.github.io/synapse/latest/usage/configuration/config_documentation.html#sentry>
///
/// 配置 Sentry 错误追踪。
///
/// # 待实现功能
/// - Sentry DSN 配置
/// - 环境信息
/// - 错误采样率
///
/// # 配置示例
/// ```yaml
/// sentry:
///   enabled: true
///   dsn: "https://your-sentry-dsn"
///   environment: "production"
///   sample_rate: 0.1
/// ```
#[derive(Debug, Clone, Deserialize)]
/// Represents SentryConfig.
pub struct SentryConfig {
    /// 是否启用 Sentry
    #[serde(default)]
    /// `enabled` field.
    pub enabled: bool,

    /// Sentry DSN
    pub dsn: Option<String>,

    /// 环境名称
    pub environment: Option<String>,

    /// 采样率 (0.0 - 1.0)
    #[serde(default = "default_sentry_sample_rate")]
    /// `sample_rate` field.
    pub sample_rate: f32,
}

fn default_sentry_sample_rate() -> f32 {
    0.1
}
*/

// ============================================================================
// 配置增强说明
//
// 要启用上述配置模块，请按以下步骤操作：
//
// 1. 取消相应配置结构体的注释
// 2. 将该配置添加到主 Config 结构体中：
//    pub struct Config {
//        // ...
//        #[serde(default)]
//        pub listeners: ListenersConfig,
//        #[serde(default)]
//        pub media_store: MediaStoreConfig,
//        // ... 等等
//    }
// 3. 在配置文件（homeserver.yaml）中添加相应配置
// 4. 实现对应的功能代码（Service, Storage, Routes 等）
// 5. 添加测试用例
// 6. 更新文档
//
// 注意：启用新配置后，需要更新 Default 实现以提供合理的默认值。
// ============================================================================
