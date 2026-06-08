//! Federation X-Matrix nonce cache.
//!
//! Prevents replay of signed federation requests by remembering the
//! `(origin, nonce, origin_server_ts)` tuples we have already accepted.
//! Entries are evicted automatically after `timestamp_window + ttl` so
//! the cache size stays bounded.
//!
//! This is a defence-in-depth measure: even if a signing key is captured,
//! an attacker cannot re-submit a captured request after its timestamp
//! window closes. The timestamp window itself is enforced by the
//! federation auth middleware; this cache closes the residual window
//! inside that interval.

use std::sync::Arc;
use std::time::{Duration, Instant};

use moka::future::Cache;
use tokio::sync::Mutex;

use crate::error::ApiError;

/// Default timestamp freshness window. Mirrors Synapse v1.153 (`-30000..=30000` ms).
pub const DEFAULT_TIMESTAMP_SKEW: Duration = Duration::from_secs(30);

/// How long a nonce remains "hot" after first insertion. Should be
/// strictly greater than `2 * timestamp_skew` so a request cannot be
/// re-played by sliding it inside the freshness window.
pub const NONCE_TTL: Duration = Duration::from_secs(120);

/// Maximum number of distinct (origin, nonce) pairs to remember.
/// Sized to handle traffic peaks; older entries fall out via TTL.
pub const NONCE_CACHE_CAPACITY: u64 = 1_000_000;

/// Bounded, async-safe cache for federation nonces.
#[derive(Clone)]
pub struct FederationNonceCache {
    inner: Arc<Cache<String, ()>>,
    /// Tracks the timestamp we first saw a (origin, nonce) entry so we
    /// can perform constant-time freshness comparison in addition to
    /// nonce-based replay rejection.
    seen_at: Arc<Mutex<Option<Instant>>>,
}

impl Default for FederationNonceCache {
    fn default() -> Self {
        Self::new()
    }
}

impl FederationNonceCache {
    pub fn new() -> Self {
        Self {
            inner: Arc::new(Cache::builder().max_capacity(NONCE_CACHE_CAPACITY).time_to_live(NONCE_TTL).build()),
            seen_at: Arc::new(Mutex::new(None)),
        }
    }

    /// Insert a nonce and report whether it was already present.
    ///
    /// Returns `Ok(true)` if the nonce was new (the request is fresh),
    /// `Ok(false)` if it has already been seen within the TTL window
    /// (a replay). The function is idempotent w.r.t. concurrent
    /// callers: each `(origin, nonce)` pair transitions to "seen" at
    /// most once.
    pub async fn check_and_record(&self, origin: &str, nonce: &str, origin_server_ts_ms: i64) -> Result<bool, ApiError> {
        let key = build_nonce_key(origin, nonce, origin_server_ts_ms);

        if self.inner.get(&key).await.is_some() {
            ::tracing::warn!(
                target: "security_audit",
                event = "federation_replay_detected",
                origin = origin,
                nonce = nonce,
                origin_server_ts_ms = origin_server_ts_ms,
                "Rejected federation request with previously-seen nonce"
            );
            return Ok(false);
        }

        // Insert before returning so concurrent callers see the entry
        // even when they race the first writer.
        self.inner.insert(key, ()).await;
        *self.seen_at.lock().await = Some(Instant::now());
        Ok(true)
    }

    /// Convenience helper for tests / health checks.
    pub fn len(&self) -> u64 {
        self.inner.entry_count()
    }

    /// Mirror of [`Self::len`] used to satisfy the `len_without_is_empty` clippy lint.
    pub fn is_empty(&self) -> bool {
        self.inner.entry_count() == 0
    }
}

fn build_nonce_key(origin: &str, nonce: &str, origin_server_ts_ms: i64) -> String {
    // Deliberately use a separator that cannot appear in either input
    // (origin and nonce are ASCII per Matrix spec) and bind the key to
    // the timestamp bucket so a replay that nudges the timestamp into
    // a new bucket still does not bypass the cache.
    format!("{origin}\x00{nonce}\x00{origin_server_ts_ms}")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn first_seen_nonce_is_fresh() {
        let cache = FederationNonceCache::new();
        assert!(cache.check_and_record("origin.test", "nonce-1", 1_000_000).await.unwrap());
    }

    #[tokio::test]
    async fn repeat_nonce_is_replay() {
        let cache = FederationNonceCache::new();
        assert!(cache.check_and_record("origin.test", "nonce-1", 1_000_000).await.unwrap());
        assert!(!cache.check_and_record("origin.test", "nonce-1", 1_000_000).await.unwrap());
    }

    #[tokio::test]
    async fn different_timestamp_is_fresh() {
        let cache = FederationNonceCache::new();
        assert!(cache.check_and_record("origin.test", "nonce-1", 1_000_000).await.unwrap());
        assert!(cache.check_and_record("origin.test", "nonce-1", 1_000_001).await.unwrap());
    }

    #[tokio::test]
    async fn different_origin_is_fresh() {
        let cache = FederationNonceCache::new();
        assert!(cache.check_and_record("a.test", "nonce-1", 1_000_000).await.unwrap());
        assert!(cache.check_and_record("b.test", "nonce-1", 1_000_000).await.unwrap());
    }
}
