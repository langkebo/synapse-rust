use crate::common::ApiError;
use crate::storage::media_quota::{CreateQuotaConfigRequest, SetUserQuotaRequest};
use crate::web::routes::AppState;
use axum::{
    extract::{Path, Query, State},
    http::StatusCode,
    response::IntoResponse,
    Json, Router,
    routing::{delete, get, post, put},
};
use serde::{Deserialize, Serialize};

#[derive(Debug, Deserialize)]
struct CheckQuotaQuery {
    file_size: i64,
}

#[derive(Debug, Deserialize)]
struct RecordUploadBody {
    media_id: String,
    file_size: i64,
    mime_type: Option<String>,
}

#[derive(Debug, Deserialize)]
struct RecordDeleteBody {
    media_id: String,
    file_size: i64,
}

#[derive(Debug, Deserialize)]
struct CreateConfigBody {
    name: String,
    description: Option<String>,
    max_storage_bytes: i64,
    max_file_size_bytes: i64,
    max_files_count: i32,
    allowed_mime_types: Option<Vec<String>>,
    blocked_mime_types: Option<Vec<String>>,
    is_default: Option<bool>,
}

#[derive(Debug, Deserialize)]
struct SetUserQuotaBody {
    user_id: String,
    quota_config_id: Option<i32>,
    custom_max_storage_bytes: Option<i64>,
    custom_max_file_size_bytes: Option<i64>,
    custom_max_files_count: Option<i32>,
}

#[derive(Debug, Deserialize)]
struct UpdateServerQuotaBody {
    max_storage_bytes: Option<i64>,
    max_file_size_bytes: Option<i64>,
    max_files_count: Option<i32>,
    alert_threshold_percent: Option<i32>,
}

#[derive(Debug, Deserialize)]
struct AlertsQuery {
    unread_only: Option<bool>,
}

#[derive(Debug, Serialize)]
struct QuotaCheckResponse {
    allowed: bool,
    reason: Option<String>,
    current_usage: i64,
    quota_limit: i64,
    usage_percent: f64,
}

impl From<crate::storage::media_quota::QuotaCheckResult> for QuotaCheckResponse {
    fn from(r: crate::storage::media_quota::QuotaCheckResult) -> Self {
        Self {
            allowed: r.allowed,
            reason: r.reason,
            current_usage: r.current_usage,
            quota_limit: r.quota_limit,
            usage_percent: r.usage_percent,
        }
    }
}

pub fn create_media_quota_router() -> Router<AppState> {
    Router::new()
        .route("/_matrix/media/v1/quota/check", get(check_quota))
        .route("/_matrix/media/v1/quota/upload", post(record_upload))
        .route("/_matrix/media/v1/quota/delete", post(record_delete))
        .route("/_matrix/media/v1/quota/stats", get(get_usage_stats))
        .route("/_matrix/media/v1/quota/alerts", get(get_alerts))
        .route("/_matrix/media/v1/quota/alerts/{alert_id}/read", put(mark_alert_read))
        .route("/_matrix/admin/v1/media/quota/configs", get(list_configs))
        .route("/_matrix/admin/v1/media/quota/configs", post(create_config))
        .route("/_matrix/admin/v1/media/quota/configs/{config_id}", delete(delete_config))
        .route("/_matrix/admin/v1/media/quota/users", post(set_user_quota))
        .route("/_matrix/admin/v1/media/quota/server", get(get_server_quota))
        .route("/_matrix/admin/v1/media/quota/server", put(update_server_quota))
}

async fn check_quota(
    State(state): State<AppState>,
    Query(query): Query<CheckQuotaQuery>,
) -> Result<impl IntoResponse, ApiError> {
    let user_id = "system";
    let result = state.services.media_quota_service
        .check_upload_quota(user_id, query.file_size)
        .await?;

    Ok(Json(QuotaCheckResponse::from(result)))
}

async fn record_upload(
    State(state): State<AppState>,
    Json(body): Json<RecordUploadBody>,
) -> Result<impl IntoResponse, ApiError> {
    let user_id = "system";
    state.services.media_quota_service
        .record_upload(user_id, &body.media_id, body.file_size, body.mime_type.as_deref())
        .await?;

    Ok(StatusCode::NO_CONTENT)
}

