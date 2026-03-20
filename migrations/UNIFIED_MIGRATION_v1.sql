-- ============================================================================
-- Synapse Rust 综合迁移脚本 v1.0.0
-- 文件: UNIFIED_MIGRATION_v1.sql
-- 创建日期: 2026-03-20
-- 描述: 整合所有增量迁移，实现一键部署
-- 幂等性: 完全幂等，可重复执行
-- 依赖: 00000000_unified_schema_v6.sql (必须先执行)
-- ============================================================================

-- ============================================================================
-- 配置
-- ============================================================================
SET statement_timeout = '30min';
SET lock_timeout = '10s';
SET client_min_messages = 'notice';

-- ============================================================================
-- 辅助函数
-- ============================================================================
CREATE OR REPLACE FUNCTION table_exists(table_name TEXT)
RETURNS BOOLEAN AS $$
BEGIN
    RETURN EXISTS (
        SELECT 1 FROM information_schema.tables
        WHERE table_schema = 'public' AND table_name = table_exists.table_name
    );
END;
$$ LANGUAGE plpgsql;

CREATE OR REPLACE FUNCTION column_exists(table_name TEXT, column_name TEXT)
RETURNS BOOLEAN AS $$
BEGIN
    RETURN EXISTS (
        SELECT 1 FROM information_schema.columns
        WHERE table_schema = 'public'
        AND table_name = column_exists.table_name
        AND column_name = column_exists.column_name
    );
END;
$$ LANGUAGE plpgsql;

CREATE OR REPLACE FUNCTION index_exists(index_name TEXT)
RETURNS BOOLEAN AS $$
BEGIN
    RETURN EXISTS (
        SELECT 1 FROM pg_indexes
        WHERE schemaname = 'public' AND indexname = index_exists.index_name
    );
END;
$$ LANGUAGE plpgsql;

BEGIN;

-- ============================================================================
-- 第一部分: 认证与安全表
-- ============================================================================

-- access_tokens 表
CREATE TABLE IF NOT EXISTS access_tokens (
    id BIGSERIAL PRIMARY KEY,
    token TEXT NOT NULL UNIQUE,
    user_id TEXT NOT NULL,
    device_id TEXT,
    created_ts BIGINT NOT NULL,
    expires_at BIGINT,
    last_used_ts BIGINT,
    user_agent TEXT,
    ip_address TEXT,
    is_revoked BOOLEAN DEFAULT FALSE,
    revoked_at BIGINT
);

CREATE INDEX IF NOT EXISTS idx_access_tokens_user_id ON access_tokens(user_id);
CREATE INDEX IF NOT EXISTS idx_access_tokens_valid ON access_tokens(is_revoked) WHERE is_revoked = FALSE;
CREATE INDEX IF NOT EXISTS idx_access_tokens_token ON access_tokens(token);

-- refresh_tokens 表
CREATE TABLE IF NOT EXISTS refresh_tokens (
    id BIGSERIAL PRIMARY KEY,
    token_hash TEXT NOT NULL UNIQUE,
    user_id TEXT NOT NULL,
    device_id TEXT,
    access_token_id TEXT,
    scope TEXT,
    created_ts BIGINT NOT NULL,
    expires_at BIGINT,
    last_used_ts BIGINT,
    use_count INTEGER DEFAULT 0,
    is_revoked BOOLEAN DEFAULT FALSE,
    revoked_at BIGINT,
    revoked_reason TEXT,
    client_info JSONB,
    ip_address TEXT,
    user_agent TEXT
);

CREATE INDEX IF NOT EXISTS idx_refresh_tokens_user_id ON refresh_tokens(user_id);
CREATE INDEX IF NOT EXISTS idx_refresh_tokens_revoked ON refresh_tokens(is_revoked) WHERE is_revoked = FALSE;

-- token_blacklist 表
CREATE TABLE IF NOT EXISTS token_blacklist (
    id BIGSERIAL PRIMARY KEY,
    token_hash TEXT NOT NULL UNIQUE,
    token_type TEXT DEFAULT 'access',
    user_id TEXT,
    revoked_at BIGINT NOT NULL,
    expires_at BIGINT,
    reason TEXT
);

CREATE INDEX IF NOT EXISTS idx_token_blacklist_hash ON token_blacklist(token_hash);

-- password_reset_tokens 表
CREATE TABLE IF NOT EXISTS password_reset_tokens (
    id BIGSERIAL PRIMARY KEY,
    user_id TEXT NOT NULL,
    email TEXT NOT NULL,
    token_hash TEXT NOT NULL,
    created_ts BIGINT NOT NULL,
    expires_at BIGINT,
    used_at BIGINT
);

CREATE INDEX IF NOT EXISTS idx_password_reset_tokens_email ON password_reset_tokens(email);
CREATE INDEX IF NOT EXISTS idx_password_reset_tokens_user ON password_reset_tokens(user_id);
CREATE INDEX IF NOT EXISTS idx_password_reset_tokens_token ON password_reset_tokens(token_hash);

-- registration_tokens 表
CREATE TABLE IF NOT EXISTS registration_tokens (
    id BIGSERIAL PRIMARY KEY,
    token TEXT NOT NULL UNIQUE,
    token_type TEXT DEFAULT 'single_use',
    description TEXT,
    max_uses INTEGER DEFAULT 0,
    uses_count INTEGER DEFAULT 0,
    is_used BOOLEAN DEFAULT FALSE,
    is_enabled BOOLEAN DEFAULT TRUE,
    created_ts BIGINT NOT NULL,
    updated_ts BIGINT,
    expires_at BIGINT,
    last_used_ts BIGINT,
    created_by TEXT NOT NULL,
    allowed_email_domains TEXT[],
    allowed_user_ids TEXT[],
    auto_join_rooms TEXT[],
    display_name TEXT,
    email TEXT
);

CREATE INDEX IF NOT EXISTS idx_registration_tokens_type ON registration_tokens(token_type);
CREATE INDEX IF NOT EXISTS idx_registration_tokens_expires ON registration_tokens(expires_at) WHERE expires_at IS NOT NULL;
CREATE INDEX IF NOT EXISTS idx_registration_tokens_enabled ON registration_tokens(is_enabled) WHERE is_enabled = TRUE;

-- ============================================================================
-- 第二部分: 用户相关表
-- ============================================================================

-- devices 表
CREATE TABLE IF NOT EXISTS devices (
    device_id TEXT NOT NULL PRIMARY KEY,
    user_id TEXT NOT NULL,
    display_name TEXT,
    device_key JSONB,
    last_seen_ts BIGINT,
    last_seen_ip TEXT,
    created_ts BIGINT NOT NULL,
    first_seen_ts BIGINT NOT NULL,
    user_agent TEXT,
    appservice_id TEXT,
    ignored_user_list TEXT
);

CREATE INDEX IF NOT EXISTS idx_devices_user_id ON devices(user_id);
CREATE INDEX IF NOT EXISTS idx_devices_last_seen ON devices(last_seen_ts DESC);

-- user_threepids 表
CREATE TABLE IF NOT EXISTS user_threepids (
    id BIGSERIAL PRIMARY KEY,
    user_id TEXT NOT NULL,
    medium TEXT NOT NULL,
    address TEXT NOT NULL,
    validated_at BIGINT,
    added_ts BIGINT NOT NULL,
    is_verified BOOLEAN DEFAULT FALSE,
    verification_token TEXT,
    verification_expires_at BIGINT
);

CREATE INDEX IF NOT EXISTS idx_user_threepids_user ON user_threepids(user_id);
CREATE UNIQUE INDEX IF NOT EXISTS idx_user_threepids_medium_address ON user_threepids(medium, address);

-- user_directory 表
CREATE TABLE IF NOT EXISTS user_directory (
    user_id TEXT NOT NULL,
    room_id TEXT NOT NULL,
    visibility TEXT NOT NULL DEFAULT 'private',
    added_by TEXT,
    created_ts BIGINT NOT NULL,
    updated_ts BIGINT,
    PRIMARY KEY (user_id, room_id)
);

