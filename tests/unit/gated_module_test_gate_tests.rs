//! D-25 守卫：**门控模块的"0 tests 假绿"**。
//!
//! ## 缺陷
//!
//! `#[cfg(feature = "X")] pub mod m;` 在 X 未打开时**根本不参与编译**，于是
//! `cargo nextest run -E 'test(m)'` 匹配 0 个用例：libtest 打印 `running 0 tests` 并
//! exit 0，nextest exit 4 —— 两者都会被 CI 当成"通过"，形成"改了一个宏却一个都没校验"
//! 的假绿。本仓已踩 4 次（C6 `server_notification`、C9 `saml`、C11 `friend_room`、
//! C15 `cas`），而且这个缺口**不体现在棘轮数字里**（census 按源码文本计数，与 feature
//! 无关）。
//!
//! ## 守卫分两层
//!
//! 1. **运行时层**：`scripts/ci/check_gated_module_tests.sh` 读
//!    `scripts/ci/gated_module_test_matrix`，对每行用**既有唯一实现**
//!    `scripts/ci/require_tests_ran.sh`（"0 个用例即失败"）包装一次 nextest 运行。
//!    CI 在 `--all-features` 车道上调用它 —— 与 lib 批次同 feature 口径，因此不额外构建。
//! 2. **静态层**（本文件）：登记表本身必须可信 —— 每个 feature 名必须真的在对应
//!    crate 的 `Cargo.toml [features]` 里、每个模块的 `pub mod` 必须真的被
//!    `#[cfg(feature = "…")]` 门控、且门禁脚本必须真的被 CI 调用（没人调用的门禁
//!    等于不存在）。此外用一次**故意的 0 命中**调用证明"0 个用例 → 失败"这条路径
//!    真的会变红（铁律 8）。

use std::collections::BTreeSet;
use std::fs;
use std::path::PathBuf;
use std::process::Command;

fn repo_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
}

fn read(relative: &str) -> String {
    let path = repo_root().join(relative);
    fs::read_to_string(&path).unwrap_or_else(|error| panic!("{} 必须可读: {error}", path.display()))
}

const MATRIX: &str = "scripts/ci/gated_module_test_matrix";
const SCRIPT: &str = "scripts/ci/check_gated_module_tests.sh";
const WRAPPER: &str = "scripts/ci/require_tests_ran.sh";

/// 登记表的一行：`<nextest 过滤器>|<所需 feature>|<声明它的 lib.rs>`
#[derive(Debug)]
struct Row {
    filter: String,
    feature: String,
    anchor: String,
}

fn rows() -> Vec<Row> {
    read(MATRIX)
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty() && !line.starts_with('#'))
        .map(|line| {
            let parts: Vec<&str> = line.split('|').collect();
            assert_eq!(parts.len(), 3, "登记表每行必须是 `<filter>|<feature>|<anchor>`，实得：{line}");
            Row {
                filter: parts[0].trim().to_string(),
                feature: parts[1].trim().to_string(),
                anchor: parts[2].trim().to_string(),
            }
        })
        .collect()
}

#[test]
fn matrix_is_non_empty_and_has_no_duplicate_filters() {
    let rows = rows();
    assert!(!rows.is_empty(), "{MATRIX} 不能为空（空表 = 门禁什么都不查）");
    let mut seen = BTreeSet::new();
    for row in &rows {
        assert!(seen.insert(row.filter.clone()), "登记表出现重复过滤器 `{}`", row.filter);
    }
    // D-25 点名的 4 个模块必须都在表里，否则那张表不能宣称覆盖了这个缺口。
    for required in ["friend_room", "server_notification", "saml", "cas"] {
        assert!(
            rows.iter().any(|row| row.filter == required),
            "D-25 点名的门控模块 `{required}` 必须登记在 {MATRIX} 中"
        );
    }
}

/// 每行声明的 feature 必须真的存在于那个 crate 的 `[features]` 段里 ——
/// feature 名写错时，运行时会"匹配 0 个用例"，但那种红很容易被误读成环境问题。
#[test]
fn every_declared_feature_exists_in_the_anchor_crate() {
    for row in rows() {
        // anchor 形如 `synapse-storage/src/lib.rs` —— crate 根是 `/src/` 之前那一段，
        // 不是该文件的父目录（那是 `src/`，下面没有 Cargo.toml）。
        let crate_dir = row.anchor.split("/src/").next().unwrap_or(&row.anchor);
        let manifest = repo_root().join(crate_dir).join("Cargo.toml");
        let text =
            fs::read_to_string(&manifest).unwrap_or_else(|error| panic!("{} 必须可读: {error}", manifest.display()));
        let features_block = text
            .split("[features]")
            .nth(1)
            .and_then(|rest| rest.split("\n[").next())
            .unwrap_or_else(|| panic!("{} 没有 [features] 段", manifest.display()));
        assert!(
            features_block.lines().any(|line| line.trim_start().starts_with(&format!("{} =", row.feature))),
            "{} 的 [features] 里没有 `{}`（登记表里的 feature 名写错了？）",
            manifest.display(),
            row.feature
        );
    }
}

