//! Core — infra, auth, media, config, event broadcasting, push.

use std::sync::Arc;

use synapse_cache::CacheManager;
use synapse_common::config::Config;
use synapse_common::metrics::MetricsCollector;
use synapse_common::server_metrics::ServerMetrics;
use synapse_common::task_queue::RedisTaskQueue;
use synapse_federation::event_broadcaster::EventBroadcaster;
use synapse_storage::*;

use crate::auth::{CredentialAuth, RoomAuth, TokenAuth};
use crate::container::SharedInfra;
use crate::UserService;

#[derive(Clone)]
pub struct CoreServices {
    pub token_auth: Arc<dyn TokenAuth>,
    pub credential_auth: Arc<dyn CredentialAuth>,
    pub room_auth: Arc<dyn RoomAuth>,
    pub registration_service: Arc<crate::registration_service::RegistrationService>,
    pub search_service: Arc<crate::search_service::SearchService>,
    pub media_service: crate::media_service::MediaService,
    pub cache: Arc<CacheManager>,
    pub task_queue: Option<Arc<RedisTaskQueue>>,
    pub metrics: Arc<MetricsCollector>,
    pub server_metrics: Arc<ServerMetrics>,
    pub server_name: String,
    pub config: Config,
    pub validator: Arc<synapse_common::validation::Validator>,
    pub key_rotation_storage: synapse_e2ee::key_rotation::KeyRotationStorage,
    pub event_broadcaster: Arc<EventBroadcaster>,
    pub event_notifier: crate::event_notifier::EventNotifier,
    pub account_data_service: Arc<crate::account_data_service::AccountDataService>,
    pub client_push_service: Arc<crate::client_push_service::ClientPushService>,
    pub user_service: Arc<UserService>,
}

impl CoreServices {
    #[allow(clippy::too_many_arguments)]
    pub async fn new(
        infra: &SharedInfra,
        validator: &Arc<synapse_common::validation::Validator>,
        token_auth: &Arc<dyn TokenAuth>,
        credential_auth: &Arc<dyn CredentialAuth>,
        room_auth: &Arc<dyn RoomAuth>,
        user_storage: &Arc<dyn UserStore>,
        server_metrics: &Arc<ServerMetrics>,
        event_broadcaster: Arc<EventBroadcaster>,
    ) -> Self {
        let search_service = Arc::new(crate::search_service::SearchService::with_postgres(
            &infra.config.search.elasticsearch_url,
            infra.config.search.enabled,
            &infra.config.search.search_index_name,
            Some(infra.pool.as_ref().clone()),
            infra.config.search.provider.clone(),
        ));
        if infra.config.search.provider == "postgres" && infra.config.search.enabled {
            let search_service_clone = search_service.clone();
            tokio::spawn(async move {
                if let Err(e) = search_service_clone.create_fts_index().await {
                    ::tracing::warn!(
                        error = %e,
                        search_provider = %"postgres",
                        search_enabled = true,
                        "Failed to create FTS index"
                    );
                }
            });
        }

        let media_path = infra.config.server.media_path.clone();
        let media_service = crate::media_service::MediaService::with_pool(
            media_path.as_str(),
            infra.task_queue.clone(),
            &infra.config.server.name,
            Some(infra.pool.clone()),
        );

        let user_service = Arc::new(UserService::new(user_storage.clone()));

        let registration_service = Arc::new(crate::registration_service::RegistrationService::new(
            user_service.clone(),
            token_auth.clone(),
            credential_auth.clone(),
            infra.metrics.clone(),
            &infra.config.server.name,
            infra.config.server.enable_registration,
            infra.task_queue.clone(),
        ));

        let room_account_data_storage = Arc::new(RoomAccountDataStorage::new(&infra.pool));
        let account_data_storage: Arc<dyn synapse_storage::account_data::AccountDataStoreApi> =
            Arc::new(synapse_storage::account_data::AccountDataStorage::new(&infra.pool));
        let account_data_service = Arc::new(crate::account_data_service::AccountDataService::new(
            account_data_storage.clone(),
            user_storage.clone(),
            room_account_data_storage,
            Arc::new(FilterStorage::new(&infra.pool)),
            Arc::new(OpenIdTokenStorage::new(&infra.pool)),
        ));

        let push_storage: Arc<dyn synapse_storage::push::PushStoreApi> =
            Arc::new(synapse_storage::push::PushStorage::new(infra.pool.clone()));
        let client_push_service =
            Arc::new(crate::client_push_service::ClientPushService::new(account_data_storage, push_storage));

        Self {
            token_auth: token_auth.clone(),
            credential_auth: credential_auth.clone(),
            room_auth: room_auth.clone(),
            registration_service,
            search_service,
            media_service,
            cache: infra.cache.clone(),
            task_queue: infra.task_queue.clone(),
            metrics: infra.metrics.clone(),
            server_metrics: server_metrics.clone(),
            server_name: infra.config.server.name.clone(),
            config: infra.config.clone(),
            validator: validator.clone(),
            key_rotation_storage: synapse_e2ee::key_rotation::KeyRotationStorage::new(infra.pool.clone()),
            event_broadcaster,
            event_notifier: crate::event_notifier::EventNotifier::new(),
            account_data_service,
            client_push_service,
            user_service,
        }
    }
}
