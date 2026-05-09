-- ============================================================================
-- Consolidated Migration: Schema Contract And Features v7
-- Version: 20260515000001
-- Created: 2026-05-15
--
-- Merged source files:
--   - 20260401000001_consolidated_schema_additions.sql
--   - 20260406000001_consolidated_schema_fixes.sql
--   - 20260410000001_consolidated_feature_additions.sql
--   - 20260422000001_schema_code_alignment.sql
--   - 20260423000001_add_federation_admission_control.sql
--   - 20260423000002_fix_auth_token_schema.sql
--   - 20260430000001_add_oidc_user_mapping.sql
--   - 20260430000002_add_missing_perf_indexes.sql
--   - 20260501000001_backup_keys_metadata.sql
--   - 20260505000001_add_user_search_and_presence_indexes.sql
--   - 20260505000002_add_saml_config_overrides.sql
--   - 20260507000002_repair_legacy_background_retention_room_alias_schema.sql
--
-- Notes:
--   - 20260301000000_create_missing_baseline_tables.sql is folded into
--     unified_schema_v7.sql and is intentionally not replayed here.
--   - SQL-internal schema_migrations writes are removed. File-level versioning
--     remains the responsibility of docker/db_migrate.sh.
-- ============================================================================


-- ===== Merged from: 20260401000001_consolidated_schema_additions.sql =====

-- ============================================================================
-- Consolidated Migration: Schema Additions & Alignment
-- Created: 2026-04-22 (consolidated from 7 migrations dated 2026-03-29 ~ 2026-04-04)
--
-- Merged source files (archived to migrations/archive/pre-consolidation-2026-04-22/):
--   1. 20260329000000_create_migration_audit_table.sql
--   2. 20260329000100_add_missing_schema_tables.sql
--   3. 20260330000012_add_federation_signing_keys.sql
--   4. 20260331000100_add_event_relations_table.sql
--   5. 20260403000001_add_openclaw_integration.sql
--   6. 20260404000001_consolidated_schema_alignment.sql
--   7. 20260404000002_consolidated_minor_features.sql
--
-- All statements use IF NOT EXISTS / IF EXISTS guards for idempotent execution.
-- ============================================================================
--no-transaction


-- ===== Merged from: 20260329000000_create_migration_audit_table.sql =====

-- +----------------------------------------------------------------------------+
-- | Migration: V260329_000__SYS_0001__create_migration_audit_table
-- | Jira: SYS-0001
-- | Author: synapse-rust team
-- | Date: 2026-03-29
-- | Description: 创建 migration_audit 表用于记录迁移执行指标
-- | Checksum: a1b2c3d4e5f6g7h8
-- +----------------------------------------------------------------------------+

BEGIN;

-- Migration Audit Table - 记录每次迁移执行的指标
CREATE TABLE IF NOT EXISTS migration_audit (
    id BIGSERIAL PRIMARY KEY,
    version VARCHAR(50) NOT NULL,
    description TEXT,
    duration_ms BIGINT NOT NULL,
    rows_affected BIGINT DEFAULT 0,
    executed_by VARCHAR(100) NOT NULL DEFAULT CURRENT_USER,
    executed_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    status VARCHAR(20) NOT NULL DEFAULT 'SUCCESS',
    error_message TEXT,
    checksum VARCHAR(64),
    migration_file VARCHAR(255),
    created_ts BIGINT NOT NULL DEFAULT (EXTRACT(EPOCH FROM NOW()) * 1000)::BIGINT
);

-- 创建索引
CREATE INDEX IF NOT EXISTS idx_migration_audit_version ON migration_audit (version);
CREATE INDEX IF NOT EXISTS idx_migration_audit_executed_at ON migration_audit (executed_at);
CREATE INDEX IF NOT EXISTS idx_migration_audit_status ON migration_audit (status);

-- 添加注释
COMMENT ON TABLE migration_audit IS '记录每次数据库迁移执行的指标，用于性能监控和问题排查';
COMMENT ON COLUMN migration_audit.duration_ms IS '迁移执行耗时（毫秒）';
COMMENT ON COLUMN migration_audit.rows_affected IS '影响的行数';
COMMENT ON COLUMN migration_audit.status IS '执行状态：SUCCESS, FAILED, ROLLED_BACK';
COMMENT ON COLUMN migration_audit.checksum IS '迁移脚本的 SHA-256 校验和';
COMMENT ON COLUMN migration_audit.migration_file IS '迁移脚本文件名';

COMMIT;

-- ===== Merged from: 20260329000100_add_missing_schema_tables.sql =====

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

-- ===== Merged from: 20260330000012_add_federation_signing_keys.sql =====

--no-transaction
CREATE TABLE IF NOT EXISTS federation_signing_keys (
    server_name TEXT NOT NULL,
    key_id TEXT NOT NULL,
    secret_key TEXT NOT NULL,
    public_key TEXT NOT NULL,
    created_ts BIGINT NOT NULL,
    expires_at BIGINT NOT NULL,
    key_json JSONB NOT NULL DEFAULT '{}'::jsonb,
    ts_added_ms BIGINT NOT NULL,
    ts_valid_until_ms BIGINT NOT NULL,
    PRIMARY KEY (server_name, key_id)
);

DO $$
BEGIN
    IF EXISTS (
        SELECT 1
        FROM information_schema.columns
        WHERE table_schema = 'public'
          AND table_name = 'federation_signing_keys'
          AND column_name = 'created_at'
    ) AND NOT EXISTS (
        SELECT 1
        FROM information_schema.columns
        WHERE table_schema = 'public'
          AND table_name = 'federation_signing_keys'
          AND column_name = 'created_ts'
    ) THEN
        ALTER TABLE federation_signing_keys RENAME COLUMN created_at TO created_ts;
    END IF;
END $$;

ALTER TABLE IF EXISTS federation_signing_keys ADD COLUMN IF NOT EXISTS created_ts BIGINT;
ALTER TABLE IF EXISTS federation_signing_keys ADD COLUMN IF NOT EXISTS expires_at BIGINT;
ALTER TABLE IF EXISTS federation_signing_keys ADD COLUMN IF NOT EXISTS key_json JSONB DEFAULT '{}'::jsonb;
ALTER TABLE IF EXISTS federation_signing_keys ADD COLUMN IF NOT EXISTS ts_added_ms BIGINT;
ALTER TABLE IF EXISTS federation_signing_keys ADD COLUMN IF NOT EXISTS ts_valid_until_ms BIGINT;

DO $$
BEGIN
    IF EXISTS (
        SELECT 1
        FROM information_schema.columns
        WHERE table_schema = 'public'
          AND table_name = 'federation_signing_keys'
          AND column_name = 'key_json'
          AND data_type <> 'jsonb'
    ) THEN
        ALTER TABLE federation_signing_keys
        ALTER COLUMN key_json TYPE JSONB
        USING COALESCE(NULLIF(BTRIM(key_json::text, '"'), ''), '{}')::jsonb;
    END IF;
END $$;

UPDATE federation_signing_keys
SET created_ts = COALESCE(created_ts, ts_added_ms, (EXTRACT(EPOCH FROM NOW()) * 1000)::BIGINT),
    expires_at = COALESCE(expires_at, ts_valid_until_ms, 0),
    key_json = COALESCE(key_json, '{}'::jsonb),
    ts_added_ms = COALESCE(ts_added_ms, created_ts, (EXTRACT(EPOCH FROM NOW()) * 1000)::BIGINT),
    ts_valid_until_ms = COALESCE(ts_valid_until_ms, expires_at, 0);

ALTER TABLE IF EXISTS federation_signing_keys ALTER COLUMN created_ts SET NOT NULL;
ALTER TABLE IF EXISTS federation_signing_keys ALTER COLUMN expires_at SET NOT NULL;
ALTER TABLE IF EXISTS federation_signing_keys ALTER COLUMN key_json SET NOT NULL;
ALTER TABLE IF EXISTS federation_signing_keys ALTER COLUMN key_json SET DEFAULT '{}'::jsonb;
ALTER TABLE IF EXISTS federation_signing_keys ALTER COLUMN ts_added_ms SET NOT NULL;
ALTER TABLE IF EXISTS federation_signing_keys ALTER COLUMN ts_valid_until_ms SET NOT NULL;

CREATE INDEX IF NOT EXISTS idx_federation_signing_keys_server_created
ON federation_signing_keys(server_name, created_ts DESC);

CREATE INDEX IF NOT EXISTS idx_federation_signing_keys_key_id
ON federation_signing_keys(key_id);

-- ===== Merged from: 20260331000100_add_event_relations_table.sql =====

--no-transaction
-- V260331_001__MIG-RELATIONS__add_event_relations_table.sql
--
-- 描述: 创建 event_relations 表支持 Matrix Relations API
-- 关联代码: src/storage/relations.rs
--
-- 支持的功能:
--   - m.annotation (reactions/表情反应)
--   - m.reference (引用)
--   - m.replace (编辑/替换)
--   - m.thread (线程回复)
--
-- 回滚: V260331_001__MIG-RELATIONS__add_event_relations_table.undo.sql

SET TIME ZONE 'UTC';

DO $$
BEGIN
    RAISE NOTICE '开始创建 event_relations 表...';
END $$;

-- ============================================================================
-- event_relations 表
-- ============================================================================

CREATE TABLE IF NOT EXISTS event_relations (
    id BIGSERIAL PRIMARY KEY,
    room_id TEXT NOT NULL,
    event_id TEXT NOT NULL,
    relates_to_event_id TEXT NOT NULL,
    relation_type TEXT NOT NULL,
    sender TEXT NOT NULL,
    origin_server_ts BIGINT NOT NULL,
    content JSONB NOT NULL DEFAULT '{}',
    is_redacted BOOLEAN NOT NULL DEFAULT FALSE,
    created_ts BIGINT NOT NULL
);

-- 唯一约束: 防止重复的关系
CREATE UNIQUE INDEX IF NOT EXISTS idx_event_relations_unique
    ON event_relations(event_id, relation_type, sender);

-- 房间和事件索引: 快速查询某个事件的所有关系
CREATE INDEX IF NOT EXISTS idx_event_relations_room_event
    ON event_relations(room_id, relates_to_event_id, relation_type);

-- 发送者索引: 快速查询某个用户发送的关系
CREATE INDEX IF NOT EXISTS idx_event_relations_sender
    ON event_relations(sender, relation_type);

-- 时间索引: 按时间排序查询
CREATE INDEX IF NOT EXISTS idx_event_relations_origin_ts
    ON event_relations(room_id, origin_server_ts DESC);

-- 注解: 表和列说明
COMMENT ON TABLE event_relations IS 'Stores Matrix event relations (annotations, references, replacements, threads)';
COMMENT ON COLUMN event_relations.event_id IS 'The event that is relating to another event';
COMMENT ON COLUMN event_relations.relates_to_event_id IS 'The event_id being related to';
COMMENT ON COLUMN event_relations.relation_type IS 'Relation type: m.annotation (reactions), m.reference, m.replace (edits), m.thread';

-- ============================================================================
-- 完成
-- ============================================================================

DO $$
BEGIN
    RAISE NOTICE 'event_relations 表创建完成';
END $$;

-- ===== Merged from: 20260403000001_add_openclaw_integration.sql =====

--no-transaction
-- OpenClaw Integration Tables
-- Version: 1.0.0
-- Date: 2026-04-03
-- Description: 创建 OpenClaw 集成所需的数据库表

-- ============================================
-- 1. OpenClaw 连接配置表
-- ============================================
CREATE TABLE IF NOT EXISTS openclaw_connections (
    id BIGSERIAL PRIMARY KEY,
    user_id TEXT NOT NULL,
    name TEXT NOT NULL,
    provider TEXT NOT NULL,
    base_url TEXT NOT NULL,
    encrypted_api_key TEXT,
    config JSONB DEFAULT '{}',
    is_default BOOLEAN DEFAULT false,
    is_active BOOLEAN DEFAULT true,
    created_ts BIGINT NOT NULL,
    updated_ts BIGINT NOT NULL,
    UNIQUE(user_id, name)
);

COMMENT ON TABLE openclaw_connections IS 'OpenClaw 连接配置表';
COMMENT ON COLUMN openclaw_connections.user_id IS '用户 ID';
COMMENT ON COLUMN openclaw_connections.name IS '连接名称';
COMMENT ON COLUMN openclaw_connections.provider IS '提供商: openai, anthropic, ollama, openclaw, custom';
COMMENT ON COLUMN openclaw_connections.base_url IS 'API 端点 URL';
COMMENT ON COLUMN openclaw_connections.encrypted_api_key IS '加密存储的 API Key';
COMMENT ON COLUMN openclaw_connections.config IS '其他配置 (temperature, maxTokens 等)';
COMMENT ON COLUMN openclaw_connections.is_default IS '是否为默认连接';
COMMENT ON COLUMN openclaw_connections.is_active IS '是否激活';

-- ============================================
-- 2. AI 对话记录表
-- ============================================
CREATE TABLE IF NOT EXISTS ai_conversations (
    id BIGSERIAL PRIMARY KEY,
    user_id TEXT NOT NULL,
    connection_id BIGINT REFERENCES openclaw_connections(id) ON DELETE SET NULL,
    title TEXT,
    model_id TEXT,
    system_prompt TEXT,
    temperature REAL DEFAULT 0.7,
    max_tokens INTEGER DEFAULT 4096,
    is_pinned BOOLEAN DEFAULT false,
    metadata JSONB DEFAULT '{}',
    created_ts BIGINT NOT NULL,
    updated_ts BIGINT NOT NULL
);

COMMENT ON TABLE ai_conversations IS 'AI 对话记录表';
COMMENT ON COLUMN ai_conversations.user_id IS '用户 ID';
COMMENT ON COLUMN ai_conversations.connection_id IS '关联的 OpenClaw 连接';
COMMENT ON COLUMN ai_conversations.title IS '对话标题';
COMMENT ON COLUMN ai_conversations.model_id IS '使用的模型 ID';
COMMENT ON COLUMN ai_conversations.system_prompt IS '系统提示词';
COMMENT ON COLUMN ai_conversations.temperature IS '温度参数';
COMMENT ON COLUMN ai_conversations.max_tokens IS '最大 Token 数';
COMMENT ON COLUMN ai_conversations.is_pinned IS '是否置顶';
COMMENT ON COLUMN ai_conversations.metadata IS '其他元数据';

-- ============================================
-- 3. AI 消息记录表
-- ============================================
CREATE TABLE IF NOT EXISTS ai_messages (
    id BIGSERIAL PRIMARY KEY,
    conversation_id BIGINT NOT NULL REFERENCES ai_conversations(id) ON DELETE CASCADE,
    role TEXT NOT NULL CHECK (role IN ('user', 'assistant', 'system', 'tool')),
    content TEXT NOT NULL,
    token_count INTEGER,
    tool_calls JSONB,
    tool_call_id TEXT,
    metadata JSONB DEFAULT '{}',
    created_ts BIGINT NOT NULL
);

COMMENT ON TABLE ai_messages IS 'AI 消息记录表';
COMMENT ON COLUMN ai_messages.conversation_id IS '关联的对话 ID';
COMMENT ON COLUMN ai_messages.role IS '消息角色: user, assistant, system, tool';
COMMENT ON COLUMN ai_messages.content IS '消息内容';
COMMENT ON COLUMN ai_messages.token_count IS 'Token 数量';
COMMENT ON COLUMN ai_messages.tool_calls IS 'Function Calling 工具调用记录';
COMMENT ON COLUMN ai_messages.tool_call_id IS '工具调用 ID (用于关联工具响应)';
COMMENT ON COLUMN ai_messages.metadata IS '其他元数据';

-- ============================================
-- 4. AI 生成记录表 (图片/视频/音频)
-- ============================================
CREATE TABLE IF NOT EXISTS ai_generations (
    id BIGSERIAL PRIMARY KEY,
    user_id TEXT NOT NULL,
    conversation_id BIGINT REFERENCES ai_conversations(id) ON DELETE SET NULL,
    type TEXT NOT NULL CHECK (type IN ('image', 'video', 'audio')),
    prompt TEXT NOT NULL,
    result_url TEXT,
    result_mxc TEXT,
    status TEXT DEFAULT 'pending' CHECK (status IN ('pending', 'processing', 'completed', 'failed')),
    error_message TEXT,
    metadata JSONB DEFAULT '{}',
    created_ts BIGINT NOT NULL,
    completed_ts BIGINT
);

COMMENT ON TABLE ai_generations IS 'AI 生成记录表 (图片/视频/音频)';
COMMENT ON COLUMN ai_generations.user_id IS '用户 ID';
COMMENT ON COLUMN ai_generations.conversation_id IS '关联的对话 ID';
COMMENT ON COLUMN ai_generations.type IS '生成类型: image, video, audio';
COMMENT ON COLUMN ai_generations.prompt IS '提示词';
COMMENT ON COLUMN ai_generations.result_url IS '结果 URL';
COMMENT ON COLUMN ai_generations.result_mxc IS 'Matrix MXC URL';
COMMENT ON COLUMN ai_generations.status IS '状态: pending, processing, completed, failed';
COMMENT ON COLUMN ai_generations.error_message IS '错误信息';
COMMENT ON COLUMN ai_generations.metadata IS '其他元数据 (尺寸、时长等)';

-- ============================================
-- 5. AI 聊天角色表
-- ============================================
CREATE TABLE IF NOT EXISTS ai_chat_roles (
    id BIGSERIAL PRIMARY KEY,
    user_id TEXT NOT NULL,
    name TEXT NOT NULL,
    description TEXT,
    system_message TEXT NOT NULL,
    model_id TEXT,
    avatar_url TEXT,
    category TEXT,
    temperature REAL DEFAULT 0.7,
    max_tokens INTEGER DEFAULT 4096,
    is_public BOOLEAN DEFAULT false,
    metadata JSONB DEFAULT '{}',
    created_ts BIGINT NOT NULL,
    updated_ts BIGINT NOT NULL
);

COMMENT ON TABLE ai_chat_roles IS 'AI 聊天角色表';
COMMENT ON COLUMN ai_chat_roles.user_id IS '用户 ID';
COMMENT ON COLUMN ai_chat_roles.name IS '角色名称';
COMMENT ON COLUMN ai_chat_roles.description IS '角色描述';
COMMENT ON COLUMN ai_chat_roles.system_message IS '系统提示词';
COMMENT ON COLUMN ai_chat_roles.model_id IS '默认模型 ID';
COMMENT ON COLUMN ai_chat_roles.avatar_url IS '头像 URL';
COMMENT ON COLUMN ai_chat_roles.category IS '分类';
COMMENT ON COLUMN ai_chat_roles.temperature IS '默认温度参数';
COMMENT ON COLUMN ai_chat_roles.max_tokens IS '默认最大 Token 数';
COMMENT ON COLUMN ai_chat_roles.is_public IS '是否公开';

