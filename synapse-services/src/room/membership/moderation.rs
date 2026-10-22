//! Room membership moderation: invite, knock, ban, unban, kick.

use crate::common::error::{ApiError, ApiResult};
use serde_json::json;
use synapse_common::current_timestamp_millis;
use synapse_common::{generate_event_id, is_legal, JoinRule, Membership, TransitionCtx};
use synapse_storage::CreateEventParams;

#[cfg(feature = "friends")]
use super::friend_dm_sync::ModerationState;
use super::service::MembershipService;

impl MembershipService {
    /// See [`invite_user`].
    ///
    /// `reason` is the optional `reason` from the request body; when present it
    /// is copied into the `m.room.member` content verbatim (upstream Synapse
    /// only emits the key when the client supplied one).
    pub async fn invite_user(
        &self,
        room_id: &str,
        inviter_id: &str,
        invitee_id: &str,
        reason: Option<&str>,
    ) -> ApiResult<()> {
        if !self
            .room_storage
            .room_exists(room_id)
            .await
            .map_err(|e| ApiError::internal_with_cause("Failed to check room", e))?
        {
            return Err(ApiError::not_found("Room not found".to_string()));
        }

        // Invite policy: the room's invite lists, the invitee's own MSC4155
        // account policy, and the MSC4284 policy server. Checked here — before
        // the remote-invite branch below — because that branch returns early,
        // and a remote invitee must not be a way around the policy.
        self.authorize_invite_policy(room_id, inviter_id, invitee_id).await?;

        // If the invitee is on a remote server, use the federation invite
        // flow instead of the local invite path.
        if self.is_remote_user(invitee_id) {
            return self.invite_user_via_federation(room_id, inviter_id, invitee_id, reason).await;
        }

        // U-2: moderation ⇒ active predicate. A deactivated account must not be
        // invitable (it can never accept).
        if !self
            .user_storage
            .active_user_exists(invitee_id)
            .await
            .map_err(|e| ApiError::internal_with_cause("Failed to check user existence", e))?
        {
            return Err(ApiError::not_found("User not found".to_string()));
        }

        self.room_auth.can_invite_user(room_id, inviter_id).await?;

        // State-machine gate: reject inviting a banned/already-joined user.
        // Power was enforced by `can_invite_user` above.
        let (from, target_is_banned) = self.resolve_membership_from(room_id, invitee_id).await?;
        let ctx =
            TransitionCtx::state_only(JoinRule::Invite, /* actor_is_target */ false, target_is_banned, false);
        is_legal(from, Membership::Invite, &ctx)?;

        // Admission runs before the state change so a refusal leaves no residue.
        let event_id = generate_event_id(&self.server_name);
        let mut invite_content = json!({
            "membership": "invite",
            "displayname": invitee_id.trim_start_matches('@').split(':').next().unwrap_or(invitee_id),
        });
        if let Some(reason) = reason {
            invite_content["reason"] = json!(reason);
        }
        let invite_content =
            self.admit_membership_event(room_id, &event_id, inviter_id, invitee_id, invite_content, true).await?;

        let member = self
            .member_storage
            .add_member(room_id, invitee_id, "invite", None, None, Some(inviter_id), None)
            .await
            .map_err(|e| ApiError::internal_with_cause("Failed to create invite event", e))?;

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
                    event_id,
                    room_id: room_id.to_string(),
                    user_id: inviter_id.to_string(),
                    event_type: "m.room.member".to_string(),
                    content: invite_content,
                    state_key: Some(invitee_id.to_string()),
                    origin_server_ts: current_timestamp_millis(),
                    redacts: None,
                },
                None,
            )
            .await
            .map_err(|e| ApiError::internal_with_cause("Failed to record m.room.member invite event", e))?;

        // Invalidate room-state cache after membership state change.
        crate::room_state_cache::invalidate_room_state(&self.cache, room_id).await;

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

    /// Record an *inbound* `m.room.member` invite that arrived over federation:
    /// the local server is the invitee's home server and the inviting user lives
    /// elsewhere, so no local PDU is minted here — the caller has already
    /// persisted the origin-signed one.
    ///
    /// Mirrors the local-invite bookkeeping in [`Self::invite_user`]: the
    /// `room_memberships` row (with `sender` = the remote inviter, so
    /// `/rooms/{roomId}/state` and `/sync` report who invited the user) and the
    /// best-effort room-summary member row.
    pub async fn record_inbound_federation_invite(
        &self,
        room_id: &str,
        invitee_id: &str,
        inviter_id: &str,
    ) -> ApiResult<()> {
        let member = self
            .member_storage
            .add_member(room_id, invitee_id, "invite", None, None, Some(inviter_id), None)
            .await
            .map_err(|e| ApiError::internal_with_cause("Failed to record inbound invite membership", e))?;

        let request = synapse_storage::room_summary::CreateSummaryMemberRequest {
            room_id: room_id.to_string(),
            user_id: invitee_id.to_string(),
            display_name: None,
            avatar_url: None,
            membership: "invite".to_string(),
            is_hero: None,
            last_active_ts: member.joined_ts.or(member.updated_ts),
        };
        // Best-effort: a room we do not host may have no summary row yet, and
        // the membership write above is the contract the caller depends on.
        if let Err(error) = self.room_summary_service.add_member(request).await {
            ::tracing::warn!(
                error = %error,
                room_id = %room_id,
                user_id = %invitee_id,
                "Failed to update room summary member for inbound invite"
            );
        }

        // Invalidate room-state cache after membership state change.
        crate::room_state_cache::invalidate_room_state(&self.cache, room_id).await;

        Ok(())
    }

    /// See [`knock_room`].
    pub async fn knock_room(&self, room_id: &str, user_id: &str, reason: Option<&str>) -> ApiResult<()> {
        if !self
            .room_storage
            .room_exists(room_id)
            .await
            .map_err(|e| ApiError::internal_with_cause("Failed to check room", e))?
        {
            return Err(ApiError::not_found("Room not found".to_string()));
        }

        let join_rule = self.resolve_join_rule(room_id).await?;
        let (from, target_is_banned) = self.resolve_membership_from(room_id, user_id).await?;

        // Idempotent no-op: already knocking.
        if from == Some(Membership::Knock) {
            return Ok(());
        }

        // B-3: a blocked user must not be admitted through the knock path either.
        self.invite_policy_gate.check_join_allowed(room_id, user_id).await?;

        // Delegate the state-machine verdict (join-rule allows knock, not
        // already joined/invited/banned) to the single transition rulebook.
        let ctx = TransitionCtx::state_only(join_rule, /* actor_is_target */ true, target_is_banned, false);
        is_legal(from, Membership::Knock, &ctx)?;

        // Third-party event admission (Synapse `check_event_allowed`), consulted
        // *before* the membership state change so a refusal leaves no residue.
        let event_id = generate_event_id(&self.server_name);
        let knock_content = json!({
            "membership": "knock",
            "reason": reason.unwrap_or_default(),
        });
        let knock_content =
            self.admit_membership_event(room_id, &event_id, user_id, user_id, knock_content, true).await?;

        self.member_storage
            .add_member(room_id, user_id, "knock", None, reason, None, None)
            .await
            .map_err(|e| ApiError::internal_with_cause("Failed to create knock event", e))?;

        // Persist the m.room.member PDU so the federation broadcast path can
        // read back the graph fields (depth / prev_events / auth_events) and
        // sign a complete PDU for remote servers — the same flow `invite_user`
        // and `ban_user` use.
        let knock_event = self
            .event_writer
            .create_event(
                CreateEventParams {
                    event_id,
                    room_id: room_id.to_string(),
                    user_id: user_id.to_string(),
                    event_type: "m.room.member".to_string(),
                    content: knock_content,
                    state_key: Some(user_id.to_string()),
                    origin_server_ts: current_timestamp_millis(),
                    redacts: None,
                },
                None,
            )
            .await
            .map_err(|e| ApiError::internal_with_cause("Failed to record m.room.member knock event", e))?;

        // Invalidate room-state cache after membership state change.
        crate::room_state_cache::invalidate_room_state(&self.cache, room_id).await;

        // Best-effort: sign and broadcast the knock event to federation peers.
        if let Err(e) = self.sign_and_broadcast_event(&knock_event).await {
            ::tracing::warn!(
                room_id = %room_id,
                user_id = %user_id,
                error = %e,
                "Failed to sign and broadcast knock event"
            );
        }

        Ok(())
    }

    /// See [`ban_user`]. Gates run here; persistence/event/broadcast live in
    /// [`Self::apply_moderation_transition`], shared with the admin endpoint.
    pub async fn ban_user(&self, room_id: &str, user_id: &str, banned_by: &str, reason: Option<&str>) -> ApiResult<()> {
        if !self
            .room_storage
            .room_exists(room_id)
            .await
            .map_err(|e| ApiError::internal_with_cause("Failed to check room", e))?
        {
            return Err(ApiError::not_found("Room not found".to_string()));
        }

        // U-2: moderation ⇒ active predicate (ban).
        if !self
            .user_storage
            .active_user_exists(user_id)
            .await
            .map_err(|e| ApiError::internal_with_cause("Failed to check user existence", e))?
        {
            return Err(ApiError::not_found("User not found".to_string()));
        }

        self.room_auth.can_ban_user(room_id, banned_by, user_id).await?;

        // State-machine gate: reject self-ban. Power level and creator
        // protection were enforced by `can_ban_user` above.
        let (from, _) = self.resolve_membership_from(room_id, user_id).await?;
        let ctx =
            TransitionCtx::state_only(JoinRule::Invite, /* actor_is_target */ banned_by == user_id, false, false);
        is_legal(from, Membership::Ban, &ctx)?;

        // Third-party event admission (Synapse `check_event_allowed`), consulted
        // *before* the membership state change so a refusal leaves no residue.
        let event_id = generate_event_id(&self.server_name);
        let content = json!({
            "membership": "ban",
            "reason": reason.unwrap_or("")
        });
        let content = self.admit_membership_event(room_id, &event_id, banned_by, user_id, content, true).await?;

        self.apply_moderation_transition(
            room_id,
            ModerationAction {
                target: user_id,
                actor: banned_by,
                transition: ModerationTransition::Ban,
                reason,
                content,
                event_id,
            },
        )
        .await
    }

    /// See [`unban_user`].
    pub async fn unban_user(
        &self,
        room_id: &str,
        user_id: &str,
        unbanned_by: &str,
        reason: Option<&str>,
    ) -> ApiResult<()> {
        self.room_auth.can_unban_user(room_id, unbanned_by, user_id).await?;

        // `to = leave` cannot tell unban from kick, so the endpoint's intent
        // supplies the precondition: only a banned user can be unbanned.
        let (_from, target_is_banned) = self.resolve_membership_from(room_id, user_id).await?;
        if !target_is_banned {
            return Err(ApiError::bad_request("User is not banned from this room".to_string()));
        }

        // Third-party event admission (Synapse `check_event_allowed`), consulted
        // *before* the membership state change so a refusal leaves no residue.
        let event_id = generate_event_id(&self.server_name);
        let content = json!({
            "membership": "leave",
            "reason": reason.unwrap_or("")
        });
        let content = self.admit_membership_event(room_id, &event_id, unbanned_by, user_id, content, true).await?;

        self.apply_moderation_transition(
            room_id,
            ModerationAction {
                target: user_id,
                actor: unbanned_by,
                transition: ModerationTransition::Unban,
                reason,
                content,
                event_id,
            },
        )
        .await
    }

    /// See [`kick_user`].
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
            .map_err(|e| ApiError::internal_with_cause("Failed to check room existence", e))?
        {
            return Err(ApiError::not_found("Room not found".to_string()));
        }

        // U-2: moderation ⇒ active predicate (kick).
        if !self
            .user_storage
            .active_user_exists(target_user_id)
            .await
            .map_err(|e| ApiError::internal_with_cause("Failed to check user existence", e))?
        {
            return Err(ApiError::not_found("User not found".to_string()));
        }

        self.room_auth.can_kick_user(room_id, kicked_by, target_user_id).await?;

        // Kick only applies to join / invite / knock; a banned user must be
        // unbanned first, and `to = leave` cannot tell kick from unban.
        let (from, _) = self.resolve_membership_from(room_id, target_user_id).await?;
        if !matches!(from, Some(Membership::Join | Membership::Invite | Membership::Knock)) {
            return Err(ApiError::bad_request("User is not currently in the room".to_string()));
        }

        // Third-party event admission (Synapse `check_event_allowed`), consulted
        // *before* the membership state change so a refusal leaves no residue.
        let event_id = generate_event_id(&self.server_name);
        let content = json!({
            "membership": "leave",
            "reason": reason.unwrap_or("")
        });
        let content = self.admit_membership_event(room_id, &event_id, kicked_by, target_user_id, content, true).await?;

        self.apply_moderation_transition(
            room_id,
            ModerationAction {
                target: target_user_id,
                actor: kicked_by,
                transition: ModerationTransition::Kick,
                reason,
                content,
                event_id,
            },
        )
        .await
    }

    /// Persist **and** broadcast one moderation transition — the tail the client
    /// endpoints reach after their gates and the admin endpoints reach directly.
    /// One `m.room.member` event sent by `actor`, one counter/summary update,
    /// one broadcast, one friend-DM sync, for both callers.
    #[cfg_attr(not(feature = "friends"), allow(unused_variables))]
    pub(super) async fn apply_moderation_transition(
        &self,
        room_id: &str,
        action: ModerationAction<'_>,
    ) -> ApiResult<()> {
        let ModerationAction { target, actor, transition, reason, content, event_id } = action;
        let was_joined = self.get_room_membership(room_id, target).await?.as_deref() == Some("join");

        match transition {
            ModerationTransition::Ban => {
                self.member_storage
                    .ban_member(room_id, target, actor)
                    .await
                    .map_err(|e| ApiError::internal_with_cause("Failed to ban user", e))?;
                // `/rooms/{id}/members` reports the reason from this column.
                if let Some(reason) = reason {
                    self.member_storage
                        .set_ban_reason(room_id, target, Some(reason))
                        .await
                        .map_err(|e| ApiError::internal_with_cause("Failed to record ban reason", e))?;
                }
            }
            ModerationTransition::Unban => {
                self.member_storage
                    .unban_member(room_id, target)
                    .await
                    .map_err(|e| ApiError::internal_with_cause("Failed to unban user", e))?;
            }
            ModerationTransition::Kick => {
                self.member_storage
                    .remove_member(room_id, target, None)
                    .await
                    .map_err(|e| ApiError::internal_with_cause("Failed to kick user", e))?;
            }
        }

        // Only a transition out of `join` moves the counter (unban cannot).
        if was_joined && matches!(transition, ModerationTransition::Ban | ModerationTransition::Kick) {
            self.decrement_member_count(room_id).await?;
        }
        // I-01: a banned/kicked user must leave the room summary projection.
        self.remove_summary_member_projection(room_id, target, None).await?;

        let event = self
            .event_writer
            .create_event(
                CreateEventParams {
                    event_id,
                    room_id: room_id.to_string(),
                    user_id: actor.to_string(),
                    event_type: "m.room.member".to_string(),
                    content,
                    state_key: Some(target.to_string()),
                    origin_server_ts: current_timestamp_millis(),
                    redacts: None,
                },
                None,
            )
            .await
            .map_err(|e| ApiError::internal_with_cause("Failed to record m.room.member moderation event", e))?;

        // Invalidate room-state cache after membership state change.
        crate::room_state_cache::invalidate_room_state(&self.cache, room_id).await;

        // Best-effort: sign and broadcast to federation peers.
        if let Err(e) = self.sign_and_broadcast_event(&event).await {
            ::tracing::warn!(
                room_id = %room_id,
                target = %target,
                error = %e,
                "Failed to sign and broadcast moderation event"
            );
        }

        #[cfg(feature = "friends")]
        {
            let state = match transition {
                ModerationTransition::Ban => ModerationState::Banned,
                ModerationTransition::Unban => ModerationState::Unbanned,
                ModerationTransition::Kick => ModerationState::Kicked,
            };
            self.sync_friend_dm_moderation(room_id, target, state, actor, reason).await;
        }

        Ok(())
    }
}

