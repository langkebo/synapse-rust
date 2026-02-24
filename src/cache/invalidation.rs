use crate::common::ApiError;
use deadpool_redis::Pool;
use futures::StreamExt;
use redis::{AsyncCommands, Client};
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::broadcast;
use tracing::{debug, error, info, warn};

pub const CACHE_INVALIDATION_CHANNEL: &str = "synapse:cache:invalidation";
pub const DEFAULT_LOCAL_CACHE_TTL_SECS: u64 = 30;
pub const DEFAULT_REDIS_CACHE_TTL_SECS: u64 = 3600;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Copy)]
pub enum InvalidationType {
    Key,
    Pattern,
    All,
    Prefix,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CacheInvalidationMessage {
    pub key: String,
    pub invalidation_type: InvalidationType,
    pub sender_instance: String,
    pub timestamp: i64,
    pub reason: Option<String>,
}

impl CacheInvalidationMessage {
    pub fn new(key: String, invalidation_type: InvalidationType, sender_instance: String) -> Self {
        Self {
            key,
            invalidation_type,
            sender_instance,
            timestamp: chrono::Utc::now().timestamp_millis(),
            reason: None,
        }
    }

    pub fn with_reason(mut self, reason: String) -> Self {
        self.reason = Some(reason);
        self
    }

    pub fn encode(&self) -> Result<Vec<u8>, ApiError> {
        serde_json::to_vec(self)
            .map_err(|e| ApiError::internal(format!("Failed to encode invalidation message: {}", e)))
    }

    pub fn decode(data: &[u8]) -> Result<Self, ApiError> {
        serde_json::from_slice(data)
            .map_err(|e| ApiError::internal(format!("Failed to decode invalidation message: {}", e)))
    }
}

#[derive(Debug, Clone)]
pub struct CacheInvalidationConfig {
    pub enabled: bool,
    pub channel_name: String,
    pub local_cache_ttl_secs: u64,
    pub redis_cache_ttl_secs: u64,
    pub instance_id: String,
    pub redis_url: String,
}

impl Default for CacheInvalidationConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            channel_name: CACHE_INVALIDATION_CHANNEL.to_string(),
            local_cache_ttl_secs: DEFAULT_LOCAL_CACHE_TTL_SECS,
            redis_cache_ttl_secs: DEFAULT_REDIS_CACHE_TTL_SECS,
            instance_id: format!("instance-{}", uuid::Uuid::new_v4()),
            redis_url: "redis://127.0.0.1:6379".to_string(),
        }
    }
}

pub struct CacheInvalidationBroadcaster {
    pool: Pool,
    config: CacheInvalidationConfig,
}

impl std::fmt::Debug for CacheInvalidationBroadcaster {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("CacheInvalidationBroadcaster")
            .field("config", &self.config)
            .finish()
    }
}

impl CacheInvalidationBroadcaster {
    pub fn new(pool: Pool, config: CacheInvalidationConfig) -> Self {
        Self { pool, config }
    }

    pub async fn broadcast_invalidation(
        &self,
        key: &str,
        invalidation_type: InvalidationType,
    ) -> Result<(), ApiError> {
        if !self.config.enabled {
            return Ok(());
        }

        let message = CacheInvalidationMessage::new(
            key.to_string(),
            invalidation_type,
            self.config.instance_id.clone(),
        );

        let encoded = message.encode()?;
        let channel = &self.config.channel_name;

        let mut conn = self.pool.get().await.map_err(|e| {
            ApiError::internal(format!("Failed to get Redis connection: {}", e))
        })?;

        let _: () = conn.publish(channel, encoded).await.map_err(|e| {
            ApiError::internal(format!("Failed to publish invalidation message: {}", e))
        })?;

        debug!(
            "Broadcasted cache invalidation: key={}, type={:?}",
            key, invalidation_type
        );

        Ok(())
    }

    pub async fn invalidate_key(&self, key: &str) -> Result<(), ApiError> {
        self.broadcast_invalidation(key, InvalidationType::Key).await
    }

    pub async fn invalidate_pattern(&self, pattern: &str) -> Result<(), ApiError> {
        self.broadcast_invalidation(pattern, InvalidationType::Pattern).await
    }

    pub async fn invalidate_prefix(&self, prefix: &str) -> Result<(), ApiError> {
        self.broadcast_invalidation(prefix, InvalidationType::Prefix).await
    }

    pub async fn invalidate_all(&self) -> Result<(), ApiError> {
        self.broadcast_invalidation("*", InvalidationType::All).await
    }
}

pub type InvalidationReceiver = broadcast::Receiver<CacheInvalidationMessage>;

