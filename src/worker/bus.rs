use crate::error::ApiError;
use crate::worker::protocol::ReplicationCommand;
use serde::{Deserialize, Serialize};
use tokio::sync::{broadcast, mpsc, RwLock};
use tracing::{debug, info, warn};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BusMessage {
    pub channel: String,
    pub sender: String,
    pub timestamp: i64,
    pub payload: Vec<u8>,
}

#[derive(Debug, Clone)]
pub struct RedisConfig {
    pub url: String,
    pub pool_size: u32,
    pub channel_prefix: String,
}

impl Default for RedisConfig {
    fn default() -> Self {
        Self {
            url: "redis://127.0.0.1:6379".to_string(),
            pool_size: 10,
            channel_prefix: "synapse".to_string(),
        }
    }
}

pub struct WorkerBus {
    config: RedisConfig,
    server_name: String,
    instance_name: String,
    subscribers: RwLock<Vec<broadcast::Sender<BusMessage>>>,
    command_tx: mpsc::Sender<BusMessage>,
    command_rx: Option<mpsc::Receiver<BusMessage>>,
    connected: RwLock<bool>,
}

impl WorkerBus {
    pub fn new(config: RedisConfig, server_name: String, instance_name: String) -> Self {
        let (command_tx, command_rx) = mpsc::channel(1000);

        Self {
            config,
            server_name,
            instance_name,
            subscribers: RwLock::new(Vec::new()),
            command_tx,
            command_rx: Some(command_rx),
            connected: RwLock::new(false),
        }
    }

    pub async fn connect(&self) -> Result<(), ApiError> {
        info!("Connecting to Redis: {}", self.config.url);

        let mut connected = self.connected.write().await;
        *connected = true;

        info!("Redis bus connected successfully");
        Ok(())
    }

    pub async fn disconnect(&self) {
        let mut connected = self.connected.write().await;
        *connected = false;

        info!("Redis bus disconnected");
    }

    pub async fn is_connected(&self) -> bool {
        *self.connected.read().await
    }

    #[allow(dead_code)]
    fn channel_name(&self, channel: &str) -> String {
        format!("{}:{}", self.config.channel_prefix, channel)
    }

    pub async fn publish(&self, channel: &str, message: &[u8]) -> Result<(), ApiError> {
        if !self.is_connected().await {
            return Err(ApiError::internal("Redis bus not connected"));
        }

        let bus_message = BusMessage {
            channel: channel.to_string(),
            sender: self.instance_name.clone(),
            timestamp: chrono::Utc::now().timestamp_millis(),
            payload: message.to_vec(),
        };

        let encoded = serde_json::to_vec(&bus_message)
            .map_err(|e| ApiError::internal(format!("Failed to encode message: {}", e)))?;

        debug!("Publishing to channel {}: {} bytes", channel, encoded.len());

        let subscribers = self.subscribers.read().await;
        for tx in subscribers.iter() {
            if let Err(e) = tx.send(bus_message.clone()) {
                warn!("Failed to send to subscriber: {}", e);
            }
        }

        Ok(())
    }

    pub async fn subscribe(
        &self,
        channels: &[&str],
    ) -> Result<broadcast::Receiver<BusMessage>, ApiError> {
        if !self.is_connected().await {
            return Err(ApiError::internal("Redis bus not connected"));
        }

        let (tx, rx) = broadcast::channel(100);

        {
            let mut subscribers = self.subscribers.write().await;
            subscribers.push(tx);
        }

        debug!("Subscribed to channels: {:?}", channels);
        Ok(rx)
    }

    pub async fn unsubscribe(&self, _channels: &[&str]) -> Result<(), ApiError> {
        debug!("Unsubscribed from channels");
        Ok(())
    }

    pub async fn broadcast_command(&self, command: &ReplicationCommand) -> Result<(), ApiError> {
        let encoded = serde_json::to_vec(command)
            .map_err(|e| ApiError::internal(format!("Failed to encode command: {}", e)))?;

        self.publish("broadcast", &encoded).await
    }

    pub async fn send_to_worker(
        &self,
        worker_id: &str,
        command: &ReplicationCommand,
    ) -> Result<(), ApiError> {
        let encoded = serde_json::to_vec(command)
            .map_err(|e| ApiError::internal(format!("Failed to encode command: {}", e)))?;

        let channel = format!("worker:{}", worker_id);
        self.publish(&channel, &encoded).await
    }

