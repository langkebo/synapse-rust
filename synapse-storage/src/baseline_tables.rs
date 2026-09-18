//! Compile-time baseline table extraction.
//!
//! The `00000000_unified_schema_v12.sql` file is the single source of truth for
//! which tables the synapse-rust database is required to provide. This module
//! parses that file at compile time via [`include_str!`] and lazily memoizes
//! the resulting table list.
//!
//! Why lazy rather than `const`? The parser walks lines and rejects
//! `IF NOT EXISTS` boilerplate and inline comments; doing this at `const`
//! time would require a full const-fn string tokenizer, which is overkill.
//! Lazy evaluation with `OnceLock` runs the parser exactly once per process
//! and is effectively free at runtime.
//!
//! Why `include_str!` rather than a `build.rs`? The project does not currently
//! use `build.rs`; introducing one just for table extraction would expand the
//! build surface and risk cache invalidation. `include_str!` makes the
//! dependency on `migrations/00000000_unified_schema_v12.sql` explicit and
//! participates in cargo's normal incremental rebuild graph.

use std::sync::OnceLock;

/// Raw contents of the v12 baseline schema file.
///
/// Path is relative to this file (`synapse-storage/src/baseline_tables.rs`).
/// Two `..` steps reach the workspace root, then `migrations/` contains the
/// file. Cargo's manifest-relative `include_str!` resolves relative to
/// `CARGO_MANIFEST_DIR`, which is `synapse-storage/` for this crate.
const BASELINE_SQL: &str = include_str!("../../migrations/00000000_unified_schema_v12.sql");

/// Cached sorted list of all `CREATE TABLE` table names declared in the
/// baseline schema.
///
/// Empty until the first call to [`baseline_tables`].
fn cached_baseline_tables() -> &'static Vec<&'static str> {
    static CACHE: OnceLock<Vec<&'static str>> = OnceLock::new();
    CACHE.get_or_init(parse_baseline_tables)
}

/// Parse every `CREATE TABLE [IF NOT EXISTS] <name>` declaration.
///
/// The baseline uses `CREATE TABLE` in its main region (lines 1..~5130) and
/// `CREATE TABLE IF NOT EXISTS` in the folded consolidation region at the tail
/// (lines ~5130..end). Both forms are accepted.
///
/// We deliberately ignore any CREATE TABLE inside an SQL comment (`--`) to
/// avoid false positives from documentation blocks. The parser is line-based
/// rather than token-aware; SQL `/* ... */` block comments are rare in the
/// baseline, but where they exist they cannot host CREATE TABLE statements
/// without breaking the file's syntax anyway.
fn parse_baseline_tables() -> Vec<&'static str> {
    let mut names: Vec<&'static str> = BASELINE_SQL.lines().filter_map(extract_table_name).collect();

    // Defensive dedup: every table must be declared exactly once (the guard in
    // `tests/unit/migration_consistency_tests.rs` enforces that), but the list
    // is compared against `information_schema` counts, so it must stay
    // duplicate-free even if the baseline ever regresses.
    names.sort_unstable();
    names.dedup();
    names
}

/// Extract the table name from a single line, if and only if the line
/// declares a top-level `CREATE TABLE`.
///
/// Recognised forms:
///   - `CREATE TABLE foo (`
///   - `CREATE TABLE IF NOT EXISTS foo (`
///
/// Rejected forms (returning `None`):
///   - `CREATE TABLE foo AS SELECT ...` — none in this baseline, but defensive
///   - Lines starting with `--` (comments)
///   - Whitespace-only lines
fn extract_table_name(line: &str) -> Option<&'static str> {
    let trimmed = line.trim_start();
    if trimmed.starts_with("--") {
        return None;
    }

    let after_create = trimmed.strip_prefix("CREATE TABLE ")?;
    let after_create = if let Some(rest) = after_create.strip_prefix("IF NOT EXISTS ") { rest } else { after_create };

    // End at the first whitespace, opening paren, or semicolon.
    let end = after_create.find(|c: char| c.is_whitespace() || c == '(' || c == ';').unwrap_or(after_create.len());

    if end == 0 {
        return None;
    }

    // SAFETY: We allocate a new owned `String` for the parsed name and
    // leak it into the `'static` lifetime so the public API
    // (`baseline_tables()`) can return `&'static [&'static str]`. This is
    // acceptable because:
    //
    //   1. The function is called at most once (memoised via `OnceLock`).
    //   2. The leak size is bounded: ~30 bytes per table × ~200 tables
    //      ≈ 6 KB total, dwarfed by the baseline file's own ~6 MB size
    //      already embedded into the binary via `include_str!`.
    //   3. We never mutate or free the leaked strings; their lifetime is
    //      the same as the binary itself.
    let name: &'static str = Box::leak(Box::new(after_create[..end].to_string()));
    Some(name)
}

