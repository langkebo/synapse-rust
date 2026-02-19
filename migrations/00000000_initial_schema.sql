-- Synapse Rust 数据库统一初始化脚本
-- 版本: 1.0.0
-- 创建日期: 2026-02-19
-- 说明: 统一数据库结构，清理冗余字段，确保字段命名规范

-- 设置客户端编码
SET client_encoding = 'UTF8';
SET standard_conforming_strings = on;

--------------------------------------------------------------------------------
-- 扩展安装
--------------------------------------------------------------------------------
CREATE EXTENSION IF NOT EXISTS pgcrypto;
CREATE EXTENSION IF NOT EXISTS "uuid-ossp";

--------------------------------------------------------------------------------
-- 核心用户表
--------------------------------------------------------------------------------
CREATE TABLE IF NOT EXISTS users (
    user_id VARCHAR(255) PRIMARY KEY,
    username VARCHAR(255) UNIQUE NOT NULL,
    password_hash TEXT NOT NULL,
    displayname VARCHAR(255),
    avatar_url TEXT,
    is_admin BOOLEAN DEFAULT FALSE,
    is_guest BOOLEAN DEFAULT FALSE,
    user_type VARCHAR(50),
    deactivated BOOLEAN DEFAULT FALSE,
    creation_ts BIGINT NOT NULL DEFAULT (EXTRACT(EPOCH FROM NOW()) * 1000)::BIGINT,
    updated_ts BIGINT,
    generation BIGINT DEFAULT 1,
    admin SMALLINT DEFAULT 0,
    upgrade_ts BIGINT,
    locked BOOLEAN DEFAULT FALSE,
    locked_reason TEXT,
    last_seen_ts BIGINT,
    last_seen_ip VARCHAR(45),
    failed_login_attempts INTEGER DEFAULT 0,
    locked_until_ts BIGINT,
    password_changed_ts BIGINT,
    must_change_password BOOLEAN DEFAULT FALSE,
    email VARCHAR(255),
    phone VARCHAR(50),
    threepids JSONB DEFAULT '[]'::JSONB,
    consent_version VARCHAR(50),
    consent_ts BIGINT,
    user_threepids JSONB DEFAULT '[]'::JSONB
);

CREATE INDEX IF NOT EXISTS idx_users_username ON users(username);
CREATE INDEX IF NOT EXISTS idx_users_email ON users(email);
CREATE INDEX IF NOT EXISTS idx_users_creation_ts ON users(creation_ts);

--------------------------------------------------------------------------------
-- 设备表
--------------------------------------------------------------------------------
CREATE TABLE IF NOT EXISTS devices (
    device_id VARCHAR(255) NOT NULL,
    user_id VARCHAR(255) NOT NULL REFERENCES users(user_id) ON DELETE CASCADE,
    display_name VARCHAR(255),
    last_seen_ts BIGINT,
    last_seen_ip VARCHAR(45),
    user_agent TEXT,
    first_seen_ts BIGINT DEFAULT (EXTRACT(EPOCH FROM NOW()) * 1000)::BIGINT,
    PRIMARY KEY (device_id, user_id)
);

CREATE INDEX IF NOT EXISTS idx_devices_user ON devices(user_id);

--------------------------------------------------------------------------------
-- 访问令牌表 (统一命名规范)
--------------------------------------------------------------------------------
CREATE TABLE IF NOT EXISTS access_tokens (
    id BIGSERIAL PRIMARY KEY,
    token TEXT UNIQUE NOT NULL,
    user_id VARCHAR(255) NOT NULL REFERENCES users(user_id) ON DELETE CASCADE,
    device_id VARCHAR(255),
    appservice_id VARCHAR(255),
    created_ts BIGINT NOT NULL DEFAULT (EXTRACT(EPOCH FROM NOW()) * 1000)::BIGINT,
    expires_ts BIGINT NOT NULL,
    last_used_ts BIGINT,
    user_agent TEXT,
    ip_address VARCHAR(45),
    is_valid BOOLEAN DEFAULT TRUE,
    invalidated_ts BIGINT
);

