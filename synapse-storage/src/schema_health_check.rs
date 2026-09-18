//! 数据库 Schema 健康检查模块
//!
//! 提供启动时的 Schema 验证，不在服务启动阶段执行运行时索引修复。
//!
//! 使用方法:
//! ```text
//! use synapse_storage::schema_health_check::run_schema_health_check;
//!
//! #[tokio::main]
//! async fn main() -> Result<(), Box<dyn std::error::Error>> {
//!     let pool = create_pool().await?;
//!     run_schema_health_check(&pool, false).await?;
//!     Ok(())
//! }
//! ```
//!
//! ## 设计 (DB-02)
//!
//! - **核心表清单来自 baseline schema 文件** (`migrations/00000000_unified_schema_v12.sql`)。
//!   通过 [`crate::baseline_tables`] 在编译期通过 `include_str!` 嵌入并解析，
//!   避免手工维护 200+ 张表的 `CORE_TABLES` 常量。
//! - **批量查询**：用 `ANY($1)` 或 `unnest` 一次往返而不是 N 次 (先前 C-4 优化)。
//! - **迁移完整性**：通过 [`crate::migration_checks`] 验证 `_sqlx_migrations` 表
//!   与 migrations 目录的一致性。
//! - **Drift 警告**：实际基础表（`table_type = 'BASE TABLE'`，排除视图）数量与
//!   baseline 期望差距过大时发警告。

use sqlx::{Pool, Postgres};
use tracing::{error, info, warn};

use crate::baseline_tables::baseline_tables;
use crate::migration_checks::{check_migration_completeness, count_public_tables};

const AUTO_REPAIR_DISABLED_MESSAGE: &str =
    "Schema health check detected missing indexes. Apply the managed migrations via docker/db_migrate.sh instead of repairing schema at runtime.";

