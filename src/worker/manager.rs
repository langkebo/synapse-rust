use crate::common::ApiError;
use crate::worker::bus::{RedisConfig, WorkerBus};
use crate::worker::health::{HealthCheckConfig, HealthChecker};
use crate::worker::load_balancer::{LoadBalanceStrategy, WorkerLoadBalancer};
use crate::worker::protocol::{ReplicationCommand, ReplicationProtocol};
use crate::worker::storage::WorkerStorage;
use crate::worker::stream::StreamWriterManager;
use crate::worker::tcp::ReplicationConnection;
use crate::worker::types::*;
use std::collections::HashMap;
use std::str::FromStr;
use std::sync::Arc;
use tokio::sync::RwLock;
use tracing::{debug, info, instrument, warn};

pub struct WorkerManager {
    storage: Arc<WorkerStorage>,
    server_name: String,
    local_worker_id: Option<String>,
    connections: Arc<RwLock<HashMap<String, ReplicationConnection>>>,
    #[allow(dead_code)]
    protocol: ReplicationProtocol,
    bus: Option<Arc<WorkerBus>>,
    stream_manager: Option<Arc<StreamWriterManager>>,
    load_balancer: Option<Arc<WorkerLoadBalancer>>,
    health_checker: Option<Arc<HealthChecker>>,
}

impl WorkerManager {
    pub fn new(storage: Arc<WorkerStorage>, server_name: String) -> Self {
        Self {
            storage,
            server_name,
            local_worker_id: None,
            connections: Arc::new(RwLock::new(HashMap::new())),
            protocol: ReplicationProtocol::new(),
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

    pub fn enable_bus(&mut self, config: RedisConfig, instance_name: String) {
        self.bus = Some(Arc::new(WorkerBus::new(
            config,
            self.server_name.clone(),
            instance_name,
        )));
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
            "Registering worker: {} ({})",
            request.worker_name,
            request.worker_type.as_str()
        );

        if let Some(existing) = self
            .storage
            .get_worker(&request.worker_id)
            .await
            .map_err(|e| ApiError::internal(format!("Failed to check existing worker: {}", e)))?
        {
            if existing.status == "running" {
                return Err(ApiError::bad_request(format!(
                    "Worker '{}' is already running",
                    existing.worker_id
                )));
            }
        }

        let worker = self
            .storage
            .register_worker(request.clone())
            .await
            .map_err(|e| ApiError::internal(format!("Failed to register worker: {}", e)))?;

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

        info!("Worker registered successfully: {}", worker.worker_id);
        Ok(worker)
    }

    #[instrument(skip(self))]
    pub async fn get(&self, worker_id: &str) -> Result<Option<WorkerInfo>, ApiError> {
        self.storage
            .get_worker(worker_id)
            .await
            .map_err(|e| ApiError::internal(format!("Failed to get worker: {}", e)))
    }

    #[instrument(skip(self))]
    pub async fn get_by_type(&self, worker_type: WorkerType) -> Result<Vec<WorkerInfo>, ApiError> {
        self.storage
            .get_workers_by_type(worker_type.as_str())
            .await
            .map_err(|e| ApiError::internal(format!("Failed to get workers by type: {}", e)))
    }

    #[instrument(skip(self))]
    pub async fn get_active(&self) -> Result<Vec<WorkerInfo>, ApiError> {
        self.storage
            .get_active_workers()
            .await
            .map_err(|e| ApiError::internal(format!("Failed to get active workers: {}", e)))
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
            .map_err(|e| ApiError::internal(format!("Failed to update worker status: {}", e)))?;

        if let Some(stats) = load_stats {
            let _ = self
                .storage
                .record_load_stats(worker_id, &stats)
                .await
                .map_err(|e| warn!("Failed to record load stats: {}", e));
        }

        debug!("Heartbeat received from worker: {}", worker_id);
        Ok(())
    }

    #[instrument(skip(self))]
    pub async fn unregister(&self, worker_id: &str) -> Result<(), ApiError> {
        info!("Unregistering worker: {}", worker_id);

        self.storage
            .unregister_worker(worker_id)
            .await
            .map_err(|e| ApiError::internal(format!("Failed to unregister worker: {}", e)))?;

        if let Some(lb) = &self.load_balancer {
            lb.unregister_worker(worker_id).await;
        }

        if let Some(hc) = &self.health_checker {
            hc.unregister_worker(worker_id).await;
        }

        let mut connections = self.connections.write().await;
        if let Some(conn) = connections.remove(worker_id) {
            conn.disconnect().await;
        }

        info!("Worker unregistered successfully: {}", worker_id);
        Ok(())
    }

    #[instrument(skip(self))]
    pub async fn send_command(
        &self,
        request: SendCommandRequest,
    ) -> Result<WorkerCommand, ApiError> {
        info!(
            "Sending command to worker: {} - {}",
            request.target_worker_id, request.command_type
        );

        let command = self
            .storage
            .create_command(request.clone())
            .await
            .map_err(|e| ApiError::internal(format!("Failed to create command: {}", e)))?;

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
                warn!("Failed to send command via TCP: {}", e);
            }
        }

