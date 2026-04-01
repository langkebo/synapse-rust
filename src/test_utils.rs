use crate::services::{DatabaseInitMode, DatabaseInitService};
use once_cell::sync::Lazy;
use sqlx::postgres::PgPoolOptions;
use sqlx::PgPool;
use std::collections::VecDeque;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

static PREPARED_TEST_POOLS: Lazy<Mutex<VecDeque<Arc<PgPool>>>> =
    Lazy::new(|| Mutex::new(VecDeque::new()));
static TEST_SCHEMA_COUNTER: AtomicU64 = AtomicU64::new(1);

pub fn enqueue_prepared_test_pool(pool: Arc<PgPool>) {
    PREPARED_TEST_POOLS.lock().unwrap().push_back(pool);
}

pub fn take_prepared_test_pool() -> Option<Arc<PgPool>> {
    PREPARED_TEST_POOLS.lock().unwrap().pop_front()
}

pub async fn prepare_isolated_test_pool() -> Result<Arc<PgPool>, String> {
    let database_url = resolve_test_database_url().await?;
    let schema_name = next_test_schema_name();

    let admin_pool = PgPoolOptions::new()
        .max_connections(1)
        .acquire_timeout(Duration::from_secs(10))
        .connect(&database_url)
        .await
        .map_err(|error| format!("failed to connect admin pool: {error}"))?;

    sqlx::query(&format!("CREATE SCHEMA {schema_name}"))
        .execute(&admin_pool)
        .await
        .map_err(|error| format!("failed to create schema {schema_name}: {error}"))?;

    let search_path_sql = format!("SET search_path TO {schema_name}, public");
    let pool = PgPoolOptions::new()
        .max_connections(10)
        .min_connections(1)
        .acquire_timeout(Duration::from_secs(15))
        .idle_timeout(Some(Duration::from_secs(300)))
        .max_lifetime(Some(Duration::from_secs(900)))
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
    let report = DatabaseInitService::new(pool.clone())
        .with_mode(DatabaseInitMode::Strict)
        .initialize()
        .await
        .map_err(|error| {
            format!("strict migration initialization failed for {schema_name}: {error}")
        })?;

    if !report.success {
        return Err(format!(
            "strict migration initialization reported errors for {schema_name}: {}",
            report.errors.join(" | ")
        ));
    }

    Ok(pool)
}

pub async fn resolve_test_database_url() -> Result<String, String> {
    let mut errors = Vec::new();

    for database_url in candidate_database_urls() {
        match PgPoolOptions::new()
            .max_connections(1)
            .acquire_timeout(Duration::from_secs(5))
            .connect(&database_url)
            .await
        {
            Ok(pool) => {
                drop(pool);
                return Ok(database_url);
            }
            Err(error) => errors.push(format!("{database_url} -> {error}")),
        }
    }

    Err(format!(
        "failed to connect to any configured test database: {}",
        errors.join(" | ")
    ))
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

fn next_test_schema_name() -> String {
    format!(
        "test_{}_{}",
        std::process::id(),
        TEST_SCHEMA_COUNTER.fetch_add(1, Ordering::SeqCst)
    )
}
