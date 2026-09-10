use thiserror::Error;

/// Default timeout for Redis connections, in milliseconds.
pub(crate) const DEFAULT_REDIS_TIMEOUT_MS: u64 = 500;

/// Intermediate error types used by `with_circuit_breaker` to uniformly convert
/// circuit-breaker / connection / timeout failures into different error types
/// (`CacheError` or `redis::RedisError`).

#[derive(Debug)]
pub(crate) struct CircuitBreakerOpen {
    pub(crate) operation: String,
}

#[derive(Debug)]
pub(crate) struct ConnectionTimeout {
    pub(crate) operation: String,
}

#[derive(Debug)]
pub(crate) struct PoolExhaustion {
    pub(crate) source: String,
}

#[derive(Debug)]
pub(crate) struct CommandTimeout {
    pub(crate) operation: String,
}

#[derive(Debug)]
pub(crate) struct OperationFailed {
    pub(crate) detail: String,
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
pub(crate) enum CacheErrorWrapper {
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
