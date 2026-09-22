use std::sync::Arc;
use std::time::Duration;
use tokio::sync::RwLock;
use tokio::time;
use tokio_util::sync::CancellationToken;
use tracing::{error, info, warn};

use synapse_storage::maintenance::{DatabaseMaintenance, MaintenanceReport};
use synapse_storage::monitoring::{DataIntegrityReport, DatabaseHealthStatus, PerformanceMetrics};
use synapse_storage::Database;

/// Startup grace period: time to let connections warm up before the first
/// heavy metric collection run (performance / integrity / maintenance).
const STARTUP_GRACE_PERIOD: Duration = Duration::from_secs(60);

/// Maintenance tasks get a longer grace period — VACUUM ANALYZE on cold tables
/// can stall for tens of seconds, so we give it 5 minutes.
const MAINTENANCE_STARTUP_DELAY: Duration = Duration::from_secs(300);

/// Default health check interval (seconds) when not configured.
const DEFAULT_HEALTH_CHECK_INTERVAL_SECS: u64 = 10;

/// Default pool metrics update interval (seconds) when not configured.
const DEFAULT_POOL_METRICS_UPDATE_INTERVAL_SECS: u64 = 5;

/// Default performance check interval (seconds) when not configured.
const DEFAULT_PERFORMANCE_CHECK_INTERVAL_SECS: u64 = 300;

/// Default integrity check interval (seconds) when not configured.
const DEFAULT_INTEGRITY_CHECK_INTERVAL_SECS: u64 = 3600;

/// Default maintenance interval (seconds) when not configured.
const DEFAULT_MAINTENANCE_INTERVAL_SECS: u64 = 86400;

/// The `ScheduledTasks` struct.
pub struct ScheduledTasks {
    database: Arc<Database>,
    last_health_status: Arc<RwLock<Option<DatabaseHealthStatus>>>,
    last_performance_metrics: Arc<RwLock<Option<PerformanceMetrics>>>,
    last_integrity_report: Arc<RwLock<Option<DataIntegrityReport>>>,
    last_maintenance_report: Arc<RwLock<Option<MaintenanceReport>>>,
    health_check_interval: Duration,
    pool_metrics_update_interval: Duration,
    performance_check_interval: Duration,
    integrity_check_interval: Duration,
    maintenance_interval: Duration,
}

impl ScheduledTasks {
    /// Construct [] using intervals from the global config.
    ///
    /// A zero / unset value in config falls back to the historical default.
    pub fn from_config(database: Arc<Database>, server_config: &synapse_common::config::ServerConfig) -> Self {
        let health = if server_config.health_check_interval_secs > 0 {
            Duration::from_secs(server_config.health_check_interval_secs)
        } else {
            Duration::from_secs(DEFAULT_HEALTH_CHECK_INTERVAL_SECS)
        };
        let pool_metrics = if server_config.pool_metrics_update_interval_secs > 0 {
            Duration::from_secs(server_config.pool_metrics_update_interval_secs)
        } else {
            Duration::from_secs(DEFAULT_POOL_METRICS_UPDATE_INTERVAL_SECS)
        };
        let performance = if server_config.performance_check_interval_secs > 0 {
            Duration::from_secs(server_config.performance_check_interval_secs)
        } else {
            Duration::from_secs(DEFAULT_PERFORMANCE_CHECK_INTERVAL_SECS)
        };
        let integrity = if server_config.integrity_check_interval_secs > 0 {
            Duration::from_secs(server_config.integrity_check_interval_secs)
        } else {
            Duration::from_secs(DEFAULT_INTEGRITY_CHECK_INTERVAL_SECS)
        };
        let maintenance = if server_config.maintenance_interval_secs > 0 {
            Duration::from_secs(server_config.maintenance_interval_secs)
        } else {
            Duration::from_secs(DEFAULT_MAINTENANCE_INTERVAL_SECS)
        };
        Self::from_parts(database, health, pool_metrics, performance, integrity, maintenance)
    }

    fn from_parts(
        database: Arc<Database>,
        health_check_interval: Duration,
        pool_metrics_update_interval: Duration,
        performance_check_interval: Duration,
        integrity_check_interval: Duration,
        maintenance_interval: Duration,
    ) -> Self {
        Self {
            database,
            last_health_status: Arc::new(RwLock::new(None)),
            last_performance_metrics: Arc::new(RwLock::new(None)),
            last_integrity_report: Arc::new(RwLock::new(None)),
            last_maintenance_report: Arc::new(RwLock::new(None)),
            health_check_interval,
            pool_metrics_update_interval,
            performance_check_interval,
            integrity_check_interval,
            maintenance_interval,
        }
    }

