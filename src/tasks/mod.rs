use chrono::Utc;
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::RwLock;
use tokio::time;
use tracing::{error, info, warn};

use crate::storage::maintenance::{DatabaseMaintenance, MaintenanceReport};
use crate::storage::{DataIntegrityReport, Database, DatabaseHealthStatus, PerformanceMetrics};

pub struct ScheduledTasks {
    database: Arc<Database>,
    last_health_status: Arc<RwLock<Option<DatabaseHealthStatus>>>,
    last_performance_metrics: Arc<RwLock<Option<PerformanceMetrics>>>,
    last_integrity_report: Arc<RwLock<Option<DataIntegrityReport>>>,
    last_maintenance_report: Arc<RwLock<Option<MaintenanceReport>>>,
    health_check_interval: Duration,
    performance_check_interval: Duration,
    integrity_check_interval: Duration,
    maintenance_interval: Duration,
}

impl ScheduledTasks {
    pub fn new(database: Arc<Database>) -> Self {
        Self {
            database,
            last_health_status: Arc::new(RwLock::new(None)),
            last_performance_metrics: Arc::new(RwLock::new(None)),
            last_integrity_report: Arc::new(RwLock::new(None)),
            last_maintenance_report: Arc::new(RwLock::new(None)),
            health_check_interval: Duration::from_secs(60),
            performance_check_interval: Duration::from_secs(300),
            integrity_check_interval: Duration::from_secs(3600),
            maintenance_interval: Duration::from_secs(86400),
        }
    }

    pub async fn start_all(&self) {
        self.start_health_check_task().await;
        self.start_performance_check_task().await;
        self.start_integrity_check_task().await;
        self.start_maintenance_task().await;
    }

    async fn start_health_check_task(&self) {
        let interval = self.health_check_interval;
        let database = self.database.clone();
        let last_status = self.last_health_status.clone();

        tokio::spawn(async move {
            let mut interval_timer = time::interval(interval);
            interval_timer.set_missed_tick_behavior(time::MissedTickBehavior::Skip);

            loop {
                interval_timer.tick().await;
                match database.health_check().await {
                    Ok(status) => {
                        *last_status.write().await = Some(status.clone());

                        if !status.is_healthy {
                            error!("Database health check failed: {:?}", status);
                        } else if status.connection_pool_status.connection_utilization > 85.0 {
                            warn!(
                                "High connection pool utilization: {:.1}%",
                                status.connection_pool_status.connection_utilization
                            );
                        }

                        info!(
                            "Database health check completed: healthy={}, pool utilization={:.1}%",
                            status.is_healthy, status.connection_pool_status.connection_utilization
                        );
                    }
                    Err(e) => {
                        error!("Failed to perform database health check: {}", e);
                    }
                }
            }
        });
    }

    async fn start_performance_check_task(&self) {
        let interval = self.performance_check_interval;
        let database = self.database.clone();
        let last_metrics = self.last_performance_metrics.clone();

        tokio::spawn(async move {
            let mut interval_timer = time::interval(interval);
            interval_timer.set_missed_tick_behavior(time::MissedTickBehavior::Skip);

            loop {
                interval_timer.tick().await;
                match database.get_performance_metrics().await {
                    Ok(metrics) => {
                        *last_metrics.write().await = Some(metrics.clone());

                        if metrics.slow_queries_count > 10 {
                            warn!(
                                "High slow query count: {} queries",
                                metrics.slow_queries_count
                            );
                        }

                        if metrics.average_query_time_ms > 100.0 {
                            warn!(
                                "High average query time: {:.2}ms",
                                metrics.average_query_time_ms
                            );
                        }

                        info!(
                            "Performance metrics: avg_query={:.2}ms, slow_queries={}, tps={:.2}",
                            metrics.average_query_time_ms,
                            metrics.slow_queries_count,
                            metrics.transactions_per_second
                        );
                    }
                    Err(e) => {
                        error!("Failed to collect performance metrics: {}", e);
                    }
                }
            }
        });
    }

    async fn start_integrity_check_task(&self) {
        let interval = self.integrity_check_interval;
        let database = self.database.clone();
        let last_report = self.last_integrity_report.clone();

        tokio::spawn(async move {
            let mut interval_timer = time::interval(interval);
            interval_timer.set_missed_tick_behavior(time::MissedTickBehavior::Skip);

            loop {
                interval_timer.tick().await;
                match database.verify_data_integrity().await {
                    Ok(report) => {
                        *last_report.write().await = Some(report.clone());

                        if report.overall_integrity_score < 80.0 {
                            error!(
                                "Data integrity issues detected: score={:.1}, violations={}",
                                report.overall_integrity_score,
                                report.foreign_key_violations.len()
                                    + report.orphaned_records.len()
                                    + report.duplicate_entries.len()
                            );
                        } else if report.overall_integrity_score < 90.0 {
                            warn!(
                                "Data integrity score below optimal: {:.1}",
                                report.overall_integrity_score
                            );
                        }

                        info!(
                            "Data integrity check: score={:.1}, violations={}, orphaned={}",
                            report.overall_integrity_score,
                            report.foreign_key_violations.len() + report.orphaned_records.len(),
                            report
                                .orphaned_records
                                .iter()
                                .map(|o| o.orphan_count)
                                .sum::<i64>()
                        );
                    }
                    Err(e) => {
                        error!("Failed to verify data integrity: {}", e);
                    }
                }
            }
        });
    }

