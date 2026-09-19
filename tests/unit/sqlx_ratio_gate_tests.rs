//! Gate tests for `scripts/ci/check_sqlx_dynamic_ratio.sh` (SQLx 动态/静态查询棘轮).
//!
//! ## 为什么需要这个文件
//!
//! 原脚本存在三处缺陷，且**没有任何测试真正执行过它**：
//!
//! 1. **扫描范围错误**：只 `grep ... src/`（根 crate），而 SQL 调用绝大多数在
//!    workspace crate。实测 `src/` 仅 25 处，`synapse-storage/src/` 有 1,123 处
//!    —— 报告数字基于约 1.7% 的样本。
//! 2. **阈值不可达**：硬编码 `max=0.30`，而实测动态占比约 96%；脚本自述引用的
//!    2026-06-03 审计就已记录 99.6% 动态。一个永远失败的门禁等于没有门禁。
//! 3. **死引用**：错误信息指向不存在的 `docs/synapse-rust/M3_SQLX_MIGRATION_PLAN.md`。
//!
//! 本文件用**子进程实际执行脚本**来锁定行为（对照 `sliding_sync_perf_gate_tests.rs`
//! 的教训：那个文件只用 Rust 重写了一遍解析逻辑，从不执行脚本，因此脚本长期
//! 失效却始终"测试通过"。该文件已于 2026-09-19 删除 —— 真门禁是
//! `benchmark.yml` 里实际调用的 `scripts/ci/sliding_sync_perf_gate.sh`，
//! Rust 侧副本属重复实现，见 `docs/audit/GATE_INTEGRITY_SWEEP_2026-09-19.md` §3 B5）。

use std::fs;
use std::path::PathBuf;
use std::process::Command;

/// 仓库根目录（`CARGO_MANIFEST_DIR` 即根 crate 目录）。
fn repo_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
}

fn gate_script_path() -> PathBuf {
    repo_root().join("scripts/ci/check_sqlx_dynamic_ratio.sh")
}

fn baseline_path() -> PathBuf {
    repo_root().join("scripts/ci/sqlx_dynamic_ratio_baseline")
}

/// 以给定环境运行门禁脚本，返回 `(exit_code, stdout+stderr)`。
fn run_gate(extra_env: &[(&str, &str)]) -> (i32, String) {
    let mut cmd = Command::new("bash");
    cmd.arg(gate_script_path());
    cmd.current_dir(repo_root());
    for (k, v) in extra_env {
        cmd.env(k, v);
    }
    let out = cmd.output().expect("gate script must be spawnable");
    let mut combined = String::from_utf8_lossy(&out.stdout).into_owned();
    combined.push_str(&String::from_utf8_lossy(&out.stderr));
    (out.status.code().unwrap_or(-1), combined)
}

// =============================================================================
// 存在性
// =============================================================================

#[test]
fn sqlx_ratio_gate_exists_and_is_a_file() {
    let path = gate_script_path();
    assert!(path.is_file(), "gate script should exist at {path:?}");
}

/// 棘轮基线必须是一个被版本控制的显式文件，而不是散落在脚本里的魔法数字。
#[test]
fn sqlx_ratio_gate_ships_an_explicit_baseline_file() {
    let path = baseline_path();
    assert!(
        path.is_file(),
        "expected an explicit ratchet baseline at {path:?}; \
         硬编码绝对阈值会让门禁要么永远失败、要么永远无用"
    );
}

// =============================================================================
// 缺陷 1：扫描范围必须覆盖 workspace crate
// =============================================================================

/// 报告的 `total` 必须覆盖 workspace，而不是只有根 crate 的 `src/`。
///
/// `synapse-storage` 单独就有 1,000+ 处动态调用，因此 workspace 扫描的
/// `total` 必然远大于仅扫 `src/` 的 25。
#[test]
fn sqlx_ratio_gate_scans_the_whole_workspace_not_only_root_src() {
    let (code, output) = run_gate(&[("SQLX_DYNAMIC_RATIO_MAX", "1.0")]);
    assert_eq!(code, 0, "以 max=1.0 运行应当通过（仅用于读取计数），实际输出:\n{output}");

    let total = parse_metric(&output, "total=").expect("输出必须包含 total=<n>");
    assert!(
        total > 1000,
        "扫描范围的 total 仅 {total}；说明只扫了根 crate 的 src/，\
         漏掉了 synapse-storage 等 workspace crate（缺陷 1 回归）\n输出:\n{output}"
    );
}

/// 扫描必须计入 workspace crate 中的动态调用，而不是只计入根 crate。
#[test]
fn sqlx_ratio_gate_counts_dynamic_calls_in_workspace_crates() {
    let (_, output) = run_gate(&[("SQLX_DYNAMIC_RATIO_MAX", "1.0")]);
    let dynamic = parse_metric(&output, "dynamic=").expect("输出必须包含 dynamic=<n>");
    assert!(dynamic > 1000, "dynamic 仅 {dynamic}，workspace crate 的动态 SQL 未被计入\n输出:\n{output}");
}

