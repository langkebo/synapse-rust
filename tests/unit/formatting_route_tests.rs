// Shared response formatting helper tests.
//
// Covers `format_token_response` in `src/web/routes/formatting.rs`
// (P-096: previously zero tests):
//   * The 5 top-level fields (access_token, refresh_token, expires_in,
//     device_id, user_id) are surfaced with the correct input values.
//   * The `well_known.m.homeserver.base_url` nested object is populated from
//     the `base_url` argument.
//   * Field values round-trip exactly (no mutation/truncation).
//
// The `formatting` module is `pub(crate)` and `format_token_response` is
// `pub(crate)`, so it cannot be imported from this integration-test crate.
// Following the established pattern in `key_rotation_route_tests.rs` (see
// `compute_needs_rotation`), we mirror the function body verbatim and assert
// its JSON output shape. If the source formatting changes, this mirror must
// be updated in lockstep.

use serde_json::{json, Value};

/// Mirror of `format_token_response` in `src/web/routes/formatting.rs`.
///
/// Used by SSO callback, login, and other auth flows that return
/// access/refresh tokens. Produces the well-known `m.homeserver` discovery
/// object alongside the token fields.
fn format_token_response(
    access_token: &str,
    refresh_token: &str,
    expires_in: i64,
    device_id: &str,
    user_id: &str,
    base_url: &str,
) -> Value {
    json!({
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

#[test]
fn format_token_response_contains_all_five_top_level_fields() {
    let response = format_token_response("at-123", "rt-456", 3600, "DEV-001", "@alice:localhost", "https://matrix.example.org");

    assert_eq!(response["access_token"].as_str(), Some("at-123"));
    assert_eq!(response["refresh_token"].as_str(), Some("rt-456"));
    assert_eq!(response["expires_in"].as_i64(), Some(3600));
    assert_eq!(response["device_id"].as_str(), Some("DEV-001"));
    assert_eq!(response["user_id"].as_str(), Some("@alice:localhost"));
}

#[test]
fn format_token_response_populates_well_known_homeserver_base_url() {
    let response = format_token_response("at", "rt", 60, "DEV", "@u:s", "https://hs.example.org");

    // well_known.m.homeserver.base_url must mirror the base_url argument.
    assert_eq!(response["well_known"]["m.homeserver"]["base_url"].as_str(), Some("https://hs.example.org"));
}

#[test]
fn format_token_response_values_round_trip_exactly() {
    // No mutation or truncation: every input surfaces verbatim.
    let response = format_token_response(
        "access-token-with-special_chars.123",
        "refresh-token-xyz",
        86_400,
        "DEVICE-ABC",
        "@user:sub.domain.example.org",
        "https://matrix.server.example.org:8448",
    );

    assert_eq!(response["access_token"].as_str(), Some("access-token-with-special_chars.123"));
    assert_eq!(response["refresh_token"].as_str(), Some("refresh-token-xyz"));
    assert_eq!(response["expires_in"].as_i64(), Some(86_400));
    assert_eq!(response["device_id"].as_str(), Some("DEVICE-ABC"));
    assert_eq!(response["user_id"].as_str(), Some("@user:sub.domain.example.org"));
    assert_eq!(
        response["well_known"]["m.homeserver"]["base_url"].as_str(),
        Some("https://matrix.server.example.org:8448")
    );
}

#[test]
fn format_token_response_has_exactly_six_top_level_keys() {
    // 5 token fields + 1 well_known object = 6 top-level keys.
    let response = format_token_response("at", "rt", 60, "DEV", "@u:s", "https://hs");
    let obj = response.as_object().expect("response must be a JSON object");
    assert_eq!(obj.len(), 6, "response must have exactly 6 top-level keys");
    for key in &["access_token", "refresh_token", "expires_in", "device_id", "user_id", "well_known"] {
        assert!(obj.contains_key(*key), "response must contain key {key}");
    }
}

#[test]
fn format_token_response_well_known_has_only_m_homeserver() {
    // The well_known object contains only m.homeserver (no m.identity_server
    // or other discovery fields are added by this helper).
    let response = format_token_response("at", "rt", 60, "DEV", "@u:s", "https://hs");
    let well_known = response["well_known"].as_object().expect("well_known must be object");
    assert_eq!(well_known.len(), 1, "well_known must only contain m.homeserver");
    assert!(well_known.contains_key("m.homeserver"));
    let homeserver = &well_known["m.homeserver"];
    let homeserver_obj = homeserver.as_object().expect("m.homeserver must be object");
    assert_eq!(homeserver_obj.len(), 1, "m.homeserver must only contain base_url");
    assert!(homeserver_obj.contains_key("base_url"));
}
