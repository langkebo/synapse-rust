//! Room membership actions: join, leave, forget.

use crate::common::error::{ApiError, ApiResult};
use serde_json::json;
use synapse_common::generate_event_id;
use synapse_storage::CreateEventParams;

use super::service::MembershipService;

impl MembershipService {
    /// Join a room, automatically detecting whether the room is local or
    /// remote.  For remote rooms, delegates to the federation make_join /
    /// send_join flow.  `via_servers` is used to select the destination
    /// homeserver for federation joins; if empty, the server name embedded
    /// in the room ID is used.
    #[::tracing::instrument(skip(self, via_servers))]
    pub async fn join_room_with_via_servers(
        &self,
        room_id: &str,
        user_id: &str,
        via_servers: &[String],
    ) -> ApiResult<()> {
        // If the room already exists locally, use the local join path.
        let room_exists = self
            .room_storage
            .room_exists(room_id)
            .await
            .map_err(|e| ApiError::internal_with_log("Failed to check room existence", &e))?;

        if room_exists {
            return self.join_room(room_id, user_id).await;
        }

        // Room doesn't exist locally — try federation join.
        // Pick a destination server: prefer the first via_server, otherwise
        // use the server name embedded in the room ID.
        let destination = via_servers
            .first()
            .cloned()
            .or_else(|| room_id.rsplit_once(':').map(|(_, srv)| srv.to_string()))
            .ok_or_else(|| {
                ApiError::bad_request("Cannot join remote room: no destination server available".to_string())
            })?;

        self.join_room_via_federation(&destination, room_id, user_id).await
    }

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

        let effective_join_rule = if let Some(event) = self
            .event_reader
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

        let current_state =
            existing_member.as_ref().and_then(|m| super::transition::MembershipState::parse_opt(&m.membership));
        let ctx = super::transition::TransitionContext::new(Some(join_rule.clone()));
        if let Err(msg) = super::transition::is_legal(current_state, super::transition::MembershipState::Join, &ctx) {
            return Err(ApiError::forbidden(msg.to_string()));
        }

        self.member_storage
            .add_member(room_id, user_id, "join", None, None, None, None)
            .await
            .map_err(|e| ApiError::internal_with_log("Failed to join room", &e))?;

        self.room_storage
            .increment_member_count(room_id)
            .await
            .map_err(|e| ApiError::internal_with_log("Failed to update member count", &e))?;

        let join_event = self
            .event_writer
            .create_event(
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
                    redacts: None,
                },
                None,
            )
            .await
            .map_err(|e| ApiError::internal_with_log("Failed to record m.room.member join event", &e))?;

        // Best-effort: sign and broadcast the join event to federation peers.
        if let Err(e) = self.sign_and_broadcast_event(&join_event).await {
            ::tracing::warn!(
                room_id = %room_id,
                user_id = %user_id,
                error = %e,
                "Failed to sign and broadcast join event"
            );
        }

        Ok(())
    }

    #[::tracing::instrument(skip(self))]
    pub async fn leave_room(&self, room_id: &str, user_id: &str) -> ApiResult<()> {
        // If the room belongs to a remote server, use the federation leave
        // flow (make_leave / send_leave).
        if self.is_remote_room(room_id) {
            let destination = room_id
                .rsplit_once(':')
                .map(|(_, srv)| srv.to_string())
                .ok_or_else(|| ApiError::bad_request("Invalid room ID: missing server name".to_string()))?;
            return self.leave_room_via_federation(&destination, room_id, user_id).await;
        }

        let existing_member = self
            .member_storage
            .get_room_member(room_id, user_id)
            .await
            .map_err(|e| ApiError::internal_with_log("Failed to check membership before leave", &e))?;

        let current_state =
            existing_member.as_ref().and_then(|m| super::transition::MembershipState::parse_opt(&m.membership));
        if let Err(msg) = super::transition::is_legal(
            current_state,
            super::transition::MembershipState::Leave,
            &super::transition::TransitionContext::default(),
        ) {
            return Err(ApiError::forbidden(msg.to_string()));
        }

        self.member_storage
            .remove_member(room_id, user_id)
            .await
            .map_err(|e| ApiError::internal_with_log("Failed to leave room", &e))?;

        if existing_member.as_ref().is_some_and(|member| member.membership == "join") {
            self.room_storage
                .decrement_member_count(room_id)
                .await
                .map_err(|e| ApiError::internal_with_log("Failed to update member count", &e))?;
        }

        let leave_event = self
            .event_writer
            .create_event(
                CreateEventParams {
                    event_id: generate_event_id(&self.server_name),
                    room_id: room_id.to_string(),
                    user_id: user_id.to_string(),
                    event_type: "m.room.member".to_string(),
                    content: json!({ "membership": "leave" }),
                    state_key: Some(user_id.to_string()),
                    origin_server_ts: chrono::Utc::now().timestamp_millis(),
                    redacts: None,
                },
                None,
            )
            .await
            .map_err(|e| ApiError::internal_with_log("Failed to record m.room.member leave event", &e))?;

        // Best-effort: sign and broadcast the leave event to federation peers.
        if let Err(e) = self.sign_and_broadcast_event(&leave_event).await {
            ::tracing::warn!(
                room_id = %room_id,
                user_id = %user_id,
                error = %e,
                "Failed to sign and broadcast leave event"
            );
        }

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
