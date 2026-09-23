//! D1 守卫：禁止**新增**生产区字面量动态 SQL（落实 Phase B2）。
//!
//! ## 为什么需要这个文件
//!
//! Phase B2 规定：新增 storage/service 代码必须用 `query!` / `query_as!` /
//! `query_scalar!` 宏；动态 SQL 仅限 DDL、动态标识符、`= ANY($1)`、
//! `QueryBuilder` 与**运行期拼装**的语句文本。但此前**没有任何门禁**能拦住
//! "新写一个 `sqlx::query("SELECT …")`"——`sqlx_ratio_gate_tests.rs` 只看总量
//! 棘轮，而总量在两个方向都可能被掩盖（同一批里 +1 动态 / +1 静态）。
//!
//! 本文件用 `scripts/ci/sqlx_query_census.py` 新增的
//! `--list-production-dynamic` 模式逐条列出生产区动态调用点，并把每条分成
//! `literal`（SQL 实参是字符串字面量）与 `runtime`（`&sql` / `&format!(…)`
//! 运行期拼装）。**只有 `literal` 受棘轮约束**。
//!
//! ## 基线为什么不是 0
//!
//! C1–C10 只静态化了计划表点名的模块；实测生产区仍存 **876 处字面量站点 /
//! 98 文件**（其余生产模块从未进入迁移范围），另有 88 处运行期拼装站点。
//! 因此本守卫是**棘轮**（`scripts/ci/sqlx_literal_production_baseline`
//! 逐文件计数，只禁增不禁减），而不是"绝对 0"断言——后者在当前树上必然全红，
//! 属"永远失败的门禁 = 没有门禁"（反冗余铁律 8 的反面）。
//! 逐文件计数而非 `path:line`：行号会随 `cargo fmt` 漂移。
//!
//! ## 扫描面非空的自证
//!
//! `collect_sources` 曾用 `"/target/" in str(path)` 排除构建产物；当仓库本身位于
//! `target/` 之下（本任务规定的验证树 `git worktree add target/cd-wt`）时，
//! **所有**源文件都含该子串 ⇒ 扫描面被清空 ⇒ 守卫以"0 站点"假通过。本文件
//! 用 [`scan_mode_reports_a_non_empty_production_surface`] 钉住这一点。

use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::atomic::{AtomicU32, Ordering};

/// 扫描根（与 `scripts/ci/sqlx_query_census.py` 的 `SCAN_DIRS` 同源）。
const SCAN_DIRS: [&str; 9] = [
    "src",
    "synapse-common/src",
    "synapse-cache/src",
    "synapse-storage/src",
    "synapse-e2ee/src",
    "synapse-federation/src",
    "synapse-services/src",
    "synapse-web/src",
    "synapse-test-utils/src",
];

/// 生产区动态调用点的实参形态。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum SiteKind {
    /// SQL 实参是字符串字面量 —— 受棘轮约束（Phase B2 禁止新增）。
    Literal,
    /// SQL 实参是运行期拼装的表达式（`&sql` / `&query` / `&format!(…)`）。
    Runtime,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct Site {
    path: String,
    line: usize,
    kind: SiteKind,
}

/// `CARGO_MANIFEST_DIR` 即根 crate 目录，也就是仓库根。
fn repo_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
}

fn census_script() -> PathBuf {
    repo_root().join("scripts/ci/sqlx_query_census.py")
}

fn baseline_path() -> PathBuf {
    repo_root().join("scripts/ci/sqlx_literal_production_baseline")
}

/// 运行普查脚本，返回 `(exit_code, stdout+stderr)`。
fn run_census(args: &[&str]) -> (i32, String) {
    let out = Command::new("python3")
        .arg(census_script())
        .args(args)
        .output()
        .expect("python3 + scripts/ci/sqlx_query_census.py must be spawnable");
    let mut combined = String::from_utf8_lossy(&out.stdout).into_owned();
    combined.push_str(&String::from_utf8_lossy(&out.stderr));
    (out.status.code().unwrap_or(-1), combined)
}

/// 在 `root` 上跑 `--list-production-dynamic` 并解析输出。
fn scan_production_dynamic(root: &Path) -> Vec<Site> {
    let root = root.to_str().expect("root path must be UTF-8");
    let (code, raw) = run_census(&["--list-production-dynamic", root]);
    assert_eq!(code, 0, "普查脚本 --list-production-dynamic 必须以 0 退出，实际:\n{raw}");
    parse_sites(&raw)
}

