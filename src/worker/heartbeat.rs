//! S4: worker-side heartbeat sender + load-stats collector.
//!
//! The homeserver exposes
//! `POST /_synapse/worker/v1/workers/{worker_id}/heartbeat`, which persists the
//! `load_stats` payload into `worker_statistics` (S1–S3). This module is the
//! missing **producer** half: it collects the metrics the worker process can
//! actually observe and POSTs them every `WorkerRuntimeConfig::heartbeat_interval_ms`
//! (see [`heartbeat_interval`] — no new interval knob is introduced).
//!
//! Authentication reuses the surface's existing mechanism: the route is gated by
//! `replication_http_auth_middleware`, which compares the `x-synapse-worker-secret`
//! header against `worker.replication.http.secret` / `secret_path`.
//!
//! # Scope (S4 only)
//!
//! There is deliberately **no** system collector here. CPU and memory stay
//! `None`: a real collector (`sysinfo` / `procfs`) is a separate step and would
//! add a dependency this change must not introduce. Only metrics with an
//! already-present source are reported; see [`collect_load_stats`].

use std::time::Duration;

use serde::Serialize;
use synapse_common::http_client::default_client;
use synapse_services::worker::{WorkerLoadStatsUpdate, WorkerStatus};
use tokio_util::sync::CancellationToken;

/// Header the server's `replication_http_auth_middleware` reads.
const SECRET_HEADER: &str = "x-synapse-worker-secret";

/// Default heartbeat interval, matching `WorkerRuntimeConfig`'s default.
const DEFAULT_HEARTBEAT_INTERVAL_MS: u64 = 5000;

/// Longest a final `stopping` heartbeat may block shutdown.
const FINAL_HEARTBEAT_TIMEOUT: Duration = Duration::from_secs(3);

/// Collect the worker's current load metrics from sources that actually exist.
///
/// `queue_length` is the Redis task queue's pending-work count — a real source
/// visible to the worker process. Every other field is `None` on purpose:
///
/// * `cpu_usage` / `memory_usage` need a real system collector (separate step);
///   this change must not add `sysinfo`/`procfs`, so nothing is invented.
/// * `active_connections` / `requests_per_second` / `average_latency_ms` are not
///   tracked anywhere in the worker process today.
///
/// The input is a parameter (not a global) so the mapping is trivially testable.
pub fn collect_load_stats(queue_length: Option<u64>) -> WorkerLoadStatsUpdate {
    WorkerLoadStatsUpdate {
        cpu_usage: None,
        memory_usage: None,
        active_connections: None,
        requests_per_second: None,
        average_latency_ms: None,
        // A queue depth that does not fit `INTEGER` is reported as "unknown"
        // rather than truncated — `worker_statistics.queue_depth` is an i32.
        queue_depth: queue_length.and_then(|depth| i32::try_from(depth).ok()),
    }
}

/// Trim the shared replication secret and treat blank as absent.
///
/// `None` means "do not send heartbeats": the caller logs a startup warning and
/// skips the sender instead of crashing.
pub fn resolve_secret(secret: Option<String>) -> Option<String> {
    secret.map(|value| value.trim().to_string()).filter(|value| !value.is_empty())
}

/// Resolve the control-plane base URL the heartbeat is POSTed to.
///
/// `SYNAPSE_WORKER_BASE_URL` wins when set; otherwise the main listener is
/// assumed to be local on `server_port` (the worker's own `server.host` is
/// typically `0.0.0.0`, which is not a connectable target).
pub fn resolve_base_url(configured: Option<String>, server_port: u16) -> String {
    configured
        .map(|value| value.trim().trim_end_matches('/').to_string())
        .filter(|value| !value.is_empty())
        .unwrap_or_else(|| format!("http://127.0.0.1:{server_port}"))
}

/// Heartbeat payload. Mirrors the route's `HeartbeatBody` — no `worker_id`
/// field, which arrives in the path and is rejected by `deny_unknown_fields` if
/// sent in the body.
///
/// The four identity fields (W-G) are what let the route create the registry row on
/// first contact; they are `Option` so the payload stays valid against a master that
/// predates them, and so a worker with an unset identity keeps heartbeating.
#[derive(Debug, Serialize)]
struct HeartbeatPayload<'a> {
    status: &'a str,
    load_stats: Option<WorkerLoadStatsUpdate>,
    #[serde(skip_serializing_if = "Option::is_none")]
    worker_name: Option<&'a str>,
    #[serde(skip_serializing_if = "Option::is_none")]
    worker_type: Option<&'a str>,
    #[serde(skip_serializing_if = "Option::is_none")]
    host: Option<&'a str>,
    #[serde(skip_serializing_if = "Option::is_none")]
    port: Option<u16>,
}

