//! Guards the single test-database-port convention (H-12).
//!
//! The harness used to disagree with itself. Five separate fallback chains
//! existed, and two of them preferred `localhost:15432` — a port from an older
//! compose file that nothing has listened on since
//! `docker-compose.dev-host-access.yml` started publishing
//! `${DB_EXPOSE_PORT:-5432}:5432`. The cost was a failed connect probe in every
//! DB-backed test process before falling through, and the four chains that did
//! *not* know about 15432 silently drifted from the two that did.
//!
//! The old chains also offered the *application* database (`…:5432/synapse`) as
//! a fallback. A harness that silently falls back to the database it is
//! supposed to be isolating itself from turns a missing configuration into data
//! corruption — the same defect `P0-4` names for CI, in local form.
//!
//! These are all properties of the source text, so they are guarded statically
//! and need no database.

#![allow(clippy::unwrap_used, clippy::expect_used)]

use std::fs;
use std::path::PathBuf;

/// Every URL a local fallback chain is allowed to contain, in order.
///
/// Both are `synapse_test` on `5432`; they differ only in the password used by
/// older local setups.
const CANONICAL_FALLBACKS: [&str; 2] = [
    "postgresql://synapse:synapse@localhost:5432/synapse_test",
    "postgresql://synapse:secret@localhost:5432/synapse_test",
];

/// Every Rust file that resolves a test-database target.
const RUST_RESOLVERS: [&str; 5] = [
    "src/test_utils.rs",
    "synapse-services/src/test_utils.rs",
    "synapse-storage/src/test_utils.rs",
    "synapse-storage/src/test_isolation.rs",
    "tests/common/mod.rs",
];

/// Shell entry points that resolve a test-database target, with the port
/// default each one must carry.
const SCRIPT_PORT_DEFAULTS: [(&str, &str); 6] = [
    ("scripts/init_test_public_schema.sh", "TEST_DB_PORT:-5432"),
    ("scripts/cleanup_test_schemas.sh", "PGPORT:-5432"),
    ("scripts/tune_test_db.sh", "PGPORT:-5432"),
    ("scripts/seed_test_db.sh", "DB_PORT:-5432"),
    ("scripts/run_bench_server.sh", "BENCH_DB_PORT:-5432"),
    ("scripts/run_local_coverage.sh", "localhost:5432/synapse_test"),
];

fn read(relative: &str) -> String {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join(relative);
    fs::read_to_string(&path).unwrap_or_else(|error| panic!("failed to read {}: {error}", path.display()))
}

/// True for lines that carry executable text (i.e. not a whole-line comment).
///
/// Comments are allowed to narrate the dead port; code is not allowed to use
/// it. This is what keeps the guard from being satisfied by deleting the
/// explanation instead of fixing the chain.
fn is_code_line(line: &str) -> bool {
    let trimmed = line.trim_start();
    !(trimmed.is_empty()
        || trimmed.starts_with("//")
        || trimmed.starts_with('#')
        || trimmed.starts_with('*')
        || trimmed.starts_with("/*"))
}

/// The ordered fallback chain of a Rust resolver: the URL literals inside its
/// `for fallback in [ … ]` / `for candidate in [ … ]` array.
fn fallback_chain(source: &str) -> Vec<String> {
    let mut chain = Vec::new();
    let mut inside = false;
    for line in source.lines() {
        if !inside {
            if line.contains("for fallback in [") || line.contains("for candidate in [") {
                inside = true;
            }
            continue;
        }
        if line.contains("] {") {
            break;
        }
        if let Some(start) = line.find("\"postgresql://") {
            if let Some(end) = line[start + 1..].find('"') {
                chain.push(line[start + 1..start + 1 + end].to_string());
            }
        }
    }
    chain
}

