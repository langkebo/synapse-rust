-- Migration: Add missing columns to rooms table
-- Version: 20260220000004
-- Description: 添加 rooms 表缺失的列以支持公开房间列表 API 和房间创建

-- 添加 name 列
DO $$
BEGIN
    IF NOT EXISTS (SELECT 1 FROM information_schema.columns WHERE table_name = 'rooms' AND column_name = 'name') THEN
        ALTER TABLE rooms ADD COLUMN name VARCHAR(255);
        RAISE NOTICE 'Added name column to rooms table';
    END IF;
END $$;

-- 添加 topic 列
DO $$
BEGIN
    IF NOT EXISTS (SELECT 1 FROM information_schema.columns WHERE table_name = 'rooms' AND column_name = 'topic') THEN
        ALTER TABLE rooms ADD COLUMN topic TEXT;
        RAISE NOTICE 'Added topic column to rooms table';
    END IF;
END $$;

-- 添加 avatar_url 列
DO $$
BEGIN
    IF NOT EXISTS (SELECT 1 FROM information_schema.columns WHERE table_name = 'rooms' AND column_name = 'avatar_url') THEN
        ALTER TABLE rooms ADD COLUMN avatar_url VARCHAR(512);
        RAISE NOTICE 'Added avatar_url column to rooms table';
    END IF;
END $$;

-- 添加 canonical_alias 列
DO $$
BEGIN
    IF NOT EXISTS (SELECT 1 FROM information_schema.columns WHERE table_name = 'rooms' AND column_name = 'canonical_alias') THEN
        ALTER TABLE rooms ADD COLUMN canonical_alias VARCHAR(255);
        RAISE NOTICE 'Added canonical_alias column to rooms table';
    END IF;
END $$;

-- 添加 join_rule 列 (与现有的 join_rules 区分)
DO $$
BEGIN
    IF NOT EXISTS (SELECT 1 FROM information_schema.columns WHERE table_name = 'rooms' AND column_name = 'join_rule') THEN
        ALTER TABLE rooms ADD COLUMN join_rule VARCHAR(50) DEFAULT 'invite';
        RAISE NOTICE 'Added join_rule column to rooms table';
    END IF;
END $$;

-- 添加 version 列 (与现有的 room_version 区分)
DO $$
BEGIN
    IF NOT EXISTS (SELECT 1 FROM information_schema.columns WHERE table_name = 'rooms' AND column_name = 'version') THEN
        ALTER TABLE rooms ADD COLUMN version VARCHAR(50) DEFAULT '6';
        RAISE NOTICE 'Added version column to rooms table';
    END IF;
END $$;

-- 添加 encryption 列
DO $$
BEGIN
    IF NOT EXISTS (SELECT 1 FROM information_schema.columns WHERE table_name = 'rooms' AND column_name = 'encryption') THEN
        ALTER TABLE rooms ADD COLUMN encryption VARCHAR(50);
        RAISE NOTICE 'Added encryption column to rooms table';
    END IF;
END $$;

-- 添加 member_count 列
DO $$
BEGIN
    IF NOT EXISTS (SELECT 1 FROM information_schema.columns WHERE table_name = 'rooms' AND column_name = 'member_count') THEN
        ALTER TABLE rooms ADD COLUMN member_count BIGINT DEFAULT 0;
        RAISE NOTICE 'Added member_count column to rooms table';
    END IF;
END $$;

-- 添加 history_visibility 列
DO $$
BEGIN
    IF NOT EXISTS (SELECT 1 FROM information_schema.columns WHERE table_name = 'rooms' AND column_name = 'history_visibility') THEN
        ALTER TABLE rooms ADD COLUMN history_visibility VARCHAR(50) DEFAULT 'shared';
        RAISE NOTICE 'Added history_visibility column to rooms table';
    END IF;
END $$;

-- 添加 creation_ts 列 (如果不存在)
DO $$
BEGIN
    IF NOT EXISTS (SELECT 1 FROM information_schema.columns WHERE table_name = 'rooms' AND column_name = 'creation_ts') THEN
        ALTER TABLE rooms ADD COLUMN creation_ts BIGINT DEFAULT (EXTRACT(EPOCH FROM NOW()) * 1000)::BIGINT;
        RAISE NOTICE 'Added creation_ts column to rooms table';
    END IF;
END $$;

-- 添加 last_activity_ts 列
DO $$
BEGIN
    IF NOT EXISTS (SELECT 1 FROM information_schema.columns WHERE table_name = 'rooms' AND column_name = 'last_activity_ts') THEN
        ALTER TABLE rooms ADD COLUMN last_activity_ts BIGINT DEFAULT (EXTRACT(EPOCH FROM NOW()) * 1000)::BIGINT;
        RAISE NOTICE 'Added last_activity_ts column to rooms table';
    END IF;
END $$;

-- 创建索引
CREATE INDEX IF NOT EXISTS idx_rooms_name ON rooms(name);
CREATE INDEX IF NOT EXISTS idx_rooms_is_public ON rooms(is_public);

-- 验证
DO $$
DECLARE
    col_count INTEGER;
BEGIN
    SELECT COUNT(*) INTO col_count
    FROM information_schema.columns
    WHERE table_name = 'rooms'
    AND column_name IN ('room_id', 'name', 'topic', 'avatar_url', 'canonical_alias', 
                         'join_rule', 'creator', 'version', 'encryption', 'is_public', 
                         'member_count', 'history_visibility', 'creation_ts', 'last_activity_ts');
    
    IF col_count >= 14 THEN
        RAISE NOTICE 'rooms table structure is correct for public rooms API';
    ELSE
        RAISE WARNING 'rooms table may be missing some columns (found %)', col_count;
    END IF;
END $$;
