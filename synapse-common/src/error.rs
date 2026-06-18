use std::sync::{Arc, OnceLock};

use axum::{
    http::{HeaderValue, StatusCode},
    response::{IntoResponse, Response},
    Json,
};
use serde::{Deserialize, Serialize};
use serde_json::json;

use crate::metrics::MetricsCollector;

static ERROR_METRICS: OnceLock<Arc<MetricsCollector>> = OnceLock::new();

pub fn init_error_metrics(collector: Arc<MetricsCollector>) {
    let _ = ERROR_METRICS.set(collector);
}

// ---------------------------------------------------------------------------
// MatrixErrorCode — Matrix spec error codes
// ---------------------------------------------------------------------------

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
    Unimplemented,
    RequestTimeout,
}

impl MatrixErrorCode {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Forbidden => "M_FORBIDDEN",
            Self::UnknownToken => "M_UNKNOWN_TOKEN",
            Self::MissingToken => "M_MISSING_TOKEN",
            Self::BadJson => "M_BAD_JSON",
            Self::NotJson => "M_NOT_JSON",
            Self::NotFound => "M_NOT_FOUND",
            Self::LimitExceeded => "M_LIMIT_EXCEEDED",
            Self::Unknown => "M_UNKNOWN",
            Self::Unrecognized => "M_UNRECOGNIZED",
            Self::Unauthorized => "M_UNAUTHORIZED",
            Self::UserDeactivated => "M_USER_DEACTIVATED",
            Self::UserInUse => "M_USER_IN_USE",
            Self::InvalidUsername => "M_INVALID_USERNAME",
            Self::RoomInUse => "M_ROOM_IN_USE",
            Self::InvalidRoomState => "M_INVALID_ROOM_STATE",
            Self::ThreepidInUse => "M_THREEPID_IN_USE",
            Self::ThreepidNotFound => "M_THREEPID_NOT_FOUND",
            Self::ThreepidAuthFailed => "M_THREEPID_AUTH_FAILED",
            Self::ThreepidDenied => "M_THREEPID_DENIED",
            Self::ServerNotTrusted => "M_SERVER_NOT_TRUSTED",
            Self::UnsupportedRoomVersion => "M_UNSUPPORTED_ROOM_VERSION",
            Self::IncompatibleRoomVersion => "M_INCOMPATIBLE_ROOM_VERSION",
            Self::BadState => "M_BAD_STATE",
            Self::GuestAccessForbidden => "M_GUEST_ACCESS_FORBIDDEN",
            Self::CaptchaNeeded => "M_CAPTCHA_NEEDED",
            Self::CaptchaInvalid => "M_CAPTCHA_INVALID",
            Self::MissingParam => "M_MISSING_PARAM",
            Self::InvalidParam => "M_INVALID_PARAM",
            Self::TooLarge => "M_TOO_LARGE",
            Self::Exclusive => "M_EXCLUSIVE",
            Self::ResourceLimitExceeded => "M_RESOURCE_LIMIT_EXCEEDED",
            Self::CannotLeaveServerNoticeRoom => "M_CANNOT_LEAVE_SERVER_NOTICE_ROOM",
            Self::Unimplemented => "M_UNRECOGNIZED",
            Self::RequestTimeout => "M_REQUEST_TIMEOUT",
        }
    }

    pub fn http_status(&self) -> StatusCode {
        match self {
            Self::Forbidden => StatusCode::FORBIDDEN,
            Self::UnknownToken => StatusCode::UNAUTHORIZED,
            Self::MissingToken => StatusCode::UNAUTHORIZED,
            Self::BadJson => StatusCode::BAD_REQUEST,
            Self::NotJson => StatusCode::BAD_REQUEST,
            Self::NotFound => StatusCode::NOT_FOUND,
            Self::LimitExceeded => StatusCode::TOO_MANY_REQUESTS,
            Self::Unknown => StatusCode::INTERNAL_SERVER_ERROR,
            Self::Unrecognized => StatusCode::NOT_FOUND,
            Self::Unauthorized => StatusCode::UNAUTHORIZED,
            Self::UserDeactivated => StatusCode::FORBIDDEN,
            Self::UserInUse => StatusCode::CONFLICT,
            Self::InvalidUsername => StatusCode::BAD_REQUEST,
            Self::RoomInUse => StatusCode::CONFLICT,
            Self::InvalidRoomState => StatusCode::BAD_REQUEST,
            Self::ThreepidInUse => StatusCode::CONFLICT,
            Self::ThreepidNotFound => StatusCode::BAD_REQUEST,
            Self::ThreepidAuthFailed => StatusCode::FORBIDDEN,
            Self::ThreepidDenied => StatusCode::FORBIDDEN,
            Self::ServerNotTrusted => StatusCode::BAD_GATEWAY,
            Self::UnsupportedRoomVersion => StatusCode::BAD_REQUEST,
            Self::IncompatibleRoomVersion => StatusCode::BAD_REQUEST,
            Self::BadState => StatusCode::BAD_REQUEST,
            Self::GuestAccessForbidden => StatusCode::FORBIDDEN,
            Self::CaptchaNeeded => StatusCode::BAD_REQUEST,
            Self::CaptchaInvalid => StatusCode::BAD_REQUEST,
            Self::MissingParam => StatusCode::BAD_REQUEST,
            Self::InvalidParam => StatusCode::BAD_REQUEST,
            Self::TooLarge => StatusCode::PAYLOAD_TOO_LARGE,
            Self::Exclusive => StatusCode::CONFLICT,
            Self::ResourceLimitExceeded => StatusCode::FORBIDDEN,
            Self::CannotLeaveServerNoticeRoom => StatusCode::FORBIDDEN,
            Self::Unimplemented => StatusCode::NOT_IMPLEMENTED,
            Self::RequestTimeout => StatusCode::REQUEST_TIMEOUT,
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
            "M_FORBIDDEN" => Ok(Self::Forbidden),
            "M_UNKNOWN_TOKEN" => Ok(Self::UnknownToken),
            "M_MISSING_TOKEN" => Ok(Self::MissingToken),
            "M_BAD_JSON" => Ok(Self::BadJson),
            "M_NOT_JSON" => Ok(Self::NotJson),
            "M_NOT_FOUND" => Ok(Self::NotFound),
            "M_LIMIT_EXCEEDED" => Ok(Self::LimitExceeded),
            "M_UNKNOWN" => Ok(Self::Unknown),
            "M_UNRECOGNIZED" => Ok(Self::Unrecognized),
            "M_UNAUTHORIZED" => Ok(Self::Unauthorized),
            "M_USER_DEACTIVATED" => Ok(Self::UserDeactivated),
            "M_USER_IN_USE" => Ok(Self::UserInUse),
            "M_INVALID_USERNAME" => Ok(Self::InvalidUsername),
            "M_ROOM_IN_USE" => Ok(Self::RoomInUse),
            "M_INVALID_ROOM_STATE" => Ok(Self::InvalidRoomState),
            "M_THREEPID_IN_USE" => Ok(Self::ThreepidInUse),
            "M_THREEPID_NOT_FOUND" => Ok(Self::ThreepidNotFound),
            "M_THREEPID_AUTH_FAILED" => Ok(Self::ThreepidAuthFailed),
            "M_THREEPID_DENIED" => Ok(Self::ThreepidDenied),
            "M_SERVER_NOT_TRUSTED" => Ok(Self::ServerNotTrusted),
            "M_UNSUPPORTED_ROOM_VERSION" => Ok(Self::UnsupportedRoomVersion),
            "M_INCOMPATIBLE_ROOM_VERSION" => Ok(Self::IncompatibleRoomVersion),
            "M_BAD_STATE" => Ok(Self::BadState),
            "M_GUEST_ACCESS_FORBIDDEN" => Ok(Self::GuestAccessForbidden),
            "M_CAPTCHA_NEEDED" => Ok(Self::CaptchaNeeded),
            "M_CAPTCHA_INVALID" => Ok(Self::CaptchaInvalid),
            "M_MISSING_PARAM" => Ok(Self::MissingParam),
            "M_INVALID_PARAM" => Ok(Self::InvalidParam),
            "M_TOO_LARGE" => Ok(Self::TooLarge),
            "M_EXCLUSIVE" => Ok(Self::Exclusive),
            "M_RESOURCE_LIMIT_EXCEEDED" => Ok(Self::ResourceLimitExceeded),
            "M_CANNOT_LEAVE_SERVER_NOTICE_ROOM" => Ok(Self::CannotLeaveServerNoticeRoom),
            "M_REQUEST_TIMEOUT" => Ok(Self::RequestTimeout),
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
                    "M_REQUEST_TIMEOUT",
                ],
            )),
        }
    }
}

