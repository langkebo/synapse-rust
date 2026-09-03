use futures::stream::{self, StreamExt};
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use std::time::Instant;
use synapse_cache::CacheManager;
use synapse_common::config::PerformanceConfig;
use synapse_common::current_timestamp_millis;
use synapse_common::error::ApiError;
use synapse_common::metrics::MetricsCollector;
use synapse_e2ee::device_keys::DeviceKeyStoreApi;
use synapse_e2ee::to_device::ToDeviceStorage;
use synapse_storage::sliding_sync::{SlidingSyncRequest, SlidingSyncResponse, SlidingSyncStoreApi};

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

/// MSC4186: TTL for txn_id idempotency cache. Retries within this window
/// receive the cached response. Matches Synapse's default of 5 minutes.
///
/// 内存修复：原 TTL 5 分钟 + 容量 10000 导致 txn_id 缓存稳态 ~1.7GB（每条
/// response 含 lists/rooms 大 serde_json::Value，clone 深拷贝）。jemalloc prof
/// 实测 clone_subtree(SlidingSyncResponse::clone) 占稳态 98.7%。将 TTL 降到 1
/// 分钟、容量降到 1000，把稳态压到 ~170MB，仍在 MSC4186 幂等重试窗口内。
const TXN_ID_CACHE_TTL_MS: u64 = 60 * 1000;

/// MSC4186: Maximum number of cached txn_id responses per service instance.
/// Bounds memory usage under retry storms; LRU eviction applies beyond this.
const MAX_TXN_ID_CACHE_ENTRIES: u64 = 1_000;

/// Histogram name used to track sliding sync response latency (ms).
const SLIDING_SYNC_LATENCY_HISTOGRAM: &str = "sliding_sync_request_duration_ms";

/// Counter name used to track slow sliding sync requests (those exceeding
/// the configured latency threshold).
const SLIDING_SYNC_SLOW_REQUESTS_COUNTER: &str = "sliding_sync_slow_requests_total";

/// Maximum time an idle incremental sliding sync is held open before returning
/// an (empty) response. Caps worst-case latency and prevents a client from
/// forcing the server to wait indefinitely via a huge `timeout`.
const MAX_SLIDING_SYNC_IDLE_WAIT_MS: u64 = 30_000;

/// Default idle wait when the client omits the `timeout` field.
const DEFAULT_SLIDING_SYNC_IDLE_WAIT_MS: u64 = 10_000;

#[derive(Clone)]
pub struct SlidingSyncService {
    storage: Arc<dyn SlidingSyncStoreApi>,
    cache: Arc<CacheManager>,
    event_reader: Arc<dyn synapse_storage::event::EventReader>,
    device_key_storage: Arc<dyn DeviceKeyStoreApi>,
    typing_service: Arc<crate::typing_service::TypingService>,
    presence_storage: Arc<dyn synapse_storage::presence::PresenceStoreApi>,
    member_storage: Arc<dyn synapse_storage::membership::MemberStoreApi>,
    device_storage: Arc<dyn synapse_storage::device::DeviceListStoreApi>,
    to_device_storage: ToDeviceStorage,
    /// MSC4354: Sticky event storage. When present, sticky events for each
    /// room are injected into the sliding sync room response as
    /// `sticky_events`. `None` disables the integration (e.g. in tests
    /// that don't exercise sticky events).
    sticky_event_storage: Option<Arc<dyn synapse_storage::sticky_event::StickyEventStoreApi>>,
    /// Tracks last-access timestamp per (user_id, device_id, conn_id) for LRU + TTL GC.
    connection_tracker: Arc<moka::sync::Cache<String, i64>>,
    /// MSC4186: txn_id idempotency cache. When a request carries a `txn_id`,
    /// the server caches the response keyed by `(user_id, device_id, txn_id)`
    /// and returns the cached body for subsequent retries with the same
    /// `txn_id`. Bounded by `MAX_TXN_ID_CACHE_ENTRIES` with a TTL of
    /// `TXN_ID_CACHE_TTL_MS` to prevent unbounded growth and stale entries.
    txn_id_cache: Arc<moka::future::Cache<String, SlidingSyncResponse>>,
    /// Metrics collector used to record sync latency histograms and slow
    /// request counters. Acts as the performance rollback gate for
    /// sliding sync (see Synapse v1.153.0rc3 revert lesson).
    metrics: Arc<MetricsCollector>,
    /// Sliding sync response latency threshold in milliseconds. Responses
    /// slower than this trigger a warning log and increment the slow
    /// request counter.
    latency_threshold_ms: u64,
    /// Wake-up channel used to implement long-polling. When present, an
    /// incremental sync that has nothing to return parks on this notifier
    /// instead of returning immediately, and is woken the moment an event
    /// lands for the user or one of their rooms. `None` disables long-polling
    /// and falls back to a plain timed wait (used by tests and benchmarks).
    event_notifier: Option<crate::event_notifier::EventNotifier>,
}

