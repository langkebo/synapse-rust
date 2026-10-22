//! What write forwarding is actually doing right now (W-I stage 5, observability).
//!
//! The design document's §9 lists two gaps this module closes, and they are the same
//! gap seen from two sides:
//!
//! * `GET /_synapse/worker/v1/topology/validate` could say **which instance owns** a
//!   stream but not **where the write is sent**, nor whether that target is usable;
//! * a reader whose writer died kept reporting healthy on `/_health` — the only symptom
//!   was a 503 on the write itself, which is exactly the signal an operator sees last.
//!
//! Two sources are combined, and the difference between them matters:
//!
//! 1. **Resolved routes** — the same `Vec<(WriteStream, WriteRoute)>` the middleware
//!    obeys, published at assembly ([`install_routes`]). Reporting from the config
//!    instead would *disagree with the running process in the one case that matters*:
//!    a replication secret that failed to load turns every forward into `Unavailable`
//!    ([`super::WriteForwarding::from_config`]), while `write_route` alone still says
//!    `Forward`.
//! 2. **Observations** — the last outcome of an actual forwarded write per stream
//!    ([`record`]). This is what makes "unreachable" a fact rather than a guess; there
//!    is no background prober here on purpose (a probe would need its own scheduler,
//!    timeout policy and failure semantics, and would still be a *different* request
//!    than the one that matters).
//!
//! Registry liveness is **not** read here (this module is pure): the caller passes a
//! [`RuntimeTopologySnapshot`] when it has one, and [`StreamRoute::writer_live`] stays
//! `None` otherwise. `/_health` passes it; `/health` (the container probe, hit every few
//! seconds) deliberately does not.

use std::collections::HashMap;
use std::sync::{OnceLock, RwLock};

use serde::Serialize;

use synapse_common::config::worker::WorkerConfig;
use synapse_common::current_timestamp_millis;
use synapse_services::worker::runtime_topology::{evaluate_runtime_topology, RuntimeTopologySnapshot};
use synapse_services::worker::topology_validator::resolved_current_instance_name;
use synapse_services::worker::write_streams::{write_route, WriteRoute, WriteStream};

/// Label of `worker_write_forward_total{stream,outcome}`; also the closed set of
/// outcomes [`LastOutcome`] can hold.
///
/// Kept in one place so a dashboard cannot silently lose a series: every exit from
/// `super::forward` records exactly one of these, and the report reads the same four.
pub(super) mod label {
    /// The writer answered, and its status is relayed verbatim.
    pub(in crate::middleware::write_forwarding) const OK: &str = "ok";
    /// The writer answered with a non-2xx; the client sees that status.
    pub(in crate::middleware::write_forwarding) const UPSTREAM_ERROR: &str = "upstream_error";
    /// The transport failed (connect, TLS, timeout). Never retried.
    pub(in crate::middleware::write_forwarding) const UNREACHABLE: &str = "unreachable";
    /// The request was refused here: the configured writer is unusable (no secret, no
    /// `instance_map` entry), or the body exceeded the forward limit. The write did
    /// **not** happen.
    pub(in crate::middleware::write_forwarding) const REFUSED: &str = "refused";
}

/// The outcome of the most recent forwarded write for one stream.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum LastOutcome {
    /// Relayed successfully.
    Ok,
    /// The writer answered with an error status. **Not** a health signal: a 403 or a 429
    /// is the writer's verdict about one write, not evidence that the path is broken.
    UpstreamError,
    /// The transport failed — the writer could not be reached.
    Unreachable,
    /// Refused here: the writer is unusable, or the body exceeded the forward limit.
    /// No write happened.
    Refused,
}

impl LastOutcome {
    /// The metric label for this outcome.
    pub(super) fn label(self) -> &'static str {
        match self {
            Self::Ok => label::OK,
            Self::UpstreamError => label::UPSTREAM_ERROR,
            Self::Unreachable => label::UNREACHABLE,
            Self::Refused => label::REFUSED,
        }
    }

    /// Whether this outcome means "the write path is broken".
    ///
    /// `UpstreamError` is excluded deliberately: the writer answered, so the path works.
    /// Degrading health on it would turn every 403/429 burst into a false alarm.
    fn is_path_failure(self) -> bool {
        matches!(self, Self::Unreachable | Self::Refused)
    }
}

/// One observation: what happened, and when.
#[derive(Debug, Clone, Copy)]
struct Observation {
    outcome: LastOutcome,
    at_ms: i64,
}

static OBSERVATIONS: OnceLock<RwLock<HashMap<&'static str, Observation>>> = OnceLock::new();

fn observations() -> &'static RwLock<HashMap<&'static str, Observation>> {
    OBSERVATIONS.get_or_init(|| RwLock::new(HashMap::new()))
}

/// Record the outcome of one forwarded write. Called from the forwarding path.
///
/// A poisoned lock is skipped rather than propagated: this is telemetry, and losing an
/// observation must never fail a write.
pub(super) fn record(stream: WriteStream, outcome: LastOutcome) {
    let Ok(mut map) = observations().write() else {
        return;
    };
    map.insert(stream.stream_name(), Observation { outcome, at_ms: current_timestamp_millis() });
}

