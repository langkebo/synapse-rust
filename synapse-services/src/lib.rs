// ROUND2-ISSUE-1: test code may use unwrap/expect/unwrap_err/panic per Rust testing idiom.
// Production lib code is still held to the strict clippy lint config in [lints.clippy].
#![cfg_attr(test, allow(clippy::unwrap_used, clippy::expect_used, clippy::unwrap_err_used, clippy::panic))]

// Sibling crate aliases. Downstream code accesses these via the module path
// (e.g., `synapse_services::cache::CacheManager`) rather than a flattened
// root namespace.

pub mod auth;
pub use synapse_cache as cache;
pub use synapse_common as common;
pub use synapse_e2ee as e2ee;
pub use synapse_federation as federation;
pub use synapse_storage as storage;

pub mod container;
pub use container::ServiceContainer;

pub mod shutdown;

pub mod wiring;

pub mod capability_governance;

// =============================================================================
// L0 — Core Matrix services (always compiled, required for core-private-chat)
// =============================================================================
pub mod account_data_service;
pub mod account_device_list_service;
pub mod account_identity_service;
/// Admin domain group — re-exports admin service types under `admin::`.
pub mod admin;
pub mod admin_audit_service;
pub mod admin_federation_service;
pub mod admin_media_service;
pub mod admin_registration_service;
pub mod admin_security_service;
pub mod admin_server_service;
pub mod admin_token_service;
pub mod admin_user_service;
pub mod application_service;
pub mod background_update_service;
pub mod captcha_service;
pub mod client_push_service;
pub mod content_scanner;
pub mod database_initializer;
pub mod dehydrated_device_service;
/// E2EE audit service (not the full e2ee crate — that is re-exported as `e2ee`).
pub mod e2ee_audit;
pub mod event_broadcaster_trait;
pub mod event_notifier;
pub mod event_report_service;
pub mod feature_flag_service;
pub mod federation_blacklist_service;
pub mod federation_key_rotation_service;
pub mod identity;
pub mod media;
pub mod media_quota_service;
pub mod media_service;
pub mod module_service;
pub mod oidc_service;
pub mod presence_service;
pub mod push;
pub use push::service as push_notification_service;
pub mod refresh_token_service;
pub mod registration_service;
pub mod registration_token_service;
pub mod relations_service;
pub mod retention_service;
pub mod room;
pub mod search_service;
pub mod sliding_sync_service;
pub mod sms_provider;
/// Sync services domain group — re-exports sync service types under `sync::`.
pub mod sync;
pub mod sync_helpers;
pub mod sync_service;
pub mod telemetry_service;
pub mod thread_service;
pub mod translation_service;

pub mod directory_service;
pub mod typing_service;
pub mod uia_service;
pub mod user_lock_service;
pub mod user_service;

