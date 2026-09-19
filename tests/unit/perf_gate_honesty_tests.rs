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
use std::process::{Command, Output};

fn repo_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
}

fn read(rel: &str) -> String {
    let p = repo_root().join(rel);
    fs::read_to_string(&p).unwrap_or_else(|e| panic!("expected {p:?} to be readable: {e}"))
}

// =============================================================================
// Executing the gate instead of grepping it
// =============================================================================

/// A hermetic sandbox in which `scripts/ci/compute_perf_gate.sh` really runs.
///
/// The script is copied byte-for-byte into a temp tree, so its `ROOT_DIR` and
/// its `artifacts/` writes stay there, and a stub `cargo` on `PATH` plays the
/// benchmark runner. That stub is the *only* fake: bash, the Criterion log
/// parsing, the ceiling comparison, the strictness decision and the exit code
/// are all the production script. The assertions this replaces only read the
/// script's source text, which is why they stayed green when the strict default
/// was flipped to `0`.
struct GateSandbox {
    root: PathBuf,
}

impl GateSandbox {
    fn new(tag: &str) -> Self {
        let unique = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .expect("system clock after the epoch")
            .as_nanos();
        let root = std::env::temp_dir().join(format!("perf_gate_{tag}_{}_{unique}", std::process::id()));
        let _ = fs::remove_dir_all(&root);
        fs::create_dir_all(root.join("scripts/ci")).expect("create sandbox scripts dir");
        fs::create_dir_all(root.join("bin")).expect("create sandbox bin dir");
        let script = repo_root().join("scripts/ci/compute_perf_gate.sh");
        fs::copy(&script, root.join("scripts/ci/compute_perf_gate.sh"))
            .unwrap_or_else(|e| panic!("copy {script:?} into the sandbox: {e}"));
        Self { root }
    }

    /// Install the stub benchmark runner. `body` is the shell body of `cargo`;
    /// its stdout is what the script captures as the Criterion log.
    fn stub_cargo(&self, body: &str) {
        use std::os::unix::fs::PermissionsExt;
        let path = self.root.join("bin/cargo");
        fs::write(&path, format!("#!/usr/bin/env bash\n{body}\n")).expect("write stub cargo");
        fs::set_permissions(&path, fs::Permissions::from_mode(0o755)).expect("chmod stub cargo");
    }

    /// Run the gate inside the sandbox. `strict = None` removes the variable
    /// entirely, which is the only way to prove what the *default* is.
    fn run(&self, strict: Option<&str>) -> Output {
        let mut command = Command::new("bash");
        command
            .arg(self.root.join("scripts/ci/compute_perf_gate.sh"))
            .current_dir(&self.root)
            .env_remove("COMPUTE_PERF_GATE_STRICT")
            .env("PATH", format!("{}:{}", self.root.join("bin").display(), std::env::var("PATH").unwrap_or_default()));
        if let Some(value) = strict {
            command.env("COMPUTE_PERF_GATE_STRICT", value);
        }
        command.output().expect("bash must be runnable")
    }
}

impl Drop for GateSandbox {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.root);
    }
}

/// Exit code plus both streams, for assertion messages.
fn render(output: &Output) -> String {
    format!(
        "exit={:?}\n--- stdout ---\n{}--- stderr ---\n{}",
        output.status.code(),
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    )
}

/// Read `<key><digits>` out of the gate's `==> Summary:` line.
fn summary_metric(text: &str, key: &str) -> Option<usize> {
    let index = text.find(key)?;
    text[index + key.len()..].chars().take_while(char::is_ascii_digit).collect::<String>().parse().ok()
}

/// Criterion-shaped output for benchmarks that satisfy the gate's ceilings.
const HEALTHY_MEASUREMENTS: &str = r#"cat <<'CRITERION'
state_resolution_chain_10
                        time:   [270.00 ns 274.52 ns 275.91 ns]
state_resolution_chain_100
                        time:   [290.00 ns 295.60 ns 296.00 ns]
auth_chain_build_10
                        time:   [5.30 µs 5.36 µs 5.40 µs]
membership_transitions/join
                        time:   [1.00 ns 1.07 ns 1.10 ns]
