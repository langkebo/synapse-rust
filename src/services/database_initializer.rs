use crate::storage::SchemaValidator;
use sqlx::PgPool;
use std::sync::Arc;
use tracing::{debug, error, info, warn};

#[allow(dead_code)]
const DEFAULT_CACHE_TTL_SECONDS: i64 = 3600;
const RUNTIME_DB_INIT_ENV: &str = "SYNAPSE_ENABLE_RUNTIME_DB_INIT";

#[derive(Debug, Clone, PartialEq)]
pub enum Environment {
    Development,
    Production,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DatabaseInitMode {
    Auto,
    Strict,
    Compatible,
}

impl Environment {
    pub fn from_env() -> Self {
        std::env::var("RUST_ENV")
            .unwrap_or_else(|_| "development".to_string())
            .to_lowercase()
            .into()
    }

    pub fn is_development(&self) -> bool {
        self == &Environment::Development
    }
}

impl From<String> for Environment {
    fn from(s: String) -> Self {
        match s.as_str() {
            "prod" | "production" | "release" => Environment::Production,
            _ => Environment::Development,
        }
    }
}

/// Service for database initialization, validation and repair operations.
/// Handles database schema verification, index creation, and cache management.
pub struct DatabaseInitService {
    pool: Arc<PgPool>,
    schema_validator: SchemaValidator,
    cache_ttl_seconds: i64,
    environment: Environment,
    mode: DatabaseInitMode,
}

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
            success: true,
            steps: Vec::new(),
            errors: Vec::new(),
            schema_status: None,
            repairs_performed: Vec::new(),
            skipped: false,
        };

        info!("开始数据库初始化流程...");

        match self.step_connection_test().await {
            Ok(msg) => {
                info!("连接测试: {}", msg);
                report.steps.push(msg);
            }
            Err(e) => {
                report.success = false;
                report.errors.push(format!("连接测试失败: {}", e));
                return Ok(report);
            }
        }

        let Some(mode) = self.resolved_mode() else {
            let message = format!(
                "运行时数据库初始化默认已禁用；请使用 docker/db_migrate.sh 与 db-migration-gate.yml 作为迁移主链，如需兼容启用请设置 {}=true",
                RUNTIME_DB_INIT_ENV
            );
            info!("{}", message);
            report.steps.push(message);
            report.skipped = true;
            return Ok(report);
        };

        info!("开始执行数据库迁移...");
        match self.step_migrations().await {
            Ok(msg) => {
                info!("迁移结果: {}", msg);
                report.steps.push(msg);
            }
            Err(e) => {
                error!("数据库迁移失败: {}", e);
                report.success = false;
                report.errors.push(format!("数据库迁移失败: {}", e));
                return Ok(report);
            }
        }

        if mode == DatabaseInitMode::Strict {
            let message =
                "严格模式仅执行 migrations/*.sql；运行时 DDL 已禁用并由迁移链路兜底".to_string();
            info!("{}", message);
            report.steps.push(message);
            return Ok(report);
        }

        let should_skip_validation = self.check_cache_valid().await?;
        if should_skip_validation {
            debug!("数据库初始化缓存有效，跳过详细验证");
            report.steps.push("使用缓存，跳过验证".to_string());
            report.skipped = true;
            if self.environment.is_development() {
                info!("数据库初始化完成 (使用缓存): success=true");
            } else {
                debug!("数据库初始化完成 (使用缓存): success=true");
            }
            return Ok(report);
        }

        match self.step_schema_validation().await {
            Ok(status) => report.schema_status = Some(status),
            Err(e) => {
                report.success = false;
                report.errors.push(format!("Schema验证失败: {}", e));
            }
        };

        match self.step_index_validation().await {
            Ok(issues) => {
                if !issues.is_empty() {
                    if self.environment.is_development() {
                        info!("发现 {} 个缺失的索引", issues.len());
                    } else {
                        debug!("发现 {} 个缺失的索引", issues.len());
                    }
                }
            }
            Err(e) => report.errors.push(format!("索引验证失败: {}", e)),
        }

        if report.success {
            self.update_init_timestamp().await?;
        }

        if self.environment.is_development() {
            info!("数据库初始化完成: success={}", report.success);
        } else {
            debug!("数据库初始化完成: success={}", report.success);
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
            DatabaseInitMode::Auto => {
                Self::runtime_db_init_enabled().then_some(DatabaseInitMode::Compatible)
            }
            DatabaseInitMode::Strict => Some(DatabaseInitMode::Strict),
            DatabaseInitMode::Compatible => Some(DatabaseInitMode::Compatible),
        }
    }

    async fn check_cache_valid(&self) -> Result<bool, sqlx::Error> {
        let result = sqlx::query_as::<_, (i64,)>(
            r#"
            SELECT value::BIGINT
            FROM db_metadata WHERE key = 'last_init_ts'
            "#,
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
            r#"
            INSERT INTO db_metadata (key, value, created_ts, updated_ts)
            VALUES ('last_init_ts', $1, $2, $2)
            ON CONFLICT (key) DO UPDATE SET
                value = EXCLUDED.value,
                updated_ts = EXCLUDED.updated_ts
            "#,
        )
        .bind(now.to_string())
        .bind(now)
        .execute(&*self.pool)
        .await?;

        Ok(())
    }

    async fn step_connection_test(&self) -> Result<String, sqlx::Error> {
        sqlx::query("SELECT 1 as test")
            .fetch_one(&*self.pool)
            .await?;
        Ok("数据库连接测试通过".to_string())
    }

    async fn step_migrations(&self) -> Result<String, sqlx::Error> {
        info!("执行数据库迁移...");

        self.ensure_schema_migrations_table().await?;

        let migrations_dir = if std::path::Path::new("/app/migrations").exists() {
            std::path::Path::new("/app/migrations")
        } else if std::path::Path::new("./migrations").exists() {
            std::path::Path::new("./migrations")
        } else {
            info!("未找到迁移目录，跳过迁移");
            return Ok("数据库迁移跳过 (无迁移文件)".to_string());
        };

        info!("使用运行时迁移文件: {:?}", migrations_dir);
        self.run_runtime_migrations(migrations_dir).await
    }

    async fn ensure_schema_migrations_table(&self) -> Result<(), sqlx::Error> {
        let create_table_sql = r#"
            CREATE TABLE IF NOT EXISTS schema_migrations (
                id BIGSERIAL PRIMARY KEY,
                version TEXT NOT NULL,
                name TEXT,
                checksum TEXT,
                applied_ts BIGINT,
                execution_time_ms BIGINT,
                success BOOLEAN NOT NULL DEFAULT TRUE,
                description TEXT,
                executed_at TIMESTAMPTZ DEFAULT NOW(),
                CONSTRAINT uq_schema_migrations_version UNIQUE (version)
            )
        "#;

        sqlx::raw_sql(create_table_sql).execute(&*self.pool).await?;

        sqlx::query(
            r#"
            CREATE INDEX IF NOT EXISTS idx_schema_migrations_version ON schema_migrations(version)
            "#,
        )
        .execute(&*self.pool)
        .await?;

        info!("schema_migrations 表已就绪");
        Ok(())
    }

    async fn is_migration_executed(&self, version: &str) -> Result<bool, sqlx::Error> {
        let result: Option<(bool,)> =
            sqlx::query_as("SELECT success FROM schema_migrations WHERE version = $1")
                .bind(version)
                .fetch_optional(&*self.pool)
                .await?;

        Ok(result.map(|(success,)| success).unwrap_or(false))
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
            r#"INSERT INTO schema_migrations (version, checksum, applied_ts, execution_time_ms, success)
               VALUES ($1, $2, $3, $4, $5)
               ON CONFLICT (version) DO UPDATE SET
                   checksum = EXCLUDED.checksum,
                   applied_ts = EXCLUDED.applied_ts,
                   executed_at = NOW(),
                   execution_time_ms = EXCLUDED.execution_time_ms,
                   success = EXCLUDED.success"#,
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

    async fn run_runtime_migrations(
        &self,
        migrations_dir: &std::path::Path,
    ) -> Result<String, sqlx::Error> {
        let mut migration_files: Vec<std::path::PathBuf> = std::fs::read_dir(migrations_dir)
            .map_err(|e| sqlx::Error::Configuration(e.to_string().into()))?
            .filter_map(|entry| entry.ok())
            .map(|entry| entry.path())
            .filter(|path| {
                path.extension().is_some_and(|ext| ext == "sql")
                    && path
                        .file_name()
                        .and_then(|name| name.to_str())
                        .is_some_and(|name| !name.ends_with(".undo.sql"))
            })
            .collect();

        migration_files.sort();

        info!("发现 {} 个迁移文件", migration_files.len());

        let mut success_count = 0;
        let mut skip_count = 0;
        let mut error_count = 0;

        for migration_file in migration_files {
            let filename = migration_file
                .file_name()
                .and_then(|n| n.to_str())
                .unwrap_or("unknown");

            let version = filename.trim_end_matches(".sql");

            match self.is_migration_executed(version).await {
                Ok(true) => {
                    debug!("迁移 {} 已执行，跳过", filename);
                    skip_count += 1;
                    continue;
                }
                Ok(false) => {}
                Err(e) => {
                    warn!("检查迁移状态失败 {}: {}", filename, e);
                }
            }

            let sql = match std::fs::read_to_string(&migration_file) {
                Ok(s) => s,
                Err(e) => {
                    warn!("无法读取迁移文件 {}: {}", filename, e);
                    error_count += 1;
                    continue;
                }
            };

            let checksum = Self::calculate_checksum(&sql);
            let normalized_sql = Self::normalize_migration_sql(&sql);
            let start_time = std::time::Instant::now();
            let statements = self.split_sql_statements(&normalized_sql);
            let mut file_success = true;

            for statement in statements {
                let trimmed = statement.trim();
                if trimmed.is_empty() || trimmed.starts_with("--") {
                    continue;
                }

                // Set statement timeout to 30 seconds to prevent indefinite hangs
                let timeout_result = sqlx::query("SET LOCAL statement_timeout = '30s'")
                    .execute(&*self.pool)
                    .await;

                if let Err(e) = timeout_result {
                    debug!("无法设置 statement_timeout: {}", e);
                }

                match sqlx::raw_sql(trimmed).execute(&*self.pool).await {
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
                            warn!("迁移 {} 语句超时 (30s): {}", filename, preview);
                            file_success = false;
                            let _ = sqlx::query("ROLLBACK").execute(&*self.pool).await;
                            break;
                        } else {
                            let preview: String = trimmed.chars().take(100).collect();
                            warn!("迁移 {} 语句执行失败: {} - {}", filename, preview, e);
                            file_success = false;
                            let _ = sqlx::query("ROLLBACK").execute(&*self.pool).await;
                            break;
                        }
                    }
                }
            }

            let execution_time_ms = start_time.elapsed().as_millis() as i64;

            let _ = self
                .record_migration(version, &checksum, execution_time_ms, file_success)
                .await;

            if file_success {
                info!("迁移 {} 执行成功 ({}ms)", filename, execution_time_ms);
                success_count += 1;
            } else {
                error_count += 1;
            }
        }

        info!(
            "迁移完成: 成功={}, 跳过={}, 错误={}",
            success_count, skip_count, error_count
        );
        Ok(format!(
            "数据库迁移执行完成 (成功: {}, 跳过: {}, 错误: {})",
            success_count, skip_count, error_count
        ))
    }

    fn split_sql_statements(&self, sql: &str) -> Vec<String> {
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
            let next_c = if i + 1 < chars.len() {
                Some(chars[i + 1])
            } else {
                None
            };
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
                            let potential_tag: String =
                                chars[start..start + tag_len].iter().collect();
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

    async fn step_schema_validation(
        &self,
    ) -> Result<crate::storage::SchemaValidationResult, sqlx::Error> {
        self.schema_validator.validate_all().await
    }

    #[cfg(feature = "runtime-ddl")]
    #[allow(dead_code)]
    async fn step_schema_repair(&self) -> Result<Vec<String>, sqlx::Error> {
        self.schema_validator.repair_missing_columns().await
    }

    async fn step_index_validation(&self) -> Result<Vec<String>, sqlx::Error> {
        self.schema_validator.validate_indexes().await
    }

    #[cfg(feature = "runtime-ddl")]
    #[allow(dead_code)]
    async fn step_create_indexes(&self) -> Result<Vec<String>, sqlx::Error> {
        self.schema_validator.create_missing_indexes().await
    }

    #[cfg(feature = "runtime-ddl")]
    #[allow(dead_code)]
    async fn step_create_e2ee_tables(&self) -> Result<String, sqlx::Error> {
        sqlx::query(
            r#"
            CREATE TABLE IF NOT EXISTS device_keys (
                id BIGSERIAL PRIMARY KEY,
                user_id TEXT NOT NULL,
                device_id TEXT NOT NULL,
                algorithm TEXT NOT NULL,
                key_id TEXT NOT NULL,
                public_key TEXT NOT NULL,
                key_data TEXT,
                signatures JSONB,
                added_ts BIGINT NOT NULL,
                created_ts BIGINT NOT NULL,
                updated_ts BIGINT,
                ts_updated_ms BIGINT,
                is_verified BOOLEAN DEFAULT FALSE,
                is_blocked BOOLEAN DEFAULT FALSE,
                display_name TEXT,
                CONSTRAINT uq_device_keys_user_device_key UNIQUE (user_id, device_id, key_id)
            )
            "#,
        )
        .execute(&*self.pool)
        .await?;

        sqlx::query(
            r#"
            CREATE INDEX IF NOT EXISTS idx_device_keys_user_device ON device_keys(user_id, device_id)
            "#,
        )
        .execute(&*self.pool)
        .await?;

        Ok("E2EE设备密钥表创建完成".to_string())
    }

    /// 创建 E2EE 核心表 - 包括 Olm 和 Megolm 会话表
    /// 这些表在迁移文件中定义，确保在迁移失败时也能创建
    #[cfg(feature = "runtime-ddl")]
    #[allow(dead_code)]
    async fn step_create_e2ee_core_tables(&self) -> Result<String, sqlx::Error> {
        // Create olm_accounts table
        sqlx::query(
            r#"
            CREATE TABLE IF NOT EXISTS olm_accounts (
                id BIGSERIAL PRIMARY KEY,
                user_id TEXT NOT NULL,
                device_id TEXT NOT NULL,
                identity_key TEXT NOT NULL,
                serialized_account TEXT NOT NULL,
                is_one_time_keys_published BOOLEAN DEFAULT FALSE,
                is_fallback_key_published BOOLEAN DEFAULT FALSE,
                created_ts BIGINT NOT NULL,
                updated_ts BIGINT NOT NULL,
                CONSTRAINT uq_olm_accounts_user_device UNIQUE (user_id, device_id)
            )
            "#,
        )
        .execute(&*self.pool)
        .await?;

        sqlx::query("CREATE INDEX IF NOT EXISTS idx_olm_accounts_user ON olm_accounts(user_id)")
            .execute(&*self.pool)
            .await?;
        sqlx::query(
            "CREATE INDEX IF NOT EXISTS idx_olm_accounts_device ON olm_accounts(device_id)",
        )
        .execute(&*self.pool)
        .await?;

        // Create olm_sessions table
        sqlx::query(
            r#"
            CREATE TABLE IF NOT EXISTS olm_sessions (
                id BIGSERIAL PRIMARY KEY,
                user_id TEXT NOT NULL,
                device_id TEXT NOT NULL,
                session_id TEXT NOT NULL,
                sender_key TEXT NOT NULL,
                receiver_key TEXT NOT NULL,
                serialized_state TEXT NOT NULL,
                message_index INTEGER DEFAULT 0,
                created_ts BIGINT NOT NULL,
                last_used_ts BIGINT NOT NULL,
                is_fallback BOOLEAN DEFAULT FALSE,
                CONSTRAINT uq_olm_sessions UNIQUE (session_id)
            )
            "#,
        )
        .execute(&*self.pool)
        .await?;

        sqlx::query("CREATE INDEX IF NOT EXISTS idx_olm_sessions_user ON olm_sessions(user_id)")
            .execute(&*self.pool)
            .await?;
        sqlx::query(
            "CREATE INDEX IF NOT EXISTS idx_olm_sessions_device ON olm_sessions(device_id)",
        )
        .execute(&*self.pool)
        .await?;

        // Create megolm_sessions table
        sqlx::query(
            r#"
            CREATE TABLE IF NOT EXISTS megolm_sessions (
                id BIGSERIAL PRIMARY KEY,
                room_id TEXT NOT NULL,
                session_id TEXT NOT NULL,
                sender_key TEXT NOT NULL,
                sender_claimed_key TEXT NOT NULL,
                forwarding_chains JSONB DEFAULT '[]',
                is_fallback BOOLEAN DEFAULT FALSE,
                session_data JSONB NOT NULL,
                message_index INTEGER DEFAULT 0,
                created_ts BIGINT NOT NULL,
                updated_ts BIGINT NOT NULL,
                used_ts BIGINT,
                CONSTRAINT uq_megolm_sessions UNIQUE (room_id, session_id)
            )
            "#,
        )
        .execute(&*self.pool)
        .await?;

        sqlx::query(
            "CREATE INDEX IF NOT EXISTS idx_megolm_sessions_room ON megolm_sessions(room_id)",
        )
        .execute(&*self.pool)
        .await?;

        // Create cross_signing_keys table
        sqlx::query(
            r#"
            CREATE TABLE IF NOT EXISTS cross_signing_keys (
                id BIGSERIAL PRIMARY KEY,
                user_id TEXT NOT NULL,
                key_type TEXT NOT NULL,
                key_id TEXT NOT NULL,
                key_data TEXT NOT NULL,
                signatures JSONB DEFAULT '{}',
                verified BOOLEAN DEFAULT FALSE,
                trust_level TEXT,
                created_ts BIGINT NOT NULL,
                updated_ts BIGINT,
                CONSTRAINT uq_cross_signing_keys UNIQUE (user_id, key_type)
            )
            "#,
        )
        .execute(&*self.pool)
        .await?;

        sqlx::query(
            "CREATE INDEX IF NOT EXISTS idx_cross_signing_keys_user ON cross_signing_keys(user_id)",
        )
        .execute(&*self.pool)
        .await?;

        // Create backup_keys table (密钥备份数据)
        sqlx::query(
            r#"
            CREATE TABLE IF NOT EXISTS backup_keys (
                id BIGSERIAL PRIMARY KEY,
                backup_id BIGINT NOT NULL,
                room_id TEXT NOT NULL,
                session_id TEXT NOT NULL,
                session_data JSONB NOT NULL,
                created_ts BIGINT NOT NULL,
                first_message_index INTEGER,
                forwarded_count INTEGER DEFAULT 0,
                is_verified BOOLEAN DEFAULT FALSE,
                backup_data JSONB
            )
            "#,
        )
        .execute(&*self.pool)
        .await?;

        sqlx::query("CREATE INDEX IF NOT EXISTS idx_backup_keys_backup ON backup_keys(backup_id)")
            .execute(&*self.pool)
            .await?;
        sqlx::query("CREATE INDEX IF NOT EXISTS idx_backup_keys_room ON backup_keys(room_id)")
            .execute(&*self.pool)
            .await?;

        Ok("E2EE核心表创建完成".to_string())
    }

    #[cfg(feature = "runtime-ddl")]
    #[allow(dead_code)]
    async fn step_ensure_additional_tables(&self) -> Result<String, sqlx::Error> {
        // Ensure typing table exists
        sqlx::query(
            r#"
            CREATE TABLE IF NOT EXISTS typing (
                user_id TEXT NOT NULL,
                room_id TEXT NOT NULL,
                typing BOOLEAN DEFAULT FALSE,
                last_active_ts BIGINT NOT NULL,
                UNIQUE (user_id, room_id)
            )
            "#,
        )
        .execute(&*self.pool)
        .await?;

        // Ensure search tables exist
        sqlx::query(
            r#"
            CREATE TABLE IF NOT EXISTS search_index (
                id SERIAL PRIMARY KEY,
                event_id VARCHAR(255) NOT NULL,
                room_id VARCHAR(255) NOT NULL,
                user_id VARCHAR(255) NOT NULL,
                event_type VARCHAR(255) NOT NULL,
                type VARCHAR(255) NOT NULL,
                content TEXT NOT NULL,
                created_ts BIGINT NOT NULL,
                updated_ts BIGINT,
                CONSTRAINT uq_search_index_event UNIQUE (event_id)
            )
            "#,
        )
        .execute(&*self.pool)
        .await?;

        sqlx::query(
            r#"
            CREATE INDEX IF NOT EXISTS idx_search_index_room ON search_index(room_id)
            "#,
        )
        .execute(&*self.pool)
        .await?;

        sqlx::query(
            r#"
            CREATE INDEX IF NOT EXISTS idx_search_index_user ON search_index(user_id)
            "#,
        )
        .execute(&*self.pool)
        .await?;

        sqlx::query(
            r#"
            CREATE INDEX IF NOT EXISTS idx_search_index_type ON search_index(event_type)
            "#,
        )
        .execute(&*self.pool)
        .await?;

        // Ensure user_directory table exists
        sqlx::query(
            r#"
            CREATE TABLE IF NOT EXISTS user_directory (
                user_id TEXT NOT NULL,
                room_id TEXT NOT NULL,
                visibility TEXT NOT NULL DEFAULT 'private',
                added_by TEXT,
                created_ts BIGINT NOT NULL,
                updated_ts BIGINT,
                CONSTRAINT pk_user_directory PRIMARY KEY (user_id, room_id)
            )
            "#,
        )
        .execute(&*self.pool)
        .await?;

        sqlx::query(
            r#"
            CREATE INDEX IF NOT EXISTS idx_user_directory_user ON user_directory(user_id)
            "#,
        )
        .execute(&*self.pool)
        .await?;

        sqlx::query(
            r#"
            CREATE INDEX IF NOT EXISTS idx_user_directory_visibility ON user_directory(visibility)
            "#,
        )
        .execute(&*self.pool)
        .await?;

        // Ensure is_guest column exists in users table
        sqlx::query("ALTER TABLE users ADD COLUMN IF NOT EXISTS is_guest BOOLEAN DEFAULT FALSE")
            .execute(&*self.pool)
            .await?;

        // Ensure user_privacy_settings table exists
        sqlx::query(
            r#"
            CREATE TABLE IF NOT EXISTS user_privacy_settings (
                user_id VARCHAR(255) PRIMARY KEY,
                allow_presence_lookup BOOLEAN DEFAULT TRUE,
                allow_profile_lookup BOOLEAN DEFAULT TRUE,
                allow_room_invites BOOLEAN DEFAULT TRUE,
                created_ts BIGINT NOT NULL,
                updated_ts BIGINT
            )
            "#,
        )
        .execute(&*self.pool)
        .await?;

        // Ensure pushers table exists
        sqlx::query(
            r#"
            CREATE TABLE IF NOT EXISTS pushers (
                id BIGSERIAL PRIMARY KEY,
                user_id TEXT NOT NULL,
                device_id TEXT NOT NULL,
                pushkey TEXT NOT NULL,
                pushkey_ts BIGINT NOT NULL,
                kind TEXT NOT NULL,
                app_id TEXT NOT NULL,
                app_display_name TEXT NOT NULL,
                device_display_name TEXT NOT NULL,
                profile_tag TEXT,
                lang TEXT DEFAULT 'en',
                data JSONB DEFAULT '{}',
                updated_ts BIGINT,
                created_ts BIGINT NOT NULL,
                is_enabled BOOLEAN DEFAULT TRUE,
                CONSTRAINT uq_pushers_user_device_pushkey UNIQUE (user_id, device_id, pushkey)
            )
            "#,
        )
        .execute(&*self.pool)
        .await?;

        sqlx::query(
            r#"
            CREATE INDEX IF NOT EXISTS idx_pushers_user ON pushers(user_id)
            "#,
        )
        .execute(&*self.pool)
        .await?;

        sqlx::query(
            r#"
            CREATE INDEX IF NOT EXISTS idx_pushers_enabled ON pushers(is_enabled) WHERE is_enabled = TRUE
            "#,
        )
        .execute(&*self.pool)
        .await?;

        // Ensure account_data table exists
        sqlx::query(
            r#"
            CREATE TABLE IF NOT EXISTS account_data (
                id BIGSERIAL PRIMARY KEY,
                user_id TEXT NOT NULL,
                data_type TEXT NOT NULL,
                content JSONB NOT NULL DEFAULT '{}',
                created_ts BIGINT NOT NULL,
                updated_ts BIGINT NOT NULL,
                CONSTRAINT uq_account_data_user_type UNIQUE (user_id, data_type)
            )
            "#,
        )
        .execute(&*self.pool)
        .await?;

        sqlx::query(
            r#"
            CREATE INDEX IF NOT EXISTS idx_account_data_user ON account_data(user_id)
            "#,
        )
        .execute(&*self.pool)
        .await?;

        // Ensure key_backups table exists with backup_id column
        sqlx::query(
            r#"
            CREATE TABLE IF NOT EXISTS key_backups (
                backup_id BIGSERIAL PRIMARY KEY,
                user_id TEXT NOT NULL,
                backup_id_text TEXT,
                algorithm TEXT NOT NULL,
                auth_data JSONB,
                auth_key TEXT,
                mgmt_key TEXT,
                version BIGINT DEFAULT 1,
                etag TEXT,
                created_ts BIGINT NOT NULL,
                updated_ts BIGINT,
                CONSTRAINT uq_key_backups_user_version UNIQUE (user_id, version)
            )
            "#,
        )
        .execute(&*self.pool)
        .await?;

        sqlx::query(
            r#"
            CREATE INDEX IF NOT EXISTS idx_key_backups_user ON key_backups(user_id)
            "#,
        )
        .execute(&*self.pool)
        .await?;

        // Ensure rooms table has guest_access column (for RoomSummary compatibility)
        // Note: rooms table has has_guest_access BOOLEAN, but room_summaries uses guest_access VARCHAR
        sqlx::query("ALTER TABLE rooms ADD COLUMN IF NOT EXISTS guest_access VARCHAR(50) DEFAULT 'forbidden'")
            .execute(&*self.pool)
            .await?;

        // Ensure refresh_tokens table has expires_at column
        sqlx::query("ALTER TABLE refresh_tokens ADD COLUMN IF NOT EXISTS expires_at BIGINT")
            .execute(&*self.pool)
            .await?;

        // Ensure room_tags table exists
        sqlx::query(
            r#"
            CREATE TABLE IF NOT EXISTS room_tags (
                id SERIAL PRIMARY KEY,
                user_id VARCHAR(255) NOT NULL,
                room_id VARCHAR(255) NOT NULL,
                tag VARCHAR(255) NOT NULL,
                order_value DOUBLE PRECISION,
                created_ts BIGINT NOT NULL,
                UNIQUE (user_id, room_id, tag)
            )
            "#,
        )
        .execute(&*self.pool)
        .await?;

        sqlx::query(
            r#"
            CREATE INDEX IF NOT EXISTS idx_room_tags_user ON room_tags(user_id)
            "#,
        )
        .execute(&*self.pool)
        .await?;

        // Ensure room_events table for event retrieval
        sqlx::query(
            r#"
            CREATE TABLE IF NOT EXISTS room_events (
                id SERIAL PRIMARY KEY,
                event_id VARCHAR(255) UNIQUE NOT NULL,
                room_id VARCHAR(255) NOT NULL,
                sender VARCHAR(255) NOT NULL,
                event_type VARCHAR(255) NOT NULL,
                state_key VARCHAR(255),
                content JSONB NOT NULL DEFAULT '{}',
                prev_event_id VARCHAR(255),
                origin_server_ts BIGINT NOT NULL,
                created_ts BIGINT NOT NULL
            )
            "#,
        )
        .execute(&*self.pool)
        .await?;

        sqlx::query(
            r#"
            CREATE INDEX IF NOT EXISTS idx_room_events_room ON room_events(room_id)
            "#,
        )
        .execute(&*self.pool)
        .await?;

        sqlx::query(
            r#"
            CREATE INDEX IF NOT EXISTS idx_room_events_event ON room_events(event_id)
            "#,
        )
        .execute(&*self.pool)
        .await?;

        // Ensure to_device_messages table for E2EE to-device messaging
        sqlx::query(
            r#"
            CREATE TABLE IF NOT EXISTS to_device_messages (
                id SERIAL PRIMARY KEY,
                sender_user_id VARCHAR(255) NOT NULL,
                sender_device_id VARCHAR(255) NOT NULL,
                recipient_user_id VARCHAR(255) NOT NULL,
                recipient_device_id VARCHAR(255) NOT NULL,
                event_type VARCHAR(255) NOT NULL,
                content JSONB NOT NULL DEFAULT '{}',
                message_id VARCHAR(255),
                stream_id BIGINT NOT NULL,
                created_ts BIGINT NOT NULL
            )
            "#,
        )
        .execute(&*self.pool)
        .await?;

        sqlx::query(
            r#"
            CREATE INDEX IF NOT EXISTS idx_to_device_recipient ON to_device_messages(recipient_user_id, recipient_device_id)
            "#,
        )
        .execute(&*self.pool)
        .await?;

        sqlx::query(
            r#"
            CREATE INDEX IF NOT EXISTS idx_to_device_stream ON to_device_messages(recipient_user_id, stream_id)
            "#,
        )
        .execute(&*self.pool)
        .await?;

        // Ensure device_lists_changes table for tracking device list updates
        sqlx::query(
            r#"
            CREATE TABLE IF NOT EXISTS device_lists_changes (
                id SERIAL PRIMARY KEY,
                user_id VARCHAR(255) NOT NULL,
                device_id VARCHAR(255),
                change_type VARCHAR(50) NOT NULL,
                stream_id BIGINT NOT NULL,
                created_ts BIGINT NOT NULL
            )
            "#,
        )
        .execute(&*self.pool)
        .await?;

        sqlx::query(
            r#"
            CREATE INDEX IF NOT EXISTS idx_device_lists_user ON device_lists_changes(user_id)
            "#,
        )
        .execute(&*self.pool)
        .await?;

        sqlx::query(
            r#"
            CREATE INDEX IF NOT EXISTS idx_device_lists_stream ON device_lists_changes(stream_id)
            "#,
        )
        .execute(&*self.pool)
        .await?;

        // Ensure room_ephemeral table for typing, receipts, etc.
        sqlx::query(
            r#"
            CREATE TABLE IF NOT EXISTS room_ephemeral (
                id SERIAL PRIMARY KEY,
                room_id VARCHAR(255) NOT NULL,
                event_type VARCHAR(255) NOT NULL,
                user_id VARCHAR(255) NOT NULL,
                content JSONB NOT NULL DEFAULT '{}',
                stream_id BIGINT NOT NULL,
                created_ts BIGINT NOT NULL,
                expires_at BIGINT
            )
            "#,
        )
        .execute(&*self.pool)
        .await?;

        sqlx::query(
            r#"
            CREATE INDEX IF NOT EXISTS idx_room_ephemeral_room ON room_ephemeral(room_id)
            "#,
        )
        .execute(&*self.pool)
        .await?;

        // Ensure device_lists_stream table for tracking device list stream position
        sqlx::query(
            r#"
            CREATE TABLE IF NOT EXISTS device_lists_stream (
                stream_id BIGSERIAL PRIMARY KEY,
                user_id VARCHAR(255) NOT NULL,
                device_id VARCHAR(255),
                created_ts BIGINT NOT NULL
            )
            "#,
        )
        .execute(&*self.pool)
        .await?;

        sqlx::query(
            r#"
            CREATE INDEX IF NOT EXISTS idx_device_lists_stream_user ON device_lists_stream(user_id)
            "#,
        )
        .execute(&*self.pool)
        .await?;

        // Ensure user_filters table for filter persistence
        sqlx::query(
            r#"
            CREATE TABLE IF NOT EXISTS user_filters (
                id SERIAL PRIMARY KEY,
                user_id VARCHAR(255) NOT NULL,
                filter_id VARCHAR(255) NOT NULL,
                filter_json JSONB NOT NULL DEFAULT '{}',
                created_ts BIGINT NOT NULL,
                UNIQUE (user_id, filter_id)
            )
            "#,
        )
        .execute(&*self.pool)
        .await?;

        sqlx::query(
            r#"
            CREATE INDEX IF NOT EXISTS idx_user_filters_user ON user_filters(user_id)
            "#,
        )
        .execute(&*self.pool)
        .await?;

        // Ensure sync_stream_id sequence table for generating stream IDs
        sqlx::query(
            r#"
            CREATE TABLE IF NOT EXISTS sync_stream_id (
                id BIGSERIAL PRIMARY KEY,
                stream_type TEXT,
                last_id BIGINT DEFAULT 0,
                updated_ts BIGINT,
                CONSTRAINT uq_sync_stream_id_type UNIQUE (stream_type)
            )
            "#,
        )
        .execute(&*self.pool)
        .await?;

        // Ensure a row exists for generating stream IDs
        sqlx::query(
            r#"
            INSERT INTO sync_stream_id (id) VALUES (1) ON CONFLICT DO NOTHING
            "#,
        )
        .execute(&*self.pool)
        .await?;

        sqlx::query("CREATE SEQUENCE IF NOT EXISTS sliding_sync_pos_seq")
            .execute(&*self.pool)
            .await?;

        sqlx::query(
            r#"
            CREATE TABLE IF NOT EXISTS sliding_sync_lists (
                id BIGSERIAL PRIMARY KEY,
                user_id TEXT NOT NULL,
                device_id TEXT NOT NULL,
                conn_id TEXT,
                list_key TEXT NOT NULL,
                sort JSONB DEFAULT '[]',
                filters JSONB DEFAULT '{}',
                room_subscription JSONB DEFAULT '{}',
                ranges JSONB DEFAULT '[]',
                created_ts BIGINT NOT NULL,
                updated_ts BIGINT NOT NULL
            )
            "#,
        )
        .execute(&*self.pool)
        .await?;

        sqlx::query(
            "CREATE UNIQUE INDEX IF NOT EXISTS idx_sliding_sync_lists_unique ON sliding_sync_lists(user_id, device_id, COALESCE(conn_id, ''), list_key)",
        )
        .execute(&*self.pool)
        .await?;

        sqlx::query(
            "CREATE INDEX IF NOT EXISTS idx_sliding_sync_lists_user_device ON sliding_sync_lists(user_id, device_id)",
        )
        .execute(&*self.pool)
        .await?;

        sqlx::query(
            r#"
            CREATE TABLE IF NOT EXISTS sliding_sync_tokens (
                id BIGSERIAL PRIMARY KEY,
                user_id TEXT NOT NULL,
                device_id TEXT NOT NULL,
                conn_id TEXT,
                token TEXT NOT NULL,
                pos BIGINT NOT NULL,
                created_ts BIGINT NOT NULL,
                expires_at BIGINT
            )
            "#,
        )
        .execute(&*self.pool)
        .await?;

        sqlx::query(
            "CREATE UNIQUE INDEX IF NOT EXISTS idx_sliding_sync_tokens_unique ON sliding_sync_tokens(user_id, device_id, COALESCE(conn_id, ''))",
        )
        .execute(&*self.pool)
        .await?;

        sqlx::query(
            "CREATE INDEX IF NOT EXISTS idx_sliding_sync_tokens_user ON sliding_sync_tokens(user_id, device_id)",
        )
        .execute(&*self.pool)
        .await?;

        sqlx::query(
            r#"
            CREATE TABLE IF NOT EXISTS sliding_sync_rooms (
                id BIGSERIAL PRIMARY KEY,
                user_id TEXT NOT NULL,
                device_id TEXT NOT NULL,
                room_id TEXT NOT NULL,
                conn_id TEXT,
                list_key TEXT,
                bump_stamp BIGINT DEFAULT 0,
                highlight_count INTEGER DEFAULT 0,
                notification_count INTEGER DEFAULT 0,
                is_dm BOOLEAN DEFAULT FALSE,
                is_encrypted BOOLEAN DEFAULT FALSE,
                is_tombstoned BOOLEAN DEFAULT FALSE,
                invited BOOLEAN DEFAULT FALSE,
                name TEXT,
                avatar TEXT,
                timestamp BIGINT DEFAULT 0,
                created_ts BIGINT NOT NULL,
                updated_ts BIGINT NOT NULL
            )
            "#,
        )
        .execute(&*self.pool)
        .await?;

        // Create unique index for sliding_sync_rooms (using COALESCE in index)
        sqlx::query(
            r#"
            CREATE UNIQUE INDEX IF NOT EXISTS idx_sliding_sync_rooms_unique ON sliding_sync_rooms (user_id, device_id, room_id, COALESCE(conn_id, ''))
            "#,
        )
        .execute(&*self.pool)
        .await?;

        sqlx::query(
            r#"
            CREATE INDEX IF NOT EXISTS idx_sliding_sync_rooms_user_device ON sliding_sync_rooms(user_id, device_id)
            "#,
        )
        .execute(&*self.pool)
        .await?;

        sqlx::query(
            r#"
            CREATE INDEX IF NOT EXISTS idx_sliding_sync_rooms_bump_stamp ON sliding_sync_rooms(bump_stamp DESC)
            "#,
        )
        .execute(&*self.pool)
        .await?;

        sqlx::query(
            r#"
            CREATE INDEX IF NOT EXISTS idx_sliding_sync_rooms_room_id ON sliding_sync_rooms(room_id, updated_ts DESC)
            "#,
        )
        .execute(&*self.pool)
        .await?;

        // Create thread_subscriptions table
        sqlx::query(
            r#"
            CREATE TABLE IF NOT EXISTS thread_subscriptions (
                id BIGSERIAL PRIMARY KEY,
                room_id TEXT NOT NULL,
                thread_id TEXT NOT NULL,
                user_id TEXT NOT NULL,
                notification_level TEXT DEFAULT 'all',
                is_muted BOOLEAN DEFAULT FALSE,
                is_pinned BOOLEAN DEFAULT FALSE,
                subscribed_ts BIGINT NOT NULL,
                updated_ts BIGINT NOT NULL,
                UNIQUE (room_id, thread_id, user_id)
            )
            "#,
        )
        .execute(&*self.pool)
        .await?;

        sqlx::query(
            r#"
            CREATE INDEX IF NOT EXISTS idx_thread_subscriptions_room_thread ON thread_subscriptions(room_id, thread_id)
            "#,
        )
        .execute(&*self.pool)
        .await?;

        // Create space_children table (with all fields including those added via migration)
        // First ensure the table exists, then add any missing columns
        sqlx::query(
            r#"
            CREATE TABLE IF NOT EXISTS space_children (
                id BIGSERIAL PRIMARY KEY,
                space_id TEXT NOT NULL,
                room_id TEXT NOT NULL,
                sender TEXT NOT NULL,
                is_suggested BOOLEAN DEFAULT FALSE,
                via_servers JSONB DEFAULT '[]',
                added_ts BIGINT NOT NULL,
                CONSTRAINT pk_space_children PRIMARY KEY (id),
                CONSTRAINT uq_space_children_space_room UNIQUE (space_id, room_id)
            )
            "#,
        )
        .execute(&*self.pool)
        .await?;

        // Add missing columns if they don't exist (for databases created before this migration)
        sqlx::query(
            r#"
            DO $$
            BEGIN
                IF NOT EXISTS (
                    SELECT 1 FROM information_schema.columns
                    WHERE table_schema = 'public' AND table_name = 'space_children'
                    AND column_name = 'order'
                ) THEN
                    ALTER TABLE space_children ADD COLUMN "order" TEXT DEFAULT '';
                END IF;
            END $$;
            "#,
        )
        .execute(&*self.pool)
        .await?;

        sqlx::query(
            r#"
            DO $$
            BEGIN
                IF NOT EXISTS (
                    SELECT 1 FROM information_schema.columns
                    WHERE table_schema = 'public' AND table_name = 'space_children'
                    AND column_name = 'suggested'
                ) THEN
                    ALTER TABLE space_children ADD COLUMN suggested BOOLEAN DEFAULT FALSE;
                END IF;
            END $$;
            "#,
        )
        .execute(&*self.pool)
        .await?;

        sqlx::query(
            r#"
            DO $$
            BEGIN
                IF NOT EXISTS (
                    SELECT 1 FROM information_schema.columns
                    WHERE table_schema = 'public' AND table_name = 'space_children'
                    AND column_name = 'added_by'
                ) THEN
                    ALTER TABLE space_children ADD COLUMN added_by TEXT DEFAULT '';
                END IF;
            END $$;
            "#,
        )
        .execute(&*self.pool)
        .await?;

        sqlx::query(
            r#"
            DO $$
            BEGIN
                IF NOT EXISTS (
                    SELECT 1 FROM information_schema.columns
                    WHERE table_schema = 'public' AND table_name = 'space_children'
                    AND column_name = 'removed_ts'
                ) THEN
                    ALTER TABLE space_children ADD COLUMN removed_ts BIGINT;
                END IF;
            END $$;
            "#,
        )
        .execute(&*self.pool)
        .await?;

        sqlx::query(
            r#"
            CREATE INDEX IF NOT EXISTS idx_space_children_space ON space_children(space_id)
            "#,
        )
        .execute(&*self.pool)
        .await?;

        sqlx::query(
            r#"
            CREATE INDEX IF NOT EXISTS idx_space_children_room ON space_children(room_id)
            "#,
        )
        .execute(&*self.pool)
        .await?;

        // Create space_hierarchy table
        sqlx::query(
            r#"
            CREATE TABLE IF NOT EXISTS space_hierarchy (
                id BIGSERIAL PRIMARY KEY,
                space_id TEXT NOT NULL,
                room_id TEXT NOT NULL,
                parent_space_id TEXT,
                depth INTEGER DEFAULT 0,
                children TEXT[],
                via_servers TEXT[],
                created_ts BIGINT NOT NULL,
                updated_ts BIGINT NOT NULL,
                UNIQUE (space_id, room_id)
            )
            "#,
        )
        .execute(&*self.pool)
        .await?;

        sqlx::query(
            r#"
            CREATE INDEX IF NOT EXISTS idx_space_hierarchy_space ON space_hierarchy(space_id)
            "#,
        )
        .execute(&*self.pool)
        .await?;

        Ok("附加表和列检查完成".to_string())
    }
}