/// 每个被登记的模块都必须**真的**由声明的 feature 门控：`pub mod <m>;` 的上一行
/// 必须是 `#[cfg(feature = "<f>")]`。这正是"未开 feature ⇒ 0 个用例"的静态依据，
/// 也是这张表不会随时间失效的原因。
#[test]
fn every_listed_module_is_gated_by_its_declared_feature() {
    for row in rows() {
        let text = read(&row.anchor);
        let lines: Vec<&str> = text.lines().collect();
        let decl = format!("pub mod {};", row.filter);
        let index = lines
            .iter()
            .position(|line| line.trim_end() == decl)
            .unwrap_or_else(|| panic!("{} 里找不到 `{decl}`", row.anchor));
        let previous = lines[..index]
            .iter()
            .rev()
            .find(|line| !line.trim().is_empty())
            .unwrap_or_else(|| panic!("{}:{} 之前没有任何属性行", row.anchor, index + 1));
        assert!(
            previous.contains("cfg(feature") && previous.contains(&format!("\"{}\"", row.feature)),
            "{}:{} 的上一行是 `{}`，但它必须是以 feature `{}` 为条件的 `#[cfg(...)]` \
             —— 否则这个模块并不是被该 feature 门控的，登记表与守卫就失去意义",
            row.anchor,
            index + 1,
            previous.trim(),
            row.feature
        );
    }
}

/// 门禁必须真的被 CI 调用：一个没人执行的门禁等于不存在（D-25 的原始形态就是
/// "已经记录了这个缺口，但没有自动守卫"）。
#[test]
fn the_gate_is_wired_into_ci() {
    assert!(repo_root().join(SCRIPT).is_file(), "{SCRIPT} 必须存在（运行时层的唯一实现）");
    assert!(
        repo_root().join(WRAPPER).is_file(),
        "{WRAPPER} 必须存在（\"0 个用例即失败\"的唯一实现，本脚本不得重复实现它）"
    );
    let ci = read(".github/workflows/ci.yml");
    assert!(
        ci.contains("check_gated_module_tests.sh"),
        "ci.yml 必须调用 {SCRIPT} —— 否则这个门禁永远不会跑，D-25 会原样复发"
    );
    let script = read(SCRIPT);
    assert!(
        script.contains("require_tests_ran.sh"),
        "{SCRIPT} 必须复用 {WRAPPER} 而不是自己再实现一遍\"0 个用例即失败\"（铁律 2）"
    );
}

/// 脚本必须**真的被执行过**：静态断言"文件存在且 ci.yml 调用了它"挡不住脚本自身的
/// 解析/引用缺陷 —— 实测就撞过一次：`echo "...$anchor）"` 里变量名后紧跟多字节字符，
/// bash 把 `）` 并进变量名，运行时报 `anchor: unbound variable`，而所有静态断言全绿
/// （正是 D-25 描述的"门禁看起来在工作、实际没跑"）。`--list` 模式只解析登记表、不跑
/// cargo，因此可以在这里廉价地端到端执行一遍。
#[test]
fn the_gate_script_parses_the_matrix_end_to_end() {
    let output = Command::new("bash")
        .arg(repo_root().join(SCRIPT))
        .arg("--list")
        .current_dir(repo_root())
        .output()
        .expect("gate script must be spawnable");
    assert!(
        output.status.success(),
        "{SCRIPT} --list 必须以 0 退出（解析失败就是门禁本身坏了）。stderr:\n{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let printed: Vec<String> = String::from_utf8_lossy(&output.stdout)
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty())
        .map(String::from)
        .collect();
    let expected: Vec<String> =
        rows().iter().map(|row| format!("{}|{}|{}", row.filter, row.feature, row.anchor)).collect();
    assert_eq!(printed, expected, "{SCRIPT} --list 必须逐行回显登记表（顺序也要一致）");
}

/// 铁律 8：证明"0 个用例 ⇒ 失败"这条判据真的会红。
///
/// 这里**不**嵌套调用 cargo：单测批次跑在共享 target 目录上，嵌套 cargo 会与其它
/// 构建抢锁（实测单个用例被拖到 440s）。改用两条极小的替身命令，直接、确定性地
/// 检验 `require_tests_ran.sh` 的判定逻辑本身：
///   * 一个退出 0 但**什么都不打印**的命令 → 脚本必须报 "ran ZERO tests" 并 exit 1（RED）；
///   * 一个打印 nextest 复数行 `Starting 3 tests …` 的命令 → 脚本必须 exit 0（正控，
///     否则上面的红可能只是"脚本恒失败"）。
///
/// 端到端（真 cargo + 真 feature）那条路由 CI 步骤
/// `bash scripts/ci/check_gated_module_tests.sh` 覆盖，本文件另有静态断言保证它被 CI 调用。
#[test]
fn the_zero_tests_judgement_goes_red_and_a_real_run_passes() {
    let run_wrapper = |args: &[&str]| {
        Command::new("bash")
            .arg(repo_root().join(WRAPPER))
            .args(args)
            .current_dir(repo_root())
            .output()
            .expect("wrapper must be spawnable")
    };

    // RED：命令成功但没有任何测试输出 ⇒ 必须失败。
    let zero = run_wrapper(&["true"]);
    assert!(
        !zero.status.success(),
        "一个「exit 0 且无任何测试输出」的命令必须被判为空转，但 {WRAPPER} 返回了成功：\n{}{}",
        String::from_utf8_lossy(&zero.stdout),
        String::from_utf8_lossy(&zero.stderr)
    );
    assert!(
        String::from_utf8_lossy(&zero.stderr).contains("ZERO tests"),
        "失败信息必须明确指出「跑了 0 个测试」，实得：\n{}",
        String::from_utf8_lossy(&zero.stderr)
    );

    // 正控：nextest 的复数输出形态必须被认作"真的跑了测试"。
    let real = run_wrapper(&["printf", "   Starting 3 tests across 1 binary\n   3 tests run: 3 passed\n"]);
    assert!(
        real.status.success(),
        "带 `Starting 3 tests` 的输出必须通过 —— 否则上面的 RED 只是脚本恒失败，证据无效：\n{}{}",
        String::from_utf8_lossy(&real.stdout),
        String::from_utf8_lossy(&real.stderr)
    );
}
