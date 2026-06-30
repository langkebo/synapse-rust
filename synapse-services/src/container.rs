use crate::auth::*;
use synapse_cache::*;
use synapse_common::config::Config;
use synapse_common::metrics::MetricsCollector;

use crate::worker::topology_validator::{
    current_instance_worker_type, global_maintenance_owner, should_run_global_maintenance,
};

#[cfg(feature = "burn-after-read")]
use crate::burn_after_read_service::BurnAfterReadService;
use std::sync::Arc;
use synapse_common::server_metrics::ServerMetrics;
use synapse_common::task_queue::RedisTaskQueue;
use synapse_e2ee::backup::KeyBackupService;
use synapse_e2ee::cross_signing::CrossSigningService;
use synapse_e2ee::device_keys::DeviceKeyService;
use synapse_e2ee::device_keys::DeviceKeyStorage;
use synapse_e2ee::key_request::KeyRequestService;
use synapse_e2ee::key_rotation::KeyRotationStorage;
use synapse_e2ee::megolm::MegolmProvider;
use synapse_e2ee::ssss::SecretStorageService;
use synapse_e2ee::to_device::ToDeviceService;
use synapse_e2ee::verification::VerificationService;
#[cfg(feature = "friends")]
use synapse_federation::FriendFederation;
use synapse_federation::{DeviceSyncManager, EventAuthChain, FederationClient, KeyRotationManager};
use synapse_storage::email_verification::EmailVerificationStorage;
pub use synapse_storage::PresenceRepository;
use synapse_storage::*;

#[derive(Clone)]
pub struct ServiceContainer {
    // Domain assemblies
    pub e2ee: E2eeServices,
    pub rooms: RoomSyncServices,
    pub federation: FederationServices,
    pub admin: AdminServices,

    // Cross-cutting service groups
    pub core: CoreServices,
    pub account: AccountServices,
    pub sso: SsoServices,
    pub extensions: ExtensionServices,
}

// =============================================================================
// Core — infra, auth, media, config
// =============================================================================

#[derive(Clone)]
pub struct CoreServices {
    pub auth_service: Arc<dyn Auth>,
    pub registration_service: Arc<crate::registration_service::RegistrationService>,
    pub search_service: Arc<crate::search_service::SearchService>,
    pub media_service: crate::media_service::MediaService,
    pub cache: Arc<CacheManager>,
    pub task_queue: Option<Arc<RedisTaskQueue>>,
    pub metrics: Arc<MetricsCollector>,
    pub server_metrics: Arc<synapse_common::server_metrics::ServerMetrics>,
    pub server_name: String,
    pub config: Config,
    pub key_rotation_storage: KeyRotationStorage,
    pub event_broadcaster: Arc<synapse_federation::event_broadcaster::EventBroadcaster>,
    pub event_notifier: crate::event_notifier::EventNotifier,
    pub account_data_service: Arc<crate::account_data_service::AccountDataService>,
    pub client_push_service: Arc<crate::client_push_service::ClientPushService>,
}

impl CoreServices {
    #[allow(clippy::too_many_arguments)]
    pub async fn new(
        pool: &Arc<sqlx::PgPool>,
        cache: &Arc<CacheManager>,
        config: &Config,
        task_queue: &Option<Arc<RedisTaskQueue>>,
        metrics: &Arc<MetricsCollector>,
        auth_service: &Arc<dyn Auth>,
        user_storage: &Arc<dyn UserStore>,
        rooms: &RoomSyncServices,
        federation: &FederationServices,
        server_metrics: &Arc<ServerMetrics>,
    ) -> Self {
        // Search service
        let search_service = Arc::new(crate::search_service::SearchService::with_postgres(
            &config.search.elasticsearch_url,
            config.search.enabled,
            &config.search.search_index_name,
            Some(pool.as_ref().clone()),
            config.search.provider.clone(),
        ));
        if config.search.provider == "postgres" && config.search.enabled {
            let search_service_clone = search_service.clone();
            tokio::spawn(async move {
                if let Err(e) = search_service_clone.create_fts_index().await {
                    ::tracing::warn!(
                        error = %e,
                        search_provider = %"postgres",
                        search_enabled = true,
                        "Failed to create FTS index"
                    );
                }
            });
        }

        // Media service
        // P2-12/P2-13: Media path is sourced solely from Config (env override
        // via SYNAPSE__SERVER__MEDIA_PATH).
        let media_path = config.server.media_path.clone();
        let media_service = crate::media_service::MediaService::with_pool(
            media_path.as_str(),
            task_queue.clone(),
            &config.server.name,
            Some(pool.clone()),
        );

        // Registration service
        let registration_service = Arc::new(crate::registration_service::RegistrationService::new(
            user_storage.clone(),
            auth_service.clone(),
            metrics.clone(),
            &config.server.name,
            config.server.enable_registration,
            task_queue.clone(),
        ));

        // Event broadcaster (federation)
        let broadcaster_server_name = config.server.get_server_name().to_string();
        let broadcaster_federation_client = federation.federation_client.clone();
        let broadcaster_member_storage = rooms.member_storage.clone();
        let broadcaster_origin = config.server.get_server_name().to_string();
        let broadcaster_batch_size = config.federation.event_broadcast_batch_size;
        let event_broadcaster = {
            let broadcaster = synapse_federation::event_broadcaster::EventBroadcaster::new(broadcaster_server_name)
                .with_client(broadcaster_federation_client)
                .with_pool(pool.as_ref().clone())
                .with_membership_storage(broadcaster_member_storage);
            broadcaster.start_batch_sender(broadcaster_origin, broadcaster_batch_size, 100).await;
            Arc::new(broadcaster)
        };

        // Account data service
        let room_account_data_storage = RoomAccountDataStorage::new(pool);
        let account_data_service = Arc::new(crate::account_data_service::AccountDataService::new(
            pool,
            user_storage.clone(),
            rooms.room_storage.clone(),
            room_account_data_storage,
            FilterStorage::new(pool),
            OpenIdTokenStorage::new(pool),
        ));

        // Client push service
        let client_push_service = Arc::new(crate::client_push_service::ClientPushService::new(pool.clone()));

        Self {
            auth_service: auth_service.clone(),
            registration_service,
            search_service,
            media_service,
            cache: cache.clone(),
            task_queue: task_queue.clone(),
            metrics: metrics.clone(),
            server_metrics: server_metrics.clone(),
            server_name: config.server.name.clone(),
            config: config.clone(),
            key_rotation_storage: KeyRotationStorage::new(pool.clone()),
            event_broadcaster,
            event_notifier: crate::event_notifier::EventNotifier::new(),
            account_data_service,
            client_push_service,
        }
    }
}

// =============================================================================
// Account — user identity, devices, tokens, presence
// =============================================================================

#[derive(Clone)]
pub struct AccountServices {
    pub account_device_list_service: Arc<crate::account_device_list_service::AccountDeviceListService>,
    pub account_identity_service: Arc<crate::account_identity_service::AccountIdentityService>,
    pub user_storage: Arc<dyn UserStore>,
    pub threepid_storage: ThreepidStorage,
    pub device_storage: Arc<dyn DeviceRepository>,
    pub token_storage: AccessTokenStorage,
    pub presence_storage: Arc<dyn PresenceRepository>,
    pub presence_service: Arc<crate::presence_service::PresenceService>,
    pub qr_login_storage: QrLoginStorage,
    pub invite_blocklist_storage: InviteBlocklistStorage,
    pub sticky_event_storage: StickyEventStorage,
}

