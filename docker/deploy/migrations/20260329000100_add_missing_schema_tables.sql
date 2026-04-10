--no-transaction
-- V260330_001__MIG-XXX__add_missing_schema_tables.sql
--
-- 描述: 为代码中引用但缺失 schema 的表创建定义
-- 按 OPTIMIZATION_PLAN.md Section 5.2 Exceptions 清理要求
--
-- 包含表:
--   - dehydrated_devices (设备脱水功能)
--   - delayed_events (延迟事件调度)
--   - e2ee_audit_log (E2EE 审计日志)
--   - e2ee_secret_storage_keys (SSSS 密钥存储)
--   - e2ee_stored_secrets (存储的 E2EE 密钥)
--   - email_verification_tokens (邮箱验证令牌)
--   - federation_access_stats (联邦访问统计)
--   - federation_blacklist_config (联邦黑名单配置)
--   - federation_blacklist_log (联邦黑名单日志)
--   - federation_blacklist_rule (联邦黑名单规则)
--   - leak_alerts (密钥泄漏告警)
--
-- 回滚: V260330_001__MIG-XXX__add_missing_schema_tables.undo.sql

SET TIME ZONE 'UTC';

DO $$
BEGIN
    RAISE NOTICE '开始创建缺失的 schema 表...';
END $$;

-- ============================================================================
-- 1. dehydrated_devices - 设备脱水表
-- ============================================================================

CREATE TABLE IF NOT EXISTS dehydrated_devices (
    id BIGSERIAL PRIMARY KEY,
    user_id TEXT NOT NULL,
    device_id TEXT NOT NULL UNIQUE,
    device_data JSONB NOT NULL DEFAULT '{}',
    algorithm TEXT NOT NULL,
    account JSONB,
    created_ts BIGINT NOT NULL,
    updated_ts BIGINT NOT NULL,
    expires_at BIGINT
);

CREATE INDEX IF NOT EXISTS idx_dehydrated_devices_user ON dehydrated_devices(user_id);
CREATE INDEX IF NOT EXISTS idx_dehydrated_devices_expires ON dehydrated_devices(expires_at) WHERE expires_at IS NOT NULL;

-- ============================================================================
-- 2. delayed_events - 延迟事件表
-- ============================================================================

CREATE TABLE IF NOT EXISTS delayed_events (
    id BIGSERIAL PRIMARY KEY,
    room_id TEXT NOT NULL,
    user_id TEXT NOT NULL,
    device_id TEXT NOT NULL,
    event_id TEXT NOT NULL UNIQUE,
    event_type TEXT NOT NULL,
    state_key TEXT,
    content JSONB NOT NULL DEFAULT '{}',
    delay_ms BIGINT NOT NULL,
    scheduled_ts BIGINT NOT NULL,
    created_ts BIGINT NOT NULL,
    status TEXT NOT NULL DEFAULT 'pending',
    retry_count INTEGER NOT NULL DEFAULT 0,
    last_error TEXT
);

CREATE INDEX IF NOT EXISTS idx_delayed_events_scheduled ON delayed_events(scheduled_ts);
CREATE INDEX IF NOT EXISTS idx_delayed_events_status ON delayed_events(status);
CREATE INDEX IF NOT EXISTS idx_delayed_events_room ON delayed_events(room_id);

-- ============================================================================
-- 3. e2ee_audit_log - E2EE 审计日志表
-- ============================================================================

CREATE TABLE IF NOT EXISTS e2ee_audit_log (
    id BIGSERIAL PRIMARY KEY,
    user_id TEXT NOT NULL,
    device_id TEXT NOT NULL,
    action TEXT NOT NULL,
    event_id TEXT,
    room_id TEXT,
    details JSONB NOT NULL DEFAULT '{}',
    created_ts BIGINT NOT NULL
);

