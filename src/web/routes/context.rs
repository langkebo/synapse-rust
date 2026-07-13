use crate::cache::{CacheManager, FederationSignatureCache};
use crate::web::routes::AppState;
use axum::extract::FromRef;
use std::collections::HashMap;
use std::sync::Arc;
use synapse_common::rate_limit_config::RateLimitConfigManager;
use tokio::sync::{Mutex, RwLock, Semaphore};

// ── CoreContext ───────────────────────────────────────────────────────────

/// Minimal context for the global request-pipeline middlewares (auth, shadow-ban,
/// csrf, rate-limit). Carries only the shared services those middlewares read.
#[derive(Clone)]
pub struct CoreContext {
    pub validator: Arc<synapse_common::validation::Validator>,
    pub token_auth: Arc<dyn synapse_services::auth::TokenAuth>,
    pub credential_auth: Arc<dyn synapse_services::auth::CredentialAuth>,
    pub room_auth: Arc<dyn synapse_services::auth::RoomAuth>,
    pub config: synapse_common::config::Config,
    pub cache: Arc<CacheManager>,
    pub rate_limit_config_manager: Option<Arc<RateLimitConfigManager>>,
}

impl CoreContext {
    /// Mirror of `AppState::rate_limit_config` so rate_limit_middleware keeps identical behavior.
    pub fn rate_limit_config(&self) -> Option<crate::common::RateLimitConfigFile> {
        self.rate_limit_config_manager.as_ref().map(|manager| manager.get_config())
    }
}

impl FromRef<AppState> for CoreContext {
    fn from_ref(state: &AppState) -> Self {
        Self {
            validator: state.services.core.validator.clone(),
            token_auth: state.services.core.token_auth.clone(),
            credential_auth: state.services.core.credential_auth.clone(),
            room_auth: state.services.core.room_auth.clone(),
            config: state.services.core.config.clone(),
            cache: state.cache.clone(),
            rate_limit_config_manager: state.rate_limit_config_manager().cloned(),
        }
    }
}

// ── RoomContext ───────────────────────────────────────────────────────────

#[derive(Clone)]
pub struct RoomContext {
    pub room_service: Arc<dyn synapse_services::RoomServiceApi>,
    pub validator: Arc<synapse_common::validation::Validator>,
    pub token_auth: Arc<dyn synapse_services::auth::TokenAuth>,
    pub credential_auth: Arc<dyn synapse_services::auth::CredentialAuth>,
    pub room_auth: Arc<dyn synapse_services::auth::RoomAuth>,
    pub server_name: String,
    pub cache: Arc<CacheManager>,
    pub sync_service: Arc<dyn synapse_services::sync_service::SyncServiceApi>,
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
    pub account_identity_service: Arc<synapse_services::account_identity_service::AccountIdentityService>,
    pub account_device_list_service: Arc<synapse_services::account_device_list_service::AccountDeviceListService>,
    pub push_notification_service: Arc<synapse_services::push_notification_service::PushNotificationService>,
    pub event_broadcaster: Arc<synapse_federation::EventBroadcaster>,
    pub cross_signing_service: synapse_e2ee::cross_signing::CrossSigningService,
    pub room_storage: Arc<dyn synapse_storage::RoomStoreApi>,
    pub friend_room_service: Arc<synapse_services::friend_room_service::models::FriendRoomService>,
    pub metrics: Arc<synapse_common::metrics::MetricsCollector>,
    #[cfg(feature = "beacons")]
    pub beacon_service: Arc<synapse_services::beacon_service::BeaconService>,
    pub presence_service: Arc<synapse_services::presence_service::PresenceService>,
    pub typing_service: Arc<synapse_services::typing_service::TypingService>,
    pub directory_service: Arc<synapse_services::directory_service::DirectoryService>,
    pub relations_service: Arc<synapse_services::relations_service::RelationsService>,
    #[cfg(feature = "voice-extended")]
    pub voice_service: Arc<synapse_services::voice_service::VoiceService>,
    pub ssss_service: synapse_e2ee::ssss::SecretStorageService,
    pub dehydrated_device_service: Arc<synapse_services::dehydrated_device_service::DehydratedDeviceService>,
    #[cfg(feature = "burn-after-read")]
    pub burn_after_read: Arc<synapse_services::burn_after_read_service::BurnAfterReadService>,
}