impl AccountServices {
    pub fn new(
        pool: &Arc<sqlx::PgPool>,
        user_storage: Arc<dyn UserStore>,
        device_storage: &Arc<dyn DeviceRepository>,
        threepid_storage: ThreepidStorage,
        presence_storage: Arc<dyn PresenceRepository>,
        presence_service: &Arc<crate::presence_service::PresenceService>,
        qr_login_storage: QrLoginStorage,
        invite_blocklist_storage: InviteBlocklistStorage,
        sticky_event_storage: StickyEventStorage,
        account_device_list_service: Arc<crate::account_device_list_service::AccountDeviceListService>,
        account_identity_service: Arc<crate::account_identity_service::AccountIdentityService>,
    ) -> Self {
        Self {
            account_device_list_service,
            account_identity_service,
            user_storage,
            threepid_storage,
            device_storage: device_storage.clone(),
            token_storage: AccessTokenStorage::new(pool),
            presence_storage,
            presence_service: presence_service.clone(),
            qr_login_storage,
            invite_blocklist_storage,
            sticky_event_storage,
        }
    }
}

// =============================================================================
// SSO — SAML, CAS, OIDC
// =============================================================================

#[derive(Clone)]
pub struct SsoServices {
    #[cfg(feature = "saml-sso")]
    pub saml_storage: synapse_storage::saml::SamlStorage,
    #[cfg(feature = "saml-sso")]
    pub saml_service: Arc<crate::saml_service::SamlService>,
    #[cfg(feature = "cas-sso")]
    pub cas_storage: synapse_storage::cas::CasStorage,
    #[cfg(feature = "cas-sso")]
    pub cas_service: Arc<crate::cas_service::CasService>,
    pub oidc_service: Option<Arc<crate::oidc_service::OidcService>>,
    pub oidc_mapping_storage: synapse_storage::oidc_user_mapping::OidcUserMappingStorage,
    #[cfg(feature = "builtin-oidc")]
    pub builtin_oidc_provider: Option<Arc<crate::builtin_oidc_provider::BuiltinOidcProvider>>,
    #[cfg(not(feature = "builtin-oidc"))]
    pub builtin_oidc_provider: Option<()>,
}

impl SsoServices {
    pub async fn new(pool: &Arc<sqlx::PgPool>, config: &Config) -> Self {
        #[cfg(feature = "saml-sso")]
        let saml_storage = synapse_storage::saml::SamlStorage::new(pool);
        #[cfg(feature = "saml-sso")]
        let saml_service = Arc::new(crate::saml_service::SamlService::new(
            Arc::new(config.saml.clone()),
            Arc::new(saml_storage.clone()),
            config.server.name.clone(),
        ));

        #[cfg(feature = "cas-sso")]
        let cas_storage = synapse_storage::cas::CasStorage::new(pool);
        #[cfg(feature = "cas-sso")]
        let cas_service =
            Arc::new(crate::cas_service::CasService::new(Arc::new(cas_storage.clone()), config.server.name.clone()));

        // OIDC services (runtime-config-driven, not feature-gated)
        let oidc_service = if config.oidc.is_enabled() {
            Some(Arc::new(crate::oidc_service::OidcService::new(Arc::new(config.oidc.clone()))))
        } else {
            None
        };

        #[cfg(feature = "builtin-oidc")]
        let builtin_oidc_provider = if config.builtin_oidc.is_enabled() {
            match crate::builtin_oidc_provider::BuiltinOidcProvider::new(Arc::new(config.builtin_oidc.clone())) {
                Ok(p) => Some(Arc::new(p)),
                Err(e) => {
                    ::tracing::error!(
                        error = %e,
                        builtin_oidc_enabled = true,
                        issuer = %config.builtin_oidc.issuer,
                        "Failed to initialize BuiltinOidcProvider, disabling"
                    );
                    None
                }
            }
        } else {
            None
        };
        #[cfg(not(feature = "builtin-oidc"))]
        let builtin_oidc_provider: Option<()> = None;

        // OIDC dual-mode startup check
        #[cfg(feature = "builtin-oidc")]
        {
            let external_enabled = oidc_service.is_some();
            let builtin_enabled = builtin_oidc_provider.is_some();
            if external_enabled && builtin_enabled {
                ::tracing::warn!(
                    "Both external OIDC (oidc.issuer) and builtin OIDC provider are enabled. \
                     Builtin OIDC is intended for development/testing only. \
                     In production, use an external IdP and disable builtin OIDC."
                );
            }
        }
        #[cfg(not(feature = "builtin-oidc"))]
        {
            if oidc_service.is_some() {
                ::tracing::info!(
                    external_oidc_enabled = true,
                    builtin_oidc_compiled = false,
                    "External OIDC provider enabled"
                );
            }
        }

        // OIDC mapping storage
        let oidc_mapping_storage = synapse_storage::oidc_user_mapping::OidcUserMappingStorage::new(pool.clone());

        Self {
            #[cfg(feature = "saml-sso")]
            saml_storage,
            #[cfg(feature = "saml-sso")]
            saml_service,
            #[cfg(feature = "cas-sso")]
            cas_storage,
            #[cfg(feature = "cas-sso")]
            cas_service,
            oidc_service,
            builtin_oidc_provider,
            oidc_mapping_storage,
        }
    }
}

// =============================================================================
// Extensions — feature-gated and cross-cutting domain services
// =============================================================================

#[derive(Clone)]
pub struct ExtensionServices {
    #[cfg(feature = "voice-extended")]
    pub voice_service: crate::voice_service::VoiceService,
    #[cfg(feature = "friends")]
    pub friend_storage: FriendRoomStorage,
    #[cfg(feature = "friends")]
    pub friend_room_service: Arc<crate::friend_room_service::FriendRoomService>,
    #[cfg(feature = "friends")]
    pub friend_federation: Arc<FriendFederation>,
    pub rtc_domain_service: Arc<crate::rtc::RtcDomainService>,
    pub directory_service: Arc<crate::directory_service::DirectoryService>,
    pub media_domain_service: Arc<crate::media::MediaDomainService>,
    #[cfg(feature = "openclaw-routes")]
    pub ai_connection_storage: synapse_storage::ai_connection::AiConnectionStorage,
    #[cfg(feature = "server-notifications")]
    pub server_notification_storage: synapse_storage::server_notification::ServerNotificationStorage,
    #[cfg(feature = "server-notifications")]
    pub server_notification_service: Arc<crate::server_notification_service::ServerNotificationService>,
    #[cfg(feature = "privacy-ext")]
    pub privacy_storage: synapse_storage::privacy::PrivacyStorage,
    #[cfg(feature = "widgets")]
    pub widget_storage: synapse_storage::widget::WidgetStorage,
    #[cfg(feature = "widgets")]
    pub widget_service: Arc<crate::widget_service::WidgetService>,
    #[cfg(feature = "burn-after-read")]
    pub burn_after_read: Arc<BurnAfterReadService>,
    pub identity_service: Arc<crate::identity::IdentityService>,
    pub translation_service: Arc<crate::translation_service::TranslationService>,
    pub uia_service: Arc<crate::uia_service::UiaService>,
    pub user_lock_service: Arc<crate::user_lock_service::UserLockService>,
}

