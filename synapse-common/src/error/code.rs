//! Matrix 规范错误码（`MatrixErrorCode`）及其字符串/HTTP 状态映射。
//!
//! 从原 `error.rs` 上帝文件拆分而来（M-2 可维护性治理）。

use axum::http::StatusCode;
use serde::{Deserialize, Serialize};

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
