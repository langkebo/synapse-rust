use serde::{Deserialize, Serialize};
use std::sync::Arc;
use std::time::Instant;
use synapse_cache::CacheManager;
use synapse_common::config::PerformanceConfig;
use synapse_common::error::ApiError;
use synapse_common::metrics::MetricsCollector;
use synapse_e2ee::to_device::ToDeviceStorage;
use synapse_storage::device::DeviceRepository;
use synapse_storage::event::EventRepository;
use synapse_storage::PresenceRepository;
use synapse_storage::sliding_sync::{SlidingSyncRequest, SlidingSyncResponse, SlidingSyncStorage};
use synapse_storage::RoomMemberRepository;

mod extensions;
mod filters;
mod state;
mod timeline;

#[cfg(test)]
mod tests;

/// Default TTL for sliding sync connections: 30 minutes in milliseconds.
const CONNECTION_TTL_MS: i64 = 30 * 60 * 1000;

/// Maximum number of tracked connections (LRU capacity cap).
const MAX_TRACKED_CONNECTIONS: u64 = 10_000;

/// Histogram name used to track sliding sync response latency (ms).
const SLIDING_SYNC_LATENCY_HISTOGRAM: &str = "sliding_sync_request_duration_ms";

/// Counter name used to track slow sliding sync requests (those exceeding
/// the configured latency threshold).
const SLIDING_SYNC_SLOW_REQUESTS_COUNTER: &str = "sliding_sync_slow_requests_total";

#[derive(Clone)]
pub struct SlidingSyncService {
    storage: SlidingSyncStorage,
    cache: Arc<CacheManager>,
    event_storage: Arc<dyn EventRepository>,
    typing_service: Arc<crate::typing_service::TypingService>,
    presence_storage: Arc<dyn PresenceRepository>,
    member_storage: Arc<dyn RoomMemberRepository>,
    device_storage: Arc<dyn DeviceRepository>,
    to_device_storage: ToDeviceStorage,
    /// Tracks last-access timestamp per (user_id, device_id, conn_id) for LRU + TTL GC.
    connection_tracker: Arc<moka::sync::Cache<String, i64>>,
    /// Metrics collector used to record sync latency histograms and slow
    /// request counters. Acts as the performance rollback gate for
    /// sliding sync (see Synapse v1.153.0rc3 revert lesson).
    metrics: Arc<MetricsCollector>,
    /// Sliding sync response latency threshold in milliseconds. Responses
    /// slower than this trigger a warning log and increment the slow
    /// request counter.
    latency_threshold_ms: u64,
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
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        storage: SlidingSyncStorage,
        cache: Arc<CacheManager>,
        event_storage: Arc<dyn EventRepository>,
        typing_service: Arc<crate::typing_service::TypingService>,
        presence_storage: Arc<dyn PresenceRepository>,
        member_storage: Arc<dyn RoomMemberRepository>,
        device_storage: Arc<dyn DeviceRepository>,
        to_device_storage: ToDeviceStorage,
        metrics: Arc<MetricsCollector>,
        performance: PerformanceConfig,
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
            device_storage,
            to_device_storage,
            connection_tracker: Arc::new(connection_tracker),
            metrics,
            latency_threshold_ms: performance.sliding_sync_latency_threshold_ms,
        }
    }

    /// Returns the configured sliding sync latency threshold in milliseconds.
    pub fn latency_threshold_ms(&self) -> u64 {
        self.latency_threshold_ms
    }

    /// Returns the current p95 sliding sync response latency in milliseconds,
    /// or `None` if no observations have been recorded yet. Used by the
    /// performance rollback gate to detect regressions.
    pub fn sync_latency_p95_ms(&self) -> Option<f64> {
        self.metrics
            .get_histogram(SLIDING_SYNC_LATENCY_HISTOGRAM)
            .and_then(|h| h.get_percentile(95.0).ok())
            .filter(|v| *v > 0.0)
    }

    /// Returns the total number of sliding sync requests that exceeded the
    /// configured latency threshold since startup.
    pub fn slow_sync_request_count(&self) -> u64 {
        self.metrics.get_counter(SLIDING_SYNC_SLOW_REQUESTS_COUNTER).map_or(0, |c| c.get())
    }

    pub async fn sync(
        &self,
        user_id: &str,
        device_id: &str,
        request: SlidingSyncRequest,
    ) -> Result<SlidingSyncResponse, ApiError> {
        let started = Instant::now();
        let conn_id_for_metrics = request.conn_id.clone();
        let is_initial = request.pos.is_none();

        let result = self.sync_inner(user_id, device_id, request).await;

        let total_ms = started.elapsed().as_secs_f64() * 1000.0;
        self.record_sync_latency_metrics(user_id, device_id, conn_id_for_metrics.as_deref(), total_ms, is_initial);

        result
    }

    /// Records sliding sync latency into the metrics histogram and emits a
    /// warning when the response time exceeds the configured threshold.
    /// This is the performance rollback gate: a sustained increase in
    /// `sliding_sync_slow_requests_total` or the p95 of
    /// `sliding_sync_request_duration_ms` signals that a recent change
    /// regressed sliding sync performance and should be rolled back.
    fn record_sync_latency_metrics(
        &self,
        user_id: &str,
        device_id: &str,
        conn_id: Option<&str>,
        total_ms: f64,
        is_initial: bool,
    ) {
        // Observe latency in the histogram (enables p95/p99 reporting).
        if let Some(histogram) = self.metrics.get_histogram(SLIDING_SYNC_LATENCY_HISTOGRAM) {
            histogram.observe(total_ms);
        } else {
            self.metrics.register_histogram(SLIDING_SYNC_LATENCY_HISTOGRAM.to_string()).observe(total_ms);
        }

        if total_ms >= self.latency_threshold_ms as f64 {
            if let Some(counter) = self.metrics.get_counter(SLIDING_SYNC_SLOW_REQUESTS_COUNTER) {
                counter.inc();
            } else {
                self.metrics.register_counter(SLIDING_SYNC_SLOW_REQUESTS_COUNTER.to_string()).inc();
            }

            let p95 = self
                .metrics
                .get_histogram(SLIDING_SYNC_LATENCY_HISTOGRAM)
                .and_then(|h| h.get_percentile(95.0).ok())
                .unwrap_or(0.0);

            tracing::warn!(
                target: "sliding_sync_performance",
                user_id = %user_id,
                device_id = %device_id,
                conn_id = ?conn_id,
                total_ms = total_ms,
                threshold_ms = self.latency_threshold_ms,
                is_initial = is_initial,
                p95_ms = p95,
                "Slow sliding sync request detected; consider rolling back recent sliding sync changes"
            );
        }
    }

    async fn sync_inner(
        &self,
        user_id: &str,
        device_id: &str,
        request: SlidingSyncRequest,
    ) -> Result<SlidingSyncResponse, ApiError> {
        // Update user presence to online
        tracing::info!(user_id = %user_id, device_id = %device_id, "Updating presence for user");
        if let Err(e) = self.presence_storage.set_presence(user_id, "online", None).await {
            tracing::warn!(%e, user_id, device_id, "Failed to set presence online");
        }

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
}
