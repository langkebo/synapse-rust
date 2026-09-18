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

use std::collections::{BTreeSet, HashSet};
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

/// How a baseline line declares an index.
///
/// The distinction is load-bearing, not cosmetic: the baseline's own v11-10
/// cleanup block drops every **plain** `uq_*` UNIQUE index while explicitly
/// sparing the ones materialised by a `UNIQUE`/`PRIMARY KEY` **constraint**
/// (it filters on `NOT EXISTS (SELECT 1 FROM pg_constraint WHERE
/// con.conindid = c.oid)`). A name declared both ways is constraint-backed in
/// the end — `uq_access_tokens_token_hash` is a table constraint at the top of
/// the baseline and a no-op `CREATE UNIQUE INDEX IF NOT EXISTS` further down —
/// so "constraint anywhere" wins over "plain somewhere else".
#[derive(Clone, Copy, PartialEq, Eq)]
enum IndexDeclKind {
    Plain,
    Constraint,
}

/// Cached sorted list of index names the baseline leaves **in place**.
fn cached_baseline_effective_index_names() -> &'static Vec<&'static str> {
    static CACHE: OnceLock<Vec<&'static str>> = OnceLock::new();
    CACHE.get_or_init(compute_baseline_effective_index_names)
}

/// Substrings that identify the v11-10 "redundant UNIQUE index" cleanup.
///
/// That block drops indexes **dynamically** (`EXECUTE format('DROP INDEX IF
/// EXISTS %I.%I', …)` over a `pg_index` cursor), so its name set cannot be read
/// off a single line. The model below encodes its rule instead, and this
/// tripwire fails loudly if the SQL changes underneath it — a silently stale
/// model is exactly the bug this function exists to fix.
const V11_10_DYNAMIC_DROP_MARKERS: [&str; 3] =
    ["DROP INDEX IF EXISTS %I.%I", "c.relname LIKE 'uq_%'", "NOT IN ('uq_to_device_txn_msgid')"];

/// The one index the v11-10 cleanup deliberately keeps.
const V11_10_KEPT_INDEXES: [&str; 1] = ["uq_to_device_txn_msgid"];

/// Compute the index names a fresh database ends up with: every declared index
/// **minus** the ones the baseline removes later.
///
/// Why this is not just `parse_baseline_index_declarations`: `schema_health_check`'s
/// `REQUIRED_INDEXES` guard used to accept any name that appeared anywhere in the
/// baseline. The baseline is not append-only — it declares `uq_room_invites_invite_code`
/// and then the v11-10 block drops it — so that guard would have accepted a
/// `REQUIRED_INDEXES` entry naming it, and every fresh deployment would have
/// logged a `Missing indexes` warning forever. Modelling removals makes the
/// guard's verdict match the schema a deployment actually gets.
///
/// Scope and failure direction: explicit `DROP INDEX` / `DROP CONSTRAINT`
/// statements and the one modelled dynamic block are handled. If a future
/// baseline gains a *new* dynamic drop form, the tripwire above fires rather
/// than silently over-approximating again.
fn compute_baseline_effective_index_names() -> Vec<&'static str> {
    let declarations = parse_baseline_index_declarations();

    let mut constraint_backed: HashSet<&'static str> = HashSet::new();
    let mut declared: BTreeSet<&'static str> = BTreeSet::new();
    for (name, kind) in declarations {
        if kind == IndexDeclKind::Constraint {
            constraint_backed.insert(name);
        }
        declared.insert(name);
    }

    let removed = parse_baseline_removed_index_names();
    let dynamic_cleanup_is_present = V11_10_DYNAMIC_DROP_MARKERS.iter().all(|marker| BASELINE_SQL.contains(marker));
    assert!(
        dynamic_cleanup_is_present,
        "the v11-10 dynamic `DROP INDEX` cleanup (markers {V11_10_DYNAMIC_DROP_MARKERS:?}) is no longer \
         present in the baseline. Either it was removed — in which case delete the modelled rule and \
         `V11_10_KEPT_INDEXES` below — or its SQL changed and this model is now stale (it would \
         silently claim dropped indexes still exist, which is the exact defect this reference must \
         not have)."
    );

    declared
        .into_iter()
        .filter(|name| {
            if removed.contains(name) {
                return false;
            }
            // v11-10: plain `uq_*` indexes lose to their constraint-backed twins.
            let dropped_by_v11_10 =
                name.starts_with("uq_") && !constraint_backed.contains(name) && !V11_10_KEPT_INDEXES.contains(name);
            !dropped_by_v11_10
        })
        .collect()
}