// ---------------------------------------------------------------------------
// ApiErrorKind — semantic error category (10 variants replacing 42)
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ApiErrorKind {
    /// 400 — client request is malformed
    BadRequest,
    /// 401 — authentication required or invalid
    Unauthorized,
    /// 403 — authenticated but not allowed
    Forbidden,
    /// 404 — resource does not exist
    NotFound,
    /// 409 — resource conflict (e.g. duplicate)
    Conflict,
    /// 410 — resource permanently gone
    Gone,
    /// 429 — rate limit exceeded
    RateLimited,
    /// 500 — unexpected internal error
    Internal,
    /// 501 — not implemented
    NotImplemented,
    /// 504 — request timed out
    Timeout,
}

impl ApiErrorKind {
    pub fn default_http_status(&self) -> StatusCode {
        match self {
            Self::BadRequest => StatusCode::BAD_REQUEST,
            Self::Unauthorized => StatusCode::UNAUTHORIZED,
            Self::Forbidden => StatusCode::FORBIDDEN,
            Self::NotFound => StatusCode::NOT_FOUND,
            Self::Conflict => StatusCode::CONFLICT,
            Self::Gone => StatusCode::GONE,
            Self::RateLimited => StatusCode::TOO_MANY_REQUESTS,
            Self::Internal => StatusCode::INTERNAL_SERVER_ERROR,
            Self::NotImplemented => StatusCode::NOT_IMPLEMENTED,
            Self::Timeout => StatusCode::REQUEST_TIMEOUT,
        }
    }
}

