use crate::auth::RoomAuth;
use crate::common::error::{ApiError, ApiResult};
use crate::policy_service::PolicyService;
use crate::*;
use futures::stream;
use futures::StreamExt;
use serde_json::{json, Value};
use std::collections::HashMap;
use std::sync::Arc;
use synapse_cache::CacheManager;
use synapse_common::current_timestamp_millis;
use synapse_common::generate_event_id;
use synapse_common::task_queue::RedisTaskQueue;
use synapse_common::validation::Validator;
use synapse_storage::room_tag::RoomTagStoreApi;
use synapse_storage::{MemberStoreApi, RoomStoreApi, StateEvent, UserStore};
use tokio::sync::RwLock;

use super::infrastructure::RoomInfrastructure;
use super::lifecycle::service::{LifecycleService, LifecycleServiceConfig};
use super::membership::service::{MembershipService, MembershipServiceConfig};
use super::messaging::service::{MessagingService, MessagingServiceConfig};
use super::state::service::{RoomStateService, RoomStateServiceConfig};
pub use synapse_storage::room::{
    decode_room_search_cursor, encode_room_search_cursor, RoomSearchCursor, RoomSearchOrder,
};
pub use synapse_storage::room_tag::RoomTag;
pub use synapse_storage::sticky_event::StickyEvent;

/// The `CreateRoomConfig` struct.
#[derive(Debug, Default, Clone)]
pub struct CreateRoomConfig {
    /// The `visibility` field.
    pub visibility: Option<String>,
    /// The `room_alias_name` field.
    pub room_alias_name: Option<String>,
    /// The `name` field.
    pub name: Option<String>,
    /// The `topic` field.
    pub topic: Option<String>,
    /// The `invite_list` field.
    pub invite_list: Option<Vec<String>>,
    /// MSC4491: per-invitee reasons keyed by user_id. Populated when createRoom
    /// `invite` entries are objects with a `reason` field. Included in the
    /// `m.room.member` invite event content.
    pub invite_reasons: Option<std::collections::HashMap<String, String>>,
    /// The `preset` field.
    pub preset: Option<String>,
    /// The `encryption` field.
    pub encryption: Option<String>,
    /// The `history_visibility` field.
    pub history_visibility: Option<String>,
    /// The `is_direct` field.
    pub is_direct: Option<bool>,
    /// The `room_type` field.
    pub room_type: Option<String>,
    /// Per Matrix C-S spec, additional state events the client wants applied
    /// after the standard set (m.room.create, m.room.member, power_levels,
    /// join_rules, history_visibility, guest_access, name, topic). Element
    /// uses this to install `m.room.encryption` for DMs.
    pub initial_state: Option<Vec<serde_json::Value>>,
    /// Extra top-level fields for the m.room.create event (e.g. `type`,
    /// `predecessor`). Merged into the create content.
    pub creation_content: Option<serde_json::Value>,
    /// Room version to record on m.room.create. Defaults to the server's
    /// `synapse_common::room_versions::DEFAULT_ROOM_VERSION` when None.
    pub room_version: Option<String>,
    /// Power level overrides applied on top of the spec defaults.
    pub power_level_content_override: Option<serde_json::Value>,
}

