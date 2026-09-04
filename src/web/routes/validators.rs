// Route-layer lightweight parameter validation.
//
// These free functions perform fast structural checks (prefix, length, colon
// split) on HTTP request parameters. They intentionally do NOT use regex so
// they can be called without constructing a `Validator` instance.
//
// For full business-layer validation (username/password policy, regex-based
// Matrix ID format checks, email, device_id, url, etc.) use
// `synapse_common::validation::Validator` (injected via Context as
// `Arc<Validator>`). The two modules are complementary, not duplicates:
//   - validators.rs  → route-layer quick reject (presence, receipt_type,
//                      membership, room_alias, event_id, structural ID checks)
//   - validation.rs  → business-layer policy (password strength, email regex,
//                      username localpart charset, timestamp window, etc.)
use crate::common::{ApiError, PresenceState};

pub fn validate_user_id(user_id: &str) -> Result<(), ApiError> {
    if user_id.is_empty() {
        return Err(ApiError::invalid_input("user_id is required".to_string()));
    }

    if !user_id.starts_with('@') {
        return Err(ApiError::invalid_input("Invalid user_id format: must start with @".to_string()));
    }

    if user_id.len() > 255 {
        return Err(ApiError::invalid_input("user_id too long (max 255 characters)".to_string()));
    }

    let parts: Vec<&str> = user_id.split(':').collect();
    if parts.len() < 2 {
        return Err(ApiError::invalid_input("Invalid user_id format: must be @username:server".to_string()));
    }

    let username = &parts[0][1..];
    if username.is_empty() {
        return Err(ApiError::invalid_input("Invalid user_id format: username cannot be empty".to_string()));
    }

    if parts[1].is_empty() {
        return Err(ApiError::invalid_input("Invalid user_id format: server cannot be empty".to_string()));
    }

    Ok(())
}

pub fn validate_room_id(room_id: &str) -> Result<(), ApiError> {
    if room_id.is_empty() {
        return Err(ApiError::invalid_input("room_id is required".to_string()));
    }
    if !room_id.starts_with('!') {
        return Err(ApiError::invalid_input("Invalid room_id format: must start with !".to_string()));
    }
    if room_id.len() > 255 {
        return Err(ApiError::invalid_input("room_id too long (max 255 characters)".to_string()));
    }

    let Some((localpart, server_name)) = room_id[1..].rsplit_once(':') else {
        return Err(ApiError::invalid_input("Invalid room_id format: must be !roomid:server".to_string()));
    };

    if localpart.is_empty() {
        return Err(ApiError::invalid_input("Invalid room_id format: room id cannot be empty".to_string()));
    }

    if server_name.is_empty() {
        return Err(ApiError::invalid_input("Invalid room_id format: server cannot be empty".to_string()));
    }

    Ok(())
}

pub fn validate_room_alias(room_alias: &str) -> Result<(), ApiError> {
    if room_alias.is_empty() {
        return Err(ApiError::invalid_input("room_alias is required".to_string()));
    }
    if !room_alias.starts_with('#') {
        return Err(ApiError::invalid_input("Invalid room_alias format: must start with #".to_string()));
    }
    if room_alias.len() > 255 {
        return Err(ApiError::invalid_input("room_alias too long (max 255 characters)".to_string()));
    }

    let Some((localpart, server_name)) = room_alias[1..].rsplit_once(':') else {
        return Err(ApiError::invalid_input("Invalid room_alias format: must be #alias:server".to_string()));
    };

    if localpart.is_empty() {
        return Err(ApiError::invalid_input("Invalid room_alias format: alias cannot be empty".to_string()));
    }

    if server_name.is_empty() {
        return Err(ApiError::invalid_input("Invalid room_alias format: server cannot be empty".to_string()));
    }

    Ok(())
}

/// Maximum length of a Matrix event_id.
///
/// A `$`-prefixed SHA-256 hash in standard Matrix encoding fits comfortably
/// under 64 chars; 255 is a generous upper bound that still rejects
/// pathological inputs (e.g. 1 MB event_ids used to cause full-index scans
/// in earlier Postgres versions).
pub const MAX_EVENT_ID_LEN: usize = 255;

