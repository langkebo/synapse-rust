//! AS Event Scheduler — automatic push pipeline for Application Services.
//!
//! Reference: [element-hq/synapse](https://github.com/element-hq/synapse) v1.153.0
//!   - `ApplicationServiceScheduler`
//!   - `_ServiceQueuer`
//!   - `_TransactionController`
//!   - `_Recoverer`
//!
//! This module provides the scheduling layer that bridges raw event ingestion
//! with per-AS delivery. When events are created, `enqueue_event` fans them out
//! to matching AS instances. A background ticker periodically drains queued
//! events into transactions and delivers them, with per-AS concurrency control
//! and exponential-backoff recovery.

use std::collections::HashMap;
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::sync::Mutex;
use tokio::time::MissedTickBehavior;
use tracing::{debug, info, warn};

use crate::application_service::ApplicationServiceManager;

/// Maximum events per transaction (Synapse default: 100).
const MAX_EVENTS_PER_TRANSACTION: usize = 100;

/// How often the scheduler ticks to flush queues (millis).
const SCHEDULER_TICK_INTERVAL_MS: u64 = 500;

/// Maximum number of consecutive retries before disabling a service.
const MAX_CONSECUTIVE_RETRIES: u32 = 10;

/// Initial backoff duration when a delivery fails (Synapse: 2s).
const RECOVERY_INITIAL_BACKOFF_MS: u64 = 2_000;

/// Maximum backoff duration (Synapse: 1h).
const RECOVERY_MAX_BACKOFF_MS: u64 = 3_600_000;

// ── Recoverer ────────────────────────────────────────────────────────────────

/// Per-AS exponential-backoff recoverer.
///
/// Tracks the last failure timestamp and computes how long the scheduler
/// should wait before retrying delivery to a given application service.
/// Modeled after Synapse's `_Recoverer`.
#[derive(Debug, Clone)]
struct Recoverer {
    /// When the last failure occurred (system monotonic time).
    last_failure: Instant,
    /// Current backoff duration, doubles on each consecutive failure.
    backoff: Duration,
    /// Maximum backoff cap.
    max_backoff: Duration,
}

impl Recoverer {
    fn new() -> Self {
        Self {
            last_failure: Instant::now(),
            backoff: Duration::from_millis(RECOVERY_INITIAL_BACKOFF_MS),
            max_backoff: Duration::from_millis(RECOVERY_MAX_BACKOFF_MS),
        }
    }

    /// Record a failure and increase the backoff (exponential, capped).
    fn record_failure(&mut self) {
        self.last_failure = Instant::now();
        self.backoff = (self.backoff * 2).min(self.max_backoff);
    }

    /// Record a successful delivery — reset backoff to initial.
    fn record_success(&mut self) {
        self.backoff = Duration::from_millis(RECOVERY_INITIAL_BACKOFF_MS);
    }

    /// Returns `true` when enough time has elapsed since the last failure
    /// to warrant another delivery attempt.
    fn is_ready(&self) -> bool {
        self.last_failure.elapsed() >= self.backoff
    }
}

// ── Public API ──────────────────────────────────────────────────────────────

/// Schedules automatic event delivery for all active application services.
///
/// Call [`ApplicationServiceScheduler::start`] once during server startup.
/// The scheduler runs a background tick loop that periodically drains per-AS
/// event queues into transactions and delivers them via HTTP.
///
/// Features:
/// - Per-AS concurrency control (one delivery at a time per service).
/// - Exponential-backoff recovery on delivery failure.
/// - Automatic disable after `MAX_CONSECUTIVE_RETRIES` consecutive failures.
pub struct ApplicationServiceScheduler {
    manager: Arc<ApplicationServiceManager>,

    /// Per-AS concurrency guard: true while a delivery is in flight.
    requests_in_flight: Mutex<HashMap<String, bool>>,

    /// Per-AS consecutive failure counter (for disable-after-N logic).
    consecutive_failures: Mutex<HashMap<String, u32>>,

