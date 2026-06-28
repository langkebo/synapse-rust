//! Extract guard that rejects non-localhost requests with 403.
//! Unified implementation for admin registration and other local-only endpoints.
//!
//! This module was extracted from `admin/register.rs` as part of QA optimization
//! (I4 — deduplicate localhost IP validation).

use axum::{
    body::Body,
    extract::ConnectInfo,
    http::{HeaderMap, StatusCode},
    response::Response,
};
use axum::extract::FromRequestParts;
use axum::http::request::Parts;
use axum::http::header;
use std::net::{IpAddr, SocketAddr};
use std::future::Future;
use url::Url;

/// Axum extractor that only allows requests from localhost.
/// Non-local requests receive 403 with a descriptive error.
pub struct LocalhostGuard;

impl<S> FromRequestParts<S> for LocalhostGuard
where
    S: Send + Sync,
{
    type Rejection = Response<Body>;

    fn from_request_parts(
        parts: &mut Parts,
        _state: &S,
    ) -> impl Future<Output = Result<Self, Self::Rejection>> + Send {
        // Read ConnectInfo without removing it so downstream handlers can also extract it.
        let connect_info = parts.extensions.get::<ConnectInfo<SocketAddr>>().cloned();
        let headers = parts.headers.clone();

        async move {
            let remote_ip = match connect_info {
                Some(ConnectInfo(addr)) => addr.ip(),
                None => {
                    return Err(register_error_response(
                        500,
                        "M_UNKNOWN",
                        "Missing connect info",
                    ));
                }
            };

            if remote_ip.is_loopback() {
                return Ok(LocalhostGuard);
            }

            // Allow proxied local requests from private IPs
            if is_local_proxy_ip(remote_ip) && request_targets_localhost(&headers) {
                return Ok(LocalhostGuard);
            }

            Err(register_error_response(
                403,
                "M_FORBIDDEN",
                "Admin registration is only available from localhost",
            ))
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
    let mut response = Response::new(Body::from(
        serde_json::to_string(&body).unwrap_or_default(),
    ));
    *response.status_mut() = StatusCode::from_u16(status).unwrap_or(StatusCode::INTERNAL_SERVER_ERROR);
    response.headers_mut().insert(
        header::CONTENT_TYPE,
        axum::http::HeaderValue::from_static("application/json"),
    );
    response
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
    normalized_host
        .parse::<IpAddr>()
        .map(|ip| ip.is_loopback())
        .unwrap_or(false)
}

fn is_local_registration_host(value: &str) -> bool {
    let candidate = value
        .split(',')
        .next()
        .map(str::trim)
        .filter(|value| !value.is_empty());

    let Some(candidate) = candidate else {
        return false;
    };

    let candidate = if candidate.contains("://") {
        candidate.to_string()
    } else {
        format!("http://{candidate}")
    };

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
    normalized_host
        .parse::<IpAddr>()
        .map(|ip| ip.is_loopback())
        .unwrap_or(false)
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

    if headers
        .get("origin")
        .and_then(|value| value.to_str().ok())
        .is_some_and(is_local_registration_origin)
    {
        return true;
    }

    headers
        .get("referer")
        .and_then(|value| value.to_str().ok())
        .is_some_and(is_local_registration_origin)
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
}
