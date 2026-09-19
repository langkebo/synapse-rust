#![allow(clippy::unwrap_used, clippy::expect_used)]

//! Regression tests for the 2026-09-12 `public`-schema search_path-shadowing
//! incident.
//!
//! Postgres resolves an unqualified relation name in DDL through `search_path`
//! at execution time. The test harness applies migrations into a per-test /
//! template schema while `public` can *also* hold a leftover copy of the same
//! tables (several migrations create tables with `CREATE TABLE IF NOT EXISTS`,
//! which silently SKIPS when the table already exists). A migration that then
//! runs `ALTER TABLE t ADD CONSTRAINT ... REFERENCES p(x)` binds `t` and `p`
//! through `search_path` and can attach the constraint to the `public` copy
//! while pointing at a *transient test schema*.
//!
//! Observed: `public.room_summary_members` carried
//! `fk_room_summary_members_room REFERENCES test_51027_403_...rooms(room_id)`,
//! and `test_<pid>_<n>_<ts>.room_summary_members` was left with no FK at all.
//! Every `synapse-storage` suite that reaches `room_summary_members` through
//! `public.rooms` then failed with SQLSTATE 23503 for a row that demonstrably
//! existed — `room_summary::db_tests::test_add_member_creates_record` was the
//! known-failing canary.
//!
//! Two independent guards:
//! 1. `migration_foreign_keys_must_not_rely_on_search_path` — static, no DB:
//!    no migration may add a constraint whose parent is resolved by search_path.
//! 2. `heal_repoints_cross_schema_foreign_key_at_same_named_parent` — dynamic:
//!    the repair helper actually re-points a corrupted FK. The fixture is a
//!    throwaway schema, never the shared `public`, because nextest runs test
//!    processes in parallel against one database.

use std::fs;
use std::path::{Path, PathBuf};

use sqlx::PgPool;

fn project_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
}

/// The consolidated baseline: it creates its tables inline, so it is exempt
/// from the incremental-migration scan.
const BASELINE: &str = "00000000_unified_schema_v12.sql";

/// Marker emitted when the forward chain is the consolidated baseline alone.
const NO_INCREMENTAL_MIGRATIONS_MARKER: &str = "consolidated-baseline-only";

fn read(path: &Path) -> String {
    fs::read_to_string(path).unwrap_or_else(|error| panic!("failed to read {}: {error}", path.display()))
}

/// Assert that an empty incremental-migration set means "the consolidated
/// baseline is the whole forward chain", not "the filter silently dropped
/// everything".
///
/// The single-forward-file count is asserted here, so the exemption is a
/// measured fact rather than an assumption.
fn assert_baseline_is_the_only_forward_migration(migrations: &Path) {
    let mut forward_sql: Vec<PathBuf> = fs::read_dir(migrations)
        .expect("migrations dir must be readable")
        .filter_map(Result::ok)
        .map(|entry| entry.path())
        .filter(|path| {
            path.extension().is_some_and(|ext| ext == "sql")
                && !path.file_name().is_some_and(|name| name.to_string_lossy().ends_with(".undo.sql"))
        })
        .collect();
    forward_sql.sort();

    assert_eq!(
        forward_sql.len(),
        1,
        "no incremental migration was iterated, yet migrations/ holds {} forward .sql files: \
         the consolidated-baseline exemption needs exactly one — `{BASELINE}`. The entry filter is \
         broken and the search_path invariant never ran. Found: {forward_sql:#?}",
        forward_sql.len()
    );
    assert_eq!(
        forward_sql[0].file_name().and_then(|name| name.to_str()),
        Some(BASELINE),
        "no incremental migration was iterated and the single forward file is not the consolidated \
         baseline `{BASELINE}`"
    );

    // Emitted rather than silent: with `--nocapture` the run states why the
    // invariant loop was skipped. Any `migrations/*.sql` turns the loop back on.
    eprintln!(
        "migration_search_path_guard: {NO_INCREMENTAL_MIGRATIONS_MARKER} — migrations/ contains \
         only the consolidated baseline `{BASELINE}`, so the search_path FK invariant is \
         intentionally vacuous. Adding any migrations/*.sql makes the loop run again."
    );
}

