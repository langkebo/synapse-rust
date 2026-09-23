//! `docs/synapse-rust-vs-synapse-comparison.md` 的可信度守卫。
//!
//! **这份守卫为什么存在**：该对比报告 v1.2 出现过两类"看起来权威、实际不成立"的陈述，
//! 且都是靠人工复核才发现的：
//!   1. **伪造引用** —— 把并不存在的 `docs/synapse-rust/api-reference.md` 当作证据挂上去；
//!   2. **过期计数** —— 声称"656 端点 / 48 模块"，而机器抽取的 `ROUTE_CONTRACT.md` 与之不符。
//!
//! 报告本身已经把"计数必须来自可复现命令、每条结论要有 `路径:行号`"写成了口径（§11.1/§12.5 D 类），
//! 但口径只有**变成门禁**才不会再次漂移 —— 这正是 AGENTS.md 铁律 8 的推论：
//! "看到长期全绿的文档门禁，先怀疑它没在工作"。
//!
//! 本文件判定两件事（都是纯谓词，因此可以用历史错误原文做红证明）：
//!   * [`path_violations`]：文档中的**仓库相对路径声明**必须存在；且不得把 gitignored 的
//!     `docs/superpowers/plans/*` 当持久证据引用（该目录在 `.gitignore:109`，对读者根本不存在）；
//!   * [`count_violations`]：文档声明的 route 条目数与模块数必须等于 `ROUTE_CONTRACT.md` 总览里的数字。
//!
//! **判据（红）**：[`the_path_checker_rejects_a_fabricated_reference`] 与
//! [`the_count_checker_rejects_a_stale_claim`] 分别把 v1.2 的错误原文喂给谓词，必须判为不合规 ——
//! 否则上面两条断言都可能"因为谓词什么都不返回"而通过。

use regex::Regex;
use std::fs;
use std::path::{Path, PathBuf};

/// 被守卫的对比报告。
const DOC: &str = "docs/synapse-rust-vs-synapse-comparison.md";
/// route 计数的事实来源（机器抽取，报告自己也声明以它为准）。
const CONTRACT: &str = "docs/synapse-rust/ROUTE_CONTRACT.md";

/// 只有以这些顶层目录开头、且含 `/` 的反引号内容才算"路径声明"。
///
/// 报告同时会提到裸文件名（`service.rs`）与路由片段（`v1/threads`），它们不是路径声明。
const TOP_LEVEL: [&str; 8] = ["docs/", "src/", "scripts/", "docker/", "migrations/", "tests/", "benches/", "synapse-"];

/// 允许的文件后缀，避免把 `migration 1`、`v1/threads/subscribed` 这类片段当路径。
const EXTENSIONS: [&str; 10] = [".md", ".rs", ".toml", ".yaml", ".yml", ".json", ".sh", ".py", ".sql", ".lock"];

/// 文档中**有意提到的不存在路径**。
///
/// 它们出现在"该文件不存在 / 已删除"这类**否定陈述**里，用于记录历史误引用本身；
/// 若把它们也当违规，§13 的修正记录就会自相矛盾。新增条目必须写清用途 ——
/// 这个清单是"允许文档提到不存在的路径"的**唯一**入口。
const HISTORICAL_NEGATIVE_MENTIONS: [&str; 2] = [
    // §13：记录"已删除的伪造引用"（v1.2 曾把它当证据）。
    "docs/synapse-rust/api-reference.md",
    // §11.2：记录"服务层不存在该文件"，用于纠正旧表述。
    "synapse-services/src/privacy.rs",
];

fn repo_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
}

fn read(relative: &str) -> String {
    let path = repo_root().join(relative);
    fs::read_to_string(&path)
        .unwrap_or_else(|error| std::panic::panic_any(format!("failed to read {}: {error}", path.display())))
}

