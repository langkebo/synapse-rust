//! Domain service for room membership operations — join, leave, invite,
//! kick, ban, unban, knock, forget, and federation membership.
//!
//! Extracted from RoomService as part of the domain split plan (Task 1).

use crate::account::UserService;
use crate::common::error::{ApiError, ApiResult};
use crate::room::membership::error::MembershipError;
use crate::policy_service::PolicyService;
use serde_json::json;
use std::str::FromStr;
use std::sync::Arc;
use synapse_cache::CacheManager;
use synapse_common::{is_legal, JoinRule, Membership, TransitionCtx};
use synapse_federation::client_api::FederationClientApi;
use synapse_federation::key_rotation::SigningKey;
use synapse_federation::signing::sign_and_hash_event;
use synapse_federation::KeyRotationManager;
use synapse_storage::event::RoomEvent;
use synapse_storage::{MemberStoreApi, RoomStoreApi, UserStore};

use synapse_e2ee::key_rotation::KeyRotationStorageApi;

use crate::room::summary::RoomSummaryService;

// MSC3083 `allow`-array parsing now lives in the single canonical
// `room::join_rules` module so the authorization gate and the `/summary`
// projection cannot drift apart again (see its module docs, and
// `docs/audit/AUDIT_SUMMARY_2026-09-12.md` §3). Re-exported here to keep the
// existing intra-crate call sites and `super::*` test imports unchanged.
pub(crate) use crate::room::join_rules::extract_allowed_join_rooms;

/// Domain service for room membership operations — join, leave, invite,
/// kick, ban, unban, knock, forget, and federation membership.
#[derive(Clone)]
#[allow(dead_code)] // Reserved fields for future use; see field-level comments.
pub struct MembershipService {
    pub(crate) member_storage: Arc<dyn MemberStoreApi>,
    pub(crate) room_storage: Arc<dyn RoomStoreApi>,
    pub(crate) event_reader: Arc<dyn synapse_storage::event::EventReader>,
    pub(crate) event_writer: Arc<dyn synapse_storage::event::EventWriter>,
    pub(crate) user_storage: Arc<dyn UserStore>,
    // Reserved for future use by membership hooks; stored for constructor parity.
    pub(crate) user_service: Arc<UserService>,
    pub(crate) room_auth: Arc<dyn crate::auth::RoomAuth>,
    pub(crate) server_name: String,
    pub(crate) federation_client: Option<Arc<dyn FederationClientApi>>,
    pub(crate) key_rotation_manager: Option<Arc<KeyRotationManager>>,
    pub(crate) event_broadcaster: Option<Arc<synapse_federation::event_broadcaster::EventBroadcaster>>,
    pub(crate) room_summary_service: Arc<RoomSummaryService>,
    pub(crate) cache: Arc<CacheManager>,
    /// Optional key-rotation storage. When present, leaving a LOCAL encrypted
    /// room marks the room's megolm session for rotation (forward secrecy).
    pub(crate) key_rotation_storage: Option<Arc<dyn KeyRotationStorageApi>>,
    /// Optional application-service manager. When present, membership events
    /// (join, leave, invite, ban) are enqueued for matching application
    /// services after they are persisted.
    pub(crate) app_service_manager: Option<Arc<crate::application_service::ApplicationServiceManager>>,
    /// Optional DB pool for wrapping multi-event persistence in a single
    /// transaction (federation join state events, etc.). When `None`,
    /// each `create_event_with_graph` call uses its own implicit transaction.
    pub(crate) db_pool: Option<sqlx::PgPool>,
    /// MSC4284 — Policy server service. When present, room join/invite
    /// operations consult the policy server before persisting. `None` in
    /// test setups or when the policy server is not configured.
    pub(crate) policy_service: Option<Arc<PolicyService>>,
}

/// Configuration for constructing a [`MembershipService`].
pub struct MembershipServiceConfig {
    /// The `member_storage` field.
    pub member_storage: Arc<dyn MemberStoreApi>,
    /// The `room_storage` field.
    pub room_storage: Arc<dyn RoomStoreApi>,
    /// The `event_reader` field.
    pub event_reader: Arc<dyn synapse_storage::event::EventReader>,
    /// The `event_writer` field.
    pub event_writer: Arc<dyn synapse_storage::event::EventWriter>,
    /// The `user_storage` field.
    pub user_storage: Arc<dyn UserStore>,
    /// The `user_service` field.
    pub user_service: Arc<UserService>,
    /// The `room_auth` field.
    pub room_auth: Arc<dyn crate::auth::RoomAuth>,
    /// The `server_name` field.
    pub server_name: String,
    /// The `federation_client` field.
    pub federation_client: Option<Arc<dyn FederationClientApi>>,
    /// The `key_rotation_manager` field.
    pub key_rotation_manager: Option<Arc<KeyRotationManager>>,
    /// The `event_broadcaster` field.
    pub event_broadcaster: Option<Arc<synapse_federation::event_broadcaster::EventBroadcaster>>,
    /// The `room_summary_service` field.
    pub room_summary_service: Arc<RoomSummaryService>,
    /// The `cache` field.
    pub cache: Arc<CacheManager>,
    /// The `key_rotation_storage` field.
    pub key_rotation_storage: Option<Arc<dyn KeyRotationStorageApi>>,
    /// The `app_service_manager` field.
    pub app_service_manager: Option<Arc<crate::application_service::ApplicationServiceManager>>,
    /// Optional DB pool for wrapping multi-event persistence in a single
    /// transaction (federation join state events, etc.).
    pub db_pool: Option<sqlx::PgPool>,
    /// MSC4284 — Policy server service. `None` in test setups or when
    /// the policy server is not configured.
    pub policy_service: Option<Arc<PolicyService>>,
}

