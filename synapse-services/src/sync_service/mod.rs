mod api;
mod api_trait;
mod data_fetch;
mod event_fetch;
mod filter;
mod lazy_load;
mod metrics;
pub mod push_rules;
mod response;
#[cfg(test)]
mod tests;
mod types;
pub use api_trait::SyncServiceApi;
pub use types::{
    BuildRoomSyncRequest, BuildRoomSyncValueRequest, BuildSyncResponseRequest, FetchEventsRequest, IncrementalUpdate,
    LazyLoadMembersRequest, LazyLoadedMembersCacheKey, RoomFilter, RoomSyncCounts, RoomSyncState,
    StateEventsBatchParams, SyncEventFormat, SyncFilter, SyncPerformanceSnapshot, SyncRequest, SyncResponseFilter,
    SyncRoomSection, SyncServiceDeps, SyncServiceRequest, SyncState, SyncToken,
};

use crate::*;
use serde_json::json;
use std::collections::{HashMap, HashSet};
use std::sync::Arc;
use std::time::Instant;

use synapse_common::*;
use synapse_e2ee::device_keys::DeviceKeyStoreApi;
use synapse_e2ee::key_rotation::KeyRotationStorage;
use synapse_storage::UserRoomMembership;
use tokio::sync::RwLock;

pub struct SyncService {
    pub(crate) presence_storage: Arc<dyn synapse_storage::presence::PresenceStoreApi>,
    pub(crate) member_storage: Arc<dyn synapse_storage::membership::MemberStoreApi>,
    pub(crate) event_reader: Arc<dyn synapse_storage::event::EventReader>,
    pub(crate) room_account_data_storage: Arc<dyn synapse_storage::room_account_data::RoomAccountDataStoreApi>,
    pub(crate) account_data_storage: Arc<dyn synapse_storage::account_data::AccountDataStoreApi>,
    pub(crate) filter_storage: Arc<dyn synapse_storage::filter::FilterStoreApi>,
    pub(crate) device_storage: Arc<dyn synapse_storage::device::DeviceListStoreApi>,
    pub(crate) device_key_storage: Arc<dyn DeviceKeyStoreApi>,
    pub(crate) key_rotation_storage: KeyRotationStorage,
    pub(crate) to_device_storage: synapse_e2ee::to_device::ToDeviceStorage,
    pub(crate) lazy_loaded_members_cache: Arc<RwLock<HashMap<LazyLoadedMembersCacheKey, HashSet<String>>>>,
    pub(crate) metrics: Arc<MetricsCollector>,
    pub(crate) performance: synapse_common::config::PerformanceConfig,
    pub(crate) cache: Arc<synapse_cache::CacheManager>,
    /// S6: event-driven wake-up for v2 /sync long-polling.
    pub(crate) event_notifier: Option<crate::event_notifier::EventNotifier>,
}

/// Maximum number of (user, device, room) entries kept in the in-memory
/// lazy-loaded members cache before the map is cleared. This prevents
/// unbounded memory growth in long-running servers with many users/rooms.
/// The cache is an optimization to avoid repeated DB lookups; clearing it
/// only causes a temporary increase in database queries.
const LAZY_LOADED_MEMBERS_CACHE_MAX_ENTRIES: usize = 50_000;

impl SyncService {
    const TIMESTAMP_TOKEN_MIN: i64 = 1_000_000_000_000;

    pub fn from_deps(deps: SyncServiceDeps) -> Self {
        Self {
            presence_storage: deps.presence_storage,
            member_storage: deps.member_storage,
            event_reader: deps.event_reader,
            room_account_data_storage: deps.room_account_data_storage,
            account_data_storage: deps.account_data_storage,
            filter_storage: deps.filter_storage,
            device_storage: deps.device_storage,
            device_key_storage: deps.device_key_storage,
            key_rotation_storage: deps.key_rotation_storage,
            to_device_storage: deps.to_device_storage,
            lazy_loaded_members_cache: Arc::new(RwLock::new(HashMap::new())),
            metrics: deps.metrics,
            performance: deps.performance,
            cache: deps.cache,
            event_notifier: deps.event_notifier,
        }
    }