/// The `RoomServiceConfig` struct.
pub struct RoomServiceConfig {
    /// The `room_storage` field.
    pub room_storage: Arc<dyn RoomStoreApi>,
    /// The `member_storage` field.
    pub member_storage: Arc<dyn MemberStoreApi>,
    /// The `event_reader` field.
    pub event_reader: Option<Arc<dyn synapse_storage::event::EventReader>>,
    /// The `event_writer` field.
    pub event_writer: Option<Arc<dyn synapse_storage::event::EventWriter>>,
    /// The `room_tag_storage` field.
    pub room_tag_storage: Arc<dyn RoomTagStoreApi>,
    /// The `user_storage` field.
    pub user_storage: Arc<dyn UserStore>,
    /// The `user_service` field.
    pub user_service: Arc<UserService>,
    /// The `room_auth` field.
    pub room_auth: Arc<dyn RoomAuth>,
    /// The `room_summary_service` field.
    pub room_summary_service: Arc<RoomSummaryService>,
    /// The `validator` field.
    pub validator: Arc<Validator>,
    /// The `server_name` field.
    pub server_name: String,
    /// The `task_queue` field.
    pub task_queue: Option<Arc<RedisTaskQueue>>,
    /// The `relations_storage` field.
    pub relations_storage: Arc<dyn synapse_storage::relations::RelationsStoreApi>,
    /// The `event_broadcaster` field.
    pub event_broadcaster: Option<Arc<synapse_federation::event_broadcaster::EventBroadcaster>>,
    /// The `app_service_manager` field.
    pub app_service_manager: Option<Arc<crate::application_service::ApplicationServiceManager>>,
    /// Server signing key manager, used to sign locally-produced PDUs before
    /// federating them.  `None` in test setups that don't exercise federation.
    pub key_rotation_manager: Option<Arc<synapse_federation::KeyRotationManager>>,
    /// Outbound federation client, used for make_join/send_join/make_leave/
    /// send_leave/invite flows.  `None` in test setups.
    pub federation_client: Option<Arc<dyn synapse_federation::client_api::FederationClientApi>>,
    #[cfg(feature = "beacons")]
    /// The `beacon_service` field.
    pub beacon_service: Option<Arc<crate::beacon_service::BeaconService>>,
    #[cfg(not(feature = "beacons"))]
    /// The `beacon_service` field.
    pub beacon_service: Option<()>,
    /// The `sticky_event_storage` field.
    pub sticky_event_storage: Arc<synapse_storage::sticky_event::StickyEventStorage>,
    /// The `cache` field.
    pub cache: Arc<CacheManager>,
    /// Optional key-rotation storage injected into the membership sub-service so
    /// that leaving a LOCAL encrypted room marks the megolm session for
    /// rotation (forward secrecy). `None` in test setups.
    pub key_rotation_storage: Option<Arc<dyn synapse_e2ee::key_rotation::KeyRotationStorageApi>>,
    /// Optional DB pool for wrapping multi-event persistence (federation join)
    /// in a single transaction instead of N+1 implicit transactions.
    pub db_pool: Option<sqlx::PgPool>,
    /// MSC4284 — Policy server service for room/user/content moderation.
    /// `None` in test setups or when the policy server is not configured.
    pub policy_service: Option<Arc<PolicyService>>,
}

/// The `RoomService` struct.
#[allow(dead_code)] // Reserved fields for future use; see field-level comments.
pub struct RoomService {
    /// Domain sub-service: membership operations (join, leave, invite, etc.)
    pub membership: MembershipService,
    /// Domain sub-service: messaging operations (events, messages, receipts, etc.)
    pub messaging: MessagingService,
    /// Domain sub-service: room state operations (aliases, tags, info, directory, etc.)
    pub state: RoomStateService,
    /// Domain sub-service: room lifecycle operations (create, upgrade, migration)
    pub lifecycle: LifecycleService,
    pub(crate) room_storage: Arc<dyn RoomStoreApi>,
    pub(crate) member_storage: Arc<dyn MemberStoreApi>,
    /// The `user_storage` field.
    pub user_storage: Arc<dyn UserStore>,
    /// The `validator` field.
    pub validator: Arc<Validator>,
    /// The `server_name` field.
    pub server_name: String,
    /// The `task_queue` field.
    pub task_queue: Option<Arc<RedisTaskQueue>>,
    /// The `active_tasks` field.
    pub active_tasks: Arc<RwLock<HashMap<String, tokio::task::JoinHandle<()>>>>,
    /// The `room_summary_service` field.
    pub room_summary_service: Arc<RoomSummaryService>,
    /// Shared infrastructure injected into sub-services.
    pub(crate) infra: RoomInfrastructure,
    pub(crate) sticky_event_storage: Arc<synapse_storage::sticky_event::StickyEventStorage>,
    pub(crate) event_reader: Arc<dyn synapse_storage::event::EventReader>,
    // Reserved: stored for potential direct use by RoomService methods;
    // currently sub-services receive their own clones via RoomInfrastructure.
    pub(crate) event_writer: Arc<dyn synapse_storage::event::EventWriter>,
}

