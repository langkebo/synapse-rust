//! TDD tests for synapse-storage remaining domain grouping (P7.3).
//!
//! These tests verify that the new domain-grouped paths resolve to the same
//! types as the (legacy or newly-added) flat paths, ensuring the refactor that
//! moves explicit flat re-exports from `lib.rs` into domain modules is
//! behavior-preserving.
//!
//! Domains covered: sync (filter, presence), infra (worker, pruning,
//! schema_health_check, trigram_ranking, server_notification), media
//! (url_preview_storage, voice), room (relations, retention, room_summary,
//! room_tag, beacon, widget, burn_after_read, friend_room), auth
//! (email_verification, refresh_token, registration_token, saml, cas,
//! privacy), application (module), plus the new ai and rtc domain groups.

/// Compile-time type identity check: asserts that two paths resolve to the
/// same concrete type. If either path is unreachable or the types differ,
/// compilation fails (RED).
/// `?Sized` allows this to work with trait objects (`&dyn Trait`) as well.
fn assert_same_type<T: ?Sized>(_a: &T, _b: &T) {}

// =============================================================================
// sync domain grouping (filter, presence)
// =============================================================================

#[test]
fn test_filter_path_identity() {
    let legacy: Option<synapse_storage::Filter> = None;
    let grouped: Option<synapse_storage::sync::Filter> = None;
    if let (Some(a), Some(b)) = (legacy, grouped) {
        assert_same_type(&a, &b);
    }
}

#[test]
fn test_filter_storage_path_identity() {
    let legacy_ref: Option<&synapse_storage::FilterStorage> = None;
    let grouped_ref: Option<&synapse_storage::sync::FilterStorage> = None;
    if let (Some(a), Some(b)) = (legacy_ref, grouped_ref) {
        assert_same_type(a, b);
    }
}

#[test]
fn test_presence_storage_path_identity() {
    let legacy_ref: Option<&synapse_storage::PresenceStorage> = None;
    let grouped_ref: Option<&synapse_storage::sync::PresenceStorage> = None;
    if let (Some(a), Some(b)) = (legacy_ref, grouped_ref) {
        assert_same_type(a, b);
    }
}

#[test]
fn test_presence_snapshot_path_identity() {
    let legacy: Option<synapse_storage::PresenceSnapshot> = None;
    let grouped: Option<synapse_storage::sync::PresenceSnapshot> = None;
    if let (Some(a), Some(b)) = (legacy, grouped) {
        assert_same_type(&a, &b);
    }
}

// =============================================================================
// infra domain grouping (worker, pruning, schema_health_check,
// trigram_ranking, server_notification)
// =============================================================================

#[test]
fn test_worker_storage_path_identity() {
    let legacy_ref: Option<&synapse_storage::WorkerStorage> = None;
    let grouped_ref: Option<&synapse_storage::infra::WorkerStorage> = None;
    if let (Some(a), Some(b)) = (legacy_ref, grouped_ref) {
        assert_same_type(a, b);
    }
}

#[test]
fn test_worker_store_api_path_identity() {
    let legacy_ref: Option<&dyn synapse_storage::WorkerStoreApi> = None;
    let grouped_ref: Option<&dyn synapse_storage::infra::WorkerStoreApi> = None;
    if let (Some(a), Some(b)) = (legacy_ref, grouped_ref) {
        assert_same_type(a, b);
    }
}

#[test]
fn test_worker_type_path_identity() {
    // `WorkerType` is an enum re-exported via the worker module.
    let legacy: Option<synapse_storage::WorkerType> = None;
    let grouped: Option<synapse_storage::infra::WorkerType> = None;
    if let (Some(a), Some(b)) = (legacy, grouped) {
        assert_same_type(&a, &b);
    }
}

#[test]
fn test_trigram_ranking_path_identity() {
    let legacy: Option<synapse_storage::TrigramRanking> = None;
    let grouped: Option<synapse_storage::infra::TrigramRanking> = None;
    if let (Some(a), Some(b)) = (legacy, grouped) {
        assert_same_type(&a, &b);
    }
}

#[test]
fn test_health_check_result_path_identity() {
    let legacy: Option<synapse_storage::HealthCheckResult> = None;
    let grouped: Option<synapse_storage::infra::HealthCheckResult> = None;
    if let (Some(a), Some(b)) = (legacy, grouped) {
        assert_same_type(&a, &b);
    }
}

#[cfg(feature = "server-notifications")]
#[test]
fn test_server_notification_storage_path_identity() {
    let legacy_ref: Option<&synapse_storage::ServerNotificationStorage> = None;
    let grouped_ref: Option<&synapse_storage::infra::ServerNotificationStorage> = None;
    if let (Some(a), Some(b)) = (legacy_ref, grouped_ref) {
        assert_same_type(a, b);
    }
}

