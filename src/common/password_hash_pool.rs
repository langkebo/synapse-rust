use std::sync::Arc;
use std::time::Instant;

use argon2::password_hash::{PasswordHash, PasswordHasher, PasswordVerifier, SaltString};
use argon2::Argon2;
use rand::rngs::OsRng;
use tokio::sync::Semaphore;
use tokio::task::JoinHandle;

use crate::common::argon2_config::Argon2Config;
use crate::common::metrics::{Counter, Gauge, Histogram, MetricsCollector};

static PASSWORD_HASH_POOL: std::sync::OnceLock<PasswordHashPool> = std::sync::OnceLock::new();

#[derive(Debug, Clone)]
pub struct PasswordHashMetrics {
    pub total_hash_operations: Counter,
    pub total_verify_operations: Counter,
    pub hash_duration_ms: Histogram,
    pub verify_duration_ms: Histogram,
    pub active_operations: Gauge,
    pub queued_operations: Counter,
    pub rejected_operations: Counter,
    pub pool_exhaustion_count: Counter,
}

impl PasswordHashMetrics {
    pub fn new(collector: &MetricsCollector) -> Self {
        Self {
            total_hash_operations: collector.register_counter("password_hash_total".to_string()),
            total_verify_operations: collector.register_counter("password_verify_total".to_string()),
            hash_duration_ms: collector.register_histogram("password_hash_duration_ms".to_string()),
            verify_duration_ms: collector.register_histogram("password_verify_duration_ms".to_string()),
            active_operations: collector.register_gauge("password_hash_active_operations".to_string()),
            queued_operations: collector.register_counter("password_hash_queued_total".to_string()),
            rejected_operations: collector.register_counter("password_hash_rejected_total".to_string()),
            pool_exhaustion_count: collector.register_counter("password_hash_pool_exhausted_total".to_string()),
        }
    }
}

#[derive(Debug, Clone)]
pub struct PasswordHashPoolConfig {
    pub max_concurrent: usize,
    pub queue_size: usize,
    pub thread_pool_size: usize,
    pub hash_timeout_ms: u64,
}

impl Default for PasswordHashPoolConfig {
    fn default() -> Self {
        Self {
            max_concurrent: 4,
            queue_size: 100,
            thread_pool_size: 2,
            hash_timeout_ms: 5000,
        }
    }
}

pub struct PasswordHashPool {
    semaphore: Arc<Semaphore>,
    config: PasswordHashPoolConfig,
    metrics: PasswordHashMetrics,
    argon2_config: Argon2Config,
    argon2: Argon2<'static>,
}

impl PasswordHashPool {
    pub fn new(config: PasswordHashPoolConfig, argon2_config: Argon2Config) -> Self {
        let metrics_collector = MetricsCollector::new();
        Self::with_metrics(config, argon2_config, &metrics_collector)
    }

    pub fn semaphore(&self) -> &Arc<Semaphore> {
        &self.semaphore
    }

    pub fn with_metrics(
        config: PasswordHashPoolConfig,
        argon2_config: Argon2Config,
        metrics_collector: &MetricsCollector,
    ) -> Self {
        let params = argon2_config
            .to_argon2_params()
            .expect("Invalid Argon2 config");
        let argon2 = Argon2::new(argon2::Algorithm::Argon2id, argon2::Version::V0x13, params);

        Self {
            semaphore: Arc::new(Semaphore::new(config.max_concurrent)),
            config,
            metrics: PasswordHashMetrics::new(metrics_collector),
            argon2_config,
            argon2,
        }
    }

    pub fn initialize_global(config: PasswordHashPoolConfig, argon2_config: Argon2Config) {
        let pool = Self::new(config, argon2_config);
        let _ = PASSWORD_HASH_POOL.set(pool);
    }

