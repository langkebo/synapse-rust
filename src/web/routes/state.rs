use crate::cache::{CacheManager, FederationSignatureCache, SignatureCacheConfig};
use crate::common::health::{CacheHealthCheck, DatabaseHealthCheck, HealthChecker};
use crate::common::rate_limit_config::RateLimitConfigManager;
use crate::services::{mcp_proxy::McpProxyService, ServiceContainer};
use crate::storage::ai_connection::AiConnectionStorage;
use std::sync::Arc;

#[derive(Clone)]
pub struct AppState {
    pub services: ServiceContainer,
    pub cache: Arc<CacheManager>,
    pub health_checker: Arc<HealthChecker>,
    pub federation_signature_cache: Arc<FederationSignatureCache>,
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

        Self {
            services,
            cache: cache.clone(),
            health_checker: Arc::new(health_checker),
            federation_signature_cache,
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
