//! Shared test infrastructure used by the root crate, the extracted HTTP crate
//! and the integration/unit test targets: per-test schema leasing, environment
//! guards, pooled/template test databases.
//!
//! Extracted from the root crate's `src/test_utils.rs` so that `synapse-web` can
//! use it without depending on the root crate (B4-5b).

// Test code may use unwrap/expect/panic per Rust testing idiom; production lib
// code is held to the strict clippy config in [lints].
#![cfg_attr(test, allow(clippy::unwrap_used, clippy::expect_used, clippy::panic))]

use sqlx::postgres::PgPoolOptions;
use sqlx::PgPool;
use std::collections::VecDeque;
use std::fs;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::LazyLock;
use std::sync::{Arc, Mutex, Once};
use std::time::Duration;
use synapse_common::current_timestamp_millis;
use synapse_common::test_schema_guard::{
    drop_schema_blocking, register_exit_callback, register_schema_cleanup, run_cleanup_blocking, running_under_nextest,
    CleanupFn, SchemaCleanup,
};
use synapse_services::database_initializer::{DatabaseInitMode, DatabaseInitService};
use tokio::sync::OnceCell;
use tokio::sync::{Mutex as TokioMutex, RwLock as TokioRwLock, Semaphore};

static PREPARED_TEST_POOLS: LazyLock<Mutex<VecDeque<Arc<PgPool>>>> = LazyLock::new(|| Mutex::new(VecDeque::new()));
/// Process-wide cache of the resolved test database URL.
///
/// Mirrors `synapse_services::test_utils::RESOLVED_TEST_DB_URL`. Each isolated
/// test pool previously re-probed every candidate URL (building and dropping a
/// probe `PgPool` each time); under `--workspace --lib --test-threads=N` with
/// thousands of DB-backed tests this connection churn collides with the
/// server's connection limit and surfaces as spurious `PoolTimedOut`
/// ("Operation timed out", P0-1 gate drift,
/// docs/audit/AUDIT_SUMMARY_2026-09-12.md). Resolving once per process and
/// reusing the URL cuts that churn to a single probe.
static RESOLVED_TEST_DB_URL: LazyLock<Mutex<Option<String>>> = LazyLock::new(|| Mutex::new(None));
/// Static `TEST_ENV_LOCK`.
pub static TEST_ENV_LOCK: LazyLock<TokioMutex<()>> = LazyLock::new(|| TokioMutex::new(()));
static TEST_SCHEMA_COUNTER: AtomicU64 = AtomicU64::new(1);
static TEMPLATE_SCHEMA_NAME: OnceCell<String> = OnceCell::const_new();
// Cached table names for the template schema. The template is created once and
// never modified, so these are populated lazily on first use and reused across
// all tests. This eliminates repeated slow `pg_tables`/`pg_class` queries
// (each taking 1.6-8s under parallel test load due to catalog lock contention).
static TEMPLATE_TABLE_NAMES: OnceCell<Vec<String>> = OnceCell::const_new();
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

// ============================================================================
// Schema lifecycle — delegated to the shared janitor
// ============================================================================
//
// This module used to keep its own `PENDING_SCHEMA_RETURNS` registry plus a
// `schedule_pending_schema_cleanup()` sweep invoked on the *next* pool
// acquisition. That design is structurally broken under cargo-nextest, which
// runs exactly one test case per process: the "next acquisition" never happens
// in the process that created the schema, so every schema leaked. That is how
// the local test database reached 23,662 leftover schemas
// (docs/audit/P5_test_schema_accumulation_2026-09-12.md).
//
// Cleanup is now owned by the shared engine in `synapse_common::test_schema_guard`:
// the janitor watches each pool's weak reference and runs the cleanup as soon
// as the last `Arc<PgPool>` is released, with an `atexit` join as the
// deterministic backstop. This works identically under `cargo test` (many tests
// per process) and nextest (one test per process).
//
// The P0 TRUNCATE-and-reuse optimization is preserved via `SchemaCleanup::
// release_or_exit_drop`: under `cargo test` the `on_release` path TRUNCATEs the
// schema and returns its name to `SCHEMA_POOL` for the next test to pop; under
// nextest (where reuse can never pay off) the `on_release` path is a plain DROP.
// The `on_exit` path is always a plain DROP — returning a name to a pool that
// is about to die is pointless.
//
// Schemas parked in `SCHEMA_POOL` as *names* (no live pool) are drained by a
// one-time `register_exit_callback` installed when the pool is first used.
static SCHEMA_POOL_EXIT_DRAIN_ONCE: Once = Once::new();

// Serialize background schema cleanup (TRUNCATE + re-seed) to a single concurrent
// task. Each TRUNCATE acquires ACCESS EXCLUSIVE locks on ~111 tables; running
// several concurrently with foreground clone DO-blocks (which take ~1000 locks
// each) exhausts PostgreSQL's shared lock table (`max_locks_per_transaction`)
// and triggers "out of shared memory" at modest test parallelism. One-at-a-time
// cleanup keeps peak lock pressure bounded without blocking test execution.
static CLEANUP_SEMAPHORE: LazyLock<Semaphore> = LazyLock::new(|| Semaphore::new(1));

/// Env kill-switch for the schema-pool reuse fast path in
/// `prepare_shared_test_pool`. Set `TEST_SCHEMA_POOL_REUSE=0` to fall back to
/// the previous clone-only behaviour (used to bisect regressions).
fn test_schema_pool_reuse_enabled() -> bool {
    std::env::var("TEST_SCHEMA_POOL_REUSE").map(|v| v != "0" && !v.eq_ignore_ascii_case("false")).unwrap_or(true)
}

// RwLock to prevent deadlock between init_template_schema (write lock, ALTER
// TABLE on template) and clone_schema_from_template (read lock, CREATE TABLE
// LIKE on template). Without this, the ALTER TABLE's AccessExclusiveLock and
// CREATE TABLE LIKE's AccessShareLock deadlock on first run when OnceCell
// initialization is retried after a runtime cancellation.
static TEMPLATE_RW_LOCK: TokioRwLock<()> = TokioRwLock::const_new(());

