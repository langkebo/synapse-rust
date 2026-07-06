use crate::cache::CacheManager;
use crate::web::routes::AppState;
use axum::extract::FromRef;
use std::sync::Arc;
use synapse_common::rate_limit_config::RateLimitConfigManager;

/// Context for room-related handlers (create, join, leave, state, messages).
///
/// Contains all services commonly accessed by room route handlers, extracted
/// from `AppState` via `FromRef`. Handlers should use `State<RoomContext>`
/// instead of `State<AppState>` to make their dependencies explicit and
/// testable.
#[derive(Clone)]
pub struct RoomContext {
    pub room_service: Arc<synapse_services::room_service::RoomService>,
    pub auth_service: Arc<dyn synapse_services::auth::Auth>,
    pub server_name: String,
    pub cache: Arc<CacheManager>,
    pub sync_service: Arc<synapse_services::sync_service::SyncService>,
    pub thread_service: Arc<synapse_services::thread_service::ThreadService>,
    pub space_service: Arc<synapse_services::space_service::SpaceService>,
    pub room_summary_service: Arc<synapse_services::room_summary_service::RoomSummaryService>,
    pub account_data_service: Arc<synapse_services::account_data_service::AccountDataService>,
    pub search_service: Arc<synapse_services::search_service::SearchService>,
    pub retention_service: Arc<synapse_services::retention_service::RetentionService>,
    pub translation_service: Arc<synapse_services::translation_service::TranslationService>,
    pub federation_client: Arc<dyn synapse_federation::client_api::FederationClientApi>,
    pub rtc_domain_service: Arc<synapse_services::rtc::RtcDomainService>,
    pub e2ee_backup_service: synapse_e2ee::backup::KeyBackupService,
    pub config: synapse_common::config::Config,
    pub admin_audit_service: Option<Arc<synapse_services::AdminAuditService>>,
    #[cfg(feature = "beacons")]
    pub beacon_service: Arc<synapse_services::beacon_service::BeaconService>,
}

impl FromRef<AppState> for RoomContext {
    fn from_ref(state: &AppState) -> Self {
        Self {
            room_service: state.services.rooms.room_service.clone(),
            auth_service: state.services.core.auth_service.clone(),
            server_name: state.services.core.server_name.clone(),
            cache: state.cache.clone(),
            sync_service: state.services.rooms.sync_service.clone(),
            thread_service: state.services.rooms.thread_service.clone(),
            space_service: state.services.rooms.space_service.clone(),
            room_summary_service: state.services.rooms.room_summary_service.clone(),
            account_data_service: state.services.core.account_data_service.clone(),
            search_service: state.services.core.search_service.clone(),
            retention_service: state.services.admin.modules.retention_service.clone(),
            translation_service: state.services.extensions.translation_service.clone(),
            federation_client: state.services.federation.federation_client.clone(),
            rtc_domain_service: state.services.extensions.rtc_domain_service.clone(),
            e2ee_backup_service: state.services.e2ee.backup_service.clone(),
            config: state.services.core.config.clone(),
            admin_audit_service: state.services.admin.security.admin_audit_service.clone().into(),
            #[cfg(feature = "beacons")]
            beacon_service: state.services.rooms.beacon_service.clone(),
        }
    }
}

/// Context for E2EE room-key backup handlers.
///
/// Contains only the services needed by the e2ee handler module.
#[derive(Clone)]
pub struct E2eeRoomContext {
    pub room_service: Arc<synapse_services::room_service::RoomService>,
    pub e2ee_backup_service: synapse_e2ee::backup::KeyBackupService,
    pub auth_service: Arc<dyn synapse_services::auth::Auth>,
    pub admin_audit_service: Option<Arc<synapse_services::AdminAuditService>>,
}

