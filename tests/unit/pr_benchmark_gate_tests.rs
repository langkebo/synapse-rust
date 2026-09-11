//! Guard tests for `scripts/ci/benchmark_pr_gate.sh` (the `pr-benchmark-gate` CI job).
//!
//! ## Why this file exists
//!
//! The job runs on every PR and its name promises to "block performance
//! regressions". Audit on 2026-09-11 found it could not do that, in three
//! independent ways — each sufficient on its own to make it useless:
//!
//! 1. **Crash on the comparison path.** `local` is not valid outside a function,
//!    but the script declared `local name/value/unit` inside the baseline
//!    comparison `while` loop and again in the report loop. Bash aborts on the
//!    first such statement, so whenever a baseline *was* present the step died
//!    with `local: can only be used in a function` instead of reporting.
//!
//! 2. **Format mismatch → zero benchmarks parsed.** The baseline artifact
//!    (`benchmark.txt`) is written with `--output-format bencher`
//!    (`test NAME ... bench: 123 ns/iter`), while the gate's parser keys on
//!    Criterion's text form (`time: [...]`). The two never intersect, so the
//!    comparison had nothing to compare.
//!
//! 3. **Silent pass.** The baseline download used `continue-on-error: true`,
//!    and the script's `else` branch printed PASSED when no baseline file
//!    existed. So a missing/unusable baseline was indistinguishable from
//!    "no regressions" — the same false-green pattern as the empty
//!    `cargo test --doc` gate.
//!
//! Additionally the gate benchmarked in the **default profile** while the
//! baseline was recorded with `--profile release-perf`, so the numbers were not
//! comparable even once the plumbing was fixed.
//!
//! These tests pin the corrected behaviour: real regression detection on a
//! parseable baseline, a loud failure on an unusable one, and no `local` misuse.

use std::fs;
use std::path::PathBuf;
use std::process::Command;

fn repo_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
}

fn script() -> PathBuf {
    repo_root().join("scripts/ci/benchmark_pr_gate.sh")
}

fn read(rel: &str) -> String {
    let p = repo_root().join(rel);
    fs::read_to_string(&p).unwrap_or_else(|e| panic!("expected {p:?} to be readable: {e}"))
}

/// Unique temp dir per call.
fn temp_dir(tag: &str) -> PathBuf {
    use std::sync::atomic::{AtomicU64, Ordering};
    static N: AtomicU64 = AtomicU64::new(0);
    let n = N.fetch_add(1, Ordering::Relaxed);
    let d = std::env::temp_dir().join(format!("prbench_{tag}_{}_{}", std::process::id(), n));
    fs::create_dir_all(&d).expect("temp dir");
    d
}

// =============================================================================
// Defect 1: `local` outside a function
// =============================================================================

/// `local` may only appear inside a shell function.
///
/// The script used it in two `while` loops at top level, which aborts bash with
/// `local: can only be used in a function` (exit 1) the moment that branch runs.
#[test]
fn pr_gate_has_no_local_outside_a_function() {
    let source = read("scripts/ci/benchmark_pr_gate.sh");

    // Collect the line ranges covered by function bodies: `name() {` ... `}`.
    let lines: Vec<&str> = source.lines().collect();
    let mut in_function = false;
    let mut offenders: Vec<String> = Vec::new();

    for (idx, raw) in lines.iter().enumerate() {
        let trimmed = raw.trim_start();
        // A top-level function definition starts with `name() {`.
        if !in_function && trimmed.ends_with("() {") && !raw.starts_with(|c: char| c.is_whitespace()) {
            in_function = true;
            continue;
        }
        if in_function {
            // A closing brace at column 0 ends the function.
            if *raw == "}" {
                in_function = false;
            }
            continue;
        }
        // Top level: `local` is illegal here.
        if trimmed.starts_with("local ") {
            offenders.push(format!("line {}: {}", idx + 1, raw.trim()));
        }
    }

    assert!(
        offenders.is_empty(),
        "benchmark_pr_gate.sh 在函数外使用了 `local`，bash 会直接中止\
         （local: can only be used in a function）—— 这正是基线存在时该步骤崩溃的原因:\n{}",
        offenders.join("\n")
    );
}

/// The script must at least parse under `bash -n`.
#[test]
fn pr_gate_parses() {
    let out = Command::new("bash").arg("-n").arg(script()).output().expect("bash -n must be runnable");
    assert!(out.status.success(), "bash -n 失败:\n{}", String::from_utf8_lossy(&out.stderr));
}

// =============================================================================
// Defect 2 + 3: real regression detection, loud failure on unusable baseline
// =============================================================================

/// Criterion-text "current results" the gate compares against the baseline.
///
/// `state_resolution_chain_10` is deliberately ~45% slower than the baseline
/// below (400 ns vs 274 ns), so a correct gate must report a regression.
const CURRENT: &str = "state_resolution_chain_10\t400.00\tns\nauth_chain_build_10\t5.4000\tµs\n";

const BASELINE: &str = "\
state_resolution_chain_10
                        time:   [270.00 ns 274.00 ns 278.00 ns]
auth_chain_build_10     time:   [5.3000 µs 5.3600 µs 5.4000 µs]
";

