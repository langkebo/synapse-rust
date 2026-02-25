use axum::{
    http::StatusCode,
    response::{IntoResponse, Response},
    Json,
};
use serde::{Deserialize, Serialize};
use serde_json::json;
use thiserror::Error;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MatrixErrorCode {
    Forbidden,
    UnknownToken,
    MissingToken,
    BadJson,
    NotJson,
    NotFound,
    LimitExceeded,
    Unknown,
    Unrecognized,
    Unauthorized,
    UserDeactivated,
    UserInUse,
    InvalidUsername,
    RoomInUse,
    InvalidRoomState,
    ThreepidInUse,
    ThreepidNotFound,
    ThreepidAuthFailed,
    ThreepidDenied,
    ServerNotTrusted,
    UnsupportedRoomVersion,
    IncompatibleRoomVersion,
    BadState,
    GuestAccessForbidden,
    CaptchaNeeded,
    CaptchaInvalid,
    MissingParam,
    InvalidParam,
    TooLarge,
    Exclusive,
    ResourceLimitExceeded,
    CannotLeaveServerNoticeRoom,
}

impl MatrixErrorCode {
    pub fn as_str(&self) -> &'static str {
        match self {
            MatrixErrorCode::Forbidden => "M_FORBIDDEN",
            MatrixErrorCode::UnknownToken => "M_UNKNOWN_TOKEN",
            MatrixErrorCode::MissingToken => "M_MISSING_TOKEN",
            MatrixErrorCode::BadJson => "M_BAD_JSON",
            MatrixErrorCode::NotJson => "M_NOT_JSON",
            MatrixErrorCode::NotFound => "M_NOT_FOUND",
            MatrixErrorCode::LimitExceeded => "M_LIMIT_EXCEEDED",
            MatrixErrorCode::Unknown => "M_UNKNOWN",
            MatrixErrorCode::Unrecognized => "M_UNRECOGNIZED",
            MatrixErrorCode::Unauthorized => "M_UNAUTHORIZED",
            MatrixErrorCode::UserDeactivated => "M_USER_DEACTIVATED",
            MatrixErrorCode::UserInUse => "M_USER_IN_USE",
            MatrixErrorCode::InvalidUsername => "M_INVALID_USERNAME",
            MatrixErrorCode::RoomInUse => "M_ROOM_IN_USE",
            MatrixErrorCode::InvalidRoomState => "M_INVALID_ROOM_STATE",
            MatrixErrorCode::ThreepidInUse => "M_THREEPID_IN_USE",
            MatrixErrorCode::ThreepidNotFound => "M_THREEPID_NOT_FOUND",
            MatrixErrorCode::ThreepidAuthFailed => "M_THREEPID_AUTH_FAILED",
            MatrixErrorCode::ThreepidDenied => "M_THREEPID_DENIED",
            MatrixErrorCode::ServerNotTrusted => "M_SERVER_NOT_TRUSTED",
            MatrixErrorCode::UnsupportedRoomVersion => "M_UNSUPPORTED_ROOM_VERSION",
            MatrixErrorCode::IncompatibleRoomVersion => "M_INCOMPATIBLE_ROOM_VERSION",
            MatrixErrorCode::BadState => "M_BAD_STATE",
            MatrixErrorCode::GuestAccessForbidden => "M_GUEST_ACCESS_FORBIDDEN",
            MatrixErrorCode::CaptchaNeeded => "M_CAPTCHA_NEEDED",
            MatrixErrorCode::CaptchaInvalid => "M_CAPTCHA_INVALID",
            MatrixErrorCode::MissingParam => "M_MISSING_PARAM",
            MatrixErrorCode::InvalidParam => "M_INVALID_PARAM",
            MatrixErrorCode::TooLarge => "M_TOO_LARGE",
            MatrixErrorCode::Exclusive => "M_EXCLUSIVE",
            MatrixErrorCode::ResourceLimitExceeded => "M_RESOURCE_LIMIT_EXCEEDED",
            MatrixErrorCode::CannotLeaveServerNoticeRoom => "M_CANNOT_LEAVE_SERVER_NOTICE_ROOM",
        }
    }

    pub fn http_status(&self) -> StatusCode {
        match self {
            MatrixErrorCode::Forbidden => StatusCode::FORBIDDEN,
            MatrixErrorCode::UnknownToken => StatusCode::UNAUTHORIZED,
            MatrixErrorCode::MissingToken => StatusCode::UNAUTHORIZED,
            MatrixErrorCode::BadJson => StatusCode::BAD_REQUEST,
            MatrixErrorCode::NotJson => StatusCode::BAD_REQUEST,
            MatrixErrorCode::NotFound => StatusCode::NOT_FOUND,
            MatrixErrorCode::LimitExceeded => StatusCode::TOO_MANY_REQUESTS,
            MatrixErrorCode::Unknown => StatusCode::INTERNAL_SERVER_ERROR,
            MatrixErrorCode::Unrecognized => StatusCode::BAD_REQUEST,
            MatrixErrorCode::Unauthorized => StatusCode::UNAUTHORIZED,
            MatrixErrorCode::UserDeactivated => StatusCode::FORBIDDEN,
            MatrixErrorCode::UserInUse => StatusCode::CONFLICT,
            MatrixErrorCode::InvalidUsername => StatusCode::BAD_REQUEST,
            MatrixErrorCode::RoomInUse => StatusCode::CONFLICT,
            MatrixErrorCode::InvalidRoomState => StatusCode::BAD_REQUEST,
            MatrixErrorCode::ThreepidInUse => StatusCode::CONFLICT,
            MatrixErrorCode::ThreepidNotFound => StatusCode::BAD_REQUEST,
            MatrixErrorCode::ThreepidAuthFailed => StatusCode::FORBIDDEN,
            MatrixErrorCode::ThreepidDenied => StatusCode::FORBIDDEN,
            MatrixErrorCode::ServerNotTrusted => StatusCode::BAD_GATEWAY,
            MatrixErrorCode::UnsupportedRoomVersion => StatusCode::BAD_REQUEST,
            MatrixErrorCode::IncompatibleRoomVersion => StatusCode::BAD_REQUEST,
            MatrixErrorCode::BadState => StatusCode::BAD_REQUEST,
            MatrixErrorCode::GuestAccessForbidden => StatusCode::FORBIDDEN,
            MatrixErrorCode::CaptchaNeeded => StatusCode::BAD_REQUEST,
            MatrixErrorCode::CaptchaInvalid => StatusCode::BAD_REQUEST,
            MatrixErrorCode::MissingParam => StatusCode::BAD_REQUEST,
            MatrixErrorCode::InvalidParam => StatusCode::BAD_REQUEST,
            MatrixErrorCode::TooLarge => StatusCode::PAYLOAD_TOO_LARGE,
            MatrixErrorCode::Exclusive => StatusCode::CONFLICT,
            MatrixErrorCode::ResourceLimitExceeded => StatusCode::FORBIDDEN,
            MatrixErrorCode::CannotLeaveServerNoticeRoom => StatusCode::FORBIDDEN,
        }
    }
}

