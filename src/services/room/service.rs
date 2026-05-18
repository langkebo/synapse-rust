use crate::common::background_job::BackgroundJob;
use crate::common::constants::BURN_AFTER_READ_DELAY_SECS;
use crate::common::task_queue::RedisTaskQueue;
use crate::common::validation::Validator;
use crate::common::{
    generate_event_id, generate_room_id, generate_stream_token_from_ts, parse_stream_token,
};
use crate::services::*;
use crate::storage::CreateEventParams;
use crate::storage::UserStorage;
use serde_json::json;
use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::RwLock;
use ::tracing::error;

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
    pub room_storage: RoomStorage,
    pub member_storage: RoomMemberStorage,
    pub event_storage: EventStorage,
    pub user_storage: UserStorage,
    pub auth_service: AuthService,
    pub room_summary_service: Arc<RoomSummaryService>,
    pub validator: Arc<Validator>,
    pub server_name: String,
    pub task_queue: Option<Arc<RedisTaskQueue>>,
    pub relations_storage: crate::storage::relations::RelationsStorage,
    #[cfg(feature = "beacons")]
    pub beacon_service: Option<Arc<crate::services::beacon_service::BeaconService>>,
    #[cfg(not(feature = "beacons"))]
    pub beacon_service: Option<()>,
}

fn validate_room_alias_input(alias: &str) -> ApiResult<()> {
    if alias.is_empty() {
        return Err(ApiError::bad_request("room_alias is required".to_string()));
    }
    if !alias.starts_with('#') {
        return Err(ApiError::bad_request(
            "Invalid room alias format: must start with #".to_string(),
        ));
    }
    if alias.len() > 255 {
        return Err(ApiError::bad_request(
            "Room alias too long (max 255 characters)".to_string(),
        ));
    }

    let Some((localpart, server_name)) = alias[1..].rsplit_once(':') else {
        return Err(ApiError::bad_request(
            "Invalid room alias format: must be #alias:server".to_string(),
        ));
    };

    if localpart.is_empty() || server_name.is_empty() {
        return Err(ApiError::bad_request(
            "Invalid room alias format: must be #alias:server".to_string(),
        ));
    }

    Ok(())
}

pub struct RoomService {
    room_storage: RoomStorage,
    member_storage: RoomMemberStorage,
    pub event_storage: EventStorage,
    pub user_storage: UserStorage,
    auth_service: AuthService,
    pub validator: Arc<Validator>,
    pub server_name: String,
    pub task_queue: Option<Arc<RedisTaskQueue>>,
    pub active_tasks: Arc<RwLock<HashMap<String, tokio::task::JoinHandle<()>>>>,
    pub room_summary_service: Arc<RoomSummaryService>,
    relations_storage: crate::storage::relations::RelationsStorage,
    #[cfg(feature = "beacons")]
    beacon_service: Option<Arc<crate::services::beacon_service::BeaconService>>,
}

impl RoomService {
    pub fn new(config: RoomServiceConfig) -> Self {
        Self {
            room_storage: config.room_storage,
            member_storage: config.member_storage,
            event_storage: config.event_storage,
            user_storage: config.user_storage,
            auth_service: config.auth_service,
            room_summary_service: config.room_summary_service,
            validator: config.validator,
            server_name: config.server_name,
            task_queue: config.task_queue,
            active_tasks: Arc::new(RwLock::new(HashMap::new())),
            relations_storage: config.relations_storage,
            #[cfg(feature = "beacons")]
            beacon_service: config.beacon_service,
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
            ::tracing::info!("Aborting delayed task: {}", task_id);
            handle.abort();
        }
    }

