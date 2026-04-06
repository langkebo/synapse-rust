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
use crate::web::routes::response_helpers::{
    created_json_from, json_from, json_vec_from, require_found, status_json,
};
use crate::web::routes::{AdminUser, AppState, AuthenticatedUser};
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
    _admin_user: AdminUser,
    Json(body): Json<RegisterWorkerBody>,
) -> Result<impl IntoResponse, ApiError> {
    let request = body.into_request()?;

    let worker = state.services.worker_manager.register(request).await?;

    Ok(created_json_from::<_, WorkerResponse>(worker))
}

pub async fn get_worker(
    State(state): State<AppState>,
    Path(worker_id): Path<String>,
    _admin_user: AdminUser,
) -> Result<impl IntoResponse, ApiError> {
    let worker = state.services.worker_manager.get(&worker_id).await?;

    Ok(json_from::<_, WorkerResponse>(require_found(
        worker,
        "Worker not found",
    )?))
}

pub async fn list_workers(
    State(state): State<AppState>,
    _admin_user: AdminUser,
) -> Result<impl IntoResponse, ApiError> {
    let workers = state.services.worker_manager.get_active().await?;

    Ok(json_vec_from::<_, WorkerResponse>(workers))
}

pub async fn list_workers_by_type(
    State(state): State<AppState>,
    Path(worker_type): Path<String>,
    _admin_user: AdminUser,
) -> Result<impl IntoResponse, ApiError> {
    let wtype = WorkerType::from_str(&worker_type).map_err(ApiError::bad_request)?;

    let workers = state.services.worker_manager.get_by_type(wtype).await?;

    Ok(json_vec_from::<_, WorkerResponse>(workers))
}

pub async fn heartbeat(
    State(state): State<AppState>,
    Path(worker_id): Path<String>,
    _auth_user: AuthenticatedUser,
    Json(body): Json<HeartbeatBody>,
) -> Result<impl IntoResponse, ApiError> {
    let status = WorkerStatus::from_str(&body.status).map_err(ApiError::bad_request)?;

    state
        .services
        .worker_manager
        .heartbeat(&worker_id, status, body.load_stats)
        .await?;

    Ok(status_json("ok"))
}

pub async fn unregister_worker(
    State(state): State<AppState>,
    Path(worker_id): Path<String>,
    _admin_user: AdminUser,
) -> Result<impl IntoResponse, ApiError> {
    state.services.worker_manager.unregister(&worker_id).await?;

    Ok(StatusCode::NO_CONTENT)
}

pub async fn send_command(
    State(state): State<AppState>,
    Path(worker_id): Path<String>,
    _admin_user: AdminUser,
    Json(body): Json<SendCommandBody>,
) -> Result<impl IntoResponse, ApiError> {
    let request = body.into_request(worker_id);

    let command = state.services.worker_manager.send_command(request).await?;

    Ok(created_json_from::<_, WorkerCommandResponse>(command))
}

pub async fn get_pending_commands(
    State(state): State<AppState>,
    Path(worker_id): Path<String>,
    _auth_user: AuthenticatedUser,
    Query(query): Query<QueryLimit>,
) -> Result<impl IntoResponse, ApiError> {
    let limit = query.limit.unwrap_or(100);
    let commands = state
        .services
        .worker_manager
        .get_pending_commands(&worker_id, limit)
        .await?;

    Ok(json_vec_from::<_, WorkerCommandResponse>(commands))
}

pub async fn complete_command(
    State(state): State<AppState>,
    Path(command_id): Path<String>,
    _auth_user: AuthenticatedUser,
) -> Result<impl IntoResponse, ApiError> {
    state
        .services
        .worker_manager
        .complete_command(&command_id)
        .await?;

    Ok(status_json("completed"))
}

pub async fn fail_command(
    State(state): State<AppState>,
    Path(command_id): Path<String>,
    _auth_user: AuthenticatedUser,
    Json(body): Json<FailTaskBody>,
) -> Result<impl IntoResponse, ApiError> {
    state
        .services
        .worker_manager
        .fail_command(&command_id, &body.error)
        .await?;

    Ok(status_json("failed"))
}

pub async fn assign_task(
    State(state): State<AppState>,
    _admin_user: AdminUser,
    Json(body): Json<AssignTaskBody>,
) -> Result<impl IntoResponse, ApiError> {
    let request = body.into_request();

    let task = state.services.worker_manager.assign_task(request).await?;

    Ok(created_json_from::<_, WorkerTaskResponse>(task))
}

