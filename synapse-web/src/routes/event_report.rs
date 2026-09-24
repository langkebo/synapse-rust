use crate::routes::context::AdminContext;
use axum::{
    extract::{Path, Query, State},
    http::StatusCode,
    response::IntoResponse,
    routing::{delete, get, post, put},
    Json, Router,
};
use serde::{Deserialize, Serialize};

use crate::routes::extractors::{EventId, RoomId, UserId};
use crate::routes::{AdminUser, AppState, AuthenticatedUser};
use synapse_common::ApiError;
use synapse_services::event_report_service::{CreateEventReportRequest, EventReport, UpdateEventReportRequest};

/// The `QueryParams` struct.
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct QueryParams {
    /// The `limit` field.
    pub limit: Option<i64>,
    /// The `since_score` field.
    pub since_score: Option<i32>,
    /// The `since_ts` field.
    pub since_ts: Option<i64>,
    /// The `since_id` field.
    pub since_id: Option<i64>,
}

/// The `CreateReportBody` struct.
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CreateReportBody {
    /// The `event_id` field.
    pub event_id: String,
    /// The `room_id` field.
    pub room_id: String,
    /// The `reported_user_id` field.
    pub reported_user_id: Option<String>,
    /// The `event_json` field.
    pub event_json: Option<serde_json::Value>,
    /// The `reason` field.
    pub reason: Option<String>,
    /// The `description` field.
    pub description: Option<String>,
    /// The `score` field.
    pub score: Option<i32>,
}

/// The `UpdateReportBody` struct.
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct UpdateReportBody {
    /// The `status` field.
    pub status: Option<String>,
    /// The `score` field.
    pub score: Option<i32>,
}

/// The `ResolveReportBody` struct.
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ResolveReportBody {
    /// The `reason` field.
    pub reason: String,
}

/// The `DismissReportBody` struct.
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DismissReportBody {
    /// The `reason` field.
    pub reason: String,
}

/// The `BlockUserBody` struct.
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct BlockUserBody {
    /// The `blocked_until` field.
    pub blocked_until: i64,
    /// The `reason` field.
    pub reason: String,
}

