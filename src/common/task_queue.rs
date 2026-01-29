use std::future::Future;
use std::pin::Pin;
use std::sync::Arc;
use tokio::sync::{mpsc, Semaphore};
use tokio::task::JoinHandle;

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

pub struct TaskResult {
    pub task_id: TaskId,
    pub success: bool,
    pub message: String,
}

pub struct TaskQueue {
    sender: mpsc::UnboundedSender<Box<dyn TaskHandler>>,
    _handle: JoinHandle<()>,
}

pub trait TaskHandler: Send + 'static {
    fn execute(&self) -> Pin<Box<dyn Future<Output = TaskResult> + Send>>;
}

impl<F, Fut> TaskHandler for F
where
    F: FnOnce() -> Fut + Send + 'static,
    Fut: Future<Output = TaskResult> + Send + 'static,
{
    fn execute(&self) -> Pin<Box<dyn Future<Output = TaskResult> + Send>> {
        Box::pin((self)())
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
            let permit = semaphore.clone().acquire_owned().await.unwrap();
            tokio::spawn(async move {
                let _permit = permit;
                task.execute().await;
            });
        }
    }

    pub fn submit<F, Fut>(&self, task: F) -> Result<(), String>
    where
        F: FnOnce() -> Fut + Send + 'static,
        Fut: Future<Output = TaskResult> + Send + 'static,
    {
        self.sender
            .send(Box::new(task))
            .map_err(|e| format!("Failed to submit task: {}", e))
    }

    pub fn submit_async<F, Fut>(&self, task: F) -> Result<(), String>
    where
        F: FnOnce() -> Fut + Send + 'static,
        Fut: Future<Output = TaskResult> + Send + 'static,
    {
        self.sender
            .send(Box::new(task))
            .map_err(|e| format!("Failed to submit async task: {}", e))
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

    pub fn submit_task<F>(&self, name: String, task: F) -> Result<TaskId, String>
    where
        F: FnOnce() -> TaskResult + Send + 'static,
    {
        let task_id = self
            .task_counter
            .fetch_add(1, std::sync::atomic::Ordering::SeqCst);
        let name_clone = name.clone();

        self.task_queue
            .submit(move || {
                async move {
                    let result = task();
                    TaskResult {
                        task_id,
                        success: result.success,
                        message: format!("Task '{}': {}", name_clone, result.message),
                    }
                }
            })
            .map_err(|e| format!("Failed to submit task '{}': {}", name, e))?;

        Ok(task_id)
    }

    pub fn submit_async_task<F, Fut>(&self, name: String, task: F) -> Result<TaskId, String>
    where
        F: FnOnce() -> Fut + Send + 'static,
        Fut: std::future::Future<Output = TaskResult> + Send + 'static,
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
                    TaskResult {
                        task_id: task_id_clone,
                        success: result.success,
                        message: format!("Task '{}': {}", name_clone, result.message),
                    }
                }
            })
            .map_err(|e| format!("Failed to submit async task '{}': {}", name, e))?;

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

    #[tokio::test]
    async fn test_task_queue() {
        let queue = TaskQueue::new(2);

        let (tx, rx) = oneshot::channel();
        queue
            .submit(move || {
                tx.send(()).unwrap();
                TaskResult {
                    task_id: 1,
                    success: true,
                    message: "Task completed".to_string(),
                }
            })
            .unwrap();

        rx.await.unwrap();
    }

    #[tokio::test]
    async fn test_background_task_manager() {
        let manager = BackgroundTaskManager::new(2);

        let task_id = manager
            .submit_task("test_task".to_string(), || TaskResult {
                task_id: 0,
                success: true,
                message: "Test task completed".to_string(),
            })
            .unwrap();

        assert_eq!(task_id, 0);

        let task_id = manager
            .submit_task("test_task_2".to_string(), || TaskResult {
                task_id: 0,
                success: false,
                message: "Test task failed".to_string(),
            })
            .unwrap();

        assert_eq!(task_id, 1);
    }

    #[tokio::test]
    async fn test_concurrent_tasks() {
        let manager = BackgroundTaskManager::new(3);

        for i in 0..5 {
            manager
                .submit_task(format!("task_{}", i), move || TaskResult {
                    task_id: 0,
                    success: true,
                    message: format!("Task {} completed", i),
                })
                .unwrap();
        }

        tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;
    }
}
