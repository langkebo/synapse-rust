//! 数据库 Schema 健康检查模块
//!
//! 提供启动时的 Schema 验证，不在服务启动阶段执行运行时索引修复。
//!
//! 使用方法:
//! ```text
//! use synapse_rust::storage::schema_health_check::run_schema_health_check;
//!
//! #[tokio::main]
//! async fn main() -> Result<(), Box<dyn std::error::Error>> {
//!     let pool = create_pool().await?;
//!     run_schema_health_check(&pool, false).await?;
//!     Ok(())
//! }
//! ```

use sqlx::{Pool, Postgres};
use tracing::{error, info, warn};

const AUTO_REPAIR_DISABLED_MESSAGE: &str =
    "Schema health check detected missing indexes. Apply the managed migrations via docker/db_migrate.sh instead of repairing schema at runtime.";

/// 核心表定义
const CORE_TABLES: &[&str] = &[
    "users",
    "devices",
    "rooms",
    "room_aliases",
    "events",
    "event_relations",
    "room_memberships",
    "access_tokens",
    "refresh_tokens",
    "user_threepids",
    "presence",
    "user_directory",
    "federation_signing_keys",
    "rate_limits",
    "report_rate_limits",
    "server_notices",
    "user_notification_settings",
    "widgets",
    "secure_key_backups",
    "secure_backup_session_keys",
];

/// 核心字段定义 (表名, 字段名)
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
    ("user_threepids", "validated_ts"),
    ("user_threepids", "verification_expires_ts"),
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
    ("user_notification_settings", "enabled"),
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
];

/// 必需索引定义 (索引名, 表名, 字段, 条件索引的 WHERE 子句)
const REQUIRED_INDEXES: &[(&str, &str, &str, Option<&str>)] = &[
    // events 表索引
    ("idx_events_room_id", "events", "room_id", None),
    ("idx_events_user_id", "events", "user_id", None),
    (
        "idx_events_origin_server_ts",
        "events",
        "origin_server_ts",
        None,
    ),
    (
        "idx_events_room_time",
        "events",
        "room_id, origin_server_ts",
        None,
    ),
    // room_memberships 表索引
    (
        "idx_memberships_user_room",
        "room_memberships",
        "user_id, room_id",
        None,
    ),
    (
        "idx_memberships_user_membership",
        "room_memberships",
        "user_id, membership",
        Some("membership = 'join'"),
    ),
    (
        "idx_memberships_room_user",
        "room_memberships",
        "room_id, user_id",
        None,
    ),
    // users 表索引
    ("idx_users_username", "users", "username", None),
    ("idx_users_created_ts", "users", "created_ts", None),
    // devices 表索引
    ("idx_devices_user_id", "devices", "user_id", None),
    // presence 表索引
    (
        "idx_presence_user_status",
        "presence",
        "user_id, presence",
        None,
    ),
    // access_tokens 表索引
    ("idx_access_tokens_user", "access_tokens", "user_id", None),
    (
        "idx_access_tokens_token_hash",
        "access_tokens",
        "token_hash",
        None,
    ),
    // refresh_tokens 表索引
    ("idx_refresh_tokens_user", "refresh_tokens", "user_id", None),
    // user_threepids 表索引
    ("idx_user_threepids_user", "user_threepids", "user_id", None),
    (
        "idx_user_threepids_medium_address",
        "user_threepids",
        "medium, address",
        None,
    ),
];

/// 健康检查结果
#[derive(Debug)]
pub struct HealthCheckResult {
    pub passed: bool,
    pub missing_tables: Vec<String>,
    pub missing_columns: Vec<String>,
    pub missing_indexes: Vec<String>,
    pub repaired_indexes: Vec<String>,
    pub warnings: Vec<String>,
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

    // 1. 检查核心表
    result.missing_tables = check_missing_tables(pool, CORE_TABLES).await?;
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
        result
            .warnings
            .push(AUTO_REPAIR_DISABLED_MESSAGE.to_string());

        if auto_repair {
            warn!(
                "Runtime schema index repair requested but disabled; use docker/db_migrate.sh to apply managed migrations"
            );
        }
    }

    // 4. 检查字段命名一致性（警告）
    result.warnings = check_field_naming_issues(pool).await?;
    if !result.warnings.is_empty() {
        warn!("Field naming issues found: {:?}", result.warnings);
    }

    // 输出结果
    if result.passed {
        info!("✅ Schema health check PASSED");
    } else {
        error!("❌ Schema health check FAILED");
    }

    Ok(result)
}

