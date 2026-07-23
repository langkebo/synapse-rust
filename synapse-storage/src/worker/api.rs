use async_trait::async_trait;

use super::models::*;
use super::WorkerStorage;

#[async_trait]
pub trait WorkerStoreApi: Send + Sync {
    async fn register_worker(&self, request: RegisterWorkerRequest) -> Result<WorkerInfo, sqlx::Error>;
    async fn get_worker(&self, worker_id: &str) -> Result<Option<WorkerInfo>, sqlx::Error>;
    async fn get_workers_by_type(&self, worker_type: &str) -> Result<Vec<WorkerInfo>, sqlx::Error>;
    async fn get_active_workers(&self) -> Result<Vec<WorkerInfo>, sqlx::Error>;
    async fn update_worker_status(&self, worker_id: &str, status: &str) -> Result<(), sqlx::Error>;
    async fn update_heartbeat(&self, worker_id: &str) -> Result<(), sqlx::Error>;
    async fn unregister_worker(&self, worker_id: &str) -> Result<(), sqlx::Error>;
    async fn create_command(&self, request: SendCommandRequest) -> Result<WorkerCommand, sqlx::Error>;
    async fn get_pending_commands(&self, worker_id: &str, limit: i64) -> Result<Vec<WorkerCommand>, sqlx::Error>;
    async fn mark_command_sent(&self, command_id: &str) -> Result<(), sqlx::Error>;
    async fn complete_command(&self, command_id: &str) -> Result<(), sqlx::Error>;
    async fn fail_command(&self, command_id: &str, error: &str) -> Result<(), sqlx::Error>;
    async fn add_event(
        &self,
        event_id: &str,
        event_type: &str,
        room_id: Option<&str>,
        sender: Option<&str>,
        event_data: serde_json::Value,
    ) -> Result<WorkerEvent, sqlx::Error>;
    async fn get_events_since(&self, stream_id: i64, limit: i64) -> Result<Vec<WorkerEvent>, sqlx::Error>;
    async fn mark_event_processed(&self, event_id: &str, worker_id: &str) -> Result<(), sqlx::Error>;
    async fn update_replication_position(
        &self,
        worker_id: &str,
        stream_name: &str,
        position: i64,
    ) -> Result<(), sqlx::Error>;
    async fn get_replication_position(&self, worker_id: &str, stream_name: &str) -> Result<Option<i64>, sqlx::Error>;
    fn record_load_stats(&self, worker_id: &str, stats: &WorkerLoadStatsUpdate) -> Result<(), sqlx::Error>;
    async fn assign_task(&self, request: AssignTaskRequest) -> Result<WorkerTaskAssignment, sqlx::Error>;
    async fn get_pending_tasks(&self, limit: i64) -> Result<Vec<WorkerTaskAssignment>, sqlx::Error>;
    async fn claim_next_pending_task(&self, worker_id: &str) -> Result<Option<WorkerTaskAssignment>, sqlx::Error>;
    async fn claim_next_pending_task_for_types(
        &self,
        worker_id: &str,
        allowed_task_types: &[String],
    ) -> Result<Option<WorkerTaskAssignment>, sqlx::Error>;
    async fn assign_task_to_worker(&self, task_id: &str, worker_id: &str) -> Result<bool, sqlx::Error>;
    async fn complete_task(&self, task_id: &str, result: Option<serde_json::Value>) -> Result<(), sqlx::Error>;
    async fn fail_task(&self, task_id: &str, error: &str) -> Result<(), sqlx::Error>;
    fn record_connection(
        &self,
        source_worker_id: &str,
        target_worker_id: &str,
        connection_type: &str,
    ) -> Result<(), sqlx::Error>;
    fn update_connection_stats(&self, request: &UpdateConnectionStatsRequest) -> Result<(), sqlx::Error>;
    async fn get_statistics(&self, limit: i64) -> Result<Vec<serde_json::Value>, sqlx::Error>;
    async fn get_type_statistics(&self) -> Result<Vec<serde_json::Value>, sqlx::Error>;
}

#[async_trait]
impl WorkerStoreApi for WorkerStorage {
    async fn register_worker(&self, request: RegisterWorkerRequest) -> Result<WorkerInfo, sqlx::Error> {
        self.register_worker(request).await
    }

    async fn get_worker(&self, worker_id: &str) -> Result<Option<WorkerInfo>, sqlx::Error> {
        self.get_worker(worker_id).await
    }

    async fn get_workers_by_type(&self, worker_type: &str) -> Result<Vec<WorkerInfo>, sqlx::Error> {
        self.get_workers_by_type(worker_type).await
    }

    async fn get_active_workers(&self) -> Result<Vec<WorkerInfo>, sqlx::Error> {
        self.get_active_workers().await
    }

