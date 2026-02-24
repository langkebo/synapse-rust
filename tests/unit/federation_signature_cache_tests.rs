use synapse_rust::cache::{
    CacheEntryKey, FederationSignatureCache, KeyRotationEvent, SignatureCacheConfig,
    SignatureCacheEntry, DEFAULT_KEY_CACHE_TTL, DEFAULT_KEY_ROTATION_GRACE_PERIOD_MS,
    DEFAULT_SIGNATURE_CACHE_TTL,
};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;
use std::thread;
use std::time::{Duration, Instant};

#[test]
fn test_default_ttl_values() {
    assert_eq!(DEFAULT_SIGNATURE_CACHE_TTL, 3600);
    assert_eq!(DEFAULT_KEY_CACHE_TTL, 3600);
    assert_eq!(DEFAULT_KEY_ROTATION_GRACE_PERIOD_MS, 600_000);
}

#[test]
fn test_signature_cache_config_default() {
    let config = SignatureCacheConfig::default();
    assert_eq!(config.signature_ttl, 3600);
    assert_eq!(config.key_ttl, 3600);
    assert_eq!(config.key_rotation_grace_period_ms, 600_000);
    assert_eq!(config.max_capacity, 10000);
}

#[test]
fn test_signature_cache_config_custom() {
    let config = SignatureCacheConfig::from_federation_config(7200, 1800, 300000);
    assert_eq!(config.signature_ttl, 7200);
    assert_eq!(config.key_ttl, 1800);
    assert_eq!(config.key_rotation_grace_period_ms, 300000);
}

#[test]
fn test_cache_entry_key_creation() {
    let key = CacheEntryKey::new("example.com", "ed25519:abc123", "content_hash");
    assert_eq!(key.origin.as_ref(), "example.com");
    assert_eq!(key.key_id.as_ref(), "ed25519:abc123");
    assert_eq!(key.content_hash.as_ref(), "content_hash");
}

#[test]
fn test_cache_entry_key_to_cache_key() {
    let key = CacheEntryKey::new("matrix.org", "ed25519:key1", "hash123");
    let cache_key = key.to_cache_key();
    assert_eq!(
        cache_key,
        "federation:signature_cache:matrix.org:ed25519:key1:hash123"
    );
}

#[test]
fn test_signature_cache_entry_not_expired() {
    let entry = SignatureCacheEntry::new(true, Duration::from_secs(3600));
    assert!(!entry.is_expired());
    assert!(entry.remaining_ttl() > Duration::from_secs(3500));
}

#[test]
fn test_signature_cache_entry_expired() {
    let entry = SignatureCacheEntry::new(true, Duration::from_millis(10));
    thread::sleep(Duration::from_millis(50));
    assert!(entry.is_expired());
    assert_eq!(entry.remaining_ttl(), Duration::ZERO);
}

#[test]
fn test_signature_cache_entry_verified_flag() {
    let verified_entry = SignatureCacheEntry::new(true, Duration::from_secs(3600));
    assert!(verified_entry.verified);

    let unverified_entry = SignatureCacheEntry::new(false, Duration::from_secs(3600));
    assert!(!unverified_entry.verified);
}

#[test]
fn test_federation_signature_cache_basic_operations() {
    let cache = FederationSignatureCache::new(SignatureCacheConfig::default());
    let key = CacheEntryKey::new("example.com", "ed25519:1", "hash1");

    assert!(cache.get_signature(&key).is_none());

    cache.set_signature(&key, true);
    let entry = cache.get_signature(&key);
    assert!(entry.is_some());
    assert!(entry.unwrap().verified);

    cache.invalidate_signature(&key);
    assert!(cache.get_signature(&key).is_none());
}

#[test]
fn test_federation_signature_cache_unverified_entry() {
    let cache = FederationSignatureCache::new(SignatureCacheConfig::default());
    let key = CacheEntryKey::new("example.com", "ed25519:1", "hash1");

    cache.set_signature(&key, false);
    let entry = cache.get_signature(&key);
    assert!(entry.is_some());
    assert!(!entry.unwrap().verified);
}

#[test]
fn test_federation_signature_cache_invalidate_by_origin() {
    let cache = FederationSignatureCache::new(SignatureCacheConfig::default());

    let key1 = CacheEntryKey::new("example.com", "ed25519:1", "hash1");
    let key2 = CacheEntryKey::new("example.com", "ed25519:2", "hash2");
    let key3 = CacheEntryKey::new("other.org", "ed25519:1", "hash3");

    cache.set_signature(&key1, true);
    cache.set_signature(&key2, true);
    cache.set_signature(&key3, true);

    cache.invalidate_signatures_for_origin("example.com");

    assert!(cache.get_signature(&key1).is_none());
    assert!(cache.get_signature(&key2).is_none());
    assert!(cache.get_signature(&key3).is_some());
}

