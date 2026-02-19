use crate::worker::types::*;
use sqlx::{FromRow, PgPool, Row};
use std::sync::Arc;
use chrono::Utc;

#[derive(Debug, Clone, FromRow)]
pub struct WorkerRow {
    pub id: i64,
    pub worker_id: String,
    pub worker_name: String,
    pub worker_type: String,
    pub host: String,
    pub port: i32,
    pub status: String,
    pub last_heartbeat_ts: Option<i64>,
    pub started_ts: i64,
    pub stopped_ts: Option<i64>,
    pub config: serde_json::Value,
    pub metadata: serde_json::Value,
    pub version: Option<String>,
}

impl From<WorkerRow> for WorkerInfo {
    fn from(row: WorkerRow) -> Self {
        Self {
            id: row.id,
            worker_id: row.worker_id,
            worker_name: row.worker_name,
            worker_type: row.worker_type,
            host: row.host,
            port: row.port,
            status: row.status,
            last_heartbeat_ts: row.last_heartbeat_ts,
            started_ts: row.started_ts,
            stopped_ts: row.stopped_ts,
            config: row.config,
            metadata: row.metadata,
            version: row.version,
        }
    }
}

#[derive(Debug, Clone, FromRow)]
pub struct WorkerCommandRow {
    pub id: i64,
    pub command_id: String,
    pub target_worker_id: String,
    pub source_worker_id: Option<String>,
    pub command_type: String,
    pub command_data: serde_json::Value,
    pub priority: i32,
    pub status: String,
    pub created_ts: i64,
    pub sent_ts: Option<i64>,
    pub completed_ts: Option<i64>,
    pub error_message: Option<String>,
    pub retry_count: i32,
    pub max_retries: i32,
}

impl From<WorkerCommandRow> for WorkerCommand {
    fn from(row: WorkerCommandRow) -> Self {
        Self {
            id: row.id,
            command_id: row.command_id,
            target_worker_id: row.target_worker_id,
            source_worker_id: row.source_worker_id,
            command_type: row.command_type,
            command_data: row.command_data,
            priority: row.priority,
            status: row.status,
            created_ts: row.created_ts,
            sent_ts: row.sent_ts,
            completed_ts: row.completed_ts,
            error_message: row.error_message,
            retry_count: row.retry_count,
            max_retries: row.max_retries,
        }
    }
}

#[derive(Debug, Clone, FromRow)]
pub struct WorkerEventRow {
    pub id: i64,
    pub event_id: String,
    pub stream_id: i64,
    pub event_type: String,
    pub room_id: Option<String>,
    pub sender: Option<String>,
    pub event_data: serde_json::Value,
    pub created_ts: i64,
    pub processed_by: Option<sqlx::types::Json<Vec<String>>>,
}

impl From<WorkerEventRow> for WorkerEvent {
    fn from(row: WorkerEventRow) -> Self {
        Self {
            id: row.id,
            event_id: row.event_id,
            stream_id: row.stream_id,
            event_type: row.event_type,
            room_id: row.room_id,
            sender: row.sender,
            event_data: row.event_data,
            created_ts: row.created_ts,
            processed_by: row.processed_by.map(|p| p.0),
        }
    }
}

#[derive(Debug, Clone, Default)]
pub struct UpdateConnectionStatsRequest {
    pub source_worker_id: String,
    pub target_worker_id: String,
    pub connection_type: String,
    pub bytes_sent: i64,
    pub bytes_received: i64,
    pub messages_sent: i64,
    pub messages_received: i64,
}

impl UpdateConnectionStatsRequest {
    pub fn new(source_worker_id: impl Into<String>, target_worker_id: impl Into<String>, connection_type: impl Into<String>) -> Self {
        Self {
            source_worker_id: source_worker_id.into(),
            target_worker_id: target_worker_id.into(),
            connection_type: connection_type.into(),
            ..Default::default()
        }
    }

    pub fn bytes_sent(mut self, bytes_sent: i64) -> Self {
        self.bytes_sent = bytes_sent;
        self
    }

    pub fn bytes_received(mut self, bytes_received: i64) -> Self {
        self.bytes_received = bytes_received;
        self
    }

    pub fn messages_sent(mut self, messages_sent: i64) -> Self {
        self.messages_sent = messages_sent;
        self
    }

    pub fn messages_received(mut self, messages_received: i64) -> Self {
        self.messages_received = messages_received;
        self
    }
}

#[derive(Clone)]
pub struct WorkerStorage {
    pool: Arc<PgPool>,
}