/// The `ReportResponse` struct.
#[derive(Debug, Serialize)]
pub struct ReportResponse {
    /// The `id` field.
    pub id: i64,
    /// The `event_id` field.
    pub event_id: String,
    /// The `room_id` field.
    pub room_id: String,
    /// The `reporter_user_id` field.
    pub reporter_user_id: String,
    /// The `reported_user_id` field.
    pub reported_user_id: Option<String>,
    /// The `reason` field.
    pub reason: Option<String>,
    /// The `description` field.
    pub description: Option<String>,
    /// The `status` field.
    pub status: String,
    /// The `score` field.
    pub score: i32,
    /// The `received_ts` field.
    pub received_ts: i64,
    /// The `resolved_ts` field.
    pub resolved_ts: Option<i64>,
    /// The `resolved_by` field.
    pub resolved_by: Option<String>,
    /// The `resolution_reason` field.
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

/// See [`create_report`].
pub async fn create_report(
    State(ctx): State<AdminContext>,
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

    let report = ctx.event_report_service.create_report(request).await?;

    Ok((StatusCode::CREATED, Json(ReportResponse::from(report))))
}

/// See [`get_report`].
pub async fn get_report(
    State(ctx): State<AdminContext>,
    _auth_user: AdminUser,
    Path(id): Path<i64>,
) -> Result<impl IntoResponse, ApiError> {
    let report =
        ctx.event_report_service.get_report(id).await?.ok_or_else(|| ApiError::not_found("Report not found"))?;

    Ok(Json(ReportResponse::from(report)))
}

/// See [`get_reports_by_event`].
pub async fn get_reports_by_event(
    State(ctx): State<AdminContext>,
    _auth_user: AdminUser,
    Path(event_id): Path<EventId>,
) -> Result<impl IntoResponse, ApiError> {
    let reports = ctx.event_report_service.get_reports_by_event(&event_id).await?;

    let response: Vec<ReportResponse> = reports.into_iter().map(ReportResponse::from).collect();

    Ok(Json(response))
}

/// See [`get_reports_by_room`].
pub async fn get_reports_by_room(
    State(ctx): State<AdminContext>,
    _auth_user: AdminUser,
    Path(room_id): Path<RoomId>,
    Query(query): Query<QueryParams>,
) -> Result<impl IntoResponse, ApiError> {
    let limit = query.limit.unwrap_or(100);

    let reports = ctx.event_report_service.get_reports_by_room(&room_id, limit, query.since_ts, query.since_id).await?;

    let response: Vec<ReportResponse> = reports.into_iter().map(ReportResponse::from).collect();

    Ok(Json(response))
}

/// See [`get_reports_by_reporter`].
pub async fn get_reports_by_reporter(
    State(ctx): State<AdminContext>,
    _auth_user: AdminUser,
    Path(reporter_user_id): Path<UserId>,
    Query(query): Query<QueryParams>,
) -> Result<impl IntoResponse, ApiError> {
    let limit = query.limit.unwrap_or(100);

    let reports = ctx
        .event_report_service
        .get_reports_by_reporter(&reporter_user_id, limit, query.since_ts, query.since_id)
        .await?;

    let response: Vec<ReportResponse> = reports.into_iter().map(ReportResponse::from).collect();

    Ok(Json(response))
}

/// See [`get_reports_by_status`].
pub async fn get_reports_by_status(
    State(ctx): State<AdminContext>,
    _auth_user: AdminUser,
    Path(status): Path<RoomId>,
    Query(query): Query<QueryParams>,
) -> Result<impl IntoResponse, ApiError> {
    let limit = query.limit.unwrap_or(100);

    let reports = ctx
        .event_report_service
        .get_reports_by_status(&status, limit, query.since_score, query.since_ts, query.since_id)
        .await?;

    let response: Vec<ReportResponse> = reports.into_iter().map(ReportResponse::from).collect();

    Ok(Json(response))
}

/// See [`get_all_reports`].
pub async fn get_all_reports(
    State(ctx): State<AdminContext>,
    _auth_user: AdminUser,
    Query(query): Query<QueryParams>,
) -> Result<impl IntoResponse, ApiError> {
    let limit = query.limit.unwrap_or(100);

    let reports =
        ctx.event_report_service.get_all_reports(limit, query.since_score, query.since_ts, query.since_id).await?;

    let response: Vec<ReportResponse> = reports.into_iter().map(ReportResponse::from).collect();

    Ok(Json(response))
}

/// See [`update_report`].
pub async fn update_report(
    State(ctx): State<AdminContext>,
    _auth_user: AdminUser,
    Path(id): Path<i64>,
    Json(body): Json<UpdateReportBody>,
) -> Result<impl IntoResponse, ApiError> {
    let request =
        UpdateEventReportRequest { status: body.status, score: body.score, resolved_by: None, resolution_reason: None };

    let report = ctx.event_report_service.update_report(id, request, &_auth_user.user_id).await?;

    Ok(Json(ReportResponse::from(report)))
}

/// See [`resolve_report`].
pub async fn resolve_report(
    State(ctx): State<AdminContext>,
    _auth_user: AdminUser,
    Path(id): Path<i64>,
    Json(body): Json<ResolveReportBody>,
) -> Result<impl IntoResponse, ApiError> {
    let report = ctx.event_report_service.resolve_report(id, &_auth_user.user_id, &body.reason).await?;

    Ok(Json(ReportResponse::from(report)))
}

/// See [`dismiss_report`].
pub async fn dismiss_report(
    State(ctx): State<AdminContext>,
    _auth_user: AdminUser,
    Path(id): Path<i64>,
    Json(body): Json<DismissReportBody>,
) -> Result<impl IntoResponse, ApiError> {
    let report = ctx.event_report_service.dismiss_report(id, &_auth_user.user_id, &body.reason).await?;

    Ok(Json(ReportResponse::from(report)))
}

/// See [`escalate_report`].
pub async fn escalate_report(
    State(ctx): State<AdminContext>,
    _auth_user: AdminUser,
    Path(id): Path<i64>,
) -> Result<impl IntoResponse, ApiError> {
    let report = ctx.event_report_service.escalate_report(id, &_auth_user.user_id).await?;

    Ok(Json(ReportResponse::from(report)))
}

/// See [`delete_report`].
pub async fn delete_report(
    State(ctx): State<AdminContext>,
    _auth_user: AdminUser,
    Path(id): Path<i64>,
) -> Result<impl IntoResponse, ApiError> {
    ctx.event_report_service.delete_report(id).await?;

    Ok(StatusCode::NO_CONTENT)
}

/// See [`check_rate_limit`].
pub async fn check_rate_limit(
    State(ctx): State<AdminContext>,
    _auth_user: AdminUser,
    Path(user_id): Path<UserId>,
) -> Result<impl IntoResponse, ApiError> {
    let check = ctx.event_report_service.check_rate_limit(&user_id).await?;

    Ok(Json(serde_json::json!({
        "is_allowed": check.is_allowed,
        "remaining_reports": check.remaining_reports,
        "block_reason": check.block_reason,
    })))
}

/// See [`block_user`].
pub async fn block_user(
    State(ctx): State<AdminContext>,
    _auth_user: AdminUser,
    Path(user_id): Path<UserId>,
    Json(body): Json<BlockUserBody>,
) -> Result<impl IntoResponse, ApiError> {
    ctx.event_report_service.block_user_reports(&user_id, body.blocked_until, &body.reason).await?;

    Ok(StatusCode::NO_CONTENT)
}

/// See [`unblock_user`].
pub async fn unblock_user(
    State(ctx): State<AdminContext>,
    _auth_user: AdminUser,
    Path(user_id): Path<UserId>,
) -> Result<impl IntoResponse, ApiError> {
    ctx.event_report_service.unblock_user_reports(&user_id).await?;

    Ok(StatusCode::NO_CONTENT)
}

/// See [`count_by_status`].
pub async fn count_by_status(
    State(ctx): State<AdminContext>,
    _auth_user: AdminUser,
    Path(status): Path<RoomId>,
) -> Result<impl IntoResponse, ApiError> {
    let count = ctx.event_report_service.count_reports_by_status(&status).await?;

    Ok(Json(serde_json::json!({
        "status": status,
        "count": count,
    })))
}

/// See [`count_all`].
pub async fn count_all(State(ctx): State<AdminContext>, _auth_user: AdminUser) -> Result<impl IntoResponse, ApiError> {
    let count = ctx.event_report_service.count_all_reports().await?;

    Ok(Json(serde_json::json!({
        "total_reports": count,
    })))
}

/// See [`get_aggregate_stats`].
///
/// 实时聚合 `event_reports`（取代原先恒返回 `[]` 的空壳实现）。响应字段与 SDK 侧
/// 契约 `matrix-js-sdk` `src/event-report/index.ts::StatsResponse`
/// （`{ total, open, resolved, dismissed, escalated }`）逐字段一致 ——
/// 该端点不再需要 `?limit=` 之类的查询参数，因此不挂 `Query` 提取器。
pub async fn get_aggregate_stats(
    State(ctx): State<AdminContext>,
    _auth_user: AdminUser,
) -> Result<impl IntoResponse, ApiError> {
    let stats = ctx.event_report_service.get_aggregate_stats().await?;

    Ok(Json(serde_json::json!({
        "total": stats.total,
        "open": stats.open,
        "resolved": stats.resolved,
        "dismissed": stats.dismissed,
        "escalated": stats.escalated,
    })))
}

/// See [`create_event_report_router`].
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
        .route("/_synapse/admin/v1/event_reports/event/{event_id}", get(get_reports_by_event))
        .route("/_synapse/admin/v1/event_reports/room/{room_id}", get(get_reports_by_room))
        .route("/_synapse/admin/v1/event_reports/reporter/{reporter_user_id}", get(get_reports_by_reporter))
        .route("/_synapse/admin/v1/event_reports/rate_limit/{user_id}", get(check_rate_limit))
        .route("/_synapse/admin/v1/event_reports/rate_limit/{user_id}/block", post(block_user))
        .route("/_synapse/admin/v1/event_reports/rate_limit/{user_id}/unblock", post(unblock_user))
        .route("/_synapse/admin/v1/event_reports/stats", get(get_aggregate_stats))
        .route_layer(axum::middleware::from_fn_with_state(
            <crate::routes::context::AdminContext as axum::extract::FromRef<crate::routes::AppState>>::from_ref(&state),
            crate::middleware::admin_auth_middleware,
        ))
        .with_state(state)
}
