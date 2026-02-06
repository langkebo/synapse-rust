-- =============================================================================
-- Synapse-Rust 数据库统一迁移脚本
-- 版本: 2.0
-- 创建日期: 2026-02-06
-- 更新日期: 2026-02-06
-- PostgreSQL版本: 15.x 兼容
-- 描述: 统一合并所有迁移文件，消除重复定义，确保Schema一致性
-- =============================================================================
-- 迁移执行顺序:
-- 1. 核心表 (Users, Devices, Auth Tokens)
-- 2. 房间和成员表
-- 3. 消息和事件表
-- 4. 社交功能表 (Friends, Private Chats)
-- 5. E2EE加密密钥表
-- 6. 媒体和语音消息表
-- 7. 系统功能表
-- =============================================================================

-- =============================================================================
-- 第一部分: 核心用户和认证表
-- =============================================================================

-- 用户表: 核心用户信息存储
CREATE TABLE IF NOT EXISTS users (
    user_id VARCHAR(255) NOT NULL PRIMARY KEY,
    username VARCHAR(255) NOT NULL UNIQUE,
    password_hash TEXT,
    is_admin BOOLEAN DEFAULT FALSE,
    is_guest BOOLEAN DEFAULT FALSE,
    consent_version VARCHAR(255),
    appservice_id VARCHAR(255),
    creation_ts BIGINT NOT NULL,
    user_type VARCHAR(50),
    deactivated BOOLEAN DEFAULT FALSE,
    shadow_banned BOOLEAN DEFAULT FALSE,
    generation BIGINT NOT NULL,
    avatar_url VARCHAR(512),
    displayname VARCHAR(255),
    invalid_update_ts BIGINT,
    migration_state VARCHAR(100),
    updated_ts BIGINT
);

-- 用户索引
CREATE INDEX IF NOT EXISTS idx_users_username ON users(username);
CREATE INDEX IF NOT EXISTS idx_users_creation_ts ON users(creation_ts);
CREATE INDEX IF NOT EXISTS idx_users_deactivated ON users(deactivated) WHERE deactivated = TRUE;

-- 设备表: 用户设备信息
CREATE TABLE IF NOT EXISTS devices (
    device_id VARCHAR(255) NOT NULL,
    user_id VARCHAR(255) NOT NULL,
    display_name VARCHAR(255),
    device_key JSONB,
    last_seen_ts BIGINT,
    last_seen_ip VARCHAR(255),
    created_at BIGINT NOT NULL,
    first_seen_ts BIGINT NOT NULL,
    created_ts BIGINT,
    appservice_id VARCHAR(255),
    ignored_user_list TEXT,
    PRIMARY KEY (device_id, user_id),
    FOREIGN KEY (user_id) REFERENCES users(user_id) ON DELETE CASCADE
);

-- 设备索引
CREATE INDEX IF NOT EXISTS idx_devices_user ON devices(user_id);
CREATE INDEX IF NOT EXISTS idx_devices_last_seen ON devices(last_seen_ts DESC);
CREATE INDEX IF NOT EXISTS idx_devices_created_at ON devices(created_at);

-- 访问令牌表
CREATE TABLE IF NOT EXISTS access_tokens (
    id BIGSERIAL PRIMARY KEY,
    token TEXT NOT NULL UNIQUE,
    user_id VARCHAR(255) NOT NULL,
    device_id VARCHAR(255),
    appservice_id VARCHAR(255),
    expires_ts BIGINT NOT NULL,
    created_ts BIGINT NOT NULL,
    last_used_ts BIGINT,
    user_agent TEXT,
    ip VARCHAR(255),
    invalidated_ts BIGINT,
    FOREIGN KEY (user_id) REFERENCES users(user_id) ON DELETE CASCADE
);

-- 访问令牌索引
CREATE INDEX IF NOT EXISTS idx_access_tokens_user ON access_tokens(user_id);
CREATE INDEX IF NOT EXISTS idx_access_tokens_expires ON access_tokens(expires_ts);
CREATE INDEX IF NOT EXISTS idx_access_tokens_token ON access_tokens(token);

-- 刷新令牌表
CREATE TABLE IF NOT EXISTS refresh_tokens (
    id BIGSERIAL PRIMARY KEY,
    token TEXT NOT NULL UNIQUE,
    user_id VARCHAR(255) NOT NULL,
    device_id VARCHAR(255) NOT NULL,
    expires_ts BIGINT NOT NULL,
    created_ts BIGINT NOT NULL,
    invalidated BOOLEAN DEFAULT FALSE,
    invalidated_ts BIGINT,
    FOREIGN KEY (user_id) REFERENCES users(user_id) ON DELETE CASCADE,
    FOREIGN KEY (device_id, user_id) REFERENCES devices(device_id, user_id) ON DELETE CASCADE
);

-- 刷新令牌索引
CREATE INDEX IF NOT EXISTS idx_refresh_tokens_user ON refresh_tokens(user_id);
CREATE INDEX IF NOT EXISTS idx_refresh_tokens_expires ON refresh_tokens(expires_ts);
CREATE INDEX IF NOT EXISTS idx_refresh_tokens_token ON refresh_tokens(token);

-- =============================================================================
-- 第二部分: 房间和成员表
-- =============================================================================

-- 房间表
CREATE TABLE IF NOT EXISTS rooms (
    room_id VARCHAR(255) NOT NULL PRIMARY KEY,
    creator VARCHAR(255) NOT NULL,
    is_public BOOLEAN DEFAULT FALSE,
    name VARCHAR(255),
    topic VARCHAR(512),
    avatar_url VARCHAR(512),
    join_rule VARCHAR(50) DEFAULT 'invite',
    history_visibility VARCHAR(50) DEFAULT 'shared',
    encryption VARCHAR(100),
    creation_ts BIGINT NOT NULL,
    last_activity_ts BIGINT NOT NULL,
    visibility VARCHAR(50) DEFAULT 'public',
    version VARCHAR(20) DEFAULT '1',
    member_count BIGINT DEFAULT 0,
    canonical_alias VARCHAR(255),
    FOREIGN KEY (creator) REFERENCES users(user_id) ON DELETE CASCADE
);

-- 房间索引
CREATE INDEX IF NOT EXISTS idx_rooms_creator ON rooms(creator);
CREATE INDEX IF NOT EXISTS idx_rooms_creation ON rooms(creation_ts);
CREATE INDEX IF NOT EXISTS idx_rooms_public ON rooms(is_public) WHERE is_public = TRUE;
CREATE INDEX IF NOT EXISTS idx_rooms_activity ON rooms(last_activity_ts DESC);
CREATE INDEX IF NOT EXISTS idx_rooms_join_rules ON rooms(join_rule);

