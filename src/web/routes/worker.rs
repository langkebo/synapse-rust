use crate::web::routes::context::AdminContext;
use axum::{
    extract::{Path, Query, State},
    http::StatusCode,
    middleware,
    response::IntoResponse,
    routing::{delete, get, post, put},
    Json, Router,
};
use serde::{Deserialize, Serialize};
use std::str::FromStr;

use crate::common::ApiError;
use crate::web::middleware::replication_http_auth_middleware;
use crate::web::routes::response_helpers::{created_json_from, json_from, json_vec_from, require_found, status_json};
use crate::web::routes::{AdminUser, AppState};
use synapse_common::config::worker::WorkerConfig;
use synapse_services::worker::types::*;

#[derive(Debug, Deserialize)]
pub struct RegisterWorkerBody {
    pub worker_id: String,
    pub worker_name: String,
    pub worker_type: String,
    pub host: String,
    pub port: u16,
    pub config: Option<serde_json::Value>,
    pub metadata: Option<serde_json::Value>,
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

#[derive(Debug, Deserialize)]
pub struct HeartbeatBody {
    pub status: String,
    pub load_stats: Option<WorkerLoadStatsUpdate>,
}

#[derive(Debug, Deserialize)]
pub struct SendCommandBody {
    pub command_type: String,
    pub command_data: serde_json::Value,
    pub priority: Option<i32>,
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

#[derive(Debug, Deserialize)]
pub struct AssignTaskBody {
    pub task_type: String,
    pub task_data: serde_json::Value,
    pub priority: Option<i32>,
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

#[derive(Debug, Deserialize)]
pub struct ConnectWorkerBody {
    pub address: String,
}

#[derive(Debug, Deserialize)]
pub struct CompleteTaskBody {
    pub result: Option<serde_json::Value>,
}

#[derive(Debug, Deserialize)]
pub struct FailTaskBody {
    pub error: String,
}

#[derive(Debug, Deserialize)]
pub struct QueryLimit {
    pub limit: Option<i64>,
}

#[derive(Debug, Deserialize)]
pub struct QueryStream {
    pub stream_id: Option<i64>,
}

#[derive(Debug, Deserialize)]
pub struct QueryPosition {
    pub stream_name: String,
}

#[derive(Debug, Serialize)]
pub struct WorkerResponse {
    pub id: i64,
    pub worker_id: String,
    pub worker_name: String,
    pub worker_type: String,
    pub instance_map_keys: Vec<String>,
    pub responsibility_domains: Vec<String>,
    pub owned_route_prefixes: Vec<String>,
    pub replication_streams: Vec<String>,
    pub capabilities: WorkerCapabilities,
    pub host: String,
    pub port: i32,
    pub status: String,
    pub last_heartbeat_ts: Option<i64>,
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

#[derive(Debug, Serialize)]
pub struct WorkerCommandResponse {
    pub command_id: String,
    pub target_worker_id: String,
    pub command_type: String,
    pub status: String,
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

#[derive(Debug, Serialize)]
pub struct WorkerTaskResponse {
    pub task_id: String,
    pub task_type: String,
    pub status: String,
    pub assigned_worker_id: Option<String>,
}

impl From<WorkerTaskAssignment> for WorkerTaskResponse {
    fn from(t: WorkerTaskAssignment) -> Self {
        Self { task_id: t.task_id, task_type: t.task_type, status: t.status, assigned_worker_id: t.assigned_worker_id }
    }
}

#[derive(Debug, Serialize)]
pub struct WorkerStreamWriterOwners {
    pub stream_name: String,
    pub owners: Vec<String>,
}

#[derive(Debug, Serialize)]
pub struct WorkerRouteOwnerExpectation {
    pub probe: String,
    pub path: String,
    pub expected_owner: String,
}

#[derive(Debug, Serialize)]
pub struct WorkerTopologyValidationResponse {
    pub worker_enabled: bool,
    pub instance_name: String,
    pub known_instances: Vec<String>,
    pub replication_enabled: bool,
    pub replication_http_enabled: bool,
    pub validation: synapse_services::worker::topology_validator::TopologyValidation,
    pub stream_writers: Vec<WorkerStreamWriterOwners>,
    pub route_owner_expectations: Vec<WorkerRouteOwnerExpectation>,
}

fn build_topology_validation_response(config: &WorkerConfig) -> WorkerTopologyValidationResponse {
    let mut known_instances = vec!["master".to_string(), config.instance_name.clone()];
    known_instances.extend(config.instance_map.keys().cloned());
    known_instances.sort();
    known_instances.dedup();

    let stream_writers = vec![
        WorkerStreamWriterOwners { stream_name: "events".to_string(), owners: config.stream_writers.events.clone() },
        WorkerStreamWriterOwners { stream_name: "typing".to_string(), owners: config.stream_writers.typing.clone() },
        WorkerStreamWriterOwners {
            stream_name: "to_device".to_string(),
            owners: config.stream_writers.to_device.clone(),
        },
        WorkerStreamWriterOwners {
            stream_name: "account_data".to_string(),
            owners: config.stream_writers.account_data.clone(),
        },
        WorkerStreamWriterOwners {
            stream_name: "receipts".to_string(),
            owners: config.stream_writers.receipts.clone(),
        },
        WorkerStreamWriterOwners {
            stream_name: "presence".to_string(),
            owners: config.stream_writers.presence.clone(),
        },
        WorkerStreamWriterOwners {
            stream_name: "push_rules".to_string(),
            owners: config.stream_writers.push_rules.clone(),
        },
        WorkerStreamWriterOwners {
            stream_name: "device_lists".to_string(),
            owners: config.stream_writers.device_lists.clone(),
        },
    ];
    let route_owner_expectations = [
        synapse_services::worker::topology_validator::RouteOwnerProbe::Sync,
        synapse_services::worker::topology_validator::RouteOwnerProbe::Media,
        synapse_services::worker::topology_validator::RouteOwnerProbe::Federation,
    ]
    .into_iter()
    .map(|probe| WorkerRouteOwnerExpectation {
        probe: probe.as_str().to_string(),
        path: probe.path().to_string(),
        expected_owner: synapse_services::worker::topology_validator::expected_route_owner_for_probe(config, probe)
            .as_str()
            .to_string(),
    })
    .collect();

    WorkerTopologyValidationResponse {
        worker_enabled: config.enabled,
        instance_name: config.instance_name.clone(),
        known_instances,
        replication_enabled: config.replication.enabled,
        replication_http_enabled: config.replication.http.enabled,
        validation: synapse_services::worker::topology_validator::validate_worker_config(config),
        stream_writers,
        route_owner_expectations,
    }
}

pub async fn register_worker(
    State(ctx): State<AdminContext>,
    _admin_user: AdminUser,
    Json(body): Json<RegisterWorkerBody>,
) -> Result<impl IntoResponse, ApiError> {
    let request: RegisterWorkerRequest = body.into_request()?;

    let worker: WorkerInfo = ctx.worker_manager.register(request).await?;

    Ok(created_json_from::<_, WorkerResponse>(worker))
}

pub async fn get_worker(
    State(ctx): State<AdminContext>,
    Path(worker_id): Path<String>,
    _admin_user: AdminUser,
) -> Result<impl IntoResponse, ApiError> {
    let worker: Option<WorkerInfo> = ctx.worker_manager.get(&worker_id).await?;

    Ok(json_from::<_, WorkerResponse>(require_found(worker, "Worker not found")?))
}

pub async fn list_workers(
    State(ctx): State<AdminContext>,
    _admin_user: AdminUser,
) -> Result<impl IntoResponse, ApiError> {
    let workers: Vec<WorkerInfo> = ctx.worker_manager.get_active().await?;

    Ok(json_vec_from::<_, WorkerResponse>(workers))
}

pub async fn list_workers_by_type(
    State(ctx): State<AdminContext>,
    Path(worker_type): Path<String>,
    _admin_user: AdminUser,
) -> Result<impl IntoResponse, ApiError> {
    let wtype: WorkerType = WorkerType::from_str(&worker_type).map_err(ApiError::bad_request)?;

    let workers: Vec<WorkerInfo> = ctx.worker_manager.get_by_type(wtype).await?;

    Ok(json_vec_from::<_, WorkerResponse>(workers))
}

pub async fn heartbeat(
    State(ctx): State<AdminContext>,
    Path(worker_id): Path<String>,
    Json(body): Json<HeartbeatBody>,
) -> Result<impl IntoResponse, ApiError> {
    let status: WorkerStatus = WorkerStatus::from_str(&body.status).map_err(ApiError::bad_request)?;

    ctx.worker_manager.heartbeat(&worker_id, status, body.load_stats).await?;

    Ok(status_json("ok"))
}

pub async fn unregister_worker(
    State(ctx): State<AdminContext>,
    Path(worker_id): Path<String>,
    _admin_user: AdminUser,
) -> Result<impl IntoResponse, ApiError> {
    ctx.worker_manager.unregister(&worker_id).await?;

    Ok(StatusCode::NO_CONTENT)
}

pub async fn send_command(
    State(ctx): State<AdminContext>,
    Path(worker_id): Path<String>,
    _admin_user: AdminUser,
    Json(body): Json<SendCommandBody>,
) -> Result<impl IntoResponse, ApiError> {
    let request: SendCommandRequest = body.into_request(worker_id);

    let command: WorkerCommand = ctx.worker_manager.send_command(request).await?;

    Ok(created_json_from::<_, WorkerCommandResponse>(command))
}

pub async fn get_pending_commands(
    State(ctx): State<AdminContext>,
    Path(worker_id): Path<String>,
    Query(query): Query<QueryLimit>,
) -> Result<impl IntoResponse, ApiError> {
    let limit: i64 = query.limit.unwrap_or(100);
    let commands: Vec<WorkerCommand> = ctx.worker_manager.get_pending_commands(&worker_id, limit).await?;

    Ok(json_vec_from::<_, WorkerCommandResponse>(commands))
}

pub async fn complete_command(
    State(ctx): State<AdminContext>,
    Path(command_id): Path<String>,
) -> Result<impl IntoResponse, ApiError> {
    ctx.worker_manager.complete_command(&command_id).await?;

    Ok(status_json("completed"))
}

pub async fn fail_command(
    State(ctx): State<AdminContext>,
    Path(command_id): Path<String>,
    Json(body): Json<FailTaskBody>,
) -> Result<impl IntoResponse, ApiError> {
    ctx.worker_manager.fail_command(&command_id, &body.error).await?;

    Ok(status_json("failed"))
}

pub async fn assign_task(
    State(ctx): State<AdminContext>,
    _admin_user: AdminUser,
    Json(body): Json<AssignTaskBody>,
) -> Result<impl IntoResponse, ApiError> {
    let request: AssignTaskRequest = body.into_request();

    let task: WorkerTaskAssignment = ctx.worker_manager.assign_task(request).await?;

    Ok(created_json_from::<_, WorkerTaskResponse>(task))
}

pub async fn get_pending_tasks(
    State(ctx): State<AdminContext>,
    _admin_user: AdminUser,
    Query(query): Query<QueryLimit>,
) -> Result<impl IntoResponse, ApiError> {
    let limit: i64 = query.limit.unwrap_or(100);
    let tasks: Vec<WorkerTaskAssignment> = ctx.worker_manager.get_pending_tasks(limit).await?;

    Ok(json_vec_from::<_, WorkerTaskResponse>(tasks))
}

pub async fn claim_next_task(
    State(ctx): State<AdminContext>,
    Path(worker_id): Path<String>,
    _admin_user: AdminUser,
) -> Result<impl IntoResponse, ApiError> {
    let task: WorkerTaskAssignment = ctx.worker_manager.claim_next_pending_task(&worker_id).await?;

    Ok(Json(WorkerTaskResponse::from(task)))
}

pub async fn claim_task(
    State(ctx): State<AdminContext>,
    Path((task_id, worker_id)): Path<(String, String)>,
    _admin_user: AdminUser,
) -> Result<impl IntoResponse, ApiError> {
    ctx.worker_manager.claim_task(&task_id, &worker_id).await?;

    Ok(status_json("claimed"))
}

pub async fn complete_task(
    State(ctx): State<AdminContext>,
    Path(task_id): Path<String>,
    Json(body): Json<CompleteTaskBody>,
) -> Result<impl IntoResponse, ApiError> {
    ctx.worker_manager.complete_task(&task_id, body.result).await?;

    Ok(status_json("completed"))
}

pub async fn fail_task(
    State(ctx): State<AdminContext>,
    Path(task_id): Path<String>,
    Json(body): Json<FailTaskBody>,
) -> Result<impl IntoResponse, ApiError> {
    ctx.worker_manager.fail_task(&task_id, &body.error).await?;

    Ok(status_json("failed"))
}

pub async fn connect_worker(
    State(ctx): State<AdminContext>,
    Path(worker_id): Path<String>,
    Json(body): Json<ConnectWorkerBody>,
) -> Result<impl IntoResponse, ApiError> {
    ctx.worker_manager.connect_to_worker(&worker_id, &body.address).await?;

    Ok(status_json("connected"))
}

pub async fn disconnect_worker(
    State(ctx): State<AdminContext>,
    Path(worker_id): Path<String>,
) -> Result<impl IntoResponse, ApiError> {
    ctx.worker_manager.disconnect_from_worker(&worker_id).await?;

    Ok(status_json("disconnected"))
}

pub async fn get_replication_position(
    State(ctx): State<AdminContext>,
    Path(worker_id): Path<String>,
    Query(query): Query<QueryPosition>,
) -> Result<impl IntoResponse, ApiError> {
    let position_opt: Option<i64> = ctx.worker_manager.get_replication_position(&worker_id, &query.stream_name).await?;
    let position = position_opt.unwrap_or(0);

    Ok(Json(serde_json::json!({
        "worker_id": worker_id,
        "stream_name": query.stream_name,
        "position": position
    })))
}

pub async fn update_replication_position(
    State(ctx): State<AdminContext>,
    Path((worker_id, stream_name)): Path<(String, String)>,
    Json(body): Json<StreamPosition>,
) -> Result<impl IntoResponse, ApiError> {
    ctx.worker_manager.update_replication_position(&worker_id, &stream_name, body.position).await?;

    Ok(status_json("updated"))
}

pub async fn get_events(
    State(ctx): State<AdminContext>,
    Query(query): Query<QueryStream>,
) -> Result<impl IntoResponse, ApiError> {
    let stream_id: i64 = query.stream_id.unwrap_or(0);
    let limit: i64 = 100;
    let events = ctx.worker_manager.get_events_since(stream_id, limit).await?;

    Ok(Json(events))
}

pub async fn get_statistics(
    State(ctx): State<AdminContext>,
    _admin_user: AdminUser,
    Query(query): Query<QueryLimit>,
) -> Result<impl IntoResponse, ApiError> {
    let limit: i64 = query.limit.unwrap_or(100).clamp(1, 1000);
    let stats = ctx.worker_manager.get_statistics(limit).await?;

    Ok(Json(stats))
}

pub async fn get_topology(_state: State<AdminContext>, _admin_user: AdminUser) -> Result<impl IntoResponse, ApiError> {
    Ok(Json(WorkerTopologySummary::baseline()))
}

pub async fn get_topology_validation(
    State(ctx): State<AdminContext>,
    _admin_user: AdminUser,
) -> Result<impl IntoResponse, ApiError> {
    Ok(Json(build_topology_validation_response(&ctx.config.worker)))
}

pub async fn get_type_statistics(
    State(ctx): State<AdminContext>,
    _admin_user: AdminUser,
) -> Result<impl IntoResponse, ApiError> {
    let stats = ctx.worker_manager.get_type_statistics().await?;

    Ok(Json(stats))
}

pub async fn select_worker(
    State(ctx): State<AdminContext>,
    Path(task_type): Path<String>,
    _admin_user: AdminUser,
) -> Result<impl IntoResponse, ApiError> {
    let worker_id_opt: Option<String> = ctx.worker_manager.select_worker_for_task(&task_type).await?;
    let worker_id = worker_id_opt.ok_or_else(|| ApiError::not_found("No worker found for task type"))?;

    Ok(Json(serde_json::json!({
        "task_type": task_type,
        "selected_worker": worker_id
    })))
}

pub fn create_worker_router(state: AppState) -> Router<AppState> {
    let admin = create_worker_admin_router(&state);
    if state.services.core.config.worker.enabled {
        admin.merge(create_worker_body_router(state.clone())).with_state(state)
    } else {
        admin.with_state(state)
    }
}

/// Always-on admin surface of the worker router. Wired unconditionally by
/// `create_router`. `worker_route_manifest()` covers exactly these routes.
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
        .route("/_synapse/worker/v1/topology", get(get_topology))
        .route("/_synapse/worker/v1/topology/validate", get(get_topology_validation))
        .route("/_synapse/worker/v1/statistics", get(get_statistics))
        .route("/_synapse/worker/v1/statistics/types", get(get_type_statistics))
        .route("/_synapse/worker/v1/select/{task_type}", get(select_worker))
        .route_layer(middleware::from_fn_with_state(<crate::web::routes::context::AdminContext as axum::extract::FromRef<crate::web::routes::AppState>>::from_ref(state), crate::web::middleware::admin_auth_middleware))
}

/// Conditional worker-body surface, only merged when
/// `ctx.config.worker.enabled` is true. Backed by
/// `worker_body_route_manifest()` and aggregated via
/// `route_module::WorkerBodyModule`.
pub fn create_worker_body_router(state: AppState) -> Router<AppState> {
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
        .route_layer(middleware::from_fn_with_state(state, replication_http_auth_middleware))
}

/// Manifest of every `(method, absolute_path)` tuple `create_worker_router`
/// **always** registers — i.e. the admin_router subset. The body subset is
/// state-gated (`config.worker.enabled`) and is reported by
/// `worker_body_route_manifest()`, aggregated through
/// `route_module::WorkerBodyModule` so the duplicate-guard and live-probe
/// test cover it whenever the feature is on.
pub fn worker_route_manifest() -> Vec<crate::web::routes::route_ledger::RouteEntry> {
    use crate::web::routes::route_ledger::RouteEntry;
    use axum::http::Method;
    [
        (Method::POST, "/_synapse/worker/v1/register"),
        (Method::GET, "/_synapse/worker/v1/workers"),
        (Method::GET, "/_synapse/worker/v1/workers/type/{worker_type}"),
        (Method::GET, "/_synapse/worker/v1/workers/{worker_id}"),
        (Method::DELETE, "/_synapse/worker/v1/workers/{worker_id}"),
        (Method::POST, "/_synapse/worker/v1/workers/{worker_id}/commands"),
        (Method::POST, "/_synapse/worker/v1/tasks"),
        (Method::GET, "/_synapse/worker/v1/tasks"),
        (Method::POST, "/_synapse/worker/v1/tasks/claim/{worker_id}"),
        (Method::POST, "/_synapse/worker/v1/tasks/{task_id}/claim/{worker_id}"),
        (Method::GET, "/_synapse/worker/v1/topology"),
        (Method::GET, "/_synapse/worker/v1/topology/validate"),
        (Method::GET, "/_synapse/worker/v1/statistics"),
        (Method::GET, "/_synapse/worker/v1/statistics/types"),
        (Method::GET, "/_synapse/worker/v1/select/{task_type}"),
    ]
    .into_iter()
    .map(|(m, p)| RouteEntry::new(m, p, "worker"))
    .collect()
}

/// Manifest for the worker body branch (heartbeat / connect / commands /
/// task completion / replication / event tail). Returned by
/// `route_module::WorkerBodyModule::manifest_for` only when
/// `config.worker.enabled` is true.
pub fn worker_body_route_manifest() -> Vec<crate::web::routes::route_ledger::RouteEntry> {
    use crate::web::routes::route_ledger::RouteEntry;
    use axum::http::Method;
    [
        (Method::POST, "/_synapse/worker/v1/workers/{worker_id}/heartbeat"),
        (Method::POST, "/_synapse/worker/v1/workers/{worker_id}/connect"),
        (Method::POST, "/_synapse/worker/v1/workers/{worker_id}/disconnect"),
        (Method::GET, "/_synapse/worker/v1/workers/{worker_id}/commands"),
        (Method::POST, "/_synapse/worker/v1/commands/{command_id}/complete"),
        (Method::POST, "/_synapse/worker/v1/commands/{command_id}/fail"),
        (Method::POST, "/_synapse/worker/v1/tasks/{task_id}/complete"),
        (Method::POST, "/_synapse/worker/v1/tasks/{task_id}/fail"),
        (Method::GET, "/_synapse/worker/v1/replication/{worker_id}/position"),
        (Method::PUT, "/_synapse/worker/v1/replication/{worker_id}/{stream_name}"),
        (Method::GET, "/_synapse/worker/v1/events"),
    ]
    .into_iter()
    .map(|(m, p)| RouteEntry::new(m, p, "worker_body"))
    .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

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
        assert_eq!(response.replication_streams, vec!["events".to_string()]);
        assert!(response.capabilities.can_persist_events);
    }

    #[test]
    fn test_worker_route_manifest_contains_topology_endpoint() {
        let manifest = worker_route_manifest();
        assert!(manifest.iter().any(|entry| entry.path == "/_synapse/worker/v1/topology"));
        assert!(manifest.iter().any(|entry| entry.path == "/_synapse/worker/v1/topology/validate"));
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

        let response = build_topology_validation_response(&config);

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

        let response = build_topology_validation_response(&config);

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
}
