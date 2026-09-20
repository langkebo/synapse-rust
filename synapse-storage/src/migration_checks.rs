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
//!
//! ## Locating the migration source (B10, fail closed)
//!
//! [`check_migration_completeness`] must read the on-disk migration set, so it
//! resolves the directory explicitly instead of degrading to "no migrations":
//!
//! 1. `SYNAPSE_MIGRATIONS_DIR` (trimmed, non-empty) — the same override
//!    `scripts/build_sqlx_migration_source.py` honours, so the build gate and the
//!    runtime check can be pointed at one tree.
//! 2. `<CARGO_MANIFEST_DIR>/migrations`, then `<CARGO_MANIFEST_DIR>/../migrations`.
//!    The second candidate exists because `synapse-storage` is a workspace member:
//!    `CARGO_MANIFEST_DIR` is `synapse-storage/`, while the single source of truth
//!    is the workspace-root `migrations/`.
//! 3. `./migrations` relative to the process working directory — the production
//!    shape: the image copies `migrations/` to `/app/migrations` and runs with
//!    `WORKDIR /app`.
//!
//! If none of those is a readable directory holding at least the unified-schema
//! baseline, or if a `.sql` entry cannot be read / is not a recognised name, the
//! check returns an error (fail closed) rather than an empty `missing` set. There
//! is deliberately **no** skip variable for this: a deployment that stores
//! `migrations/` elsewhere points `SYNAPSE_MIGRATIONS_DIR` at it — an explicit
//! location, not a bypass — and the emergency escape remains
//! `SYNAPSE_SKIP_SCHEMA_CHECK` (see `src/server/database.rs`).

use sqlx::{Pool, Postgres};
use std::path::{Path, PathBuf};
use tracing::debug;

/// Environment variable that overrides the migration source directory.
///
/// The name mirrors `scripts/build_sqlx_migration_source.py`, which already uses
/// it to point the sqlx build gate at a temp tree.
const MIGRATIONS_DIR_ENV: &str = "SYNAPSE_MIGRATIONS_DIR";

/// Prefix of the consolidated unified-schema baseline (`migrations/README.md`).
///
/// Shared with `scripts/build_sqlx_migration_source.py`; the baseline is applied
/// as a whole and carries no 14-digit version, so it is a recognised artifact and
/// never a forward migration.
const BASELINE_PREFIX: &str = "00000000_unified_schema_v";

/// Prefix of the `00000001_extensions*` entry that the baseline supersedes.
const EXTENSIONS_PREFIX: &str = "00000001_extensions";

/// Why the on-disk migration source is unusable.
///
/// Private: it is erased into [`sqlx::Error::Configuration`] so the public
/// signature of [`check_migration_completeness`] stays `Result<_, sqlx::Error>`.
#[derive(Debug)]
enum MigrationSourceError {
    /// `SYNAPSE_MIGRATIONS_DIR` was set but does not name a directory.
    EnvOverrideNotADirectory { path: PathBuf },
    /// None of the candidate locations is a directory.
    NotFound { tried: Vec<PathBuf> },
    /// The directory exists but cannot be listed.
    DirectoryUnreadable { path: PathBuf, source: std::io::Error },
    /// A directory entry could not be read, or a `.sql` file could not be opened.
    EntryUnreadable { path: PathBuf, source: std::io::Error },
    /// A directory entry name is not valid UTF-8.
    EntryNameNotUtf8 { path: PathBuf },
    /// A `.sql` entry is neither a forward migration nor a recognised artifact.
    MalformedMigrationName { path: PathBuf, name: String },
    /// The directory contains no `.sql` entries at all (not even the baseline).
    NoSqlEntries { path: PathBuf },
    /// The process working directory could not be read.
    WorkingDirectoryUnreadable { source: std::io::Error },
}

