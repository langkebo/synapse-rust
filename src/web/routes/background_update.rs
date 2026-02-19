use axum::{
    extract::{Path, Query, State},
    http::StatusCode,
    response::IntoResponse,
    Json,
    Router,
    routing::{get, post, delete},
};
use serde::{Deserialize, Serialize};

use crate::common::ApiError;
use crate::storage::background_update::{
    CreateBackgroundUpdateRequest, BackgroundUpdate, BackgroundUpdateHistory, BackgroundUpdateStats,
};
use crate::web::routes::AdminUser;
use crate::web::routes::AppState;

#[derive(Debug, Deserialize)]
pub struct QueryParams {
    pub limit: Option<i64>,
    pub offset: Option<i64>,
}

#[derive(Debug, Deserialize)]
pub struct CreateUpdateBody {
    pub job_name: String,
    pub job_type: String,
    pub description: Option<String>,
    pub table_name: Option<String>,
    pub column_name: Option<String>,
    pub total_items: Option<i32>,
    pub batch_size: Option<i32>,
    pub sleep_ms: Option<i32>,
    pub depends_on: Option<Vec<String>>,
    pub metadata: Option<serde_json::Value>,
}

#[derive(Debug, Deserialize)]
pub struct UpdateProgressBody {
    pub items_processed: i32,
    pub total_items: Option<i32>,
}

#[derive(Debug, Deserialize)]
pub struct FailUpdateBody {
    pub error_message: String,
}

#[derive(Debug, Serialize)]
pub struct UpdateResponse {
    pub job_name: String,
    pub job_type: String,
    pub description: Option<String>,
    pub table_name: Option<String>,
    pub status: String,
    pub progress: i32,
    pub total_items: i32,
    pub processed_items: i32,
    pub created_ts: i64,
    pub started_ts: Option<i64>,
    pub completed_ts: Option<i64>,
    pub error_message: Option<String>,
    pub retry_count: i32,
}

impl From<BackgroundUpdate> for UpdateResponse {
    fn from(u: BackgroundUpdate) -> Self {
        Self {
            job_name: u.job_name,
            job_type: u.job_type,
            description: u.description,
            table_name: u.table_name,
            status: u.status,
            progress: u.progress,
            total_items: u.total_items,
            processed_items: u.processed_items,
            created_ts: u.created_ts,
            started_ts: u.started_ts,
            completed_ts: u.completed_ts,
            error_message: u.error_message,
            retry_count: u.retry_count,
        }
    }
}

#[derive(Debug, Serialize)]
pub struct HistoryResponse {
    pub id: i64,
    pub job_name: String,
    pub execution_start_ts: i64,
    pub execution_end_ts: Option<i64>,
    pub status: String,
    pub items_processed: i32,
    pub error_message: Option<String>,
}

impl From<BackgroundUpdateHistory> for HistoryResponse {
    fn from(h: BackgroundUpdateHistory) -> Self {
        Self {
            id: h.id,
            job_name: h.job_name,
            execution_start_ts: h.execution_start_ts,
            execution_end_ts: h.execution_end_ts,
            status: h.status,
            items_processed: h.items_processed,
            error_message: h.error_message,
        }
    }
}

#[derive(Debug, Serialize)]
pub struct StatsResponse {
    pub stat_date: chrono::NaiveDate,
    pub total_jobs: i32,
    pub completed_jobs: i32,
    pub failed_jobs: i32,
    pub total_items_processed: i64,
}

impl From<BackgroundUpdateStats> for StatsResponse {
    fn from(s: BackgroundUpdateStats) -> Self {
        Self {
            stat_date: s.stat_date,
            total_jobs: s.total_jobs,
            completed_jobs: s.completed_jobs,
            failed_jobs: s.failed_jobs,
            total_items_processed: s.total_items_processed,
        }
    }
}

