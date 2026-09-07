//! EDU (Ephemeral Data Unit) types for federation transactions.
//!
//! Pure data types and helpers. The dispatcher that routes EDUs to handlers
//! lives in the main crate (`src/federation/edu.rs`) because it depends on
//! `AppState` and the service container.

use std::str::FromStr;

// ---------------------------------------------------------------------------
// EduType — discriminant for Matrix federation EDU types
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
/// The `EduType` enum.
pub enum EduType {
    /// The `Typing` variant.
    /// The `Presence` variant.
    /// The `DeviceListUpdate` variant.
    Typing,
    /// The `Presence` variant.
    /// The `DeviceListUpdate` variant.
    Presence,
    /// The `DeviceListUpdate` variant.
    DeviceListUpdate,
    /// `m.direct_to_device` — to-device messages relayed via federation.
    DirectToDevice,
    /// `m.receipt` — read receipts propagated between federated servers.
    Receipt,
    /// `m.signing_key_update` — cross-signing key updates propagated between federated servers.
    SigningKeyUpdate,
}

#[derive(Debug, Clone)]
/// The `UnknownEduType` type.
pub struct UnknownEduType(pub String);

/// (see code)
impl std::fmt::Display for UnknownEduType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "unknown EDU type: {}", self.0)
    }
}

/// (see code)
impl std::error::Error for UnknownEduType {}

/// (see code)
impl FromStr for EduType {
    type Err = UnknownEduType;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "m.typing" => Ok(Self::Typing),
            "m.presence" => Ok(Self::Presence),
            "m.device_list_update" => Ok(Self::DeviceListUpdate),
            "m.direct_to_device" => Ok(Self::DirectToDevice),
            "m.receipt" => Ok(Self::Receipt),
            "m.signing_key_update" => Ok(Self::SigningKeyUpdate),
            other => Err(UnknownEduType(other.to_string())),
        }
    }
}

// ---------------------------------------------------------------------------
// EduProcessResult
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Default)]
/// The `EduProcessResult` type.
pub struct EduProcessResult {
    /// The `processed` field.
    /// The `dropped` field.
    /// The `errored` field.
    pub processed: usize,
    /// The `dropped` field.
    /// The `errored` field.
    pub dropped: usize,
    /// The `errored` field.
    pub errored: usize,
}

/// (see code)
impl EduProcessResult {
    /// See [`is_empty`.
    pub fn is_empty(&self) -> bool {
        self.processed == 0 && self.dropped == 0 && self.errored == 0
    }
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

/// Check that a Matrix user_id belongs to the given origin server.
pub fn user_matches_origin(user_id: &str, origin: &str) -> bool {
    user_id.rsplit_once(':').is_some_and(|(_, server_name)| server_name == origin)
}

#[cfg(test)]
mod tests {
    use super::*;

    // --- EduType ---

    #[test]
    fn test_edu_type_from_str_valid() {
        assert_eq!("m.typing".parse::<EduType>().unwrap(), EduType::Typing);
        assert_eq!("m.presence".parse::<EduType>().unwrap(), EduType::Presence);
        assert_eq!("m.device_list_update".parse::<EduType>().unwrap(), EduType::DeviceListUpdate);
        assert_eq!("m.direct_to_device".parse::<EduType>().unwrap(), EduType::DirectToDevice);
        assert_eq!("m.receipt".parse::<EduType>().unwrap(), EduType::Receipt);
        assert_eq!("m.signing_key_update".parse::<EduType>().unwrap(), EduType::SigningKeyUpdate);
    }

    #[test]
    fn test_edu_type_from_str_invalid() {
        assert!("m.unknown".parse::<EduType>().is_err());
        assert!("".parse::<EduType>().is_err());
        assert!("random".parse::<EduType>().is_err());
    }

    #[test]
    fn test_edu_type_from_str_error_message() {
        let err = "m.typo".parse::<EduType>().unwrap_err();
        assert_eq!(err.to_string(), "unknown EDU type: m.typo");
        assert_eq!(err.0, "m.typo");
    }

    #[test]
    fn test_edu_type_copy() {
        let edu = EduType::Presence;
        let copied = edu;
        assert_eq!(edu, copied);
    }

    #[test]
    fn test_edu_type_equality() {
        assert_eq!(EduType::Typing, EduType::Typing);
        assert_ne!(EduType::Typing, EduType::Presence);
        assert_ne!(EduType::Presence, EduType::DeviceListUpdate);
        assert_ne!(EduType::DirectToDevice, EduType::Typing);
        assert_ne!(EduType::DirectToDevice, EduType::DeviceListUpdate);
        assert_ne!(EduType::Receipt, EduType::Typing);
        assert_ne!(EduType::Receipt, EduType::DeviceListUpdate);
        assert_ne!(EduType::Receipt, EduType::DirectToDevice);
        assert_ne!(EduType::SigningKeyUpdate, EduType::Typing);
        assert_ne!(EduType::SigningKeyUpdate, EduType::Receipt);
    }

    #[test]
    fn test_edu_type_clone() {
        assert_eq!(EduType::Typing.clone(), EduType::Typing);
    }

    // --- UnknownEduType ---

    #[test]
    fn test_unknown_edu_type_display() {
        let err = UnknownEduType("m.custom_edu".to_string());
        assert_eq!(err.to_string(), "unknown EDU type: m.custom_edu");
    }

    #[test]
    fn test_unknown_edu_type_clone() {
        let err = UnknownEduType("test".to_string());
        assert_eq!(err.0, "test");
    }

    // --- EduProcessResult ---

    #[test]
    fn test_edu_process_result_default() {
        let result = EduProcessResult::default();
        assert_eq!(result.processed, 0);
        assert_eq!(result.dropped, 0);
        assert_eq!(result.errored, 0);
    }

    #[test]
    fn test_edu_process_result_is_empty() {
        assert!(EduProcessResult::default().is_empty());
        assert!(!EduProcessResult { processed: 1, dropped: 0, errored: 0 }.is_empty());
        assert!(!EduProcessResult { processed: 0, dropped: 1, errored: 0 }.is_empty());
        assert!(!EduProcessResult { processed: 0, dropped: 0, errored: 1 }.is_empty());
    }

    #[test]
    fn test_edu_process_result_clone() {
        let result = EduProcessResult { processed: 5, dropped: 2, errored: 1 };
        let cloned = result;
        assert_eq!(cloned.processed, 5);
        assert_eq!(cloned.dropped, 2);
        assert_eq!(cloned.errored, 1);
    }

    // --- user_matches_origin ---

    #[test]
    fn test_user_matches_origin_valid() {
        assert!(user_matches_origin("@alice:example.com", "example.com"));
        assert!(user_matches_origin("@bob:matrix.org", "matrix.org"));
    }

    #[test]
    fn test_user_matches_origin_invalid() {
        assert!(!user_matches_origin("@alice:example.com", "other.com"));
    }

    #[test]
    fn test_user_matches_origin_no_colon() {
        assert!(!user_matches_origin("plainuser", "example.com"));
    }

    #[test]
    fn test_user_matches_origin_empty_origin() {
        assert!(!user_matches_origin("@alice:example.com", ""));
    }
}
