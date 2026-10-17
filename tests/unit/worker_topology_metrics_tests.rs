//! Worker-topology metrics (W-E): the reconciliation reaches the exposition.
//!
//! These assert the **rendered** `/metrics` text rather than comparing name
//! constants, because that is the only thing Prometheus ever sees: a name that is
//! registered but never rendered, or a value written to a series nobody scrapes,
//! would both pass a constants-only test while leaving the alert permanently
//! silent (the failure mode `check_metric_instrumentation.py` documents for the
//! HTTP and DB pools).

#![allow(clippy::unwrap_used, clippy::expect_used)]

use std::collections::HashMap;

use synapse_common::config::worker::{InstanceLocationConfig, WorkerConfig};
use synapse_common::metrics::MetricsCollector;
use synapse_services::worker::runtime_topology::{RegisteredInstance, RuntimeTopologyReport, RuntimeTopologySnapshot};
use synapse_services::worker::WorkerTopologyMetrics;

const HEARTBEAT_TIMEOUT_SECS: u64 = 90;
const NOW_MS: i64 = 1_700_000_000_000;

/// A worker-enabled configuration declaring exactly one instance,
/// `event_persister`, so a report has something concrete to be missing or live.
fn declared_config() -> WorkerConfig {
    let mut instance_map = HashMap::new();
    instance_map.insert(
        "event_persister".to_string(),
        InstanceLocationConfig { host: "127.0.0.1".to_string(), port: 9102, tls: false },
    );
    let mut config =
        WorkerConfig { enabled: true, instance_name: "master".to_string(), instance_map, ..WorkerConfig::default() };
    config.replication.enabled = true;
    config.replication.http.enabled = true;
    config.replication.http.secret = Some("a-sufficiently-long-test-secret-value".to_string());
    config
}

fn snapshot(registered: Vec<RegisteredInstance>) -> RuntimeTopologySnapshot {
    RuntimeTopologySnapshot { registered, now_ms: NOW_MS, heartbeat_timeout_secs: HEARTBEAT_TIMEOUT_SECS }
}

fn registered(instance_name: &str, heartbeat_age_secs: Option<i64>) -> RegisteredInstance {
    RegisteredInstance {
        instance_name: instance_name.to_string(),
        worker_id: format!("id-{instance_name}"),
        status: "running".to_string(),
        last_heartbeat_ts: heartbeat_age_secs.map(|age| NOW_MS - age * 1000),
    }
}

/// The first value rendered for `name` (optionally with a label selector), or
/// `None` when the series is absent. Deliberately tolerant about the exact label
/// syntax so the assertion is about "is there a number for this series".
fn rendered_value(exposition: &str, name: &str) -> Option<f64> {
    exposition
        .lines()
        .filter(|line| !line.starts_with('#'))
        .find(|line| line.starts_with(name))
        .and_then(|line| line.split_whitespace().last())
        .and_then(|value| value.parse::<f64>().ok())
}

fn normalised_report(config: &WorkerConfig, snapshot: &RuntimeTopologySnapshot) -> RuntimeTopologyReport {
    synapse_services::worker::evaluate_runtime_topology(config, snapshot)
}

#[test]
fn a_matching_deployment_renders_valid_one_and_no_gaps() {
    let config = declared_config();
    let snapshot = snapshot(vec![registered("event_persister", Some(3)), registered("master", Some(1))]);
    let report = normalised_report(&config, &snapshot);

    let collector = MetricsCollector::new();
    let metrics = WorkerTopologyMetrics::register(&collector);
    metrics.record(&report, &snapshot);
    let exposition = collector.to_prometheus_format();

    assert_eq!(
        rendered_value(&exposition, "synapse_worker_topology_valid"),
        Some(1.0),
        "a matching deployment must render valid=1:\n{exposition}"
    );
    assert_eq!(rendered_value(&exposition, "synapse_worker_registered_total"), Some(2.0));
    assert_eq!(rendered_value(&exposition, "synapse_worker_missing_instances"), Some(0.0));
    assert_eq!(rendered_value(&exposition, "synapse_worker_stale_instances"), Some(0.0));
    assert_eq!(rendered_value(&exposition, "synapse_worker_unowned_streams"), Some(0.0));
}

