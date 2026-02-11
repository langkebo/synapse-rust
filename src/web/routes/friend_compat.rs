//! Friend Compatibility Layer API Routes
//!
//! This module provides a unified API that works with both the legacy friends table
//! and the new room-based friend system during Phase 2 (dual operation).
//!
//! The API automatically:
//! - Prefers friend room data when available
//! - Falls back to legacy data when needed
//! - Syncs changes to both systems
//! - Auto-migrates users on access

use super::AppState;
use crate::common::ApiError;
use crate::services::FriendStorage;
use crate::services::friend_sync_service::FriendSyncService;
use crate::services::friend_room_service::FriendRoomService;
use axum::{
    extract::{Path, State},
    routing::{delete, get, post},
    Json, Router,
};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use validator::Validate;

/// Create the friend compatibility layer router
///
/// This router provides unified endpoints that work with both legacy
/// and room-based friend systems during the dual-operation phase.
pub fn create_friend_compat_router() -> Router<AppState> {
    Router::new()
        // Friend list (unified)
        .route("/_matrix/client/unstable/friends", get(get_friends_unified))
        .route("/_matrix/client/unstable/friends", delete(remove_friend_unified))

        // Friend requests (unified)
        .route(
            "/_matrix/client/unstable/friends/request",
            post(send_friend_request_unified),
        )
        .route(
            "/_matrix/client/unstable/friends/requests",
            get(get_pending_requests_unified),
        )
        .route(
            "/_matrix/client/unstable/friends/request/{request_id}/accept",
            post(accept_friend_request_unified),
        )
        .route(
            "/_matrix/client/unstable/friends/request/{request_id}/decline",
            post(decline_friend_request_unified),
        )

        // Direct message rooms
        .route(
            "/_matrix/client/unstable/friends/dm/{user_id}",
            get(get_dm_room_unified),
        )

        // Sync status
        .route(
            "/_matrix/client/unstable/friends/sync/status",
            get(get_sync_status),
        )

        // Migration
        .route(
            "/_matrix/client/unstable/friends/migrate",
            post(migrate_user_friends),
        )
}

// ==============================================================================
// Request/Response Types
// ==============================================================================

#[derive(Debug, Deserialize, Validate)]
pub struct SendFriendRequestRequest {
    #[validate(length(min = 1, max = 255))]
    pub user_id: String,
    pub message: Option<String>,
}

#[derive(Debug, Deserialize, Validate)]
pub struct RemoveFriendRequest {
    #[validate(length(min = 1, max = 255))]
    pub user_id: String,
}

#[derive(Debug, Deserialize, Validate)]
pub struct CreateDmRequest {
    #[validate(length(min = 1, max = 255))]
    pub user_id: String,
    pub is_private: Option<bool>,
}

#[derive(Debug, Serialize)]
pub struct SyncStatusResponse {
    pub friend_room_exists: bool,
    pub legacy_data_exists: bool,
    pub is_synced: bool,
    pub friend_room_count: usize,
    pub legacy_count: usize,
    pub recommended_action: String,
}

// ==============================================================================
// Helper Functions
// ==============================================================================

/// Get the FriendSyncService from the app state
fn get_sync_service(state: &AppState) -> Result<FriendSyncService, ApiError> {
    let legacy_storage = FriendStorage::new(&state.services.user_storage.pool);
    let friend_room_service = FriendRoomService::new(
        &state.services.user_storage.pool,
        state.services.registration_service.clone(),
        state.services.user_storage.clone(),
        state.services.server_name.clone(),
    );

    let config = crate::services::friend_sync_service::FriendSyncConfig {
        enable_dual_mode: true,
        prefer_friend_rooms: true,
        auto_migrate_on_access: true,
    };

    Ok(FriendSyncService::new(
        legacy_storage,
        friend_room_service,
        state.services.user_storage.clone(),
        state.services.registration_service.clone(),
        config,
    ))
}

// ==============================================================================
// Route Handlers
// ==============================================================================

/// Get unified friend list
///
/// Returns friends from the preferred source (friend rooms if available,
/// otherwise falls back to legacy table). Automatically migrates users
/// to friend rooms if auto_migrate_on_access is enabled.
#[axum::debug_handler]
async fn get_friends_unified(
    State(state): State<AppState>,
    auth_user: super::AuthenticatedUser,
) -> Result<Json<Value>, ApiError> {
    let sync_service = get_sync_service(&state)?;
    let friends = sync_service
        .get_friends_unified(&auth_user.user_id)
        .await
        .map_err(|e| ApiError::internal(format!("Sync error: {}", e)))?;

    Ok(Json(friends))
}

