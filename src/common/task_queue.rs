use std::future::Future;
use std::pin::Pin;
use std::sync::Arc;
use thiserror::Error;
use tokio::sync::{mpsc, Semaphore};
use tokio::task::JoinHandle;
use super::background_job::BackgroundJob;

#[cfg(test)]
use tokio::sync::oneshot;

#[derive(Debug, Error)]
pub enum TaskQueueError {
    #[error("Semaphore acquire failed: {0}")]
    SemaphoreAcquireError(String),
    #[error("Channel closed")]
    ChannelClosed,
    #[error("Task submission failed: {0}")]
    SubmissionError(String),
}

pub type TaskResult<T = ()> = Result<T, TaskQueueError>;

pub type TaskId = u64;

pub enum TaskPriority {
    Low,
    Normal,
    High,
    Critical,
}

pub struct Task {
    pub id: TaskId,
    pub priority: TaskPriority,
    pub name: String,
}

pub struct TaskResultValue {
    pub task_id: TaskId,
    pub success: bool,
    pub message: String,
}

pub struct TaskQueue {
    sender: mpsc::UnboundedSender<Box<dyn TaskHandler>>,
    _handle: JoinHandle<()>,
}

pub trait TaskHandler: Send + 'static {
    fn execute(self: Box<Self>) -> Pin<Box<dyn Future<Output = TaskResultValue> + Send>>;
}

impl<F, Fut> TaskHandler for F
where
    F: FnOnce() -> Fut + Send + 'static,
    Fut: Future<Output = TaskResultValue> + Send + 'static,
{
    fn execute(self: Box<Self>) -> Pin<Box<dyn Future<Output = TaskResultValue> + Send>> {
        Box::pin((*self)())
    }
}

impl TaskQueue {
    pub fn new(max_concurrent: usize) -> Self {
        let (sender, receiver) = mpsc::unbounded_channel();
        let handle = tokio::spawn(Self::worker(receiver, max_concurrent));

        Self {
            sender,
            _handle: handle,
        }
    }

    async fn worker(
        mut receiver: mpsc::UnboundedReceiver<Box<dyn TaskHandler>>,
        max_concurrent: usize,
    ) {
        let semaphore = Arc::new(Semaphore::new(max_concurrent));

        while let Some(task) = receiver.recv().await {
            let permit = match semaphore.clone().acquire_owned().await {
                Ok(permit) => permit,
                Err(e) => {
                    tracing::error!(error = %e, "Failed to acquire semaphore permit");
                    continue;
                }
            };

            tokio::spawn(async move {
                let _permit = permit;
                let result = task.execute().await;
                if !result.success {
                    tracing::error!(error = %result.message, "Task execution failed");
                }
            });
        }
    }

    pub fn submit<F, Fut>(&self, task: F) -> Result<(), TaskQueueError>
    where
        F: FnOnce() -> Fut + Send + 'static,
        Fut: Future<Output = TaskResultValue> + Send + 'static,
    {
        self.sender
            .send(Box::new(task))
            .map_err(|e| TaskQueueError::SubmissionError(e.to_string()))
    }

    pub fn submit_async<F, Fut>(&self, task: F) -> Result<(), TaskQueueError>
    where
        F: FnOnce() -> Fut + Send + 'static,
        Fut: Future<Output = TaskResultValue> + Send + 'static,
    {
        self.sender
            .send(Box::new(task))
            .map_err(|e| TaskQueueError::SubmissionError(e.to_string()))
    }
}

pub struct BackgroundTaskManager {
    task_queue: TaskQueue,
    task_counter: std::sync::atomic::AtomicU64,
}

impl BackgroundTaskManager {
    pub fn new(max_concurrent: usize) -> Self {
        Self {
            task_queue: TaskQueue::new(max_concurrent),
            task_counter: std::sync::atomic::AtomicU64::new(0),
        }
    }

    pub fn submit_task<F, Fut>(&self, name: String, task: F) -> Result<TaskId, TaskQueueError>
    where
        F: FnOnce() -> Fut + Send + 'static,
        Fut: Future<Output = TaskResultValue> + Send + 'static,
    {
        let task_id = self
            .task_counter
            .fetch_add(1, std::sync::atomic::Ordering::SeqCst);
        let name_clone = name.clone();

        self.task_queue
            .submit(move || async move {
                let result = task().await;
                TaskResultValue {
                    task_id,
                    success: result.success,
                    message: format!("Task '{}': {}", name_clone, result.message),
                }
            })
            .map_err(|e| TaskQueueError::SubmissionError(e.to_string()))?;

        Ok(task_id)
    }

