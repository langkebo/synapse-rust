use crate::common::ApiError;
use crate::web::routes::{AdminUser, AppState};
use axum::{
    extract::{Path, State},
    routing::{delete, get},
    Json, Router,
};
use serde_json::{json, Value};
use sqlx::Row;

pub fn create_media_router(_state: AppState) -> Router<AppState> {
    Router::new()
        .route("/_synapse/admin/v1/media", get(get_all_media))
        .route("/_synapse/admin/v1/media/{media_id}", get(get_media_info))
        .route("/_synapse/admin/v1/media/{media_id}", delete(delete_media))
        .route("/_synapse/admin/v1/media/quota", get(get_media_quota))
        .route("/_synapse/admin/v1/users/{user_id}/media", get(get_user_media))
        .route("/_synapse/admin/v1/users/{user_id}/media", delete(delete_user_media))
}

#[axum::debug_handler]
pub async fn get_all_media(
    _admin: AdminUser,
    State(state): State<AppState>,
    axum::extract::Query(params): axum::extract::Query<std::collections::HashMap<String, String>>,
) -> Result<Json<Value>, ApiError> {
    let limit = params
        .get("limit")
        .and_then(|v| v.parse().ok())
        .unwrap_or(100);
    let offset = params
        .get("offset")
        .and_then(|v| v.parse().ok())
        .unwrap_or(0);

    let media = sqlx::query(
        "SELECT media_id, media_type, upload_name, created_ts, last_access_ts, media_length, user_id, quarantined FROM media ORDER BY created_ts DESC LIMIT $1 OFFSET $2"
    )
    .bind(limit)
    .bind(offset)
    .fetch_all(&*state.services.user_storage.pool)
    .await
    .map_err(|e| ApiError::internal(format!("Database error: {}", e)))?;

    let media_list: Vec<Value> = media
        .iter()
        .map(|row| {
            json!({
                "media_id": row.get::<Option<String>, _>("media_id"),
                "media_type": row.get::<Option<String>, _>("media_type"),
                "upload_name": row.get::<Option<String>, _>("upload_name"),
                "created_ts": row.get::<Option<i64>, _>("created_ts"),
                "last_access_ts": row.get::<Option<i64>, _>("last_access_ts"),
                "media_length": row.get::<Option<i64>, _>("media_length"),
                "user_id": row.get::<Option<String>, _>("user_id"),
                "quarantined": row.get::<Option<bool>, _>("quarantined").unwrap_or(false)
            })
        })
        .collect();

    Ok(Json(json!({ "media": media_list, "total": media_list.len() })))
}

#[axum::debug_handler]
pub async fn get_media_info(
    _admin: AdminUser,
    State(state): State<AppState>,
    Path(media_id): Path<String>,
) -> Result<Json<Value>, ApiError> {
    let media = sqlx::query(
        "SELECT media_id, media_type, upload_name, created_ts, last_access_ts, media_length, user_id, quarantined FROM media WHERE media_id = $1"
    )
    .bind(&media_id)
    .fetch_optional(&*state.services.user_storage.pool)
    .await
    .map_err(|e| ApiError::internal(format!("Database error: {}", e)))?;

    match media {
        Some(row) => Ok(Json(json!({
            "media_id": row.get::<Option<String>, _>("media_id"),
            "media_type": row.get::<Option<String>, _>("media_type"),
            "upload_name": row.get::<Option<String>, _>("upload_name"),
            "created_ts": row.get::<Option<i64>, _>("created_ts"),
            "last_access_ts": row.get::<Option<i64>, _>("last_access_ts"),
            "media_length": row.get::<Option<i64>, _>("media_length"),
            "user_id": row.get::<Option<String>, _>("user_id"),
            "quarantined": row.get::<Option<bool>, _>("quarantined").unwrap_or(false)
        }))),
        None => Err(ApiError::not_found("Media not found".to_string())),
    }
}

#[axum::debug_handler]
pub async fn delete_media(
    _admin: AdminUser,
    State(state): State<AppState>,
    Path(media_id): Path<String>,
) -> Result<Json<Value>, ApiError> {
    let result = sqlx::query("DELETE FROM media WHERE media_id = $1")
        .bind(&media_id)
        .execute(&*state.services.user_storage.pool)
        .await
        .map_err(|e| ApiError::internal(format!("Database error: {}", e)))?;

    if result.rows_affected() == 0 {
        return Err(ApiError::not_found("Media not found".to_string()));
    }

    Ok(Json(json!({})))
}

#[axum::debug_handler]
pub async fn get_media_quota(
    _admin: AdminUser,
    State(state): State<AppState>,
) -> Result<Json<Value>, ApiError> {
    let total_size: i64 = sqlx::query_scalar("SELECT COALESCE(SUM(media_length), 0) FROM media")
        .fetch_one(&*state.services.user_storage.pool)
        .await
        .map_err(|e| ApiError::internal(format!("Database error: {}", e)))?;

    let total_count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM media")
        .fetch_one(&*state.services.user_storage.pool)
        .await
        .map_err(|e| ApiError::internal(format!("Database error: {}", e)))?;

    Ok(Json(json!({
        "total_size": total_size,
        "total_count": total_count,
        "default_size_limit": 10000000000i64,
        "default_count_limit": 100
    })))
}

#[axum::debug_handler]
pub async fn get_user_media(
    _admin: AdminUser,
    State(state): State<AppState>,
    Path(user_id): Path<String>,
) -> Result<Json<Value>, ApiError> {
    let media = sqlx::query(
        "SELECT media_id, media_type, upload_name, created_ts, media_length FROM media WHERE user_id = $1 ORDER BY created_ts DESC"
    )
    .bind(&user_id)
    .fetch_all(&*state.services.user_storage.pool)
    .await
    .map_err(|e| ApiError::internal(format!("Database error: {}", e)))?;

    let media_list: Vec<Value> = media
        .iter()
        .map(|row| {
            json!({
                "media_id": row.get::<Option<String>, _>("media_id"),
                "media_type": row.get::<Option<String>, _>("media_type"),
                "upload_name": row.get::<Option<String>, _>("upload_name"),
                "created_ts": row.get::<Option<i64>, _>("created_ts"),
                "media_length": row.get::<Option<i64>, _>("media_length")
            })
        })
        .collect();

    Ok(Json(json!({ "media": media_list, "total": media_list.len() })))
}

#[axum::debug_handler]
pub async fn delete_user_media(
    _admin: AdminUser,
    State(state): State<AppState>,
    Path(user_id): Path<String>,
) -> Result<Json<Value>, ApiError> {
    let result = sqlx::query("DELETE FROM media WHERE user_id = $1")
        .bind(&user_id)
        .execute(&*state.services.user_storage.pool)
        .await
        .map_err(|e| ApiError::internal(format!("Database error: {}", e)))?;

    Ok(Json(json!({ "deleted": result.rows_affected() })))
}