CREATE INDEX IF NOT EXISTS idx_user_directory_visibility ON user_directory(visibility);

-- presence 表
CREATE TABLE IF NOT EXISTS presence (
    user_id TEXT NOT NULL PRIMARY KEY,
    status_msg TEXT,
    presence TEXT NOT NULL DEFAULT 'offline',
    last_active_ts BIGINT NOT NULL DEFAULT 0,
    status_from TEXT,
    created_ts BIGINT NOT NULL,
    updated_ts BIGINT NOT NULL
);

-- password_history 表
CREATE TABLE IF NOT EXISTS password_history (
    id BIGSERIAL PRIMARY KEY,
    user_id TEXT NOT NULL,
    password_hash TEXT NOT NULL,
    created_ts BIGINT NOT NULL
);

CREATE INDEX IF NOT EXISTS idx_password_history_user ON password_history(user_id);
CREATE INDEX IF NOT EXISTS idx_password_history_created ON password_history(created_ts DESC);

-- ============================================================================
-- 第三部分: 房间相关表
-- ============================================================================

-- rooms 表 (已有基础定义，添加缺失列)
ALTER TABLE rooms ADD COLUMN IF NOT EXISTS guest_access VARCHAR(50) DEFAULT 'forbidden';
ALTER TABLE rooms ADD COLUMN IF NOT EXISTS parent_id VARCHAR(255);
ALTER TABLE rooms ADD COLUMN IF NOT EXISTS is_federated BOOLEAN DEFAULT TRUE;
ALTER TABLE rooms ADD COLUMN IF NOT EXISTS has_guest_access BOOLEAN DEFAULT FALSE;

-- room_memberships 表
CREATE TABLE IF NOT EXISTS room_memberships (
    id BIGSERIAL PRIMARY KEY,
    room_id TEXT NOT NULL,
    user_id TEXT NOT NULL,
    membership TEXT NOT NULL,
    joined_ts BIGINT,
    invited_ts BIGINT,
    left_ts BIGINT,
    banned_ts BIGINT,
    sender TEXT,
    reason TEXT,
    event_id TEXT,
    event_type TEXT,
    display_name TEXT,
    avatar_url TEXT,
    is_banned BOOLEAN DEFAULT FALSE,
    invite_token TEXT,
    updated_ts BIGINT,
    join_reason TEXT,
    banned_by TEXT,
    ban_reason TEXT,
    UNIQUE (room_id, user_id)
);

CREATE INDEX IF NOT EXISTS idx_room_memberships_room ON room_memberships(room_id);
CREATE INDEX IF NOT EXISTS idx_room_memberships_user ON room_memberships(user_id);
CREATE INDEX IF NOT EXISTS idx_room_memberships_membership ON room_memberships(membership);
CREATE INDEX IF NOT EXISTS idx_room_memberships_user_membership ON room_memberships(user_id, membership);
CREATE INDEX IF NOT EXISTS idx_room_memberships_room_membership ON room_memberships(room_id, membership);
CREATE INDEX IF NOT EXISTS idx_room_memberships_joined ON room_memberships(user_id, room_id) WHERE membership = 'join';

-- room_aliases 表
CREATE TABLE IF NOT EXISTS room_aliases (
    room_alias TEXT NOT NULL PRIMARY KEY,
    room_id TEXT NOT NULL,
    server_name TEXT NOT NULL,
    created_ts BIGINT NOT NULL
);

CREATE INDEX IF NOT EXISTS idx_room_aliases_room_id ON room_aliases(room_id);

-- room_summaries 表
CREATE TABLE IF NOT EXISTS room_summaries (
    room_id TEXT NOT NULL PRIMARY KEY,
    name TEXT,
    topic TEXT,
    canonical_alias TEXT,
    member_count BIGINT DEFAULT 0,
    joined_members BIGINT DEFAULT 0,
    invited_members BIGINT DEFAULT 0,
    hero_users JSONB DEFAULT '[]',
    is_world_readable BOOLEAN DEFAULT FALSE,
    can_guest_join BOOLEAN DEFAULT FALSE,
    is_federated BOOLEAN DEFAULT TRUE,
    encryption_state TEXT,
    updated_ts BIGINT,
    guest_access VARCHAR(50) DEFAULT 'can_join'
);

-- room_depth 表
CREATE TABLE IF NOT EXISTS room_depth (
    room_id VARCHAR(255) PRIMARY KEY,
    current_depth BIGINT NOT NULL DEFAULT 0,
    max_depth BIGINT NOT NULL DEFAULT 0,
    updated_at BIGINT NOT NULL DEFAULT (EXTRACT(EPOCH FROM NOW()) * 1000)::BIGINT
);

CREATE INDEX IF NOT EXISTS idx_room_depth_room_id ON room_depth(room_id);

-- room_events 表
CREATE TABLE IF NOT EXISTS room_events (
    id SERIAL PRIMARY KEY,
    event_id VARCHAR(255) UNIQUE NOT NULL,
    room_id VARCHAR(255) NOT NULL,
    sender VARCHAR(255) NOT NULL,
    event_type VARCHAR(255) NOT NULL,
    state_key VARCHAR(255),
    content JSONB NOT NULL DEFAULT '{}',
    prev_event_id VARCHAR(255),
    origin_server_ts BIGINT NOT NULL,
    created_ts BIGINT NOT NULL
);

CREATE INDEX IF NOT EXISTS idx_room_events_room ON room_events(room_id);
CREATE INDEX IF NOT EXISTS idx_room_events_event ON room_events(event_id);

-- room_tags 表
CREATE TABLE IF NOT EXISTS room_tags (
    id SERIAL PRIMARY KEY,
    user_id VARCHAR(255) NOT NULL,
    room_id VARCHAR(255) NOT NULL,
    tag VARCHAR(255) NOT NULL,
    order_value DOUBLE PRECISION,
    created_ts BIGINT NOT NULL,
    UNIQUE (user_id, room_id, tag)
);

-- room_parents 表
CREATE TABLE IF NOT EXISTS room_parents (
    id BIGSERIAL PRIMARY KEY,
    room_id TEXT NOT NULL,
    parent_room_id TEXT NOT NULL,
    sender TEXT NOT NULL,
    is_suggested BOOLEAN DEFAULT FALSE,
    via_servers JSONB DEFAULT '[]',
    added_ts BIGINT NOT NULL,
    UNIQUE (room_id, parent_room_id)
);

CREATE INDEX IF NOT EXISTS idx_room_parents_room ON room_parents(room_id);
CREATE INDEX IF NOT EXISTS idx_room_parents_parent ON room_parents(parent_room_id);

-- event_auth 表
CREATE TABLE IF NOT EXISTS event_auth (
    room_id VARCHAR(255) NOT NULL,
    event_id VARCHAR(255) NOT NULL,
    auth_method VARCHAR(100) NOT NULL,
    auth_data JSONB,
    created_at BIGINT NOT NULL DEFAULT (EXTRACT(EPOCH FROM NOW()) * 1000)::BIGINT,
    PRIMARY KEY (room_id, event_id, auth_method)
);

CREATE INDEX IF NOT EXISTS idx_event_auth_room_id ON event_auth(room_id);
CREATE INDEX IF NOT EXISTS idx_event_auth_event_id ON event_auth(event_id);

-- redactions 表
CREATE TABLE IF NOT EXISTS redactions (
    redacts_event_id VARCHAR(255) PRIMARY KEY,
    event_id VARCHAR(255) NOT NULL,
    reason JSONB,
    redacted_by VARCHAR(255) NOT NULL,
    redacted_at BIGINT NOT NULL DEFAULT (EXTRACT(EPOCH FROM NOW()) * 1000)::BIGINT
);

CREATE INDEX IF NOT EXISTS idx_redactions_event_id ON redactions(event_id);
CREATE INDEX IF NOT EXISTS idx_redactions_redacted_by ON redactions(redacted_by);

-- ============================================================================
-- 第四部分: E2EE 加密表
-- ============================================================================

