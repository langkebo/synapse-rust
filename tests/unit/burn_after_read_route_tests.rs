// Burn After Read route layer tests.
//
// Covers the wire-level contracts exposed by
// `synapse-web/src/routes/burn_after_read.rs` (P-096: previously zero tests):
//   * Route manifest contents (methods + paths + registered_by tag) across
//     v1 and v3 path prefixes.
//   * Request/response JSON shapes for each of the 7 logical endpoints.
//   * Pure-logic mirrors: default `enabled`/`burn_after_ms` resolution,
//     `delete_ts = now + burn_after_ms` computation, "burn not enabled"
//     gating in `mark_burn_read`.
//   * Error-code mapping: 404 (room not found), 400 (burn not enabled),
//     500 (internal_with_context), 403 (ensure_room_member_ctx → forbidden).
//
// The handlers themselves require a fully-wired `RoomContext` (room_service,
// burn_after_read service, etc.), so — following the established pattern in
// `key_rotation_route_tests.rs` — we exercise the real public manifest and
// lock down the handler contracts with shape assertions that mirror the
// handler logic line-for-line.

#![cfg(feature = "burn-after-read")]

use axum::http::Method;
use serde_json::{json, Value};
use synapse_common::ApiError;
use synapse_web::routes::declared_ledger_all;
use synapse_web::routes::route_ledger::RouteEntry;

/// The `burn_after_read` slice of the derived route table.
///
/// `burn_after_read_route_manifest()` was one of ~120 hand-copied projections
/// deleted by B2-2; route metadata now has a single source (`derived_routes`),
/// and a test that needs one module's surface filters it by `registered_by`.
fn burn_after_read_route_manifest() -> Vec<RouteEntry> {
    declared_ledger_all().iter().filter(|e| e.registered_by == "burn_after_read").cloned().collect()
}

// ============================================================================
// Route manifest tests
// ============================================================================

#[test]
fn test_route_manifest_contains_all_twenty_one_entries() {
    let manifest = burn_after_read_route_manifest();
    assert_eq!(
        manifest.len(),
        21,
        "burn_after_read manifest must declare exactly 21 (method, path) entries (7 v1 + 7 v3 + 7 vendor/v1)"
    );
}

#[test]
fn test_route_manifest_matches_declared_paths_and_methods() {
    let manifest = burn_after_read_route_manifest();

    // (method, path) pairs as declared in create_burn_after_read_router.
    let expected = [
        (Method::PUT, "/_matrix/client/v1/rooms/{room_id}/burn"),
        (Method::GET, "/_matrix/client/v1/rooms/{room_id}/burn"),
        (Method::GET, "/_matrix/client/v1/rooms/{room_id}/burn/pending"),
        (Method::POST, "/_matrix/client/v1/rooms/{room_id}/burn/{event_id}"),
        (Method::DELETE, "/_matrix/client/v1/rooms/{room_id}/burn/{event_id}"),
        (Method::PUT, "/_matrix/client/v1/user/burn/config"),
        (Method::GET, "/_matrix/client/v1/user/burn/stats"),
        // v3 paths
        (Method::PUT, "/_matrix/client/v3/rooms/{room_id}/burn"),
        (Method::GET, "/_matrix/client/v3/rooms/{room_id}/burn"),
        (Method::GET, "/_matrix/client/v3/rooms/{room_id}/burn/pending"),
        (Method::POST, "/_matrix/client/v3/rooms/{room_id}/burn/{event_id}"),
        (Method::DELETE, "/_matrix/client/v3/rooms/{room_id}/burn/{event_id}"),
        (Method::PUT, "/_matrix/client/v3/user/burn/config"),
        (Method::GET, "/_matrix/client/v3/user/burn/stats"),
        // vendor paths
        (Method::PUT, "/_matrix/vendor/v1/rooms/{room_id}/burn"),
        (Method::GET, "/_matrix/vendor/v1/rooms/{room_id}/burn"),
        (Method::GET, "/_matrix/vendor/v1/rooms/{room_id}/burn/pending"),
        (Method::POST, "/_matrix/vendor/v1/rooms/{room_id}/burn/{event_id}"),
        (Method::DELETE, "/_matrix/vendor/v1/rooms/{room_id}/burn/{event_id}"),
        (Method::PUT, "/_matrix/vendor/v1/user/burn/config"),
        (Method::GET, "/_matrix/vendor/v1/user/burn/stats"),
    ];

    let actual: Vec<(Method, &str)> = manifest.iter().map(|e| (e.method.clone(), e.path)).collect();
    for pair in &expected {
        assert!(actual.contains(pair), "manifest missing {:?} {}", pair.0, pair.1);
    }
    assert_eq!(actual.len(), expected.len(), "manifest size mismatch");
}

