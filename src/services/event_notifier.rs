use crate::common::traits::{BroadcastError, EventBroadcaster};
use dashmap::DashMap;
use deadpool_redis::Pool;
use redis::Client;
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use tokio::sync::Notify;
use tracing::{debug, info, warn};

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
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
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
/// [`EventBroadcaster`] trait.
///
/// * **This type** → local sync wake-up (room / user `Notify` + Redis fan-out)
/// * `federation::EventBroadcaster` → federation outbound (PDU/EDU batching + retry)
/// * `WorkerBus` → inter-worker pub/sub (replication commands)
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

        let notifiers: Vec<Arc<Notify>> = room_ids
            .iter()
            .map(|room_id| self.get_or_create_room_notify(room_id))
            .collect();

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

    /// Start a background task that subscribes to the Redis event notification
    /// channel and forwards remote notifications to local notifiers.
    pub fn start_redis_subscriber(&self) {
        let Some(redis_url) = self.redis_url.clone() else {
            return;
        };

        let room_notifiers = self.room_notifiers.clone();
        let user_notifiers = self.user_notifiers.clone();
        let instance_id = self.instance_id.clone();

        info!("Starting event notifier Redis subscriber on channel: {} (instance: {})", EVENT_NOTIFY_CHANNEL, instance_id);

        tokio::spawn(async move {
            loop {
                if let Err(e) =
                    Self::subscribe_and_listen(&redis_url, &room_notifiers, &user_notifiers, &instance_id).await
                {
                    warn!("Event notifier Redis subscriber error: {}, reconnecting in 1s...", e);
                    tokio::time::sleep(tokio::time::Duration::from_secs(1)).await;
                }
            }
        });
    }

    fn get_or_create_room_notify(&self, room_id: &str) -> Arc<Notify> {
        self.room_notifiers
            .entry(room_id.to_string())
            .or_insert_with(|| Arc::new(Notify::new()))
            .value()
            .clone()
    }

    fn get_or_create_user_notify(&self, user_id: &str) -> Arc<Notify> {
        self.user_notifiers
            .entry(user_id.to_string())
            .or_insert_with(|| Arc::new(Notify::new()))
            .value()
            .clone()
    }

    fn publish_redis(&self, kind: EventNotifyKind, key: &str) {
        let Some(pool) = &self.redis_pool else {
            return;
        };

        let msg = EventNotifyMessage { kind, key: key.to_string(), sender_instance: self.instance_id.clone() };

        let encoded = match serde_json::to_vec(&msg) {
            Ok(v) => v,
            Err(e) => {
                warn!("Failed to encode event notify message: {}", e);
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
                        debug!("Failed to publish event notification to Redis: {}", e);
                    }
                }
                Err(e) => {
                    debug!("Failed to get Redis connection for event notification: {}", e);
                }
            }
        });
    }

    async fn subscribe_and_listen(
        redis_url: &str,
        room_notifiers: &Arc<DashMap<String, Arc<Notify>>>,
        user_notifiers: &Arc<DashMap<String, Arc<Notify>>>,
        instance_id: &str,
    ) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        use futures::StreamExt;

        let client = Client::open(redis_url)?;
        let mut pubsub = client.get_async_pubsub().await?;

        pubsub.subscribe(EVENT_NOTIFY_CHANNEL).await?;
        let mut message_stream = pubsub.on_message();

        while let Some(msg) = message_stream.next().await {
            let payload: Vec<u8> = msg.get_payload()?;
            match serde_json::from_slice::<EventNotifyMessage>(&payload) {
                Ok(notify_msg) => {
                    if notify_msg.sender_instance == instance_id {
                        continue;
                    }

                    match notify_msg.kind {
                        EventNotifyKind::Room => {
                            if let Some(notify) = room_notifiers.get(&notify_msg.key) {
                                notify.notify_waiters();
                            }
                        }
                        EventNotifyKind::User => {
                            if let Some(notify) = user_notifiers.get(&notify_msg.key) {
                                notify.notify_waiters();
                            }
                        }
                    }
                }
                Err(e) => {
                    debug!("Failed to decode event notify message: {}", e);
                }
            }
        }

        Ok(())
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
