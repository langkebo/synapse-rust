//! Federation assembly — key rotation, federation client, device sync.

use std::sync::Arc;

use synapse_cache::CacheManager;
use synapse_common::config::Config;
use synapse_common::task_queue::RedisTaskQueue;
use synapse_e2ee::key_rotation::KeyRotationStorage;
use synapse_federation::client_api::FederationClientApi;
use synapse_federation::{DeviceSyncManager, EventAuthChain, FederationClient, KeyRotationManager};

#[derive(Clone)]
pub struct FederationServices {
    pub event_auth_chain: EventAuthChain,
    pub key_rotation_manager: KeyRotationManager,
    pub key_rotation_service: Arc<crate::federation_key_rotation_service::FederationKeyRotationService>,
    pub federation_client: Arc<dyn FederationClientApi>,
    pub device_sync_manager: DeviceSyncManager,
    pub federation_server_name: String,
}

impl FederationServices {
    pub async fn new(
        pool: &Arc<sqlx::PgPool>,
        cache: &Arc<CacheManager>,
        config: &Config,
        task_queue: &Option<Arc<RedisTaskQueue>>,
    ) -> Self {
        let event_auth_chain = EventAuthChain::new();
        let server_name = if config.federation.server_name.is_empty() {
            config.server.name.clone()
        } else {
            config.federation.server_name.clone()
        };

        let key_rotation_manager = KeyRotationManager::with_key_path_and_master_key(
            pool,
            &server_name,
            config.server.signing_key_path.clone(),
            config.federation.signing_key_master_key.as_ref().map(|k| k.as_bytes().to_vec()),
        );
        let key_rotation_service = Arc::new(crate::federation_key_rotation_service::FederationKeyRotationService::new(
            Arc::new(key_rotation_manager.clone()),
            Arc::new(KeyRotationStorage::new(pool.clone())),
        ));

        let federation_client =
            Arc::new(FederationClient::new(server_name.clone(), Arc::new(key_rotation_manager.clone())));

        let device_sync_manager = DeviceSyncManager::new(pool, Some(cache.clone()), task_queue.clone());

        Self {
            event_auth_chain,
            key_rotation_manager,
            key_rotation_service,
            federation_client,
            device_sync_manager,
            federation_server_name: server_name,
        }
    }
}
