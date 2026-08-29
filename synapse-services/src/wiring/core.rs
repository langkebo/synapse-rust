//! Core — infra, auth, media, config, event broadcasting, push.
//!
//! ARCH-07/08 (2026-08-10): The `task_queue` field was removed because it
//! duplicated `SharedInfra.task_queue` and was never accessed via the
//! container after construction. Individual services that need the task queue
//! (e.g. `RegistrationService`, `MediaService`) receive it directly from
//! `SharedInfra` during construction and hold their own copy.

use std::sync::Arc;

use synapse_cache::CacheManager;
use synapse_common::config::Config;
use synapse_common::metrics::MetricsCollector;
use synapse_common::server_metrics::ServerMetrics;
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
    pub metrics: Arc<MetricsCollector>,
    pub server_metrics: Arc<ServerMetrics>,
    pub server_name: String,
    // `config` 必须是 Arc：RoomContext/其它 Context 的 `FromRef<AppState>` 每次请求
    // 都会执行 `state.services.core.config.clone()`。若这里是裸值 Config（30+ 子结构，
    // 大量 Vec/HashMap），每次请求深拷贝整份配置 → 高分配 churn + 操作驱动的内存累积
    // （jemalloc prof 实测 8h 净增长 ~290MB，主因即此）。改 Arc 后 clone 仅引用计数 +1。
    pub config: Arc<Config>,
    pub validator: Arc<synapse_common::validation::Validator>,
    pub key_rotation_storage: synapse_e2ee::key_rotation::KeyRotationStorage,
    pub event_broadcaster: Arc<EventBroadcaster>,
    pub event_notifier: crate::event_notifier::EventNotifier,
    pub account_data_service: Arc<crate::account_data_service::AccountDataService>,
    pub client_push_service: Arc<crate::client_push_service::ClientPushService>,
    pub user_service: Arc<UserService>,
}

impl CoreServices {
    /// 返回 `&mut Config` 供测试就地修改配置。
    ///
    /// 生产代码不应使用本方法 — 共享的 `Arc<Config>` 是为避免
    /// `FromRef<AppState>` 每请求深拷贝整份配置。仅在 `ServiceContainer::new_test`
    /// 构造的、引用计数 = 1 的单 owner 场景下使用。
    ///
    /// `Arc::get_mut` 在引用计数 > 1 时返回 None；这种情况意味着配置
    /// 已被借出给 Context/路由，**应视为测试 bug 而非运行时问题**。
    #[cfg(any(test, feature = "test-utils"))]
    #[allow(clippy::expect_used)] // 引用计数 > 1 即视为测试 bug，panic 是预期行为
    pub fn config_mut(&mut self) -> &mut Config {
        Arc::get_mut(&mut self.config).expect(
            "CoreServices::config_mut: config has other strong references. \
             This is a test bug — call config_mut before sharing the container.",
        )
    }

    #[allow(clippy::too_many_arguments)]
    pub async fn new(
        infra: &SharedInfra,
        validator: &Arc<synapse_common::validation::Validator>,
        token_auth: &Arc<dyn TokenAuth>,
        credential_auth: &Arc<dyn CredentialAuth>,
        room_auth: &Arc<dyn RoomAuth>,
        user_storage: &Arc<dyn UserStore>,
        user_service: Arc<UserService>,
        server_metrics: &Arc<ServerMetrics>,
        event_broadcaster: Arc<EventBroadcaster>,
        event_notifier: crate::event_notifier::EventNotifier,
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

        // S23: user_service is now injected from the container singleton
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
            infra.cache.clone(),
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
            metrics: infra.metrics.clone(),
            server_metrics: server_metrics.clone(),
            server_name: infra.config.server.name.clone(),
            config: Arc::new(infra.config.clone()),
            validator: validator.clone(),
            key_rotation_storage: synapse_e2ee::key_rotation::KeyRotationStorage::new(infra.pool.clone()),
            event_broadcaster,
            // Shared with the sliding-sync service so that a write on the
            // request path wakes the parked long-poll of every reader.
            event_notifier,
            account_data_service,
            client_push_service,
            user_service,
        }
    }
}
