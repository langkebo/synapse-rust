//! Orphan-schema reclamation — the catalog-bloat guard for test databases.
//!
//! A per-test clone is ~1000 catalog writes. Once abandoned `test_*` schemas
//! accumulate, every one of those writes re-reads `pg_attribute` from disk:
//! measured 2026-10-19 on this machine, the same test took **95 s** on a 10 GB
//! database holding 765 residual schemas and **11 s** on a rebuilt one
//! (37 MB, zero residuals). The catalog is therefore a ~17x multiplier on the
//! whole suite, not a cosmetic housekeeping problem.
//!
//! The janitor in [`crate::test_schema_guard`] cannot be the only defence: its
//! release cleanup reports `CleanupOutcome::Retry` when a pool connection is
//! still open, and under nextest the process is already leaving, so that retry
//! never happens. Observed on a freshly rebuilt database: **one passing test
//! still left one `test_<pid>_<n>_<nanos>` schema behind**. So each process
//! sweeps a bounded number of abandoned schemas before it clones anything, and
//! refuses to run when the backlog is beyond what a sweep can recover.
//!
//! Age comes from the schema **name** — `test_<pid>_<counter>_<nanos>` embeds
//! the creation instant in nanoseconds — because PostgreSQL records no creation
//! time for a schema and `pg_class.relfilenode` mtimes are not a stable
//! contract. **The name is the manifest**: it is written by the creator, stored
//! by the catalog, survives SIGKILL, and is read back by the next run, so no
//! side file and no atexit hook is needed for a schema to be reclaimable.
//! Schemas whose names carry no timestamp (`test_<32 hex>`, the two
//! shared-template families) cannot be aged; the uuid family is counted as
//! backlog rather than quietly ignored, and [`next_stamped_schema_name`] exists
//! so no fixture creates it any more.
//!
//! The sweep never borrows a test pool: [`run_orphan_guard`] opens its own
//! single-connection pool, so a pool that is being torn down at process exit
//! cannot block the `DROP`. That is the failure this module exists to undo —
//! the janitor's release cleanup is lease-guarded, and the lease belongs to the
//! very pool that is going away.
//!
//! Env knobs:
//!
//! * `TEST_SCHEMA_LEAK_GUARD` — `deny` (default), `warn`, `off`.
//! * `TEST_SCHEMA_LEAK_BACKLOG_LIMIT` — backlog above which a run is refused
//!   (default [`DEFAULT_ORPHAN_BACKLOG_LIMIT`]).

use crate::test_schema_guard::{release_schema_lease, try_acquire_schema_lease};
use sqlx::PgPool;
use std::time::Duration;

/// Orphan age below which a schema is assumed to belong to a test that is still
/// starting: the fixture creates the schema first and takes the connection
/// lease second, so a just-created schema is briefly lease-free.
pub const ORPHAN_SCHEMA_MIN_AGE: Duration = Duration::from_secs(600);

/// Maximum number of orphaned schemas dropped per process.
///
/// `DROP SCHEMA ... CASCADE` measured 0.7–1.8 s on a 221-table schema, so an
/// unbounded sweep would stall the very run that triggers it. The backlog
/// drains over many test processes (nextest starts one per test), and
/// `scripts/cleanup_test_schemas.sh --apply` remains the bulk tool.
pub const ORPHAN_SCHEMA_DROP_BUDGET: usize = 5;

/// Backlog above which a run is refused by default. 200 is roughly two full
/// runs' worth of leaks; below it the per-process budget drains the backlog
/// faster than tests can create it.
pub const DEFAULT_ORPHAN_BACKLOG_LIMIT: usize = 200;

/// Outcome of one [`reclaim_orphan_test_schemas`] pass.
#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct OrphanReclaimReport {
    /// `test_*` schemas seen in the catalog.
    pub scanned: usize,
    /// Schemas this pass dropped.
    pub dropped: Vec<String>,
    /// Schemas that still hold a connection lease — a live test owns them.
    pub leased: usize,
    /// Schemas younger than [`ORPHAN_SCHEMA_MIN_AGE`]; possibly mid-creation.
    pub too_young: usize,
    /// Orphans left over because [`ORPHAN_SCHEMA_DROP_BUDGET`] was exhausted.
    pub deferred: usize,
    /// `test_<32 hex>` schemas, whose age cannot be derived from the name.
    pub unageable: usize,
}

