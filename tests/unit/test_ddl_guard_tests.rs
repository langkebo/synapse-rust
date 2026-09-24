//! D-36 守卫 A：**test 区自建 schema** 的静态守卫（无 DB、无迁移）。
//!
//! ## 缺陷家族
//!
//! `D-10` / `D-11` / `D-31` / `D-33` / `D-34` 五条"写入端漏列 ⇒ 必然失败 / 永远无效"
//! 同一个根因：这些模块的 DB 测试跑在**自建的简化表**上（`CREATE TABLE` 写死一份
//! 缩水 schema），于是 NOT NULL / CHECK / UNIQUE 约束被抹掉，"漏列"在测试里永远绿。
//! `background_update` 最典型：自建表的 `update_name` 可空且没有 UNIQUE，而迁移
//! baseline 里它是 `TEXT NOT NULL UNIQUE` —— `create_update` 漏写它，测试连续通过。
//!
//! ## 守卫做什么
//!
//! 用 `scripts/ci/sqlx_query_census.py --list-test-ddl`（复用计数脚本既有的
//! 注释/字符串词法剥离与生产/test 分区实现，不重写第二份扫描器）列出 test 区里
//! 出现的 `CREATE TABLE` / `ALTER TABLE` / `CREATE SCHEMA` / `DROP SCHEMA` 等 DDL，
//! 要求每一条都出现在显式名单 `scripts/ci/test_ddl_allowlist` 里。
//!
//! 名单的键是 **`path::mod::fn`（不含行号）**：行号型 allowlist 会随 `cargo fmt`
//! 漂移，把豁免悄悄变成漏网（AGENTS.md 铁律 8 与
//! `scripts/shell_routes_allowlist.txt` 的前车之鉴）。
//!
//! ## 扫描边界
//!
//! 扫描面与 `sqlx_query_census.py` 的 `SCAN_DIRS` 完全一致（根 crate + 8 个
//! workspace member 的 `src/`）。独立的 `tests/` 目标（`tests/unit`、
//! `tests/integration`）**不在**扫描面内：它们是测试二进制本身，其夹具不受本守卫
//! 约束；本守卫管的是"生产 crate 里的 `#[cfg(test)]` 模块自建 schema"。
//!
//! ## 自证能变红（铁律 8）
//!
//! `guard_flags_a_new_self_built_table_and_passes_once_allowlisted` 在临时目录里
//! 造一个自建表的 `#[cfg(test)] mod tests`，断言同一套判定逻辑先报违规、把键加进
//! 名单后转绿。没有这条用例，"名单非空 + 全绿"与"扫描器坏了"无法区分。

use std::collections::BTreeSet;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::atomic::{AtomicU32, Ordering};

/// 仓库根目录（`CARGO_MANIFEST_DIR` 即根 crate 目录）。
fn repo_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
}

fn allowlist_path() -> PathBuf {
    repo_root().join("scripts/ci/test_ddl_allowlist")
}

/// 扫描用的根。默认仓库根；`SYNAPSE_SQL_GUARD_ROOT` 可覆盖，用于对历史版本
/// 或临时探针树重放同一守卫（守卫 B 共用这个变量）。
fn scan_root() -> PathBuf {
    match std::env::var("SYNAPSE_SQL_GUARD_ROOT") {
        Ok(value) if !value.trim().is_empty() => PathBuf::from(value),
        _ => repo_root(),
    }
}

/// 运行 `--list-test-ddl`，返回 `path::item:line:VERB` 行（已排序去重）。
fn list_test_ddl(root: &Path) -> Vec<String> {
    let out = Command::new("python3")
        .arg(repo_root().join("scripts/ci/sqlx_query_census.py"))
        .arg("--root")
        .arg(root)
        .arg("--list-test-ddl")
        .output()
        .expect("census script must be spawnable");
    assert!(out.status.success(), "--list-test-ddl must succeed: {}", String::from_utf8_lossy(&out.stderr));
    let mut hits: Vec<String> = String::from_utf8_lossy(&out.stdout)
        .lines()
        .map(str::trim)
        .filter(|l| !l.is_empty())
        .map(String::from)
        .collect();
    hits.sort();
    hits.dedup();
    hits
}

/// `path::item:line:VERB` → 名单键 `path::item`。
fn hit_key(hit: &str) -> String {
    let mut parts = hit.rsplitn(3, ':');
    parts.next(); // VERB
    parts.next(); // line
    parts.next().unwrap_or(hit).to_string()
}

/// 读取名单：忽略空行与 `#` 注释，取每行第一个空白分隔的 token。
fn read_allowlist(path: &Path) -> BTreeSet<String> {
    let text = fs::read_to_string(path).unwrap_or_else(|error| panic!("failed to read {}: {error}", path.display()));
    text.lines()
        .map(str::trim)
        .filter(|line| !line.is_empty() && !line.starts_with('#'))
        .filter_map(|line| line.split_whitespace().next().map(String::from))
        .collect()
}

/// 判定：返回不在名单里的命中（`path::item` 去重，附首个行号便于定位）。
fn unallowlisted_hits(hits: &[String], allowlist: &BTreeSet<String>) -> Vec<String> {
    let mut violations: Vec<String> = Vec::new();
    let mut seen: BTreeSet<String> = BTreeSet::new();
    for hit in hits {
        let key = hit_key(hit);
        if allowlist.contains(&key) || !seen.insert(key) {
            continue;
        }
        violations.push(hit.clone());
    }
    violations
}

