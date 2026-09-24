//! D-36 守卫 B：生产 `INSERT` 的**列覆盖** CATALOG 检查（迁移 schema + 一次 catalog 查询）。
//!
//! ## 缺陷家族
//!
//! `D-10`（`media_callbacks.user_id`）、`D-11`（`room_invites.inviter`/`invitee`）、
//! `D-31`（`background_updates.update_name`）、`D-33`（`push_notification_log.sent_at`）
//! 全是"INSERT 的列清单漏了一个写入端必须提供的列"。在**迁移 schema** 上这些 INSERT
//! 必然 `23502` / `23514`，但模块 DB 测试跑在自建简化表上，约束被抹掉，于是永远绿
//! （守卫 A 管这件事的静态侧；本文件管动态侧）。
//!
//! ## 两条规则（都不读业务代码，只查 catalog）
//!
//! * **R1** `is_nullable='NO' AND column_default IS NULL AND is_identity='NO'
//!   AND is_generated='NEVER'` 的列，必须出现在该表任一 INSERT 的**字面量列清单**里；
//! * **R2** baseline 的 `ck_<table>_user_id_format` 家族（
//!   `migrations/00000000_unified_schema_v12.sql` 的 DO 循环）里 `user_id` 为
//!   `NOT NULL` 的表，列清单必须含 `user_id`。R2 专抓 `NOT NULL DEFAULT ''`：
//!   它有默认值所以 R1 不响，而默认值 `''` 违反 CHECK ⇒ 必然 23514（D-10 的形态）。
//!
//! 为什么便宜：列清单由 `scripts/ci/sqlx_query_census.py --emit-inserts` 静态抽出
//! （复用同一份词法剥离与生产/test 分区），catalog 侧只需两条 `information_schema`
//! / `pg_constraint` 查询；不需要"评估任意 CHECK 谓词"。
//!
//! ## 已知边界
//!
//! 列清单为运行期拼装（`format!` / `QueryBuilder`）或干脆没有列清单的 INSERT
//! （`INSERT … DEFAULT VALUES`）**抽不到列**，会被记为"未覆盖"并在
//! `scripts/ci/insert_column_allowlist` 里显式列出，而不是判通过。这类站点由
//! §7 D-14 跟踪。
//!
//! ## 自证能变红（铁律 8）
//!
//! `guard_reports_a_probe_table_missing_required_columns` 在隔离 schema 里建一张
//! 探针表（`user_id TEXT NOT NULL DEFAULT ''` + 一个 `NOT NULL` 无默认列 + 同名
//! `ck_*_user_id_format` 约束），喂给同一个判定函数，断言 R1 与 R2 **都**报违规，
//! 且补全列清单后转绿。没有这条，"全绿"与"判定函数根本没在工作"无法区分。
//!
//! ## RED 重放
//!
//! `SYNAPSE_SQL_GUARD_ROOT=<pre-fix checkout>` 可对历史版本重放本守卫。实测对
//! W1 之前（`2192a6d99`）的源码运行会报出
//! `synapse-storage/src/module.rs::media_callbacks user_id` 与
//! `synapse-storage/src/background_update.rs::background_updates update_name` —— 即
//! §8.2 W1 的验收样本。

use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

use serde::Deserialize;
use sqlx::{PgPool, Row};
use synapse_common::test_isolation::IsolatedTestPool;

/// 工作区迁移 baseline，编译进来交给共享的 `IsolatedTestPool`。
///
/// 与 `synapse-e2ee/src/verification/service.rs`、`synapse-storage/src/test_isolation.rs`
/// 传入的是**同一份字节**（`include_str!` 同一个文件），因此共用同一个内容指纹模板；
/// 一旦有人改传别的字符串就会多出一份模板（`tests/unit/test_isolation_unification_tests.rs`
/// 的 Guard 5 钉的正是这件事）。
const BASELINE_SQL: &str = include_str!("../../migrations/00000000_unified_schema_v12.sql");

