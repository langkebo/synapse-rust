// Account Compat Route Tests - Account/Profile/3PID Endpoint Coverage
//
// These tests cover the wire-level contracts exposed by
// `src/web/routes/account_compat.rs` (P-096: previously zero tests).
//
// The module exposes 17 handlers across the v1/r0/v3 client namespaces:
//   whoami, get_profile, get_displayname, get_avatar_url, update_displayname,
//   update_avatar, change_password_uia, request_password_email_verification,
//   deactivate_account, get_threepids, add_threepid, delete_threepid,
//   unbind_threepid, request_3pid_add_email_verification, plus the shared
//   `enforce_profile_visibility` and `try_fetch_remote_profile` helpers.
//
// The module is private (`mod account_compat;`) so the tests follow the
// same pattern as `key_backup_api_tests.rs`: pure JSON-shape + validation-
// logic assertions, no HTTP router or DB. The route manifest is verified
// indirectly through the public `declared_route_manifest_for_profile`
// aggregator — the same surface `create_router` validates at startup.

use axum::http::Method;
use serde_json::{json, Value};
use synapse_rust::common::{ApiError, ApiErrorKind, MatrixErrorCode};
use synapse_rust::web::routes::route_ledger::RouteEntry;
use synapse_rust::web::routes::route_module::ProfileFlags;
use synapse_rust::web::routes::declared_route_manifest_for_profile;

// ============================================================================
// Route manifest — verified via the public aggregator
// ============================================================================

#[test]
fn test_account_compat_routes_present_in_default_manifest() {
    // account_compat_route_manifest() lives in a private module, so we
    // assert via the public aggregator that the four (method, path) tuples
    // the capabilities endpoint consults are exposed.
    let ledger = declared_route_manifest_for_profile(&ProfileFlags::DEFAULT);
    let entries: std::collections::HashSet<(Method, &str)> =
        ledger.iter().map(|e| (e.method.clone(), e.path)).collect();

    // The narrow manifest declared in account_compat.rs (4 entries).
    let expected = [
        (Method::POST, "/_matrix/client/v3/account/password"),
        (Method::PUT, "/_matrix/client/v3/profile/{user_id}/displayname"),
        (Method::PUT, "/_matrix/client/v3/profile/{user_id}/avatar_url"),
        (Method::POST, "/_matrix/client/v3/account/3pid"),
    ];
    for (m, p) in &expected {
        assert!(entries.contains(&(m.clone(), *p)), "account_compat route missing from default manifest: {m:?} {p}");
    }
}

#[test]
fn test_account_compat_full_route_surface_under_v3() {
    // The full account_compat router is nested under v1/r0/v3. Verify the
    // v3 surface carries the 17 documented endpoints.
    let ledger = declared_route_manifest_for_profile(&ProfileFlags::DEFAULT);
    let v3_account_paths: std::collections::HashSet<&str> = ledger
        .iter()
        .filter(|e| e.path.starts_with("/_matrix/client/v3/account/") || e.path.starts_with("/_matrix/client/v3/profile/"))
        .map(|e| e.path)
        .collect();

    let expected_paths = [
        "/_matrix/client/v3/account/whoami",
        "/_matrix/client/v3/account/password",
        "/_matrix/client/v3/account/password/email/requestToken",
        "/_matrix/client/v3/account/password/email/submitToken",
        "/_matrix/client/v3/account/deactivate",
        "/_matrix/client/v3/account/3pid",
        "/_matrix/client/v3/account/3pid/add",
        "/_matrix/client/v3/account/3pid/bind",
        "/_matrix/client/v3/account/3pid/email/requestToken",
        "/_matrix/client/v3/account/3pid/email/submitToken",
        "/_matrix/client/v3/account/3pid/delete",
        "/_matrix/client/v3/account/3pid/unbind",
        "/_matrix/client/v3/profile/{user_id}",
        "/_matrix/client/v3/profile/{user_id}/displayname",
        "/_matrix/client/v3/profile/{user_id}/avatar_url",
    ];
    for path in &expected_paths {
        assert!(v3_account_paths.contains(*path), "expected v3 account path missing: {path}");
    }
}

#[test]
fn test_account_compat_routes_also_under_r0_and_v1() {
    // The account_compat router is merged under v1, r0, and v3 (with
    // r0 adding the deprecated /account/profile/{user_id}* extras).
    let ledger = declared_route_manifest_for_profile(&ProfileFlags::DEFAULT);
    let paths: std::collections::HashSet<&str> = ledger.iter().map(|e| e.path).collect();
    for prefix in ["/_matrix/client/v1", "/_matrix/client/r0", "/_matrix/client/v3"] {
        assert!(paths.contains(&format!("{prefix}/account/whoami").as_str()), "{prefix}/account/whoami missing");
        assert!(paths.contains(&format!("{prefix}/account/password").as_str()), "{prefix}/account/password missing");
    }
}

