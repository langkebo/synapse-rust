use crate::auth::*;
use crate::cache::*;
#[cfg(feature = "voip-tracking")]
use crate::call_service::CallService;
use crate::common::config::{
    AdminRegistrationConfig, Config, CorsConfig, DatabaseConfig, FederationConfig, RateLimitConfig,
    RedisConfig, SearchConfig, SecurityConfig, ServerConfig, SmtpConfig, WorkerConfig,
};
use crate::common::metrics::MetricsCollector;
use crate::common::task_queue::RedisTaskQueue;
use crate::common::*;
use crate::e2ee::backup::KeyBackupService;
use crate::e2ee::cross_signing::CrossSigningService;
use crate::e2ee::device_keys::DeviceKeyService;
use crate::e2ee::key_request::KeyRequestService;
use crate::e2ee::megolm::MegolmService;
use crate::e2ee::to_device::ToDeviceService;
use crate::e2ee::verification::VerificationService;
#[cfg(feature = "friends")]
use crate::federation::FriendFederation;
use crate::federation::{DeviceSyncManager, EventAuthChain, FederationClient, KeyRotationManager};
#[cfg(feature = "burn-after-read")]
use crate::services::burn_after_read_service::BurnAfterReadServiceImpl;
use crate::storage::email_verification::EmailVerificationStorage;
pub use crate::storage::PresenceStorage;
use crate::storage::*;
use std::sync::Arc;
use std::{env, path::Path};

