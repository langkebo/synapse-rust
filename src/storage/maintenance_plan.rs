use sqlx::{Pool, Postgres};
use std::time::Duration;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MaintenancePlan {
    pub name: String,
    pub description: String,
    pub schedule: MaintenanceSchedule,
    pub tasks: Vec<MaintenanceTask>,
    pub thresholds: AlertThresholds,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MaintenanceSchedule {
    pub vacuum_interval: Duration,
    pub analyze_interval: Duration,
    pub reindex_interval: Duration,
    pub health_check_interval: Duration,
    pub integrity_check_interval: Duration,
}

impl Default for MaintenanceSchedule {
    fn default() -> Self {
        Self {
            vacuum_interval: Duration::from_secs(3600),
            analyze_interval: Duration::from_secs(1800),
            reindex_interval: Duration::from_secs(86400),
            health_check_interval: Duration::from_secs(60),
            integrity_check_interval: Duration::from_secs(3600),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum MaintenanceTask {
    Vacuum { tables: Vec<String>, full: bool },
    Analyze { tables: Vec<String> },
    Reindex { tables: Vec<String> },
    HealthCheck,
    IntegrityCheck,
    PerformanceEvaluation,
    ClearExpiredSessions,
    ClearOldLogs,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AlertThresholds {
    pub cpu_usage_warning: f64,
    pub cpu_usage_critical: f64,
    pub memory_usage_warning: f64,
    pub memory_usage_critical: f64,
    pub disk_usage_warning: f64,
    pub disk_usage_critical: f64,
    pub connection_pool_warning: f64,
    pub connection_pool_critical: f64,
    pub slow_query_threshold_ms: u64,
    pub max_slow_queries: u64,
    pub integrity_score_warning: f64,
    pub integrity_score_critical: f64,
}

impl Default for AlertThresholds {
    fn default() -> Self {
        Self {
            cpu_usage_warning: 70.0,
            cpu_usage_critical: 90.0,
            memory_usage_warning: 75.0,
            memory_usage_critical: 90.0,
            disk_usage_warning: 80.0,
            disk_usage_critical: 95.0,
            connection_pool_warning: 75.0,
            connection_pool_critical: 90.0,
            slow_query_threshold_ms: 100,
            max_slow_queries: 10,
            integrity_score_warning: 80.0,
            integrity_score_critical: 60.0,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MaintenanceReport {
    pub timestamp: DateTime<Utc>,
    pub tasks_executed: u64,
    pub tasks_succeeded: u64,
    pub tasks_failed: u64,
    pub total_duration_ms: u64,
    pub results: Vec<MaintenanceTaskResult>,
    pub recommendations: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MaintenanceTaskResult {
    pub task: String,
    pub status: TaskStatus,
    pub duration_ms: u64,
    pub affected_rows: Option<u64>,
    pub details: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum TaskStatus {
    Success,
    Skipped,
    Failed,
    Warning,
}

pub struct MaintenanceManager {
    pool: Pool<Postgres>,
    schedule: MaintenanceSchedule,
    thresholds: AlertThresholds,
    last_vacuum: DateTime<Utc>,
    last_analyze: DateTime<Utc>,
    last_reindex: DateTime<Utc>,
}

impl MaintenanceManager {
    pub fn new(pool: Pool<Postgres>) -> Self {
        Self {
            pool,
            schedule: MaintenanceSchedule::default(),
            thresholds: AlertThresholds::default(),
            last_vacuum: Utc::now() - Duration::from_secs(86400),
            last_analyze: Utc::now() - Duration::from_secs(86400),
            last_reindex: Utc::now() - Duration::from_secs(86400 * 7),
        }
    }

    pub fn with_schedule(pool: Pool<Postgres>, schedule: MaintenanceSchedule) -> Self {
        Self {
            pool,
            schedule,
            thresholds: AlertThresholds::default(),
            last_vacuum: Utc::now() - schedule.vacuum_interval * 2,
            last_analyze: Utc::now() - schedule.analyze_interval * 2,
            last_reindex: Utc::now() - schedule.reindex_interval * 2,
        }
    }

    pub async fn run_maintenance(&mut self) -> MaintenanceReport {
        let start = std::time::Instant::now();
        let mut results = Vec::new();
        let mut tasks_executed = 0u64;
        let mut tasks_succeeded = 0u64;
        let mut tasks_failed = 0u64;
        let mut recommendations = Vec::new();

        let now = Utc::now();

        if now.signed_duration_since(self.last_vacuum) > self.schedule.vacuum_interval {
            match self.vacuum_all().await {
                Ok(result) => {
                    self.last_vacuum = now;
                    tasks_executed += 1;
                    if result.status == TaskStatus::Success {
                        tasks_succeeded += 1;
                    } else {
                        tasks_failed += 1;
                    }
                    results.push(result);
                }
                Err(e) => {
                    results.push(MaintenanceTaskResult {
                        task: "VACUUM".to_string(),
                        status: TaskStatus::Failed,
                        duration_ms: 0,
                        affected_rows: None,
                        details: Some(format!("Error: {}", e)),
                    });
                    tasks_executed += 1;
                    tasks_failed += 1;
                }
            }
        }

        if now.signed_duration_since(self.last_analyze) > self.schedule.analyze_interval {
            match self.analyze_all().await {
                Ok(result) => {
                    self.last_analyze = now;
                    tasks_executed += 1;
                    if result.status == TaskStatus::Success {
                        tasks_succeeded += 1;
                    } else {
                        tasks_failed += 1;
                    }
                    results.push(result);
                }
                Err(e) => {
                    results.push(MaintenanceTaskResult {
                        task: "ANALYZE".to_string(),
                        status: TaskStatus::Failed,
                        duration_ms: 0,
                        affected_rows: None,
                        details: Some(format!("Error: {}", e)),
                    });
                    tasks_executed += 1;
                    tasks_failed += 1;
                }
            }
        }

        match self.clear_expired_sessions().await {
            Ok(result) => {
                tasks_executed += 1;
                if result.status == TaskStatus::Success {
                    tasks_succeeded += 1;
                } else {
                    tasks_failed += 1;
                }
                results.push(result);
            }
            Err(e) => {
                results.push(MaintenanceTaskResult {
                    task: "Clear Expired Sessions".to_string(),
                    status: TaskStatus::Failed,
                    duration_ms: 0,
                    affected_rows: None,
                    details: Some(format!("Error: {}", e)),
                });
                tasks_executed += 1;
                tasks_failed += 1;
            }
        }

        if tasks_failed > 0 {
            recommendations.push("Check failed maintenance tasks and resolve underlying issues".to_string());
        }

        let duration_ms = start.elapsed().as_millis() as u64;

        MaintenanceReport {
            timestamp: now,
            tasks_executed,
            tasks_succeeded,
            tasks_failed,
            total_duration_ms: duration_ms,
            results,
            recommendations,
        }
    }

    async fn vacuum_all(&self) -> Result<MaintenanceTaskResult, sqlx::Error> {
        let start = std::time::Instant::now();

        let tables = vec![
            "users", "devices", "access_tokens", "refresh_tokens",
            "rooms", "events", "room_memberships", "presence",
            "private_sessions", "private_messages",
        ];

        let mut total_pages = 0u64;

        for table in &tables {
            let result = sqlx::query(&format!("VACUUM {}", table))
                .execute(&self.pool)
                .await;
            match result {
                Ok(_) => {}
                Err(_) => {
                    let _ = sqlx::query(&format!("VACUUM ANALYZE {}", table))
                        .execute(&self.pool)
                        .await;
                }
            }
        }

        let duration_ms = start.elapsed().as_millis() as u64;

        Ok(MaintenanceTaskResult {
            task: "VACUUM".to_string(),
            status: TaskStatus::Success,
            duration_ms,
            affected_rows: Some(total_pages),
            details: Some(format!("Vacuumed {} tables", tables.len())),
        })
    }

    async fn analyze_all(&self) -> Result<MaintenanceTaskResult, sqlx::Error> {
        let start = std::time::Instant::now();

        let tables = vec![
            "users", "devices", "access_tokens", "refresh_tokens",
            "rooms", "events", "room_memberships", "presence",
            "private_sessions", "private_messages",
        ];

        for table in &tables {
            let _ = sqlx::query(&format!("ANALYZE {}", table))
                .execute(&self.pool)
                .await;
        }

        let duration_ms = start.elapsed().as_millis() as u64;

        Ok(MaintenanceTaskResult {
            task: "ANALYZE".to_string(),
            status: TaskStatus::Success,
            duration_ms,
            affected_rows: Some(tables.len() as u64),
            details: Some(format!("Analyzed {} tables", tables.len())),
        })
    }

    async fn clear_expired_sessions(&self) -> Result<MaintenanceTaskResult, sqlx::Error> {
        let start = std::time::Instant::now();

        let thirty_days_ago = Utc::now().naive_utc() - Duration::days(30);
        let cutoff_timestamp = thirty_days_ago.timestamp();

        let result = sqlx::query(
            "DELETE FROM private_sessions WHERE updated_ts < $1 AND last_message IS NULL",
        )
        .bind(cutoff_timestamp)
        .execute(&self.pool)
        .await?;

        let duration_ms = start.elapsed().as_millis() as u64;

        Ok(MaintenanceTaskResult {
            task: "Clear Expired Sessions".to_string(),
            status: TaskStatus::Success,
            duration_ms,
            affected_rows: Some(result.rows_affected()),
            details: Some(format!("Cleared {} expired sessions", result.rows_affected())),
        })
    }

    pub async fn check_thresholds(&self, metrics: &crate::storage::PerformanceMetrics) -> Vec<Alert> {
        let mut alerts = Vec::new();

        if metrics.slow_queries_count > self.thresholds.max_slow_queries {
            alerts.push(Alert {
                severity: AlertSeverity::Warning,
                message: format!(
                    "Slow query count ({}) exceeds threshold ({})",
                    metrics.slow_queries_count, self.thresholds.max_slow_queries
                ),
                timestamp: Utc::now(),
                metric: "slow_queries".to_string(),
            });
        }

        alerts
    }

    pub fn to_json(&self, report: &MaintenanceReport) -> serde_json::Value {
        serde_json::json!({
            "timestamp": report.timestamp.to_rfc3339(),
            "tasks_executed": report.tasks_executed,
            "tasks_succeeded": report.tasks_succeeded,
            "tasks_failed": report.tasks_failed,
            "total_duration_ms": report.total_duration_ms,
            "results": report.results.iter().map(|r| serde_json::json!({
                "task": r.task,
                "status": format!("{:?}", r.status),
                "duration_ms": r.duration_ms,
                "affected_rows": r.affected_rows,
                "details": r.details,
            })).collect::<Vec<_>>(),
            "recommendations": report.recommendations,
        })
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Alert {
    pub severity: AlertSeverity,
    pub message: String,
    pub timestamp: DateTime<Utc>,
    pub metric: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum AlertSeverity {
    Info,
    Warning,
    Critical,
}

pub struct OptimizationGuide {
    pub recommendations: Vec<OptimizationRecommendation>,
}

impl OptimizationGuide {
    pub fn new() -> Self {
        let recommendations = vec![
            OptimizationRecommendation {
                category: "Query Optimization".to_string(),
                priority: 1,
                title: "添加缺失索引".to_string(),
                description: "根据慢查询日志分析，为频繁查询的列添加索引".to_string(),
                impact: "查询性能提升 50-200%".to_string(),
                effort: "低".to_string(),
            },
            OptimizationRecommendation {
                category: "Connection Pool".to_string(),
                priority: 2,
                title: "调整连接池大小".to_string(),
                description: "根据并发用户数调整连接池大小，避免连接耗尽或资源浪费".to_string(),
                impact: "连接稳定性提升 30%".to_string(),
                effort: "低".to_string(),
            },
            OptimizationRecommendation {
                category: "Storage".to_string(),
                priority: 3,
                title: "定期执行 VACUUM".to_string(),
                description: "定期清理死元组，回收磁盘空间，维持查询性能".to_string(),
                impact: "存储效率提升 10-30%".to_string(),
                effort: "低".to_string(),
            },
            OptimizationRecommendation {
                category: "Caching".to_string(),
                priority: 4,
                title: "优化缓存策略".to_string(),
                description: "增加缓存命中率，减少数据库查询压力".to_string(),
                impact: "查询负载降低 40-60%".to_string(),
                effort: "中".to_string(),
            },
            OptimizationRecommendation {
                category: "Schema".to_string(),
                priority: 5,
                title: "分区大表".to_string(),
                description: "对事件表等大表进行时间分区，提升查询性能".to_string(),
                impact: "查询性能提升 3-10 倍".to_string(),
                effort: "高".to_string(),
            },
        ];

        Self { recommendations }
    }

    pub fn to_json(&self) -> serde_json::Value {
        serde_json::json!({
            "recommendations": self.recommendations.iter().map(|r| serde_json::json!({
                "category": r.category,
                "priority": r.priority,
                "title": r.title,
                "description": r.description,
                "impact": r.impact,
                "effort": r.effort,
            })).collect::<Vec<_>>(),
        })
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OptimizationRecommendation {
    pub category: String,
    pub priority: u8,
    pub title: String,
    pub description: String,
    pub impact: String,
    pub effort: String,
}
