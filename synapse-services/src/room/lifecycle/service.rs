//! Domain service for room lifecycle operations — create, upgrade, and
//! migration.
//!
//! Extracted from RoomService as part of the domain split plan (Task 4).

use std::sync::Arc;
use synapse_common::validation::Validator;
use synapse_storage::UserStore;
use tokio::sync::RwLock;

use super::super::service::RoomService;

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
    /// Back-reference to RoomService for cross-domain calls (e.g.,
    /// `create_event`, `join_room`, `invite_user`, `room_summary_service`).
    /// Set via `set_room_service` after RoomService is wrapped in `Arc`.
    pub(crate) room_service: Arc<RwLock<Option<Arc<RoomService>>>>,
}

/// Configuration for constructing a [`LifecycleService`].
pub struct LifecycleServiceConfig {
    pub room_storage: Arc<dyn synapse_storage::RoomRepository>,
    pub member_storage: Arc<dyn synapse_storage::RoomMemberRepository>,
    pub event_storage: Arc<dyn synapse_storage::EventRepository>,
    pub user_storage: Arc<dyn UserStore>,
    pub validator: Arc<Validator>,
    pub server_name: String,
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
            room_service: Arc::new(RwLock::new(None)),
        }
    }

    /// Post-construction wiring: set the back-reference to the enclosing
    /// [`RoomService`]. Called once after `RoomService` is wrapped in `Arc`.
    pub async fn set_room_service(&self, room_service: Arc<RoomService>) {
        *self.room_service.write().await = Some(room_service);
    }

    /// Resolve the room_service back-reference, panicking if not set.
    /// Used for methods that require the back-reference (e.g.,
    /// `create_event`, `join_room`, `invite_user`, `room_summary_service`
    /// access). Panics if called before `set_room_service()` wiring.
    pub(crate) async fn room_service_ref(&self) -> Arc<RoomService> {
        self.room_service
            .read()
            .await
            .clone()
            .expect("LifecycleService::room_service back-reference not wired")
    }
}
