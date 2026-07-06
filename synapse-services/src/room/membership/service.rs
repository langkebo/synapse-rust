//! Domain service for room membership operations — join, leave, invite,
//! kick, ban, unban, knock, forget, and federation membership.
//!
//! Extracted from RoomService as part of the domain split plan (Task 1).

use crate::common::error::{ApiError, ApiResult};
use serde_json::json;
use std::sync::Arc;
use synapse_federation::client_api::FederationClientApi;
use synapse_federation::key_rotation::SigningKey;
use synapse_federation::signing::sign_and_hash_event;
use synapse_federation::KeyRotationManager;
use synapse_storage::event::RoomEvent;
use synapse_storage::UserStore;
use tokio::sync::RwLock;

use crate::room::summary::RoomSummaryService;

/// Domain service for room membership operations — join, leave, invite,
/// kick, ban, unban, knock, forget, and federation membership.
#[derive(Clone)]
pub struct MembershipService {
    pub(crate) member_storage: Arc<synapse_storage::membership::RoomMemberStorage>,
    pub(crate) room_storage: Arc<synapse_storage::room::RoomStorage>,
    pub(crate) event_storage: Arc<synapse_storage::event::EventStorage>,
    pub(crate) user_storage: Arc<dyn UserStore>,
    pub(crate) auth_service: Arc<dyn crate::auth::Auth>,
    pub(crate) server_name: String,
    pub(crate) federation_client: Arc<RwLock<Option<Arc<dyn FederationClientApi>>>>,
    pub(crate) key_rotation_manager: Arc<RwLock<Option<Arc<KeyRotationManager>>>>,
    pub(crate) event_broadcaster: Arc<RwLock<Option<Arc<synapse_federation::event_broadcaster::EventBroadcaster>>>>,
    pub(crate) room_summary_service: Arc<RoomSummaryService>,
}

/// Configuration for constructing a [`MembershipService`].
pub struct MembershipServiceConfig {
    pub member_storage: Arc<synapse_storage::membership::RoomMemberStorage>,
    pub room_storage: Arc<synapse_storage::room::RoomStorage>,
    pub event_storage: Arc<synapse_storage::event::EventStorage>,
    pub user_storage: Arc<dyn UserStore>,
    pub auth_service: Arc<dyn crate::auth::Auth>,
    pub server_name: String,
    pub federation_client: Arc<RwLock<Option<Arc<dyn FederationClientApi>>>>,
    pub key_rotation_manager: Arc<RwLock<Option<Arc<KeyRotationManager>>>>,
    pub event_broadcaster: Arc<RwLock<Option<Arc<synapse_federation::event_broadcaster::EventBroadcaster>>>>,
    pub room_summary_service: Arc<RoomSummaryService>,
}

impl MembershipService {
    pub fn new(config: MembershipServiceConfig) -> Self {
        Self {
            member_storage: config.member_storage,
            room_storage: config.room_storage,
            event_storage: config.event_storage,
            user_storage: config.user_storage,
            auth_service: config.auth_service,
            server_name: config.server_name,
            federation_client: config.federation_client,
            key_rotation_manager: config.key_rotation_manager,
            event_broadcaster: config.event_broadcaster,
            room_summary_service: config.room_summary_service,
        }
    }

    // =========================================================================
    // Federation helpers (used by federation_membership)
    // =========================================================================

    /// Extract the server name from a Matrix ID (`@user:server` or `!room:server`).
    pub(crate) fn server_name_from_id(id: &str) -> Option<&str> {
        id.rsplit_once(':').map(|(_, server)| server)
    }

    /// Return `true` if the given Matrix ID belongs to a remote server.
    pub(crate) fn is_remote_id(id: &str, local_server: &str) -> bool {
        Self::server_name_from_id(id).is_some_and(|srv| srv != local_server)
    }

    /// Check if a user ID belongs to a remote server (relative to this
    /// homeserver).
    pub fn is_remote_user(&self, user_id: &str) -> bool {
        Self::is_remote_id(user_id, &self.server_name)
    }

    /// Check if a room ID belongs to a remote server (relative to this
    /// homeserver).
    pub fn is_remote_room(&self, room_id: &str) -> bool {
        Self::is_remote_id(room_id, &self.server_name)
    }

    /// Get the federation client, returning an error if not configured.
    pub(crate) async fn require_federation_client(
        &self,
    ) -> ApiResult<Arc<dyn synapse_federation::client_api::FederationClientApi>> {
        self.federation_client
            .read()
            .await
            .clone()
            .ok_or_else(|| ApiError::internal("Federation client not configured".to_string()))
    }

