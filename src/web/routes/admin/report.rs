use crate::common::constants::{MAX_PAGINATION_LIMIT, MIN_PAGINATION_LIMIT};
use crate::common::ApiError;
use crate::web::routes::{AdminUser, AppState};
use axum::{
    extract::{Path, State},
    routing::{delete, get},
    Json, Router,
};
use serde_json::{json, Value};
use sqlx::Row;

pub fn create_report_router(_state: AppState) -> Router<AppState> {
    Router::new()
        .route("/_synapse/admin/v1/reports", get(get_all_reports))
        .route("/_synapse/admin/v1/reports/{report_id}", get(get_report))
        .route("/_synapse/admin/v1/reports/{report_id}", delete(delete_report))
        .route("/_synapse/admin/v1/rooms/{room_id}/reports", get(get_room_reports))
        .route("/_synapse/admin/v1/rooms/{room_id}/reports/{report_id}", get(get_room_report))
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
    ]
    .into_iter()
    .map(|(m, p)| RouteEntry::new(m, p, "admin::report"))
    .collect()
}

#[axum::debug_handler]
pub async fn get_all_reports(
    _admin: AdminUser,
    State(state): State<AppState>,
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

    let reports = if let (Some(score), Some(ts), Some(id)) = (since_score, since_ts, since_id) {
        sqlx::query(
            "SELECT id, room_id, event_id, reporter_user_id, reported_user_id, reason, description, status, score, received_ts FROM event_reports WHERE (score < $2 OR (score = $2 AND received_ts < $3) OR (score = $2 AND received_ts = $3 AND id < $4)) ORDER BY score DESC, received_ts DESC, id DESC LIMIT $1"
        )
        .bind(limit)
        .bind(score)
        .bind(ts)
        .bind(id)
        .fetch_all(&*state.services.rooms.event_storage.pool)
        .await
    } else {
        sqlx::query(
            "SELECT id, room_id, event_id, reporter_user_id, reported_user_id, reason, description, status, score, received_ts FROM event_reports ORDER BY score DESC, received_ts DESC, id DESC LIMIT $1"
        )
        .bind(limit)
        .fetch_all(&*state.services.rooms.event_storage.pool)
        .await
    }
    .map_err(|e| ApiError::internal_with_log("Database error", &e))?;

    let report_list: Vec<Value> = reports
        .iter()
        .map(|row| {
            json!({
                "id": row.get::<i64, _>("id"),
                "room_id": row.get::<String, _>("room_id"),
                "event_id": row.get::<String, _>("event_id"),
                "user_id": row.get::<String, _>("reporter_user_id"),
                "reported_user_id": row.get::<Option<String>, _>("reported_user_id"),
                "reason": row.get::<Option<String>, _>("reason"),
                "content": row.get::<Option<String>, _>("description"),
                "status": row.get::<Option<String>, _>("status"),
                "score": row.get::<Option<i32>, _>("score"),
                "received_ts": row.get::<i64, _>("received_ts")
            })
        })
        .collect();

    Ok(Json(json!({ "reports": report_list, "total": report_list.len() })))
}

#[axum::debug_handler]
pub async fn get_report(
    _admin: AdminUser,
    State(state): State<AppState>,
    Path(report_id): Path<i64>,
) -> Result<Json<Value>, ApiError> {
    let report = sqlx::query(
        "SELECT id, room_id, event_id, reporter_user_id, reported_user_id, reason, description, status, score, received_ts FROM event_reports WHERE id = $1"
    )
    .bind(report_id)
    .fetch_optional(&*state.services.rooms.event_storage.pool)
    .await
    .map_err(|e| ApiError::internal_with_log("Database error", &e))?;

    match report {
        Some(row) => Ok(Json(json!({
            "id": row.get::<i64, _>("id"),
            "room_id": row.get::<String, _>("room_id"),
            "event_id": row.get::<String, _>("event_id"),
            "user_id": row.get::<String, _>("reporter_user_id"),
            "reported_user_id": row.get::<Option<String>, _>("reported_user_id"),
            "reason": row.get::<Option<String>, _>("reason"),
            "content": row.get::<Option<String>, _>("description"),
            "status": row.get::<Option<String>, _>("status"),
            "score": row.get::<Option<i32>, _>("score"),
            "received_ts": row.get::<i64, _>("received_ts")
        }))),
        None => Err(ApiError::not_found("Report not found".to_string())),
    }
}

