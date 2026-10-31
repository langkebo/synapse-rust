use crate::routes::context::AdminContext;
use crate::routes::extractors::MatrixPath;
use crate::routes::extractors::MatrixQuery;
use axum::{
    extract::State,
    http::StatusCode,
    middleware,
    response::IntoResponse,
    routing::{delete, get, post, put},
    Json, Router,
};
use serde::{Deserialize, Serialize};
use std::str::FromStr;

use crate::middleware::replication_http_auth_middleware;
use crate::routes::response_helpers::{created_json_from, json_from, json_vec_from, require_found, status_json};
use crate::routes::MatrixJson;
use crate::routes::{AdminUser, AppState};
use synapse_common::ApiError;
use synapse_services::worker::topology_validator::{may_serve_delayed_events_route, DELAYED_EVENTS_WORKER_PATHS};
use synapse_services::worker::types::*;

/// Worker self-announcement on the heartbeat (W-G).
pub mod worker_self_announce;
/// The topology endpoints (W-A): the static catalogue plus the live registry.
pub mod worker_topology;

/// The `RegisterWorkerBody` struct.
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RegisterWorkerBody {
    /// The `worker_id` field.
    pub worker_id: String,
    /// The `worker_name` field.
    pub worker_name: String,
    /// The `worker_type` field.
    pub worker_type: String,
    /// The `host` field.
    pub host: String,
    /// The `port` field.
    pub port: u16,
    /// The `config` field.
    pub config: Option<serde_json::Value>,
    /// The `metadata` field.
    pub metadata: Option<serde_json::Value>,
    /// The `version` field.
    pub version: Option<String>,
}

impl RegisterWorkerBody {
    fn into_request(self) -> Result<RegisterWorkerRequest, ApiError> {
        let worker_type = WorkerType::from_str(&self.worker_type).map_err(ApiError::bad_request)?;

        Ok(RegisterWorkerRequest {
            worker_id: self.worker_id,
            worker_name: self.worker_name,
            worker_type,
            host: self.host,
            port: self.port,
            config: self.config,
            metadata: self.metadata,
            version: self.version,
        })
    }
}

/// The `HeartbeatBody` struct.
///
/// Defined in [`worker_self_announce`] (W-G) next to the rule that consumes its
/// identity fields; re-exported so the path used by callers stays the same.
pub use worker_self_announce::HeartbeatBody;

/// The `SendCommandBody` struct.
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SendCommandBody {
    /// The `command_type` field.
    pub command_type: String,
    /// The `command_data` field.
    pub command_data: serde_json::Value,
    /// The `priority` field.
    pub priority: Option<i32>,
    /// The `max_retries` field.
    pub max_retries: Option<i32>,
}

impl SendCommandBody {
    fn into_request(self, target_worker_id: String) -> SendCommandRequest {
        SendCommandRequest {
            target_worker_id,
            command_type: self.command_type,
            command_data: self.command_data,
            priority: self.priority,
            max_retries: self.max_retries,
        }
    }
}

/// The `AssignTaskBody` struct.
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AssignTaskBody {
    /// The `task_type` field.
    pub task_type: String,
    /// The `task_data` field.
    pub task_data: serde_json::Value,
    /// The `priority` field.
    pub priority: Option<i32>,
    /// The `preferred_worker_id` field.
    pub preferred_worker_id: Option<String>,
}

impl AssignTaskBody {
    fn into_request(self) -> AssignTaskRequest {
        AssignTaskRequest {
            task_type: self.task_type,
            task_data: self.task_data,
            priority: self.priority,
            preferred_worker_id: self.preferred_worker_id,
        }
    }
}

/// The `ConnectWorkerBody` struct.
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ConnectWorkerBody {
    /// The `address` field.
    pub address: String,
}

/// The `CompleteTaskBody` struct.
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CompleteTaskBody {
    /// The `result` field.
    pub result: Option<serde_json::Value>,
}

/// The `FailTaskBody` struct.
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct FailTaskBody {
    /// The `error` field.
    pub error: String,
}

/// The `QueryLimit` struct.
///
/// Internal worker protocol: the only callers are this project's own workers,
/// so an unexpected query parameter is a caller bug and is rejected.
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct QueryLimit {
    /// The `limit` field.
    pub limit: Option<i64>,
}

/// The `QueryStream` struct.
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct QueryStream {
    /// The `stream_id` field.
    pub stream_id: Option<i64>,
}

/// The `QueryPosition` struct.
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct QueryPosition {
    /// The `stream_name` field.
    pub stream_name: String,
}