impl FromRef<AppState> for E2eeRoomContext {
    fn from_ref(state: &AppState) -> Self {
        Self {
            room_service: state.services.rooms.room_service.clone(),
            e2ee_backup_service: state.services.e2ee.backup_service.clone(),
            auth_service: state.services.core.auth_service.clone(),
            admin_audit_service: state.services.admin.security.admin_audit_service.clone().into(),
        }
    }
}

/// Context for sync handlers.
#[derive(Clone)]
pub struct SyncContext {
    pub sync_service: Arc<synapse_services::sync_service::SyncService>,
    pub auth_service: Arc<dyn synapse_services::auth::Auth>,
    pub user_storage: Arc<dyn synapse_storage::UserStore>,
    pub cache: Arc<CacheManager>,
    pub config: synapse_common::config::Config,
    pub rate_limit_config_manager: Option<Arc<RateLimitConfigManager>>,
    pub admin_audit_service: Option<Arc<synapse_services::AdminAuditService>>,
}

impl FromRef<AppState> for SyncContext {
    fn from_ref(state: &AppState) -> Self {
        Self {
            sync_service: state.services.rooms.sync_service.clone(),
            auth_service: state.services.core.auth_service.clone(),
            user_storage: state.services.account.user_storage.clone(),
            cache: state.cache.clone(),
            config: state.services.core.config.clone(),
            rate_limit_config_manager: state.rate_limit_config_manager().cloned(),
            admin_audit_service: state.services.admin.security.admin_audit_service.clone().into(),
        }
    }
}

/// Context for device-related handlers.
#[derive(Clone)]
pub struct DeviceContext {
    pub device_storage: Arc<dyn synapse_storage::device::DeviceListStoreApi>,
    pub auth_service: Arc<dyn synapse_services::auth::Auth>,
    pub user_storage: Arc<dyn synapse_storage::UserStore>,
    pub server_name: String,
    pub account_device_list_service: Arc<synapse_services::account_device_list_service::AccountDeviceListService>,
    pub room_service: Arc<synapse_services::room_service::RoomService>,
    pub uia_service: Arc<synapse_services::uia_service::UiaService>,
    pub event_broadcaster: Arc<synapse_federation::EventBroadcaster>,
    pub config: synapse_common::config::Config,
    pub admin_audit_service: Option<Arc<synapse_services::AdminAuditService>>,
}

impl FromRef<AppState> for DeviceContext {
    fn from_ref(state: &AppState) -> Self {
        Self {
            device_storage: state.services.account.device_storage.clone(),
            auth_service: state.services.core.auth_service.clone(),
            user_storage: state.services.account.user_storage.clone(),
            server_name: state.services.core.server_name.clone(),
            account_device_list_service: state.services.account.account_device_list_service.clone(),
            room_service: state.services.rooms.room_service.clone(),
            uia_service: state.services.extensions.uia_service.clone(),
            event_broadcaster: state.services.core.event_broadcaster.clone(),
            config: state.services.core.config.clone(),
            admin_audit_service: state.services.admin.security.admin_audit_service.clone().into(),
        }
    }
}

/// Context for auth-related handlers (login, register, token refresh).
#[derive(Clone)]
pub struct AuthContext {
    pub auth_service: Arc<dyn synapse_services::auth::Auth>,
    pub registration_service: Arc<synapse_services::registration_service::RegistrationService>,
    pub user_storage: Arc<dyn synapse_storage::UserStore>,
    pub server_name: String,
    pub cache: Arc<CacheManager>,
    pub config: synapse_common::config::Config,
    pub admin_audit_service: Option<Arc<synapse_services::AdminAuditService>>,
}

impl FromRef<AppState> for AuthContext {
    fn from_ref(state: &AppState) -> Self {
        Self {
            auth_service: state.services.core.auth_service.clone(),
            registration_service: state.services.core.registration_service.clone(),
            user_storage: state.services.account.user_storage.clone(),
            server_name: state.services.core.server_name.clone(),
            cache: state.cache.clone(),
            config: state.services.core.config.clone(),
            admin_audit_service: state.services.admin.security.admin_audit_service.clone().into(),
        }
    }
}

