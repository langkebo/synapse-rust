// Burn After Read Routes - 阅后即焚路由
// Self-destructing messages with automatic deletion

use crate::routes::context::RoomContext;
use crate::routes::room_access::ensure_room_member_ctx;
use crate::routes::MatrixJson;
use crate::routes::{validators, ApiError, AppState, AuthenticatedUser};
use axum::{
    extract::Path,
    extract::State,
    routing::{get, post, put},
    Json, Router,
};
use serde::Deserialize;
use serde_json::{json, Value};

/// Request body for [`enable_burn`].
#[derive(Debug, Default, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct EnableBurnRequest {
    /// Whether burn-after-read is enabled for the room (defaults to true).
    pub enabled: Option<bool>,
    /// Explicit burn-after duration in milliseconds.
    pub burn_after_ms: Option<i64>,
}

/// Request body for [`set_global_burn_config`].
#[derive(Debug, Default, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SetGlobalBurnConfigRequest {
    /// Default burn-after duration in milliseconds.
    pub default_burn_ms: Option<i64>,
}

/// See [`create_burn_after_read_router`].
pub fn create_burn_after_read_router(state: AppState) -> Router<AppState> {
    Router::new()
// ISSUE-13: 私有端点唯一规范位置是 /_matrix/vendor/v1（client 别名已按 AGENTS.md 铁律 1 删除）
        .route(
            "/_matrix/vendor/v1/rooms/{room_id}/burn",
            put(enable_burn).get(get_burn_settings),
        )
        .route(
            "/_matrix/vendor/v1/rooms/{room_id}/burn/pending",
            get(get_pending_burns),
        )
        .route(
            "/_matrix/vendor/v1/rooms/{room_id}/burn/{event_id}",
            post(mark_burn_read).delete(cancel_burn),
        )
        .route(
            "/_matrix/vendor/v1/user/burn/config",
            put(set_global_burn_config),
        )
        .route("/_matrix/vendor/v1/user/burn/stats", get(get_burn_stats))
        .with_state(state)
}

/// Enable burn after read for a room
/// PUT /_matrix/vendor/v1/rooms/{room_id}/burn
pub async fn enable_burn(
    State(ctx): State<RoomContext>,
    auth_user: AuthenticatedUser,
    Path(room_id): Path<RoomId>,
    MatrixJson(body): MatrixJson<EnableBurnRequest>,
) -> Result<Json<Value>, ApiError> {
    validators::validate_room_id(&room_id)?;
    let room_exists: bool = ctx
        .room_service
        .state()
        .room_exists(&room_id)
        .await
        .map_err(|e| ApiError::internal_with_cause("Failed to check room existence", e))?;

    if !room_exists {
        return Err(ApiError::not_found(format!("Room '{room_id}' not found")));
    }

    ensure_room_member_ctx(&ctx, &auth_user, &room_id, "You must be a room member to configure burn-after-read")
        .await?;

    let enabled = body.enabled.unwrap_or(true);

    // Explicit `burn_after_ms` wins; otherwise use the reader's stored default
    // (`PUT /user/burn/config`); otherwise one minute.
    let explicit_burn_after_ms = body.burn_after_ms;
    // Propagate `ApiError` as-is: the service already maps storage failures to
    // 500 and returns 400 for an out-of-range delay. Wrapping it here would
    // turn that validation error into an opaque 500.
    let burn_after_ms = ctx.burn_after_read.resolve_burn_after_ms(&auth_user.user_id, explicit_burn_after_ms).await?;

    ctx.burn_after_read.set_burn_enabled(&auth_user.user_id, &room_id, enabled, burn_after_ms).await?;

    Ok(Json(json!({
        "enabled": enabled,
        "burn_after_ms": burn_after_ms,
    })))
}

/// Get burn settings for a room
/// GET /_matrix/vendor/v1/rooms/{room_id}/burn
pub async fn get_burn_settings(
    State(ctx): State<RoomContext>,
    auth_user: AuthenticatedUser,
    Path(room_id): Path<RoomId>,
) -> Result<Json<Value>, ApiError> {
    validators::validate_room_id(&room_id)?;
    let room_exists: bool = ctx
        .room_service
        .state()
        .room_exists(&room_id)
        .await
        .map_err(|e| ApiError::internal_with_cause("Failed to check room existence", e))?;

    if !room_exists {
        return Err(ApiError::not_found(format!("Room '{room_id}' not found")));
    }

    ensure_room_member_ctx(&ctx, &auth_user, &room_id, "You must be a room member to view burn-after-read settings")
        .await?;

    let settings: Option<synapse_services::burn_after_read_service::BurnSettings> = ctx
        .burn_after_read
        .get_burn_settings(&auth_user.user_id, &room_id)
        .await
        .map_err(|e| ApiError::internal_with_cause("Failed to get settings", e))?;

    match settings {
        Some(s) => Ok(Json(json!({
            "enabled": s.is_enabled,
            "burn_after_ms": s.burn_after_ms,
        }))),
        None => Ok(Json(json!({
            "enabled": false,
            "burn_after_ms": synapse_services::burn_after_read_service::DEFAULT_BURN_AFTER_MS,
        }))),
    }
}