/// The `WorkerResponse` struct.
#[derive(Debug, Serialize)]
pub struct WorkerResponse {
    /// The `id` field.
    pub id: i64,
    /// The `worker_id` field.
    pub worker_id: String,
    /// The `worker_name` field.
    pub worker_name: String,
    /// The `worker_type` field.
    pub worker_type: String,
    /// The `instance_map_keys` field.
    pub instance_map_keys: Vec<String>,
    /// The `responsibility_domains` field.
    pub responsibility_domains: Vec<String>,
    /// The `owned_route_prefixes` field.
    pub owned_route_prefixes: Vec<String>,
    /// The `delayed_events_paths` field.
    ///
    /// The MSC4140 delayed-events paths this worker type may serve, as admitted
    /// by [`may_serve_delayed_events_route`]. Unlike `owned_route_prefixes`
    /// (which is the coarse `/_matrix/client/*` prefix), this is the explicit
    /// family path set — the prefix alone would also admit `/login`,
    /// `/register`, admin and stream-writer endpoints.
    pub delayed_events_paths: Vec<String>,
    /// The `replication_streams` field.
    pub replication_streams: Vec<String>,
    /// The `capabilities` field.
    pub capabilities: WorkerCapabilities,
    /// The `host` field.
    pub host: String,
    /// The `port` field.
    pub port: i32,
    /// The `status` field.
    pub status: String,
    /// The `last_heartbeat_ts` field.
    pub last_heartbeat_ts: Option<i64>,
    /// The `started_ts` field.
    pub started_ts: i64,
}

impl From<WorkerInfo> for WorkerResponse {
    fn from(w: WorkerInfo) -> Self {
        let worker_type = WorkerType::from_str(&w.worker_type).ok();

        Self {
            id: w.id,
            worker_id: w.worker_id,
            worker_name: w.worker_name,
            worker_type: w.worker_type,
            instance_map_keys: worker_type
                .map(|worker_type| worker_type.instance_map_keys().iter().map(|value| (*value).to_string()).collect())
                .unwrap_or_default(),
            responsibility_domains: worker_type
                .map(|worker_type| {
                    worker_type.responsibility_domains().iter().map(|value| (*value).to_string()).collect()
                })
                .unwrap_or_default(),
            owned_route_prefixes: worker_type
                .map(|worker_type| {
                    worker_type.owned_route_prefixes().iter().map(|value| (*value).to_string()).collect()
                })
                .unwrap_or_default(),
            // The family gate is applied here so the worker-info response reports
            // exactly the delayed-events paths the runtime gate admits.
            delayed_events_paths: DELAYED_EVENTS_WORKER_PATHS
                .iter()
                .copied()
                .filter(|path| may_serve_delayed_events_route(path))
                .map(str::to_string)
                .collect(),
            replication_streams: worker_type
                .map(|worker_type| worker_type.replication_streams().iter().map(|value| (*value).to_string()).collect())
                .unwrap_or_default(),
            capabilities: worker_type.map(|worker_type| WorkerCapabilities::for_type(&worker_type)).unwrap_or_default(),
            host: w.host,
            port: w.port,
            status: w.status,
            last_heartbeat_ts: w.last_heartbeat_ts,
            started_ts: w.started_ts,
        }
    }
}

/// The `WorkerCommandResponse` struct.
#[derive(Debug, Serialize)]
pub struct WorkerCommandResponse {
    /// The `command_id` field.
    pub command_id: String,
    /// The `target_worker_id` field.
    pub target_worker_id: String,
    /// The `command_type` field.
    pub command_type: String,
    /// The `status` field.
    pub status: String,
    /// The `created_ts` field.
    pub created_ts: i64,
}

impl From<WorkerCommand> for WorkerCommandResponse {
    fn from(c: WorkerCommand) -> Self {
        Self {
            command_id: c.command_id,
            target_worker_id: c.target_worker_id,
            command_type: c.command_type,
            status: c.status,
            created_ts: c.created_ts,
        }
    }
}

/// The `WorkerTaskResponse` struct.
#[derive(Debug, Serialize)]
pub struct WorkerTaskResponse {
    /// The `task_id` field.
    pub task_id: String,
    /// The `task_type` field.
    pub task_type: String,
    /// The `status` field.
    pub status: String,
    /// The `assigned_worker_id` field.
    pub assigned_worker_id: Option<String>,
}

impl From<WorkerTaskAssignment> for WorkerTaskResponse {
    fn from(t: WorkerTaskAssignment) -> Self {
        Self { task_id: t.task_id, task_type: t.task_type, status: t.status, assigned_worker_id: t.assigned_worker_id }
    }
}

/// See [`register_worker`].
pub async fn register_worker(
    State(ctx): State<AdminContext>,
    _admin_user: AdminUser,
    MatrixJson(body): MatrixJson<RegisterWorkerBody>,
) -> Result<impl IntoResponse, ApiError> {
    let request: RegisterWorkerRequest = body.into_request()?;

    let worker: WorkerInfo = ctx.worker_manager.register(request).await?;

    Ok(created_json_from::<_, WorkerResponse>(worker))
}

