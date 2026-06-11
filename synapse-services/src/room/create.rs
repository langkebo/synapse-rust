//! Room creation logic extracted from `service.rs` for M-10 file size reduction.
//!
//! Contains `create_room` and all its private helper methods.

use super::service::{CreateRoomConfig, RoomService};
use super::utils::validate_room_alias_input;
use synapse_common::room_versions::{resolve_room_version, DEFAULT_ROOM_VERSION};
use synapse_common::{generate_event_id, generate_room_id, ApiError, ApiResult};
use synapse_storage::CreateEventParams;
use serde_json::json;

impl RoomService {
    pub async fn create_room(&self, user_id: &str, config: CreateRoomConfig) -> ApiResult<serde_json::Value> {
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

        let room_version = resolve_room_version(config.room_version.as_deref()).ok_or_else(|| {
            ApiError::unsupported_room_version(format!(
                "Unsupported room version: {}",
                config.room_version.as_deref().unwrap_or(DEFAULT_ROOM_VERSION)
            ))
        })?;

        let mut tx = self
            .room_storage
            .pool
            .begin()
            .await
            .map_err(|e| ApiError::internal_with_log("Failed to start transaction", &e))?;

        let result = self.create_room_in_db(&room_id, user_id, join_rule, is_public, room_version, Some(&mut tx)).await;
        if let Err(e) = &result {
            ::tracing::error!(
                room_id = %room_id,
                user_id = %user_id,
                join_rule = %join_rule,
                is_public,
                room_version = %room_version,
                error = %e,
                "create_room_in_db failed"
            );
            let _ = tx.rollback().await;
            return Err(ApiError::internal_with_log("Failed to create room", &e));
        }

        let now = chrono::Utc::now().timestamp_millis();
        let mut create_content = json!({
            "creator": user_id,
            "room_version": room_version,
        });
        if let Some(extra) = config.creation_content.as_ref().and_then(|v| v.as_object()) {
            if let Some(map) = create_content.as_object_mut() {
                for (k, v) in extra {
                    if matches!(k.as_str(), "room_version" | "creator") {
                        continue;
                    }
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
            ::tracing::error!(
                room_id = %room_id,
                user_id = %user_id,
                room_version = %room_version,
                error = %e,
                "m.room.create event failed"
            );
            let _ = tx.rollback().await;
            return Err(ApiError::internal_with_log("Failed to create m.room.create event", &e));
        }

        let result = self.add_creator_to_room(&room_id, user_id, Some(&mut tx)).await;
        if let Err(e) = &result {
            ::tracing::error!(
                room_id = %room_id,
                user_id = %user_id,
                error = %e,
                "add_creator_to_room failed"
            );
            let _ = tx.rollback().await;
            return Err(e.clone());
        }

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
            return Err(ApiError::internal_with_log("Failed to create m.room.member event", &e));
        }

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
        if let Some(override_obj) = config.power_level_content_override.as_ref().and_then(|v| v.as_object()) {
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
            return Err(ApiError::internal_with_log("Failed to create m.room.power_levels event", &e));
        }

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
            return Err(ApiError::internal_with_log("Failed to create m.room.join_rules event", &e));
        }

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
            return Err(ApiError::internal_with_log("Failed to create m.room.history_visibility event", &e));
        }

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
            return Err(ApiError::internal_with_log("Failed to create m.room.guest_access event", &e));
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
            return Err(ApiError::internal_with_log("Failed to set room metadata", &e));
        }

        let result = self.process_invites(&room_id, config.invite_list.as_ref(), user_id, now + 7, Some(&mut tx)).await;
        if let Err(e) = result {
            let _ = tx.rollback().await;
            return Err(ApiError::internal_with_log("Failed to process invites", &e));
        }

        let mut initial_join_rule: Option<String> = None;
        let mut has_encryption_in_initial_state = false;
        if let Some(extra_state) = config.initial_state.as_ref() {
            for (idx, evt) in extra_state.iter().enumerate() {
                let Some(obj) = evt.as_object() else { continue };
                let Some(event_type) = obj.get("type").and_then(|v| v.as_str()) else {
                    continue;
                };
                if matches!(event_type, "m.room.create" | "m.room.member" | "m.room.tombstone") {
                    let _ = tx.rollback().await;
                    return Err(ApiError::invalid_param(format!("{event_type} cannot be supplied in initial_state")));
                }
                let state_key = obj.get("state_key").and_then(|v| v.as_str()).unwrap_or("").to_string();
                let content = obj.get("content").cloned().unwrap_or_else(|| json!({}));

                if event_type == "m.room.encryption" {
                    has_encryption_in_initial_state = true;
                }

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
                    ::tracing::error!(
                        room_id = %room_id,
                        user_id = %user_id,
                        event_type = %event_type,
                        error = %e,
                        "Failed to apply initial_state event"
                    );
                    let _ = tx.rollback().await;
                    return Err(ApiError::internal_with_log("Failed to apply initial_state event {event_type}", &e));
                }

                if event_type == "m.room.join_rules" {
                    if let Some(jr) = evt.get("content").and_then(|c| c.get("join_rule")).and_then(|v| v.as_str()) {
                        initial_join_rule = Some(jr.to_string());
                    }
                }
            }
        }

        if let Some(ref algorithm) = config.encryption {
            if !has_encryption_in_initial_state {
                let encryption_ts = config.initial_state.as_ref().map_or(now + 9, |s| now + 9 + s.len() as i64);
                let result = self
                    .event_storage
                    .create_event(
                        CreateEventParams {
                            event_id: generate_event_id(&self.server_name),
                            room_id: room_id.clone(),
                            user_id: user_id.to_string(),
                            event_type: "m.room.encryption".to_string(),
                            content: json!({ "algorithm": algorithm }),
                            state_key: Some("".to_string()),
                            origin_server_ts: encryption_ts,
                        },
                        Some(&mut tx),
                    )
                    .await;
                if let Err(e) = result {
                    let _ = tx.rollback().await;
                    return Err(ApiError::internal_with_log("Failed to create m.room.encryption event", &e));
                }
            }
        }

        if let Some(ref jr) = initial_join_rule {
            if let Err(e) = sqlx::query("UPDATE rooms SET join_rules = $1 WHERE room_id = $2")
                .bind(jr)
                .bind(&room_id)
                .execute(&mut *tx)
                .await
            {
                ::tracing::warn!(error = %e, room_id = %room_id, join_rule = %jr, "Failed to update join_rules on rooms table");
            }
            join_rule = jr.as_str();
        }

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
                return Err(ApiError::internal_with_log("Failed to set privacy marker", &e));
            }
        }

        tx.commit().await.map_err(|e| ApiError::internal_with_log("Failed to commit transaction", &e))?;

        let summary_request = synapse_storage::room_summary::CreateRoomSummaryRequest {
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
        if let Err(e) = self.room_summary_service.create_summary(summary_request).await {
            ::tracing::warn!(
                error = %e,
                room_id = %room_id,
                room_type = ?config.room_type,
                join_rule = %join_rule,
                "Failed to create room summary"
            );
        }

        if let Some(ref alias) = config.room_alias_name {
            let full_alias = format!("#{}:{}", alias, self.server_name);
            validate_room_alias_input(&full_alias)?;
            if let Err(e) = self.room_storage.set_room_alias(&room_id, &full_alias, user_id).await {
                ::tracing::warn!(error = %e, room_id = %room_id, room_alias = %full_alias, user_id = %user_id, "Failed to save room alias");
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
            self.room_storage.create_room_in_tx(tx, room_id, user_id, join_rule, room_version, is_public).await
        } else {
            self.room_storage.create_room(room_id, user_id, join_rule, room_version, is_public).await
        };

        result.map(|_| ()).map_err(|e| ApiError::internal_with_log("Failed to create room", &e))
    }

    async fn add_creator_to_room(
        &self,
        room_id: &str,
        user_id: &str,
        tx: Option<&mut sqlx::Transaction<'_, sqlx::Postgres>>,
    ) -> ApiResult<()> {
        self.member_storage
            .add_member(room_id, user_id, "join", None, None, None, tx)
            .await
            .map_err(|e| ApiError::internal_with_log("Failed to add room member", &e))?;

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
                    .map_err(|e| ApiError::internal_with_log("Failed to update room name", &e))?;
            } else {
                self.room_storage
                    .update_room_name(room_id, room_name)
                    .await
                    .map_err(|e| ApiError::internal_with_log("Failed to update room name", &e))?;
            }
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
                .map_err(|e| ApiError::internal_with_log("Failed to create m.room.name event", &e))?;
        }

        if let Some(room_topic) = topic {
            if let Some(ref mut tx) = tx {
                self.room_storage
                    .update_room_topic_in_tx(tx, room_id, room_topic)
                    .await
                    .map_err(|e| ApiError::internal_with_log("Failed to update room topic", &e))?;
            } else {
                self.room_storage
                    .update_room_topic(room_id, room_topic)
                    .await
                    .map_err(|e| ApiError::internal_with_log("Failed to update room topic", &e))?;
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
                .map_err(|e| ApiError::internal_with_log("Failed to create m.room.topic event", &e))?;
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
                .map_err(|e| ApiError::internal_with_log("Failed to check users existence", &e))?;

            if let Some(ref mut t) = tx {
                let mut offset: i64 = 0;
                for invitee in invites {
                    if !existing_users.contains(invitee) {
                        ::tracing::warn!(
                            room_id = %room_id,
                            invitee = %invitee,
                            sender_user_id = %sender_user_id,
                            "Skipping invite for non-existent user"
                        );
                        continue;
                    }
                    self.member_storage
                        .add_member(room_id, invitee, "invite", None, None, Some(sender_user_id), Some(&mut **t))
                        .await
                        .map_err(|e| ApiError::internal_with_log("Failed to invite user", &e))?;
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
                        .map_err(|e| ApiError::internal_with_log("Failed to record m.room.member invite event", &e))?;
                    offset += 1;
                }
            } else {
                let mut offset: i64 = 0;
                for invitee in invites {
                    if !existing_users.contains(invitee) {
                        continue;
                    }
                    self.member_storage
                        .add_member(room_id, invitee, "invite", None, None, Some(sender_user_id), None)
                        .await
                        .map_err(|e| ApiError::internal_with_log("Failed to invite user", &e))?;
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
                        .map_err(|e| ApiError::internal_with_log("Failed to record m.room.member invite event", &e))?;
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
}
