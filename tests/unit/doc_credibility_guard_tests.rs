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

/// 从 `marker` 所在行起到下一个**顶格** `}` 为止的源码片段（含起止行）。
///
/// 守卫要判断的是"某个函数**自己**做了什么"：整文件 `contains` 会被同文件里别的函数
/// 污染（r1 挂了 `auth_middleware`，整文件匹配会让 r0/v1 的断言假通过）。
fn fn_body(source: &str, marker: &str) -> String {
    let lines: Vec<&str> = source.lines().collect();
    let start = lines
        .iter()
        .position(|line| line.contains(marker))
        .unwrap_or_else(|| std::panic::panic_any(format!("源码里找不到 `{marker}` —— 守卫前提失效")));
    let end = lines[start..]
        .iter()
        .position(|line| line.trim_end() == "}")
        .map_or_else(|| std::panic::panic_any(format!("`{marker}` 的函数体没有顶格 `}}`")), |offset| start + offset);
    lines[start..=end].join("\n")
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

// ═════════════════════════════════════════════════════════════════════════════
// 2026-10-13（审计 §2 B2）：把「被引用的文档必须存在」从 1 份扩到全仓源码引用
// ═════════════════════════════════════════════════════════════════════════════

/// 被**测试显式断言不存在**的路径。
///
/// 它们出现在"死引用"的负向样本里：`sqlx_ratio_gate_tests.rs` 断言某条错误信息曾指向一个
/// 不存在的文件；本文件的 4 条伪造引用用于证明路径检查器真的在跑（而非空转）。把它们判为
/// 违规，等于要求守卫删掉自己的红证明。
///
/// ⚠️ 第五条 `docs/superpowers/plans/…` 是 **只在干净 checkout 上才"不存在"** 的那种：
/// 该目录被 `.gitignore:113` 忽略，开发机上打过计划文件所以本地存在、CI 上永远不存在。
/// 不登记它，`code_referenced_docs_exist` 就只在 CI 上红（2026-09-23 `24f123805` 起，
/// `Test & Lint (--all-features)` 的 `Run unit test target` 一直挂在这条上）。
/// 判据：`ls docs/superpowers/plans/2026-09-22-protocol-correctness-phase1.md` 本地有、
/// `git ls-files docs/superpowers/plans/` 为空。
const INTENTIONALLY_ABSENT_DOCS: [&str; 6] = [
    "docs/synapse-rust/M3_SQLX_MIGRATION_PLAN.md",
    "docs/synapse-rust/api-reference.md",
    "docs/synapse-rust/api-reference-v2.md",
    "docs/synapse-rust/route-contract-v9.md",
    "docs/…/不存在.md",
    "docs/superpowers/plans/2026-09-22-protocol-correctness-phase1.md",
];

/// 递归收集目录下的 `.rs` 文件（跳过 `target/`、`.git/`、`node_modules/`）。
fn collect_rust_files(dir: &Path, out: &mut Vec<PathBuf>) {
    let Ok(entries) = fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        let name = entry.file_name().to_string_lossy().to_string();
        if path.is_dir() {
            if name == "target" || name == ".git" || name == "node_modules" {
                continue;
            }
            collect_rust_files(&path, out);
        } else if name.ends_with(".rs") {
            out.push(path);
        }
    }
}

