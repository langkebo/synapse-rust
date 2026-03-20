-- ============================================================================
-- 迁移脚本: 20260311000006_add_e2ee_tables.sql
-- 创建日期: 2026-03-11
-- 作者: System
-- 描述: 添加 E2EE (端到端加密) 相关表结构
-- 版本: v6.0.6
-- ============================================================================

-- ============================================================================
-- 1. one_time_keys 表 - 一次性密钥存储
-- ============================================================================
CREATE TABLE IF NOT EXISTS one_time_keys (
    id BIGSERIAL PRIMARY KEY,
    user_id TEXT NOT NULL,
    device_id TEXT NOT NULL,
    algorithm TEXT NOT NULL,
    key_id TEXT NOT NULL,
    key_data TEXT NOT NULL,
    signature TEXT,
    is_used BOOLEAN NOT NULL DEFAULT FALSE,
    created_ts BIGINT NOT NULL,
    used_ts BIGINT,
    CONSTRAINT uq_one_time_keys_user_device_key UNIQUE (user_id, device_id, key_id)
);

-- 索引
CREATE INDEX IF NOT EXISTS idx_one_time_keys_user ON one_time_keys(user_id);
CREATE INDEX IF NOT EXISTS idx_one_time_keys_device ON one_time_keys(device_id);
CREATE INDEX IF NOT EXISTS idx_one_time_keys_user_device ON one_time_keys(user_id, device_id);
CREATE INDEX IF NOT EXISTS idx_one_time_keys_unused ON one_time_keys(user_id, device_id, algorithm) WHERE is_used = FALSE;

-- 外键约束
ALTER TABLE one_time_keys 
ADD CONSTRAINT fk_one_time_keys_user_id 
FOREIGN KEY (user_id) REFERENCES users(user_id) ON DELETE CASCADE;

-- 注释
COMMENT ON TABLE one_time_keys IS '存储用户设备的一次性密钥，用于 E2EE 密钥交换';
COMMENT ON COLUMN one_time_keys.is_used IS '标记密钥是否已被使用';
COMMENT ON COLUMN one_time_keys.key_data IS '密钥数据，格式取决于算法';

-- ============================================================================
-- 2. backup_keys 表 - 密钥备份数据
-- ============================================================================
CREATE TABLE IF NOT EXISTS backup_keys (
    id BIGSERIAL PRIMARY KEY,
    backup_id BIGINT NOT NULL,
    room_id TEXT NOT NULL,
    session_id TEXT NOT NULL,
    session_data JSONB NOT NULL,
    created_ts BIGINT NOT NULL,
    CONSTRAINT fk_backup_keys_backup FOREIGN KEY (backup_id) REFERENCES key_backups(id) ON DELETE CASCADE
);

-- 索引
CREATE INDEX IF NOT EXISTS idx_backup_keys_backup ON backup_keys(backup_id);
CREATE INDEX IF NOT EXISTS idx_backup_keys_room ON backup_keys(room_id);
CREATE INDEX IF NOT EXISTS idx_backup_keys_session ON backup_keys(session_id);
CREATE UNIQUE INDEX IF NOT EXISTS uq_backup_keys_backup_session ON backup_keys(backup_id, room_id, session_id);

-- 注释
COMMENT ON TABLE backup_keys IS '存储密钥备份的会话数据';
COMMENT ON COLUMN backup_keys.session_data IS '加密的会话密钥数据';

-- ============================================================================
-- 3. device_keys 表字段补充（如果需要）
-- ============================================================================
-- 确保 created_ts 字段存在且有默认值
DO $$
BEGIN
    IF NOT EXISTS (
        SELECT 1 FROM information_schema.columns 
        WHERE table_name = 'device_keys' AND column_name = 'created_ts'
    ) THEN
        ALTER TABLE device_keys ADD COLUMN created_ts BIGINT NOT NULL DEFAULT (EXTRACT(EPOCH FROM NOW()) * 1000)::BIGINT;
    END IF;
END $$;

-- ============================================================================
-- 4. 交叉签名密钥表补充字段（如果需要）
-- ============================================================================
-- 添加 first_seen_ts 和 last_seen_ts 字段
DO $$
BEGIN
    IF NOT EXISTS (
        SELECT 1 FROM information_schema.columns 
        WHERE table_name = 'cross_signing_keys' AND column_name = 'first_seen_ts'
    ) THEN
        ALTER TABLE cross_signing_keys ADD COLUMN first_seen_ts BIGINT;
    END IF;
    
    IF NOT EXISTS (
        SELECT 1 FROM information_schema.columns 
        WHERE table_name = 'cross_signing_keys' AND column_name = 'last_seen_ts'
    ) THEN
        ALTER TABLE cross_signing_keys ADD COLUMN last_seen_ts BIGINT;
    END IF;
END $$;

-- ============================================================================
-- 5. 验证表创建
-- ============================================================================
DO $$
DECLARE
    one_time_keys_count INTEGER;
    backup_keys_count INTEGER;
BEGIN
    SELECT COUNT(*) INTO one_time_keys_count 
    FROM information_schema.tables 
    WHERE table_name = 'one_time_keys';
    
    SELECT COUNT(*) INTO backup_keys_count 
    FROM information_schema.tables 
    WHERE table_name = 'backup_keys';
    
    IF one_time_keys_count = 0 THEN
        RAISE EXCEPTION 'Table one_time_keys was not created successfully';
    END IF;
    
    IF backup_keys_count = 0 THEN
        RAISE EXCEPTION 'Table backup_keys was not created successfully';
    END IF;
    
    RAISE NOTICE 'E2EE tables created successfully';
END $$;

-- ============================================================================
-- 6. 记录迁移
-- ============================================================================
INSERT INTO schema_migrations (version, name, applied_ts, description)
VALUES (
    'v6.0.6', 
    'add_e2ee_tables', 
    (EXTRACT(EPOCH FROM NOW()) * 1000)::BIGINT, 
    'Add one_time_keys and backup_keys tables for E2EE support'
) ON CONFLICT (version) DO UPDATE SET 
    name = EXCLUDED.name,
    applied_ts = EXCLUDED.applied_ts,
    description = EXCLUDED.description;

-- ============================================================================
-- 迁移完成
-- ============================================================================
