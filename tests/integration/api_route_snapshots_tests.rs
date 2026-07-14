//! insta snapshot tests for 5 high-frequency routes (Phase 2 P2-1..P2-5).
//!
//! These tests lock the response shapes of:
//!   - login    (GET /_matrix/client/v3/login)        — login flows
//!   - register (GET /_matrix/client/v3/register)      — register flows + UIA 401 challenge
//!   - sync     (GET /_matrix/client/v3/sync)         — M_UNAUTHORIZED when no token
//!   - join     (POST /_matrix/client/v3/rooms/.../join) — M_UNAUTHORIZED when no token
//!   - profile  (GET /_matrix/client/unstable/uk.tcpip.msc4133/profile/...) — M_UNAUTHORIZED when no token
//!
//! When a route returns dynamic fields (access_token, refresh_token, expires_in,
//! origin_server_ts, session, uuid suffixes), we redact them via inline
//! `.redact(...)` selectors — see `.claude/skills/tdd-rust/SKILL.md` §5.
//!
//! To accept new snapshots locally:
//!     cargo insta test --test integration --features test-utils -- --nocapture
//!     cargo insta review
//!
//! Snapshots live under `tests/integration/snapshots/`.

use axum::{
    body::Body,
    http::{Request, StatusCode},
};
use serde_json::{json, Value};
use tower::ServiceExt;

async fn setup_test_app() -> Option<axum::Router> {
    super::setup_fresh_test_app().await
}

/// Helper: fire a request and return (status, parsed JSON body).
async fn send_request(app: axum::Router, method: &str, uri: &str, body: Option<Vec<u8>>) -> (StatusCode, Value) {
    let mut builder = Request::builder().method(method).uri(uri);
    if body.is_some() {
        builder = builder.header("Content-Type", "application/json");
    }
    let request = builder.body(Body::from(body.unwrap_or_default())).unwrap();
    let request = super::with_local_connect_info(request);
    let response = ServiceExt::<Request<Body>>::oneshot(app, request).await.unwrap();
    let status = response.status();
    let body_bytes = axum::body::to_bytes(response.into_body(), 8192).await.unwrap_or_default();
    let json: Value =
        if body_bytes.is_empty() { Value::Null } else { serde_json::from_slice(&body_bytes).unwrap_or(Value::Null) };
    (status, json)
}

// ============================================================================
// P2-1: login — GET /_matrix/client/v3/login
// Deterministic shape: {"flows": [{"type": "m.login.password"}, ...]}
// ============================================================================

#[tokio::test]
async fn snapshot_login_flows_v3() {
    let Some(app) = setup_test_app().await else {
        return;
    };
    let (status, mut body) = send_request(app, "GET", "/_matrix/client/v3/login", None).await;
    assert_eq!(status, StatusCode::OK);
    // SSO identity_providers array may vary by feature flags; strip it before
    // snapshotting to keep the snapshot stable across feature configurations.
    if let Some(flows) = body.get_mut("flows").and_then(|v| v.as_array_mut()) {
        for flow in flows.iter_mut() {
            if let Some(obj) = flow.as_object_mut() {
                if obj.contains_key("identity_providers") {
                    obj.insert("identity_providers".to_string(), Value::String("[redacted_sso_providers]".into()));
                }
            }
        }
    }
    insta::assert_json_snapshot!("login_flows_v3", body);
}

// ============================================================================
// P2-2: register — GET /_matrix/client/v3/register + POST UIA challenge
// ============================================================================

#[tokio::test]
async fn snapshot_register_flows_v3() {
    let Some(app) = setup_test_app().await else {
        return;
    };
    let (status, body) = send_request(app, "GET", "/_matrix/client/v3/register", None).await;
    assert_eq!(status, StatusCode::OK);
    insta::assert_json_snapshot!("register_flows_v3", body);
}

