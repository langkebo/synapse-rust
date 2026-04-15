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
        "SELECT token, max_uses, uses_count, expires_at, created_ts FROM registration_tokens ORDER BY created_ts DESC"
    )
    .fetch_all(&*state.services.user_storage.pool)
    .await
    .map_err(|e| ApiError::internal(format!("Database error: {}", e)))?;

    let token_list: Vec<Value> = tokens
        .iter()
        .map(|row| {
            let max_uses = row.get::<i32, _>("max_uses");
            let uses_allowed = if max_uses == 0 { None } else { Some(max_uses) };
            json!({
                "token": row.get::<Option<String>, _>("token"),
                "uses_allowed": uses_allowed,
                "pending": 0,
                "completed": row.get::<i32, _>("uses_count"),
                "expiry_time": row.get::<Option<i64>, _>("expires_at"),
                "created_ts": row.get::<Option<i64>, _>("created_ts")
            })
        })
        .collect();

    Ok(Json(json!({ "registration_tokens": token_list })))
}

#[axum::debug_handler]
pub async fn create_registration_token(
    admin: AdminUser,
    State(state): State<AppState>,
    Json(body): Json<CreateTokenRequest>,
) -> Result<Json<Value>, ApiError> {
    let now = chrono::Utc::now().timestamp_millis();
    let token = body
        .token
        .unwrap_or_else(|| crate::common::random_string(body.length.unwrap_or(16)));
    let max_uses = body.uses_allowed.unwrap_or(0);

    sqlx::query(
        "INSERT INTO registration_tokens (token, max_uses, uses_count, is_used, is_enabled, created_ts, updated_ts, expires_at, created_by) VALUES ($1, $2, 0, FALSE, TRUE, $3, $3, $4, $5)"
    )
    .bind(&token)
    .bind(max_uses)
    .bind(now)
    .bind(body.expiry_time)
    .bind(admin.user_id)
    .execute(&*state.services.user_storage.pool)
    .await
    .map_err(|e| ApiError::internal(format!("Database error: {}", e)))?;

    Ok(Json(json!({
        "token": token,
        "uses_allowed": if max_uses == 0 { None } else { Some(max_uses) },
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
        "SELECT token, max_uses, uses_count, expires_at, created_ts FROM registration_tokens WHERE token = $1"
    )
    .bind(&token)
    .fetch_optional(&*state.services.user_storage.pool)
    .await
    .map_err(|e| ApiError::internal(format!("Database error: {}", e)))?;

    match result {
        Some(row) => {
            let max_uses = row.get::<i32, _>("max_uses");
            Ok(Json(json!({
                "token": row.get::<Option<String>, _>("token"),
                "uses_allowed": if max_uses == 0 { None } else { Some(max_uses) },
                "pending": 0,
                "completed": row.get::<i32, _>("uses_count"),
                "expiry_time": row.get::<Option<i64>, _>("expires_at"),
                "created_ts": row.get::<Option<i64>, _>("created_ts")
            })))
        }
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
        "UPDATE registration_tokens SET max_uses = COALESCE($2, max_uses), expires_at = COALESCE($3, expires_at), updated_ts = $4 WHERE token = $1 RETURNING token, max_uses, uses_count, expires_at, created_ts"
    )
    .bind(&token)
    .bind(body.uses_allowed)
    .bind(body.expiry_time)
    .bind(chrono::Utc::now().timestamp_millis())
    .fetch_optional(&*state.services.user_storage.pool)
    .await
    .map_err(|e| ApiError::internal(format!("Database error: {}", e)))?;

    match result {
        Some(row) => {
            let max_uses = row.get::<i32, _>("max_uses");
            Ok(Json(json!({
                "token": row.get::<Option<String>, _>("token"),
                "uses_allowed": if max_uses == 0 { None } else { Some(max_uses) },
                "pending": 0,
                "completed": row.get::<i32, _>("uses_count"),
                "expiry_time": row.get::<Option<i64>, _>("expires_at"),
                "created_ts": row.get::<Option<i64>, _>("created_ts")
            })))
        }
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
        "SELECT id, device_id, created_ts, expires_at, is_revoked FROM access_tokens WHERE user_id = $1 ORDER BY created_ts DESC"
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
        "SELECT id, device_id, created_ts, expires_at, is_revoked FROM refresh_tokens WHERE user_id = $1 ORDER BY created_ts DESC"
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
