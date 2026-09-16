use std::collections::HashMap;
use std::sync::Arc;
use synapse_cache::CacheHealthCheck;
use synapse_cache::{CacheManager, FederationSignatureCache, SignatureCacheConfig};
use synapse_common::health::{DatabaseHealthCheck, HealthChecker};
use synapse_common::security::{ReplayProtectionCache, ReplayProtectionConfig};
use synapse_common::{RateLimitConfigFile, RateLimitConfigManager, SyncRateLimitConfigFile};
use synapse_services::ServiceContainer;
use tokio::sync::{Mutex, RwLock, Semaphore};

/// The `AppState` struct.
#[derive(Clone)]
pub struct AppState {
    // `services` 必须用 Arc 包裹：AppState 经 `#[derive(Clone)]` 实现深拷贝，
    // 而 axum 的 `Router::with_state` 会把 state clone 进每一个路由端点（~883 个）。
    // 若这里是裸值 ServiceContainer，每次 clone 都会深拷贝整棵服务树
    // （ServiceContainer → CoreServices → Config → UrlPreviewConfig 的 Vec），
    // 883 份副本常驻内存 ≈ 1.7GB（jemalloc prof 实测 clone_subtree 占 84.8%）。
    // 改 Arc 后 clone 只是引用计数 +1，Config 全程仅一份。
    /// The `services` field.
    pub services: Arc<ServiceContainer>,
    /// The `cache` field.
    pub cache: Arc<CacheManager>,
    /// The `health_checker` field.
    pub health_checker: Arc<HealthChecker>,
    /// The `federation_signature_cache` field.
    pub federation_signature_cache: Arc<FederationSignatureCache>,
    /// S1 修复：联邦重放保护缓存，用于在时间窗口内去重已验签的请求签名。
    pub replay_protection_cache: Arc<ReplayProtectionCache>,
    /// The `federation_key_fetch_priority_semaphore` field.
    pub federation_key_fetch_priority_semaphore: Arc<Semaphore>,
    /// The `federation_key_fetch_general_semaphore` field.
    pub federation_key_fetch_general_semaphore: Arc<Semaphore>,
    /// The `federation_inbound_edu_semaphore` field.
    pub federation_inbound_edu_semaphore: Arc<Semaphore>,
    /// The `federation_join_semaphore` field.
    pub federation_join_semaphore: Arc<Semaphore>,
    /// The `federation_inbound_edu_origin_semaphores` field.
    pub federation_inbound_edu_origin_semaphores: Arc<Mutex<HashMap<String, Arc<Semaphore>>>>,
    /// The `federation_presence_backoff_until` field.
    pub federation_presence_backoff_until: Arc<RwLock<HashMap<String, i64>>>,
    rate_limit_config_manager: Option<Arc<RateLimitConfigManager>>,
    /// B-4: Paths auto-derived from the route ledger (`rate_limit_exempt = true`
    /// entries) that the IP-level rate limit middleware should skip. Populated
    /// by `create_router` after ledger validation. Empty when the server is not
    /// assembled through `create_router` (e.g. in unit tests).
    pub rate_limit_exempt_paths: Arc<Vec<&'static str>>,
    /// Optional graceful-shutdown signal. When set, the `POST /_synapse/admin/v1/restart`
    /// endpoint triggers it so the process manager (Docker / systemd) can restart
    /// the homeserver cleanly.
    pub shutdown_signal: Option<tokio::sync::broadcast::Sender<()>>,
    /// 测试专用：持有从 schema pool 租借的 schema 租约（`LeasedSchema`）。
    /// 租约的生命周期随 `AppState`/`Router` 走——最后一个 `Arc` 释放时 schema
    /// 才 TRUNCATE 并归还池，避免 `setup_fresh_test_app*` 系列丢弃 `TestContext`
    /// 时提前归还 schema（并发下被其它测试复用 → 数据竞态 → 401「User not found」）。
    /// 生产构建（无 `test-utils` feature）不编译此字段，恒为 `None`。
    #[cfg(feature = "test-utils")]
    /// The `test_schema_lease` field.
    pub test_schema_lease: Option<Arc<synapse_test_utils::LeasedSchema>>,
}

/// The `SyncRateLimitOverride` struct.
#[derive(Debug, Clone)]
pub struct SyncRateLimitOverride {
    /// The `fail_open_on_error` field.
    pub fail_open_on_error: bool,
    /// The `sync` field.
    pub sync: SyncRateLimitConfigFile,
}

