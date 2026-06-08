use synapse_cache::{CacheManager, FederationSignatureCache, SignatureCacheConfig};
use synapse_common::health::{CheckResult, DatabaseHealthCheck, HealthCheck, HealthChecker};
use synapse_common::rate_limit_config::{RateLimitConfigFile, RateLimitConfigManager, SyncRateLimitConfigFile};
use synapse_services::ServiceContainer;
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::{Mutex, RwLock, Semaphore};

#[derive(Clone)]
pub struct AppState {
    pub services: ServiceContainer,
    pub cache: Arc<CacheManager>,
    pub health_checker: Arc<HealthChecker>,
    pub federation_signature_cache: Arc<FederationSignatureCache>,
    pub federation_key_fetch_priority_semaphore: Arc<Semaphore>,
    pub federation_key_fetch_general_semaphore: Arc<Semaphore>,
    pub federation_inbound_edu_semaphore: Arc<Semaphore>,
    pub federation_join_semaphore: Arc<Semaphore>,
    pub federation_inbound_edu_origin_semaphores: Arc<Mutex<HashMap<String, Arc<Semaphore>>>>,
    pub federation_presence_backoff_until: Arc<RwLock<HashMap<String, i64>>>,
    rate_limit_config_manager: Option<Arc<RateLimitConfigManager>>,
    #[cfg(feature = "openclaw-routes")]
    pub ai_connection_storage: Arc<synapse_storage::ai_connection::AiConnectionStorage>,
    #[cfg(feature = "openclaw-routes")]
    pub matrix_ai_connection_service: Arc<synapse_services::matrix_ai_connection_service::MatrixAiConnectionService>,
    #[cfg(feature = "openclaw-routes")]
    pub mcp_proxy_service: Arc<synapse_services::mcp_proxy::McpProxyService>,
    #[cfg(feature = "openclaw-routes")]
    pub openclaw_service: Arc<synapse_services::openclaw_service::OpenClawService>,
}

#[derive(Debug, Clone)]
pub struct SyncRateLimitOverride {
    pub fail_open_on_error: bool,
    pub sync: SyncRateLimitConfigFile,
}

impl AppState {
    pub fn new(services: ServiceContainer, cache: Arc<CacheManager>) -> Self {
        let mut health_checker = HealthChecker::new("0.1.0".to_string());

        health_checker.add_check(Box::new(DatabaseHealthCheck::new((*services.account.user_storage.pool).clone())));
        health_checker.add_check(Box::new(CacheHealthCheck::new(Arc::clone(&cache))));

        let federation_signature_cache =
            Arc::new(FederationSignatureCache::new(SignatureCacheConfig::from_federation_config(
                services.core.config.federation.signature_cache_ttl,
                services.core.config.federation.key_cache_ttl,
                services.core.config.federation.key_rotation_grace_period_ms,
            )));

        // Wire federation signature cache to key rotation manager so that
        // cached signature verification results are invalidated on key rotation.
        services.federation.key_rotation_manager.set_signature_cache(federation_signature_cache.clone());

        #[cfg(feature = "openclaw-routes")]
        let pool = services.account.user_storage.pool.clone();
        #[cfg(feature = "openclaw-routes")]
        let openclaw_service = {
            let openclaw_storage = Arc::new(synapse_storage::openclaw::OpenClawStorage::new(pool.clone()));
            let encryption_key = synapse_services::openclaw_service::OpenClawService::resolve_encryption_key(
                services.core.config.server.macaroon_secret_key.as_deref(),
                &services.core.config.security.secret,
            );
            Arc::new(synapse_services::openclaw_service::OpenClawService::new(openclaw_storage, encryption_key))
        };
        let key_fetch_max_concurrency = services.core.config.federation.key_fetch_max_concurrency.max(1);
        let key_fetch_general_max_concurrency =
            if key_fetch_max_concurrency <= 1 { 1 } else { (key_fetch_max_concurrency - 1).max(1) };
        let inbound_edu_max_concurrency = services.core.config.federation.inbound_edu_max_concurrency.max(1);
        let join_max_concurrency = services.core.config.federation.join_max_concurrency.max(1);

        Self {
            services,
            cache: cache.clone(),
            health_checker: Arc::new(health_checker),
            federation_signature_cache,
            federation_key_fetch_priority_semaphore: Arc::new(Semaphore::new(key_fetch_max_concurrency)),
            federation_key_fetch_general_semaphore: Arc::new(Semaphore::new(key_fetch_general_max_concurrency)),
            federation_inbound_edu_semaphore: Arc::new(Semaphore::new(inbound_edu_max_concurrency)),
            federation_join_semaphore: Arc::new(Semaphore::new(join_max_concurrency)),
            federation_inbound_edu_origin_semaphores: Arc::new(Mutex::new(HashMap::new())),
            federation_presence_backoff_until: Arc::new(RwLock::new(HashMap::new())),
            rate_limit_config_manager: None,
            #[cfg(feature = "openclaw-routes")]
            ai_connection_storage: Arc::new(synapse_storage::ai_connection::AiConnectionStorage::new(pool.clone())),
            #[cfg(feature = "openclaw-routes")]
            matrix_ai_connection_service: Arc::new(
                synapse_services::matrix_ai_connection_service::MatrixAiConnectionService::new(
                    Arc::new(synapse_storage::ai_connection::AiConnectionStorage::new(pool)),
                    Arc::new(synapse_services::mcp_proxy::McpProxyService::new(cache.clone())),
                ),
            ),
            #[cfg(feature = "openclaw-routes")]
            mcp_proxy_service: Arc::new(synapse_services::mcp_proxy::McpProxyService::new(cache)),
            #[cfg(feature = "openclaw-routes")]
            openclaw_service,
        }
    }

