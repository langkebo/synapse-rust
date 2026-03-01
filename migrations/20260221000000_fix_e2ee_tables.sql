-- =============================================================================
-- Synapse-Rust 数据库迁移脚本
-- 版本: 20260221000000
-- 描述: 修复 E2EE 密钥备份表结构
-- 问题来源: BUG-002, BUG-003 - 密钥查询和备份 API 返回 500
-- =============================================================================

-- =============================================================================
-- 第一部分: 修复 device_keys 表约束
-- 问题: 约束名不匹配导致 INSERT 失败
-- =============================================================================

-- 确保 device_keys 表存在
CREATE TABLE IF NOT EXISTS device_keys (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    user_id VARCHAR(255) NOT NULL,
    device_id VARCHAR(255) NOT NULL,
    display_name TEXT,
    algorithm VARCHAR(255) NOT NULL,
    key_id VARCHAR(255) NOT NULL,
    public_key TEXT NOT NULL,
    signatures JSONB DEFAULT '{}',
    created_at BIGINT NOT NULL,
    updated_at BIGINT NOT NULL,
    ts_updated_ms BIGINT NOT NULL DEFAULT EXTRACT(EPOCH FROM NOW()) * 1000,
    key_json JSONB DEFAULT '{}',
    ts_added_ms BIGINT NOT NULL DEFAULT EXTRACT(EPOCH FROM NOW()) * 1000,
    ts_last_accessed BIGINT,
    verified BOOLEAN DEFAULT FALSE,
    blocked BOOLEAN DEFAULT FALSE
);

-- 删除可能存在的旧约束（如果存在）
DO $$
BEGIN
    IF EXISTS (
        SELECT 1 FROM pg_constraint 
        WHERE conname = 'device_keys_user_device_unique'
    ) THEN
        ALTER TABLE device_keys DROP CONSTRAINT device_keys_user_device_unique;
    END IF;
END $$;

-- 添加正确的唯一约束
DO $$
BEGIN
    IF NOT EXISTS (
        SELECT 1 FROM pg_constraint 
        WHERE conname = 'device_keys_user_id_device_id_key_id_key'
    ) THEN
        ALTER TABLE device_keys 
        ADD CONSTRAINT device_keys_user_id_device_id_key_id_key 
        UNIQUE (user_id, device_id, key_id);
    END IF;
END $$;

-- 创建索引
CREATE INDEX IF NOT EXISTS idx_device_keys_user_id ON device_keys(user_id);
CREATE INDEX IF NOT EXISTS idx_device_keys_device_id ON device_keys(device_id);
CREATE INDEX IF NOT EXISTS idx_device_keys_key_id ON device_keys(key_id);

-- =============================================================================
-- 第二部分: 修复 key_backups 表结构
-- 问题: 列名与代码期望不匹配
-- =============================================================================

-- 重建 key_backups 表
DROP TABLE IF EXISTS key_backups CASCADE;

CREATE TABLE key_backups (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    user_id VARCHAR(255) NOT NULL,
    backup_id VARCHAR(255) NOT NULL,
    version BIGINT NOT NULL,
    algorithm VARCHAR(255) NOT NULL DEFAULT 'm.megolm_backup.v1',
    auth_key TEXT DEFAULT '',
    mgmt_key TEXT DEFAULT '',
    backup_data JSONB DEFAULT '{}',
    etag TEXT,
    created_at TIMESTAMP WITH TIME ZONE NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMP WITH TIME ZONE NOT NULL DEFAULT NOW(),
    UNIQUE(user_id, backup_id)
);

CREATE INDEX IF NOT EXISTS idx_key_backups_user_id ON key_backups(user_id);
CREATE INDEX IF NOT EXISTS idx_key_backups_version ON key_backups(version);

-- =============================================================================
-- 第三部分: 修复 backup_keys 表结构
-- 问题: 列名和约束与代码不匹配
-- =============================================================================

-- 重建 backup_keys 表
DROP TABLE IF EXISTS backup_keys CASCADE;

