use crate::auth::Claims;
use crate::common::ApiError;
use deadpool_redis::{Config, Pool, Runtime};
use moka::sync::Cache;
use redis::AsyncCommands;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;
use thiserror::Error;
use tokio::time::timeout;

pub mod circuit_breaker;
pub mod federation_signature_cache;
pub mod invalidation;
pub mod query_cache;
pub mod strategy;

pub use circuit_breaker::{CircuitBreaker, CircuitBreakerMetrics, CircuitState};
pub use federation_signature_cache::{
    CacheEntryKey, FederationSignatureCache, KeyRotationCallback, KeyRotationEvent,
    SignatureCacheConfig, SignatureCacheEntry, SignatureCacheStats, DEFAULT_KEY_CACHE_TTL,
    DEFAULT_KEY_ROTATION_GRACE_PERIOD_MS, DEFAULT_SIGNATURE_CACHE_TTL,
};
pub use invalidation::{
    CacheInvalidationBroadcaster, CacheInvalidationConfig, CacheInvalidationManager,
    CacheInvalidationMessage, CacheInvalidationSubscriber, InvalidationReceiver,
    InvalidationType, CACHE_INVALIDATION_CHANNEL, DEFAULT_LOCAL_CACHE_TTL_SECS,
    DEFAULT_REDIS_CACHE_TTL_SECS,
};
pub use query_cache::{CacheConfig as QueryCacheConfig, CacheEntry, CacheStats, QueryCache};
pub use strategy::{CacheKeyBuilder, CacheTtl};

const DEFAULT_REDIS_TIMEOUT_MS: u64 = 500;

#[derive(Debug, Error)]
pub enum CacheError {
    #[error("Redis connection timeout: {0}")]
    ConnectionTimeout(String),
    #[error("Redis command timeout: {0}")]
    CommandTimeout(String),
    #[error("Redis pool exhaustion: {0}")]
    PoolExhaustion(String),
    #[error("Redis operation failed: {0}")]
    OperationFailed(String),
    #[error("Serialization error: {0}")]
    SerializationError(String),
    #[error("Circuit breaker is open: {0}")]
    CircuitBreakerOpen(String),
}

#[derive(Debug, Clone)]
pub struct DegradationMetrics {
    pub local_cache_hits: u64,
    pub local_cache_misses: u64,
    pub redis_cache_hits: u64,
    pub redis_cache_misses: u64,
    pub circuit_breaker_rejections: u64,
    pub fallback_operations: u64,
    pub total_degraded_requests: u64,
}

impl Default for DegradationMetrics {
    fn default() -> Self {
        Self {
            local_cache_hits: 0,
            local_cache_misses: 0,
            redis_cache_hits: 0,
            redis_cache_misses: 0,
            circuit_breaker_rejections: 0,
            fallback_operations: 0,
            total_degraded_requests: 0,
        }
    }
}

impl DegradationMetrics {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn record_local_hit(&mut self) {
        self.local_cache_hits += 1;
    }

    pub fn record_local_miss(&mut self) {
        self.local_cache_misses += 1;
    }

    pub fn record_redis_hit(&mut self) {
        self.redis_cache_hits += 1;
    }

    pub fn record_redis_miss(&mut self) {
        self.redis_cache_misses += 1;
    }

    pub fn record_circuit_breaker_rejection(&mut self) {
        self.circuit_breaker_rejections += 1;
    }

    pub fn record_fallback(&mut self) {
        self.fallback_operations += 1;
    }

    pub fn record_degraded_request(&mut self) {
        self.total_degraded_requests += 1;
    }

    pub fn hit_rate(&self) -> f64 {
        let total = self.local_cache_hits
            + self.local_cache_misses
            + self.redis_cache_hits
            + self.redis_cache_misses;
        if total == 0 {
            return 0.0;
        }
        let hits = self.local_cache_hits + self.redis_cache_hits;
        (hits as f64 / total as f64) * 100.0
    }

    pub fn degradation_rate(&self) -> f64 {
        let total = self.total_degraded_requests;
        if total == 0 {
            return 0.0;
        }
        (self.fallback_operations as f64 / total as f64) * 100.0
    }
}

pub struct CacheConfig {
    pub max_capacity: u64,
    pub time_to_live: u64,
}

impl Default for CacheConfig {
    fn default() -> Self {
        Self {
            max_capacity: 10000,
            time_to_live: 3600,
        }
    }
}

#[derive(Clone, Debug)]
pub struct LocalCache {
    cache: Cache<String, String>,
}

