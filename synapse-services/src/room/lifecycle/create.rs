//! Room creation logic extracted from `service.rs` for M-10 file size reduction.
//!
//! Contains `create_room` and small utility helpers.
//! Event creation helpers live in [`create_events`].

use super::super::service::CreateRoomConfig;
use super::super::utils::validate_room_alias_input;
use super::creation_graph::CreationGraph;
use super::service::LifecycleService;
use serde_json::json;
use synapse_common::room_id::room_id_from_create_event_id;
use synapse_common::room_versions::{resolve_room_version, DEFAULT_ROOM_VERSION};
use synapse_common::{generate_room_id, ApiError, ApiResult};

impl LifecycleService {
    /// See [`create_room`].
    ///
    /// Instrumentation wrapper: the whole creation flow is timed once here and
    /// reported through `room_creation_duration_seconds` +
    /// `room_operations_total{operation="create"}`. The body lives in
    /// [`create_room_inner`] so that every early `return Err(..)` inside it is
    /// covered by a single measurement point instead of ~20 hand-placed ones.
    pub async fn create_room(&self, user_id: &str, config: CreateRoomConfig) -> ApiResult<serde_json::Value> {
        let started = std::time::Instant::now();
        let requested_version = config.room_version.clone();
        let visibility = if Self::is_public_visibility(config.visibility.as_deref()) { "public" } else { "private" };

        let result = self.create_room_inner(user_id, config).await;

        if let Some(metrics) = synapse_common::server_metrics::global_server_metrics() {
            metrics.record_room_creation(started.elapsed().as_secs_f64());
            let (outcome, error_type) = match &result {
                Ok(_) => ("success", "none"),
                Err(e) => (
                    if e.kind == synapse_common::ApiErrorKind::Forbidden { "forbidden" } else { "error" },
                    e.code_str(),
                ),
            };
            metrics.record_room_operation_labeled(
                "create",
                outcome,
                requested_version.as_deref().unwrap_or(DEFAULT_ROOM_VERSION),
                visibility,
                error_type,
            );
        }

        result
    }

    /// Body of [`create_room`]; see that method's documentation.
    async fn create_room_inner(&self, user_id: &str, config: CreateRoomConfig) -> ApiResult<serde_json::Value> {
        if let Some(alias) = &config.room_alias_name {
            if let Err(e) = self.validator.validate_username(alias) {
                return Err(e.into());
            }
        }

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

        // MSC4291 (room v12+): the room id **is** the create event's id with the
        // sigil swapped, so it cannot be chosen before the create event exists.
        // Mint the create event's content here, derive its one and only id from
        // the content, and take the room id from it. The hash must not depend on
        // the room id or the derivation would be circular: `build_pdu` therefore
        // omits `room_id` for a v12+ create event (D-6), and the placeholder
        // passed below never reaches the hash.
        // ⚠️ C88：这里必须用**严格递增**的毫秒时钟，不能用 `current_timestamp_millis()`。
        // v12 的 room_id 由 create 事件的 reference hash 决定（MSC4291），而
        // `origin_server_ts` 参与该哈希；`build_create_event_content` 对"同 body 的请求"
        // 产生**完全相同**的内容 ⇒ 同一毫秒内并发的 createRoom（负载测试的常态）会派生出
        // **同一个** room_id，撞 `rooms` 主键（Phase 3 负载测试实测）。严格递增的时钟让每个
        // create 事件拿到不同的 `origin_server_ts`，派生因此天然唯一；跨进程（多 worker）不保证
        // 唯一，那一层由 `rooms` 插入的 `ON CONFLICT (room_id) DO NOTHING` 检出并转成显式 409
        // （见 `create_room_in_db` 的错误分类）。
        let now = synapse_common::current_timestamp_millis_monotonic();
        let create_content = build_create_event_content(user_id, room_version, &config);

        // Every creatable room version derives its id from the create event
        // (MSC4291), so there is no pre-allocation escape hatch any more:
        // `CreateRoomConfig::room_id` was removed under C-5 / decision Q2(a).
        // The invariant is asserted rather than assumed, because a version that
        // carried a server-assigned id would derive the wrong room id below.
        if !synapse_common::room_versions::room_version_at_least(room_version, 12) {
            return Err(ApiError::unsupported_room_version(format!(
                "room version {room_version} cannot be created: only a version whose id is derived from \
                 its create event (room v12+) is creatable"
            )));
        }

        // The create event's id is both the room id (sigil swapped) and the id
        // pinned into its own write, so a non-finalizing writer (the legacy
        // storage path) persists the same identity this derivation used.
        let (room_id, derived_create_event_id) = {
            let placeholder_room_id = self.generate_room_id();
            let parts = synapse_common::pdu::PduParts {
                room_version,
                event_id: None,
                room_id: &placeholder_room_id,
                sender: user_id,
                event_type: "m.room.create",
                content: &create_content,
                state_key: Some(""),
                origin_server_ts: now,
                origin: &self.server_name,
                // `CreationGraph::next` gives the first event of a fresh graph
                // `depth = 1`, and depth takes part in the reference hash.
                depth: 1,
                prev_events: &[],
                auth_events: &[],
                redacts: None,
            };
            let finalized = synapse_federation::event_finalize::finalize_local_pdu(&parts).map_err(|e| {
                ApiError::internal_with_cause("Failed to derive the v12 room id from the create event", e)
            })?;
            let room_id = room_id_from_create_event_id(&finalized.event_id)
                .map_err(|e| ApiError::internal(format!("The create event id is not a v12 reference hash: {e}")))?;
            (room_id, finalized.event_id)
        };

        // MSC4284: consult the policy server before beginning the transaction.
        // Placed before tx.begin() so we don't hold a DB transaction open during
        // the policy check HTTP request. No-op when no policy service is configured.
        self.check_create_policy(&room_id, user_id).await?;

        let mut tx = self
            .room_storage
            .pool()
            .begin()
            .await
            .map_err(|e| ApiError::internal_with_cause("Failed to start transaction", e))?;

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
            return Err(ApiError::internal_with_context("Failed to create room", e));
        }

