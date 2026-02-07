use std::future::Future;
use std::pin::Pin;
use std::sync::Arc;
use thiserror::Error;
use tokio::sync::{mpsc, Semaphore};
use tokio::task::JoinHandle;

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
