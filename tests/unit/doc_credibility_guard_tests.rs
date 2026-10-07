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
//! 本文件判定三件事（都是纯谓词，因此可以用历史错误原文做红证明）：
//!   * [`path_violations`]：文档中的**仓库相对路径声明**必须存在；且不得把 gitignored 的
//!     `docs/superpowers/plans/*` 当持久证据引用（该目录在 `.gitignore:109`，对读者根本不存在）。
//!     声明解析**两种形态**：反引号内容（`` `path:行号` ``）与 markdown 链接目标（`[text](path#anchor)`）；
//!     该谓词同时用于对比报告（[`referenced_paths_all_exist`]）与工作区规则文档
//!     （[`workspace_rules_reference_paths_exist`]，后者仅在本地工作树运行 —— `.trae/` gitignored，CI 缺席）；
//!   * [`count_violations`]：文档声明的 route 条目数与模块数必须等于 `ROUTE_CONTRACT.md` 总览里的数字；
//!   * [`stale_claim_violations`]：**已被后续提交证伪的结论**要么删掉、要么用 `~~…~~` 就地标记为历史
//!     （DOC-04 的更正风格），否则读者会把作废判定当成现状 —— 这正是 §18.5(a) 那批"假缺口"的成因。
//!     该谓词先剥离删除线段落再检查，故"已更正行"天然不命中。
//!
//! **判据（红）**：[`the_path_checker_rejects_a_fabricated_reference`]（反引号伪造引用）、
//! [`the_path_checker_resolves_markdown_links`]（链接形态伪造引用 —— 证明链接确实被解析，而非空转）、
//! [`the_count_checker_rejects_a_stale_claim`] 与 [`the_stale_claim_checker_rejects_an_unretracted_claim`]
//! 分别把历史错误原文喂给谓词，必须判为不合规 —— 否则上面几条断言都可能"因为谓词什么都不返回"而通过。

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

/// 「历史轮次」章节 —— 状态列已被后续提交修掉，但正文按 §18.5 的处理约定**不改写**
/// （保留历史轨迹）。这些章节**必须**在标题后紧跟一条指向 §18 的口径指针，否则读者
/// 会把它们当成现状排期 —— 这正是 §18.5(a) 那批"假缺口"（已修却仍写缺失）的成因。
///
/// 改名/删除章节时本守卫会判"找不到标题"而失败：这是有意的，防止守卫在章节被搬走后
/// 静默失效（铁律 8）。
const HISTORICAL_SECTIONS: [&str; 5] = ["### 11.2 ", "### 11.3 ", "### 12.4 ", "### 12.5 ", "### 15.3 "];

/// 历史章节标题后必须出现的口径指针（指向 §18.5 的假缺口清单）。
const CURRENT_SCOPE_MARKER: &str = "§18.5";

/// 文档中**有意提到的不存在路径**。
///
/// 它们出现在"该文件不存在 / 已删除"这类**否定陈述**里，用于记录历史误引用本身；
/// 若把它们也当违规，§13 的修正记录就会自相矛盾。新增条目必须写清用途 ——
/// 这个清单是"允许文档提到不存在的路径"的**唯一**入口。
const HISTORICAL_NEGATIVE_MENTIONS: [&str; 5] = [
    // §13：记录"已删除的伪造引用"（v1.2 曾把它当证据）。
    "docs/synapse-rust/api-reference.md",
    // §11.2：记录"服务层不存在该文件"，用于纠正旧表述。
    "synapse-services/src/privacy.rs",
    // B8/#20119：记录"该模块已于 2026-09-24 W4/D-27 按铁律 1 整模块删除"，
    // 文档引用它是为了说明"死代码已清 + `search_index` 表成为遗留表"；
    // 该遗留表随后也按 D-39 删除（2026-09-25，`00271cf91`），故此处只保留**模块路径**的
    // 历史引用 —— 表本身已不在 baseline 里，C27 的 D-56 修掉了仍在断言它的契约用例。
    "synapse-storage/src/search_index.rs",
    // v1.8（2026-09-25）：记录"服务端 SAS 路由面已整模块删除"。文档在 §7.2 生效面、
    // §14.4 item 3 与历史更正表里**必须**点名这个文件才能说明"被删的是什么"，
    // 否则那条结论无从落地。删除原因见 `.trae/documents/E2EE验证去服务端私钥重构.md`。
    "synapse-web/src/routes/verification_routes.rs",
    // v1.8（2026-09-25）：同上，服务端 SAS 密码学实现（`derive_sas` / `confirm_sas` /
    // QR show/scan）整模块删除。历史更正表要保留"这些函数当时确实存在且被逐一核对过"，
    // 故按"否定陈述"豁免，而非删掉整行 —— 删了会丢掉"旧结论为何被推翻"的线索。
    "synapse-e2ee/src/verification/service.rs",
];

