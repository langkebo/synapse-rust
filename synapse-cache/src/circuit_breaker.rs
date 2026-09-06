use parking_lot::RwLock;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{Duration, Instant};
use synapse_common::config::CircuitBreakerConfig;
use synapse_common::metrics::{Counter, Gauge, MetricsCollector};

/// Circuit breaker state machine states.
///
/// Transitions: `Closed → Open → HalfOpen → Closed` (or back to `Open`).
///
/// - **Closed**: Normal operation; all calls pass through.
/// - **Open**: Failure threshold exceeded; calls are immediately rejected without attempting the operation.
/// - **HalfOpen**: Recovery probe; a limited number of calls are allowed to test if the backend has recovered.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CircuitState {
    /// Normal operation: all calls pass through.
    Closed,
    /// Failing: calls are rejected without attempting the operation.
    Open,
    /// Probing recovery: a limited number of calls are allowed.
    HalfOpen,
}

impl CircuitState {
    /// Numeric encoding for `circuit_breaker_state` gauge.
    /// - 0 = Closed (healthy, calls pass through)
    /// - 1 = Open (failing, calls rejected)
    /// - 2 = HalfOpen (probing, calls allowed)
    pub fn as_gauge_value(self) -> f64 {
        match self {
            CircuitState::Closed => 0.0,
            CircuitState::Open => 1.0,
            CircuitState::HalfOpen => 2.0,
        }
    }
}

/// W7+: outcome 维度。
///
/// 用枚举而非 `emit_outcome(success, failure, timeout, rejected)` 四个
/// bool 位置参数：后者在调用点写成 `emit_outcome(false, false, true, false)`
/// 时，读者无法一眼判断第 3 个位置代表什么，写错一个位置编译器也无法
/// 察觉（类型全相同）。枚举让调用点自解释，且新增 outcome 时无法漏改。
enum Outcome {
    Success,
    Failure,
    /// timeout 是 failure 的子集：`record_timeout` 会额外 emit 一次本值，
    /// 而 failure 计数由它内部调用的 `record_failure` 负责。
    Timeout,
    Rejected,
}

/// W7+ 限流熔断指标化：
///
/// `CircuitBreakerMetricsHandle` 把熔断器的内部状态投射到
/// `MetricsCollector`，让 Prometheus / 内部 dashboard 能观察到：
///
/// - `circuit_breaker_state{name="..."}` — 当前状态（0/1/2 gauge）
/// - `circuit_breaker_requests_total_<outcome>{name="..."}`
///   — 累计请求数（4 个 counter：success/failure/timeout/rejected，
///   每个 counter 一个独立 name）
///
/// **重要**：`MetricsCollector` 的内部 HashMap 用 `name` 字符串作主键，
/// `labels` 不参与 key。所以"同一个 metric + 不同 outcome label"的多
/// 维度方案 **行不通**（后注册的会覆盖前者）。本实现改用 4 个独立
/// name（业界 Prometheus exporter 标准做法），label 只保留 `name` 维度
/// 区分多个熔断器实例。
///
/// 通过 `attach_metrics(collector, name)` 注入；不注入时所有 emit
/// 都是 no-op，**零运行时代价**（一次 RwLock read + Option match）。
struct CircuitBreakerMetricsHandle {
    state_gauge: Gauge,
    success_counter: Counter,
    failure_counter: Counter,
    timeout_counter: Counter,
    rejected_counter: Counter,
}

#[derive(Debug, Clone, Default)]
pub struct CircuitBreakerMetrics {
    pub total_requests: u64,
    pub successful_requests: u64,
    pub failed_requests: u64,
    pub rejected_requests: u64,
    pub timeout_requests: u64,
    pub state_transitions: u64,
    pub last_failure: Option<Instant>,
    pub last_state_change: Option<Instant>,
}

struct SlidingWindow {
    failures: Vec<Instant>,
    successes: Vec<Instant>,
    window_size: Duration,
}

