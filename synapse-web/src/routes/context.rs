use crate::routes::AppState;
use axum::extract::FromRef;
use std::collections::HashMap;
use std::sync::Arc;
use synapse_cache::{CacheManager, FederationSignatureCache};
use synapse_common::rate_limit_config::RateLimitConfigManager;
use tokio::sync::{Mutex, RwLock, Semaphore};

// ── CoreContext ───────────────────────────────────────────────────────────

/// Minimal context for the global request-pipeline middlewares (auth, shadow-ban,
/// csrf, rate-limit). Carries only the shared services those middlewares read.
#[derive(Clone)]
pub struct CoreContext {
    /// The `validator` field.
    pub validator: Arc<synapse_common::validation::Validator>,
    /// The `token_auth` field.
    pub token_auth: Arc<dyn synapse_services::auth::TokenAuth>,
    /// The `credential_auth` field.
    pub credential_auth: Arc<dyn synapse_services::auth::CredentialAuth>,
    /// The `room_auth` field.
    pub room_auth: Arc<dyn synapse_services::auth::RoomAuth>,
    /// The `config` field.
    pub config: Arc<synapse_common::config::Config>,
    /// The `cache` field.
    pub cache: Arc<CacheManager>,
    /// The `rate_limit_config_manager` field.
    pub rate_limit_config_manager: Option<Arc<RateLimitConfigManager>>,
    /// B-4: Paths auto-derived from the route ledger that the rate limit
    /// middleware should skip (sync/sliding-sync endpoints with their own
    /// per-user+device rate limiting).
    pub rate_limit_exempt_paths: Arc<Vec<&'static str>>,
    /// W7+: 补上与其它 context（Room/Sync/Media/...）一致的 metrics 句柄，
    /// 供 `rate_limit_middleware` 发射限流指标。此前 CoreContext 是唯一
    /// 缺该字段的 context。
    pub metrics: Arc<synapse_common::metrics::MetricsCollector>,
}

impl CoreContext {
    /// Mirror of `AppState::rate_limit_config` so rate_limit_middleware keeps identical behavior.
    pub fn rate_limit_config(&self) -> Option<synapse_common::RateLimitConfigFile> {
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
            rate_limit_exempt_paths: state.rate_limit_exempt_paths.clone(),
            metrics: state.services.core.metrics.clone(),
        }
    }
}

// ── RoomContext ───────────────────────────────────────────────────────────

