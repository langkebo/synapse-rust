#![allow(clippy::unwrap_used, clippy::expect_used)]

//! 守卫：把"覆盖率低于基线"归因成**分母增长**还是**命中行减少**。
//!
//! ## 缺陷
//!
//! 基线只存 `line_pct` 时，两种完全不同的处境在棘轮眼里**一模一样**：
//!
//! * 文件从 100 行涨到 200 行、命中行从 60 涨到 60 ⇒ 60% → 30%：新代码没被覆盖；
//! * 文件还是 100 行、命中行从 60 掉到 40 ⇒ 60% → 40%：覆盖它的测试不再跑。
//!
//! 前者不是退化，后者是。2026-10-28 的第一份完整报告（674 个文件）里有 128 条这样的
//! "低于基线"，而按 `line_pct` 根本分不开 —— 于是那句 `128 条真实下降` 是我自己写错的
//! 结论（见 `docs/audit/TEST_COMPROMISE_AUDIT_2026-10-25.md` §3.1.7）。
//!
//! ## 钉住的性质
//!
//! 1. **成对**：`line_pct` 与 `lines_found` 必须来自同一次测量。floor 保持旧值时，
//!    分母也必须保持旧的 —— 拿本次 LF 配旧的 pct 会凭空造出"命中行"。
//! 2. **未知就是未知**：老条目没有 `lines_found` ⇒ 分类器打 `DROP` + `?→45`，
//!    绝不填 0、也不猜。
//! 3. **归因不改退出码**：这是诊断工具，判红仍由 `check_file_coverage.py` 负责。

use std::fs;
use std::path::PathBuf;
use std::process::Command;

fn repo_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
}

struct Outcome {
    code: i32,
    text: String,
    baseline: String,
}

/// 跑一次真实棘轮，回传退出码 / 合并输出 / **写回后**的基线。
///
/// `records` = `(lcov SF 行, LF, LH)`；`baseline` = `(归一化 key, pct, 可选 LF)`。
fn run_ratchet(records: &[(&str, u32, u32)], baseline: &[(&str, f64, Option<u32>)], extra: &[&str]) -> Outcome {
    let dir = tempfile::tempdir().expect("temp dir");
    let report = dir.path().join("probe.info");
    let baseline_file = dir.path().join("baseline.json");

    let mut lcov = String::new();
    for (path, lf, lh) in records {
        lcov.push_str(&format!("SF:{path}\nLF:{lf}\nLH:{lh}\nend_of_record\n"));
    }
    fs::write(&report, lcov).expect("synthetic lcov must be writable");

    let entries: Vec<String> = baseline
        .iter()
        .map(|(path, pct, lf)| match lf {
            Some(lf) => format!("{{\"path\":\"{path}\",\"line_pct\":{pct},\"lines_found\":{lf}}}"),
            None => format!("{{\"path\":\"{path}\",\"line_pct\":{pct}}}"),
        })
        .collect();
    fs::write(&baseline_file, format!("{{\"files\":[{}]}}\n", entries.join(",")))
        .expect("synthetic baseline must be writable");

    let output = Command::new("python3")
        .arg(repo_root().join("scripts/check_file_coverage.py"))
        .arg("--report")
        .arg(&report)
        .arg("--baseline")
        .arg(&baseline_file)
        .args(["--global-floor", "0", "--new-file-floor", "0", "--core-threshold", "0"])
        .args(extra)
        .current_dir(repo_root())
        .output()
        .expect("python3 must be runnable");

    let mut text = String::from_utf8_lossy(&output.stdout).into_owned();
    text.push_str(&String::from_utf8_lossy(&output.stderr));
    Outcome {
        code: output.status.code().unwrap_or(-1),
        text,
        baseline: fs::read_to_string(&baseline_file).unwrap_or_default(),
    }
}

/// 从基线 JSON 里取某条目的字段（`None` = 字段不存在）。
fn field(baseline_json: &str, path: &str, key: &str) -> Option<String> {
    let idx = baseline_json.find(&format!("\"path\": \"{path}\""))?;
    let rest = &baseline_json[idx..];
    let needle = format!("\"{key}\": ");
    let start = rest.find(&needle)? + needle.len();
    let tail = &rest[start..];
    // 条目边界：`}` 或 `,` 结束这个字段。
    let end = tail.find(|c: char| !(c.is_ascii_digit() || c == '.' || c == '-')).unwrap_or(tail.len());
    Some(tail[..end].to_string())
}