-- ============================================
-- 6. 索引
-- ============================================
CREATE INDEX IF NOT EXISTS idx_openclaw_connections_user ON openclaw_connections(user_id);
CREATE INDEX IF NOT EXISTS idx_openclaw_connections_provider ON openclaw_connections(provider);
CREATE INDEX IF NOT EXISTS idx_openclaw_connections_active ON openclaw_connections(is_active) WHERE is_active = true;

CREATE INDEX IF NOT EXISTS idx_ai_conversations_user ON ai_conversations(user_id);
CREATE INDEX IF NOT EXISTS idx_ai_conversations_connection ON ai_conversations(connection_id);
CREATE INDEX IF NOT EXISTS idx_ai_conversations_pinned ON ai_conversations(user_id, is_pinned) WHERE is_pinned = true;
CREATE INDEX IF NOT EXISTS idx_ai_conversations_updated ON ai_conversations(updated_ts DESC);

CREATE INDEX IF NOT EXISTS idx_ai_messages_conversation ON ai_messages(conversation_id);
CREATE INDEX IF NOT EXISTS idx_ai_messages_created ON ai_messages(conversation_id, created_ts DESC);
CREATE INDEX IF NOT EXISTS idx_ai_messages_role ON ai_messages(conversation_id, role);

CREATE INDEX IF NOT EXISTS idx_ai_generations_user ON ai_generations(user_id);
CREATE INDEX IF NOT EXISTS idx_ai_generations_conversation ON ai_generations(conversation_id);
CREATE INDEX IF NOT EXISTS idx_ai_generations_type ON ai_generations(user_id, type);
CREATE INDEX IF NOT EXISTS idx_ai_generations_status ON ai_generations(status) WHERE status IN ('pending', 'processing');

CREATE INDEX IF NOT EXISTS idx_ai_chat_roles_user ON ai_chat_roles(user_id);
CREATE INDEX IF NOT EXISTS idx_ai_chat_roles_public ON ai_chat_roles(is_public) WHERE is_public = true;
CREATE INDEX IF NOT EXISTS idx_ai_chat_roles_category ON ai_chat_roles(category);

-- ============================================
-- 7. 触发器：自动更新 updated_ts
-- ============================================
CREATE OR REPLACE FUNCTION update_updated_ts_column()
RETURNS TRIGGER AS $$
BEGIN
    NEW.updated_ts = EXTRACT(EPOCH FROM NOW())::BIGINT * 1000;
    RETURN NEW;
END;
$$ LANGUAGE plpgsql;

DO $$ BEGIN
IF NOT EXISTS (SELECT 1 FROM pg_trigger WHERE tgname = 'update_openclaw_connections_updated_ts') THEN
    CREATE TRIGGER update_openclaw_connections_updated_ts
        BEFORE UPDATE ON openclaw_connections
        FOR EACH ROW
        EXECUTE FUNCTION update_updated_ts_column();
END IF;
END $$;

DO $$ BEGIN
IF NOT EXISTS (SELECT 1 FROM pg_trigger WHERE tgname = 'update_ai_conversations_updated_ts') THEN
    CREATE TRIGGER update_ai_conversations_updated_ts
        BEFORE UPDATE ON ai_conversations
        FOR EACH ROW
        EXECUTE FUNCTION update_updated_ts_column();
END IF;
END $$;

DO $$ BEGIN
IF NOT EXISTS (SELECT 1 FROM pg_trigger WHERE tgname = 'update_ai_chat_roles_updated_ts') THEN
    CREATE TRIGGER update_ai_chat_roles_updated_ts
        BEFORE UPDATE ON ai_chat_roles
        FOR EACH ROW
        EXECUTE FUNCTION update_updated_ts_column();
END IF;
END $$;

-- ===== Merged from: 20260404000001_consolidated_schema_alignment.sql =====

--no-transaction
-- ============================================================================
-- Consolidated Schema Alignment Migration
-- Created: 2026-04-04
-- Description: Merges 10 schema alignment migrations into a single file
-- Original migrations: 20260330000001 through 20260330000013
-- ============================================================================

SET TIME ZONE 'UTC';

-- ============================================================================
-- Part 1: 20260330000001_add_thread_replies_and_receipts
-- Original file: 20260330000001_add_thread_replies_and_receipts.sql
-- ============================================================================

DO $$
BEGIN
    IF EXISTS (
        SELECT 1 FROM information_schema.columns
        WHERE table_schema = 'public' AND table_name = 'thread_roots' AND column_name = 'event_id'
    ) AND NOT EXISTS (
        SELECT 1 FROM information_schema.columns
        WHERE table_schema = 'public' AND table_name = 'thread_roots' AND column_name = 'root_event_id'
    ) THEN
        ALTER TABLE thread_roots RENAME COLUMN event_id TO root_event_id;
    END IF;

    IF EXISTS (
        SELECT 1 FROM pg_constraint
        WHERE conname = 'uq_thread_roots_room_event'
    ) AND NOT EXISTS (
        SELECT 1 FROM pg_constraint
        WHERE conname = 'uq_thread_roots_room_root_event'
    ) THEN
        ALTER TABLE thread_roots
        RENAME CONSTRAINT uq_thread_roots_room_event TO uq_thread_roots_room_root_event;
    END IF;

    IF EXISTS (
        SELECT 1 FROM pg_indexes
        WHERE schemaname = 'public' AND indexname = 'idx_thread_roots_event'
    ) AND NOT EXISTS (
        SELECT 1 FROM pg_indexes
        WHERE schemaname = 'public' AND indexname = 'idx_thread_roots_root_event'
    ) THEN
        ALTER INDEX idx_thread_roots_event RENAME TO idx_thread_roots_root_event;
    END IF;
END $$;

CREATE TABLE IF NOT EXISTS thread_replies (
    id BIGSERIAL PRIMARY KEY,
    room_id TEXT NOT NULL,
    thread_id TEXT NOT NULL,
    event_id TEXT NOT NULL,
    root_event_id TEXT NOT NULL,
    sender TEXT NOT NULL,
    in_reply_to_event_id TEXT,
    content JSONB NOT NULL DEFAULT '{}',
    origin_server_ts BIGINT NOT NULL,
    is_edited BOOLEAN NOT NULL DEFAULT FALSE,
    is_redacted BOOLEAN NOT NULL DEFAULT FALSE,
    created_ts BIGINT NOT NULL,
    CONSTRAINT uq_thread_replies_room_event UNIQUE (room_id, event_id),
    CONSTRAINT fk_thread_replies_room FOREIGN KEY (room_id) REFERENCES rooms(room_id) ON DELETE CASCADE
);

CREATE TABLE IF NOT EXISTS thread_read_receipts (
    id BIGSERIAL PRIMARY KEY,
    room_id TEXT NOT NULL,
    thread_id TEXT NOT NULL,
    user_id TEXT NOT NULL,
    last_read_event_id TEXT,
    last_read_ts BIGINT NOT NULL DEFAULT 0,
    unread_count INTEGER NOT NULL DEFAULT 0,
    updated_ts BIGINT NOT NULL,
    CONSTRAINT uq_thread_read_receipts_room_thread_user UNIQUE (room_id, thread_id, user_id),
    CONSTRAINT fk_thread_read_receipts_room FOREIGN KEY (room_id) REFERENCES rooms(room_id) ON DELETE CASCADE,
    CONSTRAINT fk_thread_read_receipts_user FOREIGN KEY (user_id) REFERENCES users(user_id) ON DELETE CASCADE
);



-- ============================================================================
-- Part 2: 20260330000002_align_thread_schema_and_relations
-- Original file: 20260330000002_align_thread_schema_and_relations.sql
-- ============================================================================

CREATE TABLE IF NOT EXISTS thread_relations (
    id BIGSERIAL PRIMARY KEY,
    room_id TEXT NOT NULL,
    event_id TEXT NOT NULL,
    relates_to_event_id TEXT NOT NULL,
    relation_type TEXT NOT NULL,
    thread_id TEXT,
    is_falling_back BOOLEAN NOT NULL DEFAULT FALSE,
    created_ts BIGINT NOT NULL,
    CONSTRAINT uq_thread_relations_room_event_type UNIQUE (room_id, event_id, relation_type),
    CONSTRAINT fk_thread_relations_room FOREIGN KEY (room_id) REFERENCES rooms(room_id) ON DELETE CASCADE
);



-- ============================================================================
-- Part 3: 20260330000003_align_retention_and_room_summary_schema
-- Original file: 20260330000003_align_retention_and_room_summary_schema.sql
-- ============================================================================

DO $$
BEGIN
    IF EXISTS (SELECT 1 FROM information_schema.tables WHERE table_name = 'room_summaries') THEN
        IF EXISTS (
            SELECT 1 FROM information_schema.columns
            WHERE table_schema = 'public' AND table_name = 'room_summaries' AND column_name = 'joined_members'
        ) AND NOT EXISTS (
            SELECT 1 FROM information_schema.columns
            WHERE table_schema = 'public' AND table_name = 'room_summaries' AND column_name = 'joined_member_count'
        ) THEN
            ALTER TABLE room_summaries RENAME COLUMN joined_members TO joined_member_count;
        END IF;

        IF EXISTS (
            SELECT 1 FROM information_schema.columns
            WHERE table_schema = 'public' AND table_name = 'room_summaries' AND column_name = 'invited_members'
        ) AND NOT EXISTS (
            SELECT 1 FROM information_schema.columns
            WHERE table_schema = 'public' AND table_name = 'room_summaries' AND column_name = 'invited_member_count'
        ) THEN
            ALTER TABLE room_summaries RENAME COLUMN invited_members TO invited_member_count;
        END IF;

        ALTER TABLE room_summaries
            ADD COLUMN IF NOT EXISTS id BIGSERIAL,
            ADD COLUMN IF NOT EXISTS room_type TEXT,
            ADD COLUMN IF NOT EXISTS avatar_url TEXT,
            ADD COLUMN IF NOT EXISTS join_rules TEXT NOT NULL DEFAULT 'invite',
            ADD COLUMN IF NOT EXISTS history_visibility TEXT NOT NULL DEFAULT 'shared',
            ADD COLUMN IF NOT EXISTS guest_access TEXT NOT NULL DEFAULT 'forbidden',
            ADD COLUMN IF NOT EXISTS is_direct BOOLEAN NOT NULL DEFAULT FALSE,
            ADD COLUMN IF NOT EXISTS is_space BOOLEAN NOT NULL DEFAULT FALSE,
            ADD COLUMN IF NOT EXISTS is_encrypted BOOLEAN NOT NULL DEFAULT FALSE,
            ADD COLUMN IF NOT EXISTS joined_member_count BIGINT NOT NULL DEFAULT 0,
            ADD COLUMN IF NOT EXISTS invited_member_count BIGINT NOT NULL DEFAULT 0,
            ADD COLUMN IF NOT EXISTS last_event_id TEXT,
            ADD COLUMN IF NOT EXISTS last_event_ts BIGINT,
            ADD COLUMN IF NOT EXISTS last_message_ts BIGINT,
            ADD COLUMN IF NOT EXISTS unread_notifications BIGINT NOT NULL DEFAULT 0,
            ADD COLUMN IF NOT EXISTS unread_highlight BIGINT NOT NULL DEFAULT 0,
            ADD COLUMN IF NOT EXISTS created_ts BIGINT NOT NULL DEFAULT 0;

        UPDATE room_summaries
        SET hero_users = '[]'::jsonb
        WHERE hero_users IS NULL;

        UPDATE room_summaries
        SET updated_ts = 0
        WHERE updated_ts IS NULL;

        CREATE UNIQUE INDEX IF NOT EXISTS idx_room_summaries_id_unique
        ON room_summaries(id);

        IF NOT EXISTS (
            SELECT 1 FROM pg_constraint
            WHERE conname = 'fk_room_summaries_room'
        ) THEN
            ALTER TABLE room_summaries
            ADD CONSTRAINT fk_room_summaries_room
            FOREIGN KEY (room_id) REFERENCES rooms(room_id) ON DELETE CASCADE;
        END IF;
    END IF;
END $$;

DO $$
BEGIN
    IF EXISTS (SELECT 1 FROM information_schema.tables WHERE table_schema = 'public' AND table_name = 'server_retention_policy') THEN
        ALTER TABLE server_retention_policy
            ADD COLUMN IF NOT EXISTS max_lifetime BIGINT,
            ADD COLUMN IF NOT EXISTS min_lifetime BIGINT NOT NULL DEFAULT 0,
            ADD COLUMN IF NOT EXISTS expire_on_clients BOOLEAN NOT NULL DEFAULT FALSE;

        IF EXISTS (
            SELECT 1
            FROM information_schema.columns
            WHERE table_schema = 'public'
              AND table_name = 'server_retention_policy'
              AND column_name = 'max_lifetime_days'
        ) AND EXISTS (
            SELECT 1
            FROM information_schema.columns
            WHERE table_schema = 'public'
              AND table_name = 'server_retention_policy'
              AND column_name = 'min_lifetime_days'
        ) THEN
            EXECUTE $stmt$
                UPDATE server_retention_policy
                SET
                    max_lifetime = COALESCE(max_lifetime, max_lifetime_days::BIGINT * 86400000),
                    min_lifetime = COALESCE(min_lifetime, min_lifetime_days::BIGINT * 86400000),
                    updated_ts = COALESCE(updated_ts, created_ts, 0)
                WHERE
                    max_lifetime IS NULL
                    OR min_lifetime = 0
                    OR updated_ts IS NULL
            $stmt$;
        ELSE
            UPDATE server_retention_policy
            SET updated_ts = COALESCE(updated_ts, created_ts, 0)
            WHERE updated_ts IS NULL;
        END IF;

        INSERT INTO server_retention_policy (id, max_lifetime, min_lifetime, expire_on_clients, created_ts, updated_ts)
        VALUES (1, NULL, 0, FALSE, 0, 0)
        ON CONFLICT (id) DO NOTHING;
    ELSE
        CREATE TABLE IF NOT EXISTS server_retention_policy (
            id BIGINT PRIMARY KEY DEFAULT 1,
            max_lifetime BIGINT,
            min_lifetime BIGINT NOT NULL DEFAULT 0,
            expire_on_clients BOOLEAN NOT NULL DEFAULT FALSE,
            created_ts BIGINT NOT NULL DEFAULT 0,
            updated_ts BIGINT NOT NULL DEFAULT 0
        );
        INSERT INTO server_retention_policy (id, max_lifetime, min_lifetime, expire_on_clients, created_ts, updated_ts)
        VALUES (1, NULL, 0, FALSE, 0, 0)
        ON CONFLICT (id) DO NOTHING;
    END IF;
END
$$;



-- ============================================================================
-- Part 4: 20260330000004_align_space_schema_and_add_space_events
-- Original file: 20260330000004_align_space_schema_and_add_space_events.sql
-- ============================================================================

DO $$
BEGIN
    IF EXISTS (SELECT 1 FROM information_schema.tables WHERE table_schema = 'public' AND table_name = 'spaces') THEN
        IF NOT EXISTS (
            SELECT 1 FROM information_schema.columns
            WHERE table_schema = 'public' AND table_name = 'spaces' AND column_name = 'room_id'
        ) THEN
            ALTER TABLE spaces ADD COLUMN room_id TEXT;
        END IF;

        IF NOT EXISTS (
            SELECT 1 FROM information_schema.columns
            WHERE table_schema = 'public' AND table_name = 'spaces' AND column_name = 'join_rule'
        ) THEN
            ALTER TABLE spaces ADD COLUMN join_rule TEXT DEFAULT 'invite';
        END IF;

        IF NOT EXISTS (
            SELECT 1 FROM information_schema.columns
            WHERE table_schema = 'public' AND table_name = 'spaces' AND column_name = 'visibility'
        ) THEN
            ALTER TABLE spaces ADD COLUMN visibility TEXT DEFAULT 'private';
        END IF;

        IF NOT EXISTS (
            SELECT 1 FROM information_schema.columns
            WHERE table_schema = 'public' AND table_name = 'spaces' AND column_name = 'parent_space_id'
        ) THEN
            ALTER TABLE spaces ADD COLUMN parent_space_id TEXT;
        END IF;

        IF EXISTS (
            SELECT 1 FROM information_schema.columns
            WHERE table_schema = 'public' AND table_name = 'spaces' AND column_name = 'join_rules'
        ) THEN
            EXECUTE $sql$
                UPDATE spaces
                SET join_rule = COALESCE(join_rule, join_rules, 'invite')
                WHERE join_rule IS NULL
            $sql$;
        ELSE
            UPDATE spaces
            SET join_rule = COALESCE(join_rule, 'invite')
            WHERE join_rule IS NULL;
        END IF;
    END IF;
END $$;

CREATE TABLE IF NOT EXISTS space_summaries (
    id BIGSERIAL PRIMARY KEY,
    space_id TEXT NOT NULL UNIQUE,
    summary JSONB DEFAULT '{}',
    children_count BIGINT DEFAULT 0,
    member_count BIGINT DEFAULT 0,
    updated_ts BIGINT NOT NULL
);



-- ============================================================================
-- Part 5: 20260330000005_align_remaining_schema_exceptions
-- Original file: 20260330000005_align_remaining_schema_exceptions.sql
-- ============================================================================

