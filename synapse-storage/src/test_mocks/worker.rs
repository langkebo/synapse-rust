use super::*;

#[allow(clippy::type_complexity)]
#[derive(Clone, Default)]
pub struct InMemoryWorkerStore {
    workers: Arc<RwLock<HashMap<String, crate::worker::WorkerInfo>>>,
    commands: Arc<RwLock<Vec<crate::worker::WorkerCommand>>>,
    events: Arc<RwLock<Vec<crate::worker::WorkerEvent>>>,
    replication_positions: Arc<RwLock<HashMap<(String, String), crate::worker::ReplicationPosition>>>,
    tasks: Arc<RwLock<Vec<crate::worker::WorkerTaskAssignment>>>,
    next_id: Arc<std::sync::atomic::AtomicI64>,
}

impl InMemoryWorkerStore {
    pub fn new() -> Self {
        Self {
            workers: Arc::new(RwLock::new(HashMap::new())),
            commands: Arc::new(RwLock::new(Vec::new())),
            events: Arc::new(RwLock::new(Vec::new())),
            replication_positions: Arc::new(RwLock::new(HashMap::new())),
            tasks: Arc::new(RwLock::new(Vec::new())),
            next_id: Arc::new(std::sync::atomic::AtomicI64::new(1)),
        }
    }
}

#[async_trait::async_trait]
impl crate::worker::WorkerStoreApi for InMemoryWorkerStore {
    async fn register_worker(
        &self,
        request: crate::worker::RegisterWorkerRequest,
    ) -> Result<crate::worker::WorkerInfo, sqlx::Error> {
        let id = self.next_id.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        let now = chrono::Utc::now().timestamp_millis();
        let info = crate::worker::WorkerInfo {
            id,
            worker_id: request.worker_id.clone(),
            worker_name: request.worker_name,
            worker_type: request.worker_type.as_str().to_string(),
            host: request.host,
            port: request.port as i32,
            status: "running".to_string(),
            last_heartbeat_ts: Some(now),
            started_ts: now,
            stopped_ts: None,
            config: request.config.unwrap_or(serde_json::Value::Null),
            metadata: request.metadata.unwrap_or(serde_json::Value::Null),
            version: request.version,
        };
        self.workers.write().await.insert(info.worker_id.clone(), info.clone());
        Ok(info)
    }

    async fn get_worker(&self, worker_id: &str) -> Result<Option<crate::worker::WorkerInfo>, sqlx::Error> {
        Ok(self.workers.read().await.get(worker_id).cloned())
    }

    async fn get_workers_by_type(&self, worker_type: &str) -> Result<Vec<crate::worker::WorkerInfo>, sqlx::Error> {
        Ok(self.workers.read().await.values().filter(|w| w.worker_type == worker_type).cloned().collect())
    }

    async fn get_active_workers(&self) -> Result<Vec<crate::worker::WorkerInfo>, sqlx::Error> {
        Ok(self
            .workers
            .read()
            .await
            .values()
            .filter(|w| w.status == "running" || w.status == "starting")
            .cloned()
            .collect())
    }

    async fn update_worker_status(&self, worker_id: &str, status: &str) -> Result<(), sqlx::Error> {
        let mut workers = self.workers.write().await;
        if let Some(worker) = workers.get_mut(worker_id) {
            worker.status = status.to_string();
            if status == "stopped" {
                worker.stopped_ts = Some(chrono::Utc::now().timestamp_millis());
            }
        }
        Ok(())
    }

    async fn update_heartbeat(&self, worker_id: &str) -> Result<(), sqlx::Error> {
        let mut workers = self.workers.write().await;
        if let Some(worker) = workers.get_mut(worker_id) {
            worker.last_heartbeat_ts = Some(chrono::Utc::now().timestamp_millis());
        }
        Ok(())
    }

    async fn unregister_worker(&self, worker_id: &str) -> Result<(), sqlx::Error> {
        self.workers.write().await.remove(worker_id);
        Ok(())
    }

    async fn create_command(
        &self,
        request: crate::worker::SendCommandRequest,
    ) -> Result<crate::worker::WorkerCommand, sqlx::Error> {
        let id = self.next_id.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        let now = chrono::Utc::now().timestamp_millis();
        let command_id = format!("cmd-{id}");
        let command = crate::worker::WorkerCommand {
            id,
            command_id: command_id.clone(),
            target_worker_id: request.target_worker_id,
            source_worker_id: None,
            command_type: request.command_type,
            command_data: request.command_data,
            priority: request.priority.unwrap_or(0),
            status: "pending".to_string(),
            created_ts: now,
            sent_ts: None,
            completed_ts: None,
            error_message: None,
            retry_count: 0,
            max_retries: request.max_retries.unwrap_or(3),
        };
        self.commands.write().await.push(command.clone());
        Ok(command)
    }

