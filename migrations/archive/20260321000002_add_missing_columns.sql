-- ============================================================================
-- 添加缺失的数据库列
-- 执行日期: 2026-03-21
--
-- 问题描述:
--   后端代码使用某些字段，但迁移文件中未定义
--   导致运行时出现 "column does not exist" 错误
--
-- 修复内容:
--   1. users.is_password_change_required (login 功能必需)
--   2. events.processed_ts (房间状态功能必需)
-- ============================================================================

-- 1. 添加 users.is_password_change_required 列
-- ============================================================================

DO $$
BEGIN
    IF NOT EXISTS (
        SELECT 1 FROM information_schema.columns 
        WHERE table_name = 'users' AND column_name = 'is_password_change_required'
    ) THEN
        ALTER TABLE users ADD COLUMN is_password_change_required BOOLEAN NOT NULL DEFAULT FALSE;
        RAISE NOTICE 'Added users.is_password_change_required column';
    ELSE
        RAISE NOTICE 'users.is_password_change_required already exists';
    END IF;
END $$;

-- 2. 添加 events.processed_ts 列
-- ============================================================================

DO $$
BEGIN
    IF NOT EXISTS (
        SELECT 1 FROM information_schema.columns 
        WHERE table_name = 'events' AND column_name = 'processed_ts'
    ) THEN
        ALTER TABLE events ADD COLUMN processed_ts BIGINT;
        RAISE NOTICE 'Added events.processed_ts column';
    ELSE
        RAISE NOTICE 'events.processed_ts already exists';
    END IF;
END $$;

-- 3. 验证修复结果
-- ============================================================================

DO $$
DECLARE
    v_users_col BOOLEAN;
    v_events_col BOOLEAN;
BEGIN
    SELECT EXISTS(
        SELECT 1 FROM information_schema.columns 
        WHERE table_name = 'users' AND column_name = 'is_password_change_required'
    ) INTO v_users_col;
    
    SELECT EXISTS(
        SELECT 1 FROM information_schema.columns 
        WHERE table_name = 'events' AND column_name = 'processed_ts'
    ) INTO v_events_col;
    
    IF v_users_col AND v_events_col THEN
        RAISE NOTICE '✅ All missing columns added successfully';
    ELSE
        RAISE WARNING '⚠️ Some columns may be missing';
    END IF;
END $$;

-- ============================================================================
-- [SUCCESS] 缺失列添加迁移完成
-- ============================================================================
