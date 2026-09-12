//! Static guard: every test-support `CREATE SCHEMA` site must have a cleanup path.
//! This module is the convergence point for the schema-lifecycle P0 audit.
//! See docs/audit/P5_test_schema_accumulation_2026-09-12.md.

use std::fs::{self, File};
use std::io::{BufRead, BufReader};
use std::path::Path;

/// Scan a path, collecting files that actually *execute* `CREATE SCHEMA` (non-comment lines)
/// and returning their paths and a simple boolean indicating whether the file already
/// references a cleanup mechanism (DROP SCHEMA, shared janitor, or legacy register).
fn find_create_schema_sites(root: &Path) -> Vec<(String, String)> {
    let mut out = Vec::new();
    if let Ok(entries) = fs::read_dir(root) {
        for entry in entries.flatten() {
            let p = entry.path();
            if p.is_dir() {
                out.extend(find_create_schema_sites(&p));
                continue;
            }
            if p.extension().and_then(|e| e.to_str()) != Some("rs") {
                continue;
            }
            if let Ok(file) = File::open(&p) {
                let reader = BufReader::new(file);
                let src: String = reader.lines().collect::<Result<Vec<_>, _>>().unwrap_or_default().join("\n");
                if !src.contains("CREATE SCHEMA") {
                    continue;
                }
                // ignore comment-only mentions
                let has_exec = src.lines().any(|l| {
                    l.contains("CREATE SCHEMA")
                        && !l.trim_start().starts_with("//")
                        && !l.trim_start().starts_with("/*")
                });
                if !has_exec {
                    continue;
                }
                out.push((p.display().to_string(), src));
            }
        }
    }
    out
}

#[test]
fn every_create_schema_site_has_a_cleanup_path() {
    let scanned = [
        "src",
        "synapse-common/src",
        "synapse-storage/src",
        "synapse-services/src",
        "synapse-e2ee/src",
        "synapse-cache/src",
        "synapse-federation/src",
    ];
    let mut total_sites = 0usize;
    let mut missing = Vec::new();

    for root in scanned {
        let root_path = std::path::Path::new(root);
        if !root_path.exists() {
            continue;
        }
        for (path, src) in find_create_schema_sites(root_path) {
            total_sites += 1;
            let has_cleanup = src.contains("DROP SCHEMA")
                || src.contains("register_schema_cleanup")
                || src.contains("test_schema_guard");
            if !has_cleanup {
                missing.push(path);
            }
        }
    }

    assert!(total_sites > 0, "expected to find CREATE SCHEMA sites; scanner is not matching");
    assert!(missing.is_empty(), "CREATE SCHEMA sites missing cleanup path:\n{:#?}", missing);
}

#[test]
fn schema_cleanup_converges_on_shared_janitor_and_blocks_private_registries() {
    // The shared engine must still implement the Weak<PgPool> + atexit + join contract.
    let engine_path = "synapse-common/src/test_schema_guard.rs";
    let engine = std::fs::read_to_string(engine_path).expect("engine file missing");
    assert!(engine.contains("Weak<PgPool>"), "engine must hold Weak<PgPool>");
    assert!(engine.contains("atexit"), "engine must register atexit handler");
    assert!(engine.contains("join()"), "engine must join cleanup threads at exit");

    // Private per-crate registries have been removed. Any *definition* of a static registry
    // is a regression.
    let scanned = [
        "src",
        "synapse-common/src",
        "synapse-storage/src",
        "synapse-services/src",
        "synapse-e2ee/src",
        "synapse-cache/src",
        "synapse-federation/src",
    ];

    for root in scanned {
        let root_path = std::path::Path::new(root);
        if !root_path.exists() {
            continue;
        }
        for entry in walk_files(root_path) {
            if entry.extension().and_then(|e| e.to_str()) != Some("rs") {
                continue;
            }
            if let Ok(src) = std::fs::read_to_string(&entry) {
                // Definitions, not comments
                if src.contains("static PENDING_SCHEMA_DROPS") || src.contains("static PENDING_SCHEMA_RETURNS") {
                    panic!("regression: private schema registry reintroduced in {}", entry.display());
                }
                // Legacy sweep APIs should not be used in code (comments are allowed)
                if has_non_comment_call(&src, "sweep_pending_schema_drops(")
                    || has_non_comment_call(&src, "schedule_pending_schema_cleanup(")
                {
                    panic!("regression: legacy sweep API used in {}", entry.display());
                }
            }
        }
    }
}

fn walk_files(root: &Path) -> Vec<std::path::PathBuf> {
    let mut out = Vec::new();
    if let Ok(entries) = fs::read_dir(root) {
        for e in entries.flatten() {
            let p = e.path();
            if p.is_dir() {
                out.extend(walk_files(&p));
            } else {
                out.push(p);
            }
        }
    }
    out
}

fn has_non_comment_call(src: &str, call: &str) -> bool {
    src.lines().any(|l| {
        if l.trim_start().starts_with("//") || l.trim_start().starts_with("/*") {
            return false;
        }
        l.contains(call)
    })
}
