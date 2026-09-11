//! Guard tests for `scripts/ci/compute_perf_gate.sh` and for the honesty of the
//! performance-threshold claims in `TESTING.md`.
//!
//! ## Why this file exists
//!
//! `TESTING.md` used to declare per-endpoint P95 targets (search ≤500 ms,
//! sync ≤1000 ms, DB ≤100 ms) plus per-benchmark targets (`whoami ≤20 ms`).
//! Audit found none of them had an executor, while measured reality was 15–30×
//! better than the numbers — so even running they could not have caught a
//! realistic regression. The only threshold in code was
//! `sliding_sync_perf_gate.sh`'s 5000 ms, and that script was not wired to CI.
//!
//! `tests/performance/query_performance_tests.rs` was worse: its
//! `assert!(duration.as_millis() < 100)` timed a `tokio::task::yield_now()`,
//! not a query — a green check that could never fail for the reason its name
//! implied, and no database was involved at all.
//!
//! The replacement is `scripts/ci/compute_perf_gate.sh`: real Criterion
//! benchmarks that need no server or database, compared against calibrated
//! ceilings. These tests keep it wired and keep the fake assertions from
//! creeping back.

use std::fs;
use std::path::PathBuf;

fn repo_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
}

fn read(rel: &str) -> String {
    let p = repo_root().join(rel);
    fs::read_to_string(&p).unwrap_or_else(|e| panic!("expected {p:?} to be readable: {e}"))
}

// =============================================================================
// The gate exists, is executable, and is wired into CI
// =============================================================================

#[test]
fn compute_perf_gate_exists_and_is_executable() {
    use std::os::unix::fs::PermissionsExt;
    let p = repo_root().join("scripts/ci/compute_perf_gate.sh");
    assert!(p.is_file(), "应存在 compute_perf_gate.sh: {p:?}");
    let mode = fs::metadata(&p).expect("metadata").permissions().mode();
    assert!(mode & 0o100 != 0, "compute_perf_gate.sh 应可执行 (mode={mode:o})");
}

/// A performance gate that CI never runs is documentation, not a gate.
#[test]
fn compute_perf_gate_is_wired_into_a_workflow() {
    let workflows = repo_root().join(".github/workflows");
    let mut found = Vec::new();
    for entry in fs::read_dir(&workflows).expect("workflows dir").flatten() {
        let path = entry.path();
        let Some(name) = path.file_name().and_then(|n| n.to_str()) else { continue };
        if !(name.ends_with(".yml") || name.ends_with(".yaml")) {
            continue;
        }
        if let Ok(text) = fs::read_to_string(&path) {
            if text.contains("compute_perf_gate") {
                found.push(name.to_string());
            }
        }
    }
    assert!(
        !found.is_empty(),
        "compute_perf_gate.sh 必须接入某个 workflow，\
         否则与它要取代的那些「无人执行的 P95 数字」没有区别"
    );
}

/// The gate must fail loudly when a benchmark produced no measurement.
#[test]
fn compute_perf_gate_fails_on_missing_measurements_by_default() {
    let script = read("scripts/ci/compute_perf_gate.sh");
    assert!(script.contains("COMPUTE_PERF_GATE_STRICT"), "应支持严格模式开关（默认应让「没测到」变成失败）");
    assert!(
        script.contains("MISSING=$((MISSING + 1))") || script.contains("missing"),
        "应统计缺失的基准，并让其影响退出码"
    );
    assert!(script.contains("EXPECTED="), "应对「至少测到几个基准」设下限，否则基准被静默跳过时门禁仍会绿");
}

// =============================================================================
// No unexecuted latency numbers may return to TESTING.md
// =============================================================================

/// `TESTING.md` must not re-declare per-endpoint P95 targets as if they were gates.
///
/// The honest current position is: pure-compute and sliding-sync gates exist;
/// P95/P99 for the API endpoints have *baselines* but **no gate**. That
/// distinction must stay explicit.
#[test]
fn testing_md_does_not_claim_unenforced_p95_gates() {
    let doc = read("TESTING.md");
    for claim in ["搜索API P95延迟：≤500ms", "同步请求 P95延迟：≤1000ms", "数据库查询 P95延迟：≤100ms"]
    {
        assert!(
            !doc.contains(claim),
            "TESTING.md 不得再声明无人执行的阈值 `{claim}`。\
             要么接一个真门禁，要么明确标为「仅有基线、无门禁」。"
        );
    }
}

/// `TESTING.md` must name the gates that actually exist.
#[test]
fn testing_md_points_at_the_real_gates() {
    let doc = read("TESTING.md");
    for gate in ["compute_perf_gate.sh", "sliding_sync_perf_gate.sh"] {
        assert!(doc.contains(gate), "TESTING.md 应指向真实门禁 `{gate}`");
    }
}

/// The doc must admit that `tests/performance/` is largely simulated.
#[test]
fn testing_md_flags_the_performance_directory_as_simulated() {
    let doc = read("TESTING.md");
    assert!(
        doc.contains("模拟") || doc.contains("simulated"),
        "TESTING.md 必须说明 tests/performance/ 多为模拟，\
         否则读者会以为它验证了性能"
    );
}

// =============================================================================
// The fake latency assertion must not come back
// =============================================================================

/// `query_performance_tests.rs` must not assert a wall-clock bound on a
/// simulated (non-DB) operation again.
///
/// The file performs no database access: every "query" is
/// `tokio::task::yield_now()`. A latency assertion there is unfalsifiable in
/// practice and creates false confidence.
///
/// Comments are stripped before checking: the module docs deliberately *mention*
/// the removed `duration.as_millis() < 100` assertion to explain why it is gone.
#[test]
fn simulated_query_perf_tests_do_not_assert_latency() {
    let src = read("tests/performance/query_performance_tests.rs");
    let code: String = src
        .lines()
        .filter(|l| {
            let t = l.trim_start();
            !t.starts_with("//") && !t.starts_with("///") && !t.starts_with("//!")
        })
        .collect::<Vec<_>>()
        .join("\n");

    assert!(
        !code.contains("as_millis() <") && !code.contains("elapsed() <"),
        "query_performance_tests.rs 不得对模拟操作断言墙钟上限：\
         它测的是 tokio::task::yield_now()，任何阈值都会通过，\
         绿勾表达不了它名字所暗示的保护。真实门禁见 compute_perf_gate.sh。\n\
         非注释代码:\n{code}"
    );
}

/// …and the file must keep saying so, so the constraint is discoverable.
#[test]
fn simulated_query_perf_tests_document_their_limits() {
    let src = read("tests/performance/query_performance_tests.rs");
    assert!(
        src.contains("SIMULATED") || src.contains("do not touch a database") || src.contains("模拟"),
        "该文件必须就地说明它是模拟的、不连库，避免被误当成性能门禁"
    );
}
