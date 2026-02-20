-- =============================================================================
-- 数据库字段规范化迁移脚本 - 第三阶段 (修复版 v2)
-- 版本: 3.0.2
-- 创建日期: 2026-02-20
-- 描述: 统一时间字段类型 (TIMESTAMP -> BIGINT) 和后缀 (_at -> _ts)
-- 修复: 处理有默认值的列，添加条件检查避免重复操作
-- =============================================================================

BEGIN;

-- =============================================================================
-- 第一部分: 时间字段类型统一 (TIMESTAMP WITH TIME ZONE -> BIGINT)
-- =============================================================================

DO $$
DECLARE
    tbl RECORD;
    col RECORD;
BEGIN
    -- 遍历所有需要转换的 timestamp 列
    FOR tbl IN 
        SELECT DISTINCT table_name 
        FROM information_schema.columns 
        WHERE table_schema = 'public' 
        AND column_name IN ('created_at', 'updated_at')
        AND data_type = 'timestamp with time zone'
    LOOP
        FOR col IN 
            SELECT column_name, column_default
            FROM information_schema.columns 
            WHERE table_schema = 'public' 
            AND table_name = tbl.table_name
            AND column_name IN ('created_at', 'updated_at')
            AND data_type = 'timestamp with time zone'
        LOOP
            -- 先删除默认值
            EXECUTE format('ALTER TABLE %I ALTER COLUMN %I DROP DEFAULT', tbl.table_name, col.column_name);
            
            -- 转换类型
            EXECUTE format('ALTER TABLE %I ALTER COLUMN %I TYPE BIGINT USING (EXTRACT(EPOCH FROM COALESCE(%I, NOW())) * 1000)::BIGINT', 
                tbl.table_name, col.column_name, col.column_name);
            
            RAISE NOTICE '转换表 % 列 % 完成', tbl.table_name, col.column_name;
        END LOOP;
    END LOOP;
    
    RAISE NOTICE '时间字段类型转换完成';
END $$;

-- =============================================================================
-- 第二部分: 时间字段后缀统一 (_at -> _ts)
-- =============================================================================

DO $$
DECLARE
    tbl RECORD;
    col RECORD;
    new_name TEXT;
BEGIN
    -- 遍历所有需要重命名的 _at 列 (BIGINT 类型)
    FOR col IN 
        SELECT table_name, column_name
        FROM information_schema.columns 
        WHERE table_schema = 'public' 
        AND column_name LIKE '%_at'
        AND data_type = 'bigint'
        AND column_name NOT IN ('expires_at', 'last_used_at', 'read_at', 'dismissed_at', 'last_request_at', 'last_success_at', 'last_failure_at')
    LOOP
        -- 计算新列名
        new_name := replace(col.column_name, '_at', '_ts');
        
        -- 检查目标列是否已存在
        IF NOT EXISTS (
            SELECT 1 FROM information_schema.columns 
            WHERE table_schema = 'public' 
            AND table_name = col.table_name 
            AND column_name = new_name
        ) THEN
            -- 重命名列
            EXECUTE format('ALTER TABLE %I RENAME COLUMN %I TO %s', 
                col.table_name, col.column_name, new_name);
            
            RAISE NOTICE '重命名表 % 列 % 完成', col.table_name, col.column_name;
        ELSE
            -- 如果目标列已存在，删除源列
            EXECUTE format('ALTER TABLE %I DROP COLUMN %I', col.table_name, col.column_name);
            RAISE NOTICE '删除表 % 冗余列 % (目标列已存在)', col.table_name, col.column_name;
        END IF;
    END LOOP;
    
    RAISE NOTICE '时间字段后缀统一完成';
END $$;

-- 特殊处理: blocked_at -> blocked_ts (即使不是 bigint 类型)
DO $$
BEGIN
    -- blocked_rooms 表
    IF EXISTS (SELECT 1 FROM information_schema.columns WHERE table_name = 'blocked_rooms' AND column_name = 'blocked_at') THEN
        ALTER TABLE blocked_rooms RENAME COLUMN blocked_at TO blocked_ts;
    END IF;
    
    -- federation_blacklist 表
    IF EXISTS (SELECT 1 FROM information_schema.columns WHERE table_name = 'federation_blacklist' AND column_name = 'blocked_at') THEN
        ALTER TABLE federation_blacklist RENAME COLUMN blocked_at TO blocked_ts;
    END IF;
END $$;

-- 删除 refresh_tokens 表中的冗余布尔字段 invalidated (已由 is_revoked 替代)
ALTER TABLE refresh_tokens DROP COLUMN IF EXISTS invalidated;

-- 删除 refresh_tokens 表中的冗余字段 token (已由 token_hash 替代)
ALTER TABLE refresh_tokens DROP COLUMN IF EXISTS token;

-- =============================================================================
-- 更新版本记录
-- =============================================================================

INSERT INTO schema_migrations (version, description, success)
VALUES ('3.0.2', 'Phase 3 (Fixed v2): Timestamp type and suffix normalization', TRUE)
ON CONFLICT (version) DO UPDATE SET success = TRUE, executed_at = NOW();

UPDATE db_metadata SET value = '3.0.2', updated_ts = (EXTRACT(EPOCH FROM NOW()) * 1000)::BIGINT
WHERE key = 'schema_version';

COMMIT;

-- =============================================================================
-- 验证脚本
-- =============================================================================

-- 验证时间字段类型 (应该返回空结果)
SELECT table_name, column_name, data_type 
FROM information_schema.columns 
WHERE table_schema = 'public' 
AND column_name LIKE '%_ts'
AND data_type != 'bigint'
ORDER BY table_name, column_name;

-- 验证没有遗留的 _at 后缀时间字段 (BIGINT 类型，排除特殊字段)
SELECT table_name, column_name, data_type 
FROM information_schema.columns 
WHERE table_schema = 'public' 
AND column_name LIKE '%_at'
AND data_type = 'bigint'
AND column_name NOT IN ('expires_at', 'last_used_at', 'read_at', 'dismissed_at', 'last_request_at', 'last_success_at', 'last_failure_at')
ORDER BY table_name, column_name;
