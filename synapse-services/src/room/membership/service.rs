//! Domain service for room membership operations — join, leave, invite,
//! kick, ban, unban, knock, forget, and federation membership.
//!
//! Extracted from RoomService as part of the domain split plan (Task 1).

use crate::common::error::{ApiError, ApiResult};
use crate::policy_service::PolicyService;
use crate::room::membership::error::MembershipError;
use serde_json::json;
use std::str::FromStr;
use std::sync::Arc;
use synapse_cache::CacheManager;
use synapse_common::{is_legal, JoinRule, Membership, TransitionCtx};
use synapse_federation::client_api::FederationClientApi;
use synapse_federation::key_rotation::SigningKey;
use synapse_federation::KeyRotationManager;
use synapse_storage::event::RoomEvent;
use synapse_storage::{MemberStoreApi, RoomStoreApi, UserStore};

use synapse_e2ee::key_rotation::KeyRotationStorageApi;

use crate::room::state_record::ResolutionCache;
use crate::room::summary::RoomSummaryService;

// MSC3083 `allow`-array parsing now lives in the single canonical
// `room::join_rules` module so the authorization gate and the `/summary`
// projection cannot drift apart again (see its module docs, and
// `docs/audit/AUDIT_SUMMARY_2026-09-12.md` §3). Re-exported here to keep the
// existing intra-crate call sites and `super::*` test imports unchanged.
pub(crate) use crate::room::join_rules::extract_allowed_join_rooms;

/// Where a room is hosted, as decided by [`MembershipService::room_locality`]
/// from the room's ownership records — never from the room id's spelling.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RoomLocality {
    /// This homeserver hosts the room: the `m.room.create` event that founded
    /// it originated here (or the locally recorded creator is local).
    Local,
    /// Another homeserver hosts the room. `destinations` are the resident
    /// servers to contact, best candidate first. It is empty when this server
    /// holds no ownership record and knows of no joined resident — callers must
    /// fail in that case, never fall back to the local path.
    Remote {
        /// The resident servers to contact, best candidate first.
        destinations: Vec<String>,
    },
}