// ---------------------------------------------------------------------------
// ErrorSource — where the error originated
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ErrorSource {
    /// Module path (e.g. "storage::room")
    pub module: String,
    /// Operation name (e.g. "get_room_messages")
    pub operation: String,
}

impl ErrorSource {
    pub fn new(module: impl Into<String>, operation: impl Into<String>) -> Self {
        Self { module: module.into(), operation: operation.into() }
    }
}

impl std::fmt::Display for ErrorSource {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "[{}::{}]", self.module, self.operation)
    }
}

// ---------------------------------------------------------------------------
// ApiError — structured error with kind / code / source / cause
// ---------------------------------------------------------------------------

pub type ApiErrorCause = Arc<dyn std::error::Error + Send + Sync>;

#[derive(Debug)]
struct RetryAfterMsCause(u64);

impl std::fmt::Display for RetryAfterMsCause {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "retry_after_ms={}", self.0)
    }
}

impl std::error::Error for RetryAfterMsCause {}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ApiError {
    pub kind: ApiErrorKind,
    pub code: MatrixErrorCode,
    pub message: String,
    #[serde(skip)]
    pub source: Option<ErrorSource>,
    #[serde(skip)]
    pub cause: Option<ApiErrorCause>,
}

// --- PartialEq: compare kind, code, message only (cause is opaque) ---

impl PartialEq for ApiError {
    fn eq(&self, other: &Self) -> bool {
        self.kind == other.kind
            && self.code == other.code
            && self.message == other.message
            && self.source == other.source
    }
}

impl Eq for ApiError {}

// --- Display / Error ---

impl std::fmt::Display for ApiError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        if let Some(ref src) = self.source {
            write!(f, "{} {}: {}", src, self.code, self.message)
        } else {
            write!(f, "{}: {}", self.code, self.message)
        }
    }
}

impl std::error::Error for ApiError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        self.cause.as_ref().map(|c| c.as_ref() as &(dyn std::error::Error + 'static))
    }
}

// ---------------------------------------------------------------------------
// Constructors (backward-compatible factory methods)
// ---------------------------------------------------------------------------

impl ApiError {
    // -- core constructors --

    pub fn bad_request(message: impl Into<String>) -> Self {
        Self {
            kind: ApiErrorKind::BadRequest,
            code: MatrixErrorCode::BadJson,
            message: message.into(),
            source: None,
            cause: None,
        }
    }

    pub fn unauthorized(message: impl Into<String>) -> Self {
        Self {
            kind: ApiErrorKind::Unauthorized,
            code: MatrixErrorCode::Unauthorized,
            message: message.into(),
            source: None,
            cause: None,
        }
    }

    pub fn forbidden(message: impl Into<String>) -> Self {
        Self {
            kind: ApiErrorKind::Forbidden,
            code: MatrixErrorCode::Forbidden,
            message: message.into(),
            source: None,
            cause: None,
        }
    }

    pub fn not_found(message: impl Into<String>) -> Self {
        Self {
            kind: ApiErrorKind::NotFound,
            code: MatrixErrorCode::NotFound,
            message: message.into(),
            source: None,
            cause: None,
        }
    }

    pub fn not_implemented(message: impl Into<String>) -> Self {
        Self {
            kind: ApiErrorKind::NotImplemented,
            code: MatrixErrorCode::Unimplemented,
            message: message.into(),
            source: None,
            cause: None,
        }
    }

    pub fn conflict(message: impl Into<String>) -> Self {
        Self {
            kind: ApiErrorKind::Conflict,
            code: MatrixErrorCode::UserInUse,
            message: message.into(),
            source: None,
            cause: None,
        }
    }

