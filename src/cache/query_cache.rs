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
    pub access_count: u32,
    pub last_accessed: Instant,
}

impl<T> CacheEntry<T> {
    pub fn new(value: T, ttl: Duration) -> Self {
        let now = Instant::now();
        Self {
            value,
            created_at: now,
            ttl,
            access_count: 1,
            last_accessed: now,
        }
    }

    pub fn is_expired(&self) -> bool {
        self.created_at.elapsed() > self.ttl
    }

    pub fn touch(&mut self) {
        self.access_count += 1;
        self.last_accessed = Instant::now();
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
    pub max_memory_mb: usize,
    pub eviction_threshold: f64,
    pub warm_on_startup: bool,
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
            max_memory_mb: 100,
            eviction_threshold: 0.9,
            warm_on_startup: false,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CacheStats {
    pub hits: u64,
    pub misses: u64,
    pub evictions: u64,
    pub total_entries: usize,
    pub memory_usage_bytes: u64,
    pub hit_rate: f64,
}

impl Default for CacheStats {
    fn default() -> Self {
        Self {
            hits: 0,
            misses: 0,
            evictions: 0,
            total_entries: 0,
            memory_usage_bytes: 0,
            hit_rate: 0.0,
        }
    }
}

#[derive(Debug, Clone)]
pub enum CacheWarmupStrategy {
    Lazy,
    Eager,
    Scheduled,
}

#[derive(Debug, Clone)]
pub struct CacheWarmupConfig {
    pub strategy: CacheWarmupStrategy,
    pub rooms: Vec<String>,
    pub users: Vec<String>,
    pub interval: Duration,
    pub batch_size: usize,
}

impl Default for CacheWarmupConfig {
    fn default() -> Self {
        Self {
            strategy: CacheWarmupStrategy::Lazy,
            rooms: Vec::new(),
            users: Vec::new(),
            interval: Duration::from_secs(300),
            batch_size: 100,
        }
    }
}

pub struct QueryCache {
    config: CacheConfig,
    warmup_config: RwLock<CacheWarmupConfig>,
    rooms: RwLock<HashMap<String, CacheEntry<serde_json::Value>>>,
    users: RwLock<HashMap<String, CacheEntry<serde_json::Value>>>,
    events: RwLock<HashMap<String, CacheEntry<serde_json::Value>>>,
    memberships: RwLock<HashMap<String, CacheEntry<serde_json::Value>>>,
    devices: RwLock<HashMap<String, CacheEntry<serde_json::Value>>>,
    tokens: RwLock<HashMap<String, CacheEntry<serde_json::Value>>>,
    stats: RwLock<CacheStats>,
    hot_keys: RwLock<HashMap<String, u32>>,
}

impl QueryCache {
    pub fn new(config: CacheConfig) -> Self {
        Self {
            config,
            warmup_config: RwLock::new(CacheWarmupConfig::default()),
            rooms: RwLock::new(HashMap::new()),
            users: RwLock::new(HashMap::new()),
            events: RwLock::new(HashMap::new()),
            memberships: RwLock::new(HashMap::new()),
            devices: RwLock::new(HashMap::new()),
            tokens: RwLock::new(HashMap::new()),
            stats: RwLock::new(CacheStats::default()),
            hot_keys: RwLock::new(HashMap::new()),
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
        self.get(&self.rooms, room_id, "room", true).await
    }

    pub async fn set_room(&self, room_id: &str, value: serde_json::Value) {
        self.set(&self.rooms, room_id, value, self.config.room_ttl)
            .await
    }

    pub async fn get_user(&self, user_id: &str) -> Option<serde_json::Value> {
        self.get(&self.users, user_id, "user", true).await
    }

    pub async fn set_user(&self, user_id: &str, value: serde_json::Value) {
        self.set(&self.users, user_id, value, self.config.user_ttl)
            .await
    }

    pub async fn get_event(&self, event_id: &str) -> Option<serde_json::Value> {
        self.get(&self.events, event_id, "event", false).await
    }

    pub async fn set_event(&self, event_id: &str, value: serde_json::Value) {
        self.set(&self.events, event_id, value, self.config.event_ttl)
            .await
    }

    pub async fn get_membership(&self, key: &str) -> Option<serde_json::Value> {
        self.get(&self.memberships, key, "membership", false).await
    }

    pub async fn set_membership(&self, key: &str, value: serde_json::Value) {
        self.set(&self.memberships, key, value, self.config.membership_ttl)
            .await
    }

    pub async fn get_device(&self, key: &str) -> Option<serde_json::Value> {
        self.get(&self.devices, key, "device", false).await
    }

    pub async fn set_device(&self, key: &str, value: serde_json::Value) {
        self.set(&self.devices, key, value, self.config.device_ttl)
            .await
    }

    pub async fn get_token(&self, token: &str) -> Option<serde_json::Value> {
        self.get(&self.tokens, token, "token", true).await
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
        track_access: bool,
    ) -> Option<T>
    where
        T: Clone,
    {
        let mut cache = cache.write().await;
        let cache_len = cache.len();
        if let Some(entry) = cache.get_mut(key) {
            if !entry.is_expired() {
                entry.touch();
                let mut stats = self.stats.write().await;
                stats.hits += 1;
                stats.total_entries = cache_len;
                stats.hit_rate = self.calculate_hit_rate_direct(&stats);
                if track_access {
                    let mut hot_keys = self.hot_keys.write().await;
                    *hot_keys.entry(key.to_string()).or_insert(0) += 1;
                }
                return Some(entry.value.clone());
            } else {
                cache.remove(key);
                let mut stats = self.stats.write().await;
                stats.evictions += 1;
            }
        } else {
            let mut stats = self.stats.write().await;
            stats.misses += 1;
            stats.total_entries = cache_len;
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
        let mut cache = cache.write().await;
        let entry = CacheEntry::new(value, ttl);

        if cache.len() >= self.config.max_entries {
            self.evict_lru(&mut cache).await;
        }

        cache.insert(key.to_string(), entry);
        drop(cache);
        self.update_total_entries().await;
    }

    async fn evict_lru<T>(&self, cache: &mut HashMap<String, CacheEntry<T>>) {
        if cache.is_empty() {
            return;
        }

        let mut entries: Vec<_> = cache.iter_mut().collect();
        entries.sort_by_key(|(_, entry)| entry.last_accessed);

        let to_remove = (entries.len() / 10).min(100);
        let keys_to_remove: Vec<String> = entries
            .into_iter()
            .take(to_remove)
            .map(|(k, _)| k.clone())
            .collect();

        for key in keys_to_remove {
            cache.remove(&key);
        }

        let mut stats = self.stats.write().await;
        stats.evictions += to_remove as u64;
    }

    fn calculate_hit_rate_direct(&self, stats: &CacheStats) -> f64 {
        let total = stats.hits + stats.misses;
        if total == 0 {
            0.0
        } else {
            stats.hits as f64 / total as f64
        }
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
        self.memberships
            .write()
            .await
            .retain(|k, _| !k.starts_with(room_id));
    }

    pub async fn invalidate_user(&self, user_id: &str) {
        self.users.write().await.remove(user_id);
        self.devices
            .write()
            .await
            .retain(|k, _| !k.starts_with(user_id));
        self.tokens
            .write()
            .await
            .retain(|k, _| !k.contains(user_id));
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
        self.hot_keys.write().await.clear();

        let mut stats = self.stats.write().await;
        stats.total_entries = 0;
    }

    pub async fn get_stats(&self) -> CacheStats {
        self.stats.read().await.clone()
    }

    pub async fn get_hot_keys(&self, limit: usize) -> Vec<(String, u32)> {
        let hot_keys = self.hot_keys.read().await;
        let mut entries: Vec<_> = hot_keys.iter().collect();
        entries.sort_by_key(|(_, &count)| std::cmp::Reverse(count));
        entries
            .into_iter()
            .take(limit)
            .map(|(k, v)| (k.clone(), *v))
            .collect()
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
        let before_count = map.len();
        map.retain(|_, entry| !entry.is_expired());
        let removed = before_count.saturating_sub(map.len());

        let mut stats = self.stats.write().await;
        stats.evictions += removed as u64;
    }

    pub async fn configure_warmup(&self, config: CacheWarmupConfig) {
        let mut warmup_config = self.warmup_config.write().await;
        *warmup_config = config;
    }

    pub async fn get_warmup_config(&self) -> CacheWarmupConfig {
        self.warmup_config.read().await.clone()
    }

    pub async fn warmup_batch(&self, items: Vec<(String, serde_json::Value)>, cache_type: &str) {
        let batch_size = self.warmup_config.read().await.batch_size;

        for chunk in items.chunks(batch_size) {
            match cache_type {
                "room" => {
                    for (key, value) in chunk {
                        self.set_room(key, value.clone()).await;
                    }
                }
                "user" => {
                    for (key, value) in chunk {
                        self.set_user(key, value.clone()).await;
                    }
                }
                _ => {}
            }
        }
    }

    pub async fn peek<T>(
        &self,
        cache: &RwLock<HashMap<String, CacheEntry<T>>>,
        key: &str,
    ) -> Option<T>
    where
        T: Clone,
    {
        let cache = cache.read().await;
        cache.get(key).map(|entry| entry.value.clone())
    }

    pub async fn get_if_fresh<T>(
        &self,
        cache: &RwLock<HashMap<String, CacheEntry<T>>>,
        key: &str,
    ) -> Option<T>
    where
        T: Clone,
    {
        let cache = cache.read().await;
        if let Some(entry) = cache.get(key) {
            if !entry.is_expired() {
                return Some(entry.value.clone());
            }
        }
        None
    }

    pub async fn set_with_hint<T>(
        &self,
        cache: &RwLock<HashMap<String, CacheEntry<T>>>,
        key: &str,
        value: T,
        ttl: Duration,
        is_hot: bool,
    ) where
        T: Clone,
    {
        let mut cache = cache.write().await;
        let entry = CacheEntry::new(value, ttl);

        if is_hot {
            let mut hot_keys = self.hot_keys.write().await;
            *hot_keys.entry(key.to_string()).or_insert(0) += 1000;
        }

        if cache.len() >= self.config.max_entries {
            self.evict_lru(&mut cache).await;
        }

        cache.insert(key.to_string(), entry);
        drop(cache);
        self.update_total_entries().await;
    }

    pub async fn get_multi_rooms(
        &self,
        room_ids: &[String],
    ) -> HashMap<String, Option<serde_json::Value>> {
        let cache = self.rooms.read().await;
        let mut results = HashMap::new();

        for room_id in room_ids {
            if let Some(entry) = cache.get(room_id) {
                if entry.created_at.elapsed() <= self.config.room_ttl {
                    let mut stats = self.stats.write().await;
                    stats.hits += 1;
                    results.insert(room_id.clone(), Some(entry.value.clone()));
                } else {
                    results.insert(room_id.clone(), None);
                }
            } else {
                let mut stats = self.stats.write().await;
                stats.misses += 1;
                results.insert(room_id.clone(), None);
            }
        }

        results
    }

    pub async fn get_multi_users(
        &self,
        user_ids: &[String],
    ) -> HashMap<String, Option<serde_json::Value>> {
        let cache = self.users.read().await;
        let mut results = HashMap::new();

        for user_id in user_ids {
            if let Some(entry) = cache.get(user_id) {
                if entry.created_at.elapsed() <= self.config.user_ttl {
                    let mut stats = self.stats.write().await;
                    stats.hits += 1;
                    results.insert(user_id.clone(), Some(entry.value.clone()));
                } else {
                    results.insert(user_id.clone(), None);
                }
            } else {
                let mut stats = self.stats.write().await;
                stats.misses += 1;
                results.insert(user_id.clone(), None);
            }
        }

        results
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
            max_memory_mb: 100,
            eviction_threshold: 0.9,
            warm_on_startup: false,
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

    #[tokio::test]
    async fn test_get_multi_rooms() {
        let cache = QueryCache::default();

        let value1 = serde_json::json!({"room": "1"});
        let value2 = serde_json::json!({"room": "2"});

        cache.set_room("!room1:server", value1.clone()).await;
        cache.set_room("!room2:server", value2.clone()).await;

        let room_ids = vec![
            "!room1:server".to_string(),
            "!room2:server".to_string(),
            "!room3:server".to_string(),
        ];
        let results = cache.get_multi_rooms(&room_ids).await;

        assert_eq!(results[&"!room1:server".to_string()], Some(value1));
        assert_eq!(results[&"!room2:server".to_string()], Some(value2));
        assert_eq!(results[&"!room3:server".to_string()], None);
    }
}