    async fn start_maintenance_task(&self) {
        let interval = self.maintenance_interval;
        let pool = self.database.pool().clone();
        let last_report = self.last_maintenance_report.clone();

        tokio::spawn(async move {
            let mut interval_timer = time::interval(interval);
            interval_timer.set_missed_tick_behavior(time::MissedTickBehavior::Skip);

            loop {
                interval_timer.tick().await;
                info!("Starting scheduled database maintenance...");

                let maintenance = DatabaseMaintenance::new(pool.clone());
                match maintenance.perform_maintenance().await {
                    Ok(report) => {
                        *last_report.write().await = Some(report.clone());

                        if !report.errors.is_empty() {
                            warn!("Maintenance completed with {} errors", report.errors.len());
                        } else {
                            info!(
                                "Database maintenance completed: duration={}ms, vacuum tables={}, reindexed={}",
                                report.duration_ms,
                                report.vacuum_results.tables_processed.len(),
                                report.reindexed_tables.len()
                            );
                        }
                    }
                    Err(e) => {
                        error!("Database maintenance failed: {}", e);
                    }
                }
            }
        });
    }

    pub async fn get_last_health_status(&self) -> Option<DatabaseHealthStatus> {
        self.last_health_status.read().await.clone()
    }

    pub async fn get_last_performance_metrics(&self) -> Option<PerformanceMetrics> {
        self.last_performance_metrics.read().await.clone()
    }

    pub async fn get_last_integrity_report(&self) -> Option<DataIntegrityReport> {
        self.last_integrity_report.read().await.clone()
    }

    pub async fn get_last_maintenance_report(&self) -> Option<MaintenanceReport> {
        self.last_maintenance_report.read().await.clone()
    }

    pub async fn trigger_health_check(&self) -> Result<DatabaseHealthStatus, String> {
        self.database
            .health_check()
            .await
            .map_err(|e| e.to_string())
    }

    pub async fn trigger_performance_check(&self) -> Result<PerformanceMetrics, String> {
        self.database
            .get_performance_metrics()
            .await
            .map_err(|e| e.to_string())
    }

    pub async fn trigger_integrity_check(&self) -> Result<DataIntegrityReport, String> {
        self.database
            .verify_data_integrity()
            .await
            .map_err(|e| e.to_string())
    }

    pub async fn trigger_maintenance(&self) -> Result<MaintenanceReport, String> {
        let pool = self.database.pool().clone();
        let maintenance = DatabaseMaintenance::new(pool);
        maintenance
            .perform_maintenance()
            .await
            .map_err(|e| e.to_string())
    }
}

pub struct TaskMetricsCollector {
    scheduled_tasks: Arc<ScheduledTasks>,
}

impl TaskMetricsCollector {
    pub fn new(scheduled_tasks: Arc<ScheduledTasks>) -> Self {
        Self { scheduled_tasks }
    }

    pub async fn collect_all(&self) -> CollectedMetrics {
        let health = self.scheduled_tasks.get_last_health_status().await;
        let performance = self.scheduled_tasks.get_last_performance_metrics().await;
        let integrity = self.scheduled_tasks.get_last_integrity_report().await;

        CollectedMetrics {
            timestamp: Utc::now(),
            health_status: health,
            performance_metrics: performance,
            integrity_report: integrity,
        }
    }
}

#[derive(Clone, Debug)]
pub struct CollectedMetrics {
    pub timestamp: chrono::DateTime<Utc>,
    pub health_status: Option<DatabaseHealthStatus>,
    pub performance_metrics: Option<PerformanceMetrics>,
    pub integrity_report: Option<DataIntegrityReport>,
}

impl CollectedMetrics {
    pub fn to_json(&self) -> serde_json::Value {
        serde_json::json!({
            "timestamp": self.timestamp.to_rfc3339(),
            "health": self.health_status.as_ref().map(|h| serde_json::json!({
                "is_healthy": h.is_healthy,
                "connection_pool": serde_json::json!({
                    "total_connections": h.connection_pool_status.total_connections,
                    "active_connections": h.connection_pool_status.busy_connections,
                    "idle_connections": h.connection_pool_status.idle_connections,
                    "utilization_percentage": h.connection_pool_status.connection_utilization,
                }),
                "last_checked": h.last_checked.to_rfc3339(),
            })),
            "performance": self.performance_metrics.as_ref().map(|p| serde_json::json!({
                "average_query_time_ms": p.average_query_time_ms,
                "slow_queries_count": p.slow_queries_count,
                "total_queries": p.total_queries,
                "transactions_per_second": p.transactions_per_second,
                "cache_hit_ratio": p.cache_hit_ratio,
                "deadlock_count": p.deadlock_count,
            })),
            "integrity": self.integrity_report.as_ref().map(|i| serde_json::json!({
                "overall_score": i.overall_integrity_score,
                "foreign_key_violations": i.foreign_key_violations.len(),
                "orphaned_records": i.orphaned_records.iter().map(|o| o.orphan_count).sum::<i64>(),
                "duplicate_entries": i.duplicate_entries.len(),
                "check_timestamp": i.check_timestamp.to_rfc3339(),
            })),
        })
    }
}
