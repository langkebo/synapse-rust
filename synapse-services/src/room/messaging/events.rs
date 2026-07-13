//! Room event operations: state events, event CRUD, signatures, create_event.

use crate::common::error::{ApiError, ApiResult};
use serde_json::json;
use synapse_common::generate_event_id;
use synapse_storage::CreateEventParams;

use super::service::MessagingService;

impl MessagingService {
    pub async fn get_event_record(&self, event_id: &str) -> ApiResult<Option<synapse_storage::RoomEvent>> {
        self.event_reader.get_event(event_id).await.map_err(|e| ApiError::internal_with_log("Failed to get event", &e))
    }

    pub async fn get_event_record_in_room(
        &self,
        room_id: &str,
        event_id: &str,
    ) -> ApiResult<synapse_storage::RoomEvent> {
        let event = self
            .event_reader
            .get_event(event_id)
            .await
            .map_err(|e| ApiError::internal_with_log("Failed to get event", &e))?
            .ok_or_else(|| ApiError::not_found("Event not found".to_string()))?;

        if event.room_id != room_id {
            return Err(ApiError::bad_request("Event does not belong to this room".to_string()));
        }

        Ok(event)
    }

    pub async fn find_event_by_timestamp(
        &self,
        room_id: &str,
        ts: i64,
        forward: bool,
    ) -> ApiResult<Option<(String, i64)>> {
        self.event_reader
            .find_event_id_by_timestamp(room_id, ts, forward)
            .await
            .map_err(|e| ApiError::internal_with_log("Database error", &e))
    }

    pub async fn report_event(
        &self,
        event_id: &str,
        room_id: &str,
        reporter_user_id: &str,
        reason: Option<&str>,
        score: i32,
    ) -> ApiResult<i64> {
        self.event_writer
            .report_event(event_id, room_id, "", reporter_user_id, reason, score)
            .await
            .map_err(|e| ApiError::internal_with_log("Failed to report event", &e))
    }

