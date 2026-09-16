//! MSC4140 — Cancellable delayed events route handlers.
//!
//! Implements the Gen 1 generic management endpoint for delayed events.
//! Clients schedule a delayed event by sending a normal `PUT /rooms/{roomId}/send/...`
//! with an `org.matrix.msc4140.delay` field in the body; the server returns a
//! `delay_id`. This module provides the management endpoints to cancel,
//! restart (heartbeat), or force-send a pending delayed event.
//!
//! Endpoint (Gen 1 — single endpoint with `action` body):
//!   POST /_matrix/client/unstable/org.matrix.msc4140/delayed_events/{delay_id}
//!   { "action": "send" | "cancel" | "restart" }
//!
//! See: https://github.com/matrix-org/matrix-spec-proposals/pull/4140

use crate::routes::context::AdminContext;
use crate::routes::extractors::AuthenticatedUser;
use crate::routes::AppState;
use axum::{
    extract::{Json, Path, State},
    http::HeaderMap,
    routing::post,
    Router,
};
use serde_json::{json, Value};
use synapse_common::ApiError;
use synapse_services::delayed_event_service::DelayedEventAction;

/// MSC4140 Gen 1 — Manage a delayed event.
///
/// `POST /_matrix/client/unstable/org.matrix.msc4140/delayed_events/{delay_id}`
///
/// The `delay_id` is the server-generated numeric id returned when the delayed
/// event was scheduled. Only the user who scheduled the event may manage it;
/// this is enforced fail-closed by comparing `event.user_id` to the caller.
pub(crate) async fn manage_delayed_event(
    State(ctx): State<AdminContext>,
    headers: HeaderMap,
    auth_user: AuthenticatedUser,
    Path(delay_id): Path<i64>,
    Json(body): Json<Value>,
) -> Result<Json<Value>, ApiError> {
    let request_id = crate::utils::auth::resolve_request_id(&headers);

    // Parse action from untyped JSON so missing/invalid fields return M_BAD_JSON
    // (400) rather than axum's default 422 deserialization rejection.
    let action_str = body
        .get("action")
        .and_then(|v| v.as_str())
        .ok_or_else(|| ApiError::bad_request("Missing or invalid 'action' field (must be a string)".to_string()))?;
    let action = DelayedEventAction::parse(action_str)?;

    // Ownership (fail-closed) and the pending-state transitions live in the
    // service; a foreign or missing delay_id both surface as M_NOT_FOUND.
    ctx.delayed_event_service.manage(delay_id, action, &auth_user.user_id, &request_id).await?;

    // MSC4140 success response is an empty JSON object.
    Ok(Json(json!({})))
}

/// MSC4140 router — mounts the delayed-events management endpoint under the
/// unstable namespace.
pub fn create_delayed_events_router() -> Router<AppState> {
    Router::new().route("/delayed_events/{delay_id}", post(manage_delayed_event))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_delayed_event_action_parse_valid() {
        assert_eq!(DelayedEventAction::parse("send").unwrap(), DelayedEventAction::Send);
        assert_eq!(DelayedEventAction::parse("cancel").unwrap(), DelayedEventAction::Cancel);
        assert_eq!(DelayedEventAction::parse("restart").unwrap(), DelayedEventAction::Restart);
    }

    #[test]
    fn test_delayed_event_action_parse_invalid() {
        assert!(DelayedEventAction::parse("delete").is_err());
        assert!(DelayedEventAction::parse("").is_err());
    }
}
