use super::models::*;
use sqlx::PgPool;
use std::sync::Arc;
use synapse_common::current_timestamp_millis;

/// The `WorkerStorage` struct.
#[derive(Clone)]
pub struct WorkerStorage {
    pool: Arc<PgPool>,
}

impl WorkerStorage {
    /// See [`status_releases_in_flight_work`].
    pub(crate) fn status_releases_in_flight_work(status: &str) -> bool {
        matches!(status, "stopped" | "error")
    }

    /// See [`new`].
    pub fn new(pool: &Arc<PgPool>) -> Self {
        Self { pool: pool.clone() }
    }

    /// See [`register_worker`].
    pub async fn register_worker(&self, request: RegisterWorkerRequest) -> Result<WorkerInfo, sqlx::Error> {
        let now = current_timestamp_millis();
        let config = request.config.unwrap_or(serde_json::json!({}));
        let metadata = request.metadata.unwrap_or(serde_json::json!({}));

        let row: WorkerRow = sqlx::query_as!(
            WorkerRow,
            r#"
            INSERT INTO workers (
                worker_id, worker_name, worker_type, host, port, status, started_ts, config, metadata, version
            )
            VALUES ($1, $2, $3, $4, $5, 'starting', $6, $7, $8, $9)
            RETURNING id, worker_id, worker_name,
                      worker_type, host, port,
                      status, last_heartbeat_ts,
                      started_ts, stopped_ts,
                      COALESCE(config, '{}'::jsonb) AS "config!",
                      COALESCE(metadata, '{}'::jsonb) AS "metadata!",
                      version
            "#,
            &request.worker_id,
            &request.worker_name,
            request.worker_type.as_str(),
            &request.host,
            request.port as i32,
            now,
            &config,
            &metadata,
            request.version.as_deref()
        )
        .fetch_one(&*self.pool)
        .await?;

        Ok(row.into())
    }

    /// See [`get_worker`].
    pub async fn get_worker(&self, worker_id: &str) -> Result<Option<WorkerInfo>, sqlx::Error> {
        let row: Option<WorkerRow> = sqlx::query_as!(
            WorkerRow,
            r#"SELECT id, worker_id, worker_name,
                      worker_type, host, port,
                      status, last_heartbeat_ts,
                      started_ts, stopped_ts,
                      COALESCE(config, '{}'::jsonb) AS "config!",
                      COALESCE(metadata, '{}'::jsonb) AS "metadata!",
                      version
               FROM workers WHERE worker_id = $1"#,
            worker_id
        )
        .fetch_optional(&*self.pool)
        .await?;

        Ok(row.map(|r| r.into()))
    }

    /// See [`get_workers_by_type`].
    pub async fn get_workers_by_type(&self, worker_type: &str) -> Result<Vec<WorkerInfo>, sqlx::Error> {
        let rows: Vec<WorkerRow> = sqlx::query_as!(
            WorkerRow,
            r#"SELECT id, worker_id, worker_name,
                      worker_type, host, port,
                      status, last_heartbeat_ts,
                      started_ts, stopped_ts,
                      COALESCE(config, '{}'::jsonb) AS "config!",
                      COALESCE(metadata, '{}'::jsonb) AS "metadata!",
                      version
               FROM workers WHERE worker_type = $1 ORDER BY started_ts DESC"#,
            worker_type
        )
        .fetch_all(&*self.pool)
        .await?;

        Ok(rows.into_iter().map(|r| r.into()).collect())
    }

    /// See [`get_active_workers`].
    pub async fn get_active_workers(&self) -> Result<Vec<WorkerInfo>, sqlx::Error> {
        let rows: Vec<WorkerRow> = sqlx::query_as!(
            WorkerRow,
            r#"
            SELECT id, worker_id, worker_name,
                   worker_type, host, port,
                   status, last_heartbeat_ts,
                   started_ts, stopped_ts,
                   COALESCE(config, '{}'::jsonb) AS "config!",
                   COALESCE(metadata, '{}'::jsonb) AS "metadata!",
                   version
            FROM workers
            WHERE status IN ('running', 'starting')
            ORDER BY started_ts DESC
            "#
        )
        .fetch_all(&*self.pool)
        .await?;

        Ok(rows.into_iter().map(|r| r.into()).collect())
    }