-- device_keys 表
CREATE TABLE IF NOT EXISTS device_keys (
    id BIGSERIAL PRIMARY KEY,
    user_id TEXT NOT NULL,
    device_id TEXT NOT NULL,
    algorithm TEXT NOT NULL,
    key_id TEXT NOT NULL,
    public_key TEXT NOT NULL,
    key_data TEXT,
    signatures JSONB,
    added_ts BIGINT NOT NULL,
    created_ts BIGINT NOT NULL,
    updated_ts BIGINT,
    ts_updated_ms BIGINT,
    is_verified BOOLEAN DEFAULT FALSE,
    is_blocked BOOLEAN DEFAULT FALSE,
    display_name TEXT,
    CONSTRAINT uq_device_keys_user_device_key UNIQUE (user_id, device_id, key_id)
);

CREATE INDEX IF NOT EXISTS idx_device_keys_user_device ON device_keys(user_id, device_id);

-- key_backups 表
CREATE TABLE IF NOT EXISTS key_backups (
    backup_id BIGSERIAL PRIMARY KEY,
    user_id TEXT NOT NULL,
    algorithm TEXT NOT NULL,
    auth_data JSONB,
    auth_key TEXT,
    version BIGINT DEFAULT 1,
    created_ts BIGINT NOT NULL,
    updated_ts BIGINT,
    CONSTRAINT uq_key_backups_user_version UNIQUE (user_id, version)
);

CREATE INDEX IF NOT EXISTS idx_key_backups_user ON key_backups(user_id);

-- backup_keys 表
CREATE TABLE IF NOT EXISTS backup_keys (
    id BIGSERIAL PRIMARY KEY,
    backup_id BIGINT NOT NULL,
    room_id TEXT NOT NULL,
    session_id TEXT NOT NULL,
    session_data JSONB NOT NULL,
    created_ts BIGINT NOT NULL
);

CREATE INDEX IF NOT EXISTS idx_backup_keys_backup ON backup_keys(backup_id);
CREATE INDEX IF NOT EXISTS idx_backup_keys_room ON backup_keys(room_id);

-- olm_accounts 表
CREATE TABLE IF NOT EXISTS olm_accounts (
    id BIGSERIAL PRIMARY KEY,
    user_id TEXT NOT NULL,
    device_id TEXT NOT NULL,
    identity_key TEXT NOT NULL,
    serialized_account TEXT NOT NULL,
    has_published_one_time_keys BOOLEAN DEFAULT FALSE,
    has_published_fallback_key BOOLEAN DEFAULT FALSE,
    created_ts BIGINT NOT NULL,
    updated_ts BIGINT NOT NULL,
    CONSTRAINT uq_olm_accounts_user_device UNIQUE (user_id, device_id)
);

-- olm_sessions 表
CREATE TABLE IF NOT EXISTS olm_sessions (
    id BIGSERIAL PRIMARY KEY,
    user_id TEXT NOT NULL,
    device_id TEXT NOT NULL,
    session_id TEXT NOT NULL UNIQUE,
    sender_key TEXT NOT NULL,
    receiver_key TEXT NOT NULL,
    serialized_state TEXT NOT NULL,
    message_index INTEGER DEFAULT 0,
    created_ts BIGINT NOT NULL,
    last_used_ts BIGINT NOT NULL,
    expires_at BIGINT
);

CREATE INDEX IF NOT EXISTS idx_olm_sessions_user_device ON olm_sessions(user_id, device_id);
CREATE INDEX IF NOT EXISTS idx_olm_sessions_sender_key ON olm_sessions(sender_key);
CREATE INDEX IF NOT EXISTS idx_olm_sessions_expires ON olm_sessions(expires_at) WHERE expires_at IS NOT NULL;

-- megolm_sessions 表
CREATE TABLE IF NOT EXISTS megolm_sessions (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    session_id TEXT NOT NULL UNIQUE,
    room_id TEXT NOT NULL,
    sender_key TEXT NOT NULL,
    session_key TEXT NOT NULL,
    algorithm TEXT NOT NULL,
    message_index BIGINT DEFAULT 0,
    created_ts BIGINT NOT NULL,
    last_used_ts BIGINT,
    expires_at BIGINT
);

CREATE INDEX IF NOT EXISTS idx_megolm_sessions_room ON megolm_sessions(room_id);
CREATE INDEX IF NOT EXISTS idx_megolm_sessions_session ON megolm_sessions(session_id);

-- cross_signing_keys 表
CREATE TABLE IF NOT EXISTS cross_signing_keys (
    id BIGSERIAL PRIMARY KEY,
    user_id TEXT NOT NULL,
    key_type TEXT NOT NULL,
    key_data TEXT NOT NULL,
    signatures JSONB,
    added_ts BIGINT NOT NULL,
    CONSTRAINT uq_cross_signing_keys_user_type UNIQUE (user_id, key_type)
);

CREATE INDEX IF NOT EXISTS idx_cross_signing_keys_user ON cross_signing_keys(user_id);

-- one_time_keys 表
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

CREATE INDEX IF NOT EXISTS idx_one_time_keys_user ON one_time_keys(user_id);
CREATE INDEX IF NOT EXISTS idx_one_time_keys_device ON one_time_keys(device_id);
CREATE INDEX IF NOT EXISTS idx_one_time_keys_user_device ON one_time_keys(user_id, device_id);

-- device_lists_stream 表
CREATE TABLE IF NOT EXISTS device_lists_stream (
    stream_id BIGSERIAL PRIMARY KEY,
    user_id VARCHAR(255) NOT NULL,
    device_id VARCHAR(255),
    created_ts BIGINT NOT NULL
);

CREATE INDEX IF NOT EXISTS idx_device_lists_stream_user ON device_lists_stream(user_id);

-- device_lists_changes 表
CREATE TABLE IF NOT EXISTS device_lists_changes (
    id SERIAL PRIMARY KEY,
    user_id VARCHAR(255) NOT NULL,
    device_id VARCHAR(255),
    change_type VARCHAR(50) NOT NULL,
    stream_id BIGINT NOT NULL,
    created_ts BIGINT NOT NULL
);

CREATE INDEX IF NOT EXISTS idx_device_lists_user ON device_lists_changes(user_id);
CREATE INDEX IF NOT EXISTS idx_device_lists_stream ON device_lists_changes(stream_id);

-- e2ee_key_requests 表
CREATE TABLE IF NOT EXISTS e2ee_key_requests (
    id BIGSERIAL PRIMARY KEY,
    request_id TEXT NOT NULL UNIQUE,
    user_id TEXT NOT NULL,
    device_id TEXT NOT NULL,
    room_id TEXT NOT NULL,
    session_id TEXT NOT NULL,
    algorithm TEXT NOT NULL,
    action TEXT NOT NULL,
    is_fulfilled BOOLEAN DEFAULT FALSE,
    fulfilled_by_device TEXT,
    fulfilled_ts BIGINT,
    created_ts BIGINT NOT NULL
);

CREATE INDEX IF NOT EXISTS idx_e2ee_key_requests_user ON e2ee_key_requests(user_id);
CREATE INDEX IF NOT EXISTS idx_e2ee_key_requests_session ON e2ee_key_requests(session_id);
CREATE INDEX IF NOT EXISTS idx_e2ee_key_requests_pending ON e2ee_key_requests(is_fulfilled) WHERE is_fulfilled = FALSE;

-- ============================================================================
-- 第五部分: 应用服务表
-- ============================================================================

-- application_services 表
CREATE TABLE IF NOT EXISTS application_services (
    as_id VARCHAR(255) PRIMARY KEY,
    url TEXT NOT NULL,
    as_token TEXT NOT NULL,
    hs_token TEXT NOT NULL,
    sender_localpart TEXT NOT NULL,
    is_enabled BOOLEAN DEFAULT FALSE,
    rate_limited BOOLEAN DEFAULT TRUE,
    protocols TEXT[] DEFAULT '{}',
    namespaces JSONB DEFAULT '{}',
    created_ts BIGINT NOT NULL,
    updated_ts BIGINT,
    description TEXT,
    sender VARCHAR(255),
    name TEXT,
    last_seen_ts BIGINT
);