DO $$
BEGIN
    CREATE TABLE IF NOT EXISTS room_summary_state (
        id BIGSERIAL PRIMARY KEY,
        room_id TEXT NOT NULL,
        event_type TEXT NOT NULL,
        state_key TEXT NOT NULL,
        event_id TEXT,
        content JSONB NOT NULL DEFAULT '{}',
        updated_ts BIGINT NOT NULL,
        CONSTRAINT uq_room_summary_state_room_type_state UNIQUE (room_id, event_type, state_key),
        CONSTRAINT fk_room_summary_state_room FOREIGN KEY (room_id) REFERENCES rooms(room_id) ON DELETE CASCADE
    );

    CREATE TABLE IF NOT EXISTS room_summary_stats (
        id BIGSERIAL PRIMARY KEY,
        room_id TEXT NOT NULL UNIQUE,
        total_events BIGINT NOT NULL DEFAULT 0,
        total_state_events BIGINT NOT NULL DEFAULT 0,
        total_messages BIGINT NOT NULL DEFAULT 0,
        total_media BIGINT NOT NULL DEFAULT 0,
        storage_size BIGINT NOT NULL DEFAULT 0,
        last_updated_ts BIGINT NOT NULL,
        CONSTRAINT fk_room_summary_stats_room FOREIGN KEY (room_id) REFERENCES rooms(room_id) ON DELETE CASCADE
    );

    CREATE TABLE IF NOT EXISTS room_summary_update_queue (
        id BIGSERIAL PRIMARY KEY,
        room_id TEXT NOT NULL,
        event_id TEXT NOT NULL,
        event_type TEXT NOT NULL,
        state_key TEXT,
        priority INTEGER NOT NULL DEFAULT 0,
        status TEXT NOT NULL DEFAULT 'pending',
        created_ts BIGINT NOT NULL,
        processed_ts BIGINT,
        error_message TEXT,
        retry_count INTEGER NOT NULL DEFAULT 0,
        CONSTRAINT fk_room_summary_update_queue_room FOREIGN KEY (room_id) REFERENCES rooms(room_id) ON DELETE CASCADE
    );

    CREATE TABLE IF NOT EXISTS room_children (
        id BIGSERIAL PRIMARY KEY,
        parent_room_id TEXT NOT NULL,
        child_room_id TEXT NOT NULL,
        state_key TEXT,
        content JSONB NOT NULL DEFAULT '{}',
        suggested BOOLEAN NOT NULL DEFAULT FALSE,
        created_ts BIGINT NOT NULL DEFAULT 0,
        updated_ts BIGINT,
        CONSTRAINT uq_room_children_parent_child UNIQUE (parent_room_id, child_room_id),
        CONSTRAINT fk_room_children_parent FOREIGN KEY (parent_room_id) REFERENCES rooms(room_id) ON DELETE CASCADE,
        CONSTRAINT fk_room_children_child FOREIGN KEY (child_room_id) REFERENCES rooms(room_id) ON DELETE CASCADE
    );

    CREATE TABLE IF NOT EXISTS retention_cleanup_queue (
        id BIGSERIAL PRIMARY KEY,
        room_id TEXT NOT NULL,
        event_id TEXT,
        event_type TEXT,
        origin_server_ts BIGINT NOT NULL,
        scheduled_ts BIGINT NOT NULL,
        status TEXT NOT NULL DEFAULT 'pending',
        created_ts BIGINT NOT NULL,
        processed_ts BIGINT,
        error_message TEXT,
        retry_count INTEGER NOT NULL DEFAULT 0,
        CONSTRAINT uq_retention_cleanup_queue_room_event UNIQUE (room_id, event_id),
        CONSTRAINT fk_retention_cleanup_queue_room FOREIGN KEY (room_id) REFERENCES rooms(room_id) ON DELETE CASCADE
    );

    CREATE TABLE IF NOT EXISTS retention_cleanup_logs (
        id BIGSERIAL PRIMARY KEY,
        room_id TEXT NOT NULL,
        events_deleted BIGINT NOT NULL DEFAULT 0,
        state_events_deleted BIGINT NOT NULL DEFAULT 0,
        media_deleted BIGINT NOT NULL DEFAULT 0,
        bytes_freed BIGINT NOT NULL DEFAULT 0,
        started_ts BIGINT NOT NULL,
        completed_ts BIGINT,
        status TEXT NOT NULL,
        error_message TEXT,
        CONSTRAINT fk_retention_cleanup_logs_room FOREIGN KEY (room_id) REFERENCES rooms(room_id) ON DELETE CASCADE
    );

    CREATE TABLE IF NOT EXISTS retention_stats (
        id BIGSERIAL PRIMARY KEY,
        room_id TEXT NOT NULL UNIQUE,
        total_events BIGINT NOT NULL DEFAULT 0,
        events_in_retention BIGINT NOT NULL DEFAULT 0,
        events_expired BIGINT NOT NULL DEFAULT 0,
        last_cleanup_ts BIGINT,
        next_cleanup_ts BIGINT,
        CONSTRAINT fk_retention_stats_room FOREIGN KEY (room_id) REFERENCES rooms(room_id) ON DELETE CASCADE
    );

    CREATE TABLE IF NOT EXISTS deleted_events_index (
        id BIGSERIAL PRIMARY KEY,
        room_id TEXT NOT NULL,
        event_id TEXT NOT NULL,
        deletion_ts BIGINT NOT NULL,
        reason TEXT NOT NULL,
        CONSTRAINT uq_deleted_events_index_room_event UNIQUE (room_id, event_id),
        CONSTRAINT fk_deleted_events_index_room FOREIGN KEY (room_id) REFERENCES rooms(room_id) ON DELETE CASCADE
    );

    CREATE TABLE IF NOT EXISTS device_trust_status (
        id BIGSERIAL PRIMARY KEY,
        user_id TEXT NOT NULL,
        device_id TEXT NOT NULL,
        trust_level TEXT NOT NULL DEFAULT 'unverified',
        verified_by_device_id TEXT,
        verified_at TIMESTAMPTZ,
        created_ts BIGINT NOT NULL,
        updated_ts BIGINT,
        CONSTRAINT uq_device_trust_status_user_device UNIQUE (user_id, device_id)
    );

    CREATE TABLE IF NOT EXISTS cross_signing_trust (
        id BIGSERIAL PRIMARY KEY,
        user_id TEXT NOT NULL,
        target_user_id TEXT NOT NULL,
        master_key_id TEXT,
        is_trusted BOOLEAN NOT NULL DEFAULT FALSE,
        trusted_at TIMESTAMPTZ,
        created_ts BIGINT NOT NULL,
        updated_ts BIGINT,
        CONSTRAINT uq_cross_signing_trust_user_target UNIQUE (user_id, target_user_id)
    );

    CREATE TABLE IF NOT EXISTS verification_requests (
        transaction_id TEXT PRIMARY KEY,
        from_user TEXT NOT NULL,
        from_device TEXT NOT NULL,
        to_user TEXT NOT NULL,
        to_device TEXT,
        method TEXT NOT NULL,
        state TEXT NOT NULL,
        created_ts BIGINT NOT NULL,
        updated_ts BIGINT
    );

    CREATE TABLE IF NOT EXISTS verification_sas (
        tx_id TEXT PRIMARY KEY,
        from_device TEXT NOT NULL,
        to_device TEXT,
        method TEXT NOT NULL,
        state TEXT NOT NULL,
        exchange_hashes JSONB NOT NULL DEFAULT '[]',
        commitment TEXT,
        pubkey TEXT,
        sas_bytes BYTEA,
        mac TEXT
    );

    CREATE TABLE IF NOT EXISTS verification_qr (
        tx_id TEXT PRIMARY KEY,
        from_device TEXT NOT NULL,
        to_device TEXT,
        state TEXT NOT NULL,
        qr_code_data TEXT,
        scanned_data TEXT
    );

    CREATE TABLE IF NOT EXISTS moderation_actions (
        id BIGSERIAL PRIMARY KEY,
        user_id TEXT NOT NULL,
        action_type TEXT NOT NULL,
        reason TEXT,
        report_id BIGINT,
        created_ts BIGINT NOT NULL,
        expires_at BIGINT,
        revoked BOOLEAN NOT NULL DEFAULT FALSE,
        revoked_reason TEXT,
        revoked_at BIGINT,
        CONSTRAINT fk_moderation_actions_user FOREIGN KEY (user_id) REFERENCES users(user_id) ON DELETE CASCADE
    );

    CREATE TABLE IF NOT EXISTS moderation_rules (
        id BIGSERIAL PRIMARY KEY,
        rule_id TEXT NOT NULL UNIQUE,
        server_id TEXT,
        rule_type TEXT NOT NULL,
        pattern TEXT NOT NULL,
        action TEXT NOT NULL,
        reason TEXT,
        created_by TEXT NOT NULL,
        created_ts BIGINT NOT NULL,
        updated_ts BIGINT NOT NULL,
        is_active BOOLEAN NOT NULL DEFAULT TRUE,
        priority INTEGER NOT NULL DEFAULT 100
    );

    CREATE TABLE IF NOT EXISTS moderation_logs (
        id BIGSERIAL PRIMARY KEY,
        rule_id TEXT NOT NULL,
        event_id TEXT NOT NULL,
        room_id TEXT NOT NULL,
        sender TEXT NOT NULL,
        content_hash TEXT NOT NULL,
        action_taken TEXT NOT NULL,
        confidence REAL NOT NULL,
        created_ts BIGINT NOT NULL
    );

    CREATE TABLE IF NOT EXISTS replication_positions (
        id BIGSERIAL PRIMARY KEY,
        worker_id TEXT NOT NULL,
        stream_name TEXT NOT NULL,
        stream_position BIGINT NOT NULL DEFAULT 0,
        updated_ts BIGINT NOT NULL,
        CONSTRAINT uq_replication_positions_worker_stream UNIQUE (worker_id, stream_name),
        CONSTRAINT fk_replication_positions_worker FOREIGN KEY (worker_id) REFERENCES workers(worker_id) ON DELETE CASCADE
    );

    CREATE TABLE IF NOT EXISTS worker_load_stats (
        id BIGSERIAL PRIMARY KEY,
        worker_id TEXT NOT NULL,
        cpu_usage REAL,
        memory_usage BIGINT,
        active_connections INTEGER,
        requests_per_second REAL,
        average_latency_ms REAL,
        queue_depth INTEGER,
        recorded_ts BIGINT NOT NULL,
        CONSTRAINT fk_worker_load_stats_worker FOREIGN KEY (worker_id) REFERENCES workers(worker_id) ON DELETE CASCADE
    );

    CREATE TABLE IF NOT EXISTS worker_task_assignments (
        id BIGSERIAL PRIMARY KEY,
        task_id TEXT NOT NULL UNIQUE,
        task_type TEXT NOT NULL,
        task_data JSONB NOT NULL DEFAULT '{}',
        priority INTEGER NOT NULL DEFAULT 0,
        status TEXT NOT NULL DEFAULT 'pending',
        assigned_worker_id TEXT,
        assigned_ts BIGINT,
        created_ts BIGINT NOT NULL,
        completed_ts BIGINT,
        result JSONB,
        error_message TEXT,
        CONSTRAINT fk_worker_task_assignments_worker FOREIGN KEY (assigned_worker_id) REFERENCES workers(worker_id) ON DELETE SET NULL
    );

    CREATE TABLE IF NOT EXISTS worker_connections (
        id BIGSERIAL PRIMARY KEY,
        source_worker_id TEXT NOT NULL,
        target_worker_id TEXT NOT NULL,
        connection_type TEXT NOT NULL,
        status TEXT NOT NULL DEFAULT 'connected',
        established_ts BIGINT NOT NULL,
        last_activity_ts BIGINT,
        bytes_sent BIGINT NOT NULL DEFAULT 0,
        bytes_received BIGINT NOT NULL DEFAULT 0,
        messages_sent BIGINT NOT NULL DEFAULT 0,
        messages_received BIGINT NOT NULL DEFAULT 0,
        CONSTRAINT uq_worker_connections_pair UNIQUE (source_worker_id, target_worker_id, connection_type),
        CONSTRAINT fk_worker_connections_source FOREIGN KEY (source_worker_id) REFERENCES workers(worker_id) ON DELETE CASCADE,
        CONSTRAINT fk_worker_connections_target FOREIGN KEY (target_worker_id) REFERENCES workers(worker_id) ON DELETE CASCADE
    );

    CREATE TABLE IF NOT EXISTS widgets (
        id BIGSERIAL PRIMARY KEY,
        widget_id TEXT NOT NULL UNIQUE,
        room_id TEXT,
        user_id TEXT NOT NULL,
        widget_type TEXT NOT NULL,
        url TEXT NOT NULL,
        name TEXT NOT NULL,
        data JSONB NOT NULL DEFAULT '{}',
        created_ts BIGINT NOT NULL,
        updated_ts BIGINT,
        is_active BOOLEAN NOT NULL DEFAULT TRUE,
        CONSTRAINT fk_widgets_room FOREIGN KEY (room_id) REFERENCES rooms(room_id) ON DELETE CASCADE,
        CONSTRAINT fk_widgets_user FOREIGN KEY (user_id) REFERENCES users(user_id) ON DELETE CASCADE
    );

    CREATE TABLE IF NOT EXISTS widget_permissions (
        id BIGSERIAL PRIMARY KEY,
        widget_id TEXT NOT NULL,
        user_id TEXT NOT NULL,
        permissions JSONB NOT NULL DEFAULT '[]',
        created_ts BIGINT NOT NULL,
        updated_ts BIGINT,
        CONSTRAINT uq_widget_permissions_widget_user UNIQUE (widget_id, user_id),
        CONSTRAINT fk_widget_permissions_widget FOREIGN KEY (widget_id) REFERENCES widgets(widget_id) ON DELETE CASCADE,
        CONSTRAINT fk_widget_permissions_user FOREIGN KEY (user_id) REFERENCES users(user_id) ON DELETE CASCADE
    );

    CREATE TABLE IF NOT EXISTS widget_sessions (
        id BIGSERIAL PRIMARY KEY,
        session_id TEXT NOT NULL UNIQUE,
        widget_id TEXT NOT NULL,
        user_id TEXT NOT NULL,
        device_id TEXT,
        created_ts BIGINT NOT NULL,
        last_active_ts BIGINT,
        expires_at BIGINT,
        is_active BOOLEAN NOT NULL DEFAULT TRUE,
        CONSTRAINT fk_widget_sessions_widget FOREIGN KEY (widget_id) REFERENCES widgets(widget_id) ON DELETE CASCADE,
        CONSTRAINT fk_widget_sessions_user FOREIGN KEY (user_id) REFERENCES users(user_id) ON DELETE CASCADE
    );

    CREATE TABLE IF NOT EXISTS server_notifications (
        id BIGSERIAL PRIMARY KEY,
        title TEXT NOT NULL,
        content TEXT NOT NULL,
        notification_type TEXT NOT NULL DEFAULT 'info',
        priority INTEGER NOT NULL DEFAULT 0,
        target_audience TEXT NOT NULL DEFAULT 'all',
        target_user_ids JSONB NOT NULL DEFAULT '[]',
        starts_at BIGINT,
        expires_at BIGINT,
        is_enabled BOOLEAN NOT NULL DEFAULT TRUE,
        is_dismissable BOOLEAN NOT NULL DEFAULT TRUE,
        action_url TEXT,
        action_text TEXT,
        created_by TEXT,
        created_ts BIGINT NOT NULL DEFAULT (EXTRACT(EPOCH FROM clock_timestamp()) * 1000)::BIGINT,
        updated_ts BIGINT NOT NULL DEFAULT (EXTRACT(EPOCH FROM clock_timestamp()) * 1000)::BIGINT
    );

    CREATE TABLE IF NOT EXISTS user_notification_status (
        id BIGSERIAL PRIMARY KEY,
        user_id TEXT NOT NULL,
        notification_id BIGINT NOT NULL,
        is_read BOOLEAN NOT NULL DEFAULT FALSE,
        is_dismissed BOOLEAN NOT NULL DEFAULT FALSE,
        read_ts BIGINT,
        dismissed_ts BIGINT,
        created_ts BIGINT NOT NULL DEFAULT (EXTRACT(EPOCH FROM clock_timestamp()) * 1000)::BIGINT,
        CONSTRAINT uq_user_notification_status_user_notification UNIQUE (user_id, notification_id),
        CONSTRAINT fk_user_notification_status_user FOREIGN KEY (user_id) REFERENCES users(user_id) ON DELETE CASCADE,
        CONSTRAINT fk_user_notification_status_notification FOREIGN KEY (notification_id) REFERENCES server_notifications(id) ON DELETE CASCADE
    );

    CREATE TABLE IF NOT EXISTS notification_templates (
        id BIGSERIAL PRIMARY KEY,
        name TEXT NOT NULL UNIQUE,
        title_template TEXT NOT NULL,
        content_template TEXT NOT NULL,
        notification_type TEXT NOT NULL DEFAULT 'info',
        variables JSONB NOT NULL DEFAULT '[]',
        is_enabled BOOLEAN NOT NULL DEFAULT TRUE,
        created_ts BIGINT NOT NULL DEFAULT (EXTRACT(EPOCH FROM clock_timestamp()) * 1000)::BIGINT,
        updated_ts BIGINT NOT NULL DEFAULT (EXTRACT(EPOCH FROM clock_timestamp()) * 1000)::BIGINT
    );

    CREATE TABLE IF NOT EXISTS notification_delivery_log (
        id BIGSERIAL PRIMARY KEY,
        notification_id BIGINT NOT NULL,
        user_id TEXT,
        delivery_method TEXT NOT NULL,
        status TEXT NOT NULL,
        error_message TEXT,
        delivered_ts BIGINT NOT NULL DEFAULT (EXTRACT(EPOCH FROM clock_timestamp()) * 1000)::BIGINT,
        CONSTRAINT fk_notification_delivery_log_notification FOREIGN KEY (notification_id) REFERENCES server_notifications(id) ON DELETE CASCADE,
        CONSTRAINT fk_notification_delivery_log_user FOREIGN KEY (user_id) REFERENCES users(user_id) ON DELETE CASCADE
    );

    CREATE TABLE IF NOT EXISTS scheduled_notifications (
        id BIGSERIAL PRIMARY KEY,
        notification_id BIGINT NOT NULL,
        scheduled_for BIGINT NOT NULL,
        is_sent BOOLEAN NOT NULL DEFAULT FALSE,
        sent_ts BIGINT,
        created_ts BIGINT NOT NULL DEFAULT (EXTRACT(EPOCH FROM clock_timestamp()) * 1000)::BIGINT,
        CONSTRAINT fk_scheduled_notifications_notification FOREIGN KEY (notification_id) REFERENCES server_notifications(id) ON DELETE CASCADE
    );

    CREATE TABLE IF NOT EXISTS secure_key_backups (
        user_id TEXT NOT NULL,
        backup_id TEXT NOT NULL,
        version TEXT NOT NULL,
        algorithm TEXT NOT NULL,
        auth_data TEXT NOT NULL,
        key_count BIGINT NOT NULL DEFAULT 0,
        created_ts BIGINT NOT NULL DEFAULT (EXTRACT(EPOCH FROM clock_timestamp()) * 1000)::BIGINT,
        updated_ts BIGINT DEFAULT (EXTRACT(EPOCH FROM clock_timestamp()) * 1000)::BIGINT,
        CONSTRAINT pk_secure_key_backups PRIMARY KEY (user_id, backup_id),
        CONSTRAINT fk_secure_key_backups_user FOREIGN KEY (user_id) REFERENCES users(user_id) ON DELETE CASCADE
    );

    CREATE TABLE IF NOT EXISTS secure_backup_session_keys (
        user_id TEXT NOT NULL,
        backup_id TEXT NOT NULL,
        room_id TEXT NOT NULL,
        session_id TEXT NOT NULL,
        encrypted_key TEXT NOT NULL,
        created_ts BIGINT NOT NULL DEFAULT (EXTRACT(EPOCH FROM clock_timestamp()) * 1000)::BIGINT,
        CONSTRAINT pk_secure_backup_session_keys PRIMARY KEY (user_id, backup_id, room_id, session_id),
        CONSTRAINT fk_secure_backup_session_keys_backup FOREIGN KEY (user_id, backup_id) REFERENCES secure_key_backups(user_id, backup_id) ON DELETE CASCADE
    );

    CREATE TABLE IF NOT EXISTS application_service_users (
        as_id TEXT NOT NULL,
        user_id TEXT NOT NULL,
        displayname TEXT,
        avatar_url TEXT,
        created_ts BIGINT NOT NULL,
        CONSTRAINT pk_application_service_users PRIMARY KEY (as_id, user_id)
    );

    CREATE TABLE IF NOT EXISTS application_service_statistics (
        id BIGSERIAL PRIMARY KEY,
        as_id TEXT NOT NULL UNIQUE,
        name TEXT,
        is_enabled BOOLEAN NOT NULL DEFAULT FALSE,
        rate_limited BOOLEAN NOT NULL DEFAULT TRUE,
        virtual_user_count BIGINT NOT NULL DEFAULT 0,
        pending_event_count BIGINT NOT NULL DEFAULT 0,
        pending_transaction_count BIGINT NOT NULL DEFAULT 0,
        last_seen_ts BIGINT,
        created_ts BIGINT NOT NULL DEFAULT (EXTRACT(EPOCH FROM clock_timestamp()) * 1000)::BIGINT
    );