impl WorkerStorage {
    pub fn new(pool: &Arc<PgPool>) -> Self {
        Self { pool: pool.clone() }
    }

    pub async fn register_worker(&self, request: RegisterWorkerRequest) -> Result<WorkerInfo, sqlx::Error> {
        let now = Utc::now().timestamp_millis();
        let config = request.config.unwrap_or(serde_json::json!({}));
        let metadata = request.metadata.unwrap_or(serde_json::json!({}));

        let row = sqlx::query_as::<_, WorkerRow>(
            r#"
            INSERT INTO workers (
                worker_id, worker_name, worker_type, host, port, status, started_ts, config, metadata, version
            )
            VALUES ($1, $2, $3, $4, $5, 'starting', $6, $7, $8, $9)
            RETURNING *
            "#,
        )
        .bind(&request.worker_id)
        .bind(&request.worker_name)
        .bind(request.worker_type.as_str())
        .bind(&request.host)
        .bind(request.port as i32)
        .bind(now)
        .bind(&config)
        .bind(&metadata)
        .bind(&request.version)
        .fetch_one(&*self.pool)
        .await?;

        Ok(row.into())
    }

    pub async fn get_worker(&self, worker_id: &str) -> Result<Option<WorkerInfo>, sqlx::Error> {
        let row = sqlx::query_as::<_, WorkerRow>(
            r#"SELECT * FROM workers WHERE worker_id = $1"#
        )
        .bind(worker_id)
        .fetch_optional(&*self.pool)
        .await?;

        Ok(row.map(|r| r.into()))
    }

    pub async fn get_workers_by_type(&self, worker_type: &str) -> Result<Vec<WorkerInfo>, sqlx::Error> {
        let rows = sqlx::query_as::<_, WorkerRow>(
            r#"SELECT * FROM workers WHERE worker_type = $1 ORDER BY started_ts DESC"#
        )
        .bind(worker_type)
        .fetch_all(&*self.pool)
        .await?;

        Ok(rows.into_iter().map(|r| r.into()).collect())
    }

    pub async fn get_active_workers(&self) -> Result<Vec<WorkerInfo>, sqlx::Error> {
        let rows = sqlx::query_as::<_, WorkerRow>(
            r#"SELECT * FROM active_workers"#
        )
        .fetch_all(&*self.pool)
        .await?;

        Ok(rows.into_iter().map(|r| r.into()).collect())
    }

    pub async fn update_worker_status(&self, worker_id: &str, status: &str) -> Result<(), sqlx::Error> {
        let now = Utc::now().timestamp_millis();
        
        sqlx::query(
            r#"
            UPDATE workers SET status = $2, last_heartbeat_ts = $3 
            WHERE worker_id = $1
            "#,
        )
        .bind(worker_id)
        .bind(status)
        .bind(now)
        .execute(&*self.pool)
        .await?;

        Ok(())
    }

    pub async fn update_heartbeat(&self, worker_id: &str) -> Result<(), sqlx::Error> {
        let now = Utc::now().timestamp_millis();
        
        sqlx::query(
            r#"UPDATE workers SET last_heartbeat_ts = $2, status = 'running' WHERE worker_id = $1"#
        )
        .bind(worker_id)
        .bind(now)
        .execute(&*self.pool)
        .await?;

        Ok(())
    }

    pub async fn unregister_worker(&self, worker_id: &str) -> Result<(), sqlx::Error> {
        let now = Utc::now().timestamp_millis();
        
        sqlx::query(
            r#"UPDATE workers SET status = 'stopped', stopped_ts = $2 WHERE worker_id = $1"#
        )
        .bind(worker_id)
        .bind(now)
        .execute(&*self.pool)
        .await?;

        Ok(())
    }

    pub async fn create_command(&self, request: SendCommandRequest) -> Result<WorkerCommand, sqlx::Error> {
        let now = Utc::now().timestamp_millis();
        let command_id = uuid::Uuid::new_v4().to_string();

        let row = sqlx::query_as::<_, WorkerCommandRow>(
            r#"
            INSERT INTO worker_commands (
                command_id, target_worker_id, command_type, command_data, priority, status, created_ts, max_retries
            )
            VALUES ($1, $2, $3, $4, $5, 'pending', $6, $7)
            RETURNING *
            "#,
        )
        .bind(&command_id)
        .bind(&request.target_worker_id)
        .bind(&request.command_type)
        .bind(&request.command_data)
        .bind(request.priority.unwrap_or(0))
        .bind(now)
        .bind(request.max_retries.unwrap_or(3))
        .fetch_one(&*self.pool)
        .await?;

        Ok(row.into())
    }