pub async fn create_update(
    State(state): State<AppState>,
    _auth_user: AdminUser,
    Json(body): Json<CreateUpdateBody>,
) -> Result<impl IntoResponse, ApiError> {
    let request = CreateBackgroundUpdateRequest {
        job_name: body.job_name,
        job_type: body.job_type,
        description: body.description,
        table_name: body.table_name,
        column_name: body.column_name,
        total_items: body.total_items,
        batch_size: body.batch_size,
        sleep_ms: body.sleep_ms,
        depends_on: body.depends_on,
        metadata: body.metadata,
    };

    let update = state.services.background_update_service.create_update(request).await?;

    Ok((StatusCode::CREATED, Json(UpdateResponse::from(update))))
}

pub async fn get_update(
    State(state): State<AppState>,
    _auth_user: AdminUser,
    Path(job_name): Path<String>,
) -> Result<impl IntoResponse, ApiError> {
    let update = state.services.background_update_service.get_update(&job_name).await?
        .ok_or_else(|| ApiError::not_found("Update not found"))?;

    Ok(Json(UpdateResponse::from(update)))
}

pub async fn get_all_updates(
    State(state): State<AppState>,
    _auth_user: AdminUser,
    Query(query): Query<QueryParams>,
) -> Result<impl IntoResponse, ApiError> {
    let limit = query.limit.unwrap_or(100);
    let offset = query.offset.unwrap_or(0);

    let updates = state.services.background_update_service.get_all_updates(limit, offset).await?;

    let response: Vec<UpdateResponse> = updates.into_iter().map(UpdateResponse::from).collect();

    Ok(Json(response))
}

pub async fn get_pending_updates(
    State(state): State<AppState>,
    _auth_user: AdminUser,
) -> Result<impl IntoResponse, ApiError> {
    let updates = state.services.background_update_service.get_pending_updates().await?;

    let response: Vec<UpdateResponse> = updates.into_iter().map(UpdateResponse::from).collect();

    Ok(Json(response))
}

pub async fn get_running_updates(
    State(state): State<AppState>,
    _auth_user: AdminUser,
) -> Result<impl IntoResponse, ApiError> {
    let updates = state.services.background_update_service.get_running_updates().await?;

    let response: Vec<UpdateResponse> = updates.into_iter().map(UpdateResponse::from).collect();

    Ok(Json(response))
}

pub async fn start_update(
    State(state): State<AppState>,
    _auth_user: AdminUser,
    Path(job_name): Path<String>,
) -> Result<impl IntoResponse, ApiError> {
    let update = state.services.background_update_service.start_update(&job_name).await?;

    Ok(Json(UpdateResponse::from(update)))
}

pub async fn update_progress(
    State(state): State<AppState>,
    _auth_user: AdminUser,
    Path(job_name): Path<String>,
    Json(body): Json<UpdateProgressBody>,
) -> Result<impl IntoResponse, ApiError> {
    let update = state.services.background_update_service.update_progress(&job_name, body.items_processed, body.total_items).await?;

    Ok(Json(UpdateResponse::from(update)))
}

pub async fn complete_update(
    State(state): State<AppState>,
    _auth_user: AdminUser,
    Path(job_name): Path<String>,
) -> Result<impl IntoResponse, ApiError> {
    let update = state.services.background_update_service.complete_update(&job_name).await?;

    Ok(Json(UpdateResponse::from(update)))
}

pub async fn fail_update(
    State(state): State<AppState>,
    _auth_user: AdminUser,
    Path(job_name): Path<String>,
    Json(body): Json<FailUpdateBody>,
) -> Result<impl IntoResponse, ApiError> {
    let update = state.services.background_update_service.fail_update(&job_name, &body.error_message).await?;

    Ok(Json(UpdateResponse::from(update)))
}

pub async fn cancel_update(
    State(state): State<AppState>,
    _auth_user: AdminUser,
    Path(job_name): Path<String>,
) -> Result<impl IntoResponse, ApiError> {
    let update = state.services.background_update_service.cancel_update(&job_name).await?;

    Ok(Json(UpdateResponse::from(update)))
}

pub async fn delete_update(
    State(state): State<AppState>,
    _auth_user: AdminUser,
    Path(job_name): Path<String>,
) -> Result<impl IntoResponse, ApiError> {
    state.services.background_update_service.delete_update(&job_name).await?;

    Ok(StatusCode::NO_CONTENT)
}

