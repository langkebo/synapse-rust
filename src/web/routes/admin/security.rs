use crate::common::ApiError;
use crate::web::routes::{AdminUser, AppState};
use axum::{
    extract::{Path, State},
    routing::{delete, get, post, put},
    Json, Router,
};
use serde::Deserialize;
use serde_json::{json, Value};
use sqlx::Row;

pub fn create_security_router(_state: AppState) -> Router<AppState> {
    Router::new()
        .route(
            "/_synapse/admin/v1/users/{user_id}/shadow_ban",
            post(shadow_ban_user),
        )
        .route(
            "/_synapse/admin/v1/users/{user_id}/shadow_ban",
            delete(unshadow_ban_user),
        )
        .route(
            "/_synapse/admin/v1/users/{user_id}/rate_limit",
            get(get_user_rate_limit),
        )
        .route(
            "/_synapse/admin/v1/users/{user_id}/rate_limit",
            put(set_user_rate_limit),
        )
        .route(
            "/_synapse/admin/v1/users/{user_id}/rate_limit",
            delete(delete_user_rate_limit),
        )
        .route(
            "/_synapse/admin/v1/users/{user_id}/override_ratelimit",
            get(get_user_override_rate_limit),
        )
        .route(
            "/_synapse/admin/v1/users/{user_id}/override_ratelimit",
            post(set_user_override_rate_limit),
        )
        .route(
            "/_synapse/admin/v1/users/{user_id}/override_ratelimit",
            delete(delete_user_override_rate_limit),
        )
}

#[derive(Debug, Deserialize)]
pub struct RateLimitRequest {
    pub messages_per_second: Option<f64>,
    pub burst_count: Option<i32>,
}

#[axum::debug_handler]
pub async fn shadow_ban_user(
    _admin: AdminUser,
    State(state): State<AppState>,
    Path(user_id): Path<String>,
) -> Result<Json<Value>, ApiError> {
    sqlx::query("UPDATE users SET is_shadow_banned = true WHERE user_id = $1")
        .bind(&user_id)
        .execute(&*state.services.user_storage.pool)
        .await
        .map_err(|e| ApiError::internal(format!("Database error: {}", e)))?;

    Ok(Json(json!({})))
}

#[axum::debug_handler]
pub async fn unshadow_ban_user(
    _admin: AdminUser,
    State(state): State<AppState>,
    Path(user_id): Path<String>,
) -> Result<Json<Value>, ApiError> {
    sqlx::query("UPDATE users SET is_shadow_banned = false WHERE user_id = $1")
        .bind(&user_id)
        .execute(&*state.services.user_storage.pool)
        .await
        .map_err(|e| ApiError::internal(format!("Database error: {}", e)))?;

    Ok(Json(json!({})))
}

#[axum::debug_handler]
pub async fn get_user_rate_limit(
    _admin: AdminUser,
    State(state): State<AppState>,
    Path(user_id): Path<String>,
) -> Result<Json<Value>, ApiError> {
    let limit =
        sqlx::query("SELECT messages_per_second, burst_count FROM rate_limits WHERE user_id = $1")
            .bind(&user_id)
            .fetch_optional(&*state.services.user_storage.pool)
            .await
            .map_err(|e| ApiError::internal(format!("Database error: {}", e)))?;

    match limit {
        Some(row) => Ok(Json(json!({
            "messages_per_second": row.get::<Option<f64>, _>("messages_per_second").unwrap_or(5.0),
            "burst_count": row.get::<Option<i32>, _>("burst_count").unwrap_or(10)
        }))),
        None => Ok(Json(json!({
            "messages_per_second": 5.0,
            "burst_count": 10
        }))),
    }
}

#[axum::debug_handler]
pub async fn set_user_rate_limit(
    _admin: AdminUser,
    State(state): State<AppState>,
    Path(user_id): Path<String>,
    Json(body): Json<RateLimitRequest>,
) -> Result<Json<Value>, ApiError> {
    let messages_per_second = body.messages_per_second.unwrap_or(5.0);
    let burst_count = body.burst_count.unwrap_or(10);

    sqlx::query(
        "INSERT INTO rate_limits (user_id, messages_per_second, burst_count) VALUES ($1, $2, $3) ON CONFLICT (user_id) DO UPDATE SET messages_per_second = $2, burst_count = $3"
    )
    .bind(&user_id)
    .bind(messages_per_second)
    .bind(burst_count)
    .execute(&*state.services.user_storage.pool)
    .await
    .map_err(|e| ApiError::internal(format!("Database error: {}", e)))?;

    Ok(Json(json!({
        "messages_per_second": messages_per_second,
        "burst_count": burst_count
    })))
}

#[axum::debug_handler]
pub async fn delete_user_rate_limit(
    _admin: AdminUser,
    State(state): State<AppState>,
    Path(user_id): Path<String>,
) -> Result<Json<Value>, ApiError> {
    sqlx::query("DELETE FROM rate_limits WHERE user_id = $1")
        .bind(&user_id)
        .execute(&*state.services.user_storage.pool)
        .await
        .map_err(|e| ApiError::internal(format!("Database error: {}", e)))?;

    Ok(Json(json!({})))
}

#[axum::debug_handler]
pub async fn get_user_override_rate_limit(
    admin: AdminUser,
    State(state): State<AppState>,
    Path(user_id): Path<String>,
) -> Result<Json<Value>, ApiError> {
    get_user_rate_limit(admin, State(state), Path(user_id)).await
}

#[axum::debug_handler]
pub async fn set_user_override_rate_limit(
    admin: AdminUser,
    State(state): State<AppState>,
    Path(user_id): Path<String>,
    body: Json<RateLimitRequest>,
) -> Result<Json<Value>, ApiError> {
    set_user_rate_limit(admin, State(state), Path(user_id), body).await
}

#[axum::debug_handler]
pub async fn delete_user_override_rate_limit(
    admin: AdminUser,
    State(state): State<AppState>,
    Path(user_id): Path<String>,
) -> Result<Json<Value>, ApiError> {
    delete_user_rate_limit(admin, State(state), Path(user_id)).await
}