/// 解析 `path:line:literal|runtime`（允许路径含 `:` 之外的分隔；以最后两个
/// `:` 切分，避免 Windows 盘符式的误切）。
fn parse_sites(raw: &str) -> Vec<Site> {
    let mut sites = Vec::new();
    for line in raw.lines() {
        if line.trim().is_empty() {
            continue;
        }
        let mut parts = line.rsplitn(3, ':');
        let kind = parts.next().unwrap_or_default();
        let line_no = parts.next().unwrap_or_default();
        let path = parts.next().unwrap_or_default();
        let kind = match kind {
            "literal" => SiteKind::Literal,
            "runtime" => SiteKind::Runtime,
            other => panic!("输出行的实参形态只能是 literal|runtime，实际 `{other}`（行：`{line}`）"),
        };
        let line_no: usize =
            line_no.parse().unwrap_or_else(|_| panic!("输出行的行号必须为数字，实际 `{line_no}`（行：`{line}`）"));
        assert!(!path.is_empty(), "输出行必须带相对路径（行：`{line}`）");
        sites.push(Site { path: path.to_owned(), line: line_no, kind });
    }
    sites
}

/// 解析棘轮基线：`<相对路径>\t<计数>`，`#` 起始行与空行忽略。
fn parse_baseline(raw: &str) -> BTreeMap<String, usize> {
    let mut baseline = BTreeMap::new();
    for line in raw.lines() {
        let line = line.trim_end();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        let (path, count) =
            line.split_once('\t').unwrap_or_else(|| panic!("基线行必须是 `<path>\\t<count>` 形式，实际 `{line}`"));
        let count: usize =
            count.parse().unwrap_or_else(|_| panic!("基线计数必须为数字，实际 `{count}`（行：`{line}`）"));
        assert!(baseline.insert(path.to_owned(), count).is_none(), "基线出现重复路径 `{path}`");
    }
    baseline
}

/// 逐文件计数，返回 `path -> 处数`。
fn literal_counts(sites: &[Site]) -> BTreeMap<String, usize> {
    let mut counts: BTreeMap<String, usize> = BTreeMap::new();
    for site in sites.iter().filter(|s| s.kind == SiteKind::Literal) {
        *counts.entry(site.path.clone()).or_default() += 1;
    }
    counts
}

/// 棘轮判定：某文件实测字面量站点数 **超过** 基线即违规。
///
/// 少于基线**不**违规（棘轮只禁增不禁减）：基线偏高是保守的，收紧应由
/// 静态化批次在同步提交里完成。
fn ratchet_violations(sites: &[Site], baseline: &BTreeMap<String, usize>) -> Vec<String> {
    let mut violations = Vec::new();
    for (path, count) in literal_counts(sites) {
        let allowed = baseline.get(&path).copied().unwrap_or(0);
        if count > allowed {
            let lines: Vec<String> = sites
                .iter()
                .filter(|s| s.kind == SiteKind::Literal && s.path == path)
                .map(|s| format!("      {}:{}", s.path, s.line))
                .collect();
            violations.push(format!(
                "  {path}: 生产区字面量动态 SQL {count} 处 > 基线 {allowed} 处（新增 {} 处）\n{}",
                count - allowed,
                lines.join("\n")
            ));
        }
    }
    violations
}

