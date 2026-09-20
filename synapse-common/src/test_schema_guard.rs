//! Shared test-schema lifecycle engine.
//!
//! ## Why this module exists
//!
//! Per-test PostgreSQL schemas used to be reclaimed by a process-local
//! registry (`PENDING_SCHEMA_DROPS` / `PENDING_SCHEMA_RETURNS`) plus a sweep
//! triggered on the *next* pool acquisition. That mechanism is fundamentally
//! broken under cargo-nextest, which runs **exactly one test case per
//! process**: the "next acquisition" never happens in the process that
//! created the schema, Rust never runs destructors for `static`s at process
//! exit, and fire-and-forget cleanup tasks spawned on a background runtime
//! are cancelled when the process dies. All three variants were measured
//! leaking 100% of schemas (see `docs/audit/P5_test_schema_accumulation_2026-09-12.md`
//! and `synapse-storage/src/test_isolation.rs`), which is how the local test
//! database accumulated 23,662 leftover schemas.
//!
//! ## Design
//!
//! Cleanup is driven by the **pool's own lifetime**, not by any future event:
//!
//! 1. `register_schema_cleanup` stores a `Weak<PgPool>` next to a cleanup
//!    action and starts a process-wide **janitor thread**.
//! 2. The janitor polls the registry; when the last `Arc<PgPool>` clone is
//!    released (test function returned), the weak reference dies and the
//!    cleanup runs — mid-process, seconds before exit.
//! 3. A `libc::atexit` handler flips an `EXITING` flag and waits (bounded —
//!    see `JANITOR_EXIT_JOIN_TIMEOUT`) for the janitor, so the process cannot
//!    exit before every remaining schema (including pools still held by
//!    `static`s, which never drop) has been dropped with the cheap exit variant
//!    of its cleanup. A release batch that was already mid-flight when the flag
//!    flipped hands its remainder to the **parallel** exit drain instead of
//!    finishing serially — see `run_release_cleanups`.
//! 4. Releasing the outer `Arc<PgPool>` is **not** proof that the schema is
//!    free. Services commonly store an *inner* `PgPool` clone
//!    (`(**pool).clone()`), which is a different `Arc` and keeps the pool — and
//!    therefore its connections — alive after the fixture dropped its
//!    `Arc<PgPool>`. Basing the drop on the weak reference alone therefore
//!    dropped schemas that were still being queried, and unqualified SQL then
//!    silently resolved through `search_path` into the shared `public` schema.
//!    The lease is therefore **connection-derived**: every connection an
//!    isolated pool opens takes a **shared** session-level advisory lock
//!    ([`schema_lease_key`]) in its `after_connect` hook, PostgreSQL releases it
//!    when the connection closes, and a release cleanup takes the *exclusive*
//!    counterpart with `pg_try_advisory_lock` and refuses to drop while any
//!    connection holds the shared one — reporting [`CleanupOutcome::Retry`] so
//!    the registry entry survives for a later pass. A schema is dropped only
//!    once no connection of its pool is open.
//!
//! This works identically under `cargo test` (many tests per process) and
//! nextest (one test per process), and requires no cooperation from call
//! sites beyond the registration that the `prepare_*_test_pool` fixture
//! functions perform internally.
//!
//! The `TestSchemaGuard` returned by those fixtures is a thin ownership
//! handle (pool + schema name). Dropping the guard early is safe: the
//! janitor tracks the pool, not the guard.

use sqlx::postgres::PgPoolOptions;
use sqlx::PgPool;
use std::ops::Deref;
use std::panic::{catch_unwind, AssertUnwindSafe};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, LazyLock, Mutex, MutexGuard, Once, Weak};
use std::time::{Duration, Instant};

/// How often the janitor checks whether registered pools have been released.
const JANITOR_POLL_INTERVAL: Duration = Duration::from_millis(50);

/// How long an entry that reported [`CleanupOutcome::Retry`] waits before the
/// janitor tries again.
///
/// Without it, a schema that stays leased for the whole test (a service holding
/// an inner `PgPool` clone) would make the janitor open a fresh cleanup
/// connection every poll interval — tens of connects per second, per schema,
/// which is exactly the connection churn the harness avoids elsewhere. The
/// retried action is cheap to repeat, so a short backoff loses nothing.
const LEASE_RETRY_BACKOFF: Duration = Duration::from_millis(250);

/// Timeout for a single cleanup connection attempt.
const CLEANUP_CONNECT_TIMEOUT: Duration = Duration::from_secs(10);
/// Parallelism for the final drain at process exit. Exit cleanups are plain
/// `DROP SCHEMA` calls on independent connections, so a small worker pool
/// keeps process shutdown fast even if a backlog accumulated.
const EXIT_DRAIN_WORKERS: usize = 4;

/// Upper bound on how long the `atexit` handler waits for the janitor thread.
///
/// `JoinHandle::join()` has no timeout, so a janitor parked in a database
/// `await` (unreachable server, pathological lock wait) would hang `exit()` —
/// and with it the whole test process — forever. Measured 2026-09-19: a full
/// integration run left 377 schemas behind and shutdown spent >19 minutes in
/// the *serial* drain; the delegation in [`run_release_cleanups`] removes that
/// specific cause, and this deadline is the remaining fail-safe. Past it the
/// process exits and any un-dropped schemas are left to
/// `scripts/cleanup_test_schemas.sh` rather than blocking CI indefinitely.
const JANITOR_EXIT_JOIN_TIMEOUT: Duration = Duration::from_secs(120);

/// Result of running one cleanup action.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CleanupOutcome {
    /// The action completed; the registry entry can be forgotten.
    Done,
    /// The action was **not** performed because the schema is still leased by a
    /// live connection. The registry entry must survive so a later janitor pass
    /// can retry it.
    Retry,
}

/// Cleanup closure: runs synchronously on the calling thread and must not
/// panic (panics are caught by the janitor and reported on stderr).
///
/// It is `Fn`, not `FnOnce`, because a lease-guarded action can report
/// [`CleanupOutcome::Retry`] and must be callable again on a later pass.
pub type CleanupFn = Box<dyn Fn() -> CleanupOutcome + Send + 'static>;