pub fn validate_event_id(event_id: &str) -> Result<(), ApiError> {
    if event_id.is_empty() {
        return Err(ApiError::invalid_input("event_id is required".to_string()));
    }
    if event_id.len() > MAX_EVENT_ID_LEN {
        return Err(ApiError::invalid_input(format!(
            "event_id too long: {} bytes (max {})",
            event_id.len(),
            MAX_EVENT_ID_LEN
        )));
    }
    if !event_id.starts_with('$') {
        return Err(ApiError::invalid_input("Invalid event_id format: must start with $".to_string()));
    }
    Ok(())
}

pub fn validate_presence_status(presence: &str) -> Result<(), ApiError> {
    if !PresenceState::valid_strs().contains(&presence) {
        return Err(ApiError::invalid_input(format!(
            "Invalid presence status. Must be one of: {}",
            PresenceState::valid_strs().join(", ")
        )));
    }
    Ok(())
}

pub fn validate_receipt_type(receipt_type: &str) -> Result<(), ApiError> {
    let valid_types = ["m.read", "m.read.private"];
    if !valid_types.contains(&receipt_type) {
        return Err(ApiError::invalid_input(format!(
            "Invalid receipt type. Must be one of: {}",
            valid_types.join(", ")
        )));
    }
    Ok(())
}

pub fn validate_membership(membership: &str) -> Result<(), ApiError> {
    let valid_memberships = ["join", "leave", "invite", "ban", "knock"];
    if !valid_memberships.contains(&membership) {
        return Err(ApiError::invalid_input(format!(
            "Invalid membership value. Must be one of: {}",
            valid_memberships.join(", ")
        )));
    }
    Ok(())
}

/// Validate a Matrix homeserver name (e.g. `matrix.org`).
///
/// This is the lightweight counterpart of `Path<ServerName>`: the typed ID
/// extractor covers the axum-bound case (route parameters), while this
/// function is for hand-built server names (configs, body fields, query
/// strings) that need the same defense-in-depth structural checks.
///
/// Matrix spec §1.5 only requires non-empty + bounded length for server
/// names (full DNS / IP / port grammar lives in the spec). We additionally
/// reject path-traversal and NUL bytes to prevent `../` or NUL injection
/// into downstream filesystem / SQL / log paths.
pub fn validate_server_name(server_name: &str) -> Result<(), ApiError> {
    if server_name.is_empty() {
        return Err(ApiError::invalid_input("server_name is required".to_string()));
    }
    if server_name.len() > 255 {
        return Err(ApiError::invalid_input(format!("server_name too long: {} bytes (max 255)", server_name.len())));
    }
    if server_name.contains('/') || server_name.contains('\\') || server_name.contains('\0') {
        return Err(ApiError::invalid_input("Invalid server_name: must not contain /, \\, or NUL".to_string()));
    }
    Ok(())
}