/// A snapshot of the observation registry, taken once per report.
fn observation_snapshot() -> HashMap<&'static str, Observation> {
    observations().read().map(|map| map.clone()).unwrap_or_default()
}

/// The non-local routes the middleware obeys, as `(stream, route)` pairs.
pub type StreamRoutes = Vec<(WriteStream, WriteRoute)>;

static ROUTES: OnceLock<RwLock<Option<StreamRoutes>>> = OnceLock::new();

fn routes_slot() -> &'static RwLock<Option<StreamRoutes>> {
    ROUTES.get_or_init(|| RwLock::new(None))
}

/// Publish the routes the middleware actually obeys.
///
/// Called from the server assembly (`synapse_rust::server::router`) rather than from
/// `WriteForwarding::from_config`, for the same reason `install_process_secret` is: a test
/// that builds a policy must not mutate process-wide state.
pub fn install_routes(routes: StreamRoutes) {
    if let Ok(mut slot) = routes_slot().write() {
        *slot = Some(routes);
    }
}

/// The published routes, if the assembly has run.
fn assembled_routes() -> Option<StreamRoutes> {
    routes_slot().read().ok().and_then(|slot| slot.clone())
}

/// Where one stream's writes go, and what was last seen of them.
#[derive(Debug, Clone, Serialize)]
pub struct StreamRoute {
    /// `worker.stream_writers` key.
    pub stream: String,
    /// The configured owner instances, in configuration order.
    pub owners: Vec<String>,
    /// `local` | `forward` | `unavailable`.
    pub route: String,
    /// The instance a forwarded write goes to.
    pub target_instance: Option<String>,
    /// `scheme://host:port` of that instance, from `worker.instance_map`.
    pub target_base_url: Option<String>,
    /// Why the target is unusable, when `route == "unavailable"`.
    pub reason: Option<String>,
    /// Whether this stream needs the Redis fan-out to wake `/sync` (design doc §4.5).
    /// `true` with Redis off is a latency defect that no correctness test sees.
    pub requires_redis: bool,
    /// The last outcome of an actual forwarded write, if any was attempted since start.
    pub last_outcome: Option<LastOutcome>,
    /// Age of that observation.
    pub last_outcome_age_secs: Option<i64>,
    /// Whether the target instance is registered and heartbeating. `None` when the
    /// caller did not supply a registry snapshot (i.e. the container probe).
    pub writer_live: Option<bool>,
}

/// The full write-forwarding picture, as served by `topology/validate` and `/_health`.
#[derive(Debug, Clone, Serialize)]
pub struct WriteForwardingReport {
    /// `worker.write_forwarding && worker.enabled`.
    pub enabled: bool,
    /// This process's instance name (`worker.instance_name`, or `master`).
    pub instance_name: String,
    /// Every stream, whether local or not — an operator asking "why did this write go
    /// there" needs the local ones too.
    pub streams: Vec<StreamRoute>,
    /// Streams that require the Redis fan-out but are forwarded with Redis off.
    pub notify_without_redis: Vec<String>,
}

impl WriteForwardingReport {
    /// `disabled` when the feature is off, `degraded` when any write path is broken,
    /// `healthy` otherwise.
    ///
    /// Three things degrade it, and each one is a fact about *this* process rather than
    /// about the configuration: an unusable target, a write that actually failed at the
    /// transport layer, and a notify-bound stream forwarded with Redis off (writes work,
    /// but every long-poll on this instance waits out its own timeout).
    pub fn status(&self) -> &'static str {
        if !self.enabled {
            return "disabled";
        }
        if !self.notify_without_redis.is_empty() {
            return "degraded";
        }
        if self.streams.iter().any(|stream| stream.route == "unavailable") {
            return "degraded";
        }
        // Only present when the caller supplied a registry snapshot. A target that is not
        // registered (or stopped heartbeating) is broken *now*, before any write is
        // attempted — this is the check that used to require a user to report a 503.
        if self.streams.iter().any(|stream| stream.writer_live == Some(false)) {
            return "degraded";
        }
        if self.streams.iter().any(|stream| stream.last_outcome.is_some_and(LastOutcome::is_path_failure)) {
            return "degraded";
        }
        "healthy"
    }

    /// One sentence for the health response, naming the reason(s) when degraded.
    pub fn summary(&self) -> String {
        if !self.enabled {
            return "Write forwarding is off: every instance executes the writes it receives".to_string();
        }
        let mut problems = Vec::new();
        let unavailable: Vec<&str> = self
            .streams
            .iter()
            .filter(|stream| stream.route == "unavailable")
            .map(|stream| stream.stream.as_str())
            .collect();
        if !unavailable.is_empty() {
            problems.push(format!("unusable writer for {}", unavailable.join(", ")));
        }
        let not_running: Vec<String> = self
            .streams
            .iter()
            .filter(|stream| stream.writer_live == Some(false))
            .filter_map(|stream| {
                stream.target_instance.as_ref().map(|instance| format!("{}->{instance}", stream.stream))
            })
            .collect();
        if !not_running.is_empty() {
            problems.push(format!("writer not running for {}", not_running.join(", ")));
        }
        let failing: Vec<&str> = self
            .streams
            .iter()
            .filter(|stream| stream.last_outcome.is_some_and(LastOutcome::is_path_failure))
            .map(|stream| stream.stream.as_str())
            .collect();
        if !failing.is_empty() {
            problems.push(format!("last forward failed for {}", failing.join(", ")));
        }
        if !self.notify_without_redis.is_empty() {
            problems.push(format!(
                "redis.enabled = false but {} is forwarded to another instance",
                self.notify_without_redis.join(", ")
            ));
        }
        if problems.is_empty() {
            return "All forwarded streams reach their configured writer".to_string();
        }
        problems.join("; ")
    }
}

