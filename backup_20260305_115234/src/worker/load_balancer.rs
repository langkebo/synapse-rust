use crate::worker::types::WorkerInfo;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use tokio::sync::RwLock;
use tracing::{debug, info, warn};

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum LoadBalanceStrategy {
    #[default]
    RoundRobin,
    LeastConnections,
    WeightedRoundRobin,
    Random,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WorkerLoadStats {
    pub worker_id: String,
    pub active_connections: u32,
    pub pending_tasks: u32,
    pub cpu_usage: f32,
    pub memory_usage: f32,
    pub last_update_ts: i64,
}

impl Default for WorkerLoadStats {
    fn default() -> Self {
        Self {
            worker_id: String::new(),
            active_connections: 0,
            pending_tasks: 0,
            cpu_usage: 0.0,
            memory_usage: 0.0,
            last_update_ts: 0,
        }
    }
}

#[derive(Debug, Clone)]
struct WorkerState {
    info: WorkerInfo,
    load_stats: WorkerLoadStats,
    weight: u32,
    request_count: u64,
}

pub struct WorkerLoadBalancer {
    workers: RwLock<HashMap<String, WorkerState>>,
    strategy: LoadBalanceStrategy,
    round_robin_index: RwLock<usize>,
}

impl WorkerLoadBalancer {
    pub fn new(strategy: LoadBalanceStrategy) -> Self {
        Self {
            workers: RwLock::new(HashMap::new()),
            strategy,
            round_robin_index: RwLock::new(0),
        }
    }

    pub async fn register_worker(&self, worker: WorkerInfo) {
        let worker_id = worker.worker_id.clone();
        let mut workers = self.workers.write().await;

        let weight = match worker.worker_type.as_str() {
            "master" => 100,
            "frontend" => 80,
            "federation_sender" => 60,
            "event_persister" => 70,
            "pusher" => 50,
            "media_repository" => 40,
            _ => 30,
        };

        workers.insert(
            worker.worker_id.clone(),
            WorkerState {
                info: worker,
                load_stats: WorkerLoadStats::default(),
                weight,
                request_count: 0,
            },
        );

        info!("Worker registered: {} (weight: {})", worker_id, weight);
    }

    pub async fn unregister_worker(&self, worker_id: &str) {
        let mut workers = self.workers.write().await;
        workers.remove(worker_id);

        info!("Worker unregistered: {}", worker_id);
    }

    pub async fn update_worker_load(&self, worker_id: &str, stats: WorkerLoadStats) {
        let mut workers = self.workers.write().await;

        if let Some(state) = workers.get_mut(worker_id) {
            state.load_stats = stats;
            debug!("Updated load stats for worker: {}", worker_id);
        }
    }

    pub async fn select_worker(&self, task_type: &str) -> Option<String> {
        let workers = self.workers.read().await;

        let candidates: Vec<&WorkerState> = workers
            .values()
            .filter(|w| self.can_handle_task(&w.info, task_type))
            .filter(|w| w.info.status == "running")
            .collect();

        if candidates.is_empty() {
            warn!("No available workers for task type: {}", task_type);
            return None;
        }

        let selected = match self.strategy {
            LoadBalanceStrategy::RoundRobin => self.select_round_robin(&candidates).await,
            LoadBalanceStrategy::LeastConnections => self.select_least_connections(&candidates),
            LoadBalanceStrategy::WeightedRoundRobin => {
                self.select_weighted_round_robin(&candidates).await
            }
            LoadBalanceStrategy::Random => self.select_random(&candidates),
        };

        drop(workers);

        if let Some(worker_id) = &selected {
            let mut workers = self.workers.write().await;
            if let Some(state) = workers.get_mut(worker_id) {
                state.request_count += 1;
            }
        }

        selected
    }

    fn can_handle_task(&self, worker: &WorkerInfo, task_type: &str) -> bool {
        match worker.worker_type.as_str() {
            "master" => true,
            "frontend" => matches!(task_type, "http" | "sync" | "presence"),
            "federation_sender" => matches!(task_type, "federation" | "federation_send"),
            "event_persister" => matches!(task_type, "event_persist" | "events"),
            "pusher" => matches!(task_type, "push" | "push_notifications"),
            "media_repository" => matches!(task_type, "media" | "media_upload" | "media_download"),
            _ => false,
        }
    }

    async fn select_round_robin(&self, candidates: &[&WorkerState]) -> Option<String> {
        if candidates.is_empty() {
            return None;
        }

        let mut index = self.round_robin_index.write().await;
        *index = (*index + 1) % candidates.len();

        Some(candidates[*index].info.worker_id.clone())
    }

    fn select_least_connections(&self, candidates: &[&WorkerState]) -> Option<String> {
        candidates
            .iter()
            .min_by(|a, b| {
                let a_load = a.load_stats.active_connections + a.load_stats.pending_tasks;
                let b_load = b.load_stats.active_connections + b.load_stats.pending_tasks;
                a_load.cmp(&b_load)
            })
            .map(|w| w.info.worker_id.clone())
    }

    async fn select_weighted_round_robin(&self, candidates: &[&WorkerState]) -> Option<String> {
        let total_weight: u32 = candidates.iter().map(|w| w.weight).sum();

        if total_weight == 0 {
            return self.select_round_robin(candidates).await;
        }

        let mut index = self.round_robin_index.write().await;
        let mut cumulative = 0u32;
        let target = (*index as u32 % total_weight) + 1;

        for candidate in candidates {
            cumulative += candidate.weight;
            if cumulative >= target {
                *index = (*index + 1) % candidates.len();
                return Some(candidate.info.worker_id.clone());
            }
        }

        candidates.first().map(|w| w.info.worker_id.clone())
    }

    fn select_random(&self, candidates: &[&WorkerState]) -> Option<String> {
        if candidates.is_empty() {
            return None;
        }

        let index = rand::random::<usize>() % candidates.len();
        Some(candidates[index].info.worker_id.clone())
    }

    pub async fn get_worker_count(&self) -> usize {
        let workers = self.workers.read().await;
        workers.len()
    }

    pub async fn get_active_worker_count(&self) -> usize {
        let workers = self.workers.read().await;
        workers
            .values()
            .filter(|w| w.info.status == "running")
            .count()
    }

    pub async fn get_worker_stats(&self, worker_id: &str) -> Option<WorkerLoadStats> {
        let workers = self.workers.read().await;
        workers.get(worker_id).map(|w| w.load_stats.clone())
    }

    pub async fn get_all_stats(&self) -> HashMap<String, WorkerLoadStats> {
        let workers = self.workers.read().await;
        workers
            .iter()
            .map(|(id, state)| (id.clone(), state.load_stats.clone()))
            .collect()
    }

    pub async fn get_total_capacity(&self) -> u32 {
        let workers = self.workers.read().await;
        workers
            .values()
            .filter(|w| w.info.status == "running")
            .map(|w| w.weight)
            .sum()
    }

    pub fn set_strategy(&mut self, strategy: LoadBalanceStrategy) {
        self.strategy = strategy;
        info!("Load balance strategy changed to: {:?}", strategy);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn create_test_worker(id: &str, worker_type: &str) -> WorkerInfo {
        let now = chrono::Utc::now().timestamp_millis();
        WorkerInfo {
            id: 0,
            worker_id: id.to_string(),
            worker_name: format!("{}-name", id),
            worker_type: worker_type.to_string(),
            status: "running".to_string(),
            host: "localhost".to_string(),
            port: 8080,
            last_heartbeat_ts: Some(now),
            started_ts: now,
            stopped_ts: None,
            config: serde_json::json!({}),
            metadata: serde_json::json!({}),
            version: Some("1.0.0".to_string()),
        }
    }

    #[tokio::test]
    async fn test_register_worker() {
        let balancer = WorkerLoadBalancer::new(LoadBalanceStrategy::RoundRobin);
        let worker = create_test_worker("worker1", "frontend");

        balancer.register_worker(worker).await;

        assert_eq!(balancer.get_worker_count().await, 1);
    }

    #[tokio::test]
    async fn test_unregister_worker() {
        let balancer = WorkerLoadBalancer::new(LoadBalanceStrategy::RoundRobin);
        let worker = create_test_worker("worker1", "frontend");

        balancer.register_worker(worker).await;
        assert_eq!(balancer.get_worker_count().await, 1);

        balancer.unregister_worker("worker1").await;
        assert_eq!(balancer.get_worker_count().await, 0);
    }

    #[tokio::test]
    async fn test_select_worker_round_robin() {
        let balancer = WorkerLoadBalancer::new(LoadBalanceStrategy::RoundRobin);

        balancer
            .register_worker(create_test_worker("worker1", "frontend"))
            .await;
        balancer
            .register_worker(create_test_worker("worker2", "frontend"))
            .await;

        let selected1 = balancer.select_worker("http").await;
        let selected2 = balancer.select_worker("http").await;

        assert!(selected1.is_some());
        assert!(selected2.is_some());
        assert_ne!(selected1, selected2);
    }

    #[tokio::test]
    async fn test_select_worker_least_connections() {
        let balancer = WorkerLoadBalancer::new(LoadBalanceStrategy::LeastConnections);

        balancer
            .register_worker(create_test_worker("worker1", "frontend"))
            .await;
        balancer
            .register_worker(create_test_worker("worker2", "frontend"))
            .await;

        balancer
            .update_worker_load(
                "worker1",
                WorkerLoadStats {
                    worker_id: "worker1".to_string(),
                    active_connections: 10,
                    pending_tasks: 5,
                    ..Default::default()
                },
            )
            .await;

        balancer
            .update_worker_load(
                "worker2",
                WorkerLoadStats {
                    worker_id: "worker2".to_string(),
                    active_connections: 2,
                    pending_tasks: 1,
                    ..Default::default()
                },
            )
            .await;

        let selected = balancer.select_worker("http").await;
        assert_eq!(selected, Some("worker2".to_string()));
    }

    #[tokio::test]
    async fn test_select_worker_by_task_type() {
        let balancer = WorkerLoadBalancer::new(LoadBalanceStrategy::RoundRobin);

        balancer
            .register_worker(create_test_worker("frontend1", "frontend"))
            .await;
        balancer
            .register_worker(create_test_worker("pusher1", "pusher"))
            .await;

        let http_worker = balancer.select_worker("http").await;
        assert_eq!(http_worker, Some("frontend1".to_string()));

        let push_worker = balancer.select_worker("push").await;
        assert_eq!(push_worker, Some("pusher1".to_string()));
    }

    #[tokio::test]
    async fn test_no_available_workers() {
        let balancer = WorkerLoadBalancer::new(LoadBalanceStrategy::RoundRobin);

        balancer
            .register_worker(create_test_worker("pusher1", "pusher"))
            .await;

        let selected = balancer.select_worker("http").await;
        assert!(selected.is_none());
    }

    #[tokio::test]
    async fn test_get_total_capacity() {
        let balancer = WorkerLoadBalancer::new(LoadBalanceStrategy::RoundRobin);

        balancer
            .register_worker(create_test_worker("master1", "master"))
            .await;
        balancer
            .register_worker(create_test_worker("frontend1", "frontend"))
            .await;

        let capacity = balancer.get_total_capacity().await;
        assert_eq!(capacity, 180);
    }
}
