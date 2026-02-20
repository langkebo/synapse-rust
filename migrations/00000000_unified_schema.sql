-- =============================================================================
-- Synapse-Rust 统一数据库迁移脚本
-- 版本: 1.0.0
-- 创建日期: 2026-02-20
-- 描述: 整合所有表结构，确保数据库架构完整性
-- 
-- 执行方式:
--   docker exec -i synapse-postgres psql -U synapse -d synapse_test < 00000000_unified_schema.sql
--
-- 回滚方式:
--   docker exec -i synapse-postgres psql -U synapse -d synapse_test < 00000000_unified_rollback.sql
-- =============================================================================

BEGIN;

-- 设置客户端编码
SET client_encoding = 'UTF8';
SET standard_conforming_strings = on;

-- 扩展安装
CREATE EXTENSION IF NOT EXISTS pgcrypto;
CREATE EXTENSION IF NOT EXISTS "uuid-ossp";

-- =============================================================================
-- 版本记录表
-- =============================================================================

CREATE TABLE IF NOT EXISTS schema_migrations (
    version VARCHAR(255) PRIMARY KEY,
    checksum VARCHAR(64),
    execution_time_ms BIGINT,
    success BOOLEAN NOT NULL DEFAULT TRUE,
    executed_at TIMESTAMP WITH TIME ZONE DEFAULT NOW(),
    error_message TEXT,
    description TEXT
);

CREATE TABLE IF NOT EXISTS db_metadata (
    key VARCHAR(255) PRIMARY KEY,
    value TEXT NOT NULL,
    created_ts BIGINT DEFAULT (EXTRACT(EPOCH FROM NOW()) * 1000)::BIGINT,
    updated_ts BIGINT
);

INSERT INTO schema_migrations (version, description, success)
VALUES ('1.0.0', 'Unified schema migration - all tables', TRUE)
ON CONFLICT (version) DO UPDATE SET success = TRUE, executed_at = NOW();

INSERT INTO db_metadata (key, value, created_ts, updated_ts)
VALUES ('schema_version', '1.0.0', (EXTRACT(EPOCH FROM NOW()) * 1000)::BIGINT, (EXTRACT(EPOCH FROM NOW()) * 1000)::BIGINT)
ON CONFLICT (key) DO UPDATE SET value = '1.0.0', updated_ts = (EXTRACT(EPOCH FROM NOW()) * 1000)::BIGINT;

-- =============================================================================
-- 第一部分: 核心用户和认证表
-- =============================================================================

-- 用户表
CREATE TABLE IF NOT EXISTS users (
    user_id VARCHAR(255) NOT NULL PRIMARY KEY,
    username VARCHAR(255) NOT NULL UNIQUE,
    password_hash TEXT,
    displayname VARCHAR(255),
    avatar_url VARCHAR(512),
    is_admin BOOLEAN DEFAULT FALSE,
    is_guest BOOLEAN DEFAULT FALSE,
    user_type VARCHAR(50),
    is_deactivated BOOLEAN DEFAULT FALSE,
    is_shadow_banned BOOLEAN DEFAULT FALSE,
    consent_version VARCHAR(255),
    appservice_id VARCHAR(255),
    creation_ts BIGINT NOT NULL DEFAULT (EXTRACT(EPOCH FROM NOW()) * 1000)::BIGINT,
    updated_ts BIGINT,
    generation BIGINT NOT NULL DEFAULT 0,
    invalid_update_ts BIGINT,
    migration_state VARCHAR(100),
    last_seen_ts BIGINT,
    last_seen_ip VARCHAR(45),
    failed_login_attempts INTEGER DEFAULT 0,
    locked_until_ts BIGINT,
    password_changed_ts BIGINT,
    must_change_password BOOLEAN DEFAULT FALSE,
    email VARCHAR(255),
    phone VARCHAR(50),
    threepids JSONB DEFAULT '[]'::JSONB,
    user_threepids JSONB DEFAULT '[]'::JSONB,
    consent_ts BIGINT
);

CREATE INDEX IF NOT EXISTS idx_users_username ON users(username);
CREATE INDEX IF NOT EXISTS idx_users_email ON users(email);
CREATE INDEX IF NOT EXISTS idx_users_creation_ts ON users(creation_ts);
CREATE INDEX IF NOT EXISTS idx_users_deactivated ON users(is_deactivated) WHERE is_deactivated = TRUE;

-- 设备表
CREATE TABLE IF NOT EXISTS devices (
    device_id VARCHAR(255) NOT NULL,
    user_id VARCHAR(255) NOT NULL,
    display_name VARCHAR(255),
    device_key JSONB,
    last_seen_ts BIGINT,
    last_seen_ip VARCHAR(255),
    created_ts BIGINT NOT NULL DEFAULT (EXTRACT(EPOCH FROM NOW()) * 1000)::BIGINT,
    first_seen_ts BIGINT NOT NULL DEFAULT (EXTRACT(EPOCH FROM NOW()) * 1000)::BIGINT,
    appservice_id VARCHAR(255),
    ignored_user_list TEXT,
    PRIMARY KEY (device_id, user_id),
    FOREIGN KEY (user_id) REFERENCES users(user_id) ON DELETE CASCADE
);

CREATE INDEX IF NOT EXISTS idx_devices_user ON devices(user_id);
CREATE INDEX IF NOT EXISTS idx_devices_last_seen ON devices(last_seen_ts DESC);

-- 访问令牌表
CREATE TABLE IF NOT EXISTS access_tokens (
    id BIGSERIAL PRIMARY KEY,
    token TEXT NOT NULL UNIQUE,
    user_id VARCHAR(255) NOT NULL,
    device_id VARCHAR(255),
    appservice_id VARCHAR(255),
    expires_ts BIGINT NOT NULL,
    created_ts BIGINT NOT NULL DEFAULT (EXTRACT(EPOCH FROM NOW()) * 1000)::BIGINT,
    last_used_ts BIGINT,
    user_agent TEXT,
    ip_address VARCHAR(45),
    is_valid BOOLEAN DEFAULT TRUE,
    revoked_ts BIGINT,
    FOREIGN KEY (user_id) REFERENCES users(user_id) ON DELETE CASCADE
);

CREATE INDEX IF NOT EXISTS idx_access_tokens_user ON access_tokens(user_id);
CREATE INDEX IF NOT EXISTS idx_access_tokens_expires ON access_tokens(expires_ts);
CREATE INDEX IF NOT EXISTS idx_access_tokens_token ON access_tokens(token);

-- 刷新令牌表
CREATE TABLE IF NOT EXISTS refresh_tokens (
    id BIGSERIAL PRIMARY KEY,
    token_hash VARCHAR(255) UNIQUE NOT NULL,
    user_id VARCHAR(255) NOT NULL REFERENCES users(user_id) ON DELETE CASCADE,
    device_id VARCHAR(255),
    access_token_id VARCHAR(255),
    scope VARCHAR(255),
    expires_at BIGINT,
    created_ts BIGINT NOT NULL DEFAULT (EXTRACT(EPOCH FROM NOW()) * 1000)::BIGINT,
    last_used_ts BIGINT,
    use_count INTEGER DEFAULT 0,
    is_revoked BOOLEAN DEFAULT FALSE,
    revoked_ts BIGINT,
    revoked_reason VARCHAR(255),
    client_info JSONB,
    ip_address VARCHAR(45),
    user_agent TEXT
);

CREATE INDEX IF NOT EXISTS idx_refresh_tokens_user ON refresh_tokens(user_id);
CREATE INDEX IF NOT EXISTS idx_refresh_tokens_hash ON refresh_tokens(token_hash);
CREATE INDEX IF NOT EXISTS idx_refresh_tokens_expires ON refresh_tokens(expires_at);

-- 令牌黑名单表
CREATE TABLE IF NOT EXISTS token_blacklist (
    id BIGSERIAL PRIMARY KEY,
    token_hash VARCHAR(255) NOT NULL UNIQUE,
    token_type VARCHAR(50) NOT NULL,
    user_id VARCHAR(255) NOT NULL,
    revoked_ts BIGINT NOT NULL DEFAULT (EXTRACT(EPOCH FROM NOW()) * 1000)::BIGINT,
    expires_at BIGINT,
    reason VARCHAR(255)
);

CREATE INDEX IF NOT EXISTS idx_token_blacklist_hash ON token_blacklist(token_hash);
CREATE INDEX IF NOT EXISTS idx_token_blacklist_user ON token_blacklist(user_id);

