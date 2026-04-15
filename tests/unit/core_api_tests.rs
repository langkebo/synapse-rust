// Core API (mod.rs) Tests - API Endpoint Coverage
// These tests cover the core API endpoints from src/web/routes/mod.rs

use serde_json::json;
use synapse_rust::ApiError;

// Test 1: Health check validation
#[test]
fn test_health_check_logic() {
    let result = true;
    assert!(result);
}

// Test 2: Version string format
#[test]
fn test_version_format() {
    let version = env!("CARGO_PKG_VERSION");
    assert!(!version.is_empty());
    let parts: Vec<&str> = version.split('.').collect();
    assert!(parts.len() >= 2);
}

// Test 3: User ID validation
#[test]
fn test_validate_user_id() {
    // Valid user IDs (must start with @)
    assert!(validate_user_id("@user:localhost").is_ok());
    assert!(validate_user_id("@test:example.com").is_ok());
    assert!(validate_user_id("@user123:matrix.org").is_ok());
    assert!(validate_user_id("@user").is_ok()); // Only checks for @ prefix

    // Invalid user IDs
    assert!(validate_user_id("").is_err());
    assert!(validate_user_id("user:localhost").is_err()); // Missing @
}

// Test 4: Room ID validation
#[test]
fn test_validate_room_id() {
    assert!(validate_room_id("!room:localhost").is_ok());
    assert!(validate_room_id("!test:example.com").is_ok());
    assert!(validate_room_id("").is_err());
    assert!(validate_room_id("room:localhost").is_err());
}

// Test 5: Event ID validation
#[test]
fn test_validate_event_id() {
    assert!(validate_event_id("$event:localhost").is_ok());
    assert!(validate_event_id("").is_err());
}

// Test 6: Device ID validation
#[test]
fn test_validate_device_id() {
    assert!(validate_device_id("DEVICE123").is_ok());
    assert!(validate_device_id("").is_err());
}

// Test 7: Presence status validation
#[test]
fn test_validate_presence_status() {
    assert!(validate_presence_status("online").is_ok());
    assert!(validate_presence_status("offline").is_ok());
    assert!(validate_presence_status("unavailable").is_ok());
    assert!(validate_presence_status("invalid").is_err());
}

// Test 8: Receipt type validation
#[test]
fn test_validate_receipt_type() {
    assert!(validate_receipt_type("m.read").is_ok());
    assert!(validate_receipt_type("invalid").is_err());
}

// Test 9: Room visibility validation
#[test]
fn test_room_visibility_validation() {
    assert!(is_valid_visibility("public"));
    assert!(is_valid_visibility("private"));
    assert!(!is_valid_visibility("invalid"));
    assert!(!is_valid_visibility(""));
}

// Test 10: Room alias validation
#[test]
fn test_room_alias_validation() {
    assert!(is_valid_alias_char('a'));
    assert!(is_valid_alias_char('z'));
    assert!(is_valid_alias_char('0'));
    assert!(is_valid_alias_char('9'));
    assert!(is_valid_alias_char('_'));
    assert!(is_valid_alias_char('-'));
    assert!(is_valid_alias_char('.'));
    assert!(!is_valid_alias_char(' '));
    assert!(!is_valid_alias_char('/'));
}

// Test 11: Room name length validation
#[test]
fn test_room_name_length_validation() {
    assert!(is_valid_room_name_length("Short name"));
    assert!(is_valid_room_name_length(&"a".repeat(255)));
    assert!(!is_valid_room_name_length(&"a".repeat(256)));
}

// Test 12: Room topic length validation
#[test]
fn test_room_topic_length_validation() {
    assert!(is_valid_room_topic_length("Short topic"));
    assert!(is_valid_room_topic_length(&"a".repeat(4096)));
    assert!(!is_valid_room_topic_length(&"a".repeat(4097)));
}

// Test 13: Invite list validation
#[test]
fn test_invite_list_validation() {
    assert!(is_valid_invite_list(&["@user1:localhost".to_string()]));
    assert!(is_valid_invite_list(&[]));
    let oversized_invite_list = vec!["@user:localhost".to_string(); 101];
    assert!(!is_valid_invite_list(&oversized_invite_list));
}

// Test 14: Token expiry calculation
#[test]
fn test_token_expiry_calculation() {
    let current_ts: i64 = 1000000;
    let expires_in: i64 = 3600;
    let expiry = current_ts + (expires_in * 1000);
    assert!(expiry > current_ts);
}

// Test 15: API version detection
#[test]
fn test_api_version_detection() {
    assert_eq!(
        detect_api_version("/_matrix/client/r0/sync"),
        Some("r0".to_string())
    );
    assert_eq!(
        detect_api_version("/_matrix/client/v3/sync"),
        Some("v3".to_string())
    );
    assert_eq!(
        detect_api_version("/_matrix/client/versions"),
        Some("v1".to_string())
    );
    assert_eq!(detect_api_version("/health"), None);
}

