use axum::{
    extract::{Path, Query, State},
    http::StatusCode,
    response::IntoResponse,
    Json,
    Router,
    routing::{get, post, put, delete},
};
use serde::{Deserialize, Serialize};

use crate::common::ApiError;
use crate::storage::retention::{
    CreateRoomRetentionPolicyRequest, UpdateRoomRetentionPolicyRequest,
    UpdateServerRetentionPolicyRequest, RoomRetentionPolicy, ServerRetentionPolicy,
    EffectiveRetentionPolicy, RetentionCleanupLog, RetentionStats, DeletedEventIndex,
};
use crate::web::routes::AuthenticatedUser;
use crate::web::routes::AppState;

#[derive(Debug, Deserialize)]
pub struct QueryLimit {
    pub limit: Option<i64>,
}

#[derive(Debug, Deserialize)]
pub struct QuerySince {
    pub since: Option<i64>,
}

#[derive(Debug, Deserialize)]
pub struct SetRoomPolicyBody {
    pub max_lifetime: Option<i64>,
    pub min_lifetime: Option<i64>,
    pub expire_on_clients: Option<bool>,
}

#[derive(Debug, Deserialize)]
pub struct SetServerPolicyBody {
    pub max_lifetime: Option<i64>,
    pub min_lifetime: Option<i64>,
    pub expire_on_clients: Option<bool>,
}

#[derive(Debug, Serialize)]
pub struct RoomPolicyResponse {
    pub room_id: String,
    pub max_lifetime: Option<i64>,
    pub min_lifetime: i64,
    pub expire_on_clients: bool,
}

impl From<RoomRetentionPolicy> for RoomPolicyResponse {
    fn from(p: RoomRetentionPolicy) -> Self {
        Self {
            room_id: p.room_id,
            max_lifetime: p.max_lifetime,
            min_lifetime: p.min_lifetime,
            expire_on_clients: p.expire_on_clients,
        }
    }
}

#[derive(Debug, Serialize)]
pub struct ServerPolicyResponse {
    pub max_lifetime: Option<i64>,
    pub min_lifetime: i64,
    pub expire_on_clients: bool,
}

impl From<ServerRetentionPolicy> for ServerPolicyResponse {
    fn from(p: ServerRetentionPolicy) -> Self {
        Self {
            max_lifetime: p.max_lifetime,
            min_lifetime: p.min_lifetime,
            expire_on_clients: p.expire_on_clients,
        }
    }
}

#[derive(Debug, Serialize)]
pub struct EffectivePolicyResponse {
    pub max_lifetime: Option<i64>,
    pub min_lifetime: i64,
    pub expire_on_clients: bool,
}

impl From<EffectiveRetentionPolicy> for EffectivePolicyResponse {
    fn from(p: EffectiveRetentionPolicy) -> Self {
        Self {
            max_lifetime: p.max_lifetime,
            min_lifetime: p.min_lifetime,
            expire_on_clients: p.expire_on_clients,
        }
    }
}

#[derive(Debug, Serialize)]
pub struct CleanupLogResponse {
    pub id: i64,
    pub room_id: String,
    pub events_deleted: i64,
    pub state_events_deleted: i64,
    pub media_deleted: i64,
    pub bytes_freed: i64,
    pub started_ts: i64,
    pub completed_ts: Option<i64>,
    pub status: String,
}

impl From<RetentionCleanupLog> for CleanupLogResponse {
    fn from(l: RetentionCleanupLog) -> Self {
        Self {
            id: l.id,
            room_id: l.room_id,
            events_deleted: l.events_deleted,
            state_events_deleted: l.state_events_deleted,
            media_deleted: l.media_deleted,
            bytes_freed: l.bytes_freed,
            started_ts: l.started_ts,
            completed_ts: l.completed_ts,
            status: l.status,
        }
    }
}

#[derive(Debug, Serialize)]
pub struct StatsResponse {
    pub room_id: String,
    pub total_events: i64,
    pub events_in_retention: i64,
    pub events_expired: i64,
    pub last_cleanup_ts: Option<i64>,
    pub next_cleanup_ts: Option<i64>,
}

impl From<RetentionStats> for StatsResponse {
    fn from(s: RetentionStats) -> Self {
        Self {
            room_id: s.room_id,
            total_events: s.total_events,
            events_in_retention: s.events_in_retention,
            events_expired: s.events_expired,
            last_cleanup_ts: s.last_cleanup_ts,
            next_cleanup_ts: s.next_cleanup_ts,
        }
    }
}