/// 检查缺失的表
async fn check_missing_tables(
    pool: &Pool<Postgres>,
    expected_tables: &[&str],
) -> Result<Vec<String>, sqlx::Error> {
    let mut missing = Vec::new();

    for table in expected_tables {
        let count: i64 = sqlx::query_scalar(
            "SELECT COUNT(*) FROM information_schema.tables WHERE table_name = $1 AND table_schema = 'public'"
        )
        .bind(table)
        .fetch_one(pool)
        .await?;

        if count == 0 {
            missing.push(table.to_string());
        }
    }

    Ok(missing)
}

/// 检查缺失的字段
async fn check_missing_columns(
    pool: &Pool<Postgres>,
    expected_columns: &[(&str, &str)],
) -> Result<Vec<String>, sqlx::Error> {
    let mut missing = Vec::new();

    for (table, column) in expected_columns {
        let count: i64 = sqlx::query_scalar(
            "SELECT COUNT(*) FROM information_schema.columns WHERE table_name = $1 AND column_name = $2 AND table_schema = 'public'"
        )
        .bind(table)
        .bind(column)
        .fetch_one(pool)
        .await?;

        if count == 0 {
            missing.push(format!("{}.{}", table, column));
        }
    }

    Ok(missing)
}

/// 检查缺失的索引
async fn check_missing_indexes(
    pool: &Pool<Postgres>,
    expected_indexes: &[(&str, &str, &str, Option<&str>)],
) -> Result<Vec<String>, sqlx::Error> {
    let mut missing = Vec::new();

    for (index_name, _table, _columns, _where) in expected_indexes {
        let count: i64 = sqlx::query_scalar(
            "SELECT COUNT(*) FROM pg_indexes WHERE indexname = $1 AND schemaname = 'public'",
        )
        .bind(index_name)
        .fetch_one(pool)
        .await?;

        if count == 0 {
            missing.push(index_name.to_string());
        }
    }

    Ok(missing)
}

/// 检查字段命名问题（警告级别）
async fn check_field_naming_issues(pool: &Pool<Postgres>) -> Result<Vec<String>, sqlx::Error> {
    let mut issues = Vec::new();

    // 检查 user_threepids 的旧字段名 (已修复，检查新字段是否存在)
    let has_validated_ts: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM information_schema.columns WHERE table_name = 'user_threepids' AND column_name = 'validated_ts'"
    )
    .fetch_one(pool)
    .await?;

    if has_validated_ts == 0 {
        issues.push("user_threepids.validated_ts - field missing (should be migrated)".to_string());
    }

    // 检查 private_messages 的新字段名
    let has_read_ts: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM information_schema.columns WHERE table_name = 'private_messages' AND column_name = 'read_ts'"
    )
    .fetch_one(pool)
    .await?;

    if has_read_ts == 0 {
        issues.push("private_messages.read_ts - field missing (should be migrated)".to_string());
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

    report.push_str(&format!(
        "## Status: {}\n\n",
        if result.passed {
            "✅ PASSED"
        } else {
            "❌ FAILED"
        }
    ));

    if !result.missing_tables.is_empty() {
        report.push_str("## Missing Tables\n");
        for table in &result.missing_tables {
            report.push_str(&format!("- {}\n", table));
        }
        report.push('\n');
    }

    if !result.missing_columns.is_empty() {
        report.push_str("## Missing Columns\n");
        for col in &result.missing_columns {
            report.push_str(&format!("- {}\n", col));
        }
        report.push('\n');
    }

    if !result.missing_indexes.is_empty() {
        report.push_str("## Missing Indexes\n");
        for idx in &result.missing_indexes {
            report.push_str(&format!("- {}\n", idx));
        }
        report.push('\n');
    }

    if !result.warnings.is_empty() {
        report.push_str("## Warnings\n");
        for warn in &result.warnings {
            report.push_str(&format!("- {}\n", warn));
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
            warnings: vec![],
        };

        assert!(!result.passed);
        assert_eq!(result.missing_tables.len(), 1);
    }

    #[test]
    fn test_core_tables_defined() {
        assert!(CORE_TABLES.contains(&"users"));
        assert!(CORE_TABLES.contains(&"rooms"));
        assert!(CORE_TABLES.contains(&"events"));
    }

    #[test]
    fn test_core_columns_defined() {
        assert!(CORE_COLUMNS
            .iter()
            .any(|(t, c)| *t == "users" && *c == "user_id"));
        assert!(CORE_COLUMNS
            .iter()
            .any(|(t, c)| *t == "events" && *c == "room_id"));
    }
}