#[test]
fn a_declared_instance_that_is_not_running_renders_valid_zero_and_a_non_zero_gap() {
    // The alertable case, and the reason the metric exists at all: the process that
    // is supposed to be serving never registered.
    let config = declared_config();
    let snapshot = snapshot(vec![registered("master", Some(1))]);
    let report = normalised_report(&config, &snapshot);
    assert_eq!(report.missing, vec!["event_persister".to_string()], "fixture must be missing the instance");

    let collector = MetricsCollector::new();
    let metrics = WorkerTopologyMetrics::register(&collector);
    metrics.record(&report, &snapshot);
    let exposition = collector.to_prometheus_format();

    assert_eq!(rendered_value(&exposition, "synapse_worker_topology_valid"), Some(0.0));
    assert_eq!(rendered_value(&exposition, "synapse_worker_missing_instances"), Some(1.0));
}

#[test]
fn a_stale_heartbeat_renders_as_a_stale_instance_and_an_age_series() {
    // Staleness has a dedicated count *and* a per-instance age: the count is what
    // an alert fires on, the age is what tells an operator which instance and for
    // how long.
    let config = declared_config();
    let snapshot = snapshot(vec![
        registered("event_persister", Some(1)),
        registered("master", Some(HEARTBEAT_TIMEOUT_SECS as i64 + 42)),
    ]);
    let report = normalised_report(&config, &snapshot);
    assert_eq!(report.stale, vec!["master".to_string()], "fixture must have exactly one stale instance");

    let collector = MetricsCollector::new();
    let metrics = WorkerTopologyMetrics::register(&collector);
    metrics.record(&report, &snapshot);
    let exposition = collector.to_prometheus_format();

    assert_eq!(rendered_value(&exposition, "synapse_worker_stale_instances"), Some(1.0));
    assert_eq!(
        rendered_value(&exposition, "synapse_worker_instance_last_heartbeat_age_seconds{instance=\"master\"}"),
        Some((HEARTBEAT_TIMEOUT_SECS + 42) as f64),
        "the stale instance's age must be published under its own label:\n{exposition}"
    );
    assert_eq!(
        rendered_value(&exposition, "synapse_worker_instance_last_heartbeat_age_seconds{instance=\"event_persister\"}"),
        Some(1.0)
    );
}

#[test]
fn an_instance_that_never_heartbeated_is_counted_as_stale_without_an_age_series() {
    // The `None` heartbeat case: there is no age to publish, but the instance must
    // not vanish from the metrics — it is counted by `stale_instances`.
    let config = declared_config();
    let snapshot = snapshot(vec![registered("event_persister", None)]);
    let report = normalised_report(&config, &snapshot);

    let collector = MetricsCollector::new();
    let metrics = WorkerTopologyMetrics::register(&collector);
    metrics.record(&report, &snapshot);
    let exposition = collector.to_prometheus_format();

    assert_eq!(rendered_value(&exposition, "synapse_worker_stale_instances"), Some(1.0));
    assert!(
        rendered_value(&exposition, "synapse_worker_instance_last_heartbeat_age_seconds{instance=\"event_persister\"}")
            .is_none(),
        "an instance with no heartbeat has no age to report:\n{exposition}"
    );
}

#[test]
fn registering_twice_does_not_split_the_series() {
    // `spawn_worker_topology_metrics` registers on every call; a second collector
    // registration for the same name must return the same gauge instead of a second
    // series (Prometheus would reject duplicate series with the same labels).
    let collector = MetricsCollector::new();
    let first = WorkerTopologyMetrics::register(&collector);
    let second = WorkerTopologyMetrics::register(&collector);

    let config = declared_config();
    let snapshot = snapshot(vec![registered("event_persister", Some(1))]);
    let report = normalised_report(&config, &snapshot);
    first.record(&report, &snapshot);
    second.record(&report, &snapshot);

    let exposition = collector.to_prometheus_format();
    let occurrences = exposition.lines().filter(|line| line.starts_with("synapse_worker_topology_valid")).count();
    assert_eq!(occurrences, 1, "the gauge must be rendered exactly once:\n{exposition}");
}
