use crate::worker::bus::WorkerBus;
use crate::worker::health::{HealthCheckConfig, HealthChecker};
use crate::worker::load_balancer::{LoadBalanceStrategy, WorkerLoadBalancer};
use crate::worker::protocol::ReplicationCommand;
use crate::worker::storage::WorkerStoreApi;
use crate::worker::stream::StreamWriterManager;
use crate::worker::tcp::ReplicationConnection;
use crate::worker::types::*;
use std::collections::{HashMap, HashSet};
use std::str::FromStr;
use std::sync::Arc;
use synapse_common::ApiError;
use tokio::sync::RwLock;
use tracing::{debug, info, instrument, warn};

/// The `WorkerManager` struct.
#[allow(dead_code)]
pub struct WorkerManager {
    storage: Arc<dyn WorkerStoreApi>,
    // Reserved for future cluster rollout; currently unused after remove enable_bus.
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

    /// See [`new`].
    pub fn new(storage: Arc<dyn WorkerStoreApi>, server_name: String) -> Self {
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

    /// See [`with_bus`].
    pub fn with_bus(mut self, bus: Arc<WorkerBus>) -> Self {
        self.bus = Some(bus);
        self
    }

    /// See [`with_stream_manager`].
    pub fn with_stream_manager(mut self, stream_manager: Arc<StreamWriterManager>) -> Self {
        self.stream_manager = Some(stream_manager);
        self
    }

    /// See [`with_load_balancer`].
    pub fn with_load_balancer(mut self, load_balancer: Arc<WorkerLoadBalancer>) -> Self {
        self.load_balancer = Some(load_balancer);
        self
    }

    /// See [`with_health_checker`].
    pub fn with_health_checker(mut self, health_checker: Arc<HealthChecker>) -> Self {
        self.health_checker = Some(health_checker);
        self
    }

    // BUS wire-up: connect() and broadcast_command() are intentionally not
    // wired in admin.rs today — multi-instance cluster replication rolls
    // out as a follow-up. When the cluster rollout lands, callers should
    // use `with_bus(Arc::new(WorkerBus::new(cfg, server_name, name)))` and
    // then call `bus.connect().await` from the container startup path.

    /// See [`enable_load_balancer`].
    pub fn enable_load_balancer(&mut self, strategy: LoadBalanceStrategy) {
        self.load_balancer = Some(Arc::new(WorkerLoadBalancer::new(strategy)));
    }

    /// See [`enable_health_checker`].
    pub fn enable_health_checker(&mut self, config: HealthCheckConfig) {
        self.health_checker = Some(Arc::new(HealthChecker::new(config)));
    }

    /// See [`bus`].
    pub fn bus(&self) -> Option<&Arc<WorkerBus>> {
        self.bus.as_ref()
    }

    /// See [`stream_manager`].
    pub fn stream_manager(&self) -> Option<&Arc<StreamWriterManager>> {
        self.stream_manager.as_ref()
    }

    /// See [`load_balancer`].
    pub fn load_balancer(&self) -> Option<&Arc<WorkerLoadBalancer>> {
        self.load_balancer.as_ref()
    }

    /// See [`health_checker`].
    pub fn health_checker(&self) -> Option<&Arc<HealthChecker>> {
        self.health_checker.as_ref()
    }

    /// See [`register`].
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
            .map_err(|e| ApiError::internal_with_context("Failed to check existing worker", &e))?
        {
            // P-080: Return 409 Conflict for any duplicate worker_id (not just "running" status)
            return Err(ApiError::conflict(format!(
                "Worker '{}' already exists with status '{}'",
                existing.worker_id, existing.status
            )));
        }

        let worker = self
            .storage
            .register_worker(request.clone())
            .await
            .map_err(|e| ApiError::internal_with_context("Failed to register worker", &e))?;

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
            if let Err(e) = bus.broadcast_command(&cmd).await {
                warn!(error = %e, worker_id = %worker.worker_id, "Failed to broadcast worker status update");
            }
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

    /// See [`get`].
    #[instrument(skip(self))]
    pub async fn get(&self, worker_id: &str) -> Result<Option<WorkerInfo>, ApiError> {
        self.storage
            .get_worker(worker_id)
            .await
            .map_err(|e| ApiError::internal_with_context("Failed to get worker", &e))
    }

    /// See [`get_by_type`].
    #[instrument(skip(self))]
    pub async fn get_by_type(&self, worker_type: WorkerType) -> Result<Vec<WorkerInfo>, ApiError> {
        self.storage
            .get_workers_by_type(worker_type.as_str())
            .await
            .map_err(|e| ApiError::internal_with_context("Failed to get workers by type", &e))
    }

    /// See [`get_active`].
    #[instrument(skip(self))]
    pub async fn get_active(&self) -> Result<Vec<WorkerInfo>, ApiError> {
        self.storage
            .get_active_workers()
            .await
            .map_err(|e| ApiError::internal_with_context("Failed to get active workers", &e))
    }

    /// See [`heartbeat`].
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
            .map_err(|e| ApiError::internal_with_context("Failed to update worker status", &e))?;

        match status {
            WorkerStatus::Starting | WorkerStatus::Running => {
                if let Some(lb) = &self.load_balancer {
                    if let Some(worker) =
                        self.storage.get_worker(worker_id).await.map_err(|e| {
                            ApiError::internal_with_context("Failed to refresh worker after heartbeat", &e)
                        })?
                    {
                        lb.register_worker(worker).await;
                    }
                }

                if let Some(hc) = &self.health_checker {
                    hc.register_worker(worker_id).await;
                    // WORK-04: 心跳驱动活性探测
                    hc.record_heartbeat(worker_id).await;
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

    /// See [`unregister`].
    #[instrument(skip(self))]
    pub async fn unregister(&self, worker_id: &str) -> Result<(), ApiError> {
        info!(worker_id = %worker_id, "Unregistering worker");

        self.storage
            .unregister_worker(worker_id)
            .await
            .map_err(|e| ApiError::internal_with_context("Failed to unregister worker", &e))?;

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

    /// See [`send_command`].
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
            .map_err(|e| ApiError::internal_with_context("Failed to create command", &e))?;

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
            .map_err(|e| ApiError::internal_with_context("Failed to mark command sent", &e))?;

        info!(
            command_id = %command.command_id,
            target_worker_id = %command.target_worker_id,
            command_type = %command.command_type,
            "Command sent successfully"
        );
        Ok(command)
    }

    /// See [`get_pending_commands`].
    #[instrument(skip(self))]
    pub async fn get_pending_commands(&self, worker_id: &str, limit: i64) -> Result<Vec<WorkerCommand>, ApiError> {
        self.storage
            .get_pending_commands(worker_id, limit)
            .await
            .map_err(|e| ApiError::internal_with_context("Failed to get pending commands", &e))
    }

    /// See [`complete_command`].
    #[instrument(skip(self))]
    pub async fn complete_command(&self, command_id: &str) -> Result<(), ApiError> {
        self.storage
            .complete_command(command_id)
            .await
            .map_err(|e| ApiError::internal_with_context("Failed to complete command", &e))?;

        info!(command_id = %command_id, "Command completed");
        Ok(())
    }

    /// See [`fail_command`].
    #[instrument(skip(self))]
    pub async fn fail_command(&self, command_id: &str, error: &str) -> Result<(), ApiError> {
        self.storage
            .fail_command(command_id, error)
            .await
            .map_err(|e| ApiError::internal_with_context("Failed to fail command", &e))?;

        warn!(command_id = %command_id, error_message = %error, "Command failed");
        Ok(())
    }

    /// See [`add_event`].
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
            .map_err(|e| ApiError::internal_with_context("Failed to add event", &e))?;

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

    /// See [`get_events_since`].
    #[instrument(skip(self))]
    pub async fn get_events_since(&self, stream_id: i64, limit: i64) -> Result<Vec<WorkerEvent>, ApiError> {
        self.storage
            .get_events_since(stream_id, limit)
            .await
            .map_err(|e| ApiError::internal_with_context("Failed to get events", &e))
    }

    /// See [`update_replication_position`].
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
            .map_err(|e| ApiError::internal_with_context("Failed to update replication position", &e))?;

        debug!("Replication position updated: {} - {} = {}", worker_id, stream_name, position);
        Ok(())
    }

    /// See [`get_replication_position`].
    #[instrument(skip(self))]
    pub async fn get_replication_position(&self, worker_id: &str, stream_name: &str) -> Result<Option<i64>, ApiError> {
        self.storage
            .get_replication_position(worker_id, stream_name)
            .await
            .map_err(|e| ApiError::internal_with_context("Failed to get replication position", &e))
    }

    /// See [`assign_task`].
    #[instrument(skip(self))]
    pub async fn assign_task(&self, request: AssignTaskRequest) -> Result<WorkerTaskAssignment, ApiError> {
        info!(task_type = %request.task_type, preferred_worker_id = ?request.preferred_worker_id, "Creating task");

        let task = self
            .storage
            .assign_task(request.clone())
            .await
            .map_err(|e| ApiError::internal_with_context("Failed to assign task", &e))?;

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
                .map_err(|e| ApiError::internal_with_context("Failed to assign task to worker", &e))?;

            if !claimed {
                return Err(ApiError::conflict("Task was already claimed before preferred assignment".to_string()));
            }
        }

        info!(task_id = %task.task_id, task_type = %task.task_type, "Task created");
        Ok(task)
    }

    /// See [`get_pending_tasks`].
    #[instrument(skip(self))]
    pub async fn get_pending_tasks(&self, limit: i64) -> Result<Vec<WorkerTaskAssignment>, ApiError> {
        self.storage
            .get_pending_tasks(limit)
            .await
            .map_err(|e| ApiError::internal_with_context("Failed to get pending tasks", &e))
    }

    /// See [`claim_task`].
    #[instrument(skip(self))]
    pub async fn claim_task(&self, task_id: &str, worker_id: &str) -> Result<(), ApiError> {
        // PERF-05: 按 task_id 直查，不再拉 1000 条 pending 到内存 find——
        // 旧实现在待领任务 >1000 时会错误地报 not_found。
        let task = self
            .storage
            .get_pending_task_by_id(task_id)
            .await
            .map_err(|e| ApiError::internal_with_context("Failed to load pending task before claim", &e))?
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
            .map_err(|e| ApiError::internal_with_context("Failed to claim task", &e))?;

        if !claimed {
            return Err(ApiError::conflict("Task is already claimed or unavailable".to_string()));
        }

        info!(task_id = %task_id, worker_id = %worker_id, "Task claimed by worker");
        Ok(())
    }

    /// See [`claim_next_pending_task`].
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
                    .map_err(|e| ApiError::internal_with_context("Failed to claim next compatible pending task", &e))?
            } else {
                self.storage
                    .claim_next_pending_task(worker_id)
                    .await
                    .map_err(|e| ApiError::internal_with_context("Failed to claim next pending task", &e))?
            };

        let task: WorkerTaskAssignment = task.ok_or_else(|| ApiError::not_found("No pending tasks available"))?;

        info!(task_id = %task.task_id, worker_id = %worker_id, "Task claimed atomically by worker");
        Ok(task)
    }
    /// See [`complete_task`].
    #[instrument(skip(self, result))]
    pub async fn complete_task(&self, task_id: &str, result: Option<serde_json::Value>) -> Result<(), ApiError> {
        self.storage
            .complete_task(task_id, result)
            .await
            .map_err(|e| ApiError::internal_with_context("Failed to complete task", &e))?;

        info!(task_id = %task_id, "Task completed");
        Ok(())
    }