#[derive(Debug, Deserialize)]
struct InsertSite {
    path: String,
    item: String,
    line: u64,
    table: String,
    columns: Vec<String>,
    dynamic: bool,
}

fn repo_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
}

fn scan_root() -> PathBuf {
    match std::env::var("SYNAPSE_SQL_GUARD_ROOT") {
        Ok(value) if !value.trim().is_empty() => PathBuf::from(value),
        _ => repo_root(),
    }
}

fn allowlist_path() -> PathBuf {
    repo_root().join("scripts/ci/insert_column_allowlist")
}

/// 迁移到最新 baseline 的隔离 schema（v12 模板克隆）。
///
/// 必须用模板克隆而不是 `prepare_isolated_test_pool()`：后者在 `Strict` 模式下跑
/// `DatabaseInitService` 的**自建** schema，那份定义不是迁移 baseline，本守卫要检的
/// 正是"生产 INSERT vs 迁移 baseline"。
async fn guard_pool() -> (IsolatedTestPool, PgPool) {
    let isolated = IsolatedTestPool::new(BASELINE_SQL)
        .await
        .expect("insert_column_coverage guard requires the migrated test template");
    let pool = (*isolated.pool()).clone();
    (isolated, pool)
}

fn production_inserts_from(root: &Path) -> Vec<InsertSite> {
    let out = Command::new("python3")
        .arg(repo_root().join("scripts/ci/sqlx_query_census.py"))
        .arg("--root")
        .arg(root)
        .arg("--emit-inserts")
        .output()
        .expect("census script must be spawnable");
    assert!(out.status.success(), "--emit-inserts must succeed: {}", String::from_utf8_lossy(&out.stderr));
    serde_json::from_slice(&out.stdout).expect("--emit-inserts must emit valid JSON")
}

fn read_allowlist(path: &Path) -> BTreeSet<String> {
    let text = fs::read_to_string(path).unwrap_or_else(|error| panic!("failed to read {}: {error}", path.display()));
    text.lines()
        .map(str::trim)
        .filter(|line| !line.is_empty() && !line.starts_with('#'))
        .filter_map(|line| line.split_whitespace().next().map(String::from))
        .collect()
}

/// R1 的必需列：`NOT NULL` + 无默认值 + 非 identity/generated。
async fn required_columns(pool: &PgPool) -> BTreeMap<String, BTreeSet<String>> {
    let rows = sqlx::query(
        "SELECT table_name, column_name FROM information_schema.columns \
         WHERE table_schema = current_schema() \
           AND is_nullable = 'NO' AND column_default IS NULL \
           AND is_identity = 'NO' AND is_generated = 'NEVER' \
           AND table_name NOT LIKE '\\_%' ESCAPE '\\'",
    )
    .fetch_all(pool)
    .await
    .expect("information_schema.columns must be readable");

    let mut required: BTreeMap<String, BTreeSet<String>> = BTreeMap::new();
    for row in rows {
        let table: String = row.get("table_name");
        let column: String = row.get("column_name");
        required.entry(table).or_default().insert(column);
    }
    required
}

/// R2 的目标表：带 `ck_<table>_user_id_format` 且 `user_id` 为 `NOT NULL`。
///
/// `NOT NULL DEFAULT ''` 也在此列（它的默认值恰恰违反 CHECK —— D-10 的形态）。
async fn not_null_user_id_tables(pool: &PgPool) -> BTreeSet<String> {
    let rows = sqlx::query(
        "SELECT c.relname AS table_name FROM pg_constraint con \
         JOIN pg_class c ON c.oid = con.conrelid \
         JOIN pg_namespace n ON n.oid = c.relnamespace \
         JOIN information_schema.columns col \
              ON col.table_schema = n.nspname AND col.table_name = c.relname \
             AND col.column_name = 'user_id' \
         WHERE con.contype = 'c' AND n.nspname = current_schema() \
           AND con.conname LIKE 'ck\\_%\\_user\\_id\\_format' ESCAPE '\\' \
           AND col.is_nullable = 'NO'",
    )
    .fetch_all(pool)
    .await
    .expect("pg_constraint must be readable");

    rows.into_iter().map(|row| row.get::<String, _>("table_name")).collect()
}