    /// Conflict with a specific Matrix error code.
    pub fn conflict_with(code: MatrixErrorCode, message: impl Into<String>) -> Self {
        Self { kind: ApiErrorKind::Conflict, code, message: message.into(), source: None, cause: None }
    }

    pub fn internal(message: impl Into<String>) -> Self {
        Self {
            kind: ApiErrorKind::Internal,
            code: MatrixErrorCode::Unknown,
            message: message.into(),
            source: None,
            cause: None,
        }
    }

    /// Log an error and return an Internal error. The internal details are
    /// logged but only a generic message is returned to the client.
    pub fn internal_with_log(context: &str, err: &dyn std::fmt::Display) -> Self {
        tracing::error!(%context, %err, "internal error");
        Self {
            kind: ApiErrorKind::Internal,
            code: MatrixErrorCode::Unknown,
            message: format!("Internal error: {context}: {err}"),
            source: None,
            cause: None,
        }
    }

    /// Log a database error and return a generic Internal error.
    pub fn database_with_log(context: &str, err: &dyn std::fmt::Display) -> Self {
        tracing::error!(%context, %err, "database error");
        Self {
            kind: ApiErrorKind::Internal,
            code: MatrixErrorCode::Unknown,
            message: format!("Database error: {context}: {err}"),
            source: None,
            cause: None,
        }
    }

    pub fn database(message: impl Into<String>) -> Self {
        Self {
            kind: ApiErrorKind::Internal,
            code: MatrixErrorCode::Unknown,
            message: message.into(),
            source: None,
            cause: None,
        }
    }

    pub fn cache(message: impl Into<String>) -> Self {
        Self {
            kind: ApiErrorKind::Internal,
            code: MatrixErrorCode::Unknown,
            message: format!("Cache error: {}", message.into()),
            source: None,
            cause: None,
        }
    }

    pub fn gone(message: impl Into<String>) -> Self {
        Self {
            kind: ApiErrorKind::Gone,
            code: MatrixErrorCode::NotFound,
            message: message.into(),
            source: None,
            cause: None,
        }
    }

    pub fn authentication(message: impl Into<String>) -> Self {
        Self {
            kind: ApiErrorKind::Unauthorized,
            code: MatrixErrorCode::UnknownToken,
            message: message.into(),
            source: None,
            cause: None,
        }
    }

    pub fn validation(message: impl Into<String>) -> Self {
        Self {
            kind: ApiErrorKind::BadRequest,
            code: MatrixErrorCode::InvalidParam,
            message: message.into(),
            source: None,
            cause: None,
        }
    }

    pub fn invalid_input(message: impl Into<String>) -> Self {
        Self {
            kind: ApiErrorKind::BadRequest,
            code: MatrixErrorCode::InvalidParam,
            message: message.into(),
            source: None,
            cause: None,
        }
    }

    pub fn crypto(message: impl Into<String>) -> Self {
        Self {
            kind: ApiErrorKind::Internal,
            code: MatrixErrorCode::Unknown,
            message: message.into(),
            source: None,
            cause: None,
        }
    }

    pub fn rate_limited(_message: impl Into<String>) -> Self {
        Self {
            kind: ApiErrorKind::RateLimited,
            code: MatrixErrorCode::LimitExceeded,
            message: "Rate limited".to_string(),
            source: None,
            cause: None,
        }
    }

    pub fn rate_limited_with_retry(retry_after_ms: u64) -> Self {
        Self {
            kind: ApiErrorKind::RateLimited,
            code: MatrixErrorCode::LimitExceeded,
            message: "Rate limited".to_string(),
            source: None,
            cause: Some(Arc::new(RetryAfterMsCause(retry_after_ms))),
        }
    }

    pub fn missing_token() -> Self {
        Self {
            kind: ApiErrorKind::Unauthorized,
            code: MatrixErrorCode::MissingToken,
            message: "Missing access token".to_string(),
            source: None,
            cause: None,
        }
    }

    pub fn not_json(message: impl Into<String>) -> Self {
        Self {
            kind: ApiErrorKind::BadRequest,
            code: MatrixErrorCode::NotJson,
            message: message.into(),
            source: None,
            cause: None,
        }
    }

    // -- domain-specific constructors (delegate to core with specific code) --

    pub fn user_deactivated(message: impl Into<String>) -> Self {
        Self {
            kind: ApiErrorKind::Forbidden,
            code: MatrixErrorCode::UserDeactivated,
            message: message.into(),
            source: None,
            cause: None,
        }
    }

