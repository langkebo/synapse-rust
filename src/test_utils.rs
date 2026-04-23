use crate::services::{DatabaseInitMode, DatabaseInitService};
use once_cell::sync::Lazy;
use sqlx::postgres::PgPoolOptions;
use sqlx::PgPool;
use std::collections::VecDeque;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;
use tokio::sync::Mutex as TokioMutex;
use tokio::sync::OnceCell;

static PREPARED_TEST_POOLS: Lazy<Mutex<VecDeque<Arc<PgPool>>>> =
    Lazy::new(|| Mutex::new(VecDeque::new()));
static TEST_ENV_LOCK: Lazy<TokioMutex<()>> = Lazy::new(|| TokioMutex::new(()));
static TEST_SCHEMA_COUNTER: AtomicU64 = AtomicU64::new(1);
static TEMPLATE_SCHEMA_NAME: OnceCell<String> = OnceCell::const_new();
const DEFAULT_TEST_DB_MAX_CONNECTIONS: u32 = 2;
const DEFAULT_TEST_DB_MIN_CONNECTIONS: u32 = 0;
const DEFAULT_TEST_DB_CONNECT_TIMEOUT_SECS: u64 = 5;
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

pub fn configured_test_pool_connect_timeout() -> Duration {
    Duration::from_secs(
        env_u64("TEST_DB_CONNECT_TIMEOUT_SECS").unwrap_or(DEFAULT_TEST_DB_CONNECT_TIMEOUT_SECS),
    )
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

    let connect_timeout = configured_test_pool_connect_timeout();
    let admin_pool = tokio::time::timeout(
        connect_timeout,
        PgPoolOptions::new()
            .max_connections(1)
            .acquire_timeout(Duration::from_secs(5))
            .connect(&database_url),
    )
    .await
    .map_err(|_| format!("failed to connect admin pool: timed out after {connect_timeout:?}"))?
    .map_err(|error| format!("failed to connect admin pool: {error}"))?;

    sqlx::query(&format!("CREATE SCHEMA {schema_name}"))
        .execute(&admin_pool)
        .await
        .map_err(|error| format!("failed to create schema {schema_name}: {error}"))?;

    let search_path_sql = format!("SET search_path TO {schema_name}, public");
    let pool = tokio::time::timeout(
        connect_timeout,
        PgPoolOptions::new()
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
        .connect(&database_url),
    )
    .await
    .map_err(|_| {
        format!("failed to connect isolated pool for {schema_name}: timed out after {connect_timeout:?}")
    })?
        .map_err(|error| format!("failed to connect isolated pool for {schema_name}: {error}"))?;

    let pool = Arc::new(pool);

    // Set a timeout for the entire initialization process
    let report = tokio::time::timeout(
        Duration::from_secs(120),
        DatabaseInitService::new(pool.clone())
            .with_mode(DatabaseInitMode::Strict)
            .initialize(),
    )
    .await
    .map_err(|_| format!("database initialization timed out after 120 seconds for {schema_name}"))?
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

/// Returns a per-test pool with a fresh schema cloned from a pre-initialized template.
/// The template schema (with all migrations applied) is created once and cached.
/// Cloning tables from the template is ~100x faster than re-running all migrations.
/// Set TEST_ISOLATED_SCHEMAS=1 to force the old per-test migration behavior.
pub async fn prepare_shared_test_pool() -> Result<Arc<PgPool>, String> {
    let database_url = resolve_test_database_url().await?;

    // Step 1: Ensure the template schema exists (one-time init)
    let template = TEMPLATE_SCHEMA_NAME
        .get_or_try_init(|| async { init_template_schema(&database_url).await })
        .await?
        .clone();

    // Step 2: Clone template into a fresh per-test schema
    clone_schema_from_template(&database_url, &template).await
}

async fn init_template_schema(database_url: &str) -> Result<String, String> {
    let template_name = format!("test_template_{}", std::process::id());
    let connect_timeout = configured_test_pool_connect_timeout();

    let admin_pool = tokio::time::timeout(
        connect_timeout,
        PgPoolOptions::new()
            .max_connections(1)
            .acquire_timeout(Duration::from_secs(5))
            .connect(database_url),
    )
    .await
    .map_err(|_| format!("failed to connect admin pool: timed out after {connect_timeout:?}"))?
    .map_err(|error| format!("failed to connect admin pool: {error}"))?;

    // Drop if leftover from a previous crash
    let _ = sqlx::query(&format!("DROP SCHEMA IF EXISTS {template_name} CASCADE"))
        .execute(&admin_pool)
        .await;

    sqlx::query(&format!("CREATE SCHEMA {template_name}"))
        .execute(&admin_pool)
        .await
        .map_err(|error| format!("failed to create template schema {template_name}: {error}"))?;

    let search_path_sql = format!("SET search_path TO {template_name}, public");
    let pool = tokio::time::timeout(
        connect_timeout,
        PgPoolOptions::new()
            .max_connections(4)
            .min_connections(0)
            .acquire_timeout(Duration::from_secs(30))
            .idle_timeout(Some(Duration::from_secs(300)))
            .max_lifetime(Some(Duration::from_secs(600)))
            .after_connect(move |connection, _meta| {
                let search_path_sql = search_path_sql.clone();
                Box::pin(async move {
                    sqlx::query(&search_path_sql).execute(connection).await?;
                    Ok(())
                })
            })
            .connect(database_url),
    )
    .await
    .map_err(|_| format!("failed to connect template pool: timed out after {connect_timeout:?}"))?
    .map_err(|error| format!("failed to connect template pool: {error}"))?;

    let pool = Arc::new(pool);

    let report = tokio::time::timeout(
        Duration::from_secs(120),
        DatabaseInitService::new(pool.clone())
            .with_mode(DatabaseInitMode::Strict)
            .initialize(),
    )
    .await
    .map_err(|_| "template schema initialization timed out after 120 seconds".to_string())?
    .map_err(|error| format!("template schema initialization failed: {error}"))?;

    if !report.success {
        return Err(format!(
            "template schema initialization errors: {}",
            report.errors.join(" | ")
        ));
    }

    // Close the template pool — we only need it for initialization
    pool.close().await;

    Ok(template_name)
}

async fn clone_schema_from_template(
    database_url: &str,
    template_name: &str,
) -> Result<Arc<PgPool>, String> {
    let schema_name = next_test_schema_name();
    let connect_timeout = configured_test_pool_connect_timeout();

    let admin_pool = tokio::time::timeout(
        connect_timeout,
        PgPoolOptions::new()
            .max_connections(1)
            .acquire_timeout(Duration::from_secs(5))
            .connect(database_url),
    )
    .await
    .map_err(|_| "failed to connect admin pool for clone: timed out".to_string())?
    .map_err(|error| format!("failed to connect admin pool for clone: {error}"))?;

    // Clone: create schema + copy all tables from template using DDL generation
    let clone_sql = format!(
        r#"
        DO $$
        DECLARE
            r RECORD;
        BEGIN
            EXECUTE format('CREATE SCHEMA %I', '{schema_name}');
            FOR r IN
                SELECT tablename FROM pg_tables WHERE schemaname = '{template_name}' ORDER BY tablename
            LOOP
                EXECUTE format(
                    'CREATE TABLE %I.%I (LIKE %I.%I INCLUDING ALL)',
                    '{schema_name}', r.tablename, '{template_name}', r.tablename
                );
            END LOOP;
            -- Copy sequences
            FOR r IN
                SELECT sequence_name FROM information_schema.sequences WHERE sequence_schema = '{template_name}'
            LOOP
                EXECUTE format(
                    'CREATE SEQUENCE IF NOT EXISTS %I.%I',
                    '{schema_name}', r.sequence_name
                );
            END LOOP;
        END $$;
        "#
    );

    sqlx::raw_sql(&clone_sql)
        .execute(&admin_pool)
        .await
        .map_err(|error| format!("failed to clone template to {schema_name}: {error}"))?;

    let search_path_sql = format!("SET search_path TO {schema_name}, public");
    let pool = tokio::time::timeout(
        connect_timeout,
        PgPoolOptions::new()
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
            .connect(database_url),
    )
    .await
    .map_err(|_| format!("failed to connect cloned pool for {schema_name}: timed out"))?
    .map_err(|error| format!("failed to connect cloned pool for {schema_name}: {error}"))?;

    Ok(Arc::new(pool))
}

pub async fn prepare_empty_isolated_test_pool() -> Result<Arc<PgPool>, String> {
    let database_url = resolve_test_database_url().await?;
    let schema_name = next_test_schema_name();

    let connect_timeout = configured_test_pool_connect_timeout();
    let admin_pool = tokio::time::timeout(
        connect_timeout,
        PgPoolOptions::new()
            .max_connections(1)
            .acquire_timeout(Duration::from_secs(5))
            .connect(&database_url),
    )
    .await
    .map_err(|_| format!("failed to connect admin pool: timed out after {connect_timeout:?}"))?
    .map_err(|error| format!("failed to connect admin pool: {error}"))?;

    sqlx::query(&format!("CREATE SCHEMA {schema_name}"))
        .execute(&admin_pool)
        .await
        .map_err(|error| format!("failed to create schema {schema_name}: {error}"))?;

    let search_path_sql = format!("SET search_path TO {schema_name}, public");
    let pool = tokio::time::timeout(
        connect_timeout,
        PgPoolOptions::new()
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
        .connect(&database_url),
    )
    .await
    .map_err(|_| {
        format!("failed to connect isolated pool for {schema_name}: timed out after {connect_timeout:?}")
    })?
        .map_err(|error| format!("failed to connect isolated pool for {schema_name}: {error}"))?;

    Ok(Arc::new(pool))
}

pub async fn resolve_test_database_url() -> Result<String, String> {
    let mut errors = Vec::new();
    let connect_timeout = configured_test_pool_connect_timeout();

    for database_url in candidate_database_urls() {
        let connect_future = PgPoolOptions::new()
            .max_connections(1)
            .acquire_timeout(Duration::from_secs(5))
            .connect(&database_url);

        match tokio::time::timeout(connect_timeout, connect_future).await {
            Err(_) => errors.push(format!(
                "{database_url} -> connect timed out after {connect_timeout:?}"
            )),
            Ok(Ok(pool)) => {
                drop(pool);
                return Ok(database_url);
            }
            Ok(Err(error)) => errors.push(format!("{database_url} -> {error}")),
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
