//! 守卫：`scripts/ci/check_metric_instrumentation.py` 必须能在**任何平台**得出同一个结论。
//!
//! 为什么需要这个文件（2026-09-22 实测的假绿/假红配对）：
//!
//! 该脚本用 `git grep -E` 做内容检索，而 `git grep -E` 走各平台的 **POSIX ERE** 引擎。
//! 它原来的模式用了 PCRE 的 `\b` / `\s`，于是同一份代码在两个平台上结论相反：
//!
//! | 平台 | `git grep -E '\bfn\s+record_auth_attempt\s*[(<]'` | 后果 |
//! |---|---|---|
//! | macOS（git 自带 regex） | **零命中** | 同名冲突集合为空 ⇒ 检查静默失效（**假绿**） |
//! | Linux（glibc） | 命中 `server_metrics.rs:360` 的**定义** | 25 个埋点方法全判"同名冲突" ⇒ 可判定 0 个 ⇒ 棘轮 stale 规则报 `FAIL 基线已过期`（**假红**） |
//!
//! Linux 上的这条假红正是每个 PR 两条 default-features 车道变红、进而让整个慢速车道
//! （Integration / **Code Coverage**）被 skipped 的原因。修法：POSIX 字符类 + 明确排除
//! 定义文件本身 + 给脚本一个 `--self-test` 让"模式在本机真的命中定义"可被断言。
//!
//! 守卫内容（都走脚本自己的机器接口，不复制它的实现）：
//! ① `--self-test` 必须 exit 0 —— 它内含"POSIX 模式在本机 git grep 上命中定义"
//!    （平台回归测试）、"定义文件被排除"、"跨类型重名仍被检出"（正对照）、
//!    以及"POSIX 分支里不得出现 `\s`/`\b`"（与平台无关的形状检查）；
//! ② 正常校验必须 exit 0（未接通集合 == 基线）；
//! ③ `--print-ambiguous` 不得把 `record_auth_attempt` 列进去（它只在 ServerMetrics 上定义）。
//!
//! **红证明**：把 `_POSIX_WORD_START` 改回 `\b`（或在 POSIX 分支里写 `\s`）→ ① FAILED；
//! 把基线删一行（未接通集合 ≠ 基线）→ ② FAILED。

use std::fs;
use std::path::PathBuf;
use std::process::Command;

fn repo_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
}

/// 去掉注释行后的步骤正文。
///
/// 扫描型守卫的老坑：**注释里会提到被禁止的东西**（这里就有
/// "`fail_ci_if_error: false`（2026-09-22 用户裁定）" 这种说明文字）。不剥注释时，
/// 把真实的 YAML 键改回 `true` 守卫也照样通过 —— 自证变红的第一步就是别匹配自己的说明。
fn without_comments(block: &str) -> String {
    block.lines().filter(|l| !l.trim_start().starts_with('#')).collect::<Vec<_>>().join("\n")
}

fn run_checker(args: &[&str]) -> (i32, String) {
    let out = Command::new("python3")
        .arg("scripts/ci/check_metric_instrumentation.py")
        .args(args)
        .current_dir(repo_root())
        .output()
        .expect("python3 必须可运行（本守卫的前提）");
    let mut text = String::from_utf8_lossy(&out.stdout).into_owned();
    text.push_str(&String::from_utf8_lossy(&out.stderr));
    (out.status.code().unwrap_or(-1), text)
}

/// ① 脚本的自证必须是绿的（平台无关）。
#[test]
fn metric_instrumentation_gate_self_test_passes() {
    let (code, out) = run_checker(&["--self-test"]);
    assert_eq!(
        code, 0,
        "check_metric_instrumentation.py --self-test 必须通过：它证明 POSIX 模式在**本机**的 \
         git grep 上真的命中 server_metrics.rs 的定义、定义文件被正确排除、跨类型重名仍被检出，\
         且 POSIX 分支里没有 `\\s` / `\\b`。失败说明这个门禁又会变成平台相关（macOS 假绿 / \
         Linux 假红）。输出：\n{out}"
    );
    assert!(out.contains("OK self-test"), "自证输出应包含结论行：\n{out}");
}

