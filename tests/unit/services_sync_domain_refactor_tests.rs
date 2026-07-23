//! TDD tests for synapse-services sync domain grouping (P5).
//!
//! These tests verify that the new domain-grouped path
//! (`synapse_services::sync::SyncService`) resolves to the same type as the
//! legacy flat path (`synapse_services::SyncService`), ensuring the refactor
//! is behavior-preserving.

/// Compile-time type identity check: asserts that two paths resolve to the
/// same concrete type. If either path is unreachable or the types differ,
/// compilation fails (RED).
fn assert_same_type<T>(_a: &T, _b: &T) {}

#[test]
fn test_sync_service_path_identity() {
    let legacy_ref: Option<&synapse_services::SyncService> = None;
    let grouped_ref: Option<&synapse_services::sync::SyncService> = None;
    if let (Some(a), Some(b)) = (legacy_ref, grouped_ref) {
        assert_same_type(a, b);
    }
}

#[test]
fn test_sliding_sync_service_path_identity() {
    let legacy_ref: Option<&synapse_services::SlidingSyncService> = None;
    let grouped_ref: Option<&synapse_services::sync::SlidingSyncService> = None;
    if let (Some(a), Some(b)) = (legacy_ref, grouped_ref) {
        assert_same_type(a, b);
    }
}

#[test]
fn test_sync_token_path_identity() {
    let legacy_ref: Option<&synapse_services::SyncToken> = None;
    let grouped_ref: Option<&synapse_services::sync::SyncToken> = None;
    if let (Some(a), Some(b)) = (legacy_ref, grouped_ref) {
        assert_same_type(a, b);
    }
}

// =============================================================================
// P7.2 — domain grouping path-identity tests for the 7 new service domains.
//
// Each test asserts that the legacy flat root path (e.g.
// `synapse_services::MediaService`, now provided via `pub use <domain>::*;`)
// resolves to the same type as the grouped domain path (e.g.
// `synapse_services::media::MediaService`).
// =============================================================================

#[test]
fn test_media_service_path_identity() {
    let legacy_ref: Option<&synapse_services::MediaService> = None;
    let grouped_ref: Option<&synapse_services::media::MediaService> = None;
    if let (Some(a), Some(b)) = (legacy_ref, grouped_ref) {
        assert_same_type(a, b);
    }
}

#[test]
fn test_media_thumbnail_settings_path_identity() {
    let legacy_ref: Option<&synapse_services::ThumbnailSettings> = None;
    let grouped_ref: Option<&synapse_services::media::ThumbnailSettings> = None;
    if let (Some(a), Some(b)) = (legacy_ref, grouped_ref) {
        assert_same_type(a, b);
    }
}

#[test]
fn test_push_notification_service_path_identity() {
    let legacy_ref: Option<&synapse_services::PushNotificationService> = None;
    let grouped_ref: Option<&synapse_services::push::PushNotificationService> = None;
    if let (Some(a), Some(b)) = (legacy_ref, grouped_ref) {
        assert_same_type(a, b);
    }
}

#[test]
fn test_account_registration_service_path_identity() {
    let legacy_ref: Option<&synapse_services::RegistrationService> = None;
    let grouped_ref: Option<&synapse_services::account::RegistrationService> = None;
    if let (Some(a), Some(b)) = (legacy_ref, grouped_ref) {
        assert_same_type(a, b);
    }
}

#[test]
fn test_account_identity_service_path_identity() {
    let legacy_ref: Option<&synapse_services::AccountIdentityService> = None;
    let grouped_ref: Option<&synapse_services::account::AccountIdentityService> = None;
    if let (Some(a), Some(b)) = (legacy_ref, grouped_ref) {
        assert_same_type(a, b);
    }
}

#[test]
fn test_event_notifier_path_identity() {
    let legacy_ref: Option<&synapse_services::EventNotifier> = None;
    let grouped_ref: Option<&synapse_services::event::EventNotifier> = None;
    if let (Some(a), Some(b)) = (legacy_ref, grouped_ref) {
        assert_same_type(a, b);
    }
}

#[test]
fn test_event_report_service_path_identity() {
    let legacy_ref: Option<&synapse_services::EventReportService> = None;
    let grouped_ref: Option<&synapse_services::event::EventReportService> = None;
    if let (Some(a), Some(b)) = (legacy_ref, grouped_ref) {
        assert_same_type(a, b);
    }
}

#[test]
fn test_room_service_path_identity() {
    let legacy_ref: Option<&synapse_services::RoomService> = None;
    let grouped_ref: Option<&synapse_services::room::RoomService> = None;
    if let (Some(a), Some(b)) = (legacy_ref, grouped_ref) {
        assert_same_type(a, b);
    }
}

#[test]
fn test_room_directory_service_path_identity() {
    let legacy_ref: Option<&synapse_services::DirectoryService> = None;
    let grouped_ref: Option<&synapse_services::room::DirectoryService> = None;
    if let (Some(a), Some(b)) = (legacy_ref, grouped_ref) {
        assert_same_type(a, b);
    }
}

#[test]
fn test_infra_feature_flag_service_path_identity() {
    let legacy_ref: Option<&synapse_services::FeatureFlagService> = None;
    let grouped_ref: Option<&synapse_services::infra::FeatureFlagService> = None;
    if let (Some(a), Some(b)) = (legacy_ref, grouped_ref) {
        assert_same_type(a, b);
    }
}

#[test]
fn test_identity_oidc_service_path_identity() {
    let legacy_ref: Option<&synapse_services::OidcService> = None;
    let grouped_ref: Option<&synapse_services::identity::OidcService> = None;
    if let (Some(a), Some(b)) = (legacy_ref, grouped_ref) {
        assert_same_type(a, b);
    }
}
