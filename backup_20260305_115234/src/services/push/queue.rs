use crate::error::ApiError;
use serde::{Deserialize, Serialize};
use std::collections::VecDeque;
use std::sync::Arc;
use tokio::sync::{Mutex, RwLock};
use tracing::{debug, info, warn};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct QueuedNotification {
    pub id: String,
    pub user_id: String,
    pub device_id: String,
    pub push_type: String,
    pub token: String,
    pub payload: serde_json::Value,
    pub priority: i32,
    pub attempts: u32,
    pub max_attempts: u32,
    pub created_ts: i64,
    pub next_attempt_ts: Option<i64>,
}

impl QueuedNotification {
    pub fn new(
        user_id: &str,
        device_id: &str,
        push_type: &str,
        token: &str,
        payload: serde_json::Value,
        priority: i32,
    ) -> Self {
        Self {
            id: uuid::Uuid::new_v4().to_string(),
            user_id: user_id.to_string(),
            device_id: device_id.to_string(),
            push_type: push_type.to_string(),
            token: token.to_string(),
            payload,
            priority,
            attempts: 0,
            max_attempts: 3,
            created_ts: chrono::Utc::now().timestamp_millis(),
            next_attempt_ts: None,
        }
    }

    pub fn can_retry(&self) -> bool {
        self.attempts < self.max_attempts
    }

    pub fn increment_attempt(&mut self) {
        self.attempts += 1;
        let delay_ms = 2u64.pow(self.attempts) * 1000;
        self.next_attempt_ts = Some(chrono::Utc::now().timestamp_millis() + delay_ms as i64);
    }
}

#[derive(Debug, Clone)]
pub struct QueueConfig {
    pub max_size: usize,
    pub batch_size: usize,
    pub max_attempts: u32,
    pub retry_delay_ms: u64,
}

impl Default for QueueConfig {
    fn default() -> Self {
        Self {
            max_size: 10000,
            batch_size: 100,
            max_attempts: 3,
            retry_delay_ms: 1000,
        }
    }
}

#[derive(Debug)]
pub struct PushQueue {
    config: QueueConfig,
    queue: Arc<Mutex<VecDeque<QueuedNotification>>>,
    pending: Arc<RwLock<std::collections::HashMap<String, QueuedNotification>>>,
    stats: Arc<RwLock<QueueStats>>,
}

#[derive(Debug, Clone, Default)]
pub struct QueueStats {
    pub total_queued: u64,
    pub total_sent: u64,
    pub total_failed: u64,
    pub current_size: usize,
}

impl PushQueue {
    pub fn new(config: QueueConfig) -> Self {
        Self {
            config,
            queue: Arc::new(Mutex::new(VecDeque::new())),
            pending: Arc::new(RwLock::new(std::collections::HashMap::new())),
            stats: Arc::new(RwLock::new(QueueStats::default())),
        }
    }

    pub async fn enqueue(&self, notification: QueuedNotification) -> Result<(), ApiError> {
        let mut queue = self.queue.lock().await;

        if queue.len() >= self.config.max_size {
            warn!("Push queue is full, dropping oldest notification");
            queue.pop_front();
        }

        queue.push_back(notification);

        let mut stats = self.stats.write().await;
        stats.total_queued += 1;
        stats.current_size = queue.len();

        debug!("Notification queued, current size: {}", queue.len());
        Ok(())
    }

    pub async fn dequeue_batch(&self) -> Vec<QueuedNotification> {
        let mut queue = self.queue.lock().await;
        let batch_size = self.config.batch_size.min(queue.len());

        let batch: Vec<QueuedNotification> = queue.drain(..batch_size).collect();

        if !batch.is_empty() {
            let mut pending = self.pending.write().await;
            for notification in &batch {
                pending.insert(notification.id.clone(), notification.clone());
            }
        }

        debug!("Dequeued {} notifications", batch.len());
        batch
    }

    pub async fn mark_sent(&self, id: &str) {
        let mut pending = self.pending.write().await;
        pending.remove(id);

        let mut stats = self.stats.write().await;
        stats.total_sent += 1;

        debug!("Notification {} marked as sent", id);
    }

    pub async fn mark_failed(&self, id: &str, retry: bool) {
        let mut pending = self.pending.write().await;

        if let Some(mut notification) = pending.remove(id) {
            if retry && notification.can_retry() {
                notification.increment_attempt();
                let mut queue = self.queue.lock().await;
                queue.push_back(notification);
            } else {
                let mut stats = self.stats.write().await;
                stats.total_failed += 1;
            }
        }

        debug!("Notification {} marked as failed (retry: {})", id, retry);
    }

    pub async fn get_size(&self) -> usize {
        let queue = self.queue.lock().await;
        queue.len()
    }

    pub async fn get_pending_count(&self) -> usize {
        let pending = self.pending.read().await;
        pending.len()
    }

    pub async fn get_stats(&self) -> QueueStats {
        let stats = self.stats.read().await;
        stats.clone()
    }

