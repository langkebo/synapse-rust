use crate::error::ApiError;
use crate::worker::bus::WorkerBus;
use crate::worker::protocol::{ReplicationCommand, ReplicationRow};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::RwLock;
use tracing::{debug, warn};

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct StreamWriters {
    pub events: Option<String>,
    pub typing: Option<String>,
    pub to_device: Option<String>,
    pub account_data: Option<String>,
    pub receipts: Option<String>,
    pub presence: Option<String>,
    pub device_lists: Option<String>,
    pub federation: Option<String>,
    pub pushers: Option<String>,
    pub caches: Option<String>,
}

impl StreamWriters {
    pub fn get_writer(&self, stream_name: &str) -> Option<&str> {
        match stream_name {
            "events" => self.events.as_deref(),
            "typing" => self.typing.as_deref(),
            "to_device" => self.to_device.as_deref(),
            "account_data" => self.account_data.as_deref(),
            "receipts" => self.receipts.as_deref(),
            "presence" => self.presence.as_deref(),
            "device_lists" => self.device_lists.as_deref(),
            "federation" => self.federation.as_deref(),
            "pushers" => self.pushers.as_deref(),
            "caches" => self.caches.as_deref(),
            _ => None,
        }
    }

    pub fn all_writers(&self) -> Vec<&str> {
        let mut writers = Vec::new();
        if let Some(w) = &self.events {
            writers.push(w.as_str());
        }
        if let Some(w) = &self.typing {
            writers.push(w.as_str());
        }
        if let Some(w) = &self.to_device {
            writers.push(w.as_str());
        }
        if let Some(w) = &self.account_data {
            writers.push(w.as_str());
        }
        if let Some(w) = &self.receipts {
            writers.push(w.as_str());
        }
        if let Some(w) = &self.presence {
            writers.push(w.as_str());
        }
        if let Some(w) = &self.device_lists {
            writers.push(w.as_str());
        }
        if let Some(w) = &self.federation {
            writers.push(w.as_str());
        }
        if let Some(w) = &self.pushers {
            writers.push(w.as_str());
        }
        if let Some(w) = &self.caches {
            writers.push(w.as_str());
        }
        writers
    }
}

#[derive(Debug, Clone)]
pub struct StreamPosition {
    pub stream_name: String,
    pub position: i64,
    pub instance_name: String,
    pub updated_ts: i64,
}

pub struct StreamWriterManager {
    config: StreamWriters,
    bus: Arc<WorkerBus>,
    instance_name: String,
    positions: RwLock<HashMap<String, StreamPosition>>,
}

impl StreamWriterManager {
    pub fn new(config: StreamWriters, bus: Arc<WorkerBus>, instance_name: String) -> Self {
        Self {
            config,
            bus,
            instance_name,
            positions: RwLock::new(HashMap::new()),
        }
    }

    pub fn get_writer(&self, stream_name: &str) -> Option<&str> {
        self.config.get_writer(stream_name)
    }

    pub fn is_local_writer(&self, stream_name: &str) -> bool {
        match self.config.get_writer(stream_name) {
            Some(writer) => writer == self.instance_name,
            None => true,
        }
    }

    pub async fn forward_to_writer(
        &self,
        stream_name: &str,
        token: &str,
        rows: Vec<ReplicationRow>,
    ) -> Result<(), ApiError> {
        let writer = self.config.get_writer(stream_name);

        match writer {
            Some(writer_name) if writer_name != self.instance_name => {
                debug!(
                    "Forwarding {} stream data to writer: {}",
                    stream_name, writer_name
                );

                let command = ReplicationCommand::Rdata {
                    stream_name: stream_name.to_string(),
                    token: token.to_string(),
                    rows,
                };

                self.bus.send_to_worker(writer_name, &command).await?;
                Ok(())
            }
            _ => {
                debug!("Processing {} stream data locally", stream_name);
                Ok(())
            }
        }
    }