/// 源码里引用的**本仓**文档路径必须存在（2026-10-13 审计 §2 B2）。
///
/// ## 为什么需要
///
/// 本文件此前只守卫 [`DOC`] **一份**文档，而代码里到处把 `docs/**` 当**证据**引用 ——
/// `synapse-common/src/room_versions.rs` 引状态表声称"24/24 全 ✅"、
/// `synapse-services/src/room/state_record.rs` 引同一文件的 §4.6、多个测试引审计轮次报告。
/// 引用一旦悬空，读者会把"文档不存在"误读成"结论不成立"，或者更糟 —— 把已删除的表当成现状。
///
/// 首次启用时（本轮）扫出 **16 处指向已归档文档**（引用没跟着归档动作更新 —— 文档已移入
/// 同目录树下的 `archive/` 子目录，引用仍指向其原位置）+ **6 处指向已删除文档**，已全部修正。
///
/// ## 边界（有意，与 [`referenced_repo_paths`] 同口径）
///
/// 本 doc 注释自己也被这条判据扫，
/// 所以**不能**在这里举「本仓 docs/ 下的示例路径」：占位符同样会被判违规（首次运行就抓到
/// 本注释里的两个占位符，这正是判据在工作而非空转的实证）。
///
/// 只认**反引号包裹**与 **markdown 链接**两种形态、且从 `docs/` 起的路径。裸提及
/// （`// docs/x/y.md §2` 不带反引号）与跨仓路径（`matrix-js-sdk/docs/…`）不在范围内 ——
/// 后者由 SDK 仓自己的守卫负责。
///
/// ## 判据（红）
///
/// 把上面任一被引用的文档改名 ⇒ 本测试失败并列出「引用路径 ← 引用者」。
/// [`the_doc_reference_scanner_has_teeth`] 另作探测下限，防止"扫描器什么都没扫到"空转通过。
#[test]
fn code_referenced_docs_exist() {
    let root = repo_root();
    let mut files = Vec::new();
    for top in ["src", "tests", "benches"] {
        collect_rust_files(&root.join(top), &mut files);
    }
    let mut crate_dirs: Vec<PathBuf> = fs::read_dir(&root)
        .expect("repo root readable")
        .flatten()
        .map(|entry| entry.path())
        .filter(|p| p.is_dir() && p.file_name().is_some_and(|n| n.to_string_lossy().starts_with("synapse-")))
        .collect();
    crate_dirs.sort();
    for dir in &crate_dirs {
        collect_rust_files(&dir.join("src"), &mut files);
    }

    let mut violations: Vec<String> = Vec::new();
    let mut checked = 0usize;
    for file in &files {
        let Ok(text) = fs::read_to_string(file) else {
            continue;
        };
        let rel_file = file
            .strip_prefix(&root)
            .map_or_else(|_| file.display().to_string(), |p| p.to_string_lossy().replace('\\', "/"));
        for reference in referenced_repo_paths(&text) {
            if !reference.starts_with("docs/") || !reference.ends_with(".md") {
                continue;
            }
            checked += 1;
            if INTENTIONALLY_ABSENT_DOCS.contains(&reference.as_str()) {
                continue;
            }
            if !root.join(&reference).exists() {
                violations.push(format!("{reference}  <-  {rel_file}"));
            }
        }
    }

    assert!(
        checked >= 40,
        "扫描器只找到 {checked} 处 docs/ 引用，低于下限 40 —— 数量骤降说明扫描器坏了\
         （路径/后缀过滤写错），不是「仓库变干净了」"
    );
    assert!(
        violations.is_empty(),
        "源码引用了不存在的文档（{} 处）：\n  {}\n\
         修法：① 文档被归档 ⇒ 路径补成 `docs/<区>/archive/<名>`；\
         ② 文档已删除 ⇒ 去掉 `docs/...` 路径形态（保留历史指代并注明已删除），或改指现行文档；\
         ③ 确属「故意引用不存在路径」的负向样本 ⇒ 加进 INTENTIONALLY_ABSENT_DOCS 并写明理由。",
        violations.len(),
        violations.join("\n  ")
    );
}

// ── 能力台账（API_COVERAGE_REPORT）的"未实现"标注必须与代码一致 ──────────────