impl ExtensionServices {
    #[allow(clippy::too_many_arguments)]
    #[allow(unused_variables)]
    pub async fn new(
        pool: &Arc<sqlx::PgPool>,
        cache: &Arc<CacheManager>,
        config: &Config,
        rooms: &RoomSyncServices,
        user_storage: &Arc<dyn UserStore>,
        _threepid_storage: &ThreepidStorage,
        presence_storage: &Arc<dyn PresenceRepository>,
        federation: &FederationServices,
        media_service: &crate::media_service::MediaService,
        media_domain_service: &Arc<crate::media::MediaDomainService>,
        ui_auth_session_timeout: i64,
    ) -> Self {
        // Friends (feature-gated)
        #[cfg(feature = "friends")]
        let friend_storage = FriendRoomStorage::new(pool.clone());
        #[cfg(feature = "friends")]
        let account_data_storage: Arc<dyn synapse_storage::AccountDataRepository> =
            Arc::new(synapse_storage::account_data::AccountDataStorage::new(pool));
        #[cfg(feature = "friends")]
        let friend_room_service = Arc::new(crate::friend_room_service::FriendRoomService::new(
            friend_storage.clone(),
            rooms.room_service.clone(),
            user_storage.clone(),
            presence_storage.clone(),
            account_data_storage,
            cache.clone(),
            config.server.name.clone(),
            Arc::new(federation.key_rotation_manager.clone()),
        ));
        #[cfg(feature = "friends")]
        let friend_federation = Arc::new(FriendFederation::new(
            friend_room_service.clone() as Arc<dyn synapse_common::traits::FriendRoomProvider>
        ));

        // VoIP tracking (feature-gated)
        #[cfg(feature = "voip-tracking")]
        let call_session_storage = synapse_storage::call_session::CallSessionStorage::new(pool.clone());
        #[cfg(feature = "voip-tracking")]
        let matrixrtc_storage = synapse_storage::matrixrtc::MatrixRTCStorage::new(pool.clone());

        // Voice service (feature-gated, depends on media_service)
        #[cfg(feature = "voice-extended")]
        let voice_storage = synapse_storage::voice::VoiceStorage::new(pool.clone());
        #[cfg(feature = "voice-extended")]
        let voice_service =
            crate::voice_service::VoiceService::new(media_service.clone(), voice_storage, &config.server.name);
        #[cfg(not(feature = "voice-extended"))]
        let _ = media_service;

        // RTC domain service — unified real-time communication
        let rtc_infra = Arc::new(crate::rtc::RtcInfraService::new(Arc::new(config.voip.clone())));
        #[cfg(feature = "voip-tracking")]
        let rtc_call = Arc::new(crate::rtc::CallOrchestrationService::new(Arc::new(call_session_storage)));
        #[cfg(feature = "voip-tracking")]
        let rtc_session = Arc::new(crate::rtc::RtcSessionService::new(matrixrtc_storage, cache.clone()));
        #[cfg(feature = "voip-tracking")]
        let rtc_sfu = Arc::new(crate::rtc::LivekitClient::new(config.livekit.clone()));
        let rtc_domain_service = Arc::new(crate::rtc::RtcDomainService::new(
            rtc_infra,
            #[cfg(feature = "voip-tracking")]
            rtc_call,
            #[cfg(feature = "voip-tracking")]
            rtc_session,
            #[cfg(feature = "voip-tracking")]
            rtc_sfu,
        ));

        // Openclaw (feature-gated)
        #[cfg(feature = "openclaw-routes")]
        let ai_connection_storage = synapse_storage::ai_connection::AiConnectionStorage::new(pool.clone());

        // Server notifications (feature-gated)
        #[cfg(feature = "server-notifications")]
        let server_notification_storage = synapse_storage::server_notification::ServerNotificationStorage::new(pool);
        #[cfg(feature = "server-notifications")]
        let server_notification_service = Arc::new(crate::server_notification_service::ServerNotificationService::new(
            Arc::new(server_notification_storage.clone()),
            user_storage.clone(),
        ));

        // Privacy (feature-gated)
        #[cfg(feature = "privacy-ext")]
        let privacy_storage = synapse_storage::privacy::PrivacyStorage::new(pool.clone());

        // Widgets (feature-gated)
        #[cfg(feature = "widgets")]
        let widget_storage = synapse_storage::widget::WidgetStorage::new(pool.clone());
        #[cfg(feature = "widgets")]
        let widget_service = Arc::new(crate::widget_service::WidgetService::new(Arc::new(widget_storage.clone())));

        // Burn-after-read (feature-gated)
        #[cfg(feature = "burn-after-read")]
        let burn_after_read = {
            let burn_storage = synapse_storage::burn_after_read::BurnAfterReadStorage::new(pool);
            Arc::new(BurnAfterReadService::new(burn_storage, rooms.event_storage.clone(), config.server.name.clone()))
        };

        // Identity service
        let identity_storage = crate::identity::IdentityStorage::new(pool);
        let identity_service =
            Arc::new(crate::identity::IdentityService::new(identity_storage, config.identity.trusted_servers.clone()));

        // Translation service
        let translation_service =
            Arc::new(crate::translation_service::TranslationService::new(config.translate.clone()));
        if config.translate.is_configured() {
            ::tracing::info!(
                translation_configured = true,
                provider = %config.translate.provider,
                "Translation service enabled"
            );
        } else {
            ::tracing::info!(
                translation_configured = false,
                mode = %"passthrough",
                "Translation service disabled"
            );
        }

        // Directory service
        let directory_service = Arc::new(crate::directory_service::DirectoryService::new());

        // UIA service
        let uia_service = Arc::new(crate::uia_service::UiaService::new(cache.clone(), ui_auth_session_timeout));

        // User lock service
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
        }
    }
}

// =============================================================================
// E2EE assembly — device keys, cross-signing, megolm, backup, verification
// =============================================================================

#[derive(Clone)]
pub struct E2eeServices {
    pub device_keys_service: DeviceKeyService,
    pub key_request_service: KeyRequestService,
    pub megolm_service: MegolmProvider,
    pub cross_signing_service: CrossSigningService,
    pub ssss_service: SecretStorageService,
    pub backup_service: KeyBackupService,
    pub dehydrated_device_service: crate::dehydrated_device_service::DehydratedDeviceService,
    pub secure_backup_service: synapse_e2ee::secure_backup::SecureBackupService,
    pub to_device_service: ToDeviceService,
    pub verification_service: VerificationService,
    pub device_trust_service: synapse_e2ee::device_trust::DeviceTrustService,
    pub to_device_storage: synapse_e2ee::to_device::ToDeviceStorage,
}