    async fn update_worker_status(&self, worker_id: &str, status: &str) -> Result<(), sqlx::Error> {
        self.update_worker_status(worker_id, status).await
    }

    async fn update_heartbeat(&self, worker_id: &str) -> Result<(), sqlx::Error> {
        self.update_heartbeat(worker_id).await
    }

    async fn unregister_worker(&self, worker_id: &str) -> Result<(), sqlx::Error> {
        self.unregister_worker(worker_id).await
    }

    async fn create_command(&self, request: SendCommandRequest) -> Result<WorkerCommand, sqlx::Error> {
        self.create_command(request).await
    }

    async fn get_pending_commands(&self, worker_id: &str, limit: i64) -> Result<Vec<WorkerCommand>, sqlx::Error> {
        self.get_pending_commands(worker_id, limit).await
    }

    async fn mark_command_sent(&self, command_id: &str) -> Result<(), sqlx::Error> {
        self.mark_command_sent(command_id).await
    }

    async fn complete_command(&self, command_id: &str) -> Result<(), sqlx::Error> {
        self.complete_command(command_id).await
    }

    async fn fail_command(&self, command_id: &str, error: &str) -> Result<(), sqlx::Error> {
        self.fail_command(command_id, error).await
    }

    async fn add_event(
        &self,
        event_id: &str,
        event_type: &str,
        room_id: Option<&str>,
        sender: Option<&str>,
        event_data: serde_json::Value,
    ) -> Result<WorkerEvent, sqlx::Error> {
        self.add_event(event_id, event_type, room_id, sender, event_data).await
    }

    async fn get_events_since(&self, stream_id: i64, limit: i64) -> Result<Vec<WorkerEvent>, sqlx::Error> {
        self.get_events_since(stream_id, limit).await
    }

    async fn mark_event_processed(&self, event_id: &str, worker_id: &str) -> Result<(), sqlx::Error> {
        self.mark_event_processed(event_id, worker_id).await
    }

    async fn update_replication_position(
        &self,
        worker_id: &str,
        stream_name: &str,
        position: i64,
    ) -> Result<(), sqlx::Error> {
        self.update_replication_position(worker_id, stream_name, position).await
    }

    async fn get_replication_position(&self, worker_id: &str, stream_name: &str) -> Result<Option<i64>, sqlx::Error> {
        self.get_replication_position(worker_id, stream_name).await
    }

    fn record_load_stats(&self, worker_id: &str, stats: &WorkerLoadStatsUpdate) -> Result<(), sqlx::Error> {
        self.record_load_stats(worker_id, stats)
    }

    async fn assign_task(&self, request: AssignTaskRequest) -> Result<WorkerTaskAssignment, sqlx::Error> {
        self.assign_task(request).await
    }

    async fn get_pending_tasks(&self, limit: i64) -> Result<Vec<WorkerTaskAssignment>, sqlx::Error> {
        self.get_pending_tasks(limit).await
    }

    async fn claim_next_pending_task(&self, worker_id: &str) -> Result<Option<WorkerTaskAssignment>, sqlx::Error> {
        self.claim_next_pending_task(worker_id).await
    }

    async fn claim_next_pending_task_for_types(
        &self,
        worker_id: &str,
        allowed_task_types: &[String],
    ) -> Result<Option<WorkerTaskAssignment>, sqlx::Error> {
        self.claim_next_pending_task_for_types(worker_id, allowed_task_types).await
    }

    async fn assign_task_to_worker(&self, task_id: &str, worker_id: &str) -> Result<bool, sqlx::Error> {
        self.assign_task_to_worker(task_id, worker_id).await
    }

    async fn complete_task(&self, task_id: &str, result: Option<serde_json::Value>) -> Result<(), sqlx::Error> {
        self.complete_task(task_id, result).await
    }

    async fn fail_task(&self, task_id: &str, error: &str) -> Result<(), sqlx::Error> {
        self.fail_task(task_id, error).await
    }

    fn record_connection(
        &self,
        source_worker_id: &str,
        target_worker_id: &str,
        connection_type: &str,
    ) -> Result<(), sqlx::Error> {
        self.record_connection(source_worker_id, target_worker_id, connection_type)
    }

    fn update_connection_stats(&self, request: &UpdateConnectionStatsRequest) -> Result<(), sqlx::Error> {
        self.update_connection_stats(request)
    }

    async fn get_statistics(&self, limit: i64) -> Result<Vec<serde_json::Value>, sqlx::Error> {
        self.get_statistics(limit).await
    }

    async fn get_type_statistics(&self) -> Result<Vec<serde_json::Value>, sqlx::Error> {
        self.get_type_statistics().await
    }
}
