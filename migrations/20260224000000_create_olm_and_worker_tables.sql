-- =============================================================================
-- Synapse-Rust 数据库迁移脚本
-- 版本: 20260224000000
-- 描述: 创建 Olm 账户和会话持久化表
-- 问题来源: E2EE 优化 - Olm 会话持久化
-- =============================================================================

-- =============================================================================
-- 第一部分: Olm 账户表
-- 问题: E2EE-001 - Olm 会话持久化需要存储账户状态
-- =============================================================================

CREATE TABLE IF NOT EXISTS olm_accounts (
    id BIGSERIAL PRIMARY KEY,
    user_id VARCHAR(255) NOT NULL,
    device_id VARCHAR(255) NOT NULL,
    identity_key VARCHAR(255) NOT NULL,
    serialized_account TEXT NOT NULL,
    one_time_keys_published BOOLEAN DEFAULT FALSE,
    fallback_key_published BOOLEAN DEFAULT FALSE,
    created_ts BIGINT NOT NULL DEFAULT EXTRACT(EPOCH FROM NOW()) * 1000,
    updated_ts BIGINT NOT NULL DEFAULT EXTRACT(EPOCH FROM NOW()) * 1000,
    UNIQUE(user_id, device_id)
);

CREATE INDEX IF NOT EXISTS idx_olm_accounts_user_id ON olm_accounts(user_id);
CREATE INDEX IF NOT EXISTS idx_olm_accounts_identity_key ON olm_accounts(identity_key);

-- =============================================================================
-- 第二部分: Olm 会话表
-- 问题: E2EE-003 - Olm 会话管理需要持久化会话状态
-- =============================================================================

CREATE TABLE IF NOT EXISTS olm_sessions (
    id BIGSERIAL PRIMARY KEY,
    user_id VARCHAR(255) NOT NULL,
    device_id VARCHAR(255) NOT NULL,
    session_id VARCHAR(255) NOT NULL UNIQUE,
    sender_key VARCHAR(255) NOT NULL,
    receiver_key VARCHAR(255) NOT NULL,
    serialized_state TEXT NOT NULL,
    message_index INTEGER DEFAULT 0,
    created_ts BIGINT NOT NULL DEFAULT EXTRACT(EPOCH FROM NOW()) * 1000,
    last_used_ts BIGINT NOT NULL DEFAULT EXTRACT(EPOCH FROM NOW()) * 1000,
    expires_ts BIGINT
);

CREATE INDEX IF NOT EXISTS idx_olm_sessions_user_device ON olm_sessions(user_id, device_id);
CREATE INDEX IF NOT EXISTS idx_olm_sessions_sender_key ON olm_sessions(sender_key);
CREATE INDEX IF NOT EXISTS idx_olm_sessions_last_used ON olm_sessions(last_used_ts);
CREATE INDEX IF NOT EXISTS idx_olm_sessions_expires ON olm_sessions(expires_ts) WHERE expires_ts IS NOT NULL;

-- =============================================================================
-- 第三部分: Worker 消息总线表
-- 问题: WORKER-001 - Redis Pub/Sub 消息总线需要流位置跟踪
-- =============================================================================

CREATE TABLE IF NOT EXISTS worker_stream_positions (
    id BIGSERIAL PRIMARY KEY,
    stream_name VARCHAR(255) NOT NULL UNIQUE,
    instance_name VARCHAR(255) NOT NULL,
    position BIGINT NOT NULL DEFAULT 0,
    updated_ts BIGINT NOT NULL DEFAULT EXTRACT(EPOCH FROM NOW()) * 1000
);

CREATE INDEX IF NOT EXISTS idx_worker_stream_positions_stream ON worker_stream_positions(stream_name);

-- =============================================================================
-- 第四部分: 推送提供商配置表
-- 问题: PUSH-001 - Push 通知优化需要存储提供商配置
-- =============================================================================

CREATE TABLE IF NOT EXISTS push_provider_configs (
    id BIGSERIAL PRIMARY KEY,
    provider_type VARCHAR(50) NOT NULL,
    provider_name VARCHAR(255) NOT NULL UNIQUE,
    config JSONB NOT NULL,
    enabled BOOLEAN DEFAULT TRUE,
    created_ts BIGINT NOT NULL DEFAULT EXTRACT(EPOCH FROM NOW()) * 1000,
    updated_ts BIGINT NOT NULL DEFAULT EXTRACT(EPOCH FROM NOW()) * 1000
);

CREATE INDEX IF NOT EXISTS idx_push_provider_configs_type ON push_provider_configs(provider_type);
CREATE INDEX IF NOT EXISTS idx_push_provider_configs_enabled ON push_provider_configs(enabled) WHERE enabled = TRUE;

-- =============================================================================
-- 最后部分: 记录迁移版本
-- =============================================================================

INSERT INTO schema_migrations (version, success, executed_at, description)
VALUES ('20260224000000', true, NOW(), 'Create Olm tables, worker stream positions, and push provider configs')
ON CONFLICT (version) DO NOTHING;

-- =============================================================================
-- 回滚脚本 (如需回滚，请手动执行以下语句)
-- =============================================================================
-- DROP TABLE IF EXISTS push_provider_configs;
-- DROP TABLE IF EXISTS worker_stream_positions;
-- DROP TABLE IF EXISTS olm_sessions;
-- DROP TABLE IF EXISTS olm_accounts;
-- DELETE FROM schema_migrations WHERE version = '20260224000000';
