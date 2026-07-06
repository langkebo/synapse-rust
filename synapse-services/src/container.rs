use crate::auth::*;
use synapse_cache::*;
use synapse_common::config::Config;
use synapse_common::metrics::MetricsCollector;

#[cfg(feature = "burn-after-read")]
use crate::worker::topology_validator::{
    current_instance_worker_type, global_maintenance_owner, should_run_global_maintenance,
};

use std::sync::Arc;
use synapse_common::server_metrics::ServerMetrics;
use synapse_common::task_queue::RedisTaskQueue;
use synapse_storage::*;

use crate::wiring;

/// Bundled shared infrastructure passed to every sub-assembler.
/// Eliminates repeated `pool, cache, config, task_queue, metrics` params.
pub struct SharedInfra {
    pub pool: Arc<sqlx::PgPool>,
    pub cache: Arc<CacheManager>,
    pub config: Config,
    pub task_queue: Option<Arc<RedisTaskQueue>>,
    pub metrics: Arc<MetricsCollector>,
}

#[derive(Clone)]
pub struct ServiceContainer {
    // Domain assemblies
    pub e2ee: wiring::E2eeServices,
    pub rooms: wiring::RoomSyncServices,
    pub federation: wiring::FederationServices,
    pub admin: wiring::AdminServices,

    // Cross-cutting service groups
    pub core: wiring::CoreServices,
    pub account: wiring::AccountServices,
    pub sso: wiring::SsoServices,
    pub extensions: wiring::ExtensionServices,
}

// =============================================================================
// Phase outputs (private — intermediate state between assembly phases)
// =============================================================================

/// Phase 1 output: shared infrastructure available to all downstream phases.
struct InfraPhase {
    infra: SharedInfra,
    server_metrics: Arc<ServerMetrics>,
    ui_auth_session_timeout: i64,
}

/// Phase 2 output: auth service + core storages needed by domain assemblies.
struct StoragePhase {
    auth_service: Arc<dyn Auth>,
    user_storage: Arc<dyn UserStore>,
    device_storage: Arc<dyn synapse_storage::device::DeviceListStoreApi>,
    threepid_storage: ThreepidStorage,
    presence_storage: Arc<dyn synapse_storage::presence::PresenceStoreApi>,
    presence_service: Arc<crate::presence_service::PresenceService>,
    qr_login_storage: Arc<dyn QrLoginStoreApi>,
    invite_blocklist_storage: Arc<dyn InviteBlocklistStoreApi>,
    sticky_event_storage: Arc<dyn StickyEventStoreApi>,
}

/// Phase 3 output: domain assemblies + media service.
struct DomainPhase {
    e2ee: wiring::E2eeServices,
    rooms: wiring::RoomSyncServices,
    admin: wiring::AdminServices,
    federation: wiring::FederationServices,
    sso: wiring::SsoServices,
    core: wiring::CoreServices,
    media_domain_service: Arc<crate::media::MediaDomainService>,
}

// =============================================================================
// ServiceContainer — phased assembly
// =============================================================================
//
// The constructor is split into 6 explicit phases to make the dependency
// graph legible and to isolate the circular-dependency workarounds (Phase 4):
//
//   Phase 1: Infrastructure     — metrics, SharedInfra bundle
//   Phase 2: Storage layer       — auth + 8 core storages
//   Phase 3: Domain assemblies   — e2ee → rooms → admin → federation → sso → core → media
//   Phase 4: Cross-domain wiring — 4 setters on RoomService (circular dependency workarounds)
//   Phase 5: Extensions + Final  — extensions, account services, container assembly
//   Phase 6: Side effects        — burn-after-read processor startup
//
// Phase 4 is the known circular-dependency seam: RoomService sits at the
// intersection of 3 dependency cycles (rooms↔admin, rooms↔core, rooms↔federation).
// The setters defer wiring until all domains are constructed, which is safe
// because CoreServices::new does not read the setter-populated fields during
// its own construction (verified by reading wiring/core.rs).

impl ServiceContainer {
    /// Returns a cloned handle to the underlying PostgreSQL connection pool.
    pub fn database_pool(&self) -> Arc<sqlx::PgPool> {
        self.account.user_storage.pool().clone()
    }

