-- =============================================================================
-- Synapse-Rust 增量数据库迁移脚本
-- 版本: 20260219000001
-- 创建日期: 2026-02-19
-- 描述: 增量更新现有表结构，添加缺失的列和索引
-- 
-- 执行方式:
--   docker exec -i synapse-postgres psql -U synapse -d synapse_test < 20260219000001_incremental_migration.sql
-- =============================================================================

-- 版本记录
INSERT INTO schema_migrations (version, description)
VALUES ('20260219000001', 'Incremental migration - add missing columns and indexes')
ON CONFLICT (version) DO NOTHING;

-- =============================================================================
-- 修复 events 表
-- =============================================================================

ALTER TABLE events ADD COLUMN IF NOT EXISTS stream_ordering BIGINT;
ALTER TABLE events ADD COLUMN IF NOT EXISTS type VARCHAR(255);
ALTER TABLE events ADD COLUMN IF NOT EXISTS topological_ordering BIGINT;

-- 更新 type 列数据
UPDATE events SET type = event_type WHERE type IS NULL AND event_type IS NOT NULL;

CREATE INDEX IF NOT EXISTS idx_events_stream ON events(stream_ordering);

-- =============================================================================
-- 修复 media_repository 表
-- =============================================================================

ALTER TABLE media_repository ADD COLUMN IF NOT EXISTS media_origin VARCHAR(255);
ALTER TABLE media_repository ADD COLUMN IF NOT EXISTS media_type VARCHAR(255);
ALTER TABLE media_repository ADD COLUMN IF NOT EXISTS media_length BIGINT;
ALTER TABLE media_repository ADD COLUMN IF NOT EXISTS upload_name TEXT;
ALTER TABLE media_repository ADD COLUMN IF NOT EXISTS created_ts BIGINT;
ALTER TABLE media_repository ADD COLUMN IF NOT EXISTS last_access_ts BIGINT;
ALTER TABLE media_repository ADD COLUMN IF NOT EXISTS quarantine_media BOOLEAN DEFAULT FALSE;
ALTER TABLE media_repository ADD COLUMN IF NOT EXISTS safe_from_quarantine BOOLEAN DEFAULT FALSE;
ALTER TABLE media_repository ADD COLUMN IF NOT EXISTS user_id VARCHAR(255);
ALTER TABLE media_repository ADD COLUMN IF NOT EXISTS server_name VARCHAR(255);

CREATE INDEX IF NOT EXISTS idx_media_repository_origin ON media_repository(media_origin);
CREATE INDEX IF NOT EXISTS idx_media_repository_user ON media_repository(user_id);

-- =============================================================================
-- 修复 rooms 表
-- =============================================================================

ALTER TABLE rooms ADD COLUMN IF NOT EXISTS join_rules VARCHAR(50) DEFAULT 'invite';
ALTER TABLE rooms ADD COLUMN IF NOT EXISTS join_rules_event_id VARCHAR(255);
ALTER TABLE rooms ADD COLUMN IF NOT EXISTS create_event_id VARCHAR(255);
ALTER TABLE rooms ADD COLUMN IF NOT EXISTS created_ts BIGINT;
ALTER TABLE rooms ADD COLUMN IF NOT EXISTS updated_ts BIGINT;

-- =============================================================================
-- 修复 voice_messages 表
-- =============================================================================

ALTER TABLE voice_messages ADD COLUMN IF NOT EXISTS processed_ts BIGINT;
ALTER TABLE voice_messages ADD COLUMN IF NOT EXISTS mime_type VARCHAR(100);
ALTER TABLE voice_messages ADD COLUMN IF NOT EXISTS encryption JSONB;
ALTER TABLE voice_messages ADD COLUMN IF NOT EXISTS waveform_data TEXT;
ALTER TABLE voice_messages ADD COLUMN IF NOT EXISTS transcribe_text TEXT;

-- 数据迁移
UPDATE voice_messages SET processed_ts = processed_at WHERE processed_ts IS NULL AND processed_at IS NOT NULL;

-- =============================================================================
-- 确保推送相关表存在
-- =============================================================================

