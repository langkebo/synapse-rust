use crate::common::ApiError;

pub fn validate_user_id(user_id: &str) -> Result<(), ApiError> {
    if user_id.is_empty() {
        return Err(ApiError::invalid_input("user_id is required".to_string()));
    }

    if !user_id.starts_with('@') {
        return Err(ApiError::invalid_input(
            "Invalid user_id format: must start with @".to_string(),
        ));
    }

    if user_id.len() > 255 {
        return Err(ApiError::invalid_input(
            "user_id too long (max 255 characters)".to_string(),
        ));
    }

    let parts: Vec<&str> = user_id.split(':').collect();
    if parts.len() < 2 {
        return Err(ApiError::invalid_input(
            "Invalid user_id format: must be @username:server".to_string(),
        ));
    }

    let username = &parts[0][1..];
    if username.is_empty() {
        return Err(ApiError::invalid_input(
            "Invalid user_id format: username cannot be empty".to_string(),
        ));
    }

    if parts[1].is_empty() {
        return Err(ApiError::invalid_input(
            "Invalid user_id format: server cannot be empty".to_string(),
        ));
    }

    Ok(())
}

pub fn validate_room_id(room_id: &str) -> Result<(), ApiError> {
    if room_id.is_empty() {
        return Err(ApiError::invalid_input("room_id is required".to_string()));
    }
    if !room_id.starts_with('!') {
        return Err(ApiError::invalid_input(
            "Invalid room_id format: must start with !".to_string(),
        ));
    }
    if room_id.len() > 255 {
        return Err(ApiError::invalid_input(
            "room_id too long (max 255 characters)".to_string(),
        ));
    }

    let Some((localpart, server_name)) = room_id[1..].rsplit_once(':') else {
        return Err(ApiError::invalid_input(
            "Invalid room_id format: must be !roomid:server".to_string(),
        ));
    };

    if localpart.is_empty() {
        return Err(ApiError::invalid_input(
            "Invalid room_id format: room id cannot be empty".to_string(),
        ));
    }

    if server_name.is_empty() {
        return Err(ApiError::invalid_input(
            "Invalid room_id format: server cannot be empty".to_string(),
        ));
    }

    Ok(())
}

pub fn validate_room_alias(room_alias: &str) -> Result<(), ApiError> {
    if room_alias.is_empty() {
        return Err(ApiError::invalid_input(
            "room_alias is required".to_string(),
        ));
    }
    if !room_alias.starts_with('#') {
        return Err(ApiError::invalid_input(
            "Invalid room_alias format: must start with #".to_string(),
        ));
    }
    if room_alias.len() > 255 {
        return Err(ApiError::invalid_input(
            "room_alias too long (max 255 characters)".to_string(),
        ));
    }

    let Some((localpart, server_name)) = room_alias[1..].rsplit_once(':') else {
        return Err(ApiError::invalid_input(
            "Invalid room_alias format: must be #alias:server".to_string(),
        ));
    };

    if localpart.is_empty() {
        return Err(ApiError::invalid_input(
            "Invalid room_alias format: alias cannot be empty".to_string(),
        ));
    }

    if server_name.is_empty() {
        return Err(ApiError::invalid_input(
            "Invalid room_alias format: server cannot be empty".to_string(),
        ));
    }

    Ok(())
}

pub fn validate_event_id(event_id: &str) -> Result<(), ApiError> {
    if event_id.is_empty() {
        return Err(ApiError::invalid_input("event_id is required".to_string()));
    }
    if !event_id.starts_with('$') {
        return Err(ApiError::invalid_input(
            "Invalid event_id format: must start with $".to_string(),
        ));
    }
    Ok(())
}

pub fn validate_presence_status(presence: &str) -> Result<(), ApiError> {
    // "busy" 对应 MSC3026 以及 SDK PresenceManager 的 PresenceState 枚举。
    let valid_statuses = ["online", "offline", "unavailable", "away", "busy"];
    if !valid_statuses.contains(&presence) {
        return Err(ApiError::invalid_input(format!(
            "Invalid presence status. Must be one of: {}",
            valid_statuses.join(", ")
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
}