CREATE TABLE backup_keys (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    user_id VARCHAR(255) NOT NULL,
    backup_id VARCHAR(255) NOT NULL,
    room_id VARCHAR(255) NOT NULL,
    session_id VARCHAR(255) NOT NULL,
    first_message_index BIGINT NOT NULL DEFAULT 0,
    forwarded_count BIGINT NOT NULL DEFAULT 0,
    is_verified BOOLEAN NOT NULL DEFAULT FALSE,
    backup_data JSONB DEFAULT '{}',
    created_at TIMESTAMP WITH TIME ZONE NOT NULL DEFAULT NOW(),
    UNIQUE(user_id, backup_id, room_id, session_id, first_message_index)
);

CREATE INDEX IF NOT EXISTS idx_backup_keys_user_id ON backup_keys(user_id);
CREATE INDEX IF NOT EXISTS idx_backup_keys_backup_id ON backup_keys(backup_id);
CREATE INDEX IF NOT EXISTS idx_backup_keys_room_id ON backup_keys(room_id);

-- =============================================================================
-- 第四部分: 确保其他 E2EE 相关表存在
-- =============================================================================

-- 确保 cross_signing_keys 表存在
CREATE TABLE IF NOT EXISTS cross_signing_keys (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    user_id VARCHAR(255) NOT NULL,
    key_type VARCHAR(50) NOT NULL,
    public_key TEXT NOT NULL,
    usage TEXT[] NOT NULL DEFAULT '{}',
    signatures JSONB DEFAULT '{}',
    created_at TIMESTAMP WITH TIME ZONE NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMP WITH TIME ZONE NOT NULL DEFAULT NOW(),
    UNIQUE(user_id, key_type)
);

CREATE INDEX IF NOT EXISTS idx_cross_signing_keys_user_id ON cross_signing_keys(user_id);

-- 确保 megolm_sessions 表存在
CREATE TABLE IF NOT EXISTS megolm_sessions (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    session_id VARCHAR(255) NOT NULL UNIQUE,
    room_id VARCHAR(255) NOT NULL,
    sender_key TEXT NOT NULL,
    session_key TEXT NOT NULL,
    algorithm VARCHAR(100) NOT NULL DEFAULT 'm.megolm.v1.aes-sha2',
    message_index BIGINT NOT NULL DEFAULT 0,
    created_at TIMESTAMP WITH TIME ZONE NOT NULL DEFAULT NOW(),
    last_used_at TIMESTAMP WITH TIME ZONE NOT NULL DEFAULT NOW(),
    expires_at TIMESTAMP WITH TIME ZONE
);

CREATE INDEX IF NOT EXISTS idx_megolm_sessions_room_id ON megolm_sessions(room_id);
CREATE INDEX IF NOT EXISTS idx_megolm_sessions_sender_key ON megolm_sessions(sender_key);

-- 确保 inbound_megolm_sessions 表存在
CREATE TABLE IF NOT EXISTS inbound_megolm_sessions (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    session_id VARCHAR(255) NOT NULL UNIQUE,
    sender_key TEXT NOT NULL,
    room_id VARCHAR(255) NOT NULL,
    session_key TEXT NOT NULL,
    algorithm VARCHAR(100) NOT NULL DEFAULT 'm.megolm.v1.aes-sha2',
    message_index BIGINT NOT NULL DEFAULT 0,
    created_at TIMESTAMP WITH TIME ZONE NOT NULL DEFAULT NOW(),
    expires_at TIMESTAMP WITH TIME ZONE
);

CREATE INDEX IF NOT EXISTS idx_inbound_megolm_sessions_sender_key ON inbound_megolm_sessions(sender_key);

-- =============================================================================
-- 最后部分: 记录迁移版本
-- =============================================================================

INSERT INTO schema_migrations (version, success, executed_at, description)
VALUES ('20260221000000', true, NOW(), 'Fix E2EE key backup tables structure')
ON CONFLICT (version) DO NOTHING;
