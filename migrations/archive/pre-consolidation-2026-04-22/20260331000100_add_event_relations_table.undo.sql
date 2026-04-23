-- V260331_001__MIG-RELATIONS__add_event_relations_table.undo.sql
--
-- 回滚: 删除 event_relations 表
-- 对应迁移: V260331_001__MIG-RELATIONS__add_event_relations_table.sql

SET TIME ZONE 'UTC';

DO $$
BEGIN
    RAISE NOTICE '开始回滚 event_relations 表...';
END $$;

-- 删除索引
DROP INDEX IF EXISTS idx_event_relations_unique;
DROP INDEX IF EXISTS idx_event_relations_room_event;
DROP INDEX IF EXISTS idx_event_relations_sender;
DROP INDEX IF EXISTS idx_event_relations_origin_ts;

-- 删除表
DROP TABLE IF EXISTS event_relations;

DO $$
BEGIN
    RAISE NOTICE 'event_relations 表回滚完成';
END $$;
