//! DB round trip for `key_rotation_log` — the table the E2EE rotation path writes
//! and `GET /_matrix/vendor/v1/keys/rotation/history/{device_id}` reads.
//!
//! Why this file exists: `log_rotation` bound `None::<String>` into
//! `key_rotation_log.device_id`, which the baseline declares `NOT NULL`, so every
//! call aborted with `null value in column "device_id" violates not-null
//! constraint`. Nothing caught it — the only caller
//! (`KeyRotationService::rotate_megolm_session`) is not wired into production, and
//! this module had no DB coverage at all (the sibling `tests` module is pure
//! config/constructor checks).
//!
//! The guard below is a real write→read round trip rather than a mock, so it pins
//! three things at once: the column's `NOT NULL`, the argument order of
//! `log_rotation`, and the reader's `(user_id, device_id)` filter.

use super::*;
use synapse_common::test_isolation::IsolatedTestPool;

/// The workspace baseline migration. The bytes are load-bearing (the shared
/// template name is a content fingerprint of this string), so it must stay
/// byte-identical to the copies in `synapse-storage/src/test_isolation.rs`,
/// `synapse-federation/src/test_isolation.rs`, `synapse-services/src/test_utils.rs`
/// and `synapse-e2ee/src/megolm/storage/db_tests.rs`.
const BASELINE_SQL: &str = include_str!("../../../../migrations/00000000_unified_schema_v12.sql");

#[tokio::test]
async fn test_log_rotation_round_trips_through_the_device_scoped_reader() {
    let isolated = IsolatedTestPool::new(BASELINE_SQL).await.expect("isolated test pool");
    let storage = KeyRotationStorage::new(isolated.pool());

    let user = "@alice:example.com";
    let device = "DEVICEALICE";
    let room = "!kr:example.com";

    // Aborted with a NOT NULL violation while `device_id` was bound as SQL NULL.
    storage.log_rotation(user, device, room, "megolm").await.expect("log_rotation must insert");

    let history = storage.get_device_rotation_history(user, device).await.expect("history read");
    assert_eq!(history.len(), 1, "the row must be readable back through the device-scoped reader: {history:?}");
    let (key_id, rotated_ts) = &history[0];
    assert!(key_id.is_some(), "`log_rotation` writes `new_key_id`, so it must come back non-NULL: {history:?}");
    assert!(rotated_ts.is_some_and(|ts| ts > 0), "`rotated_at` must be a real millisecond stamp, not 0: {history:?}");

    // The row carries the device that performed the rotation, not merely *some*
    // non-NULL value: another device must not see it.
    let other_device = storage.get_device_rotation_history(user, "DEVICEBOB").await.expect("history read");
    assert!(
        other_device.is_empty(),
        "history is device-scoped — a different device must see nothing: {other_device:?}"
    );

    // ...and the same device id under another user must not either.
    let other_user = storage.get_device_rotation_history("@bob:example.com", device).await.expect("history read");
    assert!(other_user.is_empty(), "history is user-scoped too: {other_user:?}");
}