/// 在临时目录里构造源码树并跑 `--list-production-dynamic`。
///
/// 临时根位于系统 temp（不含 `target/` 分量），因此不受构建产物排除规则影响。
fn scan_temp_tree(files: &[(&str, &str)]) -> Vec<Site> {
    static SEQ: AtomicU32 = AtomicU32::new(0);
    let root = std::env::temp_dir().join(format!(
        "sqlx_literal_guard_{}_{}",
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
    let sites = scan_production_dynamic(&root);
    let _ = fs::remove_dir_all(&root);
    sites
}

fn kinds_for(sites: &[Site], path: &str) -> Vec<SiteKind> {
    sites.iter().filter(|s| s.path == path).map(|s| s.kind).collect()
}

// =============================================================================
// 基本存在性
// =============================================================================

#[test]
fn census_script_exists_and_is_a_file() {
    let path = census_script();
    assert!(path.is_file(), "普查脚本应存在于 {path:?}");
}

#[test]
fn literal_ratchet_baseline_exists_and_is_parseable() {
    let raw = fs::read_to_string(baseline_path()).expect("棘轮基线必须可读");
    let baseline = parse_baseline(&raw);
    assert!(!baseline.is_empty(), "棘轮基线不能为空（空基线 = 任何字面量站点都算新增）");
    for dir in SCAN_DIRS {
        // 只校验扫描根形态与基线路径一致（相对仓库根、正斜杠）。
        assert!(!dir.starts_with('/'), "SCAN_DIRS 必须是相对路径：{dir}");
    }
    assert!(
        baseline.keys().all(|k| !k.starts_with('/') && !k.contains('\\')),
        "基线路径必须是相对仓库根、且用正斜杠：{baseline:?}"
    );
}

// =============================================================================
// 扫描面必须非空（假通过防线）
// =============================================================================

/// 扫描面被清空时守卫会以"0 站点"假通过。历史上 `collect_sources` 用绝对路径
/// 子串 `"/target/"` 排除构建产物，导致位于 `target/` 下的验证树整棵被跳过。
#[test]
fn scan_mode_reports_a_non_empty_production_surface() {
    let sites = scan_production_dynamic(&repo_root());
    assert!(sites.len() > 500, "生产区动态站点仅 {} 处，扫描面疑似被整体排除（假通过风险）", sites.len());
    assert!(sites.iter().any(|s| s.kind == SiteKind::Literal), "应识别出字面量站点；全部为 runtime 说明实参判定失效");
    assert!(
        sites.iter().any(|s| s.kind == SiteKind::Runtime),
        "应识别出运行期拼装站点（`&sql` / `&format!`）；一处都没有说明判定失效"
    );
}

/// 新模式的站点总数必须与默认摘要的 `dynamic_production` 一致，否则两套口径
/// 已经漂移（新模式会漏点或重复计点）。
#[test]
fn scan_mode_total_matches_census_dynamic_production() {
    let (code, raw) = run_census(&["--json", "--root", "."]);
    assert_eq!(code, 0, "--json 模式必须以 0 退出，实际:\n{raw}");
    let expected: usize = raw
        .lines()
        .find_map(|l| l.trim().strip_prefix("\"dynamic_production\":"))
        .and_then(|v| v.trim().trim_end_matches(',').parse().ok())
        .unwrap_or_else(|| panic!("--json 输出缺少 dynamic_production：\n{raw}"));
    let sites = scan_production_dynamic(&repo_root());
    assert_eq!(
        sites.len(),
        expected,
        "--list-production-dynamic 列出 {} 处，而 census 的 dynamic_production={expected}；两套口径漂移",
        sites.len()
    );
}

// =============================================================================
// 输出形态
// =============================================================================

/// 输出必须严格是 `path:line:literal|runtime`，且行号与路径非空。
#[test]
fn scan_mode_output_shape_is_path_line_kind() {
    let (_, raw) = run_census(&["--list-production-dynamic", "."]);
    for line in raw.lines().filter(|l| !l.trim().is_empty()) {
        let mut parts = line.rsplitn(3, ':');
        let kind = parts.next().unwrap_or_default();
        let line_no = parts.next().unwrap_or_default();
        let path = parts.next().unwrap_or_default();
        assert!(kind == "literal" || kind == "runtime", "形态非法（kind=`{kind}`）：`{line}`");
        assert!(line_no.parse::<usize>().is_ok(), "形态非法（line=`{line_no}`）：`{line}`");
        assert!(path.ends_with(".rs"), "形态非法（非 .rs 路径）：`{line}`");
    }
}

// =============================================================================
// 实参形态判定（字面量 vs 运行期拼装）
// =============================================================================

/// 运行期拼装的实参**必须**判为 runtime —— 这正是 Phase B2 允许的残差类别，
/// 守卫不得误伤。
#[test]
fn runtime_assembled_sql_is_classified_as_runtime() {
    let sites = scan_temp_tree(&[(
        "src/lib.rs",
        r#"
async fn f(pool: &sqlx::PgPool, sql: String) -> Result<(), sqlx::Error> {
    let mut q = String::from("SELECT 1");
    q.push_str(" WHERE true");
    sqlx::query(&sql).execute(pool).await?;
    sqlx::query(&q).execute(pool).await?;
    sqlx::query(&format!("SELECT {}", 1)).execute(pool).await?;
    sqlx::query_as::<_, (i64,)>(&sql).fetch_one(pool).await?;
    sqlx::query_scalar::<_, i64>(&sql).fetch_one(pool).await?;
    Ok(())
}
"#,
    )]);
    assert_eq!(
        kinds_for(&sites, "src/lib.rs"),
        vec![SiteKind::Runtime; 5],
        "运行期拼装（&sql / &q / &format!）必须全部判为 runtime；实际 {sites:?}"
    );
}

/// 字符串字面量（含 raw string、跨行、turbofish、`AS "col!"` 覆盖）必须判为 literal。
///
/// 夹具里的表名刻意用 `events`（真实表）而不是 `t`：本文件整体会被
/// `scripts/check_schema_table_coverage.py` 逐字扫一遍，任何 `FROM <标识符>`
/// 都会被当成"引用了未在迁移中定义的表"，一个占位表名会直接让那条门禁变红。
#[test]
fn string_literal_sql_is_classified_as_literal() {
    let sites = scan_temp_tree(&[(
        "src/lib.rs",
        r##"
async fn f(pool: &sqlx::PgPool) -> Result<(), sqlx::Error> {
    sqlx::query("SELECT 1").execute(pool).await?;
    sqlx::query(
        "SELECT 2",
    )
    .execute(pool)
    .await?;
    sqlx::query_as::<_, (i64, String)>("SELECT 3, 'x'").fetch_one(pool).await?;
    sqlx::query_scalar::<_, i64>("SELECT 4").fetch_one(pool).await?;
    sqlx::query(r"SELECT 5").execute(pool).await?;
    sqlx::query(r#"SELECT "col!" FROM events"#).execute(pool).await?;
    Ok(())
}
"##,
    )]);
    assert_eq!(
        kinds_for(&sites, "src/lib.rs"),
        vec![SiteKind::Literal; 6],
        "字面量（含 raw string / 跨行 / turbofish）必须全部判为 literal；实际 {sites:?}"
    );
}

/// 注释与字符串里的 `sqlx::query(` 不得计为站点（词法剥离已覆盖，这里防回归）。
#[test]
fn comments_and_non_sql_strings_do_not_count() {
    let sites = scan_temp_tree(&[(
        "src/lib.rs",
        r##"
//! prose mentioning sqlx::query(
/// doc mentioning sqlx::query_as(
/* block mentioning sqlx::query_scalar( */
const TEMPLATE: &str = "sqlx::query( SELECT 1";
fn f() -> &'static str { "sqlx::query(" }
"##,
    )]);
    assert!(sites.is_empty(), "注释/字符串里的 `sqlx::query(` 被算成了调用：{sites:?}");
}

/// 注释可以夹在 `(` 与字面量之间，判定必须仍为 literal（等长剥离 + 原文跳注释）。
#[test]
fn comment_between_paren_and_literal_is_still_literal() {
    let sites = scan_temp_tree(&[(
        "src/lib.rs",
        r#"
async fn f(pool: &sqlx::PgPool) -> Result<(), sqlx::Error> {
    sqlx::query(
        // 这条是生产查询
        "SELECT 1",
    )
    .execute(pool)
    .await?;
    Ok(())
}
"#,
    )]);
    assert_eq!(
        kinds_for(&sites, "src/lib.rs"),
        vec![SiteKind::Literal],
        "`(` 与字面量之间的注释不应把判定推成 runtime；实际 {sites:?}"
    );
}

// =============================================================================
// 区域划分：`#[cfg(test)]` 块与 `#[cfg(test)] mod x;` 整文件
// =============================================================================

#[test]
fn cfg_test_block_is_excluded_from_production() {
    let sites = scan_temp_tree(&[(
        "src/lib.rs",
        r#"
fn production() {
    let _ = sqlx::query("SELECT prod");
}

#[cfg(test)]
mod tests {
    fn helper() { let _ = sqlx::query("SELECT test"); }
}
"#,
    )]);
    assert_eq!(
        kinds_for(&sites, "src/lib.rs"),
        vec![SiteKind::Literal],
        "生产区应只计 1 处；`#[cfg(test)]` 块内的调用必须排除：{sites:?}"
    );
}

/// `#[cfg(test)] mod db_tests;` 引入的**整份文件**必须排除。
///
/// ⚠️ 夹具刻意把生产函数放在声明**之前**：脚本对"挂在无块体条目上的
/// `#[cfg(test)]`"用"顺延到下一个 `{`"的近似（脚本头部已登记），因此声明在前
/// 会让紧随其后的 `fn production() { … }` 被临时划入 test 区。这是脚本既有的
/// 已知近似，不是本守卫引入的行为，夹具应避开它。
#[test]
fn cfg_test_gated_file_is_excluded_from_production() {
    let sites = scan_temp_tree(&[
        (
            "src/lib.rs",
            r#"
fn production() {
    let _ = sqlx::query("SELECT prod");
}

#[cfg(test)]
mod db_tests;
"#,
        ),
        ("src/db_tests.rs", "fn helper() { let _ = sqlx::query(\"SELECT test file\"); }\n"),
    ]);
    assert_eq!(kinds_for(&sites, "src/lib.rs"), vec![SiteKind::Literal], "生产区应只计 1 处：{sites:?}");
    assert!(
        sites.iter().all(|s| s.path != "src/db_tests.rs"),
        "`#[cfg(test)] mod db_tests;` 引入的整份文件必须排除：{sites:?}"
    );
}

// =============================================================================
// 棘轮语义：新增即红，运行期拼装永远绿
// =============================================================================

#[test]
fn ratchet_fails_on_a_new_literal_site_in_a_known_file() {
    let files = [(
        "src/lib.rs",
        r#"
async fn f(pool: &sqlx::PgPool) -> Result<(), sqlx::Error> {
    sqlx::query("SELECT 1").execute(pool).await?;
    sqlx::query("SELECT 2").execute(pool).await?;
    Ok(())
}
"#,
    )];
    let sites = scan_temp_tree(&files);
    let baseline = parse_baseline("src/lib.rs\t1\n");
    let violations = ratchet_violations(&sites, &baseline);
    assert_eq!(violations.len(), 1, "计数 2 > 基线 1 必须判违规；实际 {violations:?}");
    assert!(violations[0].contains("src/lib.rs:4"), "违规信息必须带 `path:line`：{}", violations[0]);
}

#[test]
fn ratchet_fails_on_a_new_literal_site_in_an_unknown_file() {
    let files = [(
        "src/lib.rs",
        r#"
async fn f(pool: &sqlx::PgPool) -> Result<(), sqlx::Error> {
    sqlx::query("SELECT 1").execute(pool).await?;
    Ok(())
}
"#,
    )];
    let sites = scan_temp_tree(&files);
    let violations = ratchet_violations(&sites, &parse_baseline(""));
    assert_eq!(violations.len(), 1, "基线未登记的文件出现字面量站点必须违规；实际 {violations:?}");
    assert!(violations[0].contains("src/lib.rs:3"), "违规信息必须带 `path:line`：{}", violations[0]);
}

#[test]
fn ratchet_passes_when_the_literal_count_matches_the_baseline() {
    let files = [(
        "src/lib.rs",
        r#"
async fn f(pool: &sqlx::PgPool) -> Result<(), sqlx::Error> {
    sqlx::query("SELECT 1").execute(pool).await?;
    Ok(())
}
"#,
    )];
    let sites = scan_temp_tree(&files);
    assert!(ratchet_violations(&sites, &parse_baseline("src/lib.rs\t1\n")).is_empty(), "实测与基线相等不应违规");
}

#[test]
fn ratchet_allows_runtime_assembled_sites_without_any_baseline_entry() {
    let files = [(
        "src/lib.rs",
        r#"
async fn f(pool: &sqlx::PgPool, sql: String) -> Result<(), sqlx::Error> {
    sqlx::query(&sql).execute(pool).await?;
    sqlx::query(&format!("SELECT {sql}")).execute(pool).await?;
    Ok(())
}
"#,
    )];
    let sites = scan_temp_tree(&files);
    assert!(
        ratchet_violations(&sites, &parse_baseline("")).is_empty(),
        "运行期拼装站点是 Phase B2 明确允许的残差，不得触发棘轮：{sites:?}"
    );
}

// =============================================================================
// 真实树：生产区不得出现**未登记**的字面量动态 SQL
// =============================================================================

/// 主守卫：生产区字面量动态 SQL 只减不增。
///
/// 失败时输出 `path:line:literal` 明细；新增了字面量动态 SQL 的批次应改为
/// `query!` / `query_as!` / `query_scalar!`（Phase B2），或在极少数确需运行期
/// 文本时改为 `&sql` 形式（`runtime`，不受本棘轮约束）。
#[test]
fn no_new_production_literal_dynamic_sql() {
    let sites = scan_production_dynamic(&repo_root());
    assert!(!sites.is_empty(), "扫描面为空 ⇒ 守卫会假通过（见 /target/ 排除规则缺陷）");

    let raw = fs::read_to_string(baseline_path()).expect("棘轮基线必须可读");
    let baseline = parse_baseline(&raw);
    let violations = ratchet_violations(&sites, &baseline);
    assert!(
        violations.is_empty(),
        "检测到 {} 个文件新增了生产区字面量动态 SQL（Phase B2 违规）。\n\
         修法：改用 query!/query_as!/query_scalar! 宏，或改成运行期拼装的 `&sql`。\n\
         若确属已有站点被合并/移动（不应计数变化），请复核后下调\n\
         `scripts/ci/sqlx_literal_production_baseline` 中对应数字。\n\n{}",
        violations.len(),
        violations.join("\n")
    );
}
