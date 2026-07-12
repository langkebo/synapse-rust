//! Room upgrade and migration operations.

use crate::common::error::{ApiError, ApiResult};

use super::service::LifecycleService;

impl LifecycleService {
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

        self.event_storage
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