impl E2eeServices {
    pub async fn new(
        pool: &Arc<sqlx::PgPool>,
        cache: &Arc<CacheManager>,
        user_storage: &Arc<dyn UserStore>,
        megolm_encryption_key_path: Option<&str>,
    ) -> Self {
        let device_key_storage = synapse_e2ee::device_keys::DeviceKeyStorage::new(pool);
        let device_key_storage_for_cs = Arc::new(device_key_storage.clone());
        let backup_device_key_storage = device_key_storage.clone();
        let cross_signing_storage = synapse_e2ee::cross_signing::CrossSigningStorage::new(pool);
        let cross_signing_storage_arc = Arc::new(cross_signing_storage.clone());
        let dehydrated_device_storage = synapse_storage::DehydratedDeviceStorage::new(pool);

        let device_keys_service = DeviceKeyService::new(device_key_storage, cache.clone())
            .with_cross_signing_storage(cross_signing_storage_arc)
            .with_dehydrated_device_storage(dehydrated_device_storage.clone());

        let megolm_storage = synapse_e2ee::megolm::MegolmSessionStorage::new(pool);
        let encryption_key = generate_encryption_key(megolm_encryption_key_path);
        let megolm_service = MegolmProvider::from_env(megolm_storage, cache.clone(), encryption_key);

        let key_request_storage = synapse_e2ee::key_request::KeyRequestStorage::new(pool.as_ref());
        let key_request_service = KeyRequestService::new(key_request_storage, megolm_service.clone());

        let dehydrated_device_service =
            crate::dehydrated_device_service::DehydratedDeviceService::new(dehydrated_device_storage);

        let dehydrated_device_provider: Arc<dyn synapse_common::traits::DehydratedDeviceProvider> =
            Arc::new(dehydrated_device_service.clone());

        let cross_signing_service = CrossSigningService::new(cross_signing_storage)
            .with_device_keys_storage(device_key_storage_for_cs)
            .with_dehydrated_device_service(dehydrated_device_provider.clone());

        let ssss_storage = synapse_e2ee::ssss::SecretStorage::new(pool);
        let ssss_service = synapse_e2ee::ssss::SecretStorageService::new(ssss_storage)
            .with_dehydrated_device_service(dehydrated_device_provider);

        let key_backup_storage = synapse_e2ee::backup::KeyBackupStorage::new(pool);
        let backup_service =
            KeyBackupService::new(&key_backup_storage).with_device_key_storage(backup_device_key_storage);

        let secure_backup_service = synapse_e2ee::secure_backup::SecureBackupService::new(pool);

        let to_device_storage = synapse_e2ee::to_device::ToDeviceStorage::new(pool);
        let to_device_service = ToDeviceService::new(to_device_storage.clone()).with_user_storage(user_storage.clone());

        let verification_storage = synapse_e2ee::verification::VerificationStorage::new(pool);
        let verification_service = VerificationService::new(std::sync::Arc::new(verification_storage));

        let device_trust_storage = synapse_e2ee::device_trust::DeviceTrustStorage::new(pool);
        let device_trust_service = synapse_e2ee::device_trust::DeviceTrustService::new(
            std::sync::Arc::new(device_trust_storage),
            std::sync::Arc::new(verification_service.clone()),
            std::sync::Arc::new(cross_signing_service.clone()),
            std::sync::Arc::new(device_keys_service.clone()),
        );

        Self {
            device_keys_service,
            key_request_service,
            megolm_service,
            cross_signing_service,
            ssss_service,
            backup_service,
            dehydrated_device_service,
            secure_backup_service,
            to_device_service,
            verification_service,
            device_trust_service,
            to_device_storage,
        }
    }
}

// =============================================================================
// Room & Sync assembly — room, member, event, summary, space, sync, sliding_sync
// =============================================================================

#[derive(Clone)]
pub struct RoomSyncServices {
    pub room_storage: Arc<dyn RoomRepository>,
    pub member_storage: Arc<dyn RoomMemberRepository>,
    pub event_storage: Arc<dyn EventRepository>,
    pub room_summary_storage: synapse_storage::room_summary::RoomSummaryStorage,
    pub relations_storage: Arc<dyn synapse_storage::RelationsRepository>,
    pub room_summary_service: Arc<crate::room_summary_service::RoomSummaryService>,
    #[cfg(feature = "beacons")]
    pub beacon_service: Arc<crate::beacon_service::BeaconService>,
    pub room_service: Arc<crate::room_service::RoomService>,
    pub sync_service: Arc<crate::sync_service::SyncService>,
    pub sliding_sync_service: Arc<crate::sliding_sync_service::SlidingSyncService>,
    pub typing_service: Arc<crate::typing_service::TypingService>,
    pub space_storage: SpaceStorage,
    pub space_service: Arc<crate::space_service::SpaceService>,
    pub relations_service: Arc<crate::relations_service::RelationsService>,
    pub thread_storage: synapse_storage::thread::ThreadStorage,
    pub thread_service: Arc<crate::thread_service::ThreadService>,
    pub room_tag_storage: Arc<dyn RoomTagRepository>,
}

impl RoomSyncServices {
    #[allow(clippy::too_many_arguments)]
    pub async fn new(
        pool: &Arc<sqlx::PgPool>,
        cache: &Arc<CacheManager>,
        config: &Config,
        task_queue: &Option<Arc<RedisTaskQueue>>,
        auth_service: &Arc<dyn Auth>,
        presence_storage: &Arc<dyn PresenceRepository>,
        to_device_storage: &synapse_e2ee::to_device::ToDeviceStorage,
        metrics: &Arc<MetricsCollector>,
    ) -> Self {
        let server_name_for_storage = config.server.get_server_name().to_string();
        let member_storage: Arc<dyn RoomMemberRepository> =
            Arc::new(RoomMemberStorage::new(pool, &server_name_for_storage));
        let room_storage_concrete = Arc::new(RoomStorage::new(pool));
        let room_storage: Arc<dyn RoomRepository> = room_storage_concrete.clone();
        let event_storage_concrete = Arc::new(EventStorage::new(pool, server_name_for_storage));
        let event_storage: Arc<dyn EventRepository> = event_storage_concrete.clone();
        let device_storage: Arc<dyn DeviceRepository> = Arc::new(DeviceStorage::new(pool));
        let relations_storage: Arc<dyn synapse_storage::RelationsRepository> =
            Arc::new(synapse_storage::relations::RelationsStorage::new(pool));
        let room_summary_storage = synapse_storage::room_summary::RoomSummaryStorage::new(pool);
        let room_tag_storage: Arc<dyn RoomTagRepository> =
            Arc::new(synapse_storage::room_tag::RoomTagStorage::new(pool.clone()));

        let room_summary_service = Arc::new(crate::room_summary_service::RoomSummaryService::new(
            Arc::new(room_summary_storage.clone()),
            event_storage.clone(),
            Some(member_storage.clone()),
        ));

        #[cfg(feature = "beacons")]
        let beacon_service = Arc::new(crate::beacon_service::BeaconService::new(pool.clone(), cache.clone()));

        let room_service = Arc::new(crate::room_service::RoomService::new(crate::room_service::RoomServiceConfig {
            room_storage: room_storage.clone(),
            member_storage: member_storage.clone(),
            event_storage: event_storage.clone(),
            room_tag_storage: room_tag_storage.clone(),
            user_storage: Arc::new(UserStorage::new(pool, cache.clone())),
            auth_service: auth_service.clone(),
            room_summary_service: room_summary_service.clone(),
            validator: auth_service.validator().clone(),
            server_name: config.server.name.clone(),
            task_queue: task_queue.clone(),
            relations_storage: relations_storage.clone(),
            event_broadcaster: None,
            app_service_manager: None,
            key_rotation_manager: None,
            federation_client: None,
            #[cfg(feature = "beacons")]
            beacon_service: Some(beacon_service.clone()),
            #[cfg(not(feature = "beacons"))]
            beacon_service: None,
        }));

        let sync_room_account_data_storage = RoomAccountDataStorage::new(pool);
        let sync_account_data_storage = synapse_storage::account_data::AccountDataStorage::new(pool);
        let sync_device_key_storage = DeviceKeyStorage::new(pool);
        let sync_key_rotation_storage = KeyRotationStorage::new(pool.clone());
        let sync_service =
            Arc::new(crate::sync_service::SyncService::from_deps(crate::sync_service::SyncServiceDeps {
                presence_storage: presence_storage.clone(),
                member_storage: member_storage.clone(),
                event_storage: event_storage.clone(),
                room_storage: room_storage.clone(),
                room_account_data_storage: sync_room_account_data_storage,
                account_data_storage: sync_account_data_storage,
                filter_storage: FilterStorage::new(pool),
                device_storage: device_storage.clone(),
                device_key_storage: sync_device_key_storage.clone(),
                key_rotation_storage: sync_key_rotation_storage,
                to_device_storage: to_device_storage.clone(),
                metrics: metrics.clone(),
                performance: config.performance.clone(),
            }));

        let typing_service = Arc::new(crate::typing_service::TypingService::new(cache.clone()));

        let sliding_sync_storage = synapse_storage::sliding_sync::SlidingSyncStorage::new(pool.clone());
        let sliding_sync_service = Arc::new(crate::sliding_sync_service::SlidingSyncService::new(
            sliding_sync_storage,
            cache.clone(),
            event_storage.clone(),
            sync_device_key_storage,
            typing_service.clone(),
            presence_storage.clone(),
            member_storage.clone(),
            device_storage.clone(),
            to_device_storage.clone(),
            metrics.clone(),
            config.performance.clone(),
        ));

        let space_storage = SpaceStorage::new(pool);
        let space_service = Arc::new(crate::space_service::SpaceService::new(
            Arc::new(space_storage.clone()),
            room_storage.clone(),
            config.server.name.clone(),
        ));

        let relations_service = Arc::new(crate::relations_service::RelationsService::new(
            relations_storage.clone(),
            config.server.server_name.clone().unwrap_or_default(),
        ));

        let thread_storage = synapse_storage::thread::ThreadStorage::new(pool);
        let thread_service = Arc::new(crate::thread_service::ThreadService::new(Arc::new(thread_storage.clone())));

        Self {
            room_storage,
            member_storage,
            event_storage,
            room_summary_storage,
            relations_storage,
            room_summary_service,
            #[cfg(feature = "beacons")]
            beacon_service,
            room_service,
            sync_service,
            sliding_sync_service,
            typing_service,
            space_storage,
            space_service,
            relations_service,
            thread_storage,
            thread_service,
            room_tag_storage,
        }
    }
}

