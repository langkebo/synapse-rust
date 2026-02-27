use chrono::Utc;
use serde::{Deserialize, Serialize};
use sqlx::{FromRow, PgPool};
use std::sync::Arc;

#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
pub struct BackgroundUpdate {
    pub job_name: String,
    pub job_type: String,
    pub description: Option<String>,
    pub table_name: Option<String>,
    pub column_name: Option<String>,
    pub status: String,
    pub progress: i32,
    pub total_items: i32,
    pub processed_items: i32,
    pub created_ts: i64,
    pub started_ts: Option<i64>,
    pub completed_ts: Option<i64>,
    pub last_updated_ts: Option<i64>,
    pub error_message: Option<String>,
    pub retry_count: i32,
    pub max_retries: i32,
    pub batch_size: i32,
    pub sleep_ms: i32,
    pub depends_on: Option<Vec<String>>,
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
    pub job_name: String,
    pub locked_by: Option<String>,
    pub locked_ts: i64,
    pub expires_ts: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
pub struct BackgroundUpdateStats {
    pub id: i64,
    pub stat_date: chrono::NaiveDate,
    pub total_jobs: i32,
    pub completed_jobs: i32,
    pub failed_jobs: i32,
    pub total_items_processed: i64,
    pub total_execution_time_ms: i64,
    pub avg_items_per_second: Option<f64>,
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

#[derive(Clone)]
pub struct BackgroundUpdateStorage {
    pool: Arc<PgPool>,
}

impl BackgroundUpdateStorage {
    pub fn new(pool: &Arc<PgPool>) -> Self {
        Self { pool: pool.clone() }
    }

    pub async fn create_update(
        &self,
        request: CreateBackgroundUpdateRequest,
    ) -> Result<BackgroundUpdate, sqlx::Error> {
        let now = Utc::now().timestamp_millis();

        let row = sqlx::query_as::<_, BackgroundUpdate>(
            r#"
            INSERT INTO background_updates (
                job_name, job_type, description, table_name, column_name, total_items,
                batch_size, sleep_ms, depends_on, metadata, created_ts, status, max_retries
            )
            VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, 'pending', 3)
            RETURNING *
            "#,
        )
        .bind(&request.job_name)
        .bind(&request.job_type)
        .bind(&request.description)
        .bind(&request.table_name)
        .bind(&request.column_name)
        .bind(request.total_items.unwrap_or(0))
        .bind(request.batch_size.unwrap_or(100))
        .bind(request.sleep_ms.unwrap_or(1000))
        .bind(&request.depends_on)
        .bind(&request.metadata)
        .bind(now)
        .fetch_one(&*self.pool)
        .await?;

        Ok(row)
    }

    pub async fn get_update(
        &self,
        job_name: &str,
    ) -> Result<Option<BackgroundUpdate>, sqlx::Error> {
        let row = sqlx::query_as::<_, BackgroundUpdate>(
            "SELECT * FROM background_updates WHERE job_name = $1",
        )
        .bind(job_name)
        .fetch_optional(&*self.pool)
        .await?;

        Ok(row)
    }

    pub async fn get_all_updates(
        &self,
        limit: i64,
        offset: i64,
    ) -> Result<Vec<BackgroundUpdate>, sqlx::Error> {
        let rows = sqlx::query_as::<_, BackgroundUpdate>(
            "SELECT * FROM background_updates ORDER BY created_ts DESC LIMIT $1 OFFSET $2",
        )
        .bind(limit)
        .bind(offset)
        .fetch_all(&*self.pool)
        .await?;

        Ok(rows)
    }