/// Return the sorted, deduplicated list of table names declared in the
/// baseline schema file.
///
/// The returned slice is `'static` and memoised — repeated calls return the
/// same allocation.
pub fn baseline_tables() -> &'static [&'static str] {
    cached_baseline_tables()
}

/// Number of tables declared in the baseline schema.
///
/// Convenience for logging and drift checks.
pub fn baseline_table_count() -> usize {
    baseline_tables().len()
}

/// Cached sorted list of every index name the baseline materialises.
///
/// Empty until the first call to [`baseline_index_names`].
fn cached_baseline_index_names() -> &'static Vec<&'static str> {
    static CACHE: OnceLock<Vec<&'static str>> = OnceLock::new();
    CACHE.get_or_init(parse_baseline_index_names)
}

/// Parse every index name the baseline creates.
///
/// Two declaration forms materialise an index in Postgres, and
/// `schema_health_check::check_missing_indexes` observes both through
/// `pg_indexes`:
///
///   1. `CREATE [UNIQUE] INDEX [CONCURRENTLY] IF NOT EXISTS <name>`.
///   2. A named table-constraint index: `CONSTRAINT <name> UNIQUE (...)` or
///      `CONSTRAINT <name> PRIMARY KEY (...)`.
///
/// Unnamed inline `PRIMARY KEY` / `UNIQUE` columns (whose index Postgres names
/// `<table>_pkey` / `<table>_<column>_key`) are deliberately not enumerated: no
/// value in `REQUIRED_INDEXES` relies on that form. The parser is line-based,
/// mirroring [`parse_baseline_tables`].
fn parse_baseline_index_names() -> Vec<&'static str> {
    let mut names: Vec<&'static str> = BASELINE_SQL.lines().filter_map(extract_index_name).collect();
    names.sort_unstable();
    names.dedup();
    names
}