    pub fn invalid_username(message: impl Into<String>) -> Self {
        Self {
            kind: ApiErrorKind::BadRequest,
            code: MatrixErrorCode::InvalidUsername,
            message: message.into(),
            source: None,
            cause: None,
        }
    }

    pub fn user_in_use(message: impl Into<String>) -> Self {
        Self {
            kind: ApiErrorKind::Conflict,
            code: MatrixErrorCode::UserInUse,
            message: message.into(),
            source: None,
            cause: None,
        }
    }

    pub fn room_in_use(message: impl Into<String>) -> Self {
        Self {
            kind: ApiErrorKind::Conflict,
            code: MatrixErrorCode::RoomInUse,
            message: message.into(),
            source: None,
            cause: None,
        }
    }

    pub fn invalid_room_state(message: impl Into<String>) -> Self {
        Self {
            kind: ApiErrorKind::BadRequest,
            code: MatrixErrorCode::InvalidRoomState,
            message: message.into(),
            source: None,
            cause: None,
        }
    }

    pub fn threepid_in_use(message: impl Into<String>) -> Self {
        Self {
            kind: ApiErrorKind::Conflict,
            code: MatrixErrorCode::ThreepidInUse,
            message: message.into(),
            source: None,
            cause: None,
        }
    }

    pub fn threepid_not_found(message: impl Into<String>) -> Self {
        Self {
            kind: ApiErrorKind::BadRequest,
            code: MatrixErrorCode::ThreepidNotFound,
            message: message.into(),
            source: None,
            cause: None,
        }
    }

    pub fn threepid_auth_failed(message: impl Into<String>) -> Self {
        Self {
            kind: ApiErrorKind::Forbidden,
            code: MatrixErrorCode::ThreepidAuthFailed,
            message: message.into(),
            source: None,
            cause: None,
        }
    }

    pub fn threepid_denied(message: impl Into<String>) -> Self {
        Self {
            kind: ApiErrorKind::Forbidden,
            code: MatrixErrorCode::ThreepidDenied,
            message: message.into(),
            source: None,
            cause: None,
        }
    }

    pub fn server_not_trusted(message: impl Into<String>) -> Self {
        Self {
            kind: ApiErrorKind::Forbidden,
            code: MatrixErrorCode::ServerNotTrusted,
            message: message.into(),
            source: None,
            cause: None,
        }
    }

    pub fn unsupported_room_version(message: impl Into<String>) -> Self {
        Self {
            kind: ApiErrorKind::BadRequest,
            code: MatrixErrorCode::UnsupportedRoomVersion,
            message: message.into(),
            source: None,
            cause: None,
        }
    }

    pub fn incompatible_room_version(message: impl Into<String>) -> Self {
        Self {
            kind: ApiErrorKind::BadRequest,
            code: MatrixErrorCode::IncompatibleRoomVersion,
            message: message.into(),
            source: None,
            cause: None,
        }
    }

    pub fn bad_state(message: impl Into<String>) -> Self {
        Self {
            kind: ApiErrorKind::BadRequest,
            code: MatrixErrorCode::BadState,
            message: message.into(),
            source: None,
            cause: None,
        }
    }

    pub fn guest_access_forbidden(message: impl Into<String>) -> Self {
        Self {
            kind: ApiErrorKind::Forbidden,
            code: MatrixErrorCode::GuestAccessForbidden,
            message: message.into(),
            source: None,
            cause: None,
        }
    }

    pub fn captcha_needed(message: impl Into<String>) -> Self {
        Self {
            kind: ApiErrorKind::BadRequest,
            code: MatrixErrorCode::CaptchaNeeded,
            message: message.into(),
            source: None,
            cause: None,
        }
    }

    pub fn captcha_invalid(message: impl Into<String>) -> Self {
        Self {
            kind: ApiErrorKind::BadRequest,
            code: MatrixErrorCode::CaptchaInvalid,
            message: message.into(),
            source: None,
            cause: None,
        }
    }

    pub fn missing_param(message: impl Into<String>) -> Self {
        Self {
            kind: ApiErrorKind::BadRequest,
            code: MatrixErrorCode::MissingParam,
            message: message.into(),
            source: None,
            cause: None,
        }
    }

    pub fn invalid_param(message: impl Into<String>) -> Self {
        Self {
            kind: ApiErrorKind::BadRequest,
            code: MatrixErrorCode::InvalidParam,
            message: message.into(),
            source: None,
            cause: None,
        }
    }

    pub fn too_large(message: impl Into<String>) -> Self {
        Self {
            kind: ApiErrorKind::BadRequest,
            code: MatrixErrorCode::TooLarge,
            message: message.into(),
            source: None,
            cause: None,
        }
    }

