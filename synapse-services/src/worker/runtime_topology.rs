//! Runtime worker-topology reconciliation (W-A).
//!
//! [`crate::worker::topology_validator`] answers "is this **configuration**
//! self-consistent?" by reading [`WorkerConfig`] alone. This module answers the
//! complementary question, which no config-only check can: **"is the running
//! deployment the one that was configured?"** It compares the declared instances
//! against the worker registry and reports the failures that are otherwise
//! invisible —
//!
//! * a declared instance that never registered (**`missing`**): the operator
//!   believes a responsibility is covered, but nothing is serving it;
//! * a registered worker whose instance name is not in the configuration
//!   (`unexpected`): traffic and stream writes may be routed to it by nobody;
//! * an instance whose heartbeat stopped (`stale`): it registered once, so the
//!   registry still lists it, but it is not running any more;
//! * a stream whose configured owners are all not running (`unowned_streams`).
//!
//! The decision itself is a **pure function** ([`evaluate_runtime_topology`])
//! over a plain-data snapshot, so every rule below is testable without a
//! database, a clock, or a live cluster. The asynchronous part — reading the
//! registry and the clock — lives in the route (`synapse-web`), which is also
//! where the operator-facing endpoints are.
//!
//! Why `master` is exempt from the "must be registered" rule: registration is an
//! explicit administrative step (`POST /_synapse/worker/v1/register`); the worker
//! binaries only *send heartbeats*, and the main process never registers itself.
//! Without the exemption, the default configuration would report `master` as
//! missing forever — a permanent false alarm that would train operators to
//! ignore the whole report.

use std::collections::BTreeSet;

use serde::Serialize;

use synapse_common::config::worker::WorkerConfig;

use crate::worker::topology_validator::resolved_current_instance_name;
use crate::worker::types::WorkerInfo;

/// The instance name the main process always runs under.
///
/// Mirrors the literal used by [`crate::worker::topology_validator`]; the main
/// process does not register itself, so this name is treated as always live (see
/// the module documentation).
const MASTER_INSTANCE_NAME: &str = "master";

/// The configured owner instances of every stream writer this build can express,
/// paired with the stream's name.
///
/// This is the **single** `StreamWriters` → field mapping: the static validator
/// ([`crate::worker::topology_validator::validate_worker_config`]) and the
/// runtime reconciliation both read it, so a stream added to `StreamWriters`
/// without being added here is unchecked in *both* places at once. The
/// cardinality guard in `tests/unit/worker_runtime_topology_tests.rs` exists to
/// make that impossible to miss.
pub fn stream_writer_sets(config: &WorkerConfig) -> [(&'static str, &Vec<String>); 8] {
    [
        ("events", &config.stream_writers.events),
        ("typing", &config.stream_writers.typing),
        ("to_device", &config.stream_writers.to_device),
        ("account_data", &config.stream_writers.account_data),
        ("receipts", &config.stream_writers.receipts),
        ("presence", &config.stream_writers.presence),
        ("push_rules", &config.stream_writers.push_rules),
        ("device_lists", &config.stream_writers.device_lists),
    ]
}

/// How many instances may own a stream.
///
/// Mirrors upstream `synapse/config/workers.py`, which rejects `len(...) != 1`
/// for the single-writer streams and `len(...) == 0` for the ones that must have
/// at least one writer.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StreamCardinality {
    /// Exactly one instance may be configured as the stream's writer.
    ExactlyOne,
    /// One or more instances may write the stream.
    AtLeastOne,
}

/// The cardinality requirement of every stream this build can express.
///
/// Only the eight streams with a config surface are listed: upstream also
/// requires `thread_subscriptions` and `quarantined_media_changes`, but those
/// have no [`synapse_common::config::worker::StreamWriters`] field yet, so there
/// is nothing to check here (tracked as W-6 in the worker-topology audit).
pub const REQUIRED_STREAM_CARDINALITY: [(&str, StreamCardinality); 8] = [
    ("events", StreamCardinality::AtLeastOne),
    ("typing", StreamCardinality::ExactlyOne),
    ("to_device", StreamCardinality::ExactlyOne),
    ("account_data", StreamCardinality::ExactlyOne),
    ("receipts", StreamCardinality::AtLeastOne),
    ("presence", StreamCardinality::ExactlyOne),
    ("push_rules", StreamCardinality::ExactlyOne),
    ("device_lists", StreamCardinality::AtLeastOne),
];

