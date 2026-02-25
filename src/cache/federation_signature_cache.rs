use moka::sync::Cache;
use parking_lot::RwLock;
use serde::Serialize;
use std::collections::HashSet;
use std::sync::Arc;
use std::time::{Duration, Instant};

pub const DEFAULT_SIGNATURE_CACHE_TTL: u64 = 3600;
pub const DEFAULT_KEY_CACHE_TTL: u64 = 3600;
pub const DEFAULT_KEY_ROTATION_GRACE_PERIOD_MS: u64 = 600 * 1000;

#[derive(Debug, Clone, Copy, Serialize)]
pub struct SignatureCacheConfig {
    pub signature_ttl: u64,
    pub key_ttl: u64,
    pub key_rotation_grace_period_ms: u64,
    pub max_capacity: u64,
}

impl Default for SignatureCacheConfig {
    fn default() -> Self {
        Self {
            signature_ttl: DEFAULT_SIGNATURE_CACHE_TTL,
            key_ttl: DEFAULT_KEY_CACHE_TTL,
            key_rotation_grace_period_ms: DEFAULT_KEY_ROTATION_GRACE_PERIOD_MS,
            max_capacity: 10000,
        }
    }
}

impl SignatureCacheConfig {
    pub fn from_federation_config(
        signature_cache_ttl: u64,
        key_cache_ttl: u64,
        key_rotation_grace_period_ms: u64,
    ) -> Self {
        Self {
            signature_ttl: signature_cache_ttl,
            key_ttl: key_cache_ttl,
            key_rotation_grace_period_ms,
            max_capacity: 10000,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct CacheEntryKey {
    pub origin: Arc<str>,
    pub key_id: Arc<str>,
    pub content_hash: Arc<str>,
}

impl CacheEntryKey {
    pub fn new(origin: &str, key_id: &str, content_hash: &str) -> Self {
        Self {
            origin: Arc::from(origin),
            key_id: Arc::from(key_id),
            content_hash: Arc::from(content_hash),
        }
    }

    pub fn to_cache_key(&self) -> String {
        format!(
            "federation:signature_cache:{}:{}:{}",
            self.origin, self.key_id, self.content_hash
        )
    }
}

#[derive(Debug, Clone)]
pub struct SignatureCacheEntry {
    pub verified: bool,
    pub cached_at: Instant,
    pub ttl: Duration,
}

impl SignatureCacheEntry {
    pub fn new(verified: bool, ttl: Duration) -> Self {
        Self {
            verified,
            cached_at: Instant::now(),
            ttl,
        }
    }

    pub fn is_expired(&self) -> bool {
        self.cached_at.elapsed() > self.ttl
    }

    pub fn remaining_ttl(&self) -> Duration {
        self.ttl.saturating_sub(self.cached_at.elapsed())
    }
}

#[derive(Debug, Clone)]
pub struct KeyRotationEvent {
    pub origin: String,
    pub old_key_id: String,
    pub new_key_id: String,
    pub timestamp: Instant,
}

pub type KeyRotationCallback = Arc<dyn Fn(KeyRotationEvent) + Send + Sync>;

pub struct FederationSignatureCache {
    signature_cache: Cache<String, SignatureCacheEntry>,
    config: SignatureCacheConfig,
    key_rotation_listeners: RwLock<Vec<KeyRotationCallback>>,
    invalidated_keys: RwLock<HashSet<String>>,
}

impl std::fmt::Debug for FederationSignatureCache {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("FederationSignatureCache")
            .field("config", &self.config)
            .field("invalidated_keys", &self.invalidated_keys.read().len())
            .field("listener_count", &self.key_rotation_listeners.read().len())
            .finish()
    }
}

impl FederationSignatureCache {
    pub fn new(config: SignatureCacheConfig) -> Self {
        let signature_cache = Cache::builder()
            .max_capacity(config.max_capacity)
            .time_to_live(Duration::from_secs(config.signature_ttl))
            .support_invalidation_closures()
            .build();

        Self {
            signature_cache,
            config,
            key_rotation_listeners: RwLock::new(Vec::new()),
            invalidated_keys: RwLock::new(HashSet::new()),
        }
    }

    pub fn get_signature(&self, key: &CacheEntryKey) -> Option<SignatureCacheEntry> {
        let cache_key = key.to_cache_key();
        self.signature_cache.get(&cache_key)
    }

    pub fn set_signature(&self, key: &CacheEntryKey, verified: bool) {
        let cache_key = key.to_cache_key();
        let entry =
            SignatureCacheEntry::new(verified, Duration::from_secs(self.config.signature_ttl));
        self.signature_cache.insert(cache_key, entry);
    }

    pub fn invalidate_signature(&self, key: &CacheEntryKey) {
        let cache_key = key.to_cache_key();
        self.signature_cache.invalidate(&cache_key);
    }

    pub fn invalidate_signatures_for_origin(&self, origin: &str) {
        let prefix = format!("federation:signature_cache:{}:", origin);
        let _ = self.signature_cache
            .invalidate_entries_if(move |k: &String, _| k.starts_with(&prefix));
    }

    pub fn invalidate_signatures_for_key(&self, origin: &str, key_id: &str) {
        let prefix = format!("federation:signature_cache:{}:{}:", origin, key_id);
        let _ = self.signature_cache
            .invalidate_entries_if(move |k: &String, _| k.starts_with(&prefix));
    }

    pub fn clear_all(&self) {
        self.signature_cache.invalidate_all();
        self.invalidated_keys.write().clear();
    }

    pub fn register_key_rotation_listener(&self, callback: KeyRotationCallback) {
        self.key_rotation_listeners.write().push(callback);
    }

    pub fn notify_key_rotation(&self, event: KeyRotationEvent) {
        let cache_key = format!(
            "federation:verify_key:{}:{}",
            event.origin, event.old_key_id
        );
        self.invalidated_keys.write().insert(cache_key);

        self.invalidate_signatures_for_key(&event.origin, &event.old_key_id);

        let listeners = self.key_rotation_listeners.read();
        for listener in listeners.iter() {
            listener(event.clone());
        }
    }

    pub fn is_key_invalidated(&self, origin: &str, key_id: &str) -> bool {
        let cache_key = format!("federation:verify_key:{}:{}", origin, key_id);
        self.invalidated_keys.read().contains(&cache_key)
    }

    pub fn clear_invalidated_key(&self, origin: &str, key_id: &str) {
        let cache_key = format!("federation:verify_key:{}:{}", origin, key_id);
        self.invalidated_keys.write().remove(&cache_key);
    }

    pub fn get_stats(&self) -> SignatureCacheStats {
        SignatureCacheStats {
            entry_count: self.signature_cache.entry_count(),
            invalidated_key_count: self.invalidated_keys.read().len(),
            listener_count: self.key_rotation_listeners.read().len(),
            config: self.config.clone(),
        }
    }

    pub fn get_config(&self) -> &SignatureCacheConfig {
        &self.config
    }
}

impl Clone for FederationSignatureCache {
    fn clone(&self) -> Self {
        Self {
            signature_cache: self.signature_cache.clone(),
            config: self.config.clone(),
            key_rotation_listeners: RwLock::new(self.key_rotation_listeners.read().clone()),
            invalidated_keys: RwLock::new(self.invalidated_keys.read().clone()),
        }
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct SignatureCacheStats {
    pub entry_count: u64,
    pub invalidated_key_count: usize,
    pub listener_count: usize,
    pub config: SignatureCacheConfig,
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::thread;
    use std::time::Duration;

    #[test]
    fn test_signature_cache_config_default() {
        let config = SignatureCacheConfig::default();
        assert_eq!(config.signature_ttl, 3600);
        assert_eq!(config.key_ttl, 3600);
        assert_eq!(config.key_rotation_grace_period_ms, 600000);
    }

    #[test]
    fn test_signature_cache_config_from_federation() {
        let config = SignatureCacheConfig::from_federation_config(7200, 1800, 300000);
        assert_eq!(config.signature_ttl, 7200);
        assert_eq!(config.key_ttl, 1800);
        assert_eq!(config.key_rotation_grace_period_ms, 300000);
    }

    #[test]
    fn test_cache_entry_key() {
        let key = CacheEntryKey::new("example.com", "ed25519:1", "abc123");
        assert_eq!(key.origin.as_ref(), "example.com");
        assert_eq!(key.key_id.as_ref(), "ed25519:1");
        assert_eq!(key.content_hash.as_ref(), "abc123");
        assert_eq!(
            key.to_cache_key(),
            "federation:signature_cache:example.com:ed25519:1:abc123"
        );
    }

    #[test]
    fn test_signature_cache_entry() {
        let entry = SignatureCacheEntry::new(true, Duration::from_secs(3600));
        assert!(entry.verified);
        assert!(!entry.is_expired());
        assert!(entry.remaining_ttl() > Duration::from_secs(3500));
    }

    #[test]
    fn test_signature_cache_entry_expiration() {
        let entry = SignatureCacheEntry::new(true, Duration::from_millis(10));
        thread::sleep(Duration::from_millis(20));
        assert!(entry.is_expired());
        assert_eq!(entry.remaining_ttl(), Duration::ZERO);
    }

    #[test]
    fn test_federation_signature_cache_basic() {
        let cache = FederationSignatureCache::new(SignatureCacheConfig::default());
        let key = CacheEntryKey::new("example.com", "ed25519:1", "hash123");

        assert!(cache.get_signature(&key).is_none());

        cache.set_signature(&key, true);
        let entry = cache.get_signature(&key);
        assert!(entry.is_some());
        assert!(entry.unwrap().verified);

        cache.invalidate_signature(&key);
        assert!(cache.get_signature(&key).is_none());
    }

    #[test]
    fn test_federation_signature_cache_invalidate_origin() {
        let cache = FederationSignatureCache::new(SignatureCacheConfig::default());

        let key1 = CacheEntryKey::new("example.com", "ed25519:1", "hash1");
        let key2 = CacheEntryKey::new("example.com", "ed25519:2", "hash2");
        let key3 = CacheEntryKey::new("other.com", "ed25519:1", "hash3");

        cache.set_signature(&key1, true);
        cache.set_signature(&key2, true);
        cache.set_signature(&key3, true);

        cache.invalidate_signatures_for_origin("example.com");

        assert!(cache.get_signature(&key1).is_none());
        assert!(cache.get_signature(&key2).is_none());
        assert!(cache.get_signature(&key3).is_some());
    }

    #[test]
    fn test_federation_signature_cache_invalidate_key() {
        let cache = FederationSignatureCache::new(SignatureCacheConfig::default());

        let key1 = CacheEntryKey::new("example.com", "ed25519:1", "hash1");
        let key2 = CacheEntryKey::new("example.com", "ed25519:2", "hash2");

        cache.set_signature(&key1, true);
        cache.set_signature(&key2, true);

        cache.invalidate_signatures_for_key("example.com", "ed25519:1");

        assert!(cache.get_signature(&key1).is_none());
        assert!(cache.get_signature(&key2).is_some());
    }

    #[test]
    fn test_federation_signature_cache_clear_all() {
        let cache = FederationSignatureCache::new(SignatureCacheConfig::default());

        let key1 = CacheEntryKey::new("example.com", "ed25519:1", "hash1");
        let key2 = CacheEntryKey::new("other.com", "ed25519:1", "hash2");

        cache.set_signature(&key1, true);
        cache.set_signature(&key2, true);

        cache.clear_all();

        assert!(cache.get_signature(&key1).is_none());
        assert!(cache.get_signature(&key2).is_none());
    }

    #[test]
    fn test_key_rotation_listener() {
        let cache = FederationSignatureCache::new(SignatureCacheConfig::default());
        let call_count = Arc::new(AtomicUsize::new(0));
        let call_count_clone = call_count.clone();

        cache.register_key_rotation_listener(Arc::new(move |_event| {
            call_count_clone.fetch_add(1, Ordering::SeqCst);
        }));

        let event = KeyRotationEvent {
            origin: "example.com".to_string(),
            old_key_id: "ed25519:1".to_string(),
            new_key_id: "ed25519:2".to_string(),
            timestamp: Instant::now(),
        };

        cache.notify_key_rotation(event);

        assert_eq!(call_count.load(Ordering::SeqCst), 1);
    }

    #[test]
    fn test_key_rotation_invalidates_cache() {
        let cache = FederationSignatureCache::new(SignatureCacheConfig::default());

        let key = CacheEntryKey::new("example.com", "ed25519:1", "hash1");
        cache.set_signature(&key, true);
        assert!(cache.get_signature(&key).is_some());

        let event = KeyRotationEvent {
            origin: "example.com".to_string(),
            old_key_id: "ed25519:1".to_string(),
            new_key_id: "ed25519:2".to_string(),
            timestamp: Instant::now(),
        };

        cache.notify_key_rotation(event);

        assert!(cache.get_signature(&key).is_none());
        assert!(cache.is_key_invalidated("example.com", "ed25519:1"));
    }

    #[test]
    fn test_cache_stats() {
        let cache = FederationSignatureCache::new(SignatureCacheConfig::default());

        let key1 = CacheEntryKey::new("example.com", "ed25519:1", "hash1");
        let key2 = CacheEntryKey::new("example.com", "ed25519:2", "hash2");

        cache.set_signature(&key1, true);
        cache.set_signature(&key2, true);

        cache.signature_cache.run_pending_tasks();

        let stats = cache.get_stats();
        assert_eq!(stats.entry_count, 2);
        assert_eq!(stats.invalidated_key_count, 0);
        assert_eq!(stats.listener_count, 0);
    }

    #[test]
    fn test_cache_ttl_expiration() {
        let config = SignatureCacheConfig {
            signature_ttl: 1,
            ..Default::default()
        };
        let cache = FederationSignatureCache::new(config);

        let key = CacheEntryKey::new("example.com", "ed25519:1", "hash1");
        cache.set_signature(&key, true);

        assert!(cache.get_signature(&key).is_some());

        thread::sleep(Duration::from_millis(1100));

        assert!(cache.get_signature(&key).is_none());
    }
}
