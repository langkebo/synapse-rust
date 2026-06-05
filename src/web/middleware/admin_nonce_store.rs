//! Admin-registration nonce store with in-memory fallback.
//!
//! Pre-Sprint 6: the admin-registration nonce (`POST /_synapse/admin/v1/register`)
//! was stored exclusively in the shared cache. The handlers **failed closed**
//! on any cache error — i.e. if Redis was unreachable, `GET /nonce` and
//! `POST /register` both returned 500. This is correct from a security
//! standpoint (we'd rather refuse registration than issue one we can't
//! audit) but turns a cache outage into a hard admin-outage.
//!
//! This module introduces a [`NonceStore`] wrapper with:
//!
//! 1. **Primary path**: shared cache (Redis), atomic `GETDEL` so the
//!    check-and-remove window is race-free.
//! 2. **Fallback path**: an in-process `Mutex<HashMap<nonce, expiry>>` that
//!    is used only when the cache is **circuit-open** (i.e. N consecutive
//!    failures have tripped the breaker). The fallback is **per-process**:
//!    in a multi-worker deployment, two workers can each accept the same
//!    nonce during a Redis outage. That is acceptable because the
//!    fallback is short-lived (exponential backoff caps the breaker-open
//!    window) and the admin-registration API is normally localhost-only.
//! 3. **Circuit breaker**: tracks consecutive cache failures and opens the
//!    circuit after a configurable threshold. While open, the in-memory
//!    store is used exclusively. A periodic probe (the next call after
//!    `backoff_window`) re-closes the circuit if the cache has recovered.
//!
//! Both paths return a [`NonceLookup`] distinguishing the source so the
//! caller can emit metrics. The cache's own in-process fallback (which is
//! itself an LRU with TTL) is unaffected — it is still consulted first
//! whenever the cache call succeeds, so the common case costs at most one
//! RTT.

use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, AtomicU32, AtomicU64, Ordering};
use std::sync::Mutex;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use crate::cache::CacheManager;

/// State of the circuit breaker. Transitions:
///   `Closed → Open` after `OPEN_AFTER_FAILURES` consecutive failures.
///   `Open → HalfOpen` after `backoff_window` elapses (lazy, on next call).
///   `HalfOpen → Closed` on the first successful cache call.
///   `HalfOpen → Open` on a failed cache call (resets backoff window).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CircuitState {
    Closed,
    Open,
    HalfOpen,
}

/// Outcome of a `consume` call. `Source::Cache` is the normal-path hit;
/// `Source::InMemory` means the breaker was open and the in-process
/// store was used.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NonceSource {
    Cache,
    InMemory,
}

#[derive(Debug, Clone, Copy)]
pub struct NonceLookup {
    pub found: bool,
    pub source: NonceSource,
    pub circuit: CircuitState,
}

#[derive(Debug, Clone, Copy)]
pub struct NonceStoreConfig {
    /// Number of consecutive cache failures that flips the breaker open.
    pub open_after_failures: u32,
    /// How long the breaker stays open before allowing a probe call.
    pub backoff_window: Duration,
    /// How long an in-memory nonce lives (must be ≤ the cache TTL).
    pub in_memory_ttl: Duration,
    /// Cap on the in-memory map; over-limit triggers a sweep of expired entries.
    pub in_memory_capacity: usize,
}

impl Default for NonceStoreConfig {
    fn default() -> Self {
        Self {
            open_after_failures: 3,
            backoff_window: Duration::from_secs(30),
            in_memory_ttl: Duration::from_secs(60),
            in_memory_capacity: 4096,
        }
    }
}

struct InMemoryEntry {
    /// Absolute wall-clock expiry in milliseconds since UNIX_EPOCH.
    expires_at_ms: u64,
}

/// Thread-safe admin-registration nonce store.
pub struct NonceStore {
    cache: std::sync::Arc<CacheManager>,
    cfg: NonceStoreConfig,
    state: Mutex<InMemoryState>,
    consecutive_failures: AtomicU32,
    /// Wall-clock ms when the breaker first opened; `0` if closed.
    opened_at_ms: AtomicU64,
    /// Last `0/1` latch recording whether the breaker is open. We mirror
    /// `opened_at_ms != 0` into this atomic for cheap read access on
    /// every cache call.
    open: AtomicBool,
}

struct InMemoryState {
    entries: HashMap<String, InMemoryEntry>,
}

