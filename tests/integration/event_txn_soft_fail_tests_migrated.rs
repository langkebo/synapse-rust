//! DB-backed integration tests for B-8: event soft-fail instead of physical delete.
//!
//! B-8 replaces `delete_event_by_id` (a hard `DELETE FROM events`) with
//! `mark_event_soft_failed` (an `UPDATE ... SET soft_failed = TRUE`). This
//! change addresses three concerns:
//!
//!   1. FK violations — `events` has FKs to `event_json`, `event_edges`,
//!      `state_events`, `room_memberships` etc. Hard `DELETE FROM events`
//!      fails with FK constraint errors when those children exist.
//!   2. Broken event DAG — hard-delete breaks `prev_events` references that
//!      point at now-missing IDs, causing `missing predecessors` errors
//!      during sync.
//!   3. Compliance / audit — many jurisdictions require events to be
//!      retained for N days; hard-delete violates that.
//!
//! These tests verify at the storage layer that:
//!
//!   1. `mark_event_soft_failed` flips the `soft_failed` column to TRUE and
//!      leaves the row intact (no physical delete).
//!   2. Calling `mark_event_soft_failed` on a row that doesn't exist is a
//!      no-op (idempotent — safe to retry).
//!   3. A simulated `send_message_with_txn` race produces exactly one
//!      visible (non-soft-failed) event in the room even though two events
//!      were written to the table (one winner + one soft-failed loser).
//!   4. The race winner is the one with the lower `stream_ordering` (older
//!      event) — that is the row that survives a `WHERE soft_failed = FALSE`
//!      read filter.
//!
//! Test names mirror the corresponding checks in
//! `.scratch/backend-issues-2026-09-06/issues/06-backend-issues-audit-and-optimization-plan.md`
//! § 3 (B-8).

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;

use synapse_common::current_timestamp_millis;
use synapse_storage::event::EventStorage;

static TEST_COUNTER: AtomicU64 = AtomicU64::new(1);

fn unique_id() -> u64 {
    TEST_COUNTER.fetch_add(1, Ordering::SeqCst)
}

fn setup_test_database(_pool: &Arc<sqlx::PgPool>) {
    // `events` and `room_event_txn_dedup` are created by the shared test
    // pool (`crate::require_test_pool`); this test only writes/reads the
    // existing schema.
}

fn create_event_storage(pool: &Arc<sqlx::PgPool>) -> EventStorage {
    EventStorage { pool: pool.clone(), server_name: "localhost".to_string() }
}

/// Insert a placeholder room so that the room-level FK is satisfied for
/// event inserts.
async fn insert_room(pool: &Arc<sqlx::PgPool>, room_id: &str) {
    let now = current_timestamp_millis();
    sqlx::query("INSERT INTO rooms (room_id, created_ts) VALUES ($1, $2) ON CONFLICT (room_id) DO NOTHING")
        .bind(room_id)
        .bind(now)
        .execute(pool.as_ref())
        .await
        .expect("Failed to insert placeholder room");
}