impl RoomService {
    /// See [`new`].
    #[allow(clippy::expect_used)]
    pub fn new(config: RoomServiceConfig) -> Self {
        // Build shared infrastructure FIRST so its handles can be cloned into
        // MembershipService/MessagingService. Values are supplied once here and
        // immutable thereafter.
        let infra = RoomInfrastructure {
            event_broadcaster: config.event_broadcaster.clone(),
            app_service_manager: config.app_service_manager.clone(),
            key_rotation_manager: config.key_rotation_manager.clone(),
            federation_client: config.federation_client.clone(),
        };

        let membership_cfg = MembershipServiceConfig {
            member_storage: config.member_storage.clone(),
            room_storage: config.room_storage.clone(),
            event_reader: config.event_reader.clone().expect("event_reader required"),
            event_writer: config.event_writer.clone().expect("event_writer required"),
            user_storage: config.user_storage.clone(),
            user_service: config.user_service.clone(),
            room_auth: config.room_auth.clone(),
            server_name: config.server_name.clone(),
            federation_client: infra.federation_client.clone(),
            key_rotation_manager: infra.key_rotation_manager.clone(),
            event_broadcaster: infra.event_broadcaster.clone(),
            room_summary_service: config.room_summary_service.clone(),
            cache: config.cache.clone(),
            key_rotation_storage: config.key_rotation_storage.clone(),
            app_service_manager: config.app_service_manager.clone(),
            db_pool: config.db_pool.clone(),
            policy_service: config.policy_service.clone(),
        };
        let membership = MembershipService::new(membership_cfg);

        let messaging_cfg = MessagingServiceConfig {
            event_reader: config.event_reader.clone().expect("event_reader required"),
            event_writer: config.event_writer.clone().expect("event_writer required"),
            room_storage: config.room_storage.clone(),
            member_storage: config.member_storage.clone(),
            server_name: config.server_name.clone(),
            #[cfg(feature = "beacons")]
            beacon_service: config.beacon_service.clone(),
            #[cfg(not(feature = "beacons"))]
            beacon_service: None,
            task_queue: config.task_queue.clone(),
            relations_storage: config.relations_storage.clone(),
            event_broadcaster: infra.event_broadcaster.clone(),
            app_service_manager: infra.app_service_manager.clone(),
            key_rotation_manager: infra.key_rotation_manager.clone(),
            room_summary_service: config.room_summary_service.clone(),
            cache: config.cache.clone(),
        };
        let messaging = MessagingService::new(messaging_cfg);

        let state_cfg = RoomStateServiceConfig {
            room_storage: config.room_storage.clone(),
            member_storage: config.member_storage.clone(),
            event_reader: config.event_reader.clone().expect("event_reader required"),
            event_writer: config.event_writer.clone().expect("event_writer required"),
            room_tag_storage: config.room_tag_storage.clone(),
            user_storage: config.user_storage.clone(),
            user_service: config.user_service.clone(),
            server_name: config.server_name.clone(),
        };
        let state = RoomStateService::new(state_cfg);

        let lifecycle_cfg = LifecycleServiceConfig {
            room_storage: config.room_storage.clone(),
            member_storage: config.member_storage.clone(),
            event_reader: config.event_reader.clone().expect("event_reader required"),
            event_writer: config.event_writer.clone().expect("event_writer required"),
            user_storage: config.user_storage.clone(),
            user_service: config.user_service.clone(),
            validator: config.validator.clone(),
            server_name: config.server_name.clone(),
            room_summary_service: Some(config.room_summary_service.clone()),
            cache: config.cache.clone(),
            app_service_manager: config.app_service_manager.clone(),
            // MSC4284: room creation policy enforcement lives in the
            // lifecycle sub-service; join/invite live in membership.
            policy_service: config.policy_service.clone(),
        };
        let lifecycle = LifecycleService::new(lifecycle_cfg);

        Self {
            membership,
            messaging,
            state,
            lifecycle,
            room_storage: config.room_storage,
            member_storage: config.member_storage,
            user_storage: config.user_storage,
            room_summary_service: config.room_summary_service,
            validator: config.validator,
            server_name: config.server_name,
            task_queue: config.task_queue,
            active_tasks: Arc::new(RwLock::new(HashMap::new())),
            infra,
            sticky_event_storage: config.sticky_event_storage,
            event_reader: config.event_reader.clone().expect("event_reader required"),
            event_writer: config.event_writer.clone().expect("event_writer required"),
        }
    }

