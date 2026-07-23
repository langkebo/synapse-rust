//! TDD tests for synapse-storage admin domain grouping (P1).
//!
//! These tests verify that the new domain-grouped path
//! (`synapse_storage::admin::AdminFederationStorage`) resolves to the same
//! type as the legacy flat path (`synapse_storage::AdminFederationStorage`),
//! ensuring the refactor is behavior-preserving.

/// Compile-time type identity check: asserts that two paths resolve to the
/// same concrete type. If either path is unreachable or the types differ,
/// compilation fails (RED).
fn assert_same_type<T>(_a: &T, _b: &T) {}

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
