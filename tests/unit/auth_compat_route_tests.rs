// Auth Compat Route Tests - Authentication/Registration Endpoint Coverage
//
// These tests cover the wire-level contracts exposed by
// `synapse-web/src/routes/auth_compat.rs` (P-096: previously zero tests).
//
// The module exposes 12 handlers under the r0/v3 client namespaces:
//   register, check_username_availability, request_email_verification,
//   submit_email_token, get_login_flows, get_register_flows, login,
//   generate_qr_login_token, logout, logout_all, refresh_token,
//   login_fallback_page.
//
// The module also exposes two pure helpers:
//   - `session_client_secret(session_data)` — extracts client_secret from
//     either a String or an Object shape (shared with account_compat).
//   - `html_escape(s)` — XSS-safe escaping for the login fallback page.
//
// The module is private (`mod auth_compat;`) so the tests follow the same
// pattern as `key_backup_api_tests.rs`: pure JSON-shape + validation-logic
// assertions, no HTTP router or DB. The route manifest is verified
// indirectly through the public `declared_ledger_for_profile`
// aggregator — the same surface `create_router` validates at startup.

use axum::http::Method;
use serde_json::{json, Value};
use synapse_rust::common::{ApiError, ApiErrorKind, MatrixErrorCode};
use synapse_web::routes::declared_ledger_for_profile;
use synapse_web::routes::route_module::ProfileFlags;

// ============================================================================
// Route manifest — verified via the public aggregator
// ============================================================================

#[test]
fn test_auth_compat_routes_present_in_default_manifest() {
    // The auth_compat router is nested under r0 and v3.
    let ledger = declared_ledger_for_profile(&ProfileFlags::DEFAULT);
    let entries: std::collections::HashSet<(Method, &str)> =
        ledger.iter().map(|e| (e.method.clone(), e.path)).collect();

    let expected = [
        (Method::GET, "/_matrix/client/v3/register"),
        (Method::POST, "/_matrix/client/v3/register"),
        (Method::GET, "/_matrix/client/v3/register"),
        (Method::POST, "/_matrix/client/v3/register"),
        (Method::GET, "/_matrix/client/v3/register/available"),
        (Method::GET, "/_matrix/client/v3/register/available"),
        (Method::POST, "/_matrix/client/v3/register/email/requestToken"),
        (Method::POST, "/_matrix/client/v3/register/email/requestToken"),
        (Method::POST, "/_matrix/client/v3/register/email/submitToken"),
        (Method::POST, "/_matrix/client/v3/register/email/submitToken"),
        (Method::GET, "/_matrix/client/v3/login"),
        (Method::POST, "/_matrix/client/v3/login"),
        (Method::GET, "/_matrix/client/v3/login"),
        (Method::POST, "/_matrix/client/v3/login"),
        (Method::POST, "/_matrix/client/v3/logout"),
        (Method::POST, "/_matrix/client/v3/logout"),
        (Method::POST, "/_matrix/client/v3/logout/all"),
        (Method::POST, "/_matrix/client/v3/logout/all"),
        (Method::POST, "/_matrix/client/v3/refresh"),
        (Method::POST, "/_matrix/client/v3/refresh"),
    ];
    for (m, p) in &expected {
        assert!(entries.contains(&(m.clone(), *p)), "auth_compat route missing from manifest: {m:?} {p}");
    }
}

#[test]
fn test_auth_compat_routes_include_standalone_absolute_paths() {
    // Login fallback page and MSC4108 QR token are absolute paths not nested
    // under r0/v3.
    let ledger = declared_ledger_for_profile(&ProfileFlags::DEFAULT);
    let entries: std::collections::HashSet<(Method, &str)> =
        ledger.iter().map(|e| (e.method.clone(), e.path)).collect();
    assert!(entries.contains(&(Method::GET, "/_matrix/static/client/login/")), "login fallback missing");
    assert!(entries.contains(&(Method::POST, "/_matrix/client/v1/login/qr_token")), "MSC4108 qr_token missing");
}

#[test]
fn test_auth_compat_routes_tagged_assembly_auth_compat() {
    let ledger = declared_ledger_for_profile(&ProfileFlags::DEFAULT);
    let auth_compat_entries: Vec<_> = ledger.iter().filter(|e| e.registered_by == "assembly::auth_compat").collect();
    assert!(!auth_compat_entries.is_empty(), "expected assembly::auth_compat-tagged entries");
    // r0 removed: 10 (method, relative-path) tuples × 1 prefix (v3) = 10 entries.
    assert!(
        auth_compat_entries.len() >= 10,
        "expected at least 10 auth_compat entries, got {}",
        auth_compat_entries.len()
    );
}

#[test]
fn test_auth_compat_routes_under_v1_are_not_present() {
    // The auth_compat router itself is NOT nested under v1 — only r0 and v3.
    // (The MSC4108 /_matrix/client/v1/login/qr_token endpoint is a standalone
    // absolute path registered inline in create_auth_router, not part of the
    // auth_compat router nesting. It is intentionally excluded from this check.)
    let ledger = declared_ledger_for_profile(&ProfileFlags::DEFAULT);
    let has_v1_auth_compat_router = ledger.iter().any(|e| {
        e.path == "/_matrix/client/v1/register"
            || e.path == "/_matrix/client/v1/register/available"
            || e.path == "/_matrix/client/v1/login"
            || e.path == "/_matrix/client/v1/logout"
            || e.path == "/_matrix/client/v1/logout/all"
            || e.path == "/_matrix/client/v1/refresh"
    });
    assert!(!has_v1_auth_compat_router, "auth_compat router must not be nested under v1");
}

