//! 守卫：PR 覆盖率车道（`pr-coverage`）—— 范围算得准、口径说得清、缺数据不静默。
//!
//! ## 这条车道为什么存在
//!
//! `Code Coverage` 是全量口径（storage 单跑 + 其余 workspace，实测 67 分钟、1424 条
//! 集成测试），只能挂在 main push / schedule / 手动 dispatch 上。于是 PR 作者**在合并前
//! 看不到自己这次改动把覆盖率带到了哪里** —— 而要等到 main 上那一轮跑完（并且绿）才知道。
//!
//! PR 车道的做法是"只跑改动落到的那几个 crate"（`cargo llvm-cov -p <crate> --lib`），
//! 代价是读数变成**下界**：实测 `src/bin/healthcheck.rs` 合并基线 10.91%、单腿 **0.00%**；
//! `src/server/mod.rs` 32.30% vs **22.63%**。差异全部来自"这一步没跑那些测试"。
//!
//! ## 因此本文件钉住四件事
//!
//! 1. **路径 → crate 的映射不许猜**：认不出的 `.rs` 必须 fail-closed，否则新开一个顶层
//!    目录就等于"PR 车道没测它，而摘要里一切正常"；
//! 2. **变更行判定只看新增行**：删除与上下文都不该被"有没有被跑到"评判；
//! 3. **缺文件必须红**：被测 crate 没编译好 / 映射漏了一个 crate 时，不能让本步报绿；
//! 4. **PR 车道不许读合并基线**：下界与合并基线不可比，混用就是给每个 PR 造红。

use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::process::Command;

fn repo_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
}

fn script(name: &str) -> PathBuf {
    repo_root().join("scripts/ci").join(name)
}

/// `python3 <script> <args…>` → `(退出码, stdout+stderr)`。
fn run(name: &str, args: &[&str]) -> (i32, String) {
    let output = Command::new("python3")
        .arg(script(name))
        .args(args)
        .current_dir(repo_root())
        .output()
        .expect("python3 must be runnable");
    let mut combined = String::from_utf8_lossy(&output.stdout).into_owned();
    combined.push_str(&String::from_utf8_lossy(&output.stderr));
    (output.status.code().unwrap_or(-1), combined)
}

fn utf8(path: &Path) -> &str {
    path.to_str().expect("temp paths are UTF-8 on this platform")
}

/// 每个跟踪的 `.rs` 都必须有归属 crate，或**明确**排除（vendor/scripts/docker）。
///
/// 这条不测"某一条映射对不对"，测的是"映射的覆盖面"：新加一个顶层目录放源码时，
/// `--self-check` 会在这里红，而不是等某个 PR 的覆盖率数字莫名偏低。
#[test]
fn every_tracked_rust_file_belongs_to_a_crate() {
    let (code, text) = run("affected_crates.py", &["--self-check"]);

    assert_eq!(code, 0, "affected_crates --self-check failed:\n{text}");
    assert!(text.contains("均有归属"), "self-check must report coverage of the tree:\n{text}");
}

/// 映射表：根包拥有整个仓库的路径，所以"最长前缀"不够，必须按规则区分。
#[test]
fn path_to_crate_mapping_is_explicit() {
    let cases = [
        // 根 crate 只认源码目录 + 根 build.rs（否则它会吞掉所有路径）。
        ("src/server/router.rs", "synapse-rust"),
        ("tests/unit/mod.rs", "synapse-rust"),
        ("benches/federation.rs", "synapse-rust"),
        ("build.rs", "synapse-rust"),
        // 成员目录。
        ("synapse-web/src/routes/mod.rs", "synapse-web"),
        ("synapse-common/src/logging.rs", "synapse-common"),
        // 成员自己的清单/测试也算它（改了就要重测它）。
        ("synapse-common/Cargo.toml", "synapse-common"),
        ("synapse-web/tests/route_tests.rs", "synapse-web"),
        // 不影响行覆盖率的路径。
        ("docs/audit/TESTING.md", "-"),
        ("migrations/00000000_unified_schema_v12.sql", "-"),
        ("README.md", "-"),
        // 刻意排除的 `.rs`：不参与任何被测 crate 的编译。
        ("vendor/some_crate/src/lib.rs", "-"),
        ("scripts/ci/tool.rs", "-"),
    ];
    for (path, expected) in cases {
        let (code, text) = run("affected_crates.py", &["--map", path]);
        assert_eq!(code, 0, "`--map {path}` should resolve, got {code}:\n{text}");
        assert_eq!(text.trim(), expected, "`{path}` must map to {expected}, got {text:?}");
    }
}

