//! Domain service for room lifecycle operations — create, upgrade, and
//! migration.
//!
//! Extracted from RoomService as part of the domain split plan (Task 4).

use std::sync::Arc;
use synapse_cache::CacheManager;
use synapse_common::validation::Validator;
use synapse_storage::{EventStoreApi, MemberStoreApi, RoomStoreApi, UserStore};

/// Domain service for room lifecycle operations — create, upgrade, and
/// migration.
#[derive(Clone)]
pub struct LifecycleService {
    pub(crate) room_storage: Arc<dyn RoomStoreApi>,
    pub(crate) member_storage: Arc<dyn MemberStoreApi>,
    pub(crate) event_storage: Arc<dyn EventStoreApi>,
    pub(crate) user_storage: Arc<dyn UserStore>,
    pub(crate) validator: Arc<Validator>,
    pub(crate) server_name: String,
    /// Direct reference to RoomSummaryService, injected during construction
    /// instead of via a back-reference to RoomService.
    pub(crate) room_summary_service: Option<Arc<crate::room::summary::RoomSummaryService>>,
    pub(crate) cache: Arc<CacheManager>,
    /// Optional application-service manager. When present, room lifecycle
    /// events (create, upgrade) are enqueued for matching application
    /// services after the transaction commits.
    pub(crate) app_service_manager: Option<Arc<crate::application_service::ApplicationServiceManager>>,
}

/// Configuration for constructing a [`LifecycleService`].
pub struct LifecycleServiceConfig {
    pub room_storage: Arc<dyn RoomStoreApi>,
    pub member_storage: Arc<dyn MemberStoreApi>,
    pub event_storage: Arc<dyn EventStoreApi>,
    pub user_storage: Arc<dyn UserStore>,
    pub validator: Arc<Validator>,
    pub server_name: String,
    pub room_summary_service: Option<Arc<crate::room::summary::RoomSummaryService>>,
    pub cache: Arc<CacheManager>,
    pub app_service_manager: Option<Arc<crate::application_service::ApplicationServiceManager>>,
}

impl LifecycleService {
    pub fn new(config: LifecycleServiceConfig) -> Self {
        Self {
            room_storage: config.room_storage,
            member_storage: config.member_storage,
            event_storage: config.event_storage,
            user_storage: config.user_storage,
            validator: config.validator,
            server_name: config.server_name,
            room_summary_service: config.room_summary_service,
            cache: config.cache,
            app_service_manager: config.app_service_manager,
        }
    }
}
