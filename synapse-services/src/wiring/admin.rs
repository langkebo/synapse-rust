//! Admin assembly — decomposed into 5 domain sub-structs.

use std::sync::Arc;

use synapse_cache::CacheManager;
use synapse_common::config::Config;
use synapse_common::metrics::MetricsCollector;
use synapse_common::task_queue::RedisTaskQueue;
use synapse_storage::*;

use crate::account::UserService;
use crate::auth::{CredentialAuth, RoomAuth, TokenAuth};
use crate::worker::topology_validator::{
    current_instance_worker_type, global_maintenance_owner, should_run_global_maintenance,
};
use synapse_storage::email_verification::EmailVerificationStorage;

/// The `AdminUserServices` struct.
#[derive(Clone)]
pub struct AdminUserServices {
    /// The `admin_registration_service` field.
    pub admin_registration_service: crate::admin_registration_service::AdminRegistrationService,
    /// The `admin_user_service` field.
    pub admin_user_service: Arc<crate::admin_user_service::AdminUserService>,
    /// The `email_verification_service` field.
    pub email_verification_service: Arc<crate::email_verification_service::EmailVerificationService>,
    /// The `admin_token_service` field.
    pub admin_token_service: Arc<crate::admin_token_service::AdminTokenService>,
    /// The `refresh_token_storage` field.
    pub refresh_token_storage: Arc<dyn synapse_storage::refresh_token::RefreshTokenStoreApi>,
    /// The `refresh_token_service` field.
    pub refresh_token_service: Arc<crate::refresh_token_service::RefreshTokenService>,
    /// The `registration_token_storage` field.
    pub registration_token_storage: Arc<synapse_storage::registration_token::RegistrationTokenStorage>,
    /// The `registration_token_service` field.
    pub registration_token_service: Arc<crate::registration_token_service::RegistrationTokenService>,
}

/// The `AdminFederationServices` struct.
#[derive(Clone)]
pub struct AdminFederationServices {
    /// The `admin_federation_service` field.
    pub admin_federation_service: Arc<crate::admin_federation_service::AdminFederationService>,
    /// The `federation_blacklist_storage` field.
    pub federation_blacklist_storage: Arc<synapse_storage::federation_blacklist::FederationBlacklistStorage>,
    /// The `federation_blacklist_service` field.
    pub federation_blacklist_service: Arc<crate::federation_blacklist_service::FederationBlacklistService>,
}

/// The `AdminMediaServices` struct.
#[derive(Clone)]
pub struct AdminMediaServices {
    /// The `admin_media_service` field.
    pub admin_media_service: Arc<crate::admin_media_service::AdminMediaService>,
    /// The `media_quota_storage` field.
    pub media_quota_storage: Arc<synapse_storage::media_quota::MediaQuotaStorage>,
    /// The `media_quota_service` field.
    pub media_quota_service: Arc<crate::media_quota_service::MediaQuotaService>,
}

/// The `AdminSecurityServices` struct.
#[derive(Clone)]
pub struct AdminSecurityServices {
    /// The `admin_security_service` field.
    pub admin_security_service: Arc<crate::admin_security_service::AdminSecurityService>,
    /// The `captcha_storage` field.
    pub captcha_storage: Arc<synapse_storage::captcha::CaptchaStorage>,
    /// The `captcha_service` field.
    pub captcha_service: Arc<crate::captcha_service::CaptchaService>,
    /// The `audit_storage` field.
    pub audit_storage: Arc<dyn synapse_storage::audit::AuditEventStoreApi>,
    /// The `admin_audit_service` field.
    pub admin_audit_service: Arc<crate::admin_audit_service::AdminAuditService>,
    /// The `admin_server_service` field.
    pub admin_server_service: Arc<crate::admin_server_service::AdminServerService>,
    /// The `telemetry_alert_service` field.
    pub telemetry_alert_service: Arc<crate::telemetry_service::TelemetryAlertService>,
}

