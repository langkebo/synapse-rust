use crate::worker::protocol::ReplicationCommand;
use redis::Client;
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use synapse_common::current_timestamp_millis;
use synapse_common::error::ApiError;
use tokio::sync::{broadcast, mpsc, RwLock};
use tracing::{debug, info, warn};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BusMessage {
    pub channel: String,
    pub sender: String,
    pub timestamp: i64,
    pub payload: Vec<u8>,
}

/// WORK-01: Commands sent to the subscriber task for dynamic channel management.
/// Redis UNSUBSCRIBE must be sent on the same connection that subscribed,
/// so we use a command channel to instruct the subscriber task.
#[derive(Debug, Clone)]
enum SubCommand {
    /// Unsubscribe from a Redis Pub/Sub channel (full channel name with prefix)
    Unsubscribe(String),
}

/// WORK-05: Record of a failed Redis publish after all retries exhausted.
/// Stored in an in-memory ring buffer for inspection and manual replay.
#[derive(Debug, Clone)]
pub struct FailedPublish {
    pub channel: String,
    pub payload: Vec<u8>,
    pub error: String,
    pub failed_at: i64,
}

const FAILED_PUBLISH_RING_SIZE: usize = 256;

#[derive(Debug, Clone)]
pub struct RedisBusConfig {
    pub url: String,
    pub pool_size: u32,
    pub channel_prefix: String,
}

impl Default for RedisBusConfig {
    fn default() -> Self {
        Self { url: "redis://127.0.0.1:6379".to_string(), pool_size: 10, channel_prefix: "synapse".to_string() }
    }
}

/// Worker bus backed by Redis Pub/Sub for cross-instance communication.
///
/// When `connect()` is called, it creates a Redis connection pool for publishing
/// and spawns a subscriber task that listens on Redis Pub/Sub channels and
/// forwards messages to local in-memory broadcast subscribers.
///
/// If Redis is unavailable, the bus falls back to in-memory-only mode (single-
/// instance operation) so that the server can still start without Redis.
pub struct WorkerBus {
    config: RedisBusConfig,
    server_name: String,
    instance_name: String,
    subscribers: Arc<RwLock<Vec<broadcast::Sender<BusMessage>>>>,
    command_tx: mpsc::Sender<BusMessage>,
    command_rx: Option<mpsc::Receiver<BusMessage>>,
    // PERF-03: 以下状态字段使用 Arc 共享 —— Clone 直接共享而非快照，
    // 彻底消除 blocking_read 在异步上下文 panic 的隐患
    connected: Arc<RwLock<bool>>,
    /// Redis client for publishing and subscribing.
    /// `None` when Redis is not configured or connection failed (in-memory mode).
    redis_client: Arc<RwLock<Option<Arc<Client>>>>,
    /// Redis connection pool for publishing.
    redis_pool: Arc<RwLock<Option<Arc<deadpool_redis::Pool>>>>,
    /// Handle for the subscriber task, so we can abort it on disconnect.
    subscriber_task: Arc<RwLock<Option<tokio::task::JoinHandle<()>>>>,
    /// Channels that the subscriber task listens on.
    subscribed_channels: Arc<RwLock<Vec<String>>>,
    /// WORK-01: Command channel for sending unsubscribe requests to the subscriber task.
    sub_command_tx: mpsc::Sender<SubCommand>,
    sub_command_rx: std::sync::Mutex<Option<mpsc::Receiver<SubCommand>>>,
    /// WORK-05: In-memory ring buffer for failed Redis publishes.
    failed_publishes: Arc<RwLock<std::collections::VecDeque<FailedPublish>>>,
}

impl WorkerBus {
    pub fn new(config: RedisBusConfig, server_name: String, instance_name: String) -> Self {
        let (command_tx, command_rx) = mpsc::channel(1000);
        let (sub_command_tx, sub_command_rx) = mpsc::channel(100);

        Self {
            config,
            server_name,
            instance_name,
            subscribers: Arc::new(RwLock::new(Vec::new())),
            command_tx,
            command_rx: Some(command_rx),
            connected: Arc::new(RwLock::new(false)),
            redis_client: Arc::new(RwLock::new(None)),
            redis_pool: Arc::new(RwLock::new(None)),
            subscriber_task: Arc::new(RwLock::new(None)),
            subscribed_channels: Arc::new(RwLock::new(Vec::new())),
            sub_command_tx,
            sub_command_rx: std::sync::Mutex::new(Some(sub_command_rx)),
            failed_publishes: Arc::new(RwLock::new(std::collections::VecDeque::with_capacity(FAILED_PUBLISH_RING_SIZE))),
        }
    }

