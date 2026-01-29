use crate::auth::Claims;
use crate::common::ApiError;
use moka::sync::Cache;
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use tokio::sync::Mutex;

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
    client: Arc<Mutex<redis::Client>>,
}

impl RedisCache {
    pub async fn new(conn_str: &str) -> Result<Self, redis::RedisError> {
        let client = redis::Client::open(conn_str)?;
        Ok(Self {
            client: Arc::new(Mutex::new(client)),
        })
    }

    pub async fn get(&self, key: &str) -> Option<String> {
        let client = self.client.lock().await;
        if let Ok(mut conn) = client.get_multiplexed_async_connection().await {
            if let Ok(val) = redis::cmd("GET")
                .arg(key)
                .query_async::<String>(&mut conn)
                .await
            {
                return Some(val);
            }
        }
        None
    }

    pub async fn set(&self, key: &str, value: &str, ttl: u64) {
        let client = self.client.lock().await;
        if let Ok(mut conn) = client.get_multiplexed_async_connection().await {
            if ttl > 0 {
                redis::cmd("SETEX")
                    .arg(key)
                    .arg(ttl as i64)
                    .arg(value)
                    .query_async::<()>(&mut conn)
                    .await
                    .ok();
            } else {
                redis::cmd("SET")
                    .arg(key)
                    .arg(value)
                    .query_async::<()>(&mut conn)
                    .await
                    .ok();
            }
        }
    }

    pub async fn delete(&self, key: &str) {
        let client = self.client.lock().await;
        if let Ok(mut conn) = client.get_multiplexed_async_connection().await {
            redis::cmd("DEL")
                .arg(key)
                .query_async::<()>(&mut conn)
                .await
                .ok();
        }
    }
}

pub struct CacheManager {
    local: LocalCache,
    redis: Option<Arc<RedisCache>>,
    use_redis: bool,
}

impl CacheManager {
    pub fn new(config: CacheConfig) -> Self {
        Self {
            local: LocalCache::new(&config),
            redis: None,
            use_redis: false,
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
            }),
            Err(e) => {
                tracing::warn!("Failed to connect to Redis: {}, using local cache only", e);
                Ok(Self {
                    local: LocalCache::new(&config),
                    redis: None,
                    use_redis: false,
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

    pub async fn get<T: for<'de> Deserialize<'de>>(&self, key: &str) -> Result<Option<T>, ApiError> {
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
        Ok(self.local
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
            manager.set("test_key", &test_value, 60).await;

            let result: Option<String> = manager.get("test_key").await;
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
            manager.set("test_key", &test_value, 60).await;
            assert!(manager.get::<String>("test_key").await.is_some());

            manager.delete("test_key").await;
            assert!(manager.get::<String>("test_key").await.is_none());
        });
    }

    #[test]
    fn test_cache_manager_get_nonexistent() {
        let rt = tokio::runtime::Runtime::new().unwrap();
        rt.block_on(async {
            let config = CacheConfig::default();
            let manager = CacheManager::new(config);

            let result: Option<String> = manager.get("nonexistent").await;
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
