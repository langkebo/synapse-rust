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
// 2026-09-23：分区口径与词法剥离的红证明
//
// 旧计数器用 `grep … | wc -l`：注释/字符串里的 `sqlx::query(` 会被算作调用，且
// 生产与 `#[cfg(test)]` 混在一个数字里。以下用**临时源码树**直接驱动
// `scripts/ci/sqlx_query_census.py`，把新口径逐条钉住。
// =============================================================================

/// 在临时目录里构造一棵最小源码树并运行普查，返回输出。
fn run_census_on_tree(files: &[(&str, &str)]) -> String {
    use std::sync::atomic::{AtomicU32, Ordering};
    static SEQ: AtomicU32 = AtomicU32::new(0);

    let root = std::env::temp_dir().join(format!(
        "sqlx_census_{}_{}",
        std::process::id(),
        SEQ.fetch_add(1, Ordering::SeqCst)
    ));
    if root.exists() {
        fs::remove_dir_all(&root).expect("clean temp root");
    }
    for (rel, content) in files {
        let path = root.join(rel);
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).expect("create temp dirs");
        }
        fs::write(&path, content).expect("write temp source");
    }

    let out = Command::new("python3")
        .arg(repo_root().join("scripts/ci/sqlx_query_census.py"))
        .arg("--root")
        .arg(&root)
        .output()
        .expect("census script must be runnable with python3");
    // 指标行先于任何错误路径输出，因此**不能**要求 exit 0：
    // 只含注释/字符串的树合法地计到 0 处调用，而脚本把 total==0 当作
    // "扫描范围失效"的报警（exit 1）。这里以"指标行存在"为准。
    let stdout = String::from_utf8_lossy(&out.stdout).into_owned();
    assert!(
        stdout.contains("dynamic_production="),
        "census 未输出指标行（status={:?}）\nstdout:\n{stdout}\nstderr:\n{}",
        out.status.code(),
        String::from_utf8_lossy(&out.stderr)
    );
    let _ = fs::remove_dir_all(&root);
    stdout
}

/// 修复缺陷①：注释与字符串里提到 `sqlx::query(` **不得**计入。
#[test]
fn sqlx_census_ignores_calls_in_comments_and_strings() {
    let output = run_census_on_tree(&[(
        "src/lib.rs",
        r#"
//! prose mentioning sqlx::query( must not count
/// doc mentioning sqlx::query_as( must not count
/* block mentioning sqlx::query_scalar( must not count */
const TEMPLATE: &str = "sqlx::query( SELECT 1";
const RAW: &str = r#"sqlx::query_as( SELECT 1"#;
fn f() -> &'static str { "sqlx::query(" }
"#,
    )]);
    assert_eq!(
        parse_metric(&output, "dynamic_production="),
        Some(0),
        "注释/字符串里的 `sqlx::query(` 被算成了调用（缺陷①回归）\n输出:\n{output}"
    );
}

/// 真实调用必须被计入，且同一行的两处调用按**出现次数**计（修复缺陷②）。
#[test]
fn sqlx_census_counts_occurrences_not_lines() {
    let output = run_census_on_tree(&[(
        "src/lib.rs",
        r#"
fn f() {
    let _ = sqlx::query("SELECT 1");
    let _ = sqlx::query_as::<_, (i64,)>("SELECT 2");
    let _ = sqlx::query_scalar::<_, i64>("SELECT 3");
}
"#,
    )]);
    assert_eq!(
        parse_metric(&output, "dynamic_production="),
        Some(3),
        "turbofish 形式或同行多次调用被漏计（缺陷②回归）\n输出:\n{output}"
    );

    let same_line = run_census_on_tree(&[(
        "src/lib.rs",
        r#"fn f() { let _ = sqlx::query("SELECT 1"); let _ = sqlx::query("SELECT 2"); }"#,
    )]);
    assert_eq!(
        parse_metric(&same_line, "dynamic_production="),
        Some(2),
        "同一行的两处调用应计 2 处\n输出:\n{same_line}"
    );
}

/// 修复缺陷③：`#[cfg(test)]` 块内的调用归入 test 区，不计入生产。
#[test]
fn sqlx_census_splits_production_from_cfg_test_blocks() {
    let output = run_census_on_tree(&[(
        "src/lib.rs",
        r#"
fn production() { let _ = sqlx::query("SELECT prod"); }

#[cfg(test)]
mod tests {
    fn helper() { let _ = sqlx::query("SELECT test"); }
}
"#,
    )]);
    assert_eq!(parse_metric(&output, "dynamic_production="), Some(1), "生产侧应只计 1 处\n输出:\n{output}");
    assert_eq!(parse_metric(&output, "dynamic_test="), Some(1), "test 侧应计 1 处\n输出:\n{output}");
}

/// 由 `#[cfg(test)] mod x;` 引入的**整份测试文件**也必须归入 test 区。
#[test]
fn sqlx_census_treats_cfg_test_module_files_as_test() {
    let output = run_census_on_tree(&[
        ("src/lib.rs", "#[cfg(test)]\nmod db_tests;\n\nfn production() { let _ = sqlx::query(\"SELECT prod\"); }\n"),
        ("src/db_tests.rs", "fn helper() { let _ = sqlx::query(\"SELECT test\"); }\n"),
    ]);
    assert_eq!(parse_metric(&output, "dynamic_production="), Some(1), "生产侧应只计 1 处\n输出:\n{output}");
    assert_eq!(
        parse_metric(&output, "dynamic_test="),
        Some(1),
        "`#[cfg(test)] mod x;` 引入的整份文件应计入 test 区\n输出:\n{output}"
    );
}

/// 分区后棘轮必须有**独立的生产上限**：把它压到 1 必须失败。
#[test]
fn sqlx_ratio_gate_fails_when_production_dynamic_exceeds_baseline() {
    let (code, output) = run_gate(&[("SQLX_DYNAMIC_PRODUCTION_MAX", "1")]);
    assert_ne!(code, 0, "生产动态上限压到 1 后门禁仍通过 → 生产棘轮不生效\n输出:\n{output}");
    assert!(output.contains("FAIL"), "失败时必须给出明确输出\n输出:\n{output}");
}

/// 门禁必须同时报告生产与测试两个分区，否则分区口径没有落地。
#[test]
fn sqlx_ratio_gate_reports_production_and_test_split() {
    let (code, output) = run_gate(&[]);
    assert_eq!(code, 0, "当前工作树应通过\n输出:\n{output}");
    assert!(output.contains("production="), "输出必须包含 production=<n>\n输出:\n{output}");
    assert!(output.contains("test="), "输出必须包含 test=<n>\n输出:\n{output}");
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
