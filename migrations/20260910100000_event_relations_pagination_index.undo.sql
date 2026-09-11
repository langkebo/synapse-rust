-- =====================================================================
-- Undo Migration: 20260910100000_event_relations_pagination_index.undo.sql
--
-- Forward 迁移: 新增复合索引，覆盖 relations/mod.rs::get_relations 键集分页的
--               「等值前缀 + 排序键」：
--   CREATE INDEX IF NOT EXISTS idx_event_relations_room_rel_ts_evt
--       ON event_relations (room_id, relates_to_event_id, origin_server_ts DESC, event_id DESC);
--
-- 回滚: **无法真正回滚到"没有该索引"的状态** —— v11 baseline 也声明了同一索引
--       （migrations/00000000_unified_schema_v11.sql:3582），因此
--       "把索引从 schema 定义中移除"需要同步修改 baseline，本 undo 不做。
--
-- 为避免 undo 只是单向破坏 schema，这里改为**幂等重述 baseline 定义**：
-- 若该索引因误删等原因缺失，执行 undo 会把它恢复成 baseline 的形态。
-- 这样 forward → undo → forward 的往返不会让索引消失
-- （此前版本只 DROP 不重建，往返后索引会永久丢失）。
--
-- 注: 本文件曾命名为 ..._undo.sql（缺少 .undo 后缀）。由于 migrator 的
--     `find ... ! -name '*.undo.sql'` 过滤，它当时被当作**正向迁移**执行，
--     即无条件 DROP 掉刚由正向迁移建立的 P1 性能索引。已在 2b16dc3c 重命名修正。
-- =====================================================================

-- 幂等重述 baseline 定义（等价于 CREATE INDEX IF NOT EXISTS）
CREATE INDEX IF NOT EXISTS idx_event_relations_room_rel_ts_evt
    ON event_relations (room_id, relates_to_event_id, origin_server_ts DESC, event_id DESC);

-- 说明：基础索引 idx_event_relations_room_event 由 baseline 维护，本 undo 不触碰。