-- pushers 表
CREATE TABLE IF NOT EXISTS pushers (
    id BIGSERIAL PRIMARY KEY,
    user_id VARCHAR(255) NOT NULL,
    pushkey TEXT NOT NULL,
    kind VARCHAR(50) NOT NULL DEFAULT 'http',
    app_id VARCHAR(255) NOT NULL,
    app_display_name VARCHAR(255),
    device_display_name VARCHAR(255),
    profile_tag VARCHAR(255),
    lang VARCHAR(20) DEFAULT 'en',
    data JSONB DEFAULT '{}',
    enabled BOOLEAN DEFAULT true,
    created_ts BIGINT NOT NULL DEFAULT EXTRACT(EPOCH FROM NOW()) * 1000,
    last_updated_ts BIGINT,
    last_success_ts BIGINT,
    last_failure_ts BIGINT,
    failure_count INTEGER DEFAULT 0,
    CONSTRAINT pushers_user_pushkey_unique UNIQUE(user_id, pushkey)
);

CREATE INDEX IF NOT EXISTS idx_pushers_user ON pushers(user_id);
CREATE INDEX IF NOT EXISTS idx_pushers_kind ON pushers(kind);
CREATE INDEX IF NOT EXISTS idx_pushers_enabled ON pushers(enabled);

-- push_rules 表 (Matrix 标准)
CREATE TABLE IF NOT EXISTS push_rules (
    id BIGSERIAL PRIMARY KEY,
    user_id VARCHAR(255) NOT NULL,
    rule_id VARCHAR(255) NOT NULL,
    scope VARCHAR(50) NOT NULL DEFAULT 'global',
    kind VARCHAR(50) NOT NULL,
    priority INTEGER DEFAULT 0,
    conditions JSONB DEFAULT '[]',
    actions JSONB DEFAULT '[]',
    enabled BOOLEAN DEFAULT true,
    is_default BOOLEAN DEFAULT false,
    created_ts BIGINT NOT NULL DEFAULT EXTRACT(EPOCH FROM NOW()) * 1000,
    updated_ts BIGINT,
    CONSTRAINT push_rules_user_rule_unique UNIQUE(user_id, scope, kind, rule_id),
    CONSTRAINT push_rules_scope_check CHECK (scope IN ('global', 'device')),
    CONSTRAINT push_rules_kind_check CHECK (kind IN ('override', 'content', 'room', 'sender', 'underride'))
);

CREATE INDEX IF NOT EXISTS idx_push_rules_user ON push_rules(user_id);
CREATE INDEX IF NOT EXISTS idx_push_rules_scope ON push_rules(scope);
CREATE INDEX IF NOT EXISTS idx_push_rules_kind ON push_rules(kind);

-- 插入默认推送规则
INSERT INTO push_rules (user_id, rule_id, scope, kind, priority, conditions, actions, enabled, is_default)
VALUES 
    ('.default', '.m.rule.master', 'global', 'override', 0, '[]', '["dont_notify"]', true, true),
    ('.default', '.m.rule.suppress_notices', 'global', 'override', 1, '[{"kind": "event_match", "key": "content.msgtype", "pattern": "m.notice"}]', '["dont_notify"]', true, true),
    ('.default', '.m.rule.invite_for_me', 'global', 'override', 2, '[{"kind": "event_match", "key": "type", "pattern": "m.room.member"}, {"kind": "event_match", "key": "content.membership", "pattern": "invite"}, {"kind": "event_match", "key": "state_key", "pattern": "_self"}]', '["notify", {"set_tweak": "sound", "value": "default"}]', true, true),
    ('.default', '.m.rule.member_event', 'global', 'override', 3, '[{"kind": "event_match", "key": "type", "pattern": "m.room.member"}]', '["dont_notify"]', true, true),
    ('.default', '.m.rule.contains_display_name', 'global', 'content', 4, '[{"kind": "contains_display_name"}]', '["notify", {"set_tweak": "sound", "value": "default"}, {"set_tweak": "highlight", "value": true}]', true, true),
    ('.default', '.m.rule.tombstone', 'global', 'override', 5, '[{"kind": "event_match", "key": "type", "pattern": "m.room.tombstone"}, {"kind": "event_match", "key": "state_key", "pattern": ""}]', '["notify", {"set_tweak": "highlight", "value": true}]', true, true),
    ('.default', '.m.rule.room_notif', 'global', 'content', 6, '[{"kind": "event_match", "key": "content.body", "pattern": "@room"}]', '["notify", {"set_tweak": "highlight", "value": true}]', true, true),
    ('.default', '.m.rule.message', 'global', 'underride', 7, '[{"kind": "event_match", "key": "type", "pattern": "m.room.message"}]', '["notify", {"set_tweak": "sound", "value": "default"}]', true, true),
    ('.default', '.m.rule.encrypted', 'global', 'underride', 8, '[{"kind": "event_match", "key": "type", "pattern": "m.room.encrypted"}]', '["notify", {"set_tweak": "sound", "value": "default"}]', true, true)
