//! Room summary state and sync operations.
//!
//! Extracted from `summary.rs` for file size reduction.

use crate::common::ApiError;
use crate::storage::room_summary::*;
use tracing::{info, instrument};

use super::service::RoomSummaryService;

impl RoomSummaryService {
    /// See [`update_state`].
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
            .map_err(|e| ApiError::internal_with_cause("Failed to update state", e))?;

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
            if let Some(request) = Self::summary_member_from_state(room_id, state_key, content) {
                // Propagate rather than swallow: a failed member projection would
                // otherwise leave `room_summaries` silently out of sync with the
                // real membership with no retry and no failure signal (see the
                // `room_summary_update_queue` retry path, which only retries when
                // the caller surfaces an error).
                self.storage.add_member(request).await.map_err(|e| {
                    ApiError::internal_with_cause(&format!("Failed to add/update member {state_key} in summary"), e)
                })?;
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
                    .map_err(|e| ApiError::internal_with_cause("Failed to update canonical alias", e))?;
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

        self.storage
            .update_summary(room_id, request)
            .await
            .map_err(|e| ApiError::internal_with_cause("Failed to update summary from state", e))?;

        Ok(())
    }

    /// Project a single `m.room.member` state event onto the
    /// `room_summary_members` shape. Returns `None` for events without a state
    /// key, which carry no member to project.
    fn summary_member_from_state(
        room_id: &str,
        state_key: &str,
        content: &serde_json::Value,
    ) -> Option<CreateSummaryMemberRequest> {
        if state_key.is_empty() {
            return None;
        }

        let membership = content.get("membership").and_then(|v| v.as_str()).unwrap_or("join").to_string();
        let display_name = content.get("displayname").and_then(|v| v.as_str()).map(|s| s.to_string());
        let avatar_url = content.get("avatar_url").and_then(|v| v.as_str()).map(|s| s.to_string());

        Some(CreateSummaryMemberRequest {
            room_id: room_id.to_string(),
            user_id: state_key.to_string(),
            display_name,
            avatar_url,
            membership,
            is_hero: None,
            last_active_ts: None,
        })
    }

    /// See [`get_state`].
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
            .map_err(|e| ApiError::internal_with_cause("Failed to get state", e))?;

        Ok(state)
    }

    /// See [`get_all_state`].
    #[instrument(skip(self))]
    pub async fn get_all_state(&self, room_id: &str) -> Result<Vec<RoomSummaryState>, ApiError> {
        let states = self
            .storage
            .get_all_state(room_id)
            .await
            .map_err(|e| ApiError::internal_with_cause("Failed to get all state", e))?;

        Ok(states)
    }

    /// See [`sync_summary_state_and_members`].
    pub(crate) async fn sync_summary_state_and_members(&self, room_id: &str) -> Result<(), ApiError> {
        let states_res = self.event_reader.get_state_events(room_id).await;

        let states = match states_res {
            Ok(s) => s,
            Err(e) => return Err(ApiError::internal_with_cause("Failed to get current state", e)),
        };

        info!(room_id = %room_id, state_event_count = states.len(), "Syncing room summary state events");

        // B.2: `m.room.member` state events are projected in memory and written
        // by the single batched member sync below, rather than by one
        // `add_member` per event. The per-member path costs an `ensure_summary`
        // SELECT plus a full `refresh_member_counts` aggregate each, and its
        // result is immediately overwritten by the batch anyway — an O(N)
        // round-trip amplification on rooms with many members.
        let mut summary_members: Vec<CreateSummaryMemberRequest> = Vec::new();

        if !states.is_empty() {
            // Batch upsert all state events in a single query to avoid N+1
            // round trips. The per-event summary derivation still runs in a
            // loop because each event type triggers different logic.
            let entries: Vec<RoomSummaryStateEntry> = states
                .iter()
                .map(|state| RoomSummaryStateEntry {
                    event_type: state.event_type.clone().unwrap_or_default(),
                    state_key: state.state_key.clone().unwrap_or_default(),
                    event_id: Some(state.event_id.clone()),
                    content: state.content.clone(),
                })
                .collect();

            self.storage
                .set_states_batch(room_id, &entries)
                .await
                .map_err(|e| ApiError::internal_with_cause("Failed to batch upsert room summary state", e))?;

            for state in &states {
                let event_type_str = state.event_type.as_deref().unwrap_or("");
                let state_key_str = state.state_key.as_deref().unwrap_or("");

                if event_type_str == "m.room.member" {
                    if let Some(member) = Self::summary_member_from_state(room_id, state_key_str, &state.content) {
                        summary_members.push(member);
                    }
                    continue;
                }

                self.update_summary_from_state(room_id, Some(event_type_str), state_key_str, &state.content).await?;
            }
        }

        if let Some(member_storage) = self.member_storage.as_ref() {
            let join_members_res = member_storage.get_room_members(room_id, "join").await;

            let join_members = match join_members_res {
                Ok(m) => m,
                Err(e) => return Err(ApiError::internal_with_cause("Failed to get room join members", e)),
            };

            let invite_members_res = member_storage.get_room_members(room_id, "invite").await;

            let invite_members = match invite_members_res {
                Ok(m) => m,
                Err(e) => return Err(ApiError::internal_with_cause("Failed to get room invite members", e)),
            };

            // Member storage is the source of truth for join/invite rows: unlike
            // the raw state event it carries `last_active_ts`, so it wins on a
            // `user_id` collision. State-derived rows for other memberships
            // (leave/ban) are kept.
            let mut merged: std::collections::HashMap<String, CreateSummaryMemberRequest> =
                summary_members.into_iter().map(|member| (member.user_id.clone(), member)).collect();

            for member in join_members.into_iter().chain(invite_members) {
                merged.insert(
                    member.user_id.clone(),
                    CreateSummaryMemberRequest {
                        room_id: room_id.to_string(),
                        user_id: member.user_id,
                        display_name: member.display_name,
                        avatar_url: member.avatar_url,
                        membership: member.membership,
                        is_hero: Some(false),
                        last_active_ts: member.joined_ts.or(member.updated_ts),
                    },
                );
            }

            summary_members = merged.into_values().collect();
        }

        if summary_members.is_empty() {
            return Ok(());
        }

        let member_count = summary_members.len();
        info!(room_id = %room_id, member_count, "Syncing room summary members");

        self.storage.add_members_batch(room_id, summary_members).await.map_err(|e| {
            ApiError::internal_with_cause(&format!("Failed to batch add {member_count} members during sync"), e)
        })?;

        Ok(())
    }

    /// See [`synchronize_room_snapshot`].
    pub(crate) async fn synchronize_room_snapshot(&self, room_id: &str) -> Result<(), ApiError> {
        self.sync_summary_state_and_members(room_id).await?;

        self.recalculate_stats(room_id).await?;

        self.recalculate_heroes(room_id).await?;

        Ok(())
    }

    /// See [`sync_from_room`].
    pub async fn sync_from_room(&self, room_id: &str) -> Result<RoomSummaryResponse, ApiError> {
        info!(room_id = %room_id, "Syncing room summary from room");

        let existing_res = self.storage.get_summary(room_id).await;

        let existing = match existing_res {
            Ok(e) => e,
            Err(e) => return Err(ApiError::internal_with_cause("Failed to check existing summary", e)),
        };

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

        let final_summary_res = self.get_summary(room_id).await;
        match final_summary_res {
            Ok(Some(s)) => Ok(s),
            Ok(None) => Err(ApiError::not_found("Room summary not found")),
            Err(e) => Err(e),
        }
    }
}
