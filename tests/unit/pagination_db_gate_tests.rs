//! Guards for the DB-backed pagination gate (`benchmark.yml::pagination-perf-gate`).
//!
//! ## Why this file exists
//!
//! The original "pagination gate" compared two **in-memory simulation** functions
//! (`benches/performance_api_benchmarks.rs`) whose margin is ~1500x, so no real
//! `synapse-storage` SQL regression could ever trip it — a long-green gate that
//! was measuring the wrong thing (E4, `docs/audit/GATE_INTEGRITY_FOLLOWUP_2026-09-19.md`
//! §9/§11). The replacement is a real DB-backed gate: it calls the **production**
//! keyset query against a migrated events table, checks the page is the *same*
//! page, checks the plan still uses an index, and only then compares it with real
//! `LIMIT/OFFSET`.
//!
//! Those properties are what makes the gate meaningful, and every one of them is
//! a single edit away from silently disappearing (repoint the workflow, drop the
//! index check, swap the bench back to a private query, re-word the smoke check
//! into looking like the gate). This test pins each of them, so a regression has
//! to be a deliberate edit of this file too.
//!
//! Red proof: rename the workflow job, drop `BENCHMARK_DATABASE_URL` from it,
//! change the bench back to the OR-form predicate, revert the qualified
//! `ORDER BY` in `synapse-storage/src/event/pagination.rs`, or drop the
//! shallow/deep ratio check — each makes this test fail.

use std::fs;
use std::path::PathBuf;

fn repo_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
}

fn read(rel: &str) -> String {
    let path = repo_root().join(rel);
    fs::read_to_string(&path).unwrap_or_else(|error| panic!("expected {path:?} to be readable: {error}"))
}

#[test]
fn db_pagination_gate_is_wired_to_a_migrated_database() {
    let workflow = read(".github/workflows/benchmark.yml");
    assert!(
        workflow.contains("pagination-perf-gate"),
        "the DB-backed pagination gate job must exist in benchmark.yml; the in-memory check is not a gate"
    );
    assert!(
        workflow.contains("pagination_perf_gate.sh"),
        "the `pagination-perf-gate` job must actually invoke scripts/ci/pagination_perf_gate.sh"
    );
    assert!(
        workflow.contains("BENCHMARK_DATABASE_URL"),
        "the gate needs BENCHMARK_DATABASE_URL pointing at its own migrated database"
    );
    assert!(
        workflow.contains("POSTGRES_DB: synapse_bench"),
        "the gate job must provision the `synapse_bench` service; without it the benchmark silently skips"
    );
}

#[test]
fn db_pagination_gate_can_actually_fail() {
    let gate = read("scripts/ci/pagination_perf_gate.sh");
    for needle in [
        "PAGINATION_MIN_GAIN",
        "PAGINATION_MAX_SHALLOW_RATIO",
        "PAGINATION_SHALLOW_BREACH_FLOOR_US",
        "index_scan",
        "keyset_shallow_us",
        "correct",
        "BENCH_REQUIRE",
    ] {
        assert!(
            gate.contains(needle),
            "the gate must keep checking `{needle}`; dropping it is how a gate stops being one"
        );
    }
    assert!(
        gate.contains("BREACH") && gate.contains("FAILED"),
        "the gate must report and fail on a breach instead of only printing numbers"
    );
}

#[test]
fn gate_measures_the_production_keyset_query_and_its_index_plan() {
    let bench = read("benches/performance_pagination_benchmarks.rs");
    assert!(
        bench.contains("get_room_events_paginated_cursor"),
        "the bench must measure the production keyset query (`EventStorage::get_room_events_paginated_cursor`), \
         not a private reimplementation that can drift from it"
    );
    assert!(
        bench.contains("(origin_server_ts, stream_ordering) <"),
        "the index-plan probe must EXPLAIN the row-value predicate production uses; the old OR form planned as \
         Bitmap+Sort, so an OR-form probe reports `index_scan=0` for a shape nobody runs"
    );
    assert!(
        bench.contains("EXPLAIN (FORMAT TEXT) SELECT {ROOM_EVENT_COLS}"),
        "the index-plan probe must EXPLAIN the wide production select list (`ROOM_EVENT_COLS`), not a narrow \
         `SELECT event_id` proxy: with no COALESCE output column the bare ORDER BY binds to the input column and \
         the proxy stays green while the real query sorts every row above the cursor"
    );
    assert!(
        bench.contains("ORDER BY events.origin_server_ts DESC, events.stream_ordering DESC"),
        "the index-plan probe must EXPLAIN the same qualified ORDER BY production uses"
    );

    let production = read("synapse-storage/src/event/pagination.rs");
    assert!(
        production.contains("(origin_server_ts, stream_ordering) > ($2, $3)")
            && production.contains("(origin_server_ts, stream_ordering) < ($2, $3)"),
        "both keyset directions must keep the row-value predicate; reverting either to the OR form restores the \
         Bitmap+Sort plan this change removed"
    );
    assert!(
        production.contains("ORDER BY events.origin_server_ts ASC, events.stream_ordering ASC")
            && production.contains("ORDER BY events.origin_server_ts DESC, events.stream_ordering DESC"),
        "both keyset directions must qualify their sort keys with the table name; a bare `ORDER BY \
         origin_server_ts` binds to the `COALESCE(origin_server_ts, 0) AS origin_server_ts` output column and \
         loses `idx_events_room_ts_stream`"
    );
    assert!(
        !production.contains("ORDER BY origin_server_ts"),
        "no ORDER BY may resolve `origin_server_ts` against the ROOM_EVENT_COLS output alias; every keyset sort \
         key must be `events.origin_server_ts`"
    );
}

#[test]
fn in_memory_check_still_says_it_is_not_the_pagination_gate() {
    let smoke = read("scripts/check_pagination_benchmark.py");
    let lowered = smoke.to_lowercase();
    assert!(
        lowered.contains("not") && lowered.contains("pagination gate"),
        "scripts/check_pagination_benchmark.py must keep declaring that it is NOT the pagination gate \
         (~1500x simulated margin, blind to SQL regressions); rewording it into looking like the real gate \
         would recreate the E4 false-confidence problem"
    );
    assert!(smoke.contains("pagination_perf_gate.sh"), "the smoke check must point readers at the real DB-backed gate");
}
