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
            use_state_after,
            state_after_is_unstable,
        } = request;
        let room_filter = response_filter.and_then(|filter| filter.room.as_ref());
        let event_fields = response_filter.and_then(|filter| filter.event_fields.as_deref());
        let event_format = response_filter.map(|filter| filter.event_format).unwrap_or_default();
        let lazy_load_members = Self::room_filter_requests_lazy_members(room_filter);

        // MSC4222: the room-level `state` ↔ `state_after` rename is applied in
        // `build_room_sync_value`; no extra query is needed because the local
        // state batch is already `stream_ordering > since` **without an upper
        // bound** (see `get_state_events_since_batch`), i.e. it already covers
        // the whole timeline — exactly what `state_after` means.
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

        // MSC4354: Pre-fetch all room IDs where this user has sticky events.
        // This is a cheap DISTINCT query; if it fails we fail-open (empty set).
        let sticky_rooms: HashSet<String> = self.get_sticky_event_rooms(user_id).await.into_iter().collect();
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
        let mut knocked_rooms = Map::new();

        // P3-fix: collect the stripped-state rooms (invite + knock) and pre-fetch
        // their state in one batch per section, instead of issuing 8 queries per
        // room inside the loop below.
        let rooms_in_section = |section: SyncRoomSection| -> Vec<String> {
            rooms_to_include
                .iter()
                .filter(|room_id| room_sections.get(*room_id).copied() == Some(section))
                .cloned()
                .collect()
        };
        let invited_room_ids = rooms_in_section(SyncRoomSection::Invite);
        let knocked_room_ids = rooms_in_section(SyncRoomSection::Knock);
        let mut invited_stripped: HashMap<String, Vec<synapse_storage::event::StateEvent>> =
            if invited_room_ids.is_empty() {
                HashMap::new()
            } else {
                self.build_rooms_stripped_state(&invited_room_ids).await
            };
        let mut knocked_stripped: HashMap<String, Vec<synapse_storage::event::StateEvent>> =
            if knocked_room_ids.is_empty() {
                HashMap::new()
            } else {
                self.build_rooms_stripped_state(&knocked_room_ids).await
            };

        for room_id in &rooms_to_include {
            // MSC4311: invited/knocked rooms get stripped state (including
            // m.room.create) instead of full room sync. Skip the full sync
            // pipeline for them — a knocked room must never be rendered as joined.
            match room_sections.get(room_id).copied() {
                Some(SyncRoomSection::Invite) => {
                    let events = invited_stripped.remove(room_id).unwrap_or_default();
                    Self::insert_room_stripped_state(
                        &mut invited_rooms,
                        room_id,
                        &events,
                        user_id,
                        "invite_state",
                        "invite",
                    );
                    continue;
                }
                Some(SyncRoomSection::Knock) => {
                    let events = knocked_stripped.remove(room_id).unwrap_or_default();
                    Self::insert_room_stripped_state(
                        &mut knocked_rooms,
                        room_id,
                        &events,
                        user_id,
                        "knock_state",
                        "knock",
                    );
                    continue;
                }
                _ => {}
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
            let mut room_sync = Self::build_room_sync_value(BuildRoomSyncValueRequest {
                events,
                state_list: state_events,
                ephemeral_events,
                account_data_events,
                timeline_limit,
                counts: RoomSyncCounts { highlight_count, notification_count },
                event_fields,
                event_format,
                use_state_after,
                state_after_is_unstable,
            });

            // MSC4354: inject sticky_events for v2 /sync response.
            // Only fetch for rooms that have sticky events configured, avoiding
            // a query on every room of a wide initial sync (the sticky_rooms
            // set was pre-fetched in the try_join above).
            if let Some(storage) = self.sticky_event_storage.as_ref() {
                if sticky_rooms.iter().any(|r| r == room_id) {
                    if let Ok(events) = storage.get_all_is_sticky_events(room_id, user_id).await {
                        if !events.is_empty() {
                            if let Some(obj) = room_sync.as_object_mut() {
                                let sticky_json: Vec<serde_json::Value> = events
                                    .iter()
                                    .map(|e| {
                                        json!({
                                            "event_type": e.event_type,
                                            "event_id": e.event_id,
                                            "is_sticky": e.is_sticky,
                                        })
                                    })
                                    .collect();
                                obj.insert("sticky_events".to_string(), json!(sticky_json));
                            }
                        }
                    }
                }
            }

            if room_sync.is_object() && !room_sync.as_object().is_some_and(|o| o.is_empty()) {
                match room_sections.get(room_id).copied().unwrap_or(SyncRoomSection::Join) {
                    SyncRoomSection::Join => {
                        joined_rooms.insert(room_id.clone(), room_sync);
                    }
                    SyncRoomSection::Leave => {
                        left_rooms.insert(room_id.clone(), room_sync);
                    }
                    // MSC4311: Invite/Knock rooms are handled above via stripped
                    // state and never reach this match. The arms are unreachable
                    // but required for exhaustiveness; fail-closed by ignoring.
                    SyncRoomSection::Invite | SyncRoomSection::Knock => {}
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
                "knock": knocked_rooms,
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
        let BuildRoomSyncRequest {
            room_id,
            user_id,
            device_id,
            events,
            since_token,
            is_incremental,
            room_filter,
            use_state_after,
            state_after_is_unstable,
        } = request;
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
            use_state_after,
            state_after_is_unstable,
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
            use_state_after,
            state_after_is_unstable,
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

        // MSC4222: with `?use_state_after=true` the room carries `state_after`
        // (state changes up to the **end** of this timeline) **instead of**
        // `state` (changes up to its start), and the field MUST be present even
        // when empty. The payload is the same one the `state` section would
        // carry: the local state batch is `stream_ordering > since` with no upper
        // bound, so it already spans the whole timeline — do not compute it twice.
        let state_key = if use_state_after {
            if state_after_is_unstable {
                "org.matrix.msc4222.state_after"
            } else {
                "state_after"
            }
        } else {
            "state"
        };

        let mut value = json!({
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
        });
        if state_key != "state" {
            if let Some(object) = value.as_object_mut() {
                if let Some(state) = object.remove("state") {
                    object.insert(state_key.to_string(), state);
                }
            }
        }
        value
    }

    /// State event types required for stripped state per Matrix spec.
    /// `m.room.create` is first — its absence triggers the fail-closed omission.
    const STRIPPED_STATE_TYPES: &'static [&'static str] = &[
        "m.room.create",
        "m.room.join_rules",
        "m.room.name",
        "m.room.avatar",
        "m.room.topic",
        "m.room.encryption",
        "m.room.canonical_alias",
        "m.room.member",
    ];

    /// P3-fix: pre-fetch stripped state for **all** invited rooms at once.
    ///
    /// Previously the caller looped per room and each call issued
    /// `STRIPPED_STATE_TYPES.len()` (8) separate `get_state_events_by_type`
    /// queries, so a user invited to N rooms cost **8×N** queries inside one
    /// `/sync`. This hoists the lookups into one batch call per state type
    /// (**8 queries total**, plus one call per type per room-batch), i.e. the
    /// query count no longer scales with the number of invited rooms.
    ///
    /// Degradation is unchanged but now explicit: if the batch read for a type
    /// fails, the events for that type degrade to "absent" and we log it. A room
    /// that consequently has no `m.room.create` is still omitted from the invite
    /// section (MSC4311 fail-closed), matching the old per-room behaviour — but a
    /// single DB error no longer silently produces an empty stripped state with
    /// no log line.
    async fn build_rooms_stripped_state(
        &self,
        room_ids: &[String],
    ) -> HashMap<String, Vec<synapse_storage::event::StateEvent>> {
        let mut per_room: HashMap<String, Vec<synapse_storage::event::StateEvent>> = HashMap::new();

        for event_type in Self::STRIPPED_STATE_TYPES {
            match self.event_reader.get_state_events_by_type_batch(room_ids, event_type).await {
                Ok(by_room) => {
                    for (room_id, events) in by_room {
                        per_room.entry(room_id).or_default().extend(events);
                    }
                }
                Err(e) => {
                    ::tracing::warn!(
                        event_type = %event_type,
                        rooms = room_ids.len(),
                        error = %e,
                        "stripped-state batch read failed; that state type will be absent from invite_state/knock_state"
                    );
                }
            }
        }

        per_room
    }

    /// Assemble `{"<section_key>": {"events": [...]}}` from already-loaded state.
    ///
    /// Pure (no I/O) so the fail-closed rule is directly unit-testable:
    /// returns `None` when `m.room.create` is absent, because the invitee/knocker
    /// then cannot determine the room version (MSC4311).
    ///
    /// `section_key` is `invite_state` for `rooms.invite` and `knock_state` for
    /// `rooms.knock` (C-S spec: the two sections carry the same stripped shape
    /// under different keys).
    fn assemble_room_stripped_state(
        events: &[synapse_storage::event::StateEvent],
        user_id: &str,
        section_key: &str,
    ) -> Option<Value> {
        // P-18 / upstream shape: the server that invited (or was knocked at by)
        // us may have supplied the stripped state, which we stored on the
        // membership event's `unsigned.invite_room_state` /
        // `unsigned.knock_room_state`. Prefer it — for a first-contact invite the
        // room has **no state of ours** to project, and the old fail-closed rule
        // then dropped the invite from `/sync` entirely, i.e. the invitee saw
        // nothing at all.
        let unsigned_key = match section_key {
            "invite_state" => "invite_room_state",
            "knock_state" => "knock_room_state",
            _ => section_key,
        };
        if let Some(supplied) = events
            .iter()
            .find(|event| {
                event.event_type.as_deref() == Some("m.room.member")
                    && event.state_key.as_deref().is_some_and(|state_key| state_key == user_id)
            })
            .and_then(|event| event.unsigned.as_ref())
            .and_then(|unsigned| unsigned.get(unsigned_key))
            .and_then(Value::as_array)
            .filter(|entries| !entries.is_empty())
        {
            return Some(json!({
                section_key: { "events": supplied.clone() }
            }));
        }

        let mut stripped_events = Vec::new();
        let mut has_create = false;

        for event in events {
            // For m.room.member, only include the invitee's own invite event.
            if event.event_type.as_deref() == Some("m.room.member") {
                let matches_invitee = event.state_key.as_deref().is_some_and(|sk| sk == user_id);
                if !matches_invitee {
                    continue;
                }
            }

            if event.event_type.as_deref() == Some("m.room.create") {
                has_create = true;
            }

            stripped_events.push(Self::state_event_to_stripped_value(event));
        }

        if !has_create {
            return None;
        }

        Some(json!({
            section_key: {
                "events": stripped_events
            }
        }))
    }

    /// Insert one room's stripped state into its section map, fail-closed when
    /// `m.room.create` is missing (the peer then cannot know the room version).
    /// Shared by `rooms.invite` and `rooms.knock` so the two cannot drift.
    fn insert_room_stripped_state(
        target: &mut Map<String, Value>,
        room_id: &str,
        events: &[synapse_storage::event::StateEvent],
        user_id: &str,
        section_key: &str,
        section_label: &str,
    ) {
        match Self::assemble_room_stripped_state(events, user_id, section_key) {
            Some(stripped) => {
                target.insert(room_id.to_string(), stripped);
            }
            None => {
                ::tracing::warn!(
                    room_id = %room_id,
                    user_id = %user_id,
                    section = section_label,
                    "No sender-supplied stripped state and our own room state has no m.room.create; \
                     omitting from section (fail-closed) — see P-18"
                );
            }
        }
    }

    /// Convert a `StateEvent` to the stripped state event JSON format used
    /// in the `invite_state.events` / `knock_state.events` array of the sync response.
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

    /// MSC4354 helper: pre-fetch all room IDs where the given user has
    /// at least one sticky event configured.
    ///
    /// Returns `Vec<String>` of room IDs with sticky events for `user_id`.
    /// The query is fail-open: if `sticky_event_storage` is `None` or
    /// returns an error, an empty vector is returned so /sync succeeds
    /// without sticky data rather than failing the whole request.
    async fn get_sticky_event_rooms(&self, user_id: &str) -> Vec<String> {
        match &self.sticky_event_storage {
            Some(storage) => match storage.get_rooms_with_is_sticky_events(user_id).await {
                Ok(rooms) => rooms,
                Err(e) => {
                    ::tracing::warn!(
                        user_id = %user_id,
                        error = %e,
                        "Failed to fetch sticky event rooms; returning empty set (fail-open)"
                    );
                    Vec::new()
                }
            },
            None => Vec::new(),
        }
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
            prev_events: None,
            auth_events: None,
            signatures: None,
            hashes: None,
        }
    }

    /// Helper: build a state event of a given type/state_key.
    fn state_event_of(event_type: &str, state_key: Option<&str>) -> StateEvent {
        let mut ev = make_state_event();
        ev.event_type = Some(event_type.to_string());
        ev.state_key = state_key.map(|s| s.to_string());
        ev.event_id = format!("${event_type}:ex.com");
        ev
    }

    /// Fail-closed rule (MSC4311): without `m.room.create` the invitee cannot
    /// determine the room version, so the room must be omitted from the invite
    /// section rather than sent without critical state.
    #[test]
    fn stripped_state_is_none_without_create_event() {
        let events = vec![state_event_of("m.room.name", Some(""))];
        assert!(
            SyncService::assemble_room_stripped_state(&events, "@bob:ex.com", "invite_state").is_none(),
            "absence of m.room.create must yield None (fail-closed)"
        );
    }

    /// With `m.room.create` present the stripped state is produced and carries it.
    #[test]
    fn stripped_state_is_some_with_create_event() {
        let events = vec![state_event_of("m.room.create", Some("")), state_event_of("m.room.name", Some(""))];
        let value = SyncService::assemble_room_stripped_state(&events, "@bob:ex.com", "invite_state")
            .expect("m.room.create present ⇒ must assemble");
        let types: Vec<&str> = value["invite_state"]["events"]
            .as_array()
            .expect("events array")
            .iter()
            .filter_map(|e| e["type"].as_str())
            .collect();
        assert!(types.contains(&"m.room.create"), "create must be present: {types:?}");
        assert!(types.contains(&"m.room.name"), "name must be present: {types:?}");
    }

    /// H-2：knock 段与 invite 段**同一份 stripped state 形状**，只是键不同
    /// （`knock_state` vs `invite_state`）；`m.room.create` 缺失时同样 fail-closed。
    #[test]
    fn knock_stripped_state_uses_knock_state_key_and_keeps_create() {
        let events = vec![state_event_of("m.room.create", Some("")), state_event_of("m.room.name", Some(""))];
        let value = SyncService::assemble_room_stripped_state(&events, "@bob:ex.com", "knock_state")
            .expect("m.room.create present ⇒ must assemble");
        assert!(value.get("knock_state").is_some(), "key 必须是 knock_state：{value}");
        assert!(value.get("invite_state").is_none(), "不得复用 invite_state 键：{value}");
        let types: Vec<&str> = value["knock_state"]["events"]
            .as_array()
            .expect("events array")
            .iter()
            .filter_map(|e| e["type"].as_str())
            .collect();
        assert!(types.contains(&"m.room.create"), "create 必须在：{types:?}");

        // fail-closed 与 invite 一致：缺 create 就不下发该房间。
        let without_create = vec![state_event_of("m.room.name", Some(""))];
        assert!(
            SyncService::assemble_room_stripped_state(&without_create, "@bob:ex.com", "knock_state").is_none(),
            "缺 m.room.create 的 knock 房间必须被省略（与 invite 同一判据）"
        );
    }

    /// `insert_room_stripped_state` 是 invite/knock 两段共用的唯一写入点：
    /// 它必须把房间放进传入的 map（而不是"构造出来却没人用"）。
    #[test]
    fn insert_room_stripped_state_populates_the_target_map() {
        let events = vec![state_event_of("m.room.create", Some(""))];
        let mut target: serde_json::Map<String, serde_json::Value> = serde_json::Map::new();
        SyncService::insert_room_stripped_state(
            &mut target,
            "!knock:b",
            &events,
            "@bob:ex.com",
            "knock_state",
            "knock",
        );
        assert!(target.contains_key("!knock:b"), "房间必须进入目标段：{target:?}");
        assert!(target["!knock:b"].get("knock_state").is_some());
    }

    /// Only the invitee's own `m.room.member` event belongs in stripped state;
    /// other members' membership events must not leak the room's member list.
    #[test]
    fn stripped_state_keeps_only_the_invitee_member_event() {
        let events = vec![
            state_event_of("m.room.create", Some("")),
            state_event_of("m.room.member", Some("@bob:ex.com")),
            state_event_of("m.room.member", Some("@carol:ex.com")),
        ];
        let value =
            SyncService::assemble_room_stripped_state(&events, "@bob:ex.com", "invite_state").expect("create present");
        let state_keys: Vec<&str> = value["invite_state"]["events"]
            .as_array()
            .expect("events array")
            .iter()
            .filter(|e| e["type"] == "m.room.member")
            .filter_map(|e| e["state_key"].as_str())
            .collect();
        assert_eq!(
            state_keys,
            vec!["@bob:ex.com"],
            "only the invitee's membership event may be included (no member-list leak)"
        );
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
            use_state_after: false,
            state_after_is_unstable: false,
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
            use_state_after: false,
            state_after_is_unstable: false,
        };
        let value = SyncService::build_room_sync_value(request);
        let prev_batch = value["timeline"]["prev_batch"].as_str().unwrap();
        assert_eq!(prev_batch, "t1700000000000");
    }

    // ── MSC4222: `state_after` opt-in（规范形状）──────────────────────────
    // proposals/4222：客户端 `?use_state_after=true` opt-in 后，房间段**省略 `state`**、
    // 改为 `state_after`（**必须出现**，可为空）；用不稳定拼写 opt-in 时响应字段镜像为
    // `org.matrix.msc4222.state_after`。未 opt-in 的默认路径逐字不变（现有 /sync 快照即回归网）。

    /// Build a minimal JSON state event value (mimics what `state_event_to_json` produces).
    fn make_state_event_value(event_id: &str, event_type: &str, origin_server_ts: i64) -> Value {
        json!({
            "type": event_type,
            "event_id": event_id,
            "origin_server_ts": origin_server_ts,
            "sender": "@alice:ex.com",
            "content": {},
        })
    }

    fn state_after_value_request(
        use_state_after: bool,
        state_after_is_unstable: bool,
        state_list: Vec<Value>,
    ) -> BuildRoomSyncValueRequest<'static> {
        BuildRoomSyncValueRequest {
            events: Vec::new(),
            state_list,
            ephemeral_events: Vec::new(),
            account_data_events: Vec::new(),
            timeline_limit: 10,
            counts: RoomSyncCounts { highlight_count: 0, notification_count: 0 },
            event_fields: None,
            event_format: SyncEventFormat::Client,
            use_state_after,
            state_after_is_unstable,
        }
    }

    #[test]
    fn msc4222_opt_in_replaces_state_with_state_after() {
        let state = vec![make_state_event_value("$s:ex.com", "m.room.name", 1000)];
        let value = SyncService::build_room_sync_value(state_after_value_request(true, false, state));
        assert!(value.get("state").is_none(), "opt-in 后必须**省略** `state`：{value}");
        let events = value["state_after"]["events"].as_array().expect("state_after.events 必须是数组");
        assert_eq!(events.len(), 1, "state_after 必须带上本次同步的状态变化");
        assert_eq!(events[0]["event_id"], "$s:ex.com");
    }

    #[test]
    fn msc4222_unstable_opt_in_mirrors_the_unstable_field_name() {
        let value = SyncService::build_room_sync_value(state_after_value_request(
            true,
            true,
            vec![make_state_event_value("$s:ex.com", "m.room.name", 1000)],
        ));
        assert!(value.get("state").is_none(), "不稳定拼写同样要省略 `state`");
        assert!(
            value.get("org.matrix.msc4222.state_after").is_some(),
            "用不稳定参数 opt-in 时字段必须镜像为 org.matrix.msc4222.state_after：{value}"
        );
        assert!(value.get("state_after").is_none(), "不得同时给出稳定名：{value}");
    }

    #[test]
    fn msc4222_state_after_is_present_even_when_empty() {
        // 规范：支持该 MSC 的服务端**必须**返回该字段，即使为空。
        let value = SyncService::build_room_sync_value(state_after_value_request(true, false, Vec::new()));
        assert!(value.get("state_after").is_some(), "空也必须出现：{value}");
        assert_eq!(value["state_after"]["events"].as_array().expect("array").len(), 0);
    }

    #[test]
    fn msc4222_default_keeps_state_and_never_emits_state_after() {
        let state = vec![make_state_event_value("$s:ex.com", "m.room.name", 1000)];
        let value = SyncService::build_room_sync_value(state_after_value_request(false, false, state));
        assert_eq!(value["state"]["events"].as_array().expect("array").len(), 1, "默认路径必须照旧：{value}");
        assert!(value.get("state_after").is_none(), "未 opt-in 不得出现 state_after：{value}");
        assert!(value.get("org.matrix.msc4222.state_after").is_none());
    }
}
