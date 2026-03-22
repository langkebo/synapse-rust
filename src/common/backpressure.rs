//! 背压控制模块
//!
//! 提供令牌桶限流、连接池水位控制、请求队列管理

use std::sync::atomic::{AtomicU64, AtomicUsize, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::sync::Semaphore;
use tokio::time;

/// 背压配置
#[derive(Debug, Clone)]
pub struct BackpressureConfig {
    /// 最大并发请求数
    pub max_concurrent_requests: usize,
    /// 请求队列大小
    pub queue_size: usize,
    /// 超时时间
    pub timeout: Duration,
    /// 降级阈值（百分比）
    pub degradation_threshold: f64,
    /// 恢复阈值（百分比）
    pub recovery_threshold: f64,
}

impl Default for BackpressureConfig {
    fn default() -> Self {
        Self {
            max_concurrent_requests: 1000,
            queue_size: 5000,
            timeout: Duration::from_secs(30),
            degradation_threshold: 0.8,  // 80% 开始降级
            recovery_threshold: 0.3,     // 30% 以下恢复
        }
    }
}

/// 背压状态
#[derive(Debug, Clone)]
pub enum BackpressureState {
    /// 正常状态
    Normal,
    /// 降级状态
    Degraded,
    /// 过载状态
    Overloaded,
}

/// 背压指标
#[derive(Debug, Clone)]
pub struct BackpressureMetrics {
    pub current_requests: usize,
    pub queued_requests: usize,
    pub rejected_requests: u64,
    pub timed_out_requests: u64,
    pub state: BackpressureState,
    pubutilization: f64,
}

impl Default for BackpressureMetrics {
    fn default() -> Self {
        Self {
            current_requests: 0,
            queued_requests: 0,
            rejected_requests: 0,
            timed_out_requests: 0,
            state: BackpressureState::Normal,
            utilization: 0.0,
        }
    }
}

/// 令牌桶限流器
pub struct TokenBucket {
    capacity: usize,
    tokens: AtomicUsize,
    refill_rate: Duration,
    last_refill: AtomicU64,
}

impl TokenBucket {
    pub fn new(capacity: usize, refill_rate: Duration) -> Self {
        Self {
            capacity,
            tokens: AtomicUsize::new(capacity),
            refill_rate,
            last_refill: AtomicU64::new(0),
        }
    }

    /// 尝试获取一个令牌
    pub fn try_acquire(&self) -> bool {
        self.refill();
        
        loop {
            let current = self.tokens.load(Ordering::Relaxed);
            if current == 0 {
                return false;
            }
            
            if self.tokens.compare_exchange(current, current - 1, Ordering::Relaxed, Ordering::Relaxed).is_ok() {
                return true;
            }
        }
    }

    /// 手动 refill
    fn refill(&self) {
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_millis() as u64;
        
        let last = self.last_refill.load(Ordering::Relaxed);
        if now - last >= self.refill_rate.as_millis() as u64 {
            self.tokens.store(self.capacity, Ordering::Relaxed);
            self.last_refill.store(now, Ordering::Relaxed);
        }
    }
}

/// 请求限流器
pub struct RateLimiter {
    semaphore: Semaphore,
    config: BackpressureConfig,
    metrics: Arc<RateLimiterMetrics>,
}

#[derive(Debug)]
pub struct RateLimiterMetrics {
    pub active_requests: AtomicUsize,
    pub rejected: AtomicU64,
    pub acquired: AtomicU64,
}

impl RateLimiterMetrics {
    pub fn new() -> Self {
        Self {
            active_requests: AtomicUsize::new(0),
            rejected: AtomicU64::new(0),
            acquired: AtomicU64::new(0),
        }
    }
}

impl RateLimiter {
    pub fn new(config: BackpressureConfig) -> Self {
        let semaphore = Semaphore::new(config.max_concurrent_requests);
        
        Self {
            semaphore,
            config,
            metrics: Arc::new(RateLimiterMetrics::new()),
        }
    }

    /// 尝试获取许可（不阻塞）
    pub fn try_acquire(&self) -> bool {
        match self.semaphore.try_acquire() {
            Ok(_) => {
                self.metrics.acquired.fetch_add(1, Ordering::Relaxed);
                self.metrics.active_requests.fetch_add(1, Ordering::Relaxed);
                true
            }
            Err(_) => {
                self.metrics.rejected.fetch_add(1, Ordering::Relaxed);
                false
            }
        }
    }

    /// 获取许可（阻塞）
    pub async fn acquire(&self) -> Result<RateLimiterPermit, RateLimitError> {
        match tokio::time::timeout(self.config.timeout, self.semaphore.acquire()).await {
            Ok(Ok(permit)) => {
                self.metrics.acquired.fetch_add(1, Ordering::Relaxed);
                self.metrics.active_requests.fetch_add(1, Ordering::Relaxed);
                Ok(RateLimiterPermit {
                    metrics: self.metrics.clone(),
                })
            }
            Ok(Err(_)) => Err(RateLimitError::Closed),
            Err(_) => {
                self.metrics.rejected.fetch_add(1, Ordering::Relaxed);
                Err(RateLimitError::Timeout)
            }
        }
    }

    /// 获取当前指标
    pub fn get_metrics(&self) -> BackpressureMetrics {
        let active = self.metrics.active_requests.load(Ordering::Relaxed);
        let rejected = self.metrics.rejected.load(Ordering::Relaxed);
        let capacity = self.config.max_concurrent_requests;
        let utilization = active as f64 / capacity as f64;

        let state = if utilization >= self.config.degradation_threshold {
            BackpressureState::Overloaded
        } else if utilization >= self.config.degradation_threshold * 0.7 {
            BackpressureState::Degraded
        } else {
            BackpressureState::Normal
        };

        BackpressureMetrics {
            current_requests: active,
            queued_requests: 0,
            rejected_requests: rejected,
            timed_out_requests: 0,
            state,
            utilization,
        }
    }

    /// 手动释放许可
    pub fn release(&self) {
        self.metrics.active_requests.fetch_sub(1, Ordering::Relaxed);
    }
}

/// 限流器许可
pub struct RateLimiterPermit {
    metrics: Arc<RateLimiterMetrics>,
}

impl Drop for RateLimiterPermit {
    fn drop(&mut self) {
        self.metrics.active_requests.fetch_sub(1, Ordering::Relaxed);
    }
}

/// 限流错误
#[derive(Debug)]
pub enum RateLimitError {
    Timeout,
    Closed,
}

impl std::fmt::Display for RateLimitError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            RateLimitError::Timeout => write!(f, "Rate limit timeout"),
            RateLimitError::Closed => write!(f, "Rate limiter closed"),
        }
    }
}

