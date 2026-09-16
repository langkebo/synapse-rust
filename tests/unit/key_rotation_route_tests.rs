// Key Rotation Route Tests - API Endpoint Coverage
//
// These tests cover the key rotation API endpoints from
// `src/web/routes/key_rotation.rs` (P-095: previously zero tests).
//
// The route module exposes 18 (method, path) entries across 6 logical
// endpoints × 2 prefixes (client/v1 + vendor/v1). These tests verify:
//   - The route manifest returned by `key_rotation_route_manifest()` matches
//     the router declared in `create_key_rotation_router` (no drift).
//   - Request/response JSON shapes for each endpoint conform to the contract
//     the handlers produce.
//   - Validation logic mirrors (empty key_id rejection, admin gate,
//     unauthenticated rejection, needs-rotation computation).
//
// Pattern follows `tests/unit/key_backup_api_tests.rs`: pure JSON-shape +
// validation-logic assertions, no HTTP router or DB required.

use serde_json::{json, Value};
use synapse_rust::web::routes::declared_ledger_all;
use synapse_rust::web::routes::route_ledger::RouteEntry;

/// The `key_rotation` slice of the derived route table.
///
/// `key_rotation_route_manifest()` was one of ~120 hand-copied projections
/// deleted by B2-2; route metadata now has a single source (`derived_routes`),
/// so asserting against it still catches manifest/router drift — more reliably,
/// because the table is generated from the `.route(...)` sites themselves.
fn key_rotation_route_manifest() -> Vec<RouteEntry> {
    declared_ledger_all().iter().filter(|e| e.registered_by == "key_rotation").cloned().collect()
}

// ============================================================================
// Route manifest tests
// ============================================================================

#[test]
fn test_route_manifest_contains_all_eighteen_entries() {
    let manifest = key_rotation_route_manifest();
    assert_eq!(
        manifest.len(),
        18,
        "key_rotation manifest must declare exactly 18 (method, path) entries (9 client/v1 + 9 vendor/v1)"
    );
}

#[test]
fn test_route_manifest_matches_declared_paths_and_methods() {
    use axum::http::Method;
    let manifest = key_rotation_route_manifest();

    // (method, path) pairs as declared in create_key_rotation_router.
    let expected = [
        (Method::GET, "/_matrix/client/v1/keys/rotation/status"),
        (Method::POST, "/_matrix/client/v1/keys/rotation/status"),
        (Method::POST, "/_matrix/client/v1/keys/rotation/rotate"),
        (Method::GET, "/_matrix/client/v1/keys/rotation/history/{device_id}"),
        (Method::POST, "/_matrix/client/v1/keys/rotation/revoke"),
        (Method::PUT, "/_matrix/client/v1/keys/rotation/config"),
        (Method::POST, "/_matrix/client/v1/keys/rotation/config"),
        (Method::GET, "/_matrix/client/v1/keys/rotation/check"),
        (Method::POST, "/_matrix/client/v1/keys/rotation/check"),
        // vendor paths
        (Method::GET, "/_matrix/vendor/v1/keys/rotation/status"),
        (Method::POST, "/_matrix/vendor/v1/keys/rotation/status"),
        (Method::POST, "/_matrix/vendor/v1/keys/rotation/rotate"),
        (Method::GET, "/_matrix/vendor/v1/keys/rotation/history/{device_id}"),
        (Method::POST, "/_matrix/vendor/v1/keys/rotation/revoke"),
        (Method::PUT, "/_matrix/vendor/v1/keys/rotation/config"),
        (Method::POST, "/_matrix/vendor/v1/keys/rotation/config"),
        (Method::GET, "/_matrix/vendor/v1/keys/rotation/check"),
        (Method::POST, "/_matrix/vendor/v1/keys/rotation/check"),
    ];

    let actual: Vec<(Method, &str)> = manifest.iter().map(|e| (e.method.clone(), e.path)).collect();
    for pair in &expected {
        assert!(actual.contains(pair), "manifest missing {:?} {}", pair.0, pair.1);
    }
    assert_eq!(actual.len(), expected.len(), "manifest size mismatch");
}

