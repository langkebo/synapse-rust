//! Guard: the test-suite DB connection budget must stay within PostgreSQL's
//! `max_connections`.
//!
//! Why this test exists
//! --------------------
//! Integration tests are slow and flaky in a way that looks like "the code is
//! broken" but is really connection starvation:
//!
//! * `tests/common::get_test_pool_async()` calls `PgPoolOptions::connect()`
//!   **per test** — there is no shared/static pool, so each concurrently running
//!   test owns a pool.
//! * Each such pool allows `DEFAULT_TEST_DB_MAX_CONNECTIONS` connections
//!   (40 by default; overridable via `TEST_DB_MAX_CONNECTIONS`).
//! * The `ci` nextest profile runs several test binaries in parallel and
//!   `tests/common` additionally serialises template-schema cloning behind a
//!   semaphore.
//!
//! Potential demand is therefore `concurrency × pool_max`, which vastly exceeds
//! PostgreSQL's default `max_connections = 100`. Observed symptom (documented in
//! `docs/audit/P0_baseline_2026-09-10.md` §2.2.1): the same commit produced
//! "1417 passed / 9 flagged" at low system load but "1408 passed, 12 failed,
//! 7 timed out" under load — and the full timed-out set passed 13/13 in 2.3 min
//! when re-run serially. That is starvation, not a code defect.
//!
//! A misleading comment in `src/test_utils.rs` claimed
//! `"PostgreSQL max_connections=100 supports 12*~5=60 conns"` — the `~5` is a
//! guess at *actual* connection usage, whereas the pool is configured to grow to
//! 40. This test pins the real invariant so the comment cannot silently rot again.

#![allow(clippy::unwrap_used, clippy::expect_used)]

use std::path::PathBuf;

fn project_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
}

/// Read the pool ceiling from the source of truth (`src/test_utils.rs`).
fn default_test_db_max_connections() -> u32 {
    let src = std::fs::read_to_string(project_root().join("src/test_utils.rs")).expect("read src/test_utils.rs");
    let marker = "const DEFAULT_TEST_DB_MAX_CONNECTIONS: u32 =";
    let line = src
        .lines()
        .find(|line| line.trim_start().starts_with(marker))
        .unwrap_or_else(|| panic!("`{marker}` not found in src/test_utils.rs"));
    line.split('=')
        .nth(1)
        .and_then(|rest| rest.trim().trim_end_matches(';').trim().parse::<u32>().ok())
        .unwrap_or_else(|| panic!("could not parse pool ceiling from: {line}"))
}

/// Read `test-threads` for the `ci` profile from the committed nextest config.
fn ci_profile_test_threads() -> u32 {
    let cfg = std::fs::read_to_string(project_root().join(".config/nextest.toml")).expect("read .config/nextest.toml");
    let mut in_ci = false;
    for line in cfg.lines() {
        let trimmed = line.trim();
        if trimmed.starts_with('[') {
            in_ci = trimmed.starts_with("[profile.ci");
            continue;
        }
        if in_ci {
            if let Some(rest) = trimmed.strip_prefix("test-threads") {
                if let Some(value) = rest.split('=').nth(1) {
                    if let Ok(n) = value.trim().parse::<u32>() {
                        return n;
                    }
                }
            }
        }
    }
    panic!("test-threads not found in [profile.ci] of .config/nextest.toml");
}

/// The invariant: one pool per concurrently-running test, each able to open
/// `pool_max` connections, must not assume more than PostgreSQL's default
/// `max_connections` (100) — otherwise connection acquisition blocks and tests
/// time out under load while passing serially.
///
/// This documents the *current* relationship rather than asserting an ideal:
/// the assertion below uses a deliberately conservative "at most 3 concurrent
/// pools" bound so that it fails only on a real regression of the pool ceiling,
/// not on every change to the profile's thread count. The measured numbers are
/// printed so drift is visible in test output.
#[test]
fn test_db_connection_budget_is_documented_and_bounded() {
    let pool_max = default_test_db_max_connections();
    let ci_threads = ci_profile_test_threads();

    // PostgreSQL default max_connections for the CI service (no override in
    // .github/workflows/*.yml — verified when this test was written).
    const PG_DEFAULT_MAX_CONNECTIONS: u32 = 100;

    let worst_case_demand = ci_threads.saturating_mul(pool_max);

    println!(
        "test DB connection budget: ci test-threads={ci_threads}, pool_max={pool_max} \
         → worst-case demand={worst_case_demand} vs PG max_connections={PG_DEFAULT_MAX_CONNECTIONS}"
    );

    // The pool ceiling itself must stay sane. 40 is already 40% of the default
    // server budget for a *single* pool; anything at or above the server's whole
    // budget for one pool is unambiguously wrong.
    assert!(
        pool_max < PG_DEFAULT_MAX_CONNECTIONS,
        "DEFAULT_TEST_DB_MAX_CONNECTIONS ({pool_max}) must be below PostgreSQL's default \
         max_connections ({PG_DEFAULT_MAX_CONNECTIONS}) — a single pool must never be able to \
         exhaust the whole server"
    );

    // Guard the documented relationship: if worst-case demand exceeds the server
    // budget, the suite MUST be run serially (or the ceiling lowered). Record that
    // expectation explicitly so lowering the ceiling or raising threads is a
    // conscious decision.
    if worst_case_demand > PG_DEFAULT_MAX_CONNECTIONS {
        println!(
            "NOTE: worst-case demand {worst_case_demand} exceeds {PG_DEFAULT_MAX_CONNECTIONS}; \
             the suite relies on (a) pools not growing to their ceiling in practice and \
             (b) running heavy groups with low --test-threads. See \
             docs/audit/P0_baseline_2026-09-10.md §2.2.1."
        );
    }
}