    /// See [`room_summary_service`].
    pub fn room_summary_service(&self) -> &RoomSummaryService {
        &self.room_summary_service
    }

    /// See [`cleanup_completed_tasks`].
    pub async fn cleanup_completed_tasks(&self) -> usize {
        let mut tasks = self.active_tasks.write().await;
        tasks.retain(|_key, handle| !handle.is_finished());
        tasks.len()
    }

    /// See [`abort_task`].
    pub async fn abort_task(&self, task_id: &str) -> bool {
        let mut tasks = self.active_tasks.write().await;
        if let Some(handle) = tasks.remove(task_id) {
            handle.abort();
            true
        } else {
            false
        }
    }

    /// See [`shutdown`].
    pub async fn shutdown(&self) {
        let mut tasks = self.active_tasks.write().await;
        for (task_id, handle) in tasks.drain() {
            ::tracing::info!(task_id = %task_id, "Aborting delayed task");
            handle.abort();
        }
    }

    /// See [`dispatch_appservice_event`].
    pub async fn dispatch_appservice_event(
        &self,
        event_id: &str,
        room_id: &str,
        event_type: &str,
        sender: &str,
        content: &serde_json::Value,
        state_key: Option<&str>,
    ) {
        let Some(app_service_manager) = &self.infra.app_service_manager else {
            return;
        };

        if let Err(error) =
            app_service_manager.enqueue_matching_event(event_id, room_id, event_type, sender, content, state_key).await
        {
            ::tracing::warn!(
                error = %error,
                event_id = %event_id,
                room_id = %room_id,
                event_type = %event_type,
                "Failed to enqueue application service event"
            );
        }
    }

    /// See [`get_room`].
    pub async fn get_room(&self, room_id: &str) -> ApiResult<serde_json::Value> {
        let room = self
            .room_storage
            .get_room(room_id)
            .await
            .map_err(|e| ApiError::internal_with_cause("Failed to get room", e))?;

        match room {
            Some(r) => Ok(json!({
                "room_id": r.room_id,
                "name": r.name,
                "topic": r.topic,
                "canonical_alias": r.canonical_alias,
                "is_public": r.is_public,
                "creator": r.creator_user_id,
                "join_rule": r.join_rule
            })),
            None => Err(ApiError::not_found("Room not found".to_string())),
        }
    }

    /// See [`get_room_state`].
    pub async fn get_room_state(&self, room_id: &str, user_id: &str) -> ApiResult<serde_json::Value> {
        if !self
            .member_storage
            .is_member(room_id, user_id)
            .await
            .map_err(|e| ApiError::internal_with_cause("Failed to check membership", e))?
        {
            return Err(ApiError::forbidden("You are not a member of this room".to_string()));
        }

        let room = self
            .room_storage
            .get_room(room_id)
            .await
            .map_err(|e| ApiError::internal_with_cause("Failed to get room", e))?;

        match room {
            Some(r) => Ok(json!({
                "room_id": r.room_id,
                "name": r.name,
                "topic": r.topic,
                "canonical_alias": r.canonical_alias,
                "is_public": r.is_public,
                "creator": r.creator_user_id,
                "join_rule": r.join_rule
            })),
            None => Err(ApiError::not_found("Room not found".to_string())),
        }
    }

