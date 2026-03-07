-- 修复数据库 Schema 以符合 DATABASE_FIELD_STANDARDS.md 规范
-- 执行日期: 2026-03-07

-- ============================================
-- 1. 修复 events 表: redacted -> is_redacted
-- ============================================

-- 检查并重命名 redacted 列 (如果存在)
DO $$
BEGIN
    IF EXISTS (
        SELECT 1 FROM information_schema.columns 
        WHERE table_name = 'events' AND column_name = 'redacted'
    ) THEN
        -- 如果 is_redacted 不存在，重命名 redacted
        IF NOT EXISTS (
            SELECT 1 FROM information_schema.columns 
            WHERE table_name = 'events' AND column_name = 'is_redacted'
        ) THEN
            ALTER TABLE events RENAME COLUMN redacted TO is_redacted;
            RAISE NOTICE 'Renamed events.redacted to events.is_redacted';
        END IF;
    END IF;
END $$;

-- ============================================
-- 2. 修复 device_keys 表: created_ts -> added_ts
-- ============================================

-- 检查并重命名 created_ts 列
DO $$
BEGIN
    IF EXISTS (
        SELECT 1 FROM information_schema.columns 
        WHERE table_name = 'device_keys' AND column_name = 'created_ts'
    ) THEN
        -- 如果 added_ts 不存在，重命名
        IF NOT EXISTS (
            SELECT 1 FROM information_schema.columns 
            WHERE table_name = 'device_keys' AND column_name = 'added_ts'
        ) THEN
            ALTER TABLE device_keys RENAME COLUMN created_ts TO added_ts;
            RAISE NOTICE 'Renamed device_keys.created_ts to device_keys.added_ts';
        END IF;
    END IF;
END $$;

-- ============================================
-- 3. 修复 device_keys 表: updated_at -> ts_updated_ms
-- ============================================

-- 检查并重命名 updated_at 列
DO $$
BEGIN
    IF EXISTS (
        SELECT 1 FROM information_schema.columns 
        WHERE table_name = 'device_keys' AND column_name = 'updated_at'
    ) THEN
        -- 如果 ts_updated_ms 不存在，重命名
        IF NOT EXISTS (
            SELECT 1 FROM information_schema.columns 
            WHERE table_name = 'device_keys' AND column_name = 'ts_updated_ms'
        ) THEN
            ALTER TABLE device_keys RENAME COLUMN updated_at TO ts_updated_ms;
            RAISE NOTICE 'Renamed device_keys.updated_at to device_keys.ts_updated_ms';
        END IF;
    END IF;
END $$;

-- ============================================
-- 验证修复结果
-- ============================================

SELECT 'events' as table_name, column_name, data_type 
FROM information_schema.columns 
WHERE table_name = 'events' AND column_name IN ('redacted', 'is_redacted');

SELECT 'device_keys' as table_name, column_name, data_type 
FROM information_schema.columns 
WHERE table_name = 'device_keys' AND column_name IN ('created_ts', 'added_ts', 'updated_at', 'ts_updated_ms');