const DEFAULT_TEST_DB_MAX_CONNECTIONS: u32 = 40;
const DEFAULT_TEST_DB_MIN_CONNECTIONS: u32 = 0;
const DEFAULT_TEST_DB_CONNECT_TIMEOUT_SECS: u64 = 30;
const DEFAULT_TEST_DB_ACQUIRE_TIMEOUT_SECS: u64 = 30;
const DEFAULT_TEST_DB_IDLE_TIMEOUT_SECS: u64 = 60;
const DEFAULT_TEST_DB_MAX_LIFETIME_SECS: u64 = 300;
const DEFAULT_TEST_DB_INIT_TIMEOUT_SECS: u64 = 300;
// P1: raised from 8 → 12 to match nextest ci test-threads=12.
//
// Connection-budget reality check (corrected 2026-09-11; the previous comment
// claimed "PostgreSQL max_connections=100 supports 12*~5=60 conns", but `~5` was
// a guess at *actual* usage while the pool is configured to grow to
// DEFAULT_TEST_DB_MAX_CONNECTIONS = 40):
//
//   - `tests/common::get_test_pool_async()` builds a NEW pool per test (there is
//     no shared/static pool), so each concurrently running test owns a pool that
//     may hold up to 40 connections.
//   - Worst-case demand is therefore `concurrency × pool_max`. At ci
//     test-threads=12 that is 12 × 40 = 480, far beyond PostgreSQL's default
//     max_connections=100.
//   - This semaphore only bounds concurrent *template-schema clones*; it does
//     NOT bound connections.
//
// Consequence (measured, see docs/audit/P0_baseline_2026-09-10.md §2.2.1): the
// same commit reports "1417 passed / 9 flagged" at low system load and
// "1408 passed, 12 failed, 7 timed out" under load, while the timed-out set
// passes 13/13 when re-run serially. That is connection starvation, not a code
// defect. Keep heavy groups on low `--test-threads`, or lower
// TEST_DB_MAX_CONNECTIONS. `tests/unit/test_connection_budget_tests.rs` prints
// the live numbers and fails if a single pool could exhaust the server.
const DEFAULT_TEST_DB_SHARED_CLONE_CONCURRENCY: usize = 12;
const TEST_TEMPLATE_SCHEMA_REVISION: u32 = 2;
const TEST_TEMPLATE_READY_MARKER_PREFIX: &str = "synapse_test_template_ready";

/// The `EnvLockGuard` struct.
pub struct EnvLockGuard {
    _guard: tokio::sync::MutexGuard<'static, ()>,
}

/// The `EnvGuard` struct.
pub struct EnvGuard {
    original_values: Vec<(String, Option<String>)>,
}

impl EnvGuard {
    /// See [`new`].
    /// See [`new`].
    pub fn new() -> Self {
        Self { original_values: Vec::new() }
    }

    /// See [`set`].
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

