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
pub mod identity;
pub mod media;
pub mod media_quota_service;
pub mod media_service;
pub mod module_service;
pub mod oidc_mapping_service;
pub mod oidc_service;
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
pub mod typing_service;
pub mod uia_service;
pub mod user_lock_service;

pub use admin_audit_service::*;
pub use admin_federation_service::*;
pub use admin_registration_service::*;
pub use admin_user_service::*;
pub use application_service::*;
pub use database_initializer::*;
pub use dehydrated_device_service::*;
pub use directory_service::*;
pub use feature_flag_service::*;
pub use media_service::*;
pub use oidc_service::OidcService;
pub use push::service::*;
pub use registration_service::*;
pub use room::service::*;
pub use room::space::*;
pub use room::summary::*;
pub use search_service::*;
pub use sliding_sync_service::*;
pub use sync_service::*;
pub use typing_service::*;

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
pub use friend_room_service::*;

#[cfg(feature = "voice-extended")]
pub mod voice_service;
#[cfg(feature = "voice-extended")]
pub use voice_service::*;

#[cfg(feature = "saml-sso")]
pub mod saml_service;

#[cfg(feature = "cas-sso")]
pub mod cas_service;

#[cfg(feature = "beacons")]
pub mod beacon_service;
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

// Re-exports for backward compatibility
pub use auth::*;
#[allow(ambiguous_glob_reexports)]
pub use cache::*;
#[allow(ambiguous_glob_reexports)]
pub use common::*;
#[allow(ambiguous_glob_reexports)]
pub use federation::*;
pub use storage::PresenceStorage;
#[allow(ambiguous_glob_reexports)]
pub use storage::*;