/// 认不出的 `.rs` 必须 fail-closed（exit 2），不能静默当成"没有 crate 受影响"。
#[test]
fn an_unmapped_rust_path_fails_closed() {
    let (code, text) = run("affected_crates.py", &["--map", "brand-new-module/src/lib.rs"]);

    assert_eq!(code, 2, "an unowned .rs path must fail closed:\n{text}");
    assert!(text.contains("no crate claims"), "the message must say why:\n{text}");
}

/// `--features-flag` 只在**该 crate 定义了 test-utils** 时才给参数。
///
/// 反例是真实存在的：`synapse-cache` / `synapse-test-utils` 没有这个 feature，
/// 硬传 `--features test-utils` 会让 cargo 直接报错、整条腿不产出数据。
#[test]
fn the_test_utils_flag_is_only_passed_when_the_crate_declares_it() {
    let (code, text) = run("affected_crates.py", &["--features-flag", "synapse-web"]);
    assert_eq!(code, 0, "synapse-web should resolve:\n{text}");
    assert_eq!(text.trim(), "--features test-utils");

    let (code, text) = run("affected_crates.py", &["--features-flag", "synapse-cache"]);
    assert_eq!(code, 0, "synapse-cache should resolve:\n{text}");
    assert_eq!(text.trim(), "", "synapse-cache has no test-utils feature — no flag belongs here");

    let (code, _text) = run("affected_crates.py", &["--features-flag", "not-a-crate"]);
    assert_eq!(code, 2, "an unknown crate must fail closed, not silently emit nothing");
}

/// 变更行判定：只看新增行，分母只算"可执行行"（不在 `DA:` 里的注释/空行不计入）。
#[test]
fn changed_line_coverage_counts_added_executable_lines_only() {
    let dir = tempfile::tempdir().expect("temp dir");
    let report = dir.path().join("probe.info");
    // 第 10/11/12 行可执行：两命中一未命中 ⇒ 3 行里 2 行。
    fs::write(&report, "SF:/work/repo/src/probe.rs\nDA:10,1\nDA:11,0\nDA:12,2\nLF:3\nLH:2\nend_of_record\n")
        .expect("probe report");

    let patch = dir.path().join("probe.patch");
    fs::write(
        &patch,
        "diff --git a/src/probe.rs b/src/probe.rs\n\
         --- a/src/probe.rs\n\
         +++ b/src/probe.rs\n\
         @@ -9,0 +10,3 @@\n\
         +fn a() {}\n\
         +fn b() {}\n\
         +fn c() {}\n",
    )
    .expect("probe patch");

    let (code, text) = run(
        "check_changed_line_coverage.py",
        &["--report", utf8(&report), "--diff-file", utf8(&patch), "--min-percent", "60"],
    );
    assert_eq!(code, 0, "2/3 = 66.7% clears a 60% threshold:\n{text}");
    assert!(text.contains("66.7%"), "the number must be printed:\n{text}");

    let (code, text) = run(
        "check_changed_line_coverage.py",
        &["--report", utf8(&report), "--diff-file", utf8(&patch), "--min-percent", "80"],
    );
    assert_eq!(code, 1, "66.7% must fail an 80% threshold:\n{text}");
}