    pub fn get_global() -> Option<&'static PasswordHashPool> {
        PASSWORD_HASH_POOL.get()
    }

    pub fn get_or_init_default() -> &'static PasswordHashPool {
        PASSWORD_HASH_POOL.get_or_init(|| {
            Self::new(PasswordHashPoolConfig::default(), Argon2Config::default())
        })
    }

    pub fn metrics(&self) -> &PasswordHashMetrics {
        &self.metrics
    }

    pub fn available_permits(&self) -> usize {
        self.semaphore.available_permits()
    }

    pub fn max_concurrent(&self) -> usize {
        self.config.max_concurrent
    }

    pub async fn hash_password(&self, password: &str) -> Result<String, PasswordHashError> {
        let start = Instant::now();
        self.metrics.active_operations.inc();

        let acquire_result = self.semaphore.clone().try_acquire_owned();

        let _permit = match acquire_result {
            Ok(permit) => permit,
            Err(_) => {
                self.metrics.rejected_operations.inc();
                self.metrics.pool_exhaustion_count.inc();
                return Err(PasswordHashError::PoolExhausted);
            }
        };

        self.metrics.total_hash_operations.inc();

        let password = password.to_string();
        let argon2 = self.argon2.clone();

        let result = tokio::task::spawn_blocking(move || {
            let salt = SaltString::generate(&mut OsRng);
            argon2
                .hash_password(password.as_bytes(), &salt)
                .map(|hash| hash.to_string())
                .map_err(|e| PasswordHashError::HashFailed(e.to_string()))
        })
        .await
        .map_err(|e| PasswordHashError::TaskJoinError(e.to_string()))??;

        let duration = start.elapsed().as_millis() as f64;
        self.metrics.hash_duration_ms.observe(duration);
        self.metrics.active_operations.dec();

        Ok(result)
    }

    pub async fn verify_password(
        &self,
        password: &str,
        hash: &str,
    ) -> Result<bool, PasswordHashError> {
        let start = Instant::now();
        self.metrics.active_operations.inc();

        let acquire_result = self.semaphore.clone().try_acquire_owned();

        let _permit = match acquire_result {
            Ok(permit) => permit,
            Err(_) => {
                self.metrics.rejected_operations.inc();
                self.metrics.pool_exhaustion_count.inc();
                return Err(PasswordHashError::PoolExhausted);
            }
        };

        self.metrics.total_verify_operations.inc();

        let password = password.to_string();
        let hash = hash.to_string();

        let result = tokio::task::spawn_blocking(move || {
            let parsed_hash = PasswordHash::new(&hash)
                .map_err(|e| PasswordHashError::InvalidHashFormat(e.to_string()))?;

            Ok(Argon2::default()
                .verify_password(password.as_bytes(), &parsed_hash)
                .is_ok())
        })
        .await
        .map_err(|e| PasswordHashError::TaskJoinError(e.to_string()))??;

        let duration = start.elapsed().as_millis() as f64;
        self.metrics.verify_duration_ms.observe(duration);
        self.metrics.active_operations.dec();

        Ok(result)
    }

    pub async fn hash_password_blocking(
        &self,
        password: &str,
    ) -> Result<String, PasswordHashError> {
        self.hash_password(password).await
    }

    pub async fn verify_password_blocking(
        &self,
        password: &str,
        hash: &str,
    ) -> Result<bool, PasswordHashError> {
        self.verify_password(password, hash).await
    }

    pub fn try_hash_password(&self, password: &str) -> Option<JoinHandle<Result<String, PasswordHashError>>> {
        if self.semaphore.available_permits() == 0 {
            self.metrics.rejected_operations.inc();
            return None;
        }

        let password = password.to_string();
        let argon2 = self.argon2.clone();
        let metrics = self.metrics.clone();

        metrics.queued_operations.inc();

        Some(tokio::spawn(async move {
            let start = Instant::now();
            metrics.active_operations.inc();
            metrics.total_hash_operations.inc();

            let result = tokio::task::spawn_blocking(move || {
                let salt = SaltString::generate(&mut OsRng);
                argon2
                    .hash_password(password.as_bytes(), &salt)
                    .map(|hash| hash.to_string())
                    .map_err(|e| PasswordHashError::HashFailed(e.to_string()))
            })
            .await
            .map_err(|e| PasswordHashError::TaskJoinError(e.to_string()))??;

            let duration = start.elapsed().as_millis() as f64;
            metrics.hash_duration_ms.observe(duration);
            metrics.active_operations.dec();

            Ok(result)
        }))
    }
}

