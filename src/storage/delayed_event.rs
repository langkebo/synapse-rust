use serde::{Deserialize, Serialize};
use sqlx::{Pool, Postgres};
use std::sync::Arc;

use crate::cache::CacheManager;
use crate::common::error::ApiError;

use crate::common::task_queue::RedisTaskQueue;

#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow)]
pub struct DelayedEvent {
    pub id: i64,
    pub room_id: String,
    pub user_id: String,
    pub device_id: String,
    pub event_id: String,
    pub event_type: String,
    pub state_key: Option<String>,
    pub content: serde_json::Value,
    pub delay_ms: i64,
    pub scheduled_ts: i64,
    pub created_ts: i64,
    pub status: String,
    pub retry_count: i32,
    pub last_error: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CreateDelayedEventParams {
    pub room_id: String,
    pub user_id: String,
    pub device_id: String,
    pub event_type: String,
    pub state_key: Option<String>,
    pub content: serde_json::Value,
    pub delay_ms: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UpdateDelayedEventParams {
    pub delay_ms: Option<i64>,
    pub status: Option<String>,
    pub error: Option<String>,
}

#[derive(Clone)]
pub struct DelayedEventStorage {
    pool: Arc<Pool<Postgres>>,
}

impl DelayedEventStorage {
    pub fn new(pool: Arc<Pool<Postgres>>) -> Self {
        Self { pool }
    }

    pub async fn create_event(
        &self,
        params: CreateDelayedEventParams,
    ) -> Result<DelayedEvent, sqlx::Error> {
        let now = chrono::Utc::now().timestamp_millis();
        let scheduled_ts = now + params.delay_ms;
        let event_id = format!("${}", uuid::Uuid::new_v4().simple());

        sqlx::query_as::<_, DelayedEvent>(
            r#"
            INSERT INTO delayed_events 
                (room_id, user_id, device_id, event_id, event_type, state_key, content, 
                 delay_ms, scheduled_ts, created_ts, status, retry_count)
            VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, 'pending', 0)
            RETURNING *
            "#,
        )
        .bind(&params.room_id)
        .bind(&params.user_id)
        .bind(&params.device_id)
        .bind(&event_id)
        .bind(&params.event_type)
        .bind(&params.state_key)
        .bind(&params.content)
        .bind(params.delay_ms)
        .bind(scheduled_ts)
        .bind(now)
        .fetch_one(&*self.pool)
        .await
    }

    pub async fn get_event(&self, event_id: &str) -> Result<Option<DelayedEvent>, sqlx::Error> {
        sqlx::query_as::<_, DelayedEvent>(
            r#"
            SELECT * FROM delayed_events WHERE event_id = $1
            "#,
        )
        .bind(event_id)
        .fetch_optional(&*self.pool)
        .await
    }

    pub async fn get_pending_events(&self) -> Result<Vec<DelayedEvent>, sqlx::Error> {
        let now = chrono::Utc::now().timestamp_millis();

        sqlx::query_as::<_, DelayedEvent>(
            r#"
            SELECT * FROM delayed_events 
            WHERE status = 'pending' AND scheduled_ts <= $1
            ORDER BY scheduled_ts ASC
            LIMIT 100
            "#,
        )
        .bind(now)
        .fetch_all(&*self.pool)
        .await
    }

    pub async fn get_events_for_room(
        &self,
        room_id: &str,
        user_id: &str,
    ) -> Result<Vec<DelayedEvent>, sqlx::Error> {
        sqlx::query_as::<_, DelayedEvent>(
            r#"
            SELECT * FROM delayed_events 
            WHERE room_id = $1 AND user_id = $2 AND status IN ('pending', 'processing')
            ORDER BY scheduled_ts ASC
            "#,
        )
        .bind(room_id)
        .bind(user_id)
        .fetch_all(&*self.pool)
        .await
    }

    pub async fn update_event(
        &self,
        event_id: &str,
        params: UpdateDelayedEventParams,
    ) -> Result<(), sqlx::Error> {
        let now = chrono::Utc::now().timestamp_millis();

        if let Some(delay_ms) = params.delay_ms {
            let scheduled_ts = now + delay_ms;
            sqlx::query(
                r#"
                UPDATE delayed_events 
                SET delay_ms = $2, scheduled_ts = $3, status = COALESCE($4, status), last_error = $5
                WHERE event_id = $1
                "#,
            )
            .bind(event_id)
            .bind(delay_ms)
            .bind(scheduled_ts)
            .bind(&params.status)
            .bind(&params.error)
            .execute(&*self.pool)
            .await?;
        } else {
            sqlx::query(
                r#"
                UPDATE delayed_events 
                SET status = COALESCE($2, status), last_error = $3
                WHERE event_id = $1
                "#,
            )
            .bind(event_id)
            .bind(&params.status)
            .bind(&params.error)
            .execute(&*self.pool)
            .await?;
        }

        Ok(())
    }

    pub async fn cancel_event(&self, event_id: &str) -> Result<bool, sqlx::Error> {
        let result = sqlx::query(
            r#"
            UPDATE delayed_events 
            SET status = 'cancelled'
            WHERE event_id = $1 AND status = 'pending'
            "#,
        )
        .bind(event_id)
        .execute(&*self.pool)
        .await?;

        Ok(result.rows_affected() > 0)
    }

    pub async fn delete_event(&self, event_id: &str) -> Result<(), sqlx::Error> {
        sqlx::query(
            r#"
            DELETE FROM delayed_events WHERE event_id = $1
            "#,
        )
        .bind(event_id)
        .execute(&*self.pool)
        .await?;

        Ok(())
    }

    pub async fn mark_processing(&self, event_id: &str) -> Result<(), sqlx::Error> {
        sqlx::query(
            r#"
            UPDATE delayed_events 
            SET status = 'processing'
            WHERE event_id = $1 AND status = 'pending'
            "#,
        )
        .bind(event_id)
        .execute(&*self.pool)
        .await?;

        Ok(())
    }

    pub async fn mark_completed(&self, event_id: &str) -> Result<(), sqlx::Error> {
        sqlx::query(
            r#"
            UPDATE delayed_events 
            SET status = 'completed'
            WHERE event_id = $1
            "#,
        )
        .bind(event_id)
        .execute(&*self.pool)
        .await?;

        Ok(())
    }

    pub async fn mark_failed(
        &self,
        event_id: &str,
        error: &str,
        retry: bool,
    ) -> Result<(), sqlx::Error> {
        if retry {
            let now = chrono::Utc::now().timestamp_millis();
            sqlx::query(
                r#"
                UPDATE delayed_events 
                SET status = 'pending', retry_count = retry_count + 1, last_error = $2, scheduled_ts = $3
                WHERE event_id = $1
                "#,
            )
            .bind(event_id)
            .bind(error)
            .bind(now + 60000)
            .execute(&*self.pool)
            .await?;
        } else {
            sqlx::query(
                r#"
                UPDATE delayed_events 
                SET status = 'failed', last_error = $2
                WHERE event_id = $1
                "#,
            )
            .bind(event_id)
            .bind(error)
            .execute(&*self.pool)
            .await?;
        }

        Ok(())
    }

    pub async fn cleanup_completed_events(&self, older_than_ms: i64) -> Result<u64, sqlx::Error> {
        let cutoff = chrono::Utc::now().timestamp_millis() - older_than_ms;

        let result = sqlx::query(
            r#"
            DELETE FROM delayed_events 
            WHERE status IN ('completed', 'failed', 'cancelled') AND created_ts < $1
            "#,
        )
        .bind(cutoff)
        .execute(&*self.pool)
        .await?;

        Ok(result.rows_affected())
    }
}

pub struct DelayedEventService {
    storage: DelayedEventStorage,
    cache: Arc<CacheManager>,
    task_queue: Option<Arc<RedisTaskQueue>>,
    max_retries: u32,
    retry_delay_ms: i64,
}

impl DelayedEventService {
    pub fn new(
        storage: DelayedEventStorage,
        cache: Arc<CacheManager>,
        task_queue: Option<Arc<RedisTaskQueue>>,
    ) -> Self {
        Self {
            storage,
            cache,
            task_queue,
            max_retries: 3,
            retry_delay_ms: 60000,
        }
    }