/// 核心字段定义 (表名, 字段名)
///
/// 这些是必须在每张核心表中存在的业务关键字段。表名清单本身是从
/// baseline schema 自动推导的（见 [`crate::baseline_tables`]），但每张表的
/// 关键字段仍然需要手工维护——因为哪些列是"业务关键"是一个
/// 语义判断，不是 schema 结构问题。
const CORE_COLUMNS: &[(&str, &str)] = &[
    // users 表
    ("users", "user_id"),
    ("users", "username"),
    ("users", "created_ts"),
    ("users", "password_hash"),
    ("users", "is_deactivated"),
    ("users", "generation"),
    // devices 表
    ("devices", "device_id"),
    ("devices", "user_id"),
    ("devices", "last_seen_ts"),
    // rooms 表
    ("rooms", "room_id"),
    ("rooms", "creator"),
    ("rooms", "created_ts"),
    ("rooms", "is_public"),
    // room_aliases 表
    ("room_aliases", "room_alias"),
    ("room_aliases", "room_id"),
    ("room_aliases", "server_name"),
    ("room_aliases", "created_ts"),
    // events 表
    ("events", "event_id"),
    ("events", "room_id"),
    ("events", "sender"),
    ("events", "origin_server_ts"),
    ("events", "event_type"),
    // event_relations 表
    ("event_relations", "room_id"),
    ("event_relations", "event_id"),
    ("event_relations", "relates_to_event_id"),
    ("event_relations", "relation_type"),
    // room_memberships 表
    ("room_memberships", "room_id"),
    ("room_memberships", "user_id"),
    ("room_memberships", "membership"),
    ("room_memberships", "joined_ts"),
    ("room_memberships", "invited_ts"),
    ("room_memberships", "left_ts"),
    // access_tokens 表
    ("access_tokens", "token_hash"),
    ("access_tokens", "user_id"),
    ("access_tokens", "device_id"),
    ("access_tokens", "created_ts"),
    // refresh_tokens 表
    ("refresh_tokens", "token_hash"),
    ("refresh_tokens", "user_id"),
    // user_threepids 表
    ("user_threepids", "user_id"),
    ("user_threepids", "medium"),
    ("user_threepids", "address"),
    ("user_threepids", "validated_at"),
    ("user_threepids", "verification_expires_at"),
    // presence 表
    ("presence", "user_id"),
    ("presence", "presence"),
    ("presence", "last_active_ts"),
    ("federation_signing_keys", "server_name"),
    ("federation_signing_keys", "key_id"),
    ("federation_signing_keys", "created_ts"),
    // rate_limits 表
    ("rate_limits", "user_id"),
    ("rate_limits", "messages_per_second"),
    ("rate_limits", "burst_count"),
    // report_rate_limits 表
    ("report_rate_limits", "user_id"),
    ("report_rate_limits", "report_count"),
    ("report_rate_limits", "last_report_at"),
    ("report_rate_limits", "blocked_until_at"),
    ("report_rate_limits", "block_reason"),
    ("report_rate_limits", "created_ts"),
    ("report_rate_limits", "updated_ts"),
    // server_notices 表
    ("server_notices", "id"),
    ("server_notices", "user_id"),
    ("server_notices", "event_id"),
    ("server_notices", "content"),
    ("server_notices", "sent_ts"),
    // user_notification_settings 表
    ("user_notification_settings", "user_id"),
    ("user_notification_settings", "is_enabled"),
    // widgets 表
    ("widgets", "widget_id"),
    ("widgets", "room_id"),
    ("widgets", "user_id"),
    ("widgets", "widget_type"),
    // secure_key_backups 表
    ("secure_key_backups", "user_id"),
    ("secure_key_backups", "backup_id"),
    ("secure_key_backups", "version"),
    ("secure_key_backups", "algorithm"),
    // secure_backup_session_keys 表
    ("secure_backup_session_keys", "user_id"),
    ("secure_backup_session_keys", "backup_id"),
    ("secure_backup_session_keys", "room_id"),
    ("secure_backup_session_keys", "session_id"),
    ("secure_backup_session_keys", "encrypted_key"),
    // background_updates 表
    ("background_updates", "update_name"),
    ("background_updates", "status"),
    ("background_updates", "retry_count"),
    ("background_updates", "max_retries"),
    ("background_updates", "is_running"),
    // room_retention_policies 表
    ("room_retention_policies", "room_id"),
    ("room_retention_policies", "max_lifetime"),
    ("room_retention_policies", "is_expire_on_clients"),
    ("room_retention_policies", "is_server_default"),
];

struct RequiredIndex {
    display_name: &'static str,
    acceptable_names: &'static [&'static str],
}

/// 必需索引定义。
///
/// `acceptable_names` 允许兼容旧迁移和约束自动生成的唯一索引名，
/// 避免数据库已经具备等价索引时仍然报“缺失索引”。
const REQUIRED_INDEXES: &[RequiredIndex] = &[
    // `idx_events_room_id` 不在此列：`room_id` 是 events 上 ≥6 个其它索引的前导列，
    // 该冗余索引已由 baseline 删除（DB_REVIEW §12.1），要求它只会制造启动时的
    // 虚假 `Missing indexes` 警告（DB_REVIEW §14.5）。
    RequiredIndex { display_name: "idx_events_sender", acceptable_names: &["idx_events_sender", "idx_events_user_id"] },
    RequiredIndex { display_name: "idx_events_origin_server_ts", acceptable_names: &["idx_events_origin_server_ts"] },
    RequiredIndex { display_name: "idx_events_room_time", acceptable_names: &["idx_events_room_time"] },
    RequiredIndex { display_name: "idx_memberships_user_room", acceptable_names: &["idx_memberships_user_room"] },
    RequiredIndex {
        display_name: "idx_room_memberships_user_membership",
        acceptable_names: &["idx_room_memberships_user_membership", "idx_memberships_user_membership"],
    },
    RequiredIndex {
        display_name: "uq_room_memberships_room_user",
        acceptable_names: &["uq_room_memberships_room_user", "idx_memberships_room_user"],
    },
    RequiredIndex { display_name: "uq_users_username", acceptable_names: &["uq_users_username", "idx_users_username"] },
    RequiredIndex { display_name: "idx_users_created_ts", acceptable_names: &["idx_users_created_ts"] },
    RequiredIndex { display_name: "idx_devices_user_id", acceptable_names: &["idx_devices_user_id"] },
    // v10 中 presence.user_id 已由主键 pk_presence（唯一）及 idx_presence_user_id
    // 覆盖，且所有 presence 查询均仅按 user_id 过滤，故旧的 v07 复合索引
    // idx_presence_user_status(user_id, presence) 已被冗余收敛，此处接受其 v10 等价物。
    RequiredIndex {
        display_name: "idx_presence_user_status",
        acceptable_names: &["idx_presence_user_status", "pk_presence", "idx_presence_user_id"],
    },
    RequiredIndex {
        display_name: "idx_access_tokens_user_id",
        acceptable_names: &["idx_access_tokens_user_id", "idx_access_tokens_user"],
    },
    RequiredIndex {
        display_name: "idx_access_tokens_token_hash",
        acceptable_names: &["idx_access_tokens_token_hash", "uq_access_tokens_token_hash"],
    },
    RequiredIndex {
        display_name: "idx_refresh_tokens_user_id",
        acceptable_names: &["idx_refresh_tokens_user_id", "idx_refresh_tokens_user"],
    },
    RequiredIndex { display_name: "idx_user_threepids_user", acceptable_names: &["idx_user_threepids_user"] },
    RequiredIndex {
        display_name: "idx_user_threepids_medium_address",
        acceptable_names: &["idx_user_threepids_medium_address", "uq_user_threepids_medium_address"],
    },
];