/// Context for admin handlers (user management, server config, federation, media).
#[derive(Clone)]
pub struct AdminContext {
    // Core
    pub auth_service: Arc<dyn synapse_services::auth::Auth>,
    pub registration_service: Arc<synapse_services::registration_service::RegistrationService>,
    pub config: synapse_common::config::Config,
    pub server_name: String,
    pub cache: Arc<CacheManager>,
    pub metrics: Arc<synapse_common::metrics::MetricsCollector>,
    pub media_service: synapse_services::media_service::MediaService,
    // Room & sync
    pub room_service: Arc<synapse_services::room_service::RoomService>,
    pub sliding_sync_service: Arc<synapse_services::sliding_sync_service::SlidingSyncService>,
    pub space_service: Arc<synapse_services::space_service::SpaceService>,
    // Account
    pub user_storage: Arc<dyn synapse_storage::UserStore>,
    pub account_identity_service: Arc<synapse_services::account_identity_service::AccountIdentityService>,
    pub account_device_list_service: Arc<synapse_services::account_device_list_service::AccountDeviceListService>,
    pub invite_blocklist_storage: Arc<dyn synapse_storage::InviteBlocklistStoreApi>,
    // Admin — user
    pub admin_user_service: Arc<synapse_services::admin_user_service::AdminUserService>,
    pub admin_registration_service: synapse_services::admin_registration_service::AdminRegistrationService,
    pub admin_token_service: Arc<synapse_services::admin_token_service::AdminTokenService>,
    pub refresh_token_service: Arc<synapse_services::refresh_token_service::RefreshTokenService>,
    pub registration_token_service: Arc<synapse_services::registration_token_service::RegistrationTokenService>,
    pub email_verification_storage: Arc<dyn synapse_storage::email_verification::EmailVerificationStoreApi>,
    // Admin — modules
    pub background_update_service: Arc<synapse_services::background_update_service::BackgroundUpdateService>,
    pub retention_service: Arc<synapse_services::retention_service::RetentionService>,
    pub feature_flag_service: Arc<synapse_services::feature_flag_service::FeatureFlagService>,
    pub event_report_service: Arc<synapse_services::event_report_service::EventReportService>,
    pub push_notification_service: Arc<synapse_services::push_notification_service::PushNotificationService>,
    pub app_service_manager: Arc<synapse_services::application_service::ApplicationServiceManager>,
    pub app_service_scheduler: Arc<synapse_services::application_service::ApplicationServiceScheduler>,
    pub module_service: Arc<synapse_services::module_service::ModuleService>,
    pub account_validity_service: Arc<synapse_services::module_service::AccountValidityService>,
    pub worker_manager: Arc<synapse_services::worker::WorkerManager>,
    // Admin — security
    pub admin_audit_service: Arc<synapse_services::AdminAuditService>,
    pub admin_security_service: Arc<synapse_services::admin_security_service::AdminSecurityService>,
    pub admin_server_service: Arc<synapse_services::admin_server_service::AdminServerService>,
    pub captcha_service: Arc<synapse_services::captcha_service::CaptchaService>,
    pub telemetry_alert_service: Arc<synapse_services::telemetry_service::TelemetryAlertService>,
    // Admin — federation
    pub admin_federation_service: Arc<synapse_services::admin_federation_service::AdminFederationService>,
    pub federation_blacklist_service: Arc<synapse_services::federation_blacklist_service::FederationBlacklistService>,
    // Admin — media
    pub admin_media_service: Arc<synapse_services::admin_media_service::AdminMediaService>,
    pub media_quota_service: Arc<synapse_services::media_quota_service::MediaQuotaService>,
    // Cross-cutting
    pub federation_client: Arc<dyn synapse_federation::client_api::FederationClientApi>,
    #[cfg(feature = "server-notifications")]
    pub server_notification_service: Arc<synapse_services::server_notification_service::ServerNotificationService>,
    pub rate_limit_config_manager: Option<Arc<RateLimitConfigManager>>,
    pub shutdown_signal: Option<tokio::sync::broadcast::Sender<()>>,
}

