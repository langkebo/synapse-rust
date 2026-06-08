//! Room service utility functions shared across room submodules.

use synapse_common::{ApiError, ApiResult};

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