    async fn get_pending_commands(
        &self,
        worker_id: &str,
        limit: i64,
    ) -> Result<Vec<crate::worker::WorkerCommand>, sqlx::Error> {
        let commands = self.commands.read().await;
        let mut result: Vec<_> =
            commands.iter().filter(|c| c.target_worker_id == worker_id && c.status == "pending").cloned().collect();
        result.sort_by(|a, b| b.priority.cmp(&a.priority).then(a.created_ts.cmp(&b.created_ts)));
        result.truncate(limit as usize);
        Ok(result)
    }

    async fn mark_command_sent(&self, command_id: &str) -> Result<(), sqlx::Error> {
        let now = chrono::Utc::now().timestamp_millis();
        let mut commands = self.commands.write().await;
        for cmd in commands.iter_mut() {
            if cmd.command_id == command_id {
                cmd.status = "sent".to_string();
                cmd.sent_ts = Some(now);
                break;
            }
        }
        Ok(())
    }

    async fn complete_command(&self, command_id: &str) -> Result<(), sqlx::Error> {
        let now = chrono::Utc::now().timestamp_millis();
        let mut commands = self.commands.write().await;
        for cmd in commands.iter_mut() {
            if cmd.command_id == command_id {
                cmd.status = "completed".to_string();
                cmd.completed_ts = Some(now);
                break;
            }
        }
        Ok(())
    }

    async fn fail_command(&self, command_id: &str, error: &str) -> Result<(), sqlx::Error> {
        let now = chrono::Utc::now().timestamp_millis();
        let mut commands = self.commands.write().await;
        for cmd in commands.iter_mut() {
            if cmd.command_id == command_id {
                cmd.status = "failed".to_string();
                cmd.completed_ts = Some(now);
                cmd.error_message = Some(error.to_string());
                cmd.retry_count += 1;
                break;
            }
        }
        Ok(())
    }

    async fn add_event(
        &self,
        event_id: &str,
        event_type: &str,
        room_id: Option<&str>,
        sender: Option<&str>,
        event_data: serde_json::Value,
    ) -> Result<crate::worker::WorkerEvent, sqlx::Error> {
        let id = self.next_id.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        let stream_id = id;
        let now = chrono::Utc::now().timestamp_millis();
        let event = crate::worker::WorkerEvent {
            id,
            event_id: event_id.to_string(),
            stream_id,
            event_type: event_type.to_string(),
            room_id: room_id.map(|s| s.to_string()),
            sender: sender.map(|s| s.to_string()),
            event_data,
            created_ts: now,
            processed_by: Some(Vec::new()),
        };
        self.events.write().await.push(event.clone());
        Ok(event)
    }

    async fn get_events_since(
        &self,
        stream_id: i64,
        limit: i64,
    ) -> Result<Vec<crate::worker::WorkerEvent>, sqlx::Error> {
        let events = self.events.read().await;
        let mut result: Vec<_> = events.iter().filter(|e| e.stream_id > stream_id).cloned().collect();
        result.sort_by_key(|e| e.stream_id);
        result.truncate(limit as usize);
        Ok(result)
    }

    async fn mark_event_processed(&self, event_id: &str, worker_id: &str) -> Result<(), sqlx::Error> {
        let mut events = self.events.write().await;
        for event in events.iter_mut() {
            if event.event_id == event_id {
                let processed = event.processed_by.get_or_insert_with(Vec::new);
                if !processed.iter().any(|w| w == worker_id) {
                    processed.push(worker_id.to_string());
                }
                break;
            }
        }
        Ok(())
    }

    async fn update_replication_position(
        &self,
        worker_id: &str,
        stream_name: &str,
        position: i64,
    ) -> Result<(), sqlx::Error> {
        let now = chrono::Utc::now().timestamp_millis();
        let id = self.next_id.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        let key = (worker_id.to_string(), stream_name.to_string());
        self.replication_positions.write().await.insert(
            key,
            crate::worker::ReplicationPosition {
                id,
                worker_id: worker_id.to_string(),
                stream_name: stream_name.to_string(),
                stream_position: position,
                updated_ts: now,
            },
        );
        Ok(())
    }

    async fn get_replication_position(&self, worker_id: &str, stream_name: &str) -> Result<Option<i64>, sqlx::Error> {
        let key = (worker_id.to_string(), stream_name.to_string());
        Ok(self.replication_positions.read().await.get(&key).map(|p| p.stream_position))
    }

    fn record_load_stats(
        &self,
        _worker_id: &str,
        _stats: &crate::worker::WorkerLoadStatsUpdate,
    ) -> Result<(), sqlx::Error> {
        Ok(())
    }

    async fn assign_task(
        &self,
        request: crate::worker::AssignTaskRequest,
    ) -> Result<crate::worker::WorkerTaskAssignment, sqlx::Error> {
        let id = self.next_id.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        let now = chrono::Utc::now().timestamp_millis();
        let task_id = format!("task-{id}");
        let task = crate::worker::WorkerTaskAssignment {
            id,
            task_id: task_id.clone(),
            task_type: request.task_type,
            task_data: request.task_data,
            assigned_worker_id: request.preferred_worker_id,
            status: "pending".to_string(),
            priority: request.priority.unwrap_or(0),
            created_ts: now,
            assigned_ts: None,
            completed_ts: None,
            result: None,
            error_message: None,
        };
        self.tasks.write().await.push(task.clone());
        Ok(task)
    }

