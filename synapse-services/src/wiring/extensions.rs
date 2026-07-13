//! Extensions — feature-gated and cross-cutting domain services.

use std::sync::Arc;

#[cfg(feature = "burn-after-read")]
use crate::burn_after_read_service::BurnAfterReadService;
use synapse_storage::UserStore;

use crate::container::SharedInfra;
use crate::UserService;

#[derive(Clone)]
pub struct ExtensionServices {
    #[cfg(feature = "voice-extended")]
    pub voice_service: crate::voice_service::VoiceService,
    #[cfg(feature = "friends")]
    pub friend_storage: Arc<dyn synapse_storage::friend_room::FriendRoomStoreApi>,
    #[cfg(feature = "friends")]
    pub friend_room_service: Arc<crate::friend_room_service::FriendRoomService>,
    #[cfg(feature = "friends")]
    pub friend_federation: Arc<synapse_federation::FriendFederation>,
    pub rtc_domain_service: Arc<crate::rtc::RtcDomainService>,
    pub directory_service: Arc<crate::directory_service::DirectoryService>,
    pub media_domain_service: Arc<crate::media::MediaDomainService>,
    #[cfg(feature = "openclaw-routes")]
    pub ai_connection_storage: Arc<dyn synapse_storage::ai_connection::AiConnectionStoreApi>,
    #[cfg(feature = "server-notifications")]
    pub server_notification_storage: Arc<dyn synapse_storage::server_notification::ServerNotificationStoreApi>,
    #[cfg(feature = "server-notifications")]
    pub server_notification_service: Arc<crate::server_notification_service::ServerNotificationService>,
    #[cfg(feature = "privacy-ext")]
    pub privacy_storage: Arc<dyn synapse_storage::privacy::PrivacyStoreApi>,
    #[cfg(feature = "widgets")]
    pub widget_storage: Arc<dyn synapse_storage::widget::WidgetStoreApi>,
    #[cfg(feature = "widgets")]
    pub widget_service: Arc<crate::widget_service::WidgetService>,
    #[cfg(feature = "burn-after-read")]
    pub burn_after_read: Arc<BurnAfterReadService>,
    pub identity_service: Arc<crate::identity::IdentityService>,
    pub translation_service: Arc<crate::translation_service::TranslationService>,
    pub uia_service: Arc<crate::uia_service::UiaService>,
    pub user_lock_service: Arc<crate::user_lock_service::UserLockService>,
    pub user_service: Arc<UserService>,
}

/// Dependency bundle for [`ExtensionServices::new`].
pub struct ExtensionServicesDeps<'a> {
    pub infra: &'a SharedInfra,
    pub rooms: &'a super::RoomSyncServices,
    pub user_storage: &'a Arc<dyn UserStore>,
    pub threepid_storage: Arc<dyn synapse_storage::ThreepidStoreApi>,
    pub presence_storage: &'a Arc<dyn synapse_storage::presence::PresenceStoreApi>,
    pub federation: &'a super::FederationServices,
    pub media_service: &'a crate::media_service::MediaService,
    pub media_domain_service: &'a Arc<crate::media::MediaDomainService>,
    pub ui_auth_session_timeout: i64,
    pub user_service: Arc<UserService>,
}

impl ExtensionServices {
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
            user_service,
        } = deps;

        #[cfg(feature = "friends")]
        let friend_storage: Arc<dyn synapse_storage::friend_room::FriendRoomStoreApi> =
            Arc::new(synapse_storage::FriendRoomStorage::new(infra.pool.clone()));
        #[cfg(feature = "friends")]
        let account_data_storage = Arc::new(synapse_storage::account_data::AccountDataStorage::new(&infra.pool));
        #[cfg(feature = "friends")]
        let friend_room_service = Arc::new(crate::friend_room_service::FriendRoomService::new(
            friend_storage.clone(),
            rooms.room_service.clone(),
            user_storage.clone(),
            user_service.clone(),
            presence_storage.clone(),
            account_data_storage,
            infra.cache.clone(),
            infra.config.server.name.clone(),
            Arc::new(federation.key_rotation_manager.clone()),
        ));
        #[cfg(feature = "friends")]
        let friend_federation = Arc::new(synapse_federation::FriendFederation::new(
            friend_room_service.clone() as Arc<dyn synapse_common::traits::FriendRoomProvider>
        ));

        #[cfg(feature = "voip-tracking")]
        let call_session_storage: Arc<dyn synapse_storage::call_session::CallSessionStoreApi> =
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
        #[cfg(feature = "voip-tracking")]
        let rtc_sfu = Arc::new(crate::rtc::LivekitClient::new(infra.config.livekit.clone()));
        let rtc_domain_service = Arc::new(crate::rtc::RtcDomainService::new(
            rtc_infra,
            #[cfg(feature = "voip-tracking")]
            rtc_call,
            #[cfg(feature = "voip-tracking")]
            rtc_session,
            #[cfg(feature = "voip-tracking")]
            rtc_sfu,
        ));

        #[cfg(feature = "openclaw-routes")]
        let ai_connection_storage: Arc<dyn synapse_storage::ai_connection::AiConnectionStoreApi> =
            Arc::new(synapse_storage::ai_connection::AiConnectionStorage::new(infra.pool.clone()));

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
        let privacy_storage: Arc<dyn synapse_storage::privacy::PrivacyStoreApi> =
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

        let directory_service = Arc::new(crate::directory_service::DirectoryService::new());

        let uia_service = Arc::new(crate::uia_service::UiaService::new(infra.cache.clone(), ui_auth_session_timeout));

        let user_lock_service = Arc::new(crate::user_lock_service::UserLockService::new(user_storage.clone()));

        Self {
            #[cfg(feature = "voice-extended")]
            voice_service,
            #[cfg(feature = "friends")]
            friend_storage,
            #[cfg(feature = "friends")]
            friend_room_service,
            #[cfg(feature = "friends")]
            friend_federation,
            rtc_domain_service,
            directory_service,
            media_domain_service: media_domain_service.clone(),
            #[cfg(feature = "openclaw-routes")]
            ai_connection_storage,
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
            user_lock_service,
            user_service,
        }
    }
}
