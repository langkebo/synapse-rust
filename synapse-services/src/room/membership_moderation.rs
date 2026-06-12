//! Room membership moderation: invite, knock, ban, unban, kick.

use crate::common::error::{ApiError, ApiResult};
use serde_json::json;
use synapse_common::generate_event_id;
use synapse_storage::CreateEventParams;

use super::service::RoomService;

impl RoomService {
    pub async fn invite_user(&self, room_id: &str, inviter_id: &str, invitee_id: &str) -> ApiResult<()> {
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
            .user_exists(invitee_id)
            .await
            .map_err(|e| ApiError::internal_with_log("Failed to check user existence", &e))?
        {
            return Err(ApiError::not_found("User not found".to_string()));
        }

        self.auth_service.can_invite_user(room_id, inviter_id).await?;

        self.member_storage
            .add_member(room_id, invitee_id, "invite", None, None, Some(inviter_id), None)
            .await
            .map_err(|e| ApiError::internal_with_log("Failed to create invite event", &e))?;

        self.event_storage
            .create_event(
                CreateEventParams {
                    event_id: generate_event_id(&self.server_name),
                    room_id: room_id.to_string(),
                    user_id: inviter_id.to_string(),
                    event_type: "m.room.member".to_string(),
                    content: json!({
                        "membership": "invite",
                        "displayname": invitee_id
                            .trim_start_matches('@')
                            .split(':')
                            .next()
                            .unwrap_or(invitee_id),
                    }),
                    state_key: Some(invitee_id.to_string()),
                    origin_server_ts: chrono::Utc::now().timestamp_millis(),
                },
                None,
            )
            .await
            .map_err(|e| ApiError::internal_with_log("Failed to record m.room.member invite event", &e))?;

        Ok(())
    }

    pub async fn knock_room(&self, room_id: &str, user_id: &str, reason: Option<&str>) -> ApiResult<()> {
        if !self
            .room_storage
            .room_exists(room_id)
            .await
            .map_err(|e| ApiError::internal_with_log("Failed to check room", &e))?
        {
            return Err(ApiError::not_found("Room not found".to_string()));
        }

        let effective_join_rule = if let Some(event) = self
            .event_storage
            .get_state_events_by_type(room_id, "m.room.join_rules")
            .await
            .map_err(|e| ApiError::internal_with_log("Failed to load room join rules", &e))?
            .into_iter()
            .find(|event| event.state_key.as_deref().unwrap_or_default().is_empty())
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
            match member.membership.as_str() {
                "join" => {
                    return Err(ApiError::bad_request("You are already joined to this room".to_string()));
                }
                "invite" => {
                    return Err(ApiError::forbidden("You have already been invited to this room".to_string()));
                }
                "ban" => {
                    return Err(ApiError::forbidden("You are banned from this room".to_string()));
                }
                "knock" => return Ok(()),
                _ => {}
            }
        }

        if join_rule != "knock" && join_rule != "knock_restricted" {
            return Err(ApiError::forbidden("Room does not allow knock".to_string()));
        }

        self.member_storage
            .add_member(room_id, user_id, "knock", None, reason, None, None)
            .await
            .map_err(|e| ApiError::internal_with_log("Failed to create knock event", &e))?;
        Ok(())
    }

    pub async fn ban_user(&self, room_id: &str, user_id: &str, banned_by: &str, reason: Option<&str>) -> ApiResult<()> {
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

        self.auth_service.can_ban_user(room_id, banned_by, user_id).await?;

        self.member_storage
            .ban_member(room_id, user_id, banned_by)
            .await
            .map_err(|e| ApiError::internal_with_log("Failed to ban user", &e))?;

        let event_id = generate_event_id(&self.server_name);
        let content = json!({
            "membership": "ban",
            "reason": reason.unwrap_or("")
        });

        self.event_storage
            .create_event(
                CreateEventParams {
                    event_id,
                    room_id: room_id.to_string(),
                    user_id: banned_by.to_string(),
                    event_type: "m.room.member".to_string(),
                    content,
                    state_key: Some(user_id.to_string()),
                    origin_server_ts: chrono::Utc::now().timestamp_millis(),
                },
                None,
            )
            .await
            .map_err(|e| ApiError::internal_with_log("Failed to record m.room.member ban event", &e))?;

        Ok(())
    }

    pub async fn unban_user(&self, room_id: &str, user_id: &str, unbanned_by: &str) -> ApiResult<()> {
        self.auth_service.can_unban_user(room_id, unbanned_by, user_id).await?;

        self.member_storage
            .unban_member(room_id, user_id)
            .await
            .map_err(|e| ApiError::internal_with_log("Failed to unban user", &e))?;

        let event_id = generate_event_id(&self.server_name);
        let content = json!({
            "membership": "leave"
        });

        self.event_storage
            .create_event(
                CreateEventParams {
                    event_id,
                    room_id: room_id.to_string(),
                    user_id: unbanned_by.to_string(),
                    event_type: "m.room.member".to_string(),
                    content,
                    state_key: Some(user_id.to_string()),
                    origin_server_ts: chrono::Utc::now().timestamp_millis(),
                },
                None,
            )
            .await
            .map_err(|e| ApiError::internal_with_log("Failed to record m.room.member unban event", &e))?;

        Ok(())
    }

    pub async fn kick_user(
        &self,
        room_id: &str,
        target_user_id: &str,
        kicked_by: &str,
        reason: Option<&str>,
    ) -> ApiResult<()> {
        if !self
            .room_storage
            .room_exists(room_id)
            .await
            .map_err(|e| ApiError::internal_with_log("Failed to check room existence", &e))?
        {
            return Err(ApiError::not_found("Room not found".to_string()));
        }

        if !self
            .user_storage
            .user_exists(target_user_id)
            .await
            .map_err(|e| ApiError::internal_with_log("Failed to check user existence", &e))?
        {
            return Err(ApiError::not_found("User not found".to_string()));
        }

        self.auth_service.can_kick_user(room_id, kicked_by, target_user_id).await?;

        self.member_storage
            .remove_member(room_id, target_user_id)
            .await
            .map_err(|e| ApiError::internal_with_log("Failed to kick user", &e))?;

        let event_id = generate_event_id(&self.server_name);
        let content = json!({
            "membership": "leave",
            "reason": reason.unwrap_or("")
        });

        self.event_storage
            .create_event(
                CreateEventParams {
                    event_id,
                    room_id: room_id.to_string(),
                    user_id: kicked_by.to_string(),
                    event_type: "m.room.member".to_string(),
                    content,
                    state_key: Some(target_user_id.to_string()),
                    origin_server_ts: chrono::Utc::now().timestamp_millis(),
                },
                None,
            )
            .await
            .map_err(|e| ApiError::internal_with_log("Failed to record m.room.member kick event", &e))?;

        Ok(())
    }
}
