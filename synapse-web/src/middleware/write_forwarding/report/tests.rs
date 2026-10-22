//! The write-forwarding report must answer three questions an operator actually asks —
//! *where does this write go*, *is that target usable right now*, and *what happened to
//! the last one* — and it must degrade `/_health` only for facts that belong to this
//! process (a reader whose writer is gone used to report healthy with no other symptom
//! than a 503 on the write).
//!
//! Every test injects the process-wide inputs (`routes`, `observations`) through
//! `build_report_with`, so none of them depends on globals: the registry is process-wide
//! and would otherwise leak between tests (`cargo test` runs them in threads; nextest
//! hides the problem by using one process per test).

use std::collections::HashMap;

use super::*;
use synapse_common::config::worker::InstanceLocationConfig;
use synapse_services::worker::runtime_topology::{RegisteredInstance, RuntimeTopologySnapshot};

const NOW_MS: i64 = 1_700_000_000_000;

fn forwarding_config() -> WorkerConfig {
    let mut config = WorkerConfig {
        enabled: true,
        instance_name: "client_reader".to_string(),
        write_forwarding: true,
        ..Default::default()
    };
    config.replication.enabled = true;
    config.replication.http.enabled = true;
    config.replication.http.secret = Some("k".repeat(32));
    config.instance_map.insert(
        "event_persister".to_string(),
        InstanceLocationConfig { host: "persister.internal".to_string(), port: 8008, tls: false },
    );
    // `stream_writers` defaults to `["master"]`, so a realistic `instance_map` declares it.
    // Without that entry every stream still on `master` resolves to `Unavailable`, and the
    // fixture would be testing the wrong branch.
    config.instance_map.insert(
        "master".to_string(),
        InstanceLocationConfig { host: "master.internal".to_string(), port: 8008, tls: false },
    );
    config.stream_writers.events = vec!["event_persister".to_string()];
    config.stream_writers.device_lists = vec!["event_persister".to_string()];
    // One stream owned by *this* instance, so the report has a `local` sample.
    config.stream_writers.typing = vec!["client_reader".to_string()];
    config
}

fn snapshot(live: &[&str]) -> RuntimeTopologySnapshot {
    RuntimeTopologySnapshot {
        registered: live
            .iter()
            .map(|name| RegisteredInstance {
                instance_name: (*name).to_string(),
                worker_id: format!("{name}-1"),
                status: "running".to_string(),
                last_heartbeat_ts: Some(NOW_MS),
            })
            .collect(),
        now_ms: NOW_MS,
        heartbeat_timeout_secs: 60,
    }
}

fn no_observations() -> HashMap<&'static str, Observation> {
    HashMap::new()
}

fn report_for(
    config: &WorkerConfig,
    redis_enabled: bool,
    snapshot: Option<&RuntimeTopologySnapshot>,
) -> WriteForwardingReport {
    build_report_with(config, redis_enabled, snapshot, None, &no_observations())
}

fn stream<'a>(report: &'a WriteForwardingReport, name: &str) -> &'a StreamRoute {
    report.streams.iter().find(|stream| stream.stream == name).unwrap_or_else(|| panic!("{name} missing"))
}

#[test]
fn disabled_forwarding_reports_nothing_forwarded() {
    let report = report_for(&WorkerConfig::default(), true, None);

    assert!(!report.enabled);
    assert_eq!(report.status(), "disabled");
    assert!(report.streams.iter().all(|stream| stream.route == "local"));
    assert!(
        report.streams.iter().all(|stream| stream.writer_live.is_none()),
        "a local stream has no target to be alive or dead"
    );
    assert!(report.summary().contains("off"), "{}", report.summary());
}

#[test]
fn a_forwarded_stream_reports_the_resolved_target() {
    let config = forwarding_config();
    let snapshot = snapshot(&["event_persister"]);
    let report = report_for(&config, true, Some(&snapshot));

    let events = stream(&report, "events");
    assert_eq!(events.route, "forward");
    assert_eq!(events.target_instance.as_deref(), Some("event_persister"));
    assert_eq!(
        events.target_base_url.as_deref(),
        Some("http://persister.internal:8008"),
        "the base_url is what an operator needs to curl when the forward fails"
    );
    assert_eq!(events.writer_live, Some(true));
    assert!(events.requires_redis, "events reaches /sync through the notifier fan-out");

    // A stream owned by this instance is local: forwarding does not mean "everything".
    let typing = stream(&report, "typing");
    assert_eq!(typing.route, "local");
    assert_eq!(typing.target_instance, None);
    assert_eq!(typing.writer_live, None, "a local stream has no target to be alive or dead");

    // Streams still owned by `master` are forwarded **to master** — that is the whole
    // point of declaring it in `instance_map`.
    let receipts = stream(&report, "receipts");
    assert_eq!(receipts.route, "forward");
    assert_eq!(receipts.target_instance.as_deref(), Some("master"));
    assert_eq!(report.status(), "healthy");
}

