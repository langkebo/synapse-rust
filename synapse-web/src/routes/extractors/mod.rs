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
    /// See [`parse_matrix`].
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