    #[allow(clippy::too_many_arguments)]
    pub fn new(
        presence_storage: Arc<synapse_storage::presence::PresenceStorage>,
        member_storage: Arc<synapse_storage::membership::RoomMemberStorage>,
        event_storage: Arc<synapse_storage::event::EventStorage>,
        room_account_data_storage: Arc<dyn synapse_storage::room_account_data::RoomAccountDataStoreApi>,
        account_data_storage: Arc<dyn synapse_storage::account_data::AccountDataStoreApi>,
        filter_storage: Arc<dyn synapse_storage::filter::FilterStoreApi>,
        device_storage: Arc<synapse_storage::device::DeviceStorage>,
        device_key_storage: Arc<dyn DeviceKeyStoreApi>,
        key_rotation_storage: KeyRotationStorage,
        to_device_storage: synapse_e2ee::to_device::ToDeviceStorage,
        metrics: Arc<MetricsCollector>,
        performance: synapse_common::config::PerformanceConfig,
        cache: Arc<synapse_cache::CacheManager>,
        event_notifier: Option<crate::event_notifier::EventNotifier>,
    ) -> Self {
        Self::from_deps(SyncServiceDeps {
            presence_storage,
            member_storage,
            event_reader: event_storage.clone(),
            room_account_data_storage,
            account_data_storage,
            filter_storage,
            device_storage,
            device_key_storage,
            key_rotation_storage,
            to_device_storage,
            metrics,
            performance,
            cache,
            event_notifier,
        })
    }

    #[allow(clippy::too_many_arguments)]
    pub async fn sync(
        &self,
        user_id: &str,
        device_id: Option<&str>,
        timeout: u64,
        full_state: bool,
        set_presence: &str,
        filter_id: Option<&str>,
        since: Option<&str>,
    ) -> ApiResult<serde_json::Value> {
        self.sync_with_request(SyncServiceRequest {
            user_id,
            device_id,
            timeout,
            is_full_state: full_state,
            set_presence,
            filter_id,
            since,
        })
        .await
    }

    pub async fn sync_with_request(&self, request: SyncServiceRequest<'_>) -> ApiResult<serde_json::Value> {
        let SyncServiceRequest { user_id, device_id, timeout, is_full_state, set_presence, filter_id, since } = request;
        let total_started = Instant::now();
        self.update_presence(user_id, set_presence).await?;

        let since_token = since.and_then(SyncToken::parse);

        if let (Some(device_id), Some(token)) = (device_id, &since_token) {
            if let Some(to_device_since) = token.to_device_stream_id {
                let _ = self.to_device_storage.delete_messages_up_to(user_id, device_id, to_device_since).await;
            }
        }

        let response_filter = self.resolve_sync_response_filter(user_id, filter_id).await?;
        let room_filter = response_filter.as_ref().and_then(|filter| filter.room.as_ref());
        let timeline_limit = Self::timeline_limit_from_room_filter(room_filter, self.sync_event_limit());

        let is_incremental = since_token.is_some() && !is_full_state;

        let rooms_started = Instant::now();
        let include_leave = room_filter.and_then(|filter| filter.include_leave).unwrap_or(false);
        let room_memberships = self.member_storage.get_sync_rooms(user_id, include_leave).await?;
        let room_sections =
            Self::room_sections_from_memberships(&Self::filter_sync_rooms(room_memberships, room_filter));
        let room_ids: Vec<String> = room_sections.keys().cloned().collect();
        let rooms_lookup_ms = rooms_started.elapsed().as_secs_f64() * 1000.0;
        self.observe_histogram("sync_rooms_lookup_duration_ms", rooms_lookup_ms);

        let event_fetch_started = Instant::now();
        let room_events = self
            .fetch_events(FetchEventsRequest {
                user_id,
                device_id,
                room_ids: &room_ids,
                since_token: since_token.as_ref(),
                timeout,
                limit: timeline_limit,
                timeline_filter: room_filter.and_then(|filter| filter.timeline.as_ref()),
                is_incremental,
            })
            .await?;
        let event_fetch_ms = event_fetch_started.elapsed().as_secs_f64() * 1000.0;
        self.observe_histogram("sync_event_fetch_duration_ms", event_fetch_ms);

        let room_count = room_ids.len();
        let event_count = Self::count_events_by_room(&room_events);

        let response_build_started = Instant::now();
        let response = self
            .build_sync_response(BuildSyncResponseRequest {
                user_id,
                device_id,
                room_ids: &room_ids,
                room_sections: &room_sections,
                room_events,
                response_filter: response_filter.as_ref(),
                timeline_limit,
                since_token: &since_token,
                is_incremental,
            })
            .await?;
        let response_build_ms = response_build_started.elapsed().as_secs_f64() * 1000.0;
        self.observe_histogram("sync_response_build_duration_ms", response_build_ms);

        let total_ms = total_started.elapsed().as_secs_f64() * 1000.0;
        self.record_sync_request_metrics("sync", total_ms, room_count, event_count, is_incremental);
        self.log_slow_sync_request(&SyncPerformanceSnapshot {
            request_kind: "sync",
            user_id,
            total_ms,
            room_count,
            event_count,
            is_incremental,
            phases: [
                ("rooms_lookup_ms", rooms_lookup_ms),
                ("event_fetch_ms", event_fetch_ms),
                ("response_build_ms", response_build_ms),
            ],
        });

        Ok(response)
    }