#[derive(Debug, Clone)]
pub struct InitializationReport {
    pub success: bool,
    pub steps: Vec<String>,
    pub errors: Vec<String>,
    pub schema_status: Option<crate::storage::SchemaValidationResult>,
    pub repairs_performed: Vec<String>,
    pub skipped: bool,
}

impl InitializationReport {
    pub fn summary(&self) -> String {
        let mut summary = format!("数据库初始化: success={}", self.success);

        if self.skipped {
            summary.push_str(" (使用缓存)");
        }

        if !self.steps.is_empty() {
            summary.push_str(&format!("\n  已完成步骤 ({})", self.steps.len()));
            for step in &self.steps {
                summary.push_str(&format!("\n    ✓ {}", step));
            }
        }

        if !self.errors.is_empty() {
            summary.push_str(&format!("\n  错误 ({})", self.errors.len()));
            for error in &self.errors {
                summary.push_str(&format!("\n    ✗ {}", error));
            }
        }

        if let Some(ref status) = self.schema_status {
            if !status.is_healthy {
                summary.push_str(&format!(
                    "\n  Schema问题: {} 个表有问题",
                    status.schema_info.len()
                ));
                for info in &status.schema_info {
                    if !info.missing_columns.is_empty() {
                        summary.push_str(&format!(
                            "\n    - {}: 缺少列 {:?}",
                            info.table_name, info.missing_columns
                        ));
                    }
                }
            } else {
                summary.push_str("\n  Schema状态: 健康");
            }
        }

        if !self.repairs_performed.is_empty() {
            summary.push_str(&format!("\n  已修复 ({})", self.repairs_performed.len()));
            for repair in &self.repairs_performed {
                summary.push_str(&format!("\n    + {}", repair));
            }
        }

        summary
    }
}

pub async fn initialize_database(pool: &PgPool) -> Result<(), String> {
    let initializer = DatabaseInitService::new(Arc::new(pool.clone()));

    match initializer.initialize().await {
        Ok(report) => {
            if report.success {
                info!("{}", report.summary());
                Ok(())
            } else {
                error!("数据库初始化失败: {}", report.summary());
                Err(report.errors.join("; "))
            }
        }
        Err(e) => {
            error!("数据库初始化异常: {}", e);
            Err::<(), String>(e.to_string())
        }
    }
}
