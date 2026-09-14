use std::time::Duration;

/// Centralised factory for constructing canonical Redis key strings.
///
/// Using a typed builder instead of raw strings prevents key collisions
/// and makes cache invalidation auditable (grep the module for the key).
///
/// Only keys that actually have a consumer live here. Add a constructor at the
/// point a second caller needs the same namespace — do not pre-declare keys for
/// caches that do not exist yet, because an unread key table drifts away from the
/// string literals the real call sites use.
pub struct CacheKeyBuilder;

impl CacheKeyBuilder {
    /// Builds the cache key for a user's presence state.
    pub fn user_presence(user_id: &str) -> String {
        format!("user:{user_id}:presence")
    }

    /// Builds the cache key for per-IP, per-endpoint rate-limit state.
    pub fn ip_rate_limit(ip: &str, endpoint: &str) -> String {
        format!("ratelimit:ip:{ip}:{endpoint}")
    }

    /// Cache key for per-origin federation rate limiting.
    pub fn federation_origin_rate_limit(origin: &str, endpoint: &str) -> String {
        format!("ratelimit:fed:{origin}:{endpoint}")
    }
}

/// Constants for cache TTL values across cache domains.
///
/// Implemented as a zero-sized struct with associated accessors so the TTL is
/// referenced through a typed API rather than a bare `Duration::from_secs(..)`.
pub struct CacheTtl;

impl CacheTtl {
    /// Returns the TTL for user presence cache entries.
    pub fn user_presence() -> Duration {
        Duration::from_secs(60) // 1 min - balance freshness and hit rate
    }
}

#[cfg(test)]
#[allow(missing_docs)]
mod tests {
    use super::*;

    #[test]
    #[allow(missing_docs)]
    fn test_cache_key_user_presence() {
        let key = CacheKeyBuilder::user_presence("@user:example.com");
        assert_eq!(key, "user:@user:example.com:presence");
    }

    #[test]
    #[allow(missing_docs)]
    fn test_cache_key_ip_rate_limit() {
        let key = CacheKeyBuilder::ip_rate_limit("192.168.1.1", "/login");
        assert_eq!(key, "ratelimit:ip:192.168.1.1:/login");
    }

    #[test]
    #[allow(missing_docs)]
    fn test_cache_key_federation_origin_rate_limit() {
        let key = CacheKeyBuilder::federation_origin_rate_limit("remote.example.com", "/send/1");
        assert_eq!(key, "ratelimit:fed:remote.example.com:/send/1");
    }

    #[test]
    #[allow(missing_docs)]
    fn test_cache_ttl_user_presence() {
        let ttl = CacheTtl::user_presence();
        assert_eq!(ttl, Duration::from_secs(60));
    }
}
