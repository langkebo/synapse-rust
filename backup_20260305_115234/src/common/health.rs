use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// 健康状态响应结构。
///
/// 用于 /_health 端点的 JSON 响应，包含整体状态和各项检查结果。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HealthStatus {
    /// 整体状态：healthy、degraded 或 unhealthy
    pub status: String,
    /// 服务器版本
    pub version: String,
    /// 时间戳
    pub timestamp: i64,
    /// 各项检查结果映射
    pub checks: HashMap<String, CheckResult>,
}

/// 单项检查结果。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CheckResult {
    /// 检查状态：healthy、degraded 或 unhealthy
    pub status: String,
    /// 状态消息
    pub message: String,
    /// 检查耗时（毫秒）
    pub duration_ms: u64,
}

/// 健康检查级别。
///
/// 用于区分存活检查和就绪检查。
#[derive(Debug, Clone)]
pub enum HealthCheckLevel {
    /// 存活检查：服务是否正在运行
    Liveness,
    /// 就绪检查：服务是否准备好处理请求
    Readiness,
}

/// 健康检查 trait。
///
/// 定义健康检查组件必须实现的方法。
#[async_trait::async_trait]
pub trait HealthCheck: Send + Sync {
    /// 执行健康检查。
    async fn check(&self) -> CheckResult;
    /// 获取检查名称。
    fn name(&self) -> &str;
}

/// 数据库健康检查。
///
/// 检查 PostgreSQL 数据库连接是否正常。
pub struct DatabaseHealthCheck {
    /// 数据库连接池
    pool: sqlx::PgPool,
}

impl DatabaseHealthCheck {
    /// 创建新的数据库健康检查实例。
    ///
    /// # 参数
    ///
    /// * `pool` - PostgreSQL 连接池
    pub fn new(pool: sqlx::PgPool) -> Self {
        Self { pool }
    }
}

#[async_trait::async_trait]
impl HealthCheck for DatabaseHealthCheck {
    async fn check(&self) -> CheckResult {
        let start = std::time::Instant::now();

        match sqlx::query("SELECT 1").fetch_one(&self.pool).await {
            Ok(_) => CheckResult {
                status: "healthy".to_string(),
                message: "Database connection successful".to_string(),
                duration_ms: start.elapsed().as_millis() as u64,
            },
            Err(e) => CheckResult {
                status: "unhealthy".to_string(),
                message: format!("Database connection failed: {}", e),
                duration_ms: start.elapsed().as_millis() as u64,
            },
        }
    }

    fn name(&self) -> &str {
        "database"
    }
}

/// 缓存健康检查。
///
/// 检查缓存服务（Redis 或本地）是否正常。
pub struct CacheHealthCheck {
    /// 缓存管理器
    cache: crate::cache::CacheManager,
}

impl CacheHealthCheck {
    /// 创建新的缓存健康检查实例。
    ///
    /// # 参数
    ///
    /// * `cache` - 缓存管理器
    pub fn new(cache: crate::cache::CacheManager) -> Self {
        Self { cache }
    }
}

#[async_trait::async_trait]
impl HealthCheck for CacheHealthCheck {
    async fn check(&self) -> CheckResult {
        let start = std::time::Instant::now();

        match self.cache.set("health_check", "ok", 10).await {
            Ok(_) => match self.cache.get::<String>("health_check").await {
                Ok(Some(value)) if value == "ok" => CheckResult {
                    status: "healthy".to_string(),
                    message: "Cache connection successful".to_string(),
                    duration_ms: start.elapsed().as_millis() as u64,
                },
                Ok(None) => CheckResult {
                    status: "degraded".to_string(),
                    message: "Cache read returned None".to_string(),
                    duration_ms: start.elapsed().as_millis() as u64,
                },
                Ok(Some(_)) => CheckResult {
                    status: "degraded".to_string(),
                    message: "Cache value mismatch".to_string(),
                    duration_ms: start.elapsed().as_millis() as u64,
                },
                Err(e) => CheckResult {
                    status: "unhealthy".to_string(),
                    message: format!("Cache read failed: {}", e),
                    duration_ms: start.elapsed().as_millis() as u64,
                },
            },
            Err(e) => CheckResult {
                status: "unhealthy".to_string(),
                message: format!("Cache write failed: {}", e),
                duration_ms: start.elapsed().as_millis() as u64,
            },
        }
    }

