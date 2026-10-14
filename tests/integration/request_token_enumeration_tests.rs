//! Anti-enumeration contract for the `/requestToken` family
//! (`request_token_inhibit_3pid_errors`, upstream changelog `20243`).
//!
//! These drive the **production router**. They exist because the tests that used
//! to claim this coverage (`tests/unit/account_compat_route_tests.rs`,
//! "request_password_email_verification — anti-enumeration contract") asserted on
//! hand-built `json!` literals without calling any handler — deleting the
//! behaviour from the handler left them green.
//!
//! The property under test is the one the flag is named after: a client must not
//! be able to tell, from the response, whether an email address is already
//! associated with an account.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use axum::body::Body;
use axum::http::{Request, StatusCode};
use serde_json::json;
use std::sync::Arc;
use synapse_rust::cache::{CacheConfig, CacheManager};
use synapse_services::ServiceContainer;
use synapse_web::routes::state::AppState;
use tower::ServiceExt;

const PASSWORD_RESET: &str = "/_matrix/client/v3/account/password/email/requestToken";
const REGISTER: &str = "/_matrix/client/v3/register/email/requestToken";

/// The parts of a response a client can observe.
#[derive(Debug)]
struct Observed {
    status: StatusCode,
    body: serde_json::Value,
}

impl Observed {
    /// Shape only — everything except the (necessarily different, sequential)
    /// `sid` value. This is what must not depend on the registration state.
    fn shape(&self) -> Vec<String> {
        let mut keys: Vec<String> = self.body.as_object().map(|o| o.keys().cloned().collect()).unwrap_or_default();
        keys.sort();
        keys
    }
}

async fn post_request_token(app: &axum::Router, uri: &str, email: &str) -> Observed {
    let request = Request::builder()
        .method("POST")
        .uri(uri)
        .header("Content-Type", "application/json")
        .body(Body::from(json!({ "email": email, "client_secret": "enum-secret", "send_attempt": 1 }).to_string()))
        .unwrap();

    let response = app.clone().oneshot(super::with_local_connect_info(request)).await.unwrap();
    let status = response.status();
    let bytes = axum::body::to_bytes(response.into_body(), 1024 * 16).await.unwrap();
    let body = serde_json::from_slice(&bytes).unwrap_or(serde_json::Value::Null);
    Observed { status, body }
}

/// Register one account through the real endpoint, so the email we bind below
/// belongs to a genuinely existing user.
async fn register_user_and_get_id(app: &axum::Router) -> String {
    let request = Request::builder()
        .method("POST")
        .uri("/_matrix/client/v3/register")
        .header("Content-Type", "application/json")
        .body(Body::from(
            json!({
                "username": format!("enum_user_{}", rand::random::<u32>()),
                "password": "UserTest@123",
                "device_id": "ENUMDEVICE",
                "auth": { "type": "m.login.dummy" }
            })
            .to_string(),
        ))
        .unwrap();

    let response = app.clone().oneshot(super::with_local_connect_info(request)).await.unwrap();
    assert_eq!(response.status(), StatusCode::OK, "test fixture must be able to register a user");
    let bytes = axum::body::to_bytes(response.into_body(), 1024 * 16).await.unwrap();
    let body: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
    body["user_id"].as_str().unwrap().to_string()
}

/// Router + a handle for binding a verified email address, with the
/// anti-enumeration knob set explicitly.
async fn setup(
    inhibit: bool,
) -> Option<(axum::Router, Arc<synapse_services::account_identity_service::AccountIdentityService>)> {
    let pool = super::get_test_pool().await?;
    let mut container = ServiceContainer::new_test_with_pool(pool).await;
    // Keep the rate limiter out of the way: this test is about response
    // contents, not about 429s.
    super::config_mut(&mut container).rate_limit.enabled = false;
    super::config_mut(&mut container).server.request_token_inhibit_3pid_errors = inhibit;

    let identity = container.account.account_identity_service.clone();
    let cache = Arc::new(CacheManager::new(&CacheConfig::default()));
    let state = AppState::new(container, cache);
    Some((synapse_web::create_router(state), identity))
}

