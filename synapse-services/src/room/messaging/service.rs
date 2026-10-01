//! Domain service for messaging operations — events, messages, receipts,
//! read markers, burn-after-read, and federation broadcast.
//!
//! Extracted from RoomService as part of the domain split plan (Task 2).

use crate::common::error::ApiResult;
use std::collections::HashMap;
use std::sync::Arc;
use synapse_cache::CacheManager;
use synapse_common::task_queue::RedisTaskQueue;
use synapse_storage::event::{EventReader, EventWriter, RoomEvent};
use synapse_storage::membership::MemberStoreApi;
use synapse_storage::relations::RelationsStoreApi;
use synapse_storage::room::RoomStoreApi;
use tokio::sync::RwLock;

use crate::room::summary::RoomSummaryService;

/// Domain service for messaging operations — events, messages, receipts,
/// read markers, burn-after-read, and federation broadcast.
#[derive(Clone)]
pub struct MessagingService {
    pub(crate) event_reader: Arc<dyn EventReader>,
    pub(crate) event_writer: Arc<dyn EventWriter>,
    pub(crate) room_storage: Arc<dyn RoomStoreApi>,
    pub(crate) member_storage: Arc<dyn MemberStoreApi>,
    pub(crate) server_name: String,
    #[cfg(feature = "beacons")]
    pub(crate) beacon_service: Option<Arc<crate::beacon_service::BeaconService>>,
    #[cfg(not(feature = "beacons"))]
    #[allow(dead_code)]
    pub(crate) beacon_service: Option<()>, // Feature-gated placeholder
    pub(crate) task_queue: Option<Arc<RedisTaskQueue>>,
    pub(crate) active_tasks: Arc<RwLock<HashMap<String, tokio::task::JoinHandle<()>>>>,
    pub(crate) event_broadcaster: Option<Arc<synapse_federation::event_broadcaster::EventBroadcaster>>,
    pub(crate) relations_storage: Arc<dyn RelationsStoreApi>,
    /// Application service manager for dispatching events to bridges.
    pub(crate) app_service_manager: Option<Arc<crate::application_service::ApplicationServiceManager>>,
    /// Server signing key manager for signing locally-produced PDUs.
    pub(crate) key_rotation_manager: Option<Arc<synapse_federation::KeyRotationManager>>,
    /// Room summary service for updating room metadata on events.
    pub(crate) room_summary_service: Arc<RoomSummaryService>,
    pub(crate) cache: Arc<CacheManager>,
    /// The third-party event admission gate — consulted by every event write
    /// (`create_event`, `create_event_with_graph`). See
    /// [`crate::module_service::EventAdmissionGate`].
    pub(crate) event_admission_gate: Arc<dyn crate::module_service::EventAdmissionGate>,
}

/// Configuration for constructing a [`MessagingService`].
pub struct MessagingServiceConfig {
    /// The `event_reader` field.
    pub event_reader: Arc<dyn EventReader>,
    /// The `event_writer` field.
    pub event_writer: Arc<dyn EventWriter>,
    /// The `room_storage` field.
    pub room_storage: Arc<dyn RoomStoreApi>,
    /// The `member_storage` field.
    pub member_storage: Arc<dyn MemberStoreApi>,
    /// The `server_name` field.
    pub server_name: String,
    #[cfg(feature = "beacons")]
    /// The `beacon_service` field.
    pub beacon_service: Option<Arc<crate::beacon_service::BeaconService>>,
    #[cfg(not(feature = "beacons"))]
    /// The `beacon_service` field.
    pub beacon_service: Option<()>,
    /// The `task_queue` field.
    pub task_queue: Option<Arc<RedisTaskQueue>>,
    /// The `relations_storage` field.
    pub relations_storage: Arc<dyn RelationsStoreApi>,
    /// The `event_broadcaster` field.
    pub event_broadcaster: Option<Arc<synapse_federation::event_broadcaster::EventBroadcaster>>,
    /// The `app_service_manager` field.
    pub app_service_manager: Option<Arc<crate::application_service::ApplicationServiceManager>>,
    /// The `key_rotation_manager` field.
    pub key_rotation_manager: Option<Arc<synapse_federation::KeyRotationManager>>,
    /// The `room_summary_service` field.
    pub room_summary_service: Arc<RoomSummaryService>,
    /// The `cache` field.
    pub cache: Arc<CacheManager>,
    /// The `event_admission_gate` field.
    pub event_admission_gate: Arc<dyn crate::module_service::EventAdmissionGate>,
}

impl MessagingService {
    /// See [`new`].
    pub fn new(config: MessagingServiceConfig) -> Self {
        Self {
            event_reader: config.event_reader,
            event_writer: config.event_writer,
            room_storage: config.room_storage,
            member_storage: config.member_storage,
            server_name: config.server_name,
            #[cfg(feature = "beacons")]
            beacon_service: config.beacon_service,
            #[cfg(not(feature = "beacons"))]
            beacon_service: None,
            task_queue: config.task_queue,
            active_tasks: Arc::new(RwLock::new(HashMap::new())),
            event_broadcaster: config.event_broadcaster,
            relations_storage: config.relations_storage.clone(),
            app_service_manager: config.app_service_manager,
            key_rotation_manager: config.key_rotation_manager,
            room_summary_service: config.room_summary_service,
            cache: config.cache,
            event_admission_gate: config.event_admission_gate,
        }
    }

    /// Dispatch an event to application services (best-effort).
    pub(crate) async fn dispatch_appservice_event(
        &self,
        event_id: &str,
        room_id: &str,
        event_type: &str,
        sender: &str,
        content: &serde_json::Value,
        state_key: Option<&str>,
    ) {
        let Some(app_service_manager) = &self.app_service_manager else {
            return;
        };
        if let Err(error) =
            app_service_manager.enqueue_matching_event(event_id, room_id, event_type, sender, content, state_key).await
        {
            ::tracing::warn!(error = %error, "Failed to dispatch appservice event");
        }
    }

    /// Sign a locally-produced event and broadcast it to all remote servers
    /// that have joined members in the room.
    ///
    /// Thin adapter over [`crate::room::federation_broadcast`], which owns the
    /// single implementation shared with the membership service; the split used
    /// to carry two copies that disagreed on the failure policy and on where
    /// `redacts` belongs.
    ///
    /// Best-effort: in test setups without federation config, this is a no-op.
    /// Broadcast failures are logged but not propagated.
    pub(crate) async fn sign_and_broadcast_event(&self, event: &RoomEvent) -> ApiResult<()> {
        let ctx = crate::room::federation_broadcast::BroadcastContext {
            server_name: self.server_name.clone(),
            event_reader: self.event_reader.clone(),
            event_writer: self.event_writer.clone(),
            key_rotation_manager: self.key_rotation_manager.clone(),
            event_broadcaster: self.event_broadcaster.clone(),
            room_storage: self.room_storage.clone(),
        };
        crate::room::federation_broadcast::sign_and_broadcast_event(&ctx, event).await
    }
}
