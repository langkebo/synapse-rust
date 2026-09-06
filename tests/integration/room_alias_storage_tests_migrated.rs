//! DB-backed integration tests for B-7: room alias `server_name` case normalization.
//!
//! Matrix spec v1.11 § 4.3 mandates that the `server_name` portion of a
//! room alias is case-insensitive and lowercased before processing. The
//! `localpart` is case-sensitive and left untouched.
//!
//! These tests exercise the real PostgreSQL implementation of
//! `RoomStorage::set_room_alias` / `get_room_by_alias` /
//! `remove_room_alias_by_name` against a migrated schema and verify that:
//!
//!   1. Writing `#Foo:Example.com` stores the row under the canonical
//!      `#Foo:example.com` form.
//!   2. Reading `#FOO:EXAMPLE.com` resolves to the same row.
//!   3. Removing with a different case still deletes the canonical row.
//!   4. Looking up a never-inserted alias still returns `None` (no false
//!      positives from mis-normalization).
//!   5. The `20260906000000_normalize_room_alias_server_name.sql` migration
//!      rewrites historical uppercase `server_name` rows in place.
//!
//! All three test names match the ticket in
//! `.scratch/backend-issues-2026-09-06/issues/06-backend-issues-audit-and-optimization-plan.md`
//! (B-7 § 2.4) so that the audit report and the test catalog stay in sync.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;

use synapse_common::current_timestamp_millis;
use synapse_storage::room::RoomStorage;

static TEST_COUNTER: AtomicU64 = AtomicU64::new(1);

fn unique_id() -> u64 {
    TEST_COUNTER.fetch_add(1, Ordering::SeqCst)
}

fn setup_test_database(_pool: &Arc<sqlx::PgPool>) {
    // `room_aliases` is created by the shared test pool
    // (`crate::require_test_pool`); this test only reads/writes the existing
    // schema.
}

/// Insert a placeholder room so that the `fk_room_aliases_room` FK is
/// satisfied for `set_room_alias`.
async fn insert_room(pool: &sqlx::PgPool, room_id: &str) {
    let now = current_timestamp_millis();
    sqlx::query("INSERT INTO rooms (room_id, created_ts) VALUES ($1, $2) ON CONFLICT (room_id) DO NOTHING")
        .bind(room_id)
        .bind(now)
        .execute(pool)
        .await
        .expect("Failed to insert placeholder room");
}

/// B-7: writing `#Foo:Example.com` must store the alias in the canonical
/// `#Foo:example.com` form (localpart preserved, server_name lowercased).
#[tokio::test]
async fn test_set_then_get_normalizes_server_case() {
    let pool = crate::require_test_pool().await;
    setup_test_database(&pool);
    let storage = RoomStorage::new(&pool);
    let suffix = unique_id();
    let room_id = format!("!room_b7_setget_{suffix}:localhost");
    let written_alias = format!("#B7SetGet_{suffix}:Example.COM");

    insert_room(&pool, &room_id).await;

    storage
        .set_room_alias(&room_id, &written_alias, "@creator:localhost")
        .await
        .expect("set_room_alias should succeed");

    // Look up the same alias in several different casings — every variant
    // must resolve to the same room_id.
    let canonical = format!("#B7SetGet_{suffix}:example.com");
    let lookup_canonical = storage.get_room_by_alias(&canonical).await.expect("get_room_by_alias should succeed");
    assert_eq!(lookup_canonical, Some(room_id.clone()), "B-7: canonical alias must resolve to the room");

    let lookup_mixed = storage
        .get_room_by_alias(&written_alias)
        .await
        .expect("get_room_by_alias should succeed for the original mixed-case input");
    assert_eq!(
        lookup_mixed,
        Some(room_id.clone()),
        "B-7: the originally written mixed-case alias must also resolve to the room"
    );

    let lookup_upper = storage
        .get_room_by_alias(&format!("#B7SetGet_{suffix}:EXAMPLE.COM"))
        .await
        .expect("get_room_by_alias should succeed for the uppercased server_name");
    assert_eq!(
        lookup_upper,
        Some(room_id.clone()),
        "B-7: an all-uppercase lookup must resolve to the same room"
    );

    // Direct DB inspection: the row's `room_alias` and `server_name` columns
    // must be stored in canonical (lowercased server_name) form.
    let row: (String, String) =
        sqlx::query_as("SELECT room_alias, server_name FROM room_aliases WHERE room_id = $1")
            .bind(&room_id)
            .fetch_one(pool.as_ref())
            .await
            .expect("room_aliases row must exist after set_room_alias");
    assert_eq!(
        row.0, canonical,
        "B-7: persisted room_alias must have the server_name lowercased"
    );
    assert_eq!(
        row.1, "example.com",
        "B-7: persisted server_name column must be lowercased"
    );
}

/// B-7: looking up a never-inserted alias (with arbitrary case) must return
/// `None`. Guards against false positives from a buggy normalizer (e.g. one
/// that lowercased the *localpart* by accident).
#[tokio::test]
async fn test_get_nonexistent_uppercased_server() {
    let pool = crate::require_test_pool().await;
    setup_test_database(&pool);
    let storage = RoomStorage::new(&pool);
    let suffix = unique_id();
    let absent_alias = format!("#B7Absent_{suffix}:NoSuchHost.Example");

    let result = storage
        .get_room_by_alias(&absent_alias)
        .await
        .expect("get_room_by_alias should not fail on absent alias");

    assert!(
        result.is_none(),
        "B-7: looking up an alias that was never inserted must return None, got {result:?}"
    );

    // Re-query in a different case to make sure the normalizer didn't
    // accidentally synthesize a row.
    let result_upper = storage
        .get_room_by_alias(&format!("#B7Absent_{suffix}:nosuchhost.example"))
        .await
        .expect("get_room_by_alias should not fail on absent alias");
    assert!(
        result_upper.is_none(),
        "B-7: lowercased lookup of a never-inserted alias must also return None, got {result_upper:?}"
    );
}

