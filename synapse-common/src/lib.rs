//! Shared types, utilities, and configuration for the synapse-rust homeserver.
//!
//! Covers error envelopes (`error`), config models (`config`), crypto helpers,
//! rate-limiting config, metrics primitives, push-rule models, and more — every
//! module that other workspace crates depend on.
//!
//! Crate-level lint is `#![deny(missing_docs)]`: any new public item without
//! a `///` comment will fail CI immediately.

// ROUND2-ISSUE-1: test code may use unwrap/expect/unwrap_err/panic per Rust testing idiom.
// Production lib code is still held to the strict clippy lint config in [lints.clippy].
#![cfg_attr(test, allow(clippy::unwrap_used, clippy::expect_used, clippy::panic))]
// B-3.1-b-2 complete: every public item in this crate now has doc comments.
// Switched to deny so any future PR adding a new pub item without docs will
// fail CI immediately. The ratchet (scripts/quality/check_missing_docs_ratchet.sh)
// enforces the same constraint across the whole workspace for non-cleared crates.
#![deny(missing_docs)]

/// Module `argon2_config`.
pub mod argon2_config;
/// Module `background_job`.
pub mod background_job;
/// Module `canonical_json`.
pub mod canonical_json;
/// Module `claims`.
pub mod claims;
/// Module `collections`.
pub mod collections;
/// Module `concurrency`.
pub mod concurrency;
/// Module `config`.
pub mod config;
/// Module `constants`.
pub mod constants;
/// Module `crypto`.
pub mod crypto;
/// Module `early_exit`.
pub mod early_exit;
/// Module `error`.
pub mod error;
/// Module `event_models`.
pub mod event_models;
/// Module `event_utils`.
pub mod event_utils;
/// Module `feature_flags`.
pub mod feature_flags;
/// Module `federation_test_keys`.
pub mod federation_test_keys;
/// Module `friend_shard`.
pub mod friend_shard;
/// Module `health`.
pub mod health;
/// Module `http_client`.
pub mod http_client;
/// Module `key_encryption`.
pub mod key_encryption;
/// Module `logging`.
pub mod logging;
/// Module `macros`.
pub mod macros;
/// Module `media_link_signer`.
pub mod media_link_signer;
/// Module `media_locator`.
pub mod media_locator;
/// Module `membership_transition`.
pub mod membership_transition;
/// Module `metrics`.
pub mod metrics;
/// Module `nonce_cache`.
pub mod nonce_cache;
/// Module `password_hash_pool`.
pub mod password_hash_pool;
/// Module `push_rules`.
pub mod push_rules;
/// Module `rate_limit_config`.
pub mod rate_limit_config;
/// Module `redaction`.
pub mod redaction;
/// Module `regex_cache`.
pub mod regex_cache;
/// Module `room_versions`.
pub mod room_versions;
/// Module `sanitizer`.
pub mod sanitizer;
/// Module `security`.
pub mod security;
/// Module `server_metrics`.
pub mod server_metrics;
/// Module `task_queue`.
pub mod task_queue;
/// Module `telemetry_config`.
pub mod telemetry_config;
/// Module `time`.
pub mod time;
/// Module `tracing`.
pub mod tracing;
/// Module `traits`.
pub mod traits;
/// Module `transaction`.
pub mod transaction;
/// Module `types`.
pub mod types;
/// Module `validation`.
pub mod validation;
/// Module `xml_parser`.
pub mod xml_parser;

// Explicit re-exports — each item is an intentional API commitment.
// Note: RateLimitConfig, RateLimitRule, RateLimitEndpointRule, and
// RateLimitMatchType are intentionally re-exported from
// `rate_limit_config` only (not from `config`) to avoid ambiguity; the
// `config`-namespace equivalents remain reachable as
// `synapse_common::config::RateLimitConfig` etc.
//
// The legacy in-memory `rate_limit` module (user/ip/endpoint triple-bucket
// `RateLimiter`) was removed on 2026-08-09: it was never wired into the
// request path (dead code) and risked being mounted as a third rate-limit
// layer. The single authoritative limiter is
// `web/middleware/rate_limit.rs` (Redis/local token bucket, configured via
// `rate_limit.yaml`).