    pub async fn clear(&self) {
        let mut queue = self.queue.lock().await;
        queue.clear();

        let mut pending = self.pending.write().await;
        pending.clear();

        let mut stats = self.stats.write().await;
        stats.current_size = 0;

        info!("Push queue cleared");
    }

    pub async fn remove_for_device(&self, user_id: &str, device_id: &str) -> usize {
        let mut queue = self.queue.lock().await;
        let original_len = queue.len();

        queue.retain(|n| n.user_id != user_id || n.device_id != device_id);

        let removed = original_len - queue.len();
        if removed > 0 {
            debug!(
                "Removed {} notifications for device {}:{}",
                removed, user_id, device_id
            );
        }

        removed
    }

    pub async fn remove_for_user(&self, user_id: &str) -> usize {
        let mut queue = self.queue.lock().await;
        let original_len = queue.len();

        queue.retain(|n| n.user_id != user_id);

        let removed = original_len - queue.len();
        if removed > 0 {
            debug!("Removed {} notifications for user {}", removed, user_id);
        }

        removed
    }

    pub async fn prioritize(&self, id: &str) -> bool {
        let mut queue = self.queue.lock().await;

        if let Some(pos) = queue.iter().position(|n| n.id == id) {
            if let Some(notification) = queue.remove(pos) {
                queue.push_front(notification);
                debug!("Notification {} prioritized", id);
                return true;
            }
        }

        false
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_queued_notification_creation() {
        let notification = QueuedNotification::new(
            "@user:example.com",
            "DEVICE123",
            "fcm",
            "token123",
            serde_json::json!({"title": "Test"}),
            5,
        );

        assert_eq!(notification.user_id, "@user:example.com");
        assert_eq!(notification.device_id, "DEVICE123");
        assert_eq!(notification.attempts, 0);
        assert!(notification.can_retry());
    }

    #[test]
    fn test_queued_notification_retry() {
        let mut notification = QueuedNotification::new(
            "@user:example.com",
            "DEVICE123",
            "fcm",
            "token123",
            serde_json::json!({}),
            5,
        );

        assert!(notification.can_retry());
        notification.increment_attempt();
        assert_eq!(notification.attempts, 1);
        assert!(notification.next_attempt_ts.is_some());
    }

    #[test]
    fn test_queue_config_default() {
        let config = QueueConfig::default();
        assert_eq!(config.max_size, 10000);
        assert_eq!(config.batch_size, 100);
        assert_eq!(config.max_attempts, 3);
    }

    #[tokio::test]
    async fn test_push_queue_enqueue() {
        let queue = PushQueue::new(QueueConfig::default());

        let notification = QueuedNotification::new(
            "@user:example.com",
            "DEVICE123",
            "fcm",
            "token123",
            serde_json::json!({}),
            5,
        );

        queue.enqueue(notification).await.unwrap();
        assert_eq!(queue.get_size().await, 1);
    }

    #[tokio::test]
    async fn test_push_queue_dequeue_batch() {
        let queue = PushQueue::new(QueueConfig::default());

        for i in 0..5 {
            let notification = QueuedNotification::new(
                "@user:example.com",
                &format!("DEVICE{}", i),
                "fcm",
                "token",
                serde_json::json!({}),
                5,
            );
            queue.enqueue(notification).await.unwrap();
        }

        let batch = queue.dequeue_batch().await;
        assert_eq!(batch.len(), 5);
        assert_eq!(queue.get_size().await, 0);
    }

    #[tokio::test]
    async fn test_push_queue_mark_sent() {
        let queue = PushQueue::new(QueueConfig::default());

        let notification = QueuedNotification::new(
            "@user:example.com",
            "DEVICE123",
            "fcm",
            "token",
            serde_json::json!({}),
            5,
        );
        let id = notification.id.clone();

        queue.enqueue(notification).await.unwrap();
        queue.dequeue_batch().await;
        queue.mark_sent(&id).await;

        let stats = queue.get_stats().await;
        assert_eq!(stats.total_sent, 1);
    }

    #[tokio::test]
    async fn test_push_queue_remove_for_device() {
        let queue = PushQueue::new(QueueConfig::default());

        for i in 0..3 {
            let notification = QueuedNotification::new(
                "@user:example.com",
                &format!("DEVICE{}", i),
                "fcm",
                "token",
                serde_json::json!({}),
                5,
            );
            queue.enqueue(notification).await.unwrap();
        }

        let removed = queue
            .remove_for_device("@user:example.com", "DEVICE1")
            .await;
        assert_eq!(removed, 1);
        assert_eq!(queue.get_size().await, 2);
    }

    #[tokio::test]
    async fn test_push_queue_clear() {
        let queue = PushQueue::new(QueueConfig::default());

        for i in 0..5 {
            let notification = QueuedNotification::new(
                "@user:example.com",
                &format!("DEVICE{}", i),
                "fcm",
                "token",
                serde_json::json!({}),
                5,
            );
            queue.enqueue(notification).await.unwrap();
        }

        queue.clear().await;
        assert_eq!(queue.get_size().await, 0);
    }
}
