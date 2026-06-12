//! Room service utility functions shared across room submodules.

use crate::common::{ApiError, ApiResult};

/// Validate room alias format: #alias:server
pub(crate) fn validate_room_alias_input(alias: &str) -> ApiResult<()> {
    if alias.is_empty() {
        return Err(ApiError::bad_request("room_alias is required".to_string()));
    }
    if !alias.starts_with('#') {
        return Err(ApiError::bad_request("Invalid room alias format: must start with #".to_string()));
    }
    if alias.len() > 255 {
        return Err(ApiError::bad_request("Room alias too long (max 255 characters)".to_string()));
    }

    let Some((localpart, server_name)) = alias[1..].rsplit_once(':') else {
        return Err(ApiError::bad_request("Invalid room alias format: must be #alias:server".to_string()));
    };

    if localpart.is_empty() || server_name.is_empty() {
        return Err(ApiError::bad_request("Invalid room alias format: must be #alias:server".to_string()));
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_valid_room_alias() {
        assert!(validate_room_alias_input("#test:example.com").is_ok());
        assert!(validate_room_alias_input("#room123:matrix.org").is_ok());
        assert!(validate_room_alias_input("#a:b").is_ok());
    }

    #[test]
    fn test_alias_empty_string() {
        let err = validate_room_alias_input("").unwrap_err();
        assert!(err.to_string().contains("room_alias is required"));
    }

    #[test]
    fn test_alias_missing_hash_prefix() {
        let err = validate_room_alias_input("test:example.com").unwrap_err();
        assert!(err.to_string().contains("must start with #"));
    }

    #[test]
    fn test_alias_too_long() {
        let long_alias = format!("#{}:example.com", "a".repeat(250));
        let err = validate_room_alias_input(&long_alias).unwrap_err();
        assert!(err.to_string().contains("too long"));
    }

    #[test]
    fn test_alias_max_length() {
        // 255 - 1 (for #) - 13 (for :example.com) = 241 chars for localpart
        let alias = format!("#{}:example.com", "a".repeat(241));
        assert!(validate_room_alias_input(&alias).is_ok());
    }

    #[test]
    fn test_alias_missing_colon() {
        let err = validate_room_alias_input("#testexample.com").unwrap_err();
        assert!(err.to_string().contains("must be #alias:server"));
    }

    #[test]
    fn test_alias_empty_localpart() {
        let err = validate_room_alias_input("#:example.com").unwrap_err();
        assert!(err.to_string().contains("must be #alias:server"));
    }

    #[test]
    fn test_alias_empty_server_name() {
        let err = validate_room_alias_input("#test:").unwrap_err();
        assert!(err.to_string().contains("must be #alias:server"));
    }

    #[test]
    fn test_alias_multiple_colons() {
        // rsplit_once should handle multiple colons
        assert!(validate_room_alias_input("#test:sub.example.com").is_ok());
    }

    #[test]
    fn test_alias_only_hash() {
        let err = validate_room_alias_input("#").unwrap_err();
        assert!(err.to_string().contains("must be #alias:server"));
    }

    #[test]
    fn test_alias_only_colon() {
        let err = validate_room_alias_input("#:").unwrap_err();
        assert!(err.to_string().contains("must be #alias:server"));
    }

    #[test]
    fn test_alias_special_chars_in_localpart() {
        assert!(validate_room_alias_input("#test-room_123:example.com").is_ok());
    }
}