    pub async fn new(
        pool: &Arc<sqlx::PgPool>,
        cache: Arc<CacheManager>,
        config: Config,
        task_queue: Option<Arc<RedisTaskQueue>>,
    ) -> Self {
        // Phase 1: Build shared infrastructure
        let infra_phase = Self::build_infrastructure(pool, cache, config, task_queue).await;

        // Phase 2: Build auth service + core storage layer
        let storage_phase = Self::build_storage_layer(
            pool,
            &infra_phase.infra.cache,
            &infra_phase.infra.metrics,
            &infra_phase.infra.config,
        )
        .await;

        // Phase 3: Build domain assemblies (e2ee → rooms → admin → federation → sso → core → media)
        let domain_phase = Self::build_domains(&infra_phase, &storage_phase).await;

        // Phase 4: Wire cross-domain dependencies (4 setters — circular dependency workarounds)
        Self::wire_cross_domain(&domain_phase).await;

        // Phase 5: Build extensions + account services + assemble container
        let container = Self::build_container(&infra_phase, &storage_phase, domain_phase).await;

        // Phase 6: Post-construction side effects (burn-after-read processor)
        Self::start_burn_after_read_processor(&container, &infra_phase.infra.config).await;

        container
    }

    // -------------------------------------------------------------------------
    // Phase 1: Infrastructure
    // -------------------------------------------------------------------------

    async fn build_infrastructure(
        pool: &Arc<sqlx::PgPool>,
        cache: Arc<CacheManager>,
        config: Config,
        task_queue: Option<Arc<RedisTaskQueue>>,
    ) -> InfraPhase {
        let ui_auth_session_timeout = config.security.ui_auth_session_timeout;

        let metrics = Arc::new(MetricsCollector::new());
        synapse_common::error::init_error_metrics(metrics.clone());
        let server_metrics = Arc::new(ServerMetrics::new(metrics.clone()));

        let infra =
            SharedInfra { pool: pool.clone(), cache: cache.clone(), config: config.clone(), task_queue, metrics };

        InfraPhase { infra, server_metrics, ui_auth_session_timeout }
    }

    // -------------------------------------------------------------------------
    // Phase 2: Auth + Storage layer
    // -------------------------------------------------------------------------

    async fn build_storage_layer(
        pool: &Arc<sqlx::PgPool>,
        cache: &Arc<CacheManager>,
        metrics: &Arc<MetricsCollector>,
        config: &Config,
    ) -> StoragePhase {
        // Auth — must be initialized first; downstream services depend on it
        let auth_service: Arc<dyn Auth> = Arc::new(AuthService::new_with_lifetime(
            pool,
            cache.clone(),
            metrics.clone(),
            &config.security,
            &config.server.name,
            config.access_token_lifetime_seconds(),
        ));

        // Core storage
        let user_storage: Arc<dyn UserStore> = Arc::new(UserStorage::new(pool, cache.clone()));
        let device_storage: Arc<dyn synapse_storage::device::DeviceListStoreApi> = Arc::new(DeviceStorage::new(pool));
        let threepid_storage = ThreepidStorage::new(pool);
        let presence_storage: Arc<dyn synapse_storage::presence::PresenceStoreApi> =
            Arc::new(PresenceStorage::new(pool.clone(), cache.clone()));
        let presence_service = Arc::new(crate::presence_service::PresenceService::new(presence_storage.clone()));
        let qr_login_storage: Arc<dyn QrLoginStoreApi> = Arc::new(QrLoginStorage::new(pool.clone()));
        let invite_blocklist_storage: Arc<dyn InviteBlocklistStoreApi> =
            Arc::new(InviteBlocklistStorage::new(pool.clone()));
        let sticky_event_storage: Arc<dyn StickyEventStoreApi> = Arc::new(StickyEventStorage::new(pool.clone()));

        StoragePhase {
            auth_service,
            user_storage,
            device_storage,
            threepid_storage,
            presence_storage,
            presence_service,
            qr_login_storage,
            invite_blocklist_storage,
            sticky_event_storage,
        }
    }

    // -------------------------------------------------------------------------
    // Phase 3: Domain assemblies
    // -------------------------------------------------------------------------