    fn name(&self) -> &str {
        "cache"
    }
}

/// 健康检查器。
///
/// 收集和管理多个健康检查组件，执行综合健康检查。
pub struct HealthChecker {
    /// 健康检查组件列表
    checks: Vec<Box<dyn HealthCheck>>,
    /// 服务器版本
    version: String,
}

impl HealthChecker {
    /// 创建新的健康检查器。
    ///
    /// # 参数
    ///
    /// * `version` - 服务器版本号
    pub fn new(version: String) -> Self {
        Self {
            checks: Vec::new(),
            version,
        }
    }

    /// 添加健康检查组件。
    ///
    /// # 参数
    ///
    /// * `check` - 健康检查组件
    pub fn add_check(&mut self, check: Box<dyn HealthCheck>) {
        self.checks.push(check);
    }

    /// 执行存活检查。
    ///
    /// # 返回值
    ///
    /// 返回 `HealthStatus`，包含存活检查结果
    pub async fn check_liveness(&self) -> HealthStatus {
        self.perform_checks(HealthCheckLevel::Liveness).await
    }

    /// 执行就绪检查。
    ///
    /// # 返回值
    ///
    /// 返回 `HealthStatus`，包含就绪检查结果
    pub async fn check_readiness(&self) -> HealthStatus {
        self.perform_checks(HealthCheckLevel::Readiness).await
    }

    async fn perform_checks(&self, _level: HealthCheckLevel) -> HealthStatus {
        let mut checks = HashMap::new();
        let mut overall_status = "healthy";

        for check in &self.checks {
            let result = check.check().await;
            if result.status != "healthy" {
                overall_status = "unhealthy";
            }
            checks.insert(check.name().to_string(), result);
        }

        HealthStatus {
            status: overall_status.to_string(),
            version: self.version.clone(),
            timestamp: chrono::Utc::now().timestamp(),
            checks,
        }
    }
}

impl Default for HealthChecker {
    fn default() -> Self {
        Self::new("0.1.0".to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_health_status_serialization() {
        let mut checks = HashMap::new();
        checks.insert(
            "test".to_string(),
            CheckResult {
                status: "healthy".to_string(),
                message: "Test check".to_string(),
                duration_ms: 10,
            },
        );

        let status = HealthStatus {
            status: "healthy".to_string(),
            version: "0.1.0".to_string(),
            timestamp: 1234567890,
            checks,
        };

        let json = serde_json::to_string(&status).unwrap();
        assert!(json.contains("healthy"));
    }

    #[tokio::test]
    async fn test_health_checker_creation() {
        let checker = HealthChecker::new("1.0.0".to_string());
        assert_eq!(checker.version, "1.0.0");
    }

    #[tokio::test]
    async fn test_health_checker_default() {
        let checker = HealthChecker::default();
        assert_eq!(checker.version, "0.1.0");
    }

    #[tokio::test]
    async fn test_health_checker_add_check() {
        let mut checker = HealthChecker::default();
        let db_url = std::env::var("TEST_DATABASE_URL").unwrap_or_else(|_| {
            "postgres://synapse:synapse@localhost:5432/synapse_test".to_string()
        });
        if let Ok(pool) = sqlx::PgPool::connect(&db_url).await {
            let check = Box::new(DatabaseHealthCheck::new(pool));
            checker.add_check(check);
            assert_eq!(checker.checks.len(), 1);
        }
    }

    #[tokio::test]
    async fn test_health_checker_check_liveness() {
        let checker = HealthChecker::default();
        let status = checker.check_liveness().await;
        assert_eq!(status.status, "healthy");
        assert_eq!(status.version, "0.1.0");
    }

    #[tokio::test]
    async fn test_health_checker_check_readiness() {
        let checker = HealthChecker::default();
        let status = checker.check_readiness().await;
        assert_eq!(status.status, "healthy");
        assert_eq!(status.version, "0.1.0");
    }
}
