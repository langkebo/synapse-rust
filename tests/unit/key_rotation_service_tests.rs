// Key Rotation Service Tests - Service Layer Coverage
//
// Closes the P-095 service-layer test gap for the key_rotation module.
// Two service surfaces are exercised:
//
// 1. `synapse_e2ee::key_rotation` — the E2EE key rotation service types
//    (`KeyRotationConfig`, `KeyRotationLog`, `RotationStatus`). These had
//    ZERO tests. The pure-logic surface (defaults, clone semantics, serde
//    round-trips) is covered here without a database.
//
// 2. `synapse_services::infra::FederationKeyRotationService` — the route-facing
//    facade used by `src/web/routes/key_rotation.rs`. Its inline test module
//    already covers most methods; this file adds coverage for the two
//    UNTESTED methods (`set_manager_config_value`, `set_storage_config_value`)
//    plus complementary edge cases (rotate with requested key id, revoke
//    clears the seeded current key). Mocks come from the `test-utils` feature
//    (`InMemoryKeyRotationStorage`, `InMemoryKeyRotationManager`).

use std::sync::Arc;

use chrono::Utc;
use synapse_e2ee::key_rotation::{KeyRotationConfig, KeyRotationLog, RotationStatus};
use synapse_e2ee::test_mocks::InMemoryKeyRotationStorage;
use synapse_federation::test_mocks::InMemoryKeyRotationManager;
use synapse_federation::KeyRotationManagerApi;
use synapse_services::federation_key_rotation_service::FederationKeyRotationService;

// ============================================================================
// Part A — E2EE KeyRotationConfig (pure logic, previously zero tests)
// ============================================================================

#[test]
fn test_key_rotation_config_default_values() {
    // Defaults mirror the private constants in service.rs:
    //   DEFAULT_OLM_ROTATION_DAYS = 7
    //   DEFAULT_MEGOLM_ROTATION_MESSAGES = 100
    //   DEFAULT_MAX_SESSION_AGE_DAYS = 90
    //   enable_auto_rotation = true
    let config = KeyRotationConfig::default();
    assert_eq!(config.olm_rotation_days, 7);
    assert_eq!(config.megolm_rotation_messages, 100);
    assert_eq!(config.max_session_age_days, 90);
    assert!(config.enable_auto_rotation);
}

#[test]
fn test_key_rotation_config_clone_is_independent() {
    // KeyRotationConfig derives Clone; mutating a clone must not affect the
    // original. This guards the `Arc<RwLock<KeyRotationConfig>>` pattern in
    // KeyRotationService::new, where config snapshots are taken by clone.
    let original = KeyRotationConfig::default();
    let mut clone = original.clone();
    clone.olm_rotation_days = 30;
    clone.enable_auto_rotation = false;

    assert_eq!(original.olm_rotation_days, 7, "original must be untouched by clone mutation");
    assert!(original.enable_auto_rotation, "original auto-rotation flag must remain true");
    assert_eq!(clone.olm_rotation_days, 30);
    assert!(!clone.enable_auto_rotation);
}

#[test]
fn test_key_rotation_config_auto_rotation_toggle() {
    let mut config = KeyRotationConfig::default();
    assert!(config.enable_auto_rotation);

    config.enable_auto_rotation = false;
    assert!(!config.enable_auto_rotation);

    config.enable_auto_rotation = true;
    assert!(config.enable_auto_rotation);
}

#[test]
#[allow(clippy::field_reassign_with_default)]
fn test_key_rotation_config_all_fields_mutable() {
    let mut config = KeyRotationConfig::default();
    config.olm_rotation_days = 14;
    config.megolm_rotation_messages = 200;
    config.max_session_age_days = 180;

    assert_eq!(config.olm_rotation_days, 14);
    assert_eq!(config.megolm_rotation_messages, 200);
    assert_eq!(config.max_session_age_days, 180);
}

// ============================================================================
// Part B — E2EE KeyRotationLog serialization (Serialize + Deserialize)
// ============================================================================

#[test]
fn test_key_rotation_log_serialization_round_trip() {
    let now = Utc::now();
    let log = KeyRotationLog {
        id: 42,
        user_id: "@alice:example.com".to_string(),
        device_id: "DEVICE1".to_string(),
        room_id: Some("!room:example.com".to_string()),
        rotation_type: "megolm".to_string(),
        old_key_id: Some("ed25519:old".to_string()),
        new_key_id: "ed25519:new".to_string(),
        reason: Some("member_left".to_string()),
        rotated_at: now,
    };

    let json = serde_json::to_string(&log).expect("KeyRotationLog must serialize");
    let deserialized: KeyRotationLog = serde_json::from_str(&json).expect("KeyRotationLog must deserialize");

    assert_eq!(deserialized.id, 42);
    assert_eq!(deserialized.user_id, "@alice:example.com");
    assert_eq!(deserialized.device_id, "DEVICE1");
    assert_eq!(deserialized.room_id.as_deref(), Some("!room:example.com"));
    assert_eq!(deserialized.rotation_type, "megolm");
    assert_eq!(deserialized.old_key_id.as_deref(), Some("ed25519:old"));
    assert_eq!(deserialized.new_key_id, "ed25519:new");
    assert_eq!(deserialized.reason.as_deref(), Some("member_left"));
    assert_eq!(deserialized.rotated_at, now);
}

