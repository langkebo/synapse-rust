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

use chrono::Utc;
use std::collections::{HashMap, HashSet};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};
use synapse_storage::application_service::ApplicationService;
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

/// Maximum number of AS instances to actively dispatch in one tick.
const MAX_SERVICES_PER_TICK: usize = 8;

/// Pending event count that marks a service as backlog-heavy.
const HIGH_PENDING_EVENT_THRESHOLD: i64 = 50;

/// Pending transaction count that marks a service as backlog-heavy.
const HIGH_PENDING_TRANSACTION_THRESHOLD: i64 = 2;

pub(crate) const SCHEDULER_STATE_LAST_TICK_TS: &str = "scheduler_last_tick_ts";
pub(crate) const SCHEDULER_STATE_LAST_RESULT: &str = "scheduler_last_result";
pub(crate) const SCHEDULER_STATE_PENDING_EVENT_COUNT: &str = "scheduler_pending_event_count";
pub(crate) const SCHEDULER_STATE_PENDING_TRANSACTION_COUNT: &str = "scheduler_pending_transaction_count";
pub(crate) const SCHEDULER_STATE_BACKLOG_STATE: &str = "scheduler_backlog_state";
pub(crate) const SCHEDULER_STATE_TRANSACTION_STATE: &str = "scheduler_transaction_state";
pub(crate) const SCHEDULER_STATE_LAST_DISPATCHED_EVENTS: &str = "scheduler_last_dispatched_events";
pub(crate) const SCHEDULER_STATE_LAST_ELAPSED_MS: &str = "scheduler_last_elapsed_ms";
pub(crate) const SCHEDULER_STATE_TOTAL_SUCCESS_COUNT: &str = "scheduler_total_success_count";
pub(crate) const SCHEDULER_STATE_TOTAL_FAILURE_COUNT: &str = "scheduler_total_failure_count";
pub(crate) const SCHEDULER_STATE_TOTAL_BACKOFF_COUNT: &str = "scheduler_total_backoff_count";
pub(crate) const SCHEDULER_STATE_TOTAL_CAPACITY_LIMITED_COUNT: &str = "scheduler_total_capacity_limited_count";
pub(crate) const SCHEDULER_STATE_TOTAL_IN_FLIGHT_COUNT: &str = "scheduler_total_in_flight_count";

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
enum DispatchPriority {
    PendingTransaction,
    PendingEvents,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct DispatchCandidate {
    as_id: String,
    priority: DispatchPriority,
    pending_transaction_count: i64,
    pending_event_count: i64,
    has_statistics: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum SchedulerResult {
    Dispatched,
    Idle,
    Backoff,
    CapacityLimited,
    InFlight,
    Disabled,
    Failed,
}

impl SchedulerResult {
    fn as_str(self) -> &'static str {
        match self {
            Self::Dispatched => "dispatched",
            Self::Idle => "idle",
            Self::Backoff => "backoff",
            Self::CapacityLimited => "capacity_limited",
            Self::InFlight => "in_flight",
            Self::Disabled => "disabled",
            Self::Failed => "failed",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum TransactionState {
    Idle,
    PendingEvents,
    PendingTransaction,
    RetryBackoff,
    InFlight,
    CapacityLimited,
    Disabled,
}

impl TransactionState {
    fn as_str(self) -> &'static str {
        match self {
            Self::Idle => "idle",
            Self::PendingEvents => "pending_events",
            Self::PendingTransaction => "pending_transaction",
            Self::RetryBackoff => "retry_backoff",
            Self::InFlight => "in_flight",
            Self::CapacityLimited => "capacity_limited",
            Self::Disabled => "disabled",
        }
    }
}

#[derive(Default)]
struct TransactionController;

impl TransactionController {
    fn sort_candidates(candidates: &mut [DispatchCandidate], start_offset: usize) {
        candidates.sort_by(|left, right| {
            left.priority
                .cmp(&right.priority)
                .then_with(|| right.pending_transaction_count.cmp(&left.pending_transaction_count))
                .then_with(|| right.pending_event_count.cmp(&left.pending_event_count))
                .then_with(|| left.as_id.cmp(&right.as_id))
        });

        if candidates.len() <= 1 {
            return;
        }

        // Preserve priority buckets while still rotating peers inside each bucket.
        // A pending-transaction candidate must never be rotated behind a
        // pending-events-only candidate just because the round-robin cursor moved.
        let mut bucket_start = 0;
        while bucket_start < candidates.len() {
            let bucket_priority = candidates[bucket_start].priority;
            let mut bucket_end = bucket_start + 1;
            while bucket_end < candidates.len() && candidates[bucket_end].priority == bucket_priority {
                bucket_end += 1;
            }

            let bucket_len = bucket_end - bucket_start;
            if bucket_len > 1 {
                let rotate_by = start_offset % bucket_len;
                candidates[bucket_start..bucket_end].rotate_left(rotate_by);
            }

            bucket_start = bucket_end;
        }
    }

    fn plan_dispatch_order(
        &self,
        active_services: &[ApplicationService],
        statistics: &[serde_json::Value],
        start_offset: usize,
    ) -> Vec<DispatchCandidate> {
        let active_service_ids: HashSet<&str> = active_services.iter().map(|service| service.as_id.as_str()).collect();
        let stats_by_as_id: HashMap<&str, (i64, i64)> = statistics
            .iter()
            .filter_map(|entry| {
                let as_id = entry.get("as_id")?.as_str()?;
                if !active_service_ids.contains(as_id) {
                    return None;
                }

                Some((
                    as_id,
                    (
                        entry.get("pending_transaction_count").and_then(|value| value.as_i64()).unwrap_or(0),
                        entry.get("pending_event_count").and_then(|value| value.as_i64()).unwrap_or(0),
                    ),
                ))
            })
            .collect();

        let mut candidates: Vec<DispatchCandidate> = active_services
            .iter()
            .filter_map(|service| {
                let (pending_transaction_count, pending_event_count) =
                    stats_by_as_id.get(service.as_id.as_str()).copied().unwrap_or((0, 0));

                if stats_by_as_id.contains_key(service.as_id.as_str())
                    && pending_transaction_count <= 0
                    && pending_event_count <= 0
                {
                    return None;
                }

                let priority = if pending_transaction_count > 0 {
                    DispatchPriority::PendingTransaction
                } else {
                    DispatchPriority::PendingEvents
                };

                Some(DispatchCandidate {
                    as_id: service.as_id.clone(),
                    priority,
                    pending_transaction_count,
                    pending_event_count,
                    has_statistics: stats_by_as_id.contains_key(service.as_id.as_str()),
                })
            })
            .collect();

        if candidates.is_empty() {
            candidates = active_services
                .iter()
                .map(|service| DispatchCandidate {
                    as_id: service.as_id.clone(),
                    priority: DispatchPriority::PendingEvents,
                    pending_transaction_count: 0,
                    pending_event_count: 0,
                    has_statistics: false,
                })
                .collect();
        }

        Self::sort_candidates(&mut candidates, start_offset);

        candidates
    }
}

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
    controller: TransactionController,

    /// Per-AS concurrency guard: true while a delivery is in flight.
    requests_in_flight: Mutex<HashMap<String, bool>>,

    /// Per-AS consecutive failure counter (for disable-after-N logic).
    consecutive_failures: Mutex<HashMap<String, u32>>,

    /// Per-AS exponential-backoff recoverer.
    recoverers: Mutex<HashMap<String, Recoverer>>,

    /// Per-AS disabled flag (disabled after too many consecutive failures).
    disabled: Mutex<HashMap<String, bool>>,

    /// Per-AS round-robin cursor used when multiple services are ready.
    dispatch_cursor: Mutex<usize>,

    /// How many events to batch into one transaction.
    max_events_per_txn: usize,

    /// Scheduler tick interval in milliseconds.
    tick_interval_ms: u64,

    /// Maximum number of services the scheduler should actively dispatch in one tick.
    max_services_per_tick: usize,

    /// Threshold beyond which pending events are considered backlog-heavy.
    high_pending_event_threshold: i64,

    /// Threshold beyond which pending transactions are considered backlog-heavy.
    high_pending_transaction_threshold: i64,

    /// Prevent duplicate background loops on the same scheduler instance.
    started: AtomicBool,
}

impl ApplicationServiceScheduler {
    /// Create a new scheduler backed by the given [`ApplicationServiceManager`].
    pub fn new(manager: Arc<ApplicationServiceManager>) -> Self {
        Self::with_capacity_options(
            manager,
            MAX_EVENTS_PER_TRANSACTION,
            SCHEDULER_TICK_INTERVAL_MS,
            MAX_SERVICES_PER_TICK,
            HIGH_PENDING_EVENT_THRESHOLD,
            HIGH_PENDING_TRANSACTION_THRESHOLD,
        )
    }

    pub fn with_options(
        manager: Arc<ApplicationServiceManager>,
        max_events_per_txn: usize,
        tick_interval_ms: u64,
    ) -> Self {
        Self::with_capacity_options(
            manager,
            max_events_per_txn,
            tick_interval_ms,
            MAX_SERVICES_PER_TICK,
            HIGH_PENDING_EVENT_THRESHOLD,
            HIGH_PENDING_TRANSACTION_THRESHOLD,
        )
    }

    pub fn with_capacity_options(
        manager: Arc<ApplicationServiceManager>,
        max_events_per_txn: usize,
        tick_interval_ms: u64,
        max_services_per_tick: usize,
        high_pending_event_threshold: i64,
        high_pending_transaction_threshold: i64,
    ) -> Self {
        Self {
            manager,
            controller: TransactionController,
            requests_in_flight: Mutex::new(HashMap::new()),
            consecutive_failures: Mutex::new(HashMap::new()),
            recoverers: Mutex::new(HashMap::new()),
            disabled: Mutex::new(HashMap::new()),
            dispatch_cursor: Mutex::new(0),
            max_events_per_txn: max_events_per_txn.max(1),
            tick_interval_ms: tick_interval_ms.max(1),
            max_services_per_tick: max_services_per_tick.max(1),
            high_pending_event_threshold: high_pending_event_threshold.max(1),
            high_pending_transaction_threshold: high_pending_transaction_threshold.max(1),
            started: AtomicBool::new(false),
        }
    }

    /// Start the background tick loop. Returns immediately; the loop runs on a
    /// spawned `tokio` task.
    ///
    /// The ticker uses `MissedTickBehavior::Delay` so that backpressure
    /// (slow I/O) does not cause bursts.
    pub fn start(self: Arc<Self>) {
        if self.started.swap(true, Ordering::SeqCst) {
            debug!("AS scheduler already started; skipping duplicate start");
            return;
        }

        let tick_interval_ms = self.tick_interval_ms;
        let max_events_per_txn = self.max_events_per_txn;
        let max_services_per_tick = self.max_services_per_tick;
        let scheduler = self.clone();

        tokio::spawn(async move {
            let mut interval = tokio::time::interval(Duration::from_millis(tick_interval_ms));
            interval.set_missed_tick_behavior(MissedTickBehavior::Delay);

            loop {
                interval.tick().await;
                if let Err(e) = scheduler.tick().await {
                    warn!(error = %e, "AS scheduler tick failed");
                }
            }
        });

        info!(
            tick_interval_ms,
            max_events_per_txn,
            max_services_per_tick,
            recovery_initial_backoff_ms = RECOVERY_INITIAL_BACKOFF_MS,
            recovery_max_backoff_ms = RECOVERY_MAX_BACKOFF_MS,
            max_consecutive_retries = MAX_CONSECUTIVE_RETRIES,
            "AS scheduler started"
        );
    }

    pub async fn run_once(&self) -> Result<(), String> {
        self.tick().await
    }

    // ── Tick logic ──────────────────────────────────────────────────────

    async fn tick(&self) -> Result<(), String> {
        let active_services =
            self.manager.get_all_active().await.map_err(|e| format!("Failed to list active AS: {e}"))?;
        let statistics =
            self.manager.get_statistics().await.map_err(|e| format!("Failed to get appservice statistics: {e}"))?;
        let dispatch_order = self.plan_dispatch_order(&active_services, &statistics).await;

        if dispatch_order.is_empty() {
            return Ok(());
        }

        let mut dispatched_services = 0_usize;
        for candidate in dispatch_order {
            let service = if let Some(service) = active_services.iter().find(|service| service.as_id == candidate.as_id)
            {
                service
            } else {
                continue;
            };

            let observed = self.observe_candidate(candidate).await?;

            // Skip disabled services.
            if self.is_disabled(&service.as_id).await {
                self.record_scheduler_observation(&service.as_id, observed, SchedulerResult::Disabled, None, None)
                    .await;
                continue;
            }

            // Concurrency guard: only one delivery at a time per AS.
            if self.is_request_in_flight(&service.as_id).await {
                self.record_scheduler_observation(&service.as_id, observed, SchedulerResult::InFlight, None, None)
                    .await;
                continue;
            }

            // Exponential-backoff check: only retry after the backoff period.
            if !self.is_recoverer_ready(&service.as_id).await {
                self.record_scheduler_observation(&service.as_id, observed, SchedulerResult::Backoff, None, None).await;
                continue;
            }

            if observed.pending_event_count <= 0 && observed.pending_transaction_count <= 0 {
                self.record_scheduler_observation(&service.as_id, observed, SchedulerResult::Idle, Some(0), Some(0))
                    .await;
                continue;
            }

            if dispatched_services >= self.max_services_per_tick {
                self.record_scheduler_observation(
                    &service.as_id,
                    observed,
                    SchedulerResult::CapacityLimited,
                    None,
                    None,
                )
                .await;
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
                    dispatched_services += 1;
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
                    let refreshed = self
                        .observe_candidate(DispatchCandidate {
                            as_id: service.as_id.clone(),
                            priority: observed.priority,
                            pending_transaction_count: observed.pending_transaction_count,
                            pending_event_count: observed.pending_event_count,
                            has_statistics: false,
                        })
                        .await?;
                    self.record_scheduler_observation(
                        &service.as_id,
                        refreshed,
                        SchedulerResult::Dispatched,
                        Some(dispatched as i64),
                        Some(start.elapsed().as_millis() as i64),
                    )
                    .await;
                }
                Err(e) => {
                    dispatched_services += 1;
                    let failures = self.record_failure(&service.as_id).await;
                    self.recoverer_record_failure(&service.as_id).await;
                    warn!(
                        %e,
                        as_id = %service.as_id,
                        consecutive_failures = failures,
                        "AS delivery failed"
                    );
                    let refreshed = self
                        .observe_candidate(DispatchCandidate {
                            as_id: service.as_id.clone(),
                            priority: observed.priority,
                            pending_transaction_count: observed.pending_transaction_count,
                            pending_event_count: observed.pending_event_count,
                            has_statistics: false,
                        })
                        .await?;
                    self.record_scheduler_observation(
                        &service.as_id,
                        refreshed,
                        SchedulerResult::Failed,
                        Some(0),
                        Some(start.elapsed().as_millis() as i64),
                    )
                    .await;

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

    async fn plan_dispatch_order(
        &self,
        active_services: &[ApplicationService],
        statistics: &[serde_json::Value],
    ) -> Vec<DispatchCandidate> {
        let start_offset = {
            let mut cursor = self.dispatch_cursor.lock().await;
            let current = *cursor;
            *cursor = cursor.saturating_add(1);
            current
        };

        let mut candidates = self.controller.plan_dispatch_order(active_services, statistics, start_offset);
        if candidates.iter().all(|candidate| candidate.has_statistics) {
            return candidates;
        }

        let mut observed_candidates = Vec::with_capacity(candidates.len());
        for candidate in candidates.drain(..) {
            match self.observe_candidate(candidate).await {
                Ok(observed) if observed.pending_event_count > 0 || observed.pending_transaction_count > 0 => {
                    observed_candidates.push(DispatchCandidate { has_statistics: true, ..observed });
                }
                Ok(_) => {}
                Err(error) => {
                    warn!(%error, "Failed to observe scheduler candidate before dispatch planning");
                }
            }
        }

        TransactionController::sort_candidates(&mut observed_candidates, start_offset);
        observed_candidates
    }

    async fn observe_candidate(&self, candidate: DispatchCandidate) -> Result<DispatchCandidate, String> {
        if candidate.has_statistics {
            return Ok(candidate);
        }

        let pending_event_count = self
            .manager
            .count_pending_events(&candidate.as_id)
            .await
            .map_err(|e| format!("Failed to count pending appservice events for '{}': {e}", candidate.as_id))?;
        let pending_transaction_count =
            self.manager.count_pending_transactions(&candidate.as_id).await.map_err(|e| {
                format!("Failed to count pending appservice transactions for '{}': {e}", candidate.as_id)
            })?;

        Ok(DispatchCandidate {
            pending_event_count,
            pending_transaction_count,
            has_statistics: false,
            priority: if pending_transaction_count > 0 {
                DispatchPriority::PendingTransaction
            } else {
                DispatchPriority::PendingEvents
            },
            ..candidate
        })
    }

    async fn record_scheduler_observation(
        &self,
        as_id: &str,
        observed: DispatchCandidate,
        result: SchedulerResult,
        last_dispatched_events: Option<i64>,
        last_elapsed_ms: Option<i64>,
    ) {
        let tick_ts = Utc::now().timestamp_millis().to_string();
        self.set_scheduler_state(as_id, SCHEDULER_STATE_LAST_TICK_TS, &tick_ts).await;
        self.set_scheduler_state(as_id, SCHEDULER_STATE_LAST_RESULT, result.as_str()).await;
        self.set_scheduler_state(as_id, SCHEDULER_STATE_PENDING_EVENT_COUNT, &observed.pending_event_count.to_string())
            .await;
        self.set_scheduler_state(
            as_id,
            SCHEDULER_STATE_PENDING_TRANSACTION_COUNT,
            &observed.pending_transaction_count.to_string(),
        )
        .await;
        self.set_scheduler_state(
            as_id,
            SCHEDULER_STATE_BACKLOG_STATE,
            Self::backlog_state(
                observed.pending_event_count,
                observed.pending_transaction_count,
                self.high_pending_event_threshold,
                self.high_pending_transaction_threshold,
            ),
        )
        .await;
        self.set_scheduler_state(
            as_id,
            SCHEDULER_STATE_TRANSACTION_STATE,
            Self::transaction_state(result, &observed).as_str(),
        )
        .await;

        if let Some(last_dispatched_events) = last_dispatched_events {
            self.set_scheduler_state(
                as_id,
                SCHEDULER_STATE_LAST_DISPATCHED_EVENTS,
                &last_dispatched_events.to_string(),
            )
            .await;
        }
        if let Some(last_elapsed_ms) = last_elapsed_ms {
            self.set_scheduler_state(as_id, SCHEDULER_STATE_LAST_ELAPSED_MS, &last_elapsed_ms.to_string()).await;
        }

        match result {
            SchedulerResult::Dispatched => {
                self.increment_scheduler_counter(as_id, SCHEDULER_STATE_TOTAL_SUCCESS_COUNT).await;
            }
            SchedulerResult::Failed => {
                self.increment_scheduler_counter(as_id, SCHEDULER_STATE_TOTAL_FAILURE_COUNT).await;
            }
            SchedulerResult::Backoff => {
                self.increment_scheduler_counter(as_id, SCHEDULER_STATE_TOTAL_BACKOFF_COUNT).await;
            }
            SchedulerResult::CapacityLimited => {
                self.increment_scheduler_counter(as_id, SCHEDULER_STATE_TOTAL_CAPACITY_LIMITED_COUNT).await;
            }
            SchedulerResult::InFlight => {
                self.increment_scheduler_counter(as_id, SCHEDULER_STATE_TOTAL_IN_FLIGHT_COUNT).await;
            }
            SchedulerResult::Idle | SchedulerResult::Disabled => {}
        }
    }

    async fn set_scheduler_state(&self, as_id: &str, state_key: &str, state_value: &str) {
        if let Err(error) = self.manager.set_state(as_id, state_key, state_value).await {
            warn!(%error, as_id, state_key, "Failed to persist scheduler state");
        }
    }

    async fn increment_scheduler_counter(&self, as_id: &str, state_key: &str) {
        let next_value = match self.manager.get_state(as_id, state_key).await {
            Ok(Some(existing)) => existing.state_value.parse::<i64>().unwrap_or(0).saturating_add(1),
            Ok(None) => 1,
            Err(error) => {
                warn!(%error, as_id, state_key, "Failed to load scheduler counter state");
                1
            }
        };
        self.set_scheduler_state(as_id, state_key, &next_value.to_string()).await;
    }

    fn backlog_state(
        pending_event_count: i64,
        pending_transaction_count: i64,
        high_pending_event_threshold: i64,
        high_pending_transaction_threshold: i64,
    ) -> &'static str {
        if pending_transaction_count >= high_pending_transaction_threshold
            || pending_event_count >= high_pending_event_threshold
        {
            "high"
        } else if pending_transaction_count > 0 || pending_event_count > 0 {
            "normal"
        } else {
            "idle"
        }
    }

    fn transaction_state(result: SchedulerResult, observed: &DispatchCandidate) -> TransactionState {
        match result {
            SchedulerResult::Disabled => TransactionState::Disabled,
            SchedulerResult::InFlight => TransactionState::InFlight,
            SchedulerResult::Backoff => TransactionState::RetryBackoff,
            SchedulerResult::CapacityLimited => TransactionState::CapacityLimited,
            SchedulerResult::Idle => TransactionState::Idle,
            SchedulerResult::Dispatched | SchedulerResult::Failed => {
                if observed.pending_transaction_count > 0 {
                    TransactionState::PendingTransaction
                } else if observed.pending_event_count > 0 {
                    TransactionState::PendingEvents
                } else {
                    TransactionState::Idle
                }
            }
        }
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
        assert_eq!(MAX_SERVICES_PER_TICK, 8);
        assert_eq!(HIGH_PENDING_EVENT_THRESHOLD, 50);
        assert_eq!(HIGH_PENDING_TRANSACTION_THRESHOLD, 2);
    }

    #[test]
    fn test_backlog_state_uses_thresholds() {
        assert_eq!(
            ApplicationServiceScheduler::backlog_state(
                0,
                0,
                HIGH_PENDING_EVENT_THRESHOLD,
                HIGH_PENDING_TRANSACTION_THRESHOLD
            ),
            "idle"
        );
        assert_eq!(
            ApplicationServiceScheduler::backlog_state(
                1,
                0,
                HIGH_PENDING_EVENT_THRESHOLD,
                HIGH_PENDING_TRANSACTION_THRESHOLD
            ),
            "normal"
        );
        assert_eq!(ApplicationServiceScheduler::backlog_state(50, 0, 50, HIGH_PENDING_TRANSACTION_THRESHOLD), "high");
        assert_eq!(ApplicationServiceScheduler::backlog_state(0, 2, HIGH_PENDING_EVENT_THRESHOLD, 2), "high");
    }

    #[test]
    fn test_backlog_state_default_threshold_boundaries() {
        assert_eq!(
            ApplicationServiceScheduler::backlog_state(
                HIGH_PENDING_EVENT_THRESHOLD - 1,
                0,
                HIGH_PENDING_EVENT_THRESHOLD,
                HIGH_PENDING_TRANSACTION_THRESHOLD
            ),
            "normal"
        );
        assert_eq!(
            ApplicationServiceScheduler::backlog_state(
                HIGH_PENDING_EVENT_THRESHOLD,
                0,
                HIGH_PENDING_EVENT_THRESHOLD,
                HIGH_PENDING_TRANSACTION_THRESHOLD
            ),
            "high"
        );
        assert_eq!(
            ApplicationServiceScheduler::backlog_state(
                0,
                HIGH_PENDING_TRANSACTION_THRESHOLD - 1,
                HIGH_PENDING_EVENT_THRESHOLD,
                HIGH_PENDING_TRANSACTION_THRESHOLD
            ),
            "normal"
        );
        assert_eq!(
            ApplicationServiceScheduler::backlog_state(
                0,
                HIGH_PENDING_TRANSACTION_THRESHOLD,
                HIGH_PENDING_EVENT_THRESHOLD,
                HIGH_PENDING_TRANSACTION_THRESHOLD
            ),
            "high"
        );
    }

    #[test]
    fn test_backlog_state_more_aggressive_thresholds_escalate_same_load() {
        let default_event_state = ApplicationServiceScheduler::backlog_state(
            25,
            0,
            HIGH_PENDING_EVENT_THRESHOLD,
            HIGH_PENDING_TRANSACTION_THRESHOLD,
        );
        let aggressive_event_state =
            ApplicationServiceScheduler::backlog_state(25, 0, 25, HIGH_PENDING_TRANSACTION_THRESHOLD);
        assert_eq!(default_event_state, "normal");
        assert_eq!(aggressive_event_state, "high");

        let default_transaction_state = ApplicationServiceScheduler::backlog_state(
            0,
            1,
            HIGH_PENDING_EVENT_THRESHOLD,
            HIGH_PENDING_TRANSACTION_THRESHOLD,
        );
        let aggressive_transaction_state =
            ApplicationServiceScheduler::backlog_state(0, 1, HIGH_PENDING_EVENT_THRESHOLD, 1);
        assert_eq!(default_transaction_state, "normal");
        assert_eq!(aggressive_transaction_state, "high");
    }

    #[test]
    fn test_transaction_state_reflects_scheduler_result_and_pending_work() {
        let pending_events = DispatchCandidate {
            as_id: "alpha".to_owned(),
            priority: DispatchPriority::PendingEvents,
            pending_transaction_count: 0,
            pending_event_count: 2,
            has_statistics: true,
        };
        let pending_txn = DispatchCandidate {
            as_id: "beta".to_owned(),
            priority: DispatchPriority::PendingTransaction,
            pending_transaction_count: 1,
            pending_event_count: 2,
            has_statistics: true,
        };

        assert_eq!(
            ApplicationServiceScheduler::transaction_state(SchedulerResult::Dispatched, &pending_events),
            TransactionState::PendingEvents
        );
        assert_eq!(
            ApplicationServiceScheduler::transaction_state(SchedulerResult::Failed, &pending_txn),
            TransactionState::PendingTransaction
        );
        assert_eq!(
            ApplicationServiceScheduler::transaction_state(SchedulerResult::Backoff, &pending_txn),
            TransactionState::RetryBackoff
        );
        assert_eq!(
            ApplicationServiceScheduler::transaction_state(SchedulerResult::CapacityLimited, &pending_events),
            TransactionState::CapacityLimited
        );
        assert_eq!(
            ApplicationServiceScheduler::transaction_state(
                SchedulerResult::Idle,
                &DispatchCandidate { pending_transaction_count: 0, pending_event_count: 0, ..pending_events }
            ),
            TransactionState::Idle
        );
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

    #[test]
    fn test_transaction_controller_prioritizes_pending_transactions() {
        let controller = TransactionController;
        let services = vec![
            ApplicationService {
                id: 1,
                as_id: "alpha".to_string(),
                url: "http://localhost".to_string(),
                as_token: "a".to_string(),
                hs_token: "b".to_string(),
                sender_localpart: "@alpha:example.com".to_string(),
                is_enabled: true,
                is_rate_limited: false,
                protocols: vec![],
                namespaces: serde_json::json!({}),
                created_ts: 0,
                updated_ts: None,
                description: None,
                api_key: None,
                config: serde_json::json!({}),
            },
            ApplicationService {
                id: 2,
                as_id: "beta".to_string(),
                url: "http://localhost".to_string(),
                as_token: "a".to_string(),
                hs_token: "b".to_string(),
                sender_localpart: "@beta:example.com".to_string(),
                is_enabled: true,
                is_rate_limited: false,
                protocols: vec![],
                namespaces: serde_json::json!({}),
                created_ts: 0,
                updated_ts: None,
                description: None,
                api_key: None,
                config: serde_json::json!({}),
            },
        ];
        let statistics = vec![
            serde_json::json!({
                "as_id": "beta",
                "pending_transaction_count": 0,
                "pending_event_count": 5
            }),
            serde_json::json!({
                "as_id": "alpha",
                "pending_transaction_count": 1,
                "pending_event_count": 1
            }),
        ];

        let candidates = controller.plan_dispatch_order(&services, &statistics, 0);
        assert_eq!(candidates[0].as_id, "alpha");
        assert_eq!(candidates[0].priority, DispatchPriority::PendingTransaction);
        assert_eq!(candidates[1].as_id, "beta");
    }

    #[test]
    fn test_transaction_controller_rotates_ready_services() {
        let controller = TransactionController;
        let services = vec![
            ApplicationService {
                id: 1,
                as_id: "alpha".to_string(),
                url: "http://localhost".to_string(),
                as_token: "a".to_string(),
                hs_token: "b".to_string(),
                sender_localpart: "@alpha:example.com".to_string(),
                is_enabled: true,
                is_rate_limited: false,
                protocols: vec![],
                namespaces: serde_json::json!({}),
                created_ts: 0,
                updated_ts: None,
                description: None,
                api_key: None,
                config: serde_json::json!({}),
            },
            ApplicationService {
                id: 2,
                as_id: "beta".to_string(),
                url: "http://localhost".to_string(),
                as_token: "a".to_string(),
                hs_token: "b".to_string(),
                sender_localpart: "@beta:example.com".to_string(),
                is_enabled: true,
                is_rate_limited: false,
                protocols: vec![],
                namespaces: serde_json::json!({}),
                created_ts: 0,
                updated_ts: None,
                description: None,
                api_key: None,
                config: serde_json::json!({}),
            },
        ];
        let statistics = vec![
            serde_json::json!({
                "as_id": "alpha",
                "pending_transaction_count": 0,
                "pending_event_count": 2
            }),
            serde_json::json!({
                "as_id": "beta",
                "pending_transaction_count": 0,
                "pending_event_count": 1
            }),
        ];

        let rotated = controller.plan_dispatch_order(&services, &statistics, 1);
        assert_eq!(rotated[0].as_id, "beta");
        assert_eq!(rotated[1].as_id, "alpha");
    }

    #[test]
    fn test_transaction_controller_rotation_preserves_priority_buckets() {
        let controller = TransactionController;
        let services = vec![
            ApplicationService {
                id: 1,
                as_id: "alpha".to_string(),
                url: "http://localhost".to_string(),
                as_token: "a".to_string(),
                hs_token: "b".to_string(),
                sender_localpart: "@alpha:example.com".to_string(),
                is_enabled: true,
                is_rate_limited: false,
                protocols: vec![],
                namespaces: serde_json::json!({}),
                created_ts: 0,
                updated_ts: None,
                description: None,
                api_key: None,
                config: serde_json::json!({}),
            },
            ApplicationService {
                id: 2,
                as_id: "beta".to_string(),
                url: "http://localhost".to_string(),
                as_token: "a".to_string(),
                hs_token: "b".to_string(),
                sender_localpart: "@beta:example.com".to_string(),
                is_enabled: true,
                is_rate_limited: false,
                protocols: vec![],
                namespaces: serde_json::json!({}),
                created_ts: 0,
                updated_ts: None,
                description: None,
                api_key: None,
                config: serde_json::json!({}),
            },
            ApplicationService {
                id: 3,
                as_id: "gamma".to_string(),
                url: "http://localhost".to_string(),
                as_token: "a".to_string(),
                hs_token: "b".to_string(),
                sender_localpart: "@gamma:example.com".to_string(),
                is_enabled: true,
                is_rate_limited: false,
                protocols: vec![],
                namespaces: serde_json::json!({}),
                created_ts: 0,
                updated_ts: None,
                description: None,
                api_key: None,
                config: serde_json::json!({}),
            },
            ApplicationService {
                id: 4,
                as_id: "delta".to_string(),
                url: "http://localhost".to_string(),
                as_token: "a".to_string(),
                hs_token: "b".to_string(),
                sender_localpart: "@delta:example.com".to_string(),
                is_enabled: true,
                is_rate_limited: false,
                protocols: vec![],
                namespaces: serde_json::json!({}),
                created_ts: 0,
                updated_ts: None,
                description: None,
                api_key: None,
                config: serde_json::json!({}),
            },
        ];
        let statistics = vec![
            serde_json::json!({
                "as_id": "alpha",
                "pending_transaction_count": 1,
                "pending_event_count": 5
            }),
            serde_json::json!({
                "as_id": "beta",
                "pending_transaction_count": 1,
                "pending_event_count": 1
            }),
            serde_json::json!({
                "as_id": "gamma",
                "pending_transaction_count": 0,
                "pending_event_count": 4
            }),
            serde_json::json!({
                "as_id": "delta",
                "pending_transaction_count": 0,
                "pending_event_count": 2
            }),
        ];

        let rotated = controller.plan_dispatch_order(&services, &statistics, 1);
        let priorities: Vec<_> = rotated.iter().map(|candidate| candidate.priority).collect();
        assert_eq!(
            priorities,
            vec![
                DispatchPriority::PendingTransaction,
                DispatchPriority::PendingTransaction,
                DispatchPriority::PendingEvents,
                DispatchPriority::PendingEvents,
            ]
        );
        assert_eq!(
            rotated.iter().map(|candidate| candidate.as_id.as_str()).collect::<Vec<_>>(),
            vec!["beta", "alpha", "delta", "gamma"]
        );
    }
}