-- 房间成员表
CREATE TABLE IF NOT EXISTS room_memberships (
    room_id VARCHAR(255) NOT NULL,
    user_id VARCHAR(255) NOT NULL,
    event_id VARCHAR(255) NOT NULL,
    event_type VARCHAR(255),
    membership VARCHAR(50) NOT NULL,
    sender VARCHAR(255),
    stream_ordering BIGINT,
    joined_ts BIGINT NOT NULL,
    updated_ts BIGINT,
    left_ts BIGINT,
    join_reason TEXT,
    invite_rate_count BIGINT DEFAULT 0,
    ban_reason TEXT,
    ban_ts BIGINT,
    banned_by VARCHAR(255),
    is_banned BOOLEAN DEFAULT FALSE,
    invite_token VARCHAR(255),
    reason TEXT,
    is_direct BOOLEAN DEFAULT FALSE,
    room_nickname VARCHAR(255),
    display_name VARCHAR(255),
    avatar_url VARCHAR(512),
    displayname VARCHAR(255),
    PRIMARY KEY (room_id, user_id),
    FOREIGN KEY (room_id) REFERENCES rooms(room_id) ON DELETE CASCADE,
    FOREIGN KEY (user_id) REFERENCES users(user_id) ON DELETE CASCADE
);

-- 房间成员索引
CREATE INDEX IF NOT EXISTS idx_memberships_room ON room_memberships(room_id);
CREATE INDEX IF NOT EXISTS idx_memberships_user ON room_memberships(user_id);
CREATE INDEX IF NOT EXISTS idx_memberships_room_membership ON room_memberships(room_id, membership);
CREATE INDEX IF NOT EXISTS idx_memberships_room_membership_joined ON room_memberships(room_id, membership, joined_ts DESC);
CREATE INDEX IF NOT EXISTS idx_memberships_joined_ts ON room_memberships(joined_ts);

-- =============================================================================
-- 第三部分: 消息和事件表
-- =============================================================================

-- 事件表
CREATE TABLE IF NOT EXISTS events (
    event_id VARCHAR(255) NOT NULL PRIMARY KEY,
    room_id VARCHAR(255) NOT NULL,
    event_type VARCHAR(255) NOT NULL,
    content JSONB NOT NULL DEFAULT '{}'::jsonb,
    sender VARCHAR(255) NOT NULL,
    unsigned JSONB NOT NULL DEFAULT '{}'::jsonb,
    redacted BOOLEAN DEFAULT FALSE,
    origin_server_ts BIGINT NOT NULL,
    state_key VARCHAR(255),
    depth BIGINT DEFAULT 0,
    processed_ts BIGINT,
    not_before BIGINT,
    status VARCHAR(50),
    reference_image VARCHAR(512),
    origin VARCHAR(50) DEFAULT 'self',
    user_id VARCHAR(255),
    FOREIGN KEY (room_id) REFERENCES rooms(room_id) ON DELETE CASCADE,
    FOREIGN KEY (sender) REFERENCES users(user_id) ON DELETE CASCADE
);

-- 事件索引
CREATE INDEX IF NOT EXISTS idx_events_room ON events(room_id);
CREATE INDEX IF NOT EXISTS idx_events_room_ts ON events(room_id, origin_server_ts DESC);
CREATE INDEX IF NOT EXISTS idx_events_room_type ON events(room_id, event_type);
CREATE INDEX IF NOT EXISTS idx_events_sender ON events(sender);
CREATE INDEX IF NOT EXISTS idx_events_ts ON events(origin_server_ts DESC);
CREATE INDEX IF NOT EXISTS idx_events_type ON events(event_type);

-- 事件回执表 (统一Schema)
CREATE TABLE IF NOT EXISTS event_receipts (
    id BIGSERIAL PRIMARY KEY,
    room_id VARCHAR(255) NOT NULL,
    receipt_type VARCHAR(64) NOT NULL DEFAULT 'm.read',
    event_id VARCHAR(255) NOT NULL,
    user_id VARCHAR(255) NOT NULL,
    receipt_data JSONB NOT NULL DEFAULT '{}',
    created_at BIGINT NOT NULL DEFAULT (EXTRACT(EPOCH FROM NOW()) * 1000)::BIGINT,
    CONSTRAINT uk_receipt UNIQUE (room_id, receipt_type, event_id, user_id)
);

-- 事件回执索引
CREATE INDEX IF NOT EXISTS idx_event_receipts_room ON event_receipts(room_id);
CREATE INDEX IF NOT EXISTS idx_event_receipts_user ON event_receipts(user_id);
CREATE INDEX IF NOT EXISTS idx_event_receipts_room_user ON event_receipts(room_id, user_id) WHERE receipt_type = 'm.read';
CREATE INDEX IF NOT EXISTS idx_receipt_latest ON event_receipts(room_id, receipt_type, user_id, created_at DESC) INCLUDE (event_id);

-- 房间别名表
CREATE TABLE IF NOT EXISTS room_aliases (
    room_alias VARCHAR(255) NOT NULL PRIMARY KEY,
    alias VARCHAR(255) NOT NULL,
    room_id VARCHAR(255) NOT NULL,
    created_by VARCHAR(255) NOT NULL,
    creation_ts BIGINT NOT NULL,
    FOREIGN KEY (room_id) REFERENCES rooms(room_id) ON DELETE CASCADE,
    FOREIGN KEY (created_by) REFERENCES users(user_id) ON DELETE CASCADE
);

-- 房间别名索引
CREATE INDEX IF NOT EXISTS idx_room_aliases_room ON room_aliases(room_id);
CREATE INDEX IF NOT EXISTS idx_room_aliases_creator ON room_aliases(created_by);

-- 房间授权链表
CREATE TABLE IF NOT EXISTS room_auth_chains (
    room_id VARCHAR(255) NOT NULL,
    event_id VARCHAR(255) NOT NULL,
    auth_event_id VARCHAR(255) NOT NULL,
    depth BIGINT NOT NULL,
    PRIMARY KEY (room_id, event_id, auth_event_id),
    FOREIGN KEY (room_id) REFERENCES rooms(room_id) ON DELETE CASCADE
);

-- =============================================================================
-- 第四部分: 社交功能表 (好友和私聊)
-- =============================================================================

-- 好友表
CREATE TABLE IF NOT EXISTS friends (
    user_id VARCHAR(255) NOT NULL,
    friend_id VARCHAR(255) NOT NULL,
    created_ts BIGINT NOT NULL,
    note TEXT,
    PRIMARY KEY (user_id, friend_id),
    FOREIGN KEY (user_id) REFERENCES users(user_id) ON DELETE CASCADE,
    FOREIGN KEY (friend_id) REFERENCES users(user_id) ON DELETE CASCADE,
    CHECK (user_id != friend_id)
);