// =============================================================================
// Explicit root re-exports of frequently used service types.
//
// P2-11 removed the old `pub use <module>::*` wildcard surface. The exports
// below keep the root API ergonomic without reintroducing an uncontrolled
// public namespace.
// =============================================================================
pub use account_device_list_service::{
    AccountDeviceListService, DeviceListDeletion, DeviceListDelta, DeviceListEntry, DeviceListSnapshot,
}; // account device list service types
pub use account_identity_service::AccountIdentityService; // account identity and threepid service types
pub use admin::*; // admin domain group (backward-compat flat re-export)
pub use application_service::{ApplicationServiceManager, ApplicationServiceScheduler, NamespacesInfo}; // application service integration types
#[allow(ambiguous_glob_reexports)]
pub use database_initializer::{
    initialize_database, DatabaseInitMode, DatabaseInitService, Environment, InitializationReport,
}; // database initialization helpers
pub use dehydrated_device_service::DehydratedDeviceService; // dehydrated device service types
pub use directory_service::{DirectoryRoom, DirectoryService}; // room directory service types
pub use feature_flag_service::FeatureFlagService; // feature flag service types
pub use federation_key_rotation_service::FederationKeyRotationService; // federation key rotation service types
#[allow(deprecated)]
pub use media_service::{MediaService, ThumbnailConfig, ThumbnailMethod, ThumbnailSettings}; // media service types
pub use oidc_service::OidcService;
pub use push::service::{NotificationPayload, PushNotificationService, PushRuleResult, SendNotificationRequest}; // push notification service types
pub use registration_service::RegistrationService; // registration service types
pub use room::api_trait::RoomServiceApi;
pub use room::service::{CreateRoomConfig, RoomService, RoomServiceConfig}; // RoomService and room config types
pub use room::space::SpaceService; // SpaceService
pub use room::summary::{
    CreateRoomSummaryRequest, CreateSummaryMemberRequest, RoomSummaryMember, RoomSummaryResponse, RoomSummaryService,
    RoomSummaryState, RoomSummaryStats, UpdateRoomSummaryRequest, UpdateSummaryMemberRequest,
}; // RoomSummaryService
pub use search_service::{
    AdvancedSearchOptions, EventContextEntry, EventContextWindow, IndexedEvent, RoomEventsSearchFilter, SearchFilters,
    SearchResult, SearchResultItem, SearchRoomEvent, SearchRoomEventsPage, SearchRoomSummary, SearchService,
    TimestampDirection, TimestampEventMatch,
}; // search service types
pub use sync::*; // sync domain group (backward-compat flat re-export)
pub use typing_service::{TypingService, TypingUser}; // typing service types
pub use user_service::UserService; // user service convenience layer

// Backward-compatible room module aliases (Phase P2-1, P2-2)
pub use room::service as room_service;
pub use room::space as space_service;
pub use room::summary as room_summary_service;

// =============================================================================
// L2 — Optional authentication extensions (feature-gated, off by default)
// =============================================================================
#[cfg(feature = "builtin-oidc")]
pub mod builtin_oidc_provider;
#[cfg(feature = "builtin-oidc")]
pub use builtin_oidc_provider::{AuthSession, BuiltinOidcProvider, RefreshToken as BuiltinRefreshToken};

// =============================================================================
// L3 — Experimental / non-core extensions (feature-gated, off by default)
// =============================================================================
#[cfg(feature = "openclaw-routes")]
pub mod matrix_ai_connection_service;
#[cfg(feature = "openclaw-routes")]
pub mod mcp_proxy;
#[cfg(feature = "openclaw-routes")]
pub mod openclaw_service;

#[cfg(feature = "friends")]
pub mod friend_room_service;
#[cfg(feature = "friends")]
pub use friend_room_service::{
    decode_friend_list_cursor, encode_friend_list_cursor, DirectMapUpdateAction, DirectRoomSnapshot, DmPartnerInfo,
    EnsureDirectRoomResult, FriendListCursor, FriendListEntry, FriendListPage, FriendListRequest,
    FriendRoomCreateRoomConfig, FriendRoomService,
};

#[cfg(feature = "voice-extended")]
pub mod voice_service;
#[cfg(feature = "voice-extended")]
pub use voice_service::{VoiceMessageUploadParams, VoiceService};

#[cfg(feature = "saml-sso")]
pub mod saml_service;

#[cfg(feature = "cas-sso")]
pub mod cas_service;

#[cfg(feature = "beacons")]
pub mod beacon_service;
#[cfg(feature = "beacons")]
pub use beacon_service::BeaconService;

// =============================================================================
// RTC domain — unified real-time communication (TURN/STUN, calls, sessions, SFU)
// =============================================================================
pub mod rtc;