// =============================================================================
// Federation assembly — key rotation, federation client, device sync
// =============================================================================

#[derive(Clone)]
pub struct FederationServices {
    pub event_auth_chain: EventAuthChain,
    pub key_rotation_manager: KeyRotationManager,
    pub key_rotation_service: Arc<crate::federation_key_rotation_service::FederationKeyRotationService>,
    pub federation_client: Arc<FederationClient>,
    pub device_sync_manager: DeviceSyncManager,
    pub federation_server_name: String,
}

impl FederationServices {
    pub async fn new(
        pool: &Arc<sqlx::PgPool>,
        cache: &Arc<CacheManager>,
        config: &Config,
        task_queue: &Option<Arc<RedisTaskQueue>>,
    ) -> Self {
        let event_auth_chain = EventAuthChain::new();
        let server_name = if config.federation.server_name.is_empty() {
            config.server.name.clone()
        } else {
            config.federation.server_name.clone()
        };

        let key_rotation_manager = KeyRotationManager::with_key_path_and_master_key(
            pool,
            &server_name,
            config.server.signing_key_path.clone(),
            config.federation.signing_key_master_key.as_ref().map(|k| k.as_bytes().to_vec()),
        );
        let key_rotation_service = Arc::new(crate::federation_key_rotation_service::FederationKeyRotationService::new(
            Arc::new(key_rotation_manager.clone()),
            Arc::new(KeyRotationStorage::new(pool.clone())),
        ));

        let federation_client =
            Arc::new(FederationClient::new(server_name.clone(), Arc::new(key_rotation_manager.clone())));

        let device_sync_manager = DeviceSyncManager::new(pool, Some(cache.clone()), task_queue.clone());

        Self {
            event_auth_chain,
            key_rotation_manager,
            key_rotation_service,
            federation_client,
            device_sync_manager,
            federation_server_name: server_name,
        }
    }
}

#[cfg(feature = "burn-after-read")]
fn burn_after_read_processor_enabled(config_enabled: bool) -> bool {
    // P2-13: Config is the sole source of truth; env::var fallback removed.
    config_enabled
}

// =============================================================================
// Admin assembly — decomposed into 5 domain sub-structs
// =============================================================================

#[derive(Clone)]
pub struct AdminUserServices {
    pub admin_registration_service: crate::admin_registration_service::AdminRegistrationService,
    pub admin_user_service: Arc<crate::admin_user_service::AdminUserService>,
    pub email_verification_storage: EmailVerificationStorage,
    pub admin_token_service: Arc<crate::admin_token_service::AdminTokenService>,
    pub refresh_token_storage: Arc<dyn synapse_storage::RefreshTokenRepository>,
    pub refresh_token_service: Arc<crate::refresh_token_service::RefreshTokenService>,
    pub registration_token_storage: synapse_storage::registration_token::RegistrationTokenStorage,
    pub registration_token_service: Arc<crate::registration_token_service::RegistrationTokenService>,
}

#[derive(Clone)]
pub struct AdminFederationServices {
    pub admin_federation_service: Arc<crate::admin_federation_service::AdminFederationService>,
    pub federation_blacklist_storage: synapse_storage::federation_blacklist::FederationBlacklistStorage,
    pub federation_blacklist_service: Arc<crate::federation_blacklist_service::FederationBlacklistService>,
}

#[derive(Clone)]
pub struct AdminMediaServices {
    pub admin_media_service: Arc<crate::admin_media_service::AdminMediaService>,
    pub media_quota_storage: synapse_storage::media_quota::MediaQuotaStorage,
    pub media_quota_service: Arc<crate::media_quota_service::MediaQuotaService>,
}

#[derive(Clone)]
pub struct AdminSecurityServices {
    pub admin_security_service: Arc<crate::admin_security_service::AdminSecurityService>,
    pub captcha_storage: synapse_storage::captcha::CaptchaStorage,
    pub captcha_service: Arc<crate::captcha_service::CaptchaService>,
    pub audit_storage: synapse_storage::audit::AuditEventStorage,
    pub admin_audit_service: Arc<crate::admin_audit_service::AdminAuditService>,
    pub admin_server_service: Arc<crate::admin_server_service::AdminServerService>,
    pub telemetry_alert_service: Arc<crate::telemetry_service::TelemetryAlertService>,
}