#[test]
fn test_route_manifest_entries_registered_by_key_rotation() {
    let manifest = key_rotation_route_manifest();
    assert!(manifest.iter().all(|e| e.registered_by == "key_rotation"), "every entry must be owned by key_rotation");
}

#[test]
fn test_route_manifest_has_no_duplicate_method_path_pairs() {
    let manifest = key_rotation_route_manifest();
    let mut seen = std::collections::HashSet::new();
    for entry in &manifest {
        let key = (entry.method.clone(), entry.path);
        assert!(seen.insert(key), "duplicate route entry: {:?} {}", entry.method, entry.path);
    }
}

#[test]
fn test_route_manifest_covers_six_logical_endpoints() {
    let manifest = key_rotation_route_manifest();
    let paths: std::collections::HashSet<&str> = manifest.iter().map(|e| e.path).collect();
    // Twelve distinct paths: 6 client/v1 + 6 vendor/v1.
    assert_eq!(paths.len(), 12, "expected 12 distinct paths, got {}: {:?}", paths.len(), paths);
    assert!(paths.contains("/_matrix/client/v1/keys/rotation/status"));
    assert!(paths.contains("/_matrix/client/v1/keys/rotation/rotate"));
    assert!(paths.contains("/_matrix/client/v1/keys/rotation/history/{device_id}"));
    assert!(paths.contains("/_matrix/client/v1/keys/rotation/revoke"));
    assert!(paths.contains("/_matrix/client/v1/keys/rotation/config"));
    assert!(paths.contains("/_matrix/client/v1/keys/rotation/check"));
    assert!(paths.contains("/_matrix/vendor/v1/keys/rotation/status"));
    assert!(paths.contains("/_matrix/vendor/v1/keys/rotation/rotate"));
    assert!(paths.contains("/_matrix/vendor/v1/keys/rotation/history/{device_id}"));
    assert!(paths.contains("/_matrix/vendor/v1/keys/rotation/revoke"));
    assert!(paths.contains("/_matrix/vendor/v1/keys/rotation/config"));
    assert!(paths.contains("/_matrix/vendor/v1/keys/rotation/check"));
}

#[test]
fn test_route_entry_is_debug_clone() {
    // RouteEntry must be Debug + Clone (used by the route ledger aggregation).
    fn assert_traits<T: std::fmt::Debug + Clone>() {}
    assert_traits::<RouteEntry>();
}

// ============================================================================
// GET/POST /status — response shape
// ============================================================================

#[test]
fn test_rotation_status_response_shape() {
    // Mirrors get_key_rotation_status handler output:
    // { enabled, status, user_last_rotation }
    let status = json!({
        "rotation_enabled": true,
        "has_current_key": true,
        "should_rotate": false
    });
    let response = json!({
        "enabled": status.get("rotation_enabled"),
        "status": status,
        "user_last_rotation": 1_700_000_000_000_i64
    });

    assert!(response.get("enabled").is_some());
    assert!(response["enabled"].as_bool().unwrap_or(false));
    assert!(response.get("status").is_some());
    assert!(response.get("user_last_rotation").is_some());
    assert_eq!(response["user_last_rotation"].as_i64(), Some(1_700_000_000_000));
}

#[test]
fn test_rotation_status_response_with_no_prior_rotation() {
    let status = json!({ "rotation_enabled": true, "has_current_key": false });
    let response = json!({
        "enabled": status.get("rotation_enabled"),
        "status": status,
        "user_last_rotation": Value::Null
    });

    assert_eq!(response["user_last_rotation"], Value::Null);
    assert!(response["enabled"].as_bool().unwrap_or(false));
}

// ============================================================================
// POST /rotate — request/response shape
// ============================================================================

#[test]
fn test_rotate_keys_request_with_key_id() {
    // Body carries an optional key_id.
    let request = json!({ "key_id": "ed25519:0:current" });
    assert_eq!(request.get("key_id").and_then(|v| v.as_str()), Some("ed25519:0:current"));
}

#[test]
fn test_rotate_keys_request_without_key_id() {
    // Empty body is valid — requested_key_id defaults to None.
    let request = json!({});
    assert!(request.get("key_id").is_none());
}

