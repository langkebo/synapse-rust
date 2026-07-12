pub mod models;
pub mod tables;
pub use models::{initialize_database, DatabaseInitMode, DatabaseInitService, Environment, InitializationReport};

use sqlx::PgPool;
use std::sync::Arc;
use synapse_storage::SchemaValidator;
use tracing::{debug, error, info, warn};

const DEFAULT_CACHE_TTL_SECONDS: i64 = 3600;
const RUNTIME_DB_INIT_ENV: &str = "SYNAPSE_ENABLE_RUNTIME_DB_INIT";

impl DatabaseInitService {
    /// Creates a new DatabaseInitService with default cache TTL (1 hour).
    ///
    /// # Arguments
    /// * `pool` - Shared PostgreSQL connection pool
    ///
    /// # Returns
    /// A new DatabaseInitService instance
    pub fn new(pool: Arc<PgPool>) -> Self {
        Self {
            pool: pool.clone(),
            schema_validator: SchemaValidator::new(pool),
            cache_ttl_seconds: DEFAULT_CACHE_TTL_SECONDS,
            environment: Environment::from_env(),
            mode: DatabaseInitMode::Auto,
        }
    }

    /// Creates a DatabaseInitService with custom cache TTL.
    ///
    /// # Arguments
    /// * `pool` - Shared PostgreSQL connection pool
    /// * `ttl_seconds` - Cache time-to-live in seconds
    ///
    /// # Returns
    /// A new DatabaseInitService instance with specified cache TTL
    pub fn with_cache_ttl(pool: Arc<PgPool>, ttl_seconds: i64) -> Self {
        Self {
            pool: pool.clone(),
            schema_validator: SchemaValidator::new(pool),
            cache_ttl_seconds: ttl_seconds,
            environment: Environment::from_env(),
            mode: DatabaseInitMode::Auto,
        }
    }

    pub fn with_mode(mut self, mode: DatabaseInitMode) -> Self {
        self.mode = mode;
        self
    }