/// The `AdminModuleServices` struct.
#[derive(Clone)]
pub struct AdminModuleServices {
    /// The `feature_flag_storage` field.
    pub feature_flag_storage: Arc<synapse_storage::feature_flags::FeatureFlagStorage>,
    /// The `feature_flag_service` field.
    pub feature_flag_service: Arc<crate::feature_flag_service::FeatureFlagService>,
    /// The `event_report_storage` field.
    pub event_report_storage: Arc<synapse_storage::event_report::EventReportStorage>,
    /// The `event_report_service` field.
    pub event_report_service: Arc<crate::event_report_service::EventReportService>,
    /// MSC4140 — Cancellable delayed events storage.
    pub delayed_event_storage: Arc<dyn synapse_storage::delayed_events::DelayedEventStorageApi>,
    /// MSC4140 — Cancellable delayed events service.
    pub delayed_event_service: Arc<crate::delayed_event_service::DelayedEventService>,
    /// The `e2ee_audit_service` field.
    pub e2ee_audit_service: Arc<crate::e2ee_audit::E2eeAuditService>,
    /// The `background_update_storage` field.
    pub background_update_storage: Arc<dyn synapse_storage::background_update::BackgroundUpdateStoreApi>,
    /// The `background_update_service` field.
    pub background_update_service: Arc<crate::background_update_service::BackgroundUpdateService>,
    /// The `module_storage` field.
    pub module_storage: Arc<synapse_storage::module::ModuleStorage>,
    /// The `module_service` field.
    pub module_service: Arc<crate::module_service::ModuleService>,
    /// The `account_validity_service` field.
    pub account_validity_service: Arc<crate::module_service::AccountValidityService>,
    /// The `retention_storage` field.
    pub retention_storage: Arc<synapse_storage::retention::RetentionStorage>,
    /// The `retention_service` field.
    pub retention_service: Arc<crate::retention_service::RetentionService>,
    /// The `push_notification_storage` field.
    pub push_notification_storage: Arc<synapse_storage::push_notification::PushNotificationStorage>,
    /// The `push_notification_service` field.
    pub push_notification_service: Arc<crate::push_notification_service::PushNotificationService>,
    /// The `app_service_storage` field.
    pub app_service_storage: Arc<synapse_storage::application_service::ApplicationServiceStorage>,
    /// The `app_service_event_reader` field.
    pub app_service_event_reader: Arc<dyn synapse_storage::event::EventReader>,
    /// The `app_service_manager` field.
    pub app_service_manager: Arc<crate::application_service::ApplicationServiceManager>,
    /// The `app_service_scheduler` field.
    pub app_service_scheduler: Arc<crate::application_service::ApplicationServiceScheduler>,
    /// MSC4284 — Policy server service for room/user/content moderation.
    pub policy_service: Arc<crate::policy_service::PolicyService>,
    #[cfg(feature = "external-services")]
    /// The `external_service_integration` field.
    pub external_service_integration: Arc<crate::external_service_integration::ExternalServiceIntegration>,
    /// The `rendezvous_service` field.
    pub rendezvous_service: Arc<crate::rendezvous_service::RendezvousService>,
    /// The `login_token_storage` field.
    pub login_token_storage: Arc<dyn synapse_storage::login_token::LoginTokenStoreApi>,
    /// The `login_token_service` field.
    pub login_token_service: Arc<crate::login_token_service::LoginTokenService>,
    /// The `worker_storage` field.
    pub worker_storage: Arc<dyn synapse_storage::worker::WorkerStoreApi>,
    /// The `worker_manager` field.
    pub worker_manager: Arc<crate::worker::WorkerManager>,
}

