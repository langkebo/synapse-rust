use axum::{
    http::StatusCode,
    response::{IntoResponse, Response},
    Json,
};
use serde::{Deserialize, Serialize};
use serde_json::json;
use thiserror::Error;

#[derive(Debug, Error, Serialize, Deserialize, Clone, PartialEq)]
pub enum ApiError {
    #[error("Bad request: {0}")]
    BadRequest(String),

    #[error("Unauthorized: {0}")]
    Unauthorized(String),

    #[error("Forbidden: {0}")]
    Forbidden(String),

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

    #[error("Invalid input: {0}")]
    InvalidInput(String),

    #[error("Decryption error: {0}")]
    DecryptionError(String),

    #[error("Encryption error: {0}")]
    EncryptionError(String),

    #[error("Crypto error: {0}")]
    Crypto(String),
}

impl ApiError {
    pub fn bad_request(message: impl Into<String>) -> Self {
        Self::BadRequest(message.into())
    }

    pub fn unauthorized(message: impl Into<String>) -> Self {
        Self::Unauthorized(message.into())
    }

    pub fn forbidden(message: impl Into<String>) -> Self {
        Self::Forbidden(message.into())
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

    pub fn invalid_input(message: impl Into<String>) -> Self {
        Self::InvalidInput(message.into())
    }

    pub fn crypto(message: impl Into<String>) -> Self {
        Self::Crypto(message.into())
    }

    pub fn code(&self) -> &str {
        match self {
            ApiError::BadRequest(_) => "M_BAD_JSON",
            ApiError::Unauthorized(_) => "M_UNAUTHORIZED",
            ApiError::Forbidden(_) => "M_FORBIDDEN",
            ApiError::NotFound(_) => "M_NOT_FOUND",
            ApiError::Conflict(_) => "M_USER_IN_USE",
            ApiError::RateLimited => "M_LIMIT_EXCEEDED",
            ApiError::Internal(_) => "M_INTERNAL_ERROR",
            ApiError::Database(_) => "M_DB_ERROR",
            ApiError::Cache(_) => "M_CACHE_ERROR",
            ApiError::Authentication(_) => "M_AUTH_FAILED",
            ApiError::Validation(_) => "M_VALIDATION_FAILED",
            ApiError::InvalidInput(_) => "M_INVALID_INPUT",
            ApiError::DecryptionError(_) => "M_DECRYPTION_FAILED",
            ApiError::EncryptionError(_) => "M_ENCRYPTION_FAILED",
            ApiError::Crypto(_) => "M_CRYPTO_ERROR",
        }
    }

    fn to_server_error_response(
        log_msg: &str,
        user_msg: &str,
    ) -> (StatusCode, &'static str, String) {
        tracing::error!("{}", log_msg);
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            "M_UNKNOWN",
            user_msg.to_string(),
        )
    }

    pub fn message(&self) -> String {
        self.to_string()
    }
}

