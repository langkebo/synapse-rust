use crate::common::ApiError;
use crate::storage::room_summary::*;
use crate::storage::event::EventStorage;
use std::sync::Arc;
use tracing::{info, debug, warn, instrument};

pub struct RoomSummaryService {
    storage: Arc<RoomSummaryStorage>,
    event_storage: Arc<EventStorage>,
}

impl RoomSummaryService {
    pub fn new(storage: Arc<RoomSummaryStorage>, event_storage: Arc<EventStorage>) -> Self {
        Self { storage, event_storage }
    }

    #[instrument(skip(self))]
    pub async fn get_summary(&self, room_id: &str) -> Result<Option<RoomSummaryResponse>, ApiError> {
        let summary = self.storage.get_summary(room_id).await
            .map_err(|e| ApiError::internal(format!("Failed to get room summary: {}", e)))?;

        if let Some(summary) = summary {
            let heroes = self.get_heroes(room_id).await?;
            Ok(Some(summary.to_response(heroes)))
        } else {
            Ok(None)
        }
    }

    #[instrument(skip(self))]
    pub async fn get_summaries_for_user(&self, user_id: &str) -> Result<Vec<RoomSummaryResponse>, ApiError> {
        let summaries = self.storage.get_summaries_for_user(user_id).await
            .map_err(|e| ApiError::internal(format!("Failed to get user room summaries: {}", e)))?;

        let mut responses = Vec::new();
        for summary in summaries {
            let heroes = self.get_heroes(&summary.room_id).await?;
            responses.push(summary.to_response(heroes));
        }

        Ok(responses)
    }

    async fn get_heroes(&self, room_id: &str) -> Result<Vec<RoomSummaryHero>, ApiError> {
        let members = self.storage.get_heroes(room_id, 5).await
            .map_err(|e| ApiError::internal(format!("Failed to get heroes: {}", e)))?;

        Ok(members.into_iter().map(RoomSummaryHero::from).collect())
    }

    #[instrument(skip(self))]
    pub async fn create_summary(&self, request: CreateRoomSummaryRequest) -> Result<RoomSummaryResponse, ApiError> {
        info!("Creating room summary for: {}", request.room_id);

        let summary = self.storage.create_summary(request).await
            .map_err(|e| ApiError::internal(format!("Failed to create room summary: {}", e)))?;

        Ok(summary.to_response(Vec::new()))
    }

    #[instrument(skip(self))]
    pub async fn update_summary(&self, room_id: &str, request: UpdateRoomSummaryRequest) -> Result<RoomSummaryResponse, ApiError> {
        let summary = self.storage.update_summary(room_id, request).await
            .map_err(|e| ApiError::internal(format!("Failed to update room summary: {}", e)))?;

        let heroes = self.get_heroes(room_id).await?;
        Ok(summary.to_response(heroes))
    }

    #[instrument(skip(self))]
    pub async fn delete_summary(&self, room_id: &str) -> Result<(), ApiError> {
        info!("Deleting room summary for: {}", room_id);

        self.storage.delete_summary(room_id).await
            .map_err(|e| ApiError::internal(format!("Failed to delete room summary: {}", e)))?;

        Ok(())
    }

    #[instrument(skip(self))]
    pub async fn add_member(&self, request: CreateSummaryMemberRequest) -> Result<RoomSummaryMember, ApiError> {
        debug!("Adding member {} to room {}", request.user_id, request.room_id);

        let member = self.storage.add_member(request).await
            .map_err(|e| ApiError::internal(format!("Failed to add member: {}", e)))?;

        Ok(member)
    }

    #[instrument(skip(self))]
    pub async fn update_member(&self, room_id: &str, user_id: &str, request: UpdateSummaryMemberRequest) -> Result<RoomSummaryMember, ApiError> {
        let member = self.storage.update_member(room_id, user_id, request).await
            .map_err(|e| ApiError::internal(format!("Failed to update member: {}", e)))?;

        Ok(member)
    }

    #[instrument(skip(self))]
    pub async fn remove_member(&self, room_id: &str, user_id: &str) -> Result<(), ApiError> {
        debug!("Removing member {} from room {}", user_id, room_id);

        self.storage.remove_member(room_id, user_id).await
            .map_err(|e| ApiError::internal(format!("Failed to remove member: {}", e)))?;

        Ok(())
    }

    #[instrument(skip(self))]
    pub async fn get_members(&self, room_id: &str) -> Result<Vec<RoomSummaryMember>, ApiError> {
        let members = self.storage.get_members(room_id).await
            .map_err(|e| ApiError::internal(format!("Failed to get members: {}", e)))?;

        Ok(members)
    }

