// Crate-level allow: several wildcard re-exports below (cache, common, federation,
// storage, auth) intentionally overlap because they re-export items from sibling
// crates that share identically-named types (e.g. error types, config structs).
// Removing this attribute would produce `ambiguous_glob_reexports` warnings on
// those lines. The per-line `#[allow(ambiguous_glob_reexports)]` attributes mark
// the known-ambiguous sites. TODO: Replace wildcard re-exports with explicit
// exports for better API control (P2-11).
#![allow(ambiguous_glob_reexports)]

pub mod auth;
pub use synapse_cache as cache;
pub use synapse_common as common;
pub use synapse_e2ee as e2ee;
pub use synapse_federation as federation;
pub use synapse_storage as storage;

pub mod container;
pub use container::ServiceContainer;

// =============================================================================
// L0 — Core Matrix services (always compiled, required for core-private-chat)
// =============================================================================
pub mod account_data_service;
pub mod account_device_list_service;
pub mod account_identity_service;
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
pub mod oidc_mapping_service;
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
pub mod room_tag_service;
pub mod search_service;
pub mod sliding_sync_service;
pub mod sms_provider;
pub mod sync_service;
pub mod telemetry_service;
pub mod thread_service;
pub mod translation_service;

pub mod directory_service;
pub mod email_verification_service;
pub mod typing_service;
pub mod uia_service;
pub mod user_lock_service;

// =============================================================================
// Wildcard re-exports of service types.
//
// Each `pub use <module>::*` below re-exports the public service structs/traits
// (e.g. `AdminAuditService`, `RoomService`) so callers can write
// `synapse_services::AdminAuditService` instead of the fully-qualified path.
// These are wildcards for backward compatibility: the modules historically
// exposed a flat surface and many call sites rely on the short paths.
// TODO: Replace with explicit exports for better API control (P2-11).
// =============================================================================
pub use account_device_list_service::*; // account device list service types
pub use account_identity_service::*; // account identity and threepid service types
pub use admin_audit_service::*; // AdminAuditService
pub use admin_federation_service::*; // admin federation management service
pub use admin_registration_service::*; // admin registration management service
pub use admin_user_service::*; // admin user management service
pub use application_service::*; // application service integration types
pub use database_initializer::*; // database initialization helpers
pub use dehydrated_device_service::*; // dehydrated device service types
pub use directory_service::*; // room directory service types
pub use email_verification_service::*; // email verification service types
pub use feature_flag_service::*; // feature flag service types
pub use federation_key_rotation_service::*; // federation key rotation service types
pub use media_service::*; // media service types
pub use oidc_service::OidcService;
pub use presence_service::*;
pub use push::service::*; // push notification service types
pub use registration_service::*; // registration service types
pub use room::service::*; // RoomService and room config types
pub use room::space::*; // SpaceService
pub use room::summary::*; // RoomSummaryService
pub use search_service::*; // search service types
pub use sliding_sync_service::*; // sliding sync service types
pub use sync_service::*; // sync service types
pub use typing_service::*; // typing service types

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
// Wildcard re-export: friend room service types. TODO: explicit exports (P2-11).
#[cfg(feature = "friends")]
pub use friend_room_service::*;

#[cfg(feature = "voice-extended")]
pub mod voice_service;
// Wildcard re-export: voice service types. TODO: explicit exports (P2-11).
#[cfg(feature = "voice-extended")]
pub use voice_service::*;

#[cfg(feature = "saml-sso")]
pub mod saml_service;

#[cfg(feature = "cas-sso")]
pub mod cas_service;

#[cfg(feature = "beacons")]
pub mod beacon_service;
// Wildcard re-export: beacon service types. TODO: explicit exports (P2-11).
#[cfg(feature = "beacons")]
pub use beacon_service::*;

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
// Wildcard re-export: external service integration types. TODO: explicit exports (P2-11).
#[cfg(feature = "external-services")]
pub use external_service_integration::*;

#[cfg(feature = "geo-ip")]
pub mod geo_ip;

// Worker module (moved from main crate)
pub mod worker;

// Test infrastructure (moved from main crate)
#[cfg(any(test, feature = "test-utils"))]
pub mod test_config;
#[cfg(any(test, feature = "test-utils"))]
pub mod test_utils;

// Re-exports for backward compatibility.
//
// The following wildcard re-exports re-export the public API of sibling crates
// (auth, cache, common, federation, storage) so that downstream crates can
// depend on `synapse_services` alone. These are the most coupling-prone
// wildcards because the sibling crates share identically-named types
// (error enums, config structs), which is why `ambiguous_glob_reexports`
// is allowed on four of them. TODO: Replace with explicit exports for
// better API control (P2-11).
pub use auth::*; // AuthService, GuestAuthExt, PasswordPolicy, PasswordPolicyService, ...
#[allow(ambiguous_glob_reexports)]
pub use cache::*; // ambiguous: overlaps with common/storage re-exports
#[allow(ambiguous_glob_reexports)]
pub use common::*; // ambiguous: overlaps with cache/federation/storage re-exports
#[allow(ambiguous_glob_reexports)]
pub use federation::*; // ambiguous: overlaps with common re-exports
pub use storage::PresenceStorage;
#[allow(ambiguous_glob_reexports)]
pub use storage::*; // ambiguous: overlaps with common re-exports