/// 只有删除/上下文改动时，不该被算成"未覆盖"。
#[test]
fn a_deletion_only_change_is_not_judged() {
    let dir = tempfile::tempdir().expect("temp dir");
    let report = dir.path().join("probe.info");
    fs::write(&report, "SF:/work/repo/src/probe.rs\nDA:10,1\nend_of_record\n").expect("report");
    let patch = dir.path().join("probe.patch");
    fs::write(
        &patch,
        "diff --git a/src/probe.rs b/src/probe.rs\n\
         --- a/src/probe.rs\n\
         +++ b/src/probe.rs\n\
         @@ -10,3 +9,0 @@\n\
         -fn gone_a() {}\n\
         -fn gone_b() {}\n\
         -fn gone_c() {}\n",
    )
    .expect("patch");

    let (code, text) = run(
        "check_changed_line_coverage.py",
        &["--report", utf8(&report), "--diff-file", utf8(&patch), "--min-percent", "90"],
    );
    assert_eq!(code, 0, "a pure deletion has no added lines to judge:\n{text}");
    assert!(text.contains("没有新增"), "the tool must say why it judged nothing:\n{text}");
}

/// 变更文件不在报告里 ⇒ **红**，且要说清是 `NODATA` 而不是"0% 覆盖"。
///
/// 这是"守卫的守卫"：`affected_crates.py` 少算一个 crate、或那条腿编译失败时，
/// 本步必须报红 —— 否则"没测到"会以"通过"的形态出现在 PR 上。
#[test]
fn a_changed_file_missing_from_the_report_fails_closed() {
    let dir = tempfile::tempdir().expect("temp dir");
    let report = dir.path().join("probe.info");
    fs::write(&report, "SF:/work/repo/src/probe.rs\nDA:10,1\nend_of_record\n").expect("report");
    let patch = dir.path().join("probe.patch");
    fs::write(
        &patch,
        "diff --git a/synapse-common/src/absent.rs b/synapse-common/src/absent.rs\n\
         --- a/synapse-common/src/absent.rs\n\
         +++ b/synapse-common/src/absent.rs\n\
         @@ -0,0 +1,2 @@\n\
         +fn a() {}\n\
         +fn b() {}\n",
    )
    .expect("patch");

    let (code, text) = run(
        "check_changed_line_coverage.py",
        &["--report", utf8(&report), "--diff-file", utf8(&patch), "--min-percent", "0"],
    );
    assert_eq!(code, 2, "NODATA must be red even at threshold 0:\n{text}");
    assert!(text.contains("不在覆盖率报告里"), "the message must name the real problem:\n{text}");
}

/// 工具自身的变异自证（与 CI 里那一步同一条命令）。
#[test]
fn the_changed_line_gate_self_proves_its_own_rules() {
    let (code, text) = run("check_changed_line_coverage.py", &["--mutation-check"]);

    assert_eq!(code, 0, "the mutation check must pass:\n{text}");
    assert!(text.contains("mutation-check OK"), "it must print a verdict:\n{text}");
    assert!(!text.contains("MISS"), "every synthetic case must land on its verdict:\n{text}");
}

/// 车道的形状：PR 专属、有超时、跑的是 `-p <crate>` 的窄口径，而不是全量脚本。
#[test]
fn the_pr_lane_is_pr_scoped_and_cheap() {
    let workflow = fs::read_to_string(repo_root().join(".github/workflows/ci.yml")).expect("ci.yml");
    let job = job_block(&workflow, "pr-coverage").expect("ci.yml must declare a `pr-coverage` job");

    assert!(
        job.contains("github.event_name == 'pull_request'"),
        "the lane must be PR-only — main push is covered by the full `code-coverage` lane"
    );
    assert!(
        job.contains("timeout-minutes:"),
        "a cost-bounded lane needs an explicit ceiling: a timeout here means the scope was wrong"
    );
    assert!(job.contains("run_pr_coverage.sh"), "the lane must use the PR coverage runner:\n{job}");
    assert!(
        // ⚠️ 必须带 `scripts/ci/` 前缀：`run_pr_coverage.sh` 里含子串 `run_coverage.sh`，
        // 只写文件名的话这条断言永远为假（自证过：第一次就是这么写的）。
        !job.contains("scripts/ci/run_coverage.sh"),
        "the full-workspace runner must NOT appear here — that is the 67-minute lane"
    );
    assert!(job.contains("check_changed_line_coverage.py"), "the lane must judge changed lines:\n{job}");
    assert!(job.contains("affected_crates.py"), "the lane must derive its scope from the diff:\n{job}");
    assert!(
        job.contains("fetch-depth: 0"),
        "`git diff <base-sha>...HEAD` needs real history; a shallow clone turns the diff into \
         'everything changed' and the scope becomes the whole workspace"
    );
}