CREATE INDEX IF NOT EXISTS idx_access_tokens_token ON access_tokens(token);
CREATE INDEX IF NOT EXISTS idx_access_tokens_user ON access_tokens(user_id);
CREATE INDEX IF NOT EXISTS idx_access_tokens_expires ON access_tokens(expires_ts);

--------------------------------------------------------------------------------
-- 刷新令牌表 (统一命名规范，清理冗余字段)
--------------------------------------------------------------------------------
DROP TABLE IF EXISTS refresh_tokens CASCADE;

CREATE TABLE refresh_tokens (
    id BIGSERIAL PRIMARY KEY,
    token_hash VARCHAR(255) UNIQUE NOT NULL,
    user_id VARCHAR(255) NOT NULL REFERENCES users(user_id) ON DELETE CASCADE,
    device_id VARCHAR(255),
    access_token_id VARCHAR(255),
    scope VARCHAR(255),
    created_ts BIGINT NOT NULL DEFAULT (EXTRACT(EPOCH FROM NOW()) * 1000)::BIGINT,
    expires_at BIGINT,
    last_used_ts BIGINT,
    use_count INTEGER DEFAULT 0,
    is_revoked BOOLEAN DEFAULT FALSE,
    revoked_ts BIGINT,
    revoked_reason TEXT,
    client_info JSONB,
    ip_address VARCHAR(45),
    user_agent TEXT
);

CREATE INDEX IF NOT EXISTS idx_refresh_tokens_token ON refresh_tokens(token_hash);
CREATE INDEX IF NOT EXISTS idx_refresh_tokens_user ON refresh_tokens(user_id);
CREATE INDEX IF NOT EXISTS idx_refresh_tokens_expires ON refresh_tokens(expires_at);
CREATE INDEX IF NOT EXISTS idx_refresh_tokens_access_token ON refresh_tokens(access_token_id);

--------------------------------------------------------------------------------
-- 令牌黑名单表
--------------------------------------------------------------------------------
CREATE TABLE IF NOT EXISTS token_blacklist (
    id BIGSERIAL PRIMARY KEY,
    token_hash VARCHAR(255) UNIQUE NOT NULL,
    token_type VARCHAR(50) NOT NULL,
    user_id VARCHAR(255) NOT NULL REFERENCES users(user_id) ON DELETE CASCADE,
    revoked_ts BIGINT NOT NULL,
    expires_at BIGINT NOT NULL,
    reason TEXT,
    created_ts BIGINT DEFAULT (EXTRACT(EPOCH FROM NOW()) * 1000)::BIGINT
);

CREATE INDEX IF NOT EXISTS idx_token_blacklist_hash ON token_blacklist(token_hash);
CREATE INDEX IF NOT EXISTS idx_token_blacklist_expires ON token_blacklist(expires_at);

--------------------------------------------------------------------------------
-- 刷新令牌族表
--------------------------------------------------------------------------------
CREATE TABLE IF NOT EXISTS refresh_token_families (
    id BIGSERIAL PRIMARY KEY,
    family_id VARCHAR(255) UNIQUE NOT NULL,
    user_id VARCHAR(255) NOT NULL REFERENCES users(user_id) ON DELETE CASCADE,
    device_id VARCHAR(255),
    created_ts BIGINT NOT NULL DEFAULT (EXTRACT(EPOCH FROM NOW()) * 1000)::BIGINT,
    last_refresh_ts BIGINT,
    refresh_count INTEGER DEFAULT 0,
    is_compromised BOOLEAN DEFAULT FALSE,
    compromised_ts BIGINT
);

CREATE INDEX IF NOT EXISTS idx_refresh_token_families_user ON refresh_token_families(user_id);

