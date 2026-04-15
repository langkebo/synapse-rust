use crate::common::*;
use crate::e2ee::device_keys::DeviceKeyStorage;
use crate::services::*;
use crate::storage::{EventQueryFilter, PresenceStorage, UserRoomMembership};
use serde::{Deserialize, Serialize};
use serde_json::{json, Map, Value};
use std::collections::{HashMap, HashSet};
use std::sync::Arc;
use std::time::Instant;
use tokio::sync::RwLock;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SyncToken {
    pub stream_id: i64,
    pub room_id: Option<String>,
    pub event_type: Option<String>,
    pub to_device_stream_id: Option<i64>,
    pub device_list_stream_id: Option<i64>,
}

impl SyncToken {
    pub fn parse(token: &str) -> Option<Self> {
        if let Some(stripped) = token.strip_prefix('s') {
            if let Some((event_stream_id, rest)) = stripped.split_once('_') {
                let (to_device_stream_id, device_list_stream_id) =
                    rest.split_once('_').and_then(|(to_device, device_list)| {
                        let to_device_id = to_device.parse::<i64>().ok()?;
                        let device_list_id = device_list.parse::<i64>().ok()?;
                        Some((to_device_id, device_list_id))
                    })?;

                let stream_id = event_stream_id.parse::<i64>().ok()?;
                Some(Self {
                    stream_id,
                    room_id: None,
                    event_type: None,
                    to_device_stream_id: Some(to_device_stream_id),
                    device_list_stream_id: Some(device_list_stream_id),
                })
            } else {
                stripped.parse::<i64>().ok().map(|stream_id| Self {
                    stream_id,
                    room_id: None,
                    event_type: None,
                    to_device_stream_id: None,
                    device_list_stream_id: None,
                })
            }
        } else {
            None
        }
    }

    pub fn encode(&self) -> String {
        match (self.to_device_stream_id, self.device_list_stream_id) {
            (Some(to_device), Some(device_list)) => {
                format!("s{}_{}_{}", self.stream_id, to_device, device_list)
            }
            _ => format!("s{}", self.stream_id),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SyncFilter {
    pub limit: Option<i64>,
    pub types: Option<Vec<String>>,
    pub not_types: Option<Vec<String>>,
    pub rooms: Option<Vec<String>>,
    pub not_rooms: Option<Vec<String>>,
    pub contains_url: Option<bool>,
    pub lazy_load_members: Option<bool>,
    pub include_redundant_members: Option<bool>,
    pub senders: Option<Vec<String>>,
    pub not_senders: Option<Vec<String>>,
}

impl Default for SyncFilter {
    fn default() -> Self {
        Self {
            limit: Some(100),
            types: None,
            not_types: None,
            rooms: None,
            not_rooms: None,
            contains_url: None,
            lazy_load_members: None,
            include_redundant_members: None,
            senders: None,
            not_senders: None,
        }
    }
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, Default, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum SyncEventFormat {
    #[default]
    Client,
    Federation,
}

/// Unread counts for room sync
#[derive(Debug, Clone, Default)]
pub struct RoomSyncCounts {
    pub highlight_count: i64,
    pub notification_count: i64,
}

pub struct SyncServiceDeps {
    pub presence_storage: PresenceStorage,
    pub member_storage: RoomMemberStorage,
    pub event_storage: EventStorage,
    pub room_storage: RoomStorage,
    pub filter_storage: FilterStorage,
    pub device_storage: DeviceStorage,
    pub metrics: Arc<MetricsCollector>,
    pub performance: crate::common::config::PerformanceConfig,
}

pub struct SyncServiceRequest<'a> {
    pub user_id: &'a str,
    pub device_id: Option<&'a str>,
    pub timeout: u64,
    pub full_state: bool,
    pub set_presence: &'a str,
    pub filter_id: Option<&'a str>,
    pub since: Option<&'a str>,
}

struct FetchEventsRequest<'a> {
    user_id: &'a str,
    device_id: Option<&'a str>,
    room_ids: &'a [String],
    since_token: Option<&'a SyncToken>,
    timeout: u64,
    limit: i64,
    timeline_filter: Option<&'a SyncFilter>,
    is_incremental: bool,
}

struct BuildSyncResponseRequest<'a> {
    user_id: &'a str,
    device_id: Option<&'a str>,
    room_ids: &'a [String],
    room_sections: &'a HashMap<String, SyncRoomSection>,
    room_events: HashMap<String, Vec<RoomEvent>>,
    response_filter: Option<&'a SyncResponseFilter>,
    timeline_limit: i64,
    since_token: &'a Option<SyncToken>,
    is_incremental: bool,
}

struct BuildRoomSyncRequest<'a> {
    room_id: &'a str,
    user_id: &'a str,
    device_id: Option<&'a str>,
    events: Vec<RoomEvent>,
    since_token: Option<&'a SyncToken>,
    is_incremental: bool,
    room_filter: Option<&'a RoomFilter>,
}

struct BuildRoomSyncValueRequest<'a> {
    events: Vec<RoomEvent>,
    state_list: Vec<Value>,
    ephemeral_events: Vec<Value>,
    account_data_events: Vec<Value>,
    timeline_limit: i64,
    counts: RoomSyncCounts,
    event_fields: Option<&'a [String]>,
    event_format: SyncEventFormat,
}

