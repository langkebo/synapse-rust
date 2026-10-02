//! 各 stream 的当前位置 —— 指标 `synapse_storage_stream_current_position` 的数据源。
//!
//! **只收录真的有单调位置列、且生产路径会推进的 stream。** 为什么另外几张看起来
//! 像 stream 的表不在这里（取证见 `docs/audit/OPTIMIZATION_EXECUTION_PLAN_2026-09-15.md`
//! §8.3 L-1）：
//!
//! * `sync_stream_id`：只有 seed 写入，全仓无读/写者；
//! * `device_lists_outbound_pokes.stream_id`：只有 DELETE 路径，没有 INSERT；
//! * `worker_events.stream_id`：写入口 `WorkerManager::add_event` 无调用者 ⇒ 恒 0；
//! * `room_ephemeral.stream_id`：调用方传的是**墙钟毫秒**且被 UPSERT 覆盖 ⇒ 非单调；
//! * upstream 的 presence / typing / receipts / account_data / push_rules / e2ee_keys /
//!   backfill / federation 等 stream：本仓**没有**位置列（表里只有 `last_active_ts`
//!   这类时间戳，或被 `id BIGSERIAL` 之外没有游标列）。
use sqlx::PgPool;

/// 一个 stream 的当前位置。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StreamPositionRow {
    /// 指标 `stream` 标签取值（须与 `synapse_common::metrics` 侧登记表逐字一致）。
    pub stream: String,
    /// 当前最大位置；空表为 0。
    pub position: i64,
}

/// 读取每个已登记 stream 的当前位置。
///
/// 单条 `UNION ALL` 查询：5 个 `MAX(...)` 一次往返；每张表都走主键/索引，
/// 空表经 `COALESCE` 归零（`MAX` 在没有行时是 NULL）。
///
/// 输出必须与 `synapse_common::metrics::StreamPosition::ALL` 的标签集合**逐字对应**，
/// 由 `tests/integration/stream_position_tests.rs` 守卫（两边漂移即红）。
pub async fn get_stream_positions(pool: &PgPool) -> Result<Vec<StreamPositionRow>, sqlx::Error> {
    let rows = sqlx::query!(
        r#"
        SELECT 'events' AS stream, COALESCE(MAX(stream_ordering), 0) AS "position!" FROM events
        UNION ALL
        SELECT 'to_device', COALESCE(MAX(stream_id), 0) FROM to_device_messages
        UNION ALL
        SELECT 'device_lists', COALESCE(MAX(stream_id), 0) FROM device_lists_stream
        UNION ALL
        SELECT 'sliding_sync', COALESCE(MAX(pos), 0) FROM sliding_sync_tokens
        UNION ALL
        SELECT 'quarantined_media', COALESCE(MAX(stream_id), 0) FROM quarantined_media_changes
        "#,
    )
    .fetch_all(pool)
    .await?;

    Ok(rows
        .into_iter()
        .map(|row| StreamPositionRow { stream: row.stream.unwrap_or_default(), position: row.position })
        .collect())
}
