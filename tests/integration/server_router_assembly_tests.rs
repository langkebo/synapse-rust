//! P-19 (second half): the **production** router assembly must be reachable from
//! tests, and its layers must be observable.
//!
//! `synapse_web::routes::create_router` is only the route table. The server
//! serves that table wrapped in six layers
//! (`synapse_rust::server::router::build_router`): body limit, HTTP RED metrics,
//! request debug, request timeout, tracing, and the 413→`M_TOO_LARGE` rewriter.
//! The integration suite and the manual perf probes both built their app with
//! `create_router`, so none of those layers ran under test — which is why a hung
//! third-party rule looked "unbounded" in the perf probe while production bounds
//! the request (see P-19 in `docs/synapse-rust-vs-synapse-comparison.md`).
//!
//! What it pins is the HTTP RED metrics layer, read straight off
//! `ServerMetrics::http_requests_total`: `/metrics` is served by a **separate
//! listener** (`src/server/server.rs`) and is excluded from these counters, so it
//! cannot be the probe. The bare router is asserted *not* to move the counter —
//! that contrast is what proves the harness had been missing the production stack.
//!
//! **Not asserted here** (see the P-19 residual): the body limit + 413 rewriter.
//! Driving it over HTTP hangs on the media upload route in this harness (the
//! upload path waits on media storage), and on JSON routes axum's `Json` extractor
//! wraps the length error into its own `400` rejection, so the rewriter never sees
//! a 413. It stays covered by `synapse-web/src/middleware/security.rs`'s unit
//! tests.

use axum::body::Body;
use axum::http::{Request, StatusCode};
use tower::ServiceExt;

use super::{create_test_user, setup_fresh_test_app_with_state};

fn versions_request() -> Request<Body> {
    Request::builder()
        .method("GET")
        .uri("/_matrix/client/v3/versions")
        .body(Body::empty())
        .expect("the request must build")
}

#[tokio::test]
async fn production_assembly_is_reachable_and_its_layers_are_observable() {
    let Some((bare_app, state)) = setup_fresh_test_app_with_state().await else {
        return;
    };
    let metrics = state.services.core.server_metrics.clone();

    // 1. The bare route table has no HTTP RED metrics layer: a request through it
    //    must not move the counter at all.
    let before_bare = metrics.http_requests_total.get();
    let bare_response = ServiceExt::<Request<Body>>::oneshot(bare_app, versions_request()).await.expect("executes");
    assert_eq!(bare_response.status(), StatusCode::OK, "the harness app must serve the client API");
    assert_eq!(
        metrics.http_requests_total.get(),
        before_bare,
        "`create_router` alone must not record `http_requests_total` — if it does, this \
         contrast no longer proves that the production layers were missing from the harness"
    );

    // 2. The production assembly: same route table, plus the six server layers.
    let mut config = (*state.services.core.config).clone();
    config.server.max_upload_size = 1024;
    let production_app = synapse_rust::server::build_router(state, &config);

    let before_production = metrics.http_requests_total.get();
    let versions =
        ServiceExt::<Request<Body>>::oneshot(production_app.clone(), versions_request()).await.expect("executes");
    assert_eq!(versions.status(), StatusCode::OK, "the production stack must serve the client API");

    // 3. The request the production stack served was counted.
    // 4. Both requests the production stack served were counted — including the
    //    one whose response the rewriter replaced.
    assert_eq!(
        metrics.http_requests_total.get(),
        before_production + 1,
        "the production assembly must record every non-excluded request"
    );
}

/// The body limit and the 413→`M_TOO_LARGE` rewriter, end-to-end over HTTP.
///
/// The route must extract the body with something that surfaces a length-limit
/// error as **413** — `Json` wraps it into its own `400` instead, and the media
/// upload route has its own `DefaultBodyLimit` derived from the state config (and
/// blocks on media storage in this harness). The receipts route takes `body:
/// String`, so it is the one client route where the server-level limit and the
/// rewriter are both observable.
#[tokio::test]
async fn production_body_limit_surfaces_as_matrix_too_large() {
    let Some((bare_app, state)) = setup_fresh_test_app_with_state().await else {
        return;
    };
    let token = create_test_user(&bare_app).await;

    let mut config = (*state.services.core.config).clone();
    config.server.max_upload_size = 1024;
    let production_app = synapse_rust::server::build_router(state, &config);

    // Syntactically valid IDs so the `Path` extractors pass and the body extractor
    // (which is what enforces the limit) is actually reached.
    let room_id = format!("!{}", "a".repeat(43));
    let event_id = format!("${}", "a".repeat(43));
    let request = Request::builder()
        .method("POST")
        .uri(format!("/_matrix/client/v3/rooms/{room_id}/receipt/m.read/{event_id}"))
        .header("Authorization", format!("Bearer {token}"))
        .header("Content-Type", "application/json")
        .body(Body::from(vec![b'x'; 4 * 1024]))
        .expect("the request must build");

    let response = ServiceExt::<Request<Body>>::oneshot(production_app, request).await.expect("executes");
    let status = response.status();
    let body = axum::body::to_bytes(response.into_body(), 8 * 1024).await.expect("body reads");
    let json: serde_json::Value = serde_json::from_slice(&body).unwrap_or(serde_json::Value::Null);
    assert_eq!(
        status,
        StatusCode::PAYLOAD_TOO_LARGE,
        "a body over `server.max_upload_size` must be rejected with 413, got {status}: {json}"
    );
    assert_eq!(
        json["errcode"], "M_TOO_LARGE",
        "the production rewriter must turn the bare 413 into a Matrix error, got {json}"
    );
}