fn repo_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
}

fn read(relative: &str) -> String {
    let path = repo_root().join(relative);
    fs::read_to_string(&path)
        .unwrap_or_else(|error| std::panic::panic_any(format!("failed to read {}: {error}", path.display())))
}

/// 把一处原始引用（反引号内容或 markdown 链接目标）规范化为「仓库相对路径声明」。
///
/// 先去掉行号后缀（`:112-142` / `:170,210`）与链接锚点（`#L112`），再按白名单过滤；
/// 含 `*` 的是 glob（例如报告里的 `docs/*.md` 计数口径），不含 `/` 的是裸文件名或路由
/// 片段，都不是路径声明。URL（`https://…`）会在按 `:` 切分后落到裸 host，同样被挡掉。
fn repo_path_from_reference(raw: &str) -> Option<String> {
    let raw = raw.trim();
    let path = raw.split(':').next().unwrap_or(raw).trim();
    let path = path.split('#').next().unwrap_or(path).trim();
    if path.contains('*') || !path.contains('/') {
        return None;
    }
    if !TOP_LEVEL.iter().any(|top| path.starts_with(top)) {
        return None;
    }
    if !EXTENSIONS.iter().any(|ext| path.ends_with(ext)) {
        return None;
    }
    Some(path.to_string())
}

