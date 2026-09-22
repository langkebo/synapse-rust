/// The `auth` module.
pub mod auth;
/// The `cors` module.
pub mod cors;
/// The `csrf` module.
pub mod csrf;
/// The `federation_auth` module.
pub mod federation_auth;
/// The `federation_rate_limit` module.
pub mod federation_rate_limit;
/// The `http_metrics` module.
pub mod http_metrics;
/// The `rate_limit` module.
pub mod rate_limit;
/// The `security` module.
pub mod security;

pub use auth::*;
pub use cors::*;
pub use csrf::*;
pub use federation_auth::*;
pub use federation_rate_limit::*;
pub use http_metrics::*;
pub use rate_limit::*;
pub use security::*;

use axum::http::{HeaderMap, Method};
use regex::Regex;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::LazyLock;
use url::Url;

static CORS_ORIGINS_REGEX: LazyLock<Option<Regex>> = LazyLock::new(|| {
    std::env::var("CORS_ORIGIN_PATTERN").ok().and_then(|pattern| match Regex::new(&pattern) {
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

/// See [`set_bind_address`].
pub fn set_bind_address(addr: String) {
    let _ = BIND_ADDRESS.set(addr);
}

/// See [`set_trust_forwarded_headers`].
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

/// See [`is_forwarded_headers_trusted`].
pub(crate) fn is_forwarded_headers_trusted() -> bool {
    TRUST_FORWARDED_HEADERS.load(Ordering::SeqCst)
}

/// See [`is_localhost_bind`].
pub(crate) fn is_localhost_bind() -> bool {
    BIND_ADDRESS.get().is_some_and(|addr| is_local_bind_address(addr))
}

/// WEB-03: 纯函数判定绑定地址是否为本机地址。
/// 0.0.0.0 / :: / [::] 是「所有接口」通配地址，**不是** localhost——
/// 此前被算作 localhost 时，dev 模式 CORS 全开放会暴露到整个网络。
pub(crate) fn is_local_bind_address(addr: &str) -> bool {
    let host = addr.to_lowercase();
    host == "127.0.0.1" || host == "localhost" || host == "::1" || host.starts_with("127.")
}

/// See [`is_dev_mode`].
pub(crate) fn is_dev_mode() -> bool {
    std::env::var("RUST_ENV").unwrap_or_else(|_| "production".to_string()).to_lowercase() == "development"
}

/// See [`get_allowed_origins`].
pub(crate) fn get_allowed_origins() -> Vec<String> {
    if let Ok(env_value) = std::env::var("ALLOWED_ORIGINS") {
        let parsed: Vec<String> =
            env_value.split(',').map(|v| v.trim().to_string()).filter(|v| !v.is_empty()).collect();
        if !parsed.is_empty() {
            return parsed;
        }
    }

    CONFIG_ALLOWED_ORIGINS.get().cloned().unwrap_or_default()
}

/// See [`is_origin_allowed`].
pub(crate) fn is_origin_allowed(origin: &str) -> bool {
    if is_dev_mode() && is_localhost_bind() {
        return true;
    }

    let allowed_origins = get_allowed_origins();
    if allowed_origins.iter().any(|o| o == "*") {
        return true;
    }

    let in_list = allowed_origins.iter().any(|o| normalize_origin(o) == normalize_origin(origin));
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

/// See [`normalize_origin`].
pub(crate) fn normalize_origin(origin: &str) -> String {
    let normalized = origin.trim_end_matches('/').to_lowercase();
    let parts: Vec<&str> = normalized.split("://").collect();
    if parts.len() == 2 {
        format!("{}://{}", parts[0], parts[1])
    } else {
        normalized
    }
}

/// See [`extract_request_origin`].
pub(crate) fn extract_request_origin(headers: &HeaderMap) -> Option<String> {
    let host = if is_forwarded_headers_trusted() {
        headers.get("x-forwarded-host").or_else(|| headers.get("host")).and_then(|value| value.to_str().ok())?
    } else {
        headers.get("host").and_then(|value| value.to_str().ok())?
    };

    let scheme = if is_forwarded_headers_trusted() {
        headers.get("x-forwarded-proto").and_then(|value| value.to_str().ok()).unwrap_or("https")
    } else {
        "https"
    };

    Some(normalize_origin(&format!("{scheme}://{host}")))
}

/// See [`same_origin`].
pub(crate) fn same_origin(request_origin: &str, headers: &HeaderMap) -> bool {
    extract_request_origin(headers).is_some_and(|server_origin| normalize_origin(request_origin) == server_origin)
}

/// See [`is_safe_http_method`].
pub(crate) fn is_safe_http_method(method: &Method) -> bool {
    matches!(*method, Method::GET | Method::HEAD | Method::OPTIONS | Method::TRACE)
}

/// See [`extract_origin_candidate`].
pub(crate) fn extract_origin_candidate(headers: &HeaderMap) -> Option<String> {
    headers.get("origin").and_then(|value| value.to_str().ok()).map(|value| value.to_string()).or_else(|| {
        headers
            .get("referer")
            .and_then(|value| value.to_str().ok())
            .and_then(|value| Url::parse(value).ok())
            .map(|value| value.origin().ascii_serialization())
    })
}

/// See [`cors_origins_regex`].
pub(crate) fn cors_origins_regex() -> Option<&'static Regex> {
    CORS_ORIGINS_REGEX.as_ref()
}

/// See [`set_config_allowed_origins_once`].
pub(crate) fn set_config_allowed_origins_once(origins: Vec<String>) {
    let _ = CONFIG_ALLOWED_ORIGINS.set(origins);
}

#[cfg(test)]
mod tests {
    use super::*;

    // WEB-03: is_local_bind_address 纯函数判定
    #[test]
    fn web03_local_bind_addresses_accepted() {
        for addr in ["127.0.0.1", "localhost", "LOCALHOST", "::1", "127.0.0.5", "127.1.2.3"] {
            assert!(is_local_bind_address(addr), "{addr} should be recognized as local");
        }
    }

    #[test]
    fn web03_wildcard_and_remote_addresses_rejected() {
        for addr in ["0.0.0.0", "::", "[::]", "192.168.1.1", "10.0.0.1", "example.com", "172.16.0.1"] {
            assert!(!is_local_bind_address(addr), "{addr} must NOT be recognized as local");
        }
    }
}
