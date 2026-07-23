//! Room membership moderation: invite, knock, ban, unban, kick.

use crate::common::error::{ApiError, ApiResult};
use serde_json::json;
use synapse_common::current_timestamp_millis;
use synapse_common::{generate_event_id, is_legal, JoinRule, Membership, TransitionCtx};
use synapse_storage::CreateEventParams;

use super::service::MembershipService;

impl MembershipService {
    pub async fn invite_user(&self, room_id: &str, inviter_id: &str, invitee_id: &str) -> ApiResult<()> {
        if !self
            .room_storage
            .room_exists(room_id)
            .await
            .map_err(|e| ApiError::internal_with_log("Failed to check room", &e))?
        {
            return Err(ApiError::not_found("Room not found".to_string()));
        }

        // If the invitee is on a remote server, use the federation invite
        // flow instead of the local invite path.
        if self.is_remote_user(invitee_id) {
            return self.invite_user_via_federation(room_id, inviter_id, invitee_id).await;
        }

        if !self
            .user_storage
            .user_exists(invitee_id)
            .await
            .map_err(|e| ApiError::internal_with_log("Failed to check user existence", &e))?
        {
            return Err(ApiError::not_found("User not found".to_string()));
        }

        self.room_auth.can_invite_user(room_id, inviter_id).await?;

        // Validate membership transition: cannot invite banned or already-joined users.
        let target_state = self
            .member_storage
            .get_room_member(room_id, invitee_id)
            .await
            .map_err(|e| ApiError::internal_with_log("Failed to check target membership", &e))?
            .as_ref()
            .and_then(|m| super::transition::MembershipState::parse_opt(&m.membership));
        if let Err(msg) = super::transition::is_legal(
            target_state,
            super::transition::MembershipState::Invite,
            &super::transition::TransitionContext::default(),
        ) {
            return Err(ApiError::forbidden(msg.to_string()));
        }

        // State-machine gate: reject inviting a banned/already-joined user.
        // Power was enforced by `can_invite_user` above.
        let (from, target_is_banned) = self.resolve_membership_from(room_id, invitee_id).await?;
        let ctx =
            TransitionCtx::state_only(JoinRule::Invite, /* actor_is_target */ false, target_is_banned, false);
        is_legal(from, Membership::Invite, &ctx)?;

        let member = self
            .member_storage
            .add_member(room_id, invitee_id, "invite", None, None, Some(inviter_id), None)
            .await
            .map_err(|e| ApiError::internal_with_log("Failed to create invite event", &e))?;

        // Update room summary to reflect the new invite
        let request = synapse_storage::room_summary::CreateSummaryMemberRequest {
            room_id: room_id.to_string(),
            user_id: invitee_id.to_string(),
            display_name: None,
            avatar_url: None,
            membership: "invite".to_string(),
            is_hero: None,
            last_active_ts: member.joined_ts.or(member.updated_ts),
        };
        if let Err(error) = self.room_summary_service.add_member(request).await {
            ::tracing::warn!(
                error = %error,
                room_id = %room_id,
                user_id = %invitee_id,
                "Failed to update room summary member for invite"
            );
        }

        let invite_event = self
            .event_writer
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
                    origin_server_ts: current_timestamp_millis(),
                    redacts: None,
                },
                None,
            )
            .await
            .map_err(|e| ApiError::internal_with_log("Failed to record m.room.member invite event", &e))?;

        // Invalidate room-state cache after membership state change.
        let _ = self.cache.delete(&format!("room_state:{room_id}")).await;

        // Enqueue the invite event for matching application services.
        self.dispatch_appservice_event(&invite_event).await;

        // Best-effort: sign and broadcast the invite event to federation peers.
        if let Err(e) = self.sign_and_broadcast_event(&invite_event).await {
            ::tracing::warn!(
                room_id = %room_id,
                inviter_id = %inviter_id,
                invitee_id = %invitee_id,
                error = %e,
                "Failed to sign and broadcast invite event"
            );
        }

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

        let join_rule = self.resolve_join_rule(room_id).await?;
        let (from, target_is_banned) = self.resolve_membership_from(room_id, user_id).await?;

        // Idempotent no-op: already knocking.
        if from == Some(Membership::Knock) {
            return Ok(());
        }

        // Delegate the state-machine verdict (join-rule allows knock, not
        // already joined/invited/banned) to the single transition rulebook.
        let ctx = TransitionCtx::state_only(join_rule, /* actor_is_target */ true, target_is_banned, false);
        is_legal(from, Membership::Knock, &ctx)?;

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

        self.room_auth.can_ban_user(room_id, banned_by, user_id).await?;

        // Validate membership transition: only join/invite/knock can be banned.
        let target_state = self
            .member_storage
            .get_room_member(room_id, user_id)
            .await
            .map_err(|e| ApiError::internal_with_log("Failed to check target membership", &e))?
            .as_ref()
            .and_then(|m| super::transition::MembershipState::parse_opt(&m.membership));
        if let Err(msg) = super::transition::is_legal(
            target_state,
            super::transition::MembershipState::Ban,
            &super::transition::TransitionContext::default(),
        ) {
            return Err(ApiError::forbidden(msg.to_string()));
        }

        // State-machine gate: reject self-ban. Power level and creator
        // protection were enforced by `can_ban_user` above.
        let (from, _) = self.resolve_membership_from(room_id, user_id).await?;
        let ctx =
            TransitionCtx::state_only(JoinRule::Invite, /* actor_is_target */ banned_by == user_id, false, false);
        is_legal(from, Membership::Ban, &ctx)?;

        self.member_storage
            .ban_member(room_id, user_id, banned_by)
            .await
            .map_err(|e| ApiError::internal_with_log("Failed to ban user", &e))?;

        let event_id = generate_event_id(&self.server_name);
        let content = json!({
            "membership": "ban",
            "reason": reason.unwrap_or("")
        });

        let ban_event = self
            .event_writer
            .create_event(
                CreateEventParams {
                    event_id,
                    room_id: room_id.to_string(),
                    user_id: banned_by.to_string(),
                    event_type: "m.room.member".to_string(),
                    content,
                    state_key: Some(user_id.to_string()),
                    origin_server_ts: current_timestamp_millis(),
                    redacts: None,
                },
                None,
            )
            .await
            .map_err(|e| ApiError::internal_with_log("Failed to record m.room.member ban event", &e))?;

        // Invalidate room-state cache after membership state change.
        let _ = self.cache.delete(&format!("room_state:{room_id}")).await;

        // Best-effort: sign and broadcast the ban event to federation peers.
        if let Err(e) = self.sign_and_broadcast_event(&ban_event).await {
            ::tracing::warn!(
                room_id = %room_id,
                user_id = %user_id,
                error = %e,
                "Failed to sign and broadcast ban event"
            );
        }

        Ok(())
    }

    pub async fn unban_user(&self, room_id: &str, user_id: &str, unbanned_by: &str) -> ApiResult<()> {
        self.room_auth.can_unban_user(room_id, unbanned_by, user_id).await?;

        // Validate membership transition: unban is ban→leave.
        let target_state = self
            .member_storage
            .get_room_member(room_id, user_id)
            .await
            .map_err(|e| ApiError::internal_with_log("Failed to check target membership", &e))?
            .as_ref()
            .and_then(|m| super::transition::MembershipState::parse_opt(&m.membership));
        if let Err(msg) = super::transition::is_legal(
            target_state,
            super::transition::MembershipState::Leave,
            &super::transition::TransitionContext::default(),
        ) {
            return Err(ApiError::bad_request(msg.to_string()));
        }

        // State-machine precondition: unban only applies to a currently-banned
        // user. `to = leave` is ambiguous between unban and kick, so the
        // transition rulebook cannot enforce this on its own — the client
        // endpoint's intent supplies the precondition.
        let (_from, target_is_banned) = self.resolve_membership_from(room_id, user_id).await?;
        if !target_is_banned {
            return Err(ApiError::bad_request("User is not banned from this room".to_string()));
        }

        self.member_storage
            .unban_member(room_id, user_id)
            .await
            .map_err(|e| ApiError::internal_with_log("Failed to unban user", &e))?;

        let event_id = generate_event_id(&self.server_name);
        let content = json!({
            "membership": "leave"
        });

        let unban_event = self
            .event_writer
            .create_event(
                CreateEventParams {
                    event_id,
                    room_id: room_id.to_string(),
                    user_id: unbanned_by.to_string(),
                    event_type: "m.room.member".to_string(),
                    content,
                    state_key: Some(user_id.to_string()),
                    origin_server_ts: current_timestamp_millis(),
                    redacts: None,
                },
                None,
            )
            .await
            .map_err(|e| ApiError::internal_with_log("Failed to record m.room.member unban event", &e))?;

        // Invalidate room-state cache after membership state change.
        let _ = self.cache.delete(&format!("room_state:{room_id}")).await;

        // Best-effort: sign and broadcast the unban event to federation peers.
        if let Err(e) = self.sign_and_broadcast_event(&unban_event).await {
            ::tracing::warn!(
                room_id = %room_id,
                user_id = %user_id,
                error = %e,
                "Failed to sign and broadcast unban event"
            );
        }

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

        self.room_auth.can_kick_user(room_id, kicked_by, target_user_id).await?;

        // Validate membership transition: only joined members can be kicked.
        let target_state = self
            .member_storage
            .get_room_member(room_id, target_user_id)
            .await
            .map_err(|e| ApiError::internal_with_log("Failed to check target membership", &e))?
            .as_ref()
            .and_then(|m| super::transition::MembershipState::parse_opt(&m.membership));
        if let Err(msg) = super::transition::is_legal(
            target_state,
            super::transition::MembershipState::Leave,
            &super::transition::TransitionContext::default(),
        ) {
            return Err(ApiError::forbidden(msg.to_string()));
        }

        // State-machine precondition: kick only applies to a user currently in
        // the room (join / invite / knock). A banned user must be unbanned, and
        // an absent user cannot be kicked. `to = leave` is ambiguous between
        // kick and unban, so the client endpoint's intent supplies this.
        let (from, _) = self.resolve_membership_from(room_id, target_user_id).await?;
        if !matches!(from, Some(Membership::Join | Membership::Invite | Membership::Knock)) {
            return Err(ApiError::bad_request("User is not currently in the room".to_string()));
        }

        self.member_storage
            .remove_member(room_id, target_user_id)
            .await
            .map_err(|e| ApiError::internal_with_log("Failed to kick user", &e))?;

        let event_id = generate_event_id(&self.server_name);
        let content = json!({
            "membership": "leave",
            "reason": reason.unwrap_or("")
        });

        let kick_event = self
            .event_writer
            .create_event(
                CreateEventParams {
                    event_id,
                    room_id: room_id.to_string(),
                    user_id: kicked_by.to_string(),
                    event_type: "m.room.member".to_string(),
                    content,
                    state_key: Some(target_user_id.to_string()),
                    origin_server_ts: current_timestamp_millis(),
                    redacts: None,
                },
                None,
            )
            .await
            .map_err(|e| ApiError::internal_with_log("Failed to record m.room.member kick event", &e))?;

        // Invalidate room-state cache after membership state change.
        let _ = self.cache.delete(&format!("room_state:{room_id}")).await;

        // Best-effort: sign and broadcast the kick event to federation peers.
        if let Err(e) = self.sign_and_broadcast_event(&kick_event).await {
            ::tracing::warn!(
                room_id = %room_id,
                target_user_id = %target_user_id,
                error = %e,
                "Failed to sign and broadcast kick event"
            );
        }

        Ok(())
    }
}
