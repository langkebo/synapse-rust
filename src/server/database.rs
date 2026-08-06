use sqlx::postgres::PgPoolOptions;
use sqlx::PgPool;
use std::sync::Arc;
use std::time::Duration;

use crate::common::config::Config;
use synapse_services::database_initializer::DatabaseInitService;
use synapse_storage::schema_health_check::run_schema_health_check;

const DEFAULT_MAX_LIFETIME: Duration = Duration::from_secs(1800);
const DEFAULT_IDLE_TIMEOUT: Duration = Duration::from_secs(600);

/// Minimum idle database connections maintained in the connection pool.
const DB_MIN_IDLE_CONNECTIONS: u32 = 5;

/// Database session timeout SQL queries.
const DB_SET_STATEMENT_TIMEOUT: &str = "SET statement_timeout = '30s'";
const DB_SET_LOCK_TIMEOUT: &str = "SET lock_timeout = '10s'";
const DB_SET_IDLE_TIMEOUT: &str = "SET idle_in_transaction_session_timeout = '60s'";

pub async fn build_database_pool(config: &Config) -> Result<PgPool, Box<dyn std::error::Error>> {
    let pool_options = PgPoolOptions::new()
        .max_connections(config.database.max_size)
        .min_connections(config.database.min_idle.unwrap_or(DB_MIN_IDLE_CONNECTIONS))
        .acquire_timeout(Duration::from_secs(config.database.connection_timeout))
        .max_lifetime(DEFAULT_MAX_LIFETIME)
        .idle_timeout(DEFAULT_IDLE_TIMEOUT)
        .after_connect(|conn, _meta| {
            Box::pin(async move {
                sqlx::query(DB_SET_STATEMENT_TIMEOUT).execute(&mut *conn).await?;
                sqlx::query(DB_SET_LOCK_TIMEOUT).execute(&mut *conn).await?;
                sqlx::query(DB_SET_IDLE_TIMEOUT).execute(&mut *conn).await?;
                Ok(())
            })
        })
        .test_before_acquire(false);

    ::tracing::info!("[启动阶段 1/4] 连接数据库 (pool: max={}, min_idle={:?}, timeout={}s)",
        config.database.max_size, config.database.min_idle, config.database.connection_timeout);

    let database_url = config.database_url();
    let pool = pool_options.connect(&database_url).await?;

    // 记录 PG 服务端版本, 便于排查兼容性问题 (如 PG14 以下不支持某些 SQL 语法)
    let pg_version: Option<String> = sqlx::query_scalar("SELECT version()")
        .fetch_optional(&pool)
        .await
        .ok()
        .flatten();
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
