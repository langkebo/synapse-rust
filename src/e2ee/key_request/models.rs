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
            Self::Request => "request",
            Self::Cancellation => "cancellation",
            Self::Requested => "requested",
            Self::Cancelled => "cancelled",
        }
    }
}

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

#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow)]
pub struct KeyRequestInfo {
    pub request_id: String,
    pub user_id: String,
    pub device_id: String,
    pub room_id: String,
    pub session_id: String,
    pub algorithm: String,
    pub action: String,
    pub created_ts: i64,
    pub is_fulfilled: bool,
    pub fulfilled_by_device: Option<String>,
    pub fulfilled_ts: Option<i64>,
}

#[derive(Debug, Clone, Copy)]
pub struct KeyRequestPagination<'a> {
    pub user_id: &'a str,
    pub limit: i64,
    pub from_ts: Option<i64>,
    pub from_id: Option<&'a str>,
    pub status: Option<&'a str>,
    pub room_id: Option<&'a str>,
    pub session_id: Option<&'a str>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_key_request_action_as_str() {
        assert_eq!(KeyRequestAction::Request.as_str(), "request");
        assert_eq!(KeyRequestAction::Cancellation.as_str(), "cancellation");
        assert_eq!(KeyRequestAction::Requested.as_str(), "requested");
        assert_eq!(KeyRequestAction::Cancelled.as_str(), "cancelled");
    }

    #[test]
    fn test_key_request_action_from_str_valid() {
        assert_eq!("request".parse::<KeyRequestAction>().unwrap(), KeyRequestAction::Request);
        assert_eq!("cancellation".parse::<KeyRequestAction>().unwrap(), KeyRequestAction::Cancellation);
        assert_eq!("requested".parse::<KeyRequestAction>().unwrap(), KeyRequestAction::Requested);
        assert_eq!("cancelled".parse::<KeyRequestAction>().unwrap(), KeyRequestAction::Cancelled);
    }

    #[test]
    fn test_key_request_action_from_str_invalid() {
        assert!("invalid".parse::<KeyRequestAction>().is_err());
        assert!("".parse::<KeyRequestAction>().is_err());
    }

    #[test]
    fn test_key_request_action_roundtrip() {
        let actions = [
            KeyRequestAction::Request,
            KeyRequestAction::Cancellation,
            KeyRequestAction::Requested,
            KeyRequestAction::Cancelled,
        ];
        for action in &actions {
            let s = action.as_str();
            let parsed: KeyRequestAction = s.parse().unwrap();
            assert_eq!(*action, parsed);
        }
    }

    #[test]
    fn test_key_request() {
        let request = KeyRequest {
            request_id: "req_123".to_string(),
            user_id: "@alice:example.com".to_string(),
            device_id: "DEVICE1".to_string(),
            room_id: "!room:example.com".to_string(),
            session_id: "session_1".to_string(),
            algorithm: "m.megolm.v1.aes-sha2".to_string(),
            action: KeyRequestAction::Request,
            requesting_device_id: "DEVICE2".to_string(),
        };
        assert_eq!(request.request_id, "req_123");
        assert_eq!(request.algorithm, "m.megolm.v1.aes-sha2");
        assert_eq!(request.action, KeyRequestAction::Request);
    }

    #[test]
    fn test_key_request_body() {
        let body = KeyRequestBody {
            action: "request".to_string(),
            room_id: "!room:example.com".to_string(),
            sender_key: "sender_key".to_string(),
            session_id: "session_1".to_string(),
            algorithm: "m.megolm.v1.aes-sha2".to_string(),
        };
        assert_eq!(body.action, "request");
        assert_eq!(body.sender_key, "sender_key");
    }

    #[test]
    fn test_key_share_request() {
        let request = KeyShareRequest {
            user_id: "@alice:example.com".to_string(),
            device_id: "DEVICE1".to_string(),
            room_id: "!room:example.com".to_string(),
            session_id: "session_1".to_string(),
            sender_key: "sender_key".to_string(),
            algorithm: "m.megolm.v1.aes-sha2".to_string(),
            requesting_device_id: "DEVICE2".to_string(),
        };
        assert_eq!(request.user_id, "@alice:example.com");
        assert_eq!(request.requesting_device_id, "DEVICE2");
    }

    #[test]
    fn test_key_share_response() {
        let response = KeyShareResponse {
            room_id: "!room:example.com".to_string(),
            session_id: "session_1".to_string(),
            session_key: "session_key_data".to_string(),
            sender_key: "sender_key".to_string(),
            algorithm: "m.megolm.v1.aes-sha2".to_string(),
            forwarding_curve25519_key: Some("forwarding_key".to_string()),
        };
        assert_eq!(response.session_key, "session_key_data");
        assert!(response.forwarding_curve25519_key.is_some());
    }

    #[test]
    fn test_key_share_response_no_forwarding() {
        let response = KeyShareResponse {
            room_id: "!room:example.com".to_string(),
            session_id: "session_1".to_string(),
            session_key: "key".to_string(),
            sender_key: "sender".to_string(),
            algorithm: "algo".to_string(),
            forwarding_curve25519_key: None,
        };
        assert!(response.forwarding_curve25519_key.is_none());
    }

    #[test]
    fn test_key_request_info() {
        let info = KeyRequestInfo {
            request_id: "req_1".to_string(),
            user_id: "@alice:example.com".to_string(),
            device_id: "DEVICE1".to_string(),
            room_id: "!room:example.com".to_string(),
            session_id: "session_1".to_string(),
            algorithm: "m.megolm.v1.aes-sha2".to_string(),
            action: "request".to_string(),
            created_ts: 1700000000000,
            is_fulfilled: false,
            fulfilled_by_device: None,
            fulfilled_ts: None,
        };
        assert!(!info.is_fulfilled);
        assert!(info.fulfilled_by_device.is_none());
    }

    #[test]
    fn test_key_request_info_fulfilled() {
        let info = KeyRequestInfo {
            request_id: "req_2".to_string(),
            user_id: "@alice:example.com".to_string(),
            device_id: "DEVICE1".to_string(),
            room_id: "!room:example.com".to_string(),
            session_id: "session_2".to_string(),
            algorithm: "m.megolm.v1.aes-sha2".to_string(),
            action: "request".to_string(),
            created_ts: 1700000000000,
            is_fulfilled: true,
            fulfilled_by_device: Some("DEVICE2".to_string()),
            fulfilled_ts: Some(1700000100000),
        };
        assert!(info.is_fulfilled);
        assert_eq!(info.fulfilled_by_device.as_deref(), Some("DEVICE2"));
    }
}
