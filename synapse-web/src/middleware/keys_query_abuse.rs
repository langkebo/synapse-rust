//! Per-user `/keys/query` rate limiting + abuse monitoring (Sprint 6 / sec-4).
//!
//! `/keys/query` is the single most database-amplifying endpoint in the
//! client-server API: a single call can fan out to fetch keys for
//! every device of every requested user. The IP-based rate limit
//! already mounted on the global middleware is not enough — many
//! deployments NAT every client through the same egress, so a single
//! IP is not a useful abuse signal.
//!
//! This module adds two related mechanisms:
//!
//! 1. **Per-user token bucket** — keyed by `user_id`, not IP. The
//!    budget is `keys_query_per_user.{per_second, burst_size}` from
//!    the rate-limit config. The bucket is held in the same shared
//!    `CacheManager` (Redis when available, in-process map otherwise)
//!    so the limit is consistent across worker threads.
//!
//! 2. **Sliding-window abuse monitor** — a per-user sliding window
//!    of the last 60 seconds. If a user crosses the configured
//!    `keys_query_abuse_alert_per_min` threshold we emit a structured
//!    `security_audit` event and bump the `keys_query_total{result="abuse_alert"}`
//!    counter. The window is in-process (a `parking_lot::Mutex<HashMap>`)
//!    because (a) the per-window state is small and bounded by
//!    active-user count, and (b) we do not want a hot user's
//!    record to be silently dropped when its key evicts from Redis.
//!
//! The two mechanisms are complementary: the per-user token bucket
//! stops an in-flight burst, the abuse monitor is a forensics +
//! alerting signal that fires *before* the bucket trips.

use std::collections::{HashMap, VecDeque};
use std::sync::Arc;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use parking_lot::Mutex;

use synapse_cache::CacheManager;
use synapse_common::rate_limit_config::RateLimitConfigFile;
use synapse_common::server_metrics::ServerMetrics;

const WINDOW_SECONDS: u64 = 60;
const MAX_TRACKED_USERS: usize = 50_000;

fn unix_ms_now() -> u64 {
    SystemTime::now().duration_since(UNIX_EPOCH).map(|d| d.as_millis() as u64).unwrap_or(0)
}

/// Outcome of `check_and_record` for a single /keys/query call.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum KeysQueryDecision {
    /// Request is allowed; the per-user bucket had capacity and
    /// the per-minute alert threshold was not crossed.
    Allowed,
    /// Per-user token bucket is empty; the caller must back off.
    /// We return 429 to the client.
    RateLimited,
    /// Per-user token bucket had capacity, but the per-minute count
    /// crossed `keys_query_abuse_alert_per_min`. We let the request
    /// through (it's not yet abusive by the hard limit) but flag it
    /// for security review.
    AbuseAlert,
}

/// Sliding-window call record for a single user. We keep the last
/// 60s of timestamps; the deque is bounded by the per-user rate
/// limit × 60s plus headroom.
#[derive(Debug, Default)]
struct UserWindow {
    /// Millisecond timestamps of recent calls, oldest first.
    calls: VecDeque<u64>,
}

impl UserWindow {
    fn push(&mut self, ts_ms: u64) {
        self.calls.push_back(ts_ms);
        self.evict(ts_ms);
    }

    /// Drop entries older than `WINDOW_SECONDS` from the head of the deque.
    fn evict(&mut self, now_ms: u64) {
        let cutoff = now_ms.saturating_sub(WINDOW_SECONDS * 1000);
        while let Some(&front) = self.calls.front() {
            if front < cutoff {
                self.calls.pop_front();
            } else {
                break;
            }
        }
    }

    fn count_in_window(&mut self, now_ms: u64) -> usize {
        self.evict(now_ms);
        self.calls.len()
    }
}

/// Top-line snapshot emitted for dashboards and security review.
#[derive(Debug, Clone)]
pub struct KeysQueryWindowSnapshot {
    pub unique_users: u64,
    pub top_user: Option<String>,
    pub top_user_calls: u64,
}

/// In-process abuse monitor. Cheap to construct (no I/O) and safe to
/// share via `Arc`.
pub struct KeysQueryAbuseMonitor {
    cache: Arc<CacheManager>,
    metrics: Arc<ServerMetrics>,
    /// `user_id` → sliding window of recent call timestamps.
    windows: Mutex<HashMap<String, UserWindow>>,
    redis_prefix: String,
}

impl KeysQueryAbuseMonitor {
    pub fn new(cache: Arc<CacheManager>, metrics: Arc<ServerMetrics>, redis_prefix: String) -> Self {
        Self { cache, metrics, windows: Mutex::new(HashMap::new()), redis_prefix }
    }