impl std::fmt::Display for MigrationSourceError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::EnvOverrideNotADirectory { path } => write!(
                f,
                "{MIGRATIONS_DIR_ENV}={} is not a directory; point it at the directory holding \
                 {BASELINE_PREFIX}*.sql, or unset it",
                path.display()
            ),
            Self::NotFound { tried } => write!(
                f,
                "could not locate a migrations directory (tried {}); set {MIGRATIONS_DIR_ENV} to the \
                 directory holding {BASELINE_PREFIX}*.sql",
                display_paths(tried)
            ),
            Self::DirectoryUnreadable { path, source } => {
                write!(f, "cannot read the migrations directory {}: {source}", path.display())
            }
            Self::EntryUnreadable { path, source } => {
                write!(f, "cannot read migration entry {}: {source}", path.display())
            }
            Self::EntryNameNotUtf8 { path } => {
                write!(f, "migration entry {} has a non-UTF-8 name", path.display())
            }
            Self::MalformedMigrationName { path, name } => write!(
                f,
                "migration entry {name} ({}) is not a valid forward migration name; expected a \
                 14-digit timestamp prefix or a recognised artifact ({BASELINE_PREFIX}*.sql / \
                 {EXTENSIONS_PREFIX}*.sql / *.undo.sql)",
                path.display()
            ),
            Self::NoSqlEntries { path } => write!(
                f,
                "{} contains no .sql migrations; a valid source must contain {BASELINE_PREFIX}*.sql",
                path.display()
            ),
            Self::WorkingDirectoryUnreadable { source } => {
                write!(f, "cannot read the process working directory: {source}")
            }
        }
    }
}

impl std::error::Error for MigrationSourceError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::DirectoryUnreadable { source, .. }
            | Self::EntryUnreadable { source, .. }
            | Self::WorkingDirectoryUnreadable { source } => Some(source),
            _ => None,
        }
    }
}

/// Join paths for a `Display` message without allocating a temporary `Vec`.
fn display_paths(paths: &[PathBuf]) -> String {
    paths.iter().map(|path| path.display().to_string()).collect::<Vec<_>>().join(", ")
}

/// Returns `(applied_count, missing_migration_versions)`.
///
/// `missing_migration_versions` is computed by reading the filesystem for
/// `migrations/*.sql` files (excluding the `00000000_unified_schema_v*.sql`
/// baseline and any `.undo.sql` files), parsing their leading 14-digit
/// timestamp version, and diffing against the versions present in
/// `_sqlx_migrations`.
///
/// Fail closed: an unusable migration source is surfaced as `Err` (the private
/// `MigrationSourceError` boxed into [`sqlx::Error::Configuration`]), never as
/// an empty `missing` set. The caller (`run_schema_health_check`) propagates it
/// with `?`, which the startup caller treats as fatal.
pub async fn check_migration_completeness(pool: &Pool<Postgres>) -> Result<(i64, Vec<i64>), sqlx::Error> {
    completeness_against(pool, discover_migration_files()).await
}