impl SlidingWindow {
    fn new(window_size: Duration) -> Self {
        Self { failures: Vec::new(), successes: Vec::new(), window_size }
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
    last_open_log: RwLock<Option<Instant>>,
    last_half_open_log: RwLock<Option<Instant>>,
    last_close_log: RwLock<Option<Instant>>,
    /// W7+: 投射到 MetricsCollector 的可选句柄。None = 不发指标。
    /// 通过 `attach_metrics()` 在构造后注入，避免 `new()` 签名变化
    /// 破坏所有调用方（最小侵入原则）。
    metric_handle: RwLock<Option<CircuitBreakerMetricsHandle>>,
}

impl std::fmt::Debug for CircuitBreaker {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("CircuitBreaker")
            .field("config", &self.config)
            .field("state", &*self.state.read())
            .field("opened_at", &*self.opened_at.read())
            .field("metrics", &*self.metrics.read())
            .field("total_requests", &self.total_requests.load(Ordering::Relaxed))
            .field("successful_requests", &self.successful_requests.load(Ordering::Relaxed))
            .field("failed_requests", &self.failed_requests.load(Ordering::Relaxed))
            .field("rejected_requests", &self.rejected_requests.load(Ordering::Relaxed))
            .finish()
    }
}

impl CircuitBreaker {
    pub fn new(config: CircuitBreakerConfig) -> Self {
        Self {
            state: RwLock::new(CircuitState::Closed),
            opened_at: RwLock::new(None),
            window: RwLock::new(SlidingWindow::new(Duration::from_secs(config.window_size_seconds))),
            metrics: RwLock::new(CircuitBreakerMetrics::default()),
            total_requests: AtomicU64::new(0),
            successful_requests: AtomicU64::new(0),
            failed_requests: AtomicU64::new(0),
            rejected_requests: AtomicU64::new(0),
            config,
            last_open_log: RwLock::new(None),
            last_half_open_log: RwLock::new(None),
            last_close_log: RwLock::new(None),
            metric_handle: RwLock::new(None),
        }
    }

    /// W7+: 注入 MetricsCollector 句柄以发射指标。
    ///
    /// 调用时机：在 `CircuitBreaker::new` 之后；可在 `Arc<CircuitBreaker>`
    /// 共享之前或之后调（内部用 RwLock 保护）。
    ///
    /// `name` 作为 metric label 的 `name` 字段（如 `"redis_pool"`、
    /// `"federation_dispatch"`），让多个熔断器共享同一个 collector
    /// 时能被 PromQL 区分。
    ///
    /// 一次注册 5 个 metric（1 state gauge + 4 outcome counter）。
    /// 初始状态立即 emit 一次 gauge（让 dashboard 一开始就看到值）。
    /// 重复调用会覆盖前一个 handle（典型用法是只调一次）。
    ///
    /// **设计取舍**：`MetricsCollector` 用 `name` 字符串作 HashMap 主键，
    /// labels 不参与 key。outcome 维度用 4 个独立 metric name 表达
    /// （`circuit_breaker_requests_total_success` / `_failure` / `_timeout`
    /// / `_rejected`），这是 Prometheus exporter 应对"单维 label 不支持"
    /// 的标准做法。PromQL 仍可按 metric name 选择或 sum 求总：
    ///   `sum(rate(circuit_breaker_requests_total_*[5m]))` — 总 QPS
    ///   `rate(circuit_breaker_requests_total_failure[5m])` — 失败率
    pub fn attach_metrics(&self, collector: &MetricsCollector, name: impl Into<String>) {
        let name = name.into();

        // 1 个 state gauge（label 只含 name，区分多个熔断器）
        let mut state_labels = std::collections::HashMap::new();
        state_labels.insert("name".to_string(), name.clone());
        let state_gauge = collector.register_gauge_with_labels("circuit_breaker_state".to_string(), state_labels);

        // 4 个 outcome counter——每个用独立 name（因为 MetricsCollector
        // 内部按 name 索引 counter，labels 不参与 key；4 个同名 + 不同
        // labels 会互相覆盖，最后只剩 rejected 一个）
        let make_counter = |suffix: &'static str| {
            let mut labels = std::collections::HashMap::new();
            labels.insert("name".to_string(), name.clone());
            collector.register_counter_with_labels(format!("circuit_breaker_requests_total_{suffix}"), labels)
        };
        let success_counter = make_counter("success");
        let failure_counter = make_counter("failure");
        let timeout_counter = make_counter("timeout");
        let rejected_counter = make_counter("rejected");