    pub async fn room_sync(
        &self,
        user_id: &str,
        room_id: &str,
        timeout: u64,
        is_full_state: bool,
        since: Option<&str>,
    ) -> ApiResult<serde_json::Value> {
        let total_started = Instant::now();
        let since_token = since.and_then(SyncToken::parse);
        let is_incremental = since_token.is_some() && !is_full_state;

        let room_ids = vec![room_id.to_string()];
        let event_fetch_started = Instant::now();
        let room_events = self
            .fetch_events(FetchEventsRequest {
                user_id,
                device_id: None,
                room_ids: &room_ids,
                since_token: since_token.as_ref(),
                timeout,
                limit: self.sync_event_limit(),
                timeline_filter: None,
                is_incremental,
            })
            .await?;
        let event_fetch_ms = event_fetch_started.elapsed().as_secs_f64() * 1000.0;
        self.observe_histogram("room_sync_event_fetch_duration_ms", event_fetch_ms);

        let events = room_events.get(room_id).cloned().unwrap_or_default();
        let event_count = events.len();
        let room_build_started = Instant::now();
        let room_sync = self
            .build_room_sync(BuildRoomSyncRequest {
                room_id,
                user_id,
                device_id: None,
                events,
                since_token: since_token.as_ref(),
                is_incremental,
                room_filter: None,
            })
            .await?;
        let room_build_ms = room_build_started.elapsed().as_secs_f64() * 1000.0;
        self.observe_histogram("room_sync_build_duration_ms", room_build_ms);

        let mut result = match room_sync {
            serde_json::Value::Object(map) => map,
            _ => serde_json::Map::new(),
        };

        let stream_id = Self::next_event_stream_id(&since_token, &room_events, None);
        result.insert(
            "next_batch".to_string(),
            json!(SyncToken {
                stream_id,
                room_id: None,
                event_type: None,
                to_device_stream_id: None,
                device_list_stream_id: None,
            }
            .encode()),
        );

        let total_ms = total_started.elapsed().as_secs_f64() * 1000.0;
        self.record_sync_request_metrics("room_sync", total_ms, 1, event_count, is_incremental);
        self.log_slow_sync_request(&SyncPerformanceSnapshot {
            request_kind: "room_sync",
            user_id,
            total_ms,
            room_count: 1,
            event_count,
            is_incremental,
            phases: [("event_fetch_ms", event_fetch_ms), ("room_build_ms", room_build_ms), ("rooms_lookup_ms", 0.0)],
        });

        Ok(serde_json::Value::Object(result))
    }

    pub async fn room_sync_with_timeout(
        &self,
        user_id: &str,
        room_id: &str,
        timeout: u64,
        is_full_state: bool,
        since: Option<&str>,
    ) -> ApiResult<serde_json::Value> {
        // S13/N6: 外层超时与 /sync 主路径统一为「客户端 timeout + 15s 宽余」。
        // 此前硬编码 60s，客户端 timeout=120s 会在 60s 处被服务端提前截断。
        let server_timeout = synapse_common::constants::sync_server_timeout(timeout);
        let result = tokio::time::timeout(server_timeout, self.room_sync(user_id, room_id, timeout, is_full_state, since)).await;

        match result {
            Ok(Ok(value)) => Ok(value),
            Ok(Err(error)) => {
                ::tracing::error!(
                    user_id = %user_id,
                    room_id = %room_id,
                    server_timeout_ms = server_timeout.as_millis() as u64,
                    requested_timeout_ms = timeout,
                    is_full_state,
                    since = ?since,
                    error = %error,
                    "Room sync error"
                );
                Err(error)
            }
            Err(_) => {
                ::tracing::error!(
                    user_id = %user_id,
                    room_id = %room_id,
                    server_timeout_ms = server_timeout.as_millis() as u64,
                    requested_timeout_ms = timeout,
                    is_full_state,
                    since = ?since,
                    "Room sync timeout"
                );
                Err(ApiError::internal("Room sync operation timed out".to_string()))
            }
        }
    }