#[derive(Clone)]
pub struct ServiceContainer {
    pub user_storage: UserStorage,
    pub threepid_storage: ThreepidStorage,
    pub device_storage: DeviceStorage,
    pub token_storage: AccessTokenStorage,
    pub room_storage: RoomStorage,
    pub member_storage: RoomMemberStorage,
    pub event_storage: EventStorage,
    pub presence_storage: PresenceStorage,
    pub qr_login_storage: QrLoginStorage,
    pub invite_blocklist_storage: InviteBlocklistStorage,
    pub sticky_event_storage: StickyEventStorage,
    pub presence_service: PresenceStorage,
    pub auth_service: AuthService,
    pub device_keys_service: DeviceKeyService,
    pub key_request_service: KeyRequestService,
    pub megolm_service: MegolmService,
    pub cross_signing_service: CrossSigningService,
    pub backup_service: KeyBackupService,
    pub secure_backup_service: crate::e2ee::secure_backup::SecureBackupService,
    pub to_device_service: ToDeviceService,
    pub verification_service: VerificationService,
    pub device_trust_service: crate::e2ee::device_trust::DeviceTrustService,
    #[cfg(feature = "voice-extended")]
    pub voice_service: crate::services::voice_service::VoiceService,
    pub registration_service: Arc<crate::services::registration_service::RegistrationService>,
    pub room_service: Arc<crate::services::room_service::RoomService>,
    #[cfg(feature = "beacons")]
    pub beacon_service: Arc<crate::services::beacon_service::BeaconService>,
    pub sync_service: Arc<crate::services::sync_service::SyncService>,
    pub sliding_sync_service: Arc<crate::services::sliding_sync_service::SlidingSyncService>,
    pub search_service: Arc<crate::services::search_service::SearchService>,
    pub media_service: crate::services::media_service::MediaService,
    pub cache: Arc<CacheManager>,
    pub task_queue: Option<Arc<RedisTaskQueue>>,
    pub metrics: Arc<MetricsCollector>,
    pub server_name: String,
    pub config: Config,
    pub admin_registration_service:
        crate::services::admin_registration_service::AdminRegistrationService,
    pub email_verification_storage: EmailVerificationStorage,
    pub event_auth_chain: EventAuthChain,
    pub key_rotation_manager: KeyRotationManager,
    pub federation_client: Arc<FederationClient>,
    pub device_sync_manager: DeviceSyncManager,
    #[cfg(feature = "friends")]
    pub friend_storage: FriendRoomStorage,
    #[cfg(feature = "friends")]
    pub friend_room_service: Arc<crate::services::friend_room_service::FriendRoomService>,
    #[cfg(feature = "friends")]
    pub friend_federation: Arc<FriendFederation>,
    #[cfg(feature = "voip-tracking")]
    pub call_service: Arc<CallService>,
    pub directory_service: Arc<crate::services::directory_service::DirectoryServiceImpl>,
    pub dm_service: Arc<crate::services::dm_service::DMServiceImpl>,
    pub typing_service: Arc<crate::services::typing_service::TypingServiceImpl>,
    pub space_storage: SpaceStorage,
    pub space_service: Arc<crate::services::space_service::SpaceService>,
    pub app_service_storage: ApplicationServiceStorage,
    pub app_service_manager: Arc<crate::services::application_service::ApplicationServiceManager>,
    pub worker_storage: crate::worker::WorkerStorage,
    pub worker_manager: Arc<crate::worker::WorkerManager>,
    pub room_summary_storage: crate::storage::room_summary::RoomSummaryStorage,
    pub room_summary_service: Arc<crate::services::room_summary_service::RoomSummaryService>,
    pub retention_storage: crate::storage::retention::RetentionStorage,
    pub retention_service: Arc<crate::services::retention_service::RetentionService>,
    pub refresh_token_storage: crate::storage::refresh_token::RefreshTokenStorage,
    pub refresh_token_service: Arc<crate::services::refresh_token_service::RefreshTokenService>,
    pub registration_token_storage: crate::storage::registration_token::RegistrationTokenStorage,
    pub registration_token_service:
        Arc<crate::services::registration_token_service::RegistrationTokenService>,
    pub audit_storage: crate::storage::audit::AuditEventStorage,
    pub admin_audit_service: Arc<crate::services::admin_audit_service::AdminAuditService>,
    pub feature_flag_storage: crate::storage::feature_flags::FeatureFlagStorage,
    pub feature_flag_service: Arc<crate::services::feature_flag_service::FeatureFlagService>,
    pub event_report_storage: crate::storage::event_report::EventReportStorage,
    pub event_report_service: Arc<crate::services::event_report_service::EventReportService>,
    pub background_update_storage: crate::storage::background_update::BackgroundUpdateStorage,
    pub background_update_service:
        Arc<crate::services::background_update_service::BackgroundUpdateService>,
    pub module_storage: crate::storage::module::ModuleStorage,
    pub module_service: Arc<crate::services::module_service::ModuleService>,
    pub account_validity_service: Arc<crate::services::module_service::AccountValidityService>,
    #[cfg(feature = "saml-sso")]
    pub saml_storage: crate::storage::saml::SamlStorage,
    #[cfg(feature = "saml-sso")]
    pub saml_service: Arc<crate::services::saml_service::SamlService>,
    pub captcha_storage: crate::storage::captcha::CaptchaStorage,
    pub captcha_service: Arc<crate::services::captcha_service::CaptchaService>,
    pub federation_blacklist_storage:
        crate::storage::federation_blacklist::FederationBlacklistStorage,
    pub federation_blacklist_service:
        Arc<crate::services::federation_blacklist_service::FederationBlacklistService>,
    pub push_notification_storage: crate::storage::push_notification::PushNotificationStorage,
    pub push_notification_service:
        Arc<crate::services::push_notification_service::PushNotificationService>,
    pub thread_storage: crate::storage::thread::ThreadStorage,
    pub thread_service: Arc<crate::services::thread_service::ThreadService>,
    pub relations_storage: crate::storage::relations::RelationsStorage,
    pub relations_service: Arc<crate::services::relations_service::RelationsService>,
    #[cfg(feature = "cas-sso")]
    pub cas_storage: crate::storage::cas::CasStorage,
    #[cfg(feature = "cas-sso")]
    pub cas_service: Arc<crate::services::cas_service::CasService>,
    pub media_quota_storage: crate::storage::media_quota::MediaQuotaStorage,
    pub media_quota_service: Arc<crate::services::media_quota_service::MediaQuotaService>,
    #[cfg(feature = "openclaw-routes")]
    pub ai_connection_storage: crate::storage::ai_connection::AiConnectionStorage,
    #[cfg(feature = "server-notifications")]
    pub server_notification_storage: crate::storage::server_notification::ServerNotificationStorage,
    #[cfg(feature = "server-notifications")]
    pub server_notification_service:
        Arc<crate::services::server_notification_service::ServerNotificationService>,
    #[cfg(feature = "privacy-ext")]
    pub privacy_storage: crate::storage::privacy::PrivacyStorage,
    pub rendezvous_storage: crate::storage::rendezvous::RendezvousStorage,
    #[cfg(feature = "widgets")]
    pub widget_storage: crate::storage::widget::WidgetStorage,
    #[cfg(feature = "widgets")]
    pub widget_service: Arc<crate::services::widget_service::WidgetService>,
    pub telemetry_alert_service:
        Arc<crate::services::telemetry_alert_service::TelemetryAlertService>,
    #[cfg(feature = "burn-after-read")]
    pub burn_after_read: Arc<BurnAfterReadServiceImpl>,
    pub oidc_service: Option<Arc<crate::services::oidc_service::OidcService>>,
    pub builtin_oidc_provider:
        Option<Arc<crate::services::builtin_oidc_provider::BuiltinOidcProvider>>,
    pub identity_service: Arc<crate::services::identity::IdentityService>,
}

