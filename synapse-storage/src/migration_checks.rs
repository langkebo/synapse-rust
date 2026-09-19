//! DB-02 migration completeness and drift checks.
//!
//! These queries support [`crate::schema_health_check::run_schema_health_check`]
//! by:
//!
//! 1. Querying `_sqlx_migrations` (sqlx's bookkeeping table, created on first
//!    `sqlx::migrate!`) to find which migrations are applied and to detect
//!    missing ones from the file system.
//! 2. Counting tables in `information_schema.tables` to detect drift between
//!    the baseline schema and the live database.
//!
//! Splitting these helpers into a dedicated module keeps the main
//! `schema_health_check.rs` focused on orchestration logic.

use sqlx::{Pool, Postgres};
use tracing::{debug, warn};

/// Returns `(applied_count, missing_migration_versions)`.
///
/// `missing_migration_versions` is computed by reading the filesystem for
/// `migrations/*.sql` files (excluding the `00000000_unified_schema_v10.sql`
/// baseline and any `.undo.sql` files), parsing their leading 14-digit
/// timestamp version, and diffing against the versions present in
/// `_sqlx_migrations`.
///
/// Failures are non-fatal: if the function cannot read either the DB or the
/// filesystem, it returns `Err(sqlx::Error)` and the caller should record a
/// warning rather than block startup.
pub async fn check_migration_completeness(pool: &Pool<Postgres>) -> Result<(i64, Vec<i64>), sqlx::Error> {
    // 1. List versions that have been applied according to sqlx.
    //
    // `_sqlx_migrations` is sqlx's bookkeeping table, created lazily by
    // `sqlx::migrate!`. This project manages migrations through
    // `docker/db_migrate.sh` and never invokes sqlx migrate, so the table
    // may not exist on a fresh DB. Treat that case as "no sqlx records"
    // rather than as an error so the health check stays a warning, not a
    // fail-to-start.
    let applied: Vec<i64> =
        match sqlx::query_scalar::<_, i64>("SELECT version FROM _sqlx_migrations ORDER BY version ASC")
            .fetch_all(pool)
            .await
        {
            Ok(v) => v,
            Err(sqlx::Error::Database(db_err)) if db_err.code().as_deref() == Some("42P01") => {
                // 42P01 = undefined_table: _sqlx_migrations does not exist.
                // This is the expected state when migrations are managed
                // externally (e.g. docker/db_migrate.sh). Skip the sqlx side
                // of the check entirely.
                debug!("_sqlx_migrations table not present; sqlx migration completeness check is a no-op");
                return Ok((0, Vec::new()));
            }
            Err(e) => return Err(e),
        };

    debug!(applied_count = applied.len(), "queried _sqlx_migrations");

    // 2. Discover migration files on disk.
    let expected = discover_migration_files();
    let expected_set: std::collections::HashSet<i64> = expected.iter().copied().collect();

    // 3. Anything on disk that is not applied is missing.
    let missing: Vec<i64> = expected_set.iter().filter(|v| !applied.iter().any(|a| a == *v)).copied().collect();

    Ok((applied.len() as i64, missing))
}

/// Discover sqlx migration versions present in `migrations/*.sql`.
///
/// Only 14-digit-timestamp forward migrations are returned: the consolidated
/// baseline has a non-numeric prefix, and `.undo.sql` rollback helpers are not
/// migrations (the migrator would treat a `_undo.sql` name as **forward**, which
/// is why the project's suffix is `.undo.sql`; see `migrations/README.md`).
///
/// Manifest path resolution uses `CARGO_MANIFEST_DIR/../migrations`. Cargo sets
/// that variable for test binaries, but a server started from a shell does not
/// inherit it, so in production the lookup normally fails and the completeness
/// check is a no-op. That case is now logged at `warn!` — it used to return an
/// empty set silently, which is why the check sat dead with no signal — and it
/// stays non-fatal by design (see the function docs above).
fn discover_migration_files() -> Vec<i64> {
    let Some(migrations_dir) = migrations_dir() else {
        warn!(
            "CARGO_MANIFEST_DIR is not set, so migrations/ cannot be located; the migration \
             completeness check is a no-op"
        );
        return Vec::new();
    };

    let scanned = scan_migration_files_in(&migrations_dir);
    if scanned.is_empty() {
        warn!(path = %migrations_dir.display(), "no .sql entries found; the migration completeness check is a no-op");
    }

    let mut versions: Vec<i64> = scanned.into_iter().filter_map(|entry| entry.version).collect();
    versions.sort_unstable();
    versions.dedup();
    versions
}

