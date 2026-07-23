// Burn After Read Routes - 阅后即焚路由
// Self-destructing messages with automatic deletion

use crate::web::routes::context::RoomContext;
use crate::web::routes::room_access::ensure_room_member_ctx;
use crate::web::routes::{ApiError, AppState, AuthenticatedUser};
use axum::{
    extract::Path,
    extract::State,
    routing::{get, post, put},
    Json, Router,
};
use serde_json::{json, Value};
use synapse_common::current_timestamp_millis;

pub fn create_burn_after_read_router(state: AppState) -> Router<AppState> {
    Router::new()
        .route(
            "/_matrix/client/v1/rooms/{room_id}/burn",
            put(enable_burn).get(get_burn_settings),
        )
        .route(
            "/_matrix/client/v1/rooms/{room_id}/burn/pending",
            get(get_pending_burns),
        )
        .route(
            "/_matrix/client/v1/rooms/{room_id}/burn/{event_id}",
            post(mark_burn_read).delete(cancel_burn),
        )
        .route(
            "/_matrix/client/v1/user/burn/config",
            put(set_global_burn_config),
        )
        .route("/_matrix/client/v1/user/burn/stats", get(get_burn_stats))
        // v3 paths (frontend compatibility)
        .route(
            "/_matrix/client/v3/rooms/{room_id}/burn",
            put(enable_burn).get(get_burn_settings),
        )
        .route(
            "/_matrix/client/v3/rooms/{room_id}/burn/pending",
            get(get_pending_burns),
        )
        .route(
            "/_matrix/client/v3/rooms/{room_id}/burn/{event_id}",
            post(mark_burn_read).delete(cancel_burn),
        )
        .route(
            "/_matrix/client/v3/user/burn/config",
            put(set_global_burn_config),
        )
        .route("/_matrix/client/v3/user/burn/stats", get(get_burn_stats))
        .with_state(state)
}

pub fn burn_after_read_route_manifest() -> Vec<crate::web::routes::route_ledger::RouteEntry> {
    use crate::web::routes::route_ledger::RouteEntry;
    use axum::http::Method;

    [
        (Method::PUT, "/_matrix/client/v1/rooms/{room_id}/burn"),
        (Method::GET, "/_matrix/client/v1/rooms/{room_id}/burn"),
        (Method::GET, "/_matrix/client/v1/rooms/{room_id}/burn/pending"),
        (Method::POST, "/_matrix/client/v1/rooms/{room_id}/burn/{event_id}"),
        (Method::DELETE, "/_matrix/client/v1/rooms/{room_id}/burn/{event_id}"),
        (Method::PUT, "/_matrix/client/v1/user/burn/config"),
        (Method::GET, "/_matrix/client/v1/user/burn/stats"),
        // v3 paths
        (Method::PUT, "/_matrix/client/v3/rooms/{room_id}/burn"),
        (Method::GET, "/_matrix/client/v3/rooms/{room_id}/burn"),
        (Method::GET, "/_matrix/client/v3/rooms/{room_id}/burn/pending"),
        (Method::POST, "/_matrix/client/v3/rooms/{room_id}/burn/{event_id}"),
        (Method::DELETE, "/_matrix/client/v3/rooms/{room_id}/burn/{event_id}"),
        (Method::PUT, "/_matrix/client/v3/user/burn/config"),
        (Method::GET, "/_matrix/client/v3/user/burn/stats"),
    ]
    .into_iter()
    .map(|(m, p)| RouteEntry::new(m, p, "burn_after_read"))
    .collect()
}