    /// See [`get_user_rooms`].
    pub async fn get_user_rooms(&self, user_id: &str) -> ApiResult<serde_json::Value> {
        let room_ids = self
            .member_storage
            .get_joined_rooms(user_id)
            .await
            .map_err(|e| ApiError::internal_with_cause("Failed to get rooms", e))?;

        let rooms_data = self
            .room_storage
            .get_rooms_batch(&room_ids)
            .await
            .map_err(|e| ApiError::internal_with_cause("Failed to fetch rooms batch", e))?;

        let rooms: Vec<serde_json::Value> = rooms_data
            .into_iter()
            .map(|room| {
                json!({
                    "room_id": room.room_id,
                    "name": room.name,
                    "topic": room.topic,
                    "is_public": room.is_public,
                    "join_rule": room.join_rule
                })
            })
            .collect();

        Ok(json!(rooms))
    }

    /// Collect child room summaries for space hierarchy.
    ///
    /// Given a list of child room IDs, loads room metadata and state events
    /// in batches and returns JSON summaries suitable for inclusion in a
    /// room hierarchy response.
    pub async fn collect_child_rooms(&self, child_room_ids: &[String]) -> ApiResult<Vec<Value>> {
        if child_room_ids.is_empty() {
            return Ok(Vec::new());
        }

        let rooms_batch = self
            .room_storage
            .get_rooms_batch(child_room_ids)
            .await
            .map_err(|e| ApiError::internal_with_cause("Failed to load child rooms", e))?;
        let mut map = HashMap::new();
        for room in rooms_batch {
            map.insert(room.room_id.clone(), room);
        }

        let state_batch = self
            .event_reader
            .get_state_events_batch(child_room_ids)
            .await
            .map_err(|e| ApiError::internal_with_cause("Failed to load child state events", e))?;

        let mut child_rooms = Vec::new();
        for rid in child_room_ids {
            if let Some(child_room) = map.get(rid) {
                let child_state_events: &[StateEvent] = state_batch.get(rid).map_or(&[], |v| v.as_slice());
                let child_room_type = child_state_events
                    .iter()
                    .find(|e| e.event_type.as_deref() == Some("m.room.create"))
                    .and_then(|e| e.content.get("type"))
                    .and_then(|v: &Value| v.as_str())
                    .map_or(Value::Null, |s: &str| Value::String(s.to_string()));
                child_rooms.push(json!({
                    "room_id": child_room.room_id,
                    "name": child_room.name,
                    "topic": child_room.topic,
                    "avatar_url": child_room.avatar_url,
                    "join_rule": child_room.join_rule,
                    "guest_access": if child_room.is_public { "can_join" } else { "forbidden" },
                    "guest_can_join": child_room.is_public,
                    "world_readable": child_room.history_visibility == "world_readable",
                    "num_joined_members": child_room.member_count,
                    "children": [],
                    "children_state": [],
                    "room_type": child_room_type,
                    "required_state_info": []
                }));
            }
        }
        Ok(child_rooms)
    }