    /// Connect to Redis. If the connection fails, falls back to in-memory mode.
    pub async fn connect(&self) -> Result<(), ApiError> {
        info!(
            redis_url = %self.config.url,
            pool_size = self.config.pool_size,
            channel_prefix = %self.config.channel_prefix,
            server_name = %self.server_name,
            instance_name = %self.instance_name,
            "Connecting to Redis"
        );

        // Try to create Redis client and pool
        match self.try_connect_redis().await {
            Ok(()) => {
                info!(
                    server_name = %self.server_name,
                    instance_name = %self.instance_name,
                    "Redis bus connected successfully — cross-instance pub/sub enabled"
                );
            }
            Err(e) => {
                warn!(
                    error = %e,
                    server_name = %self.server_name,
                    instance_name = %self.instance_name,
                    "Failed to connect to Redis — falling back to in-memory-only mode (single-instance)"
                );
            }
        }

        // Mark as connected regardless — in-memory mode still works for single instance
        let mut connected = self.connected.write().await;
        *connected = true;

        Ok(())
    }

    /// Attempt to create a Redis client, connection pool, and subscriber task.
    async fn try_connect_redis(&self) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        let client = Client::open(self.config.url.as_str())?;

        // Test the connection
        let mut test_conn = client.get_multiplexed_async_connection().await?;
        redis::cmd("PING").query_async::<String>(&mut test_conn).await?;

        // Create connection pool for publishing
        let pool_config = deadpool_redis::Config {
            url: Some(self.config.url.clone()),
            pool: Some(deadpool_redis::PoolConfig::new(self.config.pool_size as usize)),
            ..Default::default()
        };
        let pool = pool_config.create_pool(Some(deadpool_redis::Runtime::Tokio1))?;

        let client_arc = Arc::new(client);
        let pool_arc = Arc::new(pool);

        // Store client and pool
        {
            let mut redis_client = self.redis_client.write().await;
            *redis_client = Some(Arc::clone(&client_arc));
        }
        {
            let mut redis_pool = self.redis_pool.write().await;
            *redis_pool = Some(Arc::clone(&pool_arc));
        }

        // Spawn subscriber task
        self.spawn_subscriber_task(client_arc).await;

