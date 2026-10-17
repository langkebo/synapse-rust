//! The worker topology endpoints (W-A).
//!
//! `GET /_synapse/worker/v1/topology` reports the static catalogue **plus** what
//! is actually running, and `…/topology/validate` adds the reconciliation of the
//! configured topology against that registry. Both handlers live here rather than
//! in the parent `worker` module so the (already long) route file does not grow
//! past its file-size ceiling for a feature that is self-contained.

use axum::{extract::State, response::IntoResponse, Json};
use serde::Serialize;

use synapse_common::config::worker::WorkerConfig;
use synapse_common::current_timestamp_millis;
use synapse_common::ApiError;
use synapse_services::worker::runtime_topology::{
    evaluate_runtime_topology, stream_writer_sets, RegisteredInstance, RuntimeTopologyReport, RuntimeTopologySnapshot,
};
use synapse_services::worker::topology_validator::{
    expected_route_owner_for_probe, validate_worker_config, RouteOwnerProbe,
};
use synapse_services::worker::types::WorkerTopologySummary;
use synapse_services::worker::HealthCheckConfig;

use crate::routes::context::AdminContext;
use crate::routes::AdminUser;

/// The `WorkerStreamWriterOwners` struct.
#[derive(Debug, Serialize)]
pub struct WorkerStreamWriterOwners {
    /// The `stream_name` field.
    pub stream_name: String,
    /// The `owners` field.
    pub owners: Vec<String>,
}

/// The `WorkerRouteOwnerExpectation` struct.
#[derive(Debug, Serialize)]
pub struct WorkerRouteOwnerExpectation {
    /// The `probe` field.
    pub probe: String,
    /// The `path` field.
    pub path: String,
    /// The `expected_owner` field.
    pub expected_owner: String,
}

/// The `WorkerTopologyValidationResponse` struct.
#[derive(Debug, Serialize)]
pub struct WorkerTopologyValidationResponse {
    /// The `worker_enabled` field.
    pub worker_enabled: bool,
    /// The `instance_name` field.
    pub instance_name: String,
    /// The `known_instances` field.
    pub known_instances: Vec<String>,
    /// The `replication_enabled` field.
    pub replication_enabled: bool,
    /// The `replication_http_enabled` field.
    pub replication_http_enabled: bool,
    /// The `validation` field.
    pub validation: synapse_services::worker::topology_validator::TopologyValidation,
    /// The `stream_writers` field.
    pub stream_writers: Vec<WorkerStreamWriterOwners>,
    /// The `route_owner_expectations` field.
    pub route_owner_expectations: Vec<WorkerRouteOwnerExpectation>,
    /// The reconciliation of the configured topology against the **running** one
    /// (W-A). `validation` above only ever reads the configuration; this field is
    /// what says whether the declared instances are actually running.
    pub runtime: RuntimeTopologyReport,
}

/// One running worker as the registry reports it (W-A).
#[derive(Debug, Serialize)]
pub struct WorkerLiveInstance {
    /// `worker_name` — the instance name this worker registered under, i.e. what
    /// `worker.instance_map` keys are matched against.
    pub instance_name: String,
    /// `worker_id` — the per-process identity (`SYNAPSE_WORKER_ID`).
    pub worker_id: String,
    /// `worker_type`.
    pub worker_type: String,
    /// `status`.
    pub status: String,
    /// `last_heartbeat_ts`.
    pub last_heartbeat_ts: Option<i64>,
    /// Seconds since the last heartbeat, or `None` when none was ever recorded.
    pub last_heartbeat_age_secs: Option<i64>,
}

/// The `GET /_synapse/worker/v1/topology` response (W-A).
///
/// The static catalogue is **flattened**, so the pre-existing top-level fields
/// (`workers`, `deployment_presets`) keep their names and shapes and current
/// consumers keep working; `live` is purely additive.
#[derive(Debug, Serialize)]
pub struct WorkerTopologyResponse {
    /// The static catalogue of worker types and deployment presets.
    #[serde(flatten)]
    pub catalogue: WorkerTopologySummary,
    /// What is actually running right now, straight from the registry.
    pub live: Vec<WorkerLiveInstance>,
}

