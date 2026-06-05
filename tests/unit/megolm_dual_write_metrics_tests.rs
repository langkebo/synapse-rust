//! Phase 2: Megolm 双写相关 metrics 记录准确性测试
//!
//! 覆盖 `ServerMetrics` 中 Phase 2 新增的方法：
//! - `record_megolm_vodozemac_pickle_persist`（成功/失败路径）
//! - `record_megolm_dual_write_promotion`（成功/失败路径）
//! - `record_megolm_lazy_migration_batch`（scanned/promoted 累加）

#![cfg(test)]

use std::sync::Arc;

use synapse_rust::common::metrics::MetricsCollector;
use synapse_rust::common::server_metrics::ServerMetrics;

fn make_metrics() -> ServerMetrics {
    let collector = Arc::new(MetricsCollector::new());
    ServerMetrics::new(collector)
}

#[test]
fn test_pickle_persist_success_increments_total_and_observes_duration() {
    let metrics = make_metrics();

    metrics.record_megolm_vodozemac_pickle_persist(15.5, true);
    metrics.record_megolm_vodozemac_pickle_persist(20.0, true);

    assert_eq!(metrics.megolm_vodozemac_pickle_persist_total.get(), 2);
    assert_eq!(metrics.megolm_vodozemac_pickle_persist_errors_total.get(), 0);
    // 成功时 histogram 应被 observe 2 次
    assert_eq!(metrics.megolm_pickle_persist_duration_ms.get_count(), 2);
}

#[test]
fn test_pickle_persist_failure_only_increments_error_counter() {
    let metrics = make_metrics();

    metrics.record_megolm_vodozemac_pickle_persist(10.0, false);
    metrics.record_megolm_vodozemac_pickle_persist(12.0, false);

    assert_eq!(metrics.megolm_vodozemac_pickle_persist_total.get(), 2);
    assert_eq!(metrics.megolm_vodozemac_pickle_persist_errors_total.get(), 2);
    // 失败时 histogram **不应**被 observe（避免污染分布）
    assert_eq!(metrics.megolm_pickle_persist_duration_ms.get_count(), 0);
}

#[test]
fn test_pickle_persist_mixed_success_and_failure() {
    let metrics = make_metrics();

    metrics.record_megolm_vodozemac_pickle_persist(5.0, true);
    metrics.record_megolm_vodozemac_pickle_persist(7.0, false);
    metrics.record_megolm_vodozemac_pickle_persist(8.0, true);

    assert_eq!(metrics.megolm_vodozemac_pickle_persist_total.get(), 3);
    assert_eq!(metrics.megolm_vodozemac_pickle_persist_errors_total.get(), 1);
    // histogram 只累计成功的 2 次
    assert_eq!(metrics.megolm_pickle_persist_duration_ms.get_count(), 2);
}

#[test]
fn test_dual_write_promotion_success_counter() {
    let metrics = make_metrics();

    metrics.record_megolm_dual_write_promotion(true);
    metrics.record_megolm_dual_write_promotion(true);

    assert_eq!(metrics.megolm_dual_write_promotions_total.get(), 2);
    assert_eq!(metrics.megolm_dual_write_promotion_errors_total.get(), 0);
}

#[test]
fn test_dual_write_promotion_failure_counter() {
    let metrics = make_metrics();

    metrics.record_megolm_dual_write_promotion(false);

    assert_eq!(metrics.megolm_dual_write_promotions_total.get(), 0);
    assert_eq!(metrics.megolm_dual_write_promotion_errors_total.get(), 1);
}

#[test]
fn test_lazy_migration_batch_increments_scanned_and_promoted() {
    let metrics = make_metrics();

    metrics.record_megolm_lazy_migration_batch(100, 80);
    metrics.record_megolm_lazy_migration_batch(50, 30);

    assert_eq!(metrics.megolm_lazy_migration_sessions_scanned_total.get(), 150);
    assert_eq!(metrics.megolm_lazy_migration_sessions_promoted_total.get(), 110);
}

#[test]
fn test_lazy_migration_zero_promoted_only_increments_scanned() {
    let metrics = make_metrics();

    metrics.record_megolm_lazy_migration_batch(200, 0);

    assert_eq!(metrics.megolm_lazy_migration_sessions_scanned_total.get(), 200);
    assert_eq!(metrics.megolm_lazy_migration_sessions_promoted_total.get(), 0);
}

/// 端到端模拟：encrypt 循环 N 次，全部成功 → total=N, errors=0, histogram count=N
#[test]
fn test_e2e_pickle_persist_loop_all_success() {
    let metrics = make_metrics();

    for i in 1..=10 {
        metrics.record_megolm_vodozemac_pickle_persist(i as f64, true);
    }

    assert_eq!(metrics.megolm_vodozemac_pickle_persist_total.get(), 10);
    assert_eq!(metrics.megolm_vodozemac_pickle_persist_errors_total.get(), 0);
    assert_eq!(metrics.megolm_pickle_persist_duration_ms.get_count(), 10);
}

/// 端到端模拟：模拟 70% 成功率（迁移过程中偶发 DB 失败）
#[test]
fn test_e2e_pickle_persist_loop_partial_failure() {
    let metrics = make_metrics();

    for i in 1..=100 {
        let success = i % 10 != 0; // 90% 成功率
        metrics.record_megolm_vodozemac_pickle_persist(i as f64, success);
    }

    assert_eq!(metrics.megolm_vodozemac_pickle_persist_total.get(), 100);
    assert_eq!(metrics.megolm_vodozemac_pickle_persist_errors_total.get(), 10);
    assert_eq!(metrics.megolm_pickle_persist_duration_ms.get_count(), 90);
}
