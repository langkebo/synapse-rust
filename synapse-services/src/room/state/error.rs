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
            RoomStateError::Database(source) => ApiError::database_with_cause("room state database error", source),
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

/// 这两个 `From` impl 原来一条测试都没有（覆盖率 0%）。它们决定**客户端看到的
/// HTTP 状态码与 errcode**，错一处就是"该 404 的报 500"或"该 403 的报 400"，
/// 属于对外契约而非内部实现细节，因此逐变体钉住。
#[cfg(test)]
mod tests {
    use super::*;
    use axum::http::StatusCode;

    fn status_and_code(e: RoomStateError) -> (StatusCode, &'static str) {
        let api: ApiError = e.into();
        (api.http_status(), api.code.as_str())
    }

    #[test]
    fn every_variant_maps_to_its_documented_http_semantics() {
        assert_eq!(
            status_and_code(RoomStateError::NotFound("!r:example.com".into())),
            (StatusCode::NOT_FOUND, "M_NOT_FOUND")
        );
        assert_eq!(
            status_and_code(RoomStateError::AlreadyExists("!r:example.com".into())),
            (StatusCode::CONFLICT, "M_UNKNOWN")
        );
        assert_eq!(
            status_and_code(RoomStateError::NotAuthorized("not in room".into())),
            (StatusCode::FORBIDDEN, "M_FORBIDDEN")
        );
        assert_eq!(
            status_and_code(RoomStateError::InvalidInput("bad state_key".into())),
            (StatusCode::BAD_REQUEST, "M_BAD_JSON")
        );
        assert_eq!(
            status_and_code(RoomStateError::Internal("boom".into())),
            (StatusCode::INTERNAL_SERVER_ERROR, "M_UNKNOWN")
        );
    }

    /// 数据库错误的 5xx 之外还必须保留 cause —— 线上排障只靠 message 会退化成
    /// 一句 "room state database error"，看不到真正的 SQL 失败原因。
    #[test]
    fn database_error_keeps_its_cause_and_is_5xx() {
        let api: ApiError = RoomStateError::Database(sqlx::Error::RowNotFound).into();
        assert_eq!(api.http_status(), StatusCode::INTERNAL_SERVER_ERROR);
        assert!(api.cause.is_some(), "数据库错误必须保留 source");
    }

    /// 反方向：`ApiError` 落回领域错误时统一压成 `Internal`，但原文不得丢
    /// （否则上层日志里"为什么失败"就没了）。
    #[test]
    fn api_error_round_trips_back_as_internal_with_original_text() {
        let original = ApiError::not_found("room not found");
        let back: RoomStateError = original.into();
        assert!(matches!(back, RoomStateError::Internal(_)), "反方向必须压成 Internal");
        assert!(back.to_string().contains("room not found"), "原文必须保留，实际：{back}");
    }
}
