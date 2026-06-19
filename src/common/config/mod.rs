mod loader;
mod manager;
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
pub mod sms;
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
    default_allowed_methods, default_cors_max_age, default_ui_auth_session_timeout, AdminRegistrationConfig,
    CorsConfig, SecurityConfig,
};
pub use server::{default_dehydrated_device_cleanup_interval_secs, ServerConfig};
pub use sms::SmsConfig;
pub use smtp::SmtpConfig;
pub use translate::TranslateConfig;
pub use voip::{
    ApnsConfig, FcmConfig, LivekitConfig, PushConfig, UrlBlacklistRule, UrlPreviewConfig, VoipConfig, WebPushConfig,
};
pub use worker::{InstanceLocationConfig, ReplicationConfig, ReplicationHttpConfig, StreamWriters, WorkerConfig};

// ============================================================================
// Config — central configuration struct
// ============================================================================
//
// `Config` is defined in `synapse_common::config` and re-exported here so
// that root-crate code can keep using `crate::common::config::Config`. All
// field types (ServerConfig, DatabaseConfig, …) are already re-exports from
// `synapse_common`, so the canonical `Config` is structurally identical to
// the historical root-only definition.

pub use synapse_common::config::Config;