    pub async fn update_position(&self, stream_name: &str, position: i64) -> Result<(), ApiError> {
        let now = chrono::Utc::now().timestamp_millis();

        let stream_position = StreamPosition {
            stream_name: stream_name.to_string(),
            position,
            instance_name: self.instance_name.clone(),
            updated_ts: now,
        };

        {
            let mut positions = self.positions.write().await;
            positions.insert(stream_name.to_string(), stream_position);
        }

        self.bus
            .publish_stream_position(stream_name, position)
            .await?;

        debug!("Updated position for {}: {}", stream_name, position);
        Ok(())
    }

    pub async fn get_position(&self, stream_name: &str) -> Option<i64> {
        let positions = self.positions.read().await;
        positions.get(stream_name).map(|p| p.position)
    }

    pub async fn get_all_positions(&self) -> HashMap<String, i64> {
        let positions = self.positions.read().await;
        positions
            .iter()
            .map(|(k, v)| (k.clone(), v.position))
            .collect()
    }

    pub async fn sync_positions(&self) -> Result<(), ApiError> {
        for stream_name in Self::stream_names() {
            if self.is_local_writer(stream_name) {
                let position = self.get_position(stream_name).await.unwrap_or(0);
                self.bus
                    .publish_stream_position(stream_name, position)
                    .await?;
            }
        }

        debug!("Synced all stream positions");
        Ok(())
    }

