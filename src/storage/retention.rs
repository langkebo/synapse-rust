use chrono::Utc;
use serde::{Deserialize, Serialize};
use sqlx::{FromRow, PgPool};
use std::sync::Arc;

#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
pub struct RoomRetentionPolicy {
    pub id: i64,
    pub room_id: String,
    pub max_lifetime: Option<i64>,
    pub min_lifetime: i64,
    pub expire_on_clients: bool,
    pub is_server_default: bool,
    pub created_ts: i64,
    pub updated_ts: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
pub struct ServerRetentionPolicy {
    pub id: i64,
    pub max_lifetime: Option<i64>,
    pub min_lifetime: i64,
    pub expire_on_clients: bool,
    pub created_ts: i64,
    pub updated_ts: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
pub struct RetentionCleanupQueueItem {
    pub id: i64,
    pub room_id: String,
    pub event_id: Option<String>,
    pub event_type: Option<String>,
    pub origin_server_ts: i64,
    pub scheduled_ts: i64,
    pub status: String,
    pub created_ts: i64,
    pub processed_ts: Option<i64>,
    pub error_message: Option<String>,
    pub retry_count: i32,
}

#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
pub struct RetentionCleanupLog {
    pub id: i64,
    pub room_id: String,
    pub events_deleted: i64,
    pub state_events_deleted: i64,
    pub media_deleted: i64,
    pub bytes_freed: i64,
    pub started_ts: i64,
    pub completed_ts: Option<i64>,
    pub status: String,
    pub error_message: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
pub struct DeletedEventIndex {
    pub id: i64,
    pub room_id: String,
    pub event_id: String,
    pub deletion_ts: i64,
    pub reason: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
pub struct RetentionStats {
    pub id: i64,
    pub room_id: String,
    pub total_events: i64,
    pub events_in_retention: i64,
    pub events_expired: i64,
    pub last_cleanup_ts: Option<i64>,
    pub next_cleanup_ts: Option<i64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CreateRoomRetentionPolicyRequest {
    pub room_id: String,
    pub max_lifetime: Option<i64>,
    pub min_lifetime: Option<i64>,
    pub expire_on_clients: Option<bool>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct UpdateRoomRetentionPolicyRequest {
    pub max_lifetime: Option<i64>,
    pub min_lifetime: Option<i64>,
    pub expire_on_clients: Option<bool>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UpdateServerRetentionPolicyRequest {
    pub max_lifetime: Option<i64>,
    pub min_lifetime: Option<i64>,
    pub expire_on_clients: Option<bool>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EffectiveRetentionPolicy {
    pub max_lifetime: Option<i64>,
    pub min_lifetime: i64,
    pub expire_on_clients: bool,
}

#[derive(Clone)]
pub struct RetentionStorage {
    pool: Arc<PgPool>,
}

impl RetentionStorage {
    pub fn new(pool: &Arc<PgPool>) -> Self {
        Self { pool: pool.clone() }
    }

    pub async fn create_room_policy(
        &self,
        request: CreateRoomRetentionPolicyRequest,
    ) -> Result<RoomRetentionPolicy, sqlx::Error> {
        let now = Utc::now().timestamp_millis();

        let row = sqlx::query_as::<_, RoomRetentionPolicy>(
            r#"
            INSERT INTO room_retention_policies (
                room_id, max_lifetime, min_lifetime, expire_on_clients, is_server_default, created_ts, updated_ts
            )
            VALUES ($1, $2, $3, $4, FALSE, $5, $5)
            ON CONFLICT (room_id) DO UPDATE SET
                max_lifetime = EXCLUDED.max_lifetime,
                min_lifetime = EXCLUDED.min_lifetime,
                expire_on_clients = EXCLUDED.expire_on_clients,
                updated_ts = EXCLUDED.updated_ts
            RETURNING *
            "#,
        )
        .bind(&request.room_id)
        .bind(request.max_lifetime)
        .bind(request.min_lifetime.unwrap_or(0))
        .bind(request.expire_on_clients.unwrap_or(false))
        .bind(now)
        .fetch_one(&*self.pool)
        .await?;

        Ok(row)
    }

    pub async fn get_room_policy(
        &self,
        room_id: &str,
    ) -> Result<Option<RoomRetentionPolicy>, sqlx::Error> {
        let row = sqlx::query_as::<_, RoomRetentionPolicy>(
            "SELECT * FROM room_retention_policies WHERE room_id = $1",
        )
        .bind(room_id)
        .fetch_optional(&*self.pool)
        .await?;

        Ok(row)
    }

    pub async fn update_room_policy(
        &self,
        room_id: &str,
        request: UpdateRoomRetentionPolicyRequest,
    ) -> Result<RoomRetentionPolicy, sqlx::Error> {
        let row = sqlx::query_as::<_, RoomRetentionPolicy>(
            r#"
            UPDATE room_retention_policies SET
                max_lifetime = COALESCE($2, max_lifetime),
                min_lifetime = COALESCE($3, min_lifetime),
                expire_on_clients = COALESCE($4, expire_on_clients)
            WHERE room_id = $1
            RETURNING *
            "#,
        )
        .bind(room_id)
        .bind(request.max_lifetime)
        .bind(request.min_lifetime)
        .bind(request.expire_on_clients)
        .fetch_one(&*self.pool)
        .await?;

        Ok(row)
    }

    pub async fn delete_room_policy(&self, room_id: &str) -> Result<(), sqlx::Error> {
        sqlx::query("DELETE FROM room_retention_policies WHERE room_id = $1")
            .bind(room_id)
            .execute(&*self.pool)
            .await?;

        Ok(())
    }

    pub async fn get_server_policy(&self) -> Result<ServerRetentionPolicy, sqlx::Error> {
        let row = sqlx::query_as::<_, ServerRetentionPolicy>(
            "SELECT * FROM server_retention_policy ORDER BY id LIMIT 1",
        )
        .fetch_one(&*self.pool)
        .await?;

        Ok(row)
    }

    pub async fn update_server_policy(
        &self,
        request: UpdateServerRetentionPolicyRequest,
    ) -> Result<ServerRetentionPolicy, sqlx::Error> {
        let row = sqlx::query_as::<_, ServerRetentionPolicy>(
            r#"
            UPDATE server_retention_policy SET
                max_lifetime = COALESCE($1, max_lifetime),
                min_lifetime = COALESCE($2, min_lifetime),
                expire_on_clients = COALESCE($3, expire_on_clients)
            WHERE id = (SELECT MIN(id) FROM server_retention_policy)
            RETURNING *
            "#,
        )
        .bind(request.max_lifetime)
        .bind(request.min_lifetime)
        .bind(request.expire_on_clients)
        .fetch_one(&*self.pool)
        .await?;

        Ok(row)
    }

    pub async fn get_effective_policy(
        &self,
        room_id: &str,
    ) -> Result<EffectiveRetentionPolicy, sqlx::Error> {
        let room_policy = self.get_room_policy(room_id).await?;
        let server_policy = self.get_server_policy().await?;

        Ok(EffectiveRetentionPolicy {
            max_lifetime: room_policy
                .as_ref()
                .and_then(|p| p.max_lifetime)
                .or(server_policy.max_lifetime),
            min_lifetime: room_policy
                .as_ref()
                .map(|p| p.min_lifetime)
                .unwrap_or(server_policy.min_lifetime),
            expire_on_clients: room_policy
                .as_ref()
                .map(|p| p.expire_on_clients)
                .unwrap_or(server_policy.expire_on_clients),
        })
    }

    pub async fn queue_cleanup(
        &self,
        room_id: &str,
        event_id: &str,
        event_type: &str,
        origin_server_ts: i64,
    ) -> Result<(), sqlx::Error> {
        let now = Utc::now().timestamp_millis();

        sqlx::query(
            r#"
            INSERT INTO retention_cleanup_queue (room_id, event_id, event_type, origin_server_ts, scheduled_ts, created_ts)
            VALUES ($1, $2, $3, $4, $5, $5)
            ON CONFLICT DO NOTHING
            "#,
        )
        .bind(room_id)
        .bind(event_id)
        .bind(event_type)
        .bind(origin_server_ts)
        .bind(now)
        .execute(&*self.pool)
        .await?;

        Ok(())
    }

    pub async fn get_pending_cleanups(
        &self,
        limit: i64,
    ) -> Result<Vec<RetentionCleanupQueueItem>, sqlx::Error> {
        let rows = sqlx::query_as::<_, RetentionCleanupQueueItem>(
            r#"
            SELECT * FROM retention_cleanup_queue
            WHERE status = 'pending'
            ORDER BY origin_server_ts ASC
            LIMIT $1
            FOR UPDATE SKIP LOCKED
            "#,
        )
        .bind(limit)
        .fetch_all(&*self.pool)
        .await?;

        Ok(rows)
    }

    pub async fn mark_cleanup_processed(&self, id: i64) -> Result<(), sqlx::Error> {
        let now = Utc::now().timestamp_millis();

        sqlx::query(
            "UPDATE retention_cleanup_queue SET status = 'processed', processed_ts = $2 WHERE id = $1",
        )
        .bind(id)
        .bind(now)
        .execute(&*self.pool)
        .await?;

        Ok(())
    }

    pub async fn mark_cleanup_failed(&self, id: i64, error: &str) -> Result<(), sqlx::Error> {
        sqlx::query(
            r#"
            UPDATE retention_cleanup_queue SET
                status = 'failed',
                error_message = $2,
                retry_count = retry_count + 1
            WHERE id = $1
            "#,
        )
        .bind(id)
        .bind(error)
        .execute(&*self.pool)
        .await?;

        Ok(())
    }

    pub async fn create_cleanup_log(
        &self,
        room_id: &str,
    ) -> Result<RetentionCleanupLog, sqlx::Error> {
        let now = Utc::now().timestamp_millis();

        let row = sqlx::query_as::<_, RetentionCleanupLog>(
            r#"
            INSERT INTO retention_cleanup_logs (room_id, started_ts, status)
            VALUES ($1, $2, 'running')
            RETURNING *
            "#,
        )
        .bind(room_id)
        .bind(now)
        .fetch_one(&*self.pool)
        .await?;

        Ok(row)
    }

    pub async fn complete_cleanup_log(
        &self,
        id: i64,
        events_deleted: i64,
        state_events_deleted: i64,
        media_deleted: i64,
        bytes_freed: i64,
    ) -> Result<(), sqlx::Error> {
        let now = Utc::now().timestamp_millis();

        sqlx::query(
            r#"
            UPDATE retention_cleanup_logs SET
                events_deleted = $2,
                state_events_deleted = $3,
                media_deleted = $4,
                bytes_freed = $5,
                completed_ts = $6,
                status = 'completed'
            WHERE id = $1
            "#,
        )
        .bind(id)
        .bind(events_deleted)
        .bind(state_events_deleted)
        .bind(media_deleted)
        .bind(bytes_freed)
        .bind(now)
        .execute(&*self.pool)
        .await?;

        Ok(())
    }

    pub async fn fail_cleanup_log(&self, id: i64, error: &str) -> Result<(), sqlx::Error> {
        let now = Utc::now().timestamp_millis();

        sqlx::query(
            r#"
            UPDATE retention_cleanup_logs SET
                completed_ts = $2,
                status = 'failed',
                error_message = $3
            WHERE id = $1
            "#,
        )
        .bind(id)
        .bind(now)
        .bind(error)
        .execute(&*self.pool)
        .await?;

        Ok(())
    }

    pub async fn record_deleted_event(
        &self,
        room_id: &str,
        event_id: &str,
        reason: &str,
    ) -> Result<(), sqlx::Error> {
        let now = Utc::now().timestamp_millis();

        sqlx::query(
            r#"
            INSERT INTO deleted_events_index (room_id, event_id, deletion_ts, reason)
            VALUES ($1, $2, $3, $4)
            "#,
        )
        .bind(room_id)
        .bind(event_id)
        .bind(now)
        .bind(reason)
        .execute(&*self.pool)
        .await?;

        Ok(())
    }

    pub async fn get_deleted_events(
        &self,
        room_id: &str,
        since_ts: i64,
    ) -> Result<Vec<DeletedEventIndex>, sqlx::Error> {
        let rows = sqlx::query_as::<_, DeletedEventIndex>(
            "SELECT * FROM deleted_events_index WHERE room_id = $1 AND deletion_ts > $2 ORDER BY deletion_ts ASC",
        )
        .bind(room_id)
        .bind(since_ts)
        .fetch_all(&*self.pool)
        .await?;

        Ok(rows)
    }

    pub async fn get_stats(&self, room_id: &str) -> Result<Option<RetentionStats>, sqlx::Error> {
        let row =
            sqlx::query_as::<_, RetentionStats>("SELECT * FROM retention_stats WHERE room_id = $1")
                .bind(room_id)
                .fetch_optional(&*self.pool)
                .await?;

        Ok(row)
    }

    pub async fn update_stats(
        &self,
        room_id: &str,
        total_events: i64,
        events_in_retention: i64,
        events_expired: i64,
        next_cleanup_ts: Option<i64>,
    ) -> Result<(), sqlx::Error> {
        let now = Utc::now().timestamp_millis();

        sqlx::query(
            r#"
            INSERT INTO retention_stats (room_id, total_events, events_in_retention, events_expired, last_cleanup_ts, next_cleanup_ts)
            VALUES ($1, $2, $3, $4, $5, $6)
            ON CONFLICT (room_id) DO UPDATE SET
                total_events = EXCLUDED.total_events,
                events_in_retention = EXCLUDED.events_in_retention,
                events_expired = EXCLUDED.events_expired,
                last_cleanup_ts = EXCLUDED.last_cleanup_ts,
                next_cleanup_ts = EXCLUDED.next_cleanup_ts
            "#,
        )
        .bind(room_id)
        .bind(total_events)
        .bind(events_in_retention)
        .bind(events_expired)
        .bind(now)
        .bind(next_cleanup_ts)
        .execute(&*self.pool)
        .await?;

        Ok(())
    }

    pub async fn delete_events_before(
        &self,
        room_id: &str,
        cutoff_ts: i64,
    ) -> Result<i64, sqlx::Error> {
        let result = sqlx::query(
            r#"
            DELETE FROM events 
            WHERE room_id = $1 
            AND origin_server_ts < $2
            AND event_type NOT IN ('m.room.create', 'm.room.power_levels', 'm.room.join_rules', 'm.room.history_visibility')
            AND state_key IS NULL
            "#,
        )
        .bind(room_id)
        .bind(cutoff_ts)
        .execute(&*self.pool)
        .await?;

        Ok(result.rows_affected() as i64)
    }

    pub async fn get_rooms_with_policies(&self) -> Result<Vec<RoomRetentionPolicy>, sqlx::Error> {
        let rows = sqlx::query_as::<_, RoomRetentionPolicy>(
            "SELECT * FROM room_retention_policies ORDER BY room_id",
        )
        .fetch_all(&*self.pool)
        .await?;

        Ok(rows)
    }

    pub async fn get_cleanup_logs(
        &self,
        room_id: &str,
        limit: i64,
    ) -> Result<Vec<RetentionCleanupLog>, sqlx::Error> {
        let rows = sqlx::query_as::<_, RetentionCleanupLog>(
            "SELECT * FROM retention_cleanup_logs WHERE room_id = $1 ORDER BY started_ts DESC LIMIT $2",
        )
        .bind(room_id)
        .bind(limit)
        .fetch_all(&*self.pool)
        .await?;

        Ok(rows)
    }

    pub async fn get_pending_cleanup_count(&self, room_id: &str) -> Result<i64, sqlx::Error> {
        let count: i64 = sqlx::query_scalar(
            "SELECT COUNT(*) FROM retention_cleanup_queue WHERE room_id = $1 AND status = 'pending'",
        )
        .bind(room_id)
        .fetch_one(&*self.pool)
        .await?;

        Ok(count)
    }

    pub async fn schedule_room_cleanup(&self, room_id: &str) -> Result<i64, sqlx::Error> {
        let result = sqlx::query("SELECT schedule_retention_cleanup($1)")
            .bind(room_id)
            .execute(&*self.pool)
            .await?;

        Ok(result.rows_affected() as i64)
    }
}
