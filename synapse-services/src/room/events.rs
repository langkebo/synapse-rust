//! Room event operations: state events, event CRUD, signatures, create_event.

use crate::common::error::{ApiError, ApiResult};
use serde_json::json;
use synapse_common::generate_event_id;
use synapse_storage::CreateEventParams;

use super::service::RoomService;

impl RoomService {
    pub async fn get_state_events(&self, room_id: &str) -> ApiResult<Vec<serde_json::Value>> {
        let events = self
            .event_storage
            .get_state_events(room_id)
            .await
            .map_err(|e| ApiError::internal_with_log("Failed to get state events", &e))?;

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

    pub async fn create_event(
        &self,
        params: CreateEventParams,
        tx: Option<&mut sqlx::Transaction<'_, sqlx::Postgres>>,
    ) -> ApiResult<synapse_storage::RoomEvent> {
        let room_id = params.room_id.clone();
        let event_id = params.event_id.clone();
        let event_type = params.event_type.clone();
        let state_key = params.state_key.clone();
        let should_update_summary = tx.is_none();

        let event = self
            .event_storage
            .create_event(params, tx)
            .await
            .map_err(|e| ApiError::internal_with_log("Failed to create event", &e))?;

        if should_update_summary && event_type == "m.room.canonical_alias" && state_key.as_deref() == Some("") {
            let canonical_alias = event.content.get("alias").and_then(|value| value.as_str());
            if let Err(error) = self.room_storage.set_canonical_alias(&room_id, canonical_alias).await {
                ::tracing::warn!(
                    error = %error,
                    room_id = %room_id,
                    canonical_alias = ?canonical_alias,
                    "Failed to project canonical alias onto room"
                );
            }
        }

        if should_update_summary {
            if let Err(error) =
                self.room_summary_service.queue_update(&room_id, &event_id, &event_type, state_key.as_deref()).await
            {
                ::tracing::warn!(
                    error = %error,
                    room_id = %room_id,
                    event_id = %event_id,
                    event_type = %event_type,
                    state_key = ?state_key,
                    "Failed to queue room summary update"
                );
            } else if let Err(error) = self.room_summary_service.process_pending_updates(32).await {
                ::tracing::warn!(error = %error, room_id = %room_id, batch_size = 32_u64, "Failed to process room summary updates");
            }
        }

        if should_update_summary {
            self.dispatch_appservice_event(
                &event.event_id,
                &event.room_id,
                &event.event_type,
                &event.user_id,
                &event.content,
                event.state_key.as_deref(),
            )
            .await;
        }

        Ok(event)
    }

    pub async fn get_state_events_by_type(&self, room_id: &str, event_type: &str) -> ApiResult<Vec<serde_json::Value>> {
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

    pub async fn get_pinned_event_ids(&self, room_id: &str) -> ApiResult<Vec<String>> {
        let state_events: Vec<serde_json::Value> =
            self.get_state_events_by_type(room_id, "m.room.pinned_events").await?;
        let pinned = state_events
            .first()
            .and_then(|event| event.get("content"))
            .and_then(|content| content.get("pinned").or_else(|| content.get("pinned_events")))
            .and_then(|value| value.as_array())
            .map(|entries| entries.iter().filter_map(|value| value.as_str().map(ToString::to_string)).collect())
            .unwrap_or_default();
        Ok(pinned)
    }

    pub async fn set_pinned_event_ids(
        &self,
        room_id: &str,
        user_id: &str,
        pinned_event_ids: &[String],
    ) -> ApiResult<()> {
        let event_id = generate_event_id(&self.server_name);
        let now = chrono::Utc::now().timestamp_millis();
        self.create_event(
            CreateEventParams {
                event_id,
                room_id: room_id.to_string(),
                user_id: user_id.to_string(),
                event_type: "m.room.pinned_events".to_string(),
                content: json!({ "pinned": pinned_event_ids }),
                state_key: Some(String::new()),
                origin_server_ts: now,
            },
            None,
        )
        .await
        .map_err(|e| ApiError::internal_with_log("Failed to persist pinned events state", &e))?;
        Ok(())
    }

    pub async fn get_event(&self, room_id: &str, event_id: &str) -> ApiResult<serde_json::Value> {
        let event = self
            .event_storage
            .get_event(event_id)
            .await
            .map_err(|e| ApiError::internal_with_log("Failed to get event", &e))?
            .ok_or_else(|| ApiError::not_found("Event not found".to_string()))?;

        if event.room_id != room_id {
            return Err(ApiError::not_found("Event not found in this room".to_string()));
        }

        Ok(json!({
            "event_id": event.event_id,
            "sender": event.user_id,
            "type": event.event_type,
            "content": event.content,
            "room_id": event.room_id,
            "origin_server_ts": event.origin_server_ts,
            "state_key": event.state_key,
        }))
    }

    pub async fn get_pending_events(&self, room_id: &str, limit: i64) -> ApiResult<Vec<synapse_storage::RoomEvent>> {
        self.event_storage
            .get_pending_room_events(room_id, limit)
            .await
            .map_err(|e| ApiError::internal_with_log("Failed to get pending events", &e))
    }

    pub async fn count_events_by_status(&self, room_id: &str, status: &str) -> i64 {
        self.event_storage.count_room_events_by_status(room_id, status).await.unwrap_or(0)
    }

    #[allow(clippy::too_many_arguments)]
    pub async fn save_event_signature(
        &self,
        event_id: &str,
        user_id: &str,
        device_id: &str,
        signature: &str,
        key_id: &str,
        algorithm: &str,
        created_ts: i64,
    ) -> ApiResult<()> {
        self.event_storage
            .save_event_signature(event_id, user_id, device_id, signature, key_id, algorithm, created_ts)
            .await
            .map_err(|e| ApiError::internal_with_log("Failed to save signature", &e))
    }

    pub async fn get_event_signatures(&self, event_id: &str) -> ApiResult<Vec<synapse_storage::event::EventSignature>> {
        self.event_storage
            .get_event_signatures(event_id)
            .await
            .map_err(|e| ApiError::internal_with_log("Failed to get signatures", &e))
    }
}
