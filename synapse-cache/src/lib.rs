// ROUND2-ISSUE-1: test code may use unwrap/expect/unwrap_err per Rust testing idiom.
// Production lib code is still held to the strict clippy lint config in [lints.clippy].
//! synapse-cache: caching, rate limiting, and circuit-breaking primitives.
//!
//! Submodules:
//! - [`query_cache`]: in-process namespace cache (room / user / event / device / token).
//! - [`circuit_breaker`]: token-bucket circuit breaker for backend protection.
//! - [`federation_signature_cache`]: caches federation signature verification results.
//! - [`invalidation`]: Redis Pub/Sub fan-out for cross-instance cache invalidation.
//! - [`rate_limit_metrics`]: rate-limit metrics collection.
//! - [`strategy`]: centralised cache-key prefixes and TTLs.
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

//! Multi-layer cache subsystem: in-process L1 (`LocalCache`), Redis L2 (`RedisCache`),
//! coordinator (`CacheManager`), cross-instance invalidation pub/sub, circuit
//! breaker, federation signature cache, and token-bucket rate limiter.

use deadpool_redis::{Config, Pool, PoolConfig, Runtime};
use moka::ops::compute::Op;
use moka::sync::Cache;
use serde::{Deserialize, Serialize};
use std::cell::Cell;
use std::collections::HashMap;
use std::sync::{Arc, OnceLock};
use std::time::Duration;
use synapse_common::claims::Claims;
use synapse_common::ApiError;
use thiserror::Error;
use tokio::time::timeout;

/// Circuit-breaker-protected Redis cache and in-process fallback.
pub mod circuit_breaker;
/// Federation signature verification cache.
pub mod federation_signature_cache;
/// Cross-instance cache invalidation over Redis Pub/Sub.
pub mod invalidation;
/// In-process per-namespace query cache.
pub mod query_cache;
/// Rate-limit metrics collection.
pub mod rate_limit_metrics;
/// Centralised cache-key prefixes and TTLs.
pub mod strategy;

pub use circuit_breaker::{CircuitBreaker, CircuitBreakerMetrics, CircuitState};
pub use federation_signature_cache::{
    CacheEntryKey, FederationSignatureCache, KeyRotationCallback, KeyRotationEvent, SignatureCacheConfig,
    SignatureCacheEntry, SignatureCacheStats, DEFAULT_KEY_CACHE_TTL, DEFAULT_KEY_ROTATION_GRACE_PERIOD_MS,
    DEFAULT_SIGNATURE_CACHE_TTL,
};
pub use invalidation::{
    CacheInvalidationBroadcaster, CacheInvalidationConfig, CacheInvalidationManager, CacheInvalidationMessage,
    CacheInvalidationSubscriber, InvalidationReceiver, InvalidationType, CACHE_INVALIDATION_CHANNEL,
    DEFAULT_LOCAL_CACHE_TTL_SECS, DEFAULT_REDIS_CACHE_TTL_SECS,
};
pub use query_cache::{CacheEntry, CacheStats, QueryCache, QueryCacheConfig};
pub use rate_limit_metrics::RateLimitMetrics;
pub use strategy::{CacheKeyBuilder, CacheTtl};

const DEFAULT_REDIS_TIMEOUT_MS: u64 = 500;

/// Intermediate error types used by `with_circuit_breaker` to uniformly convert
/// circuit-breaker / connection / timeout failures into different error types
/// (`CacheError` or `redis::RedisError`).

#[derive(Debug)]
struct CircuitBreakerOpen {
    operation: String,
}

#[derive(Debug)]
struct ConnectionTimeout {
    operation: String,
}

#[derive(Debug)]
struct PoolExhaustion {
    source: String,
}

#[derive(Debug)]
struct CommandTimeout {
    operation: String,
}

#[derive(Debug)]
struct OperationFailed {
    detail: String,
}

impl From<CircuitBreakerOpen> for CacheError {
    fn from(e: CircuitBreakerOpen) -> Self {
        CacheError::CircuitBreakerOpen(format!("Circuit breaker is open, rejecting Redis {} request", e.operation))
    }
}

impl From<ConnectionTimeout> for CacheError {
    fn from(e: ConnectionTimeout) -> Self {
        CacheError::ConnectionTimeout(format!("Redis pool get timeout for {}", e.operation))
    }
}

impl From<PoolExhaustion> for CacheError {
    fn from(e: PoolExhaustion) -> Self {
        CacheError::PoolExhaustion(e.source)
    }
}

impl From<CommandTimeout> for CacheError {
    fn from(e: CommandTimeout) -> Self {
        CacheError::CommandTimeout(format!("Redis {} command timeout", e.operation))
    }
}

impl From<OperationFailed> for CacheError {
    fn from(e: OperationFailed) -> Self {
        CacheError::OperationFailed(e.detail)
    }
}

impl From<CircuitBreakerOpen> for redis::RedisError {
    fn from(_: CircuitBreakerOpen) -> Self {
        redis::RedisError::from((redis::ErrorKind::IoError, "Circuit breaker is open"))
    }
}

impl From<ConnectionTimeout> for redis::RedisError {
    fn from(_: ConnectionTimeout) -> Self {
        redis::RedisError::from((redis::ErrorKind::IoError, "Redis connection timeout"))
    }
}

impl From<PoolExhaustion> for redis::RedisError {
    fn from(e: PoolExhaustion) -> Self {
        redis::RedisError::from((redis::ErrorKind::IoError, "Redis pool exhaustion", e.source))
    }
}

impl From<CommandTimeout> for redis::RedisError {
    fn from(_: CommandTimeout) -> Self {
        redis::RedisError::from((redis::ErrorKind::IoError, "Redis command timeout"))
    }
}

impl From<OperationFailed> for redis::RedisError {
    fn from(e: OperationFailed) -> Self {
        redis::RedisError::from((redis::ErrorKind::IoError, "Redis operation failed", e.detail))
    }
}

/// Wrapper error type for `get` / `expire` which discard errors internally.
/// Supports the same `From` conversions as `CacheError` so it can be used with
/// `with_circuit_breaker`.
#[derive(Debug)]
enum CacheErrorWrapper {
    CircuitBreakerOpen,
    ConnectionTimeout,
    PoolExhaustion,
    CommandTimeout,
    OperationFailed,
}

impl From<CircuitBreakerOpen> for CacheErrorWrapper {
    fn from(_: CircuitBreakerOpen) -> Self {
        CacheErrorWrapper::CircuitBreakerOpen
    }
}

impl From<ConnectionTimeout> for CacheErrorWrapper {
    fn from(_: ConnectionTimeout) -> Self {
        CacheErrorWrapper::ConnectionTimeout
    }
}

impl From<PoolExhaustion> for CacheErrorWrapper {
    fn from(_: PoolExhaustion) -> Self {
        CacheErrorWrapper::PoolExhaustion
    }
}

impl From<CommandTimeout> for CacheErrorWrapper {
    fn from(_: CommandTimeout) -> Self {
        CacheErrorWrapper::CommandTimeout
    }
}

impl From<OperationFailed> for CacheErrorWrapper {
    fn from(_: OperationFailed) -> Self {
        CacheErrorWrapper::OperationFailed
    }
}

/// Errors produced by the cache layer.
///
/// Covers connection failures, circuit-breaker trips, pool exhaustion, and serialisation errors.
#[derive(Debug, Error)]
pub enum CacheError {
    /// Redis connection could not be established within the configured timeout.
    #[error("Redis connection timeout: {0}")]
    ConnectionTimeout(String),
    /// A Redis command exceeded the configured command timeout.
    #[error("Redis command timeout: {0}")]
    CommandTimeout(String),
    /// The Redis connection pool has no available connections and is at capacity.
    #[error("Redis pool exhaustion: {0}")]
    PoolExhaustion(String),
    /// A Redis command failed for a reason other than timeout or pool exhaustion.
    #[error("Redis operation failed: {0}")]
    OperationFailed(String),
    /// Serialising or deserialising the cache value (JSON) failed.
    #[error("Serialization error: {0}")]
    SerializationError(String),
    /// The circuit breaker is open and no Redis operations are permitted.
    #[error("Circuit breaker is open: {0}")]
    CircuitBreakerOpen(String),
}

/// Metrics tracking local-cache / Redis-cache hit ratios and circuit-breaker behaviour.
///
/// Suitable for logging or metrics export.
#[derive(Debug, Clone, Default)]
pub struct DegradationMetrics {
    /// Total local in-process cache hits.
    pub local_cache_hits: u64,
    /// Total local in-process cache misses.
    pub local_cache_misses: u64,
    /// Total Redis cache hits.
    pub redis_cache_hits: u64,
    /// Total Redis cache misses.
    pub redis_cache_misses: u64,
    /// Total requests rejected because the circuit breaker was open.
    pub circuit_breaker_rejections: u64,
    /// Total requests that fell back to the database.
    pub fallback_operations: u64,
    /// Total requests that were handled in degraded mode (Redis or circuit breaker tripped).
    pub total_degraded_requests: u64,
}

impl DegradationMetrics {
    /// Constructs a fresh zeroed metrics instance.
    pub fn new() -> Self {
        Self::default()
    }

    /// Increments the local-cache hit counter.
    pub fn record_local_hit(&mut self) {
        self.local_cache_hits += 1;
    }

    /// Increments the local-cache miss counter.
    pub fn record_local_miss(&mut self) {
        self.local_cache_misses += 1;
    }

    /// Increments the Redis-cache hit counter.
    pub fn record_redis_hit(&mut self) {
        self.redis_cache_hits += 1;
    }

    /// Increments the Redis-cache miss counter.
    pub fn record_redis_miss(&mut self) {
        self.redis_cache_misses += 1;
    }

    /// Increments the circuit-breaker rejection counter.
    pub fn record_circuit_breaker_rejection(&mut self) {
        self.circuit_breaker_rejections += 1;
    }

    /// Increments the database-fallback counter.
    pub fn record_fallback(&mut self) {
        self.fallback_operations += 1;
    }

    /// Increments the total degraded-requests counter.
    pub fn record_degraded_request(&mut self) {
        self.total_degraded_requests += 1;
    }

    /// Returns the combined (local + Redis) hit rate as a percentage in `[0.0, 100.0]`.
    pub fn hit_rate(&self) -> f64 {
        let total = self.local_cache_hits + self.local_cache_misses + self.redis_cache_hits + self.redis_cache_misses;
        if total == 0 {
            return 0.0;
        }
        let hits = self.local_cache_hits + self.redis_cache_hits;
        (hits as f64 / total as f64) * 100.0
    }

    /// Returns the fraction of requests handled in degraded mode as a percentage in `[0.0, 100.0]`.
    pub fn degradation_rate(&self) -> f64 {
        let total = self.total_degraded_requests;
        if total == 0 {
            return 0.0;
        }
        (self.fallback_operations as f64 / total as f64) * 100.0
    }
}

/// Configuration for [`LocalCache`]: capacity and global TTL.
pub struct CacheConfig {
    /// Maximum number of entries the cache may hold.
    pub max_capacity: u64,
    /// Default time-to-live for entries written without a per-key TTL override (seconds).
    pub time_to_live: u64,
}

impl Default for CacheConfig {
    fn default() -> Self {
        Self {
            max_capacity: 100_000, // Increased from 50_000 for better hit rate
            time_to_live: 7200,    // Increased from 3600 (2 hours) for better cache efficiency
        }
    }
}

/// In-process moka-based cache with per-key TTL support and isolated hot namespaces.
///
/// Hot traffic domains (presence, sliding-sync, device-keys, room-state) are routed to
/// independent moka instances so a flood in one domain cannot evict the others' entries.
#[derive(Clone, Debug)]
pub struct LocalCache {
    cache: Cache<String, String>,
    /// D-1: per-key 过期截止时间。moka 0.12 `sync::Cache` 没有 per-entry TTL API，
    /// 用旁路 deadline 表实现「L1 与 L2 Redis 相同的 per-key TTL」。
    /// 用 `RwLock` 而非 `Mutex`：`get_raw` 只读判断，只需 `read()` 锁，
    /// 避免将所有 worker 线程的缓存读串行化在一把排他锁上。
    deadlines: Arc<parking_lot::RwLock<HashMap<String, std::time::Instant>>>,
    /// D-2: 独立命名空间缓存，防止高流量域（如 presence）驱逐
    /// 安全关键数据（如 device_keys、token）。每个命名空间有独立
    /// 的 moka 容量和 deadline 表。
    namespaces: Arc<HashMap<&'static str, NamespaceCache>>,
}