-- 刷新令牌家族表
CREATE TABLE IF NOT EXISTS refresh_token_families (
    id BIGSERIAL PRIMARY KEY,
    family_id VARCHAR(255) NOT NULL UNIQUE,
    user_id VARCHAR(255) NOT NULL REFERENCES users(user_id) ON DELETE CASCADE,
    device_id VARCHAR(255),
    created_ts BIGINT NOT NULL DEFAULT (EXTRACT(EPOCH FROM NOW()) * 1000)::BIGINT,
    last_refresh_ts BIGINT,
    refresh_count INTEGER DEFAULT 0,
    is_compromised BOOLEAN DEFAULT FALSE,
    compromised_ts BIGINT
);

CREATE INDEX IF NOT EXISTS idx_refresh_token_families_user ON refresh_token_families(user_id);
CREATE INDEX IF NOT EXISTS idx_refresh_token_families_family ON refresh_token_families(family_id);

-- 刷新令牌轮换表
CREATE TABLE IF NOT EXISTS refresh_token_rotations (
    id BIGSERIAL PRIMARY KEY,
    family_id VARCHAR(255) NOT NULL,
    old_token_hash VARCHAR(255),
    new_token_hash VARCHAR(255) NOT NULL,
    rotated_ts BIGINT NOT NULL DEFAULT (EXTRACT(EPOCH FROM NOW()) * 1000)::BIGINT,
    rotation_reason VARCHAR(255)
);

CREATE INDEX IF NOT EXISTS idx_refresh_token_rotations_family ON refresh_token_rotations(family_id);

-- 刷新令牌使用记录表
CREATE TABLE IF NOT EXISTS refresh_token_usage (
    id BIGSERIAL PRIMARY KEY,
    refresh_token_id BIGINT NOT NULL,
    user_id VARCHAR(255) NOT NULL,
    old_access_token_id VARCHAR(255),
    new_access_token_id VARCHAR(255) NOT NULL,
    used_ts BIGINT NOT NULL DEFAULT (EXTRACT(EPOCH FROM NOW()) * 1000)::BIGINT,
    ip_address VARCHAR(45),
    user_agent TEXT,
    success BOOLEAN DEFAULT TRUE,
    error_message TEXT
);

CREATE INDEX IF NOT EXISTS idx_refresh_token_usage_token ON refresh_token_usage(refresh_token_id);
CREATE INDEX IF NOT EXISTS idx_refresh_token_usage_user ON refresh_token_usage(user_id);

-- =============================================================================
-- 第二部分: 房间和成员表
-- =============================================================================

-- 房间表
CREATE TABLE IF NOT EXISTS rooms (
    room_id VARCHAR(255) NOT NULL PRIMARY KEY,
    creator VARCHAR(255),
    is_public BOOLEAN DEFAULT FALSE,
    room_version VARCHAR(50) DEFAULT '6',
    has_auth_chain_index BOOLEAN DEFAULT FALSE,
    create_event_id VARCHAR(255),
    join_rules VARCHAR(50) DEFAULT 'invite',
    join_rules_event_id VARCHAR(255),
    created_ts BIGINT,
    updated_ts BIGINT
);

CREATE INDEX IF NOT EXISTS idx_rooms_creator ON rooms(creator);
CREATE INDEX IF NOT EXISTS idx_rooms_is_public ON rooms(is_public);

-- 房间成员表
CREATE TABLE IF NOT EXISTS room_members (
    id BIGSERIAL PRIMARY KEY,
    room_id VARCHAR(255) NOT NULL,
    user_id VARCHAR(255) NOT NULL,
    membership VARCHAR(50) NOT NULL DEFAULT 'join',
    displayname VARCHAR(255),
    avatar_url VARCHAR(512),
    reason TEXT,
    inviter_id VARCHAR(255),
    event_id VARCHAR(255),
    created_ts BIGINT NOT NULL DEFAULT (EXTRACT(EPOCH FROM NOW()) * 1000)::BIGINT,
    updated_ts BIGINT,
    CONSTRAINT room_members_room_user_unique UNIQUE(room_id, user_id),
    CONSTRAINT room_members_membership_check CHECK (membership IN ('invite', 'join', 'knock', 'leave', 'ban'))
);

CREATE INDEX IF NOT EXISTS idx_room_members_room ON room_members(room_id);
CREATE INDEX IF NOT EXISTS idx_room_members_user ON room_members(user_id);
CREATE INDEX IF NOT EXISTS idx_room_members_membership ON room_members(membership);

-- 房间邀请表
CREATE TABLE IF NOT EXISTS room_invites (
    id BIGSERIAL PRIMARY KEY,
    invite_code VARCHAR(255) NOT NULL UNIQUE,
    room_id VARCHAR(255) NOT NULL REFERENCES rooms(room_id) ON DELETE CASCADE,
    inviter_user_id VARCHAR(255) NOT NULL REFERENCES users(user_id) ON DELETE CASCADE,
    invitee_email VARCHAR(255),
    invitee_user_id VARCHAR(255) REFERENCES users(user_id) ON DELETE SET NULL,
    is_used BOOLEAN DEFAULT FALSE,
    is_revoked BOOLEAN DEFAULT FALSE,
    expires_at BIGINT,
    created_ts BIGINT NOT NULL DEFAULT (EXTRACT(EPOCH FROM NOW()) * 1000)::BIGINT,
    used_ts BIGINT,
    revoked_ts BIGINT,
    revoked_reason TEXT
);

CREATE INDEX IF NOT EXISTS idx_room_invites_invite_code ON room_invites(invite_code);
CREATE INDEX IF NOT EXISTS idx_room_invites_room_id ON room_invites(room_id);
CREATE INDEX IF NOT EXISTS idx_room_invites_inviter ON room_invites(inviter_user_id);
CREATE INDEX IF NOT EXISTS idx_room_invites_invitee ON room_invites(invitee_user_id);
CREATE INDEX IF NOT EXISTS idx_room_invites_is_used ON room_invites(is_used);
CREATE INDEX IF NOT EXISTS idx_room_invites_expires_at ON room_invites(expires_at);

-- 封禁房间表
CREATE TABLE IF NOT EXISTS blocked_rooms (
    id BIGSERIAL PRIMARY KEY,
    room_id VARCHAR(255) UNIQUE NOT NULL,
    blocked_at BIGINT NOT NULL DEFAULT (EXTRACT(EPOCH FROM NOW()) * 1000)::BIGINT,
    blocked_by VARCHAR(255) REFERENCES users(user_id),
    reason TEXT
);

CREATE INDEX IF NOT EXISTS idx_blocked_rooms_room ON blocked_rooms(room_id);

-- =============================================================================
-- 第三部分: 事件表
-- =============================================================================

-- 事件表
CREATE TABLE IF NOT EXISTS events (
    event_id VARCHAR(255) NOT NULL PRIMARY KEY,
    room_id VARCHAR(255) NOT NULL,
    sender VARCHAR(255) NOT NULL,
    type VARCHAR(255),
    event_type VARCHAR(255),
    content JSONB,
    state_key VARCHAR(255),
    depth BIGINT,
    origin_server_ts BIGINT,
    received_ts BIGINT,
    sender_device VARCHAR(255),
    contains_url BOOLEAN DEFAULT FALSE,
    instance_name VARCHAR(255),
    is_processed BOOLEAN DEFAULT FALSE,
    is_outlier BOOLEAN DEFAULT FALSE,
    constraint_instance VARCHAR(255),
    topological_ordering BIGINT,
    stream_ordering BIGINT
);

ALTER TABLE events ADD COLUMN IF NOT EXISTS stream_ordering BIGINT;
ALTER TABLE events ADD COLUMN IF NOT EXISTS topological_ordering BIGINT;

CREATE INDEX IF NOT EXISTS idx_events_room ON events(room_id);
CREATE INDEX IF NOT EXISTS idx_events_sender ON events(sender);
CREATE INDEX IF NOT EXISTS idx_events_type ON events(event_type);
CREATE INDEX IF NOT EXISTS idx_events_stream ON events(stream_ordering);

-- =============================================================================
-- 第四部分: E2EE 加密密钥表
-- =============================================================================

-- 设备密钥表
CREATE TABLE IF NOT EXISTS device_keys (
    user_id VARCHAR(255) NOT NULL,
    device_id VARCHAR(255) NOT NULL,
    algorithm VARCHAR(255) NOT NULL,
    key_data TEXT NOT NULL,
    added_ts BIGINT NOT NULL,
    last_seen_ts BIGINT,
    is_verified BOOLEAN DEFAULT FALSE,
    PRIMARY KEY (user_id, device_id, algorithm)
);