impl LocalCache {
    pub fn new(config: &CacheConfig) -> Self {
        let cache = Cache::builder()
            .max_capacity(config.max_capacity)
            .time_to_live(std::time::Duration::from_secs(config.time_to_live))
            .build();
        Self { cache }
    }

    pub fn get(&self, token: &str) -> Option<Claims> {
        self.cache
            .get(token)
            .and_then(|s| serde_json::from_str(&s).ok())
    }

    pub fn set(&self, token: &str, claims: &Claims) {
        if let Err(e) = serde_json::to_string(claims) {
            tracing::error!(target: "cache", "Failed to serialize claims: {}", e);
        } else if let Ok(s) = serde_json::to_string(claims) {
            self.cache.insert(token.to_string(), s);
        }
    }

    pub fn set_raw(&self, key: &str, value: &str) {
        self.cache.insert(key.to_string(), value.to_string());
    }

    pub fn get_raw(&self, key: &str) -> Option<String> {
        self.cache.get(key)
    }

    pub fn remove(&self, token: &str) {
        self.cache.remove(token);
    }
}

#[derive(Clone, Debug)]
pub struct RedisCache {
    pool: Pool,
    connection_timeout: Duration,
    command_timeout: Duration,
    circuit_breaker: Arc<CircuitBreaker>,
    degradation_metrics: Arc<parking_lot::RwLock<DegradationMetrics>>,
}