pub async fn get_pending_tasks(
    State(state): State<AppState>,
    _admin_user: AdminUser,
    Query(query): Query<QueryLimit>,
) -> Result<impl IntoResponse, ApiError> {
    let limit = query.limit.unwrap_or(100);
    let tasks = state
        .services
        .worker_manager
        .get_pending_tasks(limit)
        .await?;

    Ok(json_vec_from::<_, WorkerTaskResponse>(tasks))
}

pub async fn claim_next_task(
    State(state): State<AppState>,
    Path(worker_id): Path<String>,
    _auth_user: AuthenticatedUser,
) -> Result<impl IntoResponse, ApiError> {
    let task = state
        .services
        .worker_manager
        .claim_next_pending_task(&worker_id)
        .await?;

    Ok(Json(WorkerTaskResponse::from(task)))
}

pub async fn claim_task(
    State(state): State<AppState>,
    Path((task_id, worker_id)): Path<(String, String)>,
    _auth_user: AuthenticatedUser,
) -> Result<impl IntoResponse, ApiError> {
    state
        .services
        .worker_manager
        .claim_task(&task_id, &worker_id)
        .await?;

    Ok(status_json("claimed"))
}

pub async fn complete_task(
    State(state): State<AppState>,
    Path(task_id): Path<String>,
    _auth_user: AuthenticatedUser,
    Json(body): Json<CompleteTaskBody>,
) -> Result<impl IntoResponse, ApiError> {
    state
        .services
        .worker_manager
        .complete_task(&task_id, body.result)
        .await?;

    Ok(status_json("completed"))
}

pub async fn fail_task(
    State(state): State<AppState>,
    Path(task_id): Path<String>,
    _auth_user: AuthenticatedUser,
    Json(body): Json<FailTaskBody>,
) -> Result<impl IntoResponse, ApiError> {
    state
        .services
        .worker_manager
        .fail_task(&task_id, &body.error)
        .await?;

    Ok(status_json("failed"))
}

pub async fn connect_worker(
    State(state): State<AppState>,
    Path(worker_id): Path<String>,
    _auth_user: AuthenticatedUser,
    Json(body): Json<ConnectWorkerBody>,
) -> Result<impl IntoResponse, ApiError> {
    state
        .services
        .worker_manager
        .connect_to_worker(&worker_id, &body.address)
        .await?;

    Ok(status_json("connected"))
}

pub async fn disconnect_worker(
    State(state): State<AppState>,
    Path(worker_id): Path<String>,
    _auth_user: AuthenticatedUser,
) -> Result<impl IntoResponse, ApiError> {
    state
        .services
        .worker_manager
        .disconnect_from_worker(&worker_id)
        .await?;

    Ok(status_json("disconnected"))
}

pub async fn get_replication_position(
    State(state): State<AppState>,
    Path(worker_id): Path<String>,
    _auth_user: AuthenticatedUser,
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
    _auth_user: AuthenticatedUser,
    Json(body): Json<StreamPosition>,
) -> Result<impl IntoResponse, ApiError> {
    state
        .services
        .worker_manager
        .update_replication_position(&worker_id, &stream_name, body.position)
        .await?;

    Ok(status_json("updated"))
}

pub async fn get_events(
    State(state): State<AppState>,
    _auth_user: AuthenticatedUser,
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

pub async fn get_statistics(
    State(state): State<AppState>,
    _admin_user: AdminUser,
) -> Result<impl IntoResponse, ApiError> {
    let stats = state.services.worker_manager.get_statistics().await?;

    Ok(Json(stats))
}

pub async fn get_type_statistics(
    State(state): State<AppState>,
    _admin_user: AdminUser,
) -> Result<impl IntoResponse, ApiError> {
    let stats = state.services.worker_manager.get_type_statistics().await?;

    Ok(Json(stats))
}

pub async fn select_worker(
    State(state): State<AppState>,
    Path(task_type): Path<String>,
    _admin_user: AdminUser,
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
            "/_synapse/worker/v1/tasks/claim/{worker_id}",
            post(claim_next_task),
        )
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

        let error = body
            .into_request()
            .expect_err("invalid worker type should fail");

        match error {
            ApiError::BadRequest(message) => {
                assert!(message.contains("unknown"));
            }
            other => panic!("expected bad request error, got {:?}", other),
        }
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
}