CREATE INDEX IF NOT EXISTS idx_device_keys_user ON device_keys(user_id);

-- 跨设备签名密钥表
CREATE TABLE IF NOT EXISTS cross_signing_keys (
    user_id VARCHAR(255) NOT NULL,
    key_type VARCHAR(50) NOT NULL,
    key_data TEXT NOT NULL,
    added_ts BIGINT NOT NULL,
    PRIMARY KEY (user_id, key_type)
);

CREATE INDEX IF NOT EXISTS idx_cross_signing_keys_user ON cross_signing_keys(user_id);

-- 设备签名表
CREATE TABLE IF NOT EXISTS device_signatures (
    id BIGSERIAL PRIMARY KEY,
    user_id VARCHAR(255) NOT NULL,
    device_id VARCHAR(255) NOT NULL,
    target_user_id VARCHAR(255) NOT NULL,
    target_device_id VARCHAR(255) NOT NULL,
    algorithm VARCHAR(255) NOT NULL,
    signature TEXT NOT NULL,
    created_ts BIGINT NOT NULL,
    CONSTRAINT device_signatures_unique UNIQUE(user_id, device_id, target_user_id, target_device_id, algorithm)
);

CREATE INDEX IF NOT EXISTS idx_device_signatures_user ON device_signatures(user_id);
CREATE INDEX IF NOT EXISTS idx_device_signatures_target ON device_signatures(target_user_id, target_device_id);

-- =============================================================================
-- 第五部分: 联邦表
-- =============================================================================

-- 联邦签名密钥表
CREATE TABLE IF NOT EXISTS federation_signing_keys (
    id BIGSERIAL PRIMARY KEY,
    server_name VARCHAR(255) NOT NULL,
    key_id VARCHAR(255) NOT NULL,
    secret_key TEXT NOT NULL,
    public_key TEXT NOT NULL,
    created_at BIGINT NOT NULL DEFAULT (EXTRACT(EPOCH FROM NOW()) * 1000)::BIGINT,
    expires_at BIGINT NOT NULL,
    key_json JSONB,
    ts_added_ms BIGINT,
    ts_valid_until_ms BIGINT,
    UNIQUE(server_name, key_id)
);

CREATE INDEX IF NOT EXISTS idx_federation_signing_keys_server ON federation_signing_keys(server_name);
CREATE INDEX IF NOT EXISTS idx_federation_signing_keys_expires ON federation_signing_keys(expires_at);

-- 联邦黑名单表
CREATE TABLE IF NOT EXISTS federation_blacklist (
    id SERIAL PRIMARY KEY,
    server_name VARCHAR(255) NOT NULL UNIQUE,
    reason TEXT,
    blocked_by VARCHAR(255),
    blocked_at TIMESTAMP WITH TIME ZONE DEFAULT NOW(),
    expires_at TIMESTAMP WITH TIME ZONE,
    is_enabled BOOLEAN DEFAULT TRUE
);

CREATE INDEX IF NOT EXISTS idx_federation_blacklist_server ON federation_blacklist(server_name);
CREATE INDEX IF NOT EXISTS idx_federation_blacklist_enabled ON federation_blacklist(is_enabled);

-- 联邦黑名单规则表
CREATE TABLE IF NOT EXISTS federation_blacklist_rule (
    id SERIAL PRIMARY KEY,
    rule_name VARCHAR(100) NOT NULL UNIQUE,
    rule_type VARCHAR(20) NOT NULL,
    pattern VARCHAR(255) NOT NULL,
    action VARCHAR(20) NOT NULL DEFAULT 'block',
    priority INTEGER DEFAULT 0,
    description TEXT,
    is_enabled BOOLEAN DEFAULT TRUE,
    created_at TIMESTAMP WITH TIME ZONE DEFAULT NOW(),
    updated_at TIMESTAMP WITH TIME ZONE DEFAULT NOW(),
    CONSTRAINT federation_blacklist_rule_type_check CHECK (rule_type IN ('domain', 'regex', 'wildcard')),
    CONSTRAINT federation_blacklist_rule_action_check CHECK (action IN ('block', 'quarantine', 'allow'))
);

CREATE INDEX IF NOT EXISTS idx_federation_blacklist_rule_type ON federation_blacklist_rule(rule_type);
CREATE INDEX IF NOT EXISTS idx_federation_blacklist_rule_enabled ON federation_blacklist_rule(is_enabled);

-- 联邦黑名单日志表
CREATE TABLE IF NOT EXISTS federation_blacklist_log (
    id SERIAL PRIMARY KEY,
    server_name VARCHAR(255) NOT NULL,
    action VARCHAR(50) NOT NULL,
    rule_id INTEGER,
    actor VARCHAR(255),
    reason TEXT,
    created_at TIMESTAMP WITH TIME ZONE DEFAULT NOW()
);

CREATE INDEX IF NOT EXISTS idx_federation_blacklist_log_server ON federation_blacklist_log(server_name);
CREATE INDEX IF NOT EXISTS idx_federation_blacklist_log_created ON federation_blacklist_log(created_at);

-- 联邦黑名单配置表
CREATE TABLE IF NOT EXISTS federation_blacklist_config (
    id SERIAL PRIMARY KEY,
    config_key VARCHAR(100) NOT NULL UNIQUE,
    config_value TEXT NOT NULL,
    description TEXT,
    created_at TIMESTAMP WITH TIME ZONE DEFAULT NOW(),
    updated_at TIMESTAMP WITH TIME ZONE DEFAULT NOW()
);

CREATE INDEX IF NOT EXISTS idx_federation_blacklist_config_key ON federation_blacklist_config(config_key);

-- =============================================================================
-- 第六部分: 媒体和语音消息表
-- =============================================================================

-- 媒体仓库表
CREATE TABLE IF NOT EXISTS media_repository (
    media_id VARCHAR(255) NOT NULL PRIMARY KEY,
    media_origin VARCHAR(255) NOT NULL,
    media_type VARCHAR(255),
    media_length BIGINT,
    upload_name TEXT,
    created_ts BIGINT NOT NULL DEFAULT (EXTRACT(EPOCH FROM NOW()) * 1000)::BIGINT,
    last_access_ts BIGINT,
    is_quarantined BOOLEAN DEFAULT FALSE,
    is_safe_from_quarantine BOOLEAN DEFAULT FALSE,
    user_id VARCHAR(255),
    server_name VARCHAR(255)
);

CREATE INDEX IF NOT EXISTS idx_media_repository_origin ON media_repository(media_origin);
CREATE INDEX IF NOT EXISTS idx_media_repository_user ON media_repository(user_id);

-- 语音消息表
CREATE TABLE IF NOT EXISTS voice_messages (
    id BIGSERIAL PRIMARY KEY,
    event_id VARCHAR(255) NOT NULL UNIQUE,
    room_id VARCHAR(255) NOT NULL,
    user_id VARCHAR(255) NOT NULL,
    media_id VARCHAR(255),
    duration_ms INTEGER NOT NULL,
    waveform TEXT,
    mime_type VARCHAR(100),
    file_size BIGINT,
    transcription TEXT,
    encryption JSONB,
    is_processed BOOLEAN DEFAULT FALSE,
    processed_ts BIGINT,
    created_ts BIGINT NOT NULL,
    CONSTRAINT voice_messages_event_unique UNIQUE(event_id)
);

CREATE INDEX IF NOT EXISTS idx_voice_messages_room ON voice_messages(room_id);
CREATE INDEX IF NOT EXISTS idx_voice_messages_user ON voice_messages(user_id);
CREATE INDEX IF NOT EXISTS idx_voice_messages_processed ON voice_messages(is_processed);

-- =============================================================================
-- 第七部分: 推送通知表
-- =============================================================================

-- 推送设备表
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
    is_enabled BOOLEAN DEFAULT true,
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

