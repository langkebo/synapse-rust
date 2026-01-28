use axum::{
    http::StatusCode,
    response::{IntoResponse, Response},
    Json,
};
use serde::{Deserialize, Serialize};
use serde_json::json;
use std::sync::Arc;
use thiserror::Error;

#[derive(Debug, Error, Serialize, Deserialize)]
pub enum ApiError {
    #[error("Bad request: {0}")]
    BadRequest(String),

    #[error("Unauthorized")]
    Unauthorized,

    #[error("Forbidden")]
    Forbidden,

    #[error("Not found: {0}")]
    NotFound(String),

    #[error("Conflict: {0}")]
    Conflict(String),

    #[error("Rate limited")]
    RateLimited,

    #[error("Internal error: {0}")]
    Internal(String),

    #[error("Database error: {0}")]
    Database(String),

    #[error("Cache error: {0}")]
    Cache(String),

    #[error("Authentication error: {0}")]
    Authentication(String),

    #[error("Validation error: {0}")]
    Validation(String),

    #[error("Crypto error: {0}")]
    Crypto(String),
}

impl ApiError {
    pub fn bad_request(message: impl Into<String>) -> Self {
        Self::BadRequest(message.into())
    }

    pub fn unauthorized() -> Self {
        Self::Unauthorized
    }

    pub fn forbidden() -> Self {
        Self::Forbidden
    }

    pub fn not_found(message: impl Into<String>) -> Self {
        Self::NotFound(message.into())
    }

    pub fn conflict(message: impl Into<String>) -> Self {
        Self::Conflict(message.into())
    }

    pub fn internal(message: impl Into<String>) -> Self {
        Self::Internal(message.into())
    }

    pub fn database(message: impl Into<String>) -> Self {
        Self::Database(message.into())
    }

    pub fn cache(message: impl Into<String>) -> Self {
        Self::Cache(message.into())
    }

    pub fn authentication(message: impl Into<String>) -> Self {
        Self::Authentication(message.into())
    }

    pub fn validation(message: impl Into<String>) -> Self {
        Self::Validation(message.into())
    }

    pub fn crypto(message: impl Into<String>) -> Self {
        Self::Crypto(message.into())
    }
}

impl IntoResponse for ApiError {
    fn into_response(self) -> Response {
        let (status, errcode, error) = match self {
            ApiError::BadRequest(msg) => (StatusCode::BAD_REQUEST, "M_BAD_JSON", msg),
            ApiError::Unauthorized => (StatusCode::UNAUTHORIZED, "M_UNAUTHORIZED", "Unauthorized"),
            ApiError::Forbidden => (StatusCode::FORBIDDEN, "M_FORBIDDEN", "Forbidden"),
            ApiError::NotFound(msg) => (StatusCode::NOT_FOUND, "M_NOT_FOUND", msg),
            ApiError::Conflict(msg) => (StatusCode::CONFLICT, "M_USER_IN_USE", msg),
            ApiError::RateLimited => (
                StatusCode::TOO_MANY_REQUESTS,
                "M_LIMIT_EXCEEDED",
                "Rate limited",
            ),
            ApiError::Internal(msg) => (StatusCode::INTERNAL_SERVER_ERROR, "M_UNKNOWN", msg),
            ApiError::Database(msg) => (StatusCode::INTERNAL_SERVER_ERROR, "M_UNKNOWN", msg),
            ApiError::Cache(msg) => (StatusCode::INTERNAL_SERVER_ERROR, "M_UNKNOWN", msg),
            ApiError::Authentication(msg) => {
                (StatusCode::UNAUTHORIZED, "M_UNKNOWN_TOKEN", msg)
            }
            ApiError::Validation(msg) => (StatusCode::BAD_REQUEST, "M_INVALID_PARAM", msg),
            ApiError::Crypto(msg) => (StatusCode::INTERNAL_SERVER_ERROR, "M_UNKNOWN", msg),
        };

        let body = json!({
            "errcode": errcode,
            "error": error
        });

        (status, Json(body)).into_response()
    }
}

impl From<sqlx::Error> for ApiError {
    fn from(err: sqlx::Error) -> Self {
        ApiError::Database(err.to_string())
    }
}

impl From<redis::RedisError> for ApiError {
    fn from(err: redis::RedisError) -> Self {
        ApiError::Cache(err.to_string())
    }
}

impl From<jsonwebtoken::errors::Error> for ApiError {
    fn from(err: jsonwebtoken::errors::Error) -> Self {
        ApiError::Authentication(err.to_string())
    }
}

impl From<serde_json::Error> for ApiError {
    fn from(err: serde_json::Error) -> Self {
        ApiError::BadRequest(err.to_string())
    }
}

impl From<std::string::FromUtf8Error> for ApiError {
    fn from(_err: std::string::FromUtf8Error) -> Self {
        ApiError::Validation("Invalid UTF-8 encoding".to_string())
    }
}

impl From<std::num::ParseIntError> for ApiError {
    fn from(_err: std::num::ParseIntError) -> Self {
        ApiError::Validation("Invalid number format".to_string())
    }
}

impl From<std::io::Error> for ApiError {
    fn from(err: std::io::Error) -> Self {
        ApiError::Internal(err.to_string())
    }
}

pub type ApiResult<T> = Result<T, ApiError>;