    pub fn exclusive(message: impl Into<String>) -> Self {
        Self {
            kind: ApiErrorKind::Conflict,
            code: MatrixErrorCode::Exclusive,
            message: message.into(),
            source: None,
            cause: None,
        }
    }

    pub fn resource_limit_exceeded(message: impl Into<String>) -> Self {
        Self {
            kind: ApiErrorKind::Forbidden,
            code: MatrixErrorCode::ResourceLimitExceeded,
            message: message.into(),
            source: None,
            cause: None,
        }
    }

    pub fn cannot_leave_server_notice_room(message: impl Into<String>) -> Self {
        Self {
            kind: ApiErrorKind::Forbidden,
            code: MatrixErrorCode::CannotLeaveServerNoticeRoom,
            message: message.into(),
            source: None,
            cause: None,
        }
    }

    pub fn unknown(message: impl Into<String>) -> Self {
        Self {
            kind: ApiErrorKind::Internal,
            code: MatrixErrorCode::Unknown,
            message: message.into(),
            source: None,
            cause: None,
        }
    }

    pub fn unrecognized(message: impl Into<String>) -> Self {
        Self {
            kind: ApiErrorKind::NotFound,
            code: MatrixErrorCode::Unrecognized,
            message: message.into(),
            source: None,
            cause: None,
        }
    }

    pub fn request_timeout(message: impl Into<String>) -> Self {
        Self {
            kind: ApiErrorKind::Timeout,
            code: MatrixErrorCode::RequestTimeout,
            message: message.into(),
            source: None,
            cause: None,
        }
    }

    // -- encryption / decryption (map to Internal with specific message) --

    pub fn decryption_error(message: impl Into<String>) -> Self {
        Self {
            kind: ApiErrorKind::Internal,
            code: MatrixErrorCode::Unknown,
            message: message.into(),
            source: None,
            cause: None,
        }
    }

    pub fn encryption_error(message: impl Into<String>) -> Self {
        Self {
            kind: ApiErrorKind::Internal,
            code: MatrixErrorCode::Unknown,
            message: message.into(),
            source: None,
            cause: None,
        }
    }
}

// ---------------------------------------------------------------------------
// Builder / convenience methods
// ---------------------------------------------------------------------------

impl ApiError {
    /// Attach source location information.
    pub fn with_source(mut self, module: impl Into<String>, operation: impl Into<String>) -> Self {
        self.source = Some(ErrorSource::new(module, operation));
        self
    }

    /// Chain an underlying cause error.
    pub fn with_cause(mut self, cause: impl std::error::Error + Send + Sync + 'static) -> Self {
        self.cause = Some(Arc::new(cause));
        self
    }

    /// Override the Matrix error code (use sparingly).
    pub fn with_code(mut self, code: MatrixErrorCode) -> Self {
        self.code = code;
        self
    }

    // -- kind predicates (replace match / matches!) --

    pub fn is_bad_request(&self) -> bool {
        self.kind == ApiErrorKind::BadRequest
    }
    pub fn is_unauthorized(&self) -> bool {
        self.kind == ApiErrorKind::Unauthorized
    }
    pub fn is_forbidden(&self) -> bool {
        self.kind == ApiErrorKind::Forbidden
    }
    pub fn is_not_found(&self) -> bool {
        self.kind == ApiErrorKind::NotFound
    }
    pub fn is_conflict(&self) -> bool {
        self.kind == ApiErrorKind::Conflict
    }
    pub fn is_gone(&self) -> bool {
        self.kind == ApiErrorKind::Gone
    }
    pub fn is_rate_limited(&self) -> bool {
        self.kind == ApiErrorKind::RateLimited
    }
    pub fn is_internal(&self) -> bool {
        self.kind == ApiErrorKind::Internal
    }
    pub fn is_not_implemented(&self) -> bool {
        self.kind == ApiErrorKind::NotImplemented
    }
    pub fn is_timeout(&self) -> bool {
        self.kind == ApiErrorKind::Timeout
    }

    // -- code predicates (for specific Matrix error code checks) --

    pub fn code_is(&self, code: MatrixErrorCode) -> bool {
        self.code == code
    }

    // -- accessors --

    /// Returns a reference to the Matrix error code.
    pub fn code(&self) -> &MatrixErrorCode {
        &self.code
    }

    pub fn matrix_code(&self) -> MatrixErrorCode {
        self.code
    }