    async fn get_pending_tasks(&self, limit: i64) -> Result<Vec<crate::worker::WorkerTaskAssignment>, sqlx::Error> {
        let tasks = self.tasks.read().await;
        let mut result: Vec<_> = tasks.iter().filter(|t| t.status == "pending").cloned().collect();
        result.sort_by(|a, b| b.priority.cmp(&a.priority).then(a.created_ts.cmp(&b.created_ts)));
        result.truncate(limit as usize);
        Ok(result)
    }

    async fn claim_next_pending_task(
        &self,
        worker_id: &str,
    ) -> Result<Option<crate::worker::WorkerTaskAssignment>, sqlx::Error> {
        let now = chrono::Utc::now().timestamp_millis();
        let mut tasks = self.tasks.write().await;
        // Pick highest priority, earliest created pending task
        let mut chosen: Option<usize> = None;
        for (idx, task) in tasks.iter().enumerate() {
            if task.status != "pending" {
                continue;
            }
            match chosen {
                None => chosen = Some(idx),
                Some(prev) => {
                    let prev_task = &tasks[prev];
                    let better = (task.priority > prev_task.priority)
                        || (task.priority == prev_task.priority && task.created_ts < prev_task.created_ts);
                    if better {
                        chosen = Some(idx);
                    }
                }
            }
        }
        if let Some(idx) = chosen {
            tasks[idx].status = "assigned".to_string();
            tasks[idx].assigned_worker_id = Some(worker_id.to_string());
            tasks[idx].assigned_ts = Some(now);
            Ok(Some(tasks[idx].clone()))
        } else {
            Ok(None)
        }
    }

    async fn claim_next_pending_task_for_types(
        &self,
        worker_id: &str,
        allowed_task_types: &[String],
    ) -> Result<Option<crate::worker::WorkerTaskAssignment>, sqlx::Error> {
        let now = chrono::Utc::now().timestamp_millis();
        let mut tasks = self.tasks.write().await;
        let mut chosen: Option<usize> = None;
        for (idx, task) in tasks.iter().enumerate() {
            if task.status != "pending" {
                continue;
            }
            if !allowed_task_types.iter().any(|t| t == &task.task_type) {
                continue;
            }
            match chosen {
                None => chosen = Some(idx),
                Some(prev) => {
                    let prev_task = &tasks[prev];
                    let better = (task.priority > prev_task.priority)
                        || (task.priority == prev_task.priority && task.created_ts < prev_task.created_ts);
                    if better {
                        chosen = Some(idx);
                    }
                }
            }
        }
        if let Some(idx) = chosen {
            tasks[idx].status = "assigned".to_string();
            tasks[idx].assigned_worker_id = Some(worker_id.to_string());
            tasks[idx].assigned_ts = Some(now);
            Ok(Some(tasks[idx].clone()))
        } else {
            Ok(None)
        }
    }

    async fn assign_task_to_worker(&self, task_id: &str, worker_id: &str) -> Result<bool, sqlx::Error> {
        let now = chrono::Utc::now().timestamp_millis();
        let mut tasks = self.tasks.write().await;
        for task in tasks.iter_mut() {
            if task.task_id == task_id && task.status == "pending" {
                task.status = "assigned".to_string();
                task.assigned_worker_id = Some(worker_id.to_string());
                task.assigned_ts = Some(now);
                return Ok(true);
            }
        }
        Ok(false)
    }

    async fn complete_task(&self, task_id: &str, result: Option<serde_json::Value>) -> Result<(), sqlx::Error> {
        let now = chrono::Utc::now().timestamp_millis();
        let mut tasks = self.tasks.write().await;
        for task in tasks.iter_mut() {
            if task.task_id == task_id {
                task.status = "completed".to_string();
                task.completed_ts = Some(now);
                task.result = result;
                break;
            }
        }
        Ok(())
    }

    async fn fail_task(&self, task_id: &str, error: &str) -> Result<(), sqlx::Error> {
        let now = chrono::Utc::now().timestamp_millis();
        let mut tasks = self.tasks.write().await;
        for task in tasks.iter_mut() {
            if task.task_id == task_id {
                task.status = "failed".to_string();
                task.completed_ts = Some(now);
                task.error_message = Some(error.to_string());
                break;
            }
        }
        Ok(())
    }

    fn record_connection(
        &self,
        _source_worker_id: &str,
        _target_worker_id: &str,
        _connection_type: &str,
    ) -> Result<(), sqlx::Error> {
        Ok(())
    }

    fn update_connection_stats(
        &self,
        _request: &crate::worker::UpdateConnectionStatsRequest,
    ) -> Result<(), sqlx::Error> {
        Ok(())
    }

    async fn get_statistics(&self, _limit: i64) -> Result<Vec<serde_json::Value>, sqlx::Error> {
        Ok(Vec::new())
    }

    async fn get_type_statistics(&self) -> Result<Vec<serde_json::Value>, sqlx::Error> {
        Ok(Vec::new())
    }
}
