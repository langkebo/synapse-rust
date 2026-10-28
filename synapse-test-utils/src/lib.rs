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
use std::fs;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::LazyLock;
use std::sync::{Arc, Mutex, Once};
use std::time::Duration;
use synapse_common::current_timestamp_millis;
use synapse_common::test_schema_guard::{
    drop_schema_blocking, drop_schema_cleanup, drop_schema_if_unleased_blocking, register_exit_callback,
    register_schema_cleanup, release_schema_lease, run_cleanup_blocking_outcome_lazy, running_under_nextest,
    schema_lease_key, try_acquire_schema_lease, CleanupFn, CleanupOutcome, SchemaCleanup,
};
use synapse_services::database_initializer::{DatabaseInitMode, DatabaseInitService};
use tokio::sync::OnceCell;
use tokio::sync::{Mutex as TokioMutex, RwLock as TokioRwLock, Semaphore};

mod memory_guard;
use memory_guard::is_low_memory_environment;
mod db_isolation;

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
// Design notes: the pool stores names, not `PgPool`s (a pool dies with its
// runtime); cleanup runs on a process-lifetime runtime because `Drop::drop` is
// sync; corrupted schemas (DROP TABLE / ALTER) are DROPped instead of pooled.

static SCHEMA_POOL: TokioMutex<Vec<ParkedSchema>> = TokioMutex::const_new(Vec::new());

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
// Cleanup is owned by `synapse_common::test_schema_guard`: the janitor watches
// each pool's weak reference (atexit join as backstop), identically under
// `cargo test` (many tests per process) and nextest (one per process).
//
// The P0 TRUNCATE-and-reuse optimization is preserved via
// `pooled_schema_cleanup`: `on_release` TRUNCATEs the schema and parks its name
// for the next test. *Where* the name is parked depends on the runner:
//   * `cargo test` (many tests per process) parks it in the process-local
//     `SCHEMA_POOL` below;
//   * nextest (one test per process) parks it in the **database** — see
//     "Cross-process schema pool" below — because a name parked in a dying
//     process is worth nothing. Both cleanup paths park there, including
//     `on_exit`: the exit drain runs right after the single test, so an
//     unconditional DROP at exit would throw away the schema this design exists
//     to reuse.
// Under `cargo test` the `on_exit` path stays a plain DROP — returning a name to
// a pool that is about to die is pointless.
//
// Schemas parked in `SCHEMA_POOL` as *names* (no live pool) are reclaimed in two
// independent ways, because they cover different situations:
//   1. **Idle TTL reclaimer** (2026-09-22): a one-time background thread drops
//      names parked longer than [`SCHEMA_POOL_IDLE_TTL`]. A long-lived process
//      (`cargo test` with many tests) therefore cannot accumulate parked names
//      without bound — this is the part that used to be missing.
//   2. **Process-exit drain**: the janitor's exit callback drops whatever is
//      still parked when the process ends. The TTL thread cannot help there (it
//      only runs while the process lives), so the exit drain stays.
static SCHEMA_POOL_EXIT_DRAIN_ONCE: Once = Once::new();
static SCHEMA_POOL_IDLE_RECLAIMER_ONCE: Once = Once::new();

/// How long a parked reusable schema name may sit idle before the reclaimer
/// drops it. Longer than any plausible gap between two tests in one process
/// (the pool exists to bridge exactly that gap), short enough that a long
/// `cargo test` run cannot accumulate names.
const SCHEMA_POOL_IDLE_TTL: Duration = Duration::from_secs(60);
/// How often the idle reclaimer sweeps the pool.
const SCHEMA_POOL_SWEEP_INTERVAL: Duration = Duration::from_secs(15);

/// One parked reusable schema name plus when it was parked.
struct ParkedSchema {
    name: String,
    parked_at: std::time::Instant,
}

/// Removes and returns the names parked for at least `ttl` (pure, testable).
fn take_expired(pool: &mut Vec<ParkedSchema>, now: std::time::Instant, ttl: Duration) -> Vec<String> {
    let mut kept = Vec::with_capacity(pool.len());
    let mut expired = Vec::new();
    for parked in pool.drain(..) {
        if now.saturating_duration_since(parked.parked_at) >= ttl {
            expired.push(parked.name);
        } else {
            kept.push(parked);
        }
    }
    *pool = kept;
    expired
}

// Serialize background schema cleanup (TRUNCATE + re-seed) to a single concurrent
// task. Each TRUNCATE acquires ACCESS EXCLUSIVE locks on ~111 tables; running
// several concurrently with foreground clone DO-blocks (which take ~1000 locks
// each) exhausts PostgreSQL's shared lock table (`max_locks_per_transaction`)
// and triggers "out of shared memory" at modest test parallelism. One-at-a-time
// cleanup keeps peak lock pressure bounded without blocking test execution.
//
// Held through an `Arc` and taken with `acquire_owned`: a borrowed
// `SemaphorePermit<'a>` keeps a `&Semaphore` in the cleanup future's state, and
// that makes the future fail the `Send` check ("not general enough") where the
// janitor's blocking bridge requires it.
static CLEANUP_SEMAPHORE: LazyLock<Arc<Semaphore>> = LazyLock::new(|| Arc::new(Semaphore::new(1)));

/// Env kill-switch for the schema-pool reuse fast path in
/// `prepare_shared_test_pool`. Set `TEST_SCHEMA_POOL_REUSE=0` to fall back to
/// the previous clone-only behaviour (used to bisect regressions).
fn test_schema_pool_reuse_enabled() -> bool {
    std::env::var("TEST_SCHEMA_POOL_REUSE").map(|v| v != "0" && !v.eq_ignore_ascii_case("false")).unwrap_or(true)
}

// ============================================================================
// Cross-process schema pool (nextest)
// ============================================================================
//
// `SCHEMA_POOL` above is process-local, which makes it useless under
// cargo-nextest: there is exactly one test case per process, so a schema is
// TRUNCATEd and parked by a process that is already exiting, and the next test
// runs in a different process that cannot see the name. Every nextest test
// therefore paid the full template clone (measured 2026-10-23: mean 5.21 s per
// case, 87 % of the whole gate's wall time).
//
// This pool moves the parked *names* into a dedicated bookkeeping schema
// (`_synapse_test_pool.parked_schemas`), which every test process shares. The
// schema is deliberately outside `public`: template rebuilds run
// `DROP SCHEMA public CASCADE`, which would otherwise take the pool with it.
//   * park  — after TRUNCATE + re-seed, `INSERT ... ON CONFLICT DO UPDATE`;
//   * claim — `DELETE ... WHERE ctid = (SELECT ctid ... FOR UPDATE SKIP LOCKED)
//             RETURNING schema_name`, atomic across processes, so two concurrent
//             tests can never be handed the same schema;
//   * reap  — rows older than [`DB_SCHEMA_POOL_TTL`] are deleted and their
//             schema DROPped, so an interrupted run cannot accumulate schemas.
//
// The table is created lazily under an advisory-xact lock instead of by a
// migration: migrations are the *production* schema, and a test-only table has
// no business there. The template fingerprint hashes the migrations directory,
// so creating this table cannot invalidate the cached template.
//
// Rows are namespaced by `template_name`, which embeds the migration revision
// and fingerprint, so two worktrees never hand each other schemas built from a
// different baseline.

/// How long a schema may sit in the cross-process pool before it is dropped.
/// Stays well below `test_schema_orphans::ORPHAN_SCHEMA_MIN_AGE` (600 s) so the
/// orphan guard never races the pool over the same schema.
const DB_SCHEMA_POOL_TTL: Duration = Duration::from_secs(120);
/// Advisory-lock key guarding lazy creation of [`DB_SCHEMA_POOL_TABLE`].
const DB_SCHEMA_POOL_ADVISORY_KEY: i64 = 9_732_001_001;
/// Name of the shared pool table. Fully qualified at every use site; admin
/// connections do not set `search_path`. Defined in `synapse-common` because
/// the orphan guard has to recognise parked schemas too.
const DB_SCHEMA_POOL_TABLE: &str = synapse_common::test_schema_orphans::PARKED_SCHEMA_TABLE;
/// Schema holding [`DB_SCHEMA_POOL_TABLE`]. Not `public`: template rebuilds run
/// `DROP SCHEMA public CASCADE`, which would destroy the parked rows.
const DB_SCHEMA_POOL_NAMESPACE: &str = synapse_common::test_schema_orphans::PARKED_SCHEMA_NAMESPACE;
/// Per-process cache of "the pool table exists".
///
/// Deliberately *not* a `OnceCell`: the pool lives in its own schema, which only
/// this module creates — but a stale database, a manual `DROP`, or a cleanup
/// script can still remove it, and "the table exists" is then a fact that must
/// be re-established. An `AtomicBool` can be cleared when an operation observes
/// the table gone (see [`is_pool_storage_gone`]).
static DB_POOL_TABLE_READY: AtomicBool = AtomicBool::new(false);

/// SQLSTATE `42P01` — `undefined_table`, or `3F000` — `invalid_schema_name`.
///
/// Both mean "the pool table (or the schema holding it) was removed under us".
fn is_pool_storage_gone(error: &sqlx::Error) -> bool {
    error.as_database_error().and_then(|error| error.code()).is_some_and(|code| code == "42P01" || code == "3F000")
}

/// True when the cross-process pool is the right place to park a schema.
///
/// Only under nextest: a single `cargo test` process gets the cheaper
/// process-local `SCHEMA_POOL` and must not pay a database round trip per test.
fn db_schema_pool_enabled() -> bool {
    running_under_nextest() && test_schema_pool_reuse_enabled()
}

