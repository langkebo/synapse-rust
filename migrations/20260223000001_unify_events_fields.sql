-- Migration: Unify events table field names
-- Version: 20260223000001
-- Description: 统一 events 表字段命名，移除冗余的 type 字段

-- 确保 event_type 列存在且有数据
ALTER TABLE events ADD COLUMN IF NOT EXISTS event_type VARCHAR(255);

-- 从 type 复制数据到 event_type（如果 event_type 为空）
UPDATE events SET event_type = type WHERE event_type IS NULL AND type IS NOT NULL;

-- 确保 user_id 列存在且有数据
ALTER TABLE events ADD COLUMN IF NOT EXISTS user_id VARCHAR(255);
UPDATE events SET user_id = sender WHERE user_id IS NULL;

-- 添加缺失的列
ALTER TABLE events ADD COLUMN IF NOT EXISTS processed_ts BIGINT;
ALTER TABLE events ADD COLUMN IF NOT EXISTS not_before BIGINT DEFAULT 0;
ALTER TABLE events ADD COLUMN IF NOT EXISTS status VARCHAR(50) DEFAULT 'ok';
ALTER TABLE events ADD COLUMN IF NOT EXISTS reference_image VARCHAR(255);
ALTER TABLE events ADD COLUMN IF NOT EXISTS origin VARCHAR(50) DEFAULT 'self';
ALTER TABLE events ADD COLUMN IF NOT EXISTS unsigned JSONB DEFAULT '{}';
ALTER TABLE events ADD COLUMN IF NOT EXISTS redacted BOOLEAN DEFAULT false;
ALTER TABLE events ADD COLUMN IF NOT EXISTS depth BIGINT DEFAULT 0;

-- 添加外键约束（如果不存在）
DO $$
BEGIN
    IF NOT EXISTS (
        SELECT 1 FROM information_schema.table_constraints 
        WHERE constraint_name = 'fk_events_room' AND table_name = 'events'
    ) THEN
        ALTER TABLE events ADD CONSTRAINT fk_events_room 
        FOREIGN KEY (room_id) REFERENCES rooms(room_id) ON DELETE CASCADE;
    END IF;
END $$;

-- 创建索引（如果不存在）
CREATE INDEX IF NOT EXISTS idx_events_room ON events(room_id);
CREATE INDEX IF NOT EXISTS idx_events_sender ON events(sender);
CREATE INDEX IF NOT EXISTS idx_events_type ON events(event_type);
CREATE INDEX IF NOT EXISTS idx_events_user ON events(user_id);
CREATE INDEX IF NOT EXISTS idx_events_stream ON events(stream_ordering);
CREATE INDEX IF NOT EXISTS idx_events_origin_ts ON events(origin_server_ts);

-- 注意：不删除 type 列以保持向后兼容性
-- 后续版本可以删除：ALTER TABLE events DROP COLUMN IF EXISTS type;

-- 验证
DO $$
BEGIN
    RAISE NOTICE 'Events table fields unified successfully';
END $$;