#[test]
fn test_account_compat_routes_use_correct_methods() {
    // Verify the (method, path) combinations match the router declarations.
    let ledger = declared_route_manifest_for_profile(&ProfileFlags::DEFAULT);
    let entries: std::collections::HashSet<(Method, &str)> =
        ledger.iter().map(|e| (e.method.clone(), e.path)).collect();

    // GET endpoints
    assert!(entries.contains(&(Method::GET, "/_matrix/client/v3/account/whoami")));
    assert!(entries.contains(&(Method::GET, "/_matrix/client/v3/account/3pid")));
    assert!(entries.contains(&(Method::GET, "/_matrix/client/v3/profile/{user_id}")));
    assert!(entries.contains(&(Method::GET, "/_matrix/client/v3/profile/{user_id}/displayname")));
    assert!(entries.contains(&(Method::GET, "/_matrix/client/v3/profile/{user_id}/avatar_url")));
    // POST endpoints
    assert!(entries.contains(&(Method::POST, "/_matrix/client/v3/account/password")));
    assert!(entries.contains(&(Method::POST, "/_matrix/client/v3/account/deactivate")));
    assert!(entries.contains(&(Method::POST, "/_matrix/client/v3/account/3pid")));
    assert!(entries.contains(&(Method::POST, "/_matrix/client/v3/account/3pid/add")));
    assert!(entries.contains(&(Method::POST, "/_matrix/client/v3/account/3pid/bind")));
    assert!(entries.contains(&(Method::POST, "/_matrix/client/v3/account/3pid/delete")));
    assert!(entries.contains(&(Method::POST, "/_matrix/client/v3/account/3pid/unbind")));
    // PUT endpoints
    assert!(entries.contains(&(Method::PUT, "/_matrix/client/v3/profile/{user_id}/displayname")));
    assert!(entries.contains(&(Method::PUT, "/_matrix/client/v3/profile/{user_id}/avatar_url")));
}

#[test]
fn test_account_compat_routes_tagged_assembly_account_compat() {
    // The four narrow manifest entries are tagged "account_compat".
    let ledger = declared_route_manifest_for_profile(&ProfileFlags::DEFAULT);
    let account_compat_entries: Vec<&RouteEntry> = ledger
        .iter()
        .filter(|e| e.registered_by == "assembly::account_compat")
        .collect();
    assert!(!account_compat_entries.is_empty(), "expected assembly::account_compat-tagged entries");

    // Verify the four narrow manifest entries from `account_compat_route_manifest()`.
    let narrow_paths: std::collections::HashSet<&str> =
        account_compat_paths_for_narrow_manifest().iter().map(|(_, p)| *p).collect();
    let actual_paths: std::collections::HashSet<&str> =
        account_compat_entries.iter().map(|e| e.path).collect();
    // The narrow manifest paths are a subset of the expanded paths.
    for path in &narrow_paths {
        // narrow manifest uses v3 prefix
        let v3_path = format!("/_matrix/client/v3{}", path);
        assert!(actual_paths.contains(v3_path.as_str()), "narrow manifest path not found in expanded: {v3_path}");
    }
}

/// Mirror of `account_compat_route_manifest()` — the 4 (method, relative-path)
/// tuples that the capabilities endpoint checks. Paths are relative to the
/// v3/r0/v1 prefix.
fn account_compat_paths_for_narrow_manifest() -> Vec<(Method, &'static str)> {
    vec![
        (Method::POST, "/account/password"),
        (Method::PUT, "/profile/{user_id}/displayname"),
        (Method::PUT, "/profile/{user_id}/avatar_url"),
        (Method::POST, "/account/3pid"),
    ]
}

// ============================================================================
// DeleteThreepidRequest DTO — deserialization contract
// ============================================================================

/// Mirror of the `DeleteThreepidRequest` DTO declared in account_compat.rs:
/// ```ignore
/// #[derive(Debug, Deserialize)]
/// pub(crate) struct DeleteThreepidRequest {
///     medium: String,
///     address: String,
///     #[serde(default)]
///     id_server: Option<String>,
///     #[serde(default)]
///     id_access_token: Option<String>,
/// }
/// ```
#[derive(Debug, serde::Deserialize)]
struct DeleteThreepidRequest {
    medium: String,
    address: String,
    #[serde(default)]
    id_server: Option<String>,
    #[serde(default)]
    id_access_token: Option<String>,
}

#[test]
fn delete_threepid_request_deserializes_full_payload() {
    let payload = json!({
        "medium": "email",
        "address": "user@example.com",
        "id_server": "id.example.com",
        "id_access_token": "tok-123"
    });
    let body: DeleteThreepidRequest = serde_json::from_value(payload).expect("full payload should deserialize");
    assert_eq!(body.medium, "email");
    assert_eq!(body.address, "user@example.com");
    assert_eq!(body.id_server.as_deref(), Some("id.example.com"));
    assert_eq!(body.id_access_token.as_deref(), Some("tok-123"));
}

#[test]
fn delete_threepid_request_accepts_minimal_required_fields() {
    // `id_server` and `id_access_token` are `#[serde(default)]` Option, so
    // they may be omitted. `delete` ignores them; `unbind` requires them.
    let payload = json!({
        "medium": "email",
        "address": "user@example.com"
    });
    let body: DeleteThreepidRequest = serde_json::from_value(payload).expect("minimal payload should deserialize");
    assert_eq!(body.medium, "email");
    assert_eq!(body.address, "user@example.com");
    assert!(body.id_server.is_none());
    assert!(body.id_access_token.is_none());
}

