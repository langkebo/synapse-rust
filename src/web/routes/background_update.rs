use crate::web::routes::context::AdminContext;
use axum::{
    extract::{Path, Query, State},
    http::StatusCode,
    response::IntoResponse,
    routing::{delete, get, post},
    Json, Router,
};
use serde::{Deserialize, Serialize};

use crate::common::ApiError;
use crate::web::routes::AdminUser;
use crate::web::routes::AppState;
use synapse_storage::background_update::{
    BackgroundUpdate, BackgroundUpdateHistory, BackgroundUpdateStats, CreateBackgroundUpdateRequest,
};

/// The `QueryParams` struct.
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct QueryParams {
    /// The `limit` field.
    pub limit: Option<i64>,
    /// The `from` field.
    pub from: Option<String>,
}

/// The `CreateUpdateBody` struct.
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CreateUpdateBody {
    /// The `job_name` field.
    pub job_name: String,
    /// The `job_type` field.
    pub job_type: String,
    /// The `description` field.
    pub description: Option<String>,
    /// The `table_name` field.
    pub table_name: Option<String>,
    /// The `column_name` field.
    pub column_name: Option<String>,
    /// The `total_items` field.
    pub total_items: Option<i32>,
    /// The `batch_size` field.
    pub batch_size: Option<i32>,
    /// The `sleep_ms` field.
    pub sleep_ms: Option<i32>,
    /// The `depends_on` field.
    pub depends_on: Option<Vec<String>>,
    /// The `metadata` field.
    pub metadata: Option<serde_json::Value>,
}

/// The `UpdateProgressBody` struct.
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct UpdateProgressBody {
    /// The `items_processed` field.
    pub items_processed: i32,
    /// The `total_items` field.
    pub total_items: Option<i32>,
}

/// The `FailUpdateBody` struct.
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct FailUpdateBody {
    /// The `error_message` field.
    pub error_message: String,
}

/// The `UpdateResponse` struct.
#[derive(Debug, Serialize)]
pub struct UpdateResponse {
    /// The `job_name` field.
    pub job_name: String,
    /// The `job_type` field.
    pub job_type: String,
    /// The `description` field.
    pub description: Option<String>,
    /// The `table_name` field.
    pub table_name: Option<String>,
    /// The `status` field.
    pub status: String,
    /// The `progress` field.
    pub progress: serde_json::Value,
    /// The `total_items` field.
    pub total_items: i32,
    /// The `processed_items` field.
    pub processed_items: i32,
    /// The `created_ts` field.
    pub created_ts: i64,
    /// The `started_ts` field.
    pub started_ts: Option<i64>,
    /// The `completed_ts` field.
    pub completed_ts: Option<i64>,
    /// The `error_message` field.
    pub error_message: Option<String>,
    /// The `retry_count` field.
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
            created_ts: u.created_ts.unwrap_or(0),
            started_ts: u.started_ts,
            completed_ts: u.completed_ts,
            error_message: u.error_message,
            retry_count: u.retry_count,
        }
    }
}

