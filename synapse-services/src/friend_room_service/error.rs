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

impl From<ApiError> for FriendRoomError {
    fn from(e: ApiError) -> Self {
        FriendRoomError::Internal(e.to_string())
    }
}

/// 这两个 `From` impl 原来一条测试都没有（覆盖率 0%）。友链域的两个"冲突"
/// （已是好友 / 邀请已存在）走的是同一个 409，但 errcode 都是 `M_UNKNOWN`，
/// 客户端只能靠 message 区分 —— 这条语义值得钉住，否则以后有人把
/// `FriendshipConflict` 改成 400 也不会有任何测试变红。
#[cfg(test)]
mod tests {
    use super::*;
    use axum::http::StatusCode;

    fn status_and_code(e: FriendRoomError) -> (StatusCode, &'static str) {
        let api: ApiError = e.into();
        (api.http_status(), api.code.as_str())
    }

    #[test]
    fn every_variant_maps_to_its_documented_http_semantics() {
        assert_eq!(
            status_and_code(FriendRoomError::NotFound("!fr:example.com".into())),
            (StatusCode::NOT_FOUND, "M_NOT_FOUND")
        );
        assert_eq!(
            status_and_code(FriendRoomError::FriendshipConflict("already friends".into())),
            (StatusCode::CONFLICT, "M_UNKNOWN")
        );
        assert_eq!(
            status_and_code(FriendRoomError::RequestExists("pending".into())),
            (StatusCode::CONFLICT, "M_UNKNOWN")
        );
        assert_eq!(
            status_and_code(FriendRoomError::NotAuthorized("not yours".into())),
            (StatusCode::FORBIDDEN, "M_FORBIDDEN")
        );
        assert_eq!(
            status_and_code(FriendRoomError::InvalidInput("bad".into())),
            (StatusCode::BAD_REQUEST, "M_BAD_JSON")
        );
        assert_eq!(
            status_and_code(FriendRoomError::Internal("boom".into())),
            (StatusCode::INTERNAL_SERVER_ERROR, "M_UNKNOWN")
        );
    }

    #[test]
    fn database_error_keeps_its_cause_and_is_5xx() {
        let api: ApiError = FriendRoomError::Database(sqlx::Error::RowNotFound).into();
        assert_eq!(api.http_status(), StatusCode::INTERNAL_SERVER_ERROR);
        assert!(api.cause.is_some(), "数据库错误必须保留 source");
    }

    #[test]
    fn api_error_round_trips_back_as_internal_with_original_text() {
        let original = ApiError::forbidden("nope");
        let back: FriendRoomError = original.into();
        assert!(matches!(back, FriendRoomError::Internal(_)));
        assert!(back.to_string().contains("nope"), "原文必须保留，实际：{back}");
    }
}