/// A short-lived admin pool for the pool table (claims, parks, reaps).
///
/// Not cached: each nextest process claims once or twice and exits, so a
/// module-level pool would mostly sit idle and would have to be tied to a
/// runtime that dies with the test.
async fn connect_schema_pool_admin(database_url: &str) -> Result<PgPool, String> {
    tokio::time::timeout(
        configured_test_pool_connect_timeout(),
        synapse_common::test_isolation::connect_pool_with_handshake_retry(database_url, 2, Duration::from_secs(5)),
    )
    .await
    .map_err(|_| "timed out connecting to the cross-process schema pool table".to_string())?
    .map_err(|error| format!("failed to connect to the cross-process schema pool table: {error}"))
}

/// Create [`DB_SCHEMA_POOL_TABLE`] once per process.
///
/// `CREATE TABLE IF NOT EXISTS` is not atomic against a concurrent creator (the
/// loser dies on `pg_type_typname_nsp_index`), and nextest runs several test
/// processes at once, so creation is serialized by an advisory transaction lock.
/// A failure is not cached: the flag is only set after the commit succeeds.
async fn ensure_db_schema_pool_table(pool: &PgPool) -> Result<(), String> {
    if DB_POOL_TABLE_READY.load(Ordering::SeqCst) {
        return Ok(());
    }
    let mut tx = pool.begin().await.map_err(|error| format!("could not begin pool-table creation: {error}"))?;
    sqlx::query("SELECT pg_advisory_xact_lock($1)")
        .bind(DB_SCHEMA_POOL_ADVISORY_KEY)
        .execute(&mut *tx)
        .await
        .map_err(|error| format!("could not serialize pool-table creation: {error}"))?;
    // Both statements run inside the advisory-locked transaction, and this is
    // the only code that touches the bookkeeping schema, so the `IF NOT EXISTS`
    // checks cannot lose a race with themselves.
    sqlx::raw_sql(&format!(
        "CREATE SCHEMA IF NOT EXISTS {DB_SCHEMA_POOL_NAMESPACE}; \
         CREATE TABLE IF NOT EXISTS {DB_SCHEMA_POOL_TABLE} (\
             schema_name TEXT PRIMARY KEY, \
             template_name TEXT NOT NULL, \
             parked_ts BIGINT NOT NULL)"
    ))
    .execute(&mut *tx)
    .await
    .map_err(|error| format!("could not create {DB_SCHEMA_POOL_TABLE}: {error}"))?;
    tx.commit().await.map_err(|error| format!("could not commit pool-table creation: {error}"))?;
    DB_POOL_TABLE_READY.store(true, Ordering::SeqCst);
    Ok(())
}

/// Forget the cached "table exists" answer so the next call recreates it.
fn forget_db_schema_pool_table() {
    DB_POOL_TABLE_READY.store(false, Ordering::SeqCst);
}

/// Publish `schema_name` as reusable for every other test process.
///
/// `parked_ts` is injected (rather than read here) so the tests can park a
/// back-dated entry and drive the reaper through its real code path.
///
/// A missing table is retried once: the bookkeeping schema can be removed
/// between the readiness check and the `INSERT` (a manual `DROP`, a cleanup
/// script), and the retry rebuilds it instead of failing a passing test.
async fn park_schema_in_db_pool(
    pool: &PgPool,
    schema_name: &str,
    template_name: &str,
    parked_ts: i64,
) -> Result<(), String> {
    ensure_db_schema_pool_table(pool).await?;
    match insert_parked_schema(pool, schema_name, template_name, parked_ts).await {
        Ok(()) => Ok(()),
        Err(error) if is_pool_storage_gone(&error) => {
            forget_db_schema_pool_table();
            ensure_db_schema_pool_table(pool).await?;
            insert_parked_schema(pool, schema_name, template_name, parked_ts)
                .await
                .map_err(|error| format!("could not park {schema_name} in the cross-process schema pool: {error}"))
        }
        Err(error) => Err(format!("could not park {schema_name} in the cross-process schema pool: {error}")),
    }
}

async fn insert_parked_schema(
    pool: &PgPool,
    schema_name: &str,
    template_name: &str,
    parked_ts: i64,
) -> Result<(), sqlx::Error> {
    sqlx::query(&format!(
        "INSERT INTO {DB_SCHEMA_POOL_TABLE} (schema_name, template_name, parked_ts) VALUES ($1, $2, $3) \
         ON CONFLICT (schema_name) DO UPDATE \
         SET template_name = EXCLUDED.template_name, parked_ts = EXCLUDED.parked_ts"
    ))
    .bind(schema_name)
    .bind(template_name)
    .bind(parked_ts)
    .execute(pool)
    .await
    .map(|_| ())
}

/// Take one reusable schema of `template_name` out of the shared pool.
///
/// A row can outlive its schema (interrupted run, a reap in another process, the
/// orphan guard), and `create_pool_for_schema` hard-fails on a schema that does
/// not exist, so every claimed name is verified before it is handed out. The
/// `DELETE ... RETURNING` both removes the row and makes the claim atomic: a
/// stale row is consumed, not skipped, so the loop always makes progress and
/// cannot spin on the same entry.
async fn claim_schema_from_db_pool(pool: &PgPool, template_name: &str) -> Result<Option<String>, String> {
    ensure_db_schema_pool_table(pool).await?;
    for _ in 0..8 {
        let claimed = match delete_one_pooled_schema(pool, template_name).await {
            Ok(claimed) => claimed,
            Err(error) if is_pool_storage_gone(&error) => {
                forget_db_schema_pool_table();
                ensure_db_schema_pool_table(pool).await?;
                delete_one_pooled_schema(pool, template_name)
                    .await
                    .map_err(|error| format!("could not claim a schema from the cross-process pool: {error}"))?
            }
            Err(error) => {
                return Err(format!("could not claim a schema from the cross-process pool: {error}"));
            }
        };
        let Some(schema_name) = claimed else {
            return Ok(None);
        };
        let exists: Option<String> = sqlx::query_scalar("SELECT to_regnamespace($1)::text")
            .bind(&schema_name)
            .fetch_one(pool)
            .await
            .map_err(|error| format!("could not verify claimed schema {schema_name}: {error}"))?;
        if exists.is_some() {
            return Ok(Some(schema_name));
        }
        eprintln!("schema pool: discarding cross-process pool entry {schema_name} (schema no longer exists)");
    }
    Ok(None)
}

async fn delete_one_pooled_schema(pool: &PgPool, template_name: &str) -> Result<Option<String>, sqlx::Error> {
    sqlx::query_scalar(&format!(
        "DELETE FROM {DB_SCHEMA_POOL_TABLE} \
         WHERE ctid = (SELECT ctid FROM {DB_SCHEMA_POOL_TABLE} \
                       WHERE template_name = $1 ORDER BY parked_ts LIMIT 1 FOR UPDATE SKIP LOCKED) \
         RETURNING schema_name"
    ))
    .bind(template_name)
    .fetch_optional(pool)
    .await
}

/// Drop every pooled schema parked for at least `ttl`, returning how many.
///
/// DROP is unconditional: the row was removed by the same atomic statement, so
/// no other process can be holding the schema (and a lease held by a dying
/// process must not turn into a leak).
async fn reap_expired_db_pool_entries(pool: &PgPool, now_ts: i64, ttl: Duration) -> Result<usize, String> {
    ensure_db_schema_pool_table(pool).await?;
    let cutoff = now_ts.saturating_sub(i64::try_from(ttl.as_millis()).unwrap_or(i64::MAX));
    let expired = match delete_expired_pooled_schemas(pool, cutoff).await {
        Ok(expired) => expired,
        Err(error) if is_pool_storage_gone(&error) => {
            forget_db_schema_pool_table();
            ensure_db_schema_pool_table(pool).await?;
            delete_expired_pooled_schemas(pool, cutoff)
                .await
                .map_err(|error| format!("could not reap expired cross-process pool entries: {error}"))?
        }
        Err(error) => {
            return Err(format!("could not reap expired cross-process pool entries: {error}"));
        }
    };
    if !expired.is_empty() {
        eprintln!("schema pool: dropping {} schema(s) parked for at least {ttl:?}", expired.len());
    }
    for schema_name in &expired {
        let _ = drop_schema(pool, schema_name).await;
    }
    Ok(expired.len())
}

async fn delete_expired_pooled_schemas(pool: &PgPool, cutoff: i64) -> Result<Vec<String>, sqlx::Error> {
    sqlx::query_scalar(&format!("DELETE FROM {DB_SCHEMA_POOL_TABLE} WHERE parked_ts < $1 RETURNING schema_name"))
        .bind(cutoff)
        .fetch_all(pool)
        .await
}

/// A schema name taken out of a pool, plus the admin connection that protects
/// it until the test pool is up.
///
/// Under nextest the name comes from a row the claim already deleted, so between
/// that `DELETE` and the test pool's first `after_connect` the schema is neither
/// parked nor leased — exactly what `test_schema_orphans` treats as an orphan,
/// and a reused schema's name-derived age passed `ORPHAN_SCHEMA_MIN_AGE` long
/// ago. A shared lease (compatible with the `after_connect` shared leases) closes
/// that window: once the test pool holds its own, closing the admin pool hands
/// the schema over without a gap.
struct ClaimedSchema {
    name: String,
    lease_guard: Option<PgPool>,
}

/// Take a shared lease on `schema_name` and return the connection to the pool.
///
/// Session-scoped, so the lock survives the hand-back and lasts until the pool's
/// connection is closed.
async fn hold_schema_lease(pool: &PgPool, schema_name: &str) -> Result<(), String> {
    let mut conn = pool.acquire().await.map_err(|error| format!("could not acquire an admin connection: {error}"))?;
    attach_schema_lease(&mut conn, schema_name)
        .await
        .map_err(|error| format!("could not take the shared lease on {schema_name}: {error}"))
}