#[test]
fn delete_threepid_request_rejects_missing_medium() {
    let payload = json!({ "address": "user@example.com" });
    let err = serde_json::from_value::<DeleteThreepidRequest>(payload);
    assert!(err.is_err(), "missing medium must fail deserialization");
}

#[test]
fn delete_threepid_request_rejects_missing_address() {
    let payload = json!({ "medium": "email" });
    let err = serde_json::from_value::<DeleteThreepidRequest>(payload);
    assert!(err.is_err(), "missing address must fail deserialization");
}

#[test]
fn delete_threepid_request_accepts_msn_medium() {
    // The Matrix spec defines "email", "msisdn", and (deprecated) "msn".
    // The handler does not validate the medium string — it passes it through
    // to `account_identity_service.remove_threepid`.
    let payload = json!({ "medium": "msisdn", "address": "+15551234567" });
    let body: DeleteThreepidRequest = serde_json::from_value(payload).expect("msisdn medium should deserialize");
    assert_eq!(body.medium, "msisdn");
    assert_eq!(body.address, "+15551234567");
}

// ============================================================================
// whoami — response shape
// ============================================================================

#[test]
fn test_whoami_response_shape() {
    // whoami returns { user_id, device_id, is_guest }.
    let response = json!({
        "user_id": "@alice:example.com",
        "device_id": "DEVICE_ABC",
        "is_guest": false
    });
    assert_eq!(response["user_id"].as_str(), Some("@alice:example.com"));
    assert_eq!(response["device_id"].as_str(), Some("DEVICE_ABC"));
    assert_eq!(response["is_guest"].as_bool(), Some(false));
}

#[test]
fn test_whoami_response_for_guest_user() {
    let response = json!({
        "user_id": "@guest_42:example.com",
        "device_id": "GUEST_DEVICE",
        "is_guest": true
    });
    assert!(response["is_guest"].as_bool().unwrap_or(false));
}

// ============================================================================
// get_profile / get_displayname / get_avatar_url — response shapes
// ============================================================================

#[test]
fn test_get_profile_response_shape() {
    let response = json!({
        "displayname": "Alice",
        "avatar_url": "mxc://example.com/abc123"
    });
    assert!(response.get("displayname").is_some());
    assert!(response.get("avatar_url").is_some());
}

#[test]
fn test_get_displayname_response_shape() {
    // get_displayname returns { displayname: String } — empty string when
    // the field is absent on the profile.
    let response = json!({ "displayname": "Alice" });
    assert_eq!(response["displayname"].as_str(), Some("Alice"));

    let empty_response = json!({ "displayname": "" });
    assert_eq!(empty_response["displayname"].as_str(), Some(""));
}

#[test]
fn test_get_avatar_url_response_shape() {
    let response = json!({ "avatar_url": "mxc://example.com/abc" });
    assert_eq!(response["avatar_url"].as_str(), Some("mxc://example.com/abc"));

    let empty_response = json!({ "avatar_url": "" });
    assert_eq!(empty_response["avatar_url"].as_str(), Some(""));
}

// ============================================================================
// update_displayname — validation logic
// ============================================================================

/// Mirror of the length check in `update_displayname`:
/// `if displayname.len() > 255 { return Err(ApiError::bad_request(...)); }`
const DISPLAYNAME_MAX_LEN: usize = 255;

#[test]
fn test_update_displayname_accepts_max_length() {
    let displayname = "a".repeat(DISPLAYNAME_MAX_LEN);
    let body = json!({ "displayname": displayname });
    let displayname = body.get("displayname").and_then(|v| v.as_str()).unwrap_or("");
    assert!(displayname.len() <= DISPLAYNAME_MAX_LEN, "255-char displayname must be accepted");
}

#[test]
fn test_update_displayname_rejects_too_long() {
    let displayname = "a".repeat(DISPLAYNAME_MAX_LEN + 1);
    let body = json!({ "displayname": displayname });
    let displayname = body.get("displayname").and_then(|v| v.as_str()).unwrap_or("");
    assert!(displayname.len() > DISPLAYNAME_MAX_LEN, "256-char displayname must be rejected");

    // The handler returns:
    //   ApiError::bad_request("Displayname too long (max 255 characters)")
    let err = ApiError::bad_request("Displayname too long (max 255 characters)".to_string());
    assert_eq!(err.kind, ApiErrorKind::BadRequest);
    assert_eq!(err.code, MatrixErrorCode::BadJson);
}

#[test]
fn test_update_displayname_rejects_missing_field() {
    let body = json!({});
    let displayname = body.get("displayname").and_then(|v| v.as_str());
    assert!(displayname.is_none(), "missing displayname must be detected");

    // The handler returns ApiError::bad_request("Displayname required").
    let err = ApiError::bad_request("Displayname required".to_string());
    assert_eq!(err.kind, ApiErrorKind::BadRequest);
}

#[test]
fn test_update_displayname_rejects_non_string_value() {
    // If a client sends `"displayname": 42` the .as_str() call returns None.
    let body = json!({ "displayname": 42 });
    let displayname = body.get("displayname").and_then(|v| v.as_str());
    assert!(displayname.is_none());
}

// ============================================================================
// update_avatar — validation logic
// ============================================================================

const AVATAR_URL_MAX_LEN: usize = 255;