impl AppState {
    /// See [`new`].
    pub fn new(services: ServiceContainer, cache: Arc<CacheManager>) -> Self {
        let pool = services.database_pool();
        let mut health_checker = HealthChecker::new("0.1.0".to_string());

        health_checker.add_check(Box::new(DatabaseHealthCheck::new((*pool).clone())));
        health_checker.add_check(Box::new(CacheHealthCheck::new((*cache).clone())));

        let federation_signature_cache =
            Arc::new(FederationSignatureCache::new(SignatureCacheConfig::from_federation_config(
                services.core.config.federation.signature_cache_ttl,
                services.core.config.federation.key_cache_ttl,
                services.core.config.federation.key_rotation_grace_period_ms,
            )));

        // S1 修复：初始化联邦重放保护缓存。
        let replay_protection_cache = Arc::new(ReplayProtectionCache::new(ReplayProtectionConfig {
            enabled: services.core.config.federation.replay_protection_enabled,
            cache_size: 10_000,
            window_secs: 300,
        }));

        // Wire federation signature cache to key rotation manager so that
        // cached signature verification results are invalidated on key rotation.
        services.federation.key_rotation_manager.set_signature_cache(federation_signature_cache.clone());

        let key_fetch_max_concurrency = services.core.config.federation.key_fetch_max_concurrency.max(1);
        let key_fetch_general_max_concurrency =
            if key_fetch_max_concurrency <= 1 { 1 } else { (key_fetch_max_concurrency - 1).max(1) };
        let inbound_edu_max_concurrency = services.core.config.federation.inbound_edu_max_concurrency.max(1);
        let join_max_concurrency = services.core.config.federation.join_max_concurrency.max(1);

        // W7+: 把 Redis 熔断器接到 metrics collector 上。构造器拿不到
        // collector（`CircuitBreaker::new` 不知道它的存在），故在此——
        // AppState 组装完成、services 与 cache 都已就位时——注入一次。
        // 幂等：内部 OnceLock 保证重复调用不会重复注册 counter。
        cache.attach_circuit_breaker_metrics(&services.core.metrics);

        Self {
            services: Arc::new(services),
            cache,
            health_checker: Arc::new(health_checker),
            federation_signature_cache,
            replay_protection_cache,
            federation_key_fetch_priority_semaphore: Arc::new(Semaphore::new(key_fetch_max_concurrency)),
            federation_key_fetch_general_semaphore: Arc::new(Semaphore::new(key_fetch_general_max_concurrency)),
            federation_inbound_edu_semaphore: Arc::new(Semaphore::new(inbound_edu_max_concurrency)),
            federation_join_semaphore: Arc::new(Semaphore::new(join_max_concurrency)),
            federation_inbound_edu_origin_semaphores: Arc::new(Mutex::new(HashMap::new())),
            federation_presence_backoff_until: Arc::new(RwLock::new(HashMap::new())),
            rate_limit_config_manager: None,
            rate_limit_exempt_paths: Arc::new(Vec::new()),
            shutdown_signal: None,
            #[cfg(feature = "test-utils")]
            test_schema_lease: None,
        }
    }

    /// See [`with_rate_limit_config`].
    pub fn with_rate_limit_config(mut self, manager: Arc<RateLimitConfigManager>) -> Self {
        self.rate_limit_config_manager = Some(manager);
        self
    }

    /// B-4: Set the auto-derived rate limit exempt paths collected from the
    /// route ledger. Called by `create_router` after ledger validation.
    pub fn with_rate_limit_exempt_paths(mut self, paths: Vec<&'static str>) -> Self {
        self.rate_limit_exempt_paths = Arc::new(paths);
        self
    }

    /// Wire the graceful-shutdown broadcast sender so admin endpoints
    /// (e.g. `POST /_synapse/admin/v1/restart`) can trigger a clean exit.
    pub fn with_shutdown_signal(mut self, shutdown_tx: tokio::sync::broadcast::Sender<()>) -> Self {
        self.shutdown_signal = Some(shutdown_tx);
        self
    }

    /// See [`rate_limit_config`].
    pub fn rate_limit_config(&self) -> Option<RateLimitConfigFile> {
        self.rate_limit_config_manager.as_ref().map(|manager| manager.get_config())
    }

    /// See [`rate_limit_config_manager`].
    pub fn rate_limit_config_manager(&self) -> Option<&Arc<RateLimitConfigManager>> {
        self.rate_limit_config_manager.as_ref()
    }

    /// See [`sync_rate_limit_override`].
    pub fn sync_rate_limit_override(&self) -> Option<SyncRateLimitOverride> {
        self.rate_limit_config_manager.as_ref().map(|manager| {
            let config = manager.get_config();
            SyncRateLimitOverride { fail_open_on_error: config.fail_open_on_error, sync: config.sync }
        })
    }
}