CREATE INDEX IF NOT EXISTS idx_application_services_enabled ON application_services(is_enabled) WHERE is_enabled = TRUE;

-- application_service_state 表
CREATE TABLE IF NOT EXISTS application_service_state (
    as_id VARCHAR(255) NOT NULL,
    state_key VARCHAR(255) NOT NULL,
    state_value TEXT NOT NULL,
    updated_ts BIGINT NOT NULL DEFAULT (EXTRACT(EPOCH FROM NOW()) * 1000)::BIGINT,
    PRIMARY KEY (as_id, state_key)
);

CREATE INDEX IF NOT EXISTS idx_application_service_state_as ON application_service_state(as_id);

-- application_service_events 表
CREATE TABLE IF NOT EXISTS application_service_events (
    event_id VARCHAR(255) NOT NULL,
    as_id VARCHAR(255) NOT NULL,
    room_id VARCHAR(255) NOT NULL,
    event_type VARCHAR(255) NOT NULL,
    sender VARCHAR(255) NOT NULL,
    content JSONB NOT NULL DEFAULT '{}',
    state_key TEXT,
    origin_server_ts BIGINT NOT NULL,
    processed_ts BIGINT,
    transaction_id VARCHAR(255),
    PRIMARY KEY (event_id, as_id)
);

CREATE INDEX IF NOT EXISTS idx_application_service_events_as_id ON application_service_events(as_id);
CREATE INDEX IF NOT EXISTS idx_application_service_events_room ON application_service_events(room_id);

-- application_service_transactions 表
CREATE TABLE IF NOT EXISTS application_service_transactions (
    id BIGSERIAL PRIMARY KEY,
    as_id VARCHAR(255) NOT NULL,
    txn_id VARCHAR(255) NOT NULL,
    created_ts BIGINT NOT NULL DEFAULT (EXTRACT(EPOCH FROM NOW()) * 1000)::BIGINT,
    UNIQUE (as_id, txn_id)
);

CREATE INDEX IF NOT EXISTS idx_application_service_transactions_as_id ON application_service_transactions(as_id);

-- application_service_users 表
CREATE TABLE IF NOT EXISTS application_service_users (
    as_id VARCHAR(255) NOT NULL,
    user_id VARCHAR(255) NOT NULL,
    displayname TEXT,
    avatar_url TEXT,
    created_ts BIGINT NOT NULL DEFAULT (EXTRACT(EPOCH FROM NOW()) * 1000)::BIGINT,
    PRIMARY KEY (as_id, user_id)
);

CREATE INDEX IF NOT EXISTS idx_application_service_users_as_id ON application_service_users(as_id);
CREATE INDEX IF NOT EXISTS idx_application_service_users_user_id ON application_service_users(user_id);

-- application_service_rooms 表
CREATE TABLE IF NOT EXISTS application_service_rooms (
    as_id VARCHAR(255) NOT NULL,
    room_id VARCHAR(255) NOT NULL,
    creator_as_id VARCHAR(255) NOT NULL,
    created_ts BIGINT NOT NULL DEFAULT (EXTRACT(EPOCH FROM NOW()) * 1000)::BIGINT,
    PRIMARY KEY (as_id, room_id)
);

CREATE INDEX IF NOT EXISTS idx_application_service_rooms_as_id ON application_service_rooms(as_id);
CREATE INDEX IF NOT EXISTS idx_application_service_rooms_room_id ON application_service_rooms(room_id);

-- ============================================================================
-- 第六部分: Space 相关表
-- ============================================================================

-- space_children 表
CREATE TABLE IF NOT EXISTS space_children (
    id BIGSERIAL PRIMARY KEY,
    space_id TEXT NOT NULL,
    room_id TEXT NOT NULL,
    sender TEXT NOT NULL,
    is_suggested BOOLEAN DEFAULT FALSE,
    via_servers JSONB DEFAULT '[]',
    added_ts BIGINT NOT NULL,
    CONSTRAINT pk_space_children PRIMARY KEY (id),
    CONSTRAINT uq_space_children_space_room UNIQUE (space_id, room_id)
);

CREATE INDEX IF NOT EXISTS idx_space_children_space ON space_children(space_id);
CREATE INDEX IF NOT EXISTS idx_space_children_room ON space_children(room_id);

-- space_hierarchy 表
CREATE TABLE IF NOT EXISTS space_hierarchy (
    id BIGSERIAL PRIMARY KEY,
    space_id TEXT NOT NULL,
    room_id TEXT NOT NULL,
    parent_space_id TEXT,
    depth INTEGER DEFAULT 0,
    children TEXT[],
    via_servers TEXT[],
    created_ts BIGINT NOT NULL,
    updated_ts BIGINT NOT NULL,
    UNIQUE (space_id, room_id)
);

CREATE INDEX IF NOT EXISTS idx_space_hierarchy_space ON space_hierarchy(space_id);

-- thread_subscriptions 表
CREATE TABLE IF NOT EXISTS thread_subscriptions (
    id BIGSERIAL PRIMARY KEY,
    room_id TEXT NOT NULL,
    thread_id TEXT NOT NULL,
    user_id TEXT NOT NULL,
    notification_level TEXT DEFAULT 'all',
    is_muted BOOLEAN DEFAULT FALSE,
    is_pinned BOOLEAN DEFAULT FALSE,
    subscribed_ts BIGINT NOT NULL,
    updated_ts BIGINT NOT NULL,
    UNIQUE (room_id, thread_id, user_id)
);

CREATE INDEX IF NOT EXISTS idx_thread_subscriptions_room_thread ON thread_subscriptions(room_id, thread_id);

-- thread_roots 表
CREATE TABLE IF NOT EXISTS thread_roots (
    id BIGSERIAL PRIMARY KEY,
    room_id TEXT NOT NULL,
    event_id TEXT NOT NULL,
    sender TEXT NOT NULL,
    thread_id TEXT,
    reply_count BIGINT DEFAULT 0,
    last_reply_event_id TEXT,
    last_reply_sender TEXT,
    last_reply_ts BIGINT,
    participants JSONB DEFAULT '[]',
    is_fetched BOOLEAN DEFAULT FALSE,
    created_ts BIGINT NOT NULL,
    updated_ts BIGINT,
    CONSTRAINT uq_thread_roots_room_event UNIQUE (room_id, event_id)
);

CREATE INDEX IF NOT EXISTS idx_thread_roots_room ON thread_roots(room_id);
CREATE INDEX IF NOT EXISTS idx_thread_roots_event ON thread_roots(event_id);
CREATE INDEX IF NOT EXISTS idx_thread_roots_thread ON thread_roots(thread_id);
CREATE INDEX IF NOT EXISTS idx_thread_roots_last_reply ON thread_roots(last_reply_ts DESC) WHERE last_reply_ts IS NOT NULL;

-- ============================================================================
-- 第七部分: 推送相关表
-- ============================================================================

-- pushers 表
CREATE TABLE IF NOT EXISTS pushers (
    id BIGSERIAL PRIMARY KEY,
    user_id TEXT NOT NULL,
    device_id TEXT NOT NULL,
    pushkey TEXT NOT NULL,
    pushkey_ts BIGINT NOT NULL,
    kind TEXT NOT NULL,
    app_id TEXT NOT NULL,
    app_display_name TEXT NOT NULL,
    device_display_name TEXT NOT NULL,
    profile_tag TEXT,
    lang TEXT DEFAULT 'en',
    data JSONB DEFAULT '{}',
    updated_ts BIGINT,
    created_ts BIGINT NOT NULL,
    is_enabled BOOLEAN DEFAULT TRUE,
    CONSTRAINT uq_pushers_user_device_pushkey UNIQUE (user_id, device_id, pushkey)
);

CREATE INDEX IF NOT EXISTS idx_pushers_user ON pushers(user_id);
CREATE INDEX IF NOT EXISTS idx_pushers_enabled ON pushers(is_enabled) WHERE is_enabled = TRUE;

