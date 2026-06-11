//! Room info queries: encryption status, deletion, user room lists, existence checks.

use crate::common::error::{ApiError, ApiResult};
use serde_json::json;
use sqlx::Row;

use super::service::RoomService;

impl RoomService {
    pub async fn get_room_encryption_status(
        &self,
        room_id: &str,
    ) -> ApiResult<crate::storage::room::RoomEncryptionStatus> {
        let room = self
            .room_storage
            .get_room(room_id)
            .await
            .map_err(|e| ApiError::internal_with_log("Failed to get room", &e))?
            .ok_or_else(|| ApiError::not_found("Room not found".to_string()))?;

        let is_encrypted_res = sqlx::query_scalar::<sqlx::Postgres, i32>(
            r#"SELECT 1 FROM events WHERE room_id = $1 AND event_type = 'm.room.encryption' AND state_key IS NOT NULL LIMIT 1"#,
        )
        .bind(room_id)
        .fetch_optional(&*self.room_storage.pool)
        .await;

        let is_encrypted = match is_encrypted_res {
            Ok(res) => res.is_some(),
            Err(e) => return Err(ApiError::internal_with_log("Failed to check room encryption", &e)),
        };

        let encryption_content_res = sqlx::query_scalar::<sqlx::Postgres, serde_json::Value>(
            r#"SELECT content FROM events WHERE room_id = $1 AND event_type = 'm.room.encryption' AND state_key IS NOT NULL ORDER BY origin_server_ts DESC LIMIT 1"#,
        )
        .bind(room_id)
        .fetch_optional(&*self.room_storage.pool)
        .await;

        let encryption_content = match encryption_content_res {
            Ok(content) => content,
            Err(e) => return Err(ApiError::internal_with_log("Failed to get encryption event content", &e)),
        };

        Ok(crate::storage::room::RoomEncryptionStatus::from_encryption_event(
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
        let rooms_res = sqlx::query(
            r#"
            SELECT rm.room_id, rm.membership,
                   COALESCE(r.name, '') AS name,
                   COALESCE(r.avatar_url, '') AS avatar_url,
                   rm.updated_ts
            FROM room_memberships rm
            LEFT JOIN rooms r ON rm.room_id = r.room_id
            WHERE rm.user_id = $1
            ORDER BY rm.updated_ts DESC
            "#,
        )
        .bind(user_id)
        .fetch_all(&*self.room_storage.pool)
        .await;

        let rooms = match rooms_res {
            Ok(r) => r,
            Err(e) => return Err(ApiError::internal_with_log("Failed to get user rooms", &e)),
        };

        Ok(rooms
            .into_iter()
            .map(|row| {
                json!({
                    "room_id": row.get::<String, _>("room_id"),
                    "membership": row.get::<String, _>("membership"),
                    "name": row.get::<String, _>("name"),
                    "avatar_url": row.get::<String, _>("avatar_url")
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