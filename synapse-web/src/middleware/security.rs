use crate::utils::auth::resolve_request_id;
use axum::body::Body;
use axum::http::{HeaderValue, Request};
use axum::middleware::Next;
use axum::response::IntoResponse;
use axum::response::Response;
use std::time::Instant;
use synapse_common::error::ApiError;

pub async fn logging_middleware(request: Request<Body>, next: axum::middleware::Next) -> Response {
    let start = Instant::now();
    let method = request.method().clone();
    let uri = request.uri().clone();
    let authenticated = request
        .headers()
        .get("authorization")
        .and_then(|h| h.to_str().ok())
        .and_then(|s| s.strip_prefix("Bearer "))
        .is_some();

    let mut headers = request.headers().clone();
    headers.remove("authorization");
    headers.remove("cookie");

    let response = next.run(request).await;

    let duration = start.elapsed();
    let status = response.status();

    tracing::info!(
        "Request: {} {} {} {} {:?} {}ms",
        if authenticated { "authenticated" } else { "anonymous" },
        method,
        uri,
        status.as_u16(),
        headers,
        duration.as_millis()
    );

    response
}

pub async fn security_headers_middleware(request: Request<Body>, next: axum::middleware::Next) -> Response {
    let mut response = next.run(request).await;

    response.headers_mut().insert(
        "Content-Security-Policy",
        HeaderValue::from_static(
            "default-src 'none'; \
             script-src 'self' 'wasm-unsafe-eval'; \
             style-src 'self' 'unsafe-inline'; \
             img-src 'self' data: blob: mxc:; \
             media-src 'self' mxc:; \
             connect-src 'self' wss:; \
             frame-src 'none'; \
             object-src 'none'; \
             base-uri 'self'; \
             form-action 'self'",
        ),
    );

    response.headers_mut().insert(
        "Permissions-Policy",
        HeaderValue::from_static(
            "camera=(), microphone=(), geolocation=(), \
             payment=(), usb=(), magnetometer=(), \
             gyroscope=(), accelerometer=(), \
             interest-cohort=()",
        ),
    );

    // X-Content-Type-Options: 防止 MIME 类型嗅探，全局强制
    response.headers_mut().insert("X-Content-Type-Options", HeaderValue::from_static("nosniff"));

    // HSTS: 默认启用，max-age=31536000（1年），包含子域名
    // 可通过 HSTS_MAX_AGE_SECS=0 禁用
    let hsts_max_age: u64 = std::env::var("HSTS_MAX_AGE_SECS").ok().and_then(|s| s.parse().ok()).unwrap_or(31536000);
    if hsts_max_age > 0 {
        let hsts_value = if std::env::var("HSTS_INCLUDE_SUB_DOMAINS").unwrap_or_default().to_lowercase() == "true" {
            format!("max-age={hsts_max_age}; includeSubDomains")
        } else {
            format!("max-age={hsts_max_age}")
        };
        if let Ok(value) = HeaderValue::from_str(&hsts_value) {
            response.headers_mut().insert("Strict-Transport-Security", value);
        }
    }

    response
}

pub async fn metrics_middleware(request: Request<Body>, next: axum::middleware::Next) -> Response {
    let start = Instant::now();
    let method = request.method().clone();
    let path = request.uri().path().to_string();

    let response = next.run(request).await;
    let duration = start.elapsed();
    let status = response.status().as_u16();

    tracing::debug!("{} {} {} {}ms", method, path, status, duration.as_millis());

    response
}

pub async fn request_debug_middleware(request: Request<Body>, next: Next) -> Response {
    let debug = tracing::enabled!(tracing::Level::DEBUG);
    let method = debug.then(|| request.method().clone());
    let path = debug.then(|| request.uri().path().to_string());
    if let (Some(ref method), Some(ref path)) = (&method, &path) {
        tracing::debug!("Processing request: {} {}", method, path);
    }

    let response = next.run(request).await;

    if let (Some(method), Some(path)) = (method, path) {
        tracing::debug!("Completed request: {} {} - {}", method, path, response.status());
    }

    response
}

pub async fn request_timeout_middleware(request: Request<Body>, next: Next) -> Response {
    let timeout_secs = resolve_request_timeout_secs(&request);

    let result = tokio::time::timeout(std::time::Duration::from_secs(timeout_secs), next.run(request)).await;

    match result {
        Ok(response) => response,
        Err(_) => {
            tracing::warn!("Request timeout after {}s", timeout_secs);
            ApiError::request_timeout(format!("Request processing exceeded server timeout of {}s", timeout_secs))
                .into_response()
        }
    }
}

fn resolve_request_timeout_secs(request: &Request<Body>) -> u64 {
    let path = request.uri().path();
    let default_timeout_secs = std::env::var("REQUEST_TIMEOUT_SECS").ok().and_then(|s| s.parse().ok()).unwrap_or(30);
    if !is_long_polling_endpoint(path) {
        return default_timeout_secs;
    }

    let long_poll_timeout_secs =
        std::env::var("LONG_POLL_REQUEST_TIMEOUT_SECS").ok().and_then(|s| s.parse().ok()).unwrap_or(90);

    let query_timeout_secs =
        parse_timeout_query_secs(request.uri().query()).map_or(0, |timeout_secs| timeout_secs.saturating_add(15));

    long_poll_timeout_secs.max(query_timeout_secs)
}

