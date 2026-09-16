#![allow(clippy::unwrap_used, clippy::expect_used)]

//! Static guard: an incremental migration must not re-add a column the v11
//! baseline already creates, unless it is idempotent (`IF NOT EXISTS`).
//!
//! ## The defect this locks down
//!
//! On 2026-09-12, replaying the full chain into a **fresh** database failed:
//!
//! ```text
//! [INFO] 应用迁移: 20260906010000_add_events_soft_failed.sql
//! ERROR:  column "soft_failed" of relation "events" already exists
//! [ERROR] 迁移失败: 20260906010000_add_events_soft_failed.sql
//! ```
//!
//! `soft_failed` had been folded into `00000000_unified_schema_v12.sql` (as
//! `ALTER TABLE events ADD COLUMN IF NOT EXISTS soft_failed ...`, i.e. idempotent),
//! but the incremental migration still used a bare `ADD COLUMN`. The chain aborted
//! at migration 30 of 74, so **every later migration silently never ran** on any
//! clean database.
//!
//! It stayed invisible locally because existing databases already had the
//! `schema_migrations` row, so the file was skipped entirely. It would only bite a
//! clean CI database, a new deployment, or a rebuilt test database — the same
//! "dual source of truth" shape P3 targets.
//!
//! ## The rule
//!
//! For every `ALTER TABLE <t> ADD COLUMN <c>` in an incremental migration, if the
//! baseline already declares column `<c>` on table `<t>`, the statement must be
//! guarded against re-adding it. Two guard styles are used in this repo and both
//! are accepted:
//!
//! 1. inline — `ALTER TABLE t ADD COLUMN IF NOT EXISTS c ...`;
//! 2. `DO $$ ... IF NOT EXISTS (SELECT 1 FROM information_schema.columns ...)
//!    THEN ALTER TABLE t ADD COLUMN c ... END IF; END $$;`
//!
//! Anything else is a violation. The check is deliberately *static* (no database)
//! so it runs in the fast gate rather than only when someone happens to replay a
//! fresh chain.

use std::collections::HashSet;
use std::fs;
use std::path::{Path, PathBuf};

const BASELINE: &str = "00000000_unified_schema_v12.sql";

fn project_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
}

fn read(path: &Path) -> String {
    fs::read_to_string(path).unwrap_or_else(|error| panic!("failed to read {}: {error}", path.display()))
}

/// Collect `table.column` pairs declared in the baseline, from both
/// `CREATE TABLE t (... col TYPE ...)` blocks and `ALTER TABLE t ADD COLUMN col`.
fn baseline_columns(baseline: &str) -> HashSet<String> {
    let mut out = HashSet::new();

    // 1. ALTER TABLE <t> ADD COLUMN [IF NOT EXISTS] <c>
    for line in baseline.lines() {
        let trimmed = line.trim();
        if trimmed.starts_with("--") {
            continue;
        }
        if let Some(pair) = parse_add_column(trimmed) {
            out.insert(pair);
        }
    }

    // 2. CREATE TABLE [IF NOT EXISTS] <t> ( ... )
    //    Track the current table and collect leading column names until the block
    //    closes. Column lines look like `<name> <TYPE> ...`; constraint lines begin
    //    with a known keyword and are skipped.
    let mut current: Option<String> = None;
    for raw in baseline.lines() {
        let line = raw.split("--").next().unwrap_or("");
        let trimmed = line.trim();
        if current.is_none() {
            if let Some(rest) = find_ci(trimmed, "CREATE TABLE") {
                let rest = rest.trim_start().trim_start_matches("IF NOT EXISTS").trim_start();
                if let Some(name) = ident(rest) {
                    current = Some(name);
                }
            }
            continue;
        }
        if trimmed.starts_with(')') {
            current = None;
            continue;
        }
        let lower = trimmed.to_ascii_lowercase();
        const KEYWORDS: [&str; 6] = ["constraint", "primary", "foreign", "unique", "check", "exclude"];
        if KEYWORDS.iter().any(|k| lower.starts_with(k)) {
            continue;
        }
        if let Some(col) = ident(trimmed) {
            if let Some(table) = &current {
                out.insert(format!("{table}.{col}"));
            }
        }
    }

    out
}

fn find_ci<'a>(haystack: &'a str, needle: &str) -> Option<&'a str> {
    let idx = haystack.to_ascii_uppercase().find(&needle.to_ascii_uppercase())?;
    Some(&haystack[idx + needle.len()..])
}