impl std::fmt::Display for MatrixErrorCode {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.as_str())
    }
}

impl Serialize for MatrixErrorCode {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        serializer.serialize_str(self.as_str())
    }
}

impl<'de> Deserialize<'de> for MatrixErrorCode {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        let s = String::deserialize(deserializer)?;
        match s.as_str() {
            "M_FORBIDDEN" => Ok(MatrixErrorCode::Forbidden),
            "M_UNKNOWN_TOKEN" => Ok(MatrixErrorCode::UnknownToken),
            "M_MISSING_TOKEN" => Ok(MatrixErrorCode::MissingToken),
            "M_BAD_JSON" => Ok(MatrixErrorCode::BadJson),
            "M_NOT_JSON" => Ok(MatrixErrorCode::NotJson),
            "M_NOT_FOUND" => Ok(MatrixErrorCode::NotFound),
            "M_LIMIT_EXCEEDED" => Ok(MatrixErrorCode::LimitExceeded),
            "M_UNKNOWN" => Ok(MatrixErrorCode::Unknown),
            "M_UNRECOGNIZED" => Ok(MatrixErrorCode::Unrecognized),
            "M_UNAUTHORIZED" => Ok(MatrixErrorCode::Unauthorized),
            "M_USER_DEACTIVATED" => Ok(MatrixErrorCode::UserDeactivated),
            "M_USER_IN_USE" => Ok(MatrixErrorCode::UserInUse),
            "M_INVALID_USERNAME" => Ok(MatrixErrorCode::InvalidUsername),
            "M_ROOM_IN_USE" => Ok(MatrixErrorCode::RoomInUse),
            "M_INVALID_ROOM_STATE" => Ok(MatrixErrorCode::InvalidRoomState),
            "M_THREEPID_IN_USE" => Ok(MatrixErrorCode::ThreepidInUse),
            "M_THREEPID_NOT_FOUND" => Ok(MatrixErrorCode::ThreepidNotFound),
            "M_THREEPID_AUTH_FAILED" => Ok(MatrixErrorCode::ThreepidAuthFailed),
            "M_THREEPID_DENIED" => Ok(MatrixErrorCode::ThreepidDenied),
            "M_SERVER_NOT_TRUSTED" => Ok(MatrixErrorCode::ServerNotTrusted),
            "M_UNSUPPORTED_ROOM_VERSION" => Ok(MatrixErrorCode::UnsupportedRoomVersion),
            "M_INCOMPATIBLE_ROOM_VERSION" => Ok(MatrixErrorCode::IncompatibleRoomVersion),
            "M_BAD_STATE" => Ok(MatrixErrorCode::BadState),
            "M_GUEST_ACCESS_FORBIDDEN" => Ok(MatrixErrorCode::GuestAccessForbidden),
            "M_CAPTCHA_NEEDED" => Ok(MatrixErrorCode::CaptchaNeeded),
            "M_CAPTCHA_INVALID" => Ok(MatrixErrorCode::CaptchaInvalid),
            "M_MISSING_PARAM" => Ok(MatrixErrorCode::MissingParam),
            "M_INVALID_PARAM" => Ok(MatrixErrorCode::InvalidParam),
            "M_TOO_LARGE" => Ok(MatrixErrorCode::TooLarge),
            "M_EXCLUSIVE" => Ok(MatrixErrorCode::Exclusive),
            "M_RESOURCE_LIMIT_EXCEEDED" => Ok(MatrixErrorCode::ResourceLimitExceeded),
            "M_CANNOT_LEAVE_SERVER_NOTICE_ROOM" => Ok(MatrixErrorCode::CannotLeaveServerNoticeRoom),
            _ => Err(serde::de::Error::unknown_variant(
                &s,
                &[
                    "M_FORBIDDEN",
                    "M_UNKNOWN_TOKEN",
                    "M_MISSING_TOKEN",
                    "M_BAD_JSON",
                    "M_NOT_JSON",
                    "M_NOT_FOUND",
                    "M_LIMIT_EXCEEDED",
                    "M_UNKNOWN",
                    "M_UNRECOGNIZED",
                    "M_UNAUTHORIZED",
                    "M_USER_DEACTIVATED",
                    "M_USER_IN_USE",
                    "M_INVALID_USERNAME",
                    "M_ROOM_IN_USE",
                    "M_INVALID_ROOM_STATE",
                    "M_THREEPID_IN_USE",
                    "M_THREEPID_NOT_FOUND",
                    "M_THREEPID_AUTH_FAILED",
                    "M_THREEPID_DENIED",
                    "M_SERVER_NOT_TRUSTED",
                    "M_UNSUPPORTED_ROOM_VERSION",
                    "M_INCOMPATIBLE_ROOM_VERSION",
                    "M_BAD_STATE",
                    "M_GUEST_ACCESS_FORBIDDEN",
                    "M_CAPTCHA_NEEDED",
                    "M_CAPTCHA_INVALID",
                    "M_MISSING_PARAM",
                    "M_INVALID_PARAM",
                    "M_TOO_LARGE",
                    "M_EXCLUSIVE",
                    "M_RESOURCE_LIMIT_EXCEEDED",
                    "M_CANNOT_LEAVE_SERVER_NOTICE_ROOM",
                ],
            )),
        }
    }
}

#[derive(Debug, Serialize, Deserialize)]
pub struct ApiResponse<T> {
    pub status: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub data: Option<T>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub errcode: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub retry_after_ms: Option<u64>,
}