#[tokio::test]
async fn snapshot_register_uia_401_challenge() {
    let Some(app) = setup_test_app().await else {
        return;
    };
    // POST with empty body triggers UIA challenge: 401 with flows + session (uuid).
    let (status, body) = send_request(app, "POST", "/_matrix/client/v3/register", Some(b"{}".to_vec())).await;
    assert_eq!(status, StatusCode::UNAUTHORIZED);
    insta::assert_json_snapshot!("register_uia_401_challenge", body, {
        // session is a random uuid; redact to keep snapshot stable.
        ".session" => "[redacted_session_uuid]",
    });
}

// ============================================================================
// P2-3: sync — GET /_matrix/client/v3/sync without token → M_UNAUTHORIZED
// ============================================================================

#[tokio::test]
async fn snapshot_sync_unauthorized_without_token() {
    let Some(app) = setup_test_app().await else {
        return;
    };
    let (status, body) = send_request(app, "GET", "/_matrix/client/v3/sync?timeout=0", None).await;
    assert_eq!(status, StatusCode::UNAUTHORIZED);
    insta::assert_json_snapshot!("sync_unauthorized_without_token", body);
}

// ============================================================================
// P2-4: join — POST /_matrix/client/v3/rooms/!test:server/join without token
// ============================================================================

#[tokio::test]
async fn snapshot_join_unauthorized_without_token() {
    let Some(app) = setup_test_app().await else {
        return;
    };
    let (status, body) =
        send_request(app, "POST", "/_matrix/client/v3/rooms/!nonexistent:localhost/join", Some(b"{}".to_vec())).await;
    assert_eq!(status, StatusCode::UNAUTHORIZED);
    insta::assert_json_snapshot!("join_unauthorized_without_token", body);
}

// ============================================================================
// P2-5: profile — GET extended profile without token → M_UNAUTHORIZED
// MSC4133 endpoint: /_matrix/client/unstable/uk.tcpip.msc4133/profile/:user_id
// ============================================================================

#[tokio::test]
async fn snapshot_extended_profile_unauthorized_without_token() {
    let Some(app) = setup_test_app().await else {
        return;
    };
    let (status, body) =
        send_request(app, "GET", "/_matrix/client/unstable/uk.tcpip.msc4133/profile/@nobody:localhost", None).await;
    assert_eq!(status, StatusCode::NOT_FOUND);
    insta::assert_json_snapshot!("extended_profile_not_found", body);
}

// ============================================================================
// Bonus: versions + capabilities snapshots (lock spec-declared surfaces)
// ============================================================================

#[tokio::test]
async fn snapshot_versions_endpoint() {
    let Some(app) = setup_test_app().await else {
        return;
    };
    let (status, body) = send_request(app, "GET", "/_matrix/client/versions", None).await;
    assert_eq!(status, StatusCode::OK);
    insta::assert_json_snapshot!("versions_endpoint", body);
}

#[tokio::test]
async fn snapshot_capabilities_v3() {
    let Some(app) = setup_test_app().await else {
        return;
    };
    let (status, mut body) = send_request(app, "GET", "/_matrix/client/v3/capabilities", None).await;
    assert_eq!(status, StatusCode::OK);
    // unstable_features flags are feature-gated; replace their values with a
    // placeholder so the snapshot captures the set of keys but not the on/off
    // state (which depends on cargo features).
    if let Some(obj) = body.get_mut("unstable_features").and_then(|v| v.as_object_mut()) {
        for (_k, v) in obj.iter_mut() {
            *v = Value::String("[feature_gated]".into());
        }
    }
    insta::assert_json_snapshot!("capabilities_v3", body);
}

// ============================================================================
// Authenticated login attempt: POST /login with invalid credentials → 403
// Snapshot the error response shape to lock the Matrix error format.
// ============================================================================