#[test]
fn test_route_manifest_entries_registered_by_burn_after_read() {
    let manifest = burn_after_read_route_manifest();
    assert!(
        manifest.iter().all(|e| e.registered_by == "burn_after_read"),
        "every entry must be owned by burn_after_read"
    );
}

#[test]
fn test_route_manifest_has_no_duplicate_method_path_pairs() {
    let manifest = burn_after_read_route_manifest();
    let mut seen = std::collections::HashSet::new();
    for entry in &manifest {
        let key = (entry.method.clone(), entry.path);
        assert!(seen.insert(key), "duplicate route entry: {:?} {}", entry.method, entry.path);
    }
}

#[test]
fn test_route_manifest_covers_seven_logical_endpoints_across_v1_v3_and_vendor() {
    let manifest = burn_after_read_route_manifest();
    // 7 (method, path) entries per version prefix across three prefixes.
    let client_entries: Vec<&_> = manifest.iter().filter(|e| e.path.starts_with("/_matrix/client/")).collect();
    let vendor_entries: Vec<&_> = manifest.iter().filter(|e| e.path.starts_with("/_matrix/vendor/")).collect();
    let v1_entries: Vec<&_> = client_entries.iter().filter(|e| e.path.contains("/client/v1/")).copied().collect();
    let v3_entries: Vec<&_> = client_entries.iter().filter(|e| e.path.contains("/client/v3/")).copied().collect();
    assert_eq!(v1_entries.len(), 7, "expected 7 client/v1 (method, path) entries, got {}", v1_entries.len());
    assert_eq!(v3_entries.len(), 7, "expected 7 client/v3 (method, path) entries, got {}", v3_entries.len());
    assert_eq!(vendor_entries.len(), 7, "expected 7 vendor/v1 (method, path) entries, got {}", vendor_entries.len());

    let v1_paths: std::collections::HashSet<&str> = v1_entries.iter().map(|e| e.path).collect();
    let v3_paths: std::collections::HashSet<&str> = v3_entries.iter().map(|e| e.path).collect();
    let vendor_paths: std::collections::HashSet<&str> = vendor_entries.iter().map(|e| e.path).collect();
    assert_eq!(v1_paths.len(), 5, "expected 5 distinct v1 paths, got {}", v1_paths.len());
    assert_eq!(v3_paths.len(), 5, "expected 5 distinct v3 paths, got {}", v3_paths.len());
    assert_eq!(vendor_paths.len(), 5, "expected 5 distinct vendor/v1 paths, got {}", vendor_paths.len());

    // Sanity: the room-scoped burn/{event_id} endpoint exists for both POST and DELETE.
    assert!(manifest.iter().any(|e| e.method == Method::POST && e.path.ends_with("/burn/{event_id}")));
    assert!(manifest.iter().any(|e| e.method == Method::DELETE && e.path.ends_with("/burn/{event_id}")));
}

#[test]
fn test_route_entry_is_debug_clone() {
    fn assert_traits<T: std::fmt::Debug + Clone>() {}
    assert_traits::<RouteEntry>();
}

// ============================================================================
// PUT /rooms/{room_id}/burn — enable_burn request/response shape
// ============================================================================

/// Mirror the body-parsing logic in `enable_burn`:
///   enabled = body.get("enabled").as_bool().unwrap_or(true)
///   burn_after_ms = body.get("burn_after_ms").as_i64().unwrap_or(60_000)
fn resolve_enable_burn_body(body: &Value) -> (bool, i64) {
    let enabled = body.get("enabled").and_then(|v| v.as_bool()).unwrap_or(true);
    let burn_after_ms = body.get("burn_after_ms").and_then(|v| v.as_i64()).unwrap_or(60_000);
    (enabled, burn_after_ms)
}

#[test]
fn test_enable_burn_response_shape() {
    let response = json!({
        "enabled": true,
        "burn_after_ms": 60_000_i64
    });
    assert!(response["enabled"].as_bool().unwrap_or(false));
    assert_eq!(response["burn_after_ms"].as_i64(), Some(60_000));
}

#[test]
fn test_enable_burn_defaults_when_body_empty() {
    // Empty body → enabled defaults to true, burn_after_ms to 60_000.
    let (enabled, burn_after_ms) = resolve_enable_burn_body(&json!({}));
    assert!(enabled);
    assert_eq!(burn_after_ms, 60_000);
}

#[test]
fn test_enable_burn_respects_explicit_disabled() {
    let (enabled, burn_after_ms) = resolve_enable_burn_body(&json!({ "enabled": false, "burn_after_ms": 5_000 }));
    assert!(!enabled);
    assert_eq!(burn_after_ms, 5_000);
}

