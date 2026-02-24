use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::time::Duration;
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
}

impl Default for HealthCheckConfig {
    fn default() -> Self {
        Self {
            check_interval_secs: 30,
            timeout_secs: 10,
            max_consecutive_failures: 3,
            recovery_threshold: 2,
            degraded_latency_ms: 1000,
        }
    }
}

pub struct HealthChecker {
    config: HealthCheckConfig,
    health_status: RwLock<HashMap<String, HealthCheckResult>>,
    callbacks: RwLock<Vec<HealthCallback>>,
}

impl HealthChecker {
    pub fn new(config: HealthCheckConfig) -> Self {
        Self {
            config,
            health_status: RwLock::new(HashMap::new()),
            callbacks: RwLock::new(Vec::new()),
        }
    }

    pub async fn register_worker(&self, worker_id: &str) {
        let mut status = self.health_status.write().await;
        status.insert(
            worker_id.to_string(),
            HealthCheckResult {
                worker_id: worker_id.to_string(),
                status: HealthStatus::Unknown,
                last_check_ts: chrono::Utc::now().timestamp_millis(),
                ..Default::default()
            },
        );

        debug!("Worker registered for health checks: {}", worker_id);
    }

    pub async fn unregister_worker(&self, worker_id: &str) {
        let mut status = self.health_status.write().await;
        status.remove(worker_id);

        debug!("Worker unregistered from health checks: {}", worker_id);
    }

    pub async fn check_health(&self, worker_id: &str) -> HealthCheckResult {
        let start = std::time::Instant::now();

        let result = self.perform_health_check(worker_id).await;

        let latency_ms = start.elapsed().as_millis() as u64;

        let health_result = self
            .update_health_status(worker_id, result, latency_ms)
            .await;

        self.notify_callbacks(&health_result).await;

        health_result
    }

    async fn perform_health_check(&self, worker_id: &str) -> Result<(), String> {
        let status = self.health_status.read().await;
        if !status.contains_key(worker_id) {
            return Err("Worker not registered".to_string());
        }
        drop(status);

        Ok(())
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
        current.last_check_ts = chrono::Utc::now().timestamp_millis();

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
                }
            }
            Err(e) => {
                current.consecutive_failures += 1;
                current.error_message = Some(e.clone());

                if current.consecutive_failures >= self.config.max_consecutive_failures {
                    current.status = HealthStatus::Unhealthy;
                    warn!("Worker {} marked as unhealthy: {}", worker_id, e);
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
        status
            .iter()
            .filter(|(_, r)| r.status == HealthStatus::Healthy)
            .map(|(id, _)| id.clone())
            .collect()
    }

    pub async fn get_unhealthy_workers(&self) -> Vec<String> {
        let status = self.health_status.read().await;
        status
            .iter()
            .filter(|(_, r)| r.status == HealthStatus::Unhealthy)
            .map(|(id, _)| id.clone())
            .collect()
    }

    pub async fn is_healthy(&self, worker_id: &str) -> bool {
        let status = self.health_status.read().await;
        status
            .get(worker_id)
            .map(|r| r.status == HealthStatus::Healthy || r.status == HealthStatus::Degraded)
            .unwrap_or(false)
    }

    pub fn register_callback(&self, callback: HealthCallback) {
        let mut callbacks = self.callbacks.blocking_write();
        callbacks.push(callback);
    }

    pub async fn start_periodic_checks(&self, mut shutdown_rx: mpsc::Receiver<()>) {
        let config = self.config.clone();
        let interval = Duration::from_secs(config.check_interval_secs);

        info!("Starting periodic health checks (interval: {:?})", interval);

        loop {
            tokio::select! {
                _ = shutdown_rx.recv() => {
                    info!("Health check task shutting down");
                    break;
                }
                _ = tokio::time::sleep(interval) => {
                    let workers: Vec<String> = {
                        let status = self.health_status.read().await;
                        status.keys().cloned().collect()
                    };

                    for worker_id in workers {
                        let _ = self.check_health(&worker_id).await;
                    }

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

        let avg_latency_ms = if total_workers > 0 {
            total_latency_ms / total_workers as u64
        } else {
            0
        };

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