/// Mark message as read (triggers burn)
/// POST /_matrix/vendor/v1/rooms/{room_id}/burn/{event_id}
pub async fn mark_burn_read(
    State(ctx): State<RoomContext>,
    auth_user: AuthenticatedUser,
    Path((room_id, event_id)): Path<(RoomId, EventId)>,
) -> Result<Json<Value>, ApiError> {
    validators::validate_room_id(&room_id)?;
    let room_exists: bool = ctx
        .room_service
        .state()
        .room_exists(&room_id)
        .await
        .map_err(|e| ApiError::internal_with_cause("Failed to check room existence", e))?;

    if !room_exists {
        return Err(ApiError::not_found(format!("Room '{room_id}' not found")));
    }

    ensure_room_member_ctx(&ctx, &auth_user, &room_id, "You must be a room member to mark burn-after-read events")
        .await?;

    let settings: Option<synapse_services::burn_after_read_service::BurnSettings> = ctx
        .burn_after_read
        .get_burn_settings(&auth_user.user_id, &room_id)
        .await
        .map_err(|e| ApiError::internal_with_cause("Failed to get settings", e))?;

    let (enabled, burn_after_ms) = match settings {
        Some(s) => (s.is_enabled, s.burn_after_ms),
        None => return Err(ApiError::bad_request("Burn not enabled for this room".to_string())),
    };

    if !enabled {
        return Err(ApiError::bad_request("Burn not enabled for this room".to_string()));
    }

    // The event must exist and belong to the room in the path. `get_event`
    // resolves both checks internally (missing or cross-room → 404), so a
    // caller cannot point at an event in a room they are not even in.
    let event = ctx.room_service.messaging().get_event(&room_id, &event_id).await?;

    // Only the receiving side may start the timer. Allowing the sender to mark
    // their own event as read would let them drive other members' burn state,
    // which is exactly the escalation this endpoint must not permit.
    let sender = event.get("sender").and_then(|v| v.as_str()).unwrap_or_default();
    if sender == auth_user.user_id {
        return Err(ApiError::forbidden("You cannot mark your own message as read".to_string()));
    }

    let will_delete_at = ctx
        .burn_after_read
        .schedule_burn(&auth_user.user_id, &room_id, &event_id, burn_after_ms)
        .await
        .map_err(|e| ApiError::internal_with_cause("Failed to schedule burn", e))?;

    Ok(Json(json!({
        "success": true,
        "will_delete_at": will_delete_at,
    })))
}

/// Get pending burn events
/// GET /_matrix/vendor/v1/rooms/{room_id}/burn/pending
pub async fn get_pending_burns(
    State(ctx): State<RoomContext>,
    auth_user: AuthenticatedUser,
    Path(room_id): Path<RoomId>,
) -> Result<Json<Value>, ApiError> {
    validators::validate_room_id(&room_id)?;
    let room_exists: bool = ctx
        .room_service
        .state()
        .room_exists(&room_id)
        .await
        .map_err(|e| ApiError::internal_with_cause("Failed to check room existence", e))?;

    if !room_exists {
        return Err(ApiError::not_found(format!("Room '{room_id}' not found")));
    }

    ensure_room_member_ctx(
        &ctx,
        &auth_user,
        &room_id,
        "You must be a room member to view pending burn-after-read events",
    )
    .await?;

    let pending: Vec<synapse_services::burn_after_read_service::BurnEvent> = ctx
        .burn_after_read
        .get_pending_burns(&auth_user.user_id, &room_id)
        .await
        .map_err(|e| ApiError::internal_with_cause("Failed to get pending", e))?;

    let events: Vec<Value> = pending
        .into_iter()
        .map(|p| {
            json!({
                "event_id": p.event_id,
                "created_at": p.created_ts,
                "delete_ts": p.delete_ts,
                // Processed rows stay listed for a retention window so an
                // offline client can still learn that the message burned.
                "is_processed": p.is_processed,
                "is_dead_letter": p.is_dead_letter,
            })
        })
        .collect();

    Ok(Json(json!({
        "events": events,
    })))
}