/// Validate an Application Service ID (opaque token, Matrix spec §13.9).
///
/// AS IDs are application-specific strings with no Matrix-mandated format,
/// so the only structural checks are non-empty + bounded length. Used by
/// `_synapse/admin/v1/appservices/{as_id}/*` and `/_matrix/app/v1/{as_id}`
/// route handlers as defense-in-depth alongside `app_service_auth_middleware`.
pub fn validate_as_id(as_id: &str) -> Result<(), ApiError> {
    if as_id.is_empty() {
        return Err(ApiError::invalid_input("as_id is required".to_string()));
    }
    if as_id.len() > 255 {
        return Err(ApiError::invalid_input(format!("as_id too long: {} bytes (max 255)", as_id.len())));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_validate_user_id_valid() {
        assert!(validate_user_id("@alice:example.com").is_ok());
        assert!(validate_user_id("@bob:matrix.org").is_ok());
        assert!(validate_user_id("@user:localhost").is_ok());
    }

    #[test]
    fn test_validate_user_id_invalid() {
        assert!(validate_user_id("").is_err());
        assert!(validate_user_id("alice").is_err());
        assert!(validate_user_id("@").is_err());
        assert!(validate_user_id("@:example.com").is_err());
        assert!(validate_user_id("@alice:").is_err());
    }

    #[test]
    fn test_validate_room_id_valid() {
        assert!(validate_room_id("!room:example.com").is_ok());
        assert!(validate_room_id("!abc123:matrix.org").is_ok());
    }

    #[test]
    fn test_validate_room_id_invalid() {
        assert!(validate_room_id("").is_err());
        assert!(validate_room_id("room:example.com").is_err());
        assert!(validate_room_id("!anything").is_err());
        assert!(validate_room_id("!:example.com").is_err());
        assert!(validate_room_id("!room:").is_err());
    }

    #[test]
    fn test_validate_room_alias_valid() {
        assert!(validate_room_alias("#room:example.com").is_ok());
        assert!(validate_room_alias("#room-name:matrix.org").is_ok());
    }

    #[test]
    fn test_validate_room_alias_invalid() {
        assert!(validate_room_alias("").is_err());
        assert!(validate_room_alias("room:example.com").is_err());
        assert!(validate_room_alias("#:example.com").is_err());
        assert!(validate_room_alias("#room").is_err());
        assert!(validate_room_alias("#room:").is_err());
    }

    #[test]
    fn test_validate_event_id_valid() {
        assert!(validate_event_id("$event123:example.com").is_ok());
    }

    #[test]
    fn test_validate_event_id_invalid() {
        assert!(validate_event_id("").is_err());
        assert!(validate_event_id("event123").is_err());
        // Length limit: 255 chars max
        assert!(validate_event_id(&format!("${}", "x".repeat(255))).is_err());
        assert!(validate_event_id(&format!("${}", "x".repeat(254))).is_ok());
    }

    #[test]
    fn test_validate_presence_status() {
        assert!(validate_presence_status("online").is_ok());
        assert!(validate_presence_status("offline").is_ok());
        assert!(validate_presence_status("unavailable").is_ok());
        assert!(validate_presence_status("away").is_ok());
        assert!(validate_presence_status("busy").is_ok());
        assert!(validate_presence_status("sleeping").is_err());
    }

    #[test]
    fn test_validate_receipt_type() {
        assert!(validate_receipt_type("m.read").is_ok());
        assert!(validate_receipt_type("m.read.private").is_ok());
        assert!(validate_receipt_type("m.read.core").is_err());
    }

    #[test]
    fn test_validate_membership_valid() {
        assert!(validate_membership("join").is_ok());
        assert!(validate_membership("leave").is_ok());
        assert!(validate_membership("invite").is_ok());
        assert!(validate_membership("ban").is_ok());
        assert!(validate_membership("knock").is_ok());
    }

    #[test]
    fn test_validate_membership_invalid() {
        assert!(validate_membership("kicked").is_err());
        assert!(validate_membership("banned").is_err());
        assert!(validate_membership("").is_err());
        assert!(validate_membership("pending").is_err());
    }

    #[test]
    fn test_validate_server_name_valid() {
        assert!(validate_server_name("matrix.org").is_ok());
        assert!(validate_server_name("localhost").is_ok());
        assert!(validate_server_name("192.168.1.1").is_ok());
        assert!(validate_server_name("example.com:8448").is_ok());
        assert!(validate_server_name(&"a".repeat(255)).is_ok());
    }

    #[test]
    fn test_validate_server_name_invalid() {
        assert!(validate_server_name("").is_err());
        assert!(validate_server_name(&"x".repeat(256)).is_err());
        assert!(validate_server_name("matrix.org/..").is_err());
        assert!(validate_server_name("evil\\server").is_err());
        assert!(validate_server_name("bad\0server").is_err());
    }

    #[test]
    fn test_validate_as_id_valid() {
        assert!(validate_as_id("my_appservice").is_ok());
        assert!(validate_as_id("tjg_bridge").is_ok());
        assert!(validate_as_id("APISERVICE123").is_ok());
        assert!(validate_as_id(&"a".repeat(255)).is_ok());
    }

    #[test]
    fn test_validate_as_id_invalid() {
        assert!(validate_as_id("").is_err());
        assert!(validate_as_id(&"x".repeat(256)).is_err());
    }
}
