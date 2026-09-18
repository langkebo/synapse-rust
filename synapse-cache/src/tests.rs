use crate::error::CacheConfig;
use crate::invalidation::InvalidationType;
use crate::local::{route_key, LocalCache};
use crate::manager::CacheManager;
use std::sync::Arc;
use synapse_common::claims::Claims;

#[test]
#[allow(missing_docs)]
fn test_cache_config_default() {
    let config = CacheConfig::default();
    assert_eq!(config.max_capacity, 100_000);
    assert_eq!(config.time_to_live, 7200);
}

#[test]
#[allow(missing_docs)]
fn test_cache_config_custom() {
    let config = CacheConfig { max_capacity: 5000, time_to_live: 7200 };
    assert_eq!(config.max_capacity, 5000);
    assert_eq!(config.time_to_live, 7200);
}

// 审查 #6：本地限流桶改为 moka cache 后，token bucket 行为不破坏（回归）。
#[tokio::test]
#[allow(missing_docs)]
async fn test_rate_limit_token_bucket_local_fallback_basic() {
    let manager = CacheManager::new(&CacheConfig::default());
    // burst_size=2, rate=1/s：前 2 次允许，第 3 次拒绝
    let d1 = manager.rate_limit_token_bucket_take("test:key", 1, 2).await.unwrap();
    assert!(d1.allowed);
    let d2 = manager.rate_limit_token_bucket_take("test:key", 1, 2).await.unwrap();
    assert!(d2.allowed);
    let d3 = manager.rate_limit_token_bucket_take("test:key", 1, 2).await.unwrap();
    assert!(!d3.allowed, "third take must be rejected after burst exhausted");
}

#[tokio::test]
#[allow(missing_docs)]
async fn test_rate_limit_token_bucket_distinct_keys_isolated() {
    let manager = CacheManager::new(&CacheConfig::default());
    let a = manager.rate_limit_token_bucket_take("a", 1, 1).await.unwrap();
    let b = manager.rate_limit_token_bucket_take("b", 1, 1).await.unwrap();
    assert!(a.allowed);
    assert!(b.allowed, "distinct keys must have independent buckets");
}

// 审计 #3：并发 take 不得突破 burst 上限。
//
// 改实现前 `get` 与 `insert` 分离，同一 key 的并发请求会各自读到「令牌充足」
// 并全部放行，实际放行量接近并发数。这里用 barrier 让所有任务同时进入临界区，
// 否则任务可能被顺序调度，竞态不会暴露、测试会假绿。
#[tokio::test(flavor = "multi_thread", worker_threads = 8)]
#[allow(missing_docs)]
async fn test_rate_limit_token_bucket_concurrent_take_respects_burst() {
    use std::sync::Arc;

    const BURST: u32 = 5;
    const CONCURRENCY: usize = 64;

    let manager = Arc::new(CacheManager::new(&CacheConfig::default()));
    let barrier = Arc::new(tokio::sync::Barrier::new(CONCURRENCY));

    let mut handles = Vec::with_capacity(CONCURRENCY);
    for _ in 0..CONCURRENCY {
        let manager = Arc::clone(&manager);
        let barrier = Arc::clone(&barrier);
        handles.push(tokio::spawn(async move {
            barrier.wait().await;
            // rate_per_second = 0：不补充令牌，放行量应精确等于 burst。
            manager.rate_limit_token_bucket_take("concurrent:key", 0, BURST).await.unwrap().allowed
        }));
    }

    let mut allowed = 0usize;
    for handle in handles {
        if handle.await.unwrap() {
            allowed += 1;
        }
    }

    assert_eq!(
        allowed, BURST as usize,
        "concurrent takes must respect the burst limit (allowed {allowed}, burst {BURST})"
    );
}

#[test]
#[allow(missing_docs)]
fn test_local_cache_creation() {
    let config = CacheConfig { max_capacity: 100, time_to_live: 60 };
    let _local_cache = LocalCache::new(&config);
}

#[test]
#[allow(missing_docs)]
fn test_local_cache_set_raw() {
    let config = CacheConfig::default();
    let cache = LocalCache::new(&config);
    cache.set_raw("test_key", "test_value");
    let result = cache.get_raw("test_key");
    assert_eq!(result, Some("test_value".to_string()));
}