--------------------------------------------------------------------------------
-- 刷新令牌轮换记录表
--------------------------------------------------------------------------------
CREATE TABLE IF NOT EXISTS refresh_token_rotations (
    id BIGSERIAL PRIMARY KEY,
    family_id VARCHAR(255) NOT NULL REFERENCES refresh_token_families(family_id) ON DELETE CASCADE,
    old_token_hash VARCHAR(255),
    new_token_hash VARCHAR(255) NOT NULL,
    rotated_ts BIGINT NOT NULL,
    rotation_reason VARCHAR(100)
);

CREATE INDEX IF NOT EXISTS idx_refresh_token_rotations_family ON refresh_token_rotations(family_id);

--------------------------------------------------------------------------------
-- 刷新令牌使用记录表
--------------------------------------------------------------------------------
CREATE TABLE IF NOT EXISTS refresh_token_usage (
    id BIGSERIAL PRIMARY KEY,
    refresh_token_id BIGINT NOT NULL REFERENCES refresh_tokens(id) ON DELETE CASCADE,
    user_id VARCHAR(255) NOT NULL REFERENCES users(user_id) ON DELETE CASCADE,
    old_access_token_id VARCHAR(255),
    new_access_token_id VARCHAR(255),
    used_ts BIGINT NOT NULL,
    ip_address VARCHAR(45),
    user_agent TEXT,
    success BOOLEAN NOT NULL DEFAULT TRUE,
    error_message TEXT
);

CREATE INDEX IF NOT EXISTS idx_refresh_token_usage_token ON refresh_token_usage(refresh_token_id);
CREATE INDEX IF NOT EXISTS idx_refresh_token_usage_user ON refresh_token_usage(user_id);

--------------------------------------------------------------------------------
-- 房间表
--------------------------------------------------------------------------------
CREATE TABLE IF NOT EXISTS rooms (
    room_id VARCHAR(255) PRIMARY KEY,
    name VARCHAR(255),
    topic TEXT,
    canonical_alias VARCHAR(255),
    join_rule VARCHAR(50) DEFAULT 'invite',
    creator VARCHAR(255) REFERENCES users(user_id),
    version VARCHAR(10) DEFAULT '1',
    encryption JSONB,
    is_public BOOLEAN DEFAULT FALSE,
    member_count INTEGER DEFAULT 0,
    history_visibility VARCHAR(50) DEFAULT 'shared',
    creation_ts BIGINT DEFAULT (EXTRACT(EPOCH FROM NOW()) * 1000)::BIGINT,
    avatar_url TEXT
);

CREATE INDEX IF NOT EXISTS idx_rooms_creator ON rooms(creator);

--------------------------------------------------------------------------------
-- 房间成员表
--------------------------------------------------------------------------------
CREATE TABLE IF NOT EXISTS room_members (
    id BIGSERIAL PRIMARY KEY,
    room_id VARCHAR(255) NOT NULL REFERENCES rooms(room_id) ON DELETE CASCADE,
    user_id VARCHAR(255) NOT NULL REFERENCES users(user_id) ON DELETE CASCADE,
    display_name VARCHAR(255),
    membership VARCHAR(50) DEFAULT 'join',
    avatar_url TEXT,
    join_reason TEXT,
    banned_by VARCHAR(255),
    sender VARCHAR(255),
    event_id VARCHAR(255),
    event_type VARCHAR(100),
    is_banned BOOLEAN DEFAULT FALSE,
    invite_token VARCHAR(255),
    updated_ts BIGINT,
    joined_ts BIGINT,
    left_ts BIGINT,
    reason TEXT,
    ban_reason TEXT,
    UNIQUE(room_id, user_id)
);

CREATE INDEX IF NOT EXISTS idx_room_members_room ON room_members(room_id);
CREATE INDEX IF NOT EXISTS idx_room_members_user ON room_members(user_id);