impl OrphanReclaimReport {
    /// Abandoned schemas still present after this pass.
    ///
    /// `unageable` counts too: a `test_<uuid>` schema cannot be aged from its
    /// name, so this sweep will never reclaim it and it is backlog by
    /// definition. Counting only `deferred` let a database fill with that
    /// family while the guard reported a clean bill of health.
    pub fn backlog(&self) -> usize {
        self.deferred + self.unageable
    }

    /// True when this pass changed anything, i.e. a message is worth printing.
    pub fn reclaimed_any(&self) -> bool {
        !self.dropped.is_empty()
    }
}

/// Name for a per-test schema: `test_<pid>_<counter>_<unix nanos>`.
///
/// **The name is the cleanup manifest.** It carries the creation instant, and
/// the catalog persists it, so a schema survives its process and is still
/// reclaimable by the next run — no atexit hook, no side file, and nothing to
/// lose when the process is SIGKILLed. Every fixture must use this (or the
/// identical helper in `synapse-test-utils`) so the sweep can age the schema;
/// the uuid-named family used before 2026-10-19 carries no timestamp and is
/// permanently unreclaimable by [`reclaim_orphan_test_schemas`].
pub fn next_stamped_schema_name() -> String {
    use std::sync::atomic::{AtomicU64, Ordering};
    static COUNTER: AtomicU64 = AtomicU64::new(1);
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|elapsed| elapsed.as_nanos())
        .unwrap_or_default();
    format!("test_{}_{}_{}", std::process::id(), COUNTER.fetch_add(1, Ordering::SeqCst), nanos)
}

/// Creation instant embedded in a `test_<pid>_<counter>_<nanos>` schema name.
///
/// `None` for every other family — the shared templates
/// (`test_template_v<N>_<hex>`, `test_isolation_template_<hex>`) and the
/// uuid-named per-test schemas, which carry no timestamp.
pub fn stamped_schema_created_nanos(name: &str) -> Option<u128> {
    let rest = name.strip_prefix("test_")?;
    let mut parts = rest.split('_');
    let pid = parts.next()?;
    let counter = parts.next()?;
    let nanos = parts.next()?;
    if parts.next().is_some() {
        return None;
    }
    if pid.is_empty() || counter.is_empty() {
        return None;
    }
    if !pid.bytes().all(|byte| byte.is_ascii_digit()) || !counter.bytes().all(|byte| byte.is_ascii_digit()) {
        return None;
    }
    // Unix nanos are 19 digits today; the lower bound rejects the 16-hex
    // template fingerprints, which are all-digits only by accident.
    if nanos.len() < 16 || !nanos.bytes().all(|byte| byte.is_ascii_digit()) {
        return None;
    }
    nanos.parse().ok()
}

/// True for `test_<32 hex>` — the uuid-named per-test family.
pub fn is_uuid_test_schema(name: &str) -> bool {
    name.strip_prefix("test_").is_some_and(|rest| rest.len() == 32 && rest.bytes().all(|byte| byte.is_ascii_hexdigit()))
}