    /// Run the per-user token bucket check (Redis when available,
    /// in-process map otherwise) and update the abuse window.
    /// Returns the decision the caller should act on.
    pub async fn check_and_record(
        &self,
        user_id: &str,
        config: &RateLimitConfigFile,
    ) -> KeysQueryDecision {
        if !config.keys_query_per_user.enabled {
            // Per-user limit disabled: just record and return.
            self.record_window(user_id, false, config.keys_query_abuse_alert_per_min);
            return KeysQueryDecision::Allowed;
        }

        let cache_key = format!(
            "{}{}",
            self.redis_prefix,
            synapse_cache::CacheKeyBuilder::rate_limit(user_id, "keys_query")
        );

        let decision = self
            .cache
            .rate_limit_token_bucket_take(
                &cache_key,
                config.keys_query_per_user.per_second,
                config.keys_query_per_user.burst_size,
            )
            .await;

        match decision {
            Ok(d) if !d.allowed => {
                self.metrics.record_keys_query_outcome("rate_limited");
                // Window is still updated so forensics can see the
                // sustained pressure even after the bucket trips.
                self.record_window(user_id, true, config.keys_query_abuse_alert_per_min);
                KeysQueryDecision::RateLimited
            }
            Ok(_) => {
                let alert = self.record_window(user_id, false, config.keys_query_abuse_alert_per_min);
                if alert {
                    self.metrics.record_keys_query_outcome("abuse_alert");
                    KeysQueryDecision::AbuseAlert
                } else {
                    self.metrics.record_keys_query_outcome("allowed");
                    KeysQueryDecision::Allowed
                }
            }
            Err(e) => {
                // Fail open on a Redis-only error: the in-process
                // bucket would have caught the burst anyway, and a
                // cache outage should not deny legitimate clients.
                tracing::warn!(
                    target: "security_audit",
                    event = "keys_query_rate_limit_error",
                    user_id = %user_id,
                    error = %e,
                    "Per-user rate limit lookup failed; allowing request"
                );
                let alert = self.record_window(user_id, false, config.keys_query_abuse_alert_per_min);
                if alert {
                    self.metrics.record_keys_query_outcome("abuse_alert");
                    KeysQueryDecision::AbuseAlert
                } else {
                    self.metrics.record_keys_query_outcome("allowed");
                    KeysQueryDecision::Allowed
                }
            }
        }
    }

    /// Push a timestamp into the user's window, evict expired
    /// entries, and (a) log a security audit event if the window
    /// crossed the alert threshold and (b) return `true` to the
    /// caller when that happens.
    ///
    /// `bucket_tripped` distinguishes "user was just rate-limited
    /// (so the alert is implicit)" from "user is approaching the
    /// limit" — we only emit an `abuse_alert` in the second case.
    /// `alert_threshold` is `config.keys_query_abuse_alert_per_min`
    /// (calls per 60s) — we alert when the count first crosses it
    /// in a given window.
    fn record_window(&self, user_id: &str, bucket_tripped: bool, alert_threshold: u32) -> bool {
        if bucket_tripped || alert_threshold == 0 {
            // Still record the timestamp so forensics can see the
            // volume even when the alert is suppressed.
            let now_ms = unix_ms_now();
            let mut windows = self.windows.lock();
            self.bounded_insert(&mut windows, user_id);
            let entry = windows.entry(user_id.to_string()).or_default();
            entry.push(now_ms);
            entry.count_in_window(now_ms);
            return false;
        }

        let now_ms = unix_ms_now();
        let mut windows = self.windows.lock();
        self.bounded_insert(&mut windows, user_id);
        let entry = windows.entry(user_id.to_string()).or_default();
        entry.push(now_ms);
        let count = entry.count_in_window(now_ms);

        if count as u32 >= alert_threshold {
            tracing::warn!(
                target: "security_audit",
                event = "keys_query_abuse_alert",
                user_id = %user_id,
                calls_in_last_minute = count,
                threshold = alert_threshold,
                "User exceeded /keys/query alert threshold"
            );
            return true;
        }
        false
    }

    /// Bounded insert: if the map has grown beyond `MAX_TRACKED_USERS`
    /// we drop the entry with the smallest count. This keeps the
    /// monitor from being an unbounded memory hazard under a
    /// username-spray DoS — the *hottest* users (the ones we most
    /// care about for forensics) are the ones that survive.
    fn bounded_insert(&self, windows: &mut HashMap<String, UserWindow>, user_id: &str) {
        if windows.contains_key(user_id) || windows.len() < MAX_TRACKED_USERS {
            return;
        }
        if let Some(victim) = windows.iter().min_by_key(|(_, w)| w.calls.len()).map(|(k, _)| k.clone()) {
            windows.remove(&victim);
        }
    }

    /// Compute a snapshot of the current window for the metrics
    /// endpoint. Cheap; takes the same lock `record_window` uses.
    pub fn snapshot(&self) -> KeysQueryWindowSnapshot {
        let now_ms = unix_ms_now();
        let mut windows = self.windows.lock();
        let mut top_user: Option<String> = None;
        let mut top_calls: u64 = 0;
        let mut total_users: u64 = 0;
        for (user_id, window) in windows.iter_mut() {
            let count = window.count_in_window(now_ms) as u64;
            if count == 0 {
                continue;
            }
            total_users += 1;
            if count > top_calls {
                top_calls = count;
                top_user = Some(user_id.clone());
            }
        }
        // Drop fully-evicted users so the map doesn't grow without
        // bound across long uptime.
        windows.retain(|_, w| !w.calls.is_empty());

        self.metrics.set_keys_query_window_stats(total_users, top_calls);
        KeysQueryWindowSnapshot { unique_users: total_users, top_user, top_user_calls: top_calls }
    }

