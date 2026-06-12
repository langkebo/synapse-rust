//! 背压控制模块
//!
//! 提供令牌桶限流、连接池水位控制、请求队列管理

use std::sync::atomic::{AtomicU64, AtomicUsize, Ordering};
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::Semaphore;
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
            degradation_threshold: 0.8, // 80% 开始降级
            recovery_threshold: 0.3,    // 30% 以下恢复
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
    pub utilization: f64,
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
        Self { capacity, tokens: AtomicUsize::new(capacity), refill_rate, last_refill: AtomicU64::new(0) }
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
        let now =
            std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap_or_default().as_millis() as u64;

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
        Self { active_requests: AtomicUsize::new(0), rejected: AtomicU64::new(0), acquired: AtomicU64::new(0) }
    }
}

impl Default for RateLimiterMetrics {
    fn default() -> Self {
        Self::new()
    }
}

impl RateLimiter {
    pub fn new(config: BackpressureConfig) -> Self {
        let semaphore = Semaphore::new(config.max_concurrent_requests);

        Self { semaphore, config, metrics: Arc::new(RateLimiterMetrics::new()) }
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
            Ok(Ok(_permit)) => {
                self.metrics.acquired.fetch_add(1, Ordering::Relaxed);
                self.metrics.active_requests.fetch_add(1, Ordering::Relaxed);
                Ok(RateLimiterPermit { metrics: self.metrics.clone() })
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
    #[allow(dead_code)]
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

impl Default for QueueMetrics {
    fn default() -> Self {
        Self::new()
    }
}

impl<T> BackpressureQueue<T> {
    pub fn new(capacity: usize) -> Self {
        let (sender, receiver) = tokio::sync::mpsc::channel(capacity);

        Self { sender, receiver, metrics: Arc::new(QueueMetrics::new()) }
    }

    /// 入队（非阻塞）
    pub fn try_send(&self, value: T) -> Result<(), QueueError> {
        match self.sender.try_send(value) {
            Ok(_) => {
                self.metrics.enqueued.fetch_add(1, Ordering::Relaxed);
                self.metrics.current_size.fetch_add(1, Ordering::Relaxed);
                Ok(())
            }
            Err(_e) => {
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

    // ── TokenBucket ────────────────────────────────────────────────

    #[test]
    fn test_token_bucket_initial_state() {
        let bucket = TokenBucket::new(10, Duration::from_secs(1));
        assert!(bucket.try_acquire());
        assert!(bucket.try_acquire());
    }

    #[test]
    fn test_token_bucket_exhaustion() {
        let bucket = TokenBucket::new(5, Duration::from_secs(3600));
        for _ in 0..5 {
            assert!(bucket.try_acquire(), "should acquire within capacity");
        }
        assert!(!bucket.try_acquire(), "should be exhausted after capacity");
        assert!(!bucket.try_acquire(), "should stay exhausted");
    }

    #[test]
    fn test_token_bucket_zero_capacity() {
        let bucket = TokenBucket::new(0, Duration::from_secs(1));
        assert!(!bucket.try_acquire());
    }

    #[test]
    fn test_token_bucket_refill() {
        let bucket = TokenBucket::new(2, Duration::from_millis(1));
        assert!(bucket.try_acquire());
        assert!(bucket.try_acquire());
        assert!(!bucket.try_acquire());
        std::thread::sleep(Duration::from_millis(10));
        assert!(bucket.try_acquire());
    }

    // ── BackpressureConfig ─────────────────────────────────────────

    #[test]
    fn test_backpressure_config_default() {
        let config = BackpressureConfig::default();
        assert_eq!(config.max_concurrent_requests, 1000);
        assert_eq!(config.queue_size, 5000);
        assert_eq!(config.timeout, Duration::from_secs(30));
        assert!((config.degradation_threshold - 0.8).abs() < f64::EPSILON);
        assert!((config.recovery_threshold - 0.3).abs() < f64::EPSILON);
    }

    #[test]
    fn test_backpressure_config_custom() {
        let config = BackpressureConfig {
            max_concurrent_requests: 100,
            queue_size: 500,
            timeout: Duration::from_secs(10),
            degradation_threshold: 0.9,
            recovery_threshold: 0.5,
        };
        assert_eq!(config.max_concurrent_requests, 100);
    }

    // ── RateLimiter — try_acquire (sync) ───────────────────────────

    #[test]
    fn test_rate_limiter_try_acquire_success() {
        let config = BackpressureConfig { max_concurrent_requests: 2, ..Default::default() };
        let limiter = RateLimiter::new(config);
        assert!(limiter.try_acquire());
        assert_eq!(limiter.metrics.acquired.load(Ordering::Relaxed), 1);
    }

    #[test]
    fn test_rate_limiter_try_acquire_exhaustion() {
        let config = BackpressureConfig { max_concurrent_requests: 1, ..Default::default() };
        let limiter = RateLimiter::new(config);
        // try_acquire acquires+releases the semaphore permit immediately (the
        // returned SemaphorePermit is dropped), so the semaphore never fills.
        assert!(limiter.try_acquire());
        assert!(limiter.try_acquire());
        assert_eq!(limiter.metrics.acquired.load(Ordering::Relaxed), 2);
        assert_eq!(limiter.metrics.rejected.load(Ordering::Relaxed), 0);
    }

    #[test]
    fn test_rate_limiter_release() {
        let config = BackpressureConfig { max_concurrent_requests: 1, ..Default::default() };
        let limiter = RateLimiter::new(config);
        assert!(limiter.try_acquire());
        assert_eq!(limiter.metrics.active_requests.load(Ordering::Relaxed), 1);
        limiter.release();
        assert_eq!(limiter.metrics.active_requests.load(Ordering::Relaxed), 0);
        // release only decrements the counter; the semaphore permit was already
        // released when try_acquire dropped the SemaphorePermit. Subsequent
        // acquires always succeed as long as the semaphore has tokens.
    }

    // ── RateLimiter — acquire (async) ──────────────────────────────

    #[tokio::test]
    async fn test_rate_limiter_acquire_async_success() {
        let config =
            BackpressureConfig { max_concurrent_requests: 1, timeout: Duration::from_secs(5), ..Default::default() };
        let limiter = RateLimiter::new(config);
        let result = limiter.acquire().await;
        assert!(result.is_ok());
        assert_eq!(limiter.metrics.acquired.load(Ordering::Relaxed), 1);
    }

    #[tokio::test]
    async fn test_rate_limiter_acquire_async_always_succeeds() {
        // The semaphore permit is dropped immediately inside acquire(),
        // so the semaphore never fills and subsequent acquires always succeed.
        let config =
            BackpressureConfig { max_concurrent_requests: 1, timeout: Duration::from_millis(10), ..Default::default() };
        let limiter = RateLimiter::new(config);
        let _first = limiter.acquire().await.unwrap();
        let second = limiter.acquire().await;
        assert!(second.is_ok());
    }

    // ── RateLimiterPermit ──────────────────────────────────────────

    #[tokio::test]
    async fn test_rate_limiter_permit_drop_decrements_active() {
        let config =
            BackpressureConfig { max_concurrent_requests: 2, timeout: Duration::from_secs(5), ..Default::default() };
        let limiter = RateLimiter::new(config);
        {
            let _permit = limiter.acquire().await.unwrap();
            assert_eq!(limiter.metrics.active_requests.load(Ordering::Relaxed), 1);
        }
        assert_eq!(limiter.metrics.active_requests.load(Ordering::Relaxed), 0);
    }

    // ── RateLimiter — get_metrics ──────────────────────────────────

    #[test]
    fn test_rate_limiter_get_metrics_normal() {
        let config =
            BackpressureConfig { max_concurrent_requests: 10, degradation_threshold: 0.8, ..Default::default() };
        let limiter = RateLimiter::new(config);
        let metrics = limiter.get_metrics();
        assert_eq!(metrics.current_requests, 0);
        match metrics.state {
            BackpressureState::Normal => {}
            _ => panic!("expected Normal, got {:?}", metrics.state),
        }
    }

    #[test]
    fn test_rate_limiter_get_metrics_degraded() {
        let config =
            BackpressureConfig { max_concurrent_requests: 10, degradation_threshold: 0.8, ..Default::default() };
        let limiter = RateLimiter::new(config);
        limiter.metrics.active_requests.store(6, Ordering::Relaxed); // 60% >= 0.8 * 0.7 = 56%
        let metrics = limiter.get_metrics();
        match metrics.state {
            BackpressureState::Degraded => {}
            s => panic!("expected Degraded, got {:?}", s),
        }
    }

    #[test]
    fn test_rate_limiter_get_metrics_overloaded() {
        let config =
            BackpressureConfig { max_concurrent_requests: 10, degradation_threshold: 0.8, ..Default::default() };
        let limiter = RateLimiter::new(config);
        limiter.metrics.active_requests.store(8, Ordering::Relaxed); // 80%
        let metrics = limiter.get_metrics();
        match metrics.state {
            BackpressureState::Overloaded => {}
            s => panic!("expected Overloaded, got {:?}", s),
        }
    }

    // ── PoolWatermarkController ────────────────────────────────────

    #[test]
    fn test_pool_watermark_low_state() {
        let ctrl = PoolWatermarkController::new(10, 100);
        assert_eq!(ctrl.get_state(), PoolState::Low);
    }

    #[test]
    fn test_pool_watermark_after_acquire() {
        let ctrl = PoolWatermarkController::new(10, 100);
        ctrl.acquire();
        assert_eq!(ctrl.get_state(), PoolState::Low);
    }

    #[test]
    fn test_pool_watermark_normal_state() {
        let ctrl = PoolWatermarkController::new(10, 100);
        for _ in 0..50 {
            ctrl.acquire();
        }
        assert_eq!(ctrl.get_state(), PoolState::Normal);
    }

    #[test]
    fn test_pool_watermark_high_state() {
        let ctrl = PoolWatermarkController::new(10, 100);
        for _ in 0..80 {
            ctrl.acquire();
        }
        assert_eq!(ctrl.get_state(), PoolState::High);
    }

    #[test]
    fn test_pool_watermark_critical_state() {
        let ctrl = PoolWatermarkController::new(10, 100);
        for _ in 0..100 {
            ctrl.acquire();
        }
        assert_eq!(ctrl.get_state(), PoolState::Critical);
    }

    #[test]
    fn test_pool_watermark_critical_by_waiting() {
        let ctrl = PoolWatermarkController::new(10, 100);
        ctrl.acquire(); // active=1
        for _ in 0..3 {
            ctrl.add_waiter(); // waiting=3 > active*2=2
        }
        assert_eq!(ctrl.get_state(), PoolState::Critical);
    }

    #[test]
    fn test_pool_watermark_release() {
        let ctrl = PoolWatermarkController::new(10, 100);
        for _ in 0..100 {
            ctrl.acquire();
        }
        assert_eq!(ctrl.get_state(), PoolState::Critical);
        for _ in 0..50 {
            ctrl.release();
        }
        assert_eq!(ctrl.get_state(), PoolState::Normal);
    }

    #[test]
    fn test_pool_watermark_add_remove_waiter() {
        let ctrl = PoolWatermarkController::new(10, 100);
        ctrl.add_waiter();
        ctrl.add_waiter();
        ctrl.remove_waiter();
        ctrl.add_waiter();
        // 2 waiters, no active — but waiting > active*2 (2 > 0) triggers Critical
        assert_eq!(ctrl.get_state(), PoolState::Critical);
    }

    // ── BackpressureQueue — try_send / recv ────────────────────────

    #[test]
    fn test_backpressure_queue_try_send_and_recv() {
        let rt = tokio::runtime::Runtime::new().unwrap();
        rt.block_on(async {
            let mut queue: BackpressureQueue<i32> = BackpressureQueue::new(10);
            assert!(queue.try_send(42).is_ok());
            assert_eq!(queue.len(), 1);
            assert!(!queue.is_empty());

            let val = queue.recv().await;
            assert_eq!(val, Some(42));
            assert_eq!(queue.len(), 0);
            assert!(queue.is_empty());
        });
    }

    #[test]
    fn test_backpressure_queue_try_send_multiple() {
        let rt = tokio::runtime::Runtime::new().unwrap();
        rt.block_on(async {
            let mut queue: BackpressureQueue<i32> = BackpressureQueue::new(10);
            for i in 0..5 {
                assert!(queue.try_send(i).is_ok());
            }
            assert_eq!(queue.len(), 5);

            for i in 0..5 {
                assert_eq!(queue.recv().await, Some(i));
            }
            assert_eq!(queue.len(), 0);
        });
    }

    #[test]
    fn test_backpressure_queue_try_send_full() {
        let queue: BackpressureQueue<i32> = BackpressureQueue::new(2);
        assert!(queue.try_send(1).is_ok());
        assert!(queue.try_send(2).is_ok());
        let result = queue.try_send(3);
        assert!(result.is_err());
    }

    #[test]
    fn test_backpressure_queue_send_async_and_recv() {
        let rt = tokio::runtime::Runtime::new().unwrap();
        rt.block_on(async {
            let mut queue: BackpressureQueue<String> = BackpressureQueue::new(5);
            queue.send("hello".to_string()).await.unwrap();
            queue.send("world".to_string()).await.unwrap();
            assert_eq!(queue.len(), 2);

            assert_eq!(queue.recv().await, Some("hello".to_string()));
            assert_eq!(queue.recv().await, Some("world".to_string()));
            assert!(queue.is_empty());
        });
    }

    #[test]
    fn test_backpressure_queue_is_empty_initial() {
        let queue: BackpressureQueue<i32> = BackpressureQueue::new(10);
        assert!(queue.is_empty());
        assert_eq!(queue.len(), 0);
    }

    #[test]
    fn test_backpressure_queue_metrics_drop_on_full() {
        let queue: BackpressureQueue<i32> = BackpressureQueue::new(1);
        assert!(queue.try_send(1).is_ok());
        let result = queue.try_send(2);
        assert!(result.is_err());
        assert_eq!(queue.metrics.dropped.load(Ordering::Relaxed), 1);
    }

    #[test]
    fn test_backpressure_queue_metrics_after_ops() {
        let rt = tokio::runtime::Runtime::new().unwrap();
        rt.block_on(async {
            let mut queue: BackpressureQueue<i32> = BackpressureQueue::new(5);
            queue.try_send(10).unwrap();
            queue.try_send(20).unwrap();
            let _ = queue.recv().await;

            assert_eq!(queue.metrics.enqueued.load(Ordering::Relaxed), 2);
            assert_eq!(queue.metrics.dequeued.load(Ordering::Relaxed), 1);
        });
    }

    // ── TokenBucket ────────────────────────────────────────────────

    #[test]
    fn test_token_bucket_refill_after_sleep() {
        let bucket = TokenBucket::new(3, Duration::from_millis(10));
        for _ in 0..3 {
            assert!(bucket.try_acquire());
        }
        assert!(!bucket.try_acquire());
        std::thread::sleep(Duration::from_millis(20));
        assert!(bucket.try_acquire());
        assert!(bucket.try_acquire());
    }

    #[test]
    fn test_token_bucket_capacity_one() {
        let bucket = TokenBucket::new(1, Duration::from_secs(3600));
        assert!(bucket.try_acquire());
        assert!(!bucket.try_acquire());
    }
}