pub async fn get_history(
    State(state): State<AppState>,
    _auth_user: AdminUser,
    Path(job_name): Path<String>,
    Query(query): Query<QueryParams>,
) -> Result<impl IntoResponse, ApiError> {
    let limit = query.limit.unwrap_or(100);

    let history = state.services.background_update_service.get_history(&job_name, limit).await?;

    let response: Vec<HistoryResponse> = history.into_iter().map(HistoryResponse::from).collect();

    Ok(Json(response))
}

pub async fn retry_failed(
    State(state): State<AppState>,
    _auth_user: AdminUser,
) -> Result<impl IntoResponse, ApiError> {
    let count = state.services.background_update_service.retry_failed().await?;

    Ok(Json(serde_json::json!({
        "retried_count": count,
    })))
}

pub async fn cleanup_locks(
    State(state): State<AppState>,
    _auth_user: AdminUser,
) -> Result<impl IntoResponse, ApiError> {
    let count = state.services.background_update_service.cleanup_expired_locks().await?;

    Ok(Json(serde_json::json!({
        "cleaned_count": count,
    })))
}

pub async fn count_by_status(
    State(state): State<AppState>,
    _auth_user: AdminUser,
    Path(status): Path<String>,
) -> Result<impl IntoResponse, ApiError> {
    let count = state.services.background_update_service.count_by_status(&status).await?;

    Ok(Json(serde_json::json!({
        "status": status,
        "count": count,
    })))
}

pub async fn count_all(
    State(state): State<AppState>,
    _auth_user: AdminUser,
) -> Result<impl IntoResponse, ApiError> {
    let count = state.services.background_update_service.count_all().await?;

    Ok(Json(serde_json::json!({
        "total_updates": count,
    })))
}

pub async fn get_stats(
    State(state): State<AppState>,
    _auth_user: AdminUser,
    Query(query): Query<QueryParams>,
) -> Result<impl IntoResponse, ApiError> {
    let days = query.limit.unwrap_or(30) as i32;

    let stats = state.services.background_update_service.get_stats(days).await?;

    let response: Vec<StatsResponse> = stats.into_iter().map(StatsResponse::from).collect();

    Ok(Json(response))
}

pub async fn get_next_pending(
    State(state): State<AppState>,
    _auth_user: AdminUser,
) -> Result<impl IntoResponse, ApiError> {
    let update = state.services.background_update_service.get_next_pending_update().await?;

    match update {
        Some(u) => Ok(Json(Some(UpdateResponse::from(u)))),
        None => Ok(Json(None::<UpdateResponse>)),
    }
}

pub fn create_background_update_router(state: AppState) -> Router<AppState> {
    Router::new()
        .route("/_synapse/admin/v1/background_updates", post(create_update))
        .route("/_synapse/admin/v1/background_updates", get(get_all_updates))
        .route("/_synapse/admin/v1/background_updates/count", get(count_all))
        .route("/_synapse/admin/v1/background_updates/pending", get(get_pending_updates))
        .route("/_synapse/admin/v1/background_updates/running", get(get_running_updates))
        .route("/_synapse/admin/v1/background_updates/next", get(get_next_pending))
        .route("/_synapse/admin/v1/background_updates/retry_failed", post(retry_failed))
        .route("/_synapse/admin/v1/background_updates/cleanup_locks", post(cleanup_locks))
        .route("/_synapse/admin/v1/background_updates/status/{status}/count", get(count_by_status))
        .route("/_synapse/admin/v1/background_updates/{job_name}", get(get_update))
        .route("/_synapse/admin/v1/background_updates/{job_name}", delete(delete_update))
        .route("/_synapse/admin/v1/background_updates/{job_name}/start", post(start_update))
        .route("/_synapse/admin/v1/background_updates/{job_name}/progress", post(update_progress))
        .route("/_synapse/admin/v1/background_updates/{job_name}/complete", post(complete_update))
        .route("/_synapse/admin/v1/background_updates/{job_name}/fail", post(fail_update))
        .route("/_synapse/admin/v1/background_updates/{job_name}/cancel", post(cancel_update))
        .route("/_synapse/admin/v1/background_updates/{job_name}/history", get(get_history))
        .route("/_synapse/admin/v1/background_updates/stats", get(get_stats))
        .with_state(state)
}
