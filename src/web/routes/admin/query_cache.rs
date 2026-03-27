use std::collections::HashMap;
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::sync::RwLock;

#[derive(Debug, Clone)]
pub struct CacheEntry<T> {
    pub value: T,
    pub created_at: Instant,
    pub ttl: Duration,
}

impl<T> CacheEntry<T> {
    pub fn is_expired(&self) -> bool {
        self.created_at.elapsed() > self.ttl
    }
}

#[derive(Clone)]
pub struct QueryCache {
    cache: Arc<RwLock<HashMap<String, CacheEntry<serde_json::Value>>>>,
    default_ttl: Duration,
}

impl QueryCache {
    pub fn new(default_ttl_secs: u64) -> Self {
        Self {
            cache: Arc::new(RwLock::new(HashMap::new())),
            default_ttl: Duration::from_secs(default_ttl_secs),
        }
    }

    pub async fn get(&self, key: &str) -> Option<serde_json::Value> {
        let cache = self.cache.read().await;
        if let Some(entry) = cache.get(key) {
            if !entry.is_expired() {
                tracing::debug!("Cache hit for key: {}", key);
                return Some(entry.value.clone());
            }
        }
        tracing::debug!("Cache miss for key: {}", key);
        None
    }

    pub async fn set(&self, key: String, value: serde_json::Value) {
        let entry = CacheEntry {
            value,
            created_at: Instant::now(),
            ttl: self.default_ttl,
        };
        let mut cache = self.cache.write().await;
        cache.insert(key, entry);
    }

    pub async fn set_with_ttl(&self, key: String, value: serde_json::Value, ttl: Duration) {
        let entry = CacheEntry {
            value,
            created_at: Instant::now(),
            ttl,
        };
        let mut cache = self.cache.write().await;
        cache.insert(key, entry);
    }

    pub async fn invalidate(&self, key: &str) {
        let mut cache = self.cache.write().await;
        cache.remove(key);
        tracing::debug!("Cache invalidated for key: {}", key);
    }

    pub async fn invalidate_pattern(&self, pattern: &str) {
        let mut cache = self.cache.write().await;
        let keys_to_remove: Vec<String> = cache
            .keys()
            .filter(|k| k.contains(pattern))
            .cloned()
            .collect();
        for key in keys_to_remove {
            cache.remove(&key);
        }
        tracing::debug!("Cache invalidated for pattern: {}", pattern);
    }

    pub async fn clear(&self) {
        let mut cache = self.cache.write().await;
        cache.clear();
        tracing::info!("Cache cleared");
    }

    pub async fn stats(&self) -> CacheStats {
        let cache = self.cache.read().await;
        let total = cache.len();
        let expired = cache.values().filter(|e| e.is_expired()).count();
        CacheStats {
            total_entries: total,
            expired_entries: expired,
            active_entries: total - expired,
        }
    }
}

#[derive(Debug, Clone)]
pub struct CacheStats {
    pub total_entries: usize,
    pub expired_entries: usize,
    pub active_entries: usize,
}

impl Default for QueryCache {
    fn default() -> Self {
        Self::new(300)
    }
}