/// D-2: 独立命名空间缓存实例，拥有独立的 moka Cache 和 deadline 表。
#[derive(Clone, Debug)]
struct NamespaceCache {
    cache: Cache<String, String>,
    deadlines: Arc<parking_lot::RwLock<HashMap<String, std::time::Instant>>>,
}

/// D-2: 将缓存键路由到对应的命名空间。
/// 返回 None 表示使用通用缓存实例。
fn route_key(key: &str) -> Option<&'static str> {
    // presence 数据：高写入频率、短 TTL（60s），不应驱逐其他数据
    if key.starts_with("user:") && key.ends_with(":presence") {
        return Some("presence");
    }
    // sliding_sync 去重/扩展数据：高写入量、中等 TTL
    if key.starts_with("sliding_sync:") {
        return Some("sliding_sync");
    }
    // device_keys：安全关键数据，不应被 presence 洪水驱逐
    if key.starts_with("device_keys_bulk:") {
        return Some("device_keys");
    }
    // room_state：中等写入量、中等 TTL
    if key.starts_with("room_state:") {
        return Some("room_state");
    }
    None
}

impl NamespaceCache {
    fn new(max_capacity: u64, ttl_secs: u64) -> Self {
        let deadlines = Arc::new(parking_lot::RwLock::new(HashMap::new()));
        let deadlines_for_listener = Arc::clone(&deadlines);
        let cache = Cache::builder()
            .max_capacity(max_capacity)
            .time_to_live(std::time::Duration::from_secs(ttl_secs))
            .eviction_listener(move |key: Arc<String>, _value, _cause| {
                deadlines_for_listener.write().remove(key.as_str());
            })
            .build();
        Self { cache, deadlines }
    }
}

impl LocalCache {
    /// Constructs a new cache from the given configuration.
    pub fn new(config: &CacheConfig) -> Self {
        let deadlines = Arc::new(parking_lot::RwLock::new(HashMap::new()));
        let deadlines_for_listener = Arc::clone(&deadlines);
        let cache = Cache::builder()
            .max_capacity(config.max_capacity)
            .time_to_live(std::time::Duration::from_secs(config.time_to_live))
            // 容量驱逐时同步清理 deadline，避免旁路表泄漏
            .eviction_listener(move |key: Arc<String>, _value, _cause| {
                deadlines_for_listener.write().remove(key.as_str());
            })
            .build();

        // D-2: 为高流量域创建独立 moka 实例，防止跨域驱逐
        let mut namespaces = HashMap::new();
        // presence: 高写入频率（每个用户每 60s 更新），短 TTL，容量 20K
        namespaces.insert("presence", NamespaceCache::new(20_000, 120));
        // sliding_sync: 去重键/扩展数据，中高写入量，容量 10K
        namespaces.insert("sliding_sync", NamespaceCache::new(10_000, 7200));
        // device_keys: 安全关键数据，不应被洪泛驱逐，容量 10K
        namespaces.insert("device_keys", NamespaceCache::new(10_000, 600));
        // room_state: 中等写入量，容量 20K
        namespaces.insert("room_state", NamespaceCache::new(20_000, 1200));

        Self { cache, deadlines, namespaces: Arc::new(namespaces) }
    }

    /// Looks up the cached claims associated with `token`.
    pub fn get(&self, token: &str) -> Option<Claims> {
        self.cache.get(token).and_then(|s| serde_json::from_str(&s).ok())
    }

    /// Serialises `claims` to JSON and stores it under `token`.
    pub fn set(&self, token: &str, claims: &Claims) {
        match serde_json::to_string(claims) {
            Ok(s) => {
                self.deadlines.write().remove(token);
                self.cache.insert(token.to_string(), s);
            }
            Err(e) => {
                tracing::error!(target: "cache", "Failed to serialize claims: {}", e);
            }
        }
    }

    /// Stores an arbitrary string value under `key` using the default TTL from the cache builder.
    pub fn set_raw(&self, key: &str, value: &str) {
        // D-2: 路由到独立命名空间缓存（如有）
        if let Some(ns_name) = route_key(key) {
            if let Some(ns) = self.namespaces.get(ns_name) {
                ns.deadlines.write().remove(key);
                ns.cache.insert(key.to_string(), value.to_string());
                return;
            }
        }
        // 无 per-key TTL 的普通写入：清除旧 deadline，回落到 builder 级 TTL
        self.deadlines.write().remove(key);
        self.cache.insert(key.to_string(), value.to_string());
    }

    /// D-1: 带独立 TTL 的写入。此前所有条目共用 builder 级 TTL，
    /// 调用方传入的 ttl 只作用于 L2 Redis，L1 与 L2 过期时间不一致。
    pub fn set_raw_with_ttl(&self, key: &str, value: &str, ttl: std::time::Duration) {
        // D-2: 路由到独立命名空间缓存（如有）
        if let Some(ns_name) = route_key(key) {
            if let Some(ns) = self.namespaces.get(ns_name) {
                ns.deadlines.write().insert(key.to_string(), std::time::Instant::now() + ttl);
                ns.cache.insert(key.to_string(), value.to_string());
                return;
            }
        }
        self.deadlines.write().insert(key.to_string(), std::time::Instant::now() + ttl);
        self.cache.insert(key.to_string(), value.to_string());
    }

    /// Retrieves the raw string value stored under `key`, returning `None` if absent or expired.
    pub fn get_raw(&self, key: &str) -> Option<String> {
        // D-2: 路由到独立命名空间缓存（如有）
        if let Some(ns_name) = route_key(key) {
            if let Some(ns) = self.namespaces.get(ns_name) {
                // D-1: per-key 过期判定
                let deadline = ns.deadlines.read().get(key).copied();
                if let Some(deadline) = deadline {
                    if std::time::Instant::now() >= deadline {
                        ns.cache.remove(key);
                        ns.deadlines.write().remove(key);
                        return None;
                    }
                }
                return ns.cache.get(key);
            }
        }
        // D-1: per-key 过期判定（moka 自身只认 builder 级 TTL）
        let deadline = self.deadlines.read().get(key).copied();
        if let Some(deadline) = deadline {
            if std::time::Instant::now() >= deadline {
                self.cache.remove(key);
                self.deadlines.write().remove(key);
                return None;
            }
        }
        self.cache.get(key)
    }

    /// Removes the value stored under `token` from whichever cache instance is routing-target.
    pub fn remove(&self, token: &str) {
        // D-2: 路由到独立命名空间缓存（如有）
        if let Some(ns_name) = route_key(token) {
            if let Some(ns) = self.namespaces.get(ns_name) {
                ns.deadlines.write().remove(token);
                ns.cache.remove(token);
                return;
            }
        }
        self.deadlines.write().remove(token);
        self.cache.remove(token);
    }
}

/// Redis-backed cache with circuit-breaker protection and degradation metrics.
#[derive(Clone, Debug)]
pub struct RedisCache {
    pool: Pool,
    connection_timeout: Duration,
    command_timeout: Duration,
    circuit_breaker: Arc<CircuitBreaker>,
    degradation_metrics: Arc<parking_lot::RwLock<DegradationMetrics>>,
}

impl RedisCache {
    /// Constructs a Redis cache from the given config, creating a new connection pool.
    pub fn new(config: &synapse_common::config::RedisConfig) -> Result<Self, redis::RedisError> {
        let conn_str = config.connection_url();
        let mut cfg = Config::from_url(conn_str);
        cfg.pool = Some(PoolConfig::new(config.pool_size as usize));

        let pool = cfg
            .create_pool(Some(Runtime::Tokio1))
            .map_err(|e| redis::RedisError::from((redis::ErrorKind::IoError, "Pool creation failed", e.to_string())))?;

        let connection_timeout = Duration::from_millis(config.connection_timeout_ms);
        let command_timeout = Duration::from_millis(config.command_timeout_ms);
        let circuit_breaker = Arc::new(CircuitBreaker::new(config.circuit_breaker.clone()));

        Ok(Self {
            pool,
            connection_timeout,
            command_timeout,
            circuit_breaker,
            degradation_metrics: Arc::new(parking_lot::RwLock::new(DegradationMetrics::new())),
        })
    }

    /// Wraps an existing connection pool using the default connection/command timeouts
    /// and the default circuit-breaker configuration.
    pub fn from_pool(pool: Pool) -> Self {
        Self {
            pool,
            connection_timeout: Duration::from_millis(DEFAULT_REDIS_TIMEOUT_MS),
            command_timeout: Duration::from_millis(DEFAULT_REDIS_TIMEOUT_MS),
            circuit_breaker: Arc::new(CircuitBreaker::new(synapse_common::config::CircuitBreakerConfig::default())),
            degradation_metrics: Arc::new(parking_lot::RwLock::new(DegradationMetrics::new())),
        }
    }

    /// Wraps an existing connection pool using the timeouts and circuit-breaker from `config`.
    pub fn from_pool_with_config(pool: Pool, config: &synapse_common::config::RedisConfig) -> Self {
        Self {
            pool,
            connection_timeout: Duration::from_millis(config.connection_timeout_ms),
            command_timeout: Duration::from_millis(config.command_timeout_ms),
            circuit_breaker: Arc::new(CircuitBreaker::new(config.circuit_breaker.clone())),
            degradation_metrics: Arc::new(parking_lot::RwLock::new(DegradationMetrics::new())),
        }
    }

    /// Returns a reference to the circuit breaker used by this Redis cache.
    pub fn get_circuit_breaker(&self) -> &CircuitBreaker {
        &self.circuit_breaker
    }

    /// Returns a clone of the current degradation metrics snapshot.
    pub fn get_degradation_metrics(&self) -> DegradationMetrics {
        self.degradation_metrics.read().clone()
    }

    /// Circuit breaker + connection acquisition helper.
    ///
    /// Handles the common pattern shared by all Redis operations:
    /// 1. Check circuit breaker; record rejection if open
    /// 2. Acquire a connection from the pool with connection timeout
    /// 3. Pass the connection to the caller-supplied closure for command execution
    /// 4. Record success / failure / timeout on the circuit breaker
    ///
    /// The closure `f` receives a `mut Connection` and returns `Result<T, E>`.
    /// - `Ok(t)` → `record_success()`, returns `Ok(t)`
    /// - `Err(e)` where `e` is a timeout → `record_timeout()`, returns `Err(e)`
    /// - `Err(e)` otherwise → `record_failure()`, returns `Err(e)`
    ///
    /// Connection-level failures (pool exhaustion, connection timeout) are also
    /// recorded as failures/timeouts on the circuit breaker.
    async fn with_circuit_breaker<F, Fut, T, E>(&self, operation_name: &str, f: F) -> Result<T, E>
    where
        F: FnOnce(deadpool_redis::Connection) -> Fut,
        Fut: std::future::Future<Output = Result<T, E>>,
        E: From<CircuitBreakerOpen>
            + From<ConnectionTimeout>
            + From<PoolExhaustion>
            + From<CommandTimeout>
            + From<OperationFailed>
            + std::fmt::Debug,
    {
        if !self.circuit_breaker.is_call_allowed() {
            self.degradation_metrics.write().record_circuit_breaker_rejection();
            return Err(E::from(CircuitBreakerOpen { operation: operation_name.to_string() }));
        }

        let conn_result = timeout(self.connection_timeout, self.pool.get()).await;
        let conn = match conn_result {
            Ok(Ok(conn)) => conn,
            Ok(Err(e)) => {
                tracing::error!(target: "cache", "Redis connection failed: {}", e);
                self.circuit_breaker.record_failure();
                return Err(E::from(PoolExhaustion { source: e.to_string() }));
            }
            Err(_) => {
                tracing::warn!(target: "cache", "Redis connection timed out");
                self.circuit_breaker.record_timeout();
                return Err(E::from(ConnectionTimeout { operation: operation_name.to_string() }));
            }
        };

        let cmd_future = f(conn);
        let cmd_result = timeout(self.command_timeout, cmd_future).await;

        match cmd_result {
            Ok(Ok(val)) => {
                self.circuit_breaker.record_success();
                Ok(val)
            }
            Ok(Err(e)) => {
                tracing::error!(target: "cache", "Redis {} command failed: {:?}", operation_name, e);
                self.circuit_breaker.record_failure();
                Err(e)
            }
            Err(_) => {
                tracing::warn!(target: "cache", "Redis {} command timed out", operation_name);
                self.circuit_breaker.record_timeout();
                Err(E::from(CommandTimeout { operation: operation_name.to_string() }))
            }
        }
    }