/// `docs/synapse-rust/API_COVERAGE_REPORT.md` 的能力标注是**排期依据**：
/// 团队照它判断"这个能力还没做"。标反了会直接导致重复开发，或反过来把已实现的能力
/// 当成缺口写进对外兼容性声明。
///
/// 审计 FUNC-04 记的正是这一类，且 2026-10-24 复核发现两处**都还是错的**：
///
/// | 行 | 台账原判 | 代码事实 |
/// |---|---|---|
/// | 缩略图 `animated` | `**MISSING**`，证据"`animated` 在 routes/ 与 services/ 均 0 命中" | 两处都有命中（`routes/media/download.rs:27,175,190`、`media_service.rs:530-552`），缓存键还带 `_animated` 后缀 |
/// | `M_APPSERVICE_LOGIN_UNSUPPORTED` | `PARTIAL`，证据"该码全仓 0 命中" | 码表 + 构造器 + 使用点齐全（`error/code.rs:140`、`error.rs:389`、`auth_compat.rs:210`） |
///
/// 两处的共性不是"数字过期"，而是**证据是编的**：把"我没搜到"写成"不存在"。
/// 所以判据不能只钉住这两行文案，而要钉住"声称不存在 ⇒ 代码里真的不存在"这个逻辑：
/// 代码里一旦出现该标识符，台账那一行就不许再说 `MISSING` / `0 命中`。
#[test]
fn capability_ledger_absence_claims_match_the_code() {
    const LEDGER: &str = "docs/synapse-rust/API_COVERAGE_REPORT.md";

    // (标识符, 在哪找, 台账里用来表达"不存在"的词)
    let claims = [
        ("animated", "synapse-web/src/routes/media/download.rs"),
        ("M_APPSERVICE_LOGIN_UNSUPPORTED", "synapse-common/src/error/code.rs"),
    ];

    let ledger = read(LEDGER);
    let mut problems = Vec::new();

    // 两条"这已经是历史"的写法必须先剥掉再判，否则改对之后反而恒红：
    //   * `~~…~~` —— DOC-04 约定的已更正标记；
    //   * `**原判「…」不成立**` —— 本文件的订正风格（引用旧结论并宣告它错了）。
    // 这不是"为了让测试变绿而放宽"：要判的是**当前主张**，而引用旧结论正是"它不再主张"的证据。
    let retracted = Regex::new(r"原判[^。|]*?不成立").expect("retraction regex");

    for (needle, source) in claims {
        let present_in_code = read(source).contains(needle);
        if !present_in_code {
            continue; // 代码里真的没有 ⇒ 台账怎么写都可能对，本条不管
        }
        // 只检查"提到该标识符"的那几行，避免误伤别处的 MISSING。
        for line in ledger.lines().filter(|line| line.contains(needle)) {
            let without_strike = line.split("~~").step_by(2).collect::<Vec<_>>().join("");
            let live = retracted.replace_all(&without_strike, "");
            for wrong in ["**MISSING**", "0 命中"] {
                if live.contains(wrong) {
                    problems.push(format!(
                        "`{needle}` 在 `{source}` 里存在，但台账这一行仍写 `{wrong}`：\n    {}",
                        line.trim()
                    ));
                }
            }
        }
    }

    assert!(
        problems.is_empty(),
        "能力台账与代码相反（{} 处）：\n  {}\n\
         修法：把该行改成 `TRUE（已实现）` 并给出**能复现**的证据（`路径:行号`），\n\
         或在证据里写明是哪个判据失效 —— 不要只把 `MISSING` 换成 `PARTIAL` 蒙过去。",
        problems.len(),
        problems.join("\n  ")
    );
}

/// 判据自检：把上面两条历史错误原文喂进同一套逻辑，必须判为不合规。
///
/// 没有这一条，`capability_ledger_absence_claims_match_the_code` 可能在
/// "解析什么都没匹配到"的情况下通过 —— 那正是它要防的失败形态（门禁空转）。
#[test]
fn the_capability_ledger_checker_rejects_the_two_historical_wrong_rows() {
    let source = "animated";
    let code_has_it = read("synapse-web/src/routes/media/download.rs").contains(source);
    assert!(code_has_it, "前提失效：`animated` 已不在 download.rs，本自检需要随实现更新");

    let historical = "| 缩略图动画支持（`animated` 查询参数） | 1.158 | **MISSING** | `animated` 均 **0 命中** |";
    let retracted = Regex::new(r"原判[^。|]*?不成立").expect("retraction regex");
    let stripped = historical.split("~~").step_by(2).collect::<Vec<_>>().join("");
    let live = retracted.replace_all(&stripped, "");
    assert!(
        live.contains("**MISSING**") || live.contains("0 命中"),
        "自检失效：这套「剥离删除线 / 原判订正段后找 MISSING」的判定没能识别历史错误原文"
    );
}

// ── 项目规则里的 MSC 台账 ─────────────────────────────────────────────────