-- push_rules 表
CREATE TABLE IF NOT EXISTS push_rules (
    id BIGSERIAL PRIMARY KEY,
    user_id TEXT NOT NULL,
    scope TEXT NOT NULL,
    rule_id TEXT NOT NULL,
    kind TEXT NOT NULL,
    priority_class INTEGER NOT NULL,
    priority INTEGER DEFAULT 0,
    conditions JSONB DEFAULT '[]',
    actions JSONB DEFAULT '[]',
    pattern TEXT,
    is_default BOOLEAN DEFAULT FALSE,
    is_enabled BOOLEAN DEFAULT TRUE,
    created_ts BIGINT NOT NULL,
    updated_ts BIGINT,
    CONSTRAINT uq_push_rules_user_scope_rule UNIQUE (user_id, scope, rule_id)
);

CREATE INDEX IF NOT EXISTS idx_push_rules_user ON push_rules(user_id);

-- push_devices 表
CREATE TABLE IF NOT EXISTS push_devices (
    id BIGSERIAL PRIMARY KEY,
    user_id TEXT NOT NULL,
    device_id TEXT NOT NULL,
    push_kind TEXT NOT NULL,
    app_id TEXT NOT NULL,
    app_display_name TEXT,
    device_display_name TEXT,
    profile_tag TEXT,
    pushkey TEXT NOT NULL,
    lang TEXT DEFAULT 'en',
    data JSONB DEFAULT '{}',
    created_ts BIGINT NOT NULL,
    updated_ts BIGINT,
    is_enabled BOOLEAN DEFAULT TRUE,
    CONSTRAINT uq_push_devices_user_device UNIQUE (user_id, device_id, pushkey)
);

CREATE INDEX IF NOT EXISTS idx_push_devices_user ON push_devices(user_id);

-- ============================================================================
-- 第八部分: 媒体相关表
-- ============================================================================

-- media_metadata 表
CREATE TABLE IF NOT EXISTS media_metadata (
    media_id TEXT NOT NULL PRIMARY KEY,
    server_name TEXT NOT NULL,
    content_type TEXT NOT NULL,
    file_name TEXT,
    size BIGINT NOT NULL,
    uploader_user_id TEXT,
    created_ts BIGINT NOT NULL,
    last_accessed_at BIGINT,
    quarantine_status TEXT
);

CREATE INDEX IF NOT EXISTS idx_media_uploader ON media_metadata(uploader_user_id);
CREATE INDEX IF NOT EXISTS idx_media_server ON media_metadata(server_name);

-- thumbnails 表
CREATE TABLE IF NOT EXISTS thumbnails (
    id BIGSERIAL PRIMARY KEY,
    media_id TEXT NOT NULL,
    width INTEGER NOT NULL,
    height INTEGER NOT NULL,
    method TEXT NOT NULL,
    content_type TEXT NOT NULL,
    size BIGINT NOT NULL,
    created_ts BIGINT NOT NULL
);

CREATE INDEX IF NOT EXISTS idx_thumbnails_media ON thumbnails(media_id);

-- user_media_quota 表
CREATE TABLE IF NOT EXISTS user_media_quota (
    id BIGSERIAL PRIMARY KEY,
    user_id TEXT NOT NULL UNIQUE,
    max_bytes BIGINT DEFAULT 1073741824,
    used_bytes BIGINT DEFAULT 0,
    file_count INTEGER DEFAULT 0,
    created_ts BIGINT NOT NULL,
    updated_ts BIGINT
);

CREATE INDEX IF NOT EXISTS idx_user_media_quota_used ON user_media_quota(used_bytes DESC) WHERE used_bytes > 0;

-- ============================================================================
-- 第九部分: 联邦相关表
-- ============================================================================

-- federation_servers 表
CREATE TABLE IF NOT EXISTS federation_servers (
    id BIGSERIAL PRIMARY KEY,
    server_name TEXT NOT NULL UNIQUE,
    is_blocked BOOLEAN DEFAULT FALSE,
    blocked_at BIGINT,
    blocked_reason TEXT,
    last_successful_connect_at BIGINT,
    last_failed_connect_at BIGINT,
    failure_count INTEGER DEFAULT 0
);

-- federation_blacklist 表
CREATE TABLE IF NOT EXISTS federation_blacklist (
    id BIGSERIAL PRIMARY KEY,
    server_name TEXT NOT NULL UNIQUE,
    reason TEXT,
    added_ts BIGINT NOT NULL,
    added_by TEXT,
    updated_ts BIGINT
);

CREATE INDEX IF NOT EXISTS idx_federation_blacklist_server ON federation_blacklist(server_name);

-- federation_queue 表
CREATE TABLE IF NOT EXISTS federation_queue (
    id BIGSERIAL PRIMARY KEY,
    destination TEXT NOT NULL,
    event_id TEXT NOT NULL,
    event_type TEXT NOT NULL,
    room_id TEXT,
    content JSONB NOT NULL,
    created_ts BIGINT NOT NULL,
    sent_at BIGINT,
    retry_count INTEGER DEFAULT 0,
    status TEXT DEFAULT 'pending'
);

CREATE INDEX IF NOT EXISTS idx_federation_queue_destination ON federation_queue(destination);
CREATE INDEX IF NOT EXISTS idx_federation_queue_status ON federation_queue(status);

-- federation_signing_keys 表
CREATE TABLE IF NOT EXISTS federation_signing_keys (
    id SERIAL,
    server_name TEXT NOT NULL,
    key_id TEXT NOT NULL,
    secret_key TEXT NOT NULL,
    public_key TEXT NOT NULL,
    created_ts BIGINT NOT NULL,
    expires_at BIGINT,
    key_json TEXT,
    ts_added_ms BIGINT,
    ts_valid_until_ms BIGINT,
    PRIMARY KEY (server_name, key_id)
);

CREATE INDEX IF NOT EXISTS idx_federation_signing_keys_server ON federation_signing_keys(server_name);
CREATE INDEX IF NOT EXISTS idx_federation_signing_keys_expires ON federation_signing_keys(expires_at);

-- ============================================================================
-- 第十部分: 账户数据与过滤器
-- ============================================================================

-- account_data 表
CREATE TABLE IF NOT EXISTS account_data (
    id BIGSERIAL PRIMARY KEY,
    user_id TEXT NOT NULL,
    data_type TEXT NOT NULL,
    content JSONB NOT NULL DEFAULT '{}',
    created_ts BIGINT NOT NULL,
    updated_ts BIGINT NOT NULL,
    CONSTRAINT uq_account_data_user_type UNIQUE (user_id, data_type)
);

CREATE INDEX IF NOT EXISTS idx_account_data_user ON account_data(user_id);

-- filters 表
CREATE TABLE IF NOT EXISTS filters (
    id BIGSERIAL PRIMARY KEY,
    user_id TEXT NOT NULL,
    filter_id TEXT NOT NULL,
    content JSONB NOT NULL DEFAULT '{}',
    created_ts BIGINT NOT NULL,
    CONSTRAINT uq_filters_user_filter UNIQUE (user_id, filter_id)
);

CREATE INDEX IF NOT EXISTS idx_filters_user ON filters(user_id);
CREATE INDEX IF NOT EXISTS idx_filters_filter_id ON filters(filter_id);

-- user_filters 表
CREATE TABLE IF NOT EXISTS user_filters (
    id SERIAL PRIMARY KEY,
    user_id VARCHAR(255) NOT NULL,
    filter_id VARCHAR(255) NOT NULL,
    filter_json JSONB NOT NULL DEFAULT '{}',
    created_ts BIGINT NOT NULL,
    UNIQUE (user_id, filter_id)
);

-- openid_tokens 表
CREATE TABLE IF NOT EXISTS openid_tokens (
    id BIGSERIAL PRIMARY KEY,
    token TEXT NOT NULL UNIQUE,
    user_id TEXT NOT NULL,
    device_id TEXT,
    created_ts BIGINT NOT NULL,
    expires_ts BIGINT NOT NULL,
    is_valid BOOLEAN DEFAULT TRUE
);

CREATE INDEX IF NOT EXISTS idx_openid_tokens_user ON openid_tokens(user_id);