impl FromRef<AppState> for RoomContext {
    fn from_ref(state: &AppState) -> Self {
        Self {
            room_service: state.services.rooms.room_service.clone(),
            validator: state.services.core.validator.clone(),
            token_auth: state.services.core.token_auth.clone(),
            credential_auth: state.services.core.credential_auth.clone(),
            room_auth: state.services.core.room_auth.clone(),
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
            account_identity_service: state.services.account.account_identity_service.clone(),
            account_device_list_service: state.services.account.account_device_list_service.clone(),
            push_notification_service: state.services.admin.modules.push_notification_service.clone(),
            event_broadcaster: state.services.core.event_broadcaster.clone(),
            cross_signing_service: state.services.e2ee.cross_signing_service.clone(),
            room_storage: state.services.rooms.room_storage.clone(),
            friend_room_service: state.services.extensions.friend_room_service.clone(),
            metrics: state.services.core.metrics.clone(),
            #[cfg(feature = "beacons")]
            beacon_service: state.services.rooms.beacon_service.clone(),
            presence_service: state.services.account.presence_service.clone(),
            typing_service: state.services.rooms.typing_service.clone(),
            directory_service: state.services.extensions.directory_service.clone(),
            relations_service: state.services.rooms.relations_service.clone(),
            #[cfg(feature = "voice-extended")]
            voice_service: Arc::new(state.services.extensions.voice_service.clone()),
            ssss_service: state.services.e2ee.ssss_service.clone(),
            dehydrated_device_service: Arc::new(state.services.e2ee.dehydrated_device_service.clone()),
            #[cfg(feature = "burn-after-read")]
            burn_after_read: state.services.extensions.burn_after_read.clone(),
        }
    }
}

// ── E2eeRoomContext ───────────────────────────────────────────────────────

#[derive(Clone)]
pub struct E2eeRoomContext {
    pub room_service: Arc<dyn synapse_services::RoomServiceApi>,
    pub e2ee_backup_service: synapse_e2ee::backup::KeyBackupService,
    pub secure_backup_service: synapse_e2ee::secure_backup::SecureBackupService,
    pub validator: Arc<synapse_common::validation::Validator>,
    pub token_auth: Arc<dyn synapse_services::auth::TokenAuth>,
    pub credential_auth: Arc<dyn synapse_services::auth::CredentialAuth>,
    pub room_auth: Arc<dyn synapse_services::auth::RoomAuth>,
    pub admin_audit_service: Option<Arc<synapse_services::AdminAuditService>>,
    pub pool: Arc<sqlx::PgPool>,
}

impl FromRef<AppState> for E2eeRoomContext {
    fn from_ref(state: &AppState) -> Self {
        Self {
            room_service: state.services.rooms.room_service.clone(),
            e2ee_backup_service: state.services.e2ee.backup_service.clone(),
            secure_backup_service: state.services.e2ee.secure_backup_service.clone(),
            validator: state.services.core.validator.clone(),
            token_auth: state.services.core.token_auth.clone(),
            credential_auth: state.services.core.credential_auth.clone(),
            room_auth: state.services.core.room_auth.clone(),
            admin_audit_service: state.services.admin.security.admin_audit_service.clone().into(),
            pool: state.services.database_pool(),
        }
    }
}

// ── SyncContext ───────────────────────────────────────────────────────────

#[derive(Clone)]
pub struct SyncContext {
    pub sync_service: Arc<dyn synapse_services::sync_service::SyncServiceApi>,
    pub validator: Arc<synapse_common::validation::Validator>,
    pub token_auth: Arc<dyn synapse_services::auth::TokenAuth>,
    pub credential_auth: Arc<dyn synapse_services::auth::CredentialAuth>,
    pub room_auth: Arc<dyn synapse_services::auth::RoomAuth>,
    pub user_storage: Arc<dyn synapse_storage::UserStore>,
    pub user_service: Arc<synapse_services::UserService>,
    pub cache: Arc<CacheManager>,
    pub config: synapse_common::config::Config,
    pub rate_limit_config_manager: Option<Arc<RateLimitConfigManager>>,
    pub admin_audit_service: Option<Arc<synapse_services::AdminAuditService>>,
    pub metrics: Arc<synapse_common::metrics::MetricsCollector>,
    pub sliding_sync_service: Arc<synapse_services::sliding_sync_service::SlidingSyncService>,
    pub client_push_service: Arc<synapse_services::client_push_service::ClientPushService>,
}