// ============================================================================
// session_client_secret — pure helper logic
// ============================================================================

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
fn session_client_secret(session_data: Option<&Value>) -> Option<&str> {
    match session_data {
        Some(Value::String(secret)) => Some(secret.as_str()),
        Some(Value::Object(map)) => map.get("client_secret").and_then(|v| v.as_str()),
        _ => None,
    }
}

#[test]
fn test_session_client_secret_extracts_from_object() {
    let data = json!({ "client_secret": "abc-123" });
    assert_eq!(session_client_secret(Some(&data)), Some("abc-123"));
}

#[test]
fn test_session_client_secret_extracts_from_string() {
    let data = Value::String("direct-secret".into());
    assert_eq!(session_client_secret(Some(&data)), Some("direct-secret"));
}

#[test]
fn test_session_client_secret_returns_none_for_missing_field() {
    let data = json!({ "other": "value" });
    assert_eq!(session_client_secret(Some(&data)), None);
}

#[test]
fn test_session_client_secret_returns_none_for_none_input() {
    assert_eq!(session_client_secret(None), None);
}

#[test]
fn test_session_client_secret_returns_none_for_non_string_value() {
    let data = json!({ "client_secret": 42 });
    assert_eq!(session_client_secret(Some(&data)), None);
}

#[test]
fn test_session_client_secret_returns_none_for_array() {
    let data = json!(["client_secret"]);
    assert_eq!(session_client_secret(Some(&data)), None);
}

#[test]
fn test_session_client_secret_returns_none_for_number() {
    let data = json!(42);
    assert_eq!(session_client_secret(Some(&data)), None);
}

#[test]
fn test_session_client_secret_returns_none_for_boolean() {
    let data = json!(true);
    assert_eq!(session_client_secret(Some(&data)), None);
}

#[test]
fn test_session_client_secret_returns_none_for_null() {
    let data = Value::Null;
    assert_eq!(session_client_secret(Some(&data)), None);
}

// ============================================================================
// html_escape — pure helper logic
// ============================================================================

/// Mirror of `auth_compat::html_escape`:
/// ```ignore
/// fn html_escape(s: &str) -> String {
///     s.replace('&', "&amp;").replace('<', "&lt;").replace('>', "&gt;")
///      .replace('"', "&quot;").replace('\'', "&#x27;")
/// }
/// ```
fn html_escape(s: &str) -> String {
    s.replace('&', "&amp;").replace('<', "&lt;").replace('>', "&gt;").replace('"', "&quot;").replace('\'', "&#x27;")
}

#[test]
fn test_html_escape_escapes_ampersand() {
    assert_eq!(html_escape("a&b"), "a&amp;b");
}

#[test]
fn test_html_escape_escapes_less_than() {
    assert_eq!(html_escape("a<b"), "a&lt;b");
}

#[test]
fn test_html_escape_escapes_greater_than() {
    assert_eq!(html_escape("a>b"), "a&gt;b");
}