    /// Initializes the database by running connection test, schema validation,
    /// and index creation. Results are cached based on environment settings.
    ///
    /// # Returns
    /// Result containing InitializationReport with steps, errors, and repair actions
    ///
    /// # Errors
    /// Returns sqlx::Error if database operations fail
    pub async fn initialize(&self) -> Result<InitializationReport, sqlx::Error> {
        let mut report = InitializationReport {
            is_success: true,
            steps: Vec::new(),
            errors: Vec::new(),
            schema_status: None,
            repairs_performed: Vec::new(),
            skipped: false,
        };

        info!(mode = ?self.mode, environment = ?self.environment, "开始数据库初始化流程");

        match self.step_connection_test().await {
            Ok(msg) => {
                info!(step = %"connection_test", result = %msg, "数据库初始化步骤完成");
                report.steps.push(msg);
            }
            Err(e) => {
                report.is_success = false;
                report.errors.push(format!("连接测试失败: {e}"));
                return Ok(report);
            }
        }

        let Some(mode) = self.resolved_mode() else {
            let message = format!(
                "运行时数据库初始化默认已禁用；请使用 docker/db_migrate.sh 与 db-migration-gate.yml 作为迁移主链，如需兼容启用请设置 {RUNTIME_DB_INIT_ENV}=true"
            );
            info!(runtime_db_init_enabled = false, message = %message, "跳过运行时数据库初始化");
            report.steps.push(message);
            report.skipped = true;
            return Ok(report);
        };

        info!(mode = ?mode, "开始执行数据库迁移");
        match self.step_migrations().await {
            Ok(msg) => {
                info!(step = %"migrations", result = %msg, "数据库初始化步骤完成");
                report.steps.push(msg);
            }
            Err(e) => {
                error!(error = %e, mode = ?mode, "数据库迁移失败");
                report.is_success = false;
                report.errors.push(format!("数据库迁移失败: {e}"));
                return Ok(report);
            }
        }

        if mode == DatabaseInitMode::Strict {
            let message = "严格模式仅执行 migrations/*.sql；运行时 DDL 已禁用并由迁移链路兜底".to_string();
            info!(mode = ?mode, message = %message, "数据库初始化严格模式已完成");
            report.steps.push(message);
            return Ok(report);
        }

        let should_skip_validation = self.check_cache_valid().await?;
        if should_skip_validation {
            debug!("数据库初始化缓存有效，跳过详细验证");
            report.steps.push("使用缓存，跳过验证".to_string());
            report.skipped = true;
            if self.environment.is_development() {
                info!(used_cache = true, success = true, "数据库初始化完成");
            } else {
                debug!("数据库初始化完成 (使用缓存): success=true");
            }
            return Ok(report);
        }

        match self.step_schema_validation().await {
            Ok(status) => report.schema_status = Some(status),
            Err(e) => {
                report.is_success = false;
                report.errors.push(format!("Schema验证失败: {e}"));
            }
        };

        match self.step_index_validation().await {
            Ok(issues) => {
                if !issues.is_empty() {
                    if self.environment.is_development() {
                        info!(missing_index_count = issues.len(), "发现缺失的索引");
                    } else {
                        debug!("发现 {} 个缺失的索引", issues.len());
                    }
                }
            }
            Err(e) => report.errors.push(format!("索引验证失败: {e}")),
        }

        #[cfg(feature = "runtime-ddl")]
        if mode == DatabaseInitMode::Compatible {
            match self.step_schema_repair().await {
                Ok(repaired) => {
                    if !repaired.is_empty() {
                        info!(repaired_column_count = repaired.len(), "运行时补齐了缺失列");
                        report.repairs_performed.extend(repaired);
                    }
                }
                Err(e) => {
                    report.is_success = false;
                    report.errors.push(format!("运行时列修复失败: {}", e));
                }
            }

            match self.step_create_indexes().await {
                Ok(created) => {
                    if !created.is_empty() {
                        info!(created_index_count = created.len(), "运行时补齐了缺失索引");
                        report.repairs_performed.extend(created);
                    }
                }
                Err(e) => {
                    report.is_success = false;
                    report.errors.push(format!("运行时索引修复失败: {}", e));
                }
            }

            match self.step_create_e2ee_tables().await {
                Ok(message) => report.steps.push(message),
                Err(e) => {
                    report.is_success = false;
                    report.errors.push(format!("运行时 E2EE 兼容表检查失败: {}", e));
                }
            }

            match self.step_create_e2ee_core_tables().await {
                Ok(message) => report.steps.push(message),
                Err(e) => {
                    report.is_success = false;
                    report.errors.push(format!("运行时 E2EE 核心表检查失败: {}", e));
                }
            }

            match self.step_ensure_additional_tables().await {
                Ok(message) => report.steps.push(message),
                Err(e) => {
                    report.is_success = false;
                    report.errors.push(format!("运行时附加表兼容检查失败: {}", e));
                }
            }

            match self.step_schema_validation().await {
                Ok(status) => report.schema_status = Some(status),
                Err(e) => {
                    report.is_success = false;
                    report.errors.push(format!("运行时修复后 Schema 复检失败: {}", e));
                }
            }
        }

        if report.is_success {
            self.update_init_timestamp().await?;
        }

        if self.environment.is_development() {
            info!(used_cache = false, success = report.is_success, "数据库初始化完成");
        } else {
            debug!("数据库初始化完成: success={}", report.is_success);
        }
        Ok(report)
    }

    fn runtime_db_init_enabled() -> bool {
        std::env::var(RUNTIME_DB_INIT_ENV)
            .map(|value| matches!(value.as_str(), "1" | "true" | "TRUE" | "yes" | "YES"))
            .unwrap_or(false)
    }

    fn resolved_mode(&self) -> Option<DatabaseInitMode> {
        match self.mode {
            DatabaseInitMode::Auto => Self::runtime_db_init_enabled().then_some(DatabaseInitMode::Compatible),
            DatabaseInitMode::Strict => Some(DatabaseInitMode::Strict),
            DatabaseInitMode::Compatible => Some(DatabaseInitMode::Compatible),
        }
    }