    /// See [`upgrade_room`].
    pub async fn upgrade_room(&self, old_room_id: &str, new_version: &str, user_id: &str) -> ApiResult<String> {
        let old_room = self
            .room_storage
            .get_room(old_room_id)
            .await
            .map_err(|e| ApiError::internal_with_cause("Failed to get old room", e))?
            .ok_or_else(|| ApiError::not_found("Room not found".to_string()))?;

        // Fetch old room members BEFORE creating the tombstone, so we can
        // invite them to the replacement room.  We collect joined members
        // excluding the upgrading user (who is auto-invited via create_room).
        let old_members = self
            .member_storage
            .get_room_members(old_room_id, "join")
            .await
            .map_err(|e| ApiError::internal_with_cause("Failed to fetch old room members for migration", e))?;
        let members_to_invite: Vec<String> =
            old_members.into_iter().map(|m| m.user_id).filter(|uid| uid != user_id).collect();

        let tombstone_event_id = generate_event_id(&self.server_name);
        let create_config = CreateRoomConfig {
            visibility: Some(if old_room.is_public { "public".to_string() } else { "private".to_string() }),
            room_alias_name: None,
            name: Some(old_room.name.clone().unwrap_or_else(|| "Upgraded Room".to_string())),
            topic: old_room.topic.clone(),
            invite_list: Some(vec![user_id.to_string()]),
            preset: Some("private_chat".to_string()),
            encryption: old_room.encryption.clone(),
            history_visibility: Some(old_room.history_visibility.clone()),
            is_direct: None,
            room_type: None,
            room_version: Some(new_version.to_string()),
            creation_content: Some(json!({
                "predecessor": {
                    "room_id": old_room_id,
                    "event_id": tombstone_event_id,
                }
            })),
            ..Default::default()
        };

        let replacement_room = self.lifecycle.create_room(user_id, create_config).await?;
        let new_room_id = replacement_room
            .get("room_id")
            .and_then(|value| value.as_str())
            .ok_or_else(|| ApiError::internal("Room upgrade did not return replacement room"))?
            .to_string();

        // Create the tombstone event in the OLD room via the `create_event`
        // wrapper (not the raw storage layer).  This ensures the event is
        // signed with the server's signing key and broadcast to all remote
        // servers with joined members in the old room, so federated
        // homeservers learn that the room has been replaced.
        let tombstone_event = self
            .messaging
            .create_event(
                synapse_storage::CreateEventParams {
                    event_id: tombstone_event_id,
                    room_id: old_room_id.to_string(),
                    user_id: user_id.to_string(),
                    event_type: "m.room.tombstone".to_string(),
                    content: json!({
                        "body": "This room has been replaced",
                        "replacement_room": new_room_id.clone(),
                    }),
                    state_key: Some("".to_string()),
                    origin_server_ts: current_timestamp_millis(),
                    redacts: None,
                },
                None,
            )
            .await
            .map_err(|e| ApiError::internal_with_cause("Failed to create tombstone event", e))?;

        ::tracing::info!(
            old_room_id = %old_room_id,
            new_room_id = %new_room_id,
            tombstone_event_id = %tombstone_event.event_id,
            "Room upgraded: tombstone event created and broadcast"
        );

        // Auto-join the upgrading user to the new room.  `create_room` only
        // invites the creator; we need them to be a joined member so they can
        // immediately use the replacement room (matches Synapse behavior).
        if let Err(e) = self.membership.join_room(&new_room_id, user_id).await {
            ::tracing::warn!(
                old_room_id = %old_room_id,
                new_room_id = %new_room_id,
                user_id = %user_id,
                error = %e,
                "Failed to auto-join upgrading user to replacement room"
            );
        }

        // Phase 1: Partition members into local vs remote.
        // `is_remote_user` is a cheap synchronous check (string parsing).
        let mut locals = Vec::with_capacity(members_to_invite.len());
        let mut remotes = Vec::with_capacity(members_to_invite.len());
        for uid in &members_to_invite {
            if self.membership.is_remote_user(uid) {
                remotes.push(uid.clone());
            } else {
                locals.push(uid.clone());
            }
        }

        // Phase 2: Invite local users concurrently.
        // MembershipService is Arc-based (Clone), so cloning is cheap and safe for
        // use inside concurrent futures — no shared mutable state.
        let membership = self.membership.clone();
        let new_room_id = new_room_id.clone();
        let inviter_id = user_id.to_string();
        let old_room_id_owned = old_room_id.to_string();

        let local_futures: Vec<_> = locals
            .iter()
            .map(|invitee_id| {
                let membership = membership.clone();
                let new_room_id = new_room_id.clone();
                let inviter_id = inviter_id.clone();
                let old_room_id = old_room_id_owned.clone();
                let invitee_id = invitee_id.clone();
                async move {
                    if let Err(e) = membership.invite_user(&new_room_id, &inviter_id, &invitee_id).await {
                        ::tracing::warn!(
                            old_room_id = %old_room_id,
                            new_room_id = %new_room_id,
                            invitee_id = %invitee_id,
                            error = %e,
                            "Failed to invite former member to replacement room (local path)"
                        );
                    }
                }
            })
            .collect();
        // Local invites: gather all results. Per-invite failures are warn-only
        // (logged inside the closure) and do not fail the upgrade — matches
        // Synapse behavior where a single invitee failure does not abort the
        // whole room upgrade.
        let _: Vec<()> = futures::future::join_all(local_futures).await;

        // Phase 3: Invite remote users via federation concurrently, with bounded
        // concurrency.  Remote servers may be slow or unreachable; we limit
        // in-flight requests to avoid overwhelming them (matches Synapse's default
        // `join_max_concurrency = 16`).  Failures are logged but do not block
        // the upgrade — the room is functional even if some former members were
        // not re-invited automatically.
        const FEDERATION_INVITE_CONCURRENCY: usize = 16;
        let remote_futures: Vec<_> = remotes
            .iter()
            .map(|invitee_id| {
                let membership = membership.clone();
                let new_room_id = new_room_id.clone();
                let inviter_id = inviter_id.clone();
                let old_room_id = old_room_id_owned.clone();
                let invitee_id = invitee_id.clone();
                async move {
                    if let Err(e) = membership.invite_user(&new_room_id, &inviter_id, &invitee_id).await {
                        ::tracing::warn!(
                            old_room_id = %old_room_id,
                            new_room_id = %new_room_id,
                            invitee_id = %invitee_id,
                            error = %e,
                            "Failed to invite former member to replacement room (federation path)"
                        );
                    }
                }
            })
            .collect();
        // Federation invites: always collect all results so a slow remote server
        // cannot block other invites.  Individual failures are warn-only (logged
        // inside the closure above).
        let _: Vec<()> = stream::iter(remote_futures).buffer_unordered(FEDERATION_INVITE_CONCURRENCY).collect().await;

        // Copy state events (power levels, join_rules, canonical_alias, etc.)
        // from the old room to the new room.  This is best-effort: failures
        // are logged but do not fail the upgrade, since the new room is
        // already functional with its default state.
        if let Err(e) = self.lifecycle.migrate_room_content(old_room_id, &new_room_id, user_id).await {
            ::tracing::warn!(
                old_room_id = %old_room_id,
                new_room_id = %new_room_id,
                error = %e,
                "Failed to migrate room content (best-effort)"
            );
        }

        Ok(new_room_id)
    }