    pub async fn room_unread_counts(&self, room_id: &str, user_id: &str) -> ApiResult<(i64, i64)> {
        let (highlight_count, notification_count) = self.get_unread_counts(room_id, user_id).await?;
        Ok((notification_count, highlight_count))
    }

    pub(crate) fn rooms_to_include(
        room_ids: &[String],
        room_events: &HashMap<String, Vec<RoomEvent>>,
        changed_members_by_room: &HashMap<String, HashSet<String>>,
        state_change_ts_by_room: &HashMap<String, i64>,
        is_incremental: bool,
    ) -> Vec<String> {
        if !is_incremental {
            return room_ids.to_vec();
        }

        room_ids
            .iter()
            .filter(|room_id| {
                room_events.get(*room_id).is_some_and(|events| !events.is_empty())
                    || changed_members_by_room.get(*room_id).is_some_and(|members| !members.is_empty())
                    || state_change_ts_by_room.get(*room_id).is_some_and(|&ts| ts > 0)
            })
            .cloned()
            .collect()
    }

    pub(crate) fn filter_sync_rooms(
        memberships: Vec<UserRoomMembership>,
        room_filter: Option<&RoomFilter>,
    ) -> Vec<UserRoomMembership> {
        let allowed_rooms = room_filter.and_then(|filter| filter.rooms.as_ref());
        let disallowed_rooms = room_filter.and_then(|filter| filter.not_rooms.as_ref());

        memberships
            .into_iter()
            .filter(|membership| {
                if let Some(rooms) = allowed_rooms {
                    if !rooms.is_empty() && !rooms.iter().any(|room| room == &membership.room_id) {
                        return false;
                    }
                }

                if let Some(not_rooms) = disallowed_rooms {
                    if not_rooms.iter().any(|room| room == &membership.room_id) {
                        return false;
                    }
                }

                true
            })
            .collect()
    }

    pub(crate) fn room_sections_from_memberships(
        memberships: &[UserRoomMembership],
    ) -> HashMap<String, SyncRoomSection> {
        memberships
            .iter()
            .map(|membership| {
                let section = match membership.membership.as_str() {
                    "leave" => SyncRoomSection::Leave,
                    "invite" => SyncRoomSection::Invite,
                    _ => SyncRoomSection::Join,
                };
                (membership.room_id.clone(), section)
            })
            .collect()
    }

    pub(crate) fn event_since_ts(since_token: &Option<SyncToken>) -> i64 {
        match since_token {
            // S6: timestamp-based tokens (stream_id >= 1e12) are no longer
            // treated as origin_server_ts. Return 0 so callers fall back to
            // StreamOrdering(0) for a full resync instead of the racy
            // OriginServerTs path that has same-millisecond漏读 risk.
            Some(token) if token.to_device_stream_id.is_some() || token.device_list_stream_id.is_some() => {
                token.stream_id.max(0)
            }
            Some(_) => 0,
            None => 0,
        }
    }

    pub(crate) fn next_event_stream_id(
        since_token: &Option<SyncToken>,
        room_events: &HashMap<String, Vec<RoomEvent>>,
        state_change_ts_by_room: Option<&HashMap<String, i64>>,
    ) -> i64 {
        let _ = state_change_ts_by_room; // S6: state change timestamps no longer used for token generation
        let event_max_stream = room_events.values().flat_map(|v| v.iter()).filter_map(|e| e.stream_ordering).max();

        if let Some(max_stream) = event_max_stream {
            match since_token.as_ref() {
                Some(token) => max_stream.max(token.stream_id),
                None => max_stream,
            }
        } else {
            // S6: no events with stream_ordering — preserve the since_token's
            // stream_id (or 0 if no token). Do NOT fall back to origin_server_ts,
            // which would create a timestamp-based token with same-millisecond
            // race conditions.
            match since_token.as_ref() {
                Some(token) => token.stream_id.max(0),
                None => 0,
            }
        }
    }
}
