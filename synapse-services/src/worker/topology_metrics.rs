//! Worker-topology observability (W-E).
//!
//! Before this, the topology verdict lived in exactly two places: a log line at
//! startup and (from W-A) an endpoint you have to ask. Neither is alertable — you
//! cannot page on "nobody noticed", and an endpoint nobody polls is not monitoring.
//! This module turns the same reconciliation into gauges on the shared
//! [`MetricsCollector`], which the app already renders at `/metrics` for
//! Prometheus (`synapse-web/src/routes/telemetry.rs`, scraped at
//! `synapse-app:9090` by `docker/deploy/prometheus/prometheus.yml`).
//!
//! The evaluation itself is not duplicated: this module only projects
//! [`evaluate_runtime_topology`]'s report onto gauges, so a metric can never say
//! something the endpoint would not.
//!
//! Metric names are written as literals at the registration call because
//! `scripts/ci/check_dashboard_metrics.py` extracts them from source text; a name
//! routed through a constant would be invisible to that gate. The unit tests
//! assert the *rendered* exposition instead of comparing constants, which also
//! catches a name that is registered but never rendered.

use std::sync::Arc;
use std::time::Duration;

use tokio::task::JoinHandle;
use tokio_util::sync::CancellationToken;
use tracing::warn;

use synapse_common::config::worker::WorkerConfig;
use synapse_common::metrics::{DynamicGaugeTemplate, Gauge, MetricsCollector};

use crate::worker::manager::WorkerManager;
use crate::worker::runtime_topology::{
    evaluate_runtime_topology, RegisteredInstance, RuntimeTopologyReport, RuntimeTopologySnapshot,
};

/// How often the reconciliation is re-evaluated.
///
/// Matches the scrape interval the deployment Prometheus uses for the app
/// (`docker/deploy/prometheus/prometheus.yml`, `scrape_interval: 30s`): sampling
/// faster would only add noise, sampling slower would add up to a scrape of lag
/// to any alert built on these gauges.
pub const DEFAULT_EVALUATION_INTERVAL: Duration = Duration::from_secs(30);

/// The gauges this module maintains.
///
/// Every handle is a cheap clone of the collector's own gauge (`Gauge` is an
/// `Arc` inside), so recording does not lock the collector. Not `Clone`/`Debug`
/// itself: `DynamicGaugeTemplate` implements neither, and the task only ever needs
/// to own one instance.
pub struct WorkerTopologyMetrics {
    /// 1 when the running deployment matches the configuration, 0 otherwise.
    valid: Gauge,
    /// How many instances the registry reports as active.
    registered: Gauge,
    /// How many declared instances are not running.
    missing: Gauge,
    /// How many registered instances stopped heartbeating.
    stale: Gauge,
    /// How many streams have no running owner.
    unowned_streams: Gauge,
    /// Per-instance seconds since the last heartbeat (`instance` label).
    heartbeat_age_seconds: DynamicGaugeTemplate,
}

impl WorkerTopologyMetrics {
    /// Register the topology gauges on `collector`.
    ///
    /// Idempotent in the sense that matters: re-registering the same name returns
    /// the same gauge (`MetricsCollector::register_gauge` inserts into a map), so a
    /// second call does not split the series.
    pub fn register(collector: &MetricsCollector) -> Self {
        Self {
            valid: collector.register_gauge("synapse_worker_topology_valid".to_string()),
            registered: collector.register_gauge("synapse_worker_registered_total".to_string()),
            missing: collector.register_gauge("synapse_worker_missing_instances".to_string()),
            stale: collector.register_gauge("synapse_worker_stale_instances".to_string()),
            unowned_streams: collector.register_gauge("synapse_worker_unowned_streams".to_string()),
            heartbeat_age_seconds: collector.create_dynamic_gauge_template(
                "synapse_worker_instance_last_heartbeat_age_seconds".to_string(),
                vec!["instance"],
            ),
        }
    }

    /// Publish one reconciliation result.
    ///
    /// `valid` is a gauge rather than a counter on purpose: the question is "is the
    /// deployment correct *right now*", which is what an alert needs. A counter
    /// would tell you a transition happened and leave you computing rates to find
    /// out whether you are currently broken.
    pub fn record(&self, report: &RuntimeTopologyReport, snapshot: &RuntimeTopologySnapshot) {
        self.valid.set(if report.valid { 1.0 } else { 0.0 });
        self.registered.set(snapshot.registered.len() as f64);
        self.missing.set(report.missing.len() as f64);
        self.stale.set(report.stale.len() as f64);
        self.unowned_streams.set(report.unowned_streams.len() as f64);

        for instance in &snapshot.registered {
            // An instance that never heartbeated has no age to report; it is counted
            // by `stale_instances` instead (see `is_fresh`), so it cannot go missing
            // from the metrics altogether.
            if let Some(age_secs) = instance.heartbeat_age_secs(snapshot.now_ms) {
                self.heartbeat_age_seconds.set(&[instance.instance_name.as_str()], age_secs as f64);
            }
        }
    }
}

/// Evaluate the topology every [`DEFAULT_EVALUATION_INTERVAL`] and publish it.
///
/// Only started when `worker.enabled` is true (`wiring/admin.rs`): a monolith has
/// no registry to reconcile, and the report would be a constant "valid" series that
/// says nothing while looking like coverage.
///
/// Follows this repository's background-task contract: the loop observes `shutdown`
/// with `select! { biased; .. }` so it stops with the process instead of outliving it.
pub fn spawn_worker_topology_metrics(
    manager: Arc<WorkerManager>,
    config: WorkerConfig,
    collector: Arc<MetricsCollector>,
    shutdown: CancellationToken,
) -> JoinHandle<()> {
    let metrics = WorkerTopologyMetrics::register(&collector);
    let heartbeat_timeout_secs = crate::worker::health::HealthCheckConfig::default().heartbeat_timeout_secs;

    tokio::spawn(async move {
        let mut ticker = tokio::time::interval(DEFAULT_EVALUATION_INTERVAL);
        loop {
            tokio::select! {
                biased;
                () = shutdown.cancelled() => break,
                _ = ticker.tick() => {
                    match manager.get_active().await {
                        Ok(active) => {
                            let now_ms = synapse_common::current_timestamp_millis();
                            let registered: Vec<RegisteredInstance> =
                                active.iter().map(RegisteredInstance::from_worker).collect();
                            let snapshot = RuntimeTopologySnapshot {
                                registered,
                                now_ms,
                                heartbeat_timeout_secs,
                            };
                            let report = evaluate_runtime_topology(&config, &snapshot);
                            metrics.record(&report, &snapshot);
                        }
                        Err(error) => {
                            // Keep the previous values rather than zeroing them: a
                            // database hiccup must not look like "everything is
                            // valid" to an alert on `topology_valid`.
                            warn!(%error, "Worker topology metrics: could not read the registry");
                        }
                    }
                }
            }
        }
    })
}
