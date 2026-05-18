use serde::{Deserialize, Serialize};
use sqlx::postgres::PgQueryResult;
use sqlx::PgPool;

#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow)]
pub struct FederationQueueEntry {
    pub id: i64,
    pub destination: String,
    pub event_id: String,
    pub event_type: String,
    pub room_id: Option<String>,
    pub content: serde_json::Value,
    pub created_ts: i64,
    pub sent_at: Option<i64>,
    pub retry_count: i32,
    pub status: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InsertFederationQueueRequest {
    pub destination: String,
    pub event_id: String,
    pub event_type: String,
    pub room_id: Option<String>,
    pub content: serde_json::Value,
    pub created_ts: i64,
}

pub struct FederationQueueStorage {
    pool: PgPool,
}

impl FederationQueueStorage {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }

    pub async fn insert(&self, req: &InsertFederationQueueRequest) -> Result<i64, sqlx::Error> {
        let row = sqlx::query_as::<_, (i64,)>(
            r"
            INSERT INTO federation_queue (destination, event_id, event_type, room_id, content, created_ts, status)
            VALUES ($1, $2, $3, $4, $5, $6, 'pending')
            RETURNING id
            ",
        )
        .bind(&req.destination)
        .bind(&req.event_id)
        .bind(&req.event_type)
        .bind(&req.room_id)
        .bind(&req.content)
        .bind(req.created_ts)
        .fetch_one(&self.pool)
        .await?;

        Ok(row.0)
    }

    pub async fn mark_sent(&self, id: i64, sent_at: i64) -> Result<PgQueryResult, sqlx::Error> {
        sqlx::query(
            r"
            UPDATE federation_queue
            SET status = 'sent', sent_at = $2
            WHERE id = $1
            ",
        )
        .bind(id)
        .bind(sent_at)
        .execute(&self.pool)
        .await
    }

    pub async fn increment_retry(&self, id: i64) -> Result<PgQueryResult, sqlx::Error> {
        sqlx::query(
            r"
            UPDATE federation_queue
            SET retry_count = retry_count + 1, status = 'pending'
            WHERE id = $1
            ",
        )
        .bind(id)
        .execute(&self.pool)
        .await
    }

    pub async fn mark_failed(&self, id: i64) -> Result<PgQueryResult, sqlx::Error> {
        sqlx::query(
            r"
            UPDATE federation_queue
            SET status = 'failed'
            WHERE id = $1
            ",
        )
        .bind(id)
        .execute(&self.pool)
        .await
    }

    pub async fn get_pending_by_destination(
        &self,
        destination: &str,
        limit: i64,
    ) -> Result<Vec<FederationQueueEntry>, sqlx::Error> {
        sqlx::query_as::<_, FederationQueueEntry>(
            r"
            SELECT id, destination, event_id, event_type, room_id, content, created_ts, sent_at, retry_count, status
            FROM federation_queue
            WHERE destination = $1 AND status = 'pending'
            ORDER BY created_ts ASC
            LIMIT $2
            ",
        )
        .bind(destination)
        .bind(limit)
        .fetch_all(&self.pool)
        .await
    }

    pub async fn get_all_pending(&self) -> Result<Vec<FederationQueueEntry>, sqlx::Error> {
        sqlx::query_as::<_, FederationQueueEntry>(
            r"
            SELECT id, destination, event_id, event_type, room_id, content, created_ts, sent_at, retry_count, status
            FROM federation_queue
            WHERE status = 'pending'
            ORDER BY created_ts ASC
            LIMIT 1000
            ",
        )
        .fetch_all(&self.pool)
        .await
    }

    pub async fn delete_completed(&self, older_than_ts: i64) -> Result<u64, sqlx::Error> {
        let result = sqlx::query(
            r"
            DELETE FROM federation_queue
            WHERE status IN ('sent', 'failed') AND created_ts < $1
            ",
        )
        .bind(older_than_ts)
        .execute(&self.pool)
        .await?;

        Ok(result.rows_affected())
    }

    pub async fn count_pending(&self) -> Result<i64, sqlx::Error> {
        let row = sqlx::query_as::<_, (Option<i64>,)>(
            r"SELECT COUNT(*) FROM federation_queue WHERE status = 'pending'",
        )
        .fetch_one(&self.pool)
        .await?;

        Ok(row.0.unwrap_or(0))
    }
}
