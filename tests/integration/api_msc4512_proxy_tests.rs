//! MSC4512 application-service request proxying — HTTP-layer tests.
//!
//! `synapse-web/src/routes/msc4512_proxy.rs` is the one place where the two
//! halves of the MSC are wired to the router:
//!
//! * the inbound half (`msc4512_proxy_fallback`) is the global fallback, so its
//!   dispatch can only be observed through a real request against the assembled
//!   router — there is no handler to call in isolation;
//! * the outbound half (`fed_proxy`) has nine guard clauses of which eight are
//!   reachable without a federation peer.
//!
//! Configuration decoding, prefix matching and target resolution are unit-tested
//! in `synapse_services::application_service::proxy`; this file covers the HTTP
//! behaviour on top of them.
//!
//! ⚠️ Each test builds its own app on an isolated schema (see the long comment
//! in `api_account_status_tests.rs`): a cached sqlx pool outlives its owning
//! tokio runtime under `#[tokio::test]`.
//!
//! Note on fixtures: `POST /_synapse/admin/v1/appservices` cannot express the
//! `proxy_prefix` / `proxy_url` pair (`RegisterAppServiceBody` is
//! `deny_unknown_fields` and `into_request()` hard-codes `config: None`), so the
//! application service is registered through the service layer directly — which
//! is exactly what a registration-file import does.

use axum::{
    body::Body,
    http::{Request, StatusCode},
};
use serde_json::{json, Value};
use synapse_services::application_service::RegisterApplicationServiceRequest;
use synapse_web::routes::state::AppState;
use tower::ServiceExt;
use wiremock::{matchers::method, Mock, ResponseTemplate};

use crate::common::http_mock::HttpMock;

/// Build an MSC4512-enabled app on a fresh schema.
async fn enabled_app() -> (axum::Router, AppState) {
    super::setup_fresh_test_app_with_config(|container| {
        super::config_mut(container).experimental.msc4512_enabled = true;
    })
    .await
    .expect("fresh test app setup panics on infrastructure failure, so it is always Some")
}

async fn register_user(app: &axum::Router, username: &str) -> (String, String) {
    let request = Request::builder()
        .method("POST")
        .uri("/_matrix/client/v3/register")
        .header("Content-Type", "application/json")
        .body(Body::from(
            json!({
                "username": username,
                "password": "Password123!",
                "auth": { "type": "m.login.dummy" }
            })
            .to_string(),
        ))
        .unwrap();

    let response = ServiceExt::<Request<Body>>::oneshot(app.clone(), request).await.unwrap();
    assert_eq!(response.status(), StatusCode::OK);

    let body = axum::body::to_bytes(response.into_body(), 4096).await.unwrap();
    let json: Value = serde_json::from_slice(&body).unwrap();
    (json["access_token"].as_str().unwrap().to_string(), json["user_id"].as_str().unwrap().to_string())
}

/// Register an application service the way a registration-file import would,
/// including an optional MSC4512 `config` blob.
async fn register_appservice(state: &AppState, as_id: &str, hs_token: &str, config: Option<Value>) {
    state
        .services
        .admin
        .modules
        .app_service_manager
        .register(RegisterApplicationServiceRequest {
            as_id: as_id.to_string(),
            url: None,
            as_token: format!("as_token_{as_id}"),
            hs_token: hs_token.to_string(),
            sender: format!("@{as_id}:test"),
            description: None,
            is_rate_limited: Some(false),
            protocols: None,
            namespaces: None,
            api_key: None,
            config,
        })
        .await
        .expect("application service registration should succeed");
}