/// The `RoomContext` struct.
#[derive(Clone)]
pub struct RoomContext {
    /// The `room_service` field.
    pub room_service: Arc<dyn synapse_services::room::RoomServiceApi>,
    /// The `validator` field.
    pub validator: Arc<synapse_common::validation::Validator>,
    /// The `token_auth` field.
    pub token_auth: Arc<dyn synapse_services::auth::TokenAuth>,
    /// The `credential_auth` field.
    pub credential_auth: Arc<dyn synapse_services::auth::CredentialAuth>,
    /// The `room_auth` field.
    pub room_auth: Arc<dyn synapse_services::auth::RoomAuth>,
    /// The `server_name` field.
    pub server_name: String,
    /// The `cache` field.
    pub cache: Arc<CacheManager>,
    /// The `sync_service` field.
    pub sync_service: Arc<dyn synapse_services::sync_service::SyncServiceApi>,
    /// The `thread_service` field.
    pub thread_service: Arc<synapse_services::thread_service::ThreadService>,
    /// The `space_service` field.
    pub space_service: Arc<synapse_services::room::space::SpaceService>,
    /// The `room_summary_service` field.
    pub room_summary_service: Arc<synapse_services::room::summary::RoomSummaryService>,
    /// The `account_data_service` field.
    pub account_data_service: Arc<synapse_services::account_data_service::AccountDataService>,
    /// The `search_service` field.
    pub search_service: Arc<synapse_services::search_service::SearchService>,
    /// The `retention_service` field.
    pub retention_service: Arc<synapse_services::retention_service::RetentionService>,
    /// The `translation_service` field.
    pub translation_service: Arc<synapse_services::translation_service::TranslationService>,
    /// The `federation_client` field.
    pub federation_client: Arc<dyn synapse_federation::client_api::FederationClientApi>,
    /// The `rtc_domain_service` field.
    pub rtc_domain_service: Arc<synapse_services::rtc::RtcDomainService>,
    /// The `e2ee_backup_service` field.
    pub e2ee_backup_service: synapse_e2ee::backup::KeyBackupService,
    /// The `config` field.
    pub config: Arc<synapse_common::config::Config>,
    /// The `admin_audit_service` field.
    pub admin_audit_service: Option<Arc<synapse_services::admin::AdminAuditService>>,
    /// The `account_identity_service` field.
    pub account_identity_service: Arc<synapse_services::account_identity_service::AccountIdentityService>,
    /// The `account_device_list_service` field.
    pub account_device_list_service: Arc<synapse_services::account_device_list_service::AccountDeviceListService>,
    /// The `push_notification_service` field.
    pub push_notification_service: Arc<synapse_services::push_notification_service::PushNotificationService>,
    /// The `event_broadcaster` field.
    pub event_broadcaster: Arc<synapse_federation::EventBroadcaster>,
    /// The `cross_signing_service` field.
    pub cross_signing_service: synapse_e2ee::cross_signing::CrossSigningService,
    #[cfg(feature = "friends")]
    /// The `friend_room_service` field.
    pub friend_room_service: Arc<synapse_services::friend_room_service::models::FriendRoomService>,
    /// The `metrics` field.
    pub metrics: Arc<synapse_common::metrics::MetricsCollector>,
    #[cfg(feature = "beacons")]
    /// The `beacon_service` field.
    pub beacon_service: Arc<synapse_services::beacon_service::BeaconService>,
    /// The `presence_service` field.
    pub presence_service: Arc<synapse_services::presence_service::PresenceService>,
    /// The `typing_service` field.
    pub typing_service: Arc<synapse_services::typing_service::TypingService>,
    /// The `relations_service` field.
    pub relations_service: Arc<synapse_services::relations_service::RelationsService>,
    #[cfg(feature = "voice-extended")]
    /// The `voice_service` field.
    pub voice_service: Arc<synapse_services::voice_service::VoiceService>,
    /// The `ssss_service` field.
    pub ssss_service: synapse_e2ee::ssss::SecretStorageService,
    /// The `dehydrated_device_service` field.
    pub dehydrated_device_service: Arc<synapse_services::dehydrated_device_service::DehydratedDeviceService>,
    #[cfg(feature = "burn-after-read")]
    /// The `burn_after_read` field.
    pub burn_after_read: Arc<synapse_services::burn_after_read_service::BurnAfterReadService>,
    /// MSC4140 — Delayed event storage for scheduling cancellable delayed messages.
    pub delayed_event_service: Arc<synapse_services::delayed_event_service::DelayedEventService>,
    /// The `app_service_manager` field.
    pub app_service_manager: Arc<synapse_services::application_service::ApplicationServiceManager>,
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
            #[cfg(feature = "friends")]
            friend_room_service: state.services.extensions.friend_room_service.clone(),
            metrics: state.services.core.metrics.clone(),
            #[cfg(feature = "beacons")]
            beacon_service: state.services.rooms.beacon_service.clone(),
            presence_service: state.services.account.presence_service.clone(),
            typing_service: state.services.rooms.typing_service.clone(),
            relations_service: state.services.rooms.relations_service.clone(),
            #[cfg(feature = "voice-extended")]
            voice_service: Arc::new(state.services.extensions.voice_service.clone()),
            ssss_service: state.services.e2ee.ssss_service.clone(),
            dehydrated_device_service: Arc::new(state.services.e2ee.dehydrated_device_service.clone()),
            #[cfg(feature = "burn-after-read")]
            burn_after_read: state.services.extensions.burn_after_read.clone(),
            delayed_event_service: state.services.admin.modules.delayed_event_service.clone(),
            app_service_manager: state.services.admin.modules.app_service_manager.clone(),
        }
    }
}

// ── E2eeRoomContext ───────────────────────────────────────────────────────