    /// Retrieves a string value from Redis L2 (using GET).
    ///
    /// Returns `None` for both cache miss and any backend failure (circuit breaker
    /// open, timeout, transport error). Callers requiring failure visibility should
    /// use [`get_checked`](Self::get_checked) instead.
    pub async fn get(&self, key: &str) -> Option<String> {
        use redis::AsyncCommands;
        let result = self
            .with_circuit_breaker("GET", |mut conn| async move {
                conn.get::<_, Option<String>>(key).await.map_err(|_| CacheErrorWrapper::OperationFailed)
            })
            .await;

        match result {
            Ok(val) => {
                if val.is_some() {
                    self.degradation_metrics.write().record_redis_hit();
                } else {
                    self.degradation_metrics.write().record_redis_miss();
                }
                val
            }
            Err(_) => None,
        }
    }

    /// Like [`get`](Self::get) but propagates Redis errors instead of treating
    /// them as a cache miss. Security-critical callers (e.g. account lockout)
    /// must fail closed on Redis outage rather than silently bypassing the lock.
    pub async fn get_checked(&self, key: &str) -> Result<Option<String>, CacheError> {
        use redis::AsyncCommands;
        let result = self
            .with_circuit_breaker("GET", |mut conn| async move {
                conn.get::<_, Option<String>>(key).await.map_err(|e| CacheError::OperationFailed(e.to_string()))
            })
            .await;

        match result {
            Ok(val) => {
                if val.is_some() {
                    self.degradation_metrics.write().record_redis_hit();
                } else {
                    self.degradation_metrics.write().record_redis_miss();
                }
                Ok(val)
            }
            Err(e) => Err(e),
        }
    }

    /// Batch fetch multiple keys from Redis using MGET in a single round-trip.
    ///
    /// Returns a `Vec<Option<String>>` with the same length as `keys`; missing
    /// keys are `None`. On any Redis error (circuit breaker open, timeout, etc.),
    /// returns a Vec of all `None` so callers can treat every key as a cache miss.
    pub async fn get_batch(&self, keys: &[String]) -> Vec<Option<String>> {
        if keys.is_empty() {
            return Vec::new();
        }

        let result = self
            .with_circuit_breaker("MGET", |mut conn| async move {
                let mut cmd = redis::cmd("MGET");
                for key in keys {
                    cmd.arg(key);
                }
                cmd.query_async::<Vec<Option<String>>>(&mut conn).await.map_err(|_| CacheErrorWrapper::OperationFailed)
            })
            .await;

        match result {
            Ok(vals) => {
                let mut metrics = self.degradation_metrics.write();
                for val in &vals {
                    if val.is_some() {
                        metrics.record_redis_hit();
                    } else {
                        metrics.record_redis_miss();
                    }
                }
                vals
            }
            Err(_) => vec![None; keys.len()],
        }
    }

    /// Stores a string value in Redis L2 (using SET or SETEX depending on whether `ttl > 0`).
    pub async fn set(&self, key: &str, value: &str, ttl: u64) -> Result<(), CacheError> {
        use redis::AsyncCommands;
        self.with_circuit_breaker("SET", |mut conn| async move {
            if ttl > 0 { conn.set_ex(key, value, ttl).await } else { conn.set(key, value).await }
                .map_err(|e| CacheError::OperationFailed(e.to_string()))
        })
        .await
    }

    /// C-3: Batch set multiple keys in a single Redis pipeline round-trip.
    ///
    /// Each entry is `(key, value, ttl)`. All SET commands are pipelined so
    /// the network cost is O(1) round-trip regardless of entry count, instead
    /// of O(N) when calling `set()` in a loop.
    pub async fn set_batch(&self, entries: &[(String, String, u64)]) -> Result<(), CacheError> {
        if entries.is_empty() {
            return Ok(());
        }
        self.with_circuit_breaker("MSET", |mut conn| async move {
            let mut pipe = redis::pipe();
            for (key, value, ttl) in entries {
                if *ttl > 0 {
                    pipe.cmd("SET").arg(key).arg(value).arg("EX").arg(*ttl);
                } else {
                    pipe.cmd("SET").arg(key).arg(value);
                }
            }
            pipe.query_async::<()>(&mut conn).await.map_err(|e| CacheError::OperationFailed(e.to_string()))
        })
        .await
    }

    /// Removes a key from Redis L2 (using DEL).
    pub async fn delete(&self, key: &str) -> Result<(), CacheError> {
        use redis::AsyncCommands;
        self.with_circuit_breaker("DELETE", |mut conn| async move {
            conn.del(key).await.map_err(|e| CacheError::OperationFailed(e.to_string()))
        })
        .await
    }

    /// Batch-delete multiple keys in a single Redis DEL command.
    /// Sends one PIPELINE instead of N round-trips.
    pub async fn delete_batch(&self, keys: &[String]) -> Result<(), CacheError> {
        if keys.is_empty() {
            return Ok(());
        }
        self.with_circuit_breaker("DEL (batch)", |mut conn| async move {
            let mut pipe = redis::pipe();
            for key in keys {
                pipe.del(key.as_str());
            }
            pipe.query_async::<()>(&mut conn).await.map_err(|e| CacheError::OperationFailed(e.to_string()))
        })
        .await
    }

    /// Atomically acquire a distributed lock using SET NX EX.
    ///
    /// Returns `Ok(true)` if the lock was acquired (key was set);
    /// `Ok(false)` if the key already existed (lock held by another process).
    /// Returns `Err` if Redis is unavailable or circuit breaker is open.
    pub async fn set_nx(&self, key: &str, value: &str, ttl_secs: u64) -> Result<bool, CacheError> {
        self.with_circuit_breaker("SETNX", |mut conn| async move {
            let mut cmd = redis::cmd("SET");
            cmd.arg(key).arg(value).arg("NX").arg("EX").arg(ttl_secs);
            let result: Option<String> =
                cmd.query_async(&mut conn).await.map_err(|e| CacheError::OperationFailed(e.to_string()))?;
            Ok(result.is_some())
        })
        .await
    }

    /// Release a distributed lock by deleting its key.
    ///
    /// Errors are swallowed — lock release is best-effort. The TTL ensures
    /// the lock auto-expires if the holder crashes.
    pub async fn delete_lock(&self, key: &str) {
        let _: Result<(), CacheErrorWrapper> = self
            .with_circuit_breaker("DEL (lock)", |mut conn| async move {
                use redis::AsyncCommands;
                conn.del(key).await.map_err(|_| CacheErrorWrapper::OperationFailed)
            })
            .await;
    }

    /// Increments a Redis hash field by `delta` (HINCRBY). Used for rate-limit token-bucket.
    pub async fn hincrby(&self, key: &str, field: &str, delta: i64) -> Result<i64, redis::RedisError> {
        use redis::AsyncCommands;
        self.with_circuit_breaker("HINCRBY", |mut conn| async move { conn.hincr(key, field, delta).await }).await
    }

    /// Returns all fields and values of a Redis hash (HGETALL).
    pub async fn hgetall(&self, key: &str) -> Result<HashMap<String, String>, redis::RedisError> {
        use redis::AsyncCommands;
        self.with_circuit_breaker("HGETALL", |mut conn| async move { conn.hgetall(key).await }).await
    }

    /// Extends the TTL of an existing Redis key (EXPIRE). Best-effort — errors are silently swallowed.
    pub async fn expire(&self, key: &str, ttl: u64) {
        use redis::AsyncCommands;
        let _: Result<(), CacheErrorWrapper> = self
            .with_circuit_breaker("EXPIRE", |mut conn| async move {
                conn.expire(key, ttl as i64).await.map_err(|_| CacheErrorWrapper::OperationFailed)
            })
            .await;
    }

    /// Atomic Redis-backed token-bucket rate limiter (Lua script).
    ///
    /// Takes up to `burst_size` tokens from the bucket identified by `key`,
    /// refilling at `rate_per_second`. Returns the resulting [`RateLimitDecision`].
    /// Auto-rejects when the circuit breaker is open.
    pub async fn token_bucket_take(
        &self,
        key: &str,
        now_ms: u64,
        rate_per_second: u32,
        burst_size: u32,
        ttl_seconds: u64,
    ) -> Result<RateLimitDecision, redis::RedisError> {
        if !self.circuit_breaker.is_call_allowed() {
            self.degradation_metrics.write().record_circuit_breaker_rejection();
            return Err(redis::RedisError::from((redis::ErrorKind::IoError, "Circuit breaker is open")));
        }

        let conn_result = timeout(self.connection_timeout, self.pool.get())
            .await
            .map_err(|_| redis::RedisError::from((redis::ErrorKind::IoError, "Redis connection timeout")))?;

        let mut conn = conn_result.map_err(|e| {
            redis::RedisError::from((redis::ErrorKind::IoError, "Redis pool exhaustion", e.to_string()))
        })?;

        let script = redis::Script::new(
            r#"
local key = KEYS[1]
local now = tonumber(ARGV[1])
local rate = tonumber(ARGV[2])
local burst = tonumber(ARGV[3])
local ttl = tonumber(ARGV[4])

local data = redis.call("HMGET", key, "tokens", "ts")
local tokens = tonumber(data[1])
local ts = tonumber(data[2])
if tokens == nil then
  tokens = burst
  ts = now
end

local delta_ms = now - ts
if delta_ms < 0 then
  delta_ms = 0
end

local refill = (delta_ms / 1000.0) * rate
tokens = math.min(burst, tokens + refill)

local allowed = 0
local retry_after = 0
if tokens >= 1 then
  allowed = 1
  tokens = tokens - 1
else
  allowed = 0
  local needed = 1 - tokens
  if rate > 0 then
    retry_after = math.ceil(needed / rate)
  else
    retry_after = 60
  end
end

redis.call("HSET", key, "tokens", tokens, "ts", now)
redis.call("EXPIRE", key, ttl)
local remaining = math.floor(tokens)
return {allowed, retry_after, remaining}
            "#,
        );

        let cmd_result = timeout(
            self.command_timeout,
            script
                .key(key)
                .arg(now_ms as i64)
                .arg(rate_per_second as i64)
                .arg(burst_size as i64)
                .arg(ttl_seconds as i64)
                .invoke_async::<(i64, i64, i64)>(&mut conn),
        )
        .await
        .map_err(|_| redis::RedisError::from((redis::ErrorKind::IoError, "Redis script timeout")))?;

        match cmd_result {
            Ok((allowed, retry_after_seconds, remaining)) => {
                self.circuit_breaker.record_success();
                Ok(RateLimitDecision {
                    allowed: allowed != 0,
                    retry_after_seconds: retry_after_seconds.max(0) as u64,
                    remaining: remaining.max(0) as u32,
                })
            }
            Err(e) => {
                self.circuit_breaker.record_failure();
                Err(e)
            }
        }
    }
}

/// Per-key single-flight guard type used by `get_or_fetch`.
type SingleFlightMap = Arc<tokio::sync::Mutex<HashMap<String, Arc<tokio::sync::Mutex<()>>>>>;

/// Central cache coordinator combining a local in-process cache (L1) with optional Redis (L2).
///
/// `CacheManager` provides `get` / `set` / `remove` / `get_or_fetch` / `try_acquire_lock`
/// operations that first consult the local moka cache and optionally fall through to Redis.
/// Cross-instance invalidation is handled via [`CacheInvalidationManager`].
#[derive(Clone, Debug)]
pub struct CacheManager {
    local: LocalCache,
    redis: Option<Arc<RedisCache>>,
    use_redis: bool,
    rate_limit_local: Arc<moka::sync::Cache<String, LocalRateLimitState>>,
    invalidation_manager: Option<Arc<CacheInvalidationManager>>,
    local_cache_ttl: Duration,
    /// Per-key single-flight guards used by `get_or_fetch` to prevent cache
    /// stampede when a hot key expires. Each entry is an `Arc<Mutex<()>>` that
    /// serializes concurrent fetches for the same key.
    in_flight: SingleFlightMap,
    /// W7+: 限流指标的 counter 句柄缓存。跟随 `CacheManager` 生命周期
    /// （进程内单例 / 测试逐实例隔离），**不用全局 `static`**——全局
    /// OnceLock 会把句柄绑死在第一个见到的 collector 上，测试间互相污染。
    rate_limit_metrics: OnceLock<RateLimitMetrics>,
    /// W7+: 熔断器指标是否已注入（幂等标记，不持有数据）。
    circuit_breaker_metrics_attached: OnceLock<()>,
}

impl CacheManager {
    /// Constructs a [`CacheManager`] that operates in local-only mode (no Redis).
    pub fn new(config: &CacheConfig) -> Self {
        Self {
            local: LocalCache::new(config),
            redis: None,
            use_redis: false,
            rate_limit_local: Arc::new(new_rate_limit_local_cache()),
            invalidation_manager: None,
            local_cache_ttl: Duration::from_secs(DEFAULT_LOCAL_CACHE_TTL_SECS),
            in_flight: Arc::new(tokio::sync::Mutex::new(HashMap::new())),
            rate_limit_metrics: OnceLock::new(),
            circuit_breaker_metrics_attached: OnceLock::new(),
        }
    }

