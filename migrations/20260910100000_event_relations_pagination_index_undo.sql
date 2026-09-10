-- 撤销：P1 递归关系查询加速 – event_relations 分页索引
-- 2026-09-10

DROP INDEX IF EXISTS idx_event_relations_room_rel_ts_evt;

-- 注意：基础索引保留；如需完全回滚至原始状态，需同步删除 unified_schema 中的同名索引声明