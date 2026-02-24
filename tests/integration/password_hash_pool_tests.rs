#[cfg(test)]
mod password_hash_pool_tests {
    use std::sync::Arc;
    use std::time::{Duration, Instant};

    use synapse_rust::password_hash_pool::{
        hash_password_pooled, verify_password_pooled, PasswordHashPool, PasswordHashPoolConfig,
        PasswordHashError, PoolStatus, get_pool_status,
    };
    use synapse_rust::argon2_config::Argon2Config;
    use tokio::sync::Barrier;

    fn create_test_pool_config() -> PasswordHashPoolConfig {
        PasswordHashPoolConfig {
            max_concurrent: 4,
            queue_size: 20,
            thread_pool_size: 2,
            hash_timeout_ms: 5000,
        }
    }

    fn create_test_argon2_config() -> Argon2Config {
        Argon2Config::new(65536, 3, 1).unwrap()
    }

    #[tokio::test]
    async fn test_basic_hash_and_verify() {
        let pool = PasswordHashPool::new(create_test_pool_config(), create_test_argon2_config());
        let password = "test_password_123";

        let hash = pool.hash_password(password).await.unwrap();
        assert!(hash.starts_with("$argon2id$"));

        let verify_result = pool.verify_password(password, &hash).await.unwrap();
        assert!(verify_result);

        let wrong_verify = pool.verify_password("wrong_password", &hash).await.unwrap();
        assert!(!wrong_verify);
    }

    #[tokio::test]
    async fn test_concurrent_hash_operations_limited() {
        let config = PasswordHashPoolConfig {
            max_concurrent: 2,
            queue_size: 5,
            thread_pool_size: 1,
            hash_timeout_ms: 5000,
        };
        let pool = Arc::new(PasswordHashPool::new(config, create_test_argon2_config()));

        let mut handles = vec![];
        let barrier = Arc::new(Barrier::new(6));

        for i in 0..6 {
            let pool_clone = pool.clone();
            let barrier_clone = barrier.clone();
            handles.push(tokio::spawn(async move {
                barrier_clone.wait().await;
                pool_clone.hash_password(&format!("password_{}", i)).await
            }));
        }

        let results: Vec<_> = futures::future::join_all(handles).await;

        let successful: Vec<_> = results.iter().filter(|r| {
            r.is_ok() && r.as_ref().unwrap().is_ok()
        }).collect();

        let rejected: Vec<_> = results.iter().filter(|r| {
            r.is_ok() && matches!(r.as_ref().unwrap(), Err(PasswordHashError::PoolExhausted))
        }).collect();

        assert!(successful.len() >= 2, "At least 2 operations should succeed");
        assert!(successful.len() + rejected.len() == 6, "All operations should either succeed or be rejected");
    }

    #[tokio::test]
    async fn test_pool_semaphore_behavior() {
        let config = PasswordHashPoolConfig {
            max_concurrent: 3,
            queue_size: 10,
            thread_pool_size: 1,
            hash_timeout_ms: 5000,
        };
        let pool = PasswordHashPool::new(config, create_test_argon2_config());

        assert_eq!(pool.available_permits(), 3);

        let permit1 = pool.semaphore().clone().try_acquire_owned().unwrap();
        assert_eq!(pool.available_permits(), 2);

        let permit2 = pool.semaphore().clone().try_acquire_owned().unwrap();
        assert_eq!(pool.available_permits(), 1);

        let permit3 = pool.semaphore().clone().try_acquire_owned().unwrap();
        assert_eq!(pool.available_permits(), 0);

        let permit4 = pool.semaphore().clone().try_acquire_owned();
        assert!(permit4.is_err(), "Should not be able to acquire when pool is exhausted");

        drop(permit1);
        assert_eq!(pool.available_permits(), 1);
    }

    #[tokio::test]
    async fn test_metrics_are_recorded() {
        let pool = PasswordHashPool::new(create_test_pool_config(), create_test_argon2_config());

        let initial_hash_count = pool.metrics().total_hash_operations.get();
        let initial_verify_count = pool.metrics().total_verify_operations.get();

        let hash = pool.hash_password("test").await.unwrap();
        pool.verify_password("test", &hash).await.unwrap();

        assert_eq!(pool.metrics().total_hash_operations.get(), initial_hash_count + 1);
        assert_eq!(pool.metrics().total_verify_operations.get(), initial_verify_count + 1);

        let hash_count = pool.metrics().hash_duration_ms.get_count();
        let verify_count = pool.metrics().verify_duration_ms.get_count();

        assert!(hash_count > 0, "Hash duration should be recorded");
        assert!(verify_count > 0, "Verify duration should be recorded");
    }