#[test]
fn a_writer_that_is_not_registered_degrades_before_any_write() {
    // The regression this closes: reader reports healthy while its writer process is gone.
    // Nothing has been written yet, so no `last_outcome` exists — the registry is the only
    // evidence, which is exactly why `/_health` passes a snapshot and `/health` does not.
    let config = forwarding_config();
    let empty = snapshot(&[]);
    let report = report_for(&config, true, Some(&empty));

    assert_eq!(stream(&report, "events").writer_live, Some(false));
    assert_eq!(report.status(), "degraded");
    assert!(
        report.summary().contains("events->event_persister"),
        "the summary must name the stream and its target: {}",
        report.summary()
    );

    // Without a snapshot the same configuration is not degraded: absence of evidence is
    // not evidence of an outage (the container probe must not flap on it).
    assert_eq!(report_for(&config, true, None).status(), "healthy");
}

#[test]
fn an_unusable_target_degrades_and_says_why() {
    // `replication.http.enabled = false` is the reachable cause: forwarding needs the
    // shared secret to break the loop, so `write_route` refuses instead of sending.
    let mut config = forwarding_config();
    config.replication.http.enabled = false;

    let report = report_for(&config, true, None);
    let events = stream(&report, "events");
    assert_eq!(events.route, "unavailable");
    assert!(events.reason.as_deref().is_some_and(|reason| reason.contains("replication.http")), "{:?}", events.reason);
    assert_eq!(report.status(), "degraded");
    assert!(report.summary().contains("unusable writer for events"), "{}", report.summary());
}

#[test]
fn a_notify_bound_stream_forwarded_without_redis_is_degraded() {
    // Writes still succeed; every long-poll on this instance waits out its own timeout.
    // That is invisible to a correctness test, so it has to be visible here (§4.5).
    let config = forwarding_config();
    let report = report_for(&config, false, None);

    // Both notify-bound streams are forwarded here (`events` to the persister, `to_device`
    // still on `master`), and both need the Redis fan-out to reach a reader's long-poll.
    assert_eq!(report.notify_without_redis, vec!["events".to_string(), "to_device".to_string()]);
    assert_eq!(report.status(), "degraded");
    assert!(report.summary().contains("redis.enabled = false"), "{}", report.summary());

    // `device_lists` is forwarded too and is *not* notify-bound: no degradation from it.
    assert!(!stream(&report, "device_lists").requires_redis);
}

#[test]
fn the_last_outcome_decides_but_a_writer_verdict_does_not() {
    let config = forwarding_config();
    let snapshot = snapshot(&["event_persister"]);

    // The report derives the age from wall-clock "now", so the observation is stamped
    // relative to it rather than to the fixed `NOW_MS` the registry snapshot uses.
    let mut observations = HashMap::new();
    observations.insert(
        "events",
        Observation { outcome: LastOutcome::Unreachable, at_ms: synapse_common::current_timestamp_millis() - 5_000 },
    );
    let report = build_report_with(&config, true, Some(&snapshot), None, &observations);
    assert_eq!(report.status(), "degraded");
    assert_eq!(stream(&report, "events").last_outcome, Some(LastOutcome::Unreachable));
    let age = stream(&report, "events").last_outcome_age_secs.expect("age must be reported");
    assert!((4..=15).contains(&age), "age must track the observation, got {age}");
    assert!(report.summary().contains("last forward failed for events"), "{}", report.summary());

    // Negative control: the writer answered 403/429. The path works; a burst of those is
    // the client's problem and must not read as an outage.
    let mut observations = HashMap::new();
    observations.insert("events", Observation { outcome: LastOutcome::UpstreamError, at_ms: NOW_MS });
    let report = build_report_with(&config, true, Some(&snapshot), None, &observations);
    assert_eq!(report.status(), "healthy");
    assert_eq!(stream(&report, "events").last_outcome, Some(LastOutcome::UpstreamError));
}

#[test]
fn published_routes_win_over_the_configuration() {
    // The case that makes the assembly publish its resolved routes at all: a replication
    // secret that fails to load collapses every forward to `Unavailable` in the policy,
    // while `write_route` — which cannot read the secret file — still says `Forward`.
    // Reporting from the config would therefore lie exactly when it matters.
    let config = forwarding_config();
    let published = vec![(
        WriteStream::Events,
        WriteRoute::Unavailable {
            instance: "event_persister".to_string(),
            reason: "worker.replication.http.secret / secret_path 读不到可用密钥".to_string(),
        },
    )];

    let from_config = report_for(&config, true, None);
    assert_eq!(stream(&from_config, "events").route, "forward");

    let from_assembly = build_report_with(&config, true, None, Some(published), &no_observations());
    assert_eq!(stream(&from_assembly, "events").route, "unavailable");
    assert_eq!(from_assembly.status(), "degraded");
}

#[test]
fn recording_an_outcome_reaches_the_snapshot() {
    // The write path's only hook: `record_forward` calls this, and the report reads it.
    // Uses `typing` so it cannot influence any other assertion in this file (they use
    // injected maps, but the global map is process-wide).
    record(WriteStream::Typing, LastOutcome::Unreachable);
    let snapshot = observation_snapshot();
    let observation = snapshot.get("typing").copied().expect("typing must be recorded");
    assert_eq!(observation.outcome, LastOutcome::Unreachable);
    assert_eq!(LastOutcome::Unreachable.label(), "unreachable");
}
