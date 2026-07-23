//! TDD tests for synapse-storage room domain grouping (P3).
//!
//! These tests verify that the room domain-grouped path
//! (`synapse_storage::room::RoomMemberStorage`) resolves to the same type as
//! the legacy flat path (`synapse_storage::RoomMemberStorage`), ensuring the
//! refactor is behavior-preserving.

/// Compile-time type identity check: asserts that two paths resolve to the
/// same concrete type. If either path is unreachable or the types differ,
/// compilation fails (RED).
fn assert_same_type<T: ?Sized>(_a: &T, _b: &T) {}

#[test]
fn test_room_storage_path_identity() {
    let legacy_ref: Option<&synapse_storage::RoomStorage> = None;
    let grouped_ref: Option<&synapse_storage::room::RoomStorage> = None;
    if let (Some(a), Some(b)) = (legacy_ref, grouped_ref) {
        assert_same_type(a, b);
    }
}

#[test]
fn test_room_member_storage_path_identity() {
    let legacy_ref: Option<&synapse_storage::RoomMemberStorage> = None;
    let grouped_ref: Option<&synapse_storage::room::RoomMemberStorage> = None;
    if let (Some(a), Some(b)) = (legacy_ref, grouped_ref) {
        assert_same_type(a, b);
    }
}

#[test]
fn test_room_account_data_storage_path_identity() {
    let legacy_ref: Option<&synapse_storage::RoomAccountDataStorage> = None;
    let grouped_ref: Option<&synapse_storage::room::RoomAccountDataStorage> = None;
    if let (Some(a), Some(b)) = (legacy_ref, grouped_ref) {
        assert_same_type(a, b);
    }
}

#[test]
fn test_state_group_store_api_path_identity() {
    let legacy_ref: Option<&dyn synapse_storage::StateGroupStoreApi> = None;
    let grouped_ref: Option<&dyn synapse_storage::room::StateGroupStoreApi> = None;
    if let (Some(a), Some(b)) = (legacy_ref, grouped_ref) {
        assert_same_type(a, b);
    }
}

#[test]
fn test_thread_storage_path_identity() {
    let legacy_ref: Option<&synapse_storage::ThreadStorage> = None;
    let grouped_ref: Option<&synapse_storage::room::ThreadStorage> = None;
    if let (Some(a), Some(b)) = (legacy_ref, grouped_ref) {
        assert_same_type(a, b);
    }
}
