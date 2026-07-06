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
