//! Tests for `EventStorage::verify_postgres_search_index`.
//!
//! Kept out of `db_tests.rs` on purpose: that file already sits on the file-size
//! ratchet, and these three cases exercise one read-only check rather than the
//! event CRUD surface.

use super::EventStorage;

fn server_name() -> String {
    "example.com".to_string()
}

#[tokio::test]
async fn test_verify_postgres_search_index_passes_on_migrated_schema() {
    let isolated = crate::test_isolation::isolated_test_pool().await.expect("isolated pool");
    let pool = isolated.pool();
    let storage = EventStorage::new(&pool, server_name());

    // The index ships with the migration, so verification must be a pure read:
    // calling it twice cannot depend on the first call having created anything.
    storage.verify_postgres_search_index().await.expect("the migrated schema must ship a valid event_search_fts_idx");
    storage.verify_postgres_search_index().await.expect("verification must be idempotent");
}

/// A `CONCURRENTLY` build that fails leaves an **INVALID** index behind, and
/// `IF NOT EXISTS` then skips that name forever — so verification must not report
/// success in that state (every search would silently seq-scan).
///
/// The state is produced the way PostgreSQL produces it, by flipping
/// `indisvalid` on the migrated index. Rebuilding a failing concurrent index
/// cannot be used here: the index lives on `event_search`, whose `(event_id, key)`
/// primary key makes duplicate rows impossible, so no data shape fails a UNIQUE
/// build. The flip happens in a **per-test isolated schema**, so it cannot leak.
#[tokio::test]
async fn test_verify_postgres_search_index_reports_invalid_leftover() {
    let isolated = crate::test_isolation::isolated_test_pool().await.expect("isolated pool");
    let pool = isolated.pool();
    let storage = EventStorage::new(&pool, server_name());

    let updated = sqlx::query(
        "UPDATE pg_index SET indisvalid = false \
         WHERE indexrelid = (SELECT i.indexrelid FROM pg_index i JOIN pg_class c ON c.oid = i.indexrelid \
                             WHERE i.indrelid = to_regclass('event_search') AND c.relname = 'event_search_fts_idx')",
    )
    .execute(&*pool)
    .await
    .expect("mark the migrated index INVALID");
    assert_eq!(updated.rows_affected(), 1, "precondition: the migrated index must exist on event_search");

    let err =
        storage.verify_postgres_search_index().await.expect_err("an INVALID index must not be reported as usable");
    assert!(err.to_string().contains("INVALID"), "the error must name the INVALID state and the remedy, got: {err}");
}

/// A database whose migration never ran has no `event_search_fts_idx`; search
/// would still "work" (sequential scan) but silently lose the index.
#[tokio::test]
async fn test_verify_postgres_search_index_reports_missing() {
    let isolated = crate::test_isolation::isolated_test_pool().await.expect("isolated pool");
    let pool = isolated.pool();
    let storage = EventStorage::new(&pool, server_name());

    sqlx::query("DROP INDEX event_search_fts_idx").execute(&*pool).await.expect("drop the migrated index");

    let err = storage
        .verify_postgres_search_index()
        .await
        .expect_err("a missing index must be reported, not silently tolerated");
    assert!(err.to_string().contains("missing"), "the error must name the missing index, got: {err}");
}
