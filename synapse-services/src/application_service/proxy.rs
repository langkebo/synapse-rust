//! MSC4512 — application service proxying.
//!
//! An application service may declare a `proxy_prefix` (a path prefix it wants
//! to serve) together with a `proxy_url` (where those requests are forwarded)
//! in its registration file. When the experimental `msc4512_enabled` flag is
//! on, requests to `/_matrix/{client,federation}/<version>/<proxy_prefix>/...`
//! are forwarded to that URL — see `synapse-web` for the request path.
//!
//! Parity with upstream Synapse is deliberate:
//!
//! * only prefixes listed in [`ALLOWED_PROXY_PREFIXES`] may be claimed — a
//!   bridge must never be able to shadow a generic client-server path;
//! * `proxy_prefix` and `proxy_url` are all-or-nothing;
//! * both are stripped of trailing slashes before use.
//!
//! Both the stable key names (`proxy_prefix` / `proxy_url`) and the MSC's
//! unstable names (`io.element.msc4512.proxy_prefix` /
//! `io.element.msc4512.proxy_url`) are accepted.

use std::sync::{Arc, MutexGuard};
use std::time::{Duration, Instant};

use serde_json::Value;
use synapse_common::ApiError;
use tracing::warn;

use crate::application_service::ApplicationServiceManager;

/// The reserved path prefixes an application service is allowed to claim.
///
/// Mirrors upstream Synapse's `ApplicationService.ALLOWED_PROXY_PREFIXES`.
pub const ALLOWED_PROXY_PREFIXES: &[&str] = &["rtc/livekit"];

/// How long a snapshot of the registered proxy routes is reused.
///
/// The proxy lookup sits on the fallback path, so a request that matches no
/// route would otherwise re-read the application service table on every 404.
const PROXY_ROUTE_CACHE_TTL: Duration = Duration::from_secs(10);

/// TTL-bounded snapshot of the registered proxy routes: `(fetched_at, routes)`.
///
/// Named so `clippy::type_complexity` stays satisfied for the field on
/// [`ApplicationServiceManager`] and [`ApplicationServiceManager::proxy_route_cache_guard`].
pub(crate) type ProxyRouteCacheSnapshot = Option<(Instant, Arc<Vec<AppServiceProxyTarget>>)>;
/// The mutex guarding [`ProxyRouteCacheSnapshot`].
pub(crate) type ProxyRouteCache = std::sync::Mutex<ProxyRouteCacheSnapshot>;
/// A locked view of [`ProxyRouteCache`].
pub(crate) type ProxyRouteCacheGuard<'a> = MutexGuard<'a, ProxyRouteCacheSnapshot>;

/// Stable registration-file keys.
const PROXY_PREFIX_KEY: &str = "proxy_prefix";
const PROXY_URL_KEY: &str = "proxy_url";
/// MSC4512 unstable registration-file keys.
const UNSTABLE_PROXY_PREFIX_KEY: &str = "io.element.msc4512.proxy_prefix";
const UNSTABLE_PROXY_URL_KEY: &str = "io.element.msc4512.proxy_url";

/// Which Matrix API surface a proxied request arrived on.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ProxyScheme {
    /// `/_matrix/client/...` — authenticated with an access token.
    Client,
    /// `/_matrix/federation/...` — authenticated with the caller's signature.
    Federation,
}

/// Whether `prefix` may be claimed as an application service `proxy_prefix`.
///
/// A prefix is allowed when it is exactly an entry of
/// [`ALLOWED_PROXY_PREFIXES`] or a path segment below one — i.e. `rtc/livekit`
/// and `rtc/livekit/foo` are allowed, while `rtc/livekitX` is not.
pub fn is_proxy_prefix_allowed(prefix: &str) -> bool {
    ALLOWED_PROXY_PREFIXES
        .iter()
        .any(|allowed| prefix == *allowed || prefix.strip_prefix(allowed).is_some_and(|rest| rest.starts_with('/')))
}

/// A validated `proxy_prefix` / `proxy_url` pair.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AppServiceProxyConfig {
    /// Normalised prefix, e.g. `rtc/livekit` (no leading or trailing slash).
    pub proxy_prefix: String,
    /// Normalised forwarding base URL (no trailing slash).
    pub proxy_url: String,
}