/// The DB half of the check, with the on-disk source already resolved.
///
/// Split out of [`check_migration_completeness`] so tests can pin the fail-closed
/// source path without a database (the source is resolved before the pool is ever
/// used) and without mutating process env.
async fn completeness_against(
    pool: &Pool<Postgres>,
    source: Result<Vec<i64>, MigrationSourceError>,
) -> Result<(i64, Vec<i64>), sqlx::Error> {
    // 1. Resolve the source first. An unusable migrations directory must fail the
    //    check even when `_sqlx_migrations` is absent — that absence used to make
    //    the whole check a no-op and must not mask a missing source.
    let expected = source.map_err(|error| sqlx::Error::Configuration(Box::new(error)))?;
    let expected_set: std::collections::HashSet<i64> = expected.iter().copied().collect();

    // 2. List versions that have been applied according to sqlx.
    //
    // `_sqlx_migrations` is sqlx's bookkeeping table, created lazily by
    // `sqlx::migrate!`. This project manages migrations through
    // `docker/db_migrate.sh` and never invokes sqlx migrate, so the table
    // may not exist on a fresh DB. Treat that one case as "no sqlx records"
    // so external migration management stays a no-op instead of a
    // fail-to-start. Every other query failure propagates: the three checks
    // that run before this one already fail on database errors, so a silent
    // downgrade here would only hide a real problem.
    let applied: Vec<i64> =
        match sqlx::query_scalar::<_, i64>("SELECT version FROM _sqlx_migrations ORDER BY version ASC")
            .fetch_all(pool)
            .await
        {
            Ok(v) => v,
            Err(sqlx::Error::Database(db_err)) if db_err.code().as_deref() == Some("42P01") => {
                // 42P01 = undefined_table: _sqlx_migrations does not exist.
                debug!("_sqlx_migrations table not present; sqlx migration completeness check is a no-op");
                return Ok((0, Vec::new()));
            }
            Err(e) => return Err(e),
        };

    debug!(applied_count = applied.len(), "queried _sqlx_migrations");

    // 3. Anything on disk that is not applied is missing.
    let missing: Vec<i64> = expected_set.iter().filter(|v| !applied.iter().any(|a| a == *v)).copied().collect();

    Ok((applied.len() as i64, missing))
}

/// Resolve the migration source from the process environment and read its
/// forward migration versions.
fn discover_migration_files() -> Result<Vec<i64>, MigrationSourceError> {
    let env_override = std::env::var(MIGRATIONS_DIR_ENV).ok();
    let manifest_dir = std::env::var("CARGO_MANIFEST_DIR").ok();
    let cwd = std::env::current_dir().map_err(|source| MigrationSourceError::WorkingDirectoryUnreadable { source })?;
    let dir = resolve_migrations_dir(env_override.as_deref(), manifest_dir.as_deref().map(Path::new), &cwd)?;
    discover_migration_files_in(&dir)
}

/// `SYNAPSE_MIGRATIONS_DIR` -> `<CARGO_MANIFEST_DIR>/migrations` ->
/// `<CARGO_MANIFEST_DIR>/../migrations` -> `<CWD>/migrations`.
///
/// The `../migrations` candidate is required because this crate is a workspace
/// member: `CARGO_MANIFEST_DIR` points at `synapse-storage/`, while the single
/// source of truth is the workspace-root `migrations/`. A non-empty
/// `SYNAPSE_MIGRATIONS_DIR` is authoritative: when it names something that is not
/// a directory the resolution fails instead of silently falling through.
fn resolve_migrations_dir(
    env_override: Option<&str>,
    manifest_dir: Option<&Path>,
    cwd: &Path,
) -> Result<PathBuf, MigrationSourceError> {
    if let Some(raw) = env_override.map(str::trim).filter(|value| !value.is_empty()) {
        let path = PathBuf::from(raw);
        if path.is_dir() {
            return Ok(path);
        }
        return Err(MigrationSourceError::EnvOverrideNotADirectory { path });
    }

    let mut tried = Vec::new();
    if let Some(manifest_dir) = manifest_dir {
        for candidate in [manifest_dir.join("migrations"), manifest_dir.join("..").join("migrations")] {
            if candidate.is_dir() {
                return Ok(candidate);
            }
            tried.push(candidate);
        }
    }

    let cwd_candidate = cwd.join("migrations");
    if cwd_candidate.is_dir() {
        return Ok(cwd_candidate);
    }
    tried.push(cwd_candidate);

    Err(MigrationSourceError::NotFound { tried })
}

/// Forward-migration versions in an already-resolved directory.
///
/// Fails closed: a directory with no `.sql` entries is not a valid source (it must
/// at least carry the baseline), and any `.sql` entry that is neither a forward
/// migration nor a recognised artifact is an error rather than a silent omission.
fn discover_migration_files_in(migrations_dir: &Path) -> Result<Vec<i64>, MigrationSourceError> {
    let scanned = scan_migration_files_in(migrations_dir)?;
    if scanned.is_empty() {
        return Err(MigrationSourceError::NoSqlEntries { path: migrations_dir.to_path_buf() });
    }

    let mut versions: Vec<i64> = scanned.into_iter().filter_map(|entry| entry.version).collect();
    versions.sort_unstable();
    versions.dedup();
    Ok(versions)
}