    /// See [`update_worker_status`].
    pub async fn update_worker_status(&self, worker_id: &str, status: &str) -> Result<(), sqlx::Error> {
        let now = current_timestamp_millis();
        let mut tx = self.pool.begin().await?;

        if Self::status_releases_in_flight_work(status) {
            sqlx::query!(
                r"
                UPDATE worker_task_assignments
                SET status = 'pending',
                    assigned_worker_id = NULL,
                    assigned_ts = NULL
                WHERE assigned_worker_id = $1
                  AND status IN ('pending', 'running')
                ",
                worker_id
            )
            .execute(&mut *tx)
            .await?;
        }

        sqlx::query!(
            r#"
            UPDATE workers
            SET status = $2,
                last_heartbeat_ts = $3,
                stopped_ts = CASE WHEN $2 IN ('stopped', 'error') THEN $3::BIGINT ELSE NULL END
            WHERE worker_id = $1
            "#,
            worker_id,
            status,
            now
        )
        .execute(&mut *tx)
        .await?;

        tx.commit().await?;

        Ok(())
    }

    /// See [`update_heartbeat`].
    pub async fn update_heartbeat(&self, worker_id: &str) -> Result<(), sqlx::Error> {
        let now = current_timestamp_millis();

        sqlx::query!(
            r"UPDATE workers SET last_heartbeat_ts = $2, status = 'running' WHERE worker_id = $1",
            worker_id,
            now
        )
        .execute(&*self.pool)
        .await?;

        Ok(())
    }

    /// See [`unregister_worker`].
    pub async fn unregister_worker(&self, worker_id: &str) -> Result<(), sqlx::Error> {
        let now = current_timestamp_millis();
        let mut tx = self.pool.begin().await?;

        sqlx::query!(
            r"
            UPDATE worker_task_assignments
            SET status = 'pending',
                assigned_worker_id = NULL,
                assigned_ts = NULL
            WHERE assigned_worker_id = $1
              AND status IN ('pending', 'running')
            ",
            worker_id
        )
        .execute(&mut *tx)
        .await?;

        sqlx::query!(r"UPDATE workers SET status = 'stopped', stopped_ts = $2 WHERE worker_id = $1", worker_id, now)
            .execute(&mut *tx)
            .await?;

        tx.commit().await?;

        Ok(())
    }

    /// See [`create_command`].
    pub async fn create_command(&self, request: SendCommandRequest) -> Result<WorkerCommand, sqlx::Error> {
        let now = current_timestamp_millis();
        let command_id = uuid::Uuid::new_v4().simple().to_string();

        let row: WorkerCommandRow = sqlx::query_as!(
            WorkerCommandRow,
            r#"
            INSERT INTO worker_commands (
                command_id, target_worker_id, command_type, command_data, priority, status, created_ts, max_retries
            )
            VALUES ($1, $2, $3, $4, $5, 'pending', $6, $7)
            RETURNING id, command_id, target_worker_id,
                      source_worker_id, command_type,
                      COALESCE(command_data, '{}'::jsonb) AS "command_data!",
                      priority AS "priority!", status, created_ts,
                      sent_ts, completed_ts,
                      error_message, retry_count AS "retry_count!", max_retries AS "max_retries!"
            "#,
            &command_id,
            &request.target_worker_id,
            &request.command_type,
            &request.command_data,
            request.priority.unwrap_or(0),
            now,
            request.max_retries.unwrap_or(3)
        )
        .fetch_one(&*self.pool)
        .await?;

        Ok(row.into())
    }

    /// See [`get_pending_commands`].
    pub async fn get_pending_commands(&self, worker_id: &str, limit: i64) -> Result<Vec<WorkerCommand>, sqlx::Error> {
        let rows: Vec<WorkerCommandRow> = sqlx::query_as!(
            WorkerCommandRow,
            r#"
            SELECT id, command_id, target_worker_id,
                      source_worker_id, command_type,
                      COALESCE(command_data, '{}'::jsonb) AS "command_data!",
                      priority AS "priority!", status, created_ts,
                      sent_ts, completed_ts,
                      error_message, retry_count AS "retry_count!", max_retries AS "max_retries!"
            FROM worker_commands
            WHERE target_worker_id = $1 AND status = 'pending'
            ORDER BY priority DESC, created_ts ASC
            LIMIT $2
            "#,
            worker_id,
            limit
        )
        .fetch_all(&*self.pool)
        .await?;

        Ok(rows.into_iter().map(|r| r.into()).collect())
    }

