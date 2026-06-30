//! Domain service for messaging operations — events, messages, receipts,
//! read markers, burn-after-read, and federation broadcast.
//!
//! Extracted from RoomService as part of the domain split plan (Task 2).

use crate::common::error::ApiResult;
use std::collections::HashMap;
use std::sync::Arc;
use synapse_common::task_queue::RedisTaskQueue;
use synapse_storage::event::RoomEvent;
use tokio::sync::RwLock;

use super::super::service::RoomService;

/// Domain service for messaging operations — events, messages, receipts,
/// read markers, burn-after-read, and federation broadcast.
#[derive(Clone)]
pub struct MessagingService {
    pub(crate) event_storage: Arc<dyn synapse_storage::EventRepository>,
    pub(crate) room_storage: Arc<dyn synapse_storage::RoomRepository>,
    pub(crate) member_storage: Arc<dyn synapse_storage::RoomMemberRepository>,
    pub(crate) server_name: String,
    #[cfg(feature = "beacons")]
    pub(crate) beacon_service: Option<Arc<crate::beacon_service::BeaconService>>,
    #[cfg(not(feature = "beacons"))]
    #[allow(dead_code)]
    pub(crate) beacon_service: Option<()>,
    pub(crate) task_queue: Option<Arc<RedisTaskQueue>>,
    pub(crate) active_tasks: Arc<RwLock<HashMap<String, tokio::task::JoinHandle<()>>>>,
    pub(crate) event_broadcaster: Arc<RwLock<Option<Arc<synapse_federation::event_broadcaster::EventBroadcaster>>>>,
    pub(crate) relations_storage: Arc<dyn synapse_storage::RelationsRepository>,
    /// Back-reference to RoomService for cross-domain calls (e.g.,
    /// `sign_and_broadcast_event`, `dispatch_appservice_event`,
    /// `room_summary_service`).
    /// Set via `set_room_service` after RoomService is wrapped in `Arc`.
    pub(crate) room_service: Arc<RwLock<Option<Arc<RoomService>>>>,
}

/// Configuration for constructing a [`MessagingService`].
pub struct MessagingServiceConfig {
    pub event_storage: Arc<dyn synapse_storage::EventRepository>,
    pub room_storage: Arc<dyn synapse_storage::RoomRepository>,
    pub member_storage: Arc<dyn synapse_storage::RoomMemberRepository>,
    pub server_name: String,
    #[cfg(feature = "beacons")]
    pub beacon_service: Option<Arc<crate::beacon_service::BeaconService>>,
    #[cfg(not(feature = "beacons"))]
    pub beacon_service: Option<()>,
    pub task_queue: Option<Arc<RedisTaskQueue>>,
    pub relations_storage: Arc<dyn synapse_storage::RelationsRepository>,
    pub event_broadcaster: Option<Arc<synapse_federation::event_broadcaster::EventBroadcaster>>,
}

impl MessagingService {
    pub fn new(config: MessagingServiceConfig) -> Self {
        Self {
            event_storage: config.event_storage,
            room_storage: config.room_storage,
            member_storage: config.member_storage,
            server_name: config.server_name,
            #[cfg(feature = "beacons")]
            beacon_service: config.beacon_service,
            #[cfg(not(feature = "beacons"))]
            beacon_service: None,
            task_queue: config.task_queue,
            active_tasks: Arc::new(RwLock::new(HashMap::new())),
            event_broadcaster: Arc::new(RwLock::new(config.event_broadcaster)),
            relations_storage: config.relations_storage.clone(),
            room_service: Arc::new(RwLock::new(None)),
        }
    }

    /// Post-construction wiring: set the back-reference to the enclosing
    /// [`RoomService`]. Called once after `RoomService` is wrapped in `Arc`.
    pub async fn set_room_service(&self, room_service: Arc<RoomService>) {
        *self.room_service.write().await = Some(room_service);
    }

    /// Resolve the room_service back-reference, panicking if not set.
    pub(crate) async fn room_service_ref(&self) -> Arc<RoomService> {
        self.room_service.read().await.clone().expect("MessagingService::room_service back-reference not wired")
    }

    /// Dispatch an event to application services (best-effort).
    /// Delegates to RoomService which owns the app_service_manager.
    pub(crate) async fn dispatch_appservice_event(
        &self,
        event_id: &str,
        room_id: &str,
        event_type: &str,
        sender: &str,
        content: &serde_json::Value,
        state_key: Option<&str>,
    ) {
        self.room_service_ref()
            .await
            .dispatch_appservice_event(event_id, room_id, event_type, sender, content, state_key)
            .await;
    }

    /// Sign a locally-produced event and broadcast it to all remote servers
    /// that have joined members in the room.
    ///
    /// Best-effort: delegates to RoomService which owns the key_rotation_manager
    /// and event_broadcaster. In test setups without federation config, this is
    /// a no-op.
    pub(crate) async fn sign_and_broadcast_event(&self, event: &RoomEvent) -> ApiResult<()> {
        self.room_service_ref().await.sign_and_broadcast_event(event).await
    }
}
