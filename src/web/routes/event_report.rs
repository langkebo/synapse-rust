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
use crate::storage::event_report::{
    CreateEventReportRequest, UpdateEventReportRequest,
    EventReport, EventReportHistory, EventReportStats,
};
use crate::web::routes::{AuthenticatedUser, AdminUser, AppState};

#[derive(Debug, Deserialize)]
pub struct QueryParams {
    pub limit: Option<i64>,
    pub offset: Option<i64>,
}

#[derive(Debug, Deserialize)]
pub struct CreateReportBody {
    pub event_id: String,
    pub room_id: String,
    pub reported_user_id: Option<String>,
    pub event_json: Option<serde_json::Value>,
    pub reason: Option<String>,
    pub description: Option<String>,
    pub score: Option<i32>,
}

#[derive(Debug, Deserialize)]
pub struct UpdateReportBody {
    pub status: Option<String>,
    pub score: Option<i32>,
}

#[derive(Debug, Deserialize)]
pub struct ResolveReportBody {
    pub reason: String,
}

#[derive(Debug, Deserialize)]
pub struct DismissReportBody {
    pub reason: String,
}

#[derive(Debug, Deserialize)]
pub struct BlockUserBody {
    pub blocked_until: i64,
    pub reason: String,
}

#[derive(Debug, Serialize)]
pub struct ReportResponse {
    pub id: i64,
    pub event_id: String,
    pub room_id: String,
    pub reporter_user_id: String,
    pub reported_user_id: Option<String>,
    pub reason: Option<String>,
    pub description: Option<String>,
    pub status: String,
    pub score: i32,
    pub received_ts: i64,
    pub resolved_ts: Option<i64>,
    pub resolved_by: Option<String>,
    pub resolution_reason: Option<String>,
}

impl From<EventReport> for ReportResponse {
    fn from(r: EventReport) -> Self {
        Self {
            id: r.id,
            event_id: r.event_id,
            room_id: r.room_id,
            reporter_user_id: r.reporter_user_id,
            reported_user_id: r.reported_user_id,
            reason: r.reason,
            description: r.description,
            status: r.status,
            score: r.score,
            received_ts: r.received_ts,
            resolved_ts: r.resolved_ts,
            resolved_by: r.resolved_by,
            resolution_reason: r.resolution_reason,
        }
    }
}

#[derive(Debug, Serialize)]
pub struct ReportHistoryResponse {
    pub id: i64,
    pub report_id: i64,
    pub action: String,
    pub actor_user_id: Option<String>,
    pub old_status: Option<String>,
    pub new_status: Option<String>,
    pub reason: Option<String>,
    pub created_ts: i64,
}

impl From<EventReportHistory> for ReportHistoryResponse {
    fn from(h: EventReportHistory) -> Self {
        Self {
            id: h.id,
            report_id: h.report_id,
            action: h.action,
            actor_user_id: h.actor_user_id,
            old_status: h.old_status,
            new_status: h.new_status,
            reason: h.reason,
            created_ts: h.created_ts,
        }
    }
}

#[derive(Debug, Serialize)]
pub struct StatsResponse {
    pub stat_date: chrono::NaiveDate,
    pub total_reports: i32,
    pub open_reports: i32,
    pub resolved_reports: i32,
    pub dismissed_reports: i32,
}

impl From<EventReportStats> for StatsResponse {
    fn from(s: EventReportStats) -> Self {
        Self {
            stat_date: s.stat_date,
            total_reports: s.total_reports,
            open_reports: s.open_reports,
            resolved_reports: s.resolved_reports,
            dismissed_reports: s.dismissed_reports,
        }
    }
}

pub async fn create_report(
    State(state): State<AppState>,
    auth_user: AuthenticatedUser,
    Json(body): Json<CreateReportBody>,
) -> Result<impl IntoResponse, ApiError> {
    let request = CreateEventReportRequest {
        event_id: body.event_id,
        room_id: body.room_id,
        reporter_user_id: auth_user.user_id.clone(),
        reported_user_id: body.reported_user_id,
        event_json: body.event_json,
        reason: body.reason,
        description: body.description,
        score: body.score,
    };

    let report = state.services.event_report_service.create_report(request).await?;

    Ok((StatusCode::CREATED, Json(ReportResponse::from(report))))
}

pub async fn get_report(
    State(state): State<AppState>,
    _auth_user: AdminUser,
    Path(id): Path<i64>,
) -> Result<impl IntoResponse, ApiError> {
    let report = state.services.event_report_service.get_report(id).await?
        .ok_or_else(|| ApiError::not_found("Report not found"))?;

    Ok(Json(ReportResponse::from(report)))
}

