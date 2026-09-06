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
            .map_err(|e| ApiError::internal_with_context("Failed to check room", &e))?
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
            .map_err(|e| ApiError::internal_with_context("Failed to check user existence", &e))?
        {
            return Err(ApiError::not_found("User not found".to_string()));
        }

        self.room_auth.can_invite_user(room_id, inviter_id).await?;

        // Validate membership transition: cannot invite banned or already-joined users.
        let target_state = self
            .member_storage
            .get_room_member(room_id, invitee_id)
            .await
            .map_err(|e| ApiError::internal_with_context("Failed to check target membership", &e))?
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
            .map_err(|e| ApiError::internal_with_context("Failed to create invite event", &e))?;

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
            .map_err(|e| ApiError::internal_with_context("Failed to record m.room.member invite event", &e))?;

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
            .map_err(|e| ApiError::internal_with_context("Failed to check room", &e))?
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
            .map_err(|e| ApiError::internal_with_context("Failed to create knock event", &e))?;
        Ok(())
    }

    pub async fn ban_user(&self, room_id: &str, user_id: &str, banned_by: &str, reason: Option<&str>) -> ApiResult<()> {
        if !self
            .room_storage
            .room_exists(room_id)
            .await
            .map_err(|e| ApiError::internal_with_context("Failed to check room", &e))?
        {
            return Err(ApiError::not_found("Room not found".to_string()));
        }

        if !self
            .user_storage
            .user_exists(user_id)
            .await
            .map_err(|e| ApiError::internal_with_context("Failed to check user existence", &e))?
        {
            return Err(ApiError::not_found("User not found".to_string()));
        }

        self.room_auth.can_ban_user(room_id, banned_by, user_id).await?;

        // Validate membership transition: only join/invite/knock can be banned.
        let target_state = self
            .member_storage
            .get_room_member(room_id, user_id)
            .await
            .map_err(|e| ApiError::internal_with_context("Failed to check target membership", &e))?
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
            .map_err(|e| ApiError::internal_with_context("Failed to ban user", &e))?;

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
            .map_err(|e| ApiError::internal_with_context("Failed to record m.room.member ban event", &e))?;

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
            .map_err(|e| ApiError::internal_with_context("Failed to check target membership", &e))?
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
            .map_err(|e| ApiError::internal_with_context("Failed to unban user", &e))?;

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
            .map_err(|e| ApiError::internal_with_context("Failed to record m.room.member unban event", &e))?;

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
            .map_err(|e| ApiError::internal_with_context("Failed to check room existence", &e))?
        {
            return Err(ApiError::not_found("Room not found".to_string()));
        }

        if !self
            .user_storage
            .user_exists(target_user_id)
            .await
            .map_err(|e| ApiError::internal_with_context("Failed to check user existence", &e))?
        {
            return Err(ApiError::not_found("User not found".to_string()));
        }

        self.room_auth.can_kick_user(room_id, kicked_by, target_user_id).await?;

        // Validate membership transition: only joined members can be kicked.
        let target_state = self
            .member_storage
            .get_room_member(room_id, target_user_id)
            .await
            .map_err(|e| ApiError::internal_with_context("Failed to check target membership", &e))?
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
            .remove_member(room_id, target_user_id, None)
            .await
            .map_err(|e| ApiError::internal_with_context("Failed to kick user", &e))?;

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
            .map_err(|e| ApiError::internal_with_context("Failed to record m.room.member kick event", &e))?;

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

#[cfg(test)]
mod tests {
    //! Unit tests for the moderation methods (`invite_user`, `knock_room`,
    //! `ban_user`, `unban_user`, `kick_user`) using in-memory mocks.
    //!
    //! These live in the source file rather than a separate integration test so
    //! they can use `super::*` imports without an extra `mod.rs` entry.
    //! They exercise the full service layer but against in-memory fakes — no
    //! Postgres required, no network I/O, no slow startup.
    //!
    //! See also: `tests/integration/space_children_service_tests.rs` for the
    //! Postgres-backed service-layer tests.

    #![allow(clippy::unwrap_used, clippy::expect_used)]

    use std::sync::Arc;
    use synapse_storage::test_mocks::{
        InMemoryEventStore, InMemoryMemberStore, InMemoryRoomStore, InMemoryRoomSummaryStore,
    };
    use synapse_storage::MemberStoreApi;
    use synapse_storage::RoomStoreApi;
    use synapse_storage::UserStore;
    use synapse_storage::event::{EventReader, EventWriter};

    use crate::room::membership::service::{MembershipService, MembershipServiceConfig};
    use crate::room::summary::RoomSummaryService;
    use crate::test_mocks::FakeRoomAuth;

    fn build_membership_service() -> (MembershipService, Arc<synapse_storage::test_mocks::FakeUserStore>) {
        let room_store = Arc::new(InMemoryRoomStore::new());
        let member_store = Arc::new(InMemoryMemberStore::new());
        let event_store = Arc::new(InMemoryEventStore::new());
        let summary_store = Arc::new(InMemoryRoomSummaryStore::new()) as Arc<dyn synapse_storage::room_summary::RoomSummaryStoreApi>;
        let user_store = Arc::new(synapse_storage::test_mocks::FakeUserStore::new());
        let user_store_dyn: Arc<dyn UserStore> = user_store.clone();
        let room_summary = Arc::new(RoomSummaryService::new(
            summary_store,
            event_store.clone() as Arc<dyn synapse_storage::event::EventReader>,
            None,
        ));
        let cache = Arc::new(synapse_cache::CacheManager::new(&Default::default()));

        let config = MembershipServiceConfig {
            member_storage: member_store as Arc<dyn MemberStoreApi>,
            room_storage: room_store as Arc<dyn RoomStoreApi>,
            event_reader: event_store.clone() as Arc<dyn EventReader>,
            event_writer: event_store as Arc<dyn EventWriter>,
            user_storage: user_store_dyn,
            user_service: Arc::new(crate::UserService::new(user_store.clone())),
            room_auth: Arc::new(FakeRoomAuth::new()),
            server_name: "test.localhost".to_string(),
            federation_client: None,
            key_rotation_manager: None,
            event_broadcaster: None,
            room_summary_service: room_summary,
            cache,
            key_rotation_storage: None,
            app_service_manager: None,
            db_pool: None,
        };
        (MembershipService::new(config), user_store)
    }

    #[tokio::test]
    async fn knock_room_on_knock_join_rule_succeeds() {
        let (svc, _user_store) = build_membership_service();

        // Create a room with knock join_rule.
        let room_id = "!knockable:test.localhost";
        let user_id = "@alice:test.localhost";
        let room_store = svc.room_storage.clone();
        let mem_store = svc.member_storage.clone();

        // Seed the room.
        room_store
            .create_room(room_id, "@creator:test.localhost", "knock", "1", false)
            .await
            .expect("create_room");

        // User is NOT a member yet — knock transitions from (none) to knock.
        svc.knock_room(room_id, user_id, Some("need access"))
            .await
            .expect("knock_room should succeed");

        // Verify the member state.
        let member = mem_store
            .get_room_member(room_id, user_id)
            .await
            .expect("get_room_member");
        assert!(
            member.as_ref().is_some_and(|m| m.membership == "knock"),
            "member should be in knock state"
        );
    }

    #[tokio::test]
    async fn knock_room_fails_on_private_join_rule() {
        let (svc, _user_store) = build_membership_service();

        let room_id = "!private:test.localhost";
        let user_id = "@bob:test.localhost";

        let room_store = svc.room_storage.clone();
        room_store
            .create_room(room_id, "@creator:test.localhost", "invite", "1", false)
            .await
            .expect("create_room");

        let err = svc
            .knock_room(room_id, user_id, None)
            .await
            .expect_err("knock on invite-only room should fail");
        assert!(
            err.to_string().to_lowercase().contains("transition")
                || err.to_string().to_lowercase().contains("not allowed"),
            "expected transition/forbidden error, got: {}",
            err
        );
    }

    #[tokio::test]
    async fn knock_room_fails_when_room_missing() {
        let (svc, _user_store) = build_membership_service();
        let err = svc
            .knock_room("!nonexistent:test.localhost", "@alice:test.localhost", None)
            .await
            .expect_err("knock nonexistent room should fail");
        assert!(
            err.to_string().to_lowercase().contains("not found"),
            "expected not_found error, got: {}",
            err
        );
    }

    #[tokio::test]
    async fn ban_user_happy_path_updates_membership() {
        let (svc, user_store) = build_membership_service();

        let room_id = "!mod:test.localhost";
        let target = "@bad:test.localhost";
        let moderator = "@mod:test.localhost";

        let room_store = svc.room_storage.clone();
        let mem_store = svc.member_storage.clone();

        room_store
            .create_room(room_id, moderator, "invite", "1", false)
            .await
            .expect("create_room");
        mem_store
            .add_member(room_id, moderator, "join", None, None, None, None)
            .await
            .expect("mod join");
        mem_store
            .add_member(room_id, target, "join", None, None, None, None)
            .await
            .expect("target join");

        // Seed the target user so ban_user's user_exists check passes.
        let username = target.trim_start_matches('@').split(':').next().unwrap_or(target).to_string();
        user_store
            .create_user(target, &username, None, false)
            .await
            .expect("create_user");

        svc.ban_user(room_id, target, moderator, Some("repeated spam"))
            .await
            .expect("ban_user should succeed");

        let member = mem_store
            .get_room_member(room_id, target)
            .await
            .expect("get_room_member");
        assert!(
            member.as_ref().is_some_and(|m| m.membership == "ban"),
            "target should be banned, got: {member:?}"
        );
    }

    #[tokio::test]
    async fn ban_user_fails_when_target_not_found() {
        let (svc, _user_store) = build_membership_service();

        let room_id = "!test:test.localhost";
        let moderator = "@mod:test.localhost";

        let room_store = svc.room_storage.clone();
        room_store
            .create_room(room_id, moderator, "invite", "1", false)
            .await
            .expect("create_room");

        let err = svc
            .ban_user(room_id, "@ghost:test.localhost", moderator, None)
            .await
            .expect_err("ban nonexistent user should fail");
        assert!(
            err.to_string().to_lowercase().contains("not found")
                || err.to_string().to_lowercase().contains("user not found"),
            "expected not_found error, got: {}",
            err
        );
    }

    #[tokio::test]
    async fn unban_user_happy_path_restores_access() {
        let (svc, _user_store) = build_membership_service();

        let room_id = "!unban:test.localhost";
        let target = "@was_banned:test.localhost";
        let unbanner = "@admin:test.localhost";

        let room_store = svc.room_storage.clone();
        let mem_store = svc.member_storage.clone();

        room_store
            .create_room(room_id, unbanner, "invite", "1", false)
            .await
            .expect("create_room");
        mem_store
            .add_member(room_id, unbanner, "join", None, None, None, None)
            .await
            .expect("admin join");
        mem_store
            .add_member(room_id, target, "ban", None, None, None, None)
            .await
            .expect("ban target");

        svc.unban_user(room_id, target, unbanner).await.expect("unban_user should succeed");

        let member = mem_store
            .get_room_member(room_id, target)
            .await
            .expect("get_room_member");
        // After unban, membership row is deleted (not present).
        assert!(
            member.as_ref().is_some_and(|m| m.membership == "leave"),
            "unbanned user should have leave membership (mock converts ban→leave), got: {member:?}"
        );
    }

    #[tokio::test]
    async fn unban_user_allows_reinvite_after_unban() {
        let (svc, _user_store) = build_membership_service();

        let room_id = "!reinvite:test.localhost";
        let target = "@reinvited:test.localhost";
        let admin = "@admin:test.localhost";

        let room_store = svc.room_storage.clone();
        let mem_store = svc.member_storage.clone();
        let user_store = svc.user_storage.clone();

        room_store
            .create_room(room_id, admin, "invite", "1", false)
            .await
            .expect("create_room");
        mem_store
            .add_member(room_id, admin, "join", None, None, None, None)
            .await
            .expect("admin join");
        mem_store
            .add_member(room_id, target, "ban", None, None, None, None)
            .await
            .expect("ban");
        // Seed the user so invite_user's user_exists check passes.
        let username = target.trim_start_matches('@').split(':').next().unwrap_or(target).to_string();
        user_store
            .create_user(target, &username, None, false)
            .await
            .expect("create_user");

        svc.unban_user(room_id, target, admin).await.expect("unban");

        // After unban, invite should succeed (FakeRoomAuth allows it).
        svc.invite_user(room_id, admin, target).await.expect("invite after unban");

        let member = mem_store
            .get_room_member(room_id, target)
            .await
            .expect("get_room_member");
        assert!(
            member.as_ref().is_some_and(|m| m.membership == "invite"),
            "should be invited after unban, got: {member:?}"
        );
    }

    #[tokio::test]
    async fn kick_user_happy_path_removes_join() {
        let (svc, user_store) = build_membership_service();

        let room_id = "!kick:test.localhost";
        let target = "@kicked:test.localhost";
        let actor = "@mod:test.localhost";

        let room_store = svc.room_storage.clone();
        let mem_store = svc.member_storage.clone();

        room_store
            .create_room(room_id, actor, "invite", "1", false)
            .await
            .expect("create_room");
        mem_store
            .add_member(room_id, actor, "join", None, None, None, None)
            .await
            .expect("mod join");
        mem_store
            .add_member(room_id, target, "join", None, None, None, None)
            .await
            .expect("target join");

        // Seed the target user so kick_user's user_exists check passes.
        let username = target.trim_start_matches('@').split(':').next().unwrap_or(target).to_string();
        user_store
            .create_user(target, &username, None, false)
            .await
            .expect("create_user");

        svc.kick_user(room_id, target, actor, Some("behaving badly"))
            .await
            .expect("kick_user should succeed");

        let member = mem_store
            .get_room_member(room_id, target)
            .await
            .expect("get_room_member");
        assert!(
            member.as_ref().is_some_and(|m| m.membership == "leave"),
            "kicked user should have leave membership (mock converts join→leave), got: {member:?}"
        );
    }

    #[tokio::test]
    async fn invite_user_happy_path_sets_invite_membership() {
        let (svc, _user_store) = build_membership_service();

        let room_id = "!invite:test.localhost";
        let inviter = "@inviter:test.localhost";
        let invitee = "@invitee:test.localhost";

        let room_store = svc.room_storage.clone();
        let mem_store = svc.member_storage.clone();
        let user_store = svc.user_storage.clone();

        room_store
            .create_room(room_id, inviter, "invite", "1", false)
            .await
            .expect("create_room");
        mem_store
            .add_member(room_id, inviter, "join", None, None, None, None)
            .await
            .expect("inviter join");
        // Seed the user so invite_user's user_exists check passes.
        let username = invitee.trim_start_matches('@').split(':').next().unwrap_or(invitee).to_string();
        user_store
            .create_user(invitee, &username, None, false)
            .await
            .expect("create_user");

        svc.invite_user(room_id, inviter, invitee)
            .await
            .expect("invite_user should succeed");

        let member = mem_store
            .get_room_member(room_id, invitee)
            .await
            .expect("get_room_member");
        assert!(
            member.as_ref().is_some_and(|m| m.membership == "invite"),
            "invitee should be in invite state, got: {member:?}"
        );
    }
}