-- 推送规则表
CREATE TABLE IF NOT EXISTS push_rule (
    id SERIAL PRIMARY KEY,
    user_id VARCHAR(255) NOT NULL,
    rule_id VARCHAR(255) NOT NULL,
    scope VARCHAR(50) NOT NULL DEFAULT 'global',
    kind VARCHAR(50) NOT NULL,
    priority INTEGER DEFAULT 0,
    conditions JSONB DEFAULT '[]',
    actions JSONB DEFAULT '[]',
    is_enabled BOOLEAN DEFAULT true,
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

-- 推送通知队列表
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

-- 推送通知日志表
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

-- 推送配置表
CREATE TABLE IF NOT EXISTS push_config (
    id SERIAL PRIMARY KEY,
    config_key VARCHAR(100) NOT NULL UNIQUE,
    config_value TEXT NOT NULL,
    description TEXT,
    created_at TIMESTAMP WITH TIME ZONE DEFAULT NOW(),
    updated_at TIMESTAMP WITH TIME ZONE DEFAULT NOW()
);

CREATE INDEX IF NOT EXISTS idx_push_config_key ON push_config(config_key);

-- 推送统计表
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

-- 推送器表 (兼容 Matrix 标准)
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
    is_enabled BOOLEAN DEFAULT true,
    created_ts BIGINT NOT NULL DEFAULT EXTRACT(EPOCH FROM NOW()) * 1000,
    last_updated_ts BIGINT,
    last_success_ts BIGINT,
    last_failure_ts BIGINT,
    failure_count INTEGER DEFAULT 0,
    CONSTRAINT pushers_user_pushkey_unique UNIQUE(user_id, pushkey)
);

CREATE INDEX IF NOT EXISTS idx_pushers_user ON pushers(user_id);
CREATE INDEX IF NOT EXISTS idx_pushers_kind ON pushers(kind);
CREATE INDEX IF NOT EXISTS idx_pushers_enabled ON pushers(is_enabled);

