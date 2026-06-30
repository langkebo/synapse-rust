//! Room info queries and basic metadata helpers.

use crate::common::error::{ApiError, ApiResult};
use serde_json::json;
use synapse_storage::{Room, RoomSearchCursor, RoomSearchOrder};

use super::state::service::RoomStateService;

impl RoomStateService {
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

    pub async fn cleanup_abnormal_data(&self, min_age_ms: Option<i64>) -> ApiResult<serde_json::Value> {
        self.room_storage
            .cleanup_abnormal_data(min_age_ms)
            .await
            .map_err(|e| ApiError::internal_with_log("Cleanup failed", &e))
    }

    pub async fn room_exists(&self, room_id: &str) -> ApiResult<bool> {
        let exists = self
            .room_storage
            .room_exists(room_id)
            .await
            .map_err(|e| ApiError::database_with_log("Failed to check room existence", &e))?;
        Ok(exists)
    }

    pub async fn block_room(&self, room_id: &str, blocked_by: &str, reason: Option<&str>) -> ApiResult<()> {
        let now = chrono::Utc::now().timestamp_millis();
        self.room_storage
            .block_room(room_id, now, blocked_by, reason)
            .await
            .map_err(|e| ApiError::internal_with_log("Failed to block room", &e))
    }

    pub async fn get_room_block_status(&self, room_id: &str) -> ApiResult<Option<i64>> {
        self.room_storage
            .get_room_block_status(room_id)
            .await
            .map_err(|e| ApiError::internal_with_log("Failed to get room block status", &e))
    }

    pub async fn unblock_room(&self, room_id: &str) -> ApiResult<()> {
        self.room_storage
            .unblock_room(room_id)
            .await
            .map_err(|e| ApiError::internal_with_log("Failed to unblock room", &e))
    }

    pub async fn get_public_rooms_paginated(
        &self,
        limit: i64,
        since_ts: Option<i64>,
        since_room_id: Option<&str>,
    ) -> ApiResult<Vec<synapse_storage::Room>> {
        self.room_storage
            .get_public_rooms_paginated(limit, since_ts, since_room_id)
            .await
            .map_err(|e| ApiError::internal_with_log("Failed to get public rooms", &e))
    }

    pub async fn count_public_rooms(&self) -> ApiResult<i64> {
        self.room_storage
            .count_public_rooms()
            .await
            .map_err(|e| ApiError::internal_with_log("Failed to count public rooms", &e))
    }

    pub async fn get_room_stats_overview(&self) -> ApiResult<serde_json::Value> {
        self.room_storage
            .get_room_stats_overview()
            .await
            .map_err(|e| ApiError::internal_with_log("Failed to get room statistics overview", &e))
    }

    pub async fn get_single_room_stats(&self, room_id: &str) -> ApiResult<Option<serde_json::Value>> {
        self.room_storage
            .get_single_room_stats(room_id)
            .await
            .map_err(|e| ApiError::internal_with_log("Failed to get room statistics", &e))
    }

    pub async fn get_all_rooms_with_members(
        &self,
        limit: i64,
        from: Option<RoomSearchCursor>,
        order_by: RoomSearchOrder,
    ) -> ApiResult<(Vec<(Room, i64)>, Option<String>)> {
        self.room_storage
            .get_all_rooms_with_members(limit, from, order_by)
            .await
            .map_err(|e| ApiError::database_with_log("Failed to list rooms", &e))
    }

    pub async fn get_room_count(&self) -> ApiResult<i64> {
        self.room_storage.get_room_count().await.map_err(|e| ApiError::database_with_log("Failed to count rooms", &e))
    }

    pub async fn get_room_record(&self, room_id: &str) -> ApiResult<Option<Room>> {
        self.room_storage.get_room(room_id).await.map_err(|e| ApiError::database_with_log("Failed to get room", &e))
    }

    pub async fn get_room_listings_status(&self, room_id: &str) -> ApiResult<Option<(bool, bool)>> {
        self.room_storage
            .get_room_listings_status(room_id)
            .await
            .map_err(|e| ApiError::internal_with_log("Failed to get room listing status", &e))
    }

    pub async fn set_room_public_with_directory(&self, room_id: &str) -> ApiResult<bool> {
        self.room_storage
            .set_room_public_with_directory(room_id)
            .await
            .map_err(|e| ApiError::internal_with_log("Failed to set room public", &e))
    }

    pub async fn set_room_private_with_directory(&self, room_id: &str) -> ApiResult<bool> {
        self.room_storage
            .set_room_private_with_directory(room_id)
            .await
            .map_err(|e| ApiError::internal_with_log("Failed to set room private", &e))
    }

    pub async fn shutdown_room_and_remove_members(&self, room_id: &str) -> ApiResult<()> {
        self.room_storage
            .shutdown_room(room_id)
            .await
            .map_err(|e| ApiError::internal_with_log("Failed to shutdown room", &e))?;
        self.member_storage
            .remove_all_members(room_id)
            .await
            .map_err(|e| ApiError::database_with_log("Failed to remove room members", &e))?;
        Ok(())
    }

    pub async fn grant_room_admin(&self, room_id: &str, user_id: &str) -> ApiResult<()> {
        let event_id = synapse_common::generate_event_id(&self.server_name);
        let sender = format!("@admin:{}", self.server_name);
        let now = chrono::Utc::now().timestamp_millis();
        let power_levels = json!({
            "users": {
                user_id: 100
            },
            "users_default": 0,
            "events_default": 0,
            "state_default": 50,
            "ban": 50,
            "kick": 50,
            "redact": 50,
            "invite": 0
        });

        self.event_storage
            .upsert_power_levels_event(&event_id, room_id, user_id, power_levels, now, &sender)
            .await
            .map_err(|e| ApiError::internal_with_log("Failed to grant room admin", &e))
    }

    pub async fn purge_history_before(&self, room_id: &str, timestamp: i64) -> ApiResult<u64> {
        self.event_storage
            .delete_events_before(room_id, timestamp)
            .await
            .map_err(|e| ApiError::internal_with_log("Failed to purge history", &e))
    }

    pub async fn get_room_version(&self, room_id: &str) -> ApiResult<Option<String>> {
        self.room_storage
            .get_room_version_only(room_id)
            .await
            .map_err(|e| ApiError::database_with_log("Failed to get room version", &e))
    }

    pub async fn search_all_rooms_admin(
        &self,
        search_term: Option<&str>,
        limit: i64,
        order_by: RoomSearchOrder,
        cursor: Option<RoomSearchCursor>,
        is_public: Option<bool>,
        is_encrypted: Option<bool>,
    ) -> ApiResult<(Vec<serde_json::Value>, i64, Option<String>)> {
        self.room_storage
            .search_all_rooms_admin(search_term, limit, order_by, cursor, is_public, is_encrypted)
            .await
            .map_err(|e| ApiError::internal_with_log("Search failed", &e))
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
