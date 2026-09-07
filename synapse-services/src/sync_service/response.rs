use super::types::*;
use super::SyncService;
use crate::map_internal;
use crate::*;
use serde_json::{json, Map, Value};
use std::collections::{HashMap, HashSet};
use synapse_common::current_timestamp_millis;
use synapse_common::*;
use synapse_storage::event::SinceFilter;

impl SyncService {
    /// See [`build_sync_response`].
    pub(crate) async fn build_sync_response(
        &self,
        request: BuildSyncResponseRequest<'_>,
    ) -> ApiResult<serde_json::Value> {
        let BuildSyncResponseRequest {
            user_id,
            device_id,
            room_ids,
            room_sections,
            room_events,
            response_filter,
            timeline_limit,
            since_token,
            is_incremental,
        } = request;
        let room_filter = response_filter.and_then(|filter| filter.room.as_ref());
        let event_fields = response_filter.and_then(|filter| filter.event_fields.as_deref());
        let event_format = response_filter.map(|filter| filter.event_format).unwrap_or_default();
        let lazy_load_members = Self::room_filter_requests_lazy_members(room_filter);
        let since_ts = Self::event_since_ts(since_token);
        // S6: always use StreamOrdering. Timestamp-based tokens are converted
        // to 0 for a full resync, eliminating the OriginServerTs path.
        let since_stream_ordering = since_token.as_ref().map(|t| {
            if t.stream_id > 0 && t.stream_id < Self::TIMESTAMP_TOKEN_MIN {
                t.stream_id
            } else {
                0
            }
        });
        let (changed_members_by_room, state_change_ts_by_room) = if is_incremental {
            let state_ts_result = self
                .event_reader
                .get_state_change_timestamps_batch(
                    room_ids,
                    SinceFilter::StreamOrdering(since_stream_ordering.unwrap_or(0)),
                )
                .await
                .map_err(ApiError::from)?;
            if lazy_load_members {
                let changed_members = self
                    .event_reader
                    .get_membership_state_keys_since_batch(
                        room_ids,
                        SinceFilter::StreamOrdering(since_stream_ordering.unwrap_or(0)),
                    )
                    .await
                    .map_err(ApiError::from)?;
                (changed_members, state_ts_result)
            } else {
                (HashMap::<String, HashSet<String>>::new(), state_ts_result)
            }
        } else {
            (HashMap::<String, HashSet<String>>::new(), HashMap::<String, i64>::new())
        };
        let rooms_to_include = Self::rooms_to_include(
            room_ids,
            &room_events,
            &changed_members_by_room,
            &state_change_ts_by_room,
            is_incremental,
        );
        let changed_members_by_room = if is_incremental && lazy_load_members {
            changed_members_by_room
                .into_iter()
                .filter(|(room_id, _)| rooms_to_include.iter().any(|candidate| candidate == room_id))
                .collect::<HashMap<_, _>>()
        } else {
            HashMap::new()
        };
        let state_change_ts_by_room = if is_incremental {
            state_change_ts_by_room
                .into_iter()
                .filter(|(room_id, _)| rooms_to_include.iter().any(|candidate| candidate == room_id))
                .collect::<HashMap<_, _>>()
        } else {
            HashMap::new()
        };
        let (
            state_by_room,
            ephemeral_by_room,
            room_account_data_by_room,
            unread_counts_by_room,
            presence_events,
            account_data_events,
            (to_device_events, to_device_stream_id),
            (device_lists, device_list_stream_id),
        ) = tokio::try_join!(
            self.get_state_events_for_sync_batch(
                &rooms_to_include,
                event_format,
                StateEventsBatchParams { since_ts, since_stream_ordering, is_incremental, lazy_load_members, user_id },
            ),
            self.get_room_ephemeral_events_batch(&rooms_to_include),
            self.get_room_account_data_events_batch(user_id, &rooms_to_include),
            self.get_unread_counts_batch(&rooms_to_include, user_id),
            self.get_presence_events(user_id, since_token),
            self.get_account_data_events(user_id),
            self.get_to_device_events(user_id, device_id, since_token),
            self.get_device_lists(user_id, since_token),
        )?;
        let presence_events = Self::apply_sync_filter_to_values(
            presence_events,
            response_filter.and_then(|filter| filter.presence.as_ref()),
        );
        let presence_events = Self::apply_event_fields_to_values(presence_events, event_fields);
        let account_data_events = Self::apply_event_fields_to_values(account_data_events, event_fields);
        let to_device_events = Self::apply_event_fields_to_values(to_device_events, event_fields);

        let mut joined_rooms = Map::new();
        let mut left_rooms = Map::new();
        let mut invited_rooms = Map::new();
        for room_id in &rooms_to_include {
            // MSC4311: invited rooms get stripped state (including m.room.create)
            // instead of full room sync. Skip the full sync pipeline for them.
            if room_sections.get(room_id).copied() == Some(SyncRoomSection::Invite) {
                let stripped = self.build_invited_room_stripped_state(room_id, user_id).await;
                if let Some(stripped) = stripped {
                    invited_rooms.insert(room_id.clone(), stripped);
                }
                continue;
            }

            let events = room_events.get(room_id).cloned().unwrap_or_default();
            let (timeline_events, timeline_limited) = Self::apply_timeline_limit(&events, timeline_limit);
            let state_events = Self::apply_sync_filter_to_values(
                state_by_room.get(room_id).cloned().unwrap_or_default(),
                room_filter.and_then(|filter| filter.state.as_ref()),
            );
            let state_events = self
                .apply_lazy_load_members(LazyLoadMembersRequest {
                    state_events,
                    timeline_events: &timeline_events,
                    user_id,
                    device_id,
                    room_id,
                    room_filter,
                    changed_member_ids: changed_members_by_room.get(room_id),
                    timeline_limited,
                    enabled: lazy_load_members,
                })
                .await;
            let state_events = Self::apply_event_fields_to_values(state_events, event_fields);
            let ephemeral_events = Self::apply_sync_filter_to_values(
                ephemeral_by_room.get(room_id).cloned().unwrap_or_default(),
                room_filter.and_then(|filter| filter.ephemeral.as_ref()),
            );
            let ephemeral_events = Self::apply_event_fields_to_values(ephemeral_events, event_fields);
            let account_data_events = Self::apply_sync_filter_to_values(
                room_account_data_by_room.get(room_id).cloned().unwrap_or_default(),
                room_filter.and_then(|filter| filter.account_data.as_ref()),
            );
            let account_data_events = Self::apply_event_fields_to_values(account_data_events, event_fields);
            let (highlight_count, notification_count) = unread_counts_by_room.get(room_id).copied().unwrap_or((0, 0));
            let room_sync = Self::build_room_sync_value(BuildRoomSyncValueRequest {
                events,
                state_list: state_events,
                ephemeral_events,
                account_data_events,
                timeline_limit,
                counts: RoomSyncCounts { highlight_count, notification_count },
                event_fields,
                event_format,
            });

            if room_sync.is_object() && !room_sync.as_object().is_some_and(|o| o.is_empty()) {
                match room_sections.get(room_id).copied().unwrap_or(SyncRoomSection::Join) {
                    SyncRoomSection::Join => {
                        joined_rooms.insert(room_id.clone(), room_sync);
                    }
                    SyncRoomSection::Leave => {
                        left_rooms.insert(room_id.clone(), room_sync);
                    }
                    // MSC4311: Invite rooms are handled above via stripped state
                    // and never reach this match. The arm is unreachable but
                    // required for exhaustiveness; fail-closed by ignoring.
                    SyncRoomSection::Invite => {}
                }
            }
        }

        let stream_id = Self::next_event_stream_id(since_token, &room_events, Some(&state_change_ts_by_room));
        let device_one_time_keys_count = self.build_device_one_time_keys_count(user_id, device_id).await?;

        let device_unused_fallback_key_types = self.build_device_unused_fallback_key_types(user_id, device_id).await?;

        let key_rotation_needed = self.build_key_rotation_needed(user_id).await?;

        let device_list_changes = self.build_device_list_changes(user_id, &device_lists).await?;

        Ok(json!({
            "next_batch": SyncToken {
                stream_id,
                room_id: None,
                event_type: None,
                to_device_stream_id: Some(to_device_stream_id),
                device_list_stream_id: Some(device_list_stream_id),
            }.encode(),
            "rooms": {
                "join": joined_rooms,
                "invite": invited_rooms,
                "leave": left_rooms
            },
            "presence": { "events": presence_events },
            "account_data": { "events": account_data_events },
            "to_device": { "events": to_device_events },
            "device_lists": device_lists,
            "device_one_time_keys_count": device_one_time_keys_count,
            "device_unused_fallback_key_types": device_unused_fallback_key_types,
            "key_rotation_needed": key_rotation_needed,
            "device_list_changes": device_list_changes
        }))
    }

