use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use std::time::Instant;
use thiserror::Error;

#[derive(Debug, Error)]
/// Represents MetricsError; see per-variant docs.
pub enum MetricsError {
    #[error("Histogram value comparison error: {0}")]
    /// `ValueComparisonError` variant.
    ValueComparisonError(String),
}

#[derive(Debug, Clone)]
/// Represents Metric.
pub struct Metric {
    /// `name` field.
    pub name: String,
    /// `value` field.
    pub value: f64,
    /// `timestamp` field.
    pub timestamp: Instant,
    /// `labels` field.
    pub labels: HashMap<String, String>,
}

#[derive(Debug, Clone)]
/// Represents Counter.
pub struct Counter {
    name: String,
    value: Arc<AtomicU64>,
    labels: HashMap<String, String>,
}

impl Counter {
    /// Constructs a new instance.
    pub fn new(name: String) -> Self {
        Self { name, value: Arc::new(AtomicU64::new(0)), labels: HashMap::new() }
    }

    /// Constructs a new instance with labels attached.
    pub fn with_labels(name: String, labels: HashMap<String, String>) -> Self {
        Self { name, value: Arc::new(AtomicU64::new(0)), labels }
    }

    /// Increments by one.
    pub fn inc(&self) {
        self.value.fetch_add(1, Ordering::Relaxed);
    }

    /// Increments by the given delta.
    pub fn inc_by(&self, delta: u64) {
        self.value.fetch_add(delta, Ordering::Relaxed);
    }

    /// Returns the current value.
    pub fn get(&self) -> u64 {
        self.value.load(Ordering::Relaxed)
    }

    /// Resets to its initial state.
    pub fn reset(&self) {
        self.value.store(0, Ordering::Relaxed);
    }
}

#[derive(Debug, Clone)]
/// Represents Gauge.
pub struct Gauge {
    name: String,
    value: Arc<AtomicU64>,
    labels: HashMap<String, String>,
}

impl Gauge {
    /// Constructs a new instance.
    pub fn new(name: String) -> Self {
        Self { name, value: Arc::new(AtomicU64::new(0.0f64.to_bits())), labels: HashMap::new() }
    }

    /// Constructs a new instance with labels attached.
    pub fn with_labels(name: String, labels: HashMap<String, String>) -> Self {
        Self { name, value: Arc::new(AtomicU64::new(0.0f64.to_bits())), labels }
    }

    /// Sets the value.
    pub fn set(&self, value: f64) {
        self.value.store(value.to_bits(), Ordering::Relaxed);
    }

    /// Increments by one.
    pub fn inc(&self) {
        self.add(1.0);
    }

    /// Decrements by one.
    pub fn dec(&self) {
        self.sub(1.0);
    }

    /// Adds the given delta.
    pub fn add(&self, delta: f64) {
        self.update(|current| current + delta);
    }

    /// Subtracts the given delta.
    pub fn sub(&self, delta: f64) {
        self.update(|current| current - delta);
    }

    /// Returns the current value.
    pub fn get(&self) -> f64 {
        f64::from_bits(self.value.load(Ordering::Relaxed))
    }

    fn update(&self, f: impl Fn(f64) -> f64) {
        let mut current = self.value.load(Ordering::Relaxed);
        loop {
            let current_value = f64::from_bits(current);
            let next = f(current_value).to_bits();
            match self.value.compare_exchange(current, next, Ordering::Relaxed, Ordering::Relaxed) {
                Ok(_) => break,
                Err(observed) => current = observed,
            }
        }
    }
}

/// Prometheus 原生直方图的默认分桶上界（单位：毫秒）。
///
/// 覆盖 `*_ms` 类时长指标：从亚毫秒的缓存命中断点到数秒的 DB / 联邦请求。
/// 必须**严格升序**——`Histogram::cumulative_counts` 用 `partition_point`
/// 二分定位，乱序会静默算错每个桶。
const HISTOGRAM_BUCKETS_MS: &[f64] =
    &[1.0, 2.5, 5.0, 10.0, 25.0, 50.0, 100.0, 250.0, 500.0, 1000.0, 2500.0, 5000.0, 10000.0];

/// Prometheus 原生直方图的默认分桶上界（单位：秒）。
///
/// 覆盖 `*_seconds` 类时长指标（如 `auth_login_duration_seconds`）。
/// 同样必须**严格升序**。
const HISTOGRAM_BUCKETS_SECONDS: &[f64] = &[0.005, 0.01, 0.025, 0.05, 0.1, 0.25, 0.5, 1.0, 2.5, 5.0, 10.0];

/// 按指标名后缀挑选分桶表。
///
/// 本仓所有时长直方图的**单位写在名字里**（`_ms` / `_seconds`），而 `Histogram`
/// 的观测值本身不携带单位元数据，因此只能按名字推断。非 `_seconds` 结尾者
/// （含测试用的无单位名称）一律走毫秒表。
fn histogram_buckets_for(name: &str) -> &'static [f64] {
    if name.ends_with("_seconds") {
        HISTOGRAM_BUCKETS_SECONDS
    } else {
        HISTOGRAM_BUCKETS_MS
    }
}

#[derive(Debug, Clone)]
/// Represents Histogram.
pub struct Histogram {
    name: String,
    values: Arc<parking_lot::Mutex<Vec<f64>>>,
    labels: HashMap<String, String>,
}

/// 直方图的单次加锁快照：`count` / `sum` / 各桶累积计数取自**同一时刻**。
struct HistogramSnapshot {
    /// 观测总数；同时用于 `_count` 与 `le="+Inf"` 桶。
    count: u64,
    /// 观测值之和；已归一负零，用于 `_sum`。
    sum: f64,
    /// 各分桶上界上的累积计数，与请求的 `bounds` 一一对应。
    cumulative: Vec<u64>,
}