impl<T> ApiResponse<T> {
    pub fn success(data: T) -> Self {
        Self {
            status: "ok".to_string(),
            data: Some(data),
            error: None,
            errcode: None,
            retry_after_ms: None,
        }
    }

    pub fn error(error: String, errcode: String) -> Self {
        Self {
            status: "error".to_string(),
            data: None,
            error: Some(error),
            errcode: Some(errcode),
            retry_after_ms: None,
        }
    }

    pub fn error_with_retry(error: String, errcode: String, retry_after_ms: u64) -> Self {
        Self {
            status: "error".to_string(),
            data: None,
            error: Some(error),
            errcode: Some(errcode),
            retry_after_ms: Some(retry_after_ms),
        }
    }
}

impl<T> IntoResponse for ApiResponse<T>
where
    T: Serialize,
{
    fn into_response(self) -> Response {
        let status_code = if self.status == "ok" {
            StatusCode::OK
        } else {
            match self.errcode.as_deref() {
                Some("M_NOT_FOUND") => StatusCode::NOT_FOUND,
                Some("M_FORBIDDEN") => StatusCode::FORBIDDEN,
                Some("M_UNAUTHORIZED") | Some("M_UNKNOWN_TOKEN") | Some("M_MISSING_TOKEN") => {
                    StatusCode::UNAUTHORIZED
                }
                Some("M_LIMIT_EXCEEDED") => StatusCode::TOO_MANY_REQUESTS,
                Some("M_BAD_JSON")
                | Some("M_NOT_JSON")
                | Some("M_INVALID_PARAM")
                | Some("M_MISSING_PARAM")
                | Some("M_INVALID_USERNAME")
                | Some("M_BAD_STATE")
                | Some("M_INVALID_ROOM_STATE")
                | Some("M_UNRECOGNIZED") => StatusCode::BAD_REQUEST,
                Some("M_USER_IN_USE") | Some("M_ROOM_IN_USE") | Some("M_THREEPID_IN_USE") => {
                    StatusCode::CONFLICT
                }
                Some("M_TOO_LARGE") => StatusCode::PAYLOAD_TOO_LARGE,
                Some("M_SERVER_NOT_TRUSTED") => StatusCode::BAD_GATEWAY,
                _ => StatusCode::INTERNAL_SERVER_ERROR,
            }
        };
        (status_code, Json(self)).into_response()
    }
}

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

    #[error("Gone: {0}")]
    Gone(String),

    #[error("Missing token")]
    MissingToken,

    #[error("Not JSON: {0}")]
    NotJson(String),

    #[error("User deactivated: {0}")]
    UserDeactivated(String),

    #[error("Invalid username: {0}")]
    InvalidUsername(String),

    #[error("Room in use: {0}")]
    RoomInUse(String),

    #[error("Invalid room state: {0}")]
    InvalidRoomState(String),

    #[error("Threepid in use: {0}")]
    ThreepidInUse(String),

    #[error("Threepid not found: {0}")]
    ThreepidNotFound(String),

    #[error("Threepid auth failed: {0}")]
    ThreepidAuthFailed(String),

    #[error("Threepid denied: {0}")]
    ThreepidDenied(String),

    #[error("Server not trusted: {0}")]
    ServerNotTrusted(String),

    #[error("Unsupported room version: {0}")]
    UnsupportedRoomVersion(String),

    #[error("Incompatible room version: {0}")]
    IncompatibleRoomVersion(String),

    #[error("Bad state: {0}")]
    BadState(String),

    #[error("Guest access forbidden: {0}")]
    GuestAccessForbidden(String),

    #[error("Captcha needed: {0}")]
    CaptchaNeeded(String),

    #[error("Captcha invalid: {0}")]
    CaptchaInvalid(String),

    #[error("Missing parameter: {0}")]
    MissingParam(String),

    #[error("Too large: {0}")]
    TooLarge(String),

    #[error("Exclusive: {0}")]
    Exclusive(String),

    #[error("Resource limit exceeded: {0}")]
    ResourceLimitExceeded(String),

    #[error("Cannot leave server notice room: {0}")]
    CannotLeaveServerNoticeRoom(String),

    #[error("Unknown error: {0}")]
    Unknown(String),

    #[error("Unrecognized: {0}")]
    Unrecognized(String),
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

    pub fn gone(message: impl Into<String>) -> Self {
        Self::Gone(message.into())
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

    pub fn rate_limited(_message: impl Into<String>) -> Self {
        Self::RateLimited
    }

    pub fn missing_token() -> Self {
        Self::MissingToken
    }

    pub fn not_json(message: impl Into<String>) -> Self {
        Self::NotJson(message.into())
    }

    pub fn user_deactivated(message: impl Into<String>) -> Self {
        Self::UserDeactivated(message.into())
    }

    pub fn invalid_username(message: impl Into<String>) -> Self {
        Self::InvalidUsername(message.into())
    }

    pub fn user_in_use(message: impl Into<String>) -> Self {
        Self::Conflict(message.into())
    }

    pub fn room_in_use(message: impl Into<String>) -> Self {
        Self::RoomInUse(message.into())
    }

    pub fn invalid_room_state(message: impl Into<String>) -> Self {
        Self::InvalidRoomState(message.into())
    }

    pub fn threepid_in_use(message: impl Into<String>) -> Self {
        Self::ThreepidInUse(message.into())
    }

    pub fn threepid_not_found(message: impl Into<String>) -> Self {
        Self::ThreepidNotFound(message.into())
    }

    pub fn threepid_auth_failed(message: impl Into<String>) -> Self {
        Self::ThreepidAuthFailed(message.into())
    }

    pub fn threepid_denied(message: impl Into<String>) -> Self {
        Self::ThreepidDenied(message.into())
    }

    pub fn server_not_trusted(message: impl Into<String>) -> Self {
        Self::ServerNotTrusted(message.into())
    }

    pub fn unsupported_room_version(message: impl Into<String>) -> Self {
        Self::UnsupportedRoomVersion(message.into())
    }

    pub fn incompatible_room_version(message: impl Into<String>) -> Self {
        Self::IncompatibleRoomVersion(message.into())
    }

    pub fn bad_state(message: impl Into<String>) -> Self {
        Self::BadState(message.into())
    }

    pub fn guest_access_forbidden(message: impl Into<String>) -> Self {
        Self::GuestAccessForbidden(message.into())
    }

    pub fn captcha_needed(message: impl Into<String>) -> Self {
        Self::CaptchaNeeded(message.into())
    }

    pub fn captcha_invalid(message: impl Into<String>) -> Self {
        Self::CaptchaInvalid(message.into())
    }

    pub fn missing_param(message: impl Into<String>) -> Self {
        Self::MissingParam(message.into())
    }

    pub fn too_large(message: impl Into<String>) -> Self {
        Self::TooLarge(message.into())
    }

    pub fn exclusive(message: impl Into<String>) -> Self {
        Self::Exclusive(message.into())
    }

    pub fn resource_limit_exceeded(message: impl Into<String>) -> Self {
        Self::ResourceLimitExceeded(message.into())
    }

    pub fn cannot_leave_server_notice_room(message: impl Into<String>) -> Self {
        Self::CannotLeaveServerNoticeRoom(message.into())
    }

    pub fn unknown(message: impl Into<String>) -> Self {
        Self::Unknown(message.into())
    }

    pub fn unrecognized(message: impl Into<String>) -> Self {
        Self::Unrecognized(message.into())
    }

    pub fn matrix_code(&self) -> MatrixErrorCode {
        match self {
            ApiError::BadRequest(_) => MatrixErrorCode::BadJson,
            ApiError::Unauthorized(_) => MatrixErrorCode::Unauthorized,
            ApiError::Forbidden(_) => MatrixErrorCode::Forbidden,
            ApiError::NotFound(_) => MatrixErrorCode::NotFound,
            ApiError::Conflict(_) => MatrixErrorCode::UserInUse,
            ApiError::RateLimited => MatrixErrorCode::LimitExceeded,
            ApiError::Internal(_) => MatrixErrorCode::Unknown,
            ApiError::Database(_) => MatrixErrorCode::Unknown,
            ApiError::Cache(_) => MatrixErrorCode::Unknown,
            ApiError::Authentication(_) => MatrixErrorCode::UnknownToken,
            ApiError::Validation(_) => MatrixErrorCode::InvalidParam,
            ApiError::InvalidInput(_) => MatrixErrorCode::InvalidParam,
            ApiError::DecryptionError(_) => MatrixErrorCode::Unknown,
            ApiError::EncryptionError(_) => MatrixErrorCode::Unknown,
            ApiError::Crypto(_) => MatrixErrorCode::Unknown,
            ApiError::Gone(_) => MatrixErrorCode::NotFound,
            ApiError::MissingToken => MatrixErrorCode::MissingToken,
            ApiError::NotJson(_) => MatrixErrorCode::NotJson,
            ApiError::UserDeactivated(_) => MatrixErrorCode::UserDeactivated,
            ApiError::InvalidUsername(_) => MatrixErrorCode::InvalidUsername,
            ApiError::RoomInUse(_) => MatrixErrorCode::RoomInUse,
            ApiError::InvalidRoomState(_) => MatrixErrorCode::InvalidRoomState,
            ApiError::ThreepidInUse(_) => MatrixErrorCode::ThreepidInUse,
            ApiError::ThreepidNotFound(_) => MatrixErrorCode::ThreepidNotFound,
            ApiError::ThreepidAuthFailed(_) => MatrixErrorCode::ThreepidAuthFailed,
            ApiError::ThreepidDenied(_) => MatrixErrorCode::ThreepidDenied,
            ApiError::ServerNotTrusted(_) => MatrixErrorCode::ServerNotTrusted,
            ApiError::UnsupportedRoomVersion(_) => MatrixErrorCode::UnsupportedRoomVersion,
            ApiError::IncompatibleRoomVersion(_) => MatrixErrorCode::IncompatibleRoomVersion,
            ApiError::BadState(_) => MatrixErrorCode::BadState,
            ApiError::GuestAccessForbidden(_) => MatrixErrorCode::GuestAccessForbidden,
            ApiError::CaptchaNeeded(_) => MatrixErrorCode::CaptchaNeeded,
            ApiError::CaptchaInvalid(_) => MatrixErrorCode::CaptchaInvalid,
            ApiError::MissingParam(_) => MatrixErrorCode::MissingParam,
            ApiError::TooLarge(_) => MatrixErrorCode::TooLarge,
            ApiError::Exclusive(_) => MatrixErrorCode::Exclusive,
            ApiError::ResourceLimitExceeded(_) => MatrixErrorCode::ResourceLimitExceeded,
            ApiError::CannotLeaveServerNoticeRoom(_) => {
                MatrixErrorCode::CannotLeaveServerNoticeRoom
            }
            ApiError::Unknown(_) => MatrixErrorCode::Unknown,
            ApiError::Unrecognized(_) => MatrixErrorCode::Unrecognized,
        }
    }

    pub fn code(&self) -> &'static str {
        self.matrix_code().as_str()
    }

    pub fn message(&self) -> String {
        match self {
            ApiError::BadRequest(msg) => msg.clone(),
            ApiError::Unauthorized(msg) => msg.clone(),
            ApiError::Forbidden(msg) => msg.clone(),
            ApiError::NotFound(msg) => msg.clone(),
            ApiError::Conflict(msg) => msg.clone(),
            ApiError::RateLimited => "Rate limited".to_string(),
            ApiError::Internal(msg) => format!("Internal error: {}", msg),
            ApiError::Database(msg) => format!("Database error: {}", msg),
            ApiError::Cache(msg) => format!("Cache error: {}", msg),
            ApiError::Authentication(msg) => msg.clone(),
            ApiError::Validation(msg) => msg.clone(),
            ApiError::InvalidInput(msg) => msg.clone(),
            ApiError::DecryptionError(msg) => msg.clone(),
            ApiError::EncryptionError(msg) => msg.clone(),
            ApiError::Crypto(msg) => msg.clone(),
            ApiError::Gone(msg) => msg.clone(),
            ApiError::MissingToken => "Missing access token".to_string(),
            ApiError::NotJson(msg) => msg.clone(),
            ApiError::UserDeactivated(msg) => msg.clone(),
            ApiError::InvalidUsername(msg) => msg.clone(),
            ApiError::RoomInUse(msg) => msg.clone(),
            ApiError::InvalidRoomState(msg) => msg.clone(),
            ApiError::ThreepidInUse(msg) => msg.clone(),
            ApiError::ThreepidNotFound(msg) => msg.clone(),
            ApiError::ThreepidAuthFailed(msg) => msg.clone(),
            ApiError::ThreepidDenied(msg) => msg.clone(),
            ApiError::ServerNotTrusted(msg) => msg.clone(),
            ApiError::UnsupportedRoomVersion(msg) => msg.clone(),
            ApiError::IncompatibleRoomVersion(msg) => msg.clone(),
            ApiError::BadState(msg) => msg.clone(),
            ApiError::GuestAccessForbidden(msg) => msg.clone(),
            ApiError::CaptchaNeeded(msg) => msg.clone(),
            ApiError::CaptchaInvalid(msg) => msg.clone(),
            ApiError::MissingParam(msg) => msg.clone(),
            ApiError::TooLarge(msg) => msg.clone(),
            ApiError::Exclusive(msg) => msg.clone(),
            ApiError::ResourceLimitExceeded(msg) => msg.clone(),
            ApiError::CannotLeaveServerNoticeRoom(msg) => msg.clone(),
            ApiError::Unknown(msg) => msg.clone(),
            ApiError::Unrecognized(msg) => msg.clone(),
        }
    }

    pub fn http_status(&self) -> StatusCode {
        self.matrix_code().http_status()
    }
}