ON CONFLICT (user_id, scope, kind, rule_id) DO NOTHING;

-- =============================================================================
-- 确保验证码相关表存在
-- =============================================================================

CREATE TABLE IF NOT EXISTS registration_captcha (
    id SERIAL PRIMARY KEY,
    captcha_id VARCHAR(64) NOT NULL UNIQUE,
    captcha_type VARCHAR(20) NOT NULL,
    target VARCHAR(255) NOT NULL,
    code VARCHAR(20) NOT NULL,
    created_at TIMESTAMP WITH TIME ZONE DEFAULT NOW(),
    expires_at TIMESTAMP WITH TIME ZONE NOT NULL,
    used_at TIMESTAMP WITH TIME ZONE,
    verified_at TIMESTAMP WITH TIME ZONE,
    ip_address VARCHAR(45),
    user_agent TEXT,
    attempt_count INTEGER DEFAULT 0,
    max_attempts INTEGER DEFAULT 5,
    status VARCHAR(20) DEFAULT 'pending',
    metadata JSONB DEFAULT '{}',
    CONSTRAINT registration_captcha_type_check CHECK (captcha_type IN ('email', 'sms', 'image'))
);

CREATE INDEX IF NOT EXISTS idx_registration_captcha_target ON registration_captcha(target);
CREATE INDEX IF NOT EXISTS idx_registration_captcha_status ON registration_captcha(status);
CREATE INDEX IF NOT EXISTS idx_registration_captcha_expires_at ON registration_captcha(expires_at);

CREATE TABLE IF NOT EXISTS captcha_send_log (
    id SERIAL PRIMARY KEY,
    captcha_id VARCHAR(64),
    captcha_type VARCHAR(20) NOT NULL,
    target VARCHAR(255) NOT NULL,
    sent_at TIMESTAMP WITH TIME ZONE DEFAULT NOW(),
    ip_address VARCHAR(45),
    user_agent TEXT,
    success BOOLEAN DEFAULT true,
    error_message TEXT,
    provider VARCHAR(50),
    provider_response TEXT,
    CONSTRAINT captcha_send_log_captcha_type_check CHECK (captcha_type IN ('email', 'sms', 'image'))
);

CREATE INDEX IF NOT EXISTS idx_captcha_send_log_target ON captcha_send_log(target);
CREATE INDEX IF NOT EXISTS idx_captcha_send_log_sent_at ON captcha_send_log(sent_at);

CREATE TABLE IF NOT EXISTS captcha_rate_limit (
    id SERIAL PRIMARY KEY,
    target VARCHAR(255) NOT NULL,
    ip_address VARCHAR(45),
    captcha_type VARCHAR(20) NOT NULL,
    request_count INTEGER DEFAULT 1,
    first_request_at TIMESTAMP WITH TIME ZONE DEFAULT NOW(),
    last_request_at TIMESTAMP WITH TIME ZONE DEFAULT NOW(),
    blocked_until TIMESTAMP WITH TIME ZONE,
    CONSTRAINT captcha_rate_limit_captcha_type_check CHECK (captcha_type IN ('email', 'sms', 'image')),
    CONSTRAINT captcha_rate_limit_unique UNIQUE(target, captcha_type)
);

CREATE INDEX IF NOT EXISTS idx_captcha_rate_limit_target ON captcha_rate_limit(target);
CREATE INDEX IF NOT EXISTS idx_captcha_rate_limit_blocked ON captcha_rate_limit(blocked_until);