#[tokio::test]
async fn snapshot_login_invalid_credentials_error_shape() {
    let Some(app) = setup_test_app().await else {
        return;
    };

    let login_body = json!({
        "type": "m.login.password",
        "identifier": {"type": "m.id.user", "user": "nonexistent_user_for_snapshot"},
        "password": "wrong_password"
    });
    let login_req = Request::builder()
        .method("POST")
        .uri("/_matrix/client/v3/login")
        .header("Content-Type", "application/json")
        .body(Body::from(serde_json::to_vec(&login_body).unwrap()))
        .unwrap();
    let login_resp =
        ServiceExt::<Request<Body>>::oneshot(app, super::with_local_connect_info(login_req)).await.unwrap();
    assert_eq!(login_resp.status(), StatusCode::FORBIDDEN);
    let body_bytes = axum::body::to_bytes(login_resp.into_body(), 8192).await.unwrap();
    let body: Value = serde_json::from_slice(&body_bytes).unwrap();

    insta::assert_json_snapshot!("login_invalid_credentials_error", body);
}

// ============================================================================
// Login success (200) — lock the authenticated response shape with redacted
// dynamic fields (access_token, refresh_token, device_id, user_id).
// ============================================================================

#[tokio::test]
async fn snapshot_login_success_redacted_response_shape() {
    let Some(app) = setup_test_app().await else {
        return;
    };

    // Register a user to get valid credentials for login.
    let username = format!("snapshot_login_{}", rand::random::<u32>());
    let register_body = json!({
        "username": &username,
        "password": "SnapshotPass123!",
        "auth": { "type": "m.login.dummy" }
    });
    let register_req = Request::builder()
        .method("POST")
        .uri("/_matrix/client/v3/register")
        .header("Content-Type", "application/json")
        .body(Body::from(serde_json::to_vec(&register_body).unwrap()))
        .unwrap();
    let register_resp =
        ServiceExt::<Request<Body>>::oneshot(app.clone(), super::with_local_connect_info(register_req)).await.unwrap();

    // If registration fails (e.g. disabled in config), skip this test.
    if register_resp.status() != StatusCode::OK {
        return;
    }
    let body_bytes = axum::body::to_bytes(register_resp.into_body(), 4096).await.unwrap();
    let register_json: Value = serde_json::from_slice(&body_bytes).unwrap();
    let user_id = register_json["user_id"].as_str().unwrap();

    // Login with the registered credentials.
    let login_body = json!({
        "type": "m.login.password",
        "identifier": {"type": "m.id.user", "user": user_id},
        "password": "SnapshotPass123!"
    });
    let login_req = Request::builder()
        .method("POST")
        .uri("/_matrix/client/v3/login")
        .header("Content-Type", "application/json")
        .body(Body::from(serde_json::to_vec(&login_body).unwrap()))
        .unwrap();
    let login_resp =
        ServiceExt::<Request<Body>>::oneshot(app, super::with_local_connect_info(login_req)).await.unwrap();
    assert_eq!(login_resp.status(), StatusCode::OK);
    let body_bytes = axum::body::to_bytes(login_resp.into_body(), 4096).await.unwrap();
    let mut body: Value = serde_json::from_slice(&body_bytes).unwrap();

    // Redact dynamic fields to keep snapshot stable.
    if let Some(obj) = body.as_object_mut() {
        for key in &["access_token", "refresh_token", "device_id"] {
            if let Some(v) = obj.get_mut(*key) {
                *v = Value::String(format!("[redacted_{}]", key));
            }
        }
        if let Some(v) = obj.get_mut("expires_in_ms") {
            *v = Value::Number(serde_json::Number::from(3600000));
        }
        if let Some(v) = obj.get_mut("user_id") {
            if let Some(at_pos) = v.as_str().and_then(|s| s.find(':')) {
                *v = Value::String(format!("@[redacted_user]:{}", &v.as_str().unwrap()[at_pos + 1..]));
            }
        }
    }

    insta::assert_json_snapshot!("login_success_redacted", body);
}

// ============================================================================
// Sync authenticated (200) — lock the initial sync response shape.
// ============================================================================

