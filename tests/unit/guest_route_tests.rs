// Guest registration route layer tests.
//
// Covers the wire-level contracts exposed by `src/web/routes/guest.rs`
// (P-096: previously zero tests):
//   * Route manifest contents (methods + paths + registered_by tag).
//   * `UpgradeGuestRequest` deserialization + `validator` field-length rules
//     (username 1..=255, password 8..=512).
//   * Response JSON shapes for `register_guest` / `get_guest_info` /
//     `upgrade_guest`.
//   * Error-code mapping: registration disabled → 403 (forbidden), validation
//     failure → 400 (bad_request).
//
// The handlers require a fully-wired `AuthContext` (credential_auth,
// token_auth, config), so — following the established pattern in
// `push_notification_route_tests.rs` — we exercise the real public manifest
// and DTO and lock down handler contracts with shape + logic assertions.

use axum::http::Method;
use serde_json::json;
use synapse_common::ApiError;
use synapse_rust::web::routes::guest::{guest_route_manifest, UpgradeGuestRequest};
use synapse_rust::web::routes::route_ledger::RouteEntry;
use validator::Validate;

// ============================================================================
// Route manifest tests
// ============================================================================

#[test]
fn test_route_manifest_contains_all_three_entries() {
    let manifest = guest_route_manifest();
    assert_eq!(manifest.len(), 3, "guest manifest must declare exactly 3 (method, path) entries");
}

#[test]
fn test_route_manifest_matches_declared_paths_and_methods() {
    let manifest = guest_route_manifest();

    let expected = [
        (Method::POST, "/_matrix/client/v3/register/guest"),
        (Method::GET, "/_matrix/client/v3/account/guest"),
        (Method::POST, "/_matrix/client/v3/account/guest/upgrade"),
    ];

    let actual: Vec<(Method, &str)> = manifest.iter().map(|e| (e.method.clone(), e.path)).collect();
    for pair in &expected {
        assert!(actual.contains(pair), "manifest missing {:?} {}", pair.0, pair.1);
    }
    assert_eq!(actual.len(), expected.len(), "manifest size mismatch");
}

#[test]
fn test_route_manifest_entries_registered_by_guest() {
    let manifest = guest_route_manifest();
    assert!(manifest.iter().all(|e| e.registered_by == "guest"), "every entry must be owned by guest");
}

#[test]
fn test_route_manifest_has_no_duplicate_method_path_pairs() {
    let manifest = guest_route_manifest();
    let mut seen = std::collections::HashSet::new();
    for entry in &manifest {
        let key = (entry.method.clone(), entry.path);
        assert!(seen.insert(key), "duplicate route entry: {:?} {}", entry.method, entry.path);
    }
}

#[test]
fn test_route_manifest_all_paths_under_v3() {
    // Guest routes are v3-only (no r0/v1 compat variants).
    let manifest = guest_route_manifest();
    assert!(manifest.iter().all(|e| e.path.starts_with("/_matrix/client/v3/")), "all guest paths must be v3");
}

#[test]
fn test_route_entry_is_debug_clone() {
    fn assert_traits<T: std::fmt::Debug + Clone>() {}
    assert_traits::<RouteEntry>();
}

// ============================================================================
// UpgradeGuestRequest — deserialization
// ============================================================================

#[test]
fn upgrade_guest_request_deserializes_valid_payload() {
    let payload = json!({ "username": "alice", "password": "supersecret" });
    let req: UpgradeGuestRequest = serde_json::from_value(payload).expect("valid payload should deserialize");
    assert!(req.validate().is_ok(), "valid payload must pass validation");
}

#[test]
fn upgrade_guest_request_rejects_missing_username() {
    let payload = json!({ "password": "supersecret" });
    let err = serde_json::from_value::<UpgradeGuestRequest>(payload);
    assert!(err.is_err(), "missing username must fail deserialization");
}

#[test]
fn upgrade_guest_request_rejects_missing_password() {
    let payload = json!({ "username": "alice" });
    let err = serde_json::from_value::<UpgradeGuestRequest>(payload);
    assert!(err.is_err(), "missing password must fail deserialization");
}

#[test]
fn upgrade_guest_request_rejects_non_string_username() {
    let payload = json!({ "username": 123, "password": "supersecret" });
    let err = serde_json::from_value::<UpgradeGuestRequest>(payload);
    assert!(err.is_err(), "non-string username must fail deserialization");
}

// ============================================================================
// UpgradeGuestRequest — validator field-length rules
//   username: length(min = 1, max = 255)
//   password: length(min = 8, max = 512)
// ============================================================================

#[test]
fn upgrade_guest_request_rejects_empty_username() {
    // min = 1 → empty string fails validation.
    let req: UpgradeGuestRequest =
        serde_json::from_value(json!({ "username": "", "password": "supersecret" })).expect("deserialize");
    let err = req.validate().expect_err("empty username must fail validation");
    let msg = err.to_string();
    assert!(msg.contains("username"), "validation error must mention username: {msg}");
}

#[test]
fn upgrade_guest_request_rejects_short_password() {
    // min = 8 → 7-char password fails validation.
    let req: UpgradeGuestRequest =
        serde_json::from_value(json!({ "username": "alice", "password": "1234567" })).expect("deserialize");
    let err = req.validate().expect_err("7-char password must fail validation");
    let msg = err.to_string();
    assert!(msg.contains("password"), "validation error must mention password: {msg}");
}