pub async fn get_reports_by_event(
    State(state): State<AppState>,
    _auth_user: AdminUser,
    Path(event_id): Path<String>,
) -> Result<impl IntoResponse, ApiError> {
    let reports = state.services.event_report_service.get_reports_by_event(&event_id).await?;

    let response: Vec<ReportResponse> = reports.into_iter().map(ReportResponse::from).collect();

    Ok(Json(response))
}

pub async fn get_reports_by_room(
    State(state): State<AppState>,
    _auth_user: AdminUser,
    Path(room_id): Path<String>,
    Query(query): Query<QueryParams>,
) -> Result<impl IntoResponse, ApiError> {
    let limit = query.limit.unwrap_or(100);
    let offset = query.offset.unwrap_or(0);

    let reports = state.services.event_report_service.get_reports_by_room(&room_id, limit, offset).await?;

    let response: Vec<ReportResponse> = reports.into_iter().map(ReportResponse::from).collect();

    Ok(Json(response))
}

pub async fn get_reports_by_reporter(
    State(state): State<AppState>,
    _auth_user: AdminUser,
    Path(reporter_user_id): Path<String>,
    Query(query): Query<QueryParams>,
) -> Result<impl IntoResponse, ApiError> {
    let limit = query.limit.unwrap_or(100);
    let offset = query.offset.unwrap_or(0);

    let reports = state.services.event_report_service.get_reports_by_reporter(&reporter_user_id, limit, offset).await?;

    let response: Vec<ReportResponse> = reports.into_iter().map(ReportResponse::from).collect();

    Ok(Json(response))
}

pub async fn get_reports_by_status(
    State(state): State<AppState>,
    _auth_user: AdminUser,
    Path(status): Path<String>,
    Query(query): Query<QueryParams>,
) -> Result<impl IntoResponse, ApiError> {
    let limit = query.limit.unwrap_or(100);
    let offset = query.offset.unwrap_or(0);

    let reports = state.services.event_report_service.get_reports_by_status(&status, limit, offset).await?;

    let response: Vec<ReportResponse> = reports.into_iter().map(ReportResponse::from).collect();

    Ok(Json(response))
}

pub async fn get_all_reports(
    State(state): State<AppState>,
    _auth_user: AdminUser,
    Query(query): Query<QueryParams>,
) -> Result<impl IntoResponse, ApiError> {
    let limit = query.limit.unwrap_or(100);
    let offset = query.offset.unwrap_or(0);

    let reports = state.services.event_report_service.get_all_reports(limit, offset).await?;

    let response: Vec<ReportResponse> = reports.into_iter().map(ReportResponse::from).collect();

    Ok(Json(response))
}

pub async fn update_report(
    State(state): State<AppState>,
    _auth_user: AdminUser,
    Path(id): Path<i64>,
    Json(body): Json<UpdateReportBody>,
) -> Result<impl IntoResponse, ApiError> {
    let request = UpdateEventReportRequest {
        status: body.status,
        score: body.score,
        resolved_by: None,
        resolution_reason: None,
    };

    let report = state.services.event_report_service.update_report(id, request, &_auth_user.user_id).await?;

    Ok(Json(ReportResponse::from(report)))
}

pub async fn resolve_report(
    State(state): State<AppState>,
    _auth_user: AdminUser,
    Path(id): Path<i64>,
    Json(body): Json<ResolveReportBody>,
) -> Result<impl IntoResponse, ApiError> {
    let report = state.services.event_report_service.resolve_report(id, &_auth_user.user_id, &body.reason).await?;

    Ok(Json(ReportResponse::from(report)))
}

pub async fn dismiss_report(
    State(state): State<AppState>,
    _auth_user: AdminUser,
    Path(id): Path<i64>,
    Json(body): Json<DismissReportBody>,
) -> Result<impl IntoResponse, ApiError> {
    let report = state.services.event_report_service.dismiss_report(id, &_auth_user.user_id, &body.reason).await?;

    Ok(Json(ReportResponse::from(report)))
}

pub async fn escalate_report(
    State(state): State<AppState>,
    _auth_user: AdminUser,
    Path(id): Path<i64>,
) -> Result<impl IntoResponse, ApiError> {
    let report = state.services.event_report_service.escalate_report(id, &_auth_user.user_id).await?;

    Ok(Json(ReportResponse::from(report)))
}

pub async fn delete_report(
    State(state): State<AppState>,
    _auth_user: AdminUser,
    Path(id): Path<i64>,
) -> Result<impl IntoResponse, ApiError> {
    state.services.event_report_service.delete_report(id).await?;

    Ok(StatusCode::NO_CONTENT)
}

