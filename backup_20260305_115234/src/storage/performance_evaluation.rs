use sqlx::{Pool, Postgres};
use std::time::Duration;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PerformanceEvaluation {
    pub timestamp: DateTime<Utc>,
    pub overall_score: f64,
    pub query_performance: QueryPerformanceMetrics,
    pub connection_performance: ConnectionPerformanceMetrics,
    pub transaction_performance: TransactionMetrics,
    pub storage_metrics: StorageMetrics,
    pub cache_performance: CacheMetrics,
    pub recommendations: Vec<PerformanceRecommendation>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct QueryPerformanceMetrics {
    pub average_query_time_ms: f64,
    pub p50_query_time_ms: f64,
    pub p95_query_time_ms: f64,
    pub p99_query_time_ms: f64,
    pub slow_query_count: u64,
    pub very_slow_query_count: u64,
    pub total_queries: u64,
    pub queries_per_second: f64,
    pub score: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConnectionPerformanceMetrics {
    pub pool_utilization: f64,
    pub average_wait_time_ms: f64,
    pub connection_errors: u64,
    pub timeout_count: u64,
    pub active_connections: u32,
    pub idle_connections: u32,
    pub max_connections: u32,
    pub score: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TransactionMetrics {
    pub transactions_per_second: f64,
    pub transaction_commit_rate: f64,
    pub transaction_rollback_rate: f64,
    pub average_transaction_time_ms: f64,
    pub deadlocks: u64,
    pub score: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StorageMetrics {
    pub database_size_mb: f64,
    pub table_count: u64,
    pub index_size_mb: f64,
    pub data_size_mb: f64,
    pub vacuum_pending_tables: u64,
    pub bloat_estimate_percentage: f64,
    pub score: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CacheMetrics {
    pub cache_hit_ratio: f64,
    pub cache_hit_count: u64,
    pub cache_miss_count: u64,
    pub cache_eviction_count: u64,
    pub memory_usage_mb: f64,
    pub score: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum PerformanceRecommendationType {
    QueryOptimization,
    IndexOptimization,
    ConnectionPoolAdjustment,
    MemoryIncrease,
    VacuumAnalyze,
    ConfigurationChange,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PerformanceRecommendation {
    pub r#type: PerformanceRecommendationType,
    pub priority: u8,
    pub title: String,
    pub description: String,
    pub impact: String,
    pub estimated_improvement: String,
}

pub struct PerformanceEvaluator {
    pool: Pool<Postgres>,
    query_times: Vec<Duration>,
    slow_query_threshold: Duration,
}

impl PerformanceEvaluator {
    pub fn new(pool: Pool<Postgres>, slow_query_threshold_ms: u64) -> Self {
        Self {
            pool,
            query_times: Vec::with_capacity(10000),
            slow_query_threshold: Duration::from_millis(slow_query_threshold_ms),
        }
    }

    pub fn record_query_time(&mut self, duration: Duration) {
        self.query_times.push(duration);
        if self.query_times.len() >= 10000 {
            self.query_times.remove(0);
        }
    }

    pub async fn evaluate(&self) -> PerformanceEvaluation {
        let query_metrics = self.evaluate_query_performance().await;
        let connection_metrics = self.evaluate_connection_performance().await;
        let transaction_metrics = self.evaluate_transaction_performance().await;
        let storage_metrics = self.evaluate_storage_performance().await;
        let cache_metrics = self.evaluate_cache_performance().await;

        let overall_score = self.calculate_overall_score(
            &query_metrics,
            &connection_metrics,
            &transaction_metrics,
            &storage_metrics,
            &cache_metrics,
        );

        let recommendations = self.generate_recommendations(
            &query_metrics,
            &connection_metrics,
            &transaction_metrics,
            &storage_metrics,
            &cache_metrics,
        );

        PerformanceEvaluation {
            timestamp: Utc::now(),
            overall_score,
            query_performance: query_metrics,
            connection_performance: connection_metrics,
            transaction_performance: transaction_metrics,
            storage_metrics,
            cache_performance: cache_metrics,
            recommendations,
        }
    }

    async fn evaluate_query_performance(&self) -> QueryPerformanceMetrics {
        let avg_time = self.calculate_average_query_time();
        let (p50, p95, p99) = self.calculate_percentiles();
        let slow_count = self.query_times.iter().filter(|&&t| t > self.slow_query_threshold).count() as u64;
        let very_slow_count = self.query_times.iter().filter(|&&t| t > Duration::from_millis(1000)).count() as u64;
        let total = self.query_times.len() as u64;

        let score = self.calculate_query_score(avg_time, slow_count, total);

        QueryPerformanceMetrics {
            average_query_time_ms: avg_time,
            p50_query_time_ms: p50,
            p95_query_time_ms: p95,
            p99_query_time_ms: p99,
            slow_query_count: slow_count,
            very_slow_query_count: very_slow_count,
            total_queries: total,
            queries_per_second: 0.0,
            score,
        }
    }

    async fn evaluate_connection_performance(&self) -> ConnectionPerformanceMetrics {
        let pool_size = self.pool.size();
        let idle = self.pool.num_idle();
        let busy = pool_size.saturating_sub(idle);
        let max_connections = 50;

        let utilization = if max_connections > 0 {
            (pool_size as f64 / max_connections as f64) * 100.0
        } else {
            0.0
        };

        ConnectionPerformanceMetrics {
            pool_utilization: utilization,
            average_wait_time_ms: 0.0,
            connection_errors: 0,
            timeout_count: 0,
            active_connections: busy as u32,
            idle_connections: idle as u32,
            max_connections: max_connections as u32,
            score: self.calculate_connection_score(utilization),
        }
    }

    async fn evaluate_transaction_performance(&self) -> TransactionMetrics {
        TransactionMetrics {
            transactions_per_second: 0.0,
            transaction_commit_rate: 100.0,
            transaction_rollback_rate: 0.0,
            average_transaction_time_ms: 0.0,
            deadlocks: 0,
            score: 100.0,
        }
    }

    async fn evaluate_storage_performance(&self) -> StorageMetrics {
        StorageMetrics {
            database_size_mb: 0.0,
            table_count: 24,
            index_size_mb: 0.0,
            data_size_mb: 0.0,
            vacuum_pending_tables: 0,
            bloat_estimate_percentage: 0.0,
            score: 100.0,
        }
    }

    async fn evaluate_cache_performance(&self) -> CacheMetrics {
        CacheMetrics {
            cache_hit_ratio: 0.0,
            cache_hit_count: 0,
            cache_miss_count: 0,
            cache_eviction_count: 0,
            memory_usage_mb: 0.0,
            score: 50.0,
        }
    }

    fn calculate_average_query_time(&self) -> f64 {
        if self.query_times.is_empty() {
            return 0.0;
        }
        let total: Duration = self.query_times.iter().sum();
        total.as_millis() as f64 / self.query_times.len() as f64
    }

    fn calculate_percentiles(&self) -> (f64, f64, f64) {
        if self.query_times.is_empty() {
            return (0.0, 0.0, 0.0);
        }

        let mut sorted: Vec<_> = self.query_times.clone();
        sorted.sort();

        let p50 = Self::percentile(&sorted, 50.0);
        let p95 = Self::percentile(&sorted, 95.0);
        let p99 = Self::percentile(&sorted, 99.0);

        (p50, p95, p99)
    }

    fn percentile(sorted: &[Duration], p: f64) -> f64 {
        if sorted.is_empty() {
            return 0.0;
        }
        let index = (p / 100.0) * (sorted.len() - 1) as f64;
        let lower = index.floor() as usize;
        let upper = index.ceil() as usize;

        if lower == upper {
            sorted[lower].as_millis() as f64
        } else {
            let weight = index - lower as f64;
            let lower_val = sorted[lower].as_millis() as f64;
            let upper_val = sorted[upper].as_millis() as f64;
            lower_val * (1.0 - weight) + upper_val * weight
        }
    }

    fn calculate_query_score(&self, avg_time: f64, slow_count: u64, total: u64) -> f64 {
        let time_score = if avg_time < 1.0 { 100.0 } 
        else if avg_time < 10.0 { 90.0 - (avg_time - 1.0) * 5.0 }
        else if avg_time < 100.0 { 40.0 - (avg_time - 10.0) * 0.3 }
        else { 0.0 };

        let slow_ratio = if total > 0 { slow_count as f64 / total as f64 } else { 0.0 };
        let slow_penalty = slow_ratio * 30.0;

        (time_score - slow_penalty).max(0.0)
    }

    fn calculate_connection_score(&self, utilization: f64) -> f64 {
        if utilization < 50.0 { 100.0 }
        else if utilization < 70.0 { 100.0 - (utilization - 50.0) * 2.5 }
        else if utilization < 85.0 { 50.0 - (utilization - 70.0) * 2.0 }
        else if utilization < 95.0 { 20.0 - (utilization - 85.0) * 2.0 }
        else { 0.0 }
    }

    fn calculate_overall_score(
        &self,
        query: &QueryPerformanceMetrics,
        connection: &ConnectionPerformanceMetrics,
        transaction: &TransactionMetrics,
        storage: &StorageMetrics,
        cache: &CacheMetrics,
    ) -> f64 {
        let weights = [
            (query.score, 0.30),
            (connection.score, 0.25),
            (transaction.score, 0.20),
            (storage.score, 0.15),
            (cache.score, 0.10),
        ];

        let total: f64 = weights.iter().map(|(score, weight)| score * weight).sum();
        total / weights.iter().map(|(_, weight)| weight).sum()
    }

    fn generate_recommendations(
        &self,
        query: &QueryPerformanceMetrics,
        connection: &ConnectionPerformanceMetrics,
        transaction: &TransactionMetrics,
        storage: &StorageMetrics,
        cache: &CacheMetrics,
    ) -> Vec<PerformanceRecommendation> {
        let mut recommendations = Vec::new();

        if query.p95_query_time_ms > 100.0 {
            recommendations.push(PerformanceRecommendation {
                r#type: PerformanceRecommendationType::QueryOptimization,
                priority: 1,
                title: "优化慢查询".to_string(),
                description: format!("P95 查询时间 ({:.2}ms) 超过 100ms 阈值", query.p95_query_time_ms),
                impact: "高".to_string(),
                estimated_improvement: "50-80%".to_string(),
            });
        }

        if connection.pool_utilization > 85.0 {
            recommendations.push(PerformanceRecommendation {
                r#type: PerformanceRecommendationType::ConnectionPoolAdjustment,
                priority: 2,
                title: "增加连接池大小".to_string(),
                description: format!("连接池利用率 ({:.1}%) 超过 85%", connection.pool_utilization),
                impact: "中".to_string(),
                estimated_improvement: "30-50%".to_string(),
            });
        }

        if query.slow_query_count > 10 {
            recommendations.push(PerformanceRecommendation {
                r#type: PerformanceRecommendationType::IndexOptimization,
                priority: 3,
                title: "添加缺失索引".to_string(),
                description: format!("检测到 {} 个慢查询，可能缺少索引", query.slow_query_count),
                impact: "中".to_string(),
                estimated_improvement: "40-60%".to_string(),
            });
        }

        if storage.vacuum_pending_tables > 0 {
            recommendations.push(PerformanceRecommendation {
                r#type: PerformanceRecommendationType::VacuumAnalyze,
                priority: 4,
                title: "执行 VACUUM ANALYZE".to_string(),
                description: format!("有 {} 个表需要 VACUUM", storage.vacuum_pending_tables),
                impact: "低".to_string(),
                estimated_improvement: "10-20%".to_string(),
            });
        }

        recommendations
    }

    pub fn to_json(&self, evaluation: &PerformanceEvaluation) -> serde_json::Value {
        serde_json::json!({
            "timestamp": evaluation.timestamp.to_rfc3339(),
            "overall_score": evaluation.overall_score,
            "query_performance": {
                "average_query_time_ms": evaluation.query_performance.average_query_time_ms,
                "p50_query_time_ms": evaluation.query_performance.p50_query_time_ms,
                "p95_query_time_ms": evaluation.query_performance.p95_query_time_ms,
                "p99_query_time_ms": evaluation.query_performance.p99_query_time_ms,
                "slow_query_count": evaluation.query_performance.slow_query_count,
                "score": evaluation.query_performance.score,
            },
            "connection_performance": {
                "pool_utilization": evaluation.connection_performance.pool_utilization,
                "active_connections": evaluation.connection_performance.active_connections,
                "idle_connections": evaluation.connection_performance.idle_connections,
                "max_connections": evaluation.connection_performance.max_connections,
                "score": evaluation.connection_performance.score,
            },
            "transaction_performance": {
                "transactions_per_second": evaluation.transaction_performance.transactions_per_second,
                "deadlocks": evaluation.transaction_performance.deadlocks,
                "score": evaluation.transaction_performance.score,
            },
            "storage_metrics": {
                "table_count": evaluation.storage_metrics.table_count,
                "score": evaluation.storage_metrics.score,
            },
            "recommendations": evaluation.recommendations.iter().map(|r| serde_json::json!({
                "type": format!("{:?}", r.r#type),
                "priority": r.priority,
                "title": r.title,
                "description": r.description,
                "impact": r.impact,
                "estimated_improvement": r.estimated_improvement,
            })).collect::<Vec<_>>(),
        })
    }
}