/// Enable burn after read for a room
/// PUT /_matrix/client/v1/rooms/{room_id}/burn
pub async fn enable_burn(
    State(ctx): State<RoomContext>,
    auth_user: AuthenticatedUser,
    Path(room_id): Path<String>,
    Json(body): Json<Value>,
) -> Result<Json<Value>, ApiError> {
    ensure_room_member_ctx(&ctx, &auth_user, &room_id, "You must be a room member to configure burn-after-read")
        .await?;

    let room_exists: bool = ctx
        .room_service
        .state()
        .room_exists(&room_id)
        .await
        .map_err(|e| ApiError::internal_with_log("Failed to check room existence", &e))?;

    if !room_exists {
        return Err(ApiError::not_found(format!("Room '{room_id}' not found")));
    }

    let enabled = body.get("enabled").and_then(|v| v.as_bool()).unwrap_or(true);

    let burn_after_ms = body.get("burn_after_ms").and_then(|v| v.as_i64()).unwrap_or(60_000); // Default 1 minute

    ctx.burn_after_read
        .set_burn_enabled(&auth_user.user_id, &room_id, enabled, burn_after_ms)
        .await
        .map_err(|e| ApiError::internal_with_log("Failed to enable burn", &e))?;

    Ok(Json(json!({
        "enabled": enabled,
        "burn_after_ms": burn_after_ms,
    })))
}

/// Get burn settings for a room
/// GET /_matrix/client/v1/rooms/{room_id}/burn
pub async fn get_burn_settings(
    State(ctx): State<RoomContext>,
    auth_user: AuthenticatedUser,
    Path(room_id): Path<String>,
) -> Result<Json<Value>, ApiError> {
    ensure_room_member_ctx(&ctx, &auth_user, &room_id, "You must be a room member to view burn-after-read settings")
        .await?;

    let room_exists: bool = ctx
        .room_service
        .state()
        .room_exists(&room_id)
        .await
        .map_err(|e| ApiError::internal_with_log("Failed to check room existence", &e))?;

    if !room_exists {
        return Err(ApiError::not_found(format!("Room '{room_id}' not found")));
    }

    let settings: Option<synapse_services::burn_after_read_service::BurnSettings> = ctx
        .burn_after_read
        .get_burn_settings(&auth_user.user_id, &room_id)
        .await
        .map_err(|e| ApiError::internal_with_log("Failed to get settings", &e))?;

    match settings {
        Some(s) => Ok(Json(json!({
            "enabled": s.is_enabled,
            "burn_after_ms": s.burn_after_ms,
        }))),
        None => Ok(Json(json!({
            "enabled": false,
            "burn_after_ms": 60_000,
        }))),
    }
}

/// Mark message as read (triggers burn)
/// POST /_matrix/client/v1/rooms/{room_id}/burn/{event_id}
pub async fn mark_burn_read(
    State(ctx): State<RoomContext>,
    auth_user: AuthenticatedUser,
    Path((room_id, event_id)): Path<(String, String)>,
) -> Result<Json<Value>, ApiError> {
    ensure_room_member_ctx(&ctx, &auth_user, &room_id, "You must be a room member to mark burn-after-read events")
        .await?;

    let room_exists: bool = ctx
        .room_service
        .state()
        .room_exists(&room_id)
        .await
        .map_err(|e| ApiError::internal_with_log("Failed to check room existence", &e))?;

    if !room_exists {
        return Err(ApiError::not_found(format!("Room '{room_id}' not found")));
    }

    let settings: Option<synapse_services::burn_after_read_service::BurnSettings> = ctx
        .burn_after_read
        .get_burn_settings(&auth_user.user_id, &room_id)
        .await
        .map_err(|e| ApiError::internal_with_log("Failed to get settings", &e))?;

    let (enabled, burn_after_ms) = match settings {
        Some(s) => (s.is_enabled, s.burn_after_ms),
        None => return Err(ApiError::bad_request("Burn not enabled for this room".to_string())),
    };

    if !enabled {
        return Err(ApiError::bad_request("Burn not enabled for this room".to_string()));
    }

    let delete_ts = current_timestamp_millis() + burn_after_ms;

    ctx.burn_after_read
        .schedule_burn(&auth_user.user_id, &room_id, &event_id, burn_after_ms)
        .await
        .map_err(|e| ApiError::internal_with_log("Failed to schedule burn", &e))?;

    Ok(Json(json!({
        "success": true,
        "will_delete_at": delete_ts,
    })))
}