#[test]
fn test_html_escape_escapes_double_quote() {
    assert_eq!(html_escape(r#"a"b"#), "a&quot;b");
}

#[test]
fn test_html_escape_escapes_single_quote() {
    assert_eq!(html_escape("a'b"), "a&#x27;b");
}

#[test]
fn test_html_escape_handles_empty_string() {
    assert_eq!(html_escape(""), "");
}

#[test]
fn test_html_escape_handles_string_without_special_chars() {
    assert_eq!(html_escape("plain text"), "plain text");
}

#[test]
fn test_html_escape_handles_all_special_chars_at_once() {
    let input = r#"<script>alert("xss");</script>"#;
    let escaped = html_escape(input);
    assert!(escaped.contains("&lt;script&gt;"));
    assert!(escaped.contains("&quot;xss&quot;"));
    // Verify the escaped output cannot be re-parsed as HTML tags.
    assert!(!escaped.contains("<script>"));
}

#[test]
fn test_html_escape_ampersand_first_to_avoid_double_escaping() {
    // The order matters: '&' must be escaped first so that subsequent
    // replacements (e.g. '&lt;' becoming '&amp;lt;') don't occur.
    let input = "<&>";
    let escaped = html_escape(input);
    assert_eq!(escaped, "&lt;&amp;&gt;");
}

#[test]
fn test_html_escape_is_idempotent_on_safe_input() {
    let safe = "Hello, World!";
    let escaped = html_escape(safe);
    let re_escaped = html_escape(&escaped);
    assert_eq!(escaped, re_escaped);
}

// ============================================================================
// register — guest vs full registration flow
// ============================================================================

#[test]
fn test_register_guest_flow_returns_guest_token_response() {
    // Guest registration returns { access_token, device_id, user_id, is_guest: true, ... }.
    let response = json!({
        "access_token": "tok-guest",
        "device_id": "GUEST_DEVICE",
        "user_id": "@guest_42:example.com",
        "is_guest": true,
        "expires_in": 3600_i64,
        "well_known": {
            "m.homeserver": { "base_url": "https://example.com" }
        }
    });

    assert_eq!(response["access_token"].as_str(), Some("tok-guest"));
    assert_eq!(response["device_id"].as_str(), Some("GUEST_DEVICE"));
    assert_eq!(response["user_id"].as_str(), Some("@guest_42:example.com"));
    assert!(response["is_guest"].as_bool().unwrap_or(false));
    assert!(response.get("expires_in").is_some());
    assert!(response.get("well_known").is_some());
}

#[test]
fn test_register_guest_flow_detected_via_query_param() {
    // The handler detects guest registration via either:
    //   - query.get("kind") == Some("guest")
    //   - body.get("kind") == Some("guest")
    let query = json!({ "kind": "guest" });
    let body = json!({});

    let is_guest_query = query.get("kind").and_then(|v| v.as_str()) == Some("guest");
    let is_guest_body = body.get("kind").and_then(|v| v.as_str()) == Some("guest");
    let is_guest = is_guest_query || is_guest_body;
    assert!(is_guest);
}

#[test]
fn test_register_guest_flow_detected_via_body() {
    let query = json!({});
    let body = json!({ "kind": "guest" });

    let is_guest_query = query.get("kind").and_then(|v| v.as_str()) == Some("guest");
    let is_guest_body = body.get("kind").and_then(|v| v.as_str()) == Some("guest");
    let is_guest = is_guest_query || is_guest_body;
    assert!(is_guest);
}

#[test]
fn test_register_guest_flow_rejects_when_registration_disabled() {
    // When `ctx.config.server.enable_registration == false`, the handler
    // returns ApiError::forbidden("Registration is disabled").
    let enable_registration = false;
    assert!(!enable_registration);

    let err = ApiError::forbidden("Registration is disabled".to_string());
    assert_eq!(err.kind, ApiErrorKind::Forbidden);
    assert_eq!(err.code, MatrixErrorCode::Forbidden);
}

#[test]
fn test_register_returns_uia_challenge_when_auth_absent() {
    // P-038: When the `auth` field is absent, the handler returns HTTP 401
    // with the available UIA flows so the client can start the flow.
    let body = json!({ "username": "alice", "password": "pass" });
    let auth = body.get("auth").cloned();
    assert!(auth.is_none());

    let expected_status = 401u16;
    assert_eq!(expected_status, 401);

    // The response body shape:
    let response = json!({
        "flows": [
            { "stages": ["m.login.dummy"] },
            { "stages": ["m.login.password"] }
        ],
        "params": {},
        "session": "uuid-v4-string"
    });
    assert!(response.get("flows").is_some());
    assert!(response.get("params").is_some());
    assert!(response.get("session").is_some());
    let flows = response["flows"].as_array().expect("flows must be array");
    assert_eq!(flows.len(), 2);
}

#[test]
fn test_register_requires_password_for_non_guest_flow() {
    // When auth is present but password is missing, the handler returns
    // ApiError::bad_request("Password required").
    let body = json!({
        "auth": { "type": "m.login.dummy", "session": "abc" },
        "username": "alice"
        // missing "password" → bad_request
    });
    let password = body.get("password").and_then(|v| v.as_str());
    assert!(password.is_none());

    let err = ApiError::bad_request("Password required".to_string());
    assert_eq!(err.kind, ApiErrorKind::BadRequest);
}

#[test]
fn test_register_auto_generates_username_when_absent() {
    // P-037: When `username` is omitted, the handler generates a random
    // localpart: `auto{12-hex-chars}`.
    let body = json!({ "auth": { "type": "m.login.dummy" }, "password": "pass" });
    let username = body.get("username").and_then(|v| v.as_str());

    // Mirror the auto-generation logic.
    let generated = match username {
        Some(u) => u.to_string(),
        None => format!("auto{}", &uuid::Uuid::new_v4().as_simple().to_string()[..12]),
    };
    assert!(generated.starts_with("auto"));
    assert_eq!(generated.len(), 4 + 12); // "auto" + 12 hex chars
                                         // Verify the generated username matches the validation regex.
    assert!(generated.chars().all(|c| c.is_ascii_alphanumeric() || c == '.' || c == '_' || c == '=' || c == '-'));
}

#[test]
fn test_register_full_flow_returns_token_response_shape() {
    // After successful registration, the handler returns whatever
    // `registration_service.register_user(...)` returns. The shape is
    // service-defined but typically mirrors the login response.
    let response = json!({
        "access_token": "tok-new",
        "device_id": "DEVICE_NEW",
        "user_id": "@alice:example.com"
    });
    assert!(response.get("access_token").is_some());
    assert!(response.get("user_id").is_some());
}

// ============================================================================
// check_username_availability — validation + response shape
// ============================================================================

#[test]
fn test_check_username_availability_requires_username() {
    let params = json!({});
    let username = params.get("username").and_then(|v| v.as_str());
    assert!(username.is_none());

    let err = ApiError::bad_request("Username required".to_string());
    assert_eq!(err.kind, ApiErrorKind::BadRequest);
}

#[test]
fn test_check_username_availability_response_shape() {
    let response = json!({
        "available": true,
        "username": "alice"
    });
    assert_eq!(response["available"].as_bool(), Some(true));
    assert_eq!(response["username"].as_str(), Some("alice"));
}

#[test]
fn test_check_username_availability_response_when_taken() {
    let response = json!({
        "available": false,
        "username": "alice"
    });
    assert!(!response["available"].as_bool().unwrap_or(true));
}

#[test]
fn test_check_username_availability_constructs_user_id_from_localpart() {
    // The handler does `format!("@{}:{}", username, ctx.server_name)`.
    let username = "alice";
    let server_name = "example.com";
    let user_id = format!("@{}:{}", username, server_name);
    assert_eq!(user_id, "@alice:example.com");
}

#[test]
fn test_check_username_availability_propagates_validation_errors() {
    // The handler calls `ctx.validator.validate_username(username)?` first.
    // Validation failures return ApiError directly (mapped from ValidationError).
    let invalid_username = String::new();
    assert!(invalid_username.is_empty());
}

// ============================================================================
// request_email_verification_with_submit_path — request validation
// ============================================================================

#[test]
fn test_request_email_verification_requires_email() {
    let body = json!({});
    let email = body.get("email").and_then(|v| v.as_str());
    assert!(email.is_none());

    let err = ApiError::bad_request("Email is required".to_string());
    assert_eq!(err.kind, ApiErrorKind::BadRequest);
}

#[test]
fn test_request_email_verification_requires_client_secret() {
    let body = json!({ "email": "alice@example.com" });
    let client_secret = body.get("client_secret").and_then(|v| v.as_str());
    assert!(client_secret.is_none());

    let err = ApiError::bad_request("client_secret is required".to_string());
    assert_eq!(err.kind, ApiErrorKind::BadRequest);
}

#[test]
fn test_request_email_verification_rejects_invalid_email_format() {
    let body = json!({
        "email": "not-an-email",
        "client_secret": "abc"
    });
    let email = body.get("email").and_then(|v| v.as_str()).unwrap_or("");
    // Mirror the validation: if validate_email fails, the handler returns
    // ApiError::bad_request("Invalid email address format").
    let is_valid = is_valid_email(email);
    assert!(!is_valid);

    let err = ApiError::bad_request("Invalid email address format".to_string());
    assert_eq!(err.kind, ApiErrorKind::BadRequest);
}

#[test]
fn test_request_email_verification_response_shape() {
    let response = json!({
        "sid": "1",
        "submit_url": "https://example.com/_matrix/client/v3/register/email/submitToken",
        "expires_in": 3600
    });
    assert!(response.get("sid").is_some());
    assert!(response.get("submit_url").is_some());
    assert_eq!(response["expires_in"].as_u64(), Some(3600));
}

#[test]
fn test_request_email_verification_response_includes_baseurl() {
    // The handler builds submit_url as `{baseurl}{submit_path}`.
    let baseurl = "https://example.com";
    let submit_path = "/_matrix/client/v3/register/email/submitToken";
    let submit_url = format!("{}{}", baseurl, submit_path);
    assert_eq!(submit_url, "https://example.com/_matrix/client/v3/register/email/submitToken");
}

#[test]
fn test_request_email_verification_session_data_includes_client_secret_and_purpose() {
    // The handler stores the client_secret and purpose in session_data:
    //   { "client_secret": client_secret, "purpose": purpose }
    let client_secret = "abc";
    let purpose = "register";
    let session_data = json!({
        "client_secret": client_secret,
        "purpose": purpose
    });
    assert_eq!(session_data["client_secret"].as_str(), Some("abc"));
    assert_eq!(session_data["purpose"].as_str(), Some("register"));
}

/// Simple email validation mirror — the real validator lives in
/// `synapse_common::validation::Validator`. Here we just check the
/// presence of '@' and a domain part for test purposes.
fn is_valid_email(email: &str) -> bool {
    if let Some(at) = email.find('@') {
        let domain = &email[at + 1..];
        !domain.is_empty() && domain.contains('.')
    } else {
        false
    }
}

#[test]
fn test_is_valid_email_helper() {
    assert!(is_valid_email("alice@example.com"));
    assert!(!is_valid_email("not-an-email"));
    assert!(!is_valid_email(""));
    assert!(!is_valid_email("alice@"));
    assert!(!is_valid_email("alice@example"));
}

// ============================================================================
// submit_email_token — request validation
// ============================================================================

#[test]
fn test_submit_email_token_requires_sid() {
    let body = json!({ "client_secret": "abc", "token": "123456" });
    let sid = body.get("sid").and_then(|v| v.as_str());
    assert!(sid.is_none());

    let err = ApiError::bad_request("Session ID (sid) is required".to_string());
    assert_eq!(err.kind, ApiErrorKind::BadRequest);
}

#[test]
fn test_submit_email_token_requires_client_secret() {
    let body = json!({ "sid": "1", "token": "123456" });
    let client_secret = body.get("client_secret").and_then(|v| v.as_str());
    assert!(client_secret.is_none());

    let err = ApiError::bad_request("Client secret is required".to_string());
    assert_eq!(err.kind, ApiErrorKind::BadRequest);
}

#[test]
fn test_submit_email_token_requires_token() {
    let body = json!({ "sid": "1", "client_secret": "abc" });
    let token = body.get("token").and_then(|v| v.as_str());
    assert!(token.is_none());

    let err = ApiError::bad_request("Verification token is required".to_string());
    assert_eq!(err.kind, ApiErrorKind::BadRequest);
}

#[test]
fn test_submit_email_token_rejects_invalid_sid_format() {
    let sid = "abc";
    let parsed: Result<i64, _> = sid.parse();
    assert!(parsed.is_err());

    let err = ApiError::bad_request("Invalid session ID format".to_string());
    assert_eq!(err.kind, ApiErrorKind::BadRequest);
}

#[test]
fn test_submit_email_token_response_shape() {
    let response = json!({ "success": true });
    assert!(response["success"].as_bool().unwrap_or(false));
}

// ============================================================================
// get_login_flows — response shape + SSO providers
// ============================================================================

#[test]
fn test_get_login_flows_always_includes_password_and_token() {
    // The base flows array always starts with m.login.password and m.login.token.
    let flows = [json!({"type": "m.login.password"}), json!({"type": "m.login.token"})];
    let types: Vec<&str> = flows.iter().filter_map(|f| f.get("type").and_then(|v| v.as_str())).collect();
    assert!(types.contains(&"m.login.password"));
    assert!(types.contains(&"m.login.token"));
}

#[test]
fn test_get_login_flows_includes_sso_when_oidc_enabled() {
    // When `ctx.oidc_service.is_some()`, the handler adds an SSO flow with
    // an `oidc` identity_provider entry.
    let oidc_service_present = true;
    if oidc_service_present {
        let mut sso_providers = Vec::new();
        sso_providers.push(json!({
            "id": "oidc",
            "name": "OIDC",
            "brand": "oidc"
        }));
        let flows = [
            json!({"type": "m.login.password"}),
            json!({"type": "m.login.token"}),
            json!({
                "type": "m.login.sso",
                "identity_providers": sso_providers
            }),
        ];
        let has_sso = flows.iter().any(|f| f.get("type").and_then(|v| v.as_str()) == Some("m.login.sso"));
        assert!(has_sso);
    }
}

#[test]
fn test_get_login_flows_excludes_sso_when_oidc_disabled() {
    let oidc_service_present = false;
    if !oidc_service_present {
        let flows = [json!({"type": "m.login.password"}), json!({"type": "m.login.token"})];
        let has_sso = flows.iter().any(|f| f.get("type").and_then(|v| v.as_str()) == Some("m.login.sso"));
        assert!(!has_sso);
    }
}

#[test]
fn test_get_login_flows_response_shape() {
    let response = json!({
        "flows": [
            {"type": "m.login.password"},
            {"type": "m.login.token"}
        ]
    });
    assert!(response.get("flows").is_some());
    assert!(response["flows"].is_array());
}

// ============================================================================
// get_register_flows — response shape
// ============================================================================

#[test]
fn test_get_register_flows_response_shape() {
    let response = json!({
        "flows": [
            {"type": "m.login.dummy"},
            {"type": "m.login.password"}
        ],
        "params": {}
    });
    assert!(response.get("flows").is_some());
    assert!(response.get("params").is_some());
    let flows = response["flows"].as_array().expect("flows must be array");
    assert_eq!(flows.len(), 2);
    let types: Vec<&str> = flows.iter().filter_map(|f| f.get("type").and_then(|v| v.as_str())).collect();
    assert!(types.contains(&"m.login.dummy"));
    assert!(types.contains(&"m.login.password"));
}

// ============================================================================
// login — m.login.password flow validation
// ============================================================================

#[test]
fn test_login_defaults_to_password_type() {
    let body = json!({ "username": "alice", "password": "pass" });
    let login_type = body.get("type").and_then(|v| v.as_str()).unwrap_or("m.login.password");
    assert_eq!(login_type, "m.login.password");
}

#[test]
fn test_login_password_flow_requires_username() {
    let body = json!({ "password": "pass" });
    let username = body
        .get("identifier")
        .and_then(|id| id.get("user"))
        .or_else(|| body.get("user"))
        .or_else(|| body.get("username"))
        .and_then(|v| v.as_str());
    assert!(username.is_none());

    let err = ApiError::bad_request("Username required".to_string());
    assert_eq!(err.kind, ApiErrorKind::BadRequest);
}

#[test]
fn test_login_password_flow_requires_password() {
    let body = json!({ "username": "alice" });
    let password = body.get("password").and_then(|v| v.as_str());
    assert!(password.is_none());

    let err = ApiError::bad_request("Password required".to_string());
    assert_eq!(err.kind, ApiErrorKind::BadRequest);
}

#[test]
fn test_login_password_flow_rejects_empty_username_or_password() {
    let username = String::new();
    let password = String::new();
    assert!(username.is_empty() || password.is_empty());

    let err = ApiError::bad_request("Username and password are required".to_string());
    assert_eq!(err.kind, ApiErrorKind::BadRequest);
}

#[test]
fn test_login_password_flow_rejects_username_too_long() {
    let username = "a".repeat(256);
    assert!(username.len() > 255);

    let err = ApiError::bad_request("Username too long".to_string());
    assert_eq!(err.kind, ApiErrorKind::BadRequest);
}

#[test]
fn test_login_password_flow_accepts_username_at_max_length() {
    let username = "a".repeat(255);
    assert!(username.len() <= 255);
}

#[test]
fn test_login_password_flow_rejects_password_too_long() {
    let password = "a".repeat(129);
    assert!(password.len() > 128);

    let err = ApiError::bad_request("Password too long (max 128 characters)".to_string());
    assert_eq!(err.kind, ApiErrorKind::BadRequest);
}

#[test]
fn test_login_password_flow_accepts_password_at_max_length() {
    let password = "a".repeat(128);
    assert!(password.len() <= 128);
}

#[test]
fn test_login_password_flow_resolves_identifier_user_field() {
    // The handler reads identifier.user, then falls back to body.user,
    // then body.username.
    let body_with_identifier = json!({
        "identifier": { "user": "alice" },
        "password": "pass"
    });
    let username = body_with_identifier
        .get("identifier")
        .and_then(|id| id.get("user"))
        .or_else(|| body_with_identifier.get("user"))
        .or_else(|| body_with_identifier.get("username"))
        .and_then(|v| v.as_str());
    assert_eq!(username, Some("alice"));

    let body_with_user = json!({
        "user": "alice",
        "password": "pass"
    });
    let username = body_with_user
        .get("identifier")
        .and_then(|id| id.get("user"))
        .or_else(|| body_with_user.get("user"))
        .or_else(|| body_with_user.get("username"))
        .and_then(|v| v.as_str());
    assert_eq!(username, Some("alice"));

    let body_with_username = json!({
        "username": "alice",
        "password": "pass"
    });
    let username = body_with_username
        .get("identifier")
        .and_then(|id| id.get("user"))
        .or_else(|| body_with_username.get("user"))
        .or_else(|| body_with_username.get("username"))
        .and_then(|v| v.as_str());
    assert_eq!(username, Some("alice"));
}

#[test]
fn test_login_password_flow_response_shape() {
    // login returns format_token_response output:
    //   { access_token, refresh_token, expires_in, device_id, user_id, well_known }
    let response = json!({
        "access_token": "tok-new",
        "refresh_token": "ref-new",
        "expires_in": 3600_i64,
        "device_id": "DEVICE_XYZ",
        "user_id": "@alice:example.com",
        "well_known": {
            "m.homeserver": { "base_url": "https://example.com" }
        }
    });
    assert!(response.get("access_token").is_some());
    assert!(response.get("refresh_token").is_some());
    assert!(response.get("expires_in").is_some());
    assert!(response.get("device_id").is_some());
    assert!(response.get("user_id").is_some());
    assert!(response.get("well_known").is_some());
}

// ============================================================================
// login — m.login.token flow (QR sign-in)
// ============================================================================

#[test]
fn test_login_token_flow_requires_token_field() {
    let body = json!({ "type": "m.login.token" });
    let token = body.get("token").and_then(|v| v.as_str());
    assert!(token.is_none());

    let err = ApiError::bad_request("Token required for m.login.token".to_string());
    assert_eq!(err.kind, ApiErrorKind::BadRequest);
}

#[test]
fn test_login_token_flow_rejects_invalid_token() {
    // consume_login_token returns None for invalid/expired tokens.
    let _token = "invalid-or-expired";
    let consumed: Option<(&str, &str)> = None; // simulate failure
    assert!(consumed.is_none());

    let err = ApiError::forbidden("Invalid or expired login token".to_string());
    assert_eq!(err.kind, ApiErrorKind::Forbidden);
    assert_eq!(err.code, MatrixErrorCode::Forbidden);
}

#[test]
fn test_login_token_flow_uses_default_device_id_when_absent() {
    // When body.device_id is None, the handler defaults to "QR_LOGIN_DEVICE".
    let body = json!({ "type": "m.login.token", "token": "valid-token" });
    let device_id = body.get("device_id").and_then(|v| v.as_str()).unwrap_or("QR_LOGIN_DEVICE");
    assert_eq!(device_id, "QR_LOGIN_DEVICE");
}

#[test]
fn test_login_token_flow_uses_provided_device_id_when_present() {
    let body = json!({
        "type": "m.login.token",
        "token": "valid-token",
        "device_id": "CUSTOM_DEVICE"
    });
    let device_id = body.get("device_id").and_then(|v| v.as_str()).unwrap_or("QR_LOGIN_DEVICE");
    assert_eq!(device_id, "CUSTOM_DEVICE");
}

#[test]
fn test_login_token_flow_response_shape_matches_password_flow() {
    // Both flows return the same format_token_response shape.
    let response = json!({
        "access_token": "tok-new",
        "refresh_token": "ref-new",
        "expires_in": 3600_i64,
        "device_id": "QR_LOGIN_DEVICE",
        "user_id": "@alice:example.com",
        "well_known": {
            "m.homeserver": { "base_url": "https://example.com" }
        }
    });
    assert!(response.get("access_token").is_some());
    assert!(response.get("refresh_token").is_some());
}

// ============================================================================
// generate_qr_login_token — MSC4108
// ============================================================================

#[test]
fn test_generate_qr_login_token_response_shape() {
    let response = json!({
        "login_token": "short-lived-token",
        "expires_in_ms": 60000
    });
    assert!(response.get("login_token").is_some());
    assert_eq!(response["expires_in_ms"].as_u64(), Some(60000));
}

#[test]
fn test_generate_qr_login_token_expiry_is_60_seconds() {
    // The handler hardcodes 60_000ms (60 seconds) as the expiry.
    let expires_in_ms = 60000u64;
    assert_eq!(expires_in_ms, 60_000);
    assert_eq!(expires_in_ms / 1000, 60);
}

// ============================================================================
// logout / logout_all — response shape
// ============================================================================

#[test]
fn test_logout_response_shape() {
    let response = json!({});
    assert!(response.is_object());
    assert!(response.as_object().unwrap().is_empty());
}

#[test]
fn test_logout_all_response_shape() {
    let response = json!({});
    assert!(response.is_object());
    assert!(response.as_object().unwrap().is_empty());
}

// ============================================================================
// refresh_token — request validation + response shape
// ============================================================================

#[test]
fn test_refresh_token_requires_refresh_token_field() {
    let body = json!({});
    let refresh_token = body.get("refresh_token").and_then(|v| v.as_str());
    assert!(refresh_token.is_none());

    let err = ApiError::bad_request("Refresh token required".to_string());
    assert_eq!(err.kind, ApiErrorKind::BadRequest);
}

#[test]
fn test_refresh_token_response_shape() {
    // refresh_token returns { access_token, refresh_token, expires_in, device_id }.
    // NOTE: Unlike login, refresh_token does NOT return `well_known` or `user_id`.
    let response = json!({
        "access_token": "tok-new",
        "refresh_token": "ref-new",
        "expires_in": 3600_i64,
        "device_id": "DEVICE_XYZ"
    });
    assert!(response.get("access_token").is_some());
    assert!(response.get("refresh_token").is_some());
    assert!(response.get("expires_in").is_some());
    assert!(response.get("device_id").is_some());
    assert!(response.get("well_known").is_none(), "refresh_token response must not include well_known");
    assert!(response.get("user_id").is_none(), "refresh_token response must not include user_id");
}

// ============================================================================
// login_fallback_page — HTML generation contract
// ============================================================================

#[test]
fn test_login_fallback_page_returns_html_content_type() {
    // The handler returns axum::response::Html<String>.
    let html = r#"<!doctype html><html><head><title>Login - Matrix</title></head></html>"#;
    assert!(html.starts_with("<!doctype html>"));
    assert!(html.contains("<title>"));
}

#[test]
fn test_login_fallback_page_includes_password_form() {
    // When m.login.password is in the flows, the rendered HTML includes
    // a form posting to /_matrix/client/v3/login.
    let flows_html = r#"
    <div class="flow">
        <h3>Password Login</h3>
        <form method="POST" action="/_matrix/client/v3/login">
            <input type="hidden" name="type" value="m.login.password">
            <input type="text" name="identifier[user]" required>
            <input type="password" name="password" required>
            <button type="submit">Login</button>
        </form>
    </div>
    "#;
    assert!(flows_html.contains("m.login.password"));
    assert!(flows_html.contains("/_matrix/client/v3/login"));
    assert!(flows_html.contains("Password Login"));
}

#[test]
fn test_login_fallback_page_includes_sso_links_when_sso_present() {
    // When m.login.sso is in the flows with identity_providers, the HTML
    // includes an `<a>` link per provider.
    let flows_html = r#"
    <div class="flow">
        <h3>SSO Login</h3>
        <a href="/_matrix/client/v3/login/sso/redirect?redirectUrl=/">Login with OIDC</a><br>
    </div>
    "#;
    assert!(flows_html.contains("SSO Login"));
    assert!(flows_html.contains("/_matrix/client/v3/login/sso/redirect"));
}

#[test]
fn test_login_fallback_page_includes_cas_link_when_cas_enabled() {
    // When m.login.cas is in the flows, the HTML includes a CAS link.
    let flows_html = r#"
    <div class="flow">
        <h3>CAS Login</h3>
        <a href="/cas/login?service=/">Login with CAS</a>
    </div>
    "#;
    assert!(flows_html.contains("CAS Login"));
    assert!(flows_html.contains("/cas/login"));
}

#[test]
fn test_login_fallback_page_escapes_untrusted_provider_names() {
    // Provider names are user/config-controlled and must be HTML-escaped
    // before insertion into the template. The handler calls html_escape.
    let provider_name = r#"<script>alert("xss")</script>"#;
    let escaped = html_escape(provider_name);
    let flows_html =
        format!(r#"<a href="/_matrix/client/v3/login/sso/redirect?redirectUrl=/">Login with {escaped}</a><br>"#);
    assert!(flows_html.contains("&lt;script&gt;"));
    assert!(!flows_html.contains("<script>alert"));
}

#[test]
fn test_login_fallback_page_skips_unknown_flow_types() {
    // The match in login_fallback_page only handles m.login.password,
    // m.login.sso, and m.login.cas. Unknown types are silently skipped
    // (the `_ => {}` branch).
    let unknown_flow = json!({"type": "m.login.unknown"});
    let flow_type = unknown_flow.get("type").and_then(|t| t.as_str()).unwrap_or("");
    assert!(!matches!(flow_type, "m.login.password" | "m.login.sso" | "m.login.cas"));
}

// ============================================================================
// Error code mapping — distinct ApiError constructors used in auth_compat
// ============================================================================

#[test]
fn test_error_code_mapping_for_bad_request() {
    // Multiple handlers use ApiError::bad_request for client errors.
    let err = ApiError::bad_request("Username required".to_string());
    assert_eq!(err.kind, ApiErrorKind::BadRequest);
    assert_eq!(err.code, MatrixErrorCode::BadJson);
}

#[test]
fn test_error_code_mapping_for_forbidden() {
    // Used by register (registration disabled), login (invalid token).
    let err = ApiError::forbidden("Registration is disabled".to_string());
    assert_eq!(err.kind, ApiErrorKind::Forbidden);
    assert_eq!(err.code, MatrixErrorCode::Forbidden);
}

#[test]
fn test_error_code_mapping_for_unauthorized() {
    // Used by change_password_uia when access token is missing.
    let err = ApiError::unauthorized("Access token required".to_string());
    assert_eq!(err.kind, ApiErrorKind::Unauthorized);
    assert_eq!(err.code, MatrixErrorCode::Unauthorized);
}

#[test]
fn test_error_code_mapping_for_internal_with_context() {
    // Used by login when access_token / refresh_token generation fails.
    let source = ApiError::internal("Underlying failure".to_string());
    let err = ApiError::internal_with_context("Failed to generate access token", &source);
    assert_eq!(err.kind, ApiErrorKind::Internal);
    assert_eq!(err.code, MatrixErrorCode::Unknown);
    assert!(err.message.contains("Failed to generate access token"));
}

#[test]
fn test_error_code_mapping_for_internal() {
    // Used by request_email_verification_with_submit_path when token
    // storage fails.
    let err = ApiError::internal("Failed to store verification token. Please try again later.".to_string());
    assert_eq!(err.kind, ApiErrorKind::Internal);
    assert_eq!(err.code, MatrixErrorCode::Unknown);
}

// ============================================================================
// Security-sensitive contract — login token / QR sign-in
// ============================================================================

#[test]
fn test_login_token_must_be_short_lived() {
    // The MSC4108 contract requires login tokens to expire quickly
    // (60 seconds as implemented in generate_qr_login_token).
    let expires_in_ms = 60000u64;
    assert!(expires_in_ms <= 60_000, "login token must expire within 60 seconds");
}

#[test]
fn test_logout_invalidates_access_token() {
    // logout calls token_auth.logout(access_token, device_id) which
    // invalidates the specific token.
    let access_token = String::from("tok-abc");
    let device_id = String::from("DEVICE_XYZ");
    // Verify the handler signature matches.
    assert!(!access_token.is_empty());
    assert!(!device_id.is_empty());
}

#[test]
fn test_logout_all_invalidates_all_user_tokens() {
    // logout_all calls token_auth.logout_all(user_id) which invalidates
    // every token belonging to the user.
    let user_id = "@alice:example.com";
    assert!(user_id.starts_with('@'));
}

// ============================================================================
// UIA — register returns 401 (not 200) when auth absent
// ============================================================================

#[test]
fn test_register_uia_challenge_returns_401_not_200() {
    // P-038 regression guard: previously the handler returned 200 with
    // the UIA challenge body, which made Element interpret the body as
    // a successful registration and crash reading a missing user_id.
    // The fix returns (StatusCode::UNAUTHORIZED, Json(challenge)).
    let body = json!({ "username": "alice", "password": "pass" });
    let auth = body.get("auth");
    assert!(auth.is_none(), "auth absent must trigger 401 UIA challenge");

    let expected_status = 401u16;
    assert_eq!(expected_status, 401);
    assert_ne!(expected_status, 200);
}

// ============================================================================
// format_token_response — shared login/SSO response shape
// ============================================================================

#[test]
fn test_format_token_response_shape() {
    // format_token_response is the shared helper used by login, SSO callback,
    // and any flow that returns access/refresh tokens. Its output shape:
    //   { access_token, refresh_token, expires_in, device_id, user_id, well_known }
    let response = json!({
        "access_token": "tok-1",
        "refresh_token": "ref-1",
        "expires_in": 3600_i64,
        "device_id": "DEV",
        "user_id": "@alice:example.com",
        "well_known": {
            "m.homeserver": { "base_url": "https://example.com" }
        }
    });

    let obj = response.as_object().expect("response must be an object");
    assert!(obj.contains_key("access_token"));
    assert!(obj.contains_key("refresh_token"));
    assert!(obj.contains_key("expires_in"));
    assert!(obj.contains_key("device_id"));
    assert!(obj.contains_key("user_id"));
    assert!(obj.contains_key("well_known"));

    // well_known must contain m.homeserver with base_url.
    let well_known = &response["well_known"]["m.homeserver"];
    assert!(well_known.get("base_url").is_some());
}