    pub async fn get_pending_commands(&self, worker_id: &str, limit: i64) -> Result<Vec<WorkerCommand>, sqlx::Error> {
        let rows = sqlx::query_as::<_, WorkerCommandRow>(
            r#"
            SELECT * FROM worker_commands 
            WHERE target_worker_id = $1 AND status = 'pending' 
            ORDER BY priority DESC, created_ts ASC 
            LIMIT $2
            "#,
        )
        .bind(worker_id)
        .bind(limit)
        .fetch_all(&*self.pool)
        .await?;

        Ok(rows.into_iter().map(|r| r.into()).collect())
    }

    pub async fn mark_command_sent(&self, command_id: &str) -> Result<(), sqlx::Error> {
        let now = Utc::now().timestamp_millis();
        
        sqlx::query(
            r#"UPDATE worker_commands SET status = 'sent', sent_ts = $2 WHERE command_id = $1"#
        )
        .bind(command_id)
        .bind(now)
        .execute(&*self.pool)
        .await?;

        Ok(())
    }

    pub async fn complete_command(&self, command_id: &str) -> Result<(), sqlx::Error> {
        let now = Utc::now().timestamp_millis();
        
        sqlx::query(
            r#"UPDATE worker_commands SET status = 'completed', completed_ts = $2 WHERE command_id = $1"#
        )
        .bind(command_id)
        .bind(now)
        .execute(&*self.pool)
        .await?;

        Ok(())
    }

    pub async fn fail_command(&self, command_id: &str, error: &str) -> Result<(), sqlx::Error> {
        let now = Utc::now().timestamp_millis();
        
        sqlx::query(
            r#"
            UPDATE worker_commands SET 
                status = CASE WHEN retry_count >= max_retries THEN 'failed' ELSE 'pending' END,
                retry_count = retry_count + 1,
                error_message = $2,
                completed_ts = CASE WHEN retry_count >= max_retries THEN $3 ELSE NULL END
            WHERE command_id = $1
            "#,
        )
        .bind(command_id)
        .bind(error)
        .bind(now)
        .execute(&*self.pool)
        .await?;

        Ok(())
    }

    pub async fn add_event(
        &self,
        event_id: &str,
        event_type: &str,
        room_id: Option<&str>,
        sender: Option<&str>,
        event_data: serde_json::Value,
    ) -> Result<WorkerEvent, sqlx::Error> {
        let now = Utc::now().timestamp_millis();
        
        let stream_id: i64 = sqlx::query("SELECT nextval('worker_event_stream_id_seq')")
            .fetch_one(&*self.pool)
            .await?
            .get(0);

        let row = sqlx::query_as::<_, WorkerEventRow>(
            r#"
            INSERT INTO worker_events (
                event_id, stream_id, event_type, room_id, sender, event_data, created_ts
            )
            VALUES ($1, $2, $3, $4, $5, $6, $7)
            RETURNING *
            "#,
        )
        .bind(event_id)
        .bind(stream_id)
        .bind(event_type)
        .bind(room_id)
        .bind(sender)
        .bind(&event_data)
        .bind(now)
        .fetch_one(&*self.pool)
        .await?;

        Ok(row.into())
    }

    pub async fn get_events_since(&self, stream_id: i64, limit: i64) -> Result<Vec<WorkerEvent>, sqlx::Error> {
        let rows = sqlx::query_as::<_, WorkerEventRow>(
            r#"SELECT * FROM worker_events WHERE stream_id > $1 ORDER BY stream_id ASC LIMIT $2"#
        )
        .bind(stream_id)
        .bind(limit)
        .fetch_all(&*self.pool)
        .await?;

        Ok(rows.into_iter().map(|r| r.into()).collect())
    }

    pub async fn mark_event_processed(&self, event_id: &str, worker_id: &str) -> Result<(), sqlx::Error> {
        sqlx::query(
            r#"
            UPDATE worker_events SET processed_by = array_append(COALESCE(processed_by, '{}'), $2)
            WHERE event_id = $1
            "#,
        )
        .bind(event_id)
        .bind(worker_id)
        .execute(&*self.pool)
        .await?;

        Ok(())
    }