    pub fn code_str(&self) -> &'static str {
        self.code.as_str()
    }

    /// Returns the user-facing error message.
    pub fn message(&self) -> String {
        match self.kind {
            ApiErrorKind::Internal => {
                tracing::error!(
                    kind = ?self.kind,
                    code = %self.code,
                    message = %self.message,
                    source = ?self.source,
                    "Internal error returned to client"
                );
                "An internal error occurred".to_string()
            }
            _ => self.message.clone(),
        }
    }

    /// Returns the full internal message (for logging, never shown to clients).
    pub fn internal_message(&self) -> &str {
        &self.message
    }

    pub fn http_status(&self) -> StatusCode {
        self.code.http_status()
    }

    pub fn retry_after_ms(&self) -> Option<u64> {
        if self.kind == ApiErrorKind::RateLimited {
            self.cause
                .as_ref()
                .and_then(|cause| cause.downcast_ref::<RetryAfterMsCause>().map(|retry| retry.0))
                .or(Some(5000))
        } else {
            None
        }
    }
}

// ---------------------------------------------------------------------------
// IntoResponse (Axum integration)
// ---------------------------------------------------------------------------

impl IntoResponse for ApiError {
    fn into_response(self) -> Response {
        if self.kind == ApiErrorKind::Gone {
            let status_code = StatusCode::GONE;
            return (
                status_code,
                Json(
                    serde_json::from_str::<serde_json::Value>(&self.message)
                        .unwrap_or_else(|_| json!({"errcode": self.code_str(), "error": self.message})),
                ),
            )
                .into_response();
        }

        let errcode = self.code_str().to_string();
        let error_msg = self.message();
        let status_code = self.code.http_status();

        // Emit metrics
        if let Some(collector) = ERROR_METRICS.get() {
            let metric_name = format!("http_errors_total_{}", errcode);
            if let Some(counter) = collector.get_counter(&metric_name) {
                counter.inc();
            } else {
                let mut labels = std::collections::HashMap::new();
                labels.insert("errcode".to_string(), errcode.clone());
                let counter = collector.register_counter_with_labels(metric_name, labels);
                counter.inc();
            }
        }

        let retry_after_ms = self.retry_after_ms();

        let mut body = json!({
            "errcode": errcode,
            "error": error_msg
        });

        if let Some(ms) = retry_after_ms {
            body["retry_after_ms"] = json!(ms);
        }
        let mut response = (status_code, Json(body)).into_response();
        if let Some(ms) = retry_after_ms {
            let retry_after_seconds = ms.saturating_add(999) / 1000;
            if let Ok(value) = HeaderValue::from_str(&retry_after_seconds.to_string()) {
                response.headers_mut().insert("retry-after", value);
            }
            if let Ok(value) = HeaderValue::from_str(&ms.to_string()) {
                response.headers_mut().insert("x-ratelimit-retry-after", value.clone());
                response.headers_mut().insert("x-ratelimit-after", value);
            }
        }
        response
    }
}

// ---------------------------------------------------------------------------
// ErrorContext — backward-compatible context enrichment
// ---------------------------------------------------------------------------

pub trait ErrorContext {
    fn with_context(self, module: &str, operation: &str) -> Self;
}

impl ErrorContext for ApiError {
    fn with_context(self, module: &str, operation: &str) -> Self {
        if self.kind == ApiErrorKind::Internal || self.kind == ApiErrorKind::BadRequest {
            let src = self.source.map_or_else(
                || ErrorSource::new(module, operation),
                |s| ErrorSource::new(format!("{}::{}", module, s.module), format!("{}::{}", operation, s.operation)),
            );
            Self { source: Some(src), ..self }
        } else {
            self
        }
    }
}

// ---------------------------------------------------------------------------
// Error macros (updated for struct)
// ---------------------------------------------------------------------------

#[macro_export]
macro_rules! dbg_context {
    ($result:expr, $module:expr, $operation:expr) => {
        $result.map_err(|e| e.with_context($module, $operation))
    };
}

#[macro_export]
macro_rules! safe_unwrap {
    ($option:expr, $msg:expr) => {
        match $option {
            Some(v) => Ok(v),
            None => Err($crate::error::ApiError::internal($msg)),
        }
    };
    ($option:expr, $msg:expr, $($arg:tt)*) => {
        match $option {
            Some(v) => Ok(v),
            None => Err($crate::error::ApiError::internal(format!($msg, $($arg)*))),
        }
    };
}

#[macro_export]
macro_rules! safe_unwrap_ctx {
    ($option:expr, $module:expr, $operation:expr) => {
        match $option {
            Some(v) => Ok(v),
            None => {
                Err($crate::error::ApiError::internal(format!("[{}::{}] Unexpected None value", $module, $operation)))
            }
        }
    };
}

