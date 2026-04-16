use crate::common::ApiError;
use crate::storage::event::EventStorage;
use crate::storage::membership::RoomMemberStorage;
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
        Self {
            storage,
            event_storage,
            member_storage,
        }
    }

    #[instrument(skip(self))]
    pub async fn get_summary(
        &self,
        room_id: &str,
    ) -> Result<Option<RoomSummaryResponse>, ApiError> {
        let summary = self
            .storage
            .get_summary(room_id)
            .await
            .map_err(|e| ApiError::internal(format!("Failed to get room summary: {}", e)))?;

        if let Some(summary) = summary {
            let heroes = self.get_heroes(room_id).await?;
            Ok(Some(summary.to_response(heroes)))
        } else {
            Ok(None)
        }
    }

    #[instrument(skip(self))]
    pub async fn get_summaries_for_user(
        &self,
        user_id: &str,
    ) -> Result<Vec<RoomSummaryResponse>, ApiError> {
        let summaries = self
            .storage
            .get_summaries_for_user(user_id)
            .await
            .map_err(|e| ApiError::internal(format!("Failed to get user room summaries: {}", e)))?;

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
            .map_err(|e| ApiError::internal(format!("Failed to get heroes: {}", e)))?;

        Ok(members.into_iter().map(RoomSummaryHero::from).collect())
    }

    #[instrument(skip(self))]
    pub async fn create_summary(
        &self,
        request: CreateRoomSummaryRequest,
    ) -> Result<RoomSummaryResponse, ApiError> {
        info!("Creating room summary for: {}", request.room_id);

        let room_id = request.room_id.clone();

        if self
            .storage
            .get_summary(&room_id)
            .await
            .map_err(|e| ApiError::internal(format!("Failed to check room summary: {}", e)))?
            .is_some()
        {
            self.storage
                .update_summary(&room_id, Self::create_request_to_update_request(&request))
                .await
                .map_err(|e| ApiError::internal(format!("Failed to update room summary: {}", e)))?;
        } else {
            self.storage
                .create_summary(request)
                .await
                .map_err(|e| ApiError::internal(format!("Failed to create room summary: {}", e)))?;
        }

        self.synchronize_room_snapshot(&room_id).await?;

        self.get_summary(&room_id)
            .await
            .map_err(|e| ApiError::internal(format!("Failed to get summary after sync: {}", e)))?
            .ok_or_else(|| ApiError::not_found("Room summary not found after sync"))
    }

    fn create_request_to_update_request(
        request: &CreateRoomSummaryRequest,
    ) -> UpdateRoomSummaryRequest {
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
            .map_err(|e| ApiError::internal(format!("Failed to get current state: {}", e)))?;

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
                .map_err(|e| {
                    ApiError::internal(format!("Failed to get room join members: {}", e))
                })?;

            let invite_members = member_storage
                .get_room_members(room_id, "invite")
                .await
                .map_err(|e| {
                    ApiError::internal(format!("Failed to get room invite members: {}", e))
                })?;

            let all_members: Vec<_> = join_members.into_iter().chain(invite_members).collect();

            info!("Syncing {} members for room {}", all_members.len(), room_id);

            for member in all_members {
                let request = CreateSummaryMemberRequest {
                    room_id: room_id.to_string(),
                    user_id: member.user_id.clone(),
                    display_name: member.display_name.clone(),
                    avatar_url: member.avatar_url.clone(),
                    membership: member.membership.clone(),
                    is_hero: Some(false),
                    last_active_ts: member.joined_ts.or(member.updated_ts),
                };

                if let Err(e) = self.storage.add_member(request).await {
                    warn!("Failed to add member during sync: {}", e);
                }
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
            .map_err(|e| ApiError::internal(format!("Failed to update room summary: {}", e)))?;

        let heroes = self.get_heroes(room_id).await?;
        Ok(summary.to_response(heroes))
    }

    #[instrument(skip(self))]
    pub async fn delete_summary(&self, room_id: &str) -> Result<(), ApiError> {
        info!("Deleting room summary for: {}", room_id);

        self.storage
            .delete_summary(room_id)
            .await
            .map_err(|e| ApiError::internal(format!("Failed to delete room summary: {}", e)))?;

        Ok(())
    }

    #[instrument(skip(self))]
    pub async fn add_member(
        &self,
        request: CreateSummaryMemberRequest,
    ) -> Result<RoomSummaryMember, ApiError> {
        debug!(
            "Adding member {} to room {}",
            request.user_id, request.room_id
        );

        let member = self
            .storage
            .add_member(request)
            .await
            .map_err(|e| ApiError::internal(format!("Failed to add member: {}", e)))?;

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
            .map_err(|e| ApiError::internal(format!("Failed to update member: {}", e)))?;

        Ok(member)
    }

    #[instrument(skip(self))]
    pub async fn remove_member(&self, room_id: &str, user_id: &str) -> Result<(), ApiError> {
        debug!("Removing member {} from room {}", user_id, room_id);

        self.storage
            .remove_member(room_id, user_id)
            .await
            .map_err(|e| ApiError::internal(format!("Failed to remove member: {}", e)))?;

        Ok(())
    }

    #[instrument(skip(self))]
    pub async fn get_members(&self, room_id: &str) -> Result<Vec<RoomSummaryMember>, ApiError> {
        let members = self
            .storage
            .get_members(room_id)
            .await
            .map_err(|e| ApiError::internal(format!("Failed to get members: {}", e)))?;

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
            .map_err(|e| ApiError::internal(format!("Failed to update state: {}", e)))?;

        self.update_summary_from_state(room_id, Some(event_type), state_key, &state.content)
            .await?;

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
                let membership = content
                    .get("membership")
                    .and_then(|v| v.as_str())
                    .unwrap_or("join")
                    .to_string();

                let display_name = content
                    .get("displayname")
                    .and_then(|v| v.as_str())
                    .map(|s| s.to_string());

                let avatar_url = content
                    .get("avatar_url")
                    .and_then(|v| v.as_str())
                    .map(|s| s.to_string());

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
                request.name = content
                    .get("name")
                    .and_then(|v| v.as_str())
                    .map(|s| s.to_string());
            }
            Some("m.room.topic") => {
                request.topic = content
                    .get("topic")
                    .and_then(|v| v.as_str())
                    .map(|s| s.to_string());
            }
            Some("m.room.avatar") => {
                request.avatar_url = content
                    .get("url")
                    .and_then(|v| v.as_str())
                    .map(|s| s.to_string());
            }
            Some("m.room.canonical_alias") => {
                let canonical_alias = content
                    .get("alias")
                    .and_then(|v| v.as_str());
                self.storage
                    .set_canonical_alias(room_id, canonical_alias)
                    .await
                    .map_err(|e| {
                        ApiError::internal(format!("Failed to update canonical alias: {}", e))
                    })?;
                return Ok(());
            }
            Some("m.room.join_rules") => {
                request.join_rule = content
                    .get("join_rule")
                    .and_then(|v| v.as_str())
                    .map(|s| s.to_string());
            }
            Some("m.room.history_visibility") => {
                request.history_visibility = content
                    .get("history_visibility")
                    .and_then(|v| v.as_str())
                    .map(|s| s.to_string());
            }
            Some("m.room.guest_access") => {
                request.guest_access = content
                    .get("guest_access")
                    .and_then(|v| v.as_str())
                    .map(|s| s.to_string());
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
            .map_err(|e| ApiError::internal(format!("Failed to get state: {}", e)))?;

        Ok(state)
    }

    #[instrument(skip(self))]
    pub async fn get_all_state(&self, room_id: &str) -> Result<Vec<RoomSummaryState>, ApiError> {
        let states = self
            .storage
            .get_all_state(room_id)
            .await
            .map_err(|e| ApiError::internal(format!("Failed to get all state: {}", e)))?;

        Ok(states)
    }

    #[instrument(skip(self))]
    pub async fn get_stats(&self, room_id: &str) -> Result<Option<RoomSummaryStats>, ApiError> {
        let stats = self
            .storage
            .get_stats(room_id)
            .await
            .map_err(|e| ApiError::internal(format!("Failed to get stats: {}", e)))?;

        Ok(stats)
    }

    #[instrument(skip(self))]
    pub async fn recalculate_stats(&self, room_id: &str) -> Result<RoomSummaryStats, ApiError> {
        let events = self
            .event_storage
            .get_room_events(room_id, i64::MAX)
            .await
            .map_err(|e| ApiError::internal(format!("Failed to get events: {}", e)))?;

        let total_events = events.len() as i64;
        let total_state_events = events.iter().filter(|e| e.state_key.is_some()).count() as i64;
        let total_messages = events
            .iter()
            .filter(|e| e.event_type == "m.room.message")
            .count() as i64;
        let total_media = events
            .iter()
            .filter(|e| {
                e.event_type == "m.room.message"
                    && e.content
                        .get("msgtype")
                        .and_then(|v| v.as_str())
                        .map(|t| {
                            t.starts_with("m.image")
                                || t.starts_with("m.video")
                                || t.starts_with("m.file")
                                || t.starts_with("m.audio")
                        })
                        .unwrap_or(false)
            })
            .count() as i64;

        let stats = self
            .storage
            .update_stats(
                room_id,
                total_events,
                total_state_events,
                total_messages,
                total_media,
                0,
            )
            .await
            .map_err(|e| ApiError::internal(format!("Failed to update stats: {}", e)))?;

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
        let priority = if event_type.starts_with("m.room.") {
            10
        } else {
            0
        };

        self.storage
            .queue_update(room_id, event_id, event_type, state_key, priority)
            .await
            .map_err(|e| ApiError::internal(format!("Failed to queue update: {}", e)))?;

        Ok(())
    }

    #[instrument(skip(self))]
    pub async fn process_pending_updates(&self, limit: i64) -> Result<usize, ApiError> {
        let updates = self
            .storage
            .get_pending_updates(limit)
            .await
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
                    if let Err(err) = self
                        .storage
                        .mark_update_failed(update.id, &e.to_string())
                        .await
                    {
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
            .map_err(|e| ApiError::internal(format!("Failed to get event: {}", e)))?
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
                last_message_ts: if event.event_type == "m.room.message" {
                    Some(event.origin_server_ts)
                } else {
                    None
                },
                ..Default::default()
            };

            self.storage
                .update_summary(&update.room_id, request)
                .await
                .map_err(|e| ApiError::internal(format!("Failed to update summary: {}", e)))?;
        }

        Ok(())
    }

    #[instrument(skip(self))]
    pub async fn increment_unread(&self, room_id: &str, highlight: bool) -> Result<(), ApiError> {
        self.storage
            .increment_unread_notifications(room_id, highlight)
            .await
            .map_err(|e| ApiError::internal(format!("Failed to increment unread: {}", e)))?;

        Ok(())
    }

    #[instrument(skip(self))]
    pub async fn clear_unread(&self, room_id: &str) -> Result<(), ApiError> {
        self.storage
            .clear_unread_notifications(room_id)
            .await
            .map_err(|e| ApiError::internal(format!("Failed to clear unread: {}", e)))?;

        Ok(())
    }

    #[instrument(skip(self))]
    pub async fn recalculate_heroes(&self, room_id: &str) -> Result<Vec<String>, ApiError> {
        let members = self
            .storage
            .get_hero_candidates(room_id, 5)
            .await
            .map_err(|e| ApiError::internal(format!("Failed to get heroes: {}", e)))?;

        let hero_ids: Vec<String> = members.iter().map(|m| m.user_id.clone()).collect();

        self.storage
            .set_hero_members(room_id, &hero_ids)
            .await
            .map_err(|e| ApiError::internal(format!("Failed to update hero flags: {}", e)))?;

        let hero_users = serde_json::to_value(&hero_ids)
            .map_err(|e| ApiError::internal(format!("Failed to serialize heroes: {}", e)))?;

        let request = UpdateRoomSummaryRequest {
            hero_users: Some(hero_users),
            ..Default::default()
        };

        self.storage
            .update_summary(room_id, request)
            .await
            .map_err(|e| ApiError::internal(format!("Failed to update heroes: {}", e)))?;

        Ok(hero_ids)
    }

    pub async fn sync_from_room(&self, room_id: &str) -> Result<RoomSummaryResponse, ApiError> {
        info!("Syncing room summary from room: {}", room_id);

        let existing =
            self.storage.get_summary(room_id).await.map_err(|e| {
                ApiError::internal(format!("Failed to check existing summary: {}", e))
            })?;

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
}