#[derive(Debug, Clone, Default)]
pub(crate) struct RoomSubscriptionConfig {
    pub(crate) timeline_limit: Option<u32>,
    pub(crate) required_state: Option<Vec<Vec<String>>>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub(crate) struct SlidingListWindowSnapshot {
    pub(crate) ranges: Vec<SlidingListRangeSnapshot>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub(crate) struct SlidingListRangeSnapshot {
    pub(crate) start: u32,
    pub(crate) end: u32,
    pub(crate) room_ids: Vec<String>,
}

impl SlidingSyncService {
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        storage: Arc<dyn SlidingSyncStoreApi>,
        cache: Arc<CacheManager>,
        event_reader: Arc<dyn synapse_storage::event::EventReader>,
        device_key_storage: Arc<dyn DeviceKeyStoreApi>,
        typing_service: Arc<crate::typing_service::TypingService>,
        presence_storage: Arc<dyn synapse_storage::presence::PresenceStoreApi>,
        member_storage: Arc<dyn synapse_storage::membership::MemberStoreApi>,
        device_storage: Arc<dyn synapse_storage::device::DeviceListStoreApi>,
        to_device_storage: ToDeviceStorage,
        metrics: Arc<MetricsCollector>,
        performance: PerformanceConfig,
        sticky_event_storage: Option<Arc<dyn synapse_storage::sticky_event::StickyEventStoreApi>>,
    ) -> Self {
        let connection_tracker = moka::sync::Cache::builder()
            .max_capacity(MAX_TRACKED_CONNECTIONS)
            .time_to_idle(std::time::Duration::from_millis(CONNECTION_TTL_MS as u64))
            .build();
        let txn_id_cache = moka::future::Cache::builder()
            .max_capacity(MAX_TXN_ID_CACHE_ENTRIES)
            .time_to_live(std::time::Duration::from_millis(TXN_ID_CACHE_TTL_MS))
            .build();
        Self {
            storage,
            cache,
            event_reader,
            device_key_storage,
            typing_service,
            presence_storage,
            member_storage,
            device_storage,
            to_device_storage,
            sticky_event_storage,
            connection_tracker: Arc::new(connection_tracker),
            txn_id_cache: Arc::new(txn_id_cache),
            metrics,
            latency_threshold_ms: performance.sliding_sync_latency_threshold_ms,
            event_notifier: None,
        }
    }

    /// Attaches the event notifier that powers long-polling.
    ///
    /// Without it an idle incremental sync still waits, but only on a timer —
    /// new events are not delivered until the wait elapses. Production wiring
    /// must call this; tests and benchmarks may omit it.
    #[must_use]
    pub fn with_event_notifier(mut self, event_notifier: crate::event_notifier::EventNotifier) -> Self {
        self.event_notifier = Some(event_notifier);
        self
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
        let txn_id = request.txn_id.clone();

        // MSC4186 §6.1: txn_id idempotency. When a request carries a `txn_id`,
        // return the cached response for retries with the same `txn_id`. This
        // must happen BEFORE sync_inner to avoid redundant work and to ensure
        // retry-safety when the original response was lost in transit.
        if let Some(txn_id) = &txn_id {
            let cache_key = Self::txn_id_cache_key(user_id, device_id, txn_id);
            if let Some(cached) = self.txn_id_cache.get(&cache_key).await {
                tracing::debug!(
                    user_id = %user_id,
                    device_id = %device_id,
                    txn_id = %txn_id,
                    "MSC4186 txn_id cache hit — returning cached sliding sync response"
                );
                // Record latency for the cache-hit path too, so metrics reflect
                // the real response time clients observe (sub-millisecond).
                let total_ms = started.elapsed().as_secs_f64() * 1000.0;
                self.record_sync_latency_metrics(
                    user_id,
                    device_id,
                    conn_id_for_metrics.as_deref(),
                    total_ms,
                    is_initial,
                );
                return Ok(cached);
            }
        }

        let (result, idle_wait_ms) = match self.sync_inner(user_id, device_id, request).await {
            Ok((response, idle_wait_ms)) => (Ok(response), idle_wait_ms),
            Err(e) => (Err(e), 0),
        };

        // MSC4186: on success, cache the response under txn_id so retries
        // receive the same body. Errors are NOT cached — a failed request
        // must be retried and may succeed on the next attempt.
        if let Some(txn_id) = &txn_id {
            if let Ok(ref response) = result {
                let cache_key = Self::txn_id_cache_key(user_id, device_id, txn_id);
                self.txn_id_cache.insert(cache_key, response.clone()).await;
            }
        }

        // Exclude the idle long-poll from the latency metric: parking is the
        // intended behaviour, not slowness, and counting it would permanently
        // trip the sliding-sync performance rollback gate.
        let total_ms = (started.elapsed().as_secs_f64() * 1000.0 - idle_wait_ms as f64).max(0.0);
        self.record_sync_latency_metrics(user_id, device_id, conn_id_for_metrics.as_deref(), total_ms, is_initial);

        result
    }

    /// MSC4186: Builds the txn_id cache key from `(user_id, device_id, txn_id)`.
    /// The key is namespaced to avoid collisions with other caches and to
    /// isolate responses per user+device, preventing cross-user leakage.
    fn txn_id_cache_key(user_id: &str, device_id: &str, txn_id: &str) -> String {
        format!("msc4186:txn_id:{user_id}:{device_id}:{txn_id}")
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

    /// Builds one sliding sync response.
    ///
    /// Returns the response together with the number of milliseconds the
    /// request spent parked in the idle long-poll, so the caller can subtract
    /// it from the measured latency.
    async fn sync_inner(
        &self,
        user_id: &str,
        device_id: &str,
        request: SlidingSyncRequest,
    ) -> Result<(SlidingSyncResponse, u64), ApiError> {
        // Mark the user online, but only on the *initial* sync.
        //
        // Previously this ran on EVERY sliding sync request. That had two bad
        // effects:
        //   1. It thrashed the presence table with a write per request
        //      (hundreds of redundant writes per minute per active client).
        //   2. Because the presence extension re-echoes the user's own presence
        //      on every response, the client observed a "fresh" presence event
        //      on every sync and busy-looped its sliding sync (the 1:1
        //      sync↔presence self-excitation seen in production).
        //
        // The client already owns online assertion (it sets presence=online on
        // login and refreshes it via a 4-minute heartbeat), so the server must
        // NOT rewrite presence on each incremental request.
        if request.pos.is_none() {
            if let Err(e) = self.presence_storage.set_presence(user_id, "online", None).await {
                tracing::warn!(%e, user_id, device_id, "Failed to set presence online");
            }
        }

        let conn_id = request.conn_id.as_deref();

        // Lazy GC: clean up expired connections for this user/device before proceeding.
        self.gc_expired_connections(user_id, device_id).await;

        // Touch the connection in the LRU tracker (records last access time).
        let now = current_timestamp_millis();
        let tracker_key = Self::connection_tracker_key(user_id, device_id, conn_id);
        self.connection_tracker.insert(tracker_key, now);

        let is_initial = request.pos.is_none();

        if let Some(pos_str) = &request.pos {
            if !self
                .storage
                .validate_pos(user_id, device_id, conn_id, pos_str)
                .await
                .map_err(|e| ApiError::internal_with_context("Failed to validate pos", &e))?
            {
                // MSC4186: 非法/过期的 pos 用专用 M_UNKNOWN_POS errcode，
                // 客户端据此 resetup 重同步，而非把其他 400 误判为 pos 过期。
                return Err(ApiError::unknown_pos("Invalid or expired position token"));
            }
        }

        // ── S14/SS-10: 增量 timeline 水位线 ────────────────────────────────
        // prev_event_stream_pos：上一轮同步（token 行）记录的事件流水快照，
        // 本轮增量同步的 timeline 只下发 stream_ordering 大于它的事件。
        // stream_snapshot：本轮读阶段开始时的最大流水号，在同步结束时写回
        // token 行，作为下一轮的水位线。在读阶段开始前取快照可保证：
        // 快照之后到达的事件要么被本轮读到（stream_ordering > 上一轮水位线
        // 仍成立），要么留待下一轮 —— 不会漏发。
        let prev_event_stream_pos: Option<i64> = if is_initial {
            None
        } else {
            self.storage
                .get_token(user_id, device_id, conn_id)
                .await
                .map_err(|e| ApiError::internal_with_context("Failed to load token watermark", &e))?
                .map(|token| token.event_stream_pos)
        };
        let stream_snapshot: i64 = self.event_reader.get_max_stream_ordering().await.unwrap_or(0);

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
                .map_err(|e| ApiError::internal_with_context("Failed to save list", &e))?;
        }

        if let Some(unsubs) = &request.unsubscribe_rooms {
            // B-1.5: Previously each room in `unsubscribe_rooms` triggered an
            // independent delete_room round-trip.  Replace the loop with a single
            // batch call.  Empty slice short-circuits without a DB round-trip.
            self.storage
                .delete_rooms_batch(user_id, device_id, unsubs, conn_id)
                .await
                .map_err(|e| ApiError::internal_with_context("Failed to unsubscribe rooms", &e))?;
        }

        if is_initial {
            // S12: Previously `if let Ok` + `let _ =` silently swallowed all
            // errors from both get_joined_rooms and materialize_room_from_activity.
            // Now we log warnings so failures are visible in production without
            // crashing the sync (materialization is best-effort — the room will
            // be materialized on the next incremental sync or room subscription).
            match self.member_storage.get_joined_rooms(user_id).await {
                Ok(joined_rooms) => {
                    // P3: Materialize rooms concurrently with bounded parallelism
                    // instead of sequential for-loop.  Each materialize_room_from_activity
                    // call is an independent write to the sliding-sync store, so
                    // running them concurrently with buffer_unordered(8) reduces
                    // initial sync latency for users in many rooms.
                    const MAX_CONCURRENT_MATERIALIZE: usize = 8;

                    let storage = Arc::clone(&self.storage);
                    let user_id = user_id.to_string();
                    let device_id = device_id.to_string();
                    let conn_id = conn_id.map(|s| s.to_string());

                    let results: Vec<(String, Result<Option<_>, _>)> = stream::iter(joined_rooms.into_iter())
                        .map(|room_id| {
                            let storage = Arc::clone(&storage);
                            let user_id = user_id.clone();
                            let device_id = device_id.clone();
                            let conn_id = conn_id.clone();
                            async move {
                                let result = storage
                                    .materialize_room_from_activity(
                                        &user_id,
                                        &device_id,
                                        &room_id,
                                        conn_id.as_deref(),
                                        None,
                                    )
                                    .await;
                                (room_id, result)
                            }
                        })
                        .buffer_unordered(MAX_CONCURRENT_MATERIALIZE)
                        .collect()
                        .await;

                    for (room_id, result) in results {
                        if let Err(e) = result {
                            tracing::warn!(
                                user_id = %user_id,
                                device_id = %device_id,
                                room_id = %room_id,
                                error = %e,
                                "S12: Failed to materialize room during initial sync"
                            );
                        }
                    }
                }
                Err(e) => {
                    tracing::warn!(
                        user_id = %user_id,
                        device_id = %device_id,
                        error = %e,
                        "S12: Failed to get joined rooms for initial sync materialization"
                    );
                }
            }
        }

        // ── Long-poll waiter registration ────────────────────────────────────
        // Register the wake-up waiters BEFORE reading any state below.
        //
        // Producers write to the database and *then* notify. By registering
        // first we guarantee that every event is either (a) already visible to
        // the reads below — so this sync is not idle and returns data — or
        // (b) delivered to an already-registered waiter. Registering after the
        // reads would leave a window in which a notification is dropped
        // (`notify_waiters` stores no permit) and the client stalls for the
        // full timeout.
        //
        // `Notified::enable()` is what makes registration eager; simply
        // creating the future is not enough, it only registers when first
        // polled.
        let notify_slots = match self.event_notifier.as_ref() {
            Some(notifier) if !is_initial => {
                let room_ids = self.member_storage.get_joined_rooms(user_id).await.unwrap_or_default();
                notifier.slots_for(user_id, &room_ids)
            }
            // An initial sync always returns data, so it never parks.
            _ => Vec::new(),
        };
        let mut long_poll_waiters: Vec<_> = notify_slots
            .iter()
            .map(|slot| {
                let mut waiter = Box::pin(slot.notified());
                waiter.as_mut().enable();
                waiter
            })
            .collect();

        let subscriptions_changed = self.room_subscriptions_changed(user_id, device_id, conn_id, &request).await;
        let mut lists_response = self
            .build_lists_response(user_id, device_id, conn_id, &request.lists, request.pos.as_deref())
            .await
            .map_err(|e| ApiError::internal_with_context("Failed to build lists response", &e))?;

        let mut rooms_response = self
            .build_rooms_response(user_id, device_id, conn_id, &request, prev_event_stream_pos, subscriptions_changed)
            .await
            .map_err(|e| ApiError::internal_with_context("Failed to build rooms response", &e))?;

        let mut extensions_response = self
            .build_extensions_response(
                user_id,
                device_id,
                conn_id,
                request.pos.as_deref(),
                &rooms_response,
                request.extensions.as_ref(),
            )
            .await
            .map_err(|e| ApiError::internal_with_context("Failed to build extensions response", &e))?;

        // ── Long-poll / backpressure (self-excitation loop-breaker) ───────────
        // A Matrix sliding-sync client keeps one connection open and expects
        // the SERVER to block until there is new data (or `timeout` elapses);
        // matrix-js-sdk only backs off on 5xx/429, never on an empty success.
        // synapse-rust used to answer immediately every time, so an idle client
        // re-issued its sync as fast as the network allowed (~10 req/s) — the
        // driver, alongside the presence echo, of the sync↔presence
        // self-excitation observed in production.
        //
        // Now an incremental sync with nothing to deliver parks on the event
        // notifier until either an event lands for this user / one of their
        // rooms, or `timeout` (capped at 30s) elapses. Idle traffic drops from
        // ~10 req/s to ~1 request per timeout, while delivery latency stays at
        // one round trip because the waiter is woken the instant an event is
        // written.
        // A sliding sync is idle (eligible for long-poll backpressure) when this
        // is an incremental request, carries no new extensions/account-data, and
        // the list membership did not change. We deliberately do NOT require
        // `rooms_response` to be empty: a client that subscribes to a list always
        // receives room summaries on every sync. Those are static or
        // already-seen and must not defeat the long-poll — genuinely new data is
        // instead signalled by the event notifier, which wakes this request the
        // instant an event lands for this user or one of their rooms (see the
        // `tokio::select!` below). Requiring an empty `rooms_response` here was
        // the bug that let every real (list-using) client busy-loop, because the
        // room summaries meant `rooms_response` was never empty.
        //
        // S14 例外：增量 timeline 现在只含水位线之后的新事件。非空 timeline
        // 意味着有「客户端两次请求之间落库」的新事件 —— 它们先于本请求写入，
        // 通知早已发出（当时无 waiter），park 不会被唤醒。若仍按空闲处理，
        // 超时分支会丢弃 rooms_response 并把水位线回写过这些事件，造成
        // 客户端永久丢消息。因此带新事件的增量响应必须立即返回。
        let is_idle = !is_initial
            && !subscriptions_changed
            && !Self::has_new_extension_data(extensions_response.as_ref())
            && !Self::has_list_operations(&lists_response)
            && !Self::has_new_timeline_events(&rooms_response);

        // Time spent parked. Reported back to `sync()` so the latency metric
        // measures real work only — otherwise every idle long-poll would count
        // as a "slow request" and the performance rollback gate would fire
        // continuously on a healthy server.
        let mut idle_wait_ms = 0u64;

        if is_idle {
            let wait = std::time::Duration::from_millis(
                request
                    .timeout
                    .map(|t| (t as u64).min(MAX_SLIDING_SYNC_IDLE_WAIT_MS))
                    .unwrap_or(DEFAULT_SLIDING_SYNC_IDLE_WAIT_MS),
            );
            let parked_at = std::time::Instant::now();
            let mut woken_by_event = false;

            if long_poll_waiters.is_empty() {
                // No notifier wired (tests, benchmarks): degrade to a plain
                // timed wait. Still breaks the busy-loop, but new events are
                // only picked up on the next poll.
                tokio::time::sleep(wait).await;
            } else {
                tokio::select! {
                    _ = futures::future::select_all(long_poll_waiters.iter_mut()) => {
                        woken_by_event = true;
                        tracing::trace!(
                            user_id = %user_id,
                            device_id = %device_id,
                            "Sliding sync long-poll woken by event notification"
                        );
                    }
                    _ = tokio::time::sleep(wait) => {}
                }
            }

            idle_wait_ms = parked_at.elapsed().as_millis() as u64;

            // A notification means something was persisted for this user, but
            // the response above was built *before* we parked and is empty.
            // Rebuild it so the event ships in THIS response; returning the
            // stale empty one would force the client into a second round trip
            // for data the server already has. On a timeout there is by
            // definition nothing new, so the empty response stands.
            if woken_by_event {
                lists_response = self
                    .build_lists_response(user_id, device_id, conn_id, &request.lists, request.pos.as_deref())
                    .await
                    .map_err(|e| ApiError::internal_with_context("Failed to rebuild lists response", &e))?;

                rooms_response = self
                    .build_rooms_response(
                        user_id,
                        device_id,
                        conn_id,
                        &request,
                        prev_event_stream_pos,
                        subscriptions_changed,
                    )
                    .await
                    .map_err(|e| ApiError::internal_with_context("Failed to rebuild rooms response", &e))?;

                extensions_response = self
                    .build_extensions_response(
                        user_id,
                        device_id,
                        conn_id,
                        request.pos.as_deref(),
                        &rooms_response,
                        request.extensions.as_ref(),
                    )
                    .await
                    .map_err(|e| ApiError::internal_with_context("Failed to rebuild extensions response", &e))?;
            } else {
                // Timed out without a wake-up: by definition there is no new data.
                // The response built before parking carries room summaries plus the
                // most-recent N timeline events, which the client already received
                // on its previous sync. Drop the rooms payload so the idle
                // incremental response is genuinely "no new data" (correct Matrix
                // incremental-sync semantics), preventing the client from
                // re-processing already-seen events and keeping the long-poll
                // meaningful. The notifier already handled the common case where a
                // real event lands during the wait (it wakes us above).
                rooms_response = serde_json::Value::Object(serde_json::Map::new());
            }
            tracing::debug!(
                user_id = %user_id,
                device_id = %device_id,
                idle_wait_ms = idle_wait_ms,
                "Sliding sync idle long-poll finished"
            );
        }

        let new_token = self
            .storage
            .create_or_update_token(user_id, device_id, conn_id, stream_snapshot)
            .await
            .map_err(|e| ApiError::internal_with_context("Failed to update token", &e))?;

        Ok((
            SlidingSyncResponse {
                pos: new_token.pos.to_string(),
                conn_id: request.conn_id,
                lists: lists_response,
                rooms: rooms_response,
                extensions: extensions_response,
            },
            idle_wait_ms,
        ))
    }

    /// Build the connection tracker key from (user_id, device_id, conn_id).
    fn connection_tracker_key(user_id: &str, device_id: &str, conn_id: Option<&str>) -> String {
        match conn_id {
            Some(cid) => format!("{user_id}:{device_id}:{cid}"),
            None => format!("{user_id}:{device_id}:"),
        }
    }

    /// Returns true if any list in a sliding-sync lists response carries a
    /// non-empty `ops` array, i.e. there is new list data to deliver.
    /// Used by the idle long-poll check to decide whether a sync actually has
    /// something to return (vs. being empty and eligible for backpressure).
    fn has_list_operations(lists: &serde_json::Value) -> bool {
        lists
            .as_object()
            .map(|obj| {
                obj.values()
                    .any(|list| list.get("ops").and_then(|ops| ops.as_array()).map(|a| !a.is_empty()).unwrap_or(false))
            })
            .unwrap_or(false)
    }

    /// S14: Returns true when any room in the rooms response carries a
    /// non-empty `timeline` array. Post-S14 incremental timelines only contain
    /// events newer than the client's watermark, so a non-empty timeline means
    /// genuinely new, undelivered data. Such a response must be returned
    /// immediately — parking on the notifier would never wake (the events were
    /// persisted before this request registered its waiters), and the timeout
    /// branch would drop the rooms payload while the token write-back advances
    /// the watermark past those events, permanently losing them for the client.
    fn has_new_timeline_events(rooms: &serde_json::Value) -> bool {
        rooms
            .as_object()
            .map(|obj| {
                obj.values().any(|room| {
                    room.get("timeline")
                        .and_then(|timeline| timeline.as_array())
                        .map(|a| !a.is_empty())
                        .unwrap_or(false)
                })
            })
            .unwrap_or(false)
    }

    /// 判断 extensions 响应里是否有「实际新数据」，用于 is_idle 判定。
    ///
    /// to_device / e2ee / account_data / typing / receipts 每次增量 sync 都会
    /// 回显游标或空结构（如 `{"events":[],"next_batch":"..."}`、
    /// `device_one_time_keys_count` 空对象、`{"rooms":{}}`），若把它们当成
    /// 「有数据」，`extensions_response.is_none()` 永假 → is_idle 永假 → 长轮询
    /// 失效 → 客户端以网络允许的速度忙循环（实测 ~8 req/s）。因此这里只认
    /// 「真正要交付给客户端的新内容」。
    fn has_new_extension_data(extensions: Option<&serde_json::Value>) -> bool {
        let Some(ext) = extensions else { return false };
        let Some(obj) = ext.as_object() else { return false };

        // to_device：`events` 数组非空才是有新事件（`next_batch` 游标回显不算）。
        if let Some(events) = obj.get("to_device").and_then(|td| td.get("events")).and_then(|e| e.as_array()) {
            if !events.is_empty() {
                return true;
            }
        }

        // e2ee：`device_lists.changed` / `device_lists.left` 非空才算新数据，
        // `device_one_time_keys_count` 空对象回显不算。
        if let Some(dl) = obj.get("e2ee").and_then(|e| e.get("device_lists")) {
            if dl.get("changed").and_then(|c| c.as_array()).is_some_and(|a| !a.is_empty()) {
                return true;
            }
            if dl.get("left").and_then(|l| l.as_array()).is_some_and(|a| !a.is_empty()) {
                return true;
            }
        }

        // account_data：`global` 事件数组非空 或 `rooms` 对象非空才算新数据。
        if let Some(ad) = obj.get("account_data") {
            if ad.get("global").and_then(|g| g.as_array()).is_some_and(|a| !a.is_empty()) {
                return true;
            }
            if ad.get("rooms").and_then(|r| r.as_object()).is_some_and(|o| !o.is_empty()) {
                return true;
            }
        }

        // receipts / typing：需要判断是否是「新数据」vs「回显空结构」。
        // typing：所有 rooms 的 user_ids 数组均为空 → 不算新数据。
        // receipts：已在 build_extensions_response 中做缓存去重，
        // 仅当 payload 变化时才 insert，此处检查 rooms 是否存在且非空。
        if let Some(t) = obj.get("typing") {
            if let Some(rooms) = t.get("rooms").and_then(|r| r.as_object()) {
                let has_typing = rooms
                    .values()
                    .any(|room| room.get("user_ids").and_then(|u| u.as_array()).is_some_and(|a| !a.is_empty()));
                if has_typing {
                    return true;
                }
            }
        }

        if let Some(rc) = obj.get("receipts") {
            // Receipts present with non-empty rooms means payload changed
            // (dedup in builder only inserts when changed).
            // Note: response_extensions is initialized from request extensions
            // which always contains {"enabled": true}, so just checking key
            // presence is insufficient — must check rooms content.
            if let Some(rooms) = rc.get("rooms").and_then(|r| r.as_object()) {
                if !rooms.is_empty() {
                    return true;
                }
            }
        }

        // presence：已通过变化去重（changed 才 insert），`events` 非空即新数据。
        if let Some(events) = obj.get("presence").and_then(|p| p.get("events")).and_then(|e| e.as_array()) {
            if !events.is_empty() {
                return true;
            }
        }

        false
    }

    /// P1-5: 检测 room_subscriptions 配置是否变化（新订阅房间、required_state 增减、
    /// timeline_limit 调整）。客户端每次 sync 都发完整的 room_subscriptions，配置一旦
    /// 变化应立即反映，而非被 is_idle 判定为空闲后超时丢弃 rooms_response（否则新订阅
    /// 的房间要等到下一个事件才出现在响应里）。用本地缓存存上一轮快照做对比。
    async fn room_subscriptions_changed(
        &self,
        user_id: &str,
        device_id: &str,
        conn_id: Option<&str>,
        request: &SlidingSyncRequest,
    ) -> bool {
        let key = Self::subscription_snapshot_key(user_id, device_id, conn_id);
        let current = request.room_subscriptions.as_ref().map(|s| s.to_string()).unwrap_or_default();
        let changed = self.cache.get_raw(&key).as_deref() != Some(current.as_str());
        self.cache.set_raw(&key, &current, 3600).await;
        changed
    }

    fn subscription_snapshot_key(user_id: &str, device_id: &str, conn_id: Option<&str>) -> String {
        format!("sliding_sync:subs:{user_id}:{device_id}:{}", conn_id.unwrap_or(""))
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
    ///
    /// W7+ 缓存治理：本函数此前只覆盖 3 个前缀，**漏掉 4 个 key**——
    /// extensions 的去重缓存（presence / account_data / receipts）和
    /// `e2ee:shared_users` 的前缀与已覆盖的三个都不匹配，连接过期后
    /// 只能等 TTL 自然过期，移动端频繁重连会持续产生孤儿 key。
    ///
    /// 剩余局限（已知，接受）：L2 里前缀下的 key 无法枚举——
    /// `get_keys_with_prefix` 只扫 L1，而 Redis 没有廉价的前缀删除原语，
    /// 只能靠各自 TTL 兜底。精确 key（第二类）用完整 key 调 `delete`，
    /// L1 + Redis + 跨实例广播三层都清，无此局限。
    ///
    /// 新增 per-connection 缓存键时**必须**同步登记到下面的清单，
    /// `tests::test_invalidate_connection_cache_covers_all_keys` 会锁住。
    async fn invalidate_connection_cache(&self, user_id: &str, device_id: &str, conn_id: Option<&str>) {
        // ── 前缀类：一个前缀下可能挂多个 key（如每个 list / room 一条） ──
        //
        // 旧实现：for prefix { for key { delete(key) } } —— N×M 次串行 RTT。
        // 新实现：每个 prefix 的 key 收集到 Vec，用 delete_batch 一次 PIPELINE DEL。
        //         4N 次 RTT → 3 次 RTT（prefix_scanner + 3×pipeline）。
        let prefixes = [
            Self::list_snapshot_cache_key_prefix(user_id, device_id, conn_id),
            Self::e2ee_device_list_stream_cache_key_prefix(user_id, device_id, conn_id),
            Self::room_cache_key_prefix(user_id, device_id, conn_id),
        ];

        for prefix in prefixes {
            let keys = self.cache.get_keys_with_prefix(&prefix);
            if !keys.is_empty() {
                self.cache.delete_batch(&keys).await;
            }
        }

        // ── 精确 key 类：extensions 去重缓存，一个连接固定一条 ──
        //
        // 旧实现：for key { delete(key) } —— 4 次串行 RTT。
        // 新实现：futures::future::join_all 并发发出 4 个 delete，
        //         延迟重叠，总耗时 ≈ max(各 RTT) 而非 sum(各 RTT)。
        let exact_keys = [
            Self::presence_cache_key(user_id, device_id, conn_id),
            Self::account_data_cache_key(user_id, device_id, conn_id),
            Self::receipts_cache_key(user_id, device_id, conn_id),
            Self::e2ee_shared_users_cache_key(user_id, device_id, conn_id),
        ];

        // 4 个精确 key 并发删除（各自 L1 + Redis + 广播，独立不变）。
        // 提前 borrow cache 以让闭包不捕获整个 &self——可满足 Send + 'static。
        let cache: &CacheManager = &self.cache;
        let deletes: Vec<_> = exact_keys.into_iter().map(|key| async move { cache.delete(&key).await }).collect();
        futures::future::join_all(deletes).await;
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