/// Domain service for room membership operations — join, leave, invite,
/// kick, ban, unban, knock, forget, and federation membership.
#[derive(Clone)]
pub struct MembershipService {
    pub(crate) member_storage: Arc<dyn MemberStoreApi>,
    pub(crate) room_storage: Arc<dyn RoomStoreApi>,
    pub(crate) event_reader: Arc<dyn synapse_storage::event::EventReader>,
    pub(crate) event_writer: Arc<dyn synapse_storage::event::EventWriter>,
    pub(crate) user_storage: Arc<dyn UserStore>,
    pub(crate) room_auth: Arc<dyn crate::auth::RoomAuth>,
    pub(crate) server_name: String,
    pub(crate) federation_client: Option<Arc<dyn FederationClientApi>>,
    pub(crate) key_rotation_manager: Option<Arc<KeyRotationManager>>,
    pub(crate) event_broadcaster: Option<Arc<synapse_federation::event_broadcaster::EventBroadcaster>>,
    pub(crate) room_summary_service: Arc<RoomSummaryService>,
    pub(crate) cache: Arc<CacheManager>,
    /// M-5: the process-shared state-resolution-result cache. Cloned from the
    /// `RoomService` so this service and
    /// [`MessagingService`](crate::room::messaging::service::MessagingService)
    /// reuse each other's resolutions across requests. See [`ResolutionCache`].
    pub(crate) resolution_cache: ResolutionCache,
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
    /// The invite policy gate: the room's blocklist/allowlist plus the
    /// invitee's own MSC4155 account policy. Required, not optional — every
    /// invite entry point routes through [`Self::authorize_invite_policy`],
    /// and a gate that can be absent is a gate that can be skipped.
    pub(crate) invite_policy_gate: Arc<dyn crate::invite_blocklist_service::InvitePolicyGate>,
    /// The third-party event admission gate (Synapse `check_event_allowed`).
    /// Required, not optional — every membership event write consults it via
    /// [`Self::admit_membership_event`] *before* mutating membership state, so
    /// a refusal leaves no "member but no event" residue. A gate that can be
    /// absent is a gate that can be skipped.
    pub(crate) event_admission_gate: Arc<dyn crate::module_service::EventAdmissionGate>,
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
    /// The `resolution_cache` field.
    pub resolution_cache: ResolutionCache,
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
    /// The invite policy gate. Required — see the field docs on
    /// [`MembershipService`].
    pub invite_policy_gate: Arc<dyn crate::invite_blocklist_service::InvitePolicyGate>,
    /// The third-party event admission gate. Required — see the field docs on
    /// [`MembershipService`].
    pub event_admission_gate: Arc<dyn crate::module_service::EventAdmissionGate>,
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
            room_auth: config.room_auth,
            server_name: config.server_name,
            federation_client: config.federation_client,
            key_rotation_manager: config.key_rotation_manager,
            event_broadcaster: config.event_broadcaster,
            room_summary_service: config.room_summary_service,
            cache: config.cache,
            resolution_cache: config.resolution_cache,
            key_rotation_storage: config.key_rotation_storage,
            app_service_manager: config.app_service_manager,
            db_pool: config.db_pool,
            policy_service: config.policy_service,
            invite_policy_gate: config.invite_policy_gate,
            event_admission_gate: config.event_admission_gate,
        }
    }

    /// Consult the third-party event admission gate for a prospective
    /// membership event, **before** any membership state is mutated.
    ///
    /// Returns the (possibly rule-rewritten) content. Callers must invoke this
    /// ahead of `add_member`/`remove_member`/`ban_member`/`unban_member` so a
    /// refusal returns `403` with zero state residue; `allow_modification`
    /// follows [`crate::module_service::consult_event_admission`].
    pub(crate) async fn admit_membership_event(
        &self,
        room_id: &str,
        event_id: &str,
        sender: &str,
        target_user_id: &str,
        content: serde_json::Value,
        allow_modification: bool,
    ) -> ApiResult<serde_json::Value> {
        let mut params = synapse_storage::CreateEventParams {
            event_id: event_id.to_string(),
            room_id: room_id.to_string(),
            user_id: sender.to_string(),
            event_type: "m.room.member".to_string(),
            content,
            state_key: Some(target_user_id.to_string()),
            origin_server_ts: synapse_common::current_timestamp_millis(),
            redacts: None,
        };
        crate::module_service::consult_event_admission(
            self.event_admission_gate.as_ref(),
            self.event_reader.as_ref(),
            &mut params,
            allow_modification,
        )
        .await?;
        Ok(params.content)
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

    /// The single invite-policy enforcement point.
    ///
    /// Every invite entry point calls this and nothing else — the client
    /// `/invite` handler (via [`Self::invite_user`]), the federation
    /// `/invite` endpoints, and inbound federation `m.room.member` PDUs with
    /// `membership: invite` (via [`Self::authorize_inbound_member_transition`]).
    /// Two gates, in order:
    ///
    /// 1. [`crate::invite_blocklist_service::InvitePolicyGate`] — the room's
    ///    blocklist/allowlist and the invitee's own MSC4155 account policy;
    /// 2. [`Self::check_invite_policy`] — the MSC4284 policy server, when one
    ///    is configured.
    ///
    /// Fails closed: a storage error in gate 1 is an error, not an allow.
    pub async fn authorize_invite_policy(&self, room_id: &str, inviter_id: &str, invitee_id: &str) -> ApiResult<()> {
        self.invite_policy_gate.check_invite_allowed(room_id, inviter_id, invitee_id).await?;
        self.check_invite_policy(room_id, inviter_id, invitee_id).await
    }

    // =========================================================================
    // Federation helpers (used by federation_membership)
    // =========================================================================

    /// Materialise the local `rooms` row for a room this server knows of only
    /// through a federated invite.
    ///
    /// A federated invite is how a homeserver *first learns* that a room
    /// exists, so it holds no `rooms` row yet. `events.room_id` and
    /// `room_memberships.room_id` are both foreign keys onto `rooms(room_id)`,
    /// so the invite PDU cannot be persisted until the row exists — without this
    /// the very first cross-server invite into any room fails with a foreign-key
    /// violation.
    ///
    /// Mirrors the remote-join path ([`super::federation`]), which creates the
    /// same minimal record after `send_join`. `creator` is the inviting (remote)
    /// user; `rooms.creator` carries no foreign key, so a remote id is fine.
    /// Idempotent: a no-op when the room is already known.
    pub async fn ensure_room_record_for_remote_invite(
        &self,
        room_id: &str,
        room_version: &str,
        creator: &str,
        join_rule: &str,
    ) -> ApiResult<()> {
        let exists = self
            .room_storage
            .room_exists(room_id)
            .await
            .map_err(|e| ApiError::internal_with_cause("Failed to check room existence", e))?;
        if exists {
            return Ok(());
        }

        let is_public = join_rule == "public";
        self.room_storage
            .create_room(room_id, creator, join_rule, room_version, is_public)
            .await
            .map_err(|e| ApiError::internal_with_cause("Failed to create local record for remotely invited room", e))?;

        ::tracing::info!(
            room_id = %room_id,
            room_version = %room_version,
            join_rule = %join_rule,
            "Created local record for remotely invited room"
        );
        Ok(())
    }

    /// Extract the server name from a **user** id (`@localpart:server`).
    ///
    /// User ids always carry a `:server`, so parsing them is sound. Room ids do
    /// **not**: a room v12 (MSC4291) id is `!` + 43 URL-safe base64 characters
    /// with no server part at all, and even a legacy room id's embedded server
    /// is not necessarily the server that hosts the room. Locality is therefore
    /// decided from the room's ownership records ([`Self::room_locality`]), and
    /// this helper is deliberately named for user ids only (G-21).
    pub(crate) fn user_server_name(user_id: &str) -> Option<&str> {
        user_id.rsplit_once(':').map(|(_, server)| server)
    }

    /// Return `true` if the given **user** id belongs to a remote server.
    pub(crate) fn is_remote_user_id(user_id: &str, local_server: &str) -> bool {
        Self::user_server_name(user_id).is_some_and(|srv| srv != local_server)
    }

    /// Check if a user ID belongs to a remote server (relative to this
    /// homeserver).
    pub fn is_remote_user(&self, user_id: &str) -> bool {
        Self::is_remote_user_id(user_id, &self.server_name)
    }

    /// The server that hosts `room_id`, or `None` when this homeserver holds no
    /// ownership record for the room.
    ///
    /// The room's **own records** are the only sound source (the id's spelling
    /// carries no server for a v12 room, and a legacy id's domain is not
    /// necessarily where the room lives):
    ///
    /// 1. the `m.room.create` event — its sender's server is the room's origin
    ///    server for every room version (`content.creator` for v1–v10, the event
    ///    sender from v11 on, which is also how `AuthService::resolve_room_creator`
    ///    reads it);
    /// 2. only if no create event is stored, the `rooms` row's recorded creator
    ///    (`creator_user_id` — a user id, so it always names a server).
    ///
    /// The `rooms` row is a fallback, never the primary signal: for a room this
    /// server joined over federation its `creator` column records the *local*
    /// joining user, not the room's real creator (see
    /// `join_room_via_federation`), so creator alone would call every
    /// federated room local.
    async fn room_origin_server(&self, room_id: &str) -> ApiResult<Option<String>> {
        let create_events = self
            .event_reader
            .get_state_events_by_type(room_id, "m.room.create")
            .await
            .map_err(|e| ApiError::internal_with_cause("Failed to read the room's create event", e))?;
        if let Some(event) = create_events.first() {
            // v1–v10 carry the creator in `content.creator`; v11+ removed it and
            // the create event's sender is the creator (same order as
            // `AuthService::resolve_room_creator`).
            let creator = event.content.get("creator").and_then(|c| c.as_str()).map(str::to_string).or_else(|| {
                if event.sender.is_empty() {
                    event.user_id.clone()
                } else {
                    Some(event.sender.clone())
                }
            });
            if let Some(server) = creator.as_deref().and_then(Self::user_server_name) {
                return Ok(Some(server.to_string()));
            }
        }

        let room = self
            .room_storage
            .get_room(room_id)
            .await
            .map_err(|e| ApiError::internal_with_cause("Failed to load the room record", e))?;
        Ok(room.and_then(|r| r.creator_user_id).and_then(|c| Self::user_server_name(&c).map(str::to_string)))
    }

    /// The resident servers a remote room can be reached through: the room's
    /// origin server first (it is a resident by construction), then every other
    /// server with a joined member as known locally. Order is preserved and
    /// duplicates (and this server) are removed.
    async fn resident_servers(&self, room_id: &str, origin: Option<String>) -> ApiResult<Vec<String>> {
        let mut destinations: Vec<String> = origin.into_iter().collect();
        let joined = self
            .member_storage
            .get_joined_servers_in_room(room_id, &self.server_name)
            .await
            .map_err(|e| ApiError::internal_with_cause("Failed to list the room's joined servers", e))?;
        for server in joined {
            if server != self.server_name && !destinations.contains(&server) {
                destinations.push(server);
            }
        }
        Ok(destinations)
    }

    /// Where `room_id` is hosted, decided from the room's ownership records —
    /// never from the spelling of its id (G-21).
    ///
    /// **Fail-closed**: a room this server has no ownership record for, or whose
    /// creator is unrecorded, is *remote*. The local leave path rewrites this
    /// server's membership record for the room, so it must only ever run for a
    /// room we host; a room we cannot prove we host is treated as remote
    /// instead. Remote calls carry the resident servers to contact, best
    /// candidate first — the list may be empty, and then the caller must fail
    /// rather than fall back to the local path.
    pub async fn room_locality(&self, room_id: &str) -> ApiResult<RoomLocality> {
        let Some(origin) = self.room_origin_server(room_id).await? else {
            return Ok(RoomLocality::Remote { destinations: self.resident_servers(room_id, None).await? });
        };
        if origin == self.server_name {
            return Ok(RoomLocality::Local);
        }
        Ok(RoomLocality::Remote { destinations: self.resident_servers(room_id, Some(origin)).await? })
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
        is_legal(from, to, &ctx).map_err(ApiError::from)?;

        // An invite arriving over federation is the same invite as one issued
        // locally, so it goes through the same gate. Checked after the
        // transition table so a locally-illegal invite does not cost a query.
        if to == Membership::Invite {
            self.authorize_invite_policy(room_id, sender, target).await?;
        }

        Ok(())
    }

    /// Sign a locally-produced event and broadcast it to all remote servers
    /// that have joined members in the room.
    ///
    /// Thin adapter over [`crate::room::federation_broadcast`] (the single
    /// implementation). This copy used to fail *open* — broadcasting with
    /// `prev_events: []` when the room's extremities could not be read — which
    /// the PDU projector documents as corrupting a peer's room graph.
    ///
    /// Best-effort: in test setups without federation config, this is a no-op.
    /// Broadcast failures are logged but not propagated.
    pub async fn sign_and_broadcast_event(&self, event: &RoomEvent) -> ApiResult<()> {
        let ctx = crate::room::federation_broadcast::BroadcastContext {
            server_name: self.server_name.clone(),
            event_reader: self.event_reader.clone(),
            event_writer: self.event_writer.clone(),
            key_rotation_manager: self.key_rotation_manager.clone(),
            event_broadcaster: self.event_broadcaster.clone(),
            room_storage: self.room_storage.clone(),
        };
        crate::room::federation_broadcast::sign_and_broadcast_event(&ctx, event).await
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
            return Err(MembershipError::NotAuthorized("You cannot query mutual rooms with yourself".to_string()));
        }

        let (rooms, next_batch_token) = self
            .member_storage
            .get_mutual_rooms_between(user_id, other_user_id, limit, after)
            .await
            .map_err(MembershipError::Database)?;

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
            return Err(MembershipError::NotAuthorized("You are not a member of this room".to_string()));
        }

        let membership_str = membership.unwrap_or("join");
        // Fetch limit+1 to detect if there are more pages
        let members = self
            .member_storage
            .get_room_members_paginated_with_profiles(room_id, membership_str, not_membership, limit + 1, from, dir)
            .await
            .map_err(MembershipError::Database)?;

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

    // ── user_server_name ───────────────────────────────────────────

    #[test]
    fn user_server_name_from_user_id() {
        assert_eq!(MembershipService::user_server_name("@user:myserver.com"), Some("myserver.com"));
    }

    /// A room v12 / MSC4291 id is `!` + 43 URL-safe base64 characters and has
    /// no `:server` at all, so no string parse can name a server for it. That is
    /// exactly why locality must come from the room's ownership records (G-21).
    #[test]
    fn domainless_room_id_has_no_parseable_server() {
        assert_eq!(MembershipService::user_server_name("!AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA"), None);
    }

    #[test]
    fn user_server_name_no_colon() {
        assert_eq!(MembershipService::user_server_name("justastring"), None);
    }

    #[test]
    fn user_server_name_empty() {
        assert_eq!(MembershipService::user_server_name(""), None);
    }

    #[test]
    fn user_server_name_multiple_colons() {
        // rsplit_once picks the last colon
        assert_eq!(MembershipService::user_server_name("@user:sub:server.com"), Some("server.com"));
    }

    #[test]
    fn user_server_name_trailing_colon() {
        assert_eq!(MembershipService::user_server_name("text:"), Some(""));
    }

    #[test]
    fn user_server_name_leading_colon() {
        assert_eq!(MembershipService::user_server_name(":text"), Some("text"));
    }

    // ── is_remote_user_id ──────────────────────────────────────────

    #[test]
    fn is_remote_user_id_true_for_other_server() {
        assert!(MembershipService::is_remote_user_id("@user:other.com", "myserver.com"));
    }

    #[test]
    fn is_remote_user_id_false_for_local_server() {
        assert!(!MembershipService::is_remote_user_id("@user:myserver.com", "myserver.com"));
    }

    #[test]
    fn is_remote_user_id_false_when_no_server_name() {
        assert!(!MembershipService::is_remote_user_id("no_colon", "myserver.com"));
    }

    #[test]
    fn is_remote_user_id_false_for_empty_id() {
        assert!(!MembershipService::is_remote_user_id("", "myserver.com"));
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
            room_auth: StdArc::new(FakeRoomAuth::new()),
            server_name: "localhost".to_string(),
            federation_client: None,
            key_rotation_manager: None,
            event_broadcaster: None,
            room_summary_service,
            cache: StdArc::new(CacheManager::new(&CacheConfig::default())),
            resolution_cache: crate::room::state_record::ResolutionCache::default(),
            key_rotation_storage: None,
            app_service_manager: None,
            db_pool: None,
            policy_service: None,
            invite_policy_gate: StdArc::new(crate::test_mocks::FakeInvitePolicyGate::new()),
            event_admission_gate: StdArc::new(crate::test_mocks::FakeEventAdmissionGate::new()),
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

    #[tokio::test]
    async fn inbound_invite_denied_by_policy_gate_is_rejected() {
        let mut svc = inbound_service(&[]).await;
        svc.invite_policy_gate = StdArc::new(crate::test_mocks::FakeInvitePolicyGate::denying());
        let r = svc.authorize_inbound_member_transition(ROOM, "@admin:remote", "@bob:remote", Membership::Invite).await;
        assert!(r.is_err(), "an invite refused by the policy gate must be rejected, got: {r:?}");
    }

    /// The gate is scoped to invites. A join arriving over federation must not
    /// be filtered by the invite lists, or a room that blocks invites would
    /// also stop its own members from joining.
    #[tokio::test]
    async fn inbound_join_is_not_gated_by_invite_policy() {
        let mut svc = inbound_service(&[]).await;
        svc.invite_policy_gate = StdArc::new(crate::test_mocks::FakeInvitePolicyGate::denying());
        let r = svc.authorize_inbound_member_transition(ROOM, "@bob:remote", "@bob:remote", Membership::Join).await;
        assert!(r.is_ok(), "a join is not an invite and must not be gated by invite policy: {r:?}");
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
            room_auth: StdArc::new(FakeRoomAuth::new()),
            server_name: "localhost".to_string(),
            federation_client: None,
            key_rotation_manager: None,
            event_broadcaster: None,
            room_summary_service,
            cache: StdArc::new(CacheManager::new(&CacheConfig::default())),
            resolution_cache: crate::room::state_record::ResolutionCache::default(),
            key_rotation_storage: None,
            app_service_manager: None,
            db_pool: None,
            policy_service: None,
            invite_policy_gate: StdArc::new(crate::test_mocks::FakeInvitePolicyGate::new()),
            event_admission_gate: StdArc::new(crate::test_mocks::FakeEventAdmissionGate::new()),
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
        let api_err: synapse_common::ApiError = r.unwrap_err().into();
        assert_eq!(api_err.code, synapse_common::MatrixErrorCode::Forbidden);
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