    #[instrument(skip(self))]
    pub async fn update_state(&self, room_id: &str, event_type: &str, state_key: &str, event_id: Option<&str>, content: serde_json::Value) -> Result<RoomSummaryState, ApiError> {
        let state = self.storage.set_state(room_id, event_type, state_key, event_id, content).await
            .map_err(|e| ApiError::internal(format!("Failed to update state: {}", e)))?;

        self.update_summary_from_state(room_id, event_type, state_key, &state.content).await?;

        Ok(state)
    }

    async fn update_summary_from_state(&self, room_id: &str, event_type: &str, state_key: &str, content: &serde_json::Value) -> Result<(), ApiError> {
        if !state_key.is_empty() {
            return Ok(());
        }

        let mut request = UpdateRoomSummaryRequest::default();

        match event_type {
            "m.room.name" => {
                request.name = content.get("name").and_then(|v| v.as_str()).map(|s| s.to_string());
            }
            "m.room.topic" => {
                request.topic = content.get("topic").and_then(|v| v.as_str()).map(|s| s.to_string());
            }
            "m.room.avatar" => {
                request.avatar_url = content.get("url").and_then(|v| v.as_str()).map(|s| s.to_string());
            }
            "m.room.canonical_alias" => {
                request.canonical_alias = content.get("alias").and_then(|v| v.as_str()).map(|s| s.to_string());
            }
            "m.room.join_rules" => {
                request.join_rules = content.get("join_rule").and_then(|v| v.as_str()).map(|s| s.to_string());
            }
            "m.room.history_visibility" => {
                request.history_visibility = content.get("history_visibility").and_then(|v| v.as_str()).map(|s| s.to_string());
            }
            "m.room.guest_access" => {
                request.guest_access = content.get("guest_access").and_then(|v| v.as_str()).map(|s| s.to_string());
            }
            "m.room.encryption" => {
                request.is_encrypted = Some(true);
            }
            _ => return Ok(()),
        }

        if let Err(e) = self.storage.update_summary(room_id, request).await {
            warn!("Failed to update summary from state: {}", e);
        }

        Ok(())
    }

    #[instrument(skip(self))]
    pub async fn get_state(&self, room_id: &str, event_type: &str, state_key: &str) -> Result<Option<RoomSummaryState>, ApiError> {
        let state = self.storage.get_state(room_id, event_type, state_key).await
            .map_err(|e| ApiError::internal(format!("Failed to get state: {}", e)))?;

        Ok(state)
    }

    #[instrument(skip(self))]
    pub async fn get_all_state(&self, room_id: &str) -> Result<Vec<RoomSummaryState>, ApiError> {
        let states = self.storage.get_all_state(room_id).await
            .map_err(|e| ApiError::internal(format!("Failed to get all state: {}", e)))?;

        Ok(states)
    }

    #[instrument(skip(self))]
    pub async fn get_stats(&self, room_id: &str) -> Result<Option<RoomSummaryStats>, ApiError> {
        let stats = self.storage.get_stats(room_id).await
            .map_err(|e| ApiError::internal(format!("Failed to get stats: {}", e)))?;

        Ok(stats)
    }

    #[instrument(skip(self))]
    pub async fn recalculate_stats(&self, room_id: &str) -> Result<RoomSummaryStats, ApiError> {
        let events = self.event_storage.get_room_events(room_id, i64::MAX).await
            .map_err(|e| ApiError::internal(format!("Failed to get events: {}", e)))?;

        let total_events = events.len() as i64;
        let total_state_events = events.iter().filter(|e| e.state_key.is_some()).count() as i64;
        let total_messages = events.iter().filter(|e| e.event_type == "m.room.message").count() as i64;
        let total_media = events.iter()
            .filter(|e| {
                e.event_type == "m.room.message" && 
                e.content.get("msgtype").and_then(|v| v.as_str())
                    .map(|t| t.starts_with("m.image") || t.starts_with("m.video") || t.starts_with("m.file") || t.starts_with("m.audio"))
                    .unwrap_or(false)
            })
            .count() as i64;

        let stats = self.storage.update_stats(room_id, total_events, total_state_events, total_messages, total_media, 0).await
            .map_err(|e| ApiError::internal(format!("Failed to update stats: {}", e)))?;

        Ok(stats)
    }

    #[instrument(skip(self))]
    pub async fn queue_update(&self, room_id: &str, event_id: &str, event_type: &str, state_key: Option<&str>) -> Result<(), ApiError> {
        let priority = if event_type.starts_with("m.room.") { 10 } else { 0 };

        self.storage.queue_update(room_id, event_id, event_type, state_key, priority).await
            .map_err(|e| ApiError::internal(format!("Failed to queue update: {}", e)))?;

        Ok(())
    }

