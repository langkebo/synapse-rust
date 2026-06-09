use crate::{DatabaseInitMode, DatabaseInitService};
use sqlx::postgres::PgPoolOptions;
use sqlx::PgPool;
use std::collections::VecDeque;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::LazyLock;
use std::sync::{Arc, Mutex};
use std::time::Duration;
use tokio::sync::OnceCell;
use tokio::sync::{Mutex as TokioMutex, Semaphore};

static PREPARED_TEST_POOLS: LazyLock<Mutex<VecDeque<Arc<PgPool>>>> = LazyLock::new(|| Mutex::new(VecDeque::new()));
static TEST_ENV_LOCK: LazyLock<TokioMutex<()>> = LazyLock::new(|| TokioMutex::new(()));
static TEST_SCHEMA_COUNTER: AtomicU64 = AtomicU64::new(1);
static TEMPLATE_SCHEMA_NAME: OnceCell<String> = OnceCell::const_new();
static SHARED_CLONE_SEMAPHORE: LazyLock<Semaphore> =
    LazyLock::new(|| Semaphore::new(configured_shared_clone_concurrency()));
const DEFAULT_TEST_DB_MAX_CONNECTIONS: u32 = 16;
const DEFAULT_TEST_DB_MIN_CONNECTIONS: u32 = 0;
const DEFAULT_TEST_DB_CONNECT_TIMEOUT_SECS: u64 = 5;
const DEFAULT_TEST_DB_ACQUIRE_TIMEOUT_SECS: u64 = 180;
const DEFAULT_TEST_DB_IDLE_TIMEOUT_SECS: u64 = 60;
const DEFAULT_TEST_DB_MAX_LIFETIME_SECS: u64 = 300;
const DEFAULT_TEST_DB_INIT_TIMEOUT_SECS: u64 = 300;
const DEFAULT_TEST_DB_SHARED_CLONE_CONCURRENCY: usize = 2;

pub struct EnvLockGuard {
    _guard: tokio::sync::MutexGuard<'static, ()>,
}

pub struct EnvGuard {
    original_values: Vec<(String, Option<String>)>,
}