impl RedisCache {
    pub async fn new(
        config: &crate::common::config::RedisConfig,
    ) -> Result<Self, redis::RedisError> {
        let conn_str = format!("redis://{}:{}", config.host, config.port);
        let cfg = Config::from_url(conn_str);

        let pool = cfg.create_pool(Some(Runtime::Tokio1)).map_err(|e| {
            redis::RedisError::from((
                redis::ErrorKind::IoError,
                "Pool creation failed",
                e.to_string(),
            ))
        })?;

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

    pub fn from_pool(pool: Pool) -> Self {
        Self {
            pool,
            connection_timeout: Duration::from_millis(DEFAULT_REDIS_TIMEOUT_MS),
            command_timeout: Duration::from_millis(DEFAULT_REDIS_TIMEOUT_MS),
            circuit_breaker: Arc::new(CircuitBreaker::new(
                crate::common::config::CircuitBreakerConfig::default(),
            )),
            degradation_metrics: Arc::new(parking_lot::RwLock::new(DegradationMetrics::new())),
        }
    }

    pub fn from_pool_with_config(pool: Pool, config: &crate::common::config::RedisConfig) -> Self {
        Self {
            pool,
            connection_timeout: Duration::from_millis(config.connection_timeout_ms),
            command_timeout: Duration::from_millis(config.command_timeout_ms),
            circuit_breaker: Arc::new(CircuitBreaker::new(config.circuit_breaker.clone())),
            degradation_metrics: Arc::new(parking_lot::RwLock::new(DegradationMetrics::new())),
        }
    }

    pub fn get_circuit_breaker(&self) -> &CircuitBreaker {
        &self.circuit_breaker
    }

    pub fn get_degradation_metrics(&self) -> DegradationMetrics {
        self.degradation_metrics.read().clone()
    }

    pub async fn get(&self, key: &str) -> Option<String> {
        if !self.circuit_breaker.is_call_allowed() {
            self.degradation_metrics
                .write()
                .record_circuit_breaker_rejection();
            tracing::warn!(target: "cache", "Circuit breaker is open, rejecting Redis GET request");
            return None;
        }

        let conn_result = timeout(self.connection_timeout, self.pool.get()).await;
        match conn_result {
            Ok(Ok(mut conn)) => {
                let cmd_result =
                    timeout(self.command_timeout, conn.get::<_, Option<String>>(key)).await;
                match cmd_result {
                    Ok(Ok(val)) => {
                        self.circuit_breaker.record_success();
                        if val.is_some() {
                            self.degradation_metrics.write().record_redis_hit();
                        } else {
                            self.degradation_metrics.write().record_redis_miss();
                        }
                        val
                    }
                    Ok(Err(e)) => {
                        tracing::error!(target: "cache", "Redis GET command failed: {}", e);
                        self.circuit_breaker.record_failure();
                        None
                    }
                    Err(_) => {
                        tracing::warn!(target: "cache", "Redis GET command timed out");
                        self.circuit_breaker.record_timeout();
                        None
                    }
                }
            }
            Ok(Err(e)) => {
                tracing::error!(target: "cache", "Redis connection failed: {}", e);
                self.circuit_breaker.record_failure();
                None
            }
            Err(_) => {
                tracing::warn!(target: "cache", "Redis connection timed out");
                self.circuit_breaker.record_timeout();
                None
            }
        }
    }

    pub async fn set(&self, key: &str, value: &str, ttl: u64) -> Result<(), CacheError> {
        if !self.circuit_breaker.is_call_allowed() {
            self.degradation_metrics
                .write()
                .record_circuit_breaker_rejection();
            return Err(CacheError::CircuitBreakerOpen(
                "Circuit breaker is open, rejecting Redis SET request".to_string(),
            ));
        }

        let conn_result = timeout(self.connection_timeout, self.pool.get())
            .await
            .map_err(|_| CacheError::ConnectionTimeout("Redis pool get timeout".to_string()))?;

        let mut conn = conn_result.map_err(|e| CacheError::PoolExhaustion(e.to_string()))?;

        let set_result = if ttl > 0 {
            timeout(self.command_timeout, conn.set_ex(key, value, ttl)).await
        } else {
            timeout(self.command_timeout, conn.set(key, value)).await
        };

        match set_result {
            Ok(Ok(())) => {
                self.circuit_breaker.record_success();
                Ok(())
            }
            Ok(Err(e)) => {
                tracing::error!(target: "cache", "Redis SET command failed: {}", e);
                self.circuit_breaker.record_failure();
                Err(CacheError::OperationFailed(e.to_string()))
            }
            Err(_) => {
                tracing::warn!(target: "cache", "Redis SET command timed out");
                self.circuit_breaker.record_timeout();
                Err(CacheError::CommandTimeout(
                    "Redis SET command timeout".to_string(),
                ))
            }
        }
    }

    pub async fn delete(&self, key: &str) -> Result<(), CacheError> {
        if !self.circuit_breaker.is_call_allowed() {
            self.degradation_metrics
                .write()
                .record_circuit_breaker_rejection();
            return Err(CacheError::CircuitBreakerOpen(
                "Circuit breaker is open, rejecting Redis DELETE request".to_string(),
            ));
        }

        let conn_result = timeout(self.connection_timeout, self.pool.get())
            .await
            .map_err(|_| CacheError::ConnectionTimeout("Redis pool get timeout".to_string()))?;

        let mut conn = conn_result.map_err(|e| CacheError::PoolExhaustion(e.to_string()))?;

        match timeout(self.command_timeout, conn.del(key)).await {
            Ok(Ok(())) => {
                self.circuit_breaker.record_success();
                Ok(())
            }
            Ok(Err(e)) => {
                tracing::error!(target: "cache", "Redis DELETE command failed: {}", e);
                self.circuit_breaker.record_failure();
                Err(CacheError::OperationFailed(e.to_string()))
            }
            Err(_) => {
                tracing::warn!(target: "cache", "Redis DELETE command timed out");
                self.circuit_breaker.record_timeout();
                Err(CacheError::CommandTimeout(
                    "Redis DELETE command timeout".to_string(),
                ))
            }
        }
    }

    pub async fn hincrby(
        &self,
        key: &str,
        field: &str,
        delta: i64,
    ) -> Result<i64, redis::RedisError> {
        if !self.circuit_breaker.is_call_allowed() {
            self.degradation_metrics
                .write()
                .record_circuit_breaker_rejection();
            return Err(redis::RedisError::from((
                redis::ErrorKind::IoError,
                "Circuit breaker is open",
            )));
        }

        let conn_result = timeout(self.connection_timeout, self.pool.get())
            .await
            .map_err(|_| {
                redis::RedisError::from((redis::ErrorKind::IoError, "Redis connection timeout"))
            })?;

        let mut conn = conn_result.map_err(|e| {
            redis::RedisError::from((
                redis::ErrorKind::IoError,
                "Redis pool exhaustion",
                e.to_string(),
            ))
        })?;

        let result = timeout(self.command_timeout, conn.hincr(key, field, delta))
            .await
            .map_err(|_| {
                redis::RedisError::from((redis::ErrorKind::IoError, "Redis command timeout"))
            })?;

        match result {
            Ok(val) => {
                self.circuit_breaker.record_success();
                Ok(val)
            }
            Err(e) => {
                self.circuit_breaker.record_failure();
                Err(e)
            }
        }
    }

    pub async fn hgetall(&self, key: &str) -> Result<HashMap<String, String>, redis::RedisError> {
        if !self.circuit_breaker.is_call_allowed() {
            self.degradation_metrics
                .write()
                .record_circuit_breaker_rejection();
            return Err(redis::RedisError::from((
                redis::ErrorKind::IoError,
                "Circuit breaker is open",
            )));
        }

        let conn_result = timeout(self.connection_timeout, self.pool.get())
            .await
            .map_err(|_| {
                redis::RedisError::from((redis::ErrorKind::IoError, "Redis connection timeout"))
            })?;

        let mut conn = conn_result.map_err(|e| {
            redis::RedisError::from((
                redis::ErrorKind::IoError,
                "Redis pool exhaustion",
                e.to_string(),
            ))
        })?;

        let result = timeout(self.command_timeout, conn.hgetall(key))
            .await
            .map_err(|_| {
                redis::RedisError::from((redis::ErrorKind::IoError, "Redis command timeout"))
            })?;

        match result {
            Ok(val) => {
                self.circuit_breaker.record_success();
                Ok(val)
            }
            Err(e) => {
                self.circuit_breaker.record_failure();
                Err(e)
            }
        }
    }

    pub async fn expire(&self, key: &str, ttl: u64) {
        if !self.circuit_breaker.is_call_allowed() {
            self.degradation_metrics
                .write()
                .record_circuit_breaker_rejection();
            return;
        }

        let conn_result = timeout(self.connection_timeout, self.pool.get()).await;
        if let Ok(Ok(mut conn)) = conn_result {
            let result: Result<(), redis::RedisError> =
                timeout(self.command_timeout, conn.expire(key, ttl as i64))
                    .await
                    .unwrap_or(Ok(()));

            match result {
                Ok(()) => self.circuit_breaker.record_success(),
                Err(_) => self.circuit_breaker.record_failure(),
            }
        } else {
            self.circuit_breaker.record_failure();
        }
    }

    pub async fn token_bucket_take(
        &self,
        key: &str,
        now_ms: u64,
        rate_per_second: u32,
        burst_size: u32,
        ttl_seconds: u64,
    ) -> Result<RateLimitDecision, redis::RedisError> {
        if !self.circuit_breaker.is_call_allowed() {
            self.degradation_metrics
                .write()
                .record_circuit_breaker_rejection();
            return Err(redis::RedisError::from((
                redis::ErrorKind::IoError,
                "Circuit breaker is open",
            )));
        }

        let conn_result = timeout(self.connection_timeout, self.pool.get())
            .await
            .map_err(|_| {
                redis::RedisError::from((redis::ErrorKind::IoError, "Redis connection timeout"))
            })?;

        let mut conn = conn_result.map_err(|e| {
            redis::RedisError::from((
                redis::ErrorKind::IoError,
                "Redis pool exhaustion",
                e.to_string(),
            ))
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
        .map_err(|_| {
            redis::RedisError::from((redis::ErrorKind::IoError, "Redis script timeout"))
        })?;

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

#[derive(Clone, Debug)]
pub struct CacheManager {
    local: LocalCache,
    redis: Option<Arc<RedisCache>>,
    use_redis: bool,
    rate_limit_local: Arc<parking_lot::Mutex<HashMap<String, LocalRateLimitState>>>,
    invalidation_manager: Option<Arc<CacheInvalidationManager>>,
    local_cache_ttl: Duration,
}

impl CacheManager {
    pub fn new(config: CacheConfig) -> Self {
        Self {
            local: LocalCache::new(&config),
            redis: None,
            use_redis: false,
            rate_limit_local: Arc::new(parking_lot::Mutex::new(HashMap::new())),
            invalidation_manager: None,
            local_cache_ttl: Duration::from_secs(DEFAULT_LOCAL_CACHE_TTL_SECS),
        }
    }

    pub async fn with_redis(
        config: &crate::common::config::RedisConfig,
        cache_config: CacheConfig,
    ) -> Result<Self, redis::RedisError> {
        match RedisCache::new(config).await {
            Ok(redis_cache) => {
                let pool = redis_cache.pool.clone();
                let redis_url = format!("redis://{}:{}", config.host, config.port);
                let invalidation_config = CacheInvalidationConfig {
                    enabled: true,
                    channel_name: CACHE_INVALIDATION_CHANNEL.to_string(),
                    local_cache_ttl_secs: DEFAULT_LOCAL_CACHE_TTL_SECS,
                    redis_cache_ttl_secs: DEFAULT_REDIS_CACHE_TTL_SECS,
                    instance_id: format!("instance-{}", uuid::Uuid::new_v4()),
                    redis_url,
                };
                let invalidation_manager = Arc::new(CacheInvalidationManager::new(
                    Some(pool),
                    invalidation_config,
                ));

                Ok(Self {
                    local: LocalCache::new(&cache_config),
                    redis: Some(Arc::new(redis_cache)),
                    use_redis: true,
                    rate_limit_local: Arc::new(parking_lot::Mutex::new(HashMap::new())),
                    invalidation_manager: Some(invalidation_manager),
                    local_cache_ttl: Duration::from_secs(DEFAULT_LOCAL_CACHE_TTL_SECS),
                })
            }
            Err(e) => {
                tracing::warn!("Failed to connect to Redis: {}, using local cache only", e);
                Ok(Self {
                    local: LocalCache::new(&cache_config),
                    redis: None,
                    use_redis: false,
                    rate_limit_local: Arc::new(parking_lot::Mutex::new(HashMap::new())),
                    invalidation_manager: None,
                    local_cache_ttl: Duration::from_secs(DEFAULT_LOCAL_CACHE_TTL_SECS),
                })
            }
        }
    }

    pub fn with_redis_pool(pool: Pool, cache_config: CacheConfig) -> Self {
        Self::with_redis_pool_and_url(pool, cache_config, "redis://127.0.0.1:6379")
    }

    pub fn with_redis_pool_and_url(pool: Pool, cache_config: CacheConfig, redis_url: &str) -> Self {
        let redis_cache = RedisCache::from_pool(pool.clone());
        let invalidation_config = CacheInvalidationConfig {
            enabled: true,
            channel_name: CACHE_INVALIDATION_CHANNEL.to_string(),
            local_cache_ttl_secs: DEFAULT_LOCAL_CACHE_TTL_SECS,
            redis_cache_ttl_secs: DEFAULT_REDIS_CACHE_TTL_SECS,
            instance_id: format!("instance-{}", uuid::Uuid::new_v4()),
            redis_url: redis_url.to_string(),
        };
        let invalidation_manager = Arc::new(CacheInvalidationManager::new(
            Some(pool),
            invalidation_config,
        ));

        Self {
            local: LocalCache::new(&cache_config),
            redis: Some(Arc::new(redis_cache)),
            use_redis: true,
            rate_limit_local: Arc::new(parking_lot::Mutex::new(HashMap::new())),
            invalidation_manager: Some(invalidation_manager),
            local_cache_ttl: Duration::from_secs(DEFAULT_LOCAL_CACHE_TTL_SECS),
        }
    }

    pub fn with_redis_pool_and_invalidation(
        pool: Pool,
        cache_config: CacheConfig,
        invalidation_config: CacheInvalidationConfig,
    ) -> Self {
        let redis_cache = RedisCache::from_pool(pool.clone());
        let invalidation_manager = Arc::new(CacheInvalidationManager::new(
            Some(pool),
            invalidation_config.clone(),
        ));

        Self {
            local: LocalCache::new(&cache_config),
            redis: Some(Arc::new(redis_cache)),
            use_redis: true,
            rate_limit_local: Arc::new(parking_lot::Mutex::new(HashMap::new())),
            invalidation_manager: Some(invalidation_manager),
            local_cache_ttl: Duration::from_secs(invalidation_config.local_cache_ttl_secs),
        }
    }

    pub async fn start_invalidation_subscriber(&self) -> Result<(), ApiError> {
        if let Some(im) = &self.invalidation_manager {
            im.start_subscriber().await?;
        }
        Ok(())
    }

    pub fn invalidation_manager(&self) -> Option<&Arc<CacheInvalidationManager>> {
        self.invalidation_manager.as_ref()
    }

    pub fn local_cache_ttl(&self) -> Duration {
        self.local_cache_ttl
    }

    pub fn invalidate_local_key(&self, key: &str) {
        self.local.remove(key);
    }

    pub fn get_keys_with_prefix(&self, prefix: &str) -> Vec<String> {
        self.local
            .cache
            .iter()
            .filter(|(k, _)| k.starts_with(prefix))
            .map(|(k, _)| k.to_string())
            .collect()
    }

    pub fn get_local_raw(&self, key: &str) -> Option<String> {
        self.local.get_raw(key)
    }

    pub fn remove_local(&self, key: &str) {
        self.local.remove(key);
    }

    pub fn invalidate_local_pattern(&self, pattern: &str) {
        let keys_to_remove: Vec<String> = self
            .local
            .cache
            .iter()
            .filter(|(k, _)| {
                if pattern.contains('*') {
                    let prefix = pattern.trim_end_matches('*');
                    k.starts_with(prefix)
                } else {
                    k.contains(pattern)
                }
            })
            .map(|(k, _)| k.to_string())
            .collect();

        for key in keys_to_remove {
            self.local.remove(&key);
        }
    }

    pub fn invalidate_local_all(&self) {
        self.local.cache.invalidate_all();
    }

    pub async fn broadcast_invalidation(
        &self,
        key: &str,
        invalidation_type: InvalidationType,
    ) -> Result<(), ApiError> {
        if let Some(im) = &self.invalidation_manager {
            im.broadcaster()
                .ok_or_else(|| ApiError::internal("Invalidation broadcaster not available"))?
                .broadcast_invalidation(key, invalidation_type)
                .await?;
        }
        Ok(())
    }

    pub fn subscribe_to_invalidations(&self) -> Option<InvalidationReceiver> {
        self.invalidation_manager.as_ref().and_then(|im| im.subscribe())
    }

    pub async fn handle_invalidation_message(&self, msg: &CacheInvalidationMessage) {
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
            }
        }
    }

    pub async fn get_token(&self, token: &str) -> Option<Claims> {
        // L1: Local Cache
        if let Some(claims) = self.local.get(token) {
            return Some(claims);
        }

        // L2: Redis Cache
        if self.use_redis {
            if let Some(redis) = &self.redis {
                if let Some(val) = redis.get(token).await {
                    if let Ok(claims) = serde_json::from_str(&val) {
                        // Populate L1
                        self.local.set(token, &claims);
                        return Some(claims);
                    }
                }
            }
        }
        None
    }

    pub async fn set_token(&self, token: &str, claims: &Claims, ttl: u64) {
        // Update L1
        self.local.set(token, claims);
        // Update L2
        if self.use_redis {
            if let Some(redis) = &self.redis {
                if let Ok(val) = serde_json::to_string(claims) {
                    let _ = redis.set(token, &val, ttl).await;
                }
            }
        }
    }

    pub async fn delete_token(&self, token: &str) {
        self.local.remove(token);
        if let Some(redis) = &self.redis {
            let _ = redis.delete(token).await;
        }
        if let Err(e) = self.broadcast_invalidation(token, InvalidationType::Key).await {
            tracing::warn!("Failed to broadcast token invalidation: {}", e);
        }
    }

    pub async fn is_user_active(&self, user_id: &str) -> Option<bool> {
        let key = format!("user:active:{}", user_id);
        self.get::<bool>(&key).await.ok().flatten()
    }

    pub async fn set_user_active(&self, user_id: &str, active: bool, ttl: u64) {
        let key = format!("user:active:{}", user_id);
        if let Err(e) = self.set(&key, active, ttl).await {
            ::tracing::error!(target: "cache", "Failed to set user active status: {}", e);
        }
    }

    pub async fn delete(&self, key: &str) {
        self.local.remove(key);
        if let Some(redis) = &self.redis {
            let _ = redis.delete(key).await;
        }
        if let Err(e) = self.broadcast_invalidation(key, InvalidationType::Key).await {
            tracing::warn!("Failed to broadcast key invalidation: {}", e);
        }
    }

    pub async fn delete_with_invalidation(&self, key: &str, invalidation_type: InvalidationType) {
        match invalidation_type {
            InvalidationType::Key => {
                self.local.remove(key);
                if let Some(redis) = &self.redis {
                    let _ = redis.delete(key).await;
                }
            }
            InvalidationType::Pattern | InvalidationType::Prefix => {
                self.invalidate_local_pattern(key);
            }
            InvalidationType::All => {
                self.local.cache.invalidate_all();
            }
        }
        if let Err(e) = self.broadcast_invalidation(key, invalidation_type).await {
            tracing::warn!("Failed to broadcast invalidation: {}", e);
        }
    }

    pub async fn get<T: for<'de> Deserialize<'de>>(
        &self,
        key: &str,
    ) -> Result<Option<T>, ApiError> {
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

    pub async fn hincrby(&self, key: &str, field: &str, delta: i64) -> Result<i64, ApiError> {
        if self.use_redis {
            if let Some(redis) = &self.redis {
                return redis
                    .hincrby(key, field, delta)
                    .await
                    .map_err(|e| ApiError::internal(format!("Redis error: {}", e)));
            }
        }
        Ok(0) // Local cache doesn't support HINCRBY yet, just return 0 or implement later
    }

    pub async fn hgetall(&self, key: &str) -> Result<HashMap<String, String>, ApiError> {
        if self.use_redis {
            if let Some(redis) = &self.redis {
                return redis
                    .hgetall(key)
                    .await
                    .map_err(|e| ApiError::internal(format!("Redis error: {}", e)));
            }
        }
        Ok(HashMap::new())
    }

    pub async fn expire(&self, key: &str, ttl: u64) {
        if self.use_redis {
            if let Some(redis) = &self.redis {
                redis.expire(key, ttl).await;
            }
        }
    }

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
                return Ok(redis
                    .token_bucket_take(key, now_ms, rate_per_second, burst_size, ttl_seconds)
                    .await?);
            }
        }

        let mut map = self.rate_limit_local.lock();
        let state = map.get(key).copied().unwrap_or(LocalRateLimitState {
            tokens: burst_size as f64,
            last_ms: now_ms,
        });

        let delta_ms = now_ms.saturating_sub(state.last_ms);
        let refill = (delta_ms as f64 / 1000.0) * (rate_per_second as f64);
        let mut tokens = (state.tokens + refill).min(burst_size as f64);
        let allowed = tokens >= 1.0;
        let retry_after_seconds = if allowed || rate_per_second == 0 {
            0
        } else {
            ((1.0 - tokens) / (rate_per_second as f64)).ceil().max(1.0) as u64
        };
        if allowed {
            tokens -= 1.0;
        }

        map.insert(
            key.to_string(),
            LocalRateLimitState {
                tokens,
                last_ms: now_ms,
            },
        );

        Ok(RateLimitDecision {
            allowed,
            retry_after_seconds,
            remaining: tokens.floor().max(0.0) as u32,
        })
    }
}

#[derive(Debug, Clone, Copy)]
pub struct RateLimitDecision {
    pub allowed: bool,
    pub retry_after_seconds: u64,
    pub remaining: u32,
}

#[derive(Clone, Copy, Debug)]
struct LocalRateLimitState {
    tokens: f64,
    last_ms: u64,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_cache_config_default() {
        let config = CacheConfig::default();
        assert_eq!(config.max_capacity, 10000);
        assert_eq!(config.time_to_live, 3600);
    }

