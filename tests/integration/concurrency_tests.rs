#[cfg(test)]
mod concurrency_integration_tests {
    use synapse_rust::concurrency::{ConcurrencyController, ConcurrencyLimiter};
    use tokio::runtime::Runtime;

    #[test]
    fn test_concurrency_controller_acquire_release() {
        let rt = Runtime::new().unwrap();
        rt.block_on(async {
            let controller = ConcurrencyController::new(2, "test".to_string());

            assert_eq!(controller.available_permits(), 2);

            let permit1 = controller.acquire().await;
            assert_eq!(controller.available_permits(), 1);

            let permit2 = controller.acquire().await;
            assert_eq!(controller.available_permits(), 0);

            drop(permit1);
            assert_eq!(controller.available_permits(), 1);

            drop(permit2);
            assert_eq!(controller.available_permits(), 2);
        });
    }

    #[tokio::test]
    async fn test_concurrency_controller_try_acquire() {
        let controller = ConcurrencyController::new(1, "test".to_string());

        let permit = controller.try_acquire().await;
        assert!(permit.is_some());

        let permit2 = controller.try_acquire().await;
        assert!(permit2.is_none());

        drop(permit);

        let permit3 = controller.try_acquire().await;
        assert!(permit3.is_some());
    }

    #[tokio::test]
    async fn test_concurrency_controller_clone() {
        let controller1 = ConcurrencyController::new(2, "test".to_string());
        let controller2 = controller1.clone();

        let _permit1 = controller1.acquire().await;
        assert_eq!(controller2.available_permits(), 1);

        let _permit2 = controller2.acquire().await;
        assert_eq!(controller1.available_permits(), 0);
    }

    #[test]
    fn test_concurrency_limiter_workflow() {
        let mut limiter = ConcurrencyLimiter::new();
        limiter.add_controller("api".to_string(), 10);
        limiter.add_controller("database".to_string(), 5);
        limiter.add_controller("cache".to_string(), 20);

        let api_controller = limiter.get_controller("api").unwrap();
        let db_controller = limiter.get_controller("database").unwrap();
        let cache_controller = limiter.get_controller("cache").unwrap();

        assert_eq!(api_controller.available_permits(), 10);
        assert_eq!(db_controller.available_permits(), 5);
        assert_eq!(cache_controller.available_permits(), 20);

        assert!(limiter.get_controller("nonexistent").is_none());
    }

    #[tokio::test]
    async fn test_concurrency_limiter_acquire() {
        let mut limiter = ConcurrencyLimiter::new();
        limiter.add_controller("test".to_string(), 2);

        let permit1 = limiter.acquire("test").await;
        assert!(permit1.is_some());

        let permit2 = limiter.acquire("test").await;
        assert!(permit2.is_some());

        // Use try_acquire here to avoid hanging
        let permit3 = limiter.try_acquire("test").await;
        assert!(permit3.is_none());

        drop(permit1);
        let permit4 = limiter.acquire("test").await;
        assert!(permit4.is_some());
    }

    #[tokio::test]
    async fn test_concurrency_permit_drop_logging() {
        let controller = ConcurrencyController::new(1, "test_controller".to_string());
        let _permit = controller.try_acquire().await.unwrap();
        assert_eq!(controller.available_permits(), 0);
    }
}