/// The identity a worker announces about itself on every heartbeat (W-G).
///
/// `worker_name` must be the **configured instance name**
/// (`resolved_current_instance_name`) and `worker_type` the type `WorkerType::from_str`
/// accepts — the route feeds the former to `worker_name` and parses the latter.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct WorkerIdentity {
    /// The instance name this process is configured as.
    pub worker_name: String,
    /// The worker type it serves, in `WorkerType::as_str` spelling.
    pub worker_type: String,
    /// Where it can be reached (a `synapse_worker` only serves `/metrics`).
    pub host: Option<String>,
    /// See [`WorkerIdentity::host`].
    pub port: Option<u16>,
}

/// POSTs worker heartbeats to the homeserver's worker surface.
#[derive(Debug, Clone)]
pub struct HeartbeatSender {
    client: reqwest::Client,
    base_url: String,
    worker_id: String,
    secret: String,
    interval: Duration,
    identity: WorkerIdentity,
}

impl HeartbeatSender {
    /// Build a sender from an already-resolved base URL and non-blank secret.
    ///
    /// Uses the process-wide shared `reqwest` client (connection pool + bounded
    /// timeouts) rather than constructing a new client per request.
    pub fn new(base_url: String, worker_id: String, secret: String, interval: Duration) -> Self {
        Self { client: default_client(), base_url, worker_id, secret, interval, identity: WorkerIdentity::default() }
    }

    /// Announce this worker's identity on every heartbeat (W-G).
    ///
    /// Without it the route cannot create the registry row (see
    /// `synapse-web/src/routes/worker/worker_self_announce.rs`), so the worker stays
    /// invisible to the topology despite beating happily: `update_worker_status` is a
    /// plain UPDATE that silently changes zero rows for an unknown worker.
    ///
    /// Builder rather than another `new` parameter so existing constructors and their
    /// tests keep working unchanged.
    #[must_use]
    pub fn with_identity(mut self, identity: WorkerIdentity) -> Self {
        self.identity = identity;
        self
    }

    /// The identity currently announced (empty by default).
    pub fn identity(&self) -> &WorkerIdentity {
        &self.identity
    }

    /// The full heartbeat URL for this worker.
    pub fn endpoint(&self) -> String {
        format!("{}/_synapse/worker/v1/workers/{}/heartbeat", self.base_url, self.worker_id)
    }

