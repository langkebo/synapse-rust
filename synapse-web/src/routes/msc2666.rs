//! MSC2666: Rooms in common
//!
//! Returns the list of rooms that two users share (both are joined members).
//! Endpoint: `GET /_matrix/client/unstable/org.matrix.msc2666/user/{user_id}/common_rooms`

use synapse_common::ApiError;
use crate::routes::extractors::auth::AuthenticatedUser;
use crate::routes::state::AppState;
use axum::{
    extract::{Path, State},
    Json,
};
use serde_json::json;

/// GET /_matrix/client/unstable/org.matrix.msc2666/user/{user_id}/common_rooms
///
/// Returns rooms that the authenticated user and the target user have in common.
/// Both users must be joined members of the returned rooms.
pub async fn get_common_rooms(
    State(state): State<AppState>,
    auth_user: AuthenticatedUser,
    Path(target_user_id): Path<String>,
) -> Result<Json<serde_json::Value>, ApiError> {
    // Validate target user_id format
    if !target_user_id.starts_with('@') || !target_user_id.contains(':') {
        return Err(ApiError::bad_request("Invalid user_id format".to_string()));
    }

    // A user cannot query common rooms with themselves (would just be their own rooms)
    if target_user_id == auth_user.user_id {
        return Err(ApiError::bad_request(
            "Cannot query common rooms with yourself — use /joined_rooms instead".to_string(),
        ));
    }

    // Check that the target user exists
    let target_exists = state
        .services
        .account
        .user_storage
        .user_exists(&target_user_id)
        .await
        .map_err(|e| ApiError::internal_with_log("Failed to check user existence", &e))?;

    if !target_exists {
        return Err(ApiError::not_found("User not found".to_string()));
    }

    // Query rooms in common
    let room_ids = state
        .services
        .rooms
        .member_storage
        .get_rooms_in_common(&auth_user.user_id, &target_user_id)
        .await
        .map_err(|e| ApiError::internal_with_log("Failed to get rooms in common", &e))?;

    Ok(Json(json!({
        "rooms": room_ids,
        "count": room_ids.len()
    })))
}

pub fn create_msc2666_router() -> axum::Router<AppState> {
    axum::Router::new().route("/user/{user_id}/common_rooms", axum::routing::get(get_common_rooms))
}

pub fn msc2666_route_manifest() -> Vec<crate::routes::route_ledger::RouteEntry> {
    use crate::routes::route_ledger::RouteEntry;
    use axum::http::Method;

    vec![RouteEntry::new(
        Method::GET,
        "/_matrix/client/unstable/org.matrix.msc2666/user/{user_id}/common_rooms",
        "msc2666",
    )]
}

#[cfg(test)]
mod tests {
    #[test]
    fn test_msc2666_manifest_declares_route() {
        let entries = super::msc2666_route_manifest();
        assert_eq!(entries.len(), 1);
        assert_eq!(
            entries[0].path,
            "/_matrix/client/unstable/org.matrix.msc2666/user/{user_id}/common_rooms"
        );
    }
}
