//! TDD tests for prelude modules (P6).
//!
//! These tests verify that the prelude glob-import point
//! (`synapse_storage::prelude::*` / `synapse_services::prelude::*`) exposes the
//! same types as the legacy root flat surface, ensuring the prelude is a
//! behavior-preserving backward-compat entry point.

/// Compile-time type identity check: asserts that two paths resolve to the
/// same concrete type. If either path is unreachable or the types differ,
/// compilation fails (RED).
fn assert_same_type<T>(_a: &T, _b: &T) {}

// --- P6: synapse-storage prelude (admin + auth + e2ee domain groups) ---

#[test]
fn test_storage_prelude_admin_federation_storage_identity() {
    let legacy_ref: Option<&synapse_storage::AdminFederationStorage> = None;
    let prelude_ref: Option<&synapse_storage::prelude::AdminFederationStorage> = None;
    if let (Some(a), Some(b)) = (legacy_ref, prelude_ref) {
        assert_same_type(a, b);
    }
}

#[test]
fn test_storage_prelude_user_storage_identity() {
    let legacy_ref: Option<&synapse_storage::UserStorage> = None;
    let prelude_ref: Option<&synapse_storage::prelude::UserStorage> = None;
    if let (Some(a), Some(b)) = (legacy_ref, prelude_ref) {
        assert_same_type(a, b);
    }
}

#[test]
fn test_storage_prelude_dehydrated_device_storage_identity() {
    let legacy_ref: Option<&synapse_storage::DehydratedDeviceStorage> = None;
    let prelude_ref: Option<&synapse_storage::prelude::DehydratedDeviceStorage> = None;
    if let (Some(a), Some(b)) = (legacy_ref, prelude_ref) {
        assert_same_type(a, b);
    }
}

// --- P6: synapse-services prelude (admin + sync domain groups) ---

#[test]
fn test_services_prelude_sync_service_identity() {
    let legacy_ref: Option<&synapse_services::SyncService> = None;
    let prelude_ref: Option<&synapse_services::prelude::SyncService> = None;
    if let (Some(a), Some(b)) = (legacy_ref, prelude_ref) {
        assert_same_type(a, b);
    }
}

#[test]
fn test_services_prelude_sliding_sync_service_identity() {
    let legacy_ref: Option<&synapse_services::SlidingSyncService> = None;
    let prelude_ref: Option<&synapse_services::prelude::SlidingSyncService> = None;
    if let (Some(a), Some(b)) = (legacy_ref, prelude_ref) {
        assert_same_type(a, b);
    }
}