#[test]
fn test_federation_signature_cache_invalidate_by_key() {
    let cache = FederationSignatureCache::new(SignatureCacheConfig::default());

    let key1 = CacheEntryKey::new("example.com", "ed25519:1", "hash1");
    let key2 = CacheEntryKey::new("example.com", "ed25519:2", "hash2");
    let key3 = CacheEntryKey::new("example.com", "ed25519:1", "hash3");

    cache.set_signature(&key1, true);
    cache.set_signature(&key2, true);
    cache.set_signature(&key3, true);

    cache.invalidate_signatures_for_key("example.com", "ed25519:1");

    assert!(cache.get_signature(&key1).is_none());
    assert!(cache.get_signature(&key2).is_some());
    assert!(cache.get_signature(&key3).is_none());
}

#[test]
fn test_federation_signature_cache_clear_all() {
    let cache = FederationSignatureCache::new(SignatureCacheConfig::default());

    let key1 = CacheEntryKey::new("example.com", "ed25519:1", "hash1");
    let key2 = CacheEntryKey::new("other.org", "ed25519:2", "hash2");

    cache.set_signature(&key1, true);
    cache.set_signature(&key2, true);

    cache.clear_all();

    assert!(cache.get_signature(&key1).is_none());
    assert!(cache.get_signature(&key2).is_none());
}

