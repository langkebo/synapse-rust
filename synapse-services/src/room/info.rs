//! Room info queries and basic metadata helpers.

use crate::common::error::{ApiError, ApiResult};
use serde_json::json;

use super::service::RoomService;

impl RoomService {
    pub async fn get_room_encryption_status(
        &self,
        room_id: &str,
    ) -> ApiResult<synapse_storage::room::RoomEncryptionStatus> {
        let room = self
            .room_storage
            .get_room(room_id)
            .await
            .map_err(|e| ApiError::internal_with_log("Failed to get room", &e))?
            .ok_or_else(|| ApiError::not_found("Room not found".to_string()))?;

        let encryption_events = self
            .event_storage
            .get_state_events_by_type(room_id, "m.room.encryption")
            .await
            .map_err(|e| ApiError::internal_with_log("Failed to get encryption event content", &e))?;
        let encryption_content = encryption_events.first().map(|event| event.content.clone());
        let is_encrypted = encryption_content.is_some();

        Ok(synapse_storage::room::RoomEncryptionStatus::from_encryption_event(
            is_encrypted,
            if is_encrypted {
                encryption_content
                    .as_ref()
                    .and_then(|content| content.get("algorithm").and_then(|v| v.as_str()).map(|s| s.to_string()))
                    .or_else(|| room.encryption.clone())
            } else {
                None
            },
            encryption_content.as_ref().and_then(|content| content.get("rotation_period_ms").and_then(|v| v.as_i64())),
            encryption_content
                .as_ref()
                .and_then(|content| content.get("rotation_period_msgs").and_then(|v| v.as_i64())),
        ))
    }

    pub async fn delete_room(&self, room_id: &str, requester_id: &str) -> ApiResult<()> {
        let room = self
            .room_storage
            .get_room(room_id)
            .await
            .map_err(|e| ApiError::internal_with_log("Failed to get room", &e))?
            .ok_or_else(|| ApiError::not_found("Room not found".to_string()))?;

        let requester = self
            .user_storage
            .get_user_by_id(requester_id)
            .await
            .map_err(|e| ApiError::internal_with_log("Failed to get user", &e))?
            .ok_or_else(|| ApiError::unauthorized("Requester not found"))?;

        let is_creator = room.creator_user_id.as_deref() == Some(requester_id);
        let is_admin = requester.is_admin;

        if !is_creator && !is_admin {
            return Err(ApiError::forbidden("Only the room creator or a server admin can delete a room".to_string()));
        }

        self.room_storage
            .delete_room(room_id)
            .await
            .map_err(|e| ApiError::internal_with_log("Failed to delete room", &e))
    }

    pub async fn get_user_room_list(&self, user_id: &str) -> ApiResult<Vec<serde_json::Value>> {
        let rooms = self
            .room_storage
            .get_user_room_list_summary(user_id)
            .await
            .map_err(|e| ApiError::internal_with_log("Failed to get user rooms", &e))?;

        Ok(rooms
            .into_iter()
            .map(|(room_id, membership, name, avatar_url)| {
                json!({
                    "room_id": room_id,
                    "membership": membership,
                    "name": name,
                    "avatar_url": avatar_url
                })
            })
            .collect())
    }

    pub async fn room_exists(&self, room_id: &str) -> ApiResult<bool> {
        let exists = self
            .room_storage
            .room_exists(room_id)
            .await
            .map_err(|e| ApiError::database_with_log("Failed to check room existence", &e))?;
        Ok(exists)
    }

    pub async fn is_room_creator(&self, room_id: &str, user_id: &str) -> ApiResult<bool> {
        let room = self
            .room_storage
            .get_room(room_id)
            .await
            .map_err(|e| ApiError::internal_with_log("Failed to get room", &e))?;

        match room {
            Some(r) => Ok(r.creator_user_id.as_deref() == Some(user_id)),
            None => Ok(false),
        }
    }

    pub async fn check_room_has_encryption(&self, room_id: &str) -> ApiResult<bool> {
        self.event_storage
            .check_room_has_encryption(room_id)
            .await
            .map_err(|e| ApiError::internal_with_log("Failed to check room encryption status", &e))
    }
}