/// 文档里所有**仓库相对路径声明**（去重、有序）。
///
/// 解析两种并列的引用形态：反引号内容（`` `path:行号` ``）与 markdown 链接目标
/// （`[text](path#anchor)`）。二者都是"读者会去点开"的承诺，因此都必须校验存在性 ——
/// 此前只解析反引号，链接形态的伪造引用（`[证据](docs/…/不存在.md)`）可完整漏过。
fn referenced_repo_paths(doc: &str) -> Vec<String> {
    let span = Regex::new(r"`([^`\n]+)`").expect("static regex");
    let link = Regex::new(r"\]\(([^)\n]+)\)").expect("static regex");
    let mut paths: Vec<String> = span
        .captures_iter(doc)
        .chain(link.captures_iter(doc))
        .filter_map(|captures| repo_path_from_reference(&captures[1]))
        .collect();
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

/// 每个历史章节标题后的前 6 行内必须出现指向 §18 的口径指针。
///
/// 纯谓词：给它一段"有标题、无指针"的文本就能变红，因此红证明不需要篡改真实文档。
fn stale_section_violations(doc: &str) -> Vec<String> {
    let lines: Vec<&str> = doc.lines().collect();
    let mut violations = Vec::new();

    for heading in HISTORICAL_SECTIONS {
        let Some(index) = lines.iter().position(|line| line.starts_with(heading)) else {
            violations.push(format!(
                "找不到历史章节标题 `{heading}`：章节被改名/删除后本守卫会静默失效，请同步 {HISTORICAL_SECTIONS:?}"
            ));
            continue;
        };
        let end = (index + 7).min(lines.len());
        if !lines[index..end].iter().any(|line| line.contains(CURRENT_SCOPE_MARKER)) {
            violations.push(format!(
                "`{heading}` 缺少指向 `{CURRENT_SCOPE_MARKER}` 的口径指针：历史状态列不得被当作现状引用"
            ));
        }
    }

    violations
}

/// 文档中"删除线保留的历史陈述"被剥离后的正文。
///
/// DOC-04 的更正风格是**就地删除线 + 订正**（`~~旧原文~~ → **订正**：新结论`），
/// 历史原文保留在 `~~…~~` 里以便追溯。本守卫只检查**未被删除线包裹**的文本，
/// 于是"已更正行"天然不再命中，无需按章节作用域排除。
///
/// NB：正则按行匹配（`[^\n]`），因此跨行删除线不会被当作一段 —— 正文本就不该产生跨行删除线；
/// 若出现，说明标记写错，应由人工发现而非守卫静默吞掉。
fn retracted_stripped(doc: &str) -> String {
    Regex::new(r"~~[^\n]*?~~").expect("static regex").replace_all(doc, "").into_owned()
}

/// `synapse-common/src/room_versions.rs` 里 `DEFAULT_ROOM_VERSION` 的实际取值。
///
/// 取源码而非复制常量，正是为了让"默认版本又被改了、文档没跟上"这件事自动变红。
fn default_room_version_in_source(root: &Path) -> String {
    let source = fs::read_to_string(root.join("synapse-common/src/room_versions.rs"))
        .unwrap_or_else(|error| std::panic::panic_any(format!("读取 room_versions.rs 失败: {error}")));
    let declaration = Regex::new(r#"pub const DEFAULT_ROOM_VERSION: &str = "([^"]+)""#).expect("static regex");
    declaration.captures(&source).map_or_else(
        || std::panic::panic_any("room_versions.rs 里找不到 DEFAULT_ROOM_VERSION 声明"),
        |captures| captures[1].to_string(),
    )
}

/// 已被后续提交证伪、却仍可能以"现状"口吻残留的陈述（剥离删除线后判定）。
///
/// 三类都是本仓真实踩过的坑：默认房间版本（11→12）、`validate_id_token_claims`（报告"未接线"，
/// 实为已删除）、Content Scanner（"空转/无消费者"，实为已接线）。谓词必须是纯函数，
/// 才能用历史错误原文做红证明（[`the_stale_claim_checker_rejects_an_unretracted_claim`]）。
fn stale_claim_violations(doc: &str, root: &Path) -> Vec<String> {
    let body = retracted_stripped(doc);
    let source_version = default_room_version_in_source(root);
    let mut found = Vec::new();

    // 1) 默认房间版本：只有同时点名"本仓"/`room_versions.rs` 的陈述才算**本仓**声明，
    //    且常量名后第一个整数必须等于源码值。上游对照表里的 `synapse/config/server.py::DEFAULT_ROOM_VERSION`
    //    是描述 Synapse（Python），不该按本仓阈值判。
    let version_claim = Regex::new(r"DEFAULT_ROOM_VERSION[^0-9]*([0-9]+)").expect("static regex");
    for line in body.lines() {
        if !(line.contains("本仓") || line.contains("room_versions.rs")) {
            continue;
        }
        if let Some(captures) = version_claim.captures(line) {
            if &captures[1] != source_version.as_str() {
                found.push(format!(
                    "默认房间版本陈述过期：文档写 `{}`，而 room_versions.rs 为 `{source_version}` —— {}",
                    &captures[1],
                    line.trim()
                ));
            }
        }
    }

    // 2) 已删除符号：Phase 3 C9 已把 `validate_id_token_claims` 整体删除（全仓 0 命中），
    //    正文再出现它必须与"删除/作废"同现，否则就是作废结论未被标记。
    let retraction_marks = ["删除", "作废", "已过时", "已失效"];
    for line in body.lines() {
        if line.contains("validate_id_token_claims") && !retraction_marks.iter().any(|mark| line.contains(mark)) {
            found.push(format!(
                "`validate_id_token_claims` 已整体删除（Phase 3 C9），此处的残留陈述未标记删除：{}",
                line.trim()
            ));
        }
    }

    // 3) 证伪短语：这两句曾被用来断言"Content Scanner 装配了却不扫描"，现实已接线。
    //    故意**不含**"孤儿模块" —— §12.4/§18.5 的历史更正表要引用该短语本身来说明旧判定，会误报。
    for phrase in ["0 调用点", "无消费者"] {
        for line in body.lines() {
            if line.contains(phrase) {
                found.push(format!(
                    "`{phrase}` 是已被 §15.1 N1 / §18.5(a) 作废的旧判定，未加删除线仍以现状口吻出现：{}",
                    line.trim()
                ));
            }
        }
    }

    found
}

#[test]
fn historical_sections_point_at_the_current_scope() {
    let violations = stale_section_violations(&read(DOC));
    assert!(
        violations.is_empty(),
        "以下历史章节缺少「当前口径见 §18」的指针（§18.5 处理约定 / 报告 V-13）：\n{}",
        violations.join("\n")
    );
}

/// **红证明**：同一批标题、去掉指针后必须被判违规 —— 否则上面的断言可能"因为谓词什么都
/// 不返回"而假通过；顺带证明"章节被改名"这种失效模式也会报错。
#[test]
fn the_stale_section_checker_rejects_a_missing_pointer() {
    let without_pointer = HISTORICAL_SECTIONS
        .iter()
        .map(|heading| format!("{heading}示例标题\n\n| 表头 |\n|---|\n| 已修却仍写缺失 |\n"))
        .collect::<String>();
    let violations = stale_section_violations(&without_pointer);
    assert_eq!(violations.len(), HISTORICAL_SECTIONS.len(), "每个缺指针的历史章节都必须被判违规：{violations:?}");

    let renamed = stale_section_violations("### 11.2 扩展功能（改名后）\n\n> 见 §18.5\n");
    assert_eq!(renamed.len(), HISTORICAL_SECTIONS.len() - 1, "改名后的章节必须报「找不到标题」");
    assert!(renamed[0].contains("找不到历史章节标题"), "{renamed:?}");
}

#[test]
fn referenced_paths_all_exist() {
    let violations = path_violations(&read(DOC), &repo_root());
    assert!(violations.is_empty(), "对比报告引用了不存在或不可持久访问的路径：\n{}", violations.join("\n"));
}

/// 工作区规则文档（`.trae/rules/*.md`）里的仓库路径声明也必须存在。
///
/// `.trae/` 被 `.gitignore:136` 忽略、仓库 0 文件跟踪，CI checkout 根本看不到它，
/// 因此 `docs-quality-gate.yml` 无法覆盖（同目录下的 `find` 只会静默返回空）。
/// 这里在**开发者本地工作树**用同一套 [`path_violations`] 谓词兜底：
/// DOC-06 那类"规则文档指向 `src/services/container.rs` 这类不存在路径"会在此变红。
///
/// `.trae/` 缺席时（CI）打印一行说明后跳过 —— 跳过即该门禁的已知残余风险，
/// 已在 `docs-quality-gate.yml` 的文件发现步骤注释里写明。谓词本身的红证明见
/// [`the_path_checker_rejects_a_fabricated_reference`] 与
/// [`the_path_checker_resolves_markdown_links`]，故此处不重复造假样本。
#[test]
fn workspace_rules_reference_paths_exist() {
    let rules_dir = repo_root().join(".trae/rules");
    let Ok(entries) = fs::read_dir(&rules_dir) else {
        eprintln!(
            "skip claim: .trae/rules 不存在（CI checkout 不含 gitignored 的 .trae/，见 docs-quality-gate.yml 注释）"
        );
        return;
    };
    let mut docs: Vec<PathBuf> = entries
        .filter_map(Result::ok)
        .map(|entry| entry.path())
        .filter(|path| path.extension().is_some_and(|ext| ext == "md"))
        .collect();
    docs.sort();
    assert!(!docs.is_empty(), ".trae/rules/ 存在但没有任何 .md 文件：守卫会静默空转，等于没跑（AGENTS.md 铁律 8）");

    let mut violations = Vec::new();
    for path in docs {
        let content = fs::read_to_string(&path)
            .unwrap_or_else(|error| std::panic::panic_any(format!("读取 {} 失败: {error}", path.display())));
        for violation in path_violations(&content, &repo_root()) {
            violations.push(format!("{}: {violation}", path.display()));
        }
    }
    assert!(
        violations.is_empty(),
        ".trae/rules/ 下的工作区规则文档引用了不存在的路径\
         （本地守卫；该目录 gitignored，无 CI 覆盖）：\n{}",
        violations.join("\n")
    );
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

/// **红证明**：markdown 链接（`[text](path)`）是与反引号并列的第二种"证据引用"形态，
/// 此前完全没被解析 —— 给一段**只含链接**、且目标不存在的文本必须变红，否则
/// [`referenced_paths_all_exist`] 对链接形态始终是空转（假绿）。
#[test]
fn the_path_checker_resolves_markdown_links() {
    // 反例：链接目标不存在 → 必须违规（证明链接确实被提取，而非因"没解析"而放过）。
    let fabricated_link = "详见 [索引契约](docs/synapse-rust/route-contract-v9.md)。";
    let found = path_violations(fabricated_link, &repo_root());
    assert_eq!(found.len(), 1, "markdown 链接目标不存在必须被判定，实得 {found:?}");
    assert!(found[0].contains("route-contract-v9.md"), "违规必须点名该路径：{found:?}");

    // 正例：同一形态、指向真实存在的文件 → 不得违规（证明提取后仍走同一套白名单过滤）。
    let real_link = "详见 [路由契约](docs/synapse-rust/ROUTE_CONTRACT.md)。";
    assert!(
        path_violations(real_link, &repo_root()).is_empty(),
        "真实存在的链接目标不该违规：{:?}",
        path_violations(real_link, &repo_root())
    );

    // 锚点（`#L11`）不是路径的一部分：剥离后仍须按真实文件判定。
    let anchored = "见 [契约](docs/synapse-rust/ROUTE_CONTRACT.md#L11)。";
    assert!(
        path_violations(anchored, &repo_root()).is_empty(),
        "带 `#锚点` 的链接剥离锚点后应通过：{:?}",
        path_violations(anchored, &repo_root())
    );

    // 外链（`https://…`）与纯锚点目录（`#1-项目概览`）不是仓库路径声明，不得被误判。
    let external = "参考 [exposition formats](https://next.prometheus.io/docs/instrumenting/exposition_formats/) 与 [概览](#1-项目概览)。";
    assert!(
        path_violations(external, &repo_root()).is_empty(),
        "外链与纯锚点不该被当作仓库相对路径：{:?}",
        path_violations(external, &repo_root())
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

#[test]
fn stale_claims_are_retracted() {
    let violations = stale_claim_violations(&read(DOC), &repo_root());
    assert!(
        violations.is_empty(),
        "对比报告仍残留已被后续提交证伪、却未用删除线标记为历史的陈述\
         （§18.5(a) 的「假缺口」即此类）：\n{}",
        violations.join("\n")
    );
}

/// **红证明**：把三类**更正前**的历史原文喂给谓词必须被判违规，且同一段文字一旦用
/// `~~…~~` 包裹（DOC-04 的更正风格）就不再违规 —— 后者同时证明了"剥离删除线"确实生效，
/// 否则 [`stale_claims_are_retracted`] 可能"因为谓词把一切都放过"而假通过。
#[test]
fn the_stale_claim_checker_rejects_an_unretracted_claim() {
    let root = repo_root();
    assert_eq!(default_room_version_in_source(&root), "12", "room_versions.rs 的默认版本应是 12");

    // ① 默认房间版本残留（DOC-04 更正前的 §11.1 原文口吻）。
    let stale_version = "§11.1：本仓 `DEFAULT_ROOM_VERSION` 为 11，`room_versions.rs` 注释仍称刻意与上游不同。";
    let found = stale_claim_violations(stale_version, &root);
    assert_eq!(found.len(), 1, "过期的默认房间版本陈述必须被判违规：{found:?}");
    assert!(
        stale_claim_violations(&format!("~~{stale_version}~~ → **订正**：本仓默认已为 12。"), &root).is_empty(),
        "删除线包裹后，同一句话是「已更正的历史原文」，不得再违规"
    );
    // 上游对照表的同名字段是描述 Synapse（Python），不得按本仓阈值误判。
    assert!(
        stale_claim_violations(
            "| 功能 | 默认房间版本 11 → **12** | `synapse/config/server.py::DEFAULT_ROOM_VERSION` |",
            &root
        )
        .is_empty(),
        "描述上游的 `DEFAULT_ROOM_VERSION` 不是本仓声明，不应违规"
    );

    // ② 已删除符号：未标"删除"的 `validate_id_token_claims` 陈述必须变红。
    let stale_symbol = "| OIDC 内置 | `validate_id_token_claims` 从未被调用（死代码） |";
    let found = stale_claim_violations(stale_symbol, &root);
    assert_eq!(found.len(), 1, "未标记删除的 `validate_id_token_claims` 必须被判违规：{found:?}");
    assert!(found[0].contains("validate_id_token_claims"), "{found:?}");
    assert!(
        stale_claim_violations("| OIDC 内置 | 该 `validate_id_token_claims` 已删除 |", &root).is_empty(),
        "与「删除」同现的陈述不应违规"
    );
    assert!(stale_claim_violations(&format!("~~{stale_symbol}~~"), &root).is_empty(), "删除线包裹的符号历史不应违规");

    // ③ 证伪短语：剥离后正文里两句话都必须变红（各计一条）。
    let stale_phrases = "Content Scanner 在生产路径 0 调用点，是已装配但无消费者。";
    let found = stale_claim_violations(stale_phrases, &root);
    assert_eq!(found.len(), 2, "两个证伪短语都必须被判违规：{found:?}");
    assert!(stale_claim_violations(&format!("~~{stale_phrases}~~"), &root).is_empty(), "删除线包裹后不再违规");
}