    /// See [`fail_task`].
    #[instrument(skip(self))]
    pub async fn fail_task(&self, task_id: &str, error: &str) -> Result<(), ApiError> {
        self.storage
            .fail_task(task_id, error)
            .await
            .map_err(|e| ApiError::internal_with_context("Failed to fail task", &e))?;

        warn!(task_id = %task_id, error_message = %error, "Task failed");
        Ok(())
    }

    /// See [`connect_to_worker`].
    #[instrument(skip(self))]
    pub async fn connect_to_worker(&self, worker_id: &str, addr: &str) -> Result<(), ApiError> {
        info!(worker_id = %worker_id, remote_addr = %addr, "Connecting to worker");

        let _worker = self
            .storage
            .get_worker(worker_id)
            .await
            .map_err(|e| ApiError::internal_with_context("Failed to get worker", &e))?
            .ok_or_else(|| ApiError::not_found("Worker not found"))?;

        let conn = ReplicationConnection::new(worker_id.to_string());
        conn.connect(addr).await.map_err(|e| ApiError::internal_with_context("Failed to connect to worker", &e))?;

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

    /// See [`disconnect_from_worker`].
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

    /// See [`get_statistics`].
    #[instrument(skip(self))]
    pub async fn get_statistics(&self, limit: i64) -> Result<Vec<serde_json::Value>, ApiError> {
        self.storage
            .get_statistics(limit)
            .await
            .map_err(|e| ApiError::internal_with_context("Failed to get statistics", &e))
    }

    /// See [`get_type_statistics`].
    #[instrument(skip(self))]
    pub async fn get_type_statistics(&self) -> Result<Vec<serde_json::Value>, ApiError> {
        self.storage
            .get_type_statistics()
            .await
            .map_err(|e| ApiError::internal_with_context("Failed to get type statistics", &e))
    }

    /// See [`select_worker_for_task`].
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

    /// See [`set_local_worker_id`].
    pub fn set_local_worker_id(&mut self, worker_id: String) {
        self.local_worker_id = Some(worker_id);
    }

    /// See [`get_local_worker_id`].
    pub fn get_local_worker_id(&self) -> Option<&str> {
        self.local_worker_id.as_deref()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // PERF-05: claim_task 必须能领取排在前 1000 条之外的待领任务。
    // 旧实现 `get_pending_tasks(1000)` + 内存 find，超过 1000 条即误报 not_found。
    #[tokio::test]
    async fn perf05_claim_task_beyond_first_1000_pending() {
        use synapse_storage::test_mocks::InMemoryWorkerStore;
        let store = Arc::new(InMemoryWorkerStore::new());
        let manager = WorkerManager::new(store.clone(), "test.server".to_string());
        store
            .register_worker(RegisterWorkerRequest {
                worker_id: "master-1".to_string(),
                worker_name: "master".to_string(),
                worker_type: WorkerType::Master,
                host: "127.0.0.1".to_string(),
                port: 8100,
                config: None,
                metadata: None,
                version: None,
            })
            .await
            .expect("register worker");

        // 1000 条 priority=0 的任务 + 1 条 priority=-1 的目标任务。
        // 目标任务稳定排在 pending 列表末尾，旧实现的 LIMIT 1000 必然截掉它。
        for _ in 0..1000 {
            store
                .assign_task(AssignTaskRequest {
                    task_type: "event_processing".to_string(),
                    task_data: serde_json::json!({}),
                    priority: Some(0),
                    preferred_worker_id: None,
                })
                .await
                .expect("assign task");
        }
        let target = store
            .assign_task(AssignTaskRequest {
                task_type: "event_processing".to_string(),
                task_data: serde_json::json!({}),
                priority: Some(-1),
                preferred_worker_id: None,
            })
            .await
            .expect("assign target task");

        manager.claim_task(&target.task_id, "master-1").await.expect("task beyond first 1000 must be claimable");
    }

    #[tokio::test]
    async fn perf05_claim_unknown_task_returns_not_found() {
        use synapse_storage::test_mocks::InMemoryWorkerStore;
        let store = Arc::new(InMemoryWorkerStore::new());
        let manager = WorkerManager::new(store, "test.server".to_string());

        let err = manager.claim_task("task-nonexistent", "master-1").await.expect_err("unknown task must fail");
        assert!(err.is_not_found());
    }

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