/// ② 当前树上的正常校验必须是绿的（未接通集合 == 基线）。
#[test]
fn metric_instrumentation_gate_passes_on_current_tree() {
    let (code, out) = run_checker(&[]);
    assert_eq!(
        code, 0,
        "当前树必须通过埋点可达性棘轮（未接通集合与 scripts/ci/metric_instrumentation_baseline \
         一致）。若失败：接通了埋点就重跑 `--update` 收紧基线；新增未接通埋点则应实现它。输出：\n{out}"
    );
}

/// ③ 定义文件不能被算成"同名冲突"。
#[test]
fn metric_instrumentation_gate_excludes_the_definition_file_from_ambiguity() {
    let (code, out) = run_checker(&["--print-ambiguous"]);
    assert_eq!(code, 0, "`--print-ambiguous` 必须 exit 0：\n{out}");
    let ambiguous: Vec<&str> = out.lines().map(str::trim).filter(|l| !l.is_empty()).collect();
    assert!(
        !ambiguous.contains(&"record_auth_attempt"),
        "`record_auth_attempt` 只在 ServerMetrics 上定义 ⇒ 不得被判为同名冲突（定义文件必须排除）。\
         实际歧义集合：{ambiguous:?}"
    );
}

// ─────────────────────────────────────────────────────────────────────────────
// 覆盖率链路：命令只能有一份实现，且首次真跑必须真的能到那一步
// ─────────────────────────────────────────────────────────────────────────────

/// 覆盖率命令**只有一份实现**：`scripts/ci/run_coverage.sh`。
///
/// 2026-09-22 之前同一件事有两份实现（`ci.yml::Run coverage` 内联 + `scripts/run_local_coverage.sh`），
/// 两边的 storage 步 feature 集已经不同（8 vs 10）。per-file 棘轮是单向的
/// （`save_baseline` 取 `max(prev, cur)`，只降不升会一直红），口径分叉的代价是**永久红**。
/// 本地那份已按用户裁定删除，`ci.yml` 改为调用同一个脚本。
///
/// **红证明**：把 `cargo llvm-cov` 命令写回 `ci.yml` → FAILED；把 `run_coverage.sh` 的
/// `--exclude synapse-storage` 删掉 → FAILED（脚本形态断言）。
#[test]
fn coverage_command_has_a_single_implementation() {
    let root = repo_root();
    let ci = fs::read_to_string(root.join(".github/workflows/ci.yml")).expect("read ci.yml");
    let script = fs::read_to_string(root.join("scripts/ci/run_coverage.sh")).expect("read run_coverage.sh");

    // ① ci.yml 里不得再有 llvm-cov 命令（注释里说明历史不算）。
    let offenders: Vec<&str> =
        ci.lines().map(str::trim).filter(|l| !l.starts_with('#') && l.contains("cargo llvm-cov")).collect();
    assert!(
        offenders.is_empty(),
        "覆盖率命令只能写在 scripts/ci/run_coverage.sh 里（CI 与本地共用同一条命令）；\
         ci.yml 里出现内联命令就会与基线口径分叉：{offenders:?}"
    );

    // ② CI 必须调用那个脚本。
    assert!(
        ci.contains("bash scripts/ci/run_coverage.sh"),
        "ci.yml 的 coverage job 必须调用 scripts/ci/run_coverage.sh"
    );

    // ③ 本地那份重复实现必须已经删除。
    assert!(
        !root.join("scripts/run_local_coverage.sh").exists(),
        "scripts/run_local_coverage.sh 是第二份实现（2026-09-22 用户裁定删除）；\
         若确实需要本地便利入口，请让它调用 scripts/ci/run_coverage.sh，而不是再复制一份命令"
    );

    // ④ 脚本本身必须保住两步结构的关键性质（storage 单独、单线程、被排除在第二步之外）。
    for needle in [
        "cargo llvm-cov -p synapse-storage",
        "RUST_TEST_THREADS=1",
        "--exclude synapse-storage",
        "-- --skip ledger_export_tests",
        "scripts/merge_lcov.py",
        "TEST_DB_TEMPLATE_SCHEMA",
    ] {
        assert!(
            script.contains(needle),
            "scripts/ci/run_coverage.sh 必须保留 `{needle}`（两步结构的根因见其头部注释）"
        );
    }
}

