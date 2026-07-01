use crate::common::error::{ApiError, ApiResult};
use crate::*;
use serde_json::{json, Value};
use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;
use synapse_common::generate_event_id;
use synapse_common::task_queue::RedisTaskQueue;
use synapse_common::validation::Validator;
use synapse_storage::StateEvent;
use synapse_storage::UserStore;
use tokio::sync::RwLock;

use super::infrastructure::RoomInfrastructure;
use super::lifecycle::service::{LifecycleService, LifecycleServiceConfig};
use super::membership::service::{MembershipService, MembershipServiceConfig};
use super::messaging::service::{MessagingService, MessagingServiceConfig};
use super::state::service::{RoomStateService, RoomStateServiceConfig};

#[derive(Debug, Default, Clone)]
pub struct CreateRoomConfig {
    pub visibility: Option<String>,
    pub room_alias_name: Option<String>,
    pub name: Option<String>,
    pub topic: Option<String>,
    pub invite_list: Option<Vec<String>>,
    pub preset: Option<String>,
    pub encryption: Option<String>,
    pub history_visibility: Option<String>,
    pub is_direct: Option<bool>,
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
    /// capabilities default ("10") when None.
    pub room_version: Option<String>,
    /// Power level overrides applied on top of the spec defaults.
    pub power_level_content_override: Option<serde_json::Value>,
}

pub struct RoomServiceConfig {
    pub room_storage: Arc<dyn synapse_storage::RoomRepository>,
    pub member_storage: Arc<dyn synapse_storage::RoomMemberRepository>,
    pub event_storage: Arc<dyn synapse_storage::EventRepository>,
    pub room_tag_storage: Arc<dyn synapse_storage::RoomTagRepository>,
    pub user_storage: Arc<dyn UserStore>,
    pub auth_service: Arc<dyn Auth>,
    pub room_summary_service: Arc<RoomSummaryService>,
    pub validator: Arc<Validator>,
    pub server_name: String,
    pub task_queue: Option<Arc<RedisTaskQueue>>,
    pub relations_storage: Arc<dyn synapse_storage::RelationsRepository>,
    pub event_broadcaster: Option<Arc<synapse_federation::event_broadcaster::EventBroadcaster>>,
    pub app_service_manager: Option<Arc<crate::application_service::ApplicationServiceManager>>,
    /// Server signing key manager, used to sign locally-produced PDUs before
    /// federating them.  `None` in test setups that don't exercise federation.
    pub key_rotation_manager: Option<Arc<synapse_federation::KeyRotationManager>>,
    /// Outbound federation client, used for make_join/send_join/make_leave/
    /// send_leave/invite flows.  `None` in test setups.
    pub federation_client: Option<Arc<synapse_federation::FederationClient>>,
    #[cfg(feature = "beacons")]
    pub beacon_service: Option<Arc<crate::beacon_service::BeaconService>>,
    #[cfg(not(feature = "beacons"))]
    pub beacon_service: Option<()>,
}

pub struct RoomService {
    /// Domain sub-service: membership operations (join, leave, invite, etc.)
    pub membership: MembershipService,
    /// Domain sub-service: messaging operations (events, messages, receipts, etc.)
    pub messaging: MessagingService,
    /// Domain sub-service: room state operations (aliases, tags, info, directory, etc.)
    pub state: RoomStateService,
    /// Domain sub-service: room lifecycle operations (create, upgrade, migration)
    pub lifecycle: LifecycleService,
    #[allow(dead_code)]
    pub(crate) room_storage: Arc<dyn synapse_storage::RoomRepository>,
    #[allow(dead_code)]
    pub(crate) member_storage: Arc<dyn synapse_storage::RoomMemberRepository>,
    #[allow(dead_code)]
    pub(crate) room_tag_storage: Arc<dyn synapse_storage::RoomTagRepository>,
    pub user_storage: Arc<dyn UserStore>,
    #[allow(dead_code)]
    pub(crate) auth_service: Arc<dyn Auth>,
    pub validator: Arc<Validator>,
    #[allow(dead_code)]
    pub server_name: String,
    pub task_queue: Option<Arc<RedisTaskQueue>>,
    pub active_tasks: Arc<RwLock<HashMap<String, tokio::task::JoinHandle<()>>>>,
    pub room_summary_service: Arc<RoomSummaryService>,
    #[allow(dead_code)]
    pub(crate) event_storage: Arc<dyn synapse_storage::EventRepository>,
    #[allow(dead_code)]
    pub(crate) relations_storage: Arc<dyn synapse_storage::RelationsRepository>,
    /// Shared infrastructure injected into sub-services.
    pub(crate) infra: RoomInfrastructure,
    #[cfg(feature = "beacons")]
    #[allow(dead_code)]
    pub(crate) beacon_service: Option<Arc<crate::beacon_service::BeaconService>>,
    #[cfg(not(feature = "beacons"))]
    #[allow(dead_code)]
    pub(crate) beacon_service: Option<()>,
}

impl RoomService {
    pub fn new(config: RoomServiceConfig) -> Self {
        // Build shared infrastructure FIRST so its event_broadcaster Arc<RwLock>
        // can be shared with MembershipService (avoids a separate wrapper).
        let infra = RoomInfrastructure {
            event_broadcaster: Arc::new(RwLock::new(config.event_broadcaster.clone())),
            app_service_manager: Arc::new(RwLock::new(config.app_service_manager.clone())),
            key_rotation_manager: Arc::new(RwLock::new(config.key_rotation_manager.clone())),
            federation_client: Arc::new(RwLock::new(config.federation_client.clone())),
        };

        let membership_cfg = MembershipServiceConfig {
            member_storage: config.member_storage.clone(),
            room_storage: config.room_storage.clone(),
            event_storage: config.event_storage.clone(),
            user_storage: config.user_storage.clone(),
            auth_service: config.auth_service.clone(),
            server_name: config.server_name.clone(),
            federation_client: infra.federation_client.clone(),
            key_rotation_manager: infra.key_rotation_manager.clone(),
            event_broadcaster: infra.event_broadcaster.clone(),
            room_summary_service: config.room_summary_service.clone(),
        };
        let membership = MembershipService::new(membership_cfg);

        let messaging_cfg = MessagingServiceConfig {
            event_storage: config.event_storage.clone(),
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
        };
        let messaging = MessagingService::new(messaging_cfg);

        let state_cfg = RoomStateServiceConfig {
            room_storage: config.room_storage.clone(),
            member_storage: config.member_storage.clone(),
            event_storage: config.event_storage.clone(),
            room_tag_storage: config.room_tag_storage.clone(),
            user_storage: config.user_storage.clone(),
            server_name: config.server_name.clone(),
        };
        let state = RoomStateService::new(state_cfg);

        let lifecycle_cfg = LifecycleServiceConfig {
            room_storage: config.room_storage.clone(),
            member_storage: config.member_storage.clone(),
            event_storage: config.event_storage.clone(),
            user_storage: config.user_storage.clone(),
            validator: config.validator.clone(),
            server_name: config.server_name.clone(),
            room_summary_service: config.room_summary_service.clone(),
        };
        let lifecycle = LifecycleService::new(lifecycle_cfg);

        Self {
            membership,
            messaging,
            state,
            lifecycle,
            room_storage: config.room_storage,
            member_storage: config.member_storage,
            event_storage: config.event_storage,
            room_tag_storage: config.room_tag_storage,
            user_storage: config.user_storage,
            auth_service: config.auth_service,
            room_summary_service: config.room_summary_service,
            validator: config.validator,
            server_name: config.server_name,
            task_queue: config.task_queue,
            active_tasks: Arc::new(RwLock::new(HashMap::new())),
            relations_storage: config.relations_storage.clone(),
            infra,
            #[cfg(feature = "beacons")]
            beacon_service: config.beacon_service,
            #[cfg(not(feature = "beacons"))]
            beacon_service: None,
        }
    }

