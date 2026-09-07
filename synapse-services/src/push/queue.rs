use serde::{Deserialize, Serialize};
use std::collections::VecDeque;
use std::sync::Arc;
use synapse_common::current_timestamp_millis;
use synapse_common::error::ApiError;
use tokio::sync::{Mutex, RwLock};
use tracing::{debug, info, warn};

/// The `QueuedNotification` struct.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct QueuedNotification {
    /// The `id` field.
    pub id: String,
    /// The `user_id` field.
    pub user_id: String,
    /// The `device_id` field.
    pub device_id: String,
    /// The `push_type` field.
    pub push_type: String,
    /// The `token` field.
    pub token: String,
    /// The `payload` field.
    pub payload: serde_json::Value,
    /// The `priority` field.
    pub priority: i32,
    /// The `attempts` field.
    pub attempts: u32,
    /// The `max_attempts` field.
    pub max_attempts: u32,
    /// The `created_ts` field.
    pub created_ts: i64,
    /// The `next_attempt_ts` field.
    pub next_attempt_ts: Option<i64>,
}

impl QueuedNotification {
    /// See [`new`].
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
            created_ts: current_timestamp_millis(),
            next_attempt_ts: None,
        }
    }

    /// See [`can_retry`].
    pub fn can_retry(&self) -> bool {
        self.attempts < self.max_attempts
    }

    /// See [`increment_attempt`].
    pub fn increment_attempt(&mut self) {
        self.attempts += 1;
        let delay_ms = 2u64.pow(self.attempts) * 1000;
        self.next_attempt_ts = Some(current_timestamp_millis() + delay_ms as i64);
    }
}

/// The `QueueConfig` struct.
#[derive(Debug, Clone)]
pub struct QueueConfig {
    /// The `max_size` field.
    pub max_size: usize,
    /// The `batch_size` field.
    pub batch_size: usize,
    /// The `max_attempts` field.
    pub max_attempts: u32,
    /// The `retry_delay_ms` field.
    pub retry_delay_ms: u64,
}

impl Default for QueueConfig {
    fn default() -> Self {
        Self { max_size: 10000, batch_size: 100, max_attempts: 3, retry_delay_ms: 1000 }
    }
}

/// The `PushQueue` struct.
#[derive(Debug)]
pub struct PushQueue {
    config: QueueConfig,
    queue: Arc<Mutex<VecDeque<QueuedNotification>>>,
    pending: Arc<RwLock<std::collections::HashMap<String, QueuedNotification>>>,
    stats: Arc<RwLock<QueueStats>>,
}

/// The `QueueStats` struct.
#[derive(Debug, Clone, Default)]
pub struct QueueStats {
    /// The `total_queued` field.
    pub total_queued: u64,
    /// The `total_sent` field.
    pub total_sent: u64,
    /// The `total_failed` field.
    pub total_failed: u64,
    /// The `current_size` field.
    pub current_size: usize,
}

impl PushQueue {
    /// See [`new`].
    pub fn new(config: QueueConfig) -> Self {
        Self {
            config,
            queue: Arc::new(Mutex::new(VecDeque::new())),
            pending: Arc::new(RwLock::new(std::collections::HashMap::new())),
            stats: Arc::new(RwLock::new(QueueStats::default())),
        }
    }

    /// See [`enqueue`].
    pub async fn enqueue(&self, notification: QueuedNotification) -> Result<(), ApiError> {
        let mut queue = self.queue.lock().await;

        if queue.len() >= self.config.max_size {
            let dropped = queue.pop_front();
            warn!(
                max_size = self.config.max_size,
                current_size = queue.len() + 1,
                dropped_notification_id = dropped.as_ref().map(|n| n.id.as_str()),
                dropped_user_id = dropped.as_ref().map(|n| n.user_id.as_str()),
                dropped_device_id = dropped.as_ref().map(|n| n.device_id.as_str()),
                dropped_push_type = dropped.as_ref().map(|n| n.push_type.as_str()),
                "Push queue is full, dropping oldest notification"
            );
        }

        queue.push_back(notification);

        let mut stats = self.stats.write().await;
        stats.total_queued += 1;
        stats.current_size = queue.len();

        debug!(current_size = queue.len(), "Notification queued");
        Ok(())
    }

    /// See [`dequeue_batch`].
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