    /// The payload for one heartbeat, identity fields included when announced (W-G).
    ///
    /// Split from [`send`] so the wire shape is unit-testable without an HTTP server.
    /// The one property that matters most: when nothing is announced the identity keys
    /// are **absent**, not null — the route deserializes with `deny_unknown_fields`, and
    /// a pre-W-G master would reject keys it does not know.
    fn payload(&self, status: &'static str, load_stats: Option<WorkerLoadStatsUpdate>) -> HeartbeatPayload<'_> {
        let announced_name = self.identity.worker_name.trim();
        let announced_type = self.identity.worker_type.trim();
        HeartbeatPayload {
            status,
            load_stats,
            worker_name: (!announced_name.is_empty()).then_some(announced_name),
            worker_type: (!announced_type.is_empty()).then_some(announced_type),
            host: self.identity.host.as_deref(),
            port: self.identity.port,
        }
    }

    /// Send one heartbeat.
    ///
    /// Returns `Err` for transport failures and for **any** non-2xx response —
    /// the route deliberately returns 500 when persisting `load_stats` fails, so
    /// callers must log at WARN and retry on the next tick.
    pub async fn send(&self, status: WorkerStatus, load_stats: Option<WorkerLoadStatsUpdate>) -> Result<(), String> {
        let payload = self.payload(status.as_str(), load_stats);
        let endpoint = self.endpoint();

        let response = self
            .client
            .post(&endpoint)
            .header(SECRET_HEADER, &self.secret)
            .json(&payload)
            .send()
            .await
            .map_err(|error| format!("heartbeat POST {endpoint} failed: {error}"))?;

        let status_code = response.status();
        if !status_code.is_success() {
            let body = response.text().await.unwrap_or_default();
            return Err(format!("heartbeat POST {endpoint} rejected with HTTP {status_code}: {body}"));
        }

        Ok(())
    }

    /// Send heartbeats until `shutdown` is cancelled.
    ///
    /// While the process is alive the worker reports [`WorkerStatus::Running`];
    /// on cancellation a best-effort [`WorkerStatus::Stopping`] is sent (bounded
    /// by [`FINAL_HEARTBEAT_TIMEOUT`]) so the control plane can unregister the
    /// worker promptly. A failed beat only logs at WARN — the loop never panics
    /// and never aborts the worker.
    ///
    /// `collect` yields the `load_stats` for the tick; it is awaited on every
    /// tick so the caller can refresh queue metrics.
    pub async fn run<F, Fut>(self, shutdown: CancellationToken, mut collect: F)
    where
        F: FnMut() -> Fut,
        Fut: std::future::Future<Output = Option<WorkerLoadStatsUpdate>>,
    {
        let mut ticker = tokio::time::interval(self.interval);
        ticker.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);

        loop {
            tokio::select! {
                _ = shutdown.cancelled() => {
                    match tokio::time::timeout(
                        FINAL_HEARTBEAT_TIMEOUT,
                        self.send(WorkerStatus::Stopping, None),
                    )
                    .await
                    {
                        Ok(Ok(())) => tracing::info!(worker_id = %self.worker_id, "final stopping heartbeat sent"),
                        Ok(Err(error)) => {
                            tracing::warn!(worker_id = %self.worker_id, %error, "final stopping heartbeat failed");
                        }
                        Err(_) => {
                            tracing::warn!(worker_id = %self.worker_id, "final stopping heartbeat timed out");
                        }
                    }
                    break;
                }
                _ = ticker.tick() => {
                    let load_stats = collect().await;
                    match self.send(WorkerStatus::Running, load_stats).await {
                        Ok(()) => tracing::debug!(worker_id = %self.worker_id, "worker heartbeat sent"),
                        Err(error) => tracing::warn!(
                            worker_id = %self.worker_id,
                            %error,
                            "worker heartbeat failed; retrying on next tick"
                        ),
                    }
                }
            }
        }
    }
}

