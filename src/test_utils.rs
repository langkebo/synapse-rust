use sqlx::postgres::PgPoolOptions;
use sqlx::PgPool;
use std::collections::VecDeque;
use std::fs;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::LazyLock;
use std::sync::{Arc, Mutex};
use std::time::Duration;
use synapse_services::database_initializer::{DatabaseInitMode, DatabaseInitService};
use tokio::sync::OnceCell;
use tokio::sync::{Mutex as TokioMutex, Semaphore};

static PREPARED_TEST_POOLS: LazyLock<Mutex<VecDeque<Arc<PgPool>>>> = LazyLock::new(|| Mutex::new(VecDeque::new()));
pub static TEST_ENV_LOCK: LazyLock<TokioMutex<()>> = LazyLock::new(|| TokioMutex::new(()));
static TEST_SCHEMA_COUNTER: AtomicU64 = AtomicU64::new(1);
static TEMPLATE_SCHEMA_NAME: OnceCell<String> = OnceCell::const_new();
static SHARED_CLONE_SEMAPHORE: LazyLock<Semaphore> =
    LazyLock::new(|| Semaphore::new(configured_shared_clone_concurrency()));
const DEFAULT_TEST_DB_MAX_CONNECTIONS: u32 = 20;
const DEFAULT_TEST_DB_MIN_CONNECTIONS: u32 = 0;
const DEFAULT_TEST_DB_CONNECT_TIMEOUT_SECS: u64 = 5;
const DEFAULT_TEST_DB_ACQUIRE_TIMEOUT_SECS: u64 = 180;
const DEFAULT_TEST_DB_IDLE_TIMEOUT_SECS: u64 = 60;
const DEFAULT_TEST_DB_MAX_LIFETIME_SECS: u64 = 300;
const DEFAULT_TEST_DB_INIT_TIMEOUT_SECS: u64 = 300;
const DEFAULT_TEST_DB_SHARED_CLONE_CONCURRENCY: usize = 4;
const TEST_TEMPLATE_SCHEMA_REVISION: u32 = 2;
const TEST_TEMPLATE_READY_MARKER_PREFIX: &str = "synapse_test_template_ready";

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
        TEMPLATE_SCHEMA_NAME
            .get_or_try_init(|| async { get_or_create_default_template_schema(&database_url).await })
            .await?
            .clone()
    };

    // Step 2: Clone template into a fresh per-test schema
    let _permit = SHARED_CLONE_SEMAPHORE.acquire().await.map_err(|_| "shared clone semaphore closed".to_string())?;
    let pool = clone_schema_from_template(&database_url, &template).await?;
    ensure_test_schema_contract(&pool).await?;
    Ok(pool)
}

async fn get_or_create_default_template_schema(database_url: &str) -> Result<String, String> {
    let template_name = default_template_schema_name();
    if template_schema_is_ready(database_url, &template_name).await? {
        tracing::debug!("Reusing existing test template schema: {}", template_name);
        return Ok(template_name);
    }

    tracing::debug!("Creating new test template schema: {}", template_name);
    init_template_schema(database_url, &template_name).await?;
    Ok(template_name)
}

