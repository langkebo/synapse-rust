use crate::storage::{DatabaseHealthStatus, PerformanceMetrics};
use serde::{Deserialize, Serialize};
use std::collections::VecDeque;
use std::time::Duration;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PerformanceBaseline {
    pub average_query_time_ms: f64,
    pub p95_query_time_ms: f64,
    pub p99_query_time_ms: f64,
    pub average_tps: f64,
    pub average_cache_hit_ratio: f64,
    pub max_slow_queries: u64,
    pub samples_count: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PerformanceTrend {
    pub trend_type: TrendType,
    pub metric_name: String,
    pub current_value: f64,
    pub baseline_value: f64,
    pub change_percentage: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum TrendType {
    Improving,
    Degrading,
    Stable,
}

pub struct PerformanceAnalyzer {
    baselines: Vec<PerformanceBaseline>,
    recent_metrics: VecDeque<PerformanceMetrics>,
    max_samples: usize,
    window_size: Duration,
}

impl PerformanceAnalyzer {
    pub fn new(window_size: Duration, max_samples: usize) -> Self {
        Self {
            baselines: Vec::new(),
            recent_metrics: VecDeque::with_capacity(max_samples),
            max_samples,
            window_size,
        }
    }

    pub fn add_metrics(&mut self, metrics: PerformanceMetrics) {
        if self.recent_metrics.len() >= self.max_samples {
            self.recent_metrics.pop_front();
        }
        self.recent_metrics.push_back(metrics);
    }

    pub fn calculate_baseline(&self) -> Option<PerformanceBaseline> {
        if self.recent_metrics.is_empty() {
            return None;
        }

        let query_times: Vec<f64> = self
            .recent_metrics
            .iter()
            .map(|m| m.average_query_time_ms)
            .collect();

        let tps_values: Vec<f64> = self
            .recent_metrics
            .iter()
            .map(|m| m.transactions_per_second)
            .collect();

        let cache_ratios: Vec<f64> = self
            .recent_metrics
            .iter()
            .map(|m| m.cache_hit_ratio)
            .collect();

        let slow_query_counts: Vec<u64> = self
            .recent_metrics
            .iter()
            .map(|m| m.slow_queries_count)
            .collect();

        let avg_query = Self::average(&query_times);
        let p95 = Self::percentile(&query_times, 95.0);
        let p99 = Self::percentile(&query_times, 99.0);
        let avg_tps = Self::average(&tps_values);
        let avg_cache = Self::average(&cache_ratios);
        let max_slow = slow_query_counts.into_iter().max().unwrap_or(0);

        Some(PerformanceBaseline {
            average_query_time_ms: avg_query,
            p95_query_time_ms: p95,
            p99_query_time_ms: p99,
            average_tps: avg_tps,
            average_cache_hit_ratio: avg_cache,
            max_slow_queries: max_slow,
            samples_count: self.recent_metrics.len(),
        })
    }

    pub fn analyze_trends(&self, current: &PerformanceMetrics) -> Vec<PerformanceTrend> {
        let mut trends = Vec::new();

        if let Some(baseline) = self.calculate_baseline() {
            trends.push(Self::analyze_single_trend(
                "average_query_time_ms",
                current.average_query_time_ms,
                baseline.average_query_time_ms,
                true,
            ));

            trends.push(Self::analyze_single_trend(
                "transactions_per_second",
                current.transactions_per_second,
                baseline.average_tps,
                false,
            ));

            trends.push(Self::analyze_single_trend(
                "cache_hit_ratio",
                current.cache_hit_ratio,
                baseline.average_cache_hit_ratio,
                false,
            ));
        }

        trends
    }

    fn analyze_single_trend(
        metric_name: &str,
        current: f64,
        baseline: f64,
        lower_is_better: bool,
    ) -> PerformanceTrend {
        let change_percentage = if baseline != 0.0 {
            ((current - baseline) / baseline) * 100.0
        } else {
            0.0
        };

        let trend_type = if lower_is_better {
            if change_percentage > 10.0 {
                TrendType::Degrading
            } else if change_percentage < -10.0 {
                TrendType::Improving
            } else {
                TrendType::Stable
            }
        } else {
            if change_percentage > 10.0 {
                TrendType::Improving
            } else if change_percentage < -10.0 {
                TrendType::Degrading
            } else {
                TrendType::Stable
            }
        };

        PerformanceTrend {
            trend_type,
            metric_name: metric_name.to_string(),
            current_value: current,
            baseline_value: baseline,
            change_percentage,
        }
    }

    fn average(values: &[f64]) -> f64 {
        if values.is_empty() {
            0.0
        } else {
            values.iter().sum::<f64>() / values.len() as f64
        }
    }

    fn percentile(values: &[f64], p: f64) -> f64 {
        if values.is_empty() {
            return 0.0;
        }

        let mut sorted = values.clone();
        sorted.sort_by(|a, b| a.partial_cmp(b).unwrap());

        let index = (p / 100.0) * (sorted.len() - 1) as f64;
        let lower = index.floor() as usize;
        let upper = index.ceil() as usize;

        if lower == upper {
            sorted[lower]
        } else {
            let weight = index - lower as f64;
            sorted[lower] * (1.0 - weight) + sorted[upper] * weight
        }
    }

    pub fn get_recent_samples_count(&self) -> usize {
        self.recent_metrics.len()
    }

    pub fn get_samples_capacity(&self) -> usize {
        self.max_samples
    }
}

pub struct BenchmarkRunner {
    analyzer: PerformanceAnalyzer,
}

impl BenchmarkRunner {
    pub fn new(sample_window: Duration, max_samples: usize) -> Self {
        Self {
            analyzer: PerformanceAnalyzer::new(sample_window, max_samples),
        }
    }

    pub fn record_metrics(&mut self, metrics: PerformanceMetrics) {
        self.analyzer.add_metrics(metrics);
    }

    pub fn get_current_baseline(&self) -> Option<PerformanceBaseline> {
        self.analyzer.calculate_baseline()
    }

    pub fn analyze_current_trends(&self, current: &PerformanceMetrics) -> Vec<PerformanceTrend> {
        self.analyzer.analyze_trends(current)
    }

    pub fn generate_report(&self, current: &PerformanceMetrics) -> PerformanceReport {
        let baseline = self.get_current_baseline();
        let trends = self.analyze_current_trends(current);

        PerformanceReport {
            timestamp: chrono::Utc::now(),
            current_metrics: current.clone(),
            baseline,
            trends,
            health_status: None,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PerformanceReport {
    pub timestamp: chrono::DateTime<chrono::Utc>,
    pub current_metrics: PerformanceMetrics,
    pub baseline: Option<PerformanceBaseline>,
    pub trends: Vec<PerformanceTrend>,
    pub health_status: Option<()>,
}

impl PerformanceReport {
    pub fn to_json(&self) -> serde_json::Value {
        serde_json::json!({
            "timestamp": self.timestamp.to_rfc3339(),
            "current_metrics": {
                "average_query_time_ms": self.current_metrics.average_query_time_ms,
                "slow_queries_count": self.current_metrics.slow_queries_count,
                "total_queries": self.current_metrics.total_queries,
                "transactions_per_second": self.current_metrics.transactions_per_second,
                "cache_hit_ratio": self.current_metrics.cache_hit_ratio,
                "deadlock_count": self.current_metrics.deadlock_count,
            },
            "baseline": self.baseline.as_ref().map(|b| serde_json::json!({
                "average_query_time_ms": b.average_query_time_ms,
                "p95_query_time_ms": b.p95_query_time_ms,
                "p99_query_time_ms": b.p99_query_time_ms,
                "average_tps": b.average_tps,
                "average_cache_hit_ratio": b.average_cache_hit_ratio,
                "max_slow_queries": b.max_slow_queries,
                "samples_count": b.samples_count,
            })),
            "trends": self.trends.iter().map(|t| serde_json::json!({
                "metric": t.metric_name,
                "trend": format!("{:?}", t.trend_type),
                "current": t.current_value,
                "baseline": t.baseline_value,
                "change_percentage": t.change_percentage,
            })).collect::<Vec<_>>(),
        })
    }
}