/// See [`get_worker`].
pub async fn get_worker(
    State(ctx): State<AdminContext>,
    MatrixPath(worker_id): MatrixPath<String>,
    _admin_user: AdminUser,
) -> Result<impl IntoResponse, ApiError> {
    let worker: Option<WorkerInfo> = ctx.worker_manager.get(&worker_id).await?;

    Ok(json_from::<_, WorkerResponse>(require_found(worker, "Worker not found")?))
}

/// See [`list_workers`].
pub async fn list_workers(
    State(ctx): State<AdminContext>,
    _admin_user: AdminUser,
) -> Result<impl IntoResponse, ApiError> {
    let workers: Vec<WorkerInfo> = ctx.worker_manager.get_active().await?;

    Ok(json_vec_from::<_, WorkerResponse>(workers))
}

/// See [`list_workers_by_type`].
pub async fn list_workers_by_type(
    State(ctx): State<AdminContext>,
    MatrixPath(worker_type): MatrixPath<String>,
    _admin_user: AdminUser,
) -> Result<impl IntoResponse, ApiError> {
    let wtype: WorkerType = WorkerType::from_str(&worker_type).map_err(ApiError::bad_request)?;

    let workers: Vec<WorkerInfo> = ctx.worker_manager.get_by_type(wtype).await?;

    Ok(json_vec_from::<_, WorkerResponse>(workers))
}

/// See [`heartbeat`].
pub async fn heartbeat(
    State(ctx): State<AdminContext>,
    MatrixPath(worker_id): MatrixPath<String>,
    MatrixJson(body): MatrixJson<HeartbeatBody>,
) -> Result<impl IntoResponse, ApiError> {
    let status: WorkerStatus = WorkerStatus::from_str(&body.status).map_err(ApiError::bad_request)?;

    // W-G: a worker holding only the shared secret had no way to create its registry
    // row, and the status UPDATE below silently changes 0 rows for an unknown worker.
    worker_self_announce::announce_if_unknown(&ctx, &worker_id, &body).await?;

    ctx.worker_manager.heartbeat(&worker_id, status, body.load_stats).await?;

    Ok(status_json("ok"))
}

/// See [`unregister_worker`].
pub async fn unregister_worker(
    State(ctx): State<AdminContext>,
    MatrixPath(worker_id): MatrixPath<String>,
    _admin_user: AdminUser,
) -> Result<impl IntoResponse, ApiError> {
    ctx.worker_manager.unregister(&worker_id).await?;

    Ok(StatusCode::NO_CONTENT)
}

/// See [`send_command`].
pub async fn send_command(
    State(ctx): State<AdminContext>,
    MatrixPath(worker_id): MatrixPath<String>,
    _admin_user: AdminUser,
    MatrixJson(body): MatrixJson<SendCommandBody>,
) -> Result<impl IntoResponse, ApiError> {
    let request: SendCommandRequest = body.into_request(worker_id);

    let command: WorkerCommand = ctx.worker_manager.send_command(request).await?;

    Ok(created_json_from::<_, WorkerCommandResponse>(command))
}

/// See [`get_pending_commands`].
pub async fn get_pending_commands(
    State(ctx): State<AdminContext>,
    MatrixPath(worker_id): MatrixPath<String>,
    MatrixQuery(query): MatrixQuery<QueryLimit>,
) -> Result<impl IntoResponse, ApiError> {
    let limit: i64 = query.limit.unwrap_or(100);
    let commands: Vec<WorkerCommand> = ctx.worker_manager.get_pending_commands(&worker_id, limit).await?;

    // `{commands: [...]}` (not a bare array): the worker-body client reads the
    // `commands` field, so an array made every listing silently empty.
    let commands: Vec<WorkerCommandResponse> = commands.into_iter().map(WorkerCommandResponse::from).collect();
    let total = commands.len();
    Ok(Json(serde_json::json!({ "commands": commands, "total": total })))
}

/// See [`complete_command`].
pub async fn complete_command(
    State(ctx): State<AdminContext>,
    MatrixPath(command_id): MatrixPath<String>,
) -> Result<impl IntoResponse, ApiError> {
    ctx.worker_manager.complete_command(&command_id).await?;

    Ok(status_json("completed"))
}

/// See [`fail_command`].
pub async fn fail_command(
    State(ctx): State<AdminContext>,
    MatrixPath(command_id): MatrixPath<String>,
    MatrixJson(body): MatrixJson<FailTaskBody>,
) -> Result<impl IntoResponse, ApiError> {
    ctx.worker_manager.fail_command(&command_id, &body.error).await?;

    Ok(status_json("failed"))
}