/// Mint an access token whose JWT carries the impersonating application
/// service id (MSC4190), which is what `fed_proxy` keys off.
async fn appservice_token(state: &AppState, user_id: &str, as_id: &str) -> String {
    // `access_tokens.device_id` has an FK to `devices(device_id)`, and an
    // application service supplies its own device id without going through the
    // password/QR device flow — so the row has to be materialised first, the
    // same way `/_matrix/client/v3/login` does for appservice logins.
    let device_id = format!("ASDEVICE{}{}", rand::random::<u32>(), as_id.len());
    state
        .services
        .account
        .account_device_list_service
        .create_device(&device_id, user_id, None)
        .await
        .expect("application service device row should be created");
    state
        .services
        .core
        .token_auth
        .generate_appservice_access_token(user_id, &device_id, false, as_id)
        .await
        .expect("appservice access token generation should succeed")
}

fn get_request(uri: &str, token: Option<&str>) -> Request<Body> {
    let mut builder = Request::builder().method("GET").uri(uri);
    if let Some(token) = token {
        builder = builder.header("Authorization", format!("Bearer {token}"));
    }
    builder.body(Body::empty()).unwrap()
}

async fn body_json(response: axum::response::Response) -> Value {
    let body = axum::body::to_bytes(response.into_body(), 8192).await.unwrap();
    serde_json::from_slice(&body).unwrap()
}

async fn fed_proxy_request(app: &axum::Router, token: &str, body: &Value) -> axum::response::Response {
    let request = Request::builder()
        .method("POST")
        .uri("/_matrix/client/v1/appservice/fed_proxy")
        .header("Content-Type", "application/json")
        .header("Authorization", format!("Bearer {token}"))
        .body(Body::from(body.to_string()))
        .unwrap();
    ServiceExt::<Request<Body>>::oneshot(app.clone(), request).await.unwrap()
}

async fn assert_error(response: axum::response::Response, status: StatusCode, errcode: &str) {
    assert_eq!(response.status(), status, "unexpected status for {errcode}");
    assert_eq!(body_json(response).await["errcode"], errcode);
}

// ── inbound half ─────────────────────────────────────────────────────────

/// `experimental.msc4512_enabled` defaults to `false`; the fallback must stay
/// the plain `M_UNRECOGNIZED` 404 in that case.
#[tokio::test]
async fn msc4512_disabled_by_default_returns_unrecognized() {
    let Some(app) = super::setup_test_app().await else {
        return;
    };

    let response = ServiceExt::<Request<Body>>::oneshot(app, get_request("/_matrix/client/v3/rtc/livekit/foo", None))
        .await
        .unwrap();

    assert_error(response, StatusCode::NOT_FOUND, "M_UNRECOGNIZED").await;
}

/// Enabled, but no application service claims `rtc/livekit` — still a 404.
#[tokio::test]
async fn enabled_but_unclaimed_prefix_returns_unrecognized() {
    let (app, _state) = enabled_app().await;

    let response = ServiceExt::<Request<Body>>::oneshot(app, get_request("/_matrix/client/v3/rtc/livekit/foo", None))
        .await
        .unwrap();

    assert_error(response, StatusCode::NOT_FOUND, "M_UNRECOGNIZED").await;
}

/// A claimed prefix is forwarded to `proxy_url` with the application service's
/// `hs_token`, the verified caller in `X-Matrix-User-Identifier`, and no
/// `access_token` in the forwarded query string.
#[tokio::test]
async fn client_request_is_forwarded_with_hs_token_and_without_access_token() {
    let (app, state) = enabled_app().await;
    let (token, user_id) = register_user(&app, &format!("msc4512_fwd_{}", rand::random::<u32>())).await;

    let mock = HttpMock::new().await;
    Mock::given(method("GET"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({ "proxied": true })))
        .mount(mock.inner())
        .await;

    let hs_token = "hs_token_forwarding";
    register_appservice(
        &state,
        "msc4512_forwarder",
        hs_token,
        Some(json!({ "proxy_prefix": "rtc/livekit", "proxy_url": mock.base_url() })),
    )
    .await;

    let response = ServiceExt::<Request<Body>>::oneshot(
        app,
        get_request("/_matrix/client/v3/rtc/livekit/foo?access_token=leaked&x=1", Some(&token)),
    )
    .await
    .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
    assert_eq!(body_json(response).await, json!({ "proxied": true }));

    let requests = mock.inner().received_requests().await.expect("mock server should record the forwarded request");
    assert_eq!(requests.len(), 1, "exactly one request should reach the application service");
    let forwarded = &requests[0];

    assert_eq!(forwarded.method.as_str(), "GET");
    assert_eq!(forwarded.url.path(), "/_matrix/client/v3/rtc/livekit/foo");
    assert_eq!(forwarded.url.query(), Some("x=1"), "access_token must be dropped, other query params kept");

    let header = |name: &str| forwarded.headers.get(name).and_then(|value| value.to_str().ok()).map(str::to_owned);
    assert_eq!(header("authorization").as_deref(), Some("Bearer hs_token_forwarding"));
    assert_eq!(header("x-matrix-user-identifier").as_deref(), Some(user_id.as_str()));
}