/// 判定：返回违规描述（空 = 通过）。**唯一**的规则实现，真门禁与红证明共用。
fn check_inserts(
    inserts: &[InsertSite],
    required: &BTreeMap<String, BTreeSet<String>>,
    user_id_tables: &BTreeSet<String>,
    allowlist: &BTreeSet<String>,
) -> Vec<String> {
    let mut violations: Vec<String> = Vec::new();
    let mut reported: BTreeSet<String> = BTreeSet::new();

    for site in inserts {
        let allowlist_key = format!("{}::{}", site.path, site.table);
        if allowlist.contains(&allowlist_key) {
            continue;
        }
        if site.dynamic {
            let key = format!("{allowlist_key}#dynamic");
            if reported.insert(key) {
                violations.push(format!(
                    "{}:{} unknown/uncheckable INSERT column list into `{}` (register in {})",
                    site.path,
                    site.line,
                    site.table,
                    allowlist_path().display()
                ));
            }
            continue;
        }

        let present: BTreeSet<String> = site.columns.iter().cloned().collect();
        let mut missing: Vec<String> = Vec::new();
        if let Some(required_cols) = required.get(&site.table) {
            missing.extend(required_cols.difference(&present).cloned());
        }
        if user_id_tables.contains(&site.table) && !present.contains("user_id") {
            missing.push("user_id".to_string());
        }
        if missing.is_empty() {
            continue;
        }
        missing.sort();
        missing.dedup();
        let key = format!("{allowlist_key}::{}", missing.join(","));
        // 同一个 `path::table` 可能有多条 INSERT；每个缺失列集合只报一次。
        if reported.insert(key) {
            violations.push(format!(
                "{}:{} INSERT INTO `{}` omits required column(s) {} (item: {})",
                site.path,
                site.line,
                site.table,
                missing.join(", "),
                site.item
            ));
        }
    }
    violations
}

// =============================================================================
// 真门禁
// =============================================================================

#[tokio::test]
async fn production_inserts_cover_required_columns() {
    let (_isolated, pool) = guard_pool().await;

    let inserts = production_inserts_from(&scan_root());
    assert!(!inserts.is_empty(), "扫描器没有抽出任何生产 INSERT —— 扫描面或分区判定可能已失效");

    let violations = check_inserts(
        &inserts,
        &required_columns(&pool).await,
        &not_null_user_id_tables(&pool).await,
        &read_allowlist(&allowlist_path()),
    );

    assert!(
        violations.is_empty(),
        "生产 INSERT 的列清单漏了迁移 baseline 要求的列（{} 处）：\n  {}\n\
         这类缺陷在真实 schema 上必然 23502/23514，而模块 DB 测试若不跑迁移模板就看不见（D-36）。\
         修写入端，或在 {} 里显式登记（键：path::table）。",
        violations.len(),
        violations.join("\n  "),
        allowlist_path().display()
    );
}

/// 名单不能变成"万能豁免"：每条都必须仍然命中一个真实 INSERT 站点。
#[tokio::test]
async fn insert_allowlist_entries_all_still_match_something() {
    let inserts = production_inserts_from(&scan_root());
    let live: BTreeSet<String> = inserts.iter().map(|site| format!("{}::{}", site.path, site.table)).collect();
    let stale: Vec<String> = read_allowlist(&allowlist_path()).into_iter().filter(|key| !live.contains(key)).collect();
    assert!(
        stale.is_empty(),
        "scripts/ci/insert_column_allowlist 有 {} 条已失效的条目（对应 INSERT 已删/改名）：{} —— 请删除这些行",
        stale.len(),
        stale.join(", ")
    );
}

// =============================================================================
// 红证明（铁律 8）：探针表必须让 R1 与 R2 都变红，补全列清单后转绿
// =============================================================================