    #[test]
    fn test_cache_config_custom() {
        let config = CacheConfig {
            max_capacity: 5000,
            time_to_live: 7200,
        };
        assert_eq!(config.max_capacity, 5000);
        assert_eq!(config.time_to_live, 7200);
    }

    #[test]
    fn test_local_cache_creation() {
        let config = CacheConfig {
            max_capacity: 100,
            time_to_live: 60,
        };
        let _local_cache = LocalCache::new(&config);
    }

    #[test]
    fn test_local_cache_set_raw() {
        let config = CacheConfig::default();
        let cache = LocalCache::new(&config);
        cache.set_raw("test_key", "test_value");
        let result = cache.get_raw("test_key");
        assert_eq!(result, Some("test_value".to_string()));
    }

    #[test]
    fn test_local_cache_get_raw() {
        let config = CacheConfig::default();
        let cache = LocalCache::new(&config);
        let result = cache.get_raw("nonexistent");
        assert!(result.is_none());
    }

    #[test]
    fn test_local_cache_remove() {
        let config = CacheConfig::default();
        let cache = LocalCache::new(&config);
        cache.set_raw("test_key", "test_value");
        assert!(cache.get_raw("test_key").is_some());
        cache.remove("test_key");
        assert!(cache.get_raw("test_key").is_none());
    }

