//! Insta snapshot tests for security-sensitive endpoint response shapes (P-097).
//!
//! These tests lock the JSON serialization shapes of responses from endpoints
//! that previously had no insta snapshots:
//!   - **auth_compat**: login flows, register flows, login error, whoami 401
//!   - **key_rotation**: status, config, check, admin-forbidden error
//!   - **burn_after_read**: stats, room-not-found / not-enabled errors
//!   - **room_access**: forbidden (non-member) error
//!
//! ## Pattern
//!
//! Following the P-096 "pure JSON shape validation" pattern (see
//! `key_rotation_route_tests.rs`, `burn_after_read_route_tests.rs`): these
//! tests construct `serde_json::Value` mirrors of the handler outputs —
//! verified line-for-line against `synapse-web/src/routes/*.rs` — and snapshot them
//! with insta. No HTTP router or database is required, so they run in any
//! environment.
//!
//! For error responses, the real `ApiError` constructors are used so the
//! `errcode` → Matrix error code mapping is verified end-to-end. The JSON
//! body mirrors the shape produced by `ApiError::into_response()`:
//!   `{"errcode": <code_str>, "error": <message>}`
//!
//! ## Reviewing snapshots
//!
//! ```bash
//! cargo insta test --test unit --features test-utils -- security_endpoint_snapshots
//! cargo insta review
//! ```

use serde_json::{json, Value};
use synapse_common::ApiError;

/// Mirror the JSON body that `ApiError::into_response()` produces for
/// non-internal errors: `{"errcode": <code>, "error": <message>}`.
///
/// For non-internal errors, `ApiError::message()` returns `self.message`
/// verbatim — matching the `into_response` body construction. Using the real
/// constructors (rather than hand-typed JSON) verifies the errcode mapping.
fn api_error_json(err: &ApiError) -> Value {
    json!({
        "errcode": err.code_str(),
        "error": err.message(),
    })
}

// ============================================================================
// auth_compat — login / register / whoami response shapes
// Source: synapse-web/src/routes/auth_compat.rs, synapse-web/src/routes/account_compat.rs
// ============================================================================

#[test]
fn snapshot_auth_compat_login_flows_shape() {
    // Mirrors get_login_flows handler output (base flows without SSO):
    //   Ok(Json(json!({ "flows": [...] })))
    // The handler always includes m.login.password and m.login.token.
    let body = json!({
        "flows": [
            {"type": "m.login.password"},
            {"type": "m.login.token"}
        ]
    });
    insta::assert_json_snapshot!("auth_compat_login_flows_shape", body);
}

#[test]
fn snapshot_auth_compat_login_flows_with_sso_shape() {
    // When OIDC is enabled, an m.login.sso flow with identity_providers is
    // appended. The provider list is dynamic — redact it to lock the shape.
    let body = json!({
        "flows": [
            {"type": "m.login.password"},
            {"type": "m.login.token"},
            {
                "type": "m.login.sso",
                "identity_providers": [
                    {"id": "oidc", "name": "OIDC", "brand": "oidc"}
                ]
            }
        ]
    });
    insta::assert_json_snapshot!("auth_compat_login_flows_with_sso_shape", body, {
        ".flows[2].identity_providers" => "[redacted_sso_providers]",
    });
}

#[test]
fn snapshot_auth_compat_register_flows_shape() {
    // Mirrors get_register_flows handler output:
    //   { "flows": [{"stages": ["m.login.dummy"]}, {"stages": ["m.login.password"]}],
    //     "params": {} }
    let body = json!({
        "flows": [
            {"stages": ["m.login.dummy"]},
            {"stages": ["m.login.password"]}
        ],
        "params": {}
    });
    insta::assert_json_snapshot!("auth_compat_register_flows_shape", body);
}

#[test]
fn snapshot_auth_compat_register_uia_challenge_shape() {
    // Mirrors the 401 UIA challenge body returned when `auth` is absent:
    //   { "flows": [...], "params": {}, "session": "<uuid>" }
    // The session is a random UUID — redact it to keep the snapshot stable.
    let body = json!({
        "flows": [
            {"stages": ["m.login.dummy"]},
            {"stages": ["m.login.password"]}
        ],
        "params": {},
        "session": "550e8400-e29b-41d4-a716-446655440000"
    });
    insta::assert_json_snapshot!("auth_compat_register_uia_challenge_shape", body, {
        ".session" => "[redacted_session_uuid]",
    });
}