/// Cleanup actions for one registered test schema.
///
/// Two variants are kept apart deliberately:
///
/// * `on_release` runs when the pool dies while the process is still alive.
///   Under `cargo test` this may be the expensive-but-reusable variant
///   (TRUNCATE + reseed + return the schema name to a reuse pool).
/// * `on_exit` runs when the process is terminating. Returning a schema to a
///   reuse pool is pointless once the process ends, so the exit variant is
///   always the cheap one — a plain `DROP SCHEMA`.
pub struct SchemaCleanup {
    on_release: CleanupFn,
    on_exit: CleanupFn,
}

impl SchemaCleanup {
    /// `on_release` runs `DROP SCHEMA IF EXISTS ... CASCADE` **only while this
    /// process holds the schema's connection lease** (see [`schema_lease_key`]),
    /// retrying on a later janitor pass otherwise; `on_exit` stays an
    /// unconditional `DROP SCHEMA IF EXISTS ... CASCADE`.
    ///
    /// Correct for schemas that cannot be safely reused (built by replaying
    /// migrations, or created empty and shaped by the test itself).
    pub fn drop_only(database_url: &str, schema_name: &str) -> Self {
        Self {
            on_release: leased_drop_schema_cleanup(database_url, schema_name),
            // The exit path is deliberately **not** lease-guarded: process
            // teardown is the last chance to reclaim, and a lease still held by
            // a connection that is about to die must not turn into a leak. The
            // pre-existing unconditional-drop behaviour is kept here.
            on_exit: drop_schema_cleanup(database_url, schema_name),
        }
    }

    /// `on_release` is caller-provided (e.g. TRUNCATE + return to reuse
    /// pool); the exit path always DROPs the schema.
    ///
    /// The caller's `on_release` is responsible for honouring the connection
    /// lease itself — [`drop_schema_if_unleased_blocking`] is the shared
    /// implementation — because the action is arbitrary and must run on the
    /// same session that took the lease.
    pub fn release_or_exit_drop(database_url: &str, schema_name: &str, on_release: CleanupFn) -> Self {
        Self { on_release, on_exit: drop_schema_cleanup(database_url, schema_name) }
    }
}

/// Advisory-lock key of the connection lease for `schema`.
///
/// Every connection of an isolated per-test pool takes this session-level
/// advisory lock — **shared** (`pg_advisory_lock_shared`) — in its
/// `after_connect` hook, so the lock is held exactly while at least one such
/// connection is open and is released by PostgreSQL when that connection closes.
/// The janitor's drop path takes the *exclusive* counterpart with
/// `pg_try_advisory_lock`, which conflicts with every shared holder: it succeeds
/// only when no connection of the pool is open. Shared rather than exclusive on
/// the fixture side is load-bearing — an exclusive lock would make a pool's
/// second concurrent connection block in `after_connect` until the first closed,
/// silently pinning every isolated pool to a single connection.
///
/// This is the **single** definition of the key: the fixtures and the janitor
/// both call it, so a fixture cannot publish a lease the janitor does not
/// respect (a second derivation would be the same class of bug as the two
/// registries this module replaced).
///
/// The key is a 64-bit FNV-1a hash of the schema name, which is unique per test
/// (`test_<uuid>` / `test_<pid>_<n>_<nanos>`), so two live schemas cannot
/// collide in practice.
pub fn schema_lease_key(schema: &str) -> i64 {
    let mut hash = 0xcbf2_9ce4_8422_2325u64;
    for byte in schema.as_bytes() {
        hash ^= u64::from(*byte);
        hash = hash.wrapping_mul(0x0000_0100_0000_01b3);
    }
    hash as i64
}

/// Try to take `schema`'s connection lease **exclusively** on `conn`.
///
/// `Ok(true)` means `conn` now holds the exclusive lease — no pool connection is
/// open — and must release it with [`release_schema_lease`] (or by closing).
/// `Ok(false)` means at least one connection still holds the shared lease, i.e.
/// a still-open isolated pool, so the schema is still in use.
pub async fn try_acquire_schema_lease(conn: &mut sqlx::PgConnection, schema: &str) -> Result<bool, sqlx::Error> {
    sqlx::query_scalar("SELECT pg_try_advisory_lock($1)").bind(schema_lease_key(schema)).fetch_one(conn).await
}

/// Release the lease taken by [`try_acquire_schema_lease`] on the same
/// connection.
pub async fn release_schema_lease(conn: &mut sqlx::PgConnection, schema: &str) -> Result<(), sqlx::Error> {
    sqlx::query("SELECT pg_advisory_unlock($1)").bind(schema_lease_key(schema)).execute(conn).await.map(|_| ())
}

/// Builds a cleanup closure that synchronously runs
/// `DROP SCHEMA IF EXISTS ... CASCADE` on a fresh single connection.
///
/// Unconditional: this is the at-exit variant. The release path uses
/// [`leased_drop_schema_cleanup`].
pub fn drop_schema_cleanup(database_url: &str, schema_name: &str) -> CleanupFn {
    let database_url = database_url.to_string();
    let schema_name = schema_name.to_string();
    Box::new(move || {
        drop_schema_blocking(&database_url, &schema_name);
        CleanupOutcome::Done
    })
}

/// Builds a cleanup closure that DROPs `schema_name` only while its connection
/// lease is free, reporting [`CleanupOutcome::Retry`] otherwise.
fn leased_drop_schema_cleanup(database_url: &str, schema_name: &str) -> CleanupFn {
    let database_url = database_url.to_string();
    let schema_name = schema_name.to_string();
    Box::new(move || drop_schema_if_unleased_blocking(&database_url, &schema_name))
}

/// Returns true when the current test process is driven by cargo-nextest.
///
/// Nextest sets `NEXTEST=1` in every test process and runs exactly one test
/// case per process, so mid-process schema reuse can never pay off there and
/// every cleanup should be a plain DROP.
pub fn running_under_nextest() -> bool {
    nextest_marker_present(std::env::var_os("NEXTEST").as_deref())
}

