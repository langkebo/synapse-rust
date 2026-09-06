use synapse_common::metrics::{Counter, MetricsCollector};

/// W7+ 限流中间件的指标句柄——固定基数，进程内单例。
///
/// 6 个 counter，全部在首次使用时注册一次，之后只 `inc()`（AtomicU64
/// Relaxed，无锁）。
///
/// # 设计约束（重要，勿违反）
///
/// ## 1. 绝不带 `endpoint` / `ip` 维度
///
/// `MetricsCollector` 的 registry 是 `HashMap<String, Counter>`，只按 name
/// 索引，且条目**永不移除**。把高基数值（每个路径 / 每个客户端 IP）编进
/// metric name 会让这个 HashMap 无界增长——等价于一个永不淘汰的缓存泄漏。
///
/// 需要下钻到具体端点时看已有的
/// `tracing::debug!(target: "rate_limit", ip, endpoint, ...)` 日志。
/// 分工是：**指标负责聚合告警（"限流在涨吗"），日志负责下钻诊断（"哪个端点在挨打"）**。
///
/// ## 2. counter 句柄必须注册一次后常驻
///
/// `register_counter*` 是 `HashMap::insert` **覆盖**语义，不是 get-or-create。
/// 若在热路径反复注册同名 counter，后一次会把前一次注册的句柄踢出
/// registry：registry 里暴露的是新句柄，而此前已经持有旧句柄的代码仍在
/// inc 旧对象 —— 两边计数永久分叉，采集到的值系统性偏低。
///
/// 同时每次注册都要 `counters.lock()`（全局 mutex），在限流这种每请求都
/// 走的路径上是直接的争用瓶颈。故句柄由 `OnceLock` 缓存。
// `Clone` 是共享语义而非拷贝语义：`Counter` 内部持有 `Arc<AtomicU64>`，
// clone 出来的句柄仍 inc 同一个计数器。这让 `CacheManager`（`#[derive(Clone)]`）
// 能带上本结构——但 **clone 一个 `CacheManager` 会连带复制空的 OnceLock**，
// 副本的首次 `rate_limit_metrics()` 会向传进来的 collector 再注册 6 个
// counter（覆盖 registry 中的同名条目）。生产路径上 `CacheManager` 一律
// 以 `Arc` 共享、从不 clone，故不会触发；新增 clone 用法时需留意。
#[derive(Debug, Clone)]
pub struct RateLimitMetrics {
    /// 进入限流判定的请求总数（不含 exempt 提前返回者）。
    pub requests_total: Counter,
    /// token bucket 有余量、正常放行的请求。
    pub allowed_total: Counter,
    /// token bucket 耗尽、返回 429 的请求。
    pub rejected_total: Counter,
    /// 命中 exempt 列表、未做限流判定直接放行的请求。
    pub exempt_total: Counter,
    /// 限流后端出错（或 Redis 不可用）时按 `fail_open_on_error` 放行的请求。
    ///
    /// 这是**可用性让渡**的信号：放行意味着限流此刻形同虚设。
    /// 持续非零应当告警。
    pub fail_open_total: Counter,
    /// 限流后端出错（或 Redis 不可用）时按 fail-closed 硬 429 的请求。
    ///
    /// 这是**可用性受损**的信号：后端一挂全站 429。
    pub fail_closed_total: Counter,
}