impl FromRef<AppState> for SyncContext {
    fn from_ref(state: &AppState) -> Self {
        Self {
            sync_service: state.services.rooms.sync_service.clone(),
            validator: state.services.core.validator.clone(),
            token_auth: state.services.core.token_auth.clone(),
            credential_auth: state.services.core.credential_auth.clone(),
            room_auth: state.services.core.room_auth.clone(),
            user_storage: state.services.account.user_storage.clone(),
            user_service: state.services.account.user_service.clone(),
            cache: state.cache.clone(),
            config: state.services.core.config.clone(),
            rate_limit_config_manager: state.rate_limit_config_manager().cloned(),
            admin_audit_service: state.services.admin.security.admin_audit_service.clone().into(),
            metrics: state.services.core.metrics.clone(),
            sliding_sync_service: state.services.rooms.sliding_sync_service.clone(),
            client_push_service: state.services.core.client_push_service.clone(),
        }
    }
}

impl SyncContext {
    pub fn sync_rate_limit_override(&self) -> Option<crate::web::routes::state::SyncRateLimitOverride> {
        self.rate_limit_config_manager.as_ref().map(|manager| {
            let config = manager.get_config();
            crate::web::routes::state::SyncRateLimitOverride {
                fail_open_on_error: config.fail_open_on_error,
                sync: config.sync,
            }
        })
    }
}

// ── DeviceContext ─────────────────────────────────────────────────────────

#[derive(Clone)]
pub struct DeviceContext {
    pub device_storage: Arc<dyn synapse_storage::device::DeviceListStoreApi>,
    pub validator: Arc<synapse_common::validation::Validator>,
    pub token_auth: Arc<dyn synapse_services::auth::TokenAuth>,
    pub credential_auth: Arc<dyn synapse_services::auth::CredentialAuth>,
    pub room_auth: Arc<dyn synapse_services::auth::RoomAuth>,
    pub user_storage: Arc<dyn synapse_storage::UserStore>,
    pub user_service: Arc<synapse_services::UserService>,
    pub server_name: String,
    pub account_device_list_service: Arc<synapse_services::account_device_list_service::AccountDeviceListService>,
    pub room_service: Arc<dyn synapse_services::RoomServiceApi>,
    pub uia_service: Arc<synapse_services::uia_service::UiaService>,
    pub event_broadcaster: Arc<synapse_federation::EventBroadcaster>,
    pub config: synapse_common::config::Config,
    pub admin_audit_service: Option<Arc<synapse_services::AdminAuditService>>,
    pub account_identity_service: Arc<synapse_services::account_identity_service::AccountIdentityService>,
    pub cross_signing_service: synapse_e2ee::cross_signing::CrossSigningService,
    pub device_keys_service: synapse_e2ee::device_keys::DeviceKeyService,
    pub federation_client: Arc<dyn synapse_federation::client_api::FederationClientApi>,
    pub to_device_service: synapse_e2ee::to_device::ToDeviceService,
    pub metrics: Arc<synapse_common::metrics::MetricsCollector>,
    pub cache: Arc<CacheManager>,
    pub event_notifier: synapse_services::event_notifier::EventNotifier,
    pub key_request_service: synapse_e2ee::key_request::KeyRequestService,
    pub verification_service: synapse_e2ee::verification::VerificationService,
    pub device_trust_service: synapse_e2ee::device_trust::DeviceTrustService,
    pub key_rotation_service: Arc<synapse_services::FederationKeyRotationService>,
}