/// Aggregate admin services, decomposed into 5 domain sub-structs.
///
/// ARCH-07/08 (2026-08-10): The top-level `user_service` field was removed
/// because it duplicated `ServiceContainer.account.user_service` and was never
/// accessed via the container. The `user_service` parameter in [`new`] is
/// still required — it is injected into individual admin sub-services during
/// construction.
///
/// Several `*_storage` fields in the sub-structs below are "backing storage":
/// they are constructed and stored here so that the corresponding `*_service`
/// field (which wraps the storage via its own `Arc`) remains alive for the
/// container's lifetime. The storage copies themselves are not accessed via
/// the container after construction, but are retained for potential direct
/// access by future route handlers. Verified unused storage fields include:
/// `registration_token_storage`, `captcha_storage`, `audit_storage`,
/// `feature_flag_storage`, `event_report_storage`, `background_update_storage`,
/// `retention_storage`, `push_notification_storage`, `app_service_storage`,
/// `app_service_event_reader`, `worker_storage`, `refresh_token_storage`,
/// `federation_blacklist_storage`, and `media_quota_storage`.
#[derive(Clone)]
pub struct AdminServices {
    /// The `user` field.
    pub user: AdminUserServices,
    /// The `federation` field.
    pub federation: AdminFederationServices,
    /// The `media` field.
    pub media: AdminMediaServices,
    /// The `security` field.
    pub security: AdminSecurityServices,
    /// The `modules` field.
    pub modules: AdminModuleServices,
}

