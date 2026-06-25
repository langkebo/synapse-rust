use super::types::*;
use super::SyncService;
use crate::map_internal;
use synapse_common::*;
use synapse_storage::{AccountDataStorage, RoomAccountDataStorage};

use serde_json::{json, Value};
use std::collections::{HashMap, HashSet};

impl SyncService {
    pub(crate) async fn update_presence(&self, user_id: &str, set_presence: &str) -> ApiResult<()> {
        self.presence_storage.set_presence(user_id, set_presence, None).await.ok();
        Ok(())
    }

    pub(crate) fn aggregate_ephemeral_events(events: Vec<serde_json::Value>) -> Vec<serde_json::Value> {
        let events_len = events.len();
        let mut receipt_content = serde_json::Map::new();
        let mut typing_events: Vec<serde_json::Value> = Vec::with_capacity(8);

        for event in events {
            let event_type = event.get("type").and_then(|v| v.as_str()).unwrap_or("");
            match event_type {
                "m.receipt" => {
                    if let Some(content) = event.get("content").and_then(|v| v.as_object()) {
                        for (event_id, receipt_data) in content {
                            let entry = receipt_content
                                .entry(event_id.clone())
                                .or_insert_with(|| serde_json::Value::Object(serde_json::Map::new()));
                            if let Some(entry_obj) = entry.as_object_mut() {
                                if let Some(data_obj) = receipt_data.as_object() {
                                    for (receipt_type, user_data) in data_obj {
                                        entry_obj.insert(receipt_type.clone(), user_data.clone());
                                    }
                                }
                            }
                        }
                    }
                }
                "m.typing" => {
                    typing_events.push(event);
                }
                _ => {
                    typing_events.push(event);
                }
            }
        }

        let mut result: Vec<serde_json::Value> = Vec::with_capacity(events_len.min(64));

        if !receipt_content.is_empty() {
            result.push(json!({
                "type": "m.receipt",
                "content": serde_json::Value::Object(receipt_content)
            }));
        }

        result.extend(typing_events);
        result
    }

    pub(crate) async fn get_room_state_events_batch(
        &self,
        room_ids: &[String],
        event_format: SyncEventFormat,
    ) -> ApiResult<HashMap<String, Vec<Value>>> {
        let state_events = self
            .event_storage
            .get_state_events_batch(room_ids)
            .await
            .map_err(map_internal!("Failed to get room state events"))?;

        Ok(state_events
            .into_iter()
            .map(|(room_id, events)| {
                let values = events.iter().map(|event| Self::state_event_to_json(event, event_format)).collect();
                (room_id, values)
            })
            .collect())
    }

    pub(crate) async fn get_state_events_for_sync_batch(
        &self,
        room_ids: &[String],
        event_format: SyncEventFormat,
        params: StateEventsBatchParams<'_>,
    ) -> ApiResult<HashMap<String, Vec<Value>>> {
        if room_ids.is_empty() {
            return Ok(HashMap::new());
        }

        if !params.is_incremental {
            return self.get_room_state_events_batch(room_ids, event_format).await;
        }

        let delta_state_by_room = if let Some(stream_ord) = params.since_stream_ordering {
            self.event_storage
                .get_state_events_since_stream_batch(room_ids, stream_ord)
                .await
                .map_err(|e| ApiError::internal_with_log("Failed to get room state events", &e))?
        } else {
            self.event_storage
                .get_state_events_since_batch(room_ids, params.since_ts)
                .await
                .map_err(|e| ApiError::internal_with_log("Failed to get room state events", &e))?
        };

        let newly_visible_rooms: Vec<String> = delta_state_by_room
            .iter()
            .filter_map(|(room_id, events)| {
                let user_just_joined = events.iter().any(|e| {
                    e.event_type.as_deref() == Some("m.room.member")
                        && e.state_key.as_deref() == Some(params.user_id)
                        && matches!(e.content.get("membership").and_then(|v| v.as_str()), Some("join") | Some("invite"))
                        && e.stream_ordering.is_some()
                });
                if user_just_joined {
                    Some(room_id.clone())
                } else {
                    None
                }
            })
            .collect();

        let full_state_for_newly_visible = if newly_visible_rooms.is_empty() {
            HashMap::new()
        } else {
            self.event_storage
                .get_state_events_batch(&newly_visible_rooms)
                .await
                .map_err(|e| ApiError::internal_with_log("Failed to get full state for newly visible rooms", &e))?
        };

        if !params.lazy_load_members {
            let mut result: HashMap<String, Vec<Value>> = HashMap::new();
            for (room_id, events) in delta_state_by_room {
                if let Some(full_state) = full_state_for_newly_visible.get(&room_id) {
                    let values =
                        full_state.iter().map(|event| Self::state_event_to_json(event, event_format)).collect();
                    result.insert(room_id, values);
                } else {
                    let values = events.iter().map(|event| Self::state_event_to_json(event, event_format)).collect();
                    result.insert(room_id, values);
                }
            }
            return Ok(result);
        }

        let current_member_state_by_room = self
            .event_storage
            .get_state_events_by_type_batch(room_ids, "m.room.member")
            .await
            .map_err(map_internal!("Failed to get room state events"))?;

        let mut result = HashMap::new();
        for room_id in room_ids {
            let mut values = Vec::new();

            if let Some(full_state) = full_state_for_newly_visible.get(room_id) {
                for event in full_state {
                    if event.event_type.as_deref() == Some("m.room.member") {
                        continue;
                    }
                    values.push(Self::state_event_to_json(event, event_format));
                }
            } else {
                for event in delta_state_by_room.get(room_id).into_iter().flatten() {
                    if event.event_type.as_deref() == Some("m.room.member") {
                        continue;
                    }
                    values.push(Self::state_event_to_json(event, event_format));
                }
            }

            for event in current_member_state_by_room.get(room_id).into_iter().flatten() {
                values.push(Self::state_event_to_json(event, event_format));
            }

            result.insert(room_id.clone(), values);
        }

        Ok(result)
    }