    /// See [`set_is_sticky_event`].
    pub async fn set_is_sticky_event(
        &self,
        room_id: &str,
        user_id: &str,
        event_id: &str,
        event_type: &str,
        is_sticky: bool,
    ) -> ApiResult<()> {
        self.sticky_event_storage
            .set_is_sticky_event(room_id, user_id, event_id, event_type, is_sticky)
            .await
            .map_err(|e| ApiError::internal_with_cause("Failed to set sticky event", e))
    }

    /// See [`get_is_sticky_event`].
    pub async fn get_is_sticky_event(
        &self,
        room_id: &str,
        user_id: &str,
        event_type: &str,
    ) -> ApiResult<Option<synapse_storage::sticky_event::StickyEvent>> {
        self.sticky_event_storage
            .get_is_sticky_event(room_id, user_id, event_type)
            .await
            .map_err(|e| ApiError::internal_with_cause("Failed to get sticky event", e))
    }

    /// See [`get_all_is_sticky_events`].
    pub async fn get_all_is_sticky_events(
        &self,
        room_id: &str,
        user_id: &str,
    ) -> ApiResult<Vec<synapse_storage::sticky_event::StickyEvent>> {
        self.sticky_event_storage
            .get_all_is_sticky_events(room_id, user_id)
            .await
            .map_err(|e| ApiError::internal_with_cause("Failed to get all sticky events", e))
    }

    /// See [`clear_is_sticky_event`].
    pub async fn clear_is_sticky_event(&self, room_id: &str, user_id: &str, event_type: &str) -> ApiResult<()> {
        self.sticky_event_storage
            .clear_is_sticky_event(room_id, user_id, event_type)
            .await
            .map_err(|e| ApiError::internal_with_cause("Failed to clear sticky event", e))
    }
}

