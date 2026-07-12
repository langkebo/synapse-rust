use serde_json::{json, Value};
use std::collections::HashMap;
use synapse_storage::sliding_sync::{
    SlidingSyncFilters, SlidingSyncListData, SlidingSyncListQuery, SlidingSyncRequest, SlidingSyncRoom,
};

use super::{RoomSubscriptionConfig, SlidingListRangeSnapshot, SlidingListWindowSnapshot, SlidingSyncService};

impl SlidingSyncService {
    pub(super) async fn build_lists_response(
        &self,
        user_id: &str,
        device_id: &str,
        conn_id: Option<&str>,
        lists: &std::collections::HashMap<String, SlidingSyncListData>,
        since_pos: Option<&str>,
    ) -> Result<serde_json::Value, sqlx::Error> {
        let mut lists_json = serde_json::Map::new();

        for (list_key, list_data) in lists {
            let mut range_snapshots = Vec::with_capacity(lists.len());

            for range in &list_data.ranges {
                if range.len() >= 2 {
                    let start = range[0];
                    let end = range[1];
                    let range_rooms = self
                        .storage
                        .get_rooms_for_list(SlidingSyncListQuery {
                            user_id,
                            device_id,
                            conn_id,
                            list_key,
                            start,
                            end,
                            filters: list_data.filters.as_ref(),
                        })
                        .await?;
                    let room_ids = range_rooms.into_iter().map(|room| room.room_id).collect();
                    range_snapshots.push(SlidingListRangeSnapshot { start, end, room_ids });
                }
            }

            let count =
                self.count_rooms_for_list(user_id, device_id, conn_id, list_key, list_data.filters.as_ref()).await?;

            lists_json.insert(
                list_key.clone(),
                json!({
                    "ops": self
                        .build_ops(
                            user_id,
                            device_id,
                            conn_id,
                            list_key,
                            &range_snapshots,
                            since_pos,
                        )
                        .await,
                    "count": count,
                }),
            );
        }

        Ok(serde_json::Value::Object(lists_json))
    }

    async fn count_rooms_for_list(
        &self,
        user_id: &str,
        device_id: &str,
        conn_id: Option<&str>,
        list_key: &str,
        filters: Option<&SlidingSyncFilters>,
    ) -> Result<u32, sqlx::Error> {
        let count = self.storage.count_rooms_for_list(user_id, device_id, conn_id, list_key, filters).await?;
        Ok(count as u32)
    }

    async fn build_ops(
        &self,
        user_id: &str,
        device_id: &str,
        conn_id: Option<&str>,
        list_key: &str,
        current_ranges: &[SlidingListRangeSnapshot],
        since_pos: Option<&str>,
    ) -> Vec<serde_json::Value> {
        let cache_key = Self::list_snapshot_cache_key(user_id, device_id, conn_id, list_key);
        let previous_snapshot =
            self.cache.get_raw(&cache_key).and_then(|raw| serde_json::from_str::<SlidingListWindowSnapshot>(&raw).ok());

        let ops = if let (Some(_), Some(previous)) = (since_pos, previous_snapshot.as_ref()) {
            Self::build_incremental_ops(previous, current_ranges)
                .unwrap_or_else(|| Self::build_sync_ops(current_ranges))
        } else {
            Self::build_sync_ops(current_ranges)
        };

        let snapshot = SlidingListWindowSnapshot { ranges: current_ranges.to_vec() };
        if let Ok(raw) = serde_json::to_string(&snapshot) {
            self.cache.set_raw(&cache_key, &raw, 3600).await;
        }

        ops
    }