    #[test]
    fn test_cache_manager_new() {
        let config = CacheConfig::default();
        let manager = CacheManager::new(config);
        assert!(!manager.use_redis);
        assert!(manager.redis.is_none());
    }

    #[tokio::test]
    async fn test_cache_manager_set_and_get() {
        let config = CacheConfig::default();
        let manager = CacheManager::new(config);

        let test_value = "test_value".to_string();
        let _ = manager.set("test_key", &test_value, 60).await;

        let result: Option<String> = manager.get::<String>("test_key").await.unwrap();
        assert_eq!(result, Some(test_value));
    }

    #[tokio::test]
    async fn test_cache_manager_delete() {
        let config = CacheConfig::default();
        let manager = CacheManager::new(config);

        let test_value = "test_value".to_string();
        let _ = manager.set("test_key", &test_value, 60).await;
        assert!(manager.get::<String>("test_key").await.unwrap().is_some());

        let _ = manager.delete("test_key").await;
        assert!(manager.get::<String>("test_key").await.unwrap().is_none());
    }

    #[tokio::test]
    async fn test_cache_manager_get_nonexistent() {
        let config = CacheConfig::default();
        let manager = CacheManager::new(config);

        let result: Option<String> = manager.get::<String>("nonexistent").await.unwrap();
        assert!(result.is_none());
    }

