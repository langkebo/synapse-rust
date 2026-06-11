use synapse_storage::SchemaValidator;
use sqlx::PgPool;
use std::sync::Arc;
use tracing::{error, info};

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
        std::env::var("RUST_ENV").unwrap_or_else(|_| "development".to_string()).to_lowercase().into()
    }

    pub fn is_development(&self) -> bool {
        self == &Self::Development
    }
}

impl From<String> for Environment {
    fn from(s: String) -> Self {
        match s.as_str() {
            "prod" | "production" | "release" => Self::Production,
            _ => Self::Development,
        }
    }
}

/// Service for database initialization, validation and repair operations.
/// Handles database schema verification, index creation, and cache management.
pub struct DatabaseInitService {
    pub(crate) pool: Arc<PgPool>,
    pub(crate) schema_validator: SchemaValidator,
    pub(crate) cache_ttl_seconds: i64,
    pub(crate) environment: Environment,
    pub(crate) mode: DatabaseInitMode,
}

#[derive(Debug, Clone)]
pub struct InitializationReport {
    pub is_success: bool,
    pub steps: Vec<String>,
    pub errors: Vec<String>,
    pub schema_status: Option<synapse_storage::SchemaValidationResult>,
    pub repairs_performed: Vec<String>,
    pub skipped: bool,
}

impl InitializationReport {
    pub fn summary(&self) -> String {
        let mut summary = format!("数据库初始化: success={}", self.is_success);

        if self.skipped {
            summary.push_str(" (使用缓存)");
        }

        if !self.steps.is_empty() {
            summary.push_str(&format!("\n  已完成步骤 ({})", self.steps.len()));
            for step in &self.steps {
                summary.push_str(&format!("\n    ✓ {step}"));
            }
        }

        if !self.errors.is_empty() {
            summary.push_str(&format!("\n  错误 ({})", self.errors.len()));
            for error in &self.errors {
                summary.push_str(&format!("\n    ✗ {error}"));
            }
        }

        if let Some(ref status) = self.schema_status {
            if !status.is_healthy {
                summary.push_str(&format!("\n  Schema问题: {} 个表有问题", status.schema_info.len()));
                for info in &status.schema_info {
                    if !info.missing_columns.is_empty() {
                        summary.push_str(&format!("\n    - {}: 缺少列 {:?}", info.table_name, info.missing_columns));
                    }
                }
            } else {
                summary.push_str("\n  Schema状态: 健康");
            }
        }

        if !self.repairs_performed.is_empty() {
            summary.push_str(&format!("\n  已修复 ({})", self.repairs_performed.len()));
            for repair in &self.repairs_performed {
                summary.push_str(&format!("\n    + {repair}"));
            }
        }

        summary
    }
}

pub async fn initialize_database(pool: &PgPool) -> Result<(), String> {
    let initializer = DatabaseInitService::new(Arc::new(pool.clone()));

    match initializer.initialize().await {
        Ok(report) => {
            if report.is_success {
                info!(
                    success = report.is_success,
                    skipped = report.skipped,
                    step_count = report.steps.len(),
                    repair_count = report.repairs_performed.len(),
                    summary = %report.summary(),
                    "数据库初始化成功"
                );
                Ok(())
            } else {
                error!(summary = %report.summary(), error_count = report.errors.len(), "数据库初始化失败");
                Err(report.errors.join("; "))
            }
        }
        Err(e) => {
            error!(error = %e, "数据库初始化异常");
            Err::<(), String>(e.to_string())
        }
    }
}