pub struct CacheInvalidationSubscriber {
    client: Client,
    config: CacheInvalidationConfig,
    sender: broadcast::Sender<CacheInvalidationMessage>,
    running: Arc<parking_lot::RwLock<bool>>,
}

impl std::fmt::Debug for CacheInvalidationSubscriber {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("CacheInvalidationSubscriber")
            .field("config", &self.config)
            .field("running", &*self.running.read())
            .finish()
    }
}

impl CacheInvalidationSubscriber {
    pub fn new(redis_url: &str, config: CacheInvalidationConfig) -> Result<Self, ApiError> {
        let client = Client::open(redis_url)
            .map_err(|e| ApiError::internal(format!("Failed to create Redis client: {}", e)))?;
        let (sender, _) = broadcast::channel(1024);
        Ok(Self {
            client,
            config,
            sender,
            running: Arc::new(parking_lot::RwLock::new(false)),
        })
    }

    pub fn subscribe(&self) -> InvalidationReceiver {
        self.sender.subscribe()
    }

    pub async fn start(&self) -> Result<(), ApiError> {
        if *self.running.read() {
            return Ok(());
        }

        *self.running.write() = true;
        let running = self.running.clone();
        let client = self.client.clone();
        let channel = self.config.channel_name.clone();
        let instance_id = self.config.instance_id.clone();
        let sender = self.sender.clone();

        info!(
            "Starting cache invalidation subscriber on channel: {} (instance: {})",
            channel, instance_id
        );

        tokio::spawn(async move {
            loop {
                if !*running.read() {
                    info!("Cache invalidation subscriber stopped");
                    break;
                }

                match Self::subscribe_and_listen(&client, &channel, &sender, &instance_id).await {
                    Ok(_) => {
                        debug!("Subscription ended normally, reconnecting...");
                    }
                    Err(e) => {
                        error!("Subscription error: {}, reconnecting in 1s...", e);
                        tokio::time::sleep(Duration::from_secs(1)).await;
                    }
                }
            }
        });

        Ok(())
    }

    async fn subscribe_and_listen(
        client: &Client,
        channel: &str,
        sender: &broadcast::Sender<CacheInvalidationMessage>,
        instance_id: &str,
    ) -> Result<(), ApiError> {
        let mut pubsub = client
            .get_async_pubsub()
            .await
            .map_err(|e| ApiError::internal(format!("Failed to get async pubsub: {}", e)))?;

        info!("Subscribed to cache invalidation channel: {}", channel);

        pubsub
            .subscribe(channel)
            .await
            .map_err(|e| ApiError::internal(format!("Failed to subscribe to channel: {}", e)))?;

        let mut message_stream = pubsub.on_message();
        let instance_id_owned = instance_id.to_string();
        let sender_clone = sender.clone();

        loop {
            match message_stream.next().await {
                Some(msg) => {
                    let payload: Vec<u8> = msg
                        .get_payload()
                        .map_err(|e| {
                            ApiError::internal(format!("Failed to get message payload: {}", e))
                        })?;

                    match CacheInvalidationMessage::decode(&payload) {
                        Ok(invalidation_msg) => {
                            if invalidation_msg.sender_instance != instance_id_owned {
                                debug!(
                                    "Received cache invalidation: key={}, type={:?}, from={}",
                                    invalidation_msg.key,
                                    invalidation_msg.invalidation_type,
                                    invalidation_msg.sender_instance
                                );

                                if sender_clone.send(invalidation_msg).is_err() {
                                    warn!("No active receivers for invalidation message");
                                }
                            } else {
                                debug!(
                                    "Ignoring self-sent invalidation message: key={}",
                                    invalidation_msg.key
                                );
                            }
                        }
                        Err(e) => {
                            error!("Failed to decode invalidation message: {}", e);
                        }
                    }
                }
                None => {
                    debug!("Pub/Sub stream ended");
                    break;
                }
            }
        }

        Ok(())
    }

    pub fn stop(&self) {
        *self.running.write() = false;
        info!("Cache invalidation subscriber stop requested");
    }

    pub fn is_running(&self) -> bool {
        *self.running.read()
    }
}

impl Clone for CacheInvalidationSubscriber {
    fn clone(&self) -> Self {
        Self {
            client: self.client.clone(),
            config: self.config.clone(),
            sender: self.sender.clone(),
            running: self.running.clone(),
        }
    }
}

pub struct CacheInvalidationManager {
    broadcaster: Option<Arc<CacheInvalidationBroadcaster>>,
    subscriber: Option<Arc<CacheInvalidationSubscriber>>,
    config: CacheInvalidationConfig,
}

