// Re-export from canonical `synapse-common` crate, plus root-specific `CacheHealthCheck`.
// See §5.5 of TECHNICAL_DEBT_OPTIMIZATION_PLAN for workspace mirror consolidation.
pub use synapse_common::health::*;

use crate::cache::CacheManager;

/// 缓存健康检查。
///
/// 检查缓存服务（Redis 或本地）是否正常。
/// Root-specific: depends on `crate::cache::CacheManager` which is not in synapse-common.
pub struct CacheHealthCheck {
    cache: CacheManager,
}

impl CacheHealthCheck {
    pub fn new(cache: CacheManager) -> Self {
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
                    message: format!("Cache read failed: {e}"),
                    duration_ms: start.elapsed().as_millis() as u64,
                },
            },
            Err(e) => CheckResult {
                status: "unhealthy".to_string(),
                message: format!("Cache write failed: {e}"),
                duration_ms: start.elapsed().as_millis() as u64,
            },
        }
    }

    fn name(&self) -> &str {
        "cache"
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_health_status_serialization() {
        let mut checks = std::collections::HashMap::new();
        checks.insert(
            "test".to_string(),
            CheckResult { status: "healthy".to_string(), message: "Test check".to_string(), duration_ms: 10 },
        );

        let status =
            HealthStatus { status: "healthy".to_string(), version: "0.1.0".to_string(), timestamp: 1234567890, checks };

        let json = serde_json::to_string(&status).unwrap();
        assert!(json.contains("healthy"));
    }

    #[tokio::test]
    async fn test_health_checker_creation() {
        let checker = HealthChecker::new("1.0.0".to_string());
        let status = checker.check_liveness().await;
        assert_eq!(status.version, "1.0.0");
    }

    #[tokio::test]
    async fn test_health_checker_default() {
        let checker = HealthChecker::default();
        let status = checker.check_liveness().await;
        assert_eq!(status.version, "0.1.0");
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