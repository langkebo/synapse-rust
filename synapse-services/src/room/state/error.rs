//! Room state domain error types for B3-5 error convergence.
//!
//! Defines `RoomStateError` — the unified error type for the room state domain.
//! All room state service methods return `Result<T, RoomStateError>`, which converts
//! to `ApiError` via `From<RoomStateError> for ApiError`.

use thiserror::Error;

use synapse_common::ApiError;

/// Unified room state domain error.
#[derive(Debug, Error)]
pub enum RoomStateError {
    /// Database operation failed during room state processing.
    #[error("database error: {0}")]
    Database(#[from] sqlx::Error),

    /// Room not found.
    #[error("room not found: {0}")]
    NotFound(String),

    /// Room already exists.
    #[error("room already exists: {0}")]
    AlreadyExists(String),

    /// Not authorized to perform the operation.
    #[error("not authorized: {0}")]
    NotAuthorized(String),

    /// Invalid input or request format.
    #[error("invalid input: {0}")]
    InvalidInput(String),

    /// Internal/un expected error.
    #[error("internal error: {0}")]
    Internal(String),
}

impl From<RoomStateError> for ApiError {
    fn from(e: RoomStateError) -> Self {
        match e {
            RoomStateError::Database(source) => {
                ApiError::database_with_cause("room state database error", source)
            }
            RoomStateError::NotFound(msg) => ApiError::not_found(msg),
            RoomStateError::AlreadyExists(msg) => ApiError::conflict(msg),
            RoomStateError::NotAuthorized(msg) => ApiError::forbidden(msg),
            RoomStateError::InvalidInput(msg) => ApiError::bad_request(msg),
            RoomStateError::Internal(msg) => ApiError::internal(msg),
        }
    }
}

impl From<ApiError> for RoomStateError {
    fn from(e: ApiError) -> Self {
        RoomStateError::Internal(e.to_string())
    }
}