impl FromRef<AppState> for DeviceContext {
    fn from_ref(state: &AppState) -> Self {
        Self {
            device_storage: state.services.account.device_storage.clone(),
            validator: state.services.core.validator.clone(),
            token_auth: state.services.core.token_auth.clone(),
            credential_auth: state.services.core.credential_auth.clone(),
            room_auth: state.services.core.room_auth.clone(),
            user_storage: state.services.account.user_storage.clone(),
            user_service: state.services.account.user_service.clone(),
            server_name: state.services.core.server_name.clone(),
            account_device_list_service: state.services.account.account_device_list_service.clone(),
            room_service: state.services.rooms.room_service.clone(),
            uia_service: state.services.extensions.uia_service.clone(),
            event_broadcaster: state.services.core.event_broadcaster.clone(),
            config: state.services.core.config.clone(),
            admin_audit_service: state.services.admin.security.admin_audit_service.clone().into(),
            account_identity_service: state.services.account.account_identity_service.clone(),
            cross_signing_service: state.services.e2ee.cross_signing_service.clone(),
            device_keys_service: state.services.e2ee.device_keys_service.clone(),
            federation_client: state.services.federation.federation_client.clone(),
            to_device_service: state.services.e2ee.to_device_service.clone(),
            metrics: state.services.core.metrics.clone(),
            cache: state.cache.clone(),
            event_notifier: state.services.core.event_notifier.clone(),
            key_request_service: state.services.e2ee.key_request_service.clone(),
            verification_service: state.services.e2ee.verification_service.clone(),
            device_trust_service: state.services.e2ee.device_trust_service.clone(),
            key_rotation_service: state.services.federation.key_rotation_service.clone(),
        }
    }
}

// ── AuthContext ───────────────────────────────────────────────────────────

#[derive(Clone)]
pub struct AuthContext {
    pub validator: Arc<synapse_common::validation::Validator>,
    pub token_auth: Arc<dyn synapse_services::auth::TokenAuth>,
    pub credential_auth: Arc<dyn synapse_services::auth::CredentialAuth>,
    pub room_auth: Arc<dyn synapse_services::auth::RoomAuth>,
    pub registration_service: Arc<synapse_services::registration_service::RegistrationService>,
    pub user_storage: Arc<dyn synapse_storage::UserStore>,
    pub user_service: Arc<synapse_services::UserService>,
    pub server_name: String,
    pub cache: Arc<CacheManager>,
    pub config: synapse_common::config::Config,
    pub admin_audit_service: Option<Arc<synapse_services::AdminAuditService>>,
    pub account_identity_service: Arc<synapse_services::account_identity_service::AccountIdentityService>,
    pub uia_service: Arc<synapse_services::uia_service::UiaService>,
    pub federation_client: Arc<dyn synapse_federation::client_api::FederationClientApi>,
    pub email_verification_storage: Arc<dyn synapse_storage::email_verification::EmailVerificationStoreApi>,
    pub account_device_list_service: Arc<synapse_services::account_device_list_service::AccountDeviceListService>,
    pub refresh_token_service: Arc<synapse_services::refresh_token_service::RefreshTokenService>,
    pub metrics: Arc<synapse_common::metrics::MetricsCollector>,
    pub identity_service: Arc<synapse_services::identity::IdentityService>,
    pub oidc_service: Option<Arc<synapse_services::oidc_service::OidcService>>,
    #[cfg(feature = "builtin-oidc")]
    pub builtin_oidc_provider: Option<Arc<synapse_services::builtin_oidc_provider::BuiltinOidcProvider>>,
    pub qr_login_storage: Arc<dyn synapse_storage::qr_login::QrLoginStoreApi>,
    pub threepid_storage: Arc<dyn synapse_storage::threepid::ThreepidStoreApi>,
    pub rendezvous_storage: Arc<dyn synapse_storage::rendezvous::RendezvousStoreApi>,
    pub rendezvous_message_storage: Arc<dyn synapse_storage::rendezvous::RendezvousMessageStoreApi>,
}