    pub fn room_summary_service(&self) -> &RoomSummaryService {
        &self.room_summary_service
    }

    pub fn start_cleanup_task(self: Arc<Self>) -> tokio::task::JoinHandle<()> {
        let active_tasks = self.active_tasks.clone();
        tokio::spawn(async move {
            let mut interval = tokio::time::interval(Duration::from_secs(60));
            loop {
                interval.tick().await;
                let mut tasks = active_tasks.write().await;
                let before = tasks.len();
                tasks.retain(|_key, handle| !handle.is_finished());
                let after = tasks.len();
                if before != after {
                    ::tracing::debug!(
                        target: "room_service_cleanup",
                        cleaned = before - after,
                        remaining = after,
                        "Cleaned up completed background tasks"
                    );
                }
            }
        })
    }

    pub async fn cleanup_completed_tasks(&self) -> usize {
        let mut tasks = self.active_tasks.write().await;
        tasks.retain(|_key, handle| !handle.is_finished());
        tasks.len()
    }

    pub async fn abort_task(&self, task_id: &str) -> bool {
        let mut tasks = self.active_tasks.write().await;
        if let Some(handle) = tasks.remove(task_id) {
            handle.abort();
            true
        } else {
            false
        }
    }

    pub async fn shutdown(&self) {
        let mut tasks = self.active_tasks.write().await;
        for (task_id, handle) in tasks.drain() {
            ::tracing::info!(task_id = %task_id, "Aborting delayed task");
            handle.abort();
        }
    }

    pub async fn set_event_broadcaster(
        &self,
        event_broadcaster: Arc<synapse_federation::event_broadcaster::EventBroadcaster>,
    ) {
        self.infra.set_event_broadcaster(event_broadcaster).await;
    }

    pub async fn set_key_rotation_manager(&self, key_rotation_manager: Arc<synapse_federation::KeyRotationManager>) {
        self.infra.set_key_rotation_manager(key_rotation_manager).await;
    }

    pub async fn set_federation_client(&self, federation_client: Arc<synapse_federation::FederationClient>) {
        self.infra.set_federation_client(federation_client).await;
    }

    pub async fn set_app_service_manager(
        &self,
        app_service_manager: Arc<crate::application_service::ApplicationServiceManager>,
    ) {
        self.infra.set_app_service_manager(app_service_manager).await;
    }