--------------------------------------------------------------------------------
-- 事件表
--------------------------------------------------------------------------------
CREATE TABLE IF NOT EXISTS events (
    event_id VARCHAR(255) PRIMARY KEY,
    room_id VARCHAR(255) NOT NULL REFERENCES rooms(room_id) ON DELETE CASCADE,
    user_id VARCHAR(255) NOT NULL,
    sender VARCHAR(255) REFERENCES users(user_id),
    event_type VARCHAR(100) NOT NULL,
    content JSONB NOT NULL,
    state_key VARCHAR(255),
    origin_server_ts BIGINT NOT NULL,
    processed_ts BIGINT DEFAULT (EXTRACT(EPOCH FROM NOW()) * 1000)::BIGINT,
    unsigned JSONB,
    redacted BOOLEAN DEFAULT FALSE,
    depth BIGINT DEFAULT 0,
    not_before BIGINT DEFAULT 0,
    status VARCHAR(50),
    reference_image TEXT,
    origin VARCHAR(255)
);

CREATE INDEX IF NOT EXISTS idx_events_room ON events(room_id);
CREATE INDEX IF NOT EXISTS idx_events_type ON events(event_type);
CREATE INDEX IF NOT EXISTS idx_events_sender ON events(sender);
CREATE INDEX IF NOT EXISTS idx_events_origin_ts ON events(origin_server_ts);

--------------------------------------------------------------------------------
-- 联邦签名密钥表
--------------------------------------------------------------------------------
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

--------------------------------------------------------------------------------
-- 安全事件表
--------------------------------------------------------------------------------
CREATE TABLE IF NOT EXISTS security_events (
    id BIGSERIAL PRIMARY KEY,
    event_type VARCHAR(100) NOT NULL,
    user_id VARCHAR(255) REFERENCES users(user_id) ON DELETE CASCADE,
    ip_address VARCHAR(45),
    user_agent TEXT,
    details JSONB,
    description TEXT,
    created_at TIMESTAMP WITH TIME ZONE DEFAULT NOW(),
    created_ts BIGINT DEFAULT (EXTRACT(EPOCH FROM NOW()) * 1000)::BIGINT
);

CREATE INDEX IF NOT EXISTS idx_security_events_user ON security_events(user_id);
CREATE INDEX IF NOT EXISTS idx_security_events_type ON security_events(event_type);

--------------------------------------------------------------------------------
-- IP封禁表
--------------------------------------------------------------------------------
CREATE TABLE IF NOT EXISTS ip_blocks (
    id BIGSERIAL PRIMARY KEY,
    ip_range VARCHAR(100) UNIQUE NOT NULL,
    reason TEXT NOT NULL,
    blocked_by VARCHAR(255) REFERENCES users(user_id),
    blocked_at TIMESTAMP WITH TIME ZONE DEFAULT NOW(),
    blocked_ts BIGINT DEFAULT (EXTRACT(EPOCH FROM NOW()) * 1000)::BIGINT,
    expires_at TIMESTAMP WITH TIME ZONE,
    expires_ts BIGINT
);

CREATE INDEX IF NOT EXISTS idx_ip_blocks_range ON ip_blocks(ip_range);

--------------------------------------------------------------------------------
-- IP信誉表
--------------------------------------------------------------------------------
CREATE TABLE IF NOT EXISTS ip_reputation (
    id BIGSERIAL PRIMARY KEY,
    ip_address VARCHAR(45) UNIQUE NOT NULL,
    reputation_score INTEGER DEFAULT 100,
    failed_attempts INTEGER DEFAULT 0,
    successful_attempts INTEGER DEFAULT 0,
    last_failed_ts BIGINT,
    last_success_ts BIGINT,
    blocked_until_ts BIGINT,
    risk_level VARCHAR(20) DEFAULT 'none',
    created_ts BIGINT DEFAULT (EXTRACT(EPOCH FROM NOW()) * 1000)::BIGINT,
    updated_ts BIGINT
);

CREATE INDEX IF NOT EXISTS idx_ip_reputation_ip ON ip_reputation(ip_address);

