//! Database performance monitoring and optimization utilities.
//!
//! This module provides tools for monitoring database connection pool health,
//! tracking query performance, and identifying slow queries.

use crate::common::metrics::MetricsCollector;
use sqlx::{Pool, Postgres};
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::sync::RwLock;
use serde::{Deserialize, Serialize};

/// Statistics about the database connection pool.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PoolStatistics {
    /// Total number of connections in the pool
    pub total_connections: u32,
    /// Number of idle connections
    pub idle_connections: u32,
    /// Number of active connections
    pub active_connections: u32,
    /// Maximum pool size
    pub max_connections: u32,
    /// Utilization percentage (0-100)
    pub utilization_percent: f64,
}

/// Query performance metrics.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct QueryMetrics {
    /// Name of the query or operation
    pub operation: String,
    /// Number of times this query was executed
    pub execution_count: u64,
    /// Total time spent executing this query (milliseconds)
    pub total_duration_ms: u64,
    /// Average execution time (milliseconds)
    pub avg_duration_ms: f64,
    /// Minimum execution time (milliseconds)
    pub min_duration_ms: u64,
    /// Maximum execution time (milliseconds)
    pub max_duration_ms: u64,
    /// Number of times the query was slow (>100ms)
    pub slow_count: u64,
}

/// Monitors and reports database performance metrics.
pub struct PerformanceMonitor {
    pool: Arc<Pool<Postgres>>,
    metrics: Arc<MetricsCollector>,
    /// Per-operation query metrics
    query_metrics: RwLock<std::collections::HashMap<String, QueryMetricsData>>,
    /// Threshold for considering a query "slow" (milliseconds)
    slow_query_threshold_ms: u64,
}

#[derive(Debug, Clone)]
struct QueryMetricsData {
    count: u64,
    total_ms: u64,
    min_ms: u64,
    max_ms: u64,
    slow_count: u64,
}

impl PerformanceMonitor {
    /// Creates a new performance monitor.
    ///
    /// # Arguments
    /// * `pool` - Database connection pool
    /// * `metrics` - Metrics collector for reporting
    /// * `slow_query_threshold_ms` - Threshold for slow queries (default: 100ms)
    pub fn new(
        pool: Arc<Pool<Postgres>>,
        metrics: Arc<MetricsCollector>,
        slow_query_threshold_ms: Option<u64>,
    ) -> Self {
        Self {
            pool,
            metrics,
            query_metrics: RwLock::new(std::collections::HashMap::new()),
            slow_query_threshold_ms: slow_query_threshold_ms.unwrap_or(100),
        }
    }

    /// Gets current pool statistics.
    pub async fn get_pool_stats(&self) -> PoolStatistics {
        let pool_size = self.pool.size();
        let num_idle = self.pool.num_idle() as u32;

        // Get max_connections from the pool's options
        // Note: sqlx doesn't expose max_connections directly, so we use size() as an approximation
        // In production, you should store max_connections when creating the pool
        let max_connections = std::env::var("DB_MAX_CONNECTIONS")
            .ok()
            .and_then(|s| s.parse::<u32>().ok())
            .unwrap_or(5);

        PoolStatistics {
            total_connections: pool_size,
            idle_connections: num_idle,
            active_connections: pool_size - num_idle,
            max_connections,
            utilization_percent: if pool_size > 0 {
                ((pool_size - num_idle) as f64 / pool_size as f64) * 100.0
            } else {
                0.0
            },
        }
    }

    /// Records a query execution.
    pub async fn record_query(&self, operation: &str, duration: Duration) {
        let duration_ms = duration.as_millis() as u64;

        // Update internal metrics
        {
            let mut metrics = self.query_metrics.write().await;
            let entry = metrics.entry(operation.to_string()).or_insert_with(|| QueryMetricsData {
                count: 0,
                total_ms: 0,
                min_ms: duration_ms,
                max_ms: duration_ms,
                slow_count: 0,
            });

            entry.count += 1;
            entry.total_ms += duration_ms;
            entry.min_ms = entry.min_ms.min(duration_ms);
            entry.max_ms = entry.max_ms.max(duration_ms);

            if duration_ms > self.slow_query_threshold_ms {
                entry.slow_count += 1;
                ::tracing::warn!(
                    operation,
                    duration_ms,
                    "Slow query detected (>{}ms)",
                    self.slow_query_threshold_ms
                );
            }
        }

        // Update metrics collector
        if let Some(hist) = self.metrics.get_histogram(&format!("db_query_{}_ms", operation)) {
            hist.observe(duration.as_secs_f64() * 1000.0);
        }

        // Increment query counter
        if let Some(counter) = self.metrics.get_counter(&format!("db_query_{}_total", operation)) {
            counter.inc();
        }

        // Track slow queries
        if duration_ms > self.slow_query_threshold_ms {
            if let Some(counter) = self.metrics.get_counter(&format!("db_query_{}_slow_total", operation)) {
                counter.inc();
            }
        }
    }

