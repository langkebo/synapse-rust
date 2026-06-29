use crate::cache::CacheManager;
use crate::web::routes::AppState;
use axum::extract::FromRef;
use std::sync::Arc;

/// Context for room-related handlers (create, join, leave, state, messages).
#[derive(Clone)]
pub struct RoomContext {
    pub room_service: Arc<synapse_services::room_service::RoomService>,
    pub auth_service: synapse_services::auth::AuthService,
    pub user_storage: Arc<dyn synapse_storage::UserStore>,
    pub server_name: String,
    pub cache: Arc<CacheManager>,
}

impl FromRef<AppState> for RoomContext {
    fn from_ref(state: &AppState) -> Self {
        Self {
            room_service: state.services.rooms.room_service.clone(),
            auth_service: state.services.core.auth_service.clone(),
            user_storage: state.services.account.user_storage.clone(),
            server_name: state.services.core.server_name.clone(),
            cache: state.cache.clone(),
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
}

impl FromRef<AppState> for SyncContext {
    fn from_ref(state: &AppState) -> Self {
        Self {
            sync_service: state.services.rooms.sync_service.clone(),
            auth_service: state.services.core.auth_service.clone(),
            user_storage: state.services.account.user_storage.clone(),
            cache: state.cache.clone(),
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
}

impl FromRef<AppState> for DeviceContext {
    fn from_ref(state: &AppState) -> Self {
        Self {
            device_storage: state.services.account.device_storage.clone(),
            auth_service: state.services.core.auth_service.clone(),
            user_storage: state.services.account.user_storage.clone(),
            server_name: state.services.core.server_name.clone(),
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
}

impl FromRef<AppState> for AuthContext {
    fn from_ref(state: &AppState) -> Self {
        Self {
            auth_service: state.services.core.auth_service.clone(),
            registration_service: state.services.core.registration_service.clone(),
            user_storage: state.services.account.user_storage.clone(),
            server_name: state.services.core.server_name.clone(),
            cache: state.cache.clone(),
        }
    }
}