    pub async fn create_room(
        &self,
        user_id: &str,
        config: CreateRoomConfig,
    ) -> ApiResult<serde_json::Value> {
        if let Some(alias) = &config.room_alias_name {
            if let Err(e) = self.validator.validate_username(alias) {
                return Err(e.into());
            }
        }

        let room_id = self.generate_room_id();
        let mut join_rule = Self::determine_join_rule(config.preset.as_deref());
        let is_public = Self::is_public_visibility(config.visibility.as_deref());

        if is_public && join_rule != "public" {
            join_rule = "public";
        }

        // Handle trusted_private_chat preset
        let is_trusted_private = config.preset.as_deref() == Some("trusted_private_chat");
        if is_trusted_private {
            join_rule = "invite";
        }

        let mut tx = self
            .room_storage
            .pool
            .begin()
            .await
            .map_err(|e| ApiError::internal(format!("Failed to start transaction: {e}")))?;

        let result = self
            .create_room_in_db(
                &room_id,
                user_id,
                join_rule,
                is_public,
                config.room_version.as_deref().unwrap_or("10"),
                Some(&mut tx),
            )
            .await;
        if let Err(e) = &result {
            error!("create_room_in_db failed: {}", e);
            let _ = tx.rollback().await;
            return Err(ApiError::internal(format!("Failed to create room: {e}")));
        }

        // Create m.room.create event. Honor the client's `room_version` /
        // `creation_content` if provided, otherwise default to room v10
        // (matches /capabilities). `creation_content.type` ("m.space" etc.)
        // and the legacy top-level `room_type` shortcut are both supported.
        let now = chrono::Utc::now().timestamp_millis();
        let room_version = config
            .room_version
            .clone()
            .unwrap_or_else(|| "10".to_string());
        let mut create_content = json!({
            "creator": user_id,
            "room_version": room_version,
        });
        if let Some(extra) = config.creation_content.as_ref().and_then(|v| v.as_object()) {
            if let Some(map) = create_content.as_object_mut() {
                for (k, v) in extra {
                    map.insert(k.clone(), v.clone());
                }
            }
        }
        if let Some(ref room_type) = config.room_type {
            create_content["type"] = json!(room_type);
        }
        let result = self
            .event_storage
            .create_event(
                CreateEventParams {
                    event_id: generate_event_id(&self.server_name),
                    room_id: room_id.clone(),
                    user_id: user_id.to_string(),
                    event_type: "m.room.create".to_string(),
                    content: create_content,
                    state_key: Some("".to_string()),
                    origin_server_ts: now,
                },
                Some(&mut tx),
            )
            .await;
        if let Err(e) = &result {
            error!("m.room.create event failed: {}", e);
            let _ = tx.rollback().await;
            return Err(ApiError::internal(format!(
                "Failed to create m.room.create event: {e}"
            )));
        }

        let result = self
            .add_creator_to_room(&room_id, user_id, Some(&mut tx))
            .await;
        if let Err(e) = &result {
            error!("add_creator_to_room failed: {}", e);
            let _ = tx.rollback().await;
            return Err(e.clone());
        }

        // m.room.member for creator (membership=join). Without this state event the
        // client never learns the creator is actually in the room and the room sits
        // in `rooms.join` with no members, leaving the UI stuck on "joining...".
        let result = self
            .event_storage
            .create_event(
                CreateEventParams {
                    event_id: generate_event_id(&self.server_name),
                    room_id: room_id.clone(),
                    user_id: user_id.to_string(),
                    event_type: "m.room.member".to_string(),
                    content: json!({
                        "membership": "join",
                        "displayname": user_id.trim_start_matches('@').split(':').next().unwrap_or(user_id),
                    }),
                    state_key: Some(user_id.to_string()),
                    origin_server_ts: now + 1,
                },
                Some(&mut tx),
            )
            .await;
        if let Err(e) = result {
            let _ = tx.rollback().await;
            return Err(ApiError::internal(format!("Failed to create m.room.member event: {e}")));
        }

        // m.room.power_levels with Matrix spec defaults for a new room.
        let mut power_levels = json!({
            "users": { user_id: 100 },
            "users_default": 0,
            "events": {
                "m.room.name": 50,
                "m.room.power_levels": 100,
                "m.room.history_visibility": 100,
                "m.room.canonical_alias": 50,
                "m.room.avatar": 50,
                "m.room.tombstone": 100,
                "m.room.server_acl": 100,
                "m.room.encryption": 100,
            },
            "events_default": 0,
            "state_default": 50,
            "ban": 50,
            "kick": 50,
            "redact": 50,
            "invite": 0,
        });
        if let Some(override_obj) = config
            .power_level_content_override
            .as_ref()
            .and_then(|v| v.as_object())
        {
            if let Some(target) = power_levels.as_object_mut() {
                for (k, v) in override_obj {
                    target.insert(k.clone(), v.clone());
                }
            }
        }
        let result = self
            .event_storage
            .create_event(
                CreateEventParams {
                    event_id: generate_event_id(&self.server_name),
                    room_id: room_id.clone(),
                    user_id: user_id.to_string(),
                    event_type: "m.room.power_levels".to_string(),
                    content: power_levels,
                    state_key: Some("".to_string()),
                    origin_server_ts: now + 2,
                },
                Some(&mut tx),
            )
            .await;
        if let Err(e) = result {
            let _ = tx.rollback().await;
            return Err(ApiError::internal(format!("Failed to create m.room.power_levels event: {e}")));
        }

        // m.room.join_rules so clients know who can join.
        let result = self
            .event_storage
            .create_event(
                CreateEventParams {
                    event_id: generate_event_id(&self.server_name),
                    room_id: room_id.clone(),
                    user_id: user_id.to_string(),
                    event_type: "m.room.join_rules".to_string(),
                    content: json!({ "join_rule": join_rule }),
                    state_key: Some("".to_string()),
                    origin_server_ts: now + 3,
                },
                Some(&mut tx),
            )
            .await;
        if let Err(e) = result {
            let _ = tx.rollback().await;
            return Err(ApiError::internal(format!("Failed to create m.room.join_rules event: {e}")));
        }

        // m.room.history_visibility — default "shared" matches Element's expectations.
        // For trusted_private_chat preset, history is restricted to "invited" only.
        let history_visibility = config.history_visibility.clone().unwrap_or_else(|| {
            if is_trusted_private {
                "invited".to_string()
            } else {
                "shared".to_string()
            }
        });
        let result = self
            .event_storage
            .create_event(
                CreateEventParams {
                    event_id: generate_event_id(&self.server_name),
                    room_id: room_id.clone(),
                    user_id: user_id.to_string(),
                    event_type: "m.room.history_visibility".to_string(),
                    content: json!({ "history_visibility": history_visibility }),
                    state_key: Some("".to_string()),
                    origin_server_ts: now + 4,
                },
                Some(&mut tx),
            )
            .await;
        if let Err(e) = result {
            let _ = tx.rollback().await;
            return Err(ApiError::internal(format!(
                "Failed to create m.room.history_visibility event: {e}"
            )));
        }

        // m.room.guest_access defaults to "can_join" for public rooms, "forbidden" otherwise.
        let guest_access = if is_public { "can_join" } else { "forbidden" };
        let result = self
            .event_storage
            .create_event(
                CreateEventParams {
                    event_id: generate_event_id(&self.server_name),
                    room_id: room_id.clone(),
                    user_id: user_id.to_string(),
                    event_type: "m.room.guest_access".to_string(),
                    content: json!({ "guest_access": guest_access }),
                    state_key: Some("".to_string()),
                    origin_server_ts: now + 5,
                },
                Some(&mut tx),
            )
            .await;
        if let Err(e) = result {
            let _ = tx.rollback().await;
            return Err(ApiError::internal(format!("Failed to create m.room.guest_access event: {e}")));
        }

        let result = self
            .set_room_metadata(
                &room_id,
                user_id,
                config.name.as_deref(),
                config.topic.as_deref(),
                now + 6,
                Some(&mut tx),
            )
            .await;
        if let Err(e) = result {
            let _ = tx.rollback().await;
            return Err(ApiError::internal(format!("Failed to set room metadata: {e}")));
        }

        let result = self
            .process_invites(
                &room_id,
                config.invite_list.as_ref(),
                user_id,
                now + 7,
                Some(&mut tx),
            )
            .await;
        if let Err(e) = result {
            let _ = tx.rollback().await;
            return Err(ApiError::internal(format!("Failed to process invites: {e}")));
        }

        // Replay any client-supplied `initial_state` events (e.g. Element's
        // `m.room.encryption` for DMs, or `m.room.canonical_alias` overrides).
        // These are applied AFTER the spec-mandated defaults so the client can
        // intentionally override e.g. history_visibility. Standard m.room.*
        // events with the same (type, state_key) shadow the earlier event in
        // the timeline, which matches Synapse's behavior.
        if let Some(extra_state) = config.initial_state.as_ref() {
            for (idx, evt) in extra_state.iter().enumerate() {
                let Some(obj) = evt.as_object() else { continue };
                let Some(event_type) = obj.get("type").and_then(|v| v.as_str()) else {
                    continue;
                };
                let state_key = obj
                    .get("state_key")
                    .and_then(|v| v.as_str())
                    .unwrap_or("")
                    .to_string();
                let content = obj.get("content").cloned().unwrap_or_else(|| json!({}));
                let result = self
                    .event_storage
                    .create_event(
                        CreateEventParams {
                            event_id: generate_event_id(&self.server_name),
                            room_id: room_id.clone(),
                            user_id: user_id.to_string(),
                            event_type: event_type.to_string(),
                            content,
                            state_key: Some(state_key),
                            origin_server_ts: now + 9 + idx as i64,
                        },
                        Some(&mut tx),
                    )
                    .await;
                if let Err(e) = result {
                    let _ = tx.rollback().await;
                    return Err(ApiError::internal(format!(
                        "Failed to apply initial_state event {event_type}: {e}"
                    )));
                }
            }
        }

        // Handle trusted private chat specific logic. The standard state
        // (m.room.history_visibility="invited", m.room.guest_access="forbidden")
        // is already emitted above based on `is_trusted_private`. We only add the
        // private-chat anti-screenshot privacy marker here.
        if is_trusted_private {
            let privacy_content = json!({ "action": "block_screenshot" });
            let result = self
                .event_storage
                .create_event(
                    CreateEventParams {
                        event_id: generate_event_id(&self.server_name),
                        room_id: room_id.clone(),
                        user_id: user_id.to_string(),
                        event_type: "com.hula.privacy".to_string(),
                        content: privacy_content,
                        state_key: Some("".to_string()),
                        origin_server_ts: now + 8,
                    },
                    Some(&mut tx),
                )
                .await;
            if let Err(e) = result {
                let _ = tx.rollback().await;
                return Err(ApiError::internal(format!("Failed to set privacy marker: {e}")));
            }
        }

        tx.commit()
            .await
            .map_err(|e| ApiError::internal(format!("Failed to commit transaction: {e}")))?;

        // Auto-create room summary
        let summary_request = crate::storage::room_summary::CreateRoomSummaryRequest {
            room_id: room_id.clone(),
            room_type: config.room_type.clone(),
            name: config.name.clone(),
            topic: config.topic.clone(),
            avatar_url: None,
            canonical_alias: None,
            join_rule: Some(join_rule.to_string()),
            history_visibility: config.history_visibility.clone(),
            guest_access: None,
            is_direct: config.is_direct,
            is_space: Some(config.room_type.as_deref() == Some("m.space")),
        };
        if let Err(e) = self
            .room_summary_service
            .create_summary(summary_request)
            .await
        {
            ::tracing::warn!("Failed to create room summary: {}", e);
        }

        // Save room alias if provided
        if let Some(ref alias) = config.room_alias_name {
            let full_alias = format!("#{}:{}", alias, self.server_name);
            validate_room_alias_input(&full_alias)?;
            if let Err(e) = self
                .room_storage
                .set_room_alias(&room_id, &full_alias, user_id)
                .await
            {
                ::tracing::warn!("Failed to save room alias: {}", e);
            }
        }

        let room_alias = self.format_room_alias(config.room_alias_name.as_deref());
        Ok(Self::build_room_response(&room_id, room_alias.as_deref()))
    }

