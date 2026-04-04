use crate::services::{DatabaseInitMode, DatabaseInitService};
use once_cell::sync::Lazy;
use sqlx::postgres::PgPoolOptions;
use sqlx::PgPool;
use std::collections::VecDeque;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;
use tokio::sync::Mutex as TokioMutex;

static PREPARED_TEST_POOLS: Lazy<Mutex<VecDeque<Arc<PgPool>>>> =
    Lazy::new(|| Mutex::new(VecDeque::new()));
static TEST_ENV_LOCK: Lazy<TokioMutex<()>> = Lazy::new(|| TokioMutex::new(()));
static TEST_SCHEMA_COUNTER: AtomicU64 = AtomicU64::new(1);
const DEFAULT_TEST_DB_MAX_CONNECTIONS: u32 = 4;
const DEFAULT_TEST_DB_MIN_CONNECTIONS: u32 = 0;
const DEFAULT_TEST_DB_ACQUIRE_TIMEOUT_SECS: u64 = 30;
const DEFAULT_TEST_DB_IDLE_TIMEOUT_SECS: u64 = 60;
const DEFAULT_TEST_DB_MAX_LIFETIME_SECS: u64 = 300;

pub struct EnvLockGuard {
    _guard: tokio::sync::MutexGuard<'static, ()>,
}

pub struct EnvGuard {
    original_values: Vec<(String, Option<String>)>,
}

impl EnvGuard {
    pub fn new() -> Self {
        Self {
            original_values: Vec::new(),
        }
    }

    pub fn set<K, V>(&mut self, key: K, value: V)
    where
        K: Into<String>,
        V: Into<String>,
    {
        let key = key.into();
        let value = value.into();
        self.capture_original_value(&key);
        std::env::set_var(&key, &value);
    }

    pub fn remove<K>(&mut self, key: K)
    where
        K: Into<String>,
    {
        let key = key.into();
        self.capture_original_value(&key);
        std::env::remove_var(&key);
    }

    fn capture_original_value(&mut self, key: &str) {
        if self
            .original_values
            .iter()
            .any(|(existing_key, _)| existing_key == key)
        {
            return;
        }

        self.original_values
            .push((key.to_string(), std::env::var(key).ok()));
    }
}

impl Default for EnvGuard {
    fn default() -> Self {
        Self::new()
    }
}

impl Drop for EnvGuard {
    fn drop(&mut self) {
        for (key, value) in self.original_values.iter().rev() {
            match value {
                Some(value) => std::env::set_var(key, value),
                None => std::env::remove_var(key),
            }
        }
    }
}

pub fn env_lock() -> EnvLockGuard {
    EnvLockGuard {
        _guard: TEST_ENV_LOCK.blocking_lock(),
    }
}

pub async fn env_lock_async() -> EnvLockGuard {
    EnvLockGuard {
        _guard: TEST_ENV_LOCK.lock().await,
    }
}

pub fn enqueue_prepared_test_pool(pool: Arc<PgPool>) {
    PREPARED_TEST_POOLS.lock().unwrap().push_back(pool);
}

pub fn take_prepared_test_pool() -> Option<Arc<PgPool>> {
    PREPARED_TEST_POOLS.lock().unwrap().pop_front()
}

fn env_u32(key: &str) -> Option<u32> {
    std::env::var(key)
        .ok()
        .and_then(|value| value.trim().parse::<u32>().ok())
}

fn env_u64(key: &str) -> Option<u64> {
    std::env::var(key)
        .ok()
        .and_then(|value| value.trim().parse::<u64>().ok())
}

pub fn configured_test_pool_max_connections() -> u32 {
    env_u32("TEST_DB_MAX_CONNECTIONS")
        .filter(|value| *value > 0)
        .unwrap_or(DEFAULT_TEST_DB_MAX_CONNECTIONS)
}

pub fn configured_test_pool_min_connections() -> u32 {
    env_u32("TEST_DB_MIN_CONNECTIONS")
        .map(|value| value.min(configured_test_pool_max_connections()))
        .unwrap_or(DEFAULT_TEST_DB_MIN_CONNECTIONS)
}

pub fn configured_test_pool_acquire_timeout() -> Duration {
    Duration::from_secs(
        env_u64("TEST_DB_ACQUIRE_TIMEOUT_SECS").unwrap_or(DEFAULT_TEST_DB_ACQUIRE_TIMEOUT_SECS),
    )
}

pub fn configured_test_pool_idle_timeout() -> Duration {
    Duration::from_secs(
        env_u64("TEST_DB_IDLE_TIMEOUT_SECS").unwrap_or(DEFAULT_TEST_DB_IDLE_TIMEOUT_SECS),
    )
}

pub fn configured_test_pool_max_lifetime() -> Duration {
    Duration::from_secs(
        env_u64("TEST_DB_MAX_LIFETIME_SECS").unwrap_or(DEFAULT_TEST_DB_MAX_LIFETIME_SECS),
    )
}

pub async fn prepare_isolated_test_pool() -> Result<Arc<PgPool>, String> {
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

    let search_path_sql = format!("SET search_path TO {schema_name}, public");
    let pool = PgPoolOptions::new()
        .max_connections(configured_test_pool_max_connections())
        .min_connections(configured_test_pool_min_connections())
        .acquire_timeout(configured_test_pool_acquire_timeout())
        .idle_timeout(Some(configured_test_pool_idle_timeout()))
        .max_lifetime(Some(configured_test_pool_max_lifetime()))
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

    // Set a timeout for the entire initialization process
    let report = tokio::time::timeout(
        Duration::from_secs(30),
        DatabaseInitService::new(pool.clone())
            .with_mode(DatabaseInitMode::Strict)
            .initialize(),
    )
    .await
    .map_err(|_| format!("database initialization timed out after 30 seconds for {schema_name}"))?
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