END $$;

CREATE INDEX IF NOT EXISTS idx_widgets_room_active_created
ON widgets(room_id, created_ts DESC)
WHERE is_active = TRUE;

CREATE INDEX IF NOT EXISTS idx_widgets_user_active_created
ON widgets(user_id, created_ts DESC)
WHERE is_active = TRUE;

CREATE INDEX IF NOT EXISTS idx_widget_permissions_widget
ON widget_permissions(widget_id);

CREATE INDEX IF NOT EXISTS idx_widget_permissions_user
ON widget_permissions(user_id);

CREATE INDEX IF NOT EXISTS idx_widget_sessions_widget_active_last_active
ON widget_sessions(widget_id, last_active_ts DESC)
WHERE is_active = TRUE;

CREATE INDEX IF NOT EXISTS idx_server_notifications_enabled_priority_created
ON server_notifications(priority DESC, created_ts DESC)
WHERE is_enabled = TRUE;

CREATE INDEX IF NOT EXISTS idx_user_notification_status_user_created
ON user_notification_status(user_id, created_ts DESC);

CREATE INDEX IF NOT EXISTS idx_notification_templates_enabled
ON notification_templates(is_enabled)
WHERE is_enabled = TRUE;

CREATE INDEX IF NOT EXISTS idx_notification_delivery_log_notification_delivered
ON notification_delivery_log(notification_id, delivered_ts DESC);

CREATE INDEX IF NOT EXISTS idx_scheduled_notifications_pending
ON scheduled_notifications(scheduled_for)
WHERE is_sent = FALSE;

CREATE INDEX IF NOT EXISTS idx_secure_key_backups_user_created
ON secure_key_backups(user_id, created_ts DESC);

CREATE INDEX IF NOT EXISTS idx_secure_backup_session_keys_backup
ON secure_backup_session_keys(user_id, backup_id);

CREATE INDEX IF NOT EXISTS idx_application_service_users_as
ON application_service_users(as_id);

CREATE OR REPLACE VIEW active_workers AS
SELECT id, worker_id, worker_name, worker_type, host, port, status,
       last_heartbeat_ts, started_ts, stopped_ts, config, metadata, version, is_enabled
FROM workers
WHERE status = 'running' OR status = 'starting';

CREATE OR REPLACE VIEW worker_type_statistics AS
SELECT
    w.worker_type,
    COUNT(*)::BIGINT AS total_count,
    COUNT(*) FILTER (WHERE w.status = 'running')::BIGINT AS running_count,
    COUNT(*) FILTER (WHERE w.status = 'starting')::BIGINT AS starting_count,
    COUNT(*) FILTER (WHERE w.status = 'stopping')::BIGINT AS stopping_count,
    COUNT(*) FILTER (WHERE w.status = 'stopped')::BIGINT AS stopped_count,
    AVG(ls.cpu_usage)::DOUBLE PRECISION AS avg_cpu_usage,
    AVG(ls.memory_usage)::DOUBLE PRECISION AS avg_memory_usage,
    COALESCE(SUM(conn.connection_count), 0)::BIGINT AS total_connections
FROM workers w
LEFT JOIN LATERAL (
    SELECT cpu_usage, memory_usage
    FROM worker_load_stats
    WHERE worker_id = w.worker_id
    ORDER BY recorded_ts DESC
    LIMIT 1
) ls ON TRUE
LEFT JOIN LATERAL (
    SELECT COUNT(*)::BIGINT AS connection_count
    FROM worker_connections
    WHERE source_worker_id = w.worker_id AND status = 'connected'
) conn ON TRUE
GROUP BY w.worker_type;


-- ============================================================================
-- Part 6: 20260330000006_align_notifications_push_and_misc_exceptions
-- Original file: 20260330000006_align_notifications_push_and_misc_exceptions.sql
-- ============================================================================

DO $$
BEGIN
    CREATE TABLE IF NOT EXISTS push_device (
        id BIGSERIAL PRIMARY KEY,
        user_id TEXT NOT NULL,
        device_id TEXT NOT NULL,
        push_token TEXT NOT NULL,
        push_type TEXT NOT NULL,
        app_id TEXT,
        platform TEXT,
        platform_version TEXT,
        app_version TEXT,
        locale TEXT,
        timezone TEXT,
        is_enabled BOOLEAN NOT NULL DEFAULT TRUE,
        created_ts BIGINT NOT NULL,
        updated_ts BIGINT,
        last_used_at TIMESTAMPTZ,
        last_error TEXT,
        error_count INTEGER NOT NULL DEFAULT 0,
        metadata JSONB NOT NULL DEFAULT '{}',
        CONSTRAINT uq_push_device_user_device UNIQUE (user_id, device_id),
        CONSTRAINT fk_push_device_user FOREIGN KEY (user_id) REFERENCES users(user_id) ON DELETE CASCADE
    );

    CREATE TABLE IF NOT EXISTS rate_limits (
        user_id TEXT PRIMARY KEY,
        messages_per_second DOUBLE PRECISION,
        burst_count INTEGER,
        updated_ts BIGINT NOT NULL DEFAULT (EXTRACT(EPOCH FROM clock_timestamp()) * 1000)::BIGINT,
        CONSTRAINT fk_rate_limits_user FOREIGN KEY (user_id) REFERENCES users(user_id) ON DELETE CASCADE
    );

    CREATE TABLE IF NOT EXISTS user_notification_settings (
        user_id TEXT PRIMARY KEY,
        enabled BOOLEAN NOT NULL DEFAULT TRUE,
        updated_ts BIGINT NOT NULL DEFAULT (EXTRACT(EPOCH FROM clock_timestamp()) * 1000)::BIGINT,
        CONSTRAINT fk_user_notification_settings_user FOREIGN KEY (user_id) REFERENCES users(user_id) ON DELETE CASCADE
    );

    CREATE TABLE IF NOT EXISTS server_notices (
        id BIGSERIAL PRIMARY KEY,
        user_id TEXT,
        event_id TEXT,
        content TEXT,
        sent_ts BIGINT NOT NULL DEFAULT (EXTRACT(EPOCH FROM clock_timestamp()) * 1000)::BIGINT,
        CONSTRAINT fk_server_notices_user FOREIGN KEY (user_id) REFERENCES users(user_id) ON DELETE SET NULL
    );

    CREATE TABLE IF NOT EXISTS qr_login_transactions (
        transaction_id TEXT PRIMARY KEY,
        user_id TEXT NOT NULL,
        device_id TEXT,
        status TEXT NOT NULL,
        created_ts BIGINT NOT NULL,
        updated_ts BIGINT,
        expires_at BIGINT NOT NULL,
        CONSTRAINT fk_qr_login_transactions_user FOREIGN KEY (user_id) REFERENCES users(user_id) ON DELETE CASCADE
    );

    CREATE TABLE IF NOT EXISTS reaction_aggregations (
        event_id TEXT PRIMARY KEY,
        relates_to_event_id TEXT NOT NULL,
        sender TEXT NOT NULL,
        room_id TEXT NOT NULL,
        reaction_key TEXT NOT NULL,
        count BIGINT NOT NULL DEFAULT 1,
        origin_server_ts BIGINT NOT NULL DEFAULT (EXTRACT(EPOCH FROM clock_timestamp()) * 1000)::BIGINT,
        CONSTRAINT fk_reaction_aggregations_sender FOREIGN KEY (sender) REFERENCES users(user_id) ON DELETE CASCADE,
        CONSTRAINT fk_reaction_aggregations_room FOREIGN KEY (room_id) REFERENCES rooms(room_id) ON DELETE CASCADE
    );

    CREATE TABLE IF NOT EXISTS registration_token_batches (
        id BIGSERIAL PRIMARY KEY,
        batch_id TEXT NOT NULL UNIQUE,
        description TEXT,
        token_count INTEGER NOT NULL,
        tokens_used INTEGER NOT NULL DEFAULT 0,
        created_by TEXT,
        created_ts BIGINT NOT NULL,
        expires_at BIGINT,
        is_enabled BOOLEAN NOT NULL DEFAULT TRUE,
        allowed_email_domains TEXT[],
        auto_join_rooms TEXT[]
    );
END $$;

CREATE INDEX IF NOT EXISTS idx_push_device_user_enabled
ON push_device(user_id)
WHERE is_enabled = TRUE;

CREATE INDEX IF NOT EXISTS idx_rate_limits_updated
ON rate_limits(updated_ts DESC);

CREATE INDEX IF NOT EXISTS idx_server_notices_sent
ON server_notices(sent_ts DESC);

CREATE INDEX IF NOT EXISTS idx_user_notification_settings_updated
ON user_notification_settings(updated_ts DESC);

CREATE INDEX IF NOT EXISTS idx_qr_login_transactions_expires
ON qr_login_transactions(expires_at ASC);

CREATE INDEX IF NOT EXISTS idx_qr_login_transactions_user_created
ON qr_login_transactions(user_id, created_ts DESC);

CREATE INDEX IF NOT EXISTS idx_reaction_aggregations_room_relates_origin
ON reaction_aggregations(room_id, relates_to_event_id, origin_server_ts DESC);

CREATE INDEX IF NOT EXISTS idx_registration_token_batches_created
ON registration_token_batches(created_ts DESC);

CREATE INDEX IF NOT EXISTS idx_registration_token_batches_enabled_created
ON registration_token_batches(created_ts DESC)
WHERE is_enabled = TRUE;


-- ============================================================================
-- Part 7: 20260330000007_align_uploads_and_user_settings_exceptions
-- Original file: 20260330000007_align_uploads_and_user_settings_exceptions.sql
-- ============================================================================

DO $$
BEGIN
    CREATE TABLE IF NOT EXISTS upload_progress (
        upload_id TEXT PRIMARY KEY,
        user_id TEXT NOT NULL,
        filename TEXT,
        content_type TEXT,
        total_size BIGINT,
        uploaded_size BIGINT NOT NULL DEFAULT 0,
        total_chunks INTEGER NOT NULL,
        uploaded_chunks INTEGER NOT NULL DEFAULT 0,
        status TEXT NOT NULL DEFAULT 'pending',
        created_ts BIGINT NOT NULL,
        updated_ts BIGINT,
        expires_at BIGINT NOT NULL,
        CONSTRAINT fk_upload_progress_user FOREIGN KEY (user_id) REFERENCES users(user_id) ON DELETE CASCADE
    );

    CREATE TABLE IF NOT EXISTS upload_chunks (
        upload_id TEXT NOT NULL,
        chunk_index INTEGER NOT NULL,
        chunk_data BYTEA NOT NULL,
        chunk_size BIGINT NOT NULL,
        created_ts BIGINT NOT NULL,
        CONSTRAINT pk_upload_chunks PRIMARY KEY (upload_id, chunk_index),
        CONSTRAINT fk_upload_chunks_upload FOREIGN KEY (upload_id) REFERENCES upload_progress(upload_id) ON DELETE CASCADE
    );

    CREATE TABLE IF NOT EXISTS user_settings (
        user_id TEXT PRIMARY KEY,
        theme TEXT,
        language TEXT,
        time_zone TEXT,
        created_ts BIGINT NOT NULL DEFAULT (EXTRACT(EPOCH FROM clock_timestamp()) * 1000)::BIGINT,
        updated_ts BIGINT NOT NULL DEFAULT (EXTRACT(EPOCH FROM clock_timestamp()) * 1000)::BIGINT,
        CONSTRAINT fk_user_settings_user FOREIGN KEY (user_id) REFERENCES users(user_id) ON DELETE CASCADE
    );
END $$;

CREATE INDEX IF NOT EXISTS idx_upload_progress_expires
ON upload_progress(expires_at ASC);

CREATE INDEX IF NOT EXISTS idx_upload_progress_user_created_active
ON upload_progress(user_id, created_ts DESC)
WHERE status <> 'finalized';

CREATE INDEX IF NOT EXISTS idx_upload_chunks_upload_order
ON upload_chunks(upload_id, chunk_index ASC);


-- ============================================================================
-- Part 8: 20260330000008_align_background_update_exceptions
-- Original file: 20260330000008_align_background_update_exceptions.sql
-- ============================================================================

DO $$
BEGIN
    CREATE TABLE IF NOT EXISTS background_update_locks (
        lock_name TEXT PRIMARY KEY,
        owner TEXT,
        acquired_ts BIGINT NOT NULL,
        expires_at BIGINT NOT NULL
    );

    CREATE TABLE IF NOT EXISTS background_update_history (
        id BIGSERIAL PRIMARY KEY,
        job_name TEXT NOT NULL,
        execution_start_ts BIGINT NOT NULL,
        execution_end_ts BIGINT,
        status TEXT NOT NULL,
        items_processed INTEGER NOT NULL DEFAULT 0,
        error_message TEXT,
        metadata JSONB
    );

    CREATE TABLE IF NOT EXISTS background_update_stats (
        id BIGSERIAL PRIMARY KEY,
        job_name TEXT NOT NULL,
        total_updates INTEGER NOT NULL DEFAULT 0,
        completed_updates INTEGER NOT NULL DEFAULT 0,
        failed_updates INTEGER NOT NULL DEFAULT 0,
        last_run_ts BIGINT,
        next_run_ts BIGINT,
        average_duration_ms BIGINT NOT NULL DEFAULT 0,
        created_ts BIGINT NOT NULL DEFAULT (EXTRACT(EPOCH FROM clock_timestamp()) * 1000)::BIGINT,
        updated_ts BIGINT NOT NULL DEFAULT (EXTRACT(EPOCH FROM clock_timestamp()) * 1000)::BIGINT
    );
END $$;

CREATE INDEX IF NOT EXISTS idx_background_update_locks_expires
ON background_update_locks(expires_at);

CREATE INDEX IF NOT EXISTS idx_background_update_history_job_start
ON background_update_history(job_name, execution_start_ts DESC);

CREATE INDEX IF NOT EXISTS idx_background_update_stats_created
ON background_update_stats(created_ts DESC);


-- ============================================================================
-- Part 9: 20260330000009_align_beacon_and_call_exceptions
-- Original file: 20260330000009_align_beacon_and_call_exceptions.sql
-- ============================================================================

-- 1. beacon_info
CREATE TABLE IF NOT EXISTS beacon_info (
    id BIGSERIAL PRIMARY KEY,
    room_id TEXT NOT NULL,
    event_id TEXT NOT NULL UNIQUE,
    state_key TEXT NOT NULL,
    sender TEXT NOT NULL,
    description TEXT,
    timeout BIGINT NOT NULL,
    is_live BOOLEAN NOT NULL DEFAULT TRUE,
    asset_type TEXT NOT NULL,
    created_ts BIGINT NOT NULL,
    updated_ts BIGINT NOT NULL,
    expires_at BIGINT
);

CREATE INDEX IF NOT EXISTS idx_beacon_info_room_active ON beacon_info(room_id, is_live) WHERE is_live = TRUE;
CREATE INDEX IF NOT EXISTS idx_beacon_info_room_state ON beacon_info(room_id, state_key, created_ts DESC);
CREATE INDEX IF NOT EXISTS idx_beacon_info_expires ON beacon_info(expires_at) WHERE expires_at IS NOT NULL;

-- 2. beacon_locations
CREATE TABLE IF NOT EXISTS beacon_locations (
    id BIGSERIAL PRIMARY KEY,
    room_id TEXT NOT NULL,
    event_id TEXT NOT NULL,
    beacon_info_id TEXT NOT NULL,
    sender TEXT NOT NULL,
    uri TEXT NOT NULL,
    description TEXT,
    timestamp BIGINT NOT NULL,
    accuracy BIGINT,
    created_ts BIGINT NOT NULL
);

CREATE INDEX IF NOT EXISTS idx_beacon_locations_info_ts ON beacon_locations(beacon_info_id, timestamp DESC);

-- 3. call_sessions
CREATE TABLE IF NOT EXISTS call_sessions (
    id BIGSERIAL PRIMARY KEY,
    call_id TEXT NOT NULL,
    room_id TEXT NOT NULL,
    caller_id TEXT NOT NULL,
    callee_id TEXT,
    state TEXT NOT NULL,
    offer_sdp TEXT,
    answer_sdp TEXT,
    lifetime BIGINT NOT NULL,
    created_ts BIGINT NOT NULL,
    updated_ts BIGINT,
    ended_ts BIGINT
);

CREATE UNIQUE INDEX IF NOT EXISTS idx_call_sessions_call_room ON call_sessions(call_id, room_id);
CREATE INDEX IF NOT EXISTS idx_call_sessions_active ON call_sessions(state) WHERE state != 'ended';

-- 4. call_candidates
CREATE TABLE IF NOT EXISTS call_candidates (
    id BIGSERIAL PRIMARY KEY,
    call_id TEXT NOT NULL,
    room_id TEXT NOT NULL,
    sender_id TEXT NOT NULL,
    candidate JSONB NOT NULL,
    created_ts BIGINT NOT NULL
);