/// The decision behind [`running_under_nextest`], with the environment value
/// injected.
///
/// Split out for testability: the previous test was
/// `if NEXTEST.is_none() { assert!(!running_under_nextest()) }`, whose body is
/// **skipped under nextest** — i.e. in CI, where nextest is what runs the suite
/// (sweep B15), so the guard asserted nothing where it mattered. With the value
/// injected the assertion runs in both environments and is deterministic (no
/// process-env mutation, which would race the other tests in this binary).
///
/// An empty value counts as present: nextest always writes `1`, and treating
/// `NEXTEST=""` as unset would silently flip the cleanup strategy back to
/// TRUNCATE-and-reuse in a one-test-per-process run.
fn nextest_marker_present(value: Option<&std::ffi::OsStr>) -> bool {
    value.is_some()
}

/// Synchronously connect and `DROP SCHEMA IF EXISTS "{schema_name}" CASCADE`.
///
/// Safe to call from any thread; builds its own current-thread runtime.
/// Never panics: failures are reported on stderr, leaving at most one
/// orphaned schema behind for `scripts/cleanup_test_schemas.sh`.
pub fn drop_schema_blocking(database_url: &str, schema_name: &str) {
    let database_url = database_url.to_string();
    let schema_name = schema_name.to_string();
    run_cleanup_blocking(async move {
        let pool = match PgPoolOptions::new()
            .max_connections(1)
            .acquire_timeout(CLEANUP_CONNECT_TIMEOUT)
            .connect(&database_url)
            .await
        {
            Ok(pool) => pool,
            Err(error) => {
                eprintln!("test schema cleanup: could not connect to drop {schema_name}: {error}");
                return;
            }
        };
        let drop_sql = format!(r#"DROP SCHEMA IF EXISTS "{schema_name}" CASCADE"#);
        if let Err(error) = sqlx::query(&drop_sql).execute(&pool).await {
            eprintln!("test schema cleanup: failed to drop {schema_name}: {error}");
        }
        pool.close().await;
    });
}

/// DROP `schema_name` only while this process can take its connection lease.
///
/// The lease is the **shared** one every connection of the isolated pool holds
/// (see [`schema_lease_key`]); this takes the exclusive counterpart, which
/// conflicts with all of them, so [`CleanupOutcome::Retry`] means "at least one
/// pool connection is still open, the schema is still in use, do not drop". The
/// lock is taken and held on the same connection that runs the `DROP`, so a pool
/// connection opening later blocks in its `after_connect` until the drop (and
/// the unlock) have finished.
///
/// Connection and DDL failures report on stderr and return
/// [`CleanupOutcome::Done`] — exactly like [`drop_schema_blocking`], leaving at
/// most one orphan to `scripts/cleanup_test_schemas.sh` — so a database outage
/// cannot turn the janitor into a hot retry loop.
pub fn drop_schema_if_unleased_blocking(database_url: &str, schema_name: &str) -> CleanupOutcome {
    let database_url = database_url.to_string();
    let schema_name = schema_name.to_string();
    run_cleanup_blocking_outcome(async move {
        let pool = match PgPoolOptions::new()
            .max_connections(1)
            .acquire_timeout(CLEANUP_CONNECT_TIMEOUT)
            .connect(&database_url)
            .await
        {
            Ok(pool) => pool,
            Err(error) => {
                eprintln!("test schema cleanup: could not connect to drop {schema_name}: {error}");
                return CleanupOutcome::Done;
            }
        };
        let mut conn = match pool.acquire().await {
            Ok(conn) => conn,
            Err(error) => {
                eprintln!("test schema cleanup: could not acquire a connection to drop {schema_name}: {error}");
                return CleanupOutcome::Done;
            }
        };
        match try_acquire_schema_lease(&mut conn, &schema_name).await {
            Ok(true) => {}
            Ok(false) => {
                // A connection of the isolated pool is still open, so the test
                // that owns it has not finished with the schema.
                return CleanupOutcome::Retry;
            }
            Err(error) => {
                eprintln!("test schema cleanup: could not take the schema lease for {schema_name}: {error}");
                return CleanupOutcome::Done;
            }
        }
        let drop_sql = format!(r#"DROP SCHEMA IF EXISTS "{schema_name}" CASCADE"#);
        if let Err(error) = sqlx::query(&drop_sql).execute(&mut *conn).await {
            eprintln!("test schema cleanup: failed to drop {schema_name}: {error}");
        }
        if let Err(error) = release_schema_lease(&mut conn, &schema_name).await {
            eprintln!("test schema cleanup: failed to release the schema lease for {schema_name}: {error}");
        }
        drop(conn);
        pool.close().await;
        CleanupOutcome::Done
    })
}

/// Runs an async cleanup body to completion on a dedicated current-thread
/// runtime. Cleanup cannot run on the caller's runtime: the case being
/// handled is exactly "the test that owned the pool has finished and its
/// runtime is going away".
pub fn run_cleanup_blocking<F>(future: F)
where
    F: std::future::Future<Output = ()> + Send + 'static,
{
    run_cleanup_future(future, || ());
}

/// [`run_cleanup_blocking`], returning the cleanup's outcome so a lease-guarded
/// action can ask to be retried. A runtime that cannot be built reports
/// [`CleanupOutcome::Done`] (give up, as before) rather than retrying forever.
pub fn run_cleanup_blocking_outcome<F>(future: F) -> CleanupOutcome
where
    F: std::future::Future<Output = CleanupOutcome> + Send + 'static,
{
    run_cleanup_future(future, || CleanupOutcome::Done)
}

/// Drive `future` to completion on a current-thread runtime, from a thread that
/// may itself be inside a Tokio runtime.
///
/// `Runtime::block_on` **panics** when the calling thread is already driving
/// tasks ("Cannot start a runtime from within a runtime. …"), and a test that
/// drives a release pass from its own `#[tokio::test]` runtime is exactly that
/// case: the pass collects every released entry in the process, so it can land
/// on a real, DB-backed cleanup. Before this split that panic was swallowed by
/// the janitor's `catch_unwind`, which consumed the registry entry and left the
/// schema behind.
///
/// So when a runtime is already entered the cleanup is moved to a plain thread
/// and joined; otherwise the dedicated runtime is built inline, with no extra
/// thread (the janitor's own thread, which is the hot path, takes this branch).
fn run_cleanup_future<F, T>(future: F, on_failure: fn() -> T) -> T
where
    F: std::future::Future<Output = T> + Send + 'static,
    T: Send + 'static,
{
    let run_inline = move || match tokio::runtime::Builder::new_current_thread().enable_all().build() {
        Ok(runtime) => runtime.block_on(future),
        Err(error) => {
            eprintln!("test schema cleanup: failed to build runtime: {error}");
            on_failure()
        }
    };
    if tokio::runtime::Handle::try_current().is_ok() {
        std::thread::spawn(run_inline).join().unwrap_or_else(|_| {
            eprintln!("test schema cleanup: cleanup thread panicked");
            on_failure()
        })
    } else {
        run_inline()
    }
}

/// A registered schema whose cleanup fires once the owning pool is released.
struct PendingCleanup {
    weak: Weak<PgPool>,
    schema_name: String,
    cleanup: Option<SchemaCleanup>,
    /// Earliest time a retried entry may be attempted again. Set to `now` at
    /// registration; pushed forward by [`LEASE_RETRY_BACKOFF`] on every
    /// [`CleanupOutcome::Retry`] so a long-leased schema cannot make the janitor
    /// open a cleanup connection on every poll.
    next_attempt: Instant,
}

static PENDING: LazyLock<Mutex<Vec<PendingCleanup>>> = LazyLock::new(|| Mutex::new(Vec::new()));
static EXIT_CALLBACKS: LazyLock<Mutex<Vec<CleanupFn>>> = LazyLock::new(|| Mutex::new(Vec::new()));
static EXITING: AtomicBool = AtomicBool::new(false);
static JANITOR_START: Once = Once::new();
static JANITOR_HANDLE: LazyLock<Mutex<Option<std::thread::JoinHandle<()>>>> = LazyLock::new(|| Mutex::new(None));

/// Locks a mutex, recovering from poisoning (a panicking cleanup must not
/// take the whole registry down with it).
fn lock_mutex<T>(mutex: &Mutex<T>) -> MutexGuard<'_, T> {
    mutex.lock().unwrap_or_else(|poisoned| poisoned.into_inner())
}

/// Registers `schema_name` so its cleanup runs once every `Arc<PgPool>`
/// clone of `pool` has been released (or, at the latest, at process exit).
///
/// "Every clone" is still the trigger, but not the whole decision: the release
/// cleanup is lease-guarded, so a pool kept alive through an *inner* `PgPool`
/// clone (`(**pool).clone()`) does not let the janitor drop a schema a live
/// connection is still using — the entry is retried instead.
///
/// This is the single entry point used by every test fixture in the
/// workspace; fixtures must NOT keep their own pending-drop registries
/// (that fragmentation is what made the old mechanism unfixable).
pub fn register_schema_cleanup(pool: &Arc<PgPool>, schema_name: &str, cleanup: SchemaCleanup) {
    if EXITING.load(Ordering::SeqCst) {
        // The process is already tearing down; run the exit cleanup inline.
        let SchemaCleanup { on_exit, .. } = cleanup;
        on_exit();
        return;
    }
    ensure_janitor_started();
    lock_mutex(&PENDING).push(PendingCleanup {
        weak: Arc::downgrade(pool),
        schema_name: schema_name.to_string(),
        cleanup: Some(cleanup),
        next_attempt: Instant::now(),
    });
}

/// Registers a callback to run when the process exits (after all pending
/// schema cleanups). Used by fixtures that keep reusable schema *names* in a
/// process-local pool: those names have no live `PgPool` to watch, so they
/// are dropped via this hook instead.
pub fn register_exit_callback(callback: CleanupFn) {
    if EXITING.load(Ordering::SeqCst) {
        callback();
        return;
    }
    ensure_janitor_started();
    lock_mutex(&EXIT_CALLBACKS).push(callback);
}

fn ensure_janitor_started() {
    JANITOR_START.call_once(|| {
        let spawned = std::thread::Builder::new().name("test-schema-janitor".to_string()).spawn(janitor_loop);
        match spawned {
            Ok(handle) => {
                *lock_mutex(&JANITOR_HANDLE) = Some(handle);
                // Safety: `janitor_exit_handler` is a plain `extern "C" fn`
                // with no captured state; it only flips an atomic and joins
                // the janitor thread, both sound at process-exit time.
                unsafe { libc::atexit(janitor_exit_handler) };
            }
            Err(error) => {
                eprintln!(
                    "test schema janitor: failed to spawn thread: {error}; schemas created by this process will leak"
                );
            }
        }
    });
}

extern "C" fn janitor_exit_handler() {
    EXITING.store(true, Ordering::SeqCst);
    let handle = lock_mutex(&JANITOR_HANDLE).take();
    if let Some(handle) = handle {
        // Bounded wait — see [`JANITOR_EXIT_JOIN_TIMEOUT`]. Returning without
        // joining detaches the thread, which the process teardown then kills.
        let deadline = Instant::now() + JANITOR_EXIT_JOIN_TIMEOUT;
        while !handle.is_finished() {
            if Instant::now() >= deadline {
                eprintln!(
                    "test schema janitor: still busy after {JANITOR_EXIT_JOIN_TIMEOUT:?}; exiting \
                     anyway. Schemas it had not dropped are left to scripts/cleanup_test_schemas.sh."
                );
                return;
            }
            std::thread::sleep(JANITOR_POLL_INTERVAL);
        }
        let _ = handle.join();
    }
}

fn janitor_loop() {
    loop {
        let exiting = EXITING.load(Ordering::SeqCst);
        let ready = collect_released_entries(exiting);

        if exiting {
            // Final drain: the process is exiting, so reuse-oriented cleanups
            // are pointless — run the cheap exit variant of every remaining
            // entry in parallel, then the registered exit callbacks, and let
            // the thread finish so the atexit handler's join returns.
            run_exit_drain(ready);
            let callbacks: Vec<CleanupFn> = lock_mutex(&EXIT_CALLBACKS).drain(..).collect();
            for callback in callbacks {
                let _ = catch_unwind(AssertUnwindSafe(callback));
            }
            return;
        }

        run_release_cleanups(ready);
        std::thread::sleep(JANITOR_POLL_INTERVAL);
    }
}

/// Collect every registered entry whose pool has been released (`exiting` means
/// "all of them", for the final drain).
///
/// Split from [`run_release_cleanups`] so one pass can be driven synchronously
/// from a test. The single background thread can be busy for seconds with other
/// suites' `DROP SCHEMA` work, so asserting "my callback ran within N seconds"
/// measures the shared worker's queue depth, not the behaviour under test
/// (that is what made `released_pool_triggers_cleanup_without_any_sweep` fail in
/// the full suite while passing in isolation).
///
/// An entry that reported [`CleanupOutcome::Retry`] is back in the registry and
/// is skipped until its [`PendingCleanup::next_attempt`] deadline, so a
/// long-leased schema costs one attempt per [`LEASE_RETRY_BACKOFF`] instead of
/// one per poll.
fn collect_released_entries(exiting: bool) -> Vec<PendingCleanup> {
    let now = Instant::now();
    let mut ready = Vec::new();
    let mut pending = lock_mutex(&PENDING);
    let mut i = 0;
    while i < pending.len() {
        let due = exiting || (pending[i].weak.upgrade().is_none() && pending[i].next_attempt <= now);
        if due {
            ready.push(pending.swap_remove(i));
        } else {
            i += 1;
        }
    }
    ready
}

/// Run each collected entry's `on_release` cleanup (panics are caught and
/// reported, never propagated into the janitor thread).
///
/// **Exit hand-off**: the entries here were collected *before* `EXITING` was
/// set, so without the check below the whole batch would be processed serially,
/// one `DROP SCHEMA … CASCADE` at a time, while the `atexit` handler blocks in
/// `join()`. Measured 2026-09-19 on a full integration run: 377 leftover schemas
/// turned process shutdown into >19 minutes of serial drops (sampled stack:
/// `exit → janitor_exit_handler → JoinHandle::join` blocked, janitor inside
/// `run_release_cleanups`). Once the process is exiting, reuse-oriented
/// `on_release` work is pointless anyway, so hand the current entry **and the
/// rest** to the bounded-parallel exit drain instead.
fn run_release_cleanups(ready: Vec<PendingCleanup>) {
    run_release_cleanups_with(ready, || EXITING.load(Ordering::SeqCst));
}

/// [`run_release_cleanups`] with the "is the process exiting?" decision injected.
///
/// Split out so a test can flip the answer **without** touching the
/// process-global `EXITING` flag: setting that flag in one test would silently
/// change the behaviour of every other test sharing the binary (registrations
/// would take the inline `on_exit` path). This mirrors the existing split of
/// [`collect_released_entries`] from the janitor loop.
///
/// **Retry**: an entry whose `on_release` reports [`CleanupOutcome::Retry`]
/// (the schema's connection lease was still held) is pushed back into
/// [`PENDING`] with its next attempt delayed by [`LEASE_RETRY_BACKOFF`]. Its
/// `Weak` is dead by then, so the next pass re-collects it; the lease check is
/// what decides when it is actually free. At exit the retry is pointless — the
/// exit drain runs the unconditional `on_exit` variant — so a `Retry` in the
/// exit path is simply ignored.
fn run_release_cleanups_with(ready: Vec<PendingCleanup>, is_exiting: impl Fn() -> bool) {
    run_release_cleanups_requeueing(ready, is_exiting, |entry| lock_mutex(&PENDING).push(entry));
}

/// [`run_release_cleanups_with`] with the retry sink injected.
///
/// Split out so a test can observe the re-registration of a
/// [`CleanupOutcome::Retry`] entry without touching the process-global registry
/// — and therefore without racing the background janitor, which could otherwise
/// claim the entry under test.
fn run_release_cleanups_requeueing(
    ready: Vec<PendingCleanup>,
    is_exiting: impl Fn() -> bool,
    mut requeue: impl FnMut(PendingCleanup),
) {
    let mut iter = ready.into_iter();
    while let Some(mut entry) = iter.next() {
        if is_exiting() {
            let mut rest = vec![entry];
            rest.extend(iter);
            run_exit_drain(rest);
            // Exit is a one-way door: the entries handed to the drain run their
            // `on_exit` cleanup.
            return;
        }
        let Some(cleanup) = entry.cleanup.take() else { continue };
        match catch_unwind(AssertUnwindSafe(|| (cleanup.on_release)())) {
            Ok(CleanupOutcome::Done) => {}
            Ok(CleanupOutcome::Retry) => {
                // Re-register **immediately**, not at the end of the batch: this
                // entry's own `on_release` has already finished, and a long
                // batch (another entry's slow connect, a big TRUNCATE) must not
                // make a retried entry invisible to other passes. Another pass
                // can then legitimately claim it — `collect_released_entries`
                // `swap_remove`s under the lock, so it is still run once.
                entry.cleanup = Some(cleanup);
                entry.next_attempt = Instant::now() + LEASE_RETRY_BACKOFF;
                requeue(entry);
            }
            Err(_) => eprintln!("test schema janitor: cleanup for {} panicked", entry.schema_name),
        }
    }
}

/// Runs the exit cleanup of every entry, bounded-parallel so a large
/// backlog cannot stall process shutdown.
fn run_exit_drain(entries: Vec<PendingCleanup>) {
    if entries.is_empty() {
        return;
    }
    let worker_count = EXIT_DRAIN_WORKERS.min(entries.len());
    let queue = Arc::new(Mutex::new(entries));
    let mut handles = Vec::new();
    for _ in 0..worker_count {
        let queue = Arc::clone(&queue);
        let spawned = std::thread::Builder::new().name("test-schema-exit-drain".to_string()).spawn(move || loop {
            let entry = lock_mutex(&queue).pop();
            let Some(mut entry) = entry else { return };
            if let Some(cleanup) = entry.cleanup.take() {
                let on_exit = cleanup.on_exit;
                let _ = catch_unwind(AssertUnwindSafe(on_exit));
            }
        });
        match spawned {
            Ok(handle) => handles.push(handle),
            Err(error) => {
                eprintln!("test schema janitor: failed to spawn exit-drain worker: {error}");
                break;
            }
        }
    }
    for handle in handles {
        let _ = handle.join();
    }
    // Any entries left behind by a spawn failure are finished inline.
    let mut queue = lock_mutex(&queue);
    for entry in queue.drain(..) {
        if let Some(cleanup) = entry.cleanup {
            let on_exit = cleanup.on_exit;
            let _ = catch_unwind(AssertUnwindSafe(on_exit));
        }
    }
}

/// RAII-style handle for a per-test database schema.
///
/// Holding the guard documents *ownership* of the schema for the duration of
/// a test. Cleanup is registered with the process-wide janitor at creation
/// time and runs once the **last** `Arc<PgPool>` clone is released, so
/// dropping the guard handle early is safe as long as a pool clone is alive
/// (conversely, keeping the guard but dropping every pool clone reclaims the
/// schema immediately).
pub struct TestSchemaGuard {
    pool: Arc<PgPool>,
    schema_name: String,
}

impl TestSchemaGuard {
    /// Registers `cleanup` for `schema_name` and returns the guard. Called by
    /// the `prepare_*_test_pool` fixtures after the schema and pool exist.
    pub fn new_registered(pool: Arc<PgPool>, schema_name: String, cleanup: SchemaCleanup) -> Self {
        register_schema_cleanup(&pool, &schema_name, cleanup);
        Self { pool, schema_name }
    }

    /// Returns a clone of the underlying pool.
    #[must_use]
    pub fn pool(&self) -> Arc<PgPool> {
        Arc::clone(&self.pool)
    }

    /// Returns the schema name, for debugging and assertions.
    #[must_use]
    pub fn schema_name(&self) -> &str {
        &self.schema_name
    }
}

impl Deref for TestSchemaGuard {
    type Target = PgPool;

    fn deref(&self) -> &PgPool {
        &self.pool
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::mpsc;

    fn lazy_pool() -> Arc<PgPool> {
        // Lazy pools never touch the network; the janitor only watches the
        // strong count, so an unreachable URL is fine for lifecycle tests.
        PgPoolOptions::new()
            .max_connections(1)
            .connect_lazy("postgres://synapse:synapse@127.0.0.1:1/synapse_test")
            .map(Arc::new)
            .expect("lazy pool")
    }

    /// Test database URL for the lease proof, or `None` for an *explicit* skip.
    ///
    /// A DB-backed guard that silently returns when the environment is unset
    /// reports `ok` in 0.00s and proves nothing, so an unset environment is a
    /// panic unless the operator opts out loudly with `ALLOW_SKIP_TEST_DB=1`
    /// (the same rule the sibling `test_isolation` tests use).
    fn lease_test_db_url() -> Option<String> {
        match std::env::var("TEST_DATABASE_URL").or_else(|_| std::env::var("DATABASE_URL")) {
            Ok(url) => Some(url),
            Err(_) if std::env::var("ALLOW_SKIP_TEST_DB").ok().as_deref() == Some("1") => {
                eprintln!(
                    "SKIPPING the janitor lease test: TEST_DATABASE_URL/DATABASE_URL is unset and \
                     ALLOW_SKIP_TEST_DB=1. This test proves NOTHING without a database."
                );
                None
            }
            Err(_) => panic!(
                "TEST_DATABASE_URL (or DATABASE_URL) is not set. Point it at a throwaway Postgres \
                 database, or set ALLOW_SKIP_TEST_DB=1 to skip the lease test explicitly."
            ),
        }
    }

    #[tokio::test]
    async fn released_pool_triggers_cleanup_without_any_sweep() {
        let pool = lazy_pool();
        let (tx, rx) = mpsc::channel();
        register_schema_cleanup(
            &pool,
            "test_janitor_lifecycle",
            SchemaCleanup {
                on_release: Box::new(move || {
                    let _ = tx.send(());
                    CleanupOutcome::Done
                }),
                on_exit: Box::new(|| CleanupOutcome::Done),
            },
        );
        drop(pool);
        // Drive one release pass on this thread instead of waiting on the shared
        // janitor thread: the assertion is about "a released pool's on_release
        // runs", not about how quickly a single background worker gets to it
        // (other suites' real DROP SCHEMA work can occupy it for seconds).
        // The background pass remains covered by the loop that calls the same
        // two functions; this is deterministic either way, because whichever
        // pass claims the entry first runs the callback exactly once
        // (`cleanup.take()`).
        run_release_cleanups(collect_released_entries(false));
        rx.try_recv().expect("on_release must run once the pool is released");
    }

    /// A lease-guarded cleanup that cannot take the lease must be **re-queued**,
    /// not dropped: the registry entry has to survive so a later pass can retry
    /// it. This is the half of the media flake fix that a one-shot cleanup
    /// cannot express.
    ///
    /// Synthetic and registry-free: the retry sink is injected, so the
    /// background janitor cannot claim the entry between registration and
    /// assertion (which is what made an earlier version of this test flaky).
    #[test]
    fn a_retry_outcome_is_reregistered_with_a_delayed_next_attempt() {
        use std::sync::atomic::AtomicUsize;

        let attempts = Arc::new(AtomicUsize::new(0));
        let attempts_in_cleanup = Arc::clone(&attempts);
        let entry = PendingCleanup {
            weak: Weak::<PgPool>::new(),
            schema_name: "test_janitor_retry_synthetic".to_string(),
            cleanup: Some(SchemaCleanup {
                on_release: Box::new(move || {
                    attempts_in_cleanup.fetch_add(1, Ordering::SeqCst);
                    CleanupOutcome::Retry
                }),
                on_exit: Box::new(|| CleanupOutcome::Done),
            }),
            next_attempt: Instant::now(),
        };

        let requeued = Arc::new(Mutex::new(Vec::new()));
        let sink = Arc::clone(&requeued);
        let before = Instant::now();
        run_release_cleanups_requeueing(vec![entry], || false, move |entry| lock_mutex(&sink).push(entry));

        assert_eq!(attempts.load(Ordering::SeqCst), 1, "the release action must run once this pass");
        let requeued = lock_mutex(&requeued);
        assert_eq!(requeued.len(), 1, "a Retry outcome must be re-registered, not dropped");
        let entry = &requeued[0];
        assert_eq!(entry.schema_name, "test_janitor_retry_synthetic");
        assert!(entry.cleanup.is_some(), "the retried entry must keep its cleanup for the next pass");
        assert!(
            entry.next_attempt >= before + LEASE_RETRY_BACKOFF,
            "the retried entry must be delayed by the lease-retry backoff, got {:?} after {:?}",
            entry.next_attempt.duration_since(before),
            before
        );
    }

    #[tokio::test]
    async fn guard_exposes_pool_and_schema_name() {
        let pool = lazy_pool();
        // Cleanup is a no-op on purpose: this test asserts the handle's
        // accessors, and a real `DROP` against the lazy port-1 pool would make
        // the shared janitor — and every driven release pass in this binary —
        // retry a refused connection until the 10s acquire timeout, a
        // suite-wide stall for no coverage gain.
        let guard = TestSchemaGuard::new_registered(
            pool,
            "test_guard_handle".to_string(),
            SchemaCleanup { on_release: Box::new(|| CleanupOutcome::Done), on_exit: Box::new(|| CleanupOutcome::Done) },
        );
        assert_eq!(guard.schema_name(), "test_guard_handle");
        let pool = guard.pool();
        // Deref reaches the inner PgPool.
        let _opts: &PgPool = &guard;
        drop(pool);
        drop(guard);
    }

    /// Run one release pass on a blocking thread.
    ///
    /// The pass runs the real lease-guarded drop, which builds its own
    /// current-thread runtime; that cannot happen on this test's runtime thread
    /// ("Cannot start a runtime from within a runtime"), so it goes through
    /// `spawn_blocking`. Driving the pass here instead of only waiting on the
    /// shared janitor keeps the assertion about the lease decision, not about
    /// how deep the janitor's queue is (the reason
    /// `released_pool_triggers_cleanup_without_any_sweep` drives its own pass).
    async fn drive_one_release_pass() {
        tokio::task::spawn_blocking(|| run_release_cleanups(collect_released_entries(false)))
            .await
            .expect("release-pass task");
    }

    /// [`drop_schema_if_unleased_blocking`] on a blocking thread (it builds its
    /// own runtime, which is illegal on this test's runtime thread).
    async fn drop_if_unleased(url: String, schema: String) -> CleanupOutcome {
        tokio::task::spawn_blocking(move || drop_schema_if_unleased_blocking(&url, &schema))
            .await
            .expect("drop-if-unleased task")
    }

    /// The lease must not serialize the pool it guards.
    ///
    /// The first cut of this fix took an **exclusive** session lock in
    /// `after_connect`, so a pool's second concurrent connection blocked there
    /// until the first closed: a multi-connection isolated pool (the services
    /// fixture defaults to 64) silently degenerated to one connection and its
    /// queries hit the acquire timeout — measured as a 608s media-fixture run.
    /// This pins the shared/exclusive split: several connections are open at
    /// once, and the janitor's exclusive try still fails while any of them is.
    #[tokio::test]
    async fn the_lease_allows_several_connections_and_still_blocks_the_janitor() {
        let Some(url) = lease_test_db_url() else { return };

        let schema = format!("test_lease_shared_{}", uuid::Uuid::new_v4().as_simple());
        let admin = PgPoolOptions::new()
            .max_connections(1)
            .acquire_timeout(Duration::from_secs(10))
            .connect(&url)
            .await
            .expect("admin pool");
        sqlx::query(&format!(r#"CREATE SCHEMA "{schema}""#)).execute(&admin).await.expect("create schema");

        let hook_schema = schema.clone();
        let pool = PgPoolOptions::new()
            .max_connections(2)
            .acquire_timeout(Duration::from_secs(10))
            .after_connect(move |conn, _| {
                let schema = hook_schema.clone();
                Box::pin(async move {
                    sqlx::query("SELECT pg_advisory_lock_shared($1)")
                        .bind(schema_lease_key(&schema))
                        .execute(&mut *conn)
                        .await?;
                    Ok(())
                })
            })
            .connect(&url)
            .await
            .expect("two-connection isolated pool");

        // Both queries must run concurrently (each holds its connection for
        // ~0.2s). With an exclusive lease the second connection blocks in
        // `after_connect` and this fails with a pool acquire timeout instead.
        let (first, second) = tokio::join!(
            sqlx::query_scalar::<_, i32>("SELECT 1 FROM pg_sleep(0.2)").fetch_one(&pool),
            sqlx::query_scalar::<_, i32>("SELECT 2 FROM pg_sleep(0.2)").fetch_one(&pool),
        );
        assert_eq!(first.expect("first connection"), 1);
        assert_eq!(second.expect("second connection"), 2);

        assert_eq!(
            drop_if_unleased(url.clone(), schema.clone()).await,
            CleanupOutcome::Retry,
            "the janitor must not be able to take the lease while a pool connection is open"
        );

        pool.close().await;
        assert_eq!(
            drop_if_unleased(url.clone(), schema.clone()).await,
            CleanupOutcome::Done,
            "the drop must succeed once the pool closed its connections"
        );
        let exists: bool = sqlx::query_scalar("SELECT to_regnamespace($1) IS NOT NULL")
            .bind(&schema)
            .fetch_one(&admin)
            .await
            .expect("probe schema");
        assert!(!exists, "the schema must be gone after the lease-guarded drop");
        admin.close().await;
    }

    /// **RED PROOF for the §1.9.1 media flake.** The janitor decides a schema
    /// is free from `Weak<Arc<PgPool>>`, but a service that stored an *inner*
    /// `PgPool` clone (`(**pool).clone()`) keeps the pool — and its connections
    /// — alive after the fixture drops its `Arc<PgPool>`. Before the fix the
    /// janitor then dropped the schema mid-test and unqualified SQL silently
    /// resolved to the shared `public` schema (probe: `dropped_while_in_use=true
    /// current_schema=public`).
    ///
    /// The lease is connection-derived, so the drop must wait for the last
    /// connection to close, and must still happen afterwards (the no-leak half).
    #[tokio::test]
    async fn janitor_does_not_drop_a_schema_held_through_an_inner_pool_clone() {
        let Some(url) = lease_test_db_url() else { return };

        // A tiny baseline keeps the proof fast; the schema lifecycle under test
        // does not depend on the workspace baseline.
        let isolated = crate::test_isolation::IsolatedTestPool::new("CREATE TABLE lease_probe (id integer);")
            .await
            .expect("isolated pool");
        let schema = isolated.schema_name().to_string();

        // What a service does: keep only an inner `PgPool` clone. It is a
        // different `Arc` from the one the janitor's `Weak` points at.
        let inner: PgPool = (*isolated.pool()).clone();
        let current: String = sqlx::query_scalar("SELECT current_schema()")
            .fetch_one(&inner)
            .await
            .expect("the inner clone must be able to query");
        assert_eq!(current, schema, "the inner clone must resolve into the isolated schema");
        let probe: bool =
            sqlx::query_scalar("SELECT to_regclass('lease_probe') IS NOT NULL").fetch_one(&inner).await.expect("probe");
        assert!(probe, "the cloned schema must contain the baseline's `lease_probe` table");

        // The fixture's outer `Arc<PgPool>` is gone. Before the fix this alone
        // made the janitor drop the schema while `inner` was still querying it.
        drop(isolated);

        // Several janitor poll intervals, so the *background* janitor really does
        // get its chance at the released entry — before the fix this is where the
        // schema disappeared.
        tokio::time::sleep(Duration::from_millis(400)).await;

        let still_there: bool = sqlx::query_scalar("SELECT to_regnamespace($1) IS NOT NULL")
            .bind(&schema)
            .fetch_one(&inner)
            .await
            .expect("probe whether the schema still exists");
        let current: String = sqlx::query_scalar("SELECT current_schema()")
            .fetch_one(&inner)
            .await
            .expect("the inner clone must still be able to query");
        assert!(
            still_there,
            "the janitor dropped {schema} while an inner `PgPool` clone was still open: \
             to_regnamespace={still_there} current_schema={current}"
        );
        assert_eq!(
            current, schema,
            "unqualified SQL from the still-live inner clone fell back to `{current}` instead of \
             `{schema}` — the schema it points at was dropped"
        );

        // Drive a release pass directly as well: the entry must come back as a
        // retry (its lease is still held), which is what keeps it registered
        // instead of being dropped now.
        drive_one_release_pass().await;
        let still_there: bool = sqlx::query_scalar("SELECT to_regnamespace($1) IS NOT NULL")
            .bind(&schema)
            .fetch_one(&inner)
            .await
            .expect("probe after the driven release pass");
        assert!(still_there, "a release pass dropped {schema} even though a connection of its pool was still open");

        // NO-LEAK HALF: once the last inner clone is gone the pool closes its
        // connections, the connection lease is released, and the janitor must
        // drop the schema. Each iteration waits out the retry backoff and then
        // drives a pass, so progress does not depend on the shared janitor
        // thread's queue depth.
        drop(inner);
        let observer = PgPoolOptions::new()
            .max_connections(1)
            .acquire_timeout(Duration::from_secs(10))
            .connect(&url)
            .await
            .expect("observer pool");
        let deadline = Instant::now() + Duration::from_secs(30);
        loop {
            let exists: bool = sqlx::query_scalar("SELECT to_regnamespace($1) IS NOT NULL")
                .bind(&schema)
                .fetch_one(&observer)
                .await
                .expect("probe via the observer");
            if !exists {
                break;
            }
            if Instant::now() >= deadline {
                // Diagnostic: distinguish "the entry is lost / not retried" from
                // "the lease is still held" in the failure message.
                let direct = drop_if_unleased(url.clone(), schema.clone()).await;
                let still_registered = lock_mutex(&PENDING).iter().any(|e| e.schema_name == schema);
                observer.close().await;
                panic!(
                    "the janitor never dropped {schema} after the last inner `PgPool` clone was \
                     dropped: direct lease-guarded drop -> {direct:?}, entry still registered: \
                     {still_registered}"
                );
            }
            tokio::time::sleep(Duration::from_millis(300)).await;
            drive_one_release_pass().await;
        }
        observer.close().await;
    }

    #[test]
    fn nextest_detection_follows_env() {
        // Assertions on an injected value, so this runs identically under
        // `cargo test` and under nextest (the previous body was skipped under
        // nextest — i.e. precisely in the environment it was about; sweep B15).
        assert!(!nextest_marker_present(None), "unset NEXTEST must mean 'not nextest'");
        assert!(
            nextest_marker_present(Some(std::ffi::OsStr::new("1"))),
            "nextest exports NEXTEST=1 in every test process"
        );
        assert!(
            nextest_marker_present(Some(std::ffi::OsStr::new(""))),
            "an empty NEXTEST still means 'set': treating it as unset would silently flip the \
             cleanup strategy (reuse vs plain DROP) in a one-test-per-process run"
        );
    }

    /// The exit hand-off: a release batch collected *before* `EXITING` was set
    /// must not finish serially once the process is exiting — the in-flight
    /// entry and everything after it go to the bounded-parallel exit drain
    /// (`on_exit`), because the `atexit` handler is blocked waiting on this
    /// thread. Measured 2026-09-19: without this, a full integration run spent
    /// >19 minutes dropping 377 schemas one at a time during `exit()`.
    ///
    /// Deterministic and global-state-free: the injected predicate flips after
    /// the first call, so entry 0 takes `on_release` and 1.. take `on_exit`.
    /// Setting the real `EXITING` flag in a test would change behaviour for
    /// every other test sharing the binary (registrations would take the inline
    /// `on_exit` path).
    #[test]
    fn release_pass_hands_the_remainder_to_the_exit_drain_once_exiting() {
        use std::sync::atomic::AtomicUsize;

        let releases = Arc::new(Mutex::new(Vec::new()));
        let exits = Arc::new(Mutex::new(Vec::new()));

        let entries: Vec<PendingCleanup> = (0..3)
            .map(|index| {
                let releases = Arc::clone(&releases);
                let exits = Arc::clone(&exits);
                PendingCleanup {
                    // Never upgraded: these are synthetic entries, so the pool
                    // lifetime plays no part in this assertion.
                    weak: Weak::<PgPool>::new(),
                    schema_name: format!("test_janitor_exit_handoff_{index}"),
                    cleanup: Some(SchemaCleanup {
                        on_release: Box::new(move || {
                            lock_mutex(&releases).push(index);
                            CleanupOutcome::Done
                        }),
                        on_exit: Box::new(move || {
                            lock_mutex(&exits).push(index);
                            CleanupOutcome::Done
                        }),
                    }),
                    next_attempt: Instant::now(),
                }
            })
            .collect();

        let calls = AtomicUsize::new(0);
        run_release_cleanups_with(entries, || calls.fetch_add(1, Ordering::SeqCst) >= 1);

        assert_eq!(*lock_mutex(&releases), vec![0], "only the in-flight entry may take on_release");
        let mut exited = lock_mutex(&exits).clone();
        exited.sort_unstable();
        assert_eq!(exited, vec![1, 2], "the remainder must go through the exit drain (on_exit)");
    }
}
