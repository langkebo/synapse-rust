//! The `synapse-services` crate: shared service-layer types and trait definitions
//! for the synapse-rust homeserver. Implements room, auth, registration, push,
//! sync, OIDC, and other application-level services.

// ROUND2-ISSUE-1: test code may use unwrap/expect/unwrap_err/panic per Rust testing idiom.
// Production lib code is still held to the strict clippy lint config in [lints.clippy].
#![cfg_attr(test, allow(clippy::unwrap_used, clippy::expect_used, clippy::panic))]
// B-3.1-b-5: synapse-services fully documented + deny(missing_docs).
// ratchet baseline tracked in scripts/quality/check_missing_docs_ratchet.sh;
// this crate is now at zero missing-doc warnings under `cargo doc`.
#![deny(missing_docs)]

// Sibling crate aliases. Downstream code accesses these via the module path
// (e.g., `synapse_services::cache::CacheManager`) rather than a flattened
// root namespace.

/// The `auth` module.
pub mod auth;
pub use synapse_cache as cache;
pub use synapse_common as common;
pub use synapse_e2ee as e2ee;
pub use synapse_federation as federation;
pub use synapse_storage as storage;

/// The `container` module.
pub mod container;
pub use container::ServiceContainer;

/// The `wiring` module.
pub mod wiring;

/// The `error` module — unified service-layer error types.
pub mod error;

/// The `capability_governance` module.
pub mod capability_governance;

// =============================================================================
// L0 — Core Matrix services (always compiled, required for core-private-chat)
// =============================================================================
/// Account services domain group — re-exports account service types under `account::`.
pub mod account;
/// The `account_data_service` module.
pub mod account_data_service;
/// The `account_device_list_service` module.
pub mod account_device_list_service;
/// The `account_identity_service` module.
pub mod account_identity_service;
/// Admin domain group — re-exports admin service types under `admin::`.
pub mod admin;
/// The `admin_audit_service` module.
pub mod admin_audit_service;
/// The `admin_federation_service` module.
pub mod admin_federation_service;
/// The `admin_media_service` module.
pub mod admin_media_service;
/// The `admin_registration_service` module.
pub mod admin_registration_service;
/// The `admin_security_service` module.
pub mod admin_security_service;
/// The `admin_server_service` module.
pub mod admin_server_service;
/// The `admin_token_service` module.
pub mod admin_token_service;
/// The `admin_user_service` module.
pub mod admin_user_service;
/// Application services domain group — re-exports application modules under `application::`.
pub mod application;
/// The `application_service` module.
pub mod application_service;
/// The `background_update_service` module.
pub mod background_update_service;
/// The `captcha_service` module.
pub mod captcha_service;
/// The `client_push_service` module.
pub mod client_push_service;
/// The `content_scanner` module.
pub mod content_scanner;
/// The `database_initializer` module.
pub mod database_initializer;
/// The `dehydrated_device_service` module.
pub mod dehydrated_device_service;
/// The `delayed_event_service` module.
pub mod delayed_event_service;
/// E2EE audit service (not the full e2ee crate — that is re-exported as `e2ee`).
pub mod e2ee_audit;
/// The `email_verification_service` module.
pub mod email_verification_service;
/// Event services domain group — re-exports event service types under `event::`.
pub mod event;
/// The `event_broadcaster_trait` module.
pub mod event_broadcaster_trait;
/// The `event_notifier` module.
pub mod event_notifier;
/// The `event_redaction_service` module.
pub mod event_redaction_service;
/// The `event_report_service` module.
pub mod event_report_service;
/// The `feature_flag_service` module.
pub mod feature_flag_service;
/// The `federation_blacklist_service` module.
pub mod federation_blacklist_service;
/// The `federation_key_rotation_service` module.
pub mod federation_key_rotation_service;
/// Identity services domain group — re-exports identity service types under `identity::`.
pub mod identity;
/// Infrastructure services domain group — re-exports infra service types under `infra::`.
pub mod infra;
/// The `invite_blocklist_service` module.
pub mod invite_blocklist_service;
/// The `login_token_service` module.
pub mod login_token_service;
/// The `media` module.
pub mod media;
/// The `media_quota_service` module.
pub mod media_quota_service;
/// The `media_service` module.
pub mod media_service;
/// The `module_service` module.
pub mod module_service;
/// Wake-up decorator around the storage-layer event writer, used to release
/// long-polling sliding-sync clients as soon as an event is persisted.
pub mod notifying_event_writer;
/// The `oidc_service` module.
pub mod oidc_service;
/// The `oidc_session_service` module.
pub mod oidc_session_service;
/// The `oidc_user_mapping_service` module.
pub mod oidc_user_mapping_service;
/// The `presence_service` module.
pub mod presence_service;
/// The `push` module.
pub mod push;
pub use push::service as push_notification_service;
/// The `policy_service` module.
pub mod policy_service;
/// The `refresh_token_service` module.
pub mod refresh_token_service;
/// The `registration_service` module.
pub mod registration_service;
/// The `registration_token_service` module.
pub mod registration_token_service;
/// The `relations_service` module.
pub mod relations_service;
/// The `rendezvous_service` module.
pub mod rendezvous_service;
/// The `retention_service` module.
pub mod retention_service;
/// The `room` module.
pub mod room;
/// The `search_service` module.
pub mod search_service;
/// The `sliding_sync_service` module.
pub mod sliding_sync_service;
/// The `sms_provider` module.
pub mod sms_provider;
/// Sync services domain group — re-exports sync service types under `sync::`.
pub mod sync;
/// The `sync_helpers` module.
pub mod sync_helpers;
/// The `sync_service` module.
pub mod sync_service;
/// The `telemetry_service` module.
pub mod telemetry_service;
/// The `thread_service` module.
pub mod thread_service;
/// The `translation_service` module.
pub mod translation_service;

