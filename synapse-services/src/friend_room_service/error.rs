//! Friend Room domain error types for B3-5 error convergence.
//!
//! Defines `FriendRoomError` — the unified error type for the friend room domain.
//! All friend room service methods return `Result<T, FriendRoomError>`, which converts
//! to `ApiError` via `From<FriendRoomError> for ApiError`.

use thiserror::Error;

use synapse_common::ApiError;

/// Unified friend room domain error.
#[derive(Debug, Error)]
pub enum FriendRoomError {
    /// Database operation failed during friend room processing.
    #[error("database error: {0}")]
    Database(#[from] sqlx::Error),

    /// Friend room not found.
    #[error("friend room not found: {0}")]
    NotFound(String),

    /// Friendship conflict (e.g. already friends).
    #[error("friendship conflict: {0}")]
    FriendshipConflict(String),

    /// Request already exists.
    #[error("request already exists: {0}")]
    RequestExists(String),

    /// Not authorized to perform the operation.
    #[error("not authorized: {0}")]
    NotAuthorized(String),

    /// Invalid input or request format.
    #[error("invalid input: {0}")]
    InvalidInput(String),

    /// Internal/ unexpected error.
    #[error("internal error: {0}")]
    Internal(String),
}

impl From<FriendRoomError> for ApiError {
    fn from(e: FriendRoomError) -> Self {
        match e {
            FriendRoomError::Database(source) => {
                // Preserve cause for diagnostics; maps to 500.
                ApiError::database_with_cause("friend room database error", source)
            }
            FriendRoomError::NotFound(msg) => ApiError::not_found(msg),
            FriendRoomError::FriendshipConflict(msg) => ApiError::conflict(msg),
            FriendRoomError::RequestExists(msg) => ApiError::conflict(msg),
            FriendRoomError::NotAuthorized(msg) => ApiError::forbidden(msg),
            FriendRoomError::InvalidInput(msg) => ApiError::bad_request(msg),
            FriendRoomError::Internal(msg) => ApiError::internal(msg),
        }
    }
}
