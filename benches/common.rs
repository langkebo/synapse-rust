//! Common utilities for benchmarking.
//!
//! This module provides shared configuration and utilities for all benchmarks.

use criterion::{BenchmarkId, Criterion};
use std::time::Duration;

/// Benchmark configuration constants.
pub struct BenchmarkConfig;

impl BenchmarkConfig {
    /// Default sample size for benchmarks
    pub const SAMPLE_SIZE: usize = 100;

    /// Default warm-up time in seconds
    pub const WARM_UP_TIME: u64 = 3;

    /// Default measurement time in seconds
    pub const MEASUREMENT_TIME: u64 = 5;

    /// Small dataset size for quick benchmarks
    pub const SMALL_DATASET: usize = 10;

    /// Medium dataset size
    pub const MEDIUM_DATASET: usize = 100;

    /// Large dataset size
    pub const LARGE_DATASET: usize = 1000;

    /// Extra large dataset size for stress testing
    pub const XLARGE_DATASET: usize = 10000;
}

/// Creates a configured Criterion instance.
pub fn configure_criterion() -> Criterion {
    Criterion::default()
        .sample_size(BenchmarkConfig::SAMPLE_SIZE)
        .warm_up_time(Duration::from_secs(BenchmarkConfig::WARM_UP_TIME))
        .measurement_time(Duration::from_secs(BenchmarkConfig::MEASUREMENT_TIME))
        .significance_level(0.05)
        .confidence_level(0.95)
}

/// Dataset sizes for parameterized benchmarks.
pub const DATASET_SIZES: [usize; 4] = [
    BenchmarkConfig::SMALL_DATASET,
    BenchmarkConfig::MEDIUM_DATASET,
    BenchmarkConfig::LARGE_DATASET,
    BenchmarkConfig::XLARGE_DATASET,
];

/// Benchmark result metadata.
#[derive(Debug, Clone)]
pub struct BenchmarkMetadata {
    pub name: String,
    pub category: BenchmarkCategory,
    pub description: String,
    pub baseline: Option<f64>,
    pub target: f64,
}

/// Categories of benchmarks.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BenchmarkCategory {
    DatabaseQuery,
    ApiEndpoint,
    Validation,
    Crypto,
    Cache,
    Serialization,
}

impl BenchmarkCategory {
    pub fn as_str(&self) -> &'static str {
        match self {
            BenchmarkCategory::DatabaseQuery => "database_query",
            BenchmarkCategory::ApiEndpoint => "api_endpoint",
            BenchmarkCategory::Validation => "validation",
            BenchmarkCategory::Crypto => "crypto",
            BenchmarkCategory::Cache => "cache",
            BenchmarkCategory::Serialization => "serialization",
        }
    }
}

/// Performance comparison result.
#[derive(Debug, Clone)]
pub struct ComparisonResult {
    pub operation: String,
    pub baseline_time_ms: f64,
    pub optimized_time_ms: f64,
    pub improvement_percent: f64,
    pub status: ComparisonStatus,
}

impl ComparisonResult {
    pub fn new(operation: &str, baseline_ms: f64, optimized_ms: f64) -> Self {
        let improvement_percent = if baseline_ms > 0.0 {
            ((baseline_ms - optimized_ms) / baseline_ms) * 100.0
        } else {
            0.0
        };

        let status = if improvement_percent >= 80.0 {
            ComparisonStatus::Excellent
        } else if improvement_percent >= 50.0 {
            ComparisonStatus::Good
        } else if improvement_percent >= 20.0 {
            ComparisonStatus::Moderate
        } else if improvement_percent > 0.0 {
            ComparisonStatus::Minimal
        } else {
            ComparisonStatus::Regression
        };

        Self {
            operation: operation.to_string(),
            baseline_time_ms: baseline_ms,
            optimized_time_ms: optimized_ms,
            improvement_percent,
            status,
        }
    }

    pub fn format_markdown(&self) -> String {
        let status_icon = match self.status {
            ComparisonStatus::Excellent => "🚀",
            ComparisonStatus::Good => "✅",
            ComparisonStatus::Moderate => "⚠️",
            ComparisonStatus::Minimal => "📝",
            ComparisonStatus::Regression => "❌",
        };

        format!(
            "| {} {} | {:.2} ms | {:.2} ms | {:.1}% |",
            status_icon, self.operation, self.baseline_time_ms, self.optimized_time_ms, self.improvement_percent
        )
    }
}

/// Status of a performance comparison.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ComparisonStatus {
    Excellent, // >= 80% improvement
    Good,      // >= 50% improvement
    Moderate,  // >= 20% improvement
    Minimal,   // > 0% improvement
    Regression,// Worsened performance
}

/// Helper to create benchmark IDs with context.
pub fn benchmark_id(name: &str, size: usize) -> BenchmarkId {
    BenchmarkId::new(name, size)
}

/// Setup function that returns a mock database pool for benchmarks.
#[cfg(feature = "benchmark")]
pub fn setup_benchmark_pool() -> sqlx::PgPool {
    use std::env;

    let database_url = env::var("BENCHMARK_DATABASE_URL")
        .unwrap_or_else(|_| "postgresql://synapse:synapse@localhost:5432/synapse_bench".to_string());

    tokio::runtime::Runtime::new()
        .unwrap()
        .block_on(async {
            sqlx::PgPool::connect(&database_url).await.unwrap()
        })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_comparison_result_calculation() {
        let result = ComparisonResult::new("test_query", 100.0, 20.0);
        assert_eq!(result.baseline_time_ms, 100.0);
        assert_eq!(result.optimized_time_ms, 20.0);
        assert_eq!(result.improvement_percent, 80.0);
        assert_eq!(result.status, ComparisonStatus::Excellent);
    }

    #[test]
    fn test_comparison_result_regression() {
        let result = ComparisonResult::new("test_query", 20.0, 100.0);
        assert_eq!(result.improvement_percent, -400.0);
        assert_eq!(result.status, ComparisonStatus::Regression);
    }

    #[test]
    fn test_category_as_str() {
        assert_eq!(BenchmarkCategory::DatabaseQuery.as_str(), "database_query");
        assert_eq!(BenchmarkCategory::ApiEndpoint.as_str(), "api_endpoint");
    }
}
