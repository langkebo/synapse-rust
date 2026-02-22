-- Migration: Add missing columns to events table
-- Version: 20260220000007
-- Description: 添加 events 表缺失的列以支持现有代码结构

-- 添加 user_id 列
ALTER TABLE events ADD COLUMN IF NOT EXISTS user_id VARCHAR(255);

-- 添加 processed_ts 列
ALTER TABLE events ADD COLUMN IF NOT EXISTS processed_ts BIGINT;

-- 添加 not_before 列
ALTER TABLE events ADD COLUMN IF NOT EXISTS not_before BIGINT DEFAULT 0;

-- 添加 status 列
ALTER TABLE events ADD COLUMN IF NOT EXISTS status VARCHAR(50) DEFAULT 'ok';

-- 添加 reference_image 列
ALTER TABLE events ADD COLUMN IF NOT EXISTS reference_image VARCHAR(255);

-- 添加 origin 列
ALTER TABLE events ADD COLUMN IF NOT EXISTS origin VARCHAR(50) DEFAULT 'self';

-- 添加 unsigned 列
ALTER TABLE events ADD COLUMN IF NOT EXISTS unsigned JSONB DEFAULT '{}';

-- 从 sender 复制数据到 user_id
UPDATE events SET user_id = sender WHERE user_id IS NULL;

-- 验证
DO $$
BEGIN
    RAISE NOTICE 'Events table columns added successfully';
END $$;
