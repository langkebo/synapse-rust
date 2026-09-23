//! Extensions — feature-gated and cross-cutting domain services.
//!
//! ARCH-07/08 (2026-08-10): The `user_service` field was removed because it
//! duplicated `ServiceContainer.account.user_service` and was never accessed
//! via the container. The `user_service` in [`ExtensionServicesDeps`] is
//! still required for constructing sub-services (e.g. `friend_room_service`,
//! `server_notification_service`).
//!
//! Several feature-gated `*_storage` and service fields below are not
//! accessed via the container after construction but are retained:
//! - `friend_storage`, `server_notification_storage`, `widget_storage` —
//!   backing storage for the corresponding `*_service`.
//! - `privacy_storage` — consumed during `AccountIdentityService`
//!   construction in `container.rs`; the stored copy is not re-read.
//!
//! Removed in 审查 #23 (YAGNI cleanup): `friend_federation` and
//! `user_lock_service` (constructed but never wired to a route handler) and
//! `ai_connection_storage` (duplicated `AppState`'s own independent copy).

use std::sync::Arc;

#[cfg(feature = "burn-after-read")]
use crate::burn_after_read_service::BurnAfterReadService;
use synapse_storage::UserStore;

use crate::account::UserService;
use crate::container::SharedInfra;

/// The `ExtensionServices` struct.
#[derive(Clone)]
pub struct ExtensionServices {
    #[cfg(feature = "voice-extended")]
    /// The `voice_service` field.
    pub voice_service: crate::voice_service::VoiceService,
    #[cfg(feature = "friends")]
    /// The `friend_storage` field.
    pub friend_storage: Arc<synapse_storage::friend_room::FriendRoomStorage>,
    #[cfg(feature = "friends")]
    /// The `friend_room_service` field.
    pub friend_room_service: Arc<crate::friend_room_service::FriendRoomService>,
    /// The `rtc_domain_service` field.
    pub rtc_domain_service: Arc<crate::rtc::RtcDomainService>,
    /// The `media_domain_service` field.
    pub media_domain_service: Arc<crate::media::MediaDomainService>,
    #[cfg(feature = "server-notifications")]
    /// The `server_notification_storage` field.
    pub server_notification_storage: Arc<dyn synapse_storage::server_notification::ServerNotificationStoreApi>,
    #[cfg(feature = "server-notifications")]
    /// The `server_notification_service` field.
    pub server_notification_service: Arc<crate::server_notification_service::ServerNotificationService>,
    #[cfg(feature = "privacy-ext")]
    /// The `privacy_storage` field.
    pub privacy_storage: Arc<synapse_storage::privacy::PrivacyStorage>,
    #[cfg(feature = "widgets")]
    /// The `widget_storage` field.
    pub widget_storage: Arc<dyn synapse_storage::widget::WidgetStoreApi>,
    #[cfg(feature = "widgets")]
    /// The `widget_service` field.
    pub widget_service: Arc<crate::widget_service::WidgetService>,
    #[cfg(feature = "burn-after-read")]
    /// The `burn_after_read` field.
    pub burn_after_read: Arc<BurnAfterReadService>,
    /// The `identity_service` field.
    pub identity_service: Arc<crate::identity::IdentityService>,
    /// The `translation_service` field.
    pub translation_service: Arc<crate::translation_service::TranslationService>,
    /// The `uia_service` field.
    pub uia_service: Arc<crate::uia_service::UiaService>,
}

/// Dependency bundle for [`ExtensionServices::new`].
pub struct ExtensionServicesDeps<'a> {
    /// The `infra` field.
    pub infra: &'a SharedInfra,
    /// The `rooms` field.
    pub rooms: &'a super::RoomSyncServices,
    /// The `user_storage` field.
    pub user_storage: &'a Arc<dyn UserStore>,
    /// The `threepid_storage` field.
    pub threepid_storage: Arc<dyn synapse_storage::ThreepidStoreApi>,
    /// The `presence_storage` field.
    pub presence_storage: &'a Arc<dyn synapse_storage::presence::PresenceStoreApi>,
    /// The `federation` field.
    pub federation: &'a super::FederationServices,
    /// The `media_service` field.
    pub media_service: &'a crate::media_service::MediaService,
    /// The `media_domain_service` field.
    pub media_domain_service: &'a Arc<crate::media::MediaDomainService>,
    /// The `ui_auth_session_timeout` field.
    pub ui_auth_session_timeout: i64,
    /// The `user_service` field.
    pub user_service: Arc<UserService>,
}