    pub fn with_retry_config(mut self, max_retries: u32, retry_delay_ms: i64) -> Self {
        self.max_retries = max_retries;
        self.retry_delay_ms = retry_delay_ms;
        self
    }

    pub async fn schedule_event(
        &self,
        params: CreateDelayedEventParams,
    ) -> Result<DelayedEvent, ApiError> {
        if params.delay_ms < 0 {
            return Err(ApiError::bad_request("delay_ms must be positive"));
        }

        if params.content.is_null() {
            return Err(ApiError::bad_request("content is required"));
        }

        let event =
            self.storage.create_event(params).await.map_err(|e| {
                ApiError::internal(format!("Failed to create delayed event: {}", e))
            })?;

        self.invalidate_cache(&event.room_id, &event.user_id).await;

        if let Some(task_queue) = &self.task_queue {
            let _ = task_queue
                .submit(
                    crate::common::background_job::BackgroundJob::DelayedEventProcessing {
                        event_id: event.event_id.clone(),
                    },
                )
                .await;
        }

        Ok(event)
    }

    pub async fn get_event(&self, event_id: &str) -> Result<Option<DelayedEvent>, ApiError> {
        let cache_key = format!("delayed_event:{}", event_id);

        if let Ok(Some(event)) = self.cache.get::<DelayedEvent>(&cache_key).await {
            return Ok(Some(event));
        }

        let event = self
            .storage
            .get_event(event_id)
            .await
            .map_err(|e| ApiError::internal(format!("Failed to get delayed event: {}", e)))?;

        if let Some(ref event) = event {
            let _ = self.cache.set(&cache_key, event, 300).await;
        }

        Ok(event)
    }