impl FromRef<AppState> for AdminContext {
    fn from_ref(state: &AppState) -> Self {
        Self {
            auth_service: state.services.core.auth_service.clone(),
            registration_service: state.services.core.registration_service.clone(),
            config: state.services.core.config.clone(),
            server_name: state.services.core.server_name.clone(),
            cache: state.cache.clone(),
            metrics: state.services.core.metrics.clone(),
            media_service: state.services.core.media_service.clone(),
            room_service: state.services.rooms.room_service.clone(),
            sliding_sync_service: state.services.rooms.sliding_sync_service.clone(),
            space_service: state.services.rooms.space_service.clone(),
            account_identity_service: state.services.account.account_identity_service.clone(),
            account_device_list_service: state.services.account.account_device_list_service.clone(),
            user_storage: state.services.account.user_storage.clone(),
            invite_blocklist_storage: state.services.account.invite_blocklist_storage.clone(),
            admin_user_service: state.services.admin.user.admin_user_service.clone(),
            admin_registration_service: state.services.admin.user.admin_registration_service.clone(),
            admin_token_service: state.services.admin.user.admin_token_service.clone(),
            refresh_token_service: state.services.admin.user.refresh_token_service.clone(),
            registration_token_service: state.services.admin.user.registration_token_service.clone(),
            email_verification_storage: state.services.admin.user.email_verification_storage.clone(),
            background_update_service: state.services.admin.modules.background_update_service.clone(),
            retention_service: state.services.admin.modules.retention_service.clone(),
            feature_flag_service: state.services.admin.modules.feature_flag_service.clone(),
            event_report_service: state.services.admin.modules.event_report_service.clone(),
            push_notification_service: state.services.admin.modules.push_notification_service.clone(),
            app_service_manager: state.services.admin.modules.app_service_manager.clone(),
            app_service_scheduler: state.services.admin.modules.app_service_scheduler.clone(),
            module_service: state.services.admin.modules.module_service.clone(),
            account_validity_service: state.services.admin.modules.account_validity_service.clone(),
            worker_manager: state.services.admin.modules.worker_manager.clone(),
            admin_audit_service: state.services.admin.security.admin_audit_service.clone(),
            admin_security_service: state.services.admin.security.admin_security_service.clone(),
            admin_server_service: state.services.admin.security.admin_server_service.clone(),
            captcha_service: state.services.admin.security.captcha_service.clone(),
            telemetry_alert_service: state.services.admin.security.telemetry_alert_service.clone(),
            admin_federation_service: state.services.admin.federation.admin_federation_service.clone(),
            federation_blacklist_service: state.services.admin.federation.federation_blacklist_service.clone(),
            admin_media_service: state.services.admin.media.admin_media_service.clone(),
            media_quota_service: state.services.admin.media.media_quota_service.clone(),
            federation_client: state.services.federation.federation_client.clone(),
            #[cfg(feature = "server-notifications")]
            server_notification_service: state.services.extensions.server_notification_service.clone(),
            rate_limit_config_manager: state.rate_limit_config_manager().cloned(),
            shutdown_signal: state.shutdown_signal.clone(),
        }
    }
}