impl Histogram {
    /// Constructs a new instance.
    pub fn new(name: String) -> Self {
        Self { name, values: Arc::new(parking_lot::Mutex::new(Vec::new())), labels: HashMap::new() }
    }

    /// Constructs a new instance with labels attached.
    pub fn with_labels(name: String, labels: HashMap<String, String>) -> Self {
        Self { name, values: Arc::new(parking_lot::Mutex::new(Vec::new())), labels }
    }

    /// Records a new observed value.
    pub fn observe(&self, value: f64) {
        let mut values = self.values.lock();
        values.push(value);
    }

    /// Returns all recorded values.
    pub fn get_values(&self) -> Vec<f64> {
        let values = self.values.lock();
        values.clone()
    }

    /// Returns the number of recorded values.
    pub fn get_count(&self) -> usize {
        let values = self.values.lock();
        values.len()
    }

    /// Returns the sum of recorded values.
    pub fn get_sum(&self) -> f64 {
        let values = self.values.lock();
        values.iter().sum()
    }

    /// Returns the average of recorded values.
    pub fn get_avg(&self) -> f64 {
        let values = self.values.lock();
        if values.is_empty() {
            0.0
        } else {
            values.iter().sum::<f64>() / values.len() as f64
        }
    }

    /// Returns the value at the given percentile.
    pub fn get_percentile(&self, percentile: f64) -> Result<f64, MetricsError> {
        let mut values = self.values.lock();
        if values.is_empty() {
            return Ok(0.0);
        }
        values.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
        let index = ((percentile / 100.0) * (values.len() - 1) as f64).floor() as usize;
        Ok(values[index.min(values.len() - 1)])
    }

    /// 一次加锁取出渲染直方图所需的全部数据。
    ///
    /// 必须**单次加锁**取全：`_count` / `_sum` / 各 `_bucket` 若分多次加锁读取，
    /// 并发 `observe()` 会在两次加锁之间追加观测值，产出「`_count` 比 `+Inf` 桶还大」
    /// 这类自相矛盾的抓取输出。Prometheus 不会因此报错，但分位数与 rate 计算会失真，
    /// 且这类偏差只在高并发下偶发，极难排查。
    ///
    /// `bounds` 必须**严格升序**（`partition_point` 的二分前提）。非有限观测值按
    /// Prometheus 语义落位：`NaN` 与 `+Inf` 不计入任何有限桶（只体现在 `+Inf` 桶），
    /// `-Inf` 计入所有桶。
    ///
    /// 单趟 O(n log b)：每个观测值先挂到「第一个 >= 它的桶」，再前缀和还原累积计数。
    /// 既避免 O(n*b) 的全量扫桶，也避免在抓取路径上对观测值排序。
    fn snapshot(&self, bounds: &[f64]) -> HistogramSnapshot {
        let values = self.values.lock();
        let largest = bounds.last().copied();
        let mut deltas = vec![0u64; bounds.len()];
        let mut sum = 0.0f64;

        for &value in values.iter() {
            sum += value;
            // `NaN` 比较恒为 false，`+Inf > largest`，二者都自然落在 `+Inf` 桶里。
            if let Some(largest) = largest {
                if value <= largest {
                    deltas[bounds.partition_point(|bound| *bound < value)] += 1;
                }
            }
        }

        let mut running = 0u64;
        for delta in &mut deltas {
            running += *delta;
            *delta = running;
        }

        HistogramSnapshot {
            count: values.len() as u64,
            sum: normalize_negative_zero(sum),
            // 空直方图的求和落在 `-0.0`（`Sum` 的加法单位元），必须归一。
            cumulative: deltas,
        }
    }

    /// Resets to its initial state.
    pub fn reset(&self) {
        let mut values = self.values.lock();
        values.clear();
    }
}

/// Represents MetricsCollector.
pub struct MetricsCollector {
    counters: Arc<parking_lot::Mutex<HashMap<String, Counter>>>,
    gauges: Arc<parking_lot::Mutex<HashMap<String, Gauge>>>,
    histograms: Arc<parking_lot::Mutex<HashMap<String, Histogram>>>,
}

#[derive(Debug, Clone, Copy)]
/// Represents MetricInventory.
pub struct MetricInventory {
    /// `total_counters` field.
    pub total_counters: usize,
    /// `total_gauges` field.
    pub total_gauges: usize,
    /// `total_histograms` field.
    pub total_histograms: usize,
}

/// 归一 IEEE 754 负零：`-0.0 + 0.0 == +0.0`，其它取值（含 NaN、正负无穷）不变。
///
/// 必要性：`f64` 的 `Sum` 实现以 `-0.0` 为加法单位元，因此**空**直方图的
/// `get_sum()` 返回 `-0.0`，直接渲染就是 `..._sum -0`——语法上 Prometheus 能解析，
/// 但毫无意义，而且看起来像渲染 bug，会误导排障。
fn normalize_negative_zero(value: f64) -> f64 {
    value + 0.0
}

/// 按 Prometheus 文本格式转义标签值：`\` → `\\`、`"` → `\"`、换行 → `\n`。
///
/// 未转义的 `"` 会把整条样本写坏，而 Prometheus 遇到无法解析的样本会丢弃
/// **整个** scrape（不只是这一条），所以这是必须堵的口子，不是美观问题。
fn push_label_value(output: &mut String, value: &str) {
    for character in value.chars() {
        match character {
            '\\' => output.push_str("\\\\"),
            '"' => output.push_str("\\\""),
            '\n' => output.push_str("\\n"),
            _ => output.push(character),
        }
    }
}