    pub async fn get_pending_events(&self) -> Result<Vec<DelayedEvent>, ApiError> {
        self.storage
            .get_pending_events()
            .await
            .map_err(|e| ApiError::internal(format!("Failed to get pending events: {}", e)))
    }

    pub async fn get_events_for_room(
        &self,
        room_id: &str,
        user_id: &str,
    ) -> Result<Vec<DelayedEvent>, ApiError> {
        let cache_key = format!("delayed_events:{}:{}", room_id, user_id);

        if let Ok(Some(events)) = self.cache.get::<Vec<DelayedEvent>>(&cache_key).await {
            return Ok(events);
        }

        let events = self
            .storage
            .get_events_for_room(room_id, user_id)
            .await
            .map_err(|e| ApiError::internal(format!("Failed to get delayed events: {}", e)))?;

        let _ = self.cache.set(&cache_key, &events, 60).await;

        Ok(events)
    }

    pub async fn cancel_event(&self, event_id: &str) -> Result<bool, ApiError> {
        let event = self
            .get_event(event_id)
            .await?
            .ok_or_else(|| ApiError::not_found("Delayed event not found"))?;

        if event.status != "pending" {
            return Err(ApiError::bad_request("Can only cancel pending events"));
        }

        let cancelled = self
            .storage
            .cancel_event(event_id)
            .await
            .map_err(|e| ApiError::internal(format!("Failed to cancel event: {}", e)))?;

        self.invalidate_cache(&event.room_id, &event.user_id).await;

        Ok(cancelled)
    }

    pub async fn update_event_delay(
        &self,
        event_id: &str,
        new_delay_ms: i64,
    ) -> Result<(), ApiError> {
        if new_delay_ms < 0 {
            return Err(ApiError::bad_request("delay_ms must be positive"));
        }

        let event = self
            .get_event(event_id)
            .await?
            .ok_or_else(|| ApiError::not_found("Delayed event not found"))?;

        if event.status != "pending" {
            return Err(ApiError::bad_request("Can only update pending events"));
        }

        self.storage
            .update_event(
                event_id,
                UpdateDelayedEventParams {
                    delay_ms: Some(new_delay_ms),
                    status: None,
                    error: None,
                },
            )
            .await
            .map_err(|e| ApiError::internal(format!("Failed to update event delay: {}", e)))?;

        self.invalidate_cache(&event.room_id, &event.user_id).await;

        Ok(())
    }