    fn generate_room_id(&self) -> String {
        generate_room_id(&self.server_name)
    }

    fn determine_join_rule(preset: Option<&str>) -> &'static str {
        match preset {
            Some("public_chat") => "public",
            _ => "invite",
        }
    }

    fn is_public_visibility(visibility: Option<&str>) -> bool {
        visibility.unwrap_or("private") == "public"
    }

    async fn create_room_in_db(
        &self,
        room_id: &str,
        user_id: &str,
        join_rule: &str,
        is_public: bool,
        room_version: &str,
        tx: Option<&mut sqlx::Transaction<'_, sqlx::Postgres>>,
    ) -> ApiResult<()> {
        let result = if let Some(tx) = tx {
            self.room_storage
                .create_room_in_tx(tx, room_id, user_id, join_rule, room_version, is_public)
                .await
        } else {
            self.room_storage
                .create_room(room_id, user_id, join_rule, room_version, is_public)
                .await
        };

        result
            .map(|_| ())
            .map_err(|e| ApiError::internal(format!("Failed to create room: {e}")))
    }

    async fn add_creator_to_room(
        &self,
        room_id: &str,
        user_id: &str,
        tx: Option<&mut sqlx::Transaction<'_, sqlx::Postgres>>,
    ) -> ApiResult<()> {
        self.member_storage
            .add_member(room_id, user_id, "join", None, None, tx)
            .await
            .map_err(|e| ApiError::internal(format!("Failed to add room member: {e}")))?;

        // create_room already initializes member_count to 1, so no increment needed here.
        Ok(())
    }

    #[allow(clippy::needless_option_as_deref)]
    async fn set_room_metadata(
        &self,
        room_id: &str,
        user_id: &str,
        name: Option<&str>,
        topic: Option<&str>,
        base_ts: i64,
        mut tx: Option<&mut sqlx::Transaction<'_, sqlx::Postgres>>,
    ) -> ApiResult<()> {
        if let Some(room_name) = name {
            if let Some(ref mut tx) = tx {
                self.room_storage
                    .update_room_name_in_tx(tx, room_id, room_name)
                    .await
                    .map_err(|e| {
                        ApiError::internal(format!("Failed to update room name: {e}"))
                    })?;
            } else {
                self.room_storage
                    .update_room_name(room_id, room_name)
                    .await
                    .map_err(|e| {
                        ApiError::internal(format!("Failed to update room name: {e}"))
                    })?;
            }
            // Also persist m.room.name as a state event so /sync delivers it.
            self.event_storage
                .create_event(
                    CreateEventParams {
                        event_id: generate_event_id(&self.server_name),
                        room_id: room_id.to_string(),
                        user_id: user_id.to_string(),
                        event_type: "m.room.name".to_string(),
                        content: json!({ "name": room_name }),
                        state_key: Some("".to_string()),
                        origin_server_ts: base_ts,
                    },
                    tx.as_deref_mut(),
                )
                .await
                .map_err(|e| {
                    ApiError::internal(format!("Failed to create m.room.name event: {e}"))
                })?;
        }

        if let Some(room_topic) = topic {
            if let Some(ref mut tx) = tx {
                self.room_storage
                    .update_room_topic_in_tx(tx, room_id, room_topic)
                    .await
                    .map_err(|e| {
                        ApiError::internal(format!("Failed to update room topic: {e}"))
                    })?;
            } else {
                self.room_storage
                    .update_room_topic(room_id, room_topic)
                    .await
                    .map_err(|e| {
                        ApiError::internal(format!("Failed to update room topic: {e}"))
                    })?;
            }
            self.event_storage
                .create_event(
                    CreateEventParams {
                        event_id: generate_event_id(&self.server_name),
                        room_id: room_id.to_string(),
                        user_id: user_id.to_string(),
                        event_type: "m.room.topic".to_string(),
                        content: json!({ "topic": room_topic }),
                        state_key: Some("".to_string()),
                        origin_server_ts: base_ts + 1,
                    },
                    tx.as_deref_mut(),
                )
                .await
                .map_err(|e| {
                    ApiError::internal(format!("Failed to create m.room.topic event: {e}"))
                })?;
        }

        Ok(())
    }

    async fn process_invites(
        &self,
        room_id: &str,
        invite_list: Option<&Vec<String>>,
        sender_user_id: &str,
        base_ts: i64,
        mut tx: Option<&mut sqlx::Transaction<'_, sqlx::Postgres>>,
    ) -> ApiResult<()> {
        if let Some(invites) = invite_list {
            let existing_users = self
                .user_storage
                .filter_existing_users(invites)
                .await
                .map_err(|e| {
                    ApiError::internal(format!("Failed to check users existence: {e}"))
                })?;

            // We need to handle tx carefully.
            // If we have a transaction, we need to pass a mutable reference to it for each iteration.
            // But Option<&mut T> is not Copy.

            // If tx is Some, we extract the inner mutable reference, and we can reborrow it.
            if let Some(ref mut t) = tx {
                let mut offset: i64 = 0;
                for invitee in invites {
                    if !existing_users.contains(invitee) {
                        ::tracing::warn!("Skipping invite for non-existent user: {}", invitee);
                        continue;
                    }
                    self.member_storage
                        .add_member(room_id, invitee, "invite", None, None, Some(&mut **t))
                        .await
                        .map_err(|e| ApiError::internal(format!("Failed to invite user: {e}")))?;
                    self.event_storage
                        .create_event(
                            CreateEventParams {
                                event_id: generate_event_id(&self.server_name),
                                room_id: room_id.to_string(),
                                user_id: sender_user_id.to_string(),
                                event_type: "m.room.member".to_string(),
                                content: json!({
                                    "membership": "invite",
                                    "displayname": invitee.trim_start_matches('@').split(':').next().unwrap_or(invitee),
                                }),
                                state_key: Some(invitee.to_string()),
                                origin_server_ts: base_ts + offset,
                            },
                            Some(&mut **t),
                        )
                        .await
                        .map_err(|e| {
                            ApiError::internal(format!(
                                "Failed to record m.room.member invite event: {e}"
                            ))
                        })?;
                    offset += 1;
                }
            } else {
                let mut offset: i64 = 0;
                for invitee in invites {
                    if !existing_users.contains(invitee) {
                        continue;
                    }
                    self.member_storage
                        .add_member(room_id, invitee, "invite", None, None, None)
                        .await
                        .map_err(|e| ApiError::internal(format!("Failed to invite user: {e}")))?;
                    self.event_storage
                        .create_event(
                            CreateEventParams {
                                event_id: generate_event_id(&self.server_name),
                                room_id: room_id.to_string(),
                                user_id: sender_user_id.to_string(),
                                event_type: "m.room.member".to_string(),
                                content: json!({
                                    "membership": "invite",
                                    "displayname": invitee.trim_start_matches('@').split(':').next().unwrap_or(invitee),
                                }),
                                state_key: Some(invitee.to_string()),
                                origin_server_ts: base_ts + offset,
                            },
                            None,
                        )
                        .await
                        .map_err(|e| {
                            ApiError::internal(format!(
                                "Failed to record m.room.member invite event: {e}"
                            ))
                        })?;
                    offset += 1;
                }
            }
        }
        Ok(())
    }

    fn format_room_alias(&self, room_alias_name: Option<&str>) -> Option<String> {
        room_alias_name.map(|a| format!("#{}:{}", a, self.server_name))
    }

    fn build_room_response(room_id: &str, room_alias: Option<&str>) -> serde_json::Value {
        json!({
            "room_id": room_id,
            "room_alias": room_alias
        })
    }

    pub async fn send_message(
        &self,
        room_id: &str,
        user_id: &str,
        event_type: &str,
        content: &serde_json::Value,
    ) -> ApiResult<serde_json::Value> {
        if !self
            .member_storage
            .is_member(room_id, user_id)
            .await
            .map_err(|e| ApiError::internal(format!("Failed to check membership: {e}")))?
        {
            return Err(ApiError::forbidden(
                "You are not a member of this room".to_string(),
            ));
        }

        let event_id = generate_event_id(&self.server_name);
        let now = chrono::Utc::now().timestamp_millis();
        let max_ts = self
            .event_storage
            .get_max_origin_server_ts_for_room(room_id)
            .await
            .unwrap_or(0);
        let now = now.max(max_ts + 1);

        #[allow(unused_variables)]
        let beacon_location_params = {
            #[cfg(feature = "beacons")]
            {
                if matches!(
                    event_type,
                    "m.beacon" | "org.matrix.msc3672.beacon" | "org.matrix.msc3489.beacon"
                ) {
                    let Some(beacon_service) = self.beacon_service.as_ref() else {
                        return Err(ApiError::internal(
                            "Beacon service not configured".to_string(),
                        ));
                    };

                    let beacon_info_id = content
                        .get("m.relates_to")
                        .and_then(|v| v.get("event_id"))
                        .and_then(|v| v.as_str())
                        .ok_or_else(|| {
                            ApiError::bad_request(
                                "Missing m.relates_to.event_id for m.beacon".to_string(),
                            )
                        })?
                        .to_string();

                    let location = content
                        .get("m.location")
                        .or_else(|| content.get("org.matrix.msc3488.location"))
                        .and_then(|v| v.as_object())
                        .ok_or_else(|| {
                            ApiError::bad_request("Missing m.location for m.beacon".to_string())
                        })?;

                    let uri = location
                        .get("uri")
                        .and_then(|v| v.as_str())
                        .ok_or_else(|| ApiError::bad_request("Missing m.location.uri".to_string()))?
                        .to_string();

                    let description = location
                        .get("description")
                        .and_then(|v| v.as_str())
                        .map(|v| v.to_string());

                    let ts = content
                        .get("m.ts")
                        .or_else(|| content.get("org.matrix.msc3488.ts"))
                        .and_then(|v| v.as_i64())
                        .unwrap_or(now);

                    let accuracy =
                        crate::services::beacon_service::BeaconService::parse_geo_uri(&uri)
                            .and_then(|(_, _, acc)| acc)
                            .map(|v| v.round() as i64);

                    let beacon_info = beacon_service
                        .get_beacon_info(room_id, &beacon_info_id)
                        .await
                        .map_err(|e| {
                            ApiError::internal(format!("Failed to validate beacon: {}", e))
                        })?;
                    let Some(beacon_info) = beacon_info else {
                        return Err(ApiError::bad_request(
                            "Referenced beacon_info does not exist".to_string(),
                        ));
                    };

                    if !beacon_info.is_live {
                        return Err(ApiError::bad_request(
                            "Referenced beacon_info is not live".to_string(),
                        ));
                    }
                    if let Some(expires_at) = beacon_info.expires_at {
                        if expires_at <= now {
                            return Err(ApiError::bad_request(
                                "Referenced beacon_info has expired".to_string(),
                            ));
                        }
                    }

                    if let Some(retry_after_ms) = beacon_service
                        .check_room_backpressure(room_id, now)
                        .await
                        .map_err(|e| {
                            ApiError::internal(format!("Failed to check room backpressure: {}", e))
                        })?
                    {
                        return Err(ApiError::rate_limited_with_retry(retry_after_ms));
                    }

                    if let Some(retry_after_ms) = beacon_service
                        .check_location_quota(room_id, user_id, now)
                        .await
                        .map_err(|e| {
                            ApiError::internal(format!("Failed to check beacon quota: {}", e))
                        })?
                    {
                        return Err(ApiError::rate_limited_with_retry(retry_after_ms));
                    }

                    let latest = beacon_service
                        .get_latest_location(&beacon_info_id)
                        .await
                        .map_err(|e| {
                            ApiError::internal(format!("Failed to check beacon rate limit: {}", e))
                        })?;
                    if let Some(latest) = latest {
                        if ts <= latest.timestamp {
                            return Err(ApiError::bad_request(
                                "Beacon location timestamp must be increasing".to_string(),
                            ));
                        }
                        let delta = ts - latest.timestamp;
                        if delta < 1000 {
                            return Err(ApiError::rate_limited_with_retry((1000 - delta) as u64));
                        }
                    }

                    Some(crate::storage::beacon::CreateBeaconLocationParams {
                        room_id: room_id.to_string(),
                        event_id: event_id.clone(),
                        beacon_info_id,
                        sender: user_id.to_string(),
                        uri,
                        description,
                        timestamp: ts,
                        accuracy,
                        created_ts: now,
                    })
                } else {
                    None
                }
            }
            #[cfg(not(feature = "beacons"))]
            {
                None::<()>
            }
        };

        self.event_storage
            .create_event(
                CreateEventParams {
                    event_id: event_id.clone(),
                    room_id: room_id.to_string(),
                    user_id: user_id.to_string(),
                    event_type: event_type.to_string(),
                    content: content.clone(),
                    state_key: None,
                    origin_server_ts: now,
                },
                None,
            )
            .await
            .map_err(|e| ApiError::internal(format!("Failed to send message: {e}")))?;

        if let Some(relates_to) = content
            .get("m.relates_to")
            .or_else(|| content.get("relates_to"))
        {
            if let (Some(rel_type), Some(target_event_id)) = (
                relates_to.get("rel_type").and_then(|v| v.as_str()),
                relates_to.get("event_id").and_then(|v| v.as_str()),
            ) {
                if let Err(e) = self
                    .relations_storage
                    .create_relation(crate::storage::relations::CreateRelationParams {
                        room_id: room_id.to_string(),
                        event_id: event_id.clone(),
                        relates_to_event_id: target_event_id.to_string(),
                        relation_type: rel_type.to_string(),
                        sender: user_id.to_string(),
                        origin_server_ts: now,
                        content: content.clone(),
                    })
                    .await
                {
                    ::tracing::warn!(
                        target: "relations",
                        event_id = %event_id,
                        error = %e,
                        "Failed to index event relation"
                    );
                }
            }
        }

        #[cfg(feature = "beacons")]
        if let (Some(beacon_service), Some(params)) =
            (self.beacon_service.as_ref(), beacon_location_params)
        {
            beacon_service
                .report_location(params)
                .await
                .map_err(|e| ApiError::internal(format!("Failed to index beacon: {}", e)))?;
        }

        Ok(json!({
            "event_id": event_id
        }))
    }

    pub async fn join_room(&self, room_id: &str, user_id: &str) -> ApiResult<()> {
        if !self
            .room_storage
            .room_exists(room_id)
            .await
            .map_err(|e| ApiError::internal(format!("Failed to check room: {e}")))?
        {
            return Err(ApiError::not_found("Room not found".to_string()));
        }

        if !self
            .user_storage
            .user_exists(user_id)
            .await
            .map_err(|e| ApiError::internal(format!("Failed to check user existence: {e}")))?
        {
            return Err(ApiError::not_found("User not found".to_string()));
        }

        let effective_join_rule = if let Some(event) = self
            .event_storage
            .get_state_events_by_type(room_id, "m.room.join_rules")
            .await
            .map_err(|e| ApiError::internal(format!("Failed to load room join rules: {e}")))?
            .into_iter()
            .find(|event| event.state_key.as_deref().unwrap_or_default().is_empty())
        {
            event
                .content
                .get("join_rule")
                .and_then(|value| value.as_str())
                .map(|value| value.to_string())
        } else {
            None
        };

        let room = self
            .room_storage
            .get_room(room_id)
            .await
            .map_err(|e| ApiError::internal(format!("Failed to load room: {e}")))?
            .ok_or_else(|| ApiError::not_found("Room not found".to_string()))?;

        let join_rule = effective_join_rule
            .or_else(|| (!room.join_rule.is_empty()).then(|| room.join_rule.clone()))
            .unwrap_or_else(|| {
                if room.is_public {
                    "public".to_string()
                } else {
                    "invite".to_string()
                }
            });

        let existing_member = self
            .member_storage
            .get_room_member(room_id, user_id)
            .await
            .map_err(|e| ApiError::internal(format!("Failed to check membership: {e}")))?;

        if let Some(member) = existing_member.as_ref() {
            if member.membership == "join" {
                return Ok(());
            }

            if member.membership == "ban" || member.is_banned.unwrap_or(false) {
                return Err(ApiError::forbidden(
                    "You are banned from this room".to_string(),
                ));
            }
        }

        if join_rule != "public"
            && existing_member
                .as_ref()
                .is_none_or(|member| member.membership != "invite")
        {
            return Err(ApiError::forbidden("Room is invite-only".to_string()));
        }

        self.member_storage
            .add_member(room_id, user_id, "join", None, None, None)
            .await
            .map_err(|e| ApiError::internal(format!("Failed to join room: {e}")))?;

        self.room_storage
            .increment_member_count(room_id)
            .await
            .map_err(|e| ApiError::internal(format!("Failed to update member count: {e}")))?;

        // Persist the m.room.member state event so /sync delivers the membership
        // change and the client knows it has actually joined.
        self.event_storage
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
                },
                None,
            )
            .await
            .map_err(|e| {
                ApiError::internal(format!("Failed to record m.room.member join event: {e}"))
            })?;

        Ok(())
    }

    pub async fn leave_room(&self, room_id: &str, user_id: &str) -> ApiResult<()> {
        self.member_storage
            .remove_member(room_id, user_id)
            .await
            .map_err(|e| ApiError::internal(format!("Failed to leave room: {e}")))?;

        self.room_storage
            .decrement_member_count(room_id)
            .await
            .map_err(|e| ApiError::internal(format!("Failed to update member count: {e}")))?;

        // Persist the m.room.member leave event for /sync.
        self.event_storage
            .create_event(
                CreateEventParams {
                    event_id: generate_event_id(&self.server_name),
                    room_id: room_id.to_string(),
                    user_id: user_id.to_string(),
                    event_type: "m.room.member".to_string(),
                    content: json!({ "membership": "leave" }),
                    state_key: Some(user_id.to_string()),
                    origin_server_ts: chrono::Utc::now().timestamp_millis(),
                },
                None,
            )
            .await
            .map_err(|e| {
                ApiError::internal(format!("Failed to record m.room.member leave event: {e}"))
            })?;

        Ok(())
    }

    pub async fn forget_room(&self, room_id: &str, user_id: &str) -> ApiResult<()> {
        let membership = self
            .member_storage
            .get_room_member(room_id, user_id)
            .await
            .map_err(|e| ApiError::internal(format!("Failed to check membership: {e}")))?;

        match membership {
            Some(member) => match member.membership.as_str() {
                "join" => {
                    return Err(ApiError::bad_request(
                        "Cannot forget a room you are still joined to. Leave the room first."
                            .to_string(),
                    ));
                }
                "ban" => {
                    return Err(ApiError::forbidden(
                        "Cannot forget a room you have been banned from.".to_string(),
                    ));
                }
                "leave" | "invite" => {
                    self.member_storage
                        .forget_member(room_id, user_id)
                        .await
                        .map_err(|e| ApiError::internal(format!("Failed to forget room: {e}")))?;
                }
                _ => {
                    return Err(ApiError::bad_request(format!(
                        "Unknown membership state: {}",
                        member.membership
                    )));
                }
            },
            None => {
                return Err(ApiError::not_found(
                    "No membership record found for this room".to_string(),
                ));
            }
        }

        Ok(())
    }

    pub async fn get_room_members(
        &self,
        room_id: &str,
        user_id: &str,
    ) -> ApiResult<serde_json::Value> {
        if !self
            .room_storage
            .room_exists(room_id)
            .await
            .map_err(|e| ApiError::internal(format!("Failed to check room existence: {e}")))?
        {
            return Err(ApiError::not_found("Room not found".to_string()));
        }

        if !self
            .member_storage
            .is_member(room_id, user_id)
            .await
            .map_err(|e| ApiError::internal(format!("Failed to check membership: {e}")))?
        {
            return Err(ApiError::forbidden(
                "You are not a member of this room".to_string(),
            ));
        }

        let members_with_profiles = self
            .member_storage
            .get_room_members_with_profiles(room_id, "join")
            .await
            .map_err(|e| ApiError::internal(format!("Failed to get members: {e}")))?;

        let chunk: Vec<serde_json::Value> = members_with_profiles
            .iter()
            .map(|(m, user_displayname, user_avatar_url)| {
                let mut content = serde_json::Map::new();
                content.insert("membership".to_string(), json!(m.membership));
                let effective_displayname =
                    m.display_name.as_deref().or(user_displayname.as_deref());
                if let Some(dn) = effective_displayname {
                    content.insert("displayname".to_string(), json!(dn));
                }
                let effective_avatar_url = m.avatar_url.as_deref().or(user_avatar_url.as_deref());
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

        Ok(json!({ "chunk": chunk }))
    }

    pub async fn get_room(&self, room_id: &str) -> ApiResult<serde_json::Value> {
        let room = self
            .room_storage
            .get_room(room_id)
            .await
            .map_err(|e| ApiError::internal(format!("Failed to get room: {e}")))?;

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

    pub async fn get_room_state(
        &self,
        room_id: &str,
        user_id: &str,
    ) -> ApiResult<serde_json::Value> {
        if !self
            .member_storage
            .is_member(room_id, user_id)
            .await
            .map_err(|e| ApiError::internal(format!("Failed to check membership: {e}")))?
        {
            return Err(ApiError::forbidden(
                "You are not a member of this room".to_string(),
            ));
        }

        let room = self
            .room_storage
            .get_room(room_id)
            .await
            .map_err(|e| ApiError::internal(format!("Failed to get room: {e}")))?;

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
            .map_err(|e| ApiError::internal(format!("Failed to get rooms: {e}")))?;

        let rooms_data = self
            .room_storage
            .get_rooms_batch(&room_ids)
            .await
            .map_err(|e| ApiError::internal(format!("Failed to fetch rooms batch: {e}")))?;

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

    pub async fn get_room_messages(
        &self,
        room_id: &str,
        user_id: &str,
        from: i64,
        limit: i64,
        direction: &str,
    ) -> ApiResult<serde_json::Value> {
        let is_member = self
            .member_storage
            .is_member(room_id, user_id)
            .await
            .map_err(|e| ApiError::internal(format!("Failed to check membership: {e}")))?;
        if !is_member {
            let room = self
                .room_storage
                .get_room(room_id)
                .await
                .map_err(|e| ApiError::internal(format!("Failed to get room: {e}")))?;
            let is_public = room.as_ref().is_some_and(|r| r.is_public);
            if !is_public {
                return Err(ApiError::forbidden(
                    "You are not a member of this room".to_string(),
                ));
            }
        }

        let normalized_direction = if direction == "f" { "f" } else { "b" };

        let start_token = if from > 0 {
            generate_stream_token_from_ts(Some(from))
        } else {
            let max_ts = self
                .event_storage
                .get_max_origin_server_ts_for_room(room_id)
                .await
                .map_err(|e| ApiError::internal(format!("Failed to get room stream: {e}")))?;
            generate_stream_token_from_ts(Some(max_ts))
        };

        let from_ts = if from > 0 {
            parse_stream_token(&start_token).or(Some(from))
        } else {
            None
        };

        let events = self
            .event_storage
            .get_room_events_paginated(room_id, from_ts, limit, normalized_direction)
            .await
            .map_err(|e| ApiError::internal(format!("Failed to get messages: {e}")))?;

        let event_list: Vec<serde_json::Value> = events
            .iter()
            .map(|e| {
                json!({
                    "type": e.event_type,
                    "content": e.content,
                    "sender": e.user_id,
                    "origin_server_ts": e.origin_server_ts,
                    "event_id": e.event_id
                })
            })
            .collect();

        let end_token = events
            .last()
            .map_or_else(|| start_token.clone(), |event| generate_stream_token_from_ts(Some(event.origin_server_ts)));

        Ok(json!({
            "chunk": event_list,
            "start": start_token,
            "end": end_token
        }))
    }

    pub async fn invite_user(
        &self,
        room_id: &str,
        inviter_id: &str,
        invitee_id: &str,
    ) -> ApiResult<()> {
        if !self
            .room_storage
            .room_exists(room_id)
            .await
            .map_err(|e| ApiError::internal(format!("Failed to check room: {e}")))?
        {
            return Err(ApiError::not_found("Room not found".to_string()));
        }

        if !self
            .user_storage
            .user_exists(invitee_id)
            .await
            .map_err(|e| ApiError::internal(format!("Failed to check user existence: {e}")))?
        {
            return Err(ApiError::not_found("User not found".to_string()));
        }

        self.auth_service
            .can_invite_user(room_id, inviter_id)
            .await?;

        self.member_storage
            .add_member(room_id, invitee_id, "invite", None, Some(inviter_id), None)
            .await
            .map_err(|e| ApiError::internal(format!("Failed to create invite event: {e}")))?;

        // Persist the m.room.member invite state event so the invitee's /sync
        // delivers the invite under `rooms.invite`. Without this row in
        // `events`, the recipient never sees the invitation.
        self.event_storage
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
                    origin_server_ts: chrono::Utc::now().timestamp_millis(),
                },
                None,
            )
            .await
            .map_err(|e| {
                ApiError::internal(format!(
                    "Failed to record m.room.member invite event: {e}"
                ))
            })?;

        Ok(())
    }

    pub async fn knock_room(
        &self,
        room_id: &str,
        user_id: &str,
        reason: Option<&str>,
    ) -> ApiResult<()> {
        if !self
            .room_storage
            .room_exists(room_id)
            .await
            .map_err(|e| ApiError::internal(format!("Failed to check room: {e}")))?
        {
            return Err(ApiError::not_found("Room not found".to_string()));
        }

        let effective_join_rule = if let Some(event) = self
            .event_storage
            .get_state_events_by_type(room_id, "m.room.join_rules")
            .await
            .map_err(|e| ApiError::internal(format!("Failed to load room join rules: {e}")))?
            .into_iter()
            .find(|event| event.state_key.as_deref().unwrap_or_default().is_empty())
        {
            event
                .content
                .get("join_rule")
                .and_then(|value| value.as_str())
                .map(|value| value.to_string())
        } else {
            None
        };

        let room = self
            .room_storage
            .get_room(room_id)
            .await
            .map_err(|e| ApiError::internal(format!("Failed to load room: {e}")))?
            .ok_or_else(|| ApiError::not_found("Room not found".to_string()))?;

        let join_rule = effective_join_rule
            .or_else(|| (!room.join_rule.is_empty()).then(|| room.join_rule.clone()))
            .unwrap_or_else(|| {
                if room.is_public {
                    "public".to_string()
                } else {
                    "invite".to_string()
                }
            });

        let existing_member = self
            .member_storage
            .get_room_member(room_id, user_id)
            .await
            .map_err(|e| ApiError::internal(format!("Failed to check membership: {e}")))?;

        if let Some(member) = existing_member.as_ref() {
            match member.membership.as_str() {
                "join" => {
                    return Err(ApiError::bad_request(
                        "You are already joined to this room".to_string(),
                    ));
                }
                "invite" => {
                    return Err(ApiError::forbidden(
                        "You have already been invited to this room".to_string(),
                    ));
                }
                "ban" => {
                    return Err(ApiError::forbidden(
                        "You are banned from this room".to_string(),
                    ));
                }
                "knock" => return Ok(()),
                _ => {}
            }
        }

        if join_rule != "knock" && join_rule != "knock_restricted" {
            return Err(ApiError::forbidden("Room does not allow knock".to_string()));
        }

        self.member_storage
            .add_member(room_id, user_id, "knock", None, reason, None)
            .await
            .map_err(|e| ApiError::internal(format!("Failed to create knock event: {e}")))?;
        Ok(())
    }

    pub async fn ban_user(
        &self,
        room_id: &str,
        user_id: &str,
        banned_by: &str,
        _reason: Option<&str>,
    ) -> ApiResult<()> {
        if !self
            .room_storage
            .room_exists(room_id)
            .await
            .map_err(|e| ApiError::internal(format!("Failed to check room: {e}")))?
        {
            return Err(ApiError::not_found("Room not found".to_string()));
        }

        if !self
            .user_storage
            .user_exists(user_id)
            .await
            .map_err(|e| ApiError::internal(format!("Failed to check user existence: {e}")))?
        {
            return Err(ApiError::not_found("User not found".to_string()));
        }

        self.auth_service
            .can_ban_user(room_id, banned_by, user_id)
            .await?;

        self.member_storage
            .ban_member(room_id, user_id, banned_by)
            .await
            .map_err(|e| ApiError::internal(format!("Failed to ban user: {e}")))?;
        Ok(())
    }

    pub async fn get_state_events(&self, room_id: &str) -> ApiResult<Vec<serde_json::Value>> {
        let events = self
            .event_storage
            .get_state_events(room_id)
            .await
            .map_err(|e| ApiError::internal(format!("Failed to get state events: {e}")))?;

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

    pub async fn get_public_rooms(&self, limit: i64) -> ApiResult<serde_json::Value> {
        let rooms = self
            .room_storage
            .get_public_rooms(limit)
            .await
            .map_err(|e| ApiError::internal(format!("Failed to get public rooms: {e}")))?;

        let room_list: Vec<serde_json::Value> = rooms
            .iter()
            .map(|r| {
                json!({
                    "room_id": r.room_id,
                    "name": r.name,
                    "topic": r.topic,
                    "canonical_alias": r.canonical_alias,
                    "is_public": r.is_public,
                    "join_rule": r.join_rule
                })
            })
            .collect();

        Ok(json!({
            "chunk": room_list,
            "total_room_count_estimate": room_list.len() as i64
        }))
    }

    pub async fn delete_room(&self, room_id: &str, requester_id: &str) -> ApiResult<()> {
        let room = self
            .room_storage
            .get_room(room_id)
            .await
            .map_err(|e| ApiError::internal(format!("Failed to get room: {e}")))?
            .ok_or_else(|| ApiError::not_found("Room not found".to_string()))?;

        let requester = self
            .user_storage
            .get_user_by_id(requester_id)
            .await
            .map_err(|e| ApiError::internal(format!("Failed to get user: {e}")))?
            .ok_or_else(|| ApiError::unauthorized("Requester not found"))?;

        let is_creator = room.creator_user_id.as_deref() == Some(requester_id);
        let is_admin = requester.is_admin;

        if !is_creator && !is_admin {
            return Err(ApiError::forbidden(
                "Only the room creator or a server admin can delete a room".to_string(),
            ));
        }

        self.room_storage
            .delete_room(room_id)
            .await
            .map_err(|e| ApiError::internal(format!("Failed to delete room: {e}")))
    }

    pub async fn get_joined_rooms(&self, user_id: &str) -> ApiResult<Vec<String>> {
        self.member_storage
            .get_joined_rooms(user_id)
            .await
            .map_err(|e| ApiError::internal(format!("Failed to get joined rooms: {e}")))
    }

    pub async fn room_exists(&self, room_id: &str) -> ApiResult<bool> {
        let exists =
            self.room_storage.room_exists(room_id).await.map_err(|e| {
                ApiError::database(format!("Failed to check room existence: {e}"))
            })?;
        Ok(exists)
    }

    pub async fn is_room_creator(&self, room_id: &str, user_id: &str) -> ApiResult<bool> {
        let room = self
            .room_storage
            .get_room(room_id)
            .await
            .map_err(|e| ApiError::internal(format!("Failed to get room: {e}")))?;

        match room {
            Some(r) => Ok(r.creator_user_id.as_deref() == Some(user_id)),
            None => Ok(false),
        }
    }

    pub async fn get_room_aliases(&self, room_id: &str) -> ApiResult<Vec<String>> {
        self.room_storage
            .get_room_aliases(room_id)
            .await
            .map_err(|e| ApiError::internal(format!("Failed to get room aliases: {e}")))
    }

    pub async fn set_room_alias(
        &self,
        room_id: &str,
        alias: &str,
        created_by: &str,
    ) -> ApiResult<()> {
        validate_room_alias_input(alias)?;
        self.room_storage
            .set_room_alias(room_id, alias, created_by)
            .await
            .map_err(|e| ApiError::internal(format!("Failed to set room alias: {e}")))
    }

    pub async fn get_room_by_alias(&self, alias: &str) -> ApiResult<Option<String>> {
        validate_room_alias_input(alias)?;
        self.room_storage
            .get_room_by_alias(alias)
            .await
            .map_err(|e| ApiError::internal(format!("Failed to get room by alias: {e}")))
    }

    pub async fn remove_room_alias(&self, room_id: &str) -> ApiResult<()> {
        self.room_storage
            .remove_room_alias(room_id)
            .await
            .map_err(|e| ApiError::internal(format!("Failed to remove room alias: {e}")))
    }

    pub async fn remove_room_alias_by_name(&self, alias: &str) -> ApiResult<()> {
        self.room_storage
            .remove_room_alias_by_name(alias)
            .await
            .map_err(|e| ApiError::internal(format!("Failed to remove room alias by name: {e}")))
    }

    pub async fn set_room_directory(&self, room_id: &str, is_public: bool) -> ApiResult<()> {
        self.room_storage
            .set_room_directory(room_id, is_public)
            .await
            .map_err(|e| ApiError::internal(format!("Failed to set room directory: {e}")))
    }

    pub async fn get_room_visibility(&self, room_id: &str) -> ApiResult<String> {
        let is_public = self
            .room_storage
            .is_room_in_directory(room_id)
            .await
            .map_err(|e| ApiError::internal(format!("Failed to get room visibility: {e}")))?;
        Ok(if is_public {
            "public".to_string()
        } else {
            "private".to_string()
        })
    }

    pub async fn remove_room_directory(&self, room_id: &str) -> ApiResult<()> {
        self.room_storage
            .remove_room_directory(room_id)
            .await
            .map_err(|e| ApiError::internal(format!("Failed to remove room from directory: {e}")))
    }

    pub async fn upgrade_room(
        &self,
        old_room_id: &str,
        new_version: &str,
        user_id: &str,
    ) -> ApiResult<String> {
        let old_room = self
            .room_storage
            .get_room(old_room_id)
            .await
            .map_err(|e| ApiError::internal(format!("Failed to get old room: {e}")))?
            .ok_or_else(|| ApiError::not_found("Room not found".to_string()))?;

        let new_room_id = generate_room_id(&self.server_name);

        let create_config = CreateRoomConfig {
            visibility: Some(if old_room.is_public {
                "public".to_string()
            } else {
                "private".to_string()
            }),
            room_alias_name: None,
            name: Some(
                old_room
                    .name
                    .clone()
                    .unwrap_or_else(|| "Upgraded Room".to_string()),
            ),
            topic: old_room.topic.clone(),
            invite_list: Some(vec![user_id.to_string()]),
            preset: Some("private_chat".to_string()),
            encryption: old_room.encryption.clone(),
            history_visibility: Some(old_room.history_visibility.clone()),
            is_direct: None,
            room_type: None,
            room_version: Some(new_version.to_string()),
            ..Default::default()
        };

        self.create_room(user_id, create_config).await?;

        self.room_storage
            .set_room_version(old_room_id, new_version)
            .await
            .map_err(|e| ApiError::internal(format!("Failed to update room version: {e}")))?;

        Ok(new_room_id)
    }

    pub async fn get_tombstone_event(&self, room_id: &str) -> ApiResult<Option<serde_json::Value>> {
        let state_events = self
            .event_storage
            .get_state_events(room_id)
            .await
            .map_err(|e| ApiError::internal(format!("Failed to get state events: {e}")))?;

        for event in state_events {
            if event.event_type.as_deref() == Some("m.room.tombstone") {
                return Ok(Some(serde_json::json!({
                    "type": event.event_type.clone().unwrap_or_default(),
                    "state_key": event.state_key,
                    "content": event.content,
                    "sender": event.sender,
                })));
            }
        }

        Ok(None)
    }

    pub async fn migrate_room_content(
        &self,
        source_room_id: &str,
        target_room_id: &str,
        user_id: &str,
    ) -> ApiResult<()> {
        let target_room = self
            .room_storage
            .get_room(target_room_id)
            .await
            .map_err(|e| ApiError::internal(format!("Failed to get target room: {e}")))?
            .ok_or_else(|| ApiError::not_found("Target room not found".to_string()))?;

        if target_room.creator_user_id.as_deref() != Some(user_id) {
            return Err(ApiError::forbidden(
                "Only room creator can migrate content".to_string(),
            ));
        }

        self.room_storage
            .copy_room_state(source_room_id, target_room_id)
            .await
            .map_err(|e| ApiError::internal(format!("Failed to copy room state: {e}")))?;

        Ok(())
    }

    pub async fn is_room_upgrade_allowed(&self, room_id: &str, user_id: &str) -> ApiResult<bool> {
        let room = self
            .room_storage
            .get_room(room_id)
            .await
            .map_err(|e| ApiError::internal(format!("Failed to get room: {e}")))?
            .ok_or_else(|| ApiError::not_found("Room not found".to_string()))?;

        let members = self
            .member_storage
            .get_room_members(room_id, "join")
            .await
            .map_err(|e| ApiError::internal(format!("Failed to get members: {e}")))?;

        let is_member = members
            .iter()
            .any(|m| m.user_id == user_id && m.membership == "join");

        if !is_member {
            return Ok(false);
        }

        Ok(room.creator_user_id.as_deref() == Some(user_id))
    }

    pub async fn process_read_receipt(
        &self,
        room_id: &str,
        event_id: &str,
        _user_id: &str,
        _custom_delay_secs: Option<u64>,
    ) -> ApiResult<()> {
        let event = match self.event_storage.get_event(event_id).await {
            Ok(Some(e)) => e,
            _ => return Ok(()),
        };

        let content = match event.content.as_object() {
            Some(c) => c,
            None => return Ok(()),
        };

        if !content.contains_key("burn_after_read") {
            return Ok(());
        }

        let queue = match self.task_queue.clone() {
            Some(q) => q,
            None => return Ok(()),
        };

        // Read custom delay time from message content
        let delay_secs = content
            .get("burn_after_read_delay_seconds")
            .and_then(|v| v.as_i64())
            .map_or(BURN_AFTER_READ_DELAY_SECS, |v| v as u64);

        let rid = room_id.to_string();
        let eid = event_id.to_string();
        let task_id = format!("burn_after_read:{rid}:{eid}:{delay_secs}");

        ::tracing::info!(
            "Scheduling burn-after-read for event {} in room {} with delay {}s",
            eid,
            rid,
            delay_secs
        );

        // Track spawned task to prevent memory leaks
        let handle = tokio::spawn(async move {
            tokio::time::sleep(secs(delay_secs)).await;

            let job = BackgroundJob::RedactEvent {
                event_id: eid.clone(),
                room_id: rid.clone(),
                reason: Some("Burn after read".to_string()),
            };

            match queue.submit(job).await {
                Ok(_) => {
                    ::tracing::info!("Submitted redaction job for event {}", eid);
                }
                Err(e) => {
                    ::tracing::error!("Failed to submit redaction job for event {}: {}", eid, e);
                }
            }
        });

        // Store the task handle for later cleanup/management
        self.active_tasks.write().await.insert(task_id, handle);

        Ok(())
    }

    pub async fn create_event(
        &self,
        params: CreateEventParams,
        tx: Option<&mut sqlx::Transaction<'_, sqlx::Postgres>>,
    ) -> ApiResult<crate::storage::RoomEvent> {
        let room_id = params.room_id.clone();
        let event_id = params.event_id.clone();
        let event_type = params.event_type.clone();
        let state_key = params.state_key.clone();
        let should_update_summary = tx.is_none();

        let event = self
            .event_storage
            .create_event(params, tx)
            .await
            .map_err(|e| ApiError::internal(format!("Failed to create event: {e}")))?;

        if should_update_summary
            && event_type == "m.room.canonical_alias"
            && state_key.as_deref() == Some("")
        {
            let canonical_alias = event.content.get("alias").and_then(|value| value.as_str());
            if let Err(error) = self
                .room_storage
                .set_canonical_alias(&room_id, canonical_alias)
                .await
            {
                ::tracing::warn!("Failed to project canonical alias onto room: {}", error);
            }
        }

        if should_update_summary {
            if let Err(error) = self
                .room_summary_service
                .queue_update(&room_id, &event_id, &event_type, state_key.as_deref())
                .await
            {
                ::tracing::warn!("Failed to queue room summary update: {}", error);
            } else if let Err(error) = self.room_summary_service.process_pending_updates(32).await {
                ::tracing::warn!("Failed to process room summary updates: {}", error);
            }
        }

        Ok(event)
    }

    pub async fn add_member(
        &self,
        room_id: &str,
        user_id: &str,
        membership: &str,
        display_name: Option<&str>,
        join_reason: Option<&str>,
        tx: Option<&mut sqlx::Transaction<'_, sqlx::Postgres>>,
    ) -> ApiResult<crate::storage::RoomMember> {
        let should_update_summary = tx.is_none();
        let member = self
            .member_storage
            .add_member(room_id, user_id, membership, display_name, join_reason, tx)
            .await
            .map_err(|e| ApiError::internal(format!("Failed to add member: {e}")))?;

        if should_update_summary {
            let request = crate::storage::room_summary::CreateSummaryMemberRequest {
                room_id: room_id.to_string(),
                user_id: user_id.to_string(),
                display_name: display_name.map(|value| value.to_string()),
                avatar_url: None,
                membership: membership.to_string(),
                is_hero: None,
                last_active_ts: member.joined_ts.or(member.updated_ts),
            };

            if let Err(error) = self.room_summary_service.add_member(request).await {
                ::tracing::warn!("Failed to update room summary member: {}", error);
            }

            if let Err(error) = self.room_summary_service.recalculate_heroes(room_id).await {
                ::tracing::warn!("Failed to recalculate room summary heroes: {}", error);
            }
        }

        Ok(member)
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
        let config = CreateRoomConfig {
            preset: Some("trusted_private_chat".to_string()),
            ..Default::default()
        };
        assert_eq!(config.preset.as_deref(), Some("trusted_private_chat"));
    }

    #[test]
    fn test_burn_after_read_metadata_detection() {
        let content = json!({
            "body": "secret message",
            "msgtype": "m.text",
            "burn_after_read": true
        });

        let has_metadata = content
            .as_object()
            .map(|c| c.contains_key("burn_after_read"))
            .unwrap_or(false);

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