    /// See [`remove`].
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

/// See [`env_lock`].
pub fn env_lock() -> EnvLockGuard {
    EnvLockGuard { _guard: TEST_ENV_LOCK.blocking_lock() }
}

/// See [`env_lock_async`].
pub async fn env_lock_async() -> EnvLockGuard {
    EnvLockGuard { _guard: TEST_ENV_LOCK.lock().await }
}

/// See [`enqueue_prepared_test_pool`].
pub fn enqueue_prepared_test_pool(pool: Arc<PgPool>) {
    PREPARED_TEST_POOLS.lock().unwrap_or_else(|e| e.into_inner()).push_back(pool);
}

/// See [`take_prepared_test_pool`].
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

/// See [`configured_test_pool_max_connections`].
pub fn configured_test_pool_max_connections() -> u32 {
    env_u32("TEST_DB_MAX_CONNECTIONS").filter(|value| *value > 0).unwrap_or(DEFAULT_TEST_DB_MAX_CONNECTIONS)
}

/// See [`configured_test_pool_min_connections`].
pub fn configured_test_pool_min_connections() -> u32 {
    env_u32("TEST_DB_MIN_CONNECTIONS")
        .map_or(DEFAULT_TEST_DB_MIN_CONNECTIONS, |value| value.min(configured_test_pool_max_connections()))
}

/// See [`configured_test_pool_connect_timeout`].
pub fn configured_test_pool_connect_timeout() -> Duration {
    Duration::from_secs(env_u64("TEST_DB_CONNECT_TIMEOUT_SECS").unwrap_or(DEFAULT_TEST_DB_CONNECT_TIMEOUT_SECS))
}

/// See [`configured_test_pool_acquire_timeout`].
pub fn configured_test_pool_acquire_timeout() -> Duration {
    Duration::from_secs(env_u64("TEST_DB_ACQUIRE_TIMEOUT_SECS").unwrap_or(DEFAULT_TEST_DB_ACQUIRE_TIMEOUT_SECS))
}

/// See [`configured_test_pool_idle_timeout`].
pub fn configured_test_pool_idle_timeout() -> Duration {
    Duration::from_secs(env_u64("TEST_DB_IDLE_TIMEOUT_SECS").unwrap_or(DEFAULT_TEST_DB_IDLE_TIMEOUT_SECS))
}

/// See [`configured_test_pool_max_lifetime`].
pub fn configured_test_pool_max_lifetime() -> Duration {
    Duration::from_secs(env_u64("TEST_DB_MAX_LIFETIME_SECS").unwrap_or(DEFAULT_TEST_DB_MAX_LIFETIME_SECS))
}

/// See [`configured_test_db_init_timeout`].
pub fn configured_test_db_init_timeout() -> Duration {
    Duration::from_secs(env_u64("TEST_DB_INIT_TIMEOUT_SECS").unwrap_or(DEFAULT_TEST_DB_INIT_TIMEOUT_SECS))
}

/// See [`configured_shared_clone_concurrency`].
pub fn configured_shared_clone_concurrency() -> usize {
    env_usize("TEST_DB_SHARED_CLONE_CONCURRENCY")
        .filter(|value| *value > 0)
        .unwrap_or(DEFAULT_TEST_DB_SHARED_CLONE_CONCURRENCY)
}

/// See [`configured_test_db_template_schema`].
pub fn configured_test_db_template_schema() -> Option<String> {
    env_string("TEST_DB_TEMPLATE_SCHEMA")
}

/// See [`prepare_isolated_test_pool`].
pub async fn prepare_isolated_test_pool() -> Result<Arc<PgPool>, String> {
    let database_url = resolve_test_database_url().await?;
    let schema_name = next_test_schema_name();

    // Cleanup is owned by the shared janitor (`synapse_common::test_schema_guard`):
    // it watchers the pool's weak reference and drops the schema once the last
    // `Arc<PgPool>` is released, with an atexit join as the deterministic
    // backstop. No process-local sweep is needed — the old
    // `schedule_pending_schema_cleanup()` only ran on the *next* acquisition,
    // which never happens under nextest (one test per process), so every isolated
    // schema leaked until a manual cleanup. That structural defect produced the
    // 23,662 leftover schemas recorded in
    // docs/audit/P5_test_schema_accumulation_2026-09-12.md.
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

    // Register for DROP (not TRUNCATE-and-reuse) once the last Arc is released.
    //
    // This schema was built by replaying every migration into it, not by cloning
    // the template, so `truncate_and_reseed_schema` (which TRUNCATEs every
    // template table and re-seeds the baseline) is not a safe way to reset it.
    // Dropping is the correctly-bounded lifetime for a schema this expensive —
    // the leak was never the *cost*, it was that nothing ever removed them.
    //
    // The shared janitor guarantees the DROP fires on pool release AND at process
    // exit (the atexit backstop), which is exactly what the old process-local
    // `PENDING_SCHEMA_RETURNS` sweep could not do under nextest.
    register_schema_cleanup(&pool, &schema_name, SchemaCleanup::drop_only(&database_url, &schema_name));

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

    if test_schema_pool_reuse_enabled() {
        ensure_schema_pool_exit_drain(&database_url);
        if let Some(schema_name) = SCHEMA_POOL.lock().await.pop() {
            let pool = create_pool_for_schema(&database_url, &schema_name).await?;
            register_pending_schema_return(&pool, &schema_name, template.clone(), &database_url);
            return Ok(pool);
        }
    }

    // Clone template into a fresh per-test schema
    let _permit = SHARED_CLONE_SEMAPHORE.acquire().await.map_err(|_| "shared clone semaphore closed".to_string())?;
    let (pool, schema_name) = clone_schema_from_template(&database_url, &template).await?;
    if test_schema_pool_reuse_enabled() {
        register_pending_schema_return(&pool, &schema_name, template, &database_url);
    }
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

/// Lazily populate and return the cached template table names.
/// Used to build TRUNCATE lists without re-querying pg_tables (the string_agg
/// query was a slow-statement hotspot under parallel test load).
async fn get_template_table_names(database_url: &str, template_name: &str) -> Result<&'static Vec<String>, String> {
    TEMPLATE_TABLE_NAMES
        .get_or_try_init(|| async {
            let admin_pool = tokio::time::timeout(
                configured_test_pool_connect_timeout(),
                PgPoolOptions::new().max_connections(1).acquire_timeout(Duration::from_secs(5)).connect(database_url),
            )
            .await
            .map_err(|_| "timed out connecting to read template table names".to_string())?
            .map_err(|e| format!("failed to connect for template table names: {e}"))?;

            let names: Vec<String> = sqlx::query_scalar(
                "SELECT c.relname FROM pg_class c JOIN pg_namespace n ON n.oid = c.relnamespace WHERE n.nspname = $1 AND c.relkind = 'r' ORDER BY c.relname",
            )
            .bind(template_name)
            .fetch_all(&admin_pool)
            .await
            .map_err(|e| format!("failed to list template table names: {e}"))?;
            admin_pool.close().await;
            Ok(names)
        })
        .await
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

    // Cross-process advisory lock: prevents concurrent init_template_schema
    // calls from racing when nextest uses process-per-test. The lock key is
    // a fixed hash of "synapse_test_template_init" — same across processes.
    // Without this, two processes can simultaneously DROP/CREATE the template
    // and public schemas, causing "duplicate key value violates unique
    // constraint pg_namespace_nspname_index" errors.
    let template_lock_key: i64 = 8723419; // fixed key for template init
    let lock_start = std::time::Instant::now();
    loop {
        let locked: bool = sqlx::query_scalar("SELECT pg_try_advisory_lock($1)")
            .bind(template_lock_key)
            .fetch_one(&admin_pool)
            .await
            .map_err(|e| format!("failed to acquire template init advisory lock: {e}"))?;
        if locked {
            break;
        }
        if lock_start.elapsed() > std::time::Duration::from_secs(120) {
            return Err("timed out waiting for template init advisory lock (another process may be stuck)".to_string());
        }
        tokio::time::sleep(std::time::Duration::from_millis(500)).await;
    }

    // Drop if leftover from a previous crash
    let _ = sqlx::query(&format!("DROP SCHEMA IF EXISTS {template_name} CASCADE")).execute(&admin_pool).await;

    // Clean up leftover tables in `public` schema from historical test runs.
    // If public.{table} exists, migration's `CREATE TABLE IF NOT EXISTS {table}`
    // would skip creating it in the template schema — causing missing-table
    // errors in cloned schemas. We use DROP SCHEMA CASCADE (single operation)
    // instead of per-table DROP to avoid "out of shared memory" when hundreds of
    // leftover tables exist.
    //
    // ⚠️ THIS IS DESTRUCTIVE AND IT HAS ALREADY DESTROYED A REAL DATABASE.
    //
    // On 2026-09-12 this line emptied the *deployed* `synapse` database three
    // times (253 tables -> 3, `schema_migrations` 37 -> 0) because the suite was
    // pointed at the app's own database via `TEST_DATABASE_URL`. The template is
    // rebuilt whenever `template_schema_fingerprint()` changes (any migration
    // edit), so simply *running tests* was enough to wipe production data. It
    // also explains the "mysterious" 904-failure baselines: a vanished baseline
    // makes every DB-backed test fail with `relation ... does not exist`.
    //
    // "Safe in test envs" was an assumption, not a check. Make it a check:
    // refusing to proceed is always better than silently emptying a database
    // that might be someone's deployment.
    //
    // NB: 数据库名判据——名字含 `test`（如 CI 里统一使用的 `synapse_test`）
    // 的库视为测试库，本就应当被 DROP 重建，无论它是否携带已应用的迁移。
    // 这一判据同时解决了 §3.2：旧判据只看 `public.schema_migrations` 是否存在，
    // 导致守卫连自己错误信息里建议的 `synapse_test` 也会拒绝（预迁移后该表存在）。
    let is_throwaway = std::env::var("SYNAPSE_TEST_ALLOW_PUBLIC_SCHEMA_WIPE").is_ok_and(|v| v == "1");
    let is_test_db = sqlx::query_scalar::<_, String>("SELECT current_database()")
        .fetch_one(&admin_pool)
        .await
        .map(|db| db.to_ascii_lowercase().contains("test"))
        .unwrap_or(false);
    if !is_throwaway && !is_test_db {
        // A database is treated as *not* throwaway if it carries applied
        // migrations but is not explicitly opted in and is not named like a
        // test database. That is exactly the shape of a deployed database, and
        // the shape of the one we destroyed.
        let applied: i64 = sqlx::query_scalar(
            "SELECT count(*) FROM information_schema.tables WHERE table_schema = 'public' AND table_name = 'schema_migrations'",
        )
        .fetch_one(&admin_pool)
        .await
        .unwrap_or(0);
        if applied > 0 {
            return Err(String::from(
                "refusing to DROP SCHEMA public on a database that looks deployed (public.schema_migrations exists).\n                 This step is destructive and previously wiped a real deployment.\n                 Point the test suite at a throwaway database, e.g.\n                   TEST_DATABASE_URL=postgres://.../synapse_test cargo nt ...\n                 or set SYNAPSE_TEST_ALLOW_PUBLIC_SCHEMA_WIPE=1 if this database really is disposable."
            ));
        }
    }
    let _ = sqlx::query("DROP SCHEMA IF EXISTS public CASCADE").execute(&admin_pool).await;
    let _ = sqlx::query("CREATE SCHEMA IF NOT EXISTS public").execute(&admin_pool).await;

    // Heal `public` against cross-schema foreign keys left behind by the
    // search_path-shadowing incident (2026-09-12).
    //
    // A migration that adds a constraint with unqualified
    // `ALTER TABLE t ADD CONSTRAINT ... REFERENCES p(x)` binds `t` and `p`
    // through `search_path`. When a leftover `public` copy of the table already
    // existed, `CREATE TABLE IF NOT EXISTS` in the baseline silently skipped
    // creating it in the target schema, and the constraint ended up attached to
    // `public.t` while pointing at a *transient test schema*. Every suite that
    // reaches the table through `public` then failed with SQLSTATE 23503 for a
    // row that demonstrably existed.
    //
    // Rebuilding the template from the now schema-pinned migrations fixes this
    // for fresh clones, but `public` is only cleaned above when the template is
    // rebuilt — so a database that already carries the corruption would stay
    // broken until its migration fingerprint changed. Re-point any such
    // constraint at the same-named table inside `public` so the repair is
    // idempotent and does not depend on a rebuild being triggered.
    if let Err(error) = heal_public_cross_schema_foreign_keys(&admin_pool).await {
        tracing::warn!("failed to heal public-schema shadowed foreign keys: {error}");
    }

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

    // The freshly-built template is marked ready, so any template carrying an
    // older migration fingerprint can never be selected again. Drop them here
    // (rather than leaving them for a manual script) so a long-lived test
    // database cannot accumulate one 111-table schema per migration edit.
    if let Err(error) = prune_stale_template_schemas(&admin_pool, template_name).await {
        // Pruning is housekeeping: never fail template initialization over it.
        tracing::warn!(%error, "failed to prune superseded test template schemas");
    }

    // Release the cross-process advisory lock. On error paths, the lock is
    // released automatically when admin_pool (max_connections=1) is dropped
    // and its connection is closed.
    let _ = sqlx::query("SELECT pg_advisory_unlock($1)").bind(template_lock_key).execute(&admin_pool).await;

    Ok(())
}

/// Repair foreign keys in the `public` schema whose parent table lives in a
/// different schema — the signature of the 2026-09-12 search_path-shadowing
/// incident (see the call site in `init_template_schema`).
///
/// Each offending constraint is rebuilt as `public.<child> -> public.<parent>`,
/// where `<parent>` keeps the original parent *table name*. Constraints whose
/// parent table has no same-named counterpart in `public` are left alone and
/// reported, because silently dropping them would remove enforcement rather
/// than fix it.
///
/// Idempotent: after a successful pass there are no cross-schema FKs left in
/// `public`, so the scan matches nothing on the next call.
async fn heal_public_cross_schema_foreign_keys(admin_pool: &PgPool) -> Result<(), String> {
    heal_cross_schema_foreign_keys_in(admin_pool, "public").await
}

/// Implementation of [`heal_public_cross_schema_foreign_keys`], parameterised by
/// the schema to scan so it can be exercised against a throwaway schema in
/// tests without mutating the shared `public` schema.
///
/// Public so `tests/unit/` can build a corrupted schema, point this at it, and
/// assert the repair — the shared `public` schema must never be used as the test
/// fixture, because nextest runs tests in parallel processes against one
/// database.
pub async fn heal_cross_schema_foreign_keys_in(admin_pool: &PgPool, scan_schema: &str) -> Result<(), String> {
    #[derive(sqlx::FromRow)]
    struct ShadowedFk {
        conname: String,
        child: String,
        parent_name: String,
        parent_schema: String,
        delete_action: String,
        is_deferrable: bool,
        is_deferred: bool,
        columns: Vec<String>,
        parent_columns: Vec<String>,
    }

    let rows: Vec<ShadowedFk> = sqlx::query_as(
        r"
        SELECT
            c.conname,
            c.conrelid::regclass::text AS child,
            parent.relname AS parent_name,
            parent_ns.nspname AS parent_schema,
            c.confdeltype::text AS delete_action,
            c.condeferrable AS is_deferrable,
            c.condeferred AS is_deferred,
            ARRAY(
                SELECT a.attname FROM unnest(c.conkey) WITH ORDINALITY AS k(attnum, ord)
                JOIN pg_attribute a ON a.attrelid = c.conrelid AND a.attnum = k.attnum
                ORDER BY k.ord
            ) AS columns,
            ARRAY(
                SELECT a.attname FROM unnest(c.confkey) WITH ORDINALITY AS k(attnum, ord)
                JOIN pg_attribute a ON a.attrelid = c.confrelid AND a.attnum = k.attnum
                ORDER BY k.ord
            ) AS parent_columns
        FROM pg_constraint c
        JOIN pg_class child_cls ON child_cls.oid = c.conrelid
        JOIN pg_namespace child_ns ON child_ns.oid = child_cls.relnamespace
        JOIN pg_class parent ON parent.oid = c.confrelid
        JOIN pg_namespace parent_ns ON parent_ns.oid = parent.relnamespace
        WHERE c.contype = 'f'
          AND child_ns.nspname = $1
          AND parent_ns.nspname <> $1
        ORDER BY c.conname
        ",
    )
    .bind(scan_schema)
    .fetch_all(admin_pool)
    .await
    .map_err(|error| format!("failed to query cross-schema foreign keys in {scan_schema}: {error}"))?;

    for row in rows {
        // Only tables that still exist inside the scan schema can be re-pointed;
        // the referenced parent must also exist there.
        let parent_exists: Option<String> = sqlx::query_scalar("SELECT to_regclass($1)::text")
            .bind(format!("{scan_schema}.{}", row.parent_name))
            .fetch_one(admin_pool)
            .await
            .map_err(|e| format!("to_regclass failed: {e}"))?;
        if parent_exists.is_none() {
            tracing::warn!(
                constraint = %row.conname,
                child = %row.child,
                dangling_parent = format!("{}.{}", row.parent_schema, row.parent_name),
                "foreign key points at another schema and {scan_schema} has no same-named parent; leaving it for manual review"
            );
            continue;
        }

        let on_delete = match row.delete_action.as_str() {
            "c" => " ON DELETE CASCADE",
            "n" => " ON DELETE SET NULL",
            "d" => " ON DELETE SET DEFAULT",
            "r" => " ON DELETE RESTRICT",
            _ => "",
        };
        let deferrable = if row.is_deferrable {
            if row.is_deferred {
                " DEFERRABLE INITIALLY DEFERRED"
            } else {
                " DEFERRABLE INITIALLY IMMEDIATE"
            }
        } else {
            ""
        };

        let mut tx = admin_pool.begin().await.map_err(|error| format!("failed to open heal transaction: {error}"))?;
        sqlx::query(&format!("ALTER TABLE {} DROP CONSTRAINT {}", row.child, quote_ident(&row.conname)))
            .execute(&mut *tx)
            .await
            .map_err(|error| format!("failed to drop shadowed FK {}: {error}", row.conname))?;
        sqlx::query(&format!(
            "ALTER TABLE {} ADD CONSTRAINT {} FOREIGN KEY ({}) REFERENCES {}.{} ({}){}{}",
            row.child,
            quote_ident(&row.conname),
            row.columns.iter().map(|c| quote_ident(c)).collect::<Vec<_>>().join(", "),
            quote_ident(scan_schema),
            quote_ident(&row.parent_name),
            row.parent_columns.iter().map(|c| quote_ident(c)).collect::<Vec<_>>().join(", "),
            on_delete,
            deferrable
        ))
        .execute(&mut *tx)
        .await
        .map_err(|error| format!("failed to rebuild shadowed FK {}: {error}", row.conname))?;
        tx.commit().await.map_err(|error| format!("failed to commit heal of {}: {error}", row.conname))?;

        tracing::warn!(
            constraint = %row.conname,
            child = %row.child,
            was_parent = format!("{}.{}", row.parent_schema, row.parent_name),
            "repaired search_path-shadowed foreign key in {scan_schema} schema"
        );
    }

    Ok(())
}

/// Quote a SQL identifier for interpolation into DDL built with `format!`.
/// Mirrors what `format('%I')` does server-side.
fn quote_ident(ident: &str) -> String {
    format!("\"{}\"", ident.replace('"', "\"\""))
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

/// Directory holding the `*_ready_<schema>` marker files that record which
/// template schemas are usable. Shared by the marker writer, the readiness
/// check, and [`prune_stale_template_schemas`] (which deletes the markers of
/// templates it drops).
fn template_marker_dir() -> std::path::PathBuf {
    let dir = std::env::var("CARGO_TARGET_TMPDIR")
        .ok()
        .map_or_else(
            || std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("target").join("tmp"),
            std::path::PathBuf::from,
        )
        .join("synapse_test_templates");
    let _ = std::fs::create_dir_all(&dir);
    dir
}

fn template_ready_marker_path(schema_name: &str) -> std::path::PathBuf {
    template_marker_dir().join(format!("{TEST_TEMPLATE_READY_MARKER_PREFIX}_{schema_name}"))
}

/// Drop template schemas superseded by `keep`.
///
/// `default_template_schema_name()` embeds a fingerprint of every migration file
/// (name + size + mtime), so **every migration edit mints a new template name**.
/// Nothing used to delete the previous one, so a long-lived test database
/// accumulated one full 111-table schema per migration change (40 were present
/// on 2026-09-12). They are pure waste: `template_schema_is_ready` only ever
/// consults the current fingerprint, so an old template can never be selected
/// again.
///
/// Safety rules, in order:
/// 1. `keep` is never a candidate.
/// 2. Only names matching `test_template_v<rev>_<16 hex>` are candidates — a
///    configured `TEST_DB_TEMPLATE_SCHEMA` (arbitrary user name, possibly a
///    production database's `public`) can therefore never match.
/// 3. A candidate is dropped only if `keep` actually exists, so a failed build
///    can never leave the database with no template at all.
///
/// Returns the names dropped. Idempotent.
/// Public so `tests/unit/` can exercise the drop/preserve decision against
/// throwaway schemas instead of the real template set.
pub async fn prune_stale_template_schemas(admin_pool: &PgPool, keep: &str) -> Result<Vec<String>, String> {
    let keep_exists: bool = sqlx::query_scalar("SELECT EXISTS (SELECT 1 FROM pg_namespace WHERE nspname = $1)")
        .bind(keep)
        .fetch_one(admin_pool)
        .await
        .map_err(|error| format!("failed to check template {keep}: {error}"))?;
    if !keep_exists {
        return Err(format!("refusing to prune templates: replacement template {keep} does not exist"));
    }

    // `test_template_v2_<16 hex chars>` — anchored, so no other family matches.
    let stale: Vec<String> = sqlx::query_scalar(
        "SELECT nspname FROM pg_namespace
         WHERE nspname ~ '^test_template_v[0-9]+_[0-9a-f]{16}$'
           AND nspname <> $1
         ORDER BY nspname",
    )
    .bind(keep)
    .fetch_all(admin_pool)
    .await
    .map_err(|error| format!("failed to list stale templates: {error}"))?;

    let mut dropped = Vec::new();
    for schema in stale {
        match sqlx::query(&format!("DROP SCHEMA IF EXISTS {} CASCADE", quote_ident(&schema))).execute(admin_pool).await
        {
            Ok(_) => {
                // Remove the run-speed marker too, or a stale marker file outlives
                // its schema and the readiness check has to hit the database to
                // notice (`template_schema_is_ready` already guards this, but the
                // files should not accumulate either).
                let _ = std::fs::remove_file(template_ready_marker_path(&schema));
                dropped.push(schema);
            }
            Err(error) => {
                tracing::warn!(schema = %schema, %error, "failed to drop stale test template schema");
            }
        }
    }

    if !dropped.is_empty() {
        tracing::info!(count = dropped.len(), keep = %keep, "pruned superseded test template schemas");
    }
    Ok(dropped)
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
    let timestamp = current_timestamp_millis().to_string();
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

    // Clone through the shared implementation
    // (`synapse_common::test_isolation::clone_schema_from_template`).
    //
    // This module used to carry its own ~170-line clone (table LIKE + index
    // rename + seed copy + sequence copy + view recreation). It was the third
    // implementation of the same job and it silently diverged from the shared
    // one: it copied *no* foreign keys, no triggers and no functions, and it
    // swallowed every per-index and per-view error with `EXCEPTION WHEN OTHERS
    // THEN NULL`. The shared clone replays those objects and then verifies the
    // clone's object inventory against the template's, so an incomplete clone is
    // an immediate error instead of a later order-dependent test failure.
    //
    // The shared helper does not create the schema, so we do that (and pin the
    // session `search_path`) first, exactly like the storage and services
    // fixtures.
    //
    // `raw_sql` (simple protocol, no bind params) rather than three separate
    // formatted query calls: identifiers cannot be bound, and the
    // SQLx dynamic-ratio gate counts every non-macro query call as dynamic SQL.
    // One DDL block keeps this convergence from moving that ratchet at all.
    sqlx::raw_sql(&format!(
        r#"
        DO $do$
        BEGIN
            EXECUTE format('DROP SCHEMA IF EXISTS %I CASCADE', '{schema_name}');
            EXECUTE format('CREATE SCHEMA %I', '{schema_name}');
        END
        $do$;
        "#
    ))
    .execute(&admin_pool)
    .await
    .map_err(|error| format!("failed to create the clone schema {schema_name}: {error}"))?;
    sqlx::raw_sql(&format!(r#"SET search_path TO "{schema_name}", public"#))
        .execute(&admin_pool)
        .await
        .map_err(|error| format!("failed to set search_path for {schema_name}: {error}"))?;

    // `Only(SEED_REFERENCE_TABLES)` is this fixture's historical behaviour
    // written down: phase 1b copies the baseline's seeded reference rows and
    // nothing else. Behaviour is identical to `Everything` today (measured: the
    // template's only non-empty tables are exactly these three), and the
    // `the_seed_allowlist_matches_what_the_baseline_seeds` guard keeps that true.
    synapse_common::test_isolation::clone_schema_from_template(
        &admin_pool,
        &schema_name,
        template_name,
        synapse_common::test_isolation::SeedSource::Only(synapse_common::test_isolation::SEED_REFERENCE_TABLES),
    )
    .await?;

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
    // the shared clone's `LIKE ... INCLUDING ALL` table creation carries them into
    // the clone.
    Ok((pool, schema_name))
}

// ============================================================================
// Schema pool (P0 optimization): LeasedSchema + acquire_pooled_schema
// ============================================================================

struct LeasedSchemaInner {
    /// Shared poison flag: if a destructive test sets this, the
    /// janitor DROPs the schema on release instead of reusing it.
    /// Only field accessed outside the janitor (via `poison()`).
    poisoned: Arc<AtomicBool>,
}

/// A schema leased from the pool. On release (drop of the last
/// `Arc<PgPool>`) the schema is either TRUNCATEd and returned to
/// `SCHEMA_POOL` for reuse by subsequent tests, or DROPped if it has
/// been poisoned.
///
/// Cleanup is delegated to the shared janitor in `synapse_common::test_schema_guard`
/// (`register_schema_cleanup`): the janitor polls the pool's `Weak`
/// reference and runs the cleanup as soon as the last `Arc<PgPool>`
/// is released, with an `atexit` join as the deterministic backstop.
/// This works identically under `cargo test` (many tests per process,
/// reuse pays off via TRUNCATE+return) and nextest (one test per
/// process, `on_release` is a plain DROP). The old `CLEANUP_RUNTIME.spawn`
/// in `Drop` was fire-and-forget and got cancelled at process exit,
/// leaking 100% of leased schemas under nextest.
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
            inner.poisoned.store(true, Ordering::SeqCst);
        }
    }
}

// No custom `Drop`: cleanup is driven by the janitor watching the pool's
// weak reference, registered at acquisition time (see `acquire_pooled_schema`).
// The `inner` field is kept only so `poison()` can reach the shared flag.

/// Shared background cleanup body for a leased/returned schema: connect an admin
/// pool, TRUNCATE + re-seed (returning the schema name to SCHEMA_POOL) or DROP it
/// if poisoned/corrupted. Used by both `LeasedSchema::drop` (TestContext path)
/// and the deferred `prepare_shared_test_pool` return path.
async fn cleanup_schema(database_url: String, schema_name: String, template_name: String, poisoned: bool) {
    // Serialize cleanup (see CLEANUP_SEMAPHORE docs) to bound peak lock usage.
    // The semaphore is a process-lifetime static and never closed.
    let Ok(_cleanup_permit) = CLEANUP_SEMAPHORE.acquire().await else {
        eprintln!("schema pool: cleanup semaphore closed; skipping cleanup of {schema_name}");
        return;
    };

    let admin_pool = tokio::time::timeout(
        Duration::from_secs(10),
        PgPoolOptions::new().max_connections(1).acquire_timeout(Duration::from_secs(5)).connect(&database_url),
    )
    .await;

    let admin_pool = match admin_pool {
        Ok(Ok(pool)) => pool,
        _ => {
            eprintln!("schema pool: failed to connect admin pool for cleanup of {schema_name}; schema orphaned");
            return;
        }
    };

    if poisoned {
        let _ = drop_schema(&admin_pool, &schema_name).await;
        admin_pool.close().await;
        return;
    }

    match truncate_and_reseed_schema(&admin_pool, &database_url, &schema_name, &template_name).await {
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
}

/// Register a schema returned by `prepare_shared_test_pool` for deferred return.
/// The janitor watches the pool's `Weak` and TRUNCATES + returns the schema name
/// to `SCHEMA_POOL` once the last `Arc<PgPool>` is released. Under nextest the
/// `on_release` path is a plain DROP because reuse never pays off, and the
/// shared-pool exit drain will clean any still-parking names.
fn register_pending_schema_return(pool: &Arc<PgPool>, schema_name: &str, template_name: String, database_url: &str) {
    ensure_schema_pool_exit_drain(database_url);
    let db = database_url.to_string();
    let sn = schema_name.to_string();
    // `template_name` is only needed by the post-`register_schema_cleanup` call
    // below (the closure moves `db`/`sn` but not it), so cloning it here was dead.
    let tn = template_name;
    let on_release: CleanupFn = Box::new(move || {
        // Under nextest, dropping immediately beats the wasted TRUNCATE+return.
        if running_under_nextest() {
            drop_schema_blocking(&db, &sn);
            return;
        }
        run_cleanup_blocking(async move {
            cleanup_schema(db, sn, tn, false).await;
        });
    });
    register_schema_cleanup(
        pool,
        schema_name,
        SchemaCleanup::release_or_exit_drop(database_url, schema_name, on_release),
    );
}

/// Install a one-time process-exit drain for `SCHEMA_POOL`.
///
/// Schemas parked back in `SCHEMA_POOL` are *names only* — they have no live
/// `PgPool`, so the janitor's weak-reference watch cannot see them. Without an
/// exit hook, a `cargo test` run ending with reusable schemas parked in the pool
/// would leak them. The janitor runs this callback after every pending cleanup
/// at process exit; under nextest the pool is never refilled (every
/// `on_release` is a plain DROP), so this is normally a no-op there.
fn ensure_schema_pool_exit_drain(database_url: &str) {
    SCHEMA_POOL_EXIT_DRAIN_ONCE.call_once(|| {
        let database_url = database_url.to_string();
        register_exit_callback(Box::new(move || {
            let names: Vec<String> = match SCHEMA_POOL.try_lock() {
                Ok(mut guard) => std::mem::take(&mut *guard),
                // Another thread is mid-TRUNCATE; the schema will be returned
                // and dropped by that path, or by the next run's cleanup.
                Err(_) => Vec::new(),
            };
            if names.is_empty() {
                return;
            }
            eprintln!("schema pool: dropping {} parked reusable schema(s) at exit", names.len());
            for name in &names {
                drop_schema_blocking(&database_url, name);
            }
        }));
    });
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
    // Schemas in the pool were already validated by truncate_and_reseed_schema
    // (which checks table count before TRUNCATE-ing). TRUNCATE doesn't drop
    // tables, so the count is still correct. Skip the redundant COUNT query here
    // — under parallel test load, pg_catalog queries take 1.6-8s due to lock
    // contention, and this check was the #1 source of slow-statement warnings.
    #[allow(clippy::never_loop)]
    while let Some(schema_name) = SCHEMA_POOL.lock().await.pop() {
        let pool = create_pool_for_schema(&database_url, &schema_name).await?;
        let poisoned = Arc::new(AtomicBool::new(false));
        // The closure holds its own clone of the flag so the janitor can read
        // `poison()`; the struct field keeps another clone for the test-side API.
        let closure_poisoned = poisoned.clone();
        let schema_name_clone = schema_name.clone();
        let template_name_clone = template_name.clone();
        let db_clone = database_url.clone();
        let on_release: CleanupFn = Box::new(move || {
            if running_under_nextest() || closure_poisoned.load(Ordering::SeqCst) {
                drop_schema_blocking(&db_clone, &schema_name_clone);
                return;
            }
            run_cleanup_blocking(async move {
                cleanup_schema(db_clone, schema_name_clone, template_name_clone, false).await;
            });
        });
        register_schema_cleanup(
            &pool,
            &schema_name,
            SchemaCleanup::release_or_exit_drop(&database_url, &schema_name, on_release),
        );
        ensure_schema_pool_exit_drain(&database_url);
        return Ok(LeasedSchema { pool, inner: Some(LeasedSchemaInner { poisoned }) });
    }

    // Slow path: clone a new schema from the template
    let _permit = SHARED_CLONE_SEMAPHORE.acquire().await.map_err(|_| "shared clone semaphore closed".to_string())?;
    let (pool, schema_name) = clone_schema_from_template(&database_url, &template_name).await?;
    drop(_permit);

    let poisoned = Arc::new(AtomicBool::new(false));
    // The closure holds its own clone of the flag so the janitor can read
    // `poison()`; the struct field keeps another clone for the test-side API.
    let closure_poisoned = poisoned.clone();
    let schema_name_clone = schema_name.clone();
    let template_name_clone = template_name.clone();
    let db_clone = database_url.clone();
    let on_release: CleanupFn = Box::new(move || {
        if running_under_nextest() || closure_poisoned.load(Ordering::SeqCst) {
            drop_schema_blocking(&db_clone, &schema_name_clone);
            return;
        }
        run_cleanup_blocking(async move {
            cleanup_schema(db_clone, schema_name_clone, template_name_clone, false).await;
        });
    });
    register_schema_cleanup(
        &pool,
        &schema_name,
        SchemaCleanup::release_or_exit_drop(&database_url, &schema_name, on_release),
    );
    ensure_schema_pool_exit_drain(&database_url);

    Ok(LeasedSchema { pool, inner: Some(LeasedSchemaInner { poisoned }) })
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
/// Corruption detection: the TRUNCATE statement is built from the template's
/// cached table names. If a destructive test dropped a table, the TRUNCATE
/// fails with "relation does not exist" — the error is caught by the caller
/// and the schema is DROPped instead of pooled. This replaces the previous
/// explicit COUNT(*) safety check, eliminating all slow pg_catalog queries
/// from the cleanup path.
///
/// Uses cached template table names (populated once per process) to avoid
/// querying pg_tables/pg_class on every cleanup cycle.
async fn truncate_and_reseed_schema(
    admin_pool: &PgPool,
    database_url: &str,
    schema_name: &str,
    template_name: &str,
) -> Result<(), String> {
    // Build TRUNCATE list from cached template table names.
    // Clones use CREATE TABLE LIKE, so test schema table names == template names.
    // Building in Rust avoids the slow `string_agg` query that scanned pg_tables
    // under heavy parallel catalog contention (1.6-2.1s per call).
    let template_table_names = get_template_table_names(database_url, template_name).await?;
    if template_table_names.is_empty() {
        return Err(format!("template {template_name} has no tables — cache may be stale"));
    }
    let trunc_list: String =
        template_table_names.iter().map(|name| format!("{schema_name}.{name}")).collect::<Vec<_>>().join(", ");
    let sql = format!("TRUNCATE TABLE {trunc_list} RESTART IDENTITY CASCADE");
    sqlx::raw_sql(&sql).execute(admin_pool).await.map_err(|e| format!("TRUNCATE failed for {schema_name}: {e}"))?;

    // Re-seed reference/config tables from the template — the same set the clone
    // copies, taken from the shared constant so the two paths cannot drift.
    // These are small and fast to copy.
    // Use cached table names to check existence instead of per-table EXISTS queries.
    for table in synapse_common::test_isolation::SEED_REFERENCE_TABLES {
        if !template_table_names.iter().any(|name| name == table) {
            eprintln!("schema pool: {table} not found in template {template_name}, skipping reseed");
            continue;
        }

        let sql =
            format!("INSERT INTO {schema_name}.{table} SELECT * FROM {template_name}.{table} ON CONFLICT DO NOTHING");
        // Use raw_sql since table/schema names are validated identifiers
        if let Err(e) = sqlx::raw_sql(&sql).execute(admin_pool).await {
            // Seed copy failure IS fatal — tests that depend on config rows
            // (e.g. media upload needs server_media_quota id=1) will fail
            // silently if we pool a schema with missing seed data.
            return Err(format!(
                "seed copy for {table} failed in {schema_name}: {e} — schema will be dropped, not pooled"
            ));
        }
    }

    // `TRUNCATE ... RESTART IDENTITY` above put every sequence back at 1, and the
    // rows just copied back carry explicit ids starting at 1, so without this the
    // next default-id insert into a re-seeded table returns 1 and dies on a
    // duplicate key (measured: `server_media_quota_pkey`, `Key (id)=(1)`).
    synapse_common::test_isolation::advance_schema_sequences(admin_pool, schema_name).await?;

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

/// See [`prepare_empty_isolated_test_pool`].
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
    register_schema_cleanup(&pool, &schema_name, SchemaCleanup::drop_only(&database_url, &schema_name));
    Ok(pool)
}

/// See [`resolve_test_database_url`].
pub async fn resolve_test_database_url() -> Result<String, String> {
    // Fast path: the URL was already resolved earlier in this process. This is
    // the crux of the P0-1 gate-drift fix — every DB-backed test used to build
    // a probe pool (and immediately drop it) to re-confirm which candidate URL
    // was reachable. With thousands of tests under `--test-threads=N` this once-
    // per-test churn hammered the server's connection limit and Windows/churn
    // collisions surfaced as spurious `PoolTimedOut` ("Operation timed out").
    // Resolving once per process and reusing the URL eliminates that churn.
    if let Some(cached) = RESOLVED_TEST_DB_URL.lock().unwrap_or_else(|poisoned| poisoned.into_inner()).as_ref() {
        return Ok(cached.clone());
    }

    let mut errors = Vec::new();
    let connect_timeout = configured_test_pool_connect_timeout();

    for database_url in candidate_database_urls() {
        // Probe with an explicit acquire timeout (30s, see
        // `configured_test_pool_connect_timeout`) so a slow-to-accept server
        // does not overrun the test harness. The 5s default was too tight and
        // contributed to the same P0-1 flap.
        let connect_future =
            PgPoolOptions::new().max_connections(1).acquire_timeout(Duration::from_secs(30)).connect(&database_url);

        match tokio::time::timeout(connect_timeout, connect_future).await {
            Err(_) => errors.push(format!("{database_url} -> connect timed out after {connect_timeout:?}")),
            Ok(Ok(pool)) => {
                drop(pool);
                let url = database_url.clone();
                *RESOLVED_TEST_DB_URL.lock().unwrap_or_else(|poisoned| poisoned.into_inner()) = Some(url.clone());
                return Ok(url);
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
        ALTER TABLE events ADD COLUMN IF NOT EXISTS origin TEXT DEFAULT 'self';
        ALTER TABLE events ADD COLUMN IF NOT EXISTS user_id TEXT;
        ALTER TABLE events ADD COLUMN IF NOT EXISTS stream_ordering BIGINT;
        "
}

#[cfg(test)]
mod pooled_schema_reseed_tests {
    /// A schema the pool recycles must be able to serve a default-id insert into
    /// a table the pool re-seeds.
    ///
    /// The cycle is `TRUNCATE ... RESTART IDENTITY CASCADE` (every sequence back
    /// to 1) followed by copying the reference rows back from the template — rows
    /// whose explicit ids start at 1. That combination used to leave
    /// `server_media_quota_id_seq` at 1 while a row already held `id = 1`, so the
    /// next default-id insert died on
    /// `duplicate key value violates unique constraint "server_media_quota_pkey"`.
    ///
    /// The cycle is driven **directly** rather than by acquiring twice: a first
    /// acquisition usually *clones* (which advances the sequences as part of the
    /// clone), so it would pass even with the repair removed — measured, and the
    /// reason this test calls the reset path explicitly.
    #[tokio::test]
    async fn pooled_schema_can_insert_into_a_reseeded_table() {
        if std::env::var("TEST_DATABASE_URL").is_err() {
            eprintln!(
                "SKIPPING pooled_schema_can_insert_into_a_reseeded_table: TEST_DATABASE_URL is unset, so this \
                 proves nothing."
            );
            return;
        }
        let leased = crate::acquire_pooled_schema().await.expect("pooled schema");
        let pool: &sqlx::PgPool = &leased.pool;
        let schema_name: String =
            sqlx::query_scalar("SELECT current_schema()").fetch_one(pool).await.expect("current_schema");

        // Drive the pool's reset path explicitly. This is the code under test.
        let database_url = crate::resolve_test_database_url().await.expect("database url");
        let template_name = crate::get_template_schema_name(&database_url).await.expect("template");
        let admin = sqlx::PgPool::connect(&database_url).await.expect("admin pool");
        crate::truncate_and_reseed_schema(&admin, &database_url, &schema_name, &template_name)
            .await
            .expect("reset the pooled schema");

        // One statement: the precondition (the re-seed copied the row with
        // id = 1) and the regression (a default-id insert must not collide with
        // it, so it must land above it).
        let (seeded_count, lowest_seeded, new_id): (i64, Option<i64>, i64) = sqlx::query_as(
            "WITH seeded AS (SELECT count(*) AS n, min(id) AS lo FROM server_media_quota), \
                  ins AS (INSERT INTO server_media_quota \
                          (max_storage_bytes, max_file_size_bytes, max_files_count, \
                           current_storage_bytes, current_files_count, \
                           alert_threshold_percent, updated_ts) \
                          VALUES (1, 1, 1, 0, 0, 80, 0) RETURNING id) \
             SELECT seeded.n, seeded.lo, ins.id FROM seeded, ins",
        )
        .fetch_one(pool)
        .await
        .expect(
            "a default-id insert into a re-seeded table must not collide — the reset must advance the \
             sequences past the rows it copied back",
        );
        assert!(
            seeded_count > 0 && lowest_seeded == Some(1),
            "precondition: the re-seed must have copied a row with id = 1, got count={seeded_count} \
             lowest={lowest_seeded:?}"
        );
        assert!(new_id > 1, "the re-seeded row already owns id = 1, so the insert must land above it");

        admin.close().await;
    }
}