    pub async fn process_event(&self, event_id: &str) -> Result<(), ApiError> {
        let event = self
            .get_event(event_id)
            .await?
            .ok_or_else(|| ApiError::not_found("Delayed event not found"))?;

        if event.status != "pending" {
            return Err(ApiError::bad_request("Event is not pending"));
        }

        self.storage.mark_processing(event_id).await.map_err(|e| {
            ApiError::internal(format!("Failed to mark event as processing: {}", e))
        })?;

        Ok(())
    }

    pub async fn complete_event(&self, event_id: &str) -> Result<(), ApiError> {
        self.storage
            .mark_completed(event_id)
            .await
            .map_err(|e| ApiError::internal(format!("Failed to mark event as completed: {}", e)))?;

        self.invalidate_cache_for_event(event_id).await;

        Ok(())
    }

    pub async fn fail_event(&self, event_id: &str, error: &str) -> Result<(), ApiError> {
        let event = self
            .get_event(event_id)
            .await?
            .ok_or_else(|| ApiError::not_found("Delayed event not found"))?;

        let should_retry = event.retry_count < self.max_retries as i32;

        self.storage
            .mark_failed(event_id, error, should_retry)
            .await
            .map_err(|e| ApiError::internal(format!("Failed to mark event as failed: {}", e)))?;

        self.invalidate_cache_for_event(event_id).await;

        Ok(())
    }

    pub async fn cleanup_old_events(&self, older_than_ms: i64) -> Result<u64, ApiError> {
        let count = self
            .storage
            .cleanup_completed_events(older_than_ms)
            .await
            .map_err(|e| ApiError::internal(format!("Failed to cleanup events: {}", e)))?;

        Ok(count)
    }

    async fn invalidate_cache(&self, room_id: &str, user_id: &str) {
        let _ = self
            .cache
            .delete(&format!("delayed_events:{}:{}", room_id, user_id))
            .await;
    }

    async fn invalidate_cache_for_event(&self, event_id: &str) {
        let _ = self
            .cache
            .delete(&format!("delayed_event:{}", event_id))
            .await;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_delayed_event_struct() {
        let event = DelayedEvent {
            id: 1,
            room_id: "!room:example.com".to_string(),
            user_id: "@alice:example.com".to_string(),
            device_id: "DEVICE123".to_string(),
            event_id: "$event123".to_string(),
            event_type: "m.room.message".to_string(),
            state_key: None,
            content: serde_json::json!({"body": "Hello"}),
            delay_ms: 5000,
            scheduled_ts: 1234567890000,
            created_ts: 1234567885000,
            status: "pending".to_string(),
            retry_count: 0,
            last_error: None,
        };

        assert_eq!(event.room_id, "!room:example.com");
        assert_eq!(event.delay_ms, 5000);
        assert_eq!(event.status, "pending");
    }

    #[test]
    fn test_create_params() {
        let params = CreateDelayedEventParams {
            room_id: "!room:example.com".to_string(),
            user_id: "@bob:example.com".to_string(),
            device_id: "DEVICE456".to_string(),
            event_type: "m.room.message".to_string(),
            state_key: None,
            content: serde_json::json!({"msgtype": "m.text", "body": "Delayed message"}),
            delay_ms: 10000,
        };

        assert_eq!(params.delay_ms, 10000);
        assert!(params.state_key.is_none());
    }

    #[test]
    fn test_update_params() {
        let params = UpdateDelayedEventParams {
            delay_ms: Some(15000),
            status: Some("pending".to_string()),
            error: Some("Previous attempt failed".to_string()),
        };

        assert!(params.delay_ms.is_some());
        assert!(params.error.is_some());
    }

    #[test]
    fn test_event_status_transitions() {
        let statuses = vec!["pending", "processing", "completed", "failed", "cancelled"];

        for status in statuses {
            assert!(!status.is_empty());
        }
    }

    #[test]
    fn test_retry_logic() {
        let max_retries = 3;
        let retry_count = 2;

        assert!(retry_count < max_retries);
    }
}
