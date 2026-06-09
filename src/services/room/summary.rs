use crate::common::ApiError;
use crate::storage::event::EventStorage;
use crate::storage::membership::RoomMemberStorage;
pub use crate::storage::room_summary::{
    CreateRoomSummaryRequest, CreateSummaryMemberRequest, RoomSummaryMember, RoomSummaryResponse, RoomSummaryState,
    RoomSummaryStats, UpdateRoomSummaryRequest, UpdateSummaryMemberRequest,
};
use crate::storage::room_summary::*;
use std::sync::Arc;
use tracing::{debug, info, instrument, warn};

pub struct RoomSummaryService {
    storage: Arc<RoomSummaryStorage>,
    event_storage: Arc<EventStorage>,
    member_storage: Option<Arc<RoomMemberStorage>>,
}

impl RoomSummaryService {
    pub fn new(
        storage: Arc<RoomSummaryStorage>,
        event_storage: Arc<EventStorage>,
        member_storage: Option<Arc<RoomMemberStorage>>,
    ) -> Self {
        Self { storage, event_storage, member_storage }
    }

    #[instrument(skip(self))]
    pub async fn get_summary(&self, room_id: &str) -> Result<Option<RoomSummaryResponse>, ApiError> {
        let summary = self
            .storage
            .get_summary(room_id)
            .await
            .map_err(|e| ApiError::internal_with_log("Failed to get room summary", &e))?;

        if let Some(summary) = summary {
            let heroes = self.get_heroes(room_id).await?;
            Ok(Some(summary.to_response(heroes)))
        } else {
            Ok(None)
        }
    }

    #[instrument(skip(self))]
    pub async fn get_summaries_for_user(&self, user_id: &str) -> Result<Vec<RoomSummaryResponse>, ApiError> {
        let summaries = self
            .storage
            .get_summaries_for_user(user_id)
            .await
            .map_err(|e| ApiError::internal_with_log("Failed to get user room summaries", &e))?;

        let mut responses = Vec::new();
        for summary in summaries {
            let heroes = self.get_heroes(&summary.room_id).await?;
            responses.push(summary.to_response(heroes));
        }

        Ok(responses)
    }

    async fn get_heroes(&self, room_id: &str) -> Result<Vec<RoomSummaryHero>, ApiError> {
        let members = self
            .storage
            .get_heroes(room_id, 5)
            .await
            .map_err(|e| ApiError::internal_with_log("Failed to get heroes", &e))?;

        Ok(members.into_iter().map(RoomSummaryHero::from).collect())
    }

    #[instrument(skip(self))]
    pub async fn create_summary(&self, request: CreateRoomSummaryRequest) -> Result<RoomSummaryResponse, ApiError> {
        info!("Creating room summary for: {}", request.room_id);

        let room_id = request.room_id.clone();

        if self
            .storage
            .get_summary(&room_id)
            .await
            .map_err(|e| ApiError::internal_with_log("Failed to check room summary", &e))?
            .is_some()
        {
            self.storage
                .update_summary(&room_id, Self::create_request_to_update_request(&request))
                .await
                .map_err(|e| ApiError::internal_with_log("Failed to update room summary", &e))?;
        } else {
            self.storage
                .create_summary(request)
                .await
                .map_err(|e| ApiError::internal_with_log("Failed to create room summary", &e))?;
        }

        self.synchronize_room_snapshot(&room_id).await?;

        self.get_summary(&room_id)
            .await
            .map_err(|e| ApiError::internal_with_log("Failed to get summary after sync", &e))?
            .ok_or_else(|| ApiError::not_found("Room summary not found after sync"))
    }

    fn create_request_to_update_request(request: &CreateRoomSummaryRequest) -> UpdateRoomSummaryRequest {
        UpdateRoomSummaryRequest {
            name: request.name.clone(),
            topic: request.topic.clone(),
            avatar_url: request.avatar_url.clone(),
            canonical_alias: request.canonical_alias.clone(),
            join_rule: request.join_rule.clone(),
            history_visibility: request.history_visibility.clone(),
            guest_access: request.guest_access.clone(),
            is_direct: request.is_direct,
            is_space: request.is_space,
            ..Default::default()
        }
    }