/// `CARGO_MANIFEST_DIR/../migrations`, or `None` when it cannot be resolved.
fn migrations_dir() -> Option<std::path::PathBuf> {
    let manifest_dir = std::env::var("CARGO_MANIFEST_DIR").ok()?;
    let migrations_dir = std::path::Path::new(&manifest_dir).join("..").join("migrations");
    if !migrations_dir.exists() {
        warn!(
            path = %migrations_dir.display(),
            "migrations directory not found; migration completeness check will be no-op"
        );
        return None;
    }
    Some(migrations_dir)
}

/// One `.sql` entry the scan inspected.
#[derive(Debug, Clone, PartialEq, Eq)]
struct ScannedMigration {
    /// File name as it appears in `migrations/`.
    name: String,
    /// `Some(version)` for a forward migration, `None` for anything ignored.
    version: Option<i64>,
}

/// Classify a `migrations/` entry name.
///
/// `Some(version)` = a 14-digit-timestamp forward migration.
/// `None` = everything the completeness check must ignore: the consolidated
/// baseline (non-numeric prefix), `.undo.sql` rollback helpers, non-`.sql`
/// files, and names too short to carry a version.
///
/// Pure and total so the tests can pin every branch. The real `migrations/`
/// directory holds no forward migration today, so an assertion made only
/// against the real directory would be vacuous — which is how this module's
/// tests used to pass while asserting nothing.
fn classify_migration_filename(name: &str) -> Option<i64> {
    if name.ends_with(".undo.sql") || !name.ends_with(".sql") || name.len() < 15 {
        return None;
    }
    name[..14].parse::<i64>().ok()
}

/// Every `.sql` entry in `migrations_dir`, classified, sorted by name.
fn scan_migration_files_in(migrations_dir: &std::path::Path) -> Vec<ScannedMigration> {
    let Ok(entries) = std::fs::read_dir(migrations_dir) else {
        warn!(path = %migrations_dir.display(), "could not read migrations directory");
        return Vec::new();
    };
    let mut scanned: Vec<ScannedMigration> = entries
        .filter_map(|entry| {
            let entry = entry.ok()?;
            let name = entry.file_name().to_str()?.to_string();
            if !name.ends_with(".sql") {
                return None;
            }
            let version = classify_migration_filename(&name);
            Some(ScannedMigration { name, version })
        })
        .collect();
    scanned.sort_by(|a, b| a.name.cmp(&b.name));
    scanned
}