CREATE TABLE IF NOT EXISTS captcha_template (
    id SERIAL PRIMARY KEY,
    template_name VARCHAR(100) NOT NULL UNIQUE,
    captcha_type VARCHAR(20) NOT NULL,
    subject VARCHAR(255),
    content TEXT NOT NULL,
    variables JSONB DEFAULT '[]',
    is_default BOOLEAN DEFAULT false,
    enabled BOOLEAN DEFAULT true,
    created_at TIMESTAMP WITH TIME ZONE DEFAULT NOW(),
    updated_at TIMESTAMP WITH TIME ZONE DEFAULT NOW(),
    CONSTRAINT captcha_template_captcha_type_check CHECK (captcha_type IN ('email', 'sms', 'image'))
);

CREATE INDEX IF NOT EXISTS idx_captcha_template_type ON captcha_template(captcha_type);

CREATE TABLE IF NOT EXISTS captcha_config (
    id SERIAL PRIMARY KEY,
    config_key VARCHAR(100) NOT NULL UNIQUE,
    config_value TEXT NOT NULL,
    description TEXT,
    created_at TIMESTAMP WITH TIME ZONE DEFAULT NOW(),
    updated_at TIMESTAMP WITH TIME ZONE DEFAULT NOW()
);

CREATE INDEX IF NOT EXISTS idx_captcha_config_key ON captcha_config(config_key);

-- 插入默认验证码模板和配置
INSERT INTO captcha_template (template_name, captcha_type, subject, content, variables, is_default, enabled)
VALUES 
    ('default_email', 'email', '您的注册验证码', '您的注册验证码是：{{code}}，有效期{{expiry_minutes}}分钟。如非本人操作，请忽略此邮件。', '["code", "expiry_minutes"]', true, true),
    ('default_sms', 'sms', NULL, '您的注册验证码：{{code}}，有效期{{expiry_minutes}}分钟。', '["code", "expiry_minutes"]', true, true)
ON CONFLICT (template_name) DO NOTHING;

INSERT INTO captcha_config (config_key, config_value, description)
VALUES 
    ('email.code_length', '6', '邮箱验证码长度'),
    ('email.code_expiry_minutes', '10', '邮箱验证码有效期（分钟）'),
    ('email.max_attempts', '5', '邮箱验证码最大尝试次数'),
    ('sms.code_length', '6', '短信验证码长度'),
    ('sms.code_expiry_minutes', '5', '短信验证码有效期（分钟）'),
    ('global.block_duration_minutes', '30', '触发限制后的封禁时长（分钟）')
ON CONFLICT (config_key) DO NOTHING;

-- =============================================================================
-- 确保推送通知表存在
-- =============================================================================

CREATE TABLE IF NOT EXISTS push_device (
    id SERIAL PRIMARY KEY,
    user_id VARCHAR(255) NOT NULL,
    device_id VARCHAR(255) NOT NULL,
    push_token TEXT NOT NULL,
    push_type VARCHAR(20) NOT NULL,
    app_id VARCHAR(255),
    platform VARCHAR(50),
    platform_version VARCHAR(50),
    app_version VARCHAR(50),
    locale VARCHAR(20),
    timezone VARCHAR(50),
    enabled BOOLEAN DEFAULT true,
    created_at TIMESTAMP WITH TIME ZONE DEFAULT NOW(),
    updated_at TIMESTAMP WITH TIME ZONE DEFAULT NOW(),
    last_used_at TIMESTAMP WITH TIME ZONE,
    last_error TEXT,
    error_count INTEGER DEFAULT 0,
    metadata JSONB DEFAULT '{}',
    CONSTRAINT push_device_user_device_unique UNIQUE(user_id, device_id),
    CONSTRAINT push_device_type_check CHECK (push_type IN ('fcm', 'apns', 'webpush', 'upstream'))
);

CREATE INDEX IF NOT EXISTS idx_push_device_user ON push_device(user_id);
CREATE INDEX IF NOT EXISTS idx_push_device_token ON push_device(push_token);
CREATE INDEX IF NOT EXISTS idx_push_device_type ON push_device(push_type);