    pub async fn update_replication_position(
        &self,
        worker_id: &str,
        stream_name: &str,
        position: i64,
    ) -> Result<(), sqlx::Error> {
        let now = Utc::now().timestamp_millis();
        
        sqlx::query(
            r#"
            INSERT INTO replication_positions (worker_id, stream_name, stream_position, updated_ts)
            VALUES ($1, $2, $3, $4)
            ON CONFLICT (worker_id, stream_name) DO UPDATE SET
                stream_position = EXCLUDED.stream_position,
                updated_ts = EXCLUDED.updated_ts
            "#,
        )
        .bind(worker_id)
        .bind(stream_name)
        .bind(position)
        .bind(now)
        .execute(&*self.pool)
        .await?;

        Ok(())
    }

    pub async fn get_replication_position(&self, worker_id: &str, stream_name: &str) -> Result<Option<i64>, sqlx::Error> {
        let result = sqlx::query(
            r#"SELECT stream_position FROM replication_positions WHERE worker_id = $1 AND stream_name = $2"#
        )
        .bind(worker_id)
        .bind(stream_name)
        .fetch_optional(&*self.pool)
        .await?;

        Ok(result.map(|row| row.get("stream_position")))
    }

    pub async fn record_load_stats(&self, worker_id: &str, stats: &WorkerLoadStatsUpdate) -> Result<(), sqlx::Error> {
        let now = Utc::now().timestamp_millis();
        
        sqlx::query(
            r#"
            INSERT INTO worker_load_stats (
                worker_id, cpu_usage, memory_usage, active_connections, 
                requests_per_second, average_latency_ms, queue_depth, recorded_ts
            )
            VALUES ($1, $2, $3, $4, $5, $6, $7, $8)
            "#,
        )
        .bind(worker_id)
        .bind(stats.cpu_usage)
        .bind(stats.memory_usage)
        .bind(stats.active_connections)
        .bind(stats.requests_per_second)
        .bind(stats.average_latency_ms)
        .bind(stats.queue_depth)
        .bind(now)
        .execute(&*self.pool)
        .await?;

        Ok(())
    }

    pub async fn assign_task(&self, request: AssignTaskRequest) -> Result<WorkerTaskAssignment, sqlx::Error> {
        let now = Utc::now().timestamp_millis();
        let task_id = uuid::Uuid::new_v4().to_string();

        let row = sqlx::query_as::<_, WorkerTaskAssignment>(
            r#"
            INSERT INTO worker_task_assignments (
                task_id, task_type, task_data, priority, status, created_ts
            )
            VALUES ($1, $2, $3, $4, 'pending', $5)
            RETURNING *
            "#,
        )
        .bind(&task_id)
        .bind(&request.task_type)
        .bind(&request.task_data)
        .bind(request.priority.unwrap_or(0))
        .bind(now)
        .fetch_one(&*self.pool)
        .await?;

        Ok(row)
    }

    pub async fn get_pending_tasks(&self, limit: i64) -> Result<Vec<WorkerTaskAssignment>, sqlx::Error> {
        let rows = sqlx::query_as::<_, WorkerTaskAssignment>(
            r#"
            SELECT * FROM worker_task_assignments 
            WHERE status = 'pending' 
            ORDER BY priority DESC, created_ts ASC 
            LIMIT $1
            "#,
        )
        .bind(limit)
        .fetch_all(&*self.pool)
        .await?;

        Ok(rows)
    }

    pub async fn assign_task_to_worker(&self, task_id: &str, worker_id: &str) -> Result<(), sqlx::Error> {
        let now = Utc::now().timestamp_millis();
        
        sqlx::query(
            r#"UPDATE worker_task_assignments SET assigned_worker_id = $2, assigned_ts = $3, status = 'running' WHERE task_id = $1"#
        )
        .bind(task_id)
        .bind(worker_id)
        .bind(now)
        .execute(&*self.pool)
        .await?;

        Ok(())
    }

    pub async fn complete_task(&self, task_id: &str, result: Option<serde_json::Value>) -> Result<(), sqlx::Error> {
        let now = Utc::now().timestamp_millis();
        
        sqlx::query(
            r#"UPDATE worker_task_assignments SET status = 'completed', completed_ts = $2, result = $3 WHERE task_id = $1"#
        )
        .bind(task_id)
        .bind(now)
        .bind(result)
        .execute(&*self.pool)
        .await?;

        Ok(())
    }

    pub async fn fail_task(&self, task_id: &str, error: &str) -> Result<(), sqlx::Error> {
        let now = Utc::now().timestamp_millis();
        
        sqlx::query(
            r#"UPDATE worker_task_assignments SET status = 'failed', completed_ts = $2, error_message = $3 WHERE task_id = $1"#
        )
        .bind(task_id)
        .bind(now)
        .bind(error)
        .execute(&*self.pool)
        .await?;

        Ok(())
    }