/// 文档里所有**仓库相对路径声明**（去重、有序）。
///
/// 反引号内容先去掉行号后缀（`:112-142` / `:170,210`），再按白名单过滤；
/// 含 `*` 的是 glob（例如报告里的 `docs/*.md` 计数口径），不是路径声明。
fn referenced_repo_paths(doc: &str) -> Vec<String> {
    let span = Regex::new(r"`([^`\n]+)`").expect("static regex");
    let mut paths = Vec::new();
    for captures in span.captures_iter(doc) {
        let raw = captures[1].trim();
        let path = raw.split(':').next().unwrap_or(raw).trim();
        if path.contains('*') || !path.contains('/') {
            continue;
        }
        if !TOP_LEVEL.iter().any(|top| path.starts_with(top)) {
            continue;
        }
        if !EXTENSIONS.iter().any(|ext| path.ends_with(ext)) {
            continue;
        }
        paths.push(path.to_string());
    }
    paths.sort();
    paths.dedup();
    paths
}

/// 路径类问题（不存在的引用 + gitignored 引用）。
fn path_violations(doc: &str, root: &Path) -> Vec<String> {
    let mut found = Vec::new();
    for path in referenced_repo_paths(doc) {
        if HISTORICAL_NEGATIVE_MENTIONS.contains(&path.as_str()) {
            continue;
        }
        if path.starts_with("docs/superpowers/") {
            found.push(format!(
                "{path}: 位于 gitignored 的 docs/superpowers/plans/（.gitignore:109），对读者不存在，\
                 不能作为持久证据 —— 请改引 docs/audit/ 下的报告"
            ));
            continue;
        }
        if !root.join(&path).exists() {
            found.push(format!("{path}: 文档引用了不存在的路径"));
        }
    }
    found
}

/// `ROUTE_CONTRACT.md` 总览里的 `(route 条目数, 含路由模块数)`。
fn contract_totals(contract: &str) -> (u64, u64) {
    (total_on_line(contract, "注册路由条目"), total_on_line(contract, "含路由注册的模块文件"))
}

/// 取含 `label` 的那一行里 `**<数字>**` 的值。
fn total_on_line(text: &str, label: &str) -> u64 {
    let line = text
        .lines()
        .find(|line| line.contains(label))
        .unwrap_or_else(|| std::panic::panic_any(format!("{label}: 在契约文件里找不到该标签")));
    let bold = Regex::new(r"\*\*([0-9][0-9,]*)\*\*").expect("static regex");
    let raw = bold.captures(line).map_or_else(
        || std::panic::panic_any(format!("{label}: 该行没有 **数字**：{line}")),
        |captures| captures[1].to_string(),
    );
    raw.replace(',', "")
        .parse()
        .unwrap_or_else(|error| std::panic::panic_any(format!("{label}: 解析 {raw} 失败: {error}")))
}

/// 文档中形如 `<n> 条注册路由` / `<n> 个模块` 的全部声明。
fn claimed_totals(doc: &str) -> Vec<(&'static str, u64)> {
    let claim = Regex::new(r"([0-9][0-9,]*)\s*(条注册路由|个模块)").expect("static regex");
    claim
        .captures_iter(doc)
        .map(|captures| {
            let label: &'static str = if &captures[2] == "条注册路由" { "条注册路由" } else { "个模块" };
            let value = captures[1]
                .replace(',', "")
                .parse::<u64>()
                .unwrap_or_else(|error| std::panic::panic_any(format!("解析计数 {} 失败: {error}", &captures[1])));
            (label, value)
        })
        .collect()
}

/// 计数类问题：文档声明与契约不符，或文档根本没给出声明（后者同样是可信度问题 ——
/// 报告的口径要求计数来自可复现来源）。
fn count_violations(doc: &str, contract: &str) -> Vec<String> {
    let (routes, modules) = contract_totals(contract);
    let claims = claimed_totals(doc);
    let mut found = Vec::new();
    if !claims.iter().any(|(label, _)| *label == "条注册路由") {
        found.push(format!("文档没有声明 route 条目数（ROUTE_CONTRACT.md 为 {routes}）"));
    }
    if !claims.iter().any(|(label, _)| *label == "个模块") {
        found.push(format!("文档没有声明模块数（ROUTE_CONTRACT.md 为 {modules}）"));
    }
    for (label, value) in claims {
        let expected = if label == "条注册路由" { routes } else { modules };
        if value != expected {
            found.push(format!("文档声明 {value} {label}，而 {CONTRACT} 为 {expected}"));
        }
    }
    found
}

