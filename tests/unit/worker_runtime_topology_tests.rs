#![allow(clippy::unwrap_used, clippy::expect_used)]
//! Runtime worker-topology reconciliation (W-A): the rules that compare the
//! configured topology against the running one.
//!
//! Kept outside `synapse-services/src/worker/runtime_topology.rs` on purpose: the
//! file-size ratchet (`scripts/ci/check_file_size_ratchet.py`, threshold 500)
//! counts the module and its tests as one file, and the module is meant to stay
//! well under the line.

use std::collections::HashMap;

use synapse_common::config::worker::{InstanceLocationConfig, WorkerConfig};
use synapse_services::worker::runtime_topology::{
    evaluate_runtime_topology, stream_writer_sets, RegisteredInstance, RuntimeTopologySnapshot,
    REQUIRED_STREAM_CARDINALITY,
};

/// A worker-enabled configuration whose instance names are `names`, with the
/// shipped stream writers left alone (every stream written by `master`).
fn config_with_instances(names: &[&str]) -> WorkerConfig {
    let mut instance_map = HashMap::new();
    for name in names {
        instance_map.insert(
            (*name).to_string(),
            InstanceLocationConfig { host: "127.0.0.1".to_string(), port: 8100, tls: false },
        );
    }
    WorkerConfig { enabled: true, instance_name: "master".to_string(), instance_map, ..WorkerConfig::default() }
}

fn registered(instance_name: &str, age_ms: i64, now_ms: i64) -> RegisteredInstance {
    RegisteredInstance {
        instance_name: instance_name.to_string(),
        worker_id: format!("id-{instance_name}"),
        status: "running".to_string(),
        last_heartbeat_ts: Some(now_ms - age_ms),
    }
}

fn snapshot(registered: Vec<RegisteredInstance>) -> RuntimeTopologySnapshot {
    RuntimeTopologySnapshot { registered, now_ms: 1_000_000, heartbeat_timeout_secs: 90 }
}

#[test]
fn disabled_worker_configuration_reports_nothing() {
    // Negative control for the "enabled" gate: a monolith deployment that
    // carries latent stream-writer problems must not gain a new alarm.
    let mut config = config_with_instances(&["event_persister"]);
    config.enabled = false;
    config.stream_writers.typing = Vec::new();

    let report = evaluate_runtime_topology(&config, &snapshot(vec![registered("event_persister", 0, 1_000_000)]));

    assert!(report.valid);
    assert!(report.errors.is_empty());
    assert!(report.missing.is_empty());
    assert!(report.writer_violations.is_empty());
    assert!(report.declared_instances.is_empty());
}

#[test]
fn a_declared_instance_that_never_registered_is_missing() {
    let config = config_with_instances(&["event_persister"]);

    let report = evaluate_runtime_topology(&config, &snapshot(Vec::new()));

    assert!(!report.valid);
    assert_eq!(report.missing, vec!["event_persister"]);
    assert_eq!(report.errors.len(), 1);
    assert!(report.errors[0].contains("event_persister"));
}

#[test]
fn master_is_never_reported_missing() {
    // Negative control for the exemption: the main process registers itself
    // nowhere, so requiring a registry row for it would make the default
    // configuration permanently red.
    let config = config_with_instances(&[]);

    let report = evaluate_runtime_topology(&config, &snapshot(Vec::new()));

    assert!(report.valid);
    assert!(report.missing.is_empty());
    assert_eq!(report.declared_instances, vec!["master".to_string()]);
}

#[test]
fn the_current_instance_is_never_reported_missing() {
    // The process answering the report is alive by construction, so it must
    // not be reported as missing before its heartbeat lands.
    let mut config = config_with_instances(&["sync_worker"]);
    config.instance_name = "sync_worker".to_string();

    let report = evaluate_runtime_topology(&config, &snapshot(Vec::new()));

    assert!(report.valid);
    assert!(report.missing.is_empty());
    assert!(report.declared_instances.contains(&"sync_worker".to_string()));
}

#[test]
fn a_running_instance_outside_the_configuration_is_unexpected() {
    let config = config_with_instances(&["event_persister"]);
    let snapshot =
        snapshot(vec![registered("event_persister", 0, 1_000_000), registered("media_repository", 0, 1_000_000)]);

    let report = evaluate_runtime_topology(&config, &snapshot);

    assert!(report.valid, "an undeclared instance is a warning, not an outage: {:?}", report.errors);
    assert_eq!(report.unexpected, vec!["media_repository"]);
    assert_eq!(report.warnings.len(), 1);
}

#[test]
fn a_stale_heartbeat_makes_the_instance_not_live() {
    let config = config_with_instances(&["event_persister"]);
    // Timeout is 90s; 90_001 ms without a heartbeat is stale.
    let snapshot = snapshot(vec![registered("event_persister", 90_001, 1_000_000)]);

    let report = evaluate_runtime_topology(&config, &snapshot);

    assert!(!report.valid);
    assert_eq!(report.stale, vec!["event_persister"]);
    assert_eq!(report.missing, vec!["event_persister"]);
    assert!(report.live_instances.is_empty());
}