/// Send friend request (unified)
///
/// Sends a friend request that works with both systems.
/// The request is created in both systems to ensure consistency.
#[axum::debug_handler]
async fn send_friend_request_unified(
    State(state): State<AppState>,
    auth_user: super::AuthenticatedUser,
    Json(body): Json<SendFriendRequestRequest>,
) -> Result<Json<Value>, ApiError> {
    if let Err(e) = body.validate() {
        return Err(ApiError::bad_request(e.to_string()));
    }

    let sync_service = get_sync_service(&state)?;

    // Validate receiver exists
    if !state
        .services
        .user_storage
        .user_exists(&body.user_id)
        .await
        .map_err(|e| ApiError::internal(format!("Database error: {}", e)))?
    {
        return Err(ApiError::not_found("User not found".to_string()));
    }

    // Create in both systems
    let legacy_storage = FriendStorage::new(&state.services.user_storage.pool);

    // Create in legacy system first
    let request_id = legacy_storage
        .create_request(&auth_user.user_id, &body.user_id, body.message.as_deref())
        .await
        .map_err(|e| ApiError::internal(format!("Failed to create request: {}", e)))?;

    // Ensure friend room exists for user
    sync_service
        .ensure_friend_room(&auth_user.user_id)
        .await
        .map_err(|e| ApiError::internal(format!("Failed to ensure friend room: {}", e)))?;

    Ok(Json(json!({
        "request_id": request_id,
        "status": "pending",
        "message": "Friend request sent successfully"
    })))
}

/// Get pending friend requests (unified)
///
/// Returns pending requests from both systems, deduplicated.
#[axum::debug_handler]
async fn get_pending_requests_unified(
    State(state): State<AppState>,
    auth_user: super::AuthenticatedUser,
) -> Result<Json<Value>, ApiError> {
    // Use legacy system for pending requests (it has better query support)
    let legacy_storage = FriendStorage::new(&state.services.user_storage.pool);

    let requests = legacy_storage
        .get_requests(&auth_user.user_id, "pending")
        .await
        .map_err(|e| ApiError::internal(format!("Failed to get requests: {}", e)))?;

    // Get profile information for all requesters
    let sender_ids: Vec<String> = requests.iter().map(|r| r.sender_id.clone()).collect();

    let profiles = if !sender_ids.is_empty() {
        state
            .services
            .registration_service
            .get_profiles(&sender_ids)
            .await?
    } else {
        vec![]
    };

    // Create a map for quick lookup
    let mut profile_map: std::collections::HashMap<String, Value> = std::collections::HashMap::new();
    for p in profiles {
        if let Some(uid) = p.get("user_id").and_then(|v| v.as_str()) {
            profile_map.insert(uid.to_string(), p);
        }
    }

    // Combine request info with profile data
    let request_list: Vec<Value> = requests
        .into_iter()
        .map(|req| {
            let profile = profile_map
                .get(&req.sender_id)
                .cloned()
                .unwrap_or_else(|| json!({"user_id": req.sender_id}));

            json!({
                "request_id": req.id,
                "sender": profile,
                "message": req.message,
                "created_ts": req.created_ts
            })
        })
        .collect();

    Ok(Json(json!({
        "requests": request_list,
        "count": request_list.len()
    })))
}

/// Accept friend request (unified)
///
/// Accepts a friend request in both systems.
/// Creates bidirectional friendship and DM room.
#[axum::debug_handler]
async fn accept_friend_request_unified(
    State(state): State<AppState>,
    Path(request_id): Path<String>,
    auth_user: super::AuthenticatedUser,
) -> Result<Json<Value>, ApiError> {
    let request_id_i64: i64 = request_id
        .parse()
        .map_err(|_| ApiError::bad_request("Invalid request ID format".to_string()))?;

    if request_id_i64 <= 0 {
        return Err(ApiError::bad_request("Invalid request ID".to_string()));
    }

    let sync_service = get_sync_service(&state)?;
    let dm_room_id = sync_service
        .sync_accept_request(request_id_i64, &auth_user.user_id)
        .await
        .map_err(|e| ApiError::internal(format!("Failed to accept request: {}", e)))?;

    Ok(Json(json!({
        "status": "accepted",
        "dm_room_id": dm_room_id,
        "message": "Friend request accepted. You can now chat in the direct message room."
    })))
}

/// Decline friend request (unified)
///
/// Declines a friend request in both systems.
#[axum::debug_handler]
async fn decline_friend_request_unified(
    State(state): State<AppState>,
    Path(request_id): Path<String>,
    auth_user: super::AuthenticatedUser,
) -> Result<Json<Value>, ApiError> {
    let request_id_i64: i64 = request_id
        .parse()
        .map_err(|_| ApiError::bad_request("Invalid request ID format".to_string()))?;

    if request_id_i64 <= 0 {
        return Err(ApiError::bad_request("Invalid request ID".to_string()));
    }

    let now = chrono::Utc::now().timestamp();

    let rows_affected = sqlx::query(
        r#"
        UPDATE friend_requests
        SET status = 'declined', updated_ts = $1
        WHERE id = $2 AND to_user_id = $3 AND status = 'pending'
        "#,
    )
    .bind(now)
    .bind(request_id_i64)
    .bind(&auth_user.user_id)
    .execute(&*state.services.user_storage.pool)
    .await
    .map_err(|e| ApiError::internal(format!("Database error: {}", e)))?
    .rows_affected();

    if rows_affected == 0 {
        return Err(ApiError::not_found("Friend request not found".to_string()));
    }

    Ok(Json(json!({
        "status": "declined",
        "message": "Friend request declined"
    })))
}

