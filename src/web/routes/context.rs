use crate::cache::CacheManager;
use crate::web::routes::AppState;
use axum::extract::FromRef;
use synapse_common::rate_limit_config::RateLimitConfigManager;
use std::sync::Arc;

/// Context for room-related handlers (create, join, leave, state, messages).
///
/// Contains all services commonly accessed by room route handlers, extracted
/// from `AppState` via `FromRef`. Handlers should use `State<RoomContext>`
/// instead of `State<AppState>` to make their dependencies explicit and
/// testable.
#[derive(Clone)]
pub struct RoomContext {
    pub room_service: Arc<synapse_services::room_service::RoomService>,
    pub auth_service: synapse_services::auth::AuthService,
    pub user_storage: Arc<dyn synapse_storage::UserStore>,
    pub server_name: String,
    pub cache: Arc<CacheManager>,
    pub sync_service: Arc<synapse_services::sync_service::SyncService>,
    pub thread_service: Arc<synapse_services::thread_service::ThreadService>,
    pub space_service: Arc<synapse_services::space_service::SpaceService>,
    pub room_summary_service: Arc<synapse_services::room_summary_service::RoomSummaryService>,
    pub account_data_service: Arc<synapse_services::account_data_service::AccountDataService>,
    pub search_service: Arc<synapse_services::search_service::SearchService>,
    pub retention_service: Arc<synapse_services::retention_service::RetentionService>,
    pub push_notification_service: Arc<synapse_services::push_notification_service::PushNotificationService>,
    pub translation_service: Arc<synapse_services::translation_service::TranslationService>,
    pub federation_client: Arc<synapse_federation::FederationClient>,
    pub account_device_list_service: Arc<synapse_services::account_device_list_service::AccountDeviceListService>,
    pub rtc_domain_service: Arc<synapse_services::rtc::RtcDomainService>,
    pub e2ee_backup_service: synapse_e2ee::backup::KeyBackupService,
    pub config: synapse_common::config::Config,
    #[cfg(feature = "beacons")]
    pub beacon_service: Arc<synapse_services::beacon_service::BeaconService>,
    #[cfg(feature = "friends")]
    pub friend_room_service: Arc<synapse_services::friend_room_service::FriendRoomService>,
}

impl FromRef<AppState> for RoomContext {
    fn from_ref(state: &AppState) -> Self {
        Self {
            room_service: state.services.rooms.room_service.clone(),
            auth_service: state.services.core.auth_service.clone(),
            user_storage: state.services.account.user_storage.clone(),
            server_name: state.services.core.server_name.clone(),
            cache: state.cache.clone(),
            sync_service: state.services.rooms.sync_service.clone(),
            thread_service: state.services.rooms.thread_service.clone(),
            space_service: state.services.rooms.space_service.clone(),
            room_summary_service: state.services.rooms.room_summary_service.clone(),
            account_data_service: state.services.core.account_data_service.clone(),
            search_service: state.services.core.search_service.clone(),
            retention_service: state.services.admin.modules.retention_service.clone(),
            push_notification_service: state.services.admin.modules.push_notification_service.clone(),
            translation_service: state.services.extensions.translation_service.clone(),
            federation_client: state.services.federation.federation_client.clone(),
            account_device_list_service: state.services.account.account_device_list_service.clone(),
            rtc_domain_service: state.services.extensions.rtc_domain_service.clone(),
            e2ee_backup_service: state.services.e2ee.backup_service.clone(),
            config: state.services.core.config.clone(),
            #[cfg(feature = "beacons")]
            beacon_service: state.services.rooms.beacon_service.clone(),
            #[cfg(feature = "friends")]
            friend_room_service: state.services.extensions.friend_room_service.clone(),
        }
    }
}

/// Context for sync handlers.
#[derive(Clone)]
pub struct SyncContext {
    pub sync_service: Arc<synapse_services::sync_service::SyncService>,
    pub auth_service: synapse_services::auth::AuthService,
    pub user_storage: Arc<dyn synapse_storage::UserStore>,
    pub cache: Arc<CacheManager>,
    pub config: synapse_common::config::Config,
    pub rate_limit_config_manager: Option<Arc<RateLimitConfigManager>>,
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
        }
    }
}

/// Context for device-related handlers.
#[derive(Clone)]
pub struct DeviceContext {
    pub device_storage: Arc<dyn synapse_storage::DeviceRepository>,
    pub auth_service: synapse_services::auth::AuthService,
    pub user_storage: Arc<dyn synapse_storage::UserStore>,
    pub server_name: String,
    pub account_device_list_service: Arc<synapse_services::account_device_list_service::AccountDeviceListService>,
    pub room_service: Arc<synapse_services::room_service::RoomService>,
    pub uia_service: Arc<synapse_services::uia_service::UiaService>,
    pub event_broadcaster: Arc<synapse_federation::EventBroadcaster>,
    pub config: synapse_common::config::Config,
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
        }
    }
}

/// Context for auth-related handlers (login, register, token refresh).
#[derive(Clone)]
pub struct AuthContext {
    pub auth_service: synapse_services::auth::AuthService,
    pub registration_service: Arc<synapse_services::registration_service::RegistrationService>,
    pub user_storage: Arc<dyn synapse_storage::UserStore>,
    pub server_name: String,
    pub cache: Arc<CacheManager>,
    pub config: synapse_common::config::Config,
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
        }
    }
}
