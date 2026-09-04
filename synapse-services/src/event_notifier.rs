use crate::event_broadcaster_trait::{BroadcastError, EventBroadcaster};
use dashmap::DashMap;
use deadpool_redis::Pool;
use futures::StreamExt;
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use tokio::sync::Notify;
use tokio_util::sync::CancellationToken;
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

    /// Returns the notification slots a sync connection for `user_id` should
    /// watch: the user's own slot (to-device messages, device-list changes)
    /// plus one slot per joined room (timeline events, receipts, typing).
    ///
    /// # Ordering contract (important)
    ///
    /// Callers **must** register their waiters — i.e. create the
    /// [`Notified`](tokio::sync::futures::Notified) futures and call
    /// `Notified::enable()` — *before* reading state from the database, and
    /// only await them afterwards. [`Notify::notify_waiters`] does not store a
    /// permit, so a notification that fires between the read and the
    /// registration would be lost and the client would stall until its
    /// timeout. Registering first makes the sequence race-free: producers
    /// write to the database *then* notify, so any event that is invisible to
    /// the caller's read is guaranteed to notify an already-registered waiter.
    ///
    /// # Growth
    ///
    /// Slots are created lazily. Idle slots (no live waiters, i.e. the map
    /// holds the only `Arc` reference) are reclaimed by
    /// [`evict_idle_slots`][EventNotifier::evict_idle_slots], which the
    /// container runs periodically via
    /// [`start_idle_slot_evictor`][EventNotifier::start_idle_slot_evictor]
    /// (A-7). A slot is never evicted while a sync connection still holds
    /// its `Arc<Notify>`, so in-flight waits are unaffected.
    pub fn slots_for(&self, user_id: &str, room_ids: &[String]) -> Vec<Arc<Notify>> {
        let mut slots = Vec::with_capacity(room_ids.len() + 1);
        slots.push(self.get_or_create_user_notify(user_id));
        for room_id in room_ids {
            slots.push(self.get_or_create_room_notify(room_id));
        }
        slots
    }

    /// Evict slots that have no live waiters (A-7).
    ///
    /// A slot is "idle" when the map holds the only `Arc<Notify>` reference
    /// (`strong_count == 1`): every waiter keeps its own clone for the whole
    /// duration of a long-poll, so any slot with `strong_count > 1` is still
    /// in use and must be kept. Returns the number of evicted entries.
    ///
    /// Race safety: `DashMap::retain` holds the shard write lock, so a
    /// concurrent [`slots_for`][EventNotifier::slots_for] on the same shard
    /// blocks until the retain pass finishes; a waiter that already cloned
    /// the `Arc` keeps the count above 1 and is never evicted. A slot evicted
    /// between two `slots_for` calls is simply re-created on the next call —
    /// a `Notify` with no registered waiters carries no state worth keeping.
    pub fn evict_idle_slots(&self) -> usize {
        let before = self.room_notifiers.len() + self.user_notifiers.len();
        self.room_notifiers.retain(|_, notify| Arc::strong_count(notify) > 1);
        self.user_notifiers.retain(|_, notify| Arc::strong_count(notify) > 1);
        before - (self.room_notifiers.len() + self.user_notifiers.len())
    }

    /// Start a background task that periodically evicts idle slots (A-7).
    ///
    /// Without this, the notifier maps grow by one entry per distinct
    /// user/room ever waited on and never shrink. The container wires this
    /// once at startup; the loop exits cleanly when `shutdown` is cancelled
    /// (the server's `shutdown_token` propagates here via `services`).
    ///
    /// Without the shutdown hook, the returned `JoinHandle` could only be
    /// hard-aborted on SIGTERM — the in-flight `retain` would still finish
    /// (cheap, lock-free), but the next tick would never fire, leaving the
    /// caller with no clean exit signal during graceful shutdown.
    pub fn start_idle_slot_evictor(
        &self,
        interval: std::time::Duration,
        shutdown: CancellationToken,
    ) -> tokio::task::JoinHandle<()> {
        let room_notifiers = self.room_notifiers.clone();
        let user_notifiers = self.user_notifiers.clone();
        tokio::spawn(async move {
            let mut ticker = tokio::time::interval(interval);
            // Skip the immediate first tick; there is nothing to evict at startup.
            ticker.tick().await;
            loop {
                tokio::select! {
                    biased;
                    _ = shutdown.cancelled() => {
                        debug!("EventNotifier idle slot evictor exiting on shutdown");
                        break;
                    }
                    _ = ticker.tick() => {
                        let before = room_notifiers.len() + user_notifiers.len();
                        room_notifiers.retain(|_, notify| Arc::strong_count(notify) > 1);
                        user_notifiers.retain(|_, notify| Arc::strong_count(notify) > 1);
                        let evicted = before - (room_notifiers.len() + user_notifiers.len());
                        if evicted > 0 {
                            debug!(evicted, "EventNotifier: evicted idle notify slots");
                        }
                    }
                }
            }
        })
    }

    /// Notify all connections waiting for events in the given room.
    pub fn notify_room(&self, room_id: &str) {
        self.notify_room_local(room_id);
        self.publish_redis(EventNotifyKind::Room, room_id);
    }

    /// Notify all connections waiting for data for the given user (e.g.
    /// to-device messages).
    pub fn notify_user(&self, user_id: &str) {
        self.notify_user_local(user_id);
        self.publish_redis(EventNotifyKind::User, user_id);
    }

    /// Local-only wake-up: triggers `notify_waiters()` on the room's `Notify`
    /// without publishing to Redis. Used by the Redis subscriber to handle
    /// cross-instance messages without creating a publish loop.
    fn notify_room_local(&self, room_id: &str) {
        if let Some(notify) = self.room_notifiers.get(room_id) {
            notify.notify_waiters();
        }
    }

    /// Local-only wake-up for a user slot (see [`notify_room_local`]).
    fn notify_user_local(&self, user_id: &str) {
        if let Some(notify) = self.user_notifiers.get(user_id) {
            notify.notify_waiters();
        }
    }

    /// Handle a Redis pub/sub message received from another server instance.
    ///
    /// Skips self-echo (when `sender_instance == self.instance_id`) to avoid
    /// double-notification — the local `notify_room`/`notify_user` call that
    /// triggered the Redis publish has already woken local waiters.
    ///
    /// For messages from *other* instances, wakes local waiters via
    /// `notify_room_local`/`notify_user_local` (no Redis re-publish).
    pub fn handle_redis_message(&self, msg: &EventNotifyMessage) {
        // Skip self-echo: the local notify_room/notify_user that triggered
        // the Redis publish has already woken local waiters.
        if msg.sender_instance == self.instance_id {
            return;
        }

        match msg.kind {
            EventNotifyKind::Room => self.notify_room_local(&msg.key),
            EventNotifyKind::User => self.notify_user_local(&msg.key),
        }
    }

    /// Start a background Redis pub/sub subscriber that listens for event
    /// notifications from other server instances and wakes local waiters.
    ///
    /// This is the receiving end of the cross-instance fan-out. When instance
    /// B writes an event and calls `notify_room`, the message is published to
    /// Redis; this subscriber on instance A receives it and calls
    /// `handle_redis_message` to wake instance A's waiting sync connections.
    ///
    /// # Behaviour when Redis is not configured
    ///
    /// If `with_redis` was never called, this method is a no-op and returns
    /// `Ok(())`. Single-instance deployments don't need cross-instance
    /// fan-out.
    ///
    /// # Reconnection
    ///
    /// If the Redis connection drops, the subscriber retries after 1 second.
    ///
    /// # Shutdown
    ///
    /// The reconnect loop and the inner pubsub `while let` both race
    /// against `shutdown.cancelled()` so SIGTERM exits cleanly without
    /// leaving a dangling subscriber task behind.
    pub fn start_redis_subscriber(&self, shutdown: CancellationToken) -> Result<(), String> {
        let Some(redis_url) = &self.redis_url else {
            debug!("EventNotifier: Redis not configured, skipping subscriber startup");
            return Ok(());
        };

        let client =
            redis::Client::open(redis_url.as_str()).map_err(|e| format!("Failed to create Redis client: {e}"))?;

        let channel = EVENT_NOTIFY_CHANNEL.to_string();
        let instance_id = self.instance_id.clone();
        let room_notifiers = self.room_notifiers.clone();
        let user_notifiers = self.user_notifiers.clone();

        info!(
            channel = %channel,
            instance_id = %instance_id,
            "Starting EventNotifier Redis subscriber for cross-instance fan-out"
        );

        tokio::spawn(async move {
            loop {
                if shutdown.is_cancelled() {
                    debug!("EventNotifier Redis subscriber exiting on shutdown");
                    break;
                }
                match Self::subscribe_and_listen(
                    &client,
                    &channel,
                    &instance_id,
                    &room_notifiers,
                    &user_notifiers,
                    shutdown.clone(),
                )
                .await
                {
                    Ok(_) => {
                        debug!("EventNotifier subscription ended normally, reconnecting...");
                    }
                    Err(e) => {
                        warn!("EventNotifier subscription error: {e}, reconnecting in 1s...");
                        // Race the backoff sleep against shutdown so a SIGTERM
                        // arriving mid-retry exits immediately.
                        tokio::select! {
                            biased;
                            _ = shutdown.cancelled() => break,
                            _ = tokio::time::sleep(std::time::Duration::from_secs(1)) => {}
                        }
                    }
                }
            }
        });

        Ok(())
    }

    /// Inner subscribe-and-listen loop for the Redis subscriber.
    ///
    /// Connects, subscribes to the channel, and processes messages until the
    /// connection drops or `shutdown` is cancelled. Returns `Ok(())` on
    /// normal disconnect or shutdown, `Err` on connection failure.
    async fn subscribe_and_listen(
        client: &redis::Client,
        channel: &str,
        instance_id: &str,
        room_notifiers: &Arc<DashMap<String, Arc<Notify>>>,
        user_notifiers: &Arc<DashMap<String, Arc<Notify>>>,
        shutdown: CancellationToken,
    ) -> Result<(), String> {
        let mut pubsub = client.get_async_pubsub().await.map_err(|e| format!("Failed to get async pubsub: {e}"))?;

        pubsub.subscribe(channel).await.map_err(|e| format!("Failed to subscribe to channel: {e}"))?;

        debug!("EventNotifier subscribed to channel: {}", channel);

        let mut message_stream = pubsub.on_message();

        loop {
            tokio::select! {
                biased;
                _ = shutdown.cancelled() => {
                    debug!("EventNotifier pubsub loop exiting on shutdown");
                    break Ok(());
                }
                msg = message_stream.next() => {
                    let Some(msg) = msg else {
                        // Stream ended normally (connection dropped).
                        break Ok(());
                    };
                    let payload: Vec<u8> = match msg.get_payload() {
                        Ok(p) => p,
                        Err(e) => {
                            warn!("Failed to get pub/sub message payload: {e}");
                            continue;
                        }
                    };

                    let notify_msg: EventNotifyMessage = match serde_json::from_slice(&payload) {
                        Ok(m) => m,
                        Err(e) => {
                            warn!("Failed to decode EventNotifyMessage: {e}");
                            continue;
                        }
                    };

                    // Skip self-echo
                    if notify_msg.sender_instance == instance_id {
                        continue;
                    }

                    // Wake local waiters
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
            }
        }
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
        let kind_dbg = format!("{:?}", kind);
        let key_dbg = key.to_string();
        // W-05: tag the fire-and-forget task with a span so production
        // logs can trace it without polluting the caller's context.
        let span = tracing::info_span!(
            "EventNotifier.publish_redis",
            kind = %kind_dbg,
            key = %key_dbg,
            sender_instance = %self.instance_id,
        );
        tokio::spawn(
            async move {
                let _enter = span.enter();
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
            },
        );
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

        let slots = notifier.slots_for("@waiter:example.com", std::slice::from_ref(&room_id));
        let room_slot = slots[1].clone();
        let handle = tokio::spawn(async move {
            tokio::time::timeout(tokio::time::Duration::from_secs(5), room_slot.notified()).await
        });

        // Give the waiter time to register
        tokio::time::sleep(tokio::time::Duration::from_millis(10)).await;

        notifier.notify_room(&room_id);

        handle.await.unwrap().expect("waiter should be woken before timeout");
    }

    #[tokio::test]
    async fn test_notify_user_wakes_waiter() {
        let notifier = EventNotifier::new();
        let user_id = "@alice:example.com".to_string();

        let slots = notifier.slots_for(&user_id, &[]);
        let user_slot = slots[0].clone();
        let handle = tokio::spawn(async move {
            tokio::time::timeout(tokio::time::Duration::from_secs(5), user_slot.notified()).await
        });

        tokio::time::sleep(tokio::time::Duration::from_millis(10)).await;

        notifier.notify_user(&user_id);

        handle.await.unwrap().expect("waiter should be woken before timeout");
    }

    #[test]
    fn test_slots_for_returns_user_slot_plus_one_per_room() {
        let notifier = EventNotifier::new();
        let rooms = vec!["!a:example.com".to_string(), "!b:example.com".to_string()];

        let slots = notifier.slots_for("@alice:example.com", &rooms);

        assert_eq!(slots.len(), 3, "expected 1 user slot + 2 room slots");
    }

    #[test]
    fn test_slots_for_is_stable_across_calls() {
        let notifier = EventNotifier::new();
        let rooms = vec!["!a:example.com".to_string()];

        let first = notifier.slots_for("@alice:example.com", &rooms);
        let second = notifier.slots_for("@alice:example.com", &rooms);

        // The same logical slot must hand back the same `Notify` instance,
        // otherwise a notification would be delivered to a different object
        // than the one the waiter registered on.
        for (a, b) in first.iter().zip(second.iter()) {
            assert!(Arc::ptr_eq(a, b));
        }
    }

    /// Regression guard for the long-poll ordering contract: a notification
    /// that fires *after* the waiter is registered but *before* it is awaited
    /// must still wake the waiter. This is the exact sequence sliding sync
    /// relies on (register → read database → await), and it only holds because
    /// `Notified::enable()` registers the waiter eagerly.
    #[tokio::test]
    async fn test_registered_waiter_survives_notify_before_await() {
        let notifier = EventNotifier::new();
        let rooms = vec!["!race:example.com".to_string()];
        let slots = notifier.slots_for("@alice:example.com", &rooms);

        let mut waiters: Vec<_> = slots
            .iter()
            .map(|slot| {
                let mut fut = Box::pin(slot.notified());
                fut.as_mut().enable();
                fut
            })
            .collect();

        // Fires while the waiter is registered but not yet awaited — this is
        // the window that would be lost without `enable()`.
        notifier.notify_room("!race:example.com");

        let woken = tokio::time::timeout(tokio::time::Duration::from_millis(200), async {
            futures::future::select_all(waiters.iter_mut()).await;
        })
        .await;

        assert!(woken.is_ok(), "waiter registered before the notification must still be woken");
    }

    #[tokio::test]
    async fn test_slot_wait_timeout() {
        let notifier = EventNotifier::new();
        let room_id = "!timeout:example.com".to_string();

        let slots = notifier.slots_for("@waiter:example.com", std::slice::from_ref(&room_id));
        let room_slot = slots[1].clone();

        let start = tokio::time::Instant::now();
        let result = tokio::time::timeout(tokio::time::Duration::from_millis(50), room_slot.notified()).await;
        let elapsed = start.elapsed();

        assert!(result.is_err(), "no notification should fire; the wait must time out");
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

    // ========== S8: handle_redis_message tests ==========

    /// S8: A Redis message from another instance for a room must wake the
    /// local room waiter. This is the core cross-instance fan-out scenario.
    #[tokio::test]
    async fn s8_handle_redis_message_from_other_instance_wakes_room_waiter() {
        let notifier_a = EventNotifier::new().with_instance_id("instance-A".to_string());
        let room_id = "!cross:example.com".to_string();

        // Register a waiter on instance A for this room
        let slots = notifier_a.slots_for("@alice:example.com", std::slice::from_ref(&room_id));
        let room_slot = slots[1].clone();
        let waiter = tokio::spawn(async move {
            tokio::time::timeout(tokio::time::Duration::from_secs(2), room_slot.notified()).await
        });

        // Give the waiter time to register
        tokio::time::sleep(tokio::time::Duration::from_millis(20)).await;

        // Simulate a Redis message from instance B
        let msg =
            EventNotifyMessage { kind: EventNotifyKind::Room, key: room_id, sender_instance: "instance-B".to_string() };
        notifier_a.handle_redis_message(&msg);

        let result = waiter.await.unwrap();
        assert!(result.is_ok(), "waiter must be woken by cross-instance room notification");
    }

    /// S8: A Redis message from another instance for a user must wake the
    /// local user waiter (e.g. to-device messages).
    #[tokio::test]
    async fn s8_handle_redis_message_from_other_instance_wakes_user_waiter() {
        let notifier_a = EventNotifier::new().with_instance_id("instance-A".to_string());
        let user_id = "@bob:example.com".to_string();

        let slots = notifier_a.slots_for(&user_id, &[]);
        let user_slot = slots[0].clone();
        let waiter = tokio::spawn(async move {
            tokio::time::timeout(tokio::time::Duration::from_secs(2), user_slot.notified()).await
        });

        tokio::time::sleep(tokio::time::Duration::from_millis(20)).await;

        let msg =
            EventNotifyMessage { kind: EventNotifyKind::User, key: user_id, sender_instance: "instance-B".to_string() };
        notifier_a.handle_redis_message(&msg);

        let result = waiter.await.unwrap();
        assert!(result.is_ok(), "waiter must be woken by cross-instance user notification");
    }

    /// S8: A Redis message from self (echo) must NOT wake local waiters.
    /// The local notify_room/notify_user call that triggered the publish has
    /// already woken local waiters; re-waking would be redundant (though
    /// harmless for Notify, it wastes CPU on busy systems).
    ///
    /// More importantly, skipping self-echo prevents potential notification
    /// storms in edge cases where multiple local waiters re-trigger writes.
    #[tokio::test]
    async fn s8_handle_redis_message_skips_self_echo() {
        let notifier = EventNotifier::new().with_instance_id("instance-A".to_string());
        let room_id = "!echo:example.com".to_string();

        let slots = notifier.slots_for("@alice:example.com", std::slice::from_ref(&room_id));
        let room_slot = slots[1].clone();
        let waiter = tokio::spawn(async move {
            tokio::time::timeout(tokio::time::Duration::from_millis(200), room_slot.notified()).await
        });

        tokio::time::sleep(tokio::time::Duration::from_millis(20)).await;

        // Simulate a Redis echo: same instance_id
        let msg =
            EventNotifyMessage { kind: EventNotifyKind::Room, key: room_id, sender_instance: "instance-A".to_string() };
        notifier.handle_redis_message(&msg);

        let result = waiter.await.unwrap();
        assert!(result.is_err(), "self-echo must NOT wake local waiters");
    }

    /// S8: `start_redis_subscriber` must be a safe no-op when Redis is not
    /// configured. The container always calls this method; it must not panic
    /// or error when `with_redis` was never called.
    #[test]
    fn s8_start_redis_subscriber_noop_without_redis() {
        let notifier = EventNotifier::new();
        let result = notifier.start_redis_subscriber(tokio_util::sync::CancellationToken::new());
        assert!(result.is_ok(), "start_redis_subscriber without Redis must return Ok(())");
    }

    // ========== A-7: idle slot eviction tests ==========

    /// A-7: slots whose waiters have all gone away must be reclaimable,
    /// otherwise the maps grow monotonically with every user/room ever seen.
    #[test]
    fn a7_evict_idle_slots_removes_unreferenced_slots() {
        let notifier = EventNotifier::new();
        {
            let _slots = notifier
                .slots_for("@alice:example.com", &["!r1:example.com".to_string(), "!r2:example.com".to_string()]);
            assert_eq!(notifier.broadcast_subscriber_count(), 3);
        }
        // All Arc clones dropped; only the maps hold references now.
        let evicted = notifier.evict_idle_slots();
        assert_eq!(evicted, 3, "all idle slots should be evicted");
        assert_eq!(notifier.broadcast_subscriber_count(), 0);
    }

    /// A-7: a slot still held by a live sync connection must survive eviction.
    #[test]
    fn a7_evict_idle_slots_keeps_slots_with_live_holders() {
        let notifier = EventNotifier::new();
        let held = notifier.slots_for("@bob:example.com", &["!live:example.com".to_string()]);
        {
            let _transient = notifier.slots_for("@carol:example.com", &[]);
        }
        let evicted = notifier.evict_idle_slots();
        assert_eq!(evicted, 1, "only the unreferenced user slot should be evicted");
        assert_eq!(notifier.broadcast_subscriber_count(), 2);

        // The held slot must still be the same live Notify instance.
        let again = notifier.slots_for("@bob:example.com", &["!live:example.com".to_string()]);
        for (a, b) in held.iter().zip(again.iter()) {
            assert!(Arc::ptr_eq(a, b), "live slots must not be replaced by eviction");
        }
    }

    /// A-7: the background evictor must actually reclaim idle slots.
    #[tokio::test]
    async fn a7_background_evictor_reclaims_idle_slots() {
        let notifier = EventNotifier::new();
        {
            let _slots = notifier.slots_for("@dave:example.com", &["!bg:example.com".to_string()]);
        }
        let handle = notifier.start_idle_slot_evictor(std::time::Duration::from_millis(20), tokio_util::sync::CancellationToken::new());
        tokio::time::sleep(std::time::Duration::from_millis(120)).await;
        handle.abort();
        assert_eq!(notifier.broadcast_subscriber_count(), 0, "background evictor should reclaim idle slots");
    }
}
