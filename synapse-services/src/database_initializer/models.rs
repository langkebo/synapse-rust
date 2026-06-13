use sqlx::PgPool;
use std::sync::Arc;
use synapse_storage::SchemaValidator;
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_environment_from_string_development() {
        assert_eq!(Environment::from("development".to_string()), Environment::Development);
        assert_eq!(Environment::from("dev".to_string()), Environment::Development);
        assert_eq!(Environment::from("".to_string()), Environment::Development);
        assert_eq!(Environment::from("unknown".to_string()), Environment::Development);
    }

    #[test]
    fn test_environment_from_string_production() {
        assert_eq!(Environment::from("production".to_string()), Environment::Production);
        assert_eq!(Environment::from("prod".to_string()), Environment::Production);
        assert_eq!(Environment::from("release".to_string()), Environment::Production);
    }

    #[test]
    fn test_environment_is_development() {
        assert!(Environment::Development.is_development());
        assert!(!Environment::Production.is_development());
    }

    #[test]
    fn test_environment_from_env_default() {
        std::env::remove_var("RUST_ENV");
        assert_eq!(Environment::from_env(), Environment::Development);
    }

    #[test]
    fn test_environment_from_env_production() {
        std::env::set_var("RUST_ENV", "production");
        assert_eq!(Environment::from_env(), Environment::Production);
        std::env::remove_var("RUST_ENV");
    }

    #[test]
    fn test_environment_clone() {
        let env = Environment::Production;
        assert_eq!(env.clone(), Environment::Production);
    }

    #[test]
    fn test_database_init_mode_equality() {
        assert_eq!(DatabaseInitMode::Auto, DatabaseInitMode::Auto);
        assert_ne!(DatabaseInitMode::Auto, DatabaseInitMode::Strict);
        assert_ne!(DatabaseInitMode::Strict, DatabaseInitMode::Compatible);
    }

    #[test]
    fn test_database_init_mode_copy() {
        let mode = DatabaseInitMode::Strict;
        let copied = mode;
        assert_eq!(mode, copied);
    }

    #[test]
    fn test_initialization_report_success_summary() {
        let report = InitializationReport {
            is_success: true,
            steps: vec!["Step 1".to_string(), "Step 2".to_string()],
            errors: vec![],
            schema_status: None,
            repairs_performed: vec![],
            skipped: false,
        };
        let summary = report.summary();
        assert!(summary.contains("success=true"));
        assert!(summary.contains("Step 1"));
        assert!(summary.contains("Step 2"));
    }

    #[test]
    fn test_initialization_report_failure_with_errors() {
        let report = InitializationReport {
            is_success: false,
            steps: vec!["Step 1".to_string()],
            errors: vec!["Connection failed".to_string(), "Timeout".to_string()],
            schema_status: None,
            repairs_performed: vec![],
            skipped: false,
        };
        let summary = report.summary();
        assert!(summary.contains("success=false"));
        assert!(summary.contains("Connection failed"));
        assert!(summary.contains("Timeout"));
        assert!(summary.contains("错误 (2)"));
    }

    #[test]
    fn test_initialization_report_skipped() {
        let report = InitializationReport {
            is_success: true,
            steps: vec![],
            errors: vec![],
            schema_status: None,
            repairs_performed: vec![],
            skipped: true,
        };
        let summary = report.summary();
        assert!(summary.contains("使用缓存"));
        assert!(summary.contains("success=true"));
    }

    #[test]
    fn test_initialization_report_with_repairs() {
        let report = InitializationReport {
            is_success: true,
            steps: vec!["Init complete".to_string()],
            errors: vec![],
            schema_status: None,
            repairs_performed: vec!["Fixed index".to_string(), "Added column".to_string()],
            skipped: false,
        };
        let summary = report.summary();
        assert!(summary.contains("已修复 (2)"));
        assert!(summary.contains("Fixed index"));
        assert!(summary.contains("Added column"));
    }

    #[test]
    fn test_initialization_report_empty() {
        let report = InitializationReport {
            is_success: true,
            steps: vec![],
            errors: vec![],
            schema_status: None,
            repairs_performed: vec![],
            skipped: false,
        };
        let summary = report.summary();
        assert!(summary.contains("success=true"));
        assert!(!summary.contains("已完成步骤"));
        assert!(!summary.contains("错误"));
    }
}
