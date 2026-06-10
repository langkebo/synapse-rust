use crate::routes::AppState;
use axum::{
    extract::State,
    http::{HeaderValue, Request},
    middleware::Next,
    response::Response,
};
use axum::body::Body;
use std::sync::atomic::{AtomicU64, Ordering};
use synapse_common::tracing::RequestId;
use tracing::Span;

static REQUEST_ID_COUNTER: AtomicU64 = AtomicU64::new(0);

/// Generate a unique request ID for tracing.
fn generate_request_id() -> String {
    let counter = REQUEST_ID_COUNTER.fetch_add(1, Ordering::Relaxed);
    let timestamp = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis();
    format!("req-{timestamp}-{counter:06x}")
}

/// Middleware that extracts or generates a `x-request-id` header,
/// propagates it into the tracing span, and echoes it in the response.
///
/// This enables full-chain request tracing across services using the
/// W3C TraceContext propagation standard combined with a custom request ID.
pub async fn request_id_middleware(
    State(_state): State<AppState>,
    request: Request<Body>,
    next: Next,
) -> Response {
    let request_id = request
        .headers()
        .get("x-request-id")
        .and_then(|v| v.to_str().ok())
        .map_or_else(generate_request_id, |v| v.to_string());

    // Record the request ID in the current span for full-chain tracing
    Span::current().record("request_id", &request_id);
    // Also store in span extensions for automatic child-span propagation
    Span::current().extensions_mut().insert(RequestId(request_id.clone()));
    tracing::debug!(%request_id, method = %request.method(), uri = %request.uri(), "Request started");

    let mut response = next.run(request).await;

    // Echo the request ID back to the caller
    if let Ok(value) = HeaderValue::from_str(&request_id) {
        response.headers_mut().insert("x-request-id", value);
    }

    response
}

#[cfg(test)]
mod tests {
    use super::*;
    use synapse_cache::{CacheConfig, CacheManager};
    use synapse_services::ServiceContainer;
    use axum::{
        middleware,
        routing::get,
        Router,
    };
    use std::sync::Arc;
    use tower::ServiceExt;

    async fn ok_handler() -> axum::http::StatusCode {
        axum::http::StatusCode::OK
    }

    #[tokio::test]
    async fn test_request_id_generated_when_missing() {
        let services = ServiceContainer::new_test().await;
        let cache = Arc::new(CacheManager::new(&CacheConfig::default()));
        let state = AppState::new(services, cache);

        let app = Router::new()
            .route("/test", get(ok_handler))
            .layer(middleware::from_fn_with_state(state.clone(), request_id_middleware))
            .with_state(state);

        let request = Request::builder()
            .uri("/test")
            .body(Body::empty())
            .unwrap();

        let response = app.oneshot(request).await.unwrap();
        assert_eq!(response.status(), axum::http::StatusCode::OK);
        assert!(response.headers().get("x-request-id").is_some());
    }

    #[tokio::test]
    async fn test_request_id_preserved_when_provided() {
        let services = ServiceContainer::new_test().await;
        let cache = Arc::new(CacheManager::new(&CacheConfig::default()));
        let state = AppState::new(services, cache);

        let app = Router::new()
            .route("/test", get(ok_handler))
            .layer(middleware::from_fn_with_state(state.clone(), request_id_middleware))
            .with_state(state);

        let request = Request::builder()
            .uri("/test")
            .header("x-request-id", "test-id-12345")
            .body(Body::empty())
            .unwrap();

        let response = app.oneshot(request).await.unwrap();
        assert_eq!(response.status(), axum::http::StatusCode::OK);
        assert_eq!(
            response.headers().get("x-request-id").unwrap().to_str().unwrap(),
            "test-id-12345"
        );
    }

    #[tokio::test]
    async fn test_request_id_unique_between_requests() {
        let services = ServiceContainer::new_test().await;
        let cache = Arc::new(CacheManager::new(&CacheConfig::default()));
        let state = AppState::new(services, cache);

        let app = Router::new()
            .route("/test", get(ok_handler))
            .layer(middleware::from_fn_with_state(state.clone(), request_id_middleware))
            .with_state(state);

        let request1 = Request::builder().uri("/test").body(Body::empty()).unwrap();
        let request2 = Request::builder().uri("/test").body(Body::empty()).unwrap();

        let response1 = app.clone().oneshot(request1).await.unwrap();
        let response2 = app.oneshot(request2).await.unwrap();

        let id1 = response1.headers().get("x-request-id").unwrap().to_str().unwrap();
        let id2 = response2.headers().get("x-request-id").unwrap().to_str().unwrap();
        assert_ne!(id1, id2);
    }
}