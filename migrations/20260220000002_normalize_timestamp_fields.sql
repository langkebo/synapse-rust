-- =============================================================================
-- 数据库字段规范化迁移脚本 - 第三阶段 (修复版 v3)
-- 版本: 3.0.3
-- 创建日期: 2026-02-20
-- 描述: 统一时间字段类型 (TIMESTAMP -> BIGINT) 和后缀 (_at -> _ts)
-- 修复: 移除顶层事务，每个DO块独立执行
-- =============================================================================

-- =============================================================================
-- 第一部分: 时间字段类型统一 (TIMESTAMP WITH TIME ZONE -> BIGINT)
-- =============================================================================

DO $$
DECLARE
    tbl RECORD;
    col RECORD;
BEGIN
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
            BEGIN
                EXECUTE format('ALTER TABLE %I ALTER COLUMN %I DROP DEFAULT', tbl.table_name, col.column_name);
                EXECUTE format('ALTER TABLE %I ALTER COLUMN %I TYPE BIGINT USING (EXTRACT(EPOCH FROM COALESCE(%I, NOW())) * 1000)::BIGINT', 
                    tbl.table_name, col.column_name, col.column_name);
                RAISE NOTICE '转换表 % 列 % 完成', tbl.table_name, col.column_name;
            EXCEPTION WHEN OTHERS THEN
                RAISE NOTICE '转换表 % 列 % 跳过: %', tbl.table_name, col.column_name, SQLERRM;
            END;
        END LOOP;
    END LOOP;
    RAISE NOTICE '时间字段类型转换完成';
END $$;

-- =============================================================================
-- 第二部分: 时间字段重命名 (_at -> _ts)
-- =============================================================================

DO $$
BEGIN
    IF EXISTS (SELECT 1 FROM information_schema.columns WHERE table_name = 'devices' AND column_name = 'created_at') THEN
        ALTER TABLE devices RENAME COLUMN created_at TO created_ts;
    END IF;
END $$;

DO $$
BEGIN
    IF EXISTS (SELECT 1 FROM information_schema.columns WHERE table_name = 'federation_signing_keys' AND column_name = 'created_at') THEN
        ALTER TABLE federation_signing_keys RENAME COLUMN created_at TO created_ts;
    END IF;
END $$;

DO $$
BEGIN
    IF EXISTS (SELECT 1 FROM information_schema.columns WHERE table_name = 'ip_reputation' AND column_name = 'last_failed_at') THEN
        ALTER TABLE ip_reputation RENAME COLUMN last_failed_at TO last_failed_ts;
    END IF;
END $$;

DO $$
BEGIN
    IF EXISTS (SELECT 1 FROM information_schema.columns WHERE table_name = 'ip_reputation' AND column_name = 'last_success_at') THEN
        ALTER TABLE ip_reputation RENAME COLUMN last_success_at TO last_success_ts;
    END IF;
END $$;

DO $$
BEGIN
    IF EXISTS (SELECT 1 FROM information_schema.columns WHERE table_name = 'ip_reputation' AND column_name = 'blocked_at') THEN
        ALTER TABLE ip_reputation RENAME COLUMN blocked_at TO blocked_ts;
    END IF;
END $$;

-- =============================================================================
-- 完成
-- =============================================================================

DO $$
BEGIN
    RAISE NOTICE '==========================================';
    RAISE NOTICE '时间字段规范化完成';
    RAISE NOTICE '==========================================';
END $$;