#[derive(Clone)]
pub struct AdminModuleServices {
    pub feature_flag_storage: synapse_storage::feature_flags::FeatureFlagStorage,
    pub feature_flag_service: Arc<crate::feature_flag_service::FeatureFlagService>,
    pub event_report_storage: synapse_storage::event_report::EventReportStorage,
    pub event_report_service: Arc<crate::event_report_service::EventReportService>,
    pub background_update_storage: synapse_storage::background_update::BackgroundUpdateStorage,
    pub background_update_service: Arc<crate::background_update_service::BackgroundUpdateService>,
    pub module_storage: synapse_storage::module::ModuleStorage,
    pub module_service: Arc<crate::module_service::ModuleService>,
    pub account_validity_service: Arc<crate::module_service::AccountValidityService>,
    pub retention_storage: synapse_storage::retention::RetentionStorage,
    pub retention_service: Arc<crate::retention_service::RetentionService>,
    pub push_notification_storage: synapse_storage::push_notification::PushNotificationStorage,
    pub push_notification_service: Arc<crate::push_notification_service::PushNotificationService>,
    pub app_service_storage: ApplicationServiceStorage,
    pub app_service_manager: Arc<crate::application_service::ApplicationServiceManager>,
    pub app_service_scheduler: Arc<crate::application_service::ApplicationServiceScheduler>,
    #[cfg(feature = "external-services")]
    pub external_service_integration: Arc<crate::external_service_integration::ExternalServiceIntegration>,
    pub rendezvous_storage: synapse_storage::rendezvous::RendezvousStorage,
    pub rendezvous_message_storage: synapse_storage::rendezvous::RendezvousMessageStorage,
    pub worker_storage: crate::worker::WorkerStorage,
    pub worker_manager: Arc<crate::worker::WorkerManager>,
}

/// Aggregate admin services, decomposed into 5 domain sub-structs.
#[derive(Clone)]
pub struct AdminServices {
    pub user: AdminUserServices,
    pub federation: AdminFederationServices,
    pub media: AdminMediaServices,
    pub security: AdminSecurityServices,
    pub modules: AdminModuleServices,
}

impl AdminServices {
    pub async fn new(
        pool: &Arc<sqlx::PgPool>,
        cache: &Arc<CacheManager>,
        config: &Config,
        task_queue: &Option<Arc<RedisTaskQueue>>,
        metrics: &Arc<MetricsCollector>,
        auth_service: &Arc<dyn Auth>,
        user_storage: &Arc<dyn UserStore>,
    ) -> Self {
        let admin_registration_service = crate::admin_registration_service::AdminRegistrationService::new(
            auth_service.clone(),
            config.admin_registration.clone(),
            user_storage.clone(),
            cache.clone(),
            metrics.clone(),
        );

        let email_verification_storage = EmailVerificationStorage::new(pool);
        let audit_storage = synapse_storage::audit::AuditEventStorage::new(pool);
        let admin_audit_service =
            Arc::new(crate::admin_audit_service::AdminAuditService::new(Arc::new(audit_storage.clone())));

        let feature_flag_storage = synapse_storage::feature_flags::FeatureFlagStorage::new(pool, cache.clone());
        let feature_flag_service = Arc::new(crate::feature_flag_service::FeatureFlagService::new(
            Arc::new(feature_flag_storage.clone()),
            admin_audit_service.clone(),
        ));

        let event_report_storage = synapse_storage::event_report::EventReportStorage::new(pool);
        let event_report_service =
            Arc::new(crate::event_report_service::EventReportService::new(Arc::new(event_report_storage.clone())));

        let background_update_storage = synapse_storage::background_update::BackgroundUpdateStorage::new(pool);
        let background_update_service = Arc::new(
            crate::background_update_service::BackgroundUpdateService::new(Arc::new(background_update_storage.clone()))
                .with_lock_retry_config(config.worker.lock_max_retries, config.worker.lock_max_retry_interval_ms),
        );

        let module_storage = synapse_storage::module::ModuleStorage::new(pool);
        let module_service = Arc::new(crate::module_service::ModuleService::new(Arc::new(module_storage.clone())));
        let account_validity_service =
            Arc::new(crate::module_service::AccountValidityService::new(Arc::new(module_storage.clone())));

        let retention_storage = synapse_storage::retention::RetentionStorage::new(pool);
        let chunked_upload_storage = Arc::new(synapse_storage::media::ChunkedUploadStorage::new(pool));
        let retention_service = Arc::new(crate::retention_service::RetentionService::new(
            Arc::new(retention_storage.clone()),
            chunked_upload_storage.clone(),
            metrics,
            Arc::new(audit_storage.clone()),
        ));

        let refresh_token_storage: Arc<dyn synapse_storage::RefreshTokenRepository> =
            Arc::new(synapse_storage::refresh_token::RefreshTokenStorage::new(pool));
        let refresh_token_service = Arc::new(crate::refresh_token_service::RefreshTokenService::new(
            refresh_token_storage.clone(),
            config.server.refresh_token_ttl_secs.saturating_mul(1000),
        ));

        let registration_token_storage = synapse_storage::registration_token::RegistrationTokenStorage::new(pool);
        let registration_token_service = Arc::new(crate::registration_token_service::RegistrationTokenService::new(
            Arc::new(registration_token_storage.clone()),
        ));

        let captcha_storage = synapse_storage::captcha::CaptchaStorage::new(pool);
        let captcha_service = Arc::new(crate::captcha_service::CaptchaService::with_sms_config(
            Arc::new(captcha_storage.clone()),
            task_queue.clone(),
            config.smtp.enabled,
            &config.sms,
        ));

        let federation_blacklist_storage = synapse_storage::federation_blacklist::FederationBlacklistStorage::new(pool);
        let federation_blacklist_service =
            Arc::new(crate::federation_blacklist_service::FederationBlacklistService::new(Arc::new(
                federation_blacklist_storage.clone(),
            )));
        let admin_federation_storage = synapse_storage::admin_federation::AdminFederationStorage::new(pool);
        let admin_federation_service = Arc::new(crate::admin_federation_service::AdminFederationService::new(
            admin_federation_storage,
            Arc::new(federation_blacklist_storage.clone()),
            federation_blacklist_service.clone(),
        ));

        let push_notification_storage = synapse_storage::push_notification::PushNotificationStorage::new(pool);
        let account_data_storage_for_push: Arc<dyn synapse_storage::AccountDataRepository> =
            Arc::new(synapse_storage::account_data::AccountDataStorage::new(pool));
        let push_notification_service = Arc::new(
            crate::push_notification_service::PushNotificationService::new(Arc::new(push_notification_storage.clone()))
                .with_account_data_storage(account_data_storage_for_push),
        );

        let media_quota_storage = synapse_storage::media_quota::MediaQuotaStorage::new(pool);
        let media_quota_service =
            Arc::new(crate::media_quota_service::MediaQuotaService::new(Arc::new(media_quota_storage.clone())));

        let telemetry_alert_service =
            Arc::new(crate::telemetry_service::TelemetryAlertService::new(pool.clone(), config.database.max_size));

        let rendezvous_storage = synapse_storage::rendezvous::RendezvousStorage::new(pool.clone());
        let rendezvous_message_storage = synapse_storage::rendezvous::RendezvousMessageStorage::new(pool.clone());

        let app_service_storage = ApplicationServiceStorage::new(pool);
        let app_service_event_storage: Arc<EventStorage> =
            Arc::new(EventStorage::new(pool, config.server.get_server_name().to_owned()));
        let app_service_manager = Arc::new(crate::application_service::ApplicationServiceManager::new(
            Arc::new(app_service_storage.clone()),
            app_service_event_storage,
            config.server.get_server_name().to_owned(),
        ));
        let app_service_scheduler =
            Arc::new(crate::application_service::ApplicationServiceScheduler::new(app_service_manager.clone()));
        #[cfg(feature = "external-services")]
        let external_service_integration =
            Arc::new(crate::external_service_integration::ExternalServiceIntegration::new(
                Arc::new(app_service_storage.clone()),
                config.server.get_server_name().to_owned(),
            ));
        let should_start_app_service_scheduler =
            should_run_global_maintenance(&config.worker) && !config.server.app_service_config_files.is_empty();
        if should_start_app_service_scheduler {
            app_service_scheduler.clone().start();
        } else {
            ::tracing::info!(
                worker_type = current_instance_worker_type(&config.worker).as_str(),
                maintenance_owner = global_maintenance_owner(&config.worker).as_str(),
                has_app_service_configs = !config.server.app_service_config_files.is_empty(),
                "Skipping application service scheduler startup on this worker instance"
            );
        }

        let worker_storage = crate::worker::WorkerStorage::new(pool);
        let worker_manager =
            Arc::new(crate::worker::WorkerManager::new(Arc::new(worker_storage.clone()), config.server.name.clone()));

        let admin_media_service =
            Arc::new(crate::admin_media_service::AdminMediaService::new(pool, user_storage.clone()));
        let admin_security_service = Arc::new(crate::admin_security_service::AdminSecurityService::new(
            user_storage.clone(),
            cache.clone(),
            pool,
        ));
        let admin_server_service = Arc::new(crate::admin_server_service::AdminServerService::new(pool.clone()));
        let admin_token_service = Arc::new(crate::admin_token_service::AdminTokenService::new(
            AccessTokenStorage::new(pool),
            refresh_token_storage.clone(),
            registration_token_service.clone(),
        ));
        let admin_user_service = Arc::new(crate::admin_user_service::AdminUserService::new(
            pool.clone(),
            user_storage.clone(),
            DeviceStorage::new(pool),
            RoomStorage::new(pool),
            Arc::new(RoomMemberStorage::new(pool, config.server.get_server_name())),
            config.server.name.clone(),
        ));

        Self {
            user: AdminUserServices {
                admin_registration_service,
                admin_user_service,
                email_verification_storage,
                admin_token_service,
                refresh_token_storage,
                refresh_token_service,
                registration_token_storage,
                registration_token_service,
            },
            federation: AdminFederationServices {
                admin_federation_service,
                federation_blacklist_storage,
                federation_blacklist_service,
            },
            media: AdminMediaServices { admin_media_service, media_quota_storage, media_quota_service },
            security: AdminSecurityServices {
                admin_security_service,
                captcha_storage,
                captcha_service,
                audit_storage,
                admin_audit_service,
                admin_server_service,
                telemetry_alert_service,
            },
            modules: AdminModuleServices {
                feature_flag_storage,
                feature_flag_service,
                event_report_storage,
                event_report_service,
                background_update_storage,
                background_update_service,
                module_storage,
                module_service,
                account_validity_service,
                retention_storage,
                retention_service,
                push_notification_storage,
                push_notification_service,
                app_service_storage,
                app_service_manager,
                app_service_scheduler,
                #[cfg(feature = "external-services")]
                external_service_integration,
                rendezvous_storage,
                rendezvous_message_storage,
                worker_storage,
                worker_manager,
            },
        }
    }
}