#[cfg(test)]
mod tests {
    #[test]
    fn test_create_room_summary_request() {
        let request = crate::storage::room_summary::CreateRoomSummaryRequest {
            room_id: "!room:example.com".to_string(),
            room_type: Some("m.space".to_string()),
            name: Some("Test Room".to_string()),
            topic: Some("Test topic".to_string()),
            avatar_url: Some("mxc://example.com/avatar".to_string()),
            canonical_alias: Some("#test:example.com".to_string()),
            join_rule: Some("invite".to_string()),
            history_visibility: Some("shared".to_string()),
            guest_access: Some("can_join".to_string()),
            is_direct: Some(false),
            is_space: Some(true),
        };
        assert_eq!(request.room_id, "!room:example.com");
        assert!(request.is_space.unwrap());
    }

    #[test]
    fn test_update_room_summary_request() {
        let request = crate::storage::room_summary::UpdateRoomSummaryRequest {
            name: Some("Updated Name".to_string()),
            topic: None,
            avatar_url: None,
            canonical_alias: None,
            join_rule: Some("public".to_string()),
            history_visibility: None,
            guest_access: None,
            is_direct: None,
            is_space: None,
            is_encrypted: Some(true),
            last_event_id: Some("$event:example.com".to_string()),
            last_event_ts: Some(1234567890),
            last_message_ts: None,
            hero_users: None,
        };
        assert_eq!(request.name, Some("Updated Name".to_string()));
        assert!(request.is_encrypted.unwrap());
    }