/// Names the baseline declares and then removes, from explicit statements:
/// `DROP INDEX [CONCURRENTLY] [IF EXISTS] <name>` and
/// `ALTER TABLE … DROP CONSTRAINT [IF EXISTS] <name>`.
///
/// Matched on the trimmed line, so both top-level statements and indented ones
/// inside `DO $$ … $$` blocks are found. A dynamic `EXECUTE format('DROP INDEX …')`
/// deliberately does not match here — that is the modelled rule above.
///
/// `IF EXISTS` is optional in both forms: treating its absence as "no name" would
/// silently keep a dropped index in the reference set, which is the unsafe
/// direction (the guard would then bless a `REQUIRED_INDEXES` entry no deployment
/// can satisfy). `parse_removed_index_names` is separated out so that this is
/// covered by a unit test on synthetic SQL.
fn parse_baseline_removed_index_names() -> HashSet<&'static str> {
    parse_removed_index_names(BASELINE_SQL)
}

fn parse_removed_index_names(sql: &'static str) -> HashSet<&'static str> {
    let mut removed: HashSet<&'static str> = HashSet::new();
    for line in sql.lines() {
        let trimmed = line.trim_start();
        if trimmed.starts_with("--") {
            continue;
        }
        if let Some(rest) = trimmed.strip_prefix("DROP INDEX ") {
            let rest = rest.strip_prefix("CONCURRENTLY ").unwrap_or(rest);
            let rest = rest.strip_prefix("IF EXISTS ").unwrap_or(rest);
            if let Some(name) = first_token(rest) {
                removed.insert(name);
            }
            continue;
        }
        if let Some(at) = trimmed.find("DROP CONSTRAINT ") {
            let rest = &trimmed[at + "DROP CONSTRAINT ".len()..];
            let rest = rest.strip_prefix("IF EXISTS ").unwrap_or(rest);
            if let Some(name) = first_token(rest) {
                removed.insert(name);
            }
        }
    }
    removed
}

/// Parse every index the baseline declares, with the provenance the v11-10
/// cleanup decision needs.
fn parse_baseline_index_declarations() -> Vec<(&'static str, IndexDeclKind)> {
    BASELINE_SQL.lines().filter_map(extract_index_declaration).collect()
}

/// Extract the created index from a single line, if the line creates one.
/// Extract the created index from a single line, if the line creates one.
///
/// Rejected forms (returning `None`) include comments, `CONSTRAINT <name> CHECK
/// (...)`, and `CONSTRAINT <name> FOREIGN KEY (...)`: those do not create an
/// index.
///
/// Also rejected: the named table-level **alter** form `ALTER TABLE <t> ADD
/// CONSTRAINT <name> PRIMARY KEY|UNIQUE (...)`. Postgres does build an index for
/// it, but this line-based parser only recognises `CREATE [UNIQUE] INDEX ...`
/// and in-`CREATE TABLE` `CONSTRAINT <name> UNIQUE|PRIMARY KEY (...)`. The
/// baseline has two such lines (`migrations/00000000_unified_schema_v12.sql:4164`
/// `pk_typing`, `:4175` `pk_presence_subscriptions`); both names are also
/// declared inside their `CREATE TABLE`, so nothing is lost today. The failure
/// direction is safe — an unparsed index makes `REQUIRED_INDEXES` report a
/// missing index rather than silently skipping a check.
///
/// Two declaration forms materialise an index in Postgres, and
/// `schema_health_check::check_missing_indexes` observes both through
/// `pg_indexes`:
///
///   1. `CREATE [UNIQUE] INDEX [CONCURRENTLY] IF NOT EXISTS <name>` → [`IndexDeclKind::Plain`].
///   2. A named table-constraint index: `CONSTRAINT <name> UNIQUE (...)` or
///      `CONSTRAINT <name> PRIMARY KEY (...)` → [`IndexDeclKind::Constraint`].
///
/// Unnamed inline `PRIMARY KEY` / `UNIQUE` columns (whose index Postgres names
/// `<table>_pkey` / `<table>_<column>_key`) are deliberately not enumerated: no
/// value in `REQUIRED_INDEXES` relies on that form. The parser is line-based,
/// mirroring [`parse_baseline_tables`].
fn extract_index_declaration(line: &'static str) -> Option<(&'static str, IndexDeclKind)> {
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
            return first_token(rest).map(|name| (name, IndexDeclKind::Plain));
        }
    }

    let rest = trimmed.strip_prefix("CONSTRAINT ")?;
    let name = first_token(rest)?;
    let tail = rest[name.len()..].trim_start();
    if tail.starts_with("UNIQUE") || tail.starts_with("PRIMARY KEY") {
        Some((name, IndexDeclKind::Constraint))
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

/// Return the sorted, deduplicated list of index names a fresh database ends up
/// with — every index the baseline declares, **minus** the ones it later drops
/// (see [`compute_baseline_effective_index_names`]), including indexes
/// materialised by named `UNIQUE` / `PRIMARY KEY` constraints.
///
/// Used as the compile-time reference for `REQUIRED_INDEXES` so a required index
/// that no deployment will have fails a unit test instead of printing a
/// spurious `Missing indexes` warning at every startup.
///
/// The removal modelling is the point: reading declarations alone would accept
/// `uq_room_invites_invite_code`, which the baseline declares at
/// `migrations/…v12.sql:3390` and the v11-10 cleanup block then drops.
pub fn baseline_index_names() -> &'static [&'static str] {
    cached_baseline_effective_index_names()
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
        assert!(extract_index_declaration("    CONSTRAINT ck_events_depth_nonneg CHECK (depth >= 0),").is_none());
        assert!(extract_index_declaration(
            "    CONSTRAINT fk_events_room FOREIGN KEY (room_id) REFERENCES rooms(room_id),"
        )
        .is_none());
        assert!(extract_index_declaration("-- CONSTRAINT uq_fake UNIQUE (x)").is_none());
        // Index list stays sorted + deduped like the table list.
        let names = baseline_index_names();
        let mut sorted = names.to_vec();
        sorted.sort_unstable();
        sorted.dedup();
        assert_eq!(names, &sorted[..], "baseline_index_names() must be sorted + deduped");
        assert!(names.len() >= 300, "baseline parses only {} indexes", names.len());
    }

    /// The removal parser must handle both spellings of `IF EXISTS`, stay blind to
    /// comments, and — importantly — **not** treat the dynamic
    /// `EXECUTE format('DROP INDEX …')` line as an explicit drop (that one is
    /// modelled separately, by rule). Dropping the `IF EXISTS`-optional handling
    /// here means a removed index stays in the reference set, and the
    /// `REQUIRED_INDEXES` guard then blesses an entry no deployment can satisfy.
    #[test]
    fn removal_parser_handles_both_spellings_and_ignores_dynamic_drops() {
        const SYNTHETIC: &str = "\
DROP INDEX IF EXISTS a_idx;
DROP INDEX CONCURRENTLY IF EXISTS b_idx;
DROP INDEX c_idx;
    DROP INDEX d_idx;
ALTER TABLE t DROP CONSTRAINT IF EXISTS e_uq;
ALTER TABLE t DROP CONSTRAINT f_uq;
-- DROP INDEX IF EXISTS commented_idx;
        EXECUTE format('DROP INDEX IF EXISTS %I.%I', s, n);
";
        let removed = parse_removed_index_names(SYNTHETIC);
        for expected in ["a_idx", "b_idx", "c_idx", "d_idx", "e_uq", "f_uq"] {
            assert!(removed.contains(&expected), "`{expected}` should be parsed as removed: {removed:?}");
        }
        assert!(!removed.contains(&"commented_idx"), "a commented-out DROP must not count as a removal: {removed:?}");
        assert!(
            !removed.contains(&"%I"),
            "the dynamic `EXECUTE format('DROP INDEX …')` is modelled by rule, not by this parser: {removed:?}"
        );
    }

    /// D6: the reference set must exclude indexes the baseline **drops**, not just
    /// count the ones it declares.
    ///
    /// `uq_room_invites_invite_code` is declared at
    /// `migrations/00000000_unified_schema_v12.sql:3390` as a plain UNIQUE index and
    /// then removed by the v11-10 cleanup block, so a fresh database never has it.
    /// Asserting both halves — *declared* and *not effective* — is what makes this
    /// guard meaningful: if a future refactor reverts to the naive "declared
    /// anywhere" set, the second assertion fails.
    #[test]
    fn declared_but_dropped_indexes_are_not_in_the_reference_set() {
        let declared = parse_baseline_index_declarations();
        assert!(
            declared.iter().any(|(name, _)| *name == "uq_room_invites_invite_code"),
            "the baseline is expected to declare uq_room_invites_invite_code; if that changed, \
             update this test (and the v11-10 model) rather than deleting it"
        );
        assert!(
            !baseline_index_names().contains(&"uq_room_invites_invite_code"),
            "uq_room_invites_invite_code is declared and then dropped by the v11-10 cleanup block, \
             so no deployment has it; the reference set must not contain it (DB_REVIEW §15.3 D6)"
        );
    }

    /// The v11-10 block spares constraint-backed `uq_*` indexes, so a name
    /// declared both as a constraint and as a (no-op) plain index must survive.
    /// `uq_access_tokens_token_hash` is exactly that shape — constraint at
    /// `:172`, `CREATE UNIQUE INDEX IF NOT EXISTS` at `:3791` — and dropping it
    /// from the reference set would make the `REQUIRED_INDEXES` guard fail on a
    /// perfectly valid entry.
    #[test]
    fn constraint_backed_indexes_survive_the_dynamic_cleanup() {
        let names = baseline_index_names();
        for index in ["uq_access_tokens_token_hash", "uq_refresh_tokens_token_hash", "uq_token_blacklist_token_hash"] {
            assert!(
                names.contains(&index),
                "{index} is backed by a UNIQUE constraint, which the v11-10 block explicitly spares"
            );
        }
        assert!(
            names.contains(&"uq_to_device_txn_msgid"),
            "uq_to_device_txn_msgid is the modelled exception (to-device dedup ON CONFLICT depends on it)"
        );
    }
}