/// 项目规则把 **MSC3245 标成 "Room summary"**（实为**语音消息**），且标"完整实现"。
///
/// 这条错了两份：MSC3245 是 voice messages（语音的编码/转写在客户端完成，服务端三端点
/// `convert`/`optimize`/`transcription` 按 by-design 恒返回 `M_UNRECOGNIZED`）；"Room summary"
/// 属于 MSC3266 / MSC4380 一族。照台账排期的人会去找一个不存在的服务端能力，或反过来
/// 把"服务端没实现"当成缺陷去补（审计 DOC-03，`FUNC-02` 的初判就是这么错的）。
///
/// 判据用**成对断言**：这一行必须写"语音消息"，且那一行不得再出现 "Room summary" 与
/// "完整实现" 的并列 —— 只钉住错词不够，因为把错词换成另一个错词（比如 "Voice broadcast"）
/// 同样会误导。
#[test]
fn project_rules_labels_msc3245_as_voice_messages() {
    const RULES: &str = ".trae/rules/project_rules.md";
    let path = repo_root().join(RULES);
    if !path.exists() {
        // `.trae/` 是 IDE 本地目录。该文件已于 2026-10-24 用 `git add -f` 纳入版本控制
        // （审计 NEW-02），所以 CI 里也存在；本地缺席时跳过而不是失败。
        eprintln!("SKIP：{RULES} 不存在（该文件应已入库，见 NEW-02）");
        return;
    }
    let text = fs::read_to_string(&path).expect("read project_rules.md");

    let row = text
        .lines()
        .find(|line| line.contains("MSC3245"))
        .unwrap_or_else(|| panic!("{RULES} 里找不到 MSC3245 那一行，守卫前提失效"));

    // 只看**名称列与状态列**。证据列允许引用旧错标（"此前写作 XX，两处都错"），
    // 整行匹配会把这种"说明它为什么错"的订正本身判红 —— 首版就是这么误伤的。
    let cells: Vec<&str> = row.split('|').map(str::trim).collect();
    assert!(cells.len() >= 5, "MSC3245 那一行不是 4 列表格行，守卫前提失效：\n  {row}");
    let (label, status) = (cells[2], cells[3]);

    assert!(
        label.contains("语音消息"),
        "MSC3245 是**语音消息**（voice messages），台账的名称列必须写明。实际名称列：`{label}`"
    );
    assert!(
        !label.contains("Room summary"),
        "MSC3245 不是 \"Room summary\"（那是 MSC3266/MSC4380 一族）。实际名称列：`{label}`"
    );
    assert!(
        !status.contains("完整实现"),
        "服务端三端点（convert/optimize/transcription）按 MSC3245 属 by-design 未实现，\
         不能标「完整实现」。实际状态列：`{status}`"
    );
}

// ── SEC-14：`ts` 到底发不发，注释与代码必须同时改 ──────────────────────────

/// `TimestampCheck` 的文档用「**本仓自己的出站请求就不带 `ts`**」当作"缺 `ts` 必须放行"
/// 的核心论据。这条断言的就是**那份论据与代码仍然一致**。
///
/// 为什么值得一条用例：审计 SEC-14 曾建议"缺 `ts` ⇒ 401"，而照做会让服务器拒掉自己
/// 发出的每一个联邦请求（RFC 9110 的 `X-Matrix` 参数集里根本没有 `ts`，规范补一句
/// "Unknown parameters are ignored"）。这个裁决一旦被忘记，下一个人会照着"加固"把它改回去。
///
/// 判据刻意做成**双向一致**，而不是"禁止发 `ts`"：将来真的加上 `ts`（对端若强制新鲜度
/// 会更友好）是允许的，但那时必须同步改掉那段文档 —— 两边一起变，本用例仍然绿；
/// 只改一边就会红，逼人重新读一遍裁决。
#[test]
fn signature_timestamp_claim_matches_the_outbound_header() {
    const AUTH: &str = "synapse-web/src/middleware/federation_auth.rs";
    const CLIENT: &str = "synapse-federation/src/client.rs";

    let auth = read(AUTH);
    let claims_we_send_no_ts = auth.contains("自己的出站请求就不带 `ts`");
    assert!(
        claims_we_send_no_ts,
        "`{AUTH}` 里找不到「自己的出站请求就不带 `ts`」这句论据了。\n\
         它支撑着 SEC-14 的裁决（缺 ts 必须放行）；换成别的措辞可以，但请让本用例同步更新，\n\
         否则\"为什么不能拒绝缺 ts\"这个结论就只剩口头传承。"
    );

    // 只看拼 `X-Matrix` 头的那一行：`valid_until_ts=` 之类的子串不算。
    let client = read(CLIENT);
    let header_lines: Vec<&str> = client.lines().filter(|line| line.contains("X-Matrix origin")).collect();
    assert!(!header_lines.is_empty(), "`{CLIENT}` 里找不到拼 X-Matrix 头的那行");
    let sends_ts = header_lines.iter().any(|line| line.contains("ts="));

    assert_eq!(
        claims_we_send_no_ts,
        !sends_ts,
        "文档与代码对不上：`{AUTH}` 说「出站不带 ts」，而 `{CLIENT}` 的 X-Matrix 头{}。\n\
         要么把 `ts` 从出站头里去掉，要么同步改掉 `TimestampCheck` 的文档（并重看 SEC-14 的裁决）。\n\
         出站头：{:?}",
        if sends_ts { "现在带了 ts" } else { "仍然不带 ts" },
        header_lines
    );
}