#[test]
#[allow(missing_docs)]
fn test_local_cache_get_raw() {
    let config = CacheConfig::default();
    let cache = LocalCache::new(&config);
    let result = cache.get_raw("nonexistent");
    assert!(result.is_none());
}

#[test]
#[allow(missing_docs)]
fn test_local_cache_remove() {
    let config = CacheConfig::default();
    let cache = LocalCache::new(&config);
    cache.set_raw("test_key", "test_value");
    assert!(cache.get_raw("test_key").is_some());
    cache.remove("test_key");
    assert!(cache.get_raw("test_key").is_none());
}

#[test]
#[allow(missing_docs)]
fn test_cache_manager_new() {
    let config = CacheConfig::default();
    let manager = CacheManager::new(&config);
    assert!(!manager.use_redis);
    assert!(manager.redis.is_none());
}

/// `set_raw` is documented as best-effort for L2: an unreachable Redis must not
/// panic, must not abort the write, and must still leave the value in L1. The
/// callers that store auth-relevant markers (`user:logout_all:*`,
/// `token:revocation_ok:*`) rely on exactly this shape — a visible warning plus a
/// durable fallback elsewhere — rather than on the write being infallible.
#[tokio::test]
#[allow(missing_docs)]
async fn test_set_raw_with_unreachable_redis_keeps_l1_and_does_not_panic() {
    let redis_config = synapse_common::config::RedisConfig {
        host: "127.0.0.1".to_string(),
        // Port 1 is reserved and nothing listens there, so the pool is built lazily
        // (`RedisCache::new` does not connect) and every command fails on use.
        port: 1,
        password: None,
        key_prefix: "test:".to_string(),
        pool_size: 1,
        enabled: true,
        connection_timeout_ms: 200,
        command_timeout_ms: 200,
        circuit_breaker: synapse_common::config::CircuitBreakerConfig::default(),
    };
    let manager = CacheManager::with_redis(&redis_config, &CacheConfig::default())
        .expect("pool construction must not require a live server");
    assert!(manager.redis.is_some(), "the Redis path must actually be exercised");

    manager.set_raw("best_effort_key", "best_effort_value", 30).await;

    assert_eq!(manager.get_raw("best_effort_key").as_deref(), Some("best_effort_value"));
}

#[tokio::test]
#[allow(missing_docs)]
async fn test_cache_manager_set_and_get() {
    let config = CacheConfig::default();
    let manager = CacheManager::new(&config);

    let test_value = "test_value".to_string();
    let _ = manager.set("test_key", &test_value, 60).await;

    let result: Option<String> = manager.get::<String>("test_key").await.unwrap();
    assert_eq!(result, Some(test_value));
}

#[tokio::test]
#[allow(missing_docs)]
async fn test_cache_manager_delete() {
    let config = CacheConfig::default();
    let manager = CacheManager::new(&config);

    let test_value = "test_value".to_string();
    let _ = manager.set("test_key", &test_value, 60).await;
    assert!(manager.get::<String>("test_key").await.unwrap().is_some());

    let _ = manager.delete("test_key").await;
    assert!(manager.get::<String>("test_key").await.unwrap().is_none());
}

/// P1 regression guard: `set()` must bound the **local** tier by the
/// per-write `ttl`, not by the builder-wide `CacheConfig::time_to_live`.
///
/// Before the fix, `set()` called `local.set_raw`, which has no TTL argument
/// and fell back to the builder TTL. Callers therefore got a different L1
/// lifetime than they asked for — the login-lockout counter
/// (`LOGIN_LOCKOUT_TTL_SECS = 900`) was retained for the full builder TTL
/// instead. See `docs/audit/P1_security_2026-09-10.md`.
#[tokio::test]
#[allow(missing_docs)]
async fn set_honours_per_write_ttl_for_local_tier() {
    use std::time::Duration;

    let config = CacheConfig {
        max_capacity: 100_000, // builder-wide default
        time_to_live: 3600,
    };
    let manager = CacheManager::new(&config);

    // ...but this write asks for one second.
    manager.set("p1:ttl:probe", &42u32, 1).await.expect("set must succeed");
    assert_eq!(manager.get::<u32>("p1:ttl:probe").await.unwrap(), Some(42), "value must be readable immediately");

    tokio::time::sleep(Duration::from_millis(2000)).await;

    assert_eq!(
        manager.get::<u32>("p1:ttl:probe").await.unwrap(),
        None,
        "the L1 entry must expire on the per-write ttl (1s), not the builder ttl (3600s)"
    );
}