/// 把一组标签渲染成 `{k="v",...}`；无标签且无 `bound` 时返回空字符串（调用方据此省略花括号）。
///
/// - 标签按 key 排序：`HashMap` 的迭代序每个进程都不同，排序后输出才稳定可比。
/// - `le` 由 `bound` 传入并**固定排在最后**；若标签集合里本就带 `le`，会被剔除，
///   避免出现重复标签名——重复标签同样会让该条样本被 Prometheus 拒收。
fn render_labels(labels: &HashMap<String, String>, bound: Option<&str>) -> String {
    let mut keys: Vec<&str> = labels.keys().map(String::as_str).filter(|key| *key != "le").collect();
    keys.sort_unstable();

    if keys.is_empty() && bound.is_none() {
        return String::new();
    }

    let mut output = String::with_capacity(2 + keys.len() * 16 + bound.map_or(0, str::len));
    output.push('{');
    for (index, key) in keys.iter().enumerate() {
        if index > 0 {
            output.push(',');
        }
        output.push_str(key);
        output.push_str("=\"");
        push_label_value(&mut output, labels.get(*key).map_or("", String::as_str));
        output.push('"');
    }
    if let Some(bound) = bound {
        if !keys.is_empty() {
            output.push(',');
        }
        output.push_str("le=\"");
        push_label_value(&mut output, bound);
        output.push('"');
    }
    output.push('}');
    output
}

impl MetricsCollector {
    /// Constructs a new instance.
    pub fn new() -> Self {
        Self {
            counters: Arc::new(parking_lot::Mutex::new(HashMap::new())),
            gauges: Arc::new(parking_lot::Mutex::new(HashMap::new())),
            histograms: Arc::new(parking_lot::Mutex::new(HashMap::new())),
        }
    }

    /// Registers a new counter.
    pub fn register_counter(&self, name: String) -> Counter {
        let counter = Counter::new(name.clone());
        let mut counters = self.counters.lock();
        counters.insert(name, counter.clone());
        counter
    }

    /// Registers a new labeled counter.
    pub fn register_counter_with_labels(&self, name: String, labels: HashMap<String, String>) -> Counter {
        let counter = Counter::with_labels(name.clone(), labels);
        let mut counters = self.counters.lock();
        counters.insert(name, counter.clone());
        counter
    }

    /// Registers a new gauge.
    pub fn register_gauge(&self, name: String) -> Gauge {
        let gauge = Gauge::new(name.clone());
        let mut gauges = self.gauges.lock();
        gauges.insert(name, gauge.clone());
        gauge
    }

    /// Registers a new labeled gauge.
    pub fn register_gauge_with_labels(&self, name: String, labels: HashMap<String, String>) -> Gauge {
        let gauge = Gauge::with_labels(name.clone(), labels);
        let mut gauges = self.gauges.lock();
        gauges.insert(name, gauge.clone());
        gauge
    }

    /// Registers a new histogram.
    pub fn register_histogram(&self, name: String) -> Histogram {
        let histogram = Histogram::new(name.clone());
        let mut histograms = self.histograms.lock();
        histograms.insert(name, histogram.clone());
        histogram
    }

    /// Registers a new labeled histogram.
    pub fn register_histogram_with_labels(&self, name: String, labels: HashMap<String, String>) -> Histogram {
        let histogram = Histogram::with_labels(name.clone(), labels);
        let mut histograms = self.histograms.lock();
        histograms.insert(name, histogram.clone());
        histogram
    }

    /// Returns the counter with the given name.
    pub fn get_counter(&self, name: &str) -> Option<Counter> {
        let counters = self.counters.lock();
        counters.get(name).cloned()
    }

    /// Returns the gauge with the given name.
    pub fn get_gauge(&self, name: &str) -> Option<Gauge> {
        let gauges = self.gauges.lock();
        gauges.get(name).cloned()
    }

    /// Returns the histogram with the given name.
    pub fn get_histogram(&self, name: &str) -> Option<Histogram> {
        let histograms = self.histograms.lock();
        histograms.get(name).cloned()
    }

    /// Returns a snapshot of all registered metrics.
    pub fn collect_metrics(&self) -> Vec<Metric> {
        let mut metrics = Vec::new();

        let counters = self.counters.lock();
        for counter in counters.values() {
            metrics.push(Metric {
                name: counter.name.clone(),
                value: counter.get() as f64,
                timestamp: Instant::now(),
                labels: counter.labels.clone(),
            });
        }

        let gauges = self.gauges.lock();
        for gauge in gauges.values() {
            metrics.push(Metric {
                name: gauge.name.clone(),
                value: gauge.get(),
                timestamp: Instant::now(),
                labels: gauge.labels.clone(),
            });
        }

        let histograms = self.histograms.lock();
        for histogram in histograms.values() {
            metrics.push(Metric {
                name: format!("{}_count", histogram.name),
                value: histogram.get_count() as f64,
                timestamp: Instant::now(),
                labels: histogram.labels.clone(),
            });
            metrics.push(Metric {
                name: format!("{}_sum", histogram.name),
                value: histogram.get_sum(),
                timestamp: Instant::now(),
                labels: histogram.labels.clone(),
            });
            metrics.push(Metric {
                name: format!("{}_avg", histogram.name),
                value: histogram.get_avg(),
                timestamp: Instant::now(),
                labels: histogram.labels.clone(),
            });
        }

        metrics
    }

