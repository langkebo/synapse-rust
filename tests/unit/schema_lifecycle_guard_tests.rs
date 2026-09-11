#![allow(clippy::unwrap_used, clippy::expect_used)]

//! Static guard: every test-support `CREATE SCHEMA` site must have a cleanup
//! path.
//!
//! ## Why this test exists
//!
//! The local test database reached **23,662 leftover schemas** because the
//! schema-lifecycle helper was copy-pasted into *four* places and only one copy
//! kept the cleanup half
//! (`docs/audit/P5_test_schema_accumulation_2026-09-12.md`):
//!
//! | file | had cleanup? |
//! |---|---|
//! | `src/test_utils.rs` (shared path) | yes |
//! | `src/test_utils.rs` (isolated path) | no — fixed 2026-09-12 |
//! | `synapse-services/src/test_utils.rs` | no — fixed 2026-09-12 |
//! | `synapse-storage/src/test_utils.rs` | no — fixed 2026-09-12 |
//! | `synapse-services/src/media/mod.rs` | no — fixed 2026-09-12 |
//!
//! Every one of those leaks was invisible to the existing test suite, because a
//! leaked schema does not fail any assertion — it just accumulates until the
//! catalog and the data directory's file count make PostgreSQL unusable (crash
//! recovery alone took >1h once the directory held millions of files).
//!
//! This test is deliberately **static** (no database) so it runs in the fast
//! gate and fails on the *next* copy-paste, not six months later.
//!
//! A site "has a cleanup path" if the same file either performs the drop itself
//! (`DROP SCHEMA`) or explicitly hands the schema to a drop registry
//! (`register_pending_schema_drop`). A bare `CREATE SCHEMA` with neither is
//! exactly the shape of all four leaks.

use std::fs;
use std::path::{Path, PathBuf};

fn project_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
}

/// Crates whose `test_utils` / test-support modules create schemas.
const SCANNED_ROOTS: &[&str] = &[
    "src",
    "synapse-common/src",
    "synapse-storage/src",
    "synapse-services/src",
    "synapse-e2ee/src",
    "synapse-cache/src",
    "synapse-federation/src",
];

fn rust_files_under(root: &Path, out: &mut Vec<PathBuf>) {
    let Ok(entries) = fs::read_dir(root) else {
        return;
    };
    for entry in entries.filter_map(Result::ok) {
        let path = entry.path();
        if path.is_dir() {
            rust_files_under(&path, out);
        } else if path.extension().is_some_and(|ext| ext == "rs") {
            out.push(path);
        }
    }
}

#[test]
fn every_create_schema_site_has_a_drop_path() {
    let root = project_root();
    let mut files = Vec::new();
    for rel in SCANNED_ROOTS {
        rust_files_under(&root.join(rel), &mut files);
    }
    assert!(!files.is_empty(), "scanner found no .rs files — the scan roots are wrong");

    let mut offenders = Vec::new();
    let mut sites = 0usize;
    for path in files {
        let Ok(source) = fs::read_to_string(&path) else {
            continue;
        };
        if !source.contains("CREATE SCHEMA") {
            continue;
        }
        sites += 1;
        // Ignore comment lines: several files *describe* `CREATE SCHEMA` in
        // prose (this is the same lesson the fixture guard learned).
        let creates: Vec<&str> = source
            .lines()
            .filter(|line| line.contains("CREATE SCHEMA") && !line.trim_start().starts_with("//"))
            .collect();
        if creates.is_empty() {
            continue;
        }
        // Either the file drops the schema itself, or it hands it to a drop
        // registry (media delegates to synapse-services::test_utils).
        let has_cleanup = source.contains("DROP SCHEMA") || source.contains("register_pending_schema_drop");
        if !has_cleanup {
            offenders.push(format!(
                "{} creates {} schema(s) but neither drops them nor registers them for drop — every \
                 such site leaked in 2026-09 (see docs/audit/P5_test_schema_accumulation_2026-09-12.md)",
                path.strip_prefix(&root).unwrap_or(&path).display(),
                creates.len()
            ));
        }
    }

    assert!(sites > 0, "expected to find CREATE SCHEMA sites; scanner is not matching");
    assert!(offenders.is_empty(), "schema-creating test support with no cleanup:\n  {}", offenders.join("\n  "));
}

#[test]
fn schema_drop_sweeps_are_actually_called_on_acquisition() {
    let root = project_root();
    // Registering a drop is useless if nothing ever sweeps. Each file that
    // registers a pending drop must also call a sweep somewhere.
    let expectations: &[(&str, &str)] = &[
        ("src/test_utils.rs", "schedule_pending_schema_cleanup"),
        ("synapse-services/src/test_utils.rs", "sweep_pending_schema_drops"),
        ("synapse-storage/src/test_utils.rs", "sweep_pending_schema_drops"),
    ];

    let mut offenders = Vec::new();
    for (rel, sweep) in expectations {
        let path = root.join(rel);
        let source = fs::read_to_string(&path).unwrap_or_else(|e| panic!("failed to read {}: {e}", path.display()));
        let registers = source.matches("register_pending_schema").count();
        let sweeps = source.matches(sweep).count();
        if registers == 0 {
            offenders.push(format!("{rel}: expected to register pending schema drops, found none"));
        }
        if sweeps < 2 {
            // One definition + at least one call site.
            offenders.push(format!(
                "{rel}: `{sweep}` appears {sweeps}x — a registered drop with no sweep never runs, \
                 which is how the isolated path leaked while the shared path did not"
            ));
        }
    }
    assert!(offenders.is_empty(), "pending-drop registration without a sweep:\n  {}", offenders.join("\n  "));
}