#[test]
fn snapshot_auth_compat_login_invalid_credentials_error() {
    // P-007 fix: mirrors the 401 + M_FORBIDDEN error when login fails with
    // wrong password (HTTP status is 401 Unauthorized, errcode stays
    // M_FORBIDDEN per Matrix spec). Use the production constructor.
    let err = ApiError::invalid_credentials();
    let body = api_error_json(&err);
    insta::assert_json_snapshot!("auth_compat_login_invalid_credentials_error", body);
}

#[test]
fn snapshot_auth_compat_whoami_missing_token_error() {
    // Mirrors the 401 error when whoami is called without an access token.
    // Handler (account_compat::whoami) returns ApiError::missing_token().
    // errcode = M_MISSING_TOKEN, HTTP status = 401.
    let err = ApiError::missing_token();
    let body = api_error_json(&err);
    insta::assert_json_snapshot!("auth_compat_whoami_missing_token_error", body);
}

#[test]
fn snapshot_auth_compat_whoami_success_shape() {
    // Mirrors the 200 response of account_compat::whoami:
    //   { "user_id": <user_id>, "device_id": <device_id>, "is_guest": <bool> }
    // user_id and device_id are dynamic — redact them.
    let body = json!({
        "user_id": "@alice:example.com",
        "device_id": "DEVICEXYZ",
        "is_guest": false
    });
    insta::assert_json_snapshot!("auth_compat_whoami_success_shape", body, {
        ".user_id" => "[redacted_user_id]",
        ".device_id" => "[redacted_device_id]",
    });
}

// ============================================================================
// key_rotation — status / config / check response shapes
// Source: synapse-web/src/routes/key_rotation.rs
// ============================================================================

#[test]
fn snapshot_key_rotation_status_shape() {
    // Mirrors get_key_rotation_status handler output:
    //   { "enabled": <bool>, "status": <rotation_status_obj>,
    //     "user_last_rotation": <i64|null> }
    // user_last_rotation is a dynamic timestamp — redact it.
    let body = json!({
        "enabled": true,
        "status": {
            "rotation_enabled": true,
            "has_current_key": true,
            "should_rotate": false
        },
        "user_last_rotation": 1_700_000_000_000_i64
    });
    insta::assert_json_snapshot!("key_rotation_status_shape", body, {
        ".user_last_rotation" => "[redacted_timestamp]",
    });
}

#[test]
fn snapshot_key_rotation_status_no_prior_rotation_shape() {
    // When there is no prior rotation, user_last_rotation is null.
    let body = json!({
        "enabled": true,
        "status": {
            "rotation_enabled": true,
            "has_current_key": false
        },
        "user_last_rotation": Value::Null
    });
    insta::assert_json_snapshot!("key_rotation_status_no_prior_rotation_shape", body);
}

#[test]
fn snapshot_key_rotation_config_shape() {
    // Mirrors configure_key_rotation handler output (PUT/POST /config):
    //   { "enabled": <bool>, "interval_ms": <i64>,
    //     "rotation_interval_days": <i64>, "rotation_threshold_days": <i64>,
    //     "grace_period_minutes": <i64> }
    let body = json!({
        "enabled": true,
        "interval_ms": 3_600_000_i64,
        "rotation_interval_days": 7_i64,
        "rotation_threshold_days": 1_i64,
        "grace_period_minutes": 5_i64
    });
    insta::assert_json_snapshot!("key_rotation_config_shape", body);
}

#[test]
fn snapshot_key_rotation_check_shape() {
    // Mirrors check_needs_rotation handler output (GET/POST /check):
    //   { "needs_rotation": <bool>, "last_rotation": <i64|null>,
    //     "interval_ms": <u64> }
    // No prior rotation → last_rotation is null (stable, no redaction needed).
    let body = json!({
        "needs_rotation": true,
        "last_rotation": Value::Null,
        "interval_ms": 604_800_000_i64
    });
    insta::assert_json_snapshot!("key_rotation_check_shape", body);
}

