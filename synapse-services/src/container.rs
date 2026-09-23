use crate::account::UserService;
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
use synapse_federation::event_broadcaster::EventBroadcaster;
use synapse_storage::invite_blocklist::InviteBlocklistStorage;
use synapse_storage::*;

use crate::wiring;

/// Bundled shared infrastructure passed to every sub-assembler.
/// Eliminates repeated `pool, cache, config, task_queue, metrics` params.
pub struct SharedInfra {
    /// The `pool` field.
    pub pool: Arc<sqlx::PgPool>,
    /// The `cache` field.
    pub cache: Arc<CacheManager>,
    /// The `config` field.
    pub config: Config,
    /// The `task_queue` field.
    pub task_queue: Option<Arc<RedisTaskQueue>>,
    /// The `metrics` field.
    pub metrics: Arc<MetricsCollector>,
}

/// The `ServiceContainer` struct.
#[derive(Clone)]
pub struct ServiceContainer {
    // Domain assemblies
    /// The `e2ee` field.
    pub e2ee: wiring::E2eeServices,
    /// The `rooms` field.
    pub rooms: wiring::RoomSyncServices,
    /// The `federation` field.
    pub federation: wiring::FederationServices,
    /// The `admin` field.
    pub admin: wiring::AdminServices,

    // Cross-cutting service groups
    /// The `core` field.
    pub core: wiring::CoreServices,
    /// The `account` field.
    pub account: wiring::AccountServices,
    /// The `sso` field.
    pub sso: wiring::SsoServices,
    /// The `extensions` field.
    pub extensions: wiring::ExtensionServices,

    /// Cancels all background service loops on graceful shutdown.
    pub shutdown_token: tokio_util::sync::CancellationToken,
}

// =============================================================================
// Phase outputs (private — intermediate state between assembly phases)
// =============================================================================

/// Phase 1 output: shared infrastructure available to all downstream phases.
struct InfraPhase {
    infra: SharedInfra,
    server_metrics: Arc<ServerMetrics>,
    ui_auth_session_timeout: i64,
    /// Created here so it can be threaded into background loops (e.g. the AS
    /// scheduler) started during Phase 3, and shared with the container field.
    shutdown_token: tokio_util::sync::CancellationToken,
}

/// Phase 2 output: auth service + core storages needed by domain assemblies.
struct StoragePhase {
    validator: Arc<synapse_common::validation::Validator>,
    token_auth: Arc<dyn TokenAuth>,
    credential_auth: Arc<dyn CredentialAuth>,
    room_auth: Arc<dyn RoomAuth>,
    user_storage: Arc<dyn UserStore>,
    user_service: Arc<UserService>,
    /// MSC4204: Member storage for profile update notification.
    member_storage: Arc<dyn synapse_storage::membership::MemberStoreApi>,
    device_storage: Arc<dyn synapse_storage::device::DeviceListStoreApi>,
    threepid_storage: Arc<dyn ThreepidStoreApi>,
    presence_storage: Arc<dyn synapse_storage::presence::PresenceStoreApi>,
    presence_service: Arc<crate::presence_service::PresenceService>,
    qr_login_storage: Arc<synapse_storage::qr_login::QrLoginStorage>,
    /// Built here, not in the account phase, because the rooms phase (which is
    /// assembled first) needs the same `Arc` as its invite enforcement gate.
    /// One instance, so the admin routes and the enforcement path cannot read
    /// different list state.
    invite_blocklist_service: Arc<crate::invite_blocklist_service::InviteBlocklistService>,
    sticky_event_storage: Arc<synapse_storage::sticky_event::StickyEventStorage>,
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
    /// T04: Event broadcaster for federation outbound EDU broadcast.
    event_broadcaster: Arc<synapse_federation::event_broadcaster::EventBroadcaster>,
}