impl IntoResponse for ApiError {
    fn into_response(self) -> Response {
        if matches!(self, ApiError::Gone(_)) {
            let status_code = StatusCode::GONE;
            return (
                status_code,
                Json(
                    serde_json::from_str::<serde_json::Value>(&self.message())
                        .unwrap_or_else(|_| json!({"error": "Gone", "message": self.message()})),
                ),
            )
                .into_response();
        }

        let matrix_code = self.matrix_code();
        let errcode = matrix_code.as_str().to_string();
        let error_msg = self.message();
        let status_code = matrix_code.http_status();

        let response: ApiResponse<()> = ApiResponse {
            status: "error".to_string(),
            data: None,
            error: Some(error_msg),
            errcode: Some(errcode),
            retry_after_ms: None,
        };

        (status_code, Json(response)).into_response()
    }
}

pub trait ErrorContext {
    fn with_context(self, module: &str, operation: &str) -> Self;
}

impl ErrorContext for ApiError {
    fn with_context(self, module: &str, operation: &str) -> Self {
        match self {
            ApiError::Internal(msg) => {
                ApiError::Internal(format!("[{}::{}] {}", module, operation, msg))
            }
            ApiError::Database(msg) => {
                ApiError::Database(format!("[{}::{}] {}", module, operation, msg))
            }
            ApiError::Cache(msg) => ApiError::Cache(format!("[{}::{}] {}", module, operation, msg)),
            ApiError::Authentication(msg) => {
                ApiError::Authentication(format!("[{}::{}] {}", module, operation, msg))
            }
            ApiError::Validation(msg) => {
                ApiError::Validation(format!("[{}::{}] {}", module, operation, msg))
            }
            ApiError::Crypto(msg) => {
                ApiError::Crypto(format!("[{}::{}] {}", module, operation, msg))
            }
            _ => self,
        }
    }
}