-- 好友索引
CREATE INDEX IF NOT EXISTS idx_friends_user ON friends(user_id);
CREATE INDEX IF NOT EXISTS idx_friends_friend ON friends(friend_id);
CREATE INDEX IF NOT EXISTS idx_friends_created ON friends(created_ts);

-- 好友请求表
CREATE TABLE IF NOT EXISTS friend_requests (
    id BIGSERIAL PRIMARY KEY,
    from_user_id VARCHAR(255) NOT NULL,
    to_user_id VARCHAR(255) NOT NULL,
    created_ts BIGINT NOT NULL,
    updated_ts BIGINT,
    status VARCHAR(50) DEFAULT 'pending',
    message TEXT,
    hide BOOLEAN DEFAULT FALSE,
    FOREIGN KEY (from_user_id) REFERENCES users(user_id) ON DELETE CASCADE,
    FOREIGN KEY (to_user_id) REFERENCES users(user_id) ON DELETE CASCADE
);

-- 好友请求索引
CREATE INDEX IF NOT EXISTS idx_friend_requests_from ON friend_requests(from_user_id);
CREATE INDEX IF NOT EXISTS idx_friend_requests_to ON friend_requests(to_user_id);
CREATE INDEX IF NOT EXISTS idx_friend_requests_status ON friend_requests(status);
CREATE INDEX IF NOT EXISTS idx_friend_requests_created ON friend_requests(created_ts DESC);

-- 好友分类表
CREATE TABLE IF NOT EXISTS friend_categories (
    id BIGSERIAL PRIMARY KEY,
    user_id VARCHAR(255) NOT NULL,
    name VARCHAR(255) NOT NULL,
    color VARCHAR(20) DEFAULT '#3498db',
    icon VARCHAR(50),
    sort_order BIGINT DEFAULT 0,
    created_ts BIGINT NOT NULL,
    updated_ts BIGINT,
    FOREIGN KEY (user_id) REFERENCES users(user_id) ON DELETE CASCADE,
    UNIQUE (user_id, name)
);

-- 好友分类索引
CREATE INDEX IF NOT EXISTS idx_friend_categories_user ON friend_categories(user_id);
CREATE INDEX IF NOT EXISTS idx_friend_categories_sort ON friend_categories(user_id, sort_order);

-- 私聊会话表 (统一Schema - 修复版)
CREATE TABLE IF NOT EXISTS private_sessions (
    id BIGSERIAL PRIMARY KEY,
    session_id VARCHAR(255) NOT NULL UNIQUE,
    user_id_1 VARCHAR(255) NOT NULL,
    user_id_2 VARCHAR(255) NOT NULL,
    room_id VARCHAR(255),
    last_message_id VARCHAR(255),
    last_message_content TEXT,
    unread_count INTEGER NOT NULL DEFAULT 0,
    created_ts BIGINT NOT NULL DEFAULT (EXTRACT(EPOCH FROM NOW()) * 1000)::BIGINT,
    updated_ts BIGINT NOT NULL DEFAULT (EXTRACT(EPOCH FROM NOW()) * 1000)::BIGINT,
    last_activity_ts BIGINT,
    FOREIGN KEY (user_id_1) REFERENCES users(user_id) ON DELETE CASCADE,
    FOREIGN KEY (user_id_2) REFERENCES users(user_id) ON DELETE CASCADE
);

-- 私聊会话索引
CREATE INDEX IF NOT EXISTS idx_private_sessions_user ON private_sessions(user_id_1);
CREATE INDEX IF NOT EXISTS idx_private_sessions_other ON private_sessions(user_id_2);
CREATE INDEX IF NOT EXISTS idx_private_sessions_users ON private_sessions(user_id_1, user_id_2);
CREATE INDEX IF NOT EXISTS idx_private_sessions_updated ON private_sessions(updated_ts DESC);
CREATE INDEX IF NOT EXISTS idx_private_sessions_list ON private_sessions(user_id_1, updated_ts DESC);
CREATE INDEX IF NOT EXISTS idx_private_sessions_unread ON private_sessions(user_id_1, unread_count) WHERE unread_count > 0;

-- 私聊消息表 (统一Schema - 修复版)
CREATE TABLE IF NOT EXISTS private_messages (
    id BIGSERIAL PRIMARY KEY,
    message_id VARCHAR(255) NOT NULL UNIQUE,
    session_id VARCHAR(255) NOT NULL,
    sender_id VARCHAR(255) NOT NULL,
    content TEXT NOT NULL,
    content_type VARCHAR(128) NOT NULL DEFAULT 'm.text',
    is_read BOOLEAN NOT NULL DEFAULT FALSE,
    created_at BIGINT NOT NULL DEFAULT (EXTRACT(EPOCH FROM NOW()) * 1000)::BIGINT,
    read_at BIGINT,
    FOREIGN KEY (session_id) REFERENCES private_sessions(session_id) ON DELETE CASCADE,
    FOREIGN KEY (sender_id) REFERENCES users(user_id) ON DELETE CASCADE
);

-- 私聊消息索引
CREATE INDEX IF NOT EXISTS idx_private_messages_session ON private_messages(session_id);
CREATE INDEX IF NOT EXISTS idx_private_messages_created ON private_messages(created_at DESC);
CREATE INDEX IF NOT EXISTS idx_private_messages_sender ON private_messages(sender_id);
CREATE INDEX IF NOT EXISTS idx_private_messages_list ON private_messages(session_id, created_at DESC);
CREATE INDEX IF NOT EXISTS idx_private_messages_unread ON private_messages(session_id, is_read) WHERE is_read = FALSE;

-- 用户封禁表
CREATE TABLE IF NOT EXISTS user_blocks (
    id BIGSERIAL PRIMARY KEY,
    blocker_id VARCHAR(255) NOT NULL,
    blocked_id VARCHAR(255) NOT NULL,
    reason TEXT,
    created_at BIGINT NOT NULL DEFAULT (EXTRACT(EPOCH FROM NOW()) * 1000)::BIGINT,
    PRIMARY KEY (blocker_id, blocked_id),
    FOREIGN KEY (blocker_id) REFERENCES users(user_id) ON DELETE CASCADE,
    FOREIGN KEY (blocked_id) REFERENCES users(user_id) ON DELETE CASCADE,
    CHECK (blocker_id != blocked_id)
);

-- 用户封禁索引
CREATE INDEX IF NOT EXISTS idx_user_blocks_blocker ON user_blocks(blocker_id);
CREATE INDEX IF NOT EXISTS idx_user_blocks_blocked ON user_blocks(blocked_id);
CREATE INDEX IF NOT EXISTS idx_user_blocks_created ON user_blocks(created_at DESC);