// ── SEC-12：legacy 媒体匿名读是裁决，不是漏洞 —— 文档与代码必须同时改 ────────

/// `/_matrix/media/r0|v1/download` 匿名可读是 **Matrix legacy 语义**（媒体仓库在 v1.11
/// 之前允许匿名读），收紧会让未升级的客户端所有历史图片加载失败。SEC-12 的裁决因此是
/// "登记为设计决策 + 在部署文档里说明"，**不是**给它们补鉴权。
///
/// 判据三方绑定，任一侧单独变化都会红：
///   * `docker/deploy/README.md` 的登记小节必须存在，并点名 r0/v1 与 `auth_middleware`；
///   * `create_media_r1_router` 必须挂 `auth_middleware`（r1 已收紧，VULN-01/02）；
///   * `create_media_r0_router` / `create_media_v1_router` **不得**挂它，且必须合并
///     legacy 下载 router；handler 仍取 `OptionalAuthenticatedUser`（token 可选）。
#[test]
fn legacy_media_anonymous_read_is_registered_and_matches_the_router() {
    const ROUTES: &str = "synapse-web/src/routes/media/mod.rs";
    const DOWNLOAD: &str = "synapse-web/src/routes/media/download.rs";
    const DEPLOY_DOC: &str = "docker/deploy/README.md";

    // ① 部署文档必须登记这件事：删掉小节等于把 SEC-12 重新打开。
    let doc = read(DEPLOY_DOC);
    let heading = doc
        .lines()
        .position(|line| line.contains("媒体下载的匿名读语义"))
        .unwrap_or_else(|| panic!("`{DEPLOY_DOC}` 里找不到「媒体下载的匿名读语义」小节 —— SEC-12 的登记被删了"));
    let section = doc.lines().skip(heading).take(40).collect::<Vec<_>>().join("\n");
    for needle in ["/_matrix/media/r0/download", "/_matrix/media/v1/download", "auth_middleware"] {
        assert!(section.contains(needle), "登记小节里缺少 `{needle}`：读者无法据此判断哪条路径匿名、哪条要 token");
    }

    // ② r1 必须收紧（否则它的匿名读是真漏洞，而不是"已裁决的 legacy"）。
    let routes = read(ROUTES);
    assert!(
        fn_body(&routes, "fn create_media_r1_router").contains("auth_middleware"),
        "`create_media_r1_router` 不再挂 `auth_middleware` —— 文档说 r1 要求 token，\
         代码说不用，二者必须同步"
    );

    // ③ r0/v1 必须保持匿名（挂上鉴权就等于悄悄关掉 legacy 语义，客户端会全量破图）。
    for name in ["fn create_media_v1_router", "fn create_media_r0_router"] {
        let body = fn_body(&routes, name);
        assert!(
            !body.contains("auth_middleware"),
            "`{name}` 挂上了 `auth_middleware`：这等于关闭 legacy 匿名读 —— \
             请先按客户端版本分布裁决，并同步 `{DEPLOY_DOC}` 与 SEC-12"
        );
        assert!(
            body.contains("create_media_legacy_download_router()"),
            "`{name}` 不再合并 legacy 下载 router：文档描述的路径与代码不一致"
        );
    }

    // ④ handler 级：legacy 下载仍取"token 可选"。
    assert!(
        fn_body(&read(DOWNLOAD), "async fn download_media_v1(").contains("OptionalAuthenticatedUser"),
        "`download_media_v1` 改成了强制鉴权 —— legacy r0/v1 的匿名读语义变了，\
         必须同步 `{DEPLOY_DOC}` 与 SEC-12 裁决"
    );
}