    pub async fn record_connection(
        &self,
        source_worker_id: &str,
        target_worker_id: &str,
        connection_type: &str,
    ) -> Result<(), sqlx::Error> {
        let now = Utc::now().timestamp_millis();
        
        sqlx::query(
            r#"
            INSERT INTO worker_connections (
                source_worker_id, target_worker_id, connection_type, status, established_ts
            )
            VALUES ($1, $2, $3, 'connected', $4)
            ON CONFLICT (source_worker_id, target_worker_id, connection_type) DO UPDATE SET
                status = 'connected',
                last_activity_ts = EXCLUDED.established_ts
            "#,
        )
        .bind(source_worker_id)
        .bind(target_worker_id)
        .bind(connection_type)
        .bind(now)
        .execute(&*self.pool)
        .await?;

        Ok(())
    }

    pub async fn update_connection_stats(
        &self,
        request: &UpdateConnectionStatsRequest,
    ) -> Result<(), sqlx::Error> {
        let now = Utc::now().timestamp_millis();
        
        sqlx::query(
            r#"
            UPDATE worker_connections SET
                last_activity_ts = $5,
                bytes_sent = bytes_sent + $6,
                bytes_received = bytes_received + $7,
                messages_sent = messages_sent + $8,
                messages_received = messages_received + $9
            WHERE source_worker_id = $1 AND target_worker_id = $2 AND connection_type = $3
            "#,
        )
        .bind(&request.source_worker_id)
        .bind(&request.target_worker_id)
        .bind(&request.connection_type)
        .bind(now)
        .bind(request.bytes_sent)
        .bind(request.bytes_received)
        .bind(request.messages_sent)
        .bind(request.messages_received)
        .execute(&*self.pool)
        .await?;

        Ok(())
    }

    pub async fn get_statistics(&self) -> Result<Vec<serde_json::Value>, sqlx::Error> {
        let rows = sqlx::query(r#"SELECT * FROM worker_statistics"#)
            .fetch_all(&*self.pool)
            .await?;

        Ok(rows.into_iter().map(|row| {
            serde_json::json!({
                "id": row.get::<i64, _>("id"),
                "worker_id": row.get::<String, _>("worker_id"),
                "worker_name": row.get::<String, _>("worker_name"),
                "worker_type": row.get::<String, _>("worker_type"),
                "status": row.get::<String, _>("status"),
                "host": row.get::<String, _>("host"),
                "port": row.get::<i32, _>("port"),
                "last_heartbeat_ts": row.get::<Option<i64>, _>("last_heartbeat_ts"),
                "started_ts": row.get::<i64, _>("started_ts"),
                "cpu_usage": row.get::<Option<f32>, _>("cpu_usage"),
                "memory_usage": row.get::<Option<i64>, _>("memory_usage"),
                "active_connections": row.get::<Option<i32>, _>("active_connections"),
                "requests_per_second": row.get::<Option<f32>, _>("requests_per_second"),
                "average_latency_ms": row.get::<Option<f32>, _>("average_latency_ms"),
                "queue_depth": row.get::<Option<i32>, _>("queue_depth"),
                "pending_commands": row.get::<i64, _>("pending_commands"),
                "active_tasks": row.get::<i64, _>("active_tasks"),
            })
        }).collect())
    }

    pub async fn get_type_statistics(&self) -> Result<Vec<serde_json::Value>, sqlx::Error> {
        let rows = sqlx::query(r#"SELECT * FROM worker_type_statistics"#)
            .fetch_all(&*self.pool)
            .await?;

        Ok(rows.into_iter().map(|row| {
            serde_json::json!({
                "worker_type": row.get::<String, _>("worker_type"),
                "total_count": row.get::<i64, _>("total_count"),
                "running_count": row.get::<i64, _>("running_count"),
                "starting_count": row.get::<i64, _>("starting_count"),
                "stopping_count": row.get::<i64, _>("stopping_count"),
                "stopped_count": row.get::<i64, _>("stopped_count"),
                "avg_cpu_usage": row.get::<Option<f64>, _>("avg_cpu_usage"),
                "avg_memory_usage": row.get::<Option<f64>, _>("avg_memory_usage"),
                "total_connections": row.get::<Option<i64>, _>("total_connections"),
            })
        }).collect())
    }
}