impl MembershipService {
    /// See [`new`].
    pub fn new(config: MembershipServiceConfig) -> Self {
        Self {
            member_storage: config.member_storage,
            room_storage: config.room_storage,
            event_reader: config.event_reader,
            event_writer: config.event_writer,
            user_storage: config.user_storage,
            user_service: config.user_service,
            room_auth: config.room_auth,
            server_name: config.server_name,
            federation_client: config.federation_client,
            key_rotation_manager: config.key_rotation_manager,
            event_broadcaster: config.event_broadcaster,
            room_summary_service: config.room_summary_service,
            cache: config.cache,
            key_rotation_storage: config.key_rotation_storage,
            app_service_manager: config.app_service_manager,
            db_pool: config.db_pool,
            policy_service: config.policy_service,
        }
    }

    // =========================================================================
    // MSC4284: Policy server enforcement helpers
    // =========================================================================

    /// Check policy for a room join. Returns `Ok(())` if allowed, or
    /// `Err(Forbidden)` if denied by the policy server.
    /// When no policy service is configured, always allows.
    pub(crate) async fn check_join_policy(&self, room_id: &str, user_id: &str) -> ApiResult<()> {
        let Some(policy) = &self.policy_service else {
            return Ok(());
        };
        match policy.check_room_join(room_id, user_id).await {
            crate::policy_service::PolicyResult::Allow => Ok(()),
            crate::policy_service::PolicyResult::Deny(reason) => {
                ::tracing::warn!(
                    room_id = %room_id,
                    user_id = %user_id,
                    reason = %reason,
                    "Room join denied by policy server"
                );
                Err(ApiError::forbidden(format!("Denied by policy server: {}", reason)))
            }
        }
    }

    /// Check policy for a room invite. Returns `Ok(())` if allowed, or
    /// `Err(Forbidden)` if denied by the policy server.
    /// When no policy service is configured, always allows.
    pub(crate) async fn check_invite_policy(&self, room_id: &str, inviter_id: &str, invitee_id: &str) -> ApiResult<()> {
        let Some(policy) = &self.policy_service else {
            return Ok(());
        };
        match policy.check_room_invite(room_id, inviter_id, invitee_id).await {
            crate::policy_service::PolicyResult::Allow => Ok(()),
            crate::policy_service::PolicyResult::Deny(reason) => {
                ::tracing::warn!(
                    room_id = %room_id,
                    inviter_id = %inviter_id,
                    invitee_id = %invitee_id,
                    reason = %reason,
                    "Room invite denied by policy server"
                );
                Err(ApiError::forbidden(format!("Denied by policy server: {}", reason)))
            }
        }
    }

    // =========================================================================
    // Federation helpers (used by federation_membership)
    // =========================================================================

    /// Extract the server name from a Matrix ID (`@user:server` or `!room:server`).
    pub(crate) fn server_name_from_id(id: &str) -> Option<&str> {
        id.rsplit_once(':').map(|(_, server)| server)
    }

    /// Return `true` if the given Matrix ID belongs to a remote server.
    pub(crate) fn is_remote_id(id: &str, local_server: &str) -> bool {
        Self::server_name_from_id(id).is_some_and(|srv| srv != local_server)
    }

    /// Check if a user ID belongs to a remote server (relative to this
    /// homeserver).
    pub fn is_remote_user(&self, user_id: &str) -> bool {
        Self::is_remote_id(user_id, &self.server_name)
    }

    /// Check if a room ID belongs to a remote server (relative to this
    /// homeserver).
    pub fn is_remote_room(&self, room_id: &str) -> bool {
        Self::is_remote_id(room_id, &self.server_name)
    }

    /// Get the federation client, returning an error if not configured.
    pub(crate) async fn require_federation_client(
        &self,
    ) -> ApiResult<Arc<dyn synapse_federation::client_api::FederationClientApi>> {
        self.federation_client.clone().ok_or_else(|| ApiError::internal("Federation client not configured".to_string()))
    }

    /// Get the current signing key, returning an error if not configured.
    pub(crate) async fn require_signing_key(&self) -> ApiResult<SigningKey> {
        let key_rotation_manager = self
            .key_rotation_manager
            .as_ref()
            .ok_or_else(|| ApiError::internal("Key rotation manager not configured".to_string()))?;
        key_rotation_manager
            .get_current_key()
            .await
            .map_err(|e| ApiError::internal_with_cause("Failed to get signing key", e))?
            .ok_or_else(|| ApiError::internal("No signing key available".to_string()))
    }

    /// Get state events by type — thin wrapper around `event_reader`.
    /// Returns JSON-formatted event list.
    pub(crate) async fn get_state_events_by_type(
        &self,
        room_id: &str,
        event_type: &str,
    ) -> ApiResult<Vec<serde_json::Value>> {
        let events = self
            .event_reader
            .get_state_events_by_type(room_id, event_type)
            .await
            .map_err(|e| ApiError::internal_with_cause("Failed to get state events by type", e))?;

        let event_list: Vec<serde_json::Value> = events
            .iter()
            .map(|e| {
                json!({
                    "event_id": e.event_id,
                    "sender": e.user_id,
                    "type": e.event_type,
                    "content": e.content,
                    "state_key": e.state_key
                })
            })
            .collect();

        Ok(event_list)
    }

