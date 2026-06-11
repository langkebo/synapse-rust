use crate::cache::CacheManager;
use crate::common::error::ApiError;
use crate::e2ee::device_keys::DeviceKeyStorage;
use crate::storage::membership::RoomMemberStorage;
use crate::storage::presence::PresenceStorage;
pub use crate::storage::sliding_sync::{SlidingSyncRequest, SlidingSyncResponse};
use crate::storage::sliding_sync::{
    AdminRoomTokenSyncEntry, RoomTokenSyncCursor, SlidingSyncListData, SlidingSyncRoom, SlidingSyncStorage,
};
use crate::storage::{EventStorage, RoomEvent, StateEvent};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::collections::HashMap;
use std::sync::Arc;

/// Default TTL for sliding sync connections: 30 minutes in milliseconds.
const CONNECTION_TTL_MS: i64 = 30 * 60 * 1000;

/// Maximum number of tracked connections (LRU capacity cap).
const MAX_TRACKED_CONNECTIONS: u64 = 10_000;

#[derive(Clone)]
pub struct SlidingSyncService {
    storage: SlidingSyncStorage,
    cache: Arc<CacheManager>,
    event_storage: EventStorage,
    typing_service: Arc<crate::services::typing_service::TypingService>,
    presence_storage: PresenceStorage,
    member_storage: RoomMemberStorage,
    /// Tracks last-access timestamp per (user_id, device_id, conn_id) for LRU + TTL GC.
    connection_tracker: Arc<moka::sync::Cache<String, i64>>,
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
        typing_service: Arc<crate::services::typing_service::TypingService>,
        presence_storage: PresenceStorage,
        member_storage: RoomMemberStorage,
    ) -> Self {
        let connection_tracker = moka::sync::Cache::builder()
            .max_capacity(MAX_TRACKED_CONNECTIONS)
            .time_to_idle(std::time::Duration::from_millis(CONNECTION_TTL_MS as u64))
            .build();
        Self {
            storage,
            cache,
            event_storage,
            typing_service,
            presence_storage,
            member_storage,
            connection_tracker: Arc::new(connection_tracker),
        }
    }

    pub async fn sync(
        &self,
        user_id: &str,
        device_id: &str,
        request: SlidingSyncRequest,
    ) -> Result<SlidingSyncResponse, ApiError> {
        // Update user presence to online
        tracing::info!(user_id = %user_id, device_id = %device_id, "Updating presence for user");
        let _ = self.presence_storage.set_presence(user_id, crate::common::PresenceState::Online, None).await;

        let conn_id = request.conn_id.as_deref();

        // Lazy GC: clean up expired connections for this user/device before proceeding.
        self.gc_expired_connections(user_id, device_id).await;

        // Touch the connection in the LRU tracker (records last access time).
        let now = chrono::Utc::now().timestamp_millis();
        let tracker_key = Self::connection_tracker_key(user_id, device_id, conn_id);
        self.connection_tracker.insert(tracker_key, now);

        let is_initial = request.pos.is_none();

        if let Some(pos_str) = &request.pos {
            if !self
                .storage
                .validate_pos(user_id, device_id, conn_id, pos_str)
                .await
                .map_err(|e| ApiError::internal_with_log("Failed to validate pos", &e))?
            {
                return Err(ApiError::bad_request("Invalid position token"));
            }
        }

        for (list_key, list_data) in &request.lists {
            let ranges: Vec<(u32, u32)> =
                list_data.ranges.iter().filter_map(|r| if r.len() >= 2 { Some((r[0], r[1])) } else { None }).collect();

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
                .map_err(|e| ApiError::internal_with_log("Failed to save list", &e))?;
        }

        if let Some(unsubs) = &request.unsubscribe_rooms {
            for room_id in unsubs {
                self.storage
                    .delete_room(user_id, device_id, room_id, conn_id)
                    .await
                    .map_err(|e| ApiError::internal_with_log("Failed to unsubscribe room", &e))?;
            }
        }

        if is_initial {
            if let Ok(joined_rooms) = self.member_storage.get_joined_rooms(user_id).await {
                for room_id in &joined_rooms {
                    let _ = self.storage.materialize_room_from_activity(user_id, device_id, room_id, conn_id).await;
                }
            }
        }

        let lists_response = self
            .build_lists_response(user_id, device_id, conn_id, &request.lists, request.pos.as_deref())
            .await
            .map_err(|e| ApiError::internal_with_log("Failed to build lists response", &e))?;

        let rooms_response = self
            .build_rooms_response(user_id, device_id, conn_id, &request)
            .await
            .map_err(|e| ApiError::internal_with_log("Failed to build rooms response", &e))?;

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
            .map_err(|e| ApiError::internal_with_log("Failed to build extensions response", &e))?;

        let new_token = self
            .storage
            .create_or_update_token(user_id, device_id, conn_id)
            .await
            .map_err(|e| ApiError::internal_with_log("Failed to update token", &e))?;

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
            let mut range_snapshots = Vec::with_capacity(lists.len());

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
        filters: Option<&crate::storage::sliding_sync::SlidingSyncFilters>,
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
        room_json["bump_stamp"] = json!(room.bump_stamp);

        Ok(room_json)
    }

    fn room_to_json(room: &SlidingSyncRoom) -> Value {
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
                    v.as_object().map(|obj| obj.get("enabled").and_then(|e| e.as_bool()).unwrap_or(true))
                }
            })
            .unwrap_or(false);

        if account_data_enabled {
            let room_ids: Vec<String> =
                rooms_response.as_object().map(|obj| obj.keys().cloned().collect()).unwrap_or_default();

            let global = self.storage.get_global_account_data(user_id).await?;
            let rooms = self.storage.get_room_account_data(user_id, &room_ids).await?;

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
                    v.as_object().map(|obj| obj.get("enabled").and_then(|e| e.as_bool()).unwrap_or(true))
                }
            })
            .unwrap_or(false);

        if receipts_enabled {
            let room_ids: Vec<String> =
                rooms_response.as_object().map(|obj| obj.keys().cloned().collect()).unwrap_or_default();
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
                    v.as_object().map(|obj| obj.get("enabled").and_then(|e| e.as_bool()).unwrap_or(true))
                }
            })
            .unwrap_or(false);

        if typing_enabled {
            let room_ids: Vec<String> =
                rooms_response.as_object().map(|obj| obj.keys().cloned().collect()).unwrap_or_default();
            let mut typing_rooms = serde_json::Map::new();
            match self.typing_service.get_typing_users_batch(&room_ids).await {
                Ok(batch) => {
                    for (room_id, user_ids) in batch {
                        typing_rooms.insert(room_id, serde_json::json!({ "user_ids": user_ids }));
                    }
                }
                Err(e) => {
                    tracing::warn!(error = %e, room_count = room_ids.len(), "Failed to get typing users batch");
                }
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
                    v.as_object().map(|obj| obj.get("enabled").and_then(|e| e.as_bool()).unwrap_or(true))
                }
            })
            .unwrap_or(false);

        if to_device_enabled {
            let to_device = self.build_to_device_extension(user_id, device_id, to_device_request).await?;
            response_extensions.insert("to_device".to_string(), to_device);
        }

        let e2ee_enabled = request_extensions
            .get("e2ee")
            .and_then(|v| {
                if v.as_bool() == Some(true) {
                    Some(true)
                } else {
                    v.as_object().map(|obj| obj.get("enabled").and_then(|e| e.as_bool()).unwrap_or(true))
                }
            })
            .unwrap_or(false);

        if e2ee_enabled {
            let e2ee = self.build_e2ee_extension(user_id, device_id, conn_id, since_pos).await?;
            response_extensions.insert("e2ee".to_string(), e2ee);
        }

        let presence_enabled = request_extensions
            .get("presence")
            .and_then(|v| {
                if v.as_bool() == Some(true) {
                    Some(true)
                } else {
                    v.as_object().map(|obj| obj.get("enabled").and_then(|e| e.as_bool()).unwrap_or(true))
                }
            })
            .unwrap_or(false);

        if presence_enabled {
            let room_ids: Vec<String> =
                rooms_response.as_object().map(|obj| obj.keys().cloned().collect()).unwrap_or_default();

            let mut all_members = std::collections::HashSet::new();
            all_members.insert(user_id.to_string());

            if let Ok(batch) = self.member_storage.get_members_batch(&room_ids, "join").await {
                for members in batch.values() {
                    for member in members {
                        all_members.insert(member.user_id.clone());
                    }
                }
            }

            let member_list: Vec<String> = all_members.into_iter().collect();
            let presences = self.presence_storage.get_presences(&member_list).await?;

            let mut presence_events = Vec::with_capacity(presences.len().min(32));
            for (uid, (presence, status_msg)) in presences {
                presence_events.push(serde_json::json!({
                    "sender": uid,
                    "type": "m.presence",
                    "content": {
                        "presence": presence,
                        "status_msg": status_msg,
                        "last_active_ago": 0, // Mocked for now
                    }
                }));
            }

            response_extensions.insert(
                "presence".to_string(),
                serde_json::json!({
                    "events": presence_events
                }),
            );
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
            .map_err(|e| ApiError::internal_with_log("Failed to update room state", &e))?;

        self.invalidate_room_cache(user_id, device_id, room_id, conn_id).await;

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
            .map_err(|e| ApiError::internal_with_log("Failed to bump room", &e))?;

        self.invalidate_room_cache(user_id, device_id, room_id, conn_id).await;

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
            .update_notification_counts(user_id, device_id, room_id, conn_id, highlight_count, notification_count)
            .await
            .map_err(|e| ApiError::internal_with_log("Failed to update notifications", &e))?;

        self.invalidate_room_cache(user_id, device_id, room_id, conn_id).await;

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
            .map_err(|e| ApiError::internal_with_log("Failed to remove room", &e))?;

        self.invalidate_room_cache(user_id, device_id, room_id, conn_id).await;

        Ok(())
    }

    pub async fn cleanup_expired_tokens(&self) -> Result<u64, ApiError> {
        let count = self
            .storage
            .cleanup_expired_tokens()
            .await
            .map_err(|e| ApiError::internal_with_log("Failed to cleanup tokens", &e))?;

        Ok(count)
    }

    pub async fn get_room_token_sync(
        &self,
        room_id: &str,
        limit: i64,
        from: Option<RoomTokenSyncCursor>,
    ) -> Result<(Vec<AdminRoomTokenSyncEntry>, i64), ApiError> {
        let entries = self
            .storage
            .list_room_token_sync(room_id, limit, from.as_ref())
            .await
            .map_err(|e| ApiError::internal_with_log("Failed to list room token sync", &e))?;

        let total = self
            .storage
            .count_room_token_sync(room_id)
            .await
            .map_err(|e| ApiError::internal_with_log("Failed to count room token sync", &e))?;

        Ok((entries, total))
    }

    async fn invalidate_room_cache(&self, user_id: &str, device_id: &str, room_id: &str, conn_id: Option<&str>) {
        let cache_key = if let Some(cid) = conn_id {
            format!("sliding_sync:room:{user_id}:{device_id}:{cid}:{room_id}")
        } else {
            format!("sliding_sync:room:{user_id}:{device_id}::{room_id}")
        };
        let _ = self.cache.delete(&cache_key).await;
    }

    /// Build the connection tracker key from (user_id, device_id, conn_id).
    fn connection_tracker_key(user_id: &str, device_id: &str, conn_id: Option<&str>) -> String {
        match conn_id {
            Some(cid) => format!("{user_id}:{device_id}:{cid}"),
            None => format!("{user_id}:{device_id}:"),
        }
    }

    /// Lazy GC: remove stale connection data (DB rows + cache entries) for the
    /// given user/device. A connection is considered expired when its
    /// `last_accessed_ts` is older than `CONNECTION_TTL_MS` **and** it has
    /// already been evicted from the moka TTI cache.
    ///
    /// The moka cache handles LRU eviction automatically (via `max_capacity`)
    /// and TTL expiry (via `time_to_idle`). When an entry is no longer in the
    /// tracker it means the connection has been idle beyond the TTL window, so
    /// we clean up the associated DB rows and cache keys.
    async fn gc_expired_connections(&self, user_id: &str, device_id: &str) {
        // Retrieve all connection IDs known in the DB for this user/device.
        let lists = match self.storage.get_lists(user_id, device_id, None).await {
            Ok(lists) => lists,
            Err(e) => {
                tracing::debug!("gc_expired_connections: failed to list connections: {e}");
                return;
            }
        };

        // Collect distinct conn_ids from the DB.
        let conn_ids: std::collections::HashSet<Option<String>> = lists.into_iter().map(|l| l.conn_id).collect();

        let mut expired_count = 0u64;

        for conn_id in &conn_ids {
            let tracker_key = Self::connection_tracker_key(user_id, device_id, conn_id.as_deref());

            // If the connection is still in the tracker, it's alive — skip.
            if self.connection_tracker.get(&tracker_key).is_some() {
                continue;
            }

            // The connection is not tracked (evicted by moka TTI/LRU).
            // Check whether it has truly expired (last access older than TTL).
            // Since moka already evicted it, we know it's been idle > TTL.
            // Clean up DB rows and cache entries.
            tracing::info!(
                user_id = %user_id,
                device_id = %device_id,
                conn_id = ?conn_id,
                "gc_expired_connections: cleaning up expired connection"
            );

            // Delete DB data for this connection.
            if let Err(e) = self.storage.delete_connection_data(user_id, device_id, conn_id.as_deref()).await {
                tracing::warn!(
                    error = %e,
                    user_id = %user_id,
                    device_id = %device_id,
                    conn_id = ?conn_id,
                    "gc_expired_connections: failed to delete connection data"
                );
                continue;
            }

            // Invalidate cache entries for this connection.
            self.invalidate_connection_cache(user_id, device_id, conn_id.as_deref()).await;

            expired_count += 1;
        }

        if expired_count > 0 {
            tracing::info!(
                expired_count = expired_count,
                user_id = %user_id,
                device_id = %device_id,
                "gc_expired_connections: cleaned up expired connections"
            );
        }
    }

    /// Invalidate all cache entries associated with a specific connection.
    async fn invalidate_connection_cache(&self, user_id: &str, device_id: &str, conn_id: Option<&str>) {
        let prefixes = [
            Self::list_snapshot_cache_key_prefix(user_id, device_id, conn_id),
            Self::e2ee_device_list_stream_cache_key_prefix(user_id, device_id, conn_id),
            Self::room_cache_key_prefix(user_id, device_id, conn_id),
        ];

        for prefix in prefixes {
            let keys = self.cache.get_keys_with_prefix(&prefix);
            for key in keys {
                let _ = self.cache.delete(&key).await;
            }
        }
    }

    fn list_snapshot_cache_key_prefix(user_id: &str, device_id: &str, conn_id: Option<&str>) -> String {
        match conn_id {
            Some(cid) => format!("sliding_sync:list:{user_id}:{device_id}:{cid}:"),
            None => format!("sliding_sync:list:{user_id}:{device_id}::"),
        }
    }

    fn e2ee_device_list_stream_cache_key_prefix(user_id: &str, device_id: &str, conn_id: Option<&str>) -> String {
        match conn_id {
            Some(cid) => format!("sliding_sync:e2ee:{user_id}:{device_id}:{cid}"),
            None => format!("sliding_sync:e2ee:{user_id}:{device_id}:"),
        }
    }

    fn room_cache_key_prefix(user_id: &str, device_id: &str, conn_id: Option<&str>) -> String {
        match conn_id {
            Some(cid) => format!("sliding_sync:room:{user_id}:{device_id}:{cid}:"),
            None => format!("sliding_sync:room:{user_id}:{device_id}::"),
        }
    }

    async fn build_e2ee_extension(
        &self,
        user_id: &str,
        device_id: &str,
        conn_id: Option<&str>,
        since_pos: Option<&str>,
    ) -> Result<Value, sqlx::Error> {
        let device_key_storage = DeviceKeyStorage::new(&self.event_storage.pool);
        let key_counts = device_key_storage
            .get_one_time_keys_count_by_algorithm(user_id, device_id)
            .await
            .map_err(|e| sqlx::Error::Protocol(e.to_string()))?;

        let cache_key = Self::e2ee_device_list_stream_cache_key(user_id, device_id, conn_id);
        let since_stream_id = if since_pos.is_some() {
            self.cache.get_raw(&cache_key).and_then(|raw| raw.parse::<i64>().ok()).unwrap_or(0)
        } else {
            0
        };
        let current_stream_id = self.get_current_device_list_stream_id().await?;
        let device_lists = self.get_device_lists_since(user_id, since_stream_id).await?;

        self.cache.set_raw(&cache_key, &current_stream_id.to_string(), 3600).await;

        let mut otk_counts = serde_json::Map::new();
        for (algo, count) in key_counts {
            otk_counts.insert(algo, json!(count));
        }

        let unused_fallback_types =
            device_key_storage.get_unused_fallback_key_types(user_id, device_id).await.unwrap_or_else(|_| vec![]);

        Ok(json!({
            "device_lists": device_lists,
            "device_one_time_keys_count": otk_counts,
            "device_unused_fallback_key_types": unused_fallback_types,
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

        let (events, next_batch) =
            self.get_to_device_extension_payload(user_id, device_id, since_stream_id, limit).await?;

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
        let required_state =
            value.get("required_state").and_then(|v| serde_json::from_value::<Vec<Vec<String>>>(v.clone()).ok());

        RoomSubscriptionConfig { timeline_limit, required_state }
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

        let mut events = self.event_storage.get_room_events_paginated(room_id, None, i64::from(limit) + 1, "b").await?;
        let limited = events.len() > limit as usize;
        if limited {
            events.truncate(limit as usize);
        }
        events.reverse();

        let prev_batch = events.first().map(|event| format!("t{}", event.origin_server_ts));
        let timeline = events.iter().map(Self::room_event_to_json).collect();
        Ok((timeline, limited, prev_batch))
    }

    fn required_state_matches(required_state: &[Vec<String>], event: &StateEvent) -> bool {
        let event_type = event.event_type.as_deref().unwrap_or_default();
        let state_key = event.state_key.as_deref().unwrap_or_default();
        required_state.iter().any(|entry| {
            let event_type_match = entry.first().is_some_and(|value| value == "*" || value == event_type);
            let state_key_match = entry.get(1).is_some_and(|value| value == "*" || value == state_key);
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
                    "range": [range.start, range.start + range.room_ids.len().saturating_sub(1) as u32],
                    "room_ids": range.room_ids,
                })
            })
            .collect()
    }

    fn build_incremental_ops(
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

    fn list_snapshot_cache_key(user_id: &str, device_id: &str, conn_id: Option<&str>, list_key: &str) -> String {
        match conn_id {
            Some(conn_id) => {
                format!("sliding_sync:list:{user_id}:{device_id}:{conn_id}:{list_key}")
            }
            None => format!("sliding_sync:list:{user_id}:{device_id}::{list_key}"),
        }
    }

    fn e2ee_device_list_stream_cache_key(user_id: &str, device_id: &str, conn_id: Option<&str>) -> String {
        match conn_id {
            Some(conn_id) => format!("sliding_sync:e2ee:{user_id}:{device_id}:{conn_id}"),
            None => format!("sliding_sync:e2ee:{user_id}:{device_id}:"),
        }
    }

    async fn get_current_device_list_stream_id(&self) -> Result<i64, sqlx::Error> {
        sqlx::query_scalar::<_, i64>(
            r"
            SELECT COALESCE(MAX(stream_id), 0)
            FROM device_lists_stream
            ",
        )
        .fetch_one(&*self.event_storage.pool)
        .await
    }

    async fn get_device_lists_since(&self, user_id: &str, since_stream_id: i64) -> Result<Value, sqlx::Error> {
        let changed_rows = sqlx::query!(
            r#"
            SELECT DISTINCT dls.user_id
            FROM device_lists_stream dls
            INNER JOIN room_memberships rm1 ON rm1.user_id = dls.user_id AND rm1.membership = 'join'
            INNER JOIN room_memberships rm2 ON rm2.room_id = rm1.room_id AND rm2.user_id = $2 AND rm2.membership = 'join'
            WHERE dls.stream_id > $1
              AND dls.user_id != $2
            ORDER BY dls.user_id
            LIMIT 100
            "#,
            since_stream_id,
            user_id
        )
        .fetch_all(&*self.event_storage.pool)
        .await?;

        let changed: Vec<String> = changed_rows.into_iter().map(|row| row.user_id).collect();

        let left_rows = sqlx::query!(
            r#"
            SELECT DISTINCT dls.user_id
            FROM device_lists_stream dls
            WHERE dls.stream_id > $1
              AND dls.user_id != $2
              AND NOT EXISTS (
                SELECT 1 FROM room_memberships rm1
                INNER JOIN room_memberships rm2 ON rm2.room_id = rm1.room_id AND rm2.user_id = $2 AND rm2.membership = 'join'
                WHERE rm1.user_id = dls.user_id AND rm1.membership = 'join'
              )
            ORDER BY dls.user_id
            LIMIT 100
            "#,
            since_stream_id,
            user_id
        )
        .fetch_all(&*self.event_storage.pool)
        .await?;

        let left: Vec<String> = left_rows.into_iter().map(|row| row.user_id).collect();

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
        let rows = sqlx::query!(
            r#"
            SELECT stream_id, sender_user_id, sender_device_id, event_type, content, message_id
            FROM to_device_messages
            WHERE recipient_user_id = $1
              AND recipient_device_id = $2
              AND stream_id > $3
            ORDER BY stream_id ASC
            LIMIT $4
            "#,
            user_id,
            device_id,
            since_stream_id,
            limit
        )
        .fetch_all(&*self.event_storage.pool)
        .await?;

        let mut last_stream_id = since_stream_id;
        let events: Vec<Value> = rows
            .into_iter()
            .map(|row| {
                let stream_id = row.stream_id;
                let sender = row.sender_user_id;
                let _sender_device = row.sender_device_id;
                let event_type = row.event_type;
                let content = row.content;
                let message_id = row.message_id;
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
            Some(self.get_current_to_device_stream_id(user_id, device_id).await?.to_string())
        } else {
            Some(last_stream_id.to_string())
        };

        Ok((events, next_batch))
    }

    async fn get_current_to_device_stream_id(&self, user_id: &str, device_id: &str) -> Result<i64, sqlx::Error> {
        sqlx::query_scalar::<_, i64>(
            r"
            SELECT COALESCE(MAX(stream_id), 0)
            FROM to_device_messages
            WHERE recipient_user_id = $1
              AND recipient_device_id = $2
            ",
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
        let _service = create_test_service();
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
            is_invited: false,
            name: Some("Test Room".to_string()),
            avatar: Some("mxc://example.com/avatar".to_string()),
            timestamp: 1234567890000,
            created_ts: 1234567890000,
            updated_ts: 1234567890000,
        };

        let json = SlidingSyncService::room_to_json(&room);

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
            room_ids: vec!["!room1:example.com".to_string(), "!room2:example.com".to_string()],
        }]);

        assert_eq!(ops.len(), 1);
        assert_eq!(ops[0]["op"], "SYNC");
    }

    #[tokio::test]
    async fn test_build_incremental_ops_uses_insert_and_delete() {
        let _service = create_test_service();
        let previous = SlidingListWindowSnapshot {
            ranges: vec![SlidingListRangeSnapshot {
                start: 0,
                end: 1,
                room_ids: vec!["!room1:example.com".to_string(), "!room2:example.com".to_string()],
            }],
        };
        let current = vec![SlidingListRangeSnapshot {
            start: 0,
            end: 1,
            room_ids: vec!["!room0:example.com".to_string(), "!room1:example.com".to_string()],
        }];

        let ops = SlidingSyncService::build_incremental_ops(&previous, &current).unwrap();

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
            cache: Arc::new(CacheManager::new(&crate::cache::CacheConfig::default())),
            event_storage: EventStorage::new(
                &std::sync::Arc::new(
                    sqlx::postgres::PgPoolOptions::new()
                        .max_connections(1)
                        .connect_lazy("postgres://localhost/test")
                        .unwrap(),
                ),
                "localhost".to_string(),
            ),
            typing_service: Arc::new(crate::services::typing_service::TypingService::default()),
            presence_storage: PresenceStorage::new(
                Arc::new(
                    sqlx::postgres::PgPoolOptions::new()
                        .max_connections(1)
                        .connect_lazy("postgres://localhost/test")
                        .unwrap(),
                ),
                Arc::new(CacheManager::new(&crate::cache::CacheConfig::default())),
            ),
            member_storage: RoomMemberStorage::new(
                &Arc::new(
                    sqlx::postgres::PgPoolOptions::new()
                        .max_connections(1)
                        .connect_lazy("postgres://localhost/test")
                        .unwrap(),
                ),
                "localhost",
            ),
            connection_tracker: Arc::new(
                moka::sync::Cache::builder()
                    .max_capacity(MAX_TRACKED_CONNECTIONS)
                    .time_to_idle(std::time::Duration::from_millis(CONNECTION_TTL_MS as u64))
                    .build(),
            ),
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
        assert_eq!(json.get("room_name_like").unwrap().as_str().unwrap(), "test");
    }
}
