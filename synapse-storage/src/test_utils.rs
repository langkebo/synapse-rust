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

static TEST_SCHEMA_COUNTER: AtomicU64 = AtomicU64::new(1);

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

    for fallback in [
        "postgresql://synapse:synapse@localhost:5432/synapse",
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
pub async fn prepare_empty_isolated_test_pool() -> Result<Arc<PgPool>, String> {
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

    Ok(Arc::new(pool))
}

fn next_test_schema_name() -> String {
    #[allow(clippy::expect_used)]
    let timestamp_nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .expect("system clock before unix epoch")
        .as_nanos();
    format!(
        "test_{}_{}_{}",
        std::process::id(),
        TEST_SCHEMA_COUNTER.fetch_add(1, Ordering::SeqCst),
        timestamp_nanos,
    )
}