    /// Check if the destination server is allowed by the room's server ACL
    /// policy before making an outbound federation request.
    pub(crate) async fn check_outbound_server_acl(&self, room_id: &str, destination: &str) -> ApiResult<()> {
        // Only check if the room exists locally (has state events)
        if !self.room_storage.room_exists(room_id).await? {
            return Ok(());
        }

        let acl_events = self.get_state_events_by_type(room_id, "m.room.server_acl").await?;
        let Some(acl_event) = acl_events.first() else {
            return Ok(());
        };

        let Some(acl_content) = acl_event.get("content") else {
            return Ok(());
        };

        let Some(acl) = synapse_federation::ServerAclContent::from_value(acl_content) else {
            tracing::warn!(room_id = %room_id, destination = %destination, "Failed to parse m.room.server_acl content for outbound check");
            return Ok(());
        };

        if !acl.is_server_allowed(destination) {
            return Err(ApiError::forbidden(format!(
                "Server '{}' is denied by room ACL for room '{}'",
                destination, room_id
            )));
        }

        Ok(())
    }

    /// Best-effort: enqueue a membership event for any matching application
    /// services.  Called after the event is persisted so bridges receive
    /// membership transitions (join, leave, invite, ban).
    pub(crate) async fn dispatch_appservice_event(&self, event: &RoomEvent) {
        let Some(app_service_manager) = &self.app_service_manager else {
            return;
        };

        if let Err(error) = app_service_manager
            .enqueue_matching_event(
                &event.event_id,
                &event.room_id,
                &event.event_type,
                &event.user_id,
                &event.content,
                event.state_key.as_deref(),
            )
            .await
        {
            ::tracing::warn!(
                error = %error,
                event_id = %event.event_id,
                room_id = %event.room_id,
                event_type = %event.event_type,
                "Failed to enqueue application service event for membership transition"
            );
        }
    }

    /// Resolve a target user's current membership (as the typed [`Membership`]
    /// enum) plus whether they are currently banned. `None` means the user has
    /// no membership record in the room. Used to build the `from` state for a
    /// membership-transition legality check.
    pub(crate) async fn resolve_membership_from(
        &self,
        room_id: &str,
        target_id: &str,
    ) -> ApiResult<(Option<Membership>, bool)> {
        let existing = self
            .member_storage
            .get_room_member(room_id, target_id)
            .await
            .map_err(|e| ApiError::internal_with_cause("Failed to check membership", e))?;
        let from = existing.as_ref().and_then(|m| Membership::from_str(&m.membership).ok());
        let is_banned = from == Some(Membership::Ban) || existing.as_ref().and_then(|m| m.is_banned).unwrap_or(false);
        Ok((from, is_banned))
    }

    /// Resolve the effective join rule for a room as the typed [`JoinRule`]:
    /// the `m.room.join_rules` state event wins, then the room record's
    /// `join_rule`, then a `public`/`invite` default from `is_public`. Unknown
    /// rule strings resolve to [`JoinRule::Invite`] (fail-closed).
    pub(crate) async fn resolve_join_rule(&self, room_id: &str) -> ApiResult<JoinRule> {
        Ok(self.resolve_join_rule_and_allow(room_id).await?.0)
    }

    /// Resolve both the effective [`JoinRule`] and the rooms permitted by the
    /// rule's `allow` array (MSC3083). For `restricted` / `knock_restricted`
    /// rooms the `m.room.join_rules` content carries
    /// `allow: [{ "room_id": "!space:server", "type": "m.room_membership" }, ...]`
    /// — a joiner is authorized iff they hold `join` membership in one of the
    /// listed rooms (typically a Space). For any other rule the list is empty
    /// (it is unused).
    pub(crate) async fn resolve_join_rule_and_allow(&self, room_id: &str) -> ApiResult<(JoinRule, Vec<String>)> {
        let join_rules_content = if let Some(event) = self
            .event_reader
            .get_state_events_by_type(room_id, "m.room.join_rules")
            .await
            .map_err(|e| ApiError::internal_with_cause("Failed to load room join rules", e))?
            .into_iter()
            .find(|event| event.state_key.as_deref().unwrap_or_default().is_empty())
        {
            event.content.clone()
        } else {
            serde_json::Value::Null
        };

        let effective = join_rules_content.get("join_rule").and_then(|value| value.as_str()).map(str::to_string);

        // The `allow` array is meaningful only for restricted rules; for other
        // rules the stored `join_rule` column can't carry an allow list, so an
        // empty vector is correct there.
        let allowed_rooms = extract_allowed_join_rooms(&join_rules_content);

        let room = self
            .room_storage
            .get_room(room_id)
            .await
            .map_err(|e| ApiError::internal_with_cause("Failed to load room", e))?;

        let raw = effective
            .or_else(|| room.as_ref().and_then(|r| (!r.join_rule.is_empty()).then(|| r.join_rule.clone())))
            .unwrap_or_else(|| {
                if room.as_ref().is_some_and(|r| r.is_public) {
                    "public".to_string()
                } else {
                    "invite".to_string()
                }
            });

        Ok((JoinRule::from_str(&raw).unwrap_or(JoinRule::Invite), allowed_rooms))
    }