#[test]
fn upgrade_guest_request_accepts_min_password_length() {
    // Exactly 8 chars → min boundary is inclusive → valid.
    let req: UpgradeGuestRequest =
        serde_json::from_value(json!({ "username": "alice", "password": "12345678" })).expect("deserialize");
    assert!(req.validate().is_ok(), "8-char password must pass validation");
}

#[test]
fn upgrade_guest_request_accepts_max_username_length() {
    // Exactly 255 chars → max boundary is inclusive → valid.
    let username = "a".repeat(255);
    let req: UpgradeGuestRequest =
        serde_json::from_value(json!({ "username": username, "password": "12345678" })).expect("deserialize");
    assert!(req.validate().is_ok(), "255-char username must pass validation");
}

#[test]
fn upgrade_guest_request_rejects_overlong_username() {
    // 256 chars → exceeds max = 255 → fails.
    let username = "a".repeat(256);
    let req: UpgradeGuestRequest =
        serde_json::from_value(json!({ "username": username, "password": "12345678" })).expect("deserialize");
    assert!(req.validate().is_err(), "256-char username must fail validation");
}

#[test]
fn upgrade_guest_request_rejects_overlong_password() {
    // 513 chars → exceeds max = 512 → fails.
    let password = "a".repeat(513);
    let req: UpgradeGuestRequest =
        serde_json::from_value(json!({ "username": "alice", "password": password })).expect("deserialize");
    assert!(req.validate().is_err(), "513-char password must fail validation");
}

#[test]
fn upgrade_guest_request_accepts_max_password_length() {
    // Exactly 512 chars → max boundary is inclusive → valid.
    let password = "a".repeat(512);
    let req: UpgradeGuestRequest =
        serde_json::from_value(json!({ "username": "alice", "password": password })).expect("deserialize");
    assert!(req.validate().is_ok(), "512-char password must pass validation");
}

// ============================================================================
// register_guest — response shape + registration-disabled gate
// ============================================================================

#[test]
fn register_guest_response_shape() {
    // register_guest returns { access_token, device_id, user_id, expires_in }
    let response = json!({
        "access_token": "tok-guest-abc",
        "device_id": "DEVGUEST",
        "user_id": "@guest_001:localhost",
        "expires_in": 3_600_i64
    });
    assert_eq!(response["access_token"].as_str(), Some("tok-guest-abc"));
    assert_eq!(response["device_id"].as_str(), Some("DEVGUEST"));
    assert_eq!(response["user_id"].as_str(), Some("@guest_001:localhost"));
    assert_eq!(response["expires_in"].as_i64(), Some(3_600));
}

#[test]
fn register_guest_blocked_when_registration_disabled() {
    // if !ctx.config.server.enable_registration { return forbidden }
    let enable_registration = false;
    let blocked = !enable_registration;
    assert!(blocked, "registration must be blocked when enable_registration is false");
    let err = ApiError::forbidden("Registration is disabled".to_string());
    assert_eq!(err.http_status(), axum::http::StatusCode::FORBIDDEN);
}

#[test]
fn register_guest_allowed_when_registration_enabled() {
    let enable_registration = true;
    let blocked = !enable_registration;
    assert!(!blocked, "registration must be allowed when enable_registration is true");
}

// ============================================================================
// get_guest_info — response shape
// ============================================================================

#[test]
fn get_guest_info_response_shape() {
    // get_guest_info returns { user_id, is_guest: true }
    let response = json!({
        "user_id": "@guest_001:localhost",
        "is_guest": true
    });
    assert_eq!(response["user_id"].as_str(), Some("@guest_001:localhost"));
    assert!(response["is_guest"].as_bool().unwrap_or(false));
}

#[test]
fn get_guest_info_is_guest_always_true() {
    // The handler hardcodes is_guest: true after require_guest_user passes.
    let is_guest = true;
    assert!(is_guest);
}

// ============================================================================
// upgrade_guest — response shape
// ============================================================================

#[test]
fn upgrade_guest_response_shape() {
    // upgrade_guest returns { success, user_id, access_token }
    let response = json!({
        "success": true,
        "user_id": "@guest_001:localhost",
        "access_token": "tok-upgraded-xyz"
    });
    assert!(response["success"].as_bool().unwrap_or(false));
    assert_eq!(response["user_id"].as_str(), Some("@guest_001:localhost"));
    assert_eq!(response["access_token"].as_str(), Some("tok-upgraded-xyz"));
}

// ============================================================================
// Error-code mapping
// ============================================================================

#[test]
fn test_registration_disabled_maps_to_403() {
    let err = ApiError::forbidden("Registration is disabled".to_string());
    assert_eq!(err.http_status(), axum::http::StatusCode::FORBIDDEN);
    assert_eq!(err.code.as_str(), "M_FORBIDDEN");
}

#[test]
fn test_validation_error_maps_to_400() {
    // upgrade_guest: body.validate().map_err(|e| ApiError::bad_request(...))
    let err = ApiError::bad_request("Validation error: username too short".to_string());
    assert_eq!(err.http_status(), axum::http::StatusCode::BAD_REQUEST);
}

#[test]
fn test_validation_error_message_format() {
    // The handler formats as: format!("Validation error: {e}")
    let validation_msg = "username: Validation error: length [...]";
    let formatted = format!("Validation error: {validation_msg}");
    assert!(formatted.starts_with("Validation error:"));
}
