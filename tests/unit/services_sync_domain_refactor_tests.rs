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