    /// Spec-compliant authorization for restricted / knock_restricted rooms.
    /// Per MSC3083: a user may join a restricted room iff they have `join`
    /// membership in at least one of the rooms listed in the event's `allow`
    /// array (typical case: a Space).  Returns `true` when authorized,
    /// `false` for non-restricted or when no local membership is found.
    /// If the allowed space is not locally replicated, falls back to
    /// `false` (fail-closed for client joins); federation inbound path
    /// handles replication gaps separately.
    pub(crate) async fn is_restricted_join_authorized(&self, room_id: &str, user_id: &str) -> ApiResult<bool> {
        let (rule, allowed_rooms) = self.resolve_join_rule_and_allow(room_id).await?;
        if !matches!(rule, JoinRule::Restricted | JoinRule::KnockRestricted) {
            return Ok(false);
        }
        for space_id in allowed_rooms {
            let Some(member) = self
                .member_storage
                .get_room_member(&space_id, user_id)
                .await
                .map_err(|e| ApiError::internal_with_cause("Failed to check space member", e))?
            else {
                continue;
            };
            if member.membership == "join" {
                return Ok(true);
            }
        }
        Ok(false)
    }

    /// Authorize an inbound federation `m.room.member` transition against our
    /// current room state — closes AUDIT-2026-07 S5 gap 2, where inbound member
    /// events skipped the transition table the client path enforces.
    ///
    /// Deliberately narrow to avoid rejecting legitimate backfilled state:
    /// - `leave` (leave / kick / unban) is accepted idempotently.
    /// - Power-level authorization is validated via the event's auth-event
    ///   chain elsewhere, not here, so power is delegated (state-only ctx).
    /// - For joins, join-rule authorization is deferred to the resident server
    ///   that signed the join, so a permissive rule is used; only the ban
    ///   dimension is enforced locally (a banned user cannot re-join).
    /// - For knocks, the room's real join rule is enforced (the room must
    ///   actually allow knocking).
    ///
    /// Fails closed on illegal transitions (banned re-join, invite of a banned
    /// user, self-ban, already-joined re-invite, knock into a non-knock room).
    pub async fn authorize_inbound_member_transition(
        &self,
        room_id: &str,
        sender: &str,
        target: &str,
        to: Membership,
    ) -> ApiResult<()> {
        if to == Membership::Leave {
            return Ok(());
        }
        let (from, target_is_banned) = self.resolve_membership_from(room_id, target).await?;
        let join_rule = if to == Membership::Knock { self.resolve_join_rule(room_id).await? } else { JoinRule::Public };
        let ctx = TransitionCtx::state_only(join_rule, sender == target, target_is_banned, /* restricted */ true);
        is_legal(from, to, &ctx).map_err(ApiError::from)
    }

    /// Sign a locally-produced event and broadcast it to all remote servers
    /// that have joined members in the room.
    ///
    /// Best-effort: in test setups without federation config, this is a no-op.
    /// Broadcast failures are logged but not propagated.
    pub async fn sign_and_broadcast_event(&self, event: &RoomEvent) -> ApiResult<()> {
        // 0. Check if federation signing is configured.
        let Some(key_rotation_manager) = &self.key_rotation_manager else {
            return Ok(());
        };

        // 1. Fetch prev_events (forward extremities of the room).
        // BEST-EFFORT: If we cannot fetch prev_events, we log and proceed with empty.
        // This preserves the "fail-open" design for federation broadcasting, but
        // warns operators that the PDU may be missing proper prev_events.
        let prev_events = match self.event_reader.get_latest_event_ids_in_room(&event.room_id, 10).await {
            Ok(events) => events,
            Err(e) => {
                ::tracing::warn!(
                    event_id = %event.event_id,
                    error = %e,
                    "Failed to fetch prev_events for federation broadcast; PDU may be incomplete"
                );
                Vec::new()
            }
        };

        // Exclude the event itself.
        let prev_events: Vec<String> = prev_events.into_iter().filter(|id| id != &event.event_id).collect();

        // 2. Build the PDU JSON.
        let mut pdu = json!({
            "event_id": event.event_id,
            "room_id": event.room_id,
            "sender": event.user_id,
            "user_id": event.user_id,
            "type": event.event_type,
            "content": event.content,
            "origin_server_ts": event.origin_server_ts,
            "origin": self.server_name,
            "prev_events": prev_events,
        });

        if let Some(ref state_key) = event.state_key {
            pdu["state_key"] = serde_json::Value::String(state_key.clone());
        }

        if let Some(ref redacts) = event.redacts {
            pdu["redacts"] = serde_json::Value::String(redacts.clone());
        }

        // 3. Sign and hash the PDU.
        let signing_key = key_rotation_manager
            .get_current_key()
            .await
            .map_err(|e| ApiError::internal_with_cause("Failed to get signing key", e))?
            .ok_or_else(|| ApiError::internal("No signing key available".to_string()))?;

        sign_and_hash_event(&self.server_name, &signing_key.key_id, &signing_key.secret_key, &mut pdu)
            .map_err(|e| ApiError::internal(format!("Failed to sign event: {e}")))?;

        // 4. Persist signatures and hashes back to the events table.
        let signatures = pdu.get("signatures").cloned().unwrap_or(serde_json::Value::Null);
        let hashes = pdu.get("hashes").cloned().unwrap_or(serde_json::Value::Null);
        if let Err(e) =
            self.event_writer.update_event_signatures_and_hashes(&event.event_id, &signatures, &hashes).await
        {
            ::tracing::warn!(
                event_id = %event.event_id,
                room_id = %event.room_id,
                error = %e,
                "Failed to persist event signatures/hashes"
            );
        }

        // 5. Broadcast to remote servers via event_broadcaster.
        if let Some(broadcaster) = &self.event_broadcaster {
            if let Err(e) = broadcaster.broadcast_event(&event.room_id, &pdu, &self.server_name).await {
                ::tracing::warn!(
                    event_id = %event.event_id,
                    room_id = %event.room_id,
                    error = %e,
                    "Failed to broadcast event to federation peers"
                );
            }
        }

        Ok(())
    }