--------------------------------------------------------------------------------
-- 封禁房间表
--------------------------------------------------------------------------------
CREATE TABLE IF NOT EXISTS blocked_rooms (
    id BIGSERIAL PRIMARY KEY,
    room_id VARCHAR(255) UNIQUE NOT NULL,
    blocked_at BIGINT NOT NULL DEFAULT (EXTRACT(EPOCH FROM NOW()) * 1000)::BIGINT,
    blocked_by VARCHAR(255) REFERENCES users(user_id),
    reason TEXT
);

CREATE INDEX IF NOT EXISTS idx_blocked_rooms_room ON blocked_rooms(room_id);

--------------------------------------------------------------------------------
-- 事件报告表
--------------------------------------------------------------------------------
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

--------------------------------------------------------------------------------
-- 事件报告历史表
--------------------------------------------------------------------------------
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

--------------------------------------------------------------------------------
-- 举报速率限制表
--------------------------------------------------------------------------------
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

--------------------------------------------------------------------------------
-- 事件报告统计表
--------------------------------------------------------------------------------
CREATE TABLE IF NOT EXISTS event_report_stats (
    id BIGSERIAL PRIMARY KEY,
    stat_date DATE NOT NULL UNIQUE,
    total_reports INTEGER DEFAULT 0,
    open_reports INTEGER DEFAULT 0,
    resolved_reports INTEGER DEFAULT 0,
    dismissed_reports INTEGER DEFAULT 0,
    avg_resolution_time_ms BIGINT,
    reports_by_reason JSONB,
    created_ts BIGINT DEFAULT (EXTRACT(EPOCH FROM NOW()) * 1000)::BIGINT,
    updated_ts BIGINT DEFAULT (EXTRACT(EPOCH FROM NOW()) * 1000)::BIGINT
);

CREATE INDEX IF NOT EXISTS idx_event_report_stats_date ON event_report_stats(stat_date);

--------------------------------------------------------------------------------
-- 注册令牌表
--------------------------------------------------------------------------------
CREATE TABLE IF NOT EXISTS registration_tokens (
    id BIGSERIAL PRIMARY KEY,
    token VARCHAR(255) NOT NULL UNIQUE,
    token_type VARCHAR(50) DEFAULT 'single_use',
    description TEXT,
    max_uses INTEGER DEFAULT 1,
    current_uses INTEGER DEFAULT 0,
    is_used BOOLEAN DEFAULT FALSE,
    is_active BOOLEAN DEFAULT TRUE,
    expires_at BIGINT,
    created_by VARCHAR(255) REFERENCES users(user_id) ON DELETE SET NULL,
    created_ts BIGINT NOT NULL DEFAULT (EXTRACT(EPOCH FROM NOW()) * 1000)::BIGINT,
    updated_ts BIGINT NOT NULL DEFAULT (EXTRACT(EPOCH FROM NOW()) * 1000)::BIGINT,
    last_used_ts BIGINT,
    allowed_email_domains TEXT[],
    allowed_user_ids TEXT[],
    auto_join_rooms TEXT[],
    display_name VARCHAR(255),
    email VARCHAR(255)
);

CREATE INDEX IF NOT EXISTS idx_registration_tokens_token ON registration_tokens(token);
CREATE INDEX IF NOT EXISTS idx_registration_tokens_is_active ON registration_tokens(is_active);
CREATE INDEX IF NOT EXISTS idx_registration_tokens_is_used ON registration_tokens(is_used);
CREATE INDEX IF NOT EXISTS idx_registration_tokens_expires_at ON registration_tokens(expires_at);
CREATE INDEX IF NOT EXISTS idx_registration_tokens_created_ts ON registration_tokens(created_ts DESC);
CREATE INDEX IF NOT EXISTS idx_registration_tokens_token_type ON registration_tokens(token_type);

