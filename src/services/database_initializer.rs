use crate::storage::SchemaValidator;
use sqlx::PgPool;
use std::sync::Arc;
use tracing::{debug, error, info};

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

        if self.environment.is_development() {
            info!("开始数据库初始化...");
        } else {
            debug!("开始数据库初始化...");
        }

        match self.step_connection_test().await {
            Ok(msg) => report.steps.push(msg),
            Err(e) => {
                report.success = false;
                report.errors.push(format!("连接测试失败: {}", e));
                return Ok(report);
            }
        }

        match self.step_migrations().await {
            Ok(msg) => report.steps.push(msg),
            Err(e) => {
                report.success = false;
                report.errors.push(format!("数据库迁移失败: {}", e));
                return Ok(report);
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
        MIGRATOR.run(&*self.pool).await?;
        Ok("数据库迁移执行完成".to_string())
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