/// The proxy has no unauthenticated passthrough: without a token the request is
/// rejected instead of being forwarded.
#[tokio::test]
async fn client_request_without_access_token_is_unauthorized() {
    let (app, state) = enabled_app().await;
    let mock = HttpMock::new().await;
    register_appservice(
        &state,
        "msc4512_unauth",
        "hs_token_unauth",
        Some(json!({ "proxy_prefix": "rtc/livekit", "proxy_url": mock.base_url() })),
    )
    .await;

    let response = ServiceExt::<Request<Body>>::oneshot(app, get_request("/_matrix/client/v3/rtc/livekit/foo", None))
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
    assert!(
        mock.inner().received_requests().await.unwrap_or_default().is_empty(),
        "an unauthenticated request must not be forwarded"
    );
}

/// Federation ingress is off by default, so the federated surface of a claimed
/// prefix must be absent too — even though the prefix resolves.
#[tokio::test]
async fn federation_proxied_request_is_absent_when_ingress_disabled() {
    let (app, state) = enabled_app().await;
    let mock = HttpMock::new().await;
    register_appservice(
        &state,
        "msc4512_fed_off",
        "hs_token_fed_off",
        Some(json!({ "proxy_prefix": "rtc/livekit", "proxy_url": mock.base_url() })),
    )
    .await;

    let response =
        ServiceExt::<Request<Body>>::oneshot(app, get_request("/_matrix/federation/v1/rtc/livekit/foo", None))
            .await
            .unwrap();

    assert_error(response, StatusCode::NOT_FOUND, "M_UNRECOGNIZED").await;
    assert!(
        mock.inner().received_requests().await.unwrap_or_default().is_empty(),
        "a request refused by the ingress gate must not be forwarded"
    );
}

// ── outbound half ────────────────────────────────────────────────────────

/// `fed_proxy` signs requests on behalf of an application service, so a plain
/// user token must be refused before any other validation runs.
#[tokio::test]
async fn fed_proxy_rejects_non_appservice_callers() {
    let (app, _state) = enabled_app().await;
    let (token, _) = register_user(&app, &format!("msc4512_plain_{}", rand::random::<u32>())).await;

    let response = fed_proxy_request(
        &app,
        &token,
        &json!({
            "destination": "remote.example",
            "method": "GET",
            "path": "/_matrix/federation/v1/version",
        }),
    )
    .await;

    assert_error(response, StatusCode::FORBIDDEN, "M_FORBIDDEN").await;
}

