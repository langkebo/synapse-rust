//! Domain service for room state operations — aliases, tags, info queries,
//! directory listings, block/unblock, encryption status, and admin search.
//!
//! Extracted from RoomService as part of the domain split plan (Task 3).

use crate::UserService;
use std::sync::Arc;
use synapse_storage::room_tag::RoomTagStoreApi;
use synapse_storage::{MemberStoreApi, RoomStoreApi, UserStore};

/// Domain service for room state operations — aliases, tags, info queries,
/// directory listings, block/unblock, encryption status, and admin search.
#[derive(Clone)]
pub struct RoomStateService {
    pub(crate) room_storage: Arc<dyn RoomStoreApi>,
    pub(crate) member_storage: Arc<dyn MemberStoreApi>,
    pub(crate) event_reader: Arc<dyn synapse_storage::event::EventReader>,
    pub(crate) event_writer: Arc<dyn synapse_storage::event::EventWriter>,
    pub(crate) room_tag_storage: Arc<dyn RoomTagStoreApi>,
    pub(crate) user_storage: Arc<dyn UserStore>,
    // TODO(D4): wire into user_service for room-state convenience calls
    #[allow(dead_code)]
    pub(crate) user_service: Arc<UserService>,
    pub(crate) server_name: String,
}

/// Configuration for constructing a [`RoomStateService`].
pub struct RoomStateServiceConfig {
    pub room_storage: Arc<dyn RoomStoreApi>,
    pub member_storage: Arc<dyn MemberStoreApi>,
    pub event_reader: Arc<dyn synapse_storage::event::EventReader>,
    pub event_writer: Arc<dyn synapse_storage::event::EventWriter>,
    pub room_tag_storage: Arc<dyn RoomTagStoreApi>,
    pub user_storage: Arc<dyn UserStore>,
    pub user_service: Arc<UserService>,
    pub server_name: String,
}

impl RoomStateService {
    pub fn new(config: RoomStateServiceConfig) -> Self {
        Self {
            room_storage: config.room_storage,
            member_storage: config.member_storage,
            event_reader: config.event_reader,
            event_writer: config.event_writer,
            room_tag_storage: config.room_tag_storage,
            user_storage: config.user_storage,
            user_service: config.user_service,
            server_name: config.server_name,
        }
    }
}