    pub(super) async fn build_rooms_response(
        &self,
        user_id: &str,
        device_id: &str,
        conn_id: Option<&str>,
        request: &SlidingSyncRequest,
    ) -> Result<serde_json::Value, sqlx::Error> {
        let mut rooms_json = serde_json::Map::new();
        let mut room_configs: HashMap<String, RoomSubscriptionConfig> = HashMap::new();

        if let Some(subscriptions) = &request.room_subscriptions {
            if let Some(subs_obj) = subscriptions.as_object() {
                for (room_id, config_value) in subs_obj {
                    room_configs.insert(room_id.clone(), Self::subscription_config_from_value(Some(config_value)));
                    let room = if let Some(room) = self.storage.get_room(user_id, device_id, room_id, conn_id).await? {
                        Some(room)
                    } else {
                        self.storage.materialize_room_from_activity(user_id, device_id, room_id, conn_id).await?
                    };

                    if let Some(room) = room {
                        let payload = self
                            .build_room_json(
                                &room,
                                room_configs.get(room_id).unwrap_or(&RoomSubscriptionConfig::default()),
                                request.pos.is_none(),
                            )
                            .await?;
                        rooms_json.insert(room_id.clone(), payload);
                    }
                }
            }
        }

        for (list_key, list_data) in &request.lists {
            for range in &list_data.ranges {
                if range.len() >= 2 {
                    let start = range[0];
                    let end = range[1];
                    let rooms = self
                        .storage
                        .get_rooms_for_list(SlidingSyncListQuery {
                            user_id,
                            device_id,
                            conn_id,
                            list_key,
                            start,
                            end,
                            filters: list_data.filters.as_ref(),
                        })
                        .await?;

                    for room in rooms {
                        let room_id = room.room_id.clone();
                        room_configs
                            .entry(room_id.clone())
                            .or_insert_with(|| Self::subscription_config_from_list(list_data));

                        if !rooms_json.contains_key(&room_id) {
                            let payload = self
                                .build_room_json(
                                    &room,
                                    room_configs.get(&room_id).unwrap_or(&RoomSubscriptionConfig::default()),
                                    request.pos.is_none(),
                                )
                                .await?;
                            rooms_json.insert(room_id, payload);
                        }
                    }
                }
            }
        }

        Ok(serde_json::Value::Object(rooms_json))
    }

    async fn build_room_json(
        &self,
        room: &SlidingSyncRoom,
        config: &RoomSubscriptionConfig,
        initial: bool,
    ) -> Result<serde_json::Value, sqlx::Error> {
        let mut room_json = Self::room_to_json(room);
        let required_state_events =
            self.build_required_state_events(&room.room_id, config.required_state.as_ref()).await?;
        let (timeline, limited, prev_batch) = self.build_timeline(&room.room_id, config.timeline_limit).await?;

        let state_value = json!(required_state_events);
        room_json["required_state"] = state_value.clone();
        room_json["state"] = state_value;
        room_json["timeline"] = json!(timeline);
        room_json["initial"] = json!(initial);
        room_json["limited"] = json!(limited);
        room_json["prev_batch"] = json!(prev_batch);
        room_json["num_live"] = json!(config.timeline_limit.filter(|limit| *limit > 0).map_or(0, |_| timeline.len()));
        room_json["bump_stamp"] = json!(room.bump_stamp.unwrap_or(0));

        Ok(room_json)
    }

    pub(super) fn room_to_json(room: &SlidingSyncRoom) -> Value {
        json!({
            "room_id": room.room_id,
            "name": room.name,
            "avatar": room.avatar,
            "is_dm": room.is_dm,
            "is_encrypted": room.is_encrypted,
            "is_tombstoned": room.is_tombstoned,
            "invited": room.is_invited,
            "highlight_count": room.highlight_count,
            "notification_count": room.notification_count,
            "timestamp": room.timestamp.unwrap_or(0),
        })
    }

    pub(crate) fn subscription_config_from_list(list_data: &SlidingSyncListData) -> RoomSubscriptionConfig {
        RoomSubscriptionConfig {
            timeline_limit: list_data.timeline_limit,
            required_state: list_data.required_state.clone(),
        }
    }

    pub(crate) fn subscription_config_from_value(value: Option<&serde_json::Value>) -> RoomSubscriptionConfig {
        let Some(value) = value else {
            return RoomSubscriptionConfig::default();
        };

        let timeline_limit = value
            .get("timeline_limit")
            .or_else(|| value.get("timelineLimit"))
            .and_then(|v| v.as_u64())
            .and_then(|v| u32::try_from(v).ok());
        let required_state =
            value.get("required_state").and_then(|v| serde_json::from_value::<Vec<Vec<String>>>(v.clone()).ok());

        RoomSubscriptionConfig { timeline_limit, required_state }
    }

