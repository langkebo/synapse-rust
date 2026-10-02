//! 各 stream 的当前位置 —— 指标 `synapse_storage_stream_current_position` 的数据源。
//!
//! **只收录真的有单调位置列、且生产路径会推进的 stream。** 为什么另外几张看起来
//! 像 stream 的表不在这里（取证见 `docs/audit/OPTIMIZATION_EXECUTION_PLAN_2026-09-15.md`
//! §8.3 L-1）：
//!
//! * `sync_stream_id`：只有 seed 写入，全仓无读/写者；
//! * `device_lists_outbound_pokes`：**已按铁律 1 整表删除**（2026-10-03）—— 它在生产代码里
//!   既没有 INSERT 也没有 SELECT，唯一引用是对空表做 DELETE 的 pruning 函数。联邦
//!   device-list 更新实际走 EDU 发送路径（见 `synapse-web/src/federation/edu.rs`），与本表无关。
//!   同批删除：表 + 3 索引 + FK + `prune_sent_device_lists_outbound_pokes` +
//!   `src/server/mod.rs` 的 `prune_step!` + baseline 里的建表语句；
//! * `room_ephemeral.stream_id`：**它不是 stream 位置** —— 而是按房维度的**新鲜度排序键**，
//!   由调用方写入、被 UPSERT 覆盖，且只用于 `ORDER BY stream_id DESC`
//!   （`RoomEphemeralEvent.stream_id` 在服务层从不被读取，也不做游标）⇒ **永久排除**，
//!   不是"非单调、待修"的缺陷；
//! * upstream 的 presence / typing / receipts / account_data / push_rules / e2ee_keys /
//!   backfill / federation 等 stream：本仓**没有**位置列（表里只有 `last_active_ts`
//!   这类时间戳，或被 `id BIGSERIAL` 之外没有游标列）。
//!
//! `worker_events.stream_id` **已收录**：它由事件写入路径（`EventWriter` 装饰器）
//! 推进，但只在 worker 模式（`worker.enabled`）下启用 ⇒ 单进程部署里该序列恒 0 是
//! **预期**（总线本身不启用），不是"没有写者"的漂移。
use sqlx::PgPool;

/// 一个 stream 的当前位置。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StreamPositionRow {
    /// 指标 `stream` 标签取值（须与 `synapse_common::server_metrics` 侧登记表逐字一致）。
    pub stream: String,
    /// 当前最大位置；空表为 0。
    pub position: i64,
}

/// 读取每个已登记 stream 的当前位置。
///
/// 单条 `UNION ALL` 查询：6 个 `MAX(...)` 一次往返；每张表都走主键/索引，
/// 空表经 `COALESCE` 归零（`MAX` 在没有行时是 NULL）。
///
/// 输出必须与 `synapse_common::server_metrics::StreamPosition::ALL` 的标签集合**逐字对应**，
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
        UNION ALL
        SELECT 'worker_events', COALESCE(MAX(stream_id), 0) FROM worker_events
        "#,
    )
    .fetch_all(pool)
    .await?;

    Ok(rows
        .into_iter()
        .map(|row| StreamPositionRow { stream: row.stream.unwrap_or_default(), position: row.position })
        .collect())
}