/// Parameter validation runs in a fixed order; each case below pins one branch.
#[tokio::test]
async fn fed_proxy_validates_request_parameters() {
    let (app, state) = enabled_app().await;
    let (_user_token, user_id) = register_user(&app, &format!("msc4512_param_{}", rand::random::<u32>())).await;
    register_appservice(&state, "msc4512_param", "hs_token_param", None).await;
    let token = appservice_token(&state, &user_id, "msc4512_param").await;

    let cases: [(Value, &str); 8] = [
        // Body must be a JSON object.
        (json!([]), "M_INVALID_PARAM"),
        // `destination`, `method` and `path` are all required.
        (json!({}), "M_MISSING_PARAM"),
        (json!({ "destination": "remote.example", "method": "GET" }), "M_MISSING_PARAM"),
        // Only GET/POST/PUT/DELETE.
        (
            json!({ "destination": "remote.example", "method": "PATCH", "path": "/_matrix/federation/v1/x" }),
            "M_INVALID_PARAM",
        ),
        // GET/DELETE carry no body.
        (
            json!({
                "destination": "remote.example",
                "method": "GET",
                "path": "/_matrix/federation/v1/x",
                "body": { "a": 1 },
            }),
            "M_INVALID_PARAM",
        ),
        // A body must be an object.
        (
            json!({
                "destination": "remote.example",
                "method": "POST",
                "path": "/_matrix/federation/v1/x",
                "body": [],
            }),
            "M_INVALID_PARAM",
        ),
        // `query` must be an object of strings.
        (
            json!({
                "destination": "remote.example",
                "method": "GET",
                "path": "/_matrix/federation/v1/x",
                "query": [],
            }),
            "M_INVALID_PARAM",
        ),
        (
            json!({
                "destination": "remote.example",
                "method": "GET",
                "path": "/_matrix/federation/v1/x",
                "query": { "a": 1 },
            }),
            "M_INVALID_PARAM",
        ),
    ];

    for (body, errcode) in cases {
        let response = fed_proxy_request(&app, &token, &body).await;
        assert_error(response, StatusCode::BAD_REQUEST, errcode).await;
    }
}

/// Destination and path confinement: the homeserver may not be pointed at
/// itself, and the path must stay inside the caller's `proxy_prefix`.
#[tokio::test]
async fn fed_proxy_confines_destination_and_path() {
    let (app, state) = enabled_app().await;
    let (_user_token, user_id) = register_user(&app, &format!("msc4512_path_{}", rand::random::<u32>())).await;
    let local_domain = user_id.rsplit_once(':').expect("registered user id has a domain").1.to_string();

    register_appservice(
        &state,
        "msc4512_prefix",
        "hs_token_prefix",
        Some(json!({ "proxy_prefix": "rtc/livekit", "proxy_url": "http://as.invalid" })),
    )
    .await;
    // A second service with no proxy configuration at all.
    register_appservice(&state, "msc4512_no_prefix", "hs_token_no_prefix", None).await;

    let token = appservice_token(&state, &user_id, "msc4512_prefix").await;

    let destination_denied = json!({
        "destination": local_domain,
        "method": "GET",
        "path": "/_matrix/federation/v1/rtc/livekit/foo",
    });
    let response = fed_proxy_request(&app, &token, &destination_denied).await;
    assert_error(response, StatusCode::FORBIDDEN, "M_FEDPROXY_DESTINATION_DENIED").await;

    let path_not_allowed: [Value; 3] = [
        json!({
            "destination": "remote.example",
            "method": "GET",
            "path": "/_matrix/federation/v1/rtc/../livekit/foo",
        }),
        json!({
            "destination": "remote.example",
            "method": "GET",
            "path": "/_matrix/client/v3/rooms",
        }),
        json!({
            "destination": "remote.example",
            "method": "GET",
            "path": "/_matrix/federation/v1/other/foo",
        }),
    ];
    for body in path_not_allowed {
        let response = fed_proxy_request(&app, &token, &body).await;
        assert_error(response, StatusCode::FORBIDDEN, "M_FEDPROXY_PATH_NOT_ALLOWED").await;
    }

    // A service that never declared a proxy_prefix cannot use `fed_proxy` at all.
    let token_without_prefix = appservice_token(&state, &user_id, "msc4512_no_prefix").await;
    let response = fed_proxy_request(
        &app,
        &token_without_prefix,
        &json!({
            "destination": "remote.example",
            "method": "GET",
            "path": "/_matrix/federation/v1/rtc/livekit/foo",
        }),
    )
    .await;
    assert_error(response, StatusCode::FORBIDDEN, "M_FEDPROXY_PATH_NOT_ALLOWED").await;
}
