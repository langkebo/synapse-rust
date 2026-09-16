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
/// The v10 baseline file is excluded: it is the "absorb everything" baseline
/// and counts as one logical version from sqlx's perspective. If we included
/// it, we would inflate the expected count.
///
/// `.undo.sql` files are also excluded: they are the project-specific
/// rollback helpers, not sqlx migrations.
///
/// Manifest path resolution: this function uses
/// `CARGO_MANIFEST_DIR/../migrations`. From `synapse-storage/`, the path is
/// `../migrations`. If the path does not exist (e.g. running tests outside
/// the workspace), the function returns an empty set and the health check
/// will report zero expected migrations (which is correct in that scenario).
fn discover_migration_files() -> Vec<i64> {
    let manifest_dir = match std::env::var("CARGO_MANIFEST_DIR") {
        Ok(d) => d,
        Err(_) => return Vec::new(),
    };
    let migrations_dir = std::path::Path::new(&manifest_dir).join("..").join("migrations");
    if !migrations_dir.exists() {
        warn!(
            path = %migrations_dir.display(),
            "migrations directory not found; migration completeness check will be no-op"
        );
        return Vec::new();
    }

    let entries = match std::fs::read_dir(&migrations_dir) {
        Ok(e) => e,
        Err(e) => {
            warn!(error = %e, "could not read migrations directory");
            return Vec::new();
        }
    };

    let mut versions: Vec<i64> = entries
        .filter_map(|entry| {
            let entry = entry.ok()?;
            let path = entry.path();
            let name = path.file_name()?.to_str()?;

            // The consolidated baseline (`00000000_unified_schema_v12.sql`) and
            // the extensions file (`00000001_extensions_v10.sql`) are excluded
            // by the 14-digit version parse below — their prefixes are not
            // numeric. Do not re-add a hard-coded version check here: the one
            // that used to guard `..._v10` silently stopped matching when the
            // baseline was renamed to `_v11`, and the parse already covers it.
            // Skip .undo.sql rollback helpers.
            if name.ends_with(".undo.sql") {
                return None;
            }
            if !name.ends_with(".sql") {
                return None;
            }

            // Filename pattern: `YYYYMMDDHHMMSS_description.sql`
            // The first 14 chars are the version.
            if name.len() < 15 {
                return None;
            }
            name[..14].parse::<i64>().ok()
        })
        .collect();

    versions.sort_unstable();
    versions.dedup();
    versions
}

/// Count all tables in current schema (single COUNT(*) query).
///
/// Includes sqlx-internal tables like `_sqlx_migrations` if they live in
/// `public` (they do by default). The caller uses the count purely to
/// compute a drift signal — it is not a hard correctness check.
pub async fn count_public_tables(pool: &Pool<Postgres>) -> Result<usize, sqlx::Error> {
    let count: i64 =
        sqlx::query_scalar("SELECT COUNT(*) FROM information_schema.tables WHERE table_schema = current_schema()")
            .fetch_one(pool)
            .await?;
    Ok(count as usize)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn discover_reports_only_timestamped_forward_migrations() {
        // Since the v11 consolidation every incremental delta has been folded
        // into the baseline, so the timestamped set is legitimately empty today.
        // What must hold — now and after future migrations are added — is that
        // discovery returns *only* 14-digit-timestamp forward migrations and
        // never reports the baseline / extensions / `.undo.sql` files.
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
    fn discover_excludes_baseline_and_undo() {
        let versions = discover_migration_files();
        // `00000000_unified_schema_v12.sql` and `00000001_extensions_v10.sql`
        // must never be reported as deltas — the baseline is applied as a whole,
        // and reporting it would make the "missing migrations" drift check fire
        // on every healthy deployment.
        assert!(!versions.iter().any(|v| v == &0_i64), "baseline version 0 must be excluded");
        assert!(!versions.iter().any(|v| v == &1_i64), "extensions version 1 must be excluded");
    }
}
