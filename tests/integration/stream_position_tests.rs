#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
//! V-12：`synapse_storage_stream_current_position` 的 per-stream 口径。
//!
//! 两条不变量：
//! ① 登记表 `StreamPosition::ALL`（`synapse-common`）与数据源 SQL
//!    （`synapse_storage::stream_positions::get_stream_positions`）**逐字对应**
//!    —— SQL 里加/删一个 stream 而登记表没跟上即红（每个 stream 都必须有值）；
//! ② 每个 stream 的位置**随写入推进**。
use std::collections::HashMap;

use synapse_common::server_metrics::StreamPosition;
use synapse_storage::stream_positions::get_stream_positions;

async fn positions(pool: &sqlx::PgPool) -> HashMap<String, i64> {
    get_stream_positions(pool)
        .await
        .expect("stream positions")
        .into_iter()
        .map(|row| (row.stream, row.position))
        .collect()
}

/// V-12 ①：标签集合必须与登记表逐字一致（顺序也一致：SQL 的 UNION ALL 顺序）。
#[tokio::test]
async fn stream_labels_match_the_registry_exactly() {
    let pool = crate::require_test_pool().await;

    let labels: Vec<String> = get_stream_positions(pool.as_ref())
        .await
        .expect("stream positions")
        .into_iter()
        .map(|row| row.stream)
        .collect();
    let expected: Vec<String> = StreamPosition::ALL.iter().map(|stream| stream.label().to_string()).collect();

    assert_eq!(
        labels, expected,
        "SQL 输出的 stream 集合与 StreamPosition::ALL 不一致 —— 两边必须同批修改（漂移会让 \
         /metrics 上少一条或多一条序列，且没有任何别的守卫会发现）"
    );
}

/// V-12 ②：每个已登记 stream 都随一次真实写入推进。
///
/// 这里直接插行而不是走各 stream 的写入路径：本用例验证的是**指标数据源**
/// （哪张表的哪一列、是否单调），各写入路径由各自的测试覆盖。
#[tokio::test]
async fn every_stream_position_advances_with_writes() {
    let pool = crate::require_test_pool().await;
    let before = positions(pool.as_ref()).await;
    assert_eq!(before.len(), StreamPosition::ALL.len(), "每个 stream 都必须出现在结果里");

    let suffix = uuid::Uuid::new_v4().simple().to_string();
    let user_id = format!("@streampos_{suffix}:localhost");
    let room_id = format!("!streampos_{suffix}:localhost");
    crate::ensure_test_room(&pool, &room_id).await;

    // events：`stream_ordering` 由 DEFAULT nextval 推进。
    sqlx::query(
        "INSERT INTO events (event_id, room_id, sender, event_type, content, origin_server_ts) \
         VALUES ($1, $2, $3, 'm.room.message', '{}'::jsonb, 1)",
    )
    .bind(format!("$streampos_{suffix}:localhost"))
    .bind(&room_id)
    .bind(&user_id)
    .execute(pool.as_ref())
    .await
    .unwrap();

    // to_device：`stream_id` 没有 DEFAULT，由写者 nextval（这里显式取 MAX+1 保证推进）。
    sqlx::query(
        "INSERT INTO to_device_messages \
         (stream_id, sender_user_id, sender_device_id, recipient_user_id, recipient_device_id, event_type, created_ts) \
         SELECT COALESCE(MAX(stream_id), 0) + 1, $1, 'D1', $1, 'D1', 'm.test', 1 FROM to_device_messages",
    )
    .bind(&user_id)
    .execute(pool.as_ref())
    .await
    .unwrap();

    // device_lists：`stream_id` 是 BIGSERIAL。
    sqlx::query("INSERT INTO device_lists_stream (user_id, created_ts) VALUES ($1, 1)")
        .bind(&user_id)
        .execute(pool.as_ref())
        .await
        .unwrap();

    // sliding_sync：`pos` 没有 DEFAULT，写者显式取 nextval。
    sqlx::query(
        "INSERT INTO sliding_sync_tokens (user_id, device_id, token, pos, created_ts) \
         SELECT $1, 'D1', 'tok', COALESCE(MAX(pos), 0) + 1, 1 FROM sliding_sync_tokens",
    )
    .bind(&user_id)
    .execute(pool.as_ref())
    .await
    .unwrap();

    // quarantined_media：`stream_id` 是 BIGSERIAL。
    sqlx::query(
        "INSERT INTO quarantined_media_changes (media_id, server_name, change_type, changed_by, created_ts) \
         VALUES ($1, 'localhost', 'quarantined', $2, 1)",
    )
    .bind(format!("media_{suffix}"))
    .bind(&user_id)
    .execute(pool.as_ref())
    .await
    .unwrap();

    // worker_events：`stream_id` 由 DEFAULT nextval 推进。生产写者是事件写入
    // 装饰器（仅在 worker 模式下接线），这里直接插行以验证指标数据源本身。
    sqlx::query(
        "INSERT INTO worker_events (event_id, event_type, room_id, sender, event_data, created_ts) \
         VALUES ($1, 'm.room.message', $2, $3, '{}'::jsonb, 1)",
    )
    .bind(format!("$streampos_worker_{suffix}:localhost"))
    .bind(&room_id)
    .bind(&user_id)
    .execute(pool.as_ref())
    .await
    .unwrap();

    let after = positions(pool.as_ref()).await;
    for stream in StreamPosition::ALL {
        let label = stream.label();
        assert!(after[label] > before[label], "stream `{label}` 没有随写入推进：{} -> {}", before[label], after[label]);
    }
}