    /// See [`mark_command_sent`].
    pub async fn mark_command_sent(&self, command_id: &str) -> Result<(), sqlx::Error> {
        let now = current_timestamp_millis();

        sqlx::query!(
            r"UPDATE worker_commands SET status = 'sent', sent_ts = $2 WHERE command_id = $1",
            command_id,
            now
        )
        .execute(&*self.pool)
        .await?;

        Ok(())
    }

    /// See [`complete_command`].
    pub async fn complete_command(&self, command_id: &str) -> Result<(), sqlx::Error> {
        let now = current_timestamp_millis();

        sqlx::query!(
            r"UPDATE worker_commands SET status = 'completed', completed_ts = $2 WHERE command_id = $1",
            command_id,
            now
        )
        .execute(&*self.pool)
        .await?;

        Ok(())
    }

    /// See [`fail_command`].
    pub async fn fail_command(&self, command_id: &str, error: &str) -> Result<(), sqlx::Error> {
        let now = current_timestamp_millis();

        sqlx::query!(
            r#"
            UPDATE worker_commands SET
                status = CASE WHEN retry_count >= max_retries THEN 'failed' ELSE 'pending' END,
                retry_count = retry_count + 1,
                error_message = $2,
                completed_ts = CASE WHEN retry_count >= max_retries THEN $3::BIGINT ELSE NULL END
            WHERE command_id = $1
            "#,
            command_id,
            error,
            Some(now)
        )
        .execute(&*self.pool)
        .await?;

        Ok(())
    }

    /// See [`add_event`].
    pub async fn add_event(
        &self,
        event_id: &str,
        event_type: &str,
        room_id: Option<&str>,
        sender: Option<&str>,
        event_data: serde_json::Value,
    ) -> Result<WorkerEvent, sqlx::Error> {
        let now = current_timestamp_millis();

        let row = sqlx::query_as!(
            WorkerEventRow,
            r#"
            INSERT INTO worker_events (
                event_id, event_type, room_id, sender, event_data, created_ts
            )
            VALUES ($1, $2, $3, $4, $5, $6)
            RETURNING id, event_id, stream_id, event_type, room_id,
                      sender, event_data AS "event_data!", created_ts,
                      processed_by AS "processed_by: sqlx::types::Json<Vec<String>>"
            "#,
            event_id,
            event_type,
            room_id,
            sender,
            &event_data,
            now
        )
        .fetch_one(&*self.pool)
        .await?;

        Ok(row.into())
    }

    /// See [`get_events_since`].
    pub async fn get_events_since(&self, stream_id: i64, limit: i64) -> Result<Vec<WorkerEvent>, sqlx::Error> {
        let rows = sqlx::query_as!(
            WorkerEventRow,
            r#"SELECT id, event_id, stream_id, event_type, room_id,
                      sender, event_data AS "event_data!", created_ts,
                      processed_by AS "processed_by: sqlx::types::Json<Vec<String>>"
               FROM worker_events WHERE stream_id > $1 ORDER BY stream_id ASC LIMIT $2"#,
            stream_id,
            limit
        )
        .fetch_all(&*self.pool)
        .await?;

        Ok(rows.into_iter().map(|r| r.into()).collect())
    }

    /// See [`mark_event_processed`].
    pub async fn mark_event_processed(&self, event_id: &str, worker_id: &str) -> Result<(), sqlx::Error> {
        // processed_by is JSONB (a JSON array of worker ids). Use JSONB array
        // concatenation instead of PostgreSQL array_append, which only works on
        // native array columns.
        sqlx::query!(
            r#"
            UPDATE worker_events
            SET processed_by = COALESCE(processed_by, '[]'::jsonb) || jsonb_build_array($2::text)
            WHERE event_id = $1
            "#,
            event_id,
            worker_id
        )
        .execute(&*self.pool)
        .await?;

        Ok(())
    }

    /// See [`update_replication_position`].
    pub async fn update_replication_position(
        &self,
        worker_id: &str,
        stream_name: &str,
        position: i64,
    ) -> Result<(), sqlx::Error> {
        let now = current_timestamp_millis();

        sqlx::query!(
            r#"
            INSERT INTO replication_positions (worker_id, stream_name, stream_position, updated_ts)
            VALUES ($1, $2, $3, $4)
            ON CONFLICT (worker_id, stream_name) DO UPDATE SET
                stream_position = EXCLUDED.stream_position,
                updated_ts = EXCLUDED.updated_ts
            "#,
            worker_id,
            stream_name,
            position,
            now
        )
        .execute(&*self.pool)
        .await?;

        Ok(())
    }

