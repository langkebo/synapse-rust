-- =====================================================
-- 综合迁移文件 (v6.0.4)
-- 文件: 20260316000000_comprehensive_migration.sql
-- 日期: 2026-03-14
--
-- 本文件整合了所有增量迁移的内容，用于快速部署
--  idempotent - 可重复执行
-- =====================================================

BEGIN;

-- =====================================================
-- 1. 字段命名一致性修复
-- =====================================================

-- 修复 user_threepids 表: validated_ts -> validated_at
DO $$
BEGIN
    IF EXISTS (
        SELECT 1 FROM information_schema.columns 
        WHERE table_name = 'user_threepids' AND column_name = 'validated_ts'
    ) THEN
        IF NOT EXISTS (
            SELECT 1 FROM information_schema.columns 
            WHERE table_name = 'user_threepids' AND column_name = 'validated_at'
        ) THEN
            ALTER TABLE user_threepids RENAME COLUMN validated_ts TO validated_at;
        ELSE
            ALTER TABLE user_threepids DROP COLUMN IF EXISTS validated_ts;
        END IF;
    END IF;
END $$;

-- 修复 registration_tokens 表: last_used_at -> last_used_ts
DO $$
BEGIN
    IF EXISTS (
        SELECT 1 FROM information_schema.columns 
        WHERE table_name = 'registration_tokens' AND column_name = 'last_used_at'
    ) THEN
        IF NOT EXISTS (
            SELECT 1 FROM information_schema.columns 
            WHERE table_name = 'registration_tokens' AND column_name = 'last_used_ts'
        ) THEN
            ALTER TABLE registration_tokens RENAME COLUMN last_used_at TO last_used_ts;
        ELSE
            ALTER TABLE registration_tokens DROP COLUMN IF EXISTS last_used_at;
        END IF;
    END IF;
END $$;

-- 修复 users 表索引 (password_expires_ts -> password_expires_at)
DO $$
BEGIN
    -- 删除旧索引
    DROP INDEX IF EXISTS idx_users_password_expires_old;
    
    -- 检查并重命名索引
    IF EXISTS (
        SELECT 1 FROM pg_indexes 
        WHERE indexname = 'idx_users_password_expires' 
        AND indexdef LIKE '%password_expires_ts%'
    ) THEN
        ALTER INDEX IF EXISTS idx_users_password_expires RENAME TO idx_users_password_expires_old;
    END IF;
    
    -- 创建新索引
    CREATE INDEX IF NOT EXISTS idx_users_password_expires 
    ON users(password_expires_at) WHERE password_expires_at IS NOT NULL;
END $$;

-- =====================================================
-- 2. 验证修复结果
-- =====================================================

SELECT 'user_threepids' AS table_name, column_name
FROM information_schema.columns 
WHERE table_name = 'user_threepids' 
AND column_name LIKE '%validat%';

SELECT 'registration_tokens' AS table_name, column_name
FROM information_schema.columns 
WHERE table_name = 'registration_tokens' 
AND column_name LIKE '%last_used%';

COMMIT;

-- =====================================================
-- 3. 最终验证
-- =====================================================

DO $$
DECLARE
    issues INTEGER := 0;
BEGIN
    -- 检查 users 表
    IF EXISTS (SELECT 1 FROM information_schema.columns WHERE table_name = 'users' AND column_name = 'password_expires_ts') THEN
        RAISE WARNING '⚠ users.password_expires_ts should be password_expires_at';
        issues := issues + 1;
    END IF;
    
    -- 检查 user_threepids 表  
    IF EXISTS (SELECT 1 FROM information_schema.columns WHERE table_name = 'user_threepids' AND column_name = 'validated_ts') THEN
        RAISE WARNING '⚠ user_threepids.validated_ts should be validated_at';
        issues := issues + 1;
    END IF;
    
    -- 检查 registration_tokens 表
    IF EXISTS (SELECT 1 FROM information_schema.columns WHERE table_name = 'registration_tokens' AND column_name = 'last_used_at') THEN
        RAISE WARNING '⚠ registration_tokens.last_used_at should be last_used_ts';
        issues := issues + 1;
    END IF;
    
    IF issues = 0 THEN
        RAISE NOTICE '✓ 所有字段一致性问题已修复!';
    ELSE
        RAISE WARNING '⚠ 还有 % 个问题未修复', issues;
    END IF;
END $$;