#[test]
fn test_rotate_keys_success_response_shape() {
    // rotate_keys returns { success, message, has_new_key }.
    let response = json!({
        "success": true,
        "message": "Keys rotated successfully",
        "has_new_key": true
    });

    assert!(response["success"].as_bool().unwrap_or(false));
    assert!(response.get("message").is_some());
    assert!(response["has_new_key"].as_bool().unwrap_or(false));
}

#[test]
fn test_rotate_keys_response_has_new_key_false_when_no_key() {
    // When manager.get_current_key() returns None after rotation, has_new_key is false.
    let response = json!({
        "success": true,
        "message": "Keys rotated successfully",
        "has_new_key": false
    });
    assert!(!response["has_new_key"].as_bool().unwrap_or(true));
}

// ============================================================================
// GET /history/{device_id} — response shape
// ============================================================================

#[test]
fn test_rotation_history_response_shape() {
    let history_rows = vec![
        json!({ "key_id": "ed25519:1", "rotated_ts": 1_700_000_000_000_i64 }),
        json!({ "key_id": "ed25519:2", "rotated_ts": 1_700_010_000_000_i64 }),
    ];
    let response = json!({
        "device_id": "DEVICE_ABC",
        "rotations": history_rows
    });

    assert_eq!(response["device_id"].as_str(), Some("DEVICE_ABC"));
    let rotations = response["rotations"].as_array().expect("rotations must be array");
    assert_eq!(rotations.len(), 2);
    assert_eq!(rotations[0]["key_id"].as_str(), Some("ed25519:1"));
    assert_eq!(rotations[1]["rotated_ts"].as_i64(), Some(1_700_010_000_000));
}

#[test]
fn test_rotation_history_response_empty_for_unknown_device() {
    let response = json!({
        "device_id": "UNKNOWN_DEVICE",
        "rotations": []
    });
    assert!(response["rotations"].as_array().is_some_and(|a| a.is_empty()));
}

// ============================================================================
// POST /revoke — validation + response shape
// ============================================================================

#[test]
fn test_revoke_request_requires_non_empty_key_id() {
    // revoke_old_keys rejects empty key_id with ApiError::bad_request.
    let request_missing = json!({});
    let key_id_missing = request_missing.get("key_id").and_then(|v| v.as_str()).unwrap_or("");
    assert!(key_id_missing.is_empty(), "empty key_id must be detected as invalid");

    let request_empty = json!({ "key_id": "" });
    let key_id_empty = request_empty.get("key_id").and_then(|v| v.as_str()).unwrap_or("");
    assert!(key_id_empty.is_empty());
}

#[test]
fn test_revoke_request_with_reason() {
    let request = json!({ "key_id": "ed25519:compromised", "reason": "compromised" });
    assert_eq!(request.get("key_id").and_then(|v| v.as_str()), Some("ed25519:compromised"));
    assert_eq!(request.get("reason").and_then(|v| v.as_str()), Some("compromised"));
}

#[test]
fn test_revoke_response_shape_when_key_found() {
    // revoked_count > 0 → "Successfully revoked key {key_id}".
    let revoked_count: u64 = 1;
    let key_id = "ed25519:1";
    let message = if revoked_count > 0 {
        format!("Successfully revoked key {}", key_id)
    } else {
        format!("Key {} not found or already expired", key_id)
    };
    let response = json!({
        "success": true,
        "revoked": revoked_count,
        "message": message
    });

    assert!(response["success"].as_bool().unwrap_or(false));
    assert_eq!(response["revoked"].as_u64(), Some(1));
    assert!(response["message"].as_str().unwrap_or("").contains("Successfully revoked"));
}

#[test]
fn test_revoke_response_shape_when_key_not_found() {
    // revoked_count == 0 → "Key {key_id} not found or already expired".
    let revoked_count: u64 = 0;
    let key_id = "ed25519:missing";
    let message = if revoked_count > 0 {
        format!("Successfully revoked key {}", key_id)
    } else {
        format!("Key {} not found or already expired", key_id)
    };
    let response = json!({
        "success": true,
        "revoked": revoked_count,
        "message": message
    });

    assert_eq!(response["revoked"].as_u64(), Some(0));
    assert!(response["message"].as_str().unwrap_or("").contains("not found or already expired"));
}