/// The convention violations in `source`.
///
/// Deliberately pure and total so `the_checker_rejects_the_old_chain` can feed
/// it the pre-fix text and prove the predicate actually bites — otherwise every
/// assertion in this file could pass by returning nothing.
fn violations(label: &str, source: &str) -> Vec<String> {
    let mut found = Vec::new();
    for (index, line) in source.lines().enumerate() {
        if !is_code_line(line) {
            continue;
        }
        let position = format!("{label}:{}", index + 1);
        if line.contains(":15432") {
            found.push(format!("{position}: targets the dead port 15432: {}", line.trim()));
        }
        for application_database in [":5432/synapse\"", ":5432/synapse'"] {
            if line.contains(application_database) {
                found.push(format!("{position}: falls back to the application database: {}", line.trim()));
            }
        }
    }
    found
}

#[test]
fn rust_resolvers_share_one_fallback_chain() {
    for file in RUST_RESOLVERS {
        let chain = fallback_chain(&read(file));
        assert_eq!(
            chain,
            CANONICAL_FALLBACKS.to_vec(),
            "{file} does not carry the shared test-DB fallback chain. If the chain really has to \
             change, change every copy in RUST_RESOLVERS together — the point of this test is that \
             the harness cannot disagree with itself again"
        );
    }
}

#[test]
fn every_resolver_is_scanned_by_the_guard() {
    // A file that silently drops its `for … in [ … ]` block would make
    // `rust_resolvers_share_one_fallback_chain` pass with an empty chain, so
    // assert the extraction actually found something in each file.
    for file in RUST_RESOLVERS {
        assert!(!fallback_chain(&read(file)).is_empty(), "{file} produced no fallback chain to check");
    }
}

#[test]
fn no_target_uses_the_dead_port_or_the_application_database() {
    let mut found = Vec::new();
    for file in RUST_RESOLVERS {
        found.extend(violations(file, &read(file)));
    }
    for (file, _) in SCRIPT_PORT_DEFAULTS {
        found.extend(violations(file, &read(file)));
    }
    assert!(found.is_empty(), "test-database targets violate the convention:\n{}", found.join("\n"));
}

#[test]
fn every_script_carries_the_5432_default() {
    for (file, expected) in SCRIPT_PORT_DEFAULTS {
        let source = read(file);
        assert!(
            source.contains(expected),
            "{file} must default to port 5432 (`{expected}`) — 5432 is what CI exports, what the dev \
             compose override publishes (`${{DB_EXPOSE_PORT:-5432}}:5432`) and what the local \
             Homebrew PostgreSQL listens on"
        );
    }
}

#[test]
fn environment_variables_are_consulted_before_any_fallback() {
    for file in RUST_RESOLVERS {
        let source = read(file);
        let test_database_url = source.find("TEST_DATABASE_URL").unwrap_or_else(|| {
            panic!("{file} must consult TEST_DATABASE_URL first — the env var is the only way CI can pin the target")
        });
        let first_fallback = source
            .find("postgresql://synapse:synapse@localhost:5432/synapse_test")
            .unwrap_or_else(|| panic!("{file} no longer contains the canonical fallback"));
        assert!(
            test_database_url < first_fallback,
            "{file} probes a fallback before consulting TEST_DATABASE_URL; an explicitly configured \
             target must always win over a guess"
        );
    }
}

#[test]
fn the_checker_rejects_the_old_chain() {
    // The exact shape that was in the tree before H-12: the dead port first,
    // then the application database.
    let old = concat!(
        "fn candidate_database_urls() -> Vec<String> {\n",
        "    for fallback in [\n",
        "        \"postgresql://synapse:synapse@localhost:15432/synapse_test\",\n",
        "        \"postgresql://synapse:synapse@localhost:5432/synapse\",\n",
        "    ] {\n",
    );
    assert_ne!(
        fallback_chain(old),
        CANONICAL_FALLBACKS.to_vec(),
        "the predicate must not accept the first array entry alone"
    );
    let found = violations("old", old);
    assert_eq!(found.len(), 2, "both the dead port and the application database must be flagged, got {found:?}");
    assert!(found[0].contains("15432"), "the dead port must be reported: {found:?}");
    assert!(found[1].contains("application database"), "the app database must be reported: {found:?}");
}