fn parse_timeout_query_secs(query: Option<&str>) -> Option<u64> {
    let query = query?;
    for (key, value) in url::form_urlencoded::parse(query.as_bytes()) {
        if key == "timeout" {
            let timeout_ms = value.parse::<u64>().ok()?;
            return Some(timeout_ms.div_ceil(1000));
        }
    }
    None
}

fn is_long_polling_endpoint(path: &str) -> bool {
    path.ends_with("/sync")
        || path.ends_with("/events")
        || path.contains("/_matrix/client/unstable/org.matrix.msc3575/sync")
        || path.contains("/_matrix/client/unstable/org.matrix.simplified_msc3575/sync")
}

pub async fn request_id_middleware(mut request: Request<Body>, next: Next) -> Response {
    let request_id = resolve_request_id(request.headers());

    if let Ok(v) = HeaderValue::from_str(&request_id) {
        request.headers_mut().insert("x-request-id", v);
    }

    let mut response = next.run(request).await;

    if let Ok(v) = HeaderValue::from_str(&request_id) {
        response.headers_mut().insert("x-request-id", v);
    }

    response
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::http::StatusCode;
    use axum::{middleware, routing::get, Router};
    use std::time::Duration;
    use synapse_services::test_utils::EnvGuard;
    use tower::ServiceExt;

    #[test]
    fn test_parse_timeout_query_secs() {
        assert_eq!(parse_timeout_query_secs(Some("since=1&timeout=90000&foo=bar")), Some(90));
        assert_eq!(parse_timeout_query_secs(Some("timeout=30001")), Some(31));
        assert_eq!(parse_timeout_query_secs(Some("timeout=abc")), None);
        assert_eq!(parse_timeout_query_secs(None), None);
    }

    #[tokio::test(start_paused = true)]
    async fn test_request_timeout_middleware_allows_sync_long_poll_request() {
        async fn slow_sync_handler() -> StatusCode {
            tokio::time::sleep(Duration::from_secs(90)).await;
            StatusCode::OK
        }

        let _env_lock = synapse_services::test_utils::env_lock_async().await;
        let mut env_guard = EnvGuard::new();
        env_guard.set("REQUEST_TIMEOUT_SECS", "30");
        env_guard.set("LONG_POLL_REQUEST_TIMEOUT_SECS", "90");

        let app = Router::new()
            .route("/_matrix/client/r0/sync", get(slow_sync_handler))
            .layer(middleware::from_fn(request_timeout_middleware));
        let request = Request::builder()
            .method(axum::http::Method::GET)
            .uri("/_matrix/client/r0/sync?timeout=90000")
            .body(Body::empty())
            .expect("request should build");

        let response_task =
            tokio::spawn(async move { app.oneshot(request).await.expect("sync request should succeed") });
        tokio::task::yield_now().await;
        tokio::time::advance(Duration::from_secs(90)).await;

        let response = response_task.await.expect("join should succeed");
        let status = response.status();
        let body = axum::body::to_bytes(response.into_body(), 1024).await.expect("body should be readable");
        let body_text = String::from_utf8_lossy(&body);

        assert_eq!(status, StatusCode::OK);
        assert!(!body_text.contains("M_REQUEST_TIMEOUT"));
    }

    #[tokio::test(start_paused = true)]
    async fn test_request_timeout_middleware_times_out_non_sync_request() {
        async fn slow_handler() -> StatusCode {
            tokio::time::sleep(Duration::from_secs(40)).await;
            StatusCode::OK
        }

        let _env_lock = synapse_services::test_utils::env_lock_async().await;
        let mut env_guard = EnvGuard::new();
        env_guard.set("REQUEST_TIMEOUT_SECS", "30");
        env_guard.set("LONG_POLL_REQUEST_TIMEOUT_SECS", "90");

        let app = Router::new()
            .route("/rooms/test/send", get(slow_handler))
            .layer(middleware::from_fn(request_timeout_middleware));
        let request = Request::builder()
            .method(axum::http::Method::GET)
            .uri("/rooms/test/send")
            .body(Body::empty())
            .expect("request should build");

        let response_task =
            tokio::spawn(
                async move { app.oneshot(request).await.expect("request should succeed with timeout response") },
            );
        tokio::task::yield_now().await;
        tokio::time::advance(Duration::from_secs(31)).await;

        let response = response_task.await.expect("join should succeed");
        let status = response.status();
        let body = axum::body::to_bytes(response.into_body(), 1024).await.expect("body should be readable");
        let json: serde_json::Value = serde_json::from_slice(&body).expect("response should be json");

        assert_eq!(status, StatusCode::REQUEST_TIMEOUT);
        assert_eq!(json["errcode"], "M_REQUEST_TIMEOUT");
        assert!(json["error"].as_str().expect("error should be string").contains("30"));
    }
}