    async fn build_domains(infra: &InfraPhase, storage: &StoragePhase) -> DomainPhase {
        let pool = &infra.infra.pool;
        let cache = &infra.infra.cache;
        let config = &infra.infra.config;

        // E2EE — needs pool, cache, user_storage, megolm key path
        let e2ee = wiring::E2eeServices::new(
            pool,
            cache,
            &storage.user_storage,
            config.server.megolm_encryption_key_path.as_deref(),
        )
        .await;

        // Rooms — needs infra, auth, presence, e2ee.to_device_storage
        let rooms = wiring::RoomSyncServices::new(
            &infra.infra,
            &storage.auth_service,
            &storage.presence_storage,
            &e2ee.to_device_storage,
        )
        .await;

        // Admin — needs pool, cache, config, task_queue, metrics, auth, user_storage
        let admin = wiring::AdminServices::new(
            pool,
            cache,
            config,
            &infra.infra.task_queue,
            &infra.infra.metrics,
            &storage.auth_service,
            &storage.user_storage,
        )
        .await;

        // Federation — needs pool, cache, config, task_queue
        let federation = wiring::FederationServices::new(pool, cache, config, &infra.infra.task_queue).await;

        // SSO — needs pool, config
        let sso = wiring::SsoServices::new(pool, config).await;

        // Core — needs infra, auth, user_storage, rooms, federation, server_metrics
        let core = wiring::CoreServices::new(
            &infra.infra,
            &storage.auth_service,
            &storage.user_storage,
            &rooms,
            &federation,
            &infra.server_metrics,
        )
        .await;

        // Media domain service — needs core.media_service + admin.media.media_quota_service
        let chunked_upload_service = Arc::new(crate::media::chunked_upload::ChunkedUploadService::new(pool.clone()));
        let media_domain_service = Arc::new({
            let svc = crate::media::MediaDomainService::new(
                core.media_service.clone(),
                admin.media.media_quota_service.clone(),
                chunked_upload_service.clone(),
            );
            let quarantine_storage = Arc::new(synapse_storage::media::QuarantinedMediaChangeStorage::new(pool));
            let cache_invalidation = cache.invalidation_manager().cloned();
            svc.with_quarantine_stream(quarantine_storage, cache_invalidation)
        });

        DomainPhase { e2ee, rooms, admin, federation, sso, core, media_domain_service }
    }

    // -------------------------------------------------------------------------
    // Phase 4: Wire cross-domain dependencies
    // -------------------------------------------------------------------------
    //
    // RoomService is at the center of 3 dependency cycles:
    //   rooms ←→ admin     (set_app_service_manager)
    //   rooms ←→ core      (set_event_broadcaster)
    //   rooms ←→ federation (set_key_rotation_manager, set_federation_client)
    //
    // These 4 setters defer wiring until all domains are constructed. Safe
    // because CoreServices::new / FederationServices::new / AdminServices::new
    // do not read these fields during their own construction — they only need
    // `&rooms` for member_storage access (see wiring/core.rs lines 83-92).
    // -------------------------------------------------------------------------

    async fn wire_cross_domain(domains: &DomainPhase) {
        domains.rooms.room_service.set_app_service_manager(domains.admin.modules.app_service_manager.clone()).await;
        domains.rooms.room_service.set_event_broadcaster(domains.core.event_broadcaster.clone()).await;
        domains
            .rooms
            .room_service
            .set_key_rotation_manager(Arc::new(domains.federation.key_rotation_manager.clone()))
            .await;
        domains.rooms.room_service.set_federation_client(domains.federation.federation_client.clone()).await;
    }

    // -------------------------------------------------------------------------
    // Phase 5: Extensions + Account + Container assembly
    // -------------------------------------------------------------------------

    async fn build_container(infra: &InfraPhase, storage: &StoragePhase, domains: DomainPhase) -> Self {
        let DomainPhase { e2ee, rooms, admin, federation, sso, core, media_domain_service } = domains;

        // Extensions — needs most domains + storage
        let extensions = wiring::ExtensionServices::new(wiring::ExtensionServicesDeps {
            infra: &infra.infra,
            rooms: &rooms,
            user_storage: &storage.user_storage,
            threepid_storage: &storage.threepid_storage,
            presence_storage: &storage.presence_storage,
            federation: &federation,
            media_service: &core.media_service,
            media_domain_service: &media_domain_service,
            ui_auth_session_timeout: infra.ui_auth_session_timeout,
        })
        .await;

        // Account identity service (cfg-gated — privacy-ext adds privacy_storage dep)
        #[cfg(feature = "privacy-ext")]
        let account_identity_service = Arc::new(crate::account_identity_service::AccountIdentityService::new(
            storage.user_storage.clone(),
            Arc::new(storage.threepid_storage.clone()),
            extensions.privacy_storage.clone(),
        ));
        #[cfg(not(feature = "privacy-ext"))]
        let account_identity_service = Arc::new(crate::account_identity_service::AccountIdentityService::new(
            storage.user_storage.clone(),
            Arc::new(storage.threepid_storage.clone()),
        ));

        let account_device_list_service =
            Arc::new(crate::account_device_list_service::AccountDeviceListService::new(storage.device_storage.clone()));

        Self {
            e2ee,
            rooms,
            federation,
            admin,
            core,
            account: wiring::AccountServices::new(wiring::AccountServicesDeps {
                pool: infra.infra.pool.clone(),
                user_storage: storage.user_storage.clone(),
                device_storage: storage.device_storage.clone(),
                threepid_storage: storage.threepid_storage.clone(),
                presence_storage: storage.presence_storage.clone(),
                presence_service: storage.presence_service.clone(),
                qr_login_storage: storage.qr_login_storage.clone(),
                invite_blocklist_storage: storage.invite_blocklist_storage.clone(),
                sticky_event_storage: storage.sticky_event_storage.clone(),
                account_device_list_service,
                account_identity_service,
            }),
            sso,
            extensions,
        }
    }

