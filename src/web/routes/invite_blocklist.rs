// Invite Blocklist Routes - MSC4380
// Allows room admins to control who can be invited to a room

use crate::web::routes::{ApiError, AppState, AuthenticatedUser};
use axum::{
    extract::{Path, State},
    Json,
};
use serde_json::{json, Value};

/// Get room invite blocklist
/// GET /_matrix/client/v3/rooms/{room_id}/invite_blocklist
pub async fn get_invite_blocklist(
    State(state): State<AppState>,
    _auth_user: AuthenticatedUser,
    Path(room_id): Path<String>,
) -> Result<Json<Value>, ApiError> {
    let blocklist = state
        .services
        .invite_blocklist_storage
        .get_invite_blocklist(&room_id)
        .await
        .map_err(|e| ApiError::internal(format!("Failed to get blocklist: {}", e)))?;

    Ok(Json(json!({
        "blocklist": blocklist
    })))
}

/// Set room invite blocklist
/// POST /_matrix/client/v3/rooms/{room_id}/invite_blocklist
pub async fn set_invite_blocklist(
    State(state): State<AppState>,
    auth_user: AuthenticatedUser,
    Path(room_id): Path<String>,
    Json(body): Json<Value>,
) -> Result<Json<Value>, ApiError> {
    // Check if user is room admin
    let is_admin = state
        .services
        .room_service
        .is_room_creator(&room_id, &auth_user.user_id)
        .await
        .map_err(|e| ApiError::internal(format!("Failed to check admin: {}", e)))?;

    if !is_admin {
        return Err(ApiError::forbidden(
            "Only room admins can set invite blocklist".to_string(),
        ));
    }

    let user_ids: Vec<String> = body
        .get("user_ids")
        .and_then(|v| v.as_array())
        .map(|arr| {
            arr.iter()
                .filter_map(|v| v.as_str().map(String::from))
                .collect()
        })
        .unwrap_or_default();

    state
        .services
        .invite_blocklist_storage
        .set_invite_blocklist(&room_id, user_ids.clone())
        .await
        .map_err(|e| ApiError::internal(format!("Failed to set blocklist: {}", e)))?;

    Ok(Json(json!({
        "room_id": room_id,
        "blocklist": user_ids,
        "updated_ts": chrono::Utc::now().timestamp_millis()
    })))
}

/// Get room invite allowlist
/// GET /_matrix/client/v3/rooms/{room_id}/invite_allowlist
pub async fn get_invite_allowlist(
    State(state): State<AppState>,
    _auth_user: AuthenticatedUser,
    Path(room_id): Path<String>,
) -> Result<Json<Value>, ApiError> {
    let allowlist = state
        .services
        .invite_blocklist_storage
        .get_invite_allowlist(&room_id)
        .await
        .map_err(|e| ApiError::internal(format!("Failed to get allowlist: {}", e)))?;

    Ok(Json(json!({
        "allowlist": allowlist
    })))
}

/// Set room invite allowlist
/// POST /_matrix/client/v3/rooms/{room_id}/invite_allowlist
pub async fn set_invite_allowlist(
    State(state): State<AppState>,
    auth_user: AuthenticatedUser,
    Path(room_id): Path<String>,
    Json(body): Json<Value>,
) -> Result<Json<Value>, ApiError> {
    // Check if user is room admin
    let is_admin = state
        .services
        .room_service
        .is_room_creator(&room_id, &auth_user.user_id)
        .await
        .map_err(|e| ApiError::internal(format!("Failed to check admin: {}", e)))?;

    if !is_admin {
        return Err(ApiError::forbidden(
            "Only room admins can set invite allowlist".to_string(),
        ));
    }

    let user_ids: Vec<String> = body
        .get("user_ids")
        .and_then(|v| v.as_array())
        .map(|arr| {
            arr.iter()
                .filter_map(|v| v.as_str().map(String::from))
                .collect()
        })
        .unwrap_or_default();

    state
        .services
        .invite_blocklist_storage
        .set_invite_allowlist(&room_id, user_ids.clone())
        .await
        .map_err(|e| ApiError::internal(format!("Failed to set allowlist: {}", e)))?;

    Ok(Json(json!({
        "room_id": room_id,
        "allowlist": user_ids,
        "updated_ts": chrono::Utc::now().timestamp_millis()
    })))
}
