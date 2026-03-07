-- 数据库 Schema 检查和修复脚本
-- 用于同步 Docker 镜像数据库与代码的差异
-- 运行方式: docker compose exec db psql -U synapse -d synapse_test -f /path/to/fix_schema.sql

-- ============================================
-- 1. 检查并添加 room_memberships 表缺失列
-- ============================================

-- ban_ts 列
DO $$
BEGIN
    IF NOT EXISTS (
        SELECT 1 FROM information_schema.columns 
        WHERE table_name = 'room_memberships' AND column_name = 'ban_ts'
    ) THEN
        ALTER TABLE room_memberships ADD COLUMN ban_ts BIGINT;
        RAISE NOTICE 'Added column: room_memberships.ban_ts';
    ELSE
        RAISE NOTICE 'Column already exists: room_memberships.ban_ts';
    END IF;
END $$;

-- join_reason 列
DO $$
BEGIN
    IF NOT EXISTS (
        SELECT 1 FROM information_schema.columns 
        WHERE table_name = 'room_memberships' AND column_name = 'join_reason'
    ) THEN
        ALTER TABLE room_memberships ADD COLUMN join_reason TEXT;
        RAISE NOTICE 'Added column: room_memberships.join_reason';
    ELSE
        RAISE NOTICE 'Column already exists: room_memberships.join_reason';
    END IF;
END $$;

-- ============================================
-- 2. 检查 events 表缺失列
-- ============================================

-- redacted/is_redacted 列
DO $$
BEGIN
    IF NOT EXISTS (
        SELECT 1 FROM information_schema.columns 
        WHERE table_name = 'events' AND column_name = 'is_redacted'
    ) AND NOT EXISTS (
        SELECT 1 FROM information_schema.columns 
        WHERE table_name = 'events' AND column_name = 'redacted'
    ) THEN
        ALTER TABLE events ADD COLUMN is_redacted BOOLEAN DEFAULT FALSE;
        RAISE NOTICE 'Added column: events.is_redacted';
    END IF;
END $$;

-- ============================================
-- 3. 检查 device_keys 表缺失列
-- ============================================

-- added_ts 列 (原 created_ts)
DO $$
BEGIN
    IF NOT EXISTS (
        SELECT 1 FROM information_schema.columns 
        WHERE table_name = 'device_keys' AND column_name = 'added_ts'
    ) AND NOT EXISTS (
        SELECT 1 FROM information_schema.columns 
        WHERE table_name = 'device_keys' AND column_name = 'created_ts'
    ) THEN
        ALTER TABLE device_keys ADD COLUMN added_ts BIGINT;
        RAISE NOTICE 'Added column: device_keys.added_ts';
    END IF;
END $$;

-- ts_updated_ms 列 (原 updated_at)
DO $$
BEGIN
    IF NOT EXISTS (
        SELECT 1 FROM information_schema.columns 
        WHERE table_name = 'device_keys' AND column_name = 'ts_updated_ms'
    ) AND NOT EXISTS (
        SELECT 1 FROM information_schema.columns 
        WHERE table_name = 'device_keys' AND column_name = 'updated_at'
    ) THEN
        ALTER TABLE device_keys ADD COLUMN ts_updated_ms BIGINT;
        RAISE NOTICE 'Added column: device_keys.ts_updated_ms';
    END IF;
END $$;

-- ============================================
-- 4. 验证所有关键表和列
-- ============================================

SELECT '=== room_memberships ===' as info;
SELECT column_name, data_type 
FROM information_schema.columns 
WHERE table_name = 'room_memberships' 
ORDER BY ordinal_position;

SELECT '=== events ===' as info;
SELECT column_name, data_type 
FROM information_schema.columns 
WHERE table_name = 'events' AND column_name IN ('is_redacted', 'redacted');

SELECT '=== device_keys ===' as info;
SELECT column_name, data_type 
FROM information_schema.columns 
WHERE table_name = 'device_keys' AND column_name IN ('added_ts', 'created_ts', 'ts_updated_ms', 'updated_at');