// ── MAINT-01：测试基建必须特性隔离，但测试构建仍须看得见它 ────────────────────

/// 取 manifest 里 `[header]` 段的正文（到下一个表头为止）。
///
/// 不用 `toml` 反序列化：判据是"**哪一段**里写了什么"（同名键在 `[dependencies]`
/// 与 `[dev-dependencies]` 下都合法），反序列化恰好会把这层差异吃掉。
fn toml_section(manifest: &str, header: &str) -> String {
    let marker = format!("[{header}]");
    let lines: Vec<&str> = manifest.lines().collect();
    let start = lines
        .iter()
        .position(|line| line.trim() == marker)
        .unwrap_or_else(|| panic!("manifest 里找不到 `{marker}` 段 —— 守卫前提失效"));
    let end = lines[start + 1..]
        .iter()
        .position(|line| line.trim_start().starts_with('['))
        .map_or(lines.len(), |offset| start + 1 + offset);
    lines[start + 1..end].join("\n")
}

/// 取段内某条依赖的原文（`name = { … }`）。
fn manifest_entry(section: &str, name: &str) -> String {
    section
        .lines()
        .find(|line| line.split('=').next().map(str::trim) == Some(name))
        .unwrap_or_else(|| panic!("段里找不到 `{name}` 条目 —— 守卫前提失效"))
        .to_string()
}

/// MAINT-01 的判据是**两半**，任一半单独回退都会红：
///   * `synapse-common` 侧：四个测试基建模块必须 gate 在 `test-utils` 上 —— 无条件编译会让
///     `test_schema_orphans`（带一个回收 `test_*` schema 的 janitor）随每个生产二进制进图；
///   * 消费方侧：测试构建必须显式打开特性 —— 依赖方的 `cfg(test)` 对被依赖 crate **不可见**，
///     只 gate 在 `any(test, …)` 上会让 `cargo check -p <crate> --tests` 解析不到这些模块。
///
/// 但特性只能写在**测试**那一侧：写进消费方的 `[dependencies]` 等于把隔离又撤销了。
#[test]
fn test_infrastructure_is_feature_gated_and_never_ships_in_production() {
    const COMMON: &str = "synapse-common/src/lib.rs";
    const GATE: &str = "#[cfg(any(test, feature = \"test-utils\"))]";
    const MODULES: [&str; 4] = ["test_database_guard", "test_isolation", "test_schema_guard", "test_schema_orphans"];

    // ① `synapse-common` 侧：四个模块都必须紧邻上一行就是 `test-utils` gate。
    let common = read(COMMON);
    let lines: Vec<&str> = common.lines().collect();
    for module in MODULES {
        let declaration = format!("pub mod {module};");
        let index = lines
            .iter()
            .position(|line| line.trim() == declaration)
            .unwrap_or_else(|| panic!("`{COMMON}` 里找不到 `{declaration}` —— MAINT-01 的守卫前提失效"));
        assert_eq!(
            lines.get(index.wrapping_sub(1)).map(|line| line.trim()),
            Some(GATE),
            "`{module}` 的上一行不是 `{GATE}`：无条件编译会让测试基建随生产二进制进依赖图"
        );
    }

    // ② 消费方侧：`[dev-dependencies]` 必须打开特性，`[dependencies]` 必须没打开。
    const DEV_CONSUMERS: [&str; 6] = [
        "Cargo.toml",
        "synapse-storage/Cargo.toml",
        "synapse-e2ee/Cargo.toml",
        "synapse-federation/Cargo.toml",
        "synapse-services/Cargo.toml",
        "synapse-web/Cargo.toml",
    ];
    for manifest in DEV_CONSUMERS {
        let source = read(manifest);
        assert!(
            manifest_entry(&toml_section(&source, "dev-dependencies"), "synapse-common").contains("test-utils"),
            "`{manifest}` 的 `[dev-dependencies]` 没打开 `synapse-common/test-utils`：该 crate 的 \
             `#[cfg(test)]` 夹具会解析不到 `synapse_common::test_isolation`（依赖方的 `cfg(test)` 不可见）"
        );
        assert!(
            !manifest_entry(&toml_section(&source, "dependencies"), "synapse-common").contains("test-utils"),
            "`{manifest}` 的 `[dependencies]` 打开了 `synapse-common/test-utils`：\
             测试基建会随生产依赖图一起编译（MAINT-01 回退）"
        );
    }

    // ③ `synapse-test-utils` 本身就是测试基建：库代码直接 `use` 这些模块，只能写在 `[dependencies]`。
    assert!(
        manifest_entry(&toml_section(&read("synapse-test-utils/Cargo.toml"), "dependencies"), "synapse-common")
            .contains("test-utils"),
        "`synapse-test-utils` 的库代码（`PARKED_SCHEMA_TABLE`、`template_ready_marker_path`…）直接引用 \
         `synapse-common` 的测试基建模块，`[dependencies]` 必须打开该特性"
    );

    // ④ 根 crate 只能经 `test-utils` feature 把 `synapse-test-utils` 拉进来，不得成硬依赖。
    let root = read("Cargo.toml");
    assert!(
        manifest_entry(&toml_section(&root, "dependencies"), "synapse-test-utils").contains("optional = true"),
        "根 crate 把 `synapse-test-utils` 写成了非 optional 依赖：它会进入默认（生产）构建"
    );
    assert!(
        manifest_entry(&toml_section(&root, "features"), "test-utils").contains("dep:synapse-test-utils"),
        "根 crate 的 `test-utils` feature 不再拉起 `dep:synapse-test-utils`：\
         ④ 的 optional 声明会变成永远不会被打开，测试构建反而缺 crate"
    );
}