/// Get pending burn events
/// GET /_matrix/client/v1/rooms/{room_id}/burn/pending
pub async fn get_pending_burns(
    State(ctx): State<RoomContext>,
    auth_user: AuthenticatedUser,
    Path(room_id): Path<String>,
) -> Result<Json<Value>, ApiError> {
    ensure_room_member_ctx(
        &ctx,
        &auth_user,
        &room_id,
        "You must be a room member to view pending burn-after-read events",
    )
    .await?;

    let room_exists: bool = ctx
        .room_service
        .state()
        .room_exists(&room_id)
        .await
        .map_err(|e| ApiError::internal_with_log("Failed to check room existence", &e))?;

    if !room_exists {
        return Err(ApiError::not_found(format!("Room '{room_id}' not found")));
    }

    let pending: Vec<synapse_services::burn_after_read_service::BurnEvent> = ctx
        .burn_after_read
        .get_pending_burns(&auth_user.user_id, &room_id)
        .await
        .map_err(|e| ApiError::internal_with_log("Failed to get pending", &e))?;

    let events: Vec<Value> = pending
        .into_iter()
        .map(|p| {
            json!({
                "event_id": p.event_id,
                "created_at": p.created_ts,
                "delete_ts": p.delete_ts,
            })
        })
        .collect();

    Ok(Json(json!({
        "events": events,
    })))
}

/// Cancel pending burn
/// DELETE /_matrix/client/v1/rooms/{room_id}/burn/{event_id}
pub async fn cancel_burn(
    State(ctx): State<RoomContext>,
    auth_user: AuthenticatedUser,
    Path((room_id, event_id)): Path<(String, String)>,
) -> Result<Json<Value>, ApiError> {
    ensure_room_member_ctx(&ctx, &auth_user, &room_id, "You must be a room member to cancel burn-after-read events")
        .await?;

    let room_exists: bool = ctx
        .room_service
        .state()
        .room_exists(&room_id)
        .await
        .map_err(|e| ApiError::internal_with_log("Failed to check room existence", &e))?;

    if !room_exists {
        return Err(ApiError::not_found(format!("Room '{room_id}' not found")));
    }

    ctx.burn_after_read
        .cancel_burn(&auth_user.user_id, &room_id, &event_id)
        .await
        .map_err(|e| ApiError::internal_with_log("Failed to cancel burn", &e))?;

    Ok(Json(json!({
        "success": true,
    })))
}

/// Set global burn settings
/// PUT /_matrix/client/v1/user/burn/config
pub async fn set_global_burn_config(
    State(ctx): State<RoomContext>,
    auth_user: AuthenticatedUser,
    Json(body): Json<Value>,
) -> Result<Json<Value>, ApiError> {
    let default_burn_ms: i64 = body.get("default_burn_ms").and_then(|v| v.as_i64()).unwrap_or(60_000);

    ctx.burn_after_read
        .set_user_default(&auth_user.user_id, default_burn_ms)
        .await
        .map_err(|e| ApiError::internal_with_log("Failed to set config", &e))?;

    Ok(Json(json!({
        "default_burn_ms": default_burn_ms,
    })))
}

/// Get burn statistics
/// GET /_matrix/client/v1/user/burn/stats
pub async fn get_burn_stats(
    State(ctx): State<RoomContext>,
    auth_user: AuthenticatedUser,
) -> Result<Json<Value>, ApiError> {
    let stats: synapse_services::burn_after_read_service::BurnStats = ctx
        .burn_after_read
        .get_user_stats(&auth_user.user_id)
        .await
        .map_err(|e| ApiError::internal_with_log("Failed to get stats", &e))?;

    Ok(Json(json!({
        "total_burned": stats.total_burned,
        "total_pending": stats.total_pending,
        "rooms_with_burn_enabled": stats.rooms_enabled,
    })))
}
