use crate::cache::{CacheManager, FederationSignatureCache, SignatureCacheConfig};
use crate::common::health::{CacheHealthCheck, DatabaseHealthCheck, HealthChecker};
use crate::common::rate_limit_config::{RateLimitConfigFile, RateLimitConfigManager, SyncRateLimitConfigFile};
use crate::services::ServiceContainer;
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
    /// Optional graceful-shutdown signal. When set, the `POST /_synapse/admin/v1/restart`
    /// endpoint triggers it so the process manager (Docker / systemd) can restart
    /// the homeserver cleanly.
    pub shutdown_signal: Option<tokio::sync::broadcast::Sender<()>>,
    #[cfg(feature = "openclaw-routes")]
    pub ai_connection_storage: Arc<synapse_storage::ai_connection::AiConnectionStorage>,
    #[cfg(feature = "openclaw-routes")]
    pub matrix_ai_connection_service: Arc<crate::services::matrix_ai_connection_service::MatrixAiConnectionService>,
    #[cfg(feature = "openclaw-routes")]
    pub mcp_proxy_service: Arc<crate::services::mcp_proxy::McpProxyService>,
    #[cfg(feature = "openclaw-routes")]
    pub openclaw_service: Arc<crate::services::openclaw_service::OpenClawService>,
}

#[derive(Debug, Clone)]
pub struct SyncRateLimitOverride {
    pub fail_open_on_error: bool,
    pub sync: SyncRateLimitConfigFile,
}

impl AppState {
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

        // Wire federation signature cache to key rotation manager so that
        // cached signature verification results are invalidated on key rotation.
        services.federation.key_rotation_manager.set_signature_cache(federation_signature_cache.clone());

        #[cfg(feature = "openclaw-routes")]
        let canonical_cache = cache.clone();
        #[cfg(feature = "openclaw-routes")]
        let openclaw_service = {
            let openclaw_storage = Arc::new(synapse_storage::openclaw::OpenClawStorage::new(pool.clone()));
            let encryption_key = crate::services::openclaw_service::OpenClawService::resolve_encryption_key(
                services.core.config.server.macaroon_secret_key.as_deref(),
                &services.core.config.security.secret,
            );
            Arc::new(crate::services::openclaw_service::OpenClawService::new(openclaw_storage, encryption_key))
        };
        let key_fetch_max_concurrency = services.core.config.federation.key_fetch_max_concurrency.max(1);
        let key_fetch_general_max_concurrency =
            if key_fetch_max_concurrency <= 1 { 1 } else { (key_fetch_max_concurrency - 1).max(1) };
        let inbound_edu_max_concurrency = services.core.config.federation.inbound_edu_max_concurrency.max(1);
        let join_max_concurrency = services.core.config.federation.join_max_concurrency.max(1);

        Self {
            services,
            cache,
            health_checker: Arc::new(health_checker),
            federation_signature_cache,
            federation_key_fetch_priority_semaphore: Arc::new(Semaphore::new(key_fetch_max_concurrency)),
            federation_key_fetch_general_semaphore: Arc::new(Semaphore::new(key_fetch_general_max_concurrency)),
            federation_inbound_edu_semaphore: Arc::new(Semaphore::new(inbound_edu_max_concurrency)),
            federation_join_semaphore: Arc::new(Semaphore::new(join_max_concurrency)),
            federation_inbound_edu_origin_semaphores: Arc::new(Mutex::new(HashMap::new())),
            federation_presence_backoff_until: Arc::new(RwLock::new(HashMap::new())),
            rate_limit_config_manager: None,
            shutdown_signal: None,
            #[cfg(feature = "openclaw-routes")]
            ai_connection_storage: Arc::new(synapse_storage::ai_connection::AiConnectionStorage::new(pool.clone())),
            #[cfg(feature = "openclaw-routes")]
            matrix_ai_connection_service: Arc::new(
                crate::services::matrix_ai_connection_service::MatrixAiConnectionService::new(
                    Arc::new(synapse_storage::ai_connection::AiConnectionStorage::new(pool)),
                    Arc::new(crate::services::mcp_proxy::McpProxyService::new(canonical_cache.clone())),
                ),
            ),
            #[cfg(feature = "openclaw-routes")]
            mcp_proxy_service: Arc::new(crate::services::mcp_proxy::McpProxyService::new(canonical_cache)),
            #[cfg(feature = "openclaw-routes")]
            openclaw_service,
        }
    }

    pub fn with_rate_limit_config(mut self, manager: Arc<RateLimitConfigManager>) -> Self {
        self.rate_limit_config_manager = Some(manager);
        self
    }

    /// Wire the graceful-shutdown broadcast sender so admin endpoints
    /// (e.g. `POST /_synapse/admin/v1/restart`) can trigger a clean exit.
    pub fn with_shutdown_signal(mut self, shutdown_tx: tokio::sync::broadcast::Sender<()>) -> Self {
        self.shutdown_signal = Some(shutdown_tx);
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