// ── CODE-11：审计文档里的 allow 数字必须等于棘轮基线现算值 ─────────────────────

/// `#[allow]` 存量在两处各写一遍（正文标题与附录 D 的 CODE-11 行；该行原在 §0.2.3，随本次关闭移入附录），
/// 而两处都是"手抄的现算值"。基线文件 `scripts/ci/clippy_allow_baseline`（`<count> <path>`，只许下降）
/// 是唯一权威，因此这里把两者钉死：基线降了而文档没跟、或文档写了个凭空的数字，都会红。
///
/// 不校验 `--print` 的 `N site(s)` 总数：它 = 无理由 + 有理由，而基线只记无理由，
/// 从文件里算不出来 —— 硬编码它只会制造第二处会漂的数字。
#[test]
fn clippy_allow_counts_in_the_audit_match_the_ratchet_baseline() {
    const BASELINE: &str = "scripts/ci/clippy_allow_baseline";
    const AUDIT: &str = "docs/audit/COMPREHENSIVE_AUDIT_2026-10-22.md";

    let (mut unjustified, mut files) = (0_u64, 0_u64);
    for line in read(BASELINE).lines() {
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        let count = line.split_whitespace().next().unwrap_or_else(|| panic!("`{BASELINE}` 行格式变了：{line}"));
        unjustified += count.parse::<u64>().unwrap_or_else(|_| panic!("`{BASELINE}` 的行首不是计数：{line}"));
        files += 1;
    }
    // 空基线是棘轮的目标态（0 处无理由），不是守卫前提失效：2026-10-26 存量已全部补
    // `// reason:`，文件只剩头部注释。缺文件由 `read()` 直接失败，仍保持 fail-closed。

    let audit = read(AUDIT);
    let claims = [
        ("### CODE-11", format!("{unjustified} 处")),
        ("### CODE-11", format!("（{files} 文件）")),
        ("| **CODE-11** |", format!("{unjustified} 处")),
        ("| **CODE-11** |", format!("（{files} 文件）")),
        ("| **CODE-11** |", format!("基线 {unjustified}/{files}")),
        ("存量 **", format!("存量 **{unjustified} 处**")),
    ];
    for (marker, needle) in claims {
        let line = audit
            .lines()
            .find(|line| line.contains(marker))
            .unwrap_or_else(|| panic!("`{AUDIT}` 里找不到 `{marker}` 行 —— CODE-11 的登记被删了"));
        assert!(
            line.contains(&needle),
            "CODE-11 的数字与棘轮基线不一致：基线现算 {unjustified} 处 / {files} 文件，\
             而文档该行缺 `{needle}`。先跑 `python3 scripts/ci/check_clippy_allow_ratchet.py --print` 核对\n{line}"
        );
    }
}