/// One `.sql` entry the scan inspected.
#[derive(Debug, Clone, PartialEq, Eq)]
struct ScannedMigration {
    /// File name as it appears in `migrations/`.
    name: String,
    /// `Some(version)` for a forward migration, `None` for a recognised artifact.
    version: Option<i64>,
}

/// Classify a `migrations/` entry name.
///
/// `Some(version)` = a 14-digit-timestamp forward migration.
/// `None` = a name that carries no forward version: the consolidated baseline,
/// `.undo.sql` rollback helpers, non-`.sql` entries, backup suffixes, and names
/// too short or too non-numeric to carry a version.
///
/// Pure and total so the tests can pin every branch. Classification alone does not
/// decide policy: [`scan_migration_files_in`] rejects a `.sql` name that is
/// `None` **and** not a recognised artifact (see [`is_recognized_artifact`]), so a
/// malformed forward migration is reported instead of dropped.
fn classify_migration_filename(name: &str) -> Option<i64> {
    if name.ends_with(".undo.sql") || !name.ends_with(".sql") || name.len() < 15 {
        return None;
    }
    name[..14].parse::<i64>().ok()
}

/// Names under `migrations/` that are intentionally not forward-migration
/// versions: the consolidated baseline, the `00000001_extensions*` entry it
/// supersedes, and `.undo.sql` rollback helpers.
fn is_recognized_artifact(name: &str) -> bool {
    name.ends_with(".undo.sql") || name.starts_with(BASELINE_PREFIX) || name.starts_with(EXTENSIONS_PREFIX)
}