async fn record_delete(
    State(state): State<AppState>,
    Json(body): Json<RecordDeleteBody>,
) -> Result<impl IntoResponse, ApiError> {
    let user_id = "system";
    state.services.media_quota_service
        .record_delete(user_id, &body.media_id, body.file_size)
        .await?;

    Ok(StatusCode::NO_CONTENT)
}

async fn get_usage_stats(
    State(state): State<AppState>,
) -> Result<impl IntoResponse, ApiError> {
    let user_id = "system";
    let stats = state.services.media_quota_service.get_usage_stats(user_id).await?;
    Ok(Json(stats))
}

async fn get_alerts(
    State(state): State<AppState>,
    Query(query): Query<AlertsQuery>,
) -> Result<impl IntoResponse, ApiError> {
    let user_id = "system";
    let alerts = state.services.media_quota_service
        .get_user_alerts(user_id, query.unread_only.unwrap_or(false))
        .await?;
    Ok(Json(alerts))
}

async fn mark_alert_read(
    State(state): State<AppState>,
    Path(alert_id): Path<i32>,
) -> Result<impl IntoResponse, ApiError> {
    let success = state.services.media_quota_service.mark_alert_read(alert_id).await?;
    if success {
        Ok(StatusCode::NO_CONTENT)
    } else {
        Err(ApiError::not_found("Alert not found"))
    }
}

async fn list_configs(
    State(state): State<AppState>,
) -> Result<impl IntoResponse, ApiError> {
    let configs = state.services.media_quota_service.list_quota_configs().await?;
    Ok(Json(configs))
}

async fn create_config(
    State(state): State<AppState>,
    Json(body): Json<CreateConfigBody>,
) -> Result<impl IntoResponse, ApiError> {
    let request = CreateQuotaConfigRequest {
        name: body.name,
        description: body.description,
        max_storage_bytes: body.max_storage_bytes,
        max_file_size_bytes: body.max_file_size_bytes,
        max_files_count: body.max_files_count,
        allowed_mime_types: body.allowed_mime_types,
        blocked_mime_types: body.blocked_mime_types,
        is_default: body.is_default,
    };

    let config = state.services.media_quota_service.create_quota_config(request).await?;
    Ok((StatusCode::CREATED, Json(config)))
}

async fn delete_config(
    State(state): State<AppState>,
    Path(config_id): Path<i32>,
) -> Result<impl IntoResponse, ApiError> {
    let deleted = state.services.media_quota_service.delete_quota_config(config_id).await?;
    if deleted {
        Ok(StatusCode::NO_CONTENT)
    } else {
        Err(ApiError::not_found("Config not found"))
    }
}

async fn set_user_quota(
    State(state): State<AppState>,
    Json(body): Json<SetUserQuotaBody>,
) -> Result<impl IntoResponse, ApiError> {
    let request = SetUserQuotaRequest {
        user_id: body.user_id,
        quota_config_id: body.quota_config_id,
        custom_max_storage_bytes: body.custom_max_storage_bytes,
        custom_max_file_size_bytes: body.custom_max_file_size_bytes,
        custom_max_files_count: body.custom_max_files_count,
    };

    let quota = state.services.media_quota_service.set_user_quota(request).await?;
    Ok(Json(quota))
}

async fn get_server_quota(
    State(state): State<AppState>,
) -> Result<impl IntoResponse, ApiError> {
    let quota = state.services.media_quota_service.get_server_quota().await?;
    Ok(Json(quota))
}

async fn update_server_quota(
    State(state): State<AppState>,
    Json(body): Json<UpdateServerQuotaBody>,
) -> Result<impl IntoResponse, ApiError> {
    let quota = state.services.media_quota_service
        .update_server_quota(
            body.max_storage_bytes,
            body.max_file_size_bytes,
            body.max_files_count,
            body.alert_threshold_percent,
        )
        .await?;
    Ok(Json(quota))
}