CREATE INDEX IF NOT EXISTS idx_e2ee_audit_log_user ON e2ee_audit_log(user_id);
CREATE INDEX IF NOT EXISTS idx_e2ee_audit_log_created ON e2ee_audit_log(created_ts DESC);
CREATE INDEX IF NOT EXISTS idx_e2ee_audit_log_action ON e2ee_audit_log(action);

-- ============================================================================
-- 4. e2ee_secret_storage_keys - SSSS 密钥存储表
-- ============================================================================

CREATE TABLE IF NOT EXISTS e2ee_secret_storage_keys (
    id BIGSERIAL PRIMARY KEY,
    user_id TEXT NOT NULL,
    key_name TEXT NOT NULL,
    key_id TEXT NOT NULL UNIQUE,
    algorithm TEXT NOT NULL,
    key_data BYTEA NOT NULL,
    created_ts BIGINT NOT NULL,
    updated_ts BIGINT NOT NULL,
    is_active BOOLEAN NOT NULL DEFAULT TRUE
);

CREATE INDEX IF NOT EXISTS idx_e2ee_secret_storage_keys_user ON e2ee_secret_storage_keys(user_id);
CREATE INDEX IF NOT EXISTS idx_e2ee_secret_storage_keys_key_id ON e2ee_secret_storage_keys(key_id);

-- ============================================================================
-- 5. e2ee_stored_secrets - 存储的 E2EE 密钥表
-- ============================================================================

CREATE TABLE IF NOT EXISTS e2ee_stored_secrets (
    id BIGSERIAL PRIMARY KEY,
    user_id TEXT NOT NULL,
    secret_name TEXT NOT NULL,
    secret_data BYTEA NOT NULL,
    key_key_id TEXT NOT NULL,
    created_ts BIGINT NOT NULL,
    updated_ts BIGINT NOT NULL
);

CREATE UNIQUE INDEX IF NOT EXISTS idx_e2ee_stored_secrets_user_name ON e2ee_stored_secrets(user_id, secret_name);
CREATE INDEX IF NOT EXISTS idx_e2ee_stored_secrets_key ON e2ee_stored_secrets(key_key_id);

-- ============================================================================
-- 6. email_verification_tokens - 邮箱验证令牌表
-- ============================================================================

CREATE TABLE IF NOT EXISTS email_verification_tokens (
    id BIGSERIAL PRIMARY KEY,
    user_id TEXT,
    email TEXT NOT NULL,
    token TEXT NOT NULL UNIQUE,
    expires_at TIMESTAMPTZ NOT NULL,
    created_ts TIMESTAMPTZ NOT NULL,
    used BOOLEAN NOT NULL DEFAULT FALSE,
    session_data JSONB
);

CREATE INDEX IF NOT EXISTS idx_email_verification_tokens_email ON email_verification_tokens(email);
CREATE INDEX IF NOT EXISTS idx_email_verification_tokens_expires ON email_verification_tokens(expires_at) WHERE used = FALSE;

-- ============================================================================
-- 7. federation_access_stats - 联邦访问统计表
-- ============================================================================

CREATE TABLE IF NOT EXISTS federation_access_stats (
    id BIGSERIAL PRIMARY KEY,
    server_name TEXT NOT NULL UNIQUE,
    total_requests BIGINT NOT NULL DEFAULT 0,
    successful_requests BIGINT NOT NULL DEFAULT 0,
    failed_requests BIGINT NOT NULL DEFAULT 0,
    last_request_ts BIGINT,
    last_success_ts BIGINT,
    last_failure_ts BIGINT,
    average_response_time_ms DOUBLE PRECISION NOT NULL DEFAULT 0,
    error_rate DOUBLE PRECISION NOT NULL DEFAULT 0,
    created_ts BIGINT NOT NULL,
    updated_ts BIGINT NOT NULL
);

CREATE INDEX IF NOT EXISTS idx_federation_access_stats_server ON federation_access_stats(server_name);