#[test]
fn test_key_rotation_log_serialization_with_optional_fields_none() {
    let log = KeyRotationLog {
        id: 1,
        user_id: "@bob:example.com".to_string(),
        device_id: "DEV".to_string(),
        room_id: None,
        rotation_type: "olm".to_string(),
        old_key_id: None,
        new_key_id: "ed25519:1".to_string(),
        reason: None,
        rotated_at: Utc::now(),
    };

    let json = serde_json::to_string(&log).expect("serialize");
    let parsed: serde_json::Value = serde_json::from_str(&json).expect("parse");
    // Option fields serialize to null when None (serde default).
    assert!(parsed["room_id"].is_null());
    assert!(parsed["old_key_id"].is_null());
    assert!(parsed["reason"].is_null());
    assert_eq!(parsed["new_key_id"].as_str(), Some("ed25519:1"));
}

// ============================================================================
// Part C — E2EE RotationStatus serialization (Serialize + Deserialize)
// ============================================================================

#[test]
fn test_rotation_status_serialization_round_trip() {
    let now = Utc::now();
    let status = RotationStatus { total_sessions: 10, rotated_sessions: 7, last_rotation: Some(now) };

    let json = serde_json::to_string(&status).expect("RotationStatus must serialize");
    let deserialized: RotationStatus = serde_json::from_str(&json).expect("RotationStatus must deserialize");

    assert_eq!(deserialized.total_sessions, 10);
    assert_eq!(deserialized.rotated_sessions, 7);
    assert_eq!(deserialized.last_rotation, Some(now));
}

#[test]
fn test_rotation_status_serialization_with_none_last_rotation() {
    let status = RotationStatus { total_sessions: 0, rotated_sessions: 0, last_rotation: None };

    let json = serde_json::to_string(&status).expect("serialize");
    let parsed: serde_json::Value = serde_json::from_str(&json).expect("parse");
    assert!(parsed["last_rotation"].is_null());
    assert_eq!(parsed["total_sessions"].as_i64(), Some(0));
    assert_eq!(parsed["rotated_sessions"].as_i64(), Some(0));
}

// ============================================================================
// Part D — FederationKeyRotationService (route-facing facade)
//           UNTESTED methods + complementary edge cases.
// ============================================================================

fn service_with_mocks() -> (FederationKeyRotationService, InMemoryKeyRotationManager, InMemoryKeyRotationStorage) {
    let manager = InMemoryKeyRotationManager::new();
    let storage = InMemoryKeyRotationStorage::new();
    let svc = FederationKeyRotationService::new(Arc::new(manager.clone()), Arc::new(storage.clone()));
    (svc, manager, storage)
}

#[tokio::test]
async fn test_set_manager_config_value_accepts_arbitrary_keys() {
    // set_manager_config_value delegates to manager.set_rotation_config_value.
    // Previously UNTESTED. The mock stores values without error.
    let (svc, _manager, _storage) = service_with_mocks();

    svc.set_manager_config_value("rotation_interval_days", 7).await.expect("rotation_interval_days");
    svc.set_manager_config_value("rotation_threshold_days", 1).await.expect("rotation_threshold_days");
    svc.set_manager_config_value("grace_period_minutes", 5).await.expect("grace_period_minutes");
}

#[tokio::test]
async fn test_set_storage_config_value_round_trip_via_interval_ms() {
    // set_storage_config_value delegates to storage.set_rotation_config.
    // Previously UNTESTED. Verify by reading back through get_interval_ms,
    // which reads the same "interval_ms" storage key.
    let (svc, _manager, _storage) = service_with_mocks();
    assert!(svc.get_interval_ms().await.expect("get_interval_ms").is_none());

    svc.set_storage_config_value("interval_ms", 3_600_000).await.expect("set_storage_config_value");
    assert_eq!(svc.get_interval_ms().await.expect("get_interval_ms"), Some(3_600_000));
}

#[tokio::test]
async fn test_set_storage_config_value_for_non_interval_key_does_not_affect_interval_ms() {
    // Writing a different storage key must not leak into interval_ms reads.
    let (svc, _manager, _storage) = service_with_mocks();
    svc.set_storage_config_value("olm_rotation_days", 14).await.expect("set_storage_config_value");
    assert!(svc.get_interval_ms().await.expect("get_interval_ms").is_none());
}

#[tokio::test]
async fn test_rotate_keys_with_requested_key_id_succeeds() {
    // The inline test only exercises rotate_keys(None). This covers the
    // Some(key_id) path that the route handler forwards from the request body.
    let (svc, manager, _storage) = service_with_mocks();

    let has_new_key = svc.rotate_keys(Some("ed25519:requested".to_string())).await.expect("rotate_keys");
    assert!(has_new_key, "has_new_key must be true after rotation");

    let current = manager.get_current_key().await.expect("get_current_key");
    assert!(current.is_some(), "manager must hold a current key after rotate");
}