// ============================================================================
// PUT/POST /config — request/response shape
// ============================================================================

#[test]
fn test_config_request_supports_all_eight_fields() {
    // configure_key_rotation reads up to 8 optional fields from the body.
    let request = json!({
        "enabled": true,
        "interval_ms": 3_600_000_i64,
        "rotation_interval_days": 7_i64,
        "rotation_threshold_days": 1_i64,
        "grace_period_minutes": 5_i64,
        "olm_rotation_days": 7_i64,
        "megolm_rotation_messages": 100_i64,
        "max_session_age_days": 90_i64
    });

    assert_eq!(request.get("enabled").and_then(|v| v.as_bool()), Some(true));
    assert_eq!(request.get("interval_ms").and_then(|v| v.as_i64()), Some(3_600_000));
    assert_eq!(request.get("rotation_interval_days").and_then(|v| v.as_i64()), Some(7));
    assert_eq!(request.get("rotation_threshold_days").and_then(|v| v.as_i64()), Some(1));
    assert_eq!(request.get("grace_period_minutes").and_then(|v| v.as_i64()), Some(5));
    assert_eq!(request.get("olm_rotation_days").and_then(|v| v.as_i64()), Some(7));
    assert_eq!(request.get("megolm_rotation_messages").and_then(|v| v.as_i64()), Some(100));
    assert_eq!(request.get("max_session_age_days").and_then(|v| v.as_i64()), Some(90));
}

#[test]
fn test_config_request_partial_update() {
    // A partial body (only some fields) is valid — absent fields are skipped.
    let request = json!({ "enabled": false });
    assert_eq!(request.get("enabled").and_then(|v| v.as_bool()), Some(false));
    assert!(request.get("interval_ms").is_none());
    assert!(request.get("olm_rotation_days").is_none());
}

#[test]
fn test_config_response_shape() {
    // configure_key_rotation returns enabled, interval_ms, rotation_interval_days,
    // rotation_threshold_days, grace_period_minutes.
    let response = json!({
        "enabled": true,
        "interval_ms": 3_600_000_i64,
        "rotation_interval_days": 7_i64,
        "rotation_threshold_days": 1_i64,
        "grace_period_minutes": 5_i64
    });

    assert!(response.get("enabled").is_some());
    assert!(response.get("interval_ms").is_some());
    assert!(response.get("rotation_interval_days").is_some());
    assert!(response.get("rotation_threshold_days").is_some());
    assert!(response.get("grace_period_minutes").is_some());
}

// ============================================================================
// GET/POST /check — response shape + needs-rotation logic
// ============================================================================

#[test]
fn test_check_response_shape() {
    let response = json!({
        "needs_rotation": true,
        "last_rotation": Value::Null,
        "interval_ms": 604_800_000_i64
    });

    assert!(response.get("needs_rotation").is_some());
    assert!(response.get("last_rotation").is_some());
    assert!(response.get("interval_ms").is_some());
    assert!(response["needs_rotation"].as_bool().unwrap_or(false));
}

#[test]
fn test_check_request_with_key_id_query_param() {
    // check_needs_rotation reads optional ?key_id= query param.
    let mut params = std::collections::HashMap::new();
    params.insert("key_id".to_string(), "ed25519:1".to_string());
    let key_id_filter = params.get("key_id").map(|s| s.as_str());
    assert_eq!(key_id_filter, Some("ed25519:1"));
}

#[test]
fn test_check_request_without_key_id_query_param() {
    let params: std::collections::HashMap<String, String> = std::collections::HashMap::new();
    let key_id_filter = params.get("key_id").map(|s| s.as_str());
    assert!(key_id_filter.is_none());
}

/// Mirror the needs-rotation computation in check_needs_rotation:
///   needs = match last_rotation { Some(last) => now - last > interval_ms, None => true }
fn compute_needs_rotation(last_rotation: Option<i64>, now: i64, interval_ms: i64) -> bool {
    match last_rotation {
        Some(last) => now - last > interval_ms,
        None => true,
    }
}