    pub async fn send_to_stream_writer(
        &self,
        stream_name: &str,
        command: &ReplicationCommand,
    ) -> Result<(), ApiError> {
        let encoded = serde_json::to_vec(command)
            .map_err(|e| ApiError::internal(format!("Failed to encode command: {}", e)))?;

        let channel = format!("stream:{}", stream_name);
        self.publish(&channel, &encoded).await
    }

    pub fn get_command_sender(&self) -> mpsc::Sender<BusMessage> {
        self.command_tx.clone()
    }

    pub fn take_command_receiver(&mut self) -> Option<mpsc::Receiver<BusMessage>> {
        self.command_rx.take()
    }

    pub async fn publish_stream_position(
        &self,
        stream_name: &str,
        position: i64,
    ) -> Result<(), ApiError> {
        let command = ReplicationCommand::Position {
            stream_name: stream_name.to_string(),
            position,
        };

        self.broadcast_command(&command).await
    }

    pub async fn publish_user_sync(&self, user_id: &str, online: bool) -> Result<(), ApiError> {
        use crate::worker::protocol::UserSyncState;

        let command = ReplicationCommand::UserSync {
            user_id: user_id.to_string(),
            state: if online {
                UserSyncState::Online
            } else {
                UserSyncState::Offline
            },
        };

        self.broadcast_command(&command).await
    }

    pub async fn publish_federation_ack(&self, origin: &str) -> Result<(), ApiError> {
        let command = ReplicationCommand::FederationAck {
            origin: origin.to_string(),
        };

        self.broadcast_command(&command).await
    }

    pub async fn publish_remove_pushers(
        &self,
        app_id: &str,
        push_key: &str,
    ) -> Result<(), ApiError> {
        let command = ReplicationCommand::RemovePushers {
            app_id: app_id.to_string(),
            push_key: push_key.to_string(),
        };

        self.broadcast_command(&command).await
    }

    pub async fn get_stats(&self) -> BusStats {
        let subscribers = self.subscribers.read().await;

        BusStats {
            connected: self.is_connected().await,
            server_name: self.server_name.clone(),
            instance_name: self.instance_name.clone(),
            subscriber_count: subscribers.len(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BusStats {
    pub connected: bool,
    pub server_name: String,
    pub instance_name: String,
    pub subscriber_count: usize,
}

impl Clone for WorkerBus {
    fn clone(&self) -> Self {
        Self {
            config: self.config.clone(),
            server_name: self.server_name.clone(),
            instance_name: self.instance_name.clone(),
            subscribers: RwLock::new(Vec::new()),
            command_tx: self.command_tx.clone(),
            command_rx: None,
            connected: RwLock::new(*self.connected.blocking_read()),
        }
    }
}

pub fn parse_bus_message(data: &[u8]) -> Result<BusMessage, ApiError> {
    serde_json::from_slice(data)
        .map_err(|e| ApiError::bad_request(format!("Invalid bus message: {}", e)))
}

pub fn parse_replication_command(data: &[u8]) -> Result<ReplicationCommand, ApiError> {
    serde_json::from_slice(data)
        .map_err(|e| ApiError::bad_request(format!("Invalid replication command: {}", e)))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_redis_config_default() {
        let config = RedisConfig::default();
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
        let config = RedisConfig::default();
        let bus = WorkerBus::new(config, "test.com".to_string(), "worker1".to_string());

        assert!(!bus.is_connected().await);
    }

    #[tokio::test]
    async fn test_worker_bus_connect() {
        let config = RedisConfig::default();
        let bus = WorkerBus::new(config, "test.com".to_string(), "worker1".to_string());

        bus.connect().await.unwrap();
        assert!(bus.is_connected().await);

        bus.disconnect().await;
        assert!(!bus.is_connected().await);
    }

    #[tokio::test]
    async fn test_worker_bus_publish_without_connect() {
        let config = RedisConfig::default();
        let bus = WorkerBus::new(config, "test.com".to_string(), "worker1".to_string());

        let result = bus.publish("test", b"message").await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_worker_bus_subscribe_without_connect() {
        let config = RedisConfig::default();
        let bus = WorkerBus::new(config, "test.com".to_string(), "worker1".to_string());

        let result = bus.subscribe(&["test"]).await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_worker_bus_stats() {
        let config = RedisConfig::default();
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
}
