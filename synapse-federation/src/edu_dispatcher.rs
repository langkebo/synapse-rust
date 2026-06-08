//! EDU Dispatcher — unified, typed EDU dispatch for inbound federation transactions.
//!
//! This module provides:
//!
//! - [`EduType`] — a typed enum for every EDU type the homeserver recognises.
//! - [`InboundEduResult`] — per-EDU processing outcome for metrics/logging.
//! - [`build_outbound_edu`] / [`build_outbound_room_edu`] — outbound EDU helpers.
//!
//! The actual inbound dispatch function (`dispatch_inbound_edu`) lives in the
//! main crate because it requires access to `AppState` and its services.

use serde::{Deserialize, Serialize};
use std::fmt;

// ---------------------------------------------------------------------------
// EduType
// ---------------------------------------------------------------------------

/// Typed representation of a Matrix EDU type.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum EduType {
    /// `m.presence` — user presence updates pushed over federation.
    Presence,
    /// `m.typing` — typing notifications pushed over federation.
    Typing,
    /// `m.receipt` — read receipts pushed over federation.
    Receipt,
    /// `m.device_list_update` — device list change notifications.
    DeviceListUpdate,
    /// Any EDU type not explicitly recognised.  Logged and dropped.
    Unknown,
}

impl EduType {
    /// Map a raw `edu_type` string to the typed enum.
    pub fn from_raw(raw: &str) -> Self {
        match raw {
            "m.presence" => Self::Presence,
            "m.typing" => Self::Typing,
            "m.receipt" => Self::Receipt,
            "m.device_list_update" => Self::DeviceListUpdate,
            _ => Self::Unknown,
        }
    }

    /// Return the canonical Matrix EDU type string (e.g. `"m.presence"`).
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Presence => "m.presence",
            Self::Typing => "m.typing",
            Self::Receipt => "m.receipt",
            Self::DeviceListUpdate => "m.device_list_update",
            Self::Unknown => "m.unknown",
        }
    }

    /// Whether this EDU type should be processed on the inbound side.
    pub fn is_processable(&self) -> bool {
        !matches!(self, Self::Unknown)
    }
}

impl fmt::Display for EduType {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

// ---------------------------------------------------------------------------
// InboundEduResult
// ---------------------------------------------------------------------------

/// Outcome of processing a single inbound EDU.
#[derive(Debug, Clone, Default)]
pub struct InboundEduResult {
    /// Number of individual updates successfully applied.
    pub processed: usize,
    /// Number of updates dropped (e.g. user not found, ACL blocked).
    pub dropped: usize,
    /// Number of updates that encountered an error.
    pub errored: usize,
}

impl InboundEduResult {
    pub fn new(processed: usize, dropped: usize, errored: usize) -> Self {
        Self { processed, dropped, errored }
    }

    pub fn is_ok(&self) -> bool {
        self.errored == 0
    }
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

/// Check whether a user_id matches the given origin server.
pub fn user_matches_origin(user_id: &str, origin: &str) -> bool {
    user_id.rsplit_once(':').is_some_and(|(_, server_name)| server_name == origin)
}

// ---------------------------------------------------------------------------
// Outbound EDU helpers
// ---------------------------------------------------------------------------

/// Build a well-formed outbound EDU JSON value with the correct `edu_type`
/// field set from the [`EduType`] enum.
pub fn build_outbound_edu(edu_type: EduType, content: serde_json::Value) -> serde_json::Value {
    let mut edu = serde_json::Map::new();
    edu.insert("edu_type".to_string(), serde_json::Value::String(edu_type.as_str().to_string()));
    edu.insert("content".to_string(), content);
    serde_json::Value::Object(edu)
}

/// Build a well-formed outbound EDU JSON value that also carries a `room_id`
/// (used by `m.typing` and `m.receipt`).
pub fn build_outbound_room_edu(
    edu_type: EduType,
    room_id: &str,
    content: serde_json::Value,
) -> serde_json::Value {
    let mut edu = serde_json::Map::new();
    edu.insert("edu_type".to_string(), serde_json::Value::String(edu_type.as_str().to_string()));
    edu.insert("room_id".to_string(), serde_json::Value::String(room_id.to_string()));
    edu.insert("content".to_string(), content);
    serde_json::Value::Object(edu)
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_edu_type_from_raw() {
        assert_eq!(EduType::from_raw("m.presence"), EduType::Presence);
        assert_eq!(EduType::from_raw("m.typing"), EduType::Typing);
        assert_eq!(EduType::from_raw("m.receipt"), EduType::Receipt);
        assert_eq!(EduType::from_raw("m.device_list_update"), EduType::DeviceListUpdate);
        assert_eq!(EduType::from_raw("m.custom"), EduType::Unknown);
        assert_eq!(EduType::from_raw(""), EduType::Unknown);
    }

    #[test]
    fn test_edu_type_as_str() {
        assert_eq!(EduType::Presence.as_str(), "m.presence");
        assert_eq!(EduType::Typing.as_str(), "m.typing");
        assert_eq!(EduType::Receipt.as_str(), "m.receipt");
        assert_eq!(EduType::DeviceListUpdate.as_str(), "m.device_list_update");
        assert_eq!(EduType::Unknown.as_str(), "m.unknown");
    }

    #[test]
    fn test_edu_type_display() {
        assert_eq!(format!("{}", EduType::Presence), "m.presence");
        assert_eq!(format!("{}", EduType::Typing), "m.typing");
    }

    #[test]
    fn test_edu_type_is_processable() {
        assert!(EduType::Presence.is_processable());
        assert!(EduType::Typing.is_processable());
        assert!(EduType::Receipt.is_processable());
        assert!(EduType::DeviceListUpdate.is_processable());
        assert!(!EduType::Unknown.is_processable());
    }

    #[test]
    fn test_inbound_edu_result() {
        let r = InboundEduResult::new(3, 1, 0);
        assert_eq!(r.processed, 3);
        assert_eq!(r.dropped, 1);
        assert_eq!(r.errored, 0);
        assert!(r.is_ok());

        let r_err = InboundEduResult::new(0, 0, 1);
        assert!(!r_err.is_ok());
    }

    #[test]
    fn test_build_outbound_edu() {
        let edu = build_outbound_edu(EduType::Presence, serde_json::json!({"push": []}));
        assert_eq!(edu["edu_type"], "m.presence");
        assert!(edu.get("content").is_some());
        assert!(edu.get("room_id").is_none());
    }

    #[test]
    fn test_build_outbound_room_edu() {
        let edu = build_outbound_room_edu(
            EduType::Typing,
            "!room:example.com",
            serde_json::json!({"user_ids": []}),
        );
        assert_eq!(edu["edu_type"], "m.typing");
        assert_eq!(edu["room_id"], "!room:example.com");
        assert!(edu.get("content").is_some());
    }

    #[test]
    fn test_user_matches_origin() {
        assert!(user_matches_origin("@alice:example.com", "example.com"));
        assert!(!user_matches_origin("@alice:other.com", "example.com"));
        assert!(!user_matches_origin("invalid", "example.com"));
    }
}