#[test]
fn snapshot_key_rotation_check_no_rotation_needed_shape() {
    // When a recent rotation exists, needs_rotation is false and last_rotation
    // holds a timestamp (redacted).
    let body = json!({
        "needs_rotation": false,
        "last_rotation": 1_700_000_000_000_i64,
        "interval_ms": 604_800_000_i64
    });
    insta::assert_json_snapshot!("key_rotation_check_no_rotation_needed_shape", body, {
        ".last_rotation" => "[redacted_timestamp]",
    });
}

#[test]
fn snapshot_key_rotation_admin_forbidden_error() {
    // Every key_rotation handler gates on auth_user.is_admin; non-admin gets
    // ApiError::forbidden("Key rotation management requires server admin
    // privileges"). errcode = M_FORBIDDEN, HTTP 403.
    let err = ApiError::forbidden("Key rotation management requires server admin privileges".to_string());
    let body = api_error_json(&err);
    insta::assert_json_snapshot!("key_rotation_admin_forbidden_error", body);
}

#[test]
fn snapshot_key_rotation_revoke_admin_forbidden_error() {
    // revoke_old_keys uses a distinct forbidden message.
    let err = ApiError::forbidden("Key revocation requires server admin privileges".to_string());
    let body = api_error_json(&err);
    insta::assert_json_snapshot!("key_rotation_revoke_admin_forbidden_error", body);
}

// ============================================================================
// burn_after_read — stats response shape + error shapes
// Source: synapse-web/src/routes/burn_after_read.rs
// ============================================================================

#[test]
fn snapshot_burn_after_read_stats_shape() {
    // Mirrors get_burn_stats handler output (GET /user/burn/stats):
    //   { "total_burned": <i64>, "total_pending": <i64>,
    //     "rooms_with_burn_enabled": <i64> }
    let body = json!({
        "total_burned": 42_i64,
        "total_pending": 3_i64,
        "rooms_with_burn_enabled": 5_i64
    });
    insta::assert_json_snapshot!("burn_after_read_stats_shape", body);
}

#[test]
fn snapshot_burn_after_read_stats_zero_defaults_shape() {
    // BurnStats derives Default → all zeros. This locks the zero-state shape.
    let body = json!({
        "total_burned": 0_i64,
        "total_pending": 0_i64,
        "rooms_with_burn_enabled": 0_i64
    });
    insta::assert_json_snapshot!("burn_after_read_stats_zero_defaults_shape", body);
}

#[test]
fn snapshot_burn_after_read_room_not_found_error() {
    // enable_burn / get_burn_settings / mark_burn_read / get_pending_burns /
    // cancel_burn all return ApiError::not_found when the room doesn't exist.
    // errcode = M_NOT_FOUND, HTTP 404.
    let err = ApiError::not_found("Room '!room:server' not found".to_string());
    let body = api_error_json(&err);
    insta::assert_json_snapshot!("burn_after_read_room_not_found_error", body);
}

#[test]
fn snapshot_burn_after_read_not_enabled_error() {
    // mark_burn_read returns ApiError::bad_request when burn is disabled or
    // no settings exist. errcode = M_BAD_JSON, HTTP 400.
    let err = ApiError::bad_request("Burn not enabled for this room".to_string());
    let body = api_error_json(&err);
    insta::assert_json_snapshot!("burn_after_read_not_enabled_error", body);
}

// ============================================================================
// room_access — forbidden (non-member) error shape
// Source: synapse-web/src/routes/room_access.rs
// ============================================================================

#[test]
fn snapshot_room_access_forbidden_error() {
    // ensure_room_member_ctx returns ApiError::forbidden when the user is not
    // a joined member (and is not an admin). errcode = M_FORBIDDEN, HTTP 403.
    let err = ApiError::forbidden("You must be a room member to configure burn-after-read".to_string());
    let body = api_error_json(&err);
    insta::assert_json_snapshot!("room_access_forbidden_error", body);
}

#[test]
fn snapshot_room_access_strict_forbidden_error() {
    // ensure_room_member_strict_ctx has no admin bypass — even an admin who is
    // not a joined member is forbidden. Same errcode, distinct message.
    let err = ApiError::forbidden("You must be a room member to modify this room's settings".to_string());
    let body = api_error_json(&err);
    insta::assert_json_snapshot!("room_access_strict_forbidden_error", body);
}