    /// Run the snapshot on a fixed interval and publish the
    /// gauges. Spawned by `AppState::new` so callers don't have
    /// to know about it.
    pub fn start_snapshot_task(self: Arc<Self>) {
        tokio::spawn(async move {
            let mut tick = tokio::time::interval(Duration::from_secs(15));
            // First tick fires immediately; consume it so the
            // first real sample is at +15s.
            tick.tick().await;
            loop {
                tick.tick().await;
                let _ = self.snapshot();
            }
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use synapse_cache::{CacheConfig, CacheManager};
    use synapse_common::metrics::MetricsCollector;

    fn build() -> (Arc<CacheManager>, Arc<ServerMetrics>, Arc<KeysQueryAbuseMonitor>) {
        let cache = Arc::new(CacheManager::new(&CacheConfig::default()));
        let metrics = Arc::new(ServerMetrics::new(Arc::new(MetricsCollector::new())));
        let monitor = Arc::new(KeysQueryAbuseMonitor::new(
            cache.clone(),
            metrics.clone(),
            "test:".to_string(),
        ));
        (cache, metrics, monitor)
    }

    #[tokio::test]
    async fn first_call_is_allowed() {
        let (_cache, _metrics, monitor) = build();
        let cfg = RateLimitConfigFile::default();
        assert_eq!(monitor.check_and_record("@a:server", &cfg).await, KeysQueryDecision::Allowed);
    }

    #[tokio::test]
    async fn burst_trips_user_bucket() {
        // Use a much smaller burst to make the test deterministic
        // under the async runtime. Default config (burst=20) is
        // borderline because per_second=5 means up to 5 tokens
        // can refill while the loop runs. With burst=2 the third
        // call must hit the bucket.
        let (_cache, _metrics, monitor) = build();
        let cfg = RateLimitConfigFile {
            keys_query_per_user: synapse_common::rate_limit_config::KeysQueryUserRateLimitConfig {
                enabled: true,
                per_second: 1,
                burst_size: 2,
            },
            keys_query_abuse_alert_per_min: 0, // disable alerts for this test
            ..RateLimitConfigFile::default()
        };

        assert_eq!(monitor.check_and_record("@a:server", &cfg).await, KeysQueryDecision::Allowed);
        assert_eq!(monitor.check_and_record("@a:server", &cfg).await, KeysQueryDecision::Allowed);
        // Bucket drained; the 3rd call must hit the rate limit
        // (no refill within the same millisecond).
        assert_eq!(monitor.check_and_record("@a:server", &cfg).await, KeysQueryDecision::RateLimited);
    }

    #[tokio::test]
    async fn distinct_users_have_independent_budgets() {
        let (_cache, _metrics, monitor) = build();
        let cfg = RateLimitConfigFile::default();
        for _ in 0..20 {
            let _ = monitor.check_and_record("@a:server", &cfg).await;
        }
        // `@b` has its own bucket and should be unaffected.
        assert_eq!(monitor.check_and_record("@b:server", &cfg).await, KeysQueryDecision::Allowed);
    }

    #[tokio::test]
    async fn disabled_per_user_config_always_allows() {
        let (_cache, _metrics, monitor) = build();
        let mut cfg = RateLimitConfigFile::default();
        cfg.keys_query_per_user.enabled = false;
        // 50 calls in a tight loop — well above the burst size.
        for _ in 0..50 {
            let _ = monitor.check_and_record("@a:server", &cfg).await;
        }
        assert_eq!(monitor.check_and_record("@a:server", &cfg).await, KeysQueryDecision::Allowed);
    }

    #[test]
    fn sliding_window_evicts_old_entries() {
        let mut w = UserWindow::default();
        w.push(0);
        w.push(10_000);
        // Check at now = (WINDOW_SECONDS * 1000) + 10_001 — both
        // pushed events are strictly older than the cutoff and
        // should be evicted.
        let now = WINDOW_SECONDS * 1000 + 10_001;
        assert_eq!(w.count_in_window(now), 0);
    }

    #[test]
    fn sliding_window_keeps_recent_entries() {
        // Push in increasing timestamp order, as production code
        // does (we always push "now"). Simulate a 30s window by
        // checking at a point after all three pushes.
        let mut w = UserWindow::default();
        let base = 1_000_000;
        w.push(base - 65_000); // 65s before base — outside window
        w.push(base - 30_000); // 30s before base — in window
        w.push(base - 5_000); // 5s before base — in window
        // After all pushes, only the last two should remain when
        // we look at "now = base".
        assert_eq!(w.count_in_window(base), 2);
    }
}
