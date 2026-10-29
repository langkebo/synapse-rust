//! Room creation logic extracted from `service.rs` for M-10 file size reduction.
//!
//! Contains `create_room` and small utility helpers.
//! Event creation helpers live in [`create_events`].

use super::super::service::CreateRoomConfig;
use super::super::utils::validate_room_alias_input;
use super::creation_graph::CreationGraph;
use super::service::LifecycleService;
use serde_json::json;
use std::collections::BTreeSet;
use synapse_common::room_id::room_id_from_create_event_id;
use synapse_common::room_versions::{is_supported_room_version, resolve_room_version, DEFAULT_ROOM_VERSION};
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
            // B8: `requested_version` is user-controlled (`config.room_version`
            // comes straight from the request body) and is emitted as a
            // Prometheus label value. Left unvalidated it would let a caller mint
            // unbounded distinct series and blow up metric cardinality, so the
            // label is restricted to the known room-version enumeration and every
            // other value is folded into `unknown`.
            let room_version_label = match requested_version.as_deref() {
                Some(version) if is_supported_room_version(version) => version,
                Some(_) => "unknown",
                None => DEFAULT_ROOM_VERSION,
            };
            metrics.record_room_operation_labeled("create", outcome, room_version_label, visibility, error_type);
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
        // Resolved here, before `create_content` is moved into the create event write,
        // and reused by every place that must keep creators out of `power_levels.users`
        // (MSC4289 rule 10.4): the default map, `power_level_content_override`, and
        // `initial_state`.
        let creators = creators_of(&create_content, user_id);

        // Every creatable room version derives its id from the create event
        // (MSC4291), so there is no pre-allocation escape hatch any more:
        // `CreateRoomConfig::room_id` was removed under C-5 / decision Q2(a).
        // The invariant is asserted rather than assumed, because a version that
        // carried a server-assigned id would derive the wrong room id below.
        let is_v12_plus = synapse_common::room_versions::room_version_at_least(room_version, 12);
        if !is_v12_plus {
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
                // The create event's content hash *is* the room id (MSC4291):
                // a rewrite would desync the identity already derived above.
                false,
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
            return Err(e.clone());
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
                true,
                Some(&mut tx),
            )
            .await;
        if let Err(e) = result {
            let _ = tx.rollback().await;
            return Err(e);
        }

        // MSC4289 (v12): the room-upgrade path must be out of reach of an ordinary PL-100
        // admin. A tombstone upgrades the room, and the upgrading user becomes the
        // **creator** of the replacement — so if an admin could send one, they could
        // launder themselves into creator privileges. Upstream (Synapse) therefore sets
        // `m.room.tombstone` to 150, above `state_default` (50) and above an admin (100);
        // only a creator (unlimited) clears it by default.
        let tombstone_level: i64 = if is_v12_plus { 150 } else { 100 };
        let mut power_levels = json!({
            "users": { user_id: 100 },
            "users_default": 0,
            "events": {
                "m.room.name": 50,
                "m.room.power_levels": 100,
                "m.room.history_visibility": 100,
                "m.room.canonical_alias": 50,
                "m.room.avatar": 50,
                "m.room.tombstone": tombstone_level,
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
        // MSC4289 rule 10.4: a v12+ `m.room.power_levels` must **not** name a creator in
        // `users`. The creator's power is unlimited (`AuthService::CREATOR_POWER_LEVEL`), so
        // the entry can never be enforced — but it does make the event itself invalid, and
        // that breaks every client that does a read-modify-write of this state:
        // `GET …/state/m.room.power_levels` hands the entry back, the client adds its own
        // change and PUTs the whole blob, and `verify_power_levels_change` rejects it with
        // 403 (`power_levels.users must not name a room creator`). `setPowerLevel` would
        // then be unusable in every room this server creates.
        //
        // Stripped **after** the `power_level_content_override` merge, because that merge
        // replaces the whole `users` map and real clients do send the creator at 100
        // (Element Call always does). Upstream does the same, for the same reason:
        // Synapse `_copy_and_remove_creators_from_pl_users_map`, Dendrite #509.
        if is_v12_plus {
            let stripped = strip_creators_from_power_levels(&mut power_levels, &creators);
            if !stripped.is_empty() {
                ::tracing::info!(
                    room_id = %room_id,
                    stripped_creators = ?stripped,
                    "stripped creator(s) from the initial m.room.power_levels.users (MSC4289 rule 10.4)"
                );
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
                true,
                Some(&mut tx),
            )
            .await;
        if let Err(e) = result {
            let _ = tx.rollback().await;
            return Err(e);
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
                true,
                Some(&mut tx),
            )
            .await;
        if let Err(e) = result {
            let _ = tx.rollback().await;
            return Err(e);
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
                true,
                Some(&mut tx),
            )
            .await;
        if let Err(e) = result {
            let _ = tx.rollback().await;
            return Err(e);
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
                true,
                Some(&mut tx),
            )
            .await;
        if let Err(e) = result {
            let _ = tx.rollback().await;
            return Err(e);
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
            return Err(e);
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
            return Err(e);
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
                let mut content = obj.get("content").cloned().unwrap_or_else(|| json!({}));

                // Third door into the same defect: `initial_state` may carry a
                // `m.room.power_levels` event (only create/member/tombstone are refused
                // above), and it is written verbatim through `write_creation_event` too —
                // so a client that lists the creator here would poison the room exactly the
                // same way as the default map does.
                if is_v12_plus && event_type == "m.room.power_levels" {
                    let stripped = strip_creators_from_power_levels(&mut content, &creators);
                    if !stripped.is_empty() {
                        ::tracing::info!(
                            room_id = %room_id,
                            stripped_creators = ?stripped,
                            "stripped creator(s) from an initial_state m.room.power_levels (MSC4289 rule 10.4)"
                        );
                    }
                }

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
                        true,
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
                    return Err(e);
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
                // B-6：Megolm **没有 break-in recovery**，一旦叶棘轮泄露，泄露前的内容
                // 在旧 session 里永久可解。缩短 session 生命周期是唯一可用的缓解，
                // 所以轮换参数必须显式写进状态事件（不写就等于交给客户端默认值，
                // 线上出现过不同客户端默认值不一致的情况）。
                let mut content = json!({ "algorithm": algorithm });
                if let Some(obj) = content.as_object_mut() {
                    if let Some(ms) = config.encryption_rotation_period_ms {
                        obj.insert("rotation_period_ms".to_string(), json!(ms));
                    }
                    if let Some(msgs) = config.encryption_rotation_period_msgs {
                        obj.insert("rotation_period_msgs".to_string(), json!(msgs));
                    }
                }
                let result = self
                    .write_creation_event(
                        &mut graph,
                        None,
                        &room_id,
                        user_id,
                        "m.room.encryption",
                        Some(""),
                        content,
                        encryption_ts,
                        true,
                        Some(&mut tx),
                    )
                    .await;
                if let Err(e) = result {
                    let _ = tx.rollback().await;
                    return Err(e);
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
                    true,
                    Some(&mut tx),
                )
                .await;
            if let Err(e) = result {
                let _ = tx.rollback().await;
                return Err(e);
            }
        }

        tx.commit().await.map_err(|e| ApiError::internal_with_cause("Failed to commit transaction", e))?;

        // A1：建房把整段初始状态直接写进事务（`write_creation_event`），绕过了
        // `MessagingService` 那条维护状态记录的缝。A1 之后当前状态读是 fail-closed 的，
        // 所以新房间必须在返回给客户端之前拿到记录，否则在首次「状态写」之前读不出状态
        // （`/state`、`/messages`、`/event`、`/context` 都会失败）。
        //
        // 必须在下方的 appservice 派发**之前** —— 那一步自己就要读房间状态。
        // best-effort：房间已经提交，这里失败不能回滚，只能告警；下一次状态写仍会播种。
        let state_groups = synapse_storage::state_groups::StateGroupStorage::new(self.room_storage.pool());
        let resolution_cache = crate::room::state_record::ResolutionCache::default();
        let record = crate::room::state_record::StateRecord {
            event_reader: self.event_reader.as_ref(),
            room_storage: self.room_storage.as_ref(),
            state_groups: &state_groups,
            resolution_cache: &resolution_cache,
        };
        if let Err(error) = record.seed_new_room(&room_id).await {
            ::tracing::warn!(
                error = %error,
                room_id = %room_id,
                "Failed to seed the freshly created room's state record"
            );
        }

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
        crate::room_state_cache::invalidate_room_state(&self.cache, &room_id).await;

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

/// The room's creator set, read from the create event content (MSC4289).
///
/// Wrapper rather than a re-implementation: "who is a creator" has exactly one
/// definition in this workspace (`synapse_common::room_creator`), shared with the
/// federation auth rules and [`crate::auth::AuthService`]. A second copy here would
/// drift the moment v12 adds another way to name a creator.
fn creators_of(create_content: &serde_json::Value, sender: &str) -> BTreeSet<String> {
    synapse_common::room_creator::creators_from_create_event(sender, create_content)
}

/// Drop every creator from an `m.room.power_levels` content's `users` map, returning
/// the user ids that were removed (empty when there was nothing to do).
///
/// MSC4289 rule 10.4 forbids naming a creator there, and the reason is not cosmetic:
/// a creator's power is unlimited (`AuthService::CREATOR_POWER_LEVEL`), so the number
/// can never be enforced — but its **presence** makes the event invalid, and this
/// server writes its initial `m.room.power_levels` through `write_creation_event`,
/// which does not run the auth check. Without this strip the room is born with a
/// state event that every client then reads back and re-sends verbatim, only to be
/// rejected with 403 (`power_levels.users must not name a room creator`).
///
/// Missing/non-object `users` is a no-op, not an error: the creation path is
/// deliberately tolerant here (a malformed override must not fail room creation;
/// the *client* write path is where malformed power levels get rejected).
fn strip_creators_from_power_levels(content: &mut serde_json::Value, creators: &BTreeSet<String>) -> Vec<String> {
    let Some(users) = content.get_mut("users").and_then(|users| users.as_object_mut()) else {
        return Vec::new();
    };
    let stripped: Vec<String> = users.keys().filter(|target| creators.contains(*target)).cloned().collect();
    for target in &stripped {
        users.remove(target);
    }
    stripped
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

    // ── MSC4289 rule 10.4：初始 power_levels 不得点名创建者 ──────────────────────

    #[test]
    fn strip_creators_removes_only_the_creator() {
        let mut power_levels = json!({
            "users": { "@alice:ex.com": 100, "@bob:ex.com": 50 },
            "state_default": 50,
        });
        let creators = BTreeSet::from(["@alice:ex.com".to_string()]);

        let stripped = strip_creators_from_power_levels(&mut power_levels, &creators);

        assert_eq!(stripped, vec!["@alice:ex.com".to_string()], "只该剥掉创建者本人");
        assert_eq!(power_levels["users"], json!({ "@bob:ex.com": 50 }), "非创建者条目必须原样保留");
        assert_eq!(power_levels["state_default"], json!(50), "其它键不能被连带改动");
    }

    #[test]
    fn strip_creators_also_removes_additional_creators() {
        // `additional_creators` 走同一条规则：它和 sender 一样属于创建者集合。
        let create_content = json!({
            "creator": "@alice:ex.com",
            "room_version": "12",
            "additional_creators": ["@co-admin:ex.com"],
        });
        let creators = creators_of(&create_content, "@alice:ex.com");
        let mut power_levels = json!({
            "users": { "@alice:ex.com": 100, "@co-admin:ex.com": 100, "@bob:ex.com": 50 },
        });

        let stripped = strip_creators_from_power_levels(&mut power_levels, &creators);

        assert_eq!(stripped.len(), 2, "sender 与 additional_creators 都要剥：{stripped:?}");
        assert_eq!(power_levels["users"], json!({ "@bob:ex.com": 50 }));
    }

    #[test]
    fn strip_creators_is_a_no_op_without_a_users_map() {
        let creators = BTreeSet::from(["@alice:ex.com".to_string()]);
        for mut content in [json!({}), json!({ "users": null }), json!({ "users": "not-an-object" })] {
            let before = content.clone();
            let stripped = strip_creators_from_power_levels(&mut content, &creators);
            assert!(stripped.is_empty(), "没有可用 users 时不应报告剥离：{content}");
            assert_eq!(content, before, "没有可用 users 时不得改动内容：{content}");
        }
    }
}
