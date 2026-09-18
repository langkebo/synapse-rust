#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
//! STO-05: report_rate_limits 的 SELECT→UPDATE TOCTOU 修复回归测试。
//!
//! - `record_report` 原子 UPSERT：并发计数不得丢失
//! - `check_rate_limit` 事务 + FOR UPDATE：过期封锁解除语义保持

use std::sync::Arc;
use synapse_storage::event_report::EventReportStorage;

async fn cleanup(pool: &Arc<sqlx::PgPool>, user_id: &str) {
    sqlx::query("DELETE FROM report_rate_limits WHERE user_id = $1")
        .bind(user_id)
        .execute(&**pool)
        .await
        .expect("cleanup report_rate_limits");
}

#[tokio::test]
async fn sto05_concurrent_record_report_counts_exactly() {
    let pool = crate::require_test_pool().await;
    let storage = Arc::new(EventReportStorage::new(&pool));
    let user_id = "@sto05_concurrent:example.com";
    crate::ensure_test_user(&pool, "@sto05_concurrent:example.com").await;
    cleanup(&pool, user_id).await;

    const N: usize = 20;
    let mut handles = Vec::new();
    for _ in 0..N {
        let storage = storage.clone();
        handles.push(tokio::spawn(async move {
            storage.record_report(user_id).await.expect("record_report should succeed");
        }));
    }
    for handle in handles {
        handle.await.expect("spawned record_report must not panic");
    }

    // 旧的 SELECT→UPDATE 两步走在并发下会丢失计数（count < N），
    // 原子 UPSERT 必须精确计数。
    let count: i64 = sqlx::query_scalar("SELECT report_count::bigint FROM report_rate_limits WHERE user_id = $1")
        .bind(user_id)
        .fetch_one(&*pool)
        .await
        .expect("rate limit row must exist after concurrent inserts");
    assert_eq!(count, N as i64, "concurrent record_report must not lose updates");

    cleanup(&pool, user_id).await;
}

#[tokio::test]
async fn sto05_record_report_resets_count_after_one_day() {
    let pool = crate::require_test_pool().await;
    let storage = EventReportStorage::new(&pool);
    let user_id = "@sto05_window:example.com";
    crate::ensure_test_user(&pool, "@sto05_window:example.com").await;
    cleanup(&pool, user_id).await;

    // 手工种一条「昨天」的记录，count=50
    let two_days_ago = synapse_common::current_timestamp_millis() - 2 * 86_400_000;
    sqlx::query(
        "INSERT INTO report_rate_limits (user_id, report_count, last_report_at, created_ts, updated_ts) VALUES ($1, 50, $2, $2, $2)",
    )
    .bind(user_id)
    .bind(two_days_ago)
    .execute(&*pool)
    .await
    .expect("seed stale rate limit row");

    // 超过一天窗口后计数必须重置为 1，而不是 51
    storage.record_report(user_id).await.expect("record_report");
    let count: i64 = sqlx::query_scalar("SELECT report_count::bigint FROM report_rate_limits WHERE user_id = $1")
        .bind(user_id)
        .fetch_one(&*pool)
        .await
        .expect("row must exist");
    assert_eq!(count, 1, "count must reset after the one-day window");

    cleanup(&pool, user_id).await;
}

#[tokio::test]
async fn sto05_check_rate_limit_lifts_only_expired_blocks() {
    let pool = crate::require_test_pool().await;
    let storage = EventReportStorage::new(&pool);
    let user_id = "@sto05_unblock:example.com";
    crate::ensure_test_user(&pool, "@sto05_unblock:example.com").await;
    cleanup(&pool, user_id).await;

    // 过期封锁 → 解除并放行
    storage
        .block_user_reports(user_id, synapse_common::current_timestamp_millis() - 1000, "expired block")
        .await
        .expect("block_user_reports");
    let check = storage.check_rate_limit(user_id).await.expect("check_rate_limit");
    assert!(check.is_allowed, "expired block must be lifted");

    // 未过期封锁 → 保持拒绝
    storage
        .block_user_reports(user_id, synapse_common::current_timestamp_millis() + 3_600_000, "active block")
        .await
        .expect("block_user_reports");
    let check = storage.check_rate_limit(user_id).await.expect("check_rate_limit");
    assert!(!check.is_allowed, "active block must deny");
    assert_eq!(check.block_reason.as_deref(), Some("active block"));

    cleanup(&pool, user_id).await;
}