/// Context for federation handlers (inbound transaction, keys, membership).
#[derive(Clone)]
pub struct FederationContext {
    pub auth_service: Arc<dyn synapse_services::auth::Auth>,
    pub user_storage: Arc<dyn synapse_storage::UserStore>,
    pub config: synapse_common::config::Config,
    pub server_name: String,
    pub cache: Arc<CacheManager>,
    pub metrics: Arc<synapse_common::metrics::MetricsCollector>,
    pub room_service: Arc<synapse_services::room_service::RoomService>,
    pub registration_service: Arc<synapse_services::registration_service::RegistrationService>,
    pub account_identity_service: Arc<synapse_services::account_identity_service::AccountIdentityService>,
    pub account_device_list_service: Arc<synapse_services::account_device_list_service::AccountDeviceListService>,
    pub key_rotation_manager: synapse_federation::KeyRotationManager,
    pub federation_client: Arc<dyn synapse_federation::client_api::FederationClientApi>,
    pub event_auth_chain: synapse_federation::EventAuthChain,
    pub device_sync_manager: synapse_federation::DeviceSyncManager,
    pub federation_server_name: String,
    pub admin_audit_service: Option<Arc<synapse_services::AdminAuditService>>,
    pub worker_manager: Arc<synapse_services::worker::WorkerManager>,
}

impl FromRef<AppState> for FederationContext {
    fn from_ref(state: &AppState) -> Self {
        Self {
            auth_service: state.services.core.auth_service.clone(),
            user_storage: state.services.account.user_storage.clone(),
            config: state.services.core.config.clone(),
            server_name: state.services.core.server_name.clone(),
            cache: state.cache.clone(),
            metrics: state.services.core.metrics.clone(),
            room_service: state.services.rooms.room_service.clone(),
            registration_service: state.services.core.registration_service.clone(),
            account_identity_service: state.services.account.account_identity_service.clone(),
            account_device_list_service: state.services.account.account_device_list_service.clone(),
            key_rotation_manager: state.services.federation.key_rotation_manager.clone(),
            federation_client: state.services.federation.federation_client.clone(),
            event_auth_chain: state.services.federation.event_auth_chain.clone(),
            device_sync_manager: state.services.federation.device_sync_manager.clone(),
            federation_server_name: state.services.federation.federation_server_name.clone(),
            admin_audit_service: state.services.admin.security.admin_audit_service.clone().into(),
            worker_manager: state.services.admin.modules.worker_manager.clone(),
        }
    }
}

/// Context for media and media-adjacent handlers.
#[derive(Clone)]
pub struct MediaContext {
    pub auth_service: Arc<dyn synapse_services::auth::Auth>,
    pub user_storage: Arc<dyn synapse_storage::UserStore>,
    pub config: synapse_common::config::Config,
    pub server_name: String,
    pub cache: Arc<CacheManager>,
    pub media_service: synapse_services::media_service::MediaService,
    pub media_domain_service: Arc<synapse_services::media::MediaDomainService>,
    pub media_quota_service: Arc<synapse_services::media_quota_service::MediaQuotaService>,
    pub room_service: Arc<synapse_services::room_service::RoomService>,
    pub federation_client: Arc<dyn synapse_federation::client_api::FederationClientApi>,
    pub account_identity_service: Arc<synapse_services::account_identity_service::AccountIdentityService>,
    pub admin_audit_service: Option<Arc<synapse_services::AdminAuditService>>,
}

impl FromRef<AppState> for MediaContext {
    fn from_ref(state: &AppState) -> Self {
        Self {
            auth_service: state.services.core.auth_service.clone(),
            user_storage: state.services.account.user_storage.clone(),
            config: state.services.core.config.clone(),
            server_name: state.services.core.server_name.clone(),
            cache: state.cache.clone(),
            media_service: state.services.core.media_service.clone(),
            media_domain_service: state.services.extensions.media_domain_service.clone(),
            media_quota_service: state.services.admin.media.media_quota_service.clone(),
            room_service: state.services.rooms.room_service.clone(),
            federation_client: state.services.federation.federation_client.clone(),
            account_identity_service: state.services.account.account_identity_service.clone(),
            admin_audit_service: state.services.admin.security.admin_audit_service.clone().into(),
        }
    }
}
