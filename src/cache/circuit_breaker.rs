use crate::common::config::CircuitBreakerConfig;
use parking_lot::RwLock;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{Duration, Instant};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CircuitState {
    Closed,
    Open,
    HalfOpen,
}

#[derive(Debug, Clone)]
pub struct CircuitBreakerMetrics {
    pub total_requests: u64,
    pub successful_requests: u64,
    pub failed_requests: u64,
    pub rejected_requests: u64,
    pub timeout_requests: u64,
    pub state_transitions: u64,
    pub last_failure_time: Option<Instant>,
    pub last_state_change: Option<Instant>,
}

impl Default for CircuitBreakerMetrics {
    fn default() -> Self {
        Self {
            total_requests: 0,
            successful_requests: 0,
            failed_requests: 0,
            rejected_requests: 0,
            timeout_requests: 0,
            state_transitions: 0,
            last_failure_time: None,
            last_state_change: None,
        }
    }
}

struct SlidingWindow {
    failures: Vec<Instant>,
    successes: Vec<Instant>,
    window_size: Duration,
}

impl SlidingWindow {
    fn new(window_size: Duration) -> Self {
        Self {
            failures: Vec::new(),
            successes: Vec::new(),
            window_size,
        }
    }

    fn record_failure(&mut self) {
        self.prune();
        self.failures.push(Instant::now());
    }

    fn record_success(&mut self) {
        self.prune();
        self.successes.push(Instant::now());
    }

    fn prune(&mut self) {
        let cutoff = Instant::now() - self.window_size;
        self.failures.retain(|&t| t > cutoff);
        self.successes.retain(|&t| t > cutoff);
    }

    fn failure_count(&mut self) -> usize {
        self.prune();
        self.failures.len()
    }

    fn success_count(&mut self) -> usize {
        self.prune();
        self.successes.len()
    }
}

pub struct CircuitBreaker {
    config: CircuitBreakerConfig,
    state: RwLock<CircuitState>,
    opened_at: RwLock<Option<Instant>>,
    window: RwLock<SlidingWindow>,
    metrics: RwLock<CircuitBreakerMetrics>,
    total_requests: AtomicU64,
    successful_requests: AtomicU64,
    failed_requests: AtomicU64,
    rejected_requests: AtomicU64,
}

impl std::fmt::Debug for CircuitBreaker {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("CircuitBreaker")
            .field("config", &self.config)
            .field("state", &*self.state.read())
            .field("opened_at", &*self.opened_at.read())
            .field("metrics", &*self.metrics.read())
            .field(
                "total_requests",
                &self.total_requests.load(Ordering::Relaxed),
            )
            .field(
                "successful_requests",
                &self.successful_requests.load(Ordering::Relaxed),
            )
            .field(
                "failed_requests",
                &self.failed_requests.load(Ordering::Relaxed),
            )
            .field(
                "rejected_requests",
                &self.rejected_requests.load(Ordering::Relaxed),
            )
            .finish()
    }
}

impl CircuitBreaker {
    pub fn new(config: CircuitBreakerConfig) -> Self {
        Self {
            state: RwLock::new(CircuitState::Closed),
            opened_at: RwLock::new(None),
            window: RwLock::new(SlidingWindow::new(Duration::from_secs(
                config.window_size_seconds,
            ))),
            metrics: RwLock::new(CircuitBreakerMetrics::default()),
            total_requests: AtomicU64::new(0),
            successful_requests: AtomicU64::new(0),
            failed_requests: AtomicU64::new(0),
            rejected_requests: AtomicU64::new(0),
            config,
        }
    }

    pub fn is_call_allowed(&self) -> bool {
        if !self.config.enabled {
            return true;
        }

        self.total_requests.fetch_add(1, Ordering::Relaxed);

        let current_state = *self.state.read();
        match current_state {
            CircuitState::Closed => true,
            CircuitState::Open => {
                let opened_at = *self.opened_at.read();
                if let Some(opened_time) = opened_at {
                    let elapsed = opened_time.elapsed();
                    if elapsed >= Duration::from_millis(self.config.timeout_ms) {
                        self.transition_to_half_open();
                        true
                    } else {
                        self.rejected_requests.fetch_add(1, Ordering::Relaxed);
                        false
                    }
                } else {
                    self.transition_to_closed();
                    true
                }
            }
            CircuitState::HalfOpen => true,
        }
    }

    pub fn record_success(&self) {
        self.successful_requests.fetch_add(1, Ordering::Relaxed);

        let current_state = *self.state.read();
        if current_state == CircuitState::HalfOpen {
            let mut window = self.window.write();
            window.record_success();

            if window.success_count() >= self.config.success_threshold as usize {
                drop(window);
                self.transition_to_closed();
            }
        } else if current_state == CircuitState::Closed {
            self.window.write().record_success();
        }

        let mut metrics = self.metrics.write();
        metrics.successful_requests = self.successful_requests.load(Ordering::Relaxed);
        metrics.total_requests = self.total_requests.load(Ordering::Relaxed);
    }

