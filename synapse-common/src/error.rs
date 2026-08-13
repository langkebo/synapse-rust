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
    /// MSC4335: Returned when the server has reached its user account limit.
    /// Distinct from `LimitExceeded` (generic rate limit) and
    /// `ResourceLimitExceeded` (server-wide resource exhaustion).
    UserLimitExceeded,
    /// M_UNSUPPORTED: The server does not support this feature (e.g. presence
    /// disabled). Per Matrix spec, returned with HTTP 405 Method Not Allowed.
    Unsupported,
    /// M_UNKNOWN_POS: A sliding-sync `pos` token was invalid or expired
    /// (MSC4186). Distinct from `BadJson` so clients can reset their position
    /// and resync without mistaking other 400s for pos expiry.
    UnknownPos,
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
            Self::UserLimitExceeded => "M_USER_LIMIT_EXCEEDED",
            Self::Unsupported => "M_UNSUPPORTED",
            Self::UnknownPos => "M_UNKNOWN_POS",
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
            Self::UserInUse => StatusCode::BAD_REQUEST,
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
            Self::RequestTimeout => StatusCode::GATEWAY_TIMEOUT,
            // MSC4335: Too many users — 429 with retry-after semantics
            Self::UserLimitExceeded => StatusCode::TOO_MANY_REQUESTS,
            Self::Unsupported => StatusCode::METHOD_NOT_ALLOWED,
            Self::UnknownPos => StatusCode::BAD_REQUEST,
        }
    }

    /// 从 Matrix errcode 字符串解析变体。这是字符串→变体的单一权威来源，
    /// 供 `Deserialize` 与 `ApiResponse::into_response` 复用，避免三处映射漂移。
    pub fn from_str(s: &str) -> Option<Self> {
        match s {
            "M_FORBIDDEN" => Some(Self::Forbidden),
            "M_UNKNOWN_TOKEN" => Some(Self::UnknownToken),
            "M_MISSING_TOKEN" => Some(Self::MissingToken),
            "M_BAD_JSON" => Some(Self::BadJson),
            "M_NOT_JSON" => Some(Self::NotJson),
            "M_NOT_FOUND" => Some(Self::NotFound),
            "M_LIMIT_EXCEEDED" => Some(Self::LimitExceeded),
            "M_UNKNOWN" => Some(Self::Unknown),
            "M_UNRECOGNIZED" => Some(Self::Unrecognized),
            "M_UNAUTHORIZED" => Some(Self::Unauthorized),
            "M_USER_DEACTIVATED" => Some(Self::UserDeactivated),
            "M_USER_IN_USE" => Some(Self::UserInUse),
            "M_INVALID_USERNAME" => Some(Self::InvalidUsername),
            "M_ROOM_IN_USE" => Some(Self::RoomInUse),
            "M_INVALID_ROOM_STATE" => Some(Self::InvalidRoomState),
            "M_THREEPID_IN_USE" => Some(Self::ThreepidInUse),
            "M_THREEPID_NOT_FOUND" => Some(Self::ThreepidNotFound),
            "M_THREEPID_AUTH_FAILED" => Some(Self::ThreepidAuthFailed),
            "M_THREEPID_DENIED" => Some(Self::ThreepidDenied),
            "M_SERVER_NOT_TRUSTED" => Some(Self::ServerNotTrusted),
            "M_UNSUPPORTED_ROOM_VERSION" => Some(Self::UnsupportedRoomVersion),
            "M_INCOMPATIBLE_ROOM_VERSION" => Some(Self::IncompatibleRoomVersion),
            "M_BAD_STATE" => Some(Self::BadState),
            "M_GUEST_ACCESS_FORBIDDEN" => Some(Self::GuestAccessForbidden),
            "M_CAPTCHA_NEEDED" => Some(Self::CaptchaNeeded),
            "M_CAPTCHA_INVALID" => Some(Self::CaptchaInvalid),
            "M_MISSING_PARAM" => Some(Self::MissingParam),
            "M_INVALID_PARAM" => Some(Self::InvalidParam),
            "M_TOO_LARGE" => Some(Self::TooLarge),
            "M_EXCLUSIVE" => Some(Self::Exclusive),
            "M_RESOURCE_LIMIT_EXCEEDED" => Some(Self::ResourceLimitExceeded),
            "M_CANNOT_LEAVE_SERVER_NOTICE_ROOM" => Some(Self::CannotLeaveServerNoticeRoom),
            "M_REQUEST_TIMEOUT" => Some(Self::RequestTimeout),
            "M_USER_LIMIT_EXCEEDED" => Some(Self::UserLimitExceeded),
            "M_UNSUPPORTED" => Some(Self::Unsupported),
            "M_UNKNOWN_POS" => Some(Self::UnknownPos),
            _ => None,
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
        Self::from_str(&s).ok_or_else(|| {
            serde::de::Error::unknown_variant(
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
                    "M_USER_LIMIT_EXCEEDED",
                    "M_UNSUPPORTED",
                    "M_UNKNOWN_POS",
                ],
            )
        })
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
    /// 413 — request body exceeds size limit (M_TOO_LARGE)
    PayloadTooLarge,
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
            Self::PayloadTooLarge => StatusCode::PAYLOAD_TOO_LARGE,
            Self::RateLimited => StatusCode::TOO_MANY_REQUESTS,
            Self::Internal => StatusCode::INTERNAL_SERVER_ERROR,
            Self::NotImplemented => StatusCode::NOT_IMPLEMENTED,
            Self::Timeout => StatusCode::GATEWAY_TIMEOUT,
        }
    }
}

