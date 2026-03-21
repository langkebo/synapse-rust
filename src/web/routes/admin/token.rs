use crate::common::ApiError;
use crate::web::routes::{AdminUser, AppState};
use axum::{
    extract::{Path, State},
    routing::{delete, get, post},
    Json, Router,
};
use serde::Deserialize;
use serde_json::{json, Value};
use sqlx::Row;

pub fn create_token_router(_state: AppState) -> Router<AppState> {
    Router::new()
        .route(
            "/_synapse/admin/v1/registration_tokens",
            get(get_registration_tokens),
        )
        .route(
            "/_synapse/admin/v1/registration_tokens",
            post(create_registration_token),
        )
        .route(
            "/_synapse/admin/v1/registration_tokens/{token}",
            get(get_registration_token),
        )
        .route(
            "/_synapse/admin/v1/registration_tokens/{token}",
            delete(delete_registration_token),
        )
        .route(
            "/_synapse/admin/v1/registration_tokens/{token}",
            post(update_registration_token),
        )
        .route(
            "/_synapse/admin/v1/users/{user_id}/tokens",
            get(get_user_tokens),
        )
        .route(
            "/_synapse/admin/v1/users/{user_id}/tokens/{token_id}",
            delete(delete_user_token),
        )
        .route(
            "/_synapse/admin/v1/users/{user_id}/refresh_tokens",
            get(get_user_refresh_tokens),
        )
        .route(
            "/_synapse/admin/v1/users/{user_id}/refresh_tokens/{token_id}",
            delete(delete_refresh_token),
        )
}

#[derive(Debug, Deserialize)]
pub struct CreateTokenRequest {
    pub token: Option<String>,
    pub uses_allowed: Option<i32>,
    pub expiry_time: Option<i64>,
    pub length: Option<usize>,
}

#[derive(Debug, Deserialize)]
pub struct UpdateTokenRequest {
    pub uses_allowed: Option<i32>,
    pub expiry_time: Option<i64>,
}

#[axum::debug_handler]
pub async fn get_registration_tokens(
    _admin: AdminUser,
    State(state): State<AppState>,
) -> Result<Json<Value>, ApiError> {
    let tokens = sqlx::query(
        "SELECT token, uses_allowed, pending, completed, expiry_time, created_ts FROM registration_tokens ORDER BY created_ts DESC"
    )
    .fetch_all(&*state.services.user_storage.pool)
    .await
    .map_err(|e| ApiError::internal(format!("Database error: {}", e)))?;

    let token_list: Vec<Value> = tokens
        .iter()
        .map(|row| {
            json!({
                "token": row.get::<Option<String>, _>("token"),
                "uses_allowed": row.get::<Option<i32>, _>("uses_allowed"),
                "pending": row.get::<Option<i32>, _>("pending").unwrap_or(0),
                "completed": row.get::<Option<i32>, _>("completed").unwrap_or(0),
                "expiry_time": row.get::<Option<i64>, _>("expiry_time"),
                "created_ts": row.get::<Option<i64>, _>("created_ts")
            })
        })
        .collect();

    Ok(Json(json!({ "registration_tokens": token_list })))
}

#[axum::debug_handler]
pub async fn create_registration_token(
    _admin: AdminUser,
    State(state): State<AppState>,
    Json(body): Json<CreateTokenRequest>,
) -> Result<Json<Value>, ApiError> {
    let now = chrono::Utc::now().timestamp_millis();
    let token = body
        .token
        .unwrap_or_else(|| crate::common::random_string(body.length.unwrap_or(16)));

    sqlx::query(
        "INSERT INTO registration_tokens (token, uses_allowed, pending, completed, expiry_time, created_ts) VALUES ($1, $2, 0, 0, $3, $4)"
    )
    .bind(&token)
    .bind(body.uses_allowed)
    .bind(body.expiry_time)
    .bind(now)
    .execute(&*state.services.user_storage.pool)
    .await
    .map_err(|e| ApiError::internal(format!("Database error: {}", e)))?;

    Ok(Json(json!({
        "token": token,
        "uses_allowed": body.uses_allowed,
        "pending": 0,
        "completed": 0,
        "expiry_time": body.expiry_time,
        "created_ts": now
    })))
}

#[axum::debug_handler]
pub async fn get_registration_token(
    _admin: AdminUser,
    State(state): State<AppState>,
    Path(token): Path<String>,
) -> Result<Json<Value>, ApiError> {
    let result = sqlx::query(
        "SELECT token, uses_allowed, pending, completed, expiry_time, created_ts FROM registration_tokens WHERE token = $1"
    )
    .bind(&token)
    .fetch_optional(&*state.services.user_storage.pool)
    .await
    .map_err(|e| ApiError::internal(format!("Database error: {}", e)))?;

    match result {
        Some(row) => Ok(Json(json!({
            "token": row.get::<Option<String>, _>("token"),
            "uses_allowed": row.get::<Option<i32>, _>("uses_allowed"),
            "pending": row.get::<Option<i32>, _>("pending").unwrap_or(0),
            "completed": row.get::<Option<i32>, _>("completed").unwrap_or(0),
            "expiry_time": row.get::<Option<i64>, _>("expiry_time"),
            "created_ts": row.get::<Option<i64>, _>("created_ts")
        }))),
        None => Err(ApiError::not_found("Token not found".to_string())),
    }
}