    pub fn submit_async_task<F, Fut>(&self, name: String, task: F) -> Result<TaskId, TaskQueueError>
    where
        F: FnOnce() -> Fut + Send + 'static,
        Fut: std::future::Future<Output = TaskResultValue> + Send + 'static,
    {
        let task_id = self
            .task_counter
            .fetch_add(1, std::sync::atomic::Ordering::SeqCst);
        let name_clone = name.clone();

        self.task_queue
            .submit_async(move || {
                let task_id_clone = task_id;
                async move {
                    let result = task().await;
                    TaskResultValue {
                        task_id: task_id_clone,
                        success: result.success,
                        message: format!("Task '{}': {}", name_clone, result.message),
                    }
                }
            })
            .map_err(|e| TaskQueueError::SubmissionError(e.to_string()))?;

        Ok(task_id)
    }
}

use redis::AsyncCommands;
use deadpool_redis::{Config, Pool, Runtime};

pub struct RedisTaskQueue {
    pool: Pool,
}

impl RedisTaskQueue {
    pub async fn new(config: &crate::common::config::RedisConfig) -> Result<Self, TaskQueueError> {
        let conn_str = format!("redis://{}:{}", config.host, config.port);
        let cfg = Config::from_url(conn_str);
        
        let pool = cfg.create_pool(Some(Runtime::Tokio1)).map_err(|e| {
            TaskQueueError::SubmissionError(format!("Failed to create Redis pool: {}", e))
        })?;
        Ok(Self { pool })
    }

    pub fn from_pool(pool: Pool) -> Self {
        Self { pool }
    }

    pub async fn submit(&self, job: BackgroundJob) -> Result<String, TaskQueueError> {
        let payload = serde_json::to_string(&job).map_err(|e| {
            TaskQueueError::SubmissionError(format!("Failed to serialize job: {}", e))
        })?;

        let mut conn = self.pool.get().await.map_err(|e| {
            TaskQueueError::SubmissionError(format!("Failed to get Redis connection: {}", e))
        })?;

        // XADD mq:tasks:default * payload {json}
        let id: String = conn.xadd(
            "mq:tasks:default",
            "*",
            &[("payload", &payload)],
        ).await.map_err(|e| {
            TaskQueueError::SubmissionError(format!("Failed to XADD job: {}", e))
        })?;

        tracing::info!("Submitted background job to Redis Stream: {} -> {}", id, payload);
        Ok(id)
    }

    pub async fn consume_loop<F, Fut>(&self, group_name: &str, consumer_name: &str, handler: F) -> Result<(), TaskQueueError>
    where
        F: Fn(BackgroundJob) -> Fut + Send + Sync + 'static,
        Fut: Future<Output = Result<(), String>> + Send,
    {
        // Ensure consumer group exists
        let mut conn = self.pool.get().await.map_err(|e| {
            TaskQueueError::SubmissionError(format!("Failed to get Redis connection: {}", e))
        })?;

        let _: Result<(), _> = conn.xgroup_create_mkstream("mq:tasks:default", group_name, "$").await;

        loop {
            // XREADGROUP GROUP group_name consumer_name COUNT 1 BLOCK 2000 STREAMS mq:tasks:default >
            let opts = redis::streams::StreamReadOptions::default()
                .group(group_name, consumer_name)
                .count(1)
                .block(2000);

            let result: Result<redis::streams::StreamReadReply, _> = conn.xread_options(
                &["mq:tasks:default"],
                &[">"],
                &opts,
            ).await;

            match result {
                Ok(reply) => {
                    for stream_key in reply.keys {
                        for stream_id in stream_key.ids {
                            if let Some(payload_val) = stream_id.map.get("payload") {
                                if let Ok(payload_str) = redis::from_redis_value::<String>(payload_val) {
                                    if let Ok(job) = serde_json::from_str::<BackgroundJob>(&payload_str) {
                                        tracing::info!("Processing job {}: {:?}", stream_id.id, job);
                                        match handler(job).await {
                                            Ok(_) => {
                                                // XACK
                                                let _: Result<(), _> = conn.xack("mq:tasks:default", group_name, &[&stream_id.id]).await;
                                            }
                                            Err(e) => {
                                                tracing::error!("Job processing failed: {}", e);
                                                // Logic for retry or dead letter queue could go here
                                            }
                                        }
                                    } else {
                                        tracing::error!("Failed to deserialize job payload: {}", payload_str);
                                    }
                                }
                            }
                        }
                    }
                }
                Err(e) => {
                    tracing::warn!("Redis XREADGROUP error (timeout or connection): {}", e);
                    // Add a small delay to avoid tight loop on error
                    tokio::time::sleep(tokio::time::Duration::from_millis(1000)).await;
                    // Re-acquire connection if needed
                    if let Ok(new_conn) = self.pool.get().await {
                         conn = new_conn;
                    }
                }
            }
        }
    }
    pub async fn get_metrics(&self, group_name: &str) -> Result<QueueMetrics, TaskQueueError> {
        let mut conn = self.pool.get().await.map_err(|e| {
            TaskQueueError::SubmissionError(format!("Failed to get Redis connection: {}", e))
        })?;

        // 1. Get Stream Length (XLEN)
        let queue_length: u64 = conn.xlen("mq:tasks:default").await.map_err(|e| {
            TaskQueueError::SubmissionError(format!("Failed to get queue length: {}", e))
        })?;

        // 2. Get Pending Info (XPENDING)
        // redis::streams::StreamPendingCountReply struct in `redis` crate 0.27 might have different fields or we are using it wrong.
        // Actually, `xpending` usually returns (count, min_id, max_id, consumers).
        // Let's use `redis::Value` to be safe and parse manually or check docs.
        // The `redis` crate defines `StreamPendingCountReply` as having `count`, `min_id`, `max_id`, `consumers`.
        // Wait, the error says `available field is: ids`. This means I might be using `xpending` which returns `StreamPendingReply` (the detailed one) instead of count?
        // Ah, `xpending` with just stream and group returns summary. `xpending` with count returns details.
        // The `redis` crate mapping might be tricky.
        
        // Let's use `xpending_count` if available, or just parse generic Value.
        // Looking at the error: "available field is: `ids`". This suggests `StreamPendingReply` which is the result of XPENDING with start/end/count.
        // But I called `xpending("mq:tasks:default", group_name)`.
        
        // Let's try to map to `redis::Value` and inspect/parse manually to avoid struct mismatch issues.
        let info_val: redis::Value = conn.xpending("mq:tasks:default", group_name).await.map_err(|e| {
             TaskQueueError::SubmissionError(format!("Failed to get pending info: {}", e))
        })?;
        
        // Parse the summary response: [count, min_id, max_id, [[consumer, count], ...]]
        let (count, _min, _max, consumers_list): (u64, String, String, Vec<(String, u64)>) = redis::from_redis_value(&info_val).map_err(|e| {
             TaskQueueError::SubmissionError(format!("Failed to parse pending info: {}", e))
        })?;

        Ok(QueueMetrics {
            queue_length,
            consumer_lag: count,
            consumers: consumers_list,
        })
    }
}

