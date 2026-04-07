use crate::cache::{CacheManager, FederationSignatureCache, SignatureCacheConfig};
use crate::common::health::{CacheHealthCheck, DatabaseHealthCheck, HealthChecker};
use crate::common::rate_limit_config::RateLimitConfigManager;
use crate::services::{mcp_proxy::McpProxyService, ServiceContainer};
use crate::storage::ai_connection::AiConnectionStorage;
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
    pub rate_limit_config_manager: Option<Arc<RateLimitConfigManager>>,
    pub ai_connection_storage: Arc<AiConnectionStorage>,
    pub mcp_proxy_service: Arc<McpProxyService>,
}

impl AppState {
    pub fn new(services: ServiceContainer, cache: Arc<CacheManager>) -> Self {
        let mut health_checker = HealthChecker::new("0.1.0".to_string());

        health_checker.add_check(Box::new(DatabaseHealthCheck::new(
            (*services.user_storage.pool).clone(),
        )));
        health_checker.add_check(Box::new(CacheHealthCheck::new((*cache).clone())));

        let federation_signature_cache = Arc::new(FederationSignatureCache::new(
            SignatureCacheConfig::from_federation_config(
                services.config.federation.signature_cache_ttl,
                services.config.federation.key_cache_ttl,
                services.config.federation.key_rotation_grace_period_ms,
            ),
        ));

        let pool = services.user_storage.pool.clone();
        let key_fetch_max_concurrency = services.config.federation.key_fetch_max_concurrency.max(1);
        let key_fetch_general_max_concurrency = if key_fetch_max_concurrency <= 1 {
            1
        } else {
            (key_fetch_max_concurrency - 1).max(1)
        };
        let inbound_edu_max_concurrency = services
            .config
            .federation
            .inbound_edu_max_concurrency
            .max(1);
        let join_max_concurrency = services.config.federation.join_max_concurrency.max(1);

        Self {
            services,
            cache: cache.clone(),
            health_checker: Arc::new(health_checker),
            federation_signature_cache,
            federation_key_fetch_priority_semaphore: Arc::new(Semaphore::new(
                key_fetch_max_concurrency,
            )),
            federation_key_fetch_general_semaphore: Arc::new(Semaphore::new(
                key_fetch_general_max_concurrency,
            )),
            federation_inbound_edu_semaphore: Arc::new(Semaphore::new(inbound_edu_max_concurrency)),
            federation_join_semaphore: Arc::new(Semaphore::new(join_max_concurrency)),
            federation_inbound_edu_origin_semaphores: Arc::new(Mutex::new(HashMap::new())),
            federation_presence_backoff_until: Arc::new(RwLock::new(HashMap::new())),
            rate_limit_config_manager: None,
            ai_connection_storage: Arc::new(AiConnectionStorage::new(pool)),
            mcp_proxy_service: Arc::new(McpProxyService::new(cache.clone())),
        }
    }

    pub fn with_rate_limit_config(mut self, manager: Arc<RateLimitConfigManager>) -> Self {
        self.rate_limit_config_manager = Some(manager);
        self
    }
}