        self.storage
            .mark_command_sent(&command.command_id)
            .await
            .map_err(|e| ApiError::internal(format!("Failed to mark command sent: {}", e)))?;

        info!("Command sent successfully: {}", command.command_id);
        Ok(command)
    }

    #[instrument(skip(self))]
    pub async fn get_pending_commands(
        &self,
        worker_id: &str,
        limit: i64,
    ) -> Result<Vec<WorkerCommand>, ApiError> {
        self.storage
            .get_pending_commands(worker_id, limit)
            .await
            .map_err(|e| ApiError::internal(format!("Failed to get pending commands: {}", e)))
    }

    #[instrument(skip(self))]
    pub async fn complete_command(&self, command_id: &str) -> Result<(), ApiError> {
        self.storage
            .complete_command(command_id)
            .await
            .map_err(|e| ApiError::internal(format!("Failed to complete command: {}", e)))?;

        info!("Command completed: {}", command_id);
        Ok(())
    }

    #[instrument(skip(self))]
    pub async fn fail_command(&self, command_id: &str, error: &str) -> Result<(), ApiError> {
        self.storage
            .fail_command(command_id, error)
            .await
            .map_err(|e| ApiError::internal(format!("Failed to fail command: {}", e)))?;

        warn!("Command failed: {} - {}", command_id, error);
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
            .map_err(|e| ApiError::internal(format!("Failed to add event: {}", e)))?;

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
                warn!("Failed to broadcast event to worker {}: {}", worker_id, e);
            }
        }

        Ok(())
    }

    #[instrument(skip(self))]
    pub async fn get_events_since(
        &self,
        stream_id: i64,
        limit: i64,
    ) -> Result<Vec<WorkerEvent>, ApiError> {
        self.storage
            .get_events_since(stream_id, limit)
            .await
            .map_err(|e| ApiError::internal(format!("Failed to get events: {}", e)))
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
            .map_err(|e| {
                ApiError::internal(format!("Failed to update replication position: {}", e))
            })?;

        debug!(
            "Replication position updated: {} - {} = {}",
            worker_id, stream_name, position
        );
        Ok(())
    }

    #[instrument(skip(self))]
    pub async fn get_replication_position(
        &self,
        worker_id: &str,
        stream_name: &str,
    ) -> Result<Option<i64>, ApiError> {
        self.storage
            .get_replication_position(worker_id, stream_name)
            .await
            .map_err(|e| ApiError::internal(format!("Failed to get replication position: {}", e)))
    }

    #[instrument(skip(self))]
    pub async fn assign_task(
        &self,
        request: AssignTaskRequest,
    ) -> Result<WorkerTaskAssignment, ApiError> {
        info!("Creating task: {}", request.task_type);

        let task = self
            .storage
            .assign_task(request.clone())
            .await
            .map_err(|e| ApiError::internal(format!("Failed to assign task: {}", e)))?;

        if let Some(preferred_worker_id) = request.preferred_worker_id {
            self.storage
                .assign_task_to_worker(&task.task_id, &preferred_worker_id)
                .await
                .map_err(|e| {
                    ApiError::internal(format!("Failed to assign task to worker: {}", e))
                })?;
        }

        info!("Task created: {}", task.task_id);
        Ok(task)
    }

    #[instrument(skip(self))]
    pub async fn get_pending_tasks(
        &self,
        limit: i64,
    ) -> Result<Vec<WorkerTaskAssignment>, ApiError> {
        self.storage
            .get_pending_tasks(limit)
            .await
            .map_err(|e| ApiError::internal(format!("Failed to get pending tasks: {}", e)))
    }

    #[instrument(skip(self))]
    pub async fn claim_task(&self, task_id: &str, worker_id: &str) -> Result<(), ApiError> {
        self.storage
            .assign_task_to_worker(task_id, worker_id)
            .await
            .map_err(|e| ApiError::internal(format!("Failed to claim task: {}", e)))?;

        info!("Task {} claimed by worker {}", task_id, worker_id);
        Ok(())
    }

    #[instrument(skip(self, result))]
    pub async fn complete_task(
        &self,
        task_id: &str,
        result: Option<serde_json::Value>,
    ) -> Result<(), ApiError> {
        self.storage
            .complete_task(task_id, result)
            .await
            .map_err(|e| ApiError::internal(format!("Failed to complete task: {}", e)))?;

        info!("Task completed: {}", task_id);
        Ok(())
    }

    #[instrument(skip(self))]
    pub async fn fail_task(&self, task_id: &str, error: &str) -> Result<(), ApiError> {
        self.storage
            .fail_task(task_id, error)
            .await
            .map_err(|e| ApiError::internal(format!("Failed to fail task: {}", e)))?;

        warn!("Task failed: {} - {}", task_id, error);
        Ok(())
    }

    #[instrument(skip(self))]
    pub async fn connect_to_worker(&self, worker_id: &str, addr: &str) -> Result<(), ApiError> {
        info!("Connecting to worker: {} at {}", worker_id, addr);

        let _worker = self
            .storage
            .get_worker(worker_id)
            .await
            .map_err(|e| ApiError::internal(format!("Failed to get worker: {}", e)))?
            .ok_or_else(|| ApiError::not_found("Worker not found"))?;

        let conn = ReplicationConnection::new(self.server_name.clone(), worker_id.to_string());
        conn.connect(addr)
            .await
            .map_err(|e| ApiError::internal(format!("Failed to connect to worker: {}", e)))?;

        let _ = self
            .storage
            .record_connection(
                &self.local_worker_id.clone().unwrap_or_default(),
                worker_id,
                "replication",
            )
            .await
            .map_err(|e| warn!("Failed to record connection: {}", e));

        let mut connections = self.connections.write().await;
        connections.insert(worker_id.to_string(), conn);

        info!("Connected to worker: {}", worker_id);
        Ok(())
    }

    #[instrument(skip(self))]
    pub async fn disconnect_from_worker(&self, worker_id: &str) -> Result<(), ApiError> {
        info!("Disconnecting from worker: {}", worker_id);

        let mut connections = self.connections.write().await;
        if let Some(conn) = connections.remove(worker_id) {
            conn.disconnect().await;
        }

        info!("Disconnected from worker: {}", worker_id);
        Ok(())
    }

    #[instrument(skip(self))]
    pub async fn get_statistics(&self) -> Result<Vec<serde_json::Value>, ApiError> {
        self.storage
            .get_statistics()
            .await
            .map_err(|e| ApiError::internal(format!("Failed to get statistics: {}", e)))
    }

    #[instrument(skip(self))]
    pub async fn get_type_statistics(&self) -> Result<Vec<serde_json::Value>, ApiError> {
        self.storage
            .get_type_statistics()
            .await
            .map_err(|e| ApiError::internal(format!("Failed to get type statistics: {}", e)))
    }

    pub async fn select_worker_for_task(
        &self,
        task_type: &str,
    ) -> Result<Option<String>, ApiError> {
        if let Some(lb) = &self.load_balancer {
            if let Some(worker_id) = lb.select_worker(task_type).await {
                if let Some(hc) = &self.health_checker {
                    if !hc.is_healthy(&worker_id).await {
                        warn!("Selected worker {} is not healthy, falling back", worker_id);
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
                    let caps = WorkerCapabilities::for_type(&worker_type);
                    match task_type {
                        "http" => caps.can_handle_http,
                        "federation" => caps.can_handle_federation,
                        "event_persist" => caps.can_persist_events,
                        "push" => caps.can_send_push,
                        "media" => caps.can_handle_media,
                        "background" => caps.can_run_background_tasks,
                        _ => true,
                    }
                } else {
                    false
                }
            })
            .collect();

        if candidates.is_empty() {
            return Ok(None);
        }

        let selected = candidates.iter().min_by(|a, b| {
            let a_load = a.last_heartbeat_ts.unwrap_or(0);
            let b_load = b.last_heartbeat_ts.unwrap_or(0);
            a_load.cmp(&b_load)
        });

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
}
