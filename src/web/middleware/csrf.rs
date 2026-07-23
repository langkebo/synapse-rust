use super::{extract_origin_candidate, is_origin_allowed, is_safe_http_method, same_origin};
use crate::common::error::ApiError;
use crate::web::routes::context::CoreContext;
use axum::extract::State;
use axum::http::{HeaderMap, HeaderValue, Request};
use axum::response::{IntoResponse, Response};
use axum::{body::Body, middleware::Next};
use std::time::{SystemTime, UNIX_EPOCH};

const ADMIN_TOKEN_TTL_SECS: u64 = 24 * 3600;

pub struct CsrfTokenManager {
    secret: String,
    token_ttl: std::time::Duration,
}

impl CsrfTokenManager {
    pub fn new(secret: String) -> Self {
        Self { secret, token_ttl: std::time::Duration::from_secs(ADMIN_TOKEN_TTL_SECS) }
    }

    /// Generate a CSRF token bound to `session_id`.
    ///
    /// Returns `None` when the system clock is before `UNIX_EPOCH` (an
    /// exceptionally rare pathological state). Callers should treat `None` as
    /// "no token can be issued right now" and skip emitting the header — this
    /// is the fail-closed behavior: an unissuable token cannot be valid, so
    /// subsequent unsafe-method requests will be rejected by `validate_token`.
    pub fn generate_token(&self, session_id: &str) -> Option<String> {
        let issued_at = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|duration| duration.as_secs())
            .map_err(|e| tracing::error!("CSRF token issuance failed: system clock before UNIX_EPOCH: {e}"))
            .ok()?;
        let payload = format!("{session_id}:{issued_at}");
        let signature = crate::common::crypto::compute_hash(format!("{}{}", payload, self.secret));
        Some(format!("{payload}:{signature}"))
    }

    pub fn validate_token(&self, token: &str, session_id: &str) -> bool {
        let parts: Vec<&str> = token.split(':').collect();
        if parts.len() != 3 {
            return false;
        }

        if parts[0] != session_id {
            return false;
        }

        let issued_at = match parts[1].parse::<u64>() {
            Ok(issued_at) => issued_at,
            Err(_) => return false,
        };
        // Fail-closed: if the system clock is before UNIX_EPOCH we cannot
        // establish a trustworthy `now`, so reject the token outright rather
        // than risk treating it as never-expiring (which `unwrap_or_default()`
        // + `saturating_sub` would do).
        let now = match SystemTime::now().duration_since(UNIX_EPOCH) {
            Ok(duration) => duration.as_secs(),
            Err(e) => {
                tracing::error!("CSRF token validation failed: system clock before UNIX_EPOCH: {e}");
                return false;
            }
        };
        if now.saturating_sub(issued_at) > self.token_ttl.as_secs() {
            return false;
        }

        let expected_signature =
            crate::common::crypto::compute_hash(format!("{}:{}{}", parts[0], parts[1], self.secret));
        crate::common::crypto::secure_compare(&expected_signature, parts[2])
    }
}

fn extract_cookie_session_id_for_csrf(headers: &HeaderMap) -> Option<String> {
    headers.get("cookie").and_then(|value| value.to_str().ok()).and_then(|cookie_str| {
        // Parse the specific session cookie instead of using the entire cookie string
        cookie_str
            .split(';')
            .filter_map(|pair| {
                let mut parts = pair.trim().splitn(2, '=');
                let name = parts.next()?.trim();
                let value = parts.next()?.trim();
                // Look for common session cookie names
                if name == "sid" || name == "session_id" || name == "sessionid" {
                    Some(format!("{name}={value}"))
                } else {
                    None
                }
            })
            .next()
    })
}

pub async fn csrf_middleware(State(ctx): State<CoreContext>, request: Request<Body>, next: Next) -> Response {
    let csrf_manager = CsrfTokenManager::new(ctx.config.security.csrf_secret.clone());
    let method = request.method().clone();
    let headers = request.headers().clone();
    let session_id = extract_cookie_session_id_for_csrf(&headers);
    let request_origin = extract_origin_candidate(&headers);
    let browser_authenticated_request = session_id.is_some() && request_origin.is_some();

    if !is_safe_http_method(&method) && browser_authenticated_request {
        let origin = request_origin.unwrap_or_default();
        if !same_origin(&origin, &headers) && !is_origin_allowed(&origin) {
            tracing::warn!("CSRF origin rejected: {}", origin);
            return ApiError::forbidden("Cross-site requests are not allowed").into_response();
        }

        let csrf_token = headers.get("x-csrf-token").and_then(|value| value.to_str().ok());

        match (session_id.as_deref(), csrf_token) {
            (Some(session), Some(token)) if csrf_manager.validate_token(token, session) => {}
            _ => return ApiError::forbidden("Missing or invalid CSRF token").into_response(),
        }
    }

    let mut response = next.run(request).await;

    if is_safe_http_method(&method) {
        if let Some(session_id) = session_id {
            if let Some(token) = csrf_manager.generate_token(&session_id) {
                if let Ok(value) = HeaderValue::from_str(&token) {
                    response.headers_mut().insert("x-csrf-token", value);
                }
            }
        }
    }

    response
}