        Ok(())
    }

    /// Spawn a background task that subscribes to Redis Pub/Sub channels and
    /// forwards messages to local in-memory broadcast subscribers.
    async fn spawn_subscriber_task(&self, client: Arc<Client>) {
        let instance_name = self.instance_name.clone();
        let channel_prefix = self.config.channel_prefix.clone();
        let subscribers = self.subscribers.clone();
        // WORK-01: Share subscribed_channels so the task reads the current list on each reconnect
        let subscribed_channels_arc = self.subscribed_channels.clone();
        // WORK-01: Take the command receiver to listen for unsubscribe requests
        let mut sub_command_rx = self
            .sub_command_rx
            .lock()
            .unwrap()
            .take()
            .expect("sub_command_rx already taken — spawn_subscriber_task called twice?");

        let join_handle = tokio::spawn(async move {
            use futures::StreamExt;

            loop {
                // Read the current subscribed channels on each (re)connect so
                // that unsubscribe takes effect after reconnect.
                let broadcast_channel = format!("{}:broadcast", channel_prefix);
                let current_channels = subscribed_channels_arc.read().await.clone();
                let channels: Vec<String> =
                    std::iter::once(broadcast_channel).chain(current_channels.into_iter()).collect();

                let pubsub = match client.get_async_pubsub().await {
                    Ok(pubsub) => pubsub,
                    Err(e) => {
                        warn!(
                            error = %e,
                            instance = %instance_name,
                            "Failed to create Redis pubsub connection — retrying in 5s"
                        );
                        tokio::time::sleep(std::time::Duration::from_secs(5)).await;
                        continue;
                    }
                };

                let mut pubsub = pubsub;
                let mut subscribe_failed = false;

                for ch in &channels {
                    if let Err(e) = pubsub.subscribe(ch.as_str()).await {
                        warn!(
                            error = %e,
                            channel = %ch,
                            instance = %instance_name,
                            "Failed to subscribe to Redis channel"
                        );
                        subscribe_failed = true;
                        break;
                    }
                }

                if subscribe_failed {
                    tokio::time::sleep(std::time::Duration::from_secs(5)).await;
                    continue;
                }

                info!(
                    instance = %instance_name,
                    channels = ?channels,
                    "Redis pubsub subscriber connected"
                );

                let mut message_stream = pubsub.on_message();
                let mut reconnect_delay = std::time::Duration::from_secs(5);

                loop {
                    tokio::select! {
                        msg = message_stream.next() => {
                            match msg {
                                Some(msg) => {
                                    let payload: Vec<u8> = match msg.get_payload() {
                                        Ok(p) => p,
                                        Err(e) => {
                                            debug!(error = %e, "Failed to get Redis pubsub payload");
                                            continue;
                                        }
                                    };

                                    let bus_message = BusMessage {
                                        channel: msg.get_channel_name().to_string(),
                                        sender: instance_name.clone(),
                                        timestamp: current_timestamp_millis(),
                                        payload,
                                    };

                                    // Forward to local in-memory subscribers
                                    let subs = subscribers.read().await;
                                    for tx in subs.iter() {
                                        let _ = tx.send(bus_message.clone());
                                    }
                                }
                                None => {
                                    // Stream ended (connection lost)
                                    reconnect_delay = std::time::Duration::from_secs(5);
                                    break;
                                }
                            }
                        }
                        cmd = sub_command_rx.recv() => {
                            match cmd {
                                Some(SubCommand::Unsubscribe(ch)) => {
                                    debug!(
                                        channel = %ch,
                                        instance = %instance_name,
                                        "WORK-01: Received unsubscribe command, reconnecting to apply channel changes"
                                    );
                                    // Break inner loop to drop message_stream (which borrows pubsub).
                                    // On reconnect, the subscriber reads the updated subscribed_channels
                                    // list and only subscribes to remaining channels.
                                    reconnect_delay = std::time::Duration::from_secs(1);
                                    break;
                                }
                                None => {
                                    // Command channel closed — continue processing messages
                                }
                            }
                        }
                    }
                }

                warn!(
                    instance = %instance_name,
                    delay_ms = reconnect_delay.as_millis(),
                    "Redis pubsub stream ended or channel change — reconnecting"
                );
                tokio::time::sleep(reconnect_delay).await;
            }
        });

        let mut task = self.subscriber_task.write().await;
        *task = Some(join_handle);
    }

    pub async fn disconnect(&self) {
        // Abort subscriber task
        {
            let mut task = self.subscriber_task.write().await;
            if let Some(handle) = task.take() {
                handle.abort();
            }
        }

        // Clear Redis pool and client
        {
            let mut redis_pool = self.redis_pool.write().await;
            *redis_pool = None;
        }
        {
            let mut redis_client = self.redis_client.write().await;
            *redis_client = None;
        }

        let mut connected = self.connected.write().await;
        *connected = false;

        info!(server_name = %self.server_name, instance_name = %self.instance_name, "Redis bus disconnected");
    }

    pub async fn is_connected(&self) -> bool {
        *self.connected.read().await
    }

    /// Publish a message to the bus. If Redis is connected, the message is
    /// published to Redis Pub/Sub for cross-instance delivery. The message is
    /// also delivered to local in-memory subscribers.
    pub async fn publish(&self, channel: &str, message: &[u8]) -> Result<(), ApiError> {
        if !self.is_connected().await {
            return Err(ApiError::internal("Redis bus not connected"));
        }

        let bus_message = BusMessage {
            channel: channel.to_string(),
            sender: self.instance_name.clone(),
            timestamp: current_timestamp_millis(),
            payload: message.to_vec(),
        };

        let encoded = serde_json::to_vec(&bus_message)
            .map_err(|e| ApiError::internal_with_log("Failed to encode message", &e))?;

        debug!("Publishing to channel {}: {} bytes", channel, encoded.len());

        // Publish to Redis if available (cross-instance delivery)
        let redis_pool = self.redis_pool.read().await;
        if let Some(pool) = redis_pool.as_ref() {
            let full_channel = format!("{}:{}", self.config.channel_prefix, channel);
            let pool = Arc::clone(pool);
            let full_channel = full_channel.clone();
            let encoded = encoded.clone();
            // WORK-05: Clone the DLQ Arc so the spawned task can store failed messages
            let failed_publishes = Arc::clone(&self.failed_publishes);
            tokio::spawn(async move {
                // WORK-05: 跨实例消息静默丢弃会表现为「另一台实例收不到事件」
                // 的诡异故障。先按指数退避重试（100ms → 200ms → 400ms），
                // 全部失败后存入内存 DLQ 环形缓冲，不再静默丢弃。
                const MAX_ATTEMPTS: u32 = 3;
                let mut last_err: Option<String> = None;
                for attempt in 1..=MAX_ATTEMPTS {
                    let attempt_result: Result<(), String> = async {
                        let mut conn = pool
                            .get()
                            .await
                            .map_err(|e| format!("get connection: {e}"))?;
                        use redis::AsyncCommands;
                        conn.publish::<_, _, ()>(&full_channel, &encoded)
                            .await
                            .map_err(|e| format!("publish: {e}"))
                    }
                    .await;

                    match attempt_result {
                        Ok(()) => return,
                        Err(e) => {
                            debug!(
                                error = %e,
                                channel = %full_channel,
                                attempt = attempt,
                                "Redis publish attempt failed, retrying"
                            );
                            last_err = Some(e);
                            if attempt < MAX_ATTEMPTS {
                                tokio::time::sleep(std::time::Duration::from_millis(
                                    100 * (1 << (attempt - 1)),
                                ))
                                .await;
                            }
                        }
                    }
                }
                // WORK-05: All retries exhausted — store in DLQ for inspection and replay
                let failed = FailedPublish {
                    channel: full_channel.clone(),
                    payload: encoded.clone(),
                    error: last_err.clone().unwrap_or_else(|| "unknown".to_string()),
                    failed_at: current_timestamp_millis(),
                };
                {
                    let mut dlq = failed_publishes.write().await;
                    if dlq.len() >= FAILED_PUBLISH_RING_SIZE {
                        dlq.pop_front();
                    }
                    dlq.push_back(failed);
                }
                warn!(
                    error = last_err.as_deref().unwrap_or("unknown"),
                    channel = %full_channel,
                    payload_bytes = encoded.len(),
                    attempts = MAX_ATTEMPTS,
                    "WORK-05: Failed to publish to Redis after retries — message stored in DLQ for replay"
                );
            });
        }

        // Also deliver to local in-memory subscribers
        let subscribers = self.subscribers.read().await;
        for tx in subscribers.iter() {
            if let Err(e) = tx.send(bus_message.clone()) {
                warn!(
                    error = %e,
                    channel = %channel,
                    sender_instance = %self.instance_name,
                    payload_bytes = encoded.len(),
                    "Failed to send to local subscriber"
                );
            }
        }

        Ok(())
    }

    pub async fn subscribe(&self, channels: &[&str]) -> Result<broadcast::Receiver<BusMessage>, ApiError> {
        if !self.is_connected().await {
            return Err(ApiError::internal("Redis bus not connected"));
        }

        let (tx, rx) = broadcast::channel(100);

        {
            let mut subscribers = self.subscribers.write().await;
            subscribers.push(tx);
        }

        // Track subscribed channels for Redis subscriber task
        {
            let mut subscribed = self.subscribed_channels.write().await;
            for ch in channels {
                let full_channel = format!("{}:{}", self.config.channel_prefix, ch);
                if !subscribed.contains(&full_channel) {
                    subscribed.push(full_channel);
                }
            }
        }

        debug!("Subscribed to channels: {:?}", channels);
        Ok(rx)
    }

    // WORK-01: unsubscribe 现在通过命令通道通知订阅任务真正退订 Redis Pub/Sub。
    // 订阅任务收到命令后会断开当前连接并重连，重连时读取更新后的 subscribed_channels
    // 列表，只订阅剩余频道，从而实现真正的 Redis 退订。
    pub async fn unsubscribe(&self, channels: &[&str]) -> Result<(), ApiError> {
        let removed_channels: Vec<String> = {
            let mut subscribed = self.subscribed_channels.write().await;
            let mut removed = Vec::new();
            for ch in channels {
                let full_channel = format!("{}:{}", self.config.channel_prefix, ch);
                let before = subscribed.len();
                subscribed.retain(|c| c != &full_channel);
                if subscribed.len() < before {
                    removed.push(full_channel);
                }
            }
            removed
        };

        // Send unsubscribe commands to the subscriber task so it reconnects
        // with the updated channel list, properly dropping the Redis subscription.
        for ch in &removed_channels {
            if let Err(e) = self.sub_command_tx.send(SubCommand::Unsubscribe(ch.clone())).await {
                warn!(
                    channel = %ch,
                    error = %e,
                    "WORK-01: Failed to send unsubscribe command to subscriber task"
                );
            }
        }

        debug!("Unsubscribed from channels: {:?}", channels);
        Ok(())
    }

    pub async fn broadcast_command(&self, command: &ReplicationCommand) -> Result<(), ApiError> {
        let encoded =
            serde_json::to_vec(command).map_err(|e| ApiError::internal_with_log("Failed to encode command", &e))?;

        self.publish("broadcast", &encoded).await
    }

    pub async fn send_to_worker(&self, worker_id: &str, command: &ReplicationCommand) -> Result<(), ApiError> {
        let encoded =
            serde_json::to_vec(command).map_err(|e| ApiError::internal_with_log("Failed to encode command", &e))?;

        let channel = format!("worker:{worker_id}");
        self.publish(&channel, &encoded).await
    }

    pub async fn send_to_stream_writer(&self, stream_name: &str, command: &ReplicationCommand) -> Result<(), ApiError> {
        let encoded =
            serde_json::to_vec(command).map_err(|e| ApiError::internal_with_log("Failed to encode command", &e))?;

        let channel = format!("stream:{stream_name}");
        self.publish(&channel, &encoded).await
    }

    pub fn get_command_sender(&self) -> mpsc::Sender<BusMessage> {
        self.command_tx.clone()
    }

    pub fn take_command_receiver(&mut self) -> Option<mpsc::Receiver<BusMessage>> {
        self.command_rx.take()
    }

    pub async fn publish_stream_position(&self, stream_name: &str, position: i64) -> Result<(), ApiError> {
        let command = ReplicationCommand::Position { stream_name: stream_name.to_string(), position };

        self.broadcast_command(&command).await
    }

    pub async fn publish_user_sync(&self, user_id: &str, online: bool) -> Result<(), ApiError> {
        use crate::worker::protocol::UserSyncState;

        let command = ReplicationCommand::UserSync {
            user_id: user_id.to_string(),
            state: if online { UserSyncState::Online } else { UserSyncState::Offline },
        };

        self.broadcast_command(&command).await
    }

    pub async fn publish_federation_ack(&self, origin: &str) -> Result<(), ApiError> {
        let command = ReplicationCommand::FederationAck { origin: origin.to_string() };

        self.broadcast_command(&command).await
    }

    pub async fn publish_remove_pushers(&self, app_id: &str, push_key: &str) -> Result<(), ApiError> {
        let command = ReplicationCommand::RemovePushers { app_id: app_id.to_string(), push_key: push_key.to_string() };

        self.broadcast_command(&command).await
    }

    pub async fn get_stats(&self) -> BusStats {
        let subscribers = self.subscribers.read().await;
        let has_redis = self.redis_pool.read().await.is_some();

        BusStats {
            connected: self.is_connected().await,
            server_name: self.server_name.clone(),
            instance_name: self.instance_name.clone(),
            subscriber_count: subscribers.len(),
            redis_enabled: has_redis,
        }
    }

    /// WORK-05: Returns the number of failed Redis publishes in the DLQ.
    pub async fn failed_publish_count(&self) -> usize {
        self.failed_publishes.read().await.len()
    }

    /// WORK-05: Retrieves a snapshot of failed publishes for inspection.
    pub async fn list_failed_publishes(&self) -> Vec<FailedPublish> {
        self.failed_publishes.read().await.iter().cloned().collect()
    }

    /// WORK-05: Attempts to replay all failed publishes in the DLQ.
    /// Successfully replayed messages are removed from the DLQ.
    /// Returns the number of messages successfully replayed.
    pub async fn retry_failed_publishes(&self) -> Result<usize, ApiError> {
        let entries: Vec<FailedPublish> = {
            let mut dlq = self.failed_publishes.write().await;
            dlq.drain(..).collect()
        };

        let mut succeeded = 0;
        let mut still_failed = Vec::new();

        for entry in entries {
            match self.try_republish(&entry.channel, &entry.payload).await {
                Ok(()) => succeeded += 1,
                Err(_) => still_failed.push(entry),
            }
        }

        // Re-add still-failed entries to the DLQ
        if !still_failed.is_empty() {
            let mut dlq = self.failed_publishes.write().await;
            for entry in still_failed {
                if dlq.len() >= FAILED_PUBLISH_RING_SIZE {
                    dlq.pop_front();
                }
                dlq.push_back(entry);
            }
        }

        Ok(succeeded)
    }

    /// Helper: attempt to publish a raw message to a Redis channel without
    /// spawning a background task. Used by `retry_failed_publishes`.
    async fn try_republish(&self, full_channel: &str, payload: &[u8]) -> Result<(), ApiError> {
        let redis_pool = self.redis_pool.read().await;
        let pool = redis_pool.as_ref().ok_or_else(|| ApiError::internal("Redis not connected"))?;

        let mut conn = pool
            .get()
            .await
            .map_err(|e| ApiError::internal_with_log("Redis pool error", &e))?;

        use redis::AsyncCommands;
        conn
            .publish::<_, _, ()>(full_channel, payload)
            .await
            .map_err(|e| ApiError::internal_with_log("Redis publish failed", &e))?;

        Ok(())
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BusStats {
    pub connected: bool,
    pub server_name: String,
    pub instance_name: String,
    pub subscriber_count: usize,
    pub redis_enabled: bool,
}

impl Clone for WorkerBus {
    fn clone(&self) -> Self {
        // PERF-03: 状态字段全部 Arc 共享，零锁拷贝；
        // 不再有 blocking_read —— 异步上下文中 Clone 安全。
        // 注意：克隆体与原实例共享连接状态与订阅任务句柄。
        Self {
            config: self.config.clone(),
            server_name: self.server_name.clone(),
            instance_name: self.instance_name.clone(),
            subscribers: Arc::clone(&self.subscribers),
            command_tx: self.command_tx.clone(),
            command_rx: None,
            connected: Arc::clone(&self.connected),
            redis_client: Arc::clone(&self.redis_client),
            redis_pool: Arc::clone(&self.redis_pool),
            subscriber_task: Arc::clone(&self.subscriber_task),
            subscribed_channels: Arc::clone(&self.subscribed_channels),
            sub_command_tx: self.sub_command_tx.clone(),
            sub_command_rx: std::sync::Mutex::new(None),
            failed_publishes: Arc::clone(&self.failed_publishes),
        }
    }
}

pub fn parse_bus_message(data: &[u8]) -> Result<BusMessage, ApiError> {
    serde_json::from_slice(data).map_err(|e| ApiError::bad_request(format!("Invalid bus message: {e}")))
}

pub fn parse_replication_command(data: &[u8]) -> Result<ReplicationCommand, ApiError> {
    serde_json::from_slice(data).map_err(|e| ApiError::bad_request(format!("Invalid replication command: {e}")))
}

// ---------------------------------------------------------------------------
// EventBroadcaster trait implementation
// ---------------------------------------------------------------------------

impl synapse_common::traits::EventBroadcaster for WorkerBus {
    type Message = BusMessage;

    async fn broadcast_publish(&self, message: Self::Message) -> Result<(), synapse_common::traits::BroadcastError> {
        let encoded = serde_json::to_vec(&message)
            .map_err(|e| synapse_common::traits::BroadcastError::EncodingFailed(e.to_string()))?;

        self.publish(&message.channel, &encoded)
            .await
            .map_err(|e| synapse_common::traits::BroadcastError::Transport(e.to_string()))
    }

    fn broadcast_subscriber_count(&self) -> usize {
        self.subscribers.try_read().map(|s| s.len()).unwrap_or(0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // PERF-03: Clone 不得使用 blocking_read —— 在 tokio 异步上下文中会直接 panic
    #[tokio::test]
    async fn worker_bus_clone_inside_async_context_does_not_panic() {
        let bus = WorkerBus::new(RedisBusConfig::default(), "test.server".to_string(), "worker1".to_string());
        let cloned = bus.clone();
        assert!(!cloned.is_connected().await, "fresh bus must start disconnected");
    }

    #[tokio::test]
    async fn worker_bus_clone_shares_connection_state() {
        // 克隆体必须共享连接状态（Arc 语义），否则 clone 后状态快照过期
        let bus = WorkerBus::new(RedisBusConfig::default(), "test.server".to_string(), "worker1".to_string());
        bus.connect().await.expect("connect falls back to in-memory mode");
        let cloned = bus.clone();
        assert!(cloned.is_connected().await, "clone must observe connected state");
    }

    // WORK-01: unsubscribe 必须真正移除频道，不能是空操作
    #[tokio::test]
    async fn work01_unsubscribe_removes_channels() {
        let bus = WorkerBus::new(RedisBusConfig::default(), "test.server".to_string(), "worker1".to_string());
        bus.connect().await.expect("connect falls back to in-memory mode");
        let _rx = bus.subscribe(&["room:1", "room:2"]).await.expect("subscribe should succeed");
        assert_eq!(bus.subscribed_channels.read().await.len(), 2);

        bus.unsubscribe(&["room:1"]).await.expect("unsubscribe should succeed");

        let remaining = bus.subscribed_channels.read().await.clone();
        assert_eq!(remaining, vec!["synapse:room:2".to_string()]);
    }

    #[tokio::test]
    async fn work01_unsubscribe_unknown_channel_is_noop() {
        let bus = WorkerBus::new(RedisBusConfig::default(), "test.server".to_string(), "worker1".to_string());
        bus.connect().await.expect("connect falls back to in-memory mode");
        let _rx = bus.subscribe(&["room:1"]).await.expect("subscribe should succeed");

        bus.unsubscribe(&["room:unknown"]).await.expect("unsubscribe should succeed");

        assert_eq!(bus.subscribed_channels.read().await.len(), 1);
    }

    #[test]
    fn test_redis_bus_config_default() {
        let config = RedisBusConfig::default();
        assert_eq!(config.url, "redis://127.0.0.1:6379");
        assert_eq!(config.pool_size, 10);
        assert_eq!(config.channel_prefix, "synapse");
    }

    #[test]
    fn test_bus_message_serialization() {
        let msg = BusMessage {
            channel: "test".to_string(),
            sender: "worker1".to_string(),
            timestamp: 12345,
            payload: vec![1, 2, 3],
        };

        let encoded = serde_json::to_vec(&msg).unwrap();
        let decoded: BusMessage = serde_json::from_slice(&encoded).unwrap();

        assert_eq!(decoded.channel, "test");
        assert_eq!(decoded.sender, "worker1");
        assert_eq!(decoded.timestamp, 12345);
        assert_eq!(decoded.payload, vec![1, 2, 3]);
    }

    #[tokio::test]
    async fn test_worker_bus_creation() {
        let config = RedisBusConfig::default();
        let bus = WorkerBus::new(config, "test.com".to_string(), "worker1".to_string());

        assert!(!bus.is_connected().await);
    }

    #[tokio::test]
    async fn test_worker_bus_connect_fallback() {
        // Use an invalid Redis URL — should fall back to in-memory mode
        let config = RedisBusConfig {
            url: "redis://127.0.0.1:19999".to_string(), // non-existent port
            pool_size: 2,
            channel_prefix: "synapse".to_string(),
        };
        let bus = WorkerBus::new(config, "test.com".to_string(), "worker1".to_string());

        // connect() should succeed even if Redis is unavailable (in-memory fallback)
        bus.connect().await.unwrap();
        assert!(bus.is_connected().await);

        let stats = bus.get_stats().await;
        assert!(stats.connected);
        assert!(!stats.redis_enabled); // Redis should not be enabled

        bus.disconnect().await;
        assert!(!bus.is_connected().await);
    }

    #[tokio::test]
    async fn test_worker_bus_publish_without_connect() {
        let config = RedisBusConfig::default();
        let bus = WorkerBus::new(config, "test.com".to_string(), "worker1".to_string());

        let result = bus.publish("test", b"message").await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_worker_bus_subscribe_without_connect() {
        let config = RedisBusConfig::default();
        let bus = WorkerBus::new(config, "test.com".to_string(), "worker1".to_string());

        let result = bus.subscribe(&["test"]).await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_worker_bus_stats() {
        let config = RedisBusConfig::default();
        let bus = WorkerBus::new(config, "test.com".to_string(), "worker1".to_string());

        bus.connect().await.unwrap();
        let stats = bus.get_stats().await;

        assert!(stats.connected);
        assert_eq!(stats.server_name, "test.com");
        assert_eq!(stats.instance_name, "worker1");
    }

    #[test]
    fn test_parse_bus_message() {
        let msg = BusMessage {
            channel: "test".to_string(),
            sender: "worker1".to_string(),
            timestamp: 12345,
            payload: vec![1, 2, 3],
        };

        let encoded = serde_json::to_vec(&msg).unwrap();
        let decoded = parse_bus_message(&encoded).unwrap();

        assert_eq!(decoded.channel, "test");
    }

    #[test]
    fn test_parse_replication_command() {
        let cmd = ReplicationCommand::Ping { timestamp: 12345 };
        let encoded = serde_json::to_vec(&cmd).unwrap();
        let decoded = parse_replication_command(&encoded).unwrap();

        assert_eq!(decoded, cmd);
    }

    // WORK-05: DLQ starts empty
    #[tokio::test]
    async fn work05_dlq_starts_empty() {
        let bus = WorkerBus::new(RedisBusConfig::default(), "test.server".to_string(), "worker1".to_string());
        assert_eq!(bus.failed_publish_count().await, 0, "DLQ must start empty");
        assert!(bus.list_failed_publishes().await.is_empty(), "list_failed_publishes must return empty");
    }

    // WORK-05: Clone shares DLQ state (Arc semantics)
    #[tokio::test]
    async fn work05_dlq_shared_across_clone() {
        let bus = WorkerBus::new(RedisBusConfig::default(), "test.server".to_string(), "worker1".to_string());
        let cloned = bus.clone();

        // Manually insert a failed publish into the DLQ via the original
        {
            let mut dlq = bus.failed_publishes.write().await;
            dlq.push_back(FailedPublish {
                channel: "synapse:test".to_string(),
                payload: vec![1, 2, 3],
                error: "test error".to_string(),
                failed_at: 12345,
            });
        }

        // The clone should see the same entry
        assert_eq!(cloned.failed_publish_count().await, 1, "clone must share DLQ state via Arc");
    }

    // WORK-05: DLQ ring buffer evicts oldest entries when full
    #[tokio::test]
    async fn work05_dlq_ring_buffer_eviction() {
        let bus = WorkerBus::new(RedisBusConfig::default(), "test.server".to_string(), "worker1".to_string());

        // Fill beyond capacity
        {
            let mut dlq = bus.failed_publishes.write().await;
            for i in 0..(FAILED_PUBLISH_RING_SIZE + 10) {
                if dlq.len() >= FAILED_PUBLISH_RING_SIZE {
                    dlq.pop_front();
                }
                dlq.push_back(FailedPublish {
                    channel: format!("synapse:ch-{i}"),
                    payload: vec![i as u8],
                    error: "error".to_string(),
                    failed_at: i as i64,
                });
            }
        }

        let count = bus.failed_publish_count().await;
        assert_eq!(count, FAILED_PUBLISH_RING_SIZE, "DLQ must not exceed ring size");

        let entries = bus.list_failed_publishes().await;
        // Oldest entries should have been evicted; the first entry should be ch-10
        assert_eq!(entries[0].channel, "synapse:ch-10", "oldest entries must be evicted first");
        // Latest entry should be the last one inserted
        assert_eq!(
            entries.last().unwrap().channel,
            format!("synapse:ch-{}", FAILED_PUBLISH_RING_SIZE + 9),
            "latest entry must be the most recent"
        );
    }
}