/// See [`assign_task`].
pub async fn assign_task(
    State(ctx): State<AdminContext>,
    _admin_user: AdminUser,
    MatrixJson(body): MatrixJson<AssignTaskBody>,
) -> Result<impl IntoResponse, ApiError> {
    let request: AssignTaskRequest = body.into_request();

    let task: WorkerTaskAssignment = ctx.worker_manager.assign_task(request).await?;

    Ok(created_json_from::<_, WorkerTaskResponse>(task))
}

/// See [`get_pending_tasks`].
pub async fn get_pending_tasks(
    State(ctx): State<AdminContext>,
    _admin_user: AdminUser,
    MatrixQuery(query): MatrixQuery<QueryLimit>,
) -> Result<impl IntoResponse, ApiError> {
    let limit: i64 = query.limit.unwrap_or(100);
    let tasks: Vec<WorkerTaskAssignment> = ctx.worker_manager.get_pending_tasks(limit).await?;

    // `{tasks: [...]}` (not a bare array): both worker clients read the `tasks`
    // field, so an array made every listing silently empty.
    let tasks: Vec<WorkerTaskResponse> = tasks.into_iter().map(WorkerTaskResponse::from).collect();
    let total = tasks.len();
    Ok(Json(serde_json::json!({ "tasks": tasks, "total": total })))
}

/// See [`claim_next_task`].
pub async fn claim_next_task(
    State(ctx): State<AdminContext>,
    MatrixPath(worker_id): MatrixPath<String>,
    _admin_user: AdminUser,
) -> Result<impl IntoResponse, ApiError> {
    let task: WorkerTaskAssignment = ctx.worker_manager.claim_next_pending_task(&worker_id).await?;

    Ok(Json(WorkerTaskResponse::from(task)))
}

/// See [`claim_task`].
pub async fn claim_task(
    State(ctx): State<AdminContext>,
    MatrixPath((task_id, worker_id)): MatrixPath<(String, String)>,
    _admin_user: AdminUser,
) -> Result<impl IntoResponse, ApiError> {
    ctx.worker_manager.claim_task(&task_id, &worker_id).await?;

    Ok(status_json("claimed"))
}

/// See [`complete_task`].
pub async fn complete_task(
    State(ctx): State<AdminContext>,
    MatrixPath(task_id): MatrixPath<String>,
    MatrixJson(body): MatrixJson<CompleteTaskBody>,
) -> Result<impl IntoResponse, ApiError> {
    ctx.worker_manager.complete_task(&task_id, body.result).await?;

    Ok(status_json("completed"))
}

/// See [`fail_task`].
pub async fn fail_task(
    State(ctx): State<AdminContext>,
    MatrixPath(task_id): MatrixPath<String>,
    MatrixJson(body): MatrixJson<FailTaskBody>,
) -> Result<impl IntoResponse, ApiError> {
    ctx.worker_manager.fail_task(&task_id, &body.error).await?;

    Ok(status_json("failed"))
}

/// See [`connect_worker`].
pub async fn connect_worker(
    State(ctx): State<AdminContext>,
    MatrixPath(worker_id): MatrixPath<String>,
    MatrixJson(body): MatrixJson<ConnectWorkerBody>,
) -> Result<impl IntoResponse, ApiError> {
    ctx.worker_manager.connect_to_worker(&worker_id, &body.address).await?;

    Ok(status_json("connected"))
}

/// See [`disconnect_worker`].
pub async fn disconnect_worker(
    State(ctx): State<AdminContext>,
    MatrixPath(worker_id): MatrixPath<String>,
) -> Result<impl IntoResponse, ApiError> {
    ctx.worker_manager.disconnect_from_worker(&worker_id).await?;

    Ok(status_json("disconnected"))
}

/// See [`get_replication_position`].
pub async fn get_replication_position(
    State(ctx): State<AdminContext>,
    MatrixPath(worker_id): MatrixPath<String>,
    MatrixQuery(query): MatrixQuery<QueryPosition>,
) -> Result<impl IntoResponse, ApiError> {
    let position_opt: Option<i64> = ctx.worker_manager.get_replication_position(&worker_id, &query.stream_name).await?;
    let position = position_opt.unwrap_or(0);

    Ok(Json(serde_json::json!({
        "worker_id": worker_id,
        "stream_name": query.stream_name,
        "position": position
    })))
}

/// See [`update_replication_position`].
pub async fn update_replication_position(
    State(ctx): State<AdminContext>,
    MatrixPath((worker_id, stream_name)): MatrixPath<(String, String)>,
    MatrixJson(body): MatrixJson<StreamPosition>,
) -> Result<impl IntoResponse, ApiError> {
    ctx.worker_manager.update_replication_position(&worker_id, &stream_name, body.position).await?;

    Ok(status_json("updated"))
}