impl ServiceContainer {
    pub fn new(
        pool: &Arc<sqlx::PgPool>,
        cache: Arc<CacheManager>,
        config: Config,
        task_queue: Option<Arc<RedisTaskQueue>>,
    ) -> Self {
        let media_path = env::var("SYNAPSE_MEDIA_PATH").unwrap_or_else(|_| {
            if Path::new("/app/data/media").exists() {
                "/app/data/media".to_string()
            } else {
                "./data/media".to_string()
            }
        });

        let presence_pool = pool.clone();
        let metrics = Arc::new(MetricsCollector::new());
        let auth_service = AuthService::new(
            pool,
            cache.clone(),
            metrics.clone(),
            &config.security,
            &config.server.name,
        );
        let device_key_storage = crate::e2ee::device_keys::DeviceKeyStorage::new(pool);
        let device_key_storage_for_cs = Arc::new(device_key_storage.clone());
        let cross_signing_storage = crate::e2ee::cross_signing::CrossSigningStorage::new(pool);
        let cross_signing_storage_arc = Arc::new(cross_signing_storage.clone());
        let device_keys_service = DeviceKeyService::new(device_key_storage, cache.clone())
            .with_cross_signing_storage(cross_signing_storage_arc);
        let megolm_storage = crate::e2ee::megolm::MegolmSessionStorage::new(pool);
        let encryption_key = generate_encryption_key();
        let megolm_service = MegolmService::new(megolm_storage, cache.clone(), encryption_key);
        let key_request_storage = crate::e2ee::key_request::KeyRequestStorage::new(pool.as_ref());
        let key_request_service =
            KeyRequestService::new(key_request_storage, megolm_service.clone());
        let cross_signing_service = CrossSigningService::new(cross_signing_storage)
            .with_device_keys_storage(device_key_storage_for_cs);
        let key_backup_storage = crate::e2ee::backup::KeyBackupStorage::new(pool);
        let backup_service = KeyBackupService::new(key_backup_storage);
        let secure_backup_service = crate::e2ee::secure_backup::SecureBackupService::new(pool);
        let to_device_storage = crate::e2ee::to_device::ToDeviceStorage::new(pool);
        let user_storage = UserStorage::new(pool, cache.clone());
        let threepid_storage = ThreepidStorage::new(pool);
        let to_device_service =
            ToDeviceService::new(to_device_storage).with_user_storage(user_storage.clone());
        let verification_storage = crate::e2ee::verification::VerificationStorage::new(pool);
        let verification_service =
            VerificationService::new(std::sync::Arc::new(verification_storage));
        let device_trust_storage = crate::e2ee::device_trust::DeviceTrustStorage::new(pool);
        let device_trust_service = crate::e2ee::device_trust::DeviceTrustService::new(
            std::sync::Arc::new(device_trust_storage),
            std::sync::Arc::new(verification_service.clone()),
            std::sync::Arc::new(cross_signing_service.clone()),
            std::sync::Arc::new(device_keys_service.clone()),
        );
        let presence_service = PresenceStorage::new(presence_pool.clone(), cache.clone());
        let search_service = Arc::new(
            crate::services::search_service::SearchService::with_postgres(
                &config.search.elasticsearch_url,
                config.search.enabled,
                "synapse_messages",
                Some(pool.as_ref().clone()),
                config.search.provider.clone(),
            ),
        );
        if config.search.provider == "postgres" && config.search.enabled {
            let search_service_clone = search_service.clone();
            tokio::spawn(async move {
                if let Err(e) = search_service_clone.create_fts_index().await {
                    ::tracing::warn!("Failed to create FTS index: {}", e);
                }
            });
        }
        let server_name_for_storage = config.server.get_server_name().to_string();
        let member_storage = RoomMemberStorage::new(pool, &server_name_for_storage);
        let room_storage = RoomStorage::new(pool);
        let event_storage = EventStorage::new(pool, server_name_for_storage);
        let room_summary_storage = crate::storage::room_summary::RoomSummaryStorage::new(pool);
        let room_summary_service = Arc::new(
            crate::services::room_summary_service::RoomSummaryService::new(
                Arc::new(room_summary_storage.clone()),
                Arc::new(event_storage.clone()),
                Some(Arc::new(member_storage.clone())),
            ),
        );
        let presence_storage = PresenceStorage::new(presence_pool, cache.clone());
        let qr_login_storage = QrLoginStorage::new(pool.clone());
        let invite_blocklist_storage = InviteBlocklistStorage::new(pool.clone());
        let sticky_event_storage = StickyEventStorage::new(pool.clone());
        let registration_service = Arc::new(
            crate::services::registration_service::RegistrationService::new(
                user_storage.clone(),
                auth_service.clone(),
                metrics.clone(),
                config.server.name.clone(),
                config.server.enable_registration,
                task_queue.clone(),
            ),
        );
        #[cfg(feature = "beacons")]
        let beacon_service = Arc::new(crate::services::beacon_service::BeaconService::new(
            pool.clone(),
            cache.clone(),
        ));
        let room_service = Arc::new(crate::services::room_service::RoomService::new(
            crate::services::room_service::RoomServiceConfig {
                room_storage: room_storage.clone(),
                member_storage: member_storage.clone(),
                event_storage: event_storage.clone(),
                user_storage: user_storage.clone(),
                auth_service: auth_service.clone(),
                room_summary_service: room_summary_service.clone(),
                validator: auth_service.validator.clone(),
                server_name: config.server.name.clone(),
                task_queue: task_queue.clone(),
                #[cfg(feature = "beacons")]
                beacon_service: Some(beacon_service.clone()),
                #[cfg(not(feature = "beacons"))]
                beacon_service: None,
            },
        ));
        let sync_service = Arc::new(crate::services::sync_service::SyncService::from_deps(
            crate::services::sync_service::SyncServiceDeps {
                presence_storage: presence_storage.clone(),
                member_storage: member_storage.clone(),
                event_storage: event_storage.clone(),
                room_storage: room_storage.clone(),
                filter_storage: FilterStorage::new(pool),
                device_storage: DeviceStorage::new(pool),
                metrics: metrics.clone(),
                performance: config.performance.clone(),
            },
        ));
        let typing_service = Arc::new(crate::services::typing_service::TypingServiceImpl::new());
        let sliding_sync_storage =
            crate::storage::sliding_sync::SlidingSyncStorage::new(pool.clone());
        let sliding_sync_service = Arc::new(
            crate::services::sliding_sync_service::SlidingSyncService::new(
                sliding_sync_storage,
                cache.clone(),
                event_storage.clone(),
                typing_service.clone(),
            ),
        );
        let media_service = crate::services::media_service::MediaService::new(
            media_path.as_str(),
            task_queue.clone(),
            &config.server.name,
        );
        #[cfg(feature = "voice-extended")]
        let voice_service = crate::services::voice_service::VoiceService::new(
            media_service.clone(),
            &config.server.name,
        );
        let admin_registration_service =
            crate::services::admin_registration_service::AdminRegistrationService::new(
                auth_service.clone(),
                config.admin_registration.clone(),
                cache.clone(),
                metrics.clone(),
            );
        let email_verification_storage = EmailVerificationStorage::new(pool);
        let event_auth_chain = EventAuthChain::new();
        let server_name = if config.federation.server_name.is_empty() {
            config.server.name.clone()
        } else {
            config.federation.server_name.clone()
        };
        let key_rotation_manager = KeyRotationManager::with_key_path(
            pool,
            &server_name,
            config.server.signing_key_path.clone(),
        );
        let federation_client = Arc::new(FederationClient::new(
            server_name,
            Arc::new(key_rotation_manager.clone()),
        ));
        let device_sync_manager =
            DeviceSyncManager::new(pool, Some(cache.clone()), task_queue.clone());
        #[cfg(feature = "friends")]
        let friend_storage = FriendRoomStorage::new(pool.clone());
        #[cfg(feature = "friends")]
        let friend_room_service = Arc::new(
            crate::services::friend_room_service::FriendRoomService::new(
                friend_storage.clone(),
                room_service.clone(),
                event_storage.clone(),
                config.server.name.clone(),
                Arc::new(key_rotation_manager.clone()),
            ),
        );
        #[cfg(feature = "friends")]
        let friend_federation = Arc::new(FriendFederation::new(friend_room_service.clone()));
        #[cfg(feature = "voip-tracking")]
        let call_session_storage =
            crate::storage::call_session::CallSessionStorage::new(pool.clone());
        #[cfg(feature = "voip-tracking")]
        let call_service = Arc::new(CallService::new(Arc::new(call_session_storage)));
        let directory_service =
            Arc::new(crate::services::directory_service::DirectoryServiceImpl::new());
        let dm_service = Arc::new(crate::services::dm_service::DMServiceImpl::new());
        let space_storage = SpaceStorage::new(pool);
        let space_service = Arc::new(crate::services::space_service::SpaceService::new(
            Arc::new(space_storage.clone()),
            Arc::new(room_storage.clone()),
            config.server.name.clone(),
        ));
        let app_service_storage = ApplicationServiceStorage::new(pool);
        let app_service_manager = Arc::new(
            crate::services::application_service::ApplicationServiceManager::new(
                Arc::new(app_service_storage.clone()),
                config.server.name.clone(),
            ),
        );
        let worker_storage = crate::worker::WorkerStorage::new(pool);
        let worker_manager = Arc::new(crate::worker::WorkerManager::new(
            Arc::new(worker_storage.clone()),
            config.server.name.clone(),
        ));
        let retention_storage = crate::storage::retention::RetentionStorage::new(pool);
        let retention_service =
            Arc::new(crate::services::retention_service::RetentionService::new(
                Arc::new(retention_storage.clone()),
                pool.clone(),
                metrics.clone(),
                Arc::new(crate::storage::audit::AuditEventStorage::new(pool)),
            ));
        let refresh_token_storage = crate::storage::refresh_token::RefreshTokenStorage::new(pool);
        let refresh_token_service = Arc::new(
            crate::services::refresh_token_service::RefreshTokenService::new(
                Arc::new(refresh_token_storage.clone()),
                604800000,
            ),
        );
        let registration_token_storage =
            crate::storage::registration_token::RegistrationTokenStorage::new(pool);
        let registration_token_service = Arc::new(
            crate::services::registration_token_service::RegistrationTokenService::new(Arc::new(
                registration_token_storage.clone(),
            )),
        );
        let audit_storage = crate::storage::audit::AuditEventStorage::new(pool);
        let admin_audit_service = Arc::new(
            crate::services::admin_audit_service::AdminAuditService::new(Arc::new(
                audit_storage.clone(),
            )),
        );
        let feature_flag_storage = crate::storage::feature_flags::FeatureFlagStorage::new(pool);
        let feature_flag_service = Arc::new(
            crate::services::feature_flag_service::FeatureFlagService::new(
                Arc::new(feature_flag_storage.clone()),
                admin_audit_service.clone(),
            ),
        );
        let event_report_storage = crate::storage::event_report::EventReportStorage::new(pool);
        let event_report_service = Arc::new(
            crate::services::event_report_service::EventReportService::new(Arc::new(
                event_report_storage.clone(),
            )),
        );
        let background_update_storage =
            crate::storage::background_update::BackgroundUpdateStorage::new(pool);
        let background_update_service = Arc::new(
            crate::services::background_update_service::BackgroundUpdateService::new(Arc::new(
                background_update_storage.clone(),
            )),
        );
        let module_storage = crate::storage::module::ModuleStorage::new(pool);
        let module_service = Arc::new(crate::services::module_service::ModuleService::new(
            Arc::new(module_storage.clone()),
        ));
        let account_validity_service = Arc::new(
            crate::services::module_service::AccountValidityService::new(Arc::new(
                module_storage.clone(),
            )),
        );
        #[cfg(feature = "saml-sso")]
        let saml_storage = crate::storage::saml::SamlStorage::new(pool);
        #[cfg(feature = "saml-sso")]
        let saml_service = Arc::new(crate::services::saml_service::SamlService::new(
            Arc::new(config.saml.clone()),
            Arc::new(saml_storage.clone()),
            config.server.name.clone(),
        ));
        let captcha_storage = crate::storage::captcha::CaptchaStorage::new(pool);
        let captcha_service = Arc::new(crate::services::captcha_service::CaptchaService::new(
            Arc::new(captcha_storage.clone()),
        ));
        let federation_blacklist_storage =
            crate::storage::federation_blacklist::FederationBlacklistStorage::new(pool);
        let federation_blacklist_service = Arc::new(
            crate::services::federation_blacklist_service::FederationBlacklistService::new(
                Arc::new(federation_blacklist_storage.clone()),
            ),
        );
        let push_notification_storage =
            crate::storage::push_notification::PushNotificationStorage::new(pool);
        let push_notification_service = Arc::new(
            crate::services::push_notification_service::PushNotificationService::new(Arc::new(
                push_notification_storage.clone(),
            )),
        );
        let thread_storage = crate::storage::thread::ThreadStorage::new(pool);
        let thread_service = Arc::new(crate::services::thread_service::ThreadService::new(
            Arc::new(thread_storage.clone()),
        ));
        let relations_storage = crate::storage::relations::RelationsStorage::new(pool);
        let relations_service =
            Arc::new(crate::services::relations_service::RelationsService::new(
                Arc::new(relations_storage.clone()),
            ));
        #[cfg(feature = "cas-sso")]
        let cas_storage = crate::storage::cas::CasStorage::new(pool);
        #[cfg(feature = "cas-sso")]
        let cas_service = Arc::new(crate::services::cas_service::CasService::new(
            Arc::new(cas_storage.clone()),
            config.server.name.clone(),
        ));
        let media_quota_storage = crate::storage::media_quota::MediaQuotaStorage::new(pool);
        let media_quota_service = Arc::new(
            crate::services::media_quota_service::MediaQuotaService::new(Arc::new(
                media_quota_storage.clone(),
            )),
        );
        #[cfg(feature = "openclaw-routes")]
        let ai_connection_storage =
            crate::storage::ai_connection::AiConnectionStorage::new(pool.clone());
        #[cfg(feature = "server-notifications")]
        let server_notification_storage =
            crate::storage::server_notification::ServerNotificationStorage::new(pool);
        #[cfg(feature = "server-notifications")]
        let server_notification_service = Arc::new(
            crate::services::server_notification_service::ServerNotificationService::new(Arc::new(
                server_notification_storage.clone(),
            )),
        );
        #[cfg(feature = "privacy-ext")]
        let privacy_storage = crate::storage::privacy::PrivacyStorage::new(pool.clone());
        let rendezvous_storage = crate::storage::rendezvous::RendezvousStorage::new(pool.clone());
        #[cfg(feature = "widgets")]
        let widget_storage = crate::storage::widget::WidgetStorage::new(pool.clone());
        #[cfg(feature = "widgets")]
        let widget_service = Arc::new(crate::services::widget_service::WidgetService::new(
            Arc::new(widget_storage.clone()),
        ));
        let telemetry_alert_service = Arc::new(
            crate::services::telemetry_alert_service::TelemetryAlertService::new(
                pool.clone(),
                config.database.max_size,
            ),
        );
        #[cfg(feature = "burn-after-read")]
        let burn_after_read = Arc::new(BurnAfterReadServiceImpl::new());
        let oidc_service = if config.oidc.is_enabled() {
            Some(Arc::new(crate::services::oidc_service::OidcService::new(
                Arc::new(config.oidc.clone()),
            )))
        } else {
            None
        };
        let builtin_oidc_provider = if config.builtin_oidc.is_enabled() {
            Some(Arc::new(
                crate::services::builtin_oidc_provider::BuiltinOidcProvider::new(Arc::new(
                    config.builtin_oidc.clone(),
                )),
            ))
        } else {
            None
        };

        let identity_storage = crate::services::identity::IdentityStorage::new(pool);
        let identity_service = Arc::new(crate::services::identity::IdentityService::new(
            identity_storage,
            config.identity.trusted_servers.clone(),
        ));

        Self {
            user_storage,
            threepid_storage,
            device_storage: DeviceStorage::new(pool),
            token_storage: AccessTokenStorage::new(pool),
            room_storage,
            member_storage,
            event_storage,
            presence_storage,
            qr_login_storage,
            invite_blocklist_storage,
            sticky_event_storage,
            presence_service,
            auth_service,
            device_keys_service,
            key_request_service,
            megolm_service,
            cross_signing_service,
            backup_service,
            secure_backup_service,
            to_device_service,
            verification_service,
            device_trust_service,
            #[cfg(feature = "voice-extended")]
            voice_service,
            registration_service,
            room_service,
            #[cfg(feature = "beacons")]
            beacon_service,
            sync_service,
            sliding_sync_service,
            search_service,
            media_service,
            cache,
            task_queue,
            metrics,
            server_name: config.server.name.clone(),
            config,
            admin_registration_service,
            email_verification_storage,
            event_auth_chain,
            key_rotation_manager,
            federation_client,
            device_sync_manager,
            #[cfg(feature = "friends")]
            friend_storage,
            #[cfg(feature = "friends")]
            friend_room_service,
            #[cfg(feature = "friends")]
            friend_federation,
            #[cfg(feature = "voip-tracking")]
            call_service,
            directory_service,
            dm_service,
            typing_service,
            space_storage,
            space_service,
            app_service_storage,
            app_service_manager,
            worker_storage,
            worker_manager,
            room_summary_storage,
            room_summary_service,
            retention_storage,
            retention_service,
            refresh_token_storage,
            refresh_token_service,
            registration_token_storage,
            registration_token_service,
            audit_storage,
            admin_audit_service,
            feature_flag_storage,
            feature_flag_service,
            event_report_storage,
            event_report_service,
            background_update_storage,
            background_update_service,
            module_storage,
            module_service,
            account_validity_service,
            #[cfg(feature = "saml-sso")]
            saml_storage,
            #[cfg(feature = "saml-sso")]
            saml_service,
            captcha_storage,
            captcha_service,
            federation_blacklist_storage,
            federation_blacklist_service,
            push_notification_storage,
            push_notification_service,
            thread_storage,
            thread_service,
            relations_storage,
            relations_service,
            #[cfg(feature = "cas-sso")]
            cas_storage,
            #[cfg(feature = "cas-sso")]
            cas_service,
            media_quota_storage,
            media_quota_service,
            #[cfg(feature = "openclaw-routes")]
            ai_connection_storage,
            #[cfg(feature = "server-notifications")]
            server_notification_storage,
            #[cfg(feature = "server-notifications")]
            server_notification_service,
            #[cfg(feature = "privacy-ext")]
            privacy_storage,
            rendezvous_storage,
            #[cfg(feature = "widgets")]
            widget_storage,
            #[cfg(feature = "widgets")]
            widget_service,
            telemetry_alert_service,
            #[cfg(feature = "burn-after-read")]
            burn_after_read,
            oidc_service,
            builtin_oidc_provider,
            identity_service,
        }
    }