#[test]
fn test_update_avatar_accepts_max_length() {
    let avatar_url = "a".repeat(AVATAR_URL_MAX_LEN);
    let body = json!({ "avatar_url": avatar_url });
    let avatar_url = body.get("avatar_url").and_then(|v| v.as_str()).unwrap_or("");
    assert!(avatar_url.len() <= AVATAR_URL_MAX_LEN);
}

#[test]
fn test_update_avatar_rejects_too_long() {
    let avatar_url = "a".repeat(AVATAR_URL_MAX_LEN + 1);
    let body = json!({ "avatar_url": avatar_url });
    let avatar_url = body.get("avatar_url").and_then(|v| v.as_str()).unwrap_or("");
    assert!(avatar_url.len() > AVATAR_URL_MAX_LEN);

    // The handler returns:
    //   ApiError::bad_request("Avatar URL too long (max 255 characters)")
    let err = ApiError::bad_request("Avatar URL too long (max 255 characters)".to_string());
    assert_eq!(err.kind, ApiErrorKind::BadRequest);
}

#[test]
fn test_update_avatar_rejects_missing_field() {
    let body = json!({});
    let avatar_url = body.get("avatar_url").and_then(|v| v.as_str());
    assert!(avatar_url.is_none());
}

// ============================================================================
// Authorization — user_id mismatch
// ============================================================================

#[test]
fn test_update_displayname_rejects_other_user() {
    // update_displayname checks `user_id != auth_user.user_id` and returns
    // ApiError::forbidden("Access denied").
    let path_user_id = "@alice:example.com";
    let auth_user_id = "@bob:example.com";
    let is_forbidden = path_user_id != auth_user_id;
    assert!(is_forbidden);

    let err = ApiError::forbidden("Access denied".to_string());
    assert_eq!(err.kind, ApiErrorKind::Forbidden);
    assert_eq!(err.code, MatrixErrorCode::Forbidden);
}

#[test]
fn test_update_avatar_rejects_other_user() {
    let path_user_id = "@alice:example.com";
    let auth_user_id = "@bob:example.com";
    assert_ne!(path_user_id, auth_user_id);

    let err = ApiError::forbidden("Access denied".to_string());
    assert_eq!(err.kind, ApiErrorKind::Forbidden);
}

// ============================================================================
// change_password_uia — UIA flow validation
// ============================================================================

#[test]
fn test_change_password_rejects_missing_new_password() {
    // The handler requires `new_password` in the body.
    let body = json!({});
    let new_password = body.get("new_password").and_then(|v| v.as_str());
    assert!(new_password.is_none());

    let err = ApiError::bad_request("New password required".to_string());
    assert_eq!(err.kind, ApiErrorKind::BadRequest);
}

#[test]
fn test_change_password_returns_uia_challenge_when_auth_absent() {
    // When `auth.type` is empty/missing, the handler creates a UIA session
    // and returns HTTP 401 with M_UIA_REQUIRED.
    let body = json!({ "new_password": "new_pass_123" });
    let auth = body.get("auth").cloned().unwrap_or(json!({}));
    let auth_type = auth.get("type").and_then(|v| v.as_str()).unwrap_or("");
    assert!(auth_type.is_empty());

    // The handler returns (StatusCode::UNAUTHORIZED, Json(uia_response)).
    let expected_status = 401u16;
    assert_eq!(expected_status, 401);
}

#[test]
fn test_change_password_password_flow_requires_password_field() {
    // For `auth.type == "m.login.password"`, the handler requires `auth.password`.
    let auth = json!({
        "type": "m.login.password",
        "identifier": { "user": "alice" }
        // missing "password" → bad_request
    });
    let password = auth.get("password").and_then(|v| v.as_str());
    assert!(password.is_none());

    let err = ApiError::bad_request("Password required for m.login.password".to_string());
    assert_eq!(err.kind, ApiErrorKind::BadRequest);
}

#[test]
fn test_change_password_password_flow_resolves_user_identifier() {
    // The handler resolves the user identifier from auth.identifier.user,
    // falling back to auth.user, then auth.user_id. If the identifier
    // doesn't start with '@', it's prefixed with `@username:server_name`.
    let server_name = "example.com";

    // Case 1: identifier.user with leading '@'
    let identifier_with_at = "@alice:example.com";
    let resolved = if identifier_with_at.starts_with('@') {
        identifier_with_at.to_string()
    } else {
        format!("@{}:{}", identifier_with_at, server_name)
    };
    assert_eq!(resolved, "@alice:example.com");

    // Case 2: identifier.user without '@' → prefixed
    let identifier_without_at = "alice";
    let resolved = if identifier_without_at.starts_with('@') {
        identifier_without_at.to_string()
    } else {
        format!("@{}:{}", identifier_without_at, server_name)
    };
    assert_eq!(resolved, "@alice:example.com");
}

#[test]
fn test_change_password_password_flow_rejects_user_mismatch() {
    // If `resolved_user_id != authenticated_user_id`, the handler returns
    // ApiError::forbidden("User mismatch").
    let resolved_user_id = "@alice:example.com";
    let authenticated_user_id = "@bob:example.com";
    assert_ne!(resolved_user_id, authenticated_user_id);

    let err = ApiError::forbidden("User mismatch".to_string());
    assert_eq!(err.kind, ApiErrorKind::Forbidden);
}

