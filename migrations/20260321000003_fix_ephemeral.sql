-- ============================================================================
-- 修复数据库字段命名 - room_ephemeral
-- 执行日期: 2026-03-21
--
-- 问题: Sync API 返回 500 错误
-- 原因: 代码使用 expires_at 但数据库使用 expires_ts
-- ============================================================================

-- 添加计算列 expires_at (基于 expires_ts)
DO $$
BEGIN
    IF NOT EXISTS (
        SELECT 1 FROM information_schema.columns 
        WHERE table_name = 'room_ephemeral' AND column_name = 'expires_at'
    ) THEN
        ALTER TABLE room_ephemeral ADD COLUMN expires_at BIGINT GENERATED ALWAYS AS (expires_ts) STORED;
        RAISE NOTICE 'Added room_ephemeral.expires_at computed column';
    ELSE
        RAISE NOTICE 'room_ephemeral.expires_at already exists';
    END IF;
END $$;

-- ============================================================================
-- [SUCCESS] room_ephemeral 字段修复完成
-- ============================================================================