    async fn build_device_one_time_keys_count(&self, user_id: &str, device_id: Option<&str>) -> ApiResult<Value> {
        let Some(device_id) = device_id else {
            return Ok(json!({}));
        };

        let counts = self
            .device_key_storage
            .get_one_time_keys_count_by_algorithm(user_id, device_id)
            .await
            .map_err(map_internal!("Failed to load one-time key count"))?;

        let mut result = serde_json::Map::new();
        for (algo, count) in counts {
            result.insert(algo, json!(count));
        }

        Ok(Value::Object(result))
    }

    async fn build_device_unused_fallback_key_types(&self, user_id: &str, device_id: Option<&str>) -> ApiResult<Value> {
        let Some(device_id) = device_id else {
            return Ok(json!([]));
        };

        let types = self
            .device_key_storage
            .get_unused_fallback_key_types(user_id, device_id)
            .await
            .map_err(map_internal!("Failed to load unused fallback key types"))?;

        Ok(json!(types))
    }

    async fn build_key_rotation_needed(&self, user_id: &str) -> ApiResult<Value> {
        let rooms = self
            .key_rotation_storage
            .get_rooms_needing_key_rotation(user_id)
            .await
            .map_err(map_internal!("Failed to get rooms needing key rotation"))?;

        Ok(json!({
            "rooms": rooms
        }))
    }