CREATE INDEX IF NOT EXISTS idx_call_candidates_session ON call_candidates(call_id, room_id, created_ts ASC);

-- 5. matrixrtc_sessions
CREATE TABLE IF NOT EXISTS matrixrtc_sessions (
    id BIGSERIAL PRIMARY KEY,
    room_id TEXT NOT NULL,
    session_id TEXT NOT NULL,
    application TEXT NOT NULL,
    call_id TEXT,
    creator TEXT NOT NULL,
    created_ts BIGINT NOT NULL,
    updated_ts BIGINT NOT NULL,
    is_active BOOLEAN NOT NULL DEFAULT TRUE,
    config JSONB NOT NULL
);

CREATE UNIQUE INDEX IF NOT EXISTS idx_matrixrtc_sessions_unique ON matrixrtc_sessions(room_id, session_id);
CREATE INDEX IF NOT EXISTS idx_matrixrtc_sessions_active ON matrixrtc_sessions(room_id, is_active, created_ts DESC) WHERE is_active = TRUE;

-- 6. matrixrtc_memberships
CREATE TABLE IF NOT EXISTS matrixrtc_memberships (
    id BIGSERIAL PRIMARY KEY,
    room_id TEXT NOT NULL,
    session_id TEXT NOT NULL,
    user_id TEXT NOT NULL,
    device_id TEXT NOT NULL,
    membership_id TEXT NOT NULL,
    application TEXT NOT NULL,
    call_id TEXT,
    created_ts BIGINT NOT NULL,
    updated_ts BIGINT NOT NULL,
    expires_at BIGINT,
    foci_active TEXT,
    foci_preferred JSONB,
    application_data JSONB,
    is_active BOOLEAN NOT NULL DEFAULT TRUE
);

CREATE UNIQUE INDEX IF NOT EXISTS idx_matrixrtc_memberships_unique ON matrixrtc_memberships(room_id, session_id, user_id, device_id);
CREATE INDEX IF NOT EXISTS idx_matrixrtc_memberships_active ON matrixrtc_memberships(room_id, is_active) WHERE is_active = TRUE;

-- 7. matrixrtc_encryption_keys
CREATE TABLE IF NOT EXISTS matrixrtc_encryption_keys (
    id BIGSERIAL PRIMARY KEY,
    room_id TEXT NOT NULL,
    session_id TEXT NOT NULL,
    key_index INTEGER NOT NULL,
    key TEXT NOT NULL,
    created_ts BIGINT NOT NULL,
    expires_at BIGINT,
    sender_user_id TEXT NOT NULL,
    sender_device_id TEXT NOT NULL
);

CREATE UNIQUE INDEX IF NOT EXISTS idx_matrixrtc_encryption_keys_unique ON matrixrtc_encryption_keys(room_id, session_id, key_index);


-- ============================================================================
-- Part 10: 20260330000013_align_legacy_timestamp_columns
-- Original file: 20260330000013_align_legacy_timestamp_columns.sql
-- ============================================================================

DO $$
BEGIN
    IF EXISTS (
        SELECT 1 FROM information_schema.columns
        WHERE table_schema = 'public' AND table_name = 'device_trust_status' AND column_name = 'created_at'
    ) AND NOT EXISTS (
        SELECT 1 FROM information_schema.columns
        WHERE table_schema = 'public' AND table_name = 'device_trust_status' AND column_name = 'created_ts'
    ) THEN
        ALTER TABLE device_trust_status RENAME COLUMN created_at TO created_ts;
    END IF;

    IF EXISTS (
        SELECT 1 FROM information_schema.columns
        WHERE table_schema = 'public' AND table_name = 'device_trust_status' AND column_name = 'updated_at'
    ) AND NOT EXISTS (
        SELECT 1 FROM information_schema.columns
        WHERE table_schema = 'public' AND table_name = 'device_trust_status' AND column_name = 'updated_ts'
    ) THEN
        ALTER TABLE device_trust_status RENAME COLUMN updated_at TO updated_ts;
    END IF;

    IF EXISTS (
        SELECT 1 FROM information_schema.columns
        WHERE table_schema = 'public' AND table_name = 'cross_signing_trust' AND column_name = 'created_at'
    ) AND NOT EXISTS (
        SELECT 1 FROM information_schema.columns
        WHERE table_schema = 'public' AND table_name = 'cross_signing_trust' AND column_name = 'created_ts'
    ) THEN
        ALTER TABLE cross_signing_trust RENAME COLUMN created_at TO created_ts;
    END IF;

    IF EXISTS (
        SELECT 1 FROM information_schema.columns
        WHERE table_schema = 'public' AND table_name = 'cross_signing_trust' AND column_name = 'updated_at'
    ) AND NOT EXISTS (
        SELECT 1 FROM information_schema.columns
        WHERE table_schema = 'public' AND table_name = 'cross_signing_trust' AND column_name = 'updated_ts'
    ) THEN
        ALTER TABLE cross_signing_trust RENAME COLUMN updated_at TO updated_ts;
    END IF;

    IF EXISTS (
        SELECT 1 FROM information_schema.columns
        WHERE table_schema = 'public' AND table_name = 'verification_requests' AND column_name = 'created_at'
    ) AND NOT EXISTS (
        SELECT 1 FROM information_schema.columns
        WHERE table_schema = 'public' AND table_name = 'verification_requests' AND column_name = 'created_ts'
    ) THEN
        ALTER TABLE verification_requests RENAME COLUMN created_at TO created_ts;
    END IF;

    IF EXISTS (
        SELECT 1 FROM information_schema.columns
        WHERE table_schema = 'public' AND table_name = 'verification_requests' AND column_name = 'updated_at'
    ) AND NOT EXISTS (
        SELECT 1 FROM information_schema.columns
        WHERE table_schema = 'public' AND table_name = 'verification_requests' AND column_name = 'updated_ts'
    ) THEN
        ALTER TABLE verification_requests RENAME COLUMN updated_at TO updated_ts;
    END IF;

    IF EXISTS (
        SELECT 1 FROM information_schema.columns
        WHERE table_schema = 'public' AND table_name = 'device_verification_request' AND column_name = 'created_at'
    ) AND NOT EXISTS (
        SELECT 1 FROM information_schema.columns
        WHERE table_schema = 'public' AND table_name = 'device_verification_request' AND column_name = 'created_ts'
    ) THEN
        ALTER TABLE device_verification_request RENAME COLUMN created_at TO created_ts;
    END IF;

    IF EXISTS (
        SELECT 1 FROM information_schema.columns
        WHERE table_schema = 'public' AND table_name = 'e2ee_security_events' AND column_name = 'created_at'
    ) AND NOT EXISTS (
        SELECT 1 FROM information_schema.columns
        WHERE table_schema = 'public' AND table_name = 'e2ee_security_events' AND column_name = 'created_ts'
    ) THEN
        ALTER TABLE e2ee_security_events RENAME COLUMN created_at TO created_ts;
    END IF;

    IF EXISTS (
        SELECT 1 FROM information_schema.columns
        WHERE table_schema = 'public' AND table_name = 'secure_key_backups' AND column_name = 'created_at'
    ) AND NOT EXISTS (
        SELECT 1 FROM information_schema.columns
        WHERE table_schema = 'public' AND table_name = 'secure_key_backups' AND column_name = 'created_ts'
    ) THEN
        ALTER TABLE secure_key_backups RENAME COLUMN created_at TO created_ts;
    END IF;

    IF EXISTS (
        SELECT 1 FROM information_schema.columns
        WHERE table_schema = 'public' AND table_name = 'secure_key_backups' AND column_name = 'updated_at'
    ) AND NOT EXISTS (
        SELECT 1 FROM information_schema.columns
        WHERE table_schema = 'public' AND table_name = 'secure_key_backups' AND column_name = 'updated_ts'
    ) THEN
        ALTER TABLE secure_key_backups RENAME COLUMN updated_at TO updated_ts;
    END IF;

    IF EXISTS (
        SELECT 1 FROM information_schema.columns
        WHERE table_schema = 'public' AND table_name = 'secure_backup_session_keys' AND column_name = 'created_at'
    ) AND NOT EXISTS (
        SELECT 1 FROM information_schema.columns
        WHERE table_schema = 'public' AND table_name = 'secure_backup_session_keys' AND column_name = 'created_ts'
    ) THEN
        ALTER TABLE secure_backup_session_keys RENAME COLUMN created_at TO created_ts;
    END IF;
END $$;

DO $$
BEGIN
    IF EXISTS (
        SELECT 1 FROM information_schema.columns
        WHERE table_schema = 'public' AND table_name = 'device_trust_status'
          AND column_name = 'created_ts' AND data_type <> 'bigint'
    ) THEN
        ALTER TABLE device_trust_status
        ALTER COLUMN created_ts TYPE BIGINT
        USING (EXTRACT(EPOCH FROM created_ts) * 1000)::BIGINT;
    END IF;

    IF EXISTS (
        SELECT 1 FROM information_schema.columns
        WHERE table_schema = 'public' AND table_name = 'device_trust_status'
          AND column_name = 'updated_ts' AND data_type <> 'bigint'
    ) THEN
        ALTER TABLE device_trust_status
        ALTER COLUMN updated_ts TYPE BIGINT
        USING (EXTRACT(EPOCH FROM updated_ts) * 1000)::BIGINT;
    END IF;

    IF EXISTS (
        SELECT 1 FROM information_schema.columns
        WHERE table_schema = 'public' AND table_name = 'cross_signing_trust'
          AND column_name = 'created_ts' AND data_type <> 'bigint'
    ) THEN
        ALTER TABLE cross_signing_trust
        ALTER COLUMN created_ts TYPE BIGINT
        USING (EXTRACT(EPOCH FROM created_ts) * 1000)::BIGINT;
    END IF;

    IF EXISTS (
        SELECT 1 FROM information_schema.columns
        WHERE table_schema = 'public' AND table_name = 'cross_signing_trust'
          AND column_name = 'updated_ts' AND data_type <> 'bigint'
    ) THEN
        ALTER TABLE cross_signing_trust
        ALTER COLUMN updated_ts TYPE BIGINT
        USING (EXTRACT(EPOCH FROM updated_ts) * 1000)::BIGINT;
    END IF;

    IF EXISTS (
        SELECT 1 FROM information_schema.columns
        WHERE table_schema = 'public' AND table_name = 'device_verification_request'
          AND column_name = 'created_ts' AND data_type <> 'bigint'
    ) THEN
        ALTER TABLE device_verification_request
        ALTER COLUMN created_ts TYPE BIGINT
        USING (EXTRACT(EPOCH FROM created_ts) * 1000)::BIGINT;
    END IF;

    IF EXISTS (
        SELECT 1 FROM information_schema.columns
        WHERE table_schema = 'public' AND table_name = 'e2ee_security_events'
          AND column_name = 'created_ts' AND data_type <> 'bigint'
    ) THEN
        ALTER TABLE e2ee_security_events
        ALTER COLUMN created_ts TYPE BIGINT
        USING (EXTRACT(EPOCH FROM created_ts) * 1000)::BIGINT;
    END IF;

    IF EXISTS (
        SELECT 1 FROM information_schema.columns
        WHERE table_schema = 'public' AND table_name = 'secure_key_backups'
          AND column_name = 'created_ts' AND data_type <> 'bigint'
    ) THEN
        ALTER TABLE secure_key_backups
        ALTER COLUMN created_ts DROP DEFAULT;
        ALTER TABLE secure_key_backups
        ALTER COLUMN created_ts TYPE BIGINT
        USING (EXTRACT(EPOCH FROM created_ts) * 1000)::BIGINT;
    END IF;

    IF EXISTS (
        SELECT 1 FROM information_schema.columns
        WHERE table_schema = 'public' AND table_name = 'secure_key_backups'
          AND column_name = 'updated_ts' AND data_type <> 'bigint'
    ) THEN
        ALTER TABLE secure_key_backups
        ALTER COLUMN updated_ts DROP DEFAULT;
        ALTER TABLE secure_key_backups
        ALTER COLUMN updated_ts TYPE BIGINT
        USING (EXTRACT(EPOCH FROM updated_ts) * 1000)::BIGINT;
    END IF;

    IF EXISTS (
        SELECT 1 FROM information_schema.columns
        WHERE table_schema = 'public' AND table_name = 'secure_backup_session_keys'
          AND column_name = 'created_ts' AND data_type <> 'bigint'
    ) THEN
        ALTER TABLE secure_backup_session_keys
        ALTER COLUMN created_ts DROP DEFAULT;
        ALTER TABLE secure_backup_session_keys
        ALTER COLUMN created_ts TYPE BIGINT
        USING (EXTRACT(EPOCH FROM created_ts) * 1000)::BIGINT;
    END IF;
END $$;

ALTER TABLE IF EXISTS device_trust_status ALTER COLUMN created_ts SET NOT NULL;
ALTER TABLE IF EXISTS device_trust_status ALTER COLUMN updated_ts DROP NOT NULL;
ALTER TABLE IF EXISTS cross_signing_trust ALTER COLUMN created_ts SET NOT NULL;
ALTER TABLE IF EXISTS cross_signing_trust ALTER COLUMN updated_ts DROP NOT NULL;
ALTER TABLE IF EXISTS verification_requests ALTER COLUMN created_ts SET NOT NULL;
ALTER TABLE IF EXISTS verification_requests ALTER COLUMN updated_ts DROP NOT NULL;
ALTER TABLE IF EXISTS device_verification_request ALTER COLUMN created_ts SET NOT NULL;
ALTER TABLE IF EXISTS e2ee_security_events ALTER COLUMN created_ts SET NOT NULL;
ALTER TABLE IF EXISTS secure_key_backups ALTER COLUMN created_ts SET NOT NULL;
ALTER TABLE IF EXISTS secure_key_backups ALTER COLUMN created_ts SET DEFAULT (EXTRACT(EPOCH FROM clock_timestamp()) * 1000)::BIGINT;
ALTER TABLE IF EXISTS secure_key_backups ALTER COLUMN updated_ts DROP NOT NULL;
ALTER TABLE IF EXISTS secure_key_backups ALTER COLUMN updated_ts SET DEFAULT (EXTRACT(EPOCH FROM clock_timestamp()) * 1000)::BIGINT;
ALTER TABLE IF EXISTS secure_backup_session_keys ALTER COLUMN created_ts SET NOT NULL;
ALTER TABLE IF EXISTS secure_backup_session_keys ALTER COLUMN created_ts SET DEFAULT (EXTRACT(EPOCH FROM clock_timestamp()) * 1000)::BIGINT;

DROP INDEX IF EXISTS idx_verification_requests_to_user_state;

DROP INDEX IF EXISTS idx_e2ee_security_events_user_created;

DROP INDEX IF EXISTS idx_secure_key_backups_user;
CREATE INDEX IF NOT EXISTS idx_secure_key_backups_user
ON secure_key_backups(user_id, created_ts DESC);


-- ============================================================================
-- Migration Record
-- ============================================================================


-- ===== Merged from: 20260404000002_consolidated_minor_features.sql =====

--no-transaction
-- ============================================================================
-- Consolidated Minor Features Migration
-- Created: 2026-04-04
-- Description: Merges 3 small feature migrations into a single file
-- Original migrations: 20260328000002, 20260330000010, 20260330000011
-- ============================================================================

SET TIME ZONE 'UTC';

-- ============================================================================
-- Part 1: Federation Cache (原 20260328000002)
-- ============================================================================

CREATE TABLE IF NOT EXISTS federation_cache (
    id BIGSERIAL PRIMARY KEY,
    key TEXT NOT NULL UNIQUE,
    value TEXT,
    expiry_ts BIGINT,
    created_ts BIGINT NOT NULL DEFAULT 0
);

CREATE INDEX IF NOT EXISTS idx_federation_cache_key ON federation_cache(key);
CREATE INDEX IF NOT EXISTS idx_federation_cache_expiry ON federation_cache(expiry_ts);

-- ============================================================================
-- Part 2: Audit Events (原 20260330000010)
-- ============================================================================

-- Note: audit_events table already defined in unified baseline schema
-- This section intentionally empty as duplicate table definition was removed

-- ============================================================================
-- Part 3: Feature Flags (原 20260330000011)
-- ============================================================================

CREATE TABLE IF NOT EXISTS feature_flags (
    flag_key TEXT PRIMARY KEY,
    target_scope TEXT NOT NULL,
    rollout_percent INTEGER NOT NULL DEFAULT 0,
    expires_at BIGINT,
    reason TEXT NOT NULL,
    status TEXT NOT NULL DEFAULT 'draft',
    created_by TEXT NOT NULL,
    created_ts BIGINT NOT NULL,
    updated_ts BIGINT NOT NULL
);

CREATE TABLE IF NOT EXISTS feature_flag_targets (
    id BIGSERIAL PRIMARY KEY,
    flag_key TEXT NOT NULL,
    subject_type TEXT NOT NULL,
    subject_id TEXT NOT NULL,
    created_ts BIGINT NOT NULL,
    CONSTRAINT fk_feature_flag_targets_flag_key
        FOREIGN KEY (flag_key) REFERENCES feature_flags(flag_key) ON DELETE CASCADE,
    CONSTRAINT uq_feature_flag_targets UNIQUE (flag_key, subject_type, subject_id)
);

CREATE INDEX IF NOT EXISTS idx_feature_flags_scope_status
ON feature_flags(target_scope, status, updated_ts DESC);

CREATE INDEX IF NOT EXISTS idx_feature_flags_expires_at
ON feature_flags(expires_at)
WHERE expires_at IS NOT NULL;

CREATE INDEX IF NOT EXISTS idx_feature_flag_targets_lookup
ON feature_flag_targets(flag_key, subject_type, subject_id);

-- ============================================================================
-- Migration Record
-- ============================================================================


-- ===== Merged from: 20260406000001_consolidated_schema_fixes.sql =====

-- ============================================================================
-- Consolidated Migration: Schema Fixes & Contract Alignment
-- Created: 2026-04-22 (consolidated from 8 migrations dated 2026-04-05 ~ 2026-04-06)
--
-- Merged source files:
--   1. 20260405000001_fix_push_rules_unique_constraint.sql
--   2. 20260405000002_fix_push_rules_unique_constraint_v2.sql
--   3. 20260406000001_restore_verification_requests_pending_index.sql
--   4. 20260406000002_restore_schema_contract_foreign_keys.sql
--   5. 20260406000003_restore_public_schema_contract_foreign_keys.sql
--   6. 20260406000004_cleanup_schema_contract_room_orphans.sql
--   7. 20260406000005_align_presence_schema_contract.sql
--   8. 20260406000006_align_media_quota_schema_contract.sql
--
-- All statements use IF NOT EXISTS / IF EXISTS guards for idempotent execution.
-- ============================================================================