impl FromRef<AppState> for AuthContext {
    fn from_ref(state: &AppState) -> Self {
        Self {
            validator: state.services.core.validator.clone(),
            token_auth: state.services.core.token_auth.clone(),
            credential_auth: state.services.core.credential_auth.clone(),
            room_auth: state.services.core.room_auth.clone(),
            registration_service: state.services.core.registration_service.clone(),
            user_storage: state.services.account.user_storage.clone(),
            user_service: state.services.account.user_service.clone(),
            server_name: state.services.core.server_name.clone(),
            cache: state.cache.clone(),
            config: state.services.core.config.clone(),
            admin_audit_service: state.services.admin.security.admin_audit_service.clone().into(),
            account_identity_service: state.services.account.account_identity_service.clone(),
            uia_service: state.services.extensions.uia_service.clone(),
            federation_client: state.services.federation.federation_client.clone(),
            email_verification_storage: state.services.admin.user.email_verification_storage.clone(),
            account_device_list_service: state.services.account.account_device_list_service.clone(),
            refresh_token_service: state.services.admin.user.refresh_token_service.clone(),
            metrics: state.services.core.metrics.clone(),
            identity_service: state.services.extensions.identity_service.clone(),
            oidc_service: state.services.sso.oidc_service.clone(),
            #[cfg(feature = "builtin-oidc")]
            builtin_oidc_provider: state.services.sso.builtin_oidc_provider.clone(),
            qr_login_storage: state.services.account.qr_login_storage.clone(),
            threepid_storage: state.services.account.threepid_storage.clone(),
            rendezvous_storage: state.services.admin.modules.rendezvous_storage.clone(),
            rendezvous_message_storage: state.services.admin.modules.rendezvous_message_storage.clone(),
        }
    }
}

// ── AdminContext ──────────────────────────────────────────────────────────

#[derive(Clone)]
pub struct AdminContext {
    // Core
    pub validator: Arc<synapse_common::validation::Validator>,
    pub token_auth: Arc<dyn synapse_services::auth::TokenAuth>,
    pub credential_auth: Arc<dyn synapse_services::auth::CredentialAuth>,
    pub room_auth: Arc<dyn synapse_services::auth::RoomAuth>,
    pub registration_service: Arc<synapse_services::registration_service::RegistrationService>,
    pub config: synapse_common::config::Config,
    pub server_name: String,
    pub cache: Arc<CacheManager>,
    pub metrics: Arc<synapse_common::metrics::MetricsCollector>,
    pub media_service: synapse_services::media_service::MediaService,
    // Room & sync
    pub room_service: Arc<dyn synapse_services::RoomServiceApi>,
    pub sliding_sync_service: Arc<synapse_services::sliding_sync_service::SlidingSyncService>,
    pub space_service: Arc<synapse_services::space_service::SpaceService>,
    // Account
    pub user_storage: Arc<dyn synapse_storage::UserStore>,
    pub user_service: Arc<synapse_services::UserService>,
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
    pub module_storage: Arc<dyn synapse_storage::module::ModuleStoreApi>,
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
    pub account_data_service: Arc<synapse_services::account_data_service::AccountDataService>,
    pub health_checker: Arc<crate::common::health::HealthChecker>,
    #[cfg(feature = "openclaw-routes")]
    pub openclaw_service: Arc<synapse_services::openclaw_service::OpenClawService>,
    #[cfg(feature = "openclaw-routes")]
    pub mcp_proxy_service: Arc<synapse_services::mcp_proxy::McpProxyService>,
    #[cfg(feature = "openclaw-routes")]
    pub ai_connection_storage: Arc<dyn synapse_storage::ai_connection::AiConnectionStoreApi>,
    #[cfg(feature = "openclaw-routes")]
    pub matrix_ai_connection_service: Arc<synapse_services::matrix_ai_connection_service::MatrixAiConnectionService>,
    pub room_storage: Arc<dyn synapse_storage::RoomStoreApi>,
    #[cfg(feature = "friends")]
    pub friend_room_service: Arc<synapse_services::friend_room_service::models::FriendRoomService>,
    pub ssss_service: synapse_e2ee::ssss::SecretStorageService,
    pub token_storage: Arc<dyn synapse_storage::token::AccessTokenStoreApi>,
    pub qr_login_storage: Arc<dyn synapse_storage::qr_login::QrLoginStoreApi>,
    pub client_push_service: Arc<synapse_services::client_push_service::ClientPushService>,
    #[cfg(feature = "widgets")]
    pub widget_service: Arc<synapse_services::widget_service::WidgetService>,
    #[cfg(feature = "external-services")]
    pub external_service_integration: Arc<synapse_services::external_service_integration::ExternalServiceIntegration>,
}