    /// Constructs a [`CacheManager`] backed by a Redis instance at `url`.
    pub fn with_redis(
        config: &synapse_common::config::RedisConfig,
        cache_config: &CacheConfig,
    ) -> Result<Self, redis::RedisError> {
        match RedisCache::new(config) {
            Ok(redis_cache) => {
                let pool = redis_cache.pool.clone();
                let redis_url = config.connection_url();
                let invalidation_config = CacheInvalidationConfig {
                    enabled: true,
                    channel_name: CACHE_INVALIDATION_CHANNEL.to_string(),
                    local_cache_ttl_secs: DEFAULT_LOCAL_CACHE_TTL_SECS,
                    redis_cache_ttl_secs: DEFAULT_REDIS_CACHE_TTL_SECS,
                    instance_id: format!("instance-{}", uuid::Uuid::new_v4()),
                    redis_url,
                };
                let invalidation_manager = Arc::new(CacheInvalidationManager::new(Some(pool), invalidation_config));

                Ok(Self {
                    local: LocalCache::new(cache_config),
                    redis: Some(Arc::new(redis_cache)),
                    use_redis: true,
                    rate_limit_local: Arc::new(new_rate_limit_local_cache()),
                    invalidation_manager: Some(invalidation_manager),
                    local_cache_ttl: Duration::from_secs(DEFAULT_LOCAL_CACHE_TTL_SECS),
                    in_flight: Arc::new(tokio::sync::Mutex::new(HashMap::new())),
                    rate_limit_metrics: OnceLock::new(),
                    circuit_breaker_metrics_attached: OnceLock::new(),
                })
            }
            Err(e) => {
                tracing::warn!("Failed to connect to Redis: {}, using local cache only", e);
                Ok(Self {
                    local: LocalCache::new(cache_config),
                    redis: None,
                    use_redis: false,
                    rate_limit_local: Arc::new(new_rate_limit_local_cache()),
                    invalidation_manager: None,
                    local_cache_ttl: Duration::from_secs(DEFAULT_LOCAL_CACHE_TTL_SECS),
                    in_flight: Arc::new(tokio::sync::Mutex::new(HashMap::new())),
                    rate_limit_metrics: OnceLock::new(),
                    circuit_breaker_metrics_attached: OnceLock::new(),
                })
            }
        }
    }

    /// Constructs a [`CacheManager`] backed by an existing Redis connection pool.
    pub fn with_redis_pool(pool: Pool, cache_config: &CacheConfig) -> Self {
        Self::with_redis_pool_and_url(pool, cache_config, "redis://127.0.0.1:6379")
    }

    /// Constructs a [`CacheManager`] with an explicit pool and Redis URL.
    pub fn with_redis_pool_and_url(pool: Pool, cache_config: &CacheConfig, redis_url: &str) -> Self {
        let redis_cache = RedisCache::from_pool(pool.clone());
        let invalidation_config = CacheInvalidationConfig {
            enabled: true,
            channel_name: CACHE_INVALIDATION_CHANNEL.to_string(),
            local_cache_ttl_secs: DEFAULT_LOCAL_CACHE_TTL_SECS,
            redis_cache_ttl_secs: DEFAULT_REDIS_CACHE_TTL_SECS,
            instance_id: format!("instance-{}", uuid::Uuid::new_v4()),
            redis_url: redis_url.to_string(),
        };
        let invalidation_manager = Arc::new(CacheInvalidationManager::new(Some(pool), invalidation_config));

        Self {
            local: LocalCache::new(cache_config),
            redis: Some(Arc::new(redis_cache)),
            use_redis: true,
            rate_limit_local: Arc::new(new_rate_limit_local_cache()),
            invalidation_manager: Some(invalidation_manager),
            local_cache_ttl: Duration::from_secs(DEFAULT_LOCAL_CACHE_TTL_SECS),
            in_flight: Arc::new(tokio::sync::Mutex::new(HashMap::new())),
            rate_limit_metrics: OnceLock::new(),
            circuit_breaker_metrics_attached: OnceLock::new(),
        }
    }

    /// Constructs a [`CacheManager`] with a connection pool and pub/sub invalidation.
    pub fn with_redis_pool_and_invalidation(
        pool: Pool,
        cache_config: &CacheConfig,
        invalidation_config: &CacheInvalidationConfig,
    ) -> Self {
        let redis_cache = RedisCache::from_pool(pool.clone());
        let invalidation_manager = Arc::new(CacheInvalidationManager::new(Some(pool), invalidation_config.clone()));

        Self {
            local: LocalCache::new(cache_config),
            redis: Some(Arc::new(redis_cache)),
            use_redis: true,
            rate_limit_local: Arc::new(new_rate_limit_local_cache()),
            invalidation_manager: Some(invalidation_manager),
            local_cache_ttl: Duration::from_secs(invalidation_config.local_cache_ttl_secs),
            in_flight: Arc::new(tokio::sync::Mutex::new(HashMap::new())),
            rate_limit_metrics: OnceLock::new(),
            circuit_breaker_metrics_attached: OnceLock::new(),
        }
    }

    /// W7+: 取限流指标句柄，首次调用时向 `collector` 注册 6 个 counter。
    ///
    /// 注册只在进程生命周期内发生一次；之后 `get_or_init` 是一次原子读，
    /// `inc()` 是 `AtomicU64::fetch_add(Relaxed)`——热路径无锁、无全局
    /// mutex 争用。
    ///
    /// **不要**改成每次调用都 `register_counter*`：`MetricsCollector` 的
    /// registry 是覆盖语义的 `HashMap`，反复注册会把旧句柄踢出 registry，
    /// 使 registry 里的值与已发出的计数永久分叉。
    pub fn rate_limit_metrics(&self, collector: &synapse_common::metrics::MetricsCollector) -> &RateLimitMetrics {
        self.rate_limit_metrics.get_or_init(|| RateLimitMetrics::new(collector))
    }

    /// W7+: 把 Redis 熔断器接到 `collector` 上（幂等，重复调用无副作用）。
    ///
    /// `CircuitBreaker::new` 不知道 collector 的存在，指标句柄要在构造后
    /// 注入——这里就是那个注入点，由 `AppState::new` 在启动时调用一次。
    /// 没有这一步，`circuit_breaker_requests_total_*` 与
    /// `circuit_breaker_state` 永远不会被注册（埋了探针没接采集器）。
    ///
    /// Redis 未启用时无熔断器可接，静默跳过。
    pub fn attach_circuit_breaker_metrics(&self, collector: &synapse_common::metrics::MetricsCollector) {
        self.circuit_breaker_metrics_attached.get_or_init(|| {
            if let Some(redis) = self.redis.as_ref() {
                redis.get_circuit_breaker().attach_metrics(collector, "redis");
            }
        });
    }

    /// Starts the background subscription to cross-instance cache invalidation messages.
    pub fn start_invalidation_subscriber(&self) -> Result<(), ApiError> {
        if let Some(im) = &self.invalidation_manager {
            im.start_subscriber()?;
        }
        Ok(())
    }

    /// Returns a reference to the [`CacheInvalidationManager`] if pub/sub is configured.
    pub fn invalidation_manager(&self) -> Option<&Arc<CacheInvalidationManager>> {
        self.invalidation_manager.as_ref()
    }

    /// Returns the configured TTL for local (L1) cache entries.
    pub fn local_cache_ttl(&self) -> Duration {
        self.local_cache_ttl
    }

    /// Removes a specific key from the local (L1) cache.
    pub fn invalidate_local_key(&self, key: &str) {
        self.local.remove(key);
    }

    /// Returns all local cache keys that start with `prefix`.
    pub fn get_keys_with_prefix(&self, prefix: &str) -> Vec<String> {
        let mut keys: Vec<String> =
            self.local.cache.iter().filter(|(k, _)| k.starts_with(prefix)).map(|(k, _)| k.to_string()).collect();
        // D-2: 也搜索命名空间缓存
        for ns in self.local.namespaces.values() {
            keys.extend(ns.cache.iter().filter(|(k, _)| k.starts_with(prefix)).map(|(k, _)| k.to_string()));
        }
        keys
    }

    /// Retrieves a raw string value from the local (L1) cache.
    pub fn get_local_raw(&self, key: &str) -> Option<String> {
        self.local.get_raw(key)
    }

    /// Removes a key from the local (L1) cache.
    pub fn remove_local(&self, key: &str) {
        self.local.remove(key);
    }

    /// Removes all local keys matching a glob `pattern`.
    pub fn invalidate_local_pattern(&self, pattern: &str) {
        let matcher = |k: &str| {
            if pattern.contains('*') {
                let prefix = pattern.trim_end_matches('*');
                k.starts_with(prefix)
            } else {
                k.contains(pattern)
            }
        };

        // D-2: 通用缓存实例
        let keys_to_remove: Vec<String> =
            self.local.cache.iter().filter(|(k, _)| matcher(k)).map(|(k, _)| k.to_string()).collect();
        for key in keys_to_remove {
            self.local.remove(&key);
        }

        // D-2: 命名空间缓存实例
        for ns in self.local.namespaces.values() {
            let ns_keys: Vec<String> =
                ns.cache.iter().filter(|(k, _)| matcher(k)).map(|(k, _)| k.to_string()).collect();
            for key in ns_keys {
                ns.deadlines.write().remove(&key);
                ns.cache.remove(&key);
            }
        }
    }

    /// Clears the entire local (L1) cache including all namespaces.
    pub fn invalidate_local_all(&self) {
        self.local.cache.invalidate_all();
        // D-2: 同时清空所有命名空间缓存
        for ns in self.local.namespaces.values() {
            ns.cache.invalidate_all();
            ns.deadlines.write().clear();
        }
    }

    /// Broadcasts a cache-invalidation event to all instances via Redis pub/sub.
    pub async fn broadcast_invalidation(&self, key: &str, invalidation_type: InvalidationType) -> Result<(), ApiError> {
        // PERF-08: 本地 L1 同步失效。Redis 订阅端会跳过本实例的自回声
        // （sender_instance == instance_id），不在这里处理本地就永远没人处理，
        // 调用方一旦忘记先删本地，本实例 L1 残留陈旧数据直到 TTL 过期。
        self.handle_invalidation_message(&CacheInvalidationMessage::new(
            key.to_string(),
            invalidation_type,
            String::new(),
        ));
        if let Some(im) = &self.invalidation_manager {
            im.broadcaster()
                .ok_or_else(|| ApiError::internal("Invalidation broadcaster not available"))?
                .broadcast_invalidation(key, invalidation_type)
                .await?;
        }
        Ok(())
    }

    /// Returns a channel receiver for cache-invalidation pub/sub messages.
    pub fn subscribe_to_invalidations(&self) -> Option<InvalidationReceiver> {
        self.invalidation_manager.as_ref().and_then(|im| im.subscribe())
    }

    /// Applies a cache-invalidation message to the local L1 cache.
    pub fn handle_invalidation_message(&self, msg: &CacheInvalidationMessage) {
        match msg.invalidation_type {
            InvalidationType::Key => {
                self.local.remove(&msg.key);
            }
            InvalidationType::Pattern => {
                self.invalidate_local_pattern(&msg.key);
            }
            InvalidationType::Prefix => {
                self.invalidate_local_pattern(&msg.key);
            }
            InvalidationType::All => {
                self.local.cache.invalidate_all();
                // D-2: 同时清空所有命名空间缓存
                for ns in self.local.namespaces.values() {
                    ns.cache.invalidate_all();
                    ns.deadlines.write().clear();
                }
            }
        }
    }

    /// Retrieves and validates a JWT from the cache. Returns `None` if absent or expired.
    pub async fn get_token(&self, token: &str) -> Option<Claims> {
        if let Some(claims) = self.local.get(token) {
            if claims.exp >= chrono::Utc::now().timestamp() {
                return Some(claims);
            }
            self.local.remove(token);
            return None;
        }

        if self.use_redis {
            if let Some(redis) = &self.redis {
                if let Some(val) = redis.get(token).await {
                    if let Ok(claims) = serde_json::from_str::<Claims>(&val) {
                        if claims.exp >= chrono::Utc::now().timestamp() {
                            self.local.set(token, &claims);
                            return Some(claims);
                        }
                        // 过期令牌清理：删不掉仅留一条过期记录（下次校验仍会拒绝），无害。
                        let _ = redis.delete(token).await;
                        return None;
                    }
                }
            }
        }
        None
    }

