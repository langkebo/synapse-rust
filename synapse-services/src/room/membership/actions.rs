//! Room membership actions: join, leave, forget.

use crate::common::error::{ApiError, ApiErrorKind, ApiResult};
use serde_json::json;
use synapse_common::current_timestamp_millis;
use synapse_common::{generate_event_id, is_legal, JoinRule, Membership, TransitionCtx};
use synapse_storage::CreateEventParams;

use super::service::{MembershipService, RoomLocality};

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
            .map_err(|e| ApiError::internal_with_cause("Failed to check room existence", e))?;

        if room_exists {
            return self.join_room(room_id, user_id).await;
        }

        // Room doesn't exist locally — try federation join.
        // Pick a destination server: prefer the first via_server, otherwise
        // use the server name embedded in the room ID.
        let destination = via_servers
            .first()
            .cloned()
            .or_else(|| {
                // For legacy room IDs (`!<id>:server`), extract the server portion.
                // For v12+ domainless room IDs (`!<id>`), this returns None.
                room_id.rsplit_once(':').map(|(_, srv)| srv.to_string())
            })
            .ok_or_else(|| {
                if via_servers.is_empty() {
                    // Domainless room IDs require explicit via servers for federation joins.
                    ApiError::bad_request(
                        "Cannot join remote room: no destination server available. \
                         Domainless room IDs (v12+) require explicit via servers for federation joins."
                            .to_string(),
                    )
                } else {
                    ApiError::bad_request("Cannot join remote room: no destination server available".to_string())
                }
            })?;

        self.join_room_via_federation(&destination, room_id, user_id).await
    }

    /// See [`join_room`].
    #[::tracing::instrument(skip(self))]
    pub async fn join_room(&self, room_id: &str, user_id: &str) -> ApiResult<()> {
        if !self
            .room_storage
            .room_exists(room_id)
            .await
            .map_err(|e| ApiError::internal_with_cause("Failed to check room", e))?
        {
            return Err(ApiError::not_found("Room not found".to_string()));
        }

        // U-2: joining is an authorization path ("may this account act") ⇒
        // active predicate; a deactivated account must not join a room.
        if !self
            .user_storage
            .active_user_exists(user_id)
            .await
            .map_err(|e| ApiError::internal_with_cause("Failed to check user existence", e))?
        {
            return Err(ApiError::not_found("User not found".to_string()));
        }

        let (join_rule, _allow_rooms) = self.resolve_join_rule_and_allow(room_id).await?;
        let (from, target_is_banned) = self.resolve_membership_from(room_id, user_id).await?;

        // Idempotent no-op: already joined — don't emit a duplicate join event.
        if from == Some(Membership::Join) {
            return Ok(());
        }

        // Delegate the state-machine verdict to the single membership-transition
        // rulebook. Joins need no power level, so the state-only ctx is exact.
        // For restricted / knock_restricted rooms, resolve whether the joiner
        // satisfies an `allow` condition (MSC3083: `join` membership in one of
        // the allowed spaces). Non-restricted rules skip the extra lookup.
        let restricted_join_authorized = if matches!(join_rule, JoinRule::Restricted | JoinRule::KnockRestricted) {
            self.is_restricted_join_authorized(room_id, user_id).await?
        } else {
            false
        };
        let ctx = TransitionCtx::state_only(
            join_rule,
            /* actor_is_target */ true,
            target_is_banned,
            restricted_join_authorized,
        );
        is_legal(from, Membership::Join, &ctx)?;

        // MSC4284: consult the policy server before persisting the join.
        // Placed after the state-machine gate so we don't issue an HTTP request
        // for joins that are already rejected locally. No-op when no policy
        // service is configured.
        self.check_join_policy(room_id, user_id).await?;

        self.member_storage
            .add_member(room_id, user_id, "join", None, None, None, None)
            .await
            .map_err(|e| ApiError::internal_with_cause("Failed to join room", e))?;

        self.room_storage
            .increment_member_count(room_id)
            .await
            .map_err(|e| ApiError::internal_with_cause("Failed to update member count", e))?;

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
                    origin_server_ts: current_timestamp_millis(),
                    redacts: None,
                },
                None,
            )
            .await
            .map_err(|e| ApiError::internal_with_cause("Failed to record m.room.member join event", e))?;

        // Invalidate room-state cache after membership state change.
        let _ = self.cache.delete(&format!("room_state:{room_id}")).await;

        // Enqueue the join event for matching application services.
        self.dispatch_appservice_event(&join_event).await;

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

    /// See [`leave_room`].
    #[::tracing::instrument(skip(self))]
    pub async fn leave_room(&self, room_id: &str, user_id: &str) -> ApiResult<()> {
        // Locality comes from the room's ownership records, never from the id:
        // a room v12 id carries no server at all (G-21).
        if let RoomLocality::Remote { destinations } = self.room_locality(room_id).await? {
            return self.leave_remote_room(&destinations, room_id, user_id).await;
        }

        let existing_member = self
            .member_storage
            .get_room_member(room_id, user_id)
            .await
            .map_err(|e| ApiError::internal_with_cause("Failed to check membership before leave", e))?;

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
            .remove_member(room_id, user_id, None)
            .await
            .map_err(|e| ApiError::internal_with_cause("Failed to leave room", e))?;

        if existing_member.as_ref().is_some_and(|member| member.membership == "join") {
            self.room_storage
                .decrement_member_count(room_id, None)
                .await
                .map_err(|e| ApiError::internal_with_cause("Failed to update member count", e))?;
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
                    origin_server_ts: current_timestamp_millis(),
                    redacts: None,
                },
                None,
            )
            .await
            .map_err(|e| ApiError::internal_with_cause("Failed to record m.room.member leave event", e))?;

        // Invalidate room-state cache after membership state change.
        let _ = self.cache.delete(&format!("room_state:{room_id}")).await;

        // Best-effort: sign and broadcast the leave event to federation peers.
        if let Err(e) = self.sign_and_broadcast_event(&leave_event).await {
            ::tracing::warn!(
                room_id = %room_id,
                user_id = %user_id,
                error = %e,
                "Failed to sign and broadcast leave event"
            );
        }

        // Forward secrecy: when a member leaves a LOCAL encrypted room, mark the
        // room's megolm session for rotation so the departed member cannot
        // decrypt future messages. Remote rooms return early above.
        self.trigger_key_rotation_on_leave(room_id, user_id).await;

        Ok(())
    }

    /// Leave a **remote** room by trying its resident servers in order.
    ///
    /// A room can have several residents, so the destination is a list (derived
    /// from the room's ownership records by
    /// [`room_locality`](Self::room_locality)) rather than a single server
    /// guessed from the room id. Candidates are tried best-first until one
    /// completes the make_leave / send_leave exchange.
    ///
    /// Only a failure of the *federation exchange* is retried against the next
    /// candidate: `BadRequest` / `Forbidden` are produced by the ACL check,
    /// make_leave, template validation and send_leave, all of which run before
    /// any local write, so a retry cannot corrupt local state. Any other error
    /// kind comes from signing or local persistence, which happens after the
    /// remote exchange; it is surfaced immediately instead of being replayed
    /// against another resident.
    async fn leave_remote_room(&self, destinations: &[String], room_id: &str, user_id: &str) -> ApiResult<()> {
        let mut last_error: Option<ApiError> = None;
        for destination in destinations {
            match self.leave_room_via_federation(destination, room_id, user_id).await {
                Ok(()) => return Ok(()),
                Err(err) => {
                    let retryable = matches!(err.kind, ApiErrorKind::BadRequest | ApiErrorKind::Forbidden);
                    ::tracing::warn!(
                        room_id = %room_id,
                        user_id = %user_id,
                        destination = %destination,
                        retryable = retryable,
                        error = %err,
                        "Federation leave failed"
                    );
                    last_error = Some(err);
                    if !retryable {
                        break;
                    }
                }
            }
        }
        Err(last_error.unwrap_or_else(|| {
            ApiError::bad_request(format!(
                "Cannot leave remote room {room_id}: no resident server known and this server does not host it"
            ))
        }))
    }

    /// Forward-secrecy helper: when a member leaves an encrypted room, mark
    /// the room's megolm session for rotation so the departed member cannot
    /// decrypt future messages.
    ///
    /// This is called automatically by [`leave_room`](Self::leave_room) for
    /// client-initiated leaves, and should also be called by federation
    /// `send_leave` / `send_leave_v2` handlers when a remote user leaves a
    /// locally-hosted encrypted room.
    ///
    /// No-op for unencrypted rooms or when key rotation storage is not
    /// configured.
    ///
    /// SECURITY: If `get_state_events_by_type` fails, we conservatively
    /// proceed with key rotation to avoid leaving encrypted room keys
    /// in an undefined state. A failure to fetch encryption state should
    /// not silently skip the security-critical rotation path — we attempt
    /// key rotation regardless and log the warning.
    pub async fn trigger_key_rotation_on_leave(&self, room_id: &str, user_id: &str) {
        if let Some(key_rotation_storage) = &self.key_rotation_storage {
            // SECURITY: On error, still attempt key rotation (might be encrypted).
            // A failed check should not skip key rotation for encrypted rooms.
            let encryption_state = match self.get_state_events_by_type(room_id, "m.room.encryption").await {
                Ok(events) => events,
                Err(e) => {
                    ::tracing::warn!(
                        room_id = %room_id,
                        user_id = %user_id,
                        error = %e,
                        "Failed to fetch encryption state for key rotation on leave"
                    );
                    // Proceed with key rotation attempt anyway (conservative)
                    if let Err(e) = key_rotation_storage.mark_key_rotation_needed(room_id, user_id).await {
                        ::tracing::warn!(
                            room_id = %room_id,
                            user_id = %user_id,
                            error = %e,
                            "Failed to mark key rotation needed after leave of encrypted room"
                        );
                    }
                    return;
                }
            };
            if !encryption_state.is_empty() {
                if let Err(e) = key_rotation_storage.mark_key_rotation_needed(room_id, user_id).await {
                    ::tracing::warn!(
                        room_id = %room_id,
                        user_id = %user_id,
                        error = %e,
                        "Failed to mark key rotation needed after leave of encrypted room"
                    );
                }
            }
        }
    }

    /// See [`forget_room`].
    pub async fn forget_room(&self, room_id: &str, user_id: &str) -> ApiResult<()> {
        let membership = self
            .member_storage
            .get_room_member(room_id, user_id)
            .await
            .map_err(|e| ApiError::internal_with_cause("Failed to check membership", e))?;

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
                        .forget_member(room_id, user_id, None)
                        .await
                        .map_err(|e| ApiError::internal_with_cause("Failed to forget room", e))?;
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

    /// MSC4267: atomic leave + forget in a single DB transaction.
    ///
    /// Per the spec, when a client POSTs `/rooms/{id}/leave` with
    /// `forget: true` the server MUST run the leave and the forget in the
    /// same transaction so that no other writer can observe a "leave
    /// without forget" intermediate state. The race window in the two-step
    /// flow (where another client could re-join, or a federation leave
    /// event could land, between the leave and the forget) is closed by
    /// binding both writes to a single `BEGIN ... COMMIT` boundary.
    ///
    /// Federation leave is unaffected — it is a separate code path
    /// (`leave_room_via_federation`) and does not participate in
    /// leave+forget semantics. Clients that want to leave a remote room
    /// AND forget it locally must first leave via the remote server
    /// (which propagates the leave event back), and then call
    /// `/forget` explicitly.
    pub async fn leave_and_forget(&self, room_id: &str, user_id: &str) -> ApiResult<()> {
        // For remote rooms we still want the local forget to be atomic
        // with whatever membership state is left after the federated
        // leave — but `leave_room_via_federation` is the only path that
        // can mark the membership as 'leave' in that case. Refuse to
        // combine because the federation hop is its own transaction.
        // Locality is an ownership question, not an id-parsing one (G-21).
        if let RoomLocality::Remote { .. } = self.room_locality(room_id).await? {
            return Err(ApiError::bad_request(
                "MSC4267 leave+forget is only valid for local rooms. Leave the remote room first, then call /forget."
                    .to_string(),
            ));
        }

        // Pre-flight: check the current membership is in a legal
        // 'Leave' transition (matches leave_room's guard at line 154).
        let existing_member = self
            .member_storage
            .get_room_member(room_id, user_id)
            .await
            .map_err(|e| ApiError::internal_with_cause("Failed to check membership before leave+forget", e))?;
        let current_state =
            existing_member.as_ref().and_then(|m| super::transition::MembershipState::parse_opt(&m.membership));
        if let Err(msg) = super::transition::is_legal(
            current_state,
            super::transition::MembershipState::Leave,
            &super::transition::TransitionContext::default(),
        ) {
            return Err(ApiError::forbidden(msg.to_string()));
        }

        // MSC4267 atomic path. We need a DB pool; if the service was built
        // without one (test_mocks), fall back to the non-atomic two-call
        // path so existing tests keep working.
        let Some(pool) = self.db_pool.as_ref() else {
            // Fallback: best-effort non-atomic leave+forget for environments
            // without a real DB pool (e.g. in-memory mocks).
            self.leave_room(room_id, user_id).await?;
            // forget_room itself does get_room_member + forget_member; if the
            // member was 'join' it would have been downgraded to 'leave' by
            // leave_room above so the forget is now legal.
            self.forget_room(room_id, user_id).await?;
            return Ok(());
        };

        let mut tx = pool
            .begin()
            .await
            .map_err(|e| ApiError::internal_with_cause("Failed to begin leave+forget transaction", e))?;

        // Step 1 (in tx): mark membership 'leave' and zero the
        // member count delta.
        self.member_storage
            .remove_member(room_id, user_id, Some(&mut tx))
            .await
            .map_err(|e| ApiError::internal_with_cause("Failed to leave room (in transaction)", e))?;

        if existing_member.as_ref().is_some_and(|member| member.membership == "join") {
            self.room_storage
                .decrement_member_count(room_id, Some(&mut tx))
                .await
                .map_err(|e| ApiError::internal_with_cause("Failed to update member count (in transaction)", e))?;
        }

        // Step 2 (in tx): mark membership 'forget' so the user no
        // longer sees the room in their list. Combined with step 1
        // this means the room is gone in a single COMMIT — no other
        // client can observe the intermediate 'leave' state.
        self.member_storage
            .forget_member(room_id, user_id, Some(&mut tx))
            .await
            .map_err(|e| ApiError::internal_with_cause("Failed to forget room (in transaction)", e))?;

        tx.commit().await.map_err(|e| ApiError::internal_with_cause("Failed to commit leave+forget transaction", e))?;

        // Best-effort post-commit work — same as leave_room. Failures
        // here are logged but not surfaced to the client; the
        // authoritative state is in the DB.
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
                    origin_server_ts: current_timestamp_millis(),
                    redacts: None,
                },
                None,
            )
            .await
            .map_err(|e| ApiError::internal_with_cause("Failed to record m.room.member leave event", e))?;

        let _ = self.cache.delete(&format!("room_state:{room_id}")).await;

        if let Err(e) = self.sign_and_broadcast_event(&leave_event).await {
            ::tracing::warn!(
                room_id = %room_id,
                user_id = %user_id,
                error = %e,
                "Failed to sign and broadcast leave event (leave+forget path)"
            );
        }

        // Forward secrecy: rotate the room's megolm session so the
        // departed user cannot decrypt future messages.
        self.trigger_key_rotation_on_leave(room_id, user_id).await;

        ::tracing::info!(
            target: "security_audit",
            room_id = %room_id,
            user_id = %user_id,
            "MSC4267 leave+forget completed in single transaction"
        );

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use crate::common::error::ApiError;
    use std::sync::Arc;

    use synapse_cache::{CacheConfig, CacheManager};
    use synapse_e2ee::test_mocks::InMemoryKeyRotationStorage;
    use synapse_storage::event::{EventReader, EventWriter};
    use synapse_storage::test_mocks::room_summary::InMemoryRoomSummaryStore;
    use synapse_storage::test_mocks::{FakeUserStore, InMemoryEventStore, InMemoryMemberStore, InMemoryRoomStore};
    use synapse_storage::{MemberStoreApi, RoomStoreApi, UserStore};

    use crate::room::summary::RoomSummaryService;
    use crate::test_mocks::FakeRoomAuth;

    use super::super::service::{MembershipService, MembershipServiceConfig, RoomLocality};

    const ROOM_ID: &str = "!enc:localhost";
    const USER_ID: &str = "@bob:localhost";

    /// Build a [`MembershipService`] wired with in-memory mocks and the given
    /// key-rotation spy, seeded with `@bob:localhost` joined to `!enc:localhost`.
    ///
    /// The `m.room.create` event is seeded too: locality is decided from room
    /// ownership (G-21), so a fixture that wants a *local* room must record a
    /// create event that originated here.
    async fn build_service(spy: Arc<InMemoryKeyRotationStorage>) -> MembershipService {
        let member_store = InMemoryMemberStore::new();
        member_store.add_member(ROOM_ID, USER_ID, "join", None).await.unwrap();

        let event_store = Arc::new(InMemoryEventStore::new());
        event_store
            .create_event(synapse_storage::CreateEventParams {
                event_id: "$create:localhost".to_string(),
                room_id: ROOM_ID.to_string(),
                user_id: "@alice:localhost".to_string(),
                event_type: "m.room.create".to_string(),
                content: serde_json::json!({ "room_version": "10" }),
                state_key: Some(String::new()),
                origin_server_ts: 1_000,
                redacts: None,
            })
            .await
            .unwrap();
        let room_store = InMemoryRoomStore::new();

        let event_reader: Arc<dyn EventReader> = event_store.clone();
        let event_writer: Arc<dyn EventWriter> = event_store.clone();
        let member_storage: Arc<dyn MemberStoreApi> = Arc::new(member_store);
        let room_storage: Arc<dyn RoomStoreApi> = Arc::new(room_store);
        let user_storage: Arc<dyn UserStore> = Arc::new(FakeUserStore::new());
        let room_summary_service = Arc::new(RoomSummaryService::new(
            Arc::new(InMemoryRoomSummaryStore::new()),
            event_reader.clone(),
            Some(member_storage.clone()),
        ));

        MembershipService::new(MembershipServiceConfig {
            member_storage,
            room_storage,
            event_reader,
            event_writer,
            user_storage,
            room_auth: Arc::new(FakeRoomAuth::new()),
            server_name: "localhost".to_string(),
            federation_client: None,
            key_rotation_manager: None,
            event_broadcaster: None,
            room_summary_service,
            cache: Arc::new(CacheManager::new(&CacheConfig::default())),
            key_rotation_storage: Some(spy),
            app_service_manager: None,
            db_pool: None,
            policy_service: None,
            invite_policy_gate: Arc::new(crate::test_mocks::FakeInvitePolicyGate::new()),
        })
    }

    /// Seed an `m.room.encryption` state event into the service's event store.
    async fn seed_encryption_event(svc: &MembershipService) {
        svc.event_writer
            .create_event(
                synapse_storage::CreateEventParams {
                    event_id: "$enc:localhost".to_string(),
                    room_id: ROOM_ID.to_string(),
                    user_id: USER_ID.to_string(),
                    event_type: "m.room.encryption".to_string(),
                    content: serde_json::json!({ "algorithm": "m.megolm.v1.aes-sha2" }),
                    state_key: Some("".to_string()),
                    origin_server_ts: 1_000,
                    redacts: None,
                },
                None,
            )
            .await
            .unwrap();
    }

    #[tokio::test]
    async fn leave_encrypted_room_marks_key_rotation() {
        let spy = Arc::new(InMemoryKeyRotationStorage::new());
        let svc = build_service(spy.clone()).await;
        seed_encryption_event(&svc).await;

        svc.leave_room(ROOM_ID, USER_ID).await.unwrap();

        assert_eq!(spy.marked_rotations().await, vec![(ROOM_ID.to_string(), USER_ID.to_string())]);
    }

    #[tokio::test]
    async fn leave_unencrypted_room_does_not_mark_rotation() {
        let spy = Arc::new(InMemoryKeyRotationStorage::new());
        let svc = build_service(spy.clone()).await;
        // No m.room.encryption state event seeded.

        svc.leave_room(ROOM_ID, USER_ID).await.unwrap();

        assert!(spy.marked_rotations().await.is_empty());
    }

    /// Build a service for *join* tests: creates a room with the given
    /// join_rule, seeds the `m.room.join_rules` state event, and optionally
    /// pre-seeds `@bob` with a membership state (e.g. "invite").
    async fn build_join_service(join_rule: &str, seed_bob_membership: Option<&str>) -> MembershipService {
        let member_store = InMemoryMemberStore::new();
        if let Some(mem) = seed_bob_membership {
            member_store.add_member(ROOM_ID, USER_ID, mem, None).await.unwrap();
        }

        let event_store = Arc::new(InMemoryEventStore::new());
        let room_store = InMemoryRoomStore::new();
        // Seed a room with the specified join_rule.
        room_store.create_room(ROOM_ID, "@alice:localhost", join_rule, "10", false).await.unwrap();
        // Seed the m.room.join_rules state event so resolve_join_rule picks it up.
        event_store
            .create_event(synapse_storage::CreateEventParams {
                event_id: "$join_rules:localhost".to_string(),
                room_id: ROOM_ID.to_string(),
                user_id: "@alice:localhost".to_string(),
                event_type: "m.room.join_rules".to_string(),
                content: serde_json::json!({ "join_rule": join_rule }),
                state_key: Some("".to_string()),
                origin_server_ts: 1_000,
                redacts: None,
            })
            .await
            .unwrap();

        let event_reader: Arc<dyn EventReader> = event_store.clone();
        let event_writer: Arc<dyn EventWriter> = event_store.clone();
        let member_storage: Arc<dyn MemberStoreApi> = Arc::new(member_store);
        let room_storage: Arc<dyn RoomStoreApi> = Arc::new(room_store);

        let fake_user_store = FakeUserStore::new();
        // Seed @bob as a user so active_user_exists returns true.
        fake_user_store
            .seed_user(synapse_storage::User {
                user_id: USER_ID.to_string(),
                username: "bob".to_string(),
                password_hash: None,
                is_admin: false,
                is_guest: false,
                is_shadow_banned: false,
                is_deactivated: false,
                created_ts: 0,
                updated_ts: None,
                displayname: None,
                avatar_url: None,
                email: None,
                phone: None,
                generation: None,
                consent_version: None,
                appservice_id: None,
                user_type: None,
                invalid_update_at: None,
                migration_state: None,
                password_changed_ts: None,
                is_password_change_required: false,
                password_expires_at: None,
                failed_login_attempts: 0,
                locked_until: None,
                must_change_password: false,
            })
            .await;
        let user_storage: Arc<dyn UserStore> = Arc::new(fake_user_store);
        let room_summary_service = Arc::new(RoomSummaryService::new(
            Arc::new(InMemoryRoomSummaryStore::new()),
            event_reader.clone(),
            Some(member_storage.clone()),
        ));

        MembershipService::new(MembershipServiceConfig {
            member_storage,
            room_storage,
            event_reader,
            event_writer,
            user_storage,
            room_auth: Arc::new(FakeRoomAuth::new()),
            server_name: "localhost".to_string(),
            federation_client: None,
            key_rotation_manager: None,
            event_broadcaster: None,
            room_summary_service,
            cache: Arc::new(CacheManager::new(&CacheConfig::default())),
            key_rotation_storage: None,
            app_service_manager: None,
            db_pool: None,
            policy_service: None,
            invite_policy_gate: Arc::new(crate::test_mocks::FakeInvitePolicyGate::new()),
        })
    }

    /// Verify restricted-join auth resolution: a room with
    /// `m.room.join_rules: {"join_rule": "restricted"}` requires an
    /// explicit invite — joining without one is rejected (fail-closed).
    /// This is the documented behavior: restricted-join authorization
    /// resolution is not yet wired (actions.rs:80), so restricted rooms
    /// fail closed and return an M_FORBIDDEN error.
    #[tokio::test]
    async fn restricted_join_without_invite_fails_closed() {
        // User @bob is NOT seeded as a member (from == None).
        let svc = build_join_service("restricted", None).await;

        let err = svc.join_room(ROOM_ID, USER_ID).await.unwrap_err();
        assert_eq!(err, ApiError::forbidden("You are not invited to this room"));
    }

    /// Verify that an already-invited member can join a restricted room.
    /// The Invite arm of `is_legal` always returns Ok (regardless of
    /// join_rule), so an explicit invite bypasses the restricted fail-close.
    #[tokio::test]
    async fn restricted_join_with_invite_succeeds() {
        // Pre-seed @bob with an "invite" membership state.
        let svc = build_join_service("restricted", Some("invite")).await;

        let result = svc.join_room(ROOM_ID, USER_ID).await;
        assert!(result.is_ok(), "join with invite should succeed under restricted join_rule");
    }

    /// Sanity check: public rooms accept joins without an invite.
    #[tokio::test]
    async fn public_join_without_invite_succeeds() {
        let svc = build_join_service("public", None).await;

        let result = svc.join_room(ROOM_ID, USER_ID).await;
        assert!(result.is_ok(), "join should succeed under public join_rule");
    }

    // ── G-21: locality comes from room ownership, not the id spelling ──

    /// A room v12 / MSC4291 room id: `!` + 43 URL-safe base64 characters and
    /// **no `:server`**. Any code that parses the id for a server finds nothing
    /// in it, which is exactly why locality must come from the room's records.
    const DOMAINLESS_ROOM_ID: &str = "!AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA";
    /// The creator of a room hosted by another homeserver.
    const REMOTE_CREATOR: &str = "@alice:remote.example";

    /// Build a service whose only record of `room_id` is its `m.room.create`
    /// state event, sent by `creator`, with `@bob:localhost` joined.
    /// Locality can therefore only be decided from that create event.
    async fn build_locality_service(room_id: &str, creator: &str) -> MembershipService {
        let member_store = InMemoryMemberStore::new();
        member_store.add_member(room_id, USER_ID, "join", None).await.unwrap();

        let event_store = Arc::new(InMemoryEventStore::new());
        event_store
            .create_event(synapse_storage::CreateEventParams {
                event_id: format!("$create{room_id}"),
                room_id: room_id.to_string(),
                user_id: creator.to_string(),
                event_type: "m.room.create".to_string(),
                content: serde_json::json!({ "room_version": "12" }),
                state_key: Some(String::new()),
                origin_server_ts: 1,
                redacts: None,
            })
            .await
            .unwrap();

        let room_store = InMemoryRoomStore::new();

        let event_reader: Arc<dyn EventReader> = event_store.clone();
        let event_writer: Arc<dyn EventWriter> = event_store.clone();
        let member_storage: Arc<dyn MemberStoreApi> = Arc::new(member_store);
        let room_storage: Arc<dyn RoomStoreApi> = Arc::new(room_store);
        let user_storage: Arc<dyn UserStore> = Arc::new(FakeUserStore::new());
        let room_summary_service = Arc::new(RoomSummaryService::new(
            Arc::new(InMemoryRoomSummaryStore::new()),
            event_reader.clone(),
            Some(member_storage.clone()),
        ));

        MembershipService::new(MembershipServiceConfig {
            member_storage,
            room_storage,
            event_reader,
            event_writer,
            user_storage,
            room_auth: Arc::new(FakeRoomAuth::new()),
            server_name: "localhost".to_string(),
            federation_client: None,
            key_rotation_manager: None,
            event_broadcaster: None,
            room_summary_service,
            cache: Arc::new(CacheManager::new(&CacheConfig::default())),
            key_rotation_storage: None,
            app_service_manager: None,
            db_pool: None,
            policy_service: None,
            invite_policy_gate: Arc::new(crate::test_mocks::FakeInvitePolicyGate::new()),
        })
    }

    /// A domainless room whose create event originated on another homeserver is
    /// **remote**, so leaving it must go through federation. Parsing the id
    /// (`server_name_from_id` -> `None` -> `is_remote_room` false) called it
    /// local and ran the local write path instead.
    #[tokio::test]
    async fn domainless_remote_room_leave_room_routes_to_federation() {
        let svc = build_locality_service(DOMAINLESS_ROOM_ID, REMOTE_CREATOR).await;

        // No federation client is configured in this fixture, so the remote
        // path must fail rather than silently perform a *local* leave.
        let err = svc.leave_room(DOMAINLESS_ROOM_ID, USER_ID).await.unwrap_err();
        assert_eq!(
            err.kind,
            synapse_common::ApiErrorKind::Internal,
            "the leave must be routed to federation (no client configured), got: {err:?}"
        );
        assert!(
            err.message.contains("Federation client not configured"),
            "must fail on the federation path, got: {err:?}"
        );

        // The local membership is untouched — proof the local write path was
        // not taken.
        assert_eq!(
            svc.member_storage.get_membership_state(DOMAINLESS_ROOM_ID, USER_ID).await.unwrap(),
            Some("join".to_string()),
            "a remote room's local membership record must not be rewritten locally"
        );
    }

    /// MSC4267 `leave_and_forget` is local-only and must refuse a remote room.
    /// Id-parsing locality made the domainless remote room look local, so the
    /// refusal never fired and the local leave+forget ran instead.
    #[tokio::test]
    async fn leave_and_forget_refuses_domainless_remote_room() {
        let svc = build_locality_service(DOMAINLESS_ROOM_ID, REMOTE_CREATOR).await;

        let err = svc.leave_and_forget(DOMAINLESS_ROOM_ID, USER_ID).await.unwrap_err();
        assert_eq!(err.kind, synapse_common::ApiErrorKind::BadRequest, "unexpected error: {err:?}");
        assert!(err.message.contains("only valid for local rooms"), "unexpected error: {err:?}");
        assert_eq!(
            svc.member_storage.get_membership_state(DOMAINLESS_ROOM_ID, USER_ID).await.unwrap(),
            Some("join".to_string()),
            "the refusal must not touch local membership"
        );
    }

    /// Positive control: a domainless room whose create event originated here is
    /// still local and still takes the local leave path.
    #[tokio::test]
    async fn domainless_local_room_stays_local() {
        let svc = build_locality_service(DOMAINLESS_ROOM_ID, "@alice:localhost").await;

        svc.leave_room(DOMAINLESS_ROOM_ID, USER_ID).await.unwrap();

        assert_ne!(
            svc.member_storage.get_membership_state(DOMAINLESS_ROOM_ID, USER_ID).await.unwrap(),
            Some("join".to_string()),
            "a locally hosted room must still be left on the local path"
        );
    }

    /// The locality decision itself: a remote room's resident servers are the
    /// room's origin server (from the create event) and every server with a
    /// joined member — the id supplies neither.
    #[tokio::test]
    async fn room_locality_lists_the_remote_residents() {
        // A second server's member joined the room, so the room has two
        // residents: the creator's server and `other.example`.
        let member_store = InMemoryMemberStore::new();
        member_store.add_member(DOMAINLESS_ROOM_ID, USER_ID, "join", None).await.unwrap();
        member_store.add_member(DOMAINLESS_ROOM_ID, "@carol:other.example", "join", None).await.unwrap();

        let event_store = Arc::new(InMemoryEventStore::new());
        event_store
            .create_event(synapse_storage::CreateEventParams {
                event_id: format!("$create{DOMAINLESS_ROOM_ID}"),
                room_id: DOMAINLESS_ROOM_ID.to_string(),
                user_id: REMOTE_CREATOR.to_string(),
                event_type: "m.room.create".to_string(),
                content: serde_json::json!({ "room_version": "12" }),
                state_key: Some(String::new()),
                origin_server_ts: 1,
                redacts: None,
            })
            .await
            .unwrap();

        let event_reader: Arc<dyn EventReader> = event_store.clone();
        let event_writer: Arc<dyn EventWriter> = event_store.clone();
        let member_storage: Arc<dyn MemberStoreApi> = Arc::new(member_store);
        let user_storage: Arc<dyn UserStore> = Arc::new(FakeUserStore::new());
        let room_summary_service = Arc::new(RoomSummaryService::new(
            Arc::new(InMemoryRoomSummaryStore::new()),
            event_reader.clone(),
            Some(member_storage.clone()),
        ));
        let svc = MembershipService::new(MembershipServiceConfig {
            member_storage,
            room_storage: Arc::new(InMemoryRoomStore::new()),
            event_reader,
            event_writer,
            user_storage,
            room_auth: Arc::new(FakeRoomAuth::new()),
            server_name: "localhost".to_string(),
            federation_client: None,
            key_rotation_manager: None,
            event_broadcaster: None,
            room_summary_service,
            cache: Arc::new(CacheManager::new(&CacheConfig::default())),
            key_rotation_storage: None,
            app_service_manager: None,
            db_pool: None,
            policy_service: None,
            invite_policy_gate: Arc::new(crate::test_mocks::FakeInvitePolicyGate::new()),
        });

        assert_eq!(
            svc.room_locality(DOMAINLESS_ROOM_ID).await.unwrap(),
            RoomLocality::Remote { destinations: vec!["remote.example".to_string(), "other.example".to_string()] },
            "origin server first, then the other joined residents"
        );
    }

    /// Fail-closed: a domainless room with no ownership record at all is not
    /// proven local, so it is remote — with no destination to contact, and the
    /// caller refuses rather than taking the local path.
    #[tokio::test]
    async fn unknown_domainless_room_fails_closed_to_remote() {
        // The seeded room is DOMAINLESS_ROOM_ID; query a different one.
        let svc = build_locality_service(DOMAINLESS_ROOM_ID, REMOTE_CREATOR).await;
        let unknown = "!BBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBB";

        assert_eq!(svc.room_locality(unknown).await.unwrap(), RoomLocality::Remote { destinations: vec![] });

        let err = svc.leave_and_forget(unknown, USER_ID).await.unwrap_err();
        assert_eq!(err.kind, synapse_common::ApiErrorKind::BadRequest);
        assert!(err.message.contains("only valid for local rooms"), "unexpected error: {err:?}");
    }
}