CREATE TABLE IF NOT EXISTS push_rule (
    id SERIAL PRIMARY KEY,
    user_id VARCHAR(255) NOT NULL,
    rule_id VARCHAR(255) NOT NULL,
    scope VARCHAR(50) NOT NULL DEFAULT 'global',
    kind VARCHAR(50) NOT NULL,
    priority INTEGER DEFAULT 0,
    conditions JSONB DEFAULT '[]',
    actions JSONB DEFAULT '[]',
    enabled BOOLEAN DEFAULT true,
    is_default BOOLEAN DEFAULT false,
    created_at TIMESTAMP WITH TIME ZONE DEFAULT NOW(),
    updated_at TIMESTAMP WITH TIME ZONE DEFAULT NOW(),
    CONSTRAINT push_rule_user_rule_unique UNIQUE(user_id, scope, kind, rule_id),
    CONSTRAINT push_rule_scope_check CHECK (scope IN ('global', 'device')),
    CONSTRAINT push_rule_kind_check CHECK (kind IN ('override', 'content', 'room', 'sender', 'underride'))
);

CREATE INDEX IF NOT EXISTS idx_push_rule_user ON push_rule(user_id);
CREATE INDEX IF NOT EXISTS idx_push_rule_scope ON push_rule(scope);
CREATE INDEX IF NOT EXISTS idx_push_rule_kind ON push_rule(kind);

CREATE TABLE IF NOT EXISTS push_notification_queue (
    id SERIAL PRIMARY KEY,
    user_id VARCHAR(255) NOT NULL,
    device_id VARCHAR(255) NOT NULL,
    event_id VARCHAR(255),
    room_id VARCHAR(255),
    notification_type VARCHAR(50),
    content JSONB NOT NULL,
    priority INTEGER DEFAULT 5,
    status VARCHAR(20) DEFAULT 'pending',
    attempts INTEGER DEFAULT 0,
    max_attempts INTEGER DEFAULT 5,
    next_attempt_at TIMESTAMP WITH TIME ZONE DEFAULT NOW(),
    created_at TIMESTAMP WITH TIME ZONE DEFAULT NOW(),
    sent_at TIMESTAMP WITH TIME ZONE,
    error_message TEXT,
    CONSTRAINT push_notification_queue_status_check CHECK (status IN ('pending', 'processing', 'sent', 'failed', 'cancelled'))
);

CREATE INDEX IF NOT EXISTS idx_push_notification_queue_user ON push_notification_queue(user_id);
CREATE INDEX IF NOT EXISTS idx_push_notification_queue_status ON push_notification_queue(status);
CREATE INDEX IF NOT EXISTS idx_push_notification_queue_next_attempt ON push_notification_queue(next_attempt_at);

CREATE TABLE IF NOT EXISTS push_notification_log (
    id SERIAL PRIMARY KEY,
    user_id VARCHAR(255) NOT NULL,
    device_id VARCHAR(255) NOT NULL,
    event_id VARCHAR(255),
    room_id VARCHAR(255),
    notification_type VARCHAR(50),
    push_type VARCHAR(20),
    sent_at TIMESTAMP WITH TIME ZONE DEFAULT NOW(),
    success BOOLEAN DEFAULT true,
    error_message TEXT,
    provider_response TEXT,
    response_time_ms INTEGER,
    metadata JSONB DEFAULT '{}',
    CONSTRAINT push_notification_log_push_type_check CHECK (push_type IN ('fcm', 'apns', 'webpush', 'upstream'))
);

CREATE INDEX IF NOT EXISTS idx_push_notification_log_user ON push_notification_log(user_id);
CREATE INDEX IF NOT EXISTS idx_push_notification_log_sent_at ON push_notification_log(sent_at);

CREATE TABLE IF NOT EXISTS push_config (
    id SERIAL PRIMARY KEY,
    config_key VARCHAR(100) NOT NULL UNIQUE,
    config_value TEXT NOT NULL,
    description TEXT,
    created_at TIMESTAMP WITH TIME ZONE DEFAULT NOW(),
    updated_at TIMESTAMP WITH TIME ZONE DEFAULT NOW()
);

CREATE INDEX IF NOT EXISTS idx_push_config_key ON push_config(config_key);

CREATE TABLE IF NOT EXISTS push_stats (
    id SERIAL PRIMARY KEY,
    user_id VARCHAR(255) NOT NULL,
    date DATE NOT NULL,
    total_sent INTEGER DEFAULT 0,
    total_failed INTEGER DEFAULT 0,
    fcm_sent INTEGER DEFAULT 0,
    fcm_failed INTEGER DEFAULT 0,
    apns_sent INTEGER DEFAULT 0,
    apns_failed INTEGER DEFAULT 0,
    webpush_sent INTEGER DEFAULT 0,
    webpush_failed INTEGER DEFAULT 0,
    avg_response_time_ms INTEGER DEFAULT 0,
    created_at TIMESTAMP WITH TIME ZONE DEFAULT NOW(),
    updated_at TIMESTAMP WITH TIME ZONE DEFAULT NOW(),
    CONSTRAINT push_stats_user_date_unique UNIQUE(user_id, date)
);

