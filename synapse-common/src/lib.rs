// ROUND2-ISSUE-1: test code may use unwrap/expect/unwrap_err/panic per Rust testing idiom.
// Production lib code is still held to the strict clippy lint config in [lints.clippy].
#![cfg_attr(test, allow(clippy::unwrap_used, clippy::expect_used, clippy::unwrap_err_used, clippy::panic))]

pub mod argon2_config;
pub mod background_job;
pub mod canonical_json;
pub mod claims;
pub mod collections;
pub mod concurrency;
pub mod config;
pub mod constants;
pub mod crypto;
pub mod early_exit;
pub mod error;
pub mod event_models;
pub mod event_utils;
pub mod feature_flags;
pub mod federation_test_keys;
pub mod health;
pub mod key_encryption;
pub mod logging;
pub mod macros;
pub mod media_link_signer;
pub mod media_locator;
pub mod membership_transition;
pub mod metrics;
pub mod nonce_cache;
pub mod password_hash_pool;
pub mod rate_limit;
pub mod rate_limit_config;
pub mod redaction;
pub mod regex_cache;
pub mod room_versions;
pub mod sanitizer;
pub mod security;
pub mod server_metrics;
pub mod task_queue;
pub mod telemetry_config;
pub mod time;
pub mod tracing;
pub mod traits;
pub mod transaction;
pub mod types;
pub mod validation;
pub mod xml_parser;

// Explicit re-exports — each item is an intentional API commitment.
// Note: RateLimitConfig, RateLimitRule, RateLimitEndpointRule, and
// RateLimitMatchType are intentionally re-exported from `rate_limit` /
// `rate_limit_config` only (not from `config`) to avoid ambiguity; the
// `config`-namespace equivalents remain reachable as
// `synapse_common::config::RateLimitConfig` etc.

pub use sanitizer::{create_sanitizer, create_strict_sanitizer, ContentSanitizer, SanitizerMode};

