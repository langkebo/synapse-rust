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
    pub created_ts: i64,
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

impl std::fmt::Display for CacheKey {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}:{}", self.namespace, self.key)
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_cache_config_default() {
        let config = CacheConfig::default();
        assert_eq!(config.backend, CacheBackend::Memory);
        assert_eq!(config.default_ttl_seconds, 3600);
        assert_eq!(config.max_entries, Some(10000));
        assert!(config.enable_stats);
    }

    #[test]
    fn test_cache_stats_new() {
        let stats = CacheStats::new();
        assert_eq!(stats.hits, 0);
        assert_eq!(stats.misses, 0);
        assert_eq!(stats.evictions, 0);
        assert_eq!(stats.size, 0);
        assert_eq!(stats.hit_rate, 0.0);
    }

    #[test]
    fn test_cache_stats_record_hit() {
        let mut stats = CacheStats::new();
        stats.record_hit();
        assert_eq!(stats.hits, 1);
        assert_eq!(stats.hit_rate, 1.0); // 1 hit, 0 misses = 100%
    }

    #[test]
    fn test_cache_stats_record_miss() {
        let mut stats = CacheStats::new();
        stats.record_miss();
        assert_eq!(stats.misses, 1);
        assert_eq!(stats.hit_rate, 0.0); // 0 hits, 1 miss = 0%
    }

    #[test]
    fn test_cache_stats_hit_rate_calculation() {
        let mut stats = CacheStats::new();
        stats.record_hit();
        stats.record_hit();
        stats.record_miss();
        
        assert_eq!(stats.hits, 2);
        assert_eq!(stats.misses, 1);
        assert_eq!(stats.hit_rate, 2.0 / 3.0);
    }

    #[test]
    fn test_cache_stats_record_eviction() {
        let mut stats = CacheStats::new();
        stats.record_eviction();
        stats.record_eviction();
        assert_eq!(stats.evictions, 2);
    }

    #[test]
    fn test_cache_key_display() {
        let key = CacheKey {
            namespace: "users".to_string(),
            key: "user123".to_string(),
        };
        assert_eq!(key.to_string(), "users:user123");
    }

    #[test]
    fn test_cache_backend_variants() {
        assert_eq!(CacheBackend::Memory, CacheBackend::Memory);
        assert_eq!(CacheBackend::Redis, CacheBackend::Redis);
        assert_eq!(CacheBackend::Memcached, CacheBackend::Memcached);
    }

    #[test]
    fn test_cache_strategy_variants() {
        assert_eq!(CacheStrategy::Lru, CacheStrategy::Lru);
        assert_eq!(CacheStrategy::Lfu, CacheStrategy::Lfu);
        assert_eq!(CacheStrategy::Ttl, CacheStrategy::Ttl);
        assert_eq!(CacheStrategy::WriteThrough, CacheStrategy::WriteThrough);
        assert_eq!(CacheStrategy::WriteBehind, CacheStrategy::WriteBehind);
    }

    #[test]
    fn test_invalidation_pattern() {
        let pattern = InvalidationPattern {
            pattern: "user:*".to_string(),
            namespace: Some("users".to_string()),
        };
        assert_eq!(pattern.pattern, "user:*");
        assert_eq!(pattern.namespace, Some("users".to_string()));
    }

    #[test]
    fn test_preload_request() {
        let request = PreloadRequest {
            keys: vec![
                CacheKey { namespace: "test".to_string(), key: "key1".to_string() },
                CacheKey { namespace: "test".to_string(), key: "key2".to_string() },
            ],
            ttl_seconds: Some(3600),
        };
        assert_eq!(request.keys.len(), 2);
        assert_eq!(request.ttl_seconds, Some(3600));
    }

    #[test]
    fn test_cache_entry() {
        let entry = CacheEntry {
            key: "test_key".to_string(),
            value: "test_value".to_string(),
            created_ts: 1234567890,
            expires_at: Some(1234567890 + 3600),
            hit_count: 5,
        };
        assert_eq!(entry.key, "test_key");
        assert_eq!(entry.value, "test_value");
        assert_eq!(entry.hit_count, 5);
    }
}