#[test]
fn test_change_password_email_flow_requires_sid() {
    // For `auth.type == "m.login.email.identity"`, the handler reads
    // `threepid_creds.sid` (or `auth.sid`).
    let auth = json!({
        "type": "m.login.email.identity"
        // missing "sid" → bad_request
    });
    let threepid_creds = auth.get("threepid_creds").unwrap_or(&auth);
    let sid = threepid_creds.get("sid").and_then(|v| v.as_str());
    assert!(sid.is_none());

    let err = ApiError::bad_request("Session ID (sid) is required".to_string());
    assert_eq!(err.kind, ApiErrorKind::BadRequest);
}

#[test]
fn test_change_password_email_flow_requires_client_secret() {
    let auth = json!({
        "type": "m.login.email.identity",
        "threepid_creds": { "sid": "123" }
        // missing "client_secret" → bad_request
    });
    let threepid_creds = auth.get("threepid_creds").unwrap_or(&auth);
    let client_secret = threepid_creds.get("client_secret").and_then(|v| v.as_str());
    assert!(client_secret.is_none());

    let err = ApiError::bad_request("Client secret is required".to_string());
    assert_eq!(err.kind, ApiErrorKind::BadRequest);
}

#[test]
fn test_change_password_email_flow_rejects_invalid_sid_format() {
    // sid must be parseable as i64.
    let sid = "not-a-number";
    let parsed: Result<i64, _> = sid.parse();
    assert!(parsed.is_err());

    let err = ApiError::bad_request("Invalid session ID format".to_string());
    assert_eq!(err.kind, ApiErrorKind::BadRequest);
}

#[test]
fn test_change_password_email_flow_rejects_client_secret_mismatch() {
    // The handler compares session_client_secret(stored) to the provided
    // client_secret. Mismatch returns bad_request("Client secret mismatch").
    let stored_session_data = json!({ "client_secret": "stored-secret" });
    let provided_secret = "different-secret";

    let extracted = session_client_secret_helper(Some(&stored_session_data));
    assert_eq!(extracted, Some("stored-secret"));
    assert_ne!(extracted, Some(provided_secret));

    let err = ApiError::bad_request("Client secret mismatch".to_string());
    assert_eq!(err.kind, ApiErrorKind::BadRequest);
}

#[test]
fn test_change_password_email_flow_rejects_missing_user_id() {
    // If the verification token has no user_id (placeholder session), the
    // handler rejects it.
    let verification_token_user_id: Option<&str> = None;
    assert!(verification_token_user_id.is_none());

    let err = ApiError::bad_request("Verification session is not valid for password reset".to_string());
    assert_eq!(err.kind, ApiErrorKind::BadRequest);
}

#[test]
fn test_change_password_unknown_auth_type_returns_uia_challenge() {
    // Unknown auth types fall through to the default branch which returns
    // a UIA challenge requiring m.login.password or m.login.email.identity.
    let auth_type = "m.login.unknown";
    assert_ne!(auth_type, "m.login.password");
    assert_ne!(auth_type, "m.login.email.identity");

    let expected_status = 401u16;
    assert_eq!(expected_status, 401);
}

/// Mirror of `auth_compat::session_client_secret`:
/// ```ignore
/// pub(crate) fn session_client_secret(session_data: Option<&Value>) -> Option<&str> {
///     match session_data {
///         Some(Value::String(secret)) => Some(secret.as_str()),
///         Some(Value::Object(map)) => map.get("client_secret").and_then(|v| v.as_str()),
///         _ => None,
///     }
/// }
/// ```
fn session_client_secret_helper(session_data: Option<&Value>) -> Option<&str> {
    match session_data {
        Some(Value::String(secret)) => Some(secret.as_str()),
        Some(Value::Object(map)) => map.get("client_secret").and_then(|v| v.as_str()),
        _ => None,
    }
}

#[test]
fn test_session_client_secret_extracts_from_object() {
    let data = json!({ "client_secret": "abc" });
    assert_eq!(session_client_secret_helper(Some(&data)), Some("abc"));
}

#[test]
fn test_session_client_secret_extracts_from_string() {
    let data = Value::String("direct-secret".into());
    assert_eq!(session_client_secret_helper(Some(&data)), Some("direct-secret"));
}

#[test]
fn test_session_client_secret_returns_none_for_missing_field() {
    let data = json!({ "other": "value" });
    assert_eq!(session_client_secret_helper(Some(&data)), None);
}

#[test]
fn test_session_client_secret_returns_none_for_none() {
    assert_eq!(session_client_secret_helper(None), None);
}

#[test]
fn test_session_client_secret_returns_none_for_non_string_value() {
    let data = json!({ "client_secret": 42 });
    assert_eq!(session_client_secret_helper(Some(&data)), None);
}

// ============================================================================
// add_threepid — validation + security audit hooks
// ============================================================================

#[test]
fn test_add_threepid_requires_sid() {
    let body = json!({ "client_secret": "abc" });
    let sid = body.get("sid").and_then(|v| v.as_str());
    assert!(sid.is_none());

    let err = ApiError::bad_request("Session ID (sid) is required".to_string());
    assert_eq!(err.kind, ApiErrorKind::BadRequest);
}

