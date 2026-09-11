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
use synapse_common::test_schema_guard::{SchemaCleanup, TestSchemaGuard};

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

/// Queue of pre-prepared test pools that can be reused by tests.
static PREPARED_TEST_POOLS: LazyLock<Mutex<Vec<Arc<PgPool>>>> = LazyLock::new(|| Mutex::new(Vec::new()));

/// Enqueue a pre-prepared test pool for later use.
pub fn enqueue_prepared_test_pool(pool: Arc<PgPool>) {
    PREPARED_TEST_POOLS.lock().unwrap_or_else(|e| e.into_inner()).push(pool);
}

/// Take a pre-prepared test pool if one is available.
pub fn take_prepared_test_pool() -> Option<Arc<PgPool>> {
    PREPARED_TEST_POOLS.lock().unwrap_or_else(|e| e.into_inner()).pop()
}

/// Resolve a test database URL from environment variables or fallback defaults.
pub async fn resolve_test_database_url() -> Result<String, String> {
    let mut errors = Vec::new();

    for database_url in candidate_database_urls() {
        let connect_future =
            PgPoolOptions::new().max_connections(1).acquire_timeout(Duration::from_secs(5)).connect(&database_url);

        match tokio::time::timeout(Duration::from_secs(5), connect_future).await {
            Err(_) => errors.push(format!("{database_url} -> connect timed out")),
            Ok(Ok(pool)) => {
                drop(pool);
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

    // Fallback candidates, ordered by the project's documented local-dev
    // convention. `scripts/init_test_public_schema.sh` assumes the test
    // Postgres is reachable at `localhost:15432` (the Docker host-forwarded
    // port), so that is tried FIRST. `localhost:5432` is a legacy fallback
    // and is only probed if 15432 is unavailable.
    for fallback in [
        "postgresql://synapse:synapse@localhost:15432/synapse_test",
        "postgresql://synapse:synapse@localhost:15432/synapse",
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
    let pool = PgPoolOptions::new()
        .max_connections(4)
        .min_connections(0)
        .acquire_timeout(Duration::from_secs(30))
        .after_connect(move |connection, _meta| {
            let search_path_sql = search_path_sql.clone();
            Box::pin(async move {
                sqlx::query(&search_path_sql).execute(connection).await?;
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