impl std::error::Error for RateLimitError {}

/// 连接池水位控制器
pub struct PoolWatermarkController {
    /// 低水位线
    low_watermark: usize,
    /// 高水位线
    high_watermark: usize,
    /// 当前活跃连接数
    active_connections: AtomicUsize,
    /// 获取连接等待数
    waiting_requests: AtomicUsize,
}

impl PoolWatermarkController {
    pub fn new(low_watermark: usize, high_watermark: usize) -> Self {
        Self {
            low_watermark,
            high_watermark,
            active_connections: AtomicUsize::new(0),
            waiting_requests: AtomicUsize::new(0),
        }
    }

    /// 记录连接获取
    pub fn acquire(&self) {
        self.active_connections.fetch_add(1, Ordering::Relaxed);
    }

    /// 记录连接释放
    pub fn release(&self) {
        self.active_connections.fetch_sub(1, Ordering::Relaxed);
    }

    /// 记录等待请求
    pub fn add_waiter(&self) {
        self.waiting_requests.fetch_add(1, Ordering::Relaxed);
    }

    /// 移除等待请求
    pub fn remove_waiter(&self) {
        self.waiting_requests.fetch_sub(1, Ordering::Relaxed);
    }

    /// 获取当前状态
    pub fn get_state(&self) -> PoolState {
        let active = self.active_connections.load(Ordering::Relaxed);
        let waiting = self.waiting_requests.load(Ordering::Relaxed);
        
        let utilization = active as f64 / self.high_watermark as f64;
        
        if utilization >= 1.0 || waiting > active * 2 {
            PoolState::Critical
        } else if utilization >= 0.8 {
            PoolState::High
        } else if utilization >= 0.5 {
            PoolState::Normal
        } else {
            PoolState::Low
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PoolState {
    Low,
    Normal,
    High,
    Critical,
}

/// 请求队列（带背压）
pub struct BackpressureQueue<T> {
    sender: tokio::sync::mpsc::Sender<T>,
    receiver: tokio::sync::mpsc::Receiver<T>,
    metrics: Arc<QueueMetrics>,
}

#[derive(Debug)]
pub struct QueueMetrics {
    pub enqueued: AtomicU64,
    pub dequeued: AtomicU64,
    pub dropped: AtomicU64,
    pub current_size: AtomicUsize,
}

impl QueueMetrics {
    pub fn new() -> Self {
        Self {
            enqueued: AtomicU64::new(0),
            dequeued: AtomicU64::new(0),
            dropped: AtomicU64::new(0),
            current_size: AtomicUsize::new(0),
        }
    }
}

impl<T> BackpressureQueue<T> {
    pub fn new(capacity: usize) -> Self {
        let (sender, receiver) = tokio::sync::mpsc::channel(capacity);
        
        Self {
            sender,
            receiver,
            metrics: Arc::new(QueueMetrics::new()),
        }
    }

    /// 入队（非阻塞）
    pub fn try_send(&self, value: T) -> Result<(), QueueError> {
        match self.sender.try_send(value) {
            Ok(_) => {
                self.metrics.enqueued.fetch_add(1, Ordering::Relaxed);
                self.metrics.current_size.fetch_add(1, Ordering::Relaxed);
                Ok(())
            }
            Err(e) => {
                self.metrics.dropped.fetch_add(1, Ordering::Relaxed);
                Err(QueueError::Full)
            }
        }
    }

    /// 入队（阻塞）
    pub async fn send(&self, value: T) -> Result<(), QueueError> {
        match self.sender.send(value).await {
            Ok(_) => {
                self.metrics.enqueued.fetch_add(1, Ordering::Relaxed);
                self.metrics.current_size.fetch_add(1, Ordering::Relaxed);
                Ok(())
            }
            Err(_) => {
                self.metrics.dropped.fetch_add(1, Ordering::Relaxed);
                Err(QueueError::Closed)
            }
        }
    }

    /// 出队
    pub async fn recv(&mut self) -> Option<T> {
        match self.receiver.recv().await {
            Some(value) => {
                self.metrics.dequeued.fetch_add(1, Ordering::Relaxed);
                self.metrics.current_size.fetch_sub(1, Ordering::Relaxed);
                Some(value)
            }
            None => None,
        }
    }

    /// 获取队列长度
    pub fn len(&self) -> usize {
        self.metrics.current_size.load(Ordering::Relaxed)
    }

    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }
}

#[derive(Debug)]
pub enum QueueError {
    Full,
    Closed,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_token_bucket() {
        let bucket = TokenBucket::new(10, Duration::from_secs(1));
        assert!(bucket.try_acquire());
        assert!(bucket.try_acquire());
    }

    #[test]
    fn test_rate_limiter_config_default() {
        let config = BackpressureConfig::default();
        assert_eq!(config.max_concurrent_requests, 1000);
        assert_eq!(config.queue_size, 5000);
    }

    #[test]
    fn test_pool_watermark_controller() {
        let controller = PoolWatermarkController::new(10, 100);
        controller.acquire();
        assert_eq!(controller.get_state(), PoolState::Low);
    }
}