#[test]
fn referenced_paths_all_exist() {
    let violations = path_violations(&read(DOC), &repo_root());
    assert!(violations.is_empty(), "对比报告引用了不存在或不可持久访问的路径：\n{}", violations.join("\n"));
}

#[test]
fn counts_match_the_route_contract() {
    let violations = count_violations(&read(DOC), &read(CONTRACT));
    assert!(violations.is_empty(), "对比报告的计数与 {CONTRACT} 不一致：\n{}", violations.join("\n"));
}

#[test]
fn the_path_checker_rejects_a_fabricated_reference() {
    // v1.2 的原文形态：把不存在的文件当证据。
    //
    // NB: 这里必须用一个**不在** `HISTORICAL_NEGATIVE_MENTIONS` 里的路径 ——
    // 第一版用了历史上的 `docs/synapse-rust/api-reference.md`，结果被白名单吃掉、
    // 红证明假通过（`assert_eq!(found.len(), 1)` 报 0）。白名单只服务于"文档有意记录
    // 某文件不存在"这一种语境，不能用来豁免新出现的伪造引用。
    let fabricated = "API 参考：`docs/synapse-rust/api-reference-v2.md`（656 端点）。";
    let found = path_violations(fabricated, &repo_root());
    assert_eq!(found.len(), 1, "伪造引用必须被判定，实得 {found:?}");
    assert!(found[0].contains("api-reference-v2.md"), "违规信息必须点名该路径：{found:?}");

    // v1.3 起把 gitignored 的计划文件当"计划见 …"的落点，同样是读者拿不到的证据。
    let gitignored = "计划见 `docs/superpowers/plans/2026-09-22-protocol-correctness-phase1.md`（gitignored）。";
    let found = path_violations(gitignored, &repo_root());
    assert_eq!(found.len(), 1, "gitignored 引用必须被判定，实得 {found:?}");
    assert!(found[0].contains("gitignored"), "违规信息必须解释原因：{found:?}");

    // 历史否定陈述（§13/§11.2 用来记录"该文件不存在"）不得被误判。
    let negative =
        "**不存在** `synapse-services/src/privacy.rs`；此前引用的 `docs/synapse-rust/api-reference.md` 已删除。";
    assert!(
        path_violations(negative, &repo_root()).is_empty(),
        "否定陈述里的历史路径不应被判违规：{:?}",
        path_violations(negative, &repo_root())
    );
}

#[test]
fn the_count_checker_rejects_a_stale_claim() {
    let contract = read(CONTRACT);
    let (routes, modules) = contract_totals(&contract);

    // 计数提取对格式的要求：单键前后空白都可变，逗号千分位要能解析。
    assert!(
        claimed_totals(&format!("路由 **{routes} 条注册路由**、**{modules} 个模块**")).len() == 2,
        "必须能解析出两条声明"
    );

    // v1.2 的错误原文（656 / 48）必须被判为过期。
    let stale = "路由面 **656 条注册路由**、**48 个模块**";
    let found = count_violations(stale, &contract);
    assert_eq!(found.len(), 2, "两条过期计数都必须被判定，实得 {found:?}");
    assert!(found.iter().any(|v| v.contains("656")), "必须点名 656：{found:?}");
    assert!(found.iter().any(|v| v.contains("48")), "必须点名 48：{found:?}");

    // 缺声明同样是违规（报告口径要求计数来自可复现来源）。
    let silent = "路由面完整，未给数字";
    assert_eq!(count_violations(silent, &contract).len(), 2, "缺声明必须被判违规");
}