// ---------------------------------------------------------------------------
// ApiError — structured error with kind / code / cause
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
    pub cause: Option<ApiErrorCause>,
}

// --- PartialEq: compare kind, code, message only (cause is opaque) ---

impl PartialEq for ApiError {
    fn eq(&self, other: &Self) -> bool {
        self.kind == other.kind && self.code == other.code && self.message == other.message
    }
}

impl Eq for ApiError {}

// --- Display / Error ---

impl std::fmt::Display for ApiError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}: {}", self.code, self.message)
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
            cause: None,
        }
    }

    /// MSC4186: Construct an `M_UNKNOWN_POS` error — the sliding-sync `pos`
    /// token is invalid or expired. Clients branch on this errcode to reset
    /// their position and resync, distinguishing it from other 400 responses.
    pub fn unknown_pos(message: impl Into<String>) -> Self {
        Self {
            kind: ApiErrorKind::BadRequest,
            code: MatrixErrorCode::UnknownPos,
            message: message.into(),
            cause: None,
        }
    }

    pub fn unauthorized(message: impl Into<String>) -> Self {
        Self {
            kind: ApiErrorKind::Unauthorized,
            code: MatrixErrorCode::Unauthorized,
            message: message.into(),
            cause: None,
        }
    }

    pub fn forbidden(message: impl Into<String>) -> Self {
        Self {
            kind: ApiErrorKind::Forbidden,
            code: MatrixErrorCode::Forbidden,
            message: message.into(),
            cause: None,
        }
    }

    pub fn not_found(message: impl Into<String>) -> Self {
        Self {
            kind: ApiErrorKind::NotFound,
            code: MatrixErrorCode::NotFound,
            message: message.into(),
            cause: None,
        }
    }

    pub fn not_implemented(message: impl Into<String>) -> Self {
        Self {
            kind: ApiErrorKind::NotImplemented,
            code: MatrixErrorCode::Unimplemented,
            message: message.into(),
            cause: None,
        }
    }

    /// P1-3: Construct an `M_UNSUPPORTED` error. Used when a feature (e.g.
    /// presence) is disabled in server config. The HTTP status is 501 (Not
    /// Implemented) via `ApiErrorKind::NotImplemented`; the Matrix errcode
    /// is `M_UNSUPPORTED` so clients can branch on the feature gate.
    pub fn unsupported(message: impl Into<String>) -> Self {
        Self {
            kind: ApiErrorKind::NotImplemented,
            code: MatrixErrorCode::Unsupported,
            message: message.into(),
            cause: None,
        }
    }

    pub fn conflict(message: impl Into<String>) -> Self {
        Self {
            kind: ApiErrorKind::Conflict,
            code: MatrixErrorCode::UserInUse,
            message: message.into(),
            cause: None,
        }
    }

    /// Conflict with a specific Matrix error code.
    pub fn conflict_with(code: MatrixErrorCode, message: impl Into<String>) -> Self {
        Self { kind: ApiErrorKind::Conflict, code, message: message.into(), cause: None }
    }

    pub fn internal(message: impl Into<String>) -> Self {
        Self {
            kind: ApiErrorKind::Internal,
            code: MatrixErrorCode::Unknown,
            message: message.into(),
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
            cause: None,
        }
    }

    pub fn database(message: impl Into<String>) -> Self {
        Self {
            kind: ApiErrorKind::Internal,
            code: MatrixErrorCode::Unknown,
            message: message.into(),
            cause: None,
        }
    }

    pub fn cache(message: impl Into<String>) -> Self {
        Self {
            kind: ApiErrorKind::Internal,
            code: MatrixErrorCode::Unknown,
            message: format!("Cache error: {}", message.into()),
            cause: None,
        }
    }

    pub fn gone(message: impl Into<String>) -> Self {
        Self {
            kind: ApiErrorKind::Gone,
            code: MatrixErrorCode::NotFound,
            message: message.into(),
            cause: None,
        }
    }

    pub fn authentication(message: impl Into<String>) -> Self {
        Self {
            kind: ApiErrorKind::Unauthorized,
            code: MatrixErrorCode::UnknownToken,
            message: message.into(),
            cause: None,
        }
    }

    pub fn validation(message: impl Into<String>) -> Self {
        Self {
            kind: ApiErrorKind::BadRequest,
            code: MatrixErrorCode::InvalidParam,
            message: message.into(),
            cause: None,
        }
    }

    pub fn invalid_input(message: impl Into<String>) -> Self {
        Self {
            kind: ApiErrorKind::BadRequest,
            code: MatrixErrorCode::InvalidParam,
            message: message.into(),
            cause: None,
        }
    }

    pub fn crypto(message: impl Into<String>) -> Self {
        Self {
            kind: ApiErrorKind::Internal,
            code: MatrixErrorCode::Unknown,
            message: message.into(),
            cause: None,
        }
    }

    pub fn rate_limited(_message: impl Into<String>) -> Self {
        Self {
            kind: ApiErrorKind::RateLimited,
            code: MatrixErrorCode::LimitExceeded,
            message: "Rate limited".to_string(),
            cause: None,
        }
    }

    pub fn rate_limited_with_retry(retry_after_ms: u64) -> Self {
        Self {
            kind: ApiErrorKind::RateLimited,
            code: MatrixErrorCode::LimitExceeded,
            message: "Rate limited".to_string(),
            cause: Some(Arc::new(RetryAfterMsCause(retry_after_ms))),
        }
    }

    pub fn missing_token() -> Self {
        Self {
            kind: ApiErrorKind::Unauthorized,
            code: MatrixErrorCode::MissingToken,
            message: "Missing access token".to_string(),
            cause: None,
        }
    }

    pub fn not_json(message: impl Into<String>) -> Self {
        Self {
            kind: ApiErrorKind::BadRequest,
            code: MatrixErrorCode::NotJson,
            message: message.into(),
            cause: None,
        }
    }

    // -- domain-specific constructors (delegate to core with specific code) --

    pub fn user_deactivated(message: impl Into<String>) -> Self {
        Self {
            kind: ApiErrorKind::Forbidden,
            code: MatrixErrorCode::UserDeactivated,
            message: message.into(),
            cause: None,
        }
    }

    pub fn invalid_username(message: impl Into<String>) -> Self {
        Self {
            kind: ApiErrorKind::BadRequest,
            code: MatrixErrorCode::InvalidUsername,
            message: message.into(),
            cause: None,
        }
    }

    pub fn user_in_use(message: impl Into<String>) -> Self {
        Self {
            kind: ApiErrorKind::BadRequest,
            code: MatrixErrorCode::UserInUse,
            message: message.into(),
            cause: None,
        }
    }

    pub fn room_in_use(message: impl Into<String>) -> Self {
        Self {
            kind: ApiErrorKind::Conflict,
            code: MatrixErrorCode::RoomInUse,
            message: message.into(),
            cause: None,
        }
    }

    pub fn invalid_room_state(message: impl Into<String>) -> Self {
        Self {
            kind: ApiErrorKind::BadRequest,
            code: MatrixErrorCode::InvalidRoomState,
            message: message.into(),
            cause: None,
        }
    }

    pub fn threepid_in_use(message: impl Into<String>) -> Self {
        Self {
            kind: ApiErrorKind::Conflict,
            code: MatrixErrorCode::ThreepidInUse,
            message: message.into(),
            cause: None,
        }
    }

    pub fn threepid_not_found(message: impl Into<String>) -> Self {
        Self {
            kind: ApiErrorKind::BadRequest,
            code: MatrixErrorCode::ThreepidNotFound,
            message: message.into(),
            cause: None,
        }
    }

    pub fn threepid_auth_failed(message: impl Into<String>) -> Self {
        Self {
            kind: ApiErrorKind::Forbidden,
            code: MatrixErrorCode::ThreepidAuthFailed,
            message: message.into(),
            cause: None,
        }
    }

    pub fn threepid_denied(message: impl Into<String>) -> Self {
        Self {
            kind: ApiErrorKind::Forbidden,
            code: MatrixErrorCode::ThreepidDenied,
            message: message.into(),
            cause: None,
        }
    }

    pub fn server_not_trusted(message: impl Into<String>) -> Self {
        Self {
            kind: ApiErrorKind::Forbidden,
            code: MatrixErrorCode::ServerNotTrusted,
            message: message.into(),
            cause: None,
        }
    }

    pub fn unsupported_room_version(message: impl Into<String>) -> Self {
        Self {
            kind: ApiErrorKind::BadRequest,
            code: MatrixErrorCode::UnsupportedRoomVersion,
            message: message.into(),
            cause: None,
        }
    }

    pub fn incompatible_room_version(message: impl Into<String>) -> Self {
        Self {
            kind: ApiErrorKind::BadRequest,
            code: MatrixErrorCode::IncompatibleRoomVersion,
            message: message.into(),
            cause: None,
        }
    }

    pub fn bad_state(message: impl Into<String>) -> Self {
        Self {
            kind: ApiErrorKind::BadRequest,
            code: MatrixErrorCode::BadState,
            message: message.into(),
            cause: None,
        }
    }

    pub fn guest_access_forbidden(message: impl Into<String>) -> Self {
        Self {
            kind: ApiErrorKind::Forbidden,
            code: MatrixErrorCode::GuestAccessForbidden,
            message: message.into(),
            cause: None,
        }
    }

    pub fn captcha_needed(message: impl Into<String>) -> Self {
        Self {
            kind: ApiErrorKind::BadRequest,
            code: MatrixErrorCode::CaptchaNeeded,
            message: message.into(),
            cause: None,
        }
    }

    pub fn captcha_invalid(message: impl Into<String>) -> Self {
        Self {
            kind: ApiErrorKind::BadRequest,
            code: MatrixErrorCode::CaptchaInvalid,
            message: message.into(),
            cause: None,
        }
    }

    pub fn missing_param(message: impl Into<String>) -> Self {
        Self {
            kind: ApiErrorKind::BadRequest,
            code: MatrixErrorCode::MissingParam,
            message: message.into(),
            cause: None,
        }
    }

    pub fn invalid_param(message: impl Into<String>) -> Self {
        Self {
            kind: ApiErrorKind::BadRequest,
            code: MatrixErrorCode::InvalidParam,
            message: message.into(),
            cause: None,
        }
    }

    pub fn too_large(message: impl Into<String>) -> Self {
        Self {
            kind: ApiErrorKind::PayloadTooLarge,
            code: MatrixErrorCode::TooLarge,
            message: message.into(),
            cause: None,
        }
    }

    pub fn exclusive(message: impl Into<String>) -> Self {
        Self {
            kind: ApiErrorKind::Conflict,
            code: MatrixErrorCode::Exclusive,
            message: message.into(),
            cause: None,
        }
    }

    pub fn resource_limit_exceeded(message: impl Into<String>) -> Self {
        Self {
            kind: ApiErrorKind::Forbidden,
            code: MatrixErrorCode::ResourceLimitExceeded,
            message: message.into(),
            cause: None,
        }
    }

    pub fn cannot_leave_server_notice_room(message: impl Into<String>) -> Self {
        Self {
            kind: ApiErrorKind::Forbidden,
            code: MatrixErrorCode::CannotLeaveServerNoticeRoom,
            message: message.into(),
            cause: None,
        }
    }

    pub fn unknown(message: impl Into<String>) -> Self {
        Self {
            kind: ApiErrorKind::Internal,
            code: MatrixErrorCode::Unknown,
            message: message.into(),
            cause: None,
        }
    }

    pub fn unrecognized(message: impl Into<String>) -> Self {
        // M_UNRECOGNIZED maps to HTTP 400, matching MatrixErrorCode::Unrecognized
        // and the errcode fallback table (see d8bc373c). The router-level 404
        // fallback for truly unknown paths is hardcoded in assembly.rs instead.
        Self {
            kind: ApiErrorKind::BadRequest,
            code: MatrixErrorCode::Unrecognized,
            message: message.into(),
            cause: None,
        }
    }

    pub fn request_timeout(message: impl Into<String>) -> Self {
        Self {
            kind: ApiErrorKind::Timeout,
            code: MatrixErrorCode::RequestTimeout,
            message: message.into(),
            cause: None,
        }
    }

    // -- encryption / decryption (map to Internal with specific message) --

    pub fn decryption_error(message: impl Into<String>) -> Self {
        Self {
            kind: ApiErrorKind::Internal,
            code: MatrixErrorCode::Unknown,
            message: message.into(),
            cause: None,
        }
    }

    pub fn encryption_error(message: impl Into<String>) -> Self {
        Self {
            kind: ApiErrorKind::Internal,
            code: MatrixErrorCode::Unknown,
            message: message.into(),
            cause: None,
        }
    }
}