/// P1 regression guard: the plain `get()` intentionally treats a backend
/// failure as a cache miss (`Ok(None)`), so callers **cannot** detect an
/// outage through it. Security-critical callers must use `get_checked`.
///
/// This pins the contract that made the original audit suspicion about
/// `check_login_lockout`'s `_ => Ok(())` arm unreachable.
#[tokio::test]
#[allow(missing_docs)]
async fn get_returns_ok_none_when_redis_is_unreachable() {
    use deadpool_redis::{Config as RedisPoolConfig, Runtime as RedisRuntime};

    const BROKEN_URL: &str = "redis://127.0.0.1:1";
    let pool = RedisPoolConfig::from_url(BROKEN_URL)
        .create_pool(Some(RedisRuntime::Tokio1))
        .expect("failed to build Redis pool for broken-backend test");
    let manager = CacheManager::with_redis_pool_and_url(pool, &CacheConfig::default(), BROKEN_URL);
    assert!(manager.is_redis_enabled(), "the manager is configured for Redis…");

    // …but the backend is dead, and `get` still reports a clean miss.
    assert_eq!(manager.get::<u32>("p1:broken:probe").await.expect("get must not surface the outage"), None);
    // Writes succeed because L1 accepts them.
    manager.set("p1:broken:probe", &7u32, 60).await.expect("set must succeed via L1");
    assert_eq!(manager.get::<u32>("p1:broken:probe").await.unwrap(), Some(7));
}

// C-3: Batch set/get eliminates N+1 cache writes.
#[tokio::test]
#[allow(missing_docs)]
async fn c3_set_batch_serialized_writes_all_keys_to_local_cache() {
    let manager = CacheManager::new(&CacheConfig::default());
    let entries = vec![
        ("c3:batch:k1".to_string(), "v1".to_string(), 60u64),
        ("c3:batch:k2".to_string(), "v2".to_string(), 60u64),
        ("c3:batch:k3".to_string(), "v3".to_string(), 60u64),
    ];
    manager.set_batch_serialized(&entries).await.expect("set_batch_serialized");

    manager.local.cache.run_pending_tasks();
    assert_eq!(manager.get_raw("c3:batch:k1").as_deref(), Some("v1"));
    assert_eq!(manager.get_raw("c3:batch:k2").as_deref(), Some("v2"));
    assert_eq!(manager.get_raw("c3:batch:k3").as_deref(), Some("v3"));
}

#[tokio::test]
#[allow(missing_docs)]
async fn c3_set_batch_typed_writes_and_reads_back() {
    let manager = CacheManager::new(&CacheConfig::default());
    let entries: Vec<(String, String, u64)> =
        vec![("c3:typed:a".to_string(), "alpha".to_string(), 60), ("c3:typed:b".to_string(), "beta".to_string(), 60)];
    manager.set_batch(&entries).await.expect("set_batch");

    let keys = vec!["c3:typed:a".to_string(), "c3:typed:b".to_string()];
    let results: Vec<Option<String>> = manager.get_batch(&keys).await.expect("get_batch");
    assert_eq!(results.len(), 2);
    assert_eq!(results[0].as_deref(), Some("alpha"));
    assert_eq!(results[1].as_deref(), Some("beta"));
}

#[tokio::test]
#[allow(missing_docs)]
async fn c3_set_batch_empty_is_noop() {
    let manager = CacheManager::new(&CacheConfig::default());
    let entries: Vec<(String, String, u64)> = vec![];
    manager.set_batch(&entries).await.expect("set_batch empty should be noop");
    // No panic, no error
}

#[tokio::test]
#[allow(missing_docs)]
async fn c3_set_batch_serialized_empty_is_noop() {
    let manager = CacheManager::new(&CacheConfig::default());
    let entries: Vec<(String, String, u64)> = vec![];
    manager.set_batch_serialized(&entries).await.expect("set_batch_serialized empty should be noop");
}