fn ident(text: &str) -> Option<String> {
    let text = text.trim_start();
    let text = text.strip_prefix('"').unwrap_or(text);
    let name: String = text.chars().take_while(|c| c.is_ascii_alphanumeric() || *c == '_').collect();
    if name.is_empty() {
        None
    } else {
        Some(name.to_ascii_lowercase())
    }
}

/// Parse `ALTER TABLE <t> ADD COLUMN [IF NOT EXISTS] <c>` → `t.c`, plus whether it
/// was guarded. Returns `None` for anything else.
fn parse_add_column(statement: &str) -> Option<String> {
    let statement = statement.trim().trim_end_matches(';').trim();
    let rest = find_ci(statement, "ALTER TABLE")?;
    let rest = rest.trim_start();
    let table = ident(rest)?;
    let after_table = rest[table.len().min(rest.len())..].trim_start();
    let after_table = after_table.strip_prefix('"').unwrap_or(after_table);
    let rest = find_ci(after_table, "ADD COLUMN")?;
    let rest = rest.trim_start();
    let rest = rest.strip_prefix("IF NOT EXISTS").unwrap_or(rest).trim_start();
    let column = ident(rest)?;
    Some(format!("{table}.{column}"))
}

/// Classify each `ADD COLUMN` site in a migration.
///
/// Returns `(line_no, table.column, guarded)` for every `ADD COLUMN` found.
/// `guarded` is true when the statement either carries `IF NOT EXISTS` itself or
/// sits inside a `DO` block whose preceding lines perform an
/// `information_schema.columns` existence check — the two idioms this repo uses.
fn add_column_sites(sql: &str) -> Vec<(usize, String, bool)> {
    let lines: Vec<&str> = sql.lines().collect();
    let mut out = Vec::new();
    // Index of the most recent `DO $$` opener, if we are inside one.
    let mut do_block_start: Option<usize> = None;

    for (index, line) in lines.iter().enumerate() {
        let stripped = line.split("--").next().unwrap_or("");
        let upper = stripped.to_ascii_uppercase();

        if upper.contains("DO $$") || upper.contains("DO $") {
            do_block_start = Some(index);
        }
        if do_block_start.is_some() && (upper.trim_start().starts_with("END $$") || upper.contains("END $$;")) {
            do_block_start = None;
            continue;
        }
        if !upper.contains("ADD COLUMN") {
            continue;
        }
        let Some(pair) = parse_add_column(stripped) else {
            continue;
        };
        let inline_guard = match (upper.find("ADD COLUMN"), upper.find("IF NOT EXISTS")) {
            (Some(add), Some(ine)) => ine > add,
            _ => false,
        };
        // Guarded by a wrapping DO block that checks information_schema first.
        let do_guard = match do_block_start {
            Some(start) => lines[start..index].iter().any(|l| l.to_ascii_uppercase().contains("IF NOT EXISTS")),
            None => false,
        };
        out.push((index + 1, pair, inline_guard || do_guard));
    }
    out
}

#[test]
fn incremental_add_column_must_be_idempotent_when_baseline_has_the_column() {
    let root = project_root();
    let migrations = root.join("migrations");
    let baseline = read(&migrations.join(BASELINE));
    let existing = baseline_columns(&baseline);
    assert!(existing.len() > 1000, "baseline column extraction looks broken: {}", existing.len());

    let mut entries: Vec<_> = fs::read_dir(&migrations)
        .expect("migrations dir readable")
        .filter_map(Result::ok)
        .map(|e| e.path())
        .filter(|p| {
            p.extension().is_some_and(|e| e == "sql")
                && p.file_name().is_some_and(|n| n != BASELINE)
                // `.undo.sql` files are rollback scripts, not part of the forward chain.
                && !p.file_name().unwrap().to_string_lossy().ends_with(".undo.sql")
        })
        .collect();
    entries.sort();

    let mut violations = Vec::new();
    for path in entries {
        let sql = read(&path);
        for (line_no, pair, guarded) in add_column_sites(&sql) {
            if existing.contains(&pair) && !guarded {
                violations.push(format!(
                    "{}:{} — `ADD COLUMN {}` is unguarded, but the v11 baseline already declares it. \
                     A fresh database cannot replay the chain past this file",
                    path.strip_prefix(&root).unwrap_or(&path).display(),
                    line_no,
                    pair
                ));
            }
        }
    }

    assert!(
        violations.is_empty(),
        "non-idempotent ADD COLUMN conflicting with the baseline:\n  {}",
        violations.join("\n  ")
    );
}