fn run_census_on_tree(files: &[(&str, &str)]) -> Vec<String> {
    static SEQ: AtomicU32 = AtomicU32::new(0);
    let root =
        std::env::temp_dir().join(format!("ddl_guard_{}_{}", std::process::id(), SEQ.fetch_add(1, Ordering::SeqCst)));
    if root.exists() {
        fs::remove_dir_all(&root).expect("clean temp root");
    }
    for (rel, content) in files {
        let path = root.join(rel);
        fs::create_dir_all(path.parent().expect("parent")).expect("create temp dirs");
        fs::write(&path, content).expect("write temp source");
    }
    let hits = list_test_ddl(&root);
    let _ = fs::remove_dir_all(&root);
    hits
}

// =============================================================================
// 真门禁
// =============================================================================

#[test]
fn no_unallowlisted_self_built_schema_in_test_regions() {
    let hits = list_test_ddl(&scan_root());
    assert!(!hits.is_empty(), "扫描器没有看到任何 test 区 DDL —— 扫描面或分区判定可能已失效");

    let violations = unallowlisted_hits(&hits, &read_allowlist(&allowlist_path()));
    assert!(
        violations.is_empty(),
        "test 区新增了自建 schema 的 DDL：{} —— DB 测试必须跑在迁移模板上\
         （`test_isolation::isolated_test_pool()`），否则 NOT NULL/CHECK/UNIQUE 约束被抹掉，\
         D-31 那类\"写入端漏列\"会永远绿。确属隔离机制自身的测试才能加进 {}（键：path::mod::fn）。",
        violations.join(", "),
        allowlist_path().display()
    );
}

/// 扫描器必须真的在看这一批站点：抽一个必然存在的键做存在性断言。
///
/// 没有这条，"名单非空 + 全绿"与"扫描器返回了别的东西"无法区分（铁律 8 推论）。
#[test]
fn scanner_actually_sees_a_known_self_built_schema_site() {
    let hits = list_test_ddl(&scan_root());
    let known = "synapse-storage/src/refresh_token/mod.rs::tests::setup_refresh_token_db";
    assert!(
        hits.iter().any(|hit| hit_key(hit) == known),
        "扫描器必须能定位 {known}（一个自建 refresh_token 表的既有夹具）；实得 {hits:?}"
    );
}

/// 名单本身不能是"万能豁免"：每条都必须仍然命中，否则说明代码已删/改名而条目滞留
/// （铁律 1：清理实现的同时必须删条目）。
#[test]
fn allowlist_entries_all_still_match_something() {
    let hits = list_test_ddl(&scan_root());
    let live: BTreeSet<String> = hits.iter().map(|hit| hit_key(hit)).collect();
    let stale: Vec<String> = read_allowlist(&allowlist_path()).into_iter().filter(|key| !live.contains(key)).collect();
    assert!(
        stale.is_empty(),
        "scripts/ci/test_ddl_allowlist 有 {} 条已失效的条目（对应实现已删/改名）：{} —— \
         请删除这些行，否则名单会长期漂移成\"什么都放行\"。",
        stale.len(),
        stale.join(", ")
    );
}

// =============================================================================
// 红证明（铁律 8）：违规探针必须让判定逻辑变红，加进名单后转绿
// =============================================================================

#[test]
fn guard_flags_a_new_self_built_table_and_passes_once_allowlisted() {
    let hits = run_census_on_tree(&[(
        "synapse-storage/src/probe.rs",
        r##"
pub struct Probe;

#[cfg(test)]
mod tests {
    async fn setup_probe_db(pool: &sqlx::PgPool) {
        sqlx::query(
            r#"
            CREATE TABLE IF NOT EXISTS probe_table (
                id BIGSERIAL PRIMARY KEY,
                update_name TEXT
            )
            "#,
        )
        .execute(pool)
        .await
        .expect("create probe table");
    }

    #[tokio::test]
    async fn probe() {
        let _ = setup_probe_db;
    }
}
"##,
    )]);

    assert_eq!(hits.len(), 1, "probe tree must yield exactly one DDL hit, got {hits:?}");
    let key = hit_key(&hits[0]);
    assert_eq!(key, "synapse-storage/src/probe.rs::tests::setup_probe_db");

    // RED：不在名单里 ⇒ 必须报违规。
    let violations = unallowlisted_hits(&hits, &BTreeSet::new());
    assert_eq!(violations, hits, "an unallowlisted self-built table must be reported");

    // GREEN：加进名单 ⇒ 转绿。证明判定依据确实是名单键而不是别的东西。
    let allowlisted: BTreeSet<String> = [key].into_iter().collect();
    assert!(unallowlisted_hits(&hits, &allowlisted).is_empty(), "an allowlisted key must pass");
}

/// DDL 只出现在**字符串夹具**里也必须被抓到（守卫靠字符串字面量取证，不靠注释散文）。
#[test]
fn guard_reads_ddl_out_of_string_literals_not_comments() {
    let hits = run_census_on_tree(&[(
        "synapse-storage/src/probe.rs",
        r##"
// CREATE TABLE comment_only (id INT);  ← prose, must not count
#[cfg(test)]
mod tests {
    fn fixture() -> &'static str {
        r#"
        CREATE TABLE fixture_table (id INT)
        "#
    }

    #[test]
    fn t() {
        let _ = fixture();
    }
}
"##,
    )]);
    assert_eq!(hits.len(), 1, "comment prose must not be counted, the string fixture must be: {hits:?}");
    assert!(hits[0].contains("CREATE TABLE"), "got {hits:?}");
}