#[cfg(feature = "friends")]
impl From<crate::friend_room_service::FriendRoomCreateRoomConfig> for CreateRoomConfig {
    fn from(config: crate::friend_room_service::FriendRoomCreateRoomConfig) -> Self {
        Self {
            visibility: config.visibility,
            room_alias_name: config.room_alias_name,
            name: config.name,
            topic: config.topic,
            invite_list: config.invite_list,
            invite_reasons: None,
            preset: config.preset,
            encryption: config.encryption,
            history_visibility: config.history_visibility,
            is_direct: config.is_direct,
            room_type: config.room_type,
            initial_state: config.initial_state,
            creation_content: config.creation_content,
            room_version: config.room_version,
            power_level_content_override: config.power_level_content_override,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_room_id_format() {
        let room_id = generate_room_id("example.com");
        assert!(room_id.starts_with('!'));
        assert!(room_id.contains(":example.com"));
    }

    #[test]
    fn test_event_id_format() {
        let event_id = generate_event_id("example.com");
        assert!(event_id.starts_with('$'));
    }

    #[test]
    fn test_create_room_response_format() {
        let room_id = "!testroom:example.com";
        let room_alias = "#test:example.com";

        let response = json!({
            "room_id": room_id,
            "room_alias": room_alias
        });

        assert_eq!(response["room_id"], room_id);
        assert_eq!(response["room_alias"], room_alias);
    }

    #[test]
    fn test_message_response_format() {
        let response = json!({
            "event_id": "$test_event",
            "room_id": "!testroom:example.com"
        });

        assert!(response["event_id"].is_string());
        assert!(response["room_id"].is_string());
    }

    #[test]
    fn test_public_room_visibility() {
        let is_public = true;
        assert!(is_public);
    }

    #[test]
    fn test_private_room_visibility() {
        let is_public = false;
        assert!(!is_public);
    }

    #[test]
    fn test_join_rule_public() {
        let join_rule = "public";
        assert_eq!(join_rule, "public");
    }

    #[test]
    fn test_join_rule_invite() {
        let join_rule = "invite";
        assert_eq!(join_rule, "invite");
    }

    #[test]
    fn test_join_rule_trusted_private() {
        let preset = "trusted_private_chat";
        let join_rule = match preset {
            "trusted_private_chat" => "invite",
            _ => "other",
        };
        assert_eq!(join_rule, "invite");
    }

    #[test]
    fn test_trusted_private_chat_preset_config() {
        let config = CreateRoomConfig { preset: Some("trusted_private_chat".to_string()), ..Default::default() };
        assert_eq!(config.preset.as_deref(), Some("trusted_private_chat"));
    }

    #[test]
    fn test_burn_after_read_metadata_detection() {
        let content = json!({
            "body": "secret message",
            "msgtype": "m.text",
            "burn_after_read": true
        });

        let has_metadata = content.as_object().is_some_and(|c| c.contains_key("burn_after_read"));

        assert!(has_metadata);
    }

    #[test]
    fn test_room_state_format() {
        let state = json!({
            "m.room.name": json!({
                "name": "Test Room"
            }),
            "m.room.topic": json!({
                "topic": "Test Topic"
            })
        });

        assert!(state.is_object());
        assert!(state.get("m.room.name").is_some());
    }

    #[test]
    fn test_room_list_response_format() {
        let room_list = vec![
            json!({
                "room_id": "!room1:example.com",
                "name": "Room 1",
                "member_count": 5
            }),
            json!({
                "room_id": "!room2:example.com",
                "name": "Room 2",
                "member_count": 10
            }),
        ];

        let response = json!({
            "chunk": room_list,
            "total_room_count_estimate": 2
        });

        assert_eq!(response["chunk"].as_array().unwrap().len(), 2);
        assert_eq!(response["total_room_count_estimate"], 2);
    }
}