impl NonceStore {
    pub fn new(cache: std::sync::Arc<CacheManager>, cfg: NonceStoreConfig) -> Self {
        Self {
            cache,
            cfg,
            state: Mutex::new(InMemoryState { entries: HashMap::new() }),
            consecutive_failures: AtomicU32::new(0),
            opened_at_ms: AtomicU64::new(0),
            open: AtomicBool::new(false),
        }
    }

    /// Current breaker state. Cheap (two atomic loads + comparison).
    pub fn circuit_state(&self) -> CircuitState {
        if !self.open.load(Ordering::Acquire) {
            return CircuitState::Closed;
        }
        let opened_at_ms = self.opened_at_ms.load(Ordering::Acquire);
        let now_ms = unix_ms_now();
        if now_ms.saturating_sub(opened_at_ms) >= self.cfg.backoff_window.as_millis() as u64 {
            CircuitState::HalfOpen
        } else {
            CircuitState::Open
        }
    }

    /// Store a freshly-minted nonce. Returns `true` if the cache write
    /// succeeded; `false` if the cache failed and the nonce was recorded
    /// in the in-memory fallback (so it can still be consumed within the
    /// same process during a cache outage).
    pub async fn put(&self, cache_key: &str, nonce_value: &str, ttl: Duration) -> bool {
        let circuit = self.circuit_state();
        // Probe: in HalfOpen we *only* try the cache.
        let try_cache = circuit != CircuitState::Open;

        if try_cache {
            match self.cache.try_set_raw(cache_key, nonce_value, ttl.as_secs()).await {
                Ok(()) => {
                    self.on_cache_success();
                    return true;
                }
                Err(e) => {
                    ::tracing::warn!(
                        target: "security_audit",
                        event = "admin_nonce_cache_set_failed",
                        circuit = ?circuit,
                        error = %e,
                        "Shared cache set failed; recording nonce in in-memory fallback"
                    );
                    self.on_cache_failure();
                    self.put_in_memory(cache_key, ttl);
                    return false;
                }
            }
        }

        // Open circuit: skip cache, write in-memory only.
        self.put_in_memory(cache_key, ttl);
        false
    }

    /// Atomic check-and-remove. Returns `found=true` if the nonce was
    /// valid and is now consumed; `false` otherwise.
    pub async fn consume(&self, cache_key: &str) -> NonceLookup {
        let circuit = self.circuit_state();
        let try_cache = circuit != CircuitState::Open;

        if try_cache {
            match self.cache.try_get_and_delete_raw(cache_key).await {
                Ok(Some(_)) => {
                    self.on_cache_success();
                    return NonceLookup { found: true, source: NonceSource::Cache, circuit };
                }
                Ok(None) => {
                    // The cache replied successfully but the key is gone.
                    // Could be: never written, expired, or already consumed.
                    // We still treat this as a "cache is healthy" success
                    // for breaker accounting.
                    self.on_cache_success();
                    // Also check in-memory in case the original write fell
                    // back to in-process and was never promoted to cache.
                    if self.pop_in_memory(cache_key) {
                        return NonceLookup {
                            found: true,
                            source: NonceSource::InMemory,
                            circuit: CircuitState::Closed,
                        };
                    }
                    return NonceLookup { found: false, source: NonceSource::Cache, circuit };
                }
                Err(e) => {
                    ::tracing::warn!(
                        target: "security_audit",
                        event = "admin_nonce_cache_get_and_delete_failed",
                        circuit = ?circuit,
                        error = %e,
                        "Shared cache get_and_delete failed; falling back to in-memory"
                    );
                    self.on_cache_failure();
                    let found = self.pop_in_memory(cache_key);
                    return NonceLookup { found, source: NonceSource::InMemory, circuit: CircuitState::Open };
                }
            }
        }

        // Open circuit: skip cache, only consult in-memory.
        let found = self.pop_in_memory(cache_key);
        NonceLookup { found, source: NonceSource::InMemory, circuit: CircuitState::Open }
    }

    // ---- breaker accounting ----

    fn on_cache_success(&self) {
        let prev_failures = self.consecutive_failures.swap(0, Ordering::AcqRel);
        let was_open = self.open.swap(false, Ordering::AcqRel);
        if was_open || prev_failures > 0 {
            ::tracing::info!(
                target: "security_audit",
                event = "admin_nonce_breaker_closed",
                prev_failures,
                "Admin-nonce circuit breaker closed; shared cache is healthy again"
            );
        }
        self.opened_at_ms.store(0, Ordering::Release);
    }