// PERF-08: broadcast_invalidation 必须同时失效本地 L1——Redis 订阅端
// 跳过本实例自回声，本地不失效就会残留陈旧数据。
#[tokio::test]
#[allow(missing_docs)]
async fn perf08_broadcast_invalidation_clears_local_key() {
    let manager = CacheManager::new(&CacheConfig::default());
    manager.set_raw("perf08:key", "stale", 600).await;
    // moka 写缓冲：先落实写入，保证可见性确定
    manager.local.cache.run_pending_tasks();
    assert!(manager.get_raw("perf08:key").is_some());

    manager.broadcast_invalidation("perf08:key", InvalidationType::Key).await.expect("broadcast");
    manager.local.cache.run_pending_tasks();
    assert!(manager.get_raw("perf08:key").is_none(), "local L1 must be invalidated by broadcast");
}

#[tokio::test]
#[allow(missing_docs)]
async fn perf08_broadcast_invalidation_pattern_clears_local() {
    let manager = CacheManager::new(&CacheConfig::default());
    manager.set_raw("perf08:room:1", "a", 600).await;
    manager.set_raw("perf08:room:2", "b", 600).await;
    manager.set_raw("perf08:other", "c", 600).await;
    manager.local.cache.run_pending_tasks();

    manager.broadcast_invalidation("perf08:room:", InvalidationType::Prefix).await.expect("broadcast");
    manager.local.cache.run_pending_tasks();
    assert!(manager.get_raw("perf08:room:1").is_none());
    assert!(manager.get_raw("perf08:room:2").is_none());
    assert!(manager.get_raw("perf08:other").is_some(), "unrelated key must survive prefix invalidation");
}

#[tokio::test]
#[allow(missing_docs)]
async fn perf08_broadcast_invalidation_all_clears_local() {
    let manager = CacheManager::new(&CacheConfig::default());
    manager.set_raw("perf08:any", "x", 600).await;
    manager.local.cache.run_pending_tasks();
    assert!(manager.get_raw("perf08:any").is_some());

    manager.broadcast_invalidation("*", InvalidationType::All).await.expect("broadcast");
    manager.local.cache.run_pending_tasks();
    assert!(manager.get_raw("perf08:any").is_none());
}

// D-1: L1 必须按调用方 TTL 过期（此前 L1 固定用 builder 级 7200s TTL，
// 与 L2 Redis 的 per-key TTL 不一致）
#[tokio::test]
#[allow(missing_docs)]
async fn d1_set_raw_honors_per_key_ttl_in_local_cache() {
    let manager = CacheManager::new(&CacheConfig::default());
    manager.set_raw("d1:short", "v", 1).await; // 1 秒 TTL
    manager.local.cache.run_pending_tasks();
    assert!(manager.get_raw("d1:short").is_some());

    tokio::time::sleep(std::time::Duration::from_millis(1100)).await;
    assert!(manager.get_raw("d1:short").is_none(), "L1 entry must expire after its per-key TTL");
}

#[tokio::test]
#[allow(missing_docs)]
async fn d1_set_raw_long_ttl_survives_short_window() {
    let manager = CacheManager::new(&CacheConfig::default());
    manager.set_raw("d1:long", "v", 600).await;
    manager.local.cache.run_pending_tasks();

    tokio::time::sleep(std::time::Duration::from_millis(1100)).await;
    assert!(manager.get_raw("d1:long").is_some(), "600s TTL entry must survive a 1.1s window");
}

#[tokio::test]
#[allow(missing_docs)]
async fn test_cache_manager_get_nonexistent() {
    let config = CacheConfig::default();
    let manager = CacheManager::new(&config);

    let result: Option<String> = manager.get::<String>("nonexistent").await.unwrap();
    assert!(result.is_none());
}

// ── S7: get_raw_shared —— L1 未命中时回源 L2(Redis) 并回填 L1 ──────────
//
// presence/e2ee/to-device/list-snapshot 的去重状态通过 set_raw 双写
// L1+L2，但同步 get_raw 只读 L1。跨实例/重启/本地驱逐后 L1 未命中会被
// 误判为「已变化」，回声击穿空闲长轮询（忙循环复发开关）。

/// 构造带 Redis 的 CacheManager；本地 Redis 不可达时返回 None（测试跳过）。
async fn redis_backed_manager(tag: &str) -> Option<(CacheManager, deadpool_redis::Pool, String)> {
    let pool = deadpool_redis::Config::from_url("redis://127.0.0.1:6379")
        .create_pool(Some(deadpool_redis::Runtime::Tokio1))
        .ok()?;
    let probe = tokio::time::timeout(std::time::Duration::from_millis(800), pool.get()).await.ok()?.ok()?;
    drop(probe);
    let manager = CacheManager::with_redis_pool(pool.clone(), &CacheConfig::default());
    let nanos = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_nanos()).unwrap_or(0);
    let key = format!("s7_get_raw_shared:{tag}:{}:{nanos}", std::process::id());
    Some((manager, pool, key))
}

