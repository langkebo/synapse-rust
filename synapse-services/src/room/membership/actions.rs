//! Room membership actions: join, leave, forget.

use crate::common::error::{ApiError, ApiResult};
use serde_json::json;
use synapse_common::{generate_event_id, is_legal, Membership, TransitionCtx};
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

        let join_rule = self.resolve_join_rule(room_id).await?;
        let (from, target_is_banned) = self.resolve_membership_from(room_id, user_id).await?;

        // Idempotent no-op: already joined — don't emit a duplicate join event.
        if from == Some(Membership::Join) {
            return Ok(());
        }

        // Delegate the state-machine verdict to the single membership-transition
        // rulebook. Joins need no power level, so the state-only ctx is exact;
        // restricted-join authorization resolution is not yet wired, so
        // restricted rooms fail closed (require an explicit invite).
        let ctx = TransitionCtx::state_only(join_rule, /* actor_is_target */ true, target_is_banned, false);
        is_legal(from, Membership::Join, &ctx)?;

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
        if let Some(key_rotation_storage) = &self.key_rotation_storage {
            let encryption_state =
                self.get_state_events_by_type(room_id, "m.room.encryption").await.unwrap_or_default();
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

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use synapse_cache::{CacheConfig, CacheManager};
    use synapse_e2ee::test_mocks::InMemoryKeyRotationStorage;
    use synapse_storage::test_mocks::room_summary::InMemoryRoomSummaryStore;
    use synapse_storage::event::{EventReader, EventWriter};
    use synapse_storage::test_mocks::{FakeUserStore, InMemoryEventStore, InMemoryMemberStore, InMemoryRoomStore};
    use synapse_storage::{MemberStoreApi, RoomStoreApi, UserStore};

    use crate::room::summary::RoomSummaryService;
    use crate::test_mocks::FakeRoomAuth;
    use crate::user_service::UserService;

    use super::super::service::{MembershipService, MembershipServiceConfig};

    const ROOM_ID: &str = "!enc:localhost";
    const USER_ID: &str = "@bob:localhost";

    /// Build a [`MembershipService`] wired with in-memory mocks and the given
    /// key-rotation spy, seeded with `@bob:localhost` joined to `!enc:localhost`.
    async fn build_service(spy: Arc<InMemoryKeyRotationStorage>) -> MembershipService {
        let member_store = InMemoryMemberStore::new();
        member_store.add_member(ROOM_ID, USER_ID, "join", None).await.unwrap();

        let event_store = Arc::new(InMemoryEventStore::new());
        let room_store = InMemoryRoomStore::new();

        let event_reader: Arc<dyn EventReader> = event_store.clone();
        let event_writer: Arc<dyn EventWriter> = event_store.clone();
        let member_storage: Arc<dyn MemberStoreApi> = Arc::new(member_store);
        let room_storage: Arc<dyn RoomStoreApi> = Arc::new(room_store);
        let user_storage: Arc<dyn UserStore> = Arc::new(FakeUserStore::new());
        let user_service = Arc::new(UserService::new(user_storage.clone()));

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
            user_service,
            room_auth: Arc::new(FakeRoomAuth::new()),
            server_name: "localhost".to_string(),
            federation_client: None,
            key_rotation_manager: None,
            event_broadcaster: None,
            room_summary_service,
            cache: Arc::new(CacheManager::new(&CacheConfig::default())),
            key_rotation_storage: Some(spy),
            app_service_manager: None,
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
}