/// Every `.sql` entry in `migrations_dir`, classified, sorted by name.
///
/// Returns `Result`: an unreadable directory, an unreadable `.sql` entry, a
/// non-UTF-8 name, or a `.sql` name that is not a recognised artifact is an error,
/// never a silently dropped entry.
fn scan_migration_files_in(migrations_dir: &Path) -> Result<Vec<ScannedMigration>, MigrationSourceError> {
    let entries = std::fs::read_dir(migrations_dir)
        .map_err(|source| MigrationSourceError::DirectoryUnreadable { path: migrations_dir.to_path_buf(), source })?;

    let mut scanned: Vec<ScannedMigration> = Vec::new();
    for entry in entries {
        let entry = entry
            .map_err(|source| MigrationSourceError::EntryUnreadable { path: migrations_dir.to_path_buf(), source })?;
        let name = entry
            .file_name()
            .to_str()
            .ok_or_else(|| MigrationSourceError::EntryNameNotUtf8 { path: entry.path() })?
            .to_string();
        if !name.ends_with(".sql") {
            continue;
        }

        // The migrator must be able to read every `.sql` file that is counted as
        // present; a chmod-000 entry would otherwise look like part of the source
        // while being unappliable.
        std::fs::File::open(entry.path())
            .map_err(|source| MigrationSourceError::EntryUnreadable { path: entry.path(), source })?;

        let version = classify_migration_filename(&name);
        if version.is_none() && !is_recognized_artifact(&name) {
            return Err(MigrationSourceError::MalformedMigrationName { path: entry.path(), name });
        }
        scanned.push(ScannedMigration { name, version });
    }

    scanned.sort_by(|a, b| a.name.cmp(&b.name));
    Ok(scanned)
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

    /// Throwaway directory under the process temp dir, removed on drop.
    struct TempTree {
        root: PathBuf,
    }

    impl TempTree {
        fn new(label: &str) -> Self {
            let nanos = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .expect("system clock must be after the Unix epoch")
                .as_nanos();
            let root = std::env::temp_dir().join(format!("synapse-storage-b10-{label}-{}-{nanos}", std::process::id()));
            std::fs::create_dir_all(&root).expect("temp tree must be creatable");
            Self { root }
        }

        fn join(&self, relative: &str) -> PathBuf {
            self.root.join(relative)
        }
    }

    impl Drop for TempTree {
        fn drop(&mut self) {
            // A chmod-000 probe must not leave an undeletable tree behind.
            #[cfg(unix)]
            {
                use std::os::unix::fs::PermissionsExt as _;
                let _ = std::fs::set_permissions(&self.root, std::fs::Permissions::from_mode(0o755));
            }
            let _ = std::fs::remove_dir_all(&self.root);
        }
    }

    /// Write a readable baseline so a probe directory is a *valid* source except
    /// for the one property under test.
    fn write_baseline(dir: &Path) {
        std::fs::write(dir.join("00000000_unified_schema_v12.sql"), "-- baseline probe\n")
            .expect("baseline probe must be writable");
    }

    fn workspace_migrations_dir() -> PathBuf {
        resolve_migrations_dir(None, Some(Path::new(env!("CARGO_MANIFEST_DIR"))), Path::new(".")).expect(
            "the workspace migrations/ directory must resolve when the tests run under cargo; without it the \
             discovery tests assert nothing",
        )
    }

    /// The real workspace `migrations/` scan, asserted to be **non-empty**.
    ///
    /// Every other assertion here used to run over an empty version set (the
    /// consolidated baseline is the only file in `migrations/`), so all of them
    /// were vacuously true. Pinning the scan itself is what makes the rest
    /// meaningful.
    fn workspace_scan() -> Vec<ScannedMigration> {
        let dir = workspace_migrations_dir();
        let scanned = scan_migration_files_in(&dir).expect("the real migrations/ directory must be readable");
        assert!(
            !scanned.is_empty(),
            "the scan of {} found no .sql entries — the migration completeness check would be vacuous",
            dir.display()
        );
        scanned
    }

    /// B10 red proof (a): the pre-fix logic turned a missing/unreadable source
    /// into an empty `missing` set, so the check passed vacuously (probe output:
    /// `discover_migration_files() = []`, `missing = [] => passed = true`, and a
    /// chmod-000 dir scanned to `[]`). The fix returns a named error for every one
    /// of those inputs.
    #[tokio::test]
    async fn unresolvable_source_is_an_error_not_an_empty_set() {
        let tree = TempTree::new("unresolvable");
        let manifest = tree.join("manifest");
        std::fs::create_dir_all(&manifest).expect("probe manifest dir");
        let cwd = tree.join("cwd");
        std::fs::create_dir_all(&cwd).expect("probe cwd dir");

        // (1) A path that does not exist is rejected, not ignored.
        let err = resolve_migrations_dir(Some("/nonexistent/b10-migrations-probe"), Some(&manifest), &cwd)
            .expect_err("a non-existent SYNAPSE_MIGRATIONS_DIR must fail resolution");
        let msg = err.to_string();
        assert!(msg.contains(MIGRATIONS_DIR_ENV) && msg.contains("not a directory"), "got: {msg}");

        // (2) No candidate resolves at all.
        let err =
            resolve_migrations_dir(None, Some(&manifest), &cwd).expect_err("a tree with no migrations dir must fail");
        let msg = err.to_string();
        assert!(msg.contains(MIGRATIONS_DIR_ENV) && msg.contains("could not locate"), "got: {msg}");

        // (3) An existing but empty directory is not a valid source.
        let empty = tree.join("empty");
        std::fs::create_dir_all(&empty).expect("probe empty dir");
        let err = discover_migration_files_in(&empty).expect_err("an empty migrations dir must fail");
        assert!(err.to_string().contains("no .sql migrations"), "got: {err}");

        // (4) A directory that does not exist is an error, not an empty scan.
        let err =
            discover_migration_files_in(&tree.join("does-not-exist")).expect_err("a missing migrations dir must fail");
        assert!(err.to_string().contains("cannot read the migrations directory"), "got: {err}");

        // (5) The composed check fails closed *before* the pool is used: the lazy
        //     pool below never connects, so a message naming the source proves the
        //     source error is what the check returns.
        let source = resolve_migrations_dir(Some(empty.to_str().expect("temp path is UTF-8")), None, &cwd)
            .and_then(|dir| discover_migration_files_in(&dir));
        assert!(source.is_err(), "the empty dir must not resolve to a usable source");
        let pool = sqlx::postgres::PgPoolOptions::new()
            .connect_lazy("postgres://probe:probe@127.0.0.1:1/probe")
            .expect("lazy pool construction must not connect");
        let err = completeness_against(&pool, source).await.expect_err("an unusable source must fail the check");
        assert!(err.to_string().contains("no .sql migrations"), "got: {err}");
    }

    /// B10 red proof (b): an unreadable `.sql` entry is a hard failure. The probe
    /// lives in a temp copy, never the repo's `migrations/`.
    #[cfg(unix)]
    #[test]
    fn unreadable_sql_entry_is_an_error() {
        use std::os::unix::fs::PermissionsExt as _;
        let tree = TempTree::new("unreadable-entry");
        let dir = tree.join("migrations");
        std::fs::create_dir_all(&dir).expect("probe migrations dir");
        write_baseline(&dir);
        let locked = dir.join("20260101000000_locked.sql");
        std::fs::write(&locked, "-- locked\n").expect("locked probe must be writable");
        std::fs::set_permissions(&locked, std::fs::Permissions::from_mode(0o000)).expect("chmod 000");

        // root ignores file modes, so the probe cannot prove anything there.
        if std::fs::File::open(&locked).is_ok() {
            std::fs::set_permissions(&locked, std::fs::Permissions::from_mode(0o644)).expect("restore mode");
            eprintln!("skipping unreadable_sql_entry_is_an_error: chmod 000 is not enforced for this process");
            return;
        }

        let err = discover_migration_files_in(&dir).expect_err("a chmod-000 .sql entry must fail the scan");
        let msg = err.to_string();
        std::fs::set_permissions(&locked, std::fs::Permissions::from_mode(0o644)).expect("restore mode");
        assert!(msg.contains("cannot read migration entry") && msg.contains("20260101000000_locked.sql"), "got: {msg}");
        assert!(msg.contains("Permission denied") || msg.contains("os error 13"), "got: {msg}");
    }

    /// B10 red proof (b) companion: a chmod-000 directory is reported, not
    /// swallowed into an empty scan.
    #[cfg(unix)]
    #[test]
    fn unreadable_migrations_directory_is_an_error() {
        use std::os::unix::fs::PermissionsExt as _;
        let tree = TempTree::new("unreadable-dir");
        let dir = tree.join("locked");
        std::fs::create_dir_all(&dir).expect("probe locked dir");
        write_baseline(&dir);
        std::fs::set_permissions(&dir, std::fs::Permissions::from_mode(0o000)).expect("chmod 000");

        if std::fs::read_dir(&dir).is_ok() {
            std::fs::set_permissions(&dir, std::fs::Permissions::from_mode(0o755)).expect("restore mode");
            eprintln!("skipping unreadable_migrations_directory_is_an_error: chmod 000 is not enforced");
            return;
        }

        let err = discover_migration_files_in(&dir).expect_err("a chmod-000 directory must fail the scan");
        std::fs::set_permissions(&dir, std::fs::Permissions::from_mode(0o755)).expect("restore mode");
        assert!(err.to_string().contains("cannot read the migrations directory"), "got: {err}");
    }

    /// `.sql` names that are neither forward migrations nor recognised artifacts
    /// are errors; they used to be dropped by `filter_map`.
    #[test]
    fn malformed_forward_migration_names_are_errors_not_silent_omissions() {
        let tree = TempTree::new("malformed");
        for bad in ["123.sql", "abcdefghijklmn_x.sql"] {
            let dir = tree.join(&format!("bad-{}", bad.replace('.', "_")));
            std::fs::create_dir_all(&dir).expect("probe dir");
            write_baseline(&dir);
            std::fs::write(dir.join(bad), "-- bad\n").expect("bad probe must be writable");

            let err = discover_migration_files_in(&dir).expect_err("a malformed .sql name must be reported");
            let msg = err.to_string();
            assert!(msg.contains(bad), "the error must name the offending file, got: {msg}");
            assert!(msg.contains("14-digit timestamp"), "got: {msg}");
        }
    }

    /// The recognised artifacts and real forward migrations still resolve, and
    /// non-`.sql` entries stay ignored.
    #[test]
    fn recognized_artifacts_and_forward_migrations_still_resolve() {
        let tree = TempTree::new("recognized");
        let dir = tree.join("migrations");
        std::fs::create_dir_all(&dir).expect("probe migrations dir");
        write_baseline(&dir);
        std::fs::write(dir.join("00000001_extensions_v10.sql"), "-- extensions\n").expect("extensions probe");
        std::fs::write(dir.join("20260101000000_add_thing.undo.sql"), "-- undo\n").expect("undo probe");
        std::fs::write(dir.join("20260101000000_add_thing.sql"), "-- forward\n").expect("forward probe");
        std::fs::write(dir.join("README.md"), "docs\n").expect("readme probe");

        assert_eq!(discover_migration_files_in(&dir).expect("a valid source must resolve"), vec![20260101000000]);
    }

    /// Resolution order: env override, then the manifest candidates, then CWD.
    #[test]
    fn resolution_order_is_env_then_manifest_then_cwd() {
        let tree = TempTree::new("resolution-order");
        let env_dir = tree.join("env-migrations");
        let manifest = tree.join("manifest");
        let manifest_migrations = manifest.join("migrations");
        let cwd = tree.join("cwd");
        let cwd_migrations = cwd.join("migrations");
        for dir in [&env_dir, &manifest_migrations, &cwd_migrations] {
            std::fs::create_dir_all(dir).expect("probe candidate dir");
            write_baseline(dir);
        }

        assert_eq!(
            resolve_migrations_dir(Some(env_dir.to_str().expect("UTF-8")), Some(&manifest), &cwd)
                .expect("env override must win"),
            env_dir
        );
        assert_eq!(
            resolve_migrations_dir(Some("   "), Some(&manifest), &cwd).expect("blank env is unset"),
            manifest_migrations
        );
        assert_eq!(
            resolve_migrations_dir(None, Some(&manifest), &cwd).expect("manifest candidate must win over cwd"),
            manifest_migrations
        );
        assert_eq!(resolve_migrations_dir(None, None, &cwd).expect("cwd is the production fallback"), cwd_migrations);
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

        let versions = discover_migration_files().expect("the workspace migrations source must resolve under cargo");
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
        let versions = discover_migration_files().expect("the workspace migrations source must resolve under cargo");
        // The baseline is applied as a whole; reporting it as a delta would make
        // the "missing migrations" drift check fire on every healthy deployment.
        assert!(!versions.iter().any(|v| v == &0_i64), "baseline version 0 must be excluded");
        assert!(!versions.iter().any(|v| v == &1_i64), "extensions version 1 must be excluded");

        // Non-vacuity for this test's own subject: the file the exclusion is
        // about must actually be on disk, or the assertions above prove nothing.
        let dir = workspace_migrations_dir();
        assert!(
            dir.join("00000000_unified_schema_v12.sql").is_file(),
            "the v12 baseline must exist in {} for this exclusion to mean anything",
            dir.display()
        );
    }
}
