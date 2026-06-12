//! Room membership actions: join, leave, forget.

use crate::common::error::{ApiError, ApiResult};
use crate::common::generate_event_id;
use crate::storage::CreateEventParams;
use serde_json::json;

use super::service::RoomService;

impl RoomService {
    #[::tracing::instrument(skip(self))]
    pub async fn join_room(&self, room_id: &str, user_id: &str) -> ApiResult<()> {
        if !self
            .room_storage
            .room_exists(room_id)
            .await
            .map_err(|e| ApiError::internal_with_log("Failed to check room", &e))?
        {
            return Err(ApiError::not_found("Room not found".to_string()));
        }

        if !self
            .user_storage
            .user_exists(user_id)
            .await
            .map_err(|e| ApiError::internal_with_log("Failed to check user existence", &e))?
        {
            return Err(ApiError::not_found("User not found".to_string()));
        }

        let state_events_res = self.event_storage.get_state_events_by_type(room_id, "m.room.join_rules").await;

        let state_events = match state_events_res {
            Ok(events) => events,
            Err(e) => return Err(ApiError::internal_with_log("Failed to load room join rules", &e)),
        };

        let effective_join_rule = if let Some(event) =
            state_events.into_iter().find(|event| event.state_key.as_deref().unwrap_or_default().is_empty())
        {
            event.content.get("join_rule").and_then(|value| value.as_str()).map(|value| value.to_string())
        } else {
            None
        };

        let room = self
            .room_storage
            .get_room(room_id)
            .await
            .map_err(|e| ApiError::internal_with_log("Failed to load room", &e))?
            .ok_or_else(|| ApiError::not_found("Room not found".to_string()))?;

        let join_rule = effective_join_rule
            .or_else(|| (!room.join_rule.is_empty()).then(|| room.join_rule.clone()))
            .unwrap_or_else(|| if room.is_public { "public".to_string() } else { "invite".to_string() });

        let existing_member = self
            .member_storage
            .get_room_member(room_id, user_id)
            .await
            .map_err(|e| ApiError::internal_with_log("Failed to check membership", &e))?;

        if let Some(member) = existing_member.as_ref() {
            if member.membership == "join" {
                return Ok(());
            }

            if member.membership == "ban" || member.is_banned.unwrap_or(false) {
                return Err(ApiError::forbidden("You are banned from this room".to_string()));
            }
        }

        if join_rule != "public" && existing_member.as_ref().is_none_or(|member| member.membership != "invite") {
            return Err(ApiError::forbidden("Room is invite-only".to_string()));
        }

        self.member_storage
            .add_member(room_id, user_id, "join", None, None, None, None)
            .await
            .map_err(|e| ApiError::internal_with_log("Failed to join room", &e))?;

        self.room_storage
            .increment_member_count(room_id)
            .await
            .map_err(|e| ApiError::internal_with_log("Failed to update member count", &e))?;

        // Persist the m.room.member state event so /sync delivers the membership
        // change and the client knows it has actually joined.
        self.create_event(
            CreateEventParams {
                event_id: generate_event_id(&self.server_name),
                room_id: room_id.to_string(),
                user_id: user_id.to_string(),
                event_type: "m.room.member".to_string(),
                content: json!({
                    "membership": "join",
                    "displayname": user_id.trim_start_matches('@').split(':').next().unwrap_or(user_id),
                }),
                state_key: Some(user_id.to_string()),
                origin_server_ts: chrono::Utc::now().timestamp_millis(),
            },
            None,
        )
            .await
            .map_err(|e| ApiError::internal_with_log("Failed to record m.room.member join event", &e))?;

        Ok(())
    }

    #[::tracing::instrument(skip(self))]
    pub async fn leave_room(&self, room_id: &str, user_id: &str) -> ApiResult<()> {
        self.member_storage
            .remove_member(room_id, user_id)
            .await
            .map_err(|e| ApiError::internal_with_log("Failed to leave room", &e))?;

        self.room_storage
            .decrement_member_count(room_id)
            .await
            .map_err(|e| ApiError::internal_with_log("Failed to update member count", &e))?;

        // Persist the m.room.member leave event for /sync.
        self.create_event(
            CreateEventParams {
                event_id: generate_event_id(&self.server_name),
                room_id: room_id.to_string(),
                user_id: user_id.to_string(),
                event_type: "m.room.member".to_string(),
                content: json!({ "membership": "leave" }),
                state_key: Some(user_id.to_string()),
                origin_server_ts: chrono::Utc::now().timestamp_millis(),
            },
            None,
        )
            .await
            .map_err(|e| ApiError::internal_with_log("Failed to record m.room.member leave event", &e))?;

        Ok(())
    }

    pub async fn forget_room(&self, room_id: &str, user_id: &str) -> ApiResult<()> {
        let membership = self
            .member_storage
            .get_room_member(room_id, user_id)
            .await
            .map_err(|e| ApiError::internal_with_log("Failed to check membership", &e))?;

        match membership {
            Some(member) => match member.membership.as_str() {
                "join" => {
                    return Err(ApiError::bad_request(
                        "Cannot forget a room you are still joined to. Leave the room first.".to_string(),
                    ));
                }
                "ban" => {
                    return Err(ApiError::forbidden("Cannot forget a room you have been banned from.".to_string()));
                }
                "leave" | "invite" => {
                    self.member_storage
                        .forget_member(room_id, user_id)
                        .await
                        .map_err(|e| ApiError::internal_with_log("Failed to forget room", &e))?;
                }
                _ => {
                    return Err(ApiError::bad_request(format!("Unknown membership state: {}", member.membership)));
                }
            },
            None => {
                return Err(ApiError::not_found("No membership record found for this room".to_string()));
            }
        }

        Ok(())
    }
}