impl Clone for PasswordHashPool {
    fn clone(&self) -> Self {
        Self {
            semaphore: Arc::clone(&self.semaphore),
            config: self.config.clone(),
            metrics: self.metrics.clone(),
            argon2_config: self.argon2_config,
            argon2: self.argon2.clone(),
        }
    }
}

#[derive(Debug, thiserror::Error)]
pub enum PasswordHashError {
    #[error("Password hash pool exhausted, too many concurrent operations")]
    PoolExhausted,

    #[error("Hash operation failed: {0}")]
    HashFailed(String),

    #[error("Invalid hash format: {0}")]
    InvalidHashFormat(String),

    #[error("Task join error: {0}")]
    TaskJoinError(String),

    #[error("Operation timed out")]
    Timeout,

    #[error("Queue is full")]
    QueueFull,
}

pub async fn hash_password_pooled(password: &str) -> Result<String, PasswordHashError> {
    let pool = PasswordHashPool::get_or_init_default();
    pool.hash_password(password).await
}

pub async fn verify_password_pooled(password: &str, hash: &str) -> Result<bool, PasswordHashError> {
    let pool = PasswordHashPool::get_or_init_default();
    pool.verify_password(password, hash).await
}

pub fn get_pool_metrics() -> Option<&'static PasswordHashMetrics> {
    PasswordHashPool::get_global().map(|p| p.metrics())
}

pub fn get_pool_status() -> PoolStatus {
    if let Some(pool) = PasswordHashPool::get_global() {
        PoolStatus {
            available_permits: pool.available_permits(),
            max_concurrent: pool.max_concurrent(),
            active_operations: pool.metrics().active_operations.get() as usize,
        }
    } else {
        PoolStatus::default()
    }
}

#[derive(Debug, Clone, Default)]
pub struct PoolStatus {
    pub available_permits: usize,
    pub max_concurrent: usize,
    pub active_operations: usize,
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;

    fn create_test_pool() -> PasswordHashPool {
        let config = PasswordHashPoolConfig {
            max_concurrent: 2,
            queue_size: 10,
            thread_pool_size: 1,
            hash_timeout_ms: 5000,
        };
        let argon2_config = Argon2Config::new(65536, 3, 1).unwrap();
        PasswordHashPool::new(config, argon2_config)
    }

    #[tokio::test]
    async fn test_hash_password_basic() {
        let pool = create_test_pool();
        let password = "test_password_123";

        let hash = pool.hash_password(password).await.unwrap();
        assert!(hash.starts_with("$argon2id$"));
        assert!(!hash.is_empty());
    }

    #[tokio::test]
    async fn test_verify_password_correct() {
        let pool = create_test_pool();
        let password = "correct_password";

        let hash = pool.hash_password(password).await.unwrap();
        let result = pool.verify_password(password, &hash).await.unwrap();
        assert!(result);
    }

    #[tokio::test]
    async fn test_verify_password_incorrect() {
        let pool = create_test_pool();
        let password = "correct_password";
        let wrong_password = "wrong_password";

        let hash = pool.hash_password(password).await.unwrap();
        let result = pool.verify_password(wrong_password, &hash).await.unwrap();
        assert!(!result);
    }

    #[tokio::test]
    async fn test_concurrent_hash_operations() {
        let pool = create_test_pool();
        let mut handles = vec![];

        for i in 0..4 {
            let pool_clone = pool.clone();
            let password = format!("password_{}", i);
            handles.push(tokio::spawn(async move {
                pool_clone.hash_password(&password).await
            }));
        }

        let results: Vec<_> = futures::future::join_all(handles).await;
        let successful: Vec<_> = results.into_iter().filter_map(|r| r.ok()).filter(|r| r.is_ok()).collect();

        assert!(successful.len() >= 2, "At least 2 operations should succeed");
    }