    /// Returns a summary of registered metric counts.
    pub fn inventory(&self) -> MetricInventory {
        MetricInventory {
            total_counters: self.counters.lock().len(),
            total_gauges: self.gauges.lock().len(),
            total_histograms: self.histograms.lock().len(),
        }
    }

    /// Renders the metrics in Prometheus exposition format.
    ///
    /// 直方图按 **Prometheus 原生 histogram** 输出：先 `# TYPE <name> histogram`，
    /// 随后是升序的 `<name>_bucket{le="<上界>"}` 累积计数、`<name>_bucket{le="+Inf"}`、
    /// `<name>_sum`、`<name>_count`。缺少 `_bucket` 系列时 `histogram_quantile()`
    /// 无数据可算——规则里的 `rate(*_bucket[5m])` 会永远是空的。
    ///
    /// 各指标族按名字排序输出，使同一次运行内多次抓取的文本稳定可比。
    pub fn to_prometheus_format(&self) -> String {
        let mut output = String::with_capacity(4096);

        {
            let counters = self.counters.lock();
            let mut sorted: Vec<&Counter> = counters.values().collect();
            sorted.sort_unstable_by(|left, right| left.name.cmp(&right.name));
            for counter in sorted {
                output.push_str(&format!("# HELP {} {}\n", counter.name, counter.name));
                output.push_str(&format!("# TYPE {} counter\n", counter.name));
                output.push_str(&format!(
                    "{}{} {}\n",
                    counter.name,
                    render_labels(&counter.labels, None),
                    counter.get()
                ));
            }
        }

        {
            let gauges = self.gauges.lock();
            let mut sorted: Vec<&Gauge> = gauges.values().collect();
            sorted.sort_unstable_by(|left, right| left.name.cmp(&right.name));
            for gauge in sorted {
                output.push_str(&format!("# HELP {} {}\n", gauge.name, gauge.name));
                output.push_str(&format!("# TYPE {} gauge\n", gauge.name));
                output.push_str(&format!(
                    "{}{} {}\n",
                    gauge.name,
                    render_labels(&gauge.labels, None),
                    normalize_negative_zero(gauge.get())
                ));
            }
        }

        {
            let histograms = self.histograms.lock();
            let mut sorted: Vec<&Histogram> = histograms.values().collect();
            sorted.sort_unstable_by(|left, right| left.name.cmp(&right.name));
            for histogram in sorted {
                let base = &histogram.name;
                let bounds = histogram_buckets_for(base);
                // 单次加锁取全：count / sum / 各桶必须来自同一时刻。
                let snapshot = histogram.snapshot(bounds);

                output.push_str(&format!("# HELP {base} {base}\n"));
                output.push_str(&format!("# TYPE {base} histogram\n"));

                let bound_labels: Vec<String> = bounds.iter().map(f64::to_string).collect();
                for (bound_label, bucket_count) in bound_labels.iter().zip(snapshot.cumulative.iter()) {
                    let labels = render_labels(&histogram.labels, Some(bound_label.as_str()));
                    output.push_str(&format!("{base}_bucket{labels} {bucket_count}\n"));
                }
                let inf_labels = render_labels(&histogram.labels, Some("+Inf"));
                output.push_str(&format!("{base}_bucket{inf_labels} {}\n", snapshot.count));

                let labels = render_labels(&histogram.labels, None);
                output.push_str(&format!("{base}_sum{labels} {}\n", snapshot.sum));
                output.push_str(&format!("{base}_count{labels} {}\n", snapshot.count));
            }
        }

        output
    }
}

impl Default for MetricsCollector {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_counter() {
        let counter = Counter::new("test_counter".to_string());
        assert_eq!(counter.get(), 0);
        counter.inc();
        assert_eq!(counter.get(), 1);
        counter.inc_by(5);
        assert_eq!(counter.get(), 6);
        counter.reset();
        assert_eq!(counter.get(), 0);
    }

    #[test]
    fn test_counter_with_labels() {
        let mut labels = HashMap::new();
        labels.insert("method".to_string(), "GET".to_string());
        let counter = Counter::with_labels("test_counter".to_string(), labels);
        counter.inc();
        assert_eq!(counter.get(), 1);
    }

    #[test]
    fn test_gauge() {
        let gauge = Gauge::new("test_gauge".to_string());
        assert_eq!(gauge.get(), 0.0);
        gauge.set(42.0);
        assert_eq!(gauge.get(), 42.0);
        gauge.inc();
        assert_eq!(gauge.get(), 43.0);
        gauge.dec();
        assert_eq!(gauge.get(), 42.0);
        gauge.add(10.0);
        assert_eq!(gauge.get(), 52.0);
        gauge.sub(2.0);
        assert_eq!(gauge.get(), 50.0);
    }

    #[test]
    fn test_histogram() {
        let histogram = Histogram::new("test_histogram".to_string());
        histogram.observe(1.0);
        histogram.observe(2.0);
        histogram.observe(3.0);
        assert_eq!(histogram.get_count(), 3);
        assert_eq!(histogram.get_sum(), 6.0);
        assert_eq!(histogram.get_avg(), 2.0);
        assert_eq!(histogram.get_percentile(50.0).unwrap(), 2.0);
    }

    #[test]
    fn test_metrics_collector() {
        let collector = MetricsCollector::new();
        let counter = collector.register_counter("test_counter".to_string());
        counter.inc();
        assert_eq!(collector.get_counter("test_counter").unwrap().get(), 1);

        let gauge = collector.register_gauge("test_gauge".to_string());
        gauge.set(42.0);
        assert_eq!(collector.get_gauge("test_gauge").unwrap().get(), 42.0);

        let histogram = collector.register_histogram("test_histogram".to_string());
        histogram.observe(1.0);
        assert_eq!(collector.get_histogram("test_histogram").unwrap().get_count(), 1);

        let metrics = collector.collect_metrics();
        assert!(!metrics.is_empty());
    }

