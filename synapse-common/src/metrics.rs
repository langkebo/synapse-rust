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

/// 单个直方图保留的未折叠观测值上限。
///
/// 达到上限即把已有观测折叠进分桶计数后清空 live 缓冲，使每个直方图的内存
/// 占用有界（O(分桶数) 而非 O(观测次数)）。此前 `values: Vec<f64>` 只增不减，
/// 是长跑实例里唯一随运行时长单调增长的内存放大点。
const HISTOGRAM_MAX_LIVE_SAMPLES: usize = 4096;

/// 直方图的内部状态，由单个互斥锁保护，保证 `count` / `sum` / 分桶来自同一时刻。
#[derive(Debug)]
struct HistogramData {
    /// 尚未折叠的观测值；长度恒 `<= HISTOGRAM_MAX_LIVE_SAMPLES`。
    live: Vec<f64>,
    /// 已折叠观测按 `Histogram::bounds` 的**区间计数**（非累积）；长度 == bounds.len()。
    folded_buckets: Vec<u64>,
    /// 观测总数（含已折叠与 live），用于 `_count` 与 `+Inf` 桶。
    count: u64,
    /// 观测值之和（含已折叠与 live），用于 `_sum`。
    sum: f64,
}

#[derive(Debug, Clone)]
/// Represents Histogram.
pub struct Histogram {
    name: String,
    /// 由 `name` 推断并固定的分桶上界；折叠时按它归桶。
    bounds: &'static [f64],
    data: Arc<parking_lot::Mutex<HistogramData>>,
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
        let bounds = histogram_buckets_for(&name);
        Self {
            name,
            bounds,
            data: Arc::new(parking_lot::Mutex::new(HistogramData {
                live: Vec::new(),
                folded_buckets: vec![0; bounds.len()],
                count: 0,
                sum: 0.0,
            })),
            labels: HashMap::new(),
        }
    }

    /// Constructs a new instance with labels attached.
    pub fn with_labels(name: String, labels: HashMap<String, String>) -> Self {
        let bounds = histogram_buckets_for(&name);
        Self {
            name,
            bounds,
            data: Arc::new(parking_lot::Mutex::new(HistogramData {
                live: Vec::new(),
                folded_buckets: vec![0; bounds.len()],
                count: 0,
                sum: 0.0,
            })),
            labels,
        }
    }

    /// Records a new observed value.
    ///
    /// `count` / `sum` 单调累加；`live` 达上限时把已有观测折叠进分桶计数并清空，
    /// 从而把内存占用限制在 `HISTOGRAM_MAX_LIVE_SAMPLES` 以内。
    pub fn observe(&self, value: f64) {
        let mut data = self.data.lock();
        data.live.push(value);
        data.count += 1;
        data.sum += value;
        if data.live.len() >= HISTOGRAM_MAX_LIVE_SAMPLES {
            Self::fold_live(self.bounds, &mut data);
        }
    }

    /// 把 `data.live` 按 `bounds` 归入区间计数后清空；`count` / `sum` 不受影响。
    fn fold_live(bounds: &[f64], data: &mut HistogramData) {
        let largest = bounds.last().copied();
        for &value in &data.live {
            // NaN 比较恒 false、`+Inf > largest`，二者都不计入任何有限桶。
            if let Some(largest) = largest {
                if value <= largest {
                    data.folded_buckets[bounds.partition_point(|bound| *bound < value)] += 1;
                }
            }
        }
        data.live.clear();
    }

    /// Returns the live (not yet folded) observations.
    ///
    /// 这是一个**有界窗口**（最多 `HISTOGRAM_MAX_LIVE_SAMPLES` 个），不保证包含
    /// 全部历史观测；供诊断/测试使用，不用于生产渲染。
    pub fn get_values(&self) -> Vec<f64> {
        let data = self.data.lock();
        data.live.clone()
    }

    /// Returns the number of recorded values.
    pub fn get_count(&self) -> usize {
        let data = self.data.lock();
        data.count as usize
    }

    /// Returns the sum of recorded values.
    pub fn get_sum(&self) -> f64 {
        let data = self.data.lock();
        data.sum
    }

    /// Returns the average of recorded values.
    pub fn get_avg(&self) -> f64 {
        let data = self.data.lock();
        if data.count == 0 {
            0.0
        } else {
            data.sum / data.count as f64
        }
    }

    /// Returns the value at the given percentile.
    ///
    /// 仅基于 live 窗口计算（折叠后历史观测不可还原），因此对长跑实例是近似值。
    pub fn get_percentile(&self, percentile: f64) -> Result<f64, MetricsError> {
        let mut data = self.data.lock();
        if data.live.is_empty() {
            return Ok(0.0);
        }
        data.live.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
        let index = ((percentile / 100.0) * (data.live.len() - 1) as f64).floor() as usize;
        Ok(data.live[index.min(data.live.len() - 1)])
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
    ///
    /// `count` / `sum` 取自累积状态（含已折叠观测）；各桶先按 `bounds` 统计 live，
    /// 再在 `bounds` 与 `self.bounds` 一致时合并已折叠的分桶计数。
    fn snapshot(&self, bounds: &[f64]) -> HistogramSnapshot {
        let data = self.data.lock();
        let largest = bounds.last().copied();
        let mut deltas = vec![0u64; bounds.len()];

        for &value in data.live.iter() {
            // `NaN` 比较恒为 false，`+Inf > largest`，二者都自然落在 `+Inf` 桶里。
            if let Some(largest) = largest {
                if value <= largest {
                    deltas[bounds.partition_point(|bound| *bound < value)] += 1;
                }
            }
        }

        // 折叠数据按 `self.bounds` 归桶，只有调用方请求同一张分桶表时才能合并。
        if bounds == self.bounds {
            for (delta, folded) in deltas.iter_mut().zip(data.folded_buckets.iter()) {
                *delta += *folded;
            }
        } else {
            debug_assert!(
                data.folded_buckets.iter().all(|&count| count == 0),
                "snapshot 传入非本直方图分桶表时，已折叠数据无法还原"
            );
        }

        let mut running = 0u64;
        for delta in &mut deltas {
            running += *delta;
            *delta = running;
        }

        HistogramSnapshot {
            count: data.count,
            sum: normalize_negative_zero(data.sum),
            // 空直方图的求和落在 `-0.0`（`Sum` 的加法单位元），必须归一。
            cumulative: deltas,
        }
    }

    /// Resets to its initial state.
    pub fn reset(&self) {
        let mut data = self.data.lock();
        data.live.clear();
        data.folded_buckets.iter_mut().for_each(|count| *count = 0);
        data.count = 0;
        data.sum = 0.0;
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

    /// Creates a dynamic counter template for runtime label binding.
    ///
    /// # Example
    /// ```rust
    /// # use synapse_common::metrics::MetricsCollector;
    /// let collector = MetricsCollector::new();
    /// let template = collector.create_dynamic_counter_template(
    ///     "room_operations_total".to_string(),
    ///     vec!["operation", "outcome", "room_version", "visibility", "error_type"]
    /// );
    ///
    /// // Later, observe with specific label values:
    /// template.observe(&["create", "success", "12", "public", "M_FORBIDDEN"]);
    /// ```
    pub fn create_dynamic_counter_template(&self, name: String, label_names: Vec<&str>) -> DynamicCounterTemplate {
        DynamicCounterTemplate::new(name, label_names, self.counters.clone())
    }

    /// Creates a dynamic histogram template for runtime label binding.
    ///
    /// # Example
    /// ```rust
    /// # use synapse_common::metrics::MetricsCollector;
    /// let collector = MetricsCollector::new();
    /// let template = collector.create_dynamic_histogram_template(
    ///     "message_delivery_latency_seconds".to_string(),
    ///     vec!["stage", "room_type", "message_type", "encryption"]
    /// );
    ///
    /// // Later, observe with specific label values:
    /// template.observe(1.5, &["delivered", "private", "m.room.message", "true"]);
    /// ```
    pub fn create_dynamic_histogram_template(&self, name: String, label_names: Vec<&str>) -> DynamicHistogramTemplate {
        DynamicHistogramTemplate::new(name, label_names, self.histograms.clone())
    }

    /// Creates a dynamic gauge template for runtime label binding.
    ///
    /// # Example
    /// ```rust
    /// # use synapse_common::metrics::MetricsCollector;
    /// let collector = MetricsCollector::new();
    /// let template = collector.create_dynamic_gauge_template(
    ///     "synapse_storage_stream_current_position".to_string(),
    ///     vec!["stream"]
    /// );
    ///
    /// // Later, set with specific label values (one series per stream):
    /// template.set(&["events"], 4242.0);
    /// template.set(&["device_lists"], 17.0);
    /// ```
    pub fn create_dynamic_gauge_template(&self, name: String, label_names: Vec<&str>) -> DynamicGaugeTemplate {
        DynamicGaugeTemplate::new(name, label_names, self.gauges.clone())
    }
}

/// 动态 Counter 模板：支持运行时传入标签值，避免预先绑定所有组合。
///
/// 典型场景：需要根据 `operation`, `outcome`, `room_version` 等动态组合进行计数，
/// 而不想为每个组合预先创建独立的 `Counter` 实例。
pub struct DynamicCounterTemplate {
    name: String,
    label_names: Vec<String>,
    counters: Arc<parking_lot::Mutex<HashMap<String, Counter>>>,
}

impl DynamicCounterTemplate {
    /// Creates a new dynamic counter template.
    fn new(name: String, label_names: Vec<&str>, counters: Arc<parking_lot::Mutex<HashMap<String, Counter>>>) -> Self {
        Self { name, label_names: label_names.iter().map(|s| s.to_string()).collect(), counters }
    }

    /// Observes a value with the given label values.
    ///
    /// If the number of label values doesn't match the number of label names
    /// (a programming error), a warning is logged and the observation is dropped
    /// instead of panicking, mirroring [`Self::get_counter`]'s graceful handling.
    pub fn observe(&self, label_values: &[&str]) {
        let Some(labels) = labels_from(&self.label_names, label_values) else {
            tracing::warn!(
                counter = %self.name,
                expected = self.label_names.len(),
                got = label_values.len(),
                "Dropping counter observation: label value count doesn't match label name count"
            );
            return;
        };

        // Create or reuse counter
        let label_signature = label_signature(&labels);
        let mut counters = self.counters.lock();

        if let Some(counter) = counters.get(&label_signature).cloned() {
            counter.inc();
        } else {
            let counter = Counter::with_labels(self.name.clone(), labels);
            counters.insert(label_signature, counter.clone());
            counter.inc();
        }
    }

    /// Gets the underlying counter by label values (if already created).
    pub fn get_counter(&self, label_values: &[&str]) -> Option<Counter> {
        let labels = labels_from(&self.label_names, label_values)?;
        let label_signature = label_signature(&labels);
        let counters = self.counters.lock();
        counters.get(&label_signature).cloned()
    }
}

/// 由标签名/值构造标签表；个数不匹配（编程错误）返回 `None`。
fn labels_from(label_names: &[String], label_values: &[&str]) -> Option<HashMap<String, String>> {
    if label_values.len() != label_names.len() {
        return None;
    }
    Some(label_names.iter().zip(label_values).map(|(name, value)| (name.clone(), (*value).to_string())).collect())
}

/// 标签表的稳定签名，用作动态模板的注册表 key：**同一个指标名按标签组合各存一条**，
/// 这样同名不同标签的序列不会互相覆盖（Prometheus 的数据模型）。
fn label_signature(labels: &HashMap<String, String>) -> String {
    let mut pairs: Vec<_> = labels.iter().collect();
    pairs.sort_by(|a, b| a.0.cmp(b.0));

    pairs.iter().map(|(k, v)| format!("{}={}", k, v)).collect::<Vec<_>>().join(",")
}

/// 动态 Gauge 模板：支持运行时传入标签值。
///
/// 与 [`DynamicCounterTemplate`] 同构，存在的理由也一样：`MetricsCollector::gauges`
/// 以 **name** 为 key，直接连调两次 `register_gauge_with_labels` 会让同名不同标签的
/// 序列互相覆盖（渲染里只剩最后一条）。运行时才知道标签值的 gauge（如
/// `synapse_storage_stream_current_position{stream="…"}`）必须走本模板：
/// **每个标签组合一条独立条目、共享同一个指标名**。
pub struct DynamicGaugeTemplate {
    name: String,
    label_names: Vec<String>,
    gauges: Arc<parking_lot::Mutex<HashMap<String, Gauge>>>,
}

impl DynamicGaugeTemplate {
    /// Creates a new dynamic gauge template.
    fn new(name: String, label_names: Vec<&str>, gauges: Arc<parking_lot::Mutex<HashMap<String, Gauge>>>) -> Self {
        Self { name, label_names: label_names.iter().map(|s| s.to_string()).collect(), gauges }
    }

    /// Sets the gauge for the given label values, creating it on first use.
    ///
    /// 标签值个数与标签名个数不匹配（编程错误）时记一条 warn 并丢弃，
    /// 与 [`DynamicCounterTemplate::observe`] 的处理一致。
    pub fn set(&self, label_values: &[&str], value: f64) {
        let Some(labels) = labels_from(&self.label_names, label_values) else {
            tracing::warn!(
                gauge = %self.name,
                expected = self.label_names.len(),
                got = label_values.len(),
                "Dropping gauge update: label value count doesn't match label name count"
            );
            return;
        };

        let signature = label_signature(&labels);
        let mut gauges = self.gauges.lock();
        if let Some(gauge) = gauges.get(&signature).cloned() {
            gauge.set(value);
        } else {
            let gauge = Gauge::with_labels(self.name.clone(), labels);
            gauge.set(value);
            gauges.insert(signature, gauge);
        }
    }

    /// Gets the gauge for the given label values (if that combination was set before).
    pub fn get(&self, label_values: &[&str]) -> Option<Gauge> {
        let labels = labels_from(&self.label_names, label_values)?;
        let signature = label_signature(&labels);
        self.gauges.lock().get(&signature).cloned()
    }
}

/// 动态 Histogram 模板：支持运行时传入标签值。
pub struct DynamicHistogramTemplate {
    name: String,
    label_names: Vec<String>,
    histograms: Arc<parking_lot::Mutex<HashMap<String, Histogram>>>,
}

impl DynamicHistogramTemplate {
    /// Creates a new dynamic histogram template.
    fn new(
        name: String,
        label_names: Vec<&str>,
        histograms: Arc<parking_lot::Mutex<HashMap<String, Histogram>>>,
    ) -> Self {
        Self { name, label_names: label_names.iter().map(|s| s.to_string()).collect(), histograms }
    }

    /// Observes a value with the given label values.
    ///
    /// If the number of label values doesn't match the number of label names
    /// (a programming error), a warning is logged and the observation is dropped
    /// instead of panicking, consistent with the counter template.
    pub fn observe(&self, value: f64, label_values: &[&str]) {
        let Some(labels) = labels_from(&self.label_names, label_values) else {
            tracing::warn!(
                histogram = %self.name,
                expected = self.label_names.len(),
                got = label_values.len(),
                "Dropping histogram observation: label value count doesn't match label name count"
            );
            return;
        };

        let label_signature = label_signature(&labels);
        let mut histograms = self.histograms.lock();

        if let Some(histogram) = histograms.get(&label_signature).cloned() {
            histogram.observe(value);
        } else {
            let histogram = Histogram::with_labels(self.name.clone(), labels);
            histograms.insert(label_signature, histogram.clone());
            histogram.observe(value);
        }
    }
}

impl MetricsCollector {
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

    /// Increment `name`, registering the counter on first use.
    ///
    /// Single implementation of "register-or-get then inc" — the federation
    /// glue and the media upload path both need it, and duplicating the
    /// match is how the two drift apart.
    pub fn inc_counter(&self, name: &str) {
        if let Some(counter) = self.get_counter(name) {
            counter.inc();
        } else {
            self.register_counter(name.to_string()).inc();
        }
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
    ///
    /// **`# HELP` / `# TYPE` 按指标族去重**：动态模板（counter/histogram/gauge）
    /// 允许同名指标以不同标签共存于同一 map，若逐 entry 输出元数据行，同一族会
    /// 出现多份 `# TYPE` —— Prometheus 遇到重复 `# TYPE` 会**拒绝整个 scrape**。
    /// 同族内再按渲染后的标签串排序，保证多系列顺序确定。
    pub fn to_prometheus_format(&self) -> String {
        let mut output = String::with_capacity(4096);

        {
            let counters = self.counters.lock();
            let mut sorted: Vec<_> = counters
                .values()
                .map(|counter| (counter.name.as_str(), render_labels(&counter.labels, None), counter))
                .collect();
            sorted.sort_unstable_by(|left, right| left.0.cmp(right.0).then_with(|| left.1.cmp(&right.1)));
            let mut last_name: Option<&str> = None;
            for (name, labels, counter) in sorted {
                if last_name != Some(name) {
                    output.push_str(&format!("# HELP {name} {name}\n"));
                    output.push_str(&format!("# TYPE {name} counter\n"));
                    last_name = Some(name);
                }
                output.push_str(&format!("{name}{labels} {}\n", counter.get()));
            }
        }

        {
            let gauges = self.gauges.lock();
            let mut sorted: Vec<_> =
                gauges.values().map(|gauge| (gauge.name.as_str(), render_labels(&gauge.labels, None), gauge)).collect();
            sorted.sort_unstable_by(|left, right| left.0.cmp(right.0).then_with(|| left.1.cmp(&right.1)));
            let mut last_name: Option<&str> = None;
            for (name, labels, gauge) in sorted {
                if last_name != Some(name) {
                    output.push_str(&format!("# HELP {name} {name}\n"));
                    output.push_str(&format!("# TYPE {name} gauge\n"));
                    last_name = Some(name);
                }
                output.push_str(&format!("{name}{labels} {}\n", normalize_negative_zero(gauge.get())));
            }
        }

        {
            let histograms = self.histograms.lock();
            let mut sorted: Vec<_> = histograms
                .values()
                .map(|histogram| (histogram.name.as_str(), render_labels(&histogram.labels, None), histogram))
                .collect();
            sorted.sort_unstable_by(|left, right| left.0.cmp(right.0).then_with(|| left.1.cmp(&right.1)));
            let mut last_name: Option<&str> = None;
            for (base, _, histogram) in sorted {
                let bounds = histogram_buckets_for(base);
                // 单次加锁取全：count / sum / 各桶必须来自同一时刻。
                let snapshot = histogram.snapshot(bounds);

                if last_name != Some(base) {
                    output.push_str(&format!("# HELP {base} {base}\n"));
                    output.push_str(&format!("# TYPE {base} histogram\n"));
                    last_name = Some(base);
                }

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

    /// 缺陷复现（L-1 的前置）：同名不同标签的 gauge **必须**各成一条序列。
    ///
    /// `MetricsCollector::gauges` 以 name 为 key，`register_gauge_with_labels`
    /// 第二次注册会把第一次的条目**覆盖**掉：句柄仍可写，但渲染里少一条序列。
    /// 上游 Synapse 的 `synapse_storage_stream_current_position{stream="…"}`
    /// 正是这个形状，所以不能照搬旧的注册路径。
    #[test]
    fn same_name_different_labels_gauges_must_be_distinct_series() {
        let collector = MetricsCollector::new();
        let template = collector.create_dynamic_gauge_template("stream_position".to_string(), vec!["stream"]);
        template.set(&["events"], 42.0);
        template.set(&["to_device"], 7.0);
        template.set(&["events"], 43.0);

        assert_eq!(template.get(&["events"]).map(|g| g.get()), Some(43.0), "同一标签组合复用同一条序列");
        assert_eq!(template.get(&["to_device"]).map(|g| g.get()), Some(7.0), "另一个标签组合不受影响");
        assert!(template.get(&["presence"]).is_none(), "没 set 过的标签组合不该凭空出现");
        assert!(template.get(&["events", "extra"]).is_none(), "标签值个数不匹配 → 丢弃，不 panic");

        let rendered = collector.to_prometheus_format();
        assert!(rendered.contains(r#"stream_position{stream="to_device"} 7"#), "{rendered}");
        assert!(
            rendered.contains(r#"stream_position{stream="events"} 43"#),
            "同名 gauge 被后一次注册覆盖，渲染里丢了 events 序列：\n{rendered}"
        );
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

    /// 容量上限生效：live 缓冲有界，但 `_count` / `_sum` / 分桶计数在折叠后不丢失。
    #[test]
    fn test_histogram_memory_is_bounded_and_counts_survive_fold() {
        let histogram = Histogram::new("bounded_ms".to_string());
        let total = HISTOGRAM_MAX_LIVE_SAMPLES * 3 + 7;
        for _ in 0..total {
            histogram.observe(7.0); // 7ms 落在 le="10" 桶
        }

        assert!(
            histogram.get_values().len() < HISTOGRAM_MAX_LIVE_SAMPLES,
            "live 缓冲未折叠，内存仍无界: {}",
            histogram.get_values().len()
        );
        assert_eq!(histogram.get_count(), total, "折叠不得丢失观测总数");
        assert_eq!(histogram.get_sum(), total as f64 * 7.0, "折叠不得丢失求和");
        assert_eq!(histogram.get_avg(), 7.0);

        let bounds = histogram_buckets_for("bounded_ms");
        let snapshot = histogram.snapshot(bounds);
        assert_eq!(snapshot.count, total as u64);
        let le10 = bounds.iter().position(|bound| *bound == 10.0).expect("毫秒表应含 le=10");
        assert_eq!(snapshot.cumulative[le10], total as u64, "折叠后的分桶计数必须准确");
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
    fn test_to_prometheus_format_dedups_help_and_type_for_dynamic_gauge_family() {
        // 多标签动态 gauge ⇒ 同族多系列共存。若逐 entry 输出元数据行会出多份
        // `# TYPE` —— Prometheus 遇重复 `# TYPE` 会拒绝**整个** scrape。
        let collector = MetricsCollector::new();
        let template = collector
            .create_dynamic_gauge_template("synapse_storage_stream_current_position".to_string(), vec!["stream"]);
        template.set(&["events"], 42.0);
        template.set(&["device_lists"], 7.0);

        let output = collector.to_prometheus_format();
        assert_eq!(
            output.matches("# TYPE synapse_storage_stream_current_position gauge").count(),
            1,
            "同族只应输出一份 # TYPE:\n{output}"
        );
        assert_eq!(
            output.matches("# HELP synapse_storage_stream_current_position").count(),
            1,
            "同族只应输出一份 # HELP:\n{output}"
        );
        assert!(
            output.contains("synapse_storage_stream_current_position{stream=\"events\"} 42"),
            "events 系列应存在:\n{output}"
        );
        assert!(
            output.contains("synapse_storage_stream_current_position{stream=\"device_lists\"} 7"),
            "device_lists 系列应存在:\n{output}"
        );
    }

    #[test]
    fn test_to_prometheus_format_dedups_help_and_type_for_dynamic_counter_family() {
        // 回归：`room_operations_total` / `cache_operations_total` 在生产中被多系列
        // 喂入，去重前会输出重复 `# TYPE`（现存 bug）。
        let collector = MetricsCollector::new();
        let template =
            collector.create_dynamic_counter_template("room_operations_total".to_string(), vec!["operation"]);
        template.observe(&["create"]);
        template.observe(&["join"]);

        let output = collector.to_prometheus_format();
        assert_eq!(
            output.matches("# TYPE room_operations_total counter").count(),
            1,
            "同族只应输出一份 # TYPE:\n{output}"
        );
        assert!(output.contains("room_operations_total{operation=\"create\"} 1"), "create 系列应存在:\n{output}");
        assert!(output.contains("room_operations_total{operation=\"join\"} 1"), "join 系列应存在:\n{output}");
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
