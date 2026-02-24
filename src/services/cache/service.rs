use super::models::*;
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::RwLock;

pub struct CacheService {
    memory_cache: Arc<RwLock<MemoryCache>>,
    pub config: CacheConfig,
    stats: Arc<RwLock<CacheStats>>,
}

struct MemoryCache {
    entries: HashMap<String, CacheEntryInner>,
    lru_order: Vec<String>,
    max_entries: usize,
}

#[allow(dead_code)]
struct CacheEntryInner {
    value: Vec<u8>,
    created_at: i64,
    expires_at: Option<i64>,
    hit_count: u64,
}

impl CacheService {
    pub fn new(config: CacheConfig) -> Self {
        let max_entries = config.max_entries.unwrap_or(10000);
        
        Self {
            memory_cache: Arc::new(RwLock::new(MemoryCache {
                entries: HashMap::new(),
                lru_order: Vec::new(),
                max_entries,
            })),
            config,
            stats: Arc::new(RwLock::new(CacheStats::new())),
        }
    }

    pub async fn get(&self, key: &CacheKey) -> Option<Vec<u8>> {
        let mut cache = self.memory_cache.write().await;
        let full_key = key.to_string();

        let entry_value = if let Some(entry) = cache.entries.get_mut(&full_key) {
            if let Some(expires_at) = entry.expires_at {
                if expires_at < chrono::Utc::now().timestamp_millis() {
                    cache.entries.remove(&full_key);
                    if let Some(pos) = cache.lru_order.iter().position(|k| k == &full_key) {
                        cache.lru_order.remove(pos);
                    }
                    
                    drop(cache);
                    let mut stats = self.stats.write().await;
                    stats.record_miss();
                    return None;
                }
            }

            entry.hit_count += 1;
            Some(entry.value.clone())
        } else {
            None
        };

        if entry_value.is_some() {
            if let Some(pos) = cache.lru_order.iter().position(|k| k == &full_key) {
                cache.lru_order.remove(pos);
                cache.lru_order.push(full_key.clone());
            }
            
            drop(cache);
            let mut stats = self.stats.write().await;
            stats.record_hit();
            
            entry_value
        } else {
            drop(cache);
            let mut stats = self.stats.write().await;
            stats.record_miss();
            
            None
        }
    }

    pub async fn set(&self, key: &CacheKey, value: Vec<u8>, ttl_seconds: Option<u64>) {
        let mut cache = self.memory_cache.write().await;
        let full_key = key.to_string();
        let now = chrono::Utc::now().timestamp_millis();
        
        let expires_at = ttl_seconds.map(|ttl| now + (ttl as i64 * 1000));

        if cache.entries.len() >= cache.max_entries && !cache.entries.contains_key(&full_key) {
            self.evict_lru(&mut cache).await;
        }

        let entry_size = if let Some(entry) = cache.entries.get_mut(&full_key) {
            entry.value = value;
            entry.expires_at = expires_at;
            entry.hit_count += 1;
            cache.entries.len()
        } else {
            let entry = CacheEntryInner {
                value,
                created_at: now,
                expires_at,
                hit_count: 1,
            };
            cache.entries.insert(full_key.clone(), entry);
            cache.lru_order.push(full_key);
            cache.entries.len()
        };

        drop(cache);
        let mut stats = self.stats.write().await;
        stats.size = entry_size;
    }

    pub async fn delete(&self, key: &CacheKey) -> bool {
        let mut cache = self.memory_cache.write().await;
        let full_key = key.to_string();
        
        if let Some(pos) = cache.lru_order.iter().position(|k| k == &full_key) {
            cache.lru_order.remove(pos);
        }
        
        let removed = cache.entries.remove(&full_key).is_some();
        
        if removed {
            let mut stats = self.stats.write().await;
            stats.size = cache.entries.len();
        }
        
        removed
    }

    pub async fn clear_namespace(&self, namespace: &str) {
        let mut cache = self.memory_cache.write().await;
        
        let keys_to_remove: Vec<String> = cache.entries.keys()
            .filter(|k| k.starts_with(&format!("{}:", namespace)))
            .cloned()
            .collect();
        
        for key in &keys_to_remove {
            cache.entries.remove(key);
            if let Some(pos) = cache.lru_order.iter().position(|k| k == key) {
                cache.lru_order.remove(pos);
            }
        }

        let mut stats = self.stats.write().await;
        stats.size = cache.entries.len();
    }

    pub async fn invalidate_pattern(&self, pattern: &str) {
        let mut cache = self.memory_cache.write().await;
        
        let regex = match pattern_to_regex(pattern) {
            Some(r) => r,
            None => return,
        };
        let keys_to_remove: Vec<String> = cache.entries.keys()
            .filter(|k| regex.is_match(k))
            .cloned()
            .collect();
        
        for key in &keys_to_remove {
            cache.entries.remove(key);
            if let Some(pos) = cache.lru_order.iter().position(|k| k == key) {
                cache.lru_order.remove(pos);
            }
        }
        
        let mut stats = self.stats.write().await;
        stats.size = cache.entries.len();
        stats.evictions += keys_to_remove.len() as u64;
    }

    pub async fn get_stats(&self) -> CacheStats {
        let cache = self.memory_cache.write().await;
        let mut stats = self.stats.write().await;
        stats.size = cache.entries.len();
        stats.clone()
    }

    pub async fn size(&self) -> usize {
        let cache = self.memory_cache.read().await;
        cache.entries.len()
    }

    async fn evict_lru(&self, cache: &mut MemoryCache) {
        if let Some(lru_key) = cache.lru_order.first() {
            cache.entries.remove(lru_key);
            cache.lru_order.remove(0);
            
            let mut stats = self.stats.write().await;
            stats.record_eviction();
        }
    }

    pub async fn preload(&self, keys: Vec<CacheKey>) {
        for key in keys {
            let full_key = key.to_string();
            
            let cache = self.memory_cache.write().await;
            if !cache.entries.contains_key(&full_key) {
                drop(cache);
            }
        }
    }

    pub async fn warmup(&self) {
        tracing::info!("Cache warmup started");
        
        let mut stats = self.stats.write().await;
        stats.hits = 0;
        stats.misses = 0;
        stats.evictions = 0;
        stats.hit_rate = 0.0;
        
        tracing::info!("Cache warmup completed");
    }
}

fn pattern_to_regex(pattern: &str) -> Option<regex::Regex> {
    let escaped = pattern
        .replace('*', ".*")
        .replace('?', ".");
    
    regex::Regex::new(&format!("^{}$", escaped)).ok()
}

impl Default for CacheService {
    fn default() -> Self {
        Self::new(CacheConfig::default())
    }
}