#[tokio::test]
async fn test_revoke_key_clears_seeded_current_key() {
    // The inline test only asserts revoke returns 1 on an empty manager.
    // This verifies revoke actually clears a previously-seeded current key
    // (the security-critical guarantee of revocation).
    let (svc, manager, _storage) = service_with_mocks();

    // Seed a current key by rotating.
    svc.rotate_keys(None).await.expect("rotate");
    let before = manager.get_current_key().await.expect("get_current_key");
    assert!(before.is_some(), "current key must exist after rotate");

    let revoked = svc.revoke_key("ed25519:old", Some("compromised")).await.expect("revoke_key");
    assert_eq!(revoked, 1, "revoke must report 1 affected row");

    let after = manager.get_current_key().await.expect("get_current_key");
    assert!(after.is_none(), "current key must be cleared after revoke");
}

#[tokio::test]
async fn test_set_rotation_interval_ms_persists_and_reads_back() {
    // Complementary to the inline round-trip test: verifies the value survives
    // a second overwrite with a different value (not just first write).
    let (svc, _manager, _storage) = service_with_mocks();

    svc.set_rotation_interval_ms(1_000).await.expect("first set");
    assert_eq!(svc.get_interval_ms().await.expect("read"), Some(1_000));

    svc.set_rotation_interval_ms(9_000_000).await.expect("second set");
    assert_eq!(svc.get_interval_ms().await.expect("read"), Some(9_000_000));
}

#[tokio::test]
async fn test_get_rotation_status_reflects_seeded_last_rotation() {
    // The route's GET /status handler destructures (status, last_rotation).
    // Verify the service returns last_rotation from storage and status from
    // the manager as a JSON value containing rotation_enabled.
    let manager = InMemoryKeyRotationManager::new();
    let storage = InMemoryKeyRotationStorage::new();
    storage.seed_last_rotation_ts("@carol:example.com", 1_700_000_000_000).await;
    let svc = FederationKeyRotationService::new(Arc::new(manager), Arc::new(storage));

    let (status, last_rotation) = svc.get_rotation_status("@carol:example.com").await.expect("get_rotation_status");
    assert_eq!(last_rotation, Some(1_700_000_000_000));
    assert_eq!(status["rotation_enabled"], serde_json::json!(true));
}

#[tokio::test]
async fn test_get_rotation_status_for_unknown_user_returns_none_last_rotation() {
    let (svc, _manager, _storage) = service_with_mocks();
    let (_status, last_rotation) = svc.get_rotation_status("@nobody:example.com").await.expect("get_rotation_status");
    assert_eq!(last_rotation, None);
}

#[tokio::test]
async fn test_get_rotation_history_returns_seeded_entries() {
    let manager = InMemoryKeyRotationManager::new();
    let storage = InMemoryKeyRotationStorage::new();
    let history = vec![
        (Some("ed25519:1".to_string()), Some(1_700_000_000_000_i64)),
        (Some("ed25519:2".to_string()), Some(1_700_010_000_000_i64)),
    ];
    storage.seed_device_history("@dave:example.com", "DEV_X", history).await;
    let svc = FederationKeyRotationService::new(Arc::new(manager), Arc::new(storage));

    let result = svc.get_rotation_history("@dave:example.com", "DEV_X").await.expect("get_rotation_history");
    assert_eq!(result.len(), 2);
    assert_eq!(result[0].0.as_deref(), Some("ed25519:1"));
    assert_eq!(result[1].1, Some(1_700_010_000_000));
}

#[tokio::test]
async fn test_get_rotation_history_empty_for_unknown_device() {
    let (svc, _manager, _storage) = service_with_mocks();
    let result = svc.get_rotation_history("@dave:example.com", "UNKNOWN").await.expect("get_rotation_history");
    assert!(result.is_empty());
}

#[tokio::test]
async fn test_get_max_rotation_ts_returns_seeded_value() {
    let manager = InMemoryKeyRotationManager::new();
    let storage = InMemoryKeyRotationStorage::new();
    storage.seed_last_rotation_ts("@eve:example.com", 1_700_000_000_000).await;
    let svc = FederationKeyRotationService::new(Arc::new(manager), Arc::new(storage));

    let ts = svc.get_max_rotation_ts("@eve:example.com").await.expect("get_max_rotation_ts");
    assert_eq!(ts, 1_700_000_000_000);
}

#[tokio::test]
async fn test_get_max_rotation_ts_zero_when_no_data() {
    let (svc, _manager, _storage) = service_with_mocks();
    let ts = svc.get_max_rotation_ts("@nobody:example.com").await.expect("get_max_rotation_ts");
    assert_eq!(ts, 0);
}

#[tokio::test]
async fn test_get_last_rotation_for_key_returns_none_for_unknown() {
    let (svc, _manager, _storage) = service_with_mocks();
    let result =
        svc.get_last_rotation_for_key("@alice:example.com", "unknown_key").await.expect("get_last_rotation_for_key");
    assert_eq!(result, None);
}