    pub async fn get_updates_by_status(
        &self,
        status: &str,
    ) -> Result<Vec<BackgroundUpdate>, sqlx::Error> {
        let rows = sqlx::query_as::<_, BackgroundUpdate>(
            "SELECT * FROM background_updates WHERE status = $1 ORDER BY created_ts ASC",
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

    pub async fn update_status(
        &self,
        job_name: &str,
        status: &str,
    ) -> Result<BackgroundUpdate, sqlx::Error> {
        let now = Utc::now().timestamp_millis();

        let started_ts = if status == "running" { Some(now) } else { None };

        let completed_ts = if status == "completed" {
            Some(now)
        } else {
            None
        };

        let row = sqlx::query_as::<_, BackgroundUpdate>(
            r#"
            UPDATE background_updates SET
                status = $2,
                started_ts = COALESCE($3, started_ts),
                completed_ts = COALESCE($4, completed_ts),
                last_updated_ts = $5
            WHERE job_name = $1
            RETURNING *
            "#,
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

        let row = sqlx::query_as::<_, BackgroundUpdate>(
            r#"
            UPDATE background_updates SET
                processed_items = processed_items + $2,
                total_items = COALESCE($3, total_items),
                last_updated_ts = $4,
                progress = CASE 
                    WHEN COALESCE($3, total_items) > 0 
                    THEN ROUND((processed_items + $2)::FLOAT / COALESCE($3, total_items) * 100)::INTEGER
                    ELSE progress 
                END
            WHERE job_name = $1
            RETURNING *
            "#,
        )
        .bind(job_name)
        .bind(items_processed)
        .bind(total_items)
        .bind(now)
        .fetch_one(&*self.pool)
        .await?;

        Ok(row)
    }

    pub async fn set_error(
        &self,
        job_name: &str,
        error_message: &str,
    ) -> Result<BackgroundUpdate, sqlx::Error> {
        let now = Utc::now().timestamp_millis();

        let row = sqlx::query_as::<_, BackgroundUpdate>(
            r#"
            UPDATE background_updates SET
                status = 'failed',
                error_message = $2,
                last_updated_ts = $3,
                retry_count = retry_count + 1
            WHERE job_name = $1
            RETURNING *
            "#,
        )
        .bind(job_name)
        .bind(error_message)
        .bind(now)
        .fetch_one(&*self.pool)
        .await?;

        Ok(row)
    }

    pub async fn delete_update(&self, job_name: &str) -> Result<(), sqlx::Error> {
        sqlx::query("DELETE FROM background_updates WHERE job_name = $1")
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
            r#"
            INSERT INTO background_update_locks (job_name, locked_by, locked_ts, expires_ts)
            VALUES ($1, $2, $3, $4)
            ON CONFLICT (job_name) DO UPDATE SET
                locked_by = $2,
                locked_ts = $3,
                expires_ts = $4
            WHERE background_update_locks.expires_ts < $3
            "#,
        )
        .bind(job_name)
        .bind(locked_by)
        .bind(now)
        .bind(expires)
        .execute(&*self.pool)
        .await?;

        Ok(result.rows_affected() > 0)
    }

    pub async fn release_lock(&self, job_name: &str) -> Result<(), sqlx::Error> {
        sqlx::query("DELETE FROM background_update_locks WHERE job_name = $1")
            .bind(job_name)
            .execute(&*self.pool)
            .await?;

        Ok(())
    }

    pub async fn is_locked(&self, job_name: &str) -> Result<bool, sqlx::Error> {
        let now = Utc::now().timestamp_millis();

        let count: i64 = sqlx::query_scalar(
            "SELECT COUNT(*) FROM background_update_locks WHERE job_name = $1 AND expires_ts > $2",
        )
        .bind(job_name)
        .bind(now)
        .fetch_one(&*self.pool)
        .await?;

        Ok(count > 0)
    }

    pub async fn cleanup_expired_locks(&self) -> Result<i64, sqlx::Error> {
        let now = Utc::now().timestamp_millis();

        let result = sqlx::query("DELETE FROM background_update_locks WHERE expires_ts < $1")
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
            r#"
            INSERT INTO background_update_history (
                job_name, execution_start_ts, execution_end_ts, status, items_processed, error_message, metadata
            )
            VALUES ($1, $2, $3, $4, $5, $6, $7)
            RETURNING *
            "#,
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

    pub async fn get_history(
        &self,
        job_name: &str,
        limit: i64,
    ) -> Result<Vec<BackgroundUpdateHistory>, sqlx::Error> {
        let rows = sqlx::query_as::<_, BackgroundUpdateHistory>(
            "SELECT * FROM background_update_history WHERE job_name = $1 ORDER BY execution_start_ts DESC LIMIT $2",
        )
        .bind(job_name)
        .bind(limit)
        .fetch_all(&*self.pool)
        .await?;

        Ok(rows)
    }

    pub async fn retry_failed(&self) -> Result<i64, sqlx::Error> {
        let result = sqlx::query(
            r#"
            UPDATE background_updates SET
                status = 'pending',
                error_message = NULL,
                retry_count = retry_count + 1
            WHERE status = 'failed' AND retry_count < max_retries
            "#,
        )
        .execute(&*self.pool)
        .await?;

        Ok(result.rows_affected() as i64)
    }

    pub async fn count_by_status(&self, status: &str) -> Result<i64, sqlx::Error> {
        let count: i64 =
            sqlx::query_scalar("SELECT COUNT(*) FROM background_updates WHERE status = $1")
                .bind(status)
                .fetch_one(&*self.pool)
                .await?;

        Ok(count)
    }

    pub async fn count_all(&self) -> Result<i64, sqlx::Error> {
        let count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM background_updates")
            .fetch_one(&*self.pool)
            .await?;

        Ok(count)
    }

    pub async fn get_stats(&self, days: i32) -> Result<Vec<BackgroundUpdateStats>, sqlx::Error> {
        let rows = sqlx::query_as::<_, BackgroundUpdateStats>(
            "SELECT * FROM background_update_stats WHERE stat_date >= CURRENT_DATE - $1 ORDER BY stat_date DESC",
        )
        .bind(days)
        .fetch_all(&*self.pool)
        .await?;

        Ok(rows)
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
            progress: 50,
            total_items: 100,
            processed_items: 50,
            created_ts: 1234567800,
            started_ts: Some(1234567800),
            completed_ts: None,
            last_updated_ts: Some(1234567890),
            error_message: None,
            retry_count: 0,
            max_retries: 3,
            batch_size: 1000,
            sleep_ms: 100,
            depends_on: None,
            metadata: None,
        };
        assert_eq!(update.job_name, "update_user_indices");
        assert_eq!(update.progress, 50);
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
            progress: 100,
            total_items: 100,
            processed_items: 100,
            created_ts: 1234567800,
            started_ts: Some(1234567800),
            completed_ts: Some(1234567890),
            last_updated_ts: Some(1234567890),
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
            job_name: "update_lock".to_string(),
            locked_by: Some("@admin:example.com".to_string()),
            locked_ts: 1234567890,
            expires_ts: 1234568190,
        };
        assert_eq!(lock.job_name, "update_lock");
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
            progress: Some(75),
            status: Some("running".to_string()),
            total_items: Some(100),
            processed_items: Some(75),
            error_message: None,
        };
        assert!(request.progress.is_some());
    }
}
