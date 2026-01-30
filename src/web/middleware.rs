use crate::cache::*;
use crate::AppState;
use axum::http::{HeaderMap, Request, StatusCode};
use axum::{body::Body, response::Response};
use std::sync::Arc;
use std::time::Instant;

pub async fn logging_middleware(request: Request<Body>, next: axum::middleware::Next) -> Response {
    let start = Instant::now();
    let method = request.method().clone();
    let uri = request.uri().clone();
    let headers = request.headers().clone();

    let user_id = headers
        .get("authorization")
        .and_then(|h| h.to_str().ok())
        .and_then(|s| s.strip_prefix("Bearer "))
        .map(|s| s.to_string());

    let response = next.run(request).await;

    let duration = start.elapsed();
    let status = response.status();

    tracing::info!(
        "Request: {} {} {} {} {:?} {}ms",
        user_id.unwrap_or_else(|| "anonymous".to_string()),
        method,
        uri,
        status.as_u16(),
        headers,
        duration.as_millis()
    );

    response
}

pub async fn cors_middleware(request: Request<Body>, next: axum::middleware::Next) -> Response {
    let mut response = next.run(request).await;

    let headers = response.headers_mut();
    headers.insert("Access-Control-Allow-Origin", "*".parse().unwrap());
    headers.insert(
        "Access-Control-Allow-Methods",
        "GET, POST, PUT, DELETE, OPTIONS".parse().unwrap(),
    );
    headers.insert(
        "Access-Control-Allow-Headers",
        "Content-Type, Authorization, X-Requested-With"
            .parse()
            .unwrap(),
    );
    headers.insert("Access-Control-Max-Age", "86400".parse().unwrap());

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

pub fn extract_token(headers: &HeaderMap) -> Option<String> {
    headers
        .get("authorization")
        .and_then(|v| v.to_str().ok())
        .and_then(|s| s.strip_prefix("Bearer "))
        .map(|s| s.to_string())
}

pub async fn rate_limit_middleware(
    request: Request<Body>,
    next: axum::middleware::Next,
    _cache: Arc<CacheManager>,
) -> Result<Response, StatusCode> {
    let ip = request
        .headers()
        .get("x-forwarded-for")
        .or(request.headers().get("x-real-ip"))
        .and_then(|v| v.to_str().ok())
        .unwrap_or("unknown")
        .to_string();

    tracing::debug!("Rate limit check for IP: {}", ip);

    Ok(next.run(request).await)
}

pub async fn auth_middleware(
    request: Request<Body>,
    next: axum::middleware::Next,
    _state: Arc<AppState>,
) -> Result<Response, StatusCode> {
    let token = extract_token(request.headers());

    if token.is_none() {
        return Ok(Response::builder()
            .status(StatusCode::UNAUTHORIZED)
            .body(Body::from(
                r#"{"errcode": "UNAUTHORIZED", "error": "Missing access token"}"#,
            ))
            .unwrap());
    }

    Ok(next.run(request).await)
}