    async fn build_device_list_changes(&self, _user_id: &str, device_lists: &Value) -> ApiResult<Value> {
        let changed_users: Vec<String> = device_lists
            .get("changed")
            .and_then(|v| v.as_array())
            .map(|arr| arr.iter().filter_map(|v| v.as_str().map(|s| s.to_string())).collect())
            .unwrap_or_default();

        let left_users: Vec<String> = device_lists
            .get("left")
            .and_then(|v| v.as_array())
            .map(|arr| arr.iter().filter_map(|v| v.as_str().map(|s| s.to_string())).collect())
            .unwrap_or_default();

        let mut user_device_counts = serde_json::Map::new();

        if !changed_users.is_empty() {
            let counts = match self.device_key_storage.get_device_counts_batch(&changed_users).await {
                Ok(counts) => counts,
                Err(error) => {
                    ::tracing::warn!(
                        user_count = changed_users.len(),
                        error = %error,
                        "Failed to load device counts for device list changes; omitting device_count"
                    );
                    HashMap::new()
                }
            };
            for uid in &changed_users {
                if let Some(count) = counts.get(uid) {
                    user_device_counts.insert(
                        uid.clone(),
                        json!({
                            "device_count": count,
                            "change_type": "changed"
                        }),
                    );
                }
            }
        }

        for uid in &left_users {
            user_device_counts.insert(
                uid.clone(),
                json!({
                    "change_type": "left"
                }),
            );
        }

        Ok(json!({
            "users": serde_json::Value::Object(user_device_counts),
            "changed_count": changed_users.len(),
            "left_count": left_users.len()
        }))
    }