/// 健康检查结果
#[derive(Debug)]
pub struct HealthCheckResult {
    /// The `passed` field.
    pub passed: bool,
    /// The `missing_tables` field.
    pub missing_tables: Vec<String>,
    /// The `missing_columns` field.
    pub missing_columns: Vec<String>,
    /// The `missing_indexes` field.
    pub missing_indexes: Vec<String>,
    /// The `repaired_indexes` field.
    pub repaired_indexes: Vec<String>,
    /// The `warnings` field.
    pub warnings: Vec<String>,
    /// DB-02 新增：baseline 与实际表数的差异（正值 = 数据库多了，负值 = 少了）
    pub baseline_drift: i64,
    /// DB-02 新增：当前数据库中已应用的迁移数量
    pub applied_migration_count: i64,
    /// DB-02 新增：缺少的迁移版本号（如果有）
    pub missing_migrations: Vec<i64>,
}

impl Default for HealthCheckResult {
    fn default() -> Self {
        Self {
            passed: true,
            missing_tables: Vec::new(),
            missing_columns: Vec::new(),
            missing_indexes: Vec::new(),
            repaired_indexes: Vec::new(),
            warnings: Vec::new(),
            baseline_drift: 0,
            applied_migration_count: 0,
            missing_migrations: Vec::new(),
        }
    }
}