    async fn sync_summary_state_and_members(&self, room_id: &str) -> Result<(), ApiError> {
        let states = self
            .event_storage
            .get_state_events(room_id)
            .await
            .map_err(|e| ApiError::internal_with_log("Failed to get current state", &e))?;

        info!("Syncing {} state events for room {}", states.len(), room_id);

        for state in states {
            let event_type_str = state.event_type.as_deref().unwrap_or("");
            self.update_state(
                room_id,
                event_type_str,
                state.state_key.as_deref().unwrap_or(""),
                Some(&state.event_id),
                state.content.clone(),
            )
            .await?;
        }

        if let Some(member_storage) = self.member_storage.as_ref() {
            let join_members = member_storage
                .get_room_members(room_id, "join")
                .await
                .map_err(|e| ApiError::internal_with_log("Failed to get room join members", &e))?;

            let invite_members = member_storage
                .get_room_members(room_id, "invite")
                .await
                .map_err(|e| ApiError::internal_with_log("Failed to get room invite members", &e))?;

            let all_members: Vec<_> = join_members.into_iter().chain(invite_members).collect();

            info!("Syncing {} members for room {}", all_members.len(), room_id);

            let requests: Vec<CreateSummaryMemberRequest> = all_members
                .into_iter()
                .map(|member| CreateSummaryMemberRequest {
                    room_id: room_id.to_string(),
                    user_id: member.user_id,
                    display_name: member.display_name,
                    avatar_url: member.avatar_url,
                    membership: member.membership,
                    is_hero: Some(false),
                    last_active_ts: member.joined_ts.or(member.updated_ts),
                })
                .collect();

            if let Err(e) = self.storage.add_members_batch(room_id, requests).await {
                warn!("Failed to batch add members during sync: {}", e);
            }
        }

        Ok(())
    }

    async fn synchronize_room_snapshot(&self, room_id: &str) -> Result<(), ApiError> {
        self.sync_summary_state_and_members(room_id).await?;
        self.recalculate_stats(room_id).await?;
        self.recalculate_heroes(room_id).await?;
        Ok(())
    }

    #[instrument(skip(self))]
    pub async fn update_summary(
        &self,
        room_id: &str,
        request: UpdateRoomSummaryRequest,
    ) -> Result<RoomSummaryResponse, ApiError> {
        let summary = self
            .storage
            .update_summary(room_id, request)
            .await
            .map_err(|e| ApiError::internal_with_log("Failed to update room summary", &e))?;

        let heroes = self.get_heroes(room_id).await?;
        Ok(summary.to_response(heroes))
    }

    #[instrument(skip(self))]
    pub async fn delete_summary(&self, room_id: &str) -> Result<(), ApiError> {
        info!("Deleting room summary for: {}", room_id);

        self.storage
            .delete_summary(room_id)
            .await
            .map_err(|e| ApiError::internal_with_log("Failed to delete room summary", &e))?;

        Ok(())
    }

    #[instrument(skip(self))]
    pub async fn add_member(&self, request: CreateSummaryMemberRequest) -> Result<RoomSummaryMember, ApiError> {
        debug!("Adding member {} to room {}", request.user_id, request.room_id);

        let member = self
            .storage
            .add_member(request)
            .await
            .map_err(|e| ApiError::internal_with_log("Failed to add member", &e))?;

        Ok(member)
    }

    #[instrument(skip(self))]
    pub async fn update_member(
        &self,
        room_id: &str,
        user_id: &str,
        request: UpdateSummaryMemberRequest,
    ) -> Result<RoomSummaryMember, ApiError> {
        let member = self
            .storage
            .update_member(room_id, user_id, request)
            .await
            .map_err(|e| ApiError::internal_with_log("Failed to update member", &e))?;

        Ok(member)
    }

    #[instrument(skip(self))]
    pub async fn remove_member(&self, room_id: &str, user_id: &str) -> Result<(), ApiError> {
        debug!("Removing member {} from room {}", user_id, room_id);

        self.storage
            .remove_member(room_id, user_id)
            .await
            .map_err(|e| ApiError::internal_with_log("Failed to remove member", &e))?;

        Ok(())
    }

    #[instrument(skip(self))]
    pub async fn get_members(&self, room_id: &str) -> Result<Vec<RoomSummaryMember>, ApiError> {
        let members = self
            .storage
            .get_members(room_id)
            .await
            .map_err(|e| ApiError::internal_with_log("Failed to get members", &e))?;

        Ok(members)
    }