impl FromRef<AppState> for AdminContext {
    fn from_ref(state: &AppState) -> Self {
        Self {
            validator: state.services.core.validator.clone(),
            token_auth: state.services.core.token_auth.clone(),
            credential_auth: state.services.core.credential_auth.clone(),
            room_auth: state.services.core.room_auth.clone(),
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
            user_service: state.services.account.user_service.clone(),
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
            module_storage: state.services.admin.modules.module_storage.clone(),
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
            account_data_service: state.services.core.account_data_service.clone(),
            health_checker: state.health_checker.clone(),
            #[cfg(feature = "openclaw-routes")]
            openclaw_service: state.openclaw_service.clone(),
            #[cfg(feature = "openclaw-routes")]
            mcp_proxy_service: state.mcp_proxy_service.clone(),
            #[cfg(feature = "openclaw-routes")]
            ai_connection_storage: state.ai_connection_storage.clone(),
            #[cfg(feature = "openclaw-routes")]
            matrix_ai_connection_service: state.matrix_ai_connection_service.clone(),
            room_storage: state.services.rooms.room_storage.clone(),
            #[cfg(feature = "friends")]
            friend_room_service: state.services.extensions.friend_room_service.clone(),
            ssss_service: state.services.e2ee.ssss_service.clone(),
            token_storage: state.services.account.token_storage.clone(),
            qr_login_storage: state.services.account.qr_login_storage.clone(),
            client_push_service: state.services.core.client_push_service.clone(),
            #[cfg(feature = "widgets")]
            widget_service: state.services.extensions.widget_service.clone(),
            #[cfg(feature = "external-services")]
            external_service_integration: state.services.admin.modules.external_service_integration.clone(),
        }
    }
}

// ── FederationContext ─────────────────────────────────────────────────────

#[derive(Clone)]
pub struct FederationContext {
    pub validator: Arc<synapse_common::validation::Validator>,
    pub token_auth: Arc<dyn synapse_services::auth::TokenAuth>,
    pub credential_auth: Arc<dyn synapse_services::auth::CredentialAuth>,
    pub room_auth: Arc<dyn synapse_services::auth::RoomAuth>,
    pub user_storage: Arc<dyn synapse_storage::UserStore>,
    pub user_service: Arc<synapse_services::UserService>,
    pub config: synapse_common::config::Config,
    pub server_name: String,
    pub cache: Arc<CacheManager>,
    pub metrics: Arc<synapse_common::metrics::MetricsCollector>,
    pub room_service: Arc<dyn synapse_services::RoomServiceApi>,
    pub space_service: Arc<synapse_services::space_service::SpaceService>,
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
    pub media_service: synapse_services::media_service::MediaService,
    pub account_data_service: Arc<synapse_services::account_data_service::AccountDataService>,
    pub federation_signature_cache: Arc<FederationSignatureCache>,
    pub federation_key_fetch_general_semaphore: Arc<Semaphore>,
    pub federation_key_fetch_priority_semaphore: Arc<Semaphore>,
    pub admin_federation_service: Arc<synapse_services::admin_federation_service::AdminFederationService>,
    pub device_keys_service: synapse_e2ee::device_keys::DeviceKeyService,
    pub cross_signing_service: synapse_e2ee::cross_signing::CrossSigningService,
    pub to_device_service: synapse_e2ee::to_device::ToDeviceService,
    pub presence_storage: Arc<dyn synapse_storage::presence::PresenceStoreApi>,
    pub device_storage: Arc<dyn synapse_storage::device::DeviceListStoreApi>,
    pub federation_inbound_edu_semaphore: Arc<Semaphore>,
    pub federation_inbound_edu_origin_semaphores: Arc<Mutex<HashMap<String, Arc<Semaphore>>>>,
    pub federation_presence_backoff_until: Arc<RwLock<HashMap<String, i64>>>,
    pub federation_join_semaphore: Arc<Semaphore>,
}

