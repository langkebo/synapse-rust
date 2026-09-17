//! Room messaging domain error types for B3-5 error convergence.
//!
//! Defines `RoomMessagingError` — the unified error type for the room messaging domain.
//! All room messaging service methods return `Result<T, RoomMessagingError>`, which converts
//! to `ApiError` via `From<RoomMessagingError> for ApiError`.

use thiserror::Error;

use synapse_common::ApiError;

/// Unified room messaging domain error.
#[derive(Debug, Error)]
pub enum RoomMessagingError {
    /// Database operation failed during room messaging processing.
    #[error("database error: {0}")]
    Database(#[from] sqlx::Error),

    /// Room not found.
    #[error("room not found: {0}")]
    NotFound(String),

    /// Event not found.
    #[error("event not found: {0}")]
    EventNotFound(String),

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

impl From<RoomMessagingError> for ApiError {
    fn from(e: RoomMessagingError) -> Self {
        match e {
            RoomMessagingError::Database(source) => {
                ApiError::database_with_cause("room messaging database error", source)
            }
            RoomMessagingError::NotFound(msg) => ApiError::not_found(msg),
            RoomMessagingError::EventNotFound(msg) => ApiError::not_found(msg),
            RoomMessagingError::NotAuthorized(msg) => ApiError::forbidden(msg),
            RoomMessagingError::InvalidInput(msg) => ApiError::bad_request(msg),
            RoomMessagingError::Internal(msg) => ApiError::internal(msg),
        }
    }
}

impl From<ApiError> for RoomMessagingError {
    fn from(e: ApiError) -> Self {
        RoomMessagingError::Internal(e.to_string())
    }
}