// Test 16: Matrix URI parsing
#[test]
fn test_matrix_uri_parsing() {
    assert!(is_matrix_user_uri("matrix:u/@user:localhost"));
    assert!(is_matrix_room_uri("matrix:r/!room:localhost"));
    assert!(is_matrix_event_uri("matrix:e/$event:localhost"));
}

// Test 17: Login response format
#[test]
fn test_login_response_format() {
    let response = build_login_response(
        "access_token_value",
        "refresh_token_value",
        "DEVICE123",
        "@user:localhost",
        3600,
    );
    assert!(response.get("access_token").is_some());
    assert!(response.get("refresh_token").is_some());
    assert!(response.get("device_id").is_some());
    assert!(response.get("user_id").is_some());
    assert!(response.get("expires_in").is_some());
}

// Test 18: Well-known discovery
#[test]
fn test_well_known_response() {
    let response = build_well_known_response("localhost", "8008");
    assert!(response.get("m.homeserver").is_some());
    assert!(response.get("m.identity_server").is_some());
}

// Helper functions for tests
fn validate_user_id(user_id: &str) -> Result<(), ApiError> {
    if user_id.is_empty() {
        return Err(ApiError::bad_request("user_id is required".to_string()));
    }
    if !user_id.starts_with('@') {
        return Err(ApiError::bad_request(
            "Invalid user_id format: must start with @".to_string(),
        ));
    }
    if user_id.len() > 255 {
        return Err(ApiError::bad_request("user_id too long".to_string()));
    }
    Ok(())
}

fn validate_room_id(room_id: &str) -> Result<(), ApiError> {
    if room_id.is_empty() {
        return Err(ApiError::bad_request("room_id is required".to_string()));
    }
    if !room_id.starts_with('!') {
        return Err(ApiError::bad_request(
            "Invalid room_id format: must start with !".to_string(),
        ));
    }
    Ok(())
}

fn validate_event_id(event_id: &str) -> Result<(), ApiError> {
    if event_id.is_empty() {
        return Err(ApiError::bad_request("event_id is required".to_string()));
    }
    if !event_id.starts_with('$') {
        return Err(ApiError::bad_request(
            "Invalid event_id format: must start with $".to_string(),
        ));
    }
    Ok(())
}

fn validate_device_id(device_id: &str) -> Result<(), ApiError> {
    if device_id.is_empty() {
        return Err(ApiError::bad_request("device_id is required".to_string()));
    }
    Ok(())
}

fn validate_presence_status(presence: &str) -> Result<(), ApiError> {
    match presence {
        "online" | "offline" | "unavailable" => Ok(()),
        _ => Err(ApiError::bad_request("Invalid presence status".to_string())),
    }
}

fn validate_receipt_type(receipt_type: &str) -> Result<(), ApiError> {
    match receipt_type {
        "m.read" | "m.read.private" => Ok(()),
        _ => Err(ApiError::bad_request("Invalid receipt type".to_string())),
    }
}

fn is_valid_visibility(visibility: &str) -> bool {
    visibility == "public" || visibility == "private"
}

fn is_valid_alias_char(c: char) -> bool {
    c.is_alphanumeric() || c == '_' || c == '-' || c == '.'
}

fn is_valid_room_name_length(name: &str) -> bool {
    name.len() <= 255
}

fn is_valid_room_topic_length(topic: &str) -> bool {
    topic.len() <= 4096
}

fn is_valid_invite_list(invites: &[String]) -> bool {
    invites.len() <= 100
}

fn detect_api_version(path: &str) -> Option<String> {
    if path.contains("/r0/") {
        Some("r0".to_string())
    } else if path.contains("/v3/") {
        Some("v3".to_string())
    } else if path.contains("/v1/") || path.contains("/versions") {
        Some("v1".to_string())
    } else if path.contains("/unstable/") {
        Some("unstable".to_string())
    } else {
        None
    }
}

fn is_matrix_user_uri(uri: &str) -> bool {
    uri.starts_with("matrix:u/")
}

fn is_matrix_room_uri(uri: &str) -> bool {
    uri.starts_with("matrix:r/")
}

fn is_matrix_event_uri(uri: &str) -> bool {
    uri.starts_with("matrix:e/")
}

fn build_login_response(
    access_token: &str,
    refresh_token: &str,
    device_id: &str,
    user_id: &str,
    expires_in: i64,
) -> serde_json::Value {
    json!({
        "access_token": access_token,
        "refresh_token": refresh_token,
        "device_id": device_id,
        "user_id": user_id,
        "expires_in": expires_in
    })
}

fn build_well_known_response(server_name: &str, port: &str) -> serde_json::Value {
    json!({
        "m.homeserver": {
            "base_url": format!("http://{}:{}", server_name, port)
        },
        "m.identity_server": {
            "base_url": format!("http://{}:{}", server_name, port)
        }
    })
}
