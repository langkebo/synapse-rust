//! `DegradationMetrics` 的健康度指标：命中率与降级率的**取数口径**。
//!
//! 这两项直接进监控面板与告警阈值，算错不是"数字难看"，而是**把故障藏起来**：
//!
//! - `hit_rate()` 的分母必须同时含本地与 Redis 两层 —— 漏一层会让命中率虚高，
//!   而"命中率高"恰好是"缓存正常"的那个信号；
//! - `degradation_rate()` 的分子只有 `fallback_operations`。把它当成"降级请求数"
//!   （即分子分母同一个计数）会让降级率恒 100%，"库回落率"这个维度直接消失。
//!
//! 还有一类**静默故障**只能靠空观测用例挡住：`0/0` 一旦漏判就返回 `NaN`，
//! 面板显示 `NaN`、告警表达式里的 `> 阈值` 全部变 `false` —— 不显示错误，也不告警。
//!
//! 全部走 `record_*` 公开 API，不写字段字面量：记录端改名时这里要一起红。

use synapse_cache::DegradationMetrics;

#[test]
fn hit_rate_is_zero_when_nothing_has_been_observed() {
    let metrics = DegradationMetrics::new();

    // 反向断言：不仅要是 0.0，还必须**不是** NaN（`0/0` 的默认产物）。
    assert!(!metrics.hit_rate().is_nan(), "无观测时命中率不能是 NaN");
    assert_eq!(metrics.hit_rate(), 0.0);
}

#[test]
fn hit_rate_puts_both_tiers_in_the_same_denominator() {
    let mut metrics = DegradationMetrics::new();
    metrics.record_local_hit();
    metrics.record_local_miss();
    metrics.record_redis_hit();
    metrics.record_redis_hit();

    // 3 命中 / 4 次查找 = 75%。分母只算本地 => 50%，只算 Redis => 100%，
    // 两种错法都会被这条区分出来。
    assert_eq!(metrics.hit_rate(), 75.0);
}

#[test]
fn hit_rate_reaches_100_when_every_lookup_hit() {
    let mut metrics = DegradationMetrics::new();
    metrics.record_local_hit();
    metrics.record_local_hit();
    metrics.record_redis_hit();
    metrics.record_redis_hit();

    assert_eq!(metrics.hit_rate(), 100.0);
}

#[test]
fn degradation_rate_is_zero_when_no_request_was_degraded() {
    let mut metrics = DegradationMetrics::new();
    // 有回落、但没有一次被记成"降级请求"：分母为 0。
    metrics.record_fallback();

    // 反向断言：不能因为分子非零就除出 inf / NaN。
    assert!(!metrics.degradation_rate().is_nan(), "分母为 0 时降级率不能是 NaN");
    assert!(metrics.degradation_rate().is_finite(), "分母为 0 时降级率必须是有限值");
    assert_eq!(metrics.degradation_rate(), 0.0);
}

#[test]
fn degradation_rate_counts_only_fallback_operations_in_the_numerator() {
    let mut metrics = DegradationMetrics::new();
    // 4 次降级请求：1 次回落库、2 次被熔断拒绝、1 次其它。
    for _ in 0..4 {
        metrics.record_degraded_request();
    }
    metrics.record_fallback();
    metrics.record_circuit_breaker_rejection();
    metrics.record_circuit_breaker_rejection();

    // 1/4 = 25%。若把熔断拒绝也算成回落（3/4=75%）或分子=分母（100%），都会红。
    assert_eq!(metrics.degradation_rate(), 25.0);
}

#[test]
fn degradation_rate_reaches_100_when_every_degraded_request_fell_back() {
    let mut metrics = DegradationMetrics::new();
    for _ in 0..2 {
        metrics.record_degraded_request();
        metrics.record_fallback();
    }

    assert_eq!(metrics.degradation_rate(), 100.0);
}