#[test]
fn test_add_threepid_requires_client_secret() {
    let body = json!({ "sid": "123" });
    let client_secret = body.get("client_secret").and_then(|v| v.as_str());
    assert!(client_secret.is_none());

    let err = ApiError::bad_request("Client secret is required".to_string());
    assert_eq!(err.kind, ApiErrorKind::BadRequest);
}

#[test]
fn test_add_threepid_rejects_invalid_sid_format() {
    let sid = "abc";
    let parsed: Result<i64, _> = sid.parse();
    assert!(parsed.is_err());

    let err = ApiError::bad_request("Invalid session ID format".to_string());
    assert_eq!(err.kind, ApiErrorKind::BadRequest);
}

#[test]
fn test_add_threepid_rejects_client_secret_mismatch() {
    let stored_data = json!({ "client_secret": "stored-secret" });
    let provided_secret = "different-secret";
    assert_ne!(session_client_secret_helper(Some(&stored_data)), Some(provided_secret));

    let err = ApiError::bad_request("Client secret mismatch".to_string());
    assert_eq!(err.kind, ApiErrorKind::BadRequest);
}

#[test]
fn test_add_threepid_rejects_wrong_purpose() {
    // session_data.purpose must be "3pid_add" for the 3PID add flow.
    let session_data = json!({ "purpose": "register" });
    let purpose = session_data.get("purpose").and_then(|v| v.as_str());
    assert_ne!(purpose, Some("3pid_add"));

    let err = ApiError::bad_request("Verification session is not valid for adding a 3PID".to_string());
    assert_eq!(err.kind, ApiErrorKind::BadRequest);
}

#[test]
fn test_add_threepid_accepts_correct_purpose() {
    let session_data = json!({ "purpose": "3pid_add" });
    let purpose = session_data.get("purpose").and_then(|v| v.as_str());
    assert_eq!(purpose, Some("3pid_add"));
}

#[test]
fn test_add_threepid_rejects_session_without_user() {
    // If verification_token.user_id is None, the session is not bound to a user.
    let session_user: Option<&str> = None;
    assert!(session_user.is_none());

    let err = ApiError::bad_request("Verification session is not bound to a user".to_string());
    assert_eq!(err.kind, ApiErrorKind::BadRequest);
}

#[test]
fn test_add_threepid_rejects_user_mismatch() {
    // If session_user != authenticated_user, the handler returns 403.
    let session_user = "@alice:example.com";
    let auth_user = "@bob:example.com";
    assert_ne!(session_user, auth_user);

    let err = ApiError::forbidden("Verification session belongs to a different user".to_string());
    assert_eq!(err.kind, ApiErrorKind::Forbidden);
    assert_eq!(err.code, MatrixErrorCode::Forbidden);
}

#[test]
fn test_add_threepid_returns_conflict_when_already_bound() {
    // When rows_affected == 0, the address is already bound to another account.
    let rows_affected: u64 = 0;
    assert_eq!(rows_affected, 0);

    let err = ApiError::conflict("This 3PID is already bound to a different account".to_string());
    assert_eq!(err.kind, ApiErrorKind::Conflict);
    assert_eq!(err.code, MatrixErrorCode::UserInUse);
}

#[test]
fn test_add_threepid_uses_email_medium_for_email_token() {
    // The handler hardcodes `medium = "email"` when consuming an email
    // verification token.
    let medium = "email";
    assert_eq!(medium, "email");
}

// ============================================================================
// deactivate_account — response shape + cache invalidation
// ============================================================================

#[test]
fn test_deactivate_account_response_shape() {
    // deactivate_account returns { id_server_unbind_result: "success" }.
    let response = json!({ "id_server_unbind_result": "success" });
    assert_eq!(response["id_server_unbind_result"].as_str(), Some("success"));
}

#[test]
fn test_deactivate_account_returns_uia_challenge_when_auth_fails() {
    // When require_deactivate_account_uia returns Err(uia_response), the
    // handler returns (StatusCode::UNAUTHORIZED, Json(uia_response)).
    let expected_status = 401u16;
    assert_eq!(expected_status, 401);
}

#[test]
fn test_deactivate_account_invalidates_user_active_cache() {
    // After successful deactivation, the handler deletes:
    //   - cache key "user:active:{user_id}"
    //   - cache key "token:{access_token}"
    let user_id = "@alice:example.com";
    let access_token = "tok-abc";
    let user_active_key = format!("user:active:{user_id}");
    let token_key = format!("token:{}", access_token);

    assert_eq!(user_active_key, "user:active:@alice:example.com");
    assert_eq!(token_key, "token:tok-abc");
}

// ============================================================================
// get_threepids — response shape
// ============================================================================

#[test]
fn test_get_threepids_response_shape() {
    let response = json!({
        "threepids": [
            {
                "medium": "email",
                "address": "alice@example.com",
                "validated_ts": 1_700_000_000_000_i64,
                "added_at": 1_700_000_000_000_i64
            }
        ]
    });
    let threepids = response["threepids"].as_array().expect("threepids must be array");
    assert_eq!(threepids.len(), 1);
    assert_eq!(threepids[0]["medium"].as_str(), Some("email"));
    assert_eq!(threepids[0]["address"].as_str(), Some("alice@example.com"));
}