/// Build the report for `config`.
///
/// `snapshot` is the worker registry when the caller has one (see the module docs);
/// `redis_enabled` is `redis.enabled`, which is not part of [`WorkerConfig`].
pub fn build_report(
    config: &WorkerConfig,
    redis_enabled: bool,
    snapshot: Option<&RuntimeTopologySnapshot>,
) -> WriteForwardingReport {
    build_report_with(config, redis_enabled, snapshot, assembled_routes(), &observation_snapshot())
}

/// [`build_report`] with the two process-wide inputs injected.
///
/// Exists so the tests can drive every branch without touching globals: a test that
/// installed routes or recorded an outcome would otherwise leak into every other test in
/// the same binary (nextest hides this by running one process per test; `cargo test` does
/// not).
fn build_report_with(
    config: &WorkerConfig,
    redis_enabled: bool,
    snapshot: Option<&RuntimeTopologySnapshot>,
    routes: Option<StreamRoutes>,
    observations: &HashMap<&'static str, Observation>,
) -> WriteForwardingReport {
    let enabled = config.write_forwarding && config.enabled;
    let routes = routes.unwrap_or_else(|| resolved_routes(config));

    // Registry liveness, when the caller has a snapshot. `live_instances` deliberately
    // excludes `master` and the current instance (neither registers itself in every
    // deployment), so both are added back here — the same adjustment
    // `evaluate_runtime_topology` makes for its own `unowned_streams`.
    let live: Option<std::collections::HashSet<String>> = snapshot.map(|snapshot| {
        let mut live: std::collections::HashSet<String> =
            evaluate_runtime_topology(config, snapshot).live_instances.into_iter().collect();
        live.insert("master".to_string());
        live.insert(resolved_current_instance_name(config));
        live
    });

    let now_ms = current_timestamp_millis();
    let mut notify_without_redis = Vec::new();
    let streams: Vec<StreamRoute> = WriteStream::all()
        .iter()
        .copied()
        .map(|stream| {
            let route = routes
                .iter()
                .find(|(candidate, _)| *candidate == stream)
                .map_or(WriteRoute::Local, |(_, route)| route.clone());
            let observation = observations.get(stream.stream_name()).copied();
            let (route_name, target_instance, target_base_url, reason, target) = match &route {
                WriteRoute::Local => ("local", None, None, None, None),
                WriteRoute::Forward { instance, base_url } => {
                    ("forward", Some(instance.clone()), Some(base_url.clone()), None, Some(instance.clone()))
                }
                WriteRoute::Unavailable { instance, reason } => {
                    ("unavailable", Some(instance.clone()), None, Some(reason.clone()), Some(instance.clone()))
                }
            };
            let requires_redis = stream.notify_bound();
            if enabled && requires_redis && !redis_enabled && route_name == "forward" {
                notify_without_redis.push(stream.stream_name().to_string());
            }
            StreamRoute {
                stream: stream.stream_name().to_string(),
                owners: stream.owners(config).to_vec(),
                route: route_name.to_string(),
                target_instance,
                target_base_url,
                reason,
                requires_redis,
                last_outcome: observation.map(|observation| observation.outcome),
                last_outcome_age_secs: observation.map(|observation| now_ms.saturating_sub(observation.at_ms) / 1000),
                writer_live: match (live.as_ref(), target.as_deref()) {
                    (Some(live), Some(instance)) if route_name != "local" => Some(live.contains(instance)),
                    _ => None,
                },
            }
        })
        .collect();

    WriteForwardingReport {
        enabled,
        instance_name: resolved_current_instance_name(config),
        streams,
        notify_without_redis,
    }
}

/// The routes the configuration implies, for a process that never ran the assembly
/// (tests, or an embedding that installs the router by hand).
fn resolved_routes(config: &WorkerConfig) -> StreamRoutes {
    WriteStream::all()
        .iter()
        .copied()
        .map(|stream| (stream, write_route(config, stream)))
        .filter(|(_, route)| !matches!(route, WriteRoute::Local))
        .collect()
}

#[cfg(test)]
mod tests;