// ---------------------------------------------------------------------------
// Builder / convenience methods
// ---------------------------------------------------------------------------

impl ApiError {
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
        // The two are set independently by each constructor, so an errcode is
        // not hard-wired to a single HTTP status.
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
// From impls (error conversion)
// ---------------------------------------------------------------------------

impl From<sqlx::Error> for ApiError {
    fn from(err: sqlx::Error) -> Self {
        // Classify via the structured database error kind (SQLSTATE) instead of
        // fragile string matching on the display text. `is_unique_violation()`
        // is populated by the Postgres driver from the 23505 unique_violation
        // code, so localized/custom messages can't cause false negatives or
        // positives (审查 #19).
        let is_unique_violation = err
            .as_database_error()
            .map(|e| e.is_unique_violation())
            .unwrap_or(false);

        if is_unique_violation {
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
        tracing::error!(%err, "redis error");
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
            // 收敛到 MatrixErrorCode::from_str + http_status() 单一映射源，
            // 消除第三套硬编码 errcode→状态码映射的漂移（未知 errcode 落 500）。
            self.errcode
                .as_deref()
                .and_then(MatrixErrorCode::from_str)
                .map(|code| code.http_status())
                .unwrap_or(StatusCode::INTERNAL_SERVER_ERROR)
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

    // P1-3: M_UNSUPPORTED constructor for disabled features (e.g. presence).
    #[test]
    fn test_api_error_unsupported_construction() {
        let err = ApiError::unsupported("presence disabled");
        assert_eq!(err.kind, ApiErrorKind::NotImplemented);
        assert_eq!(err.code, MatrixErrorCode::Unsupported);
        assert_eq!(err.code.as_str(), "M_UNSUPPORTED");
        assert_eq!(err.code.http_status(), StatusCode::METHOD_NOT_ALLOWED);
        assert_eq!(err.message, "presence disabled");
    }

    // MSC4186: M_UNKNOWN_POS constructor for expired/invalid sliding-sync pos.
    #[test]
    fn test_api_error_unknown_pos_construction() {
        let err = ApiError::unknown_pos("Invalid or expired position token");
        assert_eq!(err.kind, ApiErrorKind::BadRequest);
        assert_eq!(err.code, MatrixErrorCode::UnknownPos);
        assert_eq!(err.code.as_str(), "M_UNKNOWN_POS");
        assert_eq!(err.code.http_status(), StatusCode::BAD_REQUEST);
        assert_eq!(err.message, "Invalid or expired position token");
    }

    #[test]
    fn test_matrix_error_code_unknown_pos_round_trip() {
        let json = serde_json::to_string(&MatrixErrorCode::UnknownPos).unwrap();
        assert_eq!(json, "\"M_UNKNOWN_POS\"");
        let decoded: MatrixErrorCode = serde_json::from_str(&json).unwrap();
        assert_eq!(decoded, MatrixErrorCode::UnknownPos);
    }

    // 审查 #7：M_REQUEST_TIMEOUT 按 Matrix 规范应返回 504（Gateway Timeout），
    // 而非 408（Request Timeout，客户端语义）。
    #[test]
    fn test_request_timeout_maps_to_gateway_timeout_504() {
        let err = ApiError::request_timeout("timed out");
        assert_eq!(err.kind, ApiErrorKind::Timeout);
        assert_eq!(err.code.as_str(), "M_REQUEST_TIMEOUT");
        assert_eq!(err.code.http_status(), StatusCode::GATEWAY_TIMEOUT);
        assert_eq!(err.kind.default_http_status(), StatusCode::GATEWAY_TIMEOUT);
    }

    #[test]
    fn test_matrix_error_code_unsupported_round_trip() {
        let json = serde_json::to_string(&MatrixErrorCode::Unsupported).unwrap();
        assert_eq!(json, "\"M_UNSUPPORTED\"");
        let back: MatrixErrorCode = serde_json::from_str(&json).unwrap();
        assert_eq!(back, MatrixErrorCode::Unsupported);
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

    // -----------------------------------------------------------------------
    // ApiError builder methods
    // -----------------------------------------------------------------------

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
        assert_eq!(err.kind, ApiErrorKind::BadRequest);
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
        assert_eq!(err.kind, ApiErrorKind::PayloadTooLarge);
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
        assert_eq!(err.kind, ApiErrorKind::BadRequest);
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
        assert_eq!(MatrixErrorCode::RequestTimeout.http_status(), StatusCode::GATEWAY_TIMEOUT);
        assert_eq!(MatrixErrorCode::UserInUse.http_status(), StatusCode::BAD_REQUEST);
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
            MatrixErrorCode::UserInUse,
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
            MatrixErrorCode::RoomInUse,
            MatrixErrorCode::ThreepidInUse,
            MatrixErrorCode::Exclusive,
        ];
        for code in &conflict_codes {
            assert_eq!(code.http_status(), StatusCode::CONFLICT, "{code:?} should be CONFLICT");
        }
    }

    // -----------------------------------------------------------------------
    // ApiResponse::into_response 与 MatrixErrorCode::http_status 收敛一致性
    // -----------------------------------------------------------------------

    /// 全量 37 个 errcode 变体：ApiResponse 的 errcode→状态码必须与
    /// MatrixErrorCode::http_status() 完全一致，防止第三套硬编码映射漂移。
    fn all_error_codes() -> Vec<MatrixErrorCode> {
        vec![
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
            MatrixErrorCode::UserLimitExceeded,
            MatrixErrorCode::Unsupported,
            MatrixErrorCode::UnknownPos,
        ]
    }

    #[test]
    fn test_api_response_status_covers_previously_missing_errcodes() {
        use axum::response::IntoResponse;
        // 之前 ApiResponse::into_response 的硬编码 match 缺失这些 errcode（落 500），
        // 收敛到 MatrixErrorCode::from_str + http_status() 后应正确。
        let cases: &[(&str, StatusCode)] = &[
            ("M_REQUEST_TIMEOUT", StatusCode::GATEWAY_TIMEOUT),
            ("M_UNKNOWN_POS", StatusCode::BAD_REQUEST),
            ("M_USER_LIMIT_EXCEEDED", StatusCode::TOO_MANY_REQUESTS),
            ("M_USER_DEACTIVATED", StatusCode::FORBIDDEN),
            ("M_GUEST_ACCESS_FORBIDDEN", StatusCode::FORBIDDEN),
            ("M_RESOURCE_LIMIT_EXCEEDED", StatusCode::FORBIDDEN),
            ("M_CANNOT_LEAVE_SERVER_NOTICE_ROOM", StatusCode::FORBIDDEN),
            ("M_THREEPID_AUTH_FAILED", StatusCode::FORBIDDEN),
            ("M_THREEPID_DENIED", StatusCode::FORBIDDEN),
            ("M_THREEPID_NOT_FOUND", StatusCode::BAD_REQUEST),
            ("M_EXCLUSIVE", StatusCode::CONFLICT),
            ("M_UNSUPPORTED_ROOM_VERSION", StatusCode::BAD_REQUEST),
            ("M_INCOMPATIBLE_ROOM_VERSION", StatusCode::BAD_REQUEST),
            ("M_UNSUPPORTED", StatusCode::METHOD_NOT_ALLOWED),
        ];
        for (errcode, expected) in cases {
            let resp = ApiResponse::<serde_json::Value>::error("x".to_string(), (*errcode).to_string());
            assert_eq!(resp.into_response().status(), *expected, "errcode={errcode}");
        }
    }

    #[test]
    fn test_api_response_unknown_errcode_falls_back_to_internal_error() {
        use axum::response::IntoResponse;
        let resp = ApiResponse::<serde_json::Value>::error("x".to_string(), "M_NOT_A_REAL_CODE".to_string());
        assert_eq!(resp.into_response().status(), StatusCode::INTERNAL_SERVER_ERROR);
    }

    #[test]
    fn test_api_response_status_converges_with_matrix_error_code_http_status() {
        use axum::response::IntoResponse;
        // 遍历所有唯一 errcode 字符串，ApiResponse 状态码必须等于
        // MatrixErrorCode::from_str → http_status() 单一映射源。
        // 注意 Unimplemented 与 Unrecognized 共享 "M_UNRECOGNIZED"（by-design，
        // 见 as_str），故按唯一 errcode 去重后以 from_str 结果为准。
        let mut seen = std::collections::HashSet::new();
        for code in all_error_codes() {
            let errcode = code.as_str().to_string();
            if !seen.insert(errcode.clone()) {
                continue;
            }
            let resp = ApiResponse::<serde_json::Value>::error("x".to_string(), errcode.clone());
            let status = resp.into_response().status();
            let expected =
                MatrixErrorCode::from_str(&errcode).map(|c| c.http_status()).unwrap_or(StatusCode::INTERNAL_SERVER_ERROR);
            assert_eq!(status, expected, "ApiResponse 状态码 {status} 与 from_str({errcode})→http_status {expected:?} 漂移");
        }
    }

    /// as_str 与 from_str 互逆性：新增变体时若 from_str 漏加字符串映射，
    /// 此测试会在 round-trip 处失败。Unimplemented 与 Unrecognized 共享
    /// "M_UNRECOGNIZED"（by-design），单独跳过。
    #[test]
    fn test_matrix_error_code_as_str_round_trips_through_from_str() {
        for code in all_error_codes() {
            let s = code.as_str();
            let parsed = MatrixErrorCode::from_str(s);
            assert!(parsed.is_some(), "as_str({s}) 无法被 from_str 解析，from_str 漏了映射（{code:?}）");
            if code == MatrixErrorCode::Unimplemented {
                continue;
            }
            assert_eq!(parsed.unwrap(), code, "as_str→from_str round-trip 不对称（{code:?}）");
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
        assert_eq!(ApiErrorKind::PayloadTooLarge.default_http_status(), StatusCode::PAYLOAD_TOO_LARGE);
        assert_eq!(ApiErrorKind::RateLimited.default_http_status(), StatusCode::TOO_MANY_REQUESTS);
        assert_eq!(ApiErrorKind::Internal.default_http_status(), StatusCode::INTERNAL_SERVER_ERROR);
        assert_eq!(ApiErrorKind::NotImplemented.default_http_status(), StatusCode::NOT_IMPLEMENTED);
        assert_eq!(ApiErrorKind::Timeout.default_http_status(), StatusCode::GATEWAY_TIMEOUT);
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
            ApiErrorKind::PayloadTooLarge,
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

    // -----------------------------------------------------------------------
    // From<sqlx::Error> structured unique-violation classification (#19)
    // -----------------------------------------------------------------------

    /// A mock sqlx database error whose SQLSTATE is a unique violation (23505)
    /// but whose display/debug text deliberately does NOT contain the magic
    /// substrings ("duplicate key", "unique constraint", "23505", ...) the old
    /// string-matching logic relied on. This proves classification is driven by
    /// the structured error kind, not by text matching.
    #[derive(Debug)]
    struct MockUniqueViolationError;

    impl std::fmt::Display for MockUniqueViolationError {
        fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
            f.write_str("boom")
        }
    }

    impl std::error::Error for MockUniqueViolationError {}

    impl sqlx::error::DatabaseError for MockUniqueViolationError {
        fn message(&self) -> &str {
            "boom"
        }

        fn code(&self) -> Option<std::borrow::Cow<'_, str>> {
            Some("23505".into())
        }

        fn as_error(&self) -> &(dyn std::error::Error + Send + Sync + 'static) {
            self
        }

        fn as_error_mut(&mut self) -> &mut (dyn std::error::Error + Send + Sync + 'static) {
            self
        }

        fn into_error(self: Box<Self>) -> Box<dyn std::error::Error + Send + Sync + 'static> {
            self
        }

        fn kind(&self) -> sqlx::error::ErrorKind {
            sqlx::error::ErrorKind::UniqueViolation
        }
    }

    #[test]
    fn test_from_sqlx_unique_violation_is_structured_not_string_matched() {
        let err: ApiError = sqlx::Error::Database(Box::new(MockUniqueViolationError)).into();
        assert_eq!(err.kind, ApiErrorKind::BadRequest);
        assert_eq!(err.message, "A duplicate entry was found");
    }

    #[test]
    fn test_from_sqlx_row_not_found_maps_to_database_error() {
        let err: ApiError = sqlx::Error::RowNotFound.into();
        assert_eq!(err.kind, ApiErrorKind::Internal);
        assert_eq!(err.message, "A database error occurred");
    }
}
