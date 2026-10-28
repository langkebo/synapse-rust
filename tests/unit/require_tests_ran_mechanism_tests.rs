//! 守卫：`scripts/ci/require_tests_ran.sh` 的判据必须是**框架机制**，不是日志文案。
//!
//! ## 为什么要有这层守卫
//!
//! 这个包装脚本的职责是"过滤条件不再匹配任何用例时判红"。它最初靠正则匹配
//! **人类可读的进度文案**（`Starting N tests` / `test result: ok. N passed`），
//! 于是连续三次因为别人的排版变化而假红，每次修法都只是补正则：
//!
//! 1. `CARGO_TERM_COLOR=always` ⇒ 进度行带 SGR 序列，匹配不到（2026-09-20）；
//! 2. `strip_ansi | grep -q` 命中即退 + `pipefail` ⇒ 上游 EPIPE 141（2026-09-30）；
//! 3. nextest 单数 `Starting 1 test across 1 binary` ⇒ 只写复数的正则必红，
//!    `Appservice statistics cost gate` 自 `ea8276d8e` 引入起**从未绿过**（2026-10-27）。
//!
//! 现在的实现改问框架自己：nextest 注入 `--no-tests=fail`（空集由框架返回非零），
//! libtest 先跑 `--list` 数注册表行数（用例清单是结构化的，不受颜色/单复数影响）。
//!
//! 本文件把这四条行为钉住。**红证明**：
//!   * 去掉 `--no-tests=fail` 注入 ⇒ `nextest_gets_no_tests_fail_injected` FAILED；
//!   * 把 libtest 分支换回日志正则 ⇒ `libtest_empty_selection_fails_without_running_the_tests` FAILED
//!     （它会真的去跑测试，而不是只 `--list`）。
//!
//! ## 为什么不嵌套真 cargo
//!
//! 单测批次共享 `target/`，嵌套 cargo 会与其它构建抢锁（实测单条用例被拖到 440s）。
//! 这里用一个**假 cargo**：只按"是否带 `--list`"给出预设输出，并把收到的 argv
//! 落到文件里 —— 于是"注入了什么参数"和"有没有真跑测试"都能被直接断言。

use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

const WRAPPER: &str = "scripts/ci/require_tests_ran.sh";

fn repo_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
}

/// 造一个假 `cargo`，返回它所在目录（要放进 `PATH` 最前面）。
///
/// `case` 必须每个用例互不相同：假 cargo 把收到的 argv 追加到**该用例专属**的
/// `argv.log`，共用一份会被并发执行的其他用例污染（nextest 默认并行，
/// 实测 6 条里 3 条因此看到 7 次调用）。
///
/// 行为由环境变量控制，避免每个用例各写一份脚本：
/// * `FAKE_LIST=some|none` —— `--list` 时打印 2 条用例 / 0 条用例；
/// * `FAKE_RUN=ok|fail` —— 真跑时的退出码；
/// * `FAKE_NXT=empty` —— 收到 `--no-tests=fail` 时模拟"空集"，exit 4（nextest 的真实取值）。
fn fake_cargo(case: &str) -> PathBuf {
    let root = PathBuf::from(env!("CARGO_TARGET_TMPDIR")).join("require_tests_ran_probe").join(case);
    let bin = root.join("bin");
    fs::create_dir_all(&bin).expect("probe dir must be creatable");
    let path = bin.join("cargo");
    fs::write(
        &path,
        r#"#!/usr/bin/env bash
printf '%s\n' "$*" >> "$PROBE_ARGV"
case "$*" in
  *--list*)
      case "${FAKE_LIST:-some}" in
        some) printf 'probe::alpha: test\nprobe::beta: test\n\n2 tests, 0 benchmarks\n' ;;
        none) printf '\n0 tests, 0 benchmarks\n' ;;
      esac
      exit 0 ;;
esac
case "${FAKE_NXT:-}" in
  empty)
      printf '     Summary [   0.0s] 0 tests run: 0 passed, 7 skipped\nerror: no tests to run\n'
      exit 4 ;;
esac
case "${FAKE_RUN:-ok}" in
  ok) printf '    Starting 2 tests across 1 binary\n     Summary [   0.1s] 2 tests run: 2 passed\n'; exit 0 ;;
  *) printf 'test result: FAILED. 0 passed; 2 failed\n'; exit 101 ;;
esac
"#,
    )
    .expect("fake cargo must be writable");
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(&path, fs::Permissions::from_mode(0o755)).expect("fake cargo must be executable");
    }
    bin
}

struct Outcome {
    code: Option<i32>,
    stdout: String,
    stderr: String,
    /// 假 cargo 依次收到的 argv（一行一次），用于断言"注入了什么"。
    argv: Vec<String>,
}

#[allow(clippy::too_many_arguments)]
fn run_wrapper(bin: &Path, argv: &[&str], list: &str, run: &str, nxt: &str) -> Outcome {
    let log = bin.join("argv.log");
    let _ = fs::remove_file(&log);
    let system_path = std::env::var("PATH").unwrap_or_default();
    let out = Command::new("bash")
        .arg(repo_root().join(WRAPPER))
        .args(argv)
        .env("PATH", format!("{}:/usr/bin:/bin:{system_path}", bin.display()))
        .env("PROBE_ARGV", &log)
        .env("FAKE_LIST", list)
        .env("FAKE_RUN", run)
        .env("FAKE_NXT", nxt)
        .output()
        .expect("wrapper must be spawnable");
    let argv_seen = fs::read_to_string(&log).unwrap_or_default().lines().map(str::to_string).collect();
    Outcome {
        code: out.status.code(),
        stdout: String::from_utf8_lossy(&out.stdout).into_owned(),
        stderr: String::from_utf8_lossy(&out.stderr).into_owned(),
        argv: argv_seen,
    }
}