-- 兼容层: blocked_users表 (供某些代码使用)
CREATE TABLE IF NOT EXISTS blocked_users (
    user_id VARCHAR(255) NOT NULL,
    blocked_user_id VARCHAR(255) NOT NULL,
    reason TEXT,
    created_ts BIGINT NOT NULL DEFAULT (EXTRACT(EPOCH FROM NOW()) * 1000)::BIGINT,
    PRIMARY KEY (user_id, blocked_user_id),
    FOREIGN KEY (user_id) REFERENCES users(user_id) ON DELETE CASCADE,
    FOREIGN KEY (blocked_user_id) REFERENCES users(user_id) ON DELETE CASCADE
);

-- blocked_users索引
CREATE INDEX IF NOT EXISTS idx_blocked_users_user ON blocked_users(user_id);
CREATE INDEX IF NOT EXISTS idx_blocked_users_blocked ON blocked_users(blocked_user_id);

-- =============================================================================
-- 第五部分: E2EE加密密钥表
-- =============================================================================

-- 设备密钥表
CREATE TABLE IF NOT EXISTS device_keys (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    user_id VARCHAR(255) NOT NULL,
    device_id VARCHAR(255) NOT NULL,
    display_name VARCHAR(255),
    algorithm VARCHAR(100),
    key_id VARCHAR(255) NOT NULL,
    public_key TEXT NOT NULL,
    signatures JSONB,
    created_at TIMESTAMP WITH TIME ZONE NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMP WITH TIME ZONE NOT NULL DEFAULT NOW(),
    ts_updated_ms BIGINT NOT NULL,
    key_json JSONB NOT NULL DEFAULT '{}',
    ts_added_ms BIGINT NOT NULL,
    ts_last_accessed BIGINT NOT NULL,
    verified BOOLEAN DEFAULT FALSE,
    blocked BOOLEAN DEFAULT FALSE,
    UNIQUE (user_id, device_id, key_id),
    FOREIGN KEY (device_id, user_id) REFERENCES devices(device_id, user_id) ON DELETE CASCADE
);

-- 设备密钥索引
CREATE INDEX IF NOT EXISTS idx_device_keys_user ON device_keys(user_id);
CREATE INDEX IF NOT EXISTS idx_device_keys_device ON device_keys(device_id);
CREATE INDEX IF NOT EXISTS idx_device_keys_key_id ON device_keys(key_id);
CREATE INDEX IF NOT EXISTS idx_device_keys_verified ON device_keys(verified) WHERE verified = TRUE;
CREATE INDEX IF NOT EXISTS idx_device_keys_ts ON device_keys(ts_last_accessed);

-- 一次性密钥表
CREATE TABLE IF NOT EXISTS one_time_keys (
    user_id VARCHAR(255) NOT NULL,
    device_id VARCHAR(255) NOT NULL,
    key_id VARCHAR(255) NOT NULL,
    key_json JSONB NOT NULL,
    ts_created_ms BIGINT NOT NULL,
    exhausted BOOLEAN DEFAULT FALSE,
    signature_json TEXT,
    PRIMARY KEY (user_id, device_id, key_id),
    FOREIGN KEY (device_id, user_id) REFERENCES devices(device_id, user_id) ON DELETE CASCADE
);

-- 一次性密钥索引
CREATE INDEX IF NOT EXISTS idx_one_time_keys_user ON one_time_keys(user_id);
CREATE INDEX IF NOT EXISTS idx_one_time_keys_device ON one_time_keys(device_id);
CREATE INDEX IF NOT EXISTS idx_one_time_keys_exhausted ON one_time_keys(exhausted) WHERE exhausted = FALSE;

-- 密钥备份表
CREATE TABLE IF NOT EXISTS key_backups (
    user_id VARCHAR(255) NOT NULL,
    backup_id VARCHAR(255) NOT NULL,
    session_id VARCHAR(255),
    version BIGINT NOT NULL,
    algorithm VARCHAR(100) NOT NULL,
    auth_key TEXT NOT NULL,
    mgmt_key TEXT NOT NULL,
    deleted BOOLEAN DEFAULT FALSE,
    backup_data JSONB NOT NULL,
    etag VARCHAR(255),
    PRIMARY KEY (user_id, backup_id),
    FOREIGN KEY (user_id) REFERENCES users(user_id) ON DELETE CASCADE
);

-- 密钥备份索引
CREATE INDEX IF NOT EXISTS idx_key_backups_user ON key_backups(user_id);
CREATE INDEX IF NOT EXISTS idx_key_backups_version ON key_backups(version);

-- 备份密钥表
CREATE TABLE IF NOT EXISTS backup_keys (
    user_id VARCHAR(255) NOT NULL,
    backup_id VARCHAR(255) NOT NULL,
    room_id VARCHAR(255) NOT NULL,
    session_id VARCHAR(255) NOT NULL,
    first_message_index BIGINT NOT NULL,
    forwarded_count BIGINT NOT NULL,
    is_verified BOOLEAN DEFAULT FALSE,
    backup_data JSONB NOT NULL,
    PRIMARY KEY (user_id, backup_id, room_id, session_id, first_message_index),
    FOREIGN KEY (user_id, backup_id) REFERENCES key_backups(user_id, backup_id) ON DELETE CASCADE
);

-- 备份密钥索引
CREATE INDEX IF NOT EXISTS idx_backup_keys_user ON backup_keys(user_id);
CREATE INDEX IF NOT EXISTS idx_backup_keys_room ON backup_keys(room_id);

-- 密钥版本表 (统一Schema)
CREATE TABLE IF NOT EXISTS room_key_versions (
    user_id VARCHAR(255) NOT NULL,
    version VARCHAR(255) NOT NULL,
    algorithm VARCHAR(255) NOT NULL,
    auth_data TEXT NOT NULL,
    secret TEXT,
    etag VARCHAR(64),
    created_at BIGINT NOT NULL DEFAULT (EXTRACT(EPOCH FROM NOW()) * 1000)::BIGINT,
    updated_at BIGINT NOT NULL DEFAULT (EXTRACT(EPOCH FROM NOW()) * 1000)::BIGINT,
    PRIMARY KEY (user_id, version),
    FOREIGN KEY (user_id) REFERENCES users(user_id) ON DELETE CASCADE
);

-- 密钥版本索引
CREATE INDEX IF NOT EXISTS idx_key_versions_user ON room_key_versions(user_id);
CREATE INDEX IF NOT EXISTS idx_key_versions_created ON room_key_versions(user_id, created_at DESC);