    #[tokio::test]
    async fn test_rejected_operations_counter() {
        let config = PasswordHashPoolConfig {
            max_concurrent: 1,
            queue_size: 1,
            thread_pool_size: 1,
            hash_timeout_ms: 1000,
        };
        let pool = Arc::new(PasswordHashPool::new(config, create_test_argon2_config()));

        let initial_rejected = pool.metrics().rejected_operations.get();

        let mut handles = vec![];
        for i in 0..5 {
            let pool_clone = pool.clone();
            handles.push(tokio::spawn(async move {
                pool_clone.hash_password(&format!("password_{}", i)).await
            }));
        }

        let _results: Vec<_> = futures::future::join_all(handles).await;

        let final_rejected = pool.metrics().rejected_operations.get();
        assert!(final_rejected > initial_rejected, "Some operations should be rejected");
    }

    #[tokio::test]
    async fn test_pool_exhaustion_error() {
        let config = PasswordHashPoolConfig {
            max_concurrent: 1,
            queue_size: 1,
            thread_pool_size: 1,
            hash_timeout_ms: 100,
        };
        let pool = Arc::new(PasswordHashPool::new(config, create_test_argon2_config()));

        let _permit = pool.semaphore().clone().try_acquire_owned().unwrap();

        let result = pool.hash_password("test").await;
        assert!(matches!(result, Err(PasswordHashError::PoolExhausted)));
    }

    #[tokio::test]
    async fn test_concurrent_verify_same_hash() {
        let pool = PasswordHashPool::new(create_test_pool_config(), create_test_argon2_config());
        let password = "shared_password";
        let hash = pool.hash_password(password).await.unwrap();

        let mut handles = vec![];
        for _ in 0..10 {
            let pool_clone = pool.clone();
            let hash_clone = hash.clone();
            handles.push(tokio::spawn(async move {
                pool_clone.verify_password(password, &hash_clone).await
            }));
        }

        let results: Vec<_> = futures::future::join_all(handles).await;
        let successful: Vec<_> = results.into_iter().filter_map(|r| r.ok()).filter_map(|r| r.ok()).collect();
        
        assert!(successful.len() >= 4, "At least 4 verify operations should succeed");
        for result in successful {
            assert!(result, "All successful verifications should return true");
        }
    }

    #[tokio::test]
    async fn test_hash_performance_under_load() {
        let pool = PasswordHashPool::new(create_test_pool_config(), create_test_argon2_config());

        let start = Instant::now();
        let mut handles = vec![];

        for i in 0..10 {
            let pool_clone = pool.clone();
            handles.push(tokio::spawn(async move {
                pool_clone.hash_password(&format!("password_{}", i)).await
            }));
        }

        let results: Vec<_> = futures::future::join_all(handles).await;
        let duration = start.elapsed();

        let successful_count = results.iter().filter(|r| {
            r.is_ok() && r.as_ref().unwrap().is_ok()
        }).count();

        assert!(successful_count >= 4, "At least 4 operations should succeed");
        println!("Completed {} hash operations in {:?}", successful_count, duration);
    }

    #[tokio::test]
    async fn test_pool_clone_shares_semaphore() {
        let config = PasswordHashPoolConfig {
            max_concurrent: 2,
            queue_size: 5,
            thread_pool_size: 1,
            hash_timeout_ms: 5000,
        };
        let pool1 = Arc::new(PasswordHashPool::new(config, create_test_argon2_config()));
        let pool2 = Arc::clone(&pool1);

        let handle1 = tokio::spawn(async move {
            pool1.hash_password("password1").await
        });

        let handle2 = tokio::spawn(async move {
            pool2.hash_password("password2").await
        });

        let result1 = handle1.await.unwrap();
        let result2 = handle2.await.unwrap();

        assert!(result1.is_ok() || result2.is_ok(), "At least one should succeed");
    }

    #[tokio::test]
    async fn test_invalid_hash_format_error() {
        let pool = PasswordHashPool::new(create_test_pool_config(), create_test_argon2_config());

        let result = pool.verify_password("password", "not_a_valid_hash").await;
        assert!(matches!(result, Err(PasswordHashError::InvalidHashFormat(_))));
    }

    #[tokio::test]
    async fn test_empty_password_handling() {
        let pool = PasswordHashPool::new(create_test_pool_config(), create_test_argon2_config());

        let hash = pool.hash_password("").await.unwrap();
        assert!(!hash.is_empty());

        let verify = pool.verify_password("", &hash).await.unwrap();
        assert!(verify);
    }

