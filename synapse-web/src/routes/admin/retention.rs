use crate::routes::context::AdminContext;
use crate::routes::extractors::RoomId;
use crate::routes::AdminUser;
use axum::{
    extract::{Path, State},
    routing::{get, post},
    Json, Router,
};
use serde::Deserialize;
use serde_json::{json, Value};
use synapse_common::ApiError;
use synapse_services::retention_service::{CreateRoomRetentionPolicyRequest, UpdateServerRetentionPolicyRequest};

/// See [`create_retention_router`].
pub fn create_retention_router() -> Router<crate::routes::AppState> {
    Router::new()
        .route("/_synapse/admin/v1/retention/policy", get(get_retention_policy))
        .route("/_synapse/admin/v1/retention/policy", post(set_retention_policy))
        .route("/_synapse/admin/v1/retention/policy/{room_id}", get(get_room_retention_policy))
        .route("/_synapse/admin/v1/retention/policy/{room_id}", post(set_room_retention_policy))
        .route("/_synapse/admin/v1/retention/run", post(run_retention))
        .route("/_synapse/admin/v1/retention/status", get(get_retention_status))
}

/// The `RetentionPolicyRequest` struct.
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RetentionPolicyRequest {
    /// The `max_lifetime` field.
    pub max_lifetime: Option<i64>,
    /// The `min_lifetime` field.
    pub min_lifetime: Option<i64>,
    /// The `is_expire_on_clients` field.
    pub is_expire_on_clients: Option<bool>,
}

/// The `RunRetentionRequest` struct.
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RunRetentionRequest {
    /// The `room_id` field.
    pub room_id: Option<String>,
}

/// See [`get_retention_policy`].
#[axum::debug_handler]
pub async fn get_retention_policy(_admin: AdminUser, State(ctx): State<AdminContext>) -> Result<Json<Value>, ApiError> {
    let policy = ctx.retention_service.get_server_policy_optional().await?;

    match policy {
        Some(policy) => Ok(Json(json!({
            "max_lifetime": policy.max_lifetime,
            "min_lifetime": policy.min_lifetime,
            "is_expire_on_clients": policy.is_expire_on_clients
        }))),
        None => Ok(Json(json!({
            "max_lifetime": null,
            "min_lifetime": null,
            "is_expire_on_clients": false
        }))),
    }
}

/// See [`set_retention_policy`].
#[axum::debug_handler]
pub async fn set_retention_policy(
    _admin: AdminUser,
    State(ctx): State<AdminContext>,
    Json(body): Json<RetentionPolicyRequest>,
) -> Result<Json<Value>, ApiError> {
    let policy = ctx
        .retention_service
        .upsert_server_policy(UpdateServerRetentionPolicyRequest {
            max_lifetime: body.max_lifetime,
            min_lifetime: body.min_lifetime,
            is_expire_on_clients: body.is_expire_on_clients,
        })
        .await?;

    Ok(Json(json!({
        "max_lifetime": policy.max_lifetime,
        "min_lifetime": policy.min_lifetime,
        "is_expire_on_clients": policy.is_expire_on_clients
    })))
}

/// See [`get_room_retention_policy`].
#[axum::debug_handler]
pub async fn get_room_retention_policy(
    _admin: AdminUser,
    State(ctx): State<AdminContext>,
    Path(room_id): Path<RoomId>,
) -> Result<Json<Value>, ApiError> {
    let room_exists = ctx.room_service.state().room_exists(&room_id).await?;

    if !room_exists {
        return Err(ApiError::not_found("Room not found".to_string()));
    }

    let policy = ctx.retention_service.get_room_policy(&room_id).await?;

    match policy {
        Some(policy) => Ok(Json(json!({
            "room_id": room_id,
            "max_lifetime": policy.max_lifetime,
            "min_lifetime": policy.min_lifetime,
            "is_expire_on_clients": policy.is_expire_on_clients
        }))),
        None => Ok(Json(json!({
            "room_id": room_id,
            "max_lifetime": null,
            "min_lifetime": null,
            "is_expire_on_clients": false
        }))),
    }
}

/// See [`set_room_retention_policy`].
#[axum::debug_handler]
pub async fn set_room_retention_policy(
    _admin: AdminUser,
    State(ctx): State<AdminContext>,
    Path(room_id): Path<RoomId>,
    Json(body): Json<RetentionPolicyRequest>,
) -> Result<Json<Value>, ApiError> {
    if !ctx.room_service.state().room_exists(&room_id).await? {
        return Err(ApiError::not_found("Room not found".to_string()));
    }

    let policy = ctx
        .retention_service
        .set_room_policy(CreateRoomRetentionPolicyRequest {
            room_id: room_id.clone().to_string(),
            max_lifetime: body.max_lifetime,
            min_lifetime: body.min_lifetime,
            is_expire_on_clients: body.is_expire_on_clients,
        })
        .await?;

    Ok(Json(json!({
        "room_id": room_id,
        "max_lifetime": policy.max_lifetime,
        "min_lifetime": policy.min_lifetime,
        "is_expire_on_clients": policy.is_expire_on_clients
    })))
}

/// See [`run_retention`].
#[axum::debug_handler]
pub async fn run_retention(
    _admin: AdminUser,
    State(ctx): State<AdminContext>,
    Json(body): Json<RunRetentionRequest>,
) -> Result<Json<Value>, ApiError> {
    match body.room_id {
        Some(room_id) => {
            if !ctx.room_service.state().room_exists(&room_id).await? {
                return Err(ApiError::not_found("Room not found".to_string()));
            }

            let log = ctx.retention_service.run_cleanup(&room_id).await?;
            Ok(Json(json!({
                "started": true,
                "room_id": room_id,
                "events_deleted": log.events_deleted,
                "status": log.status,
                "completed_ts": log.completed_ts
            })))
        }
        None => {
            let cleaned = ctx.retention_service.run_scheduled_cleanups().await?;
            Ok(Json(json!({
                "started": true,
                "scope": "all_rooms",
                "events_deleted": cleaned
            })))
        }
    }
}

/// See [`get_retention_status`].
#[axum::debug_handler]
pub async fn get_retention_status(_admin: AdminUser, State(ctx): State<AdminContext>) -> Result<Json<Value>, ApiError> {
    let status = ctx.retention_service.get_status_summary().await?;

    let last_run = status.last_run.map(|summary| {
        json!({
            "started_ts": summary.started_ts,
            "completed_ts": summary.completed_ts,
            "duration_ms": summary.duration_ms,
            "expired_events_deleted": summary.expired_events_deleted,
            "expired_beacons_deleted": summary.expired_beacons_deleted,
            "expired_uploads_deleted": summary.expired_uploads_deleted,
            "expired_audit_events_deleted": summary.expired_audit_events_deleted,
            "failed_tasks": summary.failed_tasks
        })
    });

    Ok(Json(json!({
        "server_policy_enabled": status.server_policy_enabled,
        "rooms_with_custom_policy": status.rooms_with_custom_policy,
        "lifecycle_cleanup_enabled": ctx.config.retention.lifecycle_cleanup_enabled,
        "audit_retention_days": ctx.config.retention.audit_retention_days,
        "last_run": last_run
    })))
}
