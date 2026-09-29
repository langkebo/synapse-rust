//! `docs/synapse-rust/API_COVERAGE_REPORT.md` 的 MSC 编号登记守卫（报告 §7-D3）。
//!
//! **这份守卫为什么存在**：`MSC_SEMANTICS.md` 的维护规则要求"新增或变更任何 MSC 编号用法，
//! 必须同时更新本表"，但规则只有**变成门禁**才不会再次漂移（AGENTS.md 铁律 8 的推论：
//! "看到长期全绿的文档门禁，先怀疑它没在工作"）。报告 §7-D3 的验收判据正是
//! "未登记编号报错" —— 本文件把它落成纯谓词 + 红证明。
//!
//! **为什么不能只做大小写敏感的 `MSC####`**：报告既用大写引用（§5.1 的 `MSC4502`），
//! 也在 §八 配方里写小写号段（`msc3882`）。若只查大写，`msc9999` 这种新引用就能溜过门禁，
//! 所以判定按**大小写不敏感**归一化后再比对（红证明 3 钉死这一点）。
//!
//! **判据（红）**：[`the_msc_registry_checker_rejects_an_unregistered_number`] 把
//! "未登记编号""空登记表""小写提及"三种输入喂给谓词，必须都判为不合规 ——
//! 否则 [`report_msc_numbers_are_all_registered`] 可能"因为谓词什么都不返回"而假通过。

use regex::Regex;
use std::fs;
use std::path::PathBuf;

/// 被守卫的报告（D3 的"文档中出现的 MSC 编号"即指本文档）。
const REPORT: &str = "docs/synapse-rust/API_COVERAGE_REPORT.md";
/// 语义/登记权威（报告 §2 声明其为本仓 MSC 编号的语义权威）。
const SEMANTICS: &str = "docs/synapse-rust/MSC_SEMANTICS.md";

fn repo_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
}

fn read(relative: &str) -> String {
    let path = repo_root().join(relative);
    fs::read_to_string(&path)
        .unwrap_or_else(|error| std::panic::panic_any(format!("failed to read {}: {error}", path.display())))
}

/// 文本中出现的全部 MSC 编号（大小写不敏感、归一化为 `MSC####`、去重、升序）。
fn msc_numbers(text: &str) -> Vec<String> {
    let pattern = Regex::new(r"(?i)msc([0-9]{4})").expect("static regex");
    let mut numbers: Vec<String> = pattern.captures_iter(text).map(|c| format!("MSC{}", &c[1])).collect();
    numbers.sort();
    numbers.dedup();
    numbers
}

/// 报告里出现、但登记表（`MSC_SEMANTICS.md`）未登记的编号。
fn unregistered_msc_numbers(report: &str, semantics: &str) -> Vec<String> {
    let registered = msc_numbers(semantics);
    msc_numbers(report).into_iter().filter(|number| !registered.contains(number)).collect()
}

#[test]
fn report_msc_numbers_are_all_registered() {
    let violations = unregistered_msc_numbers(&read(REPORT), &read(SEMANTICS));
    assert!(
        violations.is_empty(),
        "报告引用了未登记在 {SEMANTICS}（§1 / §1.1）的 MSC 编号（§7-D3）：\n{}",
        violations.join("\n")
    );
}

#[test]
fn the_msc_registry_checker_rejects_an_unregistered_number() {
    // 红证明 1：报告出现未登记编号 ⇒ 必须判违规，且只点名未登记的那个。
    let report = "本仓实现 MSC3814，另借 MSC9999 承载其他语义。";
    let semantics = "| **MSC3814** | 脱水设备 | ... |";
    let found = unregistered_msc_numbers(report, semantics);
    assert_eq!(found, vec!["MSC9999".to_string()], "未登记编号必须被判违规，实得 {found:?}");

    // 红证明 2：登记表为空 ⇒ 报告里每个编号都违规。这条挡住"谓词永远返回空"的假通过。
    let found = unregistered_msc_numbers("MSC3814", "");
    assert_eq!(found, vec!["MSC3814".to_string()], "空登记表必须使全部编号违规，实得 {found:?}");

    // 红证明 3：小写提及同样受管（报告 §八 配方里就是小写 `msc3882`）。
    let found = unregistered_msc_numbers("配方含 msc3882", "");
    assert_eq!(found, vec!["MSC3882".to_string()], "小写提及必须被归一化后判定，实得 {found:?}");
}