    // -------------------------------------------------------------------------
    // Phase 6: Post-construction side effects
    // -------------------------------------------------------------------------

    /// Starts the burn-after-read processor if this worker instance is
    /// designated as the global maintenance owner and the feature is enabled.
    #[cfg(feature = "burn-after-read")]
    async fn start_burn_after_read_processor(container: &Self, config: &Config) {
        let processor_cfg = config.server.enable_burn_after_read_processor;
        let run_global_maintenance = should_run_global_maintenance(&config.worker);
        let current_worker_type = current_instance_worker_type(&config.worker);
        let maintenance_owner = global_maintenance_owner(&config.worker);

        if run_global_maintenance && wiring::admin::burn_after_read_processor_enabled(processor_cfg) {
            container.extensions.burn_after_read.recover_pending_burns().await;
            container.extensions.burn_after_read.clone().start_burn_processor().await;
        } else {
            ::tracing::info!(
                worker_type = current_worker_type.as_str(),
                maintenance_owner = maintenance_owner.as_str(),
                processor_enabled = wiring::admin::burn_after_read_processor_enabled(processor_cfg),
                "Skipping burn-after-read processor startup on this worker instance"
            );
        }
    }

    #[cfg(not(feature = "burn-after-read"))]
    async fn start_burn_after_read_processor(_container: &Self, _config: &Config) {
        // No-op when burn-after-read feature is disabled.
    }

    // -------------------------------------------------------------------------
    // Accessors
    // -------------------------------------------------------------------------

    pub fn voip_service(&self) -> &Arc<crate::rtc::RtcInfraService> {
        &self.extensions.rtc_domain_service.infra
    }

    #[cfg(feature = "voip-tracking")]
    pub fn call_service(&self) -> &Arc<crate::rtc::CallOrchestrationService> {
        &self.extensions.rtc_domain_service.call
    }

    // -------------------------------------------------------------------------
    // Test constructors
    // -------------------------------------------------------------------------

    #[cfg(any(test, feature = "test-utils"))]
    pub async fn new_test() -> Self {
        let _ = synapse_common::argon2_config::Argon2Config::initialize_global_owasp(
            synapse_common::argon2_config::Argon2Config::default(),
        );
        let pool = crate::test_utils::take_prepared_test_pool().unwrap_or_else(|| {
            let db_url = std::env::var("TEST_DATABASE_URL")
                .or_else(|_| std::env::var("DATABASE_URL"))
                .unwrap_or_else(|_| crate::test_config::test_database_url());
            #[allow(clippy::expect_used)]
            Arc::new(
                sqlx::postgres::PgPoolOptions::new()
                    .max_connections(crate::test_utils::configured_test_pool_max_connections())
                    .min_connections(crate::test_utils::configured_test_pool_min_connections())
                    .acquire_timeout(crate::test_utils::configured_test_pool_acquire_timeout())
                    .idle_timeout(Some(crate::test_utils::configured_test_pool_idle_timeout()))
                    .max_lifetime(Some(crate::test_utils::configured_test_pool_max_lifetime()))
                    .connect_lazy(&db_url)
                    .expect("Failed to create test database pool"),
            )
        });
        Self::new_test_with_pool(pool).await
    }

    #[cfg(any(test, feature = "test-utils"))]
    pub async fn new_test_with_pool(pool: Arc<sqlx::PgPool>) -> Self {
        let cache = Arc::new(CacheManager::new(&CacheConfig::default()));
        let config = crate::test_config::build_test_config();
        Self::new(&pool, cache, config, None).await
    }

    #[cfg(any(test, feature = "test-utils"))]
    pub async fn new_test_with_pool_and_cache(pool: Arc<sqlx::PgPool>, cache: Arc<CacheManager>) -> Self {
        let config = crate::test_config::build_test_config();
        Self::new(&pool, cache, config, None).await
    }
}