/// A registered application service that can serve proxied requests.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AppServiceProxyTarget {
    /// The application service id, used for logging.
    pub as_id: String,
    /// The `hs_token` used to authenticate the proxied request to the AS.
    pub hs_token: String,
    /// The normalised `proxy_prefix` this service claims.
    pub proxy_prefix: String,
    /// The normalised `proxy_url` the prefix is forwarded to.
    pub proxy_url: String,
}

/// Split `/_matrix/{client,federation}/<version>/<rest>` into the API surface
/// and `<rest>`, honouring the `unstable/<identifier>/` form.
///
/// Returns `None` for anything that is not a versioned `/_matrix/client` or
/// `/_matrix/federation` path — including one that carries no remainder.
pub fn split_scheme_and_remainder(path: &str) -> Option<(ProxyScheme, &str)> {
    let rest = path.strip_prefix("/_matrix/")?;
    let (scheme, rest) = match rest.split_once('/') {
        Some(("client", rest)) => (ProxyScheme::Client, rest),
        Some(("federation", rest)) => (ProxyScheme::Federation, rest),
        _ => return None,
    };

    let (version, after_version) = rest.split_once('/')?;
    if version == "unstable" {
        // `unstable/<identifier>/<rest>` — drop the unstable identifier too.
        match after_version.split_once('/') {
            Some((identifier, remainder)) if !identifier.is_empty() => Some((scheme, remainder)),
            _ => None,
        }
    } else if version.starts_with('v') && version.len() > 1 {
        Some((scheme, after_version))
    } else {
        None
    }
}

/// Whether `remainder` is `prefix` itself or a path below it.
pub fn prefix_matches(prefix: &str, remainder: &str) -> bool {
    remainder == prefix || remainder.strip_prefix(prefix).is_some_and(|rest| rest.starts_with('/'))
}

/// Decode the `proxy_prefix` / `proxy_url` pair out of an application service
/// `config` blob (the registration file's unparsed keys, stored as JSONB).
///
/// Returns `Ok(None)` when the registration declares neither. Both values are
/// normalised (trailing slashes stripped) and validated before being returned.
pub fn proxy_config_from_value(config: &Value) -> Result<Option<AppServiceProxyConfig>, ApiError> {
    let Some(object) = config.as_object() else {
        return Ok(None);
    };

    let prefix = object.get(PROXY_PREFIX_KEY).or_else(|| object.get(UNSTABLE_PROXY_PREFIX_KEY));
    let url = object.get(PROXY_URL_KEY).or_else(|| object.get(UNSTABLE_PROXY_URL_KEY));

    if prefix.is_none() && url.is_none() {
        return Ok(None);
    }

    match (parse_proxy_field(prefix, PROXY_PREFIX_KEY)?, parse_proxy_field(url, PROXY_URL_KEY)?) {
        (Some(prefix), Some(url)) => {
            let config = AppServiceProxyConfig {
                proxy_prefix: prefix.trim_end_matches('/').to_owned(),
                proxy_url: url.trim_end_matches('/').to_owned(),
            };
            validate_proxy_config(&config)?;
            Ok(Some(config))
        }
        _ => Err(ApiError::bad_request("proxy_prefix and proxy_url must always be set together")),
    }
}

/// Validate a `config` blob at registration time, prefixing any failure with
/// the registration file's label.
pub fn validate_proxy_config_value(config: &Value, config_label: &str) -> Result<(), ApiError> {
    proxy_config_from_value(config).map(|_| ()).map_err(|error| {
        ApiError::bad_request(format!("Application service config '{}': {}", config_label, error.message()))
    })
}

fn parse_proxy_field(value: Option<&Value>, name: &str) -> Result<Option<String>, ApiError> {
    match value {
        None | Some(Value::Null) => Ok(None),
        Some(Value::String(text)) => Ok(Some(text.clone())),
        Some(_) => Err(ApiError::bad_request(format!("`{}` must be a string", name))),
    }
}