    #[test]
    fn test_gauge_with_labels() {
        let mut labels = HashMap::new();
        labels.insert("method".to_string(), "GET".to_string());
        let gauge = Gauge::with_labels("test_gauge".to_string(), labels);
        assert_eq!(gauge.get(), 0.0);
        gauge.set(42.0);
        assert_eq!(gauge.get(), 42.0);
    }

    #[test]
    fn test_histogram_with_labels() {
        let mut labels = HashMap::new();
        labels.insert("endpoint".to_string(), "/api/v1".to_string());
        let histogram = Histogram::with_labels("test_histogram".to_string(), labels);
        histogram.observe(1.5);
        assert_eq!(histogram.get_count(), 1);
        assert_eq!(histogram.get_sum(), 1.5);
    }

    #[test]
    fn test_histogram_get_values_returns_all_observed() {
        let histogram = Histogram::new("test_histogram".to_string());
        histogram.observe(1.0);
        histogram.observe(2.0);
        histogram.observe(3.0);
        let values = histogram.get_values();
        assert_eq!(values.len(), 3);
        assert!(values.contains(&1.0));
        assert!(values.contains(&2.0));
        assert!(values.contains(&3.0));
    }

    #[test]
    fn test_histogram_get_percentile_empty_returns_zero() {
        let histogram = Histogram::new("test_histogram".to_string());
        // Empty histogram should return Ok(0.0) rather than error.
        let result = histogram.get_percentile(50.0).expect("percentile should succeed");
        assert_eq!(result, 0.0);
    }

    #[test]
    fn test_histogram_get_percentile_specific_percentiles() {
        let histogram = Histogram::new("test_histogram".to_string());
        for v in [1.0, 2.0, 3.0, 4.0, 5.0] {
            histogram.observe(v);
        }

        assert_eq!(histogram.get_percentile(0.0).unwrap(), 1.0);
        assert_eq!(histogram.get_percentile(100.0).unwrap(), 5.0);
        assert_eq!(histogram.get_percentile(50.0).unwrap(), 3.0);
    }

    #[test]
    fn test_histogram_reset_clears_all_values() {
        let histogram = Histogram::new("test_histogram".to_string());
        histogram.observe(1.0);
        histogram.observe(2.0);
        assert_eq!(histogram.get_count(), 2);

        histogram.reset();
        assert_eq!(histogram.get_count(), 0);
        assert_eq!(histogram.get_sum(), 0.0);
        assert_eq!(histogram.get_avg(), 0.0);
    }

    #[test]
    fn test_histogram_get_avg_empty_returns_zero() {
        let histogram = Histogram::new("test_histogram".to_string());
        // Empty histogram should return 0.0 average (avoids division by zero).
        assert_eq!(histogram.get_avg(), 0.0);
    }

    #[test]
    fn test_counter_with_labels_registered_via_collector() {
        let collector = MetricsCollector::new();
        let mut labels = HashMap::new();
        labels.insert("method".to_string(), "POST".to_string());
        let counter = collector.register_counter_with_labels("labeled_counter".to_string(), labels);
        counter.inc_by(3);
        assert_eq!(collector.get_counter("labeled_counter").unwrap().get(), 3);
    }

    #[test]
    fn test_register_gauge_with_labels_via_collector() {
        let collector = MetricsCollector::new();
        let mut labels = HashMap::new();
        labels.insert("env".to_string(), "prod".to_string());
        let gauge = collector.register_gauge_with_labels("labeled_gauge".to_string(), labels);
        gauge.set(99.5);
        assert_eq!(collector.get_gauge("labeled_gauge").unwrap().get(), 99.5);
    }

    #[test]
    fn test_register_histogram_with_labels_via_collector() {
        let collector = MetricsCollector::new();
        let mut labels = HashMap::new();
        labels.insert("unit".to_string(), "ms".to_string());
        let histogram = collector.register_histogram_with_labels("labeled_histogram".to_string(), labels);
        histogram.observe(42.0);
        assert_eq!(collector.get_histogram("labeled_histogram").unwrap().get_count(), 1);
    }

    #[test]
    fn test_get_counter_returns_none_when_not_registered() {
        let collector = MetricsCollector::new();
        assert!(collector.get_counter("nonexistent").is_none());
    }

    #[test]
    fn test_get_gauge_returns_none_when_not_registered() {
        let collector = MetricsCollector::new();
        assert!(collector.get_gauge("nonexistent").is_none());
    }

    #[test]
    fn test_get_histogram_returns_none_when_not_registered() {
        let collector = MetricsCollector::new();
        assert!(collector.get_histogram("nonexistent").is_none());
    }

    #[test]
    fn test_collect_metrics_includes_counters_gauges_and_histograms() {
        let collector = MetricsCollector::new();
        collector.register_counter("c1".to_string()).inc();
        collector.register_gauge("g1".to_string()).set(5.0);
        let histogram = collector.register_histogram("h1".to_string());
        histogram.observe(10.0);

        let metrics = collector.collect_metrics();

        // Counter (1) + Gauge (1) + Histogram metrics (count + sum + avg = 3) = 5 total.
        assert_eq!(metrics.len(), 5);

        // Verify histogram-derived metrics are emitted with proper suffixes.
        let names: Vec<String> = metrics.iter().map(|m| m.name.clone()).collect();
        assert!(names.contains(&"c1".to_string()));
        assert!(names.contains(&"g1".to_string()));
        assert!(names.contains(&"h1_count".to_string()));
        assert!(names.contains(&"h1_sum".to_string()));
        assert!(names.contains(&"h1_avg".to_string()));
    }

