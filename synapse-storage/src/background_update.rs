use async_trait::async_trait;
use chrono::Utc;
use serde::{Deserialize, Serialize};
use sqlx::{FromRow, PgPool};
use std::sync::Arc;

fn decode_background_update_cursor(cursor: &str) -> Option<(i64, &str)> {
    let (created_ts, job_name) = cursor.split_once('|')?;
    let created_ts = created_ts.parse::<i64>().ok()?;
    if job_name.is_empty() {
        return None;
    }
    Some((created_ts, job_name))
}

fn encode_background_update_cursor(created_ts: i64, job_name: &str) -> String {
    format!("{created_ts}|{job_name}")
}

#[cfg(test)]
mod cursor_tests {
    use super::{decode_background_update_cursor, encode_background_update_cursor};

    #[test]
    fn test_background_update_cursor_round_trip() {
        let cursor = encode_background_update_cursor(1_700_000_000_000, "job-name");
        assert_eq!(decode_background_update_cursor(&cursor), Some((1_700_000_000_000, "job-name")));
    }

    #[test]
    fn test_background_update_cursor_rejects_invalid_value() {
        assert_eq!(decode_background_update_cursor("bad-cursor"), None);
        assert_eq!(decode_background_update_cursor("123|"), None);
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
pub struct BackgroundUpdate {
    pub job_name: String,
    pub job_type: String,
    pub description: Option<String>,
    pub table_name: Option<String>,
    pub column_name: Option<String>,
    pub status: String,
    pub progress: serde_json::Value,
    pub total_items: i32,
    pub processed_items: i32,
    pub created_ts: Option<i64>,
    pub started_ts: Option<i64>,
    pub completed_ts: Option<i64>,
    pub updated_ts: Option<i64>,
    pub error_message: Option<String>,
    pub retry_count: i32,
    pub max_retries: i32,
    pub batch_size: i32,
    pub sleep_ms: i32,
    pub depends_on: Option<serde_json::Value>,
    pub metadata: Option<serde_json::Value>,
}

#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
pub struct BackgroundUpdateHistory {
    pub id: i64,
    pub job_name: String,
    pub execution_start_ts: i64,
    pub execution_end_ts: Option<i64>,
    pub status: String,
    pub items_processed: i32,
    pub error_message: Option<String>,
    pub metadata: Option<serde_json::Value>,
}

#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
pub struct BackgroundUpdateLock {
    pub lock_name: String,
    pub owner: Option<String>,
    pub acquired_ts: i64,
    pub expires_at: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
pub struct BackgroundUpdateStats {
    pub id: i64,
    pub job_name: String,
    pub total_updates: i32,
    pub completed_updates: i32,
    pub failed_updates: i32,
    pub last_run_ts: Option<i64>,
    pub next_run_ts: Option<i64>,
    pub average_duration_ms: i64,
    pub created_ts: i64,
    pub updated_ts: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CreateBackgroundUpdateRequest {
    pub job_name: String,
    pub job_type: String,
    pub description: Option<String>,
    pub table_name: Option<String>,
    pub column_name: Option<String>,
    pub total_items: Option<i32>,
    pub batch_size: Option<i32>,
    pub sleep_ms: Option<i32>,
    pub depends_on: Option<Vec<String>>,
    pub metadata: Option<serde_json::Value>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct UpdateBackgroundUpdateRequest {
    pub status: Option<String>,
    pub progress: Option<i32>,
    pub total_items: Option<i32>,
    pub processed_items: Option<i32>,
    pub error_message: Option<String>,
}

#[async_trait]
pub trait BackgroundUpdateStoreApi: Send + Sync {
    async fn create_update(&self, request: CreateBackgroundUpdateRequest) -> Result<BackgroundUpdate, sqlx::Error>;
    async fn get_update(&self, job_name: &str) -> Result<Option<BackgroundUpdate>, sqlx::Error>;
    async fn get_all_updates(
        &self,
        limit: i64,
        from: Option<String>,
    ) -> Result<(Vec<BackgroundUpdate>, Option<String>), sqlx::Error>;
    async fn get_pending_updates(&self) -> Result<Vec<BackgroundUpdate>, sqlx::Error>;
    async fn get_running_updates(&self) -> Result<Vec<BackgroundUpdate>, sqlx::Error>;
    async fn update_status(&self, job_name: &str, status: &str) -> Result<BackgroundUpdate, sqlx::Error>;
    async fn update_progress(
        &self,
        job_name: &str,
        items_processed: i32,
        total_items: Option<i32>,
    ) -> Result<BackgroundUpdate, sqlx::Error>;
    async fn set_error(&self, job_name: &str, error_message: &str) -> Result<BackgroundUpdate, sqlx::Error>;
    async fn delete_update(&self, job_name: &str) -> Result<(), sqlx::Error>;
    async fn acquire_lock_with_retry(
        &self,
        job_name: &str,
        locked_by: &str,
        lock_duration_ms: i64,
        max_retries: u32,
        max_retry_interval_ms: u64,
    ) -> Result<bool, sqlx::Error>;
    async fn release_lock(&self, job_name: &str) -> Result<(), sqlx::Error>;
    async fn is_locked(&self, job_name: &str) -> Result<bool, sqlx::Error>;
    async fn cleanup_expired_locks(&self) -> Result<i64, sqlx::Error>;
    async fn add_history(
        &self,
        job_name: &str,
        status: &str,
        items_processed: i32,
        error_message: Option<&str>,
        metadata: Option<serde_json::Value>,
    ) -> Result<BackgroundUpdateHistory, sqlx::Error>;
    async fn get_history(&self, job_name: &str, limit: i64) -> Result<Vec<BackgroundUpdateHistory>, sqlx::Error>;
    async fn retry_failed(&self) -> Result<i64, sqlx::Error>;
    async fn count_by_status(&self, status: &str) -> Result<i64, sqlx::Error>;
    async fn count_all(&self) -> Result<i64, sqlx::Error>;
    async fn get_stats(&self, limit: i32) -> Result<Vec<BackgroundUpdateStats>, sqlx::Error>;
}

#[derive(Clone)]
pub struct BackgroundUpdateStorage {
    pool: Arc<PgPool>,
}

impl BackgroundUpdateStorage {
    pub fn new(pool: &Arc<PgPool>) -> Self {
        Self { pool: pool.clone() }
    }

    pub async fn create_update(&self, request: CreateBackgroundUpdateRequest) -> Result<BackgroundUpdate, sqlx::Error> {
        let now = Utc::now().timestamp_millis();

        // Schema column `depends_on` is JSONB (default '[]'), but
        // CreateBackgroundUpdateRequest.depends_on is Vec<String>. Encode the
        // Vec as a JSON array so sqlx sends it as a JSONB parameter instead of
        // a PostgreSQL text[].
        let depends_on_json: serde_json::Value = match &request.depends_on {
            Some(v) => serde_json::Value::Array(v.iter().map(|s| serde_json::Value::String(s.clone())).collect()),
            None => serde_json::Value::Array(vec![]),
        };

        let row = sqlx::query_as::<_, BackgroundUpdate>(
            r"
            INSERT INTO background_updates (
                job_name, job_type, description, table_name, column_name, total_items,
                batch_size, sleep_ms, depends_on, metadata, created_ts, status, max_retries
            )
            VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, 'pending', 3)
            RETURNING *
            ",
        )
        .bind(&request.job_name)
        .bind(&request.job_type)
        .bind(&request.description)
        .bind(&request.table_name)
        .bind(&request.column_name)
        .bind(request.total_items.unwrap_or(0))
        .bind(request.batch_size.unwrap_or(100))
        .bind(request.sleep_ms.unwrap_or(1000))
        .bind(&depends_on_json)
        .bind(&request.metadata)
        .bind(now)
        .fetch_one(&*self.pool)
        .await?;

        Ok(row)
    }

    pub async fn get_update(&self, job_name: &str) -> Result<Option<BackgroundUpdate>, sqlx::Error> {
        let row = sqlx::query_as::<_, BackgroundUpdate>("SELECT job_name, job_type, description, table_name, column_name, status, progress, total_items, processed_items, created_ts, started_ts, completed_ts, updated_ts, error_message, retry_count, max_retries, batch_size, sleep_ms, depends_on, metadata FROM background_updates WHERE update_name = $1")
            .bind(job_name)
            .fetch_optional(&*self.pool)
            .await?;

        Ok(row)
    }

    pub async fn get_all_updates(
        &self,
        limit: i64,
        from: Option<String>,
    ) -> Result<(Vec<BackgroundUpdate>, Option<String>), sqlx::Error> {
        let decoded = from.as_deref().and_then(decode_background_update_cursor);
        let rows = sqlx::query_as::<_, BackgroundUpdate>(
            "SELECT job_name, job_type, description, table_name, column_name, status, progress, total_items, processed_items, created_ts, started_ts, completed_ts, updated_ts, error_message, retry_count, max_retries, batch_size, sleep_ms, depends_on, metadata FROM background_updates
             WHERE ($2::BIGINT IS NULL AND $3::TEXT IS NULL)
                OR created_ts < $2
                OR (created_ts = $2 AND job_name < $3)
             ORDER BY created_ts DESC, job_name DESC
             LIMIT $1",
        )
        .bind(limit)
        .bind(decoded.map(|(created_ts, _)| created_ts))
        .bind(decoded.map(|(_, job_name)| job_name))
        .fetch_all(&*self.pool)
        .await?;

        let next_from = if rows.len() as i64 == limit {
            rows.last().map(|row| encode_background_update_cursor(row.created_ts.unwrap_or(0), &row.job_name))
        } else {
            None
        };

        Ok((rows, next_from))
    }

    pub async fn get_updates_by_status(&self, status: &str) -> Result<Vec<BackgroundUpdate>, sqlx::Error> {
        let rows = sqlx::query_as::<_, BackgroundUpdate>(
            "SELECT job_name, job_type, description, table_name, column_name, status, progress, total_items, processed_items, created_ts, started_ts, completed_ts, updated_ts, error_message, retry_count, max_retries, batch_size, sleep_ms, depends_on, metadata FROM background_updates WHERE status = $1 ORDER BY created_ts ASC",
        )
        .bind(status)
        .fetch_all(&*self.pool)
        .await?;

        Ok(rows)
    }

    pub async fn get_pending_updates(&self) -> Result<Vec<BackgroundUpdate>, sqlx::Error> {
        self.get_updates_by_status("pending").await
    }

    pub async fn get_running_updates(&self) -> Result<Vec<BackgroundUpdate>, sqlx::Error> {
        self.get_updates_by_status("running").await
    }

    pub async fn update_status(&self, job_name: &str, status: &str) -> Result<BackgroundUpdate, sqlx::Error> {
        let now = Utc::now().timestamp_millis();

        let started_ts = if status == "running" { Some(now) } else { None };

        let completed_ts = if status == "completed" { Some(now) } else { None };

        let row = sqlx::query_as::<_, BackgroundUpdate>(
            r"
            UPDATE background_updates SET
                status = $2,
                started_ts = COALESCE($3, started_ts),
                completed_ts = COALESCE($4, completed_ts),
                updated_ts = $5
            WHERE update_name = $1
            RETURNING *
            ",
        )
        .bind(job_name)
        .bind(status)
        .bind(started_ts)
        .bind(completed_ts)
        .bind(now)
        .fetch_one(&*self.pool)
        .await?;

        Ok(row)
    }

    pub async fn update_progress(
        &self,
        job_name: &str,
        items_processed: i32,
        total_items: Option<i32>,
    ) -> Result<BackgroundUpdate, sqlx::Error> {
        let now = Utc::now().timestamp_millis();

        // Schema column `progress` is JSONB (default '{}'), so wrap the
        // computed percentage as a JSONB value to keep CASE branch types
        // consistent.
        let row = sqlx::query_as::<_, BackgroundUpdate>(
            r"
            UPDATE background_updates SET
                processed_items = processed_items + $2,
                total_items = COALESCE($3, total_items),
                updated_ts = $4,
                progress = CASE
                    WHEN COALESCE($3, total_items) > 0
                    THEN to_jsonb(ROUND((processed_items + $2)::FLOAT / COALESCE($3, total_items) * 100)::INTEGER)
                    ELSE progress
                END
            WHERE update_name = $1
            RETURNING *
            ",
        )
        .bind(job_name)
        .bind(items_processed)
        .bind(total_items)
        .bind(now)
        .fetch_one(&*self.pool)
        .await?;

        Ok(row)
    }

    pub async fn set_error(&self, job_name: &str, error_message: &str) -> Result<BackgroundUpdate, sqlx::Error> {
        let now = Utc::now().timestamp_millis();

        let row = sqlx::query_as::<_, BackgroundUpdate>(
            r"
            UPDATE background_updates SET
                status = 'failed',
                error_message = $2,
                updated_ts = $3,
                retry_count = retry_count + 1
            WHERE update_name = $1
            RETURNING *
            ",
        )
        .bind(job_name)
        .bind(error_message)
        .bind(now)
        .fetch_one(&*self.pool)
        .await?;

        Ok(row)
    }

    pub async fn delete_update(&self, job_name: &str) -> Result<(), sqlx::Error> {
        sqlx::query("DELETE FROM background_updates WHERE update_name = $1")
            .bind(job_name)
            .execute(&*self.pool)
            .await?;

        Ok(())
    }

    pub async fn acquire_lock(
        &self,
        job_name: &str,
        locked_by: &str,
        lock_duration_ms: i64,
    ) -> Result<bool, sqlx::Error> {
        let now = Utc::now().timestamp_millis();
        let expires = now + lock_duration_ms;

        let result = sqlx::query(
            r"
            INSERT INTO background_update_locks (lock_name, owner, acquired_ts, expires_at)
            VALUES ($1, $2, $3, $4)
            ON CONFLICT (lock_name) DO UPDATE SET
                owner = $2,
                acquired_ts = $3,
                expires_at = $4
            WHERE background_update_locks.expires_at < $3
            ",
        )
        .bind(job_name)
        .bind(locked_by)
        .bind(now)
        .bind(expires)
        .execute(&*self.pool)
        .await?;

        Ok(result.rows_affected() > 0)
    }

    /// Acquire a background update lock with exponential backoff retry.
    ///
    /// This method retries lock acquisition up to `max_retries` times, with
    /// exponential backoff capped at `max_retry_interval_ms`. This prevents
    /// CPU starvation / DoS under lock contention while still allowing
    /// workers to eventually acquire the lock.
    ///
    /// Aligned with Synapse v1.153.0 which lowered
    /// `WORKER_LOCK_MAX_RETRY_INTERVAL` to 5 seconds.
    ///
    /// Returns `Ok(true)` if the lock was acquired, `Ok(false)` if all
    /// retries were exhausted without acquiring the lock.
    pub async fn acquire_lock_with_retry(
        &self,
        job_name: &str,
        locked_by: &str,
        lock_duration_ms: i64,
        max_retries: u32,
        max_retry_interval_ms: u64,
    ) -> Result<bool, sqlx::Error> {
        // First attempt without delay.
        if self.acquire_lock(job_name, locked_by, lock_duration_ms).await? {
            return Ok(true);
        }

        // Retry with exponential backoff: 100ms, 200ms, 400ms, ..., capped.
        let mut delay_ms: u64 = 100;
        for _attempt in 0..max_retries {
            let sleep_ms = delay_ms.min(max_retry_interval_ms);
            tokio::time::sleep(std::time::Duration::from_millis(sleep_ms)).await;

            if self.acquire_lock(job_name, locked_by, lock_duration_ms).await? {
                return Ok(true);
            }
            delay_ms = delay_ms.saturating_mul(2);
        }

        Ok(false)
    }

    pub async fn release_lock(&self, job_name: &str) -> Result<(), sqlx::Error> {
        sqlx::query("DELETE FROM background_update_locks WHERE lock_name = $1")
            .bind(job_name)
            .execute(&*self.pool)
            .await?;

        Ok(())
    }

    pub async fn is_locked(&self, job_name: &str) -> Result<bool, sqlx::Error> {
        let now = Utc::now().timestamp_millis();

        let count: i64 =
            sqlx::query_scalar("SELECT COUNT(*) FROM background_update_locks WHERE lock_name = $1 AND expires_at > $2")
                .bind(job_name)
                .bind(now)
                .fetch_one(&*self.pool)
                .await?;

        Ok(count > 0)
    }

    pub async fn cleanup_expired_locks(&self) -> Result<i64, sqlx::Error> {
        let now = Utc::now().timestamp_millis();

        let result = sqlx::query("DELETE FROM background_update_locks WHERE expires_at < $1")
            .bind(now)
            .execute(&*self.pool)
            .await?;

        Ok(result.rows_affected() as i64)
    }

    pub async fn add_history(
        &self,
        job_name: &str,
        status: &str,
        items_processed: i32,
        error_message: Option<&str>,
        metadata: Option<serde_json::Value>,
    ) -> Result<BackgroundUpdateHistory, sqlx::Error> {
        let now = Utc::now().timestamp_millis();

        let row = sqlx::query_as::<_, BackgroundUpdateHistory>(
            r"
            INSERT INTO background_update_history (
                job_name, execution_start_ts, execution_end_ts, status, items_processed, error_message, metadata
            )
            VALUES ($1, $2, $3, $4, $5, $6, $7)
            RETURNING *
            ",
        )
        .bind(job_name)
        .bind(now)
        .bind(now)
        .bind(status)
        .bind(items_processed)
        .bind(error_message)
        .bind(metadata)
        .fetch_one(&*self.pool)
        .await?;

        Ok(row)
    }

    pub async fn get_history(&self, job_name: &str, limit: i64) -> Result<Vec<BackgroundUpdateHistory>, sqlx::Error> {
        let rows = sqlx::query_as::<_, BackgroundUpdateHistory>(
            "SELECT id, job_name, execution_start_ts, execution_end_ts, status, items_processed, error_message, metadata FROM background_update_history WHERE job_name = $1 ORDER BY execution_start_ts DESC LIMIT $2",
        )
        .bind(job_name)
        .bind(limit)
        .fetch_all(&*self.pool)
        .await?;

        Ok(rows)
    }

    pub async fn retry_failed(&self) -> Result<i64, sqlx::Error> {
        let result = sqlx::query(
            r"
            UPDATE background_updates SET
                status = 'pending',
                error_message = NULL,
                retry_count = retry_count + 1
            WHERE status = 'failed' AND retry_count < max_retries
            ",
        )
        .execute(&*self.pool)
        .await?;

        Ok(result.rows_affected() as i64)
    }

    pub async fn count_by_status(&self, status: &str) -> Result<i64, sqlx::Error> {
        let count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM background_updates WHERE status = $1")
            .bind(status)
            .fetch_one(&*self.pool)
            .await?;

        Ok(count)
    }

    pub async fn count_all(&self) -> Result<i64, sqlx::Error> {
        let count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM background_updates").fetch_one(&*self.pool).await?;

        Ok(count)
    }

    pub async fn get_stats(&self, limit: i32) -> Result<Vec<BackgroundUpdateStats>, sqlx::Error> {
        let rows = sqlx::query_as::<_, BackgroundUpdateStats>(
            "SELECT id, job_name, total_updates, completed_updates, failed_updates, last_run_ts, next_run_ts, average_duration_ms, created_ts, updated_ts FROM background_update_stats ORDER BY created_ts DESC LIMIT $1",
        )
        .bind(limit as i64)
        .fetch_all(&*self.pool)
        .await?;

        Ok(rows)
    }
}

#[async_trait]
impl BackgroundUpdateStoreApi for BackgroundUpdateStorage {
    async fn create_update(&self, request: CreateBackgroundUpdateRequest) -> Result<BackgroundUpdate, sqlx::Error> {
        self.create_update(request).await
    }
    async fn get_update(&self, job_name: &str) -> Result<Option<BackgroundUpdate>, sqlx::Error> {
        self.get_update(job_name).await
    }
    async fn get_all_updates(
        &self,
        limit: i64,
        from: Option<String>,
    ) -> Result<(Vec<BackgroundUpdate>, Option<String>), sqlx::Error> {
        self.get_all_updates(limit, from).await
    }
    async fn get_pending_updates(&self) -> Result<Vec<BackgroundUpdate>, sqlx::Error> {
        self.get_pending_updates().await
    }
    async fn get_running_updates(&self) -> Result<Vec<BackgroundUpdate>, sqlx::Error> {
        self.get_running_updates().await
    }
    async fn update_status(&self, job_name: &str, status: &str) -> Result<BackgroundUpdate, sqlx::Error> {
        self.update_status(job_name, status).await
    }
    async fn update_progress(
        &self,
        job_name: &str,
        items_processed: i32,
        total_items: Option<i32>,
    ) -> Result<BackgroundUpdate, sqlx::Error> {
        self.update_progress(job_name, items_processed, total_items).await
    }
    async fn set_error(&self, job_name: &str, error_message: &str) -> Result<BackgroundUpdate, sqlx::Error> {
        self.set_error(job_name, error_message).await
    }
    async fn delete_update(&self, job_name: &str) -> Result<(), sqlx::Error> {
        self.delete_update(job_name).await
    }
    async fn acquire_lock_with_retry(
        &self,
        job_name: &str,
        locked_by: &str,
        lock_duration_ms: i64,
        max_retries: u32,
        max_retry_interval_ms: u64,
    ) -> Result<bool, sqlx::Error> {
        self.acquire_lock_with_retry(job_name, locked_by, lock_duration_ms, max_retries, max_retry_interval_ms).await
    }
    async fn release_lock(&self, job_name: &str) -> Result<(), sqlx::Error> {
        self.release_lock(job_name).await
    }
    async fn is_locked(&self, job_name: &str) -> Result<bool, sqlx::Error> {
        self.is_locked(job_name).await
    }
    async fn cleanup_expired_locks(&self) -> Result<i64, sqlx::Error> {
        self.cleanup_expired_locks().await
    }
    async fn add_history(
        &self,
        job_name: &str,
        status: &str,
        items_processed: i32,
        error_message: Option<&str>,
        metadata: Option<serde_json::Value>,
    ) -> Result<BackgroundUpdateHistory, sqlx::Error> {
        self.add_history(job_name, status, items_processed, error_message, metadata).await
    }
    async fn get_history(&self, job_name: &str, limit: i64) -> Result<Vec<BackgroundUpdateHistory>, sqlx::Error> {
        self.get_history(job_name, limit).await
    }
    async fn retry_failed(&self) -> Result<i64, sqlx::Error> {
        self.retry_failed().await
    }
    async fn count_by_status(&self, status: &str) -> Result<i64, sqlx::Error> {
        self.count_by_status(status).await
    }
    async fn count_all(&self) -> Result<i64, sqlx::Error> {
        self.count_all().await
    }
    async fn get_stats(&self, limit: i32) -> Result<Vec<BackgroundUpdateStats>, sqlx::Error> {
        self.get_stats(limit).await
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_background_update_creation() {
        let update = BackgroundUpdate {
            job_name: "update_user_indices".to_string(),
            job_type: "index_update".to_string(),
            description: Some("Update user indices".to_string()),
            table_name: Some("users".to_string()),
            column_name: None,
            status: "running".to_string(),
            progress: serde_json::json!(50),
            total_items: 100,
            processed_items: 50,
            created_ts: Some(1234567800),
            started_ts: Some(1234567800),
            completed_ts: None,
            updated_ts: Some(1234567890),
            error_message: None,
            retry_count: 0,
            max_retries: 3,
            batch_size: 1000,
            sleep_ms: 100,
            depends_on: None,
            metadata: None,
        };
        assert_eq!(update.job_name, "update_user_indices");
        assert_eq!(update.progress, serde_json::json!(50));
    }

    #[test]
    fn test_background_update_completed() {
        let update = BackgroundUpdate {
            job_name: "vacuum_tables".to_string(),
            job_type: "vacuum".to_string(),
            description: None,
            table_name: Some("events".to_string()),
            column_name: None,
            status: "completed".to_string(),
            progress: serde_json::json!(100),
            total_items: 100,
            processed_items: 100,
            created_ts: Some(1234567800),
            started_ts: Some(1234567800),
            completed_ts: Some(1234567890),
            updated_ts: Some(1234567890),
            error_message: None,
            retry_count: 0,
            max_retries: 3,
            batch_size: 1000,
            sleep_ms: 100,
            depends_on: None,
            metadata: None,
        };
        assert_eq!(update.status, "completed");
    }

    #[test]
    fn test_background_update_history_creation() {
        let history = BackgroundUpdateHistory {
            id: 1,
            job_name: "update_user_indices".to_string(),
            execution_start_ts: 1234567800,
            execution_end_ts: Some(1234567890),
            status: "completed".to_string(),
            items_processed: 100,
            error_message: None,
            metadata: None,
        };
        assert_eq!(history.status, "completed");
    }

    #[test]
    fn test_background_update_lock_creation() {
        let lock = BackgroundUpdateLock {
            lock_name: "update_lock".to_string(),
            owner: Some("@admin:example.com".to_string()),
            acquired_ts: 1234567890,
            expires_at: 1234568190,
        };
        assert_eq!(lock.lock_name, "update_lock");
    }

    #[test]
    fn test_create_background_update_request() {
        let request = CreateBackgroundUpdateRequest {
            job_name: "new_update".to_string(),
            job_type: "custom".to_string(),
            description: Some("New update job".to_string()),
            table_name: None,
            column_name: None,
            total_items: Some(1000),
            batch_size: Some(1000),
            sleep_ms: Some(100),
            depends_on: None,
            metadata: None,
        };
        assert_eq!(request.job_name, "new_update");
    }

    #[test]
    fn test_update_background_update_request() {
        let request = UpdateBackgroundUpdateRequest {
            status: Some("running".to_string()),
            progress: Some(75),
            total_items: Some(100),
            processed_items: Some(75),
            error_message: None,
        };
        assert!(request.status.is_some());
    }

    // ===== Database-dependent tests =====

    use std::sync::atomic::{AtomicU64, Ordering};

    static BU_TEST_COUNTER: AtomicU64 = AtomicU64::new(1);

    fn bu_unique_suffix() -> u64 {
        BU_TEST_COUNTER.fetch_add(1, Ordering::SeqCst)
    }

    async fn setup_background_update_db(pool: &Arc<PgPool>) {
        sqlx::query(
            r#"
            CREATE TABLE IF NOT EXISTS background_updates (
                id BIGSERIAL PRIMARY KEY,
                update_name TEXT,
                job_name TEXT,
                job_type TEXT,
                description TEXT,
                table_name TEXT,
                column_name TEXT,
                is_running BOOLEAN DEFAULT FALSE,
                status TEXT DEFAULT 'pending',
                progress JSONB DEFAULT '{}',
                total_items INTEGER DEFAULT 0,
                processed_items INTEGER DEFAULT 0,
                created_ts BIGINT NOT NULL,
                started_ts BIGINT,
                completed_ts BIGINT,
                updated_ts BIGINT,
                error_message TEXT,
                retry_count INTEGER DEFAULT 0,
                max_retries INTEGER DEFAULT 3,
                batch_size INTEGER DEFAULT 100,
                sleep_ms INTEGER DEFAULT 100,
                depends_on JSONB DEFAULT '[]',
                metadata JSONB DEFAULT '{}'
            )
            "#,
        )
        .execute(&**pool)
        .await
        .expect("Failed to create background_updates table");

        sqlx::query(
            r#"
            CREATE TABLE IF NOT EXISTS background_update_locks (
                lock_name TEXT PRIMARY KEY,
                owner TEXT,
                acquired_ts BIGINT NOT NULL,
                expires_at BIGINT NOT NULL
            )
            "#,
        )
        .execute(&**pool)
        .await
        .expect("Failed to create background_update_locks table");

        sqlx::query(
            r#"
            CREATE TABLE IF NOT EXISTS background_update_history (
                id BIGSERIAL PRIMARY KEY,
                job_name TEXT NOT NULL,
                execution_start_ts BIGINT NOT NULL,
                execution_end_ts BIGINT,
                status TEXT NOT NULL,
                items_processed INTEGER NOT NULL DEFAULT 0,
                error_message TEXT,
                metadata JSONB
            )
            "#,
        )
        .execute(&**pool)
        .await
        .expect("Failed to create background_update_history table");

        sqlx::query(
            r#"
            CREATE TABLE IF NOT EXISTS background_update_stats (
                id BIGSERIAL PRIMARY KEY,
                job_name TEXT NOT NULL,
                total_updates INTEGER NOT NULL DEFAULT 0,
                completed_updates INTEGER NOT NULL DEFAULT 0,
                failed_updates INTEGER NOT NULL DEFAULT 0,
                last_run_ts BIGINT,
                next_run_ts BIGINT,
                average_duration_ms BIGINT NOT NULL DEFAULT 0,
                created_ts BIGINT NOT NULL,
                updated_ts BIGINT NOT NULL
            )
            "#,
        )
        .execute(&**pool)
        .await
        .expect("Failed to create background_update_stats table");
    }

    async fn insert_update_row(
        pool: &PgPool,
        update_name: &str,
        status: &str,
        created_ts: i64,
        total_items: i32,
        processed_items: i32,
        retry_count: i32,
        max_retries: i32,
    ) {
        sqlx::query(
            r#"
            INSERT INTO background_updates (
                update_name, job_name, job_type, status, created_ts, progress,
                total_items, processed_items, retry_count, max_retries, batch_size, sleep_ms,
                depends_on, metadata
            )
            VALUES ($1, $1, 'test_type', $2, $3, '{}', $4, $5, $6, $7, 100, 100, '[]', '{}')
            "#,
        )
        .bind(update_name)
        .bind(status)
        .bind(created_ts)
        .bind(total_items)
        .bind(processed_items)
        .bind(retry_count)
        .bind(max_retries)
        .execute(pool)
        .await
        .expect("Failed to insert test update row");
    }

    async fn get_bu_test_pool() -> Option<Arc<PgPool>> {
        match crate::test_utils::prepare_empty_isolated_test_pool().await {
            Ok(pool) => Some(pool),
            Err(error) => {
                tracing::warn!("Skipping background_update DB test because test database is unavailable: {error}");
                None
            }
        }
    }

    #[tokio::test]
    async fn test_db_create_update() {
        let pool = match get_bu_test_pool().await {
            Some(p) => p,
            None => return,
        };
        setup_background_update_db(&pool).await;

        let storage = BackgroundUpdateStorage::new(&pool);
        let suffix = bu_unique_suffix();
        let request = CreateBackgroundUpdateRequest {
            job_name: format!("job_{suffix}"),
            job_type: "migration".to_string(),
            description: Some("Test migration".to_string()),
            table_name: Some("events".to_string()),
            column_name: None,
            total_items: Some(1000),
            batch_size: Some(500),
            sleep_ms: Some(200),
            depends_on: None,
            metadata: None,
        };

        let update = storage.create_update(request).await.expect("Failed to create update");

        assert_eq!(update.job_name, format!("job_{suffix}"));
        assert_eq!(update.job_type, "migration");
        assert_eq!(update.status, "pending");
        assert_eq!(update.total_items, 1000);
        assert_eq!(update.batch_size, 500);
        assert_eq!(update.sleep_ms, 200);
        assert_eq!(update.max_retries, 3);
        assert_eq!(update.processed_items, 0);
        assert!(update.created_ts.unwrap_or(0) > 0);
    }

    #[tokio::test]
    async fn test_db_create_update_defaults() {
        let pool = match get_bu_test_pool().await {
            Some(p) => p,
            None => return,
        };
        setup_background_update_db(&pool).await;

        let storage = BackgroundUpdateStorage::new(&pool);
        let suffix = bu_unique_suffix();
        let request = CreateBackgroundUpdateRequest {
            job_name: format!("job_{suffix}"),
            job_type: "custom".to_string(),
            description: None,
            table_name: None,
            column_name: None,
            total_items: None,
            batch_size: None,
            sleep_ms: None,
            depends_on: None,
            metadata: None,
        };

        let update = storage.create_update(request).await.expect("Failed to create update with defaults");

        assert_eq!(update.total_items, 0);
        assert_eq!(update.batch_size, 100);
        assert_eq!(update.sleep_ms, 1000);
        assert_eq!(update.max_retries, 3);
    }

    #[tokio::test]
    async fn test_db_get_update_found_and_missing() {
        let pool = match get_bu_test_pool().await {
            Some(p) => p,
            None => return,
        };
        setup_background_update_db(&pool).await;

        let storage = BackgroundUpdateStorage::new(&pool);
        let now = chrono::Utc::now().timestamp_millis();
        insert_update_row(&pool, "test_job_1", "pending", now, 100, 0, 0, 3).await;

        let found = storage.get_update("test_job_1").await.expect("Failed to get update");
        assert!(found.is_some());
        let found = found.unwrap();
        assert_eq!(found.job_name, "test_job_1");
        assert_eq!(found.status, "pending");

        let missing = storage.get_update("nonexistent_job").await.expect("Failed to query missing");
        assert!(missing.is_none());
    }

    #[tokio::test]
    async fn test_db_get_all_updates_empty() {
        let pool = match get_bu_test_pool().await {
            Some(p) => p,
            None => return,
        };
        setup_background_update_db(&pool).await;

        let storage = BackgroundUpdateStorage::new(&pool);
        let (rows, next) = storage.get_all_updates(10, None).await.expect("Failed to get all updates");

        assert!(rows.is_empty());
        assert!(next.is_none());
    }

    #[tokio::test]
    async fn test_db_get_all_updates_pagination() {
        let pool = match get_bu_test_pool().await {
            Some(p) => p,
            None => return,
        };
        setup_background_update_db(&pool).await;

        let storage = BackgroundUpdateStorage::new(&pool);
        let base_ts = 2_000_000_000_000i64;

        // Insert 3 rows with distinct timestamps
        for i in 0..3i64 {
            insert_update_row(&pool, &format!("job_{i}"), "pending", base_ts + i * 1000, 100, 0, 0, 3).await;
        }

        // First page: limit 2
        let (rows, next) = storage.get_all_updates(2, None).await.expect("Failed to get first page");
        assert_eq!(rows.len(), 2);
        assert!(next.is_some());
        // ORDER BY created_ts DESC — newest first
        assert_eq!(rows[0].job_name, "job_2");
        assert_eq!(rows[1].job_name, "job_1");

        // Second page: use cursor
        let (rows2, next2) = storage.get_all_updates(2, next).await.expect("Failed to get second page");
        assert_eq!(rows2.len(), 1);
        assert!(next2.is_none());
        assert_eq!(rows2[0].job_name, "job_0");
    }

    #[tokio::test]
    async fn test_db_get_updates_by_status() {
        let pool = match get_bu_test_pool().await {
            Some(p) => p,
            None => return,
        };
        setup_background_update_db(&pool).await;

        let storage = BackgroundUpdateStorage::new(&pool);
        let now = chrono::Utc::now().timestamp_millis();

        insert_update_row(&pool, "pending_1", "pending", now, 100, 0, 0, 3).await;
        insert_update_row(&pool, "running_1", "running", now, 100, 50, 0, 3).await;
        insert_update_row(&pool, "pending_2", "pending", now, 200, 0, 0, 3).await;

        let pending = storage.get_updates_by_status("pending").await.expect("Failed to get pending");
        assert_eq!(pending.len(), 2);

        let running = storage.get_updates_by_status("running").await.expect("Failed to get running");
        assert_eq!(running.len(), 1);

        let completed = storage.get_updates_by_status("completed").await.expect("Failed to get completed");
        assert!(completed.is_empty());
    }

    #[tokio::test]
    async fn test_db_get_pending_and_running_updates() {
        let pool = match get_bu_test_pool().await {
            Some(p) => p,
            None => return,
        };
        setup_background_update_db(&pool).await;

        let storage = BackgroundUpdateStorage::new(&pool);
        let now = chrono::Utc::now().timestamp_millis();

        insert_update_row(&pool, "p1", "pending", now, 100, 0, 0, 3).await;
        insert_update_row(&pool, "r1", "running", now, 100, 50, 0, 3).await;

        let pending = storage.get_pending_updates().await.expect("Failed to get pending");
        assert_eq!(pending.len(), 1);
        assert_eq!(pending[0].job_name, "p1");

        let running = storage.get_running_updates().await.expect("Failed to get running");
        assert_eq!(running.len(), 1);
        assert_eq!(running[0].job_name, "r1");
    }

    #[tokio::test]
    async fn test_db_update_status_running() {
        let pool = match get_bu_test_pool().await {
            Some(p) => p,
            None => return,
        };
        setup_background_update_db(&pool).await;

        let storage = BackgroundUpdateStorage::new(&pool);
        let now = chrono::Utc::now().timestamp_millis();
        insert_update_row(&pool, "status_job", "pending", now, 100, 0, 0, 3).await;

        let updated = storage.update_status("status_job", "running").await.expect("Failed to update status to running");

        assert_eq!(updated.status, "running");
        assert!(updated.started_ts.is_some());
        assert!(updated.updated_ts.is_some());
    }

    #[tokio::test]
    async fn test_db_update_status_completed() {
        let pool = match get_bu_test_pool().await {
            Some(p) => p,
            None => return,
        };
        setup_background_update_db(&pool).await;

        let storage = BackgroundUpdateStorage::new(&pool);
        let now = chrono::Utc::now().timestamp_millis();
        insert_update_row(&pool, "complete_job", "running", now, 100, 100, 0, 3).await;

        let updated =
            storage.update_status("complete_job", "completed").await.expect("Failed to update status to completed");

        assert_eq!(updated.status, "completed");
        assert!(updated.completed_ts.is_some());
    }

    #[tokio::test]
    async fn test_db_update_progress() {
        let pool = match get_bu_test_pool().await {
            Some(p) => p,
            None => return,
        };
        setup_background_update_db(&pool).await;

        let storage = BackgroundUpdateStorage::new(&pool);
        let now = chrono::Utc::now().timestamp_millis();
        insert_update_row(&pool, "progress_job", "running", now, 100, 0, 0, 3).await;

        let updated = storage.update_progress("progress_job", 25, Some(100)).await.expect("Failed to update progress");

        assert_eq!(updated.processed_items, 25);
        assert_eq!(updated.total_items, 100);

        // Update again without total_items (should keep existing total)
        let updated2 =
            storage.update_progress("progress_job", 25, None).await.expect("Failed to update progress again");
        assert_eq!(updated2.processed_items, 50);
        assert_eq!(updated2.total_items, 100);
    }

    #[tokio::test]
    async fn test_db_set_error() {
        let pool = match get_bu_test_pool().await {
            Some(p) => p,
            None => return,
        };
        setup_background_update_db(&pool).await;

        let storage = BackgroundUpdateStorage::new(&pool);
        let now = chrono::Utc::now().timestamp_millis();
        insert_update_row(&pool, "error_job", "running", now, 100, 50, 0, 3).await;

        let updated = storage.set_error("error_job", "DB connection lost").await.expect("Failed to set error");

        assert_eq!(updated.status, "failed");
        assert_eq!(updated.error_message.as_deref(), Some("DB connection lost"));
        assert_eq!(updated.retry_count, 1);
    }

    #[tokio::test]
    async fn test_db_delete_update() {
        let pool = match get_bu_test_pool().await {
            Some(p) => p,
            None => return,
        };
        setup_background_update_db(&pool).await;

        let storage = BackgroundUpdateStorage::new(&pool);
        let now = chrono::Utc::now().timestamp_millis();
        insert_update_row(&pool, "delete_job", "completed", now, 100, 100, 0, 3).await;

        storage.delete_update("delete_job").await.expect("Failed to delete update");

        let after = storage.get_update("delete_job").await.expect("Failed to get update");
        assert!(after.is_none());
    }

    #[tokio::test]
    async fn test_db_acquire_lock_success() {
        let pool = match get_bu_test_pool().await {
            Some(p) => p,
            None => return,
        };
        setup_background_update_db(&pool).await;

        let storage = BackgroundUpdateStorage::new(&pool);

        let acquired = storage.acquire_lock("lock_1", "worker_1", 60_000).await.expect("Failed to acquire lock");
        assert!(acquired);

        let is_locked = storage.is_locked("lock_1").await.expect("Failed to check lock");
        assert!(is_locked);
    }

    #[tokio::test]
    async fn test_db_acquire_lock_contention() {
        let pool = match get_bu_test_pool().await {
            Some(p) => p,
            None => return,
        };
        setup_background_update_db(&pool).await;

        let storage = BackgroundUpdateStorage::new(&pool);

        // First worker acquires the lock
        let first = storage.acquire_lock("lock_2", "worker_1", 60_000).await.expect("Failed to acquire lock");
        assert!(first);

        // Second worker tries while lock is active → should fail
        let second = storage.acquire_lock("lock_2", "worker_2", 60_000).await.expect("Failed to acquire lock");
        assert!(!second);
    }

    #[tokio::test]
    async fn test_db_acquire_lock_after_expiry() {
        let pool = match get_bu_test_pool().await {
            Some(p) => p,
            None => return,
        };
        setup_background_update_db(&pool).await;

        let storage = BackgroundUpdateStorage::new(&pool);

        // Acquire with very short duration
        let first = storage.acquire_lock("lock_3", "worker_1", 1).await.expect("Failed to acquire lock");
        assert!(first);

        // Wait for lock to expire
        tokio::time::sleep(std::time::Duration::from_millis(50)).await;

        // Second worker should now acquire the expired lock
        let second = storage.acquire_lock("lock_3", "worker_2", 60_000).await.expect("Failed to acquire lock");
        assert!(second);
    }

    #[tokio::test]
    async fn test_db_acquire_lock_with_retry_success() {
        let pool = match get_bu_test_pool().await {
            Some(p) => p,
            None => return,
        };
        setup_background_update_db(&pool).await;

        let storage = BackgroundUpdateStorage::new(&pool);

        // No existing lock → should succeed on first attempt
        let acquired = storage
            .acquire_lock_with_retry("lock_retry_1", "worker_1", 60_000, 3, 100)
            .await
            .expect("Failed to acquire lock with retry");
        assert!(acquired);
    }

    #[tokio::test]
    async fn test_db_acquire_lock_with_retry_exhausted() {
        let pool = match get_bu_test_pool().await {
            Some(p) => p,
            None => return,
        };
        setup_background_update_db(&pool).await;

        let storage = BackgroundUpdateStorage::new(&pool);

        // Pre-acquire the lock with a long duration
        storage.acquire_lock("lock_retry_2", "holder", 300_000).await.expect("Failed to pre-acquire lock");

        // Retry should exhaust all attempts and return false
        let acquired = storage
            .acquire_lock_with_retry("lock_retry_2", "worker_2", 60_000, 2, 10)
            .await
            .expect("Failed to acquire lock with retry");
        assert!(!acquired);
    }

    #[tokio::test]
    async fn test_db_release_and_is_locked() {
        let pool = match get_bu_test_pool().await {
            Some(p) => p,
            None => return,
        };
        setup_background_update_db(&pool).await;

        let storage = BackgroundUpdateStorage::new(&pool);

        storage.acquire_lock("lock_4", "worker_1", 60_000).await.expect("Failed to acquire lock");
        assert!(storage.is_locked("lock_4").await.expect("Failed to check lock"));

        storage.release_lock("lock_4").await.expect("Failed to release lock");
        assert!(!storage.is_locked("lock_4").await.expect("Failed to check lock"));

        // Releasing non-existent lock should not error
        storage.release_lock("nonexistent").await.expect("Releasing nonexistent should not error");
    }

    #[tokio::test]
    async fn test_db_is_locked_expired() {
        let pool = match get_bu_test_pool().await {
            Some(p) => p,
            None => return,
        };
        setup_background_update_db(&pool).await;

        let storage = BackgroundUpdateStorage::new(&pool);

        // Acquire with very short duration
        storage.acquire_lock("lock_5", "worker_1", 1).await.expect("Failed to acquire lock");

        // Wait for expiry
        tokio::time::sleep(std::time::Duration::from_millis(50)).await;

        // Expired lock should not be considered locked
        let is_locked = storage.is_locked("lock_5").await.expect("Failed to check lock");
        assert!(!is_locked);
    }

    #[tokio::test]
    async fn test_db_cleanup_expired_locks() {
        let pool = match get_bu_test_pool().await {
            Some(p) => p,
            None => return,
        };
        setup_background_update_db(&pool).await;

        let storage = BackgroundUpdateStorage::new(&pool);

        // Acquire an expired lock (duration 1ms)
        storage.acquire_lock("expired_lock", "worker", 1).await.expect("Failed to acquire lock");
        // Acquire an active lock
        storage.acquire_lock("active_lock", "worker", 300_000).await.expect("Failed to acquire lock");

        // Wait for the first to expire
        tokio::time::sleep(std::time::Duration::from_millis(50)).await;

        let deleted = storage.cleanup_expired_locks().await.expect("Failed to cleanup expired locks");
        assert_eq!(deleted, 1);

        // Active lock should still be locked
        assert!(storage.is_locked("active_lock").await.expect("Failed to check lock"));
        assert!(!storage.is_locked("expired_lock").await.expect("Failed to check lock"));
    }

    #[tokio::test]
    async fn test_db_add_and_get_history() {
        let pool = match get_bu_test_pool().await {
            Some(p) => p,
            None => return,
        };
        setup_background_update_db(&pool).await;

        let storage = BackgroundUpdateStorage::new(&pool);

        let h1 = storage.add_history("hist_job", "completed", 100, None, None).await.expect("Failed to add history 1");
        assert_eq!(h1.job_name, "hist_job");
        assert_eq!(h1.status, "completed");
        assert_eq!(h1.items_processed, 100);

        let h2 = storage
            .add_history("hist_job", "failed", 50, Some("connection error"), Some(serde_json::json!({"retry": 3})))
            .await
            .expect("Failed to add history 2");
        assert_eq!(h2.status, "failed");
        assert_eq!(h2.error_message.as_deref(), Some("connection error"));

        let history = storage.get_history("hist_job", 10).await.expect("Failed to get history");
        assert_eq!(history.len(), 2);
        // ORDER BY execution_start_ts DESC — most recent first
        assert_eq!(history[0].status, "failed");
        assert_eq!(history[1].status, "completed");

        // Test limit
        let limited = storage.get_history("hist_job", 1).await.expect("Failed to get limited history");
        assert_eq!(limited.len(), 1);

        // Empty history for non-existent job
        let empty = storage.get_history("nonexistent", 10).await.expect("Failed to get empty history");
        assert!(empty.is_empty());
    }

    #[tokio::test]
    async fn test_db_retry_failed() {
        let pool = match get_bu_test_pool().await {
            Some(p) => p,
            None => return,
        };
        setup_background_update_db(&pool).await;

        let storage = BackgroundUpdateStorage::new(&pool);
        let now = chrono::Utc::now().timestamp_millis();

        // Failed with retry_count 0, max_retries 3 → should be retried
        insert_update_row(&pool, "retryable_job", "failed", now, 100, 50, 0, 3).await;
        // Failed with retry_count 3, max_retries 3 → should NOT be retried
        insert_update_row(&pool, "exhausted_job", "failed", now, 100, 50, 3, 3).await;

        let count = storage.retry_failed().await.expect("Failed to retry failed updates");
        assert_eq!(count, 1);

        // Verify retryable_job is now pending with retry_count = 1
        let retried = storage.get_update("retryable_job").await.expect("Failed to get update").unwrap();
        assert_eq!(retried.status, "pending");
        assert_eq!(retried.retry_count, 1);
        assert!(retried.error_message.is_none());

        // Verify exhausted_job is still failed
        let exhausted = storage.get_update("exhausted_job").await.expect("Failed to get update").unwrap();
        assert_eq!(exhausted.status, "failed");
        assert_eq!(exhausted.retry_count, 3);
    }

    #[tokio::test]
    async fn test_db_count_by_status_and_all() {
        let pool = match get_bu_test_pool().await {
            Some(p) => p,
            None => return,
        };
        setup_background_update_db(&pool).await;

        let storage = BackgroundUpdateStorage::new(&pool);
        let now = chrono::Utc::now().timestamp_millis();

        insert_update_row(&pool, "p1", "pending", now, 100, 0, 0, 3).await;
        insert_update_row(&pool, "p2", "pending", now, 100, 0, 0, 3).await;
        insert_update_row(&pool, "r1", "running", now, 100, 50, 0, 3).await;

        let pending_count = storage.count_by_status("pending").await.expect("Failed to count pending");
        assert_eq!(pending_count, 2);

        let running_count = storage.count_by_status("running").await.expect("Failed to count running");
        assert_eq!(running_count, 1);

        let total = storage.count_all().await.expect("Failed to count all");
        assert_eq!(total, 3);

        let completed_count = storage.count_by_status("completed").await.expect("Failed to count completed");
        assert_eq!(completed_count, 0);
    }

    #[tokio::test]
    async fn test_db_get_stats_empty() {
        let pool = match get_bu_test_pool().await {
            Some(p) => p,
            None => return,
        };
        setup_background_update_db(&pool).await;

        let storage = BackgroundUpdateStorage::new(&pool);
        let stats = storage.get_stats(10).await.expect("Failed to get stats");
        assert!(stats.is_empty());
    }

    #[tokio::test]
    async fn test_db_get_stats_with_data() {
        let pool = match get_bu_test_pool().await {
            Some(p) => p,
            None => return,
        };
        setup_background_update_db(&pool).await;

        let storage = BackgroundUpdateStorage::new(&pool);
        let now = chrono::Utc::now().timestamp_millis();

        sqlx::query(
            r#"INSERT INTO background_update_stats (
                job_name, total_updates, completed_updates, failed_updates,
                last_run_ts, next_run_ts, average_duration_ms, created_ts, updated_ts
            ) VALUES ($1, 10, 8, 2, $2, $3, 5000, $2, $2)"#,
        )
        .bind("stats_job")
        .bind(now)
        .bind(now + 3_600_000)
        .execute(pool.as_ref())
        .await
        .expect("Failed to insert stats row");

        let stats = storage.get_stats(10).await.expect("Failed to get stats");
        assert_eq!(stats.len(), 1);
        assert_eq!(stats[0].job_name, "stats_job");
        assert_eq!(stats[0].total_updates, 10);
        assert_eq!(stats[0].completed_updates, 8);
        assert_eq!(stats[0].failed_updates, 2);
    }

    #[tokio::test]
    async fn test_db_store_api_trait_impl() {
        let pool = match get_bu_test_pool().await {
            Some(p) => p,
            None => return,
        };
        setup_background_update_db(&pool).await;

        let storage = BackgroundUpdateStorage::new(&pool);
        let suffix = bu_unique_suffix();

        // Test trait method create_update
        let request = CreateBackgroundUpdateRequest {
            job_name: format!("trait_job_{suffix}"),
            job_type: "test".to_string(),
            description: None,
            table_name: None,
            column_name: None,
            total_items: Some(100),
            batch_size: None,
            sleep_ms: None,
            depends_on: None,
            metadata: None,
        };
        let update =
            BackgroundUpdateStoreApi::create_update(&storage, request).await.expect("trait create_update failed");
        assert_eq!(update.status, "pending");

        // Test trait method count_all
        let total = BackgroundUpdateStoreApi::count_all(&storage).await.expect("trait count_all failed");
        assert!(total >= 1);

        // Test trait method delete_update — note: create_update doesn't set update_name,
        // so we need to manually set it for delete to work
        sqlx::query("UPDATE background_updates SET update_name = job_name WHERE update_name IS NULL")
            .execute(pool.as_ref())
            .await
            .expect("Failed to set update_name");
        BackgroundUpdateStoreApi::delete_update(&storage, &format!("trait_job_{suffix}"))
            .await
            .expect("trait delete_update failed");

        let after = BackgroundUpdateStoreApi::get_update(&storage, &format!("trait_job_{suffix}"))
            .await
            .expect("trait get_update failed");
        assert!(after.is_none());
    }
}