/// One worker as the registry reports it.
///
/// Plain data on purpose: the evaluation must not depend on the storage layer's
/// `WorkerInfo` (which carries a dozen unrelated columns), so the route maps one
/// into the other.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RegisteredInstance {
    /// `worker_name` — the instance name the deployment gave this worker, i.e.
    /// what `instance_map` keys are matched against.
    pub instance_name: String,
    /// `worker_id` — the per-process identity (`SYNAPSE_WORKER_ID`).
    pub worker_id: String,
    /// The stored status (`running`, `starting`, …).
    pub status: String,
    /// `last_heartbeat_ts`, or `None` when no heartbeat was ever recorded.
    pub last_heartbeat_ts: Option<i64>,
}

impl RegisteredInstance {
    /// Project a registry row into the plain data an evaluation consumes.
    ///
    /// Lives here rather than at each call site so the operator-facing endpoint and
    /// the metrics task (W-E) cannot disagree about what the registry says: both go
    /// through this one mapping.
    pub fn from_worker(worker: &WorkerInfo) -> Self {
        Self {
            instance_name: worker.worker_name.clone(),
            worker_id: worker.worker_id.clone(),
            status: worker.status.clone(),
            last_heartbeat_ts: worker.last_heartbeat_ts,
        }
    }

    /// Seconds since this instance's last heartbeat, or `None` when it never sent
    /// one. Seconds (not milliseconds) because that is the unit Prometheus
    /// conventions — and this crate's existing gauges — use for ages.
    pub fn heartbeat_age_secs(&self, now_ms: i64) -> Option<i64> {
        self.last_heartbeat_ts.map(|ts| now_ms.saturating_sub(ts) / 1000)
    }
}

/// The registry and clock readings an evaluation needs.
#[derive(Debug, Clone)]
pub struct RuntimeTopologySnapshot {
    /// Every worker the registry returned as active.
    pub registered: Vec<RegisteredInstance>,
    /// Evaluation time, milliseconds since the Unix epoch.
    pub now_ms: i64,
    /// How long an instance may go without a heartbeat before it counts as not
    /// running. Taken from the live health checker's own configuration so the
    /// report and the worker-selection path agree on what "running" means.
    pub heartbeat_timeout_secs: u64,
}

/// The reconciliation result.
///
/// Lists are sorted so the response is stable across runs (and so tests can
/// assert on them directly).
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct RuntimeTopologyReport {
    /// Whether the running deployment matches the configuration, i.e. whether
    /// [`Self::errors`] is empty.
    pub valid: bool,
    /// Instance names the configuration declares (`master`, the instance this
    /// process runs as, and every `instance_map` key).
    pub declared_instances: Vec<String>,
    /// Declared or registered instances that are actually running.
    pub live_instances: Vec<String>,
    /// Declared instances that are not running. Excludes `master` and the
    /// current instance — neither is expected to register itself.
    pub missing: Vec<String>,
    /// Registered instances whose heartbeat stopped.
    pub stale: Vec<String>,
    /// Running instances the configuration does not declare.
    pub unexpected: Vec<String>,
    /// Streams whose configured owners are all not running.
    pub unowned_streams: Vec<String>,
    /// Cardinality violations read from the configuration (duplicate owners, an
    /// "exactly one" stream with zero or several, an "at least one" stream with
    /// none).
    pub writer_violations: Vec<String>,
    /// Everything that makes the running deployment non-conforming.
    pub errors: Vec<String>,
    /// Non-fatal observations.
    pub warnings: Vec<String>,
}

