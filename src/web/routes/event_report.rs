use crate::web::routes::context::AdminContext;
use axum::{
    extract::{Path, Query, State},
    http::StatusCode,
    response::IntoResponse,
    routing::{delete, get, post, put},
    Json, Router,
};
use serde::{Deserialize, Serialize};

use crate::common::ApiError;
use crate::web::routes::extractors::{EventId, RoomId, UserId};
use crate::web::routes::{AdminUser, AppState, AuthenticatedUser};
use synapse_storage::event_report::{
    CreateEventReportRequest, EventReport, EventReportHistory, EventReportStats, UpdateEventReportRequest,
};

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

/// The `ReportHistoryResponse` struct.
#[derive(Debug, Serialize)]
pub struct ReportHistoryResponse {
    /// The `id` field.
    pub id: i64,
    /// The `report_id` field.
    pub report_id: i64,
    /// The `action` field.
    pub action: String,
    /// The `actor_user_id` field.
    pub actor_user_id: Option<String>,
    /// The `old_status` field.
    pub old_status: Option<String>,
    /// The `new_status` field.
    pub new_status: Option<String>,
    /// The `reason` field.
    pub reason: Option<String>,
    /// The `created_ts` field.
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

/// The `StatsResponse` struct.
#[derive(Debug, Serialize)]
pub struct StatsResponse {
    /// The `id` field.
    pub id: i64,
    /// The `date` field.
    pub date: chrono::NaiveDate,
    /// The `total_reports` field.
    pub total_reports: i32,
    /// The `open_reports` field.
    pub open_reports: i32,
    /// The `resolved_reports` field.
    pub resolved_reports: i32,
    /// The `dismissed_reports` field.
    pub dismissed_reports: i32,
    /// The `avg_resolution_time_hours` field.
    pub avg_resolution_time_hours: Option<i32>,
    /// The `avg_resolution_time_ms` field.
    pub avg_resolution_time_ms: Option<i64>,
    /// The `created_ts` field.
    pub created_ts: i64,
    /// The `updated_ts` field.
    pub updated_ts: i64,
}

impl From<EventReportStats> for StatsResponse {
    fn from(s: EventReportStats) -> Self {
        Self {
            id: s.id,
            date: s.stat_date,
            total_reports: s.total_reports,
            open_reports: s.open_reports,
            resolved_reports: s.resolved_reports,
            dismissed_reports: s.dismissed_reports,
            avg_resolution_time_hours: s
                .avg_resolution_time_ms
                .and_then(|avg_resolution_time_ms| i32::try_from(avg_resolution_time_ms / 3_600_000).ok()),
            avg_resolution_time_ms: s.avg_resolution_time_ms,
            created_ts: s.created_ts,
            updated_ts: s.updated_ts,
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

/// See [`get_report_history`].
pub async fn get_report_history(
    State(ctx): State<AdminContext>,
    _auth_user: AdminUser,
    Path(id): Path<i64>,
) -> Result<impl IntoResponse, ApiError> {
    let history = ctx.event_report_service.get_report_history(id).await?;

    let response: Vec<ReportHistoryResponse> = history.into_iter().map(ReportHistoryResponse::from).collect();

    Ok(Json(response))
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

/// See [`get_stats`].
pub async fn get_stats(
    State(ctx): State<AdminContext>,
    _auth_user: AdminUser,
    Query(query): Query<QueryParams>,
) -> Result<impl IntoResponse, ApiError> {
    let days = query.limit.unwrap_or(30) as i32;

    let stats = ctx.event_report_service.get_stats(days).await?;

    let response: Vec<StatsResponse> = stats.into_iter().map(StatsResponse::from).collect();

    Ok(Json(response))
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
        .route("/_synapse/admin/v1/event_reports/{id}/history", get(get_report_history))
        .route("/_synapse/admin/v1/event_reports/event/{event_id}", get(get_reports_by_event))
        .route("/_synapse/admin/v1/event_reports/room/{room_id}", get(get_reports_by_room))
        .route("/_synapse/admin/v1/event_reports/reporter/{reporter_user_id}", get(get_reports_by_reporter))
        .route("/_synapse/admin/v1/event_reports/rate_limit/{user_id}", get(check_rate_limit))
        .route("/_synapse/admin/v1/event_reports/rate_limit/{user_id}/block", post(block_user))
        .route("/_synapse/admin/v1/event_reports/rate_limit/{user_id}/unblock", post(unblock_user))
        .route("/_synapse/admin/v1/event_reports/stats", get(get_stats))
        .route_layer(axum::middleware::from_fn_with_state(<crate::web::routes::context::AdminContext as axum::extract::FromRef<crate::web::routes::AppState>>::from_ref(&state), crate::web::middleware::admin_auth_middleware))
        .with_state(state)
}

/// See [`event_report_route_manifest`].
pub fn event_report_route_manifest() -> Vec<crate::web::routes::route_ledger::RouteEntry> {
    use crate::web::routes::route_ledger::RouteEntry;
    use axum::http::Method;
    [
        (Method::POST, "/_synapse/admin/v1/event_reports"),
        (Method::GET, "/_synapse/admin/v1/event_reports"),
        (Method::GET, "/_synapse/admin/v1/event_reports/count"),
        (Method::GET, "/_synapse/admin/v1/event_reports/status/{status}"),
        (Method::GET, "/_synapse/admin/v1/event_reports/status/{status}/count"),
        (Method::GET, "/_synapse/admin/v1/event_reports/{id}"),
        (Method::PUT, "/_synapse/admin/v1/event_reports/{id}"),
        (Method::DELETE, "/_synapse/admin/v1/event_reports/{id}"),
        (Method::POST, "/_synapse/admin/v1/event_reports/{id}/resolve"),
        (Method::POST, "/_synapse/admin/v1/event_reports/{id}/dismiss"),
        (Method::POST, "/_synapse/admin/v1/event_reports/{id}/escalate"),
        (Method::GET, "/_synapse/admin/v1/event_reports/{id}/history"),
        (Method::GET, "/_synapse/admin/v1/event_reports/event/{event_id}"),
        (Method::GET, "/_synapse/admin/v1/event_reports/room/{room_id}"),
        (Method::GET, "/_synapse/admin/v1/event_reports/reporter/{reporter_user_id}"),
        (Method::GET, "/_synapse/admin/v1/event_reports/rate_limit/{user_id}"),
        (Method::POST, "/_synapse/admin/v1/event_reports/rate_limit/{user_id}/block"),
        (Method::POST, "/_synapse/admin/v1/event_reports/rate_limit/{user_id}/unblock"),
        (Method::GET, "/_synapse/admin/v1/event_reports/stats"),
    ]
    .into_iter()
    .map(|(m, p)| RouteEntry::new(m, p, "event_report"))
    .collect()
}