    /// See [`get_replication_position`].
    pub async fn get_replication_position(
        &self,
        worker_id: &str,
        stream_name: &str,
    ) -> Result<Option<i64>, sqlx::Error> {
        let result = sqlx::query_scalar!(
            r#"SELECT stream_position FROM replication_positions WHERE worker_id = $1 AND stream_name = $2"#,
            worker_id,
            stream_name
        )
        .fetch_optional(&*self.pool)
        .await?;

        Ok(result)
    }

    /// See [`record_load_stats`].
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

    /// Persist the load metrics reported in a heartbeat (`load_stats`).
    ///
    /// Upserts on `worker_id` (the unique index added to the consolidated
    /// baseline) so repeated heartbeats keep exactly **one** row per worker
    /// instead of appending counter duplicates.
    ///
    /// Every metric column is `COALESCE(EXCLUDED.<col>, worker_statistics.<col>)`:
    /// a later heartbeat that reports only a subset of metrics must not NULL out
    /// values reported earlier. `last_heartbeat_ts`/`updated_ts` are always
    /// overwritten. `created_ts` is only used on insert (`$9` twice), so the
    /// original creation time survives updates.
    ///
    /// There is deliberately **no** FK to `workers(worker_id)`: a heartbeat can
    /// arrive before or independently of registration, and a FK would turn that
    /// into a 500. Semantics are "reported ⇒ recorded".
    pub async fn upsert_statistics(
        &self,
        worker_id: &str,
        stats: &WorkerLoadStatsUpdate,
        now: i64,
    ) -> Result<(), sqlx::Error> {
        sqlx::query!(
            r#"
            INSERT INTO worker_statistics (worker_id, cpu_usage, memory_usage, active_connections,
                                           requests_per_second, average_latency_ms, queue_depth,
                                           last_heartbeat_ts, created_ts, updated_ts)
            VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9,$9)
            ON CONFLICT (worker_id) DO UPDATE SET
              cpu_usage          = COALESCE(EXCLUDED.cpu_usage,          worker_statistics.cpu_usage),
              memory_usage       = COALESCE(EXCLUDED.memory_usage,       worker_statistics.memory_usage),
              active_connections = COALESCE(EXCLUDED.active_connections, worker_statistics.active_connections),
              requests_per_second= COALESCE(EXCLUDED.requests_per_second,worker_statistics.requests_per_second),
              average_latency_ms = COALESCE(EXCLUDED.average_latency_ms, worker_statistics.average_latency_ms),
              queue_depth        = COALESCE(EXCLUDED.queue_depth,        worker_statistics.queue_depth),
              last_heartbeat_ts  = EXCLUDED.last_heartbeat_ts,
              updated_ts         = EXCLUDED.updated_ts
            "#,
            worker_id,
            stats.cpu_usage,
            stats.memory_usage,
            stats.active_connections,
            stats.requests_per_second,
            stats.average_latency_ms,
            stats.queue_depth,
            now,
            now
        )
        .execute(&*self.pool)
        .await?;

