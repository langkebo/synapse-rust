//! Admin service-group type definitions (extracted from `wiring/admin.rs`).
//!
//! 2026-10-10 从 `wiring/admin.rs` 抽出 —— 该文件的体积已越过棘轮天花板，
//! 而「类型定义」与「装配逻辑（`impl AdminServices::new`）」本就是两个关注点。
//! 原有的 `wiring::admin::{AdminServices, Admin*Services}` 路径由 `admin.rs`
//! 的 `pub use` 保持不变。

use std::sync::Arc;

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
    /// The `refresh_token_service` field.
    pub refresh_token_service: Arc<crate::refresh_token_service::RefreshTokenService>,
    /// The `registration_token_service` field.
    pub registration_token_service: Arc<crate::registration_token_service::RegistrationTokenService>,
}

/// The `AdminFederationServices` struct.
#[derive(Clone)]
pub struct AdminFederationServices {
    /// The `admin_federation_service` field.
    pub admin_federation_service: Arc<crate::admin_federation_service::AdminFederationService>,
    /// The `federation_blacklist_service` field.
    pub federation_blacklist_service: Arc<crate::federation_blacklist_service::FederationBlacklistService>,
}

/// The `AdminMediaServices` struct.
#[derive(Clone)]
pub struct AdminMediaServices {
    /// The `admin_media_service` field.
    pub admin_media_service: Arc<crate::admin_media_service::AdminMediaService>,
    /// The `media_quota_service` field.
    pub media_quota_service: Arc<crate::media_quota_service::MediaQuotaService>,
}

/// The `AdminSecurityServices` struct.
#[derive(Clone)]
pub struct AdminSecurityServices {
    /// The `admin_security_service` field.
    pub admin_security_service: Arc<crate::admin_security_service::AdminSecurityService>,
    /// The `captcha_service` field.
    pub captcha_service: Arc<crate::captcha_service::CaptchaService>,
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
    /// The `feature_flag_service` field.
    pub feature_flag_service: Arc<crate::feature_flag_service::FeatureFlagService>,
    /// The `event_report_service` field.
    pub event_report_service: Arc<crate::event_report_service::EventReportService>,
    /// MSC4140 — Cancellable delayed events storage.
    pub delayed_event_storage: Arc<dyn synapse_storage::delayed_events::DelayedEventStorageApi>,
    /// MSC4140 — Cancellable delayed events service.
    pub delayed_event_service: Arc<crate::delayed_event_service::DelayedEventService>,
    /// The `e2ee_audit_service` field.
    pub e2ee_audit_service: Arc<crate::e2ee_audit::E2eeAuditService>,
    /// The `background_update_service` field.
    pub background_update_service: Arc<crate::background_update_service::BackgroundUpdateService>,
    /// The `module_storage` field.
    pub module_storage: Arc<synapse_storage::module::ModuleStorage>,
    /// The `module_service` field.
    pub module_service: Arc<crate::module_service::ModuleService>,
    /// The `account_validity_service` field.
    pub account_validity_service: Arc<crate::module_service::AccountValidityService>,
    /// The `retention_service` field.
    pub retention_service: Arc<crate::retention_service::RetentionService>,
    /// The `push_notification_service` field.
    pub push_notification_service: Arc<crate::push_notification_service::PushNotificationService>,
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
/// RD-09 (2026-10-23): the 14 "backing storage" fields that were retained here
/// on the theory that "future route handlers" might want direct access are gone.
/// Every `*_service` already owns its own `Arc` to the backing storage, so the
/// container-level copies were pure dead weight (never read after construction).
/// Removed: `registration_token_storage`, `captcha_storage`, `audit_storage`,
/// `feature_flag_storage`, `event_report_storage`, `background_update_storage`,
/// `retention_storage`, `push_notification_storage`, `app_service_storage`,
/// `app_service_event_reader`, `worker_storage`, `refresh_token_storage`,
/// `federation_blacklist_storage`, `media_quota_storage`.
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