/// Claim a reusable schema for `template_name`, or `None` when no pool has one.
///
/// The single entry point both acquisition paths use, so the process-local and
/// the cross-process pools cannot drift apart (and a change to the "which pool"
/// decision applies to both). The caller must drop [`ClaimedSchema::lease_guard`]
/// — closing the admin pool — only after the test pool is connected.
async fn take_reusable_schema_name(database_url: &str, template_name: &str) -> Option<ClaimedSchema> {
    if !test_schema_pool_reuse_enabled() {
        return None;
    }
    if !db_schema_pool_enabled() {
        return SCHEMA_POOL.lock().await.pop().map(|parked| ClaimedSchema { name: parked.name, lease_guard: None });
    }
    let admin_pool = match connect_schema_pool_admin(database_url).await {
        Ok(pool) => pool,
        Err(error) => {
            eprintln!("schema pool: {error}; cloning a fresh schema instead");
            return None;
        }
    };
    if let Err(error) = reap_expired_db_pool_entries(&admin_pool, current_timestamp_millis(), DB_SCHEMA_POOL_TTL).await
    {
        eprintln!("schema pool: reaping expired cross-process entries failed: {error}");
    }
    let claimed = claim_schema_from_db_pool(&admin_pool, template_name).await;
    match claimed {
        Ok(Some(name)) => {
            if let Err(error) = hold_schema_lease(&admin_pool, &name).await {
                // Handing out a schema no lease protects would let the orphan
                // guard drop it under the test's feet; a fresh clone is safe.
                eprintln!("schema pool: {error}; cloning a fresh schema instead");
                let _ = drop_schema(&admin_pool, &name).await;
                admin_pool.close().await;
                return None;
            }
            Some(ClaimedSchema { name, lease_guard: Some(admin_pool) })
        }
        Ok(None) => {
            admin_pool.close().await;
            None
        }
        Err(error) => {
            eprintln!("schema pool: {error}; cloning a fresh schema instead");
            admin_pool.close().await;
            None
        }
    }
}

// RwLock to prevent deadlock between init_template_schema (write lock, ALTER
// TABLE on template) and clone_schema_from_template (read lock, CREATE TABLE
// LIKE on template). Without this, the ALTER TABLE's AccessExclusiveLock and
// CREATE TABLE LIKE's AccessShareLock deadlock on first run when OnceCell
// initialization is retried after a runtime cancellation.
static TEMPLATE_RW_LOCK: TokioRwLock<()> = TokioRwLock::const_new(());

// Lowered 40 → 10 on 2026-10-19. The 40 ceiling was never demand (see the
// model below), but it let a single fixture hold up to 40 server backends at
// ~10 MB each, and the 16 GB host swapped out with 10 test processes before
// `max_connections` was ever reached. 10 still exceeds what any fixture asks
// for (queries are serial) while bounding the worst case to
// `test-threads × 10`.
const DEFAULT_TEST_DB_MAX_CONNECTIONS: u32 = 10;
const DEFAULT_TEST_DB_MIN_CONNECTIONS: u32 = 0;
const DEFAULT_TEST_DB_CONNECT_TIMEOUT_SECS: u64 = 30;
const DEFAULT_TEST_DB_ACQUIRE_TIMEOUT_SECS: u64 = 30;
const DEFAULT_TEST_DB_IDLE_TIMEOUT_SECS: u64 = 60;
const DEFAULT_TEST_DB_MAX_LIFETIME_SECS: u64 = 300;
const DEFAULT_TEST_DB_INIT_TIMEOUT_SECS: u64 = 300;
// P1: raised from 8 → 12 to match nextest ci test-threads=12.
//
// Connection-budget accounting (recalibrated 2026-09-19 — see
// `tests/unit/test_connection_budget_tests.rs`):
//
//   - `tests/common::get_test_pool_async()` builds a NEW pool per test, so each
//     concurrently running test owns a pool whose *ceiling* is
//     `DEFAULT_TEST_DB_MAX_CONNECTIONS = 40`.
//   - That ceiling is not demand. `DEFAULT_TEST_DB_MIN_CONNECTIONS = 0` (sqlx
//     opens lazily) and every DB fixture issues its queries serially — the
//     isolated pool documents this and caps itself at `max_connections(1)`.
//   - The budget therefore counts **demand**: `test-threads × connections held
//     per test` + a reserve for the admin/template/clone pools, which at
//     test-threads=12 is 12 × 1 + 20 = 32 ≤ 100.
//   - The earlier model (`12 × 40 = 480 > 100`) described a violation that never
//     happened and was only `println!`-ed, so it enforced nothing. The gate now
//     asserts the demand model and can go red (raise test-threads or the
//     per-test constant).
//
// History: the same commit reported "1417 passed / 9 flagged" at low load and
// "1408 passed, 12 failed, 7 timed out" under load, while the timed-out set
// passed 13/13 when re-run serially — starvation, not a code defect (see
// 已删除的 P0 基线记录 §2.2.1). Keep heavy groups on low
// `--test-threads`, or lower TEST_DB_MAX_CONNECTIONS.
const DEFAULT_TEST_DB_SHARED_CLONE_CONCURRENCY: usize = 12;
const TEST_TEMPLATE_SCHEMA_REVISION: u32 = 2;

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
        .map_or(if is_low_memory_environment() { 6 } else { DEFAULT_TEST_DB_SHARED_CLONE_CONCURRENCY }, |v| v)
}

/// See [`configured_test_db_template_schema`].
pub fn configured_test_db_template_schema() -> Option<String> {
    env_string("TEST_DB_TEMPLATE_SCHEMA")
}

/// Extensions that must be installed in `public`, never in a template schema.
///
/// PostgreSQL extensions are **database-scoped**, not schema-scoped: a database
/// holds at most one `pgcrypto`, living in exactly one schema. A clone schema
/// therefore *cannot* own its own copy — `CREATE EXTENSION` inside a clone is a
/// no-op once the extension exists anywhere, and `ALTER EXTENSION ... SET SCHEMA`
/// would move the single instance *out of* the template, breaking it for every
/// other clone.
///
/// Clones must instead resolve extension functions through `search_path`, which
/// is `<clone>, public` for every pooled and isolated schema. So any extension
/// the baseline creates has to be pinned to `public`.
///
/// Leaving one in the template schema breaks things two ways:
///   * `clone_statement` used to replay it as ordinary functions, emitting
///     `CREATE ... LANGUAGE c` for its C functions. A non-superuser role is
///     refused with `permission denied for language c`, so every
///     pooled-schema test silently skipped instead of running;
///   * the functions would be unreachable from the clone's `search_path`.
///
/// `pgcrypto` is the one that regressed (its C functions ended up in the
/// template); `pg_trgm` had an explicit pin already. Keep both here so the
/// invariant has a single definition.
const PUBLIC_EXTENSIONS: &[&str] = &["pgcrypto", "pg_trgm"];

/// Pin [`PUBLIC_EXTENSIONS`] into `public`. Idempotent.
///
/// Failure is reported rather than swallowed: `ALTER EXTENSION ... SET SCHEMA`
/// requires ownership of the extension's **member objects**, not just the
/// extension. A database whose extensions were originally installed by a
/// different (typically superuser) role therefore cannot be repaired by the
/// unprivileged test role — measured here: `must be owner of function digest`
/// while the extension itself is owned by the test role. Historically this
/// failure was hidden by a bare `let _ =`, which is how pgcrypto ended up
/// pinned inside a template schema without anyone noticing.
///
/// Nothing in the baseline or in Rust calls a pgcrypto function by name
/// (`gen_random_uuid()` is a `pg_catalog` built-in since PG13; the only
/// `digest` in the tree is `sha2::Sha256::digest`), and the clone's inventory
/// check ignores extension members, so an unrelocatable extension is currently
/// harmless. The durable fix is for the baseline to say
/// `CREATE EXTENSION ... SCHEMA public` so a fresh template is correct by
/// construction; until then this warning is the visible signal.
async fn pin_public_extensions(pool: &PgPool) {
    static WARNED: std::sync::Once = std::sync::Once::new();
    for extension in PUBLIC_EXTENSIONS {
        // `IF NOT EXISTS` is a no-op when the extension already exists in ANY
        // schema, so the `ALTER` is what actually relocates it.
        let created =
            sqlx::query(&format!("CREATE EXTENSION IF NOT EXISTS {extension} SCHEMA public")).execute(pool).await;
        let moved = sqlx::query(&format!("ALTER EXTENSION {extension} SET SCHEMA public")).execute(pool).await;
        if let Err(error) = created {
            tracing::debug!("CREATE EXTENSION {extension} skipped: {error}");
        }
        if let Err(error) = moved {
            WARNED.call_once(|| {
                tracing::warn!(
                    extension,
                    error = %error,
                    "cannot relocate extension into `public` (needs member ownership); clones will \
                     not see its functions by name. Fix the baseline to create it with \
                     `SCHEMA public` and rebuild the template."
                );
            });
        }
    }
}

/// Take the schema's **connection lease** on `conn`.
///
/// The lock is session-scoped and **shared**, so PostgreSQL releases it when the
/// connection closes, several connections of the same pool may hold it at once,
/// and the janitor's *exclusive* `pg_try_advisory_lock` (see
/// `synapse_common::test_schema_guard::schema_lease_key`) succeeds only once
/// none of them is open. This is what keeps a per-test schema alive while a
/// service holds only an inner `PgPool` clone (`(**pool).clone()`) after the
/// fixture dropped its `Arc<PgPool>` — the §1.9.1 media flake.
async fn attach_schema_lease(conn: &mut sqlx::PgConnection, schema: &str) -> Result<(), sqlx::Error> {
    sqlx::query("SELECT pg_advisory_lock_shared($1)").bind(schema_lease_key(schema)).execute(&mut *conn).await?;
    Ok(())
}