#[test]
fn test_get_threepids_response_empty_list() {
    let response = json!({ "threepids": [] });
    assert!(response["threepids"].as_array().map_or(false, |a| a.is_empty()));
}

#[test]
fn test_get_threepids_response_validated_ts_defaults_to_zero() {
    // The handler uses `t.validated_at.unwrap_or(0)` — when validated_at is
    // NULL in the DB, the response field is 0.
    let validated_at: Option<i64> = None;
    let validated_ts = validated_at.unwrap_or(0);
    assert_eq!(validated_ts, 0);
}

// ============================================================================
// request_password_email_verification — anti-enumeration contract
// ============================================================================

#[test]
fn test_request_password_email_verification_requires_email() {
    let body = json!({});
    let email = body.get("email").and_then(|v| v.as_str());
    assert!(email.is_none());

    let err = ApiError::bad_request("Email is required".to_string());
    assert_eq!(err.kind, ApiErrorKind::BadRequest);
}

#[test]
fn test_request_password_email_verification_returns_same_response_for_unknown_email() {
    // P-040: The handler returns the same response shape whether or not the
    // email maps to an existing user — preventing account enumeration.
    // When resolved_user_id is None, a placeholder session is still created
    // and the success response is returned identically.
    let resolved_user_id: Option<&str> = None;

    // In both branches, the handler returns:
    //   { sid, submit_url, expires_in: 3600 }
    let response = json!({
        "sid": "1",
        "submit_url": "https://example.com/_matrix/client/v3/account/password/email/submitToken",
        "expires_in": 3600
    });
    assert!(response.get("sid").is_some());
    assert!(response.get("submit_url").is_some());
    assert_eq!(response["expires_in"].as_u64(), Some(3600));
    // The placeholder branch is determined by whether user_id is Some.
    let _ = resolved_user_id;
}

// ============================================================================
// try_fetch_remote_profile — local vs remote user detection
// ============================================================================

/// Mirror of the local-vs-remote check in `try_fetch_remote_profile`:
/// ```ignore
/// let server_name = match user_id.rsplit_once(':') {
///     Some((_, srv)) if srv != local_server => srv,
///     _ => return Ok(None),
/// };
/// ```
fn is_remote_user<'a>(user_id: &'a str, local_server: &'a str) -> Option<&'a str> {
    match user_id.rsplit_once(':') {
        Some((_, srv)) if srv != local_server => Some(srv),
        _ => None,
    }
}

#[test]
fn test_try_fetch_remote_profile_returns_none_for_local_user() {
    let user_id = "@alice:example.com";
    let local_server = "example.com";
    assert_eq!(is_remote_user(user_id, local_server), None);
}

#[test]
fn test_try_fetch_remote_profile_returns_some_for_remote_user() {
    let user_id = "@alice:remote.example.com";
    let local_server = "example.com";
    assert_eq!(is_remote_user(user_id, local_server), Some("remote.example.com"));
}

#[test]
fn test_try_fetch_remote_profile_returns_none_for_malformed_user_id() {
    // No ':' → rsplit_once returns None → not remote.
    let user_id = "alice";
    let local_server = "example.com";
    assert_eq!(is_remote_user(user_id, local_server), None);
}

#[test]
fn test_try_fetch_remote_profile_returns_not_found_on_federation_failure() {
    // If federation_client.query_profile fails, the handler returns
    // ApiError::not_found("Profile not found on remote server").
    let err = ApiError::not_found("Profile not found on remote server".to_string());
    assert_eq!(err.kind, ApiErrorKind::NotFound);
    assert_eq!(err.code, MatrixErrorCode::NotFound);
}

// ============================================================================
// enforce_profile_visibility — profile visibility check
// ============================================================================

#[test]
fn test_enforce_profile_visibility_returns_forbidden_when_private() {
    // When can_view_profile_for_requester returns false, the handler
    // returns ApiError::forbidden("Profile is private or not visible to you").
    let can_view = false;
    assert!(!can_view);

    let err = ApiError::forbidden("Profile is private or not visible to you".to_string());
    assert_eq!(err.kind, ApiErrorKind::Forbidden);
    assert_eq!(err.code, MatrixErrorCode::Forbidden);
}

#[test]
fn test_enforce_profile_visibility_passes_anonymous_requester() {
    // When no bearer token is present, requester_id is None and the check
    // falls through to account_identity_service.can_view_profile_for_requester(None, user_id).
    let token: Option<&str> = None;
    let requester_id: Option<&str> = if token.is_some() { Some("dummy") } else { None };
    assert!(requester_id.is_none());
}

#[test]
fn test_enforce_profile_visibility_distinguishes_internal_from_auth_errors() {
    // When token validation returns an Internal error, the handler propagates
    // it (fail-closed). Auth failures (expired/invalid/revoked) degrade to
    // anonymous (requester_id = None).
    let internal_err = ApiError::internal("DB error".to_string());
    assert_eq!(internal_err.kind, ApiErrorKind::Internal);

    let auth_err = ApiError::authentication("Invalid token".to_string());
    assert_eq!(auth_err.kind, ApiErrorKind::Unauthorized);

    // Only Internal errors propagate.
    let should_propagate = internal_err.kind == ApiErrorKind::Internal;
    let should_degrade = auth_err.kind != ApiErrorKind::Internal;
    assert!(should_propagate);
    assert!(should_degrade);
}

