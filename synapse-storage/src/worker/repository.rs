use super::models::*;
use chrono::Utc;
use sqlx::PgPool;
use std::sync::Arc;

#[derive(Clone)]
pub struct WorkerStorage {
    pool: Arc<PgPool>,
}

impl WorkerStorage {
    pub(crate) fn status_releases_in_flight_work(status: &str) -> bool {
        matches!(status, "stopped" | "error")
    }

    pub fn new(pool: &Arc<PgPool>) -> Self {
        Self { pool: pool.clone() }
    }

    pub async fn register_worker(&self, request: RegisterWorkerRequest) -> Result<WorkerInfo, sqlx::Error> {
        let now = Utc::now().timestamp_millis();
        let config = request.config.unwrap_or(serde_json::json!({}));
        let metadata = request.metadata.unwrap_or(serde_json::json!({}));

        let row: WorkerRow = sqlx::query_as::<_, WorkerRow>(
            r#"
            INSERT INTO workers (
                worker_id, worker_name, worker_type, host, port, status, started_ts, config, metadata, version
            )
            VALUES ($1, $2, $3, $4, $5, 'starting', $6, $7, $8, $9)
            RETURNING id, worker_id, worker_name,
                      worker_type, host, port,
                      status, last_heartbeat_ts,
                      started_ts, stopped_ts,
                      COALESCE(config, '{}'::jsonb) as config,
                      COALESCE(metadata, '{}'::jsonb) as metadata,
                      version
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
        .bind(request.version.as_deref())
        .fetch_one(&*self.pool)
        .await?;

        Ok(row.into())
    }

    pub async fn get_worker(&self, worker_id: &str) -> Result<Option<WorkerInfo>, sqlx::Error> {
        let row: Option<WorkerRow> = sqlx::query_as::<_, WorkerRow>(
            r#"SELECT id, worker_id, worker_name,
                      worker_type, host, port,
                      status, last_heartbeat_ts,
                      started_ts, stopped_ts,
                      COALESCE(config, '{}'::jsonb) as config,
                      COALESCE(metadata, '{}'::jsonb) as metadata,
                      version
               FROM workers WHERE worker_id = $1"#,
        )
        .bind(worker_id)
        .fetch_optional(&*self.pool)
        .await?;

        Ok(row.map(|r| r.into()))
    }

    pub async fn get_workers_by_type(&self, worker_type: &str) -> Result<Vec<WorkerInfo>, sqlx::Error> {
        let rows: Vec<WorkerRow> = sqlx::query_as::<_, WorkerRow>(
            r#"SELECT id, worker_id, worker_name,
                      worker_type, host, port,
                      status, last_heartbeat_ts,
                      started_ts, stopped_ts,
                      COALESCE(config, '{}'::jsonb) as config,
                      COALESCE(metadata, '{}'::jsonb) as metadata,
                      version
               FROM workers WHERE worker_type = $1 ORDER BY started_ts DESC"#,
        )
        .bind(worker_type)
        .fetch_all(&*self.pool)
        .await?;

        Ok(rows.into_iter().map(|r| r.into()).collect())
    }

    pub async fn get_active_workers(&self) -> Result<Vec<WorkerInfo>, sqlx::Error> {
        let rows: Vec<WorkerRow> = sqlx::query_as::<_, WorkerRow>(
            r#"
            SELECT id, worker_id, worker_name,
                   worker_type, host, port,
                   status, last_heartbeat_ts,
                   started_ts, stopped_ts,
                   COALESCE(config, '{}'::jsonb) as config,
                   COALESCE(metadata, '{}'::jsonb) as metadata,
                   version
            FROM workers
            WHERE status IN ('running', 'starting')
            ORDER BY started_ts DESC
            "#,
        )
        .fetch_all(&*self.pool)
        .await?;

        Ok(rows.into_iter().map(|r| r.into()).collect())
    }

    pub async fn update_worker_status(&self, worker_id: &str, status: &str) -> Result<(), sqlx::Error> {
        let now = Utc::now().timestamp_millis();
        let mut tx = self.pool.begin().await?;

        if Self::status_releases_in_flight_work(status) {
            sqlx::query(
                r"
                UPDATE worker_task_assignments
                SET status = 'pending',
                    assigned_worker_id = NULL,
                    assigned_ts = NULL
                WHERE assigned_worker_id = $1
                  AND status IN ('pending', 'running')
                ",
            )
            .bind(worker_id)
            .execute(&mut *tx)
            .await?;
        }

        sqlx::query(
            r"
            UPDATE workers
            SET status = $2,
                last_heartbeat_ts = $3,
                stopped_ts = CASE WHEN $2 IN ('stopped', 'error') THEN $3 ELSE NULL END
            WHERE worker_id = $1
            ",
        )
        .bind(worker_id)
        .bind(status)
        .bind(now)
        .execute(&mut *tx)
        .await?;

        tx.commit().await?;

        Ok(())
    }

    pub async fn update_heartbeat(&self, worker_id: &str) -> Result<(), sqlx::Error> {
        let now = Utc::now().timestamp_millis();

        sqlx::query(r"UPDATE workers SET last_heartbeat_ts = $2, status = 'running' WHERE worker_id = $1")
            .bind(worker_id)
            .bind(now)
            .execute(&*self.pool)
            .await?;

        Ok(())
    }

    pub async fn unregister_worker(&self, worker_id: &str) -> Result<(), sqlx::Error> {
        let now = Utc::now().timestamp_millis();
        let mut tx = self.pool.begin().await?;

        sqlx::query(
            r"
            UPDATE worker_task_assignments
            SET status = 'pending',
                assigned_worker_id = NULL,
                assigned_ts = NULL
            WHERE assigned_worker_id = $1
              AND status IN ('pending', 'running')
            ",
        )
        .bind(worker_id)
        .execute(&mut *tx)
        .await?;

        sqlx::query(r"UPDATE workers SET status = 'stopped', stopped_ts = $2 WHERE worker_id = $1")
            .bind(worker_id)
            .bind(now)
            .execute(&mut *tx)
            .await?;

        tx.commit().await?;

        Ok(())
    }

    pub async fn create_command(&self, request: SendCommandRequest) -> Result<WorkerCommand, sqlx::Error> {
        let now = Utc::now().timestamp_millis();
        let command_id = uuid::Uuid::new_v4().to_string();

        let row: WorkerCommandRow = sqlx::query_as::<_, WorkerCommandRow>(
            r#"
            INSERT INTO worker_commands (
                command_id, target_worker_id, command_type, command_data, priority, status, created_ts, max_retries
            )
            VALUES ($1, $2, $3, $4, $5, 'pending', $6, $7)
            RETURNING id, command_id, target_worker_id,
                      source_worker_id, command_type,
                      COALESCE(command_data, '{}'::jsonb) as command_data,
                      priority, status, created_ts,
                      sent_ts, completed_ts,
                      error_message, retry_count, max_retries
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
        let rows: Vec<WorkerCommandRow> = sqlx::query_as::<_, WorkerCommandRow>(
            r#"
            SELECT id, command_id, target_worker_id,
                      source_worker_id, command_type,
                      COALESCE(command_data, '{}'::jsonb) as command_data,
                      priority, status, created_ts,
                      sent_ts, completed_ts,
                      error_message, retry_count, max_retries
            FROM worker_commands
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

        sqlx::query(r"UPDATE worker_commands SET status = 'sent', sent_ts = $2 WHERE command_id = $1")
            .bind(command_id)
            .bind(now)
            .execute(&*self.pool)
            .await?;

        Ok(())
    }

    pub async fn complete_command(&self, command_id: &str) -> Result<(), sqlx::Error> {
        let now = Utc::now().timestamp_millis();

        sqlx::query(r"UPDATE worker_commands SET status = 'completed', completed_ts = $2 WHERE command_id = $1")
            .bind(command_id)
            .bind(now)
            .execute(&*self.pool)
            .await?;

        Ok(())
    }

    pub async fn fail_command(&self, command_id: &str, error: &str) -> Result<(), sqlx::Error> {
        let now = Utc::now().timestamp_millis();

        sqlx::query(
            r"
            UPDATE worker_commands SET
                status = CASE WHEN retry_count >= max_retries THEN 'failed' ELSE 'pending' END,
                retry_count = retry_count + 1,
                error_message = $2,
                completed_ts = CASE WHEN retry_count >= max_retries THEN $3::BIGINT ELSE NULL END
            WHERE command_id = $1
            ",
        )
        .bind(command_id)
        .bind(error)
        .bind(Some(now))
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

        let row = sqlx::query_as::<_, WorkerEventRow>(
            r"
            INSERT INTO worker_events (
                event_id, event_type, room_id, sender, event_data, created_ts
            )
            VALUES ($1, $2, $3, $4, $5, $6)
            RETURNING id, event_id, stream_id, event_type, room_id,
                      sender, event_data, created_ts, processed_by
            ",
        )
        .bind(event_id)
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
            r"SELECT id, event_id, stream_id, event_type, room_id,
                      sender, event_data, created_ts, processed_by
               FROM worker_events WHERE stream_id > $1 ORDER BY stream_id ASC LIMIT $2",
        )
        .bind(stream_id)
        .bind(limit)
        .fetch_all(&*self.pool)
        .await?;

        Ok(rows.into_iter().map(|r| r.into()).collect())
    }

    pub async fn mark_event_processed(&self, event_id: &str, worker_id: &str) -> Result<(), sqlx::Error> {
        // processed_by is JSONB (a JSON array of worker ids). Use JSONB array
        // concatenation instead of PostgreSQL array_append, which only works on
        // native array columns.
        sqlx::query(
            r"
            UPDATE worker_events
            SET processed_by = COALESCE(processed_by, '[]'::jsonb) || jsonb_build_array($2::text)
            WHERE event_id = $1
            ",
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
            r"
            INSERT INTO replication_positions (worker_id, stream_name, stream_position, updated_ts)
            VALUES ($1, $2, $3, $4)
            ON CONFLICT (worker_id, stream_name) DO UPDATE SET
                stream_position = EXCLUDED.stream_position,
                updated_ts = EXCLUDED.updated_ts
            ",
        )
        .bind(worker_id)
        .bind(stream_name)
        .bind(position)
        .bind(now)
        .execute(&*self.pool)
        .await?;

        Ok(())
    }

    pub async fn get_replication_position(
        &self,
        worker_id: &str,
        stream_name: &str,
    ) -> Result<Option<i64>, sqlx::Error> {
        let result = sqlx::query_scalar::<_, i64>(
            r#"SELECT stream_position FROM replication_positions WHERE worker_id = $1 AND stream_name = $2"#,
        )
        .bind(worker_id)
        .bind(stream_name)
        .fetch_optional(&*self.pool)
        .await?;

        Ok(result)
    }

    pub fn record_load_stats(&self, worker_id: &str, stats: &WorkerLoadStatsUpdate) -> Result<(), sqlx::Error> {
        tracing::debug!(
            worker_id = worker_id,
            cpu = ?stats.cpu_usage,
            memory = ?stats.memory_usage,
            connections = ?stats.active_connections,
            rps = ?stats.requests_per_second,
            latency_ms = ?stats.average_latency_ms,
            queue = ?stats.queue_depth,
            "worker load stats"
        );
        Ok(())
    }

    pub async fn assign_task(&self, request: AssignTaskRequest) -> Result<WorkerTaskAssignment, sqlx::Error> {
        let now = Utc::now().timestamp_millis();
        let task_id = uuid::Uuid::new_v4().to_string();

        let row: WorkerTaskAssignment = sqlx::query_as::<_, WorkerTaskAssignment>(
            r#"
            INSERT INTO worker_task_assignments (
                task_id, task_type, task_data, priority, status, created_ts
            )
            VALUES ($1, $2, $3, $4, 'pending', $5)
            RETURNING id, task_id, task_type,
                      COALESCE(task_data, '{}'::jsonb) as task_data,
                      assigned_worker_id,
                      status, priority,
                      created_ts, assigned_ts,
                      completed_ts, result,
                      error_message
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
        let rows: Vec<WorkerTaskAssignment> = sqlx::query_as::<_, WorkerTaskAssignment>(
            r#"
            SELECT id, task_id, task_type,
                      COALESCE(task_data, '{}'::jsonb) as task_data,
                      assigned_worker_id,
                      status, priority,
                      created_ts, assigned_ts,
                      completed_ts, result,
                      error_message
            FROM worker_task_assignments
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

    pub async fn claim_next_pending_task(&self, worker_id: &str) -> Result<Option<WorkerTaskAssignment>, sqlx::Error> {
        let now = Utc::now().timestamp_millis();

        sqlx::query_as::<_, WorkerTaskAssignment>(
            r#"
            UPDATE worker_task_assignments
            SET assigned_worker_id = $1, assigned_ts = $2, status = 'running'
            WHERE id = (
                SELECT id
                FROM worker_task_assignments
                WHERE status = 'pending'
                  AND assigned_worker_id IS NULL
                ORDER BY priority DESC, created_ts ASC
                LIMIT 1
                FOR UPDATE SKIP LOCKED
            )
            RETURNING id, task_id, task_type,
                      COALESCE(task_data, '{}'::jsonb) as task_data,
                      assigned_worker_id,
                      status, priority,
                      created_ts, assigned_ts,
                      completed_ts, result,
                      error_message
            "#,
        )
        .bind(worker_id)
        .bind(now)
        .fetch_optional(&*self.pool)
        .await
    }

    pub async fn claim_next_pending_task_for_types(
        &self,
        worker_id: &str,
        allowed_task_types: &[String],
    ) -> Result<Option<WorkerTaskAssignment>, sqlx::Error> {
        let now = Utc::now().timestamp_millis();

        sqlx::query_as::<_, WorkerTaskAssignment>(
            r#"
            UPDATE worker_task_assignments
            SET assigned_worker_id = $1, assigned_ts = $2, status = 'running'
            WHERE id = (
                SELECT id
                FROM worker_task_assignments
                WHERE status = 'pending'
                  AND assigned_worker_id IS NULL
                  AND task_type = ANY($3)
                ORDER BY priority DESC, created_ts ASC
                LIMIT 1
                FOR UPDATE SKIP LOCKED
            )
            RETURNING id, task_id, task_type,
                      COALESCE(task_data, '{}'::jsonb) as task_data,
                      assigned_worker_id,
                      status, priority,
                      created_ts, assigned_ts,
                      completed_ts, result,
                      error_message
            "#,
        )
        .bind(worker_id)
        .bind(now)
        .bind(allowed_task_types)
        .fetch_optional(&*self.pool)
        .await
    }

    pub async fn assign_task_to_worker(&self, task_id: &str, worker_id: &str) -> Result<bool, sqlx::Error> {
        let now = Utc::now().timestamp_millis();

        let result: sqlx::postgres::PgQueryResult = sqlx::query(
            r"
            UPDATE worker_task_assignments
            SET assigned_worker_id = $2, assigned_ts = $3, status = 'running'
            WHERE task_id = $1
              AND status = 'pending'
              AND assigned_worker_id IS NULL
            ",
        )
        .bind(task_id)
        .bind(worker_id)
        .bind(now)
        .execute(&*self.pool)
        .await?;

        Ok(result.rows_affected() == 1)
    }

    pub async fn complete_task(&self, task_id: &str, result: Option<serde_json::Value>) -> Result<(), sqlx::Error> {
        let now = Utc::now().timestamp_millis();

        sqlx::query(
            r"UPDATE worker_task_assignments SET status = 'completed', completed_ts = $2, result = $3 WHERE task_id = $1",
        )
        .bind(task_id)
        .bind(now)
        .bind(result.as_ref())
        .execute(&*self.pool)
        .await?;

        Ok(())
    }

    pub async fn fail_task(&self, task_id: &str, error: &str) -> Result<(), sqlx::Error> {
        let now = Utc::now().timestamp_millis();

        sqlx::query(
            r"UPDATE worker_task_assignments SET status = 'failed', completed_ts = $2, error_message = $3 WHERE task_id = $1",
        )
        .bind(task_id)
        .bind(now)
        .bind(error)
        .execute(&*self.pool)
        .await?;

        Ok(())
    }

    pub fn record_connection(
        &self,
        source_worker_id: &str,
        target_worker_id: &str,
        connection_type: &str,
    ) -> Result<(), sqlx::Error> {
        tracing::info!(
            source = source_worker_id,
            target = target_worker_id,
            conn_type = connection_type,
            "worker connection established"
        );
        Ok(())
    }

    pub fn update_connection_stats(&self, request: &UpdateConnectionStatsRequest) -> Result<(), sqlx::Error> {
        tracing::debug!(
            source = %request.source_worker_id,
            target = %request.target_worker_id,
            conn_type = %request.connection_type,
            bytes_sent = request.bytes_sent,
            bytes_received = request.bytes_received,
            "worker connection stats"
        );
        Ok(())
    }

    pub async fn get_statistics(&self, limit: i64) -> Result<Vec<serde_json::Value>, sqlx::Error> {
        let rows = sqlx::query(
            r"SELECT id, worker_id, worker_name, worker_type, status,
                      host, port, last_heartbeat_ts, started_ts,
                      cpu_usage, memory_usage, active_connections,
                      requests_per_second, average_latency_ms,
                      queue_depth, pending_commands, active_tasks
               FROM worker_statistics
               ORDER BY id DESC
               LIMIT $1",
        )
        .bind(limit)
        .fetch_all(&*self.pool)
        .await?;

        Ok(rows
            .into_iter()
            .map(|row| {
                use sqlx::Row;
                serde_json::json!({
                    "id": row.get::<i64, _>("id"),
                    "worker_id": row.get::<String, _>("worker_id"),
                    "worker_name": row.get::<Option<String>, _>("worker_name"),
                    "worker_type": row.get::<Option<String>, _>("worker_type"),
                    "status": row.get::<Option<String>, _>("status"),
                    "host": row.get::<Option<String>, _>("host"),
                    "port": row.get::<Option<i32>, _>("port"),
                    "last_heartbeat_ts": row.get::<Option<i64>, _>("last_heartbeat_ts"),
                    "started_ts": row.get::<Option<i64>, _>("started_ts"),
                    "cpu_usage": row.get::<Option<f64>, _>("cpu_usage"),
                    "memory_usage": row.get::<Option<f64>, _>("memory_usage"),
                    "active_connections": row.get::<Option<i32>, _>("active_connections"),
                    "requests_per_second": row.get::<Option<f64>, _>("requests_per_second"),
                    "average_latency_ms": row.get::<Option<f64>, _>("average_latency_ms"),
                    "queue_depth": row.get::<Option<i32>, _>("queue_depth"),
                    "pending_commands": row.get::<Option<i32>, _>("pending_commands"),
                    "active_tasks": row.get::<Option<i32>, _>("active_tasks"),
                })
            })
            .collect())
    }

    pub async fn get_type_statistics(&self) -> Result<Vec<serde_json::Value>, sqlx::Error> {
        let rows = sqlx::query(
            r"
            SELECT worker_type, total_count, running_count, starting_count,
                   stopping_count, stopped_count, avg_cpu_usage, avg_memory_usage,
                   total_connections
            FROM worker_type_statistics
            ",
        )
        .fetch_all(&*self.pool)
        .await?;

        Ok(rows
            .into_iter()
            .map(|row| {
                use sqlx::Row;
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
            })
            .collect())
    }
}
