//! Room summary stats and queue operations.
//!
//! Extracted from `summary.rs` for file size reduction.

use crate::common::{ApiError, ApiResult};
use crate::storage::room_summary::*;
use tracing::{warn, instrument};

use super::summary::RoomSummaryService;

impl RoomSummaryService {
    #[instrument(skip(self))]
    pub async fn get_stats(&self, room_id: &str) -> Result<Option<RoomSummaryStats>, ApiError> {
        let stats_res = self
            .storage
            .get_stats(room_id)
            .await;
        
        match stats_res {
            Ok(s) => Ok(s),
            Err(e) => Err(ApiError::internal_with_log("Failed to get stats", &e)),
        }
    }

    #[instrument(skip(self))]
    pub async fn recalculate_stats(&self, room_id: &str) -> Result<RoomSummaryStats, ApiError> {
        let events_res = self
            .event_storage
            .get_room_events(room_id, i64::MAX)
            .await;
        
        let events = match events_res {
            Ok(e) => e,
            Err(e) => return Err(ApiError::internal_with_log("Failed to get events", &e)),
        };

        let total_events = events.len() as i64;
        let total_state_events = events.iter().filter(|e| e.state_key.is_some()).count() as i64;
        let total_messages = events.iter().filter(|e| e.event_type == "m.room.message").count() as i64;
        let total_media = events
            .iter()
            .filter(|e| {
                e.event_type == "m.room.message"
                    && e.content
                        .get("msgtype")
                        .and_then(|v| v.as_str())
                        .is_some_and(|t| t == "m.image" || t == "m.video" || t == "m.file" || t == "m.audio")
            })
            .count() as i64;

        let stats_res = self
            .storage
            .update_stats(room_id, total_events, total_state_events, total_messages, total_media, 0)
            .await;
        
        match stats_res {
            Ok(s) => Ok(s),
            Err(e) => Err(ApiError::internal_with_log("Failed to update stats", &e)),
        }
    }

    #[instrument(skip(self))]
    pub async fn queue_update(
        &self,
        room_id: &str,
        event_id: &str,
        event_type: &str,
        state_key: Option<&str>,
    ) -> Result<(), ApiError> {
        let priority = if event_type.starts_with("m.room.") { 10 } else { 0 };

        let result = self.storage
            .queue_update(room_id, event_id, event_type, state_key, priority)
            .await;
        
        match result {
            Ok(_) => Ok(()),
            Err(e) => Err(ApiError::internal_with_log("Failed to queue update", &e)),
        }
    }

    pub async fn process_pending_updates(&self, limit: i64) -> ApiResult<usize> {
        let updates_res = self
            .storage
            .get_pending_updates(limit)
            .await;
        
        let updates = match updates_res {
            Ok(u) => u,
            Err(e) => return Err(ApiError::internal_with_log("Failed to get pending updates", &e)),
        };

        let mut processed = 0;
        for update in updates {
            let process_res: ApiResult<()> = self.process_update(&update).await;
            match process_res {
                Ok(_) => {
                    if let Err(e) = self.storage.mark_update_processed(update.id).await {
                        warn!(error = %e, update_id = update.id, "Failed to mark update processed");
                    }
                    processed += 1;
                }
                Err(e) => {
                    let mark_failed_res = self.storage.mark_update_failed(update.id, &e.to_string()).await;
                    if let Err(err) = mark_failed_res {
                        warn!(error = %err, update_id = update.id, cause = %e, "Failed to mark update failed");
                    }
                }
            }
        }

        Ok(processed)
    }

    async fn process_update(&self, update: &RoomSummaryUpdateQueueItem) -> Result<(), ApiError> {
        let event_res = self
            .event_storage
            .get_event(&update.event_id)
            .await;
        
        let event = match event_res {
            Ok(Some(e)) => e,
            Ok(None) => return Err(ApiError::not_found("Event not found")),
            Err(e) => return Err(ApiError::internal_with_log("Failed to get event", &e)),
        };

        if event.state_key.is_some() {
            self.update_state(
                &update.room_id,
                &event.event_type,
                event.state_key.as_deref().unwrap_or(""),
                Some(&event.event_id),
                event.content.clone(),
            )
            .await?;
        } else {
            let request = UpdateRoomSummaryRequest {
                last_event_id: Some(event.event_id.clone()),
                last_event_ts: Some(event.origin_server_ts),
                last_message_ts: if event.event_type == "m.room.message" { Some(event.origin_server_ts) } else { None },
                ..Default::default()
            };

            let update_res = self.storage
                .update_summary(&update.room_id, request)
                .await;
            if let Err(e) = update_res {
                return Err(ApiError::internal_with_log("Failed to update summary", &e));
            }
        }

        Ok(())
    }

    #[instrument(skip(self))]
    pub async fn increment_unread(&self, room_id: &str, highlight: bool) -> Result<(), ApiError> {
        let result = self.storage
            .increment_unread_notifications(room_id, highlight)
            .await;
        
        match result {
            Ok(_) => Ok(()),
            Err(e) => Err(ApiError::internal_with_log("Failed to increment unread", &e)),
        }
    }

    #[instrument(skip(self))]
    pub async fn clear_unread(&self, room_id: &str) -> Result<(), ApiError> {
        let result = self.storage
            .clear_unread_notifications(room_id)
            .await;
        
        match result {
            Ok(_) => Ok(()),
            Err(e) => Err(ApiError::internal_with_log("Failed to clear unread", &e)),
        }
    }

    #[instrument(skip(self))]
    pub async fn recalculate_heroes(&self, room_id: &str) -> Result<Vec<String>, ApiError> {
        let members_res = self
            .storage
            .get_hero_candidates(room_id, 5)
            .await;
        
        let members = match members_res {
            Ok(m) => m,
            Err(e) => return Err(ApiError::internal_with_log("Failed to get heroes", &e)),
        };

        let hero_ids: Vec<String> = members.iter().map(|m| m.user_id.clone()).collect();

        let set_hero_res = self.storage
            .set_hero_members(room_id, &hero_ids)
            .await;
        if let Err(e) = set_hero_res {
            return Err(ApiError::internal_with_log("Failed to update hero flags", &e));
        }

        let hero_users = match serde_json::to_value(&hero_ids) {
            Ok(v) => v,
            Err(e) => return Err(ApiError::internal_with_log("Failed to serialize heroes", &e)),
        };

        let request = UpdateRoomSummaryRequest { hero_users: Some(hero_users), ..Default::default() };

        let update_res = self.storage
            .update_summary(room_id, request)
            .await;
        if let Err(e) = update_res {
            return Err(ApiError::internal_with_log("Failed to update heroes", &e));
        }

        Ok(hero_ids)
    }
}