    #[test]
    fn test_collect_metrics_empty_returns_empty_vec() {
        let collector = MetricsCollector::new();
        let metrics = collector.collect_metrics();
        assert!(metrics.is_empty());
    }

    #[test]
    fn test_inventory_returns_counts() {
        let collector = MetricsCollector::new();
        collector.register_counter("c1".to_string());
        collector.register_counter("c2".to_string());
        collector.register_gauge("g1".to_string());
        collector.register_histogram("h1".to_string());

        let inventory = collector.inventory();
        assert_eq!(inventory.total_counters, 2);
        assert_eq!(inventory.total_gauges, 1);
        assert_eq!(inventory.total_histograms, 1);
    }

    #[test]
    fn test_inventory_empty_returns_zero() {
        let collector = MetricsCollector::new();
        let inventory = collector.inventory();
        assert_eq!(inventory.total_counters, 0);
        assert_eq!(inventory.total_gauges, 0);
        assert_eq!(inventory.total_histograms, 0);
    }

    #[test]
    fn test_default_for_metrics_collector() {
        let collector = MetricsCollector::default();
        let inventory = collector.inventory();
        assert_eq!(inventory.total_counters, 0);
    }

    #[test]
    fn test_to_prometheus_format_includes_counter_help_and_type() {
        let collector = MetricsCollector::new();
        collector.register_counter("requests".to_string()).inc_by(5);

        let output = collector.to_prometheus_format();
        assert!(output.contains("# HELP requests"));
        assert!(output.contains("# TYPE requests counter"));
        assert!(output.contains("requests 5"));
    }

    #[test]
    fn test_to_prometheus_format_includes_gauge_help_and_type() {
        let collector = MetricsCollector::new();
        collector.register_gauge("temperature".to_string()).set(23.5);

        let output = collector.to_prometheus_format();
        assert!(output.contains("# HELP temperature"));
        assert!(output.contains("# TYPE temperature gauge"));
        assert!(output.contains("temperature 23.5"));
    }

    #[test]
    fn test_to_prometheus_format_includes_histogram_count_and_sum() {
        let collector = MetricsCollector::new();
        let histogram = collector.register_histogram("latency".to_string());
        histogram.observe(10.0);
        histogram.observe(20.0);

        let output = collector.to_prometheus_format();
        assert!(output.contains("latency_count 2"));
        assert!(output.contains("latency_sum 30"));
        // 直方图是**一个**指标族：TYPE 声明挂在族名上（histogram），
        // 而不是把 _count / _sum 拆成两个独立的 counter。
        assert!(output.contains("# TYPE latency histogram"), "{output}");
        assert!(!output.contains("# TYPE latency_count counter"));
        assert!(!output.contains("# TYPE latency_sum counter"));
        assert!(output.contains("latency_bucket{le=\"+Inf\"} 2"), "{output}");
    }

    #[test]
    fn test_to_prometheus_format_with_labeled_counter() {
        let collector = MetricsCollector::new();
        let mut labels = HashMap::new();
        labels.insert("method".to_string(), "GET".to_string());
        collector.register_counter_with_labels("http_requests".to_string(), labels).inc_by(10);

        let output = collector.to_prometheus_format();
        assert!(output.contains("http_requests{method=\"GET\"} 10"));
    }

    #[test]
    fn test_to_prometheus_format_with_labeled_gauge() {
        let collector = MetricsCollector::new();
        let mut labels = HashMap::new();
        labels.insert("env".to_string(), "prod".to_string());
        collector.register_gauge_with_labels("active_users".to_string(), labels).set(500.0);

        let output = collector.to_prometheus_format();
        assert!(output.contains("active_users{env=\"prod\"} 500"));
    }

    #[test]
    fn test_to_prometheus_format_with_labeled_histogram() {
        let collector = MetricsCollector::new();
        let mut labels = HashMap::new();
        labels.insert("unit".to_string(), "ms".to_string());
        let histogram = collector.register_histogram_with_labels("duration".to_string(), labels);
        histogram.observe(50.0);

        let output = collector.to_prometheus_format();
        assert!(output.contains("duration_count{unit=\"ms\"} 1"));
        assert!(output.contains("duration_sum{unit=\"ms\"} 50"));
    }

    #[test]
    fn test_to_prometheus_format_empty_returns_empty_string() {
        let collector = MetricsCollector::new();
        let output = collector.to_prometheus_format();
        assert!(output.is_empty());
    }

    #[test]
    fn test_counter_inc_by_large_delta() {
        let counter = Counter::new("test".to_string());
        counter.inc_by(u64::MAX);
        assert_eq!(counter.get(), u64::MAX);
        // Overflow wraps to 0 with fetch_add (Relaxed).
        counter.inc();
        assert_eq!(counter.get(), 0);
    }

    #[test]
    fn test_gauge_subtraction_can_go_negative() {
        let gauge = Gauge::new("test".to_string());
        gauge.set(5.0);
        gauge.sub(10.0);
        assert_eq!(gauge.get(), -5.0);
    }

    #[test]
    fn test_gauge_concurrent_additions_are_atomic() {
        use std::thread;
        let gauge = Gauge::new("test".to_string());
        let gauge_clone = gauge.clone();
        let handles: Vec<_> = (0..4)
            .map(|_| {
                let g = gauge_clone.clone();
                thread::spawn(move || {
                    for _ in 0..1000 {
                        g.add(1.0);
                    }
                })
            })
            .collect();
        for h in handles {
            h.join().unwrap();
        }
        assert_eq!(gauge.get(), 4000.0);
    }