    /// Per-AS exponential-backoff recoverer.
    recoverers: Mutex<HashMap<String, Recoverer>>,

    /// Per-AS disabled flag (disabled after too many consecutive failures).
    disabled: Mutex<HashMap<String, bool>>,

    /// How many events to batch into one transaction.
    max_events_per_txn: usize,
}

impl ApplicationServiceScheduler {
    /// Create a new scheduler backed by the given [`ApplicationServiceManager`].
    pub fn new(manager: Arc<ApplicationServiceManager>) -> Self {
        Self {
            manager,
            requests_in_flight: Mutex::new(HashMap::new()),
            consecutive_failures: Mutex::new(HashMap::new()),
            recoverers: Mutex::new(HashMap::new()),
            disabled: Mutex::new(HashMap::new()),
            max_events_per_txn: MAX_EVENTS_PER_TRANSACTION,
        }
    }

    /// Start the background tick loop. Returns immediately; the loop runs on a
    /// spawned `tokio` task.
    ///
    /// The ticker uses `MissedTickBehavior::Delay` so that backpressure
    /// (slow I/O) does not cause bursts.
    pub fn start(self: Arc<Self>) {
        tokio::spawn(async move {
            let mut interval = tokio::time::interval(Duration::from_millis(SCHEDULER_TICK_INTERVAL_MS));
            interval.set_missed_tick_behavior(MissedTickBehavior::Delay);

            loop {
                interval.tick().await;
                if let Err(e) = self.tick().await {
                    warn!(error = %e, "AS scheduler tick failed");
                }
            }
        });

        info!(
            tick_interval_ms = SCHEDULER_TICK_INTERVAL_MS,
            max_events_per_txn = MAX_EVENTS_PER_TRANSACTION,
            recovery_initial_backoff_ms = RECOVERY_INITIAL_BACKOFF_MS,
            recovery_max_backoff_ms = RECOVERY_MAX_BACKOFF_MS,
            max_consecutive_retries = MAX_CONSECUTIVE_RETRIES,
            "AS scheduler started"
        );
    }

    // ── Tick logic ──────────────────────────────────────────────────────

    async fn tick(&self) -> Result<(), String> {
        let active_services =
            self.manager.get_all_active().await.map_err(|e| format!("Failed to list active AS: {e}"))?;

        for service in &active_services {
            // Skip disabled services.
            if self.is_disabled(&service.as_id).await {
                continue;
            }

            // Concurrency guard: only one delivery at a time per AS.
            if self.is_request_in_flight(&service.as_id).await {
                continue;
            }

            // Exponential-backoff check: only retry after the backoff period.
            if !self.is_recoverer_ready(&service.as_id).await {
                continue;
            }

            // Mark in-flight.
            self.set_request_in_flight(&service.as_id, true).await;

            let start = Instant::now();
            let result = self.manager.process_pending_for_service(&service.as_id, self.max_events_per_txn as i64).await;

            // Clear in-flight flag.
            self.set_request_in_flight(&service.as_id, false).await;

            match result {
                Ok(dispatched) => {
                    if dispatched > 0 {
                        debug!(
                            as_id = %service.as_id,
                            dispatched,
                            elapsed_ms = start.elapsed().as_millis(),
                            "AS transaction delivered"
                        );
                    }
                    self.reset_consecutive_failures(&service.as_id).await;
                    self.recoverer_record_success(&service.as_id).await;
                }
                Err(e) => {
                    let failures = self.record_failure(&service.as_id).await;
                    self.recoverer_record_failure(&service.as_id).await;
                    warn!(
                        %e,
                        as_id = %service.as_id,
                        consecutive_failures = failures,
                        "AS delivery failed"
                    );

                    if failures >= MAX_CONSECUTIVE_RETRIES {
                        warn!(
                            as_id = %service.as_id,
                            max_retries = MAX_CONSECUTIVE_RETRIES,
                            "Disabling AS after consecutive failures"
                        );
                        self.set_disabled(&service.as_id, true).await;
                    }
                }
            }
        }

        Ok(())
    }

