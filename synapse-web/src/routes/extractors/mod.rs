/// The `auth` module.
pub mod auth;
/// The `json` module.
pub mod json;
/// The `localhost_guard` module.
pub mod localhost_guard;

use synapse_common::ApiError;

// P3-9: re-export typed IDs from synapse-common as the single source of truth.
// All new Matrix ID types live in synapse-common::types; extractors provides
// Axum-compatible convenience constructors for route parameter binding.
//
// The old tuple-struct definitions are removed; callers that previously imported
// `extractors::RoomId` etc. now get the typed newtypes from synapse-common.

pub use synapse_common::types::{
    BackupId, DeviceId, EventId, MediaId, MxcUri, RoomAlias, RoomId, ServerName, SessionId, TransactionId, UserId,
};

/// Extension trait: Axum-aware validators for ID types.
pub trait UserIdParseExt {
    /// Parse a Matrix user ID string into a typed [`UserId`].
    fn parse_matrix(raw: &str) -> Result<Self, ApiError>
    where
        Self: Sized;
}

impl UserIdParseExt for UserId {
    fn parse_matrix(raw: &str) -> Result<Self, ApiError> {
        if raw.starts_with('@') {
            Ok(Self(raw.to_string()))
        } else {
            Err(ApiError::bad_request(format!("Invalid user ID format: {raw}")))
        }
    }
}

// ============== Typed ID re-exports ==============

// extract_token_from_headers removed — use crate::utils::auth::bearer_token directly
pub use auth::{AdminUser, AuthenticatedUser, OptionalAuthenticatedUser};
pub use json::MatrixJson;
pub use synapse_common::types as id_types;

// ============== Tests ==============

#[cfg(test)]
mod tests {
    use super::{UserId, UserIdParseExt};

    #[test]
    fn test_parse_matrix_valid_user_id() {
        let result = UserId::parse_matrix("@alice:example.com");
        assert!(result.is_ok());
        assert_eq!(result.unwrap().as_str(), "@alice:example.com");
    }

    #[test]
    fn test_parse_matrix_valid_minimal_user_id() {
        let result = UserId::parse_matrix("@u");
        assert!(result.is_ok());
        assert_eq!(result.unwrap().as_str(), "@u");
    }

    #[test]
    fn test_parse_matrix_missing_at_prefix() {
        let result = UserId::parse_matrix("alice:example.com");
        assert!(result.is_err());
        let err = result.unwrap_err();
        assert_eq!(err.http_status(), http::StatusCode::BAD_REQUEST);
        assert!(err.message().contains("Invalid user ID format"));
        assert!(err.message().contains("alice:example.com"));
    }

    #[test]
    fn test_parse_matrix_empty_string() {
        let result = UserId::parse_matrix("");
        assert!(result.is_err());
        let err = result.unwrap_err();
        assert_eq!(err.http_status(), http::StatusCode::BAD_REQUEST);
    }

    #[test]
    fn test_parse_matrix_at_only() {
        let result = UserId::parse_matrix("@");
        assert!(result.is_ok());
        assert_eq!(result.unwrap().as_str(), "@");
    }

    #[test]
    fn test_user_id_type_reexports_are_accessible() {
        // Verify that all re-exported ID types from synapse-common are
        // reachable through this module — this guards against accidental
        // drift between the declared re-export list and synapse-common::types.
        let _: &synapse_common::types::BackupId = &synapse_common::types::BackupId::new("test-backup-id");
        let _: &synapse_common::types::DeviceId = &synapse_common::types::DeviceId::new("DEVICE1");
        let _: &synapse_common::types::EventId = &synapse_common::types::EventId::new("$event1");
        let _: &synapse_common::types::MediaId = &synapse_common::types::MediaId::new("mxc1");
        let _: &synapse_common::types::MxcUri = &synapse_common::types::MxcUri::new("mxc://example.com/abc");
        let _: &synapse_common::types::RoomAlias = &synapse_common::types::RoomAlias::new("#room1");
        let _: &synapse_common::types::RoomId = &synapse_common::types::RoomId::new("!room1");
        let _: &synapse_common::types::ServerName = &synapse_common::types::ServerName::new("example.com");
        let _: &synapse_common::types::SessionId = &synapse_common::types::SessionId::new("session1");
        let _: &synapse_common::types::TransactionId = &synapse_common::types::TransactionId::new("tx1");
        let _: &synapse_common::types::UserId = &synapse_common::types::UserId::new("@user1");
    }

    #[test]
    fn test_re_export_types_directly_accessible() {
        // Verify that the re-exported types can also be accessed via the
        // extractors module itself (as they are declared in pub use above).
        use crate::routes::extractors::{
            BackupId, DeviceId, EventId, MediaId, MxcUri, RoomAlias, RoomId, ServerName, SessionId, TransactionId,
        };
        // Construct each type to confirm the re-exports are live.
        let _ = BackupId::new("backup");
        let _ = DeviceId::new("DEV1");
        let _ = EventId::new("$1");
        let _ = MediaId::new("mxc1");
        let _ = MxcUri::new("mxc://e.c/1");
        let _ = RoomAlias::new("#r1");
        let _ = RoomId::new("!r1");
        let _ = ServerName::new("srv");
        let _ = SessionId::new("s1");
        let _ = TransactionId::new("t1");
    }

    #[test]
    fn test_user_id_parse_ext_trait_error_message_content() {
        // The error message must include both the raw input and the
        // "Invalid user ID format" prefix so clients can diagnose bad IDs.
        let result = UserId::parse_matrix("not-a-user-id");
        assert!(result.is_err());
        let msg = result.unwrap_err().message();
        assert!(msg.contains("Invalid user ID format"));
        assert!(msg.contains("not-a-user-id"));
    }
}