impl std::fmt::Debug for CacheInvalidationManager {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("CacheInvalidationManager")
            .field("config", &self.config)
            .field("has_broadcaster", &self.broadcaster.is_some())
            .field("has_subscriber", &self.subscriber.is_some())
            .finish()
    }
}

impl CacheInvalidationManager {
    pub fn new(pool: Option<Pool>, config: CacheInvalidationConfig) -> Self {
        let (broadcaster, subscriber) = if let Some(p) = pool {
            let subscriber = CacheInvalidationSubscriber::new(&config.redis_url, config.clone())
                .map(Arc::new)
                .ok();
            (
                Some(Arc::new(CacheInvalidationBroadcaster::new(p, config.clone()))),
                subscriber,
            )
        } else {
            (None, None)
        };

        Self {
            broadcaster,
            subscriber,
            config,
        }
    }

    pub fn broadcaster(&self) -> Option<&Arc<CacheInvalidationBroadcaster>> {
        self.broadcaster.as_ref()
    }

    pub fn subscriber(&self) -> Option<&Arc<CacheInvalidationSubscriber>> {
        self.subscriber.as_ref()
    }

    pub fn config(&self) -> &CacheInvalidationConfig {
        &self.config
    }

    pub async fn start_subscriber(&self) -> Result<(), ApiError> {
        if let Some(subscriber) = &self.subscriber {
            subscriber.start().await?;
        }
        Ok(())
    }

    pub async fn invalidate_key(&self, key: &str) -> Result<(), ApiError> {
        if let Some(broadcaster) = &self.broadcaster {
            broadcaster.invalidate_key(key).await?;
        }
        Ok(())
    }

    pub async fn invalidate_pattern(&self, pattern: &str) -> Result<(), ApiError> {
        if let Some(broadcaster) = &self.broadcaster {
            broadcaster.invalidate_pattern(pattern).await?;
        }
        Ok(())
    }

    pub async fn invalidate_prefix(&self, prefix: &str) -> Result<(), ApiError> {
        if let Some(broadcaster) = &self.broadcaster {
            broadcaster.invalidate_prefix(prefix).await?;
        }
        Ok(())
    }

    pub async fn invalidate_all(&self) -> Result<(), ApiError> {
        if let Some(broadcaster) = &self.broadcaster {
            broadcaster.invalidate_all().await?;
        }
        Ok(())
    }

    pub fn subscribe(&self) -> Option<InvalidationReceiver> {
        self.subscriber.as_ref().map(|s| s.subscribe())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_cache_invalidation_message_creation() {
        let msg = CacheInvalidationMessage::new(
            "test_key".to_string(),
            InvalidationType::Key,
            "instance-1".to_string(),
        );

        assert_eq!(msg.key, "test_key");
        assert_eq!(msg.invalidation_type, InvalidationType::Key);
        assert_eq!(msg.sender_instance, "instance-1");
        assert!(msg.reason.is_none());
    }

    #[test]
    fn test_cache_invalidation_message_with_reason() {
        let msg = CacheInvalidationMessage::new(
            "test_key".to_string(),
            InvalidationType::Key,
            "instance-1".to_string(),
        )
        .with_reason("User logged out".to_string());

        assert_eq!(msg.reason, Some("User logged out".to_string()));
    }

    #[test]
    fn test_cache_invalidation_message_encode_decode() {
        let msg = CacheInvalidationMessage::new(
            "test_key".to_string(),
            InvalidationType::Pattern,
            "instance-1".to_string(),
        )
        .with_reason("Test reason".to_string());

        let encoded = msg.encode().unwrap();
        let decoded = CacheInvalidationMessage::decode(&encoded).unwrap();

        assert_eq!(decoded.key, msg.key);
        assert_eq!(decoded.invalidation_type, msg.invalidation_type);
        assert_eq!(decoded.sender_instance, msg.sender_instance);
        assert_eq!(decoded.reason, msg.reason);
    }

    #[test]
    fn test_cache_invalidation_config_default() {
        let config = CacheInvalidationConfig::default();

        assert!(config.enabled);
        assert_eq!(config.channel_name, CACHE_INVALIDATION_CHANNEL);
        assert_eq!(config.local_cache_ttl_secs, DEFAULT_LOCAL_CACHE_TTL_SECS);
        assert_eq!(config.redis_cache_ttl_secs, DEFAULT_REDIS_CACHE_TTL_SECS);
        assert!(config.instance_id.starts_with("instance-"));
    }

    #[test]
    fn test_invalidation_type_equality() {
        assert_eq!(InvalidationType::Key, InvalidationType::Key);
        assert_ne!(InvalidationType::Key, InvalidationType::Pattern);
        assert_ne!(InvalidationType::Pattern, InvalidationType::All);
    }
}