    #[tokio::test]
    async fn test_long_password_handling() {
        let pool = PasswordHashPool::new(create_test_pool_config(), create_test_argon2_config());

        let long_password = "a".repeat(1000);
        let hash = pool.hash_password(&long_password).await.unwrap();

        let verify = pool.verify_password(&long_password, &hash).await.unwrap();
        assert!(verify);

        let wrong_verify = pool.verify_password(&"a".repeat(999), &hash).await.unwrap();
        assert!(!wrong_verify);
    }

    #[tokio::test]
    async fn test_special_characters_in_password() {
        let pool = PasswordHashPool::new(create_test_pool_config(), create_test_argon2_config());

        let special_password = "p@$$w0rd!#$%^&*()_+-=[]{}|;':\",./<>?`~";
        let hash = pool.hash_password(special_password).await.unwrap();

        let verify = pool.verify_password(special_password, &hash).await.unwrap();
        assert!(verify);
    }

    #[tokio::test]
    async fn test_unicode_password_handling() {
        let pool = PasswordHashPool::new(create_test_pool_config(), create_test_argon2_config());

        let unicode_password = "密码测试🔐🎉";
        let hash = pool.hash_password(unicode_password).await.unwrap();

        let verify = pool.verify_password(unicode_password, &hash).await.unwrap();
        assert!(verify);
    }

    #[tokio::test]
    async fn test_pool_status_reporting() {
        let config = PasswordHashPoolConfig {
            max_concurrent: 5,
            queue_size: 10,
            thread_pool_size: 2,
            hash_timeout_ms: 5000,
        };
        let pool = PasswordHashPool::new(config, create_test_argon2_config());

        assert_eq!(pool.available_permits(), 5);
        assert_eq!(pool.max_concurrent(), 5);
    }

    #[tokio::test]
    async fn test_active_operations_tracking() {
        let pool = PasswordHashPool::new(create_test_pool_config(), create_test_argon2_config());

        let initial_active = pool.metrics().active_operations.get();

        let hash = pool.hash_password("test").await.unwrap();

        let final_active = pool.metrics().active_operations.get();
        assert_eq!(initial_active, final_active, "Active operations should return to initial after completion");
    }

    #[tokio::test]
    async fn test_high_concurrency_stress() {
        let config = PasswordHashPoolConfig {
            max_concurrent: 8,
            queue_size: 50,
            thread_pool_size: 4,
            hash_timeout_ms: 10000,
        };
        let pool = Arc::new(PasswordHashPool::new(config, create_test_argon2_config()));

        let mut handles = vec![];
        let operation_count = 20;

        for i in 0..operation_count {
            let pool_clone = pool.clone();
            handles.push(tokio::spawn(async move {
                let hash = pool_clone.hash_password(&format!("password_{}", i)).await?;
                pool_clone.verify_password(&format!("password_{}", i), &hash).await
            }));
        }

        let results: Vec<_> = futures::future::join_all(handles).await;

        let successful: Vec<_> = results.iter().filter(|r| {
            r.is_ok() && r.as_ref().unwrap().is_ok() && *r.as_ref().unwrap().as_ref().unwrap()
        }).collect();

        assert!(successful.len() >= 8, "At least 8 operations should complete successfully");
    }

    #[tokio::test]
    async fn test_pool_exhaustion_recovery() {
        let config = PasswordHashPoolConfig {
            max_concurrent: 1,
            queue_size: 1,
            thread_pool_size: 1,
            hash_timeout_ms: 5000,
        };
        let pool = PasswordHashPool::new(config, create_test_argon2_config());

        let result1 = pool.hash_password("password1").await;
        assert!(result1.is_ok());

        let result2 = pool.hash_password("password2").await;
        assert!(result2.is_ok());
    }

    #[tokio::test]
    async fn test_duration_metrics_percentile() {
        let pool = PasswordHashPool::new(create_test_pool_config(), create_test_argon2_config());

        for i in 0..10 {
            let _ = pool.hash_password(&format!("password_{}", i)).await;
        }

        let p50 = pool.metrics().hash_duration_ms.get_percentile(50.0).unwrap();
        let p95 = pool.metrics().hash_duration_ms.get_percentile(95.0).unwrap();

        assert!(p50 > 0.0, "P50 should be positive");
        assert!(p95 >= p50, "P95 should be >= P50");
    }
}