    #[instrument(skip(self))]
    pub async fn update_state(
        &self,
        room_id: &str,
        event_type: &str,
        state_key: &str,
        event_id: Option<&str>,
        content: serde_json::Value,
    ) -> Result<RoomSummaryState, ApiError> {
        let state = self
            .storage
            .set_state(room_id, event_type, state_key, event_id, content)
            .await
            .map_err(|e| ApiError::internal_with_log("Failed to update state", &e))?;

        self.update_summary_from_state(room_id, Some(event_type), state_key, &state.content).await?;

        Ok(state)
    }

    async fn update_summary_from_state(
        &self,
        room_id: &str,
        event_type: Option<&str>,
        state_key: &str,
        content: &serde_json::Value,
    ) -> Result<(), ApiError> {
        if event_type == Some("m.room.member") {
            if !state_key.is_empty() {
                let membership = content.get("membership").and_then(|v| v.as_str()).unwrap_or("join").to_string();

                let display_name = content.get("displayname").and_then(|v| v.as_str()).map(|s| s.to_string());

                let avatar_url = content.get("avatar_url").and_then(|v| v.as_str()).map(|s| s.to_string());

                let request = CreateSummaryMemberRequest {
                    room_id: room_id.to_string(),
                    user_id: state_key.to_string(),
                    display_name,
                    avatar_url,
                    membership,
                    is_hero: None,
                    last_active_ts: None,
                };

                if let Err(e) = self.storage.add_member(request).await {
                    warn!("Failed to add/update member in summary: {}", e);
                }
            }
            return Ok(());
        }

        let mut request = UpdateRoomSummaryRequest::default();

        match event_type {
            Some("m.room.name") => {
                request.name = content.get("name").and_then(|v| v.as_str()).map(|s| s.to_string());
            }
            Some("m.room.topic") => {
                request.topic = content.get("topic").and_then(|v| v.as_str()).map(|s| s.to_string());
            }
            Some("m.room.avatar") => {
                request.avatar_url = content.get("url").and_then(|v| v.as_str()).map(|s| s.to_string());
            }
            Some("m.room.canonical_alias") => {
                let canonical_alias = content.get("alias").and_then(|v| v.as_str());
                self.storage
                    .set_canonical_alias(room_id, canonical_alias)
                    .await
                    .map_err(|e| ApiError::internal_with_log("Failed to update canonical alias", &e))?;
                return Ok(());
            }
            Some("m.room.join_rules") => {
                request.join_rule = content.get("join_rule").and_then(|v| v.as_str()).map(|s| s.to_string());
            }
            Some("m.room.history_visibility") => {
                request.history_visibility =
                    content.get("history_visibility").and_then(|v| v.as_str()).map(|s| s.to_string());
            }
            Some("m.room.guest_access") => {
                request.guest_access = content.get("guest_access").and_then(|v| v.as_str()).map(|s| s.to_string());
            }
            Some("m.room.encryption") => {
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
    pub async fn get_state(
        &self,
        room_id: &str,
        event_type: &str,
        state_key: &str,
    ) -> Result<Option<RoomSummaryState>, ApiError> {
        let state = self
            .storage
            .get_state(room_id, event_type, state_key)
            .await
            .map_err(|e| ApiError::internal_with_log("Failed to get state", &e))?;

        Ok(state)
    }

    #[instrument(skip(self))]
    pub async fn get_all_state(&self, room_id: &str) -> Result<Vec<RoomSummaryState>, ApiError> {
        let states = self
            .storage
            .get_all_state(room_id)
            .await
            .map_err(|e| ApiError::internal_with_log("Failed to get all state", &e))?;

        Ok(states)
    }

    #[instrument(skip(self))]
    pub async fn get_stats(&self, room_id: &str) -> Result<Option<RoomSummaryStats>, ApiError> {
        let stats = self
            .storage
            .get_stats(room_id)
            .await
            .map_err(|e| ApiError::internal_with_log("Failed to get stats", &e))?;

        Ok(stats)
    }

    #[instrument(skip(self))]
    pub async fn recalculate_stats(&self, room_id: &str) -> Result<RoomSummaryStats, ApiError> {
        let events = self
            .event_storage
            .get_room_events(room_id, i64::MAX)
            .await
            .map_err(|e| ApiError::internal_with_log("Failed to get events", &e))?;

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

        let stats = self
            .storage
            .update_stats(room_id, total_events, total_state_events, total_messages, total_media, 0)
            .await
            .map_err(|e| ApiError::internal_with_log("Failed to update stats", &e))?;

        Ok(stats)
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

        self.storage
            .queue_update(room_id, event_id, event_type, state_key, priority)
            .await
            .map_err(|e| ApiError::internal_with_log("Failed to queue update", &e))?;

        Ok(())
    }

    #[instrument(skip(self))]
    pub async fn process_pending_updates(&self, limit: i64) -> Result<usize, ApiError> {
        let updates = self
            .storage
            .get_pending_updates(limit)
            .await
            .map_err(|e| ApiError::internal_with_log("Failed to get pending updates", &e))?;

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
        let event = self
            .event_storage
            .get_event(&update.event_id)
            .await
            .map_err(|e| ApiError::internal_with_log("Failed to get event", &e))?
            .ok_or_else(|| ApiError::not_found("Event not found"))?;

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

            self.storage
                .update_summary(&update.room_id, request)
                .await
                .map_err(|e| ApiError::internal_with_log("Failed to update summary", &e))?;
        }

        Ok(())
    }

    #[instrument(skip(self))]
    pub async fn increment_unread(&self, room_id: &str, highlight: bool) -> Result<(), ApiError> {
        self.storage
            .increment_unread_notifications(room_id, highlight)
            .await
            .map_err(|e| ApiError::internal_with_log("Failed to increment unread", &e))?;

        Ok(())
    }

    #[instrument(skip(self))]
    pub async fn clear_unread(&self, room_id: &str) -> Result<(), ApiError> {
        self.storage
            .clear_unread_notifications(room_id)
            .await
            .map_err(|e| ApiError::internal_with_log("Failed to clear unread", &e))?;

        Ok(())
    }

    #[instrument(skip(self))]
    pub async fn recalculate_heroes(&self, room_id: &str) -> Result<Vec<String>, ApiError> {
        let members = self
            .storage
            .get_hero_candidates(room_id, 5)
            .await
            .map_err(|e| ApiError::internal_with_log("Failed to get heroes", &e))?;

        let hero_ids: Vec<String> = members.iter().map(|m| m.user_id.clone()).collect();

        self.storage
            .set_hero_members(room_id, &hero_ids)
            .await
            .map_err(|e| ApiError::internal_with_log("Failed to update hero flags", &e))?;

        let hero_users = serde_json::to_value(&hero_ids)
            .map_err(|e| ApiError::internal_with_log("Failed to serialize heroes", &e))?;

        let request = UpdateRoomSummaryRequest { hero_users: Some(hero_users), ..Default::default() };

        self.storage
            .update_summary(room_id, request)
            .await
            .map_err(|e| ApiError::internal_with_log("Failed to update heroes", &e))?;

        Ok(hero_ids)
    }

    pub async fn sync_from_room(&self, room_id: &str) -> Result<RoomSummaryResponse, ApiError> {
        info!("Syncing room summary from room: {}", room_id);

        let existing = self
            .storage
            .get_summary(room_id)
            .await
            .map_err(|e| ApiError::internal_with_log("Failed to check existing summary", &e))?;

        if existing.is_none() {
            let request = CreateRoomSummaryRequest {
                room_id: room_id.to_string(),
                room_type: None,
                name: None,
                topic: None,
                avatar_url: None,
                canonical_alias: None,
                join_rule: None,
                history_visibility: None,
                guest_access: None,
                is_direct: None,
                is_space: None,
            };

            return self.create_summary(request).await;
        }

        self.synchronize_room_snapshot(room_id).await?;

        self.get_summary(room_id)
            .await
            .transpose()
            .unwrap_or_else(|| Err(ApiError::not_found("Room summary not found")))
    }

    #[instrument(skip(self))]
    pub async fn get_summaries_by_ids(&self, room_ids: &[String]) -> Result<Vec<RoomSummaryResponse>, ApiError> {
        if room_ids.is_empty() {
            return Ok(Vec::new());
        }

        let summaries = self
            .storage
            .get_summaries_by_ids(room_ids)
            .await
            .map_err(|e| ApiError::internal_with_log("Failed to get room summaries", &e))?;

        let mut responses = Vec::with_capacity(summaries.len());
        for summary in summaries {
            let heroes = self.get_heroes(&summary.room_id).await?;
            responses.push(summary.to_response(heroes));
        }

        Ok(responses)
    }
}