    /// Gets metrics for all tracked queries.
    pub async fn get_query_metrics(&self) -> Vec<QueryMetrics> {
        let metrics = self.query_metrics.read().await;
        metrics
            .iter()
            .map(|(operation, data)| QueryMetrics {
                operation: operation.clone(),
                execution_count: data.count,
                total_duration_ms: data.total_ms,
                avg_duration_ms: if data.count > 0 {
                    data.total_ms as f64 / data.count as f64
                } else {
                    0.0
                },
                min_duration_ms: data.min_ms,
                max_duration_ms: data.max_ms,
                slow_count: data.slow_count,
            })
            .collect()
    }

    /// Gets metrics for a specific operation.
    pub async fn get_operation_metrics(&self, operation: &str) -> Option<QueryMetrics> {
        let metrics = self.query_metrics.read().await;
        metrics.get(operation).map(|data| QueryMetrics {
            operation: operation.to_string(),
            execution_count: data.count,
            total_duration_ms: data.total_ms,
            avg_duration_ms: if data.count > 0 {
                data.total_ms as f64 / data.count as f64
            } else {
                0.0
            },
            min_duration_ms: data.min_ms,
            max_duration_ms: data.max_ms,
            slow_count: data.slow_count,
        })
    }

    /// Resets all query metrics.
    pub async fn reset_metrics(&self) {
        let mut metrics = self.query_metrics.write().await;
        metrics.clear();
    }

    /// Checks pool health and returns warnings if issues detected.
    pub async fn check_pool_health(&self) -> Vec<String> {
        let mut warnings = Vec::new();
        let stats = self.get_pool_stats().await;

        // Check for high utilization
        if stats.utilization_percent > 80.0 {
            warnings.push(format!(
                "High pool utilization: {:.1}% ({}/{} connections)",
                stats.utilization_percent, stats.active_connections, stats.max_connections
            ));
        }

        // Check if all connections are active (potential connection leak)
        if stats.idle_connections == 0 && stats.total_connections > 0 {
            warnings.push(
                "No idle connections - possible connection leak or consider increasing pool size".to_string()
            );
        }

        // Check if pool is at max capacity
        if stats.total_connections >= stats.max_connections {
            warnings.push(format!(
                "Pool at maximum capacity ({} connections) - consider increasing max_connections",
                stats.max_connections
            ));
        }

        warnings
    }
}

/// Helper for timing database queries.
///
/// # Example
/// ```ignore
/// let result = time_query(&monitor, "get_user", async {
///     user_storage.get_user(user_id).await
/// }).await?;
/// ```
pub async fn time_query<F, T>(
    monitor: &PerformanceMonitor,
    operation: &str,
    f: F,
) -> Result<T, sqlx::Error>
where
    F: std::future::Future<Output = Result<T, sqlx::Error>>,
{
    let start = Instant::now();
    let result = f.await;
    let duration = start.elapsed();

    monitor.record_query(operation, duration).await;

    result
}

/// Macro for automatically timing queries.
///
/// # Example
/// ```ignore
/// let user = timed_query!(monitor, "get_user", user_storage.get_user(user_id)).await?;
/// ```
#[macro_export]
macro_rules! timed_query {
    ($monitor:expr, $operation:expr, $expr:expr) => {
        $crate::storage::performance::time_query(
            &$monitor,
            $operation,
            async { $expr },
        )
        .await
    };
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_pool_stats_calculation() {
        // Test utilization calculation
        let util = ((10 - 2) as f64 / 10 as f64) * 100.0;
        assert_eq!(util, 80.0);
    }

    #[test]
    fn test_query_metrics_aggregation() {
        let mut data = QueryMetricsData {
            count: 0,
            total_ms: 0,
            min_ms: 100,
            max_ms: 0,
            slow_count: 0,
        };

        // Simulate queries: 50ms, 75ms, 150ms (slow), 25ms
        let queries = vec![50, 75, 150, 25];
        let slow_threshold = 100;

        for &ms in &queries {
            data.count += 1;
            data.total_ms += ms;
            data.min_ms = data.min_ms.min(ms);
            data.max_ms = data.max_ms.max(ms);
            if ms > slow_threshold {
                data.slow_count += 1;
            }
        }

        assert_eq!(data.count, 4);
        assert_eq!(data.total_ms, 300);
        assert_eq!(data.min_ms, 25);
        assert_eq!(data.max_ms, 150);
        assert_eq!(data.slow_count, 1);
    }
}
