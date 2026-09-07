use sqlx::postgres::PgPoolOptions;
use sqlx::PgPool;
use std::sync::Arc;
use std::time::Duration;

use crate::common::config::Config;
use synapse_services::database_initializer::DatabaseInitService;
use synapse_storage::schema_health_check::run_schema_health_check;

/// 格式化 PG 超时语句，统一输出 PG 接受的 `'30s'` 形式。
///
/// PG 接受 `SET ... = <int>ms` 或 `'<int>{ms|s|min}'`；本项目所有超时都按秒配置，
/// 显式带单位避免歧义（`'0'` 会被解析为毫秒）。
fn format_pg_timeout(seconds: u64) -> String {
    format!("'{seconds}s'")
}

/// See [`build_database_pool`].
pub async fn build_database_pool(config: &Config) -> Result<PgPool, Box<dyn std::error::Error>> {
    let db_cfg = &config.database;
    let statement_timeout_sql = format!("SET statement_timeout = {}", format_pg_timeout(db_cfg.statement_timeout_secs));
    let lock_timeout_sql = format!("SET lock_timeout = {}", format_pg_timeout(db_cfg.lock_timeout_secs));
    let idle_in_tx_timeout_sql = format!(
        "SET idle_in_transaction_session_timeout = {}",
        format_pg_timeout(db_cfg.idle_in_transaction_timeout_secs)
    );

    let min_idle = db_cfg.min_idle.unwrap_or(db_cfg.min_idle_floor);
    let max_lifetime = Duration::from_secs(db_cfg.max_lifetime_secs);
    let idle_timeout = Duration::from_secs(db_cfg.idle_timeout_secs);

    let pool_options = PgPoolOptions::new()
        .max_connections(db_cfg.max_size)
        .min_connections(min_idle)
        .acquire_timeout(Duration::from_secs(db_cfg.connection_timeout))
        .max_lifetime(max_lifetime)
        .idle_timeout(idle_timeout)
        .after_connect(move |conn, _meta| {
            let statement_timeout_sql = statement_timeout_sql.clone();
            let lock_timeout_sql = lock_timeout_sql.clone();
            let idle_in_tx_timeout_sql = idle_in_tx_timeout_sql.clone();
            Box::pin(async move {
                sqlx::query(&statement_timeout_sql).execute(&mut *conn).await?;
                sqlx::query(&lock_timeout_sql).execute(&mut *conn).await?;
                sqlx::query(&idle_in_tx_timeout_sql).execute(&mut *conn).await?;
                Ok(())
            })
        })
        .test_before_acquire(false);

    ::tracing::info!(
        "[启动阶段 1/4] 连接数据库 (pool: max={}, min_idle={}, timeout={}s, max_lifetime={}s, idle_timeout={}s)",
        db_cfg.max_size,
        min_idle,
        db_cfg.connection_timeout,
        db_cfg.max_lifetime_secs,
        db_cfg.idle_timeout_secs
    );
    ::tracing::info!(
        "[启动阶段 1/4] PG session 超时: statement_timeout={}s, lock_timeout={}s, idle_in_transaction={}s",
        db_cfg.statement_timeout_secs,
        db_cfg.lock_timeout_secs,
        db_cfg.idle_in_transaction_timeout_secs
    );

    let database_url = config.database_url();
    let pool = pool_options.connect(&database_url).await?;

    // 记录 PG 服务端版本, 便于排查兼容性问题 (如 PG14 以下不支持某些 SQL 语法)
    let pg_version: Option<String> = sqlx::query_scalar("SELECT version()").fetch_optional(&pool).await.ok().flatten();
    ::tracing::info!("[启动阶段 1/4] 数据库连接建立: {}", pg_version.as_deref().unwrap_or("unknown"));
    let pool = Arc::new(pool);

    // 先执行运行时数据库初始化，确保所有表存在
    let runtime_db_init_enabled = std::env::var("SYNAPSE_ENABLE_RUNTIME_DB_INIT")
        .map(|value| matches!(value.as_str(), "1" | "true" | "TRUE" | "yes" | "YES"))
        .unwrap_or(false);
    let skip_db_init = std::env::var("SYNAPSE_SKIP_DB_INIT")
        .map(|value| matches!(value.as_str(), "1" | "true" | "TRUE" | "yes" | "YES"))
        .unwrap_or(false);
    let migrations_dir = if std::path::Path::new("/app/migrations").exists() {
        "/app/migrations"
    } else if std::path::Path::new("./migrations").exists() {
        "./migrations"
    } else {
        "(none)"
    };
    if !runtime_db_init_enabled || skip_db_init {
        ::tracing::info!(
            "[启动阶段 1/4] 运行时数据库初始化已禁用 (SYNAPSE_ENABLE_RUNTIME_DB_INIT={}, SYNAPSE_SKIP_DB_INIT={}); 迁移主链: docker/db_migrate.sh + db-migration-gate.yml, 迁移目录: {}",
            runtime_db_init_enabled, skip_db_init, migrations_dir
        );
    } else {
        ::tracing::info!("[启动阶段 1/4] 开始运行时数据库初始化 (迁移目录: {})", migrations_dir);
        let db_init_service = DatabaseInitService::new(pool.clone());
        db_init_service.initialize().await?;
        ::tracing::info!("[启动阶段 1/4] 运行时数据库初始化完成");
    }

    // 运行数据库 Schema 健康检查（在运行时初始化之后）
    let skip_schema_check = std::env::var("SYNAPSE_SKIP_SCHEMA_CHECK").unwrap_or_default().to_lowercase() == "true";

    if skip_schema_check {
        ::tracing::warn!("[启动阶段 1/4] ⚠️  跳过数据库 schema 健康检查 (SYNAPSE_SKIP_SCHEMA_CHECK=true)");
    } else {
        ::tracing::info!("[启动阶段 1/4] 开始数据库 schema 健康检查...");
        match run_schema_health_check(&pool, false).await {
            Ok(result) => {
                if result.passed {
                    ::tracing::info!("[启动阶段 1/4] ✅ 数据库 schema 校验通过");
                } else {
                    ::tracing::error!("[启动阶段 1/4] ❌ 数据库 schema 校验失败");
                    if !result.missing_tables.is_empty() {
                        ::tracing::error!("  Missing tables: {:?}", result.missing_tables);
                    }
                    if !result.missing_columns.is_empty() {
                        ::tracing::error!("  Missing columns: {:?}", result.missing_columns);
                    }
                    if !result.repaired_indexes.is_empty() {
                        ::tracing::info!("  Repaired indexes: {:?}", result.repaired_indexes);
                    }
                    // 如果有严重问题（缺少表或列），给出可执行的修复指引后退出
                    if !result.missing_tables.is_empty() || !result.missing_columns.is_empty() {
                        ::tracing::error!(
                            "💡 To fix: run pending migrations against your database, e.g.\n   \
                             DATABASE_URL=\"postgresql://USER:PASS@HOST:PORT/DBNAME\" \\\n   \
                             bash docker/db_migrate.sh migrate\n   \
                             If you understand the risk and want to start anyway, set \
                             SYNAPSE_SKIP_SCHEMA_CHECK=true (NOT recommended for production)."
                        );
                        return Err("Database schema validation failed: missing critical tables or columns. \
                             Run `docker/db_migrate.sh migrate` against the configured database \
                             (or set SYNAPSE_SKIP_SCHEMA_CHECK=true to bypass this check)."
                            .into());
                    }
                }
                if !result.warnings.is_empty() {
                    ::tracing::warn!("Schema warnings (non-critical): {:?}", result.warnings);
                }
            }
            Err(e) => {
                ::tracing::error!("[启动阶段 1/4] 数据库 schema 健康检查执行失败: {}", e);
                // Schema health check itself failed — this is NOT safe to ignore.
                // We might be running against a half-migrated schema.
                return Err(format!(
                    "Database schema health check failed to execute: {e}. \
                     This may indicate a connectivity issue or a corrupt migration state. \
                     Fix the database connection or set SYNAPSE_SKIP_SCHEMA_CHECK=true \
                     to bypass this check (NOT recommended for production)."
                )
                .into());
            }
        }
    }

    // Drop the Arc wrapper and return the inner PgPool.
    // The db_init_service has already been dropped, so there should be
    // exactly one reference to the Arc.
    match Arc::try_unwrap(pool) {
        Ok(p) => Ok(p),
        Err(arc) => {
            // Fallback: if for some reason there are still outstanding
            // references, clone the underlying pool (PgPool::clone is cheap).
            Ok((*arc).clone())
        }
    }
}