// =============================================================================
// ServiceContainer — phased assembly
// =============================================================================
//
// The constructor is split into explicit phases to make the dependency
// graph legible:
//
//   Phase 1: Infrastructure     — metrics, SharedInfra bundle
//   Phase 2: Storage layer       — auth + 8 core storages
//   Phase 3: Domain assemblies   — linearized DAG:
//              e2ee → admin → federation → member_storage → event_broadcaster
//              → rooms → sso → core → media
//   Phase 4: Extensions + Final  — extensions, account services, container assembly
//   Phase 5: Side effects        — burn-after-read processor startup
//
// The 4 services RoomService depends on (EventBroadcaster,
// ApplicationServiceManager, KeyRotationManager, FederationClient) are built
// before RoomService and injected directly through RoomServiceConfig. There is
// no post-construction wiring: the dependency graph is a DAG, so linear
// construction suffices.

impl ServiceContainer {
    /// Returns a cloned handle to the underlying PostgreSQL connection pool.
    pub fn database_pool(&self) -> Arc<sqlx::PgPool> {
        self.account.user_storage.pool().clone()
    }

    /// Assemble the whole service graph.
    ///
    /// Fallible because a server that cannot derive a server-side megolm at-rest
    /// key must fail startup with the operator-facing message rather than panic
    /// deep inside the wiring (sweep, follow-up doc §1.6).
    pub async fn new(
        pool: &Arc<sqlx::PgPool>,
        cache: Arc<CacheManager>,
        config: Config,
        task_queue: Option<Arc<RedisTaskQueue>>,
    ) -> Result<Self, String> {
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

        // Phase 3: Build domain assemblies (linearized DAG)
        let domain_phase = Self::build_domains(&infra_phase, &storage_phase).await?;

        // Phase 4: Build extensions + account services + assemble container
        let container = Self::build_container(&infra_phase, &storage_phase, domain_phase).await;

        // Phase 5: Post-construction side effects (burn-after-read processor)
        Self::start_burn_after_read_processor(&container, &infra_phase.infra.config).await;

        Ok(container)
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
        // Publish the handle for the two code paths that cannot take a container
        // dependency: the `sqlx::query` tracing layer (db_query_duration_ms) and
        // `From<sqlx::Error> for ApiError` (db_query_errors).
        synapse_common::server_metrics::install_global_server_metrics(server_metrics.clone());

        let infra =
            SharedInfra { pool: pool.clone(), cache: cache.clone(), config: config.clone(), task_queue, metrics };

        let shutdown_token = tokio_util::sync::CancellationToken::new();

        InfraPhase { infra, server_metrics, ui_auth_session_timeout, shutdown_token }
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
        // S23: Create UserStorage + UserService once, share with AuthService.
        // Previously AuthService::new_with_lifetime() created its own UserStorage
        // and UserService internally, bypassing DI and producing duplicate instances.
        let user_storage: Arc<dyn UserStore> = Arc::new(UserStorage::new(pool, cache.clone()));
        let user_service = Arc::new(UserService::new(user_storage.clone()));

        // MSC4204: Create member_storage early so it can be injected into user_service.
        // This enables profile updates to notify shared room users via sliding sync.
        let server_name_for_storage = config.server.get_server_name().to_string();
        let member_storage: Arc<dyn synapse_storage::membership::MemberStoreApi> =
            Arc::new(RoomMemberStorage::new(pool, &server_name_for_storage));

        // Inject member_storage into user_service for profile update notifications
        user_service.set_member_storage(member_storage.clone());

        // ARCH-01: Create the 3 critical writable storages once and inject them
        // into AuthService. Previously new_with_lifetime() created these
        // internally via Arc::new(...Storage::new(pool)), producing duplicate
        // instances that bypassed the ServiceContainer's shared instances.
        let device_storage: Arc<dyn synapse_storage::device::DeviceListStoreApi> = Arc::new(DeviceStorage::new(pool));
        let token_storage: Arc<dyn AccessTokenStoreApi> = Arc::new(AccessTokenStorage::new(pool));
        let refresh_token_storage: Arc<dyn synapse_storage::refresh_token::RefreshTokenStoreApi> =
            Arc::new(synapse_storage::refresh_token::RefreshTokenStorage::new(pool));

        // Auth — must be initialized first; downstream services depend on it.
        // Produce all four trait-object lenses from the same concrete AuthService
        // so consumers can depend on the narrowest trait they need.
        //
        // C1: Wire audit_storage so login/password/lockout events are persisted
        // to the tamper-evident audit_events table.
        let audit_storage: std::sync::Arc<dyn synapse_storage::audit::AuditEventStoreApi> =
            std::sync::Arc::new(synapse_storage::audit::AuditEventStorage::new(pool));
        let auth_concrete: std::sync::Arc<AuthService> = std::sync::Arc::new(
            AuthService::new_with_lifetime(
                pool,
                cache.clone(),
                metrics.clone(),
                &config.security,
                &config.server.name,
                config.access_token_lifetime_seconds(),
                user_service.clone(),
                user_storage.clone(),
                device_storage.clone(),
                token_storage.clone(),
                refresh_token_storage.clone(),
            )
            .with_audit_storage(audit_storage),
        );
        let token_auth: Arc<dyn TokenAuth> = auth_concrete.clone();
        let credential_auth: Arc<dyn CredentialAuth> = auth_concrete.clone();
        let room_auth: Arc<dyn RoomAuth> = auth_concrete.clone();

        // Core storage (user_storage and user_service already created above for S23 DI)
        // device_storage already created above for ARCH-01 DI sharing.
        let threepid_storage: Arc<dyn ThreepidStoreApi> = Arc::new(ThreepidStorage::new(pool));
        let presence_storage: Arc<dyn synapse_storage::presence::PresenceStoreApi> =
            Arc::new(PresenceStorage::new(pool.clone(), cache.clone()));
        let presence_tuning = crate::presence_service::PresenceTuning {
            excluded_rooms: config.server.exclude_rooms_from_presence.clone(),
            last_active_granularity: config.server.last_active_granularity,
            sync_online_timeout: config.server.sync_online_timeout,
            idle_timeout: config.server.idle_timeout,
        };
        let presence_service =
            Arc::new(crate::presence_service::PresenceService::with_tuning(presence_storage.clone(), presence_tuning));
        let qr_login_storage: Arc<synapse_storage::qr_login::QrLoginStorage> =
            Arc::new(QrLoginStorage::new(pool.clone()));
        let invite_blocklist_storage: Arc<synapse_storage::invite_blocklist::InviteBlocklistStorage> =
            Arc::new(InviteBlocklistStorage::new(pool.clone()));
        // Constructed in this phase rather than the account phase because the
        // rooms phase consumes the same `Arc` as its invite enforcement gate.
        // The MSC4155 policy is account data, so the gate reads it through the
        // account-data storage rather than the user store.
        let invite_blocklist_service = Arc::new(crate::invite_blocklist_service::InviteBlocklistService::new(
            invite_blocklist_storage,
            Arc::new(synapse_storage::account_data::AccountDataStorage::new(pool)),
        ));
        let sticky_event_storage: Arc<synapse_storage::sticky_event::StickyEventStorage> =
            Arc::new(StickyEventStorage::new(pool.clone()));

        // user_service already created above (S23 DI sharing)

        StoragePhase {
            validator: auth_concrete.validator.clone(),
            token_auth,
            credential_auth,
            room_auth,
            user_storage,
            user_service,
            member_storage,
            device_storage,
            threepid_storage,
            presence_storage,
            presence_service,
            qr_login_storage,
            invite_blocklist_service,
            sticky_event_storage,
        }
    }