/// nextest 车道：包装脚本必须**自己**注入 `--no-tests=fail`。
///
/// 这是"问框架"的核心：没有这个旗标，nextest 在空集时退出码是 4 —— CI 只看到
/// "失败了"，而**空集**这个原因要人去读日志才看得出来；有了它，退出码本身就是判据。
#[test]
fn nextest_gets_no_tests_fail_injected() {
    let bin = fake_cargo("nextest_gets_no_tests_fail_injected");
    let out = run_wrapper(&bin, &["cargo", "nextest", "run", "--test", "unit"], "some", "ok", "");
    assert_eq!(out.code, Some(0), "nextest 正常跑完必须判绿：{}", out.stderr);
    assert_eq!(out.argv.len(), 1, "nextest 车道不该被拆成多次调用：{:?}", out.argv);
    assert!(out.argv[0].contains("--no-tests=fail"), "nextest 调用必须被注入 `--no-tests=fail`，实得：{}", out.argv[0]);
    assert!(
        !out.stderr.contains("falling back to log-text matching"),
        "nextest 是机制判据，不该走日志兜底：{}",
        out.stderr
    );
}

/// nextest 车道：空集由框架返回非零，包装脚本原样透传。
#[test]
fn nextest_empty_selection_propagates_framework_failure() {
    let bin = fake_cargo("nextest_empty_selection_propagates_framework_failure");
    let out =
        run_wrapper(&bin, &["cargo", "nextest", "run", "--test", "unit", "-E", "test(nope)"], "some", "ok", "empty");
    assert_eq!(
        out.code,
        Some(4),
        "空集必须透传 nextest 的退出码（4），实得 {:?}\nstdout={}\nstderr={}",
        out.code,
        out.stdout,
        out.stderr
    );
}

/// 调用方自己传 `--no-tests=pass` 等于关掉本门禁 —— 必须显式拒绝，而不是静默复制。
#[test]
fn nextest_no_tests_pass_is_rejected() {
    let bin = fake_cargo("nextest_no_tests_pass_is_rejected");
    let out = run_wrapper(&bin, &["cargo", "nextest", "run", "--test", "unit", "--no-tests=pass"], "some", "ok", "");
    assert_eq!(out.code, Some(2), "必须按用法错误拒绝：{}", out.stderr);
    assert!(
        out.stderr.contains("would let this gate pass on an empty selection"),
        "拒绝理由必须说明后果，实得：{}",
        out.stderr
    );
}

/// libtest 车道：空集必须判红，而且**只做 `--list` 探测**，不执行任何用例。
///
/// "不执行"是可观测的：假 cargo 只收到一次 `--list` 调用。若实现回退成
/// "先跑再读日志"，这里会看到第二次调用 —— 那意味着空集时仍然白跑了一趟
/// （对 DB 车道是几十秒 + 一次 schema 克隆）。
#[test]
fn libtest_empty_selection_fails_without_running_the_tests() {
    let bin = fake_cargo("libtest_empty_selection_fails_without_running_the_tests");
    let out = run_wrapper(
        &bin,
        &["cargo", "test", "--locked", "--test", "integration", "no_such_module", "--", "--test-threads=1"],
        "none",
        "ok",
        "",
    );
    assert_eq!(out.code, Some(1), "空集必须 exit 1：{}", out.stderr);
    assert!(out.stderr.contains("ZERO tests"), "失败信息必须点明跑了 0 个测试，实得：{}", out.stderr);
    assert_eq!(out.argv.len(), 1, "空集时不该再跑真正的测试，实际调用链：{:?}", out.argv);
    assert!(out.argv[0].ends_with("-- --list"), "唯一一次调用必须是 `--list` 探测，实得：{}", out.argv[0]);
}

/// libtest 车道：`--exact` 改变过滤语义（精确 vs 子串），探测必须沿用同一语义。
///
/// 否则最常见的那类漂移 —— 过滤串写短了 —— 会被子串匹配掩盖成"有测试"，
/// 探测通过而真正的 `--exact` 运行仍然 0 命中。
#[test]
fn libtest_probe_preserves_exact_matching() {
    let bin = fake_cargo("libtest_probe_preserves_exact_matching");
    let out = run_wrapper(
        &bin,
        &["cargo", "test", "--test", "integration", "m::tests::t", "--", "--exact", "--nocapture"],
        "some",
        "ok",
        "",
    );
    assert_eq!(out.code, Some(0), "{}", out.stderr);
    assert!(
        out.argv[0].contains("--exact") && out.argv[0].ends_with("--list"),
        "探测必须带 `--exact`，实得：{}",
        out.argv[0]
    );
}

/// libtest 车道：探测通过后真跑的退出码必须原样透传（不能被探测的成功掩盖）。
#[test]
fn libtest_real_failure_propagates() {
    let bin = fake_cargo("libtest_real_failure_propagates");
    let out = run_wrapper(&bin, &["cargo", "test", "--test", "unit", "probe"], "some", "fail", "");
    assert_eq!(
        out.code,
        Some(101),
        "真跑失败必须透传 101，实得 {:?}\nstdout={}\nstderr={}",
        out.code,
        out.stdout,
        out.stderr
    );
    assert_eq!(out.argv.len(), 2, "应当先 `--list` 再真跑：{:?}", out.argv);
}
