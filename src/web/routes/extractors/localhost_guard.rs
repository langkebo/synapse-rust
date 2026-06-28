//! Extract guard that rejects non-localhost requests with 403.
//! Unified implementation for admin registration and other local-only endpoints.
//!
//! This module was extracted from `admin/register.rs` as part of QA optimization
//! (I4 — deduplicate localhost IP validation).

use axum::extract::FromRequestParts;
use axum::http::header;
use axum::http::request::Parts;
use axum::{
    body::Body,
    extract::ConnectInfo,
    http::{HeaderMap, StatusCode},
    response::Response,
};
use std::future::Future;
use std::net::{IpAddr, SocketAddr};
use url::Url;

use crate::web::utils::ip::extract_client_ip;

/// Axum extractor that only allows requests from localhost.
/// Non-local requests receive 403 with a descriptive error.
pub struct LocalhostGuard;

impl<S> FromRequestParts<S> for LocalhostGuard
where
    S: Send + Sync,
{
    type Rejection = Response<Body>;

    fn from_request_parts(parts: &mut Parts, _state: &S) -> impl Future<Output = Result<Self, Self::Rejection>> + Send {
        // Read ConnectInfo without removing it so downstream handlers can also extract it.
        let connect_info = parts.extensions.get::<ConnectInfo<SocketAddr>>().cloned();
        let headers = parts.headers.clone();

        async move {
            let remote_ip = match connect_info {
                Some(ConnectInfo(addr)) => addr.ip(),
                None => {
                    return Err(register_error_response(500, "M_UNKNOWN", "Missing connect info"));
                }
            };

            let is_loopback = remote_ip.is_loopback();
            let is_proxied_localhost = is_local_proxy_ip(remote_ip) && request_targets_localhost(&headers);

            if !is_loopback && !is_proxied_localhost {
                return Err(register_error_response(
                    403,
                    "M_FORBIDDEN",
                    "Admin registration is only available from localhost",
                ));
            }

            // C1: Check forwarded client IP — even when the direct connection appears
            // to be loopback or a trusted proxy, a forwarded-for header may reveal the
            // true client. Reject any non-local client IP.
            if let Some(client_ip) = extract_registration_client_ip(&headers) {
                if !is_local_client_ip(&client_ip) {
                    return Err(register_error_response(
                        403,
                        "M_FORBIDDEN",
                        "Admin registration is only available from localhost",
                    ));
                }
            }

            // C2: Enforce origin header — reject non-local origins arriving over
            // loopback or trusted proxy connections.
            if let Some(origin) = headers.get("origin").and_then(|value| value.to_str().ok()) {
                if !is_local_registration_origin(origin) {
                    return Err(register_error_response(
                        403,
                        "M_FORBIDDEN",
                        "Admin registration origin is not allowed",
                    ));
                }
            }

            // C3: Enforce referer header — apply same local-only policy as origin.
            if let Some(referer) = headers.get("referer").and_then(|value| value.to_str().ok()) {
                if !is_local_registration_origin(referer) {
                    return Err(register_error_response(
                        403,
                        "M_FORBIDDEN",
                        "Admin registration origin is not allowed",
                    ));
                }
            }

            Ok(LocalhostGuard)
        }
    }
}

// ---------------------------------------------------------------------------
// Helper functions (extracted from admin/register.rs)
// ---------------------------------------------------------------------------

/// Builds a JSON error response.
/// Shared between this module and admin/register.rs (for admin-specific policy checks).
pub(crate) fn register_error_response(status: u16, errcode: &str, error: &str) -> Response<Body> {
    let body = serde_json::json!({ "errcode": errcode, "error": error });
    let mut response = Response::new(Body::from(serde_json::to_string(&body).unwrap_or_default()));
    *response.status_mut() = StatusCode::from_u16(status).unwrap_or(StatusCode::INTERNAL_SERVER_ERROR);
    response.headers_mut().insert(header::CONTENT_TYPE, axum::http::HeaderValue::from_static("application/json"));
    response
}

/// Extract the forwarded client IP from request headers using the standard
/// x-forwarded-for / x-real-ip / forwarded priority order.
/// Mirrors `extract_registration_client_ip` from the original admin/register.rs.
fn extract_registration_client_ip(headers: &HeaderMap) -> Option<String> {
    let priority = vec!["x-forwarded-for".to_string(), "x-real-ip".to_string(), "forwarded".to_string()];
    extract_client_ip(headers, &priority)
}

/// Returns true when the supplied string is either the literal "localhost" or
/// a parseable loopback address.
fn is_local_client_ip(ip: &str) -> bool {
    if ip.eq_ignore_ascii_case("localhost") {
        return true;
    }
    ip.parse::<IpAddr>().map(|ip| ip.is_loopback()).unwrap_or(false)
}