/// Build the sender's interval from the existing `WorkerRuntimeConfig` default.
///
/// Reuses `heartbeat_interval_ms` rather than introducing a new interval knob.
pub fn heartbeat_interval(configured_ms: Option<u64>) -> Duration {
    Duration::from_millis(configured_ms.unwrap_or(DEFAULT_HEARTBEAT_INTERVAL_MS))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn collect_load_stats_maps_queue_length_to_queue_depth() {
        let stats = collect_load_stats(Some(42));

        assert_eq!(stats.queue_depth, Some(42));
    }

    #[test]
    fn collect_load_stats_without_a_source_reports_no_queue_depth() {
        let stats = collect_load_stats(None);

        assert_eq!(stats.queue_depth, None, "a missing queue source must not be faked as 0");
    }

    #[test]
    fn collect_load_stats_leaves_cpu_and_memory_unset() {
        // S4 deliberately has no system collector; CPU/memory must stay NULL
        // until a real collector (separate step) supplies them.
        let stats = collect_load_stats(Some(7));

        assert_eq!(stats.cpu_usage, None);
        assert_eq!(stats.memory_usage, None);
    }

    #[test]
    fn collect_load_stats_leaves_untracked_metrics_unset() {
        let stats = collect_load_stats(Some(7));

        assert_eq!(stats.active_connections, None);
        assert_eq!(stats.requests_per_second, None);
        assert_eq!(stats.average_latency_ms, None);
    }

    #[test]
    fn collect_load_stats_reports_unknown_when_queue_length_overflows_i32() {
        let stats = collect_load_stats(Some(i32::MAX as u64 + 1));

        assert_eq!(stats.queue_depth, None);
    }

    #[test]
    fn collect_load_stats_accepts_i32_max() {
        let stats = collect_load_stats(Some(i32::MAX as u64));

        assert_eq!(stats.queue_depth, Some(i32::MAX));
    }

    #[test]
    fn resolve_secret_rejects_missing_and_blank_secrets() {
        assert_eq!(resolve_secret(None), None);
        assert_eq!(resolve_secret(Some(String::new())), None);
        assert_eq!(resolve_secret(Some("   \n".to_string())), None);
    }

    #[test]
    fn resolve_secret_trims_a_real_secret() {
        assert_eq!(
            resolve_secret(Some("  worker_replication_secret_2026 \n".to_string())).as_deref(),
            Some("worker_replication_secret_2026")
        );
    }

    #[test]
    fn resolve_base_url_falls_back_to_the_local_main_listener() {
        assert_eq!(resolve_base_url(None, 8008), "http://127.0.0.1:8008");
        assert_eq!(resolve_base_url(Some("   ".to_string()), 8008), "http://127.0.0.1:8008");
    }

    #[test]
    fn resolve_base_url_trims_trailing_slash_from_override() {
        assert_eq!(
            resolve_base_url(Some("https://master.example.com/".to_string()), 8008),
            "https://master.example.com"
        );
    }

    #[test]
    fn heartbeat_interval_reuses_the_runtime_config_default() {
        assert_eq!(heartbeat_interval(None), Duration::from_millis(5000));
        assert_eq!(heartbeat_interval(Some(1500)), Duration::from_millis(1500));
    }

    #[test]
    fn sender_endpoint_has_no_double_slash() {
        let sender = HeartbeatSender::new(
            resolve_base_url(Some("https://master.example.com/".to_string()), 8008),
            "worker-1".to_string(),
            "secret".to_string(),
            Duration::from_millis(5000),
        );

        assert_eq!(sender.endpoint(), "https://master.example.com/_synapse/worker/v1/workers/worker-1/heartbeat");
    }

    // === W-G: the heartbeat carries the worker's identity so the route can create the
    // registry row (a worker holding only the shared secret has no register route) ===

    fn sender_without_identity() -> HeartbeatSender {
        HeartbeatSender::new(
            "http://127.0.0.1:8008".to_string(),
            "worker-1".to_string(),
            "secret".to_string(),
            Duration::from_millis(5000),
        )
    }

    #[test]
    fn a_heartbeat_without_an_announced_identity_omits_the_identity_keys() {
        // Absent, not null: the route uses `deny_unknown_fields`, and a master that
        // predates W-G rejects keys it does not know. This is what keeps a mixed
        // deployment (new worker, old master) working.
        let sender = sender_without_identity();
        let value = serde_json::to_value(sender.payload("running", None)).expect("payload must serialize");
        let object = value.as_object().expect("payload is a JSON object");

        assert_eq!(object.get("status").and_then(|v| v.as_str()), Some("running"));
        for key in ["worker_name", "worker_type", "host", "port"] {
            assert!(
                !object.contains_key(key),
                "'{key}' must be absent (not null) when nothing is announced; got {value}"
            );
        }
    }

    #[test]
    fn an_announced_identity_is_carried_on_every_heartbeat() {
        let sender = sender_without_identity().with_identity(WorkerIdentity {
            worker_name: "event_persister".to_string(),
            worker_type: "event_persister".to_string(),
            host: Some("127.0.0.1".to_string()),
            port: Some(9091),
        });

        let value = serde_json::to_value(sender.payload("running", None)).expect("payload must serialize");

        assert_eq!(value.get("worker_name").and_then(|v| v.as_str()), Some("event_persister"));
        assert_eq!(value.get("worker_type").and_then(|v| v.as_str()), Some("event_persister"));
        assert_eq!(value.get("host").and_then(|v| v.as_str()), Some("127.0.0.1"));
        assert_eq!(value.get("port").and_then(|v| v.as_u64()), Some(9091));
    }

    #[test]
    fn a_blank_announced_name_is_treated_as_not_announced() {
        // A deployment that leaves the instance name unset must keep heartbeating
        // rather than start sending a name the route would refuse to build a row from.
        let sender = sender_without_identity().with_identity(WorkerIdentity {
            worker_name: "   ".to_string(),
            worker_type: String::new(),
            host: None,
            port: None,
        });

        let value = serde_json::to_value(sender.payload("running", None)).expect("payload must serialize");
        let object = value.as_object().expect("payload is a JSON object");

        assert!(!object.contains_key("worker_name"), "a blank name must not be announced");
        assert!(!object.contains_key("worker_type"), "a blank type must not be announced");
    }

    #[test]
    fn with_identity_is_a_builder_that_leaves_the_original_alone() {
        // `with_identity` takes `self` by value; the point of pinning it is that the
        // default (no identity) stays the default for every existing constructor call.
        let plain = sender_without_identity();
        assert_eq!(plain.identity(), &WorkerIdentity::default());

        let announced = sender_without_identity().with_identity(WorkerIdentity {
            worker_name: "sync_worker".to_string(),
            worker_type: "synchrotron".to_string(),
            host: None,
            port: None,
        });
        assert_eq!(announced.identity().worker_name, "sync_worker");
        assert_eq!(announced.identity().worker_type, "synchrotron");
    }
}