    /// See [`build_room_sync`].
    pub(crate) async fn build_room_sync(&self, request: BuildRoomSyncRequest<'_>) -> ApiResult<serde_json::Value> {
        let BuildRoomSyncRequest { room_id, user_id, device_id, events, since_token, is_incremental, room_filter } =
            request;
        let since_ts = Self::event_since_ts(&since_token.cloned());
        // S6: always use StreamOrdering for membership state key queries.
        let since_stream_ord = since_token
            .as_ref()
            .map(|t| if t.stream_id > 0 && t.stream_id < Self::TIMESTAMP_TOKEN_MIN { t.stream_id } else { 0 })
            .unwrap_or(0);
        let (
            changed_member_ids,
            state_list,
            ephemeral_events,
            account_data_events,
            (highlight_count, notification_count),
        ) = tokio::try_join!(
            async {
                let lazy_load_members = Self::room_filter_requests_lazy_members(room_filter);
                if is_incremental && lazy_load_members {
                    self.event_reader
                        .get_membership_state_keys_since_batch(
                            &[room_id.to_string()],
                            SinceFilter::StreamOrdering(since_stream_ord),
                        )
                        .await
                        .map(|mut room_map| room_map.remove(room_id).unwrap_or_default())
                        .map_err(Into::into)
                } else {
                    Ok(HashSet::new())
                }
            },
            async {
                let lazy_load_members = Self::room_filter_requests_lazy_members(room_filter);
                let state_by_room = self
                    .get_state_events_for_sync_batch(
                        &[room_id.to_string()],
                        SyncEventFormat::Client,
                        StateEventsBatchParams {
                            since_ts,
                            since_stream_ordering: Some(since_stream_ord),
                            is_incremental,
                            lazy_load_members,
                            user_id,
                        },
                    )
                    .await?;
                Ok(state_by_room.get(room_id).cloned().unwrap_or_default())
            },
            self.get_room_ephemeral_events(room_id, user_id),
            self.get_room_account_data_events(room_id, user_id),
            self.get_unread_counts(room_id, user_id),
        )?;

        let (timeline_events, timeline_limited) = Self::apply_timeline_limit(&events, self.sync_event_limit());
        let lazy_load_members = Self::room_filter_requests_lazy_members(room_filter);
        let state_list = Self::apply_sync_filter_to_values(state_list, room_filter.and_then(|f| f.state.as_ref()));
        let state_list = self
            .apply_lazy_load_members(LazyLoadMembersRequest {
                state_events: state_list,
                timeline_events: &timeline_events,
                user_id,
                device_id,
                room_id,
                room_filter,
                changed_member_ids: Some(&changed_member_ids),
                timeline_limited,
                enabled: lazy_load_members,
            })
            .await;
        // state_list already contains the delta computed by
        // get_state_events_for_sync_batch above; pass it through unchanged.
        // (Previously this line cleared the list for incremental syncs, which
        // broke MSC3967 incremental-state semantics on the per-room code path.)
        let ephemeral_events =
            Self::apply_sync_filter_to_values(ephemeral_events, room_filter.and_then(|f| f.ephemeral.as_ref()));
        let account_data_events =
            Self::apply_sync_filter_to_values(account_data_events, room_filter.and_then(|f| f.account_data.as_ref()));

        Ok(Self::build_room_sync_value(BuildRoomSyncValueRequest {
            events,
            state_list,
            ephemeral_events,
            account_data_events,
            timeline_limit: self.sync_event_limit(),
            counts: RoomSyncCounts { highlight_count, notification_count },
            event_fields: None,
            event_format: SyncEventFormat::Client,
        }))
    }

    /// See [`event_to_json`].
    pub(crate) fn event_to_json(event: &RoomEvent, event_format: SyncEventFormat) -> Value {
        let mut obj = crate::sync_helpers::room_event_to_json(event);
        if event_format == SyncEventFormat::Federation {
            obj["depth"] = json!(event.depth);
            obj["origin"] = json!(event.origin);
        }
        obj
    }

    /// See [`state_event_to_json`].
    pub(crate) fn state_event_to_json(event: &StateEvent, event_format: SyncEventFormat) -> Value {
        let mut obj = crate::sync_helpers::state_event_to_json(event);
        if event_format == SyncEventFormat::Federation {
            obj["depth"] = json!(event.depth);
            obj["origin"] = json!(event.origin);
        }
        obj
    }

    /// See [`build_room_sync_value`].
    pub(crate) fn build_room_sync_value(request: BuildRoomSyncValueRequest<'_>) -> Value {
        let BuildRoomSyncValueRequest {
            events,
            state_list,
            ephemeral_events,
            account_data_events,
            timeline_limit,
            counts,
            event_fields,
            event_format,
        } = request;
        let (events, limited) = Self::apply_timeline_limit(&events, timeline_limit);
        let event_list: Vec<Value> = events
            .iter()
            .map(|event| Self::filter_event_fields(Self::event_to_json(event, event_format), event_fields))
            .collect();
        let prev_batch = events.first().map_or_else(
            || generate_pagination_token(current_timestamp_millis(), None),
            |event| generate_pagination_token(event.origin_server_ts, event.stream_ordering),
        );

        json!({
            "state": {
                "events": state_list
            },
            "timeline": {
                "events": event_list,
                "limited": limited,
                "prev_batch": prev_batch
            },
            "ephemeral": {
                "events": ephemeral_events
            },
            "account_data": {
                "events": account_data_events
            },
            "unread_notifications": {
                "highlight_count": counts.highlight_count,
                "notification_count": counts.notification_count
            }
        })
    }

