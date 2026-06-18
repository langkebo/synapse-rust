use crate::auth::*;
use synapse_cache::*;
use synapse_common::config::Config;
#[cfg(any(test, feature = "test-utils"))]
use synapse_common::config::{
    AdminRegistrationConfig, CorsConfig, DatabaseConfig, FederationConfig, RateLimitConfig, RedisConfig, SearchConfig,
    SecurityConfig, ServerConfig, SmtpConfig, WorkerConfig,
};
use synapse_common::metrics::MetricsCollector;

const DEFAULT_REFRESH_TOKEN_TTL_MS: i64 = 7 * 24 * 60 * 60 * 1000;
#[cfg(feature = "burn-after-read")]
use crate::burn_after_read_service::BurnAfterReadService;
use std::sync::Arc;
use std::{env, path::Path};
#[cfg(any(test, feature = "test-utils"))]
use synapse_common::config::PostgresFtsConfig;
use synapse_common::server_metrics::ServerMetrics;
use synapse_common::task_queue::RedisTaskQueue;
use synapse_e2ee::backup::KeyBackupService;
use synapse_e2ee::cross_signing::CrossSigningService;
use synapse_e2ee::device_keys::DeviceKeyService;
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
pub use synapse_storage::PresenceStorage;
use synapse_storage::*;

#[derive(Clone)]
#[allow(private_interfaces)]
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
    pub auth_service: AuthService,
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
}

// =============================================================================
// Account — user identity, devices, tokens, presence
// =============================================================================

