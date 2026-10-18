//! JSON rejection contract: **every** body-parse failure must answer with a
//! Matrix error body, never Axum's `text/plain` rejection text.
//!
//! ## Why this exists
//!
//! I-02 in `docs/audit/LIVE_INTEGRATION_BACKEND_FINDINGS_AUDIT_2026-10-18.md`:
//! the router used bare `axum::Json<T>` in 254 extractor positions, so a client
//! that sent a wrong `Content-Type`, broken JSON, or a payload with the wrong
//! field names got:
//!
//! ```text
//! HTTP/1.1 415 Unsupported Media Type
//! content-type: text/plain
//! Expected request with `Content-Type: application/json`
//! ```
//!
//! The SDKs parse the body expecting `errcode`; a `text/plain` body makes them
//! fall back to a generic "unknown error", which is exactly how a *contract*
//! defect turns into a phantom client bug. The routes now use `MatrixJson`,
//! whose rejection mapper emits `M_NOT_JSON` / `M_BAD_JSON`.
//!
//! This test pins the mapper at the HTTP boundary (status, content type and
//! `errcode`), so a future route cannot silently reintroduce a bare `Json<T>`:
//! even one stray extractor shows up as a `text/plain` response here.
//!
//! Scope note: this drives a *minimal* router rather than the whole app, so it
//! needs no database and runs in the unit lane. The conversion of all 289
//! call sites is additionally guarded by `scripts/ci/check_*.sh` and by the
//! absence of bare `Json<` in extractor position.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use axum::body::Body;
use axum::http::{Request, StatusCode};
use axum::routing::post;
use axum::Router;
use serde::Deserialize;
use synapse_common::ApiError;
use synapse_web::routes::MatrixJson;
use tower::ServiceExt as _;

/// Deliberately `deny_unknown_fields`: that is what makes a typo'd field name a
/// deserialization error instead of a silently ignored key (the I-04 shape).
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct ProbeBody {
    name: String,
}

async fn echo(MatrixJson(body): MatrixJson<ProbeBody>) -> axum::Json<serde_json::Value> {
    axum::Json(serde_json::json!({ "name": body.name }))
}

fn router() -> Router {
    Router::new().route("/probe", post(echo))
}

/// Sends `body` with `content_type` and returns `(status, content-type, body)`.
async fn probe(content_type: &str, body: &'static str) -> (StatusCode, String, serde_json::Value) {
    let request = Request::builder()
        .method("POST")
        .uri("/probe")
        .header("content-type", content_type)
        .body(Body::from(body))
        .expect("request must build");

    let response = router().oneshot(request).await.expect("router must answer");
    let status = response.status();
    let content_type =
        response.headers().get("content-type").and_then(|value| value.to_str().ok()).unwrap_or_default().to_string();
    let bytes = axum::body::to_bytes(response.into_body(), 64 * 1024).await.expect("body must read");
    let json: serde_json::Value = serde_json::from_slice(&bytes).unwrap_or_else(|_| {
        panic!(
            "response body must be JSON (got `{}`) — a text/plain rejection means a bare \
             `Json<T>` extractor slipped back into a handler",
            String::from_utf8_lossy(&bytes)
        )
    });
    (status, content_type, json)
}

/// Wrong `Content-Type` (the report's 415 case) must be a Matrix `M_NOT_JSON`.
#[tokio::test]
async fn wrong_content_type_returns_matrix_not_json() {
    let (status, content_type, body) = probe("text/plain", "x").await;

    assert_eq!(status, StatusCode::BAD_REQUEST, "Matrix has no 415 for this: the body is simply not JSON");
    assert!(
        content_type.starts_with("application/json"),
        "content-type must be application/json, got `{content_type}`"
    );
    assert_eq!(body["errcode"], "M_NOT_JSON", "body: {body}");
}

/// Broken JSON with the right content type is also `M_NOT_JSON`.
#[tokio::test]
async fn malformed_json_returns_matrix_not_json() {
    let (status, content_type, body) = probe("application/json", r#"{"name": "#).await;

    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert!(content_type.starts_with("application/json"), "got `{content_type}`");
    assert_eq!(body["errcode"], "M_NOT_JSON", "body: {body}");
}

/// Valid JSON that does not match the handler's type is `M_BAD_JSON`
/// (this is the `{"name":123}` / unknown-field shape from I-04).
#[tokio::test]
async fn wrong_field_types_return_matrix_bad_json() {
    for payload in [r#"{"name":123}"#, r#"{"name":"ok","unexpected":true}"#, r#"{}"#] {
        let (status, content_type, body) = probe("application/json", payload).await;

        assert_eq!(status, StatusCode::BAD_REQUEST, "payload `{payload}`");
        assert!(content_type.starts_with("application/json"), "payload `{payload}` got `{content_type}`");
        assert_eq!(body["errcode"], "M_BAD_JSON", "payload `{payload}` body: {body}");
        assert!(
            body["error"].as_str().is_some_and(|error| !error.is_empty()),
            "Matrix error bodies carry a human-readable `error`: {body}"
        );
    }
}

/// The happy path is untouched.
#[tokio::test]
async fn valid_body_still_succeeds() {
    let (status, content_type, body) = probe("application/json", r#"{"name":"ok"}"#).await;

    assert_eq!(status, StatusCode::OK);
    assert!(content_type.starts_with("application/json"));
    assert_eq!(body["name"], "ok");
}

/// `Option<MatrixJson<T>>` keeps Axum's "no content type ⇒ None" semantics while
/// still mapping malformed bodies through the Matrix error shape.
#[tokio::test]
async fn optional_matrix_json_maps_rejections_through_the_same_mapper() {
    async fn maybe(MatrixJson(body): MatrixJson<ProbeBody>) -> axum::Json<serde_json::Value> {
        axum::Json(serde_json::json!({ "name": body.name }))
    }
    async fn optional(body: Option<MatrixJson<ProbeBody>>) -> axum::Json<serde_json::Value> {
        axum::Json(match body {
            Some(MatrixJson(body)) => serde_json::json!({ "name": body.name }),
            None => serde_json::json!({ "absent": true }),
        })
    }
    let _ = maybe; // documented companion handler, kept for symmetry

    let app = Router::new().route("/optional", post(optional));

    // No `Content-Type` at all ⇒ handler sees `None` (no error).
    let response = app
        .clone()
        .oneshot(Request::builder().method("POST").uri("/optional").body(Body::from("")).expect("request must build"))
        .await
        .expect("router must answer");
    assert_eq!(response.status(), StatusCode::OK);

    // Wrong content type ⇒ Matrix error body, not `text/plain`.
    let response = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/optional")
                .header("content-type", "text/plain")
                .body(Body::from("x"))
                .expect("request must build"),
        )
        .await
        .expect("router must answer");
    assert_eq!(response.status(), StatusCode::BAD_REQUEST);
    let bytes = axum::body::to_bytes(response.into_body(), 64 * 1024).await.expect("body must read");
    let body: serde_json::Value = serde_json::from_slice(&bytes).expect("body must be JSON");
    assert_eq!(body["errcode"], "M_NOT_JSON", "body: {body}");
}

/// Guard the error type itself: `MatrixJson`'s rejection is `ApiError`, whose
/// response is JSON. A refactor that switches to `JsonRejection` would fail to
/// compile here, which is the point.
#[test]
fn matrix_json_rejection_is_an_api_error() {
    fn assert_rejection_type<T>()
    where
        T: axum::extract::FromRequest<()>,
        T::Rejection: Into<ApiError>,
    {
    }
    assert_rejection_type::<MatrixJson<ProbeBody>>();
}
