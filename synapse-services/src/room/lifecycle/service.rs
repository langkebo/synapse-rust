//! Domain service for room lifecycle operations — create, upgrade, and
//! migration.
//!
//! Extracted from RoomService as part of the domain split plan (Task 4).

use std::sync::Arc;
use synapse_common::validation::Validator;
use synapse_storage::UserStore;

/// Domain service for room lifecycle operations — create, upgrade, and
/// migration.
#[derive(Clone)]
pub struct LifecycleService {
    pub(crate) room_storage: Arc<dyn synapse_storage::RoomRepository>,
    pub(crate) member_storage: Arc<dyn synapse_storage::RoomMemberRepository>,
    pub(crate) event_storage: Arc<dyn synapse_storage::EventRepository>,
    pub(crate) user_storage: Arc<dyn UserStore>,
    pub(crate) validator: Arc<Validator>,
    pub(crate) server_name: String,
    /// Direct reference to RoomSummaryService, injected during construction
    /// instead of via a back-reference to RoomService.
    pub(crate) room_summary_service: Arc<crate::room::summary::RoomSummaryService>,
}

/// Configuration for constructing a [`LifecycleService`].
pub struct LifecycleServiceConfig {
    pub room_storage: Arc<dyn synapse_storage::RoomRepository>,
    pub member_storage: Arc<dyn synapse_storage::RoomMemberRepository>,
    pub event_storage: Arc<dyn synapse_storage::EventRepository>,
    pub user_storage: Arc<dyn UserStore>,
    pub validator: Arc<Validator>,
    pub server_name: String,
    pub room_summary_service: Arc<crate::room::summary::RoomSummaryService>,
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
        }
    }
}
