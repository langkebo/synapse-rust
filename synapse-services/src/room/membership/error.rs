//! Room membership domain error types for B3-5 error convergence.
//!
//! Defines `MembershipError` — the unified error type for the room membership domain.
//! All room membership service methods return `Result<T, MembershipError>`, which converts
//! to `ApiError` via `From<MembershipError> for ApiError`.

use thiserror::Error;

use synapse_common::ApiError;

/// Unified room membership domain error.
#[derive(Debug, Error)]
pub enum MembershipError {
    /// Database operation failed during membership processing.
    #[error("database error: {0}")]
    Database(#[from] sqlx::Error),

    /// Room not found.
    #[error("room not found: {0}")]
    NotFound(String),

    /// User not found.
    #[error("user not found: {0}")]
    UserNotFound(String),

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

impl From<MembershipError> for ApiError {
    fn from(e: MembershipError) -> Self {
        match e {
            MembershipError::Database(source) => {
                ApiError::database_with_cause("membership database error", source)
            }
            MembershipError::NotFound(msg) => ApiError::not_found(msg),
            MembershipError::UserNotFound(msg) => ApiError::not_found(msg),
            MembershipError::NotAuthorized(msg) => ApiError::forbidden(msg),
            MembershipError::InvalidInput(msg) => ApiError::bad_request(msg),
            MembershipError::Internal(msg) => ApiError::internal(msg),
        }
    }
}

impl From<ApiError> for MembershipError {
    fn from(e: ApiError) -> Self {
        MembershipError::Internal(e.to_string())
    }
}