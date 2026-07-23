use crate::common::ApiError;
use crate::common::{MAX_PAGINATION_LIMIT, MIN_PAGINATION_LIMIT};
use crate::web::routes::context::AdminContext;
use crate::web::routes::AdminUser;
use axum::{
    extract::{Path, State},
    routing::{delete, get},
    Json, Router,
};
use serde_json::{json, Value};
use synapse_storage::event_report::EventReport;

pub fn create_report_router() -> Router<crate::web::routes::AppState> {
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

pub fn admin_report_route_manifest() -> Vec<crate::web::routes::route_ledger::RouteEntry> {
    use crate::web::routes::route_ledger::RouteEntry;
    use axum::http::Method;
    [
        (Method::GET, "/_synapse/admin/v1/reports"),
        (Method::GET, "/_synapse/admin/v1/reports/{report_id}"),
        (Method::DELETE, "/_synapse/admin/v1/reports/{report_id}"),
        (Method::GET, "/_synapse/admin/v1/rooms/{room_id}/reports"),
        (Method::GET, "/_synapse/admin/v1/rooms/{room_id}/reports/{report_id}"),
        (Method::DELETE, "/_synapse/admin/v1/rooms/{room_id}/reports/{report_id}"),
    ]
    .into_iter()
    .map(|(m, p)| RouteEntry::new(m, p, "admin::report"))
    .collect()
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

#[axum::debug_handler]
pub async fn get_all_reports(
    _admin: AdminUser,
    State(ctx): State<AdminContext>,
    axum::extract::Query(params): axum::extract::Query<std::collections::HashMap<String, String>>,
) -> Result<Json<Value>, ApiError> {
    let limit = params
        .get("limit")
        .and_then(|v| v.parse().ok())
        .unwrap_or(100)
        .clamp(MIN_PAGINATION_LIMIT, MAX_PAGINATION_LIMIT);
    let since_score = params.get("since_score").and_then(|v| v.parse::<i32>().ok());
    let since_ts = params.get("since_ts").and_then(|v| v.parse::<i64>().ok());
    let since_id = params.get("since_id").and_then(|v| v.parse::<i64>().ok());

    if params.contains_key("offset") && params.get("offset").and_then(|v| v.parse::<i64>().ok()).unwrap_or(0) > 0 {
        return Err(ApiError::bad_request(
            "Legacy offset pagination is no longer supported; use since_ts/since_id cursors".to_string(),
        ));
    }

    let reports = ctx.event_report_service.get_all_reports(limit, since_score, since_ts, since_id).await?;
    let report_list: Vec<Value> = reports.iter().map(report_to_json).collect();

    Ok(Json(json!({ "reports": report_list, "total": report_list.len() })))
}

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

#[axum::debug_handler]
pub async fn delete_report(
    _admin: AdminUser,
    State(ctx): State<AdminContext>,
    Path(report_id): Path<i64>,
) -> Result<Json<Value>, ApiError> {
    ctx.event_report_service.delete_report(report_id).await?;

    Ok(Json(json!({})))
}

#[axum::debug_handler]
pub async fn get_room_reports(
    _admin: AdminUser,
    State(ctx): State<AdminContext>,
    Path(room_id): Path<String>,
    axum::extract::Query(params): axum::extract::Query<std::collections::HashMap<String, String>>,
) -> Result<Json<Value>, ApiError> {
    let room_exists = ctx.room_service.state().room_exists(&room_id).await?;

    if !room_exists {
        return Err(ApiError::not_found("Room not found".to_string()));
    }

    let limit = params
        .get("limit")
        .and_then(|v| v.parse().ok())
        .unwrap_or(100)
        .clamp(MIN_PAGINATION_LIMIT, MAX_PAGINATION_LIMIT);
    let since_ts = params.get("since_ts").and_then(|v| v.parse::<i64>().ok());
    let since_id = params.get("since_id").and_then(|v| v.parse::<i64>().ok());

    if params.contains_key("offset") && params.get("offset").and_then(|v| v.parse::<i64>().ok()).unwrap_or(0) > 0 {
        return Err(ApiError::bad_request(
            "Legacy offset pagination is no longer supported; use since_ts/since_id cursors".to_string(),
        ));
    }

    let reports = ctx.event_report_service.get_reports_by_room(&room_id, limit, since_ts, since_id).await?;
    let report_list: Vec<Value> = reports.iter().map(report_to_json).collect();

    Ok(Json(json!({ "reports": report_list, "total": report_list.len() })))
}

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
