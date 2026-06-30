//! Room upgrade and migration operations.

use crate::common::error::{ApiError, ApiResult};
use serde_json::json;
use synapse_common::generate_event_id;
use synapse_storage::CreateEventParams;

use super::lifecycle::service::LifecycleService;
use super::service::CreateRoomConfig;

impl LifecycleService {
    pub async fn upgrade_room(&self, old_room_id: &str, new_version: &str, user_id: &str) -> ApiResult<String> {
        let old_room = self
            .room_storage
            .get_room(old_room_id)
            .await
            .map_err(|e| ApiError::internal_with_log("Failed to get old room", &e))?
            .ok_or_else(|| ApiError::not_found("Room not found".to_string()))?;

        // Fetch old room members BEFORE creating the tombstone, so we can
        // invite them to the replacement room.  We collect joined members
        // excluding the upgrading user (who is auto-invited via create_room).
        let old_members = self
            .member_storage
            .get_room_members(old_room_id, "join")
            .await
            .map_err(|e| ApiError::internal_with_log("Failed to fetch old room members for migration", &e))?;
        let members_to_invite: Vec<String> =
            old_members.into_iter().map(|m| m.user_id).filter(|uid| uid != user_id).collect();

        let tombstone_event_id = generate_event_id(&self.server_name);
        let create_config = CreateRoomConfig {
            visibility: Some(if old_room.is_public { "public".to_string() } else { "private".to_string() }),
            room_alias_name: None,
            name: Some(old_room.name.clone().unwrap_or_else(|| "Upgraded Room".to_string())),
            topic: old_room.topic.clone(),
            invite_list: Some(vec![user_id.to_string()]),
            preset: Some("private_chat".to_string()),
            encryption: old_room.encryption.clone(),
            history_visibility: Some(old_room.history_visibility.clone()),
            is_direct: None,
            room_type: None,
            room_version: Some(new_version.to_string()),
            creation_content: Some(json!({
                "predecessor": {
                    "room_id": old_room_id,
                    "event_id": tombstone_event_id,
                }
            })),
            ..Default::default()
        };

        let replacement_room = self.create_room(user_id, create_config).await?;
        let new_room_id = replacement_room
            .get("room_id")
            .and_then(|value| value.as_str())
            .ok_or_else(|| ApiError::internal("Room upgrade did not return replacement room"))?
            .to_string();

        // Create the tombstone event in the OLD room via the `create_event`
        // wrapper (not the raw storage layer).  This ensures the event is
        // signed with the server's signing key and broadcast to all remote
        // servers with joined members in the old room, so federated
        // homeservers learn that the room has been replaced.
        let room_svc = self.room_service_ref().await;
        let tombstone_event = room_svc
            .create_event(
                CreateEventParams {
                    event_id: tombstone_event_id,
                    room_id: old_room_id.to_string(),
                    user_id: user_id.to_string(),
                    event_type: "m.room.tombstone".to_string(),
                    content: json!({
                        "body": "This room has been replaced",
                        "replacement_room": new_room_id.clone(),
                    }),
                    state_key: Some("".to_string()),
                    origin_server_ts: chrono::Utc::now().timestamp_millis(),
                    redacts: None,
                },
                None,
            )
            .await
            .map_err(|e| ApiError::internal_with_log("Failed to create tombstone event", &e))?;

        ::tracing::info!(
            old_room_id = %old_room_id,
            new_room_id = %new_room_id,
            tombstone_event_id = %tombstone_event.event_id,
            "Room upgraded: tombstone event created and broadcast"
        );

        // Auto-join the upgrading user to the new room.  `create_room` only
        // invites the creator; we need them to be a joined member so they can
        // immediately use the replacement room (matches Synapse behavior).
        if let Err(e) = room_svc.join_room(&new_room_id, user_id).await {
            ::tracing::warn!(
                old_room_id = %old_room_id,
                new_room_id = %new_room_id,
                user_id = %user_id,
                error = %e,
                "Failed to auto-join upgrading user to replacement room"
            );
        }

        // Invite all former joined members of the old room to the new room.
        // Local users go through the local invite path; remote users go
        // through the federation invite path (handled transparently by
        // `invite_user`).
        for invitee_id in &members_to_invite {
            if let Err(e) = room_svc.invite_user(&new_room_id, user_id, invitee_id).await {
                ::tracing::warn!(
                    old_room_id = %old_room_id,
                    new_room_id = %new_room_id,
                    invitee_id = %invitee_id,
                    error = %e,
                    "Failed to invite old room member to replacement room"
                );
            }
        }

        // Copy state events (power levels, join_rules, canonical_alias, etc.)
        // from the old room to the new room.  This is best-effort: failures
        // are logged but do not fail the upgrade, since the new room is
        // already functional with its default state.
        if let Err(e) = self.migrate_room_content(old_room_id, &new_room_id, user_id).await {
            ::tracing::warn!(
                old_room_id = %old_room_id,
                new_room_id = %new_room_id,
                error = %e,
                "Failed to migrate room content (best-effort)"
            );
        }

        Ok(new_room_id)
    }

    pub async fn get_tombstone_event(&self, room_id: &str) -> ApiResult<Option<serde_json::Value>> {
        let state_events = self
            .event_storage
            .get_state_events(room_id)
            .await
            .map_err(|e| ApiError::internal_with_log("Failed to get state events", &e))?;

        for event in state_events {
            if event.event_type.as_deref() == Some("m.room.tombstone") {
                return Ok(Some(serde_json::json!({
                    "type": event.event_type.clone().unwrap_or_default(),
                    "state_key": event.state_key,
                    "content": event.content,
                    "sender": event.sender,
                })));
            }
        }

        Ok(None)
    }

    pub async fn migrate_room_content(
        &self,
        source_room_id: &str,
        target_room_id: &str,
        user_id: &str,
    ) -> ApiResult<()> {
        let target_room = self
            .room_storage
            .get_room(target_room_id)
            .await
            .map_err(|e| ApiError::internal_with_log("Failed to get target room", &e))?
            .ok_or_else(|| ApiError::not_found("Target room not found".to_string()))?;

        if target_room.creator_user_id.as_deref() != Some(user_id) {
            return Err(ApiError::forbidden("Only room creator can migrate content".to_string()));
        }

        self.room_storage
            .copy_room_state(source_room_id, target_room_id)
            .await
            .map_err(|e| ApiError::internal_with_log("Failed to copy room state", &e))?;

        Ok(())
    }

    pub async fn is_room_upgrade_allowed(&self, room_id: &str, user_id: &str) -> ApiResult<bool> {
        let room = self
            .room_storage
            .get_room(room_id)
            .await
            .map_err(|e| ApiError::internal_with_log("Failed to get room", &e))?
            .ok_or_else(|| ApiError::not_found("Room not found".to_string()))?;

        let members = self
            .member_storage
            .get_room_members(room_id, "join")
            .await
            .map_err(|e| ApiError::internal_with_log("Failed to get members", &e))?;

        let is_member = members.iter().any(|m| m.user_id == user_id && m.membership == "join");

        if !is_member {
            return Ok(false);
        }

        Ok(room.creator_user_id.as_deref() == Some(user_id))
    }
}