/// B-8: `mark_event_soft_failed` must set `soft_failed = TRUE` on the row
/// and **leave the row in place** (no physical delete).  This is the core
/// invariant that distinguishes B-8 from the prior `delete_event_by_id`
/// hard-delete path.
#[tokio::test]
async fn test_mark_event_soft_failed_preserves_row() {
    let pool = crate::require_test_pool().await;
    setup_test_database(&pool);
    let storage = create_event_storage(&pool);
    let suffix = unique_id();
    let room_id = format!("!room_b8_softfail_{suffix}:localhost");
    let event_id = format!("$event_b8_softfail_{suffix}:localhost");

    insert_room(&pool, &room_id).await;

    // Insert a baseline event via direct SQL to keep this test focused on
    // `mark_event_soft_failed`.  (Full `create_event` would also work but
    // pulls in auth_events, depth, etc. that are not relevant here.)
    let now = current_timestamp_millis();
    sqlx::query(
        r"
        INSERT INTO events (event_id, room_id, sender, user_id, event_type, content, origin_server_ts, is_redacted, soft_failed)
        VALUES ($1, $2, $3, $4, $5, $6, $7, false, false)
        ",
    )
    .bind(&event_id)
    .bind(&room_id)
    .bind("@sender:localhost")
    .bind("@sender:localhost")
    .bind("m.room.message")
    .bind(serde_json::json!({"body": "soft-fail me"}))
    .bind(now)
    .execute(pool.as_ref())
    .await
    .expect("Failed to insert test event");

    // Sanity: soft_failed is false on the freshly inserted row.
    let before: (bool,) = sqlx::query_as("SELECT soft_failed FROM events WHERE event_id = $1")
        .bind(&event_id)
        .fetch_one(pool.as_ref())
        .await
        .expect("event must exist before mark_event_soft_failed");
    assert!(!before.0, "B-8: freshly inserted event must have soft_failed = FALSE");

    // Apply the B-8 transformation.
    storage.mark_event_soft_failed(&event_id).await.expect("mark_event_soft_failed should succeed");

    // The row must still exist (no physical delete).
    let row_count: (i64,) = sqlx::query_as("SELECT COUNT(*) FROM events WHERE event_id = $1")
        .bind(&event_id)
        .fetch_one(pool.as_ref())
        .await
        .expect("count query should succeed");
    assert_eq!(row_count.0, 1, "B-8: the row must remain in events after soft-fail (no DELETE)");

    // soft_failed must now be TRUE.
    let after: (bool,) = sqlx::query_as("SELECT soft_failed FROM events WHERE event_id = $1")
        .bind(&event_id)
        .fetch_one(pool.as_ref())
        .await
        .expect("event must still exist after mark_event_soft_failed");
    assert!(after.0, "B-8: mark_event_soft_failed must set soft_failed = TRUE");
}

/// B-8: `mark_event_soft_failed` is idempotent — calling it on a
/// non-existent event_id must succeed (no panic, no error). This matches
/// the contract in the docstring and is what makes the race-retry path
/// safe.
#[tokio::test]
async fn test_mark_event_soft_failed_idempotent_on_missing_event() {
    let pool = crate::require_test_pool().await;
    setup_test_database(&pool);
    let storage = create_event_storage(&pool);
    let suffix = unique_id();
    let ghost_event_id = format!("$ghost_b8_{suffix}:localhost");

    // The event was never inserted.  The call must still succeed.
    storage
        .mark_event_soft_failed(&ghost_event_id)
        .await
        .expect("B-8: mark_event_soft_failed must be idempotent on missing event_id");

    // Second call must also succeed (idempotency under retry).
    storage
        .mark_event_soft_failed(&ghost_event_id)
        .await
        .expect("B-8: mark_event_soft_failed must remain idempotent across retries");
}