        // The creation events below are emitted in one linear order inside this
        // transaction. The write-path decorator cannot resolve their DAG metadata
        // (transactional reads cannot see uncommitted rows), so the sequence is
        // tracked here — see `creation_graph`.
        let mut graph = CreationGraph::new(room_version);

        let result = self
            .write_creation_event(
                &mut graph,
                Some(derived_create_event_id.as_str()),
                &room_id,
                user_id,
                "m.room.create",
                Some(""),
                create_content,
                now,
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
            return Err(ApiError::internal_with_context("Failed to create m.room.create event", e));
        }
        // The create event's persisted identity must be the one the room id was
        // derived from, or the `rooms` row and its create event would disagree
        // about the room's identity (MSC4291). Divergence would mean the
        // derivation and the write path disagree, so fail the transaction rather
        // than persist an unresolvable room.
        if let Ok(create_event_id) = &result {
            let expected_room_id = synapse_common::room_id::room_id_from_create_event_id(create_event_id);
            if expected_room_id.as_deref() != Ok(room_id.as_str()) {
                ::tracing::error!(
                    room_id = %room_id,
                    create_event_id = %create_event_id,
                    "the v12 room id does not derive from the persisted create event"
                );
                let _ = tx.rollback().await;
                return Err(ApiError::internal(format!(
                    "the create event id {create_event_id} does not derive the room id {room_id}"
                )));
            }
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
            .write_creation_event(
                &mut graph,
                None,
                &room_id,
                user_id,
                "m.room.member",
                Some(user_id),
                json!({
                    "membership": "join",
                    "displayname": user_id.trim_start_matches('@').split(':').next().unwrap_or(user_id),
                }),
                now + 1,
                Some(&mut tx),
            )
            .await;
        if let Err(e) = result {
            let _ = tx.rollback().await;
            return Err(ApiError::internal_with_cause("Failed to create m.room.member event", e));
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
            .write_creation_event(
                &mut graph,
                None,
                &room_id,
                user_id,
                "m.room.power_levels",
                Some(""),
                power_levels,
                now + 2,
                Some(&mut tx),
            )
            .await;
        if let Err(e) = result {
            let _ = tx.rollback().await;
            return Err(ApiError::internal_with_cause("Failed to create m.room.power_levels event", e));
        }

        let result = self
            .write_creation_event(
                &mut graph,
                None,
                &room_id,
                user_id,
                "m.room.join_rules",
                Some(""),
                json!({ "join_rule": join_rule }),
                now + 3,
                Some(&mut tx),
            )
            .await;
        if let Err(e) = result {
            let _ = tx.rollback().await;
            return Err(ApiError::internal_with_cause("Failed to create m.room.join_rules event", e));
        }

        let history_visibility = config.history_visibility.clone().unwrap_or_else(|| {
            if is_trusted_private {
                "invited".to_string()
            } else {
                "shared".to_string()
            }
        });
        let result = self
            .write_creation_event(
                &mut graph,
                None,
                &room_id,
                user_id,
                "m.room.history_visibility",
                Some(""),
                json!({ "history_visibility": history_visibility }),
                now + 4,
                Some(&mut tx),
            )
            .await;
        if let Err(e) = result {
            let _ = tx.rollback().await;
            return Err(ApiError::internal_with_cause("Failed to create m.room.history_visibility event", e));
        }

        let guest_access = if is_public { "can_join" } else { "forbidden" };
        let result = self
            .write_creation_event(
                &mut graph,
                None,
                &room_id,
                user_id,
                "m.room.guest_access",
                Some(""),
                json!({ "guest_access": guest_access }),
                now + 5,
                Some(&mut tx),
            )
            .await;
        if let Err(e) = result {
            let _ = tx.rollback().await;
            return Err(ApiError::internal_with_cause("Failed to create m.room.guest_access event", e));
        }

        let result = self
            .set_room_metadata(
                &room_id,
                user_id,
                config.name.as_deref(),
                config.topic.as_deref(),
                now + 6,
                &mut graph,
                Some(&mut tx),
            )
            .await;
        if let Err(e) = result {
            let _ = tx.rollback().await;
            return Err(ApiError::internal_with_cause("Failed to set room metadata", e));
        }

        let result = self
            .process_invites(
                &room_id,
                config.invite_list.as_ref(),
                config.invite_reasons.as_ref(),
                user_id,
                now + 7,
                &mut graph,
                &mut tx,
            )
            .await;
        if let Err(e) = result {
            let _ = tx.rollback().await;
            return Err(ApiError::internal_with_cause("Failed to process invites", e));
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
                    .write_creation_event(
                        &mut graph,
                        None,
                        &room_id,
                        user_id,
                        event_type,
                        Some(&state_key),
                        content,
                        now + 9 + idx as i64,
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
                    return Err(ApiError::internal_with_context(
                        "Failed to apply initial_state event {event_type}",
                        &e,
                    ));
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
                    .write_creation_event(
                        &mut graph,
                        None,
                        &room_id,
                        user_id,
                        "m.room.encryption",
                        Some(""),
                        json!({ "algorithm": algorithm }),
                        encryption_ts,
                        Some(&mut tx),
                    )
                    .await;
                if let Err(e) = result {
                    let _ = tx.rollback().await;
                    return Err(ApiError::internal_with_cause("Failed to create m.room.encryption event", e));
                }
            }
        }

        if let Some(ref jr) = initial_join_rule {
            if let Err(e) = self.room_storage.update_join_rule_in_tx(&mut tx, &room_id, jr).await {
                ::tracing::warn!(error = %e, room_id = %room_id, join_rule = %jr, "Failed to update join_rules on rooms table");
            }
            join_rule = jr.as_str();
        }

        if is_trusted_private {
            let privacy_content = json!({ "action": "block_screenshot" });
            let result = self
                .write_creation_event(
                    &mut graph,
                    None,
                    &room_id,
                    user_id,
                    "com.hula.privacy",
                    Some(""),
                    privacy_content,
                    now + 8,
                    Some(&mut tx),
                )
                .await;
            if let Err(e) = result {
                let _ = tx.rollback().await;
                return Err(ApiError::internal_with_cause("Failed to set privacy marker", e));
            }
        }

        tx.commit().await.map_err(|e| ApiError::internal_with_cause("Failed to commit transaction", e))?;

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
        if let Some(ref svc) = self.room_summary_service {
            if let Err(e) = svc.create_summary(summary_request).await {
                ::tracing::warn!(
                    error = %e,
                    room_id = %room_id,
                    room_type = ?config.room_type,
                    join_rule = %join_rule,
                    "Failed to create room summary"
                );
            }
        }

        if let Some(ref alias) = config.room_alias_name {
            let full_alias = format!("#{}:{}", alias, self.server_name);
            validate_room_alias_input(&full_alias)?;
            if let Err(e) = self.room_storage.set_room_alias(&room_id, &full_alias, user_id).await {
                ::tracing::warn!(error = %e, room_id = %room_id, room_alias = %full_alias, user_id = %user_id, "Failed to save room alias");
            }
        }

        let room_alias = self.format_room_alias(config.room_alias_name.as_deref());

        // Invalidate room-state cache after room creation writes initial state.
        let _ = self.cache.delete(&format!("room_state:{room_id}")).await;

        // After the transaction commits, enqueue the initial state events for
        // any matching application services.  Events created inside the
        // transaction bypass the messaging layer's appservice dispatch, so we
        // replay them here.
        self.dispatch_appservice_events_for_room(&room_id).await;

        Ok(Self::build_room_response(&room_id, room_alias.as_deref()))
    }

    fn generate_room_id(&self) -> String {
        generate_room_id(&self.server_name)
    }

    /// See [`determine_join_rule`].
    pub(crate) fn determine_join_rule(preset: Option<&str>) -> &'static str {
        match preset {
            Some("public_chat") => "public",
            _ => "invite",
        }
    }