    #[tokio::test]
    async fn test_cache_manager_token_operations() {
        let config = CacheConfig::default();
        let manager = CacheManager::new(config);

        let claims = Claims {
            sub: "test_subject".to_string(),
            user_id: "@test:example.com".to_string(),
            admin: false,
            device_id: Some("DEVICE123".to_string()),
            exp: 1234567890,
            iat: 1234567890,
        };

        manager.set_token("test_token", &claims, 3600).await;
        let result = manager.get_token("test_token").await;
        assert!(result.is_some());
        assert_eq!(result.unwrap().user_id, "@test:example.com");

        manager.delete_token("test_token").await;
        let result = manager.get_token("test_token").await;
        assert!(result.is_none());
    }

    #[test]
    fn test_claims_struct() {
        let claims = Claims {
            sub: "user_subject".to_string(),
            user_id: "@user:example.com".to_string(),
            admin: false,
            device_id: Some("DEVICE456".to_string()),
            exp: 1234567890,
            iat: 1234567890,
        };
        assert_eq!(claims.user_id, "@user:example.com");
        assert_eq!(claims.device_id, Some("DEVICE456".to_string()));
    }
}

pub mod compression {
    use std::io::{Read, Write};

    const COMPRESSION_THRESHOLD: usize = 1024;