    #[test]
    fn test_update_room_summary_request_default() {
        let request = crate::storage::room_summary::UpdateRoomSummaryRequest::default();
        assert!(request.name.is_none());
        assert!(request.topic.is_none());
        assert!(request.is_encrypted.is_none());
    }

    #[test]
    fn test_room_summary_structure() {
        let summary = crate::storage::room_summary::RoomSummary {
            id: Some(1),
            room_id: "!room:example.com".to_string(),
            room_type: Some("m.space".to_string()),
            name: Some("Test Room".to_string()),
            topic: Some("Topic".to_string()),
            avatar_url: None,
            canonical_alias: None,
            join_rule: "invite".to_string(),
            history_visibility: "shared".to_string(),
            guest_access: "forbidden".to_string(),
            is_direct: false,
            is_space: false,
            is_encrypted: false,
            member_count: 10,
            joined_member_count: 8,
            invited_member_count: 2,
            hero_users: serde_json::json!([]),
            last_event_id: None,
            last_event_ts: None,
            last_message_ts: None,
            unread_notifications: 0,
            unread_highlight: 0,
            updated_ts: Some(1234567890),
            created_ts: Some(1234567800),
        };
        assert_eq!(summary.member_count, 10);
        assert_eq!(summary.joined_member_count, 8);
    }