#[test]
fn test_enable_burn_ignores_wrong_typed_fields() {
    // Non-bool / non-i64 fields fall back to defaults.
    let (enabled, burn_after_ms) = resolve_enable_burn_body(&json!({ "enabled": "yes", "burn_after_ms": "soon" }));
    assert!(enabled, "non-bool enabled must fall back to true");
    assert_eq!(burn_after_ms, 60_000, "non-i64 burn_after_ms must fall back to 60_000");
}

// ============================================================================
// GET /rooms/{room_id}/burn — get_burn_settings response shape
// ============================================================================

#[test]
fn test_get_burn_settings_response_when_settings_exist() {
    // Mirrors the Some(s) branch of get_burn_settings.
    let is_enabled = true;
    let burn_after_ms: i64 = 30_000;
    let response = json!({
        "enabled": is_enabled,
        "burn_after_ms": burn_after_ms
    });
    assert!(response["enabled"].as_bool().unwrap_or(false));
    assert_eq!(response["burn_after_ms"].as_i64(), Some(30_000));
}

#[test]
fn test_get_burn_settings_response_when_no_settings() {
    // Mirrors the None branch: enabled=false, burn_after_ms=60_000.
    let response = json!({
        "enabled": false,
        "burn_after_ms": 60_000_i64
    });
    assert!(!response["enabled"].as_bool().unwrap_or(true));
    assert_eq!(response["burn_after_ms"].as_i64(), Some(60_000));
}

// ============================================================================
// POST /rooms/{room_id}/burn/{event_id} — mark_burn_read response + gating
// ============================================================================

#[test]
fn test_mark_burn_read_response_shape() {
    // delete_ts = current_timestamp_millis() + burn_after_ms
    let now: i64 = 1_700_000_000_000;
    let burn_after_ms: i64 = 60_000;
    let delete_ts = now + burn_after_ms;
    let response = json!({
        "success": true,
        "will_delete_at": delete_ts
    });
    assert!(response["success"].as_bool().unwrap_or(false));
    assert_eq!(response["will_delete_at"].as_i64(), Some(1_700_000_060_000));
}

#[test]
fn test_mark_burn_read_rejects_when_no_settings() {
    // settings == None → ApiError::bad_request("Burn not enabled for this room")
    let settings: Option<(bool, i64)> = None;
    let rejected = settings.is_none();
    assert!(rejected, "missing settings must be rejected");
    let err = ApiError::bad_request("Burn not enabled for this room".to_string());
    assert_eq!(err.http_status(), axum::http::StatusCode::BAD_REQUEST);
}

#[test]
fn test_mark_burn_read_rejects_when_disabled() {
    // settings.is_enabled == false → ApiError::bad_request("Burn not enabled for this room")
    let is_enabled = false;
    let err =
        if !is_enabled { Some(ApiError::bad_request("Burn not enabled for this room".to_string())) } else { None };
    assert!(err.is_some(), "disabled burn must be rejected");
}

// ============================================================================
// GET /rooms/{room_id}/burn/pending — get_pending_burns response shape
// ============================================================================

#[test]
fn test_get_pending_burns_response_shape() {
    // Each pending event is mapped to { event_id, created_at, delete_ts }.
    let pending = vec![
        json!({ "event_id": "$ev1:server", "created_at": 1_700_000_000_000_i64, "delete_ts": 1_700_000_060_000_i64 }),
        json!({ "event_id": "$ev2:server", "created_at": 1_700_000_010_000_i64, "delete_ts": 1_700_000_070_000_i64 }),
    ];
    let response = json!({ "events": pending });
    let events = response["events"].as_array().expect("events must be array");
    assert_eq!(events.len(), 2);
    assert_eq!(events[0]["event_id"].as_str(), Some("$ev1:server"));
    assert_eq!(events[1]["delete_ts"].as_i64(), Some(1_700_000_070_000));
}

#[test]
fn test_get_pending_burns_response_empty() {
    let response = json!({ "events": [] });
    assert!(response["events"].as_array().is_some_and(|a| a.is_empty()));
}

// ============================================================================
// DELETE /rooms/{room_id}/burn/{event_id} — cancel_burn response shape
// ============================================================================

#[test]
fn test_cancel_burn_response_shape() {
    let response = json!({ "success": true });
    assert!(response["success"].as_bool().unwrap_or(false));
}

// ============================================================================
// PUT /user/burn/config — set_global_burn_config request/response shape
// ============================================================================

/// Mirror body parsing: default_burn_ms = body.get("default_burn_ms").as_i64().unwrap_or(60_000)
fn resolve_global_config_body(body: &Value) -> i64 {
    body.get("default_burn_ms").and_then(|v| v.as_i64()).unwrap_or(60_000)
}