    pub(crate) async fn get_presence_events(
        &self,
        user_id: &str,
        _since: &Option<SyncToken>,
    ) -> ApiResult<Vec<serde_json::Value>> {
        let presence = self
            .presence_storage
            .get_presence_with_meta(user_id)
            .await
            .map_err(map_internal!("Failed to get presence for sync"))?;

        let Some((presence, status_msg, last_active_ts)) = presence else {
            return Ok(Vec::new());
        };

        let now = chrono::Utc::now().timestamp_millis();
        let last_active_ago = if presence == "offline" { None } else { last_active_ts.map(|ts| (now - ts).max(0)) };
        let currently_active = if presence == "online" {
            Some(last_active_ts.is_some_and(|ts| (now - ts) <= 5 * 60 * 1000))
        } else if presence == "offline" {
            None
        } else {
            Some(false)
        };

        Ok(vec![json!({
            "content": {
                "avatar_url": null,
                "displayname": null,
                "last_active_ago": last_active_ago,
                "presence": presence,
                "status_msg": status_msg,
                "currently_active": currently_active
            },
            "sender": user_id,
            "type": "m.presence"
        })])
    }

    pub(crate) async fn get_account_data_events(&self, user_id: &str) -> ApiResult<Vec<serde_json::Value>> {
        let rows = AccountDataStorage::new(&self.event_storage.pool)
            .list_account_data(user_id)
            .await
            .map_err(map_internal!("Failed to get account data"))?;

        let mut events: Vec<serde_json::Value> = rows
            .iter()
            .map(|row| {
                json!({
                    "type": row.data_type,
                    "content": row.content
                })
            })
            .collect();

        let joined_room_ids: HashSet<String> = self
            .member_storage
            .get_joined_rooms(user_id)
            .await
            .map_err(map_internal!("Failed to load joined rooms"))?
            .into_iter()
            .collect();
        if let Some(direct) = events.iter_mut().find(|e| e["type"] == "m.direct") {
            if let Some(map) = direct.get_mut("content").and_then(|c| c.as_object_mut()) {
                map.retain(|_, value| {
                    if let Some(rooms) = value.as_array_mut() {
                        rooms.retain(|room| {
                            room.as_str()
                                .is_some_and(|id| !id.is_empty() && id.starts_with('!') && joined_room_ids.contains(id))
                        });
                        !rooms.is_empty()
                    } else {
                        false
                    }
                });
            }
        }

        let username = user_id.trim_start_matches('@').split(':').next().unwrap_or("");
        if let Some(existing) = events.iter_mut().find(|e| e["type"] == "m.push_rules") {
            if let Some(content) = existing.get_mut("content") {
                crate::sync_service::push_rules::merge_default_push_rules(content, user_id, username);
            }
        } else {
            events.push(json!({
                "type": "m.push_rules",
                "content": crate::sync_service::push_rules::default_push_rules_for_user(
                    user_id, username,
                ),
            }));
        }

        Ok(events)
    }

    pub(crate) async fn get_to_device_events(
        &self,
        user_id: &str,
        device_id: Option<&str>,
        since: &Option<SyncToken>,
    ) -> ApiResult<(Vec<serde_json::Value>, i64)> {
        let Some(device_id) = device_id else {
            return Ok((Vec::new(), 0));
        };
        let since_stream_id = Self::to_device_since_stream_id(since);
        self.to_device_storage
            .get_messages_since(user_id, device_id, since_stream_id, self.sync_to_device_limit())
            .await
            .map_err(map_internal!("Failed to get to-device events"))
    }

