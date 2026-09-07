use serde_json::Value;
use synapse_common::error::ApiError;
use synapse_storage::sliding_sync::{AdminRoomTokenSyncEntry, RoomTokenSyncCursor};
use synapse_storage::StateEvent;

use super::SlidingSyncService;
use crate::sync_helpers;

impl SlidingSyncService {
    /// See [`build_required_state_events`].
    pub(super) async fn build_required_state_events(
        &self,
        room_id: &str,
        required_state: Option<&Vec<Vec<String>>>,
    ) -> Result<Vec<Value>, sqlx::Error> {
        let Some(required_state) = required_state else {
            return Ok(Vec::new());
        };

        let cache_key = format!("room_state:{room_id}");

        // S10/N2: 该键的失效由 room service 的状态变更路径负责
        // （room/lifecycle/create.rs、messaging/events.rs、membership/*.rs 等
        // 在状态事件写入后删除 `room_state:{room_id}`）。此前此处另有一套
        // `sliding_sync:room:{user}:{device}:{conn}:{room}` 键的删除逻辑，
        // 但该键全仓无人写入，属永落空空操作，已随其无调用方的宿主方法一并删除。
        // Try cache first.
        let state_events: Vec<StateEvent> = match self.cache.get::<Vec<StateEvent>>(&cache_key).await {
            Ok(Some(cached)) => cached,
            _ => {
                let fetched = self.event_reader.get_state_events(room_id).await?;
                // Best-effort cache write; failure is non-fatal.
                let _ = self.cache.set(&cache_key, &fetched, 300).await;
                fetched
            }
        };
        Ok(state_events
            .into_iter()
            .filter(|event| Self::required_state_matches(required_state, event))
            .map(|event| sync_helpers::state_event_to_json(&event))
            .collect())
    }

    /// See [`required_state_matches`].
    pub(crate) fn required_state_matches(required_state: &[Vec<String>], event: &StateEvent) -> bool {
        let event_type = event.event_type.as_deref().unwrap_or_default();
        let state_key = event.state_key.as_deref().unwrap_or_default();
        required_state.iter().any(|entry| {
            let event_type_match = entry.first().is_some_and(|value| value == "*" || value == event_type);
            let state_key_match = entry.get(1).is_some_and(|value| value == "*" || value == state_key);
            event_type_match && state_key_match
        })
    }

    /// See [`cleanup_expired_tokens`].
    pub async fn cleanup_expired_tokens(&self) -> Result<u64, ApiError> {
        let count = self
            .storage
            .cleanup_expired_tokens()
            .await
            .map_err(|e| ApiError::internal_with_context("Failed to cleanup tokens", &e))?;

        Ok(count)
    }

    /// See [`get_room_token_sync`].
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
            .map_err(|e| ApiError::internal_with_context("Failed to list room token sync", &e))?;

        let total = self
            .storage
            .count_room_token_sync(room_id)
            .await
            .map_err(|e| ApiError::internal_with_context("Failed to count room token sync", &e))?;

        Ok((entries, total))
    }
}
