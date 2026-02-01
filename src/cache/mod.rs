use crate::auth::Claims;
use crate::common::ApiError;
use deadpool_redis::{Config, Pool, Runtime};
use moka::sync::Cache;
use redis::AsyncCommands;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;
use tokio::time::timeout;

pub mod strategy;

pub use strategy::{CacheKeyBuilder, CacheTtl};

const REDIS_TIMEOUT: Duration = Duration::from_millis(200);

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
        if let Ok(s) = serde_json::to_string(claims) {
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

pub struct RedisCache {
    pool: Pool,
}

impl RedisCache {
    pub async fn new(conn_str: &str) -> Result<Self, redis::RedisError> {
        let cfg = Config::from_url(conn_str);
        let pool = cfg.create_pool(Some(Runtime::Tokio1)).map_err(|e| {
            redis::RedisError::from((
                redis::ErrorKind::IoError,
                "Pool creation failed",
                e.to_string(),
            ))
        })?;
        Ok(Self { pool })
    }

    pub async fn get(&self, key: &str) -> Option<String> {
        let conn_result = timeout(REDIS_TIMEOUT, self.pool.get()).await;
        if let Ok(Ok(mut conn)) = conn_result {
            let cmd_result = timeout(REDIS_TIMEOUT, conn.get::<_, Option<String>>(key)).await;
            if let Ok(Ok(val)) = cmd_result {
                return val;
            }
        }
        None
    }

    pub async fn set(&self, key: &str, value: &str, ttl: u64) {
        let conn_result = timeout(REDIS_TIMEOUT, self.pool.get()).await;
        if let Ok(Ok(mut conn)) = conn_result {
            if ttl > 0 {
                let _: Result<(), _> = timeout(REDIS_TIMEOUT, conn.set_ex(key, value, ttl))
                    .await
                    .unwrap_or(Ok(()));
            } else {
                let _: Result<(), _> = timeout(REDIS_TIMEOUT, conn.set(key, value))
                    .await
                    .unwrap_or(Ok(()));
            }
        }
    }

    pub async fn delete(&self, key: &str) {
        let conn_result = timeout(REDIS_TIMEOUT, self.pool.get()).await;
        if let Ok(Ok(mut conn)) = conn_result {
            let _: Result<(), _> = timeout(REDIS_TIMEOUT, conn.del(key))
                .await
                .unwrap_or(Ok(()));
        }
    }

    pub async fn hincrby(
        &self,
        key: &str,
        field: &str,
        delta: i64,
    ) -> Result<i64, redis::RedisError> {
        let conn_result = timeout(REDIS_TIMEOUT, self.pool.get()).await.map_err(|_| {
            redis::RedisError::from((redis::ErrorKind::IoError, "Redis connection timeout"))
        })?;

        let mut conn = conn_result.map_err(|e| {
            redis::RedisError::from((
                redis::ErrorKind::IoError,
                "Redis pool exhaustion",
                e.to_string(),
            ))
        })?;

        timeout(REDIS_TIMEOUT, conn.hincr(key, field, delta))
            .await
            .map_err(|_| {
                redis::RedisError::from((redis::ErrorKind::IoError, "Redis command timeout"))
            })?
    }

    pub async fn hgetall(&self, key: &str) -> Result<HashMap<String, String>, redis::RedisError> {
        let conn_result = timeout(REDIS_TIMEOUT, self.pool.get()).await.map_err(|_| {
            redis::RedisError::from((redis::ErrorKind::IoError, "Redis connection timeout"))
        })?;

        let mut conn = conn_result.map_err(|e| {
            redis::RedisError::from((
                redis::ErrorKind::IoError,
                "Redis pool exhaustion",
                e.to_string(),
            ))
        })?;

        timeout(REDIS_TIMEOUT, conn.hgetall(key))
            .await
            .map_err(|_| {
                redis::RedisError::from((redis::ErrorKind::IoError, "Redis command timeout"))
            })?
    }

    pub async fn expire(&self, key: &str, ttl: u64) {
        let conn_result = timeout(REDIS_TIMEOUT, self.pool.get()).await;
        if let Ok(Ok(mut conn)) = conn_result {
            let _: Result<(), redis::RedisError> =
                timeout(REDIS_TIMEOUT, conn.expire(key, ttl as i64))
                    .await
                    .unwrap_or(Ok(()));
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
        let conn_result = timeout(REDIS_TIMEOUT, self.pool.get()).await.map_err(|_| {
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
            REDIS_TIMEOUT,
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

        let (allowed, retry_after_seconds, remaining) = cmd_result?;

        Ok(RateLimitDecision {
            allowed: allowed != 0,
            retry_after_seconds: retry_after_seconds.max(0) as u64,
            remaining: remaining.max(0) as u32,
        })
    }
}

pub struct CacheManager {
    local: LocalCache,
    redis: Option<Arc<RedisCache>>,
    use_redis: bool,
    rate_limit_local: Arc<parking_lot::Mutex<HashMap<String, LocalRateLimitState>>>,
}

impl CacheManager {
    pub fn new(config: CacheConfig) -> Self {
        Self {
            local: LocalCache::new(&config),
            redis: None,
            use_redis: false,
            rate_limit_local: Arc::new(parking_lot::Mutex::new(HashMap::new())),
        }
    }

    pub async fn with_redis(
        conn_str: &str,
        config: CacheConfig,
    ) -> Result<Self, redis::RedisError> {
        match RedisCache::new(conn_str).await {
            Ok(redis_cache) => Ok(Self {
                local: LocalCache::new(&config),
                redis: Some(Arc::new(redis_cache)),
                use_redis: true,
                rate_limit_local: Arc::new(parking_lot::Mutex::new(HashMap::new())),
            }),
            Err(e) => {
                tracing::warn!("Failed to connect to Redis: {}, using local cache only", e);
                Ok(Self {
                    local: LocalCache::new(&config),
                    redis: None,
                    use_redis: false,
                    rate_limit_local: Arc::new(parking_lot::Mutex::new(HashMap::new())),
                })
            }
        }
    }

    pub async fn get_token(&self, token: &str) -> Option<Claims> {
        if self.use_redis {
            if let Some(redis) = &self.redis {
                if let Some(val) = redis.get(token).await {
                    if let Ok(claims) = serde_json::from_str(&val) {
                        return Some(claims);
                    }
                }
            }
        }
        self.local.get(token)
    }

    pub async fn set_token(&self, token: &str, claims: &Claims, ttl: u64) {
        self.local.set(token, claims);
        if self.use_redis {
            if let Some(redis) = &self.redis {
                if let Ok(val) = serde_json::to_string(claims) {
                    redis.set(token, &val, ttl).await;
                }
            }
        }
    }

    pub async fn delete_token(&self, token: &str) {
        self.local.remove(token);
        if let Some(redis) = &self.redis {
            redis.delete(token).await;
        }
    }

    pub async fn delete(&self, key: &str) {
        self.local.remove(key);
        if let Some(redis) = &self.redis {
            redis.delete(key).await;
        }
    }

    pub async fn get<T: for<'de> Deserialize<'de>>(
        &self,
        key: &str,
    ) -> Result<Option<T>, ApiError> {
        let key = key.to_string();
        if self.use_redis {
            if let Some(redis) = &self.redis {
                if let Some(val) = redis.get(&key).await {
                    if let Ok(result) = serde_json::from_str(&val) {
                        return Ok(Some(result));
                    }
                }
            }
        }
        Ok(self
            .local
            .get_raw(&key)
            .and_then(|s| serde_json::from_str(&s).ok()))
    }

    pub async fn set<T: Serialize>(&self, key: &str, value: T, ttl: u64) -> Result<(), ApiError> {
        if let Ok(val) = serde_json::to_string(&value) {
            self.local.set_raw(key, &val);
            if self.use_redis {
                if let Some(redis) = &self.redis {
                    redis.set(key, &val, ttl).await;
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

#[derive(Clone, Copy)]
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

    #[test]
    fn test_cache_manager_set_and_get() {
        let rt = tokio::runtime::Runtime::new().unwrap();
        rt.block_on(async {
            let config = CacheConfig::default();
            let manager = CacheManager::new(config);

            let test_value = "test_value".to_string();
            let _ = manager.set("test_key", &test_value, 60).await;

            let result: Option<String> = manager.get::<String>("test_key").await.unwrap();
            assert_eq!(result, Some(test_value));
        });
    }

    #[test]
    fn test_cache_manager_delete() {
        let rt = tokio::runtime::Runtime::new().unwrap();
        rt.block_on(async {
            let config = CacheConfig::default();
            let manager = CacheManager::new(config);

            let test_value = "test_value".to_string();
            let _ = manager.set("test_key", &test_value, 60).await;
            assert!(manager.get::<String>("test_key").await.unwrap().is_some());

            let _ = manager.delete("test_key").await;
            assert!(manager.get::<String>("test_key").await.unwrap().is_none());
        });
    }

    #[test]
    fn test_cache_manager_get_nonexistent() {
        let rt = tokio::runtime::Runtime::new().unwrap();
        rt.block_on(async {
            let config = CacheConfig::default();
            let manager = CacheManager::new(config);

            let result: Option<String> = manager.get::<String>("nonexistent").await.unwrap();
            assert!(result.is_none());
        });
    }

    #[test]
    fn test_cache_manager_token_operations() {
        let rt = tokio::runtime::Runtime::new().unwrap();
        rt.block_on(async {
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
        });
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