fn pct_of(baseline_json: &str, path: &str) -> f64 {
    field(baseline_json, path, "line_pct")
        .unwrap_or_else(|| panic!("baseline must still contain {path}:\n{baseline_json}"))
        .parse()
        .expect("line_pct must be numeric")
}

fn lf_of(baseline_json: &str, path: &str) -> Option<u32> {
    field(baseline_json, path, "lines_found").map(|raw| raw.parse().expect("lines_found must be numeric"))
}

/// lcov 侧写法；基线侧是 `_normalize_path` 之后的 key（没有 `src/` 段）。
const PROBE_SF: &str = "synapse-common/src/probe.rs";
const PROBE: &str = "synapse-common/probe.rs";

/// (1) 棘轮正常收紧时，分母跟着本次测量一起写入 —— 这是后续能归因的前提。
#[test]
fn tightening_writes_the_measurement_with_its_denominator() {
    let out = run_ratchet(&[(PROBE_SF, 100, 90)], &[(PROBE, 80.0, None)], &[]);
    assert_eq!(out.code, 0, "90% 高于 80% 的 floor，不该红：\n{}", out.text);
    assert_eq!(pct_of(&out.baseline, PROBE), 90.0);
    assert_eq!(
        lf_of(&out.baseline, PROBE),
        Some(100),
        "floor 涨到 90 时，它的分母（LF 100）必须一起记下来，否则下一轮无法归因：\n{}",
        out.baseline
    );
}

/// (2) **反造假**：floor 保持旧值时，分母也必须保持旧的。
///
/// 这是整条改动的核心不变量。若在回归分支里写本次 LF，基线就会存成
/// "59.47% × 769 行"这种拼出来的组合 —— 换算回去是 457 行命中，而真实值可能是别的。
/// 一个错的"命中行数"比"未知"危险得多：它会让分类器把真退化判成非退化。
#[test]
fn a_regression_keeps_the_old_denominator_not_the_new_one() {
    let out = run_ratchet(&[(PROBE_SF, 769, 283)], &[(PROBE, 59.47, Some(476))], &[]);
    assert_eq!(out.code, 1, "低于 floor 必须红：\n{}", out.text);
    assert_eq!(pct_of(&out.baseline, PROBE), 59.47, "floor 不许被洗白：\n{}", out.baseline);
    assert_eq!(
        lf_of(&out.baseline, PROBE),
        Some(476),
        "分母必须是**旧**的 476（与 59.47% 同源），不能写成本次的 769：\n{}",
        out.baseline
    );
}

/// (3) 老条目（没有分母）回归时，**不许**凭空补一个分母。
///
/// 这是"未知"与"零"的区别：补 0 会让 `?→769` 变成 `0→769`，而 0 行可执行的文件
/// 根本不可能有 59.47% 的 floor —— 自相矛盾的读数比缺失更难查。
#[test]
fn a_legacy_entry_does_not_gain_a_denominator_it_never_had() {
    let out = run_ratchet(&[(PROBE_SF, 100, 80)], &[(PROBE, 90.0, None)], &[]);
    assert_eq!(out.code, 1, "低于 floor 必须红：\n{}", out.text);
    assert_eq!(pct_of(&out.baseline, PROBE), 90.0);
    assert_eq!(
        lf_of(&out.baseline, PROBE),
        None,
        "老条目没有分母，本轮也没赚到它 —— 保持缺失，不猜：\n{}",
        out.baseline
    );
}

/// (4) 显式 `--allow-lower` 是"接受这个新 floor"，分母随新 floor 一起换。
#[test]
fn allow_lower_writes_the_new_pair() {
    let out = run_ratchet(&[(PROBE_SF, 100, 80)], &[(PROBE, 90.0, Some(500))], &["--allow-lower", PROBE]);
    assert_eq!(out.code, 0, "点名下调不该红：\n{}", out.text);
    assert_eq!(pct_of(&out.baseline, PROBE), 80.0);
    assert_eq!(
        lf_of(&out.baseline, PROBE),
        Some(100),
        "floor 落到 80 时，分母必须换成同一轮的 100，不能留着 500：\n{}",
        out.baseline
    );
}

// ── 分类器：DENOM / LOST-HITS / 未知 ──────────────────────────────────────────

struct Drift {
    text: String,
}

impl Drift {
    /// 该文件被归到了哪一类 —— 只看**行首**的类别列。
    ///
    /// 不能用 `text.contains("DENOM")`：结论行里永远有 `DENOM 0 = …`，那种断言在
    /// 任何输入下都成立，等于没测。
    fn kind_of(&self, path: &str) -> Option<String> {
        self.text
            .lines()
            .filter(|line| !line.starts_with(char::is_whitespace) && line.contains(path))
            .find_map(|line| line.split_whitespace().next().map(str::to_string))
    }
}

