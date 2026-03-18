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
    let offset = params
        .get("offset")
        .and_then(|v| v.parse().ok())
        .unwrap_or(0);

    let reports = sqlx::query(
        "SELECT id, room_id, event_id, user_id, reason, content, received_ts FROM event_reports ORDER BY received_ts DESC LIMIT $1 OFFSET $2"
    )
    .bind(limit)
    .bind(offset)
    .fetch_all(&*state.services.event_storage.pool)
    .await
    .map_err(|e| ApiError::internal(format!("Database error: {}", e)))?;

    let report_list: Vec<Value> = reports
        .iter()
        .map(|row| {
            json!({
                "id": row.get::<Option<i64>, _>("id"),
                "room_id": row.get::<Option<String>, _>("room_id"),
                "event_id": row.get::<Option<String>, _>("event_id"),
                "user_id": row.get::<Option<String>, _>("user_id"),
                "reason": row.get::<Option<String>, _>("reason"),
                "content": row.get::<Option<String>, _>("content"),
                "received_ts": row.get::<Option<i64>, _>("received_ts")
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
        "SELECT id, room_id, event_id, user_id, reason, content, received_ts FROM event_reports WHERE id = $1"
    )
    .bind(report_id)
    .fetch_optional(&*state.services.event_storage.pool)
    .await
    .map_err(|e| ApiError::internal(format!("Database error: {}", e)))?;

    match report {
        Some(row) => Ok(Json(json!({
            "id": row.get::<Option<i64>, _>("id"),
            "room_id": row.get::<Option<String>, _>("room_id"),
            "event_id": row.get::<Option<String>, _>("event_id"),
            "user_id": row.get::<Option<String>, _>("user_id"),
            "reason": row.get::<Option<String>, _>("reason"),
            "content": row.get::<Option<String>, _>("content"),
            "received_ts": row.get::<Option<i64>, _>("received_ts")
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
        .execute(&*state.services.event_storage.pool)
        .await
        .map_err(|e| ApiError::internal(format!("Database error: {}", e)))?;

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
) -> Result<Json<Value>, ApiError> {
    let reports = sqlx::query(
        "SELECT id, room_id, event_id, user_id, reason, content, received_ts FROM event_reports WHERE room_id = $1 ORDER BY received_ts DESC"
    )
    .bind(&room_id)
    .fetch_all(&*state.services.event_storage.pool)
    .await
    .map_err(|e| ApiError::internal(format!("Database error: {}", e)))?;

    let report_list: Vec<Value> = reports
        .iter()
        .map(|row| {
            json!({
                "id": row.get::<Option<i64>, _>("id"),
                "room_id": row.get::<Option<String>, _>("room_id"),
                "event_id": row.get::<Option<String>, _>("event_id"),
                "user_id": row.get::<Option<String>, _>("user_id"),
                "reason": row.get::<Option<String>, _>("reason"),
                "content": row.get::<Option<String>, _>("content"),
                "received_ts": row.get::<Option<i64>, _>("received_ts")
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
    let report = sqlx::query(
        "SELECT id, room_id, event_id, user_id, reason, content, received_ts FROM event_reports WHERE id = $1 AND room_id = $2"
    )
    .bind(report_id)
    .bind(&room_id)
    .fetch_optional(&*state.services.event_storage.pool)
    .await
    .map_err(|e| ApiError::internal(format!("Database error: {}", e)))?;

    match report {
        Some(row) => Ok(Json(json!({
            "id": row.get::<Option<i64>, _>("id"),
            "room_id": row.get::<Option<String>, _>("room_id"),
            "event_id": row.get::<Option<String>, _>("event_id"),
            "user_id": row.get::<Option<String>, _>("user_id"),
            "reason": row.get::<Option<String>, _>("reason"),
            "content": row.get::<Option<String>, _>("content"),
            "received_ts": row.get::<Option<i64>, _>("received_ts")
        }))),
        None => Err(ApiError::not_found("Report not found".to_string())),
    }
}
