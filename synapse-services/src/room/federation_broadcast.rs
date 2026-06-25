//! Outbound PDU signing and broadcasting.
//!
//! This module wires locally-produced events into the federation outbound
//! pipeline:
//! 1. Build the full PDU JSON from a stored `RoomEvent`
//! 2. Fetch `prev_events` from the room's forward extremities
//! 3. Sign and hash the PDU via `sign_and_hash_event`
//! 4. Persist `signatures` / `hashes` back to the `events` table
//! 5. Broadcast the signed PDU to all eligible remote servers via
//!    `EventBroadcaster::broadcast_event`
//!
//! Reference: element-hq/synapse
//! `synapse/handlers/federation_sender.py::FederationSenderHandler.send_pdu`
//! and `synapse/crypto/event_signing.py::add_hashes_and_signatures`

use crate::common::error::{ApiError, ApiResult};
use serde_json::{json, Value};
use synapse_federation::signing::sign_and_hash_event;
use synapse_storage::event::RoomEvent;

use super::service::RoomService;

impl RoomService {
    /// Sign a locally-produced event and broadcast it to all remote servers
    /// that have joined members in the room.
    ///
    /// This is a best-effort operation:
    /// - If no `key_rotation_manager` is configured (test setups), the method
    ///   returns `Ok(())` without signing or broadcasting.
    /// - Signing failures are returned as errors so callers can log them.
    /// - Broadcast failures are logged but do not propagate (the event is
    ///   still valid locally).
    pub async fn sign_and_broadcast_event(&self, event: &RoomEvent) -> ApiResult<()> {
        // 0. Check if federation signing is configured.
        let key_rotation_guard = self.key_rotation_manager.read().await;
        let Some(ref key_rotation_manager) = *key_rotation_guard else {
            // No signing key configured — skip signing and broadcasting.
            // This is normal in test setups.
            return Ok(());
        };

        // 1. Fetch prev_events (forward extremities of the room).
        let prev_events = self.event_storage.get_latest_event_ids_in_room(&event.room_id, 10).await.unwrap_or_default();

        // Exclude the event itself if it somehow already appears in the
        // extremities list (e.g. re-broadcast after retry).
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
            pdu["state_key"] = Value::String(state_key.clone());
        }

        if let Some(ref redacts) = event.redacts {
            pdu["redacts"] = Value::String(redacts.clone());
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
        let signatures = pdu.get("signatures").cloned().unwrap_or(Value::Null);
        let hashes = pdu.get("hashes").cloned().unwrap_or(Value::Null);
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

        // 5. Broadcast to remote servers.
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

        Ok(())
    }
}
