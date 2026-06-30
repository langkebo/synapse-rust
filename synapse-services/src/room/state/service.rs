//! Domain service for room state operations — aliases, tags, info queries,
//! directory listings, block/unblock, encryption status, and admin search.
//!
//! Extracted from RoomService as part of the domain split plan (Task 3).

use std::sync::Arc;
use synapse_storage::UserStore;

/// Domain service for room state operations — aliases, tags, info queries,
/// directory listings, block/unblock, encryption status, and admin search.
#[derive(Clone)]
pub struct RoomStateService {
    pub(crate) room_storage: Arc<dyn synapse_storage::RoomRepository>,
    pub(crate) member_storage: Arc<dyn synapse_storage::RoomMemberRepository>,
    pub(crate) event_storage: Arc<dyn synapse_storage::EventRepository>,
    pub(crate) room_tag_storage: synapse_storage::room_tag::RoomTagStorage,
    pub(crate) user_storage: Arc<dyn UserStore>,
    pub(crate) server_name: String,
}

/// Configuration for constructing a [`RoomStateService`].
pub struct RoomStateServiceConfig {
    pub room_storage: Arc<dyn synapse_storage::RoomRepository>,
    pub member_storage: Arc<dyn synapse_storage::RoomMemberRepository>,
    pub event_storage: Arc<dyn synapse_storage::EventRepository>,
    pub room_tag_storage: synapse_storage::room_tag::RoomTagStorage,
    pub user_storage: Arc<dyn UserStore>,
    pub server_name: String,
}

impl RoomStateService {
    pub fn new(config: RoomStateServiceConfig) -> Self {
        Self {
            room_storage: config.room_storage,
            member_storage: config.member_storage,
            event_storage: config.event_storage,
            room_tag_storage: config.room_tag_storage,
            user_storage: config.user_storage,
            server_name: config.server_name,
        }
    }
}