#[tokio::test]
#[allow(missing_docs)]
async fn test_get_raw_shared_without_redis_local_miss_returns_none() {
    let manager = CacheManager::new(&CacheConfig::default());
    assert!(manager.get_raw_shared("s7:no_such_key").await.is_none());
}

#[tokio::test]
#[allow(missing_docs)]
async fn test_get_raw_shared_local_hit() {
    let manager = CacheManager::new(&CacheConfig::default());
    manager.set_raw("s7:local_hit", "v1", 60).await;
    assert_eq!(manager.get_raw_shared("s7:local_hit").await.as_deref(), Some("v1"));
}

#[tokio::test]
#[allow(missing_docs)]
async fn test_get_raw_shared_falls_back_to_redis_and_backfills_local() {
    use redis::AsyncCommands;
    let Some((manager, pool, key)) = redis_backed_manager("fallback").await else {
        eprintln!("skip: local redis unavailable");
        return;
    };

    // 绕过 manager 直写 Redis，模拟「另一个实例写入 / 本实例重启后 L1 为空」
    {
        let mut conn = pool.get().await.expect("redis conn");
        let _: () = conn.set_ex(&key, "shared_value", 60).await.expect("seed redis");
    }
    assert!(manager.get_raw(&key).is_none(), "前置条件：L1 必须未命中");

    let result = manager.get_raw_shared(&key).await;
    assert_eq!(result.as_deref(), Some("shared_value"), "L1 未命中必须回源 Redis");

    // 回源后应回填 L1，后续同步读直接命中
    assert_eq!(manager.get_raw(&key).as_deref(), Some("shared_value"), "回源后必须回填 L1");

    let mut conn = pool.get().await.expect("redis conn");
    let _: () = conn.del(&key).await.expect("cleanup");
}

#[tokio::test]
#[allow(missing_docs)]
async fn test_get_raw_shared_redis_miss_returns_none() {
    let Some((manager, _pool, key)) = redis_backed_manager("miss").await else {
        eprintln!("skip: local redis unavailable");
        return;
    };
    assert!(manager.get_raw_shared(&key).await.is_none(), "L1/L2 均未命中必须返回 None");
}

#[tokio::test]
#[allow(missing_docs)]
async fn test_cache_manager_token_operations() {
    let config = CacheConfig::default();
    let manager = CacheManager::new(&config);

    let now = chrono::Utc::now().timestamp();
    let claims = Claims {
        sub: "test_subject".to_string(),
        user_id: "@test:example.com".to_string(),
        jti: "test-jti-cache-ops".to_string(),
        is_admin: false,
        device_id: Some("DEVICE123".to_string()),
        exp: now + 3600,
        iat: now,
        iss: None,
        aud: None,
    };

    manager.set_token("test_token", &claims, 3600).await;
    let result = manager.get_token("test_token").await;
    assert!(result.is_some());
    assert_eq!(result.unwrap().user_id, "@test:example.com");

    manager.delete_token("test_token").await;
    let result = manager.get_token("test_token").await;
    assert!(result.is_none());
}

#[tokio::test]
#[allow(missing_docs)]
async fn test_get_or_fetch_miss_populates_cache() {
    let config = CacheConfig::default();
    let manager = CacheManager::new(&config);

    // First call: cache miss → fetch → cache → return.
    let result: String =
        manager.get_or_fetch("gof_key", 60, || async { Ok("fetched_value".to_string()) }).await.unwrap();
    assert_eq!(result, "fetched_value");

    // Second call: cache hit → no fetch needed.
    let fetch_called = Arc::new(std::sync::atomic::AtomicBool::new(false));
    let fetch_called_clone = fetch_called.clone();
    let result: String = manager
        .get_or_fetch("gof_key", 60, || {
            let flag = fetch_called_clone.clone();
            async move {
                flag.store(true, std::sync::atomic::Ordering::SeqCst);
                Ok("should_not_be_called".to_string())
            }
        })
        .await
        .unwrap();
    assert_eq!(result, "fetched_value");
    assert!(!fetch_called.load(std::sync::atomic::Ordering::SeqCst));
}