#[tokio::test]
async fn guard_reports_a_probe_table_missing_required_columns() {
    let (_isolated, pool) = guard_pool().await;

    // 探针表刻意复刻两种形态：R1（`must_fill TEXT NOT NULL` 无默认）与
    // R2（`user_id TEXT NOT NULL DEFAULT ''` + 同名 ck 约束 —— D-10 的形态）。
    sqlx::query(
        "CREATE TABLE guard_probe_rows (
            id BIGSERIAL PRIMARY KEY,
            user_id TEXT NOT NULL DEFAULT '',
            must_fill TEXT NOT NULL
         )",
    )
    .execute(&pool)
    .await
    .expect("probe table must be creatable");

    sqlx::query(
        "ALTER TABLE guard_probe_rows ADD CONSTRAINT ck_guard_probe_rows_user_id_format \
         CHECK (user_id IS NULL OR user_id ~ '^@[a-zA-Z0-9._=+./-]+:[a-zA-Z0-9.-]+$')",
    )
    .execute(&pool)
    .await
    .expect("probe check constraint must be creatable");

    let required = required_columns(&pool).await;
    let user_id_tables = not_null_user_id_tables(&pool).await;
    assert!(
        required.get("guard_probe_rows").is_some_and(|cols| cols.contains("must_fill")),
        "R1 must see the probe's NOT NULL column, got {:?}",
        required.get("guard_probe_rows")
    );
    assert!(
        user_id_tables.contains("guard_probe_rows"),
        "R2 must see the probe's ck_*_user_id_format table, got {user_id_tables:?}"
    );

    let bad = InsertSite {
        path: "probe/guard.rs".to_string(),
        item: "probe".to_string(),
        line: 1,
        table: "guard_probe_rows".to_string(),
        columns: vec!["id".to_string()],
        dynamic: false,
    };
    let violations = check_inserts(&[bad], &required, &user_id_tables, &BTreeSet::new());
    assert_eq!(violations.len(), 1, "R1+R2 must report exactly one violation, got {violations:?}");
    assert!(violations[0].contains("must_fill"), "R1 must fire: {violations:?}");
    assert!(violations[0].contains("user_id"), "R2 must fire: {violations:?}");

    // GREEN：列清单补全后必须转绿。
    let good = InsertSite {
        path: "probe/guard.rs".to_string(),
        item: "probe".to_string(),
        line: 1,
        table: "guard_probe_rows".to_string(),
        columns: vec!["id".to_string(), "user_id".to_string(), "must_fill".to_string()],
        dynamic: false,
    };
    assert!(
        check_inserts(&[good], &required, &user_id_tables, &BTreeSet::new()).is_empty(),
        "a complete column list must pass"
    );

    // 名单键 `path::table` 也必须真的放行（否则名单机制无效）。
    let allowlisted: BTreeSet<String> = ["probe/guard.rs::guard_probe_rows".to_string()].into_iter().collect();
    let bad_again = InsertSite {
        path: "probe/guard.rs".to_string(),
        item: "probe".to_string(),
        line: 1,
        table: "guard_probe_rows".to_string(),
        columns: vec!["id".to_string()],
        dynamic: false,
    };
    assert!(
        check_inserts(&[bad_again], &required, &user_id_tables, &allowlisted).is_empty(),
        "an allowlisted path::table must pass"
    );
}

/// 列清单抽不到（运行期拼装 / 无列清单）必须记为"未覆盖"，不能判通过。
#[tokio::test]
async fn dynamic_column_lists_are_reported_as_uncovered_not_passed() {
    let (_isolated, pool) = guard_pool().await;
    let dynamic = InsertSite {
        path: "probe/dynamic.rs".to_string(),
        item: "probe".to_string(),
        line: 7,
        table: "guard_probe_rows".to_string(),
        columns: Vec::new(),
        dynamic: true,
    };
    let violations = check_inserts(
        &[dynamic],
        &required_columns(&pool).await,
        &not_null_user_id_tables(&pool).await,
        &BTreeSet::new(),
    );
    assert_eq!(violations.len(), 1, "an uncheckable column list must be reported: {violations:?}");
    assert!(violations[0].contains("unknown/uncheckable"), "got {violations:?}");
}