    pub(super) fn build_sync_ops(ranges: &[SlidingListRangeSnapshot]) -> Vec<Value> {
        ranges
            .iter()
            .filter(|range| !range.room_ids.is_empty())
            .map(|range| {
                json!({
                    "op": "SYNC",
                    "range": [range.start, range.start + range.room_ids.len().saturating_sub(1) as u32],
                    "room_ids": range.room_ids,
                })
            })
            .collect()
    }

    pub(super) fn build_incremental_ops(
        previous: &SlidingListWindowSnapshot,
        current: &[SlidingListRangeSnapshot],
    ) -> Option<Vec<Value>> {
        if previous.ranges.len() != 1 || current.len() != 1 {
            return None;
        }

        let previous = previous.ranges.first()?;
        let current = current.first()?;
        if previous.start != current.start || previous.end != current.end {
            return None;
        }

        let mut working: Vec<Option<String>> = previous.room_ids.iter().map(|s| Some(s.clone())).collect();
        let mut ops = Vec::with_capacity(current.room_ids.len());
        let mut index = 0usize;

        while index < current.room_ids.len() || index < working.len() {
            if index >= current.room_ids.len() {
                if working[index].is_some() {
                    ops.push(json!({
                        "op": "DELETE",
                        "index": previous.start + index as u32,
                    }));
                }
                working[index] = None;
                let next_some = working[index + 1..].iter().position(|s| s.is_some());
                match next_some {
                    Some(offset) => index += offset + 1,
                    None => break,
                }
                continue;
            }

            if index >= working.len() {
                let room_id = current.room_ids[index].clone();
                ops.push(json!({
                    "op": "INSERT",
                    "index": previous.start + index as u32,
                    "room_id": room_id,
                }));
                working.push(Some(room_id));
                index += 1;
                continue;
            }

            if working[index].as_deref() == Some(&current.room_ids[index]) {
                index += 1;
                continue;
            }

            let find_offset = working[index + 1..].iter().position(|s| s.as_deref() == Some(&current.room_ids[index]));
            if let Some(offset) = find_offset {
                // Delete entries from index to index+offset (they shift left)
                for (del_idx, item) in working.iter_mut().enumerate().take(index + offset + 1).skip(index) {
                    if item.is_some() {
                        ops.push(json!({
                            "op": "DELETE",
                            "index": previous.start + del_idx as u32,
                        }));
                        *item = None;
                    }
                }
                // After deletions, the target room is now at `index` — insert current room here
                let room_id = current.room_ids[index].clone();
                ops.push(json!({
                    "op": "INSERT",
                    "index": previous.start + index as u32,
                    "room_id": room_id,
                }));
                working[index] = Some(room_id);
                index += 1;
                continue;
            }

            let room_id = current.room_ids[index].clone();
            // If the slot is occupied, delete first before inserting
            if working[index].is_some() {
                ops.push(json!({
                    "op": "DELETE",
                    "index": previous.start + index as u32,
                }));
            }
            ops.push(json!({
                "op": "INSERT",
                "index": previous.start + index as u32,
                "room_id": room_id,
            }));
            working[index] = Some(room_id);
            index += 1;
        }

        let working_ids: Vec<String> = working.iter().filter_map(|s| s.clone()).collect();
        if working_ids == current.room_ids {
            Some(ops)
        } else {
            None
        }
    }

    pub(crate) fn list_snapshot_cache_key(
        user_id: &str,
        device_id: &str,
        conn_id: Option<&str>,
        list_key: &str,
    ) -> String {
        match conn_id {
            Some(conn_id) => {
                format!("sliding_sync:list:{user_id}:{device_id}:{conn_id}:{list_key}")
            }
            None => format!("sliding_sync:list:{user_id}:{device_id}::{list_key}"),
        }
    }
}
