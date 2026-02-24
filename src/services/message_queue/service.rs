use super::models::*;
use std::collections::{HashMap, VecDeque};
use std::sync::Arc;
use tokio::sync::RwLock;
use tokio::time::Duration;

pub struct MessageQueue {
    config: QueueConfig,
    queues: Arc<RwLock<HashMap<String, VecDeque<QueueMessage>>>>,
    pending: Arc<RwLock<HashMap<String, QueueMessage>>>,
    stats: Arc<RwLock<HashMap<String, QueueStats>>>,
}

impl MessageQueue {
    pub fn new(config: QueueConfig) -> Self {
        Self {
            config,
            queues: Arc::new(RwLock::new(HashMap::new())),
            pending: Arc::new(RwLock::new(HashMap::new())),
            stats: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    pub async fn create_queue(&self, name: &str) -> Result<(), String> {
        let mut queues = self.queues.write().await;
        
        if queues.contains_key(name) {
            return Err(format!("Queue {} already exists", name));
        }
        
        queues.insert(name.to_string(), VecDeque::new());
        
        let mut stats = self.stats.write().await;
        stats.insert(name.to_string(), QueueStats {
            queue_name: name.to_string(),
            message_count: 0,
            consumer_count: 0,
            avg_wait_time_ms: 0.0,
            processing_rate: 0.0,
        });
        
        Ok(())
    }

    pub async fn delete_queue(&self, name: &str) -> Result<(), String> {
        let mut queues = self.queues.write().await;
        queues.remove(name);
        
        let mut stats = self.stats.write().await;
        stats.remove(name);
        
        Ok(())
    }

    pub async fn publish(&self, request: PublishRequest) -> Result<String, String> {
        let message_id = uuid::Uuid::new_v4().to_string();
        let now = chrono::Utc::now().timestamp_millis();
        
        let message = QueueMessage {
            id: message_id.clone(),
            queue: request.queue.clone(),
            payload: request.payload,
            priority: request.priority.unwrap_or(0),
            created_at: now,
            expires_at: request.expires_at,
            retry_count: 0,
            headers: request.headers.unwrap_or_default(),
        };
        
        if let Some(delay) = request.delay_ms {
            let pending = self.pending.clone();
            let message_clone = message.clone();
            let delay_clone = delay;
            
            tokio::spawn(async move {
                tokio::time::sleep(Duration::from_millis(delay_clone)).await;
                let mut p = pending.write().await;
                p.insert(message_clone.id.clone(), message_clone);
            });
        }
        
        let mut queues = self.queues.write().await;
        
        let queue = queues
            .entry(request.queue.clone())
            .or_insert_with(VecDeque::new);
        
        let insert_pos = queue
            .iter()
            .position(|m| m.priority < message.priority)
            .unwrap_or(queue.len());
        
        queue.insert(insert_pos, message);
        
        let mut stats = self.stats.write().await;
        if let Some(stat) = stats.get_mut(&request.queue) {
            stat.message_count += 1;
        }
        
        Ok(message_id)
    }

    pub async fn consume(&self, request: ConsumeRequest) -> Result<Vec<QueueMessage>, String> {
        let mut queues = self.queues.write().await;
        
        let queue = queues
            .get_mut(&request.queue)
            .ok_or_else(|| format!("Queue {} not found", request.queue))?;
        
        let mut messages = Vec::new();
        let max = std::cmp::min(request.max_messages as usize, queue.len());
        
        for _ in 0..max {
            if let Some(msg) = queue.pop_front() {
                let mut pending = self.pending.write().await;
                pending.insert(msg.id.clone(), msg.clone());
                messages.push(msg);
            }
        }
        
        Ok(messages)
    }

    pub async fn ack(&self, request: AckRequest) -> Result<(), String> {
        let mut pending = self.pending.write().await;
        pending.remove(&request.message_id);
        
        Ok(())
    }

    pub async fn nack(&self, message_id: &str, requeue: bool) -> Result<(), String> {
        let mut pending = self.pending.write().await;
        
        if let Some(mut message) = pending.remove(message_id) {
            if requeue && message.retry_count < self.config.max_retries {
                message.retry_count += 1;
                
                let mut queues = self.queues.write().await;
                if let Some(queue) = queues.get_mut(&message.queue) {
                    queue.push_back(message);
                }
            }
        }
        
        Ok(())
    }

    pub async fn get_queue_stats(&self, queue_name: &str) -> Result<QueueStats, String> {
        let stats = self.stats.read().await;
        
        stats
            .get(queue_name)
            .cloned()
            .ok_or_else(|| format!("Queue {} not found", queue_name))
    }

    pub async fn list_queues(&self) -> Vec<String> {
        let queues = self.queues.read().await;
        queues.keys().cloned().collect()
    }

    pub async fn queue_length(&self, queue_name: &str) -> Result<u64, String> {
        let queues = self.queues.read().await;
        
        queues
            .get(queue_name)
            .map(|q| q.len() as u64)
            .ok_or_else(|| format!("Queue {} not found", queue_name))
    }

    pub async fn purge_queue(&self, queue_name: &str) -> Result<u64, String> {
        let mut queues = self.queues.write().await;
        
        if let Some(queue) = queues.get_mut(queue_name) {
            let count = queue.len() as u64;
            queue.clear();
            
            let mut stats = self.stats.write().await;
            if let Some(stat) = stats.get_mut(queue_name) {
                stat.message_count = 0;
            }
            
            Ok(count)
        } else {
            Err(format!("Queue {} not found", queue_name))
        }
    }

    pub async fn dead_letter(&self, message: &QueueMessage) -> Result<(), String> {
        let dlq_config = DeadLetterConfig::default();
        
        if !dlq_config.enabled {
            return Ok(());
        }
        
        let request = PublishRequest {
            queue: dlq_config.queue_name.clone(),
            payload: message.payload.clone(),
            priority: Some(message.priority),
            delay_ms: None,
            expires_at: Some(
                chrono::Utc::now().timestamp_millis() + dlq_config.retention_ms as i64
            ),
            headers: Some(message.headers.clone()),
        };
        
        self.publish(request).await?;
        
        Ok(())
    }
}

impl Default for MessageQueue {
    fn default() -> Self {
        Self::new(QueueConfig::default())
    }
}
