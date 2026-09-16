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

/// LZ77-based compression utilities for large cache values.
///
/// Uses gzip (compression level 6) for payloads ≥ 1 KiB; smaller values are stored
/// verbatim with a `0` prefix byte so the decompressor can distinguish the two paths.
pub mod compression {
    use std::io::{Read, Write};

    const COMPRESSION_THRESHOLD: usize = 1024;

    /// Compresses `data` if it exceeds 1 KiB; small payloads are returned verbatim with a `0` prefix.
    pub fn compress(data: &[u8]) -> Result<Vec<u8>, &'static str> {
        if data.len() < COMPRESSION_THRESHOLD {
            let mut result = Vec::with_capacity(data.len() + 1);
            result.push(0);
            result.extend_from_slice(data);
            Ok(result)
        } else {
            let mut encoder = flate2::write::GzEncoder::new(Vec::new(), flate2::Compression::new(6));
            encoder.write_all(data).map_err(|_| "Failed to compress")?;
            let compressed = encoder.finish().map_err(|_| "Failed to finish compression")?;

            let mut result = Vec::with_capacity(compressed.len() + 1);
            result.push(1);
            result.extend_from_slice(&compressed);
            Ok(result)
        }
    }

    /// Inverse of [`compress`] — inspects the prefix byte to dispatch to the right path.
    pub fn decompress(data: &[u8]) -> Result<Vec<u8>, &'static str> {
        if data.is_empty() {
            return Err("Empty data");
        }

        let is_compressed = data[0] == 1;
        let payload = &data[1..];

        if !is_compressed {
            Ok(payload.to_vec())
        } else {
            let mut decoder = flate2::read::GzDecoder::new(payload);
            let mut decompressed = Vec::new();
            decoder.read_to_end(&mut decompressed).map_err(|_| "Failed to decompress")?;
            Ok(decompressed)
        }
    }

    /// UTF-8-aware wrapper around [`compress`] — convenience for string payloads.
    pub fn compress_string(s: &str) -> Result<Vec<u8>, &'static str> {
        compress(s.as_bytes())
    }

    /// Inverse of [`compress_string`] — also validates UTF-8 on the decompressed bytes.
    pub fn decompress_to_string(data: &[u8]) -> Result<String, &'static str> {
        decompress(data).and_then(|bytes| String::from_utf8(bytes).map_err(|_| "Invalid UTF-8"))
    }

    /// Returns `true` when [`compress`] would actually compress (i.e. payload ≥ 1 KiB).
    pub fn should_compress(data: &[u8]) -> bool {
        data.len() >= COMPRESSION_THRESHOLD
    }
}

#[cfg(test)]
mod compression_tests {
    use super::compression::*;

    #[test]
    #[allow(missing_docs)]
    fn test_compress_decompress_roundtrip() {
        let original = b"Hello, World! This is a test string for compression.";

        let compressed = compress(original).unwrap();
        assert!(!compressed.is_empty());

        let decompressed = decompress(&compressed).unwrap();
        assert_eq!(decompressed, original);
    }

    #[test]
    #[allow(missing_docs)]
    fn test_small_data_not_compressed() {
        let original = b"small";

        let compressed = compress(original).unwrap();
        assert_eq!(compressed[0], 0);
        assert_eq!(&compressed[1..], original);
    }

    #[test]
    #[allow(missing_docs)]
    fn test_compress_string_roundtrip() {
        let original = "Test string with unicode: 你好世界 🌍";

        let compressed = compress_string(original).unwrap();
        let decompressed = decompress_to_string(&compressed).unwrap();
        assert_eq!(decompressed, original);
    }

    #[test]
    #[allow(missing_docs)]
    fn test_decompress_empty() {
        let result = decompress(&[]);
        assert!(result.is_err());
    }

    #[test]
    #[allow(missing_docs)]
    fn test_compress_decompress_large_data() {
        let original: Vec<u8> = (0..10000).map(|i| (i % 256) as u8).collect();

        let compressed = compress(&original).unwrap();
        let decompressed = decompress(&compressed).unwrap();
        assert_eq!(decompressed, original);
    }
}