#[macro_export]
macro_rules! wrap_result {
    ($result:expr, $module:expr, $operation:expr) => {
        $result.map_err(|e| $crate::error::ApiError::internal(format!("[{}::{}] {}", $module, $operation, e)))
    };
}

#[macro_export]
macro_rules! bail {
    ($msg:expr) => {
        return Err($crate::error::ApiError::internal($msg))
    };
    ($msg:expr, $($arg:tt)*) => {
        return Err($crate::error::ApiError::internal(format!($msg, $($arg)*)))
    };
}

#[macro_export]
macro_rules! ensure {
    ($cond:expr, $msg:expr) => {
        if !($cond) {
            return Err($crate::error::ApiError::bad_request($msg));
        }
    };
    ($cond:expr, $msg:expr, $($arg:tt)*) => {
        if !($cond) {
            return Err($crate::error::ApiError::bad_request(format!($msg, $($arg)*)));
        }
    };
}

#[macro_export]
macro_rules! ensure_forbidden {
    ($cond:expr, $msg:expr) => {
        if !($cond) {
            return Err($crate::error::ApiError::forbidden($msg));
        }
    };
}

#[macro_export]
macro_rules! ensure_not_found {
    ($cond:expr, $msg:expr) => {
        if !($cond) {
            return Err($crate::error::ApiError::not_found($msg));
        }
    };
}

#[macro_export]
macro_rules! ensure_unauthorized {
    ($cond:expr, $msg:expr) => {
        if !($cond) {
            return Err($crate::error::ApiError::unauthorized($msg));
        }
    };
}

// ---------------------------------------------------------------------------
// From impls (error conversion)
// ---------------------------------------------------------------------------

impl From<sqlx::Error> for ApiError {
    fn from(err: sqlx::Error) -> Self {
        let error_msg = format!("{err:?}").to_lowercase();
        if error_msg.contains("duplicate key")
            || error_msg.contains("unique constraint")
            || error_msg.contains("23505")
            || error_msg.contains("violates unique constraint")
        {
            tracing::error!(%err, "duplicate database entry");
            ApiError::bad_request("A duplicate entry was found")
        } else {
            tracing::error!(%err, "database error");
            ApiError::database("A database error occurred")
        }
    }
}

impl From<redis::RedisError> for ApiError {
    fn from(err: redis::RedisError) -> Self {
        ApiError::cache(err.to_string())
    }
}

impl From<serde_json::Error> for ApiError {
    fn from(err: serde_json::Error) -> Self {
        ApiError::not_json(err.to_string())
    }
}

impl From<std::string::FromUtf8Error> for ApiError {
    fn from(_err: std::string::FromUtf8Error) -> Self {
        ApiError::validation("Invalid UTF-8 encoding")
    }
}

impl From<std::num::ParseIntError> for ApiError {
    fn from(_err: std::num::ParseIntError) -> Self {
        ApiError::validation("Invalid number format")
    }
}

impl From<std::io::Error> for ApiError {
    fn from(err: std::io::Error) -> Self {
        ApiError::internal(err.to_string())
    }
}

// ---------------------------------------------------------------------------
// ApiResponse (unchanged)
// ---------------------------------------------------------------------------

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
        Self { status: "ok".to_string(), data: Some(data), error: None, errcode: None, retry_after_ms: None }
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
                Some("M_UNAUTHORIZED") | Some("M_UNKNOWN_TOKEN") | Some("M_MISSING_TOKEN") => StatusCode::UNAUTHORIZED,
                Some("M_LIMIT_EXCEEDED") => StatusCode::TOO_MANY_REQUESTS,
                Some("M_UNRECOGNIZED") => StatusCode::NOT_FOUND,
                Some("M_BAD_JSON")
                | Some("M_NOT_JSON")
                | Some("M_INVALID_PARAM")
                | Some("M_MISSING_PARAM")
                | Some("M_INVALID_USERNAME")
                | Some("M_BAD_STATE")
                | Some("M_INVALID_ROOM_STATE") => StatusCode::BAD_REQUEST,
                Some("M_USER_IN_USE") | Some("M_ROOM_IN_USE") | Some("M_THREEPID_IN_USE") => StatusCode::CONFLICT,
                Some("M_TOO_LARGE") => StatusCode::PAYLOAD_TOO_LARGE,
                Some("M_SERVER_NOT_TRUSTED") => StatusCode::BAD_GATEWAY,
                _ => StatusCode::INTERNAL_SERVER_ERROR,
            }
        };
        (status_code, Json(self)).into_response()
    }
}

// ---------------------------------------------------------------------------
// Type alias
// ---------------------------------------------------------------------------

pub type ApiResult<T> = Result<T, ApiError>;
