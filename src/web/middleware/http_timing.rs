//! HTTP request timing middleware (Sprint 6 / obs-3).
//!
//! Wraps every request in a start/end timer, then on the way out records
//! `http_request_duration_seconds{method, path, status}` into a Prometheus
//! histogram. This gives dashboards the standard "p50 / p95 / p99 per
//! route" breakdown that catches the slow endpoint before users do.
//!
//! **Cardinality control**: `path` is taken from `axum::extract::MatchedPath`
//! when available, falling back to the raw `Uri::path`. The matched path
//! is the route *template* (e.g. `/_matrix/client/v3/rooms/{room_id}/state`)
//! rather than the resolved URI (which would contain the actual room id and
//! blow up label cardinality). When the matched path is missing (404 on
//! unknown routes) we substitute the literal string `unmatched` so the
//! cardinality stays bounded.
//!
//! **Status**: taken from the response's `StatusCode::as_u16()`. We do not
//! collapse 4xx/5xx into labels here — the full status code is useful for
//! splitting 401 (auth) from 403 (forbidden) from 429 (rate-limited). If
//! this becomes a cardinality concern, switch to a `status_class` derived
//! label (1xx/2xx/3xx/4xx/5xx).

use std::time::Instant;

use axum::extract::{MatchedPath, Request};
use axum::middleware::Next;
use axum::response::Response;
use std::sync::Arc;

use crate::common::server_metrics::ServerMetrics;

/// Axum middleware: time the request, record `(method, path, status)` into
/// `http_request_duration_ms`.
pub async fn http_timing_middleware(
    axum::extract::State(state): axum::extract::State<Arc<ServerMetrics>>,
    req: Request,
    next: Next,
) -> Response {
    let method = req.method().as_str().to_string();

    // Prefer the matched path template; fall back to a bounded sentinel.
    let path = req
        .extensions()
        .get::<MatchedPath>()
        .map_or_else(|| "unmatched".to_string(), |m| m.as_str().to_string());

    // Bump the in-flight gauge on entry so a saturated server shows up
    // immediately rather than only via the latency histogram.
    state.incr_http_in_flight();

    let start = Instant::now();
    let response = next.run(req).await;
    let elapsed_ms = start.elapsed().as_secs_f64() * 1000.0;
    let status = response.status().as_u16();

    // Always decrement, regardless of status — saturation is about how
    // many requests are *in* the server, not how many finished successfully.
    state.decr_http_in_flight();
    state.record_http_request_timing(&method, &path, status, elapsed_ms);

    response
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::body::Body;
    use axum::http::Request as AxumRequest;
    use axum::middleware::from_fn_with_state;
    use axum::routing::get;
    use axum::Router;
    use tower::ServiceExt;

    async fn echo() -> &'static str {
        "ok"
    }

    fn test_router(metrics: Arc<ServerMetrics>) -> Router {
        // Sprint 6 / obs-3: use axum 0.7 capture syntax (`{name}`),
        // not the deprecated `:name` form. The route template is
        // what the middleware records as the `path` label.
        Router::new()
            .route("/_matrix/client/v3/rooms/{room_id}/state", get(echo))
            .route("/_matrix/client/v3/whoami", get(echo))
            .layer(from_fn_with_state(metrics, http_timing_middleware))
    }

    #[tokio::test]
    async fn records_timing_for_matched_path() {
        let collector = Arc::new(crate::common::metrics::MetricsCollector::new());
        let metrics = Arc::new(ServerMetrics::new(collector));
        let app = test_router(metrics.clone());

        let req = AxumRequest::builder()
            .method("GET")
            .uri("/_matrix/client/v3/rooms/!abc:server/state")
            .body(Body::empty())
            .unwrap();
        let resp = app.oneshot(req).await.unwrap();
        assert_eq!(resp.status().as_u16(), 200);

        // The middleware should have observed exactly one sample and
        // bumped the global histogram + counter.
        assert_eq!(metrics.http_request_duration.get_count(), 1);
        assert!(metrics.http_active_requests.get() >= 0.0, "gauge must not underflow");
    }
}
