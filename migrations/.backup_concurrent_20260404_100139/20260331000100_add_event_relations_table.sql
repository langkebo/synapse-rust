-- V260331_001__MIG-RELATIONS__add_event_relations_table.sql
--
-- 描述: 创建 event_relations 表支持 Matrix Relations API
-- 关联代码: src/storage/relations.rs
--
-- 支持的功能:
--   - m.annotation (reactions/表情反应)
--   - m.reference (引用)
--   - m.replace (编辑/替换)
--   - m.thread (线程回复)
--
-- 回滚: V260331_001__MIG-RELATIONS__add_event_relations_table.undo.sql

SET TIME ZONE 'UTC';

DO $$
BEGIN
    RAISE NOTICE '开始创建 event_relations 表...';
END $$;

-- ============================================================================
-- event_relations 表
-- ============================================================================

CREATE TABLE IF NOT EXISTS event_relations (
    id BIGSERIAL PRIMARY KEY,
    room_id TEXT NOT NULL,
    event_id TEXT NOT NULL,
    relates_to_event_id TEXT NOT NULL,
    relation_type TEXT NOT NULL,
    sender TEXT NOT NULL,
    origin_server_ts BIGINT NOT NULL,
    content JSONB NOT NULL DEFAULT '{}',
    is_redacted BOOLEAN NOT NULL DEFAULT FALSE,
    created_ts BIGINT NOT NULL
);

-- 唯一约束: 防止重复的关系
CREATE UNIQUE INDEX CONCURRENTLY IF NOT EXISTS idx_event_relations_unique
    ON event_relations(event_id, relation_type, sender);

-- 房间和事件索引: 快速查询某个事件的所有关系
CREATE INDEX CONCURRENTLY IF NOT EXISTS idx_event_relations_room_event
    ON event_relations(room_id, relates_to_event_id, relation_type);

-- 发送者索引: 快速查询某个用户发送的关系
CREATE INDEX CONCURRENTLY IF NOT EXISTS idx_event_relations_sender
    ON event_relations(sender, relation_type);

-- 时间索引: 按时间排序查询
CREATE INDEX CONCURRENTLY IF NOT EXISTS idx_event_relations_origin_ts
    ON event_relations(room_id, origin_server_ts DESC);

-- 注解: 表和列说明
COMMENT ON TABLE event_relations IS 'Stores Matrix event relations (annotations, references, replacements, threads)';
COMMENT ON COLUMN event_relations.event_id IS 'The event that is relating to another event';
COMMENT ON COLUMN event_relations.relates_to_event_id IS 'The event_id being related to';
COMMENT ON COLUMN event_relations.relation_type IS 'Relation type: m.annotation (reactions), m.reference, m.replace (edits), m.thread';

-- ============================================================================
-- 完成
-- ============================================================================

DO $$
BEGIN
    RAISE NOTICE 'event_relations 表创建完成';
END $$;
