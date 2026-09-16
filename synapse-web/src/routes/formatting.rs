// Shared response formatting helpers.

/// Format a token response JSON object — used by SSO callback, login, and
/// other authentication flows that return access/refresh tokens.
pub(crate) fn format_token_response(
    access_token: &str,
    refresh_token: &str,
    expires_in: i64,
    device_id: &str,
    user_id: &str,
    base_url: &str,
) -> serde_json::Value {
    serde_json::json!({
        "access_token": access_token,
        "refresh_token": refresh_token,
        "expires_in": expires_in,
        "device_id": device_id,
        "user_id": user_id,
        "well_known": {
            "m.homeserver": { "base_url": base_url }
        }
    })
}
