use std::time::{Duration, Instant};

pub struct SlowQueryLogger {
    threshold: Duration,
}

impl SlowQueryLogger {
    pub fn new(threshold_ms: u64) -> Self {
        Self {
            threshold: Duration::from_millis(threshold_ms),
        }
    }

    pub fn log_query<F, T>(&self, query_name: &str, query: &str, f: F) -> T
    where
        F: FnOnce() -> T,
    {
        let start = Instant::now();
        let result = f();
        let elapsed = start.elapsed();

        if elapsed > self.threshold {
            tracing::warn!(
                query_name = %query_name,
                duration_ms = elapsed.as_millis(),
                query = %query,
                "SLOW_QUERY"
            );
        } else {
            tracing::debug!(
                query_name = %query_name,
                duration_ms = elapsed.as_millis(),
                "query completed"
            );
        }

        result
    }

    pub async fn log_query_async<F, T, Fut>(&self, query_name: &str, query: &str, f: F) -> T
    where
        F: FnOnce() -> Fut,
        Fut: std::future::Future<Output = T>,
    {
        let start = Instant::now();
        let result = f().await;
        let elapsed = start.elapsed();

        if elapsed > self.threshold {
            tracing::warn!(
                query_name = %query_name,
                duration_ms = elapsed.as_millis(),
                query = %query,
                "SLOW_QUERY"
            );
        } else {
            tracing::debug!(
                query_name = %query_name,
                duration_ms = elapsed.as_millis(),
                "query completed"
            );
        }

        result
    }
}

impl Default for SlowQueryLogger {
    fn default() -> Self {
        Self::new(100)
    }
}