impl FromRef<AppState> for FederationContext {
    fn from_ref(state: &AppState) -> Self {
        Self {
            validator: state.services.core.validator.clone(),
            token_auth: state.services.core.token_auth.clone(),
            credential_auth: state.services.core.credential_auth.clone(),
            room_auth: state.services.core.room_auth.clone(),
            user_storage: state.services.account.user_storage.clone(),
            user_service: state.services.account.user_service.clone(),
            config: state.services.core.config.clone(),
            server_name: state.services.core.server_name.clone(),
            cache: state.cache.clone(),
            metrics: state.services.core.metrics.clone(),
            room_service: state.services.rooms.room_service.clone(),
            space_service: state.services.rooms.space_service.clone(),
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
            media_service: state.services.core.media_service.clone(),
            account_data_service: state.services.core.account_data_service.clone(),
            federation_signature_cache: state.federation_signature_cache.clone(),
            federation_key_fetch_general_semaphore: state.federation_key_fetch_general_semaphore.clone(),
            federation_key_fetch_priority_semaphore: state.federation_key_fetch_priority_semaphore.clone(),
            admin_federation_service: state.services.admin.federation.admin_federation_service.clone(),
            device_keys_service: state.services.e2ee.device_keys_service.clone(),
            cross_signing_service: state.services.e2ee.cross_signing_service.clone(),
            to_device_service: state.services.e2ee.to_device_service.clone(),
            presence_storage: state.services.account.presence_storage.clone(),
            device_storage: state.services.account.device_storage.clone(),
            federation_inbound_edu_semaphore: state.federation_inbound_edu_semaphore.clone(),
            federation_inbound_edu_origin_semaphores: state.federation_inbound_edu_origin_semaphores.clone(),
            federation_presence_backoff_until: state.federation_presence_backoff_until.clone(),
            federation_join_semaphore: state.federation_join_semaphore.clone(),
        }
    }
}

// ── MediaContext ──────────────────────────────────────────────────────────

#[derive(Clone)]
pub struct MediaContext {
    pub validator: Arc<synapse_common::validation::Validator>,
    pub token_auth: Arc<dyn synapse_services::auth::TokenAuth>,
    pub credential_auth: Arc<dyn synapse_services::auth::CredentialAuth>,
    pub room_auth: Arc<dyn synapse_services::auth::RoomAuth>,
    pub user_storage: Arc<dyn synapse_storage::UserStore>,
    pub user_service: Arc<synapse_services::UserService>,
    pub config: synapse_common::config::Config,
    pub server_name: String,
    pub cache: Arc<CacheManager>,
    pub media_service: synapse_services::media_service::MediaService,
    pub media_domain_service: Arc<synapse_services::media::MediaDomainService>,
    pub media_quota_service: Arc<synapse_services::media_quota_service::MediaQuotaService>,
    pub room_service: Arc<dyn synapse_services::RoomServiceApi>,
    pub federation_client: Arc<dyn synapse_federation::client_api::FederationClientApi>,
    pub account_identity_service: Arc<synapse_services::account_identity_service::AccountIdentityService>,
    pub admin_audit_service: Option<Arc<synapse_services::AdminAuditService>>,
}

