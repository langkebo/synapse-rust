use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::time::Duration;
use synapse_common::current_timestamp_millis;
use tokio::sync::{mpsc, RwLock};
use tracing::{debug, info, warn};

type HealthCallback = Box<dyn Fn(&str, HealthStatus) + Send + Sync>;

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum HealthStatus {
    Healthy,
    Unhealthy,
    Degraded,
    #[default]
    Unknown,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HealthCheckResult {
    pub worker_id: String,
    pub status: HealthStatus,
    pub latency_ms: u64,
    pub last_check_ts: i64,
    pub consecutive_failures: u32,
    pub error_message: Option<String>,
}

impl Default for HealthCheckResult {
    fn default() -> Self {
        Self {
            worker_id: String::new(),
            status: HealthStatus::Unknown,
            latency_ms: 0,
            last_check_ts: 0,
            consecutive_failures: 0,
            error_message: None,
        }
    }
}

#[derive(Debug, Clone)]
pub struct HealthCheckConfig {
    pub check_interval_secs: u64,
    pub timeout_secs: u64,
    pub max_consecutive_failures: u32,
    pub recovery_threshold: u32,
    pub degraded_latency_ms: u64,
    /// WORK-04: 心跳超时（秒）。超过该时长未收到心跳的 worker 判定为
    /// 探测失败——崩溃的 worker 不再「注册即健康」。
    pub heartbeat_timeout_secs: u64,
}

impl Default for HealthCheckConfig {
    fn default() -> Self {
        Self {
            check_interval_secs: 30,
            timeout_secs: 10,
            max_consecutive_failures: 3,
            recovery_threshold: 2,
            degraded_latency_ms: 1000,
            heartbeat_timeout_secs: 90,
        }
    }
}

pub struct HealthChecker {
    config: HealthCheckConfig,
    health_status: RwLock<HashMap<String, HealthCheckResult>>,
    callbacks: RwLock<Vec<HealthCallback>>,
    last_heartbeat: RwLock<HashMap<String, i64>>,
}

impl HealthChecker {
    pub fn new(config: HealthCheckConfig) -> Self {
        Self {
            config,
            health_status: RwLock::new(HashMap::new()),
            callbacks: RwLock::new(Vec::new()),
            last_heartbeat: RwLock::new(HashMap::new()),
        }
    }

    pub async fn register_worker(&self, worker_id: &str) {
        let mut status = self.health_status.write().await;
        status.entry(worker_id.to_string()).or_insert_with(|| HealthCheckResult {
            worker_id: worker_id.to_string(),
            status: HealthStatus::Unknown,
            last_check_ts: current_timestamp_millis(),
            ..Default::default()
        });
        drop(status);

        // 注册即有一次心跳（注册动作本身由心跳路径触发）
        self.record_heartbeat(worker_id).await;

        debug!("Worker registered for health checks: {}", worker_id);
    }

    /// WORK-04: 记录 worker 心跳，供健康检查做活性探测。
    pub async fn record_heartbeat(&self, worker_id: &str) {
        self.last_heartbeat.write().await.insert(worker_id.to_string(), current_timestamp_millis());
    }

    pub async fn unregister_worker(&self, worker_id: &str) {
        let mut status = self.health_status.write().await;
        status.remove(worker_id);
        drop(status);
        self.last_heartbeat.write().await.remove(worker_id);

        debug!("Worker unregistered from health checks: {}", worker_id);
    }

    pub async fn check_health(&self, worker_id: &str) -> HealthCheckResult {
        let start = std::time::Instant::now();

        let result = self.perform_health_check(worker_id).await;

        let latency_ms = start.elapsed().as_millis() as u64;

        let health_result = self.update_health_status(worker_id, result, latency_ms).await;

        self.notify_callbacks(&health_result).await;

        health_result
    }

    async fn perform_health_check(&self, worker_id: &str) -> Result<(), String> {
        {
            let status = self.health_status.read().await;
            if !status.contains_key(worker_id) {
                return Err("Worker not registered".to_string());
            }
        }

        // WORK-04: 真实活性探测——崩溃的 worker 停止心跳，超过
        // heartbeat_timeout 即探测失败；不再「注册表有键就 Healthy」。
        let last_beat = self.last_heartbeat.read().await.get(worker_id).copied();
        match last_beat {
            Some(ts) => {
                let elapsed_ms = current_timestamp_millis() - ts;
                if elapsed_ms > self.config.heartbeat_timeout_secs as i64 * 1000 {
                    return Err(format!("Heartbeat stale: last seen {elapsed_ms}ms ago"));
                }
                Ok(())
            }
            None => Err("No heartbeat recorded".to_string()),
        }
    }

    async fn update_health_status(
        &self,
        worker_id: &str,
        check_result: Result<(), String>,
        latency_ms: u64,
    ) -> HealthCheckResult {
        let mut status = self.health_status.write().await;

        let current = status.entry(worker_id.to_string()).or_default();
        current.worker_id = worker_id.to_string();
        current.latency_ms = latency_ms;
        current.last_check_ts = current_timestamp_millis();

        match check_result {
            Ok(()) => {
                current.error_message = None;

                if current.consecutive_failures > 0 {
                    current.consecutive_failures = current.consecutive_failures.saturating_sub(1);
                }

                if latency_ms > self.config.degraded_latency_ms {
                    current.status = HealthStatus::Degraded;
                } else if current.consecutive_failures == 0 {
                    current.status = HealthStatus::Healthy;
                } else if current.consecutive_failures >= self.config.recovery_threshold {
                    current.status = HealthStatus::Degraded;
                } else {
                    current.status = HealthStatus::Healthy;
                }
            }
            Err(e) => {
                current.consecutive_failures += 1;
                current.error_message = Some(e.clone());

                if current.consecutive_failures >= self.config.max_consecutive_failures {
                    current.status = HealthStatus::Unhealthy;
                    warn!(
                        worker_id = %worker_id,
                        error = %e,
                        consecutive_failures = current.consecutive_failures,
                        max_consecutive_failures = self.config.max_consecutive_failures,
                        "Worker marked as unhealthy"
                    );
                } else {
                    current.status = HealthStatus::Degraded;
                }
            }
        }

        current.clone()
    }

    async fn notify_callbacks(&self, result: &HealthCheckResult) {
        let callbacks = self.callbacks.read().await;
        for callback in callbacks.iter() {
            callback(&result.worker_id, result.status);
        }
    }

    pub async fn get_health(&self, worker_id: &str) -> Option<HealthCheckResult> {
        let status = self.health_status.read().await;
        status.get(worker_id).cloned()
    }

    pub async fn get_all_health(&self) -> HashMap<String, HealthCheckResult> {
        let status = self.health_status.read().await;
        status.clone()
    }

    pub async fn get_healthy_workers(&self) -> Vec<String> {
        let status = self.health_status.read().await;
        status.iter().filter(|(_, r)| r.status == HealthStatus::Healthy).map(|(id, _)| id.clone()).collect()
    }

    pub async fn get_unhealthy_workers(&self) -> Vec<String> {
        let status = self.health_status.read().await;
        status.iter().filter(|(_, r)| r.status == HealthStatus::Unhealthy).map(|(id, _)| id.clone()).collect()
    }

    pub async fn is_healthy(&self, worker_id: &str) -> bool {
        let status = self.health_status.read().await;
        status.get(worker_id).is_some_and(|r| r.status == HealthStatus::Healthy || r.status == HealthStatus::Degraded)
    }

    pub fn register_callback(&self, callback: HealthCallback) {
        let mut callbacks = self.callbacks.blocking_write();
        callbacks.push(callback);
    }

    pub async fn start_periodic_checks(&self, mut shutdown_rx: mpsc::Receiver<()>) {
        let config = self.config.clone();
        let interval = Duration::from_secs(config.check_interval_secs);

        info!(
            check_interval_secs = config.check_interval_secs,
            timeout_secs = config.timeout_secs,
            max_consecutive_failures = config.max_consecutive_failures,
            recovery_threshold = config.recovery_threshold,
            degraded_latency_ms = config.degraded_latency_ms,
            "Starting periodic health checks"
        );

        loop {
            tokio::select! {
                _ = shutdown_rx.recv() => {
                    info!(check_interval_secs = config.check_interval_secs, "Health check task shutting down");
                    break;
                }
                _ = tokio::time::sleep(interval) => {
                    let workers: Vec<String> = {
                        let status = self.health_status.read().await;
                        status.keys().cloned().collect()
                    };

                    // Parallelize health checks across all registered workers since
                    // each check_health call is independent (different worker_id) and
                    // mutates state through internal RwLocks.
                    let check_futures =
                        workers.iter().map(|worker_id| self.check_health(worker_id));
                    futures::future::join_all(check_futures).await;

                    debug!("Completed health check cycle for {} workers",
                           self.health_status.read().await.len());
                }
            }
        }
    }

    pub async fn get_stats(&self) -> HealthCheckStats {
        let status = self.health_status.read().await;

        let total_workers = status.len() as u32;
        let mut healthy_count = 0u32;
        let mut unhealthy_count = 0u32;
        let mut degraded_count = 0u32;
        let mut unknown_count = 0u32;
        let mut total_latency_ms = 0u64;

        for result in status.values() {
            match result.status {
                HealthStatus::Healthy => healthy_count += 1,
                HealthStatus::Unhealthy => unhealthy_count += 1,
                HealthStatus::Degraded => degraded_count += 1,
                HealthStatus::Unknown => unknown_count += 1,
            }
            total_latency_ms += result.latency_ms;
        }

        let avg_latency_ms = if total_workers > 0 { total_latency_ms / total_workers as u64 } else { 0 };

        HealthCheckStats {
            total_workers,
            healthy_count,
            unhealthy_count,
            degraded_count,
            unknown_count,
            total_latency_ms,
            avg_latency_ms,
        }
    }
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct HealthCheckStats {
    pub total_workers: u32,
    pub healthy_count: u32,
    pub unhealthy_count: u32,
    pub degraded_count: u32,
    pub unknown_count: u32,
    pub total_latency_ms: u64,
    pub avg_latency_ms: u64,
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;

    #[test]
    fn test_health_check_config_default() {
        let config = HealthCheckConfig::default();
        assert_eq!(config.check_interval_secs, 30);
        assert_eq!(config.max_consecutive_failures, 3);
    }

    #[tokio::test]
    async fn test_register_worker() {
        let checker = HealthChecker::new(HealthCheckConfig::default());

        checker.register_worker("worker1").await;

        let health = checker.get_health("worker1").await;
        assert!(health.is_some());
        assert_eq!(health.unwrap().status, HealthStatus::Unknown);
    }

    #[tokio::test]
    async fn test_unregister_worker() {
        let checker = HealthChecker::new(HealthCheckConfig::default());

        checker.register_worker("worker1").await;
        assert!(checker.get_health("worker1").await.is_some());

        checker.unregister_worker("worker1").await;
        assert!(checker.get_health("worker1").await.is_none());
    }

    #[tokio::test]
    async fn test_check_health() {
        let checker = HealthChecker::new(HealthCheckConfig::default());

        checker.register_worker("worker1").await;

        let result = checker.check_health("worker1").await;

        assert!(result.status == HealthStatus::Healthy || result.status == HealthStatus::Degraded);
    }

    #[tokio::test]
    async fn test_get_healthy_workers() {
        let checker = HealthChecker::new(HealthCheckConfig::default());

        checker.register_worker("worker1").await;
        checker.register_worker("worker2").await;

        checker.check_health("worker1").await;
        checker.check_health("worker2").await;

        let healthy = checker.get_healthy_workers().await;
        assert!(!healthy.is_empty());
    }

    #[tokio::test]
    async fn test_health_check_stats() {
        let checker = HealthChecker::new(HealthCheckConfig::default());

        checker.register_worker("worker1").await;
        checker.register_worker("worker2").await;

        checker.check_health("worker1").await;
        checker.check_health("worker2").await;

        let stats = checker.get_stats().await;
        assert_eq!(stats.total_workers, 2);
    }

    #[tokio::test]
    async fn test_health_checker_marks_worker_unhealthy_after_consecutive_failures() {
        let checker = HealthChecker::new(HealthCheckConfig {
            max_consecutive_failures: 3,
            recovery_threshold: 2,
            ..HealthCheckConfig::default()
        });

        checker.register_worker("worker1").await;

        checker.update_health_status("worker1", Err("timeout".to_string()), 10).await;
        checker.update_health_status("worker1", Err("timeout".to_string()), 10).await;
        let result = checker.update_health_status("worker1", Err("timeout".to_string()), 10).await;

        assert_eq!(result.status, HealthStatus::Unhealthy);
        assert_eq!(result.consecutive_failures, 3);
        assert_eq!(result.error_message.as_deref(), Some("timeout"));
        assert_eq!(checker.get_unhealthy_workers().await, vec!["worker1".to_string()]);
    }

    #[tokio::test]
    async fn test_health_checker_recovers_from_unhealthy_via_degraded_before_healthy() {
        let checker = HealthChecker::new(HealthCheckConfig {
            max_consecutive_failures: 3,
            recovery_threshold: 2,
            degraded_latency_ms: 1000,
            ..HealthCheckConfig::default()
        });

        checker.register_worker("worker1").await;

        checker.update_health_status("worker1", Err("timeout".to_string()), 10).await;
        checker.update_health_status("worker1", Err("timeout".to_string()), 10).await;
        let unhealthy = checker.update_health_status("worker1", Err("timeout".to_string()), 10).await;
        assert_eq!(unhealthy.status, HealthStatus::Unhealthy);
        assert_eq!(unhealthy.consecutive_failures, 3);

        let recovering = checker.update_health_status("worker1", Ok(()), 10).await;
        assert_eq!(recovering.status, HealthStatus::Degraded);
        assert_eq!(recovering.consecutive_failures, 2);

        let healthy = checker.update_health_status("worker1", Ok(()), 10).await;
        assert_eq!(healthy.status, HealthStatus::Healthy);
        assert_eq!(healthy.consecutive_failures, 1);
        assert!(checker.is_healthy("worker1").await);
    }

    // WORK-04: 心跳停滞的 worker（如进程崩溃）必须被探测为失败，
    // 连续失败达到阈值后标记 Unhealthy——不再「注册表有键即健康」。
    #[tokio::test]
    async fn work04_stale_heartbeat_marks_worker_unhealthy() {
        let checker = HealthChecker::new(HealthCheckConfig {
            max_consecutive_failures: 2,
            heartbeat_timeout_secs: 60,
            ..HealthCheckConfig::default()
        });
        checker.register_worker("worker1").await;

        // 模拟崩溃：最后一次心跳在 1 小时前
        let stale_ts = current_timestamp_millis() - 3_600_000;
        checker.last_heartbeat.write().await.insert("worker1".to_string(), stale_ts);

        checker.check_health("worker1").await;
        let result = checker.check_health("worker1").await;

        assert_eq!(result.status, HealthStatus::Unhealthy);
        assert!(result.error_message.as_deref().unwrap_or("").contains("Heartbeat stale"));
    }

    // WORK-04: 补跳后从停滞恢复
    #[tokio::test]
    async fn work04_record_heartbeat_restores_health() {
        let checker = HealthChecker::new(HealthCheckConfig {
            max_consecutive_failures: 2,
            heartbeat_timeout_secs: 60,
            ..HealthCheckConfig::default()
        });
        checker.register_worker("worker1").await;

        let stale_ts = current_timestamp_millis() - 3_600_000;
        checker.last_heartbeat.write().await.insert("worker1".to_string(), stale_ts);
        checker.check_health("worker1").await;
        checker.check_health("worker1").await;
        assert_eq!(checker.get_health("worker1").await.unwrap().status, HealthStatus::Unhealthy);

        // worker 恢复心跳
        checker.record_heartbeat("worker1").await;
        checker.check_health("worker1").await;
        checker.check_health("worker1").await;
        let result = checker.check_health("worker1").await;
        assert!(result.status == HealthStatus::Healthy || result.status == HealthStatus::Degraded);
    }

    #[test]
    fn test_callback() {
        let checker = HealthChecker::new(HealthCheckConfig::default());

        let callback_called = Arc::new(std::sync::atomic::AtomicBool::new(false));
        let callback_called_clone = callback_called.clone();

        checker.register_callback(Box::new(move |_worker_id, _status| {
            callback_called_clone.store(true, std::sync::atomic::Ordering::SeqCst);
        }));

        let rt = tokio::runtime::Runtime::new().unwrap();
        rt.block_on(async {
            checker.register_worker("worker1").await;
            checker.check_health("worker1").await;
        });

        assert!(callback_called.load(std::sync::atomic::Ordering::SeqCst));
    }
}
