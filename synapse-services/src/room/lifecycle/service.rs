//! Domain service for room lifecycle operations — create, upgrade, and
//! migration.
//!
//! Extracted from RoomService as part of the domain split plan (Task 4).

use std::sync::Arc;
use synapse_common::validation::Validator;
use synapse_storage::{MemberStoreApi, RoomStoreApi, UserStore};

/// Domain service for room lifecycle operations — create, upgrade, and
/// migration.
#[derive(Clone)]
pub struct LifecycleService {
    pub(crate) room_storage: Arc<dyn RoomStoreApi>,
    pub(crate) member_storage: Arc<dyn MemberStoreApi>,
    pub(crate) event_reader: Arc<dyn synapse_storage::event::EventReader>,
    pub(crate) event_writer: Arc<dyn synapse_storage::event::EventWriter>,
    pub(crate) user_storage: Arc<dyn UserStore>,
    pub(crate) validator: Arc<Validator>,
    pub(crate) server_name: String,
    /// Direct reference to RoomSummaryService, injected during construction
    /// instead of via a back-reference to RoomService.
    pub(crate) room_summary_service: Option<Arc<crate::room::summary::RoomSummaryService>>,
}

/// Configuration for constructing a [`LifecycleService`].
pub struct LifecycleServiceConfig {
    pub room_storage: Arc<dyn RoomStoreApi>,
    pub member_storage: Arc<dyn MemberStoreApi>,
    pub event_reader: Arc<dyn synapse_storage::event::EventReader>,
    pub event_writer: Arc<dyn synapse_storage::event::EventWriter>,
    pub user_storage: Arc<dyn UserStore>,
    pub validator: Arc<Validator>,
    pub server_name: String,
    pub room_summary_service: Option<Arc<crate::room::summary::RoomSummaryService>>,
}

impl LifecycleService {
    pub fn new(config: LifecycleServiceConfig) -> Self {
        Self {
            room_storage: config.room_storage,
            member_storage: config.member_storage,
            event_reader: config.event_reader,
            event_writer: config.event_writer,
            user_storage: config.user_storage,
            validator: config.validator,
            server_name: config.server_name,
            room_summary_service: config.room_summary_service,
        }
    }
}