CREATE INDEX IF NOT EXISTS idx_push_stats_user ON push_stats(user_id);
CREATE INDEX IF NOT EXISTS idx_push_stats_date ON push_stats(date);

-- 插入默认推送配置
INSERT INTO push_config (config_key, config_value, description)
VALUES 
    ('fcm.enabled', 'false', 'Enable FCM push notifications'),
    ('apns.enabled', 'false', 'Enable APNS push notifications'),
    ('webpush.enabled', 'false', 'Enable WebPush notifications'),
    ('push.rate_limit_per_minute', '60', 'Rate limit per user per minute'),
    ('push.batch_size', '100', 'Batch size for push processing')
ON CONFLICT (config_key) DO NOTHING;

-- 插入默认推送规则到 push_rule 表
INSERT INTO push_rule (user_id, rule_id, scope, kind, priority, conditions, actions, enabled, is_default)
VALUES 
    ('.default', 'm.rule.master', 'global', 'override', 0, '[]', '["dont_notify"]', true, true),
    ('.default', 'm.rule.suppress_notice', 'global', 'override', 1, '[{"kind": "event_match", "key": "content.msgtype", "pattern": "m.notice"}]', '["dont_notify"]', true, true),
    ('.default', 'm.rule.invite_for_me', 'global', 'override', 2, '[{"kind": "event_match", "key": "type", "pattern": "m.room.member"}, {"kind": "event_match", "key": "content.membership", "pattern": "invite"}, {"kind": "event_match", "key": "state_key", "pattern": "_self"}]', '["notify", {"set_tweak": "sound", "value": "default"}]', true, true),
    ('.default', 'm.rule.member_event', 'global', 'override', 3, '[{"kind": "event_match", "key": "type", "pattern": "m.room.member"}]', '["dont_notify"]', true, true),
    ('.default', 'm.rule.contains_display_name', 'global', 'content', 4, '[{"kind": "contains_display_name"}]', '["notify", {"set_tweak": "sound", "value": "default"}, {"set_tweak": "highlight", "value": true}]', true, true),
    ('.default', 'm.rule.tombstone', 'global', 'override', 5, '[{"kind": "event_match", "key": "type", "pattern": "m.room.tombstone"}, {"kind": "event_match", "key": "state_key", "pattern": ""}]', '["notify", {"set_tweak": "highlight", "value": true}]', true, true),
    ('.default', 'm.rule.room_notif', 'global', 'content', 6, '[{"kind": "event_match", "key": "content.body", "pattern": "@room"}]', '["notify", {"set_tweak": "highlight", "value": true}]', true, true),
    ('.default', 'm.rule.message', 'global', 'underride', 7, '[{"kind": "event_match", "key": "type", "pattern": "m.room.message"}]', '["notify", {"set_tweak": "sound", "value": "default"}]', true, true),
    ('.default', 'm.rule.encrypted', 'global', 'underride', 8, '[{"kind": "event_match", "key": "type", "pattern": "m.room.encrypted"}]', '["notify", {"set_tweak": "sound", "value": "default"}]', true, true)
ON CONFLICT (user_id, scope, kind, rule_id) DO NOTHING;

-- =============================================================================
-- 确保联邦黑名单表存在
-- =============================================================================

CREATE TABLE IF NOT EXISTS federation_blacklist (
    id SERIAL PRIMARY KEY,
    server_name VARCHAR(255) NOT NULL UNIQUE,
    block_type VARCHAR(50) NOT NULL,
    reason TEXT,
    blocked_by VARCHAR(255),
    created_at TIMESTAMP WITH TIME ZONE DEFAULT NOW(),
    expires_at TIMESTAMP WITH TIME ZONE,
    is_active BOOLEAN DEFAULT true,
    metadata JSONB DEFAULT '{}',
    CONSTRAINT federation_blacklist_type_check CHECK (block_type IN ('blacklist', 'whitelist', 'quarantine'))
);