/// **口径不变量**：PR 车道与它的两个工具都不许碰合并基线。
///
/// 单腿读数是下界（实测 `src/bin/healthcheck.rs` 10.91% → 0.00%），拿它去比对
/// `coverage_baseline.json` 会让几乎每个被改到的文件都红 —— 那种门禁两周内必然被静音。
#[test]
fn the_pr_lane_never_reads_the_merged_baseline() {
    let workflow = fs::read_to_string(repo_root().join(".github/workflows/ci.yml")).expect("ci.yml");
    let job = job_block(&workflow, "pr-coverage").expect("pr-coverage job");
    assert!(!job.contains("coverage_baseline"), "the PR lane must not read the merged baseline:\n{job}");

    for name in ["affected_crates.py", "check_changed_line_coverage.py", "run_pr_coverage.sh"] {
        let text = fs::read_to_string(script(name)).expect("script must be readable");
        // 说明性注释里可以提到基线（解释"为什么不用它"），所以只看可执行文本：
        // 任何 `open(...baseline...)` / `--baseline` 参数才是真的读。
        assert!(
            !text.contains("--baseline"),
            "{name} must not take a --baseline argument: the PR leg is a lower bound, \
             not a comparable measurement"
        );
    }
}

/// 从 `ci.yml` 里切出某个 job 的整段（到下一个顶层 job key 为止）。
fn job_block(workflow: &str, job_id: &str) -> Option<String> {
    let lines: Vec<&str> = workflow.lines().collect();
    let start = lines.iter().position(|line| *line == format!("  {job_id}:"))?;
    let mut block = Vec::new();
    for line in &lines[start..] {
        let is_next_job = !block.is_empty()
            && line.starts_with("  ")
            && !line.starts_with("   ")
            && line.trim_end().ends_with(':')
            && !line.trim_start().starts_with('#');
        if is_next_job {
            break;
        }
        block.push(*line);
    }
    Some(block.join("\n"))
}

// ── 采集器（`run_pr_coverage.sh`）的探针 ─────────────────────────────────────
//
// 真跑一轮 `cargo llvm-cov` 要十几分钟且与其它测试抢 `target/`，所以用一个**假 cargo**：
// 它把 `(RUST_TEST_THREADS, argv)` 记录下来，收到 `llvm-cov report` 时写一份最小 lcov。
// 于是"每条腿的包范围/旗标顺序/降级合并/不写空报告"四件事都能直接断言。

struct Probe {
    bin: PathBuf,
    log: PathBuf,
    out: PathBuf,
    target: PathBuf,
}

fn write_executable(path: &Path, body: &str) {
    fs::write(path, body).expect("probe script must be writable");
    let mut perms = fs::metadata(path).expect("probe metadata").permissions();
    perms.set_mode(0o755);
    fs::set_permissions(path, perms).expect("probe must be executable");
}