pub use argon2_config::{Argon2Config, Argon2ConfigError};
pub use background_job::BackgroundJob;
pub use canonical_json::{
    canonical_json, canonical_json_bytes, remove_signatures_and_unsigned, CanonicalEvent, CanonicalJsonError,
};
pub use claims::{Claims, ClaimsBuilder};
pub use collections::{
    hashmap_with_capacity, hashset_with_capacity, vec_with_capacity, HashMapBuilder, HashSetBuilder, VecBuilder,
};
pub use concurrency::{ConcurrencyController, ConcurrencyLimiter, ConcurrencyPermit};
pub use config::{
    default_admin_mfa_allowed_drift_steps, default_admin_rbac_enabled, default_allowed_headers,
    default_allowed_methods, default_cors_max_age, default_dehydrated_device_cleanup_interval_secs,
    default_ui_auth_session_timeout, AdminRegistrationConfig, ApnsConfig, BuiltinOidcConfig, BuiltinOidcUser,
    CircuitBreakerConfig, Config, ConfigError, ConfigManager, CorsConfig, DatabaseConfig, ExperimentalConfig,
    FcmConfig, FederationConfig, FederationRateLimitConfig, IdentityConfig, InstanceLocationConfig, LivekitConfig,
    LoggingConfig, OidcAttributeMapping, OidcConfig, PerformanceConfig, PolicyServerConfig, PostgresFtsConfig,
    PostgresFtsWeights, PushConfig, RedisConfig, ReplicationConfig, ReplicationHttpConfig, RetentionConfig,
    RetentionPolicy, RetentionPurgeJob, SamlAttributeMapping, SamlConfig, SearchConfig, SecurityConfig, ServerConfig,
    SmsConfig, SmtpConfig, SmtpRateLimitConfig, StreamWriters, SyncRateLimitConfig, TranslateConfig, TrustedKeyServer,
    UrlBlacklistRule, UrlPreviewConfig, VoipConfig, WebPushConfig, WorkerConfig,
};
pub use constants::{
    millis, secs, ADMIN_REGISTER_NONCE_RATE_LIMIT, ADMIN_REGISTER_RATE_LIMIT, BURN_AFTER_READ_DELAY_SECS,
    DB_ACQUIRE_TIMEOUT_SECS, DEFAULT_ACCESS_TOKEN_EXPIRY_SECS, DEFAULT_CACHE_TTL_SECONDS, DEFAULT_DB_MAX_CONNECTIONS,
    DEFAULT_GUEST_ACCESS, DEFAULT_HISTORY_VISIBILITY, DEFAULT_JOIN_RULE, DEFAULT_PAGE_SIZE,
    DEFAULT_REFRESH_TOKEN_EXPIRY_SECS, MAX_DEVICE_ID_LENGTH, MAX_DISPLAY_NAME_LENGTH, MAX_MESSAGE_LENGTH,
    MAX_PAGINATION_LIMIT, MAX_PASSWORD_LENGTH, MAX_REASON_LENGTH, MAX_ROOM_ALIAS_LENGTH, MAX_USERNAME_LENGTH,
    MAX_VOICE_DATA_SIZE, MIN_PAGINATION_LIMIT, MIN_PASSWORD_LENGTH, MIN_USERNAME_LENGTH, SESSION_IDLE_TIMEOUT_SECS,
    SESSION_MAX_LIFETIME_SECS, TIMESTAMP_WINDOW_SECONDS, TOKEN_BUCKET_CAPACITY, USER_PROFILE_CACHE_TTL,
};
#[cfg(test)]
pub use crypto::generate_signing_key;
pub use crypto::{
    compute_hash, decode_base64, decode_base64_32, decode_hex, encode_base64, encode_hex, generate_device_id,
    generate_event_id, generate_room_id, generate_salt, generate_token, hash_password, hash_password_with_config,
    hash_password_with_params, hash_token, hash_token_legacy, hmac_sha256, is_legacy_hash, migrate_password_hash,
    migrate_password_hash_with_config, random_string, secure_compare, secure_compare_bytes, validate_token_hash_secret,
    verify_password, verify_password_legacy, verify_token_hash, ServerSigningKey,
};
pub use early_exit::{early_continue, early_exit, early_return, EarlyExit};
pub use error::{
    init_error_metrics, ApiError, ApiErrorCause, ApiErrorKind, ApiResponse, ApiResult, ErrorSource, MatrixErrorCode,
};
pub use event_utils::{event_to_json, event_to_json_without_age, events_to_json, events_to_json_without_age};
pub use feature_flags::{
    DmFlags, FeatureFlags, PusherFlags, RoomSummaryFlags, RuntimeFeatureFlagService, SpaceFlags, VerificationFlags,
};
#[cfg(any(test, feature = "test-utils"))]
pub use federation_test_keys::{
    generate_federation_test_keypair, sign_federation_request, verify_federation_signature, FederationTestKeypair,
};
pub use health::{CheckResult, DatabaseHealthCheck, HealthCheck, HealthCheckLevel, HealthChecker, HealthStatus};
pub use key_encryption::{decrypt_key, encrypt_key, is_encrypted};
pub use logging::init_logging;
pub use media_link_signer::{MediaLinkSigner, DEFAULT_MEDIA_LINK_TTL_SECS};
pub use media_locator::MediaLocator;
pub use membership_transition::{is_legal, JoinRule, TransitionCtx, TransitionError};
pub use metrics::{Counter, Gauge, Histogram, Metric, MetricInventory, MetricsCollector, MetricsError};
pub use nonce_cache::{FederationNonceCache, DEFAULT_TIMESTAMP_SKEW, NONCE_CACHE_CAPACITY, NONCE_TTL};
pub use password_hash_pool::{
    get_pool_metrics, get_pool_status, PasswordHashError, PasswordHashMetrics, PasswordHashPool,
    PasswordHashPoolConfig, PoolStatus,
};
#[allow(deprecated)] // RateLimitConfig is a deprecated alias kept for API compatibility
pub use rate_limit::{RateLimitConfig, RateLimitInfo, RateLimitState, RateLimitStats, RateLimiter};
pub use rate_limit_config::{
    select_endpoint_rule, select_endpoint_rule_runtime, start_config_watcher, RateLimitBackend, RateLimitConfigAdapter,
    RateLimitConfigError, RateLimitConfigFile, RateLimitConfigManager, RateLimitEndpointRule, RateLimitMatchType,
    RateLimitRule, SyncRateLimitConfigFile,
};
pub use redaction::{
    allowed_content_keys, extract_redacts, redact_content, redact_event_for_hash, CANONICAL_JSON_TOP_LEVEL_FIELDS,
};
pub use regex_cache::RegexCache;
pub use room_versions::{
    can_create_room_version, can_federate_room_version, can_join_room_version, can_parse_room_version,
    client_room_versions_capability, federation_room_versions_capability, is_supported_room_version,
    resolve_room_version, RoomVersionCapability, RoomVersionDisposition, DEFAULT_ROOM_VERSION, SUPPORTED_ROOM_VERSIONS,
};
pub use security::{
    check_url_against_blacklist, compute_signature_hash, is_ip_in_blacklist, ConstantTimeComparison,
    ReplayProtectionCache, ReplayProtectionConfig, ReplayProtectionStats, SecurityValidator,
};
#[cfg(test)]
pub use task_queue::{BackgroundTaskManager, TaskHandler, TaskId, TaskQueue, TaskResultValue};
pub use task_queue::{QueueMetrics, RedisTaskQueue, TaskQueueError};
pub use telemetry_config::{OpenTelemetryConfig, PrometheusConfig};
pub use time::{
    calculate_age, calculate_ttl, current_timestamp_millis, current_timestamp_utc, generate_stream_token_from_ts,
    is_expired, parse_stream_token,
};
pub use tracing::{DistributedTracer, RequestId, RequestIdPropagationLayer};
pub use transaction::{
    is_retryable_db_error, AdvisoryLockGuard, ManagedTransaction, TransactionError, TransactionManager,
    TransactionResult,
};
pub use types::{EventId, Membership, Presence, PresenceState, RoomAlias, RoomVersion, SecretString, UserId};
pub use validation::{ValidationContext, ValidationError, ValidationResult, Validator};
pub use xml_parser::{parse_saml_metadata, parse_saml_response, SamlAssertionData, SamlMetadataParsed, XmlParseError};