/// 运行完整的 Schema 健康检查
///
/// # Arguments
/// * `pool` - 数据库连接池
/// * `auto_repair` - 是否自动修复缺失的索引
///
/// # Returns
/// * `HealthCheckResult` - 健康检查结果
pub async fn run_schema_health_check(
    pool: &Pool<Postgres>,
    auto_repair: bool,
) -> Result<HealthCheckResult, sqlx::Error> {
    let mut result = HealthCheckResult::default();

    info!("Starting database schema health check...");

    // 1. 检查核心表（DB-02：从 baseline 自动推导，覆盖全部 ~200+ 张表）
    let expected_tables: &[&str] = baseline_tables();
    info!(expected_table_count = expected_tables.len(), "validating tables from baseline schema");
    result.missing_tables = check_missing_tables(pool, expected_tables).await?;
    if !result.missing_tables.is_empty() {
        result.passed = false;
        error!("Missing tables: {:?}", result.missing_tables);
    }

    // 2. 检查核心字段
    result.missing_columns = check_missing_columns(pool, CORE_COLUMNS).await?;
    if !result.missing_columns.is_empty() {
        result.passed = false;
        error!("Missing columns: {:?}", result.missing_columns);
    }

    // 3. 检查必需索引
    result.missing_indexes = check_missing_indexes(pool, REQUIRED_INDEXES).await?;
    if !result.missing_indexes.is_empty() {
        warn!("Missing indexes: {:?}", result.missing_indexes);
        result.warnings.push(AUTO_REPAIR_DISABLED_MESSAGE.to_string());

        if auto_repair {
            warn!(
                "Runtime schema index repair requested but disabled; use docker/db_migrate.sh to apply managed migrations"
            );
        }
    }

    // 4. 检查字段命名一致性（警告）
    let mut naming_issues = check_field_naming_issues(pool).await?;
    result.warnings.append(&mut naming_issues);

    // 5. DB-02 新增：迁移完整性检查
    match check_migration_completeness(pool).await {
        Ok((applied, missing)) => {
            result.applied_migration_count = applied;
            result.missing_migrations = missing.clone();
            if !missing.is_empty() {
                result.passed = false;
                let preview_count = missing.len().min(10);
                error!(missing_count = missing.len(), "Missing sqlx migrations: {:?}", &missing[..preview_count]);
            }
        }
        Err(e) => {
            let msg = format!("Could not check _sqlx_migrations table: {e}");
            error!("{}", msg);
            result.warnings.push(msg);
        }
    }

    // 6. DB-02 新增：Drift 警告
    let actual_table_count = count_public_tables(pool).await?;
    let drift = actual_table_count as i64 - expected_tables.len() as i64;
    result.baseline_drift = drift;
    if drift.abs() > 10 {
        let msg = format!(
            "Baseline drift detected: baseline expects {} tables, database has {} (drift = {}). Investigate before trusting this report.",
            expected_tables.len(),
            actual_table_count,
            drift
        );
        warn!("{}", msg);
        result.warnings.push(msg);
    }

    if !result.warnings.is_empty() {
        warn!("Schema warnings (non-critical): {} item(s)", result.warnings.len());
    }

    if result.passed {
        info!(
            tables = expected_tables.len(),
            migrations = result.applied_migration_count,
            drift = result.baseline_drift,
            "Schema health check PASSED"
        );
    } else {
        error!("Schema health check FAILED");
    }

    Ok(result)
}

/// 检查缺失的表
/// C-4: 批量查询——此前每张表一条 SELECT，30+ 张表产生 30+ 次 DB 往返。
/// 现在用 ANY($1) 一次性查出存在的表，在 Rust 侧做差集。
async fn check_missing_tables(pool: &Pool<Postgres>, expected_tables: &[&str]) -> Result<Vec<String>, sqlx::Error> {
    let existing: Vec<String> = sqlx::query_scalar(
        "SELECT table_name FROM information_schema.tables WHERE table_schema = current_schema() AND table_name = ANY($1)",
    )
    .bind(expected_tables)
    .fetch_all(pool)
    .await?;

    let existing_set: std::collections::HashSet<&str> = existing.iter().map(|s| s.as_str()).collect();
    let missing: Vec<String> =
        expected_tables.iter().filter(|t| !existing_set.contains(*t)).map(|s| s.to_string()).collect();

    Ok(missing)
}

/// 检查缺失的字段
/// C-4: 批量查询——此前每个 (table, column) 对一条 SELECT，100+ 对产生 100+ 次 DB 往返。
/// 现在用 unnest 一次性查出所有存在的列，在 Rust 侧做差集。
async fn check_missing_columns(
    pool: &Pool<Postgres>,
    expected_columns: &[(&str, &str)],
) -> Result<Vec<String>, sqlx::Error> {
    let tables: Vec<&str> = expected_columns.iter().map(|(t, _)| *t).collect();
    let columns: Vec<&str> = expected_columns.iter().map(|(_, c)| *c).collect();

    let existing: Vec<(String, String)> = sqlx::query_as(
        "SELECT t.tbl, t.col FROM unnest($1::text[], $2::text[]) AS t(tbl, col) \
         JOIN information_schema.columns c ON c.table_schema = current_schema() \
         AND c.table_name = t.tbl AND c.column_name = t.col",
    )
    .bind(&tables)
    .bind(&columns)
    .fetch_all(pool)
    .await?;

    let existing_set: std::collections::HashSet<(String, String)> = existing.into_iter().collect();
    let missing: Vec<String> = expected_columns
        .iter()
        .filter(|(t, c)| !existing_set.contains(&(t.to_string(), c.to_string())))
        .map(|(t, c)| format!("{t}.{c}"))
        .collect();

    Ok(missing)
}