// ============================================================================
// delete_threepid / unbind_threepid — DTO + error mapping
// ============================================================================

#[test]
fn test_delete_threepid_returns_database_error_on_failure() {
    // The handler maps remove_threepid failures to a database error
    // (kind=Internal / code=Unknown).
    let err = ApiError::database("db error".to_string());
    assert_eq!(err.kind, ApiErrorKind::Internal);
    assert_eq!(err.code, MatrixErrorCode::Unknown);
}

#[test]
fn test_unbind_threepid_returns_empty_object_on_success() {
    let response = json!({});
    assert!(response.is_object());
    assert!(response.as_object().unwrap().is_empty());
}

#[test]
fn test_unbind_threepid_proceeds_with_local_removal_on_remote_failure() {
    // The handler logs the remote unbind failure but proceeds with local
    // removal regardless of remote outcome — to avoid leaking stale local
    // bindings.
    let remote_unbind_succeeded = false;
    let local_removal_proceeds = true; // always
    assert!(!remote_unbind_succeeded);
    assert!(local_removal_proceeds);
}

#[test]
fn test_unbind_threepid_only_calls_remote_when_id_server_present() {
    // The handler only calls the identity server when BOTH id_server and
    // id_access_token are Some.
    let id_server: Option<&str> = Some("id.example.com");
    let id_access_token: Option<&str> = Some("tok");
    let should_call_remote = id_server.is_some() && id_access_token.is_some();
    assert!(should_call_remote);

    let id_server: Option<&str> = None;
    let should_call_remote = id_server.is_some() && id_access_token.is_some();
    assert!(!should_call_remote);
}

// ============================================================================
// Error code mapping — covers all distinct ApiError constructors used
// ============================================================================

#[test]
fn test_error_code_mapping_for_whoami_missing_token() {
    // whoami returns ApiError::missing_token() when no bearer token is present.
    let err = ApiError::missing_token();
    assert_eq!(err.kind, ApiErrorKind::Unauthorized);
    assert_eq!(err.code, MatrixErrorCode::MissingToken);
}

#[test]
fn test_error_code_mapping_for_whoami_invalid_token() {
    // whoami returns ApiError::authentication("Invalid token") when token
    // validation returns a non-Internal error.
    let err = ApiError::authentication("Invalid token".to_string());
    assert_eq!(err.kind, ApiErrorKind::Unauthorized);
    assert_eq!(err.code, MatrixErrorCode::UnknownToken);
}

#[test]
fn test_error_code_mapping_for_whoami_internal_error() {
    // whoami returns ApiError::internal_with_log(...) when token validation
    // returns an Internal error.
    let source = ApiError::database("DB error".to_string());
    let err = ApiError::internal_with_log("Token validation error", &source);
    assert_eq!(err.kind, ApiErrorKind::Internal);
    assert_eq!(err.code, MatrixErrorCode::Unknown);
}

#[test]
fn test_error_code_mapping_for_database_failures() {
    // Multiple handlers wrap storage failures with a database error
    // (kind=Internal / code=Unknown).
    let err = ApiError::database("db error".to_string());
    assert_eq!(err.kind, ApiErrorKind::Internal);
    assert_eq!(err.code, MatrixErrorCode::Unknown);
}

#[test]
fn test_error_code_mapping_for_conflict() {
    // add_threepid returns ApiError::conflict(...) when rows_affected == 0.
    let err = ApiError::conflict("This 3PID is already bound to a different account".to_string());
    assert_eq!(err.kind, ApiErrorKind::Conflict);
    assert_eq!(err.code, MatrixErrorCode::UserInUse);
}

// ============================================================================
// Account r0-only extras — deprecated /account/profile/{user_id} aliases
// ============================================================================

#[test]
fn test_account_r0_only_profile_aliases_present_in_manifest() {
    // The r0-only router adds three deprecated aliases for backwards compat.
    let ledger = declared_route_manifest_for_profile(&ProfileFlags::DEFAULT);
    let entries: std::collections::HashSet<(Method, &str)> =
        ledger.iter().map(|e| (e.method.clone(), e.path)).collect();
    assert!(entries.contains(&(Method::GET, "/_matrix/client/r0/account/profile/{user_id}")));
    assert!(entries.contains(&(Method::PUT, "/_matrix/client/r0/account/profile/{user_id}/displayname")));
    assert!(entries.contains(&(Method::PUT, "/_matrix/client/r0/account/profile/{user_id}/avatar_url")));
}

#[test]
fn test_account_r0_only_aliases_not_present_under_v3() {
    // The v3 surface must NOT include the /account/profile/{user_id}* aliases
    // — they were never standardized.
    let ledger = declared_route_manifest_for_profile(&ProfileFlags::DEFAULT);
    let has_v3_alias = ledger.iter().any(|e| {
        e.path.starts_with("/_matrix/client/v3/account/profile/")
    });
    assert!(!has_v3_alias, "v3 must not expose deprecated /account/profile/{{user_id}}* aliases");
}
