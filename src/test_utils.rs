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
use tokio::sync::{Mutex as TokioMutex, RwLock as TokioRwLock, Semaphore};

static PREPARED_TEST_POOLS: LazyLock<Mutex<VecDeque<Arc<PgPool>>>> = LazyLock::new(|| Mutex::new(VecDeque::new()));
pub static TEST_ENV_LOCK: LazyLock<TokioMutex<()>> = LazyLock::new(|| TokioMutex::new(()));
static TEST_SCHEMA_COUNTER: AtomicU64 = AtomicU64::new(1);
static TEMPLATE_SCHEMA_NAME: OnceCell<String> = OnceCell::const_new();
static SHARED_CLONE_SEMAPHORE: LazyLock<Semaphore> =
    LazyLock::new(|| Semaphore::new(configured_shared_clone_concurrency()));

// ============================================================================
// Schema pool (P0 optimization): reuse TRUNCATEd schemas across tests
// ============================================================================
//
// Before: every test called `clone_schema_from_template()` which runs a heavy
// PL/pgSQL DO block (CREATE SCHEMA + CREATE TABLE LIKE x N + DROP/CREATE
// INDEX + seed copy + sequences + views) — 35-60s per test on a cold DB.
//
// After: the first N tests clone schemas (N = parallelism). On Drop, each
// schema is TRUNCATEd (fast — ~1-2s) and pushed to SCHEMA_POOL. Subsequent
// tests pop a pre-TRUNCATEd schema from the pool, skipping the clone entirely.
//
// Expected speedup: 35-60s → 1-3s per test (15-20x faster).
//
// Design notes:
// - SCHEMA_POOL stores only schema NAMES (Strings), not PgPools, to avoid
//   cross-runtime pool issues (each test runtime is short-lived; a pool
//   created on one runtime breaks when that runtime is dropped).
// - Cleanup (TRUNCATE + re-seed) runs on a dedicated CLEANUP_RUNTIME that
//   persists for the whole process lifetime, since `Drop::drop` is sync and
//   cannot await. The cleanup task creates its own admin connection.
// - Schemas corrupted by destructive tests (DROP TABLE, ALTER) are detected
//   by a table-count safety check and DROPped instead of pooled.

static SCHEMA_POOL: TokioMutex<Vec<String>> = TokioMutex::const_new(Vec::new());

// RwLock to prevent deadlock between init_template_schema (write lock, ALTER
// TABLE on template) and clone_schema_from_template (read lock, CREATE TABLE
// LIKE on template). Without this, the ALTER TABLE's AccessExclusiveLock and
// CREATE TABLE LIKE's AccessShareLock deadlock on first run when OnceCell
// initialization is retried after a runtime cancellation.
static TEMPLATE_RW_LOCK: TokioRwLock<()> = TokioRwLock::const_new(());

#[allow(clippy::expect_used)]
static CLEANUP_RUNTIME: LazyLock<tokio::runtime::Runtime> = LazyLock::new(|| {
    tokio::runtime::Builder::new_multi_thread()
        .worker_threads(2)
        .enable_all()
        .thread_name("test-schema-cleanup")
        .build()
        .expect("failed to build test schema cleanup runtime")
});
const DEFAULT_TEST_DB_MAX_CONNECTIONS: u32 = 40;
const DEFAULT_TEST_DB_MIN_CONNECTIONS: u32 = 0;
const DEFAULT_TEST_DB_CONNECT_TIMEOUT_SECS: u64 = 5;
const DEFAULT_TEST_DB_ACQUIRE_TIMEOUT_SECS: u64 = 30;
const DEFAULT_TEST_DB_IDLE_TIMEOUT_SECS: u64 = 60;
const DEFAULT_TEST_DB_MAX_LIFETIME_SECS: u64 = 300;
const DEFAULT_TEST_DB_INIT_TIMEOUT_SECS: u64 = 300;
// P1: raised from 8 → 12 to match nextest ci test-threads=12.
// Each parallel test may clone the template schema concurrently; DB pool
// max=40 per pool, PostgreSQL max_connections=100 supports 12*~5=60 conns.
const DEFAULT_TEST_DB_SHARED_CLONE_CONCURRENCY: usize = 12;
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

    // Ensure pg_trgm is in `public` schema (see init_template_schema for rationale).
    let _ = sqlx::query("CREATE EXTENSION IF NOT EXISTS pg_trgm SCHEMA public").execute(&admin_pool).await;
    let _ = sqlx::query("ALTER EXTENSION pg_trgm SET SCHEMA public").execute(&admin_pool).await;

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
///
/// Note: For new code, prefer `acquire_pooled_schema()` which reuses TRUNCATEd
/// schemas across tests (15-20x faster than cloning on every call).
pub async fn prepare_shared_test_pool() -> Result<Arc<PgPool>, String> {
    let database_url = resolve_test_database_url().await?;
    let template = get_template_schema_name(&database_url).await?;

    // Clone template into a fresh per-test schema
    let _permit = SHARED_CLONE_SEMAPHORE.acquire().await.map_err(|_| "shared clone semaphore closed".to_string())?;
    let (pool, _schema_name) = clone_schema_from_template(&database_url, &template).await?;
    // Note: ensure_test_schema_contract is NOT called here — the template schema
    // already has all contract columns applied during init_template_schema(), and
    // CREATE TABLE LIKE ... INCLUDING ALL copies them to clones. Calling it again
    // was redundant (21 ALTER TABLE ADD COLUMN IF NOT EXISTS per test — pure waste).
    Ok(pool)
}