    async fn check_cache_valid(&self) -> Result<bool, sqlx::Error> {
        let result = sqlx::query_as::<_, (i64,)>(
            r"
            SELECT value::BIGINT
            FROM db_metadata WHERE key = 'last_init_ts'
            ",
        )
        .fetch_optional(&*self.pool)
        .await;

        match result {
            Ok(Some(row)) => {
                let last_init_ts: i64 = row.0;
                let now = chrono::Utc::now().timestamp();
                let elapsed = now - last_init_ts;

                if elapsed > 0 && elapsed < self.cache_ttl_seconds {
                    return Ok(true);
                }
            }
            Ok(None) => {
                debug!("db_metadata 表不存在或无缓存记录");
            }
            Err(e) => {
                debug!("检查缓存失败，继续执行初始化: {}", e);
            }
        }

        Ok(false)
    }

    async fn update_init_timestamp(&self) -> Result<(), sqlx::Error> {
        let now = chrono::Utc::now().timestamp();

        sqlx::query(
            r"
            INSERT INTO db_metadata (key, value, created_ts, updated_ts)
            VALUES ('last_init_ts', $1, $2, $2)
            ON CONFLICT (key) DO UPDATE SET
                value = EXCLUDED.value,
                updated_ts = EXCLUDED.updated_ts
            ",
        )
        .bind(now.to_string())
        .bind(now)
        .execute(&*self.pool)
        .await?;

        Ok(())
    }

    async fn step_connection_test(&self) -> Result<String, sqlx::Error> {
        sqlx::query("SELECT 1 as test").fetch_one(&*self.pool).await?;
        Ok("数据库连接测试通过".to_string())
    }

    async fn step_migrations(&self) -> Result<String, sqlx::Error> {
        info!("执行数据库迁移");

        self.ensure_schema_migrations_table().await?;

        let lock_key: i64 =
            sqlx::query_scalar("SELECT hashtext(current_database() || ':' || current_schema())::bigint")
                .fetch_one(&*self.pool)
                .await?;
        let mut lock_conn = self.pool.acquire().await?;
        let lock_start = std::time::Instant::now();
        loop {
            let locked: bool =
                sqlx::query_scalar("SELECT pg_try_advisory_lock($1)").bind(lock_key).fetch_one(&mut *lock_conn).await?;
            if locked {
                break;
            }
            if lock_start.elapsed() > std::time::Duration::from_secs(10) {
                return Err(sqlx::Error::Configuration(Box::new(std::io::Error::new(
                    std::io::ErrorKind::TimedOut,
                    format!("failed to acquire schema migration lock: {lock_key}"),
                ))));
            }
            tokio::time::sleep(std::time::Duration::from_millis(200)).await;
        }

        let migrations_dir = if std::path::Path::new("/app/migrations").exists() {
            std::path::Path::new("/app/migrations")
        } else if std::path::Path::new("./migrations").exists() {
            std::path::Path::new("./migrations")
        } else {
            info!("未找到迁移目录，跳过迁移");
            let _ = sqlx::query("SELECT pg_advisory_unlock($1)").bind(lock_key).execute(&mut *lock_conn).await;
            return Ok("数据库迁移跳过 (无迁移文件)".to_string());
        };

        info!(migrations_dir = ?migrations_dir, "使用运行时迁移文件");
        let result = self.run_runtime_migrations(migrations_dir).await;
        let _ = sqlx::query("SELECT pg_advisory_unlock($1)").bind(lock_key).execute(&mut *lock_conn).await;
        result
    }