/// Re-exported item.
pub use sanitizer::{create_sanitizer, create_strict_sanitizer, ContentSanitizer, SanitizerMode};

/// Re-exported item.
pub use argon2_config::{Argon2Config, Argon2ConfigError};
/// Re-exported item.
pub use background_job::BackgroundJob;
/// Re-exported item.
pub use canonical_json::{
    canonical_json, canonical_json_bytes, remove_signatures_and_unsigned, CanonicalEvent, CanonicalJsonError,
};
/// Re-exported item.
pub use claims::{Claims, ClaimsBuilder};
/// Re-exported item.
pub use collections::{
    hashmap_with_capacity, hashset_with_capacity, vec_with_capacity, HashMapBuilder, HashSetBuilder, VecBuilder,
};
/// Re-exported item.
pub use concurrency::{ConcurrencyController, ConcurrencyLimiter, ConcurrencyPermit};
/// Re-exported item.
pub use config::{
    default_admin_mfa_allowed_drift_steps, default_admin_rbac_enabled, default_allowed_headers,
    default_allowed_methods, default_cors_max_age, default_dehydrated_device_cleanup_interval_secs,
    default_ui_auth_session_timeout, AdminRegistrationConfig, ApnsConfig, BuiltinOidcConfig, BuiltinOidcUser,
    CircuitBreakerConfig, Config, ConfigError, ConfigManager, CorsConfig, DatabaseConfig, ExperimentalConfig,
    FcmConfig, FederationConfig, FederationRateLimitConfig, IdentityConfig, InstanceLocationConfig, LivekitConfig,
    LoggingConfig, MasConfig, OidcAttributeMapping, OidcConfig, PerformanceConfig, PolicyServerConfig,
    PostgresFtsConfig, PostgresFtsWeights, PushConfig, RedisConfig, ReplicationConfig, ReplicationHttpConfig,
    RetentionConfig, RetentionPolicy, RetentionPurgeJob, SamlAttributeMapping, SamlConfig, SearchConfig,
    SecurityConfig, ServerConfig, SmsConfig, SmtpConfig, SmtpRateLimitConfig, StreamWriters, SyncRateLimitConfig,
    TranslateConfig, TrustedKeyServer, UrlBlacklistRule, UrlPreviewConfig, VoipConfig, WebPushConfig, WorkerConfig,
};
/// Re-exported item.
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
/// Re-exported item.
pub use crypto::generate_signing_key;
/// Re-exported item.
pub use crypto::{
    compute_hash, decode_base64, decode_base64_32, decode_hex, encode_base64, encode_hex, generate_device_id,
    generate_event_id, generate_room_id, generate_salt, generate_token, hash_password, hash_password_with_config,
    hash_password_with_params, hash_token, hash_token_legacy, hmac_sha256, is_legacy_hash, migrate_password_hash,
    migrate_password_hash_with_config, random_string, secure_compare, secure_compare_bytes, validate_token_hash_secret,
    verify_password, verify_password_legacy, verify_token_hash, ServerSigningKey,
};
/// Re-exported item.
pub use early_exit::{early_continue, early_exit, early_return, EarlyExit};
/// Re-exported item.
pub use error::{init_error_metrics, ApiError, ApiErrorCause, ApiErrorKind, ApiResponse, ApiResult, MatrixErrorCode};
/// Re-exported item.
pub use event_utils::{event_to_json, event_to_json_without_age, events_to_json, events_to_json_without_age};
/// Re-exported item.
pub use feature_flags::{
    DmFlags, FeatureFlags, PusherFlags, RoomSummaryFlags, RuntimeFeatureFlagService, SpaceFlags, VerificationFlags,
};
#[cfg(any(test, feature = "test-utils"))]
/// Re-exported item.
pub use federation_test_keys::{
    generate_federation_test_keypair, sign_federation_request, verify_federation_signature, FederationTestKeypair,
};
/// Re-exported item.
pub use friend_shard::{shard_for_user_id, shard_to_state_key, sort_letter_for};
/// Re-exported item.
pub use health::{CheckResult, DatabaseHealthCheck, HealthCheck, HealthCheckLevel, HealthChecker, HealthStatus};
/// Re-exported item.
pub use key_encryption::{decrypt_key, encrypt_key, is_encrypted};
/// Re-exported item.
pub use logging::init_logging;
/// Re-exported item.
pub use media_link_signer::{MediaLinkSigner, DEFAULT_MEDIA_LINK_TTL_SECS};
/// Re-exported item.
pub use media_locator::MediaLocator;
/// Re-exported item.
pub use membership_transition::{is_legal, JoinRule, TransitionCtx, TransitionError};
/// Re-exported item.
pub use metrics::{Counter, Gauge, Histogram, Metric, MetricInventory, MetricsCollector, MetricsError};
/// Re-exported item.
pub use nonce_cache::{FederationNonceCache, DEFAULT_TIMESTAMP_SKEW, NONCE_CACHE_CAPACITY, NONCE_TTL};
/// Re-exported item.
pub use password_hash_pool::{
    get_pool_metrics, get_pool_status, PasswordHashError, PasswordHashMetrics, PasswordHashPool,
    PasswordHashPoolConfig, PoolStatus,
};
/// Re-exported item.
pub use rate_limit_config::{
    select_endpoint_rule, select_endpoint_rule_runtime, start_config_watcher, RateLimitBackend, RateLimitConfigAdapter,
    RateLimitConfigError, RateLimitConfigFile, RateLimitConfigManager, RateLimitEndpointRule, RateLimitMatchType,
    RateLimitRule, SyncRateLimitConfigFile,
};
/// Re-exported item.
pub use redaction::{
    allowed_content_keys, extract_redacts, redact_content, redact_event_for_hash, CANONICAL_JSON_TOP_LEVEL_FIELDS,
};
/// Re-exported item.
pub use regex_cache::RegexCache;
/// Re-exported item.
pub use room_versions::{
    can_create_room_version, can_federate_room_version, can_join_room_version, can_parse_room_version,
    client_room_versions_capability, federation_room_versions_capability, is_supported_room_version,
    resolve_room_version, RoomVersionCapability, RoomVersionDisposition, DEFAULT_ROOM_VERSION, SUPPORTED_ROOM_VERSIONS,
};
/// Re-exported item.
pub use security::{
    check_url_against_blacklist, check_url_and_resolve, compute_signature_hash, is_ip_in_blacklist,
    resolve_host_checked, ConstantTimeComparison, ReplayProtectionCache, ReplayProtectionConfig, ReplayProtectionStats,
    SecurityValidator,
};
#[cfg(test)]
/// Re-exported item.
pub use task_queue::{BackgroundTaskManager, TaskHandler, TaskId, TaskQueue, TaskResultValue};
/// Re-exported item.
pub use task_queue::{QueueMetrics, RedisTaskQueue, TaskQueueError};
/// Re-exported item.
pub use telemetry_config::{OpenTelemetryConfig, PrometheusConfig};
/// Re-exported item.
pub use time::{
    calculate_age, calculate_ttl, current_timestamp_millis, current_timestamp_utc, generate_pagination_token,
    generate_stream_token_from_ts, is_expired, parse_pagination_token, parse_stream_token,
};
/// Re-exported item.
pub use tracing::{DistributedTracer, RequestId, RequestIdPropagationLayer};
/// Re-exported item.
pub use transaction::{
    is_retryable_db_error, AdvisoryLockGuard, ManagedTransaction, TransactionError, TransactionManager,
    TransactionResult,
};
/// Re-exported item.
pub use types::{EventId, Membership, Presence, PresenceState, RoomAlias, RoomVersion, SecretString, UserId};
/// Re-exported item.
pub use validation::{ValidationContext, ValidationError, ValidationResult, Validator};
/// Re-exported item.
pub use xml_parser::{parse_saml_metadata, parse_saml_response, SamlAssertionData, SamlMetadataParsed, XmlParseError};