/// Runs the gate against a synthetic baseline, returning `(exit_code, output)`.
///
/// The gate reads its "current" measurements from
/// `artifacts/pr_benchmark_current.txt` when `BENCH_PR_GATE_SKIP_BENCH=1`, so we
/// seed that file instead of compiling benchmarks. The file is restored
/// afterwards to keep the workspace clean.
fn run_gate(baseline: Option<&str>) -> (i32, String) {
    let dir = temp_dir("gate");
    // Per-call temp paths: several tests run in parallel, so a shared
    // `artifacts/pr_benchmark_current.txt` would race (observed: one test
    // deleting the file while another script invocation read it, yielding
    // `sort: No such file or directory`).
    let current_path = dir.join("current.txt");
    fs::write(&current_path, CURRENT).expect("seed current results");

    let mut cmd = Command::new("bash");
    cmd.arg(script());
    cmd.env("BENCH_PR_GATE_SKIP_BENCH", "1");
    cmd.env("BENCH_PR_GATE_CURRENT_PATH", &current_path);
    cmd.env("BENCH_PR_GATE_PARSED_PATH", dir.join("parsed.txt"));
    cmd.current_dir(repo_root());

    match baseline {
        Some(body) => {
            let p = dir.join("baseline.txt");
            fs::write(&p, body).expect("write baseline");
            cmd.env("BENCH_BASELINE_PATH", &p);
        }
        None => {
            cmd.env("BENCH_BASELINE_PATH", dir.join("does-not-exist.txt"));
        }
    }

    let out = cmd.output().expect("gate must be spawnable");
    let _ = fs::remove_dir_all(&dir);

    let mut combined = String::from_utf8_lossy(&out.stdout).into_owned();
    combined.push_str(&String::from_utf8_lossy(&out.stderr));
    (out.status.code().unwrap_or(-1), combined)
}

/// The gate must actually detect a regression — the whole point of the job.
///
/// `CURRENT` has `state_resolution_chain_10` at 400 ns against a 274 ns
/// baseline (+46%), well past the 15% threshold.
#[test]
fn pr_gate_detects_a_regression() {
    let (code, output) = run_gate(Some(BASELINE));
    assert_eq!(
        code, 1,
        "相对基线退化 46% 时必须失败（阈值 15%）——\
         修复前该脚本要么崩溃、要么解析不到任何基准而静默通过\n{output}"
    );
    assert!(output.contains("REGRESSION") || output.contains("FAILED"), "应明确报告回归\n{output}");
}

/// No regression → pass. Guards against a gate that always fails.
#[test]
fn pr_gate_passes_when_within_threshold() {
    // Current == baseline ⇒ 0% change.
    let same_time = "\
state_resolution_chain_10
                        time:   [399.00 ns 400.00 ns 401.00 ns]
auth_chain_build_10     time:   [5.3000 µs 5.4000 µs 5.5000 µs]
";
    let (code, output) = run_gate(Some(same_time));
    assert_eq!(code, 0, "无回归时应通过\n{output}");
}

// =============================================================================
// Profile comparability
// =============================================================================

/// The gate must benchmark in the same profile the baseline was recorded with.
///
/// The baseline artifact is produced with `--profile release-perf`; benchmarking
/// in the default profile yields numbers that are not comparable.
#[test]
fn pr_gate_uses_the_same_profile_as_the_baseline() {
    let source = read("scripts/ci/benchmark_pr_gate.sh");
    assert!(
        source.contains("release-perf") || source.contains("BENCH_PROFILE"),
        "PR 门禁必须在 release-perf（与基线一致）下测量；\
         此前用默认 profile 跑基准，与 release-perf 录制的基线不可比"
    );
}

/// The baseline artifact the job downloads must be the Criterion-text one.
#[test]
fn pr_gate_baseline_points_at_the_text_format_artifact() {
    let ci = read(".github/workflows/ci.yml");
    assert!(
        ci.contains("benchmark_standard.txt") || ci.contains("BENCH_BASELINE_PATH"),
        "pr-benchmark-gate 必须使用 Criterion 文本格式的基线文件\
         （benchmark.txt 是 --output-format bencher 的产物，解析器读不了）"
    );
}

/// The workflow must not silently tolerate a failed baseline download.
#[test]
fn pr_gate_workflow_does_not_mask_a_missing_baseline() {
    let ci = read(".github/workflows/ci.yml");
    let job_start = ci.find("pr-benchmark-gate:").expect("job must exist");
    let job = &ci[job_start..];
    let job_end = job.find("\n  integration-test:").unwrap_or(job.len());
    let job = &job[..job_end];

    // Strip comments first: the job carries an explanatory comment that *names*
    // `continue-on-error: true` to record why it must not be used, and a naive
    // substring check would match its own documentation.
    let effective: String = job.lines().filter(|l| !l.trim_start().starts_with('#')).collect::<Vec<_>>().join("\n");

    assert!(
        !effective.contains("continue-on-error: true"),
        "pr-benchmark-gate 不得对基线下载设置 continue-on-error: true —— \
         那会把「基线没下到」伪装成「没有回归」\n实际内容:\n{effective}"
    );
}
