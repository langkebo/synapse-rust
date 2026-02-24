-- =============================================================================
-- Synapse-Rust 数据库迁移脚本
-- 版本: 20260226000001
-- 描述: 创建用户3PID表
-- =============================================================================

SET client_encoding = 'UTF8';
SET standard_conforming_strings = on;

-- =============================================================================
-- 用户3PID表
-- =============================================================================

CREATE TABLE IF NOT EXISTS user_threepids (
    id BIGSERIAL PRIMARY KEY,
    user_id VARCHAR(255) NOT NULL,
    address VARCHAR(255) NOT NULL,
    medium VARCHAR(50) NOT NULL,
    validated_at BIGINT NOT NULL,
    added_at BIGINT NOT NULL,
    UNIQUE(address, medium, user_id)
);

CREATE INDEX IF NOT EXISTS idx_user_threepids_user_id 
    ON user_threepids(user_id);

CREATE INDEX IF NOT EXISTS idx_user_threepids_address_medium 
    ON user_threepids(address, medium);

CREATE INDEX IF NOT EXISTS idx_user_threepids_validated 
    ON user_threepids(validated_at);

-- =============================================================================
-- 记录迁移版本
-- =============================================================================

INSERT INTO schema_migrations (version, description, success)
VALUES ('20260226000001', 'Create user_threepids table', TRUE)
ON CONFLICT (version) DO UPDATE SET success = TRUE, executed_at = NOW();
