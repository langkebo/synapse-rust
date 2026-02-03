use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::time::Duration;
use tokio::sync::RwLock;
use tokio::time::Instant;

#[derive(Debug, Clone)]
pub struct CacheEntry<T> {
    pub value: T,
    pub created_at: Instant,
    pub ttl: Duration,
}

impl<T> CacheEntry<T> {
    pub fn new(value: T, ttl: Duration) -> Self {
        Self {
            value,
            created_at: Instant::now(),
            ttl,
        }
    }

    pub fn is_expired(&self) -> bool {
        self.created_at.elapsed() > self.ttl
    }
}

#[derive(Debug, Clone)]
pub struct CacheConfig {
    pub room_ttl: Duration,
    pub user_ttl: Duration,
    pub event_ttl: Duration,
    pub membership_ttl: Duration,
    pub device_ttl: Duration,
    pub token_ttl: Duration,
    pub max_entries: usize,
}

impl Default for CacheConfig {
    fn default() -> Self {
        Self {
            room_ttl: Duration::from_secs(300),
            user_ttl: Duration::from_secs(600),
            event_ttl: Duration::from_secs(60),
            membership_ttl: Duration::from_secs(120),
            device_ttl: Duration::from_secs(300),
            token_ttl: Duration::from_secs(3600),
            max_entries: 10000,
        }
    }
}

pub struct QueryCache {
    config: CacheConfig,
    rooms: RwLock<HashMap<String, CacheEntry<serde_json::Value>>>,
    users: RwLock<HashMap<String, CacheEntry<serde_json::Value>>>,
    events: RwLock<HashMap<String, CacheEntry<serde_json::Value>>>,
    memberships: RwLock<HashMap<String, CacheEntry<serde_json::Value>>>,
    devices: RwLock<HashMap<String, CacheEntry<serde_json::Value>>>,
    tokens: RwLock<HashMap<String, CacheEntry<serde_json::Value>>>,
    stats: RwLock<CacheStats>,
}

#[derive(Debug, Default, Clone, Serialize, Deserialize)]
pub struct CacheStats {
    pub hits: u64,
    pub misses: u64,
    pub evictions: u64,
    pub total_entries: usize,
}

impl QueryCache {
    pub fn new(config: CacheConfig) -> Self {
        Self {
            config,
            rooms: RwLock::new(HashMap::new()),
            users: RwLock::new(HashMap::new()),
            events: RwLock::new(HashMap::new()),
            memberships: RwLock::new(HashMap::new()),
            devices: RwLock::new(HashMap::new()),
            tokens: RwLock::new(HashMap::new()),
            stats: RwLock::new(CacheStats::default()),
        }
    }
}

impl Default for QueryCache {
    fn default() -> Self {
        Self::new(CacheConfig::default())
    }
}

impl QueryCache {
    pub async fn get_room(&self, room_id: &str) -> Option<serde_json::Value> {
        self.get(&self.rooms, room_id, "room").await
    }

    pub async fn set_room(&self, room_id: &str, value: serde_json::Value) {
        self.set(&self.rooms, room_id, value, self.config.room_ttl)
            .await
    }

    pub async fn get_user(&self, user_id: &str) -> Option<serde_json::Value> {
        self.get(&self.users, user_id, "user").await
    }

    pub async fn set_user(&self, user_id: &str, value: serde_json::Value) {
        self.set(&self.users, user_id, value, self.config.user_ttl)
            .await
    }

    pub async fn get_event(&self, event_id: &str) -> Option<serde_json::Value> {
        self.get(&self.events, event_id, "event").await
    }

    pub async fn set_event(&self, event_id: &str, value: serde_json::Value) {
        self.set(&self.events, event_id, value, self.config.event_ttl)
            .await
    }

    pub async fn get_membership(&self, key: &str) -> Option<serde_json::Value> {
        self.get(&self.memberships, key, "membership").await
    }

    pub async fn set_membership(&self, key: &str, value: serde_json::Value) {
        self.set(&self.memberships, key, value, self.config.membership_ttl)
            .await
    }

    pub async fn get_device(&self, key: &str) -> Option<serde_json::Value> {
        self.get(&self.devices, key, "device").await
    }

    pub async fn set_device(&self, key: &str, value: serde_json::Value) {
        self.set(&self.devices, key, value, self.config.device_ttl)
            .await
    }

    pub async fn get_token(&self, token: &str) -> Option<serde_json::Value> {
        self.get(&self.tokens, token, "token").await
    }

    pub async fn set_token(&self, token: &str, value: serde_json::Value) {
        self.set(&self.tokens, token, value, self.config.token_ttl)
            .await
    }

    async fn get<T>(
        &self,
        cache: &RwLock<HashMap<String, CacheEntry<T>>>,
        key: &str,
        _cache_type: &str,
    ) -> Option<T>
    where
        T: Clone,
    {
        let cache = cache.read().await;
        if let Some(entry) = cache.get(key) {
            if !entry.is_expired() {
                let mut stats = self.stats.write().await;
                stats.hits += 1;
                return Some(entry.value.clone());
            } else {
                let mut stats = self.stats.write().await;
                stats.evictions += 1;
            }
        } else {
            let mut stats = self.stats.write().await;
            stats.misses += 1;
        }
        None
    }

    async fn set<T>(
        &self,
        cache: &RwLock<HashMap<String, CacheEntry<T>>>,
        key: &str,
        value: T,
        ttl: Duration,
    ) where
        T: Clone,
    {
        {
            let mut cache = cache.write().await;
            let entry = CacheEntry::new(value, ttl);

            if cache.len() >= self.config.max_entries {
                let mut stats = self.stats.write().await;
                stats.evictions += 1;
            }

            cache.insert(key.to_string(), entry);
        }
        self.update_total_entries().await;
    }

