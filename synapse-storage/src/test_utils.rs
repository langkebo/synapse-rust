//! Test utilities for synapse-storage.
//!
//! Provides isolated test database pool helpers used by `#[cfg(test)]` code
//! within this crate.

use sqlx::postgres::PgPoolOptions;
use sqlx::PgPool;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::LazyLock;
use std::sync::{Arc, Mutex};
use std::time::Duration;
use synapse_common::test_schema_guard::{schema_lease_key, SchemaCleanup, TestSchemaGuard};

static TEST_SCHEMA_COUNTER: AtomicU64 = AtomicU64::new(1);

// ============================================================================
// Schema lifecycle
// ============================================================================
//
// This module used to keep its own `PENDING_SCHEMA_DROPS` registry — a third
// copy of a mechanism that was copy-pasted across three crates and could
// never converge. The registry+sweep design is fundamentally broken under
// nextest's one-process-per-test model (the sweep only ran on the *next*
// pool acquisition, which never happens in the creating process), which is
// how the local test database reached 23,662 leftover schemas
// (docs/audit/P5_test_schema_accumulation_2026-09-12.md).
//
// Cleanup is now owned by the shared engine in
// `synapse_common::test_schema_guard`: the janitor watches the pool's weak
// reference and runs `DROP SCHEMA ... CASCADE` as soon as the last
// `Arc<PgPool>` is released, with an atexit join as the deterministic
// backstop. The guard returned below is the caller-visible ownership handle.

/// Process-wide cache of the resolved test database URL.
/// Each isolated test pool previously re-probed every candidate URL (building
/// and dropping a probe `PgPool` each time); under `--lib --test-threads=N`
/// with thousands of DB-backed tests this connection churn collides with the
/// server's connection limit and surfaces as spurious `PoolTimedOut`
/// ("Operation timed out", P0-1 gate drift). Resolving once per process and
/// reusing the URL cuts that churn to a single probe.
static RESOLVED_TEST_DB_URL: LazyLock<Mutex<Option<String>>> = LazyLock::new(|| Mutex::new(None));

/// Resolve a test database URL from environment variables or fallback defaults.
pub async fn resolve_test_database_url() -> Result<String, String> {
    // Fast path: reuse the URL resolved earlier in this process.
    if let Some(cached) = RESOLVED_TEST_DB_URL.lock().unwrap_or_else(|e| e.into_inner()).clone() {
        return Ok(cached);
    }

    let mut errors = Vec::new();

    for database_url in candidate_database_urls() {
        // Use the same generous timeout window as the isolated pools so a
        // moment of server-side connection pressure under `--test-threads=N`
        // cannot turn a health check into a hard failure.
        let connect_future =
            PgPoolOptions::new().max_connections(1).acquire_timeout(Duration::from_secs(30)).connect(&database_url);

        match tokio::time::timeout(Duration::from_secs(30), connect_future).await {
            Err(_) => errors.push(format!("{database_url} -> connect timed out")),
            Ok(Ok(pool)) => {
                drop(pool);
                // Remember the resolved URL so subsequent tests skip probing.
                if let Ok(mut cache) = RESOLVED_TEST_DB_URL.lock() {
                    *cache = Some(database_url.clone());
                }
                return Ok(database_url);
            }
            Ok(Err(error)) => errors.push(format!("{database_url} -> {error}")),
        }
    }

    Err(format!("failed to connect to any configured test database: {}", errors.join(" | ")))
}

fn candidate_database_urls() -> Vec<String> {
    let mut urls = Vec::new();

    for key in ["TEST_DATABASE_URL", "DATABASE_URL"] {
        if let Ok(value) = std::env::var(key) {
            if !urls.iter().any(|existing| existing == &value) {
                urls.push(value);
            }
        }
    }

    // Test-DB fallback convention — asserted by
    // `tests/unit/test_db_url_convention_tests.rs`, so keep every copy in sync:
    //   * port `5432`: what CI exports, what the dev compose override publishes
    //     (`${DB_EXPOSE_PORT:-5432}:5432`) and what `init_test_public_schema.sh`
    //     defaults to;
    //   * `synapse_test` only, never the application database — a harness that
    //     silently falls back to the database under test turns a configuration
    //     mistake into data loss;
    //   * no `15432`: a dead host-forward from an older compose file. Nothing
    //     listens there, so probing it first cost a connect timeout in every
    //     DB-backed test process before falling through (H-12).
    // Fail closed under CI: a wrong TEST_DATABASE_URL must not be papered over by the
    // hard-coded localhost fallback. See synapse_common::test_isolation::test_db_fallback_allowed.
    if !synapse_common::test_isolation::test_db_fallback_allowed() {
        return urls;
    }
    for fallback in [
        "postgresql://synapse:synapse@localhost:5432/synapse_test",
        "postgresql://synapse:secret@localhost:5432/synapse_test",
    ] {
        let fallback = fallback.to_string();
        if !urls.iter().any(|existing| existing == &fallback) {
            urls.push(fallback);
        }
    }

    urls
}