    pub fn with_rate_limit_config(mut self, manager: Arc<RateLimitConfigManager>) -> Self {
        self.rate_limit_config_manager = Some(manager);
        self
    }

    pub fn rate_limit_config(&self) -> Option<RateLimitConfigFile> {
        self.rate_limit_config_manager.as_ref().map(|manager| manager.get_config())
    }

    pub fn sync_rate_limit_override(&self) -> Option<SyncRateLimitOverride> {
        self.rate_limit_config_manager.as_ref().map(|manager| {
            let config = manager.get_config();
            SyncRateLimitOverride { fail_open_on_error: config.fail_open_on_error, sync: config.sync }
        })
    }
}

/// Cache health check implementation.
/// Moved from synapse-common to the main crate to avoid circular dependency.
struct CacheHealthCheck {
    cache: Arc<CacheManager>,
}

impl CacheHealthCheck {
    fn new(cache: Arc<CacheManager>) -> Self {
        Self { cache }
    }
}

#[async_trait::async_trait]
impl HealthCheck for CacheHealthCheck {
    async fn check(&self) -> CheckResult {
        let start = std::time::Instant::now();

        match self.cache.set("health_check", "ok", 10).await {
            Ok(_) => match self.cache.get::<String>("health_check").await {
                Ok(Some(value)) if value == "ok" => CheckResult {
                    status: "healthy".to_string(),
                    message: "Cache connection successful".to_string(),
                    duration_ms: start.elapsed().as_millis() as u64,
                },
                Ok(None) => CheckResult {
                    status: "degraded".to_string(),
                    message: "Cache read returned None".to_string(),
                    duration_ms: start.elapsed().as_millis() as u64,
                },
                Ok(Some(_)) => CheckResult {
                    status: "degraded".to_string(),
                    message: "Cache value mismatch".to_string(),
                    duration_ms: start.elapsed().as_millis() as u64,
                },
                Err(e) => CheckResult {
                    status: "unhealthy".to_string(),
                    message: format!("Cache read failed: {e}"),
                    duration_ms: start.elapsed().as_millis() as u64,
                },
            },
            Err(e) => CheckResult {
                status: "unhealthy".to_string(),
                message: format!("Cache write failed: {e}"),
                duration_ms: start.elapsed().as_millis() as u64,
            },
        }
    }

    fn name(&self) -> &str {
        "cache"
    }
}