/// The intent behind a transition (`Unban` also stores `leave`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum ModerationTransition {
    Ban,
    Unban,
    Kick,
}

/// One transition's inputs — bundled so the helper keeps a small signature.
pub(super) struct ModerationAction<'a> {
    pub target: &'a str,
    pub actor: &'a str,
    pub transition: ModerationTransition,
    pub reason: Option<&'a str>,
    pub content: serde_json::Value,
    pub event_id: String,
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
    use synapse_storage::event::{EventReader, EventWriter};
    use synapse_storage::test_mocks::{
        InMemoryEventStore, InMemoryMemberStore, InMemoryRoomStore, InMemoryRoomSummaryStore,
    };
    use synapse_storage::MemberStoreApi;
    use synapse_storage::RoomStoreApi;
    use synapse_storage::UserStore;

    use crate::room::membership::service::{MembershipService, MembershipServiceConfig};
    use crate::room::summary::RoomSummaryService;
    use crate::test_mocks::FakeRoomAuth;

    fn build_membership_service() -> (MembershipService, Arc<synapse_storage::test_mocks::FakeUserStore>) {
        let room_store = Arc::new(InMemoryRoomStore::new());
        let member_store = Arc::new(InMemoryMemberStore::new());
        let event_store = Arc::new(InMemoryEventStore::new());
        let summary_store =
            Arc::new(InMemoryRoomSummaryStore::new()) as Arc<dyn synapse_storage::room_summary::RoomSummaryStoreApi>;
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
            room_auth: Arc::new(FakeRoomAuth::new()),
            server_name: "test.localhost".to_string(),
            federation_client: None,
            key_rotation_manager: None,
            event_broadcaster: None,
            room_summary_service: room_summary,
            cache,
            resolution_cache: crate::room::state_record::ResolutionCache::default(),
            key_rotation_storage: None,
            app_service_manager: None,
            db_pool: None,
            policy_service: None,
            invite_policy_gate: Arc::new(crate::test_mocks::FakeInvitePolicyGate::new()),
            event_admission_gate: Arc::new(crate::test_mocks::FakeEventAdmissionGate::new()),
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
        room_store.create_room(room_id, "@creator:test.localhost", "knock", "1", false).await.expect("create_room");

        // User is NOT a member yet — knock transitions from (none) to knock.
        svc.knock_room(room_id, user_id, Some("need access")).await.expect("knock_room should succeed");

        // Verify the member state.
        let member = mem_store.get_room_member(room_id, user_id).await.expect("get_room_member");
        assert!(member.as_ref().is_some_and(|m| m.membership == "knock"), "member should be in knock state");
    }

    #[tokio::test]
    async fn knock_room_fails_on_private_join_rule() {
        let (svc, _user_store) = build_membership_service();

        let room_id = "!private:test.localhost";
        let user_id = "@bob:test.localhost";

        let room_store = svc.room_storage.clone();
        room_store.create_room(room_id, "@creator:test.localhost", "invite", "1", false).await.expect("create_room");

        let err = svc.knock_room(room_id, user_id, None).await.expect_err("knock on invite-only room should fail");
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
        assert!(err.to_string().to_lowercase().contains("not found"), "expected not_found error, got: {}", err);
    }

    /// B-3: a blocked user must not be admitted through the knock path either,
    /// even when the room's join rule would otherwise allow knocking.
    #[tokio::test]
    async fn knock_room_denied_by_policy_gate_is_rejected() {
        let (mut svc, _user_store) = build_membership_service();
        svc.invite_policy_gate = Arc::new(crate::test_mocks::FakeInvitePolicyGate::denying());

        let room_id = "!knockblocked:test.localhost";
        let user_id = "@blocked:test.localhost";
        svc.room_storage
            .create_room(room_id, "@creator:test.localhost", "knock", "1", false)
            .await
            .expect("create_room");

        let err =
            svc.knock_room(room_id, user_id, None).await.expect_err("a blocked user must not be allowed to knock");
        assert_eq!(err.code, synapse_common::MatrixErrorCode::Forbidden, "expected M_FORBIDDEN, got: {err:?}");

        let member = svc.member_storage.get_room_member(room_id, user_id).await.expect("get_room_member");
        assert!(member.is_none(), "a rejected knock must not be persisted, got: {member:?}");
    }

    #[tokio::test]
    async fn ban_user_happy_path_updates_membership() {
        let (svc, user_store) = build_membership_service();

        let room_id = "!mod:test.localhost";
        let target = "@bad:test.localhost";
        let moderator = "@mod:test.localhost";

        let room_store = svc.room_storage.clone();
        let mem_store = svc.member_storage.clone();

        room_store.create_room(room_id, moderator, "invite", "1", false).await.expect("create_room");
        mem_store.add_member(room_id, moderator, "join", None, None, None, None).await.expect("mod join");
        mem_store.add_member(room_id, target, "join", None, None, None, None).await.expect("target join");

        // Seed the target user so ban_user's active_user_exists check passes.
        let username = target.trim_start_matches('@').split(':').next().unwrap_or(target).to_string();
        user_store.create_user(target, &username, None, false).await.expect("create_user");

        svc.ban_user(room_id, target, moderator, Some("repeated spam")).await.expect("ban_user should succeed");

        let member = mem_store.get_room_member(room_id, target).await.expect("get_room_member");
        assert!(member.as_ref().is_some_and(|m| m.membership == "ban"), "target should be banned, got: {member:?}");
        // The reason is persisted on the membership row, not only in the event.
        assert_eq!(
            member.as_ref().and_then(|m| m.ban_reason.as_deref()),
            Some("repeated spam"),
            "ban reason should be recorded on the membership row"
        );
    }

    #[tokio::test]
    async fn ban_user_fails_when_target_not_found() {
        let (svc, _user_store) = build_membership_service();

        let room_id = "!test:test.localhost";
        let moderator = "@mod:test.localhost";

        let room_store = svc.room_storage.clone();
        room_store.create_room(room_id, moderator, "invite", "1", false).await.expect("create_room");

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

        room_store.create_room(room_id, unbanner, "invite", "1", false).await.expect("create_room");
        mem_store.add_member(room_id, unbanner, "join", None, None, None, None).await.expect("admin join");
        mem_store.add_member(room_id, target, "ban", None, None, None, None).await.expect("ban target");
        svc.set_ban_reason(room_id, target, Some("stale reason")).await.expect("seed ban reason");

        svc.unban_user(room_id, target, unbanner, None).await.expect("unban_user should succeed");

        let member = mem_store.get_room_member(room_id, target).await.expect("get_room_member");
        // After unban, membership row is deleted (not present).
        assert!(
            member.as_ref().is_some_and(|m| m.membership == "leave"),
            "unbanned user should have leave membership (mock converts ban→leave), got: {member:?}"
        );
        // The reason is cleared with the ban, so a later reasonless ban cannot
        // inherit it.
        assert_eq!(
            member.as_ref().and_then(|m| m.ban_reason.as_deref()),
            None,
            "unban should clear the recorded ban reason"
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

        room_store.create_room(room_id, admin, "invite", "1", false).await.expect("create_room");
        mem_store.add_member(room_id, admin, "join", None, None, None, None).await.expect("admin join");
        mem_store.add_member(room_id, target, "ban", None, None, None, None).await.expect("ban");
        // Seed the user so invite_user's active_user_exists check passes.
        let username = target.trim_start_matches('@').split(':').next().unwrap_or(target).to_string();
        user_store.create_user(target, &username, None, false).await.expect("create_user");

        svc.unban_user(room_id, target, admin, None).await.expect("unban");

        // After unban, invite should succeed (FakeRoomAuth allows it).
        svc.invite_user(room_id, admin, target, None).await.expect("invite after unban");

        let member = mem_store.get_room_member(room_id, target).await.expect("get_room_member");
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

        room_store.create_room(room_id, actor, "invite", "1", false).await.expect("create_room");
        mem_store.add_member(room_id, actor, "join", None, None, None, None).await.expect("mod join");
        mem_store.add_member(room_id, target, "join", None, None, None, None).await.expect("target join");

        // Seed the target user so kick_user's active_user_exists check passes.
        let username = target.trim_start_matches('@').split(':').next().unwrap_or(target).to_string();
        user_store.create_user(target, &username, None, false).await.expect("create_user");

        svc.kick_user(room_id, target, actor, Some("behaving badly")).await.expect("kick_user should succeed");

        let member = mem_store.get_room_member(room_id, target).await.expect("get_room_member");
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

        room_store.create_room(room_id, inviter, "invite", "1", false).await.expect("create_room");
        mem_store.add_member(room_id, inviter, "join", None, None, None, None).await.expect("inviter join");
        // Seed the user so invite_user's active_user_exists check passes.
        let username = invitee.trim_start_matches('@').split(':').next().unwrap_or(invitee).to_string();
        user_store.create_user(invitee, &username, None, false).await.expect("create_user");

        svc.invite_user(room_id, inviter, invitee, None).await.expect("invite_user should succeed");

        let member = mem_store.get_room_member(room_id, invitee).await.expect("get_room_member");
        assert!(
            member.as_ref().is_some_and(|m| m.membership == "invite"),
            "invitee should be in invite state, got: {member:?}"
        );
    }

    #[tokio::test]
    async fn invite_user_denied_by_policy_gate_is_rejected() {
        let (mut svc, _user_store) = build_membership_service();
        svc.invite_policy_gate = Arc::new(crate::test_mocks::FakeInvitePolicyGate::denying());

        let room_id = "!denied:test.localhost";
        let inviter = "@inviter:test.localhost";
        let invitee = "@invitee:test.localhost";

        let room_store = svc.room_storage.clone();
        let mem_store = svc.member_storage.clone();
        let user_store = svc.user_storage.clone();

        room_store.create_room(room_id, inviter, "invite", "1", false).await.expect("create_room");
        mem_store.add_member(room_id, inviter, "join", None, None, None, None).await.expect("inviter join");
        let username = invitee.trim_start_matches('@').split(':').next().unwrap_or(invitee).to_string();
        user_store.create_user(invitee, &username, None, false).await.expect("create_user");

        let err = svc
            .invite_user(room_id, inviter, invitee, None)
            .await
            .expect_err("the policy gate must reject this invite");
        assert_eq!(err.code, synapse_common::MatrixErrorCode::Forbidden, "expected M_FORBIDDEN, got: {err:?}");

        let member = mem_store.get_room_member(room_id, invitee).await.expect("get_room_member");
        assert!(
            member.as_ref().is_none_or(|m| m.membership != "invite"),
            "a rejected invite must not be persisted, got: {member:?}"
        );
    }

    /// The remote-invite branch returns early, so the gate has to run before
    /// it — otherwise inviting a remote user would be a way around the lists.
    #[tokio::test]
    async fn invite_user_gate_runs_before_the_remote_branch() {
        let (mut svc, _user_store) = build_membership_service();
        svc.invite_policy_gate = Arc::new(crate::test_mocks::FakeInvitePolicyGate::denying());

        let room_id = "!remote:test.localhost";
        let inviter = "@inviter:test.localhost";
        svc.room_storage.create_room(room_id, inviter, "invite", "1", false).await.expect("create_room");

        let err = svc
            .invite_user(room_id, inviter, "@remote:other.example", None)
            .await
            .expect_err("a remote invitee must not bypass the policy gate");
        assert_eq!(
            err.code,
            synapse_common::MatrixErrorCode::Forbidden,
            "expected the gate's M_FORBIDDEN, not a federation-client error: {err:?}"
        );
    }

    // ── I-01: moderation must keep the `room_summary_members` projection clean ──

    async fn seed_summary_member(svc: &MembershipService, room_id: &str, user_id: &str, membership: &str) {
        svc.room_summary_service
            .add_member(synapse_storage::room_summary::CreateSummaryMemberRequest {
                room_id: room_id.to_string(),
                user_id: user_id.to_string(),
                display_name: None,
                avatar_url: None,
                membership: membership.to_string(),
                is_hero: None,
                last_active_ts: None,
            })
            .await
            .expect("seed summary member");
    }

    #[tokio::test]
    async fn ban_user_removes_summary_projection() {
        let (svc, user_store) = build_membership_service();

        let room_id = "!ban-summary:test.localhost";
        let target = "@banned:test.localhost";
        let moderator = "@mod:test.localhost";

        svc.room_storage.create_room(room_id, moderator, "invite", "1", false).await.expect("create_room");
        svc.member_storage.add_member(room_id, moderator, "join", None, None, None, None).await.expect("mod join");
        svc.member_storage.add_member(room_id, target, "join", None, None, None, None).await.expect("target join");
        seed_summary_member(&svc, room_id, target, "join").await;

        let username = target.trim_start_matches('@').split(':').next().unwrap_or(target).to_string();
        user_store.create_user(target, &username, None, false).await.expect("create_user");

        svc.ban_user(room_id, target, moderator, Some("spam")).await.expect("ban_user should succeed");

        let members = svc.room_summary_service.get_members(room_id).await.expect("get_members");
        assert!(
            members.iter().all(|m| m.user_id != target),
            "a banned user must disappear from the summary projection, got: {members:?}"
        );
    }

    #[tokio::test]
    async fn kick_user_removes_summary_projection() {
        let (svc, user_store) = build_membership_service();

        let room_id = "!kick-summary:test.localhost";
        let target = "@kicked:test.localhost";
        let actor = "@mod:test.localhost";

        svc.room_storage.create_room(room_id, actor, "invite", "1", false).await.expect("create_room");
        svc.member_storage.add_member(room_id, actor, "join", None, None, None, None).await.expect("mod join");
        svc.member_storage.add_member(room_id, target, "join", None, None, None, None).await.expect("target join");
        seed_summary_member(&svc, room_id, target, "join").await;

        let username = target.trim_start_matches('@').split(':').next().unwrap_or(target).to_string();
        user_store.create_user(target, &username, None, false).await.expect("create_user");

        svc.kick_user(room_id, target, actor, Some("behaving badly")).await.expect("kick_user should succeed");

        let members = svc.room_summary_service.get_members(room_id).await.expect("get_members");
        assert!(
            members.iter().all(|m| m.user_id != target),
            "a kicked user must disappear from the summary projection, got: {members:?}"
        );
    }

    // =========================================================================
    // friend-DM moderation mirroring hook
    // =========================================================================

    #[cfg(feature = "friends")]
    #[derive(Debug, Clone, PartialEq, Eq)]
    struct RecordedSync {
        room_id: String,
        user_id: String,
        state: &'static str,
        changed_by: Option<String>,
        reason: Option<String>,
    }

    #[cfg(feature = "friends")]
    #[derive(Default)]
    struct RecordingFriendDmSync {
        calls: std::sync::Mutex<Vec<RecordedSync>>,
        fail: bool,
    }

    #[cfg(feature = "friends")]
    impl RecordingFriendDmSync {
        fn calls(&self) -> Vec<RecordedSync> {
            self.calls.lock().expect("lock").clone()
        }
    }

    #[cfg(feature = "friends")]
    #[async_trait::async_trait]
    impl crate::room::membership::friend_dm_sync::FriendDmMembershipSync for RecordingFriendDmSync {
        async fn sync_membership_state(
            &self,
            room_id: &str,
            affected_user_id: &str,
            state: crate::room::membership::friend_dm_sync::ModerationState,
            changed_by: Option<&str>,
            reason: Option<&str>,
        ) -> Result<usize, String> {
            self.calls.lock().expect("lock").push(RecordedSync {
                room_id: room_id.to_string(),
                user_id: affected_user_id.to_string(),
                state: state.as_str(),
                changed_by: changed_by.map(str::to_string),
                reason: reason.map(str::to_string),
            });
            if self.fail {
                Err("hook failure".to_string())
            } else {
                Ok(1)
            }
        }
    }

    #[cfg(feature = "friends")]
    async fn seed_moderation_fixture(
        svc: &MembershipService,
        user_store: &synapse_storage::test_mocks::FakeUserStore,
        room_id: &str,
        target: &str,
        actor: &str,
    ) {
        svc.room_storage.create_room(room_id, actor, "invite", "1", false).await.expect("create_room");
        svc.member_storage.add_member(room_id, actor, "join", None, None, None, None).await.expect("actor join");
        svc.member_storage.add_member(room_id, target, "join", None, None, None, None).await.expect("target join");
        let username = target.trim_start_matches('@').split(':').next().unwrap_or(target).to_string();
        user_store.create_user(target, &username, None, false).await.expect("create_user");
    }

    #[cfg(feature = "friends")]
    #[tokio::test]
    async fn ban_and_unban_dispatch_friend_dm_sync() {
        let (svc, user_store) = build_membership_service();
        let hook = Arc::new(RecordingFriendDmSync::default());
        svc.install_friend_dm_sync(hook.clone());

        let room_id = "!hook-ban:test.localhost";
        let target = "@hookbad:test.localhost";
        let actor = "@mod:test.localhost";
        seed_moderation_fixture(&svc, &user_store, room_id, target, actor).await;

        svc.ban_user(room_id, target, actor, Some("spam")).await.expect("ban_user");
        svc.unban_user(room_id, target, actor, None).await.expect("unban_user");

        assert_eq!(
            hook.calls(),
            vec![
                RecordedSync {
                    room_id: room_id.to_string(),
                    user_id: target.to_string(),
                    state: "banned",
                    changed_by: Some(actor.to_string()),
                    reason: Some("spam".to_string()),
                },
                RecordedSync {
                    room_id: room_id.to_string(),
                    user_id: target.to_string(),
                    state: "unbanned",
                    changed_by: Some(actor.to_string()),
                    reason: None,
                },
            ]
        );
    }

    #[cfg(feature = "friends")]
    #[tokio::test]
    async fn kick_dispatch_friend_dm_sync() {
        let (svc, user_store) = build_membership_service();
        let hook = Arc::new(RecordingFriendDmSync::default());
        svc.install_friend_dm_sync(hook.clone());

        let room_id = "!hook-kick:test.localhost";
        let target = "@hookkicked:test.localhost";
        let actor = "@mod:test.localhost";
        seed_moderation_fixture(&svc, &user_store, room_id, target, actor).await;

        svc.kick_user(room_id, target, actor, Some("behaving badly")).await.expect("kick_user");

        assert_eq!(
            hook.calls(),
            vec![RecordedSync {
                room_id: room_id.to_string(),
                user_id: target.to_string(),
                state: "kicked",
                changed_by: Some(actor.to_string()),
                reason: Some("behaving badly".to_string()),
            }]
        );
    }

    #[cfg(feature = "friends")]
    #[tokio::test]
    async fn admin_moderation_dispatches_friend_dm_sync() {
        let (svc, user_store) = build_membership_service();
        let hook = Arc::new(RecordingFriendDmSync::default());
        svc.install_friend_dm_sync(hook.clone());

        let room_id = "!hook-admin:test.localhost";
        let target = "@hooktarget:test.localhost";
        let admin = "@admin:test.localhost";
        seed_moderation_fixture(&svc, &user_store, room_id, target, admin).await;

        svc.admin_ban_user_membership(room_id, target, admin, Some("admin ban")).await.expect("admin ban");
        svc.admin_unban_user_membership(room_id, target, admin).await.expect("admin unban");
        svc.admin_kick_user_membership(room_id, target, admin, Some("admin kick")).await.expect("admin kick");

        assert_eq!(
            hook.calls(),
            vec![
                RecordedSync {
                    room_id: room_id.to_string(),
                    user_id: target.to_string(),
                    state: "banned",
                    changed_by: Some(admin.to_string()),
                    reason: Some("admin ban".to_string()),
                },
                RecordedSync {
                    room_id: room_id.to_string(),
                    user_id: target.to_string(),
                    state: "unbanned",
                    changed_by: Some(admin.to_string()),
                    reason: None,
                },
                RecordedSync {
                    room_id: room_id.to_string(),
                    user_id: target.to_string(),
                    state: "kicked",
                    changed_by: Some(admin.to_string()),
                    reason: Some("admin kick".to_string()),
                },
            ]
        );
    }

    #[cfg(feature = "friends")]
    #[tokio::test]
    async fn moderation_survives_absent_or_failing_hook() {
        // No hook installed: moderation must be a no-op for the mirror, not an error.
        let (svc, user_store) = build_membership_service();
        let room_id = "!hook-absent:test.localhost";
        let target = "@hookabsent:test.localhost";
        let actor = "@mod:test.localhost";
        seed_moderation_fixture(&svc, &user_store, room_id, target, actor).await;
        svc.ban_user(room_id, target, actor, None).await.expect("ban without hook");

        // Failing hook: the moderation write already happened, so the error is swallowed.
        let (svc, user_store) = build_membership_service();
        let hook = Arc::new(RecordingFriendDmSync { fail: true, ..Default::default() });
        svc.install_friend_dm_sync(hook.clone());
        let room_id = "!hook-failing:test.localhost";
        let target = "@hookfailing:test.localhost";
        seed_moderation_fixture(&svc, &user_store, room_id, target, actor).await;
        svc.admin_ban_user_membership(room_id, target, actor, None).await.expect("admin ban with failing hook");

        assert_eq!(hook.calls().len(), 1, "the failing hook must still have been invoked");
        let member = svc
            .member_storage
            .get_room_member(room_id, target)
            .await
            .expect("get_room_member")
            .expect("member present");
        assert_eq!(member.membership, "ban", "moderation must persist despite hook failure");
    }
}