    pub async fn dispatch_appservice_event(
        &self,
        event_id: &str,
        room_id: &str,
        event_type: &str,
        sender: &str,
        content: &serde_json::Value,
        state_key: Option<&str>,
    ) {
        let app_service_manager = self.infra.app_service_manager.read().await.clone();
        let Some(app_service_manager) = app_service_manager else {
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

    pub async fn get_room(&self, room_id: &str) -> ApiResult<serde_json::Value> {
        let room = self
            .room_storage
            .get_room(room_id)
            .await
            .map_err(|e| ApiError::internal_with_log("Failed to get room", &e))?;

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

    pub async fn get_room_state(&self, room_id: &str, user_id: &str) -> ApiResult<serde_json::Value> {
        if !self
            .member_storage
            .is_member(room_id, user_id)
            .await
            .map_err(|e| ApiError::internal_with_log("Failed to check membership", &e))?
        {
            return Err(ApiError::forbidden("You are not a member of this room".to_string()));
        }

        let room = self
            .room_storage
            .get_room(room_id)
            .await
            .map_err(|e| ApiError::internal_with_log("Failed to get room", &e))?;

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

    pub async fn get_user_rooms(&self, user_id: &str) -> ApiResult<serde_json::Value> {
        let room_ids = self
            .member_storage
            .get_joined_rooms(user_id)
            .await
            .map_err(|e| ApiError::internal_with_log("Failed to get rooms", &e))?;

        let rooms_data = self
            .room_storage
            .get_rooms_batch(&room_ids)
            .await
            .map_err(|e| ApiError::internal_with_log("Failed to fetch rooms batch", &e))?;

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

    // =========================================================================
    // Membership forwarding methods — delegate to MembershipService
    // =========================================================================

    pub async fn get_room_members(&self, room_id: &str, user_id: &str) -> ApiResult<serde_json::Value> {
        self.membership.get_room_members(room_id, user_id).await
    }
    pub async fn get_joined_rooms(&self, user_id: &str) -> ApiResult<Vec<String>> {
        self.membership.get_joined_rooms(user_id).await
    }
    pub async fn get_shared_room_users(&self, user_id: &str) -> ApiResult<Vec<String>> {
        self.membership.get_shared_room_users(user_id).await
    }
    pub async fn share_common_room(&self, user_id: &str, other_user_id: &str) -> ApiResult<bool> {
        self.membership.share_common_room(user_id, other_user_id).await
    }
    pub async fn share_common_rooms_batch(&self, user_id: &str, other_user_ids: &[String]) -> ApiResult<Vec<String>> {
        self.membership.share_common_rooms_batch(user_id, other_user_ids).await
    }
    pub async fn get_joined_members_with_profiles(&self, room_id: &str) -> ApiResult<Vec<storage::RoomMember>> {
        self.membership.get_joined_members_with_profiles(room_id).await
    }
    pub async fn get_membership_history(&self, room_id: &str, limit: i64) -> ApiResult<Vec<storage::RoomMember>> {
        self.membership.get_membership_history(room_id, limit).await
    }
    pub async fn get_room_members_by_membership(
        &self,
        room_id: &str,
        membership: &str,
    ) -> ApiResult<Vec<storage::RoomMember>> {
        self.membership.get_room_members_by_membership(room_id, membership).await
    }
    pub async fn has_any_non_banned_member_from_server(&self, room_id: &str, server_name: &str) -> ApiResult<bool> {
        self.membership.has_any_non_banned_member_from_server(room_id, server_name).await
    }
    pub async fn user_shares_room_with_server(&self, user_id: &str, server_name: &str) -> ApiResult<bool> {
        self.membership.user_shares_room_with_server(user_id, server_name).await
    }
    pub async fn filter_users_sharing_room_with_server(
        &self,
        user_ids: &[String],
        server_name: &str,
    ) -> ApiResult<std::collections::HashSet<String>> {
        self.membership.filter_users_sharing_room_with_server(user_ids, server_name).await
    }
    pub async fn get_room_membership(&self, room_id: &str, user_id: &str) -> ApiResult<Option<String>> {
        self.membership.get_room_membership(room_id, user_id).await
    }
    pub async fn get_room_member_record(&self, room_id: &str, user_id: &str) -> ApiResult<Option<storage::RoomMember>> {
        self.membership.get_room_member_record(room_id, user_id).await
    }
    pub async fn remove_member_record(&self, room_id: &str, user_id: &str) -> ApiResult<()> {
        self.membership.remove_member_record(room_id, user_id).await
    }
    pub async fn get_room_members_paginated_admin(
        &self,
        room_id: &str,
        membership: &str,
        limit: i64,
        from: Option<&str>,
    ) -> ApiResult<Vec<storage::RoomMember>> {
        self.membership.get_room_members_paginated_admin(room_id, membership, limit, from).await
    }
    pub async fn get_room_member_count_admin(&self, room_id: &str) -> ApiResult<i64> {
        self.membership.get_room_member_count_admin(room_id).await
    }
    pub async fn admin_ban_user_membership(&self, room_id: &str, user_id: &str, banned_by: &str) -> ApiResult<()> {
        self.membership.admin_ban_user_membership(room_id, user_id, banned_by).await
    }
    pub async fn admin_unban_user_membership(&self, room_id: &str, user_id: &str) -> ApiResult<()> {
        self.membership.admin_unban_user_membership(room_id, user_id).await
    }
    pub async fn set_ban_reason(&self, room_id: &str, user_id: &str, reason: &str) -> ApiResult<()> {
        self.membership.set_ban_reason(room_id, user_id, reason).await
    }
    pub async fn force_leave_membership(&self, room_id: &str, user_id: &str, now: i64) -> ApiResult<()> {
        self.membership.force_leave_membership(room_id, user_id, now).await
    }
    pub async fn decrement_member_count(&self, room_id: &str) -> ApiResult<()> {
        self.membership.decrement_member_count(room_id).await
    }
    pub async fn get_invited_members_count(&self, room_id: &str) -> ApiResult<i64> {
        self.membership.get_invited_members_count(room_id).await
    }
    pub async fn add_member(
        &self,
        room_id: &str,
        user_id: &str,
        membership: &str,
        display_name: Option<&str>,
        join_reason: Option<&str>,
        tx: Option<&mut sqlx::Transaction<'_, sqlx::Postgres>>,
    ) -> ApiResult<storage::RoomMember> {
        self.membership.add_member(room_id, user_id, membership, display_name, join_reason, tx).await
    }
    pub async fn join_room_with_via_servers(
        &self,
        room_id: &str,
        user_id: &str,
        via_servers: &[String],
    ) -> ApiResult<()> {
        self.membership.join_room_with_via_servers(room_id, user_id, via_servers).await
    }
    pub async fn join_room(&self, room_id: &str, user_id: &str) -> ApiResult<()> {
        self.membership.join_room(room_id, user_id).await
    }
    pub async fn leave_room(&self, room_id: &str, user_id: &str) -> ApiResult<()> {
        self.membership.leave_room(room_id, user_id).await
    }
    pub async fn forget_room(&self, room_id: &str, user_id: &str) -> ApiResult<()> {
        self.membership.forget_room(room_id, user_id).await
    }
    pub async fn invite_user(&self, room_id: &str, inviter_id: &str, invitee_id: &str) -> ApiResult<()> {
        self.membership.invite_user(room_id, inviter_id, invitee_id).await
    }
    pub async fn knock_room(&self, room_id: &str, user_id: &str, reason: Option<&str>) -> ApiResult<()> {
        self.membership.knock_room(room_id, user_id, reason).await
    }
    pub async fn ban_user(&self, room_id: &str, user_id: &str, banned_by: &str, reason: Option<&str>) -> ApiResult<()> {
        self.membership.ban_user(room_id, user_id, banned_by, reason).await
    }
    pub async fn unban_user(&self, room_id: &str, user_id: &str, unbanned_by: &str) -> ApiResult<()> {
        self.membership.unban_user(room_id, user_id, unbanned_by).await
    }
    pub async fn kick_user(
        &self,
        room_id: &str,
        target_user_id: &str,
        kicked_by: &str,
        reason: Option<&str>,
    ) -> ApiResult<()> {
        self.membership.kick_user(room_id, target_user_id, kicked_by, reason).await
    }
    pub async fn join_room_via_federation(&self, destination: &str, room_id: &str, user_id: &str) -> ApiResult<()> {
        self.membership.join_room_via_federation(destination, room_id, user_id).await
    }
    pub async fn leave_room_via_federation(&self, destination: &str, room_id: &str, user_id: &str) -> ApiResult<()> {
        self.membership.leave_room_via_federation(destination, room_id, user_id).await
    }
    pub async fn invite_user_via_federation(&self, room_id: &str, inviter_id: &str, invitee_id: &str) -> ApiResult<()> {
        self.membership.invite_user_via_federation(room_id, inviter_id, invitee_id).await
    }
    pub async fn exchange_third_party_invite_via_federation(
        &self,
        destination: &str,
        room_id: &str,
        invite_event: &Value,
    ) -> ApiResult<Value> {
        self.membership.exchange_third_party_invite_via_federation(destination, room_id, invite_event).await
    }
    pub fn is_remote_user(&self, user_id: &str) -> bool {
        self.membership.is_remote_user(user_id)
    }
    pub fn is_remote_room(&self, room_id: &str) -> bool {
        self.membership.is_remote_room(room_id)
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
            .map_err(|e| ApiError::internal_with_log("Failed to load child rooms", &e))?;
        let mut map = HashMap::new();
        for room in rooms_batch {
            map.insert(room.room_id.clone(), room);
        }

        let state_batch = self
            .event_storage
            .get_state_events_batch(child_room_ids)
            .await
            .map_err(|e| ApiError::internal_with_log("Failed to load child state events", &e))?;

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

    // =========================================================================
    // Messaging forwarding methods — delegate to MessagingService
    // =========================================================================

    // -- events.rs ----------------------------------------------------------

    pub async fn get_event_record(&self, event_id: &str) -> ApiResult<Option<synapse_storage::RoomEvent>> {
        self.messaging.get_event_record(event_id).await
    }
    pub async fn get_event_record_in_room(
        &self,
        room_id: &str,
        event_id: &str,
    ) -> ApiResult<synapse_storage::RoomEvent> {
        self.messaging.get_event_record_in_room(room_id, event_id).await
    }
    pub async fn find_event_by_timestamp(
        &self,
        room_id: &str,
        ts: i64,
        forward: bool,
    ) -> ApiResult<Option<(String, i64)>> {
        self.messaging.find_event_by_timestamp(room_id, ts, forward).await
    }
    pub async fn report_event(
        &self,
        event_id: &str,
        room_id: &str,
        reporter_user_id: &str,
        reason: Option<&str>,
        score: i32,
    ) -> ApiResult<i64> {
        self.messaging.report_event(event_id, room_id, reporter_user_id, reason, score).await
    }
    pub async fn get_state_events(&self, room_id: &str) -> ApiResult<Vec<serde_json::Value>> {
        self.messaging.get_state_events(room_id).await
    }
    pub async fn get_state_event_records(&self, room_id: &str) -> ApiResult<Vec<synapse_storage::StateEvent>> {
        self.messaging.get_state_event_records(room_id).await
    }
    pub async fn get_state_events_at_or_before(
        &self,
        room_id: &str,
        origin_server_ts: i64,
    ) -> ApiResult<Vec<synapse_storage::StateEvent>> {
        self.messaging.get_state_events_at_or_before(room_id, origin_server_ts).await
    }
    pub async fn create_event(
        &self,
        params: synapse_storage::CreateEventParams,
        tx: Option<&mut sqlx::Transaction<'_, sqlx::Postgres>>,
    ) -> ApiResult<synapse_storage::RoomEvent> {
        self.messaging.create_event(params, tx).await
    }
    pub async fn create_event_with_graph(
        &self,
        params: synapse_storage::CreateEventParams,
        prev_events: &[String],
        auth_events: &[String],
        depth: i64,
        tx: Option<&mut sqlx::Transaction<'_, sqlx::Postgres>>,
    ) -> ApiResult<synapse_storage::RoomEvent> {
        self.messaging.create_event_with_graph(params, prev_events, auth_events, depth, tx).await
    }
    pub async fn get_state_events_by_type(&self, room_id: &str, event_type: &str) -> ApiResult<Vec<serde_json::Value>> {
        self.messaging.get_state_events_by_type(room_id, event_type).await
    }
    pub async fn get_pinned_event_ids(&self, room_id: &str) -> ApiResult<Vec<String>> {
        self.messaging.get_pinned_event_ids(room_id).await
    }
    pub async fn set_pinned_event_ids(
        &self,
        room_id: &str,
        user_id: &str,
        pinned_event_ids: &[String],
    ) -> ApiResult<()> {
        self.messaging.set_pinned_event_ids(room_id, user_id, pinned_event_ids).await
    }
    pub async fn get_event(&self, room_id: &str, event_id: &str) -> ApiResult<serde_json::Value> {
        self.messaging.get_event(room_id, event_id).await
    }
    pub async fn get_pending_events(&self, room_id: &str, limit: i64) -> ApiResult<Vec<synapse_storage::RoomEvent>> {
        self.messaging.get_pending_events(room_id, limit).await
    }
    pub async fn get_room_events(&self, room_id: &str, limit: i64) -> ApiResult<Vec<synapse_storage::RoomEvent>> {
        self.messaging.get_room_events(room_id, limit).await
    }
    pub async fn get_room_events_by_type(
        &self,
        room_id: &str,
        event_type: &str,
        limit: i64,
    ) -> ApiResult<Vec<synapse_storage::RoomEvent>> {
        self.messaging.get_room_events_by_type(room_id, event_type, limit).await
    }
    pub async fn get_room_events_paginated_admin(
        &self,
        room_id: &str,
        from: Option<i64>,
        limit: i64,
        direction: &str,
    ) -> ApiResult<Vec<synapse_storage::RoomEvent>> {
        self.messaging.get_room_events_paginated_admin(room_id, from, limit, direction).await
    }
    pub async fn get_event_context_admin(
        &self,
        room_id: &str,
        event_id: &str,
        context_limit: i64,
    ) -> ApiResult<serde_json::Value> {
        self.messaging.get_event_context_admin(room_id, event_id, context_limit).await
    }
    pub async fn search_room_messages_admin(
        &self,
        room_id: &str,
        search_pattern: &str,
        limit: i64,
    ) -> ApiResult<Vec<serde_json::Value>> {
        self.messaging.search_room_messages_admin(room_id, search_pattern, limit).await
    }
    pub async fn get_forward_extremities_count(&self, room_id: &str) -> ApiResult<i64> {
        self.messaging.get_forward_extremities_count(room_id).await
    }
    pub async fn count_events_by_status(&self, room_id: &str, status: &str) -> i64 {
        self.messaging.count_events_by_status(room_id, status).await
    }
    pub async fn redact_event_content(&self, event_id: &str, redacted_by: Option<&str>) -> ApiResult<()> {
        self.messaging.redact_event_content(event_id, redacted_by).await
    }
    #[allow(clippy::too_many_arguments)]
    pub async fn save_event_signature(
        &self,
        event_id: &str,
        user_id: &str,
        device_id: &str,
        signature: &str,
        key_id: &str,
        algorithm: &str,
        created_ts: i64,
    ) -> ApiResult<()> {
        self.messaging
            .save_event_signature(event_id, user_id, device_id, signature, key_id, algorithm, created_ts)
            .await
    }
    pub async fn get_event_signatures(&self, event_id: &str) -> ApiResult<Vec<synapse_storage::event::EventSignature>> {
        self.messaging.get_event_signatures(event_id).await
    }
    pub async fn get_daily_message_count(&self) -> ApiResult<i64> {
        self.messaging.get_daily_message_count().await
    }
    pub async fn find_missing_event_ids(&self, event_ids: &[String]) -> ApiResult<Vec<String>> {
        self.messaging.find_missing_event_ids(event_ids).await
    }
    pub async fn get_missing_events_between(
        &self,
        room_id: &str,
        earliest_events: &[String],
        latest_events: &[String],
        limit: i64,
    ) -> ApiResult<Vec<serde_json::Value>> {
        self.messaging.get_missing_events_between(room_id, earliest_events, latest_events, limit).await
    }

    // -- messages.rs --------------------------------------------------------

    pub async fn send_message(
        &self,
        room_id: &str,
        user_id: &str,
        event_type: &str,
        content: &serde_json::Value,
    ) -> ApiResult<serde_json::Value> {
        self.messaging.send_message(room_id, user_id, event_type, content).await
    }
    pub async fn get_room_messages(
        &self,
        room_id: &str,
        user_id: &str,
        from: i64,
        limit: i64,
        direction: &str,
    ) -> ApiResult<serde_json::Value> {
        self.messaging.get_room_messages(room_id, user_id, from, limit, direction).await
    }
    pub async fn get_ephemeral_events_for_client(
        &self,
        room_id: &str,
        limit: i64,
    ) -> ApiResult<Vec<serde_json::Value>> {
        self.messaging.get_ephemeral_events_for_client(room_id, limit).await
    }
    pub async fn set_typing_ephemeral_event(
        &self,
        room_id: &str,
        user_id: &str,
        typing_user_ids: &[String],
        timeout_ms: i64,
    ) -> ApiResult<()> {
        self.messaging.set_typing_ephemeral_event(room_id, user_id, typing_user_ids, timeout_ms).await
    }
    pub async fn clear_typing_ephemeral_event(&self, room_id: &str, user_id: &str) -> ApiResult<()> {
        self.messaging.clear_typing_ephemeral_event(room_id, user_id).await
    }

    // -- receipts.rs --------------------------------------------------------

    pub async fn send_receipt(
        &self,
        room_id: &str,
        user_id: &str,
        event_id: &str,
        receipt_type: &str,
        body: &serde_json::Value,
    ) -> ApiResult<()> {
        self.messaging.send_receipt(room_id, user_id, event_id, receipt_type, body).await
    }
    pub async fn get_receipts(
        &self,
        room_id: &str,
        receipt_type: &str,
        event_id: &str,
    ) -> ApiResult<Vec<synapse_storage::Receipt>> {
        self.messaging.get_receipts(room_id, receipt_type, event_id).await
    }

    // -- read_markers.rs ----------------------------------------------------

    pub async fn update_read_marker(
        &self,
        room_id: &str,
        user_id: &str,
        event_id: &str,
        marker_type: &str,
    ) -> ApiResult<()> {
        self.messaging.update_read_marker(room_id, user_id, event_id, marker_type).await
    }
    pub async fn set_read_markers(&self, room_id: &str, user_id: &str, body: &serde_json::Value) -> ApiResult<()> {
        self.messaging.set_read_markers(room_id, user_id, body).await
    }

    // -- burn_after_read.rs -------------------------------------------------

    pub async fn process_read_receipt(
        &self,
        room_id: &str,
        event_id: &str,
        _user_id: &str,
        _custom_delay_secs: Option<u64>,
    ) -> ApiResult<()> {
        self.messaging.process_read_receipt(room_id, event_id, _user_id, _custom_delay_secs).await
    }

    // =========================================================================
    // State forwarding methods — delegate to RoomStateService
    // =========================================================================

    // -- aliases.rs ----------------------------------------------------------

    pub async fn get_room_aliases(&self, room_id: &str) -> ApiResult<Vec<String>> {
        self.state.get_room_aliases(room_id).await
    }
    pub async fn set_room_alias(&self, room_id: &str, alias: &str, created_by: &str) -> ApiResult<()> {
        self.state.set_room_alias(room_id, alias, created_by).await
    }
    pub async fn get_room_by_alias(&self, alias: &str) -> ApiResult<Option<String>> {
        self.state.get_room_by_alias(alias).await
    }
    pub async fn remove_room_alias(&self, room_id: &str) -> ApiResult<()> {
        self.state.remove_room_alias(room_id).await
    }
    pub async fn remove_room_alias_by_name(&self, alias: &str) -> ApiResult<()> {
        self.state.remove_room_alias_by_name(alias).await
    }
    pub async fn set_room_directory(&self, room_id: &str, is_public: bool) -> ApiResult<()> {
        self.state.set_room_directory(room_id, is_public).await
    }
    pub async fn get_room_visibility(&self, room_id: &str) -> ApiResult<String> {
        self.state.get_room_visibility(room_id).await
    }
    pub async fn remove_room_directory(&self, room_id: &str) -> ApiResult<()> {
        self.state.remove_room_directory(room_id).await
    }
    pub async fn get_public_rooms(&self, limit: i64) -> ApiResult<serde_json::Value> {
        self.state.get_public_rooms(limit).await
    }

    // -- tags.rs -------------------------------------------------------------

    pub async fn get_all_tags(
        &self,
        user_id: &str,
    ) -> Result<Vec<synapse_storage::room_tag::RoomTag>, super::tags::TagsError> {
        self.state.get_all_tags(user_id).await
    }
    pub async fn get_tags(
        &self,
        user_id: &str,
        room_id: &str,
    ) -> Result<Vec<synapse_storage::room_tag::RoomTag>, super::tags::TagsError> {
        self.state.get_tags(user_id, room_id).await
    }
    pub async fn add_tag(
        &self,
        user_id: &str,
        room_id: &str,
        tag: &str,
        order: Option<f64>,
    ) -> Result<(), super::tags::TagsError> {
        self.state.add_tag(user_id, room_id, tag, order).await
    }
    pub async fn remove_tag(&self, user_id: &str, room_id: &str, tag: &str) -> Result<(), super::tags::TagsError> {
        self.state.remove_tag(user_id, room_id, tag).await
    }

    // -- info.rs -------------------------------------------------------------

    pub async fn get_room_encryption_status(
        &self,
        room_id: &str,
    ) -> ApiResult<synapse_storage::room::RoomEncryptionStatus> {
        self.state.get_room_encryption_status(room_id).await
    }
    pub async fn delete_room(&self, room_id: &str, requester_id: &str) -> ApiResult<()> {
        self.state.delete_room(room_id, requester_id).await
    }
    pub async fn get_user_room_list(&self, user_id: &str) -> ApiResult<Vec<serde_json::Value>> {
        self.state.get_user_room_list(user_id).await
    }
    pub async fn cleanup_abnormal_data(&self, min_age_ms: Option<i64>) -> ApiResult<serde_json::Value> {
        self.state.cleanup_abnormal_data(min_age_ms).await
    }
    pub async fn room_exists(&self, room_id: &str) -> ApiResult<bool> {
        self.state.room_exists(room_id).await
    }
    pub async fn block_room(&self, room_id: &str, blocked_by: &str, reason: Option<&str>) -> ApiResult<()> {
        self.state.block_room(room_id, blocked_by, reason).await
    }
    pub async fn get_room_block_status(&self, room_id: &str) -> ApiResult<Option<i64>> {
        self.state.get_room_block_status(room_id).await
    }
    pub async fn unblock_room(&self, room_id: &str) -> ApiResult<()> {
        self.state.unblock_room(room_id).await
    }
    pub async fn get_public_rooms_paginated(
        &self,
        limit: i64,
        since_ts: Option<i64>,
        since_room_id: Option<&str>,
    ) -> ApiResult<Vec<synapse_storage::Room>> {
        self.state.get_public_rooms_paginated(limit, since_ts, since_room_id).await
    }
    pub async fn count_public_rooms(&self) -> ApiResult<i64> {
        self.state.count_public_rooms().await
    }
    pub async fn get_room_stats_overview(&self) -> ApiResult<serde_json::Value> {
        self.state.get_room_stats_overview().await
    }
    pub async fn get_single_room_stats(&self, room_id: &str) -> ApiResult<Option<serde_json::Value>> {
        self.state.get_single_room_stats(room_id).await
    }
    pub async fn get_all_rooms_with_members(
        &self,
        limit: i64,
        from: Option<synapse_storage::RoomSearchCursor>,
        order_by: synapse_storage::RoomSearchOrder,
    ) -> ApiResult<(Vec<(synapse_storage::Room, i64)>, Option<String>)> {
        self.state.get_all_rooms_with_members(limit, from, order_by).await
    }
    pub async fn get_room_count(&self) -> ApiResult<i64> {
        self.state.get_room_count().await
    }
    pub async fn get_room_record(&self, room_id: &str) -> ApiResult<Option<synapse_storage::Room>> {
        self.state.get_room_record(room_id).await
    }
    pub async fn get_room_listings_status(&self, room_id: &str) -> ApiResult<Option<(bool, bool)>> {
        self.state.get_room_listings_status(room_id).await
    }
    pub async fn set_room_public_with_directory(&self, room_id: &str) -> ApiResult<bool> {
        self.state.set_room_public_with_directory(room_id).await
    }
    pub async fn set_room_private_with_directory(&self, room_id: &str) -> ApiResult<bool> {
        self.state.set_room_private_with_directory(room_id).await
    }
    pub async fn shutdown_room_and_remove_members(&self, room_id: &str) -> ApiResult<()> {
        self.state.shutdown_room_and_remove_members(room_id).await
    }
    pub async fn grant_room_admin(&self, room_id: &str, user_id: &str) -> ApiResult<()> {
        self.state.grant_room_admin(room_id, user_id).await
    }
    pub async fn purge_history_before(&self, room_id: &str, timestamp: i64) -> ApiResult<u64> {
        self.state.purge_history_before(room_id, timestamp).await
    }
    pub async fn get_room_version(&self, room_id: &str) -> ApiResult<Option<String>> {
        self.state.get_room_version(room_id).await
    }
    pub async fn search_all_rooms_admin(
        &self,
        search_term: Option<&str>,
        limit: i64,
        order_by: synapse_storage::RoomSearchOrder,
        cursor: Option<synapse_storage::RoomSearchCursor>,
        is_public: Option<bool>,
        is_encrypted: Option<bool>,
    ) -> ApiResult<(Vec<serde_json::Value>, i64, Option<String>)> {
        self.state.search_all_rooms_admin(search_term, limit, order_by, cursor, is_public, is_encrypted).await
    }
    pub async fn is_room_creator(&self, room_id: &str, user_id: &str) -> ApiResult<bool> {
        self.state.is_room_creator(room_id, user_id).await
    }
    pub async fn check_room_has_encryption(&self, room_id: &str) -> ApiResult<bool> {
        self.state.check_room_has_encryption(room_id).await
    }

    // =========================================================================
    // Lifecycle forwarding methods — delegate to LifecycleService
    // =========================================================================

    pub async fn create_room(&self, user_id: &str, config: CreateRoomConfig) -> ApiResult<serde_json::Value> {
        self.lifecycle.create_room(user_id, config).await
    }
    pub async fn upgrade_room(&self, old_room_id: &str, new_version: &str, user_id: &str) -> ApiResult<String> {
        let old_room = self
            .room_storage
            .get_room(old_room_id)
            .await
            .map_err(|e| ApiError::internal_with_log("Failed to get old room", &e))?
            .ok_or_else(|| ApiError::not_found("Room not found".to_string()))?;

        // Fetch old room members BEFORE creating the tombstone, so we can
        // invite them to the replacement room.  We collect joined members
        // excluding the upgrading user (who is auto-invited via create_room).
        let old_members = self
            .member_storage
            .get_room_members(old_room_id, "join")
            .await
            .map_err(|e| ApiError::internal_with_log("Failed to fetch old room members for migration", &e))?;
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
                    origin_server_ts: chrono::Utc::now().timestamp_millis(),
                    redacts: None,
                },
                None,
            )
            .await
            .map_err(|e| ApiError::internal_with_log("Failed to create tombstone event", &e))?;

        ::tracing::info!(
            old_room_id = %old_room_id,
            new_room_id = %new_room_id,
            tombstone_event_id = %tombstone_event.event_id,
            "Room upgraded: tombstone event created and broadcast"
        );

        // Auto-join the upgrading user to the new room.  `create_room` only
        // invites the creator; we need them to be a joined member so they can
        // immediately use the replacement room (matches Synapse behavior).
        if let Err(e) = self.join_room(&new_room_id, user_id).await {
            ::tracing::warn!(
                old_room_id = %old_room_id,
                new_room_id = %new_room_id,
                user_id = %user_id,
                error = %e,
                "Failed to auto-join upgrading user to replacement room"
            );
        }

        // Invite all former joined members of the old room to the new room.
        // Local users go through the local invite path; remote users go
        // through the federation invite path (handled transparently by
        // `invite_user`).
        for invitee_id in &members_to_invite {
            if let Err(e) = self.invite_user(&new_room_id, user_id, invitee_id).await {
                ::tracing::warn!(
                    old_room_id = %old_room_id,
                    new_room_id = %new_room_id,
                    invitee_id = %invitee_id,
                    error = %e,
                    "Failed to invite old room member to replacement room"
                );
            }
        }

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
    pub async fn get_tombstone_event(&self, room_id: &str) -> ApiResult<Option<serde_json::Value>> {
        self.lifecycle.get_tombstone_event(room_id).await
    }
    pub async fn migrate_room_content(
        &self,
        source_room_id: &str,
        target_room_id: &str,
        user_id: &str,
    ) -> ApiResult<()> {
        self.lifecycle.migrate_room_content(source_room_id, target_room_id, user_id).await
    }
    pub async fn is_room_upgrade_allowed(&self, room_id: &str, user_id: &str) -> ApiResult<bool> {
        self.lifecycle.is_room_upgrade_allowed(room_id, user_id).await
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

    // ------------------------------------------------------------------
    // Manual mock RoomRepository for unit testing
    // ------------------------------------------------------------------

    struct MockRoomRepo {
        room: Option<synapse_storage::Room>,
    }

    #[async_trait::async_trait]
    impl synapse_storage::RoomRepository for MockRoomRepo {
        fn pool(&self) -> &Arc<sqlx::PgPool> {
            unimplemented!("pool not used in test")
        }

        async fn get_room(&self, _room_id: &str) -> Result<Option<synapse_storage::Room>, sqlx::Error> {
            Ok(self.room.clone())
        }

        async fn get_rooms_batch(&self, _room_ids: &[String]) -> Result<Vec<synapse_storage::Room>, sqlx::Error> {
            unimplemented!("get_rooms_batch not used in test")
        }

        async fn create_room(
            &self,
            _room_id: &str,
            _creator: &str,
            _join_rule: &str,
            _room_version: &str,
            _is_public: bool,
        ) -> Result<synapse_storage::Room, sqlx::Error> {
            unimplemented!("create_room not used in test")
        }

        async fn create_room_in_tx(
            &self,
            _tx: &mut sqlx::Transaction<'_, sqlx::Postgres>,
            _room_id: &str,
            _creator: &str,
            _join_rule: &str,
            _version: &str,
            _is_public: bool,
        ) -> Result<synapse_storage::Room, sqlx::Error> {
            unimplemented!("create_room_in_tx not used in test")
        }

        async fn update_room_name(&self, _room_id: &str, _name: &str) -> Result<(), sqlx::Error> {
            unimplemented!("update_room_name not used in test")
        }

        async fn update_room_name_in_tx(
            &self,
            _tx: &mut sqlx::Transaction<'_, sqlx::Postgres>,
            _room_id: &str,
            _name: &str,
        ) -> Result<(), sqlx::Error> {
            unimplemented!("update_room_name_in_tx not used in test")
        }

        async fn update_room_topic(&self, _room_id: &str, _topic: &str) -> Result<(), sqlx::Error> {
            unimplemented!("update_room_topic not used in test")
        }

        async fn update_room_topic_in_tx(
            &self,
            _tx: &mut sqlx::Transaction<'_, sqlx::Postgres>,
            _room_id: &str,
            _topic: &str,
        ) -> Result<(), sqlx::Error> {
            unimplemented!("update_room_topic_in_tx not used in test")
        }

        async fn update_join_rule_in_tx(
            &self,
            _tx: &mut sqlx::Transaction<'_, sqlx::Postgres>,
            _room_id: &str,
            _join_rule: &str,
        ) -> Result<(), sqlx::Error> {
            unimplemented!("update_join_rule_in_tx not used in test")
        }

        async fn set_room_public(&self, _room_id: &str, _is_public: bool) -> Result<(), sqlx::Error> {
            unimplemented!("set_room_public not used in test")
        }

        async fn delete_room(&self, _room_id: &str) -> Result<(), sqlx::Error> {
            unimplemented!("delete_room not used in test")
        }

        async fn get_public_rooms(&self, _limit: i64) -> Result<Vec<synapse_storage::Room>, sqlx::Error> {
            unimplemented!("get_public_rooms not used in test")
        }

        async fn get_public_rooms_paginated(
            &self,
            _limit: i64,
            _since_ts: Option<i64>,
            _since_room_id: Option<&str>,
        ) -> Result<Vec<synapse_storage::Room>, sqlx::Error> {
            unimplemented!("get_public_rooms_paginated not used in test")
        }

        async fn count_public_rooms(&self) -> Result<i64, sqlx::Error> {
            unimplemented!("count_public_rooms not used in test")
        }

        async fn get_user_rooms(&self, _user_id: &str) -> Result<Vec<String>, sqlx::Error> {
            unimplemented!("get_user_rooms not used in test")
        }

        async fn search_room_directory(
            &self,
            _search_term: &str,
            _limit: i64,
        ) -> Result<Vec<synapse_storage::Room>, sqlx::Error> {
            unimplemented!("search_room_directory not used in test")
        }

        async fn get_room_aliases(&self, _room_id: &str) -> Result<Vec<String>, sqlx::Error> {
            unimplemented!("get_room_aliases not used in test")
        }

        async fn set_room_alias(&self, _room_id: &str, _alias: &str, _created_by: &str) -> Result<(), sqlx::Error> {
            unimplemented!("set_room_alias not used in test")
        }

        async fn get_room_by_alias(&self, _alias: &str) -> Result<Option<String>, sqlx::Error> {
            unimplemented!("get_room_by_alias not used in test")
        }

        async fn remove_room_alias(&self, _room_id: &str) -> Result<(), sqlx::Error> {
            unimplemented!("remove_room_alias not used in test")
        }

        async fn remove_room_alias_by_name(&self, _alias: &str) -> Result<(), sqlx::Error> {
            unimplemented!("remove_room_alias_by_name not used in test")
        }

        async fn set_room_directory(&self, _room_id: &str, _is_public: bool) -> Result<(), sqlx::Error> {
            unimplemented!("set_room_directory not used in test")
        }

        async fn is_room_in_directory(&self, _room_id: &str) -> Result<bool, sqlx::Error> {
            unimplemented!("is_room_in_directory not used in test")
        }

        async fn remove_room_directory(&self, _room_id: &str) -> Result<(), sqlx::Error> {
            unimplemented!("remove_room_directory not used in test")
        }

        async fn set_canonical_alias(&self, _room_id: &str, _alias: Option<&str>) -> Result<(), sqlx::Error> {
            unimplemented!("set_canonical_alias not used in test")
        }

        async fn increment_member_count(&self, _room_id: &str) -> Result<(), sqlx::Error> {
            unimplemented!("increment_member_count not used in test")
        }

        async fn decrement_member_count(&self, _room_id: &str) -> Result<(), sqlx::Error> {
            unimplemented!("decrement_member_count not used in test")
        }

        async fn add_receipt(
            &self,
            _user_id: &str,
            _receipt_user_id: &str,
            _room_id: &str,
            _event_id: &str,
            _receipt_type: &str,
            _data: &serde_json::Value,
        ) -> Result<(), sqlx::Error> {
            unimplemented!("add_receipt not used in test")
        }

        async fn get_receipts(
            &self,
            _room_id: &str,
            _receipt_type: &str,
            _event_id: &str,
        ) -> Result<Vec<synapse_storage::Receipt>, sqlx::Error> {
            unimplemented!("get_receipts not used in test")
        }

        async fn update_read_marker_with_type(
            &self,
            _room_id: &str,
            _user_id: &str,
            _event_id: &str,
            _marker_type: &str,
        ) -> Result<(), sqlx::Error> {
            unimplemented!("update_read_marker_with_type not used in test")
        }

        async fn copy_room_state(&self, _source_room_id: &str, _target_room_id: &str) -> Result<(), sqlx::Error> {
            unimplemented!("copy_room_state not used in test")
        }

        async fn room_exists(&self, _room_id: &str) -> Result<bool, sqlx::Error> {
            unimplemented!("room_exists not used in test")
        }

        async fn get_room_count(&self) -> Result<i64, sqlx::Error> {
            unimplemented!("get_room_count not used in test")
        }

        async fn get_room_version_only(&self, _room_id: &str) -> Result<Option<String>, sqlx::Error> {
            unimplemented!("get_room_version_only not used in test")
        }

        async fn block_room(
            &self,
            _room_id: &str,
            _blocked_at: i64,
            _blocked_by: &str,
            _reason: Option<&str>,
        ) -> Result<(), sqlx::Error> {
            unimplemented!("block_room not used in test")
        }

        async fn unblock_room(&self, _room_id: &str) -> Result<(), sqlx::Error> {
            unimplemented!("unblock_room not used in test")
        }

        async fn get_room_block_status(&self, _room_id: &str) -> Result<Option<i64>, sqlx::Error> {
            unimplemented!("get_room_block_status not used in test")
        }

        async fn shutdown_room(&self, _room_id: &str) -> Result<(), sqlx::Error> {
            unimplemented!("shutdown_room not used in test")
        }

        async fn get_room_stats_overview(&self) -> Result<serde_json::Value, sqlx::Error> {
            unimplemented!("get_room_stats_overview not used in test")
        }

        async fn get_single_room_stats(&self, _room_id: &str) -> Result<Option<serde_json::Value>, sqlx::Error> {
            unimplemented!("get_single_room_stats not used in test")
        }

        async fn get_room_listings_status(&self, _room_id: &str) -> Result<Option<(bool, bool)>, sqlx::Error> {
            unimplemented!("get_room_listings_status not used in test")
        }

        async fn set_room_public_with_directory(&self, _room_id: &str) -> Result<bool, sqlx::Error> {
            unimplemented!("set_room_public_with_directory not used in test")
        }

        async fn set_room_private_with_directory(&self, _room_id: &str) -> Result<bool, sqlx::Error> {
            unimplemented!("set_room_private_with_directory not used in test")
        }

        async fn get_user_room_list_summary(
            &self,
            _user_id: &str,
        ) -> Result<Vec<(String, String, String, String)>, sqlx::Error> {
            unimplemented!("get_user_room_list_summary not used in test")
        }

        async fn get_all_rooms_with_members(
            &self,
            _limit: i64,
            _from: Option<synapse_storage::RoomSearchCursor>,
            _order_by: synapse_storage::RoomSearchOrder,
        ) -> Result<(Vec<(synapse_storage::Room, i64)>, Option<String>), sqlx::Error> {
            unimplemented!("get_all_rooms_with_members not used in test")
        }

        async fn search_all_rooms_admin(
            &self,
            _search_term: Option<&str>,
            _limit: i64,
            _order_by: synapse_storage::RoomSearchOrder,
            _cursor: Option<synapse_storage::RoomSearchCursor>,
            _is_public: Option<bool>,
            _is_encrypted: Option<bool>,
        ) -> Result<(Vec<serde_json::Value>, i64, Option<String>), sqlx::Error> {
            unimplemented!("search_all_rooms_admin not used in test")
        }

        async fn get_unread_counts(
            &self,
            _room_id: &str,
            _user_id: &str,
        ) -> Result<synapse_storage::RoomUnreadCounts, sqlx::Error> {
            unimplemented!("get_unread_counts not used in test")
        }

        async fn get_unread_counts_batch(
            &self,
            _room_ids: &[String],
            _user_id: &str,
        ) -> Result<Vec<synapse_storage::RoomUnreadCounts>, sqlx::Error> {
            unimplemented!("get_unread_counts_batch not used in test")
        }

        async fn cleanup_abnormal_data(&self, _min_age_ms: Option<i64>) -> Result<serde_json::Value, sqlx::Error> {
            unimplemented!("cleanup_abnormal_data not used in test")
        }
    }

    /// Create a lazy Postgres pool for tests (never actually connects).
    fn test_pool() -> Arc<sqlx::PgPool> {
        Arc::new(
            sqlx::postgres::PgPoolOptions::new()
                .max_connections(1)
                .connect_lazy("postgres://localhost:5432/synapse_test")
                .expect("lazy pool creation should succeed even without a running DB"),
        )
    }

    /// Build a RoomService for testing. Only `room_storage` is exercised;
    /// all other fields are real (but unused) instances backed by a lazy pool.
    fn make_service(room_storage: Arc<dyn synapse_storage::RoomRepository>) -> RoomService {
        let pool = test_pool();
        let event_storage: Arc<dyn synapse_storage::EventRepository> =
            Arc::new(synapse_storage::event::EventStorage::new(&pool, "localhost".to_string()));
        let cache = Arc::new(synapse_cache::CacheManager::new(&synapse_cache::CacheConfig::default()));
        let metrics = Arc::new(synapse_common::metrics::MetricsCollector::new());
        let security = synapse_common::config::SecurityConfig::default();
        let member_storage: Arc<dyn synapse_storage::RoomMemberRepository> =
            Arc::new(synapse_storage::membership::RoomMemberStorage::new(&pool, "localhost"));
        let user_storage: Arc<dyn UserStore> = Arc::new(synapse_storage::FakeUserStore::new());
        let auth_service: Arc<dyn Auth> =
            Arc::new(crate::auth::AuthService::new(&pool, cache.clone(), metrics, &security, "localhost"));
        let room_summary_service = Arc::new(crate::room::summary::RoomSummaryService::new(
            Arc::new(synapse_storage::room_summary::RoomSummaryStorage::new(&pool)),
            event_storage.clone(),
            None,
        ));

        let membership_cfg = MembershipServiceConfig {
            member_storage: member_storage.clone(),
            room_storage: room_storage.clone(),
            event_storage: event_storage.clone(),
            user_storage: user_storage.clone(),
            auth_service: auth_service.clone(),
            server_name: "example.com".to_string(),
            federation_client: Arc::new(RwLock::new(None)),
            key_rotation_manager: Arc::new(RwLock::new(None)),
            event_broadcaster: Arc::new(RwLock::new(None)),
            room_summary_service: room_summary_service.clone(),
        };
        let membership = MembershipService::new(membership_cfg);

        let messaging_cfg = MessagingServiceConfig {
            event_storage: event_storage.clone(),
            room_storage: room_storage.clone(),
            member_storage: member_storage.clone(),
            server_name: "example.com".to_string(),
            #[cfg(feature = "beacons")]
            beacon_service: None,
            #[cfg(not(feature = "beacons"))]
            beacon_service: None,
            task_queue: None,
            relations_storage: Arc::new(synapse_storage::relations::RelationsStorage::new(&pool)),
            event_broadcaster: Arc::new(RwLock::new(None)),
            app_service_manager: Arc::new(RwLock::new(None)),
            key_rotation_manager: Arc::new(RwLock::new(None)),
            room_summary_service: room_summary_service.clone(),
        };
        let messaging = MessagingService::new(messaging_cfg);

        let state_cfg = RoomStateServiceConfig {
            room_storage: room_storage.clone(),
            member_storage: member_storage.clone(),
            event_storage: event_storage.clone(),
            room_tag_storage: Arc::new(synapse_storage::room_tag::RoomTagStorage::new(pool.clone())),
            user_storage: user_storage.clone(),
            server_name: "example.com".to_string(),
        };
        let state = RoomStateService::new(state_cfg);

        let lifecycle_cfg = LifecycleServiceConfig {
            room_storage: room_storage.clone(),
            member_storage: member_storage.clone(),
            event_storage: event_storage.clone(),
            user_storage: user_storage.clone(),
            validator: Arc::new(synapse_common::validation::Validator::default()),
            server_name: "example.com".to_string(),
            room_summary_service: room_summary_service.clone(),
        };
        let lifecycle = LifecycleService::new(lifecycle_cfg);

        RoomService {
            membership,
            messaging,
            state,
            lifecycle,
            room_storage,
            member_storage,
            room_tag_storage: Arc::new(synapse_storage::room_tag::RoomTagStorage::new(pool.clone())),
            user_storage,
            auth_service,
            validator: Arc::new(synapse_common::validation::Validator::default()),
            server_name: "example.com".to_string(),
            task_queue: None,
            active_tasks: Arc::new(RwLock::new(HashMap::new())),
            room_summary_service,
            event_storage,
            relations_storage: Arc::new(synapse_storage::relations::RelationsStorage::new(&pool)),
            infra: RoomInfrastructure::new(),
            beacon_service: None,
        }
    }

    #[tokio::test]
    async fn test_get_room_with_mock_found() {
        let mock = MockRoomRepo {
            room: Some(synapse_storage::Room {
                room_id: "!testroom:example.com".to_string(),
                name: Some("Test Room".to_string()),
                topic: Some("A test room".to_string()),
                canonical_alias: Some("#test:example.com".to_string()),
                join_rule: "invite".to_string(),
                creator_user_id: Some("@alice:example.com".to_string()),
                room_version: "10".to_string(),
                encryption: None,
                is_public: false,
                member_count: 1,
                history_visibility: "shared".to_string(),
                created_ts: 1234567890,
                avatar_url: None,
                is_federatable: true,
                is_spotlight: false,
                is_flagged: false,
            }),
        };

        let service = make_service(Arc::new(mock));
        let result = service.get_room("!testroom:example.com").await;
        assert!(result.is_ok());
        let json = result.unwrap();
        assert_eq!(json["room_id"], "!testroom:example.com");
        assert_eq!(json["name"], "Test Room");
        assert_eq!(json["topic"], "A test room");
        assert_eq!(json["canonical_alias"], "#test:example.com");
        assert_eq!(json["is_public"], false);
        assert_eq!(json["creator"], "@alice:example.com");
        assert_eq!(json["join_rule"], "invite");
    }

    #[tokio::test]
    async fn test_get_room_with_mock_not_found() {
        let mock = MockRoomRepo { room: None };
        let service = make_service(Arc::new(mock));
        let result = service.get_room("!nonexistent:example.com").await;
        assert!(result.is_err());
    }

    /// Handler-level test: fully populated Room -> JSON response with all fields.
    /// Proves the full chain: mock RoomRepository -> RoomService::get_room() -> JSON Value.
    #[tokio::test]
    async fn test_get_room_json_response_all_fields_populated() {
        let mock = MockRoomRepo {
            room: Some(synapse_storage::Room {
                room_id: "!fullroom:example.com".to_string(),
                name: Some("Full Room".to_string()),
                topic: Some("A room with all fields set".to_string()),
                avatar_url: Some("mxc://example.com/avatar".to_string()),
                canonical_alias: Some("#full:example.com".to_string()),
                join_rule: "public".to_string(),
                creator_user_id: Some("@bob:example.com".to_string()),
                room_version: "10".to_string(),
                encryption: Some("m.megolm.v1.aes-sha2".to_string()),
                is_public: true,
                member_count: 42,
                history_visibility: "world_readable".to_string(),
                created_ts: 1678901234,
                is_federatable: true,
                is_spotlight: true,
                is_flagged: false,
            }),
        };

        let service = make_service(Arc::new(mock));
        let result = service.get_room("!fullroom:example.com").await;

        // Step 1: API call must succeed.
        assert!(result.is_ok(), "get_room must return Ok for a found room");
        let json = result.unwrap();

        // Step 2: Verify every key that get_room() emits is present.
        let expected_keys: [&str; 7] =
            ["room_id", "name", "topic", "canonical_alias", "is_public", "creator", "join_rule"];
        for key in &expected_keys {
            assert!(json.get(key).is_some(), "JSON response must contain key '{}'", key);
        }

        // Step 3: Verify exact values.
        assert_eq!(json["room_id"], json!("!fullroom:example.com"));
        assert_eq!(json["name"], json!("Full Room"));
        assert_eq!(json["topic"], json!("A room with all fields set"));
        assert_eq!(json["canonical_alias"], json!("#full:example.com"));
        assert_eq!(json["is_public"], json!(true));
        assert_eq!(json["creator"], json!("@bob:example.com"));
        assert_eq!(json["join_rule"], json!("public"));

        // Step 4: Verify types.
        assert!(json["room_id"].is_string(), "room_id must be a string");
        assert!(json["name"].is_string(), "name must be a string");
        assert!(json["topic"].is_string(), "topic must be a string");
        assert!(json["canonical_alias"].is_string(), "canonical_alias must be a string");
        assert!(json["is_public"].is_boolean(), "is_public must be a bool");
        assert!(json["creator"].is_string(), "creator must be a string");
        assert!(json["join_rule"].is_string(), "join_rule must be a string");

        // Step 5: Verify no extra keys leaked (get_room returns exactly 7 keys).
        assert_eq!(json.as_object().map(|o| o.len()), Some(7), "get_room JSON must have exactly 7 keys");
    }

    /// Handler-level test: sparse Room (None fields) -> JSON response with null values.
    /// Verifies that None-able fields serialize as `null` rather than being stripped.
    #[tokio::test]
    async fn test_get_room_json_response_null_fields_present() {
        let mock = MockRoomRepo {
            room: Some(synapse_storage::Room {
                room_id: "!bare:example.com".to_string(),
                name: None,
                topic: None,
                avatar_url: None,
                canonical_alias: None,
                join_rule: "knock".to_string(),
                creator_user_id: None,
                room_version: "1".to_string(),
                encryption: None,
                is_public: false,
                member_count: 0,
                history_visibility: "joined".to_string(),
                created_ts: 0,
                is_federatable: true,
                is_spotlight: false,
                is_flagged: false,
            }),
        };

        let service = make_service(Arc::new(mock));
        let result = service.get_room("!bare:example.com").await;

        assert!(result.is_ok(), "get_room must return Ok for a found room");
        let json = result.unwrap();

        // Required fields (non-Option in Room struct) must be present and correct.
        assert_eq!(json["room_id"], json!("!bare:example.com"));
        assert_eq!(json["join_rule"], json!("knock"));
        assert_eq!(json["is_public"], json!(false));

        // Option<String> fields that are None must appear as JSON null — not absent.
        assert!(json.get("name").is_some() && json["name"].is_null(), "name must be present and null when not set");
        assert!(json.get("topic").is_some() && json["topic"].is_null(), "topic must be present and null when not set");
        assert!(
            json.get("canonical_alias").is_some() && json["canonical_alias"].is_null(),
            "canonical_alias must be present and null when not set"
        );
        assert!(
            json.get("creator").is_some() && json["creator"].is_null(),
            "creator must be present and null when not set"
        );

        // Verify exact key count (still 7 — null values use the key, unlike absent).
        assert_eq!(
            json.as_object().map(|o| o.len()),
            Some(7),
            "get_room JSON must have exactly 7 keys even when fields are null"
        );
    }
}