CREATE INDEX IF NOT EXISTS idx_federation_blacklist_server ON federation_blacklist(server_name);
CREATE INDEX IF NOT EXISTS idx_federation_blacklist_active ON federation_blacklist(is_active);

CREATE TABLE IF NOT EXISTS federation_blacklist_rule (
    id SERIAL PRIMARY KEY,
    rule_name VARCHAR(255) NOT NULL,
    rule_type VARCHAR(50) NOT NULL,
    pattern VARCHAR(512) NOT NULL,
    action VARCHAR(50) NOT NULL,
    priority INTEGER DEFAULT 100,
    description TEXT,
    enabled BOOLEAN DEFAULT true,
    created_at TIMESTAMP WITH TIME ZONE DEFAULT NOW(),
    updated_at TIMESTAMP WITH TIME ZONE DEFAULT NOW(),
    CONSTRAINT federation_blacklist_rule_name_unique UNIQUE(rule_name)
);

CREATE INDEX IF NOT EXISTS idx_federation_blacklist_rule_type ON federation_blacklist_rule(rule_type);
CREATE INDEX IF NOT EXISTS idx_federation_blacklist_rule_enabled ON federation_blacklist_rule(enabled);

CREATE TABLE IF NOT EXISTS federation_blacklist_log (
    id SERIAL PRIMARY KEY,
    server_name VARCHAR(255) NOT NULL,
    action VARCHAR(50) NOT NULL,
    old_status VARCHAR(50),
    new_status VARCHAR(50),
    reason TEXT,
    performed_by VARCHAR(255),
    ip_address VARCHAR(45),
    user_agent TEXT,
    created_at TIMESTAMP WITH TIME ZONE DEFAULT NOW(),
    metadata JSONB DEFAULT '{}'
);

CREATE INDEX IF NOT EXISTS idx_federation_blacklist_log_server ON federation_blacklist_log(server_name);
CREATE INDEX IF NOT EXISTS idx_federation_blacklist_log_created ON federation_blacklist_log(created_at);

CREATE TABLE IF NOT EXISTS federation_blacklist_config (
    id SERIAL PRIMARY KEY,
    config_key VARCHAR(100) NOT NULL UNIQUE,
    config_value TEXT NOT NULL,
    description TEXT,
    created_at TIMESTAMP WITH TIME ZONE DEFAULT NOW(),
    updated_at TIMESTAMP WITH TIME ZONE DEFAULT NOW()
);

CREATE INDEX IF NOT EXISTS idx_federation_blacklist_config_key ON federation_blacklist_config(config_key);

-- 插入默认联邦黑名单规则
INSERT INTO federation_blacklist_rule (rule_name, rule_type, pattern, action, priority, description, enabled)
VALUES 
    ('block_malicious_servers', 'domain', 'malicious.example.com', 'block', 1000, 'Block known malicious server', true),
    ('block_spam_servers', 'regex', '.*spam\\..*', 'block', 900, 'Block spam servers', true),
    ('quarantine_new_servers', 'wildcard', '*.new', 'quarantine', 100, 'Quarantine new servers for review', true)
ON CONFLICT (rule_name) DO NOTHING;

-- =============================================================================
-- 确保认证相关表存在
-- =============================================================================

-- SAML 表
CREATE TABLE IF NOT EXISTS saml_user_mapping (
    id SERIAL PRIMARY KEY,
    name_id VARCHAR(255) NOT NULL UNIQUE,
    user_id VARCHAR(255) NOT NULL,
    idp_id VARCHAR(255),
    attributes JSONB DEFAULT '{}',
    created_at TIMESTAMP WITH TIME ZONE DEFAULT NOW(),
    updated_at TIMESTAMP WITH TIME ZONE DEFAULT NOW()
);

CREATE INDEX IF NOT EXISTS idx_saml_user_mapping_user ON saml_user_mapping(user_id);
CREATE INDEX IF NOT EXISTS idx_saml_user_mapping_idp ON saml_user_mapping(idp_id);

