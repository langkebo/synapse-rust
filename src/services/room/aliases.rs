//! Room alias and directory operations.

use crate::common::error::{ApiError, ApiResult};
use serde_json::json;

use super::service::RoomService;
use super::utils::validate_room_alias_input;

impl RoomService {
    // ── Alias operations ──

    pub async fn get_room_aliases(&self, room_id: &str) -> ApiResult<Vec<String>> {
        self.room_storage
            .get_room_aliases(room_id)
            .await
            .map_err(|e| ApiError::internal_with_log("Failed to get room aliases", &e))
    }

    pub async fn set_room_alias(&self, room_id: &str, alias: &str, created_by: &str) -> ApiResult<()> {
        validate_room_alias_input(alias)?;
        self.room_storage
            .set_room_alias(room_id, alias, created_by)
            .await
            .map_err(|e| ApiError::internal_with_log("Failed to set room alias", &e))
    }

    pub async fn get_room_by_alias(&self, alias: &str) -> ApiResult<Option<String>> {
        validate_room_alias_input(alias)?;
        self.room_storage
            .get_room_by_alias(alias)
            .await
            .map_err(|e| ApiError::internal_with_log("Failed to get room by alias", &e))
    }

    pub async fn remove_room_alias(&self, room_id: &str) -> ApiResult<()> {
        self.room_storage
            .remove_room_alias(room_id)
            .await
            .map_err(|e| ApiError::internal_with_log("Failed to remove room alias", &e))
    }

    pub async fn remove_room_alias_by_name(&self, alias: &str) -> ApiResult<()> {
        self.room_storage
            .remove_room_alias_by_name(alias)
            .await
            .map_err(|e| ApiError::internal_with_log("Failed to remove room alias by name", &e))
    }

    // ── Directory / visibility operations ──

    pub async fn set_room_directory(&self, room_id: &str, is_public: bool) -> ApiResult<()> {
        self.room_storage
            .set_room_directory(room_id, is_public)
            .await
            .map_err(|e| ApiError::internal_with_log("Failed to set room directory", &e))
    }

    pub async fn get_room_visibility(&self, room_id: &str) -> ApiResult<String> {
        let is_public = self
            .room_storage
            .is_room_in_directory(room_id)
            .await
            .map_err(|e| ApiError::internal_with_log("Failed to get room visibility", &e))?;
        Ok(if is_public { "public".to_string() } else { "private".to_string() })
    }

    pub async fn remove_room_directory(&self, room_id: &str) -> ApiResult<()> {
        self.room_storage
            .remove_room_directory(room_id)
            .await
            .map_err(|e| ApiError::internal_with_log("Failed to remove room from directory", &e))
    }

    // ── Public rooms ──

    pub async fn get_public_rooms(&self, limit: i64) -> ApiResult<serde_json::Value> {
        let rooms = self
            .room_storage
            .get_public_rooms(limit)
            .await
            .map_err(|e| ApiError::internal_with_log("Failed to get public rooms", &e))?;

        let room_list: Vec<serde_json::Value> = rooms
            .iter()
            .map(|r| {
                json!({
                    "room_id": r.room_id,
                    "name": r.name,
                    "topic": r.topic,
                    "canonical_alias": r.canonical_alias,
                    "is_public": r.is_public,
                    "join_rule": r.join_rule
                })
            })
            .collect();

        Ok(json!({
            "chunk": room_list,
            "total_room_count_estimate": room_list.len() as i64
        }))
    }
}
