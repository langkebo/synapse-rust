-- =============================================================================
-- Synapse-Rust 数据库迁移脚本
-- 版本: 20260213000000
-- 描述: 修复E2EE密钥上传约束问题
-- 问题: ON CONFLICT (user_id, device_id) 与表约束 UNIQUE (user_id, device_id, key_id) 不匹配
-- 解决方案: 添加 (user_id, device_id) 的唯一约束用于设备密钥更新
-- =============================================================================

BEGIN;

-- =============================================================================
-- 第一部分: 修复 device_keys 表约束
-- =============================================================================

-- 检查并添加 (user_id, device_id) 的唯一约束
-- 这个约束用于 UPSERT 操作，确保每个设备的密钥可以被更新
DO $$
BEGIN
    -- 检查约束是否已存在
    IF NOT EXISTS (
        SELECT 1 FROM pg_constraint 
        WHERE conname = 'device_keys_user_device_unique'
        AND conrelid = 'device_keys'::regclass
    ) THEN
        -- 对于已存在的数据，先清理重复记录（保留最新的）
        DELETE FROM device_keys a
        USING device_keys b
        WHERE a.user_id = b.user_id
        AND a.device_id = b.device_id
        AND a.key_id != b.key_id
        AND a.ts_updated_ms < b.ts_updated_ms;
        
        -- 添加唯一约束
        ALTER TABLE device_keys 
        ADD CONSTRAINT device_keys_user_device_unique 
        UNIQUE (user_id, device_id);
        
        RAISE NOTICE 'Added unique constraint device_keys_user_device_unique';
    ELSE
        RAISE NOTICE 'Unique constraint device_keys_user_device_unique already exists';
    END IF;
END $$;

-- =============================================================================
-- 第二部分: 优化 one_time_keys 表
-- =============================================================================

-- 确保一次性密钥表存在且有正确的约束
DO $$
BEGIN
    IF EXISTS (SELECT 1 FROM information_schema.tables WHERE table_name = 'one_time_keys') THEN
        -- 添加索引优化查询性能
        IF NOT EXISTS (
            SELECT 1 FROM pg_indexes 
            WHERE indexname = 'idx_one_time_keys_user_device'
        ) THEN
            CREATE INDEX idx_one_time_keys_user_device ON one_time_keys(user_id, device_id);
            RAISE NOTICE 'Added index idx_one_time_keys_user_device';
        END IF;
        
        -- 添加未使用密钥的索引
        IF NOT EXISTS (
            SELECT 1 FROM pg_indexes 
            WHERE indexname = 'idx_one_time_keys_available'
        ) THEN
            CREATE INDEX idx_one_time_keys_available ON one_time_keys(user_id, device_id, key_id) 
            WHERE exhausted = FALSE;
            RAISE NOTICE 'Added index idx_one_time_keys_available';
        END IF;
    END IF;
END $$;

-- =============================================================================
-- 第三部分: 添加辅助函数
-- =============================================================================

-- 创建或替换获取密钥计数的函数
CREATE OR REPLACE FUNCTION get_one_time_key_counts(p_user_id VARCHAR, p_device_id VARCHAR)
RETURNS TABLE(algorithm VARCHAR, count BIGINT) AS $$
BEGIN
    RETURN QUERY
    SELECT 
        COALESCE(
            SPLIT_PART(ok.key_id, ':', 1),
            'signed_curve25519'
        ) as algorithm,
        COUNT(*)::BIGINT as count
    FROM one_time_keys ok
    WHERE ok.user_id = p_user_id
    AND ok.device_id = p_device_id
    AND ok.exhausted = FALSE
    GROUP BY SPLIT_PART(ok.key_id, ':', 1);
END;
$$ LANGUAGE plpgsql;

-- =============================================================================
-- 第四部分: 数据一致性检查
-- =============================================================================

-- 检查并修复孤立记录
DELETE FROM device_keys 
WHERE device_id NOT IN (SELECT device_id FROM devices WHERE devices.user_id = device_keys.user_id);

-- 更新时间戳字段
UPDATE device_keys 
SET ts_updated_ms = EXTRACT(EPOCH FROM NOW()) * 1000 
WHERE ts_updated_ms IS NULL OR ts_updated_ms = 0;

UPDATE one_time_keys 
SET ts_created_ms = EXTRACT(EPOCH FROM NOW()) * 1000 
WHERE ts_created_ms IS NULL OR ts_created_ms = 0;

-- =============================================================================
-- 第五部分: 验证迁移
-- =============================================================================

DO $$
DECLARE
    constraint_count INTEGER;
    index_count INTEGER;
BEGIN
    -- 验证约束
    SELECT COUNT(*) INTO constraint_count
    FROM pg_constraint 
    WHERE conname = 'device_keys_user_device_unique'
    AND conrelid = 'device_keys'::regclass;
    
    IF constraint_count = 0 THEN
        RAISE EXCEPTION 'Migration failed: device_keys_user_device_unique constraint not found';
    END IF;
    
    -- 验证索引
    SELECT COUNT(*) INTO index_count
    FROM pg_indexes 
    WHERE indexname IN ('idx_one_time_keys_user_device', 'idx_one_time_keys_available');
    
    RAISE NOTICE 'Migration completed successfully. Constraints: %, Indexes: %', constraint_count, index_count;
END $$;

COMMIT;

-- =============================================================================
-- 迁移完成
-- =============================================================================
-- 预期效果:
-- 1. device_keys 表添加 (user_id, device_id) 唯一约束
-- 2. one_time_keys 表添加性能优化索引
-- 3. 创建 get_one_time_key_counts 辅助函数
-- 4. 清理孤立数据并修复时间戳
-- =============================================================================
