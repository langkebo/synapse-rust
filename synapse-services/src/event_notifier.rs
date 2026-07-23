use crate::event_broadcaster_trait::{BroadcastError, EventBroadcaster};
use dashmap::DashMap;
use deadpool_redis::Pool;
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use tokio::sync::Notify;
use tracing::{debug, warn};

const EVENT_NOTIFY_CHANNEL: &str = "synapse:events:notify";

/// Message payload for the [`EventNotifier`] broadcast channel.
///
/// Carries the notification kind (room or user), the target key, and the
/// originating instance identifier (used to avoid echo on cross-instance
/// Redis fan-out).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EventNotifyMessage {
    pub kind: EventNotifyKind,
    pub key: String,
    pub sender_instance: String,
}

/// Whether a notification targets a room or a user.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
pub enum EventNotifyKind {
    Room,
    User,
}

/// Event notifier for instantly waking up waiting sync connections.
///
/// Uses `tokio::sync::Notify` per room/user key so that long-polling sync
/// requests can be woken immediately when new data is available, instead of
/// relying on periodic polling.
///
/// # Cross-instance fan-out
///
/// When Redis is configured (via [`EventNotifier::with_redis`]), notifications
/// are also published to a Redis Pub/Sub channel so that other server
/// instances in the same deployment can wake their local waiters.
///
/// # Relationship to other broadcasters
///
/// This is one of three event-distribution implementations that share the
/// [`EventBroadcaster`] trait. See [`event_broadcaster_trait`][crate::event_broadcaster_trait]
/// for the full comparison table and selection guide.
///
/// * **This type** → local sync wake-up (room / user `Notify` + Redis fan-out)
/// * [`federation::EventBroadcaster`][synapse_federation::event_broadcaster::EventBroadcaster] → federation outbound (PDU/EDU batching + retry)
/// * [`WorkerBus`][crate::worker::bus::WorkerBus] → inter-worker pub/sub (replication commands)
pub struct EventNotifier {
    room_notifiers: Arc<DashMap<String, Arc<Notify>>>,
    user_notifiers: Arc<DashMap<String, Arc<Notify>>>,
    redis_pool: Option<Pool>,
    redis_url: Option<String>,
    instance_id: String,
}

impl std::fmt::Debug for EventNotifier {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("EventNotifier")
            .field("has_redis", &self.redis_pool.is_some())
            .field("instance_id", &self.instance_id)
            .finish()
    }
}

impl EventNotifier {
    pub fn new() -> Self {
        Self {
            room_notifiers: Arc::new(DashMap::new()),
            user_notifiers: Arc::new(DashMap::new()),
            redis_pool: None,
            redis_url: None,
            instance_id: format!("instance-{}", uuid::Uuid::new_v4()),
        }
    }

    pub fn with_redis(mut self, pool: Pool, redis_url: String) -> Self {
        self.redis_pool = Some(pool);
        self.redis_url = Some(redis_url);
        self
    }

    pub fn with_instance_id(mut self, instance_id: String) -> Self {
        self.instance_id = instance_id;
        self
    }

    /// Wait until at least one of the given rooms receives a notification, or
    /// the timeout elapses.
    pub async fn wait_for_room(&self, room_ids: &[String], timeout: tokio::time::Duration) {
        if room_ids.is_empty() {
            tokio::time::sleep(timeout).await;
            return;
        }

        let notifiers: Vec<Arc<Notify>> =
            room_ids.iter().map(|room_id| self.get_or_create_room_notify(room_id)).collect();

        let futures: Vec<_> = notifiers.iter().map(|n| Box::pin(n.notified())).collect();

        tokio::select! {
            _ = futures::future::select_all(futures) => {}
            _ = tokio::time::sleep(timeout) => {}
        }
    }

    /// Wait until the given user receives a notification, or the timeout
    /// elapses.
    pub async fn wait_for_user(&self, user_id: &str, timeout: tokio::time::Duration) {
        let notify = self.get_or_create_user_notify(user_id);
        tokio::select! {
            _ = notify.notified() => {}
            _ = tokio::time::sleep(timeout) => {}
        }
    }