    pub fn stream_names() -> &'static [&'static str] {
        &[
            "events",
            "typing",
            "to_device",
            "account_data",
            "receipts",
            "presence",
            "device_lists",
            "federation",
            "pushers",
            "caches",
        ]
    }

    pub fn get_local_streams(&self) -> Vec<&'static str> {
        Self::stream_names()
            .iter()
            .filter(|s| self.is_local_writer(s))
            .copied()
            .collect()
    }

    pub async fn can_write(&self, stream_name: &str) -> bool {
        self.is_local_writer(stream_name)
    }

    pub async fn validate_writer(
        &self,
        stream_name: &str,
        writer_instance: &str,
    ) -> Result<(), ApiError> {
        match self.config.get_writer(stream_name) {
            Some(configured_writer) => {
                if configured_writer != writer_instance {
                    warn!(
                        "Writer mismatch for {}: expected {}, got {}",
                        stream_name, configured_writer, writer_instance
                    );
                    return Err(ApiError::forbidden(format!(
                        "Instance {} is not configured to write to stream {}",
                        writer_instance, stream_name
                    )));
                }
                Ok(())
            }
            None => {
                if writer_instance != self.instance_name {
                    warn!(
                        "Unconfigured stream {} written by non-master instance {}",
                        stream_name, writer_instance
                    );
                }
                Ok(())
            }
        }
    }

    pub async fn get_stats(&self) -> StreamWriterStats {
        let positions = self.positions.read().await;

        StreamWriterStats {
            instance_name: self.instance_name.clone(),
            local_streams: self
                .get_local_streams()
                .iter()
                .map(|s| s.to_string())
                .collect(),
            positions: positions
                .iter()
                .map(|(k, v)| (k.clone(), v.position))
                .collect(),
        }
    }

    pub async fn get_all_stream_positions(&self) -> Vec<StreamPosition> {
        let positions = self.positions.read().await;
        positions.values().cloned().collect()
    }

    pub async fn update_positions_bulk(&self, updates: HashMap<String, i64>) -> Result<(), ApiError> {
        let now = chrono::Utc::now().timestamp_millis();
        
        {
            let mut positions = self.positions.write().await;
            for (stream_name, position) in updates {
                let stream_position = StreamPosition {
                    stream_name: stream_name.clone(),
                    position,
                    instance_name: self.instance_name.clone(),
                    updated_ts: now,
                };
                positions.insert(stream_name, stream_position);
            }
        }

        Ok(())
    }

    pub async fn get_stream_config(&self) -> StreamWriters {
        self.config.clone()
    }

    pub async fn update_stream_config(&mut self, new_config: StreamWriters) {
        self.config = new_config;
    }

    pub async fn reset_position(&self, stream_name: &str) -> Result<(), ApiError> {
        self.update_position(stream_name, 0).await
    }

    pub async fn advance_position_if_greater(&self, stream_name: &str, new_position: i64) -> Result<bool, ApiError> {
        let current = self.get_position(stream_name).await.unwrap_or(0);
        
        if new_position > current {
            self.update_position(stream_name, new_position).await?;
            Ok(true)
        } else {
            Ok(false)
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StreamWriterStats {
    pub instance_name: String,
    pub local_streams: Vec<String>,
    pub positions: HashMap<String, i64>,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::worker::bus::RedisConfig;

    fn create_test_bus() -> Arc<WorkerBus> {
        let config = RedisConfig::default();
        Arc::new(WorkerBus::new(
            config,
            "test.com".to_string(),
            "worker1".to_string(),
        ))
    }

    #[test]
    fn test_stream_writers_default() {
        let writers = StreamWriters::default();
        assert!(writers.events.is_none());
        assert!(writers.typing.is_none());
    }

    #[test]
    fn test_stream_writers_get_writer() {
        let mut writers = StreamWriters::default();
        writers.events = Some("worker1".to_string());

        assert_eq!(writers.get_writer("events"), Some("worker1"));
        assert_eq!(writers.get_writer("typing"), None);
    }

    #[test]
    fn test_stream_writers_all_writers() {
        let mut writers = StreamWriters::default();
        writers.events = Some("worker1".to_string());
        writers.typing = Some("worker2".to_string());

        let all = writers.all_writers();
        assert_eq!(all.len(), 2);
    }

    #[tokio::test]
    async fn test_stream_writer_manager_creation() {
        let bus = create_test_bus();
        let config = StreamWriters::default();
        let manager = StreamWriterManager::new(config, bus, "worker1".to_string());

        assert!(manager.is_local_writer("events"));
    }

    #[tokio::test]
    async fn test_stream_writer_manager_with_config() {
        let bus = create_test_bus();
        let mut config = StreamWriters::default();
        config.events = Some("worker2".to_string());

        let manager = StreamWriterManager::new(config, bus, "worker1".to_string());

        assert!(!manager.is_local_writer("events"));
        assert!(manager.is_local_writer("typing"));
    }

    #[tokio::test]
    async fn test_stream_writer_manager_position() {
        let bus = create_test_bus();
        let config = StreamWriters::default();
        let manager = StreamWriterManager::new(config, bus, "worker1".to_string());

        assert!(manager.get_position("events").await.is_none());

        let result = manager.update_position("events", 100).await;
        if result.is_err() {
            return;
        }
        assert_eq!(manager.get_position("events").await, Some(100));
    }

    #[tokio::test]
    async fn test_stream_writer_manager_local_streams() {
        let bus = create_test_bus();
        let mut config = StreamWriters::default();
        config.events = Some("worker2".to_string());

        let manager = StreamWriterManager::new(config, bus, "worker1".to_string());

        let local = manager.get_local_streams();
        assert!(!local.contains(&"events"));
        assert!(local.contains(&"typing"));
    }

    #[tokio::test]
    async fn test_stream_writer_manager_stats() {
        let bus = create_test_bus();
        let config = StreamWriters::default();
        let manager = StreamWriterManager::new(config, bus, "worker1".to_string());

        let result = manager.update_position("events", 100).await;
        if result.is_err() {
            return;
        }

        let stats = manager.get_stats().await;
        assert_eq!(stats.instance_name, "worker1");
        assert_eq!(stats.positions.get("events"), Some(&100));
    }

    #[test]
    fn test_stream_names() {
        let names = StreamWriterManager::stream_names();
        assert!(names.contains(&"events"));
        assert!(names.contains(&"typing"));
        assert!(names.contains(&"to_device"));
    }
}
