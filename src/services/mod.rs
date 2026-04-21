#![allow(ambiguous_glob_reexports)]

pub mod container;
pub use crate::auth::*;
pub use crate::cache::*;
pub use crate::common::*;
pub use crate::storage::PresenceStorage;
pub use crate::storage::*;
pub use container::ServiceContainer;

// =============================================================================
// Core Matrix services (always compiled)
// =============================================================================
pub mod admin_audit_service;
pub mod admin_registration_service;
pub mod application_service;
pub mod auth;
pub mod background_update_service;
pub mod builtin_oidc_provider;
pub mod cache;
pub mod captcha_service;
pub mod content_scanner;
pub mod database_initializer;
pub mod dehydrated_device_service;
pub mod e2ee;
pub mod event_report_service;
pub mod feature_flag_service;
pub mod federation_blacklist_service;
pub mod geo_ip;
pub mod identity;
pub mod key_rotation_service;
pub mod media;
pub mod media_quota_service;
pub mod media_service;
pub mod message_queue;
pub mod moderation_service;
pub mod module_service;
pub mod oidc_service;
pub mod push;
pub mod push_notification_service;
pub mod push_service;
pub mod read_receipt_service;
pub mod refresh_token_service;
pub mod registration_service;
pub mod registration_token_service;
pub mod relations_service;
pub mod retention_service;
pub mod room_service;
pub mod room_summary_service;
pub mod search_service;
pub mod sliding_sync_service;
pub mod space_service;
pub mod sync_service;
pub mod telemetry_alert_service;
pub mod telemetry_service;
pub mod thread_service;
pub mod url_preview_service;

pub mod directory_service;
pub mod dm_service;
pub mod typing_service;

pub use admin_audit_service::*;
pub use admin_registration_service::*;
pub use application_service::*;
pub use builtin_oidc_provider::{
    AuthSession, BuiltinOidcProvider, RefreshToken as BuiltinRefreshToken,
};
pub use database_initializer::*;
pub use dehydrated_device_service::*;
pub use feature_flag_service::*;
pub use media_service::*;
pub use moderation_service::*;
pub use oidc_service::OidcService;
pub use push_service::*;
pub use read_receipt_service::*;
pub use registration_service::*;
pub use room_service::*;
pub use room_summary_service::*;
pub use search_service::*;
pub use sliding_sync_service::*;
pub use space_service::*;
pub use sync_service::*;
pub use telemetry_alert_service::*;
pub use url_preview_service::*;
pub use directory_service::*;
pub use dm_service::*;
pub use typing_service::*;

// =============================================================================
// Feature-gated extension services
// =============================================================================
#[cfg(feature = "openclaw-routes")]
pub mod matrix_ai_connection_service;
#[cfg(feature = "openclaw-routes")]
pub mod mcp_proxy;

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

#[cfg(feature = "voip-tracking")]
pub mod call_service;
#[cfg(feature = "voip-tracking")]
pub mod livekit_client;
#[cfg(feature = "voip-tracking")]
pub mod matrixrtc_service;
#[cfg(feature = "voip-tracking")]
pub use livekit_client::*;
#[cfg(feature = "voip-tracking")]
pub use matrixrtc_service::*;
// VoipService provides TURN server config (standard Matrix) — always available
pub mod voip_service;
pub use voip_service::*;

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

#[cfg(feature = "external-services")]
pub mod webhook_notification;