    /// Notify all connections waiting for events in the given room.
    pub fn notify_room(&self, room_id: &str) {
        if let Some(notify) = self.room_notifiers.get(room_id) {
            notify.notify_waiters();
        }

        self.publish_redis(EventNotifyKind::Room, room_id);
    }

    /// Notify all connections waiting for data for the given user (e.g.
    /// to-device messages).
    pub fn notify_user(&self, user_id: &str) {
        if let Some(notify) = self.user_notifiers.get(user_id) {
            notify.notify_waiters();
        }

        self.publish_redis(EventNotifyKind::User, user_id);
    }

    fn get_or_create_room_notify(&self, room_id: &str) -> Arc<Notify> {
        self.room_notifiers.entry(room_id.to_string()).or_insert_with(|| Arc::new(Notify::new())).value().clone()
    }

    fn get_or_create_user_notify(&self, user_id: &str) -> Arc<Notify> {
        self.user_notifiers.entry(user_id.to_string()).or_insert_with(|| Arc::new(Notify::new())).value().clone()
    }

    fn publish_redis(&self, kind: EventNotifyKind, key: &str) {
        let Some(pool) = &self.redis_pool else {
            return;
        };

        let msg = EventNotifyMessage { kind, key: key.to_string(), sender_instance: self.instance_id.clone() };

        let encoded = match serde_json::to_vec(&msg) {
            Ok(v) => v,
            Err(e) => {
                warn!(
                    error = %e,
                    kind = ?kind,
                    key = %key,
                    sender_instance = %self.instance_id,
                    "Failed to encode event notify message"
                );
                return;
            }
        };

        let pool = pool.clone();
        let channel = EVENT_NOTIFY_CHANNEL.to_string();
        tokio::spawn(async move {
            match pool.get().await {
                Ok(mut conn) => {
                    use redis::AsyncCommands;
                    let result: Result<(), redis::RedisError> = conn.publish(&channel, encoded).await;
                    if let Err(e) = result {
                        debug!(error = %e, channel = %channel, "Failed to publish event notification to Redis");
                    }
                }
                Err(e) => {
                    debug!(error = %e, channel = %channel, "Failed to get Redis connection for event notification");
                }
            }
        });
    }
}

impl EventBroadcaster for EventNotifier {
    type Message = EventNotifyMessage;

    async fn broadcast_publish(&self, message: Self::Message) -> Result<(), BroadcastError> {
        match message.kind {
            EventNotifyKind::Room => self.notify_room(&message.key),
            EventNotifyKind::User => self.notify_user(&message.key),
        }
        Ok(())
    }

    fn broadcast_subscriber_count(&self) -> usize {
        self.room_notifiers.len() + self.user_notifiers.len()
    }
}

impl Default for EventNotifier {
    fn default() -> Self {
        Self::new()
    }
}