/// The `HistoryResponse` struct.
#[derive(Debug, Serialize)]
pub struct HistoryResponse {
    /// The `id` field.
    pub id: i64,
    /// The `job_name` field.
    pub job_name: String,
    /// The `execution_start_ts` field.
    pub execution_start_ts: i64,
    /// The `execution_end_ts` field.
    pub execution_end_ts: Option<i64>,
    /// The `status` field.
    pub status: String,
    /// The `items_processed` field.
    pub items_processed: i32,
    /// The `error_message` field.
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

/// The `StatsResponse` struct.
#[derive(Debug, Serialize)]
pub struct StatsResponse {
    /// The `id` field.
    pub id: i64,
    /// The `job_name` field.
    pub job_name: String,
    /// The `total_updates` field.
    pub total_updates: i32,
    /// The `completed_updates` field.
    pub completed_updates: i32,
    /// The `failed_updates` field.
    pub failed_updates: i32,
    /// The `last_run_ts` field.
    pub last_run_ts: Option<i64>,
    /// The `next_run_ts` field.
    pub next_run_ts: Option<i64>,
    /// The `average_duration_ms` field.
    pub average_duration_ms: i64,
    /// The `created_ts` field.
    pub created_ts: i64,
    /// The `updated_ts` field.
    pub updated_ts: i64,
}

impl From<BackgroundUpdateStats> for StatsResponse {
    fn from(s: BackgroundUpdateStats) -> Self {
        Self {
            id: s.id,
            job_name: s.job_name,
            total_updates: s.total_updates,
            completed_updates: s.completed_updates,
            failed_updates: s.failed_updates,
            last_run_ts: s.last_run_ts,
            next_run_ts: s.next_run_ts,
            average_duration_ms: s.average_duration_ms,
            created_ts: s.created_ts,
            updated_ts: s.updated_ts,
        }
    }
}

/// See [`create_update`].
pub async fn create_update(
    State(ctx): State<AdminContext>,
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

    let update = ctx.background_update_service.create_update(request).await?;

    Ok((StatusCode::CREATED, Json(UpdateResponse::from(update))))
}

/// See [`get_update`].
pub async fn get_update(
    State(ctx): State<AdminContext>,
    _auth_user: AdminUser,
    Path(job_name): Path<String>,
) -> Result<impl IntoResponse, ApiError> {
    let update = ctx
        .background_update_service
        .get_update(&job_name)
        .await?
        .ok_or_else(|| ApiError::not_found("Update not found"))?;

    Ok(Json(UpdateResponse::from(update)))
}

/// See [`get_all_updates`].
pub async fn get_all_updates(
    State(ctx): State<AdminContext>,
    _auth_user: AdminUser,
    Query(query): Query<QueryParams>,
) -> Result<impl IntoResponse, ApiError> {
    let limit = query.limit.unwrap_or(100).clamp(1, 500);

    let (updates, next_batch) = ctx.background_update_service.get_all_updates(limit, query.from).await?;

    let response: Vec<UpdateResponse> = updates.into_iter().map(UpdateResponse::from).collect();

    Ok(Json(serde_json::json!({
        "updates": response,
        "next_batch": next_batch
    })))
}

/// See [`get_pending_updates`].
pub async fn get_pending_updates(
    State(ctx): State<AdminContext>,
    _auth_user: AdminUser,
) -> Result<impl IntoResponse, ApiError> {
    let updates = ctx.background_update_service.get_pending_updates().await?;

    let response: Vec<UpdateResponse> = updates.into_iter().map(UpdateResponse::from).collect();

    Ok(Json(response))
}

/// See [`get_running_updates`].
pub async fn get_running_updates(
    State(ctx): State<AdminContext>,
    _auth_user: AdminUser,
) -> Result<impl IntoResponse, ApiError> {
    let updates = ctx.background_update_service.get_running_updates().await?;

    let response: Vec<UpdateResponse> = updates.into_iter().map(UpdateResponse::from).collect();

    Ok(Json(response))
}

/// See [`start_update`].
pub async fn start_update(
    State(ctx): State<AdminContext>,
    _auth_user: AdminUser,
    Path(job_name): Path<String>,
) -> Result<impl IntoResponse, ApiError> {
    let update = ctx.background_update_service.start_update(&job_name).await?;

    Ok(Json(UpdateResponse::from(update)))
}

/// See [`update_progress`].
pub async fn update_progress(
    State(ctx): State<AdminContext>,
    _auth_user: AdminUser,
    Path(job_name): Path<String>,
    Json(body): Json<UpdateProgressBody>,
) -> Result<impl IntoResponse, ApiError> {
    let update =
        ctx.background_update_service.update_progress(&job_name, body.items_processed, body.total_items).await?;

    Ok(Json(UpdateResponse::from(update)))
}

/// See [`complete_update`].
pub async fn complete_update(
    State(ctx): State<AdminContext>,
    _auth_user: AdminUser,
    Path(job_name): Path<String>,
) -> Result<impl IntoResponse, ApiError> {
    let update = ctx.background_update_service.complete_update(&job_name).await?;

    Ok(Json(UpdateResponse::from(update)))
}

/// See [`fail_update`].
pub async fn fail_update(
    State(ctx): State<AdminContext>,
    _auth_user: AdminUser,
    Path(job_name): Path<String>,
    Json(body): Json<FailUpdateBody>,
) -> Result<impl IntoResponse, ApiError> {
    let update = ctx.background_update_service.fail_update(&job_name, &body.error_message).await?;

    Ok(Json(UpdateResponse::from(update)))
}

/// See [`cancel_update`].
pub async fn cancel_update(
    State(ctx): State<AdminContext>,
    _auth_user: AdminUser,
    Path(job_name): Path<String>,
) -> Result<impl IntoResponse, ApiError> {
    let update = ctx.background_update_service.cancel_update(&job_name).await?;

    Ok(Json(UpdateResponse::from(update)))
}

/// See [`delete_update`].
pub async fn delete_update(
    State(ctx): State<AdminContext>,
    _auth_user: AdminUser,
    Path(job_name): Path<String>,
) -> Result<impl IntoResponse, ApiError> {
    ctx.background_update_service.delete_update(&job_name).await?;

    Ok(StatusCode::NO_CONTENT)
}

/// See [`get_history`].
pub async fn get_history(
    State(ctx): State<AdminContext>,
    _auth_user: AdminUser,
    Path(job_name): Path<String>,
    Query(query): Query<QueryParams>,
) -> Result<impl IntoResponse, ApiError> {
    let limit = query.limit.unwrap_or(100).clamp(1, 500);

    let history = ctx.background_update_service.get_history(&job_name, limit).await?;

    let response: Vec<HistoryResponse> = history.into_iter().map(HistoryResponse::from).collect();

    Ok(Json(response))
}

/// See [`retry_failed`].
pub async fn retry_failed(
    State(ctx): State<AdminContext>,
    _auth_user: AdminUser,
) -> Result<impl IntoResponse, ApiError> {
    let count = ctx.background_update_service.retry_failed().await?;

    Ok(Json(serde_json::json!({
        "retried_count": count,
    })))
}

/// See [`cleanup_locks`].
pub async fn cleanup_locks(
    State(ctx): State<AdminContext>,
    _auth_user: AdminUser,
) -> Result<impl IntoResponse, ApiError> {
    let count = ctx.background_update_service.cleanup_expired_locks().await?;

    Ok(Json(serde_json::json!({
        "cleaned_count": count,
    })))
}

/// See [`count_by_status`].
pub async fn count_by_status(
    State(ctx): State<AdminContext>,
    _auth_user: AdminUser,
    Path(status): Path<String>,
) -> Result<impl IntoResponse, ApiError> {
    let count = ctx.background_update_service.count_by_status(&status).await?;

    Ok(Json(serde_json::json!({
        "status": status,
        "count": count,
    })))
}

/// See [`count_all`].
pub async fn count_all(State(ctx): State<AdminContext>, _auth_user: AdminUser) -> Result<impl IntoResponse, ApiError> {
    let count = ctx.background_update_service.count_all().await?;

    Ok(Json(serde_json::json!({
        "total_updates": count,
    })))
}

/// See [`get_stats`].
pub async fn get_stats(
    State(ctx): State<AdminContext>,
    _auth_user: AdminUser,
    Query(query): Query<QueryParams>,
) -> Result<impl IntoResponse, ApiError> {
    let days = query.limit.unwrap_or(30) as i32;

    let stats = ctx.background_update_service.get_stats(days).await?;

    let response: Vec<StatsResponse> = stats.into_iter().map(StatsResponse::from).collect();

    Ok(Json(response))
}

/// See [`get_next_pending`].
pub async fn get_next_pending(
    State(ctx): State<AdminContext>,
    _auth_user: AdminUser,
) -> Result<impl IntoResponse, ApiError> {
    let update = ctx.background_update_service.get_next_pending_update().await?;

    match update {
        Some(u) => Ok(Json(Some(UpdateResponse::from(u)))),
        None => Ok(Json(None::<UpdateResponse>)),
    }
}

/// The `BackgroundUpdateStatus` struct.
#[derive(Debug, Serialize)]
pub struct BackgroundUpdateStatus {
    /// The `pending_count` field.
    pub pending_count: i64,
    /// The `running_count` field.
    pub running_count: i64,
    /// The `completed_count` field.
    pub completed_count: i64,
    /// The `failed_count` field.
    pub failed_count: i64,
    /// The `total_count` field.
    pub total_count: i64,
    /// The `current_update` field.
    pub current_update: Option<UpdateResponse>,
}

/// See [`get_status`].
pub async fn get_status(State(ctx): State<AdminContext>, _auth_user: AdminUser) -> Result<impl IntoResponse, ApiError> {
    let pending = ctx.background_update_service.count_by_status("pending").await?;
    let running = ctx.background_update_service.count_by_status("running").await?;
    let completed = ctx.background_update_service.count_by_status("completed").await?;
    let failed = ctx.background_update_service.count_by_status("failed").await?;
    let total = ctx.background_update_service.count_all().await?;

    let current = ctx.background_update_service.get_next_pending_update().await?;

    Ok(Json(BackgroundUpdateStatus {
        pending_count: pending,
        running_count: running,
        completed_count: completed,
        failed_count: failed,
        total_count: total,
        current_update: current.map(UpdateResponse::from),
    }))
}

/// See [`create_background_update_router`].
pub fn create_background_update_router(state: AppState) -> Router<AppState> {
    Router::new()
        .route("/_synapse/admin/v1/background_updates", post(create_update))
        .route("/_synapse/admin/v1/background_updates", get(get_all_updates))
        .route("/_synapse/admin/v1/background_updates/count", get(count_all))
        .route("/_synapse/admin/v1/background_updates/pending", get(get_pending_updates))
        .route("/_synapse/admin/v1/background_updates/running", get(get_running_updates))
        .route("/_synapse/admin/v1/background_updates/next", get(get_next_pending))
        .route("/_synapse/admin/v1/background_updates/status", get(get_status))
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
        .route_layer(axum::middleware::from_fn_with_state(<crate::web::routes::context::AdminContext as axum::extract::FromRef<crate::web::routes::AppState>>::from_ref(&state), crate::web::middleware::admin_auth_middleware))
        .with_state(state)
}