    pub fn new_test() -> Self {
        let _ = crate::common::argon2_config::Argon2Config::initialize_global_owasp(
            crate::common::argon2_config::Argon2Config::default(),
        );
        let pool = crate::test_utils::take_prepared_test_pool().unwrap_or_else(|| {
            let db_url = std::env::var("TEST_DATABASE_URL")
                .or_else(|_| std::env::var("DATABASE_URL"))
                .unwrap_or_else(|| crate::test_config::test_database_url());
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
        Self::new_test_with_pool(pool)
    }

    pub fn new_test_with_pool(pool: Arc<sqlx::PgPool>) -> Self {
        let cache = Arc::new(CacheManager::new(CacheConfig::default()));
        let config = build_test_config();
        Self::new(&pool, cache, config, None)
    }

    pub fn new_test_with_pool_and_cache(pool: Arc<sqlx::PgPool>, cache: Arc<CacheManager>) -> Self {
        let config = build_test_config();
        Self::new(&pool, cache, config, None)
    }
}

fn build_test_config() -> Config {
    let host = std::env::var("DATABASE_HOST").unwrap_or_else(|_| "localhost".to_string());
    let port: u16 = std::env::var("DATABASE_PORT")
        .ok()
        .and_then(|p| p.parse().ok())
        .unwrap_or(5432);
    let user = std::env::var("DATABASE_USER").unwrap_or_else(|_| "synapse".to_string());
    let pass = std::env::var("DATABASE_PASSWORD").unwrap_or_else(|_| "synapse".to_string());
    let name = std::env::var("DATABASE_NAME").unwrap_or_else(|_| "synapse".to_string());
    let test_pool_max_connections = crate::test_utils::configured_test_pool_max_connections();
    let test_pool_min_connections = crate::test_utils::configured_test_pool_min_connections();

    Config {
        server: ServerConfig {
            name: "localhost".to_string(),
            host: "0.0.0.0".to_string(),
            port: 28008,
            public_baseurl: None,
            signing_key_path: None,
            macaroon_secret_key: None,
            form_secret: None,
            server_name: None,
            suppress_key_server_warning: false,
            serve_server_wellknown: false,
            soft_file_limit: 0,
            user_agent_suffix: None,
            web_client_location: None,
            registration_shared_secret: None,
            admin_contact: None,
            max_upload_size: 1000000,
            max_image_resolution: 1000000,
            enable_registration: true,
            enable_registration_captcha: false,
            background_tasks_interval: 60,
            expire_access_token: true,
            expire_access_token_lifetime: 3600,
            refresh_token_lifetime: 604800,
            refresh_token_sliding_window_size: 1000,
            session_duration: 86400,
            warmup_pool: true,
        },
        database: DatabaseConfig {
            host,
            port,
            username: user,
            password: pass,
            name,
            pool_size: test_pool_max_connections,
            max_size: test_pool_max_connections,
            min_idle: Some(test_pool_min_connections),
            connection_timeout: crate::test_utils::configured_test_pool_acquire_timeout().as_secs(),
        },
        redis: RedisConfig {
            host: "localhost".to_string(),
            port: 6379,
            password: None,
            key_prefix: "test:".to_string(),
            pool_size: 10,
            enabled: false,
            connection_timeout_ms: 5000,
            command_timeout_ms: 3000,
            circuit_breaker: crate::common::config::CircuitBreakerConfig::default(),
        },
        logging: crate::common::config::LoggingConfig {
            level: "info".to_string(),
            format: "json".to_string(),
            log_file: None,
            log_dir: None,
        },
        federation: FederationConfig {
            enabled: true,
            allow_ingress: false,
            server_name: "test.example.com".to_string(),
            federation_port: 8448,
            connection_pool_size: 10,
            max_transaction_payload: 50000,
            ca_file: None,
            client_ca_file: None,
            signing_key: None,
            key_id: None,
            trusted_key_servers: vec![],
            key_refresh_interval: 86400,
            suppress_key_server_warning: false,
            signature_cache_ttl: 3600,
            key_cache_ttl: 3600,
            key_rotation_grace_period_ms: 600000,
            key_fetch_max_concurrency: 32,
            key_fetch_timeout_ms: 5000,
            process_inbound_edus: false,
            inbound_edus_max_per_txn: 100,
            inbound_edu_max_concurrency: 8,
            inbound_edu_acquire_timeout_ms: 250,
            inbound_edu_per_origin_max_concurrency: 2,
            process_inbound_presence_edus: false,
            inbound_presence_updates_max_per_txn: 50,
            inbound_presence_backoff_ms: 3000,
            join_max_concurrency: 16,
            join_acquire_timeout_ms: 750,
            admission_mode: false,
        },
        security: SecurityConfig {
            secret: "test_secret".to_string(),
            expiry_time: 3600,
            refresh_token_expiry: 604800,
            argon2_m_cost: 65536,
            argon2_t_cost: 3,
            argon2_p_cost: 1,
            allow_legacy_hashes: false,
            login_failure_lockout_threshold: 5,
            login_lockout_duration_seconds: 900,
            admin_mfa_required: false,
            admin_mfa_shared_secret: String::new(),
            admin_mfa_allowed_drift_steps: 1,
            admin_rbac_enabled: true,
        },
        search: SearchConfig {
            enabled: false,
            elasticsearch_url: "http://localhost:9200".to_string(),
            postgres_fts: PostgresFtsConfig {
                enabled: true,
                weights: Default::default(),
            },
            provider: "postgres".to_string(),
        },
        rate_limit: RateLimitConfig::default(),
        admin_registration: AdminRegistrationConfig {
            enabled: true,
            shared_secret: "test_shared_secret".to_string(),
            nonce_timeout_seconds: 60,
            allow_external_access: false,
            production_only: true,
            ip_whitelist: Vec::new(),
            require_captcha: false,
            require_manual_approval: false,
            approval_tokens: Vec::new(),
        },
        builtin_oidc: crate::common::config::BuiltinOidcConfig::default(),
        worker: WorkerConfig::default(),
        cors: CorsConfig::default(),
        smtp: SmtpConfig::default(),
        voip: crate::common::config::VoipConfig::default(),
        push: crate::common::config::PushConfig::default(),
        url_preview: crate::common::config::UrlPreviewConfig::default(),
        oidc: crate::common::config::OidcConfig::default(),
        saml: crate::common::config::SamlConfig::default(),
        retention: crate::common::config::RetentionConfig::default(),
        telemetry: crate::common::telemetry_config::OpenTelemetryConfig::default(),
        prometheus: crate::common::telemetry_config::PrometheusConfig::default(),
        performance: crate::common::config::PerformanceConfig::default(),
        experimental: crate::common::config::ExperimentalConfig::default(),
        identity: crate::common::config::IdentityConfig::default(),
    }
}

fn generate_encryption_key() -> [u8; 32] {
    let mut key = [0u8; 32];
    for byte in key.iter_mut() {
        *byte = rand::random();
    }
    key
}
