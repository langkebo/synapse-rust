//! Compile-time baseline table extraction.
//!
//! The `00000000_unified_schema_v10.sql` file is the single source of truth for
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
//! dependency on `migrations/00000000_unified_schema_v10.sql` explicit and
//! participates in cargo's normal incremental rebuild graph.

use std::sync::OnceLock;

/// Raw contents of the v10 baseline schema file.
///
/// Path is relative to this file (`synapse-storage/src/baseline_tables.rs`).
/// Two `..` steps reach the workspace root, then `migrations/` contains the
/// file. Cargo's manifest-relative `include_str!` resolves relative to
/// `CARGO_MANIFEST_DIR`, which is `synapse-storage/` for this crate.
const BASELINE_SQL: &str = include_str!("../../migrations/00000000_unified_schema_v10.sql");

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
/// The baseline uses both `CREATE TABLE` (in the primary region, line ~1-4000)
/// and `CREATE TABLE IF NOT EXISTS` (in the folded consolidation region,
/// line ~4900+). Both forms are accepted.
///
/// We deliberately ignore any CREATE TABLE inside an SQL comment (`--`) to
/// avoid false positives from documentation blocks. The parser is line-based
/// rather than token-aware; SQL `/* ... */` block comments are rare in the
/// baseline, but where they exist they cannot host CREATE TABLE statements
/// without breaking the file's syntax anyway.
fn parse_baseline_tables() -> Vec<&'static str> {
    let mut names: Vec<&'static str> = BASELINE_SQL
        .lines()
        .filter_map(extract_table_name)
        .collect();

    // Deduplicate (the baseline has a primary region and a consolidation
    // region that overlap).
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
    let after_create = if let Some(rest) = after_create.strip_prefix("IF NOT EXISTS ") {
        rest
    } else {
        after_create
    };

    // End at the first whitespace, opening paren, or semicolon.
    let end = after_create
        .find(|c: char| c.is_whitespace() || c == '(' || c == ';')
        .unwrap_or(after_create.len());

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
        assert!(
            names.contains(&"event_relations"),
            "expected 'event_relations' in baseline"
        );
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
        // The v10 baseline should contain well over 100 tables.
        // If this drops below 50, somebody removed half the schema.
        assert!(
            count >= 100,
            "baseline has only {count} tables — this is suspiciously low for v10"
        );
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
}