/// Bind `email` as a verified threepid of a freshly registered user, and return
/// the pair `(registered_email, unknown_email)`.
async fn seed_registered_email(
    app: &axum::Router,
    identity: &Arc<synapse_services::account_identity_service::AccountIdentityService>,
) -> (String, String) {
    let user_id = register_user_and_get_id(app).await;
    let suffix = rand::random::<u32>();
    let registered = format!("enum-registered-{suffix}@example.com");
    let unknown = format!("enum-unknown-{suffix}@example.com");
    let now = synapse_common::current_timestamp_millis();

    identity
        .add_verified_threepid(&user_id, "email", &registered, now, now)
        .await
        .expect("binding a verified threepid is a test-fixture precondition");

    (registered, unknown)
}

/// With `request_token_inhibit_3pid_errors = true` (the default), all three
/// flows answer identically whether or not the address is registered.
#[tokio::test]
async fn inhibiting_hides_the_registration_state_on_every_request_token_flow() {
    let Some((app, identity)) = setup(true).await else { return };
    let (registered, unknown) = seed_registered_email(&app, &identity).await;

    for uri in [PASSWORD_RESET, REGISTER] {
        let known = post_request_token(&app, uri, &registered).await;
        let absent = post_request_token(&app, uri, &unknown).await;

        assert_eq!(known.status, StatusCode::OK, "{uri}: a registered address must look like success");
        assert_eq!(absent.status, StatusCode::OK, "{uri}: an unknown address must look like success");
        assert_eq!(
            known.shape(),
            absent.shape(),
            "{uri}: the response *shape* must not depend on the registration state"
        );
        assert_eq!(
            known.body.get("expires_in"),
            absent.body.get("expires_in"),
            "{uri}: expires_in must not depend on the registration state"
        );
        assert_eq!(
            known.body.get("submit_url"),
            absent.body.get("submit_url"),
            "{uri}: submit_url must not depend on the registration state"
        );
        assert!(known.body.get("errcode").is_none(), "{uri}: no error may leak for a registered address");
        assert!(absent.body.get("errcode").is_none(), "{uri}: no error may leak for an unknown address");
        // Both really did produce a session id (not a vacuous `{}`).
        assert!(known.body["sid"].is_string(), "{uri}: a sid must be issued");
        assert!(absent.body["sid"].is_string(), "{uri}: a sid must be issued");
        assert_ne!(
            known.body["sid"], absent.body["sid"],
            "{uri}: sanity — two distinct sessions, so the equality above is not trivially satisfied"
        );
    }
}

/// With the knob off, the honest spec answers come back — which is also what
/// proves the flag is actually wired rather than decorative.
#[tokio::test]
async fn turning_the_knob_off_restores_the_honest_spec_errors() {
    let Some((app, identity)) = setup(false).await else { return };
    let (registered, unknown) = seed_registered_email(&app, &identity).await;

    // Registration: an already-taken address is a 400 `M_THREEPID_IN_USE`
    // (registration.yaml), an unknown one still succeeds.
    let taken = post_request_token(&app, REGISTER, &registered).await;
    assert_eq!(taken.status, StatusCode::BAD_REQUEST, "register/requestToken: M_THREEPID_IN_USE is a 400");
    assert_eq!(taken.body["errcode"], "M_THREEPID_IN_USE");
    assert_eq!(post_request_token(&app, REGISTER, &unknown).await.status, StatusCode::OK);

    // Password reset is the opposite direction: an address with no account is a
    // 400 `M_THREEPID_NOT_FOUND` (password_management.yaml), a registered one
    // still succeeds.
    let no_account = post_request_token(&app, PASSWORD_RESET, &unknown).await;
    assert_eq!(no_account.status, StatusCode::BAD_REQUEST, "password/requestToken: M_THREEPID_NOT_FOUND is a 400");
    assert_eq!(no_account.body["errcode"], "M_THREEPID_NOT_FOUND");
    assert_eq!(post_request_token(&app, PASSWORD_RESET, &registered).await.status, StatusCode::OK);
}