    /// Build the stripped state for an invited room (MSC4311).
    ///
    /// Returns `{"invite_state": {"events": [...]}}` containing the key
    /// state events the invitee needs to render the room preview and
    /// determine the room version (via `m.room.create`).
    ///
    /// Per the Matrix spec, the stripped state includes:
    /// m.room.create, m.room.join_rules, m.room.name, m.room.avatar,
    /// m.room.topic, m.room.encryption, m.room.canonical_alias,
    /// m.room.member (the invitee's invite event).
    ///
    /// Returns `None` if `m.room.create` cannot be loaded (fail-closed:
    /// without room version info the client cannot safely process the invite).
    async fn build_invited_room_stripped_state(&self, room_id: &str, user_id: &str) -> Option<Value> {
        // State event types required for stripped state per Matrix spec.
        // m.room.create is first — its absence triggers fail-closed return.
        const STRIPPED_STATE_TYPES: &[&str] = &[
            "m.room.create",
            "m.room.join_rules",
            "m.room.name",
            "m.room.avatar",
            "m.room.topic",
            "m.room.encryption",
            "m.room.canonical_alias",
            "m.room.member",
        ];

        let mut stripped_events = Vec::new();
        let mut has_create = false;

        for event_type in STRIPPED_STATE_TYPES {
            let state_events = self.event_reader.get_state_events_by_type(room_id, event_type).await.ok()?;

            for event in state_events {
                // For m.room.member, only include the invitee's invite event.
                if *event_type == "m.room.member" {
                    let matches_invitee = event.state_key.as_deref().is_some_and(|sk| sk == user_id);
                    if !matches_invitee {
                        continue;
                    }
                }

                if *event_type == "m.room.create" {
                    has_create = true;
                }

                stripped_events.push(Self::state_event_to_stripped_value(&event));
            }
        }

        // MSC4311 fail-closed: if m.room.create is missing, the invitee
        // cannot determine the room version. Return None so the room is
        // omitted from the invite section rather than sent without critical
        // state.
        if !has_create {
            ::tracing::warn!(
                room_id = %room_id,
                user_id = %user_id,
                "Stripped state for invited room missing m.room.create, omitting from invite section (fail-closed)"
            );
            return None;
        }

        Some(json!({
            "invite_state": {
                "events": stripped_events
            }
        }))
    }