#[tokio::test]
#[allow(missing_docs)]
async fn test_get_or_fetch_single_flight_only_one_fetch() {
    // Verify that concurrent get_or_fetch calls for the same key trigger
    // the fetch closure exactly once.
    let config = CacheConfig::default();
    let manager = Arc::new(CacheManager::new(&config));

    let fetch_count = Arc::new(std::sync::atomic::AtomicU32::new(0));
    let key = "gof_single_flight";

    // Make the fetch slow so concurrent callers pile up waiting on the
    // single-flight guard before the value is cached.
    let mut handles = Vec::new();
    for _ in 0..10 {
        let manager = manager.clone();
        let fetch_count = fetch_count.clone();
        handles.push(tokio::spawn(async move {
            manager
                .get_or_fetch::<_, _, String>(key, 60, || {
                    let count = fetch_count.clone();
                    async move {
                        count.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
                        tokio::time::sleep(std::time::Duration::from_millis(50)).await;
                        Ok("single_flight_value".to_string())
                    }
                })
                .await
                .unwrap()
        }));
    }

    for handle in handles {
        let value = handle.await.unwrap();
        assert_eq!(value, "single_flight_value");
    }

    // Only the first request should have run the fetch; the rest reused
    // the cached result via the double-check inside the single-flight guard.
    assert_eq!(fetch_count.load(std::sync::atomic::Ordering::SeqCst), 1);
}

#[test]
#[allow(missing_docs)]
fn test_claims_struct() {
    let claims = Claims {
        sub: "user_subject".to_string(),
        user_id: "@user:example.com".to_string(),
        jti: "test-jti-claims-struct".to_string(),
        is_admin: false,
        device_id: Some("DEVICE456".to_string()),
        exp: 1234567890,
        iat: 1234567890,
        iss: None,
        aud: None,
    };
    assert_eq!(claims.user_id, "@user:example.com");
    assert_eq!(claims.device_id, Some("DEVICE456".to_string()));
}

// ── D-2: 命名空间缓存隔离测试 ──────────────────────────────────

#[test]
#[allow(missing_docs)]
fn d2_route_key_presence() {
    assert_eq!(route_key("user:@alice:example.com:presence"), Some("presence"));
    assert_eq!(route_key("user:@bob:test.org:presence"), Some("presence"));
    assert_eq!(route_key("user:@alice:example.com:profile"), None);
}

#[test]
#[allow(missing_docs)]
fn d2_route_key_sliding_sync() {
    assert_eq!(route_key("sliding_sync:presence:@alice:dev1"), Some("sliding_sync"));
    assert_eq!(route_key("sliding_sync:e2ee:@alice:dev1"), Some("sliding_sync"));
    assert_eq!(route_key("sliding_sync:filter:abc123"), Some("sliding_sync"));
}

#[test]
#[allow(missing_docs)]
fn d2_route_key_device_keys() {
    assert_eq!(route_key("device_keys_bulk:@alice:example.com"), Some("device_keys"));
}

#[test]
#[allow(missing_docs)]
fn d2_route_key_room_state() {
    assert_eq!(route_key("room_state:!abc:example.com"), Some("room_state"));
}

#[test]
#[allow(missing_docs)]
fn d2_route_key_general() {
    assert_eq!(route_key("token:abc123"), None);
    assert_eq!(route_key("user:@alice:profile"), None);
    assert_eq!(route_key("some_random_key"), None);
}

#[test]
#[allow(missing_docs)]
fn d2_namespace_isolation_presence_does_not_evict_general() {
    let cache = LocalCache::new(&CacheConfig::default());

    cache.set_raw("token:abc", "token_val");
    cache.set_raw("user:@alice:profile", "profile_val");

    for i in 0..100 {
        cache.set_raw(&format!("user:@user{i}:test:presence"), "presence_val");
    }

    cache.cache.run_pending_tasks();
    assert!(cache.get_raw("token:abc").is_some(), "general cache must survive presence flood");
    assert!(cache.get_raw("user:@alice:profile").is_some(), "general cache must survive presence flood");
    assert!(cache.get_raw("user:@user0:test:presence").is_some());
    assert!(cache.get_raw("user:@user99:test:presence").is_some());
}

