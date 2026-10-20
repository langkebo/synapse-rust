use crate::routes::context::AdminContext;
use crate::routes::extractors::RoomId;
use crate::routes::AdminUser;
use axum::{
    extract::{Path, State},
    routing::{delete, get},
    Json, Router,
};
use serde_json::{json, Value};
use synapse_common::ApiError;
use synapse_common::{MAX_PAGINATION_LIMIT, MIN_PAGINATION_LIMIT};
use synapse_services::event_report_service::EventReport;

/// See [`create_report_router`].
pub fn create_report_router() -> Router<crate::routes::AppState> {
    Router::new()
        .route("/_synapse/admin/v1/reports", get(get_all_reports))
        .route("/_synapse/admin/v1/reports/{report_id}", get(get_report))
        .route("/_synapse/admin/v1/reports/{report_id}", delete(delete_report))
        .route("/_synapse/admin/v1/rooms/{room_id}/reports", get(get_room_reports))
        .route(
            "/_synapse/admin/v1/rooms/{room_id}/reports/{report_id}",
            get(get_room_report).delete(delete_room_report),
        )
}

fn report_to_json(report: &EventReport) -> Value {
    json!({
        "id": report.id,
        "room_id": report.room_id,
        "event_id": report.event_id,
        "user_id": report.reporter_user_id,
        "reported_user_id": report.reported_user_id,
        "reason": report.reason,
        "content": report.description,
        "status": report.status,
        "score": report.score,
        "received_ts": report.received_ts
    })
}

/// Query parameters accepted by [`get_all_reports`].
///
/// Deliberately **not** `deny_unknown_fields`: the handler used to take a bare
/// `HashMap<String, String>` and ignored unknown parameters.
#[derive(Debug, Default, serde::Deserialize)]
pub struct ReportsQuery {
    limit: Option<String>,
    since_score: Option<String>,
    since_ts: Option<String>,
    since_id: Option<String>,
    offset: Option<String>,
}

/// Query parameters accepted by [`get_room_reports`].
#[derive(Debug, Default, serde::Deserialize)]
pub struct RoomReportsQuery {
    limit: Option<String>,
    since_ts: Option<String>,
    since_id: Option<String>,
    offset: Option<String>,
}

/// See [`get_all_reports`].
#[axum::debug_handler]
pub async fn get_all_reports(
    _admin: AdminUser,
    State(ctx): State<AdminContext>,
    axum::extract::Query(params): axum::extract::Query<ReportsQuery>,
) -> Result<Json<Value>, ApiError> {
    let limit = params
        .limit
        .as_deref()
        .and_then(|v| v.parse().ok())
        .unwrap_or(100)
        .clamp(MIN_PAGINATION_LIMIT, MAX_PAGINATION_LIMIT);
    let since_score = params.since_score.as_deref().and_then(|v| v.parse::<i32>().ok());
    let since_ts = params.since_ts.as_deref().and_then(|v| v.parse::<i64>().ok());
    let since_id = params.since_id.as_deref().and_then(|v| v.parse::<i64>().ok());

    if params.offset.as_deref().and_then(|v| v.parse::<i64>().ok()).unwrap_or(0) > 0 {
        return Err(ApiError::bad_request(
            "Legacy offset pagination is no longer supported; use since_ts/since_id cursors".to_string(),
        ));
    }

    let reports = ctx.event_report_service.get_all_reports(limit, since_score, since_ts, since_id).await?;
    let report_list: Vec<Value> = reports.iter().map(report_to_json).collect();

    Ok(Json(json!({ "reports": report_list, "total": report_list.len() })))
}

/// See [`get_report`].
#[axum::debug_handler]
pub async fn get_report(
    _admin: AdminUser,
    State(ctx): State<AdminContext>,
    Path(report_id): Path<i64>,
) -> Result<Json<Value>, ApiError> {
    let report = ctx.event_report_service.get_report(report_id).await?;

    match report {
        Some(report) => Ok(Json(report_to_json(&report))),
        None => Err(ApiError::not_found("Report not found".to_string())),
    }
}

/// See [`delete_report`].
#[axum::debug_handler]
pub async fn delete_report(
    _admin: AdminUser,
    State(ctx): State<AdminContext>,
    Path(report_id): Path<i64>,
) -> Result<Json<Value>, ApiError> {
    ctx.event_report_service.delete_report(report_id).await?;

    Ok(Json(json!({})))
}

/// See [`get_room_reports`].
#[axum::debug_handler]
pub async fn get_room_reports(
    _admin: AdminUser,
    State(ctx): State<AdminContext>,
    Path(room_id): Path<RoomId>,
    axum::extract::Query(params): axum::extract::Query<RoomReportsQuery>,
) -> Result<Json<Value>, ApiError> {
    let room_exists = ctx.room_service.state().room_exists(&room_id).await?;

    if !room_exists {
        return Err(ApiError::not_found("Room not found".to_string()));
    }

    let limit = params
        .limit
        .as_deref()
        .and_then(|v| v.parse().ok())
        .unwrap_or(100)
        .clamp(MIN_PAGINATION_LIMIT, MAX_PAGINATION_LIMIT);
    let since_ts = params.since_ts.as_deref().and_then(|v| v.parse::<i64>().ok());
    let since_id = params.since_id.as_deref().and_then(|v| v.parse::<i64>().ok());

    if params.offset.as_deref().and_then(|v| v.parse::<i64>().ok()).unwrap_or(0) > 0 {
        return Err(ApiError::bad_request(
            "Legacy offset pagination is no longer supported; use since_ts/since_id cursors".to_string(),
        ));
    }

    let reports = ctx.event_report_service.get_reports_by_room(&room_id, limit, since_ts, since_id).await?;
    let report_list: Vec<Value> = reports.iter().map(report_to_json).collect();

    Ok(Json(json!({ "reports": report_list, "total": report_list.len() })))
}

/// See [`get_room_report`].
#[axum::debug_handler]
pub async fn get_room_report(
    _admin: AdminUser,
    State(ctx): State<AdminContext>,
    Path((room_id, report_id)): Path<(String, i64)>,
) -> Result<Json<Value>, ApiError> {
    let room_exists = ctx.room_service.state().room_exists(&room_id).await?;

    if !room_exists {
        return Err(ApiError::not_found("Room not found".to_string()));
    }

    let report = ctx.event_report_service.get_report(report_id).await?;

    match report {
        Some(report) if report.room_id == room_id => Ok(Json(report_to_json(&report))),
        _ => Err(ApiError::not_found("Report not found".to_string())),
    }
}

/// See [`delete_room_report`].
#[axum::debug_handler]
pub async fn delete_room_report(
    _admin: AdminUser,
    State(ctx): State<AdminContext>,
    Path((room_id, report_id)): Path<(String, i64)>,
) -> Result<Json<Value>, ApiError> {
    let room_exists = ctx.room_service.state().room_exists(&room_id).await?;

    if !room_exists {
        return Err(ApiError::not_found("Room not found".to_string()));
    }

    let report = ctx.event_report_service.get_report(report_id).await?;

    match report {
        Some(report) if report.room_id == room_id => {
            ctx.event_report_service.delete_report(report_id).await?;
            Ok(Json(json!({})))
        }
        _ => Err(ApiError::not_found("Report not found".to_string())),
    }
}