    /// Convert a `StateEvent` to the stripped state event JSON format used
    /// in the `invite_state.events` array of the sync response.
    fn state_event_to_stripped_value(event: &synapse_storage::event::StateEvent) -> Value {
        json!({
            "type": event.event_type,
            "state_key": event.state_key,
            "sender": event.sender,
            "content": event.content,
            "event_id": event.event_id,
            "origin_server_ts": event.origin_server_ts
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    use synapse_storage::event::{RoomEvent, StateEvent};

    fn make_room_event() -> RoomEvent {
        RoomEvent {
            event_id: "$ev1:ex.com".into(),
            room_id: "!room:ex.com".into(),
            user_id: "@alice:ex.com".into(),
            event_type: "m.room.message".into(),
            content: json!({"body": "hello"}),
            state_key: None,
            depth: 5,
            origin_server_ts: 1700000000000,
            processed_ts: 1700000001000,
            not_before: 0,
            status: None,
            origin: "ex.com".into(),
            stream_ordering: Some(100),
            redacts: None,
        }
    }

    // ── event_to_json ────────────────────────────────────────────────

    #[test]
    fn event_to_json_client_format() {
        let event = make_room_event();
        let json = SyncService::event_to_json(&event, SyncEventFormat::Client);
        assert_eq!(json["type"], "m.room.message");
        assert_eq!(json["event_id"], "$ev1:ex.com");
        assert_eq!(json["room_id"], "!room:ex.com");
        assert_eq!(json["sender"], "@alice:ex.com");
        assert_eq!(json["content"]["body"], "hello");
        assert!(json["unsigned"]["age"].is_i64());
        // Client format excludes depth/origin
        assert!(json.get("depth").is_none());
        assert!(json.get("origin").is_none());
    }

    #[test]
    fn event_to_json_federation_format() {
        let event = make_room_event();
        let json = SyncService::event_to_json(&event, SyncEventFormat::Federation);
        assert_eq!(json["depth"], 5);
        assert_eq!(json["origin"], "ex.com");
    }

    #[test]
    fn event_to_json_with_state_key() {
        let mut event = make_room_event();
        event.state_key = Some("".into());
        let json = SyncService::event_to_json(&event, SyncEventFormat::Client);
        assert_eq!(json["state_key"], "");
    }

    // ── state_event_to_json ──────────────────────────────────────────

    fn make_state_event() -> StateEvent {
        StateEvent {
            event_id: "$se1:ex.com".into(),
            room_id: "!room:ex.com".into(),
            sender: "@alice:ex.com".into(),
            event_type: Some("m.room.member".into()),
            content: json!({"membership": "join"}),
            state_key: Some("@alice:ex.com".into()),
            unsigned: None,
            is_redacted: None,
            origin_server_ts: 1700000000000,
            depth: Some(3),
            processed_ts: Some(1700000001000),
            not_before: None,
            status: None,
            origin: Some("ex.com".into()),
            user_id: None,
            stream_ordering: None,
        }
    }

    #[test]
    fn state_event_to_json_client_format() {
        let event = make_state_event();
        let json = SyncService::state_event_to_json(&event, SyncEventFormat::Client);
        assert_eq!(json["type"], "m.room.member");
        assert_eq!(json["event_id"], "$se1:ex.com");
        assert_eq!(json["state_key"], "@alice:ex.com");
        assert_eq!(json["sender"], "@alice:ex.com");
        assert_eq!(json["content"]["membership"], "join");
        assert!(json["unsigned"]["age"].is_i64());
        assert!(json.get("depth").is_none());
    }

    #[test]
    fn state_event_to_json_falls_back_to_sender() {
        let mut event = make_state_event();
        event.user_id = Some("@bob:ex.com".into());
        let json = SyncService::state_event_to_json(&event, SyncEventFormat::Client);
        assert_eq!(json["sender"], "@bob:ex.com");
    }

    #[test]
    fn state_event_to_json_falls_back_to_default_event_type() {
        let mut event = make_state_event();
        event.event_type = None;
        let json = SyncService::state_event_to_json(&event, SyncEventFormat::Client);
        assert_eq!(json["type"], "m.room.message");
    }

    // ── build_room_sync_value ─────────────────────────────────────────

    #[test]
    fn build_room_sync_value_prev_batch_uses_composite_token() {
        // ISSUE 2.1.1: prev_batch 必须为复合 `t{ts}_{stream}` 格式，
        // 与 /messages 的 generate_pagination_token 对齐，避免 backfill
        // 时同毫秒事件被跳过。
        let event = make_room_event(); // origin_server_ts=1700000000000, stream_ordering=Some(100)
        let request = BuildRoomSyncValueRequest {
            events: vec![event],
            state_list: vec![],
            ephemeral_events: vec![],
            account_data_events: vec![],
            timeline_limit: 10,
            counts: RoomSyncCounts { highlight_count: 0, notification_count: 0 },
            event_fields: None,
            event_format: SyncEventFormat::Client,
        };
        let value = SyncService::build_room_sync_value(request);
        let prev_batch = value["timeline"]["prev_batch"].as_str().unwrap();
        assert_eq!(prev_batch, "t1700000000000_100");
    }

    #[test]
    fn build_room_sync_value_prev_batch_falls_back_to_legacy_when_no_stream() {
        // 无 stream_ordering 的事件退化为 legacy `t{ts}`，保持向后兼容。
        let mut event = make_room_event();
        event.stream_ordering = None;
        let request = BuildRoomSyncValueRequest {
            events: vec![event],
            state_list: vec![],
            ephemeral_events: vec![],
            account_data_events: vec![],
            timeline_limit: 10,
            counts: RoomSyncCounts { highlight_count: 0, notification_count: 0 },
            event_fields: None,
            event_format: SyncEventFormat::Client,
        };
        let value = SyncService::build_room_sync_value(request);
        let prev_batch = value["timeline"]["prev_batch"].as_str().unwrap();
        assert_eq!(prev_batch, "t1700000000000");
    }
}
