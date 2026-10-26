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

/// 这条 `From` impl 原来没有测试（覆盖率 0%）。RTC 的错误码是客户端
/// "要不要重试 / 要不要重新建通话"的唯一依据，逐变体钉住。
#[cfg(test)]
mod tests {
    use super::*;
    use axum::http::StatusCode;

    fn status_and_code(e: RtcError) -> (StatusCode, &'static str) {
        let api: ApiError = e.into();
        (api.http_status(), api.code.as_str())
    }

    #[test]
    fn every_variant_maps_to_its_documented_http_semantics() {
        assert_eq!(status_and_code(RtcError::SessionNotFound("call-1".into())), (StatusCode::NOT_FOUND, "M_NOT_FOUND"));
        assert_eq!(status_and_code(RtcError::SessionAlreadyExists), (StatusCode::CONFLICT, "M_UNKNOWN"));
        assert_eq!(status_and_code(RtcError::NotAuthorized), (StatusCode::FORBIDDEN, "M_FORBIDDEN"));
        assert_eq!(
            status_and_code(RtcError::Internal("boom".into())),
            (StatusCode::INTERNAL_SERVER_ERROR, "M_UNKNOWN")
        );
    }

    #[test]
    fn database_error_keeps_its_cause_and_is_5xx() {
        let api: ApiError = RtcError::Database(sqlx::Error::RowNotFound).into();
        assert_eq!(api.http_status(), StatusCode::INTERNAL_SERVER_ERROR);
        assert!(api.cause.is_some(), "数据库错误必须保留 source");
    }
}
