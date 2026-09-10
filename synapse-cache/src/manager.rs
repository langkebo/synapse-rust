use super::error::CacheConfig;
use super::local::LocalCache;
use super::remote::RedisCache;
use crate::invalidation::{
    CacheInvalidationConfig, CacheInvalidationManager, CacheInvalidationMessage, InvalidationReceiver,
    InvalidationType, CACHE_INVALIDATION_CHANNEL, DEFAULT_LOCAL_CACHE_TTL_SECS, DEFAULT_REDIS_CACHE_TTL_SECS,
};
use crate::rate_limit_metrics::RateLimitMetrics;
use deadpool_redis::Pool;
use moka::ops::compute::Op;
use serde::{Deserialize, Serialize};
use std::cell::Cell;
use std::collections::HashMap;
use std::sync::{Arc, OnceLock};
use std::time::Duration;
use synapse_common::claims::Claims;
use synapse_common::ApiError;

/// Per-key single-flight guard type used by `get_or_fetch`.
pub(crate) type SingleFlightMap = Arc<tokio::sync::Mutex<HashMap<String, Arc<tokio::sync::Mutex<()>>>>>;

/// Central cache coordinator combining a local in-process cache (L1) with optional Redis (L2).
///
/// `CacheManager` provides `get` / `set` / `remove` / `get_or_fetch` / `try_acquire_lock`
/// operations that first consult the local moka cache and optionally fall through to Redis.
/// Cross-instance invalidation is handled via [`CacheInvalidationManager`].
#[derive(Clone, Debug)]
pub struct CacheManager {
    pub(crate) local: LocalCache,
    pub(crate) redis: Option<Arc<RedisCache>>,
    pub(crate) use_redis: bool,
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
            // P1: honour the per-write `ttl` for the local tier too. Previously
            // this called `local.set_raw`, which ignores `ttl` and falls back to
            // the builder-wide `CacheConfig::time_to_live`; callers that pass a
            // short TTL (or a lockout window such as 900s) silently got a
            // different L1 lifetime than they asked for.
            self.local.set_raw_with_ttl(key, &val, Duration::from_secs(ttl));
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
        // P1: same per-write TTL guarantee as `set`.
        self.local.set_raw_with_ttl(key, value, Duration::from_secs(ttl));
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