    pub fn record_failure(&self) {
        self.failed_requests.fetch_add(1, Ordering::Relaxed);

        let current_state = *self.state.read();
        if current_state == CircuitState::HalfOpen {
            self.transition_to_open();
        } else if current_state == CircuitState::Closed {
            let mut window = self.window.write();
            window.record_failure();

            if window.failure_count() >= self.config.failure_threshold as usize {
                drop(window);
                self.transition_to_open();
            }
        }

        let mut metrics = self.metrics.write();
        metrics.failed_requests = self.failed_requests.load(Ordering::Relaxed);
        metrics.total_requests = self.total_requests.load(Ordering::Relaxed);
        metrics.last_failure_time = Some(Instant::now());
    }

    pub fn record_timeout(&self) {
        self.record_failure();

        let mut metrics = self.metrics.write();
        metrics.timeout_requests += 1;
    }

    fn transition_to_open(&self) {
        let mut state = self.state.write();
        if *state != CircuitState::Open {
            *state = CircuitState::Open;
            *self.opened_at.write() = Some(Instant::now());

            let mut metrics = self.metrics.write();
            metrics.state_transitions += 1;
            metrics.last_state_change = Some(Instant::now());

            tracing::warn!(
                target: "circuit_breaker",
                "Circuit breaker opened due to failure threshold reached"
            );
        }
    }

    fn transition_to_half_open(&self) {
        let mut state = self.state.write();
        if *state != CircuitState::HalfOpen {
            *state = CircuitState::HalfOpen;
            *self.opened_at.write() = None;

            self.window.write().successes.clear();
            self.window.write().failures.clear();

            let mut metrics = self.metrics.write();
            metrics.state_transitions += 1;
            metrics.last_state_change = Some(Instant::now());

            tracing::info!(
                target: "circuit_breaker",
                "Circuit breaker transitioned to half-open state"
            );
        }
    }

    fn transition_to_closed(&self) {
        let mut state = self.state.write();
        if *state != CircuitState::Closed {
            *state = CircuitState::Closed;
            *self.opened_at.write() = None;

            self.window.write().successes.clear();
            self.window.write().failures.clear();

            let mut metrics = self.metrics.write();
            metrics.state_transitions += 1;
            metrics.last_state_change = Some(Instant::now());

            tracing::info!(
                target: "circuit_breaker",
                "Circuit breaker closed - service recovered"
            );
        }
    }

    pub fn current_state(&self) -> CircuitState {
        *self.state.read()
    }

    pub fn get_metrics(&self) -> CircuitBreakerMetrics {
        let mut metrics = self.metrics.read().clone();
        metrics.total_requests = self.total_requests.load(Ordering::Relaxed);
        metrics.successful_requests = self.successful_requests.load(Ordering::Relaxed);
        metrics.failed_requests = self.failed_requests.load(Ordering::Relaxed);
        metrics.rejected_requests = self.rejected_requests.load(Ordering::Relaxed);
        metrics
    }

    pub fn reset(&self) {
        *self.state.write() = CircuitState::Closed;
        *self.opened_at.write() = None;
        self.window.write().successes.clear();
        self.window.write().failures.clear();

        self.total_requests.store(0, Ordering::Relaxed);
        self.successful_requests.store(0, Ordering::Relaxed);
        self.failed_requests.store(0, Ordering::Relaxed);
        self.rejected_requests.store(0, Ordering::Relaxed);

        *self.metrics.write() = CircuitBreakerMetrics::default();

        tracing::info!(
            target: "circuit_breaker",
            "Circuit breaker manually reset"
        );
    }

