//! Legacy Friend API Routes (DEPRECATED)
//!
//! These endpoints are deprecated and return 410 Gone responses.
//! Clients should migrate to the new room-based friend system.
//!
//! New endpoints:
//! - /_matrix/client/v1/friends/* (room-based friend system)
//! - /_matrix/client/unstable/friends/* (compatibility layer)

use super::AppState;
use crate::common::ApiError;
use axum::{
    extract::Path,
    routing::{delete, get, post, put},
    Json, Router,
};
use serde_json::json;

/// Create the legacy friend router (DEPRECATED)
///
/// All endpoints return 410 Gone with migration information.
/// This router should be removed after client migration is complete.
pub fn create_friend_router(_state: AppState) -> Router<AppState> {
    Router::new()
        .route("/_synapse/enhanced/friends/search", get(deprecated_search_users))
        .route("/_synapse/enhanced/friends", get(deprecated_get_friends))
        .route("/_synapse/enhanced/friends/batch", get(deprecated_friends_batch))
        .route("/_synapse/enhanced/friend/request", get(deprecated_send_request))
        .route("/_synapse/enhanced/friend/requests", get(deprecated_get_requests))
        .route("/_synapse/enhanced/friend/request/{request_id}/accept", get(deprecated_accept_request))
        .route("/_synapse/enhanced/friend/request/{request_id}/decline", get(deprecated_decline_request))
        .route("/_synapse/enhanced/friend/request/{request_id}", delete(deprecated_cancel_request))
        .route("/_synapse/enhanced/friends/{friend_id}", delete(deprecated_remove_friend))
        .route("/_synapse/enhanced/friends/{friend_id}/note", put(deprecated_update_note))
        .route("/_synapse/enhanced/friends/categories", get(deprecated_get_categories))
        .route("/_synapse/enhanced/friends/categories", post(deprecated_create_category))
        .route(
            "/_synapse/enhanced/friends/categories/{category_id}",
            put(deprecated_update_category),
        )
        .route(
            "/_synapse/enhanced/friends/categories/{category_id}",
            delete(deprecated_delete_category),
        )
        .route(
            "/_synapse/enhanced/friends/{friend_id}/category",
            put(deprecated_set_friend_category),
        )
        .route("/_synapse/enhanced/friends/suggestions", get(deprecated_get_suggestions))
}

// ==============================================================================
// Deprecated Endpoint Handlers
// ==============================================================================

/// Return 410 Gone for deprecated endpoints
fn deprecated_response(endpoint: &str, new_endpoint: &str) -> ApiError {
    ApiError::gone(
        json!({
            "error": "Endpoint deprecated",
            "message": format!("The '{}' endpoint has been removed.", endpoint),
            "new_endpoint": new_endpoint,
            "migration_guide": "https://docs.example.com/friend-system-migration"
        })
        .to_string(),
    )
}

async fn deprecated_search_users() -> Result<Json<serde_json::Value>, ApiError> {
    Err(deprecated_response(
        "User Search",
        "/_matrix/client/v1/users/search",
    ))
}

async fn deprecated_get_friends() -> Result<Json<serde_json::Value>, ApiError> {
    Err(deprecated_response(
        "Get Friends",
        "/_matrix/client/v1/friends",
    ))
}

async fn deprecated_friends_batch() -> Result<Json<serde_json::Value>, ApiError> {
    Err(deprecated_response(
        "Batch Get Friends",
        "/_matrix/client/v1/friends",
    ))
}

async fn deprecated_send_request() -> Result<Json<serde_json::Value>, ApiError> {
    Err(deprecated_response(
        "Send Friend Request",
        "/_matrix/client/v1/friends/request",
    ))
}

async fn deprecated_get_requests() -> Result<Json<serde_json::Value>, ApiError> {
    Err(deprecated_response(
        "Get Friend Requests",
        "/_matrix/client/v1/friends/requests",
    ))
}

async fn deprecated_accept_request(
    Path(_request_id): Path<String>,
) -> Result<Json<serde_json::Value>, ApiError> {
    Err(deprecated_response(
        "Accept Friend Request",
        "/_matrix/client/v1/friends/request/{request_id}/accept",
    ))
}

async fn deprecated_decline_request(
    Path(_request_id): Path<String>,
) -> Result<Json<serde_json::Value>, ApiError> {
    Err(deprecated_response(
        "Decline Friend Request",
        "/_matrix/client/v1/friends/request/{request_id}/decline",
    ))
}

async fn deprecated_cancel_request(
    Path(_request_id): Path<String>,
) -> Result<Json<serde_json::Value>, ApiError> {
    Err(deprecated_response(
        "Cancel Friend Request",
        "/_matrix/client/v1/friends/request/{request_id}",
    ))
}

async fn deprecated_remove_friend(
    Path(_friend_id): Path<String>,
) -> Result<Json<serde_json::Value>, ApiError> {
    Err(deprecated_response(
        "Remove Friend",
        "/_matrix/client/v1/friends",
    ))
}

async fn deprecated_update_note() -> Result<Json<serde_json::Value>, ApiError> {
    Err(deprecated_response(
        "Update Friend Note",
        "Use room state event instead",
    ))
}

async fn deprecated_get_categories() -> Result<Json<serde_json::Value>, ApiError> {
    Err(deprecated_response(
        "Get Friend Categories",
        "Use Matrix spaces instead",
    ))
}

async fn deprecated_create_category() -> Result<Json<serde_json::Value>, ApiError> {
    Err(deprecated_response(
        "Create Friend Category",
        "Use Matrix spaces instead",
    ))
}

async fn deprecated_update_category() -> Result<Json<serde_json::Value>, ApiError> {
    Err(deprecated_response(
        "Update Friend Category",
        "Use Matrix spaces instead",
    ))
}

async fn deprecated_delete_category() -> Result<Json<serde_json::Value>, ApiError> {
    Err(deprecated_response(
        "Delete Friend Category",
        "Use Matrix spaces instead",
    ))
}

async fn deprecated_set_friend_category() -> Result<Json<serde_json::Value>, ApiError> {
    Err(deprecated_response(
        "Set Friend Category",
        "Use Matrix spaces instead",
    ))
}

async fn deprecated_get_suggestions() -> Result<Json<serde_json::Value>, ApiError> {
    Err(deprecated_response(
        "Get Friend Suggestions",
        "Use user directory instead",
    ))
}