impl ExtensionServices {
    /// See [`new`].
    pub async fn new(deps: ExtensionServicesDeps<'_>) -> Self {
        let ExtensionServicesDeps {
            infra,
            rooms,
            user_storage,
            threepid_storage: _,
            presence_storage,
            federation,
            media_service,
            media_domain_service,
            ui_auth_session_timeout,
            #[allow(unused_variables)]
            user_service,
        } = deps;

        #[cfg(feature = "friends")]
        let friend_storage: Arc<synapse_storage::friend_room::FriendRoomStorage> =
            Arc::new(synapse_storage::FriendRoomStorage::new(infra.pool.clone()));
        #[cfg(feature = "friends")]
        let account_data_storage = Arc::new(synapse_storage::account_data::AccountDataStorage::new(&infra.pool));
        #[cfg(feature = "friends")]
        let friend_room_service = Arc::new(crate::friend_room_service::FriendRoomService::new(
            friend_storage.clone(),
            rooms.room_service.clone(),
            user_storage.clone(),
            presence_storage.clone(),
            account_data_storage,
            infra.cache.clone(),
            infra.config.server.name.clone(),
            Arc::new(federation.key_rotation_manager.clone()),
        ));
        // Suppress unused-variable warnings when `friends` feature is disabled:
        // rooms/presence_storage/federation/user_storage are only consumed by the block above.
        #[cfg(not(feature = "friends"))]
        let _ = (rooms, presence_storage, federation, user_storage);

        #[cfg(feature = "voip-tracking")]
        let call_session_storage: Arc<synapse_storage::call_session::CallSessionStorage> =
            Arc::new(synapse_storage::call_session::CallSessionStorage::new(infra.pool.clone()));
        #[cfg(feature = "voip-tracking")]
        let matrixrtc_storage = synapse_storage::matrixrtc::MatrixRTCStorage::new(infra.pool.clone());

        #[cfg(feature = "voice-extended")]
        let voice_storage = synapse_storage::voice::VoiceStorage::new(infra.pool.clone());
        #[cfg(feature = "voice-extended")]
        let voice_service =
            crate::voice_service::VoiceService::new(media_service.clone(), voice_storage, &infra.config.server.name);
        #[cfg(not(feature = "voice-extended"))]
        let _ = media_service;

        let rtc_infra = Arc::new(crate::rtc::RtcInfraService::new(Arc::new(infra.config.voip.clone())));
        #[cfg(feature = "voip-tracking")]
        let rtc_call = Arc::new(crate::rtc::CallOrchestrationService::new(call_session_storage));
        #[cfg(feature = "voip-tracking")]
        let rtc_session = Arc::new(crate::rtc::RtcSessionService::new(matrixrtc_storage, infra.cache.clone()));
        let rtc_domain_service = Arc::new(crate::rtc::RtcDomainService::new(
            rtc_infra,
            #[cfg(feature = "voip-tracking")]
            rtc_call,
            #[cfg(feature = "voip-tracking")]
            rtc_session,
        ));

        #[cfg(feature = "server-notifications")]
        let server_notification_storage: Arc<
            dyn synapse_storage::server_notification::ServerNotificationStoreApi,
        > = Arc::new(synapse_storage::server_notification::ServerNotificationStorage::new(&infra.pool));
        #[cfg(feature = "server-notifications")]
        let server_notification_service = Arc::new(crate::server_notification_service::ServerNotificationService::new(
            server_notification_storage.clone(),
            user_service.clone(),
        ));

        #[cfg(feature = "privacy-ext")]
        let privacy_storage: Arc<synapse_storage::privacy::PrivacyStorage> =
            Arc::new(synapse_storage::privacy::PrivacyStorage::new(infra.pool.clone()));

        #[cfg(feature = "widgets")]
        let widget_storage: Arc<dyn synapse_storage::widget::WidgetStoreApi> =
            Arc::new(synapse_storage::widget::WidgetStorage::new(infra.pool.clone()));
        #[cfg(feature = "widgets")]
        let widget_service = Arc::new(crate::widget_service::WidgetService::new(widget_storage.clone()));

        #[cfg(feature = "burn-after-read")]
        let burn_after_read = {
            let burn_storage = Arc::new(synapse_storage::burn_after_read::BurnAfterReadStorage::new(&infra.pool));
            Arc::new(BurnAfterReadService::new(
                burn_storage,
                rooms.event_writer.clone(),
                infra.config.server.name.clone(),
            ))
        };

        let identity_storage = crate::identity::IdentityStorage::new(&infra.pool);
        let identity_service = Arc::new(crate::identity::IdentityService::new(
            identity_storage,
            infra.config.identity.trusted_servers.clone(),
        ));

        let translation_service =
            Arc::new(crate::translation_service::TranslationService::new(infra.config.translate.clone()));
        if infra.config.translate.is_configured() {
            ::tracing::info!(
                translation_configured = true,
                provider = %infra.config.translate.provider,
                "Translation service enabled"
            );
        } else {
            ::tracing::info!(
                translation_configured = false,
                mode = %"passthrough",
                "Translation service disabled"
            );
        }

        let uia_service = Arc::new(crate::uia_service::UiaService::new(infra.cache.clone(), ui_auth_session_timeout));

        Self {
            #[cfg(feature = "voice-extended")]
            voice_service,
            #[cfg(feature = "friends")]
            friend_storage,
            #[cfg(feature = "friends")]
            friend_room_service,
            rtc_domain_service,
            media_domain_service: media_domain_service.clone(),
            #[cfg(feature = "server-notifications")]
            server_notification_storage,
            #[cfg(feature = "server-notifications")]
            server_notification_service,
            #[cfg(feature = "privacy-ext")]
            privacy_storage,
            #[cfg(feature = "widgets")]
            widget_storage,
            #[cfg(feature = "widgets")]
            widget_service,
            #[cfg(feature = "burn-after-read")]
            burn_after_read,
            identity_service,
            translation_service,
            uia_service,
        }
    }
}
