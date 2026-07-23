//! TDD tests for synapse-storage admin domain grouping (P1).
//!
//! These tests verify that the new domain-grouped path
//! (`synapse_storage::admin::AdminFederationStorage`) resolves to the same
//! type as the legacy flat path (`synapse_storage::AdminFederationStorage`),
//! ensuring the refactor is behavior-preserving.

/// Compile-time type identity check: asserts that two paths resolve to the
/// same concrete type. If either path is unreachable or the types differ,
/// compilation fails (RED).
/// `?Sized` allows this to work with trait objects (`&dyn Trait`) as well.
fn assert_same_type<T: ?Sized>(_a: &T, _b: &T) {}

#[test]
fn test_admin_federation_storage_path_identity() {
    let _legacy: synapse_storage::AdminFederationStorage;
    let _grouped: synapse_storage::admin::AdminFederationStorage;

    // If both paths resolve to the same type, this compiles.
    // If `admin` module doesn't exist yet, this is RED.
    let legacy_ref: Option<&synapse_storage::AdminFederationStorage> = None;
    let grouped_ref: Option<&synapse_storage::admin::AdminFederationStorage> = None;
    if let (Some(a), Some(b)) = (legacy_ref, grouped_ref) {
        assert_same_type(a, b);
    }
}

#[test]
fn test_admin_media_storage_path_identity() {
    let legacy_ref: Option<&synapse_storage::AdminMediaStorage> = None;
    let grouped_ref: Option<&synapse_storage::admin::AdminMediaStorage> = None;
    if let (Some(a), Some(b)) = (legacy_ref, grouped_ref) {
        assert_same_type(a, b);
    }
}

#[test]
fn test_audit_event_storage_path_identity() {
    let legacy_ref: Option<&synapse_storage::AuditEventStorage> = None;
    let grouped_ref: Option<&synapse_storage::admin::AuditEventStorage> = None;
    if let (Some(a), Some(b)) = (legacy_ref, grouped_ref) {
        assert_same_type(a, b);
    }
}

// --- P2: e2ee domain grouping ---

#[test]
fn test_dehydrated_device_storage_path_identity() {
    let legacy_ref: Option<&synapse_storage::DehydratedDeviceStorage> = None;
    let grouped_ref: Option<&synapse_storage::e2ee::DehydratedDeviceStorage> = None;
    if let (Some(a), Some(b)) = (legacy_ref, grouped_ref) {
        assert_same_type(a, b);
    }
}

#[test]
fn test_e2ee_audit_storage_path_identity() {
    let legacy_ref: Option<&synapse_storage::E2eeAuditStorage> = None;
    let grouped_ref: Option<&synapse_storage::e2ee::E2eeAuditStorage> = None;
    if let (Some(a), Some(b)) = (legacy_ref, grouped_ref) {
        assert_same_type(a, b);
    }
}

// --- P4: auth domain grouping ---

#[test]
fn test_user_storage_path_identity() {
    let legacy_ref: Option<&synapse_storage::UserStorage> = None;
    let grouped_ref: Option<&synapse_storage::auth::UserStorage> = None;
    if let (Some(a), Some(b)) = (legacy_ref, grouped_ref) {
        assert_same_type(a, b);
    }
}

#[test]
fn test_device_storage_path_identity() {
    let legacy_ref: Option<&synapse_storage::DeviceStorage> = None;
    let grouped_ref: Option<&synapse_storage::auth::DeviceStorage> = None;
    if let (Some(a), Some(b)) = (legacy_ref, grouped_ref) {
        assert_same_type(a, b);
    }
}

#[test]
fn test_access_token_storage_path_identity() {
    let legacy_ref: Option<&synapse_storage::AccessTokenStorage> = None;
    let grouped_ref: Option<&synapse_storage::auth::AccessTokenStorage> = None;
    if let (Some(a), Some(b)) = (legacy_ref, grouped_ref) {
        assert_same_type(a, b);
    }
}

// =============================================================================
// P7.1: New domain groupings (media, push, event, account, moderation, sync,
//       space, infra, application, oidc)
// =============================================================================

// --- media domain grouping (media, media_quota) ---

#[test]
fn test_media_quota_storage_path_identity() {
    let legacy_ref: Option<&synapse_storage::MediaQuotaStorage> = None;
    let grouped_ref: Option<&synapse_storage::media::MediaQuotaStorage> = None;
    if let (Some(a), Some(b)) = (legacy_ref, grouped_ref) {
        assert_same_type(a, b);
    }
}

#[test]
fn test_quarantined_media_change_store_api_path_identity() {
    let legacy_ref: Option<&dyn synapse_storage::QuarantinedMediaChangeStoreApi> = None;
    let grouped_ref: Option<&dyn synapse_storage::media::QuarantinedMediaChangeStoreApi> = None;
    if let (Some(a), Some(b)) = (legacy_ref, grouped_ref) {
        assert_same_type(a, b);
    }
}

// --- push domain grouping (push, push_notification) ---

#[test]
fn test_push_storage_path_identity() {
    let legacy_ref: Option<&synapse_storage::PushStorage> = None;
    let grouped_ref: Option<&synapse_storage::push::PushStorage> = None;
    if let (Some(a), Some(b)) = (legacy_ref, grouped_ref) {
        assert_same_type(a, b);
    }
}