fn is_local_registration_origin(value: &str) -> bool {
    if value.eq_ignore_ascii_case("null") {
        return false;
    }
    let Ok(url) = Url::parse(value) else {
        return false;
    };
    let Some(host) = url.host_str() else {
        return false;
    };
    if host.eq_ignore_ascii_case("localhost") {
        return true;
    }
    let normalized_host = host.trim_matches(|c| c == '[' || c == ']');
    normalized_host.parse::<IpAddr>().map(|ip| ip.is_loopback()).unwrap_or(false)
}

fn is_local_registration_host(value: &str) -> bool {
    let candidate = value.split(',').next().map(str::trim).filter(|value| !value.is_empty());

    let Some(candidate) = candidate else {
        return false;
    };

    let candidate = if candidate.contains("://") { candidate.to_string() } else { format!("http://{candidate}") };

    let Ok(url) = Url::parse(&candidate) else {
        return false;
    };
    let Some(host) = url.host_str() else {
        return false;
    };

    if host.eq_ignore_ascii_case("localhost") {
        return true;
    }

    let normalized_host = host.trim_matches(|c| c == '[' || c == ']');
    normalized_host.parse::<IpAddr>().map(|ip| ip.is_loopback()).unwrap_or(false)
}

fn is_local_proxy_ip(ip: IpAddr) -> bool {
    match ip {
        IpAddr::V4(ip) => ip.is_private() || ip.is_link_local(),
        IpAddr::V6(ip) => ip.is_unique_local() || ip.is_unicast_link_local(),
    }
}

fn request_targets_localhost(headers: &HeaderMap) -> bool {
    if headers
        .get("x-forwarded-host")
        .or_else(|| headers.get("host"))
        .and_then(|value| value.to_str().ok())
        .is_some_and(is_local_registration_host)
    {
        return true;
    }

    if headers.get("origin").and_then(|value| value.to_str().ok()).is_some_and(is_local_registration_origin) {
        return true;
    }

    headers.get("referer").and_then(|value| value.to_str().ok()).is_some_and(is_local_registration_origin)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_local_registration_origin_localhost() {
        assert!(is_local_registration_origin("http://localhost:8008"));
        assert!(is_local_registration_origin("https://127.0.0.1:8448"));
        assert!(is_local_registration_origin("http://[::1]:8008"));
    }

    #[test]
    fn test_local_registration_origin_remote() {
        assert!(!is_local_registration_origin("https://evil.example.com"));
        assert!(!is_local_registration_origin("null"));
        assert!(!is_local_registration_origin("http://192.168.1.1:8080"));
    }

    #[test]
    fn test_local_registration_host_localhost() {
        assert!(is_local_registration_host("localhost:8008"));
        assert!(is_local_registration_host("127.0.0.1:8448"));
        assert!(is_local_registration_host("[::1]:8008"));
    }

    #[test]
    fn test_local_registration_host_remote() {
        assert!(!is_local_registration_host("evil.example.com"));
    }

    #[test]
    fn test_is_local_client_ip() {
        assert!(is_local_client_ip("127.0.0.1"));
        assert!(is_local_client_ip("::1"));
        assert!(is_local_client_ip("localhost"));
        assert!(!is_local_client_ip("203.0.113.9"));
        assert!(!is_local_client_ip("evil.example.com"));
    }

    /// Ported from `test_ensure_local_admin_registration_request_rejects_non_local_origin`
    /// in the original admin/register.rs. Verifies that a connection from loopback with a
    /// non-local origin header is rejected.
    #[test]
    fn test_localhost_guard_rejects_non_local_origin() {
        let mut headers = HeaderMap::new();
        headers.insert("origin", "https://evil.example.com".parse().unwrap());
        let uri = "/_synapse/admin/v1/register".parse::<axum::http::Uri>().unwrap();
        let (mut parts, _) = axum::http::Request::new(()).into_parts();
        parts.uri = uri;
        parts.headers = headers;
        parts.extensions.insert(ConnectInfo("127.0.0.1:8008".parse::<SocketAddr>().unwrap()));

        let rt = tokio::runtime::Runtime::new().unwrap();
        let result = rt.block_on(LocalhostGuard::from_request_parts(&mut parts, &()));
        assert!(result.is_err());
    }

    /// Ported from `test_ensure_local_admin_registration_request_accepts_local_origin`
    /// in the original admin/register.rs. Verifies that a connection from loopback with
    /// local origin and referer headers is accepted.
    #[test]
    fn test_localhost_guard_accepts_local_origin() {
        let mut headers = HeaderMap::new();
        headers.insert("origin", "http://localhost:3000".parse().unwrap());
        headers.insert("referer", "http://127.0.0.1:3000/setup".parse().unwrap());
        let uri = "/_synapse/admin/v1/register".parse::<axum::http::Uri>().unwrap();
        let (mut parts, _) = axum::http::Request::new(()).into_parts();
        parts.uri = uri;
        parts.headers = headers;
        parts.extensions.insert(ConnectInfo("127.0.0.1:8008".parse::<SocketAddr>().unwrap()));

        let rt = tokio::runtime::Runtime::new().unwrap();
        let result = rt.block_on(LocalhostGuard::from_request_parts(&mut parts, &()));
        assert!(result.is_ok());
    }
}