        // 立即 emit 当前状态（让 dashboard 一开始就看到 gauge 值）
        state_gauge.set(CircuitState::Closed.as_gauge_value());

        *self.metric_handle.write() = Some(CircuitBreakerMetricsHandle {
            state_gauge,
            success_counter,
            failure_counter,
            timeout_counter,
            rejected_counter,
        });
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
                        self.emit_outcome(Outcome::Rejected);
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

    /// W7+: 内部 helper——把 outcome 计数 emit 到对应的预注册 counter。
    /// `metric_handle` 为 None 时是 no-op（一次 RwLock read + Option match）。
    fn emit_outcome(&self, outcome: Outcome) {
        if let Some(handle) = self.metric_handle.read().as_ref() {
            match outcome {
                Outcome::Success => handle.success_counter.inc(),
                Outcome::Failure => handle.failure_counter.inc(),
                Outcome::Timeout => handle.timeout_counter.inc(),
                Outcome::Rejected => handle.rejected_counter.inc(),
            }
        }
    }

    /// W7+: emit state gauge 变更
    fn emit_state_change(&self, new_state: CircuitState) {
        if let Some(handle) = self.metric_handle.read().as_ref() {
            handle.state_gauge.set(new_state.as_gauge_value());
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

        // W7+: emit success outcome（metric_handle 为 None 时 no-op）
        self.emit_outcome(Outcome::Success);
    }

    pub fn record_failure(&self) {
        if !self.config.enabled {
            return;
        }

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
        metrics.last_failure = Some(Instant::now());

        // W7+: emit failure outcome
        self.emit_outcome(Outcome::Failure);
    }

    pub fn record_timeout(&self) {
        // record_timeout 调 record_failure 复用失败语义（HalfOpen → Open 转换、
        // 滑动窗口失败计数、metrics.failed_requests 累加都共享），并由
        // record_failure 内部 emit failure outcome。
        // 此处额外 emit timeout outcome，让 timeout 成为可独立观察的子集：
        //   _timeout 计数 <= _failure 计数（每次 timeout 必含一次 failure）
        // PromQL 用例（outcome 是 4 个独立 metric name，不是 label）：
        //   rate(circuit_breaker_requests_total_failure[5m]) — 失败率（含 timeout）
        //   rate(circuit_breaker_requests_total_timeout[5m]) — timeout 子率
        //   _failure - _timeout 即可得"非 timeout 的失败"数
        self.record_failure();

        let mut metrics = self.metrics.write();
        metrics.timeout_requests += 1;

        // W7+: emit timeout outcome（failure 已由 record_failure emit）
        self.emit_outcome(Outcome::Timeout);
    }

    fn transition_to_open(&self) {
        let mut state = self.state.write();
        if *state != CircuitState::Open {
            *state = CircuitState::Open;
            *self.opened_at.write() = Some(Instant::now());

            let mut metrics = self.metrics.write();
            metrics.state_transitions += 1;
            metrics.last_state_change = Some(Instant::now());

            let should_log = {
                let mut last_log = self.last_open_log.write();
                let now = Instant::now();
                if let Some(last) = *last_log {
                    if now.duration_since(last) < Duration::from_secs(60) {
                        false
                    } else {
                        *last_log = Some(now);
                        true
                    }
                } else {
                    *last_log = Some(now);
                    true
                }
            };

            if should_log {
                tracing::warn!(
                    target: "circuit_breaker",
                    "Circuit breaker opened due to failure threshold reached"
                );
            }
        }

        // W7+: emit state 变更（仅当 *state 实际改变时——transition 是
        // 幂等检查，重复调 transition_to_open 不会重复 emit）
        if *state == CircuitState::Open {
            drop(state);
            self.emit_state_change(CircuitState::Open);
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

            let should_log = {
                let mut last_log = self.last_half_open_log.write();
                let now = Instant::now();
                if let Some(last) = *last_log {
                    if now.duration_since(last) < Duration::from_secs(60) {
                        false
                    } else {
                        *last_log = Some(now);
                        true
                    }
                } else {
                    *last_log = Some(now);
                    true
                }
            };

            if should_log {
                tracing::info!(
                    target: "circuit_breaker",
                    "Circuit breaker transitioned to half-open state"
                );
            }
        }

        // W7+: emit state 变更（仅当 *state 实际改变时）
        if *state == CircuitState::HalfOpen {
            drop(state);
            self.emit_state_change(CircuitState::HalfOpen);
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

            let should_log = {
                let mut last_log = self.last_close_log.write();
                let now = Instant::now();
                if let Some(last) = *last_log {
                    if now.duration_since(last) < Duration::from_secs(60) {
                        false
                    } else {
                        *last_log = Some(now);
                        true
                    }
                } else {
                    *last_log = Some(now);
                    true
                }
            };

            if should_log {
                tracing::info!(
                    target: "circuit_breaker",
                    "Circuit breaker closed - service recovered"
                );
            }
        }

        // W7+: emit state 变更（仅当 *state 实际改变时）
        if *state == CircuitState::Closed {
            drop(state);
            self.emit_state_change(CircuitState::Closed);
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

    /// Returns the failure rate as a percentage (0–100).
    ///
    /// Computed as `failed_requests / total_requests * 100`. Returns `0.0` if no
    /// requests have been recorded.
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
    #[allow(missing_docs)]
    fn test_circuit_breaker_starts_closed() {
        let cb = CircuitBreaker::new(test_config());
        assert_eq!(cb.current_state(), CircuitState::Closed);
        assert!(cb.is_call_allowed());
    }

    #[test]
    #[allow(missing_docs)]
    fn test_circuit_breaker_disabled() {
        let config = CircuitBreakerConfig { enabled: false, ..test_config() };
        let cb = CircuitBreaker::new(config);

        for _ in 0..10 {
            cb.record_failure();
        }

        assert_eq!(cb.current_state(), CircuitState::Closed);
        assert!(cb.is_call_allowed());
    }

    #[test]
    #[allow(missing_docs)]
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
    #[allow(missing_docs)]
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
    #[allow(missing_docs)]
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
    #[allow(missing_docs)]
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
    #[allow(missing_docs)]
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
        assert!(metrics.last_failure.is_some());
    }

    #[test]
    #[allow(missing_docs)]
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
    #[allow(missing_docs)]
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
    #[allow(missing_docs)]
    fn test_circuit_breaker_record_timeout() {
        let cb = CircuitBreaker::new(test_config());

        cb.is_call_allowed();
        cb.record_timeout();

        let metrics = cb.get_metrics();
        assert_eq!(metrics.timeout_requests, 1);
        assert_eq!(metrics.failed_requests, 1);
    }

    #[test]
    #[allow(missing_docs)]
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

    // ── W7+: MetricsCollector 集成测试 ───────────────────────────
    //
    // 验证：
    // 1. attach_metrics 注册 5 个 metric（1 state gauge + 4 outcome counter）
    // 2. record_success / failure / timeout / rejected 都 inc 对应 counter
    // 3. state transition emit gauge
    // 4. 不 attach 时所有 emit 是 no-op（零运行时代价）
    //
    // 已知：`MetricsCollector` 内部用 `name` 字符串作 HashMap 主键，
    // labels 不参与 key，所以 4 个 outcome counter 用 4 个独立 name
    // （`circuit_breaker_requests_total_success` / `_failure` / ...）。
    // 测试用 `collect_metrics()` 拿 Vec<Metric> 验证（`Metric.value` 是 f64）。

    #[test]
    #[allow(missing_docs)]
    fn test_attach_metrics_emits_initial_closed_state() {
        use synapse_common::metrics::MetricsCollector;
        let cb = CircuitBreaker::new(test_config());
        let collector = MetricsCollector::new();

        cb.attach_metrics(&collector, "test_cb");

        // 初始 Closed 状态立即 emit
        let state_gauge = collector.get_gauge("circuit_breaker_state").expect("state gauge registered");
        assert_eq!(state_gauge.get(), 0.0, "Closed = 0");

        // 收集所有 metrics 名称，验证 5 个 metric 都注册了
        let all = collector.collect_metrics();
        let names: Vec<&str> = all.iter().map(|m| m.name.as_str()).collect();
        assert!(names.contains(&"circuit_breaker_state"), "state gauge registered: {:?}", names);
        // 4 个独立 outcome counter（name 后缀 _success/_failure/_timeout/_rejected）
        for suffix in &["success", "failure", "timeout", "rejected"] {
            let expected = format!("circuit_breaker_requests_total_{suffix}");
            assert!(
                names.contains(&expected.as_str()),
                "outcome counter `{}` registered, all names: {:?}",
                expected,
                names
            );
        }
        // 全部初始值为 0
        for m in &all {
            if m.name.starts_with("circuit_breaker_") {
                assert_eq!(m.value, 0.0, "metric {} should start at 0, got {}", m.name, m.value);
            }
        }
    }

    /// 端到端：注册到 MetricsCollector 的指标必须出现在 Prometheus 抓取
    /// 输出里。埋点写对但没进 scrape 输出 = 线上看不到，且不报错。
    ///
    /// 抓取端点是 `src/server/mod.rs` 的 `render_prometheus_metrics`
    /// （独立端口，默认 9090 + `/metrics`，由 `telemetry.prometheus.enabled`
    /// 开启），它直接渲染 `MetricsCollector::to_prometheus_format()`。
    #[test]
    #[allow(missing_docs)]
    fn test_attach_metrics_appears_in_prometheus_output() {
        use synapse_common::metrics::MetricsCollector;
        let cb = CircuitBreaker::new(test_config());
        let collector = MetricsCollector::new();
        cb.attach_metrics(&collector, "redis");
        cb.record_failure();

        let output = collector.to_prometheus_format();

        // state gauge：Closed=0，带 name label
        assert!(output.contains("# TYPE circuit_breaker_state gauge"), "state gauge 类型声明缺失:\n{output}");
        assert!(output.contains("circuit_breaker_state{name=\"redis\"} 0"), "state gauge 样本缺失:\n{output}");

        // failure counter：上面 record_failure 了一次 → 1
        assert!(
            output.contains("# TYPE circuit_breaker_requests_total_failure counter"),
            "failure counter 类型声明缺失:\n{output}"
        );
        assert!(
            output.contains("circuit_breaker_requests_total_failure{name=\"redis\"} 1"),
            "failure counter 样本缺失或数值不对:\n{output}"
        );

        // 其余 3 个 outcome counter 也应出现（初值 0）
        for suffix in ["success", "timeout", "rejected"] {
            assert!(
                output.contains(&format!("circuit_breaker_requests_total_{suffix}{{name=\"redis\"}} 0")),
                "outcome counter {suffix} 未出现在 prometheus 输出:\n{output}"
            );
        }
    }

    #[test]
    #[allow(missing_docs)]
    fn test_attach_metrics_emits_outcome_counters() {
        use synapse_common::metrics::MetricsCollector;
        let cb = CircuitBreaker::new(test_config());
        let collector = MetricsCollector::new();
        cb.attach_metrics(&collector, "cb");

        // success ×2
        cb.record_success();
        cb.record_success();
        // failure ×1
        cb.record_failure();
        // timeout ×1（内部调 record_failure + 额外 emit timeout）
        cb.record_timeout();

        // 通过 collect_metrics 拿 f64 计数（counter 内部 u64 → f64 cast）
        let all = collector.collect_metrics();
        let counter_value = |metric_name: &str| -> u64 {
            all.iter().find(|m| m.name == metric_name).map(|m| m.value as u64).unwrap_or(0)
        };

        assert_eq!(counter_value("circuit_breaker_requests_total_success"), 2);
        assert_eq!(
            counter_value("circuit_breaker_requests_total_failure"),
            2,
            "1 次 record_failure + 1 次 record_timeout 内部 record_failure = 2"
        );
        assert_eq!(counter_value("circuit_breaker_requests_total_timeout"), 1);
        assert_eq!(counter_value("circuit_breaker_requests_total_rejected"), 0);
    }

    #[test]
    #[allow(missing_docs)]
    fn test_attach_metrics_emits_state_transitions() {
        use synapse_common::metrics::MetricsCollector;
        let cb = CircuitBreaker::new(test_config());
        let collector = MetricsCollector::new();
        cb.attach_metrics(&collector, "cb");

        let state_gauge = collector.get_gauge("circuit_breaker_state").unwrap();
        assert_eq!(state_gauge.get(), 0.0, "initial Closed = 0");

        // 触发 Closed → Open（3 次 record_failure 达阈值）
        for _ in 0..3 {
            cb.record_failure();
        }
        assert_eq!(cb.current_state(), CircuitState::Open);
        assert_eq!(state_gauge.get(), 1.0, "Open = 1");

        // 触发 Open → HalfOpen（等 timeout_ms 后 is_call_allowed）
        std::thread::sleep(std::time::Duration::from_millis(150));
        let _ = cb.is_call_allowed();
        assert_eq!(cb.current_state(), CircuitState::HalfOpen);
        assert_eq!(state_gauge.get(), 2.0, "HalfOpen = 2");

        // 触发 HalfOpen → Closed（连续 2 次 success 达 success_threshold）
        cb.record_success();
        cb.record_success();
        assert_eq!(cb.current_state(), CircuitState::Closed);
        assert_eq!(state_gauge.get(), 0.0, "Closed = 0");
    }

    #[test]
    #[allow(missing_docs)]
    fn test_attach_metrics_emits_rejected_on_open() {
        // 验证 Open 状态下 is_call_allowed 拒绝时 increment rejected counter
        use synapse_common::metrics::MetricsCollector;
        let cb = CircuitBreaker::new(test_config());
        let collector = MetricsCollector::new();
        cb.attach_metrics(&collector, "cb");

        // 触发 Open
        for _ in 0..3 {
            cb.record_failure();
        }
        assert_eq!(cb.current_state(), CircuitState::Open);

        // 5 次被拒（Open 状态，timeout_ms 100ms 内）
        for _ in 0..5 {
            let allowed = cb.is_call_allowed();
            assert!(!allowed, "should be rejected in Open state");
        }

        let all = collector.collect_metrics();
        let rejected = all
            .iter()
            .find(|m| m.name == "circuit_breaker_requests_total_rejected")
            .map(|m| m.value as u64)
            .unwrap_or(0);
        assert_eq!(rejected, 5, "5 次拒绝应全部 inc rejected counter");
    }

    #[test]
    #[allow(missing_docs)]
    fn test_no_metrics_handle_is_noop() {
        // 不 attach_metrics 时 record_* 必须仍然工作（不 panic / 不影响 atomic）
        let cb = CircuitBreaker::new(test_config());
        cb.record_success();
        cb.record_failure();
        cb.record_timeout();
        let m = cb.get_metrics();
        assert_eq!(m.successful_requests, 1);
        assert_eq!(m.failed_requests, 2, "record_timeout 调 record_failure");
        assert_eq!(m.timeout_requests, 1);
    }
}