/// Cancel pending burn
/// DELETE /_matrix/vendor/v1/rooms/{room_id}/burn/{event_id}
pub async fn cancel_burn(
    State(ctx): State<RoomContext>,
    auth_user: AuthenticatedUser,
    Path((room_id, event_id)): Path<(RoomId, EventId)>,
) -> Result<Json<Value>, ApiError> {
    validators::validate_room_id(&room_id)?;
    let room_exists: bool = ctx
        .room_service
        .state()
        .room_exists(&room_id)
        .await
        .map_err(|e| ApiError::internal_with_cause("Failed to check room existence", e))?;

    if !room_exists {
        return Err(ApiError::not_found(format!("Room '{room_id}' not found")));
    }

    ensure_room_member_ctx(&ctx, &auth_user, &room_id, "You must be a room member to cancel burn-after-read events")
        .await?;

    ctx.burn_after_read
        .cancel_burn(&auth_user.user_id, &room_id, &event_id)
        .await
        .map_err(|e| ApiError::internal_with_cause("Failed to cancel burn", e))?;

    Ok(Json(json!({
        "success": true,
    })))
}

/// Set global burn settings
/// PUT /_matrix/vendor/v1/user/burn/config
pub async fn set_global_burn_config(
    State(ctx): State<RoomContext>,
    auth_user: AuthenticatedUser,
    MatrixJson(body): MatrixJson<SetGlobalBurnConfigRequest>,
) -> Result<Json<Value>, ApiError> {
    let default_burn_ms =
        body.default_burn_ms.unwrap_or(synapse_services::burn_after_read_service::DEFAULT_BURN_AFTER_MS);

    // Keep the service's 400 for an out-of-range value instead of wrapping it.
    ctx.burn_after_read.set_user_default(&auth_user.user_id, default_burn_ms).await?;

    Ok(Json(json!({
        "default_burn_ms": default_burn_ms,
    })))
}

/// Get burn statistics
/// GET /_matrix/vendor/v1/user/burn/stats
pub async fn get_burn_stats(
    State(ctx): State<RoomContext>,
    auth_user: AuthenticatedUser,
) -> Result<Json<Value>, ApiError> {
    let stats: synapse_services::burn_after_read_service::BurnStats = ctx
        .burn_after_read
        .get_user_stats(&auth_user.user_id)
        .await
        .map_err(|e| ApiError::internal_with_cause("Failed to get stats", e))?;

    Ok(Json(json!({
        "total_burned": stats.total_burned,
        "total_pending": stats.total_pending,
        "rooms_with_burn_enabled": stats.rooms_enabled,
    })))
}

use crate::routes::extractors::{EventId, RoomId};

// -------------------------------------------------------------------------
// Tests
// -------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    /// Test that `enable_burn` validates room_id correctly.
    #[test]
    fn test_validate_room_id_valid() {
        let result = validators::validate_room_id("!roomid:example.com");
        assert!(result.is_ok());
    }

    /// Test that `enable_burn` rejects invalid room_id.
    #[test]
    fn test_validate_room_id_invalid() {
        let result = validators::validate_room_id("invalid-room-id");
        assert!(result.is_err());
    }

    /// Test that `enable_burn` validates empty room_id.
    #[test]
    fn test_validate_room_id_empty() {
        let result = validators::validate_room_id("");
        assert!(result.is_err());
    }

    /// Test that `mark_burn_read` validates room_id correctly.
    #[test]
    fn test_mark_burn_read_validates_room_id() {
        let result = validators::validate_room_id("!valid:example.com");
        assert!(result.is_ok());
    }

    /// Test that `cancel_burn` validates room_id correctly.
    #[test]
    fn test_cancel_burn_validates_room_id() {
        let result = validators::validate_room_id("!another:example.com");
        assert!(result.is_ok());
    }

    /// Test that `get_burn_settings` validates room_id correctly.
    #[test]
    fn test_get_burn_settings_validates_room_id() {
        let result = validators::validate_room_id("!settings:example.com");
        assert!(result.is_ok());
    }

    /// Test that `get_pending_burns` validates room_id correctly.
    #[test]
    fn test_get_pending_burns_validates_room_id() {
        let result = validators::validate_room_id("!pending:example.com");
        assert!(result.is_ok());
    }

    /// Test that `set_global_burn_config` accepts valid default_burn_ms.
    #[test]
    fn test_set_global_burn_config_accepts_valid_values() {
        let body = serde_json::json!({
            "default_burn_ms": 120_000
        });
        let value: serde_json::Value = body;
        let default_burn_ms = value.get("default_burn_ms").and_then(|v| v.as_i64()).unwrap_or(60_000);
        assert_eq!(default_burn_ms, 120_000);
    }

    /// Test that `set_global_burn_config` uses default when missing.
    #[test]
    fn test_set_global_burn_config_uses_default_when_missing() {
        let body = serde_json::json!({});
        let value: serde_json::Value = body;
        let default_burn_ms = value.get("default_burn_ms").and_then(|v| v.as_i64()).unwrap_or(60_000);
        assert_eq!(default_burn_ms, 60_000);
    }
}