CRITERION
"#;

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
///
/// Proven by *executing* `compute_perf_gate.sh` against a benchmark runner that
/// exits 0 while measuring nothing — not by grepping its source. The text
/// assertions this replaces stayed green when `COMPUTE_PERF_GATE_STRICT` was
/// defaulted to `0`, because the script's prose contains the same identifiers.
#[test]
fn compute_perf_gate_fails_on_missing_measurements_by_default() {
    let sandbox = GateSandbox::new("missing_default");
    // A runner that "succeeds" but emits no Criterion measurement at all.
    sandbox.stub_cargo("exit 0");

    // No COMPUTE_PERF_GATE_STRICT in the environment: the default decides.
    let output = sandbox.run(None);
    let text = render(&output);
    assert!(!output.status.success(), "默认必须严格：一个基准都没测到却退出 0，等于「静默跳过」也能绿。\n{text}");
    assert_eq!(summary_metric(&text, "missing="), Some(1), "缺失的基准必须计入 missing:\n{text}");
    assert!(text.contains("Compute Performance Gate: FAILED"), "应打印失败摘要:\n{text}");
}

/// The explicit strict value is not the only path to strictness — but it must
/// work too, so pinning it in CI cannot silently become lenient.
#[test]
fn compute_perf_gate_strict_env_var_fails_on_missing_measurements() {
    let sandbox = GateSandbox::new("missing_strict");
    sandbox.stub_cargo("exit 0");

    let output = sandbox.run(Some("1"));
    let text = render(&output);
    assert!(!output.status.success(), "COMPUTE_PERF_GATE_STRICT=1 必须在没测到基准时失败:\n{text}");
    assert_eq!(summary_metric(&text, "missing="), Some(1), "缺失的基准必须计入 missing:\n{text}");
}

/// Lenient mode is an explicit opt-out: it tolerates missing measurements, says
/// so, and still fails a real ceiling breach.
#[test]
fn compute_perf_gate_lenient_mode_is_explicit_and_still_fails_breaches() {
    let sandbox = GateSandbox::new("lenient");
    sandbox.stub_cargo("exit 0");

    let lenient = sandbox.run(Some("0"));
    let text = render(&lenient);
    assert!(lenient.status.success(), "COMPUTE_PERF_GATE_STRICT=0 是显式的宽松模式，缺测量不应失败:\n{text}");
    assert!(text.contains("non-strict"), "宽松模式必须自报家门，否则与严格模式无法区分:\n{text}");
    assert_eq!(summary_metric(&text, "missing="), Some(0), "宽松模式下 missing 不参与失败判定:\n{text}");

    // Lenient relaxes "measured nothing", not "measured too slow".
    let breaching = HEALTHY_MEASUREMENTS.replace("274.52", "3000000.00");
    sandbox.stub_cargo(&breaching);
    let breach = sandbox.run(Some("0"));
    assert!(!breach.status.success(), "宽松模式仍必须对超阈值失败:\n{}", render(&breach));
}

/// Positive control: when the expected measurements are present and inside the
/// ceilings the gate passes — otherwise "exits non-zero" alone would satisfy
/// every assertion above.
#[test]
fn compute_perf_gate_passes_when_the_expected_measurements_are_present() {
    let sandbox = GateSandbox::new("healthy");
    sandbox.stub_cargo(HEALTHY_MEASUREMENTS);

    let output = sandbox.run(Some("1"));
    let text = render(&output);
    assert!(output.status.success(), "测到全部基准且都在阈值内时必须通过:\n{text}");
    assert!(text.contains("Compute Performance Gate: PASSED"), "应打印通过摘要:\n{text}");
    assert_eq!(summary_metric(&text, "breaches="), Some(0), "不得有超阈值:\n{text}");
    assert_eq!(summary_metric(&text, "missing="), Some(0), "不得有缺失:\n{text}");
    assert!(
        summary_metric(&text, "measured=").is_some_and(|measured| measured >= 4),
        "EXPECTED 下限（3 个 federation + 至少 1 个 membership）必须真的被测到:\n{text}"
    );
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
