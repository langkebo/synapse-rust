use crate::common::ApiError;
use crate::web::routes::{AdminUser, AppState};
use axum::{
    extract::State,
    routing::{get, post},
    Json, Router,
};
use serde::Deserialize;
use serde_json::{json, Value};
use sqlx::Row;

pub fn create_background_router(_state: AppState) -> Router<AppState> {
    Router::new()
        .route("/_synapse/admin/v1/background_updates", get(get_background_updates))
        .route("/_synapse/admin/v1/background_updates/start", post(start_background_update))
        .route("/_synapse/admin/v1/background_updates/stop", post(stop_background_update))
        .route("/_synapse/admin/v1/background_updates/cancel", post(cancel_background_update))
}

#[derive(Debug, Deserialize)]
pub struct BackgroundUpdateRequest {
    pub job_name: Option<String>,
}

#[axum::debug_handler]
pub async fn get_background_updates(
    _admin: AdminUser,
    State(state): State<AppState>,
) -> Result<Json<Value>, ApiError> {
    let updates = sqlx::query(
        "SELECT job_name, progress, status, started_at, completed_at FROM background_updates ORDER BY started_at DESC"
    )
    .fetch_all(&*state.services.user_storage.pool)
    .await
    .map_err(|e| ApiError::internal(format!("Database error: {}", e)))?;

    let update_list: Vec<Value> = updates
        .iter()
        .map(|row| {
            json!({
                "job_name": row.get::<Option<String>, _>("job_name"),
                "progress": row.get::<Option<f64>, _>("progress"),
                "status": row.get::<Option<String>, _>("status"),
                "started_at": row.get::<Option<i64>, _>("started_at"),
                "completed_at": row.get::<Option<i64>, _>("completed_at")
            })
        })
        .collect();

    Ok(Json(json!({ "updates": update_list, "total": update_list.len() })))
}

#[axum::debug_handler]
pub async fn start_background_update(
    _admin: AdminUser,
    State(state): State<AppState>,
    Json(body): Json<BackgroundUpdateRequest>,
) -> Result<Json<Value>, ApiError> {
    let now = chrono::Utc::now().timestamp_millis();

    if let Some(job_name) = body.job_name {
        sqlx::query(
            "UPDATE background_updates SET status = 'running', started_at = $2 WHERE job_name = $1 AND status = 'pending'"
        )
        .bind(&job_name)
        .bind(now)
        .execute(&*state.services.user_storage.pool)
        .await
        .map_err(|e| ApiError::internal(format!("Database error: {}", e)))?;
    } else {
        sqlx::query(
            "UPDATE background_updates SET status = 'running', started_at = $1 WHERE status = 'pending'"
        )
        .bind(now)
        .execute(&*state.services.user_storage.pool)
        .await
        .map_err(|e| ApiError::internal(format!("Database error: {}", e)))?;
    }

    Ok(Json(json!({ "started": true })))
}

#[axum::debug_handler]
pub async fn stop_background_update(
    _admin: AdminUser,
    State(state): State<AppState>,
    Json(body): Json<BackgroundUpdateRequest>,
) -> Result<Json<Value>, ApiError> {
    if let Some(job_name) = body.job_name {
        sqlx::query(
            "UPDATE background_updates SET status = 'paused' WHERE job_name = $1 AND status = 'running'"
        )
        .bind(&job_name)
        .execute(&*state.services.user_storage.pool)
        .await
        .map_err(|e| ApiError::internal(format!("Database error: {}", e)))?;
    } else {
        sqlx::query(
            "UPDATE background_updates SET status = 'paused' WHERE status = 'running'"
        )
        .execute(&*state.services.user_storage.pool)
        .await
        .map_err(|e| ApiError::internal(format!("Database error: {}", e)))?;
    }

    Ok(Json(json!({ "stopped": true })))
}

#[axum::debug_handler]
pub async fn cancel_background_update(
    _admin: AdminUser,
    State(state): State<AppState>,
    Json(body): Json<BackgroundUpdateRequest>,
) -> Result<Json<Value>, ApiError> {
    if let Some(job_name) = body.job_name {
        sqlx::query("DELETE FROM background_updates WHERE job_name = $1")
            .bind(&job_name)
            .execute(&*state.services.user_storage.pool)
            .await
            .map_err(|e| ApiError::internal(format!("Database error: {}", e)))?;
    }

    Ok(Json(json!({ "cancelled": true })))
}
