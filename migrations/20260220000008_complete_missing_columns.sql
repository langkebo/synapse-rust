-- Migration: Complete missing columns for all tables
-- Version: 20260220000008
-- Description: 添加所有缺失的列以支持现有代码结构

-- Events 表
ALTER TABLE events ADD COLUMN IF NOT EXISTS user_id VARCHAR(255);
ALTER TABLE events ADD COLUMN IF NOT EXISTS processed_ts BIGINT;
ALTER TABLE events ADD COLUMN IF NOT EXISTS not_before BIGINT DEFAULT 0;
ALTER TABLE events ADD COLUMN IF NOT EXISTS status VARCHAR(50) DEFAULT 'ok';
ALTER TABLE events ADD COLUMN IF NOT EXISTS reference_image VARCHAR(255);
ALTER TABLE events ADD COLUMN IF NOT EXISTS origin VARCHAR(50) DEFAULT 'self';
ALTER TABLE events ADD COLUMN IF NOT EXISTS unsigned JSONB DEFAULT '{}';
ALTER TABLE events ADD COLUMN IF NOT EXISTS redacted BOOLEAN DEFAULT false;
UPDATE events SET user_id = sender WHERE user_id IS NULL;

-- Presence 表
ALTER TABLE presence ADD COLUMN IF NOT EXISTS created_ts BIGINT DEFAULT (EXTRACT(EPOCH FROM NOW()) * 1000)::BIGINT;
ALTER TABLE presence ADD COLUMN IF NOT EXISTS updated_ts BIGINT DEFAULT (EXTRACT(EPOCH FROM NOW()) * 1000)::BIGINT;

-- 验证
DO $$
BEGIN
    RAISE NOTICE 'All missing columns added successfully';
END $$;