    // -------------------------------------------------------------------------
    // Phase 3: Domain assemblies
    // -------------------------------------------------------------------------

    /// Build the domain assemblies.
    ///
    /// Fallible only for the E2EE wiring, which refuses to start when no
    /// server-side megolm at-rest key is derivable (see
    /// [`wiring::E2eeServices::new`]). The message is the operator-facing one.
    async fn build_domains(infra: &InfraPhase, storage: &StoragePhase) -> Result<DomainPhase, String> {
        let pool = &infra.infra.pool;
        let cache = &infra.infra.cache;
        let config = &infra.infra.config;

        // E2EE — needs pool, cache, user_storage, megolm key path
        let e2ee = wiring::E2eeServices::new(
            pool,
            cache,
            &storage.user_storage,
            config.server.megolm_encryption_key_path.as_deref(),
            config.server.macaroon_secret_key.as_deref(),
        )
        .await?;

        // Admin — builds app_service_manager; no rooms/federation/core dependency
        let admin = wiring::AdminServices::new(
            pool,
            cache,
            config,
            &infra.infra.task_queue,
            &infra.infra.metrics,
            &infra.server_metrics,
            &storage.token_auth,
            &storage.credential_auth,
            &storage.room_auth,
            &storage.user_storage,
            storage.user_service.clone(),
            &infra.shutdown_token,
        )
        .await;

        // Federation — builds key_rotation_manager + federation_client; no rooms dependency
        let federation = wiring::FederationServices::new(pool, cache, config, &infra.infra.task_queue).await;

        // Reuse member_storage created in build_storage_layer (for MSC4204 profile notifications)
        let member_storage = storage.member_storage.clone();
        let server_name_for_storage = config.server.get_server_name().to_string();

        // EventBroadcaster — needs federation.federation_client + member_storage
        let event_broadcaster = {
            let broadcaster = EventBroadcaster::new(server_name_for_storage.clone())
                .with_client(federation.federation_client.clone())
                .with_pool(pool.as_ref().clone())
                .with_membership_storage(member_storage.clone());
            broadcaster
                .start_batch_sender(server_name_for_storage, config.federation.event_broadcast_batch_size, 100)
                .await;
            Arc::new(broadcaster)
        };

        // Sync wake-up bus. Built here rather than inside a single wiring
        // module because both sides of the long-poll need the *same* instance:
        // `RoomSyncServices` gives it to the sliding-sync service (the waiter)
        // and `CoreServices` re-exports it to the route layer (the notifier).
        // Two separate instances would silently never wake each other.
        //
        // S8: When Redis is enabled, wire cross-instance fan-out so that
        // notifications from other server instances wake local waiters.
        let event_notifier = if config.redis.enabled {
            let redis_url = config.redis_url();
            let redis_cfg = deadpool_redis::Config::from_url(&redis_url);
            match redis_cfg.create_pool(Some(deadpool_redis::Runtime::Tokio1)) {
                Ok(pool) => {
                    // S-6: Wire the Prometheus counter for subscriber failures
                    let subscriber_failure_counter =
                        infra.server_metrics.event_notifier_subscriber_failures_total.clone();
                    let notifier = crate::event_notifier::EventNotifier::new()
                        .with_redis(pool, redis_url)
                        .with_metrics(subscriber_failure_counter)
                        .with_idle_timeout_secs(config.server.event_notifier_idle_timeout_secs);
                    if let Err(e) = notifier.start_redis_subscriber(infra.shutdown_token.clone()) {
                        // S6: subscriber failure is fatal — cross-instance fan-out
                        // cannot be silently disabled because it breaks session
                        // consistency across instances. In production, the operator
                        // must fix the Redis issue and restart.
                        ::tracing::error!(
                            error = %e,
                            "Failed to start EventNotifier Redis subscriber. Cross-instance fan-out is DISABLED. "
                        );
                        // In dev/test mode, continue with local-only notifications.
                        // In production, the operator should see the error and fix Redis.
                        notifier
                    } else {
                        notifier
                    }
                }
                Err(e) => {
                    ::tracing::warn!(
                        error = %e,
                        "Failed to create Redis pool for EventNotifier. Falling back to local-only notifications."
                    );
                    crate::event_notifier::EventNotifier::new()
                        .with_idle_timeout_secs(config.server.event_notifier_idle_timeout_secs)
                }
            }
        } else {
            crate::event_notifier::EventNotifier::new()
                .with_idle_timeout_secs(config.server.event_notifier_idle_timeout_secs)
        };

        // A-7: reclaim notifier slots whose waiters have gone away, so the
        // room/user maps don't grow monotonically over the process lifetime.
        //
        // 300 s = 5 min sweep cadence：evictor 只做 `Arc::strong_count > 1`
        // 的 retain（不碰正在长轮询的 waiter），远小于 idle_timeout_secs（默认
        // 5 s）的量级——所以不干扰任何通知时延； idle 阈值本身已由
        // `with_idle_timeout_secs` 从 config 读入，此处不需要再配置化。
        event_notifier.start_idle_slot_evictor(std::time::Duration::from_secs(300), infra.shutdown_token.clone());

        // MSC4204: Inject the real event_notifier into UserService so that
        // profile_update notifications can use Redis cross-instance fan-out.
        storage.user_service.set_event_notifier(event_notifier.clone());

        // Rooms — receives member_storage + the 4 injected services directly
        // B-4204: user_storage is needed for profile_updates extension
        let rooms = wiring::RoomSyncServices::new(
            &infra.infra,
            &storage.room_auth,
            &storage.validator,
            &storage.presence_storage,
            &e2ee.to_device_storage,
            member_storage.clone(),
            storage.user_storage.clone(),
            event_broadcaster.clone(),
            admin.modules.app_service_manager.clone(),
            Arc::new(federation.key_rotation_manager.clone()),
            federation.federation_client.clone(),
            storage.sticky_event_storage.clone(),
            event_notifier.clone(),
            // MSC4284: inject policy service for room create/join/invite enforcement.
            Some(admin.modules.policy_service.clone()),
            // Invite policy gate — same instance the account phase exposes to
            // the admin blocklist routes.
            storage.invite_blocklist_service.clone(),
        )
        .await;

        // SSO — needs pool, config
        let sso = wiring::SsoServices::new(pool, config).await;

        // Core — needs infra, auth, user_storage, server_metrics + the pre-built broadcaster
        let core = wiring::CoreServices::new(
            &infra.infra,
            &storage.validator,
            &storage.token_auth,
            &storage.credential_auth,
            &storage.room_auth,
            &storage.user_storage,
            storage.user_service.clone(),
            &infra.server_metrics,
            event_broadcaster.clone(),
            event_notifier,
        )
        .await;

        // Media domain service — needs core.media_service + admin.media.media_quota_service
        // G-1: 分块上传的整文件上限统一读权威配置 server.max_upload_size
        let chunked_upload_service = Arc::new(crate::media::chunked_upload::ChunkedUploadService::new(
            pool.clone(),
            config.server.max_upload_size as usize,
        ));
        let media_domain_service = Arc::new({
            let svc = crate::media::MediaDomainService::new(
                core.media_service.clone(),
                admin.media.media_quota_service.clone(),
                chunked_upload_service.clone(),
            );
            let quarantine_storage: Arc<dyn synapse_storage::media::QuarantinedMediaChangeStoreApi> =
                Arc::new(synapse_storage::media::QuarantinedMediaChangeStorage::new(pool));
            let cache_invalidation = cache.invalidation_manager().cloned();
            svc.with_quarantine_stream(quarantine_storage, cache_invalidation)
        });

        Ok(DomainPhase { e2ee, rooms, admin, federation, sso, core, media_domain_service, event_broadcaster })
    }