    pub async fn get_state_events(&self, room_id: &str) -> ApiResult<Vec<serde_json::Value>> {
        let events = self
            .event_reader
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

    pub async fn get_state_event_records(&self, room_id: &str) -> ApiResult<Vec<synapse_storage::StateEvent>> {
        self.event_reader
            .get_state_events(room_id)
            .await
            .map_err(|e| ApiError::database_with_log("Failed to get room state", &e))
    }

    pub async fn get_state_events_at_or_before(
        &self,
        room_id: &str,
        origin_server_ts: i64,
    ) -> ApiResult<Vec<synapse_storage::StateEvent>> {
        self.event_reader
            .get_state_events_at_or_before(room_id, origin_server_ts)
            .await
            .map_err(|e| ApiError::database_with_log("Failed to get room state", &e))
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
            .event_writer
            .create_event(params, tx)
            .await
            .map_err(|e| ApiError::internal_with_log("Failed to create event", &e))?;

        // Invalidate room-state cache when a state event is written.
        // Best-effort: failure to delete is non-fatal.
        if state_key.is_some() {
            let _ = self.cache.delete(&format!("room_state:{room_id}")).await;
        }

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

        // Best-effort: sign and broadcast locally-produced events to
        // federation peers.  Skipped when a transaction is provided (the
        // caller owns the event lifecycle in that case).  Failures are
        // logged inside `sign_and_broadcast_event` and do not affect the
        // local event creation.
        if should_update_summary {
            if let Err(e) = self.sign_and_broadcast_event(&event).await {
                ::tracing::warn!(
                    event_id = %event.event_id,
                    room_id = %event.room_id,
                    event_type = %event.event_type,
                    error = %e,
                    "Failed to sign and broadcast event"
                );
            }
        }

        Ok(event)
    }

    /// Like `create_event` but also persists the PDU's DAG metadata
    /// (`prev_events`, `auth_events`, `depth`) and populates `event_edges`.
    /// Used by the inbound federation transaction handler so that
    /// `/get_missing_events` can walk the DAG and outbound backfill has the
    /// graph data it needs.
    pub async fn create_event_with_graph(
        &self,
        params: CreateEventParams,
        prev_events: &[String],
        auth_events: &[String],
        depth: i64,
        tx: Option<&mut sqlx::Transaction<'_, sqlx::Postgres>>,
    ) -> ApiResult<synapse_storage::RoomEvent> {
        let room_id = params.room_id.clone();
        let event_id = params.event_id.clone();
        let event_type = params.event_type.clone();
        let state_key = params.state_key.clone();
        let should_update_summary = tx.is_none();

        let event = self
            .event_writer
            .create_event_with_graph(params, prev_events, auth_events, depth, tx)
            .await
            .map_err(|e| ApiError::internal_with_log("Failed to create event with graph data", &e))?;

        // Invalidate room-state cache when a state event is written.
        // Best-effort: failure to delete is non-fatal.
        if state_key.is_some() {
            let _ = self.cache.delete(&format!("room_state:{room_id}")).await;
        }

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
            .event_reader
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
                redacts: None,
            },
            None,
        )
        .await
        .map_err(|e| ApiError::internal_with_log("Failed to persist pinned events state", &e))?;
        Ok(())
    }

    pub async fn get_event(&self, room_id: &str, event_id: &str) -> ApiResult<serde_json::Value> {
        let event = self
            .event_reader
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
        self.event_reader
            .get_pending_room_events(room_id, limit)
            .await
            .map_err(|e| ApiError::internal_with_log("Failed to get pending events", &e))
    }

    pub async fn get_room_events(&self, room_id: &str, limit: i64) -> ApiResult<Vec<synapse_storage::RoomEvent>> {
        self.event_reader
            .get_room_events(room_id, limit)
            .await
            .map_err(|e| ApiError::internal_with_log("Failed to get room events", &e))
    }

    pub async fn get_room_events_by_type(
        &self,
        room_id: &str,
        event_type: &str,
        limit: i64,
    ) -> ApiResult<Vec<synapse_storage::RoomEvent>> {
        self.event_reader
            .get_room_events_by_type(room_id, event_type, limit)
            .await
            .map_err(|e| ApiError::internal_with_log("Failed to get room events by type", &e))
    }

    pub async fn get_room_events_paginated_admin(
        &self,
        room_id: &str,
        from: Option<i64>,
        limit: i64,
        direction: &str,
    ) -> ApiResult<Vec<synapse_storage::RoomEvent>> {
        self.event_reader
            .get_room_events_paginated(room_id, from, limit, direction)
            .await
            .map_err(|e| ApiError::database_with_log("Failed to get room messages", &e))
    }

    pub async fn get_event_context_admin(
        &self,
        room_id: &str,
        event_id: &str,
        context_limit: i64,
    ) -> ApiResult<serde_json::Value> {
        let event = self
            .event_reader
            .get_event(event_id)
            .await
            .map_err(|e| ApiError::database_with_log("Failed to get event", &e))?
            .ok_or_else(|| ApiError::not_found("Event not found".to_string()))?;

        if event.room_id != room_id {
            return Err(ApiError::not_found("Event not found in this room".to_string()));
        }

        let events_before = self
            .event_reader
            .get_events_before_context(room_id, event.origin_server_ts, context_limit)
            .await
            .map_err(|e| ApiError::database_with_log("Failed to get preceding context", &e))?;

        let events_after = self
            .event_reader
            .get_events_after_context(room_id, event.origin_server_ts, context_limit)
            .await
            .map_err(|e| ApiError::database_with_log("Failed to get following context", &e))?;

        Ok(json!({
            "event": {
                "event_id": event.event_id,
                "type": event.event_type,
                "sender": event.user_id,
                "state_key": event.state_key,
                "content": event.content,
                "room_id": event.room_id,
                "origin_server_ts": event.origin_server_ts
            },
            "events_before": events_before,
            "events_after": events_after,
            "state": []
        }))
    }

    pub async fn search_room_messages_admin(
        &self,
        room_id: &str,
        search_pattern: &str,
        limit: i64,
    ) -> ApiResult<Vec<serde_json::Value>> {
        self.event_reader
            .search_room_messages_admin(room_id, search_pattern, limit)
            .await
            .map_err(|e| ApiError::internal_with_log("Search failed", &e))
    }

    pub async fn get_forward_extremities_count(&self, room_id: &str) -> ApiResult<i64> {
        self.event_reader
            .get_forward_extremities_count(room_id)
            .await
            .map_err(|e| ApiError::database_with_log("Failed to get forward extremities", &e))
    }

    pub async fn count_events_by_status(&self, room_id: &str, status: &str) -> i64 {
        self.event_reader.count_room_events_by_status(room_id, status).await.unwrap_or(0)
    }

    pub async fn redact_event_content(&self, event_id: &str, redacted_by: Option<&str>) -> ApiResult<()> {
        self.event_writer
            .redact_event_content(event_id, redacted_by)
            .await
            .map_err(|e| ApiError::internal_with_log("Failed to redact event content", &e))
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
        self.event_writer
            .save_event_signature(event_id, user_id, device_id, signature, key_id, algorithm, created_ts)
            .await
            .map_err(|e| ApiError::internal_with_log("Failed to save signature", &e))
    }

    pub async fn get_event_signatures(&self, event_id: &str) -> ApiResult<Vec<synapse_storage::event::EventSignature>> {
        self.event_reader
            .get_event_signatures(event_id)
            .await
            .map_err(|e| ApiError::internal_with_log("Failed to get signatures", &e))
    }

    #[tracing::instrument(skip(self))]
    pub async fn get_daily_message_count(&self) -> ApiResult<i64> {
        self.event_reader
            .get_daily_message_count()
            .await
            .map_err(|e| ApiError::internal_with_log("Failed to get daily message count", &e))
    }

    #[tracing::instrument(skip(self))]
    pub async fn find_missing_event_ids(&self, event_ids: &[String]) -> ApiResult<Vec<String>> {
        self.event_reader
            .find_missing_event_ids(event_ids)
            .await
            .map_err(|e| ApiError::internal_with_log("Failed to find missing event ids", &e))
    }

    #[tracing::instrument(skip(self))]
    pub async fn get_missing_events_between(
        &self,
        room_id: &str,
        earliest_events: &[String],
        latest_events: &[String],
        limit: i64,
    ) -> ApiResult<Vec<serde_json::Value>> {
        self.event_reader
            .get_missing_events_between(room_id, earliest_events, latest_events, limit)
            .await
            .map_err(|e| ApiError::internal_with_log("Failed to walk event DAG for missing events", &e))
    }
}