/// The `E2eeRoomContext` struct.
#[derive(Clone)]
pub struct E2eeRoomContext {
    /// The `room_service` field.
    pub room_service: Arc<dyn synapse_services::room::RoomServiceApi>,
    /// The `e2ee_backup_service` field.
    pub e2ee_backup_service: synapse_e2ee::backup::KeyBackupService,
    /// The `secure_backup_service` field.
    pub secure_backup_service: synapse_e2ee::secure_backup::SecureBackupService,
    /// The `validator` field.
    pub validator: Arc<synapse_common::validation::Validator>,
    /// The `token_auth` field.
    pub token_auth: Arc<dyn synapse_services::auth::TokenAuth>,
    /// The `credential_auth` field.
    pub credential_auth: Arc<dyn synapse_services::auth::CredentialAuth>,
    /// The `room_auth` field.
    pub room_auth: Arc<dyn synapse_services::auth::RoomAuth>,
    /// The `admin_audit_service` field.
    pub admin_audit_service: Option<Arc<synapse_services::admin::AdminAuditService>>,
    /// The `pool` field.
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

/// The `SyncContext` struct.
#[derive(Clone)]
pub struct SyncContext {
    /// The `sync_service` field.
    pub sync_service: Arc<dyn synapse_services::sync_service::SyncServiceApi>,
    /// The `validator` field.
    pub validator: Arc<synapse_common::validation::Validator>,
    /// The `token_auth` field.
    pub token_auth: Arc<dyn synapse_services::auth::TokenAuth>,
    /// The `credential_auth` field.
    pub credential_auth: Arc<dyn synapse_services::auth::CredentialAuth>,
    /// The `room_auth` field.
    pub room_auth: Arc<dyn synapse_services::auth::RoomAuth>,
    /// The `user_service` field.
    pub user_service: Arc<synapse_services::account::UserService>,
    /// The `cache` field.
    pub cache: Arc<CacheManager>,
    /// The `config` field.
    pub config: Arc<synapse_common::config::Config>,
    /// The `rate_limit_config_manager` field.
    pub rate_limit_config_manager: Option<Arc<RateLimitConfigManager>>,
    /// The `admin_audit_service` field.
    pub admin_audit_service: Option<Arc<synapse_services::admin::AdminAuditService>>,
    /// The `metrics` field.
    pub metrics: Arc<synapse_common::metrics::MetricsCollector>,
    /// The `sliding_sync_service` field.
    pub sliding_sync_service: Arc<synapse_services::sliding_sync_service::SlidingSyncService>,
    /// The `client_push_service` field.
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
    /// See [`sync_rate_limit_override`].
    pub fn sync_rate_limit_override(&self) -> Option<crate::routes::state::SyncRateLimitOverride> {
        self.rate_limit_config_manager.as_ref().map(|manager| {
            let config = manager.get_config();
            crate::routes::state::SyncRateLimitOverride {
                fail_open_on_error: config.fail_open_on_error,
                sync: config.sync,
            }
        })
    }
}

// ── DeviceContext ─────────────────────────────────────────────────────────

/// The `DeviceContext` struct.
#[derive(Clone)]
pub struct DeviceContext {
    /// The `device_storage` field.
    pub device_storage: Arc<dyn synapse_storage::device::DeviceListStoreApi>,
    /// The `validator` field.
    pub validator: Arc<synapse_common::validation::Validator>,
    /// The `token_auth` field.
    pub token_auth: Arc<dyn synapse_services::auth::TokenAuth>,
    /// The `credential_auth` field.
    pub credential_auth: Arc<dyn synapse_services::auth::CredentialAuth>,
    /// The `room_auth` field.
    pub room_auth: Arc<dyn synapse_services::auth::RoomAuth>,
    /// The `user_service` field.
    pub user_service: Arc<synapse_services::account::UserService>,
    /// The `server_name` field.
    pub server_name: String,
    /// The `account_device_list_service` field.
    pub account_device_list_service: Arc<synapse_services::account_device_list_service::AccountDeviceListService>,
    /// The `room_service` field.
    pub room_service: Arc<dyn synapse_services::room::RoomServiceApi>,
    /// The `uia_service` field.
    pub uia_service: Arc<synapse_services::uia_service::UiaService>,
    /// The `event_broadcaster` field.
    pub event_broadcaster: Arc<synapse_federation::EventBroadcaster>,
    /// The `config` field.
    pub config: Arc<synapse_common::config::Config>,
    /// The `admin_audit_service` field.
    pub admin_audit_service: Option<Arc<synapse_services::admin::AdminAuditService>>,
    /// The `account_identity_service` field.
    pub account_identity_service: Arc<synapse_services::account_identity_service::AccountIdentityService>,
    /// The `cross_signing_service` field.
    pub cross_signing_service: synapse_e2ee::cross_signing::CrossSigningService,
    /// The `device_keys_service` field.
    pub device_keys_service: synapse_e2ee::device_keys::DeviceKeyService,
    /// The `federation_client` field.
    pub federation_client: Arc<dyn synapse_federation::client_api::FederationClientApi>,
    /// The `to_device_service` field.
    pub to_device_service: synapse_e2ee::to_device::ToDeviceService,
    /// The `metrics` field.
    pub metrics: Arc<synapse_common::metrics::MetricsCollector>,
    /// The `cache` field.
    pub cache: Arc<CacheManager>,
    /// The `event_notifier` field.
    pub event_notifier: synapse_services::event_notifier::EventNotifier,
    /// The `key_request_service` field.
    pub key_request_service: synapse_e2ee::key_request::KeyRequestService,
    /// The `verification_service` field.
    pub verification_service: synapse_e2ee::verification::VerificationService,
    /// The `device_trust_service` field.
    pub device_trust_service: synapse_e2ee::device_trust::DeviceTrustService,
    /// The `key_rotation_service` field.
    pub key_rotation_service: Arc<synapse_services::infra::FederationKeyRotationService>,
}

impl FromRef<AppState> for DeviceContext {
    fn from_ref(state: &AppState) -> Self {
        Self {
            device_storage: state.services.account.device_storage.clone(),
            validator: state.services.core.validator.clone(),
            token_auth: state.services.core.token_auth.clone(),
            credential_auth: state.services.core.credential_auth.clone(),
            room_auth: state.services.core.room_auth.clone(),
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

/// The `AuthContext` struct.
#[derive(Clone)]
pub struct AuthContext {
    /// The `validator` field.
    pub validator: Arc<synapse_common::validation::Validator>,
    /// The `token_auth` field.
    pub token_auth: Arc<dyn synapse_services::auth::TokenAuth>,
    /// The `credential_auth` field.
    pub credential_auth: Arc<dyn synapse_services::auth::CredentialAuth>,
    /// The `room_auth` field.
    pub room_auth: Arc<dyn synapse_services::auth::RoomAuth>,
    /// The `registration_service` field.
    pub registration_service: Arc<synapse_services::registration_service::RegistrationService>,
    /// The `user_service` field.
    pub user_service: Arc<synapse_services::account::UserService>,
    /// The `server_name` field.
    pub server_name: String,
    /// The `cache` field.
    pub cache: Arc<CacheManager>,
    /// The `config` field.
    pub config: Arc<synapse_common::config::Config>,
    /// The `admin_audit_service` field.
    pub admin_audit_service: Option<Arc<synapse_services::admin::AdminAuditService>>,
    /// The `account_identity_service` field.
    pub account_identity_service: Arc<synapse_services::account_identity_service::AccountIdentityService>,
    /// The `uia_service` field.
    pub uia_service: Arc<synapse_services::uia_service::UiaService>,
    /// The `federation_client` field.
    pub federation_client: Arc<dyn synapse_federation::client_api::FederationClientApi>,
    /// The `email_verification_storage` field.
    pub email_verification_storage: Arc<synapse_storage::email_verification::EmailVerificationStorage>,
    /// The `account_device_list_service` field.
    pub account_device_list_service: Arc<synapse_services::account_device_list_service::AccountDeviceListService>,
    /// The `refresh_token_service` field.
    pub refresh_token_service: Arc<synapse_services::refresh_token_service::RefreshTokenService>,
    /// The `metrics` field.
    pub metrics: Arc<synapse_common::metrics::MetricsCollector>,
    /// The `identity_service` field.
    pub identity_service: Arc<synapse_services::identity::IdentityService>,
    /// The `oidc_service` field.
    pub oidc_service: Option<Arc<synapse_services::oidc_service::OidcService>>,
    #[cfg(feature = "builtin-oidc")]
    /// The `builtin_oidc_provider` field.
    pub builtin_oidc_provider: Option<Arc<synapse_services::builtin_oidc_provider::BuiltinOidcProvider>>,
    /// The `rendezvous_service` field.
    pub rendezvous_service: Arc<synapse_services::rendezvous_service::RendezvousService>,
    /// The `login_token_service` field.
    pub login_token_service: Arc<synapse_services::login_token_service::LoginTokenService>,
}

impl FromRef<AppState> for AuthContext {
    fn from_ref(state: &AppState) -> Self {
        Self {
            validator: state.services.core.validator.clone(),
            token_auth: state.services.core.token_auth.clone(),
            credential_auth: state.services.core.credential_auth.clone(),
            room_auth: state.services.core.room_auth.clone(),
            registration_service: state.services.core.registration_service.clone(),
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
            rendezvous_service: state.services.admin.modules.rendezvous_service.clone(),
            login_token_service: state.services.admin.modules.login_token_service.clone(),
        }
    }
}

// ── AdminContext ──────────────────────────────────────────────────────────

/// The `AdminContext` struct.
#[derive(Clone)]
pub struct AdminContext {
    // Core
    /// The `validator` field.
    pub validator: Arc<synapse_common::validation::Validator>,
    /// The `token_auth` field.
    pub token_auth: Arc<dyn synapse_services::auth::TokenAuth>,
    /// The `credential_auth` field.
    pub credential_auth: Arc<dyn synapse_services::auth::CredentialAuth>,
    /// The `room_auth` field.
    pub room_auth: Arc<dyn synapse_services::auth::RoomAuth>,
    /// The `registration_service` field.
    pub registration_service: Arc<synapse_services::registration_service::RegistrationService>,
    /// The `config` field.
    pub config: Arc<synapse_common::config::Config>,
    /// The `server_name` field.
    pub server_name: String,
    /// The `cache` field.
    pub cache: Arc<CacheManager>,
    /// The `metrics` field.
    pub metrics: Arc<synapse_common::metrics::MetricsCollector>,
    /// The `media_service` field.
    pub media_service: synapse_services::media_service::MediaService,
    // Room & sync
    /// The `room_service` field.
    pub room_service: Arc<dyn synapse_services::room::RoomServiceApi>,
    /// The `sliding_sync_service` field.
    pub sliding_sync_service: Arc<synapse_services::sliding_sync_service::SlidingSyncService>,
    /// The `space_service` field.
    pub space_service: Arc<synapse_services::room::space::SpaceService>,
    // Account
    /// The `user_service` field.
    pub user_service: Arc<synapse_services::account::UserService>,
    /// The `account_identity_service` field.
    pub account_identity_service: Arc<synapse_services::account_identity_service::AccountIdentityService>,
    /// The `account_device_list_service` field.
    pub account_device_list_service: Arc<synapse_services::account_device_list_service::AccountDeviceListService>,
    /// The `invite_blocklist_storage` field.
    pub invite_blocklist_storage: Arc<synapse_storage::invite_blocklist::InviteBlocklistStorage>,
    // Admin — user
    /// The `admin_user_service` field.
    pub admin_user_service: Arc<synapse_services::admin_user_service::AdminUserService>,
    /// The `admin_registration_service` field.
    pub admin_registration_service: synapse_services::admin_registration_service::AdminRegistrationService,
    /// The `admin_token_service` field.
    pub admin_token_service: Arc<synapse_services::admin_token_service::AdminTokenService>,
    /// The `refresh_token_service` field.
    pub refresh_token_service: Arc<synapse_services::refresh_token_service::RefreshTokenService>,
    /// The `registration_token_service` field.
    pub registration_token_service: Arc<synapse_services::registration_token_service::RegistrationTokenService>,
    /// The `email_verification_storage` field.
    pub email_verification_storage: Arc<synapse_storage::email_verification::EmailVerificationStorage>,
    // Admin — modules
    /// The `background_update_service` field.
    pub background_update_service: Arc<synapse_services::background_update_service::BackgroundUpdateService>,
    /// The `retention_service` field.
    pub retention_service: Arc<synapse_services::retention_service::RetentionService>,
    /// The `feature_flag_service` field.
    pub feature_flag_service: Arc<synapse_services::feature_flag_service::FeatureFlagService>,
    /// The `event_report_service` field.
    pub event_report_service: Arc<synapse_services::event_report_service::EventReportService>,
    /// MSC4140 — Cancellable delayed events storage.
    pub delayed_event_service: Arc<synapse_services::delayed_event_service::DelayedEventService>,
    /// MSC4284 — Policy server service for room/user/content moderation.
    pub policy_service: Arc<synapse_services::policy_service::PolicyService>,
    /// Event storage for admin redact/purge operations.
    pub event_storage: synapse_storage::event::EventStorage,
    /// The `push_notification_service` field.
    pub push_notification_service: Arc<synapse_services::push_notification_service::PushNotificationService>,
    /// The `app_service_manager` field.
    pub app_service_manager: Arc<synapse_services::application_service::ApplicationServiceManager>,
    /// The `app_service_scheduler` field.
    pub app_service_scheduler: Arc<synapse_services::application_service::ApplicationServiceScheduler>,
    /// The `module_service` field.
    pub module_service: Arc<synapse_services::module_service::ModuleService>,
    /// The `module_storage` field.
    pub module_storage: Arc<synapse_storage::module::ModuleStorage>,
    /// The `account_validity_service` field.
    pub account_validity_service: Arc<synapse_services::module_service::AccountValidityService>,
    /// The `worker_manager` field.
    pub worker_manager: Arc<synapse_services::worker::WorkerManager>,
    // Admin — security
    /// The `admin_audit_service` field.
    pub admin_audit_service: Arc<synapse_services::admin::AdminAuditService>,
    /// The `admin_security_service` field.
    pub admin_security_service: Arc<synapse_services::admin_security_service::AdminSecurityService>,
    /// The `admin_server_service` field.
    pub admin_server_service: Arc<synapse_services::admin_server_service::AdminServerService>,
    /// The `captcha_service` field.
    pub captcha_service: Arc<synapse_services::captcha_service::CaptchaService>,
    /// The `telemetry_alert_service` field.
    pub telemetry_alert_service: Arc<synapse_services::telemetry_service::TelemetryAlertService>,
    // Admin — federation
    /// The `admin_federation_service` field.
    pub admin_federation_service: Arc<synapse_services::admin_federation_service::AdminFederationService>,
    /// The `federation_blacklist_service` field.
    pub federation_blacklist_service: Arc<synapse_services::federation_blacklist_service::FederationBlacklistService>,
    // Admin — media
    /// The `admin_media_service` field.
    pub admin_media_service: Arc<synapse_services::admin_media_service::AdminMediaService>,
    // Cross-cutting
    /// The `federation_client` field.
    pub federation_client: Arc<dyn synapse_federation::client_api::FederationClientApi>,
    #[cfg(feature = "server-notifications")]
    /// The `server_notification_service` field.
    pub server_notification_service: Arc<synapse_services::server_notification_service::ServerNotificationService>,
    /// The `rate_limit_config_manager` field.
    pub rate_limit_config_manager: Option<Arc<RateLimitConfigManager>>,
    /// The `shutdown_signal` field.
    pub shutdown_signal: Option<tokio::sync::broadcast::Sender<()>>,
    /// The `account_data_service` field.
    pub account_data_service: Arc<synapse_services::account_data_service::AccountDataService>,
    /// The `health_checker` field.
    pub health_checker: Arc<synapse_common::health::HealthChecker>,
    #[cfg(feature = "friends")]
    /// The `friend_room_service` field.
    pub friend_room_service: Arc<synapse_services::friend_room_service::models::FriendRoomService>,
    /// The `ssss_service` field.
    pub ssss_service: synapse_e2ee::ssss::SecretStorageService,
    /// The `token_storage` field.
    pub token_storage: Arc<dyn synapse_storage::token::AccessTokenStoreApi>,
    /// The `client_push_service` field.
    pub client_push_service: Arc<synapse_services::client_push_service::ClientPushService>,
    #[cfg(feature = "widgets")]
    /// The `widget_service` field.
    pub widget_service: Arc<synapse_services::widget_service::WidgetService>,
    #[cfg(feature = "external-services")]
    /// The `external_service_integration` field.
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
            delayed_event_service: state.services.admin.modules.delayed_event_service.clone(),
            policy_service: state.services.admin.modules.policy_service.clone(),
            event_storage: synapse_storage::event::EventStorage::new(
                &state.services.database_pool(),
                state.services.core.server_name.clone(),
            ),
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
            federation_client: state.services.federation.federation_client.clone(),
            #[cfg(feature = "server-notifications")]
            server_notification_service: state.services.extensions.server_notification_service.clone(),
            rate_limit_config_manager: state.rate_limit_config_manager().cloned(),
            shutdown_signal: state.shutdown_signal.clone(),
            account_data_service: state.services.core.account_data_service.clone(),
            health_checker: state.health_checker.clone(),
            #[cfg(feature = "friends")]
            friend_room_service: state.services.extensions.friend_room_service.clone(),
            ssss_service: state.services.e2ee.ssss_service.clone(),
            token_storage: state.services.account.token_storage.clone(),
            client_push_service: state.services.core.client_push_service.clone(),
            #[cfg(feature = "widgets")]
            widget_service: state.services.extensions.widget_service.clone(),
            #[cfg(feature = "external-services")]
            external_service_integration: state.services.admin.modules.external_service_integration.clone(),
        }
    }
}

// ── FederationContext ─────────────────────────────────────────────────────

/// The `FederationContext` struct.
#[derive(Clone)]
pub struct FederationContext {
    /// The `validator` field.
    pub validator: Arc<synapse_common::validation::Validator>,
    /// The `token_auth` field.
    pub token_auth: Arc<dyn synapse_services::auth::TokenAuth>,
    /// The `credential_auth` field.
    pub credential_auth: Arc<dyn synapse_services::auth::CredentialAuth>,
    /// The `room_auth` field.
    pub room_auth: Arc<dyn synapse_services::auth::RoomAuth>,
    /// The `user_service` field.
    pub user_service: Arc<synapse_services::account::UserService>,
    /// The `config` field.
    pub config: Arc<synapse_common::config::Config>,
    /// The `server_name` field.
    pub server_name: String,
    /// The `cache` field.
    pub cache: Arc<CacheManager>,
    /// The `metrics` field.
    pub metrics: Arc<synapse_common::metrics::MetricsCollector>,
    /// The `room_service` field.
    pub room_service: Arc<dyn synapse_services::room::RoomServiceApi>,
    /// The `space_service` field.
    pub space_service: Arc<synapse_services::room::space::SpaceService>,
    /// The `registration_service` field.
    pub registration_service: Arc<synapse_services::registration_service::RegistrationService>,
    /// The `account_identity_service` field.
    pub account_identity_service: Arc<synapse_services::account_identity_service::AccountIdentityService>,
    /// The `account_device_list_service` field.
    pub account_device_list_service: Arc<synapse_services::account_device_list_service::AccountDeviceListService>,
    /// The `key_rotation_manager` field.
    pub key_rotation_manager: synapse_federation::KeyRotationManager,
    /// The `federation_client` field.
    pub federation_client: Arc<dyn synapse_federation::client_api::FederationClientApi>,
    /// The `event_auth_chain` field.
    pub event_auth_chain: synapse_federation::EventAuthChain,
    /// The `device_sync_manager` field.
    pub device_sync_manager: synapse_federation::DeviceSyncManager,
    /// The `federation_server_name` field.
    pub federation_server_name: String,
    /// The `admin_audit_service` field.
    pub admin_audit_service: Option<Arc<synapse_services::admin::AdminAuditService>>,
    /// The `worker_manager` field.
    pub worker_manager: Arc<synapse_services::worker::WorkerManager>,
    /// The `media_service` field.
    pub media_service: synapse_services::media_service::MediaService,
    /// The `account_data_service` field.
    pub account_data_service: Arc<synapse_services::account_data_service::AccountDataService>,
    /// The `federation_signature_cache` field.
    pub federation_signature_cache: Arc<FederationSignatureCache>,
    /// S1 修复：联邦重放保护缓存。
    pub replay_protection_cache: Arc<synapse_common::security::ReplayProtectionCache>,
    /// The `federation_key_fetch_general_semaphore` field.
    pub federation_key_fetch_general_semaphore: Arc<Semaphore>,
    /// The `federation_key_fetch_priority_semaphore` field.
    pub federation_key_fetch_priority_semaphore: Arc<Semaphore>,
    /// The `admin_federation_service` field.
    pub admin_federation_service: Arc<synapse_services::admin_federation_service::AdminFederationService>,
    /// The `device_keys_service` field.
    pub device_keys_service: synapse_e2ee::device_keys::DeviceKeyService,
    /// The `cross_signing_service` field.
    pub cross_signing_service: synapse_e2ee::cross_signing::CrossSigningService,
    /// The `to_device_service` field.
    pub to_device_service: synapse_e2ee::to_device::ToDeviceService,
    /// The `presence_storage` field.
    pub presence_storage: Arc<dyn synapse_storage::presence::PresenceStoreApi>,
    /// The `device_storage` field.
    pub device_storage: Arc<dyn synapse_storage::device::DeviceListStoreApi>,
    /// The `federation_inbound_edu_semaphore` field.
    pub federation_inbound_edu_semaphore: Arc<Semaphore>,
    /// The `federation_inbound_edu_origin_semaphores` field.
    pub federation_inbound_edu_origin_semaphores: Arc<Mutex<HashMap<String, Arc<Semaphore>>>>,
    /// The `federation_presence_backoff_until` field.
    pub federation_presence_backoff_until: Arc<RwLock<HashMap<String, i64>>>,
    /// The `federation_join_semaphore` field.
    pub federation_join_semaphore: Arc<Semaphore>,
}

impl FromRef<AppState> for FederationContext {
    fn from_ref(state: &AppState) -> Self {
        Self {
            validator: state.services.core.validator.clone(),
            token_auth: state.services.core.token_auth.clone(),
            credential_auth: state.services.core.credential_auth.clone(),
            room_auth: state.services.core.room_auth.clone(),
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
            replay_protection_cache: state.replay_protection_cache.clone(),
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

/// The `MediaContext` struct.
#[derive(Clone)]
pub struct MediaContext {
    /// The `validator` field.
    pub validator: Arc<synapse_common::validation::Validator>,
    /// The `token_auth` field.
    pub token_auth: Arc<dyn synapse_services::auth::TokenAuth>,
    /// The `credential_auth` field.
    pub credential_auth: Arc<dyn synapse_services::auth::CredentialAuth>,
    /// The `room_auth` field.
    pub room_auth: Arc<dyn synapse_services::auth::RoomAuth>,
    /// The `user_service` field.
    pub user_service: Arc<synapse_services::account::UserService>,
    /// The `config` field.
    pub config: Arc<synapse_common::config::Config>,
    /// The `server_name` field.
    pub server_name: String,
    /// The `cache` field.
    pub cache: Arc<CacheManager>,
    /// The `media_service` field.
    pub media_service: synapse_services::media_service::MediaService,
    /// The `media_domain_service` field.
    pub media_domain_service: Arc<synapse_services::media::MediaDomainService>,
    /// The `room_service` field.
    pub room_service: Arc<dyn synapse_services::room::RoomServiceApi>,
    /// The `federation_client` field.
    pub federation_client: Arc<dyn synapse_federation::client_api::FederationClientApi>,
    /// The `account_identity_service` field.
    pub account_identity_service: Arc<synapse_services::account_identity_service::AccountIdentityService>,
    /// The `admin_audit_service` field.
    pub admin_audit_service: Option<Arc<synapse_services::admin::AdminAuditService>>,
}

impl FromRef<AppState> for MediaContext {
    fn from_ref(state: &AppState) -> Self {
        Self {
            validator: state.services.core.validator.clone(),
            token_auth: state.services.core.token_auth.clone(),
            credential_auth: state.services.core.credential_auth.clone(),
            room_auth: state.services.core.room_auth.clone(),
            user_service: state.services.account.user_service.clone(),
            config: state.services.core.config.clone(),
            server_name: state.services.core.server_name.clone(),
            cache: state.cache.clone(),
            media_service: state.services.core.media_service.clone(),
            media_domain_service: state.services.extensions.media_domain_service.clone(),
            room_service: state.services.rooms.room_service.clone(),
            federation_client: state.services.federation.federation_client.clone(),
            account_identity_service: state.services.account.account_identity_service.clone(),
            admin_audit_service: state.services.admin.security.admin_audit_service.clone().into(),
        }
    }
}

// ── SsoContext ────────────────────────────────────────────────────────────

/// The `SsoContext` struct.
#[derive(Clone)]
pub struct SsoContext {
    /// The `validator` field.
    pub validator: Arc<synapse_common::validation::Validator>,
    /// The `token_auth` field.
    pub token_auth: Arc<dyn synapse_services::auth::TokenAuth>,
    /// The `credential_auth` field.
    pub credential_auth: Arc<dyn synapse_services::auth::CredentialAuth>,
    /// The `room_auth` field.
    pub room_auth: Arc<dyn synapse_services::auth::RoomAuth>,
    /// The `config` field.
    pub config: Arc<synapse_common::config::Config>,
    /// The `server_name` field.
    pub server_name: String,
    /// The `cache` field.
    pub cache: Arc<CacheManager>,
    /// The `registration_service` field.
    pub registration_service: Arc<synapse_services::registration_service::RegistrationService>,
    /// The `user_service` field.
    pub user_service: Arc<synapse_services::account::UserService>,
    /// The `account_identity_service` field.
    pub account_identity_service: Arc<synapse_services::account_identity_service::AccountIdentityService>,
    /// The `account_device_list_service` field.
    pub account_device_list_service: Arc<synapse_services::account_device_list_service::AccountDeviceListService>,
    #[cfg(feature = "saml-sso")]
    /// The `saml_service` field.
    pub saml_service: Arc<synapse_services::saml_service::SamlService>,
    #[cfg(feature = "cas-sso")]
    /// The `cas_service` field.
    pub cas_service: Arc<synapse_services::cas_service::CasService>,
    /// The `oidc_service` field.
    pub oidc_service: Option<Arc<synapse_services::oidc_service::OidcService>>,
    /// The `oidc_mapping_storage` field.
    pub oidc_mapping_storage: Arc<dyn synapse_storage::oidc_user_mapping::OidcUserMappingStoreApi>,
    /// The `oidc_session_service` field.
    pub oidc_session_service: Arc<synapse_services::oidc_session_service::OidcSessionService>,
    #[cfg(feature = "builtin-oidc")]
    /// The `builtin_oidc_provider` field.
    pub builtin_oidc_provider: Option<Arc<synapse_services::builtin_oidc_provider::BuiltinOidcProvider>>,
    #[cfg(not(feature = "builtin-oidc"))]
    /// The `builtin_oidc_provider` field.
    pub builtin_oidc_provider: Option<()>,
    /// The `admin_audit_service` field.
    pub admin_audit_service: Option<Arc<synapse_services::admin::AdminAuditService>>,
    /// The `refresh_token_service` field.
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
            user_service: state.services.account.user_service.clone(),
            account_identity_service: state.services.account.account_identity_service.clone(),
            account_device_list_service: state.services.account.account_device_list_service.clone(),
            #[cfg(feature = "saml-sso")]
            saml_service: state.services.sso.saml_service.clone(),
            #[cfg(feature = "cas-sso")]
            cas_service: state.services.sso.cas_service.clone(),
            oidc_service: state.services.sso.oidc_service.clone(),
            oidc_mapping_storage: state.services.sso.oidc_mapping_storage.clone(),
            oidc_session_service: state.services.sso.oidc_session_service.clone(),
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

/// The `FriendContext` struct.
#[cfg(feature = "friends")]
#[derive(Clone)]
pub struct FriendContext {
    /// The `friend_room_service` field.
    pub friend_room_service: Arc<synapse_services::friend_room_service::models::FriendRoomService>,
    /// The `validator` field.
    pub validator: Arc<synapse_common::validation::Validator>,
    /// The `token_auth` field.
    pub token_auth: Arc<dyn synapse_services::auth::TokenAuth>,
    /// The `credential_auth` field.
    pub credential_auth: Arc<dyn synapse_services::auth::CredentialAuth>,
    /// The `room_auth` field.
    pub room_auth: Arc<dyn synapse_services::auth::RoomAuth>,
    /// The `server_name` field.
    pub server_name: String,
    /// The `cache` field.
    pub cache: Arc<CacheManager>,
    /// The `config` field.
    pub config: Arc<synapse_common::config::Config>,
    /// The `user_service` field.
    pub user_service: Arc<synapse_services::account::UserService>,
    /// The `room_service` field.
    pub room_service: Arc<dyn synapse_services::room::RoomServiceApi>,
    /// The `admin_audit_service` field.
    pub admin_audit_service: Option<Arc<synapse_services::admin::AdminAuditService>>,
    /// The `account_identity_service` field.
    pub account_identity_service: Arc<synapse_services::account_identity_service::AccountIdentityService>,
    /// The `federation_client` field.
    pub federation_client: Arc<dyn synapse_federation::client_api::FederationClientApi>,
}

#[cfg(feature = "friends")]
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
            user_service: state.services.account.user_service.clone(),
            room_service: state.services.rooms.room_service.clone(),
            admin_audit_service: state.services.admin.security.admin_audit_service.clone().into(),
            account_identity_service: state.services.account.account_identity_service.clone(),
            federation_client: state.services.federation.federation_client.clone(),
        }
    }
}