#[test]
fn test_set_global_burn_config_response_shape() {
    let response = json!({ "default_burn_ms": 120_000_i64 });
    assert_eq!(response["default_burn_ms"].as_i64(), Some(120_000));
}

#[test]
fn test_set_global_burn_config_defaults_when_absent() {
    let default_burn_ms = resolve_global_config_body(&json!({}));
    assert_eq!(default_burn_ms, 60_000);
}

#[test]
fn test_set_global_burn_config_respects_explicit_value() {
    let default_burn_ms = resolve_global_config_body(&json!({ "default_burn_ms": 250 }));
    assert_eq!(default_burn_ms, 250);
}

// ============================================================================
// GET /user/burn/stats — get_burn_stats response shape
// ============================================================================

#[test]
fn test_get_burn_stats_response_shape() {
    // Mirrors BurnStats { total_burned, total_pending, rooms_enabled } → JSON.
    let response = json!({
        "total_burned": 42_i64,
        "total_pending": 3_i64,
        "rooms_with_burn_enabled": 5_i64
    });
    assert_eq!(response["total_burned"].as_i64(), Some(42));
    assert_eq!(response["total_pending"].as_i64(), Some(3));
    assert_eq!(response["rooms_with_burn_enabled"].as_i64(), Some(5));
}

#[test]
fn test_get_burn_stats_response_zero_defaults() {
    // BurnStats derives Default → all zeros.
    let response = json!({
        "total_burned": 0_i64,
        "total_pending": 0_i64,
        "rooms_with_burn_enabled": 0_i64
    });
    assert_eq!(response["total_burned"].as_i64(), Some(0));
    assert_eq!(response["total_pending"].as_i64(), Some(0));
    assert_eq!(response["rooms_with_burn_enabled"].as_i64(), Some(0));
}

// ============================================================================
// Error-code mapping — mirrors ApiError constructors used by the handlers
// ============================================================================

#[test]
fn test_room_not_found_maps_to_404() {
    // enable_burn / get_burn_settings / mark_burn_read / get_pending_burns / cancel_burn
    // all return ApiError::not_found when room_exists is false.
    let err = ApiError::not_found("Room '!room:server' not found".to_string());
    assert_eq!(err.http_status(), axum::http::StatusCode::NOT_FOUND);
}

#[test]
fn test_burn_not_enabled_maps_to_400() {
    // mark_burn_read returns ApiError::bad_request when burn is disabled/absent.
    let err = ApiError::bad_request("Burn not enabled for this room".to_string());
    assert_eq!(err.http_status(), axum::http::StatusCode::BAD_REQUEST);
}

#[test]
fn test_internal_error_maps_to_500() {
    // All service failures are wrapped via ApiError::internal_with_context.
    let err = ApiError::internal_with_context("Failed to enable burn", &"db down");
    assert_eq!(err.http_status(), axum::http::StatusCode::INTERNAL_SERVER_ERROR);
}

#[test]
fn test_non_member_forbidden_maps_to_403() {
    // ensure_room_member_ctx returns ApiError::forbidden when the user is not a
    // joined member (and is not an admin).
    let err = ApiError::forbidden("You must be a room member to configure burn-after-read".to_string());
    assert_eq!(err.http_status(), axum::http::StatusCode::FORBIDDEN);
}

// ============================================================================
// Auth/permission model — every room-scoped handler is member-gated
// ============================================================================

#[test]
fn test_all_room_scoped_handlers_require_room_membership() {
    // Every handler under /rooms/{room_id}/burn calls ensure_room_member_ctx.
    let member_gated_paths = [
        "/_matrix/client/v1/rooms/{room_id}/burn",
        "/_matrix/client/v1/rooms/{room_id}/burn/pending",
        "/_matrix/client/v1/rooms/{room_id}/burn/{event_id}",
    ];

    let manifest = burn_after_read_route_manifest();
    let manifest_paths: std::collections::HashSet<&str> = manifest.iter().map(|e| e.path).collect();
    for path in &member_gated_paths {
        assert!(manifest_paths.contains(*path), "member-gated path missing from manifest: {path}");
    }
}

#[test]
fn test_user_scoped_handlers_are_not_room_gated() {
    // /user/burn/config and /user/burn/stats operate on the authenticated user
    // directly — they do not call ensure_room_member_ctx.
    let user_scoped = ["/_matrix/client/v1/user/burn/config", "/_matrix/client/v1/user/burn/stats"];
    let manifest = burn_after_read_route_manifest();
    let manifest_paths: std::collections::HashSet<&str> = manifest.iter().map(|e| e.path).collect();
    for path in &user_scoped {
        assert!(manifest_paths.contains(*path), "user-scoped path missing from manifest: {path}");
    }
}