// =============================================================================
// ServiceContainer — orchestrated assembly
// =============================================================================

impl ServiceContainer {
    /// Returns a cloned handle to the underlying PostgreSQL connection pool.
    pub fn database_pool(&self) -> Arc<sqlx::PgPool> {
        self.account.user_storage.pool().clone()
    }

    pub async fn new(
        pool: &Arc<sqlx::PgPool>,
        cache: Arc<CacheManager>,
        config: Config,
        task_queue: Option<Arc<RedisTaskQueue>>,
    ) -> Self {
        let ui_auth_session_timeout = config.security.ui_auth_session_timeout;

        // Shared infrastructure — metrics and server_metrics
        let metrics = Arc::new(MetricsCollector::new());
        synapse_common::error::init_error_metrics(metrics.clone());
        let server_metrics = Arc::new(ServerMetrics::new(metrics.clone()));

        // Auth — must be initialized first; downstream services depend on it
        let auth_service: Arc<dyn Auth> = Arc::new(AuthService::new_with_lifetime(
            pool,
            cache.clone(),
            metrics.clone(),
            &config.security,
            &config.server.name,
            config.access_token_lifetime_seconds(),
        ));

        // Core storage
        let user_storage: Arc<dyn UserStore> = Arc::new(UserStorage::new(pool, cache.clone()));
        let device_storage: Arc<dyn DeviceRepository> = Arc::new(DeviceStorage::new(pool));
        let threepid_storage = ThreepidStorage::new(pool);
        let presence_storage: Arc<dyn PresenceRepository> = Arc::new(PresenceStorage::new(pool.clone(), cache.clone()));
        let presence_service = Arc::new(crate::presence_service::PresenceService::new(presence_storage.clone()));
        let qr_login_storage = QrLoginStorage::new(pool.clone());
        let invite_blocklist_storage = InviteBlocklistStorage::new(pool.clone());
        let sticky_event_storage = StickyEventStorage::new(pool.clone());

        // Domain assemblies
        let e2ee =
            E2eeServices::new(pool, &cache, &user_storage, config.server.megolm_encryption_key_path.as_deref()).await;
        let rooms = RoomSyncServices::new(
            pool,
            &cache,
            &config,
            &task_queue,
            &auth_service,
            &presence_storage,
            &e2ee.to_device_storage,
            &metrics,
        )
        .await;
        let admin =
            AdminServices::new(pool, &cache, &config, &task_queue, &metrics, &auth_service, &user_storage).await;
        rooms.room_service.set_app_service_manager(admin.modules.app_service_manager.clone()).await;
        let federation = FederationServices::new(pool, &cache, &config, &task_queue).await;
        let sso = SsoServices::new(pool, &config).await;
        let core = CoreServices::new(
            pool,
            &cache,
            &config,
            &task_queue,
            &metrics,
            &auth_service,
            &user_storage,
            &rooms,
            &federation,
            &server_metrics,
        )
        .await;
        rooms.room_service.set_event_broadcaster(core.event_broadcaster.clone()).await;
        rooms.room_service.set_key_rotation_manager(Arc::new(federation.key_rotation_manager.clone())).await;
        rooms.room_service.set_federation_client(federation.federation_client.clone()).await;
        // Media domain service (needs core.media_service and admin.media_quota_service)
        let chunked_upload_service = Arc::new(crate::media::chunked_upload::ChunkedUploadService::new(pool.clone()));
        let media_domain_service = Arc::new({
            let svc = crate::media::MediaDomainService::new(
                core.media_service.clone(),
                admin.media.media_quota_service.clone(),
                chunked_upload_service.clone(),
            );
            let quarantine_storage = Arc::new(synapse_storage::media::QuarantinedMediaChangeStorage::new(pool));
            let cache_invalidation = cache.invalidation_manager().cloned();
            svc.with_quarantine_stream(quarantine_storage, cache_invalidation)
        });

        // Extensions
        let extensions = ExtensionServices::new(
            pool,
            &cache,
            &config,
            &rooms,
            &user_storage,
            &threepid_storage,
            &presence_storage,
            &federation,
            &core.media_service,
            &media_domain_service,
            ui_auth_session_timeout,
        )
        .await;

        // Account services (needs extensions.privacy_storage for account_identity_service)
        #[cfg(feature = "privacy-ext")]
        let account_identity_service = Arc::new(crate::account_identity_service::AccountIdentityService::new(
            user_storage.clone(),
            threepid_storage.clone(),
            extensions.privacy_storage.clone(),
        ));
        #[cfg(not(feature = "privacy-ext"))]
        let account_identity_service = Arc::new(crate::account_identity_service::AccountIdentityService::new(
            user_storage.clone(),
            threepid_storage.clone(),
        ));
        let account_device_list_service =
            Arc::new(crate::account_device_list_service::AccountDeviceListService::new(DeviceStorage::new(pool)));
        // Worker topology — compute before config is moved into the container
        #[cfg(feature = "burn-after-read")]
        let burn_after_read_processor_cfg = config.server.enable_burn_after_read_processor;
        #[cfg(feature = "burn-after-read")]
        let run_global_maintenance = should_run_global_maintenance(&config.worker);
        #[cfg(feature = "burn-after-read")]
        let current_worker_type = current_instance_worker_type(&config.worker);
        #[cfg(feature = "burn-after-read")]
        let maintenance_owner = global_maintenance_owner(&config.worker);

        let container = Self {
            e2ee,
            rooms,
            federation,
            admin,
            core,
            account: AccountServices::new(
                pool,
                user_storage,
                &device_storage,
                threepid_storage,
                presence_storage,
                &presence_service,
                qr_login_storage,
                invite_blocklist_storage,
                sticky_event_storage,
                account_device_list_service,
                account_identity_service,
            ),
            sso,
            extensions,
        };

        #[cfg(feature = "burn-after-read")]
        if run_global_maintenance && burn_after_read_processor_enabled(burn_after_read_processor_cfg) {
            container.extensions.burn_after_read.recover_pending_burns().await;
            container.extensions.burn_after_read.clone().start_burn_processor().await;
        } else {
            ::tracing::info!(
                worker_type = current_worker_type.as_str(),
                maintenance_owner = maintenance_owner.as_str(),
                processor_enabled = burn_after_read_processor_enabled(burn_after_read_processor_cfg),
                "Skipping burn-after-read processor startup on this worker instance"
            );
        }

        container
    }