-- 密钥会话表 (统一Schema)
CREATE TABLE IF NOT EXISTS room_key_sessions (
    id BIGSERIAL PRIMARY KEY,
    user_id VARCHAR(255) NOT NULL,
    version VARCHAR(255) NOT NULL,
    room_id VARCHAR(255) NOT NULL,
    session_id VARCHAR(255) NOT NULL,
    first_message_index INTEGER NOT NULL DEFAULT 0,
    forwarded_count INTEGER NOT NULL DEFAULT 0,
    is_verified BOOLEAN NOT NULL DEFAULT FALSE,
    session_data TEXT NOT NULL,
    created_at BIGINT NOT NULL DEFAULT (EXTRACT(EPOCH FROM NOW()) * 1000)::BIGINT,
    updated_at BIGINT NOT NULL DEFAULT (EXTRACT(EPOCH FROM NOW()) * 1000)::BIGINT,
    PRIMARY KEY (user_id, version, room_id, session_id),
    FOREIGN KEY (user_id, version) REFERENCES room_key_versions(user_id, version) ON DELETE CASCADE
);

-- 密钥会话索引
CREATE INDEX IF NOT EXISTS idx_keys_sessions_user_version ON room_key_sessions(user_id, version);
CREATE INDEX IF NOT EXISTS idx_keys_sessions_room ON room_key_sessions(user_id, room_id);
CREATE INDEX IF NOT EXISTS idx_keys_sessions_session ON room_key_sessions(session_id);
CREATE INDEX IF NOT EXISTS idx_keys_room_version ON room_key_sessions(room_id, version, user_id);

-- 跨签名密钥表
CREATE TABLE IF NOT EXISTS cross_signing_keys (
    user_id VARCHAR(255) NOT NULL,
    key_type VARCHAR(50) NOT NULL,
    key_id VARCHAR(255) NOT NULL,
    key_json JSONB NOT NULL,
    ts_added_ms BIGINT NOT NULL,
    ts_updated_ms BIGINT NOT NULL,
    verified BOOLEAN DEFAULT FALSE,
    blocked BOOLEAN DEFAULT FALSE,
    PRIMARY KEY (user_id, key_type),
    FOREIGN KEY (user_id) REFERENCES users(user_id) ON DELETE CASCADE
);

-- 跨签名密钥索引
CREATE INDEX IF NOT EXISTS idx_cross_signing_user ON cross_signing_keys(user_id);
CREATE INDEX IF NOT EXISTS idx_cross_signing_type ON cross_signing_keys(key_type);

-- 签名表
CREATE TABLE IF NOT EXISTS signatures (
    user_id VARCHAR(255) NOT NULL,
    key_id VARCHAR(255) NOT NULL,
    target_user_id VARCHAR(255) NOT NULL,
    target_key_id VARCHAR(255) NOT NULL,
    signature_json JSONB NOT NULL,
    ts_added_ms BIGINT NOT NULL,
    PRIMARY KEY (user_id, key_id, target_user_id, target_key_id),
    FOREIGN KEY (user_id) REFERENCES users(user_id) ON DELETE CASCADE
);

-- 签名索引
CREATE INDEX IF NOT EXISTS idx_signatures_user ON signatures(user_id);
CREATE INDEX IF NOT EXISTS idx_signatures_target ON signatures(target_user_id);

-- =============================================================================
-- Megolm会话表 (E2EE房间密钥分发)
-- =============================================================================
CREATE TABLE IF NOT EXISTS megolm_sessions (
    id VARCHAR(255) NOT NULL PRIMARY KEY,
    session_id VARCHAR(255) NOT NULL UNIQUE,
    room_id VARCHAR(255) NOT NULL,
    sender_key VARCHAR(255) NOT NULL,
    session_key TEXT NOT NULL,
    algorithm VARCHAR(100) NOT NULL DEFAULT 'm.megolm.v1.aes-sha2',
    message_index BIGINT NOT NULL DEFAULT 0,
    created_at BIGINT NOT NULL,
    last_used_at BIGINT NOT NULL,
    expires_at BIGINT NOT NULL
);

-- Megolm会话索引
CREATE INDEX IF NOT EXISTS idx_megolm_session_id ON megolm_sessions(session_id);
CREATE INDEX IF NOT EXISTS idx_megolm_room ON megolm_sessions(room_id);
CREATE INDEX IF NOT EXISTS idx_megolm_sender_key ON megolm_sessions(sender_key);
CREATE INDEX IF NOT EXISTS idx_megolm_expires ON megolm_sessions(expires_at);

-- =============================================================================
-- 第六部分: 媒体和语音消息表
-- =============================================================================

-- 语音消息表 (统一Schema)
CREATE TABLE IF NOT EXISTS voice_messages (
    id BIGSERIAL PRIMARY KEY,
    message_id VARCHAR(255) NOT NULL UNIQUE,
    user_id VARCHAR(255) NOT NULL,
    room_id VARCHAR(255),
    session_id VARCHAR(255),
    file_path VARCHAR(512) NOT NULL,
    content_type VARCHAR(100) NOT NULL,
    duration_ms INT NOT NULL,
    file_size BIGINT NOT NULL,
    waveform_data TEXT,
    transcribe_text TEXT,
    created_ts BIGINT NOT NULL,
    FOREIGN KEY (user_id) REFERENCES users(user_id) ON DELETE CASCADE,
    FOREIGN KEY (room_id) REFERENCES rooms(room_id) ON DELETE SET NULL
);

-- 语音消息索引
CREATE INDEX IF NOT EXISTS idx_voice_user ON voice_messages(user_id);
CREATE INDEX IF NOT EXISTS idx_voice_room ON voice_messages(room_id);
CREATE INDEX IF NOT EXISTS idx_voice_created ON voice_messages(created_at DESC);
CREATE INDEX IF NOT EXISTS idx_voice_user_created ON voice_messages(user_id, created_at DESC);

-- 语音使用统计表
CREATE TABLE IF NOT EXISTS voice_usage_stats (
    id BIGSERIAL PRIMARY KEY,
    user_id VARCHAR(255) NOT NULL,
    date DATE NOT NULL,
    message_count INTEGER DEFAULT 0,
    total_duration_ms BIGINT DEFAULT 0,
    total_file_size BIGINT DEFAULT 0,
    created_ts BIGINT NOT NULL,
    updated_ts BIGINT NOT NULL,
    FOREIGN KEY (user_id) REFERENCES users(user_id) ON DELETE CASCADE,
    UNIQUE (user_id, date)
);

-- 语音使用统计索引
CREATE INDEX IF NOT EXISTS idx_voice_usage_user ON voice_usage_stats(user_id);
CREATE INDEX IF NOT EXISTS idx_voice_usage_date ON voice_usage_stats(date);