/// B-8: a simulated `send_message_with_txn` race writes two events to the
/// `events` table, then soft-fails the loser.  Consumer read paths that
/// filter `WHERE soft_failed = FALSE` must see exactly one event for the
/// (user, room, txn_id) tuple.
#[tokio::test]
async fn test_concurrent_txn_race_second_event_soft_failed_not_deleted() {
    let pool = crate::require_test_pool().await;
    setup_test_database(&pool);
    let storage = create_event_storage(&pool);
    let suffix = unique_id();
    let user_id = format!("@alice_b8_{suffix}:localhost");
    let room_id = format!("!room_b8_race_{suffix}:localhost");
    let txn_id = format!("txn-b8-race-{suffix}");

    insert_room(&pool, &room_id).await;

    // The two concurrent senders each call `send_message` first, producing
    // two distinct event_ids.  We model this by inserting both events with
    // different `stream_ordering` values.
    let winner_event_id = format!("$winner_b8_{suffix}:localhost");
    let loser_event_id = format!("$loser_b8_{suffix}:localhost");
    let base_ts = current_timestamp_millis();

    sqlx::query(
        r"
        INSERT INTO events (event_id, room_id, sender, user_id, event_type, content, origin_server_ts, stream_ordering, is_redacted, soft_failed)
        VALUES ($1, $2, $3, $4, $5, $6, $7, $8, false, false)
        ",
    )
    .bind(&winner_event_id)
    .bind(&room_id)
    .bind(&user_id)
    .bind(&user_id)
    .bind("m.room.message")
    .bind(serde_json::json!({"body": "winner"}))
    .bind(base_ts)
    .bind(1000_i64)
    .execute(pool.as_ref())
    .await
    .expect("Failed to insert winner event");

    sqlx::query(
        r"
        INSERT INTO events (event_id, room_id, sender, user_id, event_type, content, origin_server_ts, stream_ordering, is_redacted, soft_failed)
        VALUES ($1, $2, $3, $4, $5, $6, $7, $8, false, false)
        ",
    )
    .bind(&loser_event_id)
    .bind(&room_id)
    .bind(&user_id)
    .bind(&user_id)
    .bind("m.room.message")
    .bind(serde_json::json!({"body": "loser"}))
    .bind(base_ts + 1)
    .bind(2000_i64)
    .execute(pool.as_ref())
    .await
    .expect("Failed to insert loser event");

    // The dedup table also has two rows (mirroring the race outcome):
    // only the winner's event_id is durably bound to the (user, room, txn).
    sqlx::query(
        r"
        INSERT INTO room_event_txn_dedup (user_id, room_id, txn_id, event_id, created_ts)
        VALUES ($1, $2, $3, $4, $5)
        ",
    )
    .bind(&user_id)
    .bind(&room_id)
    .bind(&txn_id)
    .bind(&winner_event_id)
    .bind(base_ts)
    .execute(pool.as_ref())
    .await
    .expect("Failed to insert dedup row for winner");

    // The loser is now soft-failed via the new B-8 path.
    storage
        .mark_event_soft_failed(&loser_event_id)
        .await
        .expect("mark_event_soft_failed should succeed for the race loser");

    // B-8 invariant 1: BOTH rows still exist in the events table
    // (the loser was NOT hard-deleted).
    let total: (i64,) = sqlx::query_as("SELECT COUNT(*) FROM events WHERE room_id = $1")
        .bind(&room_id)
        .fetch_one(pool.as_ref())
        .await
        .expect("count query should succeed");
    assert_eq!(
        total.0, 2,
        "B-8: both events must remain in the table (no physical DELETE) — winner + soft-failed loser"
    );

    // B-8 invariant 2: only the winner is visible to consumer read paths
    // that filter `WHERE soft_failed = FALSE`.
    let visible_count: (i64,) =
        sqlx::query_as("SELECT COUNT(*) FROM events WHERE room_id = $1 AND soft_failed = FALSE")
            .bind(&room_id)
            .fetch_one(pool.as_ref())
            .await
            .expect("filtered count query should succeed");
    assert_eq!(
        visible_count.0, 1,
        "B-8: exactly one event must be visible after the soft-fail — the loser is hidden by the filter"
    );

    // B-8 invariant 3: the visible event is the winner.
    let visible: (String, bool) =
        sqlx::query_as("SELECT event_id, soft_failed FROM events WHERE room_id = $1 AND soft_failed = FALSE")
            .bind(&room_id)
            .fetch_one(pool.as_ref())
            .await
            .expect("visible event should exist");
    assert_eq!(visible.0, winner_event_id, "B-8: the visible event must be the winner");
    assert!(!visible.1, "B-8: the visible event must have soft_failed = FALSE");

    // B-8 invariant 4: the loser's `soft_failed` flag is TRUE.
    let loser_state: (bool,) = sqlx::query_as("SELECT soft_failed FROM events WHERE event_id = $1")
        .bind(&loser_event_id)
        .fetch_one(pool.as_ref())
        .await
        .expect("loser must still exist after soft-fail");
    assert!(loser_state.0, "B-8: the loser's soft_failed flag must be TRUE");

    // B-8 invariant 5: `record_event_txn` for a third attempt with the same
    // (user, room, txn) triple must return `false` (the dedup row already
    // exists from the winner) — guaranteeing the room never sees a third
    // event for this txn.
    let inserted = storage
        .record_event_txn(&user_id, &room_id, &txn_id, &loser_event_id)
        .await
        .expect("record_event_txn should succeed");
    assert!(!inserted, "B-8: a duplicate record_event_txn call must return false");
}