    pub fn compress(data: &[u8]) -> Result<Vec<u8>, &'static str> {
        if data.len() < COMPRESSION_THRESHOLD {
            let mut result = Vec::with_capacity(data.len() + 1);
            result.push(0);
            result.extend_from_slice(data);
            Ok(result)
        } else {
            let mut encoder =
                flate2::write::GzEncoder::new(Vec::new(), flate2::Compression::new(6));
            encoder.write_all(data).map_err(|_| "Failed to compress")?;
            let compressed = encoder
                .finish()
                .map_err(|_| "Failed to finish compression")?;

            let mut result = Vec::with_capacity(compressed.len() + 1);
            result.push(1);
            result.extend_from_slice(&compressed);
            Ok(result)
        }
    }

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
            decoder
                .read_to_end(&mut decompressed)
                .map_err(|_| "Failed to decompress")?;
            Ok(decompressed)
        }
    }

    pub fn compress_string(s: &str) -> Result<Vec<u8>, &'static str> {
        compress(s.as_bytes())
    }

    pub fn decompress_to_string(data: &[u8]) -> Result<String, &'static str> {
        decompress(data).and_then(|bytes| String::from_utf8(bytes).map_err(|_| "Invalid UTF-8"))
    }

    pub fn should_compress(data: &[u8]) -> bool {
        data.len() >= COMPRESSION_THRESHOLD
    }
}