/// The `typing_service` module.
pub mod typing_service;
/// The `uia_service` module.
pub mod uia_service;
/// The `user_service` module.
pub mod user_service;

// =============================================================================
// L2 — Optional authentication extensions (feature-gated, off by default)
// =============================================================================
/// The `builtin_oidc_provider` module.
#[cfg(feature = "builtin-oidc")]
pub mod builtin_oidc_provider;

// =============================================================================
// L3 — Experimental / non-core extensions (feature-gated, off by default)
// =============================================================================
/// The `friend_room_service` module.
#[cfg(feature = "friends")]
pub mod friend_room_service;

/// The `voice_service` module.
#[cfg(feature = "voice-extended")]
pub mod voice_service;

/// The `saml_service` module.
#[cfg(feature = "saml-sso")]
pub mod saml_service;

/// The `cas_service` module.
#[cfg(feature = "cas-sso")]
pub mod cas_service;

/// The `beacon_service` module.
#[cfg(feature = "beacons")]
pub mod beacon_service;

// =============================================================================
// RTC domain — unified real-time communication (TURN/STUN, calls, sessions, SFU)
// =============================================================================
/// The `rtc` module.
pub mod rtc;

#[cfg(feature = "voip-tracking")]
pub use rtc::CallOrchestrationService;
pub use rtc::RtcInfraService;
pub use rtc::RtcInfraSettings;
#[cfg(feature = "voip-tracking")]
pub use rtc::RtcSessionService;
pub use rtc::TurnCredentials;
pub use rtc::VoipSettings;
#[cfg(feature = "voip-tracking")]
pub use rtc::{
    to_matrix_event, CallAnswer, CallAnswerEvent, CallCandidatesEvent, CallHangupEvent, CallInviteEvent, CallOffer,
    CallState, IceCandidate,
};
#[cfg(feature = "voip-tracking")]
pub use synapse_common::config::LivekitConfig;

/// The `widget_service` module.
#[cfg(feature = "widgets")]
pub mod widget_service;

/// The `server_notification_service` module.
#[cfg(feature = "server-notifications")]
pub mod server_notification_service;

/// The `burn_after_read_service` module.
#[cfg(feature = "burn-after-read")]
pub mod burn_after_read_service;

/// The `external_service_integration` module.
#[cfg(feature = "external-services")]
pub mod external_service_integration;

// Worker module (moved from main crate)
/// The `worker` module.
pub mod worker;

// Test infrastructure (moved from main crate)
/// The `test_config` module.
#[cfg(any(test, feature = "test-utils"))]
pub mod test_config;
/// The `test_utils` module.
#[cfg(any(test, feature = "test-utils"))]
pub mod test_utils;

/// Test-build-only schema-cleanup exit hook (B'). See the module docs.
#[cfg(test)]
mod test_exit_hook;

// Pre-positioned Mock adapters (TDD workflow — see .claude/skills/tdd-rust/SKILL.md)
/// One-way error-conversion golden tests (B3-5 / A9).
#[cfg(any(test, feature = "test-utils"))]
pub mod error_conversion_tests;
/// The `test_mocks` module.
#[cfg(any(test, feature = "test-utils"))]
pub mod test_mocks;

// Internal bridge imports of sibling crates.
//
// `common` and `storage` still expose broad internal namespaces so existing
// `crate::...` references inside `synapse-services` remain stable while the
// public root API stays explicit.
pub use auth::{AuthService, Claims, ClaimsBuilder, PasswordPolicy, PasswordPolicyService, PasswordValidationResult};
pub use cache::{
    circuit_breaker, compression, federation_signature_cache, invalidation, strategy, CacheConfig, CacheEntryKey,
    CacheError, CacheInvalidationBroadcaster, CacheInvalidationConfig, CacheInvalidationManager,
    CacheInvalidationMessage, CacheInvalidationSubscriber, CacheKeyBuilder, CacheManager, CacheTtl, CircuitBreaker,
    CircuitBreakerMetrics, CircuitState, DegradationMetrics, FederationSignatureCache, InvalidationReceiver,
    InvalidationType, KeyRotationCallback, KeyRotationEvent, LocalCache, RateLimitDecision, RedisCache,
    SignatureCacheConfig, SignatureCacheEntry, SignatureCacheStats, CACHE_INVALIDATION_CHANNEL, DEFAULT_KEY_CACHE_TTL,
    DEFAULT_KEY_ROTATION_GRACE_PERIOD_MS, DEFAULT_LOCAL_CACHE_TTL_SECS, DEFAULT_REDIS_CACHE_TTL_SECS,
    DEFAULT_SIGNATURE_CACHE_TTL,
}; // cache crate root items
pub(crate) use common::*; // internal crate access; no longer flattened into public API
pub use federation::{
    client, device_sync, event_auth, event_broadcaster, key_rotation, memory_tracker, signing, state_resolution,
    DeviceSyncManager, EventAuthChain, EventBroadcaster, FederationClient, FederationMemoryReport,
    FederationMemoryTracker, KeyRotationManager, MemoryStats,
}; // federation crate root items
#[cfg(feature = "friends")]
pub use federation::{friend, FriendFederation, FriendFederationClient};
pub use storage::PresenceStorage;
pub(crate) use storage::*; // internal crate access; no longer flattened into public API