--------------------------------------------------------------------------------
-- 注册令牌使用记录表
--------------------------------------------------------------------------------
CREATE TABLE IF NOT EXISTS registration_token_usage (
    id BIGSERIAL PRIMARY KEY,
    token_id BIGINT NOT NULL REFERENCES registration_tokens(id) ON DELETE CASCADE,
    token VARCHAR(255) NOT NULL,
    user_id VARCHAR(255) NOT NULL REFERENCES users(user_id) ON DELETE CASCADE,
    username VARCHAR(255),
    email VARCHAR(255),
    ip_address VARCHAR(45),
    user_agent TEXT,
    used_ts BIGINT NOT NULL DEFAULT (EXTRACT(EPOCH FROM NOW()) * 1000)::BIGINT,
    success BOOLEAN DEFAULT TRUE,
    error_message TEXT
);

CREATE INDEX IF NOT EXISTS idx_registration_token_usage_token_id ON registration_token_usage(token_id);
CREATE INDEX IF NOT EXISTS idx_registration_token_usage_user_id ON registration_token_usage(user_id);
CREATE INDEX IF NOT EXISTS idx_registration_token_usage_used_ts ON registration_token_usage(used_ts DESC);

--------------------------------------------------------------------------------
-- 房间邀请表
--------------------------------------------------------------------------------
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

--------------------------------------------------------------------------------
-- 注册令牌批量创建表
--------------------------------------------------------------------------------
CREATE TABLE IF NOT EXISTS registration_token_batches (
    id BIGSERIAL PRIMARY KEY,
    batch_id VARCHAR(255) NOT NULL UNIQUE,
    description TEXT,
    token_count INTEGER NOT NULL,
    tokens_used INTEGER DEFAULT 0,
    created_by VARCHAR(255) REFERENCES users(user_id) ON DELETE SET NULL,
    created_ts BIGINT NOT NULL DEFAULT (EXTRACT(EPOCH FROM NOW()) * 1000)::BIGINT,
    expires_at BIGINT,
    is_active BOOLEAN DEFAULT TRUE,
    allowed_email_domains TEXT[],
    auto_join_rooms TEXT[]
);

CREATE INDEX IF NOT EXISTS idx_registration_token_batches_batch_id ON registration_token_batches(batch_id);
CREATE INDEX IF NOT EXISTS idx_registration_token_batches_created_by ON registration_token_batches(created_by);
CREATE INDEX IF NOT EXISTS idx_registration_token_batches_is_active ON registration_token_batches(is_active);

--------------------------------------------------------------------------------
-- 模式迁移记录表
--------------------------------------------------------------------------------
CREATE TABLE IF NOT EXISTS schema_migrations (
    version VARCHAR(255) PRIMARY KEY,
    checksum VARCHAR(64),
    execution_time_ms BIGINT,
    success BOOLEAN NOT NULL DEFAULT TRUE,
    executed_at TIMESTAMP WITH TIME ZONE DEFAULT NOW(),
    error_message TEXT
);

--------------------------------------------------------------------------------
-- 数据库元数据表
--------------------------------------------------------------------------------
CREATE TABLE IF NOT EXISTS db_metadata (
    key VARCHAR(255) PRIMARY KEY,
    value TEXT NOT NULL,
    created_ts BIGINT DEFAULT (EXTRACT(EPOCH FROM NOW()) * 1000)::BIGINT,
    updated_ts BIGINT
);

--------------------------------------------------------------------------------
-- 插入初始元数据
--------------------------------------------------------------------------------
INSERT INTO db_metadata (key, value, created_ts, updated_ts)
VALUES ('schema_version', '1.0.0', (EXTRACT(EPOCH FROM NOW()) * 1000)::BIGINT, (EXTRACT(EPOCH FROM NOW()) * 1000)::BIGINT)
ON CONFLICT (key) DO UPDATE SET value = '1.0.0', updated_ts = (EXTRACT(EPOCH FROM NOW()) * 1000)::BIGINT;

INSERT INTO schema_migrations (version, success, executed_at)
VALUES ('1.0.0', TRUE, NOW())
ON CONFLICT (version) DO UPDATE SET success = TRUE, executed_at = NOW();

--------------------------------------------------------------------------------
-- 完成提示
--------------------------------------------------------------------------------
-- 数据库初始化完成
-- 版本: 1.0.0
-- 日期: 2026-02-19
