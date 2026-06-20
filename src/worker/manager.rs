use crate::common::ApiError;
use crate::worker::bus::{RedisBusConfig, WorkerBus};
use crate::worker::health::{HealthCheckConfig, HealthChecker};
use crate::worker::load_balancer::{LoadBalanceStrategy, WorkerLoadBalancer};
use crate::worker::protocol::ReplicationCommand;
use crate::worker::storage::WorkerStorage;
use crate::worker::stream::StreamWriterManager;
use crate::worker::tcp::ReplicationConnection;
use crate::worker::types::*;
use std::collections::{HashMap, HashSet};
use std::str::FromStr;
use std::sync::Arc;
use tokio::sync::RwLock;
use tracing::{debug, info, instrument, warn};

pub struct WorkerManager {
    storage: Arc<WorkerStorage>,
    server_name: String,
    local_worker_id: Option<String>,
    connections: Arc<RwLock<HashMap<String, ReplicationConnection>>>,
    bus: Option<Arc<WorkerBus>>,
    stream_manager: Option<Arc<StreamWriterManager>>,
    load_balancer: Option<Arc<WorkerLoadBalancer>>,
    health_checker: Option<Arc<HealthChecker>>,
}

impl WorkerManager {
    fn select_most_recent_worker<'a>(candidates: &[&'a WorkerInfo]) -> Option<&'a WorkerInfo> {
        candidates.iter().copied().max_by(|a, b| {
            let a_heartbeat = a.last_heartbeat_ts.unwrap_or(0);
            let b_heartbeat = b.last_heartbeat_ts.unwrap_or(0);
            a_heartbeat.cmp(&b_heartbeat).then_with(|| b.started_ts.cmp(&a.started_ts))
        })
    }

    fn select_fallback_candidate<'a>(
        candidates: &[&'a WorkerInfo],
        healthy_worker_ids: Option<&HashSet<String>>,
    ) -> Option<&'a WorkerInfo> {
        if let Some(healthy_worker_ids) = healthy_worker_ids {
            let healthy_candidates: Vec<&WorkerInfo> =
                candidates.iter().copied().filter(|worker| healthy_worker_ids.contains(&worker.worker_id)).collect();

            if !healthy_candidates.is_empty() {
                return Self::select_most_recent_worker(&healthy_candidates);
            }
        }

        Self::select_most_recent_worker(candidates)
    }

    fn worker_supports_task_type(worker_type: WorkerType, task_type: &str) -> bool {
        match worker_type {
            WorkerType::Master => true,
            WorkerType::Frontend => matches!(task_type, "http" | "presence"),
            WorkerType::Synchrotron => matches!(task_type, "sync"),
            WorkerType::FederationSender => matches!(task_type, "federation" | "federation_send"),
            WorkerType::FederationReader => matches!(task_type, "federation_read" | "federation_ingress"),
            WorkerType::EventPersister => matches!(task_type, "event_persist" | "events" | "event_processing"),
            WorkerType::Pusher => matches!(task_type, "push" | "push_notifications"),
            WorkerType::MediaRepository => matches!(task_type, "media" | "media_upload" | "media_download"),
            WorkerType::Background | WorkerType::AppService => {
                matches!(task_type, "background" | "background_jobs" | "smoke" | "smoke_test")
            }
        }
    }

    fn worker_supported_task_types(worker_type: WorkerType) -> Option<Vec<String>> {
        match worker_type {
            WorkerType::Master => None,
            WorkerType::Frontend => Some(vec!["http".to_string(), "presence".to_string()]),
            WorkerType::Synchrotron => Some(vec!["sync".to_string()]),
            WorkerType::FederationSender => Some(vec!["federation".to_string(), "federation_send".to_string()]),
            WorkerType::FederationReader => Some(vec!["federation_read".to_string(), "federation_ingress".to_string()]),
            WorkerType::EventPersister => {
                Some(vec!["event_persist".to_string(), "events".to_string(), "event_processing".to_string()])
            }
            WorkerType::Pusher => Some(vec!["push".to_string(), "push_notifications".to_string()]),
            WorkerType::MediaRepository => {
                Some(vec!["media".to_string(), "media_upload".to_string(), "media_download".to_string()])
            }
            WorkerType::Background | WorkerType::AppService => Some(vec![
                "background".to_string(),
                "background_jobs".to_string(),
                "smoke".to_string(),
                "smoke_test".to_string(),
            ]),
        }
    }

    fn validate_worker_task_ownership(worker: &WorkerInfo, task_type: &str) -> Result<WorkerType, ApiError> {
        let worker_type = WorkerType::from_str(&worker.worker_type).map_err(ApiError::bad_request)?;
        Self::validate_worker_is_running(worker)?;
        if !Self::worker_supports_task_type(worker_type, task_type) {
            return Err(ApiError::bad_request(format!(
                "Worker '{}' of type '{}' cannot own task type '{}'",
                worker.worker_id,
                worker_type.as_str(),
                task_type
            )));
        }
        Ok(worker_type)
    }

    fn validate_worker_is_running(worker: &WorkerInfo) -> Result<WorkerType, ApiError> {
        let worker_type = WorkerType::from_str(&worker.worker_type).map_err(ApiError::bad_request)?;
        if worker.status != WorkerStatus::Running.as_str() {
            return Err(ApiError::conflict(format!(
                "Worker '{}' is not running and cannot claim or own tasks",
                worker.worker_id
            )));
        }
        Ok(worker_type)
    }

    pub fn new(storage: Arc<WorkerStorage>, server_name: String) -> Self {
        Self {
            storage,
            server_name,
            local_worker_id: None,
            connections: Arc::new(RwLock::new(HashMap::new())),
            bus: None,
            stream_manager: None,
            load_balancer: None,
            health_checker: None,
        }
    }

    pub fn with_bus(mut self, bus: Arc<WorkerBus>) -> Self {
        self.bus = Some(bus);
        self
    }

    pub fn with_stream_manager(mut self, stream_manager: Arc<StreamWriterManager>) -> Self {
        self.stream_manager = Some(stream_manager);
        self
    }

    pub fn with_load_balancer(mut self, load_balancer: Arc<WorkerLoadBalancer>) -> Self {
        self.load_balancer = Some(load_balancer);
        self
    }

    pub fn with_health_checker(mut self, health_checker: Arc<HealthChecker>) -> Self {
        self.health_checker = Some(health_checker);
        self
    }

    pub fn enable_bus(&mut self, config: RedisBusConfig, instance_name: String) {
        self.bus = Some(Arc::new(WorkerBus::new(config, self.server_name.clone(), instance_name)));
    }

    pub fn enable_load_balancer(&mut self, strategy: LoadBalanceStrategy) {
        self.load_balancer = Some(Arc::new(WorkerLoadBalancer::new(strategy)));
    }

    pub fn enable_health_checker(&mut self, config: HealthCheckConfig) {
        self.health_checker = Some(Arc::new(HealthChecker::new(config)));
    }

    pub fn bus(&self) -> Option<&Arc<WorkerBus>> {
        self.bus.as_ref()
    }

    pub fn stream_manager(&self) -> Option<&Arc<StreamWriterManager>> {
        self.stream_manager.as_ref()
    }

    pub fn load_balancer(&self) -> Option<&Arc<WorkerLoadBalancer>> {
        self.load_balancer.as_ref()
    }

    pub fn health_checker(&self) -> Option<&Arc<HealthChecker>> {
        self.health_checker.as_ref()
    }

    #[instrument(skip(self, request))]
    pub async fn register(&self, request: RegisterWorkerRequest) -> Result<WorkerInfo, ApiError> {
        info!(
            worker_id = %request.worker_id,
            worker_name = %request.worker_name,
            worker_type = %request.worker_type.as_str(),
            "Registering worker"
        );

        if let Some(existing) = self
            .storage
            .get_worker(&request.worker_id)
            .await
            .map_err(|e| ApiError::internal_with_log("Failed to check existing worker", &e))?
        {
            if existing.status == "running" {
                return Err(ApiError::bad_request(format!("Worker '{}' is already running", existing.worker_id)));
            }
        }

        let worker = self
            .storage
            .register_worker(request.clone())
            .await
            .map_err(|e| ApiError::internal_with_log("Failed to register worker", &e))?;

        if let Some(lb) = &self.load_balancer {
            lb.register_worker(worker.clone()).await;
        }

        if let Some(hc) = &self.health_checker {
            hc.register_worker(&worker.worker_id).await;
        }

        if let Some(bus) = &self.bus {
            let cmd = ReplicationCommand::Replicate {
                stream_name: "workers".to_string(),
                token: worker.worker_id.clone(),
                data: serde_json::json!({
                    "worker_id": worker.worker_id,
                    "worker_type": worker.worker_type,
                    "status": worker.status,
                }),
            };
            let _ = bus.broadcast_command(&cmd).await;
        }

        info!(
            worker_id = %worker.worker_id,
            worker_name = %worker.worker_name,
            worker_type = %worker.worker_type,
            status = %worker.status,
            "Worker registered successfully"
        );
        Ok(worker)
    }

    #[instrument(skip(self))]
    pub async fn get(&self, worker_id: &str) -> Result<Option<WorkerInfo>, ApiError> {
        self.storage.get_worker(worker_id).await.map_err(|e| ApiError::internal_with_log("Failed to get worker", &e))
    }

    #[instrument(skip(self))]
    pub async fn get_by_type(&self, worker_type: WorkerType) -> Result<Vec<WorkerInfo>, ApiError> {
        self.storage
            .get_workers_by_type(worker_type.as_str())
            .await
            .map_err(|e| ApiError::internal_with_log("Failed to get workers by type", &e))
    }

    #[instrument(skip(self))]
    pub async fn get_active(&self) -> Result<Vec<WorkerInfo>, ApiError> {
        self.storage
            .get_active_workers()
            .await
            .map_err(|e| ApiError::internal_with_log("Failed to get active workers", &e))
    }

    #[instrument(skip(self))]
    pub async fn heartbeat(
        &self,
        worker_id: &str,
        status: WorkerStatus,
        load_stats: Option<WorkerLoadStatsUpdate>,
    ) -> Result<(), ApiError> {
        self.storage
            .update_worker_status(worker_id, status.as_str())
            .await
            .map_err(|e| ApiError::internal_with_log("Failed to update worker status", &e))?;

        match status {
            WorkerStatus::Starting | WorkerStatus::Running => {
                if let Some(lb) = &self.load_balancer {
                    if let Some(worker) = self
                        .storage
                        .get_worker(worker_id)
                        .await
                        .map_err(|e| ApiError::internal_with_log("Failed to refresh worker after heartbeat", &e))?
                    {
                        lb.register_worker(worker).await;
                    }
                }

                if let Some(hc) = &self.health_checker {
                    hc.register_worker(worker_id).await;
                }
            }
            WorkerStatus::Stopping | WorkerStatus::Stopped | WorkerStatus::Error => {
                if let Some(lb) = &self.load_balancer {
                    lb.unregister_worker(worker_id).await;
                }

                if let Some(hc) = &self.health_checker {
                    hc.unregister_worker(worker_id).await;
                }
            }
        }

        if let Some(stats) = load_stats {
            let _ = self
                .storage
                .record_load_stats(worker_id, &stats)
                .map_err(|e| warn!(error = %e, worker_id = %worker_id, "Failed to record load stats"));
        }

        debug!("Heartbeat received from worker: {}", worker_id);
        Ok(())
    }

    #[instrument(skip(self))]
    pub async fn unregister(&self, worker_id: &str) -> Result<(), ApiError> {
        info!(worker_id = %worker_id, "Unregistering worker");

        self.storage
            .unregister_worker(worker_id)
            .await
            .map_err(|e| ApiError::internal_with_log("Failed to unregister worker", &e))?;

        if let Some(lb) = &self.load_balancer {
            lb.unregister_worker(worker_id).await;
        }

        if let Some(hc) = &self.health_checker {
            hc.unregister_worker(worker_id).await;
        }

        let conn = {
            let mut connections = self.connections.write().await;
            connections.remove(worker_id)
        };
        if let Some(conn) = conn {
            conn.disconnect().await;
        }

        info!(worker_id = %worker_id, "Worker unregistered successfully");
        Ok(())
    }

    #[instrument(skip(self))]
    pub async fn send_command(&self, request: SendCommandRequest) -> Result<WorkerCommand, ApiError> {
        info!(
            target_worker_id = %request.target_worker_id,
            command_type = %request.command_type,
            "Sending command to worker"
        );

        let command = self
            .storage
            .create_command(request.clone())
            .await
            .map_err(|e| ApiError::internal_with_log("Failed to create command", &e))?;

        let connections = self.connections.read().await;
        if let Some(conn) = connections.get(&request.target_worker_id) {
            let cmd = ReplicationCommand::Replicate {
                stream_name: "commands".to_string(),
                token: command.command_id.clone(),
                data: serde_json::json!({
                    "command_id": command.command_id,
                    "command_type": command.command_type,
                    "command_data": command.command_data,
                }),
            };

            if let Err(e) = conn.send_command(&cmd).await {
                warn!(
                    error = %e,
                    target_worker_id = %request.target_worker_id,
                    command_id = %command.command_id,
                    command_type = %command.command_type,
                    "Failed to send command via TCP"
                );
            }
        }

        self.storage
            .mark_command_sent(&command.command_id)
            .await
            .map_err(|e| ApiError::internal_with_log("Failed to mark command sent", &e))?;

        info!(
            command_id = %command.command_id,
            target_worker_id = %command.target_worker_id,
            command_type = %command.command_type,
            "Command sent successfully"
        );
        Ok(command)
    }

    #[instrument(skip(self))]
    pub async fn get_pending_commands(&self, worker_id: &str, limit: i64) -> Result<Vec<WorkerCommand>, ApiError> {
        self.storage
            .get_pending_commands(worker_id, limit)
            .await
            .map_err(|e| ApiError::internal_with_log("Failed to get pending commands", &e))
    }

    #[instrument(skip(self))]
    pub async fn complete_command(&self, command_id: &str) -> Result<(), ApiError> {
        self.storage
            .complete_command(command_id)
            .await
            .map_err(|e| ApiError::internal_with_log("Failed to complete command", &e))?;

        info!(command_id = %command_id, "Command completed");
        Ok(())
    }

    #[instrument(skip(self))]
    pub async fn fail_command(&self, command_id: &str, error: &str) -> Result<(), ApiError> {
        self.storage
            .fail_command(command_id, error)
            .await
            .map_err(|e| ApiError::internal_with_log("Failed to fail command", &e))?;

        warn!(command_id = %command_id, error_message = %error, "Command failed");
        Ok(())
    }

    #[instrument(skip(self, event_data))]
    pub async fn add_event(
        &self,
        event_id: &str,
        event_type: &str,
        room_id: Option<&str>,
        sender: Option<&str>,
        event_data: serde_json::Value,
    ) -> Result<WorkerEvent, ApiError> {
        let event = self
            .storage
            .add_event(event_id, event_type, room_id, sender, event_data)
            .await
            .map_err(|e| ApiError::internal_with_log("Failed to add event", &e))?;

        self.broadcast_event(&event).await?;

        debug!("Event added: {} (stream_id: {})", event_id, event.stream_id);
        Ok(event)
    }

    async fn broadcast_event(&self, event: &WorkerEvent) -> Result<(), ApiError> {
        let connections = self.connections.read().await;

        let cmd = ReplicationCommand::Rdata {
            stream_name: "events".to_string(),
            token: event.stream_id.to_string(),
            rows: vec![crate::worker::protocol::ReplicationRow {
                stream_id: event.stream_id,
                data: serde_json::json!({
                    "event_id": event.event_id,
                    "event_type": event.event_type,
                    "room_id": event.room_id,
                    "sender": event.sender,
                    "event_data": event.event_data,
                }),
            }],
        };

        for (worker_id, conn) in connections.iter() {
            if let Err(e) = conn.send_command(&cmd).await {
                warn!(
                    error = %e,
                    worker_id = %worker_id,
                    event_id = %event.event_id,
                    event_type = %event.event_type,
                    room_id = ?event.room_id,
                    "Failed to broadcast event to worker"
                );
            }
        }

        Ok(())
    }

    #[instrument(skip(self))]
    pub async fn get_events_since(&self, stream_id: i64, limit: i64) -> Result<Vec<WorkerEvent>, ApiError> {
        self.storage
            .get_events_since(stream_id, limit)
            .await
            .map_err(|e| ApiError::internal_with_log("Failed to get events", &e))
    }

    #[instrument(skip(self))]
    pub async fn update_replication_position(
        &self,
        worker_id: &str,
        stream_name: &str,
        position: i64,
    ) -> Result<(), ApiError> {
        self.storage
            .update_replication_position(worker_id, stream_name, position)
            .await
            .map_err(|e| ApiError::internal_with_log("Failed to update replication position", &e))?;

        debug!("Replication position updated: {} - {} = {}", worker_id, stream_name, position);
        Ok(())
    }

    #[instrument(skip(self))]
    pub async fn get_replication_position(&self, worker_id: &str, stream_name: &str) -> Result<Option<i64>, ApiError> {
        self.storage
            .get_replication_position(worker_id, stream_name)
            .await
            .map_err(|e| ApiError::internal_with_log("Failed to get replication position", &e))
    }

    #[instrument(skip(self))]
    pub async fn assign_task(&self, request: AssignTaskRequest) -> Result<WorkerTaskAssignment, ApiError> {
        info!(task_type = %request.task_type, preferred_worker_id = ?request.preferred_worker_id, "Creating task");

        let task = self
            .storage
            .assign_task(request.clone())
            .await
            .map_err(|e| ApiError::internal_with_log("Failed to assign task", &e))?;

        if let Some(preferred_worker_id) = request.preferred_worker_id {
            let worker = self
                .get(&preferred_worker_id)
                .await?
                .ok_or_else(|| ApiError::not_found(format!("Preferred worker '{}' not found", preferred_worker_id)))?;
            Self::validate_worker_task_ownership(&worker, &task.task_type)?;

            let claimed = self
                .storage
                .assign_task_to_worker(&task.task_id, &preferred_worker_id)
                .await
                .map_err(|e| ApiError::internal_with_log("Failed to assign task to worker", &e))?;

            if !claimed {
                return Err(ApiError::conflict("Task was already claimed before preferred assignment".to_string()));
            }
        }

        info!(task_id = %task.task_id, task_type = %task.task_type, "Task created");
        Ok(task)
    }

    #[instrument(skip(self))]
    pub async fn get_pending_tasks(&self, limit: i64) -> Result<Vec<WorkerTaskAssignment>, ApiError> {
        self.storage
            .get_pending_tasks(limit)
            .await
            .map_err(|e| ApiError::internal_with_log("Failed to get pending tasks", &e))
    }

    #[instrument(skip(self))]
    pub async fn claim_task(&self, task_id: &str, worker_id: &str) -> Result<(), ApiError> {
        let task = self
            .storage
            .get_pending_tasks(1000)
            .await
            .map_err(|e| ApiError::internal_with_log("Failed to inspect pending tasks before claim", &e))?
            .into_iter()
            .find(|task| task.task_id == task_id)
            .ok_or_else(|| ApiError::not_found("Task is not pending or unavailable"))?;
        let worker = self
            .get(worker_id)
            .await?
            .ok_or_else(|| ApiError::not_found(format!("Worker '{}' not found", worker_id)))?;
        Self::validate_worker_task_ownership(&worker, &task.task_type)?;

        let claimed = self
            .storage
            .assign_task_to_worker(task_id, worker_id)
            .await
            .map_err(|e| ApiError::internal_with_log("Failed to claim task", &e))?;

        if !claimed {
            return Err(ApiError::conflict("Task is already claimed or unavailable".to_string()));
        }

        info!(task_id = %task_id, worker_id = %worker_id, "Task claimed by worker");
        Ok(())
    }

    #[instrument(skip(self))]
    pub async fn claim_next_pending_task(&self, worker_id: &str) -> Result<WorkerTaskAssignment, ApiError> {
        let worker = self
            .get(worker_id)
            .await?
            .ok_or_else(|| ApiError::not_found(format!("Worker '{}' not found", worker_id)))?;
        let worker_type = Self::validate_worker_is_running(&worker)?;
        let task: Option<WorkerTaskAssignment> =
            if let Some(task_types) = Self::worker_supported_task_types(worker_type) {
                self.storage
                    .claim_next_pending_task_for_types(worker_id, &task_types)
                    .await
                    .map_err(|e| ApiError::internal_with_log("Failed to claim next compatible pending task", &e))?
            } else {
                self.storage
                    .claim_next_pending_task(worker_id)
                    .await
                    .map_err(|e| ApiError::internal_with_log("Failed to claim next pending task", &e))?
            };

        let task: WorkerTaskAssignment = task.ok_or_else(|| ApiError::not_found("No pending tasks available"))?;

        info!(task_id = %task.task_id, worker_id = %worker_id, "Task claimed atomically by worker");
        Ok(task)
    }
    #[instrument(skip(self, result))]
    pub async fn complete_task(&self, task_id: &str, result: Option<serde_json::Value>) -> Result<(), ApiError> {
        self.storage
            .complete_task(task_id, result)
            .await
            .map_err(|e| ApiError::internal_with_log("Failed to complete task", &e))?;

        info!(task_id = %task_id, "Task completed");
        Ok(())
    }

    #[instrument(skip(self))]
    pub async fn fail_task(&self, task_id: &str, error: &str) -> Result<(), ApiError> {
        self.storage
            .fail_task(task_id, error)
            .await
            .map_err(|e| ApiError::internal_with_log("Failed to fail task", &e))?;

        warn!(task_id = %task_id, error_message = %error, "Task failed");
        Ok(())
    }

    #[instrument(skip(self))]
    pub async fn connect_to_worker(&self, worker_id: &str, addr: &str) -> Result<(), ApiError> {
        info!(worker_id = %worker_id, remote_addr = %addr, "Connecting to worker");

        let _worker = self
            .storage
            .get_worker(worker_id)
            .await
            .map_err(|e| ApiError::internal_with_log("Failed to get worker", &e))?
            .ok_or_else(|| ApiError::not_found("Worker not found"))?;

        let conn = ReplicationConnection::new(worker_id.to_string());
        conn.connect(addr).await.map_err(|e| ApiError::internal_with_log("Failed to connect to worker", &e))?;

        let _ = self
            .storage
            .record_connection(&self.local_worker_id.clone().unwrap_or_default(), worker_id, "replication")
            .map_err(|e| {
                warn!(
                    error = %e,
                    local_worker_id = %self.local_worker_id.clone().unwrap_or_default(),
                    worker_id = %worker_id,
                    connection_type = %"replication",
                    "Failed to record connection"
                )
            });

        let mut connections = self.connections.write().await;
        connections.insert(worker_id.to_string(), conn);

        info!(worker_id = %worker_id, remote_addr = %addr, "Connected to worker");
        Ok(())
    }

    #[instrument(skip(self))]
    pub async fn disconnect_from_worker(&self, worker_id: &str) -> Result<(), ApiError> {
        info!(worker_id = %worker_id, "Disconnecting from worker");

        let conn = {
            let mut connections = self.connections.write().await;
            connections.remove(worker_id)
        };
        if let Some(conn) = conn {
            conn.disconnect().await;
        }

        info!(worker_id = %worker_id, "Disconnected from worker");
        Ok(())
    }

    #[instrument(skip(self))]
    pub async fn get_statistics(&self, limit: i64) -> Result<Vec<serde_json::Value>, ApiError> {
        self.storage
            .get_statistics(limit)
            .await
            .map_err(|e| ApiError::internal_with_log("Failed to get statistics", &e))
    }

    #[instrument(skip(self))]
    pub async fn get_type_statistics(&self) -> Result<Vec<serde_json::Value>, ApiError> {
        self.storage
            .get_type_statistics()
            .await
            .map_err(|e| ApiError::internal_with_log("Failed to get type statistics", &e))
    }

    pub async fn select_worker_for_task(&self, task_type: &str) -> Result<Option<String>, ApiError> {
        if let Some(lb) = &self.load_balancer {
            if let Some(worker_id) = lb.select_worker(task_type).await {
                if let Some(hc) = &self.health_checker {
                    if !hc.is_healthy(&worker_id).await {
                        warn!(worker_id = %worker_id, task_type = %task_type, "Selected worker is not healthy, falling back");
                        return self.select_worker_fallback(task_type).await;
                    }
                }
                return Ok(Some(worker_id));
            }
        }

        self.select_worker_fallback(task_type).await
    }

    async fn select_worker_fallback(&self, task_type: &str) -> Result<Option<String>, ApiError> {
        let active_workers = self.get_active().await?;

        let candidates: Vec<&WorkerInfo> = active_workers
            .iter()
            .filter(|w| {
                if let Ok(worker_type) = WorkerType::from_str(&w.worker_type) {
                    Self::worker_supports_task_type(worker_type, task_type)
                } else {
                    false
                }
            })
            .collect();

        if candidates.is_empty() {
            return Ok(None);
        }

        let healthy_worker_ids = if let Some(hc) = &self.health_checker {
            // Parallelize health checks across all candidate workers since each
            // is_healthy call is independent (no early exit, no inter-iteration deps).
            let health_futures = candidates.iter().map(|candidate| {
                let hc = Arc::clone(hc);
                let worker_id = candidate.worker_id.clone();
                async move {
                    let is_healthy = hc.is_healthy(&worker_id).await;
                    (worker_id, is_healthy)
                }
            });
            let healthy: HashSet<String> = futures::future::join_all(health_futures)
                .await
                .into_iter()
                .filter(|(_, is_healthy)| *is_healthy)
                .map(|(worker_id, _)| worker_id)
                .collect();
            Some(healthy)
        } else {
            None
        };

        let selected = Self::select_fallback_candidate(&candidates, healthy_worker_ids.as_ref());

        Ok(selected.map(|w| w.worker_id.clone()))
    }

    pub fn set_local_worker_id(&mut self, worker_id: String) {
        self.local_worker_id = Some(worker_id);
    }

    pub fn get_local_worker_id(&self) -> Option<&str> {
        self.local_worker_id.as_deref()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_worker_capabilities_for_task() {
        let master_caps = WorkerCapabilities::for_type(&WorkerType::Master);
        assert!(master_caps.can_handle_http);
        assert!(master_caps.can_persist_events);

        let frontend_caps = WorkerCapabilities::for_type(&WorkerType::Frontend);
        assert!(frontend_caps.can_handle_http);
        assert!(!frontend_caps.can_persist_events);
    }

    #[test]
    fn test_worker_type_as_str() {
        assert_eq!(WorkerType::Master.as_str(), "master");
        assert_eq!(WorkerType::EventPersister.as_str(), "event_persister");
    }

    #[test]
    fn test_worker_supports_task_type_mappings() {
        assert!(WorkerManager::worker_supports_task_type(WorkerType::Master, "event_processing"));
        assert!(WorkerManager::worker_supports_task_type(WorkerType::EventPersister, "event_processing"));
        assert!(WorkerManager::worker_supports_task_type(WorkerType::Synchrotron, "sync"));
        assert!(WorkerManager::worker_supports_task_type(WorkerType::Background, "smoke_test"));
        assert!(!WorkerManager::worker_supports_task_type(WorkerType::Frontend, "event_processing"));
        assert!(!WorkerManager::worker_supports_task_type(WorkerType::Pusher, "background"));
    }

    #[test]
    fn test_validate_worker_task_ownership_rejects_wrong_task_type() {
        let worker = WorkerInfo {
            id: 1,
            worker_id: "frontend-1".to_string(),
            worker_name: "frontend".to_string(),
            worker_type: "frontend".to_string(),
            host: "127.0.0.1".to_string(),
            port: 8101,
            status: "running".to_string(),
            last_heartbeat_ts: Some(1),
            started_ts: 1,
            stopped_ts: None,
            config: serde_json::json!({}),
            metadata: serde_json::json!({}),
            version: Some("1.0.0".to_string()),
        };

        let err = WorkerManager::validate_worker_task_ownership(&worker, "event_processing")
            .expect_err("frontend should not own event processing tasks");
        assert!(err.is_bad_request());
    }

    #[test]
    fn test_validate_worker_task_ownership_accepts_running_compatible_worker() {
        let worker = WorkerInfo {
            id: 1,
            worker_id: "event-persister-1".to_string(),
            worker_name: "event-persister".to_string(),
            worker_type: "event_persister".to_string(),
            host: "127.0.0.1".to_string(),
            port: 8102,
            status: "running".to_string(),
            last_heartbeat_ts: Some(1),
            started_ts: 1,
            stopped_ts: None,
            config: serde_json::json!({}),
            metadata: serde_json::json!({}),
            version: Some("1.0.0".to_string()),
        };

        let worker_type = WorkerManager::validate_worker_task_ownership(&worker, "event_processing")
            .expect("running event persister should own event_processing tasks");
        assert_eq!(worker_type, WorkerType::EventPersister);
    }

    #[test]
    fn test_validate_worker_task_ownership_rejects_non_running_worker_even_when_task_type_matches() {
        let worker = WorkerInfo {
            id: 1,
            worker_id: "background-1".to_string(),
            worker_name: "background".to_string(),
            worker_type: "background".to_string(),
            host: "127.0.0.1".to_string(),
            port: 8103,
            status: "stopped".to_string(),
            last_heartbeat_ts: Some(1),
            started_ts: 1,
            stopped_ts: Some(2),
            config: serde_json::json!({}),
            metadata: serde_json::json!({}),
            version: Some("1.0.0".to_string()),
        };

        let err = WorkerManager::validate_worker_task_ownership(&worker, "background_jobs")
            .expect_err("stopped worker must not claim or own tasks even when task type matches");
        assert!(err.is_conflict());
        assert!(err.to_string().contains("is not running"));
    }

    #[test]
    fn test_select_most_recent_worker_prefers_freshest_heartbeat() {
        let older = WorkerInfo {
            id: 1,
            worker_id: "frontend-older".to_string(),
            worker_name: "frontend-older".to_string(),
            worker_type: "frontend".to_string(),
            host: "127.0.0.1".to_string(),
            port: 8101,
            status: "running".to_string(),
            last_heartbeat_ts: Some(100),
            started_ts: 1,
            stopped_ts: None,
            config: serde_json::json!({}),
            metadata: serde_json::json!({}),
            version: Some("1.0.0".to_string()),
        };
        let newer = WorkerInfo {
            id: 2,
            worker_id: "frontend-newer".to_string(),
            worker_name: "frontend-newer".to_string(),
            worker_type: "frontend".to_string(),
            host: "127.0.0.1".to_string(),
            port: 8102,
            status: "running".to_string(),
            last_heartbeat_ts: Some(200),
            started_ts: 2,
            stopped_ts: None,
            config: serde_json::json!({}),
            metadata: serde_json::json!({}),
            version: Some("1.0.0".to_string()),
        };

        let selected = WorkerManager::select_most_recent_worker(&[&older, &newer])
            .expect("one of the candidates should be selected");
        assert_eq!(selected.worker_id, newer.worker_id);
    }

    #[test]
    fn test_select_most_recent_worker_treats_missing_heartbeat_as_staler_than_present_value() {
        let missing = WorkerInfo {
            id: 1,
            worker_id: "frontend-missing".to_string(),
            worker_name: "frontend-missing".to_string(),
            worker_type: "frontend".to_string(),
            host: "127.0.0.1".to_string(),
            port: 8101,
            status: "running".to_string(),
            last_heartbeat_ts: None,
            started_ts: 1,
            stopped_ts: None,
            config: serde_json::json!({}),
            metadata: serde_json::json!({}),
            version: Some("1.0.0".to_string()),
        };
        let present = WorkerInfo {
            id: 2,
            worker_id: "frontend-present".to_string(),
            worker_name: "frontend-present".to_string(),
            worker_type: "frontend".to_string(),
            host: "127.0.0.1".to_string(),
            port: 8102,
            status: "running".to_string(),
            last_heartbeat_ts: Some(1),
            started_ts: 2,
            stopped_ts: None,
            config: serde_json::json!({}),
            metadata: serde_json::json!({}),
            version: Some("1.0.0".to_string()),
        };

        let selected = WorkerManager::select_most_recent_worker(&[&missing, &present])
            .expect("one of the candidates should be selected");
        assert_eq!(selected.worker_id, present.worker_id);
    }

    #[test]
    fn test_select_fallback_candidate_prefers_healthy_candidate_over_staler_unhealthy_one() {
        let unhealthy_newer = WorkerInfo {
            id: 1,
            worker_id: "frontend-unhealthy".to_string(),
            worker_name: "frontend-unhealthy".to_string(),
            worker_type: "frontend".to_string(),
            host: "127.0.0.1".to_string(),
            port: 8101,
            status: "running".to_string(),
            last_heartbeat_ts: Some(200),
            started_ts: 2,
            stopped_ts: None,
            config: serde_json::json!({}),
            metadata: serde_json::json!({}),
            version: Some("1.0.0".to_string()),
        };
        let healthy_older = WorkerInfo {
            id: 2,
            worker_id: "frontend-healthy".to_string(),
            worker_name: "frontend-healthy".to_string(),
            worker_type: "frontend".to_string(),
            host: "127.0.0.1".to_string(),
            port: 8102,
            status: "running".to_string(),
            last_heartbeat_ts: Some(100),
            started_ts: 1,
            stopped_ts: None,
            config: serde_json::json!({}),
            metadata: serde_json::json!({}),
            version: Some("1.0.0".to_string()),
        };
        let healthy_worker_ids = HashSet::from([healthy_older.worker_id.clone()]);

        let selected =
            WorkerManager::select_fallback_candidate(&[&unhealthy_newer, &healthy_older], Some(&healthy_worker_ids))
                .expect("healthy fallback candidate should be selected");
        assert_eq!(selected.worker_id, healthy_older.worker_id);
    }

    #[test]
    fn test_select_fallback_candidate_falls_back_to_recent_worker_when_no_healthy_candidates_exist() {
        let newer = WorkerInfo {
            id: 1,
            worker_id: "frontend-newer".to_string(),
            worker_name: "frontend-newer".to_string(),
            worker_type: "frontend".to_string(),
            host: "127.0.0.1".to_string(),
            port: 8101,
            status: "running".to_string(),
            last_heartbeat_ts: Some(200),
            started_ts: 2,
            stopped_ts: None,
            config: serde_json::json!({}),
            metadata: serde_json::json!({}),
            version: Some("1.0.0".to_string()),
        };
        let older = WorkerInfo {
            id: 2,
            worker_id: "frontend-older".to_string(),
            worker_name: "frontend-older".to_string(),
            worker_type: "frontend".to_string(),
            host: "127.0.0.1".to_string(),
            port: 8102,
            status: "running".to_string(),
            last_heartbeat_ts: Some(100),
            started_ts: 1,
            stopped_ts: None,
            config: serde_json::json!({}),
            metadata: serde_json::json!({}),
            version: Some("1.0.0".to_string()),
        };
        let healthy_worker_ids = HashSet::new();

        let selected = WorkerManager::select_fallback_candidate(&[&older, &newer], Some(&healthy_worker_ids))
            .expect("recent fallback candidate should still be selected");
        assert_eq!(selected.worker_id, newer.worker_id);
    }
}