// =============================================================================
// media domain grouping (url_preview_storage, voice)
// =============================================================================

#[test]
fn test_url_preview_store_api_path_identity() {
    let legacy_ref: Option<&dyn synapse_storage::UrlPreviewStoreApi> = None;
    let grouped_ref: Option<&dyn synapse_storage::media::UrlPreviewStoreApi> = None;
    if let (Some(a), Some(b)) = (legacy_ref, grouped_ref) {
        assert_same_type(a, b);
    }
}

#[cfg(feature = "voice-extended")]
#[test]
fn test_voice_storage_path_identity() {
    let legacy_ref: Option<&synapse_storage::VoiceStorage> = None;
    let grouped_ref: Option<&synapse_storage::media::VoiceStorage> = None;
    if let (Some(a), Some(b)) = (legacy_ref, grouped_ref) {
        assert_same_type(a, b);
    }
}

// =============================================================================
// room domain grouping (relations, retention, room_summary, room_tag, beacon,
// widget, burn_after_read, friend_room)
// =============================================================================

#[test]
fn test_event_relation_path_identity() {
    let legacy: Option<synapse_storage::EventRelation> = None;
    let grouped: Option<synapse_storage::room::EventRelation> = None;
    if let (Some(a), Some(b)) = (legacy, grouped) {
        assert_same_type(&a, &b);
    }
}

#[test]
fn test_relations_storage_path_identity() {
    let legacy_ref: Option<&synapse_storage::RelationsStorage> = None;
    let grouped_ref: Option<&synapse_storage::room::RelationsStorage> = None;
    if let (Some(a), Some(b)) = (legacy_ref, grouped_ref) {
        assert_same_type(a, b);
    }
}

#[test]
fn test_room_summary_storage_path_identity() {
    let legacy_ref: Option<&synapse_storage::RoomSummaryStorage> = None;
    let grouped_ref: Option<&synapse_storage::room::RoomSummaryStorage> = None;
    if let (Some(a), Some(b)) = (legacy_ref, grouped_ref) {
        assert_same_type(a, b);
    }
}

#[test]
fn test_room_tag_path_identity() {
    let legacy: Option<synapse_storage::RoomTag> = None;
    let grouped: Option<synapse_storage::room::RoomTag> = None;
    if let (Some(a), Some(b)) = (legacy, grouped) {
        assert_same_type(&a, &b);
    }
}

#[test]
fn test_room_tag_storage_path_identity() {
    let legacy_ref: Option<&synapse_storage::RoomTagStorage> = None;
    let grouped_ref: Option<&synapse_storage::room::RoomTagStorage> = None;
    if let (Some(a), Some(b)) = (legacy_ref, grouped_ref) {
        assert_same_type(a, b);
    }
}

#[test]
fn test_room_retention_policy_path_identity() {
    let legacy: Option<synapse_storage::RoomRetentionPolicy> = None;
    let grouped: Option<synapse_storage::room::RoomRetentionPolicy> = None;
    if let (Some(a), Some(b)) = (legacy, grouped) {
        assert_same_type(&a, &b);
    }
}

#[cfg(feature = "beacons")]
#[test]
fn test_beacon_storage_path_identity() {
    let legacy_ref: Option<&synapse_storage::BeaconStorage> = None;
    let grouped_ref: Option<&synapse_storage::room::BeaconStorage> = None;
    if let (Some(a), Some(b)) = (legacy_ref, grouped_ref) {
        assert_same_type(a, b);
    }
}

#[cfg(feature = "widgets")]
#[test]
fn test_widget_path_identity() {
    let legacy: Option<synapse_storage::Widget> = None;
    let grouped: Option<synapse_storage::room::Widget> = None;
    if let (Some(a), Some(b)) = (legacy, grouped) {
        assert_same_type(&a, &b);
    }
}

#[cfg(feature = "burn-after-read")]
#[test]
fn test_burn_after_read_storage_path_identity() {
    let legacy_ref: Option<&synapse_storage::BurnAfterReadStorage> = None;
    let grouped_ref: Option<&synapse_storage::room::BurnAfterReadStorage> = None;
    if let (Some(a), Some(b)) = (legacy_ref, grouped_ref) {
        assert_same_type(a, b);
    }
}

#[cfg(feature = "friends")]
#[test]
fn test_friend_room_storage_path_identity() {
    let legacy_ref: Option<&synapse_storage::FriendRoomStorage> = None;
    let grouped_ref: Option<&synapse_storage::room::FriendRoomStorage> = None;
    if let (Some(a), Some(b)) = (legacy_ref, grouped_ref) {
        assert_same_type(a, b);
    }
}

// =============================================================================
// auth domain grouping (email_verification, refresh_token, registration_token,
// saml, cas, privacy)
// =============================================================================