#[test]
fn a_heartbeat_exactly_at_the_timeout_is_still_live() {
    // Boundary negative control: the health checker uses the same inclusive
    // comparison (`elapsed > timeout` is the failure), so the two must agree.
    let config = config_with_instances(&["event_persister"]);
    let snapshot = snapshot(vec![registered("event_persister", 90_000, 1_000_000)]);

    let report = evaluate_runtime_topology(&config, &snapshot);

    assert!(report.valid);
    assert_eq!(report.live_instances, vec!["event_persister"]);
    assert!(report.stale.is_empty());
}

#[test]
fn a_registered_instance_without_a_heartbeat_is_not_live() {
    let config = config_with_instances(&["event_persister"]);
    let snapshot = snapshot(vec![RegisteredInstance {
        instance_name: "event_persister".to_string(),
        worker_id: "id-event-persister".to_string(),
        status: "starting".to_string(),
        last_heartbeat_ts: None,
    }]);

    let report = evaluate_runtime_topology(&config, &snapshot);

    assert!(!report.valid);
    assert_eq!(report.stale, vec!["event_persister"]);
}

#[test]
fn a_stream_whose_only_owner_is_not_running_is_unowned() {
    let mut config = config_with_instances(&["event_persister"]);
    config.stream_writers.events = vec!["event_persister".to_string()];

    let report = evaluate_runtime_topology(&config, &snapshot(Vec::new()));

    assert!(!report.valid);
    assert_eq!(report.unowned_streams, vec!["events"]);
    assert!(report.errors.iter().any(|error| error.contains("stream 'events'")));
}

#[test]
fn a_stream_owned_by_master_is_owned_without_registration() {
    // Negative control for the "master counts as live" rule: the shipped
    // defaults are all `master`, and no deployment registers the main
    // process, so the default configuration must report no unowned stream.
    let config = config_with_instances(&[]);

    let report = evaluate_runtime_topology(&config, &snapshot(Vec::new()));

    assert!(report.valid);
    assert!(report.unowned_streams.is_empty());
}

#[test]
fn a_stream_with_an_empty_owner_list_is_left_to_the_static_validator() {
    let mut config = config_with_instances(&[]);
    config.stream_writers.presence = Vec::new();

    let report = evaluate_runtime_topology(&config, &snapshot(Vec::new()));

    assert!(report.unowned_streams.is_empty(), "the static validator owns the empty case");
}

#[test]
fn the_default_configuration_has_no_cardinality_violation() {
    // Negative control for the rule table: every shipped default is `["master"]`.
    let config = config_with_instances(&[]);

    let report = evaluate_runtime_topology(&config, &snapshot(Vec::new()));

    assert!(report.writer_violations.is_empty());
    assert!(report.valid);
}

#[test]
fn an_exactly_one_stream_with_two_owners_is_a_violation() {
    let mut config = config_with_instances(&["client_reader", "client_reader-2"]);
    config.stream_writers.typing = vec!["client_reader".to_string(), "client_reader-2".to_string()];

    let report = evaluate_runtime_topology(&config, &snapshot(Vec::new()));

    assert!(report
        .writer_violations
        .iter()
        .any(|violation| violation.contains("stream 'typing'") && violation.contains("exactly one")));
}

#[test]
fn an_exactly_one_stream_with_no_owner_is_a_violation() {
    let mut config = config_with_instances(&[]);
    config.stream_writers.to_device = Vec::new();

    let report = evaluate_runtime_topology(&config, &snapshot(Vec::new()));

    assert!(report
        .writer_violations
        .iter()
        .any(|violation| violation.contains("stream 'to_device'") && violation.contains("exactly one")));
}

#[test]
fn a_duplicate_owner_is_a_violation_even_when_the_cardinality_matches() {
    // `events` allows several writers, so one instance listed twice is the
    // only thing wrong here — a case the cardinality rule alone cannot see.
    let mut config = config_with_instances(&["event_persister"]);
    config.stream_writers.events = vec!["event_persister".to_string(), "event_persister".to_string()];

    let report = evaluate_runtime_topology(&config, &snapshot(Vec::new()));

    assert!(report.writer_violations.iter().any(|violation| violation.contains("more than once")));
}

#[test]
fn an_at_least_one_stream_with_several_owners_is_accepted() {
    // Negative control for the "at least one" side of the table: sharding the
    // event stream across writers is legitimate upstream.
    let mut config = config_with_instances(&["event_persister", "event_persister-2"]);
    config.stream_writers.events = vec!["event_persister".to_string(), "event_persister-2".to_string()];

    let report = evaluate_runtime_topology(&config, &snapshot(Vec::new()));

    assert!(report.writer_violations.is_empty());
}

#[test]
fn every_stream_in_the_configuration_has_a_cardinality_rule() {
    // Guards the rule table against a new `StreamWriters` field being added
    // without a requirement (which would silently skip the check).
    let config = config_with_instances(&[]);
    let configured: Vec<&str> = stream_writer_sets(&config).iter().map(|(name, _)| *name).collect();

    for stream in &configured {
        assert!(
            REQUIRED_STREAM_CARDINALITY.iter().any(|(name, _)| name == stream),
            "stream '{stream}' has no cardinality rule"
        );
    }
    assert_eq!(configured.len(), REQUIRED_STREAM_CARDINALITY.len());
}
