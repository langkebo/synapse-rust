use std::future::Future;
use std::time::Duration;

pub trait EarlyExit {
    fn with_timeout(self, timeout: Duration) -> impl Future<Output = Option<Self::Output>>
    where
        Self: Sized + Future,
    {
        async move { tokio::time::timeout(timeout, self).await.ok() }
    }

    fn with_deadline(
        self,
        deadline: tokio::time::Instant,
    ) -> impl Future<Output = Option<Self::Output>>
    where
        Self: Sized + Future,
    {
        async move { tokio::time::timeout_at(deadline, self).await.ok() }
    }
}

impl<F: Future> EarlyExit for F {}

pub fn early_exit<T>(condition: bool, value: T) -> Option<T> {
    if condition {
        Some(value)
    } else {
        None
    }
}

pub fn early_return<T, E>(_condition: bool, error: E) -> Result<T, E> {
    Err(error)
}

pub fn early_continue<T>(condition: bool, value: T) -> Option<T> {
    if condition {
        None
    } else {
        Some(value)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tokio::time::Instant;

    #[tokio::test]
    async fn test_early_exit_timeout() {
        tokio::time::pause();
        let future = async {
            tokio::time::sleep(Duration::from_millis(100)).await;
            42
        };

        let result = future.with_timeout(Duration::from_millis(50)).await;
        assert_eq!(result, None);

        let future = async {
            tokio::time::sleep(Duration::from_millis(10)).await;
            42
        };

        let result = future.with_timeout(Duration::from_millis(50)).await;
        assert_eq!(result, Some(42));
    }

    #[tokio::test]
    async fn test_early_exit_deadline() {
        tokio::time::pause();
        let future = async {
            tokio::time::sleep(Duration::from_millis(100)).await;
            42
        };

        let deadline = Instant::now() + Duration::from_millis(50);
        let result = future.with_deadline(deadline).await;
        assert_eq!(result, None);

        let future = async {
            tokio::time::sleep(Duration::from_millis(10)).await;
            42
        };

        let deadline = Instant::now() + Duration::from_millis(50);
        let result = future.with_deadline(deadline).await;
        assert_eq!(result, Some(42));
    }

    #[test]
    fn test_early_exit_function() {
        assert_eq!(early_exit(true, 42), Some(42));
        assert_eq!(early_exit(false, 42), None);
    }

    #[test]
    fn test_early_continue_function() {
        assert_eq!(early_continue(true, 42), None);
        assert_eq!(early_continue(false, 42), Some(42));
    }

    #[test]
    fn test_early_return_function() {
        let result: Result<i32, &str> = early_return(true, "error");
        assert_eq!(result, Err("error"));

        let result: Result<i32, &str> = early_return(false, "error");
        assert_eq!(result, Err("error"));
    }
}