#[derive(Debug, Serialize)]
pub struct DeletedEventResponse {
    pub event_id: String,
    pub room_id: String,
    pub deletion_ts: i64,
    pub reason: String,
}

impl From<DeletedEventIndex> for DeletedEventResponse {
    fn from(e: DeletedEventIndex) -> Self {
        Self {
            event_id: e.event_id,
            room_id: e.room_id,
            deletion_ts: e.deletion_ts,
            reason: e.reason,
        }
    }
}

pub async fn get_room_policy(
    State(state): State<AppState>,
    Path(room_id): Path<String>,
) -> Result<impl IntoResponse, ApiError> {
    let policy = state.services.retention_service.get_room_policy(&room_id).await?
        .ok_or_else(|| ApiError::not_found("Room retention policy not found"))?;

    Ok(Json(RoomPolicyResponse::from(policy)))
}

pub async fn get_effective_policy(
    State(state): State<AppState>,
    Path(room_id): Path<String>,
) -> Result<impl IntoResponse, ApiError> {
    let policy = state.services.retention_service.get_effective_policy(&room_id).await?;

    Ok(Json(EffectivePolicyResponse::from(policy)))
}

pub async fn set_room_policy(
    State(state): State<AppState>,
    Path(room_id): Path<String>,
    _auth_user: AuthenticatedUser,
    Json(body): Json<SetRoomPolicyBody>,
) -> Result<impl IntoResponse, ApiError> {
    let request = CreateRoomRetentionPolicyRequest {
        room_id,
        max_lifetime: body.max_lifetime,
        min_lifetime: body.min_lifetime,
        expire_on_clients: body.expire_on_clients,
    };

    let policy = state.services.retention_service.set_room_policy(request).await?;

    Ok((StatusCode::CREATED, Json(RoomPolicyResponse::from(policy))))
}

pub async fn update_room_policy(
    State(state): State<AppState>,
    Path(room_id): Path<String>,
    _auth_user: AuthenticatedUser,
    Json(body): Json<SetRoomPolicyBody>,
) -> Result<impl IntoResponse, ApiError> {
    let request = UpdateRoomRetentionPolicyRequest {
        max_lifetime: body.max_lifetime,
        min_lifetime: body.min_lifetime,
        expire_on_clients: body.expire_on_clients,
    };

    let policy = state.services.retention_service.update_room_policy(&room_id, request).await?;

    Ok(Json(RoomPolicyResponse::from(policy)))
}

pub async fn delete_room_policy(
    State(state): State<AppState>,
    Path(room_id): Path<String>,
    _auth_user: AuthenticatedUser,
) -> Result<impl IntoResponse, ApiError> {
    state.services.retention_service.delete_room_policy(&room_id).await?;

    Ok(StatusCode::NO_CONTENT)
}

pub async fn get_server_policy(
    State(state): State<AppState>,
) -> Result<impl IntoResponse, ApiError> {
    let policy = state.services.retention_service.get_server_policy().await?;

    Ok(Json(ServerPolicyResponse::from(policy)))
}

pub async fn update_server_policy(
    State(state): State<AppState>,
    _auth_user: AuthenticatedUser,
    Json(body): Json<SetServerPolicyBody>,
) -> Result<impl IntoResponse, ApiError> {
    let request = UpdateServerRetentionPolicyRequest {
        max_lifetime: body.max_lifetime,
        min_lifetime: body.min_lifetime,
        expire_on_clients: body.expire_on_clients,
    };

    let policy = state.services.retention_service.update_server_policy(request).await?;

    Ok(Json(ServerPolicyResponse::from(policy)))
}

pub async fn run_cleanup(
    State(state): State<AppState>,
    Path(room_id): Path<String>,
    _auth_user: AuthenticatedUser,
) -> Result<impl IntoResponse, ApiError> {
    let log = state.services.retention_service.run_cleanup(&room_id).await?;

    Ok(Json(CleanupLogResponse::from(log)))
}

pub async fn schedule_cleanup(
    State(state): State<AppState>,
    Path(room_id): Path<String>,
    _auth_user: AuthenticatedUser,
) -> Result<impl IntoResponse, ApiError> {
    let count = state.services.retention_service.schedule_room_cleanup(&room_id).await?;

    Ok(Json(serde_json::json!({
        "room_id": room_id,
        "scheduled_count": count,
    })))
}