        debug!(batch_size = batch.len(), "Dequeued notifications");
        batch
    }

    /// See [`mark_sent`].
    pub async fn mark_sent(&self, id: &str) {
        let mut pending = self.pending.write().await;
        pending.remove(id);

        let mut stats = self.stats.write().await;
        stats.total_sent += 1;

        debug!(notification_id = %id, "Notification marked as sent");
    }

    /// See [`mark_failed`].
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

        debug!(notification_id = %id, retry, "Notification marked as failed");
    }

    /// See [`get_size`].
    pub async fn get_size(&self) -> usize {
        let queue = self.queue.lock().await;
        queue.len()
    }

    /// See [`get_pending_count`].
    pub async fn get_pending_count(&self) -> usize {
        let pending = self.pending.read().await;
        pending.len()
    }

    /// See [`get_stats`].
    pub async fn get_stats(&self) -> QueueStats {
        let stats = self.stats.read().await;
        stats.clone()
    }

    /// See [`clear`].
    pub async fn clear(&self) {
        let mut queue = self.queue.lock().await;
        queue.clear();

        let mut pending = self.pending.write().await;
        pending.clear();

        let mut stats = self.stats.write().await;
        stats.current_size = 0;

        info!(cleared_queue = true, current_size = stats.current_size, "Push queue cleared");
    }

    /// See [`remove_for_device`].
    pub async fn remove_for_device(&self, user_id: &str, device_id: &str) -> usize {
        let mut queue = self.queue.lock().await;
        let original_len = queue.len();

        queue.retain(|n| n.user_id != user_id || n.device_id != device_id);

        let removed = original_len - queue.len();
        if removed > 0 {
            debug!(removed, user_id = %user_id, device_id = %device_id, "Removed notifications for device");
        }

        removed
    }

    /// See [`remove_for_user`].
    pub async fn remove_for_user(&self, user_id: &str) -> usize {
        let mut queue = self.queue.lock().await;
        let original_len = queue.len();

        queue.retain(|n| n.user_id != user_id);

        let removed = original_len - queue.len();
        if removed > 0 {
            debug!(removed, user_id = %user_id, "Removed notifications for user");
        }

        removed
    }

    /// See [`prioritize`].
    pub async fn prioritize(&self, id: &str) -> bool {
        let mut queue = self.queue.lock().await;

        if let Some(pos) = queue.iter().position(|n| n.id == id) {
            if let Some(notification) = queue.remove(pos) {
                queue.push_front(notification);
                debug!(notification_id = %id, "Notification prioritized");
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
        let mut notification =
            QueuedNotification::new("@user:example.com", "DEVICE123", "fcm", "token123", serde_json::json!({}), 5);

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

        let notification =
            QueuedNotification::new("@user:example.com", "DEVICE123", "fcm", "token123", serde_json::json!({}), 5);

        queue.enqueue(notification).await.unwrap();
        assert_eq!(queue.get_size().await, 1);
    }

    #[tokio::test]
    async fn test_push_queue_dequeue_batch() {
        let queue = PushQueue::new(QueueConfig::default());

        for i in 0..5 {
            let notification = QueuedNotification::new(
                "@user:example.com",
                &format!("DEVICE{i}"),
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

        let notification =
            QueuedNotification::new("@user:example.com", "DEVICE123", "fcm", "token", serde_json::json!({}), 5);
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
                &format!("DEVICE{i}"),
                "fcm",
                "token",
                serde_json::json!({}),
                5,
            );
            queue.enqueue(notification).await.unwrap();
        }

        let removed = queue.remove_for_device("@user:example.com", "DEVICE1").await;
        assert_eq!(removed, 1);
        assert_eq!(queue.get_size().await, 2);
    }

    #[tokio::test]
    async fn test_push_queue_clear() {
        let queue = PushQueue::new(QueueConfig::default());

        for i in 0..5 {
            let notification = QueuedNotification::new(
                "@user:example.com",
                &format!("DEVICE{i}"),
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

    #[tokio::test]
    async fn test_push_queue_mark_failed() {
        let queue = PushQueue::new(QueueConfig::default());

        let notification =
            QueuedNotification::new("@user:example.com", "DEVICE123", "fcm", "token", serde_json::json!({}), 5);
        let id = notification.id.clone();

        queue.enqueue(notification).await.unwrap();
        queue.dequeue_batch().await;
        queue.mark_failed(&id, false).await;

        let stats = queue.get_stats().await;
        assert_eq!(stats.total_failed, 1);
    }

    #[tokio::test]
    async fn test_push_queue_remove_for_user() {
        let queue = PushQueue::new(QueueConfig::default());

        for i in 0..3 {
            let notification = QueuedNotification::new(
                "@user:example.com",
                &format!("DEVICE{i}"),
                "fcm",
                "token",
                serde_json::json!({}),
                5,
            );
            queue.enqueue(notification).await.unwrap();
        }

        let removed = queue.remove_for_user("@user:example.com").await;
        assert_eq!(removed, 3);
        assert_eq!(queue.get_size().await, 0);
    }

    #[tokio::test]
    async fn test_push_queue_prioritize() {
        let queue = PushQueue::new(QueueConfig::default());

        let notification1 =
            QueuedNotification::new("@user:example.com", "DEVICE1", "fcm", "token", serde_json::json!({}), 5);
        let id = notification1.id.clone();

        queue.enqueue(notification1).await.unwrap();
        let prioritized = queue.prioritize(&id).await;
        assert!(prioritized);
    }

    #[tokio::test]
    async fn test_push_queue_get_pending_count() {
        let queue = PushQueue::new(QueueConfig::default());

        let notification =
            QueuedNotification::new("@user:example.com", "DEVICE123", "fcm", "token", serde_json::json!({}), 5);

        queue.enqueue(notification).await.unwrap();
        queue.dequeue_batch().await;

        let pending_count = queue.get_pending_count().await;
        assert_eq!(pending_count, 1);
    }

    #[test]
    fn test_queued_notification_max_attempts_reached() {
        let mut notification =
            QueuedNotification::new("@user:example.com", "DEVICE123", "fcm", "token123", serde_json::json!({}), 5);

        notification.max_attempts = 2;
        notification.increment_attempt();
        notification.increment_attempt();
        assert!(!notification.can_retry());
    }
}