/// 扫描必须排除 `.claude/worktrees/` 下的旧仓库副本，否则计数会随本地
/// worktree 状态漂移，棘轮基线无法稳定复现。
#[test]
fn sqlx_ratio_gate_excludes_stale_worktree_copies() {
    let (_, output) = run_gate(&[("SQLX_DYNAMIC_RATIO_MAX", "1.0")]);
    assert!(!output.contains(".claude/worktrees"), "扫描结果中不应出现 .claude/worktrees 下的路径\n输出:\n{output}");
    // 同步断言脚本确实带了排除规则，避免"恰好没有命中"的假通过。
    let script = fs::read_to_string(gate_script_path()).expect("gate script must be readable");
    assert!(script.contains(".claude"), "脚本必须显式排除 .claude/（旧 worktree 副本会污染计数）");
}

// =============================================================================
// 缺陷 3：不得引用不存在的文档
// =============================================================================

/// 脚本**实际会输出/执行**的文档引用必须真实存在。
///
/// 这里刻意排除注释行：原缺陷是失败信息里
/// `请参考 docs/.../M3_SQLX_MIGRATION_PLAN.md` 指向不存在的文件，
/// 会把运维引向死路。而脚本头部"本次修复了什么"的历史注释属于文档，
/// 不应阻止门禁通过。
#[test]
fn sqlx_ratio_gate_does_not_reference_missing_docs() {
    let script = fs::read_to_string(gate_script_path()).expect("gate script must be readable");

    // 去掉整行注释（bash `#`）。字符串内出现 `#` 的引用在本脚本中不存在。
    let effective: String =
        script.lines().filter(|line| !line.trim_start().starts_with('#')).collect::<Vec<_>>().join("\n");

    for token in effective.split_whitespace() {
        let cleaned = token.trim_matches(|c: char| c == '"' || c == '\'' || c == '`' || c == ')' || c == ',');
        if cleaned.starts_with("docs/") {
            assert!(repo_root().join(cleaned).exists(), "脚本（非注释部分）引用了不存在的文档 `{cleaned}`（死引用）");
        }
    }
}

/// 上一条测试不能是空转：脚本必须确实**曾经**引用过该文档路径，
/// 否则它无法代表本次修复的缺陷形态。
#[test]
fn sqlx_ratio_gate_historically_referenced_a_plan_doc() {
    let script = fs::read_to_string(gate_script_path()).expect("gate script must be readable");
    assert!(
        script.contains("M3_SQLX_MIGRATION_PLAN"),
        "脚本头部应保留说明该死引用已被修复的注释；\
         若删除，请同步删除本测试"
    );
    assert!(
        !repo_root().join("docs/synapse-rust/M3_SQLX_MIGRATION_PLAN.md").exists(),
        "该计划文档若已被创建，请让门禁改为引用它，并移除本测试"
    );
}

// =============================================================================
// 棘轮语义：动态不得增加、静态不得减少
// =============================================================================

/// 棘轮必须真的能拦住回归：把基线压到 0 应当失败，而不是静默通过。
#[test]
fn sqlx_ratio_gate_fails_when_dynamic_exceeds_baseline() {
    // 通过环境变量把允许的动态上限压到远低于实际值 → 必须失败。
    let (code, output) = run_gate(&[("SQLX_DYNAMIC_MAX_BASELINE", "1")]);
    assert_ne!(
        code, 0,
        "把动态上限压到 1 后门禁仍通过 → 棘轮不生效（这是原 max=0.30 缺陷的另一面：\
         既永远失败又无法表达真实基线）\n输出:\n{output}"
    );
    assert!(output.contains("FAIL") || output.contains("fail"), "失败时必须给出明确输出\n输出:\n{output}");
}

/// 默认（无环境变量）运行必须**通过** —— 否则说明基线没有和代码同步，
/// 门禁一接入 CI 就是红的。
#[test]
fn sqlx_ratio_gate_passes_on_current_tree() {
    let (code, output) = run_gate(&[]);
    assert_eq!(
        code, 0,
        "当前工作树应当与棘轮基线一致并通过；若刚新增了动态 SQL，\
         请更新 scripts/ci/sqlx_dynamic_ratio_baseline\n输出:\n{output}"
    );
}

/// 门禁必须报告 static 计数，因为棘轮的另一半是"静态不得减少"。
#[test]
fn sqlx_ratio_gate_reports_static_count() {
    let (_, output) = run_gate(&[]);
    let static_count = parse_metric(&output, "static=").expect("输出必须包含 static=<n>");
    assert!(
        static_count > 0,
        "static 计数为 0，但 workspace 中确实存在 query!/query_as! 宏调用\
         （synapse-storage 的 refresh_token/token 模块）\n输出:\n{output}"
    );
}

// =============================================================================
// helpers
// =============================================================================

/// 从门禁输出里取出 `key=<digits>` 形式的计数。
fn parse_metric(output: &str, key: &str) -> Option<u64> {
    let idx = output.find(key)?;
    let rest = &output[idx + key.len()..];
    // 容忍 `key= 123`、`key=-1` 之类的空白/符号。
    let rest = rest.trim_start_matches(|c: char| c.is_whitespace() || c == '+' || c == '-');
    let digits: String = rest.chars().take_while(char::is_ascii_digit).collect();
    digits.parse().ok()
}
