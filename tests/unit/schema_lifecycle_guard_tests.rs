//! Static guard: every test-support `CREATE SCHEMA` site must have a cleanup path.
//! This module is the convergence point for the schema-lifecycle P0 audit.
//! See docs/audit/P5_test_schema_accumulation_2026-09-12.md.

use std::fs::{self, File};
use std::io::{BufRead, BufReader};
use std::path::{Path, PathBuf};

/// The only roots that hold test-support Rust code allowed to create schemas.
///
/// Both guards below must scan the same tree, so the list lives in exactly one
/// place: a second literal copy is how `synapse-test-utils/src` (the shared
/// harness itself) and `tests/` (the suites that hand-roll their own pools)
/// stayed outside the guard while `total_sites > 0` kept it green.
const SCAN_ROOTS: &[&str] = &[
    "src",
    "synapse-common/src",
    "synapse-storage/src",
    "synapse-services/src",
    "synapse-e2ee/src",
    "synapse-cache/src",
    "synapse-federation/src",
    "synapse-test-utils/src",
    "tests",
];

/// [`SCAN_ROOTS`] must keep this many entries. Pinned so that deleting a root —
/// including one that currently contributes no site — fails the test instead of
/// silently narrowing coverage.
const EXPECTED_SCAN_ROOT_COUNT: usize = 9;

/// Files that provably execute the marker on 2026-09-21, measured by this
/// scanner. Every one must still be found: dropping a scan root or breaking the
/// matcher removes a known site and turns the guard red, instead of silently
/// shrinking coverage to "found at least one file somewhere".
const KNOWN_CREATE_SCHEMA_SITES: [&str; 6] = [
    "synapse-common/src/test_isolation.rs",
    "synapse-storage/src/test_utils.rs",
    "synapse-services/src/test_utils.rs",
    "synapse-test-utils/src/lib.rs",
    "tests/unit/migration_search_path_tests.rs",
    "tests/unit/test_schema_housekeeping_tests.rs",
];

/// Lower bound on the number of matched files: exactly the measured count, so a
/// matcher regression that still finds *something* cannot pass.
const MIN_CREATE_SCHEMA_SITES: usize = KNOWN_CREATE_SCHEMA_SITES.len();

/// Assembled with `concat!` on purpose: `tests/` is itself a scan root now, and
/// this file must not look like a schema-creating site merely because it holds
/// the matcher. Comment-only mentions are skipped separately.
const CREATE_SCHEMA_MARKER: &str = concat!("CREATE", " SCHEMA");

/// Same `concat!` reason as [`CREATE_SCHEMA_MARKER`]: the registry/legacy-API
/// literals would otherwise match this guard file's own matcher source.
const PRIVATE_REGISTRY_MARKERS: [&str; 2] =
    [concat!("static PENDING", "_SCHEMA_DROPS"), concat!("static PENDING", "_SCHEMA_RETURNS")];
const LEGACY_SWEEP_CALLS: [&str; 2] =
    [concat!("sweep_pending", "_schema_drops("), concat!("schedule_pending", "_schema_cleanup(")];

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
                if !src.contains(CREATE_SCHEMA_MARKER) {
                    continue;
                }
                // ignore comment-only mentions
                let has_exec = src.lines().any(|l| {
                    l.contains(CREATE_SCHEMA_MARKER)
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
    assert_eq!(
        SCAN_ROOTS.len(),
        EXPECTED_SCAN_ROOT_COUNT,
        "SCAN_ROOTS was edited: removing a root must be a deliberate, justified change — a root \
         that currently contributes no site would otherwise be dropped silently"
    );

    let mut total_sites = 0usize;
    let mut missing = Vec::new();
    let mut found: Vec<PathBuf> = Vec::new();

    for root in SCAN_ROOTS {
        let root_path = Path::new(root);
        // A root that silently disappears (renamed crate, deleted directory)
        // must fail here: the old `if !root_path.exists() { continue; }` turned
        // "we stopped scanning" into a green check.
        assert!(
            root_path.is_dir(),
            "scan root `{root}` must exist; if the tree moved, move it in SCAN_ROOTS too — \
             skipping a root silently shrinks this guard"
        );
        for (path, src) in find_create_schema_sites(root_path) {
            total_sites += 1;
            found.push(PathBuf::from(&path));
            let has_cleanup = src.contains("DROP SCHEMA")
                || src.contains("register_schema_cleanup")
                || src.contains("test_schema_guard");
            if !has_cleanup {
                missing.push(path);
            }
        }
    }

    // Non-vacuity is per known site and in aggregate, not `> 0`: the guard must
    // prove it still covers the code it was written for.
    for known in KNOWN_CREATE_SCHEMA_SITES {
        assert!(
            found.iter().any(|p| p.ends_with(known)),
            "known `{CREATE_SCHEMA_MARKER}` site `{known}` was not found — a scan root was dropped \
             or the matcher broke. Found:\n{found:#?}"
        );
    }
    assert!(
        total_sites >= MIN_CREATE_SCHEMA_SITES,
        "expected >= {MIN_CREATE_SCHEMA_SITES} `{CREATE_SCHEMA_MARKER}` sites, found {total_sites}; \
         scan roots or matcher are broken"
    );
    assert!(missing.is_empty(), "`{CREATE_SCHEMA_MARKER}` sites missing cleanup path:\n{:#?}", missing);
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
    // is a regression. Same roots as `every_create_schema_site_has_a_cleanup_path`.
    for root in SCAN_ROOTS {
        let root_path = Path::new(root);
        assert!(root_path.is_dir(), "scan root `{root}` must exist");
        for entry in walk_files(root_path) {
            if entry.extension().and_then(|e| e.to_str()) != Some("rs") {
                continue;
            }
            if let Ok(src) = std::fs::read_to_string(&entry) {
                // Definitions, not comments
                if PRIVATE_REGISTRY_MARKERS.iter().any(|marker| src.contains(*marker)) {
                    panic!("regression: private schema registry reintroduced in {}", entry.display());
                }
                // Legacy sweep APIs should not be used in code (comments are allowed)
                if LEGACY_SWEEP_CALLS.iter().any(|call| has_non_comment_call(&src, call)) {
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