fn validate_proxy_config(config: &AppServiceProxyConfig) -> Result<(), ApiError> {
    if config.proxy_prefix.is_empty() {
        return Err(ApiError::bad_request("`proxy_prefix` must not be empty"));
    }
    if config.proxy_url.is_empty() {
        return Err(ApiError::bad_request("`proxy_url` must not be empty"));
    }
    if !is_proxy_prefix_allowed(&config.proxy_prefix) {
        return Err(ApiError::bad_request(format!(
            "`proxy_prefix` '{}' is reserved and cannot be claimed by an application service",
            config.proxy_prefix
        )));
    }
    url::Url::parse(&config.proxy_url).map_err(|error| {
        ApiError::bad_request(format!("`proxy_url` '{}' is not a valid URL: {}", config.proxy_url, error))
    })?;
    Ok(())
}

impl ApplicationServiceManager {
    /// Resolve the part of a `/_matrix/{client,federation}/<version>/` path that
    /// follows the version (or unstable identifier) to the application service
    /// that claims it.
    ///
    /// The longest matching prefix wins, so a service claiming `rtc/livekit`
    /// takes precedence over one claiming `rtc`. Returns `None` when no
    /// registered service claims the path.
    pub async fn resolve_proxy_target(&self, remainder: &str) -> Result<Option<AppServiceProxyTarget>, ApiError> {
        let routes = self.proxy_routes().await?;
        Ok(routes
            .iter()
            .filter(|route| prefix_matches(&route.proxy_prefix, remainder))
            .max_by_key(|route| route.proxy_prefix.len())
            .cloned())
    }