    pub(crate) async fn get_device_lists(
        &self,
        user_id: &str,
        since: &Option<SyncToken>,
    ) -> ApiResult<(serde_json::Value, i64)> {
        let since_stream_id = Self::device_list_since_stream_id(since);
        let (changed, _) = self
            .device_storage
            .get_device_lists_since_with_shared_rooms(since_stream_id, user_id)
            .await
            .map_err(map_internal!("Failed to get device lists"))?;
        let left = self.get_device_list_left_users_for_sync(user_id, since).await?;
        let max_stream_id = self
            .device_storage
            .get_max_device_list_stream_id()
            .await
            .map_err(map_internal!("Failed to get device list stream position"))?;

        Ok((
            json!({
                "changed": changed,
                "left": left
            }),
            max_stream_id,
        ))
    }

    async fn get_device_list_left_users_for_sync(
        &self,
        user_id: &str,
        since: &Option<SyncToken>,
    ) -> ApiResult<Vec<String>> {
        let Some(since_token) = since.as_ref() else {
            return Ok(Vec::new());
        };
        if since_token.stream_id <= 0 {
            return Ok(Vec::new());
        }

        let room_memberships = self
            .member_storage
            .get_sync_rooms(user_id, true)
            .await
            .map_err(map_internal!("Failed to get sync rooms for device list left users"))?;
        let room_ids: Vec<String> = room_memberships.into_iter().map(|membership| membership.room_id).collect();
        if room_ids.is_empty() {
            return Ok(Vec::new());
        }

        let filter =
            synapse_storage::EventQueryFilter { types: Some(vec!["m.room.member".to_string()]), ..Default::default() };
        let membership_events_by_room = self
            .event_storage
            .get_room_events_since_stream_batch_filtered(&room_ids, since_token.stream_id, 1000, &filter)
            .await
            .map_err(map_internal!("Failed to get membership delta for device list left users"))?;

        let current_shared_users: HashSet<String> = self
            .member_storage
            .get_shared_room_users(user_id)
            .await
            .map_err(map_internal!("Failed to get current shared room users"))?
            .into_iter()
            .collect();

        let mut left_candidates: HashSet<String> = HashSet::new();

        for (room_id, mut events) in membership_events_by_room {
            events.sort_by_key(|event| (event.stream_ordering.unwrap_or_default(), event.origin_server_ts));

            let mut users_with_join_in_delta: HashSet<String> = HashSet::new();
            let mut requester_left_room = false;
            let mut latest_membership_by_user: HashMap<String, String> = HashMap::new();

            for event in &events {
                if event.event_type != "m.room.member" {
                    continue;
                }
                let Some(state_key) = event.state_key.as_deref() else {
                    continue;
                };
                let Some(membership) = event.content.get("membership").and_then(|value| value.as_str()) else {
                    continue;
                };

                if membership == "join" {
                    users_with_join_in_delta.insert(state_key.to_string());
                }
                latest_membership_by_user.insert(state_key.to_string(), membership.to_string());
            }

            for (state_key, membership) in latest_membership_by_user {
                if state_key == user_id {
                    if membership != "join" && membership != "invite" {
                        requester_left_room = true;
                    }
                    continue;
                }

                if membership == "join" || membership == "invite" {
                    continue;
                }

                let should_report_left = if users_with_join_in_delta.contains(&state_key) {
                    membership == "leave" || membership == "ban"
                } else {
                    let current_member = self
                        .member_storage
                        .get_room_member(&room_id, &state_key)
                        .await
                        .map_err(map_internal!("Failed to load room member for device list left users"))?;

                    current_member.is_some_and(|member| {
                        let was_joined = member.joined_ts.is_some();
                        match membership.as_str() {
                            // A ban can directly terminate sharing even if the storage row has not
                            // been stamped with `left_ts`.
                            "ban" => was_joined,
                            // A real leave/kick path stamps `left_ts`; unban transitions back to
                            // `leave` without creating a new sharing loss and should not re-emit.
                            "leave" => was_joined && member.left_ts.is_some(),
                            // Forget only hides an already-left room and must not create a fresh
                            // device_lists.left entry.
                            "forget" => false,
                            _ => false,
                        }
                    })
                };

                if should_report_left {
                    left_candidates.insert(state_key);
                }
            }

            if requester_left_room {
                let joined_members = self
                    .member_storage
                    .get_room_members(&room_id, "join")
                    .await
                    .map_err(map_internal!("Failed to load joined members for requester left room"))?;
                for member in joined_members {
                    if member.user_id != user_id {
                        left_candidates.insert(member.user_id);
                    }
                }
            }
        }

        let mut left: Vec<String> =
            left_candidates.into_iter().filter(|candidate| !current_shared_users.contains(candidate)).collect();
        left.sort();
        left.dedup();
        Ok(left)
    }