/// Create an empty isolated test schema with no tables.
///
/// This is used by tests that create their own tables from scratch within
/// a fresh PostgreSQL schema. The schema is named uniquely per test run
/// and uses `SET search_path` so all queries are isolated.
///
/// The returned [`TestSchemaGuard`] owns the schema's lifecycle: the shared
/// janitor drops the schema once the last `Arc<PgPool>` clone is released
/// (at the latest, before the process exits). Hold the guard for the test's
/// duration, or take `guard.pool()` and let the janitor track the pool.
pub async fn prepare_empty_isolated_test_pool() -> Result<TestSchemaGuard, String> {
    let database_url = resolve_test_database_url().await?;
    let schema_name = next_test_schema_name();

    let admin_pool = PgPoolOptions::new()
        .max_connections(1)
        .acquire_timeout(Duration::from_secs(5))
        .connect(&database_url)
        .await
        .map_err(|error| format!("failed to connect admin pool: {error}"))?;

    sqlx::query(&format!("CREATE SCHEMA {schema_name}"))
        .execute(&admin_pool)
        .await
        .map_err(|error| format!("failed to create schema {schema_name}: {error}"))?;

    if sqlx::query(&format!("CREATE EXTENSION IF NOT EXISTS pg_trgm SCHEMA {schema_name}"))
        .execute(&admin_pool)
        .await
        .is_err()
    {
        let _ = sqlx::query("CREATE EXTENSION IF NOT EXISTS pg_trgm").execute(&admin_pool).await;
    }

    let search_path_sql = format!("SET search_path TO {schema_name}, public");
    let lease_schema = schema_name.clone();
    let pool = PgPoolOptions::new()
        .max_connections(4)
        .min_connections(0)
        .acquire_timeout(Duration::from_secs(30))
        .after_connect(move |connection, _meta| {
            let search_path_sql = search_path_sql.clone();
            let lease_schema = lease_schema.clone();
            Box::pin(async move {
                sqlx::query(&search_path_sql).execute(&mut *connection).await?;
                // Connection-derived lease: the janitor's drop waits for every
                // connection of this pool to close, so an inner `PgPool` clone
                // (`(**pool).clone()`) held by a service keeps the schema alive
                // after the fixture's `Arc<PgPool>` is dropped. The lock is
                // **shared** so this pool's own 4 connections can coexist; the
                // janitor takes the conflicting *exclusive* lock with `try`.
                sqlx::query("SELECT pg_advisory_lock_shared($1)")
                    .bind(schema_lease_key(&lease_schema))
                    .execute(&mut *connection)
                    .await?;
                Ok(())
            })
        })
        .connect(&database_url)
        .await
        .map_err(|error| format!("failed to connect isolated pool for {schema_name}: {error}"))?;

    let pool = Arc::new(pool);
    let guard = TestSchemaGuard::new_registered(
        pool,
        schema_name.clone(),
        SchemaCleanup::drop_only(&database_url, &schema_name),
    );
    Ok(guard)
}

fn next_test_schema_name() -> String {
    #[allow(clippy::expect_used)]
    let timestamp_nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .expect("system clock before unix epoch")
        .as_nanos();
    format!("test_{}_{}_{}", std::process::id(), TEST_SCHEMA_COUNTER.fetch_add(1, Ordering::SeqCst), timestamp_nanos,)
}

/// Connect a shared pool to the default test database (no schema isolation).
///
/// This is the single convergence point for the ~50 module-local `test_pool()`
/// fixtures that previously each hard-coded their own `TEST_DATABASE_URL`
/// fallback, `max_connections` and `acquire_timeout` (drift risk, bypassed the
/// process-level URL probe cache). Tests that only touch the shared `public`
/// schema tables (no per-test `CREATE SCHEMA`) should delegate to this and keep
/// their unique-suffix + manual-cleanup row isolation; tests that need a
/// dedicated schema must use [`prepare_empty_isolated_test_pool`] instead.
pub async fn connect_shared_test_pool() -> Result<Arc<PgPool>, String> {
    #[cfg(test)]
    crate::test_exit_hook::ensure();
    let database_url = resolve_test_database_url().await?;
    let pool = PgPoolOptions::new()
        .max_connections(2)
        .acquire_timeout(Duration::from_secs(30))
        .connect(&database_url)
        .await
        .map_err(|error| format!("failed to connect shared test pool: {error}"))?;
    Ok(Arc::new(pool))
}