// Backward-compatible re-exports from rtc domain
#[cfg(feature = "voip-tracking")]
pub use rtc::CallOrchestrationService as CallService;
#[cfg(feature = "voip-tracking")]
pub use rtc::CallOrchestrationService;
#[cfg(feature = "voip-tracking")]
pub use rtc::LivekitClient;
pub use rtc::RtcInfraService as VoipService;
pub use rtc::RtcInfraService;
pub use rtc::RtcInfraSettings;
#[cfg(feature = "voip-tracking")]
pub use rtc::RtcSessionService as MatrixRTCService;
#[cfg(feature = "voip-tracking")]
pub use rtc::RtcSessionService;
pub use rtc::TurnCredentials;
pub use rtc::VoipSettings;
#[cfg(feature = "voip-tracking")]
pub use rtc::{
    to_matrix_event, CallAnswer, CallAnswerEvent, CallCandidatesEvent, CallHangupEvent, CallInviteEvent, CallOffer,
    CallState, CreateRoomRequest, CreateRoomResponse, IceCandidate, JoinRoomRequest, JoinRoomResponse, LivekitCodec,
    LivekitError, LivekitParticipant, LivekitRoom, LivekitTrack, RoomParticipant, TrackInfo,
};
#[cfg(feature = "voip-tracking")]
pub use synapse_common::config::LivekitConfig;

#[cfg(feature = "widgets")]
pub mod widget_service;

#[cfg(feature = "server-notifications")]
pub mod server_notification_service;

#[cfg(feature = "burn-after-read")]
pub mod burn_after_read_service;

#[cfg(feature = "external-services")]
pub mod external_service_integration;
#[cfg(feature = "external-services")]
pub use external_service_integration::{
    ExternalServiceConfig, ExternalServiceIntegration, ExternalServiceType, ServiceHealthStatus, TrendRadarConfig,
    TrendRadarPayload, WebhookAuthInput, WebhookPayload,
};
#[cfg(all(feature = "external-services", feature = "openclaw-routes"))]
pub use external_service_integration::{OpenClawConfig, OpenClawPayload};

#[cfg(feature = "geo-ip")]
pub mod geo_ip;

// Worker module (moved from main crate)
pub mod worker;

// Test infrastructure (moved from main crate)
#[cfg(any(test, feature = "test-utils"))]
pub mod test_config;
#[cfg(any(test, feature = "test-utils"))]
pub mod test_utils;

// Pre-positioned Mock adapters (TDD workflow — see .claude/skills/tdd-rust/SKILL.md)
#[cfg(any(test, feature = "test-utils"))]
pub mod test_mocks;

// Internal bridge imports of sibling crates.
//
// `common` and `storage` still expose broad internal namespaces so existing
// `crate::...` references inside `synapse-services` remain stable while the
// public root API stays explicit.
pub use auth::{AuthService, Claims, ClaimsBuilder, PasswordPolicy, PasswordPolicyService, PasswordValidationResult};
pub use cache::{
    circuit_breaker, compression, federation_signature_cache, invalidation, query_cache, strategy, CacheConfig,
    CacheEntry, CacheEntryKey, CacheError, CacheInvalidationBroadcaster, CacheInvalidationConfig,
    CacheInvalidationManager, CacheInvalidationMessage, CacheInvalidationSubscriber, CacheKeyBuilder, CacheManager,
    CacheStats, CacheTtl, CircuitBreaker, CircuitBreakerMetrics, CircuitState, DegradationMetrics,
    FederationSignatureCache, InvalidationReceiver, InvalidationType, KeyRotationCallback, KeyRotationEvent,
    LocalCache, QueryCache, QueryCacheConfig, RateLimitDecision, RedisCache, SignatureCacheConfig, SignatureCacheEntry,
    SignatureCacheStats, CACHE_INVALIDATION_CHANNEL, DEFAULT_KEY_CACHE_TTL, DEFAULT_KEY_ROTATION_GRACE_PERIOD_MS,
    DEFAULT_LOCAL_CACHE_TTL_SECS, DEFAULT_REDIS_CACHE_TTL_SECS, DEFAULT_SIGNATURE_CACHE_TTL,
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