-- ============================================================================
-- 第十一部分: 其他功能表
-- ============================================================================

-- search_index 表
CREATE TABLE IF NOT EXISTS search_index (
    id SERIAL PRIMARY KEY,
    event_id VARCHAR(255) NOT NULL,
    room_id VARCHAR(255) NOT NULL,
    user_id VARCHAR(255) NOT NULL,
    event_type VARCHAR(255) NOT NULL,
    type VARCHAR(255) NOT NULL,
    content TEXT NOT NULL,
    created_ts BIGINT NOT NULL,
    updated_ts BIGINT,
    CONSTRAINT uq_search_index_event UNIQUE (event_id)
);

CREATE INDEX IF NOT EXISTS idx_search_index_room ON search_index(room_id);
CREATE INDEX IF NOT EXISTS idx_search_index_user ON search_index(user_id);
CREATE INDEX IF NOT EXISTS idx_search_index_type ON search_index(event_type);

-- to_device_messages 表
CREATE TABLE IF NOT EXISTS to_device_messages (
    id SERIAL PRIMARY KEY,
    sender_user_id VARCHAR(255) NOT NULL,
    sender_device_id VARCHAR(255) NOT NULL,
    recipient_user_id VARCHAR(255) NOT NULL,
    recipient_device_id VARCHAR(255) NOT NULL,
    event_type VARCHAR(255) NOT NULL,
    content JSONB NOT NULL DEFAULT '{}',
    message_id VARCHAR(255),
    stream_id BIGINT NOT NULL,
    created_ts BIGINT NOT NULL
);

CREATE INDEX IF NOT EXISTS idx_to_device_recipient ON to_device_messages(recipient_user_id, recipient_device_id);
CREATE INDEX IF NOT EXISTS idx_to_device_stream ON to_device_messages(recipient_user_id, stream_id);

-- room_ephemeral 表
CREATE TABLE IF NOT EXISTS room_ephemeral (
    id SERIAL PRIMARY KEY,
    room_id VARCHAR(255) NOT NULL,
    event_type VARCHAR(255) NOT NULL,
    user_id VARCHAR(255) NOT NULL,
    content JSONB NOT NULL DEFAULT '{}',
    stream_id BIGINT NOT NULL,
    created_ts BIGINT NOT NULL,
    expires_at BIGINT
);

CREATE INDEX IF NOT EXISTS idx_room_ephemeral_room ON room_ephemeral(room_id);

-- typing 表
CREATE TABLE IF NOT EXISTS typing (
    user_id TEXT NOT NULL,
    room_id TEXT NOT NULL,
    typing BOOLEAN DEFAULT FALSE,
    last_active_ts BIGINT NOT NULL,
    UNIQUE (user_id, room_id)
);

-- event_receipts 表
CREATE TABLE IF NOT EXISTS event_receipts (
    id BIGSERIAL PRIMARY KEY,
    event_id TEXT NOT NULL,
    room_id TEXT NOT NULL,
    user_id TEXT NOT NULL,
    receipt_type TEXT NOT NULL,
    ts BIGINT NOT NULL,
    data JSONB DEFAULT '{}',
    created_ts BIGINT NOT NULL,
    updated_ts BIGINT NOT NULL,
    CONSTRAINT uq_event_receipts_event_room_user_type UNIQUE (event_id, room_id, user_id, receipt_type)
);

CREATE INDEX IF NOT EXISTS idx_event_receipts_event ON event_receipts(event_id);
CREATE INDEX IF NOT EXISTS idx_event_receipts_room ON event_receipts(room_id);

-- event_reports 表
CREATE TABLE IF NOT EXISTS event_reports (
    id BIGSERIAL PRIMARY KEY,
    event_id TEXT NOT NULL,
    room_id TEXT NOT NULL,
    reporter_user_id TEXT NOT NULL,
    reported_user_id TEXT,
    event_json JSONB,
    reason TEXT,
    description TEXT,
    status TEXT DEFAULT 'open',
    score INTEGER DEFAULT 0,
    received_ts BIGINT NOT NULL,
    resolved_at BIGINT,
    resolved_by TEXT,
    resolution_reason TEXT
);

CREATE INDEX IF NOT EXISTS idx_event_reports_event ON event_reports(event_id);
CREATE INDEX IF NOT EXISTS idx_event_reports_room ON event_reports(room_id);
CREATE INDEX IF NOT EXISTS idx_event_reports_reporter ON event_reports(reporter_user_id);
CREATE INDEX IF NOT EXISTS idx_event_reports_status ON event_reports(status);

-- notifications 表
CREATE TABLE IF NOT EXISTS notifications (
    id BIGSERIAL PRIMARY KEY,
    user_id TEXT NOT NULL,
    event_id TEXT,
    room_id TEXT,
    ts BIGINT NOT NULL,
    notification_type VARCHAR(50) DEFAULT 'message',
    profile_tag VARCHAR(255),
    is_read BOOLEAN DEFAULT FALSE,
    created_ts BIGINT NOT NULL,
    updated_ts BIGINT
);

CREATE INDEX IF NOT EXISTS idx_notifications_user_id ON notifications(user_id);
CREATE INDEX IF NOT EXISTS idx_notifications_ts ON notifications(ts DESC);
CREATE INDEX IF NOT EXISTS idx_notifications_room ON notifications(room_id);

-- user_privacy_settings 表
CREATE TABLE IF NOT EXISTS user_privacy_settings (
    user_id VARCHAR(255) PRIMARY KEY,
    allow_presence_lookup BOOLEAN DEFAULT TRUE,
    allow_profile_lookup BOOLEAN DEFAULT TRUE,
    allow_room_invites BOOLEAN DEFAULT TRUE,
    created_ts BIGINT NOT NULL,
    updated_ts BIGINT
);

-- sliding_sync_rooms 表
CREATE TABLE IF NOT EXISTS sliding_sync_rooms (
    id BIGSERIAL PRIMARY KEY,
    user_id TEXT NOT NULL,
    device_id TEXT NOT NULL,
    room_id TEXT NOT NULL,
    conn_id TEXT,
    list_key TEXT,
    bump_stamp BIGINT DEFAULT 0,
    highlight_count INTEGER DEFAULT 0,
    notification_count INTEGER DEFAULT 0,
    is_dm BOOLEAN DEFAULT FALSE,
    is_encrypted BOOLEAN DEFAULT FALSE,
    is_tombstoned BOOLEAN DEFAULT FALSE,
    invited BOOLEAN DEFAULT FALSE,
    name TEXT,
    avatar TEXT,
    timestamp BIGINT DEFAULT 0,
    created_ts BIGINT NOT NULL,
    updated_ts BIGINT NOT NULL,
    CONSTRAINT uq_sliding_sync_room UNIQUE (user_id, device_id, room_id, COALESCE(conn_id, ''))
);

CREATE INDEX IF NOT EXISTS idx_sliding_sync_rooms_user_device ON sliding_sync_rooms(user_id, device_id);
CREATE INDEX IF NOT EXISTS idx_sliding_sync_rooms_bump_stamp ON sliding_sync_rooms(bump_stamp DESC);

-- sync_stream_id 表
CREATE TABLE IF NOT EXISTS sync_stream_id (
    id BIGSERIAL PRIMARY KEY,
    stream_type TEXT,
    last_id BIGINT DEFAULT 0,
    updated_ts BIGINT,
    CONSTRAINT uq_sync_stream_id_type UNIQUE (stream_type)
);

-- delayed_events 表
CREATE TABLE IF NOT EXISTS delayed_events (
    id SERIAL PRIMARY KEY,
    event_id TEXT NOT NULL UNIQUE,
    room_id TEXT NOT NULL,
    event_type TEXT NOT NULL,
    delay_ts BIGINT NOT NULL,
    expire_ts BIGINT,
    status TEXT DEFAULT 'pending',
    created_ts BIGINT NOT NULL
);

CREATE INDEX IF NOT EXISTS idx_delayed_events_room ON delayed_events(room_id);
CREATE INDEX IF NOT EXISTS idx_delayed_events_expire ON delayed_events(expire_ts) WHERE expire_ts IS NOT NULL;

