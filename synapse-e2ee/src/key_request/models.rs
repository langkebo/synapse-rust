use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
/// The `KeyRequest` type.
pub struct KeyRequest {
    /// The `request_id` field.
    /// The `user_id` field.
    /// The `device_id` field.
    /// The `room_id` field.
    /// The `session_id` field.
    /// The `algorithm` field.
    /// The `action` field.
    /// The `requesting_device_id` field.
    pub request_id: String,
    /// The `user_id` field.
    /// The `device_id` field.
    /// The `room_id` field.
    /// The `session_id` field.
    /// The `algorithm` field.
    /// The `action` field.
    /// The `requesting_device_id` field.
    pub user_id: String,
    /// The `device_id` field.
    /// The `room_id` field.
    /// The `session_id` field.
    /// The `algorithm` field.
    /// The `action` field.
    /// The `requesting_device_id` field.
    pub device_id: String,
    /// The `room_id` field.
    /// The `session_id` field.
    /// The `algorithm` field.
    /// The `action` field.
    /// The `requesting_device_id` field.
    pub room_id: String,
    /// The `session_id` field.
    /// The `algorithm` field.
    /// The `action` field.
    /// The `requesting_device_id` field.
    pub session_id: String,
    /// The `algorithm` field.
    /// The `action` field.
    /// The `requesting_device_id` field.
    pub algorithm: String,
    /// The `action` field.
    /// The `requesting_device_id` field.
    pub action: KeyRequestAction,
    /// The `requesting_device_id` field.
    pub requesting_device_id: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
/// The `KeyRequestAction` enum.
pub enum KeyRequestAction {
    /// The `Request` variant.
    /// The `Cancellation` variant.
    /// The `Requested` variant.
    /// The `Cancelled` variant.
    Request,
    /// The `Cancellation` variant.
    /// The `Requested` variant.
    /// The `Cancelled` variant.
    Cancellation,
    /// The `Requested` variant.
    /// The `Cancelled` variant.
    Requested,
    /// The `Cancelled` variant.
    Cancelled,
}

/// (see code)
impl KeyRequestAction {
    /// See [`as_str`].
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Request => "request",
            Self::Cancellation => "cancellation",
            Self::Requested => "requested",
            Self::Cancelled => "cancelled",
        }
    }
}

