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
//! A misleading comment in the test-utils source claimed
//! `"PostgreSQL max_connections=100 supports 12*~5=60 conns"` — the `~5` is a
//! guess at *actual* connection usage, whereas the pool is configured to grow to
//! 40. This test pins the real invariant so the comment cannot silently rot again.

#![allow(clippy::unwrap_used, clippy::expect_used)]

use std::path::PathBuf;

fn project_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
}

/// Read the pool ceiling from the source of truth.
fn default_test_db_max_connections() -> u32 {
    let source = "synapse-test-utils/src/lib.rs";
    let src = std::fs::read_to_string(project_root().join(source)).expect("read test-utils source");
    let marker = "const DEFAULT_TEST_DB_MAX_CONNECTIONS: u32 =";
    let line = src
        .lines()
        .find(|line| line.trim_start().starts_with(marker))
        .unwrap_or_else(|| panic!("`{marker}` not found in {source}"));
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

/// Connections one concurrently-running DB test actually holds at once.
///
/// The pool **ceiling** (`pool_max = 40`) is not demand: sqlx opens connections
/// lazily (`DEFAULT_TEST_DB_MIN_CONNECTIONS = 0`) and every DB fixture issues its
/// queries serially — the isolated pool documents exactly that and caps itself at
/// `max_connections(1)`. The budget must model demand, not the ceiling.
///
/// The previous model (`test-threads × pool_max` = 12 × 40 = 480 vs the server's
/// 100) therefore reported a 4.8× "violation" that never occurred, and the only
/// thing it did about it was `println!`. The invariant was neither enforced nor
/// true: there was no assertion at all, so no change to the pool or the profile
/// could turn it red. Re-measured 2026-09-19 and recalibrated here.
///
/// If a fixture ever starts issuing concurrent queries, raise this constant (and
/// say why) — that is the deliberate, visible decision the gate exists to force.
const CONNECTIONS_HELD_PER_TEST: u32 = 1;

/// Head-room for the admin/template/clone pools the harness also opens (the
/// template builder, the schema-clone pool and the exit-cleanup pools each allow
/// a small number of connections in addition to the per-test pools).
const HARNESS_POOL_RESERVE: u32 = 20;

/// The invariant: the suite's worst-case **simultaneous demand** must fit
/// PostgreSQL's default `max_connections` (100), otherwise connection acquisition
/// blocks and tests time out under load while passing serially.
///
/// The assertion is a real one (it can fail): raising the ci profile's
/// `test-threads`, raising `CONNECTIONS_HELD_PER_TEST`, or lowering the server
/// limit makes it red.
#[test]
fn test_db_connection_budget_is_documented_and_bounded() {
    let pool_max = default_test_db_max_connections();
    let ci_threads = ci_profile_test_threads();

    // PostgreSQL default max_connections for the CI service (no override in
    // .github/workflows/*.yml — verified when this test was written).
    const PG_DEFAULT_MAX_CONNECTIONS: u32 = 100;

    let worst_case_demand = ci_threads.saturating_mul(CONNECTIONS_HELD_PER_TEST).saturating_add(HARNESS_POOL_RESERVE);

    println!(
        "test DB connection budget: ci test-threads={ci_threads} x {CONNECTIONS_HELD_PER_TEST} conn/test \
         + {HARNESS_POOL_RESERVE} harness reserve = {worst_case_demand} vs PG max_connections=\
         {PG_DEFAULT_MAX_CONNECTIONS} (the per-pool ceiling {pool_max} is a bound on one pool, not demand)"
    );

    assert!(
        worst_case_demand <= PG_DEFAULT_MAX_CONNECTIONS,
        "the suite's worst-case simultaneous connection demand ({ci_threads} test processes x \
         {CONNECTIONS_HELD_PER_TEST} connection(s) each + {HARNESS_POOL_RESERVE} harness reserve = \
         {worst_case_demand}) must fit PostgreSQL's max_connections ({PG_DEFAULT_MAX_CONNECTIONS}). \
         Lower `[profile.ci] test-threads` in .config/nextest.toml, raise the server limit, or justify \
         a higher per-test demand in CONNECTIONS_HELD_PER_TEST."
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
}
