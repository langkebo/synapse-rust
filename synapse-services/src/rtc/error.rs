//! RTC domain error types for B3-5 error convergence.
//!
//! Defines `RtcError` — the unified error type for the RTC domain.
//! All RTC service methods return `Result<T, RtcError>`, which converts
//! to `ApiError` via `From<RtcError> for ApiError`.

use thiserror::Error;

use synapse_common::ApiError;

/// Unified RTC domain error.
#[derive(Debug, Error)]
pub enum RtcError {
    /// Database operation failed during RTC processing.
    #[error("database error: {0}")]
    Database(#[from] sqlx::Error),

    /// Call session not found.
    #[error("call session not found: {0}")]
    SessionNotFound(String),

    /// Call session already exists (conflict).
    #[error("call session already exists")]
    SessionAlreadyExists,

    /// Not authorized to perform the operation.
    #[error("not authorized for RTC operation")]
    NotAuthorized,

    /// Internal/ unexpected error.
    #[error("internal RTC error: {0}")]
    Internal(String),
}

impl From<RtcError> for ApiError {
    fn from(e: RtcError) -> Self {
        match e {
            RtcError::Database(source) => {
                // Preserve cause for diagnostics; maps to 500.
                ApiError::database_with_cause("rtc database error", source)
            }
            RtcError::SessionNotFound(msg) => ApiError::not_found(msg),
            RtcError::SessionAlreadyExists => ApiError::conflict("call session already exists"),
            RtcError::NotAuthorized => ApiError::forbidden("not authorized for RTC operation"),
            RtcError::Internal(msg) => ApiError::internal(msg),
        }
    }
}