impl AdminServices {
    /// See [`new`].
    #[allow(clippy::too_many_arguments)]
    pub async fn new(
        pool: &Arc<sqlx::PgPool>,
        cache: &Arc<CacheManager>,
        config: &Config,
        task_queue: &Option<Arc<RedisTaskQueue>>,
        metrics: &Arc<MetricsCollector>,
        server_metrics: &Arc<synapse_common::server_metrics::ServerMetrics>,
        token_auth: &Arc<dyn TokenAuth>,
        credential_auth: &Arc<dyn CredentialAuth>,
        _room_auth: &Arc<dyn RoomAuth>,
        user_storage: &Arc<dyn UserStore>,
        user_service: Arc<UserService>,
        shutdown_token: &tokio_util::sync::CancellationToken,
    ) -> Self {
        let admin_registration_service = crate::admin_registration_service::AdminRegistrationService::new(
            token_auth.clone(),
            credential_auth.clone(),
            config.server.name.clone(),
            config.admin_registration.clone(),
            user_storage.clone(),
            user_service.clone(),
            cache.clone(),
            metrics.clone(),
        );

        let email_verification_storage: Arc<synapse_storage::email_verification::EmailVerificationStorage> =
            Arc::new(EmailVerificationStorage::new(pool));
        let email_verification_service =
            Arc::new(crate::email_verification_service::EmailVerificationService::new(email_verification_storage));
        let audit_storage: Arc<dyn synapse_storage::audit::AuditEventStoreApi> =
            Arc::new(synapse_storage::audit::AuditEventStorage::new(pool));
        let admin_audit_service = Arc::new(crate::admin_audit_service::AdminAuditService::new(audit_storage.clone()));

        let feature_flag_storage: Arc<synapse_storage::feature_flags::FeatureFlagStorage> =
            Arc::new(synapse_storage::feature_flags::FeatureFlagStorage::new(pool, cache.clone()));
        let feature_flag_service = Arc::new(crate::feature_flag_service::FeatureFlagService::new(
            feature_flag_storage.clone(),
            admin_audit_service.clone(),
        ));

        let e2ee_audit_service = Arc::new(crate::e2ee_audit::E2eeAuditService::new(pool.clone()));
        let event_report_storage: Arc<synapse_storage::event_report::EventReportStorage> =
            Arc::new(synapse_storage::event_report::EventReportStorage::new(pool));
        let event_report_service =
            Arc::new(crate::event_report_service::EventReportService::new(event_report_storage.clone()));

        // MSC4140 — Cancellable delayed events storage.
        let delayed_event_storage: Arc<dyn synapse_storage::delayed_events::DelayedEventStorageApi> =
            Arc::new(synapse_storage::delayed_events::DelayedEventStorage::new(pool.clone()));
        let delayed_event_service =
            Arc::new(crate::delayed_event_service::DelayedEventService::new(delayed_event_storage.clone()));
        let background_update_storage: Arc<dyn synapse_storage::background_update::BackgroundUpdateStoreApi> =
            Arc::new(synapse_storage::background_update::BackgroundUpdateStorage::new(pool));
        let background_update_service = Arc::new(
            crate::background_update_service::BackgroundUpdateService::new(background_update_storage.clone())
                .with_lock_retry_config(config.worker.lock_max_retries, config.worker.lock_max_retry_interval_ms),
        );

        let module_storage: Arc<synapse_storage::module::ModuleStorage> =
            Arc::new(synapse_storage::module::ModuleStorage::new(pool));
        let module_service = Arc::new(crate::module_service::ModuleService::new(module_storage.clone()));
        let account_validity_service =
            Arc::new(crate::module_service::AccountValidityService::new(module_storage.clone()));

        let retention_storage: Arc<synapse_storage::retention::RetentionStorage> =
            Arc::new(synapse_storage::retention::RetentionStorage::new(pool));
        let chunked_upload_storage: Arc<synapse_storage::media::chunked_upload::ChunkedUploadStorage> =
            Arc::new(synapse_storage::media::ChunkedUploadStorage::new(pool));
        let retention_service = Arc::new(crate::retention_service::RetentionService::new(
            retention_storage.clone(),
            chunked_upload_storage.clone(),
            metrics,
            audit_storage.clone(),
        ));

        let refresh_token_storage: Arc<dyn synapse_storage::refresh_token::RefreshTokenStoreApi> =
            Arc::new(synapse_storage::refresh_token::RefreshTokenStorage::new(pool));
        let refresh_token_service = Arc::new(crate::refresh_token_service::RefreshTokenService::new(
            refresh_token_storage.clone(),
            config.refresh_token_lifetime_seconds().saturating_mul(1000),
        ));

        let registration_token_storage: Arc<synapse_storage::registration_token::RegistrationTokenStorage> =
            Arc::new(synapse_storage::registration_token::RegistrationTokenStorage::new(pool));
        let registration_token_service = Arc::new(crate::registration_token_service::RegistrationTokenService::new(
            registration_token_storage.clone(),
        ));

        let captcha_storage: Arc<synapse_storage::captcha::CaptchaStorage> =
            Arc::new(synapse_storage::captcha::CaptchaStorage::new(pool));
        let captcha_service = Arc::new(crate::captcha_service::CaptchaService::with_sms_config(
            captcha_storage.clone(),
            task_queue.clone(),
            config.smtp.enabled,
            &config.sms,
        ));

        let federation_blacklist_storage: Arc<synapse_storage::federation_blacklist::FederationBlacklistStorage> =
            Arc::new(synapse_storage::federation_blacklist::FederationBlacklistStorage::new(pool));
        let federation_blacklist_service = Arc::new(
            crate::federation_blacklist_service::FederationBlacklistService::new(federation_blacklist_storage.clone()),
        );
        let admin_federation_storage: Arc<synapse_storage::admin_federation::AdminFederationStorage> =
            Arc::new(synapse_storage::admin_federation::AdminFederationStorage::new(pool));
        let admin_federation_service = Arc::new(crate::admin_federation_service::AdminFederationService::new(
            admin_federation_storage,
            federation_blacklist_storage.clone(),
            federation_blacklist_service.clone(),
        ));

        let push_notification_storage: Arc<synapse_storage::push_notification::PushNotificationStorage> =
            Arc::new(synapse_storage::push_notification::PushNotificationStorage::new(pool));
        let account_data_storage_for_push = Arc::new(synapse_storage::account_data::AccountDataStorage::new(pool));
        let push_notification_service =
            crate::push_notification_service::PushNotificationService::new(push_notification_storage.clone())
                .with_account_data_storage(account_data_storage_for_push)
                .with_server_metrics(server_metrics.clone());
        // A-10: consume `push.push_gateway_url` — it was parsed and validated but
        // never used to build a gateway, so `PushNotificationService::push_gateway`
        // stayed `None` for the process lifetime and every `upstream`-type push
        // failed with "upstream push provider is not initialized". Wiring it here
        // is the only place the config and the service meet.
        //
        // `validate_push_gateway_url` is re-run deliberately: the config loader
        // does not validate it, and a bad URL must degrade to "gateway disabled"
        // (pushes then fail closed with that same clear error) rather than panic
        // or be handed to the SSRF-checked sender as if it were trusted.
        let push_notification_service = match config.push.push_gateway_url.as_deref() {
            Some(url) => match crate::push::gateway::validate_push_gateway_url(url) {
                Ok(()) => {
                    let gateway_config = crate::push::gateway::PushGatewayConfig::default();
                    let gateway = Arc::new(crate::push::gateway::PushGateway::new(&gateway_config));
                    push_notification_service.with_push_gateway(gateway)
                }
                Err(error) => {
                    tracing::warn!(
                        %error,
                        "push.push_gateway_url is set but invalid; upstream push delivery stays disabled"
                    );
                    push_notification_service
                }
            },
            None => push_notification_service,
        };
        // Providers are configured by rows in the `push_config` table. Without this
        // call every provider stays `None`, so `send_to_provider` can never reach
        // `send_with_retry` and no push is actually delivered.
        //
        // A config read failure must NOT abort startup (the homeserver is still
        // usable without push), but it must not be silent either: `container::new`
        // is infallible, so log loudly and leave the providers unset — delivery for
        // an enabled-but-uninitialized provider then fails closed instead of
        // reporting a fake success (see `PushNotificationService::provider_unavailable`).
        if let Err(error) = push_notification_service.initialize_providers().await {
            tracing::error!(
                %error,
                "Failed to initialize push providers from push_config; push delivery will be reported as failed"
            );
        }
        let push_notification_service = Arc::new(push_notification_service);

        let media_quota_storage: Arc<synapse_storage::media_quota::MediaQuotaStorage> =
            Arc::new(synapse_storage::media_quota::MediaQuotaStorage::new(pool));
        let media_quota_service =
            Arc::new(crate::media_quota_service::MediaQuotaService::new(media_quota_storage.clone()));

        let telemetry_alert_service =
            Arc::new(crate::telemetry_service::TelemetryAlertService::new(pool.clone(), config.database.max_size));

        let rendezvous_storage: Arc<dyn synapse_storage::rendezvous::RendezvousStoreApi> =
            Arc::new(synapse_storage::rendezvous::RendezvousStorage::new(pool.clone()));
        let rendezvous_service = Arc::new(crate::rendezvous_service::RendezvousService::new(rendezvous_storage));
        let login_token_storage: Arc<dyn synapse_storage::login_token::LoginTokenStoreApi> =
            Arc::new(synapse_storage::login_token::LoginTokenStorage::new(pool));
        let login_token_service =
            Arc::new(crate::login_token_service::LoginTokenService::new(login_token_storage.clone()));

        let app_service_storage: Arc<synapse_storage::application_service::ApplicationServiceStorage> =
            Arc::new(ApplicationServiceStorage::new(pool));
        let app_service_event_concrete = Arc::new(EventStorage::new(pool, config.server.get_server_name().to_owned()));
        let app_service_event_reader: Arc<dyn synapse_storage::event::EventReader> = app_service_event_concrete.clone();
        let app_service_manager = Arc::new(crate::application_service::ApplicationServiceManager::new(
            app_service_storage.clone(),
            app_service_event_reader.clone(),
            config.server.get_server_name().to_owned(),
        ));
        let app_service_scheduler =
            Arc::new(crate::application_service::ApplicationServiceScheduler::new(app_service_manager.clone()));
        #[cfg(feature = "external-services")]
        let external_service_integration =
            Arc::new(crate::external_service_integration::ExternalServiceIntegration::new(
                app_service_storage.clone(),
                config.server.get_server_name().to_owned(),
            ));

        let should_start_app_service_scheduler =
            should_run_global_maintenance(&config.worker) && !config.server.app_service_config_files.is_empty();
        if should_start_app_service_scheduler {
            let _ = app_service_scheduler.clone().start(shutdown_token.clone());
        } else {
            ::tracing::info!(
                worker_type = current_instance_worker_type(&config.worker).as_str(),
                maintenance_owner = global_maintenance_owner(&config.worker).as_str(),
                has_app_service_configs = !config.server.app_service_config_files.is_empty(),
                "Skipping application service scheduler startup on this worker instance"
            );
        }

        let worker_storage: Arc<dyn synapse_storage::worker::WorkerStoreApi> =
            Arc::new(synapse_storage::worker::WorkerStorage::new(pool));
        let worker_manager = Arc::new(crate::worker::WorkerManager::new(worker_storage.clone()));

        let admin_media_storage = Arc::new(AdminMediaStorage::new(pool));
        let quarantine_change_storage = Arc::new(QuarantinedMediaChangeStorage::new(pool));
        let admin_media_service = Arc::new(crate::admin_media_service::AdminMediaService::new(
            admin_media_storage,
            quarantine_change_storage,
            user_service.clone(),
        ));
        let rate_limit_storage = Arc::new(RateLimitStorage::new(pool));
        let admin_security_service = Arc::new(crate::admin_security_service::AdminSecurityService::new(
            user_storage.clone(),
            user_service.clone(),
            rate_limit_storage,
            cache.clone(),
        ));
        let admin_server_service = Arc::new(crate::admin_server_service::AdminServerService::new(pool.clone()));
        let admin_token_service = Arc::new(crate::admin_token_service::AdminTokenService::new(
            Arc::new(AccessTokenStorage::new(pool)),
            refresh_token_storage.clone(),
            registration_token_service.clone(),
        ));

        let admin_user_service = Arc::new(
            crate::admin_user_service::AdminUserService::new(
                pool.clone(),
                user_service.clone(),
                user_storage.clone(),
                Arc::new(DeviceStorage::new(pool)),
                Arc::new(RoomStorage::new(pool)),
                Arc::new(RoomMemberStorage::new(pool, config.server.get_server_name())),
                config.server.name.clone(),
            )
            .with_evict_concurrency(config.server.admin_evict_max_concurrency)
            .with_evict_page_size(config.server.admin_evict_page_size),
        );

        Self {
            user: AdminUserServices {
                admin_registration_service,
                admin_user_service,
                email_verification_service,
                admin_token_service,
                refresh_token_storage,
                refresh_token_service,
                registration_token_storage,
                registration_token_service,
            },
            federation: AdminFederationServices {
                admin_federation_service,
                federation_blacklist_storage,
                federation_blacklist_service,
            },
            media: AdminMediaServices { admin_media_service, media_quota_storage, media_quota_service },
            security: AdminSecurityServices {
                admin_security_service,
                captcha_storage,
                captcha_service,
                audit_storage,
                admin_audit_service,
                admin_server_service,
                telemetry_alert_service,
            },
            modules: AdminModuleServices {
                feature_flag_storage,
                feature_flag_service,
                event_report_storage,
                event_report_service,
                delayed_event_storage,
                delayed_event_service,
                e2ee_audit_service,
                background_update_storage,
                background_update_service,
                module_storage,
                module_service,
                account_validity_service,
                retention_storage,
                retention_service,
                push_notification_storage,
                push_notification_service,
                app_service_storage,
                app_service_event_reader,
                app_service_manager,
                app_service_scheduler,
                policy_service: Arc::new(crate::policy_service::PolicyService::new(config.policy_server.clone())),
                #[cfg(feature = "external-services")]
                external_service_integration,
                rendezvous_service,
                login_token_storage,
                login_token_service,
                worker_storage,
                worker_manager,
            },
        }
    }
}

/// See [`burn_after_read_processor_enabled`].
#[cfg(feature = "burn-after-read")]
pub(crate) fn burn_after_read_processor_enabled(config_enabled: bool) -> bool {
    config_enabled
}