-- ============================================================================
-- 8. federation_blacklist_config - 联邦黑名单配置表
-- ============================================================================

CREATE TABLE IF NOT EXISTS federation_blacklist_config (
    id BIGSERIAL PRIMARY KEY,
    server_name TEXT NOT NULL UNIQUE,
    block_type TEXT NOT NULL,
    reason TEXT,
    blocked_by TEXT NOT NULL,
    created_ts BIGINT NOT NULL,
    updated_ts BIGINT NOT NULL,
    expires_at BIGINT,
    is_enabled BOOLEAN NOT NULL DEFAULT TRUE,
    metadata JSONB NOT NULL DEFAULT '{}'
);

CREATE INDEX IF NOT EXISTS idx_federation_blacklist_config_enabled ON federation_blacklist_config(is_enabled) WHERE is_enabled = TRUE;

-- ============================================================================
-- 9. federation_blacklist_log - 联邦黑名单日志表
-- ============================================================================

CREATE TABLE IF NOT EXISTS federation_blacklist_log (
    id BIGSERIAL PRIMARY KEY,
    server_name TEXT NOT NULL,
    action TEXT NOT NULL,
    old_status TEXT,
    new_status TEXT,
    reason TEXT,
    performed_by TEXT NOT NULL,
    performed_ts BIGINT NOT NULL,
    ip_address TEXT,
    user_agent TEXT,
    metadata JSONB NOT NULL DEFAULT '{}'
);

CREATE INDEX IF NOT EXISTS idx_federation_blacklist_log_server ON federation_blacklist_log(server_name);
CREATE INDEX IF NOT EXISTS idx_federation_blacklist_log_performed ON federation_blacklist_log(performed_ts DESC);

-- ============================================================================
-- 10. federation_blacklist_rule - 联邦黑名单规则表
-- ============================================================================

CREATE TABLE IF NOT EXISTS federation_blacklist_rule (
    id BIGSERIAL PRIMARY KEY,
    rule_name TEXT NOT NULL,
    rule_type TEXT NOT NULL,
    pattern TEXT NOT NULL,
    action TEXT NOT NULL,
    priority INTEGER NOT NULL DEFAULT 0,
    is_enabled BOOLEAN NOT NULL DEFAULT TRUE,
    description TEXT,
    created_ts BIGINT NOT NULL,
    updated_ts BIGINT NOT NULL,
    created_by TEXT NOT NULL
);

CREATE INDEX IF NOT EXISTS idx_federation_blacklist_rule_enabled ON federation_blacklist_rule(is_enabled) WHERE is_enabled = TRUE;
CREATE INDEX IF NOT EXISTS idx_federation_blacklist_rule_priority ON federation_blacklist_rule(priority DESC);

-- ============================================================================
-- 11. leak_alerts - 密钥泄漏告警表
-- ============================================================================

CREATE TABLE IF NOT EXISTS leak_alerts (
    id BIGSERIAL PRIMARY KEY,
    user_id TEXT NOT NULL,
    key_id TEXT NOT NULL,
    alert_type TEXT NOT NULL,
    severity TEXT NOT NULL,
    details JSONB NOT NULL DEFAULT '{}',
    created_ts BIGINT NOT NULL,
    acknowledged BOOLEAN NOT NULL DEFAULT FALSE,
    acknowledged_by TEXT,
    acknowledged_at BIGINT
);

CREATE INDEX IF NOT EXISTS idx_leak_alerts_user ON leak_alerts(user_id);
CREATE INDEX IF NOT EXISTS idx_leak_alerts_created ON leak_alerts(created_ts DESC);
CREATE INDEX IF NOT EXISTS idx_leak_alerts_acknowledged ON leak_alerts(acknowledged) WHERE acknowledged = FALSE;

-- ============================================================================
-- 完成
-- ============================================================================

DO $$
BEGIN
    RAISE NOTICE '缺失 schema 表创建完成';
END $$;