    /// Caches a JWT with the given TTL in both L1 and L2.
    pub async fn set_token(&self, token: &str, claims: &Claims, ttl: u64) {
        // Update L1
        self.local.set(token, claims);
        // Update L2
        if self.use_redis {
            if let Some(redis) = &self.redis {
                if let Ok(val) = serde_json::to_string(claims) {
                    // 纯缓存写：Redis 写失败仅意味着本次不命中缓存（下次走 DB 校验），
                    // DB 是令牌权威来源，fail-open 安全。
                    let _ = redis.set(token, &val, ttl).await;
                }
            }
        }
    }

    /// Revokes a JWT from all cache layers and broadcasts invalidation.
    pub async fn delete_token(&self, token: &str) {
        self.local.remove(token);
        if let Some(redis) = &self.redis {
            // 安全相关：令牌撤销（登出/刷新轮换/吊销）。Redis 删除失败不能再静默
            // 吞掉——被撤销的令牌若残留在跨实例缓存中，其它实例仍可能命中并接受，
            // 构成 fail-open（与 #13 登录锁定 fail-closed 原则一致，改为显式记录）。
            if let Err(e) = redis.delete(token).await {
                ::tracing::error!(target: "cache", error = %e, "Failed to delete revoked token from Redis cache");
            }
        }
        if let Err(e) = self.broadcast_invalidation(token, InvalidationType::Key).await {
            tracing::warn!("Failed to broadcast token invalidation: {}", e);
        }
    }

    /// Returns `Some(true/false)` if the user active flag is cached, `None` if not present.
    pub async fn is_user_active(&self, user_id: &str) -> Option<bool> {
        let key = format!("user:active:{user_id}");
        self.get::<bool>(&key).await.ok().flatten()
    }

    /// Caches the user-active flag (`active`) with the given TTL.
    pub async fn set_user_active(&self, user_id: &str, active: bool, ttl: u64) {
        let key = format!("user:active:{user_id}");
        if let Err(e) = self.set(&key, active, ttl).await {
            ::tracing::error!(target: "cache", "Failed to set user active status: {}", e);
        }
    }

    /// Stores a raw string value with an explicit TTL in both L1 and L2.
    pub async fn set_raw(&self, key: &str, value: &str, ttl: u64) {
        // D-1: L1 也按调用方 TTL 过期，与 L2 Redis 保持一致
        self.local.set_raw_with_ttl(key, value, Duration::from_secs(ttl));
        if let Some(redis) = &self.redis {
            // 纯缓存写：Redis 写失败仅导致跨实例不命中（去重键等），非鉴权语义，fail-open 安全。
            let _ = redis.set(key, value, ttl).await;
        }
    }

    /// Retrieves a raw string value from L1, then falls back to L2.
    pub fn get_raw(&self, key: &str) -> Option<String> {
        self.local.get_raw(key)
    }

    /// S7: Like `get_raw`, but falls back to L2 (Redis) on an L1 miss and
    /// backfills L1 on a hit.
    ///
    /// `set_raw` writes both L1 and L2, while synchronous `get_raw` only reads
    /// L1. Callers whose state must survive cross-instance routing, restarts,
    /// or local eviction (e.g. the sliding-sync presence/e2ee/to-device
    /// de-duplication keys) should use this async variant; a spurious L1 miss
    /// there is misread as "changed" and re-introduces the sync↔presence
    /// busy-loop the de-dup was meant to break.
    pub async fn get_raw_shared(&self, key: &str) -> Option<String> {
        // L1: Local Cache
        if let Some(val) = self.local.get_raw(key) {
            return Some(val);
        }

        // L2: Redis Cache
        if self.use_redis {
            if let Some(redis) = &self.redis {
                if let Some(val) = redis.get(key).await {
                    // Populate L1 so subsequent synchronous reads hit locally.
                    self.local.set_raw(key, &val);
                    return Some(val);
                }
            }
        }
        None
    }

    /// Removes a key from both cache layers.
    pub async fn delete(&self, key: &str) {
        self.local.remove(key);
        if let Some(redis) = &self.redis {
            if let Err(e) = redis.delete(key).await {
                ::tracing::warn!(target: "cache", cache_key = %key, error = %e, "Failed to delete cache entry from Redis");
            }
        }
        if let Err(e) = self.broadcast_invalidation(key, InvalidationType::Key).await {
            tracing::warn!("Failed to broadcast key invalidation: {}", e);
        }
    }

    /// Batch-delete multiple keys. Clears L1 for each, fires a single
    /// Redis `DEL k1 k2 k3 ...` pipeline, and broadcasts one
    /// `InvalidationType::Key` per key (跨实例失效语义保持不变).
    ///
    /// Use when invalidating many keys at once (sliding sync 连接断开时
    /// 清空 1 prefix + 4 精确 key). 把 N 次 RTT 折成 1 次 RTT.
    pub async fn delete_batch(&self, keys: &[String]) {
        if keys.is_empty() {
            return;
        }
        for key in keys {
            self.local.remove(key);
        }
        if let Some(redis) = &self.redis {
            if let Err(e) = redis.delete_batch(keys).await {
                ::tracing::warn!(
                    target: "cache",
                    count = keys.len(),
                    error = %e,
                    "Failed to batch-delete cache entries from Redis"
                );
            }
        }
        for key in keys {
            if let Err(e) = self.broadcast_invalidation(key, InvalidationType::Key).await {
                tracing::warn!("Failed to broadcast key invalidation: {}", e);
            }
        }
    }

    /// Deletes a key and broadcasts a cross-instance invalidation.
    pub async fn delete_with_invalidation(&self, key: &str, invalidation_type: InvalidationType) {
        match invalidation_type {
            InvalidationType::Key => {
                self.local.remove(key);
                if let Some(redis) = &self.redis {
                    if let Err(e) = redis.delete(key).await {
                        ::tracing::warn!(target: "cache", cache_key = %key, error = %e, "Failed to delete cache entry from Redis (with invalidation)");
                    }
                }
            }
            InvalidationType::Pattern | InvalidationType::Prefix => {
                self.invalidate_local_pattern(key);
            }
            InvalidationType::All => {
                self.local.cache.invalidate_all();
                // D-2: 同时清空所有命名空间缓存
                for ns in self.local.namespaces.values() {
                    ns.cache.invalidate_all();
                    ns.deadlines.write().clear();
                }
            }
        }
        if let Err(e) = self.broadcast_invalidation(key, invalidation_type).await {
            tracing::warn!("Failed to broadcast invalidation: {}", e);
        }
    }

    /// Retrieves and deserializes a value, falling back to L2 (Redis) on L1 miss.
    /// Returns `Ok(None)` for cache miss; returns `Err` only on serialization or
    /// non-recoverable backend failure.
    pub async fn get<T: for<'de> Deserialize<'de>>(&self, key: &str) -> Result<Option<T>, ApiError> {
        let key = key.to_string();

        // L1: Local Cache
        if let Some(val) = self.local.get_raw(&key) {
            if let Ok(result) = serde_json::from_str(&val) {
                return Ok(Some(result));
            }
        }

        // L2: Redis Cache
        if self.use_redis {
            if let Some(redis) = &self.redis {
                if let Some(val) = redis.get(&key).await {
                    if let Ok(result) = serde_json::from_str(&val) {
                        // Populate L1
                        self.local.set_raw(&key, &val);
                        return Ok(Some(result));
                    }
                }
            }
        }
        Ok(None)
    }

    /// Like [`get`](Self::get) but propagates Redis errors instead of treating
    /// them as a cache miss. Security-critical callers (e.g. account lockout)
    /// must fail closed on Redis outage rather than silently bypassing the lock.
    pub async fn get_checked<T: for<'de> Deserialize<'de>>(&self, key: &str) -> Result<Option<T>, ApiError> {
        let key = key.to_string();

        // L1: Local Cache
        if let Some(val) = self.local.get_raw(&key) {
            if let Ok(result) = serde_json::from_str(&val) {
                return Ok(Some(result));
            }
        }

        // L2: Redis Cache — propagate errors (fail closed)
        if self.use_redis {
            if let Some(redis) = &self.redis {
                let val = redis
                    .get_checked(&key)
                    .await
                    .map_err(|e| ApiError::internal_with_context("Redis GET failed", &e))?;
                if let Some(val) = val {
                    if let Ok(result) = serde_json::from_str(&val) {
                        // Populate L1
                        self.local.set_raw(&key, &val);
                        return Ok(Some(result));
                    }
                }
            }
        }
        Ok(None)
    }

    /// Batch fetch multiple keys from the cache (L1 local + L2 Redis MGET).
    ///
    /// Returns a `Vec<Option<T>>` with the same length as `keys`; missing keys
    /// are `None`. When Redis is enabled, all L1-missing keys are fetched via a
    /// single MGET round-trip, eliminating N+1 cache queries. When Redis is
    /// disabled, L1 is checked for each key centrally.
    pub async fn get_batch<T: for<'de> Deserialize<'de>>(&self, keys: &[String]) -> Result<Vec<Option<T>>, ApiError> {
        if keys.is_empty() {
            return Ok(Vec::new());
        }

        let mut results: Vec<Option<T>> = Vec::with_capacity(keys.len());
        let mut missing_indices: Vec<usize> = Vec::new();
        let mut missing_keys: Vec<String> = Vec::new();

        // L1: Local Cache - check all keys first
        for (i, key) in keys.iter().enumerate() {
            if let Some(val) = self.local.get_raw(key) {
                if let Ok(result) = serde_json::from_str::<T>(&val) {
                    results.push(Some(result));
                    continue;
                }
            }
            results.push(None);
            missing_indices.push(i);
            missing_keys.push(key.clone());
        }

        if missing_keys.is_empty() {
            return Ok(results);
        }

        // L2: Redis Cache - batch fetch missing keys via MGET
        if self.use_redis {
            if let Some(redis) = &self.redis {
                let raw_values = redis.get_batch(&missing_keys).await;
                for (idx, raw) in raw_values.into_iter().enumerate() {
                    let result_index = missing_indices[idx];
                    if let Some(val) = raw {
                        if let Ok(result) = serde_json::from_str::<T>(&val) {
                            // Populate L1
                            self.local.set_raw(&missing_keys[idx], &val);
                            results[result_index] = Some(result);
                        }
                    }
                }
            }
        }

        Ok(results)
    }

    /// Best-effort cache write: Redis write failures are swallowed (fail-open),
    /// which is safe for pure-cache data whose authority is the database — a
    /// failed write only means the next read misses and falls back to DB.
    /// Security-critical callers must use [`set_checked`](Self::set_checked).
    pub async fn set<T: Serialize>(&self, key: &str, value: T, ttl: u64) -> Result<(), ApiError> {
        if let Ok(val) = serde_json::to_string(&value) {
            self.local.set_raw(key, &val);
            if self.use_redis {
                if let Some(redis) = &self.redis {
                    let _ = redis.set(key, &val, ttl).await;
                }
            }
        }
        Ok(())
    }

    /// Like [`set`](Self::set) but propagates Redis errors instead of silently
    /// swallowing them. Security-critical callers (e.g. account lockout) must
    /// fail closed on Redis outage rather than leaving the lock unset.
    pub async fn set_checked(&self, key: &str, value: &str, ttl: u64) -> Result<(), ApiError> {
        self.local.set_raw(key, value);
        if self.use_redis {
            if let Some(redis) = &self.redis {
                redis
                    .set(key, value, ttl)
                    .await
                    .map_err(|e| ApiError::internal_with_context("Redis SET failed", &e))?;
            }
        }
        Ok(())
    }

    /// Try to acquire a distributed lock (Redis SET NX EX).
    ///
    /// Returns `Ok(true)` if acquired; `Ok(false)` if already held by another
    /// process. Returns `Err` if Redis is unavailable (caller should decide
    /// whether to fail-open or fail-closed).
    pub async fn try_acquire_lock(&self, lock_key: &str, ttl_secs: u64) -> Result<bool, ApiError> {
        if self.use_redis {
            if let Some(redis) = &self.redis {
                redis
                    .set_nx(lock_key, "locked", ttl_secs)
                    .await
                    .map_err(|e| ApiError::internal_with_context("Redis SETNX failed", &e))
            } else {
                Err(ApiError::internal("Redis not available"))
            }
        } else {
            // No Redis: fail-open (let the DB's UNIQUE constraint protect idempotency)
            Ok(true)
        }
    }

    /// Release a distributed lock (best-effort, errors swallowed).
    ///
    /// Lock auto-expires via TTL if the holder crashes, so failures are safe.
    pub async fn release_lock(&self, lock_key: &str) {
        if self.use_redis {
            if let Some(redis) = &self.redis {
                redis.delete_lock(lock_key).await;
            }
        }
    }