impl IntoResponse for ApiError {
    fn into_response(self) -> Response {
        let (status, errcode, error) = match self {
            ApiError::BadRequest(msg) => (StatusCode::BAD_REQUEST, "M_BAD_JSON", msg),
            ApiError::Unauthorized(msg) => (StatusCode::UNAUTHORIZED, "M_UNAUTHORIZED", msg),
            ApiError::Forbidden(msg) => (StatusCode::FORBIDDEN, "M_FORBIDDEN", msg),
            ApiError::NotFound(msg) => (StatusCode::NOT_FOUND, "M_NOT_FOUND", msg),
            ApiError::Conflict(msg) => (StatusCode::CONFLICT, "M_USER_IN_USE", msg),
            ApiError::RateLimited => (
                StatusCode::TOO_MANY_REQUESTS,
                "M_LIMIT_EXCEEDED",
                "Rate limited".to_string(),
            ),
            ApiError::Internal(msg) => Self::to_server_error_response(
                &format!("Internal error: {}", msg),
                "Internal server error",
            ),
            ApiError::Database(msg) => Self::to_server_error_response(
                &format!("Database error: {}", msg),
                "Database operation failed",
            ),
            ApiError::Cache(msg) => Self::to_server_error_response(
                &format!("Cache error: {}", msg),
                "Cache operation failed",
            ),
            ApiError::Authentication(msg) => (StatusCode::UNAUTHORIZED, "M_UNKNOWN_TOKEN", msg),
            ApiError::Validation(msg) => (StatusCode::BAD_REQUEST, "M_INVALID_PARAM", msg),
            ApiError::InvalidInput(msg) => (StatusCode::BAD_REQUEST, "M_BAD_JSON", msg),
            ApiError::DecryptionError(msg) => (StatusCode::UNAUTHORIZED, "M_UNKNOWN", msg),
            ApiError::EncryptionError(msg) => Self::to_server_error_response(
                &format!("Encryption error: {}", msg),
                "Encryption operation failed",
            ),
            ApiError::Crypto(msg) => Self::to_server_error_response(
                &format!("Crypto error: {}", msg),
                "Cryptographic operation failed",
            ),
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
        let error_msg = format!("{:?}", err).to_lowercase();
        if error_msg.contains("duplicate key")
            || error_msg.contains("unique constraint")
            || error_msg.contains("23505")
            || error_msg.contains("violates unique constraint")
        {
            ApiError::BadRequest(format!("Duplicate entry: {}", err))
        } else {
            ApiError::Database(err.to_string())
        }
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

impl From<ed25519_dalek::ed25519::Error> for ApiError {
    fn from(err: ed25519_dalek::ed25519::Error) -> Self {
        ApiError::Crypto(format!("Ed25519 error: {}", err))
    }
}

impl From<crate::e2ee::crypto::CryptoError> for ApiError {
    fn from(err: crate::e2ee::crypto::CryptoError) -> Self {
        match err {
            crate::e2ee::crypto::CryptoError::EncryptionError(msg) => {
                ApiError::EncryptionError(msg)
            }
            crate::e2ee::crypto::CryptoError::DecryptionError(msg) => {
                ApiError::DecryptionError(msg)
            }
            _ => ApiError::Crypto(err.to_string()),
        }
    }
}

pub type ApiResult<T> = Result<T, ApiError>;

#[derive(Debug)]
pub struct ApiResponse<T>(pub Result<T, ApiError>);

impl<T> ApiResponse<T> {
    pub fn into_result(self) -> Result<T, ApiError> {
        self.0
    }
}

impl<T> From<ApiResponse<T>> for Response
where
    T: IntoResponse,
{
    fn from(resp: ApiResponse<T>) -> Self {
        match resp.0 {
            Ok(value) => value.into_response(),
            Err(err) => err.into_response(),
        }
    }
}

impl<T> From<Result<T, ApiError>> for ApiResponse<T> {
    fn from(result: Result<T, ApiError>) -> Self {
        ApiResponse(result)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::http::StatusCode;

    #[test]
    fn test_api_error_variants() {
        let errors = vec![
            ApiError::BadRequest("test".to_string()),
            ApiError::Unauthorized("unauthorized".to_string()),
            ApiError::Forbidden("forbidden".to_string()),
            ApiError::NotFound("not found".to_string()),
            ApiError::Conflict("conflict".to_string()),
            ApiError::RateLimited,
            ApiError::Internal("internal".to_string()),
            ApiError::Database("db error".to_string()),
            ApiError::Cache("cache error".to_string()),
            ApiError::Authentication("auth error".to_string()),
            ApiError::Validation("validation error".to_string()),
            ApiError::InvalidInput("invalid input".to_string()),
            ApiError::DecryptionError("decrypt error".to_string()),
            ApiError::EncryptionError("encrypt error".to_string()),
            ApiError::Crypto("crypto error".to_string()),
        ];

        for error in errors {
            let _ = format!("{:?}", error);
        }
    }

    #[test]
    fn test_api_error_factory_methods() {
        assert!(matches!(
            ApiError::bad_request("test"),
            ApiError::BadRequest(_)
        ));
        assert!(matches!(
            ApiError::unauthorized("unauthorized"),
            ApiError::Unauthorized(_)
        ));
        assert!(matches!(
            ApiError::forbidden("test"),
            ApiError::Forbidden(_)
        ));
        assert!(matches!(ApiError::not_found("test"), ApiError::NotFound(_)));
        assert!(matches!(ApiError::conflict("test"), ApiError::Conflict(_)));
        assert!(matches!(ApiError::internal("test"), ApiError::Internal(_)));
        assert!(matches!(ApiError::database("test"), ApiError::Database(_)));
        assert!(matches!(ApiError::cache("test"), ApiError::Cache(_)));
        assert!(matches!(
            ApiError::authentication("test"),
            ApiError::Authentication(_)
        ));
        assert!(matches!(
            ApiError::validation("test"),
            ApiError::Validation(_)
        ));
        assert!(matches!(
            ApiError::invalid_input("test"),
            ApiError::InvalidInput(_)
        ));
        assert!(matches!(ApiError::crypto("test"), ApiError::Crypto(_)));
    }

    #[test]
    fn test_into_response_bad_request() {
        let error = ApiError::bad_request("invalid json");
        let response = error.into_response();
        assert_eq!(response.status(), StatusCode::BAD_REQUEST);
    }

    #[test]
    fn test_into_response_unauthorized() {
        let error = ApiError::unauthorized("unauthorized");
        let response = error.into_response();
        assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
    }

    #[test]
    fn test_into_response_forbidden() {
        let error = ApiError::forbidden("access denied");
        let response = error.into_response();
        assert_eq!(response.status(), StatusCode::FORBIDDEN);
    }

    #[test]
    fn test_into_response_not_found() {
        let error = ApiError::not_found("resource not found");
        let response = error.into_response();
        assert_eq!(response.status(), StatusCode::NOT_FOUND);
    }

    #[test]
    fn test_into_response_conflict() {
        let error = ApiError::conflict("user already exists");
        let response = error.into_response();
        assert_eq!(response.status(), StatusCode::CONFLICT);
    }

    #[test]
    fn test_into_response_rate_limited() {
        let error = ApiError::RateLimited;
        let response = error.into_response();
        assert_eq!(response.status(), StatusCode::TOO_MANY_REQUESTS);
    }

    #[test]
    fn test_into_response_internal() {
        let error = ApiError::internal("internal server error");
        let response = error.into_response();
        assert_eq!(response.status(), StatusCode::INTERNAL_SERVER_ERROR);
    }

    #[test]
    fn test_from_sqlx_error() {
        let sqlx_error = sqlx::Error::RowNotFound;
        let api_error: ApiError = sqlx_error.into();
        assert!(matches!(api_error, ApiError::Database(_)));
    }

    #[test]
    fn test_from_redis_error() {
        let redis_error = redis::RedisError::from((redis::ErrorKind::InvalidClientConfig, "test"));
        let api_error: ApiError = redis_error.into();
        assert!(matches!(api_error, ApiError::Cache(_)));
    }

    #[test]
    fn test_from_jsonwebtoken_error() {
        let jwt_error =
            jsonwebtoken::errors::Error::from(jsonwebtoken::errors::ErrorKind::InvalidToken);
        let api_error: ApiError = jwt_error.into();
        assert!(matches!(api_error, ApiError::Authentication(_)));
    }

    #[test]
    fn test_from_serde_json_error() {
        let json_error = serde_json::from_str::<serde_json::Value>("invalid json").unwrap_err();
        let api_error: ApiError = json_error.into();
        assert!(matches!(api_error, ApiError::BadRequest(_)));
    }

    #[test]
    fn test_from_utf8_error() {
        let invalid_utf8 = vec![0xFF, 0xFE, 0xFD];
        let utf8_error = String::from_utf8(invalid_utf8).unwrap_err();
        let api_error: ApiError = utf8_error.into();
        assert!(matches!(api_error, ApiError::Validation(_)));
    }

    #[test]
    fn test_from_parse_int_error() {
        let parse_error = "abc".parse::<i32>().unwrap_err();
        let api_error: ApiError = parse_error.into();
        assert!(matches!(api_error, ApiError::Validation(_)));
    }

    #[test]
    fn test_from_io_error() {
        let io_error = std::io::Error::new(std::io::ErrorKind::NotFound, "file not found");
        let api_error: ApiError = io_error.into();
        assert!(matches!(api_error, ApiError::Internal(_)));
    }

    #[test]
    fn test_api_result_alias() {
        let success: ApiResult<String> = Ok("test".to_string());
        let failure: ApiResult<String> = Err(ApiError::not_found("not found"));
        assert!(success.is_ok());
        assert!(failure.is_err());
    }
}
