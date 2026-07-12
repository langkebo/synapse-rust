pub(crate) mod e2ee;
pub(crate) mod events;
pub(crate) mod management;
pub(crate) mod members;
pub(crate) mod receipts;
pub(crate) mod state;

pub(crate) use e2ee::*;
pub(crate) use events::*;
pub(crate) use management::*;
pub(crate) use members::*;
pub(crate) use receipts::*;
pub(crate) use state::*;

use crate::common::{parse_stream_token, ApiError};
use crate::web::routes::context::RoomContext;
use crate::web::routes::{ensure_room_member_ctx, ensure_room_member_strict_ctx, AuthenticatedUser};
use serde::{Deserialize, Serialize};

fn parse_room_messages_from_token(params: &serde_json::Value) -> i64 {
    params
        .get("from")
        .and_then(|v| v.as_str())
        .and_then(|token| parse_stream_token(token).or_else(|| token.parse().ok()))
        .unwrap_or(0)
}

pub(crate) async fn ensure_room_view_access(
    ctx: &RoomContext,
    auth_user: &AuthenticatedUser,
    room_id: &str,
) -> Result<(), ApiError> {
    ensure_room_member_strict_ctx(ctx, auth_user, room_id, "You must be a member of this room to view events").await?;

    Ok(())
}

pub(crate) fn normalize_room_event_type(event_type: &str) -> String {
    if event_type.starts_with("m.room.") || event_type.starts_with("m.") {
        event_type.to_string()
    } else {
        format!("m.room.{event_type}")
    }
}

pub(crate) fn state_event_content_response(content: &serde_json::Value) -> serde_json::Value {
    content.clone()
}

pub(crate) async fn ensure_room_state_write_access(
    ctx: &RoomContext,
    auth_user: &AuthenticatedUser,
    room_id: &str,
    event_type: &str,
) -> Result<(), ApiError> {
    ensure_room_member_ctx(ctx, auth_user, room_id, "You must be a member of this room to send state events").await?;

    ctx.auth_service.verify_state_event_write(room_id, &auth_user.user_id, event_type).await?;

    Ok(())
}

pub(crate) async fn get_room_event(
    ctx: &RoomContext,
    room_id: &str,
    event_id: &str,
) -> Result<serde_json::Value, ApiError> {
    ctx.room_service.messaging().get_event(room_id, event_id).await
}

#[derive(Debug, Deserialize)]
pub(crate) struct UpgradeRoomRequest {
    pub(crate) new_version: String,
}

#[derive(Debug, Serialize)]
pub(crate) struct UpgradeRoomResponse {
    pub(crate) replacement_room: String,
}

#[cfg(test)]
mod tests {
    use super::state_event_content_response;
    use serde_json::json;

    #[test]
    fn test_state_event_content_response_returns_raw_content_for_empty_state_key() {
        let content = json!({
            "topic": "raw topic payload"
        });

        let response = state_event_content_response(&content);

        assert_eq!(response, content);
        assert!(response.get("event_id").is_none());
        assert!(response.get("type").is_none());
    }

    #[test]
    fn test_state_event_content_response_returns_raw_content_for_keyed_state() {
        let content = json!({
            "enabled": true,
            "label": "alpha"
        });

        let response = state_event_content_response(&content);

        assert_eq!(response, content);
        assert!(response.get("state_key").is_none());
        assert!(response.get("sender").is_none());
    }
}