#[test]
fn test_email_verification_storage_path_identity() {
    let legacy_ref: Option<&synapse_storage::EmailVerificationStorage> = None;
    let grouped_ref: Option<&synapse_storage::auth::EmailVerificationStorage> = None;
    if let (Some(a), Some(b)) = (legacy_ref, grouped_ref) {
        assert_same_type(a, b);
    }
}

#[test]
fn test_refresh_token_path_identity() {
    let legacy: Option<synapse_storage::RefreshToken> = None;
    let grouped: Option<synapse_storage::auth::RefreshToken> = None;
    if let (Some(a), Some(b)) = (legacy, grouped) {
        assert_same_type(&a, &b);
    }
}

#[test]
fn test_refresh_token_storage_path_identity() {
    let legacy_ref: Option<&synapse_storage::RefreshTokenStorage> = None;
    let grouped_ref: Option<&synapse_storage::auth::RefreshTokenStorage> = None;
    if let (Some(a), Some(b)) = (legacy_ref, grouped_ref) {
        assert_same_type(a, b);
    }
}

#[test]
fn test_registration_token_storage_path_identity() {
    let legacy_ref: Option<&synapse_storage::RegistrationTokenStorage> = None;
    let grouped_ref: Option<&synapse_storage::auth::RegistrationTokenStorage> = None;
    if let (Some(a), Some(b)) = (legacy_ref, grouped_ref) {
        assert_same_type(a, b);
    }
}

#[cfg(feature = "saml-sso")]
#[test]
fn test_saml_storage_path_identity() {
    let legacy_ref: Option<&synapse_storage::SamlStorage> = None;
    let grouped_ref: Option<&synapse_storage::auth::SamlStorage> = None;
    if let (Some(a), Some(b)) = (legacy_ref, grouped_ref) {
        assert_same_type(a, b);
    }
}

#[cfg(feature = "cas-sso")]
#[test]
fn test_cas_storage_path_identity() {
    let legacy_ref: Option<&synapse_storage::CasStorage> = None;
    let grouped_ref: Option<&synapse_storage::auth::CasStorage> = None;
    if let (Some(a), Some(b)) = (legacy_ref, grouped_ref) {
        assert_same_type(a, b);
    }
}

#[cfg(feature = "privacy-ext")]
#[test]
fn test_privacy_storage_path_identity() {
    let legacy_ref: Option<&synapse_storage::PrivacyStorage> = None;
    let grouped_ref: Option<&synapse_storage::auth::PrivacyStorage> = None;
    if let (Some(a), Some(b)) = (legacy_ref, grouped_ref) {
        assert_same_type(a, b);
    }
}

// =============================================================================
// application domain grouping (module)
// =============================================================================

#[test]
fn test_module_path_identity() {
    let legacy: Option<synapse_storage::Module> = None;
    let grouped: Option<synapse_storage::application::Module> = None;
    if let (Some(a), Some(b)) = (legacy, grouped) {
        assert_same_type(&a, &b);
    }
}

// =============================================================================
// New domain groups: ai (openclaw-routes) and rtc (voip-tracking)
// =============================================================================

#[cfg(feature = "openclaw-routes")]
#[test]
fn test_openclaw_storage_path_identity() {
    let legacy_ref: Option<&synapse_storage::OpenClawStorage> = None;
    let grouped_ref: Option<&synapse_storage::ai::OpenClawStorage> = None;
    if let (Some(a), Some(b)) = (legacy_ref, grouped_ref) {
        assert_same_type(a, b);
    }
}

#[cfg(feature = "openclaw-routes")]
#[test]
fn test_ai_connection_path_identity() {
    let legacy: Option<synapse_storage::AiConnection> = None;
    let grouped: Option<synapse_storage::ai::AiConnection> = None;
    if let (Some(a), Some(b)) = (legacy, grouped) {
        assert_same_type(&a, &b);
    }
}

#[cfg(feature = "voip-tracking")]
#[test]
fn test_call_session_storage_path_identity() {
    let legacy_ref: Option<&synapse_storage::CallSessionStorage> = None;
    let grouped_ref: Option<&synapse_storage::rtc::CallSessionStorage> = None;
    if let (Some(a), Some(b)) = (legacy_ref, grouped_ref) {
        assert_same_type(a, b);
    }
}

#[cfg(feature = "voip-tracking")]
#[test]
fn test_matrix_rtc_storage_path_identity() {
    let legacy_ref: Option<&synapse_storage::MatrixRTCStorage> = None;
    let grouped_ref: Option<&synapse_storage::rtc::MatrixRTCStorage> = None;
    if let (Some(a), Some(b)) = (legacy_ref, grouped_ref) {
        assert_same_type(a, b);
    }
}