    fn proxy_route_cache_guard(&self) -> ProxyRouteCacheGuard<'_> {
        self.proxy_route_cache.lock().unwrap_or_else(|poisoned| poisoned.into_inner())
    }

    async fn proxy_routes(&self) -> Result<Arc<Vec<AppServiceProxyTarget>>, ApiError> {
        {
            let cache = self.proxy_route_cache_guard();
            if let Some((fetched_at, routes)) = cache.as_ref() {
                if fetched_at.elapsed() < PROXY_ROUTE_CACHE_TTL {
                    return Ok(Arc::clone(routes));
                }
            }
        }

        let mut routes = Vec::new();
        for service in self.get_all_active().await? {
            match proxy_config_from_value(&service.config) {
                Ok(Some(config)) => routes.push(AppServiceProxyTarget {
                    as_id: service.as_id,
                    hs_token: service.hs_token,
                    proxy_prefix: config.proxy_prefix,
                    proxy_url: config.proxy_url,
                }),
                Ok(None) => {}
                Err(error) => warn!(
                    as_id = %service.as_id,
                    %error,
                    "Ignoring invalid application service proxy configuration"
                ),
            }
        }

        let routes = Arc::new(routes);
        *self.proxy_route_cache_guard() = Some((Instant::now(), Arc::clone(&routes)));
        Ok(routes)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    // ── is_proxy_prefix_allowed ───────────────────────────────────────

    #[test]
    fn exact_allowed_prefix_is_allowed() {
        assert!(is_proxy_prefix_allowed("rtc/livekit"));
    }

    #[test]
    fn sub_path_of_allowed_prefix_is_allowed() {
        assert!(is_proxy_prefix_allowed("rtc/livekit/foo"));
    }

    #[test]
    fn prefix_sharing_a_name_boundary_is_not_allowed() {
        assert!(!is_proxy_prefix_allowed("rtc/livekitX"));
    }

    #[test]
    fn unrelated_prefix_is_not_allowed() {
        assert!(!is_proxy_prefix_allowed("rtc"));
        assert!(!is_proxy_prefix_allowed("_matrix/client/v3/rooms"));
    }

    // ── split_scheme_and_remainder ────────────────────────────────────

    #[test]
    fn splits_versioned_client_path() {
        assert_eq!(
            split_scheme_and_remainder("/_matrix/client/v3/rtc/livekit/foo"),
            Some((ProxyScheme::Client, "rtc/livekit/foo"))
        );
    }

    #[test]
    fn splits_versioned_federation_path() {
        assert_eq!(
            split_scheme_and_remainder("/_matrix/federation/v1/rtc/livekit"),
            Some((ProxyScheme::Federation, "rtc/livekit"))
        );
    }

    #[test]
    fn splits_unstable_path_dropping_identifier() {
        assert_eq!(
            split_scheme_and_remainder("/_matrix/client/unstable/io.element.msc4512/rtc/livekit/foo"),
            Some((ProxyScheme::Client, "rtc/livekit/foo"))
        );
    }

    #[test]
    fn rejects_non_matrix_paths() {
        assert_eq!(split_scheme_and_remainder("/_matrix/app/v1/transactions"), None);
        assert_eq!(split_scheme_and_remainder("/foo/bar"), None);
    }

    #[test]
    fn rejects_dominant_unversioned_segment() {
        assert_eq!(split_scheme_and_remainder("/_matrix/client/rtc/livekit"), None);
    }

    #[test]
    fn rejects_version_without_remainder() {
        assert_eq!(split_scheme_and_remainder("/_matrix/client/v3"), None);
        assert_eq!(split_scheme_and_remainder("/_matrix/client/unstable/io.element.msc4512"), None);
    }

    #[test]
    fn rejects_empty_unstable_identifier() {
        assert_eq!(split_scheme_and_remainder("/_matrix/client/unstable//rtc/livekit"), None);
    }

    // ── prefix_matches ────────────────────────────────────────────────

    #[test]
    fn prefix_matches_exact_and_nested() {
        assert!(prefix_matches("rtc/livekit", "rtc/livekit"));
        assert!(prefix_matches("rtc/livekit", "rtc/livekit/foo/bar"));
    }

    #[test]
    fn prefix_does_not_match_partial_segment() {
        assert!(!prefix_matches("rtc/livekit", "rtc/livekitX"));
        assert!(!prefix_matches("rtc/livekit", "rtc"));
    }

    // ── proxy_config_from_value ───────────────────────────────────────

    #[test]
    fn stable_keys_are_parsed_and_normalised() {
        let value = json!({"proxy_prefix": "rtc/livekit/", "proxy_url": "http://as:8008/"});
        let config = proxy_config_from_value(&value).unwrap().expect("config");
        assert_eq!(config.proxy_prefix, "rtc/livekit");
        assert_eq!(config.proxy_url, "http://as:8008");
    }

    #[test]
    fn unstable_keys_are_parsed() {
        let value = json!({
            "io.element.msc4512.proxy_prefix": "rtc/livekit",
            "io.element.msc4512.proxy_url": "http://as:8008",
        });
        let config = proxy_config_from_value(&value).unwrap().expect("config");
        assert_eq!(config.proxy_prefix, "rtc/livekit");
    }

    #[test]
    fn absent_proxy_keys_yield_none() {
        assert_eq!(proxy_config_from_value(&json!({"other": 1})).unwrap(), None);
        assert_eq!(proxy_config_from_value(&json!(null)).unwrap(), None);
    }

    #[test]
    fn prefix_without_url_is_rejected() {
        let error = proxy_config_from_value(&json!({"proxy_prefix": "rtc/livekit"})).unwrap_err();
        assert!(error.message().contains("must always be set together"), "{}", error.message());
    }

    #[test]
    fn non_string_prefix_is_rejected() {
        let error = proxy_config_from_value(&json!({"proxy_prefix": 7, "proxy_url": "http://as:8008"})).unwrap_err();
        assert!(error.message().contains("must be a string"), "{}", error.message());
    }

    #[test]
    fn reserved_prefix_is_rejected() {
        let error =
            proxy_config_from_value(&json!({"proxy_prefix": "rooms", "proxy_url": "http://as:8008"})).unwrap_err();
        assert!(error.message().contains("reserved"), "{}", error.message());
    }

    #[test]
    fn invalid_proxy_url_is_rejected() {
        let error =
            proxy_config_from_value(&json!({"proxy_prefix": "rtc/livekit", "proxy_url": "not a url"})).unwrap_err();
        assert!(error.message().contains("not a valid URL"), "{}", error.message());
    }

    #[test]
    fn registration_label_is_prefixed_in_error() {
        let error = validate_proxy_config_value(&json!({"proxy_prefix": "rooms"}), "/etc/as.yaml").unwrap_err();
        assert!(error.message().contains("/etc/as.yaml"), "{}", error.message());
    }
}
