//! TDD tests for synapse-services remaining domain grouping (P7.4).
//!
//! These tests verify that the new domain-grouped paths resolve to the same
//! types as the legacy flat / direct module paths, ensuring the refactor is
//! behavior-preserving. Each test asserts compile-time type identity between
//! the grouped path (e.g. `synapse_services::account::UserService`) and the
//! legacy flat path (e.g. `synapse_services::UserService`).

/// Compile-time type identity check: asserts that two paths resolve to the
/// same concrete type. If either path is unreachable or the types differ,
/// compilation fails (RED).
fn assert_same_type<T: ?Sized>(_a: &T, _b: &T) {}

// =============================================================================
// account domain — UserService, DehydratedDeviceService (previously flat in lib.rs)
// =============================================================================

#[test]
fn test_account_user_service_path_identity() {
    let legacy: Option<synapse_services::UserService> = None;
    let grouped: Option<synapse_services::account::UserService> = None;
    if let (Some(a), Some(b)) = (legacy, grouped) {
        assert_same_type(&a, &b);
    }
}

#[test]
fn test_account_dehydrated_device_service_path_identity() {
    let legacy: Option<synapse_services::DehydratedDeviceService> = None;
    let grouped: Option<synapse_services::account::DehydratedDeviceService> = None;
    if let (Some(a), Some(b)) = (legacy, grouped) {
        assert_same_type(&a, &b);
    }
}

#[test]
fn test_account_refresh_token_service_path_identity() {
    let legacy: Option<synapse_services::RefreshTokenService> = None;
    let grouped: Option<synapse_services::account::RefreshTokenService> = None;
    if let (Some(a), Some(b)) = (legacy, grouped) {
        assert_same_type(&a, &b);
    }
}

#[test]
fn test_account_registration_token_service_path_identity() {
    let legacy: Option<synapse_services::RegistrationTokenService> = None;
    let grouped: Option<synapse_services::account::RegistrationTokenService> = None;
    if let (Some(a), Some(b)) = (legacy, grouped) {
        assert_same_type(&a, &b);
    }
}

#[test]
fn test_account_user_lock_service_path_identity() {
    let legacy: Option<synapse_services::UserLockService> = None;
    let grouped: Option<synapse_services::account::UserLockService> = None;
    if let (Some(a), Some(b)) = (legacy, grouped) {
        assert_same_type(&a, &b);
    }
}

#[test]
fn test_account_captcha_service_path_identity() {
    let legacy: Option<synapse_services::CaptchaService> = None;
    let grouped: Option<synapse_services::account::CaptchaService> = None;
    if let (Some(a), Some(b)) = (legacy, grouped) {
        assert_same_type(&a, &b);
    }
}

#[test]
fn test_account_account_data_service_path_identity() {
    let legacy: Option<synapse_services::AccountDataService> = None;
    let grouped: Option<synapse_services::account::AccountDataService> = None;
    if let (Some(a), Some(b)) = (legacy, grouped) {
        assert_same_type(&a, &b);
    }
}

#[test]
fn test_account_uia_session_path_identity() {
    let legacy: Option<synapse_services::UiaSession> = None;
    let grouped: Option<synapse_services::account::UiaSession> = None;
    if let (Some(a), Some(b)) = (legacy, grouped) {
        assert_same_type(&a, &b);
    }
}

// =============================================================================
// sync domain — SearchService, SearchResult, PresenceService (previously flat)
// =============================================================================

#[test]
fn test_sync_search_service_path_identity() {
    let legacy: Option<synapse_services::SearchService> = None;
    let grouped: Option<synapse_services::sync::SearchService> = None;
    if let (Some(a), Some(b)) = (legacy, grouped) {
        assert_same_type(&a, &b);
    }
}

#[test]
fn test_sync_search_result_path_identity() {
    // NOTE: SearchResult also exists in synapse_storage, so the root-level
    // `synapse_services::SearchResult` is ambiguous between the sync glob and
    // the `pub(crate) use storage::*;` bridge import. Use the direct module
    // path as the reference instead.
    let legacy: Option<synapse_services::search_service::SearchResult> = None;
    let grouped: Option<synapse_services::sync::SearchResult> = None;
    if let (Some(a), Some(b)) = (legacy, grouped) {
        assert_same_type(&a, &b);
    }
}

#[test]
fn test_sync_search_filters_path_identity() {
    let legacy: Option<synapse_services::SearchFilters> = None;
    let grouped: Option<synapse_services::sync::SearchFilters> = None;
    if let (Some(a), Some(b)) = (legacy, grouped) {
        assert_same_type(&a, &b);
    }
}

#[test]
fn test_sync_presence_service_path_identity() {
    let legacy: Option<synapse_services::presence_service::PresenceService> = None;
    let grouped: Option<synapse_services::sync::PresenceService> = None;
    if let (Some(a), Some(b)) = (legacy, grouped) {
        assert_same_type(&a, &b);
    }
}

// =============================================================================
// media domain — MediaQuotaService (grouped via media glob vs direct module)
// =============================================================================

#[test]
fn test_media_quota_service_path_identity() {
    let legacy: Option<synapse_services::media_quota_service::MediaQuotaService> = None;
    let grouped: Option<synapse_services::media::MediaQuotaService> = None;
    if let (Some(a), Some(b)) = (legacy, grouped) {
        assert_same_type(&a, &b);
    }
}

// =============================================================================
// room domain — RelationsService, RetentionService, ThreadService
// =============================================================================

