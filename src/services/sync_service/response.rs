use super::types::*;
use super::SyncService;
use crate::common::*;
use crate::e2ee::device_keys::DeviceKeyStorage;
use crate::map_internal;
use crate::storage::{RoomEvent, StateEvent};
use serde_json::{json, Map, Value};
use std::collections::{HashMap, HashSet};

impl SyncService {
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
        let since_stream_ordering = since_token
            .as_ref()
            .filter(|t| t.stream_id < Self::TIMESTAMP_TOKEN_MIN && t.stream_id > 0)
            .map(|t| t.stream_id);
        let (changed_members_by_room, state_change_ts_by_room) = if is_incremental {
            let state_ts_result = if let Some(stream_ord) = since_stream_ordering {
                self.event_storage
                    .get_state_change_timestamps_since_stream_batch(room_ids, stream_ord)
                    .await
                    .map_err(ApiError::from)?
            } else {
                self.event_storage
                    .get_state_change_timestamps_batch(room_ids, since_ts)
                    .await
                    .map_err(ApiError::from)?
            };
            if lazy_load_members {
                let changed_members = if let Some(stream_ord) = since_stream_ordering {
                    self.event_storage
                        .get_membership_state_keys_since_stream_batch(room_ids, stream_ord)
                        .await
                        .map_err(ApiError::from)?
                } else {
                    self.event_storage
                        .get_membership_state_keys_since_batch(room_ids, since_ts)
                        .await
                        .map_err(ApiError::from)?
                };
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
        for room_id in &rooms_to_include {
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
                }
            }
        }

        let stream_id = Self::next_event_stream_id(since_token, &room_events, Some(&state_change_ts_by_room));
        let device_one_time_keys_count = self.build_device_one_time_keys_count(user_id, device_id).await?;

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
                "invite": {},
                "leave": left_rooms
            },
            "presence": { "events": presence_events },
            "account_data": { "events": account_data_events },
            "to_device": { "events": to_device_events },
            "device_lists": device_lists,
            "device_one_time_keys_count": device_one_time_keys_count,
            "key_rotation_needed": key_rotation_needed,
            "device_list_changes": device_list_changes
        }))
    }

    async fn build_device_one_time_keys_count(&self, user_id: &str, device_id: Option<&str>) -> ApiResult<Value> {
        let Some(device_id) = device_id else {
            return Ok(json!({}));
        };

        let device_key_storage = DeviceKeyStorage::new(&self.device_storage.pool);
        let counts = device_key_storage
            .get_one_time_keys_count_by_algorithm(user_id, device_id)
            .await
            .map_err(map_internal!("Failed to load one-time key count"))?;

        let mut result = serde_json::Map::new();
        for (algo, count) in counts {
            result.insert(algo, json!(count));
        }

        Ok(Value::Object(result))
    }

    async fn build_key_rotation_needed(&self, user_id: &str) -> ApiResult<Value> {
        let rotation_storage = crate::e2ee::key_rotation::KeyRotationStorage::new(self.event_storage.pool.clone());

        let rooms = rotation_storage
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
        let device_key_storage = DeviceKeyStorage::new(&self.device_storage.pool);

        for uid in &changed_users {
            if let Ok(count) = device_key_storage.get_device_count(uid).await {
                user_device_counts.insert(
                    uid.clone(),
                    json!({
                        "device_count": count,
                        "change_type": "changed"
                    }),
                );
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

    pub(crate) async fn build_room_sync(&self, request: BuildRoomSyncRequest<'_>) -> ApiResult<serde_json::Value> {
        let BuildRoomSyncRequest { room_id, user_id, device_id, events, since_token, is_incremental, room_filter } =
            request;
        let since_ts = Self::event_since_ts(&since_token.cloned());
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
                    self.event_storage
                        .get_membership_state_keys_since_batch(&[room_id.to_string()], since_ts)
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
                            since_stream_ordering: None,
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
        let state_list = if is_incremental { Vec::new() } else { state_list };
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

    pub(crate) fn event_to_json(event: &RoomEvent, event_format: SyncEventFormat) -> Value {
        let now = chrono::Utc::now().timestamp_millis();
        let age = now.saturating_sub(event.origin_server_ts);

        let mut obj = json!({
            "type": event.event_type,
            "content": event.content,
            "sender": event.user_id,
            "origin_server_ts": event.origin_server_ts,
            "event_id": event.event_id,
            "room_id": event.room_id,
            "unsigned": {
                "age": age
            }
        });

        if let Some(ref state_key) = event.state_key {
            obj["state_key"] = json!(state_key);
        }

        if event_format == SyncEventFormat::Federation {
            obj["depth"] = json!(event.depth);
            obj["origin"] = json!(event.origin);
        }

        obj
    }

    pub(crate) fn state_event_to_json(event: &StateEvent, event_format: SyncEventFormat) -> Value {
        let now = chrono::Utc::now().timestamp_millis();
        let sender = event.user_id.as_deref().unwrap_or(&event.sender);
        let age = now.saturating_sub(event.origin_server_ts);
        let event_type = event.event_type.as_deref().unwrap_or("m.room.message");
        let mut obj = json!({
            "type": event_type,
            "content": event.content,
            "sender": sender,
            "origin_server_ts": event.origin_server_ts,
            "event_id": event.event_id,
            "room_id": event.room_id,
            "unsigned": {
                "age": age
            }
        });
        if let Some(ref state_key) = event.state_key {
            obj["state_key"] = json!(state_key);
        }
        if event_format == SyncEventFormat::Federation {
            obj["depth"] = json!(event.depth);
            obj["origin"] = json!(event.origin);
        }
        obj
    }

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
            || format!("t{}", chrono::Utc::now().timestamp_millis()),
            |event| format!("t{}", event.origin_server_ts),
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
}