    pub(crate) fn to_device_since_stream_id(since: &Option<SyncToken>) -> i64 {
        since.as_ref().and_then(|token| token.to_device_stream_id).unwrap_or(0)
    }

    pub(crate) fn device_list_since_stream_id(since: &Option<SyncToken>) -> i64 {
        since.as_ref().and_then(|token| token.device_list_stream_id).unwrap_or(0)
    }

    pub(crate) async fn get_room_ephemeral_events(
        &self,
        room_id: &str,
        _user_id: &str,
    ) -> ApiResult<Vec<serde_json::Value>> {
        let now = chrono::Utc::now().timestamp_millis();
        let limit = self.sync_ephemeral_limit();
        let rows = self
            .event_storage
            .get_ephemeral_events(room_id, now, limit)
            .await
            .map_err(map_internal!("Failed to get ephemeral events"))?;

        let events: Vec<serde_json::Value> = rows
            .iter()
            .map(|row| {
                json!({
                    "type": row.event_type,
                    "content": row.content
                })
            })
            .collect();

        let events = Self::aggregate_ephemeral_events(events);

        Ok(events)
    }

    pub(crate) async fn get_room_ephemeral_events_batch(
        &self,
        room_ids: &[String],
    ) -> ApiResult<HashMap<String, Vec<serde_json::Value>>> {
        let limit = self.sync_ephemeral_limit();
        let mut result: HashMap<String, Vec<serde_json::Value>> =
            room_ids.iter().cloned().map(|room_id| (room_id, Vec::new())).collect();
        if room_ids.is_empty() {
            return Ok(result);
        }

        let now = chrono::Utc::now().timestamp_millis();
        let rows = self
            .event_storage
            .get_ephemeral_events_batch(room_ids, now, limit)
            .await
            .map_err(map_internal!("Failed to get room ephemeral events"))?;

        for (room_id, room_events) in rows {
            if let Some(events) = result.get_mut(&room_id) {
                for row in room_events {
                    events.push(json!({
                        "type": row.event_type,
                        "content": row.content
                    }));
                }
            }
        }

        for (_room_id, events) in result.iter_mut() {
            *events = Self::aggregate_ephemeral_events(std::mem::take(events));
        }

        Ok(result)
    }

    pub(crate) async fn get_room_account_data_events(
        &self,
        room_id: &str,
        user_id: &str,
    ) -> ApiResult<Vec<serde_json::Value>> {
        let rows = RoomAccountDataStorage::list_room_account_data(&self.event_storage.pool, user_id, room_id)
            .await
            .map_err(map_internal!("Failed to get room account data"))?;

        Ok(rows
            .iter()
            .map(|row| {
                json!({
                    "type": row.data_type,
                    "content": row.content
                })
            })
            .collect())
    }

    pub(crate) async fn get_room_account_data_events_batch(
        &self,
        user_id: &str,
        room_ids: &[String],
    ) -> ApiResult<HashMap<String, Vec<serde_json::Value>>> {
        let mut result: HashMap<String, Vec<serde_json::Value>> =
            room_ids.iter().cloned().map(|room_id| (room_id, Vec::new())).collect();
        if room_ids.is_empty() {
            return Ok(result);
        }

        let rows = RoomAccountDataStorage::list_room_account_data_batch(&self.event_storage.pool, user_id, room_ids)
            .await
            .map_err(map_internal!("Failed to get room account data"))?;

        for row in rows {
            if let Some(events) = result.get_mut(&row.room_id) {
                events.push(json!({
                    "type": row.data_type,
                    "content": row.content
                }));
            }
        }

        Ok(result)
    }

    pub(crate) async fn get_unread_counts(&self, room_id: &str, user_id: &str) -> ApiResult<(i64, i64)> {
        let counts = self
            .room_storage
            .get_unread_counts(room_id, user_id)
            .await
            .map_err(map_internal!("Failed to get unread counts"))?;
        Ok((counts.highlight_count, counts.notification_count))
    }

    pub(crate) async fn get_unread_counts_batch(
        &self,
        room_ids: &[String],
        user_id: &str,
    ) -> ApiResult<HashMap<String, (i64, i64)>> {
        let mut result: HashMap<String, (i64, i64)> =
            room_ids.iter().cloned().map(|room_id| (room_id, (0, 0))).collect();
        if room_ids.is_empty() {
            return Ok(result);
        }

        let rows = self
            .room_storage
            .get_unread_counts_batch(room_ids, user_id)
            .await
            .map_err(map_internal!("Failed to get unread counts"))?;

        for row in rows {
            result.insert(row.room_id, (row.highlight_count, row.notification_count));
        }

        Ok(result)
    }
}
