use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum CacheBackend {
    Memory,
    Redis,
    Memcached,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CacheConfig {
    pub backend: CacheBackend,
    pub redis_url: Option<String>,
    pub memcached_url: Option<String>,
    pub default_ttl_seconds: u64,
    pub max_entries: Option<usize>,
    pub enable_stats: bool,
}

impl Default for CacheConfig {
    fn default() -> Self {
        Self {
            backend: CacheBackend::Memory,
            redis_url: None,
            memcached_url: None,
            default_ttl_seconds: 3600,
            max_entries: Some(10000),
            enable_stats: true,
        }
    }
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
pub enum CacheStrategy {
    Lru,
    Lfu,
    Ttl,
    WriteThrough,
    WriteBehind,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CacheEntry<T> {
    pub key: String,
    pub value: T,
    pub created_at: i64,
    pub expires_at: Option<i64>,
    pub hit_count: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CacheStats {
    pub hits: u64,
    pub misses: u64,
    pub evictions: u64,
    pub size: usize,
    pub hit_rate: f64,
}

impl CacheStats {
    pub fn new() -> Self {
        Self {
            hits: 0,
            misses: 0,
            evictions: 0,
            size: 0,
            hit_rate: 0.0,
        }
    }

    pub fn record_hit(&mut self) {
        self.hits += 1;
        self.update_hit_rate();
    }

    pub fn record_miss(&mut self) {
        self.misses += 1;
        self.update_hit_rate();
    }

    pub fn record_eviction(&mut self) {
        self.evictions += 1;
    }

    fn update_hit_rate(&mut self) {
        let total = self.hits + self.misses;
        if total > 0 {
            self.hit_rate = self.hits as f64 / total as f64;
        }
    }
}

impl Default for CacheStats {
    fn default() -> Self {
        Self::new()
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CacheKey {
    pub namespace: String,
    pub key: String,
}

impl CacheKey {
    pub fn new(namespace: &str, key: &str) -> Self {
        Self {
            namespace: namespace.to_string(),
            key: key.to_string(),
        }
    }

    pub fn to_string(&self) -> String {
        format!("{}:{}", self.namespace, self.key)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InvalidationPattern {
    pub pattern: String,
    pub namespace: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PreloadRequest {
    pub keys: Vec<CacheKey>,
    pub ttl_seconds: Option<u64>,
}