/// See [`get_events`].
pub async fn get_events(
    State(ctx): State<AdminContext>,
    MatrixQuery(query): MatrixQuery<QueryStream>,
) -> Result<impl IntoResponse, ApiError> {
    let stream_id: i64 = query.stream_id.unwrap_or(0);
    let limit: i64 = 100;
    let events = ctx.worker_manager.get_events_since(stream_id, limit).await?;

    Ok(Json(events))
}

/// See [`get_statistics`].
pub async fn get_statistics(
    State(ctx): State<AdminContext>,
    _admin_user: AdminUser,
    MatrixQuery(query): MatrixQuery<QueryLimit>,
) -> Result<impl IntoResponse, ApiError> {
    let limit: i64 = query.limit.unwrap_or(100).clamp(1, 1000);
    let stats = ctx.worker_manager.get_statistics(limit).await?;

    Ok(Json(stats))
}

/// See [`get_type_statistics`].
pub async fn get_type_statistics(
    State(ctx): State<AdminContext>,
    _admin_user: AdminUser,
) -> Result<impl IntoResponse, ApiError> {
    let stats = ctx.worker_manager.get_type_statistics().await?;

    Ok(Json(stats))
}

/// See [`select_worker`].
pub async fn select_worker(
    State(ctx): State<AdminContext>,
    MatrixPath(task_type): MatrixPath<String>,
    _admin_user: AdminUser,
) -> Result<impl IntoResponse, ApiError> {
    let worker_id_opt: Option<String> = ctx.worker_manager.select_worker_for_task(&task_type).await?;
    let worker_id = worker_id_opt.ok_or_else(|| ApiError::not_found("No worker found for task type"))?;

    Ok(Json(serde_json::json!({
        "task_type": task_type,
        "selected_worker": worker_id
    })))
}

/// Always-on admin surface of the worker router. Wired unconditionally by
/// `create_router`; the derived route table covers exactly these routes under
/// the `worker` label.
pub fn create_worker_admin_router(state: &AppState) -> Router<AppState> {
    Router::new()
        .route("/_synapse/worker/v1/register", post(register_worker))
        .route("/_synapse/worker/v1/workers", get(list_workers))
        .route("/_synapse/worker/v1/workers/type/{worker_type}", get(list_workers_by_type))
        .route("/_synapse/worker/v1/workers/{worker_id}", get(get_worker))
        .route("/_synapse/worker/v1/workers/{worker_id}", delete(unregister_worker))
        .route("/_synapse/worker/v1/workers/{worker_id}/commands", post(send_command))
        .route("/_synapse/worker/v1/tasks", post(assign_task))
        .route("/_synapse/worker/v1/tasks", get(get_pending_tasks))
        .route("/_synapse/worker/v1/tasks/claim/{worker_id}", post(claim_next_task))
        .route("/_synapse/worker/v1/tasks/{task_id}/claim/{worker_id}", post(claim_task))
        .route("/_synapse/worker/v1/topology", get(worker_topology::get_topology))
        .route("/_synapse/worker/v1/topology/validate", get(worker_topology::get_topology_validation))
        .route("/_synapse/worker/v1/statistics", get(get_statistics))
        .route("/_synapse/worker/v1/statistics/types", get(get_type_statistics))
        .route("/_synapse/worker/v1/select/{task_type}", get(select_worker))
        .route_layer(middleware::from_fn_with_state(
            <crate::routes::context::AdminContext as axum::extract::FromRef<crate::routes::AppState>>::from_ref(state),
            crate::middleware::admin_auth_middleware,
        ))
}

