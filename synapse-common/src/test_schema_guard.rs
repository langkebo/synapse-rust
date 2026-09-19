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

/// Cleanup closure: runs synchronously on the calling thread and must not
/// panic (panics are caught by the janitor and reported on stderr).
pub type CleanupFn = Box<dyn FnOnce() + Send + 'static>;

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
    /// Both paths run `DROP SCHEMA IF EXISTS ... CASCADE`.
    ///
    /// Correct for schemas that cannot be safely reused (built by replaying
    /// migrations, or created empty and shaped by the test itself).
    pub fn drop_only(database_url: &str, schema_name: &str) -> Self {
        Self {
            on_release: drop_schema_cleanup(database_url, schema_name),
            on_exit: drop_schema_cleanup(database_url, schema_name),
        }
    }

    /// `on_release` is caller-provided (e.g. TRUNCATE + return to reuse
    /// pool); the exit path always DROPs the schema.
    pub fn release_or_exit_drop(database_url: &str, schema_name: &str, on_release: CleanupFn) -> Self {
        Self { on_release, on_exit: drop_schema_cleanup(database_url, schema_name) }
    }
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

/// Builds a cleanup closure that synchronously runs
/// `DROP SCHEMA IF EXISTS ... CASCADE` on a fresh single connection.
pub fn drop_schema_cleanup(database_url: &str, schema_name: &str) -> CleanupFn {
    let database_url = database_url.to_string();
    let schema_name = schema_name.to_string();
    Box::new(move || drop_schema_blocking(&database_url, &schema_name))
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

/// Runs an async cleanup body to completion on a dedicated current-thread
/// runtime. Cleanup cannot run on the caller's runtime: the case being
/// handled is exactly "the test that owned the pool has finished and its
/// runtime is going away".
pub fn run_cleanup_blocking<F>(future: F)
where
    F: std::future::Future<Output = ()>,
{
    match tokio::runtime::Builder::new_current_thread().enable_all().build() {
        Ok(runtime) => runtime.block_on(future),
        Err(error) => eprintln!("test schema cleanup: failed to build runtime: {error}"),
    }
}

/// A registered schema whose cleanup fires once the owning pool is released.
struct PendingCleanup {
    weak: Weak<PgPool>,
    schema_name: String,
    cleanup: Option<SchemaCleanup>,
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
fn collect_released_entries(exiting: bool) -> Vec<PendingCleanup> {
    let mut ready = Vec::new();
    let mut pending = lock_mutex(&PENDING);
    let mut i = 0;
    while i < pending.len() {
        if exiting || pending[i].weak.upgrade().is_none() {
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
fn run_release_cleanups_with(ready: Vec<PendingCleanup>, is_exiting: impl Fn() -> bool) {
    let mut iter = ready.into_iter();
    while let Some(mut entry) = iter.next() {
        if is_exiting() {
            let mut rest = vec![entry];
            rest.extend(iter);
            run_exit_drain(rest);
            return;
        }
        if let Some(cleanup) = entry.cleanup.take() {
            let on_release = cleanup.on_release;
            if catch_unwind(AssertUnwindSafe(on_release)).is_err() {
                eprintln!("test schema janitor: cleanup for {} panicked", entry.schema_name);
            }
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
                }),
                on_exit: Box::new(|| {}),
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

    #[tokio::test]
    async fn guard_exposes_pool_and_schema_name() {
        let pool = lazy_pool();
        let guard = TestSchemaGuard::new_registered(
            pool,
            "test_guard_handle".to_string(),
            SchemaCleanup::drop_only("postgres://127.0.0.1:1/none", "test_guard_handle"),
        );
        assert_eq!(guard.schema_name(), "test_guard_handle");
        let pool = guard.pool();
        // Deref reaches the inner PgPool.
        let _opts: &PgPool = &guard;
        drop(pool);
        drop(guard);
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
                        on_release: Box::new(move || lock_mutex(&releases).push(index)),
                        on_exit: Box::new(move || lock_mutex(&exits).push(index)),
                    }),
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