    #[test]
    fn test_metric_struct_construction() {
        let mut labels = HashMap::new();
        labels.insert("key".to_string(), "value".to_string());
        let metric =
            Metric { name: "test_metric".to_string(), value: 42.0, timestamp: Instant::now(), labels: labels.clone() };
        assert_eq!(metric.name, "test_metric");
        assert_eq!(metric.value, 42.0);
        assert_eq!(metric.labels, labels);
    }

    #[test]
    fn test_metric_inventory_struct_debug() {
        let inventory = MetricInventory { total_counters: 1, total_gauges: 2, total_histograms: 3 };
        let debug = format!("{inventory:?}");
        assert!(debug.contains("MetricInventory"));
        assert!(debug.contains("1"));
        assert!(debug.contains("2"));
        assert!(debug.contains("3"));
    }

    #[test]
    fn test_histogram_bucket_bounds_are_strictly_ascending() {
        // `cumulative_counts` 用 partition_point 二分定位，升序是硬前提；
        // 乱序不会 panic，只会静默算错每个桶——所以这里钉死。
        for bounds in [HISTOGRAM_BUCKETS_MS, HISTOGRAM_BUCKETS_SECONDS] {
            assert!(bounds.windows(2).all(|pair| pair[0] < pair[1]), "分桶上界必须严格升序: {bounds:?}");
        }
    }

    #[test]
    fn test_histogram_buckets_selected_by_name_suffix() {
        assert_eq!(histogram_buckets_for("db_query_duration_ms"), HISTOGRAM_BUCKETS_MS);
        assert_eq!(histogram_buckets_for("auth_login_duration_seconds"), HISTOGRAM_BUCKETS_SECONDS);
        // 无单位后缀者（含测试用名）退回毫秒表。
        assert_eq!(histogram_buckets_for("latency"), HISTOGRAM_BUCKETS_MS);
    }

    #[test]
    fn test_to_prometheus_format_histogram_buckets_are_cumulative() {
        let collector = MetricsCollector::new();
        let histogram = collector.register_histogram("latency_ms".to_string());
        for value in [0.5, 3.0, 30.0, 300.0, 3000.0] {
            histogram.observe(value);
        }

        let output = collector.to_prometheus_format();

        assert!(output.contains("# TYPE latency_ms histogram"), "{output}");
        // 累积语义：每个桶是「v <= le」的累计数，不是区间计数。
        assert!(output.contains("latency_ms_bucket{le=\"1\"} 1"), "{output}");
        assert!(output.contains("latency_ms_bucket{le=\"2.5\"} 1"), "{output}");
        assert!(output.contains("latency_ms_bucket{le=\"5\"} 2"), "{output}");
        assert!(output.contains("latency_ms_bucket{le=\"50\"} 3"), "{output}");
        assert!(output.contains("latency_ms_bucket{le=\"500\"} 4"), "{output}");
        assert!(output.contains("latency_ms_bucket{le=\"5000\"} 5"), "{output}");
        assert!(output.contains("latency_ms_bucket{le=\"+Inf\"} 5"), "{output}");
        assert!(output.contains("latency_ms_count 5"), "{output}");
        assert!(output.contains("latency_ms_sum 3333.5"), "{output}");
    }

    #[test]
    fn test_to_prometheus_format_histogram_buckets_never_decrease() {
        // 累积不变量：按输出顺序解析 le 计数，必须单调不减，且 +Inf == 观测总数。
        let collector = MetricsCollector::new();
        let histogram = collector.register_histogram("mono_ms".to_string());
        for value in [0.1, 1.0, 7.0, 42.0, 999.0] {
            histogram.observe(value);
        }

        let output = collector.to_prometheus_format();
        let counts: Vec<u64> = output
            .lines()
            .filter_map(|line| line.strip_prefix("mono_ms_bucket{"))
            .filter_map(|rest| rest.rsplit_once('}'))
            .filter_map(|(_, value)| value.trim().parse::<u64>().ok())
            .collect();

        assert_eq!(counts.len(), HISTOGRAM_BUCKETS_MS.len() + 1, "应含全部有限桶 + +Inf 桶");
        assert!(counts.windows(2).all(|pair| pair[0] <= pair[1]), "桶计数必须单调不减: {counts:?}");
        assert_eq!(counts.last().copied(), Some(5), "+Inf 桶应等于观测总数");
    }

    #[test]
    fn test_to_prometheus_format_seconds_histogram_uses_seconds_buckets() {
        let collector = MetricsCollector::new();
        let histogram = collector.register_histogram("auth_login_duration_seconds".to_string());
        histogram.observe(0.05);

        let output = collector.to_prometheus_format();
        assert!(output.contains("auth_login_duration_seconds_bucket{le=\"0.005\"} 0"), "{output}");
        assert!(output.contains("auth_login_duration_seconds_bucket{le=\"0.025\"} 0"), "{output}");
        assert!(output.contains("auth_login_duration_seconds_bucket{le=\"0.05\"} 1"), "{output}");
    }