    async fn ensure_schema_migrations_table(&self) -> Result<(), sqlx::Error> {
        let create_table_sql = r"
            CREATE TABLE IF NOT EXISTS schema_migrations (
                id BIGSERIAL PRIMARY KEY,
                version TEXT NOT NULL,
                name TEXT,
                checksum TEXT,
                applied_ts BIGINT,
                execution_time_ms BIGINT,
                is_success BOOLEAN NOT NULL DEFAULT TRUE,
                description TEXT,
                executed_at TIMESTAMPTZ DEFAULT NOW(),
                CONSTRAINT uq_schema_migrations_version UNIQUE (version)
            )
        ";

        sqlx::raw_sql(create_table_sql).execute(&*self.pool).await?;

        sqlx::query(
            r"
            CREATE INDEX IF NOT EXISTS idx_schema_migrations_version ON schema_migrations(version)
            ",
        )
        .execute(&*self.pool)
        .await?;

        info!(table_name = %"schema_migrations", "数据库迁移记录表已就绪");
        Ok(())
    }

    async fn is_migration_executed(&self, version: &str) -> Result<bool, sqlx::Error> {
        let result: Option<(bool,)> = sqlx::query_as("SELECT is_success FROM schema_migrations WHERE version = $1")
            .bind(version)
            .fetch_optional(&*self.pool)
            .await?;

        Ok(result.is_some_and(|(success,)| success))
    }

    async fn record_migration(
        &self,
        version: &str,
        checksum: &str,
        execution_time_ms: i64,
        success: bool,
    ) -> Result<(), sqlx::Error> {
        let now_ts = chrono::Utc::now().timestamp_millis();
        sqlx::query(
            r"INSERT INTO schema_migrations (version, checksum, applied_ts, execution_time_ms, is_success)
               VALUES ($1, $2, $3, $4, $5)
               ON CONFLICT (version) DO UPDATE SET
                   checksum = EXCLUDED.checksum,
                   applied_ts = EXCLUDED.applied_ts,
                   executed_at = NOW(),
                   execution_time_ms = EXCLUDED.execution_time_ms,
                   is_success = EXCLUDED.is_success",
        )
        .bind(version)
        .bind(checksum)
        .bind(now_ts)
        .bind(execution_time_ms)
        .bind(success)
        .execute(&*self.pool)
        .await?;

        Ok(())
    }

    fn calculate_checksum(content: &str) -> String {
        use std::collections::hash_map::DefaultHasher;
        use std::hash::{Hash, Hasher};

        let mut hasher = DefaultHasher::new();
        content.hash(&mut hasher);
        format!("{:016x}", hasher.finish())
    }