-- ===== Merged from: 20260405000001_fix_push_rules_unique_constraint.sql =====

ALTER TABLE push_rules
    DROP CONSTRAINT IF EXISTS uq_push_rules_user_scope_rule;

ALTER TABLE push_rules
    DROP CONSTRAINT IF EXISTS uq_push_rules_user_scope_kind_rule;

ALTER TABLE push_rules
    ADD CONSTRAINT uq_push_rules_user_scope_kind_rule UNIQUE (user_id, scope, kind, rule_id);

-- ===== Merged from: 20260405000002_fix_push_rules_unique_constraint_v2.sql =====

ALTER TABLE push_rules
    DROP CONSTRAINT IF EXISTS uq_push_rules_user_scope_rule;

ALTER TABLE push_rules
    DROP CONSTRAINT IF EXISTS uq_push_rules_user_scope_kind_rule;

ALTER TABLE push_rules
    ADD CONSTRAINT uq_push_rules_user_scope_kind_rule UNIQUE (user_id, scope, kind, rule_id);


-- ===== Merged from: 20260406000001_restore_verification_requests_pending_index.sql =====

-- ============================================================================
-- Restore verification_requests pending lookup index
-- Created: 2026-04-06
-- Description: Re-create a critical verification_requests index that was
-- accidentally dropped during schema alignment consolidation.
-- ============================================================================

SET TIME ZONE 'UTC';

CREATE INDEX IF NOT EXISTS idx_verification_requests_to_user_state
ON verification_requests(to_user, state, updated_ts DESC);


-- ===== Merged from: 20260406000002_restore_schema_contract_foreign_keys.sql =====

-- ============================================================================
-- Restore schema-contract foreign keys
-- Created: 2026-04-06
-- Description: Re-create foreign keys required by schema validator and
-- database integrity tests for room summary and retention tables.
-- ============================================================================

SET TIME ZONE 'UTC';

DO $$
BEGIN
    IF NOT EXISTS (
        SELECT 1 FROM pg_constraint WHERE conname = 'fk_room_summary_state_room'
    ) THEN
        ALTER TABLE room_summary_state
        ADD CONSTRAINT fk_room_summary_state_room
        FOREIGN KEY (room_id) REFERENCES rooms(room_id) ON DELETE CASCADE;
    END IF;

    IF NOT EXISTS (
        SELECT 1 FROM pg_constraint WHERE conname = 'fk_room_summary_stats_room'
    ) THEN
        ALTER TABLE room_summary_stats
        ADD CONSTRAINT fk_room_summary_stats_room
        FOREIGN KEY (room_id) REFERENCES rooms(room_id) ON DELETE CASCADE;
    END IF;

    IF NOT EXISTS (
        SELECT 1 FROM pg_constraint WHERE conname = 'fk_room_summary_update_queue_room'
    ) THEN
        ALTER TABLE room_summary_update_queue
        ADD CONSTRAINT fk_room_summary_update_queue_room
        FOREIGN KEY (room_id) REFERENCES rooms(room_id) ON DELETE CASCADE;
    END IF;

    IF NOT EXISTS (
        SELECT 1 FROM pg_constraint WHERE conname = 'fk_room_children_parent'
    ) THEN
        ALTER TABLE room_children
        ADD CONSTRAINT fk_room_children_parent
        FOREIGN KEY (parent_room_id) REFERENCES rooms(room_id) ON DELETE CASCADE;
    END IF;

    IF NOT EXISTS (
        SELECT 1 FROM pg_constraint WHERE conname = 'fk_room_children_child'
    ) THEN
        ALTER TABLE room_children
        ADD CONSTRAINT fk_room_children_child
        FOREIGN KEY (child_room_id) REFERENCES rooms(room_id) ON DELETE CASCADE;
    END IF;

    IF NOT EXISTS (
        SELECT 1 FROM pg_constraint WHERE conname = 'fk_retention_cleanup_queue_room'
    ) THEN
        ALTER TABLE retention_cleanup_queue
        ADD CONSTRAINT fk_retention_cleanup_queue_room
        FOREIGN KEY (room_id) REFERENCES rooms(room_id) ON DELETE CASCADE;
    END IF;

    IF NOT EXISTS (
        SELECT 1 FROM pg_constraint WHERE conname = 'fk_retention_cleanup_logs_room'
    ) THEN
        ALTER TABLE retention_cleanup_logs
        ADD CONSTRAINT fk_retention_cleanup_logs_room
        FOREIGN KEY (room_id) REFERENCES rooms(room_id) ON DELETE CASCADE;
    END IF;

    IF NOT EXISTS (
        SELECT 1 FROM pg_constraint WHERE conname = 'fk_retention_stats_room'
    ) THEN
        ALTER TABLE retention_stats
        ADD CONSTRAINT fk_retention_stats_room
        FOREIGN KEY (room_id) REFERENCES rooms(room_id) ON DELETE CASCADE;
    END IF;

    IF NOT EXISTS (
        SELECT 1 FROM pg_constraint WHERE conname = 'fk_deleted_events_index_room'
    ) THEN
        ALTER TABLE deleted_events_index
        ADD CONSTRAINT fk_deleted_events_index_room
        FOREIGN KEY (room_id) REFERENCES rooms(room_id) ON DELETE CASCADE;
    END IF;
END $$;


-- ===== Merged from: 20260406000003_restore_public_schema_contract_foreign_keys.sql =====

-- ============================================================================
-- Restore public schema-contract foreign keys
-- Created: 2026-04-06
-- Description: Re-create room summary and retention foreign keys in the public
-- schema. Constraint existence checks are schema-scoped to avoid false
-- positives from isolated test schemas.
-- ============================================================================

SET TIME ZONE 'UTC';

DO $$
BEGIN
    IF NOT EXISTS (
        SELECT 1
        FROM information_schema.table_constraints
        WHERE table_schema = 'public'
          AND table_name = 'room_summary_state'
          AND constraint_name = 'fk_room_summary_state_room'
    ) THEN
        ALTER TABLE room_summary_state
        ADD CONSTRAINT fk_room_summary_state_room
        FOREIGN KEY (room_id) REFERENCES rooms(room_id) ON DELETE CASCADE;
    END IF;

    IF NOT EXISTS (
        SELECT 1
        FROM information_schema.table_constraints
        WHERE table_schema = 'public'
          AND table_name = 'room_summary_stats'
          AND constraint_name = 'fk_room_summary_stats_room'
    ) THEN
        ALTER TABLE room_summary_stats
        ADD CONSTRAINT fk_room_summary_stats_room
        FOREIGN KEY (room_id) REFERENCES rooms(room_id) ON DELETE CASCADE;
    END IF;

    IF NOT EXISTS (
        SELECT 1
        FROM information_schema.table_constraints
        WHERE table_schema = 'public'
          AND table_name = 'room_summary_update_queue'
          AND constraint_name = 'fk_room_summary_update_queue_room'
    ) THEN
        ALTER TABLE room_summary_update_queue
        ADD CONSTRAINT fk_room_summary_update_queue_room
        FOREIGN KEY (room_id) REFERENCES rooms(room_id) ON DELETE CASCADE;
    END IF;

    IF NOT EXISTS (
        SELECT 1
        FROM information_schema.table_constraints
        WHERE table_schema = 'public'
          AND table_name = 'room_children'
          AND constraint_name = 'fk_room_children_parent'
    ) THEN
        ALTER TABLE room_children
        ADD CONSTRAINT fk_room_children_parent
        FOREIGN KEY (parent_room_id) REFERENCES rooms(room_id) ON DELETE CASCADE;
    END IF;

    IF NOT EXISTS (
        SELECT 1
        FROM information_schema.table_constraints
        WHERE table_schema = 'public'
          AND table_name = 'room_children'
          AND constraint_name = 'fk_room_children_child'
    ) THEN
        ALTER TABLE room_children
        ADD CONSTRAINT fk_room_children_child
        FOREIGN KEY (child_room_id) REFERENCES rooms(room_id) ON DELETE CASCADE;
    END IF;

    IF NOT EXISTS (
        SELECT 1
        FROM information_schema.table_constraints
        WHERE table_schema = 'public'
          AND table_name = 'retention_cleanup_queue'
          AND constraint_name = 'fk_retention_cleanup_queue_room'
    ) THEN
        ALTER TABLE retention_cleanup_queue
        ADD CONSTRAINT fk_retention_cleanup_queue_room
        FOREIGN KEY (room_id) REFERENCES rooms(room_id) ON DELETE CASCADE;
    END IF;

    IF NOT EXISTS (
        SELECT 1
        FROM information_schema.table_constraints
        WHERE table_schema = 'public'
          AND table_name = 'retention_cleanup_logs'
          AND constraint_name = 'fk_retention_cleanup_logs_room'
    ) THEN
        ALTER TABLE retention_cleanup_logs
        ADD CONSTRAINT fk_retention_cleanup_logs_room
        FOREIGN KEY (room_id) REFERENCES rooms(room_id) ON DELETE CASCADE;
    END IF;

    IF NOT EXISTS (
        SELECT 1
        FROM information_schema.table_constraints
        WHERE table_schema = 'public'
          AND table_name = 'retention_stats'
          AND constraint_name = 'fk_retention_stats_room'
    ) THEN
        ALTER TABLE retention_stats
        ADD CONSTRAINT fk_retention_stats_room
        FOREIGN KEY (room_id) REFERENCES rooms(room_id) ON DELETE CASCADE;
    END IF;

    IF NOT EXISTS (
        SELECT 1
        FROM information_schema.table_constraints
        WHERE table_schema = 'public'
          AND table_name = 'deleted_events_index'
          AND constraint_name = 'fk_deleted_events_index_room'
    ) THEN
        ALTER TABLE deleted_events_index
        ADD CONSTRAINT fk_deleted_events_index_room
        FOREIGN KEY (room_id) REFERENCES rooms(room_id) ON DELETE CASCADE;
    END IF;
END $$;


-- ===== Merged from: 20260406000004_cleanup_schema_contract_room_orphans.sql =====

-- ============================================================================
-- Cleanup schema-contract room orphans
-- Created: 2026-04-06
-- Description: Remove orphan rows from derived room summary and retention
-- tables so room foreign keys can be restored safely.
-- ============================================================================

SET TIME ZONE 'UTC';

DELETE FROM room_summary_state rss
WHERE NOT EXISTS (
    SELECT 1 FROM rooms r WHERE r.room_id = rss.room_id
);

DELETE FROM room_summary_stats rs
WHERE NOT EXISTS (
    SELECT 1 FROM rooms r WHERE r.room_id = rs.room_id
);

DELETE FROM room_summary_update_queue rsuq
WHERE NOT EXISTS (
    SELECT 1 FROM rooms r WHERE r.room_id = rsuq.room_id
);

DELETE FROM room_children rc
WHERE NOT EXISTS (
    SELECT 1 FROM rooms parent WHERE parent.room_id = rc.parent_room_id
)
   OR NOT EXISTS (
    SELECT 1 FROM rooms child WHERE child.room_id = rc.child_room_id
);

DELETE FROM retention_cleanup_queue rcq
WHERE NOT EXISTS (
    SELECT 1 FROM rooms r WHERE r.room_id = rcq.room_id
);

DELETE FROM retention_cleanup_logs rcl
WHERE NOT EXISTS (
    SELECT 1 FROM rooms r WHERE r.room_id = rcl.room_id
);

DELETE FROM retention_stats rs
WHERE NOT EXISTS (
    SELECT 1 FROM rooms r WHERE r.room_id = rs.room_id
);

DELETE FROM deleted_events_index dei
WHERE NOT EXISTS (
    SELECT 1 FROM rooms r WHERE r.room_id = dei.room_id
);


-- ===== Merged from: 20260406000005_align_presence_schema_contract.sql =====

-- ============================================================================
-- Align presence schema contract
-- Created: 2026-04-06
-- Description: Repair legacy presence nullability/default drift so presence
-- schema contract tests match the unified schema baseline.
-- ============================================================================

SET TIME ZONE 'UTC';

UPDATE presence
SET presence = 'offline'
WHERE presence IS NULL;

UPDATE presence
SET last_active_ts = 0
WHERE last_active_ts IS NULL;

ALTER TABLE IF EXISTS presence
    ALTER COLUMN presence SET DEFAULT 'offline';

ALTER TABLE IF EXISTS presence
    ALTER COLUMN last_active_ts SET DEFAULT 0;

ALTER TABLE IF EXISTS presence
    ALTER COLUMN presence SET NOT NULL;

ALTER TABLE IF EXISTS presence
    ALTER COLUMN last_active_ts SET NOT NULL;

CREATE INDEX IF NOT EXISTS idx_presence_user_status
ON presence(user_id, presence);


-- ===== Merged from: 20260406000006_align_media_quota_schema_contract.sql =====

-- ============================================================================
-- Align media quota schema contract
-- Created: 2026-04-06
-- Description: Restore media quota tables/columns required by MediaQuotaStorage
-- and the schema contract migration gate.
-- ============================================================================

SET TIME ZONE 'UTC';

CREATE TABLE IF NOT EXISTS media_usage_log (
    id BIGSERIAL PRIMARY KEY,
    user_id TEXT NOT NULL,
    media_id TEXT NOT NULL,
    file_size_bytes BIGINT NOT NULL,
    mime_type TEXT,
    operation TEXT NOT NULL,
    timestamp BIGINT NOT NULL
);

CREATE INDEX IF NOT EXISTS idx_media_usage_log_user
ON media_usage_log(user_id);

CREATE INDEX IF NOT EXISTS idx_media_usage_log_timestamp
ON media_usage_log(timestamp);

CREATE TABLE IF NOT EXISTS media_quota_alerts (
    id BIGSERIAL PRIMARY KEY,
    user_id TEXT NOT NULL,
    alert_type TEXT NOT NULL,
    threshold_percent INTEGER NOT NULL,
    current_usage_bytes BIGINT NOT NULL,
    quota_limit_bytes BIGINT NOT NULL,
    message TEXT,
    is_read BOOLEAN NOT NULL DEFAULT FALSE,
    created_ts BIGINT NOT NULL DEFAULT (EXTRACT(EPOCH FROM NOW())::BIGINT * 1000)
);

CREATE INDEX IF NOT EXISTS idx_media_quota_alerts_user
ON media_quota_alerts(user_id)
WHERE is_read = FALSE;

CREATE TABLE IF NOT EXISTS server_media_quota (
    id BIGSERIAL PRIMARY KEY,
    max_storage_bytes BIGINT,
    max_file_size_bytes BIGINT,
    max_files_count INTEGER,
    current_storage_bytes BIGINT NOT NULL DEFAULT 0,
    current_files_count INTEGER NOT NULL DEFAULT 0,
    alert_threshold_percent INTEGER NOT NULL DEFAULT 80,
    updated_ts BIGINT NOT NULL
);

ALTER TABLE media_quota_config
    ADD COLUMN IF NOT EXISTS name TEXT,
    ADD COLUMN IF NOT EXISTS description TEXT,
    ADD COLUMN IF NOT EXISTS max_storage_bytes BIGINT,
    ADD COLUMN IF NOT EXISTS max_file_size_bytes BIGINT,
    ADD COLUMN IF NOT EXISTS max_files_count INTEGER,
    ADD COLUMN IF NOT EXISTS allowed_mime_types JSONB,
    ADD COLUMN IF NOT EXISTS blocked_mime_types JSONB,
    ADD COLUMN IF NOT EXISTS is_default BOOLEAN;

UPDATE media_quota_config
SET name = COALESCE(name, NULLIF(config_name, ''), 'default')
WHERE name IS NULL;

UPDATE media_quota_config
SET max_storage_bytes = COALESCE(max_storage_bytes, 10737418240)
WHERE max_storage_bytes IS NULL;

UPDATE media_quota_config
SET max_file_size_bytes = COALESCE(max_file_size_bytes, max_file_size, 10485760)
WHERE max_file_size_bytes IS NULL;

UPDATE media_quota_config
SET max_files_count = COALESCE(max_files_count, 10000)
WHERE max_files_count IS NULL;

UPDATE media_quota_config
SET allowed_mime_types = COALESCE(allowed_mime_types, to_jsonb(allowed_content_types), '[]'::jsonb)
WHERE allowed_mime_types IS NULL;

UPDATE media_quota_config
SET blocked_mime_types = COALESCE(blocked_mime_types, '[]'::jsonb)
WHERE blocked_mime_types IS NULL;

UPDATE media_quota_config
SET is_default = COALESCE(is_default, FALSE)
WHERE is_default IS NULL;

ALTER TABLE media_quota_config
    ALTER COLUMN config_name SET DEFAULT '',
    ALTER COLUMN created_ts SET DEFAULT (EXTRACT(EPOCH FROM NOW())::BIGINT * 1000),
    ALTER COLUMN name SET DEFAULT 'default',
    ALTER COLUMN max_storage_bytes SET DEFAULT 10737418240,
    ALTER COLUMN max_file_size_bytes SET DEFAULT 10485760,
    ALTER COLUMN max_files_count SET DEFAULT 10000,
    ALTER COLUMN allowed_mime_types SET DEFAULT '[]'::jsonb,
    ALTER COLUMN blocked_mime_types SET DEFAULT '[]'::jsonb,
    ALTER COLUMN is_default SET DEFAULT FALSE;

ALTER TABLE media_quota_config
    ALTER COLUMN name SET NOT NULL,
    ALTER COLUMN max_storage_bytes SET NOT NULL,
    ALTER COLUMN max_file_size_bytes SET NOT NULL,
    ALTER COLUMN max_files_count SET NOT NULL,
    ALTER COLUMN allowed_mime_types SET NOT NULL,
    ALTER COLUMN blocked_mime_types SET NOT NULL,
    ALTER COLUMN is_default SET NOT NULL;

ALTER TABLE user_media_quota
    ADD COLUMN IF NOT EXISTS quota_config_id BIGINT,
    ADD COLUMN IF NOT EXISTS custom_max_storage_bytes BIGINT,
    ADD COLUMN IF NOT EXISTS custom_max_file_size_bytes BIGINT,
    ADD COLUMN IF NOT EXISTS custom_max_files_count INTEGER,
    ADD COLUMN IF NOT EXISTS current_storage_bytes BIGINT,
    ADD COLUMN IF NOT EXISTS current_files_count INTEGER;

UPDATE user_media_quota
SET current_storage_bytes = COALESCE(current_storage_bytes, used_bytes, 0)
WHERE current_storage_bytes IS NULL;