#[test]
fn test_push_notification_storage_path_identity() {
    let legacy_ref: Option<&synapse_storage::PushNotificationStorage> = None;
    let grouped_ref: Option<&synapse_storage::push::PushNotificationStorage> = None;
    if let (Some(a), Some(b)) = (legacy_ref, grouped_ref) {
        assert_same_type(a, b);
    }
}

// --- event domain grouping (event) ---

#[test]
fn test_event_storage_path_identity() {
    let legacy_ref: Option<&synapse_storage::EventStorage> = None;
    let grouped_ref: Option<&synapse_storage::event::EventStorage> = None;
    if let (Some(a), Some(b)) = (legacy_ref, grouped_ref) {
        assert_same_type(a, b);
    }
}

// --- account domain grouping (account_data, qr_login, rendezvous) ---

#[test]
fn test_account_data_storage_path_identity() {
    let legacy_ref: Option<&synapse_storage::AccountDataStorage> = None;
    let grouped_ref: Option<&synapse_storage::account::AccountDataStorage> = None;
    if let (Some(a), Some(b)) = (legacy_ref, grouped_ref) {
        assert_same_type(a, b);
    }
}

#[test]
fn test_qr_login_storage_path_identity() {
    let legacy_ref: Option<&synapse_storage::QrLoginStorage> = None;
    let grouped_ref: Option<&synapse_storage::account::QrLoginStorage> = None;
    if let (Some(a), Some(b)) = (legacy_ref, grouped_ref) {
        assert_same_type(a, b);
    }
}

// --- moderation domain grouping (moderation, invite_blocklist) ---

#[test]
fn test_moderation_storage_path_identity() {
    let legacy_ref: Option<&synapse_storage::ModerationStorage> = None;
    let grouped_ref: Option<&synapse_storage::moderation::ModerationStorage> = None;
    if let (Some(a), Some(b)) = (legacy_ref, grouped_ref) {
        assert_same_type(a, b);
    }
}

// --- sync domain grouping (sliding_sync, search_index) ---

#[test]
fn test_sliding_sync_storage_path_identity() {
    let legacy_ref: Option<&synapse_storage::SlidingSyncStorage> = None;
    let grouped_ref: Option<&synapse_storage::sync::SlidingSyncStorage> = None;
    if let (Some(a), Some(b)) = (legacy_ref, grouped_ref) {
        assert_same_type(a, b);
    }
}

// --- space domain grouping (space, sticky_event) ---

#[test]
fn test_space_storage_path_identity() {
    let legacy_ref: Option<&synapse_storage::SpaceStorage> = None;
    let grouped_ref: Option<&synapse_storage::space::SpaceStorage> = None;
    if let (Some(a), Some(b)) = (legacy_ref, grouped_ref) {
        assert_same_type(a, b);
    }
}

// --- infra domain grouping (background_update, feature_flags,
//     federation_blacklist, federation_queue, maintenance, monitoring,
//     performance, rate_limit, schema_validator) ---

#[test]
fn test_background_update_storage_path_identity() {
    let legacy_ref: Option<&synapse_storage::BackgroundUpdateStorage> = None;
    let grouped_ref: Option<&synapse_storage::infra::BackgroundUpdateStorage> = None;
    if let (Some(a), Some(b)) = (legacy_ref, grouped_ref) {
        assert_same_type(a, b);
    }
}

#[test]
fn test_federation_queue_store_api_path_identity() {
    let legacy_ref: Option<&dyn synapse_storage::FederationQueueStoreApi> = None;
    let grouped_ref: Option<&dyn synapse_storage::infra::FederationQueueStoreApi> = None;
    if let (Some(a), Some(b)) = (legacy_ref, grouped_ref) {
        assert_same_type(a, b);
    }
}

// --- application domain grouping (application_service) ---

#[test]
fn test_application_service_storage_path_identity() {
    let legacy_ref: Option<&synapse_storage::ApplicationServiceStorage> = None;
    let grouped_ref: Option<&synapse_storage::application::ApplicationServiceStorage> = None;
    if let (Some(a), Some(b)) = (legacy_ref, grouped_ref) {
        assert_same_type(a, b);
    }
}

// --- oidc domain grouping (oauth_client_storage, oidc_session_storage,
//     oidc_user_mapping) ---

#[test]
fn test_oidc_user_mapping_storage_path_identity() {
    let legacy_ref: Option<&synapse_storage::OidcUserMappingStorage> = None;
    let grouped_ref: Option<&synapse_storage::oidc::OidcUserMappingStorage> = None;
    if let (Some(a), Some(b)) = (legacy_ref, grouped_ref) {
        assert_same_type(a, b);
    }
}

#[test]
fn test_oauth_client_store_api_path_identity() {
    let legacy_ref: Option<&dyn synapse_storage::OAuthClientStoreApi> = None;
    let grouped_ref: Option<&dyn synapse_storage::oidc::OAuthClientStoreApi> = None;
    if let (Some(a), Some(b)) = (legacy_ref, grouped_ref) {
        assert_same_type(a, b);
    }
}