-- server_retention_policy 表
CREATE TABLE IF NOT EXISTS server_retention_policy (
    id BIGSERIAL PRIMARY KEY,
    policy_name TEXT NOT NULL UNIQUE,
    min_lifetime_days INTEGER DEFAULT 90,
    max_lifetime_days INTEGER DEFAULT 365,
    allow_per_room_override BOOLEAN DEFAULT TRUE,
    is_default BOOLEAN DEFAULT FALSE,
    created_ts BIGINT NOT NULL,
    updated_ts BIGINT
);

CREATE INDEX IF NOT EXISTS idx_server_retention_policy_default ON server_retention_policy(is_default) WHERE is_default = TRUE;

-- user_stats 表
CREATE TABLE IF NOT EXISTS user_stats (
    id BIGSERIAL PRIMARY KEY,
    user_id TEXT NOT NULL UNIQUE,
    last_active_ts BIGINT,
    last_synced_ts BIGINT,
    messages_sent BIGINT DEFAULT 0,
    rooms_joined BIGINT DEFAULT 0,
    created_ts BIGINT NOT NULL,
    updated_ts BIGINT
);

CREATE INDEX IF NOT EXISTS idx_user_stats_user ON user_stats(user_id);

-- group_memberships 表
CREATE TABLE IF NOT EXISTS group_memberships (
    id BIGSERIAL PRIMARY KEY,
    group_id TEXT NOT NULL,
    user_id TEXT NOT NULL,
    is_admin BOOLEAN DEFAULT FALSE,
    is_public BOOLEAN DEFAULT FALSE,
    created_ts BIGINT NOT NULL,
    updated_ts BIGINT,
    UNIQUE (group_id, user_id)
);

CREATE INDEX IF NOT EXISTS idx_group_memberships_group ON group_memberships(group_id);
CREATE INDEX IF NOT EXISTS idx_group_memberships_user ON group_memberships(user_id);

-- user_external_ids 表
CREATE TABLE IF NOT EXISTS user_external_ids (
    id BIGSERIAL PRIMARY KEY,
    auth_id TEXT NOT NULL,
    auth_provider TEXT NOT NULL,
    user_id TEXT NOT NULL,
    created_ts BIGINT NOT NULL,
    UNIQUE (auth_provider, auth_id)
);

CREATE INDEX IF NOT EXISTS idx_user_external_ids_user ON user_external_ids(user_id);

-- login_tokens 表
CREATE TABLE IF NOT EXISTS login_tokens (
    id BIGSERIAL PRIMARY KEY,
    token_hash TEXT NOT NULL UNIQUE,
    user_id TEXT NOT NULL,
    expires_at BIGINT NOT NULL,
    used_at BIGINT,
    created_ts BIGINT NOT NULL
);

CREATE INDEX IF NOT EXISTS idx_login_tokens_hash ON login_tokens(token_hash);
CREATE INDEX IF NOT EXISTS idx_login_tokens_expires ON login_tokens(expires_at) WHERE expires_at > 0;

-- room_summary_members 表
CREATE TABLE IF NOT EXISTS room_summary_members (
    id BIGSERIAL PRIMARY KEY,
    room_id VARCHAR(255) NOT NULL,
    user_id VARCHAR(255) NOT NULL,
    display_name TEXT,
    avatar_url TEXT,
    membership VARCHAR(50) DEFAULT 'join',
    is_hero BOOLEAN DEFAULT false,
    last_active_ts BIGINT,
    updated_ts BIGINT DEFAULT EXTRACT(EPOCH FROM NOW())::BIGINT * 1000,
    created_ts BIGINT DEFAULT EXTRACT(EPOCH FROM NOW())::BIGINT * 1000
);

CREATE INDEX IF NOT EXISTS idx_room_summary_members_room ON room_summary_members(room_id);
CREATE UNIQUE INDEX IF NOT EXISTS idx_room_summary_members_user_room ON room_summary_members(user_id, room_id);

-- cross_signing_trust 表 (for E2EE)
CREATE TABLE IF NOT EXISTS cross_signing_trust (
    id BIGSERIAL PRIMARY KEY,
    user_id VARCHAR(255) NOT NULL,
    target_user_id VARCHAR(255) NOT NULL,
    master_key_id VARCHAR(255),
    is_trusted BOOLEAN DEFAULT FALSE,
    trusted_at TIMESTAMP WITH TIME ZONE,
    created_at TIMESTAMP WITH TIME ZONE DEFAULT CURRENT_TIMESTAMP,
    updated_at TIMESTAMP WITH TIME ZONE DEFAULT CURRENT_TIMESTAMP,
    UNIQUE (user_id, target_user_id)
);

CREATE INDEX IF NOT EXISTS idx_cross_signing_trust_target ON cross_signing_trust(target_user_id);

-- key_rotation_log 表
CREATE TABLE IF NOT EXISTS key_rotation_log (
    id BIGSERIAL PRIMARY KEY,
    user_id VARCHAR(255) NOT NULL,
    device_id VARCHAR(255) NOT NULL,
    room_id VARCHAR(255),
    rotation_type VARCHAR(50) NOT NULL,
    old_key_id VARCHAR(255),
    new_key_id VARCHAR(255),
    reason VARCHAR(255),
    rotated_at TIMESTAMP WITH TIME ZONE DEFAULT CURRENT_TIMESTAMP
);

CREATE INDEX IF NOT EXISTS idx_key_rotation_user_room ON key_rotation_log(user_id, room_id);
CREATE INDEX IF NOT EXISTS idx_key_rotation_at ON key_rotation_log(rotated_at);

-- e2ee_security_events 表
CREATE TABLE IF NOT EXISTS e2ee_security_events (
    id BIGSERIAL PRIMARY KEY,
    user_id VARCHAR(255) NOT NULL,
    device_id VARCHAR(255),
    event_type VARCHAR(100) NOT NULL,
    event_data JSONB,
    ip_address VARCHAR(45),
    user_agent VARCHAR(512),
    created_at TIMESTAMP WITH TIME ZONE DEFAULT CURRENT_TIMESTAMP
);

CREATE INDEX IF NOT EXISTS idx_e2ee_security_user_events ON e2ee_security_events(user_id, created_at);
CREATE INDEX IF NOT EXISTS idx_e2ee_security_type ON e2ee_security_events(event_type);

-- device_verification_request 表
CREATE TABLE IF NOT EXISTS device_verification_request (
    id BIGSERIAL PRIMARY KEY,
    user_id VARCHAR(255) NOT NULL,
    new_device_id VARCHAR(255) NOT NULL,
    requesting_device_id VARCHAR(255),
    verification_method VARCHAR(50) NOT NULL,
    status VARCHAR(50) NOT NULL DEFAULT 'pending',
    request_token VARCHAR(255) NOT NULL UNIQUE,
    commitment VARCHAR(255),
    pubkey VARCHAR(255),
    created_at TIMESTAMP WITH TIME ZONE DEFAULT CURRENT_TIMESTAMP,
    expires_at TIMESTAMP WITH TIME ZONE NOT NULL,
    completed_at TIMESTAMP WITH TIME ZONE,
    UNIQUE (user_id, new_device_id)
);

CREATE INDEX IF NOT EXISTS idx_verification_user_device ON device_verification_request(user_id, new_device_id);
CREATE INDEX IF NOT EXISTS idx_verification_status ON device_verification_request(status);

-- device_trust_status 表
CREATE TABLE IF NOT EXISTS device_trust_status (
    id BIGSERIAL PRIMARY KEY,
    user_id VARCHAR(255) NOT NULL,
    device_id VARCHAR(255) NOT NULL,
    trust_level VARCHAR(50) NOT NULL DEFAULT 'unverified',
    verified_by_device_id VARCHAR(255),
    verified_at TIMESTAMP WITH TIME ZONE,
    created_at TIMESTAMP WITH TIME ZONE DEFAULT CURRENT_TIMESTAMP,
    updated_at TIMESTAMP WITH TIME ZONE DEFAULT CURRENT_TIMESTAMP,
    UNIQUE (user_id, device_id)
);

