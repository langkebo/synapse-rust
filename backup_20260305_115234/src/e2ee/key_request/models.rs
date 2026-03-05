use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct KeyRequest {
    pub request_id: String,
    pub user_id: String,
    pub device_id: String,
    pub room_id: String,
    pub session_id: String,
    pub algorithm: String,
    pub action: KeyRequestAction,
    pub requesting_device_id: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum KeyRequestAction {
    Request,
    Cancellation,
    Requested,
    Cancelled,
}

impl KeyRequestAction {
    pub fn as_str(&self) -> &'static str {
        match self {
            KeyRequestAction::Request => "request",
            KeyRequestAction::Cancellation => "cancellation",
            KeyRequestAction::Requested => "requested",
            KeyRequestAction::Cancelled => "cancelled",
        }
    }
}

impl std::str::FromStr for KeyRequestAction {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "request" => Ok(KeyRequestAction::Request),
            "cancellation" => Ok(KeyRequestAction::Cancellation),
            "requested" => Ok(KeyRequestAction::Requested),
            "cancelled" => Ok(KeyRequestAction::Cancelled),
            _ => Err(format!("Unknown action: {}", s)),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct KeyRequestBody {
    pub action: String,
    pub room_id: String,
    pub sender_key: String,
    pub session_id: String,
    pub algorithm: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct KeyShareRequest {
    pub user_id: String,
    pub device_id: String,
    pub room_id: String,
    pub session_id: String,
    pub sender_key: String,
    pub algorithm: String,
    pub requesting_device_id: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct KeyShareResponse {
    pub room_id: String,
    pub session_id: String,
    pub session_key: String,
    pub sender_key: String,
    pub algorithm: String,
    pub forwarding_curve25519_key: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct KeyRequestInfo {
    pub request_id: String,
    pub user_id: String,
    pub device_id: String,
    pub room_id: String,
    pub session_id: String,
    pub algorithm: String,
    pub action: String,
    pub created_ts: i64,
    pub fulfilled: bool,
    pub fulfilled_by_device: Option<String>,
    pub fulfilled_ts: Option<i64>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct KeyRequestQuery {
    pub user_id: Option<String>,
    pub device_id: Option<String>,
    pub room_id: Option<String>,
    pub session_id: Option<String>,
    pub action: Option<String>,
    pub fulfilled: Option<bool>,
}
