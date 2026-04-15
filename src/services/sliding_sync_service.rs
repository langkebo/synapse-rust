use crate::cache::CacheManager;
use crate::common::error::ApiError;
use crate::e2ee::device_keys::DeviceKeyStorage;
use crate::services::TypingService;
use crate::storage::sliding_sync::{
    AdminRoomTokenSyncEntry, SlidingSyncListData, SlidingSyncRequest, SlidingSyncResponse,
    SlidingSyncRoom, SlidingSyncStorage,
};
use crate::storage::{EventStorage, RoomEvent, StateEvent};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::collections::HashMap;
use std::sync::Arc;

#[derive(Clone)]
pub struct SlidingSyncService {
    storage: SlidingSyncStorage,
    cache: Arc<CacheManager>,
    event_storage: EventStorage,
    typing_service: Arc<crate::services::typing_service::TypingServiceImpl>,
}

#[derive(Debug, Clone, Default)]
struct RoomSubscriptionConfig {
    timeline_limit: Option<u32>,
    required_state: Option<Vec<Vec<String>>>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
struct SlidingListWindowSnapshot {
    ranges: Vec<SlidingListRangeSnapshot>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct SlidingListRangeSnapshot {
    start: u32,
    end: u32,
    room_ids: Vec<String>,
}

impl SlidingSyncService {
    pub fn new(
        storage: SlidingSyncStorage,
        cache: Arc<CacheManager>,
        event_storage: EventStorage,
        typing_service: Arc<crate::services::typing_service::TypingServiceImpl>,
    ) -> Self {
        Self {
            storage,
            cache,
            event_storage,
            typing_service,
        }
    }

    pub async fn sync(
        &self,
        user_id: &str,
        device_id: &str,
        request: SlidingSyncRequest,
    ) -> Result<SlidingSyncResponse, ApiError> {
        let conn_id = request.conn_id.as_deref();

        if let Some(pos_str) = &request.pos {
            if !self
                .storage
                .validate_pos(user_id, device_id, conn_id, pos_str)
                .await
                .map_err(|e| ApiError::internal(format!("Failed to validate pos: {}", e)))?
            {
                return Err(ApiError::bad_request("Invalid position token"));
            }
        }

        for (list_key, list_data) in &request.lists {
            let ranges: Vec<(u32, u32)> = list_data
                .ranges
                .iter()
                .filter_map(|r| {
                    if r.len() >= 2 {
                        Some((r[0], r[1]))
                    } else {
                        None
                    }
                })
                .collect();

            self.storage
                .save_list(
                    user_id,
                    device_id,
                    conn_id,
                    list_key,
                    &list_data.sort,
                    list_data.filters.as_ref(),
                    None,
                    &ranges,
                )
                .await
                .map_err(|e| ApiError::internal(format!("Failed to save list: {}", e)))?;
        }

        if let Some(unsubs) = &request.unsubscribe_rooms {
            for room_id in unsubs {
                self.storage
                    .delete_room(user_id, device_id, room_id, conn_id)
                    .await
                    .map_err(|e| {
                        ApiError::internal(format!("Failed to unsubscribe room: {}", e))
                    })?;
            }
        }

        let lists_response = self
            .build_lists_response(
                user_id,
                device_id,
                conn_id,
                &request.lists,
                request.pos.as_deref(),
            )
            .await
            .map_err(|e| ApiError::internal(format!("Failed to build lists response: {}", e)))?;

        let rooms_response = self
            .build_rooms_response(user_id, device_id, conn_id, &request)
            .await
            .map_err(|e| ApiError::internal(format!("Failed to build rooms response: {}", e)))?;

        let extensions_response = self
            .build_extensions_response(
                user_id,
                device_id,
                conn_id,
                request.pos.as_deref(),
                &rooms_response,
                request.extensions.as_ref(),
            )
            .await
            .map_err(|e| {
                ApiError::internal(format!("Failed to build extensions response: {}", e))
            })?;

        let new_token = self
            .storage
            .create_or_update_token(user_id, device_id, conn_id)
            .await
            .map_err(|e| ApiError::internal(format!("Failed to update token: {}", e)))?;

        Ok(SlidingSyncResponse {
            pos: new_token.pos.to_string(),
            conn_id: request.conn_id,
            lists: lists_response,
            rooms: rooms_response,
            extensions: extensions_response,
        })
    }

