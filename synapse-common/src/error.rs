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
            Self::Unrecognized => StatusCode::BAD_REQUEST,
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
        self.kind.default_http_status()
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
        // Use `kind` (explicit HTTP semantic kind) for the HTTP status code,
        // and `code` (Matrix error code) only for the `errcode` JSON field.
        // This lets e.g. `ApiError::unrecognized()` (kind=NotFound, code=Unrecognized)
        // correctly return 404 while keeping `M_UNRECOGNIZED` in the body.
        let status_code = self.kind.default_http_status();

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
// with_context — backward-compatible context enrichment
// ---------------------------------------------------------------------------

impl ApiError {
    pub fn with_context(self, module: &str, operation: &str) -> Self {
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
                Some("M_UNRECOGNIZED") => StatusCode::BAD_REQUEST,
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

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use std::error::Error as _;

    // -----------------------------------------------------------------------
    // ApiError construction and Display
    // -----------------------------------------------------------------------

    #[test]
    fn test_api_error_bad_request_construction() {
        let err = ApiError::bad_request("invalid input");
        assert_eq!(err.kind, ApiErrorKind::BadRequest);
        assert_eq!(err.code, MatrixErrorCode::BadJson);
        assert_eq!(err.message, "invalid input");
        assert!(err.source.is_none());
        assert!(err.cause.is_none());
    }

    #[test]
    fn test_api_error_unauthorized_construction() {
        let err = ApiError::unauthorized("bad token");
        assert_eq!(err.kind, ApiErrorKind::Unauthorized);
        assert_eq!(err.code, MatrixErrorCode::Unauthorized);
        assert_eq!(err.message, "bad token");
    }

    #[test]
    fn test_api_error_forbidden_construction() {
        let err = ApiError::forbidden("no access");
        assert_eq!(err.kind, ApiErrorKind::Forbidden);
        assert_eq!(err.code, MatrixErrorCode::Forbidden);
    }

    #[test]
    fn test_api_error_not_found_construction() {
        let err = ApiError::not_found("missing resource");
        assert_eq!(err.kind, ApiErrorKind::NotFound);
        assert_eq!(err.code, MatrixErrorCode::NotFound);
    }

    #[test]
    fn test_api_error_internal_construction() {
        let err = ApiError::internal("bug");
        assert_eq!(err.kind, ApiErrorKind::Internal);
        assert_eq!(err.code, MatrixErrorCode::Unknown);
    }

    #[test]
    fn test_api_error_not_implemented_construction() {
        let err = ApiError::not_implemented("not done yet");
        assert_eq!(err.kind, ApiErrorKind::NotImplemented);
        assert_eq!(err.code, MatrixErrorCode::Unimplemented);
    }

    #[test]
    fn test_api_error_conflict_construction() {
        let err = ApiError::conflict("duplicate");
        assert_eq!(err.kind, ApiErrorKind::Conflict);
        assert_eq!(err.code, MatrixErrorCode::UserInUse);
    }

    #[test]
    fn test_api_error_conflict_with_construction() {
        let err = ApiError::conflict_with(MatrixErrorCode::RoomInUse, "room in use");
        assert_eq!(err.kind, ApiErrorKind::Conflict);
        assert_eq!(err.code, MatrixErrorCode::RoomInUse);
        assert_eq!(err.message, "room in use");
    }

    #[test]
    fn test_api_error_rate_limited_construction() {
        let err = ApiError::rate_limited("too fast");
        assert_eq!(err.kind, ApiErrorKind::RateLimited);
        assert_eq!(err.code, MatrixErrorCode::LimitExceeded);
        assert_eq!(err.message, "Rate limited");
    }

    #[test]
    fn test_api_error_rate_limited_with_retry_construction() {
        let err = ApiError::rate_limited_with_retry(3000);
        assert_eq!(err.kind, ApiErrorKind::RateLimited);
        assert_eq!(err.code, MatrixErrorCode::LimitExceeded);
        assert_eq!(err.message, "Rate limited");
        assert!(err.cause.is_some());
    }

    #[test]
    fn test_api_error_missing_token() {
        let err = ApiError::missing_token();
        assert_eq!(err.kind, ApiErrorKind::Unauthorized);
        assert_eq!(err.code, MatrixErrorCode::MissingToken);
        assert_eq!(err.message, "Missing access token");
    }

    #[test]
    fn test_api_error_not_json() {
        let err = ApiError::not_json("not json data");
        assert_eq!(err.kind, ApiErrorKind::BadRequest);
        assert_eq!(err.code, MatrixErrorCode::NotJson);
        assert_eq!(err.message, "not json data");
    }

    #[test]
    fn test_api_error_gone_construction() {
        let err = ApiError::gone("resource deleted");
        assert_eq!(err.kind, ApiErrorKind::Gone);
        assert_eq!(err.code, MatrixErrorCode::NotFound);
    }

    #[test]
    fn test_api_error_authentication() {
        let err = ApiError::authentication("bad credentials");
        assert_eq!(err.kind, ApiErrorKind::Unauthorized);
        assert_eq!(err.code, MatrixErrorCode::UnknownToken);
    }

    #[test]
    fn test_api_error_validation() {
        let err = ApiError::validation("invalid field");
        assert_eq!(err.kind, ApiErrorKind::BadRequest);
        assert_eq!(err.code, MatrixErrorCode::InvalidParam);
    }

    #[test]
    fn test_api_error_database() {
        let err = ApiError::database("db connection failed");
        assert_eq!(err.kind, ApiErrorKind::Internal);
        assert_eq!(err.code, MatrixErrorCode::Unknown);
    }

    #[test]
    fn test_api_error_cache() {
        let err = ApiError::cache("redis down");
        assert_eq!(err.kind, ApiErrorKind::Internal);
        assert_eq!(err.message, "Cache error: redis down");
    }

    // -----------------------------------------------------------------------
    // ApiError Display formatting
    // -----------------------------------------------------------------------

    #[test]
    fn test_api_error_display_without_source() {
        let err = ApiError::bad_request("test message");
        assert_eq!(format!("{}", err), "M_BAD_JSON: test message");
    }

    #[test]
    fn test_api_error_display_with_source() {
        let err = ApiError::bad_request("test message").with_source("storage::room", "get_messages");
        assert_eq!(format!("{}", err), "[storage::room::get_messages] M_BAD_JSON: test message");
    }

    #[test]
    fn test_api_error_display_internal_message() {
        let err = ApiError::internal("something went wrong");
        assert_eq!(format!("{}", err), "M_UNKNOWN: something went wrong");
    }

    #[test]
    fn test_api_error_debug_not_empty() {
        let err = ApiError::forbidden("debug test");
        assert!(!format!("{:?}", err).is_empty());
    }

    #[test]
    fn test_api_error_display_contains_message() {
        let err = ApiError::not_found("resource not found");
        assert!(format!("{}", err).contains("resource not found"));
        assert!(format!("{}", err).contains("M_NOT_FOUND"));
    }

    // -----------------------------------------------------------------------
    // ApiError PartialEq
    // -----------------------------------------------------------------------

    #[test]
    fn test_api_error_partial_eq_equal() {
        let err1 = ApiError::bad_request("msg");
        let err2 = ApiError::bad_request("msg");
        assert_eq!(err1, err2);
    }

    #[test]
    fn test_api_error_partial_eq_different_kind() {
        let err1 = ApiError::bad_request("msg");
        let err2 = ApiError::forbidden("msg");
        assert_ne!(err1, err2);
    }

    #[test]
    fn test_api_error_partial_eq_different_message() {
        let err1 = ApiError::bad_request("msg one");
        let err2 = ApiError::bad_request("msg two");
        assert_ne!(err1, err2);
    }

    #[test]
    fn test_api_error_partial_eq_same_source() {
        let err1 = ApiError::bad_request("msg").with_source("mod", "op");
        let err2 = ApiError::bad_request("msg").with_source("mod", "op");
        assert_eq!(err1, err2);
    }

    #[test]
    fn test_api_error_partial_eq_different_source() {
        let err1 = ApiError::bad_request("msg").with_source("mod", "op1");
        let err2 = ApiError::bad_request("msg").with_source("mod", "op2");
        assert_ne!(err1, err2);
    }

    // -----------------------------------------------------------------------
    // ApiError builder methods
    // -----------------------------------------------------------------------

    #[test]
    fn test_api_error_with_source() {
        let err = ApiError::bad_request("msg").with_source("my_module", "my_operation");
        let src = err.source.expect("source should be set");
        assert_eq!(src.module, "my_module");
        assert_eq!(src.operation, "my_operation");
    }

    #[test]
    fn test_api_error_with_cause() {
        let cause = std::io::Error::new(std::io::ErrorKind::Other, "underlying cause");
        let err = ApiError::internal("msg").with_cause(cause);
        assert!(err.cause.is_some());
        assert!(err.source().is_some());
    }

    #[test]
    fn test_api_error_with_code() {
        let err = ApiError::bad_request("msg").with_code(MatrixErrorCode::InvalidParam);
        assert_eq!(err.code, MatrixErrorCode::InvalidParam);
    }

    // -----------------------------------------------------------------------
    // ApiError predicate methods
    // -----------------------------------------------------------------------

    #[test]
    fn test_api_error_is_bad_request() {
        assert!(ApiError::bad_request("msg").is_bad_request());
        assert!(!ApiError::forbidden("msg").is_bad_request());
    }

    #[test]
    fn test_api_error_is_unauthorized() {
        assert!(ApiError::unauthorized("msg").is_unauthorized());
        assert!(!ApiError::bad_request("msg").is_unauthorized());
    }

    #[test]
    fn test_api_error_is_forbidden() {
        assert!(ApiError::forbidden("msg").is_forbidden());
        assert!(!ApiError::bad_request("msg").is_forbidden());
    }

    #[test]
    fn test_api_error_is_not_found() {
        assert!(ApiError::not_found("msg").is_not_found());
        assert!(!ApiError::bad_request("msg").is_not_found());
    }

    #[test]
    fn test_api_error_is_conflict() {
        assert!(ApiError::conflict("msg").is_conflict());
        assert!(!ApiError::bad_request("msg").is_conflict());
    }

    #[test]
    fn test_api_error_is_gone() {
        assert!(ApiError::gone("msg").is_gone());
        assert!(!ApiError::bad_request("msg").is_gone());
    }

    #[test]
    fn test_api_error_is_rate_limited() {
        assert!(ApiError::rate_limited("msg").is_rate_limited());
        assert!(!ApiError::bad_request("msg").is_rate_limited());
    }

    #[test]
    fn test_api_error_is_internal() {
        assert!(ApiError::internal("msg").is_internal());
        assert!(!ApiError::bad_request("msg").is_internal());
    }

    #[test]
    fn test_api_error_is_not_implemented() {
        assert!(ApiError::not_implemented("msg").is_not_implemented());
        assert!(!ApiError::bad_request("msg").is_not_implemented());
    }

    #[test]
    fn test_api_error_is_timeout() {
        assert!(ApiError::request_timeout("msg").is_timeout());
        assert!(!ApiError::bad_request("msg").is_timeout());
    }

    #[test]
    fn test_api_error_code_is() {
        let err = ApiError::bad_request("msg");
        assert!(err.code_is(MatrixErrorCode::BadJson));
        assert!(!err.code_is(MatrixErrorCode::NotFound));
    }

    // -----------------------------------------------------------------------
    // ApiError accessor methods
    // -----------------------------------------------------------------------

    #[test]
    fn test_api_error_code_accessor() {
        let err = ApiError::forbidden("msg");
        assert_eq!(err.code(), &MatrixErrorCode::Forbidden);
        assert_eq!(err.matrix_code(), MatrixErrorCode::Forbidden);
        assert_eq!(err.code_str(), "M_FORBIDDEN");
    }

    #[test]
    fn test_api_error_message_for_internal() {
        let err = ApiError::internal("secret details");
        // `message()` masks the internal details for clients
        assert_eq!(err.message(), "An internal error occurred");
    }

    #[test]
    fn test_api_error_message_for_non_internal() {
        let err = ApiError::bad_request("bad input");
        assert_eq!(err.message(), "bad input");
    }

    #[test]
    fn test_api_error_internal_message() {
        let err = ApiError::internal("secret details");
        assert_eq!(err.internal_message(), "secret details");
    }

    #[test]
    fn test_api_error_http_status() {
        let err = ApiError::not_found("gone");
        assert_eq!(err.http_status(), StatusCode::NOT_FOUND);

        let err = ApiError::forbidden("no access");
        assert_eq!(err.http_status(), StatusCode::FORBIDDEN);
    }

    #[test]
    fn test_api_error_retry_after_ms_for_rate_limited_without_cause() {
        let err = ApiError::rate_limited("slow down");
        assert_eq!(err.retry_after_ms(), Some(5000));
    }

    #[test]
    fn test_api_error_retry_after_ms_for_rate_limited_with_retry() {
        let err = ApiError::rate_limited_with_retry(3000);
        assert_eq!(err.retry_after_ms(), Some(3000));
    }

    #[test]
    fn test_api_error_retry_after_ms_for_non_rate_limited() {
        let err = ApiError::bad_request("nope");
        assert_eq!(err.retry_after_ms(), None);
    }

    #[test]
    fn test_api_error_domain_constructors() {
        // user_deactivated
        let err = ApiError::user_deactivated("account disabled");
        assert_eq!(err.kind, ApiErrorKind::Forbidden);
        assert_eq!(err.code, MatrixErrorCode::UserDeactivated);

        // invalid_username
        let err = ApiError::invalid_username("bad chars");
        assert_eq!(err.kind, ApiErrorKind::BadRequest);
        assert_eq!(err.code, MatrixErrorCode::InvalidUsername);

        // user_in_use
        let err = ApiError::user_in_use("user taken");
        assert_eq!(err.kind, ApiErrorKind::Conflict);
        assert_eq!(err.code, MatrixErrorCode::UserInUse);

        // room_in_use
        let err = ApiError::room_in_use("room in use");
        assert_eq!(err.kind, ApiErrorKind::Conflict);
        assert_eq!(err.code, MatrixErrorCode::RoomInUse);

        // invalid_room_state
        let err = ApiError::invalid_room_state("wrong state");
        assert_eq!(err.kind, ApiErrorKind::BadRequest);
        assert_eq!(err.code, MatrixErrorCode::InvalidRoomState);

        // threepid_in_use
        let err = ApiError::threepid_in_use("email in use");
        assert_eq!(err.kind, ApiErrorKind::Conflict);
        assert_eq!(err.code, MatrixErrorCode::ThreepidInUse);

        // threepid_not_found
        let err = ApiError::threepid_not_found("email not found");
        assert_eq!(err.kind, ApiErrorKind::BadRequest);
        assert_eq!(err.code, MatrixErrorCode::ThreepidNotFound);

        // threepid_auth_failed
        let err = ApiError::threepid_auth_failed("auth failed");
        assert_eq!(err.kind, ApiErrorKind::Forbidden);
        assert_eq!(err.code, MatrixErrorCode::ThreepidAuthFailed);

        // threepid_denied
        let err = ApiError::threepid_denied("denied");
        assert_eq!(err.kind, ApiErrorKind::Forbidden);
        assert_eq!(err.code, MatrixErrorCode::ThreepidDenied);

        // server_not_trusted
        let err = ApiError::server_not_trusted("untrusted");
        assert_eq!(err.kind, ApiErrorKind::Forbidden);
        assert_eq!(err.code, MatrixErrorCode::ServerNotTrusted);

        // unsupported_room_version
        let err = ApiError::unsupported_room_version("bad version");
        assert_eq!(err.kind, ApiErrorKind::BadRequest);
        assert_eq!(err.code, MatrixErrorCode::UnsupportedRoomVersion);

        // incompatible_room_version
        let err = ApiError::incompatible_room_version("wrong version");
        assert_eq!(err.kind, ApiErrorKind::BadRequest);
        assert_eq!(err.code, MatrixErrorCode::IncompatibleRoomVersion);

        // bad_state
        let err = ApiError::bad_state("bad state");
        assert_eq!(err.kind, ApiErrorKind::BadRequest);
        assert_eq!(err.code, MatrixErrorCode::BadState);

        // guest_access_forbidden
        let err = ApiError::guest_access_forbidden("guest blocked");
        assert_eq!(err.kind, ApiErrorKind::Forbidden);
        assert_eq!(err.code, MatrixErrorCode::GuestAccessForbidden);

        // captcha_needed
        let err = ApiError::captcha_needed("captcha required");
        assert_eq!(err.kind, ApiErrorKind::BadRequest);
        assert_eq!(err.code, MatrixErrorCode::CaptchaNeeded);

        // captcha_invalid
        let err = ApiError::captcha_invalid("wrong captcha");
        assert_eq!(err.kind, ApiErrorKind::BadRequest);
        assert_eq!(err.code, MatrixErrorCode::CaptchaInvalid);

        // missing_param
        let err = ApiError::missing_param("missing field");
        assert_eq!(err.kind, ApiErrorKind::BadRequest);
        assert_eq!(err.code, MatrixErrorCode::MissingParam);

        // invalid_param
        let err = ApiError::invalid_param("bad field");
        assert_eq!(err.kind, ApiErrorKind::BadRequest);
        assert_eq!(err.code, MatrixErrorCode::InvalidParam);

        // too_large
        let err = ApiError::too_large("too big");
        assert_eq!(err.kind, ApiErrorKind::BadRequest);
        assert_eq!(err.code, MatrixErrorCode::TooLarge);

        // exclusive
        let err = ApiError::exclusive("exclusive");
        assert_eq!(err.kind, ApiErrorKind::Conflict);
        assert_eq!(err.code, MatrixErrorCode::Exclusive);

        // resource_limit_exceeded
        let err = ApiError::resource_limit_exceeded("over limit");
        assert_eq!(err.kind, ApiErrorKind::Forbidden);
        assert_eq!(err.code, MatrixErrorCode::ResourceLimitExceeded);

        // cannot_leave_server_notice_room
        let err = ApiError::cannot_leave_server_notice_room("stuck");
        assert_eq!(err.kind, ApiErrorKind::Forbidden);
        assert_eq!(err.code, MatrixErrorCode::CannotLeaveServerNoticeRoom);

        // unknown
        let err = ApiError::unknown("unknown error");
        assert_eq!(err.kind, ApiErrorKind::Internal);
        assert_eq!(err.code, MatrixErrorCode::Unknown);

        // unrecognized
        let err = ApiError::unrecognized("unrecognized");
        assert_eq!(err.kind, ApiErrorKind::NotFound);
        assert_eq!(err.code, MatrixErrorCode::Unrecognized);

        // request_timeout
        let err = ApiError::request_timeout("timed out");
        assert_eq!(err.kind, ApiErrorKind::Timeout);
        assert_eq!(err.code, MatrixErrorCode::RequestTimeout);

        // decryption_error
        let err = ApiError::decryption_error("decrypt failed");
        assert_eq!(err.kind, ApiErrorKind::Internal);
        assert_eq!(err.code, MatrixErrorCode::Unknown);

        // encryption_error
        let err = ApiError::encryption_error("encrypt failed");
        assert_eq!(err.kind, ApiErrorKind::Internal);
        assert_eq!(err.code, MatrixErrorCode::Unknown);
    }

    // -----------------------------------------------------------------------
    // MatrixErrorCode::as_str()
    // -----------------------------------------------------------------------

    #[test]
    fn test_matrix_error_code_as_str() {
        assert_eq!(MatrixErrorCode::Forbidden.as_str(), "M_FORBIDDEN");
        assert_eq!(MatrixErrorCode::Unknown.as_str(), "M_UNKNOWN");
        assert_eq!(MatrixErrorCode::NotFound.as_str(), "M_NOT_FOUND");
        assert_eq!(MatrixErrorCode::BadJson.as_str(), "M_BAD_JSON");
        assert_eq!(MatrixErrorCode::Unauthorized.as_str(), "M_UNAUTHORIZED");
        assert_eq!(MatrixErrorCode::Unimplemented.as_str(), "M_UNRECOGNIZED");
        assert_eq!(MatrixErrorCode::UserDeactivated.as_str(), "M_USER_DEACTIVATED");
        assert_eq!(MatrixErrorCode::LimitExceeded.as_str(), "M_LIMIT_EXCEEDED");
        assert_eq!(MatrixErrorCode::RequestTimeout.as_str(), "M_REQUEST_TIMEOUT");
    }

    #[test]
    fn test_matrix_error_code_as_str_all_variants() {
        // Spot-check that every variant returns a non-empty M_ prefixed string
        let codes = [
            MatrixErrorCode::Forbidden,
            MatrixErrorCode::UnknownToken,
            MatrixErrorCode::MissingToken,
            MatrixErrorCode::BadJson,
            MatrixErrorCode::NotJson,
            MatrixErrorCode::NotFound,
            MatrixErrorCode::LimitExceeded,
            MatrixErrorCode::Unknown,
            MatrixErrorCode::Unrecognized,
            MatrixErrorCode::Unauthorized,
            MatrixErrorCode::UserDeactivated,
            MatrixErrorCode::UserInUse,
            MatrixErrorCode::InvalidUsername,
            MatrixErrorCode::RoomInUse,
            MatrixErrorCode::InvalidRoomState,
            MatrixErrorCode::ThreepidInUse,
            MatrixErrorCode::ThreepidNotFound,
            MatrixErrorCode::ThreepidAuthFailed,
            MatrixErrorCode::ThreepidDenied,
            MatrixErrorCode::ServerNotTrusted,
            MatrixErrorCode::UnsupportedRoomVersion,
            MatrixErrorCode::IncompatibleRoomVersion,
            MatrixErrorCode::BadState,
            MatrixErrorCode::GuestAccessForbidden,
            MatrixErrorCode::CaptchaNeeded,
            MatrixErrorCode::CaptchaInvalid,
            MatrixErrorCode::MissingParam,
            MatrixErrorCode::InvalidParam,
            MatrixErrorCode::TooLarge,
            MatrixErrorCode::Exclusive,
            MatrixErrorCode::ResourceLimitExceeded,
            MatrixErrorCode::CannotLeaveServerNoticeRoom,
            MatrixErrorCode::Unimplemented,
            MatrixErrorCode::RequestTimeout,
        ];
        for code in &codes {
            let s = code.as_str();
            assert!(s.starts_with("M_"), "as_str() for {code:?} did not start with M_: got {s}");
            assert!(!s.is_empty(), "as_str() for {code:?} returned empty string");
        }
    }

    // -----------------------------------------------------------------------
    // MatrixErrorCode::http_status()
    // -----------------------------------------------------------------------

    #[test]
    fn test_matrix_error_code_http_status() {
        assert_eq!(MatrixErrorCode::Forbidden.http_status(), StatusCode::FORBIDDEN);
        assert_eq!(MatrixErrorCode::NotFound.http_status(), StatusCode::NOT_FOUND);
        assert_eq!(MatrixErrorCode::BadJson.http_status(), StatusCode::BAD_REQUEST);
        assert_eq!(MatrixErrorCode::LimitExceeded.http_status(), StatusCode::TOO_MANY_REQUESTS);
        assert_eq!(MatrixErrorCode::Unimplemented.http_status(), StatusCode::NOT_IMPLEMENTED);
        assert_eq!(MatrixErrorCode::Unknown.http_status(), StatusCode::INTERNAL_SERVER_ERROR);
        assert_eq!(MatrixErrorCode::Unauthorized.http_status(), StatusCode::UNAUTHORIZED);
        assert_eq!(MatrixErrorCode::ServerNotTrusted.http_status(), StatusCode::BAD_GATEWAY);
        assert_eq!(MatrixErrorCode::TooLarge.http_status(), StatusCode::PAYLOAD_TOO_LARGE);
        assert_eq!(MatrixErrorCode::RequestTimeout.http_status(), StatusCode::REQUEST_TIMEOUT);
        assert_eq!(MatrixErrorCode::UserInUse.http_status(), StatusCode::CONFLICT);
    }

    #[test]
    fn test_matrix_error_code_http_status_all_variants() {
        // Verify every variant's http_status matches its expected grouping
        let auth_codes = [MatrixErrorCode::UnknownToken, MatrixErrorCode::MissingToken, MatrixErrorCode::Unauthorized];
        for code in &auth_codes {
            assert_eq!(code.http_status(), StatusCode::UNAUTHORIZED, "{code:?} should be UNAUTHORIZED");
        }

        let bad_request_codes = [
            MatrixErrorCode::BadJson,
            MatrixErrorCode::NotJson,
            MatrixErrorCode::Unrecognized,
            MatrixErrorCode::InvalidUsername,
            MatrixErrorCode::InvalidRoomState,
            MatrixErrorCode::UnsupportedRoomVersion,
            MatrixErrorCode::IncompatibleRoomVersion,
            MatrixErrorCode::BadState,
            MatrixErrorCode::CaptchaNeeded,
            MatrixErrorCode::CaptchaInvalid,
            MatrixErrorCode::MissingParam,
            MatrixErrorCode::InvalidParam,
            MatrixErrorCode::ThreepidNotFound,
        ];
        for code in &bad_request_codes {
            assert_eq!(code.http_status(), StatusCode::BAD_REQUEST, "{code:?} should be BAD_REQUEST");
        }

        let forbidden_codes = [
            MatrixErrorCode::Forbidden,
            MatrixErrorCode::UserDeactivated,
            MatrixErrorCode::ThreepidAuthFailed,
            MatrixErrorCode::ThreepidDenied,
            MatrixErrorCode::GuestAccessForbidden,
            MatrixErrorCode::ResourceLimitExceeded,
            MatrixErrorCode::CannotLeaveServerNoticeRoom,
        ];
        for code in &forbidden_codes {
            assert_eq!(code.http_status(), StatusCode::FORBIDDEN, "{code:?} should be FORBIDDEN");
        }

        let conflict_codes = [
            MatrixErrorCode::UserInUse,
            MatrixErrorCode::RoomInUse,
            MatrixErrorCode::ThreepidInUse,
            MatrixErrorCode::Exclusive,
        ];
        for code in &conflict_codes {
            assert_eq!(code.http_status(), StatusCode::CONFLICT, "{code:?} should be CONFLICT");
        }
    }

    // -----------------------------------------------------------------------
    // MatrixErrorCode Display
    // -----------------------------------------------------------------------

    #[test]
    fn test_matrix_error_code_display() {
        assert_eq!(format!("{}", MatrixErrorCode::Forbidden), "M_FORBIDDEN");
        assert_eq!(format!("{}", MatrixErrorCode::NotFound), "M_NOT_FOUND");
        assert_eq!(format!("{}", MatrixErrorCode::Unknown), "M_UNKNOWN");
    }

    // -----------------------------------------------------------------------
    // ErrorSource
    // -----------------------------------------------------------------------

    #[test]
    fn test_error_source_new() {
        let source = ErrorSource::new("storage::room", "get_messages");
        assert_eq!(source.module, "storage::room");
        assert_eq!(source.operation, "get_messages");
    }

    #[test]
    fn test_error_source_new_with_strings() {
        let source = ErrorSource::new(String::from("module"), String::from("operation"));
        assert_eq!(source.module, "module");
        assert_eq!(source.operation, "operation");
    }

    #[test]
    fn test_error_source_display() {
        let source = ErrorSource::new("mod", "op");
        assert_eq!(format!("{}", source), "[mod::op]");
    }

    #[test]
    fn test_error_source_display_with_paths() {
        let source = ErrorSource::new("storage::room::repository", "find_by_id");
        assert_eq!(format!("{}", source), "[storage::room::repository::find_by_id]");
    }

    #[test]
    fn test_error_source_debug() {
        let source = ErrorSource::new("a", "b");
        assert!(!format!("{:?}", source).is_empty());
    }

    // -----------------------------------------------------------------------
    // ApiErrorKind
    // -----------------------------------------------------------------------

    #[test]
    fn test_api_error_kind_default_http_status() {
        assert_eq!(ApiErrorKind::BadRequest.default_http_status(), StatusCode::BAD_REQUEST);
        assert_eq!(ApiErrorKind::Unauthorized.default_http_status(), StatusCode::UNAUTHORIZED);
        assert_eq!(ApiErrorKind::Forbidden.default_http_status(), StatusCode::FORBIDDEN);
        assert_eq!(ApiErrorKind::NotFound.default_http_status(), StatusCode::NOT_FOUND);
        assert_eq!(ApiErrorKind::Conflict.default_http_status(), StatusCode::CONFLICT);
        assert_eq!(ApiErrorKind::Gone.default_http_status(), StatusCode::GONE);
        assert_eq!(ApiErrorKind::RateLimited.default_http_status(), StatusCode::TOO_MANY_REQUESTS);
        assert_eq!(ApiErrorKind::Internal.default_http_status(), StatusCode::INTERNAL_SERVER_ERROR);
        assert_eq!(ApiErrorKind::NotImplemented.default_http_status(), StatusCode::NOT_IMPLEMENTED);
        assert_eq!(ApiErrorKind::Timeout.default_http_status(), StatusCode::REQUEST_TIMEOUT);
    }

    #[test]
    fn test_api_error_kind_serde_roundtrip() {
        let variants = [
            ApiErrorKind::BadRequest,
            ApiErrorKind::Unauthorized,
            ApiErrorKind::Forbidden,
            ApiErrorKind::NotFound,
            ApiErrorKind::Conflict,
            ApiErrorKind::Gone,
            ApiErrorKind::RateLimited,
            ApiErrorKind::Internal,
            ApiErrorKind::NotImplemented,
            ApiErrorKind::Timeout,
        ];
        for variant in &variants {
            let json = serde_json::to_string(variant).expect("serialize");
            let deserialized: ApiErrorKind = serde_json::from_str(&json).expect("deserialize");
            assert_eq!(*variant, deserialized, "roundtrip failed for {variant:?}");
        }
    }

    // -----------------------------------------------------------------------
    // Send + Sync (required by Axum)
    // -----------------------------------------------------------------------

    #[test]
    fn test_error_types_are_send_sync() {
        fn assert_send_sync<T: Send + Sync>() {}
        assert_send_sync::<ApiError>();
        assert_send_sync::<MatrixErrorCode>();
        assert_send_sync::<ApiErrorKind>();
        assert_send_sync::<ErrorSource>();
        assert_send_sync::<ApiResponse<String>>();
    }

    // -----------------------------------------------------------------------
    // From impls
    // -----------------------------------------------------------------------

    #[test]
    fn test_from_serde_json_error() {
        let serde_err = serde_json::from_str::<i32>("not a number").unwrap_err();
        let api_err: ApiError = serde_err.into();
        assert_eq!(api_err.kind, ApiErrorKind::BadRequest);
        assert_eq!(api_err.code, MatrixErrorCode::NotJson);
    }

    #[test]
    fn test_from_parse_int_error() {
        let parse_err = "abc".parse::<i32>().unwrap_err();
        let api_err: ApiError = parse_err.into();
        assert_eq!(api_err.kind, ApiErrorKind::BadRequest);
        assert_eq!(api_err.code, MatrixErrorCode::InvalidParam);
    }

    #[test]
    fn test_from_utf8_error() {
        let bytes = vec![0xFF, 0xFE, 0x00];
        let utf8_err = String::from_utf8(bytes).unwrap_err();
        let api_err: ApiError = utf8_err.into();
        assert_eq!(api_err.kind, ApiErrorKind::BadRequest);
        assert_eq!(api_err.code, MatrixErrorCode::InvalidParam);
        assert_eq!(api_err.message, "Invalid UTF-8 encoding");
    }

    // -----------------------------------------------------------------------
    // ApiResponse
    // -----------------------------------------------------------------------

    #[test]
    fn test_api_response_success() {
        let resp: ApiResponse<i32> = ApiResponse::success(42);
        assert_eq!(resp.status, "ok");
        assert_eq!(resp.data, Some(42));
        assert!(resp.error.is_none());
        assert!(resp.errcode.is_none());
        assert!(resp.retry_after_ms.is_none());
    }

    #[test]
    fn test_api_response_error() {
        let resp: ApiResponse<()> = ApiResponse::error("bad things".into(), "M_UNKNOWN".into());
        assert_eq!(resp.status, "error");
        assert!(resp.data.is_none());
        assert_eq!(resp.error.as_deref(), Some("bad things"));
        assert_eq!(resp.errcode.as_deref(), Some("M_UNKNOWN"));
        assert!(resp.retry_after_ms.is_none());
    }

    #[test]
    fn test_api_response_error_with_retry() {
        let resp: ApiResponse<()> =
            ApiResponse::error_with_retry("rate limited".into(), "M_LIMIT_EXCEEDED".into(), 5000);
        assert_eq!(resp.status, "error");
        assert!(resp.data.is_none());
        assert_eq!(resp.error.as_deref(), Some("rate limited"));
        assert_eq!(resp.errcode.as_deref(), Some("M_LIMIT_EXCEEDED"));
        assert_eq!(resp.retry_after_ms, Some(5000));
    }
}
