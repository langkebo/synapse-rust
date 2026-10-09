// ROUND2-ISSUE-1: test code may use unwrap/expect/unwrap_err per Rust testing idiom.
// Production lib code is still held to the strict clippy lint config in [lints.clippy].
//! synapse-cache: caching, rate limiting, and circuit-breaking primitives.
//!
//! Submodules:
//! - [`circuit_breaker`]: token-bucket circuit breaker for backend protection.
//! - [`federation_signature_cache`]: caches federation signature verification results.
//! - [`invalidation`]: Redis Pub/Sub fan-out for cross-instance cache invalidation.
//! - [`rate_limit_metrics`]: rate-limit metrics collection.
//! - [`strategy`]: centralised cache-key prefixes and TTLs.
//! - [`error`]: cache error types and configuration.
//! - [`local`]: in-process L1 (`LocalCache`) implementations.
//! - [`remote`]: Redis-backed L2 (`RedisCache`) implementations.
//! - [`manager`]: coordinator (`CacheManager`) and rate limiting.
//!
//! Top-level items: [`CacheManager`] wraps Redis with a circuit breaker and
//! per-key degradation, [`LocalCache`] is the in-process moka cache, and
//! [`CacheError`] is the common error type.

#![cfg_attr(test, allow(clippy::unwrap_used, clippy::expect_used))]
// B-3.1-b ratchet in progress — see scripts/quality/check_missing_docs_ratchet.sh.
// We keep the deny level now that B-3.1-b-1 has cleared every public item in
// this crate (cargo doc -p synapse-cache reports 0 missing). The ratchet script
// (scripts/quality/check_missing_docs_ratchet.sh) will fail any PR that
// introduces a new undocumented public item anywhere in the workspace, so
// neighbouring crates will hit the ratchet before they could regress this one.
#![deny(missing_docs)]

/// Circuit-breaker-protected Redis cache and in-process fallback.
pub mod circuit_breaker;
/// Federation signature verification cache.
pub mod federation_signature_cache;
/// Cross-instance cache invalidation over Redis Pub/Sub.
pub mod invalidation;
/// Rate-limit metrics collection.
pub mod rate_limit_metrics;
/// Centralised cache-key prefixes and TTLs.
pub mod strategy;

/// Cache error types and configuration.
pub mod error;
/// The `health` module.
pub mod health;
/// In-process local cache implementations.
pub mod local;
/// Cache manager and rate limiting.
pub mod manager;
/// Redis-backed cache implementations.
pub mod remote;

pub use circuit_breaker::{CircuitBreaker, CircuitBreakerMetrics, CircuitState};
pub use error::{CacheConfig, CacheError, DegradationMetrics};
pub use federation_signature_cache::{
    CacheEntryKey, FederationSignatureCache, KeyRotationCallback, KeyRotationEvent, SignatureCacheConfig,
    SignatureCacheEntry, SignatureCacheStats, DEFAULT_KEY_CACHE_TTL, DEFAULT_KEY_ROTATION_GRACE_PERIOD_MS,
    DEFAULT_SIGNATURE_CACHE_TTL,
};
pub use health::CacheHealthCheck;
pub use invalidation::{
    CacheInvalidationBroadcaster, CacheInvalidationConfig, CacheInvalidationManager, CacheInvalidationMessage,
    CacheInvalidationSubscriber, InvalidationReceiver, InvalidationType, CACHE_INVALIDATION_CHANNEL,
    DEFAULT_LOCAL_CACHE_TTL_SECS, DEFAULT_REDIS_CACHE_TTL_SECS,
};
pub use local::LocalCache;
pub use manager::{CacheManager, RateLimitDecision};
pub use rate_limit_metrics::RateLimitMetrics;
pub use remote::RedisCache;
pub use strategy::{CacheKeyBuilder, CacheTtl};

#[cfg(test)]
mod tests;