    /// Get the current signing key, returning an error if not configured.
    pub(crate) async fn require_signing_key(&self) -> ApiResult<SigningKey> {
        let key_rotation_manager = self
            .key_rotation_manager
            .read()
            .await
            .clone()
            .ok_or_else(|| ApiError::internal("Key rotation manager not configured".to_string()))?;
        key_rotation_manager
            .get_current_key()
            .await
            .map_err(|e| ApiError::internal_with_log("Failed to get signing key", &e))?
            .ok_or_else(|| ApiError::internal("No signing key available".to_string()))
    }

    /// Get state events by type — thin wrapper around `event_storage`.
    /// Returns JSON-formatted event list.
    pub(crate) async fn get_state_events_by_type(
        &self,
        room_id: &str,
        event_type: &str,
    ) -> ApiResult<Vec<serde_json::Value>> {
        let events = self
            .event_storage
            .get_state_events_by_type(room_id, event_type)
            .await
            .map_err(|e| ApiError::internal_with_log("Failed to get state events by type", &e))?;

        let event_list: Vec<serde_json::Value> = events
            .iter()
            .map(|e| {
                json!({
                    "event_id": e.event_id,
                    "sender": e.user_id,
                    "type": e.event_type,
                    "content": e.content,
                    "state_key": e.state_key
                })
            })
            .collect();

        Ok(event_list)
    }

    /// Check if the destination server is allowed by the room's server ACL
    /// policy before making an outbound federation request.
    pub(crate) async fn check_outbound_server_acl(&self, room_id: &str, destination: &str) -> ApiResult<()> {
        // Only check if the room exists locally (has state events)
        if !self.room_storage.room_exists(room_id).await? {
            return Ok(());
        }

        let acl_events = self.get_state_events_by_type(room_id, "m.room.server_acl").await?;
        let Some(acl_event) = acl_events.first() else {
            return Ok(());
        };

        let Some(acl_content) = acl_event.get("content") else {
            return Ok(());
        };

        let Some(acl) = synapse_federation::ServerAclContent::from_value(acl_content) else {
            tracing::warn!(room_id = %room_id, destination = %destination, "Failed to parse m.room.server_acl content for outbound check");
            return Ok(());
        };

        if !acl.is_server_allowed(destination) {
            return Err(ApiError::forbidden(format!(
                "Server '{}' is denied by room ACL for room '{}'",
                destination, room_id
            )));
        }

        Ok(())
    }

    /// Sign a locally-produced event and broadcast it to all remote servers
    /// that have joined members in the room.
    ///
    /// Best-effort: in test setups without federation config, this is a no-op.
    /// Broadcast failures are logged but not propagated.
    pub async fn sign_and_broadcast_event(&self, event: &RoomEvent) -> ApiResult<()> {
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

#[cfg(test)]
mod tests {
    use super::*;

    // ── server_name_from_id ────────────────────────────────────────

    #[test]
    fn server_name_from_user_id() {
        assert_eq!(MembershipService::server_name_from_id("@user:myserver.com"), Some("myserver.com"));
    }

    #[test]
    fn server_name_from_room_id() {
        assert_eq!(MembershipService::server_name_from_id("!room:myserver.com"), Some("myserver.com"));
    }

    #[test]
    fn server_name_from_id_no_colon() {
        assert_eq!(MembershipService::server_name_from_id("justastring"), None);
    }

    #[test]
    fn server_name_from_id_empty() {
        assert_eq!(MembershipService::server_name_from_id(""), None);
    }

    #[test]
    fn server_name_from_id_multiple_colons() {
        // rsplit_once picks the last colon
        assert_eq!(MembershipService::server_name_from_id("@user:sub:server.com"), Some("server.com"));
    }

    #[test]
    fn server_name_from_id_trailing_colon() {
        assert_eq!(MembershipService::server_name_from_id("text:"), Some(""));
    }

    #[test]
    fn server_name_from_id_leading_colon() {
        assert_eq!(MembershipService::server_name_from_id(":text"), Some("text"));
    }

    // ── is_remote_id ───────────────────────────────────────────────

    #[test]
    fn is_remote_id_true_for_other_server() {
        assert!(MembershipService::is_remote_id("@user:other.com", "myserver.com"));
    }

    #[test]
    fn is_remote_id_false_for_local_server() {
        assert!(!MembershipService::is_remote_id("@user:myserver.com", "myserver.com"));
    }

    #[test]
    fn is_remote_id_false_when_no_server_name() {
        assert!(!MembershipService::is_remote_id("no_colon", "myserver.com"));
    }

    #[test]
    fn is_remote_id_false_for_empty_id() {
        assert!(!MembershipService::is_remote_id("", "myserver.com"));
    }
}