impl RateLimitMetrics {
    /// 向 `collector` 注册 6 个 counter。**只在进程生命周期内调用一次**
    /// （由 `CacheManager::rate_limit_metrics()` 的 `OnceLock` 保证）。
    pub fn new(collector: &MetricsCollector) -> Self {
        Self {
            requests_total: collector.register_counter("rate_limit_requests_total".to_string()),
            allowed_total: collector.register_counter("rate_limit_requests_allowed_total".to_string()),
            rejected_total: collector.register_counter("rate_limit_requests_rejected_total".to_string()),
            exempt_total: collector.register_counter("rate_limit_requests_exempt_total".to_string()),
            fail_open_total: collector.register_counter("rate_limit_fail_open_total".to_string()),
            fail_closed_total: collector.register_counter("rate_limit_fail_closed_total".to_string()),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    #[allow(missing_docs)]
    fn test_registers_all_six_counters() {
        let collector = MetricsCollector::new();
        let metrics = RateLimitMetrics::new(&collector);

        // 6 个 counter 都注册进 registry 且初始为 0
        let all = collector.collect_metrics();
        let names: Vec<&str> = all.iter().map(|m| m.name.as_str()).collect();
        for expected in [
            "rate_limit_requests_total",
            "rate_limit_requests_allowed_total",
            "rate_limit_requests_rejected_total",
            "rate_limit_requests_exempt_total",
            "rate_limit_fail_open_total",
            "rate_limit_fail_closed_total",
        ] {
            assert!(names.contains(&expected), "counter `{}` registered, got {:?}", expected, names);
        }

        // 句柄与 registry 是同一个对象（覆盖语义下这点最容易被破坏）
        metrics.rejected_total.inc();
        metrics.rejected_total.inc();
        let after = collector.collect_metrics();
        let rejected = after.iter().find(|m| m.name == "rate_limit_requests_rejected_total").unwrap();
        assert_eq!(rejected.value, 2.0, "handle must point at the registered counter");
    }

    /// 端到端：限流指标必须出现在 Prometheus 抓取输出里。
    ///
    /// 抓取端点见 `src/server/mod.rs::render_prometheus_metrics`（独立端口，
    /// 默认 9090 + `/metrics`，需 `telemetry.prometheus.enabled = true`），
    /// 它直接渲染 `MetricsCollector::to_prometheus_format()`。
    #[test]
    #[allow(missing_docs)]
    fn test_rate_limit_metrics_appear_in_prometheus_output() {
        let collector = MetricsCollector::new();
        let m = RateLimitMetrics::new(&collector);
        m.rejected_total.inc();
        m.rejected_total.inc();
        m.allowed_total.inc();

        let output = collector.to_prometheus_format();

        assert!(
            output.contains("# TYPE rate_limit_requests_rejected_total counter"),
            "rejected counter 类型声明缺失:\n{output}"
        );
        assert!(
            output.contains("rate_limit_requests_rejected_total 2"),
            "rejected counter 样本缺失或数值不对（期望 2）:\n{output}"
        );
        assert!(output.contains("rate_limit_requests_allowed_total 1"), "allowed counter 数值不对:\n{output}");

        // 6 个 counter 全部出现在输出里
        for name in [
            "rate_limit_requests_total",
            "rate_limit_requests_allowed_total",
            "rate_limit_requests_rejected_total",
            "rate_limit_requests_exempt_total",
            "rate_limit_fail_open_total",
            "rate_limit_fail_closed_total",
        ] {
            assert!(output.contains(name), "counter `{name}` 未出现在 prometheus 输出:\n{output}");
        }
    }

    #[test]
    #[allow(missing_docs)]
    fn test_counters_are_independent() {
        let collector = MetricsCollector::new();
        let m = RateLimitMetrics::new(&collector);

        m.requests_total.inc();
        m.allowed_total.inc();
        m.exempt_total.inc();
        m.fail_open_total.inc();
        m.fail_closed_total.inc();

        let all = collector.collect_metrics();
        let value = |name: &str| -> u64 { all.iter().find(|x| x.name == name).map(|x| x.value as u64).unwrap_or(0) };
        assert_eq!(value("rate_limit_requests_total"), 1);
        assert_eq!(value("rate_limit_requests_allowed_total"), 1);
        assert_eq!(value("rate_limit_requests_rejected_total"), 0, "未 inc 应保持 0");
        assert_eq!(value("rate_limit_requests_exempt_total"), 1);
        assert_eq!(value("rate_limit_fail_open_total"), 1);
        assert_eq!(value("rate_limit_fail_closed_total"), 1);
    }
}
