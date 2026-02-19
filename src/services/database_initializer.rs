use crate::storage::SchemaValidator;
use sqlx::PgPool;
use std::sync::Arc;
use tracing::{debug, error, info, warn};

const DEFAULT_CACHE_TTL_SECONDS: i64 = 3600;

static MIGRATOR: sqlx::migrate::Migrator = sqlx::migrate!("./migrations");

#[derive(Debug, Clone, PartialEq)]
pub enum Environment {
    Development,
    Production,
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
            schema_validator: SchemaValidator::new(&pool),
            cache_ttl_seconds: DEFAULT_CACHE_TTL_SECONDS,
            environment: Environment::from_env(),
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
            schema_validator: SchemaValidator::new(&pool),
            cache_ttl_seconds: ttl_seconds,
            environment: Environment::from_env(),
        }
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

        match self.step_create_e2ee_tables().await {
            Ok(msg) => report.steps.push(msg),
            Err(e) => {
                report.success = false;
                report.errors.push(format!("E2EE表创建失败: {}", e));
            }
        }

        match self.step_ensure_additional_tables().await {
            Ok(msg) => report.steps.push(msg),
            Err(e) => {
                report.success = false;
                report.errors.push(format!("附加表检查失败: {}", e));
            }
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

        if let Some(ref status) = report.schema_status {
            if !status.is_healthy {
                match self.step_schema_repair().await {
                    Ok(repairs) => report.repairs_performed = repairs,
                    Err(e) => {
                        report.success = false;
                        report.errors.push(format!("Schema修复失败: {}", e));
                    }
                };
            }
        }

        match self.step_index_validation().await {
            Ok(issues) => {
                if !issues.is_empty() {
                    if self.environment.is_development() {
                        info!("发现 {} 个缺失的索引，正在创建...", issues.len());
                    } else {
                        debug!("发现 {} 个缺失的索引，正在创建...", issues.len());
                    }
                    match self.step_create_indexes().await {
                        Ok(created) => report
                            .steps
                            .push(format!("已创建 {} 个索引", created.len())),
                        Err(e) => report.errors.push(format!("索引创建失败: {}", e)),
                    };
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

    async fn check_cache_valid(&self) -> Result<bool, sqlx::Error> {
        let result = sqlx::query_as::<_, (i64,)>(
            r#"
            SELECT value::BIGINT
            FROM db_metadata WHERE key = 'last_init_ts'
            "#,
        )
        .fetch_optional(&*self.pool)
        .await?;

        if let Some(row) = result {
            let last_init_ts: i64 = row.0;
            let now = chrono::Utc::now().timestamp();
            let elapsed = now - last_init_ts;

            if elapsed > 0 && elapsed < self.cache_ttl_seconds {
                return Ok(true);
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
            info!("使用内置迁移...");
            match MIGRATOR.run(&*self.pool).await {
                Ok(_) => {
                    info!("内置迁移执行成功");
                    return Ok("数据库迁移执行完成 (内置模式)".to_string());
                }
                Err(e) => {
                    error!("内置迁移执行失败: {}", e);
                    return Err(sqlx::Error::Configuration(e.to_string().into()));
                }
            }
        };

        info!("使用运行时迁移文件: {:?}", migrations_dir);
        self.run_runtime_migrations(migrations_dir).await
    }

    async fn ensure_schema_migrations_table(&self) -> Result<(), sqlx::Error> {
        let create_table_sql = r#"
            CREATE TABLE IF NOT EXISTS schema_migrations (
                version VARCHAR(255) PRIMARY KEY,
                checksum VARCHAR(64),
                executed_at TIMESTAMP WITH TIME ZONE NOT NULL DEFAULT NOW(),
                execution_time_ms BIGINT,
                success BOOLEAN NOT NULL DEFAULT TRUE
            )
        "#;
        
        sqlx::raw_sql(create_table_sql).execute(&*self.pool).await?;
        info!("schema_migrations 表已就绪");
        Ok(())
    }

    async fn is_migration_executed(&self, version: &str) -> Result<bool, sqlx::Error> {
        let result: Option<(bool,)> = sqlx::query_as(
            "SELECT success FROM schema_migrations WHERE version = $1"
        )
        .bind(version)
        .fetch_optional(&*self.pool)
        .await?;
        
        Ok(result.map(|(success,)| success).unwrap_or(false))
    }

    async fn record_migration(&self, version: &str, checksum: &str, execution_time_ms: i64, success: bool) -> Result<(), sqlx::Error> {
        sqlx::query(
            r#"INSERT INTO schema_migrations (version, checksum, execution_time_ms, success)
               VALUES ($1, $2, $3, $4)
               ON CONFLICT (version) DO UPDATE SET
                   checksum = EXCLUDED.checksum,
                   executed_at = NOW(),
                   execution_time_ms = EXCLUDED.execution_time_ms,
                   success = EXCLUDED.success"#
        )
        .bind(version)
        .bind(checksum)
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
            .filter(|path| path.extension().is_some_and(|ext| ext == "sql"))
            .collect();
        
        migration_files.sort();
        
        info!("发现 {} 个迁移文件", migration_files.len());

        let mut success_count = 0;
        let mut skip_count = 0;
        let mut error_count = 0;

        for migration_file in migration_files {
            let filename = migration_file.file_name()
                .and_then(|n| n.to_str())
                .unwrap_or("unknown");
            
            let version = filename.split('_').next().unwrap_or(filename);
            
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
            let start_time = std::time::Instant::now();
            let statements = self.split_sql_statements(&sql);
            let mut file_success = true;

            for statement in statements {
                let trimmed = statement.trim();
                if trimmed.is_empty() || trimmed.starts_with("--") {
                    continue;
                }

                match sqlx::raw_sql(trimmed).execute(&*self.pool).await {
                    Ok(_) => {}
                    Err(e) => {
                        let err_str = e.to_string();
                        if err_str.contains("already exists") || 
                           err_str.contains("duplicate key value") {
                        } else {
                            let preview: String = trimmed.chars().take(100).collect();
                            warn!("迁移 {} 语句执行警告: {} - {}", filename, preview, e);
                            file_success = false;
                        }
                    }
                }
            }

            let execution_time_ms = start_time.elapsed().as_millis() as i64;
            
            if let Err(e) = self.record_migration(version, &checksum, execution_time_ms, file_success).await {
                warn!("记录迁移状态失败 {}: {}", filename, e);
            }

            if file_success {
                info!("迁移 {} 执行成功 ({}ms)", filename, execution_time_ms);
                success_count += 1;
            } else {
                skip_count += 1;
            }
        }

        info!("迁移完成: 成功={}, 跳过={}, 错误={}", success_count, skip_count, error_count);
        Ok(format!("数据库迁移执行完成 (成功: {}, 跳过: {}, 错误: {})", success_count, skip_count, error_count))
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

    async fn step_schema_repair(&self) -> Result<Vec<String>, sqlx::Error> {
        self.schema_validator.repair_missing_columns().await
    }

    async fn step_index_validation(&self) -> Result<Vec<String>, sqlx::Error> {
        self.schema_validator.validate_indexes().await
    }

    async fn step_create_indexes(&self) -> Result<Vec<String>, sqlx::Error> {
        self.schema_validator.create_missing_indexes().await
    }

    async fn step_create_e2ee_tables(&self) -> Result<String, sqlx::Error> {
        sqlx::query(
            r#"
            CREATE TABLE IF NOT EXISTS device_keys (
                id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
                user_id VARCHAR(255) NOT NULL,
                device_id VARCHAR(255) NOT NULL,
                display_name VARCHAR(255),
                algorithm VARCHAR(100),
                key_id VARCHAR(255) NOT NULL,
                public_key TEXT NOT NULL,
                signatures JSONB,
                created_at TIMESTAMP WITH TIME ZONE NOT NULL DEFAULT NOW(),
                updated_at TIMESTAMP WITH TIME ZONE NOT NULL DEFAULT NOW(),
                ts_updated_ms BIGINT NOT NULL,
                key_json JSONB NOT NULL DEFAULT '{}',
                ts_added_ms BIGINT NOT NULL,
                ts_last_accessed BIGINT NOT NULL,
                verified BOOLEAN DEFAULT FALSE,
                blocked BOOLEAN DEFAULT FALSE,
                UNIQUE (user_id, device_id, key_id),
                FOREIGN KEY (device_id, user_id) REFERENCES devices(device_id, user_id) ON DELETE CASCADE
            )
            "#,
        )
        .execute(&*self.pool)
        .await?;

        sqlx::query(
            r#"
            CREATE INDEX IF NOT EXISTS idx_device_keys_user ON device_keys(user_id)
            "#,
        )
        .execute(&*self.pool)
        .await?;

        sqlx::query(
            r#"
            CREATE INDEX IF NOT EXISTS idx_device_keys_device ON device_keys(device_id)
            "#,
        )
        .execute(&*self.pool)
        .await?;

        sqlx::query(
            r#"
            CREATE INDEX IF NOT EXISTS idx_device_keys_key_id ON device_keys(key_id)
            "#,
        )
        .execute(&*self.pool)
        .await?;

        sqlx::query(
            r#"
            CREATE INDEX IF NOT EXISTS idx_device_keys_verified ON device_keys(verified) WHERE verified = TRUE
            "#,
        )
        .execute(&*self.pool)
        .await?;

        sqlx::query(
            r#"
            CREATE INDEX IF NOT EXISTS idx_device_keys_ts ON device_keys(ts_last_accessed)
            "#,
        )
        .execute(&*self.pool)
        .await?;

        Ok("E2EE设备密钥表创建完成".to_string())
    }

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

        // Ensure is_guest column exists in users table
        sqlx::query("ALTER TABLE users ADD COLUMN IF NOT EXISTS is_guest BOOLEAN DEFAULT FALSE")
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
