use std::sync::Arc;
use tokio::sync::Semaphore;

pub struct ConcurrencyController {
    semaphore: Arc<Semaphore>,
    name: String,
}

impl ConcurrencyController {
    pub fn new(max_concurrent: usize, name: String) -> Self {
        Self {
            semaphore: Arc::new(Semaphore::new(max_concurrent)),
            name,
        }
    }

    pub async fn acquire(&self) -> ConcurrencyPermit {
        let permit = self.semaphore.clone().acquire_owned().await.unwrap();
        ConcurrencyPermit {
            _permit: permit,
            name: self.name.clone(),
        }
    }

    pub async fn try_acquire(&self) -> Option<ConcurrencyPermit> {
        self.semaphore
            .clone()
            .try_acquire_owned()
            .ok()
            .map(|permit| ConcurrencyPermit {
                _permit: permit,
                name: self.name.clone(),
            })
    }

    pub fn available_permits(&self) -> usize {
        self.semaphore.available_permits()
    }
}

impl Clone for ConcurrencyController {
    fn clone(&self) -> Self {
        Self {
            semaphore: Arc::clone(&self.semaphore),
            name: self.name.clone(),
        }
    }
}

pub struct ConcurrencyPermit {
    _permit: tokio::sync::OwnedSemaphorePermit,
    name: String,
}

impl Drop for ConcurrencyPermit {
    fn drop(&mut self) {
        tracing::debug!("Concurrency permit released for: {}", self.name);
    }
}

pub struct ConcurrencyLimiter {
    controllers: std::collections::HashMap<String, ConcurrencyController>,
}

impl ConcurrencyLimiter {
    pub fn new() -> Self {
        Self {
            controllers: std::collections::HashMap::new(),
        }
    }

    pub fn add_controller(&mut self, name: String, max_concurrent: usize) {
        let controller = ConcurrencyController::new(max_concurrent, name.clone());
        self.controllers.insert(name, controller);
    }

    pub fn get_controller(&self, name: &str) -> Option<ConcurrencyController> {
        self.controllers.get(name).cloned()
    }

    pub async fn acquire(&self, name: &str) -> Option<ConcurrencyPermit> {
        if let Some(controller) = self.get_controller(name) {
            Some(controller.acquire().await)
        } else {
            None
        }
    }

    pub async fn try_acquire(&self, name: &str) -> Option<ConcurrencyPermit> {
        if let Some(controller) = self.get_controller(name) {
            controller.try_acquire().await
        } else {
            None
        }
    }
}

impl Default for ConcurrencyLimiter {
    fn default() -> Self {
        Self::new()
    }
}

#[macro_export]
macro_rules! with_concurrency_limit {
    ($controller:expr, $block:block) => {
        {
            let permit = $controller.acquire().await;
            $block
        }
    };
}

#[macro_export]
macro_rules! try_with_concurrency_limit {
    ($controller:expr, $block:block) => {
        {
            if let Some(permit) = $controller.try_acquire().await {
                Some($block)
            } else {
                None
            }
        }
    };
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_concurrency_controller() {
        let controller = ConcurrencyController::new(2, "test".to_string());

        assert_eq!(controller.available_permits(), 2);

        let _permit1 = controller.acquire().await;
        assert_eq!(controller.available_permits(), 1);

        let _permit2 = controller.acquire().await;
        assert_eq!(controller.available_permits(), 0);
    }

    #[tokio::test]
    async fn test_concurrency_controller_try_acquire() {
        let controller = ConcurrencyController::new(1, "test".to_string());

        let permit = controller.try_acquire().await;
        assert!(permit.is_some());

        let permit = controller.try_acquire().await;
        assert!(permit.is_none());
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

    #[tokio::test]
    async fn test_concurrency_limiter() {
        let mut limiter = ConcurrencyLimiter::new();
        limiter.add_controller("test".to_string(), 2);

        let controller = limiter.get_controller("test").unwrap();
        assert_eq!(controller.available_permits(), 2);

        let _permit = controller.acquire().await;
        assert_eq!(controller.available_permits(), 1);
    }

    #[tokio::test]
    async fn test_concurrency_permit_drop() {
        let controller = ConcurrencyController::new(1, "test".to_string());

        {
            let _permit = controller.acquire().await;
            assert_eq!(controller.available_permits(), 0);
        }

        assert_eq!(controller.available_permits(), 1);
    }

    #[tokio::test]
    async fn test_with_concurrency_limit_macro() {
        let controller = ConcurrencyController::new(1, "test".to_string());

        let result = with_concurrency_limit!(&controller, {
            42
        });

        assert_eq!(result, 42);
    }

    #[tokio::test]
    async fn test_try_with_concurrency_limit_macro() {
        let controller = ConcurrencyController::new(1, "test".to_string());

        let result = try_with_concurrency_limit!(&controller, {
            Some(42)
        });

        assert_eq!(result, Some(Some(42)));

        let _permit = controller.acquire().await;

        let result = try_with_concurrency_limit!(&controller, {
            Some(42)
        });

        assert_eq!(result, None);
    }
}