/// Drops abandoned per-test schemas that are past [`ORPHAN_SCHEMA_MIN_AGE`] and
/// hold no connection lease.
///
/// Only the stamped `test_<pid>_<n>_<nanos>` family is eligible: its age comes
/// from the name, so a schema created by a process that started a moment ago is
/// never touched. The lease check is the same one the janitor uses, so a schema
/// a live test still owns is skipped (`leased`), not dropped.
///
/// Failures to list or to acquire a connection are returned; a failure to drop
/// one schema is reported on stderr and the pass continues, because one
/// malformed schema must not block the sweep.
pub async fn reclaim_orphan_test_schemas(
    pool: &PgPool,
    now_nanos: u128,
    min_age: Duration,
    budget: usize,
) -> Result<OrphanReclaimReport, String> {
    let names: Vec<String> =
        sqlx::query_scalar("SELECT nspname FROM pg_namespace WHERE nspname ~ '^test_' ORDER BY nspname")
            .fetch_all(pool)
            .await
            .map_err(|error| format!("could not list test schemas: {error}"))?;

    let mut report = OrphanReclaimReport { scanned: names.len(), ..OrphanReclaimReport::default() };
    let min_age_nanos = min_age.as_nanos();

    for name in names {
        let Some(created) = stamped_schema_created_nanos(&name) else {
            if is_uuid_test_schema(&name) {
                report.unageable += 1;
            }
            continue;
        };
        if now_nanos.saturating_sub(created) < min_age_nanos {
            report.too_young += 1;
            continue;
        }
        if report.dropped.len() >= budget {
            report.deferred += 1;
            continue;
        }

        let mut conn = match pool.acquire().await {
            Ok(conn) => conn,
            Err(error) => return Err(format!("could not acquire a connection to reclaim orphans: {error}")),
        };
        match try_acquire_schema_lease(&mut conn, &name).await {
            Ok(true) => {}
            Ok(false) => {
                report.leased += 1;
                continue;
            }
            Err(error) => {
                eprintln!("test schema guard: could not take the lease for {name}: {error}");
                report.leased += 1;
                continue;
            }
        }
        let quoted = name.replace('"', "\"\"");
        let drop_sql = format!(r#"DROP SCHEMA IF EXISTS "{quoted}" CASCADE"#);
        match sqlx::query(&drop_sql).execute(&mut *conn).await {
            Ok(_) => report.dropped.push(name.clone()),
            Err(error) => eprintln!("test schema guard: failed to drop orphan {name}: {error}"),
        }
        if let Err(error) = release_schema_lease(&mut conn, &name).await {
            eprintln!("test schema guard: failed to release the lease for {name}: {error}");
        }
    }

    Ok(report)
}

/// Guard mode, from `TEST_SCHEMA_LEAK_GUARD`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OrphanGuardMode {
    /// Fail the fixture when the backlog is too large (default).
    Deny,
    /// Print the remediation command and continue.
    Warn,
    /// Do nothing (`TEST_SCHEMA_LEAK_GUARD=off`).
    Off,
}

/// See [`OrphanGuardMode`].
pub fn orphan_guard_mode() -> OrphanGuardMode {
    match std::env::var("TEST_SCHEMA_LEAK_GUARD").ok().map(|value| value.trim().to_ascii_lowercase()).as_deref() {
        Some("off") | Some("0") | Some("false") => OrphanGuardMode::Off,
        Some("warn") => OrphanGuardMode::Warn,
        _ => OrphanGuardMode::Deny,
    }
}

/// Backlog limit from `TEST_SCHEMA_LEAK_BACKLOG_LIMIT`.
pub fn orphan_backlog_limit() -> usize {
    std::env::var("TEST_SCHEMA_LEAK_BACKLOG_LIMIT")
        .ok()
        .and_then(|value| value.trim().parse::<usize>().ok())
        .filter(|limit| *limit > 0)
        .unwrap_or(DEFAULT_ORPHAN_BACKLOG_LIMIT)
}

