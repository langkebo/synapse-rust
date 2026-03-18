// Burn After Read Routes - 阅后即焚路由
// Self-destructing messages with automatic deletion

use crate::services::burn_after_read_service::BurnAfterReadService;
use crate::web::routes::{ApiError, AppState, AuthenticatedUser};
use axum::{
    extract::Path,
    extract::State,
    Json,
    Router,
    routing::{get, post, put},
};
use serde_json::{json, Value};
use chrono::Utc;
use tracing::warn;

pub fn create_burn_after_read_router(state: AppState) -> Router<AppState> {
    Router::new()
        .route("/_matrix/client/v1/rooms/{room_id}/burn", put(enable_burn).get(get_burn_settings))
        .route("/_matrix/client/v1/rooms/{room_id}/burn/pending", get(get_pending_burns))
        .route("/_matrix/client/v1/rooms/{room_id}/burn/{event_id}", post(mark_burn_read).delete(cancel_burn))
        .route("/_matrix/client/v1/user/burn/config", put(set_global_burn_config))
        .route("/_matrix/client/v1/user/burn/stats", get(get_burn_stats))
        .with_state(state)
}

/// Enable burn after read for a room
/// PUT /_matrix/client/v1/rooms/{room_id}/burn
pub async fn enable_burn(
    State(state): State<AppState>,
    auth_user: AuthenticatedUser,
    Path(room_id): Path<String>,
    Json(body): Json<Value>,
) -> Result<Json<Value>, ApiError> {
    let enabled = body
        .get("enabled")
        .and_then(|v| v.as_bool())
        .unwrap_or(true);

    let burn_after_ms = body
        .get("burn_after_ms")
        .and_then(|v| v.as_i64())
        .unwrap_or(60000); // Default 1 minute

    state
        .services
        .burn_after_read
        .set_burn_enabled(&auth_user.user_id, &room_id, enabled, burn_after_ms)
        .await
        .map_err(|e| ApiError::internal(format!("Failed to enable burn: {}", e)))?;

    Ok(Json(json!({
        "enabled": enabled,
        "burn_after_ms": burn_after_ms,
    })))
}

/// Get burn settings for a room
/// GET /_matrix/client/v1/rooms/{room_id}/burn
pub async fn get_burn_settings(
    State(state): State<AppState>,
    auth_user: AuthenticatedUser,
    Path(room_id): Path<String>,
) -> Result<Json<Value>, ApiError> {
    let settings = state
        .services
        .burn_after_read
        .get_burn_settings(&auth_user.user_id, &room_id)
        .await
        .map_err(|e| ApiError::internal(format!("Failed to get settings: {}", e)))?;

    match settings {
        Some(s) => Ok(Json(json!({
            "enabled": s.enabled,
            "burn_after_ms": s.burn_after_ms,
        }))),
        None => Ok(Json(json!({
            "enabled": false,
            "burn_after_ms": 60000,
        }))),
    }
}

/// Mark message as read (triggers burn)
/// POST /_matrix/client/v1/rooms/{room_id}/burn/{event_id}
pub async fn mark_burn_read(
    State(state): State<AppState>,
    auth_user: AuthenticatedUser,
    Path((room_id, event_id)): Path<(String, String)>,
) -> Result<Json<Value>, ApiError> {
    let settings = state
        .services
        .burn_after_read
        .get_burn_settings(&auth_user.user_id, &room_id)
        .await
        .map_err(|e| ApiError::internal(format!("Failed to get settings: {}", e)))?;

    let (enabled, burn_after_ms) = match settings {
        Some(s) => (s.enabled, s.burn_after_ms),
        None => return Err(ApiError::bad_request("Burn not enabled for this room".to_string())),
    };

    if !enabled {
        return Err(ApiError::bad_request("Burn not enabled for this room".to_string()));
    }

    // Schedule message deletion
    let state_clone = state.clone();
    let room_id_clone = room_id.clone();
    let event_id_clone = event_id.clone();
    let user_id = auth_user.user_id.clone();

    tokio::spawn(async move {
        tokio::time::sleep(tokio::time::Duration::from_millis(burn_after_ms as u64)).await;
        
        if let Err(e) = state_clone
            .services
            .burn_after_read
            .delete_burned_message(&user_id, &room_id_clone, &event_id_clone)
            .await
        {
            warn!("Failed to delete burned message: {}", e);
        }
    });

    Ok(Json(json!({
        "success": true,
        "will_delete_at": Utc::now().timestamp_millis() + burn_after_ms,
    })))
}

/// Get pending burn events
/// GET /_matrix/client/v1/rooms/{room_id}/burn/pending
pub async fn get_pending_burns(
    State(state): State<AppState>,
    auth_user: AuthenticatedUser,
    Path(room_id): Path<String>,
) -> Result<Json<Value>, ApiError> {
    let pending = state
        .services
        .burn_after_read
        .get_pending_burns(&auth_user.user_id, &room_id)
        .await
        .map_err(|e| ApiError::internal(format!("Failed to get pending: {}", e)))?;

    let events: Vec<Value> = pending
        .into_iter()
        .map(|p| json!({
            "event_id": p.event_id,
            "created_at": p.created_at,
            "delete_at": p.delete_at,
        }))
        .collect();

    Ok(Json(json!({
        "events": events,
    })))
}

/// Cancel pending burn
/// DELETE /_matrix/client/v1/rooms/{room_id}/burn/{event_id}
pub async fn cancel_burn(
    State(state): State<AppState>,
    auth_user: AuthenticatedUser,
    Path((room_id, event_id)): Path<(String, String)>,
) -> Result<Json<Value>, ApiError> {
    state
        .services
        .burn_after_read
        .cancel_burn(&auth_user.user_id, &room_id, &event_id)
        .await
        .map_err(|e| ApiError::internal(format!("Failed to cancel burn: {}", e)))?;

    Ok(Json(json!({
        "success": true,
    })))
}

/// Set global burn settings
/// PUT /_matrix/client/v1/user/burn/config
pub async fn set_global_burn_config(
    State(state): State<AppState>,
    auth_user: AuthenticatedUser,
    Json(body): Json<Value>,
) -> Result<Json<Value>, ApiError> {
    let default_burn_ms = body
        .get("default_burn_ms")
        .and_then(|v| v.as_i64())
        .unwrap_or(60000);

    state
        .services
        .burn_after_read
        .set_user_default(&auth_user.user_id, default_burn_ms)
        .await
        .map_err(|e| ApiError::internal(format!("Failed to set config: {}", e)))?;

    Ok(Json(json!({
        "default_burn_ms": default_burn_ms,
    })))
}

/// Get burn statistics
/// GET /_matrix/client/v1/user/burn/stats
pub async fn get_burn_stats(
    State(state): State<AppState>,
    auth_user: AuthenticatedUser,
) -> Result<Json<Value>, ApiError> {
    let stats = state
        .services
        .burn_after_read
        .get_user_stats(&auth_user.user_id)
        .await
        .map_err(|e| ApiError::internal(format!("Failed to get stats: {}", e)))?;

    Ok(Json(json!({
        "total_burned": stats.total_burned,
        "total_pending": stats.total_pending,
        "rooms_with_burn_enabled": stats.rooms_enabled,
    })))
}