CREATE INDEX IF NOT EXISTS idx_device_trust_user_trust ON device_trust_status(user_id, trust_level);
CREATE INDEX IF NOT EXISTS idx_device_trust_level ON device_trust_status(trust_level);

-- secure_key_backups 表
CREATE TABLE IF NOT EXISTS secure_key_backups (
    id BIGSERIAL PRIMARY KEY,
    user_id VARCHAR(255) NOT NULL,
    backup_id VARCHAR(255) NOT NULL,
    version VARCHAR(50) NOT NULL,
    algorithm VARCHAR(50) NOT NULL DEFAULT 'm.megolm_backup.v1.secure',
    auth_data JSONB NOT NULL,
    key_count BIGINT DEFAULT 0,
    created_at TIMESTAMP WITH TIME ZONE DEFAULT CURRENT_TIMESTAMP,
    updated_at TIMESTAMP WITH TIME ZONE DEFAULT CURRENT_TIMESTAMP,
    UNIQUE (user_id, backup_id)
);

CREATE INDEX IF NOT EXISTS idx_secure_backup_user ON secure_key_backups(user_id);

-- secure_backup_session_keys 表
CREATE TABLE IF NOT EXISTS secure_backup_session_keys (
    id BIGSERIAL PRIMARY KEY,
    user_id VARCHAR(255) NOT NULL,
    backup_id VARCHAR(255) NOT NULL,
    room_id VARCHAR(255) NOT NULL,
    session_id VARCHAR(255) NOT NULL,
    encrypted_key TEXT NOT NULL,
    created_at TIMESTAMP WITH TIME ZONE DEFAULT CURRENT_TIMESTAMP,
    UNIQUE (user_id, backup_id, room_id, session_id)
);

CREATE INDEX IF NOT EXISTS idx_secure_backup_keys_user ON secure_backup_session_keys(user_id, backup_id);

-- invites 表 (for invite blocking)
CREATE TABLE IF NOT EXISTS invites (
    id BIGSERIAL PRIMARY KEY,
    room_id TEXT NOT NULL,
    inviter TEXT NOT NULL,
    invitee TEXT NOT NULL,
    is_blocked BOOLEAN DEFAULT FALSE,
    blocked_reason TEXT,
    created_ts BIGINT NOT NULL,
    updated_ts BIGINT
);

CREATE INDEX IF NOT EXISTS idx_invites_room ON invites(room_id);
CREATE INDEX IF NOT EXISTS idx_invites_invitee ON invites(invitee);

-- invitation_blocks 表
CREATE TABLE IF NOT EXISTS invitation_blocks (
    id BIGSERIAL PRIMARY KEY,
    inviter TEXT NOT NULL,
    invitee TEXT NOT NULL,
    is_permanent BOOLEAN DEFAULT FALSE,
    expires_at BIGINT,
    reason TEXT,
    created_ts BIGINT NOT NULL
);

CREATE INDEX IF NOT EXISTS idx_invitation_blocks_inviter ON invitation_blocks(inviter);

-- ============================================================================
-- 第十二部分: 字段修复 (幂等)
-- ============================================================================

-- 修复 users 表字段名
DO $$
BEGIN
    IF column_exists('users', 'must_change_password') THEN
        ALTER TABLE users RENAME COLUMN must_change_password TO is_password_change_required;
        RAISE NOTICE 'Renamed must_change_password to is_password_change_required';
    END IF;
END $$;

-- 修复 users 表索引
DO $$
BEGIN
    IF index_exists('idx_users_must_change_password') THEN
        DROP INDEX IF EXISTS idx_users_must_change_password;
    END IF;
    CREATE INDEX IF NOT EXISTS idx_users_is_password_change_required
        ON users(is_password_change_required) WHERE is_password_change_required = TRUE;
END $$;

-- 修复 user_threepids 字段
DO $$
BEGIN
    IF column_exists('user_threepids', 'validated_ts') THEN
        IF NOT column_exists('user_threepids', 'validated_at') THEN
            ALTER TABLE user_threepids RENAME COLUMN validated_ts TO validated_at;
            RAISE NOTICE 'Renamed validated_ts to validated_at in user_threepids';
        ELSE
            ALTER TABLE user_threepids DROP COLUMN IF EXISTS validated_ts;
            RAISE NOTICE 'Dropped redundant validated_ts column';
        END IF;
    END IF;
END $$;

-- 修复 registration_tokens 字段
DO $$
BEGIN
    IF column_exists('registration_tokens', 'last_used_at') THEN
        IF NOT column_exists('registration_tokens', 'last_used_ts') THEN
            ALTER TABLE registration_tokens RENAME COLUMN last_used_at TO last_used_ts;
            RAISE NOTICE 'Renamed last_used_at to last_used_ts in registration_tokens';
        ELSE
            ALTER TABLE registration_tokens DROP COLUMN IF EXISTS last_used_at;
            RAISE NOTICE 'Dropped redundant last_used_at column';
        END IF;
    END IF;
END $$;

-- ============================================================================
-- 第十三部分: 数据验证
-- ============================================================================

DO $$
DECLARE
    error_count INTEGER := 0;
    warning_count INTEGER := 0;
BEGIN
    -- 检查必需表是否存在
    FOR table_name IN SELECT unnest(ARRAY[
        'users', 'devices', 'rooms', 'events', 'room_memberships',
        'access_tokens', 'refresh_tokens', 'device_keys', 'key_backups',
        'pushers', 'space_children', 'account_data'
    ]) LOOP
        IF NOT table_exists(table_name) THEN
            RAISE WARNING 'Missing required table: %', table_name;
            error_count := error_count + 1;
        END IF;
    END LOOP;

    -- 检查字段一致性
    IF column_exists('users', 'must_change_password') THEN
        RAISE WARNING 'users.must_change_password should be renamed to is_password_change_required';
        warning_count := warning_count + 1;
    END IF;

    IF column_exists('user_threepids', 'validated_ts') THEN
        RAISE WARNING 'user_threepids.validated_ts should be renamed to validated_at';
        warning_count := warning_count + 1;
    END IF;

    IF column_exists('registration_tokens', 'last_used_at') THEN
        RAISE WARNING 'registration_tokens.last_used_at should be renamed to last_used_ts';
        warning_count := warning_count + 1;
    END IF;

    -- 输出结果
    IF error_count = 0 AND warning_count = 0 THEN
        RAISE NOTICE '✓ Database migration completed successfully - all checks passed';
    ELSIF error_count = 0 THEN
        RAISE NOTICE '⚠ Migration completed with % warnings (non-critical)', warning_count;
    ELSE
        RAISE WARNING '⚠ Migration completed with % errors and % warnings', error_count, warning_count;
    END IF;
END $$;

-- ============================================================================
-- 第十四部分: 记录迁移
-- ============================================================================

INSERT INTO schema_migrations (version, name, description, applied_at, success)
VALUES (
    'UNIFIED_v1.0.0',
    'unified_migration',
    'Consolidated migration v1.0.0 - combines all incremental migrations',
    (EXTRACT(EPOCH FROM NOW()) * 1000)::BIGINT,
    TRUE
) ON CONFLICT (version) DO UPDATE SET
    applied_at = (EXTRACT(EPOCH FROM NOW()) * 1000)::BIGINT,
    success = TRUE,
    description = 'Consolidated migration v1.0.0 - combines all incremental migrations';

COMMIT;

-- ============================================================================
-- 回滚方案 (如需回滚)
-- ============================================================================
-- 注意: 回滚综合迁移非常危险，可能导致数据丢失
-- 强烈建议使用备份恢复，而非回滚
--
-- 回滚步骤:
-- 1. 使用备份恢复: pg_restore -U synapse -d synapse -c backup_full_YYYYMMDD.dump
-- 2. 不要尝试手动回滚各个变更
--
-- 禁止执行以下操作:
-- - DROP TABLE (会丢失数据)
-- - DROP COLUMN (会丢失数据)
-- - 删除索引 (会影响性能)
--
-- ============================================================================