async fn init_template_schema(database_url: &str, template_name: &str) -> Result<(), String> {
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

    sqlx::query(&format!("CREATE SCHEMA IF NOT EXISTS {template_name}"))
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
            .max_connections(configured_test_pool_max_connections())
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

    ensure_test_schema_contract(&pool).await?;
    mark_template_schema_ready(template_name)?;

    // Close the template pool — we only need it for initialization
    pool.close().await;

    Ok(())
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

fn template_ready_marker_path(schema_name: &str) -> std::path::PathBuf {
    let dir = std::env::var("CARGO_TARGET_TMPDIR")
        .ok()
        .map_or_else(
            || std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("target").join("tmp"),
            std::path::PathBuf::from,
        )
        .join("synapse_test_templates");
    let _ = std::fs::create_dir_all(&dir);
    dir.join(format!("{TEST_TEMPLATE_READY_MARKER_PREFIX}_{schema_name}"))
}

async fn template_schema_is_ready(database_url: &str, schema_name: &str) -> Result<bool, String> {
    let marker_path = template_ready_marker_path(schema_name);
    if !marker_path.exists() {
        return Ok(false);
    }

    // Also verify the schema still exists (in case someone dropped it manually)
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
    .map_err(|error| format!("failed to verify template schema existence for {schema_name}: {error}"))?;

    if !exists {
        // Schema disappeared, remove stale marker
        let _ = std::fs::remove_file(marker_path);
        return Ok(false);
    }

    Ok(true)
}

fn default_template_schema_name() -> String {
    format!("test_template_v{}_{}", TEST_TEMPLATE_SCHEMA_REVISION, template_schema_fingerprint())
}

fn template_schema_fingerprint() -> String {
    let mut manifest =
        format!("schema-rev:{TEST_TEMPLATE_SCHEMA_REVISION};contract-sql:{};", ensure_test_schema_contract_sql());
    let migrations_dir = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("migrations");

    let mut migration_entries = match fs::read_dir(&migrations_dir) {
        Ok(entries) => entries
            .filter_map(Result::ok)
            .filter_map(|entry| {
                let file_type = entry.file_type().ok()?;
                if !file_type.is_file() {
                    return None;
                }
                let file_name = entry.file_name();
                let file_name = file_name.to_str()?;
                let metadata = entry.metadata().ok()?;
                let modified =
                    metadata.modified().ok().and_then(|time| time.duration_since(std::time::UNIX_EPOCH).ok())?;
                Some(format!("{file_name}:{}:{};", metadata.len(), modified.as_secs()))
            })
            .collect::<Vec<_>>(),
        Err(_) => vec!["migrations-dir-missing".to_string()],
    };
    migration_entries.sort();
    for entry in migration_entries {
        manifest.push_str(&entry);
    }

    format!("{:016x}", fnv1a64(manifest.as_bytes()))
}

fn fnv1a64(bytes: &[u8]) -> u64 {
    let mut hash = 0xcbf29ce484222325u64;
    for byte in bytes {
        hash ^= u64::from(*byte);
        hash = hash.wrapping_mul(0x100000001b3);
    }
    hash
}

fn mark_template_schema_ready(schema_name: &str) -> Result<(), String> {
    let marker_path = template_ready_marker_path(schema_name);
    let timestamp = chrono::Utc::now().timestamp_millis().to_string();
    std::fs::write(&marker_path, timestamp)
        .map_err(|error| format!("failed to write template ready marker to {:?}: {error}", marker_path))?;

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

    // Clone: create schema + copy all tables from template using DDL generation.
    // INCLUDING ALL copies defaults, constraints, indexes, and storage parameters
    // (but NOT foreign keys — LIKE never copies FKs, so seed-copy order is unconstrained).
    //
    // Structure-only LIKE does not copy rows, so migration-seeded *reference/config*
    // rows (e.g. the server_media_quota id=1 row) are missing in clones. We copy those
    // for a curated allowlist of config tables so per-test clones behave like a
    // freshly-migrated DB. The development-only `@admin:localhost` seed in `users` is
    // intentionally NOT copied — tests manage their own users and many assert an empty
    // users table.
    const SEED_REFERENCE_TABLES: &[&str] = &["server_media_quota", "server_retention_policy", "sync_stream_id"];
    let seed_copy_stmts = SEED_REFERENCE_TABLES
        .iter()
        .map(|table| {
            format!(
                "                IF EXISTS (SELECT 1 FROM pg_tables WHERE schemaname = '{template_name}' AND tablename = '{table}') THEN
                    EXECUTE format('INSERT INTO %I.%I SELECT * FROM %I.%I', '{schema_name}', '{table}', '{template_name}', '{table}');
                END IF;"
            )
        })
        .collect::<Vec<_>>()
        .join("\n");
    let clone_sql = format!(
        r"
        DO $$
        DECLARE
            r RECORD;
            v_attempts INTEGER;
            v_created INTEGER;
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
            -- Copy migration seed rows for reference/config tables only.
{seed_copy_stmts}
            -- Copy sequences with their current values so id generation stays consistent.
            FOR r IN
                SELECT sequence_name FROM information_schema.sequences WHERE sequence_schema = '{template_name}'
            LOOP
                EXECUTE format(
                    'CREATE SEQUENCE IF NOT EXISTS %I.%I',
                    '{schema_name}', r.sequence_name
                );
                EXECUTE format(
                    'SELECT setval(%L, (SELECT last_value FROM %I.%I), (SELECT is_called FROM %I.%I))',
                    '{schema_name}.' || r.sequence_name,
                    '{template_name}', r.sequence_name,
                    '{template_name}', r.sequence_name
                );
            END LOOP;
            -- Clone views from the template schema.
            -- Views are NOT copied by CREATE TABLE LIKE, so we recreate them.
            -- pg_views.definition stores schema-qualified table references (resolved
            -- at creation time), so we must replace the template schema name with the
            -- new schema name to make views reference the cloned tables.
            EXECUTE format('SET search_path TO %I, public', '{schema_name}');
            v_attempts := 0;
            LOOP
                v_attempts := v_attempts + 1;
                v_created := 0;
                FOR r IN
                    SELECT viewname, definition FROM pg_views WHERE schemaname = '{template_name}' ORDER BY viewname
                LOOP
                    BEGIN
                        EXECUTE format(
                            'CREATE OR REPLACE VIEW %I.%I AS %s',
                            '{schema_name}', r.viewname,
                            REPLACE(r.definition, '{template_name}.', '{schema_name}.')
                        );
                        v_created := v_created + 1;
                    EXCEPTION WHEN OTHERS THEN
                        NULL;
                    END;
                END LOOP;
                EXIT WHEN v_created = 0 OR v_attempts >= 5;
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
    // Note: ensure_test_schema_contract is NOT called here — it is called once in
    // prepare_shared_test_pool after cloning, and the template schema already has the
    // contract applied (so cloned tables already include the contract columns).
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
    sqlx::raw_sql(ensure_test_schema_contract_sql())
        .execute(&**pool)
        .await
        .map_err(|error| format!("failed to ensure test schema contract: {error}"))?;

    Ok(())
}

fn ensure_test_schema_contract_sql() -> &'static str {
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
        "
}