#[derive(Clone)]
pub struct AccountServices {
    pub user_storage: UserStorage,
    pub threepid_storage: ThreepidStorage,
    pub device_storage: DeviceStorage,
    pub token_storage: AccessTokenStorage,
    pub presence_storage: PresenceStorage,
    pub qr_login_storage: QrLoginStorage,
    pub invite_blocklist_storage: InviteBlocklistStorage,
    pub sticky_event_storage: StickyEventStorage,
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
    #[cfg(feature = "builtin-oidc")]
    pub builtin_oidc_provider: Option<Arc<crate::builtin_oidc_provider::BuiltinOidcProvider>>,
    #[cfg(not(feature = "builtin-oidc"))]
    pub builtin_oidc_provider: Option<()>,
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

fn assemble_e2ee(pool: &Arc<sqlx::PgPool>, cache: &Arc<CacheManager>, user_storage: &UserStorage) -> E2eeServices {
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
    let encryption_key = generate_encryption_key();
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
    let backup_service = KeyBackupService::new(&key_backup_storage).with_device_key_storage(backup_device_key_storage);

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

    E2eeServices {
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

// =============================================================================
// Room & Sync assembly — room, member, event, summary, space, sync, sliding_sync
// =============================================================================

#[derive(Clone)]
pub struct RoomSyncServices {
    pub room_storage: RoomStorage,
    pub member_storage: RoomMemberStorage,
    pub event_storage: EventStorage,
    pub room_summary_storage: synapse_storage::room_summary::RoomSummaryStorage,
    pub relations_storage: synapse_storage::relations::RelationsStorage,
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
}

#[allow(clippy::too_many_arguments)]
fn assemble_room_and_sync(
    pool: &Arc<sqlx::PgPool>,
    cache: &Arc<CacheManager>,
    config: &Config,
    task_queue: &Option<Arc<RedisTaskQueue>>,
    auth_service: &AuthService,
    presence_storage: &PresenceStorage,
    to_device_storage: &synapse_e2ee::to_device::ToDeviceStorage,
    metrics: &Arc<MetricsCollector>,
) -> RoomSyncServices {
    let server_name_for_storage = config.server.get_server_name().to_string();
    let member_storage = RoomMemberStorage::new(pool, &server_name_for_storage);
    let room_storage = RoomStorage::new(pool);
    let event_storage = EventStorage::new(pool, server_name_for_storage);
    let relations_storage = synapse_storage::relations::RelationsStorage::new(pool);
    let room_summary_storage = synapse_storage::room_summary::RoomSummaryStorage::new(pool);

    let room_summary_service = Arc::new(crate::room_summary_service::RoomSummaryService::new(
        Arc::new(room_summary_storage.clone()),
        Arc::new(event_storage.clone()),
        Some(Arc::new(member_storage.clone())),
    ));

    #[cfg(feature = "beacons")]
    let beacon_service = Arc::new(crate::beacon_service::BeaconService::new(pool.clone(), cache.clone()));

    let room_service = Arc::new(crate::room_service::RoomService::new(crate::room_service::RoomServiceConfig {
        room_storage: room_storage.clone(),
        member_storage: member_storage.clone(),
        event_storage: event_storage.clone(),
        user_storage: UserStorage::new(pool, cache.clone()),
        auth_service: auth_service.clone(),
        room_summary_service: room_summary_service.clone(),
        validator: auth_service.validator.clone(),
        server_name: config.server.name.clone(),
        task_queue: task_queue.clone(),
        relations_storage: relations_storage.clone(),
        event_broadcaster: None,
        app_service_manager: None,
        #[cfg(feature = "beacons")]
        beacon_service: Some(beacon_service.clone()),
        #[cfg(not(feature = "beacons"))]
        beacon_service: None,
    }));

    let sync_service = Arc::new(crate::sync_service::SyncService::from_deps(crate::sync_service::SyncServiceDeps {
        presence_storage: presence_storage.clone(),
        member_storage: member_storage.clone(),
        event_storage: event_storage.clone(),
        room_storage: room_storage.clone(),
        filter_storage: FilterStorage::new(pool),
        device_storage: DeviceStorage::new(pool),
        to_device_storage: to_device_storage.clone(),
        metrics: metrics.clone(),
        performance: config.performance.clone(),
    }));

    let typing_service = Arc::new(crate::typing_service::TypingService::new(cache.clone()));

    let sliding_sync_storage = synapse_storage::sliding_sync::SlidingSyncStorage::new(pool.clone());
    let device_storage = synapse_storage::device::DeviceStorage::new(pool);
    let sliding_sync_service = Arc::new(crate::sliding_sync_service::SlidingSyncService::new(
        sliding_sync_storage,
        cache.clone(),
        event_storage.clone(),
        typing_service.clone(),
        presence_storage.clone(),
        member_storage.clone(),
        device_storage,
        to_device_storage.clone(),
    ));

    let space_storage = SpaceStorage::new(pool);
    let space_service = Arc::new(crate::space_service::SpaceService::new(
        Arc::new(space_storage.clone()),
        Arc::new(room_storage.clone()),
        config.server.name.clone(),
    ));

    let relations_service = Arc::new(crate::relations_service::RelationsService::new(
        Arc::new(relations_storage.clone()),
        config.server.server_name.clone().unwrap_or_default(),
    ));

    let thread_storage = synapse_storage::thread::ThreadStorage::new(pool);
    let thread_service = Arc::new(crate::thread_service::ThreadService::new(Arc::new(thread_storage.clone())));

    RoomSyncServices {
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
    }
}

// =============================================================================
// Federation assembly — key rotation, federation client, device sync
// =============================================================================

#[derive(Clone)]
pub struct FederationServices {
    pub event_auth_chain: EventAuthChain,
    pub key_rotation_manager: KeyRotationManager,
    pub federation_client: Arc<FederationClient>,
    pub device_sync_manager: DeviceSyncManager,
    pub federation_server_name: String,
}

fn assemble_federation(
    pool: &Arc<sqlx::PgPool>,
    cache: &Arc<CacheManager>,
    config: &Config,
    task_queue: &Option<Arc<RedisTaskQueue>>,
) -> FederationServices {
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

    let federation_client =
        Arc::new(FederationClient::new(server_name.clone(), Arc::new(key_rotation_manager.clone())));

    let device_sync_manager = DeviceSyncManager::new(pool, Some(cache.clone()), task_queue.clone());

    FederationServices {
        event_auth_chain,
        key_rotation_manager,
        federation_client,
        device_sync_manager,
        federation_server_name: server_name,
    }
}

#[cfg(feature = "burn-after-read")]
fn burn_after_read_processor_enabled() -> bool {
    !matches!(
        env::var("SYNAPSE_ENABLE_BURN_AFTER_READ_PROCESSOR").ok().as_deref(),
        Some("0" | "false" | "FALSE" | "False" | "off" | "OFF" | "Off")
    )
}

// =============================================================================
// Admin assembly — audit, feature flags, modules, background updates
// =============================================================================

#[derive(Clone)]
pub struct AdminServices {
    pub admin_registration_service: crate::admin_registration_service::AdminRegistrationService,
    pub audit_storage: synapse_storage::audit::AuditEventStorage,
    pub admin_audit_service: Arc<crate::admin_audit_service::AdminAuditService>,
    pub admin_federation_service: Arc<crate::admin_federation_service::AdminFederationService>,
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
    pub refresh_token_storage: synapse_storage::refresh_token::RefreshTokenStorage,
    pub refresh_token_service: Arc<crate::refresh_token_service::RefreshTokenService>,
    pub registration_token_storage: synapse_storage::registration_token::RegistrationTokenStorage,
    pub registration_token_service: Arc<crate::registration_token_service::RegistrationTokenService>,
    pub captcha_storage: synapse_storage::captcha::CaptchaStorage,
    pub captcha_service: Arc<crate::captcha_service::CaptchaService>,
    pub federation_blacklist_storage: synapse_storage::federation_blacklist::FederationBlacklistStorage,
    pub federation_blacklist_service: Arc<crate::federation_blacklist_service::FederationBlacklistService>,
    pub push_notification_storage: synapse_storage::push_notification::PushNotificationStorage,
    pub push_notification_service: Arc<crate::push_notification_service::PushNotificationService>,
    pub media_quota_storage: synapse_storage::media_quota::MediaQuotaStorage,
    pub media_quota_service: Arc<crate::media_quota_service::MediaQuotaService>,
    pub telemetry_alert_service: Arc<crate::telemetry_service::TelemetryAlertService>,
    pub email_verification_storage: EmailVerificationStorage,
    pub rendezvous_storage: synapse_storage::rendezvous::RendezvousStorage,
    pub app_service_storage: ApplicationServiceStorage,
    pub app_service_manager: Arc<crate::application_service::ApplicationServiceManager>,
    pub app_service_scheduler: Arc<crate::application_service::ApplicationServiceScheduler>,
    pub worker_storage: crate::worker::WorkerStorage,
    pub worker_manager: Arc<crate::worker::WorkerManager>,
}

fn assemble_admin_support(
    pool: &Arc<sqlx::PgPool>,
    cache: &Arc<CacheManager>,
    config: &Config,
    task_queue: &Option<Arc<RedisTaskQueue>>,
    metrics: &Arc<MetricsCollector>,
    auth_service: &AuthService,
    user_storage: &UserStorage,
) -> AdminServices {
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
    let background_update_service = Arc::new(crate::background_update_service::BackgroundUpdateService::new(Arc::new(
        background_update_storage.clone(),
    )));

    let module_storage = synapse_storage::module::ModuleStorage::new(pool);
    let module_service = Arc::new(crate::module_service::ModuleService::new(Arc::new(module_storage.clone())));
    let account_validity_service =
        Arc::new(crate::module_service::AccountValidityService::new(Arc::new(module_storage.clone())));

    let retention_storage = synapse_storage::retention::RetentionStorage::new(pool);
    let retention_service = Arc::new(crate::retention_service::RetentionService::new(
        Arc::new(retention_storage.clone()),
        pool.clone(),
        metrics,
        Arc::new(audit_storage.clone()),
    ));

    let refresh_token_storage = synapse_storage::refresh_token::RefreshTokenStorage::new(pool);
    let refresh_token_service = Arc::new(crate::refresh_token_service::RefreshTokenService::new(
        Arc::new(refresh_token_storage.clone()),
        DEFAULT_REFRESH_TOKEN_TTL_MS,
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
    let federation_blacklist_service = Arc::new(crate::federation_blacklist_service::FederationBlacklistService::new(
        Arc::new(federation_blacklist_storage.clone()),
    ));
    let admin_federation_storage = synapse_storage::admin_federation::AdminFederationStorage::new(pool);
    let admin_federation_service = Arc::new(crate::admin_federation_service::AdminFederationService::new(
        admin_federation_storage,
        Arc::new(federation_blacklist_storage.clone()),
        federation_blacklist_service.clone(),
    ));

    let push_notification_storage = synapse_storage::push_notification::PushNotificationStorage::new(pool);
    let push_notification_service = Arc::new(crate::push_notification_service::PushNotificationService::new(Arc::new(
        push_notification_storage.clone(),
    )));

    let media_quota_storage = synapse_storage::media_quota::MediaQuotaStorage::new(pool);
    let media_quota_service =
        Arc::new(crate::media_quota_service::MediaQuotaService::new(Arc::new(media_quota_storage.clone())));

    let telemetry_alert_service =
        Arc::new(crate::telemetry_service::TelemetryAlertService::new(pool.clone(), config.database.max_size));

    let rendezvous_storage = synapse_storage::rendezvous::RendezvousStorage::new(pool.clone());

    let app_service_storage = ApplicationServiceStorage::new(pool);
    let app_service_manager = Arc::new(crate::application_service::ApplicationServiceManager::new(
        Arc::new(app_service_storage.clone()),
        Arc::new(EventStorage::new(pool, config.server.get_server_name().to_owned())),
        config.server.get_server_name().to_owned(),
    ));
    let app_service_scheduler =
        Arc::new(crate::application_service::ApplicationServiceScheduler::new(app_service_manager.clone()));
    if !config.server.app_service_config_files.is_empty() {
        app_service_scheduler.clone().start();
    } else {
        ::tracing::info!(
            has_app_service_configs = false,
            "Skipping application service scheduler startup because no appservice config files are declared"
        );
    }

    let worker_storage = crate::worker::WorkerStorage::new(pool);
    let worker_manager =
        Arc::new(crate::worker::WorkerManager::new(Arc::new(worker_storage.clone()), config.server.name.clone()));

    AdminServices {
        admin_registration_service,
        audit_storage,
        admin_audit_service,
        admin_federation_service,
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
        refresh_token_storage,
        refresh_token_service,
        registration_token_storage,
        registration_token_service,
        captcha_storage,
        captcha_service,
        federation_blacklist_storage,
        federation_blacklist_service,
        push_notification_storage,
        push_notification_service,
        media_quota_storage,
        media_quota_service,
        telemetry_alert_service,
        email_verification_storage,
        rendezvous_storage,
        app_service_storage,
        app_service_manager,
        app_service_scheduler,
        worker_storage,
        worker_manager,
    }
}

// =============================================================================
// ServiceContainer — orchestrated assembly
// =============================================================================

impl ServiceContainer {
    pub async fn new(
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

        let ui_auth_session_timeout = config.security.ui_auth_session_timeout;
        let broadcaster_server_name = config.server.server_name.clone().unwrap_or_else(|| "localhost".to_string());

        // Shared infrastructure — metrics and server_metrics
        let metrics = Arc::new(MetricsCollector::new());
        synapse_common::error::init_error_metrics(metrics.clone());
        let server_metrics = Arc::new(ServerMetrics::new(metrics.clone()));

        // Auth — must be initialized first; downstream services depend on it
        let auth_service = AuthService::new_with_lifetime(
            pool,
            cache.clone(),
            metrics.clone(),
            &config.security,
            &config.server.name,
            config.access_token_lifetime_seconds(),
        );

        // Core storage
        let user_storage = UserStorage::new(pool, cache.clone());
        let threepid_storage = ThreepidStorage::new(pool);
        let presence_storage = PresenceStorage::new(pool.clone(), cache.clone());
        let qr_login_storage = QrLoginStorage::new(pool.clone());
        let invite_blocklist_storage = InviteBlocklistStorage::new(pool.clone());
        let sticky_event_storage = StickyEventStorage::new(pool.clone());

        // E2EE — domain assembly
        let e2ee = assemble_e2ee(pool, &cache, &user_storage);

        // Search service
        let search_service = Arc::new(crate::search_service::SearchService::with_postgres(
            &config.search.elasticsearch_url,
            config.search.enabled,
            "synapse_messages",
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

        // Room & Sync — domain assembly
        let rooms = assemble_room_and_sync(
            pool,
            &cache,
            &config,
            &task_queue,
            &auth_service,
            &presence_storage,
            &e2ee.to_device_storage,
            &metrics,
        );

        // Media service
        let media_service = crate::media_service::MediaService::with_pool(
            media_path.as_str(),
            task_queue.clone(),
            &config.server.name,
            Some(pool.clone()),
        );
        let chunked_upload_service = Arc::new(crate::media::chunked_upload::ChunkedUploadService::new(pool.clone()));

        #[cfg(feature = "voice-extended")]
        let voice_storage = synapse_storage::voice::VoiceStorage::new(pool.clone());

        #[cfg(feature = "voice-extended")]
        let voice_service =
            crate::voice_service::VoiceService::new(media_service.clone(), voice_storage, &config.server.name);

        // Admin & support services — domain assembly
        let admin = assemble_admin_support(pool, &cache, &config, &task_queue, &metrics, &auth_service, &user_storage);
        rooms.room_service.set_app_service_manager(admin.app_service_manager.clone()).await;
        let media_domain_service = {
            let svc = crate::media::MediaDomainService::new(
                media_service.clone(),
                admin.media_quota_service.clone(),
                chunked_upload_service.clone(),
            );
            let quarantine_storage = Arc::new(synapse_storage::media::QuarantinedMediaChangeStorage::new(pool));
            let cache_invalidation = cache.invalidation_manager().cloned();
            svc.with_quarantine_stream(quarantine_storage, cache_invalidation)
        };
        let media_domain_service = Arc::new(media_domain_service);

        // Federation — domain assembly
        let federation = assemble_federation(pool, &cache, &config, &task_queue);

        // Registration service
        let registration_service = Arc::new(crate::registration_service::RegistrationService::new(
            user_storage.clone(),
            auth_service.clone(),
            metrics.clone(),
            &config.server.name,
            config.server.enable_registration,
            task_queue.clone(),
        ));

        // Directory service
        let directory_service = Arc::new(crate::directory_service::DirectoryService::new());

        // =========================================================================
        // Feature-gated extensions (L3 — off by default in core-private-chat)
        // =========================================================================

        #[cfg(feature = "friends")]
        let friend_storage = FriendRoomStorage::new(pool.clone());
        #[cfg(feature = "friends")]
        let account_data_storage = synapse_storage::account_data::AccountDataStorage::new(pool);
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

        #[cfg(feature = "voip-tracking")]
        let call_session_storage = synapse_storage::call_session::CallSessionStorage::new(pool.clone());
        #[cfg(feature = "voip-tracking")]
        let matrixrtc_storage = synapse_storage::matrixrtc::MatrixRTCStorage::new(pool.clone());

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

        #[cfg(feature = "openclaw-routes")]
        let ai_connection_storage = synapse_storage::ai_connection::AiConnectionStorage::new(pool.clone());

        #[cfg(feature = "server-notifications")]
        let server_notification_storage = synapse_storage::server_notification::ServerNotificationStorage::new(pool);
        #[cfg(feature = "server-notifications")]
        let server_notification_service = Arc::new(crate::server_notification_service::ServerNotificationService::new(
            Arc::new(server_notification_storage.clone()),
        ));

        #[cfg(feature = "privacy-ext")]
        let privacy_storage = synapse_storage::privacy::PrivacyStorage::new(pool.clone());

        #[cfg(feature = "widgets")]
        let widget_storage = synapse_storage::widget::WidgetStorage::new(pool.clone());
        #[cfg(feature = "widgets")]
        let widget_service = Arc::new(crate::widget_service::WidgetService::new(Arc::new(widget_storage.clone())));

        #[cfg(feature = "burn-after-read")]
        let burn_after_read = {
            let burn_storage = synapse_storage::burn_after_read::BurnAfterReadStorage::new(pool);
            Arc::new(BurnAfterReadService::new(burn_storage, rooms.event_storage.clone(), config.server.name.clone()))
        };

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

        // Event broadcaster (federation)
        let broadcaster_federation_client = federation.federation_client.clone();
        let broadcaster_member_storage = rooms.member_storage.clone();
        let broadcaster_origin = config.server.get_server_name().to_string();
        let event_broadcaster = {
            let broadcaster = synapse_federation::event_broadcaster::EventBroadcaster::new(broadcaster_server_name)
                .with_client(broadcaster_federation_client)
                .with_pool(pool.as_ref().clone())
                .with_membership_storage(Arc::new(broadcaster_member_storage));
            broadcaster.start_batch_sender(broadcaster_origin, 20, 100).await;
            Arc::new(broadcaster)
        };

        rooms.room_service.set_event_broadcaster(event_broadcaster.clone()).await;

        // User lock service — needs user_storage before it's moved into the container
        let user_lock_service =
            Arc::new(crate::user_lock_service::UserLockService::new(Arc::new(user_storage.clone())));

        let container = Self {
            e2ee,
            rooms,
            federation,
            admin,
            core: CoreServices {
                auth_service,
                registration_service,
                search_service,
                media_service,
                cache: cache.clone(),
                task_queue,
                metrics,
                server_name: config.server.name.clone(),
                config,
                key_rotation_storage: KeyRotationStorage::new(pool.clone()),
                server_metrics,
                event_broadcaster,
                event_notifier: crate::event_notifier::EventNotifier::new(),
            },
            account: AccountServices {
                user_storage,
                threepid_storage,
                device_storage: DeviceStorage::new(pool),
                token_storage: AccessTokenStorage::new(pool),
                presence_storage: presence_storage.clone(),
                qr_login_storage,
                invite_blocklist_storage,
                sticky_event_storage,
            },
            sso: SsoServices {
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
            },
            extensions: ExtensionServices {
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
                media_domain_service,
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
                uia_service: Arc::new(crate::uia_service::UiaService::new(cache.clone(), ui_auth_session_timeout)),
                user_lock_service,
            },
        };

        #[cfg(feature = "burn-after-read")]
        if burn_after_read_processor_enabled() {
            container.extensions.burn_after_read.recover_pending_burns().await;
            container.extensions.burn_after_read.clone().start_burn_processor().await;
        } else {
            ::tracing::info!(
                processor_enabled = false,
                "Skipping burn-after-read processor startup because it is disabled by environment"
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
        let config = build_test_config();
        Self::new(&pool, cache, config, None).await
    }

    #[cfg(any(test, feature = "test-utils"))]
    pub async fn new_test_with_pool_and_cache(pool: Arc<sqlx::PgPool>, cache: Arc<CacheManager>) -> Self {
        let config = build_test_config();
        Self::new(&pool, cache, config, None).await
    }
}

#[cfg(any(test, feature = "test-utils"))]
fn build_test_config() -> Config {
    let host = std::env::var("DATABASE_HOST").unwrap_or_else(|_| "localhost".to_string());
    let port: u16 = std::env::var("DATABASE_PORT").ok().and_then(|p| p.parse().ok()).unwrap_or(5432);
    let user = std::env::var("DATABASE_USER").unwrap_or_else(|_| "synapse".to_string());
    let pass = std::env::var("DATABASE_PASSWORD").unwrap_or_else(|_| "synapse".to_string());
    let name = std::env::var("DATABASE_NAME").unwrap_or_else(|_| "synapse".to_string());
    let test_pool_max_connections = crate::test_utils::configured_test_pool_max_connections();
    let test_pool_min_connections = crate::test_utils::configured_test_pool_min_connections();

    Config {
        server: ServerConfig {
            name: "localhost".to_string(),
            host: "0.0.0.0".to_string(),
            port: 8008,
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
            remote_media_lifetime: 2592000,
            local_media_lifetime: 0,
            enable_registration: true,
            enable_registration_captcha: false,
            background_tasks_interval: 60,
            dehydrated_device_cleanup_interval_secs: 3600,
            expire_access_token: true,
            expire_access_token_lifetime: 3600,
            refresh_token_lifetime: 604800,
            refresh_token_sliding_window_size: 1000,
            session_duration: 86400,
            warmup_pool: true,
            allow_public_rooms_without_auth: false,
            allow_public_rooms_over_federation: true,
            auto_join_rooms: vec![],
            autocreate_auto_join_rooms: true,
            encryption_enabled_by_default_for_room_type: None,
            app_service_config_files: vec![],
            presence_enabled: true,
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
            circuit_breaker: synapse_common::config::CircuitBreakerConfig::default(),
        },
        logging: synapse_common::config::LoggingConfig {
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
            key_rotation_grace_period_ms: 60_0000,
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
            signing_key_master_key: None,
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
            ui_auth_session_timeout: 900,
        },
        search: SearchConfig {
            enabled: false,
            elasticsearch_url: "http://localhost:9200".to_string(),
            postgres_fts: PostgresFtsConfig { enabled: true, weights: Default::default() },
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
        builtin_oidc: synapse_common::config::BuiltinOidcConfig::default(),
        worker: WorkerConfig::default(),
        cors: CorsConfig::default(),
        smtp: SmtpConfig::default(),
        sms: synapse_common::config::SmsConfig::default(),
        voip: synapse_common::config::VoipConfig::default(),
        livekit: synapse_common::config::LivekitConfig::default(),
        push: synapse_common::config::PushConfig::default(),
        url_preview: synapse_common::config::UrlPreviewConfig::default(),
        oidc: synapse_common::config::OidcConfig::default(),
        saml: synapse_common::config::SamlConfig::default(),
        retention: synapse_common::config::RetentionConfig::default(),
        telemetry: synapse_common::telemetry_config::OpenTelemetryConfig::default(),
        prometheus: synapse_common::telemetry_config::PrometheusConfig::default(),
        performance: synapse_common::config::PerformanceConfig::default(),
        experimental: synapse_common::config::ExperimentalConfig::default(),
        identity: synapse_common::config::IdentityConfig::default(),
        translate: synapse_common::config::TranslateConfig::default(),
    }
}

fn generate_encryption_key() -> [u8; 32] {
    use base64::{engine::general_purpose::STANDARD as B64, Engine as _};

    let path = std::env::var("SYNAPSE_MEGOLM_ENCRYPTION_KEY_PATH").ok();

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
                        let _ = std::fs::set_permissions(&path_buf, std::fs::Permissions::from_mode(0o600));
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
            "SYNAPSE_MEGOLM_ENCRYPTION_KEY_PATH not set; megolm encryption key is ephemeral \
             — all encrypted megolm sessions will be unreadable after server restart. \
             Set this env var to a writable file path for production."
        );
    }

    key
}