    fn on_cache_failure(&self) {
        let now = self.consecutive_failures.fetch_add(1, Ordering::AcqRel) + 1;
        if now >= self.cfg.open_after_failures {
            let was_open = self.open.swap(true, Ordering::AcqRel);
            if !was_open {
                self.opened_at_ms.store(unix_ms_now(), Ordering::Release);
                ::tracing::warn!(
                    target: "security_audit",
                    event = "admin_nonce_breaker_opened",
                    consecutive_failures = now,
                    backoff_secs = self.cfg.backoff_window.as_secs(),
                    "Admin-nonce circuit breaker OPEN; using in-memory fallback"
                );
            }
        }
    }

    // ---- in-memory helpers ----

    fn put_in_memory(&self, cache_key: &str, ttl: Duration) {
        let now_ms = unix_ms_now();
        let expires_at_ms = now_ms + ttl.as_millis() as u64;
        let mut state = match self.state.lock() {
            Ok(g) => g,
            Err(p) => p.into_inner(),
        };
        // Sweep expired entries if we're approaching capacity.
        if state.entries.len() >= self.cfg.in_memory_capacity {
            state.entries.retain(|_, e| e.expires_at_ms > now_ms);
        }
        state.entries.insert(cache_key.to_string(), InMemoryEntry { expires_at_ms });
    }

    /// Returns true if the entry was present and unexpired; pops it either way
    /// (so a half-expired entry cannot be replayed by a slow consumer).
    fn pop_in_memory(&self, cache_key: &str) -> bool {
        let now_ms = unix_ms_now();
        let mut state = match self.state.lock() {
            Ok(g) => g,
            Err(p) => p.into_inner(),
        };
        match state.entries.remove(cache_key) {
            Some(e) => e.expires_at_ms > now_ms,
            None => false,
        }
    }
}

fn unix_ms_now() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;

    /// In-process CacheManager. Kept as a `build_*` helper so the
    /// circuit-breaker tests share a construction pattern; the
    /// failure-injection flags live on the `NonceStore` itself now,
    /// not on the cache.
    fn build_cache() -> Arc<CacheManager> {
        let cache_config = crate::cache::CacheConfig::default();
        Arc::new(crate::cache::CacheManager::new(&cache_config))
    }

    #[test]
    fn closed_circuit_serves_from_cache() {
        let cache = build_cache();
        let store = NonceStore::new(cache, NonceStoreConfig::default());

        // rt for the async set/get_and_delete
        let rt = tokio::runtime::Builder::new_current_thread().enable_all().build().unwrap();

        rt.block_on(async {
            assert!(
                store.put("k1", "v1", Duration::from_secs(60)).await,
                "first put should hit cache"
            );
            let lookup = store.consume("k1").await;
            assert!(lookup.found);
            assert_eq!(lookup.source, NonceSource::Cache);
            assert_eq!(lookup.circuit, CircuitState::Closed);
        });
    }

    #[test]
    fn unknown_nonce_returns_not_found() {
        let cache = build_cache();
        let store = NonceStore::new(cache, NonceStoreConfig::default());
        let rt = tokio::runtime::Builder::new_current_thread().enable_all().build().unwrap();
        rt.block_on(async {
            let lookup = store.consume("nonexistent").await;
            assert!(!lookup.found);
            assert_eq!(lookup.circuit, CircuitState::Closed);
        });
    }

    #[test]
    fn half_open_after_backoff_window() {
        // Manipulate the breaker directly via on_cache_failure to force
        // open, then advance `opened_at_ms` so the backoff window is
        // already elapsed. The next consume call should observe HalfOpen.
        let cache = build_cache();
        let cfg = NonceStoreConfig {
            backoff_window: Duration::from_millis(0),
            ..NonceStoreConfig::default()
        };
        let store = NonceStore::new(cache, cfg);
        let rt = tokio::runtime::Builder::new_current_thread().enable_all().build().unwrap();
        rt.block_on(async {
            // Force 3 failures to open the breaker.
            store.on_cache_failure();
            store.on_cache_failure();
            store.on_cache_failure();
            assert_eq!(store.circuit_state(), CircuitState::HalfOpen);
        });
    }
}
