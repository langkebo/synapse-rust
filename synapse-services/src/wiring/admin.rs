//! Admin assembly — decomposed into 5 domain sub-structs.

use std::sync::Arc;

use synapse_cache::CacheManager;
use synapse_common::config::Config;
use synapse_common::metrics::MetricsCollector;
use synapse_common::task_queue::RedisTaskQueue;
use synapse_storage::*;

use crate::auth::{CredentialAuth, RoomAuth, TokenAuth};
use crate::worker::topology_validator::{
    current_instance_worker_type, global_maintenance_owner, should_run_global_maintenance,
};
use crate::UserService;
use synapse_storage::email_verification::EmailVerificationStorage;

#[derive(Clone)]
pub struct AdminUserServices {
    pub admin_registration_service: crate::admin_registration_service::AdminRegistrationService,
    pub admin_user_service: Arc<crate::admin_user_service::AdminUserService>,
    pub email_verification_storage: Arc<dyn synapse_storage::email_verification::EmailVerificationStoreApi>,
    pub admin_token_service: Arc<crate::admin_token_service::AdminTokenService>,
    pub refresh_token_storage: Arc<dyn synapse_storage::refresh_token::RefreshTokenStoreApi>,
    pub refresh_token_service: Arc<crate::refresh_token_service::RefreshTokenService>,
    pub registration_token_storage: Arc<dyn synapse_storage::registration_token::RegistrationTokenStoreApi>,
    pub registration_token_service: Arc<crate::registration_token_service::RegistrationTokenService>,
}

#[derive(Clone)]
pub struct AdminFederationServices {
    pub admin_federation_service: Arc<crate::admin_federation_service::AdminFederationService>,
    pub federation_blacklist_storage: Arc<dyn synapse_storage::federation_blacklist::FederationBlacklistStoreApi>,
    pub federation_blacklist_service: Arc<crate::federation_blacklist_service::FederationBlacklistService>,
}

#[derive(Clone)]
pub struct AdminMediaServices {
    pub admin_media_service: Arc<crate::admin_media_service::AdminMediaService>,
    pub media_quota_storage: Arc<dyn synapse_storage::media_quota::MediaQuotaStoreApi>,
    pub media_quota_service: Arc<crate::media_quota_service::MediaQuotaService>,
}

#[derive(Clone)]
pub struct AdminSecurityServices {
    pub admin_security_service: Arc<crate::admin_security_service::AdminSecurityService>,
    pub captcha_storage: Arc<dyn synapse_storage::captcha::CaptchaStoreApi>,
    pub captcha_service: Arc<crate::captcha_service::CaptchaService>,
    pub audit_storage: synapse_storage::audit::AuditEventStorage,
    pub admin_audit_service: Arc<crate::admin_audit_service::AdminAuditService>,
    pub admin_server_service: Arc<crate::admin_server_service::AdminServerService>,
    pub telemetry_alert_service: Arc<crate::telemetry_service::TelemetryAlertService>,
}

#[derive(Clone)]
pub struct AdminModuleServices {
    pub feature_flag_storage: Arc<dyn synapse_storage::feature_flags::FeatureFlagStoreApi>,
    pub feature_flag_service: Arc<crate::feature_flag_service::FeatureFlagService>,
    pub event_report_storage: Arc<dyn synapse_storage::event_report::EventReportStoreApi>,
    pub event_report_service: Arc<crate::event_report_service::EventReportService>,
    pub background_update_storage: synapse_storage::background_update::BackgroundUpdateStorage,
    pub background_update_service: Arc<crate::background_update_service::BackgroundUpdateService>,
    pub module_storage: Arc<dyn synapse_storage::module::ModuleStoreApi>,
    pub module_service: Arc<crate::module_service::ModuleService>,
    pub account_validity_service: Arc<crate::module_service::AccountValidityService>,
    pub retention_storage: Arc<dyn synapse_storage::retention::RetentionStoreApi>,
    pub retention_service: Arc<crate::retention_service::RetentionService>,
    pub push_notification_storage: Arc<dyn synapse_storage::push_notification::PushNotificationStoreApi>,
    pub push_notification_service: Arc<crate::push_notification_service::PushNotificationService>,
    pub app_service_storage: Arc<dyn synapse_storage::application_service::ApplicationServiceStoreApi>,
    pub app_service_event_reader: Arc<dyn synapse_storage::event::EventReader>,
    pub app_service_manager: Arc<crate::application_service::ApplicationServiceManager>,
    pub app_service_scheduler: Arc<crate::application_service::ApplicationServiceScheduler>,
    #[cfg(feature = "external-services")]
    pub external_service_integration: Arc<crate::external_service_integration::ExternalServiceIntegration>,
    pub rendezvous_storage: Arc<dyn synapse_storage::rendezvous::RendezvousStoreApi>,
    pub rendezvous_message_storage: Arc<dyn synapse_storage::rendezvous::RendezvousMessageStoreApi>,
    pub worker_storage: Arc<dyn synapse_storage::worker::WorkerStoreApi>,
    pub worker_manager: Arc<crate::worker::WorkerManager>,
}

