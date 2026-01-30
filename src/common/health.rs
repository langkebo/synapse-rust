use serde::{Deserialize, Serialize};
use std::collections::HashMap;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HealthStatus {
    pub status: String,
    pub version: String,
    pub timestamp: i64,
    pub checks: HashMap<String, CheckResult>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CheckResult {
    pub status: String,
    pub message: String,
    pub duration_ms: u64,
}

#[derive(Debug, Clone)]
pub enum HealthCheckLevel {
    Liveness,
    Readiness,
}

#[async_trait::async_trait]
pub trait HealthCheck: Send + Sync {
    async fn check(&self) -> CheckResult;
    fn name(&self) -> &str;
}

pub struct DatabaseHealthCheck {
    pool: sqlx::PgPool,
}

impl DatabaseHealthCheck {
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

pub struct CacheHealthCheck {
    cache: crate::cache::CacheManager,
}

impl CacheHealthCheck {
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

pub struct HealthChecker {
    checks: Vec<Box<dyn HealthCheck>>,
    version: String,
}

impl HealthChecker {
    pub fn new(version: String) -> Self {
        Self {
            checks: Vec::new(),
            version,
        }
    }

    pub fn add_check(&mut self, check: Box<dyn HealthCheck>) {
        self.checks.push(check);
    }

    pub async fn check_liveness(&self) -> HealthStatus {
        self.perform_checks(HealthCheckLevel::Liveness).await
    }

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
    #[ignore = "Requires running PostgreSQL server"]
    async fn test_health_checker_add_check() {
        let mut checker = HealthChecker::default();
        let check = Box::new(DatabaseHealthCheck::new(
            sqlx::PgPool::connect("postgresql://test:test@localhost/test")
                .await
                .expect(
                    "Database connection failed - this test requires a running PostgreSQL server",
                ),
        ));
        checker.add_check(check);
        assert_eq!(checker.checks.len(), 1);
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
