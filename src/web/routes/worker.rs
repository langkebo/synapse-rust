use axum::{
    extract::{Path, Query, State},
    http::StatusCode,
    response::IntoResponse,
    routing::{delete, get, post, put},
    Json, Router,
};
use serde::{Deserialize, Serialize};
use std::str::FromStr;

use crate::common::ApiError;
use crate::web::routes::AppState;
use crate::web::routes::AuthenticatedUser;
use crate::worker::types::*;

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

#[derive(Debug, Deserialize)]
pub struct AssignTaskBody {
    pub task_type: String,
    pub task_data: serde_json::Value,
    pub priority: Option<i32>,
    pub preferred_worker_id: Option<String>,
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
    pub host: String,
    pub port: i32,
    pub status: String,
    pub last_heartbeat_ts: Option<i64>,
    pub started_ts: i64,
}

impl From<WorkerInfo> for WorkerResponse {
    fn from(w: WorkerInfo) -> Self {
        Self {
            id: w.id,
            worker_id: w.worker_id,
            worker_name: w.worker_name,
            worker_type: w.worker_type,
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
        Self {
            task_id: t.task_id,
            task_type: t.task_type,
            status: t.status,
            assigned_worker_id: t.assigned_worker_id,
        }
    }
}

pub async fn register_worker(
    State(state): State<AppState>,
    _auth_user: AuthenticatedUser,
    Json(body): Json<RegisterWorkerBody>,
) -> Result<impl IntoResponse, ApiError> {
    let worker_type = WorkerType::from_str(&body.worker_type).map_err(ApiError::bad_request)?;

    let request = RegisterWorkerRequest {
        worker_id: body.worker_id,
        worker_name: body.worker_name,
        worker_type,
        host: body.host,
        port: body.port,
        config: body.config,
        metadata: body.metadata,
        version: body.version,
    };

    let worker = state.services.worker_manager.register(request).await?;

    Ok((StatusCode::CREATED, Json(WorkerResponse::from(worker))))
}

pub async fn get_worker(
    State(state): State<AppState>,
    Path(worker_id): Path<String>,
) -> Result<impl IntoResponse, ApiError> {
    let worker = state
        .services
        .worker_manager
        .get(&worker_id)
        .await?
        .ok_or_else(|| ApiError::not_found("Worker not found"))?;

    Ok(Json(WorkerResponse::from(worker)))
}

pub async fn list_workers(State(state): State<AppState>) -> Result<impl IntoResponse, ApiError> {
    let workers = state.services.worker_manager.get_active().await?;

    let response: Vec<WorkerResponse> = workers.into_iter().map(WorkerResponse::from).collect();

    Ok(Json(response))
}

pub async fn list_workers_by_type(
    State(state): State<AppState>,
    Path(worker_type): Path<String>,
) -> Result<impl IntoResponse, ApiError> {
    let wtype = WorkerType::from_str(&worker_type).map_err(ApiError::bad_request)?;

    let workers = state.services.worker_manager.get_by_type(wtype).await?;

    let response: Vec<WorkerResponse> = workers.into_iter().map(WorkerResponse::from).collect();

    Ok(Json(response))
}

pub async fn heartbeat(
    State(state): State<AppState>,
    Path(worker_id): Path<String>,
    Json(body): Json<HeartbeatBody>,
) -> Result<impl IntoResponse, ApiError> {
    let status = WorkerStatus::from_str(&body.status).map_err(ApiError::bad_request)?;

    state
        .services
        .worker_manager
        .heartbeat(&worker_id, status, body.load_stats)
        .await?;

    Ok(Json(serde_json::json!({ "status": "ok" })))
}

pub async fn unregister_worker(
    State(state): State<AppState>,
    Path(worker_id): Path<String>,
    _auth_user: AuthenticatedUser,
) -> Result<impl IntoResponse, ApiError> {
    state.services.worker_manager.unregister(&worker_id).await?;

    Ok(StatusCode::NO_CONTENT)
}

pub async fn send_command(
    State(state): State<AppState>,
    Path(worker_id): Path<String>,
    _auth_user: AuthenticatedUser,
    Json(body): Json<SendCommandBody>,
) -> Result<impl IntoResponse, ApiError> {
    let request = SendCommandRequest {
        target_worker_id: worker_id,
        command_type: body.command_type,
        command_data: body.command_data,
        priority: body.priority,
        max_retries: body.max_retries,
    };

    let command = state.services.worker_manager.send_command(request).await?;

    Ok((
        StatusCode::CREATED,
        Json(WorkerCommandResponse::from(command)),
    ))
}

pub async fn get_pending_commands(
    State(state): State<AppState>,
    Path(worker_id): Path<String>,
    Query(query): Query<QueryLimit>,
) -> Result<impl IntoResponse, ApiError> {
    let limit = query.limit.unwrap_or(100);
    let commands = state
        .services
        .worker_manager
        .get_pending_commands(&worker_id, limit)
        .await?;

    let response: Vec<WorkerCommandResponse> = commands
        .into_iter()
        .map(WorkerCommandResponse::from)
        .collect();

    Ok(Json(response))
}

pub async fn complete_command(
    State(state): State<AppState>,
    Path(command_id): Path<String>,
) -> Result<impl IntoResponse, ApiError> {
    state
        .services
        .worker_manager
        .complete_command(&command_id)
        .await?;

    Ok(Json(serde_json::json!({ "status": "completed" })))
}

pub async fn fail_command(
    State(state): State<AppState>,
    Path(command_id): Path<String>,
    Json(body): Json<FailTaskBody>,
) -> Result<impl IntoResponse, ApiError> {
    state
        .services
        .worker_manager
        .fail_command(&command_id, &body.error)
        .await?;

    Ok(Json(serde_json::json!({ "status": "failed" })))
}

pub async fn assign_task(
    State(state): State<AppState>,
    _auth_user: AuthenticatedUser,
    Json(body): Json<AssignTaskBody>,
) -> Result<impl IntoResponse, ApiError> {
    let request = AssignTaskRequest {
        task_type: body.task_type,
        task_data: body.task_data,
        priority: body.priority,
        preferred_worker_id: body.preferred_worker_id,
    };

    let task = state.services.worker_manager.assign_task(request).await?;

    Ok((StatusCode::CREATED, Json(WorkerTaskResponse::from(task))))
}

pub async fn get_pending_tasks(
    State(state): State<AppState>,
    Query(query): Query<QueryLimit>,
) -> Result<impl IntoResponse, ApiError> {
    let limit = query.limit.unwrap_or(100);
    let tasks = state
        .services
        .worker_manager
        .get_pending_tasks(limit)
        .await?;

    let response: Vec<WorkerTaskResponse> =
        tasks.into_iter().map(WorkerTaskResponse::from).collect();

    Ok(Json(response))
}

pub async fn claim_task(
    State(state): State<AppState>,
    Path((task_id, worker_id)): Path<(String, String)>,
) -> Result<impl IntoResponse, ApiError> {
    state
        .services
        .worker_manager
        .claim_task(&task_id, &worker_id)
        .await?;

    Ok(Json(serde_json::json!({ "status": "claimed" })))
}

pub async fn complete_task(
    State(state): State<AppState>,
    Path(task_id): Path<String>,
    Json(body): Json<CompleteTaskBody>,
) -> Result<impl IntoResponse, ApiError> {
    state
        .services
        .worker_manager
        .complete_task(&task_id, body.result)
        .await?;

    Ok(Json(serde_json::json!({ "status": "completed" })))
}

pub async fn fail_task(
    State(state): State<AppState>,
    Path(task_id): Path<String>,
    Json(body): Json<FailTaskBody>,
) -> Result<impl IntoResponse, ApiError> {
    state
        .services
        .worker_manager
        .fail_task(&task_id, &body.error)
        .await?;

    Ok(Json(serde_json::json!({ "status": "failed" })))
}

pub async fn connect_worker(
    State(state): State<AppState>,
    Path(worker_id): Path<String>,
    Json(body): Json<ConnectWorkerBody>,
) -> Result<impl IntoResponse, ApiError> {
    state
        .services
        .worker_manager
        .connect_to_worker(&worker_id, &body.address)
        .await?;

    Ok(Json(serde_json::json!({ "status": "connected" })))
}

pub async fn disconnect_worker(
    State(state): State<AppState>,
    Path(worker_id): Path<String>,
) -> Result<impl IntoResponse, ApiError> {
    state
        .services
        .worker_manager
        .disconnect_from_worker(&worker_id)
        .await?;

    Ok(Json(serde_json::json!({ "status": "disconnected" })))
}

pub async fn get_replication_position(
    State(state): State<AppState>,
    Path(worker_id): Path<String>,
    Query(query): Query<QueryPosition>,
) -> Result<impl IntoResponse, ApiError> {
    let position = state
        .services
        .worker_manager
        .get_replication_position(&worker_id, &query.stream_name)
        .await?;

    Ok(Json(serde_json::json!({
        "worker_id": worker_id,
        "stream_name": query.stream_name,
        "position": position
    })))
}

pub async fn update_replication_position(
    State(state): State<AppState>,
    Path((worker_id, stream_name)): Path<(String, String)>,
    Json(body): Json<StreamPosition>,
) -> Result<impl IntoResponse, ApiError> {
    state
        .services
        .worker_manager
        .update_replication_position(&worker_id, &stream_name, body.position)
        .await?;

    Ok(Json(serde_json::json!({ "status": "updated" })))
}

pub async fn get_events(
    State(state): State<AppState>,
    Query(query): Query<QueryStream>,
) -> Result<impl IntoResponse, ApiError> {
    let stream_id = query.stream_id.unwrap_or(0);
    let limit = 100;
    let events = state
        .services
        .worker_manager
        .get_events_since(stream_id, limit)
        .await?;

    Ok(Json(events))
}

pub async fn get_statistics(State(state): State<AppState>) -> Result<impl IntoResponse, ApiError> {
    let stats = state.services.worker_manager.get_statistics().await?;

    Ok(Json(stats))
}

pub async fn get_type_statistics(
    State(state): State<AppState>,
) -> Result<impl IntoResponse, ApiError> {
    let stats = state.services.worker_manager.get_type_statistics().await?;

    Ok(Json(stats))
}

pub async fn select_worker(
    State(state): State<AppState>,
    Path(task_type): Path<String>,
) -> Result<impl IntoResponse, ApiError> {
    let worker_id = state
        .services
        .worker_manager
        .select_worker_for_task(&task_type)
        .await?;

    Ok(Json(serde_json::json!({
        "task_type": task_type,
        "selected_worker": worker_id
    })))
}

pub fn create_worker_router(state: AppState) -> Router<AppState> {
    Router::new()
        .route("/_synapse/worker/v1/register", post(register_worker))
        .route("/_synapse/worker/v1/workers", get(list_workers))
        .route(
            "/_synapse/worker/v1/workers/type/{worker_type}",
            get(list_workers_by_type),
        )
        .route("/_synapse/worker/v1/workers/{worker_id}", get(get_worker))
        .route(
            "/_synapse/worker/v1/workers/{worker_id}",
            delete(unregister_worker),
        )
        .route(
            "/_synapse/worker/v1/workers/{worker_id}/heartbeat",
            post(heartbeat),
        )
        .route(
            "/_synapse/worker/v1/workers/{worker_id}/connect",
            post(connect_worker),
        )
        .route(
            "/_synapse/worker/v1/workers/{worker_id}/disconnect",
            post(disconnect_worker),
        )
        .route(
            "/_synapse/worker/v1/workers/{worker_id}/commands",
            post(send_command),
        )
        .route(
            "/_synapse/worker/v1/workers/{worker_id}/commands",
            get(get_pending_commands),
        )
        .route(
            "/_synapse/worker/v1/commands/{command_id}/complete",
            post(complete_command),
        )
        .route(
            "/_synapse/worker/v1/commands/{command_id}/fail",
            post(fail_command),
        )
        .route("/_synapse/worker/v1/tasks", post(assign_task))
        .route("/_synapse/worker/v1/tasks", get(get_pending_tasks))
        .route(
            "/_synapse/worker/v1/tasks/{task_id}/claim/{worker_id}",
            post(claim_task),
        )
        .route(
            "/_synapse/worker/v1/tasks/{task_id}/complete",
            post(complete_task),
        )
        .route("/_synapse/worker/v1/tasks/{task_id}/fail", post(fail_task))
        .route(
            "/_synapse/worker/v1/replication/{worker_id}/position",
            get(get_replication_position),
        )
        .route(
            "/_synapse/worker/v1/replication/{worker_id}/{stream_name}",
            put(update_replication_position),
        )
        .route("/_synapse/worker/v1/events", get(get_events))
        .route("/_synapse/worker/v1/statistics", get(get_statistics))
        .route(
            "/_synapse/worker/v1/statistics/types",
            get(get_type_statistics),
        )
        .route("/_synapse/worker/v1/select/{task_type}", get(select_worker))
        .with_state(state)
}