struct LazyLoadMembersRequest<'a> {
    state_events: Vec<Value>,
    timeline_events: &'a [RoomEvent],
    user_id: &'a str,
    device_id: Option<&'a str>,
    room_id: &'a str,
    room_filter: Option<&'a RoomFilter>,
    changed_member_ids: Option<&'a HashSet<String>>,
    timeline_limited: bool,
    enabled: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RoomFilter {
    pub rooms: Option<Vec<String>>,
    pub not_rooms: Option<Vec<String>>,
    pub include_leave: Option<bool>,
    pub state: Option<SyncFilter>,
    pub timeline: Option<SyncFilter>,
    pub ephemeral: Option<SyncFilter>,
    pub account_data: Option<SyncFilter>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SyncResponseFilter {
    pub event_fields: Option<Vec<String>>,
    pub event_format: SyncEventFormat,
    pub room: Option<RoomFilter>,
    pub presence: Option<SyncFilter>,
}

impl Default for RoomFilter {
    fn default() -> Self {
        Self {
            rooms: None,
            not_rooms: None,
            include_leave: Some(false),
            state: Some(SyncFilter::default()),
            timeline: Some(SyncFilter {
                limit: Some(50),
                ..Default::default()
            }),
            ephemeral: Some(SyncFilter::default()),
            account_data: Some(SyncFilter::default()),
        }
    }
}

impl Default for SyncResponseFilter {
    fn default() -> Self {
        Self {
            event_fields: None,
            event_format: SyncEventFormat::Client,
            room: Some(RoomFilter::default()),
            presence: Some(SyncFilter::default()),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum SyncRoomSection {
    Join,
    Leave,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SyncRequest {
    pub since: Option<String>,
    pub filter: Option<String>,
    pub full_state: bool,
    pub set_presence: Option<String>,
    pub timeout: u64,
}

#[derive(Debug, Clone)]
pub struct SyncState {
    pub rooms: HashMap<String, RoomSyncState>,
    pub last_stream_id: i64,
}

#[derive(Debug, Clone)]
pub struct RoomSyncState {
    pub timeline_limit: i64,
    pub last_event_id: Option<String>,
    pub last_stream_id: i64,
}

#[derive(Debug, Clone)]
struct SyncPerformanceSnapshot<'a> {
    request_kind: &'a str,
    user_id: &'a str,
    total_ms: f64,
    room_count: usize,
    event_count: usize,
    is_incremental: bool,
    phases: [(&'a str, f64); 3],
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum IncrementalUpdate {
    Events,
    ToDevice,
    DeviceLists,
    Timeout,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
struct LazyLoadedMembersCacheKey {
    user_id: String,
    device_id: Option<String>,
    room_id: String,
}

impl LazyLoadedMembersCacheKey {
    fn new(user_id: &str, device_id: Option<&str>, room_id: &str) -> Self {
        Self {
            user_id: user_id.to_string(),
            device_id: device_id.map(str::to_string),
            room_id: room_id.to_string(),
        }
    }
}

pub struct SyncService {
    presence_storage: PresenceStorage,
    member_storage: RoomMemberStorage,
    event_storage: EventStorage,
    room_storage: RoomStorage,
    filter_storage: FilterStorage,
    #[allow(dead_code)]
    device_storage: DeviceStorage,
    lazy_loaded_members_cache: Arc<RwLock<HashMap<LazyLoadedMembersCacheKey, HashSet<String>>>>,
    metrics: Arc<MetricsCollector>,
    performance: crate::common::config::PerformanceConfig,
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
        metrics: Arc<MetricsCollector>,
        performance: crate::common::config::PerformanceConfig,
    ) -> Self {
        Self::from_deps(SyncServiceDeps {
            presence_storage,
            member_storage,
            event_storage,
            room_storage,
            filter_storage,
            device_storage,
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
            full_state,
            set_presence,
            filter_id,
            since,
        })
        .await
    }

    pub async fn sync_with_request(
        &self,
        request: SyncServiceRequest<'_>,
    ) -> ApiResult<serde_json::Value> {
        let SyncServiceRequest {
            user_id,
            device_id,
            timeout,
            full_state,
            set_presence,
            filter_id,
            since,
        } = request;
        let total_started = Instant::now();
        self.update_presence(user_id, set_presence).await?;
        let response_filter = self
            .resolve_sync_response_filter(user_id, filter_id)
            .await?;
        let room_filter = response_filter
            .as_ref()
            .and_then(|filter| filter.room.as_ref());
        let timeline_limit =
            Self::timeline_limit_from_room_filter(room_filter, self.sync_event_limit());

        let since_token = since.and_then(SyncToken::parse);
        let is_incremental = since_token.is_some() && !full_state;

        let rooms_started = Instant::now();
        let include_leave = room_filter
            .and_then(|filter| filter.include_leave)
            .unwrap_or(false);
        let room_memberships = self
            .member_storage
            .get_sync_rooms(user_id, include_leave)
            .await?;
        let room_sections = Self::room_sections_from_memberships(&Self::filter_sync_rooms(
            room_memberships,
            room_filter,
        ));
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
        self.log_slow_sync_request(SyncPerformanceSnapshot {
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
        full_state: bool,
        since: Option<&str>,
    ) -> ApiResult<serde_json::Value> {
        let total_started = Instant::now();
        let since_token = since.and_then(SyncToken::parse);
        let is_incremental = since_token.is_some() && !full_state;

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

        let stream_id = self.next_event_stream_id(&since_token, &room_events, None);
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
        self.log_slow_sync_request(SyncPerformanceSnapshot {
            request_kind: "room_sync",
            user_id,
            total_ms,
            room_count: 1,
            event_count,
            is_incremental,
            phases: [
                ("event_fetch_ms", event_fetch_ms),
                ("room_build_ms", room_build_ms),
                ("rooms_lookup_ms", 0.0),
            ],
        });

        Ok(serde_json::Value::Object(result))
    }

    pub async fn room_unread_counts(&self, room_id: &str, user_id: &str) -> ApiResult<(i64, i64)> {
        let (highlight_count, notification_count) =
            self.get_unread_counts(room_id, user_id).await?;
        Ok((notification_count, highlight_count))
    }

    fn count_events_by_room(room_events: &HashMap<String, Vec<RoomEvent>>) -> usize {
        room_events.values().map(Vec::len).sum()
    }

    fn observe_histogram(&self, name: &str, value: f64) {
        if let Some(histogram) = self.metrics.get_histogram(name) {
            histogram.observe(value);
        } else {
            self.metrics
                .register_histogram(name.to_string())
                .observe(value);
        }
    }

    fn increment_counter(&self, name: &str) {
        if let Some(counter) = self.metrics.get_counter(name) {
            counter.inc();
        } else {
            self.metrics.register_counter(name.to_string()).inc();
        }
    }

    fn record_sync_request_metrics(
        &self,
        request_kind: &str,
        total_ms: f64,
        room_count: usize,
        event_count: usize,
        is_incremental: bool,
    ) {
        self.increment_counter(&format!("{request_kind}_requests_total"));
        self.observe_histogram(&format!("{request_kind}_request_duration_ms"), total_ms);
        self.observe_histogram(
            &format!("{request_kind}_request_room_count"),
            room_count as f64,
        );
        self.observe_histogram(
            &format!("{request_kind}_request_event_count"),
            event_count as f64,
        );

        if is_incremental {
            self.increment_counter(&format!("{request_kind}_incremental_requests_total"));
        } else {
            self.increment_counter(&format!("{request_kind}_full_requests_total"));
        }

        if self.is_slow_request(total_ms) {
            self.increment_counter(&format!("{request_kind}_slow_requests_total"));
        }
    }

    fn is_slow_request(&self, total_ms: f64) -> bool {
        Self::is_slow_request_for(total_ms, self.performance.sync_slow_request_threshold_ms)
    }

    fn is_slow_request_for(total_ms: f64, threshold_ms: u64) -> bool {
        total_ms >= threshold_ms as f64
    }

    fn log_slow_sync_request(&self, snapshot: SyncPerformanceSnapshot<'_>) {
        if self.is_slow_request(snapshot.total_ms) {
            ::tracing::event!(
                target: "sync_performance",
                ::tracing::Level::WARN,
                request_kind = snapshot.request_kind,
                user_id = snapshot.user_id,
                total_ms = snapshot.total_ms,
                room_count = snapshot.room_count,
                event_count = snapshot.event_count,
                is_incremental = snapshot.is_incremental,
                phase_one_name = snapshot.phases[0].0,
                phase_one_ms = snapshot.phases[0].1,
                phase_two_name = snapshot.phases[1].0,
                phase_two_ms = snapshot.phases[1].1,
                phase_three_name = snapshot.phases[2].0,
                phase_three_ms = snapshot.phases[2].1,
                "Slow sync request detected"
            );
        }
    }

    fn sync_event_limit(&self) -> i64 {
        i64::from(self.performance.sync_event_limit)
    }

    fn sync_to_device_limit(&self) -> i64 {
        i64::from(self.performance.sync_to_device_limit)
    }

    fn sync_ephemeral_limit(&self) -> i64 {
        i64::from(self.performance.sync_ephemeral_limit)
    }

    fn sync_poll_interval(&self) -> std::time::Duration {
        std::time::Duration::from_millis(self.performance.sync_poll_interval_ms)
    }

    async fn update_presence(&self, user_id: &str, set_presence: &str) -> ApiResult<()> {
        if set_presence != "offline" {
            self.presence_storage
                .set_presence(user_id, set_presence, None)
                .await
                .ok();
        }
        Ok(())
    }

    async fn fetch_events(
        &self,
        request: FetchEventsRequest<'_>,
    ) -> ApiResult<HashMap<String, Vec<RoomEvent>>> {
        let FetchEventsRequest {
            user_id,
            device_id,
            room_ids,
            since_token,
            timeout,
            limit,
            timeline_filter,
            is_incremental,
        } = request;
        let event_filter = Self::event_query_filter_from_sync_filter(timeline_filter);
        let fetch_limit = if limit <= 0 {
            1
        } else {
            limit.saturating_add(1)
        };

        if is_incremental {
            let since_ts = self.event_since_ts(&since_token.map(|t| (*t).clone()));
            let events = match event_filter.as_ref() {
                Some(filter) => {
                    self.event_storage
                        .get_room_events_since_batch_filtered(
                            room_ids,
                            since_ts,
                            fetch_limit,
                            filter,
                        )
                        .await?
                }
                None => {
                    self.event_storage
                        .get_room_events_since_batch(room_ids, since_ts, fetch_limit)
                        .await?
                }
            };

            if events.values().all(|v| v.is_empty()) && timeout > 0 {
                let update = self
                    .wait_for_incremental_update(
                        user_id,
                        device_id,
                        room_ids,
                        since_ts,
                        since_token,
                        timeout,
                    )
                    .await?;

                match update {
                    IncrementalUpdate::Events => match event_filter.as_ref() {
                        Some(filter) => self
                            .event_storage
                            .get_room_events_since_batch_filtered(
                                room_ids,
                                since_ts,
                                fetch_limit,
                                filter,
                            )
                            .await
                            .map_err(Into::into),
                        None => self
                            .event_storage
                            .get_room_events_since_batch(room_ids, since_ts, fetch_limit)
                            .await
                            .map_err(Into::into),
                    },
                    IncrementalUpdate::Timeout
                    | IncrementalUpdate::ToDevice
                    | IncrementalUpdate::DeviceLists => Ok(events),
                }
            } else {
                Ok(events)
            }
        } else {
            match event_filter.as_ref() {
                Some(filter) => self
                    .event_storage
                    .get_room_events_batch_filtered(room_ids, fetch_limit, filter)
                    .await
                    .map_err(Into::into),
                None => self
                    .event_storage
                    .get_room_events_batch(room_ids, fetch_limit)
                    .await
                    .map_err(Into::into),
            }
        }
    }

    #[allow(dead_code)]
    async fn poll_for_events(
        &self,
        room_ids: &[String],
        since_ts: i64,
        limit: i64,
        timeout: u64,
    ) -> ApiResult<HashMap<String, Vec<RoomEvent>>> {
        let timeout_duration = std::time::Duration::from_millis(timeout);
        let start = std::time::Instant::now();
        let poll_interval = self.sync_poll_interval();

        loop {
            let has_events = self
                .event_storage
                .has_room_events_since(room_ids, since_ts)
                .await
                .map_err(|e| ApiError::internal(format!("Failed to poll for events: {}", e)))?;

            if has_events || start.elapsed() >= timeout_duration {
                return self
                    .event_storage
                    .get_room_events_since_batch(room_ids, since_ts, limit)
                    .await
                    .map_err(Into::into);
            }

            tokio::time::sleep(poll_interval).await;
        }
    }

    async fn wait_for_incremental_update(
        &self,
        user_id: &str,
        device_id: Option<&str>,
        room_ids: &[String],
        since_ts: i64,
        since_token: Option<&SyncToken>,
        timeout: u64,
    ) -> ApiResult<IncrementalUpdate> {
        let timeout_duration = std::time::Duration::from_millis(timeout);
        let start = std::time::Instant::now();
        let poll_interval = self.sync_poll_interval();

        let since_to_device = since_token.and_then(|t| t.to_device_stream_id).unwrap_or(0);
        let since_device_lists = since_token
            .and_then(|t| t.device_list_stream_id)
            .unwrap_or(0);

        loop {
            if start.elapsed() >= timeout_duration {
                return Ok(IncrementalUpdate::Timeout);
            }

            let (has_events, has_to_device, has_device_lists) = tokio::try_join!(
                self.has_incremental_room_updates(room_ids, since_ts),
                self.has_incremental_to_device_updates(user_id, device_id, since_to_device),
                self.has_incremental_device_list_updates(since_device_lists),
            )?;

            if has_events {
                return Ok(IncrementalUpdate::Events);
            }

            if has_to_device {
                return Ok(IncrementalUpdate::ToDevice);
            }

            if has_device_lists {
                return Ok(IncrementalUpdate::DeviceLists);
            }

            tokio::time::sleep(poll_interval).await;
        }
    }

    async fn has_incremental_room_updates(
        &self,
        room_ids: &[String],
        since_ts: i64,
    ) -> ApiResult<bool> {
        self.event_storage
            .has_room_events_since(room_ids, since_ts)
            .await
            .map_err(|e| ApiError::internal(format!("Failed to poll for events: {}", e)))
    }

    async fn has_incremental_to_device_updates(
        &self,
        user_id: &str,
        device_id: Option<&str>,
        since_stream_id: i64,
    ) -> ApiResult<bool> {
        let Some(device_id) = device_id else {
            return Ok(false);
        };

        let has_to_device = sqlx::query_scalar::<_, i32>(
            r#"
            SELECT 1
            FROM to_device_messages
            WHERE recipient_user_id = $1
              AND recipient_device_id = $2
              AND stream_id > $3
            LIMIT 1
            "#,
        )
        .bind(user_id)
        .bind(device_id)
        .bind(since_stream_id)
        .fetch_optional(&*self.event_storage.pool)
        .await
        .map_err(|e| ApiError::internal(format!("Failed to poll for to-device updates: {}", e)))?;

        Ok(has_to_device.is_some())
    }

    async fn has_incremental_device_list_updates(&self, since_stream_id: i64) -> ApiResult<bool> {
        let has_device_lists = sqlx::query_scalar::<_, i32>(
            r#"
            SELECT 1
            FROM device_lists_stream
            WHERE stream_id > $1
            LIMIT 1
            "#,
        )
        .bind(since_stream_id)
        .fetch_optional(&*self.event_storage.pool)
        .await
        .map_err(|e| {
            ApiError::internal(format!("Failed to poll for device-list updates: {}", e))
        })?;

        Ok(has_device_lists.is_some())
    }

    async fn build_sync_response(
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
        let event_format = response_filter
            .map(|filter| filter.event_format)
            .unwrap_or_default();
        let lazy_load_members = Self::room_filter_requests_lazy_members(room_filter);
        let since_ts = self.event_since_ts(since_token);
        let (changed_members_by_room, state_change_ts_by_room) = if is_incremental {
            if lazy_load_members {
                tokio::try_join!(
                    self.event_storage
                        .get_membership_state_keys_since_batch(room_ids, since_ts),
                    self.event_storage
                        .get_state_change_timestamps_batch(room_ids, since_ts),
                )
                .map_err(ApiError::from)?
            } else {
                (
                    HashMap::<String, HashSet<String>>::new(),
                    self.event_storage
                        .get_state_change_timestamps_batch(room_ids, since_ts)
                        .await
                        .map_err(ApiError::from)?,
                )
            }
        } else {
            (
                HashMap::<String, HashSet<String>>::new(),
                HashMap::<String, i64>::new(),
            )
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
                .filter(|(room_id, _)| {
                    rooms_to_include
                        .iter()
                        .any(|candidate| candidate == room_id)
                })
                .collect::<HashMap<_, _>>()
        } else {
            HashMap::new()
        };
        let state_change_ts_by_room = if is_incremental {
            state_change_ts_by_room
                .into_iter()
                .filter(|(room_id, _)| {
                    rooms_to_include
                        .iter()
                        .any(|candidate| candidate == room_id)
                })
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
            to_device_events,
            device_lists,
            to_device_stream_id,
            device_list_stream_id,
        ) = tokio::try_join!(
            self.get_state_events_for_sync_batch(
                &rooms_to_include,
                event_format,
                since_ts,
                is_incremental,
                lazy_load_members,
            ),
            self.get_room_ephemeral_events_batch(&rooms_to_include),
            self.get_room_account_data_events_batch(user_id, &rooms_to_include),
            self.get_unread_counts_batch(&rooms_to_include, user_id),
            self.get_presence_events(user_id, since_token),
            self.get_account_data_events(user_id),
            self.get_to_device_events(user_id, device_id, since_token),
            self.get_device_lists(user_id, since_token),
            async {
                match device_id {
                    Some(device_id) => {
                        self.get_current_to_device_stream_id(user_id, device_id)
                            .await
                    }
                    None => Ok(0),
                }
            },
            self.get_current_device_list_stream_id(),
        )?;
        let presence_events = Self::apply_sync_filter_to_values(
            presence_events,
            response_filter.and_then(|filter| filter.presence.as_ref()),
        );
        let presence_events = Self::apply_event_fields_to_values(presence_events, event_fields);
        let account_data_events =
            Self::apply_event_fields_to_values(account_data_events, event_fields);
        let to_device_events = Self::apply_event_fields_to_values(to_device_events, event_fields);

        let mut joined_rooms = Map::new();
        let mut left_rooms = Map::new();
        for room_id in &rooms_to_include {
            let events = room_events.get(room_id).cloned().unwrap_or_default();
            let (timeline_events, timeline_limited) =
                Self::apply_timeline_limit(events.clone(), timeline_limit);
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
            let ephemeral_events =
                Self::apply_event_fields_to_values(ephemeral_events, event_fields);
            let account_data_events = Self::apply_sync_filter_to_values(
                room_account_data_by_room
                    .get(room_id)
                    .cloned()
                    .unwrap_or_default(),
                room_filter.and_then(|filter| filter.account_data.as_ref()),
            );
            let account_data_events =
                Self::apply_event_fields_to_values(account_data_events, event_fields);
            let (highlight_count, notification_count) = unread_counts_by_room
                .get(room_id)
                .copied()
                .unwrap_or((0, 0));
            let room_sync = self.build_room_sync_value(BuildRoomSyncValueRequest {
                events,
                state_list: state_events,
                ephemeral_events,
                account_data_events,
                timeline_limit,
                counts: RoomSyncCounts {
                    highlight_count,
                    notification_count,
                },
                event_fields,
                event_format,
            });

            if room_sync.is_object() && !room_sync.as_object().is_some_and(|o| o.is_empty()) {
                match room_sections
                    .get(room_id)
                    .copied()
                    .unwrap_or(SyncRoomSection::Join)
                {
                    SyncRoomSection::Join => {
                        joined_rooms.insert(room_id.clone(), room_sync);
                    }
                    SyncRoomSection::Leave => {
                        left_rooms.insert(room_id.clone(), room_sync);
                    }
                }
            }
        }

        let stream_id =
            self.next_event_stream_id(since_token, &room_events, Some(&state_change_ts_by_room));
        let device_one_time_keys_count = self
            .build_device_one_time_keys_count(user_id, device_id)
            .await?;

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
            "device_one_time_keys_count": device_one_time_keys_count
        }))
    }

    async fn build_device_one_time_keys_count(
        &self,
        user_id: &str,
        device_id: Option<&str>,
    ) -> ApiResult<Value> {
        let Some(device_id) = device_id else {
            return Ok(json!({}));
        };

        let device_key_storage = DeviceKeyStorage::new(&self.device_storage.pool);
        let count = device_key_storage
            .get_one_time_keys_count(user_id, device_id)
            .await
            .map_err(|e| ApiError::internal(format!("Failed to load one-time key count: {}", e)))?;

        Ok(json!({
            "signed_curve25519": count
        }))
    }

    async fn build_room_sync(
        &self,
        request: BuildRoomSyncRequest<'_>,
    ) -> ApiResult<serde_json::Value> {
        let BuildRoomSyncRequest {
            room_id,
            user_id,
            device_id,
            events,
            since_token,
            is_incremental,
            room_filter,
        } = request;
        let since_ts = self.event_since_ts(&since_token.cloned());
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
                        since_ts,
                        is_incremental,
                        lazy_load_members,
                    )
                    .await?;
                Ok(state_by_room.get(room_id).cloned().unwrap_or_default())
            },
            self.get_room_ephemeral_events(room_id, user_id),
            self.get_room_account_data_events(room_id, user_id),
            self.get_unread_counts(room_id, user_id),
        )?;

        let (timeline_events, timeline_limited) =
            Self::apply_timeline_limit(events.clone(), self.sync_event_limit());
        let lazy_load_members = Self::room_filter_requests_lazy_members(room_filter);
        let state_list = Self::apply_sync_filter_to_values(
            state_list,
            room_filter.and_then(|f| f.state.as_ref()),
        );
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
        let ephemeral_events = Self::apply_sync_filter_to_values(
            ephemeral_events,
            room_filter.and_then(|f| f.ephemeral.as_ref()),
        );
        let account_data_events = Self::apply_sync_filter_to_values(
            account_data_events,
            room_filter.and_then(|f| f.account_data.as_ref()),
        );

        Ok(self.build_room_sync_value(BuildRoomSyncValueRequest {
            events,
            state_list,
            ephemeral_events,
            account_data_events,
            timeline_limit: self.sync_event_limit(),
            counts: RoomSyncCounts {
                highlight_count,
                notification_count,
            },
            event_fields: None,
            event_format: SyncEventFormat::Client,
        }))
    }

    fn event_to_json(&self, event: &RoomEvent, event_format: SyncEventFormat) -> Value {
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

        // Include state_key for state events
        if let Some(ref state_key) = event.state_key {
            obj["state_key"] = json!(state_key);
        }

        if event_format == SyncEventFormat::Federation {
            obj["depth"] = json!(event.depth);
            obj["origin"] = json!(event.origin);
        }

        obj
    }

    fn state_event_to_json(&self, event: &StateEvent, event_format: SyncEventFormat) -> Value {
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

    fn build_room_sync_value(&self, request: BuildRoomSyncValueRequest<'_>) -> Value {
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
        let (events, limited) = Self::apply_timeline_limit(events, timeline_limit);
        let event_list: Vec<Value> = events
            .iter()
            .map(|event| {
                Self::filter_event_fields(self.event_to_json(event, event_format), event_fields)
            })
            .collect();
        let prev_batch = events
            .first()
            .map(|event| format!("t{}", event.origin_server_ts))
            .unwrap_or_else(|| format!("t{}", chrono::Utc::now().timestamp_millis()));

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

    async fn resolve_sync_response_filter(
        &self,
        user_id: &str,
        filter_id: Option<&str>,
    ) -> ApiResult<Option<SyncResponseFilter>> {
        let Some(filter_id) = filter_id else {
            return Ok(None);
        };

        if filter_id.trim_start().starts_with('{') {
            let inline_filter: Value = serde_json::from_str(filter_id)
                .map_err(|e| ApiError::bad_request(format!("Invalid sync filter JSON: {}", e)))?;
            return Ok(Some(Self::sync_response_filter_from_filter_json(
                &inline_filter,
            )));
        }

        let stored = self.filter_storage.get_filter(user_id, filter_id).await?;
        Ok(stored
            .as_ref()
            .map(|filter| Self::sync_response_filter_from_filter_json(&filter.content)))
    }

    fn sync_response_filter_from_filter_json(filter: &serde_json::Value) -> SyncResponseFilter {
        SyncResponseFilter {
            event_fields: Self::json_string_array(filter.get("event_fields")),
            event_format: Self::event_format_from_json(filter.get("event_format")),
            room: Some(Self::room_filter_from_filter_json(filter)),
            presence: Self::sync_filter_from_json(filter.get("presence")),
        }
    }

    fn timeline_limit_from_room_filter(
        room_filter: Option<&RoomFilter>,
        default_limit: i64,
    ) -> i64 {
        room_filter
            .and_then(|filter| filter.timeline.as_ref())
            .and_then(|timeline| timeline.limit)
            .filter(|limit| *limit > 0)
            .map(|limit| limit.min(default_limit))
            .unwrap_or(default_limit)
    }

    fn event_query_filter_from_sync_filter(
        filter: Option<&SyncFilter>,
    ) -> Option<EventQueryFilter> {
        let filter = filter?;
        let query_filter = EventQueryFilter {
            types: filter.types.clone(),
            not_types: filter.not_types.clone(),
            senders: filter.senders.clone(),
            not_senders: filter.not_senders.clone(),
        };

        if query_filter
            .types
            .as_ref()
            .is_some_and(|values| !values.is_empty())
            || query_filter
                .not_types
                .as_ref()
                .is_some_and(|values| !values.is_empty())
            || query_filter
                .senders
                .as_ref()
                .is_some_and(|values| !values.is_empty())
            || query_filter
                .not_senders
                .as_ref()
                .is_some_and(|values| !values.is_empty())
        {
            Some(query_filter)
        } else {
            None
        }
    }

    fn room_filter_from_filter_json(filter: &serde_json::Value) -> RoomFilter {
        let room = filter.get("room");
        RoomFilter {
            rooms: Self::json_string_array(room.and_then(|value| value.get("rooms"))),
            not_rooms: Self::json_string_array(room.and_then(|value| value.get("not_rooms"))),
            include_leave: room
                .and_then(|value| value.get("include_leave"))
                .and_then(|value| value.as_bool()),
            state: Self::sync_filter_from_json(room.and_then(|value| value.get("state"))),
            timeline: Self::sync_filter_from_json(room.and_then(|value| value.get("timeline"))),
            ephemeral: Self::sync_filter_from_json(room.and_then(|value| value.get("ephemeral"))),
            account_data: Self::sync_filter_from_json(
                room.and_then(|value| value.get("account_data")),
            ),
        }
    }

    fn sync_filter_from_json(filter: Option<&serde_json::Value>) -> Option<SyncFilter> {
        let filter = filter?;
        Some(SyncFilter {
            limit: filter.get("limit").and_then(|value| value.as_i64()),
            types: Self::json_string_array(filter.get("types")),
            not_types: Self::json_string_array(filter.get("not_types")),
            rooms: Self::json_string_array(filter.get("rooms")),
            not_rooms: Self::json_string_array(filter.get("not_rooms")),
            contains_url: filter.get("contains_url").and_then(|value| value.as_bool()),
            lazy_load_members: filter
                .get("lazy_load_members")
                .and_then(|value| value.as_bool()),
            include_redundant_members: filter
                .get("include_redundant_members")
                .and_then(|value| value.as_bool()),
            senders: Self::json_string_array(filter.get("senders")),
            not_senders: Self::json_string_array(filter.get("not_senders")),
        })
    }

    fn event_format_from_json(value: Option<&Value>) -> SyncEventFormat {
        match value.and_then(|value| value.as_str()) {
            Some("federation") => SyncEventFormat::Federation,
            _ => SyncEventFormat::Client,
        }
    }

    fn filter_event_fields(event: Value, event_fields: Option<&[String]>) -> Value {
        let Some(event_fields) = event_fields else {
            return event;
        };
        let Some(source) = event.as_object() else {
            return event;
        };

        let mut filtered = Map::new();
        for field in event_fields {
            if let Some((root_key, nested_path)) = field.split_once('.') {
                if let Some(value) = source.get(root_key) {
                    Self::insert_nested_field(&mut filtered, root_key, nested_path, value);
                }
            } else if let Some(value) = source.get(field) {
                filtered.insert(field.clone(), value.clone());
            }
        }

        Value::Object(filtered)
    }

    fn insert_nested_field(
        target: &mut Map<String, Value>,
        root_key: &str,
        path: &str,
        value: &Value,
    ) {
        let Some(source_obj) = value.as_object() else {
            return;
        };
        let mut current_source = source_obj;
        let mut segments = path.split('.').peekable();
        let mut nested = Map::new();
        let mut current_target = &mut nested;

        while let Some(segment) = segments.next() {
            let Some(source_value) = current_source.get(segment) else {
                return;
            };

            if segments.peek().is_none() {
                current_target.insert(segment.to_string(), source_value.clone());
                break;
            }

            let Some(next_source) = source_value.as_object() else {
                return;
            };

            current_target = current_target
                .entry(segment.to_string())
                .or_insert_with(|| Value::Object(Map::new()))
                .as_object_mut()
                .expect("nested object inserted above");
            current_source = next_source;
        }

        if !nested.is_empty() {
            Self::merge_json_object(target, root_key.to_string(), Value::Object(nested));
        }
    }

    fn merge_json_object(target: &mut Map<String, Value>, key: String, value: Value) {
        match (target.get_mut(&key), value) {
            (Some(Value::Object(existing)), Value::Object(incoming)) => {
                for (incoming_key, incoming_value) in incoming {
                    Self::merge_json_object(existing, incoming_key, incoming_value);
                }
            }
            (_, incoming) => {
                target.insert(key, incoming);
            }
        }
    }

    fn apply_event_fields_to_values(
        events: Vec<Value>,
        event_fields: Option<&[String]>,
    ) -> Vec<Value> {
        events
            .into_iter()
            .map(|event| Self::filter_event_fields(event, event_fields))
            .collect()
    }

    fn json_string_array(value: Option<&serde_json::Value>) -> Option<Vec<String>> {
        value.and_then(|value| {
            value.as_array().map(|entries| {
                entries
                    .iter()
                    .filter_map(|entry| entry.as_str().map(ToOwned::to_owned))
                    .collect()
            })
        })
    }

    fn apply_sync_filter_to_values(
        events: Vec<serde_json::Value>,
        filter: Option<&SyncFilter>,
    ) -> Vec<serde_json::Value> {
        let Some(filter) = filter else {
            return events;
        };

        events
            .into_iter()
            .filter(|event| Self::value_matches_sync_filter(event, filter))
            .collect()
    }

    fn room_filter_requests_lazy_members(room_filter: Option<&RoomFilter>) -> bool {
        room_filter
            .and_then(|filter| {
                filter
                    .state
                    .as_ref()
                    .and_then(|state| state.lazy_load_members)
                    .or_else(|| {
                        filter
                            .timeline
                            .as_ref()
                            .and_then(|timeline| timeline.lazy_load_members)
                    })
            })
            .unwrap_or(false)
    }

    fn room_filter_requests_redundant_members(room_filter: Option<&RoomFilter>) -> bool {
        room_filter
            .and_then(|filter| {
                filter
                    .state
                    .as_ref()
                    .and_then(|state| state.include_redundant_members)
                    .or_else(|| {
                        filter
                            .timeline
                            .as_ref()
                            .and_then(|timeline| timeline.include_redundant_members)
                    })
            })
            .unwrap_or(false)
    }

    async fn get_known_lazy_loaded_members(
        &self,
        user_id: &str,
        device_id: Option<&str>,
        room_id: &str,
    ) -> HashSet<String> {
        let cache_key = LazyLoadedMembersCacheKey::new(user_id, device_id, room_id);
        {
            let cache = self.lazy_loaded_members_cache.read().await;
            if let Some(known_members) = cache.get(&cache_key) {
                return known_members.clone();
            }
        }

        let known_members = match device_id {
            Some(device_id) => self
                .device_storage
                .get_lazy_loaded_members(user_id, device_id, room_id)
                .await
                .unwrap_or_default(),
            None => HashSet::new(),
        };

        let mut cache = self.lazy_loaded_members_cache.write().await;
        cache.insert(cache_key, known_members.clone());
        known_members
    }

    async fn persist_lazy_loaded_members(
        &self,
        user_id: &str,
        device_id: Option<&str>,
        room_id: &str,
        known_members: &HashSet<String>,
    ) {
        if known_members.is_empty() {
            return;
        }

        if let Some(device_id) = device_id {
            let _ = self
                .device_storage
                .upsert_lazy_loaded_members(user_id, device_id, room_id, known_members)
                .await;
        }
    }

    async fn apply_lazy_load_members(&self, request: LazyLoadMembersRequest<'_>) -> Vec<Value> {
        let LazyLoadMembersRequest {
            state_events,
            timeline_events,
            user_id,
            device_id,
            room_id,
            room_filter,
            changed_member_ids,
            timeline_limited,
            enabled,
        } = request;
        if !enabled {
            return state_events;
        }

        let cache_key = LazyLoadedMembersCacheKey::new(user_id, device_id, room_id);
        let known_members = self
            .get_known_lazy_loaded_members(user_id, device_id, room_id)
            .await;
        let include_redundant_members = Self::room_filter_requests_redundant_members(room_filter);
        let changed_member_ids = changed_member_ids
            .filter(|_| !timeline_limited)
            .cloned()
            .unwrap_or_default();
        let (filtered_events, known_now) = Self::apply_lazy_load_members_with_cache(
            state_events,
            timeline_events,
            user_id,
            &known_members,
            include_redundant_members,
            &changed_member_ids,
        );

        if !known_now.is_empty() {
            let mut cache = self.lazy_loaded_members_cache.write().await;
            cache
                .entry(cache_key)
                .or_default()
                .extend(known_now.iter().cloned());
        }
        self.persist_lazy_loaded_members(user_id, device_id, room_id, &known_now)
            .await;

        filtered_events
    }

    fn apply_lazy_load_members_with_cache(
        state_events: Vec<Value>,
        timeline_events: &[RoomEvent],
        user_id: &str,
        known_members: &HashSet<String>,
        include_redundant_members: bool,
        changed_member_ids: &HashSet<String>,
    ) -> (Vec<Value>, HashSet<String>) {
        let mut required_members: HashSet<&str> = HashSet::from([user_id]);
        for event in timeline_events {
            required_members.insert(event.user_id.as_str());
            if event.event_type == "m.room.member" {
                if let Some(state_key) = event.state_key.as_deref() {
                    required_members.insert(state_key);
                }
            }
        }
        for user_id in changed_member_ids {
            required_members.insert(user_id.as_str());
        }

        let mut known_now: HashSet<String> = timeline_events
            .iter()
            .filter(|event| event.event_type == "m.room.member")
            .filter_map(|event| event.state_key.clone())
            .collect();
        let filtered_events = state_events
            .into_iter()
            .filter(|event| {
                if event.get("type").and_then(|value| value.as_str()) != Some("m.room.member") {
                    return true;
                }

                let Some(state_key) = event.get("state_key").and_then(|value| value.as_str())
                else {
                    return false;
                };
                if !required_members.contains(state_key) {
                    return false;
                }

                known_now.insert(state_key.to_string());
                include_redundant_members
                    || changed_member_ids.contains(state_key)
                    || !known_members.contains(state_key)
            })
            .collect();

        (filtered_events, known_now)
    }

    fn value_matches_sync_filter(event: &serde_json::Value, filter: &SyncFilter) -> bool {
        let room_id = event.get("room_id").and_then(|value| value.as_str());
        let event_type = event.get("type").and_then(|value| value.as_str());
        let sender = event.get("sender").and_then(|value| value.as_str());
        let contains_url = event
            .get("content")
            .and_then(|value| value.as_object())
            .is_some_and(|content| content.get("url").is_some());

        if let Some(rooms) = &filter.rooms {
            if !rooms.is_empty()
                && !room_id.is_some_and(|value| rooms.iter().any(|room| room == value))
            {
                return false;
            }
        }

        if let Some(not_rooms) = &filter.not_rooms {
            if room_id.is_some_and(|value| not_rooms.iter().any(|room| room == value)) {
                return false;
            }
        }

        if let Some(expected_contains_url) = filter.contains_url {
            if contains_url != expected_contains_url {
                return false;
            }
        }

        if let Some(types) = &filter.types {
            if !types.is_empty()
                && !event_type.is_some_and(|value| {
                    types
                        .iter()
                        .any(|pattern| Self::matches_wildcard(value, pattern))
                })
            {
                return false;
            }
        }

        if let Some(not_types) = &filter.not_types {
            if event_type.is_some_and(|value| {
                not_types
                    .iter()
                    .any(|pattern| Self::matches_wildcard(value, pattern))
            }) {
                return false;
            }
        }

        if let Some(senders) = &filter.senders {
            if !senders.is_empty()
                && !sender.is_some_and(|value| senders.iter().any(|s| s == value))
            {
                return false;
            }
        }

        if let Some(not_senders) = &filter.not_senders {
            if sender.is_some_and(|value| not_senders.iter().any(|s| s == value)) {
                return false;
            }
        }

        true
    }

    fn matches_wildcard(actual: &str, pattern: &str) -> bool {
        if let Some(prefix) = pattern.strip_suffix('*') {
            actual.starts_with(prefix)
        } else {
            actual == pattern
        }
    }

    fn apply_timeline_limit(events: Vec<RoomEvent>, timeline_limit: i64) -> (Vec<RoomEvent>, bool) {
        if timeline_limit <= 0 {
            return (Vec::new(), !events.is_empty());
        }

        let limited = events.len() as i64 > timeline_limit;
        let mut events: Vec<RoomEvent> = events.into_iter().take(timeline_limit as usize).collect();
        events.reverse();
        (events, limited)
    }

    async fn get_room_state_events_batch(
        &self,
        room_ids: &[String],
        event_format: SyncEventFormat,
    ) -> ApiResult<HashMap<String, Vec<Value>>> {
        let state_events = self
            .event_storage
            .get_state_events_batch(room_ids)
            .await
            .map_err(|e| ApiError::internal(format!("Failed to get room state events: {}", e)))?;

        Ok(state_events
            .into_iter()
            .map(|(room_id, events)| {
                let values = events
                    .iter()
                    .map(|event| self.state_event_to_json(event, event_format))
                    .collect();
                (room_id, values)
            })
            .collect())
    }

    async fn get_state_events_for_sync_batch(
        &self,
        room_ids: &[String],
        event_format: SyncEventFormat,
        since_ts: i64,
        is_incremental: bool,
        lazy_load_members: bool,
    ) -> ApiResult<HashMap<String, Vec<Value>>> {
        if room_ids.is_empty() {
            return Ok(HashMap::new());
        }

        if !is_incremental {
            return self
                .get_room_state_events_batch(room_ids, event_format)
                .await;
        }

        let delta_state_by_room = self
            .event_storage
            .get_state_events_since_batch(room_ids, since_ts)
            .await
            .map_err(|e| ApiError::internal(format!("Failed to get room state events: {}", e)))?;

        if !lazy_load_members {
            return Ok(delta_state_by_room
                .into_iter()
                .map(|(room_id, events)| {
                    let values = events
                        .iter()
                        .map(|event| self.state_event_to_json(event, event_format))
                        .collect();
                    (room_id, values)
                })
                .collect());
        }

        let current_member_state_by_room = self
            .event_storage
            .get_state_events_by_type_batch(room_ids, "m.room.member")
            .await
            .map_err(|e| ApiError::internal(format!("Failed to get room state events: {}", e)))?;

        let mut result = HashMap::new();
        for room_id in room_ids {
            let mut values = Vec::new();

            for event in delta_state_by_room.get(room_id).into_iter().flatten() {
                if event.event_type.as_deref() == Some("m.room.member") {
                    continue;
                }
                values.push(self.state_event_to_json(event, event_format));
            }

            for event in current_member_state_by_room
                .get(room_id)
                .into_iter()
                .flatten()
            {
                values.push(self.state_event_to_json(event, event_format));
            }

            result.insert(room_id.clone(), values);
        }

        Ok(result)
    }

    async fn get_presence_events(
        &self,
        user_id: &str,
        _since: &Option<SyncToken>,
    ) -> ApiResult<Vec<serde_json::Value>> {
        Ok(vec![json!({
            "content": {
                "avatar_url": null,
                "displayname": null,
                "last_active_ago": 0,
                "presence": "online"
            },
            "sender": user_id,
            "type": "m.presence"
        })])
    }

    async fn get_account_data_events(&self, user_id: &str) -> ApiResult<Vec<serde_json::Value>> {
        // Load user account_data from DB
        let rows = sqlx::query("SELECT data_type, content FROM account_data WHERE user_id = $1")
            .bind(user_id)
            .fetch_all(&*self.event_storage.pool)
            .await
            .map_err(|e| ApiError::internal(format!("Failed to get account data: {}", e)))?;

        let mut events: Vec<serde_json::Value> = rows
            .iter()
            .map(|row| {
                use sqlx::Row;
                let data_type: String = row.get("data_type");
                let content: serde_json::Value = row.get("content");
                json!({
                    "type": data_type,
                    "content": content
                })
            })
            .collect();

        // If no push rules exist yet, inject the Matrix spec default push rules
        let has_push_rules = events.iter().any(|e| e["type"] == "m.push_rules");
        if !has_push_rules {
            events.push(json!({
                "type": "m.push_rules",
                "content": {
                    "global": {
                        "content": [
                            {
                                "actions": ["notify", {"set_tweak": "highlight", "value": false}],
                                "conditions": [{"kind": "contains_display_name"}],
                                "default": true,
                                "enabled": true,
                                "rule_id": ".m.rule.contains_display_name"
                            }
                        ],
                        "override": [
                            {
                                "actions": ["dont_notify"],
                                "conditions": [{"kind": "event_match", "key": "content.msgtype", "pattern": "m.notice"}],
                                "default": true,
                                "enabled": true,
                                "rule_id": ".m.rule.suppress_notices"
                            }
                        ],
                        "room": [],
                        "sender": [],
                        "underride": [
                            {
                                "actions": ["notify", {"set_tweak": "sound", "value": "default"}],
                                "conditions": [{"kind": "event_match", "key": "type", "pattern": "m.room.message"}],
                                "default": true,
                                "enabled": true,
                                "rule_id": ".m.rule.message"
                            }
                        ]
                    }
                }
            }));
        }

        Ok(events)
    }

    async fn get_to_device_events(
        &self,
        user_id: &str,
        device_id: Option<&str>,
        since: &Option<SyncToken>,
    ) -> ApiResult<Vec<serde_json::Value>> {
        let Some(device_id) = device_id else {
            return Ok(Vec::new());
        };
        let since_stream_id = self.to_device_since_stream_id(since);

        let rows = sqlx::query(
            r#"
            SELECT sender_user_id, sender_device_id, event_type, content, message_id
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
        .bind(self.sync_to_device_limit())
        .fetch_all(&*self.event_storage.pool)
        .await
        .map_err(|e| ApiError::internal(format!("Failed to get to-device events: {}", e)))?;

        let events: Vec<serde_json::Value> = rows
            .iter()
            .map(|row| {
                use sqlx::Row;
                let sender: String = row.get("sender_user_id");
                let _sender_device: String = row.get("sender_device_id");
                let event_type: String = row.get("event_type");
                let content: serde_json::Value = row.get("content");
                let message_id: Option<String> = row.get("message_id");

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

        Ok(events)
    }

    async fn get_device_lists(
        &self,
        user_id: &str,
        since: &Option<SyncToken>,
    ) -> ApiResult<serde_json::Value> {
        let since_stream_id = self.device_list_since_stream_id(since);

        // Get users whose devices have changed
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
        .await
        .map_err(|e| ApiError::internal(format!("Failed to get device lists: {}", e)))?;

        let changed: Vec<String> = changed_rows
            .iter()
            .map(|row| {
                use sqlx::Row;
                row.get("user_id")
            })
            .collect();

        // Get users who left (no longer share rooms)
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
        .await
        .map_err(|e| ApiError::internal(format!("Failed to get left device lists: {}", e)))?;

        let left: Vec<String> = left_rows
            .iter()
            .map(|row| {
                use sqlx::Row;
                row.get("user_id")
            })
            .collect();

        Ok(json!({
            "changed": changed,
            "left": left
        }))
    }

    async fn get_current_to_device_stream_id(
        &self,
        user_id: &str,
        device_id: &str,
    ) -> ApiResult<i64> {
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
        .map_err(|e| ApiError::internal(format!("Failed to get to-device stream ID: {}", e)))
    }

    async fn get_current_device_list_stream_id(&self) -> ApiResult<i64> {
        sqlx::query_scalar::<_, i64>(
            r#"
            SELECT COALESCE(MAX(stream_id), 0)
            FROM device_lists_stream
            "#,
        )
        .fetch_one(&*self.event_storage.pool)
        .await
        .map_err(|e| ApiError::internal(format!("Failed to get device-list stream ID: {}", e)))
    }

    fn to_device_since_stream_id(&self, since: &Option<SyncToken>) -> i64 {
        since
            .as_ref()
            .and_then(|token| token.to_device_stream_id)
            .unwrap_or(0)
    }

    fn device_list_since_stream_id(&self, since: &Option<SyncToken>) -> i64 {
        since
            .as_ref()
            .and_then(|token| token.device_list_stream_id)
            .unwrap_or(0)
    }

    async fn get_room_ephemeral_events(
        &self,
        room_id: &str,
        _user_id: &str,
    ) -> ApiResult<Vec<serde_json::Value>> {
        let now = chrono::Utc::now().timestamp_millis();
        let limit = self.sync_ephemeral_limit();

        let rows = sqlx::query(
            r#"
            SELECT event_type, user_id, content
            FROM room_ephemeral
            WHERE room_id = $1
            AND (expires_at IS NULL OR expires_at > $2)
            ORDER BY stream_id DESC
            LIMIT $3
            "#,
        )
        .bind(room_id)
        .bind(now)
        .bind(limit)
        .fetch_all(&*self.event_storage.pool)
        .await
        .map_err(|e| ApiError::internal(format!("Failed to get ephemeral events: {}", e)))?;

        let events: Vec<serde_json::Value> = rows
            .iter()
            .map(|row| {
                use sqlx::Row;
                let event_type: String = row.get("event_type");
                let user_id: String = row.get("user_id");
                let content: serde_json::Value = row.get("content");

                json!({
                    "type": event_type,
                    "sender": user_id,
                    "content": content
                })
            })
            .collect();

        Ok(events)
    }

    async fn get_room_ephemeral_events_batch(
        &self,
        room_ids: &[String],
    ) -> ApiResult<HashMap<String, Vec<serde_json::Value>>> {
        let limit = self.sync_ephemeral_limit();
        let mut result: HashMap<String, Vec<serde_json::Value>> = room_ids
            .iter()
            .cloned()
            .map(|room_id| (room_id, Vec::new()))
            .collect();
        if room_ids.is_empty() {
            return Ok(result);
        }

        let now = chrono::Utc::now().timestamp_millis();
        let rows = sqlx::query(
            r#"
            SELECT room_id, event_type, user_id, content, stream_id
            FROM (
                SELECT
                    room_id,
                    event_type,
                    user_id,
                    content,
                    stream_id,
                    ROW_NUMBER() OVER (
                        PARTITION BY room_id
                        ORDER BY stream_id DESC
                    ) AS rn
                FROM room_ephemeral
                WHERE room_id = ANY($1)
                  AND (expires_at IS NULL OR expires_at > $2)
            ) ranked
            WHERE rn <= $3
            ORDER BY room_id, stream_id DESC
            "#,
        )
        .bind(room_ids)
        .bind(now)
        .bind(limit)
        .fetch_all(&*self.event_storage.pool)
        .await
        .map_err(|e| ApiError::internal(format!("Failed to get room ephemeral events: {}", e)))?;

        for row in rows {
            use sqlx::Row;
            let room_id: String = row.get("room_id");
            if let Some(events) = result.get_mut(&room_id) {
                if events.len() >= limit as usize {
                    continue;
                }
                let event_type: String = row.get("event_type");
                let user_id: String = row.get("user_id");
                let content: serde_json::Value = row.get("content");
                events.push(json!({
                    "type": event_type,
                    "sender": user_id,
                    "content": content
                }));
            }
        }

        Ok(result)
    }

    async fn get_room_account_data_events(
        &self,
        room_id: &str,
        user_id: &str,
    ) -> ApiResult<Vec<serde_json::Value>> {
        let rows = sqlx::query(
            "SELECT data_type, data as content FROM room_account_data WHERE user_id = $1 AND room_id = $2",
        )
        .bind(user_id)
        .bind(room_id)
        .fetch_all(&*self.event_storage.pool)
        .await
        .map_err(|e| ApiError::internal(format!("Failed to get room account data: {}", e)))?;

        Ok(rows
            .iter()
            .map(|row| {
                use sqlx::Row;
                let data_type: String = row.get("data_type");
                let content: serde_json::Value = row.get("content");
                json!({
                    "type": data_type,
                    "content": content
                })
            })
            .collect())
    }

    async fn get_room_account_data_events_batch(
        &self,
        user_id: &str,
        room_ids: &[String],
    ) -> ApiResult<HashMap<String, Vec<serde_json::Value>>> {
        let mut result: HashMap<String, Vec<serde_json::Value>> = room_ids
            .iter()
            .cloned()
            .map(|room_id| (room_id, Vec::new()))
            .collect();
        if room_ids.is_empty() {
            return Ok(result);
        }

        let rows = sqlx::query(
            r#"
            SELECT room_id, data_type, data as content
            FROM room_account_data
            WHERE user_id = $1 AND room_id = ANY($2)
            "#,
        )
        .bind(user_id)
        .bind(room_ids)
        .fetch_all(&*self.event_storage.pool)
        .await
        .map_err(|e| ApiError::internal(format!("Failed to get room account data: {}", e)))?;

        for row in rows {
            use sqlx::Row;
            let room_id: String = row.get("room_id");
            if let Some(events) = result.get_mut(&room_id) {
                let data_type: String = row.get("data_type");
                let content: serde_json::Value = row.get("content");
                events.push(json!({
                    "type": data_type,
                    "content": content
                }));
            }
        }

        Ok(result)
    }

    async fn get_unread_counts(&self, room_id: &str, user_id: &str) -> ApiResult<(i64, i64)> {
        // Get the user's last read event timestamp from read_markers
        let last_read_ts: Option<i64> = sqlx::query_scalar(
            r#"
            SELECT e.origin_server_ts
            FROM read_markers rm
            JOIN events e ON e.event_id = rm.event_id
            WHERE rm.room_id = $1 AND rm.user_id = $2
            LIMIT 1
            "#,
        )
        .bind(room_id)
        .bind(user_id)
        .fetch_optional(&*self.event_storage.pool)
        .await
        .map_err(|e| ApiError::internal(format!("Failed to get read marker: {}", e)))?
        .flatten();

        let since_ts = last_read_ts.unwrap_or(0);

        // Count total unread events (notifications)
        let notification_count: i64 = sqlx::query_scalar(
            r#"
            SELECT COUNT(*)
            FROM events
            WHERE room_id = $1
              AND user_id != $2
              AND origin_server_ts > $3
              AND state_key IS NULL
            "#,
        )
        .bind(room_id)
        .bind(user_id)
        .bind(since_ts)
        .fetch_one(&*self.event_storage.pool)
        .await
        .unwrap_or(0);

        // Count highlight events (mentions of the user)
        let highlight_count: i64 = sqlx::query_scalar(
            r#"
            SELECT COUNT(*)
            FROM events
            WHERE room_id = $1
              AND user_id != $2
              AND origin_server_ts > $3
              AND state_key IS NULL
              AND (
                content::text LIKE $4
                OR content::text LIKE '%@room%'
              )
            "#,
        )
        .bind(room_id)
        .bind(user_id)
        .bind(since_ts)
        .bind(format!("%{}%", user_id))
        .fetch_one(&*self.event_storage.pool)
        .await
        .unwrap_or(0);

        Ok((highlight_count, notification_count))
    }

    async fn get_unread_counts_batch(
        &self,
        room_ids: &[String],
        user_id: &str,
    ) -> ApiResult<HashMap<String, (i64, i64)>> {
        let mut result: HashMap<String, (i64, i64)> = room_ids
            .iter()
            .cloned()
            .map(|room_id| (room_id, (0, 0)))
            .collect();
        if room_ids.is_empty() {
            return Ok(result);
        }

        let mention_pattern = format!("%{}%", user_id);
        let rows = sqlx::query(
            r#"
            WITH target_rooms AS (
                SELECT UNNEST($2::text[]) AS room_id
            ),
            last_reads AS (
                SELECT tr.room_id, COALESCE(MAX(e.origin_server_ts), 0) AS last_read_ts
                FROM target_rooms tr
                LEFT JOIN read_markers rm
                  ON rm.room_id = tr.room_id
                 AND rm.user_id = $1
                LEFT JOIN events e
                  ON e.event_id = rm.event_id
                GROUP BY tr.room_id
            )
            SELECT
                tr.room_id,
                COALESCE(COUNT(ev.event_id) FILTER (
                    WHERE ev.user_id != $1
                      AND ev.state_key IS NULL
                      AND ev.origin_server_ts > lr.last_read_ts
                ), 0) AS notification_count,
                COALESCE(COUNT(ev.event_id) FILTER (
                    WHERE ev.user_id != $1
                      AND ev.state_key IS NULL
                      AND ev.origin_server_ts > lr.last_read_ts
                      AND (
                        ev.content::text LIKE $3
                        OR ev.content::text LIKE '%@room%'
                      )
                ), 0) AS highlight_count
            FROM target_rooms tr
            LEFT JOIN last_reads lr
              ON lr.room_id = tr.room_id
            LEFT JOIN events ev
              ON ev.room_id = tr.room_id
             AND ev.user_id != $1
             AND ev.state_key IS NULL
             AND ev.origin_server_ts > lr.last_read_ts
            GROUP BY tr.room_id, lr.last_read_ts
            "#,
        )
        .bind(user_id)
        .bind(room_ids)
        .bind(mention_pattern)
        .fetch_all(&*self.event_storage.pool)
        .await
        .map_err(|e| ApiError::internal(format!("Failed to get unread counts: {}", e)))?;

        for row in rows {
            use sqlx::Row;
            let room_id: String = row.get("room_id");
            let notification_count: i64 = row.get("notification_count");
            let highlight_count: i64 = row.get("highlight_count");
            result.insert(room_id, (highlight_count, notification_count));
        }

        Ok(result)
    }

    fn rooms_to_include(
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
                room_events
                    .get(*room_id)
                    .map(|events| !events.is_empty())
                    .unwrap_or(false)
                    || changed_members_by_room
                        .get(*room_id)
                        .map(|members| !members.is_empty())
                        .unwrap_or(false)
                    || state_change_ts_by_room
                        .get(*room_id)
                        .map(|&ts| ts > 0)
                        .unwrap_or(false)
            })
            .cloned()
            .collect()
    }

    fn filter_sync_rooms(
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

    fn room_sections_from_memberships(
        memberships: &[UserRoomMembership],
    ) -> HashMap<String, SyncRoomSection> {
        memberships
            .iter()
            .map(|membership| {
                let section = if membership.membership == "leave" {
                    SyncRoomSection::Leave
                } else {
                    SyncRoomSection::Join
                };
                (membership.room_id.clone(), section)
            })
            .collect()
    }

    fn event_since_ts(&self, since_token: &Option<SyncToken>) -> i64 {
        match since_token {
            Some(token) if token.stream_id >= Self::TIMESTAMP_TOKEN_MIN => token.stream_id,
            Some(token)
                if token.to_device_stream_id.is_some() || token.device_list_stream_id.is_some() =>
            {
                token.stream_id.max(0)
            }
            Some(_) => 0,
            None => 0,
        }
    }

    fn next_event_stream_id(
        &self,
        since_token: &Option<SyncToken>,
        room_events: &HashMap<String, Vec<RoomEvent>>,
        state_change_ts_by_room: Option<&HashMap<String, i64>>,
    ) -> i64 {
        let event_max_ts = room_events
            .values()
            .flat_map(|v| v.iter())
            .map(|e| e.origin_server_ts)
            .max();
        let state_max_ts = state_change_ts_by_room
            .into_iter()
            .flat_map(|entries| entries.values().copied())
            .max();
        let max_ts = event_max_ts.max(state_max_ts);

        match (max_ts, since_token.as_ref()) {
            (Some(ts), Some(token)) => ts.max(token.stream_id),
            (Some(ts), None) => ts,
            (None, Some(token)) => token.stream_id,
            (None, None) => chrono::Utc::now().timestamp_millis(),
        }
    }

    pub async fn get_room_messages(
        &self,
        room_id: &str,
        user_id: &str,
        from: &str,
        limit: i64,
        _dir: &str,
    ) -> ApiResult<serde_json::Value> {
        if !self
            .member_storage
            .is_member(room_id, user_id)
            .await
            .map_err(|e| ApiError::internal(format!("Failed to check membership: {}", e)))?
        {
            return Err(ApiError::forbidden(
                "You are not a member of this room".to_string(),
            ));
        }

        let events = self
            .event_storage
            .get_room_events(room_id, limit)
            .await
            .map_err(|e| ApiError::internal(format!("Failed to get messages: {}", e)))?;

        let event_list: Vec<serde_json::Value> = events
            .iter()
            .map(|e| self.event_to_json(e, SyncEventFormat::Client))
            .collect();

        let end_token = events
            .last()
            .map(|e| format!("t{}", e.origin_server_ts))
            .unwrap_or_else(|| format!("t{}", chrono::Utc::now().timestamp_millis()));

        Ok(json!({
            "chunk": event_list,
            "start": from,
            "end": end_token
        }))
    }

    pub async fn get_public_rooms(
        &self,
        limit: i64,
        _since: Option<&str>,
    ) -> ApiResult<serde_json::Value> {
        let rooms = self
            .room_storage
            .get_public_rooms(limit)
            .await
            .map_err(|e| ApiError::internal(format!("Failed to get public rooms: {}", e)))?;

        let room_list: Vec<serde_json::Value> = rooms
            .iter()
            .map(|r| {
                json!({
                    "room_id": r.room_id,
                    "name": r.name,
                    "topic": r.topic,
                    "canonical_alias": r.canonical_alias,
                    "is_public": r.is_public,
                    "join_rule": r.join_rule
                })
            })
            .collect();

        let next_batch = if room_list.len() as i64 >= limit {
            Some(format!("p{}", chrono::Utc::now().timestamp_millis()))
        } else {
            None
        };

        let mut response = json!({
            "chunk": room_list,
            "total_room_count_estimate": room_list.len() as i64
        });

        if let Some(batch) = next_batch {
            response["next_batch"] = json!(batch);
        }

        Ok(response)
    }

    pub async fn get_filter(
        &self,
        _user_id: &str,
        _filter_id: &str,
    ) -> ApiResult<serde_json::Value> {
        Ok(json!({}))
    }

    pub async fn set_filter(
        &self,
        _user_id: &str,
        _filter: &serde_json::Value,
    ) -> ApiResult<String> {
        Ok(format!("filter_{}", chrono::Utc::now().timestamp_millis()))
    }

    pub async fn get_events(
        &self,
        user_id: &str,
        from: &str,
        _timeout: u64,
    ) -> ApiResult<serde_json::Value> {
        let room_ids = self
            .member_storage
            .get_joined_rooms(user_id)
            .await
            .map_err(|e| ApiError::internal(format!("Failed to get rooms: {}", e)))?;

        let since_ts: i64 = from
            .trim_start_matches('s')
            .trim_start_matches('t')
            .parse()
            .map_err(|_| ApiError::invalid_input("Invalid 'from' token".to_string()))?;

        let limit = 100i64;
        let events = self
            .event_storage
            .get_room_events_since_batch(&room_ids, since_ts, limit)
            .await
            .map_err(|e| ApiError::internal(format!("Failed to get events: {}", e)))?;

        let mut chunk = vec![];
        for room_events in events.values() {
            for event in room_events {
                chunk.push(self.event_to_json(event, SyncEventFormat::Client));
            }
        }

        let end_token = format!("s{}", chrono::Utc::now().timestamp_millis());

        Ok(json!({
            "start": from,
            "end": end_token,
            "chunk": chunk
        }))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_sync_token_parse() {
        let token = SyncToken::parse("s1234567890");
        assert!(token.is_some());
        let token = token.unwrap();
        assert_eq!(token.stream_id, 1234567890);
    }

    #[test]
    fn test_sync_token_encode() {
        let token = SyncToken {
            stream_id: 1234567890,
            room_id: None,
            event_type: None,
            to_device_stream_id: None,
            device_list_stream_id: None,
        };
        assert_eq!(token.encode(), "s1234567890");
    }

    #[test]
    fn test_sync_token_roundtrip() {
        let original = SyncToken {
            stream_id: 9876543210,
            room_id: None,
            event_type: None,
            to_device_stream_id: None,
            device_list_stream_id: None,
        };
        let encoded = original.encode();
        let parsed = SyncToken::parse(&encoded).unwrap();
        assert_eq!(original.stream_id, parsed.stream_id);
    }

    #[test]
    fn test_sync_token_multistream_roundtrip() {
        let original = SyncToken {
            stream_id: 1_777_000_000_000,
            room_id: None,
            event_type: None,
            to_device_stream_id: Some(4321),
            device_list_stream_id: Some(9876),
        };
        let encoded = original.encode();
        assert_eq!(encoded, "s1777000000000_4321_9876");
        let parsed = SyncToken::parse(&encoded).unwrap();
        assert_eq!(parsed.stream_id, original.stream_id);
        assert_eq!(parsed.to_device_stream_id, original.to_device_stream_id);
        assert_eq!(parsed.device_list_stream_id, original.device_list_stream_id);
    }

    #[test]
    fn test_sync_filter_default() {
        let filter = SyncFilter::default();
        assert_eq!(filter.limit, Some(100));
        assert!(filter.types.is_none());
        assert!(filter.rooms.is_none());
    }

    #[test]
    fn test_room_filter_default() {
        let filter = RoomFilter::default();
        assert_eq!(filter.include_leave, Some(false));
        assert!(filter.state.is_some());
        assert!(filter.timeline.is_some());
        assert_eq!(filter.timeline.unwrap().limit, Some(50));
    }

    #[test]
    fn test_sync_response_format() {
        let response = json!({
            "next_batch": "s1234567890",
            "rooms": {
                "join": {},
                "invite": {},
                "leave": {}
            },
            "presence": json!({
                "events": []
            }),
            "account_data": json!({
                "events": []
            }),
            "to_device": json!({
                "events": []
            }),
            "device_lists": {
                "changed": [],
                "left": []
            }
        });

        assert!(response.get("next_batch").is_some());
        assert!(response["rooms"]["join"].is_object());
        assert!(response["presence"]["events"].is_array());
        assert!(response["device_lists"]["changed"].is_array());
    }

    #[test]
    fn test_room_timeline_format() {
        let timeline = json!({
            "events": [],
            "limited": true,
            "prev_batch": "t1234567890"
        });

        assert!(timeline["events"].is_array());
        assert!(timeline["limited"].is_boolean());
        assert_eq!(timeline["prev_batch"], "t1234567890");
    }

    #[test]
    fn test_room_state_format() {
        let state = json!({
            "events": []
        });
        assert!(state["events"].is_array());
    }

    #[test]
    fn test_presence_format() {
        let presence = json!({
            "events": []
        });
        assert!(presence["events"].is_array());
    }

    #[test]
    fn test_account_data_format() {
        let account_data = json!({
            "events": []
        });
        assert!(account_data["events"].is_array());
    }

    #[test]
    fn test_to_device_format() {
        let to_device = json!({
            "events": []
        });
        assert!(to_device["events"].is_array());
    }

    #[test]
    fn test_device_lists_format() {
        let device_lists = json!({
            "changed": ["@user1:example.com"],
            "left": ["@user2:example.com"]
        });

        assert!(device_lists["changed"].is_array());
        assert!(device_lists["left"].is_array());
    }

    #[test]
    fn test_unread_notifications_format() {
        let notifications = json!({
            "highlight_count": 0,
            "notification_count": 0
        });

        assert_eq!(notifications["highlight_count"], 0);
        assert_eq!(notifications["notification_count"], 0);
    }

    #[test]
    fn test_ephemeral_format() {
        let ephemeral = json!({
            "events": []
        });
        assert!(ephemeral["events"].is_array());
    }

    #[test]
    fn test_room_messages_response_format() {
        let response = json!({
            "chunk": [],
            "start": "t1234567890",
            "end": "t1234567899"
        });

        assert!(response.get("chunk").is_some());
        assert!(response.get("start").is_some());
        assert!(response.get("end").is_some());
    }

    #[test]
    fn test_public_rooms_response_format() {
        let response = json!({
            "chunk": [],
            "total_room_count_estimate": 0,
            "next_batch": "p1234567890"
        });

        assert!(response.get("chunk").is_some());
        assert!(response.get("total_room_count_estimate").is_some());
        assert!(response.get("next_batch").is_some());
    }

    #[test]
    fn test_count_events_by_room() {
        let room_events = HashMap::from([
            (
                "!a:example.com".to_string(),
                vec![sample_room_event("1"), sample_room_event("2")],
            ),
            ("!b:example.com".to_string(), vec![sample_room_event("3")]),
        ]);

        assert_eq!(SyncService::count_events_by_room(&room_events), 3);
    }

    #[test]
    fn test_timeline_limit_from_room_filter() {
        let filter = json!({
            "room": {
                "timeline": {
                    "limit": 8
                }
            }
        });
        let room_filter = SyncService::room_filter_from_filter_json(&filter);

        assert_eq!(
            SyncService::timeline_limit_from_room_filter(Some(&room_filter), 50),
            8
        );
    }

    #[test]
    fn test_event_query_filter_from_sync_filter_ignores_limit_only_filter() {
        let filter = SyncFilter {
            limit: Some(3),
            types: None,
            not_types: None,
            rooms: None,
            not_rooms: None,
            contains_url: None,
            lazy_load_members: None,
            include_redundant_members: None,
            senders: None,
            not_senders: None,
        };

        assert!(SyncService::event_query_filter_from_sync_filter(Some(&filter)).is_none());
    }

    #[test]
    fn test_event_query_filter_from_sync_filter_preserves_matchers() {
        let filter = SyncFilter {
            limit: Some(5),
            types: Some(vec!["m.room.message".to_string()]),
            not_types: Some(vec!["m.room.redaction".to_string()]),
            rooms: None,
            not_rooms: None,
            contains_url: None,
            lazy_load_members: None,
            include_redundant_members: None,
            senders: Some(vec!["@alice:localhost".to_string()]),
            not_senders: Some(vec!["@mallory:localhost".to_string()]),
        };

        let query_filter = SyncService::event_query_filter_from_sync_filter(Some(&filter))
            .expect("timeline matcher filter should be pushed to query layer");

        assert_eq!(query_filter.types, Some(vec!["m.room.message".to_string()]));
        assert_eq!(
            query_filter.not_types,
            Some(vec!["m.room.redaction".to_string()])
        );
        assert_eq!(
            query_filter.senders,
            Some(vec!["@alice:localhost".to_string()])
        );
        assert_eq!(
            query_filter.not_senders,
            Some(vec!["@mallory:localhost".to_string()])
        );
    }

    #[test]
    fn test_timeline_limit_from_room_filter_ignores_missing_limit() {
        let filter = json!({
            "room": {
                "timeline": {}
            }
        });
        let room_filter = SyncService::room_filter_from_filter_json(&filter);

        assert_eq!(
            SyncService::timeline_limit_from_room_filter(Some(&room_filter), 50),
            50
        );
    }

    #[test]
    fn test_sync_filter_from_json_parses_matchers() {
        let filter = json!({
            "limit": 12,
            "types": ["m.room.member"],
            "not_types": ["m.room.redaction"],
            "rooms": ["!room:localhost"],
            "not_rooms": ["!blocked:localhost"],
            "contains_url": true,
            "lazy_load_members": true,
            "include_redundant_members": true,
            "senders": ["@alice:localhost"],
            "not_senders": ["@mallory:localhost"]
        });

        let parsed = SyncService::sync_filter_from_json(Some(&filter)).unwrap();

        assert_eq!(parsed.limit, Some(12));
        assert_eq!(parsed.types, Some(vec!["m.room.member".to_string()]));
        assert_eq!(parsed.not_types, Some(vec!["m.room.redaction".to_string()]));
        assert_eq!(parsed.rooms, Some(vec!["!room:localhost".to_string()]));
        assert_eq!(
            parsed.not_rooms,
            Some(vec!["!blocked:localhost".to_string()])
        );
        assert_eq!(parsed.contains_url, Some(true));
        assert_eq!(parsed.lazy_load_members, Some(true));
        assert_eq!(parsed.include_redundant_members, Some(true));
        assert_eq!(parsed.senders, Some(vec!["@alice:localhost".to_string()]));
        assert_eq!(
            parsed.not_senders,
            Some(vec!["@mallory:localhost".to_string()])
        );
    }

    #[test]
    fn test_room_filter_from_filter_json_parses_sections() {
        let filter = json!({
            "room": {
                "rooms": ["!allowed:localhost"],
                "not_rooms": ["!blocked:localhost"],
                "include_leave": true,
                "state": {
                    "types": ["m.room.name"]
                },
                "timeline": {
                    "limit": 5
                },
                "ephemeral": {
                    "senders": ["@alice:localhost"]
                },
                "account_data": {
                    "not_types": ["m.tag"]
                }
            }
        });

        let parsed = SyncService::room_filter_from_filter_json(&filter);

        assert_eq!(parsed.rooms, Some(vec!["!allowed:localhost".to_string()]));
        assert_eq!(
            parsed.not_rooms,
            Some(vec!["!blocked:localhost".to_string()])
        );
        assert_eq!(parsed.include_leave, Some(true));
        assert_eq!(
            parsed.state.and_then(|state| state.types),
            Some(vec!["m.room.name".to_string()])
        );
        assert_eq!(parsed.timeline.and_then(|timeline| timeline.limit), Some(5));
        assert_eq!(
            parsed.ephemeral.and_then(|ephemeral| ephemeral.senders),
            Some(vec!["@alice:localhost".to_string()])
        );
        assert_eq!(
            parsed
                .account_data
                .and_then(|account_data| account_data.not_types),
            Some(vec!["m.tag".to_string()])
        );
    }

    #[test]
    fn test_sync_response_filter_from_filter_json_parses_presence() {
        let filter = json!({
            "event_fields": ["type", "content.body", "unsigned.age"],
            "event_format": "federation",
            "presence": {
                "types": ["m.presence"],
                "not_senders": ["@mallory:localhost"]
            },
            "room": {
                "timeline": {
                    "limit": 7
                }
            }
        });

        let parsed = SyncService::sync_response_filter_from_filter_json(&filter);

        assert_eq!(
            parsed.event_fields,
            Some(vec![
                "type".to_string(),
                "content.body".to_string(),
                "unsigned.age".to_string()
            ])
        );
        assert_eq!(parsed.event_format, SyncEventFormat::Federation);
        assert_eq!(
            parsed
                .presence
                .as_ref()
                .and_then(|presence| presence.types.as_ref()),
            Some(&vec!["m.presence".to_string()])
        );
        assert_eq!(
            parsed
                .presence
                .as_ref()
                .and_then(|presence| presence.not_senders.as_ref()),
            Some(&vec!["@mallory:localhost".to_string()])
        );
        assert_eq!(
            parsed
                .room
                .as_ref()
                .and_then(|room| room.timeline.as_ref())
                .and_then(|timeline| timeline.limit),
            Some(7)
        );
    }

    #[test]
    fn test_apply_sync_filter_to_values_filters_types_and_senders() {
        let events = vec![
            json!({
                "type": "m.receipt",
                "sender": "@alice:localhost",
                "content": {}
            }),
            json!({
                "type": "m.typing",
                "sender": "@bob:localhost",
                "content": {}
            }),
            json!({
                "type": "m.receipt",
                "sender": "@mallory:localhost",
                "content": {}
            }),
        ];
        let filter = SyncFilter {
            limit: None,
            types: Some(vec!["m.receipt".to_string()]),
            not_types: None,
            rooms: None,
            not_rooms: None,
            contains_url: None,
            lazy_load_members: None,
            include_redundant_members: None,
            senders: None,
            not_senders: Some(vec!["@mallory:localhost".to_string()]),
        };

        let filtered = SyncService::apply_sync_filter_to_values(events, Some(&filter));

        assert_eq!(filtered.len(), 1);
        assert_eq!(filtered[0]["type"], "m.receipt");
        assert_eq!(filtered[0]["sender"], "@alice:localhost");
    }

    #[test]
    fn test_apply_sync_filter_to_values_filters_presence_events() {
        let events = vec![
            json!({
                "type": "m.presence",
                "sender": "@alice:localhost",
                "content": { "presence": "online" }
            }),
            json!({
                "type": "m.presence",
                "sender": "@mallory:localhost",
                "content": { "presence": "online" }
            }),
        ];
        let filter = SyncFilter {
            limit: None,
            types: Some(vec!["m.presence".to_string()]),
            not_types: None,
            rooms: None,
            not_rooms: None,
            contains_url: None,
            lazy_load_members: None,
            include_redundant_members: None,
            senders: Some(vec!["@alice:localhost".to_string()]),
            not_senders: None,
        };

        let filtered = SyncService::apply_sync_filter_to_values(events, Some(&filter));

        assert_eq!(filtered.len(), 1);
        assert_eq!(filtered[0]["type"], "m.presence");
        assert_eq!(filtered[0]["sender"], "@alice:localhost");
    }

    #[test]
    fn test_apply_timeline_limit_truncates_events() {
        let (events, limited) = SyncService::apply_timeline_limit(
            vec![
                sample_room_event("1"),
                sample_room_event("2"),
                sample_room_event("3"),
            ],
            2,
        );

        assert!(limited);
        assert_eq!(events.len(), 2);
        assert_eq!(events[0].event_id, "$event2");
        assert_eq!(events[1].event_id, "$event1");
    }

    #[test]
    fn test_slow_request_threshold() {
        assert!(SyncService::is_slow_request_for(750.0, 750));
        assert!(!SyncService::is_slow_request_for(749.0, 750));
    }

    #[test]
    fn test_apply_sync_filter_to_values_filters_rooms_and_wildcard_types() {
        let events = vec![
            json!({
                "room_id": "!allowed:localhost",
                "type": "m.room.message",
                "sender": "@alice:localhost",
                "content": {}
            }),
            json!({
                "room_id": "!blocked:localhost",
                "type": "m.room.member",
                "sender": "@alice:localhost",
                "content": {}
            }),
        ];
        let filter = SyncFilter {
            limit: None,
            types: Some(vec!["m.room.*".to_string()]),
            not_types: Some(vec!["m.room.member".to_string()]),
            rooms: Some(vec!["!allowed:localhost".to_string()]),
            not_rooms: Some(vec!["!blocked:localhost".to_string()]),
            contains_url: None,
            lazy_load_members: None,
            include_redundant_members: None,
            senders: None,
            not_senders: None,
        };

        let filtered = SyncService::apply_sync_filter_to_values(events, Some(&filter));

        assert_eq!(filtered.len(), 1);
        assert_eq!(filtered[0]["room_id"], "!allowed:localhost");
        assert_eq!(filtered[0]["type"], "m.room.message");
    }

    #[test]
    fn test_filter_event_fields_keeps_nested_fields() {
        let event = json!({
            "type": "m.room.message",
            "content": {
                "body": "hello",
                "msgtype": "m.text"
            },
            "unsigned": {
                "age": 12,
                "transaction_id": "t1"
            },
            "sender": "@alice:localhost"
        });

        let filtered = SyncService::filter_event_fields(
            event,
            Some(&[
                "type".to_string(),
                "content.body".to_string(),
                "unsigned.age".to_string(),
            ]),
        );

        assert_eq!(filtered["type"], "m.room.message");
        assert_eq!(filtered["content"]["body"], "hello");
        assert!(filtered["content"].get("msgtype").is_none());
        assert_eq!(filtered["unsigned"]["age"], 12);
        assert!(filtered["unsigned"].get("transaction_id").is_none());
        assert!(filtered.get("sender").is_none());
    }

    #[test]
    fn test_apply_sync_filter_to_values_filters_contains_url() {
        let events = vec![
            json!({
                "type": "m.room.message",
                "sender": "@alice:localhost",
                "content": { "body": "file", "url": "mxc://example.com/file" }
            }),
            json!({
                "type": "m.room.message",
                "sender": "@alice:localhost",
                "content": { "body": "plain text" }
            }),
        ];
        let filter = SyncFilter {
            limit: None,
            types: Some(vec!["m.room.message".to_string()]),
            not_types: None,
            rooms: None,
            not_rooms: None,
            contains_url: Some(true),
            lazy_load_members: None,
            include_redundant_members: None,
            senders: None,
            not_senders: None,
        };

        let filtered = SyncService::apply_sync_filter_to_values(events, Some(&filter));

        assert_eq!(filtered.len(), 1);
        assert_eq!(filtered[0]["content"]["url"], "mxc://example.com/file");
    }

    #[test]
    fn test_apply_lazy_load_members_keeps_only_timeline_members_and_self() {
        let state_events = vec![
            json!({
                "type": "m.room.member",
                "state_key": "@alice:localhost",
                "content": { "membership": "join" }
            }),
            json!({
                "type": "m.room.member",
                "state_key": "@bob:localhost",
                "content": { "membership": "join" }
            }),
            json!({
                "type": "m.room.member",
                "state_key": "@carol:localhost",
                "content": { "membership": "join" }
            }),
            json!({
                "type": "m.room.name",
                "state_key": "",
                "content": { "name": "Test Room" }
            }),
        ];
        let timeline_events = vec![RoomEvent {
            event_id: "$event".to_string(),
            room_id: "!room:localhost".to_string(),
            user_id: "@bob:localhost".to_string(),
            event_type: "m.room.message".to_string(),
            content: json!({ "body": "hello" }),
            state_key: None,
            depth: 1,
            origin_server_ts: 1,
            processed_ts: 1,
            not_before: 0,
            status: None,
            reference_image: None,
            origin: "self".to_string(),
        }];

        let (filtered, known_now) = SyncService::apply_lazy_load_members_with_cache(
            state_events,
            &timeline_events,
            "@alice:localhost",
            &HashSet::new(),
            false,
            &HashSet::new(),
        );

        assert_eq!(filtered.len(), 3);
        assert!(filtered
            .iter()
            .any(|event| event["state_key"] == "@alice:localhost"));
        assert!(filtered
            .iter()
            .any(|event| event["state_key"] == "@bob:localhost"));
        assert!(filtered.iter().any(|event| event["type"] == "m.room.name"));
        assert!(!filtered
            .iter()
            .any(|event| event["state_key"] == "@carol:localhost"));
        assert!(known_now.contains("@bob:localhost"));
        assert!(known_now.contains("@alice:localhost"));
    }

    #[test]
    fn test_apply_lazy_load_members_skips_cached_members_by_default() {
        let state_events = vec![
            json!({
                "type": "m.room.member",
                "state_key": "@alice:localhost",
                "content": { "membership": "join" }
            }),
            json!({
                "type": "m.room.member",
                "state_key": "@bob:localhost",
                "content": { "membership": "join" }
            }),
            json!({
                "type": "m.room.name",
                "state_key": "",
                "content": { "name": "Test Room" }
            }),
        ];
        let timeline_events = vec![RoomEvent {
            event_id: "$event".to_string(),
            room_id: "!room:localhost".to_string(),
            user_id: "@bob:localhost".to_string(),
            event_type: "m.room.message".to_string(),
            content: json!({ "body": "hello" }),
            state_key: None,
            depth: 1,
            origin_server_ts: 1,
            processed_ts: 1,
            not_before: 0,
            status: None,
            reference_image: None,
            origin: "self".to_string(),
        }];

        let (filtered, known_now) = SyncService::apply_lazy_load_members_with_cache(
            state_events,
            &timeline_events,
            "@alice:localhost",
            &HashSet::from([String::from("@bob:localhost")]),
            false,
            &HashSet::new(),
        );

        assert_eq!(filtered.len(), 2);
        assert!(filtered
            .iter()
            .any(|event| event["state_key"] == "@alice:localhost"));
        assert!(!filtered
            .iter()
            .any(|event| event["state_key"] == "@bob:localhost"));
        assert!(filtered.iter().any(|event| event["type"] == "m.room.name"));
        assert!(known_now.contains("@bob:localhost"));
    }

    #[test]
    fn test_apply_lazy_load_members_can_include_redundant_members() {
        let state_events = vec![
            json!({
                "type": "m.room.member",
                "state_key": "@alice:localhost",
                "content": { "membership": "join" }
            }),
            json!({
                "type": "m.room.member",
                "state_key": "@bob:localhost",
                "content": { "membership": "join" }
            }),
        ];
        let timeline_events = vec![RoomEvent {
            event_id: "$event".to_string(),
            room_id: "!room:localhost".to_string(),
            user_id: "@bob:localhost".to_string(),
            event_type: "m.room.message".to_string(),
            content: json!({ "body": "hello" }),
            state_key: None,
            depth: 1,
            origin_server_ts: 1,
            processed_ts: 1,
            not_before: 0,
            status: None,
            reference_image: None,
            origin: "self".to_string(),
        }];

        let (filtered, _) = SyncService::apply_lazy_load_members_with_cache(
            state_events,
            &timeline_events,
            "@alice:localhost",
            &HashSet::from([String::from("@bob:localhost")]),
            true,
            &HashSet::new(),
        );

        assert_eq!(filtered.len(), 2);
        assert!(filtered
            .iter()
            .any(|event| event["state_key"] == "@alice:localhost"));
        assert!(filtered
            .iter()
            .any(|event| event["state_key"] == "@bob:localhost"));
    }

    #[test]
    fn test_apply_lazy_load_members_includes_state_delta_members_on_incremental_sync() {
        let state_events = vec![
            json!({
                "type": "m.room.member",
                "state_key": "@alice:localhost",
                "content": { "membership": "join" }
            }),
            json!({
                "type": "m.room.member",
                "state_key": "@dave:localhost",
                "content": { "membership": "join" }
            }),
        ];

        let (filtered, known_now) = SyncService::apply_lazy_load_members_with_cache(
            state_events,
            &[],
            "@alice:localhost",
            &HashSet::new(),
            false,
            &HashSet::from([String::from("@dave:localhost")]),
        );

        assert_eq!(filtered.len(), 2);
        assert!(filtered
            .iter()
            .any(|event| event["state_key"] == "@alice:localhost"));
        assert!(filtered
            .iter()
            .any(|event| event["state_key"] == "@dave:localhost"));
        assert!(known_now.contains("@dave:localhost"));
    }

    #[test]
    fn test_apply_lazy_load_members_replays_cached_state_delta_members() {
        let state_events = vec![json!({
            "type": "m.room.member",
            "state_key": "@dave:localhost",
            "content": { "membership": "join" }
        })];

        let (filtered, known_now) = SyncService::apply_lazy_load_members_with_cache(
            state_events,
            &[],
            "@alice:localhost",
            &HashSet::from([String::from("@dave:localhost")]),
            false,
            &HashSet::from([String::from("@dave:localhost")]),
        );

        assert_eq!(filtered.len(), 1);
        assert_eq!(filtered[0]["state_key"], "@dave:localhost");
        assert!(known_now.contains("@dave:localhost"));
    }

    #[test]
    fn test_rooms_to_include_keeps_rooms_with_state_delta_membership() {
        let room_ids = vec![
            "!timeline:localhost".to_string(),
            "!state:localhost".to_string(),
            "!skip:localhost".to_string(),
        ];
        let room_events = HashMap::from([
            (
                "!timeline:localhost".to_string(),
                vec![RoomEvent {
                    event_id: "$event".to_string(),
                    room_id: "!timeline:localhost".to_string(),
                    user_id: "@alice:localhost".to_string(),
                    event_type: "m.room.message".to_string(),
                    content: json!({ "body": "hello" }),
                    state_key: None,
                    depth: 1,
                    origin_server_ts: 1,
                    processed_ts: 1,
                    not_before: 0,
                    status: None,
                    reference_image: None,
                    origin: "self".to_string(),
                }],
            ),
            ("!state:localhost".to_string(), Vec::new()),
            ("!skip:localhost".to_string(), Vec::new()),
        ]);
        let changed_members_by_room = HashMap::from([
            (
                "!state:localhost".to_string(),
                HashSet::from([String::from("@dave:localhost")]),
            ),
            ("!skip:localhost".to_string(), HashSet::new()),
        ]);

        let non_member_state_change_ts_by_room = HashMap::from([
            ("!state:localhost".to_string(), 2_i64),
            ("!skip:localhost".to_string(), 0_i64),
        ]);

        let rooms = SyncService::rooms_to_include(
            &room_ids,
            &room_events,
            &changed_members_by_room,
            &non_member_state_change_ts_by_room,
            true,
        );

        assert_eq!(
            rooms,
            vec![
                "!timeline:localhost".to_string(),
                "!state:localhost".to_string()
            ]
        );
    }

    #[test]
    fn test_filter_sync_rooms_respects_room_lists() {
        let memberships = vec![
            UserRoomMembership {
                room_id: "!keep:localhost".to_string(),
                membership: "join".to_string(),
            },
            UserRoomMembership {
                room_id: "!drop:localhost".to_string(),
                membership: "leave".to_string(),
            },
        ];
        let room_filter = RoomFilter {
            rooms: Some(vec!["!keep:localhost".to_string()]),
            not_rooms: Some(vec!["!drop:localhost".to_string()]),
            include_leave: Some(true),
            state: None,
            timeline: None,
            ephemeral: None,
            account_data: None,
        };

        let filtered = SyncService::filter_sync_rooms(memberships, Some(&room_filter));

        assert_eq!(filtered.len(), 1);
        assert_eq!(filtered[0].room_id, "!keep:localhost");
        assert_eq!(filtered[0].membership, "join");
    }

    fn sample_room_event(event_id_suffix: &str) -> RoomEvent {
        RoomEvent {
            event_id: format!("$event{event_id_suffix}"),
            room_id: "!room:example.com".to_string(),
            user_id: "@user:example.com".to_string(),
            event_type: "m.room.message".to_string(),
            content: json!({
                "body": "hello",
                "msgtype": "m.text"
            }),
            state_key: None,
            depth: 1,
            origin_server_ts: 1_777_000_000_000,
            processed_ts: 1_777_000_000_000,
            not_before: 0,
            status: None,
            reference_image: None,
            origin: "example.com".to_string(),
        }
    }
}