    #[tokio::test]
    async fn test_pool_exhaustion() {
        let config = PasswordHashPoolConfig {
            max_concurrent: 1,
            queue_size: 1,
            thread_pool_size: 1,
            hash_timeout_ms: 1000,
        };
        let argon2_config = Argon2Config::new(65536, 3, 1).unwrap();
        let pool = Arc::new(PasswordHashPool::new(config, argon2_config));

        let pool_clone = pool.clone();
        let handle1 = tokio::spawn(async move {
            pool_clone.hash_password("password1").await
        });

        tokio::time::sleep(Duration::from_millis(10)).await;

        let pool_clone = pool.clone();
        let handle2 = tokio::spawn(async move {
            pool_clone.hash_password("password2").await
        });

        let result1 = handle1.await.unwrap();
        let result2 = handle2.await.unwrap();

        assert!(result1.is_ok() || result2.is_ok());
    }

    #[tokio::test]
    async fn test_metrics_tracking() {
        let pool = create_test_pool();

        let initial_hash_count = pool.metrics().total_hash_operations.get();
        let initial_verify_count = pool.metrics().total_verify_operations.get();

        let hash = pool.hash_password("test").await.unwrap();
        pool.verify_password("test", &hash).await.unwrap();

        assert_eq!(pool.metrics().total_hash_operations.get(), initial_hash_count + 1);
        assert_eq!(pool.metrics().total_verify_operations.get(), initial_verify_count + 1);
    }

    #[tokio::test]
    async fn test_hash_duration_metrics() {
        let pool = create_test_pool();

        pool.hash_password("test_password").await.unwrap();

        let count = pool.metrics().hash_duration_ms.get_count();
        assert!(count > 0);

        let avg = pool.metrics().hash_duration_ms.get_avg();
        assert!(avg > 0.0);
    }

    #[tokio::test]
    async fn test_global_pool_initialization() {
        let config = PasswordHashPoolConfig::default();
        let argon2_config = Argon2Config::default();
        PasswordHashPool::initialize_global(config, argon2_config);

        let pool = PasswordHashPool::get_global();
        assert!(pool.is_some());

        let hash = hash_password_pooled("global_test").await.unwrap();
        assert!(hash.starts_with("$argon2id$"));
    }

    #[tokio::test]
    async fn test_pool_status() {
        let pool = create_test_pool();

        assert_eq!(pool.available_permits(), 2);
        assert_eq!(pool.max_concurrent(), 2);
    }

    #[tokio::test]
    async fn test_invalid_hash_format() {
        let pool = create_test_pool();

        let result = pool.verify_password("password", "invalid_hash").await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_empty_password() {
        let pool = create_test_pool();

        let hash = pool.hash_password("").await.unwrap();
        let result = pool.verify_password("", &hash).await.unwrap();
        assert!(result);
    }

    #[tokio::test]
    async fn test_concurrent_verify_operations() {
        let pool = create_test_pool();
        let password = "shared_password";
        let hash = pool.hash_password(password).await.unwrap();

        let mut handles = vec![];
        for _ in 0..4 {
            let pool_clone = pool.clone();
            let hash_clone = hash.clone();
            handles.push(tokio::spawn(async move {
                pool_clone.verify_password(password, &hash_clone).await
            }));
        }

        let results: Vec<_> = futures::future::join_all(handles).await;
        let successful: Vec<_> = results.into_iter().filter_map(|r| r.ok()).filter_map(|r| r.ok()).collect();
        
        assert!(successful.len() >= 2, "At least 2 verify operations should succeed");
        for result in successful {
            assert!(result, "All successful verifications should return true");
        }
    }

    #[tokio::test]
    async fn test_pool_clone_consistency() {
        let pool1 = create_test_pool();
        let pool2 = pool1.clone();

        let hash1 = pool1.hash_password("password").await.unwrap();
        let verify_result = pool2.verify_password("password", &hash1).await.unwrap();

        assert!(verify_result);
    }
}