/// Reconcile the configured topology against the running one.
///
/// * A **disabled** worker configuration has no runtime topology to reconcile
///   (the registry is only populated when workers are actually run), so the
///   report is empty and valid. This deliberately matches
///   [`crate::worker::topology_validator::validate_worker_config`], which also
///   skips its stream checks when `worker.enabled` is false: a latent config
///   problem in a monolith deployment must not turn into a new alarm here.
/// * Liveness is decided from `last_heartbeat_ts` against
///   [`RuntimeTopologySnapshot::heartbeat_timeout_secs`]; an instance that never
///   heartbeated is not live.
pub fn evaluate_runtime_topology(config: &WorkerConfig, snapshot: &RuntimeTopologySnapshot) -> RuntimeTopologyReport {
    if !config.enabled {
        return RuntimeTopologyReport {
            valid: true,
            declared_instances: Vec::new(),
            live_instances: Vec::new(),
            missing: Vec::new(),
            stale: Vec::new(),
            unexpected: Vec::new(),
            unowned_streams: Vec::new(),
            writer_violations: Vec::new(),
            errors: Vec::new(),
            warnings: Vec::new(),
        };
    }

    let declared = declared_instances(config);
    let current_instance = resolved_current_instance_name(config);

    let mut live: BTreeSet<String> = BTreeSet::new();
    let mut stale: BTreeSet<String> = BTreeSet::new();
    for instance in &snapshot.registered {
        if is_fresh(instance, snapshot.now_ms, snapshot.heartbeat_timeout_secs) {
            live.insert(instance.instance_name.clone());
        } else {
            stale.insert(instance.instance_name.clone());
        }
    }

    // See the module documentation: neither of these registers itself, so
    // requiring them to appear in the registry would be a permanent false alarm.
    let self_evidently_live: BTreeSet<String> =
        [MASTER_INSTANCE_NAME.to_string(), current_instance].into_iter().collect();

    let missing: Vec<String> =
        declared.difference(&live).filter(|name| !self_evidently_live.contains(*name)).cloned().collect();
    let unexpected: Vec<String> = live.difference(&declared).cloned().collect();

    // `master` counts as live here for the same reason as above: a stream whose
    // only configured owner is `master` is owned by the control plane, which is
    // not something the registry can prove.
    let mut live_with_master = live.clone();
    live_with_master.insert(MASTER_INSTANCE_NAME.to_string());
    let unowned_streams = unowned_streams(config, &live_with_master);

    let writer_violations = stream_writer_violations(config);

    let mut errors: Vec<String> = Vec::new();
    errors.extend(missing.iter().map(|name| {
        format!("declared instance '{name}' is not running: no active worker is registered under that name")
    }));
    errors.extend(
        stale
            .iter()
            .map(|name| format!("instance '{name}' stopped heartbeating: it is registered but not running any more")),
    );
    errors.extend(unowned_streams.iter().map(|stream| {
        format!("stream '{stream}' has no running owner: none of its configured writer instances is running")
    }));
    errors.extend(writer_violations.iter().cloned());

    // A worker registered under an undeclared name is a drift signal, not proof
    // of an outage: an operator may have registered a task runner that never
    // appears in `instance_map` on purpose.
    let warnings: Vec<String> = unexpected
        .iter()
        .map(|name| format!("instance '{name}' is running but not declared in worker.instance_map"))
        .collect();

    RuntimeTopologyReport {
        valid: errors.is_empty(),
        declared_instances: declared.into_iter().collect(),
        live_instances: live.into_iter().collect(),
        missing,
        stale: stale.into_iter().collect(),
        unexpected,
        unowned_streams,
        writer_violations,
        errors,
        warnings,
    }
}

/// The instance names the configuration declares.
fn declared_instances(config: &WorkerConfig) -> BTreeSet<String> {
    let mut declared: BTreeSet<String> = BTreeSet::new();
    declared.insert(MASTER_INSTANCE_NAME.to_string());
    declared.insert(resolved_current_instance_name(config));
    declared.extend(config.instance_map.keys().cloned());
    declared
}

/// Whether this registry row is recent enough to count as running.
///
/// A row that never carried a heartbeat is not live: a crashed worker that
/// registered but never got going must not be reported as healthy.
fn is_fresh(instance: &RegisteredInstance, now_ms: i64, heartbeat_timeout_secs: u64) -> bool {
    match instance.last_heartbeat_ts {
        Some(last_heartbeat) => now_ms.saturating_sub(last_heartbeat) <= heartbeat_timeout_secs as i64 * 1000,
        None => false,
    }
}

/// Streams whose configured owners are all absent from `live_instances`.
///
/// An empty owner list is skipped: the static validator already reports that
/// (`stream writer '…' has no configured owners`) and reporting it in two places
/// would double-count one problem.
fn unowned_streams(config: &WorkerConfig, live_instances: &BTreeSet<String>) -> Vec<String> {
    let mut unowned = Vec::new();
    for (stream, owners) in stream_writer_sets(config) {
        if owners.is_empty() {
            continue;
        }
        if owners.iter().all(|owner| !live_instances.contains(owner)) {
            unowned.push(stream.to_string());
        }
    }
    unowned
}

/// Cardinality violations of the configured stream writers.
///
/// Configuration-only: this says nothing about whether the instances are
/// running (that is [`unowned_streams`]).
fn stream_writer_violations(config: &WorkerConfig) -> Vec<String> {
    let mut violations = Vec::new();

    for (stream, owners) in stream_writer_sets(config) {
        let mut seen: BTreeSet<&String> = BTreeSet::new();
        for owner in owners {
            if !seen.insert(owner) {
                violations.push(format!("stream '{stream}' lists instance '{owner}' more than once"));
            }
        }

        let requirement =
            REQUIRED_STREAM_CARDINALITY.iter().find(|(name, _)| *name == stream).map(|(_, cardinality)| *cardinality);
        match requirement {
            Some(StreamCardinality::ExactlyOne) if owners.len() != 1 => {
                violations
                    .push(format!("stream '{stream}' must have exactly one writer instance, has {}", owners.len()));
            }
            Some(StreamCardinality::AtLeastOne) if owners.is_empty() => {
                violations.push(format!("stream '{stream}' must have at least one writer instance"));
            }
            _ => {}
        }
    }

    violations
}
