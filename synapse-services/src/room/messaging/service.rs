//! Domain service for messaging operations — events, messages, receipts,
//! read markers, burn-after-read, and federation broadcast.
//!
//! Extracted from RoomService as part of the domain split plan (Task 2).

use crate::common::error::{ApiError, ApiResult};
use serde_json::json;
use std::collections::HashMap;
use std::sync::Arc;
use synapse_common::task_queue::RedisTaskQueue;
use synapse_federation::signing::sign_and_hash_event;
use synapse_storage::event::{EventStoreApi, RoomEvent};
use synapse_storage::membership::MemberStoreApi;
use synapse_storage::relations::RelationsStoreApi;
use synapse_storage::room::RoomStoreApi;
use tokio::sync::RwLock;

use crate::room::summary::RoomSummaryService;

/// Domain service for messaging operations — events, messages, receipts,
/// read markers, burn-after-read, and federation broadcast.
#[derive(Clone)]
pub struct MessagingService {
    pub(crate) event_storage: Arc<dyn EventStoreApi>,
    pub(crate) room_storage: Arc<dyn RoomStoreApi>,
    pub(crate) member_storage: Arc<dyn MemberStoreApi>,
    pub(crate) server_name: String,
    #[cfg(feature = "beacons")]
    pub(crate) beacon_service: Option<Arc<crate::beacon_service::BeaconService>>,
    #[cfg(not(feature = "beacons"))]
    #[allow(dead_code)]
    pub(crate) beacon_service: Option<()>,
    pub(crate) task_queue: Option<Arc<RedisTaskQueue>>,
    pub(crate) active_tasks: Arc<RwLock<HashMap<String, tokio::task::JoinHandle<()>>>>,
    pub(crate) event_broadcaster: Arc<RwLock<Option<Arc<synapse_federation::event_broadcaster::EventBroadcaster>>>>,
    pub(crate) relations_storage: Arc<dyn RelationsStoreApi>,
    /// Application service manager for dispatching events to bridges.
    pub(crate) app_service_manager: Arc<RwLock<Option<Arc<crate::application_service::ApplicationServiceManager>>>>,
    /// Server signing key manager for signing locally-produced PDUs.
    pub(crate) key_rotation_manager: Arc<RwLock<Option<Arc<synapse_federation::KeyRotationManager>>>>,
    /// Room summary service for updating room metadata on events.
    pub(crate) room_summary_service: Arc<RoomSummaryService>,
}

/// Configuration for constructing a [`MessagingService`].
pub struct MessagingServiceConfig {
    pub event_storage: Arc<dyn EventStoreApi>,
    pub room_storage: Arc<dyn RoomStoreApi>,
    pub member_storage: Arc<dyn MemberStoreApi>,
    pub server_name: String,
    #[cfg(feature = "beacons")]
    pub beacon_service: Option<Arc<crate::beacon_service::BeaconService>>,
    #[cfg(not(feature = "beacons"))]
    pub beacon_service: Option<()>,
    pub task_queue: Option<Arc<RedisTaskQueue>>,
    pub relations_storage: Arc<dyn RelationsStoreApi>,
    pub event_broadcaster: Arc<RwLock<Option<Arc<synapse_federation::event_broadcaster::EventBroadcaster>>>>,
    pub app_service_manager: Arc<RwLock<Option<Arc<crate::application_service::ApplicationServiceManager>>>>,
    pub key_rotation_manager: Arc<RwLock<Option<Arc<synapse_federation::KeyRotationManager>>>>,
    pub room_summary_service: Arc<RoomSummaryService>,
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
            event_broadcaster: config.event_broadcaster,
            relations_storage: config.relations_storage.clone(),
            app_service_manager: config.app_service_manager,
            key_rotation_manager: config.key_rotation_manager,
            room_summary_service: config.room_summary_service,
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
        let app_service_manager = self.app_service_manager.read().await.clone();
        let Some(app_service_manager) = app_service_manager else {
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
    /// Best-effort: in test setups without federation config, this is a no-op.
    /// Broadcast failures are logged but not propagated.
    pub(crate) async fn sign_and_broadcast_event(&self, event: &RoomEvent) -> ApiResult<()> {
        // 0. Check if federation signing is configured.
        let key_rotation_guard = self.key_rotation_manager.read().await;
        let Some(ref key_rotation_manager) = *key_rotation_guard else {
            return Ok(());
        };

        // 1. Fetch prev_events (forward extremities of the room).
        let prev_events = self.event_storage.get_latest_event_ids_in_room(&event.room_id, 10).await.unwrap_or_default();

        // Exclude the event itself.
        let prev_events: Vec<String> = prev_events.into_iter().filter(|id| id != &event.event_id).collect();

        // 2. Build the PDU JSON.
        let mut pdu = json!({
            "event_id": event.event_id,
            "room_id": event.room_id,
            "sender": event.user_id,
            "user_id": event.user_id,
            "type": event.event_type,
            "content": event.content,
            "origin_server_ts": event.origin_server_ts,
            "origin": self.server_name,
            "prev_events": prev_events,
        });

        if let Some(ref state_key) = event.state_key {
            pdu["state_key"] = serde_json::Value::String(state_key.clone());
        }

        if let Some(ref redacts) = event.redacts {
            pdu["redacts"] = serde_json::Value::String(redacts.clone());
        }

        // 3. Sign and hash the PDU.
        let signing_key = key_rotation_manager
            .get_current_key()
            .await
            .map_err(|e| ApiError::internal_with_log("Failed to get signing key", &e))?
            .ok_or_else(|| ApiError::internal("No signing key available".to_string()))?;

        sign_and_hash_event(&self.server_name, &signing_key.key_id, &signing_key.secret_key, &mut pdu)
            .map_err(|e| ApiError::internal(format!("Failed to sign event: {e}")))?;

        // 4. Persist signatures and hashes back to the events table.
        let signatures = pdu.get("signatures").cloned().unwrap_or(serde_json::Value::Null);
        let hashes = pdu.get("hashes").cloned().unwrap_or(serde_json::Value::Null);
        if let Err(e) =
            self.event_storage.update_event_signatures_and_hashes(&event.event_id, &signatures, &hashes).await
        {
            ::tracing::warn!(
                event_id = %event.event_id,
                room_id = %event.room_id,
                error = %e,
                "Failed to persist event signatures/hashes"
            );
        }

        // 5. Broadcast to remote servers via event_broadcaster.
        {
            let broadcaster_guard = self.event_broadcaster.read().await;
            if let Some(ref broadcaster) = *broadcaster_guard {
                if let Err(e) = broadcaster.broadcast_event(&event.room_id, &pdu, &self.server_name).await {
                    ::tracing::warn!(
                        event_id = %event.event_id,
                        room_id = %event.room_id,
                        error = %e,
                        "Failed to broadcast event to federation peers"
                    );
                }
            }
        }

        Ok(())
    }
}