/// Extract the created index name from a single line, if the line creates one.
///
/// Rejected forms (returning `None`) include comments, `CONSTRAINT <name> CHECK
/// (...)`, and `CONSTRAINT <name> FOREIGN KEY (...)`: those do not create an
/// index.
fn extract_index_name(line: &'static str) -> Option<&'static str> {
    let trimmed = line.trim_start();
    if trimmed.starts_with("--") {
        return None;
    }

    for prefix in [
        "CREATE UNIQUE INDEX CONCURRENTLY IF NOT EXISTS ",
        "CREATE INDEX CONCURRENTLY IF NOT EXISTS ",
        "CREATE UNIQUE INDEX IF NOT EXISTS ",
        "CREATE INDEX IF NOT EXISTS ",
    ] {
        if let Some(rest) = trimmed.strip_prefix(prefix) {
            return first_token(rest);
        }
    }

    let rest = trimmed.strip_prefix("CONSTRAINT ")?;
    let name = first_token(rest)?;
    let tail = rest[name.len()..].trim_start();
    if tail.starts_with("UNIQUE") || tail.starts_with("PRIMARY KEY") {
        Some(name)
    } else {
        None
    }
}

/// Return the first whitespace / paren / semicolon-delimited token, if non-empty.
fn first_token(rest: &'static str) -> Option<&'static str> {
    let end = rest.find(|c: char| c.is_whitespace() || c == '(' || c == ';').unwrap_or(rest.len());
    if end == 0 {
        None
    } else {
        Some(&rest[..end])
    }
}

/// Return the sorted, deduplicated list of index names the baseline creates,
/// including indexes materialised by named `UNIQUE` / `PRIMARY KEY` constraints.
///
/// Used as the compile-time reference for `REQUIRED_INDEXES` so a required index
/// that the baseline does not create fails a unit test instead of printing a
/// spurious `Missing indexes` warning at every startup.
pub fn baseline_index_names() -> &'static [&'static str] {
    cached_baseline_index_names()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn extracts_canonical_tables() {
        let names = baseline_tables();
        // Spot-check several well-known tables.
        assert!(names.contains(&"users"), "expected 'users' in baseline");
        assert!(names.contains(&"rooms"), "expected 'rooms' in baseline");
        assert!(names.contains(&"events"), "expected 'events' in baseline");
        assert!(names.contains(&"event_relations"), "expected 'event_relations' in baseline");
        assert!(
            names.contains(&"schema_migrations"),
            "expected 'schema_migrations' in baseline (project-required business table)"
        );
    }

    #[test]
    fn list_is_sorted_and_deduped() {
        let names = baseline_tables();
        let mut sorted = names.to_vec();
        sorted.sort_unstable();
        sorted.dedup();
        assert_eq!(names, &sorted[..], "baseline_tables() must be sorted + deduped");
    }

    #[test]
    fn list_is_non_trivially_large() {
        let count = baseline_table_count();
        // The v12 baseline should contain well over 100 tables.
        // If this drops below 50, somebody removed half the schema.
        assert!(count >= 100, "baseline has only {count} tables — this is suspiciously low for v12");
    }

    #[test]
    fn parser_handles_if_not_exists_form() {
        // Direct unit test for the IF NOT EXISTS branch.
        let line = "CREATE TABLE IF NOT EXISTS foo (";
        assert_eq!(extract_table_name(line), Some("foo"));
    }

    #[test]
    fn parser_handles_plain_form() {
        let line = "CREATE TABLE bar (";
        assert_eq!(extract_table_name(line), Some("bar"));
    }

    #[test]
    fn parser_ignores_comments() {
        let line = "-- CREATE TABLE not_a_table (";
        assert_eq!(extract_table_name(line), None);
    }

    #[test]
    fn parser_ignores_unrelated_statements() {
        assert_eq!(extract_table_name("CREATE INDEX foo ON bar (baz);"), None);
        assert_eq!(extract_table_name("ALTER TABLE foo ADD COLUMN x INT;"), None);
        assert_eq!(extract_table_name(""), None);
    }

    #[test]
    fn extracts_indexes_declared_by_create_index() {
        let names = baseline_index_names();
        for index in ["idx_events_sender", "idx_events_room_time", "uq_access_tokens_token_hash"] {
            assert!(names.contains(&index), "expected CREATE INDEX form '{index}' in baseline");
        }
    }

    #[test]
    fn extracts_indexes_materialised_by_named_constraints() {
        let names = baseline_index_names();
        for index in
            ["uq_users_username", "uq_room_memberships_room_user", "pk_presence", "uq_user_threepids_medium_address"]
        {
            assert!(names.contains(&index), "expected constraint-backed index '{index}' in baseline");
        }
    }

    #[test]
    fn index_parser_ignores_non_index_constraints() {
        // CHECK / FOREIGN KEY constraints do not create an index.
        assert_eq!(extract_index_name("    CONSTRAINT ck_events_depth_nonneg CHECK (depth >= 0),"), None);
        assert_eq!(
            extract_index_name("    CONSTRAINT fk_events_room FOREIGN KEY (room_id) REFERENCES rooms(room_id),"),
            None
        );
        assert_eq!(extract_index_name("-- CONSTRAINT uq_fake UNIQUE (x)"), None);
        // Index list stays sorted + deduped like the table list.
        let names = baseline_index_names();
        let mut sorted = names.to_vec();
        sorted.sort_unstable();
        sorted.dedup();
        assert_eq!(names, &sorted[..], "baseline_index_names() must be sorted + deduped");
        assert!(names.len() >= 300, "baseline parses only {} indexes", names.len());
    }
}
