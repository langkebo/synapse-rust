use crate::circuit_breaker::CircuitBreaker;
use crate::error::{
    CacheError, CacheErrorWrapper, CircuitBreakerOpen, CommandTimeout, ConnectionTimeout, DegradationMetrics,
    OperationFailed, PoolExhaustion, DEFAULT_REDIS_TIMEOUT_MS,
};
use crate::manager::RateLimitDecision;
use deadpool_redis::{Config, Pool, PoolConfig, Runtime};
use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;
use tokio::time::timeout;

/// Redis-backed cache with circuit-breaker protection and degradation metrics.
#[derive(Clone, Debug)]
pub struct RedisCache {
    pub(crate) pool: Pool,
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