    #[test]
    fn test_room_summary_member_structure() {
        let member = crate::storage::room_summary::RoomSummaryMember {
            id: 1,
            room_id: "!room:example.com".to_string(),
            user_id: "@user:example.com".to_string(),
            display_name: Some("User".to_string()),
            avatar_url: Some("mxc://example.com/avatar".to_string()),
            membership: "join".to_string(),
            is_hero: true,
            last_active_ts: Some(1234567890),
            updated_ts: 1234567890,
            created_ts: 1234567800,
        };
        assert!(member.is_hero);
        assert_eq!(member.membership, "join");
    }

    #[test]
    fn test_room_summary_state_structure() {
        let state = crate::storage::room_summary::RoomSummaryState {
            id: 1,
            room_id: "!room:example.com".to_string(),
            event_type: "m.room.name".to_string(),
            state_key: "".to_string(),
            event_id: Some("$event:example.com".to_string()),
            content: serde_json::json!({"name": "Room Name"}),
            updated_ts: 1234567890,
        };
        assert_eq!(state.event_type, "m.room.name");
        assert!(state.event_id.is_some());
    }

    #[test]
    fn test_room_summary_stats_structure() {
        let stats = crate::storage::room_summary::RoomSummaryStats {
            id: 1,
            room_id: "!room:example.com".to_string(),
            total_events: 1000,
            total_state_events: 50,
            total_messages: 800,
            total_media: 100,
            storage_size: 1024000,
            last_updated_ts: 1234567890,
        };
        assert_eq!(stats.total_events, 1000);
        assert_eq!(stats.total_messages, 800);
    }

    #[test]
    fn test_create_summary_member_request() {
        let request = crate::storage::room_summary::CreateSummaryMemberRequest {
            room_id: "!room:example.com".to_string(),
            user_id: "@user:example.com".to_string(),
            display_name: Some("User".to_string()),
            avatar_url: None,
            membership: "join".to_string(),
            is_hero: Some(false),
            last_active_ts: None,
        };
        assert_eq!(request.user_id, "@user:example.com");
        assert!(!request.is_hero.unwrap());
    }

    #[test]
    fn test_update_summary_member_request() {
        let request = crate::storage::room_summary::UpdateSummaryMemberRequest {
            display_name: Some("New Name".to_string()),
            avatar_url: Some("mxc://example.com/new".to_string()),
            membership: Some("leave".to_string()),
            is_hero: Some(true),
            last_active_ts: Some(1234567890),
        };
        assert_eq!(request.membership, Some("leave".to_string()));
    }

    #[test]
    fn test_room_summary_update_queue_item() {
        let item = crate::storage::room_summary::RoomSummaryUpdateQueueItem {
            id: 1,
            room_id: "!room:example.com".to_string(),
            event_id: "$event:example.com".to_string(),
            event_type: "m.room.message".to_string(),
            state_key: None,
            priority: 0,
            status: "pending".to_string(),
            created_ts: 1234567890,
            processed_ts: None,
            error_message: None,
            retry_count: 0,
        };
        assert_eq!(item.status, "pending");
        assert!(item.state_key.is_none());
    }

    #[test]
    fn test_event_priority_calculation() {
        let state_event_type = "m.room.name";
        let message_event_type = "m.room.message";

        let state_priority = if state_event_type.starts_with("m.room.") {
            10
        } else {
            0
        };
        let message_priority = if message_event_type.starts_with("m.room.") {
            10
        } else {
            0
        };

        assert_eq!(state_priority, 10);
        assert_eq!(message_priority, 10);
    }
}