/// (see code)
impl std::str::FromStr for KeyRequestAction {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "request" => Ok(Self::Request),
            "cancellation" => Ok(Self::Cancellation),
            "requested" => Ok(Self::Requested),
            "cancelled" => Ok(Self::Cancelled),
            _ => Err(format!("Unknown action: {s}")),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
/// The `KeyRequestBody` type.
pub struct KeyRequestBody {
    /// The `action` field.
    /// The `room_id` field.
    /// The `sender_key` field.
    /// The `session_id` field.
    /// The `algorithm` field.
    pub action: String,
    /// The `room_id` field.
    /// The `sender_key` field.
    /// The `session_id` field.
    /// The `algorithm` field.
    pub room_id: String,
    /// The `sender_key` field.
    /// The `session_id` field.
    /// The `algorithm` field.
    pub sender_key: String,
    /// The `session_id` field.
    /// The `algorithm` field.
    pub session_id: String,
    /// The `algorithm` field.
    pub algorithm: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
/// The `KeyShareRequest` type.
pub struct KeyShareRequest {
    /// The `user_id` field.
    /// The `device_id` field.
    /// The `room_id` field.
    /// The `session_id` field.
    /// The `sender_key` field.
    /// The `algorithm` field.
    /// The `requesting_device_id` field.
    pub user_id: String,
    /// The `device_id` field.
    /// The `room_id` field.
    /// The `session_id` field.
    /// The `sender_key` field.
    /// The `algorithm` field.
    /// The `requesting_device_id` field.
    pub device_id: String,
    /// The `room_id` field.
    /// The `session_id` field.
    /// The `sender_key` field.
    /// The `algorithm` field.
    /// The `requesting_device_id` field.
    pub room_id: String,
    /// The `session_id` field.
    /// The `sender_key` field.
    /// The `algorithm` field.
    /// The `requesting_device_id` field.
    pub session_id: String,
    /// The `sender_key` field.
    /// The `algorithm` field.
    /// The `requesting_device_id` field.
    pub sender_key: String,
    /// The `algorithm` field.
    /// The `requesting_device_id` field.
    pub algorithm: String,
    /// The `requesting_device_id` field.
    pub requesting_device_id: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
/// The `KeyShareResponse` type.
pub struct KeyShareResponse {
    /// The `room_id` field.
    /// The `session_id` field.
    /// The `session_key` field.
    /// The `sender_key` field.
    /// The `algorithm` field.
    /// The `forwarding_curve25519_key` field.
    pub room_id: String,
    /// The `session_id` field.
    /// The `session_key` field.
    /// The `sender_key` field.
    /// The `algorithm` field.
    /// The `forwarding_curve25519_key` field.
    pub session_id: String,
    /// The `session_key` field.
    /// The `sender_key` field.
    /// The `algorithm` field.
    /// The `forwarding_curve25519_key` field.
    pub session_key: String,
    /// The `sender_key` field.
    /// The `algorithm` field.
    /// The `forwarding_curve25519_key` field.
    pub sender_key: String,
    /// The `algorithm` field.
    /// The `forwarding_curve25519_key` field.
    pub algorithm: String,
    /// The `forwarding_curve25519_key` field.
    pub forwarding_curve25519_key: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow)]
/// The `KeyRequestInfo` type.
pub struct KeyRequestInfo {
    /// The `request_id` field.
    /// The `user_id` field.
    /// The `device_id` field.
    /// The `room_id` field.
    /// The `session_id` field.
    /// The `algorithm` field.
    /// The `action` field.
    /// The `created_ts` field.
    /// The `is_fulfilled` field.
    /// The `fulfilled_by_device` field.
    /// The `fulfilled_ts` field.
    pub request_id: String,
    /// The `user_id` field.
    /// The `device_id` field.
    /// The `room_id` field.
    /// The `session_id` field.
    /// The `algorithm` field.
    /// The `action` field.
    /// The `created_ts` field.
    /// The `is_fulfilled` field.
    /// The `fulfilled_by_device` field.
    /// The `fulfilled_ts` field.
    pub user_id: String,
    /// The `device_id` field.
    /// The `room_id` field.
    /// The `session_id` field.
    /// The `algorithm` field.
    /// The `action` field.
    /// The `created_ts` field.
    /// The `is_fulfilled` field.
    /// The `fulfilled_by_device` field.
    /// The `fulfilled_ts` field.
    pub device_id: String,
    /// The `room_id` field.
    /// The `session_id` field.
    /// The `algorithm` field.
    /// The `action` field.
    /// The `created_ts` field.
    /// The `is_fulfilled` field.
    /// The `fulfilled_by_device` field.
    /// The `fulfilled_ts` field.
    pub room_id: String,
    /// The `session_id` field.
    /// The `algorithm` field.
    /// The `action` field.
    /// The `created_ts` field.
    /// The `is_fulfilled` field.
    /// The `fulfilled_by_device` field.
    /// The `fulfilled_ts` field.
    pub session_id: String,
    /// The `algorithm` field.
    /// The `action` field.
    /// The `created_ts` field.
    /// The `is_fulfilled` field.
    /// The `fulfilled_by_device` field.
    /// The `fulfilled_ts` field.
    pub algorithm: String,
    /// The `action` field.
    /// The `created_ts` field.
    /// The `is_fulfilled` field.
    /// The `fulfilled_by_device` field.
    /// The `fulfilled_ts` field.
    pub action: String,
    /// The `created_ts` field.
    /// The `is_fulfilled` field.
    /// The `fulfilled_by_device` field.
    /// The `fulfilled_ts` field.
    pub created_ts: i64,
    /// The `is_fulfilled` field.
    /// The `fulfilled_by_device` field.
    /// The `fulfilled_ts` field.
    pub is_fulfilled: bool,
    /// The `fulfilled_by_device` field.
    /// The `fulfilled_ts` field.
    pub fulfilled_by_device: Option<String>,
    /// The `fulfilled_ts` field.
    pub fulfilled_ts: Option<i64>,
}

#[derive(Debug, Clone, Copy)]
/// The `KeyRequestPagination` type.
pub struct KeyRequestPagination<'a> {
    /// The `user_id` field.
    /// The `limit` field.
    /// The `from_ts` field.
    /// The `from_id` field.
    /// The `status` field.
    /// The `room_id` field.
    /// The `session_id` field.
    pub user_id: &'a str,
    /// The `limit` field.
    /// The `from_ts` field.
    /// The `from_id` field.
    /// The `status` field.
    /// The `room_id` field.
    /// The `session_id` field.
    pub limit: i64,
    /// The `from_ts` field.
    /// The `from_id` field.
    /// The `status` field.
    /// The `room_id` field.
    /// The `session_id` field.
    pub from_ts: Option<i64>,
    /// The `from_id` field.
    /// The `status` field.
    /// The `room_id` field.
    /// The `session_id` field.
    pub from_id: Option<&'a str>,
    /// The `status` field.
    /// The `room_id` field.
    /// The `session_id` field.
    pub status: Option<&'a str>,
    /// The `room_id` field.
    /// The `session_id` field.
    pub room_id: Option<&'a str>,
    /// The `session_id` field.
    pub session_id: Option<&'a str>,
}
