mod api;
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

pub use types::*;

use synapse_common::*;
use crate::*;
use synapse_storage::{PresenceStorage, UserRoomMembership};
use serde_json::json;
use std::collections::{HashMap, HashSet};
use std::sync::Arc;
use std::time::Instant;
use tokio::sync::RwLock;

pub struct SyncService {
    pub(crate) presence_storage: PresenceStorage,
    pub(crate) member_storage: RoomMemberStorage,
    pub(crate) event_storage: EventStorage,
    pub(crate) room_storage: RoomStorage,
    pub(crate) filter_storage: FilterStorage,
    pub(crate) device_storage: DeviceStorage,
    pub(crate) to_device_storage: synapse_e2ee::to_device::ToDeviceStorage,
    pub(crate) lazy_loaded_members_cache: Arc<RwLock<HashMap<LazyLoadedMembersCacheKey, HashSet<String>>>>,
    pub(crate) metrics: Arc<MetricsCollector>,
    pub(crate) performance: synapse_common::config::PerformanceConfig,
}

impl SyncService {
    const TIMESTAMP_TOKEN_MIN: i64 = 1_000_000_000_000;

    pub fn from_deps(deps: SyncServiceDeps) -> Self {
        Self {
            presence_storage: deps.presence_storage,
            member_storage: deps.member_storage,
            event_storage: deps.event_storage,
            room_storage: deps.room_storage,
            filter_storage: deps.filter_storage,
            device_storage: deps.device_storage,
            to_device_storage: deps.to_device_storage,
            lazy_loaded_members_cache: Arc::new(RwLock::new(HashMap::new())),
            metrics: deps.metrics,
            performance: deps.performance,
        }
    }

    #[allow(clippy::too_many_arguments)]
    pub fn new(
        presence_storage: PresenceStorage,
        member_storage: RoomMemberStorage,
        event_storage: EventStorage,
        room_storage: RoomStorage,
        filter_storage: FilterStorage,
        device_storage: DeviceStorage,
        to_device_storage: synapse_e2ee::to_device::ToDeviceStorage,
        metrics: Arc<MetricsCollector>,
        performance: synapse_common::config::PerformanceConfig,
    ) -> Self {
        Self::from_deps(SyncServiceDeps {
            presence_storage,
            member_storage,
            event_storage,
            room_storage,
            filter_storage,
            device_storage,
            to_device_storage,
            metrics,
            performance,
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

        let since_token = since.and_then(SyncToken::parse);
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
        let result = tokio::time::timeout(
            std::time::Duration::from_secs(60),
            self.room_sync(user_id, room_id, timeout, is_full_state, since),
        )
        .await;

        match result {
            Ok(Ok(value)) => Ok(value),
            Ok(Err(error)) => {
                ::tracing::error!("Room sync error for user {} in room {}: {}", user_id, room_id, error);
                Err(error)
            }
            Err(_) => {
                ::tracing::error!("Room sync timeout for user {} in room {}", user_id, room_id);
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
                let section =
                    if membership.membership == "leave" { SyncRoomSection::Leave } else { SyncRoomSection::Join };
                (membership.room_id.clone(), section)
            })
            .collect()
    }

    pub(crate) fn event_since_ts(since_token: &Option<SyncToken>) -> i64 {
        match since_token {
            Some(token) if token.stream_id >= Self::TIMESTAMP_TOKEN_MIN => token.stream_id,
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
        let event_max_stream = room_events.values().flat_map(|v| v.iter()).filter_map(|e| e.stream_ordering).max();
        let event_max_ts = room_events.values().flat_map(|v| v.iter()).map(|e| e.origin_server_ts).max();
        let state_max_ts = state_change_ts_by_room.into_iter().flat_map(|entries| entries.values().copied()).max();

        if let Some(max_stream) = event_max_stream {
            match since_token.as_ref() {
                Some(token) => max_stream.max(token.stream_id),
                None => max_stream,
            }
        } else {
            let max_ts = event_max_ts.max(state_max_ts);
            match (max_ts, since_token.as_ref()) {
                (Some(ts), Some(token)) => ts.max(token.stream_id),
                (Some(ts), None) => ts,
                (None, Some(token)) => token.stream_id,
                (None, None) => chrono::Utc::now().timestamp_millis(),
            }
        }
    }
}
