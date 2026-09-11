-- =====================================================================
-- Undo Migration: 20260906010000_add_events_soft_failed.undo.sql
--
-- Forward 迁移: events 表新增 soft_failed BOOLEAN NOT NULL DEFAULT FALSE，
--               并建部分索引 idx_events_room_soft_failed。
--
-- 回滚: 删除该列与其专用索引。**但请先阅读下方警告。**
--
-- ⚠️ 语义警告：forward 迁移引入 soft-fail 是为了取代"物理删除重复事件"
--    （txn_dedup.rs 中并发 txn_id 竞态下用 DELETE 抹掉落败的重复事件，
--     会引发 FK 违约、prev_events DAG 断链与审计/合规丢失）。
--    当前所有消费方读路径都以 `WHERE soft_failed = FALSE` 过滤。
--    一旦删除该列，**所有此前被标记 soft_failed = TRUE 的事件会重新对客户端可见**，
--    包括本应被去重隐藏的事件。回滚前请确认：
--      SELECT count(*) FROM events WHERE soft_failed = TRUE;
--    若该值不为 0，回滚会改变对客户端可见的事件集合。
--
-- 另注: v11 baseline 也声明了 soft_failed 与其索引（forward 迁移已折入 baseline），
--       因此仅靠本 undo 无法把"schema 定义"恢复到 pre-B-8 状态，
--       完整回退需要同步修改 baseline。此处只撤销迁移本身施加的对象。
-- =====================================================================

-- 专用部分索引（forward 迁移新增；baseline 中同样声明）
DROP INDEX IF EXISTS idx_events_room_soft_failed;

-- soft_failed 列
ALTER TABLE events DROP COLUMN IF EXISTS soft_failed;

-- 有意**不**删除 idx_events_stream_ordering：
-- forward 迁移虽以 `CREATE INDEX CONCURRENTLY IF NOT EXISTS idx_events_stream_ordering
-- ON events (room_id, stream_ordering DESC) WHERE soft_failed = FALSE` 声明它，
-- 但 baseline 早已用**同一名字**定义了 `ON events(stream_ordering)`（非部分索引）。
-- 由于 IF NOT EXISTS 的存在，forward 迁移在 baseline 建库路径下从未真正创建过它，
-- 实际存在的是 baseline 版本；删除它会破坏 baseline 的索引集合。