/// Aggregate admin services, decomposed into 5 domain sub-structs.
#[derive(Clone)]
pub struct AdminServices {
    pub user: AdminUserServices,
    pub federation: AdminFederationServices,
    pub media: AdminMediaServices,
    pub security: AdminSecurityServices,
    pub modules: AdminModuleServices,
    pub user_service: Arc<UserService>,
}

impl AdminServices {
    #[allow(clippy::too_many_arguments)]
    pub async fn new(
        pool: &Arc<sqlx::PgPool>,
        cache: &Arc<CacheManager>,
        config: &Config,
        task_queue: &Option<Arc<RedisTaskQueue>>,
        metrics: &Arc<MetricsCollector>,
        token_auth: &Arc<dyn TokenAuth>,
        credential_auth: &Arc<dyn CredentialAuth>,
        _room_auth: &Arc<dyn RoomAuth>,
        user_storage: &Arc<dyn UserStore>,
    ) -> Self {
        let user_service = Arc::new(UserService::new(user_storage.clone()));

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

        let email_verification_storage: Arc<dyn synapse_storage::email_verification::EmailVerificationStoreApi> =
            Arc::new(EmailVerificationStorage::new(pool));
        let audit_storage = synapse_storage::audit::AuditEventStorage::new(pool);
        let admin_audit_service =
            Arc::new(crate::admin_audit_service::AdminAuditService::new(Arc::new(audit_storage.clone())));

        let feature_flag_storage: Arc<dyn synapse_storage::feature_flags::FeatureFlagStoreApi> =
            Arc::new(synapse_storage::feature_flags::FeatureFlagStorage::new(pool, cache.clone()));
        let feature_flag_service = Arc::new(crate::feature_flag_service::FeatureFlagService::new(
            feature_flag_storage.clone(),
            admin_audit_service.clone(),
        ));

        let event_report_storage: Arc<dyn synapse_storage::event_report::EventReportStoreApi> =
            Arc::new(synapse_storage::event_report::EventReportStorage::new(pool));
        let event_report_service =
            Arc::new(crate::event_report_service::EventReportService::new(event_report_storage.clone()));

        let background_update_storage = synapse_storage::background_update::BackgroundUpdateStorage::new(pool);
        let background_update_service = Arc::new(
            crate::background_update_service::BackgroundUpdateService::new(Arc::new(background_update_storage.clone()))
                .with_lock_retry_config(config.worker.lock_max_retries, config.worker.lock_max_retry_interval_ms),
        );

        let module_storage: Arc<dyn synapse_storage::module::ModuleStoreApi> =
            Arc::new(synapse_storage::module::ModuleStorage::new(pool));
        let module_service = Arc::new(crate::module_service::ModuleService::new(module_storage.clone()));
        let account_validity_service =
            Arc::new(crate::module_service::AccountValidityService::new(module_storage.clone()));

        let retention_storage: Arc<dyn synapse_storage::retention::RetentionStoreApi> =
            Arc::new(synapse_storage::retention::RetentionStorage::new(pool));
        let chunked_upload_storage: Arc<dyn synapse_storage::media::ChunkedUploadStoreApi> =
            Arc::new(synapse_storage::media::ChunkedUploadStorage::new(pool));
        let retention_service = Arc::new(crate::retention_service::RetentionService::new(
            retention_storage.clone(),
            chunked_upload_storage.clone(),
            metrics,
            Arc::new(audit_storage.clone()),
        ));

        let refresh_token_storage: Arc<dyn synapse_storage::refresh_token::RefreshTokenStoreApi> =
            Arc::new(synapse_storage::refresh_token::RefreshTokenStorage::new(pool));
        let refresh_token_service = Arc::new(crate::refresh_token_service::RefreshTokenService::new(
            refresh_token_storage.clone(),
            config.server.refresh_token_ttl_secs.saturating_mul(1000),
        ));

        let registration_token_storage: Arc<dyn synapse_storage::registration_token::RegistrationTokenStoreApi> =
            Arc::new(synapse_storage::registration_token::RegistrationTokenStorage::new(pool));
        let registration_token_service = Arc::new(crate::registration_token_service::RegistrationTokenService::new(
            registration_token_storage.clone(),
        ));

        let captcha_storage: Arc<dyn synapse_storage::captcha::CaptchaStoreApi> =
            Arc::new(synapse_storage::captcha::CaptchaStorage::new(pool));
        let captcha_service = Arc::new(crate::captcha_service::CaptchaService::with_sms_config(
            captcha_storage.clone(),
            task_queue.clone(),
            config.smtp.enabled,
            &config.sms,
        ));

        let federation_blacklist_storage: Arc<dyn synapse_storage::federation_blacklist::FederationBlacklistStoreApi> =
            Arc::new(synapse_storage::federation_blacklist::FederationBlacklistStorage::new(pool));
        let federation_blacklist_service = Arc::new(
            crate::federation_blacklist_service::FederationBlacklistService::new(federation_blacklist_storage.clone()),
        );
        let admin_federation_storage: Arc<dyn synapse_storage::admin_federation::AdminFederationStoreApi> =
            Arc::new(synapse_storage::admin_federation::AdminFederationStorage::new(pool));
        let admin_federation_service = Arc::new(crate::admin_federation_service::AdminFederationService::new(
            admin_federation_storage,
            federation_blacklist_storage.clone(),
            federation_blacklist_service.clone(),
        ));

        let push_notification_storage: Arc<dyn synapse_storage::push_notification::PushNotificationStoreApi> =
            Arc::new(synapse_storage::push_notification::PushNotificationStorage::new(pool));
        let account_data_storage_for_push = Arc::new(synapse_storage::account_data::AccountDataStorage::new(pool));
        let push_notification_service = Arc::new(
            crate::push_notification_service::PushNotificationService::new(push_notification_storage.clone())
                .with_account_data_storage(account_data_storage_for_push),
        );

        let media_quota_storage: Arc<dyn synapse_storage::media_quota::MediaQuotaStoreApi> =
            Arc::new(synapse_storage::media_quota::MediaQuotaStorage::new(pool));
        let media_quota_service =
            Arc::new(crate::media_quota_service::MediaQuotaService::new(media_quota_storage.clone()));

        let telemetry_alert_service =
            Arc::new(crate::telemetry_service::TelemetryAlertService::new(pool.clone(), config.database.max_size));

        let rendezvous_storage: Arc<dyn synapse_storage::rendezvous::RendezvousStoreApi> =
            Arc::new(synapse_storage::rendezvous::RendezvousStorage::new(pool.clone()));
        let rendezvous_message_storage: Arc<dyn synapse_storage::rendezvous::RendezvousMessageStoreApi> =
            Arc::new(synapse_storage::rendezvous::RendezvousMessageStorage::new(pool.clone()));

        let app_service_storage: Arc<dyn synapse_storage::application_service::ApplicationServiceStoreApi> =
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
            app_service_scheduler.clone().start();
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
        let worker_manager =
            Arc::new(crate::worker::WorkerManager::new(worker_storage.clone(), config.server.name.clone()));

        let admin_media_storage = Arc::new(AdminMediaStorage::new(pool));
        let admin_media_service =
            Arc::new(crate::admin_media_service::AdminMediaService::new(admin_media_storage, user_service.clone()));
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

        let admin_user_service = Arc::new(crate::admin_user_service::AdminUserService::new(
            pool.clone(),
            user_service.clone(),
            user_storage.clone(),
            Arc::new(DeviceStorage::new(pool)),
            Arc::new(RoomStorage::new(pool)),
            Arc::new(RoomMemberStorage::new(pool, config.server.get_server_name())),
            config.server.name.clone(),
        ));

        Self {
            user: AdminUserServices {
                admin_registration_service,
                admin_user_service,
                email_verification_storage,
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
                #[cfg(feature = "external-services")]
                external_service_integration,
                rendezvous_storage,
                rendezvous_message_storage,
                worker_storage,
                worker_manager,
            },
            user_service,
        }
    }
}

#[cfg(feature = "burn-after-read")]
pub(crate) fn burn_after_read_processor_enabled(config_enabled: bool) -> bool {
    config_enabled
}
