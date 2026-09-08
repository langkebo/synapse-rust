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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn as_str_all_variants() {
        assert_eq!(KeyRequestAction::Request.as_str(), "request");
        assert_eq!(KeyRequestAction::Cancellation.as_str(), "cancellation");
        assert_eq!(KeyRequestAction::Requested.as_str(), "requested");
        assert_eq!(KeyRequestAction::Cancelled.as_str(), "cancelled");
    }

    #[test]
    fn from_str_roundtrip_all_variants() {
        assert_eq!("request".parse::<KeyRequestAction>().unwrap(), KeyRequestAction::Request);
        assert_eq!("cancellation".parse::<KeyRequestAction>().unwrap(), KeyRequestAction::Cancellation);
        assert_eq!("requested".parse::<KeyRequestAction>().unwrap(), KeyRequestAction::Requested);
        assert_eq!("cancelled".parse::<KeyRequestAction>().unwrap(), KeyRequestAction::Cancelled);
    }

    #[test]
    fn from_str_unknown_rejects() {
        assert!("bogus".parse::<KeyRequestAction>().is_err());
        assert!("".parse::<KeyRequestAction>().is_err());
    }

    #[test]
    fn serde_roundtrip() {
        let action = KeyRequestAction::Cancellation;
        let json = serde_json::to_string(&action).unwrap();
        let rt: KeyRequestAction = serde_json::from_str(&json).unwrap();
        assert_eq!(rt, action);
    }

    // ── KeyRequestInfo ──────────────────────────────────────────

    #[test]
    fn key_request_info_serde_roundtrip() {
        let info = KeyRequestInfo {
            request_id: "req-uuid-001".to_string(),
            user_id: "@alice:example.org".to_string(),
            device_id: "DEVICE1".to_string(),
            room_id: "!room:example.org".to_string(),
            session_id: "session1".to_string(),
            algorithm: "m.megolm.v1.aes-sha2".to_string(),
            action: "request".to_string(),
            created_ts: 1_700_000_000_000,
            is_fulfilled: false,
            fulfilled_by_device: None,
            fulfilled_ts: None,
        };
        let json = serde_json::to_string(&info).unwrap();
        let rt: KeyRequestInfo = serde_json::from_str(&json).unwrap();
        assert_eq!(rt.request_id, info.request_id);
        assert_eq!(rt.is_fulfilled, false);
        assert_eq!(rt.fulfilled_by_device, None);
    }

    #[test]
    fn key_request_info_fulfilled_roundtrip() {
        let info = KeyRequestInfo {
            request_id: "req-uuid-002".to_string(),
            user_id: "@bob:example.org".to_string(),
            device_id: "DEVICE2".to_string(),
            room_id: "!room:example.org".to_string(),
            session_id: "session2".to_string(),
            algorithm: "m.megolm.v1.aes-sha2".to_string(),
            action: "request".to_string(),
            created_ts: 1_700_000_001_000,
            is_fulfilled: true,
            fulfilled_by_device: Some("DEVICE3".to_string()),
            fulfilled_ts: Some(1_700_000_002_000),
        };
        let json = serde_json::to_string(&info).unwrap();
        let rt: KeyRequestInfo = serde_json::from_str(&json).unwrap();
        assert_eq!(rt.is_fulfilled, true);
        assert_eq!(rt.fulfilled_by_device, Some("DEVICE3".to_string()));
        assert_eq!(rt.fulfilled_ts, Some(1_700_000_002_000));
    }

    // ── KeyShareResponse ────────────────────────────────────────

    #[test]
    fn key_share_response_no_forwarding_key() {
        let resp = KeyShareResponse {
            room_id: "!room:example.org".to_string(),
            session_id: "session1".to_string(),
            session_key: "encrypted_key_data".to_string(),
            sender_key: "curve25519_sender_key".to_string(),
            algorithm: "m.megolm.v1.aes-sha2".to_string(),
            forwarding_curve25519_key: None,
        };
        let json = serde_json::to_string(&resp).unwrap();
        let rt: KeyShareResponse = serde_json::from_str(&json).unwrap();
        assert_eq!(rt.forwarding_curve25519_key, None);
    }

    #[test]
    fn key_share_response_with_forwarding_key() {
        let resp = KeyShareResponse {
            room_id: "!room:example.org".to_string(),
            session_id: "session1".to_string(),
            session_key: "encrypted_key_data".to_string(),
            sender_key: "curve25519_sender_key".to_string(),
            algorithm: "m.megolm.v1.aes-sha2".to_string(),
            forwarding_curve25519_key: Some("forwarded_curve_key".to_string()),
        };
        let json = serde_json::to_string(&resp).unwrap();
        let rt: KeyShareResponse = serde_json::from_str(&json).unwrap();
        assert_eq!(rt.forwarding_curve25519_key, Some("forwarded_curve_key".to_string()));
    }
}