/// Conditional worker-body surface, only merged when
/// `ctx.config.worker.enabled` is true. The derived route table carries these
/// routes under the `worker_body` label at `RouteProfile::Worker` rank, so
/// they only surface for a worker-enabled profile.
pub fn create_worker_body_router(state: &AppState) -> Router<AppState> {
    Router::new()
        .route("/_synapse/worker/v1/workers/{worker_id}/heartbeat", post(heartbeat))
        .route("/_synapse/worker/v1/workers/{worker_id}/connect", post(connect_worker))
        .route("/_synapse/worker/v1/workers/{worker_id}/disconnect", post(disconnect_worker))
        .route("/_synapse/worker/v1/workers/{worker_id}/commands", get(get_pending_commands))
        .route("/_synapse/worker/v1/commands/{command_id}/complete", post(complete_command))
        .route("/_synapse/worker/v1/commands/{command_id}/fail", post(fail_command))
        .route("/_synapse/worker/v1/tasks/{task_id}/complete", post(complete_task))
        .route("/_synapse/worker/v1/tasks/{task_id}/fail", post(fail_task))
        .route("/_synapse/worker/v1/replication/{worker_id}/position", get(get_replication_position))
        .route("/_synapse/worker/v1/replication/{worker_id}/{stream_name}", put(update_replication_position))
        .route("/_synapse/worker/v1/events", get(get_events))
        .route_layer(middleware::from_fn_with_state(
            <crate::routes::context::CoreContext as axum::extract::FromRef<AppState>>::from_ref(state),
            replication_http_auth_middleware,
        ))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::routes::worker::worker_topology::{
        build_topology_validation_response, WorkerLiveInstance, WorkerTopologyResponse,
    };
    use synapse_common::config::worker::WorkerConfig;
    use synapse_services::worker::runtime_topology::{RegisteredInstance, RuntimeTopologySnapshot};

    /// A registry snapshot in which every named instance is freshly heartbeated.
    fn runtime_snapshot(instance_names: &[&str]) -> RuntimeTopologySnapshot {
        let now_ms = 1_000_000;
        RuntimeTopologySnapshot {
            registered: instance_names
                .iter()
                .map(|name| RegisteredInstance {
                    instance_name: (*name).to_string(),
                    worker_id: format!("id-{name}"),
                    status: "running".to_string(),
                    last_heartbeat_ts: Some(now_ms),
                })
                .collect(),
            now_ms,
            heartbeat_timeout_secs: 90,
        }
    }

    /// A worker-enabled configuration declaring `event_persister` as the sole
    /// writer of the event stream (the split topology's most common shape).
    fn split_configuration() -> WorkerConfig {
        let mut config = WorkerConfig { enabled: true, instance_name: "master".to_string(), ..WorkerConfig::default() };
        config.instance_map.insert(
            "event_persister".to_string(),
            synapse_common::config::worker::InstanceLocationConfig {
                host: "127.0.0.1".to_string(),
                port: 9102,
                tls: false,
            },
        );
        config.stream_writers.events = vec!["event_persister".to_string()];
        config.replication.enabled = true;
        config.replication.http.enabled = true;
        config.replication.http.secret = Some("test-secret".to_string());
        config
    }

    #[test]
    fn test_register_worker_body_into_request_parses_worker_type() {
        let body = RegisterWorkerBody {
            worker_id: "worker-1".to_string(),
            worker_name: "Worker One".to_string(),
            worker_type: "frontend".to_string(),
            host: "127.0.0.1".to_string(),
            port: 8080,
            config: Some(serde_json::json!({"mode": "active"})),
            metadata: Some(serde_json::json!({"zone": "a"})),
            version: Some("1.0.0".to_string()),
        };

        let request = body.into_request().expect("worker type should parse");

        assert_eq!(request.worker_id, "worker-1");
        assert_eq!(request.worker_name, "Worker One");
        assert_eq!(request.worker_type, WorkerType::Frontend);
        assert_eq!(request.port, 8080);
        assert_eq!(request.version.as_deref(), Some("1.0.0"));
    }

    #[test]
    fn test_register_worker_body_into_request_rejects_invalid_worker_type() {
        let body = RegisterWorkerBody {
            worker_id: "worker-1".to_string(),
            worker_name: "Worker One".to_string(),
            worker_type: "unknown".to_string(),
            host: "127.0.0.1".to_string(),
            port: 8080,
            config: None,
            metadata: None,
            version: None,
        };

        let error = body.into_request().expect_err("invalid worker type should fail");

        assert!(error.is_bad_request());
        assert!(error.internal_message().contains("unknown"));
    }

    #[test]
    fn test_send_command_body_into_request_preserves_target_and_fields() {
        let body = SendCommandBody {
            command_type: "rebalance".to_string(),
            command_data: serde_json::json!({"room_id": "!room:example.com"}),
            priority: Some(5),
            max_retries: Some(3),
        };

        let request = body.into_request("worker-a".to_string());

        assert_eq!(request.target_worker_id, "worker-a");
        assert_eq!(request.command_type, "rebalance");
        assert_eq!(request.priority, Some(5));
        assert_eq!(request.max_retries, Some(3));
    }

    #[test]
    fn test_assign_task_body_into_request_preserves_fields() {
        let body = AssignTaskBody {
            task_type: "http".to_string(),
            task_data: serde_json::json!({"path": "/sync"}),
            priority: Some(10),
            preferred_worker_id: Some("worker-b".to_string()),
        };

        let request = body.into_request();

        assert_eq!(request.task_type, "http");
        assert_eq!(request.priority, Some(10));
        assert_eq!(request.preferred_worker_id.as_deref(), Some("worker-b"));
    }

    #[test]
    fn test_worker_response_from_worker_info_includes_topology_metadata() {
        let response = WorkerResponse::from(WorkerInfo {
            id: 1,
            worker_id: "worker-1".to_string(),
            worker_name: "Worker One".to_string(),
            worker_type: "event_persister".to_string(),
            host: "127.0.0.1".to_string(),
            port: 9000,
            status: "running".to_string(),
            last_heartbeat_ts: Some(123),
            started_ts: 456,
            stopped_ts: None,
            config: serde_json::json!({}),
            metadata: serde_json::json!({}),
            version: Some("1.0.0".to_string()),
        });

        assert_eq!(response.instance_map_keys, vec!["event_persister".to_string()]);
        assert_eq!(response.responsibility_domains, vec!["event_persistence".to_string()]);
        assert_eq!(response.owned_route_prefixes, vec!["/_synapse/worker/v1/replication/*".to_string()]);
        assert_eq!(
            response.delayed_events_paths,
            vec![
                "/_matrix/client/unstable/org.matrix.msc4140/rooms/{room_id}/delayed_event/{event_type}/{txn_id}"
                    .to_string(),
                "/_matrix/client/unstable/org.matrix.msc4140/delayed_events".to_string(),
                "/_matrix/client/unstable/org.matrix.msc4140/delayed_events/{delay_id}".to_string(),
                "/_matrix/client/unstable/org.matrix.msc4140/delayed_events/{delay_id}/cancel".to_string(),
                "/_matrix/client/unstable/org.matrix.msc4140/delayed_events/{delay_id}/restart".to_string(),
                "/_matrix/client/unstable/org.matrix.msc4140/delayed_events/{delay_id}/send".to_string()
            ]
        );
        assert_eq!(response.replication_streams, vec!["events".to_string()]);
        assert!(response.capabilities.can_persist_events);
    }

    #[test]
    fn test_worker_route_table_contains_topology_endpoint() {
        let ledger = crate::routes::assembly::declared_ledger_all();
        let paths: Vec<&str> = ledger.iter().map(|entry| entry.path).collect();
        assert!(paths.contains(&"/_synapse/worker/v1/topology"));
        assert!(paths.contains(&"/_synapse/worker/v1/topology/validate"));
    }

    #[test]
    fn test_build_topology_validation_response_reports_known_instances_and_validity() {
        let mut config = WorkerConfig { enabled: true, instance_name: "master".to_string(), ..WorkerConfig::default() };
        config.instance_map.insert(
            "event_persister".to_string(),
            synapse_common::config::worker::InstanceLocationConfig {
                host: "127.0.0.1".to_string(),
                port: 9102,
                tls: false,
            },
        );
        config.stream_writers.events = vec!["event_persister".to_string()];
        config.replication.enabled = true;
        config.replication.http.enabled = true;
        config.replication.http.secret = Some("test-secret".to_string());

        let response = build_topology_validation_response(&config, true, &runtime_snapshot(&["event_persister"]));

        assert!(response.validation.valid);
        assert!(response.known_instances.iter().any(|name| name == "master"));
        assert!(response.known_instances.iter().any(|name| name == "event_persister"));
        assert!(response
            .stream_writers
            .iter()
            .any(|entry| entry.stream_name == "events" && entry.owners == vec!["event_persister".to_string()]));
        assert!(response
            .route_owner_expectations
            .iter()
            .any(|entry| entry.probe == "sync" && entry.expected_owner == "master"));
    }

    #[test]
    fn test_topology_validation_says_where_a_write_is_actually_sent() {
        // `stream_writers` says who *owns* a stream; this section says what this process
        // *does* about it. The regression it guards: a reader could report
        // `events: [event_persister]` while every forwarded write was refused because the
        // replication secret failed to load, and no endpoint said so.
        let mut config =
            WorkerConfig { enabled: true, instance_name: "client_reader".to_string(), ..WorkerConfig::default() };
        config.instance_map.insert(
            "event_persister".to_string(),
            synapse_common::config::worker::InstanceLocationConfig {
                host: "127.0.0.1".to_string(),
                port: 9102,
                tls: false,
            },
        );
        config.stream_writers.events = vec!["event_persister".to_string()];
        config.replication.enabled = true;
        config.replication.http.enabled = true;
        config.replication.http.secret = Some("test-secret".to_string());
        config.write_forwarding = true;

        // `stream_writers` defaults to `["master"]`, so every stream not listed above
        // points there; without this entry they would resolve to `Unavailable` and the
        // assertion below would be testing the wrong branch.
        config.instance_map.insert(
            "master".to_string(),
            synapse_common::config::worker::InstanceLocationConfig {
                host: "127.0.0.1".to_string(),
                port: 8008,
                tls: false,
            },
        );

        let response = build_topology_validation_response(&config, true, &runtime_snapshot(&["event_persister"]));

        let events = response
            .write_forwarding
            .streams
            .iter()
            .find(|stream| stream.stream == "events")
            .expect("events must be reported");
        assert_eq!(events.route, "forward");
        assert_eq!(events.target_instance.as_deref(), Some("event_persister"));
        assert_eq!(
            events.target_base_url.as_deref(),
            Some("http://127.0.0.1:9102"),
            "the resolved base_url is what an operator curls when a forward fails"
        );
        assert_eq!(events.writer_live, Some(true), "the registry knows the target is running");

        // A stream still owned by `master` is not silently local: `write_forwarding` is on,
        // so it is forwarded to `master` too.
        let receipts = response
            .write_forwarding
            .streams
            .iter()
            .find(|stream| stream.stream == "receipts")
            .expect("receipts must be reported");
        assert_eq!(receipts.route, "forward");
        assert_eq!(receipts.target_instance.as_deref(), Some("master"));
    }

    #[test]
    fn test_build_topology_validation_response_reports_specialized_route_owner_expectations() {
        let mut config = WorkerConfig { enabled: true, instance_name: "master".to_string(), ..WorkerConfig::default() };
        config.instance_map.insert(
            "sync_worker".to_string(),
            synapse_common::config::worker::InstanceLocationConfig {
                host: "127.0.0.1".to_string(),
                port: 8103,
                tls: false,
            },
        );
        config.instance_map.insert(
            "media_repository".to_string(),
            synapse_common::config::worker::InstanceLocationConfig {
                host: "127.0.0.1".to_string(),
                port: 8104,
                tls: false,
            },
        );
        config.instance_map.insert(
            "federation_reader".to_string(),
            synapse_common::config::worker::InstanceLocationConfig {
                host: "127.0.0.1".to_string(),
                port: 8449,
                tls: false,
            },
        );
        config.replication.enabled = true;
        config.replication.http.enabled = true;
        config.replication.http.secret = Some("test-secret".to_string());

        let response = build_topology_validation_response(
            &config,
            true,
            &runtime_snapshot(&["sync_worker", "media_repository", "federation_reader"]),
        );

        assert!(response
            .route_owner_expectations
            .iter()
            .any(|entry| entry.probe == "sync" && entry.expected_owner == "synchrotron"));
        assert!(response
            .route_owner_expectations
            .iter()
            .any(|entry| entry.probe == "media" && entry.expected_owner == "media_repository"));
        assert!(response
            .route_owner_expectations
            .iter()
            .any(|entry| entry.probe == "federation" && entry.expected_owner == "federation_reader"));
    }

    #[test]
    fn test_topology_validation_reports_a_declared_instance_that_is_not_running() {
        // The gap this endpoint previously could not see: the configuration is
        // self-consistent (`validation.valid` stays true) but the instance that
        // is supposed to write the event stream never started.
        let config = split_configuration();

        let response = build_topology_validation_response(&config, true, &runtime_snapshot(&[]));

        assert!(response.validation.valid, "the static verdict must not change with runtime state");
        assert!(!response.runtime.valid);
        assert_eq!(response.runtime.missing, vec!["event_persister".to_string()]);
        assert_eq!(response.runtime.unowned_streams, vec!["events".to_string()]);
    }

    #[test]
    fn test_topology_validation_runtime_is_valid_when_every_declared_instance_runs() {
        let config = split_configuration();

        let response = build_topology_validation_response(&config, true, &runtime_snapshot(&["event_persister"]));

        assert!(response.runtime.valid, "unexpected errors: {:?}", response.runtime.errors);
        assert_eq!(response.runtime.live_instances, vec!["event_persister".to_string()]);
        assert!(response.runtime.missing.is_empty());
    }

    #[test]
    fn test_topology_validation_leaves_the_runtime_report_empty_when_workers_are_disabled() {
        // A monolith deployment has no registry to reconcile against; the new
        // field must not turn into a permanent alarm there.
        let config = WorkerConfig::default();

        let response = build_topology_validation_response(&config, true, &runtime_snapshot(&[]));

        assert!(response.runtime.valid);
        assert!(response.runtime.errors.is_empty());
        assert!(response.runtime.declared_instances.is_empty());
    }

    #[test]
    fn test_topology_response_flattens_the_static_catalogue_and_adds_live() {
        // The pre-existing top-level fields must survive the wrapper unchanged,
        // otherwise adding `live` would break every current consumer.
        let response = WorkerTopologyResponse {
            catalogue: WorkerTopologySummary::baseline(),
            live: vec![WorkerLiveInstance {
                instance_name: "event_persister".to_string(),
                worker_id: "id-1".to_string(),
                worker_type: "event_persister".to_string(),
                status: "running".to_string(),
                last_heartbeat_ts: Some(1_000_000),
                last_heartbeat_age_secs: Some(3),
            }],
        };

        let value = serde_json::to_value(&response).expect("the topology response must serialize");

        assert!(value.get("workers").is_some(), "`workers` must stay a top-level field");
        assert!(value.get("deployment_presets").is_some(), "`deployment_presets` must stay top-level");
        assert_eq!(value["live"][0]["instance_name"], "event_persister");
        assert_eq!(value["live"][0]["last_heartbeat_age_secs"], 3);
    }
}