    pub fn voip_service(&self) -> &Arc<crate::rtc::RtcInfraService> {
        &self.extensions.rtc_domain_service.infra
    }

    #[cfg(feature = "voip-tracking")]
    pub fn call_service(&self) -> &Arc<crate::rtc::CallOrchestrationService> {
        &self.extensions.rtc_domain_service.call
    }

    #[cfg(any(test, feature = "test-utils"))]
    pub async fn new_test() -> Self {
        let _ = synapse_common::argon2_config::Argon2Config::initialize_global_owasp(
            synapse_common::argon2_config::Argon2Config::default(),
        );
        let pool = crate::test_utils::take_prepared_test_pool().unwrap_or_else(|| {
            let db_url = std::env::var("TEST_DATABASE_URL")
                .or_else(|_| std::env::var("DATABASE_URL"))
                .unwrap_or_else(|_| crate::test_config::test_database_url());
            #[allow(clippy::expect_used)]
            Arc::new(
                sqlx::postgres::PgPoolOptions::new()
                    .max_connections(crate::test_utils::configured_test_pool_max_connections())
                    .min_connections(crate::test_utils::configured_test_pool_min_connections())
                    .acquire_timeout(crate::test_utils::configured_test_pool_acquire_timeout())
                    .idle_timeout(Some(crate::test_utils::configured_test_pool_idle_timeout()))
                    .max_lifetime(Some(crate::test_utils::configured_test_pool_max_lifetime()))
                    .connect_lazy(&db_url)
                    .expect("Failed to create test database pool"),
            )
        });
        Self::new_test_with_pool(pool).await
    }

    #[cfg(any(test, feature = "test-utils"))]
    pub async fn new_test_with_pool(pool: Arc<sqlx::PgPool>) -> Self {
        let cache = Arc::new(CacheManager::new(&CacheConfig::default()));
        let config = crate::test_config::build_test_config();
        Self::new(&pool, cache, config, None).await
    }

    #[cfg(any(test, feature = "test-utils"))]
    pub async fn new_test_with_pool_and_cache(pool: Arc<sqlx::PgPool>, cache: Arc<CacheManager>) -> Self {
        let config = crate::test_config::build_test_config();
        Self::new(&pool, cache, config, None).await
    }
}

fn generate_encryption_key(config_path: Option<&str>) -> [u8; 32] {
    use base64::{engine::general_purpose::STANDARD as B64, Engine as _};

    // P2-13: Config is the sole source of truth; env::var fallback removed.
    // Use SYNAPSE__SERVER__MEGOLM_ENCRYPTION_KEY_PATH to override via standard
    // config env mechanism.
    let path = config_path.map(|p| p.to_string());

    if let Some(ref p) = path {
        let path_buf = std::path::PathBuf::from(p);
        if path_buf.exists() {
            match std::fs::read_to_string(&path_buf) {
                Ok(content) => {
                    let trimmed = content.trim();
                    match B64.decode(trimmed) {
                        Ok(bytes) if bytes.len() == 32 => {
                            let mut key = [0u8; 32];
                            key.copy_from_slice(&bytes);
                            ::tracing::info!(path = %path_buf.display(), "Loaded megolm encryption key");
                            return key;
                        }
                        Ok(bytes) => {
                            ::tracing::error!(
                                "Megolm key at {} has wrong length ({} != 32); refusing to \
                                 overwrite — fix or remove the file",
                                path_buf.display(),
                                bytes.len()
                            );
                        }
                        Err(e) => {
                            ::tracing::error!(
                                "Megolm key at {} is not valid base64: {} — refusing to \
                                 overwrite",
                                path_buf.display(),
                                e
                            );
                        }
                    }
                }
                Err(e) => {
                    ::tracing::error!(
                        "Failed to read megolm key {}: {} — generating ephemeral key",
                        path_buf.display(),
                        e
                    );
                }
            }
        }
    }

    let mut key = [0u8; 32];
    use rand::RngCore;
    rand::rng().fill_bytes(&mut key);

    if let Some(ref p) = path {
        let path_buf = std::path::PathBuf::from(p);
        if !path_buf.exists() {
            if let Some(parent) = path_buf.parent() {
                let _ = std::fs::create_dir_all(parent);
            }
            let encoded = B64.encode(key);
            match std::fs::write(&path_buf, encoded.as_bytes()) {
                Ok(_) => {
                    #[cfg(unix)]
                    {
                        use std::os::unix::fs::PermissionsExt;
                        if let Err(e) = std::fs::set_permissions(&path_buf, std::fs::Permissions::from_mode(0o600)) {
                            ::tracing::warn!(path = %path_buf.display(), error = %e, "Failed to set 0600 permissions on megolm key file");
                        }
                    }
                    ::tracing::info!(path = %path_buf.display(), "Persisted new megolm encryption key");
                }
                Err(e) => {
                    ::tracing::error!(
                        "Failed to persist megolm key to {}: {} — key is ephemeral, \
                         existing encrypted sessions will be lost on restart",
                        path_buf.display(),
                        e
                    );
                }
            }
        }
    } else {
        ::tracing::warn!(
            "server.megolm_encryption_key_path is not configured; megolm encryption key is \
             ephemeral — all encrypted megolm sessions will be unreadable after server \
             restart. Set `server.megolm_encryption_key_path` or \
             `SYNAPSE__SERVER__MEGOLM_ENCRYPTION_KEY_PATH` to a writable file path for \
             production."
        );
    }

    key
}