impl FromRef<AppState> for MediaContext {
    fn from_ref(state: &AppState) -> Self {
        Self {
            validator: state.services.core.validator.clone(),
            token_auth: state.services.core.token_auth.clone(),
            credential_auth: state.services.core.credential_auth.clone(),
            room_auth: state.services.core.room_auth.clone(),
            user_storage: state.services.account.user_storage.clone(),
            user_service: state.services.account.user_service.clone(),
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

// ── SsoContext ────────────────────────────────────────────────────────────

#[derive(Clone)]
pub struct SsoContext {
    pub validator: Arc<synapse_common::validation::Validator>,
    pub token_auth: Arc<dyn synapse_services::auth::TokenAuth>,
    pub credential_auth: Arc<dyn synapse_services::auth::CredentialAuth>,
    pub room_auth: Arc<dyn synapse_services::auth::RoomAuth>,
    pub config: synapse_common::config::Config,
    pub server_name: String,
    pub cache: Arc<CacheManager>,
    pub registration_service: Arc<synapse_services::registration_service::RegistrationService>,
    pub user_storage: Arc<dyn synapse_storage::UserStore>,
    pub user_service: Arc<synapse_services::UserService>,
    pub account_identity_service: Arc<synapse_services::account_identity_service::AccountIdentityService>,
    pub account_device_list_service: Arc<synapse_services::account_device_list_service::AccountDeviceListService>,
    #[cfg(feature = "saml-sso")]
    pub saml_service: Arc<synapse_services::saml_service::SamlService>,
    #[cfg(feature = "cas-sso")]
    pub cas_service: Arc<synapse_services::cas_service::CasService>,
    pub oidc_service: Option<Arc<synapse_services::oidc_service::OidcService>>,
    pub oidc_mapping_storage: Arc<dyn synapse_storage::oidc_user_mapping::OidcUserMappingStoreApi>,
    #[cfg(feature = "builtin-oidc")]
    pub builtin_oidc_provider: Option<Arc<synapse_services::builtin_oidc_provider::BuiltinOidcProvider>>,
    #[cfg(not(feature = "builtin-oidc"))]
    pub builtin_oidc_provider: Option<()>,
    pub admin_audit_service: Option<Arc<synapse_services::AdminAuditService>>,
    pub refresh_token_service: Arc<synapse_services::refresh_token_service::RefreshTokenService>,
}

impl FromRef<AppState> for SsoContext {
    fn from_ref(state: &AppState) -> Self {
        Self {
            validator: state.services.core.validator.clone(),
            token_auth: state.services.core.token_auth.clone(),
            credential_auth: state.services.core.credential_auth.clone(),
            room_auth: state.services.core.room_auth.clone(),
            config: state.services.core.config.clone(),
            server_name: state.services.core.server_name.clone(),
            cache: state.cache.clone(),
            registration_service: state.services.core.registration_service.clone(),
            user_storage: state.services.account.user_storage.clone(),
            user_service: state.services.account.user_service.clone(),
            account_identity_service: state.services.account.account_identity_service.clone(),
            account_device_list_service: state.services.account.account_device_list_service.clone(),
            #[cfg(feature = "saml-sso")]
            saml_service: state.services.sso.saml_service.clone(),
            #[cfg(feature = "cas-sso")]
            cas_service: state.services.sso.cas_service.clone(),
            oidc_service: state.services.sso.oidc_service.clone(),
            oidc_mapping_storage: state.services.sso.oidc_mapping_storage.clone(),
            #[cfg(feature = "builtin-oidc")]
            builtin_oidc_provider: state.services.sso.builtin_oidc_provider.clone(),
            #[cfg(not(feature = "builtin-oidc"))]
            builtin_oidc_provider: None,
            admin_audit_service: state.services.admin.security.admin_audit_service.clone().into(),
            refresh_token_service: state.services.admin.user.refresh_token_service.clone(),
        }
    }
}

// ── FriendContext ─────────────────────────────────────────────────────────

#[derive(Clone)]
pub struct FriendContext {
    pub friend_room_service: Arc<synapse_services::friend_room_service::models::FriendRoomService>,
    pub validator: Arc<synapse_common::validation::Validator>,
    pub token_auth: Arc<dyn synapse_services::auth::TokenAuth>,
    pub credential_auth: Arc<dyn synapse_services::auth::CredentialAuth>,
    pub room_auth: Arc<dyn synapse_services::auth::RoomAuth>,
    pub server_name: String,
    pub cache: Arc<CacheManager>,
    pub config: synapse_common::config::Config,
    pub user_storage: Arc<dyn synapse_storage::UserStore>,
    pub user_service: Arc<synapse_services::UserService>,
    pub room_service: Arc<dyn synapse_services::RoomServiceApi>,
    pub admin_audit_service: Option<Arc<synapse_services::AdminAuditService>>,
    pub account_identity_service: Arc<synapse_services::account_identity_service::AccountIdentityService>,
    pub federation_client: Arc<dyn synapse_federation::client_api::FederationClientApi>,
}

impl FromRef<AppState> for FriendContext {
    fn from_ref(state: &AppState) -> Self {
        Self {
            friend_room_service: state.services.extensions.friend_room_service.clone(),
            validator: state.services.core.validator.clone(),
            token_auth: state.services.core.token_auth.clone(),
            credential_auth: state.services.core.credential_auth.clone(),
            room_auth: state.services.core.room_auth.clone(),
            server_name: state.services.core.server_name.clone(),
            cache: state.cache.clone(),
            config: state.services.core.config.clone(),
            user_storage: state.services.account.user_storage.clone(),
            user_service: state.services.account.user_service.clone(),
            room_service: state.services.rooms.room_service.clone(),
            admin_audit_service: state.services.admin.security.admin_audit_service.clone().into(),
            account_identity_service: state.services.account.account_identity_service.clone(),
            federation_client: state.services.federation.federation_client.clone(),
        }
    }
}
