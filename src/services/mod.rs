pub mod container;
#[cfg(any(test, feature = "test-utils"))]
pub mod test_config;
pub use container::ServiceContainer;

// =============================================================================
// L0 — Core Matrix services (always compiled, required for core-private-chat)
// =============================================================================
pub use synapse_services::{
    account_data_service, admin_audit_service, admin_federation_service, admin_media_service,
    admin_registration_service, admin_security_service, admin_server_service, admin_token_service,
    admin_user_service, application_service, background_update_service, captcha_service, client_push_service,
    content_scanner, database_initializer, dehydrated_device_service, e2ee_audit, event_notifier,
    event_report_service, feature_flag_service, federation_blacklist_service, identity, media, media_quota_service,
    media_service, module_service, oidc_mapping_service, oidc_service, push, push_notification_service,
    refresh_token_service, registration_service, registration_token_service, relations_service, retention_service,
    room, room_tag_service, search_service, sliding_sync_service, sync_service, telemetry_service, thread_service,
    translation_service,
};

pub use synapse_services::{directory_service, typing_service, uia_service};

pub use synapse_services::auth;

pub use synapse_services::e2ee_audit as e2ee;

// =============================================================================
// Service re-exports
// =============================================================================
pub use synapse_services::{
    admin_audit_service::*, admin_federation_service::*, admin_media_service::*, admin_registration_service::*,
    admin_security_service::*, admin_server_service::*, admin_token_service::*, admin_user_service::*,
    application_service::*, dehydrated_device_service::*, directory_service::*, feature_flag_service::*,
    media_service::*, oidc_mapping_service::*, push::service::*, registration_service::*, room::service::*,
    room::space::*, room::summary::*, room_tag_service::*, search_service::*, sliding_sync_service::*,
    sync_service::*, typing_service::*,
};

pub use synapse_services::oidc_service::OidcService;

// Backward-compatible room module aliases (Phase P2-1, P2-2)
pub use synapse_services::room::service as room_service;
pub use synapse_services::room::space as space_service;
pub use synapse_services::room::summary as room_summary_service;

// =============================================================================
// L2 — Optional authentication extensions (feature-gated, off by default)
// =============================================================================
#[cfg(feature = "builtin-oidc")]
pub use synapse_services::builtin_oidc_provider;
#[cfg(feature = "builtin-oidc")]
pub use synapse_services::{AuthSession, BuiltinOidcProvider, BuiltinRefreshToken};

// =============================================================================
// L3 — Experimental / non-core extensions (feature-gated, off by default)
// =============================================================================
#[cfg(feature = "openclaw-routes")]
pub use synapse_services::{matrix_ai_connection_service, mcp_proxy, openclaw_service};

#[cfg(feature = "friends")]
pub use synapse_services::friend_room_service;

#[cfg(feature = "voice-extended")]
pub use synapse_services::voice_service;
#[cfg(feature = "voice-extended")]
pub use synapse_services::voice_service::*;

#[cfg(feature = "saml-sso")]
pub use synapse_services::saml_service;

#[cfg(feature = "cas-sso")]
pub use synapse_services::cas_service;

#[cfg(feature = "beacons")]
pub use synapse_services::beacon_service;
#[cfg(feature = "beacons")]
pub use synapse_services::beacon_service::*;

// =============================================================================
// RTC domain — unified real-time communication (TURN/STUN, calls, sessions, SFU)
// =============================================================================
pub use synapse_services::rtc;

// Backward-compatible re-exports from rtc domain
#[cfg(feature = "voip-tracking")]
pub use crate::common::config::LivekitConfig;
#[cfg(feature = "voip-tracking")]
pub use synapse_services::rtc::CallOrchestrationService as CallService;
#[cfg(feature = "voip-tracking")]
pub use synapse_services::rtc::CallOrchestrationService;
#[cfg(feature = "voip-tracking")]
pub use synapse_services::rtc::LivekitClient;
pub use synapse_services::rtc::RtcInfraService as VoipService;
pub use synapse_services::rtc::RtcInfraService;
pub use synapse_services::rtc::RtcInfraSettings;
#[cfg(feature = "voip-tracking")]
pub use synapse_services::rtc::RtcSessionService as MatrixRTCService;
#[cfg(feature = "voip-tracking")]
pub use synapse_services::rtc::RtcSessionService;
pub use synapse_services::rtc::TurnCredentials;
pub use synapse_services::rtc::VoipSettings;
#[cfg(feature = "voip-tracking")]
pub use synapse_services::rtc::{
    to_matrix_event, CallAnswer, CallAnswerEvent, CallCandidatesEvent, CallHangupEvent, CallInviteEvent, CallOffer,
    CallState, CreateRoomRequest, CreateRoomResponse, IceCandidate, JoinRoomRequest, JoinRoomResponse, LivekitCodec,
    LivekitError, LivekitParticipant, LivekitRoom, LivekitTrack, RoomParticipant, TrackInfo,
};

#[cfg(feature = "widgets")]
pub use synapse_services::widget_service;

#[cfg(feature = "server-notifications")]
pub use synapse_services::server_notification_service;

#[cfg(feature = "burn-after-read")]
pub use synapse_services::burn_after_read_service;

#[cfg(feature = "external-services")]
pub use synapse_services::external_service_integration;
#[cfg(feature = "external-services")]
pub use synapse_services::external_service_integration::*;

#[cfg(feature = "geo-ip")]
pub use synapse_services::geo_ip;