pub async fn process_pending_cleanups(
    State(state): State<AppState>,
    _auth_user: AuthenticatedUser,
    Query(query): Query<QueryLimit>,
) -> Result<impl IntoResponse, ApiError> {
    let limit = query.limit.unwrap_or(100);
    let processed = state.services.retention_service.process_pending_cleanups(limit).await?;

    Ok(Json(serde_json::json!({
        "processed": processed,
    })))
}

pub async fn get_stats(
    State(state): State<AppState>,
    Path(room_id): Path<String>,
) -> Result<impl IntoResponse, ApiError> {
    let stats = state.services.retention_service.get_stats(&room_id).await?
        .ok_or_else(|| ApiError::not_found("Retention stats not found"))?;

    Ok(Json(StatsResponse::from(stats)))
}

pub async fn get_cleanup_logs(
    State(state): State<AppState>,
    Path(room_id): Path<String>,
    Query(query): Query<QueryLimit>,
) -> Result<impl IntoResponse, ApiError> {
    let limit = query.limit.unwrap_or(10);
    let logs = state.services.retention_service.get_cleanup_logs(&room_id, limit).await?;

    let response: Vec<CleanupLogResponse> = logs.into_iter().map(CleanupLogResponse::from).collect();

    Ok(Json(response))
}

pub async fn get_deleted_events(
    State(state): State<AppState>,
    Path(room_id): Path<String>,
    Query(query): Query<QuerySince>,
) -> Result<impl IntoResponse, ApiError> {
    let since = query.since.unwrap_or(0);
    let events = state.services.retention_service.get_deleted_events(&room_id, since).await?;

    let response: Vec<DeletedEventResponse> = events.into_iter().map(DeletedEventResponse::from).collect();

    Ok(Json(response))
}

pub async fn get_rooms_with_policies(
    State(state): State<AppState>,
) -> Result<impl IntoResponse, ApiError> {
    let policies = state.services.retention_service.get_rooms_with_policies().await?;

    let response: Vec<RoomPolicyResponse> = policies.into_iter().map(RoomPolicyResponse::from).collect();

    Ok(Json(response))
}

pub async fn get_pending_cleanup_count(
    State(state): State<AppState>,
    Path(room_id): Path<String>,
) -> Result<impl IntoResponse, ApiError> {
    let count = state.services.retention_service.get_pending_cleanup_count(&room_id).await?;

    Ok(Json(serde_json::json!({
        "room_id": room_id,
        "pending_count": count,
    })))
}

pub async fn run_scheduled_cleanups(
    State(state): State<AppState>,
    _auth_user: AuthenticatedUser,
) -> Result<impl IntoResponse, ApiError> {
    let total_cleaned = state.services.retention_service.run_scheduled_cleanups().await?;

    Ok(Json(serde_json::json!({
        "total_events_cleaned": total_cleaned,
    })))
}

pub fn create_retention_router(state: AppState) -> Router<AppState> {
    Router::new()
        .route("/_synapse/retention/v1/rooms/{room_id}/policy", get(get_room_policy))
        .route("/_synapse/retention/v1/rooms/{room_id}/policy", post(set_room_policy))
        .route("/_synapse/retention/v1/rooms/{room_id}/policy", put(update_room_policy))
        .route("/_synapse/retention/v1/rooms/{room_id}/policy", delete(delete_room_policy))
        .route("/_synapse/retention/v1/rooms/{room_id}/effective_policy", get(get_effective_policy))
        .route("/_synapse/retention/v1/rooms/{room_id}/cleanup", post(run_cleanup))
        .route("/_synapse/retention/v1/rooms/{room_id}/cleanup/schedule", post(schedule_cleanup))
        .route("/_synapse/retention/v1/rooms/{room_id}/stats", get(get_stats))
        .route("/_synapse/retention/v1/rooms/{room_id}/logs", get(get_cleanup_logs))
        .route("/_synapse/retention/v1/rooms/{room_id}/deleted", get(get_deleted_events))
        .route("/_synapse/retention/v1/rooms/{room_id}/pending", get(get_pending_cleanup_count))
        .route("/_synapse/retention/v1/server/policy", get(get_server_policy))
        .route("/_synapse/retention/v1/server/policy", put(update_server_policy))
        .route("/_synapse/retention/v1/rooms", get(get_rooms_with_policies))
        .route("/_synapse/retention/v1/cleanups/process", post(process_pending_cleanups))
        .route("/_synapse/retention/v1/cleanups/run_scheduled", post(run_scheduled_cleanups))
        .with_state(state)
}