/// Table names created by the v11 baseline. These are exactly the names that a
/// migration can accidentally pick up from `public` instead of the target schema.
fn baseline_table_names(baseline: &str) -> Vec<String> {
    let mut names = Vec::new();
    for line in baseline.lines() {
        let mut rest = match line.find("CREATE TABLE") {
            Some(index) => &line[index + "CREATE TABLE".len()..],
            None => continue,
        };
        // Only the unqualified form is of interest; `CREATE TABLE public.x`
        // is already pinned.
        rest = rest.trim_start();
        if rest.starts_with("public.") || rest.starts_with("pg_") {
            continue;
        }
        rest = rest.strip_prefix("IF NOT EXISTS").unwrap_or(rest).trim_start();
        let name: String = rest.chars().take_while(|c| c.is_ascii_alphanumeric() || *c == '_').collect();
        if name.is_empty() {
            continue;
        }
        if !names.contains(&name) {
            names.push(name);
        }
    }
    names
}

/// Extract `ADD CONSTRAINT ... REFERENCES <parent>(` targets from a migration
/// body and report the ones that are unqualified AND name a baseline table
/// while no `current_schema()` pin appears nearby.
///
/// Scope is deliberately limited to `ADD CONSTRAINT`:
///
/// * `ALTER TABLE t ADD CONSTRAINT ... FOREIGN KEY ... REFERENCES p(x)` is the
///   silent form described in the module docs.
/// * An inline FK inside `CREATE TABLE ... (...)` cannot silently mis-bind: if
///   the parent is missing from the target schema, the DDL fails loudly instead
///   of attaching to `public`.
fn unqualified_baseline_refs(sql: &str, baseline_tables: &[String]) -> Vec<(usize, String)> {
    let lines: Vec<&str> = sql.lines().collect();
    let mut hits = Vec::new();
    for (index, line) in lines.iter().enumerate() {
        let stripped = line.split("--").next().unwrap_or("");
        if !stripped.contains("ADD CONSTRAINT") {
            continue;
        }
        let Some(pos) = stripped.find("REFERENCES") else {
            continue;
        };
        let after = stripped[pos + "REFERENCES".len()..].trim_start();
        // Already schema-qualified (`public.rooms`, `%I.rooms`) → safe.
        let parent: String = after.chars().take_while(|c| c.is_ascii_alphanumeric() || *c == '_').collect();
        if parent.is_empty() || !after[parent.len()..].starts_with('(') {
            continue;
        }
        if !baseline_tables.iter().any(|t| t == &parent) {
            continue;
        }
        // A `current_schema()` pin nearby means the DDL is built dynamically and
        // explicitly schema-bound.
        let lo = index.saturating_sub(6);
        let hi = (index + 3).min(lines.len());
        let window = lines[lo..hi].join("\n");
        if window.contains("current_schema()") {
            continue;
        }
        hits.push((index + 1, parent));
    }
    hits
}

#[test]
fn migration_foreign_keys_must_not_rely_on_search_path() {
    let root = project_root();
    let migrations_dir = root.join("migrations");
    let baseline = read(&migrations_dir.join(BASELINE));
    let baseline_tables = baseline_table_names(&baseline);
    assert!(baseline_tables.len() > 100, "baseline table extraction looks broken: {}", baseline_tables.len());

    let mut violations = Vec::new();
    let mut entries: Vec<_> = fs::read_dir(&migrations_dir)
        .expect("migrations dir must be readable")
        .filter_map(Result::ok)
        .map(|entry| entry.path())
        .filter(|path| {
            path.extension().is_some_and(|ext| ext == "sql")
                // The baseline itself creates its tables inline; an unqualified
                // parent reference there cannot outlive the CREATE that made it.
                && path.file_name().is_none_or(|name| name != BASELINE)
                // `.undo.sql` files are rollback scripts, not part of the forward chain.
                && !path.file_name().is_some_and(|name| name.to_string_lossy().ends_with(".undo.sql"))
        })
        .collect();
    entries.sort();

    if entries.is_empty() {
        // The consolidated baseline is the only forward migration right now, so
        // the invariant loop below cannot run. Say so explicitly and *check* it:
        // an empty iteration that just falls through to `violations.is_empty()`
        // is how this guard went vacuous once the chain was consolidated.
        assert_baseline_is_the_only_forward_migration(&migrations_dir);
    } else {
        for path in entries {
            let sql = read(&path);
            for (line_no, parent) in unqualified_baseline_refs(&sql, &baseline_tables) {
                violations.push(format!(
                    "{}:{} REFERENCES {parent} is resolved via search_path; a leftover public.{parent} \
                     can capture the constraint. Pin it with current_schema()",
                    path.strip_prefix(&root).unwrap_or(&path).display(),
                    line_no
                ));
            }
        }
    }

    assert!(violations.is_empty(), "search_path-dependent foreign keys in migrations:\n  {}", violations.join("\n  "));
}