    #[instrument(skip(self))]
    pub async fn process_pending_updates(&self, limit: i64) -> Result<usize, ApiError> {
        let updates = self.storage.get_pending_updates(limit).await
            .map_err(|e| ApiError::internal(format!("Failed to get pending updates: {}", e)))?;

        let mut processed = 0;
        for update in updates {
            match self.process_update(&update).await {
                Ok(_) => {
                    if let Err(e) = self.storage.mark_update_processed(update.id).await {
                        warn!("Failed to mark update processed: {}", e);
                    }
                    processed += 1;
                }
                Err(e) => {
                    if let Err(err) = self.storage.mark_update_failed(update.id, &e.to_string()).await {
                        warn!("Failed to mark update failed: {}", err);
                    }
                }
            }
        }

        Ok(processed)
    }

    async fn process_update(&self, update: &RoomSummaryUpdateQueueItem) -> Result<(), ApiError> {
        let event = self.event_storage.get_event(&update.event_id).await
            .map_err(|e| ApiError::internal(format!("Failed to get event: {}", e)))?
            .ok_or_else(|| ApiError::not_found("Event not found"))?;

        if event.state_key.is_some() {
            self.update_state(
                &update.room_id,
                &event.event_type,
                event.state_key.as_deref().unwrap_or(""),
                Some(&event.event_id),
                event.content.clone(),
            ).await?;
        } else {
            let request = UpdateRoomSummaryRequest {
                last_event_id: Some(event.event_id.clone()),
                last_event_ts: Some(event.origin_server_ts),
                last_message_ts: if event.event_type == "m.room.message" {
                    Some(event.origin_server_ts)
                } else {
                    None
                },
                ..Default::default()
            };

            self.storage.update_summary(&update.room_id, request).await
                .map_err(|e| ApiError::internal(format!("Failed to update summary: {}", e)))?;
        }

        Ok(())
    }

    #[instrument(skip(self))]
    pub async fn increment_unread(&self, room_id: &str, highlight: bool) -> Result<(), ApiError> {
        self.storage.increment_unread_notifications(room_id, highlight).await
            .map_err(|e| ApiError::internal(format!("Failed to increment unread: {}", e)))?;

        Ok(())
    }

    #[instrument(skip(self))]
    pub async fn clear_unread(&self, room_id: &str) -> Result<(), ApiError> {
        self.storage.clear_unread_notifications(room_id).await
            .map_err(|e| ApiError::internal(format!("Failed to clear unread: {}", e)))?;

        Ok(())
    }

    #[instrument(skip(self))]
    pub async fn recalculate_heroes(&self, room_id: &str) -> Result<Vec<String>, ApiError> {
        let members = self.storage.get_heroes(room_id, 5).await
            .map_err(|e| ApiError::internal(format!("Failed to get heroes: {}", e)))?;

        let hero_ids: Vec<String> = members.iter().map(|m| m.user_id.clone()).collect();

        let hero_users = serde_json::to_value(&hero_ids)
            .map_err(|e| ApiError::internal(format!("Failed to serialize heroes: {}", e)))?;

        let request = UpdateRoomSummaryRequest {
            hero_users: Some(hero_users),
            ..Default::default()
        };

        self.storage.update_summary(room_id, request).await
            .map_err(|e| ApiError::internal(format!("Failed to update heroes: {}", e)))?;

        Ok(hero_ids)
    }

    pub async fn sync_from_room(&self, room_id: &str) -> Result<RoomSummaryResponse, ApiError> {
        info!("Syncing room summary from room: {}", room_id);

        let existing = self.storage.get_summary(room_id).await
            .map_err(|e| ApiError::internal(format!("Failed to check existing summary: {}", e)))?;

        if existing.is_none() {
            let request = CreateRoomSummaryRequest {
                room_id: room_id.to_string(),
                room_type: None,
                name: None,
                topic: None,
                avatar_url: None,
                canonical_alias: None,
                join_rules: None,
                history_visibility: None,
                guest_access: None,
                is_direct: None,
                is_space: None,
            };

            self.create_summary(request).await?;
        }

        let states = self.event_storage.get_state_events(room_id).await
            .map_err(|e| ApiError::internal(format!("Failed to get current state: {}", e)))?;

        for state in states {
            self.update_state(
                room_id,
                &state.event_type,
                state.state_key.as_deref().unwrap_or(""),
                Some(&state.event_id),
                state.content.clone(),
            ).await?;
        }

        self.get_summary(room_id).await
            .transpose()
            .unwrap_or_else(|| Err(ApiError::not_found("Room summary not found")))
    }
}