fn probe(case: &str) -> Probe {
    let root = PathBuf::from(env!("CARGO_TARGET_TMPDIR")).join("pr_coverage_probe").join(case);
    let bin = root.join("bin");
    fs::create_dir_all(&bin).expect("probe bin dir");
    let log = root.join("argv.log");
    if log.exists() {
        fs::remove_file(&log).expect("stale log must be removable");
    }

    write_executable(
        &bin.join("cargo"),
        r#"#!/usr/bin/env bash
printf '%s\t%s\n' "${RUST_TEST_THREADS:-unset}" "$*" >> "$PROBE_ARGV"

if [ "$1" = "llvm-cov" ] && [ "$2" = "report" ]; then
    case " $* " in
        *"${FAKE_REPORT_FAIL_FOR:-__never__}"*)
            echo "fake cargo: report failed for $*" >&2
            exit 1
            ;;
    esac
    out=""
    prev=""
    for arg in "$@"; do
        if [ "$prev" = "--output-path" ]; then out="$arg"; fi
        prev="$arg"
    done
    [ -n "$out" ] || { echo "fake cargo: report without --output-path" >&2; exit 1; }
    mkdir -p "$(dirname "$out")"
    printf 'SF:%s\nDA:1,1\nLF:1\nLH:1\nend_of_record\n' "${FAKE_SF:-/work/repo/src/probe.rs}" > "$out"
    exit 0
fi

exit "${FAKE_TEST_RC:-0}"
"#,
    );
    write_executable(&bin.join("cargo-llvm-cov"), "#!/usr/bin/env bash\nexit 0\n");
    // 只有参数里含 `synapse-storage` 时脚本才会问 psql（模板表数）。
    write_executable(&bin.join("psql"), "#!/usr/bin/env bash\nprintf '222\\n'\n");

    Probe { bin, log, out: root.join("coverage"), target: root.join("target") }
}

struct Outcome {
    code: i32,
    log: String,
    text: String,
    report_written: bool,
}

fn run_runner(probe: &Probe, crates: &[&str], extra_env: &[(&str, &str)]) -> Outcome {
    let inherited = std::env::var("PATH").unwrap_or_default();
    let mut command = Command::new("bash");
    command
        .arg(script("run_pr_coverage.sh"))
        .args(crates)
        .current_dir(repo_root())
        .env("PATH", format!("{}:{}", probe.bin.display(), inherited))
        .env("PROBE_ARGV", &probe.log)
        .env("OUTPUT_DIR", &probe.out)
        .env("CARGO_TARGET_DIR", &probe.target)
        .env("TEST_DB_TEMPLATE_SCHEMA", "test_template_ci")
        .env("TEST_DATABASE_URL", "postgresql://synapse:probe@localhost:5432/synapse_test")
        // `affected_crates.py` 要跑**真** `cargo metadata`（path 首位是假 cargo）。
        .env("CARGO", std::env::var("CARGO").unwrap_or_else(|_| "cargo".to_string()))
        // `BASH_ENV` 会把 shim 目录重新前置，把假 cargo 顶掉 —— 这条用例就什么也没证明。
        .env_remove("BASH_ENV")
        // 主车道（`run_coverage.sh`）用 `env RUST_TEST_THREADS=4 cargo llvm-cov …` 起测试，
        // 于是**任何**在它里面跑的用例都会继承这个变量。不清掉的话，非 storage 腿读到的
        // 线程数永远是 4，下面"只有 storage 需要单线程"的断言必红（2026-10-28 实测：
        // `llvm-cov -p synapse-web … --lib --no-report --no-fail-fast`, threads=4）。
        // 这里量的是"脚本有没有自己钉线程数"，不是"环境里恰好有什么"。
        .env_remove("RUST_TEST_THREADS");
    for (key, value) in extra_env {
        command.env(key, value);
    }
    let output = command.output().expect("run_pr_coverage.sh must be runnable");

    Outcome {
        code: output.status.code().unwrap_or(-1),
        log: fs::read_to_string(&probe.log).unwrap_or_default(),
        text: String::from_utf8_lossy(&output.stderr).into_owned() + &String::from_utf8_lossy(&output.stdout),
        report_written: probe.out.join("lcov.info").is_file(),
    }
}