/// One registry read, two views of it: the operator-facing `live` list and the
/// plain-data snapshot the runtime reconciliation consumes.
///
/// Both endpoints read the registry exactly once per request. The snapshot also
/// carries the liveness threshold, which has to be the **same** one the
/// worker-selection path uses or the report and the router would disagree about
/// what "running" means: `HealthCheckConfig` is private to the checker and the
/// wiring builds it with `HealthCheckConfig::default()`
/// (`synapse-services/src/wiring/admin.rs`), so the shipped default is read here.
/// If that wiring ever passes a custom config, expose it from the checker instead
/// of letting the two drift.
async fn read_worker_registry(
    ctx: &AdminContext,
) -> Result<(Vec<WorkerLiveInstance>, RuntimeTopologySnapshot), ApiError> {
    let now_ms = current_timestamp_millis();
    let heartbeat_timeout_secs = HealthCheckConfig::default().heartbeat_timeout_secs;

    let active = ctx.worker_manager.get_active().await?;
    let mut live = Vec::with_capacity(active.len());
    let mut registered = Vec::with_capacity(active.len());
    for worker in active {
        live.push(WorkerLiveInstance {
            instance_name: worker.worker_name.clone(),
            worker_id: worker.worker_id.clone(),
            worker_type: worker.worker_type.clone(),
            status: worker.status.clone(),
            last_heartbeat_ts: worker.last_heartbeat_ts,
            last_heartbeat_age_secs: worker.last_heartbeat_ts.map(|ts| now_ms.saturating_sub(ts) / 1000),
        });
        registered.push(RegisteredInstance {
            instance_name: worker.worker_name,
            worker_id: worker.worker_id,
            status: worker.status,
            last_heartbeat_ts: worker.last_heartbeat_ts,
        });
    }

    Ok((live, RuntimeTopologySnapshot { registered, now_ms, heartbeat_timeout_secs }))
}

/// The static validation response, with the runtime reconciliation attached.
///
/// `pub(crate)` rather than private: the parent `worker` module's tests assert on
/// the response without standing up a registry.
pub(crate) fn build_topology_validation_response(
    config: &WorkerConfig,
    snapshot: &RuntimeTopologySnapshot,
) -> WorkerTopologyValidationResponse {
    let mut known_instances = vec!["master".to_string(), config.instance_name.clone()];
    known_instances.extend(config.instance_map.keys().cloned());
    known_instances.sort();
    known_instances.dedup();

    // Reuses the single stream→field mapping from the services crate: a stream
    // added to `StreamWriters` without a mapping would otherwise be invisible
    // here (and in the runtime reconciliation) at the same time.
    let stream_writers = stream_writer_sets(config)
        .into_iter()
        .map(|(stream_name, owners)| WorkerStreamWriterOwners {
            stream_name: stream_name.to_string(),
            owners: owners.to_vec(),
        })
        .collect();
    let route_owner_expectations = [RouteOwnerProbe::Sync, RouteOwnerProbe::Media, RouteOwnerProbe::Federation]
        .into_iter()
        .map(|probe| WorkerRouteOwnerExpectation {
            probe: probe.as_str().to_string(),
            path: probe.path().to_string(),
            expected_owner: expected_route_owner_for_probe(config, probe).as_str().to_string(),
        })
        .collect();

    WorkerTopologyValidationResponse {
        worker_enabled: config.enabled,
        instance_name: config.instance_name.clone(),
        known_instances,
        replication_enabled: config.replication.enabled,
        replication_http_enabled: config.replication.http.enabled,
        validation: validate_worker_config(config),
        stream_writers,
        route_owner_expectations,
        runtime: evaluate_runtime_topology(config, snapshot),
    }
}

/// See [`get_topology`].
pub async fn get_topology(
    State(ctx): State<AdminContext>,
    _admin_user: AdminUser,
) -> Result<impl IntoResponse, ApiError> {
    let (live, _) = read_worker_registry(&ctx).await?;

    Ok(Json(WorkerTopologyResponse { catalogue: WorkerTopologySummary::baseline(), live }))
}

/// See [`get_topology_validation`].
pub async fn get_topology_validation(
    State(ctx): State<AdminContext>,
    _admin_user: AdminUser,
) -> Result<impl IntoResponse, ApiError> {
    let (_, snapshot) = read_worker_registry(&ctx).await?;

    Ok(Json(build_topology_validation_response(&ctx.config.worker, &snapshot)))
}