/// Resolve the template schema name, creating it if necessary (one-time init).
/// Extracted from prepare_shared_test_pool for reuse by acquire_pooled_schema.
async fn get_template_schema_name(database_url: &str) -> Result<String, String> {
    if let Some(schema_name) = configured_test_db_template_schema() {
        ensure_template_schema_exists(database_url, &schema_name).await?;
        Ok(schema_name)
    } else {
        Ok(TEMPLATE_SCHEMA_NAME
            .get_or_try_init(|| async { get_or_create_default_template_schema(database_url).await })
            .await?
            .clone())
    }
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

    // Clean up leftover tables in `public` schema from historical test runs.
    // If public.{table} exists, migration's `CREATE TABLE IF NOT EXISTS {table}`
    // would skip creating it in the template schema — causing missing-table
    // errors in cloned schemas. Dropping and recreating `public` is safe in
    // test envs because test data lives in test_XXX schemas, not public.
    // We use DROP SCHEMA CASCADE (single operation) instead of per-table DROP
    // to avoid "out of shared memory" when hundreds of leftover tables exist.
    let _ = sqlx::query("DROP SCHEMA IF EXISTS public CASCADE").execute(&admin_pool).await;
    let _ = sqlx::query("CREATE SCHEMA public").execute(&admin_pool).await;

    sqlx::query(&format!("CREATE SCHEMA IF NOT EXISTS {template_name}"))
        .execute(&admin_pool)
        .await
        .map_err(|error| format!("failed to create template schema {template_name}: {error}"))?;

    // Install pg_trgm in `public` schema (NOT template_name) so its functions
    // (similarity(), % operator) are resolvable from any test schema via the
    // standard search_path `test_XXX, public`. Installing in template_name
    // would hide the functions from clones whose search_path is `test_XXX, public`
    // (template_name is NOT in the search_path of cloned/pooled schemas).
    // `CREATE EXTENSION IF NOT EXISTS` is a no-op if the extension already exists
    // (in any schema), so this is safe to call on every template init.
    let _ = sqlx::query("CREATE EXTENSION IF NOT EXISTS pg_trgm SCHEMA public").execute(&admin_pool).await;
    // If the extension was previously installed in a different schema (e.g. by
    // an older version of this code), move it to public so functions are
    // accessible. ALTER EXTENSION ... SET SCHEMA is idempotent.
    let _ = sqlx::query("ALTER EXTENSION pg_trgm SET SCHEMA public").execute(&admin_pool).await;

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

    // Acquire write lock so no clone_schema_from_template can run concurrently.
    // The ALTER TABLEs below take AccessExclusiveLock on template tables; a
    // concurrent CREATE TABLE LIKE (AccessShareLock) would deadlock.
    {
        let _write_guard = TEMPLATE_RW_LOCK.write().await;
        ensure_test_schema_contract(&pool).await?;
    }
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

/// Clone the template schema into a fresh per-test schema and return the pool
/// plus the schema name. The schema name is needed by the pool's lease tracker
/// to TRUNCATE/DROP it later.
async fn clone_schema_from_template(database_url: &str, template_name: &str) -> Result<(Arc<PgPool>, String), String> {
    let schema_name = next_test_schema_name();
    let connect_timeout = configured_test_pool_connect_timeout();

    // Acquire read lock so init_template_schema (write lock) cannot run
    // concurrently. Multiple clones can run concurrently (shared read lock).
    let _read_guard = TEMPLATE_RW_LOCK.read().await;

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
                    RAISE WARNING 'seed copy: inserted % rows into {schema_name}.{table}', (SELECT count(*) FROM {schema_name}.{table});
                ELSE
                    RAISE NOTICE 'seed copy: {table} not found in template {template_name}, skipping';
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
            -- Restore original index names from template.
            -- CREATE TABLE LIKE ... INCLUDING ALL copies indexes but PostgreSQL
            -- assigns auto-generated names. Tests check for specific index names
            -- via has_index_named(), so we drop non-constraint indexes (which
            -- have auto-generated names) and recreate them from the template's
            -- index definitions (which have the original names).
            FOR r IN
                SELECT c.relname AS idx_name
                FROM pg_class c
                JOIN pg_namespace n ON n.oid = c.relnamespace AND n.nspname = '{schema_name}'
                JOIN pg_index i ON i.indexrelid = c.oid
                WHERE c.relkind = 'i'
                  AND NOT EXISTS (
                      SELECT 1 FROM pg_constraint con WHERE con.conindid = c.oid
                  )
            LOOP
                EXECUTE format('DROP INDEX %I.%I', '{schema_name}', r.idx_name);
            END LOOP;
            FOR r IN
                SELECT t.indexdef AS def
                FROM pg_indexes t
                WHERE t.schemaname = '{template_name}'
                  AND NOT EXISTS (
                      SELECT 1 FROM pg_constraint con
                      JOIN pg_class c ON c.oid = con.conindid
                      JOIN pg_namespace n ON n.oid = c.relnamespace AND n.nspname = '{template_name}'
                      WHERE c.relname = t.indexname
                  )
            LOOP
                BEGIN
                    EXECUTE REPLACE(r.def, '{template_name}.', '{schema_name}.');
                EXCEPTION WHEN OTHERS THEN
                    NULL;
                END;
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
    // Note: ensure_test_schema_contract is NOT called here — the template schema
    // already has all contract columns applied (during init_template_schema), and
    // CREATE TABLE LIKE ... INCLUDING ALL copies them to clones automatically.
    Ok((pool, schema_name))
}

// ============================================================================
// Schema pool (P0 optimization): LeasedSchema + acquire_pooled_schema
// ============================================================================

struct LeasedSchemaInner {
    schema_name: String,
    template_name: String,
    database_url: String,
    poisoned: bool,
}

/// A schema leased from the pool. On Drop, the schema is either TRUNCATEd and
/// returned to `SCHEMA_POOL` (for reuse by subsequent tests) or DROPped (if
/// poisoned by a destructive test that modified schema structure).
///
/// The cleanup runs asynchronously on `CLEANUP_RUNTIME` (a dedicated background
/// runtime) because `Drop::drop` is sync and cannot await.
pub struct LeasedSchema {
    /// The PgPool connected to this schema's search_path. Tests use this directly.
    pub pool: Arc<PgPool>,
    inner: Option<LeasedSchemaInner>,
}

impl LeasedSchema {
    /// Mark as poisoned — the schema will be DROPped on release instead of
    /// TRUNCATEd. Call this in tests that modify schema structure (DROP TABLE,
    /// ALTER, etc.) so the corrupted schema doesn't get reused.
    pub fn poison(&mut self) {
        if let Some(inner) = self.inner.as_mut() {
            inner.poisoned = true;
        }
    }
}

impl Drop for LeasedSchema {
    fn drop(&mut self) {
        let Some(inner) = self.inner.take() else { return };
        let LeasedSchemaInner { schema_name, template_name, database_url, poisoned } = inner;

        // Spawn cleanup on the dedicated background runtime. This runtime
        // persists for the whole process, so the task completes even after the
        // test's own tokio runtime is dropped.
        CLEANUP_RUNTIME.spawn(async move {
            let admin_pool = tokio::time::timeout(
                Duration::from_secs(10),
                PgPoolOptions::new().max_connections(1).acquire_timeout(Duration::from_secs(5)).connect(&database_url),
            )
            .await;

            let admin_pool = match admin_pool {
                Ok(Ok(pool)) => pool,
                _ => {
                    eprintln!(
                        "schema pool: failed to connect admin pool for cleanup of {schema_name}; schema orphaned"
                    );
                    return;
                }
            };

            if poisoned {
                let _ = drop_schema(&admin_pool, &schema_name).await;
                admin_pool.close().await;
                return;
            }

            match truncate_and_reseed_schema(&admin_pool, &schema_name, &template_name).await {
                Ok(()) => {
                    // Safety check passed — return schema name to pool for reuse
                    SCHEMA_POOL.lock().await.push(schema_name);
                }
                Err(error) => {
                    eprintln!("schema pool: cleanup failed for {schema_name} ({error}); dropping schema");
                    let _ = drop_schema(&admin_pool, &schema_name).await;
                }
            }
            admin_pool.close().await;
        });
    }
}

/// Acquire a schema from the pool. Fast path: pop a pre-TRUNCATEd schema name
/// from `SCHEMA_POOL` and create a fresh pool for it. Slow path: clone a new
/// schema from the template (first N tests only, where N = parallelism).
///
/// The returned `LeasedSchema` auto-cleans on Drop — no explicit release needed.
/// Tests using `TestContext` get this automatically; no test code changes required.
pub async fn acquire_pooled_schema() -> Result<LeasedSchema, String> {
    let database_url = resolve_test_database_url().await?;
    let template_name = get_template_schema_name(&database_url).await?;

    // Fast path: reuse a TRUNCATEd schema from the pool.
    // Validate table count against template before reusing — a corrupted schema
    // (e.g. one where a destructive test DROPped tables) must not be reused.
    while let Some(schema_name) = SCHEMA_POOL.lock().await.pop() {
        if validate_schema_table_count(&database_url, &schema_name, &template_name).await {
            let pool = create_pool_for_schema(&database_url, &schema_name).await?;
            return Ok(LeasedSchema {
                pool,
                inner: Some(LeasedSchemaInner { schema_name, template_name, database_url, poisoned: false }),
            });
        }
        // Schema corrupted — drop it and try the next one from the pool
        eprintln!("schema pool: dropping corrupted schema {schema_name} (table count mismatch)");
        if let Ok(admin_pool) =
            PgPoolOptions::new().max_connections(1).acquire_timeout(Duration::from_secs(5)).connect(&database_url).await
        {
            let _ = drop_schema(&admin_pool, &schema_name).await;
            admin_pool.close().await;
        }
    }

    // Slow path: clone a new schema from the template
    let _permit = SHARED_CLONE_SEMAPHORE.acquire().await.map_err(|_| "shared clone semaphore closed".to_string())?;
    let (pool, schema_name) = clone_schema_from_template(&database_url, &template_name).await?;
    drop(_permit);

    Ok(LeasedSchema {
        pool,
        inner: Some(LeasedSchemaInner { schema_name, template_name, database_url, poisoned: false }),
    })
}

/// Validate that a schema has the same table count as the template.
/// Returns false if the schema is corrupted (missing tables).
async fn validate_schema_table_count(database_url: &str, schema_name: &str, template_name: &str) -> bool {
    let Ok(admin_pool) =
        PgPoolOptions::new().max_connections(1).acquire_timeout(Duration::from_secs(5)).connect(database_url).await
    else {
        return true; // Can't validate — assume OK (fail open, not blocking tests)
    };

    let result = async {
        let count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM pg_tables WHERE schemaname = $1")
            .bind(schema_name)
            .fetch_one(&admin_pool)
            .await?;
        let template: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM pg_tables WHERE schemaname = $1")
            .bind(template_name)
            .fetch_one(&admin_pool)
            .await?;
        Ok::<_, sqlx::Error>(count >= template)
    }
    .await;

    admin_pool.close().await;
    result.unwrap_or(true) // Fail open on query errors
}

/// Create a fresh PgPool for an existing schema, setting search_path on connect.
/// Used when popping a schema name from the pool (the pool stores names only,
/// not PgPools, to avoid cross-runtime pool issues).
async fn create_pool_for_schema(database_url: &str, schema_name: &str) -> Result<Arc<PgPool>, String> {
    let search_path_sql = format!("SET search_path TO {schema_name}, public");
    let connect_timeout = configured_test_pool_connect_timeout();

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
    .map_err(|_| format!("failed to connect pooled schema {schema_name}: timed out after {connect_timeout:?}"))?
    .map_err(|error| format!("failed to connect pooled schema {schema_name}: {error}"))?;

    Ok(Arc::new(pool))
}

/// TRUNCATE all tables in the schema and re-seed reference/config tables from
/// the template. This is the fast cleanup path (~1-2s) that makes schemas
/// reusable across tests without re-cloning.
///
/// Safety check: if the table count drops below a threshold (indicating a
/// destructive test corrupted the schema), returns an error so the caller
/// DROPs the schema instead of pooling it.
async fn truncate_and_reseed_schema(admin_pool: &PgPool, schema_name: &str, template_name: &str) -> Result<(), String> {
    // Safety check: compare table count against the template. A healthy clone
    // must have the same number of tables as the template. If a destructive
    // test dropped some, we detect it here and refuse to pool the corrupted schema.
    let table_count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM pg_tables WHERE schemaname = $1")
        .bind(schema_name)
        .fetch_one(admin_pool)
        .await
        .map_err(|e| format!("failed to count tables in {schema_name}: {e}"))?;

    let template_count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM pg_tables WHERE schemaname = $1")
        .bind(template_name)
        .fetch_one(admin_pool)
        .await
        .map_err(|e| format!("failed to count tables in template {template_name}: {e}"))?;

    if table_count < template_count {
        return Err(format!(
            "schema {schema_name} has {table_count} tables but template has {template_count} — likely corrupted by a destructive test"
        ));
    }

    // TRUNCATE all tables in one statement (CASCADE handles FK constraints;
    // RESTART IDENTITY resets sequences). This is much faster than DROP+CREATE.
    let trunc_list: Option<String> = sqlx::query_scalar(
        r"
        SELECT string_agg(format('%I.%I', schemaname, tablename), ', ')
        FROM pg_tables WHERE schemaname = $1
        ",
    )
    .bind(schema_name)
    .fetch_one(admin_pool)
    .await
    .map_err(|e| format!("failed to build TRUNCATE list for {schema_name}: {e}"))?;

    if let Some(trunc_list) = trunc_list {
        let sql = format!("TRUNCATE TABLE {trunc_list} RESTART IDENTITY CASCADE");
        sqlx::raw_sql(&sql).execute(admin_pool).await.map_err(|e| format!("TRUNCATE failed for {schema_name}: {e}"))?;
    }

    // Re-seed reference/config tables from the template (same 3 tables that
    // clone_schema_from_template copies). These are small and fast to copy.
    for table in &["server_media_quota", "server_retention_policy", "sync_stream_id"] {
        let exists_in_template: bool = sqlx::query_scalar::<_, bool>(
            "SELECT EXISTS (SELECT 1 FROM pg_tables WHERE schemaname = $1 AND tablename = $2)",
        )
        .bind(template_name)
        .bind(*table)
        .fetch_one(admin_pool)
        .await
        .map_err(|e| format!("failed to check {table} in template: {e}"))?;

        if exists_in_template {
            let sql = format!(
                "INSERT INTO {schema_name}.{table} SELECT * FROM {template_name}.{table} ON CONFLICT DO NOTHING"
            );
            // Use raw_sql since table/schema names are validated identifiers
            if let Err(e) = sqlx::raw_sql(&sql).execute(admin_pool).await {
                // Seed copy failure IS fatal — tests that depend on config rows
                // (e.g. media upload needs server_media_quota id=1) will fail
                // silently if we pool a schema with missing seed data.
                return Err(format!(
                    "seed copy for {table} failed in {schema_name}: {e} — schema will be dropped, not pooled"
                ));
            }
        } else {
            eprintln!("schema pool: {table} not found in template {template_name}, skipping reseed");
        }
    }

    Ok(())
}

/// DROP a schema entirely (CASCADE). Used when a schema is poisoned or
/// TRUNCATE cleanup fails, so corrupted schemas don't get reused.
async fn drop_schema(pool: &PgPool, schema_name: &str) -> Result<(), String> {
    sqlx::query(&format!("DROP SCHEMA IF EXISTS {schema_name} CASCADE"))
        .execute(pool)
        .await
        .map_err(|e| format!("DROP SCHEMA {schema_name} failed: {e}"))?;
    Ok(())
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

    // Ensure pg_trgm is in `public` schema (see init_template_schema for rationale).
    let _ = sqlx::query("CREATE EXTENSION IF NOT EXISTS pg_trgm SCHEMA public").execute(&admin_pool).await;
    let _ = sqlx::query("ALTER EXTENSION pg_trgm SET SCHEMA public").execute(&admin_pool).await;

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