/// B-7: `remove_room_alias_by_name` must normalize its input too — a
/// client that asks to remove `#foo:EXAMPLE.com` must still drop the
/// canonical row stored under `#foo:example.com`.
#[tokio::test]
async fn test_remove_normalizes_server_case() {
    let pool = crate::require_test_pool().await;
    setup_test_database(&pool);
    let storage = RoomStorage::new(&pool);
    let suffix = unique_id();
    let room_id = format!("!room_b7_remove_{suffix}:localhost");
    let canonical = format!("#B7Remove_{suffix}:example.com");

    insert_room(&pool, &room_id).await;
    storage
        .set_room_alias(&room_id, &canonical, "@creator:localhost")
        .await
        .expect("set_room_alias should succeed");

    // Sanity: the alias resolves before removal.
    assert_eq!(
        storage.get_room_by_alias(&canonical).await.expect("get_room_by_alias"),
        Some(room_id.clone()),
        "B-7: alias must resolve before removal"
    );

    // Remove via a different-cased input.
    storage
        .remove_room_alias_by_name(&format!("#B7Remove_{suffix}:EXAMPLE.com"))
        .await
        .expect("remove_room_alias_by_name should succeed");

    // The row must be gone for all case variants.
    let after = storage
        .get_room_by_alias(&canonical)
        .await
        .expect("get_room_by_alias should succeed after removal");
    assert!(
        after.is_none(),
        "B-7: alias must be removed even when the removal request used a different server_name case"
    );

    // Direct DB inspection: the row is fully gone (not just lowercased).
    let row_count: (i64,) =
        sqlx::query_as("SELECT COUNT(*) FROM room_aliases WHERE room_id = $1")
            .bind(&room_id)
            .fetch_one(pool.as_ref())
            .await
            .expect("count query should succeed");
    assert_eq!(row_count.0, 0, "B-7: no rows must remain for the room after normalized removal");
}

/// B-7 migration: simulate a pre-migration legacy row (uppercase
/// `server_name`) and verify the migration SQL rewrites it in place. The
/// migration script (`20260906000000_normalize_room_alias_server_name.sql`)
/// is exercised by running its `UPDATE` directly inside the test schema
/// because the shared test pool only applies the latest migration once.
#[tokio::test]
async fn test_legacy_uppercase_data_normalized_after_migration() {
    let pool = crate::require_test_pool().await;
    setup_test_database(&pool);
    let suffix = unique_id();
    let room_id = format!("!room_b7_migrate_{suffix}:localhost");
    let legacy_alias = format!("#B7Legacy_{suffix}:EXAMPLE.COM");
    let canonical_alias = format!("#B7Legacy_{suffix}:example.com");

    insert_room(&pool, &room_id).await;

    // Insert a row in legacy (uppercase) form, bypassing the new
    // `set_room_alias` normalizer so we can simulate a pre-migration row.
    let now = current_timestamp_millis();
    sqlx::query(
        r"
        INSERT INTO room_aliases (room_alias, room_id, server_name, created_ts)
        VALUES ($1, $2, $3, $4)
        ",
    )
    .bind(&legacy_alias)
    .bind(&room_id)
    .bind("EXAMPLE.COM")
    .bind(now)
    .execute(pool.as_ref())
    .await
    .expect("legacy insert should succeed");

    // Sanity: the legacy row exists in its un-normalized form.
    let before: (String, String) =
        sqlx::query_as("SELECT room_alias, server_name FROM room_aliases WHERE room_id = $1")
            .bind(&room_id)
            .fetch_one(pool.as_ref())
            .await
            .expect("legacy row must exist before migration");
    assert_eq!(before.0, legacy_alias, "B-7: pre-migration row must be uppercase");
    assert_eq!(before.1, "EXAMPLE.COM", "B-7: pre-migration server_name must be uppercase");

    // Apply the same UPDATE that the migration runs. We execute the SQL
    // verbatim from `migrations/20260906000000_normalize_room_alias_server_name.sql`
    // so that the test guards both the production code path AND the
    // migration contract.
    sqlx::query(
        r"
        UPDATE room_aliases
        SET
            room_alias  = SUBSTRING(room_alias FROM 1 FOR POSITION(':' IN room_alias))
                         || LOWER(SUBSTRING(room_alias FROM POSITION(':' IN room_alias) + 1)),
            server_name = LOWER(server_name)
        WHERE
            server_name <> LOWER(server_name)
            AND POSITION(':' IN room_alias) > 0
        ",
    )
    .execute(pool.as_ref())
    .await
    .expect("migration UPDATE should succeed");

    // The row must now be in canonical form.
    let after: (String, String) =
        sqlx::query_as("SELECT room_alias, server_name FROM room_aliases WHERE room_id = $1")
            .bind(&room_id)
            .fetch_one(pool.as_ref())
            .await
            .expect("row must still exist after migration");
    assert_eq!(
        after.0, canonical_alias,
        "B-7 migration: legacy room_alias must be rewritten with the server_name lowercased"
    );
    assert_eq!(
        after.1, "example.com",
        "B-7 migration: legacy server_name column must be lowercased"
    );

    // The new code path can now resolve the rewritten row via any case.
    let storage = RoomStorage::new(&pool);
    let lookup = storage
        .get_room_by_alias(&canonical_alias)
        .await
        .expect("get_room_by_alias should succeed post-migration");
    assert_eq!(
        lookup,
        Some(room_id.clone()),
        "B-7 migration: post-migration lookup must resolve to the room"
    );
}
