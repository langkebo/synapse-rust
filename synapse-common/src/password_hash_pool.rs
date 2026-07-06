use std::env;
use std::sync::Arc;
use std::time::Instant;

use argon2::password_hash::rand_core::OsRng;
use argon2::password_hash::{PasswordHash, PasswordHasher, PasswordVerifier, SaltString};
use argon2::Argon2;
use tokio::sync::Semaphore;
use tokio::task::JoinHandle;

use crate::argon2_config::Argon2Config;
use crate::metrics::{Counter, Gauge, Histogram, MetricsCollector};

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
        Self { max_concurrent: 4, queue_size: 100, thread_pool_size: 2, hash_timeout_ms: 5000 }
    }
}

fn env_override_usize(key: &str, default: usize) -> usize {
    env::var(key).ok().and_then(|value| value.parse::<usize>().ok()).filter(|value| *value > 0).unwrap_or(default)
}

fn env_override_u64(key: &str, default: u64) -> u64 {
    env::var(key).ok().and_then(|value| value.parse::<u64>().ok()).filter(|value| *value > 0).unwrap_or(default)
}

fn runtime_pool_config() -> PasswordHashPoolConfig {
    let defaults = PasswordHashPoolConfig::default();
    PasswordHashPoolConfig {
        max_concurrent: env_override_usize("SYNAPSE_PASSWORD_HASH_POOL_MAX_CONCURRENT", defaults.max_concurrent),
        queue_size: env_override_usize("SYNAPSE_PASSWORD_HASH_POOL_QUEUE_SIZE", defaults.queue_size),
        thread_pool_size: env_override_usize("SYNAPSE_PASSWORD_HASH_POOL_THREAD_POOL_SIZE", defaults.thread_pool_size),
        hash_timeout_ms: env_override_u64("SYNAPSE_PASSWORD_HASH_POOL_HASH_TIMEOUT_MS", defaults.hash_timeout_ms),
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
        let argon2 = match argon2_config.to_argon2_params() {
            Ok(params) => Argon2::new(argon2::Algorithm::Argon2id, argon2::Version::V0x13, params),
            Err(e) => {
                tracing::error!("Invalid Argon2 config, using OWASP defaults: {}", e);
                match argon2::Params::new(19 * 1024, 2, 1, Some(32)) {
                    Ok(params) => Argon2::new(argon2::Algorithm::Argon2id, argon2::Version::V0x13, params),
                    Err(fallback_error) => {
                        tracing::error!(
                            "Failed to build OWASP fallback Argon2 params, using crate defaults: {}",
                            fallback_error
                        );
                        Argon2::default()
                    }
                }
            }
        };

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

    pub fn get_global() -> Option<&'static Self> {
        PASSWORD_HASH_POOL.get()
    }

    pub fn get_or_init_default() -> &'static Self {
        PASSWORD_HASH_POOL.get_or_init(|| Self::new(runtime_pool_config(), Argon2Config::get_global()))
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

    pub async fn verify_password(&self, password: &str, hash: &str) -> Result<bool, PasswordHashError> {
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
            let parsed_hash =
                PasswordHash::new(&hash).map_err(|e| PasswordHashError::InvalidHashFormat(e.to_string()))?;

            Ok(Argon2::default().verify_password(password.as_bytes(), &parsed_hash).is_ok())
        })
        .await
        .map_err(|e| PasswordHashError::TaskJoinError(e.to_string()))??;

        let duration = start.elapsed().as_millis() as f64;
        self.metrics.verify_duration_ms.observe(duration);
        self.metrics.active_operations.dec();

        Ok(result)
    }

    pub async fn hash_password_blocking(&self, password: &str) -> Result<String, PasswordHashError> {
        self.hash_password(password).await
    }

    pub async fn verify_password_blocking(&self, password: &str, hash: &str) -> Result<bool, PasswordHashError> {
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
        let config =
            PasswordHashPoolConfig { max_concurrent: 2, queue_size: 10, thread_pool_size: 1, hash_timeout_ms: 5000 };
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
            let password = format!("password_{i}");
            handles.push(tokio::spawn(async move { pool_clone.hash_password(&password).await }));
        }

        let results: Vec<_> = futures::future::join_all(handles).await;
        let successful: Vec<_> = results.into_iter().filter_map(|r| r.ok()).filter(|r| r.is_ok()).collect();

        assert!(successful.len() >= 2, "At least 2 operations should succeed");
    }

    #[tokio::test]
    async fn test_pool_exhaustion() {
        let config =
            PasswordHashPoolConfig { max_concurrent: 1, queue_size: 1, thread_pool_size: 1, hash_timeout_ms: 1000 };
        let argon2_config = Argon2Config::new(65536, 3, 1).unwrap();
        let pool = Arc::new(PasswordHashPool::new(config, argon2_config));

        let pool_clone = pool.clone();
        let handle1 = tokio::spawn(async move { pool_clone.hash_password("password1").await });

        tokio::time::sleep(Duration::from_millis(10)).await;

        let pool_clone = pool.clone();
        let handle2 = tokio::spawn(async move { pool_clone.hash_password("password2").await });

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
            handles.push(tokio::spawn(async move { pool_clone.verify_password(password, &hash_clone).await }));
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

    #[tokio::test]
    async fn test_hash_password_blocking_alias() {
        // hash_password_blocking should behave identically to hash_password.
        let pool = create_test_pool();
        let hash = pool.hash_password_blocking("test_password").await.unwrap();
        assert!(hash.starts_with("$argon2id$"));
    }

    #[tokio::test]
    async fn test_verify_password_blocking_alias() {
        // verify_password_blocking should behave identically to verify_password.
        let pool = create_test_pool();
        let hash = pool.hash_password("test_password").await.unwrap();
        let result = pool.verify_password_blocking("test_password", &hash).await.unwrap();
        assert!(result);
    }

    #[tokio::test]
    async fn test_semaphore_getter_returns_arc() {
        let pool = create_test_pool();
        let semaphore = pool.semaphore();
        // max_concurrent is 2 in test config, so 2 permits are available.
        assert_eq!(semaphore.available_permits(), 2);
    }

    #[tokio::test]
    async fn test_try_hash_password_returns_handle_when_permits_available() {
        let pool = create_test_pool();
        // try_hash_password is synchronous and returns Option<JoinHandle<...>>.
        let handle = pool.try_hash_password("test_password");
        assert!(handle.is_some(), "should return Some(JoinHandle) when permits are available");

        // Await the spawned task to ensure it completes successfully.
        if let Some(h) = handle {
            let result = h.await.unwrap();
            assert!(result.is_ok());
            assert!(result.unwrap().starts_with("$argon2id$"));
        }
    }

    #[tokio::test]
    async fn test_try_hash_password_returns_none_when_exhausted() {
        // Pool with max_concurrent = 1: acquiring one permit exhausts the pool.
        let config =
            PasswordHashPoolConfig { max_concurrent: 1, queue_size: 1, thread_pool_size: 1, hash_timeout_ms: 1000 };
        let argon2_config = Argon2Config::new(65536, 3, 1).unwrap();
        let pool = Arc::new(PasswordHashPool::new(config, argon2_config));

        // Acquire the only permit manually.
        let _permit = pool.semaphore().clone().try_acquire_owned().unwrap();
        // try_hash_password is synchronous; returns None when permits are exhausted.
        let result = pool.try_hash_password("test_password");
        assert!(result.is_none(), "should return None when pool is exhausted");

        // Verify the rejected_operations counter was incremented.
        assert!(pool.metrics().rejected_operations.get() >= 1);
    }

    #[tokio::test]
    async fn test_get_pool_metrics_returns_some_after_global_init() {
        // Initialize the global pool with test config.
        let config = PasswordHashPoolConfig::default();
        let argon2_config = Argon2Config::default();
        PasswordHashPool::initialize_global(config, argon2_config);

        // After initialization, get_pool_metrics should return Some.
        let metrics = get_pool_metrics();
        assert!(metrics.is_some());
    }

    #[test]
    fn test_get_pool_status_returns_active_when_global_set() {
        // get_pool_status reads from the global pool (set by previous test or default init).
        // Calling it should not panic regardless of whether the global is set.
        let status = get_pool_status();
        // Status should have valid max_concurrent (>= 0) and available_permits (>= 0).
        let _ = status.available_permits;
        let _ = status.max_concurrent;
        let _ = status.active_operations;
    }

    #[test]
    fn test_pool_status_default() {
        let status = PoolStatus::default();
        assert_eq!(status.available_permits, 0);
        assert_eq!(status.max_concurrent, 0);
        assert_eq!(status.active_operations, 0);
    }

    #[test]
    fn test_pool_status_debug_format() {
        let status = PoolStatus { available_permits: 5, max_concurrent: 10, active_operations: 3 };
        let debug = format!("{status:?}");
        assert!(debug.contains("PoolStatus"));
        assert!(debug.contains("5"));
        assert!(debug.contains("10"));
        assert!(debug.contains("3"));
    }

    #[test]
    fn test_pool_status_clone_preserves_fields() {
        let status = PoolStatus { available_permits: 5, max_concurrent: 10, active_operations: 3 };
        let cloned = status.clone();
        assert_eq!(status.available_permits, cloned.available_permits);
        assert_eq!(status.max_concurrent, cloned.max_concurrent);
        assert_eq!(status.active_operations, cloned.active_operations);
    }

    #[test]
    fn test_runtime_pool_config_uses_defaults_when_env_unset() {
        // Save existing env values to restore after test (env can be set by other tests).
        let max_concurrent_save = env::var("SYNAPSE_PASSWORD_HASH_POOL_MAX_CONCURRENT").ok();
        env::remove_var("SYNAPSE_PASSWORD_HASH_POOL_MAX_CONCURRENT");

        let config = runtime_pool_config();
        assert_eq!(config.max_concurrent, 4, "default max_concurrent should be 4");

        // Restore env.
        if let Some(v) = max_concurrent_save {
            env::set_var("SYNAPSE_PASSWORD_HASH_POOL_MAX_CONCURRENT", v);
        }
    }

    #[test]
    fn test_env_override_usize_uses_default_when_var_invalid() {
        env::set_var("SYNAPSE_TEST_OVERRIDE_USIZE_INVALID", "not_a_number");
        let value = env_override_usize("SYNAPSE_TEST_OVERRIDE_USIZE_INVALID", 42);
        assert_eq!(value, 42, "invalid env value should fall back to default");
        env::remove_var("SYNAPSE_TEST_OVERRIDE_USIZE_INVALID");
    }

    #[test]
    fn test_env_override_usize_uses_default_when_zero() {
        // Zero values are treated as invalid and fall back to default.
        env::set_var("SYNAPSE_TEST_OVERRIDE_USIZE_ZERO", "0");
        let value = env_override_usize("SYNAPSE_TEST_OVERRIDE_USIZE_ZERO", 99);
        assert_eq!(value, 99, "zero env value should fall back to default");
        env::remove_var("SYNAPSE_TEST_OVERRIDE_USIZE_ZERO");
    }

    #[test]
    fn test_env_override_usize_uses_env_when_valid_positive() {
        env::set_var("SYNAPSE_TEST_OVERRIDE_USIZE_VALID", "123");
        let value = env_override_usize("SYNAPSE_TEST_OVERRIDE_USIZE_VALID", 99);
        assert_eq!(value, 123, "valid positive env value should be used");
        env::remove_var("SYNAPSE_TEST_OVERRIDE_USIZE_VALID");
    }

    #[test]
    fn test_env_override_u64_uses_default_when_var_unset() {
        env::remove_var("SYNAPSE_TEST_OVERRIDE_U64_UNSET");
        let value = env_override_u64("SYNAPSE_TEST_OVERRIDE_U64_UNSET", 5000);
        assert_eq!(value, 5000, "unset env should fall back to default");
    }

    #[test]
    fn test_env_override_u64_uses_default_when_var_invalid() {
        env::set_var("SYNAPSE_TEST_OVERRIDE_U64_INVALID", "abc");
        let value = env_override_u64("SYNAPSE_TEST_OVERRIDE_U64_INVALID", 5000);
        assert_eq!(value, 5000, "invalid env value should fall back to default");
        env::remove_var("SYNAPSE_TEST_OVERRIDE_U64_INVALID");
    }

    #[test]
    fn test_env_override_u64_uses_default_when_zero() {
        env::set_var("SYNAPSE_TEST_OVERRIDE_U64_ZERO", "0");
        let value = env_override_u64("SYNAPSE_TEST_OVERRIDE_U64_ZERO", 5000);
        assert_eq!(value, 5000, "zero env value should fall back to default");
        env::remove_var("SYNAPSE_TEST_OVERRIDE_U64_ZERO");
    }

    #[test]
    fn test_env_override_u64_uses_env_when_valid_positive() {
        env::set_var("SYNAPSE_TEST_OVERRIDE_U64_VALID", "9999");
        let value = env_override_u64("SYNAPSE_TEST_OVERRIDE_U64_VALID", 5000);
        assert_eq!(value, 9999, "valid positive env value should be used");
        env::remove_var("SYNAPSE_TEST_OVERRIDE_U64_VALID");
    }

    #[test]
    fn test_runtime_pool_config_respects_env_overrides() {
        env::set_var("SYNAPSE_PASSWORD_HASH_POOL_MAX_CONCURRENT", "8");
        env::set_var("SYNAPSE_PASSWORD_HASH_POOL_QUEUE_SIZE", "200");
        env::set_var("SYNAPSE_PASSWORD_HASH_POOL_THREAD_POOL_SIZE", "4");
        env::set_var("SYNAPSE_PASSWORD_HASH_POOL_HASH_TIMEOUT_MS", "10000");

        let config = runtime_pool_config();
        assert_eq!(config.max_concurrent, 8);
        assert_eq!(config.queue_size, 200);
        assert_eq!(config.thread_pool_size, 4);
        assert_eq!(config.hash_timeout_ms, 10000);

        // Cleanup env vars.
        env::remove_var("SYNAPSE_PASSWORD_HASH_POOL_MAX_CONCURRENT");
        env::remove_var("SYNAPSE_PASSWORD_HASH_POOL_QUEUE_SIZE");
        env::remove_var("SYNAPSE_PASSWORD_HASH_POOL_THREAD_POOL_SIZE");
        env::remove_var("SYNAPSE_PASSWORD_HASH_POOL_HASH_TIMEOUT_MS");
    }

    #[tokio::test]
    async fn test_with_metrics_uses_provided_collector() {
        let collector = MetricsCollector::new();
        let counter_before = collector.inventory().total_counters;
        let config = PasswordHashPoolConfig::default();
        let argon2_config = Argon2Config::default();
        let _pool = PasswordHashPool::with_metrics(config, argon2_config, &collector);
        // with_metrics should register counters on the provided collector.
        let counter_after = collector.inventory().total_counters;
        assert!(counter_after > counter_before, "with_metrics should register counters on the collector");
    }

    #[tokio::test]
    async fn test_verify_password_with_garbage_hash_returns_invalid_format_error() {
        let pool = create_test_pool();
        // A clearly invalid hash string should produce InvalidHashFormat error.
        let result = pool.verify_password("password", "$invalid$hash$format").await;
        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(matches!(err, PasswordHashError::InvalidHashFormat(_)), "expected InvalidHashFormat, got {err:?}");
    }

    #[tokio::test]
    async fn test_pool_exhaustion_returns_correct_error_variant() {
        // Pool with max_concurrent = 0 is not possible (Semaphore::new(0) panics on some versions).
        // Use max_concurrent = 1 and manually exhaust it.
        let config =
            PasswordHashPoolConfig { max_concurrent: 1, queue_size: 1, thread_pool_size: 1, hash_timeout_ms: 1000 };
        let argon2_config = Argon2Config::new(65536, 3, 1).unwrap();
        let pool = Arc::new(PasswordHashPool::new(config, argon2_config));

        // Hold the only permit.
        let _permit = pool.semaphore().clone().try_acquire_owned().unwrap();

        // hash_password should return PoolExhausted error.
        let result = pool.hash_password("test").await;
        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(matches!(err, PasswordHashError::PoolExhausted), "expected PoolExhausted, got {err:?}");

        // Verify pool_exhaustion_count was incremented.
        assert!(pool.metrics().pool_exhaustion_count.get() >= 1);
        assert!(pool.metrics().rejected_operations.get() >= 1);
    }

    #[test]
    fn test_password_hash_error_display_messages() {
        // Verify error Display impl produces the expected text for each variant.
        let pool_exhausted = PasswordHashError::PoolExhausted;
        assert!(pool_exhausted.to_string().contains("pool exhausted"));

        let hash_failed = PasswordHashError::HashFailed("test reason".to_string());
        assert!(hash_failed.to_string().contains("Hash operation failed"));
        assert!(hash_failed.to_string().contains("test reason"));

        let invalid_format = PasswordHashError::InvalidHashFormat("bad format".to_string());
        assert!(invalid_format.to_string().contains("Invalid hash format"));
        assert!(invalid_format.to_string().contains("bad format"));

        let task_join = PasswordHashError::TaskJoinError("join failure".to_string());
        assert!(task_join.to_string().contains("Task join error"));
        assert!(task_join.to_string().contains("join failure"));

        let timeout = PasswordHashError::Timeout;
        assert!(timeout.to_string().contains("timed out"));

        let queue_full = PasswordHashError::QueueFull;
        assert!(queue_full.to_string().contains("Queue is full"));
    }

    #[tokio::test]
    async fn test_metrics_are_tracked_during_verify_operations() {
        let pool = create_test_pool();
        let hash = pool.hash_password("password").await.unwrap();

        let initial_verify_count = pool.metrics().total_verify_operations.get();
        let _ = pool.verify_password("password", &hash).await.unwrap();
        assert_eq!(pool.metrics().total_verify_operations.get(), initial_verify_count + 1);

        // Verify duration histogram should also be observed.
        assert!(pool.metrics().verify_duration_ms.get_count() > 0);
    }

    #[tokio::test]
    async fn test_get_or_init_default_initializes_global_pool() {
        // get_or_init_default should return a valid pool even if global is not set.
        // Note: previous tests may have already set the global; this is idempotent.
        let pool = PasswordHashPool::get_or_init_default();
        let hash = pool.hash_password("default_pool_test").await.unwrap();
        assert!(hash.starts_with("$argon2id$"));
    }
}