CREATE TABLE IF NOT EXISTS saml_sessions (
    id SERIAL PRIMARY KEY,
    session_id VARCHAR(255) NOT NULL UNIQUE,
    name_id VARCHAR(255),
    user_id VARCHAR(255),
    idp_id VARCHAR(255),
    request_id VARCHAR(255),
    created_at TIMESTAMP WITH TIME ZONE DEFAULT NOW(),
    expires_at TIMESTAMP WITH TIME ZONE,
    status VARCHAR(50) DEFAULT 'active',
    metadata JSONB DEFAULT '{}'
);

CREATE INDEX IF NOT EXISTS idx_saml_sessions_session ON saml_sessions(session_id);
CREATE INDEX IF NOT EXISTS idx_saml_sessions_user ON saml_sessions(user_id);
CREATE INDEX IF NOT EXISTS idx_saml_sessions_expires ON saml_sessions(expires_at);

CREATE TABLE IF NOT EXISTS saml_identity_providers (
    id SERIAL PRIMARY KEY,
    idp_id VARCHAR(255) NOT NULL UNIQUE,
    entity_id VARCHAR(255) NOT NULL,
    metadata_url TEXT,
    metadata_xml TEXT,
    sso_url VARCHAR(512),
    slo_url VARCHAR(512),
    x509_cert TEXT,
    enabled BOOLEAN DEFAULT true,
    created_at TIMESTAMP WITH TIME ZONE DEFAULT NOW(),
    updated_at TIMESTAMP WITH TIME ZONE DEFAULT NOW()
);

CREATE INDEX IF NOT EXISTS idx_saml_identity_providers_idp ON saml_identity_providers(idp_id);

-- CAS 表
CREATE TABLE IF NOT EXISTS cas_tickets (
    id SERIAL PRIMARY KEY,
    ticket VARCHAR(255) NOT NULL UNIQUE,
    user_id VARCHAR(255) NOT NULL,
    service VARCHAR(512) NOT NULL,
    created_at TIMESTAMP WITH TIME ZONE DEFAULT NOW(),
    expires_at TIMESTAMP WITH TIME ZONE NOT NULL,
    consumed_at TIMESTAMP WITH TIME ZONE,
    status VARCHAR(50) DEFAULT 'valid',
    metadata JSONB DEFAULT '{}',
    CONSTRAINT cas_tickets_status_check CHECK (status IN ('valid', 'consumed', 'expired'))
);

CREATE INDEX IF NOT EXISTS idx_cas_tickets_ticket ON cas_tickets(ticket);
CREATE INDEX IF NOT EXISTS idx_cas_tickets_user ON cas_tickets(user_id);
CREATE INDEX IF NOT EXISTS idx_cas_tickets_expires ON cas_tickets(expires_at);

CREATE TABLE IF NOT EXISTS cas_services (
    id SERIAL PRIMARY KEY,
    service_id VARCHAR(255) NOT NULL UNIQUE,
    service_url VARCHAR(512) NOT NULL,
    name VARCHAR(255),
    description TEXT,
    enabled BOOLEAN DEFAULT true,
    allowed_attributes JSONB DEFAULT '[]',
    created_at TIMESTAMP WITH TIME ZONE DEFAULT NOW(),
    updated_at TIMESTAMP WITH TIME ZONE DEFAULT NOW()
);

CREATE INDEX IF NOT EXISTS idx_cas_services_service ON cas_services(service_id);

CREATE TABLE IF NOT EXISTS cas_user_attributes (
    id SERIAL PRIMARY KEY,
    user_id VARCHAR(255) NOT NULL,
    attribute_name VARCHAR(255) NOT NULL,
    attribute_value TEXT,
    created_at TIMESTAMP WITH TIME ZONE DEFAULT NOW(),
    updated_at TIMESTAMP WITH TIME ZONE DEFAULT NOW(),
    CONSTRAINT cas_user_attributes_unique UNIQUE(user_id, attribute_name)
);

CREATE INDEX IF NOT EXISTS idx_cas_user_attributes_user ON cas_user_attributes(user_id);

-- =============================================================================
-- 验证迁移结果
-- =============================================================================

DO $$
DECLARE
    table_count INTEGER;
BEGIN
    SELECT COUNT(*) INTO table_count
    FROM information_schema.tables 
    WHERE table_schema = 'public';
    
    RAISE NOTICE '==========================================';
    RAISE NOTICE 'Incremental migration completed!';
    RAISE NOTICE 'Total tables in public schema: %', table_count;
    RAISE NOTICE '==========================================';
END $$;