impl Clone for EventNotifier {
    fn clone(&self) -> Self {
        Self {
            room_notifiers: self.room_notifiers.clone(),
            user_notifiers: self.user_notifiers.clone(),
            redis_pool: self.redis_pool.clone(),
            redis_url: self.redis_url.clone(),
            instance_id: self.instance_id.clone(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_notify_room_wakes_waiter() {
        let notifier = EventNotifier::new();
        let room_id = "!test:example.com".to_string();

        let notifier_clone = notifier.clone();
        let room_id_clone = room_id.clone();
        let handle = tokio::spawn(async move {
            notifier_clone.wait_for_room(&[room_id_clone], tokio::time::Duration::from_secs(5)).await;
        });

        // Give the waiter time to register
        tokio::time::sleep(tokio::time::Duration::from_millis(10)).await;

        notifier.notify_room(&room_id);

        handle.await.unwrap();
    }

    #[tokio::test]
    async fn test_notify_user_wakes_waiter() {
        let notifier = EventNotifier::new();
        let user_id = "@alice:example.com".to_string();

        let notifier_clone = notifier.clone();
        let user_id_clone = user_id.clone();
        let handle = tokio::spawn(async move {
            notifier_clone.wait_for_user(&user_id_clone, tokio::time::Duration::from_secs(5)).await;
        });

        tokio::time::sleep(tokio::time::Duration::from_millis(10)).await;

        notifier.notify_user(&user_id);

        handle.await.unwrap();
    }

    #[tokio::test]
    async fn test_wait_for_room_timeout() {
        let notifier = EventNotifier::new();
        let room_id = "!timeout:example.com".to_string();

        let start = tokio::time::Instant::now();
        notifier.wait_for_room(&[room_id], tokio::time::Duration::from_millis(50)).await;
        let elapsed = start.elapsed();

        assert!(elapsed >= tokio::time::Duration::from_millis(40));
    }

    #[tokio::test]
    async fn test_notify_room_without_waiters() {
        let notifier = EventNotifier::new();
        // Should not panic
        notifier.notify_room("!empty:example.com");
        notifier.notify_user("@nobody:example.com");
    }

    // ========== EventNotifyKind tests ==========

    #[test]
    fn test_event_notify_kind_room() {
        assert_eq!(EventNotifyKind::Room, EventNotifyKind::Room);
        assert_ne!(EventNotifyKind::Room, EventNotifyKind::User);
    }

    #[test]
    fn test_event_notify_kind_clone() {
        let kind = EventNotifyKind::Room;
        assert_eq!(kind, EventNotifyKind::Room);
    }

    #[test]
    fn test_event_notify_kind_copy() {
        let kind = EventNotifyKind::Room;
        let copied = kind;
        assert_eq!(kind, copied);
        assert_eq!(copied, EventNotifyKind::Room);
    }

    // ========== EventNotifyMessage tests ==========

    #[test]
    fn test_event_notify_message_room() {
        let msg = EventNotifyMessage {
            kind: EventNotifyKind::Room,
            key: "!room:example.com".to_string(),
            sender_instance: "instance-1".to_string(),
        };
        assert_eq!(msg.kind, EventNotifyKind::Room);
        assert_eq!(msg.key, "!room:example.com");
        assert_eq!(msg.sender_instance, "instance-1");
    }

    #[test]
    fn test_event_notify_message_user() {
        let msg = EventNotifyMessage {
            kind: EventNotifyKind::User,
            key: "@alice:example.com".to_string(),
            sender_instance: "instance-2".to_string(),
        };
        assert_eq!(msg.kind, EventNotifyKind::User);
        assert_eq!(msg.key, "@alice:example.com");
    }

    #[test]
    fn test_event_notify_message_serialization() {
        let msg = EventNotifyMessage {
            kind: EventNotifyKind::Room,
            key: "!room:example.com".to_string(),
            sender_instance: "instance-1".to_string(),
        };
        let json = serde_json::to_string(&msg).unwrap();
        let deserialized: EventNotifyMessage = serde_json::from_str(&json).unwrap();
        assert_eq!(deserialized.kind, EventNotifyKind::Room);
        assert_eq!(deserialized.key, "!room:example.com");
        assert_eq!(deserialized.sender_instance, "instance-1");
    }

    // ========== EventNotifier tests ==========

    #[test]
    fn test_event_notifier_new() {
        let notifier = EventNotifier::new();
        assert_eq!(notifier.broadcast_subscriber_count(), 0);
    }

    #[test]
    fn test_event_notifier_default() {
        let notifier = EventNotifier::default();
        assert_eq!(notifier.broadcast_subscriber_count(), 0);
    }

    #[test]
    fn test_event_notifier_with_instance_id() {
        let notifier = EventNotifier::new().with_instance_id("custom-id".to_string());
        // Just verify it doesn't panic and the builder works
        let _ = notifier.clone();
    }

    #[test]
    fn test_event_notifier_clone() {
        let notifier = EventNotifier::new();
        let cloned = notifier.clone();
        assert_eq!(cloned.broadcast_subscriber_count(), notifier.broadcast_subscriber_count());
    }

    #[test]
    fn test_event_notifier_debug() {
        let notifier = EventNotifier::new();
        let debug_str = format!("{:?}", notifier);
        assert!(debug_str.contains("EventNotifier"));
        assert!(debug_str.contains("has_redis"));
    }

    // ========== EventBroadcaster impl tests ==========

    #[test]
    fn test_event_notifier_broadcast_subscriber_count() {
        let notifier = EventNotifier::new();
        assert_eq!(notifier.broadcast_subscriber_count(), 0);
    }
}