UPDATE user_media_quota
SET current_files_count = COALESCE(current_files_count, file_count, 0)
WHERE current_files_count IS NULL;

ALTER TABLE user_media_quota
    ALTER COLUMN current_storage_bytes SET DEFAULT 0,
    ALTER COLUMN current_files_count SET DEFAULT 0;

ALTER TABLE user_media_quota
    ALTER COLUMN current_storage_bytes SET NOT NULL,
    ALTER COLUMN current_files_count SET NOT NULL;

UPDATE media_quota_alerts
SET is_read = FALSE
WHERE is_read IS NULL;

ALTER TABLE media_quota_alerts
    ALTER COLUMN created_ts SET DEFAULT (EXTRACT(EPOCH FROM NOW())::BIGINT * 1000),
    ALTER COLUMN is_read SET DEFAULT FALSE;

ALTER TABLE media_quota_alerts
    ALTER COLUMN is_read SET NOT NULL;

INSERT INTO server_media_quota (
    id,
    max_storage_bytes,
    max_file_size_bytes,
    max_files_count,
    current_storage_bytes,
    current_files_count,
    alert_threshold_percent,
    updated_ts
)
SELECT
    1,
    10995116277760,
    1073741824,
    1000000,
    0,
    0,
    80,
    EXTRACT(EPOCH FROM NOW())::BIGINT * 1000
WHERE NOT EXISTS (
    SELECT 1 FROM server_media_quota WHERE id = 1
);

UPDATE server_media_quota
SET current_storage_bytes = COALESCE(current_storage_bytes, 0),
    current_files_count = COALESCE(current_files_count, 0),
    alert_threshold_percent = COALESCE(alert_threshold_percent, 80)
WHERE id = 1;

ALTER TABLE server_media_quota
    ALTER COLUMN current_storage_bytes SET DEFAULT 0,
    ALTER COLUMN current_files_count SET DEFAULT 0,
    ALTER COLUMN alert_threshold_percent SET DEFAULT 80;

ALTER TABLE server_media_quota
    ALTER COLUMN current_storage_bytes SET NOT NULL,
    ALTER COLUMN current_files_count SET NOT NULL,
    ALTER COLUMN alert_threshold_percent SET NOT NULL;


-- ===== Merged from: 20260410000001_consolidated_feature_additions.sql =====

-- ============================================================================
-- Consolidated Migration: Feature Additions & Indexes
-- Created: 2026-04-22 (consolidated from 7 migrations dated 2026-04-07 ~ 2026-04-18)
--
-- Merged source files:
--   1. 20260407000001_add_ai_connections.sql
--   2. 20260409090000_to_device_stream_id_seq.sql
--   3. 20260413000001_align_report_rate_limits_schema_contract.sql
--   4. 20260413000002_add_lazy_loaded_members.sql
--   5. 20260414000001_add_application_service_webhook_auth.sql
--   6. 20260414000002_hash_access_tokens.sql
--   7. 20260418010100_add_users_created_ts_index.sql
--
-- All statements use IF NOT EXISTS / IF EXISTS guards for idempotent execution.
-- ============================================================================


-- ===== Merged from: 20260407000001_add_ai_connections.sql =====

-- Migration: add ai_connections table
-- Created at: 2026-04-09
-- Description: AI connection configuration table for MCP tool integrations

CREATE TABLE IF NOT EXISTS ai_connections (
    id VARCHAR(36) PRIMARY KEY,
    user_id VARCHAR(255) NOT NULL,
    provider VARCHAR(50) NOT NULL,  -- 'openclaw', 'trendradar', 'hula'
    config JSONB,                   -- 连接配置（如 mcp_url: http://127.0.0.1:3333）
    is_active BOOLEAN DEFAULT true,
    created_ts BIGINT NOT NULL,
    updated_ts BIGINT
);

CREATE INDEX IF NOT EXISTS idx_ai_connections_user_id ON ai_connections(user_id);
CREATE INDEX IF NOT EXISTS idx_ai_connections_provider ON ai_connections(provider);

-- ===== Merged from: 20260409090000_to_device_stream_id_seq.sql =====

DO $$
DECLARE
    target_schema TEXT := current_schema();
    max_stream_id BIGINT := 0;
BEGIN
    IF NOT EXISTS (
        SELECT 1
        FROM pg_class c
        JOIN pg_namespace n ON n.oid = c.relnamespace
        WHERE c.relkind = 'S'
          AND n.nspname = target_schema
          AND c.relname = 'to_device_stream_id_seq'
    ) THEN
        EXECUTE format('CREATE SEQUENCE %I.to_device_stream_id_seq', target_schema);
    END IF;

    IF EXISTS (
        SELECT 1
        FROM information_schema.columns
        WHERE table_schema = target_schema
          AND table_name = 'to_device_messages'
          AND column_name = 'stream_id'
    ) THEN
        EXECUTE format(
            'SELECT COALESCE(MAX(stream_id), 0) FROM %I.to_device_messages',
            target_schema
        )
        INTO max_stream_id;

        PERFORM setval(
            format('%I.to_device_stream_id_seq', target_schema)::regclass,
            GREATEST(max_stream_id, 1),
            max_stream_id > 0
        );
    END IF;
END $$;

-- ===== Merged from: 20260413000001_align_report_rate_limits_schema_contract.sql =====

SET TIME ZONE 'UTC';

CREATE TABLE IF NOT EXISTS report_rate_limits (
    id BIGSERIAL,
    user_id TEXT NOT NULL,
    report_count INTEGER DEFAULT 0,
    is_blocked BOOLEAN DEFAULT FALSE,
    blocked_until_at BIGINT,
    block_reason TEXT,
    last_report_at BIGINT,
    created_ts BIGINT NOT NULL,
    updated_ts BIGINT,
    CONSTRAINT pk_report_rate_limits PRIMARY KEY (id),
    CONSTRAINT uq_report_rate_limits_user UNIQUE (user_id)
);

DO $$
BEGIN
    IF EXISTS (
        SELECT 1
        FROM information_schema.columns
        WHERE table_schema = current_schema()
          AND table_name = 'report_rate_limits'
          AND column_name = 'blocked_until'
    ) AND NOT EXISTS (
        SELECT 1
        FROM information_schema.columns
        WHERE table_schema = current_schema()
          AND table_name = 'report_rate_limits'
          AND column_name = 'blocked_until_at'
    ) THEN
        ALTER TABLE report_rate_limits RENAME COLUMN blocked_until TO blocked_until_at;
    END IF;

    IF EXISTS (
        SELECT 1
        FROM information_schema.columns
        WHERE table_schema = current_schema()
          AND table_name = 'report_rate_limits'
          AND column_name = 'blocked_until_ts'
    ) AND NOT EXISTS (
        SELECT 1
        FROM information_schema.columns
        WHERE table_schema = current_schema()
          AND table_name = 'report_rate_limits'
          AND column_name = 'blocked_until_at'
    ) THEN
        ALTER TABLE report_rate_limits RENAME COLUMN blocked_until_ts TO blocked_until_at;
    END IF;

    IF EXISTS (
        SELECT 1
        FROM information_schema.columns
        WHERE table_schema = current_schema()
          AND table_name = 'report_rate_limits'
          AND column_name = 'last_report_ts'
    ) AND NOT EXISTS (
        SELECT 1
        FROM information_schema.columns
        WHERE table_schema = current_schema()
          AND table_name = 'report_rate_limits'
          AND column_name = 'last_report_at'
    ) THEN
        ALTER TABLE report_rate_limits RENAME COLUMN last_report_ts TO last_report_at;
    END IF;

    IF NOT EXISTS (
        SELECT 1
        FROM information_schema.columns
        WHERE table_schema = current_schema()
          AND table_name = 'report_rate_limits'
          AND column_name = 'blocked_until_at'
    ) THEN
        ALTER TABLE report_rate_limits ADD COLUMN blocked_until_at BIGINT;
    END IF;

    IF NOT EXISTS (
        SELECT 1
        FROM information_schema.columns
        WHERE table_schema = current_schema()
          AND table_name = 'report_rate_limits'
          AND column_name = 'block_reason'
    ) THEN
        ALTER TABLE report_rate_limits ADD COLUMN block_reason TEXT;
    END IF;

    IF NOT EXISTS (
        SELECT 1
        FROM information_schema.columns
        WHERE table_schema = current_schema()
          AND table_name = 'report_rate_limits'
          AND column_name = 'last_report_at'
    ) THEN
        ALTER TABLE report_rate_limits ADD COLUMN last_report_at BIGINT;
    END IF;

    IF NOT EXISTS (
        SELECT 1
        FROM information_schema.columns
        WHERE table_schema = current_schema()
          AND table_name = 'report_rate_limits'
          AND column_name = 'updated_ts'
    ) THEN
        ALTER TABLE report_rate_limits ADD COLUMN updated_ts BIGINT;
    END IF;
END $$;

UPDATE report_rate_limits
SET updated_ts = COALESCE(updated_ts, created_ts)
WHERE updated_ts IS NULL;

CREATE INDEX IF NOT EXISTS idx_report_rate_limits_user
ON report_rate_limits(user_id);


-- ===== Merged from: 20260413000002_add_lazy_loaded_members.sql =====

SET TIME ZONE 'UTC';

CREATE TABLE IF NOT EXISTS lazy_loaded_members (
    user_id TEXT NOT NULL,
    device_id TEXT NOT NULL,
    room_id TEXT NOT NULL,
    member_user_id TEXT NOT NULL,
    created_ts BIGINT NOT NULL,
    updated_ts BIGINT NOT NULL,
    CONSTRAINT pk_lazy_loaded_members PRIMARY KEY (user_id, device_id, room_id, member_user_id)
);

CREATE INDEX IF NOT EXISTS idx_lazy_loaded_members_lookup
ON lazy_loaded_members(user_id, device_id, room_id);


-- ===== Merged from: 20260414000001_add_application_service_webhook_auth.sql =====

DO $$
BEGIN
    IF EXISTS (SELECT 1 FROM information_schema.tables WHERE table_name = 'application_services') THEN
        ALTER TABLE application_services ADD COLUMN IF NOT EXISTS api_key TEXT;
        ALTER TABLE application_services ADD COLUMN IF NOT EXISTS config JSONB NOT NULL DEFAULT '{}'::jsonb;
    END IF;
END $$;

-- ===== Merged from: 20260414000002_hash_access_tokens.sql =====

CREATE EXTENSION IF NOT EXISTS pgcrypto;

ALTER TABLE access_tokens
ADD COLUMN IF NOT EXISTS token_hash TEXT;

UPDATE access_tokens
SET token_hash = replace(
        replace(
            trim(trailing '=' from encode(digest(token, 'sha256'), 'base64')),
            '+',
            '-'
        ),
        '/',
        '_'
    )
WHERE token_hash IS NULL
  AND token IS NOT NULL;

ALTER TABLE access_tokens
ALTER COLUMN token DROP NOT NULL;

DO $$
BEGIN
    IF NOT EXISTS (
        SELECT 1
        FROM pg_constraint
        WHERE conname = 'uq_access_tokens_token_hash'
    ) THEN
        ALTER TABLE access_tokens
        ADD CONSTRAINT uq_access_tokens_token_hash UNIQUE (token_hash);
    END IF;
END $$;

ALTER TABLE access_tokens
DROP CONSTRAINT IF EXISTS uq_access_tokens_token;

CREATE INDEX IF NOT EXISTS idx_access_tokens_token_hash
ON access_tokens(token_hash);

UPDATE access_tokens
SET token = NULL
WHERE token IS NOT NULL;

ALTER TABLE access_tokens
ALTER COLUMN token_hash SET NOT NULL;

-- ===== Merged from: 20260418010100_add_users_created_ts_index.sql =====

CREATE INDEX IF NOT EXISTS idx_users_created_ts
ON users(created_ts DESC);


-- ===== Merged from: 20260422000001_schema_code_alignment.sql =====

-- ============================================================================
-- 数据库结构对齐迁移 (Schema-Code Alignment)
-- 日期: 2026-04-22
-- 目的: 修复 schema 审计发现的 CRITICAL 级不一致
-- ============================================================================

-- C-05: device_keys 缺少 is_fallback 列
ALTER TABLE device_keys ADD COLUMN IF NOT EXISTS is_fallback BOOLEAN NOT NULL DEFAULT FALSE;
CREATE INDEX IF NOT EXISTS idx_device_keys_fallback ON device_keys(user_id, device_id) WHERE is_fallback = TRUE;

-- C-08: to_device_transactions 表不存在
CREATE TABLE IF NOT EXISTS to_device_transactions (
    id BIGSERIAL PRIMARY KEY,
    transaction_id TEXT,
    message_id TEXT,
    sender_user_id TEXT NOT NULL,
    sender_device_id TEXT NOT NULL,
    created_ts BIGINT NOT NULL,
    CONSTRAINT uq_to_device_transactions_txn UNIQUE (transaction_id, sender_user_id, sender_device_id)
);
CREATE INDEX IF NOT EXISTS idx_to_device_transactions_created ON to_device_transactions(created_ts);
CREATE UNIQUE INDEX IF NOT EXISTS uq_to_device_transactions_msg ON to_device_transactions(sender_user_id, sender_device_id, message_id);

-- C-09: push_rules 缺少 priority_class 的兼容性处理
-- push_rules 表已在 unified schema 中有 priority_class，此处确保列存在
ALTER TABLE push_rules ADD COLUMN IF NOT EXISTS priority_class INTEGER NOT NULL DEFAULT 0;

-- C-10/C-11/C-12: push_notification_queue/log/config 补齐缺失列
-- push_notification_queue: 代码需要 priority, status, attempts, max_attempts, next_attempt_at, sent_at, error_message
ALTER TABLE push_notification_queue ADD COLUMN IF NOT EXISTS priority INTEGER NOT NULL DEFAULT 0;
ALTER TABLE push_notification_queue ADD COLUMN IF NOT EXISTS status TEXT NOT NULL DEFAULT 'pending';
ALTER TABLE push_notification_queue ADD COLUMN IF NOT EXISTS attempts INTEGER NOT NULL DEFAULT 0;
ALTER TABLE push_notification_queue ADD COLUMN IF NOT EXISTS max_attempts INTEGER NOT NULL DEFAULT 3;
ALTER TABLE push_notification_queue ADD COLUMN IF NOT EXISTS next_attempt_at TIMESTAMPTZ;
ALTER TABLE push_notification_queue ADD COLUMN IF NOT EXISTS sent_at TIMESTAMPTZ;
ALTER TABLE push_notification_queue ADD COLUMN IF NOT EXISTS error_message TEXT;

-- push_notification_log: 代码需要 event_id, room_id, notification_type, push_type, sent_at, success, provider_response, response_time_ms, metadata
ALTER TABLE push_notification_log ADD COLUMN IF NOT EXISTS event_id TEXT;
ALTER TABLE push_notification_log ADD COLUMN IF NOT EXISTS room_id TEXT;
ALTER TABLE push_notification_log ADD COLUMN IF NOT EXISTS notification_type TEXT;
ALTER TABLE push_notification_log ADD COLUMN IF NOT EXISTS push_type TEXT;
ALTER TABLE push_notification_log ADD COLUMN IF NOT EXISTS sent_at TIMESTAMPTZ;
ALTER TABLE push_notification_log ADD COLUMN IF NOT EXISTS success BOOLEAN;
ALTER TABLE push_notification_log ADD COLUMN IF NOT EXISTS provider_response TEXT;
ALTER TABLE push_notification_log ADD COLUMN IF NOT EXISTS response_time_ms INTEGER;
ALTER TABLE push_notification_log ADD COLUMN IF NOT EXISTS metadata JSONB NOT NULL DEFAULT '{}';

-- push_config: 代码使用 config_key/config_value 而非 config_type/config_data
ALTER TABLE push_config ADD COLUMN IF NOT EXISTS config_key TEXT;
ALTER TABLE push_config ADD COLUMN IF NOT EXISTS config_value TEXT;

-- C-16: e2ee_key_requests 缺少 updated_ts 列
ALTER TABLE e2ee_key_requests ADD COLUMN IF NOT EXISTS updated_ts BIGINT;

-- ============================================================================
-- 第二轮审计修复 (2026-04-22 续)
-- ============================================================================

-- federation_blacklist: 代码需要 block_type, blocked_by, created_ts, expires_at, is_enabled, metadata
-- 基线 schema 只有: server_name, reason, added_ts, added_by, updated_ts
ALTER TABLE federation_blacklist ADD COLUMN IF NOT EXISTS block_type TEXT NOT NULL DEFAULT 'manual';
ALTER TABLE federation_blacklist ADD COLUMN IF NOT EXISTS blocked_by TEXT;
ALTER TABLE federation_blacklist ADD COLUMN IF NOT EXISTS created_ts BIGINT;
ALTER TABLE federation_blacklist ADD COLUMN IF NOT EXISTS expires_at BIGINT;
ALTER TABLE federation_blacklist ADD COLUMN IF NOT EXISTS is_enabled BOOLEAN NOT NULL DEFAULT TRUE;
ALTER TABLE federation_blacklist ADD COLUMN IF NOT EXISTS metadata JSONB NOT NULL DEFAULT '{}';
DO $$ BEGIN
    IF EXISTS (SELECT 1 FROM information_schema.columns WHERE table_name = 'federation_blacklist' AND column_name = 'added_ts') THEN
        UPDATE federation_blacklist SET created_ts = added_ts WHERE created_ts IS NULL AND added_ts IS NOT NULL;
    END IF;
    IF EXISTS (SELECT 1 FROM information_schema.columns WHERE table_name = 'federation_blacklist' AND column_name = 'added_by') THEN
        UPDATE federation_blacklist SET blocked_by = added_by WHERE blocked_by IS NULL AND added_by IS NOT NULL;
    END IF;
END $$;

-- event_signatures: INSERT 缺少 algorithm 列 — 添加默认值使其可省略
-- 注意: 已有数据的 algorithm 为 NOT NULL，新增默认值仅影响新 INSERT
DO $$ BEGIN
    ALTER TABLE event_signatures ALTER COLUMN algorithm SET DEFAULT 'ed25519';
EXCEPTION WHEN others THEN NULL;
END $$;

-- push_notification_queue: 放宽 NOT NULL 约束（代码使用 Option<String>）
DO $$ BEGIN
    ALTER TABLE push_notification_queue ALTER COLUMN event_id DROP NOT NULL;
    ALTER TABLE push_notification_queue ALTER COLUMN room_id DROP NOT NULL;
    ALTER TABLE push_notification_queue ALTER COLUMN notification_type DROP NOT NULL;
EXCEPTION WHEN others THEN NULL;
END $$;

