//! Per-test schema isolation for `synapse-federation`'s DB tests.
//!
//! Federation used to have **no** DB-test infrastructure at all (no `IsolatedTestPool`, no
//! `BASELINE_SQL`; dev-deps were `wiremock` only), so every DB path in this crate had to run
//! against the shared `public` schema. That is why the self-healing branch in
//! `key_rotation.rs` (“the signing-key table is missing — create it”) could never be
//! constructed: dropping a table on the shared schema would break concurrently running tests
//! (the same trap D-57 / D-75 / D-76 recorded for other shared state).
//!
//! The pool lifecycle (struct, construction, janitor-registered cleanup) lives in
//! [`synapse_common::test_isolation::IsolatedTestPool`] — that module is compiled
//! unconditionally and is therefore reachable from every sibling crate, whereas a
//! `#[cfg(test)]` module in `synapse-storage` is not.
//!
//! Usage:
//! ```ignore
//! let isolated = crate::test_isolation::isolated_test_pool().await.unwrap();
//! let pool = isolated.pool();
//! // schema is auto-dropped when `isolated` is dropped
//! ```
//!
//! The template-schema machinery (fingerprint, naming, advisory lock, build, single-round-trip
//! clone, inventory validation) also lives in `synapse_common::test_isolation`.

pub use synapse_common::test_isolation::IsolatedTestPool;

/// Baseline SQL for isolated schemas, handed to the shared pool constructor.
///
/// ⚠️ The exact bytes are load-bearing **twice over**:
/// * the template name is a content fingerprint of this string, so any edit forks a second
///   template (the shared `ensure_template_schema` rebuilds from scratch) instead of reusing
///   the one already in the database;
/// * `tests/unit/test_isolation_unification_tests.rs` re-evaluates every copy's `include_str!`
///   and asserts they all hash to the same value as the on-disk migration, so this copy is
///   pinned together with the ones in `synapse-storage/src/test_isolation.rs`,
///   `synapse-services/src/test_utils.rs`, `synapse-test-utils/src/lib.rs` and
///   `synapse-e2ee/src/{backup,olm}/storage.rs`.
fn isolated_baseline_sql() -> &'static str {
    include_str!("../../migrations/00000000_unified_schema_v12.sql")
}

/// Create a pool backed by a fresh schema cloned from the shared v12 template.
///
/// DB tests in this crate should start here: the schema is dropped on `Drop`, so tests cannot
/// leak state into each other or into `public` — and destructive fixtures (dropping a table to
/// exercise a self-healing path) become safe to write.
pub async fn isolated_test_pool() -> Result<IsolatedTestPool, sqlx::Error> {
    IsolatedTestPool::new(isolated_baseline_sql()).await
}