    async fn build_lists_response(
        &self,
        user_id: &str,
        device_id: &str,
        conn_id: Option<&str>,
        lists: &std::collections::HashMap<String, SlidingSyncListData>,
        since_pos: Option<&str>,
    ) -> Result<serde_json::Value, sqlx::Error> {
        let mut lists_json = serde_json::Map::new();

        for (list_key, list_data) in lists {
            let mut range_snapshots = Vec::new();

            for range in &list_data.ranges {
                if range.len() >= 2 {
                    let start = range[0];
                    let end = range[1];
                    let range_rooms = self
                        .storage
                        .get_rooms_for_list(crate::storage::sliding_sync::SlidingSyncListQuery {
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
                    range_snapshots.push(SlidingListRangeSnapshot {
                        start,
                        end,
                        room_ids,
                    });
                }
            }

            let count = self
                .count_rooms_for_list(
                    user_id,
                    device_id,
                    conn_id,
                    list_key,
                    list_data.filters.as_ref(),
                )
                .await?;

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
        filters: Option<&crate::storage::sliding_sync::SlidingSyncFilters>,
    ) -> Result<u32, sqlx::Error> {
        let count = self
            .storage
            .count_rooms_for_list(user_id, device_id, conn_id, list_key, filters)
            .await?;
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
        let previous_snapshot = self
            .cache
            .get_raw(&cache_key)
            .and_then(|raw| serde_json::from_str::<SlidingListWindowSnapshot>(&raw).ok());

        let ops = if let (Some(_), Some(previous)) = (since_pos, previous_snapshot.as_ref()) {
            self.build_incremental_ops(previous, current_ranges)
                .unwrap_or_else(|| Self::build_sync_ops(current_ranges))
        } else {
            Self::build_sync_ops(current_ranges)
        };

        let snapshot = SlidingListWindowSnapshot {
            ranges: current_ranges.to_vec(),
        };
        if let Ok(raw) = serde_json::to_string(&snapshot) {
            self.cache.set_raw(&cache_key, &raw, 3600).await;
        }

        ops
    }

    async fn build_rooms_response(
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
                    room_configs.insert(
                        room_id.clone(),
                        Self::subscription_config_from_value(Some(config_value)),
                    );
                    let room = if let Some(room) = self
                        .storage
                        .get_room(user_id, device_id, room_id, conn_id)
                        .await?
                    {
                        Some(room)
                    } else {
                        self.storage
                            .materialize_room_from_activity(user_id, device_id, room_id, conn_id)
                            .await?
                    };

                    if let Some(room) = room {
                        let payload = self
                            .build_room_json(
                                &room,
                                room_configs
                                    .get(room_id)
                                    .unwrap_or(&RoomSubscriptionConfig::default()),
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
                        .get_rooms_for_list(crate::storage::sliding_sync::SlidingSyncListQuery {
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
                                    room_configs
                                        .get(&room_id)
                                        .unwrap_or(&RoomSubscriptionConfig::default()),
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
        let mut room_json = self.room_to_json(room);
        let required_state_events = self
            .build_required_state_events(&room.room_id, config.required_state.as_ref())
            .await?;
        let (timeline, limited, prev_batch) = self
            .build_timeline(&room.room_id, config.timeline_limit)
            .await?;

        room_json["required_state"] = json!(required_state_events);
        room_json["state"] = json!(required_state_events);
        room_json["timeline"] = json!(timeline);
        room_json["initial"] = json!(initial);
        room_json["limited"] = json!(limited);
        room_json["prev_batch"] = json!(prev_batch);
        room_json["num_live"] = json!(config
            .timeline_limit
            .filter(|limit| *limit > 0)
            .map(|_| timeline.len())
            .unwrap_or(0));
        room_json["bump_stamp"] = json!(room.bump_stamp);

        Ok(room_json)
    }

    fn room_to_json(&self, room: &SlidingSyncRoom) -> Value {
        json!({
            "room_id": room.room_id,
            "name": room.name,
            "avatar": room.avatar,
            "is_dm": room.is_dm,
            "is_encrypted": room.is_encrypted,
            "is_tombstoned": room.is_tombstoned,
            "invited": room.invited,
            "highlight_count": room.highlight_count,
            "notification_count": room.notification_count,
            "timestamp": room.timestamp,
        })
    }

    async fn build_extensions_response(
        &self,
        user_id: &str,
        device_id: &str,
        conn_id: Option<&str>,
        since_pos: Option<&str>,
        rooms_response: &serde_json::Value,
        request_extensions: Option<&serde_json::Value>,
    ) -> Result<Option<serde_json::Value>, sqlx::Error> {
        let Some(request_extensions) = request_extensions else {
            return Ok(None);
        };

        let mut response_extensions = request_extensions.as_object().cloned().unwrap_or_default();

        let account_data_enabled = request_extensions
            .get("account_data")
            .and_then(|v| {
                if v.as_bool() == Some(true) {
                    Some(true)
                } else {
                    v.as_object()
                        .map(|obj| obj.get("enabled").and_then(|e| e.as_bool()).unwrap_or(true))
                }
            })
            .unwrap_or(false);

        if account_data_enabled {
            let room_ids: Vec<String> = rooms_response
                .as_object()
                .map(|obj| obj.keys().cloned().collect())
                .unwrap_or_default();

            let global = self.storage.get_global_account_data(user_id).await?;
            let rooms = self
                .storage
                .get_room_account_data(user_id, &room_ids)
                .await?;

            response_extensions.insert(
                "account_data".to_string(),
                serde_json::json!({
                    "global": global,
                    "rooms": rooms
                }),
            );
        }

        let receipts_enabled = request_extensions
            .get("receipts")
            .and_then(|v| {
                if v.as_bool() == Some(true) {
                    Some(true)
                } else {
                    v.as_object()
                        .map(|obj| obj.get("enabled").and_then(|e| e.as_bool()).unwrap_or(true))
                }
            })
            .unwrap_or(false);

        if receipts_enabled {
            let room_ids: Vec<String> = rooms_response
                .as_object()
                .map(|obj| obj.keys().cloned().collect())
                .unwrap_or_default();
            let receipts = self.storage.get_receipts_for_rooms(&room_ids).await?;
            response_extensions.insert(
                "receipts".to_string(),
                serde_json::json!({
                    "rooms": receipts
                }),
            );
        }

        let typing_enabled = request_extensions
            .get("typing")
            .and_then(|v| {
                if v.as_bool() == Some(true) {
                    Some(true)
                } else {
                    v.as_object()
                        .map(|obj| obj.get("enabled").and_then(|e| e.as_bool()).unwrap_or(true))
                }
            })
            .unwrap_or(false);

        if typing_enabled {
            let room_ids: Vec<String> = rooms_response
                .as_object()
                .map(|obj| obj.keys().cloned().collect())
                .unwrap_or_default();
            let mut typing_rooms = serde_json::Map::new();
            for room_id in room_ids {
                let typing_users = self
                    .typing_service
                    .get_typing_users(&room_id)
                    .await
                    .map_err(|e| sqlx::Error::Protocol(e.to_string()))?;
                let user_ids: Vec<String> = typing_users.into_keys().collect();
                typing_rooms.insert(room_id, serde_json::json!({ "user_ids": user_ids }));
            }
            response_extensions.insert(
                "typing".to_string(),
                serde_json::json!({
                    "rooms": typing_rooms
                }),
            );
        }

        let to_device_request = request_extensions.get("to_device");
        let to_device_enabled = to_device_request
            .and_then(|v| {
                if v.as_bool() == Some(true) {
                    Some(true)
                } else {
                    v.as_object()
                        .map(|obj| obj.get("enabled").and_then(|e| e.as_bool()).unwrap_or(true))
                }
            })
            .unwrap_or(false);

        if to_device_enabled {
            let to_device = self
                .build_to_device_extension(user_id, device_id, to_device_request)
                .await?;
            response_extensions.insert("to_device".to_string(), to_device);
        }

        let e2ee_enabled = request_extensions
            .get("e2ee")
            .and_then(|v| {
                if v.as_bool() == Some(true) {
                    Some(true)
                } else {
                    v.as_object()
                        .map(|obj| obj.get("enabled").and_then(|e| e.as_bool()).unwrap_or(true))
                }
            })
            .unwrap_or(false);

        if e2ee_enabled {
            let e2ee = self
                .build_e2ee_extension(user_id, device_id, conn_id, since_pos)
                .await?;
            response_extensions.insert("e2ee".to_string(), e2ee);
        }

        if response_extensions.is_empty() {
            Ok(None)
        } else {
            Ok(Some(serde_json::Value::Object(response_extensions)))
        }
    }

    #[allow(clippy::too_many_arguments)]
    pub async fn update_room_state(
        &self,
        user_id: &str,
        device_id: &str,
        room_id: &str,
        conn_id: Option<&str>,
        bump_stamp: i64,
        highlight_count: i32,
        notification_count: i32,
        is_dm: bool,
        is_encrypted: bool,
        name: Option<&str>,
        avatar: Option<&str>,
    ) -> Result<(), ApiError> {
        self.storage
            .upsert_room(
                user_id,
                device_id,
                room_id,
                conn_id,
                None,
                bump_stamp,
                highlight_count,
                notification_count,
                is_dm,
                is_encrypted,
                false,
                false,
                name,
                avatar,
                bump_stamp,
            )
            .await
            .map_err(|e| ApiError::internal(format!("Failed to update room state: {}", e)))?;

        self.invalidate_room_cache(user_id, device_id, room_id, conn_id)
            .await;

        Ok(())
    }

    pub async fn bump_room(
        &self,
        user_id: &str,
        device_id: &str,
        room_id: &str,
        conn_id: Option<&str>,
        bump_stamp: i64,
    ) -> Result<(), ApiError> {
        self.storage
            .bump_room(user_id, device_id, room_id, conn_id, bump_stamp)
            .await
            .map_err(|e| ApiError::internal(format!("Failed to bump room: {}", e)))?;

        self.invalidate_room_cache(user_id, device_id, room_id, conn_id)
            .await;

        Ok(())
    }

    pub async fn update_notification_counts(
        &self,
        user_id: &str,
        device_id: &str,
        room_id: &str,
        conn_id: Option<&str>,
        highlight_count: i32,
        notification_count: i32,
    ) -> Result<(), ApiError> {
        self.storage
            .update_notification_counts(
                user_id,
                device_id,
                room_id,
                conn_id,
                highlight_count,
                notification_count,
            )
            .await
            .map_err(|e| ApiError::internal(format!("Failed to update notifications: {}", e)))?;

        self.invalidate_room_cache(user_id, device_id, room_id, conn_id)
            .await;

        Ok(())
    }

    pub async fn remove_room(
        &self,
        user_id: &str,
        device_id: &str,
        room_id: &str,
        conn_id: Option<&str>,
    ) -> Result<(), ApiError> {
        self.storage
            .delete_room(user_id, device_id, room_id, conn_id)
            .await
            .map_err(|e| ApiError::internal(format!("Failed to remove room: {}", e)))?;

        self.invalidate_room_cache(user_id, device_id, room_id, conn_id)
            .await;

        Ok(())
    }

    pub async fn cleanup_expired_tokens(&self) -> Result<u64, ApiError> {
        let count = self
            .storage
            .cleanup_expired_tokens()
            .await
            .map_err(|e| ApiError::internal(format!("Failed to cleanup tokens: {}", e)))?;

        Ok(count)
    }

    pub async fn get_room_token_sync(
        &self,
        room_id: &str,
        limit: i64,
        offset: i64,
    ) -> Result<(Vec<AdminRoomTokenSyncEntry>, i64), ApiError> {
        let entries = self
            .storage
            .list_room_token_sync(room_id, limit, offset)
            .await
            .map_err(|e| ApiError::internal(format!("Failed to list room token sync: {}", e)))?;

        let total = self
            .storage
            .count_room_token_sync(room_id)
            .await
            .map_err(|e| ApiError::internal(format!("Failed to count room token sync: {}", e)))?;

        Ok((entries, total))
    }

    async fn invalidate_room_cache(
        &self,
        user_id: &str,
        device_id: &str,
        room_id: &str,
        conn_id: Option<&str>,
    ) {
        let cache_key = if let Some(cid) = conn_id {
            format!(
                "sliding_sync:room:{}:{}:{}:{}",
                user_id, device_id, cid, room_id
            )
        } else {
            format!("sliding_sync:room:{}:{}::{}", user_id, device_id, room_id)
        };
        let _ = self.cache.delete(&cache_key).await;
    }

    async fn build_e2ee_extension(
        &self,
        user_id: &str,
        device_id: &str,
        conn_id: Option<&str>,
        since_pos: Option<&str>,
    ) -> Result<Value, sqlx::Error> {
        let device_key_storage = DeviceKeyStorage::new(&self.event_storage.pool);
        let key_count = device_key_storage
            .get_one_time_keys_count(user_id, device_id)
            .await
            .map_err(|e| sqlx::Error::Protocol(e.to_string()))?;

        let cache_key = Self::e2ee_device_list_stream_cache_key(user_id, device_id, conn_id);
        let since_stream_id = if since_pos.is_some() {
            self.cache
                .get_raw(&cache_key)
                .and_then(|raw| raw.parse::<i64>().ok())
                .unwrap_or(0)
        } else {
            0
        };
        let current_stream_id = self.get_current_device_list_stream_id().await?;
        let device_lists = self
            .get_device_lists_since(user_id, since_stream_id)
            .await?;

        self.cache
            .set_raw(&cache_key, &current_stream_id.to_string(), 3600)
            .await;

        Ok(json!({
            "device_lists": device_lists,
            "device_one_time_keys_count": {
                "signed_curve25519": key_count
            },
            "device_unused_fallback_key_types": [],
        }))
    }

    async fn build_to_device_extension(
        &self,
        user_id: &str,
        device_id: &str,
        request_to_device: Option<&Value>,
    ) -> Result<Value, sqlx::Error> {
        let since_stream_id = request_to_device
            .and_then(|value| value.as_object())
            .and_then(|obj| obj.get("since"))
            .and_then(|value| value.as_str())
            .and_then(|value| value.parse::<i64>().ok())
            .unwrap_or(0);
        let limit = request_to_device
            .and_then(|value| value.as_object())
            .and_then(|obj| obj.get("limit"))
            .and_then(|value| value.as_i64())
            .filter(|value| *value > 0)
            .unwrap_or(100);

        let (events, next_batch) = self
            .get_to_device_extension_payload(user_id, device_id, since_stream_id, limit)
            .await?;

        Ok(json!({
            "events": events,
            "next_batch": next_batch,
        }))
    }

    fn subscription_config_from_list(list_data: &SlidingSyncListData) -> RoomSubscriptionConfig {
        RoomSubscriptionConfig {
            timeline_limit: list_data.timeline_limit,
            required_state: list_data.required_state.clone(),
        }
    }

    fn subscription_config_from_value(value: Option<&serde_json::Value>) -> RoomSubscriptionConfig {
        let Some(value) = value else {
            return RoomSubscriptionConfig::default();
        };

        let timeline_limit = value
            .get("timeline_limit")
            .or_else(|| value.get("timelineLimit"))
            .and_then(|v| v.as_u64())
            .and_then(|v| u32::try_from(v).ok());
        let required_state = value
            .get("required_state")
            .and_then(|v| serde_json::from_value::<Vec<Vec<String>>>(v.clone()).ok());

        RoomSubscriptionConfig {
            timeline_limit,
            required_state,
        }
    }

    async fn build_required_state_events(
        &self,
        room_id: &str,
        required_state: Option<&Vec<Vec<String>>>,
    ) -> Result<Vec<Value>, sqlx::Error> {
        let Some(required_state) = required_state else {
            return Ok(Vec::new());
        };

        let state_events = self.event_storage.get_state_events(room_id).await?;
        Ok(state_events
            .into_iter()
            .filter(|event| Self::required_state_matches(required_state, event))
            .map(|event| Self::state_event_to_json(&event))
            .collect())
    }

    async fn build_timeline(
        &self,
        room_id: &str,
        timeline_limit: Option<u32>,
    ) -> Result<(Vec<Value>, bool, Option<String>), sqlx::Error> {
        let Some(limit) = timeline_limit.filter(|limit| *limit > 0) else {
            return Ok((Vec::new(), false, None));
        };

        let mut events = self
            .event_storage
            .get_room_events_paginated(room_id, None, i64::from(limit) + 1, "b")
            .await?;
        let limited = events.len() > limit as usize;
        if limited {
            events.truncate(limit as usize);
        }
        events.reverse();

        let prev_batch = events
            .first()
            .map(|event| format!("t{}", event.origin_server_ts));
        let timeline = events.iter().map(Self::room_event_to_json).collect();
        Ok((timeline, limited, prev_batch))
    }

    fn required_state_matches(required_state: &[Vec<String>], event: &StateEvent) -> bool {
        let event_type = event.event_type.as_deref().unwrap_or_default();
        let state_key = event.state_key.as_deref().unwrap_or_default();
        required_state.iter().any(|entry| {
            let event_type_match = entry
                .first()
                .map(|value| value == "*" || value == event_type)
                .unwrap_or(false);
            let state_key_match = entry
                .get(1)
                .map(|value| value == "*" || value == state_key)
                .unwrap_or(false);
            event_type_match && state_key_match
        })
    }

    fn room_event_to_json(event: &RoomEvent) -> Value {
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
        if let Some(state_key) = &event.state_key {
            obj["state_key"] = json!(state_key);
        }
        obj
    }

    fn state_event_to_json(event: &StateEvent) -> Value {
        let now = chrono::Utc::now().timestamp_millis();
        let age = now.saturating_sub(event.origin_server_ts);
        let sender = event.user_id.as_deref().unwrap_or(&event.sender);
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
        if let Some(state_key) = &event.state_key {
            obj["state_key"] = json!(state_key);
        }
        obj
    }

    fn build_sync_ops(ranges: &[SlidingListRangeSnapshot]) -> Vec<Value> {
        ranges
            .iter()
            .filter(|range| !range.room_ids.is_empty())
            .map(|range| {
                json!({
                    "op": "SYNC",
                    "range": [range.start, range.start + range.room_ids.len() as u32 - 1],
                    "room_ids": range.room_ids,
                })
            })
            .collect()
    }

    fn build_incremental_ops(
        &self,
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

        let mut working = previous.room_ids.clone();
        let mut ops = Vec::new();
        let mut index = 0usize;

        while index < current.room_ids.len() || index < working.len() {
            if index >= current.room_ids.len() {
                ops.push(json!({
                    "op": "DELETE",
                    "index": previous.start + index as u32,
                }));
                working.remove(index);
                continue;
            }

            if index >= working.len() {
                let room_id = current.room_ids[index].clone();
                ops.push(json!({
                    "op": "INSERT",
                    "index": previous.start + index as u32,
                    "room_id": room_id,
                }));
                working.insert(index, room_id);
                index += 1;
                continue;
            }

            if working[index] == current.room_ids[index] {
                index += 1;
                continue;
            }

            if let Some(offset) = working[index + 1..]
                .iter()
                .position(|room_id| room_id == &current.room_ids[index])
            {
                for _ in 0..=offset {
                    ops.push(json!({
                        "op": "DELETE",
                        "index": previous.start + index as u32,
                    }));
                    working.remove(index);
                }
                continue;
            }

            let room_id = current.room_ids[index].clone();
            ops.push(json!({
                "op": "INSERT",
                "index": previous.start + index as u32,
                "room_id": room_id,
            }));
            working.insert(index, room_id);
            index += 1;
        }

        if working == current.room_ids {
            Some(ops)
        } else {
            None
        }
    }

    fn list_snapshot_cache_key(
        user_id: &str,
        device_id: &str,
        conn_id: Option<&str>,
        list_key: &str,
    ) -> String {
        match conn_id {
            Some(conn_id) => format!(
                "sliding_sync:list:{}:{}:{}:{}",
                user_id, device_id, conn_id, list_key
            ),
            None => format!("sliding_sync:list:{}:{}::{}", user_id, device_id, list_key),
        }
    }

    fn e2ee_device_list_stream_cache_key(
        user_id: &str,
        device_id: &str,
        conn_id: Option<&str>,
    ) -> String {
        match conn_id {
            Some(conn_id) => format!("sliding_sync:e2ee:{}:{}:{}", user_id, device_id, conn_id),
            None => format!("sliding_sync:e2ee:{}:{}:", user_id, device_id),
        }
    }

    async fn get_current_device_list_stream_id(&self) -> Result<i64, sqlx::Error> {
        sqlx::query_scalar::<_, i64>(
            r#"
            SELECT COALESCE(MAX(stream_id), 0)
            FROM device_lists_stream
            "#,
        )
        .fetch_one(&*self.event_storage.pool)
        .await
    }

    async fn get_device_lists_since(
        &self,
        user_id: &str,
        since_stream_id: i64,
    ) -> Result<Value, sqlx::Error> {
        let changed_rows = sqlx::query(
            r#"
            SELECT DISTINCT user_id
            FROM device_lists_stream
            WHERE stream_id > $1
              AND user_id != $2
            ORDER BY user_id
            LIMIT 100
            "#,
        )
        .bind(since_stream_id)
        .bind(user_id)
        .fetch_all(&*self.event_storage.pool)
        .await?;

        let changed: Vec<String> = changed_rows
            .into_iter()
            .map(|row| {
                use sqlx::Row;
                row.get("user_id")
            })
            .collect();

        let left_rows = sqlx::query(
            r#"
            SELECT DISTINCT dl.user_id
            FROM device_lists_stream dl
            LEFT JOIN room_memberships rm ON rm.user_id = dl.user_id
            WHERE dl.stream_id > $1
              AND dl.user_id != $2
              AND rm.user_id IS NULL
            ORDER BY dl.user_id
            LIMIT 100
            "#,
        )
        .bind(since_stream_id)
        .bind(user_id)
        .fetch_all(&*self.event_storage.pool)
        .await?;

        let left: Vec<String> = left_rows
            .into_iter()
            .map(|row| {
                use sqlx::Row;
                row.get("user_id")
            })
            .collect();

        Ok(json!({
            "changed": changed,
            "left": left,
        }))
    }

    async fn get_to_device_extension_payload(
        &self,
        user_id: &str,
        device_id: &str,
        since_stream_id: i64,
        limit: i64,
    ) -> Result<(Vec<Value>, Option<String>), sqlx::Error> {
        let rows = sqlx::query(
            r#"
            SELECT stream_id, sender_user_id, sender_device_id, event_type, content, message_id
            FROM to_device_messages
            WHERE recipient_user_id = $1
              AND recipient_device_id = $2
              AND stream_id > $3
            ORDER BY stream_id ASC
            LIMIT $4
            "#,
        )
        .bind(user_id)
        .bind(device_id)
        .bind(since_stream_id)
        .bind(limit)
        .fetch_all(&*self.event_storage.pool)
        .await?;

        let mut last_stream_id = since_stream_id;
        let events: Vec<Value> = rows
            .into_iter()
            .map(|row| {
                use sqlx::Row;
                let stream_id: i64 = row.get("stream_id");
                let sender: String = row.get("sender_user_id");
                let _sender_device: String = row.get("sender_device_id");
                let event_type: String = row.get("event_type");
                let content: Value = row.get("content");
                let message_id: Option<String> = row.get("message_id");
                last_stream_id = stream_id;

                let mut obj = json!({
                    "type": event_type,
                    "sender": sender,
                    "content": content,
                });

                if let Some(mid) = message_id {
                    obj["message_id"] = json!(mid);
                }

                obj
            })
            .collect();

        let next_batch = if events.is_empty() {
            Some(
                self.get_current_to_device_stream_id(user_id, device_id)
                    .await?
                    .to_string(),
            )
        } else {
            Some(last_stream_id.to_string())
        };

        Ok((events, next_batch))
    }

    async fn get_current_to_device_stream_id(
        &self,
        user_id: &str,
        device_id: &str,
    ) -> Result<i64, sqlx::Error> {
        sqlx::query_scalar::<_, i64>(
            r#"
            SELECT COALESCE(MAX(stream_id), 0)
            FROM to_device_messages
            WHERE recipient_user_id = $1
              AND recipient_device_id = $2
            "#,
        )
        .bind(user_id)
        .bind(device_id)
        .fetch_one(&*self.event_storage.pool)
        .await
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::storage::sliding_sync::SlidingSyncFilters;

    #[tokio::test]
    async fn test_room_to_json() {
        let service = create_test_service();
        let room = SlidingSyncRoom {
            id: 1,
            user_id: "@alice:example.com".to_string(),
            device_id: "DEVICE123".to_string(),
            room_id: "!room:example.com".to_string(),
            conn_id: None,
            list_key: Some("main".to_string()),
            bump_stamp: 1234567890000,
            highlight_count: 5,
            notification_count: 10,
            is_dm: true,
            is_encrypted: true,
            is_tombstoned: false,
            invited: false,
            name: Some("Test Room".to_string()),
            avatar: Some("mxc://example.com/avatar".to_string()),
            timestamp: 1234567890000,
            created_ts: 1234567890000,
            updated_ts: 1234567890000,
        };

        let json = service.room_to_json(&room);

        assert_eq!(json["room_id"], "!room:example.com");
        assert_eq!(json["name"], "Test Room");
        assert_eq!(json["highlight_count"], 5);
        assert!(json["is_dm"].as_bool().unwrap());
    }

    #[tokio::test]
    async fn test_build_ops_empty() {
        let ops = SlidingSyncService::build_sync_ops(&[]);
        assert!(ops.is_empty());
    }

    #[tokio::test]
    async fn test_build_ops_with_rooms() {
        let ops = SlidingSyncService::build_sync_ops(&[SlidingListRangeSnapshot {
            start: 0,
            end: 1,
            room_ids: vec![
                "!room1:example.com".to_string(),
                "!room2:example.com".to_string(),
            ],
        }]);

        assert_eq!(ops.len(), 1);
        assert_eq!(ops[0]["op"], "SYNC");
    }

    #[tokio::test]
    async fn test_build_incremental_ops_uses_insert_and_delete() {
        let service = create_test_service();
        let previous = SlidingListWindowSnapshot {
            ranges: vec![SlidingListRangeSnapshot {
                start: 0,
                end: 1,
                room_ids: vec![
                    "!room1:example.com".to_string(),
                    "!room2:example.com".to_string(),
                ],
            }],
        };
        let current = vec![SlidingListRangeSnapshot {
            start: 0,
            end: 1,
            room_ids: vec![
                "!room0:example.com".to_string(),
                "!room1:example.com".to_string(),
            ],
        }];

        let ops = service.build_incremental_ops(&previous, &current).unwrap();

        assert!(ops.iter().any(|op| op["op"] == "INSERT"));
        assert!(ops.iter().any(|op| op["op"] == "DELETE"));
    }

    fn create_test_service() -> SlidingSyncService {
        SlidingSyncService {
            storage: SlidingSyncStorage::new(std::sync::Arc::new(
                sqlx::postgres::PgPoolOptions::new()
                    .max_connections(1)
                    .connect_lazy("postgres://localhost/test")
                    .unwrap(),
            )),
            cache: Arc::new(CacheManager::new(crate::cache::CacheConfig::default())),
            event_storage: EventStorage::new(
                &std::sync::Arc::new(
                    sqlx::postgres::PgPoolOptions::new()
                        .max_connections(1)
                        .connect_lazy("postgres://localhost/test")
                        .unwrap(),
                ),
                "localhost".to_string(),
            ),
            typing_service: Arc::new(crate::services::typing_service::TypingServiceImpl::new()),
        }
    }

    #[tokio::test]
    async fn test_sliding_sync_filters_serialization() {
        let filters = SlidingSyncFilters {
            is_invite: Some(false),
            is_tombstoned: None,
            room_name_like: Some("test".to_string()),
            ..Default::default()
        };

        let json = serde_json::to_value(&filters).unwrap();

        assert!(json.get("is_invite").is_some());
        assert!(json.get("is_tombstoned").is_none());
        assert_eq!(
            json.get("room_name_like").unwrap().as_str().unwrap(),
            "test"
        );
    }
}