/// 检查缺失的索引
/// C-4: 批量查询——此前每组索引一条 SELECT。现在收集所有可接受名称
/// 一次性查询，在 Rust 侧判断每组是否有至少一个匹配。
async fn check_missing_indexes(
    pool: &Pool<Postgres>,
    expected_indexes: &[RequiredIndex],
) -> Result<Vec<String>, sqlx::Error> {
    // Collect all acceptable index names across all groups
    let all_names: Vec<&str> = expected_indexes.iter().flat_map(|e| e.acceptable_names.iter().copied()).collect();

    let existing: Vec<String> = sqlx::query_scalar(
        "SELECT indexname FROM pg_indexes WHERE schemaname = current_schema() AND indexname = ANY($1)",
    )
    .bind(&all_names)
    .fetch_all(pool)
    .await?;

    let existing_set: std::collections::HashSet<&str> = existing.iter().map(|s| s.as_str()).collect();
    let missing: Vec<String> = expected_indexes
        .iter()
        .filter(|e| !e.acceptable_names.iter().any(|name| existing_set.contains(*name)))
        .map(|e| e.display_name.to_string())
        .collect();

    Ok(missing)
}

/// 检查字段命名问题（警告级别）
async fn check_field_naming_issues(pool: &Pool<Postgres>) -> Result<Vec<String>, sqlx::Error> {
    let mut issues = Vec::new();

    // 检查 user_threepids 的旧字段名 (已修复，检查新字段是否存在)
    let has_validated_at: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM information_schema.columns WHERE table_schema = current_schema() AND table_name = 'user_threepids' AND column_name = 'validated_at'"
    )
    .fetch_one(pool)
    .await?;

    if has_validated_at == 0 {
        issues.push("user_threepids.validated_at - field missing (should be migrated from validated_ts)".to_string());
    }

    Ok(issues)
}

/// 快速验证（不自动修复）
pub async fn quick_validate(pool: &Pool<Postgres>) -> Result<bool, sqlx::Error> {
    let result = run_schema_health_check(pool, false).await?;
    Ok(result.passed)
}