    async fn run_runtime_migrations(&self, migrations_dir: &std::path::Path) -> Result<String, sqlx::Error> {
        let mut migration_files: Vec<std::path::PathBuf> = std::fs::read_dir(migrations_dir)
            .map_err(|e| sqlx::Error::Configuration(e.to_string().into()))?
            .filter_map(|entry| entry.ok())
            .map(|entry| entry.path())
            .filter(|path| {
                path.extension().is_some_and(|ext| ext == "sql")
                    && path.file_name().and_then(|name| name.to_str()).is_some_and(|name| !name.ends_with(".undo.sql"))
            })
            .collect();

        migration_files.sort();

        info!(migration_file_count = migration_files.len(), "发现迁移文件");

        let mut success_count = 0;
        let mut skip_count = 0;
        let mut error_count = 0;

        for migration_file in migration_files {
            let filename = migration_file.file_name().and_then(|n| n.to_str()).unwrap_or("unknown");

            let version = filename.trim_end_matches(".sql");

            match self.is_migration_executed(version).await {
                Ok(true) => {
                    debug!("迁移 {} 已执行，跳过", filename);
                    skip_count += 1;
                    continue;
                }
                Ok(false) => {}
                Err(e) => {
                    warn!(error = %e, filename = %filename, version = %version, "检查迁移状态失败");
                }
            }

            let sql = match std::fs::read_to_string(&migration_file) {
                Ok(s) => s,
                Err(e) => {
                    warn!(error = %e, filename = %filename, version = %version, "无法读取迁移文件");
                    error_count += 1;
                    continue;
                }
            };

            let checksum = Self::calculate_checksum(&sql);
            let normalized_sql = Self::normalize_migration_sql(&sql);
            let start_time = std::time::Instant::now();
            let statements = Self::split_sql_statements(&normalized_sql);
            let mut file_success = true;
            let mut conn = self.pool.acquire().await?;

            for statement in statements {
                let trimmed = statement.trim();
                if trimmed.is_empty() || trimmed.starts_with("--") {
                    continue;
                }

                // Set statement timeout to 30 seconds to prevent indefinite hangs
                let timeout_result = sqlx::query("SET statement_timeout = '30s'").execute(&mut *conn).await;

                if let Err(e) = timeout_result {
                    debug!("无法设置 statement_timeout: {}", e);
                }

                match sqlx::raw_sql(trimmed).execute(&mut *conn).await {
                    Ok(_) => {}
                    Err(e) => {
                        let err_str = e.to_string();
                        if err_str.contains("already exists")
                            || err_str.contains("duplicate key value")
                            || err_str.contains("multiple primary keys")
                            || err_str.contains("relation already exists")
                        {
                            debug!("迁移 {} 跳过已存在对象: {}", filename, err_str);
                        } else if err_str.contains("canceling statement due to statement timeout") {
                            let preview: String = trimmed.chars().take(100).collect();
                            warn!(
                                filename = %filename,
                                version = %version,
                                statement_timeout_secs = 30_u64,
                                statement_preview = %preview,
                                "迁移语句超时"
                            );
                            file_success = false;
                            let _ = sqlx::query("ROLLBACK").execute(&mut *conn).await;
                            break;
                        } else {
                            let preview: String = trimmed.chars().take(100).collect();
                            warn!(
                                error = %e,
                                filename = %filename,
                                version = %version,
                                statement_preview = %preview,
                                "迁移语句执行失败"
                            );
                            file_success = false;
                            let _ = sqlx::query("ROLLBACK").execute(&mut *conn).await;
                            break;
                        }
                    }
                }
            }

            let execution_time_ms = start_time.elapsed().as_millis() as i64;

            let _ = self.record_migration(version, &checksum, execution_time_ms, file_success).await;

            if file_success {
                info!(filename = %filename, execution_time_ms, "迁移执行成功");
                success_count += 1;
            } else {
                error_count += 1;
            }
        }

        info!(success_count, skip_count, error_count, "迁移完成");
        Ok(format!("数据库迁移执行完成 (成功: {success_count}, 跳过: {skip_count}, 错误: {error_count})"))
    }

    fn split_sql_statements(sql: &str) -> Vec<String> {
        #[derive(Debug, Clone, Copy, PartialEq)]
        enum State {
            Normal,
            InSingleQuote,
            InDoubleQuote,
            InDollarQuote,
            InLineComment,
            InBlockComment,
        }

        let mut statements = Vec::new();
        let mut current = String::new();
        let mut state = State::Normal;
        let mut dollar_tag = String::new();
        let mut paren_depth: i32 = 0;
        let chars: Vec<char> = sql.chars().collect();
        let mut i = 0;

        while i < chars.len() {
            let c = chars[i];
            let next_c = if i + 1 < chars.len() { Some(chars[i + 1]) } else { None };
            let prev_c = if i > 0 { Some(chars[i - 1]) } else { None };

            match state {
                State::Normal => {
                    if c == '-' && next_c == Some('-') {
                        state = State::InLineComment;
                        i += 1;
                    } else if c == '/' && next_c == Some('*') {
                        state = State::InBlockComment;
                        i += 1;
                    } else if c == '\'' {
                        state = State::InSingleQuote;
                        current.push(c);
                    } else if c == '"' {
                        state = State::InDoubleQuote;
                        current.push(c);
                    } else if c == '$' {
                        if let Some(tag_end) = Self::find_dollar_tag_end(&chars, i) {
                            if tag_end > i {
                                dollar_tag = chars[i..=tag_end].iter().collect();
                                state = State::InDollarQuote;
                                current.push_str(&dollar_tag);
                                i = tag_end;
                            } else {
                                current.push(c);
                            }
                        } else {
                            current.push(c);
                        }
                    } else if c == '(' {
                        paren_depth += 1;
                        current.push(c);
                    } else if c == ')' {
                        paren_depth = paren_depth.saturating_sub(1);
                        current.push(c);
                    } else if c == ';' && paren_depth == 0 {
                        let trimmed = current.trim();
                        if !trimmed.is_empty() {
                            statements.push(trimmed.to_string());
                        }
                        current = String::new();
                    } else {
                        current.push(c);
                    }
                }
                State::InSingleQuote => {
                    current.push(c);
                    if c == '\'' {
                        if next_c == Some('\'') {
                            i += 1;
                            current.push('\'');
                        } else {
                            state = State::Normal;
                        }
                    }
                }
                State::InDoubleQuote => {
                    current.push(c);
                    if c == '"' && prev_c != Some('\\') {
                        state = State::Normal;
                    }
                }
                State::InDollarQuote => {
                    current.push(c);
                    if c == '$' && dollar_tag.len() > 1 {
                        let tag_len = dollar_tag.len();
                        let start = i.saturating_sub(tag_len - 1);
                        if start + tag_len <= chars.len() {
                            let potential_tag: String = chars[start..start + tag_len].iter().collect();
                            if potential_tag == dollar_tag {
                                state = State::Normal;
                                dollar_tag.clear();
                            }
                        }
                    }
                }
                State::InLineComment => {
                    if c == '\n' {
                        state = State::Normal;
                    }
                }
                State::InBlockComment => {
                    if c == '*' && next_c == Some('/') {
                        state = State::Normal;
                        i += 1;
                    }
                }
            }
            i += 1;
        }

        if !current.trim().is_empty() {
            statements.push(current.trim().to_string());
        }

        statements.retain(|s| !s.trim().is_empty());
        statements
    }

