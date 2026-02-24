use serde::{Deserialize, Serialize};
use std::collections::HashMap;

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum MessageQueueBackend {
    InMemory,
    Redis,
    Kafka,
    RabbitMQ,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct QueueConfig {
    pub backend: MessageQueueBackend,
    pub redis_url: Option<String>,
    pub kafka_brokers: Option<Vec<String>>,
    pub rabbitmq_url: Option<String>,
    pub default_timeout_ms: u64,
    pub max_retries: u32,
    pub retry_delay_ms: u64,
}

impl Default for QueueConfig {
    fn default() -> Self {
        Self {
            backend: MessageQueueBackend::InMemory,
            redis_url: None,
            kafka_brokers: None,
            rabbitmq_url: None,
            default_timeout_ms: 5000,
            max_retries: 3,
            retry_delay_ms: 1000,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct QueueMessage {
    pub id: String,
    pub queue: String,
    pub payload: Vec<u8>,
    pub priority: i32,
    pub created_at: i64,
    pub expires_at: Option<i64>,
    pub retry_count: u32,
    pub headers: HashMap<String, String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct QueueStats {
    pub queue_name: String,
    pub message_count: u64,
    pub consumer_count: u32,
    pub avg_wait_time_ms: f64,
    pub processing_rate: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConsumerGroup {
    pub group_id: String,
    pub queue: String,
    pub members: Vec<String>,
    pub assigned_partitions: Vec<i32>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AckRequest {
    pub message_id: String,
    pub group_id: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PublishRequest {
    pub queue: String,
    pub payload: Vec<u8>,
    pub priority: Option<i32>,
    pub delay_ms: Option<u64>,
    pub expires_at: Option<i64>,
    pub headers: Option<HashMap<String, String>>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConsumeRequest {
    pub queue: String,
    pub group_id: String,
    pub consumer_id: String,
    pub max_messages: u32,
    pub timeout_ms: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DeadLetterConfig {
    pub enabled: bool,
    pub queue_name: String,
    pub max_retries: u32,
    pub retention_ms: u64,
}

impl Default for DeadLetterConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            queue_name: "dead_letter".to_string(),
            max_retries: 3,
            retention_ms: 86400000,
        }
    }
}