-- 媒体仓库表 (统一Schema)
CREATE TABLE IF NOT EXISTS media_repository (
    id BIGSERIAL PRIMARY KEY,
    media_id VARCHAR(255) NOT NULL UNIQUE,
    server_name VARCHAR(255) NOT NULL,
    user_id VARCHAR(255),
    content_type VARCHAR(128) NOT NULL,
    file_size_bytes BIGINT NOT NULL,
    file_path VARCHAR(512) NOT NULL,
    checksum VARCHAR(64),
    upload_name VARCHAR(255),
    created_at BIGINT NOT NULL DEFAULT (EXTRACT(EPOCH FROM NOW()) * 1000)::BIGINT,
    last_accessed_at BIGINT,
    quarantined BOOLEAN NOT NULL DEFAULT FALSE,
    deleted BOOLEAN NOT NULL DEFAULT FALSE,
    FOREIGN KEY (user_id) REFERENCES users(user_id) ON DELETE SET NULL
);

-- 媒体仓库索引
CREATE INDEX IF NOT EXISTS idx_media_server ON media_repository(server_name, media_id);
CREATE INDEX IF NOT EXISTS idx_media_user ON media_repository(user_id);
CREATE INDEX IF NOT EXISTS idx_media_created ON media_repository(created_at DESC);
CREATE INDEX IF NOT EXISTS idx_media_quarantined ON media_repository(quarantined) WHERE quarantined = TRUE;
CREATE INDEX IF NOT EXISTS idx_media_deleted ON media_repository(deleted) WHERE deleted = FALSE;

-- 媒体缩略图表
CREATE TABLE IF NOT EXISTS media_thumbnails (
    id BIGSERIAL PRIMARY KEY,
    media_id VARCHAR(255) NOT NULL,
    width INTEGER NOT NULL,
    height INTEGER NOT NULL,
    content_type VARCHAR(128) NOT NULL,
    file_size_bytes BIGINT NOT NULL,
    file_path VARCHAR(512) NOT NULL,
    created_at BIGINT NOT NULL DEFAULT (EXTRACT(EPOCH FROM NOW()) * 1000)::BIGINT,
    UNIQUE (media_id, width, height),
    FOREIGN KEY (media_id) REFERENCES media_repository(media_id) ON DELETE CASCADE
);

-- 媒体缩略图索引
CREATE INDEX IF NOT EXISTS idx_media_thumbnails_media ON media_thumbnails(media_id);

-- =============================================================================
-- 第七部分: 系统功能表
-- =============================================================================

-- 用户存在状态表
CREATE TABLE IF NOT EXISTS presence (
    id BIGSERIAL PRIMARY KEY,
    user_id VARCHAR(255) NOT NULL UNIQUE,
    presence VARCHAR(50) DEFAULT 'offline',
    status_msg VARCHAR(255),
    last_active_ts BIGINT NOT NULL,
    created_ts BIGINT DEFAULT EXTRACT(epoch FROM now())::bigint,
    updated_ts BIGINT,
    FOREIGN KEY (user_id) REFERENCES users(user_id) ON DELETE CASCADE
);

-- 用户存在状态索引
CREATE INDEX IF NOT EXISTS idx_presence_user ON presence(user_id);
CREATE INDEX IF NOT EXISTS idx_presence_status ON presence(presence);
CREATE INDEX IF NOT EXISTS idx_presence_last_active ON presence(last_active_ts DESC);

-- 房间目录表
CREATE TABLE IF NOT EXISTS room_directory (
    room_id VARCHAR(255) NOT NULL PRIMARY KEY,
    is_public BOOLEAN DEFAULT TRUE,
    name VARCHAR(255),
    topic VARCHAR(512),
    avatar_url VARCHAR(512),
    canonical_alias VARCHAR(255),
    member_count BIGINT DEFAULT 0,
    primary_category VARCHAR(100),
    searchable BOOLEAN DEFAULT TRUE,
    FOREIGN KEY (room_id) REFERENCES rooms(room_id) ON DELETE CASCADE
);

-- 房间目录索引
CREATE INDEX IF NOT EXISTS idx_room_directory_public ON room_directory(is_public) WHERE is_public = TRUE;
CREATE INDEX IF NOT EXISTS idx_room_directory_category ON room_directory(primary_category);

-- 用户目录表
CREATE TABLE IF NOT EXISTS user_directory (
    user_id VARCHAR(255) NOT NULL PRIMARY KEY,
    displayname VARCHAR(255),
    avatar_url VARCHAR(512),
    last_active_ts BIGINT,
    searchable BOOLEAN DEFAULT TRUE,
    UNIQUE (user_id),
    FOREIGN KEY (user_id) REFERENCES users(user_id) ON DELETE CASCADE
);

-- 用户目录索引
CREATE INDEX IF NOT EXISTS idx_user_directory_searchable ON user_directory(searchable) WHERE searchable = TRUE;
CREATE INDEX IF NOT EXISTS idx_user_directory_name ON user_directory(displayname);
CREATE INDEX IF NOT EXISTS idx_user_directory_active ON user_directory(last_active_ts DESC);

-- 用户目录搜索表
CREATE TABLE IF NOT EXISTS user_directory_search (
    user_id VARCHAR(255) NOT NULL PRIMARY KEY,
    ts_vector TSVECTOR,
    FOREIGN KEY (user_id) REFERENCES user_directory(user_id) ON DELETE CASCADE
);

-- 用户目录搜索索引
CREATE INDEX IF NOT EXISTS idx_user_directory_search_vector ON user_directory_search USING GIN(ts_vector);

-- 用户房间表
CREATE TABLE IF NOT EXISTS user_rooms (
    user_id VARCHAR(255) NOT NULL,
    room_id VARCHAR(255) NOT NULL,
    membership VARCHAR(50) NOT NULL,
    since_ts BIGINT NOT NULL,
    UNIQUE (user_id, room_id),
    FOREIGN KEY (user_id) REFERENCES users(user_id) ON DELETE CASCADE,
    FOREIGN KEY (room_id) REFERENCES rooms(room_id) ON DELETE CASCADE
);

-- 用户房间索引
CREATE INDEX IF NOT EXISTS idx_user_rooms_user ON user_rooms(user_id);
CREATE INDEX IF NOT EXISTS idx_user_rooms_room ON user_rooms(room_id);

-- 事件签名表
CREATE TABLE IF NOT EXISTS event_signatures (
    event_id VARCHAR(255) NOT NULL,
    algorithm VARCHAR(50) NOT NULL,
    key_id VARCHAR(255) NOT NULL,
    signature TEXT NOT NULL,
    PRIMARY KEY (event_id, algorithm, key_id),
    FOREIGN KEY (event_id) REFERENCES events(event_id) ON DELETE CASCADE
);

-- 事件签名索引
CREATE INDEX IF NOT EXISTS idx_event_signatures_event ON event_signatures(event_id);