pub async fn get_report_history(
    State(state): State<AppState>,
    _auth_user: AdminUser,
    Path(id): Path<i64>,
) -> Result<impl IntoResponse, ApiError> {
    let history = state.services.event_report_service.get_report_history(id).await?;

    let response: Vec<ReportHistoryResponse> = history.into_iter().map(ReportHistoryResponse::from).collect();

    Ok(Json(response))
}

pub async fn check_rate_limit(
    State(state): State<AppState>,
    _auth_user: AdminUser,
    Path(user_id): Path<String>,
) -> Result<impl IntoResponse, ApiError> {
    let check = state.services.event_report_service.check_rate_limit(&user_id).await?;

    Ok(Json(serde_json::json!({
        "is_allowed": check.is_allowed,
        "remaining_reports": check.remaining_reports,
        "block_reason": check.block_reason,
    })))
}

pub async fn block_user(
    State(state): State<AppState>,
    _auth_user: AdminUser,
    Path(user_id): Path<String>,
    Json(body): Json<BlockUserBody>,
) -> Result<impl IntoResponse, ApiError> {
    state.services.event_report_service.block_user_reports(&user_id, body.blocked_until, &body.reason).await?;

    Ok(StatusCode::NO_CONTENT)
}

pub async fn unblock_user(
    State(state): State<AppState>,
    _auth_user: AdminUser,
    Path(user_id): Path<String>,
) -> Result<impl IntoResponse, ApiError> {
    state.services.event_report_service.unblock_user_reports(&user_id).await?;

    Ok(StatusCode::NO_CONTENT)
}

pub async fn get_stats(
    State(state): State<AppState>,
    _auth_user: AdminUser,
    Query(query): Query<QueryParams>,
) -> Result<impl IntoResponse, ApiError> {
    let days = query.limit.unwrap_or(30) as i32;

    let stats = state.services.event_report_service.get_stats(days).await?;

    let response: Vec<StatsResponse> = stats.into_iter().map(StatsResponse::from).collect();

    Ok(Json(response))
}

pub async fn count_by_status(
    State(state): State<AppState>,
    _auth_user: AdminUser,
    Path(status): Path<String>,
) -> Result<impl IntoResponse, ApiError> {
    let count = state.services.event_report_service.count_reports_by_status(&status).await?;

    Ok(Json(serde_json::json!({
        "status": status,
        "count": count,
    })))
}

pub async fn count_all(
    State(state): State<AppState>,
    _auth_user: AdminUser,
) -> Result<impl IntoResponse, ApiError> {
    let count = state.services.event_report_service.count_all_reports().await?;

    Ok(Json(serde_json::json!({
        "total_reports": count,
    })))
}

pub fn create_event_report_router(state: AppState) -> Router<AppState> {
    Router::new()
        .route("/_synapse/admin/v1/event_reports", post(create_report))
        .route("/_synapse/admin/v1/event_reports", get(get_all_reports))
        .route("/_synapse/admin/v1/event_reports/count", get(count_all))
        .route("/_synapse/admin/v1/event_reports/status/{status}", get(get_reports_by_status))
        .route("/_synapse/admin/v1/event_reports/status/{status}/count", get(count_by_status))
        .route("/_synapse/admin/v1/event_reports/{id}", get(get_report))
        .route("/_synapse/admin/v1/event_reports/{id}", put(update_report))
        .route("/_synapse/admin/v1/event_reports/{id}", delete(delete_report))
        .route("/_synapse/admin/v1/event_reports/{id}/resolve", post(resolve_report))
        .route("/_synapse/admin/v1/event_reports/{id}/dismiss", post(dismiss_report))
        .route("/_synapse/admin/v1/event_reports/{id}/escalate", post(escalate_report))
        .route("/_synapse/admin/v1/event_reports/{id}/history", get(get_report_history))
        .route("/_synapse/admin/v1/event_reports/event/{event_id}", get(get_reports_by_event))
        .route("/_synapse/admin/v1/event_reports/room/{room_id}", get(get_reports_by_room))
        .route("/_synapse/admin/v1/event_reports/reporter/{reporter_user_id}", get(get_reports_by_reporter))
        .route("/_synapse/admin/v1/event_reports/rate_limit/{user_id}", get(check_rate_limit))
        .route("/_synapse/admin/v1/event_reports/rate_limit/{user_id}/block", post(block_user))
        .route("/_synapse/admin/v1/event_reports/rate_limit/{user_id}/unblock", post(unblock_user))
        .route("/_synapse/admin/v1/event_reports/stats", get(get_stats))
        .with_state(state)
}
