use crate::common::ApiError;
use crate::web::routes::context::AdminContext;
use crate::web::routes::AdminUser;
use axum::{
    extract::{Path, State},
    routing::{get, post},
    Json, Router,
};
use serde::Deserialize;
use serde_json::{json, Value};
use synapse_storage::retention::{CreateRoomRetentionPolicyRequest, UpdateServerRetentionPolicyRequest};

pub fn create_retention_router() -> Router<crate::web::routes::AppState> {
    Router::new()
        .route("/_synapse/admin/v1/retention/policy", get(get_retention_policy))
        .route("/_synapse/admin/v1/retention/policy", post(set_retention_policy))
        .route("/_synapse/admin/v1/retention/policy/{room_id}", get(get_room_retention_policy))
        .route("/_synapse/admin/v1/retention/policy/{room_id}", post(set_room_retention_policy))
        .route("/_synapse/admin/v1/retention/run", post(run_retention))
        .route("/_synapse/admin/v1/retention/status", get(get_retention_status))
}

pub fn admin_retention_route_manifest() -> Vec<crate::web::routes::route_ledger::RouteEntry> {
    use crate::web::routes::route_ledger::RouteEntry;
    use axum::http::Method;
    [
        (Method::GET, "/_synapse/admin/v1/retention/policy"),
        (Method::POST, "/_synapse/admin/v1/retention/policy"),
        (Method::GET, "/_synapse/admin/v1/retention/policy/{room_id}"),
        (Method::POST, "/_synapse/admin/v1/retention/policy/{room_id}"),
        (Method::POST, "/_synapse/admin/v1/retention/run"),
        (Method::GET, "/_synapse/admin/v1/retention/status"),
    ]
    .into_iter()
    .map(|(m, p)| RouteEntry::new(m, p, "admin::retention"))
    .collect()
}

#[derive(Debug, Deserialize)]
pub struct RetentionPolicyRequest {
    pub max_lifetime: Option<i64>,
    pub min_lifetime: Option<i64>,
    pub is_expire_on_clients: Option<bool>,
}

#[derive(Debug, Deserialize)]
pub struct RunRetentionRequest {
    pub room_id: Option<String>,
}

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

#[axum::debug_handler]
pub async fn get_room_retention_policy(
    _admin: AdminUser,
    State(ctx): State<AdminContext>,
    Path(room_id): Path<String>,
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

#[axum::debug_handler]
pub async fn set_room_retention_policy(
    _admin: AdminUser,
    State(ctx): State<AdminContext>,
    Path(room_id): Path<String>,
    Json(body): Json<RetentionPolicyRequest>,
) -> Result<Json<Value>, ApiError> {
    if !ctx.room_service.state().room_exists(&room_id).await? {
        return Err(ApiError::not_found("Room not found".to_string()));
    }

    let policy = ctx
        .retention_service
        .set_room_policy(CreateRoomRetentionPolicyRequest {
            room_id: room_id.clone(),
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
            "cleanup_queue_items_processed": summary.cleanup_queue_items_processed,
            "cleanup_queue_rows_pruned": summary.cleanup_queue_rows_pruned,
            "failed_tasks": summary.failed_tasks
        })
    });

    Ok(Json(json!({
        "server_policy_enabled": status.server_policy_enabled,
        "rooms_with_custom_policy": status.rooms_with_custom_policy,
        "lifecycle_cleanup_enabled": ctx.config.retention.lifecycle_cleanup_enabled,
        "cleanup_batch_size": ctx.config.retention.cleanup_batch_size,
        "audit_retention_days": ctx.config.retention.audit_retention_days,
        "queue_retention_days": ctx.config.retention.queue_retention_days,
        "last_run": last_run
    })))
}