/// See [`prepare_isolated_test_pool`].
pub async fn prepare_isolated_test_pool() -> Result<Arc<PgPool>, String> {
    #[cfg(test)]
    crate::test_exit_hook::ensure();
    let database_url = resolve_test_database_url().await?;
    if db_isolation::enabled() {
        return db_isolation::prepare_pool(&database_url).await;
    }
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
        synapse_common::test_isolation::connect_pool_with_handshake_retry(&database_url, 1, Duration::from_secs(5)),
    )
    .await
    .map_err(|_| format!("failed to connect admin pool: timed out after {connect_timeout:?}"))?
    .map_err(|error| format!("failed to connect admin pool: {error}"))?;

    sqlx::query(&format!("CREATE SCHEMA {schema_name}"))
        .execute(&admin_pool)
        .await
        .map_err(|error| format!("failed to create schema {schema_name}: {error}"))?;

    // Baseline extensions must live in `public` (see `PUBLIC_EXTENSIONS`).
    pin_public_extensions(&admin_pool).await;

    let search_path_sql = format!("SET search_path TO {schema_name}, public");
    let lease_schema = schema_name.clone();
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
                let lease_schema = lease_schema.clone();
                Box::pin(async move {
                    sqlx::query(&search_path_sql).execute(&mut *connection).await?;
                    attach_schema_lease(connection, &lease_schema).await?;
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
        DatabaseInitService::new(pool.clone())
            .with_mode(DatabaseInitMode::Strict)
            .with_migrations_dir(workspace_migrations_dir())
            .initialize(),
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
    #[cfg(test)]
    crate::test_exit_hook::ensure();
    let database_url = resolve_test_database_url().await?;
    if db_isolation::enabled() {
        return db_isolation::prepare_pool(&database_url).await;
    }
    let template = get_template_schema_name(&database_url).await?;

    if test_schema_pool_reuse_enabled() {
        ensure_schema_pool_exit_drain(&database_url);
        ensure_schema_pool_idle_reclaimer(&database_url);
        if let Some(claimed) = take_reusable_schema_name(&database_url, &template).await {
            let ClaimedSchema { name: schema_name, lease_guard } = claimed;
            let connected = create_pool_for_schema(&database_url, &schema_name).await;
            if let Some(admin_pool) = lease_guard {
                admin_pool.close().await;
            }
            let pool = connected?;
            register_pending_schema_return(&pool, &schema_name, &template, &database_url);
            return Ok(pool);
        }
    }

    // Clone template into a fresh per-test schema
    let _permit = SHARED_CLONE_SEMAPHORE.acquire().await.map_err(|_| "shared clone semaphore closed".to_string())?;
    let (pool, schema_name) = clone_schema_from_template(&database_url, &template).await?;
    if test_schema_pool_reuse_enabled() {
        register_pending_schema_return(&pool, &schema_name, &template, &database_url);
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
                synapse_common::test_isolation::connect_pool_with_handshake_retry(
                    database_url,
                    1,
                    Duration::from_secs(5),
                ),
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
        synapse_common::test_isolation::connect_pool_with_handshake_retry(database_url, 1, Duration::from_secs(5)),
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

    // Baseline extensions must live in `public` (NOT template_name) so their
    // functions (similarity(), %, digest(), crypt(), ...) are resolvable from
    // any clone via the standard search_path `test_XXX, public`; template_name
    // is NOT on a clone's search_path. See `PUBLIC_EXTENSIONS` for why the
    // extension cannot simply be cloned along with the template.
    pin_public_extensions(&admin_pool).await;

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
        DatabaseInitService::new(pool.clone())
            .with_mode(DatabaseInitMode::Strict)
            .with_migrations_dir(workspace_migrations_dir())
            .initialize(),
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
        synapse_common::test_isolation::connect_pool_with_handshake_retry(database_url, 1, Duration::from_secs(5)),
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

// The ready-marker directory/file helpers now live in
// `synapse_common::test_isolation` — one definition shared with the isolation
// template family (which previously wrote no file marker at all, so
// `cleanup_test_schemas.sh` treated the current isolation template as a deletion
// candidate; follow-up doc §2.5). Re-exported under the old names so this module's
// call sites and the marker reader keep a single path convention.
use synapse_common::test_isolation::template_ready_marker_path;

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
///
/// Public so `tests/unit/` can exercise the drop/preserve decision against
/// throwaway schemas instead of the real template set — through
/// [`prune_stale_template_schemas_in`] inside a rolled-back transaction, because
/// the sweep cannot tell a throwaway `keep` from the real one and would
/// otherwise delete the template a concurrent test is cloning from.
pub async fn prune_stale_template_schemas(admin_pool: &PgPool, keep: &str) -> Result<Vec<String>, String> {
    let mut conn = admin_pool
        .acquire()
        .await
        .map_err(|error| format!("failed to acquire a connection to prune templates: {error}"))?;
    let dropped = prune_stale_template_schemas_in(&mut conn, keep).await?;

    // Marker files are outside the database, so they are removed only after the
    // DROPs above have run on the connection (each one autocommits). Callers that
    // drive the sweep inside a transaction they intend to roll back must use
    // [`prune_stale_template_schemas_in`] directly, or a surviving template would
    // lose its run-speed marker.
    for schema in &dropped {
        let _ = std::fs::remove_file(template_ready_marker_path(schema));
    }

    Ok(dropped)
}

/// The sweep behind [`prune_stale_template_schemas`], over a borrowable
/// connection, so a caller can run it inside a transaction it rolls back.
///
/// Only the tests need that: the sweep drops **every** member of
/// `test_template_v<rev>_<hex>` except `keep`, so exercising it against the live
/// database with a synthetic `keep` deletes the template a concurrent test may be
/// cloning from. Measured on a full gate: the template vanished between
/// `acquire_pooled_schema`'s readiness check and its clone, and
/// `pooled_schema_reseed_tests::pooled_schema_can_insert_into_a_reseeded_table`
/// failed with `schema "test_template_v2_…" does not exist`. PostgreSQL DDL is
/// transactional, so a test that runs this inside a rolled-back transaction
/// observes the same drop/preserve decisions without touching the real set.
pub async fn prune_stale_template_schemas_in(conn: &mut sqlx::PgConnection, keep: &str) -> Result<Vec<String>, String> {
    let keep_exists: bool = sqlx::query_scalar("SELECT EXISTS (SELECT 1 FROM pg_namespace WHERE nspname = $1)")
        .bind(keep)
        .fetch_one(&mut *conn)
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
    .fetch_all(&mut *conn)
    .await
    .map_err(|error| format!("failed to list stale templates: {error}"))?;

    let mut dropped = Vec::new();
    for schema in stale {
        match sqlx::query(&format!("DROP SCHEMA IF EXISTS {} CASCADE", quote_ident(&schema))).execute(&mut *conn).await
        {
            Ok(_) => dropped.push(schema),
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
        synapse_common::test_isolation::connect_pool_with_handshake_retry(database_url, 1, Duration::from_secs(5)),
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

/// Locate the workspace `migrations/` directory.
///
/// It cannot be derived from this crate's own layout: `CARGO_MANIFEST_DIR` is
/// `synapse-test-utils/`, which has no `migrations/`. The previous
/// `join("migrations")` therefore always missed, `read_dir` failed, and the
/// fingerprint below silently degraded to the constant `migrations-dir-missing` —
/// so **editing the baseline never rebuilt the template** and the whole integration
/// suite kept exercising a stale schema (observed 2026-09-18: making
/// `e2ee_audit_log.device_id` nullable had no effect until the template was dropped
/// by hand). `migrations/README.md` claims the opposite ("内容指纹随之变化，会铸造一次
/// 新模板"), which is only true once this resolves correctly.
// reason: 测试基建：找不到 `migrations/` 说明工作区布局被破坏；静默降级会让整条集成链跑陈旧 schema（见上方注释）
#[allow(clippy::expect_used)]
fn workspace_migrations_dir() -> std::path::PathBuf {
    std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .ancestors()
        .map(|ancestor| ancestor.join("migrations"))
        .find(|candidate| {
            candidate.is_dir()
                && fs::read_dir(candidate).is_ok_and(|mut entries| {
                    entries
                        .any(|entry| entry.is_ok_and(|entry| entry.path().extension().is_some_and(|ext| ext == "sql")))
                })
        })
        .expect(
            "no workspace `migrations/` directory containing .sql files was found at or above \
             CARGO_MANIFEST_DIR; test templates cannot be fingerprinted against the schema",
        )
}

fn template_schema_fingerprint() -> String {
    // Hashing ~250KB of SQL on every acquire would be pure waste; the answer cannot
    // change within a process.
    static FINGERPRINT: std::sync::OnceLock<String> = std::sync::OnceLock::new();
    FINGERPRINT.get_or_init(compute_template_schema_fingerprint).clone()
}

/// Whether a regular file in `migrations/` is an input to the template schema.
///
/// Only `*.sql` is. The directory also holds documentation (`README.md`,
/// `INDEXES.md`) which is legitimately edited; the fingerprint has to answer
/// "is this schema still the one the migrations describe?", and a `.md` file
/// never creates a table. Including documentation meant a pure doc edit renamed
/// the template (`4dbe2a3f35bb97eb` → `854961cd2ea546e5`, observed 2026-09-18)
/// and triggered a needless full rebuild (~35–60s), while making "editing
/// `migrations/` re-mints the template" a noisy signal.
pub fn is_schema_input(file_name: &str) -> bool {
    std::path::Path::new(file_name).extension().is_some_and(|extension| extension == "sql")
}

/// Build the manifest whose FNV-1a hash is the template fingerprint.
///
/// Split out of [`compute_template_schema_fingerprint`] so that "which files
/// count as schema input" is directly testable against a scratch directory,
/// without the workspace ``migrations/`` path or `tempfile`.
pub fn template_schema_manifest(migrations_dir: &std::path::Path) -> String {
    let mut manifest =
        format!("schema-rev:{TEST_TEMPLATE_SCHEMA_REVISION};contract-sql:{};", ensure_test_schema_contract_sql());

    // Hash the file **contents**, not (length, mtime): a `git checkout`/`git stash`
    // rewrites mtimes without changing content (needless rebuild) while a same-length
    // edit can keep both (stale template served for a changed schema). Content is the
    // only signal that matches the question being asked — "is this schema still the
    // one the migrations describe?".
    let mut migration_entries: Vec<(String, String)> = fs::read_dir(migrations_dir)
        .unwrap_or_else(|error| panic!("cannot read migrations directory {}: {error}", migrations_dir.display()))
        .filter_map(|entry| {
            // Every failure in this loop changes the fingerprint, so none of them
            // may be skipped. The old shape used `.filter_map(Result::ok)` for the
            // directory entries and `fs::read(..).ok()?` for the contents: a file
            // that could not be stat'ed or read silently dropped out of the
            // manifest, the fingerprint stopped describing the schema, and the
            // stale template kept being served (sweep D1 — note the line above
            // already used `panic!`, so only these two were swallowing).
            let entry =
                entry.unwrap_or_else(|error| panic!("cannot read an entry of {}: {error}", migrations_dir.display()));
            let file_type =
                entry.file_type().unwrap_or_else(|error| panic!("cannot stat {}: {error}", entry.path().display()));
            if !file_type.is_file() {
                return None;
            }
            let file_name = entry.file_name().to_str().map_or_else(
                || panic!("migrations entry is not valid UTF-8: {}", entry.path().display()),
                str::to_string,
            );
            if !is_schema_input(&file_name) {
                return None;
            }
            let contents = fs::read(entry.path())
                .unwrap_or_else(|error| panic!("cannot read schema input {}: {error}", entry.path().display()));
            Some((file_name, format!("{:016x}", fnv1a64(&contents))))
        })
        .collect();
    migration_entries.sort();
    for (file_name, content_hash) in migration_entries {
        manifest.push_str(&format!("{file_name}:{content_hash};"));
    }

    manifest
}

fn compute_template_schema_fingerprint() -> String {
    let manifest = template_schema_manifest(&workspace_migrations_dir());
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
        synapse_common::test_isolation::connect_pool_with_handshake_retry(database_url, 1, Duration::from_secs(5)),
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

    // Pin the baseline extensions into `public` on EVERY clone, not only at
    // template build time. A template created before the pin existed still holds
    // e.g. pgcrypto's C functions inside the template schema — which is not on a
    // clone's `search_path`, so the extension's functions would be unreachable
    // from the clone. Relocating here is idempotent and self-heals any template
    // in that state without needing a rebuild.
    pin_public_extensions(&admin_pool).await;

    // `Only(SEED_REFERENCE_TABLES)` is this fixture's historical behaviour
    // written down: phase 1b copies the baseline's seeded reference rows and
    // nothing else. Behaviour is identical to `Everything` today (measured: the
    // template's only non-empty tables are exactly these), and the
    // `the_seed_allowlist_matches_what_the_baseline_seeds` guard keeps that true.
    synapse_common::test_isolation::clone_schema_from_template(
        &admin_pool,
        &schema_name,
        template_name,
        synapse_common::test_isolation::SeedSource::Only(synapse_common::test_isolation::SEED_REFERENCE_TABLES),
    )
    .await?;

    let search_path_sql = format!("SET search_path TO {schema_name}, public");
    let lease_schema = schema_name.clone();
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
                let lease_schema = lease_schema.clone();
                Box::pin(async move {
                    sqlx::query(&search_path_sql).execute(&mut *connection).await?;
                    attach_schema_lease(connection, &lease_schema).await?;
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
/// pool, take the schema's connection lease, then TRUNCATE + re-seed (returning
/// the schema name to `SCHEMA_POOL`) or DROP it if poisoned/corrupted. Used by
/// both the `LeasedSchema` path and the deferred `prepare_shared_test_pool`
/// return path.
///
/// Returns [`CleanupOutcome::Retry`] when the lease cannot be taken: a
/// connection of the leased pool is still open, so the schema is still in use
/// and must be neither TRUNCATEd nor DROPped. The janitor re-queues the entry
/// for a later pass.
async fn cleanup_schema(
    database_url: String,
    schema_name: String,
    template_name: String,
    poisoned: bool,
) -> CleanupOutcome {
    // Serialize cleanup (see CLEANUP_SEMAPHORE docs) to bound peak lock usage.
    // The semaphore is a process-lifetime static and never closed.
    let Ok(_cleanup_permit) = Arc::clone(&CLEANUP_SEMAPHORE).acquire_owned().await else {
        eprintln!("schema pool: cleanup semaphore closed; skipping cleanup of {schema_name}");
        return CleanupOutcome::Done;
    };

    // Two connections: one that holds the session-level lease for the whole
    // cleanup, one for the TRUNCATE / DROP. The lease must stay on a distinct
    // session from the work, because the work runs through the pool.
    let admin_pool = tokio::time::timeout(
        Duration::from_secs(10),
        synapse_common::test_isolation::connect_pool_with_handshake_retry(&database_url, 2, Duration::from_secs(5)),
    )
    .await;

    let admin_pool = match admin_pool {
        Ok(Ok(pool)) => pool,
        _ => {
            eprintln!("schema pool: failed to connect admin pool for cleanup of {schema_name}; schema orphaned");
            return CleanupOutcome::Done;
        }
    };

    let mut lease_conn = match admin_pool.acquire().await {
        Ok(conn) => conn,
        Err(error) => {
            eprintln!("schema pool: failed to acquire the lease connection for {schema_name}: {error}");
            return CleanupOutcome::Done;
        }
    };
    match try_acquire_schema_lease(&mut lease_conn, &schema_name).await {
        Ok(true) => {}
        Ok(false) => return CleanupOutcome::Retry,
        Err(error) => {
            eprintln!("schema pool: failed to take the schema lease for {schema_name}: {error}");
            return CleanupOutcome::Done;
        }
    }

    // Parking is deferred until after the lease is released: a claiming process
    // takes the shared lease in `after_connect`, so a row published while this
    // session still held the exclusive lease would stall that process.
    let mut park_for_cross_process_reuse = false;
    let outcome = if poisoned {
        let _ = drop_schema(&admin_pool, &schema_name).await;
        CleanupOutcome::Done
    } else {
        match truncate_and_reseed_schema(admin_pool.clone(), &database_url, &schema_name, &template_name).await {
            Ok(()) => {
                if db_schema_pool_enabled() {
                    park_for_cross_process_reuse = true;
                } else {
                    // Safety check passed — return schema name to pool for reuse
                    SCHEMA_POOL
                        .lock()
                        .await
                        .push(ParkedSchema { name: schema_name.clone(), parked_at: std::time::Instant::now() });
                }
                CleanupOutcome::Done
            }
            Err(error) => {
                eprintln!("schema pool: cleanup failed for {schema_name} ({error}); dropping schema");
                let _ = drop_schema(&admin_pool, &schema_name).await;
                CleanupOutcome::Done
            }
        }
    };

    if let Err(error) = release_schema_lease(&mut lease_conn, &schema_name).await {
        eprintln!("schema pool: failed to release the schema lease for {schema_name}: {error}");
    }
    drop(lease_conn);

    if park_for_cross_process_reuse {
        let parked =
            park_schema_in_db_pool(&admin_pool, &schema_name, &template_name, current_timestamp_millis()).await;
        if let Err(error) = parked {
            eprintln!("schema pool: {error}; dropping schema instead of parking it");
            let _ = drop_schema(&admin_pool, &schema_name).await;
        }
    }

    admin_pool.close().await;
    outcome
}

/// How long the exit path waits for the schema's own connections to disappear.
///
/// The pool is dropped microseconds before the exit drain runs, but PostgreSQL
/// releases its session-scoped shared leases only once it notices the closed
/// sockets. The janitor's release path handles that with a backoff + requeue;
/// the exit drain has no requeue, so the wait has to happen here — otherwise
/// every nextest process would drop the very schema it just finished using.
///
/// The backoff sleeps the drain thread rather than using `tokio::time::sleep`:
/// the retry loop lives outside the runtime (see `on_exit` below), so each
/// attempt is one whole blocking cleanup pass.
const EXIT_PARK_ATTEMPTS: usize = 10;
const EXIT_PARK_BACKOFF: Duration = Duration::from_millis(100);

/// Run one [`cleanup_schema`] pass on the janitor's blocking path.
///
/// The future is built on the driving thread rather than here: this `async fn`
/// awaits nested `async fn`s that take `&Pool<Postgres>` / `&mut PgConnection` /
/// `&str`, and handing that future to a `Send`-bounded bridge makes rustc prove
/// `Send` under a higher-ranked binder, which fails with "`Send` is not general
/// enough". See [`run_cleanup_blocking_outcome_lazy`].
fn run_schema_cleanup_blocking(database_url: String, schema_name: String, template_name: String) -> CleanupOutcome {
    run_cleanup_blocking_outcome_lazy(move || async move {
        cleanup_schema(database_url, schema_name, template_name, false).await
    })
}

/// Cleanup strategy for a schema that may be reused across tests — and, under
/// nextest, across *processes*.
///
/// * `on_release`: a poisoned schema is dropped (lease-guarded); otherwise it is
///   TRUNCATEd + re-seeded and its name is parked in whichever pool fits the
///   runner (`SCHEMA_POOL` under `cargo test`, the shared table under nextest).
/// * `on_exit`: under nextest this runs the *same* park — the exit drain fires
///   right after the single test of the process, so dropping here would throw
///   away exactly the schema cross-process reuse exists to keep. The drain
///   ignores [`CleanupOutcome::Retry`], so a schema still leased at exit is
///   dropped instead of parked. Under `cargo test` it stays a plain DROP: the
///   process-local pool dies with the process.
///
/// Both paths are lease-guarded: releasing the outer `Arc<PgPool>` is not proof
/// the schema is free, because services can keep only an inner `PgPool` clone.
fn pooled_schema_cleanup(
    database_url: &str,
    schema_name: &str,
    template_name: &str,
    poisoned: Arc<AtomicBool>,
) -> SchemaCleanup {
    let on_release: CleanupFn = {
        let db = database_url.to_string();
        let sn = schema_name.to_string();
        let tn = template_name.to_string();
        let poisoned = poisoned.clone();
        Box::new(move || {
            if poisoned.load(Ordering::SeqCst) {
                return drop_schema_if_unleased_blocking(&db, &sn);
            }
            let db = db.clone();
            let sn = sn.clone();
            let tn = tn.clone();
            run_schema_cleanup_blocking(db, sn, tn)
        })
    };

    let on_exit: CleanupFn = if db_schema_pool_enabled() {
        let db = database_url.to_string();
        let sn = schema_name.to_string();
        let tn = template_name.to_string();
        Box::new(move || {
            if poisoned.load(Ordering::SeqCst) {
                drop_schema_blocking(&db, &sn);
                return CleanupOutcome::Done;
            }
            let (cleanup_db, cleanup_sn, cleanup_tn) = (db.clone(), sn.clone(), tn.clone());
            let mut outcome = CleanupOutcome::Retry;
            for attempt in 1..=EXIT_PARK_ATTEMPTS {
                let (attempt_db, attempt_sn, attempt_tn) = (cleanup_db.clone(), cleanup_sn.clone(), cleanup_tn.clone());
                outcome = run_schema_cleanup_blocking(attempt_db, attempt_sn, attempt_tn);
                if outcome == CleanupOutcome::Done {
                    break;
                }
                if attempt < EXIT_PARK_ATTEMPTS {
                    std::thread::sleep(EXIT_PARK_BACKOFF);
                }
            }

            if outcome == CleanupOutcome::Retry {
                eprintln!("schema pool: {sn} is still leased at exit; dropping it instead of parking");
                drop_schema_blocking(&db, &sn);
            }
            CleanupOutcome::Done
        })
    } else {
        drop_schema_cleanup(database_url, schema_name)
    };

    SchemaCleanup::release_or_exit(on_release, on_exit)
}

/// Register a schema returned by `prepare_shared_test_pool` for deferred return.
/// The janitor watches the pool's `Weak` and runs [`pooled_schema_cleanup`] once
/// the last `Arc<PgPool>` is released.
fn register_pending_schema_return(pool: &Arc<PgPool>, schema_name: &str, template_name: &str, database_url: &str) {
    ensure_schema_pool_exit_drain(database_url);
    ensure_schema_pool_idle_reclaimer(database_url);
    let cleanup = pooled_schema_cleanup(database_url, schema_name, template_name, Arc::new(AtomicBool::new(false)));
    register_schema_cleanup(pool, schema_name, cleanup);
}

/// Install a one-time process-exit drain for `SCHEMA_POOL`.
///
/// Schemas parked back in `SCHEMA_POOL` are *names only* — they have no live
/// `PgPool`, so the janitor's weak-reference watch cannot see them. Without an
/// exit hook, a `cargo test` run ending with reusable schemas parked in the pool
/// would leak them. The janitor runs this callback after every pending cleanup
/// at process exit. Under nextest the per-schema `on_exit` path parks schemas in
/// the cross-process pool instead of `SCHEMA_POOL`, so this drain only ever sees
/// entries from the `cargo test` path.
fn ensure_schema_pool_exit_drain(database_url: &str) {
    SCHEMA_POOL_EXIT_DRAIN_ONCE.call_once(|| {
        let database_url = database_url.to_string();
        register_exit_callback(Box::new(move || {
            let names: Vec<String> = match SCHEMA_POOL.try_lock() {
                Ok(mut guard) => std::mem::take(&mut *guard).into_iter().map(|parked| parked.name).collect(),
                // Another thread is mid-TRUNCATE; the schema will be returned
                // and dropped by that path, or by the next run's cleanup.
                Err(_) => Vec::new(),
            };
            if names.is_empty() {
                return CleanupOutcome::Done;
            }
            eprintln!("schema pool: dropping {} parked reusable schema(s) at exit", names.len());
            for name in &names {
                drop_schema_blocking(&database_url, name);
            }
            CleanupOutcome::Done
        }));
    });
}

/// Install the one-time idle-TTL sweeper for `SCHEMA_POOL`.
///
/// The pool exists to bridge the gap between two tests in the *same* process.
/// A `cargo test` run that parks a schema and then spends minutes elsewhere
/// would otherwise hold that schema (≈255 tables) until the process ends, and
/// the exit drain cannot help — it only runs when the process is already
/// leaving. So a single background thread drops names parked for at least
/// [`SCHEMA_POOL_IDLE_TTL`] every [`SCHEMA_POOL_SWEEP_INTERVAL`].
///
/// A plain OS thread, not a `tokio` task: the sweeper must outlive the
/// short-lived per-test runtimes, must not keep any of them alive, and must
/// work even when no runtime is entered. It is detached and dies with the
/// process. Under nextest nothing is ever parked (`on_release` is a plain DROP,
/// see [`SchemaCleanup::release_or_exit_drop`]), so the thread is not started.
fn ensure_schema_pool_idle_reclaimer(database_url: &str) {
    if running_under_nextest() {
        return;
    }
    SCHEMA_POOL_IDLE_RECLAIMER_ONCE.call_once(|| {
        let database_url = database_url.to_string();
        let spawned = std::thread::Builder::new().name("schema-pool-idle-reclaimer".to_string()).spawn(move || loop {
            std::thread::sleep(SCHEMA_POOL_SWEEP_INTERVAL);
            sweep_idle_parked_schemas(&database_url, std::time::Instant::now(), SCHEMA_POOL_IDLE_TTL);
        });
        if let Err(error) = spawned {
            // Not fatal: the exit drain still cleans up, so the cost of a
            // failed spawn is schema retention until the process exits.
            eprintln!(
                "schema pool: could not start the idle reclaimer thread ({error}); \
                 parked names will now only be dropped at process exit"
            );
        }
    });
}

/// One sweep of `SCHEMA_POOL`: DROP every name parked for at least `ttl`, and
/// report how many were dropped.
///
/// Split out from the reclaimer thread so the sweep and its test run the *same*
/// code — a test of [`take_expired`] alone would only prove the predicate, not
/// that expired names actually leave the database. `try_lock` is deliberate: a
/// test mid-park/mid-pop is not a reason to block the sweeper, and the name it
/// is holding is either already out of the pool (so this sweep cannot see it)
/// or still in it (so the next sweep catches it).
fn sweep_idle_parked_schemas(database_url: &str, now: std::time::Instant, ttl: Duration) -> usize {
    let expired = match SCHEMA_POOL.try_lock() {
        Ok(mut guard) => take_expired(&mut guard, now, ttl),
        Err(_) => return 0,
    };
    if expired.is_empty() {
        return 0;
    }
    eprintln!("schema pool: idle reclaimer dropping {} schema(s) parked for at least {ttl:?}", expired.len());
    for name in &expired {
        drop_schema_blocking(database_url, name);
    }
    expired.len()
}

/// Acquire a schema from the pool. Fast path: claim a pre-TRUNCATEd schema name
/// (`SCHEMA_POOL` inside one process, the shared table across nextest processes)
/// and create a fresh pool for it. Slow path: clone a new schema from the
/// template (first N tests only, where N = parallelism).
///
/// The returned `LeasedSchema` auto-cleans on Drop — no explicit release needed.
/// Tests using `TestContext` get this automatically; no test code changes required.
pub async fn acquire_pooled_schema() -> Result<LeasedSchema, String> {
    #[cfg(test)]
    crate::test_exit_hook::ensure();
    let database_url = resolve_test_database_url().await?;
    // A cloned database is never reused, so `poison()` has nothing to mark.
    if db_isolation::enabled() {
        let pool = db_isolation::prepare_pool(&database_url).await?;
        return Ok(LeasedSchema { pool, inner: None });
    }
    let template_name = get_template_schema_name(&database_url).await?;

    // Fast path: reuse a TRUNCATEd schema (process-local `SCHEMA_POOL` under
    // `cargo test`, the shared table under nextest). Pooled schemas were already
    // validated by `truncate_and_reseed_schema`; skipping the COUNT here removes
    // the pg_catalog query that used to take 1.6-8 s under parallel load (the #1
    // source of slow-statement warnings). Only the name travels from here on.
    if let Some(claimed) = take_reusable_schema_name(&database_url, &template_name).await {
        let ClaimedSchema { name: schema_name, lease_guard } = claimed;
        let connected = create_pool_for_schema(&database_url, &schema_name).await;
        if let Some(admin_pool) = lease_guard {
            admin_pool.close().await;
        }
        let pool = connected?;
        let poisoned = Arc::new(AtomicBool::new(false));
        register_schema_cleanup(
            &pool,
            &schema_name,
            pooled_schema_cleanup(&database_url, &schema_name, &template_name, poisoned.clone()),
        );
        ensure_schema_pool_exit_drain(&database_url);
        return Ok(LeasedSchema { pool, inner: Some(LeasedSchemaInner { poisoned }) });
    }

    // Slow path: clone a new schema from the template
    let _permit = SHARED_CLONE_SEMAPHORE.acquire().await.map_err(|_| "shared clone semaphore closed".to_string())?;
    let (pool, schema_name) = clone_schema_from_template(&database_url, &template_name).await?;
    drop(_permit);

    let poisoned = Arc::new(AtomicBool::new(false));
    register_schema_cleanup(
        &pool,
        &schema_name,
        pooled_schema_cleanup(&database_url, &schema_name, &template_name, poisoned.clone()),
    );
    ensure_schema_pool_exit_drain(&database_url);

    Ok(LeasedSchema { pool, inner: Some(LeasedSchemaInner { poisoned }) })
}

/// Create a fresh PgPool for an existing schema, setting search_path on connect.
/// Used when popping a schema name from the pool (the pool stores names only,
/// not PgPools, to avoid cross-runtime pool issues).
async fn create_pool_for_schema(database_url: &str, schema_name: &str) -> Result<Arc<PgPool>, String> {
    let search_path_sql = format!("SET search_path TO {schema_name}, public");
    let lease_schema = schema_name.to_string();
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
                let lease_schema = lease_schema.clone();
                Box::pin(async move {
                    sqlx::query(&search_path_sql).execute(&mut *connection).await?;
                    attach_schema_lease(connection, &lease_schema).await?;
                    Ok(())
                })
            })
            .connect(database_url),
    )
    .await
    .map_err(|_| format!("failed to connect pooled schema {schema_name}: timed out after {connect_timeout:?}"))?
    .map_err(|error| format!("failed to connect pooled schema {schema_name}: {error}"))?;

    let pool = Arc::new(pool);
    ensure_schema_has_baseline(&pool, schema_name, database_url).await?;
    Ok(pool)
}

/// 断言"这个池真的能看见基线表"。
///
/// 2026-10-21 实测：本机 shell 里残留 `TEST_DATABASE_URL=…:55432/synapse_test`，那个端口上
/// 确实有个同名库、但 `public` 是 **0 表**。测试栈一路"成功"连上去，于是每个用例拿到一个
/// 空 schema：handler 报 `relation "users" does not exist` 的 **500**，看起来完全像应用 bug
/// （实际排查花了 20 分钟，最后靠 `psql` 数表才发现库是空的）。
///
/// "连上了"不等于"库是迁移过的"。这里是所有 schema 池的唯一出口，就在这一层喊出来，
/// 错误信息带上**脱敏后的**连接串，直接指向该跑哪条命令。
pub(crate) async fn ensure_schema_has_baseline(
    pool: &PgPool,
    schema_name: &str,
    database_url: &str,
) -> Result<(), String> {
    let probe: Option<String> = sqlx::query_scalar("SELECT to_regclass('users')::text")
        .fetch_one(pool)
        .await
        .map_err(|error| format!("could not probe schema {schema_name} for the baseline table `users`: {error}"))?;

    if probe.is_some() {
        return Ok(());
    }

    Err(format!(
        "the test schema `{schema_name}` has no `users` table — this database was never migrated \
         (or the template it was cloned from is empty).\n  \
         database: {}\n  \
         fix: bash scripts/db_migrate.sh migrate   (or rebuild the template: \
         bash scripts/test_db_template.sh ensure)\n  \
         also check TEST_DATABASE_URL: it must point at the throwaway test database \
         (port 5432 by default). A stale value from an old worktree (e.g. port 55432) is the \
         classic cause — that database exists but is empty.",
        redact_db_url(database_url)
    ))
}

/// `postgresql://user:password@host/db` → `postgresql://user:***@host/db`。
///
/// 错误信息会进 CI 日志，口令不能跟着进去。
fn redact_db_url(url: &str) -> String {
    let Some((scheme, rest)) = url.split_once("://") else {
        return url.to_string();
    };
    let Some((credentials, tail)) = rest.rsplit_once('@') else {
        return url.to_string();
    };
    // 必须是**第一个**冒号：`user:pa:ss` 用 rsplit 会算出 `user:pa:***`，把口令前半段留在
    // 日志里。用户名里不允许冒号（RFC 3986 userinfo），所以第一次切分就是对的边界。
    let masked = match credentials.split_once(':') {
        Some((user, _password)) => format!("{user}:***"),
        None => credentials.to_string(),
    };
    format!("{scheme}://{masked}@{tail}")
}

/// TRUNCATE all tables in the schema and re-seed reference/config tables from
/// the template. This is the fast cleanup path (~1-2s) that makes schemas
/// reusable across tests without re-cloning.
///
/// Corruption detection has two layers. [`ensure_schema_structure_matches_template`]
/// runs first and rejects a schema whose object inventory drifted from the
/// template — an injected `ALTER TABLE ... ADD CONSTRAINT`, an extra table, a
/// dropped trigger. Without it those survive the TRUNCATE and are handed to the
/// next test (measured: a `CHECK (false)` on `refresh_tokens` leaked across
/// processes and failed six unrelated registration/voice/third-party tests).
/// The TRUNCATE statement is then built from the template's cached table names,
/// so a dropped table fails with "relation does not exist" — both errors are
/// caught by the caller and the schema is DROPped instead of pooled.
///
/// Uses cached template table names (populated once per process) to avoid
/// re-reading pg_tables for the TRUNCATE list on every cleanup cycle.
async fn truncate_and_reseed_schema(
    admin_pool: PgPool,
    database_url: &str,
    schema_name: &str,
    template_name: &str,
) -> Result<(), String> {
    ensure_schema_structure_matches_template(&admin_pool, schema_name, template_name).await?;

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
    sqlx::raw_sql(&sql).execute(&admin_pool).await.map_err(|e| format!("TRUNCATE failed for {schema_name}: {e}"))?;

    // Re-seed reference/config tables from the template — the same set the clone
    // copies, taken from the shared constant so the two paths cannot drift.
    // These are small and fast to copy.
    // Use cached table names to check existence instead of per-table EXISTS queries.
    //
    // The names are copied out of the `&'static [&'static str]` before the loop:
    // iterating it directly leaves a `slice::Iter<'_, &str>` / `&&str` in the
    // generator's state across the awaits below, and rustc then fails to prove
    // this future `Send` ("not general enough"), which the janitor's blocking
    // bridge requires.
    let seed_tables: Vec<String> =
        synapse_common::test_isolation::SEED_REFERENCE_TABLES.iter().map(|table| (*table).to_string()).collect();
    for table in seed_tables {
        if !template_table_names.iter().any(|name| name == &table) {
            eprintln!("schema pool: {table} not found in template {template_name}, skipping reseed");
            continue;
        }

        let sql =
            format!("INSERT INTO {schema_name}.{table} SELECT * FROM {template_name}.{table} ON CONFLICT DO NOTHING");
        // Use raw_sql since table/schema names are validated identifiers
        if let Err(e) = sqlx::raw_sql(&sql).execute(&admin_pool).await {
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
    synapse_common::test_isolation::advance_schema_sequences(&admin_pool, schema_name).await?;

    Ok(())
}

/// Compare a reusable schema's object inventory with the template's and fail on
/// any difference, so [`cleanup_schema`] DROPps it instead of parking it.
///
/// The inventory is the **name** set of tables, constraints and non-internal
/// triggers. Definitions are deliberately not compared: the clone re-points every
/// foreign key at its own schema, so `pg_get_constraintdef` differs legitimately
/// between template and clone. Names still catch every shape a test leaves
/// behind — `ADD/DROP CONSTRAINT`, an extra `CREATE TABLE`, a dropped trigger —
/// and phase 1d/1e of the clone restore the template's index/constraint names,
/// so a pristine clone compares equal (verified against a parked schema: 0 drift).
///
/// A failed comparison also fails closed (schema dropped), never open.
async fn ensure_schema_structure_matches_template(
    pool: &PgPool,
    schema_name: &str,
    template_name: &str,
) -> Result<(), String> {
    const DRIFT_SQL: &str = r#"
        WITH expected AS (
            SELECT 'table:' || tablename AS entry
              FROM pg_tables
             WHERE schemaname = $1 AND tablename <> $3
            UNION ALL
            SELECT 'constraint:' || c.conname AS entry
              FROM pg_constraint c
              JOIN pg_namespace n ON n.oid = c.connamespace
             WHERE n.nspname = $1
            UNION ALL
            SELECT 'trigger:' || t.tgname AS entry
              FROM pg_trigger t
              JOIN pg_class c ON c.oid = t.tgrelid
              JOIN pg_namespace n ON n.oid = c.relnamespace
             WHERE n.nspname = $1 AND NOT t.tgisinternal
        ), actual AS (
            SELECT 'table:' || tablename AS entry
              FROM pg_tables
             WHERE schemaname = $2
            UNION ALL
            SELECT 'constraint:' || c.conname AS entry
              FROM pg_constraint c
              JOIN pg_namespace n ON n.oid = c.connamespace
             WHERE n.nspname = $2
            UNION ALL
            SELECT 'trigger:' || t.tgname AS entry
              FROM pg_trigger t
              JOIN pg_class c ON c.oid = t.tgrelid
              JOIN pg_namespace n ON n.oid = c.relnamespace
             WHERE n.nspname = $2 AND NOT t.tgisinternal
        )
        SELECT entry
          FROM ((SELECT entry FROM expected EXCEPT SELECT entry FROM actual)
                UNION ALL
                (SELECT entry FROM actual EXCEPT SELECT entry FROM expected)) AS drift
         ORDER BY entry
         LIMIT 5
        "#;

    let drift: Vec<String> = sqlx::query_scalar(DRIFT_SQL)
        .bind(template_name)
        .bind(schema_name)
        .bind(synapse_common::test_isolation::TEMPLATE_READY_TABLE)
        .fetch_all(pool)
        .await
        .map_err(|error| format!("structure comparison of {schema_name} against {template_name} failed: {error}"))?;

    if drift.is_empty() {
        return Ok(());
    }
    Err(format!(
        "structure drift against template {template_name} ({}); schema will be dropped, not pooled",
        drift.join(", ")
    ))
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
        synapse_common::test_isolation::connect_pool_with_handshake_retry(&database_url, 1, Duration::from_secs(5)),
    )
    .await
    .map_err(|_| format!("failed to connect admin pool: timed out after {connect_timeout:?}"))?
    .map_err(|error| format!("failed to connect admin pool: {error}"))?;

    sqlx::query(&format!("CREATE SCHEMA {schema_name}"))
        .execute(&admin_pool)
        .await
        .map_err(|error| format!("failed to create schema {schema_name}: {error}"))?;

    // Baseline extensions must live in `public` (see `PUBLIC_EXTENSIONS`).
    pin_public_extensions(&admin_pool).await;

    let search_path_sql = format!("SET search_path TO {schema_name}, public");
    let lease_schema = schema_name.clone();
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
                let lease_schema = lease_schema.clone();
                Box::pin(async move {
                    sqlx::query(&search_path_sql).execute(&mut *connection).await?;
                    attach_schema_lease(connection, &lease_schema).await?;
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
        let mut last_error = None;

        for attempt in 1..=PROBE_ATTEMPTS {
            // Probe with an explicit acquire timeout (30s, see
            // `configured_test_pool_connect_timeout`) so a slow-to-accept server
            // does not overrun the test harness. The 5s default was too tight and
            // contributed to the same P0-1 flap.
            let connect_future =
                PgPoolOptions::new().max_connections(1).acquire_timeout(Duration::from_secs(30)).connect(&database_url);

            match tokio::time::timeout(connect_timeout, connect_future).await {
                Err(_) => {
                    last_error = Some(format!("{database_url} -> connect timed out after {connect_timeout:?}"));
                    break;
                }
                Ok(Ok(pool)) => {
                    drop(pool);
                    synapse_common::test_schema_orphans::run_orphan_guard(&database_url).await?;
                    let url = database_url.clone();
                    *RESOLVED_TEST_DB_URL.lock().unwrap_or_else(|poisoned| poisoned.into_inner()) = Some(url.clone());
                    return Ok(url);
                }
                Ok(Err(error)) => {
                    let message = error.to_string();
                    let transient = is_transient_probe_error(&message);
                    last_error = Some(format!("{database_url} -> {message}"));

                    if !transient || attempt == PROBE_ATTEMPTS {
                        break;
                    }
                    tokio::time::sleep(Duration::from_millis(200 * u64::from(attempt))).await;
                }
            }
        }

        if let Some(error) = last_error {
            errors.push(error);
        }
    }

    Err(format!("failed to connect to any configured test database: {}", errors.join(" | ")))
}

/// How many fresh connections [`resolve_test_database_url`] will open for one
/// candidate URL before giving up.
const PROBE_ATTEMPTS: u32 = 3;

/// Transport-level probe flake worth one more fresh connection.
///
/// A full gate runs ~10k short-lived test processes, each opening its own probe
/// connection. Under that churn sqlx occasionally reads a NUL byte while
/// reading the server's greeting (`unknown message type: '\0'`) — measured on
/// `/tmp/full_gate4.log`, where `api_space_routes_tests::test_create_space_route_rejects_non_creator_for_foreign_room`
/// lost its only probe to it and the whole process failed before any test body
/// ran. The connection never existed, so retrying it says nothing about the
/// database being reachable: a genuinely unreachable / misconfigured URL still
/// fails after all attempts.
fn is_transient_probe_error(message: &str) -> bool {
    message.contains("unknown message type") || message.contains("connection closed")
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

fn next_test_schema_name() -> String {
    synapse_common::test_schema_orphans::next_stamped_schema_name()
}
async fn ensure_test_schema_contract(pool: &Arc<PgPool>) -> Result<(), String> {
    sqlx::raw_sql(ensure_test_schema_contract_sql())
        .execute(&**pool)
        .await
        .map_err(|error| format!("failed to ensure test schema contract: {error}"))?;

    Ok(())
}

fn ensure_test_schema_contract_sql() -> &'static str {
    // Shared with `scripts/test_db_template.sh`, which applies the same file when
    // building the database template: one text, two consumers, no drift.
    include_str!("../../test-support/test_schema_contract.sql")
}

#[cfg(test)]
mod redact_db_url_tests {
    use super::redact_db_url;

    #[test]
    fn password_is_masked_but_the_rest_survives() {
        assert_eq!(
            redact_db_url("postgresql://synapse:s3cr3t@localhost:5432/synapse_test"),
            "postgresql://synapse:***@localhost:5432/synapse_test"
        );
    }

    #[test]
    fn urls_without_credentials_or_scheme_pass_through() {
        // 没有 `@`：整串原样返回（不能把库名当口令吞掉）。
        assert_eq!(redact_db_url("postgresql://localhost/synapse_test"), "postgresql://localhost/synapse_test");
        assert_eq!(redact_db_url("not-a-url"), "not-a-url");
    }

    #[test]
    fn a_password_containing_colons_is_still_masked() {
        // `rsplit_once(':')`：口令里带 `:` 也只把最后一段替换掉，用户名保留。
        assert_eq!(
            redact_db_url("postgres://user:pa:ss@db.example.com:5432/x"),
            "postgres://user:***@db.example.com:5432/x"
        );
    }
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
        crate::truncate_and_reseed_schema(admin.clone(), &database_url, &schema_name, &template_name)
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

/// Tests for the cross-process schema pool table (the nextest path): a name
/// parked by one process must reach exactly one later process, and a name that
/// can no longer serve a test must never be handed out.
///
/// These drive the pool helpers directly against a throwaway schema (`CREATE
/// SCHEMA`, no template clone) so they stay cheap.
#[cfg(test)]
mod db_schema_pool_tests;

/// Tests for the `SCHEMA_POOL` idle-TTL reclaimer (sweep A10).
#[cfg(test)]
mod schema_pool_idle_reclaimer_tests {
    use super::{sweep_idle_parked_schemas, take_expired, ParkedSchema, SCHEMA_POOL, SCHEMA_POOL_IDLE_TTL};
    use std::time::{Duration, Instant};

    fn parked(name: &str, parked_at: Instant) -> ParkedSchema {
        ParkedSchema { name: name.to_string(), parked_at }
    }

    /// The predicate: a name is expired exactly at the TTL boundary, not before.
    /// Parked names must survive a sweep while they are still fresh, otherwise
    /// the reuse fast path (the whole point of the pool) would never pay off.
    #[test]
    fn takes_only_names_parked_for_at_least_the_ttl() {
        let now = Instant::now();
        let ttl = Duration::from_secs(60);
        let mut pool = vec![
            parked("test_fresh", now),
            parked("test_at_boundary", now - ttl),
            parked("test_stale", now - Duration::from_secs(61)),
        ];

        let expired = take_expired(&mut pool, now, ttl);

        assert_eq!(
            expired,
            vec!["test_at_boundary".to_string(), "test_stale".to_string()],
            "only names at or past the TTL may be taken"
        );
        assert_eq!(pool.len(), 1, "the fresh name must stay parked");
        assert_eq!(pool[0].name, "test_fresh");
    }

    /// A sweep that finds nothing must be a no-op, and an empty pool must not
    /// panic — the thread runs this every 15s for the whole process lifetime.
    #[test]
    fn sweeping_a_fresh_or_empty_pool_takes_nothing() {
        let now = Instant::now();
        let ttl = Duration::from_secs(60);
        let mut fresh = vec![parked("test_fresh", now)];
        assert!(take_expired(&mut fresh, now, ttl).is_empty());
        assert_eq!(fresh.len(), 1);

        let mut empty: Vec<ParkedSchema> = Vec::new();
        assert!(take_expired(&mut empty, now, ttl).is_empty());
        assert!(empty.is_empty());
    }

    /// The real thing: a parked name that has outlived the TTL must be gone from
    /// the database after one sweep, driven through the same
    /// `sweep_idle_parked_schemas` the reclaimer thread calls.
    ///
    /// The schema is asserted **present** before the sweep and **absent** after
    /// it. Without the first half a wrong query (`to_regnamespace` typo, wrong
    /// bind) would return `None` either way and the test would pass while
    /// proving nothing — the same "gate that cannot go red" shape the repo has
    /// been bitten by before.
    #[tokio::test]
    async fn expired_parked_schema_is_dropped_from_the_database() {
        if std::env::var("TEST_DATABASE_URL").is_err() {
            eprintln!(
                "SKIPPING expired_parked_schema_is_dropped_from_the_database: TEST_DATABASE_URL is unset, so \
                 this proves nothing."
            );
            return;
        }
        let database_url = crate::resolve_test_database_url().await.expect("database url");
        let template_name = crate::get_template_schema_name(&database_url).await.expect("template");
        let (pool, schema_name) =
            crate::clone_schema_from_template(&database_url, &template_name).await.expect("clone a schema");
        // The janitor owns the schema from here on: if this test fails before the
        // sweep, the name is dropped when the pool's last `Arc` goes away rather
        // than leaking until the process ends.
        synapse_common::test_schema_guard::register_schema_cleanup(
            &pool,
            &schema_name,
            synapse_common::test_schema_guard::SchemaCleanup::drop_only(&database_url, &schema_name),
        );
        // Close before parking: the cloned pool holds a shared connection lease,
        // and the sweep (like the exit drain) is a plain DROP that does not wait
        // for leases — holding the pool open would leave it pointed at a schema
        // that no longer exists.
        pool.close().await;
        drop(pool);

        // Back-date by a full TTL: the production TTL is used rather than zero
        // so the sweep cannot take a name another test parked concurrently.
        SCHEMA_POOL.lock().await.push(parked(&schema_name, Instant::now() - SCHEMA_POOL_IDLE_TTL));
        let admin = sqlx::PgPool::connect(&database_url).await.expect("admin pool");
        let before: Option<String> = sqlx::query_scalar("SELECT to_regnamespace($1)::text")
            .bind(&schema_name)
            .fetch_one(&admin)
            .await
            .expect("query pg_namespace — a failure here would make the absence assertion below vacuous");
        assert_eq!(before.as_deref(), Some(schema_name.as_str()), "precondition: the clone must exist");

        let dropped = sweep_idle_parked_schemas(&database_url, Instant::now(), SCHEMA_POOL_IDLE_TTL);

        assert!(dropped >= 1, "the schema parked beyond the TTL must have been taken");
        let after: Option<String> = sqlx::query_scalar("SELECT to_regnamespace($1)::text")
            .bind(&schema_name)
            .fetch_one(&admin)
            .await
            .expect("query pg_namespace — a failure here would make the absence assertion below vacuous");
        admin.close().await;
        assert!(
            after.is_none(),
            "sweep_idle_parked_schemas reported dropping {schema_name} but it still exists: {after:?}"
        );
    }
}

/// Test-build-only schema-cleanup exit hook (B'). See the module docs.
///
/// The implementation is the shared `synapse-common/tests-support/` file — one
/// copy for all five test builds, not one per crate.
#[cfg(test)]
#[path = "../../synapse-common/tests-support/exit_hook_impl.rs"]
mod test_exit_hook;