async fn connect_admin() -> Option<PgPool> {
    let url = std::env::var("TEST_DATABASE_URL").or_else(|_| std::env::var("DATABASE_URL")).ok()?;
    sqlx::postgres::PgPoolOptions::new().max_connections(1).connect(&url).await.ok()
}

#[tokio::test]
async fn heal_repoints_cross_schema_foreign_key_at_same_named_parent() {
    let Some(pool) = connect_admin().await else {
        eprintln!("skipping: no TEST_DATABASE_URL/DATABASE_URL reachable");
        return;
    };

    let stale = format!("a4f2_stale_{}", std::process::id());
    let child = format!("a4f2_child_{}", std::process::id());

    for schema in [&stale, &child] {
        sqlx::query(&format!("DROP SCHEMA IF EXISTS {schema} CASCADE")).execute(&pool).await.unwrap();
        sqlx::query(&format!("CREATE SCHEMA {schema}")).execute(&pool).await.unwrap();
    }

    // `stale` plays the role of the transient test schema the FK was wrongly
    // bound to. `child` plays the role of the corrupted `public` schema and owns
    // a same-named parent table.
    for schema in [&stale, &child] {
        sqlx::query(&format!("CREATE TABLE {schema}.rooms (room_id text PRIMARY KEY)")).execute(&pool).await.unwrap();
    }
    sqlx::query(&format!(
        "CREATE TABLE {child}.room_summary_members (
             id bigserial PRIMARY KEY,
             room_id text NOT NULL,
             CONSTRAINT fk_room_summary_members_room
                 FOREIGN KEY (room_id) REFERENCES {stale}.rooms(room_id) ON DELETE CASCADE
         )"
    ))
    .execute(&pool)
    .await
    .unwrap();

    let before: Vec<String> = sqlx::query_scalar(&format!(
        "SELECT confrelid::regclass::text FROM pg_constraint
         WHERE conrelid = '{child}.room_summary_members'::regclass AND contype = 'f'"
    ))
    .fetch_all(&pool)
    .await
    .unwrap();
    assert_eq!(before, vec![format!("{stale}.rooms")], "fixture must start corrupted");

    synapse_test_utils::heal_cross_schema_foreign_keys_in(&pool, &child).await.expect("heal must succeed");

    let after: Vec<String> = sqlx::query_scalar(&format!(
        "SELECT confrelid::regclass::text FROM pg_constraint
         WHERE conrelid = '{child}.room_summary_members'::regclass AND contype = 'f'"
    ))
    .fetch_all(&pool)
    .await
    .unwrap();
    assert_eq!(after, vec![format!("{child}.rooms")], "heal must re-point the FK at the same-named local parent");

    // Idempotent: a second pass finds nothing to repair.
    synapse_test_utils::heal_cross_schema_foreign_keys_in(&pool, &child).await.expect("second heal must succeed");
    let still: Vec<String> = sqlx::query_scalar(&format!(
        "SELECT confrelid::regclass::text FROM pg_constraint
         WHERE conrelid = '{child}.room_summary_members'::regclass AND contype = 'f'"
    ))
    .fetch_all(&pool)
    .await
    .unwrap();
    assert_eq!(still, vec![format!("{child}.rooms")], "heal must be idempotent");

    // The stale schema is now unreferenced and can be dropped.
    sqlx::query(&format!("DROP SCHEMA {stale} CASCADE")).execute(&pool).await.unwrap();
    sqlx::query(&format!("DROP SCHEMA {child} CASCADE")).execute(&pool).await.unwrap();
}
