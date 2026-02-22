-- Migration: Fix devices table column names
-- Version: 20260220000003
-- Description: 统一 devices 表字段命名，移除 created_at，使用 created_ts
-- 修复: 移除顶层事务，每个DO块独立执行

-- 检查并删除 created_at 列（如果存在）
DO $$
BEGIN
    IF EXISTS (
        SELECT 1 FROM information_schema.columns 
        WHERE table_name = 'devices' AND column_name = 'created_at'
    ) THEN
        ALTER TABLE devices DROP COLUMN created_at;
        RAISE NOTICE 'Dropped created_at column from devices table';
    END IF;
END $$;

-- 确保 created_ts 列存在
DO $$
BEGIN
    IF NOT EXISTS (
        SELECT 1 FROM information_schema.columns 
        WHERE table_name = 'devices' AND column_name = 'created_ts'
    ) THEN
        ALTER TABLE devices ADD COLUMN created_ts BIGINT NOT NULL DEFAULT (EXTRACT(EPOCH FROM NOW()) * 1000)::BIGINT;
        RAISE NOTICE 'Added created_ts column to devices table';
    END IF;
END $$;

-- 确保 first_seen_ts 列存在
DO $$
BEGIN
    IF NOT EXISTS (
        SELECT 1 FROM information_schema.columns 
        WHERE table_name = 'devices' AND column_name = 'first_seen_ts'
    ) THEN
        ALTER TABLE devices ADD COLUMN first_seen_ts BIGINT NOT NULL DEFAULT (EXTRACT(EPOCH FROM NOW()) * 1000)::BIGINT;
        RAISE NOTICE 'Added first_seen_ts column to devices table';
    END IF;
END $$;

-- 确保 last_seen_ts 列存在
DO $$
BEGIN
    IF NOT EXISTS (
        SELECT 1 FROM information_schema.columns 
        WHERE table_name = 'devices' AND column_name = 'last_seen_ts'
    ) THEN
        ALTER TABLE devices ADD COLUMN last_seen_ts BIGINT;
        RAISE NOTICE 'Added last_seen_ts column to devices table';
    END IF;
END $$;

-- 确保 last_seen_ip 列存在
DO $$
BEGIN
    IF NOT EXISTS (
        SELECT 1 FROM information_schema.columns 
        WHERE table_name = 'devices' AND column_name = 'last_seen_ip'
    ) THEN
        ALTER TABLE devices ADD COLUMN last_seen_ip VARCHAR(45);
        RAISE NOTICE 'Added last_seen_ip column to devices table';
    END IF;
END $$;

-- 创建索引（如果不存在）
CREATE INDEX IF NOT EXISTS idx_devices_user ON devices(user_id);
CREATE INDEX IF NOT EXISTS idx_devices_last_seen ON devices(last_seen_ts DESC);

-- 验证表结构
DO $$
DECLARE
    col_count INTEGER;
BEGIN
    SELECT COUNT(*) INTO col_count
    FROM information_schema.columns
    WHERE table_name = 'devices'
    AND column_name IN ('device_id', 'user_id', 'display_name', 'created_ts', 'first_seen_ts', 'last_seen_ts');
    
    IF col_count >= 6 THEN
        RAISE NOTICE 'devices table structure is correct';
    ELSE
        RAISE WARNING 'devices table may be missing some columns';
    END IF;
END $$;