#[cfg(test)]
mod tests {
    use super::*;
    #[cfg(feature = "test-utils")]
    use crate::cache::{CacheConfig, CacheManager};
    #[cfg(feature = "test-utils")]
    use crate::web::routes::AppState;
    #[cfg(feature = "test-utils")]
    use axum::http::StatusCode;
    #[cfg(feature = "test-utils")]
    use axum::{middleware, routing::post, Router};
    #[cfg(feature = "test-utils")]
    use std::sync::Arc;
    use std::time::Duration;
    #[cfg(feature = "test-utils")]
    use synapse_services::ServiceContainer;
    #[cfg(feature = "test-utils")]
    use tower::ServiceExt;

    // Serializes the tests that mutate the process-global TRUST_FORWARDED_HEADERS
    // flag so they never overlap under parallel/shuffled test execution. Lock is
    // poison-tolerant because these tests assert (and may panic) while holding it.
    static FORWARDED_TRUST_LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());

    fn lock_forwarded_trust() -> std::sync::MutexGuard<'static, ()> {
        FORWARDED_TRUST_LOCK.lock().unwrap_or_else(|e| e.into_inner())
    }

    #[test]
    fn test_csrf_token_round_trip() {
        let manager = CsrfTokenManager::new("secret".to_string());
        let token =
            manager.generate_token("session-123").expect("system clock should be after unix epoch in test environment");

        assert!(manager.validate_token(&token, "session-123"));
        assert!(!manager.validate_token(&token, "other-session"));
    }

    #[test]
    fn test_csrf_token_expiration_is_enforced() {
        let manager = CsrfTokenManager { secret: "secret".to_string(), token_ttl: Duration::from_secs(1) };
        let old_timestamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("system clock should be after unix epoch")
            .as_secs()
            .saturating_sub(10);
        let payload = format!("session-123:{old_timestamp}");
        let signature = crate::common::crypto::compute_hash(format!("{}{}", payload, manager.secret));
        let token = format!("{}:{}", payload, &signature[..16]);

        assert!(!manager.validate_token(&token, "session-123"));
    }

    /// Regression test for the fail-closed clock-regression BUG.
    ///
    /// Previously `generate_token`/`validate_token` used
    /// `SystemTime::now().duration_since(UNIX_EPOCH).unwrap_or_default()`,
    /// which silently returned `0` when the system clock was before
    /// `UNIX_EPOCH`. Combined with `saturating_sub`, a token issued under a
    /// broken clock could be treated as never-expiring.
    ///
    /// The fix changes `generate_token` to return `Option<String>` (None when
    /// the clock is broken) and `validate_token` to reject tokens when the
    /// clock is broken. Under a normal clock, `generate_token` must still
    /// return `Some`.
    #[test]
    fn test_generate_token_returns_some_under_normal_clock() {
        let manager = CsrfTokenManager::new("secret".to_string());
        assert!(manager.generate_token("session-123").is_some());
    }

    #[test]
    fn test_extract_cookie_session_id_for_csrf_only_uses_cookie() {
        let mut headers = HeaderMap::new();
        headers.insert("authorization", "Bearer access-token".parse().expect("valid auth header"));
        headers.insert("cookie", "sid=session-cookie".parse().expect("valid cookie header"));

        assert_eq!(extract_cookie_session_id_for_csrf(&headers), Some("sid=session-cookie".to_string()));

        let mut cookie_only_headers = HeaderMap::new();
        cookie_only_headers.insert("cookie", "sid=session-cookie".parse().expect("valid cookie header"));
        assert_eq!(extract_cookie_session_id_for_csrf(&cookie_only_headers), Some("sid=session-cookie".to_string()));

        let mut auth_only_headers = HeaderMap::new();
        auth_only_headers.insert("authorization", "Bearer access-token".parse().expect("valid auth header"));
        assert_eq!(extract_cookie_session_id_for_csrf(&auth_only_headers), None);
    }

    #[cfg(feature = "test-utils")]
    #[tokio::test]
    async fn test_csrf_middleware_rejects_cross_site_cookie_post_without_token() {
        async fn ok_handler() -> StatusCode {
            StatusCode::OK
        }

        let mut services = ServiceContainer::new_test().await;
        services.core.server_name = "matrix.example.com".to_string();

        let cache = Arc::new(CacheManager::new(&CacheConfig::default()));
        let state = AppState::new(services, cache);

        let app = Router::new()
            .route("/submit", post(ok_handler))
            .layer(middleware::from_fn_with_state(state.clone(), csrf_middleware))
            .with_state(state);

        let request = Request::builder()
            .method(axum::http::Method::POST)
            .uri("/submit")
            .header("cookie", "sid=session-cookie")
            .header("origin", "https://evil.example.com")
            .header("host", "matrix.example.com")
            .body(Body::empty())
            .expect("request should build");

        let response = app.oneshot(request).await.expect("request should succeed");
        let status = response.status();
        let body = axum::body::to_bytes(response.into_body(), 1024).await.expect("body should be readable");
        let json: serde_json::Value = serde_json::from_slice(&body).expect("response should be json");

        assert_eq!(status, StatusCode::FORBIDDEN);
        assert_eq!(json["error"], "Cross-site requests are not allowed");
    }

    #[cfg(feature = "test-utils")]
    #[tokio::test]
    async fn test_csrf_middleware_rejects_same_origin_cookie_post_without_token() {
        async fn ok_handler() -> StatusCode {
            StatusCode::OK
        }

        let mut services = ServiceContainer::new_test().await;
        services.core.server_name = "matrix.example.com".to_string();

        let cache = Arc::new(CacheManager::new(&CacheConfig::default()));
        let state = AppState::new(services, cache);

        let app = Router::new()
            .route("/submit", post(ok_handler))
            .layer(middleware::from_fn_with_state(state.clone(), csrf_middleware))
            .with_state(state);

        let request = Request::builder()
            .method(axum::http::Method::POST)
            .uri("/submit")
            .header("cookie", "sid=session-cookie")
            .header("origin", "https://matrix.example.com")
            .header("host", "matrix.example.com")
            .body(Body::empty())
            .expect("request should build");

        let response = app.oneshot(request).await.expect("request should succeed");
        let status = response.status();
        let body = axum::body::to_bytes(response.into_body(), 1024).await.expect("body should be readable");
        let json: serde_json::Value = serde_json::from_slice(&body).expect("response should be json");

        assert_eq!(status, StatusCode::FORBIDDEN);
        assert_eq!(json["error"], "Missing or invalid CSRF token");
    }

    #[cfg(feature = "test-utils")]
    #[tokio::test]
    async fn test_csrf_middleware_allows_same_origin_cookie_post_with_valid_token() {
        async fn ok_handler() -> StatusCode {
            StatusCode::OK
        }

        let mut services = ServiceContainer::new_test().await;
        services.core.server_name = "matrix.example.com".to_string();

        let csrf_manager = CsrfTokenManager::new(services.core.config.security.csrf_secret.clone());
        let session_id = "sid=session-cookie";
        let csrf_token = csrf_manager
            .generate_token(session_id)
            .expect("system clock should be after unix epoch in test environment");

        let cache = Arc::new(CacheManager::new(&CacheConfig::default()));
        let state = AppState::new(services, cache);

        let app = Router::new()
            .route("/submit", post(ok_handler))
            .layer(middleware::from_fn_with_state(state.clone(), csrf_middleware))
            .with_state(state);

        let request = Request::builder()
            .method(axum::http::Method::POST)
            .uri("/submit")
            .header("cookie", session_id)
            .header("origin", "https://matrix.example.com")
            .header("host", "matrix.example.com")
            .header("x-csrf-token", csrf_token)
            .body(Body::empty())
            .expect("request should build");

        let response = app.oneshot(request).await.expect("request should succeed");

        assert_eq!(response.status(), StatusCode::OK);
    }

    #[test]
    fn test_extract_origin_candidate_uses_origin_or_referer() {
        let mut headers = HeaderMap::new();
        headers.insert("origin", "https://app.example.com".parse().expect("valid origin header"));
        assert_eq!(extract_origin_candidate(&headers), Some("https://app.example.com".to_string()));

        let mut referer_headers = HeaderMap::new();
        referer_headers
            .insert("referer", "https://app.example.com/path?query=1".parse().expect("valid referer header"));
        assert_eq!(extract_origin_candidate(&referer_headers), Some("https://app.example.com".to_string()));
    }

    #[test]
    fn test_same_origin_ignores_forwarded_headers_by_default() {
        let _guard = lock_forwarded_trust();
        super::super::set_trust_forwarded_headers(false);

        let mut headers = HeaderMap::new();
        headers.insert("x-forwarded-host", "matrix.example.com".parse().expect("valid host header"));
        headers.insert("x-forwarded-proto", "https".parse().expect("valid proto header"));

        assert!(!same_origin("https://matrix.example.com", &headers));
    }

    #[test]
    fn test_same_origin_uses_host_header_when_forwarded_not_trusted() {
        let _guard = lock_forwarded_trust();
        super::super::set_trust_forwarded_headers(false);

        let mut headers = HeaderMap::new();
        headers.insert("host", "matrix.example.com".parse().expect("valid host header"));
        headers.insert("x-forwarded-host", "evil.example.com".parse().expect("valid host header"));

        assert!(same_origin("https://matrix.example.com", &headers));
        assert!(!same_origin("https://evil.example.com", &headers));
    }

    #[test]
    fn test_same_origin_uses_forwarded_host_and_proto_when_trusted() {
        let _guard = lock_forwarded_trust();
        super::super::set_trust_forwarded_headers(true);

        let mut headers = HeaderMap::new();
        headers.insert("x-forwarded-host", "matrix.example.com".parse().expect("valid host header"));
        headers.insert("x-forwarded-proto", "https".parse().expect("valid proto header"));

        assert!(same_origin("https://matrix.example.com", &headers));
        assert!(!same_origin("https://other.example.com", &headers));

        super::super::set_trust_forwarded_headers(false);
    }
}