-- 推送规则表 (兼容 Matrix 标准)
CREATE TABLE IF NOT EXISTS push_rules (
    id BIGSERIAL PRIMARY KEY,
    user_id VARCHAR(255) NOT NULL,
    rule_id VARCHAR(255) NOT NULL,
    scope VARCHAR(50) NOT NULL DEFAULT 'global',
    kind VARCHAR(50) NOT NULL,
    priority INTEGER DEFAULT 0,
    conditions JSONB DEFAULT '[]',
    actions JSONB DEFAULT '[]',
    is_enabled BOOLEAN DEFAULT true,
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

-- =============================================================================
-- 第八部分: 验证码表
-- =============================================================================

-- 注册验证码表
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

-- 验证码发送日志表
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

-- 验证码频率限制表
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

-- 验证码模板表
CREATE TABLE IF NOT EXISTS captcha_template (
    id SERIAL PRIMARY KEY,
    template_name VARCHAR(100) NOT NULL UNIQUE,
    captcha_type VARCHAR(20) NOT NULL,
    subject VARCHAR(255),
    content TEXT NOT NULL,
    variables JSONB DEFAULT '[]',
    is_default BOOLEAN DEFAULT false,
    is_enabled BOOLEAN DEFAULT true,
    created_at TIMESTAMP WITH TIME ZONE DEFAULT NOW(),
    updated_at TIMESTAMP WITH TIME ZONE DEFAULT NOW(),
    CONSTRAINT captcha_template_captcha_type_check CHECK (captcha_type IN ('email', 'sms', 'image'))
);

CREATE INDEX IF NOT EXISTS idx_captcha_template_type ON captcha_template(captcha_type);

-- 验证码配置表
CREATE TABLE IF NOT EXISTS captcha_config (
    id SERIAL PRIMARY KEY,
    config_key VARCHAR(100) NOT NULL UNIQUE,
    config_value TEXT NOT NULL,
    description TEXT,
    created_at TIMESTAMP WITH TIME ZONE DEFAULT NOW(),
    updated_at TIMESTAMP WITH TIME ZONE DEFAULT NOW()
);

CREATE INDEX IF NOT EXISTS idx_captcha_config_key ON captcha_config(config_key);

-- =============================================================================
-- 第九部分: SAML 认证表
-- =============================================================================

-- SAML 用户映射表
CREATE TABLE IF NOT EXISTS saml_user_mapping (
    id SERIAL PRIMARY KEY,
    user_id VARCHAR(255) NOT NULL UNIQUE,
    name_id VARCHAR(255) NOT NULL,
    issuer VARCHAR(255) NOT NULL,
    attributes JSONB DEFAULT '{}',
    first_auth_at TIMESTAMP WITH TIME ZONE DEFAULT NOW(),
    last_auth_at TIMESTAMP WITH TIME ZONE DEFAULT NOW(),
    CONSTRAINT saml_user_mapping_name_issuer_unique UNIQUE(name_id, issuer)
);

CREATE INDEX IF NOT EXISTS idx_saml_user_mapping_user ON saml_user_mapping(user_id);
CREATE INDEX IF NOT EXISTS idx_saml_user_mapping_name_id ON saml_user_mapping(name_id);

-- SAML 会话表
CREATE TABLE IF NOT EXISTS saml_sessions (
    id SERIAL PRIMARY KEY,
    session_id VARCHAR(255) NOT NULL UNIQUE,
    user_id VARCHAR(255),
    name_id VARCHAR(255),
    issuer VARCHAR(255),
    created_at TIMESTAMP WITH TIME ZONE DEFAULT NOW(),
    expires_at TIMESTAMP WITH TIME ZONE NOT NULL,
    attributes JSONB DEFAULT '{}',
    status VARCHAR(50) DEFAULT 'active'
);

CREATE INDEX IF NOT EXISTS idx_saml_sessions_session ON saml_sessions(session_id);
CREATE INDEX IF NOT EXISTS idx_saml_sessions_user ON saml_sessions(user_id);
CREATE INDEX IF NOT EXISTS idx_saml_sessions_expires ON saml_sessions(expires_at);

-- SAML 身份提供商表
CREATE TABLE IF NOT EXISTS saml_identity_providers (
    id SERIAL PRIMARY KEY,
    entity_id VARCHAR(255) NOT NULL UNIQUE,
    name VARCHAR(255) NOT NULL,
    metadata_url TEXT,
    metadata_xml TEXT,
    sso_url TEXT,
    slo_url TEXT,
    x509cert TEXT,
    is_enabled BOOLEAN DEFAULT true,
    created_at TIMESTAMP WITH TIME ZONE DEFAULT NOW(),
    updated_at TIMESTAMP WITH TIME ZONE DEFAULT NOW()
);

CREATE INDEX IF NOT EXISTS idx_saml_identity_providers_entity ON saml_identity_providers(entity_id);

-- =============================================================================
-- 第十部分: CAS 认证表
-- =============================================================================

-- CAS 票据表
CREATE TABLE IF NOT EXISTS cas_tickets (
    id SERIAL PRIMARY KEY,
    ticket VARCHAR(255) NOT NULL UNIQUE,
    service VARCHAR(255) NOT NULL,
    user_id VARCHAR(255) NOT NULL,
    created_at TIMESTAMP WITH TIME ZONE DEFAULT NOW(),
    expires_at TIMESTAMP WITH TIME ZONE NOT NULL,
    used_at TIMESTAMP WITH TIME ZONE,
    status VARCHAR(50) DEFAULT 'pending'
);

CREATE INDEX IF NOT EXISTS idx_cas_tickets_ticket ON cas_tickets(ticket);
CREATE INDEX IF NOT EXISTS idx_cas_tickets_user ON cas_tickets(user_id);
CREATE INDEX IF NOT EXISTS idx_cas_tickets_expires ON cas_tickets(expires_at);

-- CAS 服务表
CREATE TABLE IF NOT EXISTS cas_services (
    id SERIAL PRIMARY KEY,
    service_id VARCHAR(255) NOT NULL UNIQUE,
    name VARCHAR(255) NOT NULL,
    service_url VARCHAR(255) NOT NULL,
    description TEXT,
    is_enabled BOOLEAN DEFAULT true,
    created_at TIMESTAMP WITH TIME ZONE DEFAULT NOW(),
    updated_at TIMESTAMP WITH TIME ZONE DEFAULT NOW()
);

CREATE INDEX IF NOT EXISTS idx_cas_services_service ON cas_services(service_id);

-- CAS 用户属性表
CREATE TABLE IF NOT EXISTS cas_user_attributes (
    id SERIAL PRIMARY KEY,
    user_id VARCHAR(255) NOT NULL,
    attribute_name VARCHAR(100) NOT NULL,
    attribute_value TEXT,
    created_at TIMESTAMP WITH TIME ZONE DEFAULT NOW(),
    CONSTRAINT cas_user_attributes_unique UNIQUE(user_id, attribute_name)
);

CREATE INDEX IF NOT EXISTS idx_cas_user_attributes_user ON cas_user_attributes(user_id);

-- =============================================================================
-- 第十一部分: 模块表
-- =============================================================================

-- 模块表
CREATE TABLE IF NOT EXISTS modules (
    id SERIAL PRIMARY KEY,
    module_name VARCHAR(100) NOT NULL UNIQUE,
    module_type VARCHAR(50) NOT NULL,
    is_enabled BOOLEAN DEFAULT true,
    config JSONB DEFAULT '{}',
    priority INTEGER DEFAULT 0,
    description TEXT,
    created_at TIMESTAMP WITH TIME ZONE DEFAULT NOW(),
    updated_at TIMESTAMP WITH TIME ZONE DEFAULT NOW()
);

CREATE INDEX IF NOT EXISTS idx_modules_name ON modules(module_name);
CREATE INDEX IF NOT EXISTS idx_modules_type ON modules(module_type);

-- 模块执行日志表
CREATE TABLE IF NOT EXISTS module_execution_logs (
    id SERIAL PRIMARY KEY,
    module_name VARCHAR(100) NOT NULL,
    event_type VARCHAR(100),
    input_data JSONB,
    output_data JSONB,
    success BOOLEAN DEFAULT true,
    error_message TEXT,
    execution_time_ms INTEGER,
    created_at TIMESTAMP WITH TIME ZONE DEFAULT NOW()
);

CREATE INDEX IF NOT EXISTS idx_module_execution_logs_module ON module_execution_logs(module_name);
CREATE INDEX IF NOT EXISTS idx_module_execution_logs_created ON module_execution_logs(created_at);

-- =============================================================================
-- 第十二部分: 注册令牌表
-- =============================================================================

-- 注册令牌表
CREATE TABLE IF NOT EXISTS registration_tokens (
    id BIGSERIAL PRIMARY KEY,
    token VARCHAR(255) NOT NULL UNIQUE,
    description TEXT,
    max_uses INTEGER DEFAULT 0,
    current_uses INTEGER DEFAULT 0,
    expires_at BIGINT,
    created_ts BIGINT NOT NULL DEFAULT (EXTRACT(EPOCH FROM NOW()) * 1000)::BIGINT,
    created_by VARCHAR(255) REFERENCES users(user_id),
    is_enabled BOOLEAN DEFAULT TRUE,
    allowed_email_domains TEXT[],
    auto_join_rooms TEXT[]
);

CREATE INDEX IF NOT EXISTS idx_registration_tokens_token ON registration_tokens(token);
CREATE INDEX IF NOT EXISTS idx_registration_tokens_enabled ON registration_tokens(is_enabled);
CREATE INDEX IF NOT EXISTS idx_registration_tokens_expires ON registration_tokens(expires_at);

-- 注册令牌使用记录表
CREATE TABLE IF NOT EXISTS registration_token_usage (
    id BIGSERIAL PRIMARY KEY,
    token_id BIGINT NOT NULL REFERENCES registration_tokens(id) ON DELETE CASCADE,
    user_id VARCHAR(255) NOT NULL REFERENCES users(user_id) ON DELETE CASCADE,
    used_ts BIGINT NOT NULL DEFAULT (EXTRACT(EPOCH FROM NOW()) * 1000)::BIGINT,
    ip_address VARCHAR(45),
    user_agent TEXT
);

CREATE INDEX IF NOT EXISTS idx_registration_token_usage_token_id ON registration_token_usage(token_id);
CREATE INDEX IF NOT EXISTS idx_registration_token_usage_user_id ON registration_token_usage(user_id);
CREATE INDEX IF NOT EXISTS idx_registration_token_usage_used_ts ON registration_token_usage(used_ts DESC);

-- 注册令牌批量创建表
CREATE TABLE IF NOT EXISTS registration_token_batches (
    id BIGSERIAL PRIMARY KEY,
    batch_id VARCHAR(255) NOT NULL UNIQUE,
    description TEXT,
    token_count INTEGER NOT NULL,
    tokens_used INTEGER DEFAULT 0,
    created_by VARCHAR(255) REFERENCES users(user_id) ON DELETE SET NULL,
    created_ts BIGINT NOT NULL DEFAULT (EXTRACT(EPOCH FROM NOW()) * 1000)::BIGINT,
    expires_at BIGINT,
    is_enabled BOOLEAN DEFAULT TRUE,
    allowed_email_domains TEXT[],
    auto_join_rooms TEXT[]
);

CREATE INDEX IF NOT EXISTS idx_registration_token_batches_batch_id ON registration_token_batches(batch_id);
CREATE INDEX IF NOT EXISTS idx_registration_token_batches_created_by ON registration_token_batches(created_by);
CREATE INDEX IF NOT EXISTS idx_registration_token_batches_is_enabled ON registration_token_batches(is_enabled);

-- =============================================================================
-- 第十三部分: 事件报告表
-- =============================================================================

-- 事件报告表
CREATE TABLE IF NOT EXISTS event_reports (
    id BIGSERIAL PRIMARY KEY,
    event_id VARCHAR(255) NOT NULL,
    room_id VARCHAR(255) NOT NULL REFERENCES rooms(room_id) ON DELETE CASCADE,
    reporter_user_id VARCHAR(255) NOT NULL REFERENCES users(user_id) ON DELETE CASCADE,
    reported_user_id VARCHAR(255),
    event_json JSONB,
    reason TEXT,
    description TEXT,
    status VARCHAR(50) DEFAULT 'open',
    score INTEGER DEFAULT 0,
    received_ts BIGINT DEFAULT (EXTRACT(EPOCH FROM NOW()) * 1000)::BIGINT,
    resolved_ts BIGINT,
    resolved_by VARCHAR(255),
    resolution_reason TEXT
);

CREATE INDEX IF NOT EXISTS idx_event_reports_event ON event_reports(event_id);
CREATE INDEX IF NOT EXISTS idx_event_reports_room ON event_reports(room_id);
CREATE INDEX IF NOT EXISTS idx_event_reports_reporter ON event_reports(reporter_user_id);
CREATE INDEX IF NOT EXISTS idx_event_reports_status ON event_reports(status);

-- 事件报告历史表
CREATE TABLE IF NOT EXISTS event_report_history (
    id BIGSERIAL PRIMARY KEY,
    report_id BIGINT NOT NULL REFERENCES event_reports(id) ON DELETE CASCADE,
    action VARCHAR(100) NOT NULL,
    actor_user_id VARCHAR(255),
    actor_role VARCHAR(50),
    old_status VARCHAR(50),
    new_status VARCHAR(50),
    reason TEXT,
    created_ts BIGINT DEFAULT (EXTRACT(EPOCH FROM NOW()) * 1000)::BIGINT,
    metadata JSONB
);

CREATE INDEX IF NOT EXISTS idx_event_report_history_report ON event_report_history(report_id);

-- 举报速率限制表
CREATE TABLE IF NOT EXISTS report_rate_limits (
    id BIGSERIAL PRIMARY KEY,
    user_id VARCHAR(255) NOT NULL UNIQUE REFERENCES users(user_id) ON DELETE CASCADE,
    report_count INTEGER DEFAULT 0,
    last_report_ts BIGINT,
    blocked_until_ts BIGINT,
    is_blocked BOOLEAN DEFAULT FALSE,
    block_reason TEXT,
    created_ts BIGINT DEFAULT (EXTRACT(EPOCH FROM NOW()) * 1000)::BIGINT,
    updated_ts BIGINT DEFAULT (EXTRACT(EPOCH FROM NOW()) * 1000)::BIGINT
);

CREATE INDEX IF NOT EXISTS idx_report_rate_limits_user ON report_rate_limits(user_id);

-- 事件报告统计表
CREATE TABLE IF NOT EXISTS event_report_stats (
    id BIGSERIAL PRIMARY KEY,
    date DATE NOT NULL UNIQUE,
    total_reports INTEGER DEFAULT 0,
    open_reports INTEGER DEFAULT 0,
    resolved_reports INTEGER DEFAULT 0,
    dismissed_reports INTEGER DEFAULT 0,
    avg_resolution_time_hours INTEGER DEFAULT 0,
    created_ts BIGINT DEFAULT (EXTRACT(EPOCH FROM NOW()) * 1000)::BIGINT,
    updated_ts BIGINT DEFAULT (EXTRACT(EPOCH FROM NOW()) * 1000)::BIGINT
);

CREATE INDEX IF NOT EXISTS idx_event_report_stats_date ON event_report_stats(date);

-- =============================================================================
-- 第十四部分: 后台更新表
-- =============================================================================

-- 后台更新队列表
CREATE TABLE IF NOT EXISTS background_updates (
    update_name VARCHAR(255) NOT NULL PRIMARY KEY,
    depends_on VARCHAR(255),
    updated_ts BIGINT,
    total_duration_ms BIGINT
);

CREATE INDEX IF NOT EXISTS idx_background_updates_depends ON background_updates(depends_on);

-- =============================================================================
-- 第十五部分: 空间表
-- =============================================================================

-- 空间表
CREATE TABLE IF NOT EXISTS spaces (
    space_id VARCHAR(255) NOT NULL PRIMARY KEY,
    room_id VARCHAR(255) NOT NULL,
    name VARCHAR(255),
    topic TEXT,
    avatar_url VARCHAR(512),
    creator VARCHAR(255) NOT NULL,
    join_rule VARCHAR(50) DEFAULT 'invite',
    visibility VARCHAR(50) DEFAULT 'private',
    creation_ts BIGINT NOT NULL,
    updated_ts BIGINT,
    is_public BOOLEAN DEFAULT FALSE,
    parent_space_id VARCHAR(255)
);

CREATE INDEX IF NOT EXISTS idx_spaces_room ON spaces(room_id);
CREATE INDEX IF NOT EXISTS idx_spaces_creator ON spaces(creator);
CREATE INDEX IF NOT EXISTS idx_spaces_parent ON spaces(parent_space_id);

-- 空间子房间表
CREATE TABLE IF NOT EXISTS space_children (
    id BIGSERIAL PRIMARY KEY,
    space_id VARCHAR(255) NOT NULL,
    room_id VARCHAR(255) NOT NULL,
    via_servers JSONB DEFAULT '[]',
    "order" VARCHAR(255),
    is_suggested BOOLEAN DEFAULT FALSE,
    added_by VARCHAR(255) NOT NULL,
    added_ts BIGINT NOT NULL,
    removed_ts BIGINT,
    CONSTRAINT space_children_unique UNIQUE(space_id, room_id)
);

CREATE INDEX IF NOT EXISTS idx_space_children_space ON space_children(space_id);
CREATE INDEX IF NOT EXISTS idx_space_children_room ON space_children(room_id);

-- =============================================================================
-- 第十六部分: 线程表
-- =============================================================================

-- 线程根消息表
CREATE TABLE IF NOT EXISTS thread_roots (
    id BIGSERIAL PRIMARY KEY,
    room_id VARCHAR(255) NOT NULL,
    event_id VARCHAR(255) NOT NULL UNIQUE,
    thread_id VARCHAR(255) NOT NULL,
    creator VARCHAR(255) NOT NULL,
    created_ts BIGINT NOT NULL,
    last_reply_ts BIGINT,
    reply_count INTEGER DEFAULT 0,
    is_enabled BOOLEAN DEFAULT TRUE
);

CREATE INDEX IF NOT EXISTS idx_thread_roots_room ON thread_roots(room_id);
CREATE INDEX IF NOT EXISTS idx_thread_roots_thread ON thread_roots(thread_id);
CREATE INDEX IF NOT EXISTS idx_thread_roots_creator ON thread_roots(creator);

-- 线程回复表
CREATE TABLE IF NOT EXISTS thread_replies (
    id BIGSERIAL PRIMARY KEY,
    thread_id VARCHAR(255) NOT NULL,
    event_id VARCHAR(255) NOT NULL UNIQUE,
    room_id VARCHAR(255) NOT NULL,
    sender VARCHAR(255) NOT NULL,
    created_ts BIGINT NOT NULL
);

CREATE INDEX IF NOT EXISTS idx_thread_replies_thread ON thread_replies(thread_id);
CREATE INDEX IF NOT EXISTS idx_thread_replies_room ON thread_replies(room_id);

-- 线程订阅表
CREATE TABLE IF NOT EXISTS thread_subscriptions (
    id BIGSERIAL PRIMARY KEY,
    thread_id VARCHAR(255) NOT NULL,
    user_id VARCHAR(255) NOT NULL,
    subscribed_ts BIGINT NOT NULL,
    notify BOOLEAN DEFAULT TRUE,
    CONSTRAINT thread_subscriptions_unique UNIQUE(thread_id, user_id)
);

CREATE INDEX IF NOT EXISTS idx_thread_subscriptions_thread ON thread_subscriptions(thread_id);
CREATE INDEX IF NOT EXISTS idx_thread_subscriptions_user ON thread_subscriptions(user_id);

-- =============================================================================
-- 第十七部分: 数据保留表
-- =============================================================================

-- 数据保留策略表
CREATE TABLE IF NOT EXISTS retention_policies (
    id SERIAL PRIMARY KEY,
    room_id VARCHAR(255),
    max_lifetime BIGINT,
    min_lifetime BIGINT,
    is_server_default BOOLEAN DEFAULT FALSE,
    created_at TIMESTAMP WITH TIME ZONE DEFAULT NOW(),
    updated_at TIMESTAMP WITH TIME ZONE DEFAULT NOW()
);

CREATE INDEX IF NOT EXISTS idx_retention_policies_room ON retention_policies(room_id);

-- 数据保留清理日志表
CREATE TABLE IF NOT EXISTS retention_cleanup_logs (
    id SERIAL PRIMARY KEY,
    room_id VARCHAR(255),
    events_removed INTEGER DEFAULT 0,
    media_removed INTEGER DEFAULT 0,
    started_at TIMESTAMP WITH TIME ZONE,
    completed_at TIMESTAMP WITH TIME ZONE,
    status VARCHAR(50) DEFAULT 'pending'
);

CREATE INDEX IF NOT EXISTS idx_retention_cleanup_logs_room ON retention_cleanup_logs(room_id);
CREATE INDEX IF NOT EXISTS idx_retention_cleanup_logs_status ON retention_cleanup_logs(status);

-- =============================================================================
-- 第十八部分: 媒体配额表
-- =============================================================================

-- 用户媒体配额表
CREATE TABLE IF NOT EXISTS user_media_quota (
    id SERIAL PRIMARY KEY,
    user_id VARCHAR(255) NOT NULL UNIQUE,
    quota_bytes BIGINT DEFAULT 1073741824,
    used_bytes BIGINT DEFAULT 0,
    media_count INTEGER DEFAULT 0,
    created_at TIMESTAMP WITH TIME ZONE DEFAULT NOW(),
    updated_at TIMESTAMP WITH TIME ZONE DEFAULT NOW(),
    FOREIGN KEY (user_id) REFERENCES users(user_id) ON DELETE CASCADE
);

CREATE INDEX IF NOT EXISTS idx_user_media_quota_user ON user_media_quota(user_id);

-- 服务器媒体配额表
CREATE TABLE IF NOT EXISTS server_media_quota (
    id SERIAL PRIMARY KEY,
    server_name VARCHAR(255) NOT NULL UNIQUE,
    quota_bytes BIGINT,
    used_bytes BIGINT DEFAULT 0,
    created_at TIMESTAMP WITH TIME ZONE DEFAULT NOW(),
    updated_at TIMESTAMP WITH TIME ZONE DEFAULT NOW()
);

CREATE INDEX IF NOT EXISTS idx_server_media_quota_server ON server_media_quota(server_name);

-- 媒体配额告警表
CREATE TABLE IF NOT EXISTS media_quota_alerts (
    id SERIAL PRIMARY KEY,
    user_id VARCHAR(255),
    server_name VARCHAR(255),
    alert_type VARCHAR(50) NOT NULL,
    threshold_percent INTEGER,
    current_usage BIGINT,
    message TEXT,
    created_at TIMESTAMP WITH TIME ZONE DEFAULT NOW(),
    acknowledged BOOLEAN DEFAULT FALSE,
    acknowledged_at TIMESTAMP WITH TIME ZONE,
    acknowledged_by VARCHAR(255)
);

CREATE INDEX IF NOT EXISTS idx_media_quota_alerts_user ON media_quota_alerts(user_id);
CREATE INDEX IF NOT EXISTS idx_media_quota_alerts_server ON media_quota_alerts(server_name);

-- =============================================================================
-- 第十九部分: 服务器通知表
-- =============================================================================

-- 服务器通知表
CREATE TABLE IF NOT EXISTS server_notifications (
    id SERIAL PRIMARY KEY,
    title VARCHAR(255) NOT NULL,
    content TEXT NOT NULL,
    notification_type VARCHAR(50) DEFAULT 'info',
    priority INTEGER DEFAULT 0,
    target_audience VARCHAR(50) DEFAULT 'all',
    target_user_ids JSONB DEFAULT '[]',
    starts_at TIMESTAMP WITH TIME ZONE,
    expires_at TIMESTAMP WITH TIME ZONE,
    is_enabled BOOLEAN DEFAULT TRUE,
    is_dismissable BOOLEAN DEFAULT TRUE,
    action_url VARCHAR(512),
    action_text VARCHAR(255),
    created_by VARCHAR(255),
    created_at TIMESTAMP WITH TIME ZONE DEFAULT NOW(),
    updated_at TIMESTAMP WITH TIME ZONE DEFAULT NOW()
);

CREATE INDEX IF NOT EXISTS idx_server_notifications_enabled ON server_notifications(is_enabled);
CREATE INDEX IF NOT EXISTS idx_server_notifications_type ON server_notifications(notification_type);
CREATE INDEX IF NOT EXISTS idx_server_notifications_expires ON server_notifications(expires_at);

-- 计划通知表
CREATE TABLE IF NOT EXISTS scheduled_notifications (
    id SERIAL PRIMARY KEY,
    notification_id INTEGER REFERENCES server_notifications(id) ON DELETE CASCADE,
    scheduled_at TIMESTAMP WITH TIME ZONE NOT NULL,
    status VARCHAR(50) DEFAULT 'pending',
    sent_at TIMESTAMP WITH TIME ZONE,
    error_message TEXT,
    created_at TIMESTAMP WITH TIME ZONE DEFAULT NOW()
);

CREATE INDEX IF NOT EXISTS idx_scheduled_notifications_scheduled ON scheduled_notifications(scheduled_at);
CREATE INDEX IF NOT EXISTS idx_scheduled_notifications_status ON scheduled_notifications(status);

-- =============================================================================
-- 第二十部分: 应用服务表
-- =============================================================================

-- 应用服务表
CREATE TABLE IF NOT EXISTS application_services (
    id BIGSERIAL PRIMARY KEY,
    as_id VARCHAR(255) NOT NULL UNIQUE,
    url VARCHAR(255) NOT NULL,
    as_token VARCHAR(255) NOT NULL,
    hs_token VARCHAR(255) NOT NULL,
    sender VARCHAR(255) NOT NULL,
    name VARCHAR(255),
    description TEXT,
    rate_limited BOOLEAN DEFAULT FALSE,
    protocols JSONB DEFAULT '[]',
    namespaces JSONB DEFAULT '{}',
    created_ts BIGINT NOT NULL,
    updated_ts BIGINT,
    last_seen_ts BIGINT,
    is_enabled BOOLEAN DEFAULT TRUE
);

CREATE INDEX IF NOT EXISTS idx_application_services_id ON application_services(as_id);
CREATE INDEX IF NOT EXISTS idx_application_services_token ON application_services(as_token);
CREATE INDEX IF NOT EXISTS idx_application_services_enabled ON application_services(is_enabled);

-- =============================================================================
-- 第二十一部分: Worker 表
-- =============================================================================

-- Worker 表
CREATE TABLE IF NOT EXISTS workers (
    id BIGSERIAL PRIMARY KEY,
    worker_id VARCHAR(255) NOT NULL UNIQUE,
    worker_name VARCHAR(255) NOT NULL,
    worker_type VARCHAR(50) NOT NULL,
    host VARCHAR(255) NOT NULL,
    port INTEGER NOT NULL,
    status VARCHAR(50) DEFAULT 'starting',
    last_heartbeat_ts BIGINT,
    started_ts BIGINT NOT NULL,
    stopped_ts BIGINT,
    config JSONB DEFAULT '{}',
    metadata JSONB DEFAULT '{}',
    version VARCHAR(50)
);

CREATE INDEX IF NOT EXISTS idx_workers_id ON workers(worker_id);
CREATE INDEX IF NOT EXISTS idx_workers_type ON workers(worker_type);
CREATE INDEX IF NOT EXISTS idx_workers_status ON workers(status);

-- Worker 连接表
CREATE TABLE IF NOT EXISTS worker_connections (
    id BIGSERIAL PRIMARY KEY,
    source_worker_id VARCHAR(255) NOT NULL,
    target_worker_id VARCHAR(255) NOT NULL,
    connection_type VARCHAR(50) NOT NULL,
    established_ts BIGINT NOT NULL,
    last_activity_ts BIGINT,
    bytes_sent BIGINT DEFAULT 0,
    bytes_received BIGINT DEFAULT 0,
    messages_sent BIGINT DEFAULT 0,
    messages_received BIGINT DEFAULT 0,
    CONSTRAINT worker_connections_unique UNIQUE(source_worker_id, target_worker_id, connection_type)
);

CREATE INDEX IF NOT EXISTS idx_worker_connections_source ON worker_connections(source_worker_id);
CREATE INDEX IF NOT EXISTS idx_worker_connections_target ON worker_connections(target_worker_id);

-- Worker 健康检查表
CREATE TABLE IF NOT EXISTS worker_health_checks (
    id SERIAL PRIMARY KEY,
    worker_id VARCHAR(255) NOT NULL,
    check_type VARCHAR(50) NOT NULL,
    status VARCHAR(50) NOT NULL,
    details JSONB DEFAULT '{}',
    checked_at TIMESTAMP WITH TIME ZONE DEFAULT NOW()
);

CREATE INDEX IF NOT EXISTS idx_worker_health_checks_worker ON worker_health_checks(worker_id);
CREATE INDEX IF NOT EXISTS idx_worker_health_checks_checked ON worker_health_checks(checked_at);

-- =============================================================================
-- 第二十二部分: 安全和监控表
-- =============================================================================

-- IP 声誉表
CREATE TABLE IF NOT EXISTS ip_reputation (
    id SERIAL PRIMARY KEY,
    ip_address VARCHAR(45) NOT NULL UNIQUE,
    reputation_score INTEGER DEFAULT 100,
    failed_attempts INTEGER DEFAULT 0,
    successful_attempts INTEGER DEFAULT 0,
    last_failed_ts BIGINT,
    last_success_ts BIGINT,
    is_blocked BOOLEAN DEFAULT FALSE,
    blocked_ts BIGINT,
    blocked_until_ts BIGINT,
    block_reason TEXT,
    risk_level VARCHAR(20) DEFAULT 'none',
    created_ts BIGINT DEFAULT (EXTRACT(EPOCH FROM NOW()) * 1000)::BIGINT,
    updated_ts BIGINT
);

CREATE INDEX IF NOT EXISTS idx_ip_reputation_ip ON ip_reputation(ip_address);
CREATE INDEX IF NOT EXISTS idx_ip_reputation_blocked ON ip_reputation(is_blocked);

-- IP 封禁表
CREATE TABLE IF NOT EXISTS ip_blocks (
    id SERIAL PRIMARY KEY,
    ip_address VARCHAR(45) NOT NULL,
    cidr_range VARCHAR(50),
    ip_range VARCHAR(100) UNIQUE,
    reason TEXT,
    blocked_by VARCHAR(255),
    blocked_ts BIGINT DEFAULT (EXTRACT(EPOCH FROM NOW()) * 1000)::BIGINT,
    expires_ts BIGINT,
    is_enabled BOOLEAN DEFAULT TRUE
);

CREATE INDEX IF NOT EXISTS idx_ip_blocks_ip ON ip_blocks(ip_address);
CREATE INDEX IF NOT EXISTS idx_ip_blocks_enabled ON ip_blocks(is_enabled);
CREATE INDEX IF NOT EXISTS idx_ip_blocks_range ON ip_blocks(ip_range);

-- 安全事件表
CREATE TABLE IF NOT EXISTS security_events (
    id SERIAL PRIMARY KEY,
    event_type VARCHAR(100) NOT NULL,
    user_id VARCHAR(255),
    ip_address VARCHAR(45),
    user_agent TEXT,
    details JSONB DEFAULT '{}',
    description TEXT,
    severity VARCHAR(50) DEFAULT 'info',
    created_at TIMESTAMP WITH TIME ZONE DEFAULT NOW(),
    created_ts BIGINT DEFAULT (EXTRACT(EPOCH FROM NOW()) * 1000)::BIGINT
);

CREATE INDEX IF NOT EXISTS idx_security_events_type ON security_events(event_type);
CREATE INDEX IF NOT EXISTS idx_security_events_user ON security_events(user_id);
CREATE INDEX IF NOT EXISTS idx_security_events_created ON security_events(created_at);

-- =============================================================================
-- 第二十三部分: 邮件验证表
-- =============================================================================

-- 邮件验证令牌表
CREATE TABLE IF NOT EXISTS email_verification_tokens (
    id SERIAL PRIMARY KEY,
    user_id VARCHAR(255) NOT NULL,
    email VARCHAR(255) NOT NULL,
    token VARCHAR(255) NOT NULL UNIQUE,
    created_at TIMESTAMP WITH TIME ZONE DEFAULT NOW(),
    expires_at TIMESTAMP WITH TIME ZONE NOT NULL,
    verified_at TIMESTAMP WITH TIME ZONE,
    status VARCHAR(50) DEFAULT 'pending',
    ip_address VARCHAR(45),
    user_agent TEXT
);

CREATE INDEX IF NOT EXISTS idx_email_verification_tokens_user ON email_verification_tokens(user_id);
CREATE INDEX IF NOT EXISTS idx_email_verification_tokens_token ON email_verification_tokens(token);
CREATE INDEX IF NOT EXISTS idx_email_verification_tokens_status ON email_verification_tokens(status);

-- =============================================================================
-- 第二十四部分: 插入默认数据
-- =============================================================================

-- 插入默认验证码模板
INSERT INTO captcha_template (template_name, captcha_type, subject, content, variables, is_default, enabled)
VALUES 
    ('default_email', 'email', '您的注册验证码', '您的注册验证码是：{{code}}，有效期{{expiry_minutes}}分钟。如非本人操作，请忽略此邮件。', '["code", "expiry_minutes"]', true, true),
    ('default_sms', 'sms', NULL, '您的注册验证码：{{code}}，有效期{{expiry_minutes}}分钟。', '["code", "expiry_minutes"]', true, true)
ON CONFLICT (template_name) DO NOTHING;

-- 插入默认验证码配置
INSERT INTO captcha_config (config_key, config_value, description)
VALUES 
    ('email.code_length', '6', '邮箱验证码长度'),
    ('email.code_expiry_minutes', '10', '邮箱验证码有效期（分钟）'),
    ('email.max_attempts', '5', '邮箱验证码最大尝试次数'),
    ('sms.code_length', '6', '短信验证码长度'),
    ('sms.code_expiry_minutes', '5', '短信验证码有效期（分钟）'),
    ('global.block_duration_minutes', '30', '触发限制后的封禁时长（分钟）')
ON CONFLICT (config_key) DO NOTHING;

-- 插入默认推送配置
INSERT INTO push_config (config_key, config_value, description)
VALUES 
    ('fcm.enabled', 'false', 'Enable FCM push notifications'),
    ('apns.enabled', 'false', 'Enable APNS push notifications'),
    ('webpush.enabled', 'false', 'Enable WebPush notifications'),
    ('push.rate_limit_per_minute', '60', 'Rate limit per user per minute'),
    ('push.batch_size', '100', 'Batch size for push processing')
ON CONFLICT (config_key) DO NOTHING;

-- 插入默认推送规则
INSERT INTO push_rule (user_id, rule_id, scope, kind, priority, conditions, actions, enabled, is_default)
VALUES 
    ('.default', 'm.rule.master', 'global', 'override', 0, '[]', '["dont_notify"]', true, true),
    ('.default', 'm.rule.suppress_notices', 'global', 'override', 1, '[{"kind": "event_match", "key": "content.msgtype", "pattern": "m.notice"}]', '["dont_notify"]', true, true),
    ('.default', 'm.rule.invite_for_me', 'global', 'override', 2, '[{"kind": "event_match", "key": "type", "pattern": "m.room.member"}, {"kind": "event_match", "key": "content.membership", "pattern": "invite"}, {"kind": "event_match", "key": "state_key", "pattern": "_self"}]', '["notify", {"set_tweak": "sound", "value": "default"}]', true, true),
    ('.default', 'm.rule.member_event', 'global', 'override', 3, '[{"kind": "event_match", "key": "type", "pattern": "m.room.member"}]', '["dont_notify"]', true, true),
    ('.default', 'm.rule.contains_display_name', 'global', 'content', 4, '[{"kind": "contains_display_name"}]', '["notify", {"set_tweak": "sound", "value": "default"}, {"set_tweak": "highlight", "value": true}]', true, true),
    ('.default', 'm.rule.tombstone', 'global', 'override', 5, '[{"kind": "event_match", "key": "type", "pattern": "m.room.tombstone"}, {"kind": "event_match", "key": "state_key", "pattern": ""}]', '["notify", {"set_tweak": "highlight", "value": true}]', true, true),
    ('.default', 'm.rule.room_notif', 'global', 'content', 6, '[{"kind": "event_match", "key": "content.body", "pattern": "@room"}]', '["notify", {"set_tweak": "highlight", "value": true}]', true, true),
    ('.default', 'm.rule.message', 'global', 'underride', 7, '[{"kind": "event_match", "key": "type", "pattern": "m.room.message"}]', '["notify", {"set_tweak": "sound", "value": "default"}]', true, true),
    ('.default', 'm.rule.encrypted', 'global', 'underride', 8, '[{"kind": "event_match", "key": "type", "pattern": "m.room.encrypted"}]', '["notify", {"set_tweak": "sound", "value": "default"}]', true, true)
ON CONFLICT (user_id, scope, kind, rule_id) DO NOTHING;

-- 插入默认推送规则 (兼容表)
INSERT INTO push_rules (user_id, rule_id, scope, kind, priority, conditions, actions, enabled, is_default)
SELECT user_id, rule_id, scope, kind, priority, conditions, actions, enabled, is_default
FROM push_rule
WHERE user_id = '.default'
ON CONFLICT (user_id, scope, kind, rule_id) DO NOTHING;

-- 插入默认联邦黑名单规则
INSERT INTO federation_blacklist_rule (rule_name, rule_type, pattern, action, priority, description, enabled)
VALUES 
    ('block_malicious_servers', 'domain', 'malicious.example.com', 'block', 1000, 'Block known malicious server', true),
    ('block_spam_servers', 'regex', '.*spam\\..*', 'block', 900, 'Block spam servers', true),
    ('quarantine_new_servers', 'wildcard', '*.new', 'quarantine', 100, 'Quarantine new servers for review', true)
ON CONFLICT (rule_name) DO NOTHING;

-- =============================================================================
-- 第二十五部分: 验证迁移
-- =============================================================================

DO $$
DECLARE
    table_count INTEGER;
BEGIN
    -- 验证核心表存在
    SELECT COUNT(*) INTO table_count
    FROM information_schema.tables 
    WHERE table_schema = 'public'
    AND table_name IN ('users', 'devices', 'access_tokens', 'rooms', 'events');
    
    IF table_count < 5 THEN
        RAISE EXCEPTION 'Core tables migration failed';
    END IF;
    
    -- 验证推送表存在
    SELECT COUNT(*) INTO table_count
    FROM information_schema.tables 
    WHERE table_schema = 'public'
    AND table_name IN ('push_device', 'push_rule', 'push_notification_queue', 'pushers', 'push_rules');
    
    IF table_count < 5 THEN
        RAISE EXCEPTION 'Push notification tables migration failed';
    END IF;
    
    -- 验证验证码表存在
    SELECT COUNT(*) INTO table_count
    FROM information_schema.tables 
    WHERE table_schema = 'public'
    AND table_name LIKE 'captcha%' OR table_name LIKE 'registration_captcha';
    
    IF table_count < 4 THEN
        RAISE EXCEPTION 'Captcha tables migration failed';
    END IF;
    
    -- 验证认证表存在
    SELECT COUNT(*) INTO table_count
    FROM information_schema.tables 
    WHERE table_schema = 'public'
    AND (table_name LIKE 'saml_%' OR table_name LIKE 'cas_%');
    
    IF table_count < 6 THEN
        RAISE EXCEPTION 'Authentication tables migration failed';
    END IF;
    
    -- 验证联邦黑名单表存在
    SELECT COUNT(*) INTO table_count
    FROM information_schema.tables 
    WHERE table_schema = 'public'
    AND table_name LIKE 'federation_blacklist%';
    
    IF table_count < 3 THEN
        RAISE EXCEPTION 'Federation blacklist tables migration failed';
    END IF;
    
    -- 验证缺失的表
    SELECT COUNT(*) INTO table_count
    FROM information_schema.tables 
    WHERE table_schema = 'public'
    AND table_name IN ('federation_signing_keys', 'blocked_rooms', 'room_invites', 'report_rate_limits');
    
    IF table_count < 4 THEN
        RAISE EXCEPTION 'Additional tables migration failed';
    END IF;
    
    RAISE NOTICE '==========================================';
    RAISE NOTICE 'All migrations completed successfully!';
    RAISE NOTICE 'Total tables verified: %', (SELECT COUNT(*) FROM information_schema.tables WHERE table_schema = 'public');
    RAISE NOTICE '==========================================';
END $$;

COMMIT;

-- =============================================================================
-- 迁移完成
-- =============================================================================