#[test]
fn test_key_rotation_listener_registration() {
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
fn test_multiple_key_rotation_listeners() {
    let cache = FederationSignatureCache::new(SignatureCacheConfig::default());
    let call_count1 = Arc::new(AtomicUsize::new(0));
    let call_count2 = Arc::new(AtomicUsize::new(0));
    let call_count1_clone = call_count1.clone();
    let call_count2_clone = call_count2.clone();

    cache.register_key_rotation_listener(Arc::new(move |_event| {
        call_count1_clone.fetch_add(1, Ordering::SeqCst);
    }));
    cache.register_key_rotation_listener(Arc::new(move |_event| {
        call_count2_clone.fetch_add(1, Ordering::SeqCst);
    }));

    let event = KeyRotationEvent {
        origin: "example.com".to_string(),
        old_key_id: "ed25519:1".to_string(),
        new_key_id: "ed25519:2".to_string(),
        timestamp: Instant::now(),
    };

    cache.notify_key_rotation(event);
    assert_eq!(call_count1.load(Ordering::SeqCst), 1);
    assert_eq!(call_count2.load(Ordering::SeqCst), 1);
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
}

#[test]
fn test_key_rotation_marks_key_invalidated() {
    let cache = FederationSignatureCache::new(SignatureCacheConfig::default());

    let event = KeyRotationEvent {
        origin: "example.com".to_string(),
        old_key_id: "ed25519:1".to_string(),
        new_key_id: "ed25519:2".to_string(),
        timestamp: Instant::now(),
    };

    cache.notify_key_rotation(event);

    assert!(cache.is_key_invalidated("example.com", "ed25519:1"));
    assert!(!cache.is_key_invalidated("example.com", "ed25519:2"));
}

#[test]
fn test_clear_invalidated_key() {
    let cache = FederationSignatureCache::new(SignatureCacheConfig::default());

    let event = KeyRotationEvent {
        origin: "example.com".to_string(),
        old_key_id: "ed25519:1".to_string(),
        new_key_id: "ed25519:2".to_string(),
        timestamp: Instant::now(),
    };

    cache.notify_key_rotation(event);
    assert!(cache.is_key_invalidated("example.com", "ed25519:1"));

    cache.clear_invalidated_key("example.com", "ed25519:1");
    assert!(!cache.is_key_invalidated("example.com", "ed25519:1"));
}

#[test]
fn test_cache_stats() {
    let cache = FederationSignatureCache::new(SignatureCacheConfig::default());

    let key1 = CacheEntryKey::new("example.com", "ed25519:1", "hash1");
    let key2 = CacheEntryKey::new("example.com", "ed25519:2", "hash2");

    cache.set_signature(&key1, true);
    cache.set_signature(&key2, true);

    let stats = cache.get_stats();
    assert_eq!(stats.entry_count, 2);
    assert_eq!(stats.invalidated_key_count, 0);
    assert_eq!(stats.listener_count, 0);
}

#[test]
fn test_cache_stats_with_rotation() {
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

    let stats = cache.get_stats();
    assert_eq!(stats.invalidated_key_count, 1);
    assert_eq!(stats.listener_count, 1);
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

#[test]
fn test_cache_entry_remaining_ttl() {
    let entry = SignatureCacheEntry::new(true, Duration::from_secs(3600));

    let remaining = entry.remaining_ttl();
    assert!(remaining <= Duration::from_secs(3600));
    assert!(remaining > Duration::from_secs(3500));
}

#[test]
fn test_cache_entry_remaining_ttl_near_expiration() {
    let entry = SignatureCacheEntry::new(true, Duration::from_millis(100));

    thread::sleep(Duration::from_millis(50));

    let remaining = entry.remaining_ttl();
    assert!(remaining < Duration::from_millis(50));
}

#[test]
fn test_cache_clone() {
    let cache = FederationSignatureCache::new(SignatureCacheConfig::default());
    let key = CacheEntryKey::new("example.com", "ed25519:1", "hash1");

    cache.set_signature(&key, true);

    let cloned_cache = cache.clone();

    assert!(cloned_cache.get_signature(&key).is_some());
}

#[test]
fn test_key_rotation_event_fields() {
    let now = Instant::now();
    let event = KeyRotationEvent {
        origin: "matrix.org".to_string(),
        old_key_id: "ed25519:old".to_string(),
        new_key_id: "ed25519:new".to_string(),
        timestamp: now,
    };

    assert_eq!(event.origin, "matrix.org");
    assert_eq!(event.old_key_id, "ed25519:old");
    assert_eq!(event.new_key_id, "ed25519:new");
    assert_eq!(event.timestamp, now);
}

#[test]
fn test_cache_config_from_config() {
    let config =
        SignatureCacheConfig::from_federation_config(DEFAULT_SIGNATURE_CACHE_TTL, DEFAULT_KEY_CACHE_TTL, DEFAULT_KEY_ROTATION_GRACE_PERIOD_MS);

    assert_eq!(config.signature_ttl, DEFAULT_SIGNATURE_CACHE_TTL);
    assert_eq!(config.key_ttl, DEFAULT_KEY_CACHE_TTL);
    assert_eq!(
        config.key_rotation_grace_period_ms,
        DEFAULT_KEY_ROTATION_GRACE_PERIOD_MS
    );
}

#[test]
fn test_cache_with_custom_max_capacity() {
    let config = SignatureCacheConfig {
        max_capacity: 100,
        ..Default::default()
    };
    let cache = FederationSignatureCache::new(config);

    for i in 0..150 {
        let key = CacheEntryKey::new("example.com", "ed25519:1", &format!("hash{}", i));
        cache.set_signature(&key, true);
    }

    let stats = cache.get_stats();
    assert!(stats.entry_count <= 100);
}

#[test]
fn test_concurrent_cache_access() {
    let cache = Arc::new(FederationSignatureCache::new(SignatureCacheConfig::default()));
    let mut handles = vec![];

    for i in 0..10 {
        let cache_clone = cache.clone();
        let handle = thread::spawn(move || {
            let key = CacheEntryKey::new("example.com", "ed25519:1", &format!("hash{}", i));
            cache_clone.set_signature(&key, true);
            cache_clone.get_signature(&key)
        });
        handles.push(handle);
    }

    for handle in handles {
        let result = handle.join().unwrap();
        assert!(result.is_some());
    }
}

#[test]
fn test_cache_entry_with_zero_ttl() {
    let entry = SignatureCacheEntry::new(true, Duration::ZERO);
    assert!(entry.is_expired());
    assert_eq!(entry.remaining_ttl(), Duration::ZERO);
}

#[test]
fn test_cache_clear_all_clears_invalidated_keys() {
    let cache = FederationSignatureCache::new(SignatureCacheConfig::default());

    let event = KeyRotationEvent {
        origin: "example.com".to_string(),
        old_key_id: "ed25519:1".to_string(),
        new_key_id: "ed25519:2".to_string(),
        timestamp: Instant::now(),
    };

    cache.notify_key_rotation(event);
    assert!(cache.is_key_invalidated("example.com", "ed25519:1"));

    cache.clear_all();
    assert!(!cache.is_key_invalidated("example.com", "ed25519:1"));
}
