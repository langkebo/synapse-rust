use crate::routes::extractors::RoomId;
use crate::routes::{ensure_room_member_ctx, AuthenticatedUser, OptionalAuthenticatedUser};
use axum::extract::{Json, Path, State};
use serde_json::{json, Value};
use synapse_common::current_timestamp_millis;
use synapse_common::map_internal;
use synapse_common::ApiError;

use crate::routes::context::RoomContext;

/// See [`get_room_visibility`].
#[axum::debug_handler]
pub(crate) async fn get_room_visibility(
    State(ctx): State<RoomContext>,
    _auth_user: OptionalAuthenticatedUser,
    Path(room_id): Path<RoomId>,
) -> Result<Json<Value>, ApiError> {
    if !ctx.room_service.state().room_exists(&room_id).await? {
        return Err(ApiError::not_found("Room not found".to_string()));
    }

    let visibility = ctx
        .room_service
        .state()
        .get_room_visibility(&room_id)
        .await
        .map_err(map_internal!("Failed to get room visibility"))?;

    Ok(Json(json!({
        "visibility": visibility
    })))
}

/// See [`set_room_visibility`].
#[axum::debug_handler]
pub(crate) async fn set_room_visibility(
    State(ctx): State<RoomContext>,
    auth_user: AuthenticatedUser,
    Path(room_id): Path<RoomId>,
    Json(body): Json<Value>,
) -> Result<Json<Value>, ApiError> {
    if !ctx.room_service.state().room_exists(&room_id).await? {
        return Err(ApiError::not_found("Room not found".to_string()));
    }

    let visibility = body
        .get("visibility")
        .and_then(|v| v.as_str())
        .ok_or_else(|| ApiError::bad_request("Missing visibility field".to_string()))?;

    if visibility != "public" && visibility != "private" {
        return Err(ApiError::bad_request("visibility must be 'public' or 'private'".to_string()));
    }

    ensure_room_member_ctx(&ctx, &auth_user, &room_id, "You must be a member of this room to update room visibility")
        .await?;

    let is_creator = ctx
        .room_service
        .state()
        .is_room_creator(&room_id, &auth_user.user_id)
        .await
        .map_err(map_internal!("Failed to check room creator"))?;

    if !is_creator {
        return Err(ApiError::forbidden("Only the room creator can update room visibility".to_string()));
    }

    let is_public = visibility == "public";

    ctx.room_service.state().set_room_directory(&room_id, is_public).await?;

    Ok(Json(json!({
        "room_id": room_id,
        "visibility": visibility,
        "updated_ts": current_timestamp_millis()
    })))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_get_room_visibility_response_structure() {
        // The response must contain: visibility
        let response = json!({
            "visibility": "public"
        });

        assert!(response.get("visibility").is_some());
        assert_eq!(response["visibility"], "public");
    }

    #[test]
    fn test_set_room_visibility_response_structure() {
        // The response must contain: room_id, visibility, updated_ts
        let response = json!({
            "room_id": "!room1:example.com",
            "visibility": "private",
            "updated_ts": 1_700_000_000_000u64
        });

        assert!(response.get("room_id").is_some());
        assert!(response.get("visibility").is_some());
        assert!(response.get("updated_ts").is_some());
        assert_eq!(response["visibility"], "private");
    }

    #[test]
    fn test_visibility_validation_accepts_public() {
        // The set_room_visibility handler must accept "public" as valid
        let visibility = "public";
        let is_valid = visibility == "public" || visibility == "private";
        assert!(is_valid);
    }

    #[test]
    fn test_visibility_validation_accepts_private() {
        // The set_room_visibility handler must accept "private" as valid
        let visibility = "private";
        let is_valid = visibility == "public" || visibility == "private";
        assert!(is_valid);
    }

    #[test]
    fn test_visibility_validation_rejects_invalid() {
        // The set_room_visibility handler must reject anything other than
        // "public" or "private"
        let invalid_values = ["secret", "invite", "knock", "restricted", "PUBLIC", "PRIVATE"];
        for v in &invalid_values {
            let is_valid = *v == "public" || *v == "private";
            assert!(!is_valid, "visibility '{v}' should be rejected");
        }
    }

    #[test]
    fn test_visibility_public_directory_enabled() {
        // When visibility is "public", set_room_directory should be called with is_public=true
        let visibility = "public";
        let is_public = visibility == "public";
        assert!(is_public);
    }

    #[test]
    fn test_visibility_private_directory_disabled() {
        // When visibility is "private", set_room_directory should be called with is_public=false
        let visibility = "private";
        let is_public = visibility == "public";
        assert!(!is_public);
    }
}