    /// Spawn every background maintenance loop. Each loop terminates cleanly
    /// when `shutdown` is cancelled (SIGTERM → the server's `shutdown_token`
    /// is propagated here). Without this hook the four `tokio::spawn` tasks
    /// would continue running until the runtime is torn down — they would
    /// keep querying Postgres while the listener is closing, which surfaces
    /// as a noisy stream of "failed to perform database X" errors during
    /// graceful shutdown and delays process exit by the longest loop
    /// interval.
    pub fn start_all(&self, shutdown: CancellationToken) {
        self.start_health_check_task(shutdown.clone());
        self.start_pool_metrics_update_task(shutdown.clone());
        self.start_performance_check_task(shutdown.clone());
        self.start_integrity_check_task(shutdown.clone());
        self.start_maintenance_task(shutdown);
    }

    fn start_health_check_task(&self, shutdown: CancellationToken) {
        let interval = self.health_check_interval;
        let database = self.database.clone();
        let last_status = self.last_health_status.clone();

        tokio::spawn(async move {
            let mut interval_timer = time::interval(interval);
            interval_timer.set_missed_tick_behavior(time::MissedTickBehavior::Skip);

            loop {
                tokio::select! {
                    // Bias the select arm so a cancellation arriving exactly
                    // at tick time still breaks out instead of running one
                    // extra health check after SIGTERM.
                    biased;
                    _ = shutdown.cancelled() => {
                        info!("health check task exiting on shutdown");
                        break;
                    }
                    _ = interval_timer.tick() => {
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
                }
            }
        });
    }

    fn start_pool_metrics_update_task(&self, shutdown: CancellationToken) {
        let interval = self.pool_metrics_update_interval;
        let database = self.database.clone();

        tokio::spawn(async move {
            let mut interval_timer = time::interval(interval);
            interval_timer.set_missed_tick_behavior(time::MissedTickBehavior::Skip);

            loop {
                tokio::select! {
                    biased;
                    _ = shutdown.cancelled() => {
                        info!("pool metrics update task exiting on shutdown");
                        break;
                    }
                    _ = interval_timer.tick() => {
                        database.update_pool_metrics().await;
                    }
                }
            }
        });
    }

    fn start_performance_check_task(&self, shutdown: CancellationToken) {
        let interval = self.performance_check_interval;
        let database = self.database.clone();
        let last_metrics = self.last_performance_metrics.clone();

        tokio::spawn(async move {
            // Avoid running heavy stat queries while the server is still
            // accepting its very first requests. Race the startup sleep
            // against shutdown so a SIGTERM during the 60 s grace window
            // does not block exit.
            tokio::select! {
                biased;
                _ = shutdown.cancelled() => {
                    info!("performance check task exiting during startup grace");
                    return;
                }
                _ = time::sleep(STARTUP_GRACE_PERIOD) => {}
            }

            let mut interval_timer = time::interval(interval);
            interval_timer.set_missed_tick_behavior(time::MissedTickBehavior::Skip);

            loop {
                tokio::select! {
                    biased;
                    _ = shutdown.cancelled() => {
                        info!("performance check task exiting on shutdown");
                        break;
                    }
                    _ = interval_timer.tick() => {
                        match database.get_performance_metrics().await {
                            Ok(metrics) => {
                                *last_metrics.write().await = Some(metrics.clone());

                                if metrics.slow_queries_count > 10 {
                                    warn!("High slow query count: {} queries", metrics.slow_queries_count);
                                }

                                if metrics.average_query_time_ms > 100.0 {
                                    warn!("High average query time: {:.2}ms", metrics.average_query_time_ms);
                                }

                                info!(
                                    "Performance metrics: avg_query={:.2}ms, slow_queries={}, tps={:.2}",
                                    metrics.average_query_time_ms, metrics.slow_queries_count, metrics.transactions_per_second
                                );
                            }
                            Err(e) => {
                                error!("Failed to collect performance metrics: {}", e);
                            }
                        }
                    }
                }
            }
        });
    }

    fn start_integrity_check_task(&self, shutdown: CancellationToken) {
        let interval = self.integrity_check_interval;
        let database = self.database.clone();
        let last_report = self.last_integrity_report.clone();

        tokio::spawn(async move {
            tokio::select! {
                biased;
                _ = shutdown.cancelled() => {
                    info!("integrity check task exiting during startup grace");
                    return;
                }
                _ = time::sleep(STARTUP_GRACE_PERIOD) => {}
            }

            let mut interval_timer = time::interval(interval);
            interval_timer.set_missed_tick_behavior(time::MissedTickBehavior::Skip);

            loop {
                tokio::select! {
                    biased;
                    _ = shutdown.cancelled() => {
                        info!("integrity check task exiting on shutdown");
                        break;
                    }
                    _ = interval_timer.tick() => {
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
                                    warn!("Data integrity score below optimal: {:.1}", report.overall_integrity_score);
                                }

                                info!(
                                    "Data integrity check: score={:.1}, violations={}, orphaned={}",
                                    report.overall_integrity_score,
                                    report.foreign_key_violations.len() + report.orphaned_records.len(),
                                    report.orphaned_records.iter().map(|o| o.orphan_count).sum::<i64>()
                                );
                            }
                            Err(e) => {
                                error!("Failed to verify data integrity: {}", e);
                            }
                        }
                    }
                }
            }
        });
    }

    fn start_maintenance_task(&self, shutdown: CancellationToken) {
        let interval = self.maintenance_interval;
        let pool = self.database.pool().clone();
        let last_report = self.last_maintenance_report.clone();

        tokio::spawn(async move {
            // VACUUM ANALYZE on cold tables can stall for tens of seconds.
            // Defer the first run to give startup traffic a smooth ramp;
            // PostgreSQL's autovacuum is sufficient for this window. The
            // startup sleep races against shutdown so a SIGTERM during the
            // 5 min grace window exits cleanly without firing VACUUM.
            tokio::select! {
                biased;
                _ = shutdown.cancelled() => {
                    info!("maintenance task exiting during startup delay");
                    return;
                }
                _ = time::sleep(MAINTENANCE_STARTUP_DELAY) => {}
            }

            let mut interval_timer = time::interval(interval);
            interval_timer.set_missed_tick_behavior(time::MissedTickBehavior::Skip);

            loop {
                tokio::select! {
                    biased;
                    _ = shutdown.cancelled() => {
                        info!("maintenance task exiting on shutdown");
                        break;
                    }
                    _ = interval_timer.tick() => {
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
                }
            }
        });
    }

    /// Return the last recorded health status.
    /// Return the last recorded health status.
    pub async fn get_last_health_status(&self) -> Option<DatabaseHealthStatus> {
        self.last_health_status.read().await.clone()
    }

    /// Return the last performance metrics snapshot.
    /// Return the last performance metrics snapshot.
    pub async fn get_last_performance_metrics(&self) -> Option<PerformanceMetrics> {
        self.last_performance_metrics.read().await.clone()
    }

    /// Return the last integrity report.
    /// Return the last integrity report.
    pub async fn get_last_integrity_report(&self) -> Option<DataIntegrityReport> {
        self.last_integrity_report.read().await.clone()
    }

    /// Return the last maintenance report.
    /// Return the last maintenance report.
    pub async fn get_last_maintenance_report(&self) -> Option<MaintenanceReport> {
        self.last_maintenance_report.read().await.clone()
    }

    /// Trigger a manual health check.
    /// Trigger a manual health check.
    pub async fn trigger_health_check(&self) -> Result<DatabaseHealthStatus, String> {
        self.database.health_check().await.map_err(|e| e.to_string())
    }

    /// Trigger a manual performance check.
    /// Trigger a manual performance check.
    pub async fn trigger_performance_check(&self) -> Result<PerformanceMetrics, String> {
        self.database.get_performance_metrics().await.map_err(|e| e.to_string())
    }

    /// Trigger a manual integrity check.
    /// Trigger a manual integrity check.
    pub async fn trigger_integrity_check(&self) -> Result<DataIntegrityReport, String> {
        self.database.verify_data_integrity().await.map_err(|e| e.to_string())
    }

    /// Trigger a manual maintenance check.
    /// Trigger a manual maintenance check.
    pub async fn trigger_maintenance(&self) -> Result<MaintenanceReport, String> {
        let pool = self.database.pool().clone();
        let maintenance = DatabaseMaintenance::new(pool);
        maintenance.perform_maintenance().await.map_err(|e| e.to_string())
    }
}
