use super::types::*;
use super::SyncService;
use crate::map_internal;
use serde_json::{json, Value};
use std::collections::{HashMap, HashSet};
use synapse_common::*;
use synapse_storage::event::SinceFilter;

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
            .event_reader
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
            self.event_reader
                .get_state_events_since_batch(room_ids, SinceFilter::StreamOrdering(stream_ord))
                .await
                .map_err(|e| ApiError::internal_with_log("Failed to get room state events", &e))?
        } else {
            self.event_reader
                .get_state_events_since_batch(room_ids, SinceFilter::OriginServerTs(params.since_ts))
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
            self.event_reader
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
            .event_reader
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
        // Cache-through: account data changes infrequently, so a 600 s TTL
        // with write-through invalidation is safe (OPT-015-b, audit 04 §5).
        const ACCOUNT_DATA_CACHE_TTL_SECS: u64 = 600;
        let cache_key = format!("account_data:{user_id}");
        if let Ok(Some(cached)) = self.cache.get::<Vec<serde_json::Value>>(&cache_key).await {
            return Ok(cached);
        }

        let rows = self
            .account_data_storage
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

        let _ = self.cache.set(&cache_key, &events, ACCOUNT_DATA_CACHE_TTL_SECS).await;

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

        // Cache the GLOBAL (not per-user) device-list max stream id on the /sync
        // hot path with a short 5s TTL and no invalidation: staleness is bounded
        // because the next sync (≤5s later) re-reads it (OPT-015-c, audit 04 §5).
        const DEVICE_LIST_MAX_STREAM_CACHE_KEY: &str = "device_list_max_stream_id";
        const DEVICE_LIST_MAX_STREAM_TTL_SECS: u64 = 5;
        let max_stream_id: i64 = match self.cache.get::<i64>(DEVICE_LIST_MAX_STREAM_CACHE_KEY).await {
            Ok(Some(v)) => v,
            _ => {
                let v = self
                    .device_storage
                    .get_max_device_list_stream_id()
                    .await
                    .map_err(map_internal!("Failed to get device list stream position"))?;
                let _ = self.cache.set(DEVICE_LIST_MAX_STREAM_CACHE_KEY, v, DEVICE_LIST_MAX_STREAM_TTL_SECS).await;
                v
            }
        };

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
            .event_reader
            .get_room_events_batch_since_filtered(
                &room_ids,
                SinceFilter::StreamOrdering(since_token.stream_id),
                1000,
                &filter,
            )
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
            .event_reader
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
            .event_reader
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
        let rows = self
            .room_account_data_storage
            .list_room_account_data(user_id, room_id)
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

        let rows = self
            .room_account_data_storage
            .list_room_account_data_batch(user_id, room_ids)
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
            .event_reader
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
            .event_reader
            .get_unread_counts_batch(room_ids, user_id)
            .await
            .map_err(map_internal!("Failed to get unread counts"))?;

        for row in rows {
            result.insert(row.room_id, (row.highlight_count, row.notification_count));
        }

        Ok(result)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::sync_service::types::SyncServiceDeps;
    use std::collections::HashSet;
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::sync::Arc;
    use synapse_storage::account_data::AccountDataStoreApi;
    use synapse_storage::device::{Device, DeviceListStoreApi};
    use synapse_storage::test_mocks::InMemoryDeviceListStore;

    /// [`AccountDataStoreApi`] test double that counts how many times
    /// `list_account_data` is called, delegating every method to an inner
    /// [`InMemoryAccountDataStore`]. Used to prove OPT-015-b caches
    /// account data so two `/sync` calls hit storage only once.
    #[derive(Debug)]
    struct CountingAccountDataStore {
        inner: synapse_storage::test_mocks::InMemoryAccountDataStore,
        list_calls: Arc<AtomicUsize>,
    }

    #[async_trait::async_trait]
    impl synapse_storage::account_data::AccountDataStoreApi for CountingAccountDataStore {
        async fn list_account_data(
            &self,
            user_id: &str,
        ) -> Result<Vec<synapse_storage::account_data::AccountDataRecord>, ApiError> {
            self.list_calls.fetch_add(1, Ordering::SeqCst);
            self.inner.list_account_data(user_id).await
        }

        async fn get_account_data_content(
            &self,
            user_id: &str,
            data_type: &str,
        ) -> Result<Option<serde_json::Value>, ApiError> {
            self.inner.get_account_data_content(user_id, data_type).await
        }

        async fn delete_account_data(&self, user_id: &str, data_type: &str) -> Result<bool, ApiError> {
            self.inner.delete_account_data(user_id, data_type).await
        }

        async fn upsert_account_data(
            &self,
            user_id: &str,
            data_type: &str,
            content: serde_json::Value,
        ) -> Result<(), ApiError> {
            self.inner.upsert_account_data(user_id, data_type, content).await
        }
    }

    /// Builds a [`SyncService`] over in-memory account-data and member stores
    /// plus a lazy pool for other storages, plus an in-memory cache.
    fn sync_service_with_account_data_store(
        account_data_store: Arc<dyn synapse_storage::account_data::AccountDataStoreApi>,
    ) -> SyncService {
        let pool: Arc<sqlx::PgPool> = Arc::new(
            sqlx::postgres::PgPoolOptions::new()
                .connect_lazy("postgres://synapse:synapse@localhost/synapse")
                .expect("lazy pool"),
        );
        let cache = Arc::new(synapse_cache::CacheManager::new(&synapse_cache::CacheConfig::default()));

        // Use in-memory member store so get_joined_rooms works without a real DB.
        let member_store: Arc<dyn synapse_storage::membership::MemberStoreApi> =
            Arc::new(synapse_storage::test_mocks::InMemoryMemberStore::new());

        SyncService::from_deps(SyncServiceDeps {
            presence_storage: Arc::new(synapse_storage::presence::PresenceStorage::new(pool.clone(), cache.clone())),
            member_storage: member_store,
            event_reader: Arc::new(synapse_storage::event::EventStorage::new(&pool, "localhost".to_string())),
            room_storage: Arc::new(synapse_storage::room::RoomStorage::new(&pool)),
            room_account_data_storage: Arc::new(synapse_storage::room_account_data::RoomAccountDataStorage::new(&pool)),
            account_data_storage: account_data_store,
            filter_storage: Arc::new(synapse_storage::filter::FilterStorage::new(&pool)),
            device_storage: Arc::new(synapse_storage::test_mocks::InMemoryDeviceListStore::new()),
            device_key_storage: synapse_e2ee::device_keys::DeviceKeyStorage::new(&pool),
            key_rotation_storage: synapse_e2ee::key_rotation::KeyRotationStorage::new(pool.clone()),
            to_device_storage: synapse_e2ee::to_device::ToDeviceStorage::new(&pool),
            metrics: Arc::new(synapse_common::MetricsCollector::new()),
            performance: synapse_common::config::PerformanceConfig::default(),
            cache,
        })
    }

    #[tokio::test]
    async fn account_data_events_are_cached_after_first_read() {
        let calls = Arc::new(AtomicUsize::new(0));
        let inner = synapse_storage::test_mocks::InMemoryAccountDataStore::new();

        // Seed one account-data row so the output is non-empty.
        inner
            .upsert_account_data(
                "@alice:localhost",
                "m.direct",
                serde_json::json!({"@bob:localhost": ["!room1:localhost"]}),
            )
            .await
            .expect("seed account data");

        let account_data_store: Arc<dyn synapse_storage::account_data::AccountDataStoreApi> =
            Arc::new(CountingAccountDataStore { inner, list_calls: calls.clone() });

        let sync = sync_service_with_account_data_store(account_data_store);

        let first = sync.get_account_data_events("@alice:localhost").await.expect("first call");
        let second = sync.get_account_data_events("@alice:localhost").await.expect("second call");

        assert_eq!(first, second, "both calls must return the same account data");
        assert_eq!(
            calls.load(Ordering::SeqCst),
            1,
            "account data must be read from storage exactly once across two syncs",
        );
    }

    /// [`DeviceListStoreApi`] test double that counts how many times the GLOBAL
    /// device-list max stream id is read, delegating every other method to an
    /// inner [`InMemoryDeviceListStore`]. Used to prove OPT-015-c caches the
    /// max stream id so two `/sync` calls hit storage only once.
    struct CountingDeviceListStore {
        inner: InMemoryDeviceListStore,
        max_stream_id_calls: Arc<AtomicUsize>,
    }

    #[async_trait::async_trait]
    impl DeviceListStoreApi for CountingDeviceListStore {
        async fn get_max_device_list_stream_id(&self) -> Result<i64, sqlx::Error> {
            self.max_stream_id_calls.fetch_add(1, Ordering::SeqCst);
            self.inner.get_max_device_list_stream_id().await
        }

        async fn insert_device_list_change(
            &self,
            user_id: &str,
            device_id: Option<&str>,
            change_type: &str,
            stream_id: i64,
        ) -> Result<(), sqlx::Error> {
            self.inner.insert_device_list_change(user_id, device_id, change_type, stream_id).await
        }

        async fn create_device(
            &self,
            device_id: &str,
            user_id: &str,
            display_name: Option<&str>,
        ) -> Result<Device, sqlx::Error> {
            self.inner.create_device(device_id, user_id, display_name).await
        }

        async fn delete_device(&self, device_id: &str) -> Result<(), sqlx::Error> {
            self.inner.delete_device(device_id).await
        }

        async fn get_user_devices(&self, user_id: &str) -> Result<Vec<Device>, sqlx::Error> {
            self.inner.get_user_devices(user_id).await
        }

        async fn get_device(&self, device_id: &str) -> Result<Option<Device>, sqlx::Error> {
            self.inner.get_device(device_id).await
        }

        async fn update_user_device_display_name(
            &self,
            user_id: &str,
            device_id: &str,
            display_name: &str,
        ) -> Result<u64, sqlx::Error> {
            self.inner.update_user_device_display_name(user_id, device_id, display_name).await
        }

        async fn get_max_device_list_stream_id_for_user(&self, user_id: &str) -> Result<i64, sqlx::Error> {
            self.inner.get_max_device_list_stream_id_for_user(user_id).await
        }

        async fn get_device_list_changed_users(
            &self,
            from: i64,
            to: i64,
            requester_id: &str,
        ) -> Result<Vec<String>, sqlx::Error> {
            self.inner.get_device_list_changed_users(from, to, requester_id).await
        }

        async fn get_device_list_left_users(
            &self,
            from: i64,
            to: i64,
            requester_id: &str,
        ) -> Result<Vec<String>, sqlx::Error> {
            self.inner.get_device_list_left_users(from, to, requester_id).await
        }

        async fn get_users_devices_batch(&self, users: &[String]) -> Result<HashMap<String, Vec<Device>>, sqlx::Error> {
            self.inner.get_users_devices_batch(users).await
        }

        async fn get_device_list_changes(
            &self,
            since: i64,
            to: i64,
            users: &[String],
        ) -> Result<Vec<(String, Option<String>, String, i64)>, sqlx::Error> {
            self.inner.get_device_list_changes(since, to, users).await
        }

        async fn get_devices_by_user_device_pairs(
            &self,
            user_ids: &[&str],
            device_ids: &[&str],
        ) -> Result<Vec<(String, String, Option<String>, Option<i64>)>, sqlx::Error> {
            self.inner.get_devices_by_user_device_pairs(user_ids, device_ids).await
        }

        async fn filter_existing_users(&self, users: &[String]) -> Result<Vec<String>, sqlx::Error> {
            self.inner.filter_existing_users(users).await
        }

        async fn has_device_list_updates_since(&self, since_stream_id: i64) -> Result<bool, sqlx::Error> {
            self.inner.has_device_list_updates_since(since_stream_id).await
        }

        async fn get_device_lists_since_with_shared_rooms(
            &self,
            since_stream_id: i64,
            exclude_user_id: &str,
        ) -> Result<(Vec<String>, Vec<String>), sqlx::Error> {
            self.inner.get_device_lists_since_with_shared_rooms(since_stream_id, exclude_user_id).await
        }

        async fn get_lazy_loaded_members(
            &self,
            user_id: &str,
            device_id: &str,
            room_id: &str,
        ) -> Result<HashSet<String>, sqlx::Error> {
            self.inner.get_lazy_loaded_members(user_id, device_id, room_id).await
        }

        async fn upsert_lazy_loaded_members(
            &self,
            user_id: &str,
            device_id: &str,
            room_id: &str,
            member_user_ids: &HashSet<String>,
        ) -> Result<u64, sqlx::Error> {
            self.inner.upsert_lazy_loaded_members(user_id, device_id, room_id, member_user_ids).await
        }

        async fn delete_user_devices_batch(&self, user_id: &str, device_ids: &[String]) -> Result<u64, sqlx::Error> {
            self.inner.delete_user_devices_batch(user_id, device_ids).await
        }

        async fn get_device_by_id(&self, device_id: &str) -> Result<Option<Device>, sqlx::Error> {
            self.inner.get_device_by_id(device_id).await
        }

        async fn delete_device_returning_count(&self, user_id: &str, device_id: &str) -> Result<u64, sqlx::Error> {
            self.inner.delete_device_returning_count(user_id, device_id).await
        }

        async fn delete_all_devices(&self, user_id: &str) -> Result<(), sqlx::Error> {
            self.inner.delete_all_devices(user_id).await
        }

        async fn get_device_count(&self, user_id: &str) -> Result<i64, sqlx::Error> {
            self.inner.get_device_count(user_id).await
        }
    }

    /// Builds a [`SyncService`] over a lazily-connected pool (never queried in
    /// this test because `since` is `None`) plus an in-memory cache and the
    /// supplied counting device store. Mirrors the helper in `filter.rs`.
    fn sync_service_with_device_store(device_store: Arc<dyn DeviceListStoreApi>) -> SyncService {
        let pool: Arc<sqlx::PgPool> = Arc::new(
            sqlx::postgres::PgPoolOptions::new()
                .connect_lazy("postgres://synapse:synapse@localhost/synapse")
                .expect("lazy pool"),
        );
        let cache = Arc::new(synapse_cache::CacheManager::new(&synapse_cache::CacheConfig::default()));

        SyncService::from_deps(SyncServiceDeps {
            presence_storage: Arc::new(synapse_storage::presence::PresenceStorage::new(pool.clone(), cache.clone())),
            member_storage: Arc::new(synapse_storage::membership::RoomMemberStorage::new(&pool, "localhost")),
            event_reader: Arc::new(synapse_storage::event::EventStorage::new(&pool, "localhost".to_string())),
            room_storage: Arc::new(synapse_storage::room::RoomStorage::new(&pool)),
            room_account_data_storage: Arc::new(synapse_storage::room_account_data::RoomAccountDataStorage::new(&pool)),
            account_data_storage: Arc::new(synapse_storage::account_data::AccountDataStorage::new(&pool)),
            filter_storage: Arc::new(synapse_storage::filter::FilterStorage::new(&pool)),
            device_storage: device_store,
            device_key_storage: synapse_e2ee::device_keys::DeviceKeyStorage::new(&pool),
            key_rotation_storage: synapse_e2ee::key_rotation::KeyRotationStorage::new(pool.clone()),
            to_device_storage: synapse_e2ee::to_device::ToDeviceStorage::new(&pool),
            metrics: Arc::new(synapse_common::MetricsCollector::new()),
            performance: synapse_common::config::PerformanceConfig::default(),
            cache,
        })
    }

    #[tokio::test]
    async fn device_list_max_stream_id_is_cached_after_first_read() {
        let calls = Arc::new(AtomicUsize::new(0));
        let inner = InMemoryDeviceListStore::new();
        // Bump the in-memory stream position so the cached value is non-zero.
        inner.create_device("DEV1", "@bob:localhost", None).await.expect("seed device");

        let device_store: Arc<dyn DeviceListStoreApi> =
            Arc::new(CountingDeviceListStore { inner, max_stream_id_calls: calls.clone() });

        let sync = sync_service_with_device_store(device_store);

        // `since = None` keeps the left-user path from touching other storages,
        // so only the device store is exercised on the lazy pool.
        let (_first, first_max) = sync.get_device_lists("@alice:localhost", &None).await.expect("first call");
        let (_second, second_max) = sync.get_device_lists("@alice:localhost", &None).await.expect("second call");

        assert_eq!(first_max, second_max, "both calls must return the same max stream id");
        assert_eq!(
            calls.load(Ordering::SeqCst),
            1,
            "the global device-list max stream id must be read from storage exactly once across two syncs",
        );
    }
}