/// Count all base tables in the current schema (single COUNT(*) query).
///
/// `table_type = 'BASE TABLE'` is load-bearing: `information_schema.tables`
/// also lists views, and the baseline declares two of them
/// (`active_workers`, `worker_type_statistics`). Without the filter this
/// counted 232 against the baseline's 230, so every healthy deployment
/// reported `Baseline drift: 2` — a permanent false positive that trains
/// people to ignore the one signal meant to catch a half-applied baseline.
/// (Materialized views — `rooms_summaries_mv`, `public_room_directory` — are
/// not exposed by `information_schema` at all and were never counted.)
///
/// Still includes sqlx-internal base tables like `_sqlx_migrations` if they
/// live in the current schema. The caller uses the count purely to compute a
/// drift signal with a ±10 tolerance — it is not a hard correctness check.
pub async fn count_public_tables(pool: &Pool<Postgres>) -> Result<usize, sqlx::Error> {
    let count: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM information_schema.tables \
         WHERE table_schema = current_schema() AND table_type = 'BASE TABLE'",
    )
    .fetch_one(pool)
    .await?;
    Ok(count as usize)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The real workspace `migrations/` scan, asserted to be **non-empty**.
    ///
    /// Every other assertion here used to run over an empty version set (the
    /// consolidated baseline is the only file in `migrations/`), so all of them
    /// were vacuously true. Pinning the scan itself is what makes the rest
    /// meaningful.
    fn workspace_scan() -> Vec<ScannedMigration> {
        let dir = migrations_dir().expect(
            "CARGO_MANIFEST_DIR/../migrations must resolve when the tests run under cargo; without it \
             the discovery tests assert nothing",
        );
        let scanned = scan_migration_files_in(&dir);
        assert!(
            !scanned.is_empty(),
            "the scan of {} found no .sql entries — the migration completeness check would be vacuous",
            dir.display()
        );
        scanned
    }

    #[test]
    fn discover_reports_only_timestamped_forward_migrations() {
        // Since the consolidation every incremental delta has been folded into
        // the baseline, so the timestamped set is legitimately empty today.
        // What must hold — now and after future migrations are added — is that
        // discovery returns *only* 14-digit-timestamp forward migrations and
        // never reports the baseline / `.undo.sql` files.
        let scanned = workspace_scan();

        // Non-vacuity: the scan classifies a file rather than silently dropping
        // it. The baseline is present and classified as "not a forward
        // migration" — which is what the version assertions below rely on.
        assert!(
            scanned.iter().any(|entry| entry.name == "00000000_unified_schema_v12.sql" && entry.version.is_none()),
            "the v12 baseline must be scanned and classified as non-forward; got {scanned:?}"
        );

        let versions = discover_migration_files();
        for v in &versions {
            assert!(v >= &20240101000000, "version {v} looks too small to be a 14-digit timestamp");
        }

        // Sorted ascending.
        let mut sorted = versions.clone();
        sorted.sort_unstable();
        assert_eq!(sorted, versions, "discovery must produce sorted output");

        // Deduplicated.
        let mut deduped = versions.clone();
        deduped.dedup();
        assert_eq!(deduped, versions, "discovery must produce deduplicated output");
    }

    #[test]
    fn classify_ignores_the_baseline_undo_helpers_and_non_migrations() {
        // The baseline exclusion is exercised against the file that actually
        // exists, not only against a synthetic name.
        workspace_scan();

        assert_eq!(
            classify_migration_filename("00000000_unified_schema_v12.sql"),
            None,
            "the consolidated baseline has no 14-digit version and must be excluded"
        );
        assert_eq!(
            classify_migration_filename("00000001_extensions_v10.sql"),
            None,
            "a name whose 14-char prefix is not numeric must be excluded even though it looks like a \
             migration — this is the rename that silently disabled an earlier hard-coded check"
        );
        assert_eq!(
            classify_migration_filename("20260101000000_add_thing.sql"),
            Some(20260101000000),
            "a 14-digit-timestamp forward migration must be discovered"
        );
        assert_eq!(
            classify_migration_filename("20260101000000_add_thing.undo.sql"),
            None,
            "rollback helpers must be excluded — the migrator would apply a `_undo.sql` name as a \
             forward migration, which is why the suffix is `.undo.sql`"
        );
        assert_eq!(classify_migration_filename("README.md"), None, "non-SQL entries must be excluded");
        assert_eq!(classify_migration_filename("123.sql"), None, "too short to carry a 14-digit version");
        assert_eq!(classify_migration_filename("20260101000000_x.sql.bak"), None, "backups must be excluded");
        assert_eq!(
            classify_migration_filename("abcdefghijklmn_x.sql"),
            None,
            "a non-numeric 14-character prefix must be excluded"
        );
    }

    #[test]
    fn discover_excludes_baseline_and_undo() {
        let versions = discover_migration_files();
        // The baseline is applied as a whole; reporting it as a delta would make
        // the "missing migrations" drift check fire on every healthy deployment.
        assert!(!versions.iter().any(|v| v == &0_i64), "baseline version 0 must be excluded");
        assert!(!versions.iter().any(|v| v == &1_i64), "extensions version 1 must be excluded");

        // Non-vacuity for this test's own subject: the file the exclusion is
        // about must actually be on disk, or the assertions above prove nothing.
        let dir = migrations_dir().expect("migrations dir must resolve under cargo test");
        assert!(
            dir.join("00000000_unified_schema_v12.sql").is_file(),
            "the v12 baseline must exist in {} for this exclusion to mean anything",
            dir.display()
        );
    }
}
