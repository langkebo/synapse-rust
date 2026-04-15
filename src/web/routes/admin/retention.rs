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
    pub expire_on_clients: Option<bool>,
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
        "INSERT INTO server_retention_policy (id, max_lifetime, min_lifetime, expire_on_clients, created_ts, updated_ts) VALUES (1, $1, $2, $3, EXTRACT(EPOCH FROM NOW())::BIGINT * 1000, EXTRACT(EPOCH FROM NOW())::BIGINT * 1000) ON CONFLICT (id) DO UPDATE SET max_lifetime = $1, min_lifetime = $2, expire_on_clients = $3, updated_ts = EXTRACT(EPOCH FROM NOW())::BIGINT * 1000"
    )
    .bind(body.max_lifetime)
    .bind(body.min_lifetime)
    .bind(body.expire_on_clients.unwrap_or(false))
    .execute(&*state.services.room_storage.pool)
    .await
    .map_err(|e| ApiError::internal(format!("Database error: {}", e)))?;

    Ok(Json(json!({
        "max_lifetime": body.max_lifetime,
        "min_lifetime": body.min_lifetime,
        "expire_on_clients": body.expire_on_clients.unwrap_or(false)
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
    if !state
        .services
        .room_storage
        .room_exists(&room_id)
        .await
        .map_err(|e| ApiError::internal(format!("Database error: {}", e)))?
    {
        return Err(ApiError::not_found("Room not found".to_string()));
    }

    sqlx::query(
        "INSERT INTO room_retention_policies (room_id, max_lifetime, min_lifetime, expire_on_clients, is_server_default, created_ts, updated_ts) VALUES ($1, $2, $3, $4, FALSE, EXTRACT(EPOCH FROM NOW())::BIGINT * 1000, EXTRACT(EPOCH FROM NOW())::BIGINT * 1000) ON CONFLICT (room_id) DO UPDATE SET max_lifetime = $2, min_lifetime = $3, expire_on_clients = $4, updated_ts = EXTRACT(EPOCH FROM NOW())::BIGINT * 1000"
    )
    .bind(&room_id)
    .bind(body.max_lifetime)
    .bind(body.min_lifetime)
    .bind(body.expire_on_clients.unwrap_or(false))
    .execute(&*state.services.room_storage.pool)
    .await
    .map_err(|e| ApiError::internal(format!("Database error: {}", e)))?;

    Ok(Json(json!({
        "room_id": room_id,
        "max_lifetime": body.max_lifetime,
        "min_lifetime": body.min_lifetime,
        "expire_on_clients": body.expire_on_clients.unwrap_or(false)
    })))
}

#[axum::debug_handler]
pub async fn run_retention(
    _admin: AdminUser,
    State(state): State<AppState>,
    Json(body): Json<RunRetentionRequest>,
) -> Result<Json<Value>, ApiError> {
    match body.room_id {
        Some(room_id) => {
            let log = state
                .services
                .retention_service
                .run_cleanup(&room_id)
                .await?;
            Ok(Json(json!({
                "started": true,
                "room_id": room_id,
                "events_deleted": log.events_deleted,
                "status": log.status,
                "completed_ts": log.completed_ts
            })))
        }
        None => {
            let cleaned = state
                .services
                .retention_service
                .run_scheduled_cleanups()
                .await?;
            Ok(Json(json!({
                "started": true,
                "scope": "all_rooms",
                "events_deleted": cleaned
            })))
        }
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

    let last_run = state
        .services
        .retention_service
        .get_last_lifecycle_summary()
        .await
        .map(|summary| {
            json!({
                "started_ts": summary.started_ts,
                "completed_ts": summary.completed_ts,
                "duration_ms": summary.duration_ms,
                "expired_events_deleted": summary.expired_events_deleted,
                "expired_beacons_deleted": summary.expired_beacons_deleted,
                "expired_uploads_deleted": summary.expired_uploads_deleted,
                "expired_audit_events_deleted": summary.expired_audit_events_deleted,
                "cleanup_queue_items_processed": summary.cleanup_queue_items_processed,
                "cleanup_queue_rows_pruned": summary.cleanup_queue_rows_pruned,
                "failed_tasks": summary.failed_tasks
            })
        });

    Ok(Json(json!({
        "server_policy_enabled": server_policy_exists,
        "rooms_with_custom_policy": rooms_with_policy,
        "lifecycle_cleanup_enabled": state.services.config.retention.lifecycle_cleanup_enabled,
        "cleanup_batch_size": state.services.config.retention.cleanup_batch_size,
        "audit_retention_days": state.services.config.retention.audit_retention_days,
        "queue_retention_days": state.services.config.retention.queue_retention_days,
        "last_run": last_run
    })))
}
