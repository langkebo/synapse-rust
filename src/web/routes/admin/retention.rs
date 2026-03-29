use crate::common::ApiError;
use crate::web::routes::{AdminUser, AppState};
use axum::{
    extract::{Path, State},
    routing::{get, post},
    Json, Router,
};
use serde::Deserialize;
use serde_json::{json, Value};
use sqlx::Row;

pub fn create_retention_router(_state: AppState) -> Router<AppState> {
    Router::new()
        .route(
            "/_synapse/admin/v1/retention/policy",
            get(get_retention_policy),
        )
        .route(
            "/_synapse/admin/v1/retention/policy",
            post(set_retention_policy),
        )
        .route(
            "/_synapse/admin/v1/retention/policy/{room_id}",
            get(get_room_retention_policy),
        )
        .route(
            "/_synapse/admin/v1/retention/policy/{room_id}",
            post(set_room_retention_policy),
        )
        .route("/_synapse/admin/v1/retention/run", post(run_retention))
        .route(
            "/_synapse/admin/v1/retention/status",
            get(get_retention_status),
        )
}

#[derive(Debug, Deserialize)]
pub struct RetentionPolicyRequest {
    pub max_lifetime: Option<i64>,
    pub min_lifetime: Option<i64>,
}

#[derive(Debug, Deserialize)]
pub struct RunRetentionRequest {
    pub room_id: Option<String>,
}

#[axum::debug_handler]
pub async fn get_retention_policy(
    _admin: AdminUser,
    State(state): State<AppState>,
) -> Result<Json<Value>, ApiError> {
    let policy = sqlx::query(
        "SELECT max_lifetime, min_lifetime, expire_on_clients FROM server_retention_policy LIMIT 1",
    )
    .fetch_optional(&*state.services.room_storage.pool)
    .await
    .map_err(|e| ApiError::internal(format!("Database error: {}", e)))?;

    match policy {
        Some(row) => Ok(Json(json!({
            "max_lifetime": row.get::<Option<i64>, _>("max_lifetime"),
            "min_lifetime": row.get::<Option<i64>, _>("min_lifetime"),
            "expire_on_clients": row.get::<bool, _>("expire_on_clients")
        }))),
        None => Ok(Json(json!({
            "max_lifetime": null,
            "min_lifetime": null,
            "expire_on_clients": false
        }))),
    }
}

#[axum::debug_handler]
pub async fn set_retention_policy(
    _admin: AdminUser,
    State(state): State<AppState>,
    Json(body): Json<RetentionPolicyRequest>,
) -> Result<Json<Value>, ApiError> {
    sqlx::query(
        "INSERT INTO server_retention_policy (id, max_lifetime, min_lifetime, expire_on_clients, created_ts, updated_ts) VALUES (1, $1, $2, FALSE, EXTRACT(EPOCH FROM NOW())::BIGINT * 1000, EXTRACT(EPOCH FROM NOW())::BIGINT * 1000) ON CONFLICT (id) DO UPDATE SET max_lifetime = $1, min_lifetime = $2, updated_ts = EXTRACT(EPOCH FROM NOW())::BIGINT * 1000"
    )
    .bind(body.max_lifetime)
    .bind(body.min_lifetime)
    .execute(&*state.services.room_storage.pool)
    .await
    .map_err(|e| ApiError::internal(format!("Database error: {}", e)))?;

    Ok(Json(json!({
        "max_lifetime": body.max_lifetime,
        "min_lifetime": body.min_lifetime
    })))
}

#[axum::debug_handler]
pub async fn get_room_retention_policy(
    _admin: AdminUser,
    State(state): State<AppState>,
    Path(room_id): Path<String>,
) -> Result<Json<Value>, ApiError> {
    let policy = sqlx::query(
        "SELECT max_lifetime, min_lifetime, expire_on_clients FROM room_retention_policies WHERE room_id = $1",
    )
    .bind(&room_id)
    .fetch_optional(&*state.services.room_storage.pool)
    .await
    .map_err(|e| ApiError::internal(format!("Database error: {}", e)))?;

    match policy {
        Some(row) => Ok(Json(json!({
            "room_id": room_id,
            "max_lifetime": row.get::<Option<i64>, _>("max_lifetime"),
            "min_lifetime": row.get::<Option<i64>, _>("min_lifetime"),
            "expire_on_clients": row.get::<bool, _>("expire_on_clients")
        }))),
        None => Ok(Json(json!({
            "room_id": room_id,
            "max_lifetime": null,
            "min_lifetime": null,
            "expire_on_clients": false
        }))),
    }
}

#[axum::debug_handler]
pub async fn set_room_retention_policy(
    _admin: AdminUser,
    State(state): State<AppState>,
    Path(room_id): Path<String>,
    Json(body): Json<RetentionPolicyRequest>,
) -> Result<Json<Value>, ApiError> {
    sqlx::query(
        "INSERT INTO room_retention_policies (room_id, max_lifetime, min_lifetime, expire_on_clients, is_server_default, created_ts, updated_ts) VALUES ($1, $2, $3, FALSE, FALSE, EXTRACT(EPOCH FROM NOW())::BIGINT * 1000, EXTRACT(EPOCH FROM NOW())::BIGINT * 1000) ON CONFLICT (room_id) DO UPDATE SET max_lifetime = $2, min_lifetime = $3, updated_ts = EXTRACT(EPOCH FROM NOW())::BIGINT * 1000"
    )
    .bind(&room_id)
    .bind(body.max_lifetime)
    .bind(body.min_lifetime)
    .execute(&*state.services.room_storage.pool)
    .await
    .map_err(|e| ApiError::internal(format!("Database error: {}", e)))?;

    Ok(Json(json!({
        "room_id": room_id,
        "max_lifetime": body.max_lifetime,
        "min_lifetime": body.min_lifetime
    })))
}

#[axum::debug_handler]
pub async fn run_retention(
    _admin: AdminUser,
    State(_state): State<AppState>,
    Json(body): Json<RunRetentionRequest>,
) -> Result<Json<Value>, ApiError> {
    match body.room_id {
        Some(room_id) => Ok(Json(json!({
            "started": true,
            "room_id": room_id
        }))),
        None => Ok(Json(json!({
            "started": true,
            "scope": "all_rooms"
        }))),
    }
}

#[axum::debug_handler]
pub async fn get_retention_status(
    _admin: AdminUser,
    State(state): State<AppState>,
) -> Result<Json<Value>, ApiError> {
    let rooms_with_policy: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM room_retention_policies")
        .fetch_one(&*state.services.room_storage.pool)
        .await
        .map_err(|e| ApiError::internal(format!("Database error: {}", e)))?;

    let server_policy_exists: bool =
        sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM server_retention_policy)")
            .fetch_one(&*state.services.room_storage.pool)
            .await
            .map_err(|e| ApiError::internal(format!("Database error: {}", e)))?;

    Ok(Json(json!({
        "server_policy_enabled": server_policy_exists,
        "rooms_with_custom_policy": rooms_with_policy,
        "last_run": null
    })))
}