impl EnvGuard {
    pub fn new() -> Self {
        Self { original_values: Vec::new() }
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
        if self.original_values.iter().any(|(existing_key, _)| existing_key == key) {
            return;
        }

        self.original_values.push((key.to_string(), std::env::var(key).ok()));
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
    EnvLockGuard { _guard: TEST_ENV_LOCK.blocking_lock() }
}

pub async fn env_lock_async() -> EnvLockGuard {
    EnvLockGuard { _guard: TEST_ENV_LOCK.lock().await }
}

pub fn enqueue_prepared_test_pool(pool: Arc<PgPool>) {
    PREPARED_TEST_POOLS.lock().unwrap_or_else(|e| e.into_inner()).push_back(pool);
}

pub fn take_prepared_test_pool() -> Option<Arc<PgPool>> {
    PREPARED_TEST_POOLS.lock().unwrap_or_else(|e| e.into_inner()).pop_front()
}

fn env_u32(key: &str) -> Option<u32> {
    std::env::var(key).ok().and_then(|value| value.trim().parse::<u32>().ok())
}

fn env_u64(key: &str) -> Option<u64> {
    std::env::var(key).ok().and_then(|value| value.trim().parse::<u64>().ok())
}

fn env_usize(key: &str) -> Option<usize> {
    std::env::var(key).ok().and_then(|value| value.trim().parse::<usize>().ok())
}

fn env_string(key: &str) -> Option<String> {
    std::env::var(key).ok().map(|value| value.trim().to_string()).filter(|value| !value.is_empty())
}

pub fn configured_test_pool_max_connections() -> u32 {
    env_u32("TEST_DB_MAX_CONNECTIONS").filter(|value| *value > 0).unwrap_or(DEFAULT_TEST_DB_MAX_CONNECTIONS)
}

pub fn configured_test_pool_min_connections() -> u32 {
    env_u32("TEST_DB_MIN_CONNECTIONS")
        .map_or(DEFAULT_TEST_DB_MIN_CONNECTIONS, |value| value.min(configured_test_pool_max_connections()))
}

pub fn configured_test_pool_connect_timeout() -> Duration {
    Duration::from_secs(env_u64("TEST_DB_CONNECT_TIMEOUT_SECS").unwrap_or(DEFAULT_TEST_DB_CONNECT_TIMEOUT_SECS))
}

pub fn configured_test_pool_acquire_timeout() -> Duration {
    Duration::from_secs(env_u64("TEST_DB_ACQUIRE_TIMEOUT_SECS").unwrap_or(DEFAULT_TEST_DB_ACQUIRE_TIMEOUT_SECS))
}

pub fn configured_test_pool_idle_timeout() -> Duration {
    Duration::from_secs(env_u64("TEST_DB_IDLE_TIMEOUT_SECS").unwrap_or(DEFAULT_TEST_DB_IDLE_TIMEOUT_SECS))
}

pub fn configured_test_pool_max_lifetime() -> Duration {
    Duration::from_secs(env_u64("TEST_DB_MAX_LIFETIME_SECS").unwrap_or(DEFAULT_TEST_DB_MAX_LIFETIME_SECS))
}

pub fn configured_test_db_init_timeout() -> Duration {
    Duration::from_secs(env_u64("TEST_DB_INIT_TIMEOUT_SECS").unwrap_or(DEFAULT_TEST_DB_INIT_TIMEOUT_SECS))
}

pub fn configured_shared_clone_concurrency() -> usize {
    env_usize("TEST_DB_SHARED_CLONE_CONCURRENCY")
        .filter(|value| *value > 0)
        .unwrap_or(DEFAULT_TEST_DB_SHARED_CLONE_CONCURRENCY)
}

pub fn configured_test_db_template_schema() -> Option<String> {
    env_string("TEST_DB_TEMPLATE_SCHEMA")
}

pub async fn prepare_isolated_test_pool() -> Result<Arc<PgPool>, String> {
    let database_url = resolve_test_database_url().await?;
    let schema_name = next_test_schema_name();

    let connect_timeout = configured_test_pool_connect_timeout();
    let admin_pool = tokio::time::timeout(
        connect_timeout,
        PgPoolOptions::new().max_connections(1).acquire_timeout(Duration::from_secs(5)).connect(&database_url),
    )
    .await
    .map_err(|_| format!("failed to connect admin pool: timed out after {connect_timeout:?}"))?
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
    .map_err(|_| format!("failed to connect isolated pool for {schema_name}: timed out after {connect_timeout:?}"))?
    .map_err(|error| format!("failed to connect isolated pool for {schema_name}: {error}"))?;

    let pool = Arc::new(pool);

    let init_timeout = configured_test_db_init_timeout();
    let report = tokio::time::timeout(
        init_timeout,
        DatabaseInitService::new(pool.clone()).with_mode(DatabaseInitMode::Strict).initialize(),
    )
    .await
    .map_err(|_| format!("database initialization timed out after {:?} for {schema_name}", init_timeout))?
    .map_err(|error| format!("strict migration initialization failed for {schema_name}: {error}"))?;

    if !report.is_success {
        return Err(format!(
            "strict migration initialization reported errors for {schema_name}: {}",
            report.errors.join(" | ")
        ));
    }

    ensure_test_schema_contract(&pool).await?;

    Ok(pool)
}

/// Returns a per-test pool with a fresh schema cloned from a pre-initialized template.
/// The template schema (with all migrations applied) is created once and cached.
/// Cloning tables from the template is ~100x faster than re-running all migrations.
/// Set TEST_ISOLATED_SCHEMAS=1 to force the old per-test migration behavior.
pub async fn prepare_shared_test_pool() -> Result<Arc<PgPool>, String> {
    let database_url = resolve_test_database_url().await?;

    // Step 1: Ensure the template schema exists (one-time init)
    let template = if let Some(schema_name) = configured_test_db_template_schema() {
        ensure_template_schema_exists(&database_url, &schema_name).await?;
        schema_name
    } else {
        TEMPLATE_SCHEMA_NAME.get_or_try_init(|| async { init_template_schema(&database_url).await }).await?.clone()
    };

    // Step 2: Clone template into a fresh per-test schema
    let _permit = SHARED_CLONE_SEMAPHORE.acquire().await.map_err(|_| "shared clone semaphore closed".to_string())?;
    let pool = clone_schema_from_template(&database_url, &template).await?;
    ensure_test_schema_contract(&pool).await?;
    Ok(pool)
}

async fn init_template_schema(database_url: &str) -> Result<String, String> {
    let template_name = format!("test_template_{}", std::process::id());
    let connect_timeout = configured_test_pool_connect_timeout();

    let admin_pool = tokio::time::timeout(
        connect_timeout,
        PgPoolOptions::new().max_connections(1).acquire_timeout(Duration::from_secs(5)).connect(database_url),
    )
    .await
    .map_err(|_| format!("failed to connect admin pool: timed out after {connect_timeout:?}"))?
    .map_err(|error| format!("failed to connect admin pool: {error}"))?;

    // Drop if leftover from a previous crash
    let _ = sqlx::query(&format!("DROP SCHEMA IF EXISTS {template_name} CASCADE")).execute(&admin_pool).await;

    sqlx::query(&format!("CREATE SCHEMA {template_name}"))
        .execute(&admin_pool)
        .await
        .map_err(|error| format!("failed to create template schema {template_name}: {error}"))?;

    if sqlx::query(&format!("CREATE EXTENSION IF NOT EXISTS pg_trgm SCHEMA {template_name}"))
        .execute(&admin_pool)
        .await
        .is_err()
    {
        let _ = sqlx::query("CREATE EXTENSION IF NOT EXISTS pg_trgm").execute(&admin_pool).await;
    }

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

    let init_timeout = configured_test_db_init_timeout();
    let report = tokio::time::timeout(
        init_timeout,
        DatabaseInitService::new(pool.clone()).with_mode(DatabaseInitMode::Strict).initialize(),
    )
    .await
    .map_err(|_| format!("template schema initialization timed out after {:?}", init_timeout))?
    .map_err(|error| format!("template schema initialization failed: {error}"))?;

    if !report.is_success {
        return Err(format!("template schema initialization errors: {}", report.errors.join(" | ")));
    }

    // Close the template pool — we only need it for initialization
    pool.close().await;

    Ok(template_name)
}

async fn ensure_template_schema_exists(database_url: &str, schema_name: &str) -> Result<(), String> {
    let connect_timeout = configured_test_pool_connect_timeout();
    let admin_pool = tokio::time::timeout(
        connect_timeout,
        PgPoolOptions::new().max_connections(1).acquire_timeout(Duration::from_secs(5)).connect(database_url),
    )
    .await
    .map_err(|_| format!("failed to connect admin pool: timed out after {connect_timeout:?}"))?
    .map_err(|error| format!("failed to connect admin pool: {error}"))?;

    let exists = sqlx::query_scalar::<_, bool>(
        r"
        SELECT EXISTS (
            SELECT 1
            FROM information_schema.schemata
            WHERE schema_name = $1
        )
        ",
    )
    .bind(schema_name)
    .fetch_one(&admin_pool)
    .await
    .map_err(|error| format!("failed to verify template schema {schema_name}: {error}"))?;

    if !exists {
        return Err(format!("configured template schema does not exist: {schema_name}"));
    }

    Ok(())
}

async fn clone_schema_from_template(database_url: &str, template_name: &str) -> Result<Arc<PgPool>, String> {
    let schema_name = next_test_schema_name();
    let connect_timeout = configured_test_pool_connect_timeout();

    let admin_pool = tokio::time::timeout(
        connect_timeout,
        PgPoolOptions::new().max_connections(1).acquire_timeout(Duration::from_secs(5)).connect(database_url),
    )
    .await
    .map_err(|_| "failed to connect admin pool for clone: timed out".to_string())?
    .map_err(|error| format!("failed to connect admin pool for clone: {error}"))?;

    // Clone: create schema + copy all tables from template using DDL generation
    let clone_sql = format!(
        r"
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
        "
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

    let pool = Arc::new(pool);
    ensure_test_schema_contract(&pool).await?;
    Ok(pool)
}

pub async fn prepare_empty_isolated_test_pool() -> Result<Arc<PgPool>, String> {
    let database_url = resolve_test_database_url().await?;
    let schema_name = next_test_schema_name();

    let connect_timeout = configured_test_pool_connect_timeout();
    let admin_pool = tokio::time::timeout(
        connect_timeout,
        PgPoolOptions::new().max_connections(1).acquire_timeout(Duration::from_secs(5)).connect(&database_url),
    )
    .await
    .map_err(|_| format!("failed to connect admin pool: timed out after {connect_timeout:?}"))?
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
    .map_err(|_| format!("failed to connect isolated pool for {schema_name}: timed out after {connect_timeout:?}"))?
    .map_err(|error| format!("failed to connect isolated pool for {schema_name}: {error}"))?;

    let pool = Arc::new(pool);
    Ok(pool)
}

pub async fn resolve_test_database_url() -> Result<String, String> {
    let mut errors = Vec::new();
    let connect_timeout = configured_test_pool_connect_timeout();

    for database_url in candidate_database_urls() {
        let connect_future =
            PgPoolOptions::new().max_connections(1).acquire_timeout(Duration::from_secs(5)).connect(&database_url);

        match tokio::time::timeout(connect_timeout, connect_future).await {
            Err(_) => errors.push(format!("{database_url} -> connect timed out after {connect_timeout:?}")),
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

fn next_test_schema_name() -> String {
    #[allow(clippy::expect_used)]
    let timestamp_nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .expect("system clock before unix epoch")
        .as_nanos();
    format!("test_{}_{}_{}", std::process::id(), TEST_SCHEMA_COUNTER.fetch_add(1, Ordering::SeqCst), timestamp_nanos,)
}

async fn ensure_test_schema_contract(pool: &Arc<PgPool>) -> Result<(), String> {
    sqlx::raw_sql(
        r"
        ALTER TABLE users ADD COLUMN IF NOT EXISTS email TEXT;
        ALTER TABLE users ADD COLUMN IF NOT EXISTS phone TEXT;
        ALTER TABLE users ADD COLUMN IF NOT EXISTS generation BIGINT DEFAULT 0;
        ALTER TABLE users ADD COLUMN IF NOT EXISTS consent_version TEXT;
        ALTER TABLE users ADD COLUMN IF NOT EXISTS appservice_id TEXT;
        ALTER TABLE users ADD COLUMN IF NOT EXISTS user_type TEXT;
        ALTER TABLE users ADD COLUMN IF NOT EXISTS invalid_update_at BIGINT;
        ALTER TABLE users ADD COLUMN IF NOT EXISTS migration_state TEXT;
        ALTER TABLE users ADD COLUMN IF NOT EXISTS password_changed_ts BIGINT;
        ALTER TABLE users ADD COLUMN IF NOT EXISTS is_password_change_required BOOLEAN DEFAULT FALSE;
        ALTER TABLE users ADD COLUMN IF NOT EXISTS must_change_password BOOLEAN DEFAULT FALSE;
        ALTER TABLE users ADD COLUMN IF NOT EXISTS password_expires_at BIGINT;
        ALTER TABLE users ADD COLUMN IF NOT EXISTS failed_login_attempts INTEGER DEFAULT 0;
        ALTER TABLE users ADD COLUMN IF NOT EXISTS locked_until BIGINT;

        ALTER TABLE access_tokens ADD COLUMN IF NOT EXISTS token_hash TEXT;
        ALTER TABLE access_tokens ADD COLUMN IF NOT EXISTS token TEXT;
        ALTER TABLE access_tokens ADD COLUMN IF NOT EXISTS last_used_ts BIGINT;
        ALTER TABLE access_tokens ADD COLUMN IF NOT EXISTS user_agent TEXT;
        ALTER TABLE access_tokens ADD COLUMN IF NOT EXISTS ip_address TEXT;
        ALTER TABLE access_tokens ADD COLUMN IF NOT EXISTS is_revoked BOOLEAN DEFAULT FALSE;
        ALTER TABLE access_tokens ALTER COLUMN token DROP NOT NULL;

        ALTER TABLE events ADD COLUMN IF NOT EXISTS signatures JSONB DEFAULT '{}'::jsonb;
        ALTER TABLE events ADD COLUMN IF NOT EXISTS hashes JSONB DEFAULT '{}'::jsonb;
        ALTER TABLE events ADD COLUMN IF NOT EXISTS unsigned JSONB DEFAULT '{}'::jsonb;
        ALTER TABLE events ADD COLUMN IF NOT EXISTS processed_at BIGINT;
        ALTER TABLE events ADD COLUMN IF NOT EXISTS not_before BIGINT DEFAULT 0;
        ALTER TABLE events ADD COLUMN IF NOT EXISTS status TEXT DEFAULT 'persisted';
        ALTER TABLE events ADD COLUMN IF NOT EXISTS reference_image TEXT;
        ALTER TABLE events ADD COLUMN IF NOT EXISTS origin TEXT DEFAULT 'self';
        ALTER TABLE events ADD COLUMN IF NOT EXISTS user_id TEXT;
        ALTER TABLE events ADD COLUMN IF NOT EXISTS stream_ordering BIGINT;
        ",
    )
    .execute(&**pool)
    .await
    .map_err(|error| format!("failed to ensure test schema contract: {error}"))?;

    Ok(())
}