    fn normalize_migration_sql(sql: &str) -> String {
        sql.replace("table_schema = 'public'", "table_schema = current_schema()")
            .replace("table_schema='public'", "table_schema = current_schema()")
            .replace("schemaname = 'public'", "schemaname = current_schema()")
            .replace("schemaname='public'", "schemaname = current_schema()")
    }

    fn find_dollar_tag_end(chars: &[char], start: usize) -> Option<usize> {
        let mut i = start + 1;
        while i < chars.len() {
            let c = chars[i];
            if c == '$' {
                return Some(i);
            }
            if !c.is_ascii_alphanumeric() && c != '_' {
                return None;
            }
            i += 1;
        }
        None
    }

    async fn step_schema_validation(&self) -> Result<synapse_storage::SchemaValidationResult, sqlx::Error> {
        self.schema_validator.validate_all().await
    }

    #[cfg(feature = "runtime-ddl")]
    async fn step_schema_repair(&self) -> Result<Vec<String>, sqlx::Error> {
        self.schema_validator.repair_missing_columns().await
    }

    async fn step_index_validation(&self) -> Result<Vec<String>, sqlx::Error> {
        self.schema_validator.validate_indexes().await
    }

    #[cfg(feature = "runtime-ddl")]
    async fn step_create_indexes(&self) -> Result<Vec<String>, sqlx::Error> {
        self.schema_validator.create_missing_indexes().await
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_utils;

    #[tokio::test]
    async fn test_schema_migrations_table_has_is_success_column() {
        // Acquire serialized access to test environment
        let _guard = test_utils::env_lock_async().await;

        // Create pool to isolated empty schema
        let pool = test_utils::prepare_empty_isolated_test_pool().await.expect("failed to create isolated test pool");

        // Create the table via the actual function under test
        let init = DatabaseInitService::new(pool.clone());
        init.ensure_schema_migrations_table().await.expect("failed to create schema_migrations table");

        // Verify: INSERT and SELECT using is_success (the name Rust queries use)
        sqlx::query("INSERT INTO schema_migrations (version, is_success) VALUES ($1, $2)")
            .bind("test_version")
            .bind(true)
            .execute(&*pool)
            .await
            .expect("INSERT with is_success should work");

        let (success,): (bool,) = sqlx::query_as("SELECT is_success FROM schema_migrations WHERE version = $1")
            .bind("test_version")
            .fetch_one(&*pool)
            .await
            .expect("SELECT with is_success should work");

        assert!(success, "is_success should be true");
    }
}