    /// See [`is_public_visibility`].
    pub(crate) fn is_public_visibility(visibility: Option<&str>) -> bool {
        visibility.unwrap_or("private") == "public"
    }

    /// See [`format_room_alias`].
    pub(crate) fn format_room_alias(&self, room_alias_name: Option<&str>) -> Option<String> {
        room_alias_name.map(|a| format!("#{}:{}", a, self.server_name))
    }

    /// See [`build_room_response`].
    pub(crate) fn build_room_response(room_id: &str, room_alias: Option<&str>) -> serde_json::Value {
        json!({
            "room_id": room_id,
            "room_alias": room_alias
        })
    }

    /// After `create_room` commits its transaction, replay the room's
    /// initial state events to any matching application services.
    ///
    /// Events created inside the transaction bypass the messaging layer's
    /// appservice dispatch (which only fires when no transaction is
    /// supplied).  This method queries the committed state events and
    /// enqueues each one so bridges receive the full room creation payload.
    async fn dispatch_appservice_events_for_room(&self, room_id: &str) {
        let Some(app_service_manager) = &self.app_service_manager else {
            return;
        };

        let state_events = match self.event_reader.get_state_events(room_id).await {
            Ok(events) => events,
            Err(error) => {
                ::tracing::warn!(
                    error = %error,
                    room_id = %room_id,
                    "Failed to load state events for appservice dispatch after room creation"
                );
                return;
            }
        };

        for event in state_events {
            if let Err(error) = app_service_manager
                .enqueue_matching_event(
                    &event.event_id,
                    &event.room_id,
                    event.event_type.as_deref().unwrap_or(""),
                    &event.sender,
                    &event.content,
                    event.state_key.as_deref(),
                )
                .await
            {
                ::tracing::warn!(
                    error = %error,
                    event_id = %event.event_id,
                    room_id = %event.room_id,
                    event_type = ?event.event_type,
                    "Failed to enqueue application service event after room creation"
                );
            }
        }
    }
}