    /// C-3: Batch set multiple key-value pairs with a single Redis pipeline.
    ///
    /// Eliminates N+1 cache writes: presence batch updates, room member
    /// batch caching, etc. can now populate L1 + L2 in O(1) Redis round-trip
    /// instead of O(N) individual SET calls.
    ///
    /// Each tuple is `(key, serialized_value, ttl)`. Values must already be
    /// serialized by the caller (typically via `serde_json::to_string`).
    pub async fn set_batch_serialized(&self, entries: &[(String, String, u64)]) -> Result<(), ApiError> {
        if entries.is_empty() {
            return Ok(());
        }
        // L1: set all local entries synchronously
        for (key, value, _ttl) in entries {
            self.local.set_raw(key, value);
        }
        // L2: single Redis pipeline round-trip
        if self.use_redis {
            if let Some(redis) = &self.redis {
                let _ = redis.set_batch(entries).await;
            }
        }
        Ok(())
    }

    /// C-3: Batch set multiple typed values with a single Redis pipeline.
    ///
    /// Convenience wrapper that serializes each value before calling
    /// `set_batch_serialized`.
    pub async fn set_batch<T: Serialize>(&self, entries: &[(String, T, u64)]) -> Result<(), ApiError> {
        if entries.is_empty() {
            return Ok(());
        }
        let serialized: Vec<(String, String, u64)> = entries
            .iter()
            .filter_map(|(key, value, ttl)| serde_json::to_string(value).ok().map(|v| (key.clone(), v, *ttl)))
            .collect();
        self.set_batch_serialized(&serialized).await
    }

    /// Get a value from the cache, or fetch and cache it on miss with single-flight protection.
    ///
    /// This prevents cache stampede when a hot key expires: only one fetch
    /// operation runs for a given key, while concurrent requests wait for it to
    /// complete and then reuse the cached result. On a cache miss the caller
    /// supplied `fetch` closure is invoked; its result is written back to both
    /// L1 (local) and L2 (Redis) caches with the provided `ttl` before being
    /// returned. If the `fetch` fails the error is propagated and nothing is
    /// cached, so the next request will retry.
    pub async fn get_or_fetch<F, Fut, T>(&self, key: &str, ttl: u64, fetch: F) -> Result<T, ApiError>
    where
        F: FnOnce() -> Fut,
        Fut: std::future::Future<Output = Result<T, ApiError>>,
        T: serde::Serialize + serde::de::DeserializeOwned + Clone,
    {
        // Fast path: value already cached.
        if let Some(cached) = self.get::<T>(key).await? {
            return Ok(cached);
        }

        // Single-flight: get or create a per-key mutex so only one fetch runs.
        let mutex = {
            let mut in_flight = self.in_flight.lock().await;
            in_flight.entry(key.to_string()).or_insert_with(|| Arc::new(tokio::sync::Mutex::new(()))).clone()
        };

        // 持有单飞锁执行 fetch；结果（含 Err）在锁释放后统一清理，避免 in_flight
        // 只插不删导致 map 无限增长（#31）。
        let result = {
            let _guard = mutex.lock().await;

            let fetched: Result<T, ApiError> = async {
                // Double-check after acquiring the lock: another request may have
                // already populated the cache while we were waiting.
                if let Some(cached) = self.get::<T>(key).await? {
                    return Ok(cached);
                }

                // Cache miss confirmed under the guard — fetch, cache, and return.
                let value = fetch().await?;
                self.set(key, &value, ttl).await?;
                Ok(value)
            }
            .await;

            fetched
        };

        // 锁已释放（_guard drop），清理单飞条目，防止 map 无限增长。
        self.in_flight.lock().await.remove(key);
        result
    }

    /// Set negative cache for "not found" results to prevent repeated lookups
    pub async fn set_not_found(&self, key: &str, ttl: u64) -> Result<(), ApiError> {
        let marker = "NOT_FOUND";
        self.local.set_raw(key, marker);
        if self.use_redis {
            if let Some(redis) = &self.redis {
                let _ = redis.set(key, marker, ttl).await;
            }
        }
        Ok(())
    }

    /// Check if a key was cached as "not found"
    pub async fn is_not_found(&self, key: &str) -> bool {
        // Check L1
        if let Some(val) = self.local.get_raw(key) {
            if val == "NOT_FOUND" {
                return true;
            }
        }
        // Check L2
        if self.use_redis {
            if let Some(redis) = &self.redis {
                if let Some(val) = redis.get(key).await {
                    if val == "NOT_FOUND" {
                        return true;
                    }
                }
            }
        }
        false
    }

    /// Increments a hash field by `delta`. Used for token-bucket rate limiting.
    pub async fn hincrby(&self, key: &str, field: &str, delta: i64) -> Result<i64, ApiError> {
        if self.use_redis {
            if let Some(redis) = &self.redis {
                return redis
                    .hincrby(key, field, delta)
                    .await
                    .map_err(|e| ApiError::internal_with_context("Redis error", &e));
            }
        }
        Ok(0) // Local cache doesn't support HINCRBY yet, just return 0 or implement later
    }

    /// Returns all fields and values of a Redis hash.
    pub async fn hgetall(&self, key: &str) -> Result<HashMap<String, String>, ApiError> {
        if self.use_redis {
            if let Some(redis) = &self.redis {
                return redis.hgetall(key).await.map_err(|e| ApiError::internal_with_context("Redis error", &e));
            }
        }
        Ok(HashMap::new())
    }

    /// Sets or extends the TTL of an existing key.
    pub async fn expire(&self, key: &str, ttl: u64) {
        if self.use_redis {
            if let Some(redis) = &self.redis {
                redis.expire(key, ttl).await;
            }
        }
    }

    /// Returns `true` when the cache manager has a working Redis backend.
    ///
    /// This is used by the rate-limit layer to decide whether token-bucket
    /// state is shared across workers (Redis) or process-local (in-memory).
    pub fn is_redis_enabled(&self) -> bool {
        self.use_redis
    }

    /// Token-bucket rate limiter backed by Redis.
    ///
    /// # Architecture
    ///
    /// Dual-tier design — two tiers of state for cross-instance sharing:
    /// 1. **L1: local memory** (moka). Process-local only. Cheap, O(1).
    /// 2. **L2: Redis** (Lua script). Shared across all server instances +
    ///    survives restarts. Used for IP-level, `/sync` and sliding-sync
    ///    token buckets.
    ///
    /// Dispatch:
    /// - `redis` present → hit Redis directly (no L1 cache for rate-limit
    ///   state). Redis Lua does HINCRBY + TTL atomically.
    /// - `redis` absent → fall back to L1 local moka bucket. Only correct
    ///   in single-instance deployments; multi-worker would see each
    ///   instance with its own independent bucket.
    ///
    /// Why two tiers? The rate-limit key encodes either an IP, a user+device
    /// pair, or a user+device+kind triple — all of which must be shared
    /// across worker processes so that burst limits are enforced at the
    /// deployment level, not per-instance.
    ///
    /// `fail_open` / `fail_closed` decision is made upstream in the
    /// middleware layer; this function only returns a `Result` with the
    /// Redis error when the Lua script could not be run.
    pub async fn rate_limit_token_bucket_take(
        &self,
        key: &str,
        rate_per_second: u32,
        burst_size: u32,
    ) -> Result<RateLimitDecision, ApiError> {
        let now_ms = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_else(|_| std::time::Duration::from_secs(0))
            .as_millis() as u64;

        let ttl_seconds = {
            let rate = rate_per_second.max(1) as u64;
            let burst = burst_size.max(1) as u64;
            (burst.saturating_mul(2).saturating_div(rate)).max(60)
        };

        if self.use_redis {
            if let Some(redis) = &self.redis {
                return Ok(redis.token_bucket_take(key, now_ms, rate_per_second, burst_size, ttl_seconds).await?);
            }
        }

        // Local in-memory fallback: rate-limit state is NOT shared across workers.
        // Log a one-time warning so operators are aware of the inconsistency risk.
        {
            static WARNED: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);
            if !WARNED.swap(true, std::sync::atomic::Ordering::Relaxed) {
                tracing::warn!(
                    "Rate limiting is using in-memory token bucket (Redis not available). \
                     In multi-worker deployments, rate-limit state will NOT be shared across workers. \
                     Enable Redis for consistent rate limiting."
                );
            }
        }

        // 审计 #3：整个「读状态 → 补充 → 判定 → 扣减 → 写回」必须在单次原子操作内完成。
        //
        // 此前是 `get` 与 `insert` 两步分离。moka 保证单个操作线程安全，但不保证
        // read-modify-write 序列。Tokio 多线程 runtime 下同一 key 的并发请求跑在不同
        // worker 线程，可以同时读到 `tokens >= 1.0` 并全部放行，把 burst 上限放大到
        // 接近并发数——而这恰恰发生在 Redis 不可用、后端已经不健康、最需要限流生效的
        // 时刻。注意 `get`/`insert` 之间没有 `.await` 并不能排除该问题：不同 worker
        // 线程之间是真正并行的。
        //
        // `and_compute_with` 用 key 级锁把同一 key 的 compute 串行化，读改写之间不再
        // 存在窗口。它的闭包只能回传 `Op`，无法直接带出判定结果，所以用一个 `Cell`
        // 把决策带回来（闭包同步执行于调用线程，`RateLimitDecision` 是 `Copy`）。
        let decision = Cell::new(RateLimitDecision { allowed: false, retry_after_seconds: 0, remaining: 0 });

        let _ = self.rate_limit_local.entry_by_ref(key).and_compute_with(|existing| {
            let prev = existing
                .map_or(LocalRateLimitState { tokens: burst_size as f64, last_ms: now_ms }, |entry| entry.into_value());

            let delta_ms = now_ms.saturating_sub(prev.last_ms);
            let refill = (delta_ms as f64 / 1000.0) * (rate_per_second as f64);
            let mut tokens = (prev.tokens + refill).min(burst_size as f64);
            let allowed = tokens >= 1.0;
            let retry_after_seconds = if allowed || rate_per_second == 0 {
                0
            } else {
                ((1.0 - tokens) / (rate_per_second as f64)).ceil().max(1.0) as u64
            };
            if allowed {
                tokens -= 1.0;
            }

            decision.set(RateLimitDecision { allowed, retry_after_seconds, remaining: tokens.floor().max(0.0) as u32 });

            Op::Put(LocalRateLimitState { tokens, last_ms: now_ms })
        });

        Ok(decision.get())
    }
}

/// Rate-limit decision returned by [`CacheManager::rate_limit_token_bucket_take`].
///
/// This is stored inside the `pub struct` so callers can inspect all fields directly
/// without the crate exposing any interior mutability.
#[derive(Debug, Clone, Copy)]
pub struct RateLimitDecision {
    /// `true` if the request is within the rate budget and should be allowed.
    pub allowed: bool,
    /// Seconds the caller should wait before retrying after a `!allowed` decision.
    pub retry_after_seconds: u64,
    /// Tokens remaining in the bucket after this take (clamped at 0).
    pub remaining: u32,
}

#[derive(Clone, Copy, Debug)]
struct LocalRateLimitState {
    tokens: f64,
    last_ms: u64,
}

/// 审查 #6：本地限流桶（Redis 不可用时的降级路径）的容量与 TTL 上限。
/// 此前用无界 `HashMap`，攻击者用随机 IP/账号即可让桶无限增长导致 OOM。
/// 改用带容量 + TTL 的 moka cache 自动驱逐。
const RATE_LIMIT_LOCAL_MAX_ENTRIES: u64 = 100_000;
const RATE_LIMIT_LOCAL_TTL_SECS: u64 = 300;