-- 事件报告表
CREATE TABLE IF NOT EXISTS event_reports (
    id BIGSERIAL PRIMARY KEY,
    event_id VARCHAR(255) NOT NULL,
    room_id VARCHAR(255) NOT NULL,
    reporter_id VARCHAR(255) NOT NULL,
    reason VARCHAR(500),
    created_ts BIGINT NOT NULL,
    resolved_ts BIGINT,
    resolved_by VARCHAR(255),
    score INTEGER DEFAULT -100,
    FOREIGN KEY (event_id) REFERENCES events(event_id) ON DELETE CASCADE,
    FOREIGN KEY (reporter_id) REFERENCES users(user_id) ON DELETE CASCADE,
    FOREIGN KEY (room_id) REFERENCES rooms(room_id) ON DELETE CASCADE
);

-- 事件报告索引
CREATE INDEX IF NOT EXISTS idx_event_reports_event ON event_reports(event_id);
CREATE INDEX IF NOT EXISTS idx_event_reports_reporter ON event_reports(reporter_id);
CREATE INDEX IF NOT EXISTS idx_event_reports_room ON event_reports(room_id);

-- 设备消息表
CREATE TABLE IF NOT EXISTS to_device_messages (
    id BIGSERIAL PRIMARY KEY,
    user_id VARCHAR(255) NOT NULL,
    device_id VARCHAR(255) NOT NULL,
    message_type VARCHAR(255) NOT NULL,
    content JSONB NOT NULL,
    created_ts BIGINT NOT NULL,
    processed_ts BIGINT,
    FOREIGN KEY (device_id, user_id) REFERENCES devices(device_id, user_id) ON DELETE CASCADE
);

-- 设备消息索引
CREATE INDEX IF NOT EXISTS idx_to_device_user ON to_device_messages(user_id);
CREATE INDEX IF NOT EXISTS idx_to_device_device ON to_device_messages(device_id);
CREATE INDEX IF NOT EXISTS idx_to_device_created ON to_device_messages(created_ts DESC);

-- 打字状态表
CREATE TABLE IF NOT EXISTS typing (
    user_id VARCHAR(255) NOT NULL,
    room_id VARCHAR(255) NOT NULL,
    typing BOOLEAN DEFAULT FALSE,
    last_active_ts BIGINT NOT NULL,
    UNIQUE (user_id, room_id),
    FOREIGN KEY (user_id) REFERENCES users(user_id) ON DELETE CASCADE,
    FOREIGN KEY (room_id) REFERENCES rooms(room_id) ON DELETE CASCADE
);

-- 打字状态索引
CREATE INDEX IF NOT EXISTS idx_typing_user ON typing(user_id);
CREATE INDEX IF NOT EXISTS idx_typing_room ON typing(room_id);

-- 用户账户数据表
CREATE TABLE IF NOT EXISTS user_account_data (
    user_id VARCHAR(255) NOT NULL,
    account_data_type VARCHAR(255) NOT NULL,
    content JSONB NOT NULL,
    created_ts BIGINT NOT NULL,
    updated_ts BIGINT,
    UNIQUE (user_id, account_data_type),
    FOREIGN KEY (user_id) REFERENCES users(user_id) ON DELETE CASCADE
);

-- 用户账户数据索引
CREATE INDEX IF NOT EXISTS idx_user_account_data ON user_account_data(user_id);

-- 房间账户数据表
CREATE TABLE IF NOT EXISTS room_account_data (
    room_id VARCHAR(255) NOT NULL,
    account_data_type VARCHAR(255) NOT NULL,
    content JSONB NOT NULL,
    created_ts BIGINT NOT NULL,
    updated_ts BIGINT,
    UNIQUE (room_id, account_data_type),
    FOREIGN KEY (room_id) REFERENCES rooms(room_id) ON DELETE CASCADE
);

-- 房间账户数据索引
CREATE INDEX IF NOT EXISTS idx_room_account_data ON room_account_data(room_id);

-- 房间状态表
CREATE TABLE IF NOT EXISTS room_state (
    room_id VARCHAR(255) NOT NULL,
    event_type VARCHAR(255) NOT NULL,
    state_key VARCHAR(255) NOT NULL,
    event_id VARCHAR(255) NOT NULL,
    content JSONB NOT NULL,
    created_ts BIGINT NOT NULL,
    UNIQUE (room_id, event_type, state_key),
    FOREIGN KEY (room_id) REFERENCES rooms(room_id) ON DELETE CASCADE,
    FOREIGN KEY (event_id) REFERENCES events(event_id) ON DELETE CASCADE
);

-- 房间状态索引
CREATE INDEX IF NOT EXISTS idx_room_state_room ON room_state(room_id);
CREATE INDEX IF NOT EXISTS idx_room_state_type ON room_state(event_type);

-- 读取标记表
CREATE TABLE IF NOT EXISTS read_markers (
    id BIGSERIAL PRIMARY KEY,
    user_id VARCHAR(255) NOT NULL,
    room_id VARCHAR(255) NOT NULL,
    event_id VARCHAR(255) NOT NULL,
    stream_ordering BIGINT,
    created_ts BIGINT NOT NULL,
    FOREIGN KEY (user_id) REFERENCES users(user_id) ON DELETE CASCADE,
    FOREIGN KEY (room_id) REFERENCES rooms(room_id) ON DELETE CASCADE,
    UNIQUE (user_id, room_id)
);

-- 读取标记索引
CREATE INDEX IF NOT EXISTS idx_read_markers_user ON read_markers(user_id);
CREATE INDEX IF NOT EXISTS idx_read_markers_room ON read_markers(room_id);

-- IP阻止表
CREATE TABLE IF NOT EXISTS ip_blocks (
    id BIGSERIAL PRIMARY KEY,
    ip_range VARCHAR(50) NOT NULL UNIQUE,
    reason TEXT,
    blocked_ts BIGINT NOT NULL,
    expires_ts BIGINT,
    blocked_by VARCHAR(255)
);

-- IP阻止索引
CREATE INDEX IF NOT EXISTS idx_ip_blocks_range ON ip_blocks(ip_range);
CREATE INDEX IF NOT EXISTS idx_ip_blocks_expires ON ip_blocks(expires_ts) WHERE expires_ts IS NOT NULL;

-- IP信誉表
CREATE TABLE IF NOT EXISTS ip_reputation (
    ip VARCHAR(50) NOT NULL PRIMARY KEY,
    reputation_score INTEGER DEFAULT 0,
    last_updated_ts BIGINT NOT NULL,
    abuse_detected BOOLEAN DEFAULT FALSE,
    spam_detected BOOLEAN DEFAULT FALSE
);