#[axum::debug_handler]
pub async fn delete_report(
    _admin: AdminUser,
    State(state): State<AppState>,
    Path(report_id): Path<i64>,
) -> Result<Json<Value>, ApiError> {
    let result = sqlx::query("DELETE FROM event_reports WHERE id = $1")
        .bind(report_id)
        .execute(&*state.services.rooms.event_storage.pool)
        .await
        .map_err(|e| ApiError::internal_with_log("Database error", &e))?;

    if result.rows_affected() == 0 {
        return Err(ApiError::not_found("Report not found".to_string()));
    }

    Ok(Json(json!({})))
}

#[axum::debug_handler]
pub async fn get_room_reports(
    _admin: AdminUser,
    State(state): State<AppState>,
    Path(room_id): Path<String>,
    axum::extract::Query(params): axum::extract::Query<std::collections::HashMap<String, String>>,
) -> Result<Json<Value>, ApiError> {
    let room_exists = state
        .services.rooms.room_storage
        .room_exists(&room_id)
        .await
        .map_err(|e| ApiError::internal_with_log("Database error", &e))?;

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

    let reports = if let (Some(ts), Some(id)) = (since_ts, since_id) {
        sqlx::query(
            "SELECT id, room_id, event_id, reporter_user_id, reported_user_id, reason, description, status, score, received_ts FROM event_reports WHERE room_id = $1 AND (received_ts < $2 OR (received_ts = $2 AND id < $3)) ORDER BY received_ts DESC, id DESC LIMIT $4"
        )
        .bind(&room_id)
        .bind(ts)
        .bind(id)
        .bind(limit)
        .fetch_all(&*state.services.rooms.event_storage.pool)
        .await
    } else {
        sqlx::query(
            "SELECT id, room_id, event_id, reporter_user_id, reported_user_id, reason, description, status, score, received_ts FROM event_reports WHERE room_id = $1 ORDER BY received_ts DESC, id DESC LIMIT $2"
        )
        .bind(&room_id)
        .bind(limit)
        .fetch_all(&*state.services.rooms.event_storage.pool)
        .await
    }
    .map_err(|e| ApiError::internal_with_log("Database error", &e))?;

    let report_list: Vec<Value> = reports
        .iter()
        .map(|row| {
            json!({
                "id": row.get::<i64, _>("id"),
                "room_id": row.get::<String, _>("room_id"),
                "event_id": row.get::<String, _>("event_id"),
                "user_id": row.get::<String, _>("reporter_user_id"),
                "reported_user_id": row.get::<Option<String>, _>("reported_user_id"),
                "reason": row.get::<Option<String>, _>("reason"),
                "content": row.get::<Option<String>, _>("description"),
                "status": row.get::<Option<String>, _>("status"),
                "score": row.get::<Option<i32>, _>("score"),
                "received_ts": row.get::<i64, _>("received_ts")
            })
        })
        .collect();

    Ok(Json(json!({ "reports": report_list, "total": report_list.len() })))
}

#[axum::debug_handler]
pub async fn get_room_report(
    _admin: AdminUser,
    State(state): State<AppState>,
    Path((room_id, report_id)): Path<(String, i64)>,
) -> Result<Json<Value>, ApiError> {
    let room_exists = state
        .services.rooms.room_storage
        .room_exists(&room_id)
        .await
        .map_err(|e| ApiError::internal_with_log("Database error", &e))?;

    if !room_exists {
        return Err(ApiError::not_found("Room not found".to_string()));
    }

    let report = sqlx::query(
        "SELECT id, room_id, event_id, reporter_user_id, reported_user_id, reason, description, status, score, received_ts FROM event_reports WHERE id = $1 AND room_id = $2"
    )
    .bind(report_id)
    .bind(&room_id)
    .fetch_optional(&*state.services.rooms.event_storage.pool)
    .await
    .map_err(|e| ApiError::internal_with_log("Database error", &e))?;

    match report {
        Some(row) => Ok(Json(json!({
            "id": row.get::<i64, _>("id"),
            "room_id": row.get::<String, _>("room_id"),
            "event_id": row.get::<String, _>("event_id"),
            "user_id": row.get::<String, _>("reporter_user_id"),
            "reported_user_id": row.get::<Option<String>, _>("reported_user_id"),
            "reason": row.get::<Option<String>, _>("reason"),
            "content": row.get::<Option<String>, _>("description"),
            "status": row.get::<Option<String>, _>("status"),
            "score": row.get::<Option<i32>, _>("score"),
            "received_ts": row.get::<i64, _>("received_ts")
        }))),
        None => Err(ApiError::not_found("Report not found".to_string())),
    }
}