#[axum::debug_handler]
pub async fn delete_registration_token(
    _admin: AdminUser,
    State(state): State<AppState>,
    Path(token): Path<String>,
) -> Result<Json<Value>, ApiError> {
    let result = sqlx::query("DELETE FROM registration_tokens WHERE token = $1")
        .bind(&token)
        .execute(&*state.services.user_storage.pool)
        .await
        .map_err(|e| ApiError::internal(format!("Database error: {}", e)))?;

    if result.rows_affected() == 0 {
        return Err(ApiError::not_found("Token not found".to_string()));
    }

    Ok(Json(json!({})))
}

#[axum::debug_handler]
pub async fn update_registration_token(
    _admin: AdminUser,
    State(state): State<AppState>,
    Path(token): Path<String>,
    Json(body): Json<UpdateTokenRequest>,
) -> Result<Json<Value>, ApiError> {
    let result = sqlx::query(
        "UPDATE registration_tokens SET uses_allowed = COALESCE($2, uses_allowed), expiry_time = COALESCE($3, expiry_time) WHERE token = $1 RETURNING token, uses_allowed, pending, completed, expiry_time, created_ts"
    )
    .bind(&token)
    .bind(body.uses_allowed)
    .bind(body.expiry_time)
    .fetch_optional(&*state.services.user_storage.pool)
    .await
    .map_err(|e| ApiError::internal(format!("Database error: {}", e)))?;

    match result {
        Some(row) => Ok(Json(json!({
            "token": row.get::<Option<String>, _>("token"),
            "uses_allowed": row.get::<Option<i32>, _>("uses_allowed"),
            "pending": row.get::<Option<i32>, _>("pending").unwrap_or(0),
            "completed": row.get::<Option<i32>, _>("completed").unwrap_or(0),
            "expiry_time": row.get::<Option<i64>, _>("expiry_time"),
            "created_ts": row.get::<Option<i64>, _>("created_ts")
        }))),
        None => Err(ApiError::not_found("Token not found".to_string())),
    }
}

#[axum::debug_handler]
pub async fn get_user_tokens(
    _admin: AdminUser,
    State(state): State<AppState>,
    Path(user_id): Path<String>,
) -> Result<Json<Value>, ApiError> {
    let tokens = sqlx::query(
        "SELECT id, token_hash, device_id, created_ts, expires_at, is_revoked FROM access_tokens WHERE user_id = $1 ORDER BY created_ts DESC"
    )
    .bind(&user_id)
    .fetch_all(&*state.services.token_storage.pool)
    .await
    .map_err(|e| ApiError::internal(format!("Database error: {}", e)))?;

    let token_list: Vec<Value> = tokens
        .iter()
        .map(|row| {
            json!({
                "id": row.get::<Option<i64>, _>("id"),
                "device_id": row.get::<Option<String>, _>("device_id"),
                "created_ts": row.get::<Option<i64>, _>("created_ts"),
                "expires_at": row.get::<Option<i64>, _>("expires_at"),
                "is_revoked": row.get::<Option<bool>, _>("is_revoked").unwrap_or(false)
            })
        })
        .collect();

    Ok(Json(
        json!({ "tokens": token_list, "total": token_list.len() }),
    ))
}

#[axum::debug_handler]
pub async fn delete_user_token(
    _admin: AdminUser,
    State(state): State<AppState>,
    Path((user_id, token_id)): Path<(String, i64)>,
) -> Result<Json<Value>, ApiError> {
    let result = sqlx::query("DELETE FROM access_tokens WHERE id = $1 AND user_id = $2")
        .bind(token_id)
        .bind(&user_id)
        .execute(&*state.services.token_storage.pool)
        .await
        .map_err(|e| ApiError::internal(format!("Database error: {}", e)))?;

    if result.rows_affected() == 0 {
        return Err(ApiError::not_found("Token not found".to_string()));
    }

    Ok(Json(json!({})))
}

#[axum::debug_handler]
pub async fn get_user_refresh_tokens(
    _admin: AdminUser,
    State(state): State<AppState>,
    Path(user_id): Path<String>,
) -> Result<Json<Value>, ApiError> {
    let tokens = sqlx::query(
        "SELECT id, token_hash, device_id, created_ts, expires_at, is_revoked FROM refresh_tokens WHERE user_id = $1 ORDER BY created_ts DESC"
    )
    .bind(&user_id)
    .fetch_all(&*state.services.token_storage.pool)
    .await
    .map_err(|e| ApiError::internal(format!("Database error: {}", e)))?;

    let token_list: Vec<Value> = tokens
        .iter()
        .map(|row| {
            json!({
                "id": row.get::<Option<i64>, _>("id"),
                "device_id": row.get::<Option<String>, _>("device_id"),
                "created_ts": row.get::<Option<i64>, _>("created_ts"),
                "expires_at": row.get::<Option<i64>, _>("expires_at"),
                "is_revoked": row.get::<Option<bool>, _>("is_revoked").unwrap_or(false)
            })
        })
        .collect();

    Ok(Json(
        json!({ "refresh_tokens": token_list, "total": token_list.len() }),
    ))
}

#[axum::debug_handler]
pub async fn delete_refresh_token(
    _admin: AdminUser,
    State(state): State<AppState>,
    Path((user_id, token_id)): Path<(String, i64)>,
) -> Result<Json<Value>, ApiError> {
    let result = sqlx::query("DELETE FROM refresh_tokens WHERE id = $1 AND user_id = $2")
        .bind(token_id)
        .bind(&user_id)
        .execute(&*state.services.token_storage.pool)
        .await
        .map_err(|e| ApiError::internal(format!("Database error: {}", e)))?;

    if result.rows_affected() == 0 {
        return Err(ApiError::not_found("Refresh token not found".to_string()));
    }

    Ok(Json(json!({})))
}