    async fn update_total_entries(&self) {
        let total = self.rooms.read().await.len()
            + self.users.read().await.len()
            + self.events.read().await.len()
            + self.memberships.read().await.len()
            + self.devices.read().await.len()
            + self.tokens.read().await.len();

        let mut stats = self.stats.write().await;
        stats.total_entries = total;
    }

    pub async fn invalidate_room(&self, room_id: &str) {
        self.rooms.write().await.remove(room_id);
    }

    pub async fn invalidate_user(&self, user_id: &str) {
        self.users.write().await.remove(user_id);
        self.devices
            .write()
            .await
            .retain(|k, _| !k.starts_with(user_id));
    }

    pub async fn invalidate_event(&self, event_id: &str) {
        self.events.write().await.remove(event_id);
    }

    pub async fn invalidate_membership(&self, key: &str) {
        self.memberships.write().await.remove(key);
    }

    pub async fn invalidate_device(&self, key: &str) {
        self.devices.write().await.remove(key);
    }

    pub async fn invalidate_token(&self, token: &str) {
        self.tokens.write().await.remove(token);
    }

    pub async fn clear(&self) {
        self.rooms.write().await.clear();
        self.users.write().await.clear();
        self.events.write().await.clear();
        self.memberships.write().await.clear();
        self.devices.write().await.clear();
        self.tokens.write().await.clear();

        let mut stats = self.stats.write().await;
        stats.total_entries = 0;
    }

    pub async fn get_stats(&self) -> CacheStats {
        self.stats.read().await.clone()
    }

    pub async fn cleanup_expired(&self) {
        let now = Instant::now();

        self.cleanup_map(&self.rooms, now).await;
        self.cleanup_map(&self.users, now).await;
        self.cleanup_map(&self.events, now).await;
        self.cleanup_map(&self.memberships, now).await;
        self.cleanup_map(&self.devices, now).await;
        self.cleanup_map(&self.tokens, now).await;

        self.update_total_entries().await;
    }

    async fn cleanup_map<T>(&self, cache: &RwLock<HashMap<String, CacheEntry<T>>>, _now: Instant) {
        let mut map = cache.write().await;
        map.retain(|_, entry| !entry.is_expired());
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test(start_paused = true)]
    async fn test_cache_entry_expiration() {
        let entry = CacheEntry::new("test_value", Duration::from_secs(1));
        assert!(!entry.is_expired());

        tokio::time::advance(Duration::from_millis(1100)).await;
        assert!(entry.is_expired());
    }

    #[tokio::test]
    async fn test_cache_set_get() {
        let cache = QueryCache::default();

        let value = serde_json::json!({"test": "data"});
        cache.set_room("!test:server", value.clone()).await;

        let retrieved = cache.get_room("!test:server").await;
        assert_eq!(retrieved, Some(value));
    }

    #[tokio::test]
    async fn test_cache_miss() {
        let cache = QueryCache::default();

        let retrieved = cache.get_room("!nonexistent:server").await;
        assert_eq!(retrieved, None);
    }

    #[tokio::test(start_paused = true)]
    async fn test_cache_expiration() {
        let cache = QueryCache::new(CacheConfig {
            room_ttl: Duration::from_millis(100),
            user_ttl: Duration::from_millis(100),
            event_ttl: Duration::from_millis(100),
            membership_ttl: Duration::from_millis(100),
            device_ttl: Duration::from_millis(100),
            token_ttl: Duration::from_millis(100),
            max_entries: 10000,
        });

        let value = serde_json::json!({"test": "data"});
        cache.set_room("!test:server", value.clone()).await;

        let retrieved = cache.get_room("!test:server").await;
        assert_eq!(retrieved, Some(value));

        tokio::time::advance(Duration::from_millis(150)).await;

        let retrieved = cache.get_room("!test:server").await;
        assert_eq!(retrieved, None);
    }

    #[tokio::test]
    async fn test_cache_invalidation() {
        let cache = QueryCache::default();

        let value = serde_json::json!({"test": "data"});
        cache.set_room("!test:server", value.clone()).await;

        let retrieved = cache.get_room("!test:server").await;
        assert_eq!(retrieved, Some(value));

        cache.invalidate_room("!test:server").await;

        let retrieved = cache.get_room("!test:server").await;
        assert_eq!(retrieved, None);
    }

    #[tokio::test]
    async fn test_cache_stats() {
        let cache = QueryCache::default();

        cache.get_room("!nonexistent:server").await;
        cache.get_user("@nonexistent:server").await;

        let value = serde_json::json!({"test": "data"});
        cache.set_room("!test:server", value.clone()).await;
        cache.get_room("!test:server").await;

        let stats = cache.get_stats().await;
        assert_eq!(stats.hits, 1);
        assert_eq!(stats.misses, 2);
        assert_eq!(stats.evictions, 0);
        assert_eq!(stats.total_entries, 1);
    }

    #[tokio::test]
    async fn test_cache_cleanup_expired() {
        let cache = QueryCache::default();

        let value = serde_json::json!({"test": "data"});
        cache.set_room("!test:server", value.clone()).await;

        let stats = cache.get_stats().await;
        assert_eq!(stats.total_entries, 1);

        cache.cleanup_expired().await;

        let stats = cache.get_stats().await;
        assert_eq!(stats.total_entries, 1);
    }
}