-- push_notification_log: 放宽 pushkey/status NOT NULL（代码不提供这些列）
DO $$ BEGIN
    ALTER TABLE push_notification_log ALTER COLUMN pushkey DROP NOT NULL;
    ALTER TABLE push_notification_log ALTER COLUMN status DROP NOT NULL;
EXCEPTION WHEN others THEN NULL;
END $$;

-- user_privacy_settings: 旧 schema 使用 allow_* BOOLEAN 列，代码使用 *_visibility TEXT 列
-- 为已部署环境添加新列（新环境通过 extensions_privacy.sql 直接创建正确 schema）
DO $$ BEGIN
    IF NOT EXISTS (SELECT 1 FROM information_schema.columns WHERE table_name = 'user_privacy_settings' AND column_name = 'id') THEN
        ALTER TABLE user_privacy_settings ADD COLUMN id BIGSERIAL;
    END IF;
    ALTER TABLE user_privacy_settings ADD COLUMN IF NOT EXISTS profile_visibility TEXT NOT NULL DEFAULT 'public';
    ALTER TABLE user_privacy_settings ADD COLUMN IF NOT EXISTS avatar_visibility TEXT NOT NULL DEFAULT 'public';
    ALTER TABLE user_privacy_settings ADD COLUMN IF NOT EXISTS displayname_visibility TEXT NOT NULL DEFAULT 'public';
    ALTER TABLE user_privacy_settings ADD COLUMN IF NOT EXISTS presence_visibility TEXT NOT NULL DEFAULT 'contacts';
    ALTER TABLE user_privacy_settings ADD COLUMN IF NOT EXISTS room_membership_visibility TEXT NOT NULL DEFAULT 'contacts';
EXCEPTION WHEN others THEN NULL;
END $$;

-- ============================================================================
-- 第三轮审计修复 (2026-04-22 续)
-- ============================================================================

-- e2ee_secret_storage_keys: 代码使用 encrypted_key/public_key/signatures，schema 使用 key_data
ALTER TABLE e2ee_secret_storage_keys ADD COLUMN IF NOT EXISTS encrypted_key TEXT;
ALTER TABLE e2ee_secret_storage_keys ADD COLUMN IF NOT EXISTS public_key TEXT;
ALTER TABLE e2ee_secret_storage_keys ADD COLUMN IF NOT EXISTS signatures JSONB;

-- e2ee_stored_secrets: 代码使用 encrypted_secret/key_id，schema 使用 secret_data/key_key_id
ALTER TABLE e2ee_stored_secrets ADD COLUMN IF NOT EXISTS encrypted_secret TEXT;
-- key_id 列可能与 e2ee_secret_storage_keys 的 UNIQUE key_id 冲突，使用不同名
DO $$ BEGIN
    ALTER TABLE e2ee_stored_secrets ADD COLUMN IF NOT EXISTS key_id TEXT;
EXCEPTION WHEN others THEN NULL;
END $$;

-- e2ee_audit_log: 代码使用 operation/key_id/ip_address，schema 使用 action/event_id (无 ip_address)
ALTER TABLE e2ee_audit_log ADD COLUMN IF NOT EXISTS operation TEXT;
ALTER TABLE e2ee_audit_log ADD COLUMN IF NOT EXISTS key_id TEXT;
ALTER TABLE e2ee_audit_log ADD COLUMN IF NOT EXISTS ip_address TEXT;

-- space_summaries: SELECT * 返回 id，但 SpaceSummary struct 无 id 字段
-- 修复方式: 不改 schema，改代码（添加 id 字段到 struct）

-- ============================================================================
-- 第四轮审计修复 (2026-04-22 续)
-- ============================================================================

-- registration_token_usage: 代码使用 7 个不存在的列
ALTER TABLE registration_token_usage ADD COLUMN IF NOT EXISTS token TEXT;
ALTER TABLE registration_token_usage ADD COLUMN IF NOT EXISTS username TEXT;
ALTER TABLE registration_token_usage ADD COLUMN IF NOT EXISTS email TEXT;
ALTER TABLE registration_token_usage ADD COLUMN IF NOT EXISTS ip_address TEXT;
ALTER TABLE registration_token_usage ADD COLUMN IF NOT EXISTS user_agent TEXT;
ALTER TABLE registration_token_usage ADD COLUMN IF NOT EXISTS success BOOLEAN NOT NULL DEFAULT TRUE;
ALTER TABLE registration_token_usage ADD COLUMN IF NOT EXISTS error_message TEXT;

-- room_invites: 代码使用完全不同的列名（invite_code 设计）
ALTER TABLE room_invites ADD COLUMN IF NOT EXISTS invite_code TEXT;
ALTER TABLE room_invites ADD COLUMN IF NOT EXISTS inviter_user_id TEXT;
ALTER TABLE room_invites ADD COLUMN IF NOT EXISTS invitee_email TEXT;
ALTER TABLE room_invites ADD COLUMN IF NOT EXISTS invitee_user_id TEXT;
ALTER TABLE room_invites ADD COLUMN IF NOT EXISTS is_used BOOLEAN NOT NULL DEFAULT FALSE;
ALTER TABLE room_invites ADD COLUMN IF NOT EXISTS is_revoked BOOLEAN NOT NULL DEFAULT FALSE;
ALTER TABLE room_invites ADD COLUMN IF NOT EXISTS used_ts BIGINT;
ALTER TABLE room_invites ADD COLUMN IF NOT EXISTS revoked_at BIGINT;
ALTER TABLE room_invites ADD COLUMN IF NOT EXISTS revoked_reason TEXT;
DO $$ BEGIN
    IF EXISTS (SELECT 1 FROM information_schema.columns WHERE table_name = 'room_invites' AND column_name = 'inviter') THEN
        UPDATE room_invites SET inviter_user_id = inviter WHERE inviter_user_id IS NULL AND inviter IS NOT NULL;
    END IF;
    IF EXISTS (SELECT 1 FROM information_schema.columns WHERE table_name = 'room_invites' AND column_name = 'invitee') THEN
        UPDATE room_invites SET invitee_user_id = invitee WHERE invitee_user_id IS NULL AND invitee IS NOT NULL;
    END IF;
END $$;

-- application_service_state: 代码使用 state_value (String) 但 schema 使用 value (JSONB)
ALTER TABLE application_service_state ADD COLUMN IF NOT EXISTS state_value TEXT;

-- application_service_transactions: 代码使用不同的列名
ALTER TABLE application_service_transactions ADD COLUMN IF NOT EXISTS transaction_id TEXT;
ALTER TABLE application_service_transactions ADD COLUMN IF NOT EXISTS events JSONB;
ALTER TABLE application_service_transactions ADD COLUMN IF NOT EXISTS sent_ts BIGINT;
ALTER TABLE application_service_transactions ADD COLUMN IF NOT EXISTS completed_ts BIGINT;
ALTER TABLE application_service_transactions ADD COLUMN IF NOT EXISTS retry_count INTEGER NOT NULL DEFAULT 0;
ALTER TABLE application_service_transactions ADD COLUMN IF NOT EXISTS last_error TEXT;
DO $$ BEGIN
    IF EXISTS (SELECT 1 FROM information_schema.columns WHERE table_name = 'application_service_transactions' AND column_name = 'txn_id') THEN
        UPDATE application_service_transactions SET transaction_id = txn_id WHERE transaction_id IS NULL AND txn_id IS NOT NULL;
    END IF;
    IF EXISTS (SELECT 1 FROM information_schema.columns WHERE table_name = 'application_service_transactions' AND column_name = 'data') THEN
        UPDATE application_service_transactions SET events = data WHERE events IS NULL AND data IS NOT NULL;
    END IF;
    UPDATE application_service_transactions SET sent_ts = created_ts WHERE sent_ts IS NULL AND created_ts IS NOT NULL;
END $$;

-- thread_subscriptions: 代码缺少 is_pinned 字段 (schema 有)
-- 修复方式: 代码添加字段（已在 Rust 代码中修复）

-- registration_tokens: created_by 放宽 NOT NULL
DO $$ BEGIN
    ALTER TABLE registration_tokens ALTER COLUMN created_by DROP NOT NULL;
EXCEPTION WHEN others THEN NULL;
END $$;


-- ===== Merged from: 20260423000001_add_federation_admission_control.sql =====

ALTER TABLE federation_servers ADD COLUMN IF NOT EXISTS status TEXT NOT NULL DEFAULT 'active';
ALTER TABLE federation_servers ADD COLUMN IF NOT EXISTS updated_ts BIGINT;

CREATE INDEX IF NOT EXISTS idx_federation_servers_status ON federation_servers(status);

COMMENT ON COLUMN federation_servers.status IS 'Federation admission status: pending, active, rejected';
COMMENT ON COLUMN federation_servers.updated_ts IS 'Timestamp of last status update in milliseconds';


-- ===== Merged from: 20260423000002_fix_auth_token_schema.sql =====

ALTER TABLE access_tokens ALTER COLUMN token DROP NOT NULL;

DO $$
BEGIN
    IF NOT EXISTS (
        SELECT 1 FROM pg_constraint WHERE conname = 'uq_access_tokens_token_hash'
    ) THEN
        ALTER TABLE access_tokens ADD CONSTRAINT uq_access_tokens_token_hash UNIQUE (token_hash);
    END IF;
END $$;

DO $$
BEGIN
    IF NOT EXISTS (
        SELECT 1 FROM pg_constraint WHERE conname = 'uq_refresh_tokens_token_hash'
    ) THEN
        ALTER TABLE refresh_tokens ADD CONSTRAINT uq_refresh_tokens_token_hash UNIQUE (token_hash);
    END IF;
END $$;

DO $$
BEGIN
    IF NOT EXISTS (
        SELECT 1 FROM pg_constraint WHERE conname = 'uq_token_blacklist_token_hash'
    ) THEN
        ALTER TABLE token_blacklist ADD CONSTRAINT uq_token_blacklist_token_hash UNIQUE (token_hash);
    END IF;
END $$;

CREATE INDEX IF NOT EXISTS idx_access_tokens_token_hash ON access_tokens(token_hash);
CREATE INDEX IF NOT EXISTS idx_access_tokens_valid ON access_tokens(is_revoked) WHERE is_revoked = FALSE;
CREATE INDEX IF NOT EXISTS idx_refresh_tokens_user_id ON refresh_tokens(user_id);
CREATE INDEX IF NOT EXISTS idx_refresh_tokens_revoked ON refresh_tokens(is_revoked);
CREATE INDEX IF NOT EXISTS idx_token_blacklist_hash ON token_blacklist(token_hash);


-- ===== Merged from: 20260430000001_add_oidc_user_mapping.sql =====

-- =============================================================================
-- OIDC user mapping: bind external OIDC (issuer, subject) -> local Matrix user
-- =============================================================================
-- Without this binding, a local user @alice:server registered via password can
-- be impersonated by anyone who can make an OIDC IdP issue a token whose
-- mapped localpart resolves to "alice". The token endpoint must refuse to
-- issue Matrix credentials for an existing local user that has no recorded
-- (issuer, subject) ownership.

CREATE TABLE IF NOT EXISTS oidc_user_mapping (
    id BIGSERIAL,
    issuer TEXT NOT NULL,
    subject TEXT NOT NULL,
    user_id TEXT NOT NULL,
    first_seen_ts BIGINT NOT NULL,
    last_authenticated_ts BIGINT NOT NULL,
    authentication_count INTEGER NOT NULL DEFAULT 1,
    CONSTRAINT pk_oidc_user_mapping PRIMARY KEY (id),
    CONSTRAINT uq_oidc_user_mapping_issuer_subject UNIQUE (issuer, subject)
);

CREATE INDEX IF NOT EXISTS idx_oidc_user_mapping_user ON oidc_user_mapping(user_id);


-- ===== Merged from: 20260430000002_add_missing_perf_indexes.sql =====

-- =============================================================================
-- Restore the two performance indexes that schema_health_check expects but
-- which are not present in the consolidated schema:
--   - idx_memberships_user_room  on room_memberships(user_id, room_id)
--   - idx_user_threepids_medium_address on user_threepids(medium, address)
-- Both speed up extremely hot lookups (room membership joins on the per-user
-- side, and 3PID resolution on login / password reset). Synapse upstream has
-- analogous indexes; we previously archived them but never re-applied.
-- =============================================================================

DO $$
BEGIN
    IF EXISTS (SELECT 1 FROM information_schema.tables
               WHERE table_schema = 'public' AND table_name = 'room_memberships') THEN
        EXECUTE 'CREATE INDEX IF NOT EXISTS idx_memberships_user_room
                 ON room_memberships(user_id, room_id)';
    END IF;

    IF EXISTS (SELECT 1 FROM information_schema.tables
               WHERE table_schema = 'public' AND table_name = 'user_threepids') THEN
        EXECUTE 'CREATE INDEX IF NOT EXISTS idx_user_threepids_medium_address
                 ON user_threepids(medium, address)';
    END IF;
END
$$;


-- ===== Merged from: 20260501000001_backup_keys_metadata.sql =====

-- Promote per-session KeyBackupData metadata to real columns so we can
-- index/query them and stop wrapping them inside the session_data jsonb
-- payload. See docs/synapse-rust/SPEC_ALIGNMENT_PLAN_2026-05-01.md §4.

ALTER TABLE backup_keys
    ADD COLUMN IF NOT EXISTS first_message_index BIGINT  NOT NULL DEFAULT 0,
    ADD COLUMN IF NOT EXISTS forwarded_count     BIGINT  NOT NULL DEFAULT 0,
    ADD COLUMN IF NOT EXISTS is_verified         BOOLEAN NOT NULL DEFAULT FALSE;

DO $$ BEGIN
    IF EXISTS (SELECT 1 FROM information_schema.columns WHERE table_name = 'backup_keys' AND column_name = 'session_data') THEN
        UPDATE backup_keys
        SET    first_message_index = COALESCE((session_data ->> 'first_message_index')::BIGINT,  first_message_index),
               forwarded_count     = COALESCE((session_data ->> 'forwarded_count')::BIGINT,      forwarded_count),
               is_verified         = COALESCE((session_data ->> 'is_verified')::BOOLEAN,         is_verified)
        WHERE  jsonb_typeof(session_data) = 'object';
    END IF;
END $$;


-- ===== Merged from: 20260505000001_add_user_search_and_presence_indexes.sql =====

-- Case-insensitive directory search on username.
CREATE INDEX IF NOT EXISTS idx_users_lower_username ON users (LOWER(username));

-- Search code uses LOWER(COALESCE(displayname, '')), so index the same expression.
CREATE INDEX IF NOT EXISTS idx_users_lower_displayname
    ON users (LOWER(COALESCE(displayname, '')));

-- Support exact/prefix email lookup in directory search.
CREATE INDEX IF NOT EXISTS idx_users_lower_email
    ON users (LOWER(COALESCE(email, '')));

-- Friend list and search fall back to created_ts ordering.
CREATE INDEX IF NOT EXISTS idx_users_created_ts ON users (created_ts DESC);

-- Presence joins for friend list projection.
CREATE INDEX IF NOT EXISTS idx_presence_user_id ON presence (user_id);


-- ===== Merged from: 20260505000002_add_saml_config_overrides.sql =====

-- Persistent runtime overrides for SamlConfig fields.
-- Admin-edited values from PUT /_synapse/admin/v1/saml/config are stored
-- here keyed by field name (e.g. "metadata_url", "session_lifetime") so
-- they survive process restarts. Only fields listed in
-- SamlService::MUTABLE_CONFIG_FIELDS may be written; enforcement is in
-- the service layer, not in the schema.
CREATE TABLE IF NOT EXISTS saml_config_overrides (
    config_key TEXT NOT NULL,
    config_value JSONB NOT NULL,
    updated_ts BIGINT NOT NULL,
    CONSTRAINT pk_saml_config_overrides PRIMARY KEY (config_key)
);

CREATE INDEX IF NOT EXISTS idx_saml_config_overrides_updated_ts
    ON saml_config_overrides (updated_ts DESC);


-- ===== Merged from: 20260507000002_repair_legacy_background_retention_room_alias_schema.sql =====

BEGIN;

-- Repair legacy databases that predate the consolidated schema for
-- background updates, retention policies, and room aliases.

ALTER TABLE background_updates
    ADD COLUMN IF NOT EXISTS retry_count INTEGER NOT NULL DEFAULT 0,
    ADD COLUMN IF NOT EXISTS max_retries INTEGER NOT NULL DEFAULT 3,
    ADD COLUMN IF NOT EXISTS is_running BOOLEAN NOT NULL DEFAULT FALSE;

CREATE INDEX IF NOT EXISTS idx_background_updates_running
    ON background_updates(is_running)
    WHERE is_running = TRUE;

ALTER TABLE room_retention_policies
    ADD COLUMN IF NOT EXISTS is_server_default BOOLEAN NOT NULL DEFAULT FALSE;

CREATE INDEX IF NOT EXISTS idx_room_retention_policies_server_default
    ON room_retention_policies(is_server_default)
    WHERE is_server_default = TRUE;

ALTER TABLE room_aliases
    ADD COLUMN IF NOT EXISTS room_alias TEXT,
    ADD COLUMN IF NOT EXISTS server_name TEXT;

DO $$
BEGIN
    IF EXISTS (
        SELECT 1
        FROM information_schema.columns
        WHERE table_schema = 'public'
          AND table_name = 'room_aliases'
          AND column_name = 'alias'
    ) THEN
        EXECUTE $sql$
            UPDATE room_aliases
            SET room_alias = alias
            WHERE room_alias IS NULL
              AND alias IS NOT NULL
        $sql$;
    END IF;
END $$;

UPDATE room_aliases
SET server_name = NULLIF(split_part(room_alias, ':', 2), '')
WHERE server_name IS NULL
  AND room_alias IS NOT NULL
  AND position(':' IN room_alias) > 0;

UPDATE room_aliases
SET server_name = ''
WHERE server_name IS NULL;

ALTER TABLE room_aliases
    ALTER COLUMN server_name SET DEFAULT '',
    ALTER COLUMN server_name SET NOT NULL;

DO $$
BEGIN
    IF NOT EXISTS (
        SELECT 1
        FROM room_aliases
        WHERE room_alias IS NULL
    ) THEN
        EXECUTE 'ALTER TABLE room_aliases ALTER COLUMN room_alias SET NOT NULL';
    END IF;
END $$;

CREATE UNIQUE INDEX IF NOT EXISTS idx_room_aliases_room_alias
    ON room_aliases(room_alias);

CREATE INDEX IF NOT EXISTS idx_room_aliases_room_id
    ON room_aliases(room_id);

COMMIT;
