//! Cache health check for the `/health` readiness surface.
//!
//! Wraps `CacheManager` so the readiness endpoint can report the cache
//! (Redis-backed or in-memory fallback) as healthy / degraded / unhealthy.
//! Moved here from the root crate's `src/common/health.rs` facade, which could
//! not be reached from the extracted HTTP crate (B4-5b).

use crate::CacheManager;
use synapse_common::health::{CheckResult, HealthCheck};

/// 缓存健康检查。
///
/// 检查缓存服务（Redis 或本地）是否正常：写入后读回 `"ok"` 才算 healthy。
pub struct CacheHealthCheck {
    cache: CacheManager,
}

impl CacheHealthCheck {
    /// See [`new`].
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
    use crate::error::CacheConfig;

    #[test]
    fn name_is_cache() {
        let check = CacheHealthCheck::new(CacheManager::new(&CacheConfig::default()));
        assert_eq!(check.name(), "cache");
    }

    #[tokio::test]
    async fn in_memory_cache_reports_healthy() {
        let check = CacheHealthCheck::new(CacheManager::new(&CacheConfig::default()));
        let result = check.check().await;
        assert_eq!(result.status, "healthy", "message: {}", result.message);
    }
}