/// Build the `m.room.create` event content from the room creation config.
///
/// Extracted as a pure function so the content shape (including the
/// `is_direct` flag required by MSC vectors for DM rooms) can be unit-tested
/// without a database. Per the Matrix spec, `is_direct` is only emitted when
/// it is `Some(true)` — clients treat its absence as `false`.
fn build_create_event_content(user_id: &str, room_version: &str, config: &CreateRoomConfig) -> serde_json::Value {
    let mut create_content = json!({
        "creator": user_id,
        "room_version": room_version,
    });
    if let Some(extra) = config.creation_content.as_ref().and_then(|v| v.as_object()) {
        if let Some(map) = create_content.as_object_mut() {
            for (k, v) in extra {
                // `room_version` and `creator` are reserved and set above.
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
    // P0-1: `is_direct` must be present in `m.room.create` so other clients
    // (and the SDK) can identify the room as a direct message. Previously
    // this flag was only persisted to the room summary, which left the create
    // event non-compliant and broke DM detection on the client side.
    if config.is_direct == Some(true) {
        create_content["is_direct"] = json!(true);
    }
    create_content
}

#[cfg(test)]
mod tests {
    use super::*;

    // ── determine_join_rule ────────────────────────────────────────

    #[test]
    fn determine_join_rule_public_chat() {
        assert_eq!(LifecycleService::determine_join_rule(Some("public_chat")), "public");
    }

    #[test]
    fn determine_join_rule_private_chat() {
        assert_eq!(LifecycleService::determine_join_rule(Some("private_chat")), "invite");
    }

    #[test]
    fn determine_join_rule_none() {
        assert_eq!(LifecycleService::determine_join_rule(None), "invite");
    }

    #[test]
    fn determine_join_rule_unknown_preset() {
        assert_eq!(LifecycleService::determine_join_rule(Some("trusted_private_chat")), "invite");
    }

    // ── is_public_visibility ────────────────────────────────────────

    #[test]
    fn is_public_visibility_public() {
        assert!(LifecycleService::is_public_visibility(Some("public")));
    }

    #[test]
    fn is_public_visibility_private() {
        assert!(!LifecycleService::is_public_visibility(Some("private")));
    }

    #[test]
    fn is_public_visibility_none_defaults_private() {
        assert!(!LifecycleService::is_public_visibility(None));
    }

    // ── build_room_response ─────────────────────────────────────────

    #[test]
    fn build_room_response_with_alias() {
        let resp = LifecycleService::build_room_response("!room:ex.com", Some("#alias:ex.com"));
        assert_eq!(resp["room_id"], "!room:ex.com");
        assert_eq!(resp["room_alias"], "#alias:ex.com");
    }

    #[test]
    fn build_room_response_without_alias() {
        let resp = LifecycleService::build_room_response("!room:ex.com", None);
        assert_eq!(resp["room_id"], "!room:ex.com");
        assert!(resp["room_alias"].is_null());
    }

    // ── build_create_event_content (P0-1 regression) ────────────────

    #[test]
    fn build_create_event_content_includes_is_direct_when_true() {
        let config = CreateRoomConfig { is_direct: Some(true), ..Default::default() };
        let content = build_create_event_content("@alice:ex.com", "11", &config);
        assert_eq!(content["is_direct"], json!(true));
        assert_eq!(content["creator"], "@alice:ex.com");
        assert_eq!(content["room_version"], "11");
    }

    #[test]
    fn build_create_event_content_omits_is_direct_when_false() {
        let config = CreateRoomConfig { is_direct: Some(false), ..Default::default() };
        let content = build_create_event_content("@alice:ex.com", "11", &config);
        assert!(content.get("is_direct").is_none(), "is_direct must be absent when Some(false)");
    }

    #[test]
    fn build_create_event_content_omits_is_direct_when_none() {
        let config = CreateRoomConfig { is_direct: None, ..Default::default() };
        let content = build_create_event_content("@alice:ex.com", "11", &config);
        assert!(content.get("is_direct").is_none(), "is_direct must be absent when None");
    }

    #[test]
    fn build_create_event_content_merges_creation_content_extras() {
        let config = CreateRoomConfig {
            is_direct: Some(true),
            room_type: Some("m.direct".to_string()),
            creation_content: Some(json!({ "m.federate": false, "creator": "should_be_ignored" })),
            ..Default::default()
        };
        let content = build_create_event_content("@alice:ex.com", "11", &config);
        assert_eq!(content["is_direct"], json!(true));
        assert_eq!(content["type"], "m.direct");
        assert_eq!(content["m.federate"], json!(false));
        // Reserved keys in creation_content must not override the canonical ones.
        assert_eq!(content["creator"], "@alice:ex.com");
        assert_eq!(content["room_version"], "11");
    }
}