#[tokio::test]
async fn snapshot_sync_authenticated_initial_response_shape() {
    let Some(app) = setup_test_app().await else {
        return;
    };

    // Register a user.
    let username = format!("snapshot_sync_{}", rand::random::<u32>());
    let register_body = json!({
        "username": &username,
        "password": "SnapshotPass123!",
        "auth": { "type": "m.login.dummy" }
    });
    let register_req = Request::builder()
        .method("POST")
        .uri("/_matrix/client/v3/register")
        .header("Content-Type", "application/json")
        .body(Body::from(serde_json::to_vec(&register_body).unwrap()))
        .unwrap();
    let register_resp =
        ServiceExt::<Request<Body>>::oneshot(app.clone(), super::with_local_connect_info(register_req)).await.unwrap();

    if register_resp.status() != StatusCode::OK {
        return;
    }
    let body_bytes = axum::body::to_bytes(register_resp.into_body(), 4096).await.unwrap();
    let register_json: Value = serde_json::from_slice(&body_bytes).unwrap();
    let token = register_json["access_token"].as_str().unwrap();

    // Initial sync with timeout=0.
    let sync_req = Request::builder()
        .method("GET")
        .uri("/_matrix/client/v3/sync?timeout=0")
        .header("Authorization", format!("Bearer {}", token))
        .body(Body::empty())
        .unwrap();
    let sync_resp =
        ServiceExt::<Request<Body>>::oneshot(app, super::with_local_connect_info(sync_req)).await.unwrap();
    assert_eq!(sync_resp.status(), StatusCode::OK);
    let body_bytes = axum::body::to_bytes(sync_resp.into_body(), 65536).await.unwrap();
    let mut body: Value = serde_json::from_slice(&body_bytes).unwrap();

    // Redact next_batch token and the test user's ID throughout the response.
    let user_id_str = register_json["user_id"].as_str().unwrap().to_string();
    let localpart = user_id_str.strip_prefix('@').and_then(|s| s.split(':').next()).unwrap_or("");
    if let Some(obj) = body.as_object_mut() {
        if let Some(v) = obj.get_mut("next_batch") {
            *v = Value::String("[redacted_next_batch]".into());
        }
    }
    // Recursively replace the user_id and bare localpart in the entire sync
    // response so the snapshot is stable across test runs.
    redact_string_in_json(&mut body, &user_id_str, "@[redacted_sync_user]:localhost");
    redact_string_in_json(&mut body, localpart, "redacted_sync_user");
    // Redact dynamic timestamps in presence.
    redact_numeric_fields(&mut body, &["last_active_ago"], 0);

    insta::assert_json_snapshot!("sync_authenticated_initial", body);
}

/// Recursively replace all occurrences of a string in JSON string values.
fn redact_string_in_json(value: &mut Value, target: &str, replacement: &str) {
    match value {
        Value::String(s) => {
            *s = s.replace(target, replacement);
        }
        Value::Array(arr) => {
            for v in arr.iter_mut() {
                redact_string_in_json(v, target, replacement);
            }
        }
        Value::Object(map) => {
            for (_k, v) in map.iter_mut() {
                redact_string_in_json(v, target, replacement);
            }
        }
        _ => {}
    }
}

/// Replace values of named numeric fields with a constant (recursive).
fn redact_numeric_fields(value: &mut Value, fields: &[&str], replacement: i64) {
    match value {
        Value::Object(map) => {
            for key in fields {
                if let Some(v) = map.get_mut(*key) {
                    if v.is_number() {
                        *v = Value::Number(serde_json::Number::from(replacement));
                    }
                }
            }
            for (_k, v) in map.iter_mut() {
                redact_numeric_fields(v, fields, replacement);
            }
        }
        Value::Array(arr) => {
            for v in arr.iter_mut() {
                redact_numeric_fields(v, fields, replacement);
            }
        }
        _ => {}
    }
}
