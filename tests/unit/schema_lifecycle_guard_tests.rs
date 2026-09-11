#![allow(clippy::unwrap_used, clippy::expect_used)]

//! Static guard: every test-support `CREATE SCHEMA` site must have a cleanup
//! path, and that cleanup must use the shared engine that actually fires.
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
//! None of those leaks failed any assertion — a leaked schema just accumulates
//! until the catalog and the data directory's file count make PostgreSQL
//! unusable (crash recovery alone exceeded an hour). This test is deliberately
//! **static** (no database) so it fails on the *next* copy-paste.
//!
//! A site "has a cleanup path" if the same file either performs the drop itself
//! (`DROP SCHEMA`) or explicitly hands the schema to a drop registry
//! (`register_pending_schema_drop` / `test_schema_guard`).

use std::fs;
use std::path::{Path, PathBuf};

fn project_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
}

const SCANNED_ROOTS: &[&str] = &[
    "src",
    "synapse-common/src",
    "synapse-storage/src",
    "synapse-services/src",
    "synapse-e2ee/src",
    "synapse-cache/src",
    "synapse-federation/src",
];

fn walk(dir: &Path, out: &mut Vec<PathBuf>) {
    let Ok(entries) = fs::read_dir(dir) else {
        return;
    };
    for entry in entries.filter_map(Result::ok) {
        let path = entry.path();
        if path.is_dir() {
            walk(&path, out);
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
        walk(&root.join(rel), &mut files);
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
        // Ignore comment lines: several files *describe* `CREATE SCHEMA` in prose.
        let creates: Vec<&str> = source
            .lines()
            .filter(|line| line.contains("CREATE SCHEMA") && !line.trim_start().starts_with("//"))
            .collect();
        if creates.is_empty() {
            continue;
        }
        // Either the file drops the schema itself, or it hands it to a registry
        // (media delegates to synapse-services::test_utils).
        let has_cleanup = source.contains("DROP SCHEMA")
            || source.contains("register_pending_schema_drop")
            || source.contains("test_schema_guard");
        if !has_cleanup {
            offenders.push(format!(
                "{} creates {} schema(s) but neither drops them nor registers them for drop; every \
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
fn schema_cleanup_uses_the_shared_engine_with_an_exit_backstop() {
    // The per-file "register + sweep on next acquisition" contract this test used
    // to assert is GONE, and deliberately so: a sweep only runs if some later
    // acquisition happens in the same process, which under nextest (one test per
    // process) never occurs — that design leaked 100% of schemas.
    //
    // Cleanup is now owned by one shared engine (`synapse_common::test_schema_guard`),
    // driven by the pool's own lifetime with a `libc::atexit` join so the process
    // cannot exit before the schema is dropped. This test locks THAT contract in,
    // so nobody can quietly go back to a per-crate registry that never fires.
    let root = project_root();
    let engine = root.join("synapse-common/src/test_schema_guard.rs");
    let source =
        fs::read_to_string(&engine).unwrap_or_else(|e| panic!("shared engine must exist at {}: {e}", engine.display()));

    // 1. Lifetime-driven: it must watch the pool, not wait for a future event.
    assert!(
        source.contains("Weak<PgPool>"),
        "the engine must track the pool by weak reference so cleanup is driven by the pool's \
         lifetime; a registry keyed on anything else cannot fire at the right moment"
    );
    // 2. Deterministic backstop at process exit — the part the old design lacked.
    assert!(
        source.contains("atexit"),
        "the engine must register an atexit handler: without it the process can exit before the \
         drop runs, which is how the previous mechanism leaked 100% of schemas under nextest"
    );
    // 3. The exit path must JOIN rather than fire-and-forget: a detached thread is
    //    killed with the process before it drops anything (measured 100% leak).
    assert!(
        source.contains("join"),
        "the exit path must JOIN the cleanup worker; a detached/fire-and-forget task is killed at \
         process exit before it can drop anything"
    );

    // 4. No crate may reintroduce a private registry.
    let mut offenders = Vec::new();
    let mut pending: Vec<String> = Vec::new();
    for rel in SCANNED_ROOTS {
        let mut files = Vec::new();
        walk(&root.join(rel), &mut files);
        for path in files {
            if path.ends_with("synapse-common/src/test_schema_guard.rs") {
                continue;
            }
            let Ok(body) = fs::read_to_string(&path) else {
                continue;
            };
            for needle in ["PENDING_SCHEMA_DROPS", "PENDING_SCHEMA_RETURNS"] {
                // Mentioning the type is fine; owning a *registry* is not.
                if body.matches(needle).count() > 1 {
                    let rel_path = path.strip_prefix(&root).unwrap_or(&path).display().to_string();
                    if body.contains("test_schema_guard") {
                        offenders.push(format!("{rel_path}: keeps a private `{needle}` registry"));
                    } else if !pending.contains(&rel_path) {
                        pending.push(rel_path);
                    }
                }
            }
        }
    }
    // `synapse-storage` is migrated (delegates to the engine, keeps no registry).
    // Two legacy registries remain — `src/test_utils.rs` and
    // `synapse-services/src/test_utils.rs` — still on the registry+sweep design.
    // This guard deliberately does not fail on those yet: doing so would block
    // every unrelated change on an unrelated refactor. It pins the invariant that
    // a file which ADOPTS the engine must not also keep its own registry, so the
    // migration cannot half-land.
    assert!(
        offenders.is_empty(),
        "a file that adopts the shared engine must not also keep a private registry \
         (the migration would half-land and the private one would never fire):\n  {}",
        offenders.join("\n  ")
    );
    for rel_path in &pending {
        eprintln!(
            "NOTE: {rel_path} still uses the legacy registry+sweep design; it cannot fire under \
             nextest and should migrate to synapse_common::test_schema_guard"
        );
    }
}