-- IP信誉索引
CREATE INDEX IF NOT EXISTS idx_ip_reputation_score ON ip_reputation(reputation_score);
CREATE INDEX IF NOT EXISTS idx_ip_reputation_abuse ON ip_reputation(abuse_detected) WHERE abuse_detected = TRUE;

-- 联邦签名密钥表
CREATE TABLE IF NOT EXISTS federation_signing_keys (
    server_name VARCHAR(255) NOT NULL,
    key_id VARCHAR(255) NOT NULL,
    key_json JSONB NOT NULL,
    ts_added_ms BIGINT NOT NULL,
    ts_valid_until_ms BIGINT NOT NULL,
    verified BOOLEAN DEFAULT FALSE,
    PRIMARY KEY (server_name, key_id)
);

-- 联邦签名密钥索引
CREATE INDEX IF NOT EXISTS idx_federation_signing_server ON federation_signing_keys(server_name);
CREATE INDEX IF NOT EXISTS idx_federation_valid_until ON federation_signing_keys(ts_valid_until_ms);

-- 密钥变化表
CREATE TABLE IF NOT EXISTS key_changes (
    user_id VARCHAR(255) NOT NULL,
    room_id VARCHAR(255) NOT NULL,
    stream_id BIGINT NOT NULL,
    PRIMARY KEY (user_id, room_id, stream_id)
);

-- 密钥变化索引
CREATE INDEX IF NOT EXISTS idx_key_changes_user ON key_changes(user_id);
CREATE INDEX IF NOT EXISTS idx_key_changes_stream ON key_changes(stream_id);

-- 会话密钥表
CREATE TABLE IF NOT EXISTS session_keys (
    session_id VARCHAR(255) NOT NULL,
    session_key TEXT NOT NULL,
    room_id VARCHAR(255) NOT NULL,
    first_message_index INTEGER NOT NULL,
    is_verified BOOLEAN DEFAULT FALSE,
    uploaded_ts BIGINT NOT NULL,
    PRIMARY KEY (session_id, room_id, first_message_index)
);

-- 会话密钥索引
CREATE INDEX IF NOT EXISTS idx_session_keys_session ON session_keys(session_id);
CREATE INDEX IF NOT EXISTS idx_session_keys_room ON session_keys(room_id);

-- 安全事件表
CREATE TABLE IF NOT EXISTS security_events (
    id BIGSERIAL PRIMARY KEY,
    event_type VARCHAR(255) NOT NULL,
    severity VARCHAR(50) DEFAULT 'info',
    user_id VARCHAR(255),
    ip_address VARCHAR(255),
    user_agent TEXT,
    details TEXT,
    created_at BIGINT NOT NULL,
    resolved BOOLEAN DEFAULT FALSE,
    resolved_by VARCHAR(255),
    resolved_ts BIGINT
);

-- 安全事件索引
CREATE INDEX IF NOT EXISTS idx_security_events_user ON security_events(user_id);
CREATE INDEX IF NOT EXISTS idx_security_events_type ON security_events(event_type);
CREATE INDEX IF NOT EXISTS idx_security_events_created ON security_events(created_at DESC);
CREATE INDEX IF NOT EXISTS idx_security_events_unresolved ON security_events(resolved) WHERE resolved = FALSE;

-- 速率限制表
CREATE TABLE IF NOT EXISTS ratelimit (
    id BIGSERIAL PRIMARY KEY,
    user_id VARCHAR(255),
    ip_address VARCHAR(255),
    endpoint VARCHAR(255) NOT NULL,
    request_count INTEGER DEFAULT 0,
    window_start_ts BIGINT NOT NULL,
    window_size_ms BIGINT NOT NULL
);

-- 速率限制索引
CREATE INDEX IF NOT EXISTS idx_ratelimit_user ON ratelimit(user_id);
CREATE INDEX IF NOT EXISTS idx_ratelimit_ip ON ratelimit(ip_address);
CREATE INDEX IF NOT EXISTS idx_ratelimit_endpoint ON ratelimit(endpoint, window_start_ts);

-- 房间密钥分发表
CREATE TABLE IF NOT EXISTS room_key_distributions (
    id BIGSERIAL PRIMARY KEY,
    room_id VARCHAR(255) NOT NULL,
    event_id VARCHAR(255) NOT NULL,
    created_ts BIGINT NOT NULL,
    content JSONB NOT NULL,
    FOREIGN KEY (event_id) REFERENCES events(event_id) ON DELETE CASCADE,
    FOREIGN KEY (room_id) REFERENCES rooms(room_id) ON DELETE CASCADE
);

-- 房间密钥分发索引
CREATE INDEX IF NOT EXISTS idx_room_key_distribution_event ON room_key_distributions(event_id);
CREATE INDEX IF NOT EXISTS idx_room_key_distribution_room ON room_key_distributions(room_id);

-- 邮箱验证令牌表
CREATE TABLE IF NOT EXISTS email_verification_tokens (
    id BIGSERIAL PRIMARY KEY,
    user_id VARCHAR(255) NOT NULL,
    email VARCHAR(255) NOT NULL,
    token VARCHAR(255) NOT NULL UNIQUE,
    expires_ts BIGINT NOT NULL,
    used BOOLEAN DEFAULT FALSE,
    created_ts BIGINT NOT NULL,
    FOREIGN KEY (user_id) REFERENCES users(user_id) ON DELETE CASCADE
);

-- 邮箱验证令牌索引
CREATE INDEX IF NOT EXISTS idx_email_verification_user ON email_verification_tokens(user_id);
CREATE INDEX IF NOT EXISTS idx_email_verification_token ON email_verification_tokens(token);
CREATE INDEX IF NOT EXISTS idx_email_verification_expires ON email_verification_tokens(expires_ts);

-- 数据库元数据表
CREATE TABLE IF NOT EXISTS db_metadata (
    key VARCHAR(255) NOT NULL PRIMARY KEY,
    value TEXT NOT NULL,
    updated_ts BIGINT
);

-- SQLx迁移表
CREATE TABLE IF NOT EXISTS _sqlx_migrations (
    id BIGSERIAL PRIMARY KEY,
    checksum BYTEA NOT NULL,
    description TEXT NOT NULL,
    success BOOLEAN NOT NULL,
    start_time TIMESTAMPTZ NOT NULL,
    finish_time TIMESTAMPTZ NOT NULL,
    script_name TEXT NOT NULL
);

-- =============================================================================
-- 验证查询
-- =============================================================================
SELECT 'Migration completed successfully' as status, 
       COUNT(*) as table_count 
FROM information_schema.tables 
WHERE table_schema = 'public' 
AND table_name NOT LIKE 'pg_%' 
AND table_name NOT LIKE 'sql_%';

-- =============================================================================
-- 迁移完成
-- =============================================================================