/// 首次真跑必须真的能走到棘轮那一步：Codecov 不得阻塞，llvm-cov 必须钉版本。
///
/// 两条都是"首次运行必然踩"的坑（2026-09-22 实测）：
///   ① Codecov 步骤曾是 `fail_ci_if_error: true`，而 `gh secret list` 为空 —— tokenless
///      上传一旦失败，就会在棘轮**已经判定通过之后**把整个 job 弄红；用户裁定
///      "门禁是 per-file 棘轮，Codecov 只是可视化" ⇒ 必须 false。
///   ② `cargo install cargo-llvm-cov --locked` 不钉版本 ⇒ 同一条命令在不同日期可能给出
///      不同覆盖率，per-file 棘轮的结论不可复现。
///
/// **红证明**：把 `fail_ci_if_error` 改回 true → FAILED；把 `--version 0.8.7` 删掉 → FAILED。
#[test]
fn coverage_job_cannot_be_blocked_by_codecov_and_pins_llvm_cov() {
    let root = repo_root();
    let ci = fs::read_to_string(root.join(".github/workflows/ci.yml")).expect("read ci.yml");

    let codecov_step = ci
        .split("- name: ")
        .find(|s| s.starts_with("Upload coverage to Codecov"))
        .expect("ci.yml 必须有 `Upload coverage to Codecov` 步骤（可视化）");
    let codecov_keys = without_comments(codecov_step);
    assert!(
        codecov_keys.lines().any(|l| l.trim() == "fail_ci_if_error: false"),
        "Codecov 是可视化、不是门禁（用户 2026-09-22 裁定）：`fail_ci_if_error` 必须为 false，\
         否则仓库没有 CODECOV_TOKEN 时会在棘轮通过之后把 job 弄红：\n{codecov_step}"
    );

    let install_step = ci
        .split("- name: ")
        .find(|s| s.starts_with("Install cargo-llvm-cov"))
        .expect("ci.yml 必须有 `Install cargo-llvm-cov` 步骤");
    let install_cmd = without_comments(install_step);
    assert!(
        install_cmd.contains("--version 0.8.7"),
        "cargo-llvm-cov 必须钉版本（覆盖率棘轮的结论要可复现）：\n{install_step}"
    );
}

/// coverage job 的棘轮步骤必须带齐 CI 口径的旗标（P0-1：它从未执行过，所以只能静态钉住）。
///
/// **红证明**：删掉 `--non-unit-coverable` → FAILED；把 `--format lcov` 删掉 → FAILED。
#[test]
fn coverage_ratchet_step_declares_the_documented_flags() {
    let root = repo_root();
    let ci = fs::read_to_string(root.join(".github/workflows/ci.yml")).expect("read ci.yml");
    let step = ci
        .split("- name: ")
        .find(|s| s.starts_with("Per-file coverage ratchet"))
        .expect("ci.yml 必须有 `Per-file coverage ratchet` 步骤");

    for needle in [
        "--report coverage/lcov.info",
        "--format lcov",
        "--baseline scripts/ci/coverage_baseline.json",
        "--new-file-floor 30",
        "--core-files scripts/ci/core_file_coverage_prefixes.txt",
        "--core-threshold 70",
        "--non-unit-coverable scripts/ci/non_unit_coverable_prefixes.txt",
    ] {
        assert!(
            without_comments(step).contains(needle),
            "coverage 棘轮步骤必须带 `{needle}`（注释里的说明不算）：\n{step}"
        );
    }
}