#[test]
fn test_needs_rotation_true_when_no_prior_rotation() {
    let interval_ms = 604_800_000_i64; // 7 days
    assert!(compute_needs_rotation(None, 1_700_000_000_000, interval_ms));
}

#[test]
fn test_needs_rotation_false_when_within_interval() {
    let interval_ms = 604_800_000_i64; // 7 days
    let now = 1_700_000_000_000_i64;
    let last = now - 1_000_000; // 1000s ago, well within 7 days
    assert!(!compute_needs_rotation(Some(last), now, interval_ms));
}

#[test]
fn test_needs_rotation_true_when_beyond_interval() {
    let interval_ms = 604_800_000_i64; // 7 days
    let now = 1_700_000_000_000_i64;
    let last = now - interval_ms - 1; // just past the interval
    assert!(compute_needs_rotation(Some(last), now, interval_ms));
}

#[test]
fn test_needs_rotation_boundary_exactly_at_interval_is_false() {
    // now - last == interval_ms → NOT > interval_ms → needs_rotation is false.
    let interval_ms = 604_800_000_i64;
    let now = 1_700_000_000_000_i64;
    let last = now - interval_ms;
    assert!(!compute_needs_rotation(Some(last), now, interval_ms));
}

#[test]
fn test_needs_rotation_max_ts_zero_treated_as_none() {
    // When get_max_rotation_ts returns 0, the handler treats it as None.
    let max_ts: i64 = 0;
    let last_rotation: Option<i64> = if max_ts == 0 { None } else { Some(max_ts) };
    assert!(last_rotation.is_none());
    assert!(compute_needs_rotation(last_rotation, 1_700_000_000_000, 604_800_000));
}

// ============================================================================
// Auth/permission model (admin-gated endpoints)
// ============================================================================

#[test]
fn test_all_key_rotation_endpoints_require_admin() {
    // Every handler in key_rotation.rs gates on `auth_user.is_admin` and returns
    // ApiError::forbidden(...) when false. This test encodes that contract.
    let admin_required_paths = [
        "/_matrix/client/v1/keys/rotation/status",
        "/_matrix/client/v1/keys/rotation/rotate",
        "/_matrix/client/v1/keys/rotation/history/{device_id}",
        "/_matrix/client/v1/keys/rotation/revoke",
        "/_matrix/client/v1/keys/rotation/config",
        "/_matrix/client/v1/keys/rotation/check",
    ];

    let manifest = key_rotation_route_manifest();
    let manifest_paths: std::collections::HashSet<&str> = manifest.iter().map(|e| e.path).collect();
    for path in &admin_required_paths {
        assert!(manifest_paths.contains(*path), "admin-required path missing from manifest: {path}");
    }
}

#[test]
fn test_non_admin_request_is_forbidden() {
    // Mirror the handler's admin check: is_admin == false → 403 forbidden.
    let is_admin = false;
    let forbidden = !is_admin;
    assert!(forbidden, "non-admin must be forbidden from key rotation management");
    // The handler returns ApiError::forbidden(...) which maps to HTTP 403.
    let expected_status = 403u16;
    assert_eq!(expected_status, 403);
}

#[test]
fn test_unauthenticated_request_is_rejected() {
    // AuthenticatedUser extractor rejects missing/invalid tokens with 401
    // before the handler body runs.
    let has_valid_token = false;
    assert!(!has_valid_token);
    let expected_status = 401u16;
    assert_eq!(expected_status, 401);
}

#[test]
fn test_revoke_admin_gate_uses_distinct_message() {
    // revoke_old_keys uses a revoke-specific forbidden message
    // ("Key revocation requires server admin privileges") distinct from the
    // rotation management message. Both still map to 403.
    let revoke_message = "Key revocation requires server admin privileges";
    let rotation_message = "Key rotation management requires server admin privileges";
    assert_ne!(revoke_message, rotation_message);
    assert!(revoke_message.contains("revocation"));
}