/// Runs one sweep against `database_url` and turns the backlog into a refusal
/// (or a warning) according to [`orphan_guard_mode`].
///
/// Called once per test process by `synapse-test-utils`, from
/// `resolve_test_database_url`, so every pool path (shared clone, pooled
/// schema, isolated schema) is covered by a single call site.
pub async fn run_orphan_guard(database_url: &str) -> Result<(), String> {
    let mode = orphan_guard_mode();
    if mode == OrphanGuardMode::Off {
        return Ok(());
    }

    let pool = sqlx::postgres::PgPoolOptions::new()
        .max_connections(1)
        .acquire_timeout(Duration::from_secs(10))
        .connect(database_url)
        .await
        .map_err(|error| format!("schema guard: could not connect to sweep orphaned schemas: {error}"))?;
    let now_nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|elapsed| elapsed.as_nanos())
        .unwrap_or_default();
    let report = reclaim_orphan_test_schemas(&pool, now_nanos, ORPHAN_SCHEMA_MIN_AGE, ORPHAN_SCHEMA_DROP_BUDGET).await;
    pool.close().await;
    let report = report?;

    if report.reclaimed_any() {
        eprintln!("schema guard: dropped {} orphaned test schema(s) left behind by earlier runs", report.dropped.len());
    }
    if report.unageable > 0 {
        eprintln!(
            "schema guard: {} uuid-named test schema(s) carry no timestamp and were not reclaimed; \
             scripts/cleanup_test_schemas.sh handles that family",
            report.unageable
        );
    }

    let limit = orphan_backlog_limit();
    if report.backlog() > limit {
        let message = format!(
            "schema guard: {} orphaned test schema(s) remain (limit {limit}). Clone DDL re-reads the whole \
             catalog once this backlog grows, which measured 17x slower per test.\n  \
             Fix: bash scripts/cleanup_test_schemas.sh --apply\n  \
             (set TEST_SCHEMA_LEAK_GUARD=warn to run anyway, =off to disable this guard)",
            report.backlog()
        );
        if mode == OrphanGuardMode::Deny {
            return Err(message);
        }
        eprintln!("{message}");
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The sweep only treats `test_<pid>_<n>_<nanos>` as ageable, and the age
    /// must come from the name alone — there is no catalog column for it. A
    /// wrong parse would either drop a live schema (no age ⇒ skip is the safe
    /// branch) or never reclaim anything.
    #[test]
    fn orphan_age_parses_only_the_stamped_family() {
        // Real leaked name observed on 2026-10-19 in `synapse_test`.
        assert_eq!(stamped_schema_created_nanos("test_21225_1_1792413865684199000"), Some(1_792_413_865_684_199_000));

        // Shared templates and uuid-named schemas carry no timestamp.
        assert_eq!(stamped_schema_created_nanos("test_template_v2_bc4d1bf10ac0e676"), None);
        assert_eq!(stamped_schema_created_nanos("test_isolation_template_bc4d1bf10ac0e676"), None);
        assert_eq!(stamped_schema_created_nanos("test_0123456789abcdef0123456789abcdef"), None);

        // Malformed stamps must not be guessed at.
        assert_eq!(stamped_schema_created_nanos("test_1_1"), None);
        assert_eq!(stamped_schema_created_nanos("test_1_1_123"), None);
        assert_eq!(stamped_schema_created_nanos("test_1_1_1792413865684199000_extra"), None);
        assert_eq!(stamped_schema_created_nanos("test_x_1_1792413865684199000"), None);
        assert_eq!(stamped_schema_created_nanos("public"), None);
    }

    #[test]
    fn uuid_family_is_classified_as_unageable() {
        assert!(is_uuid_test_schema("test_0123456789abcdef0123456789abcdef"));
        assert!(!is_uuid_test_schema("test_21225_1_1792413865684199000"));
        assert!(!is_uuid_test_schema("test_template_v2_bc4d1bf10ac0e676"));
        assert!(!is_uuid_test_schema("test_0123456789abcdef0123456789abcde"));
    }

    /// A report with deferred work must be distinguishable from a clean pass:
    /// the caller turns `backlog()` into a refusal, and an off-by-one here
    /// would either block a clean run or hide a 10 GB catalog in the making.
    /// `unageable` counts as backlog — the sweep can never reclaim that family.
    #[test]
    fn backlog_counts_deferred_and_unageable_orphans() {
        let report = OrphanReclaimReport {
            scanned: 7,
            dropped: vec!["test_1_1_1792413865684199000".to_string()],
            leased: 2,
            too_young: 1,
            deferred: 3,
            unageable: 2,
        };
        assert_eq!(report.backlog(), 5);
        assert!(report.reclaimed_any());

        assert_eq!(OrphanReclaimReport::default().backlog(), 0);
        assert!(!OrphanReclaimReport::default().reclaimed_any());
    }

    /// The stamped name must round-trip through the age parser: a generator
    /// change that breaks the format would silently make every schema
    /// unageable and disable the whole sweep.
    #[test]
    fn generated_names_are_ageable() {
        let first = next_stamped_schema_name();
        let second = next_stamped_schema_name();
        assert_ne!(first, second);
        assert!(stamped_schema_created_nanos(&first).is_some(), "not ageable: {first}");
        assert!(stamped_schema_created_nanos(&second).is_some(), "not ageable: {second}");
    }
}