        Ok(())
    }

    /// See [`assign_task`].
    pub async fn assign_task(&self, request: AssignTaskRequest) -> Result<WorkerTaskAssignment, sqlx::Error> {
        let now = current_timestamp_millis();
        let task_id = uuid::Uuid::new_v4().simple().to_string();

        let row: WorkerTaskAssignment = sqlx::query_as!(
            WorkerTaskAssignment,
            r#"
            INSERT INTO worker_task_assignments (
                task_id, task_type, task_data, priority, status, created_ts
            )
            VALUES ($1, $2, $3, $4, 'pending', $5)
            RETURNING id, task_id, task_type,
                      COALESCE(task_data, '{}'::jsonb) AS "task_data!",
                      assigned_worker_id,
                      status, priority,
                      created_ts, assigned_ts,
                      completed_ts, result,
                      error_message
            "#,
            &task_id,
            &request.task_type,
            &request.task_data,
            request.priority.unwrap_or(0),
            now
        )
        .fetch_one(&*self.pool)
        .await?;

        Ok(row)
    }

    /// See [`get_pending_tasks`].
    pub async fn get_pending_tasks(&self, limit: i64) -> Result<Vec<WorkerTaskAssignment>, sqlx::Error> {
        let rows: Vec<WorkerTaskAssignment> = sqlx::query_as!(
            WorkerTaskAssignment,
            r#"
            SELECT id, task_id, task_type,
                      COALESCE(task_data, '{}'::jsonb) AS "task_data!",
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
            limit
        )
        .fetch_all(&*self.pool)
        .await?;

        Ok(rows)
    }

    /// PERF-05: 按 task_id 直查单个待领任务。
    /// 替代「拉 1000 条 pending 到内存再 find」的模式——任务数超过 1000 时
    /// 旧模式不仅慢，还会错误地报告目标任务不存在。
    pub async fn get_pending_task_by_id(&self, task_id: &str) -> Result<Option<WorkerTaskAssignment>, sqlx::Error> {
        sqlx::query_as!(
            WorkerTaskAssignment,
            r#"
            SELECT id, task_id, task_type,
                      COALESCE(task_data, '{}'::jsonb) AS "task_data!",
                      assigned_worker_id,
                      status, priority,
                      created_ts, assigned_ts,
                      completed_ts, result,
                      error_message
            FROM worker_task_assignments
            WHERE task_id = $1 AND status = 'pending'
            "#,
            task_id
        )
        .fetch_optional(&*self.pool)
        .await
    }

    /// See [`claim_next_pending_task`].
    pub async fn claim_next_pending_task(&self, worker_id: &str) -> Result<Option<WorkerTaskAssignment>, sqlx::Error> {
        let now = current_timestamp_millis();

        sqlx::query_as!(
            WorkerTaskAssignment,
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
                      COALESCE(task_data, '{}'::jsonb) AS "task_data!",
                      assigned_worker_id,
                      status, priority,
                      created_ts, assigned_ts,
                      completed_ts, result,
                      error_message
            "#,
            worker_id,
            now
        )
        .fetch_optional(&*self.pool)
        .await
    }

    /// See [`claim_next_pending_task_for_types`].
    pub async fn claim_next_pending_task_for_types(
        &self,
        worker_id: &str,
        allowed_task_types: &[String],
    ) -> Result<Option<WorkerTaskAssignment>, sqlx::Error> {
        let now = current_timestamp_millis();

        sqlx::query_as!(
            WorkerTaskAssignment,
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
                      COALESCE(task_data, '{}'::jsonb) AS "task_data!",
                      assigned_worker_id,
                      status, priority,
                      created_ts, assigned_ts,
                      completed_ts, result,
                      error_message
            "#,
            worker_id,
            now,
            allowed_task_types
        )
        .fetch_optional(&*self.pool)
        .await
    }

    /// See [`assign_task_to_worker`].
    pub async fn assign_task_to_worker(&self, task_id: &str, worker_id: &str) -> Result<bool, sqlx::Error> {
        let now = current_timestamp_millis();

        let result: sqlx::postgres::PgQueryResult = sqlx::query!(
            r"
            UPDATE worker_task_assignments
            SET assigned_worker_id = $2, assigned_ts = $3, status = 'running'
            WHERE task_id = $1
              AND status = 'pending'
              AND assigned_worker_id IS NULL
            ",
            task_id,
            worker_id,
            now
        )
        .execute(&*self.pool)
        .await?;

        Ok(result.rows_affected() == 1)
    }

    /// See [`complete_task`].
    pub async fn complete_task(&self, task_id: &str, result: Option<serde_json::Value>) -> Result<(), sqlx::Error> {
        let now = current_timestamp_millis();

        sqlx::query!(
            r"UPDATE worker_task_assignments SET status = 'completed', completed_ts = $2, result = $3 WHERE task_id = $1",
            task_id,
            now,
            result.as_ref()
        )
        .execute(&*self.pool)
        .await?;

        Ok(())
    }

    /// See [`fail_task`].
    pub async fn fail_task(&self, task_id: &str, error: &str) -> Result<(), sqlx::Error> {
        let now = current_timestamp_millis();

        sqlx::query!(
            r"UPDATE worker_task_assignments SET status = 'failed', completed_ts = $2, error_message = $3 WHERE task_id = $1",
            task_id,
            now,
            error
        )
        .execute(&*self.pool)
        .await?;

        Ok(())
    }

    /// See [`record_connection`].
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

    /// See [`update_connection_stats`].
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

    /// Returns worker identity/status plus per-worker counters, newest first.
    ///
    /// **Contract fixed 2026-09-23.** The previous version selected 15 columns that
    /// exist in **neither** `workers` nor `worker_statistics`: its doc comment cited
    /// migration `20260812120000_worker_statistics_load_metrics`, which is not in
    /// `migrations/`, and the query failed with SQLSTATE 42703 (`column "worker_name"
    /// does not exist`) on every call — this endpoint never returned a single row.
    ///
    /// Identity/lifecycle fields live in `workers`; the counters and load metrics
    /// live in `worker_statistics`, which is now written by
    /// [`upsert_statistics`](Self::upsert_statistics) from the heartbeat payload.
    /// The LEFT JOIN + `AS "col?"` overrides are load-bearing: sqlx does not infer
    /// LEFT-JOIN nullability, so without `?` every worker lacking a statistics row
    /// would fail to decode (`UnexpectedNullError`) instead of yielding `null`.
    ///
    /// `worker_statistics.last_heartbeat_ts` is **not** emitted: the payload
    /// already carries `workers.last_heartbeat_ts` under that key, and the
    /// statistics copy is written with the same `now` from the same heartbeat, so
    /// a second key would be a duplicate. The identity field is left as-is.
    pub async fn get_statistics(&self, limit: i64) -> Result<Vec<serde_json::Value>, sqlx::Error> {
        let rows = sqlx::query!(
            r#"
            SELECT w.id AS "id!",
                   w.worker_id AS "worker_id!",
                   w.worker_name AS "worker_name!",
                   w.worker_type AS "worker_type!",
                   w.status AS "status!",
                   w.host AS "host!",
                   w.port AS "port!",
                   w.last_heartbeat_ts,
                   w.started_ts AS "started_ts!",
                   s.total_messages_sent AS "total_messages_sent?",
                   s.total_messages_received AS "total_messages_received?",
                   s.total_errors AS "total_errors?",
                   s.last_message_ts AS "last_message_ts?",
                   s.last_error_ts AS "last_error_ts?",
                   s.avg_processing_time_ms AS "avg_processing_time_ms?",
                   s.uptime_seconds AS "uptime_seconds?",
                   s.cpu_usage AS "cpu_usage?",
                   s.memory_usage AS "memory_usage?",
                   s.active_connections AS "active_connections?",
                   s.requests_per_second AS "requests_per_second?",
                   s.average_latency_ms AS "average_latency_ms?",
                   s.queue_depth AS "queue_depth?"
            FROM workers w
            LEFT JOIN worker_statistics s ON s.worker_id = w.worker_id
            ORDER BY w.id DESC
            LIMIT $1
            "#,
            limit
        )
        .fetch_all(&*self.pool)
        .await?;

        Ok(rows
            .into_iter()
            .map(|row| {
                serde_json::json!({
                    "id": row.id,
                    "worker_id": row.worker_id,
                    "worker_name": row.worker_name,
                    "worker_type": row.worker_type,
                    "status": row.status,
                    "host": row.host,
                    "port": row.port,
                    "last_heartbeat_ts": row.last_heartbeat_ts,
                    "started_ts": row.started_ts,
                    "total_messages_sent": row.total_messages_sent,
                    "total_messages_received": row.total_messages_received,
                    "total_errors": row.total_errors,
                    "last_message_ts": row.last_message_ts,
                    "last_error_ts": row.last_error_ts,
                    "avg_processing_time_ms": row.avg_processing_time_ms,
                    "uptime_seconds": row.uptime_seconds,
                    "cpu_usage": row.cpu_usage,
                    "memory_usage": row.memory_usage,
                    "active_connections": row.active_connections,
                    "requests_per_second": row.requests_per_second,
                    "average_latency_ms": row.average_latency_ms,
                    "queue_depth": row.queue_depth,
                })
            })
            .collect())
    }

    /// See [`get_type_statistics`].
    pub async fn get_type_statistics(&self) -> Result<Vec<serde_json::Value>, sqlx::Error> {
        let rows = sqlx::query!(
            r#"
            SELECT worker_type AS "worker_type!", total_count AS "total_count!",
                   running_count AS "running_count!", starting_count AS "starting_count!",
                   stopping_count AS "stopping_count!", stopped_count AS "stopped_count!",
                   avg_cpu_usage, avg_memory_usage,
                   total_connections
            FROM worker_type_statistics
            "#
        )
        .fetch_all(&*self.pool)
        .await?;

        Ok(rows
            .into_iter()
            .map(|row| {
                serde_json::json!({
                    "worker_type": row.worker_type,
                    "total_count": row.total_count,
                    "running_count": row.running_count,
                    "starting_count": row.starting_count,
                    "stopping_count": row.stopping_count,
                    "stopped_count": row.stopped_count,
                    "avg_cpu_usage": row.avg_cpu_usage,
                    "avg_memory_usage": row.avg_memory_usage,
                    "total_connections": row.total_connections,
                })
            })
            .collect())
    }
}
