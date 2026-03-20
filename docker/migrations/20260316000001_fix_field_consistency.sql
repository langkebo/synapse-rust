-- =====================================================
-- 数据库字段一致性修复迁移
-- 文件: 20260316000001_fix_field_consistency.sql
-- 日期: 2026-03-14
-- 描述: 修复字段命名不一致问题，统一使用规范定义的字段名
--
-- 修复内容:
-- 1. users.password_expires_at (保持不变，已正确)
-- 2. user_threepids.validated_ts -> validated_at
-- 3. registration_tokens.last_used_at -> last_used_ts
--
-- 注意: refresh_tokens 表已使用 last_used_ts，无需修改
-- =====================================================

BEGIN;

-- =====================================================
-- 1. 修复 user_threepids 表
-- =====================================================

-- 检查并修复 validated_ts -> validated_at
DO $$
BEGIN
    IF EXISTS (
        SELECT 1 FROM information_schema.columns 
        WHERE table_name = 'user_threepids' AND column_name = 'validated_ts'
    ) THEN
        -- 检查是否已有 validated_at 字段
        IF NOT EXISTS (
            SELECT 1 FROM information_schema.columns 
            WHERE table_name = 'user_threepids' AND column_name = 'validated_at'
        ) THEN
            ALTER TABLE user_threepids RENAME COLUMN validated_ts TO validated_at;
            RAISE NOTICE 'Renamed validated_ts to validated_at in user_threepids';
        ELSE
            -- 如果已有 validated_at，删除 validated_ts
            ALTER TABLE user_threepids DROP COLUMN IF EXISTS validated_ts;
            RAISE NOTICE 'Dropped redundant validated_ts column';
        END IF;
    ELSE
        RAISE NOTICE 'Column validated_ts does not exist in user_threepids, skipping';
    END IF;
END $$;

-- =====================================================
-- 2. 修复 registration_tokens 表
-- =====================================================

-- 检查并修复 last_used_at -> last_used_ts
DO $$
BEGIN
    IF EXISTS (
        SELECT 1 FROM information_schema.columns 
        WHERE table_name = 'registration_tokens' AND column_name = 'last_used_at'
    ) THEN
        -- 检查是否已有 last_used_ts 字段
        IF NOT EXISTS (
            SELECT 1 FROM information_schema.columns 
            WHERE table_name = 'registration_tokens' AND column_name = 'last_used_ts'
        ) THEN
            ALTER TABLE registration_tokens RENAME COLUMN last_used_at TO last_used_ts;
            RAISE NOTICE 'Renamed last_used_at to last_used_ts in registration_tokens';
        ELSE
            -- 如果已有 last_used_ts，删除 last_used_at
            ALTER TABLE registration_tokens DROP COLUMN IF EXISTS last_used_at;
            RAISE NOTICE 'Dropped redundant last_used_at column';
        END IF;
    ELSE
        RAISE NOTICE 'Column last_used_at does not exist in registration_tokens, skipping';
    END IF;
END $$;

-- =====================================================
-- 3. 验证修复结果
-- =====================================================

-- 验证 user_threepids 表
SELECT 
    'user_threepids' AS table_name,
    column_name,
    data_type,
    is_nullable
FROM information_schema.columns 
WHERE table_name = 'user_threepids' 
AND column_name IN ('validated_at', 'validated_ts')
ORDER BY column_name;

-- 验证 registration_tokens 表
SELECT 
    'registration_tokens' AS table_name,
    column_name,
    data_type,
    is_nullable
FROM information_schema.columns 
WHERE table_name = 'registration_tokens' 
AND column_name IN ('last_used_at', 'last_used_ts')
ORDER BY column_name;

-- =====================================================
-- 4. 代码修复提示
-- =====================================================

-- 修复后，代码中的字段映射应保持不变：
-- user_threepids: pub validated_at: Option<i64>
-- registration_tokens: pub last_used_ts: Option<i64>

COMMIT;

-- =====================================================
-- 5. 字段一致性最终验证
-- =====================================================

DO $$
DECLARE
    inconsistency_count INTEGER := 0;
BEGIN
    -- 检查 users 表
    IF EXISTS (SELECT 1 FROM information_schema.columns WHERE table_name = 'users' AND column_name = 'password_expires_ts') THEN
        RAISE WARNING 'users table still has password_expires_ts (should be password_expires_at)';
        inconsistency_count := inconsistency_count + 1;
    END IF;
    
    -- 检查 user_threepids 表  
    IF EXISTS (SELECT 1 FROM information_schema.columns WHERE table_name = 'user_threepids' AND column_name = 'validated_ts') THEN
        RAISE WARNING 'user_threepids table still has validated_ts (should be validated_at)';
        inconsistency_count := inconsistency_count + 1;
    END IF;
    
    -- 检查 registration_tokens 表
    IF EXISTS (SELECT 1 FROM information_schema.columns WHERE table_name = 'registration_tokens' AND column_name = 'last_used_at') THEN
        RAISE WARNING 'registration_tokens table still has last_used_at (should be last_used_ts)';
        inconsistency_count := inconsistency_count + 1;
    END IF;
    
    IF inconsistency_count = 0 THEN
        RAISE NOTICE '✓ All field consistency issues resolved!';
    ELSE
        RAISE WARNING '⚠ Found % remaining inconsistencies', inconsistency_count;
    END IF;
END $$;