#[cfg(test)]
mod compression_tests {
    use super::compression::*;

    #[test]
    fn test_compress_decompress_roundtrip() {
        let original = b"Hello, World! This is a test string for compression.";

        let compressed = compress(original).unwrap();
        assert!(!compressed.is_empty());

        let decompressed = decompress(&compressed).unwrap();
        assert_eq!(decompressed, original);
    }

    #[test]
    fn test_small_data_not_compressed() {
        let original = b"small";

        let compressed = compress(original).unwrap();
        assert_eq!(compressed[0], 0);
        assert_eq!(&compressed[1..], original);
    }

    #[test]
    fn test_compress_string_roundtrip() {
        let original = "Test string with unicode: 你好世界 🌍";

        let compressed = compress_string(original).unwrap();
        let decompressed = decompress_to_string(&compressed).unwrap();
        assert_eq!(decompressed, original);
    }

    #[test]
    fn test_decompress_empty() {
        let result = decompress(&[]);
        assert!(result.is_err());
    }

    #[test]
    fn test_compress_decompress_large_data() {
        let original: Vec<u8> = (0..10000).map(|i| (i % 256) as u8).collect();

        let compressed = compress(&original).unwrap();
        let decompressed = decompress(&compressed).unwrap();
        assert_eq!(decompressed, original);
    }
}