#[test]
#[allow(missing_docs)]
fn d2_invalidate_all_clears_namespaces() {
    let cache = LocalCache::new(&CacheConfig::default());

    cache.set_raw("token:abc", "token_val");
    cache.set_raw("user:@alice:test:presence", "presence_val");
    cache.set_raw("sliding_sync:presence:@bob:dev1", "sync_val");

    assert!(cache.get_raw("token:abc").is_some());
    assert!(cache.get_raw("user:@alice:test:presence").is_some());
    assert!(cache.get_raw("sliding_sync:presence:@bob:dev1").is_some());

    cache.cache.invalidate_all();
    for ns in cache.namespaces.values() {
        ns.cache.invalidate_all();
        ns.deadlines.write().clear();
    }
    cache.cache.run_pending_tasks();
    for ns in cache.namespaces.values() {
        ns.cache.run_pending_tasks();
    }

    assert!(cache.get_raw("token:abc").is_none());
    assert!(cache.get_raw("user:@alice:test:presence").is_none());
    assert!(cache.get_raw("sliding_sync:presence:@bob:dev1").is_none());
}
// ── W7+: 熔断 / 限流指标接线 ───────────────────────────────────────
//
// 这些测试锁住的是「指标真的被注册」这件事本身。埋点代码即使写对了，
// 只要注入点（AppState::new 里的 attach 调用）被删，指标就永远为 0 而
// 没有任何报错——这类「静默失效」必须有测试兜底。

#[test]
#[allow(missing_docs)]
fn test_attach_circuit_breaker_metrics_registers_counters() {
    // create_pool 是 lazy 的，不需要真的有 Redis 在跑——这里只是要一个
    // 带 circuit_breaker 的 RedisCache 实例。
    let pool = deadpool_redis::Config::from_url("redis://127.0.0.1:6379")
        .create_pool(Some(deadpool_redis::Runtime::Tokio1))
        .expect("pool creation is lazy and must not require a live server");
    let cache = CacheManager::with_redis_pool(pool, &CacheConfig::default());
    let collector = synapse_common::metrics::MetricsCollector::new();

    cache.attach_circuit_breaker_metrics(&collector);

    let all = collector.collect_metrics();
    let names: Vec<&str> = all.iter().map(|m| m.name.as_str()).collect();
    for expected in [
        "circuit_breaker_state",
        "circuit_breaker_requests_total_success",
        "circuit_breaker_requests_total_failure",
        "circuit_breaker_requests_total_timeout",
        "circuit_breaker_requests_total_rejected",
    ] {
        assert!(names.contains(&expected), "missing `{expected}`, got {names:?}");
    }
    // 初值：state=Closed(0)，4 个 counter 均为 0
    for m in &all {
        assert_eq!(m.value, 0.0, "{} should start at 0, got {}", m.name, m.value);
    }
}

#[test]
#[allow(missing_docs)]
fn test_attach_circuit_breaker_metrics_is_idempotent() {
    let pool = deadpool_redis::Config::from_url("redis://127.0.0.1:6379")
        .create_pool(Some(deadpool_redis::Runtime::Tokio1))
        .expect("pool creation is lazy");
    let cache = CacheManager::with_redis_pool(pool, &CacheConfig::default());
    let collector = synapse_common::metrics::MetricsCollector::new();

    // 重复 attach 不能重复注册：MetricsCollector 按 name 覆盖，重复注册
    // 会把已发出的句柄踢出 registry 导致计数分叉。
    cache.attach_circuit_breaker_metrics(&collector);
    let breaker = cache.redis.as_ref().expect("redis").get_circuit_breaker();
    breaker.record_failure();
    cache.attach_circuit_breaker_metrics(&collector);

    let all = collector.collect_metrics();
    let failure = all.iter().find(|m| m.name == "circuit_breaker_requests_total_failure").map_or(0.0, |m| m.value);
    assert_eq!(failure, 1.0, "第二次 attach 不得重置或分叉已有计数");
}

#[test]
#[allow(missing_docs)]
fn test_attach_circuit_breaker_metrics_without_redis_is_noop() {
    // 无 Redis 时没有熔断器可接，静默跳过（不得 panic）
    let cache = CacheManager::new(&CacheConfig::default());
    let collector = synapse_common::metrics::MetricsCollector::new();
    cache.attach_circuit_breaker_metrics(&collector);
    assert!(collector.collect_metrics().is_empty());
}