#[macro_export]
macro_rules! dbg_context {
    ($result:expr, $module:expr, $operation:expr) => {
        $result.map_err(|e| e.with_context($module, $operation))
    };
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
        ApiError::not_json(err.to_string())
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

#[cfg(test)]
mod tests {
    use super::*;
    use axum::http::StatusCode;

    #[test]
    fn test_matrix_error_code_as_str() {
        assert_eq!(MatrixErrorCode::Forbidden.as_str(), "M_FORBIDDEN");
        assert_eq!(MatrixErrorCode::UnknownToken.as_str(), "M_UNKNOWN_TOKEN");
        assert_eq!(MatrixErrorCode::MissingToken.as_str(), "M_MISSING_TOKEN");
        assert_eq!(MatrixErrorCode::BadJson.as_str(), "M_BAD_JSON");
        assert_eq!(MatrixErrorCode::NotJson.as_str(), "M_NOT_JSON");
        assert_eq!(MatrixErrorCode::NotFound.as_str(), "M_NOT_FOUND");
        assert_eq!(MatrixErrorCode::LimitExceeded.as_str(), "M_LIMIT_EXCEEDED");
        assert_eq!(MatrixErrorCode::Unknown.as_str(), "M_UNKNOWN");
        assert_eq!(MatrixErrorCode::Unrecognized.as_str(), "M_UNRECOGNIZED");
        assert_eq!(MatrixErrorCode::Unauthorized.as_str(), "M_UNAUTHORIZED");
        assert_eq!(
            MatrixErrorCode::UserDeactivated.as_str(),
            "M_USER_DEACTIVATED"
        );
        assert_eq!(MatrixErrorCode::UserInUse.as_str(), "M_USER_IN_USE");
        assert_eq!(
            MatrixErrorCode::InvalidUsername.as_str(),
            "M_INVALID_USERNAME"
        );
        assert_eq!(MatrixErrorCode::RoomInUse.as_str(), "M_ROOM_IN_USE");
        assert_eq!(
            MatrixErrorCode::InvalidRoomState.as_str(),
            "M_INVALID_ROOM_STATE"
        );
        assert_eq!(MatrixErrorCode::ThreepidInUse.as_str(), "M_THREEPID_IN_USE");
        assert_eq!(
            MatrixErrorCode::ThreepidNotFound.as_str(),
            "M_THREEPID_NOT_FOUND"
        );
        assert_eq!(
            MatrixErrorCode::ThreepidAuthFailed.as_str(),
            "M_THREEPID_AUTH_FAILED"
        );
        assert_eq!(
            MatrixErrorCode::ThreepidDenied.as_str(),
            "M_THREEPID_DENIED"
        );
        assert_eq!(
            MatrixErrorCode::ServerNotTrusted.as_str(),
            "M_SERVER_NOT_TRUSTED"
        );
        assert_eq!(
            MatrixErrorCode::UnsupportedRoomVersion.as_str(),
            "M_UNSUPPORTED_ROOM_VERSION"
        );
        assert_eq!(
            MatrixErrorCode::IncompatibleRoomVersion.as_str(),
            "M_INCOMPATIBLE_ROOM_VERSION"
        );
        assert_eq!(MatrixErrorCode::BadState.as_str(), "M_BAD_STATE");
        assert_eq!(
            MatrixErrorCode::GuestAccessForbidden.as_str(),
            "M_GUEST_ACCESS_FORBIDDEN"
        );
        assert_eq!(MatrixErrorCode::CaptchaNeeded.as_str(), "M_CAPTCHA_NEEDED");
        assert_eq!(
            MatrixErrorCode::CaptchaInvalid.as_str(),
            "M_CAPTCHA_INVALID"
        );
        assert_eq!(MatrixErrorCode::MissingParam.as_str(), "M_MISSING_PARAM");
        assert_eq!(MatrixErrorCode::InvalidParam.as_str(), "M_INVALID_PARAM");
        assert_eq!(MatrixErrorCode::TooLarge.as_str(), "M_TOO_LARGE");
        assert_eq!(MatrixErrorCode::Exclusive.as_str(), "M_EXCLUSIVE");
        assert_eq!(
            MatrixErrorCode::ResourceLimitExceeded.as_str(),
            "M_RESOURCE_LIMIT_EXCEEDED"
        );
        assert_eq!(
            MatrixErrorCode::CannotLeaveServerNoticeRoom.as_str(),
            "M_CANNOT_LEAVE_SERVER_NOTICE_ROOM"
        );
    }

    #[test]
    fn test_matrix_error_code_http_status() {
        assert_eq!(
            MatrixErrorCode::Forbidden.http_status(),
            StatusCode::FORBIDDEN
        );
        assert_eq!(
            MatrixErrorCode::UnknownToken.http_status(),
            StatusCode::UNAUTHORIZED
        );
        assert_eq!(
            MatrixErrorCode::MissingToken.http_status(),
            StatusCode::UNAUTHORIZED
        );
        assert_eq!(
            MatrixErrorCode::BadJson.http_status(),
            StatusCode::BAD_REQUEST
        );
        assert_eq!(
            MatrixErrorCode::NotJson.http_status(),
            StatusCode::BAD_REQUEST
        );
        assert_eq!(
            MatrixErrorCode::NotFound.http_status(),
            StatusCode::NOT_FOUND
        );
        assert_eq!(
            MatrixErrorCode::LimitExceeded.http_status(),
            StatusCode::TOO_MANY_REQUESTS
        );
        assert_eq!(
            MatrixErrorCode::UserInUse.http_status(),
            StatusCode::CONFLICT
        );
        assert_eq!(
            MatrixErrorCode::TooLarge.http_status(),
            StatusCode::PAYLOAD_TOO_LARGE
        );
        assert_eq!(
            MatrixErrorCode::ServerNotTrusted.http_status(),
            StatusCode::BAD_GATEWAY
        );
    }

    #[test]
    fn test_matrix_error_code_serialization() {
        let code = MatrixErrorCode::Forbidden;
        let json = serde_json::to_string(&code).unwrap();
        assert_eq!(json, "\"M_FORBIDDEN\"");

        let code = MatrixErrorCode::UnknownToken;
        let json = serde_json::to_string(&code).unwrap();
        assert_eq!(json, "\"M_UNKNOWN_TOKEN\"");
    }

    #[test]
    fn test_matrix_error_code_deserialization() {
        let code: MatrixErrorCode = serde_json::from_str("\"M_FORBIDDEN\"").unwrap();
        assert_eq!(code, MatrixErrorCode::Forbidden);

        let code: MatrixErrorCode = serde_json::from_str("\"M_UNKNOWN_TOKEN\"").unwrap();
        assert_eq!(code, MatrixErrorCode::UnknownToken);

        let code: MatrixErrorCode = serde_json::from_str("\"M_USER_DEACTIVATED\"").unwrap();
        assert_eq!(code, MatrixErrorCode::UserDeactivated);
    }

    #[test]
    fn test_api_error_matrix_code_mapping() {
        assert_eq!(
            ApiError::bad_request("test").matrix_code(),
            MatrixErrorCode::BadJson
        );
        assert_eq!(
            ApiError::unauthorized("test").matrix_code(),
            MatrixErrorCode::Unauthorized
        );
        assert_eq!(
            ApiError::forbidden("test").matrix_code(),
            MatrixErrorCode::Forbidden
        );
        assert_eq!(
            ApiError::not_found("test").matrix_code(),
            MatrixErrorCode::NotFound
        );
        assert_eq!(
            ApiError::conflict("test").matrix_code(),
            MatrixErrorCode::UserInUse
        );
        assert_eq!(
            ApiError::RateLimited.matrix_code(),
            MatrixErrorCode::LimitExceeded
        );
        assert_eq!(
            ApiError::missing_token().matrix_code(),
            MatrixErrorCode::MissingToken
        );
        assert_eq!(
            ApiError::not_json("test").matrix_code(),
            MatrixErrorCode::NotJson
        );
        assert_eq!(
            ApiError::user_deactivated("test").matrix_code(),
            MatrixErrorCode::UserDeactivated
        );
        assert_eq!(
            ApiError::invalid_username("test").matrix_code(),
            MatrixErrorCode::InvalidUsername
        );
        assert_eq!(
            ApiError::room_in_use("test").matrix_code(),
            MatrixErrorCode::RoomInUse
        );
        assert_eq!(
            ApiError::invalid_room_state("test").matrix_code(),
            MatrixErrorCode::InvalidRoomState
        );
        assert_eq!(
            ApiError::threepid_in_use("test").matrix_code(),
            MatrixErrorCode::ThreepidInUse
        );
        assert_eq!(
            ApiError::threepid_not_found("test").matrix_code(),
            MatrixErrorCode::ThreepidNotFound
        );
        assert_eq!(
            ApiError::threepid_auth_failed("test").matrix_code(),
            MatrixErrorCode::ThreepidAuthFailed
        );
        assert_eq!(
            ApiError::threepid_denied("test").matrix_code(),
            MatrixErrorCode::ThreepidDenied
        );
        assert_eq!(
            ApiError::server_not_trusted("test").matrix_code(),
            MatrixErrorCode::ServerNotTrusted
        );
        assert_eq!(
            ApiError::unsupported_room_version("test").matrix_code(),
            MatrixErrorCode::UnsupportedRoomVersion
        );
        assert_eq!(
            ApiError::incompatible_room_version("test").matrix_code(),
            MatrixErrorCode::IncompatibleRoomVersion
        );
        assert_eq!(
            ApiError::bad_state("test").matrix_code(),
            MatrixErrorCode::BadState
        );
        assert_eq!(
            ApiError::guest_access_forbidden("test").matrix_code(),
            MatrixErrorCode::GuestAccessForbidden
        );
        assert_eq!(
            ApiError::captcha_needed("test").matrix_code(),
            MatrixErrorCode::CaptchaNeeded
        );
        assert_eq!(
            ApiError::captcha_invalid("test").matrix_code(),
            MatrixErrorCode::CaptchaInvalid
        );
        assert_eq!(
            ApiError::missing_param("test").matrix_code(),
            MatrixErrorCode::MissingParam
        );
        assert_eq!(
            ApiError::too_large("test").matrix_code(),
            MatrixErrorCode::TooLarge
        );
        assert_eq!(
            ApiError::exclusive("test").matrix_code(),
            MatrixErrorCode::Exclusive
        );
        assert_eq!(
            ApiError::resource_limit_exceeded("test").matrix_code(),
            MatrixErrorCode::ResourceLimitExceeded
        );
        assert_eq!(
            ApiError::cannot_leave_server_notice_room("test").matrix_code(),
            MatrixErrorCode::CannotLeaveServerNoticeRoom
        );
        assert_eq!(
            ApiError::unknown("test").matrix_code(),
            MatrixErrorCode::Unknown
        );
        assert_eq!(
            ApiError::unrecognized("test").matrix_code(),
            MatrixErrorCode::Unrecognized
        );
    }

    #[test]
    fn test_api_error_variants() {
        let errors: Vec<ApiError> = vec![
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
            ApiError::MissingToken,
            ApiError::NotJson("not json".to_string()),
            ApiError::UserDeactivated("user deactivated".to_string()),
            ApiError::InvalidUsername("invalid username".to_string()),
            ApiError::RoomInUse("room in use".to_string()),
            ApiError::InvalidRoomState("invalid room state".to_string()),
            ApiError::ThreepidInUse("threepid in use".to_string()),
            ApiError::ThreepidNotFound("threepid not found".to_string()),
            ApiError::ThreepidAuthFailed("threepid auth failed".to_string()),
            ApiError::ThreepidDenied("threepid denied".to_string()),
            ApiError::ServerNotTrusted("server not trusted".to_string()),
            ApiError::UnsupportedRoomVersion("unsupported room version".to_string()),
            ApiError::IncompatibleRoomVersion("incompatible room version".to_string()),
            ApiError::BadState("bad state".to_string()),
            ApiError::GuestAccessForbidden("guest access forbidden".to_string()),
            ApiError::CaptchaNeeded("captcha needed".to_string()),
            ApiError::CaptchaInvalid("captcha invalid".to_string()),
            ApiError::MissingParam("missing param".to_string()),
            ApiError::TooLarge("too large".to_string()),
            ApiError::Exclusive("exclusive".to_string()),
            ApiError::ResourceLimitExceeded("resource limit exceeded".to_string()),
            ApiError::CannotLeaveServerNoticeRoom("cannot leave server notice room".to_string()),
            ApiError::Unknown("unknown".to_string()),
            ApiError::Unrecognized("unrecognized".to_string()),
        ];

        for error in errors {
            let _ = format!("{:?}", error);
            let _ = error.code();
            let _ = error.message();
            let _ = error.http_status();
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
        assert!(matches!(ApiError::missing_token(), ApiError::MissingToken));
        assert!(matches!(ApiError::not_json("test"), ApiError::NotJson(_)));
        assert!(matches!(
            ApiError::user_deactivated("test"),
            ApiError::UserDeactivated(_)
        ));
        assert!(matches!(
            ApiError::invalid_username("test"),
            ApiError::InvalidUsername(_)
        ));
        assert!(matches!(
            ApiError::room_in_use("test"),
            ApiError::RoomInUse(_)
        ));
        assert!(matches!(
            ApiError::invalid_room_state("test"),
            ApiError::InvalidRoomState(_)
        ));
        assert!(matches!(
            ApiError::threepid_in_use("test"),
            ApiError::ThreepidInUse(_)
        ));
        assert!(matches!(
            ApiError::threepid_not_found("test"),
            ApiError::ThreepidNotFound(_)
        ));
        assert!(matches!(
            ApiError::threepid_auth_failed("test"),
            ApiError::ThreepidAuthFailed(_)
        ));
        assert!(matches!(
            ApiError::threepid_denied("test"),
            ApiError::ThreepidDenied(_)
        ));
        assert!(matches!(
            ApiError::server_not_trusted("test"),
            ApiError::ServerNotTrusted(_)
        ));
        assert!(matches!(
            ApiError::unsupported_room_version("test"),
            ApiError::UnsupportedRoomVersion(_)
        ));
        assert!(matches!(
            ApiError::incompatible_room_version("test"),
            ApiError::IncompatibleRoomVersion(_)
        ));
        assert!(matches!(ApiError::bad_state("test"), ApiError::BadState(_)));
        assert!(matches!(
            ApiError::guest_access_forbidden("test"),
            ApiError::GuestAccessForbidden(_)
        ));
        assert!(matches!(
            ApiError::captcha_needed("test"),
            ApiError::CaptchaNeeded(_)
        ));
        assert!(matches!(
            ApiError::captcha_invalid("test"),
            ApiError::CaptchaInvalid(_)
        ));
        assert!(matches!(
            ApiError::missing_param("test"),
            ApiError::MissingParam(_)
        ));
        assert!(matches!(ApiError::too_large("test"), ApiError::TooLarge(_)));
        assert!(matches!(
            ApiError::exclusive("test"),
            ApiError::Exclusive(_)
        ));
        assert!(matches!(
            ApiError::resource_limit_exceeded("test"),
            ApiError::ResourceLimitExceeded(_)
        ));
        assert!(matches!(
            ApiError::cannot_leave_server_notice_room("test"),
            ApiError::CannotLeaveServerNoticeRoom(_)
        ));
        assert!(matches!(ApiError::unknown("test"), ApiError::Unknown(_)));
        assert!(matches!(
            ApiError::unrecognized("test"),
            ApiError::Unrecognized(_)
        ));
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
    fn test_into_response_missing_token() {
        let error = ApiError::missing_token();
        let response = error.into_response();
        assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
    }

    #[test]
    fn test_into_response_user_deactivated() {
        let error = ApiError::user_deactivated("User account has been deactivated");
        let response = error.into_response();
        assert_eq!(response.status(), StatusCode::FORBIDDEN);
    }

    #[test]
    fn test_into_response_too_large() {
        let error = ApiError::too_large("Request body too large");
        let response = error.into_response();
        assert_eq!(response.status(), StatusCode::PAYLOAD_TOO_LARGE);
    }

    #[test]
    fn test_into_response_server_not_trusted() {
        let error = ApiError::server_not_trusted("Server is not in trusted list");
        let response = error.into_response();
        assert_eq!(response.status(), StatusCode::BAD_GATEWAY);
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
        assert!(matches!(api_error, ApiError::NotJson(_)));
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

    #[test]
    fn test_all_matrix_error_codes_covered() {
        let expected_codes = [
            "M_FORBIDDEN",
            "M_UNKNOWN_TOKEN",
            "M_MISSING_TOKEN",
            "M_BAD_JSON",
            "M_NOT_JSON",
            "M_NOT_FOUND",
            "M_LIMIT_EXCEEDED",
            "M_UNKNOWN",
            "M_UNRECOGNIZED",
            "M_UNAUTHORIZED",
            "M_USER_DEACTIVATED",
            "M_USER_IN_USE",
            "M_INVALID_USERNAME",
            "M_ROOM_IN_USE",
            "M_INVALID_ROOM_STATE",
            "M_THREEPID_IN_USE",
            "M_THREEPID_NOT_FOUND",
            "M_THREEPID_AUTH_FAILED",
            "M_THREEPID_DENIED",
            "M_SERVER_NOT_TRUSTED",
            "M_UNSUPPORTED_ROOM_VERSION",
            "M_INCOMPATIBLE_ROOM_VERSION",
            "M_BAD_STATE",
            "M_GUEST_ACCESS_FORBIDDEN",
            "M_CAPTCHA_NEEDED",
            "M_CAPTCHA_INVALID",
            "M_MISSING_PARAM",
            "M_INVALID_PARAM",
            "M_TOO_LARGE",
            "M_EXCLUSIVE",
            "M_RESOURCE_LIMIT_EXCEEDED",
            "M_CANNOT_LEAVE_SERVER_NOTICE_ROOM",
        ];

        for expected_code in expected_codes {
            let found = match expected_code {
                "M_FORBIDDEN" => MatrixErrorCode::Forbidden.as_str(),
                "M_UNKNOWN_TOKEN" => MatrixErrorCode::UnknownToken.as_str(),
                "M_MISSING_TOKEN" => MatrixErrorCode::MissingToken.as_str(),
                "M_BAD_JSON" => MatrixErrorCode::BadJson.as_str(),
                "M_NOT_JSON" => MatrixErrorCode::NotJson.as_str(),
                "M_NOT_FOUND" => MatrixErrorCode::NotFound.as_str(),
                "M_LIMIT_EXCEEDED" => MatrixErrorCode::LimitExceeded.as_str(),
                "M_UNKNOWN" => MatrixErrorCode::Unknown.as_str(),
                "M_UNRECOGNIZED" => MatrixErrorCode::Unrecognized.as_str(),
                "M_UNAUTHORIZED" => MatrixErrorCode::Unauthorized.as_str(),
                "M_USER_DEACTIVATED" => MatrixErrorCode::UserDeactivated.as_str(),
                "M_USER_IN_USE" => MatrixErrorCode::UserInUse.as_str(),
                "M_INVALID_USERNAME" => MatrixErrorCode::InvalidUsername.as_str(),
                "M_ROOM_IN_USE" => MatrixErrorCode::RoomInUse.as_str(),
                "M_INVALID_ROOM_STATE" => MatrixErrorCode::InvalidRoomState.as_str(),
                "M_THREEPID_IN_USE" => MatrixErrorCode::ThreepidInUse.as_str(),
                "M_THREEPID_NOT_FOUND" => MatrixErrorCode::ThreepidNotFound.as_str(),
                "M_THREEPID_AUTH_FAILED" => MatrixErrorCode::ThreepidAuthFailed.as_str(),
                "M_THREEPID_DENIED" => MatrixErrorCode::ThreepidDenied.as_str(),
                "M_SERVER_NOT_TRUSTED" => MatrixErrorCode::ServerNotTrusted.as_str(),
                "M_UNSUPPORTED_ROOM_VERSION" => MatrixErrorCode::UnsupportedRoomVersion.as_str(),
                "M_INCOMPATIBLE_ROOM_VERSION" => MatrixErrorCode::IncompatibleRoomVersion.as_str(),
                "M_BAD_STATE" => MatrixErrorCode::BadState.as_str(),
                "M_GUEST_ACCESS_FORBIDDEN" => MatrixErrorCode::GuestAccessForbidden.as_str(),
                "M_CAPTCHA_NEEDED" => MatrixErrorCode::CaptchaNeeded.as_str(),
                "M_CAPTCHA_INVALID" => MatrixErrorCode::CaptchaInvalid.as_str(),
                "M_MISSING_PARAM" => MatrixErrorCode::MissingParam.as_str(),
                "M_INVALID_PARAM" => MatrixErrorCode::InvalidParam.as_str(),
                "M_TOO_LARGE" => MatrixErrorCode::TooLarge.as_str(),
                "M_EXCLUSIVE" => MatrixErrorCode::Exclusive.as_str(),
                "M_RESOURCE_LIMIT_EXCEEDED" => MatrixErrorCode::ResourceLimitExceeded.as_str(),
                "M_CANNOT_LEAVE_SERVER_NOTICE_ROOM" => {
                    MatrixErrorCode::CannotLeaveServerNoticeRoom.as_str()
                }
                _ => panic!("Unexpected error code: {}", expected_code),
            };
            assert_eq!(found, expected_code);
        }
    }
}