    // ── Concurrency helpers ─────────────────────────────────────────────

    async fn is_request_in_flight(&self, as_id: &str) -> bool {
        self.requests_in_flight.lock().await.get(as_id).copied().unwrap_or(false)
    }

    async fn set_request_in_flight(&self, as_id: &str, inflight: bool) {
        let mut map = self.requests_in_flight.lock().await;
        if inflight {
            map.insert(as_id.to_string(), true);
        } else {
            map.remove(as_id);
        }
    }

    // ── Disabled tracking ───────────────────────────────────────────────

    async fn is_disabled(&self, as_id: &str) -> bool {
        self.disabled.lock().await.get(as_id).copied().unwrap_or(false)
    }

    async fn set_disabled(&self, as_id: &str, disabled: bool) {
        if disabled {
            self.disabled.lock().await.insert(as_id.to_string(), true);
        } else {
            self.disabled.lock().await.remove(as_id);
        }
    }

    // ── Failure tracking ────────────────────────────────────────────────

    async fn record_failure(&self, as_id: &str) -> u32 {
        let mut map = self.consecutive_failures.lock().await;
        let count = map.entry(as_id.to_string()).or_insert(0);
        *count += 1;
        *count
    }

    async fn reset_consecutive_failures(&self, as_id: &str) {
        self.consecutive_failures.lock().await.remove(as_id);
    }

    // ── Recoverer (exponential backoff) ─────────────────────────────────

    async fn recoverer_record_failure(&self, as_id: &str) {
        self.recoverers.lock().await.entry(as_id.to_string()).or_insert_with(Recoverer::new).record_failure();
    }

    async fn recoverer_record_success(&self, as_id: &str) {
        if let Some(r) = self.recoverers.lock().await.get_mut(as_id) {
            r.record_success();
        }
    }

    async fn is_recoverer_ready(&self, as_id: &str) -> bool {
        self.recoverers.lock().await.get(as_id).map(|r| r.is_ready()).unwrap_or(true)
    }
}

// ── Tests ────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_default_constants() {
        assert_eq!(MAX_EVENTS_PER_TRANSACTION, 100);
        assert_eq!(SCHEDULER_TICK_INTERVAL_MS, 500);
        assert_eq!(MAX_CONSECUTIVE_RETRIES, 10);
        assert_eq!(RECOVERY_INITIAL_BACKOFF_MS, 2_000);
        assert_eq!(RECOVERY_MAX_BACKOFF_MS, 3_600_000);
    }

    #[test]
    fn test_recoverer_backoff_grows_exponentially() {
        let mut r = Recoverer::new();
        assert_eq!(r.backoff, Duration::from_millis(2_000)); // initial

        r.record_failure();
        assert_eq!(r.backoff, Duration::from_millis(4_000));

        r.record_failure();
        assert_eq!(r.backoff, Duration::from_millis(8_000));

        r.record_success();
        assert_eq!(r.backoff, Duration::from_millis(2_000)); // reset
    }

    #[test]
    fn test_recoverer_backoff_capped() {
        let mut r = Recoverer::new();
        // Record enough failures to exceed the max.
        for _ in 0..20 {
            r.record_failure();
        }
        assert_eq!(r.backoff, Duration::from_millis(RECOVERY_MAX_BACKOFF_MS));
    }

    #[test]
    fn test_recoverer_ready_after_backoff() {
        let mut r = Recoverer::new();
        // Simulate a failure in the past.
        r.last_failure = Instant::now() - Duration::from_millis(RECOVERY_INITIAL_BACKOFF_MS + 100);
        assert!(r.is_ready());

        // Simulate a recent failure.
        r.last_failure = Instant::now();
        assert!(!r.is_ready());
    }
}