//! Global HTTP request metrics middleware.
//!
//! This is the **only** production caller of [`ServerMetrics::record_http_request`],
//! [`ServerMetrics::http_request_started`] and [`ServerMetrics::http_request_finished`].
//! Before it existed those three methods were defined and unit-tested but never
//! invoked, so `http_requests_total`, `http_request_errors_total`,
//! `http_active_requests` and `http_request_duration_ms` stayed at zero no matter
//! how much traffic the server served — which silently disabled every HTTP alert
//! built on them (`HighHTTPErrorRate`, `HTTPRequestDurationHigh`,
//! `HTTPActiveRequestsGrowing`). See `scripts/ci/check_metric_instrumentation.py`.

use axum::body::Body;
use axum::extract::State;
use axum::http::Request;
use axum::middleware::Next;
use axum::response::Response;
use std::sync::Arc;
use std::time::Instant;
use synapse_common::server_metrics::ServerMetrics;

/// Paths deliberately excluded from HTTP metrics.
///
/// `/metrics` is served by a *separate* listener (see `src/server/server.rs`
/// prometheus router), so it never reaches this middleware anyway — it is listed
/// defensively so that merging the routers later cannot silently turn every
/// scrape into a counted request and drown real traffic in the rate.
///
/// Health endpoints are excluded because orchestrators poll them on a fixed
/// schedule: counting them inflates `http_requests_total` with constant noise and
/// drags the duration histogram toward the (trivial) probe latency.
const EXCLUDED_PATHS: &[&str] = &["/health", "/healthz", "/metrics"];

/// Records request count/duration/errors and maintains the in-flight gauge.
///
/// Mounted globally in `src/server/router.rs` via
/// `middleware::from_fn_with_state(server_metrics, http_metrics_middleware)`.
pub async fn http_metrics_middleware(
    State(metrics): State<Arc<ServerMetrics>>,
    request: Request<Body>,
    next: Next,
) -> Response {
    if EXCLUDED_PATHS.contains(&request.uri().path()) {
        return next.run(request).await;
    }

    metrics.http_request_started();
    // RAII so the gauge is released on *every* exit path, including client
    // disconnect: axum drops the handler future mid-flight in that case, so a
    // plain decrement after `next.run(...)` would never execute and
    // `http_active_requests` would drift upward forever.
    let _in_flight = InFlightGuard { metrics: metrics.clone() };
    let start = Instant::now();

    let response = next.run(request).await;

    let status = response.status();
    // Error = 4xx/5xx, matching the documented contract of
    // `http_request_errors_total`. Deliberately *not* `!status.is_success()`:
    // that would also count 1xx/3xx, and Matrix servers do emit redirects.
    //
    // Note for operators: this counts 4xx as errors, so `HighHTTPErrorRate`
    // (>1%) also trips on ordinary client-side failures such as expired tokens
    // (401), unknown rooms (404) and rate limiting (429). If that proves noisy,
    // narrow the alert expression to `... and on() rate(...{code=~"5.."})` — the
    // counter itself is intentionally 4xx+5xx per its documented contract.
    let is_error = status.is_client_error() || status.is_server_error();
    metrics.record_http_request(start.elapsed().as_secs_f64() * 1000.0, !is_error);

    response
}

/// Releases the in-flight gauge when the request future ends, however it ends.
struct InFlightGuard {
    metrics: Arc<ServerMetrics>,
}

impl Drop for InFlightGuard {
    fn drop(&mut self) {
        self.metrics.http_request_finished();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::body::Body as AxumBody;
    use axum::http::{Request as HttpRequest, StatusCode};
    use axum::routing::get;
    use axum::Router;
    use synapse_common::metrics::MetricsCollector;
    use tower::ServiceExt;

    fn req(path: &str) -> HttpRequest<AxumBody> {
        HttpRequest::builder().uri(path).body(AxumBody::empty()).expect("build test request")
    }

    fn build_app(status: StatusCode) -> (Router, Arc<ServerMetrics>) {
        let metrics = Arc::new(ServerMetrics::new(Arc::new(MetricsCollector::new())));
        let app = Router::new()
            .route("/ok", get(move || async move { status }))
            .layer(axum::middleware::from_fn_with_state(metrics.clone(), http_metrics_middleware));
        (app, metrics)
    }

    #[tokio::test]
    async fn counts_request_and_records_duration() {
        let (app, metrics) = build_app(StatusCode::OK);

        assert!(app.oneshot(req("/ok")).await.is_ok());

        assert_eq!(metrics.http_requests_total.get(), 1);
        assert_eq!(metrics.http_request_errors_total.get(), 0);
        assert_eq!(metrics.http_request_duration.get_count(), 1);
    }

    #[tokio::test]
    async fn counts_5xx_as_error() {
        let (app, metrics) = build_app(StatusCode::INTERNAL_SERVER_ERROR);

        app.oneshot(req("/ok")).await.ok();

        assert_eq!(metrics.http_request_errors_total.get(), 1);
    }

    #[tokio::test]
    async fn gauge_returns_to_zero_after_request() {
        let (app, metrics) = build_app(StatusCode::OK);

        app.oneshot(req("/ok")).await.ok();

        assert_eq!(metrics.http_active_requests.get(), 0.0, "in-flight gauge must be released");
    }

    #[tokio::test]
    async fn excludes_health_probe_paths() {
        let (app, metrics) = build_app(StatusCode::OK);

        // /health is excluded (and unmatched => 404), so only /ok is counted.
        app.clone().oneshot(req("/health")).await.ok();
        app.oneshot(req("/ok")).await.ok();

        assert_eq!(metrics.http_requests_total.get(), 1, "only the non-excluded request counts");
    }
}