#[derive(Debug, serde::Serialize)]
pub struct QueueMetrics {
    pub queue_length: u64,
    pub consumer_lag: u64,
    pub consumers: Vec<(String, u64)>,
}

impl Default for BackgroundTaskManager {
    fn default() -> Self {
        Self::new(10)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::panic;

    #[tokio::test]
    async fn test_task_queue() {
        let queue = TaskQueue::new(2);

        let (tx, rx) = oneshot::channel();

        let submit_result = queue.submit(move || async move {
            tx.send(())
                .map_err(|_| {
                    panic::panic_any(TestPanic("Failed to send through channel".to_string()))
                })
                .unwrap();
            TaskResultValue {
                task_id: 1,
                success: true,
                message: "Task completed".to_string(),
            }
        });

        assert!(submit_result.is_ok(), "Task submission should succeed");

        let rx_result = rx.await;
        assert!(rx_result.is_ok(), "Should receive the channel value");
    }

    #[derive(Debug)]
    struct TestPanic(String);

    impl std::fmt::Display for TestPanic {
        fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
            write!(f, "{}", self.0)
        }
    }

    impl std::error::Error for TestPanic {}

    #[tokio::test]
    async fn test_background_task_manager() {
        let manager = BackgroundTaskManager::new(2);

        let task_id = manager.submit_task("test_task".to_string(), || async {
            TaskResultValue {
                task_id: 0,
                success: true,
                message: "Test task completed".to_string(),
            }
        });

        assert!(task_id.is_ok(), "Task submission should succeed");
        assert_eq!(task_id.unwrap(), 0);

        let task_id = manager.submit_task("test_task_2".to_string(), || async {
            TaskResultValue {
                task_id: 0,
                success: false,
                message: "Test task failed".to_string(),
            }
        });

        assert!(task_id.is_ok(), "Task submission should succeed");
        assert_eq!(task_id.unwrap(), 1);
    }

    #[tokio::test]
    async fn test_concurrent_tasks() {
        tokio::time::pause();
        let manager = BackgroundTaskManager::new(3);

        for i in 0..5 {
            let task_id = i;
            let result = manager.submit_task(format!("task_{}", i), move || async move {
                TaskResultValue {
                    task_id,
                    success: true,
                    message: format!("Task {} completed", i),
                }
            });

            assert!(result.is_ok(), "Task {} submission should succeed", i);
        }

        tokio::time::advance(tokio::time::Duration::from_millis(100)).await;
    }
}
