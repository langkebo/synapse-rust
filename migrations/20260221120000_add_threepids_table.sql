-- =============================================================================
-- Synapse-Rust 数据库迁移脚本
-- 版本: 20260221120000
-- 描述: 添加 user_threepids 表支持 3PID 功能
-- 问题来源: BUG-P1-002, BUG-P1-003, BUG-P1-004
-- =============================================================================

-- =============================================================================
-- 第一部分: 创建 user_threepids 表
-- =============================================================================

CREATE TABLE IF NOT EXISTS user_threepids (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    user_id VARCHAR(255) NOT NULL,
    medium VARCHAR(50) NOT NULL,
    address VARCHAR(255) NOT NULL,
    validated_at BIGINT,
    added_at BIGINT NOT NULL DEFAULT EXTRACT(EPOCH FROM NOW()) * 1000,
    session_id VARCHAR(255),
    client_secret VARCHAR(255),
    CONSTRAINT user_threepids_user_medium_address_key UNIQUE (user_id, medium, address)
);

-- 创建索引
CREATE INDEX IF NOT EXISTS idx_user_threepids_user_id ON user_threepids(user_id);
CREATE INDEX IF NOT EXISTS idx_user_threepids_medium ON user_threepids(medium);
CREATE INDEX IF NOT EXISTS idx_user_threepids_address ON user_threepids(address);

-- =============================================================================
-- 第二部分: 添加注释
-- =============================================================================

COMMENT ON TABLE user_threepids IS '用户第三方标识符表（邮箱、手机号等）';
COMMENT ON COLUMN user_threepids.user_id IS '用户ID';
COMMENT ON COLUMN user_threepids.medium IS '标识符类型: email, msisdn';
COMMENT ON COLUMN user_threepids.address IS '标识符地址: 邮箱地址或手机号';
COMMENT ON COLUMN user_threepids.validated_at IS '验证时间戳（毫秒）';
COMMENT ON COLUMN user_threepids.added_at IS '添加时间戳（毫秒）';

-- =============================================================================
-- 第三部分: 记录迁移版本
-- =============================================================================

INSERT INTO schema_migrations (version, success, executed_at, description)
VALUES ('20260221120000', true, NOW(), 'Add user_threepids table for 3PID support')
ON CONFLICT (version) DO NOTHING;