    pub fn failure_rate(&self) -> f64 {
        let total = self.total_requests.load(Ordering::Relaxed);
        if total == 0 {
            return 0.0;
        }
        let failed = self.failed_requests.load(Ordering::Relaxed);
        (failed as f64) / (total as f64) * 100.0
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::thread;
    use std::time::Duration;

    fn test_config() -> CircuitBreakerConfig {
        CircuitBreakerConfig {
            enabled: true,
            failure_threshold: 3,
            success_threshold: 2,
            timeout_ms: 100,
            window_size_seconds: 60,
        }
    }

    #[test]
    fn test_circuit_breaker_starts_closed() {
        let cb = CircuitBreaker::new(test_config());
        assert_eq!(cb.current_state(), CircuitState::Closed);
        assert!(cb.is_call_allowed());
    }

    #[test]
    fn test_circuit_breaker_disabled() {
        let config = CircuitBreakerConfig {
            enabled: false,
            ..test_config()
        };
        let cb = CircuitBreaker::new(config);

        for _ in 0..10 {
            cb.record_failure();
        }

        assert_eq!(cb.current_state(), CircuitState::Closed);
        assert!(cb.is_call_allowed());
    }

    #[test]
    fn test_circuit_breaker_opens_after_threshold() {
        let cb = CircuitBreaker::new(test_config());

        for _ in 0..3 {
            assert!(cb.is_call_allowed());
            cb.record_failure();
        }

        assert_eq!(cb.current_state(), CircuitState::Open);
        assert!(!cb.is_call_allowed());
    }

    #[test]
    fn test_circuit_breaker_transitions_to_half_open() {
        let cb = CircuitBreaker::new(test_config());

        for _ in 0..3 {
            cb.is_call_allowed();
            cb.record_failure();
        }

        assert_eq!(cb.current_state(), CircuitState::Open);

        thread::sleep(Duration::from_millis(150));

        assert!(cb.is_call_allowed());
        assert_eq!(cb.current_state(), CircuitState::HalfOpen);
    }

    #[test]
    fn test_circuit_breaker_closes_after_success_threshold() {
        let cb = CircuitBreaker::new(test_config());

        for _ in 0..3 {
            cb.is_call_allowed();
            cb.record_failure();
        }

        assert_eq!(cb.current_state(), CircuitState::Open);

        thread::sleep(Duration::from_millis(150));

        assert!(cb.is_call_allowed());
        assert_eq!(cb.current_state(), CircuitState::HalfOpen);

        for _ in 0..2 {
            cb.is_call_allowed();
            cb.record_success();
        }

        assert_eq!(cb.current_state(), CircuitState::Closed);
    }

    #[test]
    fn test_circuit_breaker_reopens_on_failure_in_half_open() {
        let cb = CircuitBreaker::new(test_config());

        for _ in 0..3 {
            cb.is_call_allowed();
            cb.record_failure();
        }

        assert_eq!(cb.current_state(), CircuitState::Open);

        thread::sleep(Duration::from_millis(150));

        assert!(cb.is_call_allowed());
        assert_eq!(cb.current_state(), CircuitState::HalfOpen);

        cb.is_call_allowed();
        cb.record_failure();

        assert_eq!(cb.current_state(), CircuitState::Open);
    }

    #[test]
    fn test_circuit_breaker_metrics() {
        let cb = CircuitBreaker::new(test_config());

        for _ in 0..2 {
            cb.is_call_allowed();
            cb.record_success();
        }

        for _ in 0..3 {
            cb.is_call_allowed();
            cb.record_failure();
        }

        let metrics = cb.get_metrics();
        assert_eq!(metrics.total_requests, 5);
        assert_eq!(metrics.successful_requests, 2);
        assert_eq!(metrics.failed_requests, 3);
        assert!(metrics.last_failure_time.is_some());
    }

    #[test]
    fn test_circuit_breaker_reset() {
        let cb = CircuitBreaker::new(test_config());

        for _ in 0..3 {
            cb.is_call_allowed();
            cb.record_failure();
        }

        assert_eq!(cb.current_state(), CircuitState::Open);

        cb.reset();

        assert_eq!(cb.current_state(), CircuitState::Closed);
        let metrics = cb.get_metrics();
        assert_eq!(metrics.total_requests, 0);
        assert_eq!(metrics.failed_requests, 0);
    }

    #[test]
    fn test_circuit_breaker_failure_rate() {
        let cb = CircuitBreaker::new(test_config());

        for _ in 0..7 {
            cb.is_call_allowed();
            cb.record_success();
        }

        for _ in 0..3 {
            cb.is_call_allowed();
            cb.record_failure();
        }

        let rate = cb.failure_rate();
        assert!((rate - 30.0).abs() < 0.01);
    }

    #[test]
    fn test_circuit_breaker_record_timeout() {
        let cb = CircuitBreaker::new(test_config());

        cb.is_call_allowed();
        cb.record_timeout();

        let metrics = cb.get_metrics();
        assert_eq!(metrics.timeout_requests, 1);
        assert_eq!(metrics.failed_requests, 1);
    }

    #[test]
    fn test_circuit_breaker_rejected_requests() {
        let cb = CircuitBreaker::new(test_config());

        for _ in 0..3 {
            cb.is_call_allowed();
            cb.record_failure();
        }

        assert_eq!(cb.current_state(), CircuitState::Open);

        for _ in 0..5 {
            let _ = cb.is_call_allowed();
        }

        let metrics = cb.get_metrics();
        assert_eq!(metrics.rejected_requests, 5);
    }
}
