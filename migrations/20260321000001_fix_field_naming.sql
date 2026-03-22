-- ============================================================================
-- 修复数据库字段命名不一致 (v2)
-- 执行日期: 2026-03-21
--
-- 问题描述:
--   迁移文件 00000000_unified_schema_v6.sql 中存在旧字段名
--   但代码已使用新字段名，导致数据库实际结构与迁移文件不一致
--
-- 修复内容:
--   1. user_threepids.validated_at → validated_ts
--   2. user_threepids.verification_expires_at → verification_expires_ts
--   3. private_messages.read_at → read_ts
--
-- 状态: 主迁移文件已更新，此脚本保留用于旧数据库升级
-- ============================================================================

-- 1. 修复 user_threepids 表字段
-- ============================================================================

-- 检查并重命名字段 (如果存在旧字段)
DO $$
BEGIN
    -- 重命名 validated_at → validated_ts
    IF EXISTS (
        SELECT 1 FROM information_schema.columns 
        WHERE table_name = 'user_threepids' AND column_name = 'validated_at'
    ) THEN
        ALTER TABLE user_threepids RENAME COLUMN validated_at TO validated_ts;
        RAISE NOTICE 'Renamed user_threepids.validated_at → validated_ts';
    END IF;
    
    -- 重命名 verification_expires_at → verification_expires_ts
    IF EXISTS (
        SELECT 1 FROM information_schema.columns 
        WHERE table_name = 'user_threepids' AND column_name = 'verification_expires_at'
    ) THEN
        ALTER TABLE user_threepids RENAME COLUMN verification_expires_at TO verification_expires_ts;
        RAISE NOTICE 'Renamed user_threepids.verification_expires_at → verification_expires_ts';
    END IF;
END $$;

-- 2. 修复 private_messages 表字段
-- ============================================================================

DO $$
BEGIN
    -- 重命名 read_at → read_ts
    IF EXISTS (
        SELECT 1 FROM information_schema.columns 
        WHERE table_name = 'private_messages' AND column_name = 'read_at'
    ) THEN
        ALTER TABLE private_messages RENAME COLUMN read_at TO read_ts;
        RAISE NOTICE 'Renamed private_messages.read_at → read_ts';
    END IF;
END $$;

-- 3. 验证修复结果
-- ============================================================================

DO $$
DECLARE
    v_count INTEGER := 0;
BEGIN
    -- 检查是否还有遗留的旧字段名
    SELECT COUNT(*) INTO v_count FROM information_schema.columns 
    WHERE table_schema = 'public' 
    AND (
        (table_name = 'user_threepids' AND column_name IN ('validated_at', 'verification_expires_at'))
        OR (table_name = 'private_messages' AND column_name = 'read_at')
    );
    
    IF v_count = 0 THEN
        RAISE NOTICE '✅ All field naming fixes completed successfully';
    ELSE
        RAISE WARNING '⚠️ Still have % old field names', v_count;
    END IF;
END $$;

-- ============================================================================
-- [SUCCESS] 字段命名修复迁移完成
-- ============================================================================
