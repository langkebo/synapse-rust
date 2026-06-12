use serde::Deserialize;

mod manager;
mod loader;
mod validation;

pub use manager::ConfigManager;

// ============================================================================
// Sub-module declarations
// ============================================================================

pub mod auth;
pub mod builtin_oidc;
pub mod database;
pub mod error;
pub mod experimental;
pub mod federation;
pub mod identity;
pub mod logging;
pub mod performance;
pub mod policy_server;
pub mod push;
pub mod rate_limit;
pub mod retention;
pub mod search;
pub mod security;
pub mod server;
pub mod smtp;
#[cfg(test)]
mod tests;
pub mod translate;
pub mod voip;
pub mod worker;

// ============================================================================
// Re-exports for backward compatibility
// ============================================================================

pub use auth::{OidcAttributeMapping, OidcConfig, SamlAttributeMapping, SamlConfig};
pub use builtin_oidc::{BuiltinOidcConfig, BuiltinOidcUser};
pub use database::{CircuitBreakerConfig, DatabaseConfig, RedisConfig};
pub use error::ConfigError;
pub use experimental::ExperimentalConfig;
pub use federation::{FederationConfig, TrustedKeyServer};
pub use identity::IdentityConfig;
pub use logging::LoggingConfig;
pub use performance::PerformanceConfig;
pub use policy_server::PolicyServerConfig;
pub use rate_limit::{RateLimitConfig, RateLimitEndpointRule, RateLimitMatchType, RateLimitRule, SyncRateLimitConfig};
pub use retention::{RetentionConfig, RetentionPolicy, RetentionPurgeJob};
pub use search::{PostgresFtsConfig, PostgresFtsWeights, SearchConfig};
pub use security::{
    default_admin_mfa_allowed_drift_steps, default_admin_rbac_enabled, default_allowed_headers,
    default_allowed_methods, default_cors_max_age, default_ui_auth_session_timeout,
    AdminRegistrationConfig, CorsConfig, SecurityConfig,
};
pub use server::{default_dehydrated_device_cleanup_interval_secs, ServerConfig};
pub use smtp::SmtpConfig;
pub use translate::TranslateConfig;
pub use voip::{
    ApnsConfig, FcmConfig, LivekitConfig, PushConfig, UrlBlacklistRule, UrlPreviewConfig, VoipConfig, WebPushConfig,
};
pub use worker::{InstanceLocationConfig, ReplicationConfig, ReplicationHttpConfig, StreamWriters, WorkerConfig};

// ============================================================================
// Config — central configuration struct
// ============================================================================

/// Main configuration class for the Matrix Homeserver, containing all configuration sub-items.
/// Loaded via environment variables or configuration file.
#[derive(Debug, Clone, Deserialize, Default)]
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
    pub rate_limit: RateLimitConfig,
    /// Admin registration configuration
    #[serde(default)]
    pub admin_registration: AdminRegistrationConfig,
    /// Worker node configuration
    #[serde(default)]
    pub worker: WorkerConfig,
    /// CORS configuration
    #[serde(default)]
    pub cors: CorsConfig,
    /// SMTP email configuration
    #[serde(default)]
    pub smtp: SmtpConfig,
    /// VoIP/TURN configuration
    #[serde(default)]
    pub voip: VoipConfig,
    /// Livekit SFU configuration
    #[serde(default)]
    pub livekit: LivekitConfig,
    /// Push notification configuration
    #[serde(default)]
    pub push: PushConfig,
    /// URL preview configuration
    #[serde(default)]
    pub url_preview: UrlPreviewConfig,
    /// OIDC single sign-on configuration (external Provider)
    #[serde(default)]
    pub oidc: OidcConfig,
    /// Built-in OIDC Provider configuration
    #[serde(default)]
    pub builtin_oidc: BuiltinOidcConfig,
    /// SAML single sign-on configuration
    #[serde(default)]
    pub saml: SamlConfig,
    /// Message retention policy configuration
    #[serde(default)]
    pub retention: RetentionConfig,
    /// OpenTelemetry configuration
    #[serde(default)]
    pub telemetry: crate::common::telemetry_config::OpenTelemetryConfig,
    /// Prometheus configuration
    #[serde(default)]
    pub prometheus: crate::common::telemetry_config::PrometheusConfig,
    /// Performance optimization configuration
    #[serde(default)]
    pub performance: PerformanceConfig,
    /// Experimental feature configuration
    #[serde(default)]
    pub experimental: ExperimentalConfig,
    /// Identity Server configuration
    #[serde(default)]
    pub identity: IdentityConfig,
    /// Translation service configuration
    #[serde(default)]
    pub translate: TranslateConfig,
}

// ============================================================================
// Config — convenience accessors
// ============================================================================

impl Config {
    pub fn database_url(&self) -> String {
        format!(
            "postgres://{}:{}@{}:{}/{}",
            self.database.username, self.database.password, self.database.host, self.database.port, self.database.name
        )
    }

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
}
