pub mod auth;
pub mod cors;
pub mod csrf;
pub mod federation_auth;
pub mod rate_limit;
pub mod security;

pub use auth::*;
pub use cors::*;
pub use csrf::*;
pub use federation_auth::*;
pub use rate_limit::*;
pub use security::*;

use axum::http::{HeaderMap, Method};
use std::sync::LazyLock;
use std::sync::atomic::{AtomicBool, Ordering};
use regex::Regex;
use url::Url;

static CORS_ORIGINS_REGEX: LazyLock<Option<Regex>> = LazyLock::new(|| {
    std::env::var("CORS_ORIGIN_PATTERN")
        .ok()
        .and_then(|pattern| match Regex::new(&pattern) {
            Ok(regex) => Some(regex),
            Err(e) => {
                tracing::error!("Invalid CORS_ORIGIN_PATTERN regex '{}': {}", pattern, e);
                None
            }
        })
});

static CONFIG_ALLOWED_ORIGINS: std::sync::OnceLock<Vec<String>> = std::sync::OnceLock::new();
static BIND_ADDRESS: std::sync::OnceLock<String> = std::sync::OnceLock::new();
static TRUST_FORWARDED_HEADERS: AtomicBool = AtomicBool::new(false);

pub fn set_bind_address(addr: String) {
    let _ = BIND_ADDRESS.set(addr);
}

pub fn set_trust_forwarded_headers(trust: bool) {
    let was_trusted = TRUST_FORWARDED_HEADERS.swap(trust, Ordering::SeqCst);
    if trust && !was_trusted {
        tracing::warn!(
            "TRUST_FORWARDED_HEADERS is enabled. Only enable this when running behind a trusted \
             reverse proxy that strips incoming x-forwarded-* headers. \
             If clients can set these headers directly, CSRF same-origin checks can be bypassed."
        );
    }
}

pub(crate) fn is_forwarded_headers_trusted() -> bool {
    TRUST_FORWARDED_HEADERS.load(Ordering::SeqCst)
}

pub(crate) fn is_localhost_bind() -> bool {
    BIND_ADDRESS
        .get()
        .is_some_and(|addr| {
            let host = addr.to_lowercase();
            host == "127.0.0.1"
                || host == "localhost"
                || host == "::1"
                || host == "0.0.0.0"
                || host == "::"
                || host == "[::]"
                || host.starts_with("127.")
        })
}

pub(crate) fn is_dev_mode() -> bool {
    std::env::var("RUST_ENV")
        .unwrap_or_else(|_| "production".to_string())
        .to_lowercase()
        == "development"
}

pub(crate) fn get_allowed_origins() -> Vec<String> {
    if let Ok(env_value) = std::env::var("ALLOWED_ORIGINS") {
        let parsed: Vec<String> = env_value
            .split(',')
            .map(|v| v.trim().to_string())
            .filter(|v| !v.is_empty())
            .collect();
        if !parsed.is_empty() {
            return parsed;
        }
    }

    CONFIG_ALLOWED_ORIGINS.get().cloned().unwrap_or_default()
}

pub(crate) fn is_origin_allowed(origin: &str) -> bool {
    if is_dev_mode() && is_localhost_bind() {
        return true;
    }

    let allowed_origins = get_allowed_origins();
    if allowed_origins.iter().any(|o| o == "*") {
        return true;
    }

    let in_list = allowed_origins
        .iter()
        .any(|o| normalize_origin(o) == normalize_origin(origin));
    if in_list {
        return true;
    }

    if let Some(ref pattern) = *CORS_ORIGINS_REGEX {
        if pattern.is_match(origin) {
            return true;
        }
    }

    false
}

pub(crate) fn normalize_origin(origin: &str) -> String {
    let normalized = origin.trim_end_matches('/').to_lowercase();
    let parts: Vec<&str> = normalized.split("://").collect();
    if parts.len() == 2 {
        format!("{}://{}", parts[0], parts[1])
    } else {
        normalized
    }
}

pub(crate) fn extract_request_origin(headers: &HeaderMap) -> Option<String> {
    let host = if is_forwarded_headers_trusted() {
        headers
            .get("x-forwarded-host")
            .or_else(|| headers.get("host"))
            .and_then(|value| value.to_str().ok())?
    } else {
        headers
            .get("host")
            .and_then(|value| value.to_str().ok())?
    };

    let scheme = if is_forwarded_headers_trusted() {
        headers
            .get("x-forwarded-proto")
            .and_then(|value| value.to_str().ok())
            .unwrap_or("https")
    } else {
        "https"
    };

    Some(normalize_origin(&format!("{scheme}://{host}")))
}

pub(crate) fn same_origin(request_origin: &str, headers: &HeaderMap) -> bool {
    extract_request_origin(headers)
        .is_some_and(|server_origin| normalize_origin(request_origin) == server_origin)
}

pub(crate) fn is_safe_http_method(method: &Method) -> bool {
    matches!(
        *method,
        Method::GET | Method::HEAD | Method::OPTIONS | Method::TRACE
    )
}

pub(crate) fn extract_origin_candidate(headers: &HeaderMap) -> Option<String> {
    headers
        .get("origin")
        .and_then(|value| value.to_str().ok())
        .map(|value| value.to_string())
        .or_else(|| {
            headers
                .get("referer")
                .and_then(|value| value.to_str().ok())
                .and_then(|value| Url::parse(value).ok())
                .map(|value| value.origin().ascii_serialization())
        })
}

pub(crate) fn cors_origins_regex() -> Option<&'static Regex> {
    CORS_ORIGINS_REGEX.as_ref()
}

pub(crate) fn set_config_allowed_origins_once(origins: Vec<String>) {
    let _ = CONFIG_ALLOWED_ORIGINS.set(origins);
}