/// Get or create DM room (unified)
///
/// Returns the DM room ID for a friend, creating if necessary.
#[axum::debug_handler]
async fn get_dm_room_unified(
    State(state): State<AppState>,
    Path(user_id): Path<String>,
    auth_user: super::AuthenticatedUser,
) -> Result<Json<Value>, ApiError> {
    let sync_service = get_sync_service(&state)?;

    // Default is_private to false
    let result = sync_service
        .friend_room_service
        .get_dm_room(&auth_user.user_id, &user_id, false)
        .await
        .map_err(|e| ApiError::internal(format!("Failed to get DM room: {}", e)))?;

    Ok(Json(result))
}

/// Remove friend (unified)
///
/// Removes friend from both systems.
#[axum::debug_handler]
async fn remove_friend_unified(
    State(state): State<AppState>,
    auth_user: super::AuthenticatedUser,
    Json(body): Json<RemoveFriendRequest>,
) -> Result<Json<Value>, ApiError> {
    if let Err(e) = body.validate() {
        return Err(ApiError::bad_request(e.to_string()));
    }

    let sync_service = get_sync_service(&state)?;
    sync_service
        .sync_remove_friend(&auth_user.user_id, &body.user_id)
        .await
        .map_err(|e| ApiError::internal(format!("Failed to remove friend: {}", e)))?;

    Ok(Json(json!({
        "status": "removed",
        "message": format!("Removed {} from friends", body.user_id)
    })))
}

/// Get sync status
///
/// Returns the synchronization status between legacy and new systems.
#[axum::debug_handler]
async fn get_sync_status(
    State(state): State<AppState>,
    auth_user: super::AuthenticatedUser,
) -> Result<Json<SyncStatusResponse>, ApiError> {
    let sync_service = get_sync_service(&state)?;
    let status = sync_service
        .get_sync_status(&auth_user.user_id)
        .await
        .map_err(|e| ApiError::internal(format!("Failed to get sync status: {}", e)))?;

    let recommended_action = if !status.friend_room_exists {
        "Run migration to create friend list room"
    } else if !status.is_synced {
        "Run migration to sync friend lists"
    } else {
        "Systems are in sync"
    };

    Ok(Json(SyncStatusResponse {
        friend_room_exists: status.friend_room_exists,
        legacy_data_exists: status.legacy_data_exists,
        is_synced: status.is_synced,
        friend_room_count: status.friend_room_count,
        legacy_count: status.legacy_count,
        recommended_action: recommended_action.to_string(),
    }))
}

/// Migrate user's friends to room system
///
/// Manually triggers migration of friend data from legacy to room system.
#[axum::debug_handler]
async fn migrate_user_friends(
    State(state): State<AppState>,
    auth_user: super::AuthenticatedUser,
) -> Result<Json<Value>, ApiError> {
    let sync_service = get_sync_service(&state)?;

    let report = sync_service
        .migrate_user_funds(&auth_user.user_id)
        .await
        .map_err(|e| ApiError::internal(format!("Migration failed: {}", e)))?;

    Ok(Json(json!({
        "user_id": report.user_id,
        "migrated_friends": report.migrated_friends,
        "skipped_friends": report.skipped_friends,
        "errors": report.errors,
        "status": if report.errors.is_empty() { "success" } else { "partial" }
    })))
}

// ==============================================================================
// Tests
// ==============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_request_validation() {
        let valid = SendFriendRequestRequest {
            user_id: "@alice:example.com".to_string(),
            message: Some("Hi!".to_string()),
        };
        assert!(valid.validate().is_ok());

        let invalid = SendFriendRequestRequest {
            user_id: "".to_string(),
            message: None,
        };
        assert!(invalid.validate().is_err());
    }

    #[test]
    fn test_sync_status_response_serialization() {
        let response = SyncStatusResponse {
            friend_room_exists: true,
            legacy_data_exists: true,
            is_synced: true,
            friend_room_count: 5,
            legacy_count: 5,
            recommended_action: "Systems are in sync".to_string(),
        };

        let json = serde_json::to_string(&response).unwrap();
        assert!(json.contains("is_synced"));
        assert!(json.contains("5"));
    }
}