/// 每条腿都必须显式限定包范围、只跑 `--lib`、并且**先跑测试再出报告**。
///
/// 三个旗标各自对应一次真实的空手而归：缺 `-p` ⇒ 报告按"当前包"过滤；
/// 缺 `--no-report` ⇒ 测试一红就什么都不写；把 `--no-report` 放到 `--` 之后 ⇒ libtest
/// 报 unknown argument。
#[test]
fn the_runner_scopes_every_leg_and_separates_test_from_report() {
    let probe = probe("scoped");
    let outcome = run_runner(&probe, &["synapse-web", "synapse-cache", "synapse-storage"], &[]);

    assert_eq!(outcome.code, 0, "all legs green must exit 0:\n{}", outcome.text);
    assert!(outcome.report_written, "the merged report must be written:\n{}", outcome.text);

    let mut tested: Vec<&str> = Vec::new();
    for line in outcome.log.lines() {
        let (threads, argv) = line.split_once('\t').expect("probe log is `<threads>\t<argv>`");
        if argv.starts_with("llvm-cov report") {
            assert!(argv.contains("-p "), "every report must carry an explicit package scope: {argv}");
            assert!(
                !argv.contains("--workspace"),
                "`cargo llvm-cov report` rejects --workspace, and a bare report only covers the \
                 current package: {argv}"
            );
            continue;
        }
        tested.push(argv);
        assert!(argv.contains("--lib"), "the PR lane measures the lib target only: {argv}");
        assert!(
            argv.contains("--no-report"),
            "test and report must be separate calls, or a red test wipes the data: {argv}"
        );
        if argv.contains("-p synapse-storage") {
            assert_eq!(
                threads, "1",
                "storage's db_tests hit the shared `public` schema and must run single-threaded: {argv}"
            );
        } else {
            assert_eq!(threads, "unset", "only storage needs the single-threaded pin: {argv}");
        }
    }

    assert_eq!(tested.len(), 3, "one leg per crate, got:\n{}", outcome.log);
    let web = tested.iter().find(|argv| argv.contains("-p synapse-web")).expect("web leg");
    let cache = tested.iter().find(|argv| argv.contains("-p synapse-cache")).expect("cache leg");
    assert!(web.contains("--features test-utils"), "synapse-web declares test-utils, so its leg must enable it: {web}");
    assert!(
        !cache.contains("--features"),
        "synapse-cache has no test-utils feature — passing it makes cargo refuse the command: {cache}"
    );
}

/// 一条腿没产出 lcov ⇒ 降级合并（另一条腿的数据必须活下来），但仍然是红。
#[test]
fn the_runner_degrades_instead_of_erasing_everything() {
    let probe = probe("degraded");
    let outcome = run_runner(&probe, &["synapse-web", "synapse-common"], &[("FAKE_REPORT_FAIL_FOR", "synapse-web")]);

    assert!(outcome.report_written, "the surviving leg's data must still be merged:\n{}", outcome.text);
    assert!(!outcome.text.contains("Traceback"), "fail with a message, not a traceback:\n{}", outcome.text);
    assert!(outcome.text.contains("降级合并"), "the log must say a leg is missing:\n{}", outcome.text);
    assert_ne!(outcome.code, 0, "a missing leg is still red:\n{}", outcome.text);
}

/// 一条腿都没有 ⇒ **不写空报告**，明确失败。
#[test]
fn the_runner_refuses_to_write_an_empty_report() {
    let probe = probe("empty");
    let outcome = run_runner(&probe, &["synapse-web"], &[("FAKE_REPORT_FAIL_FOR", "llvm-cov")]);

    assert!(!outcome.report_written, "no data means no report:\n{}", outcome.text);
    assert!(outcome.text.contains("没有任何一条腿产出 lcov"), "the failure must name the cause:\n{}", outcome.text);
    assert_ne!(outcome.code, 0, "an empty run must be red:\n{}", outcome.text);
}
