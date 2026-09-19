//! TDD tests for synapse-services sync domain grouping (P5).
//!
//! These tests verify that the new domain-grouped path
//! (`synapse_services::sync::SyncService`) resolves to the same type as the
//! legacy flat path (`synapse_services::sync_service::SyncService`), ensuring the refactor
//! is behavior-preserving.

/// Compile-time type identity check: asserts that two paths resolve to the
/// same concrete type. If either path is unreachable or the types differ,
/// compilation fails (RED).
fn assert_same_type<T>(_a: &T, _b: &T) {}

#[test]
fn test_sync_service_path_identity() {
    let legacy_ref: Option<&synapse_services::sync_service::SyncService> = None;
    let grouped_ref: Option<&synapse_services::sync::SyncService> = None;
    if let (Some(a), Some(b)) = (legacy_ref, grouped_ref) {
        assert_same_type(a, b);
    }
}

#[test]
fn test_sliding_sync_service_path_identity() {
    let legacy_ref: Option<&synapse_services::sliding_sync_service::SlidingSyncService> = None;
    let grouped_ref: Option<&synapse_services::sync::SlidingSyncService> = None;
    if let (Some(a), Some(b)) = (legacy_ref, grouped_ref) {
        assert_same_type(a, b);
    }
}

#[test]
fn test_sync_token_path_identity() {
    let legacy_ref: Option<&synapse_services::sync_service::SyncToken> = None;
    let grouped_ref: Option<&synapse_services::sync::SyncToken> = None;
    if let (Some(a), Some(b)) = (legacy_ref, grouped_ref) {
        assert_same_type(a, b);
    }
}

// =============================================================================
// P7.2 — domain grouping path-identity tests for the 7 new service domains.
//
// Each test asserts that the legacy flat root path (e.g.
// `synapse_services::media_service::MediaService`, now provided via `pub use <domain>::*;`)
// resolves to the same type as the grouped domain path (e.g.
// `synapse_services::media::MediaService`).
// =============================================================================

#[test]
fn test_media_service_path_identity() {
    let legacy_ref: Option<&synapse_services::media_service::MediaService> = None;
    let grouped_ref: Option<&synapse_services::media::MediaService> = None;
    if let (Some(a), Some(b)) = (legacy_ref, grouped_ref) {
        assert_same_type(a, b);
    }
}

#[test]
fn test_media_thumbnail_settings_path_identity() {
    let legacy_ref: Option<&synapse_services::media_service::ThumbnailSettings> = None;
    let grouped_ref: Option<&synapse_services::media::ThumbnailSettings> = None;
    if let (Some(a), Some(b)) = (legacy_ref, grouped_ref) {
        assert_same_type(a, b);
    }
}

#[test]
fn test_push_notification_service_path_identity() {
    // Legacy flat alias `pub use push::service as push_notification_service;`
    // (synapse-services/src/lib.rs). Comparing `push::…` against itself would
    // pass even if the alias were deleted.
    let legacy_ref: Option<&synapse_services::push_notification_service::PushNotificationService> = None;
    let grouped_ref: Option<&synapse_services::push::PushNotificationService> = None;
    if let (Some(a), Some(b)) = (legacy_ref, grouped_ref) {
        assert_same_type(a, b);
    }
}

#[test]
fn test_account_registration_service_path_identity() {
    let legacy_ref: Option<&synapse_services::registration_service::RegistrationService> = None;
    let grouped_ref: Option<&synapse_services::account::RegistrationService> = None;
    if let (Some(a), Some(b)) = (legacy_ref, grouped_ref) {
        assert_same_type(a, b);
    }
}

#[test]
fn test_account_identity_service_path_identity() {
    let legacy_ref: Option<&synapse_services::account_identity_service::AccountIdentityService> = None;
    let grouped_ref: Option<&synapse_services::account::AccountIdentityService> = None;
    if let (Some(a), Some(b)) = (legacy_ref, grouped_ref) {
        assert_same_type(a, b);
    }
}

#[test]
fn test_event_notifier_path_identity() {
    let legacy_ref: Option<&synapse_services::event_notifier::EventNotifier> = None;
    let grouped_ref: Option<&synapse_services::event::EventNotifier> = None;
    if let (Some(a), Some(b)) = (legacy_ref, grouped_ref) {
        assert_same_type(a, b);
    }
}

#[test]
fn test_event_report_service_path_identity() {
    let legacy_ref: Option<&synapse_services::event_report_service::EventReportService> = None;
    let grouped_ref: Option<&synapse_services::event::EventReportService> = None;
    if let (Some(a), Some(b)) = (legacy_ref, grouped_ref) {
        assert_same_type(a, b);
    }
}

#[test]
fn test_room_service_path_identity() {
    // `RoomService` is defined in `room::service`; there was never a flat
    // `room_service` module, so the direct module path is the reference.
    let legacy_ref: Option<&synapse_services::room::service::RoomService> = None;
    let grouped_ref: Option<&synapse_services::room::RoomService> = None;
    if let (Some(a), Some(b)) = (legacy_ref, grouped_ref) {
        assert_same_type(a, b);
    }
}

#[test]
fn test_infra_feature_flag_service_path_identity() {
    let legacy_ref: Option<&synapse_services::feature_flag_service::FeatureFlagService> = None;
    let grouped_ref: Option<&synapse_services::infra::FeatureFlagService> = None;
    if let (Some(a), Some(b)) = (legacy_ref, grouped_ref) {
        assert_same_type(a, b);
    }
}

#[test]
fn test_identity_oidc_service_path_identity() {
    let legacy_ref: Option<&synapse_services::oidc_service::OidcService> = None;
    let grouped_ref: Option<&synapse_services::identity::OidcService> = None;
    if let (Some(a), Some(b)) = (legacy_ref, grouped_ref) {
        assert_same_type(a, b);
    }
}
