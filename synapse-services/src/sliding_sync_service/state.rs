use serde_json::Value;
use synapse_common::error::ApiError;
use synapse_storage::sliding_sync::{AdminRoomTokenSyncEntry, RoomTokenSyncCursor};
use synapse_storage::StateEvent;

use super::SlidingSyncService;
use crate::sync_helpers;

impl SlidingSyncService {
    pub(super) async fn build_required_state_events(
        &self,
        room_id: &str,
        required_state: Option<&Vec<Vec<String>>>,
    ) -> Result<Vec<Value>, sqlx::Error> {
        let Some(required_state) = required_state else {
            return Ok(Vec::new());
        };

        let state_events = self.event_storage.get_state_events(room_id).await?;
        Ok(state_events
            .into_iter()
            .filter(|event| Self::required_state_matches(required_state, event))
            .map(|event| sync_helpers::state_event_to_json(&event))
            .collect())
    }

    pub(crate) fn required_state_matches(required_state: &[Vec<String>], event: &StateEvent) -> bool {
        let event_type = event.event_type.as_deref().unwrap_or_default();
        let state_key = event.state_key.as_deref().unwrap_or_default();
        required_state.iter().any(|entry| {
            let event_type_match = entry.first().is_some_and(|value| value == "*" || value == event_type);
            let state_key_match = entry.get(1).is_some_and(|value| value == "*" || value == state_key);
            event_type_match && state_key_match
        })
    }

    #[allow(clippy::too_many_arguments)]
    pub async fn update_room_state(
        &self,
        user_id: &str,
        device_id: &str,
        room_id: &str,
        conn_id: Option<&str>,
        bump_stamp: i64,
        highlight_count: i32,
        notification_count: i32,
        is_dm: bool,
        is_encrypted: bool,
        name: Option<&str>,
        avatar: Option<&str>,
    ) -> Result<(), ApiError> {
        self.storage
            .upsert_room(
                user_id,
                device_id,
                room_id,
                conn_id,
                None,
                bump_stamp,
                highlight_count,
                notification_count,
                is_dm,
                is_encrypted,
                false,
                false,
                name,
                avatar,
                bump_stamp,
            )
            .await
            .map_err(|e| ApiError::internal_with_log("Failed to update room state", &e))?;

        self.invalidate_room_cache(user_id, device_id, room_id, conn_id).await;

        Ok(())
    }

    pub async fn bump_room(
        &self,
        user_id: &str,
        device_id: &str,
        room_id: &str,
        conn_id: Option<&str>,
        bump_stamp: i64,
    ) -> Result<(), ApiError> {
        self.storage
            .bump_room(user_id, device_id, room_id, conn_id, bump_stamp)
            .await
            .map_err(|e| ApiError::internal_with_log("Failed to bump room", &e))?;

        self.invalidate_room_cache(user_id, device_id, room_id, conn_id).await;

        Ok(())
    }

    pub async fn update_notification_counts(
        &self,
        user_id: &str,
        device_id: &str,
        room_id: &str,
        conn_id: Option<&str>,
        highlight_count: i32,
        notification_count: i32,
    ) -> Result<(), ApiError> {
        self.storage
            .update_notification_counts(user_id, device_id, room_id, conn_id, highlight_count, notification_count)
            .await
            .map_err(|e| ApiError::internal_with_log("Failed to update notifications", &e))?;

        self.invalidate_room_cache(user_id, device_id, room_id, conn_id).await;

        Ok(())
    }

    pub async fn remove_room(
        &self,
        user_id: &str,
        device_id: &str,
        room_id: &str,
        conn_id: Option<&str>,
    ) -> Result<(), ApiError> {
        self.storage
            .delete_room(user_id, device_id, room_id, conn_id)
            .await
            .map_err(|e| ApiError::internal_with_log("Failed to remove room", &e))?;

        self.invalidate_room_cache(user_id, device_id, room_id, conn_id).await;

        Ok(())
    }

    pub async fn cleanup_expired_tokens(&self) -> Result<u64, ApiError> {
        let count = self
            .storage
            .cleanup_expired_tokens()
            .await
            .map_err(|e| ApiError::internal_with_log("Failed to cleanup tokens", &e))?;

        Ok(count)
    }

    pub async fn get_room_token_sync(
        &self,
        room_id: &str,
        limit: i64,
        from: Option<RoomTokenSyncCursor>,
    ) -> Result<(Vec<AdminRoomTokenSyncEntry>, i64), ApiError> {
        let entries = self
            .storage
            .list_room_token_sync(room_id, limit, from.as_ref())
            .await
            .map_err(|e| ApiError::internal_with_log("Failed to list room token sync", &e))?;

        let total = self
            .storage
            .count_room_token_sync(room_id)
            .await
            .map_err(|e| ApiError::internal_with_log("Failed to count room token sync", &e))?;

        Ok((entries, total))
    }
}