#[test]
fn test_room_relations_service_path_identity() {
    let legacy: Option<synapse_services::relations_service::RelationsService> = None;
    let grouped: Option<synapse_services::room::RelationsService> = None;
    if let (Some(a), Some(b)) = (legacy, grouped) {
        assert_same_type(&a, &b);
    }
}

#[test]
fn test_room_retention_service_path_identity() {
    let legacy: Option<synapse_services::retention_service::RetentionService> = None;
    let grouped: Option<synapse_services::room::RetentionService> = None;
    if let (Some(a), Some(b)) = (legacy, grouped) {
        assert_same_type(&a, &b);
    }
}

#[test]
fn test_room_retention_status_summary_path_identity() {
    let legacy: Option<synapse_services::retention_service::RetentionStatusSummary> = None;
    let grouped: Option<synapse_services::room::RetentionStatusSummary> = None;
    if let (Some(a), Some(b)) = (legacy, grouped) {
        assert_same_type(&a, &b);
    }
}

#[test]
fn test_room_thread_service_path_identity() {
    let legacy: Option<synapse_services::thread_service::ThreadService> = None;
    let grouped: Option<synapse_services::room::ThreadService> = None;
    if let (Some(a), Some(b)) = (legacy, grouped) {
        assert_same_type(&a, &b);
    }
}

#[test]
fn test_room_thread_create_thread_request_path_identity() {
    let legacy: Option<synapse_services::thread_service::CreateThreadRequest> = None;
    let grouped: Option<synapse_services::room::CreateThreadRequest> = None;
    if let (Some(a), Some(b)) = (legacy, grouped) {
        assert_same_type(&a, &b);
    }
}

// =============================================================================
// infra domain — BackgroundUpdateService, TelemetryService, TranslationResult,
// E2eeAuditService
// =============================================================================

#[test]
fn test_infra_background_update_service_path_identity() {
    let legacy: Option<synapse_services::background_update_service::BackgroundUpdateService> = None;
    let grouped: Option<synapse_services::infra::BackgroundUpdateService> = None;
    if let (Some(a), Some(b)) = (legacy, grouped) {
        assert_same_type(&a, &b);
    }
}

#[test]
fn test_infra_telemetry_service_path_identity() {
    let legacy: Option<synapse_services::telemetry_service::TelemetryService> = None;
    let grouped: Option<synapse_services::infra::TelemetryService> = None;
    if let (Some(a), Some(b)) = (legacy, grouped) {
        assert_same_type(&a, &b);
    }
}

#[test]
fn test_infra_translation_result_path_identity() {
    let legacy: Option<synapse_services::translation_service::TranslationResult> = None;
    let grouped: Option<synapse_services::infra::TranslationResult> = None;
    if let (Some(a), Some(b)) = (legacy, grouped) {
        assert_same_type(&a, &b);
    }
}

#[test]
fn test_infra_e2ee_audit_service_path_identity() {
    let legacy: Option<synapse_services::e2ee_audit::E2eeAuditService> = None;
    let grouped: Option<synapse_services::infra::E2eeAuditService> = None;
    if let (Some(a), Some(b)) = (legacy, grouped) {
        assert_same_type(&a, &b);
    }
}

// =============================================================================
// application domain — ApplicationServiceManager, ApplicationServiceScheduler,
// NamespacesInfo, ModuleService (new domain in P7.4)
// =============================================================================

#[test]
fn test_application_service_manager_path_identity() {
    let legacy: Option<synapse_services::ApplicationServiceManager> = None;
    let grouped: Option<synapse_services::application::ApplicationServiceManager> = None;
    if let (Some(a), Some(b)) = (legacy, grouped) {
        assert_same_type(&a, &b);
    }
}

#[test]
fn test_application_service_scheduler_path_identity() {
    let legacy: Option<synapse_services::ApplicationServiceScheduler> = None;
    let grouped: Option<synapse_services::application::ApplicationServiceScheduler> = None;
    if let (Some(a), Some(b)) = (legacy, grouped) {
        assert_same_type(&a, &b);
    }
}

#[test]
fn test_application_namespaces_info_path_identity() {
    let legacy: Option<synapse_services::NamespacesInfo> = None;
    let grouped: Option<synapse_services::application::NamespacesInfo> = None;
    if let (Some(a), Some(b)) = (legacy, grouped) {
        assert_same_type(&a, &b);
    }
}

#[test]
fn test_application_module_service_path_identity() {
    let legacy: Option<synapse_services::module_service::ModuleService> = None;
    let grouped: Option<synapse_services::application::ModuleService> = None;
    if let (Some(a), Some(b)) = (legacy, grouped) {
        assert_same_type(&a, &b);
    }
}

#[test]
fn test_application_module_registry_path_identity() {
    let legacy: Option<synapse_services::module_service::ModuleRegistry> = None;
    let grouped: Option<synapse_services::application::ModuleRegistry> = None;
    if let (Some(a), Some(b)) = (legacy, grouped) {
        assert_same_type(&a, &b);
    }
}

// =============================================================================
// push domain — ClientPushService (previously a root module only)
// =============================================================================

#[test]
fn test_push_client_push_service_path_identity() {
    let legacy: Option<synapse_services::client_push_service::ClientPushService> = None;
    let grouped: Option<synapse_services::push::ClientPushService> = None;
    if let (Some(a), Some(b)) = (legacy, grouped) {
        assert_same_type(&a, &b);
    }
}

#[test]
fn test_push_upsert_pusher_request_path_identity() {
    let legacy: Option<synapse_services::client_push_service::UpsertPusherRequest> = None;
    let grouped: Option<synapse_services::push::UpsertPusherRequest> = None;
    if let (Some(a), Some(b)) = (legacy, grouped) {
        assert_same_type(&a, &b);
    }
}