    // ── MSC2666: Mutual Rooms (Get rooms in common with another user) ─────

    /// Returns a paginated list of rooms where both the authenticated user
    /// and `other_user_id` have `join` membership.
    ///
    /// Per MSC2666, querying mutual rooms with yourself returns M_FORBIDDEN.
    ///
    /// # Response
    /// - `joined`: Array of room IDs both users are joined to
    /// - `next_batch_token`: pagination token (room_id of last room) for `after` param
    #[allow(clippy::too_many_arguments)]
    pub async fn get_mutual_rooms_between(
        &self,
        user_id: &str,
        other_user_id: &str,
        limit: i64,
        after: Option<&str>,
    ) -> Result<serde_json::Value, MembershipError> {
        if user_id == other_user_id {
            return Err(MembershipError::NotAuthorized(
                "You cannot query mutual rooms with yourself".to_string(),
            ));
        }

        let (rooms, next_batch_token) = self
            .member_storage
            .get_mutual_rooms_between(user_id, other_user_id, limit, after)
            .await
            .map_err(|e| MembershipError::Database(e))?;

        let mut result = json!({
            "joined": rooms,
        });

        if let Some(token) = next_batch_token {
            result["next_batch_token"] = json!(token);
        }

        Ok(result)
    }

    // ── MSC4502: Paginated room members for client endpoints.
    ///
    /// Returns a page of members with a `next_batch` cursor for
    /// continued pagination. `not_membership` excludes specified
    /// membership types (e.g., "leave").
    ///
    /// The `next_batch` value is the `user_id` of the last member in
    /// the current page; pass it as `from` in the next request.
    #[allow(clippy::too_many_arguments)]
    pub async fn get_room_members_paginated(
        &self,
        room_id: &str,
        user_id: &str,
        membership: Option<&str>,
        not_membership: Option<&str>,
        limit: i64,
        from: Option<&str>,
        dir: Option<&str>,
    ) -> Result<serde_json::Value, MembershipError> {
        if !self
            .room_storage
            .room_exists(room_id)
            .await
            .map_err(|_e| MembershipError::Internal("Failed to check room existence".to_string()))?
        {
            return Err(MembershipError::NotFound("Room not found".to_string()));
        }

        if !self
            .member_storage
            .is_member(room_id, user_id)
            .await
            .map_err(|_e| MembershipError::Internal("Failed to check membership".to_string()))?
        {
            return Err(MembershipError::NotAuthorized(
                "You are not a member of this room".to_string(),
            ));
        }

        let membership_str = membership.unwrap_or("join");
        // Fetch limit+1 to detect if there are more pages
        let members = self
            .member_storage
            .get_room_members_paginated_with_profiles(room_id, membership_str, not_membership, limit + 1, from, dir)
            .await
            .map_err(|e| MembershipError::Database(e))?;

        let has_more = members.len() as i64 > limit;
        // Truncate to requested limit
        let members: Vec<_> = members.into_iter().take(limit as usize).collect();

        let (chunk, next_batch) = if members.is_empty() {
            (Vec::new(), None)
        } else {
            let chunk: Vec<serde_json::Value> = members
                .iter()
                .map(|(m, dn, av)| {
                    let mut content = serde_json::Map::new();
                    content.insert("membership".to_string(), json!(m.membership));
                    let effective_displayname = m.display_name.as_deref().or(dn.as_deref());
                    if let Some(dn) = effective_displayname {
                        content.insert("displayname".to_string(), json!(dn));
                    }
                    let effective_avatar_url = m.avatar_url.as_deref().or(av.as_deref());
                    if let Some(au) = effective_avatar_url {
                        content.insert("avatar_url".to_string(), json!(au));
                    }
                    if let Some(reason) = &m.reason {
                        content.insert("reason".to_string(), json!(reason));
                    }
                    json!({
                        "type": "m.room.member",
                        "state_key": m.user_id,
                        "content": content,
                        "event_id": m.event_id,
                        "origin_server_ts": m.joined_ts.unwrap_or(m.updated_ts.unwrap_or(0)),
                        "room_id": m.room_id,
                        "sender": m.sender.as_deref().unwrap_or(&m.user_id),
                    })
                })
                .collect();

            // next_batch = user_id of last member for forward, first for backward
            let next = if dir.map(|d| d == "b").unwrap_or(false) {
                chunk.first().and_then(|c| c.get("state_key").and_then(|v| v.as_str()).map(String::from))
            } else {
                chunk.last().and_then(|c| c.get("state_key").and_then(|v| v.as_str()).map(String::from))
            };
            (chunk, next)
        };

        let mut result = json!({ "chunk": chunk });
        if has_more {
            if let Some(nb) = next_batch {
                result["next_batch"] = json!(nb);
            }
        }
        Ok(result)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::room::join_rules::is_valid_matrix_id;

    // ── server_name_from_id ────────────────────────────────────────

    #[test]
    fn server_name_from_user_id() {
        assert_eq!(MembershipService::server_name_from_id("@user:myserver.com"), Some("myserver.com"));
    }

    #[test]
    fn server_name_from_room_id() {
        assert_eq!(MembershipService::server_name_from_id("!room:myserver.com"), Some("myserver.com"));
    }

    #[test]
    fn server_name_from_id_no_colon() {
        assert_eq!(MembershipService::server_name_from_id("justastring"), None);
    }

    #[test]
    fn server_name_from_id_empty() {
        assert_eq!(MembershipService::server_name_from_id(""), None);
    }

    #[test]
    fn server_name_from_id_multiple_colons() {
        // rsplit_once picks the last colon
        assert_eq!(MembershipService::server_name_from_id("@user:sub:server.com"), Some("server.com"));
    }

    #[test]
    fn server_name_from_id_trailing_colon() {
        assert_eq!(MembershipService::server_name_from_id("text:"), Some(""));
    }

    #[test]
    fn server_name_from_id_leading_colon() {
        assert_eq!(MembershipService::server_name_from_id(":text"), Some("text"));
    }

    // ── is_remote_id ───────────────────────────────────────────────

    #[test]
    fn is_remote_id_true_for_other_server() {
        assert!(MembershipService::is_remote_id("@user:other.com", "myserver.com"));
    }

    #[test]
    fn is_remote_id_false_for_local_server() {
        assert!(!MembershipService::is_remote_id("@user:myserver.com", "myserver.com"));
    }

    #[test]
    fn is_remote_id_false_when_no_server_name() {
        assert!(!MembershipService::is_remote_id("no_colon", "myserver.com"));
    }

    #[test]
    fn is_remote_id_false_for_empty_id() {
        assert!(!MembershipService::is_remote_id("", "myserver.com"));
    }

    // ── extract_allowed_join_rooms / is_valid_matrix_id (MSC3083) ───────

    #[test]
    fn extract_allow_missing_or_not_array_is_empty() {
        // No `allow` key at all.
        assert!(extract_allowed_join_rooms(&json!({"join_rule": "restricted"})).is_empty());
        // `allow` present but not an array (malformed → fail-closed).
        assert!(extract_allowed_join_rooms(&json!({"allow": "not-an-array"})).is_empty());
        // `allow` null.
        assert!(extract_allowed_join_rooms(&json!({"allow": null})).is_empty());
        // Non-restricted rule with empty array.
        assert!(extract_allowed_join_rooms(&json!({"allow": []})).is_empty());
    }

    #[test]
    fn extract_allow_single_membership_entry() {
        let rooms = extract_allowed_join_rooms(&json!({
            "join_rule": "restricted",
            "allow": [
                {"room_id": "!space:example.com", "type": "m.room_membership"}
            ]
        }));
        assert_eq!(rooms, vec!["!space:example.com".to_string()]);
    }

    #[test]
    fn extract_allow_defaults_type_to_membership() {
        // `type` is omitted → per MSC3083 it defaults to m.room_membership, so
        // the room must still be accepted.
        let rooms = extract_allowed_join_rooms(&json!({
            "allow": [{"room_id": "!space:example.com"}]
        }));
        assert_eq!(rooms, vec!["!space:example.com".to_string()]);
    }

    #[test]
    fn extract_allow_ignores_non_membership_types() {
        // Only a role-based rule (no room_id) and an unknown type are dropped;
        // the single valid membership entry survives.
        let rooms = extract_allowed_join_rooms(&json!({
            "allow": [
                {"type": "m.room_membership", "room_id": "!keep:example.com"},
                {"type": "org.example.custom"},
                {"type": "m.room_role", "role": "bot"}
            ]
        }));
        assert_eq!(rooms, vec!["!keep:example.com".to_string()]);
    }

    #[test]
    fn extract_allow_dedupes_and_sorts() {
        // Duplicates collapse; the output is sorted for deterministic
        // fingerprinting upstream.
        let rooms = extract_allowed_join_rooms(&json!({
            "allow": [
                {"room_id": "!zz:example.com", "type": "m.room_membership"},
                {"room_id": "!aa:example.com", "type": "m.room_membership"},
                {"room_id": "!zz:example.com", "type": "m.room_membership"}
            ]
        }));
        assert_eq!(rooms, vec!["!aa:example.com".to_string(), "!zz:example.com".to_string()]);
    }

    #[test]
    fn extract_allow_drops_malformed_room_ids_fail_closed() {
        // Each malformed entry is silently dropped; only the valid one remains.
        let rooms = extract_allowed_join_rooms(&json!({
            "allow": [
                {"room_id": "no-sigil:example.com", "type": "m.room_membership"},
                {"room_id": "!nohost", "type": "m.room_membership"},
                {"room_id": "!empty:@", "type": "m.room_membership"},
                {"room_id": "", "type": "m.room_membership"},
                {"type": "m.room_membership"},
                {"room_id": "!good:example.com", "type": "m.room_membership"}
            ]
        }));
        assert_eq!(rooms, vec!["!good:example.com".to_string()]);
    }

    #[test]
    fn valid_matrix_id_accepts_sigil_with_server() {
        assert!(is_valid_matrix_id("!room:example.com"));
        assert!(is_valid_matrix_id("#alias:example.com"));
        // Servers may carry ports (colons) — still valid via rfind split.
        assert!(is_valid_matrix_id("!room:example.com:8448"));
    }

    #[test]
    fn valid_matrix_id_rejects_bad_shapes() {
        assert!(!is_valid_matrix_id(""));
        assert!(!is_valid_matrix_id("@user:example.com")); // user sigil not accepted here
        assert!(!is_valid_matrix_id("!onlylocal")); // no server separator
        assert!(!is_valid_matrix_id("!:example.com")); // empty localpart
        assert!(!is_valid_matrix_id("!room:")); // empty server
        assert!(!is_valid_matrix_id("!room:exa mple.com")); // whitespace in server
        assert!(!is_valid_matrix_id("!room:exa/mple.com")); // path separator in server
    }

    // ── authorize_inbound_member_transition (federation S5 gap 2) ──────

    use std::sync::Arc as StdArc;
    use synapse_cache::{CacheConfig, CacheManager};
    use synapse_storage::event::{EventReader, EventWriter};
    use synapse_storage::test_mocks::room_summary::InMemoryRoomSummaryStore;
    use synapse_storage::test_mocks::{FakeUserStore, InMemoryEventStore, InMemoryMemberStore, InMemoryRoomStore};
    use synapse_storage::{MemberStoreApi, RoomStoreApi, UserStore};

    use crate::room::summary::RoomSummaryService;
    use crate::test_mocks::FakeRoomAuth;
    use crate::user_service::UserService;

    const ROOM: &str = "!fed:localhost";

    /// Build a [`MembershipService`] over in-memory stores, seeding a public
    /// room and any given `(user, membership)` members.
    async fn inbound_service(members: &[(&str, &str)]) -> MembershipService {
        let member_store = InMemoryMemberStore::new();
        for (user, membership) in members {
            member_store.add_member(ROOM, user, membership, None).await.unwrap();
        }

        let room_store = InMemoryRoomStore::new();
        room_store.create_room(ROOM, "@creator:localhost", "public", "10", true).await.unwrap();

        let event_store = StdArc::new(InMemoryEventStore::new());
        let event_reader: StdArc<dyn EventReader> = event_store.clone();
        let event_writer: StdArc<dyn EventWriter> = event_store.clone();
        let member_storage: StdArc<dyn MemberStoreApi> = StdArc::new(member_store);
        let room_storage: StdArc<dyn RoomStoreApi> = StdArc::new(room_store);
        let user_storage: StdArc<dyn UserStore> = StdArc::new(FakeUserStore::new());
        let user_service = StdArc::new(UserService::new(user_storage.clone()));

        let room_summary_service = StdArc::new(RoomSummaryService::new(
            StdArc::new(InMemoryRoomSummaryStore::new()),
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
            room_auth: StdArc::new(FakeRoomAuth::new()),
            server_name: "localhost".to_string(),
            federation_client: None,
            key_rotation_manager: None,
            event_broadcaster: None,
            room_summary_service,
            cache: StdArc::new(CacheManager::new(&CacheConfig::default())),
            key_rotation_storage: None,
            app_service_manager: None,
            db_pool: None,
            policy_service: None,
        })
    }

    #[tokio::test]
    async fn inbound_clean_join_is_allowed() {
        let svc = inbound_service(&[]).await;
        let r = svc.authorize_inbound_member_transition(ROOM, "@bob:remote", "@bob:remote", Membership::Join).await;
        assert!(r.is_ok(), "clean join should be allowed: {r:?}");
    }

    #[tokio::test]
    async fn inbound_banned_user_rejoin_is_rejected() {
        let svc = inbound_service(&[("@bob:remote", "ban")]).await;
        let r = svc.authorize_inbound_member_transition(ROOM, "@bob:remote", "@bob:remote", Membership::Join).await;
        assert!(r.is_err(), "banned user re-join must be rejected");
    }

    #[tokio::test]
    async fn inbound_invite_of_banned_user_is_rejected() {
        let svc = inbound_service(&[("@bob:remote", "ban")]).await;
        let r = svc.authorize_inbound_member_transition(ROOM, "@admin:remote", "@bob:remote", Membership::Invite).await;
        assert!(r.is_err(), "inviting a banned user must be rejected");
    }

    #[tokio::test]
    async fn inbound_self_ban_is_rejected() {
        let svc = inbound_service(&[("@bob:remote", "join")]).await;
        let r = svc.authorize_inbound_member_transition(ROOM, "@bob:remote", "@bob:remote", Membership::Ban).await;
        assert!(r.is_err(), "self-ban must be rejected");
    }

    #[tokio::test]
    async fn inbound_leave_is_always_accepted() {
        let svc = inbound_service(&[]).await;
        // Even for a user with no local membership record, leave is idempotent.
        let r =
            svc.authorize_inbound_member_transition(ROOM, "@ghost:remote", "@ghost:remote", Membership::Leave).await;
        assert!(r.is_ok(), "leave should be accepted idempotently: {r:?}");
    }

    // ── MSC2666: Mutual Rooms (get_mutual_rooms_between) ─────────

    /// Build a [`MembershipService`] seeding members across arbitrary rooms.
    /// Each entry is `(room_id, user_id, membership)`; rooms are created on
    /// first reference.
    async fn mutual_service(members: &[(&str, &str, &str)]) -> MembershipService {
        let member_store = InMemoryMemberStore::new();
        let room_store = InMemoryRoomStore::new();
        let mut seen: std::collections::HashSet<&str> = std::collections::HashSet::new();
        for (room, user, membership) in members {
            if seen.insert(*room) {
                room_store.create_room(room, "@creator:localhost", "public", "10", true).await.unwrap();
            }
            member_store.add_member(room, user, membership, None).await.unwrap();
        }

        let event_store = StdArc::new(InMemoryEventStore::new());
        let event_reader: StdArc<dyn EventReader> = event_store.clone();
        let event_writer: StdArc<dyn EventWriter> = event_store.clone();
        let member_storage: StdArc<dyn MemberStoreApi> = StdArc::new(member_store);
        let room_storage: StdArc<dyn RoomStoreApi> = StdArc::new(room_store);
        let user_storage: StdArc<dyn UserStore> = StdArc::new(FakeUserStore::new());
        let user_service = StdArc::new(UserService::new(user_storage.clone()));
        let room_summary_service = StdArc::new(RoomSummaryService::new(
            StdArc::new(InMemoryRoomSummaryStore::new()),
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
            room_auth: StdArc::new(FakeRoomAuth::new()),
            server_name: "localhost".to_string(),
            federation_client: None,
            key_rotation_manager: None,
            event_broadcaster: None,
            room_summary_service,
            cache: StdArc::new(CacheManager::new(&CacheConfig::default())),
            key_rotation_storage: None,
            app_service_manager: None,
            db_pool: None,
            policy_service: None,
        })
    }

    #[tokio::test]
    async fn mutual_rooms_returns_common_joined_rooms() {
        let svc = mutual_service(&[
            ("!a:localhost", "@alice:localhost", "join"),
            ("!a:localhost", "@bob:localhost", "join"),
            ("!b:localhost", "@alice:localhost", "join"),
            ("!b:localhost", "@bob:localhost", "join"),
            ("!c:localhost", "@alice:localhost", "join"),
            // bob not in !c → not mutual
        ])
        .await;
        let result = svc.get_mutual_rooms_between("@alice:localhost", "@bob:localhost", 100, None).await.unwrap();
        let joined: Vec<&str> = result["joined"].as_array().unwrap().iter().map(|v| v.as_str().unwrap()).collect();
        assert_eq!(joined, vec!["!a:localhost", "!b:localhost"], "only !a and !b are mutual, sorted");
        assert!(result.get("next_batch_token").is_none(), "no token when under limit");
    }

    #[tokio::test]
    async fn mutual_rooms_self_query_returns_forbidden() {
        let svc = mutual_service(&[("!a:localhost", "@alice:localhost", "join")]).await;
        let r = svc.get_mutual_rooms_between("@alice:localhost", "@alice:localhost", 100, None).await;
        assert!(r.is_err());
        assert_eq!(r.unwrap_err().code(), &synapse_common::MatrixErrorCode::Forbidden);
    }

    #[tokio::test]
    async fn mutual_rooms_empty_when_no_common() {
        let svc =
            mutual_service(&[("!a:localhost", "@alice:localhost", "join"), ("!b:localhost", "@bob:localhost", "join")])
                .await;
        let result = svc.get_mutual_rooms_between("@alice:localhost", "@bob:localhost", 100, None).await.unwrap();
        assert!(result["joined"].as_array().unwrap().is_empty());
    }

    #[tokio::test]
    async fn mutual_rooms_excludes_non_join_membership() {
        // bob is only invited to !a (not joined) → !a not mutual
        let svc = mutual_service(&[
            ("!a:localhost", "@alice:localhost", "join"),
            ("!a:localhost", "@bob:localhost", "invite"),
        ])
        .await;
        let result = svc.get_mutual_rooms_between("@alice:localhost", "@bob:localhost", 100, None).await.unwrap();
        assert!(result["joined"].as_array().unwrap().is_empty(), "invite-only room must be excluded");
    }

    #[tokio::test]
    async fn mutual_rooms_pagination_emits_next_batch_token() {
        let svc = mutual_service(&[
            ("!a:localhost", "@alice:localhost", "join"),
            ("!a:localhost", "@bob:localhost", "join"),
            ("!b:localhost", "@alice:localhost", "join"),
            ("!b:localhost", "@bob:localhost", "join"),
            ("!c:localhost", "@alice:localhost", "join"),
            ("!c:localhost", "@bob:localhost", "join"),
        ])
        .await;
        // limit=2 → first page + token
        let page1 = svc.get_mutual_rooms_between("@alice:localhost", "@bob:localhost", 2, None).await.unwrap();
        assert_eq!(page1["joined"].as_array().unwrap().len(), 2);
        let token = page1["next_batch_token"].as_str().unwrap().to_string();
        assert_eq!(token, "!b:localhost", "token is last room of first page");

        // second page using token
        let page2 = svc.get_mutual_rooms_between("@alice:localhost", "@bob:localhost", 2, Some(&token)).await.unwrap();
        let joined2: Vec<&str> = page2["joined"].as_array().unwrap().iter().map(|v| v.as_str().unwrap()).collect();
        assert_eq!(joined2, vec!["!c:localhost"]);
        assert!(page2.get("next_batch_token").is_none(), "last page has no token");
    }
}