/// 跑一次真实的 `classify_coverage_drift.py`（`--only-drops`，即所有低于基线的行）。
fn classify(records: &[(&str, u32, u32)], baseline: &[(&str, f64, Option<u32>)]) -> Drift {
    let dir = tempfile::tempdir().expect("temp dir");
    let report = dir.path().join("probe.info");
    let baseline_file = dir.path().join("baseline.json");

    let mut lcov = String::new();
    for (path, lf, lh) in records {
        lcov.push_str(&format!("SF:{path}\nLF:{lf}\nLH:{lh}\nend_of_record\n"));
    }
    fs::write(&report, lcov).expect("synthetic lcov must be writable");

    let entries: Vec<String> = baseline
        .iter()
        .map(|(path, pct, lf)| match lf {
            Some(lf) => format!("{{\"path\":\"{path}\",\"line_pct\":{pct},\"lines_found\":{lf}}}"),
            None => format!("{{\"path\":\"{path}\",\"line_pct\":{pct}}}"),
        })
        .collect();
    fs::write(&baseline_file, format!("{{\"files\":[{}]}}\n", entries.join(",")))
        .expect("synthetic baseline must be writable");

    let output = Command::new("python3")
        .arg(repo_root().join("scripts/ci/classify_coverage_drift.py"))
        .arg("--report")
        .arg(&report)
        .arg("--baseline")
        .arg(&baseline_file)
        .arg("--only-drops")
        .current_dir(repo_root())
        .output()
        .expect("python3 must be runnable");

    Drift { text: String::from_utf8_lossy(&output.stdout).into_owned() + &String::from_utf8_lossy(&output.stderr) }
}

/// (5) 分母翻倍、命中行不变 ⇒ `DENOM`（新代码没被覆盖），不是退化。
#[test]
fn denominator_growth_is_not_a_regression() {
    // 100 行 60 命中 = 60%；涨到 200 行仍是 60 命中 = 30%。
    let out = classify(&[(PROBE_SF, 200, 60)], &[(PROBE, 60.0, Some(100))]);
    assert_eq!(out.kind_of(PROBE).as_deref(), Some("DENOM"), "分母翻倍必须归因成 DENOM：\n{}", out.text);
    assert!(out.text.contains("100→200"), "输出必须把两侧分母都打出来，否则读者无法复核：\n{}", out.text);
}

/// (6) 行数不变、命中行掉了 20 行 ⇒ `LOST-HITS`（真退化）。
#[test]
fn lost_hits_is_the_real_regression() {
    // 100 行：60 命中 → 40 命中。
    let out = classify(&[(PROBE_SF, 100, 40)], &[(PROBE, 60.0, Some(100))]);
    assert_eq!(out.kind_of(PROBE).as_deref(), Some("LOST-HITS"), "命中行减少必须单独归类：\n{}", out.text);
}

/// (7) 老基线没有分母 ⇒ `DROP` + `?→100`，并且**明说**这不是回归。
#[test]
fn an_attribution_without_a_denominator_says_so() {
    let out = classify(&[(PROBE_SF, 100, 40)], &[(PROBE, 60.0, None)]);
    assert_eq!(out.kind_of(PROBE).as_deref(), Some("DROP"), "无法归因必须落到 DROP：\n{}", out.text);
    assert!(out.text.contains("?→100"), "缺失的一侧必须打成 `?`，不能填 0：\n{}", out.text);
    assert!(
        out.text.contains("DROP 不等于回归"),
        "结论行必须提醒读者 DROP 不是回归 —— 这正是上一轮写错的那句：\n{}",
        out.text
    );
}

/// (8) 差值在 1 行以内不硬判：分母没变大、命中也只是抖了 0.5 行 ⇒ 仍是 `DROP`。
///
/// 这条是"宁可说不知道，也不要编一个成因"。`pct` 只有两位小数，1 行以内的变化
/// 分不清是测量抖动还是真变化。
#[test]
fn a_sub_line_change_is_not_attributed() {
    // 200 行：60.0% → 59.5%，命中 120 → 119。
    let out = classify(&[(PROBE_SF, 200, 119)], &[(PROBE, 60.0, Some(200))]);
    assert_eq!(out.kind_of(PROBE).as_deref(), Some("DROP"), "分母没变、命中只掉 1 行以内 ⇒ 不归因：\n{}", out.text);
}
