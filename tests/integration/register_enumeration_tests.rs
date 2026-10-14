//! `registration.inhibit_user_in_use_error` — anti-enumeration for
//! `GET /register/available` (upstream parity; the upstream default is `false`).
//!
//! The endpoint answers "is this localpart free?", which is a membership oracle
//! by construction. Upstream's flag makes it answer `available: true`
//! unconditionally. These tests drive the production router for both settings.
//!
//! What the flag does **not** do (upstream is explicit about this): a conflicting
//! localpart still fails when registration *completes*, because the conflict is
//! enforced by the unique index on `users.user_id`. This repo has no earlier
//! check either — `synapse-services/src/auth/register.rs` maps the
//! unique-violation of `create_user` to `M_USER_IN_USE` — so `POST /register`
//! needs no change and behaves identically under both settings.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use axum::body::Body;
use axum::http::{Request, StatusCode};
use serde_json::json;
use std::sync::Arc;
use synapse_rust::cache::{CacheConfig, CacheManager};
use synapse_services::ServiceContainer;
use synapse_web::routes::state::AppState;
use tower::ServiceExt;

/// Build a router with `inhibit_user_in_use_error` set explicitly.
async fn setup(inhibit: bool) -> Option<axum::Router> {
    let pool = super::get_test_pool().await?;
    let mut container = ServiceContainer::new_test_with_pool(pool).await;
    // Keep the rate limiter out of the way: these tests are about response
    // contents, not about 429s.
    super::config_mut(&mut container).rate_limit.enabled = false;
    super::config_mut(&mut container).server.inhibit_user_in_use_error = inhibit;

    let cache = Arc::new(CacheManager::new(&CacheConfig::default()));
    let state = AppState::new(container, cache);
    Some(synapse_web::create_router(state))
}

/// Register one account through the real endpoint and return its localpart, so
/// the "taken" fixture is produced by the same code path production uses.
async fn register_a_user(app: &axum::Router) -> String {
    let username = format!("taken_{}", rand::random::<u32>());
    let request = Request::builder()
        .method("POST")
        .uri("/_matrix/client/v3/register")
        .header("Content-Type", "application/json")
        .body(Body::from(
            json!({
                "username": username,
                "password": "UserTest@123",
                "device_id": "AVAILDEVICE",
                "auth": { "type": "m.login.dummy" }
            })
            .to_string(),
        ))
        .unwrap();

    let response = app.clone().oneshot(super::with_local_connect_info(request)).await.unwrap();
    assert_eq!(response.status(), StatusCode::OK, "test fixture must be able to register a user");
    username
}

async fn probe(app: &axum::Router, username: &str) -> serde_json::Value {
    let request = Request::builder()
        .method("GET")
        .uri(format!("/_matrix/client/v3/register/available?username={username}"))
        .body(Body::empty())
        .unwrap();

    let response = app.clone().oneshot(super::with_local_connect_info(request)).await.unwrap();
    assert_eq!(response.status(), StatusCode::OK, "availability probe must answer 200");
    let bytes = axum::body::to_bytes(response.into_body(), 1024 * 16).await.unwrap();
    serde_json::from_slice(&bytes).unwrap()
}

/// Default (`false`, upstream parity): the endpoint still answers truthfully.
/// This is the baseline the inhibited mode is measured against — without it, the
/// second test could pass vacuously against an endpoint that always says `true`.
#[tokio::test]
async fn availability_probe_reports_the_truth_when_not_inhibited() {
    let Some(app) = setup(false).await else { return };
    let taken = register_a_user(&app).await;
    let free = format!("free_{}", rand::random::<u32>());

    assert_eq!(probe(&app, &taken).await["available"], json!(false), "a registered localpart is not free");
    assert_eq!(probe(&app, &free).await["available"], json!(true), "an unused localpart is free");
}

/// With the flag on, the probe must not depend on whether the localpart exists —
/// which is only possible if it no longer consults the users table at all.
#[tokio::test]
async fn inhibiting_the_user_in_use_error_hides_membership_on_the_availability_probe() {
    let Some(app) = setup(true).await else { return };
    let taken = register_a_user(&app).await;
    let free = format!("free_{}", rand::random::<u32>());

    let taken_body = probe(&app, &taken).await;
    let free_body = probe(&app, &free).await;

    assert_eq!(taken_body["available"], json!(true), "a taken localpart must be reported as available");
    assert_eq!(free_body["available"], json!(true), "a free localpart must be reported as available");

    // The bodies necessarily differ in the echoed `username`; what must not vary
    // is the response *shape* and the verdict.
    let keys = |v: &serde_json::Value| {
        let mut keys: Vec<String> = v.as_object().expect("object body").keys().cloned().collect();
        keys.sort();
        keys
    };
    assert_eq!(keys(&taken_body), keys(&free_body), "the response shape must not depend on existence");
}