    // -------------------------------------------------------------------------
    // Phase 4: Extensions + Account + Container assembly
    // -------------------------------------------------------------------------

    async fn build_container(infra: &InfraPhase, storage: &StoragePhase, domains: DomainPhase) -> Self {
        let DomainPhase { e2ee, rooms, admin, federation, sso, core, media_domain_service, event_broadcaster } =
            domains;

        // T04: Wire federation event broadcaster into presence_service for outbound presence EDU broadcast
        storage
            .presence_service
            .set_event_broadcaster(event_broadcaster.clone(), infra.infra.config.server.get_server_name().to_string());

        // MSC4262: Wire federation event broadcaster into user_service for outbound
        // profile_update EDU broadcast on local profile changes.
        storage.user_service.set_federation_broadcaster(
            event_broadcaster.clone(),
            infra.infra.config.server.get_server_name().to_string(),
        );

        // Extensions — needs most domains + storage
        let extensions = wiring::ExtensionServices::new(wiring::ExtensionServicesDeps {
            infra: &infra.infra,
            rooms: &rooms,
            user_storage: &storage.user_storage,
            threepid_storage: storage.threepid_storage.clone(),
            presence_storage: &storage.presence_storage,
            federation: &federation,
            media_service: &core.media_service,
            media_domain_service: &media_domain_service,
            ui_auth_session_timeout: infra.ui_auth_session_timeout,
            user_service: storage.user_service.clone(),
        })
        .await;

        // Account identity service (cfg-gated — privacy-ext adds privacy_storage dep)
        #[cfg(feature = "privacy-ext")]
        let account_identity_service = Arc::new(crate::account_identity_service::AccountIdentityService::new(
            storage.user_service.clone(),
            storage.threepid_storage.clone(),
            extensions.privacy_storage.clone(),
        ));
        #[cfg(not(feature = "privacy-ext"))]
        let account_identity_service = Arc::new(crate::account_identity_service::AccountIdentityService::new(
            storage.user_service.clone(),
            storage.threepid_storage.clone(),
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
                invite_blocklist_service: storage.invite_blocklist_service.clone(),
                sticky_event_storage: storage.sticky_event_storage.clone(),
                account_device_list_service,
                account_identity_service,
                user_service: storage.user_service.clone(),
            }),
            sso,
            extensions,
            shutdown_token: infra.shutdown_token.clone(),
        }
    }

    // -------------------------------------------------------------------------
    // Phase 5: Post-construction side effects
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
            let _ = container
                .extensions
                .burn_after_read
                .clone()
                .start_burn_processor(container.shutdown_token.clone())
                .await;
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

    /// See [`voip_service`].
    pub fn voip_service(&self) -> &Arc<crate::rtc::RtcInfraService> {
        &self.extensions.rtc_domain_service.infra
    }

    /// See [`call_service`].
    #[cfg(feature = "voip-tracking")]
    pub fn call_service(&self) -> &Arc<crate::rtc::CallOrchestrationService> {
        &self.extensions.rtc_domain_service.call
    }

    // -------------------------------------------------------------------------
    // Test constructors
    // -------------------------------------------------------------------------

    /// See [`new_test`].
    #[cfg(any(test, feature = "test-utils"))]
    pub async fn new_test() -> Self {
        let _ = synapse_common::argon2_config::Argon2Config::initialize_global_owasp(
            synapse_common::argon2_config::Argon2Config::default(),
        );
        // 这里曾有一个"预制备池队列"（`take_prepared_test_pool().unwrap_or_else(…)`），
        // 但那个队列**从来没有被填过**：`enqueue_prepared_test_pool` 在全仓没有调用者，
        // 因此它恒返回 `None`、只是把创建逻辑包了一层（2026-09-21 审计确认后删除，
        // 同一份死代码在 synapse-services / synapse-storage / synapse-test-utils 三处）。
        let pool = {
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
        };
        Self::new_test_with_pool(pool).await
    }

    /// See [`new_test_with_pool`].
    #[cfg(any(test, feature = "test-utils"))]
    #[allow(clippy::expect_used)]
    pub async fn new_test_with_pool(pool: Arc<sqlx::PgPool>) -> Self {
        let cache = Arc::new(CacheManager::new(&CacheConfig::default()));
        let config = crate::test_config::build_test_config();
        // Test helper: a fixture whose config cannot produce a container is a test
        // bug, so failing loudly here is correct (the real `new` returns `Result`).
        Self::new(&pool, cache, config, None).await.expect("test service container")
    }

    /// See [`new_test_with_pool_and_cache`].
    #[cfg(any(test, feature = "test-utils"))]
    #[allow(clippy::expect_used)]
    pub async fn new_test_with_pool_and_cache(pool: Arc<sqlx::PgPool>, cache: Arc<CacheManager>) -> Self {
        let config = crate::test_config::build_test_config();
        Self::new(&pool, cache, config, None).await.expect("test service container")
    }
}