/// 详细验证报告
pub async fn detailed_report(pool: &Pool<Postgres>) -> Result<String, sqlx::Error> {
    let result = run_schema_health_check(pool, false).await?;

    let mut report = String::new();
    report.push_str("# Database Schema Health Report\n\n");

    report.push_str(&format!("## Status: {}\n\n", if result.passed { "PASSED" } else { "FAILED" }));

    report.push_str("## Metrics (DB-02)\n\n");
    report.push_str(&format!(
        "| Metric | Value |\n|--------|-------|\n| Baseline table count | {} |\n| Actual table count | {} |\n| Baseline drift | {} |\n| Missing tables | {} |\n| Missing columns | {} |\n| Missing indexes | {} |\n| Applied migrations | {} |\n| Missing migrations | {} |\n\n",
        crate::baseline_tables::baseline_table_count(),
        crate::baseline_tables::baseline_table_count() as i64 + result.baseline_drift,
        result.baseline_drift,
        result.missing_tables.len(),
        result.missing_columns.len(),
        result.missing_indexes.len(),
        result.applied_migration_count,
        result.missing_migrations.len()
    ));

    if !result.missing_tables.is_empty() {
        report.push_str("## Missing Tables\n");
        for table in &result.missing_tables {
            report.push_str(&format!("- {table}\n"));
        }
        report.push('\n');
    }

    if !result.missing_columns.is_empty() {
        report.push_str("## Missing Columns\n");
        for col in &result.missing_columns {
            report.push_str(&format!("- {col}\n"));
        }
        report.push('\n');
    }

    if !result.missing_indexes.is_empty() {
        report.push_str("## Missing Indexes\n");
        for idx in &result.missing_indexes {
            report.push_str(&format!("- {idx}\n"));
        }
        report.push('\n');
    }

    if !result.missing_migrations.is_empty() {
        report.push_str("## Missing Migrations\n");
        for v in &result.missing_migrations {
            report.push_str(&format!("- {v}\n"));
        }
        report.push('\n');
    }

    if !result.warnings.is_empty() {
        report.push_str("## Warnings\n");
        for warn in &result.warnings {
            report.push_str(&format!("- {warn}\n"));
        }
        report.push('\n');
    }

    Ok(report)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_health_check_result_default() {
        let result = HealthCheckResult::default();
        assert!(result.passed);
        assert!(result.missing_tables.is_empty());
    }

    #[test]
    fn test_health_check_result_with_issues() {
        let result = HealthCheckResult {
            passed: false,
            missing_tables: vec!["users".to_string()],
            missing_columns: vec!["events.room_id".to_string()],
            missing_indexes: vec!["idx_events_room".to_string()],
            repaired_indexes: vec![],
            warnings: vec!["test warning".to_string()],
            baseline_drift: 3,
            applied_migration_count: 5,
            missing_migrations: vec![20240101000001],
        };

        assert!(!result.passed);
        assert_eq!(result.missing_tables.len(), 1);
        assert_eq!(result.baseline_drift, 3);
        assert_eq!(result.applied_migration_count, 5);
        assert_eq!(result.missing_migrations, vec![20240101000001]);
    }

    #[test]
    fn test_core_columns_defined() {
        assert!(CORE_COLUMNS.iter().any(|(t, c)| *t == "users" && *c == "user_id"));
        assert!(CORE_COLUMNS.iter().any(|(t, c)| *t == "events" && *c == "room_id"));
        assert!(CORE_COLUMNS.iter().any(|(t, c)| *t == "background_updates" && *c == "retry_count"));
        assert!(CORE_COLUMNS.iter().any(|(t, c)| *t == "room_retention_policies" && *c == "is_server_default"));
    }

    /// DB-02：核心表现在从 baseline schema 自动推导。
    /// 旧测试 `test_core_tables_defined` 检查 CORE_TABLES 常量，已被移除。
    /// 新测试验证 baseline 解析正确覆盖了 v10 的关键表。
    #[test]
    fn test_baseline_covers_known_core_tables() {
        let tables = baseline_tables();
        // 这些表在 v10 baseline 中必须出现。
        for required in &["users", "rooms", "events", "background_updates", "room_retention_policies"] {
            assert!(tables.contains(required), "baseline must contain '{required}' but it does not");
        }
    }

    /// 防复发守卫：`REQUIRED_INDEXES` 的每一组都必须至少有一个名字是 baseline
    /// 真实创建的索引（`CREATE INDEX` 或具名 UNIQUE/PK 约束索引）。
    ///
    /// 否则每个全新库启动时都会对不存在的索引打印虚假的 `Missing indexes`
    /// 警告（DB_REVIEW §14.5：`idx_events_room_id` 曾如此；同批发现的还有
    /// `idx_user_threepids_medium_address`，其真实名字是约束生成的
    /// `uq_user_threepids_medium_address`）。
    #[test]
    fn required_indexes_are_created_by_baseline() {
        let baseline = crate::baseline_tables::baseline_index_names();
        let unsatisfied: Vec<&str> = REQUIRED_INDEXES
            .iter()
            .filter(|required| !required.acceptable_names.iter().any(|name| baseline.contains(name)))
            .map(|required| required.display_name)
            .collect();
        assert!(
            unsatisfied.is_empty(),
            "REQUIRED_INDEXES lists indexes the baseline never creates (spurious startup warnings): {unsatisfied:?}"
        );
    }
}