fn new_rate_limit_local_cache() -> moka::sync::Cache<String, LocalRateLimitState> {
    moka::sync::Cache::builder()
        .max_capacity(RATE_LIMIT_LOCAL_MAX_ENTRIES)
        .time_to_live(std::time::Duration::from_secs(RATE_LIMIT_LOCAL_TTL_SECS))
        .build()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    #[allow(missing_docs)]
    fn test_cache_config_default() {
        let config = CacheConfig::default();
        assert_eq!(config.max_capacity, 100_000);
        assert_eq!(config.time_to_live, 7200);
    }

    #[test]
    #[allow(missing_docs)]
    fn test_cache_config_custom() {
        let config = CacheConfig { max_capacity: 5000, time_to_live: 7200 };
        assert_eq!(config.max_capacity, 5000);
        assert_eq!(config.time_to_live, 7200);
    }

    // 审查 #6：本地限流桶改为 moka cache 后，token bucket 行为不破坏（回归）。
    #[tokio::test]
    #[allow(missing_docs)]
    async fn test_rate_limit_token_bucket_local_fallback_basic() {
        let manager = CacheManager::new(&CacheConfig::default());
        // burst_size=2, rate=1/s：前 2 次允许，第 3 次拒绝
        let d1 = manager.rate_limit_token_bucket_take("test:key", 1, 2).await.unwrap();
        assert!(d1.allowed);
        let d2 = manager.rate_limit_token_bucket_take("test:key", 1, 2).await.unwrap();
        assert!(d2.allowed);
        let d3 = manager.rate_limit_token_bucket_take("test:key", 1, 2).await.unwrap();
        assert!(!d3.allowed, "third take must be rejected after burst exhausted");
    }

    #[tokio::test]
    #[allow(missing_docs)]
    async fn test_rate_limit_token_bucket_distinct_keys_isolated() {
        let manager = CacheManager::new(&CacheConfig::default());
        let a = manager.rate_limit_token_bucket_take("a", 1, 1).await.unwrap();
        let b = manager.rate_limit_token_bucket_take("b", 1, 1).await.unwrap();
        assert!(a.allowed);
        assert!(b.allowed, "distinct keys must have independent buckets");
    }

    // 审计 #3：并发 take 不得突破 burst 上限。
    //
    // 改实现前 `get` 与 `insert` 分离，同一 key 的并发请求会各自读到「令牌充足」
    // 并全部放行，实际放行量接近并发数。这里用 barrier 让所有任务同时进入临界区，
    // 否则任务可能被顺序调度，竞态不会暴露、测试会假绿。
    #[tokio::test(flavor = "multi_thread", worker_threads = 8)]
    #[allow(missing_docs)]
    async fn test_rate_limit_token_bucket_concurrent_take_respects_burst() {
        use std::sync::Arc;

        const BURST: u32 = 5;
        const CONCURRENCY: usize = 64;

        let manager = Arc::new(CacheManager::new(&CacheConfig::default()));
        let barrier = Arc::new(tokio::sync::Barrier::new(CONCURRENCY));

        let mut handles = Vec::with_capacity(CONCURRENCY);
        for _ in 0..CONCURRENCY {
            let manager = Arc::clone(&manager);
            let barrier = Arc::clone(&barrier);
            handles.push(tokio::spawn(async move {
                barrier.wait().await;
                // rate_per_second = 0：不补充令牌，放行量应精确等于 burst。
                manager.rate_limit_token_bucket_take("concurrent:key", 0, BURST).await.unwrap().allowed
            }));
        }

        let mut allowed = 0usize;
        for handle in handles {
            if handle.await.unwrap() {
                allowed += 1;
            }
        }

        assert_eq!(
            allowed, BURST as usize,
            "concurrent takes must respect the burst limit (allowed {allowed}, burst {BURST})"
        );
    }

    #[test]
    #[allow(missing_docs)]
    fn test_local_cache_creation() {
        let config = CacheConfig { max_capacity: 100, time_to_live: 60 };
        let _local_cache = LocalCache::new(&config);
    }

    #[test]
    #[allow(missing_docs)]
    fn test_local_cache_set_raw() {
        let config = CacheConfig::default();
        let cache = LocalCache::new(&config);
        cache.set_raw("test_key", "test_value");
        let result = cache.get_raw("test_key");
        assert_eq!(result, Some("test_value".to_string()));
    }

    #[test]
    #[allow(missing_docs)]
    fn test_local_cache_get_raw() {
        let config = CacheConfig::default();
        let cache = LocalCache::new(&config);
        let result = cache.get_raw("nonexistent");
        assert!(result.is_none());
    }

    #[test]
    #[allow(missing_docs)]
    fn test_local_cache_remove() {
        let config = CacheConfig::default();
        let cache = LocalCache::new(&config);
        cache.set_raw("test_key", "test_value");
        assert!(cache.get_raw("test_key").is_some());
        cache.remove("test_key");
        assert!(cache.get_raw("test_key").is_none());
    }

    #[test]
    #[allow(missing_docs)]
    fn test_cache_manager_new() {
        let config = CacheConfig::default();
        let manager = CacheManager::new(&config);
        assert!(!manager.use_redis);
        assert!(manager.redis.is_none());
    }

    #[tokio::test]
    #[allow(missing_docs)]
    async fn test_cache_manager_set_and_get() {
        let config = CacheConfig::default();
        let manager = CacheManager::new(&config);

        let test_value = "test_value".to_string();
        let _ = manager.set("test_key", &test_value, 60).await;

        let result: Option<String> = manager.get::<String>("test_key").await.unwrap();
        assert_eq!(result, Some(test_value));
    }

    #[tokio::test]
    #[allow(missing_docs)]
    async fn test_cache_manager_delete() {
        let config = CacheConfig::default();
        let manager = CacheManager::new(&config);

        let test_value = "test_value".to_string();
        let _ = manager.set("test_key", &test_value, 60).await;
        assert!(manager.get::<String>("test_key").await.unwrap().is_some());

        let _ = manager.delete("test_key").await;
        assert!(manager.get::<String>("test_key").await.unwrap().is_none());
    }

    // C-3: Batch set/get eliminates N+1 cache writes.
    #[tokio::test]
    #[allow(missing_docs)]
    async fn c3_set_batch_serialized_writes_all_keys_to_local_cache() {
        let manager = CacheManager::new(&CacheConfig::default());
        let entries = vec![
            ("c3:batch:k1".to_string(), "v1".to_string(), 60u64),
            ("c3:batch:k2".to_string(), "v2".to_string(), 60u64),
            ("c3:batch:k3".to_string(), "v3".to_string(), 60u64),
        ];
        manager.set_batch_serialized(&entries).await.expect("set_batch_serialized");

        manager.local.cache.run_pending_tasks();
        assert_eq!(manager.get_raw("c3:batch:k1").as_deref(), Some("v1"));
        assert_eq!(manager.get_raw("c3:batch:k2").as_deref(), Some("v2"));
        assert_eq!(manager.get_raw("c3:batch:k3").as_deref(), Some("v3"));
    }

    #[tokio::test]
    #[allow(missing_docs)]
    async fn c3_set_batch_typed_writes_and_reads_back() {
        let manager = CacheManager::new(&CacheConfig::default());
        let entries: Vec<(String, String, u64)> = vec![
            ("c3:typed:a".to_string(), "alpha".to_string(), 60),
            ("c3:typed:b".to_string(), "beta".to_string(), 60),
        ];
        manager.set_batch(&entries).await.expect("set_batch");

        let keys = vec!["c3:typed:a".to_string(), "c3:typed:b".to_string()];
        let results: Vec<Option<String>> = manager.get_batch(&keys).await.expect("get_batch");
        assert_eq!(results.len(), 2);
        assert_eq!(results[0].as_deref(), Some("alpha"));
        assert_eq!(results[1].as_deref(), Some("beta"));
    }

    #[tokio::test]
    #[allow(missing_docs)]
    async fn c3_set_batch_empty_is_noop() {
        let manager = CacheManager::new(&CacheConfig::default());
        let entries: Vec<(String, String, u64)> = vec![];
        manager.set_batch(&entries).await.expect("set_batch empty should be noop");
        // No panic, no error
    }

    #[tokio::test]
    #[allow(missing_docs)]
    async fn c3_set_batch_serialized_empty_is_noop() {
        let manager = CacheManager::new(&CacheConfig::default());
        let entries: Vec<(String, String, u64)> = vec![];
        manager.set_batch_serialized(&entries).await.expect("set_batch_serialized empty should be noop");
    }

    // PERF-08: broadcast_invalidation 必须同时失效本地 L1——Redis 订阅端
    // 跳过本实例自回声，本地不失效就会残留陈旧数据。
    #[tokio::test]
    #[allow(missing_docs)]
    async fn perf08_broadcast_invalidation_clears_local_key() {
        let manager = CacheManager::new(&CacheConfig::default());
        manager.set_raw("perf08:key", "stale", 600).await;
        // moka 写缓冲：先落实写入，保证可见性确定
        manager.local.cache.run_pending_tasks();
        assert!(manager.get_raw("perf08:key").is_some());

        manager.broadcast_invalidation("perf08:key", InvalidationType::Key).await.expect("broadcast");
        manager.local.cache.run_pending_tasks();
        assert!(manager.get_raw("perf08:key").is_none(), "local L1 must be invalidated by broadcast");
    }

    #[tokio::test]
    #[allow(missing_docs)]
    async fn perf08_broadcast_invalidation_pattern_clears_local() {
        let manager = CacheManager::new(&CacheConfig::default());
        manager.set_raw("perf08:room:1", "a", 600).await;
        manager.set_raw("perf08:room:2", "b", 600).await;
        manager.set_raw("perf08:other", "c", 600).await;
        manager.local.cache.run_pending_tasks();

        manager.broadcast_invalidation("perf08:room:", InvalidationType::Prefix).await.expect("broadcast");
        manager.local.cache.run_pending_tasks();
        assert!(manager.get_raw("perf08:room:1").is_none());
        assert!(manager.get_raw("perf08:room:2").is_none());
        assert!(manager.get_raw("perf08:other").is_some(), "unrelated key must survive prefix invalidation");
    }

    #[tokio::test]
    #[allow(missing_docs)]
    async fn perf08_broadcast_invalidation_all_clears_local() {
        let manager = CacheManager::new(&CacheConfig::default());
        manager.set_raw("perf08:any", "x", 600).await;
        manager.local.cache.run_pending_tasks();
        assert!(manager.get_raw("perf08:any").is_some());

        manager.broadcast_invalidation("*", InvalidationType::All).await.expect("broadcast");
        manager.local.cache.run_pending_tasks();
        assert!(manager.get_raw("perf08:any").is_none());
    }

    // D-1: L1 必须按调用方 TTL 过期（此前 L1 固定用 builder 级 7200s TTL，
    // 与 L2 Redis 的 per-key TTL 不一致）
    #[tokio::test]
    #[allow(missing_docs)]
    async fn d1_set_raw_honors_per_key_ttl_in_local_cache() {
        let manager = CacheManager::new(&CacheConfig::default());
        manager.set_raw("d1:short", "v", 1).await; // 1 秒 TTL
        manager.local.cache.run_pending_tasks();
        assert!(manager.get_raw("d1:short").is_some());

        tokio::time::sleep(std::time::Duration::from_millis(1100)).await;
        assert!(manager.get_raw("d1:short").is_none(), "L1 entry must expire after its per-key TTL");
    }

    #[tokio::test]
    #[allow(missing_docs)]
    async fn d1_set_raw_long_ttl_survives_short_window() {
        let manager = CacheManager::new(&CacheConfig::default());
        manager.set_raw("d1:long", "v", 600).await;
        manager.local.cache.run_pending_tasks();

        tokio::time::sleep(std::time::Duration::from_millis(1100)).await;
        assert!(manager.get_raw("d1:long").is_some(), "600s TTL entry must survive a 1.1s window");
    }

    #[tokio::test]
    #[allow(missing_docs)]
    async fn test_cache_manager_get_nonexistent() {
        let config = CacheConfig::default();
        let manager = CacheManager::new(&config);

        let result: Option<String> = manager.get::<String>("nonexistent").await.unwrap();
        assert!(result.is_none());
    }

    // ── S7: get_raw_shared —— L1 未命中时回源 L2(Redis) 并回填 L1 ──────────
    //
    // presence/e2ee/to-device/list-snapshot 的去重状态通过 set_raw 双写
    // L1+L2，但同步 get_raw 只读 L1。跨实例/重启/本地驱逐后 L1 未命中会被
    // 误判为「已变化」，回声击穿空闲长轮询（忙循环复发开关）。

    /// 构造带 Redis 的 CacheManager；本地 Redis 不可达时返回 None（测试跳过）。
    async fn redis_backed_manager(tag: &str) -> Option<(CacheManager, deadpool_redis::Pool, String)> {
        let pool = deadpool_redis::Config::from_url("redis://127.0.0.1:6379")
            .create_pool(Some(deadpool_redis::Runtime::Tokio1))
            .ok()?;
        let probe = tokio::time::timeout(std::time::Duration::from_millis(800), pool.get()).await.ok()?.ok()?;
        drop(probe);
        let manager = CacheManager::with_redis_pool(pool.clone(), &CacheConfig::default());
        let nanos =
            std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_nanos()).unwrap_or(0);
        let key = format!("s7_get_raw_shared:{tag}:{}:{nanos}", std::process::id());
        Some((manager, pool, key))
    }

    #[tokio::test]
    #[allow(missing_docs)]
    async fn test_get_raw_shared_without_redis_local_miss_returns_none() {
        let manager = CacheManager::new(&CacheConfig::default());
        assert!(manager.get_raw_shared("s7:no_such_key").await.is_none());
    }

    #[tokio::test]
    #[allow(missing_docs)]
    async fn test_get_raw_shared_local_hit() {
        let manager = CacheManager::new(&CacheConfig::default());
        manager.set_raw("s7:local_hit", "v1", 60).await;
        assert_eq!(manager.get_raw_shared("s7:local_hit").await.as_deref(), Some("v1"));
    }

    #[tokio::test]
    #[allow(missing_docs)]
    async fn test_get_raw_shared_falls_back_to_redis_and_backfills_local() {
        use redis::AsyncCommands;
        let Some((manager, pool, key)) = redis_backed_manager("fallback").await else {
            eprintln!("skip: local redis unavailable");
            return;
        };

        // 绕过 manager 直写 Redis，模拟「另一个实例写入 / 本实例重启后 L1 为空」
        {
            let mut conn = pool.get().await.expect("redis conn");
            let _: () = conn.set_ex(&key, "shared_value", 60).await.expect("seed redis");
        }
        assert!(manager.get_raw(&key).is_none(), "前置条件：L1 必须未命中");

        let result = manager.get_raw_shared(&key).await;
        assert_eq!(result.as_deref(), Some("shared_value"), "L1 未命中必须回源 Redis");

        // 回源后应回填 L1，后续同步读直接命中
        assert_eq!(manager.get_raw(&key).as_deref(), Some("shared_value"), "回源后必须回填 L1");

        let mut conn = pool.get().await.expect("redis conn");
        let _: () = conn.del(&key).await.expect("cleanup");
    }

    #[tokio::test]
    #[allow(missing_docs)]
    async fn test_get_raw_shared_redis_miss_returns_none() {
        let Some((manager, _pool, key)) = redis_backed_manager("miss").await else {
            eprintln!("skip: local redis unavailable");
            return;
        };
        assert!(manager.get_raw_shared(&key).await.is_none(), "L1/L2 均未命中必须返回 None");
    }

    #[tokio::test]
    #[allow(missing_docs)]
    async fn test_cache_manager_token_operations() {
        let config = CacheConfig::default();
        let manager = CacheManager::new(&config);

        let now = chrono::Utc::now().timestamp();
        let claims = Claims {
            sub: "test_subject".to_string(),
            user_id: "@test:example.com".to_string(),
            jti: "test-jti-cache-ops".to_string(),
            is_admin: false,
            device_id: Some("DEVICE123".to_string()),
            exp: now + 3600,
            iat: now,
            iss: None,
            aud: None,
        };

        manager.set_token("test_token", &claims, 3600).await;
        let result = manager.get_token("test_token").await;
        assert!(result.is_some());
        assert_eq!(result.unwrap().user_id, "@test:example.com");

        manager.delete_token("test_token").await;
        let result = manager.get_token("test_token").await;
        assert!(result.is_none());
    }

    #[tokio::test]
    #[allow(missing_docs)]
    async fn test_get_or_fetch_miss_populates_cache() {
        let config = CacheConfig::default();
        let manager = CacheManager::new(&config);

        // First call: cache miss → fetch → cache → return.
        let result: String =
            manager.get_or_fetch("gof_key", 60, || async { Ok("fetched_value".to_string()) }).await.unwrap();
        assert_eq!(result, "fetched_value");

        // Second call: cache hit → no fetch needed.
        let fetch_called = Arc::new(std::sync::atomic::AtomicBool::new(false));
        let fetch_called_clone = fetch_called.clone();
        let result: String = manager
            .get_or_fetch("gof_key", 60, || {
                let flag = fetch_called_clone.clone();
                async move {
                    flag.store(true, std::sync::atomic::Ordering::SeqCst);
                    Ok("should_not_be_called".to_string())
                }
            })
            .await
            .unwrap();
        assert_eq!(result, "fetched_value");
        assert!(!fetch_called.load(std::sync::atomic::Ordering::SeqCst));
    }

    #[tokio::test]
    #[allow(missing_docs)]
    async fn test_get_or_fetch_single_flight_only_one_fetch() {
        // Verify that concurrent get_or_fetch calls for the same key trigger
        // the fetch closure exactly once.
        let config = CacheConfig::default();
        let manager = Arc::new(CacheManager::new(&config));

        let fetch_count = Arc::new(std::sync::atomic::AtomicU32::new(0));
        let key = "gof_single_flight";

        // Make the fetch slow so concurrent callers pile up waiting on the
        // single-flight guard before the value is cached.
        let mut handles = Vec::new();
        for _ in 0..10 {
            let manager = manager.clone();
            let fetch_count = fetch_count.clone();
            handles.push(tokio::spawn(async move {
                manager
                    .get_or_fetch::<_, _, String>(key, 60, || {
                        let count = fetch_count.clone();
                        async move {
                            count.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
                            tokio::time::sleep(std::time::Duration::from_millis(50)).await;
                            Ok("single_flight_value".to_string())
                        }
                    })
                    .await
                    .unwrap()
            }));
        }

        for handle in handles {
            let value = handle.await.unwrap();
            assert_eq!(value, "single_flight_value");
        }

        // Only the first request should have run the fetch; the rest reused
        // the cached result via the double-check inside the single-flight guard.
        assert_eq!(fetch_count.load(std::sync::atomic::Ordering::SeqCst), 1);
    }

    #[test]
    #[allow(missing_docs)]
    fn test_claims_struct() {
        let claims = Claims {
            sub: "user_subject".to_string(),
            user_id: "@user:example.com".to_string(),
            jti: "test-jti-claims-struct".to_string(),
            is_admin: false,
            device_id: Some("DEVICE456".to_string()),
            exp: 1234567890,
            iat: 1234567890,
            iss: None,
            aud: None,
        };
        assert_eq!(claims.user_id, "@user:example.com");
        assert_eq!(claims.device_id, Some("DEVICE456".to_string()));
    }

    // ── D-2: 命名空间缓存隔离测试 ──────────────────────────────────

    #[test]
    #[allow(missing_docs)]
    fn d2_route_key_presence() {
        assert_eq!(route_key("user:@alice:example.com:presence"), Some("presence"));
        assert_eq!(route_key("user:@bob:test.org:presence"), Some("presence"));
        assert_eq!(route_key("user:@alice:example.com:profile"), None);
    }

    #[test]
    #[allow(missing_docs)]
    fn d2_route_key_sliding_sync() {
        assert_eq!(route_key("sliding_sync:presence:@alice:dev1"), Some("sliding_sync"));
        assert_eq!(route_key("sliding_sync:e2ee:@alice:dev1"), Some("sliding_sync"));
        assert_eq!(route_key("sliding_sync:filter:abc123"), Some("sliding_sync"));
    }

    #[test]
    #[allow(missing_docs)]
    fn d2_route_key_device_keys() {
        assert_eq!(route_key("device_keys_bulk:@alice:example.com"), Some("device_keys"));
    }

    #[test]
    #[allow(missing_docs)]
    fn d2_route_key_room_state() {
        assert_eq!(route_key("room_state:!abc:example.com"), Some("room_state"));
    }

    #[test]
    #[allow(missing_docs)]
    fn d2_route_key_general() {
        assert_eq!(route_key("token:abc123"), None);
        assert_eq!(route_key("user:@alice:profile"), None);
        assert_eq!(route_key("some_random_key"), None);
    }

    #[test]
    #[allow(missing_docs)]
    fn d2_namespace_isolation_presence_does_not_evict_general() {
        let cache = LocalCache::new(&CacheConfig::default());

        cache.set_raw("token:abc", "token_val");
        cache.set_raw("user:@alice:profile", "profile_val");

        for i in 0..100 {
            cache.set_raw(&format!("user:@user{i}:test:presence"), "presence_val");
        }

        cache.cache.run_pending_tasks();
        assert!(cache.get_raw("token:abc").is_some(), "general cache must survive presence flood");
        assert!(cache.get_raw("user:@alice:profile").is_some(), "general cache must survive presence flood");
        assert!(cache.get_raw("user:@user0:test:presence").is_some());
        assert!(cache.get_raw("user:@user99:test:presence").is_some());
    }

    #[test]
    #[allow(missing_docs)]
    fn d2_invalidate_all_clears_namespaces() {
        let cache = LocalCache::new(&CacheConfig::default());

        cache.set_raw("token:abc", "token_val");
        cache.set_raw("user:@alice:test:presence", "presence_val");
        cache.set_raw("sliding_sync:presence:@bob:dev1", "sync_val");

        assert!(cache.get_raw("token:abc").is_some());
        assert!(cache.get_raw("user:@alice:test:presence").is_some());
        assert!(cache.get_raw("sliding_sync:presence:@bob:dev1").is_some());

        cache.cache.invalidate_all();
        for ns in cache.namespaces.values() {
            ns.cache.invalidate_all();
            ns.deadlines.write().clear();
        }
        cache.cache.run_pending_tasks();
        for ns in cache.namespaces.values() {
            ns.cache.run_pending_tasks();
        }

        assert!(cache.get_raw("token:abc").is_none());
        assert!(cache.get_raw("user:@alice:test:presence").is_none());
        assert!(cache.get_raw("sliding_sync:presence:@bob:dev1").is_none());
    }
    // ── W7+: 熔断 / 限流指标接线 ───────────────────────────────────────
    //
    // 这些测试锁住的是「指标真的被注册」这件事本身。埋点代码即使写对了，
    // 只要注入点（AppState::new 里的 attach 调用）被删，指标就永远为 0 而
    // 没有任何报错——这类「静默失效」必须有测试兜底。

    #[test]
    #[allow(missing_docs)]
    fn test_attach_circuit_breaker_metrics_registers_counters() {
        // create_pool 是 lazy 的，不需要真的有 Redis 在跑——这里只是要一个
        // 带 circuit_breaker 的 RedisCache 实例。
        let pool = deadpool_redis::Config::from_url("redis://127.0.0.1:6379")
            .create_pool(Some(deadpool_redis::Runtime::Tokio1))
            .expect("pool creation is lazy and must not require a live server");
        let cache = CacheManager::with_redis_pool(pool, &CacheConfig::default());
        let collector = synapse_common::metrics::MetricsCollector::new();

        cache.attach_circuit_breaker_metrics(&collector);

        let all = collector.collect_metrics();
        let names: Vec<&str> = all.iter().map(|m| m.name.as_str()).collect();
        for expected in [
            "circuit_breaker_state",
            "circuit_breaker_requests_total_success",
            "circuit_breaker_requests_total_failure",
            "circuit_breaker_requests_total_timeout",
            "circuit_breaker_requests_total_rejected",
        ] {
            assert!(names.contains(&expected), "missing `{expected}`, got {names:?}");
        }
        // 初值：state=Closed(0)，4 个 counter 均为 0
        for m in &all {
            assert_eq!(m.value, 0.0, "{} should start at 0, got {}", m.name, m.value);
        }
    }

    #[test]
    #[allow(missing_docs)]
    fn test_attach_circuit_breaker_metrics_is_idempotent() {
        let pool = deadpool_redis::Config::from_url("redis://127.0.0.1:6379")
            .create_pool(Some(deadpool_redis::Runtime::Tokio1))
            .expect("pool creation is lazy");
        let cache = CacheManager::with_redis_pool(pool, &CacheConfig::default());
        let collector = synapse_common::metrics::MetricsCollector::new();

        // 重复 attach 不能重复注册：MetricsCollector 按 name 覆盖，重复注册
        // 会把已发出的句柄踢出 registry 导致计数分叉。
        cache.attach_circuit_breaker_metrics(&collector);
        let breaker = cache.redis.as_ref().expect("redis").get_circuit_breaker();
        breaker.record_failure();
        cache.attach_circuit_breaker_metrics(&collector);

        let all = collector.collect_metrics();
        let failure = all.iter().find(|m| m.name == "circuit_breaker_requests_total_failure").map_or(0.0, |m| m.value);
        assert_eq!(failure, 1.0, "第二次 attach 不得重置或分叉已有计数");
    }

    #[test]
    #[allow(missing_docs)]
    fn test_attach_circuit_breaker_metrics_without_redis_is_noop() {
        // 无 Redis 时没有熔断器可接，静默跳过（不得 panic）
        let cache = CacheManager::new(&CacheConfig::default());
        let collector = synapse_common::metrics::MetricsCollector::new();
        cache.attach_circuit_breaker_metrics(&collector);
        assert!(collector.collect_metrics().is_empty());
    }
}

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