    #[test]
    fn test_to_prometheus_format_empty_histogram_still_exposes_buckets() {
        // 未被观测过的直方图也必须铺满 le 系列，否则 histogram_quantile 连序列都取不到。
        let collector = MetricsCollector::new();
        collector.register_histogram("idle_ms".to_string());

        let output = collector.to_prometheus_format();
        assert!(output.contains("# TYPE idle_ms histogram"), "{output}");
        for bound in HISTOGRAM_BUCKETS_MS {
            let expected = format!("idle_ms_bucket{{le=\"{bound}\"}} 0");
            assert!(output.contains(&expected), "缺少零值桶: {expected}\n{output}");
        }
        assert!(output.contains("idle_ms_bucket{le=\"+Inf\"} 0"), "{output}");
        assert!(output.contains("idle_ms_count 0"), "{output}");
        assert!(output.contains("idle_ms_sum 0"), "{output}");
        // 空直方图的 sum 在 f64 上是 `-0.0`，必须归一，否则渲染出 `-0`。
        assert!(!output.contains("idle_ms_sum -0"), "负零必须归一:\n{output}");
    }

    #[test]
    fn test_to_prometheus_format_histogram_bucket_merges_metric_labels() {
        let collector = MetricsCollector::new();
        let mut labels = HashMap::new();
        labels.insert("unit".to_string(), "ms".to_string());
        let histogram = collector.register_histogram_with_labels("db_query_duration_ms".to_string(), labels);
        histogram.observe(50.0);

        let output = collector.to_prometheus_format();
        assert!(output.contains("db_query_duration_ms_bucket{unit=\"ms\",le=\"1\"} 0"), "{output}");
        assert!(output.contains("db_query_duration_ms_bucket{unit=\"ms\",le=\"50\"} 1"), "{output}");
        assert!(output.contains("db_query_duration_ms_bucket{unit=\"ms\",le=\"+Inf\"} 1"), "{output}");
        assert!(output.contains("db_query_duration_ms_sum{unit=\"ms\"} 50"), "{output}");
        assert!(output.contains("db_query_duration_ms_count{unit=\"ms\"} 1"), "{output}");
    }

    #[test]
    fn test_to_prometheus_format_drops_user_supplied_le_label() {
        // 重复标签名会让该条样本被 Prometheus 拒收，必须由渲染层剔除用户传入的 le。
        let collector = MetricsCollector::new();
        let mut labels = HashMap::new();
        labels.insert("le".to_string(), "999".to_string());
        let histogram = collector.register_histogram_with_labels("weird_ms".to_string(), labels);
        histogram.observe(5.0);

        let output = collector.to_prometheus_format();
        assert!(!output.contains("le=\"999\""), "用户传入的 le 必须被剔除:\n{output}");
        assert!(output.contains("weird_ms_bucket{le=\"5\"} 1"), "{output}");
    }

    #[test]
    fn test_to_prometheus_format_escapes_label_values() {
        // 未转义的 `"` 会把整条样本写坏，而 Prometheus 会因此丢弃整个 scrape。
        let collector = MetricsCollector::new();
        let mut labels = HashMap::new();
        labels.insert("route".to_string(), "/a\"b\\c".to_string());
        collector.register_counter_with_labels("escaped_total".to_string(), labels).inc();

        let output = collector.to_prometheus_format();
        assert!(output.contains("escaped_total{route=\"/a\\\"b\\\\c\"} 1"), "{output}");
    }

    #[test]
    fn test_to_prometheus_format_orders_metric_families_by_name() {
        // HashMap 迭代序每个进程都不同 → 输出必须排序才稳定可比。
        // 排序范围是「同一族内部」；族与族之间保持 counter → gauge → histogram 的固定分块。
        let collector = MetricsCollector::new();
        collector.register_counter("zeta_total".to_string()).inc();
        collector.register_counter("alpha_total".to_string()).inc();
        collector.register_gauge("beta".to_string()).set(1.0);

        let output = collector.to_prometheus_format();
        let alpha = output.find("# TYPE alpha_total counter").expect("alpha 应存在");
        let zeta = output.find("# TYPE zeta_total counter").expect("zeta 应存在");
        let beta = output.find("# TYPE beta gauge").expect("beta 应存在");

        assert!(alpha < zeta, "同族内应按名字升序:\n{output}");
        assert!(zeta < beta, "counter 块应整体排在 gauge 块之前:\n{output}");
    }

    #[test]
    fn test_snapshot_ignores_non_finite_observations() {
        // NaN / +Inf 只落 +Inf 桶，不污染有限桶（Prometheus 官方语义）。
        let histogram = Histogram::new("edge_ms".to_string());
        histogram.observe(1.0);
        histogram.observe(f64::NAN);
        histogram.observe(f64::INFINITY);

        let snapshot = histogram.snapshot(&[1.0, 10.0]);
        assert_eq!(snapshot.cumulative, vec![1, 1]);
        assert_eq!(snapshot.count, 3, "count 含 NaN/+Inf，+Inf 桶才能与它相等");
    }

    #[test]
    fn test_snapshot_returns_empty_buckets_for_empty_bounds() {
        let histogram = Histogram::new("edge_ms".to_string());
        histogram.observe(1.0);
        let snapshot = histogram.snapshot(&[]);
        assert!(snapshot.cumulative.is_empty());
        assert_eq!(snapshot.count, 1);
        assert_eq!(snapshot.sum, 1.0);
    }

    #[test]
    fn test_snapshot_sum_and_count_are_consistent_with_last_bucket() {
        // 不变量：_count == +Inf 桶；单次加锁保证三者同刻。
        let histogram = Histogram::new("consistent_ms".to_string());
        for value in [3.0, 700.0, 9000.0] {
            histogram.observe(value);
        }
        let bounds = histogram_buckets_for("consistent_ms");
        let snapshot = histogram.snapshot(bounds);
        assert_eq!(snapshot.count, 3);
        assert_eq!(snapshot.sum, 9703.0);
        assert_eq!(snapshot.cumulative.last().copied(), Some(3), "+Inf 桶必须等于 count");
    }
}
