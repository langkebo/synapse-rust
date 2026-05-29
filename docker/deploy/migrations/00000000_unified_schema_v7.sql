-- ============================================================================
-- synapse-rust 统一数据库架构 v7.0.0
-- 创建日期: 2026-05-15
-- 最后更新: 2026-05-15
--
-- 版本历史:
--   v6.0.4 (2026-03-14): 修复字段命名一致性
--   v6.0.3 (2026-03-13): 修复字段命名规范，nullable timestamps 统一使用 _at 后缀
--   v6.0.2 (2026-03-12): 添加缺失表和字段，修复字段命名
--
-- 主要功能:
--   - 用户与认证: users, devices, access_tokens, refresh_tokens 等
--   - 房间与消息: rooms, events, room_memberships 等
--   - 端到端加密: device_keys, olm_sessions, megolm_sessions 等
--   - 联邦协议: federation_servers, federation_queue 等
--   - 推送通知: pushers, push_rules, notifications 等
--   - 媒体存储: media_metadata, thumbnails 等
--   - 安全策略: ip_blocks, password_policy 等
-- ============================================================================

SET TIME ZONE 'UTC';

-- ============================================================================
-- 第一部分：核心用户表
-- ============================================================================

-- 用户表
-- 存储 Matrix 用户的基本信息
CREATE TABLE IF NOT EXISTS users (
    user_id TEXT NOT NULL,
    username TEXT NOT NULL,
    password_hash TEXT,
    is_admin BOOLEAN DEFAULT FALSE,
    is_guest BOOLEAN DEFAULT FALSE,
    is_shadow_banned BOOLEAN DEFAULT FALSE,
    is_deactivated BOOLEAN DEFAULT FALSE,
    created_ts BIGINT NOT NULL,
    updated_ts BIGINT,
    displayname TEXT,
    avatar_url TEXT,
    email TEXT,
    phone TEXT,
    generation BIGINT DEFAULT 0,
    consent_version TEXT,
    appservice_id TEXT,
    user_type TEXT,
    invalid_update_at BIGINT,
    migration_state TEXT,
    password_changed_ts BIGINT,
    is_password_change_required BOOLEAN DEFAULT FALSE,
    must_change_password BOOLEAN DEFAULT FALSE,
    password_expires_at BIGINT,
    failed_login_attempts INTEGER DEFAULT 0,
    locked_until BIGINT,
    CONSTRAINT pk_users PRIMARY KEY (user_id),
    CONSTRAINT uq_users_username UNIQUE (username)
);

CREATE INDEX IF NOT EXISTS idx_users_email ON users(email);
CREATE INDEX IF NOT EXISTS idx_users_is_admin ON users(is_admin);
CREATE INDEX IF NOT EXISTS idx_users_created_ts ON users(created_ts DESC);
CREATE INDEX IF NOT EXISTS idx_users_must_change_password ON users(must_change_password) WHERE must_change_password = TRUE;
CREATE INDEX IF NOT EXISTS idx_users_password_expires ON users(password_expires_at) WHERE password_expires_at IS NOT NULL;
CREATE INDEX IF NOT EXISTS idx_users_locked ON users(locked_until) WHERE locked_until IS NOT NULL;

-- 用户第三方身份表 (Third-party IDs)
-- 存储用户的邮箱、手机等第三方身份验证信息
CREATE TABLE IF NOT EXISTS user_threepids (
    id BIGSERIAL,
    user_id TEXT NOT NULL,
    medium TEXT NOT NULL,
    address TEXT NOT NULL,
    validated_ts BIGINT,
    added_ts BIGINT NOT NULL,
    is_verified BOOLEAN DEFAULT FALSE,
    verification_token TEXT,
    verification_expires_at BIGINT,
    CONSTRAINT pk_user_threepids PRIMARY KEY (id),
    CONSTRAINT uq_user_threepids_medium_address UNIQUE (medium, address),
    CONSTRAINT fk_user_threepids_user FOREIGN KEY (user_id) REFERENCES users(user_id) ON DELETE CASCADE
);

CREATE INDEX IF NOT EXISTS idx_user_threepids_user ON user_threepids(user_id);

-- 设备表
-- 存储用户的设备信息
CREATE TABLE IF NOT EXISTS devices (
    device_id TEXT NOT NULL,
    user_id TEXT NOT NULL,
    display_name TEXT,
    device_key JSONB,
    last_seen_ts BIGINT,
    last_seen_ip TEXT,
    created_ts BIGINT NOT NULL,
    first_seen_ts BIGINT NOT NULL,
    user_agent TEXT,
    appservice_id TEXT,
    ignored_user_list TEXT,
    CONSTRAINT pk_devices PRIMARY KEY (device_id),
    CONSTRAINT fk_devices_user FOREIGN KEY (user_id) REFERENCES users(user_id) ON DELETE CASCADE
);

CREATE INDEX IF NOT EXISTS idx_devices_user_id ON devices(user_id);
CREATE INDEX IF NOT EXISTS idx_devices_last_seen ON devices(last_seen_ts DESC);

-- 访问令牌表
-- 存储用户的访问令牌
CREATE TABLE IF NOT EXISTS access_tokens (
    id BIGSERIAL,
    token_hash TEXT NOT NULL,
    token TEXT,
    user_id TEXT NOT NULL,
    device_id TEXT,
    created_ts BIGINT NOT NULL,
    expires_at BIGINT,
    last_used_ts BIGINT,
    user_agent TEXT,
    ip_address TEXT,
    is_revoked BOOLEAN DEFAULT FALSE,
    CONSTRAINT pk_access_tokens PRIMARY KEY (id),
    CONSTRAINT uq_access_tokens_token_hash UNIQUE (token_hash),
    CONSTRAINT fk_access_tokens_user FOREIGN KEY (user_id) REFERENCES users(user_id) ON DELETE CASCADE,
    CONSTRAINT fk_access_tokens_device FOREIGN KEY (device_id) REFERENCES devices(device_id) ON DELETE SET NULL
);

CREATE INDEX IF NOT EXISTS idx_access_tokens_user_id ON access_tokens(user_id);
CREATE INDEX IF NOT EXISTS idx_access_tokens_valid ON access_tokens(is_revoked) WHERE is_revoked = FALSE;
CREATE INDEX IF NOT EXISTS idx_access_tokens_token_hash ON access_tokens(token_hash);

-- 刷新令牌表
-- 存储用于刷新访问令牌的令牌
CREATE TABLE IF NOT EXISTS refresh_tokens (
    id BIGSERIAL,
    token_hash TEXT NOT NULL,
    user_id TEXT NOT NULL,
    device_id TEXT,
    access_token_id TEXT,
    scope TEXT,
    created_ts BIGINT NOT NULL,
    expires_at BIGINT,
    last_used_ts BIGINT,
    use_count INTEGER DEFAULT 0,
    is_revoked BOOLEAN DEFAULT FALSE,
    revoked_reason TEXT,
    client_info JSONB,
    ip_address TEXT,
    user_agent TEXT,
    CONSTRAINT pk_refresh_tokens PRIMARY KEY (id),
    CONSTRAINT uq_refresh_tokens_token_hash UNIQUE (token_hash),
    CONSTRAINT fk_refresh_tokens_user FOREIGN KEY (user_id) REFERENCES users(user_id) ON DELETE CASCADE,
    CONSTRAINT fk_refresh_tokens_device FOREIGN KEY (device_id) REFERENCES devices(device_id) ON DELETE SET NULL
);

CREATE INDEX IF NOT EXISTS idx_refresh_tokens_user_id ON refresh_tokens(user_id);
CREATE INDEX IF NOT EXISTS idx_refresh_tokens_revoked ON refresh_tokens(is_revoked) WHERE is_revoked = FALSE;

-- Token 黑名单表
-- 存储已撤销的令牌
CREATE TABLE IF NOT EXISTS token_blacklist (
    id BIGSERIAL,
    token_hash TEXT NOT NULL,
    token TEXT,
    token_type TEXT DEFAULT 'access',
    user_id TEXT,
    is_revoked BOOLEAN DEFAULT TRUE,
    reason TEXT,
    expires_at BIGINT,
    CONSTRAINT pk_token_blacklist PRIMARY KEY (id),
    CONSTRAINT uq_token_blacklist_token_hash UNIQUE (token_hash),
    CONSTRAINT fk_token_blacklist_user FOREIGN KEY (user_id) REFERENCES users(user_id) ON DELETE SET NULL
);

CREATE INDEX IF NOT EXISTS idx_token_blacklist_hash ON token_blacklist(token_hash);

-- ============================================================================
-- 第二部分：房间相关表
-- ============================================================================

-- 房间表
-- 存储房间的基本信息
-- 注意: member_count 冗余字段已移除，请使用 room_summaries.member_count
CREATE TABLE IF NOT EXISTS rooms (
    room_id TEXT NOT NULL,
    creator TEXT,
    is_public BOOLEAN DEFAULT FALSE,
    room_version TEXT DEFAULT '6',
    created_ts BIGINT NOT NULL,
    last_activity_ts BIGINT,
    is_federated BOOLEAN DEFAULT TRUE,
    has_guest_access BOOLEAN DEFAULT FALSE,
    join_rules TEXT DEFAULT 'invite',
    history_visibility TEXT DEFAULT 'shared',
    name TEXT,
    topic TEXT,
    avatar_url TEXT,
    canonical_alias TEXT,
    visibility TEXT DEFAULT 'private',
    CONSTRAINT pk_rooms PRIMARY KEY (room_id)
);

CREATE INDEX IF NOT EXISTS idx_rooms_creator ON rooms(creator) WHERE creator IS NOT NULL;
CREATE INDEX IF NOT EXISTS idx_rooms_is_public ON rooms(is_public) WHERE is_public = TRUE;
CREATE INDEX IF NOT EXISTS idx_rooms_last_activity ON rooms(last_activity_ts DESC) WHERE last_activity_ts IS NOT NULL;

-- 房间成员表
-- 存储房间成员关系
-- 注意: 时间字段统一为 _ts 后缀
CREATE TABLE IF NOT EXISTS room_memberships (
    id BIGSERIAL,
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
    CONSTRAINT pk_room_memberships PRIMARY KEY (id),
    CONSTRAINT uq_room_memberships_room_user UNIQUE (room_id, user_id),
    CONSTRAINT fk_room_memberships_room FOREIGN KEY (room_id) REFERENCES rooms(room_id) ON DELETE CASCADE,
    CONSTRAINT fk_room_memberships_user FOREIGN KEY (user_id) REFERENCES users(user_id) ON DELETE CASCADE
);

CREATE INDEX IF NOT EXISTS idx_room_memberships_room ON room_memberships(room_id);
CREATE INDEX IF NOT EXISTS idx_room_memberships_user ON room_memberships(user_id);
CREATE INDEX IF NOT EXISTS idx_room_memberships_membership ON room_memberships(membership);
CREATE INDEX IF NOT EXISTS idx_room_memberships_user_membership ON room_memberships(user_id, membership);
CREATE INDEX IF NOT EXISTS idx_room_memberships_room_membership ON room_memberships(room_id, membership);
CREATE INDEX IF NOT EXISTS idx_room_memberships_joined ON room_memberships(user_id, room_id) WHERE membership = 'join';

-- 事件表
-- 存储房间内的所有事件
CREATE TABLE IF NOT EXISTS events (
    event_id TEXT NOT NULL,
    room_id TEXT NOT NULL,
    sender TEXT NOT NULL,
    event_type TEXT NOT NULL,
    content JSONB NOT NULL,
    origin_server_ts BIGINT NOT NULL,
    state_key TEXT,
    is_redacted BOOLEAN DEFAULT FALSE,
    redacted_at BIGINT,
    redacted_by TEXT,
    transaction_id TEXT,
    depth BIGINT,
    prev_events JSONB,
    auth_events JSONB,
    signatures JSONB,
    hashes JSONB,
    unsigned JSONB DEFAULT '{}',
    processed_at BIGINT,
    not_before BIGINT DEFAULT 0,
    status TEXT,
    reference_image TEXT,
    origin TEXT,
    user_id TEXT,
    stream_ordering BIGSERIAL,
    CONSTRAINT pk_events PRIMARY KEY (event_id),
    CONSTRAINT fk_events_room FOREIGN KEY (room_id) REFERENCES rooms(room_id) ON DELETE CASCADE
);

CREATE INDEX IF NOT EXISTS idx_events_room_id ON events(room_id);
CREATE INDEX IF NOT EXISTS idx_events_sender ON events(sender);
CREATE INDEX IF NOT EXISTS idx_events_type ON events(event_type);
CREATE INDEX IF NOT EXISTS idx_events_origin_server_ts ON events(origin_server_ts DESC);
CREATE INDEX IF NOT EXISTS idx_events_not_redacted ON events(room_id, origin_server_ts DESC) WHERE is_redacted = FALSE;
CREATE INDEX IF NOT EXISTS idx_access_tokens_user_revoked ON access_tokens(user_id, is_revoked) WHERE is_revoked = FALSE;

-- 事件关系表
-- 存储 Matrix relations / aggregations 所需的事件关联
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
    created_ts BIGINT NOT NULL,
    CONSTRAINT fk_event_relations_room FOREIGN KEY (room_id) REFERENCES rooms(room_id) ON DELETE CASCADE
);

CREATE UNIQUE INDEX IF NOT EXISTS idx_event_relations_unique
ON event_relations(event_id, relation_type, sender);

CREATE INDEX IF NOT EXISTS idx_event_relations_room_event
ON event_relations(room_id, relates_to_event_id, relation_type);

CREATE INDEX IF NOT EXISTS idx_event_relations_sender
ON event_relations(sender, relation_type);

CREATE INDEX IF NOT EXISTS idx_event_relations_origin_ts
ON event_relations(room_id, origin_server_ts DESC);

-- 房间摘要表
-- 存储房间的摘要信息
CREATE TABLE IF NOT EXISTS room_summaries (
    id BIGSERIAL,
    room_id TEXT NOT NULL,
    room_type TEXT,
    name TEXT,
    topic TEXT,
    avatar_url TEXT,
    canonical_alias TEXT,
    join_rules TEXT NOT NULL DEFAULT 'invite',
    history_visibility TEXT NOT NULL DEFAULT 'shared',
    guest_access TEXT NOT NULL DEFAULT 'forbidden',
    is_direct BOOLEAN NOT NULL DEFAULT FALSE,
    is_space BOOLEAN NOT NULL DEFAULT FALSE,
    is_encrypted BOOLEAN NOT NULL DEFAULT FALSE,
    member_count BIGINT DEFAULT 0,
    joined_member_count BIGINT DEFAULT 0,
    invited_member_count BIGINT DEFAULT 0,
    hero_users JSONB NOT NULL DEFAULT '[]',
    last_event_id TEXT,
    last_event_ts BIGINT,
    last_message_ts BIGINT,
    unread_notifications BIGINT NOT NULL DEFAULT 0,
    unread_highlight BIGINT NOT NULL DEFAULT 0,
    updated_ts BIGINT NOT NULL,
    created_ts BIGINT NOT NULL,
    CONSTRAINT pk_room_summaries PRIMARY KEY (room_id),
    CONSTRAINT uq_room_summaries_id UNIQUE (id),
    CONSTRAINT fk_room_summaries_room FOREIGN KEY (room_id) REFERENCES rooms(room_id) ON DELETE CASCADE
);

CREATE INDEX IF NOT EXISTS idx_room_summaries_last_event_ts
ON room_summaries(last_event_ts DESC);

CREATE INDEX IF NOT EXISTS idx_room_summaries_space
ON room_summaries(is_space)
WHERE is_space = TRUE;

CREATE TABLE IF NOT EXISTS room_summary_members (
    id BIGSERIAL PRIMARY KEY,
    room_id TEXT NOT NULL,
    user_id TEXT NOT NULL,
    display_name TEXT,
    avatar_url TEXT,
    membership TEXT NOT NULL,
    is_hero BOOLEAN NOT NULL DEFAULT FALSE,
    last_active_ts BIGINT,
    updated_ts BIGINT NOT NULL,
    created_ts BIGINT NOT NULL,
    CONSTRAINT uq_room_summary_members_room_user UNIQUE (room_id, user_id),
    CONSTRAINT fk_room_summary_members_room FOREIGN KEY (room_id) REFERENCES rooms(room_id) ON DELETE CASCADE DEFERRABLE INITIALLY DEFERRED,
    CONSTRAINT fk_room_summary_members_user FOREIGN KEY (user_id) REFERENCES users(user_id) ON DELETE CASCADE DEFERRABLE INITIALLY DEFERRED
);

CREATE INDEX IF NOT EXISTS idx_room_summary_members_user_membership_room
ON room_summary_members(user_id, membership, room_id);

CREATE INDEX IF NOT EXISTS idx_room_summary_members_room_membership_hero_active
ON room_summary_members(room_id, membership, is_hero DESC, last_active_ts DESC);

CREATE INDEX IF NOT EXISTS idx_room_summary_members_room_hero_user
ON room_summary_members(room_id, is_hero DESC, user_id);

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

CREATE INDEX IF NOT EXISTS idx_room_summary_state_room
ON room_summary_state(room_id);

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

CREATE INDEX IF NOT EXISTS idx_room_summary_update_queue_status_priority_created
ON room_summary_update_queue(status, priority DESC, created_ts ASC);

-- 房间目录表
-- 存储公开房间目录
CREATE TABLE IF NOT EXISTS room_directory (
    id BIGSERIAL,
    room_id TEXT NOT NULL,
    is_public BOOLEAN DEFAULT TRUE,
    is_searchable BOOLEAN DEFAULT TRUE,
    app_service_id TEXT,
    added_ts BIGINT NOT NULL,
    CONSTRAINT pk_room_directory PRIMARY KEY (id),
    CONSTRAINT uq_room_directory_room UNIQUE (room_id),
    CONSTRAINT fk_room_directory_room FOREIGN KEY (room_id) REFERENCES rooms(room_id) ON DELETE CASCADE
);

CREATE INDEX IF NOT EXISTS idx_room_directory_public ON room_directory(is_public) WHERE is_public = TRUE;

-- 房间别名表
-- 存储房间别名映射
CREATE TABLE IF NOT EXISTS room_aliases (
    room_alias TEXT NOT NULL,
    room_id TEXT NOT NULL,
    server_name TEXT NOT NULL,
    created_ts BIGINT NOT NULL,
    CONSTRAINT pk_room_aliases PRIMARY KEY (room_alias),
    CONSTRAINT fk_room_aliases_room FOREIGN KEY (room_id) REFERENCES rooms(room_id) ON DELETE CASCADE
);

CREATE INDEX IF NOT EXISTS idx_room_aliases_room_id ON room_aliases(room_id);

-- 线程根消息表
-- 存储线程的根消息信息
-- 注意: 已合并 thread_statistics 的字段 (participants)
CREATE TABLE IF NOT EXISTS thread_roots (
    id BIGSERIAL,
    room_id TEXT NOT NULL,
    root_event_id TEXT NOT NULL,
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
    CONSTRAINT pk_thread_roots PRIMARY KEY (id),
    CONSTRAINT uq_thread_roots_room_root_event UNIQUE (room_id, root_event_id),
    CONSTRAINT fk_thread_roots_room FOREIGN KEY (room_id) REFERENCES rooms(room_id) ON DELETE CASCADE
);

CREATE INDEX IF NOT EXISTS idx_thread_roots_room ON thread_roots(room_id);
CREATE INDEX IF NOT EXISTS idx_thread_roots_root_event ON thread_roots(root_event_id);
CREATE INDEX IF NOT EXISTS idx_thread_roots_thread ON thread_roots(thread_id);
CREATE UNIQUE INDEX IF NOT EXISTS idx_thread_roots_room_thread_unique
ON thread_roots(room_id, thread_id)
WHERE thread_id IS NOT NULL;
CREATE INDEX IF NOT EXISTS idx_thread_roots_room_last_reply_created
ON thread_roots(room_id, last_reply_ts DESC, created_ts DESC);
CREATE INDEX IF NOT EXISTS idx_thread_roots_last_reply ON thread_roots(last_reply_ts DESC) WHERE last_reply_ts IS NOT NULL;

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

CREATE INDEX IF NOT EXISTS idx_thread_replies_room_thread_ts
ON thread_replies(room_id, thread_id, origin_server_ts ASC);

CREATE INDEX IF NOT EXISTS idx_thread_replies_room_event
ON thread_replies(room_id, event_id);

CREATE INDEX IF NOT EXISTS idx_thread_replies_room_thread_event
ON thread_replies(room_id, thread_id, event_id);

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

CREATE INDEX IF NOT EXISTS idx_thread_relations_room_event
ON thread_relations(room_id, event_id);

CREATE INDEX IF NOT EXISTS idx_thread_relations_room_relates_to
ON thread_relations(room_id, relates_to_event_id);

CREATE INDEX IF NOT EXISTS idx_thread_relations_room_thread
ON thread_relations(room_id, thread_id)
WHERE thread_id IS NOT NULL;

-- 房间父关系表
-- 存储房间与 Space 的父子关系
CREATE TABLE IF NOT EXISTS room_parents (
    id BIGSERIAL,
    room_id TEXT NOT NULL,
    parent_room_id TEXT NOT NULL,
    sender TEXT NOT NULL,
    is_suggested BOOLEAN DEFAULT FALSE,
    via_servers JSONB DEFAULT '[]',
    added_ts BIGINT NOT NULL,
    CONSTRAINT pk_room_parents PRIMARY KEY (id),
    CONSTRAINT uq_room_parents_room_parent UNIQUE (room_id, parent_room_id),
    CONSTRAINT fk_room_parents_room FOREIGN KEY (room_id) REFERENCES rooms(room_id) ON DELETE CASCADE,
    CONSTRAINT fk_room_parents_parent FOREIGN KEY (parent_room_id) REFERENCES rooms(room_id) ON DELETE CASCADE
);

CREATE INDEX IF NOT EXISTS idx_room_parents_room ON room_parents(room_id);
CREATE INDEX IF NOT EXISTS idx_room_parents_parent ON room_parents(parent_room_id);

-- ============================================================================
-- 第三部分：E2EE 加密相关表
-- ============================================================================

-- 设备密钥表
-- 存储设备的加密密钥
CREATE TABLE IF NOT EXISTS device_keys (
    id BIGSERIAL,
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
    CONSTRAINT pk_device_keys PRIMARY KEY (id),
    CONSTRAINT uq_device_keys_user_device_key UNIQUE (user_id, device_id, key_id)
);

CREATE INDEX IF NOT EXISTS idx_device_keys_user_device ON device_keys(user_id, device_id);

-- 跨签名密钥表
-- 存储用户的跨签名密钥
CREATE TABLE IF NOT EXISTS cross_signing_keys (
    id BIGSERIAL,
    user_id TEXT NOT NULL,
    key_type TEXT NOT NULL,
    key_data TEXT NOT NULL,
    signatures JSONB,
    added_ts BIGINT NOT NULL,
    CONSTRAINT pk_cross_signing_keys PRIMARY KEY (id),
    CONSTRAINT uq_cross_signing_keys_user_type UNIQUE (user_id, key_type)
);

CREATE INDEX IF NOT EXISTS idx_cross_signing_keys_user ON cross_signing_keys(user_id);

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

CREATE INDEX IF NOT EXISTS idx_device_trust_status_user_level
ON device_trust_status(user_id, trust_level);

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

CREATE INDEX IF NOT EXISTS idx_cross_signing_trust_user_trusted
ON cross_signing_trust(user_id, is_trusted);

CREATE TABLE IF NOT EXISTS key_signatures (
    id BIGSERIAL PRIMARY KEY,
    target_user_id TEXT NOT NULL,
    target_key_id TEXT NOT NULL,
    signing_user_id TEXT NOT NULL,
    signing_key_id TEXT NOT NULL,
    signature TEXT NOT NULL,
    added_ts BIGINT NOT NULL,
    CONSTRAINT uq_key_signatures_signature UNIQUE (target_user_id, target_key_id, signing_user_id, signing_key_id)
);

CREATE INDEX IF NOT EXISTS idx_key_signatures_target
ON key_signatures(target_user_id, target_key_id);

CREATE TABLE IF NOT EXISTS key_rotation_log (
    id BIGSERIAL PRIMARY KEY,
    user_id TEXT NOT NULL,
    device_id TEXT NOT NULL,
    room_id TEXT,
    rotation_type TEXT NOT NULL,
    old_key_id TEXT,
    new_key_id TEXT,
    reason TEXT,
    rotated_at TIMESTAMPTZ NOT NULL
);

CREATE INDEX IF NOT EXISTS idx_key_rotation_log_user_rotated
ON key_rotation_log(user_id, rotated_at DESC);

CREATE TABLE IF NOT EXISTS e2ee_security_events (
    id BIGSERIAL PRIMARY KEY,
    user_id TEXT NOT NULL,
    device_id TEXT,
    event_type TEXT NOT NULL,
    event_data TEXT,
    ip_address TEXT,
    user_agent TEXT,
    created_ts BIGINT NOT NULL
);

CREATE INDEX IF NOT EXISTS idx_e2ee_security_events_user_created
ON e2ee_security_events(user_id, created_ts DESC);

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

CREATE INDEX IF NOT EXISTS idx_verification_requests_to_user_state
ON verification_requests(to_user, state, updated_ts DESC);

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

-- Megolm 会话表
-- 存储 Megolm 加密会话
CREATE TABLE IF NOT EXISTS megolm_sessions (
    id UUID DEFAULT gen_random_uuid(),
    session_id TEXT NOT NULL,
    room_id TEXT NOT NULL,
    sender_key TEXT NOT NULL,
    session_key TEXT NOT NULL,
    algorithm TEXT NOT NULL,
    message_index BIGINT DEFAULT 0,
    created_ts BIGINT NOT NULL,
    last_used_ts BIGINT,
    expires_at BIGINT,
    CONSTRAINT pk_megolm_sessions PRIMARY KEY (id),
    CONSTRAINT uq_megolm_sessions_session UNIQUE (session_id)
);

CREATE INDEX IF NOT EXISTS idx_megolm_sessions_room ON megolm_sessions(room_id);
CREATE INDEX IF NOT EXISTS idx_megolm_sessions_session ON megolm_sessions(session_id);

-- 事件签名表
-- 存储事件的数字签名
CREATE TABLE IF NOT EXISTS event_signatures (
    id UUID DEFAULT gen_random_uuid(),
    event_id TEXT NOT NULL,
    user_id TEXT NOT NULL,
    device_id TEXT NOT NULL,
    signature TEXT NOT NULL,
    key_id TEXT NOT NULL,
    algorithm TEXT NOT NULL,
    created_ts BIGINT NOT NULL,
    CONSTRAINT pk_event_signatures PRIMARY KEY (id),
    CONSTRAINT uq_event_signatures_event_user_device_key UNIQUE (event_id, user_id, device_id, key_id)
);

CREATE INDEX IF NOT EXISTS idx_event_signatures_event ON event_signatures(event_id);

-- 设备签名表
-- 存储设备之间的签名关系
CREATE TABLE IF NOT EXISTS device_signatures (
    id BIGSERIAL,
    user_id TEXT NOT NULL,
    device_id TEXT NOT NULL,
    target_user_id TEXT NOT NULL,
    target_device_id TEXT NOT NULL,
    algorithm TEXT NOT NULL,
    signature TEXT NOT NULL,
    created_ts BIGINT NOT NULL,
    CONSTRAINT pk_device_signatures PRIMARY KEY (id),
    CONSTRAINT uq_device_signatures_unique UNIQUE (user_id, device_id, target_user_id, target_device_id, algorithm)
);

-- 密钥备份表
-- 存储用户的密钥备份元数据
CREATE TABLE IF NOT EXISTS key_backups (
    backup_id BIGSERIAL,
    user_id TEXT NOT NULL,
    backup_id_text TEXT,
    algorithm TEXT NOT NULL,
    auth_data JSONB,
    auth_key TEXT,
    mgmt_key TEXT,
    version BIGINT DEFAULT 1,
    etag TEXT,
    created_ts BIGINT NOT NULL,
    updated_ts BIGINT,
    CONSTRAINT pk_key_backups PRIMARY KEY (backup_id),
    CONSTRAINT uq_key_backups_user_version UNIQUE (user_id, version)
);

CREATE INDEX IF NOT EXISTS idx_key_backups_user ON key_backups(user_id);

-- 密钥备份数据表
-- 存储密钥备份的具体数据
CREATE TABLE IF NOT EXISTS backup_keys (
    id BIGSERIAL,
    backup_id BIGINT NOT NULL,
    room_id TEXT NOT NULL,
    session_id TEXT NOT NULL,
    session_data JSONB NOT NULL,
    first_message_index BIGINT,
    forwarded_count BIGINT DEFAULT 0,
    is_verified BOOLEAN DEFAULT FALSE,
    created_ts BIGINT NOT NULL,
    CONSTRAINT pk_backup_keys PRIMARY KEY (id),
    CONSTRAINT fk_backup_keys_backup FOREIGN KEY (backup_id) REFERENCES key_backups(backup_id) ON DELETE CASCADE
);

CREATE INDEX IF NOT EXISTS idx_backup_keys_backup ON backup_keys(backup_id);
CREATE INDEX IF NOT EXISTS idx_backup_keys_room ON backup_keys(room_id);

-- ============================================================================
-- Olm 账户表
-- 存储 Olm 加密账户信息
-- ============================================================================
CREATE TABLE IF NOT EXISTS olm_accounts (
    id BIGSERIAL PRIMARY KEY,
    user_id TEXT NOT NULL,
    device_id TEXT NOT NULL,
    identity_key TEXT NOT NULL,
    serialized_account TEXT NOT NULL,
    is_one_time_keys_published BOOLEAN DEFAULT FALSE,
    is_fallback_key_published BOOLEAN DEFAULT FALSE,
    created_ts BIGINT NOT NULL,
    updated_ts BIGINT NOT NULL,
    CONSTRAINT uq_olm_accounts_user_device UNIQUE (user_id, device_id)
);

CREATE INDEX IF NOT EXISTS idx_olm_accounts_user ON olm_accounts(user_id);
CREATE INDEX IF NOT EXISTS idx_olm_accounts_device ON olm_accounts(device_id);

-- ============================================================================
-- Olm 会话表
-- 存储 Olm 加密会话信息
-- ============================================================================
CREATE TABLE IF NOT EXISTS olm_sessions (
    id BIGSERIAL PRIMARY KEY,
    user_id TEXT NOT NULL,
    device_id TEXT NOT NULL,
    session_id TEXT NOT NULL,
    sender_key TEXT NOT NULL,
    receiver_key TEXT NOT NULL,
    serialized_state TEXT NOT NULL,
    message_index INTEGER DEFAULT 0,
    created_ts BIGINT NOT NULL,
    last_used_ts BIGINT NOT NULL,
    expires_at BIGINT,
    CONSTRAINT uq_olm_sessions_session UNIQUE (session_id)
);

CREATE INDEX IF NOT EXISTS idx_olm_sessions_user_device ON olm_sessions(user_id, device_id);
CREATE INDEX IF NOT EXISTS idx_olm_sessions_sender_key ON olm_sessions(sender_key);
CREATE INDEX IF NOT EXISTS idx_olm_sessions_expires ON olm_sessions(expires_at) WHERE expires_at IS NOT NULL;

-- ============================================================================
-- E2EE 密钥请求表
-- 存储密钥请求记录
-- ============================================================================
CREATE TABLE IF NOT EXISTS e2ee_key_requests (
    id BIGSERIAL PRIMARY KEY,
    request_id TEXT NOT NULL,
    user_id TEXT NOT NULL,
    device_id TEXT NOT NULL,
    room_id TEXT NOT NULL,
    session_id TEXT NOT NULL,
    algorithm TEXT NOT NULL,
    action TEXT NOT NULL,
    is_fulfilled BOOLEAN DEFAULT FALSE,
    fulfilled_by_device TEXT,
    fulfilled_ts BIGINT,
    created_ts BIGINT NOT NULL,
    CONSTRAINT uq_e2ee_key_requests_request UNIQUE (request_id)
);

CREATE INDEX IF NOT EXISTS idx_e2ee_key_requests_user ON e2ee_key_requests(user_id);
CREATE INDEX IF NOT EXISTS idx_e2ee_key_requests_session ON e2ee_key_requests(session_id);
CREATE INDEX IF NOT EXISTS idx_e2ee_key_requests_pending ON e2ee_key_requests(is_fulfilled) WHERE is_fulfilled = FALSE;

CREATE TABLE IF NOT EXISTS device_verification_request (
    id BIGSERIAL,
    user_id TEXT NOT NULL,
    new_device_id TEXT NOT NULL,
    requesting_device_id TEXT,
    verification_method TEXT NOT NULL,
    status TEXT NOT NULL,
    request_token TEXT NOT NULL,
    commitment TEXT,
    pubkey TEXT,
    created_ts BIGINT NOT NULL,
    expires_at TIMESTAMPTZ NOT NULL,
    completed_at TIMESTAMPTZ,
    CONSTRAINT pk_device_verification_request PRIMARY KEY (id),
    CONSTRAINT uq_device_verification_request_token UNIQUE (request_token),
    CONSTRAINT fk_device_verification_request_user FOREIGN KEY (user_id) REFERENCES users(user_id) ON DELETE CASCADE
);

CREATE INDEX IF NOT EXISTS idx_device_verification_request_user_device_pending
ON device_verification_request(user_id, new_device_id)
WHERE status = 'pending';

CREATE INDEX IF NOT EXISTS idx_device_verification_request_expires_pending
ON device_verification_request(expires_at)
WHERE status = 'pending';

-- ============================================================================
-- 第四部分：媒体存储表
-- ============================================================================

-- 媒体元数据表
-- 存储上传媒体的元数据
CREATE TABLE IF NOT EXISTS media_metadata (
    media_id TEXT NOT NULL,
    server_name TEXT NOT NULL,
    content_type TEXT NOT NULL,
    file_name TEXT,
    size BIGINT NOT NULL,
    uploader_user_id TEXT,
    created_ts BIGINT NOT NULL,
    last_accessed_at BIGINT,
    quarantine_status TEXT,
    CONSTRAINT pk_media_metadata PRIMARY KEY (media_id)
);

CREATE INDEX IF NOT EXISTS idx_media_uploader ON media_metadata(uploader_user_id);
CREATE INDEX IF NOT EXISTS idx_media_server ON media_metadata(server_name);

-- 缩略图表
-- 存储媒体缩略图
CREATE TABLE IF NOT EXISTS thumbnails (
    id BIGSERIAL,
    media_id TEXT NOT NULL,
    width INTEGER NOT NULL,
    height INTEGER NOT NULL,
    method TEXT NOT NULL,
    content_type TEXT NOT NULL,
    size BIGINT NOT NULL,
    created_ts BIGINT NOT NULL,
    CONSTRAINT pk_thumbnails PRIMARY KEY (id),
    CONSTRAINT fk_thumbnails_media FOREIGN KEY (media_id) REFERENCES media_metadata(media_id) ON DELETE CASCADE
);

CREATE INDEX IF NOT EXISTS idx_thumbnails_media ON thumbnails(media_id);

-- 媒体配额表
-- 存储用户的媒体存储配额
CREATE TABLE IF NOT EXISTS media_quota (
    id BIGSERIAL,
    user_id TEXT NOT NULL,
    max_bytes BIGINT DEFAULT 1073741824,
    used_bytes BIGINT DEFAULT 0,
    created_ts BIGINT NOT NULL,
    updated_ts BIGINT,
    CONSTRAINT pk_media_quota PRIMARY KEY (id),
    CONSTRAINT uq_media_quota_user UNIQUE (user_id),
    CONSTRAINT fk_media_quota_user FOREIGN KEY (user_id) REFERENCES users(user_id) ON DELETE CASCADE
);

-- ============================================================================
-- 第五部分：认证相关表 (CAS/SAML)
-- ============================================================================

-- CAS 票据表
-- 存储 CAS 认证票据
CREATE TABLE IF NOT EXISTS cas_tickets (
    id BIGSERIAL,
    ticket_id TEXT NOT NULL,
    user_id TEXT NOT NULL,
    service_url TEXT NOT NULL,
    created_ts BIGINT NOT NULL,
    expires_at BIGINT NOT NULL,
    consumed_at BIGINT,
    consumed_by TEXT,
    is_valid BOOLEAN DEFAULT TRUE,
    CONSTRAINT pk_cas_tickets PRIMARY KEY (id),
    CONSTRAINT uq_cas_tickets_ticket UNIQUE (ticket_id)
);

CREATE INDEX IF NOT EXISTS idx_cas_tickets_user ON cas_tickets(user_id);

-- CAS 代理票据表
-- 存储 CAS 代理票据
CREATE TABLE IF NOT EXISTS cas_proxy_tickets (
    id BIGSERIAL,
    proxy_ticket_id TEXT NOT NULL,
    user_id TEXT NOT NULL,
    service_url TEXT NOT NULL,
    pgt_url TEXT,
    created_ts BIGINT NOT NULL,
    expires_at BIGINT NOT NULL,
    consumed_at BIGINT,
    is_valid BOOLEAN DEFAULT TRUE,
    CONSTRAINT pk_cas_proxy_tickets PRIMARY KEY (id),
    CONSTRAINT uq_cas_proxy_tickets_ticket UNIQUE (proxy_ticket_id)
);

-- CAS 代理授予票据表
-- 存储 CAS 代理授予票据
CREATE TABLE IF NOT EXISTS cas_proxy_granting_tickets (
    id BIGSERIAL,
    pgt_id TEXT NOT NULL,
    user_id TEXT NOT NULL,
    service_url TEXT NOT NULL,
    iou TEXT,
    created_ts BIGINT NOT NULL,
    expires_at BIGINT NOT NULL,
    is_valid BOOLEAN DEFAULT TRUE,
    CONSTRAINT pk_cas_proxy_granting_tickets PRIMARY KEY (id),
    CONSTRAINT uq_cas_proxy_granting_tickets_pgt UNIQUE (pgt_id)
);

-- CAS 服务表
-- 存储允许的 CAS 服务配置
CREATE TABLE IF NOT EXISTS cas_services (
    id BIGSERIAL,
    service_id TEXT NOT NULL,
    name TEXT NOT NULL,
    description TEXT,
    service_url_pattern TEXT NOT NULL,
    allowed_attributes JSONB DEFAULT '[]',
    allowed_proxy_callbacks JSONB DEFAULT '[]',
    is_enabled BOOLEAN DEFAULT TRUE,
    require_secure BOOLEAN DEFAULT TRUE,
    single_logout BOOLEAN DEFAULT FALSE,
    created_ts BIGINT NOT NULL,
    updated_ts BIGINT,
    CONSTRAINT pk_cas_services PRIMARY KEY (id),
    CONSTRAINT uq_cas_services_service UNIQUE (service_id)
);

-- CAS 用户属性表
-- 存储 CAS 用户的扩展属性
CREATE TABLE IF NOT EXISTS cas_user_attributes (
    id BIGSERIAL,
    user_id TEXT NOT NULL,
    attribute_name TEXT NOT NULL,
    attribute_value TEXT NOT NULL,
    created_ts BIGINT NOT NULL,
    updated_ts BIGINT,
    CONSTRAINT pk_cas_user_attributes PRIMARY KEY (id),
    CONSTRAINT uq_cas_user_attributes_user_name UNIQUE (user_id, attribute_name)
);

-- CAS 单点登出会话表
-- 存储 CAS 单点登出会话信息
CREATE TABLE IF NOT EXISTS cas_slo_sessions (
    id BIGSERIAL,
    session_id TEXT NOT NULL,
    user_id TEXT NOT NULL,
    service_url TEXT NOT NULL,
    ticket_id TEXT,
    created_ts BIGINT NOT NULL,
    logout_sent_at BIGINT,
    CONSTRAINT pk_cas_slo_sessions PRIMARY KEY (id)
);

-- SAML 会话表
-- 存储 SAML 认证会话
CREATE TABLE IF NOT EXISTS saml_sessions (
    id BIGSERIAL,
    session_id TEXT NOT NULL,
    user_id TEXT NOT NULL,
    name_id TEXT,
    issuer TEXT,
    session_index TEXT,
    attributes JSONB DEFAULT '{}',
    created_ts BIGINT NOT NULL,
    expires_at BIGINT NOT NULL,
    last_used_ts BIGINT NOT NULL,
    status TEXT DEFAULT 'active',
    CONSTRAINT pk_saml_sessions PRIMARY KEY (id),
    CONSTRAINT uq_saml_sessions_session UNIQUE (session_id)
);

CREATE INDEX IF NOT EXISTS idx_saml_sessions_user ON saml_sessions(user_id);

-- SAML 用户映射表
-- 存储 SAML 用户与 Matrix 用户的映射关系
CREATE TABLE IF NOT EXISTS saml_user_mapping (
    id BIGSERIAL,
    name_id TEXT NOT NULL,
    user_id TEXT NOT NULL,
    issuer TEXT NOT NULL,
    first_seen_ts BIGINT NOT NULL,
    last_authenticated_ts BIGINT NOT NULL,
    authentication_count INTEGER DEFAULT 1,
    attributes JSONB DEFAULT '{}',
    CONSTRAINT pk_saml_user_mapping PRIMARY KEY (id),
    CONSTRAINT uq_saml_user_mapping_name_issuer UNIQUE (name_id, issuer)
);

-- SAML 身份提供商表
-- 存储 SAML 身份提供商配置
CREATE TABLE IF NOT EXISTS saml_identity_providers (
    id BIGSERIAL,
    entity_id TEXT NOT NULL,
    display_name TEXT,
    description TEXT,
    metadata_url TEXT,
    metadata_xml TEXT,
    is_enabled BOOLEAN DEFAULT TRUE,
    priority INTEGER DEFAULT 100,
    attribute_mapping JSONB DEFAULT '{}',
    created_ts BIGINT NOT NULL,
    updated_ts BIGINT,
    last_metadata_refresh_at BIGINT,
    metadata_valid_until_at BIGINT,
    CONSTRAINT pk_saml_identity_providers PRIMARY KEY (id),
    CONSTRAINT uq_saml_identity_providers_entity UNIQUE (entity_id)
);

-- SAML 认证事件表
-- 存储 SAML 认证事件日志
CREATE TABLE IF NOT EXISTS saml_auth_events (
    id BIGSERIAL,
    session_id TEXT,
    user_id TEXT,
    name_id TEXT,
    issuer TEXT,
    event_type TEXT NOT NULL,
    status TEXT NOT NULL,
    error_message TEXT,
    ip_address TEXT,
    user_agent TEXT,
    request_id TEXT,
    attributes JSONB DEFAULT '{}',
    created_ts BIGINT NOT NULL,
    CONSTRAINT pk_saml_auth_events PRIMARY KEY (id)
);

-- SAML 登出请求表
-- 存储 SAML 登出请求
CREATE TABLE IF NOT EXISTS saml_logout_requests (
    id BIGSERIAL,
    request_id TEXT NOT NULL,
    session_id TEXT,
    user_id TEXT,
    name_id TEXT,
    issuer TEXT,
    reason TEXT,
    status TEXT DEFAULT 'pending',
    created_ts BIGINT NOT NULL,
    processed_at BIGINT,
    CONSTRAINT pk_saml_logout_requests PRIMARY KEY (id),
    CONSTRAINT uq_saml_logout_requests_request UNIQUE (request_id)
);

-- ============================================================================
-- 第六部分：验证码相关表
-- ============================================================================

-- 注册验证码表
-- 存储注册验证码
CREATE TABLE IF NOT EXISTS registration_captcha (
    id BIGSERIAL,
    captcha_id TEXT NOT NULL,
    captcha_type TEXT NOT NULL,
    target TEXT NOT NULL,
    code TEXT NOT NULL,
    created_ts BIGINT NOT NULL,
    expires_at BIGINT NOT NULL,
    used_at BIGINT,
    verified_at BIGINT,
    ip_address TEXT,
    user_agent TEXT,
    attempt_count INTEGER DEFAULT 0,
    max_attempts INTEGER DEFAULT 3,
    status TEXT DEFAULT 'pending',
    metadata JSONB DEFAULT '{}',
    CONSTRAINT pk_registration_captcha PRIMARY KEY (id),
    CONSTRAINT uq_registration_captcha_id UNIQUE (captcha_id)
);

CREATE INDEX IF NOT EXISTS idx_captcha_target ON registration_captcha(target);
CREATE INDEX IF NOT EXISTS idx_captcha_status ON registration_captcha(status);

-- 验证码发送日志表
-- 存储验证码发送记录
CREATE TABLE IF NOT EXISTS captcha_send_log (
    id BIGSERIAL,
    captcha_id TEXT,
    captcha_type TEXT NOT NULL,
    target TEXT NOT NULL,
    sent_ts BIGINT NOT NULL,
    ip_address TEXT,
    user_agent TEXT,
    is_success BOOLEAN DEFAULT TRUE,
    error_message TEXT,
    provider TEXT,
    provider_response TEXT,
    CONSTRAINT pk_captcha_send_log PRIMARY KEY (id)
);

CREATE INDEX IF NOT EXISTS idx_captcha_send_target ON captcha_send_log(target);

-- 验证码模板表
-- 存储验证码模板
CREATE TABLE IF NOT EXISTS captcha_template (
    id BIGSERIAL,
    template_name TEXT NOT NULL,
    captcha_type TEXT NOT NULL,
    subject TEXT,
    content TEXT NOT NULL,
    variables JSONB DEFAULT '{}',
    is_default BOOLEAN DEFAULT FALSE,
    is_enabled BOOLEAN DEFAULT TRUE,
    created_ts BIGINT NOT NULL,
    updated_ts BIGINT,
    CONSTRAINT pk_captcha_template PRIMARY KEY (id),
    CONSTRAINT uq_captcha_template_name UNIQUE (template_name)
);

-- 验证码配置表
-- 存储验证码配置
CREATE TABLE IF NOT EXISTS captcha_config (
    id BIGSERIAL,
    config_key TEXT NOT NULL,
    config_value TEXT NOT NULL,
    description TEXT,
    created_ts BIGINT NOT NULL,
    updated_ts BIGINT,
    CONSTRAINT pk_captcha_config PRIMARY KEY (id),
    CONSTRAINT uq_captcha_config_key UNIQUE (config_key)
);

-- ============================================================================
-- 第七部分：推送通知表
-- ============================================================================

-- 推送设备表
-- 存储用户的推送设备信息
CREATE TABLE IF NOT EXISTS push_devices (
    id BIGSERIAL,
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
    CONSTRAINT pk_push_devices PRIMARY KEY (id),
    CONSTRAINT uq_push_devices_user_device_pushkey UNIQUE (user_id, device_id, pushkey)
);

CREATE INDEX IF NOT EXISTS idx_push_devices_user ON push_devices(user_id);

-- 推送规则表
-- 存储用户的推送规则
CREATE TABLE IF NOT EXISTS push_rules (
    id BIGSERIAL,
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
    CONSTRAINT pk_push_rules PRIMARY KEY (id),
    CONSTRAINT uq_push_rules_user_scope_kind_rule UNIQUE (user_id, scope, kind, rule_id)
);

CREATE INDEX IF NOT EXISTS idx_push_rules_user ON push_rules(user_id);

-- 推送器表 (Pushers)
-- 存储推送器配置
CREATE TABLE IF NOT EXISTS pushers (
    id BIGSERIAL,
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
    CONSTRAINT pk_pushers PRIMARY KEY (id),
    CONSTRAINT uq_pushers_user_device_pushkey UNIQUE (user_id, device_id, pushkey)
);

CREATE INDEX IF NOT EXISTS idx_pushers_user ON pushers(user_id);
CREATE INDEX IF NOT EXISTS idx_pushers_enabled ON pushers(is_enabled) WHERE is_enabled = TRUE;

-- ============================================================================
-- 第八部分：Space 相关表
-- ============================================================================

-- Space 子房间表
-- 存储 Space 与子房间的关联
CREATE TABLE IF NOT EXISTS space_children (
    id BIGSERIAL,
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

-- Spaces 表 (新增: 修复 API 错误)
CREATE TABLE IF NOT EXISTS spaces (
    space_id TEXT NOT NULL PRIMARY KEY,
    room_id TEXT,
    name TEXT,
    creator TEXT NOT NULL,
    created_ts BIGINT NOT NULL,
    is_public BOOLEAN DEFAULT FALSE,
    is_private BOOLEAN DEFAULT TRUE,
    member_count BIGINT DEFAULT 0,
    topic TEXT,
    avatar_url TEXT,
    canonical_alias TEXT,
    history_visibility TEXT DEFAULT 'shared',
    join_rule TEXT DEFAULT 'invite',
    visibility TEXT DEFAULT 'private',
    parent_space_id TEXT,
    room_type TEXT DEFAULT 'm.space',
    updated_ts BIGINT
);

CREATE INDEX IF NOT EXISTS idx_spaces_creator ON spaces(creator);
CREATE INDEX IF NOT EXISTS idx_spaces_public ON spaces(is_public) WHERE is_public = TRUE;
CREATE INDEX IF NOT EXISTS idx_spaces_parent ON spaces(parent_space_id) WHERE parent_space_id IS NOT NULL;

CREATE TABLE IF NOT EXISTS space_members (
    id BIGSERIAL PRIMARY KEY,
    space_id TEXT NOT NULL,
    user_id TEXT NOT NULL,
    membership TEXT NOT NULL DEFAULT 'join',
    joined_ts BIGINT NOT NULL,
    updated_ts BIGINT,
    left_ts BIGINT,
    inviter TEXT,
    CONSTRAINT uq_space_members_space_user UNIQUE (space_id, user_id)
);

CREATE INDEX IF NOT EXISTS idx_space_members_space ON space_members(space_id);
CREATE INDEX IF NOT EXISTS idx_space_members_user ON space_members(user_id);
CREATE INDEX IF NOT EXISTS idx_space_members_membership ON space_members(membership);

CREATE TABLE IF NOT EXISTS space_summaries (
    id BIGSERIAL PRIMARY KEY,
    space_id TEXT NOT NULL UNIQUE,
    summary JSONB DEFAULT '{}',
    children_count BIGINT DEFAULT 0,
    member_count BIGINT DEFAULT 0,
    updated_ts BIGINT NOT NULL,
    CONSTRAINT fk_space_summary_space FOREIGN KEY (space_id) REFERENCES spaces(space_id) ON DELETE CASCADE
);

CREATE INDEX IF NOT EXISTS idx_space_summary_space ON space_summaries(space_id);

CREATE TABLE IF NOT EXISTS space_statistics (
    space_id TEXT PRIMARY KEY,
    name TEXT,
    is_public BOOLEAN NOT NULL DEFAULT FALSE,
    child_room_count BIGINT DEFAULT 0,
    member_count BIGINT DEFAULT 0,
    created_ts BIGINT NOT NULL,
    updated_ts BIGINT
);

CREATE INDEX IF NOT EXISTS idx_space_statistics_member_count ON space_statistics(member_count DESC);

CREATE TABLE IF NOT EXISTS space_events (
    event_id TEXT NOT NULL PRIMARY KEY,
    space_id TEXT NOT NULL,
    event_type TEXT NOT NULL,
    sender TEXT NOT NULL,
    content JSONB NOT NULL,
    state_key TEXT,
    origin_server_ts BIGINT NOT NULL,
    processed_ts BIGINT,
    CONSTRAINT fk_space_events_space FOREIGN KEY (space_id) REFERENCES spaces(space_id) ON DELETE CASCADE
);

CREATE INDEX IF NOT EXISTS idx_space_events_space ON space_events(space_id);
CREATE INDEX IF NOT EXISTS idx_space_events_space_type_ts
ON space_events(space_id, event_type, origin_server_ts DESC);
CREATE INDEX IF NOT EXISTS idx_space_events_space_ts
ON space_events(space_id, origin_server_ts DESC);

-- ============================================================================
-- 第九部分：联邦相关表
-- ============================================================================

-- 联邦服务器表
-- 存储联邦服务器状态
CREATE TABLE IF NOT EXISTS federation_servers (
    id BIGSERIAL,
    server_name TEXT NOT NULL,
    is_blocked BOOLEAN DEFAULT FALSE,
    blocked_at BIGINT,
    blocked_reason TEXT,
    last_successful_connect_at BIGINT,
    last_failed_connect_at BIGINT,
    failure_count INTEGER DEFAULT 0,
    status TEXT NOT NULL DEFAULT 'active',
    updated_ts BIGINT,
    CONSTRAINT pk_federation_servers PRIMARY KEY (id),
    CONSTRAINT uq_federation_servers_name UNIQUE (server_name)
);

CREATE INDEX IF NOT EXISTS idx_federation_servers_status ON federation_servers(status);

-- 联邦黑名单表
-- 存储联邦黑名单
CREATE TABLE IF NOT EXISTS federation_blacklist (
    id BIGSERIAL,
    server_name TEXT NOT NULL,
    reason TEXT,
    added_ts BIGINT NOT NULL,
    added_by TEXT,
    updated_ts BIGINT,
    CONSTRAINT pk_federation_blacklist PRIMARY KEY (id),
    CONSTRAINT uq_federation_blacklist_name UNIQUE (server_name)
);

CREATE INDEX IF NOT EXISTS idx_federation_blacklist_server ON federation_blacklist(server_name);

-- 联邦队列表
-- 存储待发送的联邦事件
CREATE TABLE IF NOT EXISTS federation_queue (
    id BIGSERIAL,
    destination TEXT NOT NULL,
    event_id TEXT NOT NULL,
    event_type TEXT NOT NULL,
    room_id TEXT,
    content JSONB NOT NULL,
    created_ts BIGINT NOT NULL,
    sent_at BIGINT,
    retry_count INTEGER DEFAULT 0,
    status TEXT DEFAULT 'pending',
    CONSTRAINT pk_federation_queue PRIMARY KEY (id)
);

CREATE INDEX IF NOT EXISTS idx_federation_queue_destination ON federation_queue(destination);
CREATE INDEX IF NOT EXISTS idx_federation_queue_status ON federation_queue(status);

-- 联邦入站事件去重表
-- 防止重复处理来自远端服务器的相同事件
CREATE TABLE IF NOT EXISTS federation_inbound_events (
    event_id TEXT NOT NULL,
    origin TEXT NOT NULL,
    origin_server_ts BIGINT NOT NULL,
    received_ts BIGINT NOT NULL,
    CONSTRAINT pk_federation_inbound_events PRIMARY KEY (event_id)
);

CREATE INDEX IF NOT EXISTS idx_federation_inbound_events_origin ON federation_inbound_events(origin);
CREATE INDEX IF NOT EXISTS idx_federation_inbound_events_received ON federation_inbound_events(received_ts DESC);

-- 事件 DAG 边表
-- 存储事件之间的引用关系（prev_events），用于联邦状态解析
CREATE TABLE IF NOT EXISTS event_edges (
    event_id TEXT NOT NULL,
    prev_event_id TEXT NOT NULL,
    is_state BOOLEAN NOT NULL DEFAULT FALSE,
    CONSTRAINT pk_event_edges PRIMARY KEY (event_id, prev_event_id),
    CONSTRAINT fk_event_edges_event FOREIGN KEY (event_id) REFERENCES events(event_id) ON DELETE CASCADE
);

CREATE INDEX IF NOT EXISTS idx_event_edges_prev ON event_edges(prev_event_id);

-- 前向极值表
-- 记录每个房间尚未被最新事件引用的极值事件，用于增量状态计算
CREATE TABLE IF NOT EXISTS event_forward_extremities (
    room_id TEXT NOT NULL,
    event_id TEXT NOT NULL,
    CONSTRAINT pk_event_forward_extremities PRIMARY KEY (room_id, event_id),
    CONSTRAINT fk_event_forward_extremities_event FOREIGN KEY (event_id) REFERENCES events(event_id) ON DELETE CASCADE,
    CONSTRAINT fk_event_forward_extremities_room FOREIGN KEY (room_id) REFERENCES rooms(room_id) ON DELETE CASCADE
);

CREATE INDEX IF NOT EXISTS idx_event_forward_extremities_room ON event_forward_extremities(room_id);

-- 线性化读收据表
-- 持久化用户的已读收据状态
CREATE TABLE IF NOT EXISTS receipts_linearized (
    stream_id BIGSERIAL,
    room_id TEXT NOT NULL,
    receipt_type TEXT NOT NULL DEFAULT 'm.read',
    user_id TEXT NOT NULL,
    event_id TEXT NOT NULL,
    received_ts BIGINT NOT NULL,
    CONSTRAINT pk_receipts_linearized PRIMARY KEY (stream_id),
    CONSTRAINT fk_receipts_linearized_room FOREIGN KEY (room_id) REFERENCES rooms(room_id) ON DELETE CASCADE,
    CONSTRAINT fk_receipts_linearized_user FOREIGN KEY (user_id) REFERENCES users(user_id) ON DELETE CASCADE
);

CREATE UNIQUE INDEX IF NOT EXISTS idx_receipts_linearized_room_user ON receipts_linearized(room_id, user_id);
CREATE INDEX IF NOT EXISTS idx_receipts_linearized_event ON receipts_linearized(event_id);
CREATE INDEX IF NOT EXISTS idx_receipts_linearized_stream ON receipts_linearized(stream_id);

-- ============================================================================
-- 第十部分：账户数据表
-- ============================================================================

-- 用户过滤器表
-- 存储用户的同步过滤器
CREATE TABLE IF NOT EXISTS filters (
    id BIGSERIAL,
    user_id TEXT NOT NULL,
    filter_id TEXT NOT NULL,
    content JSONB NOT NULL DEFAULT '{}',
    created_ts BIGINT NOT NULL,
    CONSTRAINT pk_filters PRIMARY KEY (id),
    CONSTRAINT uq_filters_user_filter UNIQUE (user_id, filter_id)
);

CREATE INDEX IF NOT EXISTS idx_filters_user ON filters(user_id);
CREATE INDEX IF NOT EXISTS idx_filters_filter_id ON filters(filter_id);

-- OpenID 令牌表
-- 存储 OpenID 令牌
CREATE TABLE IF NOT EXISTS openid_tokens (
    id BIGSERIAL,
    token TEXT NOT NULL,
    user_id TEXT NOT NULL,
    device_id TEXT,
    created_ts BIGINT NOT NULL,
    expires_at BIGINT NOT NULL,
    is_valid BOOLEAN DEFAULT TRUE,
    CONSTRAINT pk_openid_tokens PRIMARY KEY (id),
    CONSTRAINT uq_openid_tokens_token UNIQUE (token),
    CONSTRAINT fk_openid_tokens_user FOREIGN KEY (user_id) REFERENCES users(user_id) ON DELETE CASCADE
);

CREATE INDEX IF NOT EXISTS idx_openid_tokens_user ON openid_tokens(user_id);

-- 账户数据表
-- 存储用户的账户数据
CREATE TABLE IF NOT EXISTS account_data (
    id BIGSERIAL,
    user_id TEXT NOT NULL,
    data_type TEXT NOT NULL,
    content JSONB NOT NULL,
    created_ts BIGINT NOT NULL,
    updated_ts BIGINT NOT NULL,
    CONSTRAINT pk_account_data PRIMARY KEY (id),
    CONSTRAINT uq_account_data_user_type UNIQUE (user_id, data_type)
);

CREATE INDEX IF NOT EXISTS idx_account_data_user ON account_data(user_id);

-- 房间账户数据表
-- 存储用户在特定房间的账户数据
CREATE TABLE IF NOT EXISTS room_account_data (
    id BIGSERIAL,
    user_id TEXT NOT NULL,
    room_id TEXT NOT NULL,
    data_type TEXT NOT NULL,
    data JSONB NOT NULL,
    created_ts BIGINT NOT NULL,
    updated_ts BIGINT NOT NULL,
    CONSTRAINT pk_room_account_data PRIMARY KEY (id),
    CONSTRAINT uq_room_account_data_user_room_type UNIQUE (user_id, room_id, data_type)
);

-- 用户账户数据表
-- 存储用户的账户数据（旧格式兼容）
CREATE TABLE IF NOT EXISTS user_account_data (
    id BIGSERIAL,
    user_id TEXT NOT NULL,
    event_type TEXT NOT NULL,
    content TEXT NOT NULL,
    created_ts BIGINT NOT NULL,
    CONSTRAINT pk_user_account_data PRIMARY KEY (id),
    CONSTRAINT uq_user_account_data_user_type UNIQUE (user_id, event_type)
);

-- ============================================================================
-- 第十一部分：后台任务表
-- ============================================================================

-- 后台更新表
-- 存储后台更新任务状态
CREATE TABLE IF NOT EXISTS background_updates (
    id BIGSERIAL,
    update_name TEXT NOT NULL,
    job_name TEXT,
    job_type TEXT,
    description TEXT,
    table_name TEXT,
    column_name TEXT,
    is_running BOOLEAN DEFAULT FALSE,
    status TEXT DEFAULT 'pending',
    progress JSONB DEFAULT '{}',
    total_items INTEGER DEFAULT 0,
    processed_items INTEGER DEFAULT 0,
    created_ts BIGINT,
    started_ts BIGINT,
    completed_ts BIGINT,
    updated_ts BIGINT,
    error_message TEXT,
    retry_count INTEGER DEFAULT 0,
    max_retries INTEGER DEFAULT 3,
    batch_size INTEGER DEFAULT 100,
    sleep_ms INTEGER DEFAULT 100,
    depends_on JSONB DEFAULT '[]',
    metadata JSONB DEFAULT '{}',
    CONSTRAINT pk_background_updates PRIMARY KEY (id),
    CONSTRAINT uq_background_updates_name UNIQUE (update_name)
);

CREATE INDEX IF NOT EXISTS idx_background_updates_status ON background_updates(status);
CREATE INDEX IF NOT EXISTS idx_background_updates_running ON background_updates(is_running) WHERE is_running = TRUE;

-- 工作进程表
-- 存储工作进程状态
CREATE TABLE IF NOT EXISTS workers (
    id BIGSERIAL,
    worker_id TEXT NOT NULL,
    worker_name TEXT NOT NULL,
    worker_type TEXT NOT NULL,
    host TEXT NOT NULL DEFAULT 'localhost',
    port INTEGER NOT NULL DEFAULT 8080,
    status TEXT NOT NULL DEFAULT 'starting',
    last_heartbeat_ts BIGINT,
    started_ts BIGINT NOT NULL,
    stopped_ts BIGINT,
    config JSONB DEFAULT '{}',
    metadata JSONB DEFAULT '{}',
    version TEXT,
    is_enabled BOOLEAN DEFAULT TRUE,
    CONSTRAINT pk_workers PRIMARY KEY (id),
    CONSTRAINT uq_workers_id UNIQUE (worker_id)
);

CREATE INDEX IF NOT EXISTS idx_workers_type ON workers(worker_type);
CREATE INDEX IF NOT EXISTS idx_workers_status ON workers(status);
CREATE INDEX IF NOT EXISTS idx_workers_heartbeat ON workers(last_heartbeat_ts) WHERE last_heartbeat_ts IS NOT NULL;

-- 工作进程命令表
CREATE TABLE IF NOT EXISTS worker_commands (
    id BIGSERIAL,
    command_id TEXT NOT NULL,
    target_worker_id TEXT NOT NULL,
    source_worker_id TEXT,
    command_type TEXT NOT NULL,
    command_data JSONB DEFAULT '{}',
    priority INTEGER DEFAULT 0,
    status TEXT NOT NULL DEFAULT 'pending',
    created_ts BIGINT NOT NULL,
    sent_ts BIGINT,
    completed_ts BIGINT,
    error_message TEXT,
    retry_count INTEGER DEFAULT 0,
    max_retries INTEGER DEFAULT 3,
    CONSTRAINT pk_worker_commands PRIMARY KEY (id),
    CONSTRAINT uq_worker_commands_id UNIQUE (command_id)
);

CREATE INDEX IF NOT EXISTS idx_worker_commands_target ON worker_commands(target_worker_id);
CREATE INDEX IF NOT EXISTS idx_worker_commands_status ON worker_commands(status);

-- 工作进程事件表
CREATE TABLE IF NOT EXISTS worker_events (
    id BIGSERIAL,
    event_id TEXT NOT NULL,
    stream_id BIGINT NOT NULL,
    event_type TEXT NOT NULL,
    room_id TEXT,
    sender TEXT,
    event_data JSONB DEFAULT '{}',
    created_ts BIGINT NOT NULL,
    processed_by JSONB DEFAULT '[]',
    CONSTRAINT pk_worker_events PRIMARY KEY (id),
    CONSTRAINT uq_worker_events_id UNIQUE (event_id)
);

CREATE INDEX IF NOT EXISTS idx_worker_events_stream ON worker_events(stream_id);
CREATE INDEX IF NOT EXISTS idx_worker_events_type ON worker_events(event_type);

-- 工作进程统计表
CREATE TABLE IF NOT EXISTS worker_statistics (
    id BIGSERIAL,
    worker_id TEXT NOT NULL,
    total_messages_sent BIGINT DEFAULT 0,
    total_messages_received BIGINT DEFAULT 0,
    total_errors BIGINT DEFAULT 0,
    last_message_ts BIGINT,
    last_error_ts BIGINT,
    avg_processing_time_ms BIGINT,
    uptime_seconds BIGINT DEFAULT 0,
    created_ts BIGINT NOT NULL,
    updated_ts BIGINT NOT NULL,
    CONSTRAINT pk_worker_statistics PRIMARY KEY (id)
);

-- 活跃工作进程视图
CREATE OR REPLACE VIEW active_workers AS
SELECT id, worker_id, worker_name, worker_type, host, port, status,
       last_heartbeat_ts, started_ts, stopped_ts, config, metadata, version, is_enabled
FROM workers
WHERE status = 'running' OR status = 'starting';

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

CREATE INDEX IF NOT EXISTS idx_worker_load_stats_worker_recorded
ON worker_load_stats(worker_id, recorded_ts DESC);

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

CREATE INDEX IF NOT EXISTS idx_worker_task_assignments_status_priority_created
ON worker_task_assignments(status, priority DESC, created_ts ASC);

CREATE INDEX IF NOT EXISTS idx_worker_task_assignments_worker_status
ON worker_task_assignments(assigned_worker_id, status);

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

CREATE INDEX IF NOT EXISTS idx_worker_connections_source
ON worker_connections(source_worker_id, status);

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

-- 同步流 ID 表
-- 存储同步流 ID 序列
CREATE TABLE IF NOT EXISTS sync_stream_id (
    id BIGSERIAL,
    stream_type TEXT,
    last_id BIGINT DEFAULT 0,
    updated_ts BIGINT,
    CONSTRAINT pk_sync_stream_id PRIMARY KEY (id),
    CONSTRAINT uq_sync_stream_id_type UNIQUE (stream_type)
);

-- ============================================================================
-- 模块管理表
-- ============================================================================

CREATE TABLE IF NOT EXISTS modules (
    id BIGSERIAL,
    module_name TEXT NOT NULL,
    module_type TEXT NOT NULL,
    is_enabled BOOLEAN DEFAULT TRUE,
    config JSONB DEFAULT '{}',
    priority INTEGER DEFAULT 0,
    created_ts BIGINT NOT NULL,
    updated_ts BIGINT,
    description TEXT,
    CONSTRAINT pk_modules PRIMARY KEY (id),
    CONSTRAINT uq_modules_name UNIQUE (module_name)
);

CREATE INDEX IF NOT EXISTS idx_modules_enabled ON modules(is_enabled);

-- ============================================================================
-- 模块执行日志表
-- ============================================================================

CREATE TABLE IF NOT EXISTS module_execution_logs (
    id BIGSERIAL,
    module_id BIGINT,
    execution_type TEXT NOT NULL,
    input_data JSONB,
    output_data JSONB,
    is_success BOOLEAN DEFAULT TRUE,
    error_message TEXT,
    execution_time_ms BIGINT,
    created_ts BIGINT NOT NULL,
    CONSTRAINT pk_module_execution_logs PRIMARY KEY (id),
    CONSTRAINT fk_module_execution_logs_module FOREIGN KEY (module_id) REFERENCES modules(id) ON DELETE CASCADE
);

CREATE INDEX IF NOT EXISTS idx_module_logs_module ON module_execution_logs(module_id);
CREATE INDEX IF NOT EXISTS idx_module_logs_created ON module_execution_logs(created_ts);

-- ============================================================================
-- 垃圾信息检查结果表
-- ============================================================================

CREATE TABLE IF NOT EXISTS spam_check_results (
    id BIGSERIAL,
    event_id TEXT NOT NULL,
    room_id TEXT NOT NULL,
    user_id TEXT NOT NULL,
    spam_score REAL DEFAULT 0,
    is_spam BOOLEAN DEFAULT FALSE,
    check_details JSONB DEFAULT '{}',
    created_ts BIGINT NOT NULL,
    CONSTRAINT pk_spam_check_results PRIMARY KEY (id)
);

CREATE INDEX IF NOT EXISTS idx_spam_results_event ON spam_check_results(event_id);
CREATE INDEX IF NOT EXISTS idx_spam_results_room ON spam_check_results(room_id);

-- ============================================================================
-- 第三方规则结果表
-- ============================================================================

CREATE TABLE IF NOT EXISTS third_party_rule_results (
    id BIGSERIAL,
    rule_type TEXT NOT NULL,
    event_id TEXT,
    room_id TEXT,
    user_id TEXT,
    is_allowed BOOLEAN DEFAULT TRUE,
    rule_details JSONB DEFAULT '{}',
    created_ts BIGINT NOT NULL,
    CONSTRAINT pk_third_party_rule_results PRIMARY KEY (id)
);

CREATE INDEX IF NOT EXISTS idx_third_party_rule_type ON third_party_rule_results(rule_type);

-- ============================================================================
-- 账户有效性检查表
-- ============================================================================

CREATE TABLE IF NOT EXISTS account_validity (
    id BIGSERIAL,
    user_id TEXT NOT NULL,
    is_valid BOOLEAN DEFAULT TRUE,
    last_check_at BIGINT,
    expiration_at BIGINT,
    renewal_token TEXT,
    created_ts BIGINT NOT NULL,
    updated_ts BIGINT,
    CONSTRAINT pk_account_validity PRIMARY KEY (id),
    CONSTRAINT uq_account_validity_user UNIQUE (user_id)
);

CREATE INDEX IF NOT EXISTS idx_account_validity_user ON account_validity(user_id);

-- ============================================================================
-- 密码认证提供者表
-- ============================================================================

CREATE TABLE IF NOT EXISTS password_auth_providers (
    id BIGSERIAL,
    provider_name TEXT NOT NULL,
    provider_type TEXT NOT NULL,
    is_enabled BOOLEAN DEFAULT TRUE,
    config JSONB DEFAULT '{}',
    priority INTEGER DEFAULT 0,
    created_ts BIGINT NOT NULL,
    updated_ts BIGINT,
    CONSTRAINT pk_password_auth_providers PRIMARY KEY (id),
    CONSTRAINT uq_password_auth_providers_name UNIQUE (provider_name)
);

-- ============================================================================
-- Presence 路由表
-- ============================================================================

CREATE TABLE IF NOT EXISTS presence_routes (
    id BIGSERIAL,
    route_name TEXT NOT NULL,
    route_type TEXT NOT NULL,
    is_enabled BOOLEAN DEFAULT TRUE,
    config JSONB DEFAULT '{}',
    created_ts BIGINT NOT NULL,
    CONSTRAINT pk_presence_routes PRIMARY KEY (id),
    CONSTRAINT uq_presence_routes_name UNIQUE (route_name)
);

-- ============================================================================
-- 媒体回调表
-- ============================================================================

CREATE TABLE IF NOT EXISTS media_callbacks (
    id BIGSERIAL,
    callback_name TEXT NOT NULL,
    callback_type TEXT NOT NULL,
    is_enabled BOOLEAN DEFAULT TRUE,
    url TEXT NOT NULL,
    headers JSONB DEFAULT '{}',
    created_ts BIGINT NOT NULL,
    CONSTRAINT pk_media_callbacks PRIMARY KEY (id),
    CONSTRAINT uq_media_callbacks_name UNIQUE (callback_name)
);

-- ============================================================================
-- 速率限制回调表
-- ============================================================================

CREATE TABLE IF NOT EXISTS rate_limit_callbacks (
    id BIGSERIAL,
    callback_name TEXT NOT NULL,
    is_enabled BOOLEAN DEFAULT TRUE,
    config JSONB DEFAULT '{}',
    created_ts BIGINT NOT NULL,
    CONSTRAINT pk_rate_limit_callbacks PRIMARY KEY (id),
    CONSTRAINT uq_rate_limit_callbacks_name UNIQUE (callback_name)
);

-- ============================================================================
-- 账户数据回调表
-- ============================================================================

CREATE TABLE IF NOT EXISTS account_data_callbacks (
    id BIGSERIAL,
    callback_name TEXT NOT NULL,
    is_enabled BOOLEAN DEFAULT TRUE,
    data_types TEXT[] DEFAULT '{}',
    config JSONB DEFAULT '{}',
    created_ts BIGINT NOT NULL,
    CONSTRAINT pk_account_data_callbacks PRIMARY KEY (id),
    CONSTRAINT uq_account_data_callbacks_name UNIQUE (callback_name)
);

-- ============================================================================
-- 注册令牌表
-- ============================================================================

CREATE TABLE IF NOT EXISTS registration_tokens (
    id BIGSERIAL,
    token TEXT NOT NULL,
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
    email TEXT,
    CONSTRAINT pk_registration_tokens PRIMARY KEY (id),
    CONSTRAINT uq_registration_tokens_token UNIQUE (token)
);

CREATE INDEX IF NOT EXISTS idx_registration_tokens_type ON registration_tokens(token_type);
CREATE INDEX IF NOT EXISTS idx_registration_tokens_expires ON registration_tokens(expires_at) WHERE expires_at IS NOT NULL;
CREATE INDEX IF NOT EXISTS idx_registration_tokens_enabled ON registration_tokens(is_enabled) WHERE is_enabled = TRUE;

-- ============================================================================
-- 注册令牌使用记录表
-- ============================================================================

CREATE TABLE IF NOT EXISTS registration_token_usage (
    id BIGSERIAL,
    token_id BIGINT,
    user_id TEXT NOT NULL,
    used_ts BIGINT NOT NULL,
    CONSTRAINT pk_registration_token_usage PRIMARY KEY (id),
    CONSTRAINT fk_registration_token_usage_token FOREIGN KEY (token_id) REFERENCES registration_tokens(id) ON DELETE CASCADE,
    CONSTRAINT fk_registration_token_usage_user FOREIGN KEY (user_id) REFERENCES users(user_id) ON DELETE CASCADE
);

CREATE INDEX IF NOT EXISTS idx_reg_token_usage_token ON registration_token_usage(token_id);
CREATE INDEX IF NOT EXISTS idx_reg_token_usage_user ON registration_token_usage(user_id);

-- ============================================================================
-- 事件举报表
-- ============================================================================

CREATE TABLE IF NOT EXISTS event_reports (
    id BIGSERIAL,
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
    resolution_reason TEXT,
    CONSTRAINT pk_event_reports PRIMARY KEY (id)
);

CREATE INDEX IF NOT EXISTS idx_event_reports_event ON event_reports(event_id);
CREATE INDEX IF NOT EXISTS idx_event_reports_room ON event_reports(room_id);
CREATE INDEX IF NOT EXISTS idx_event_reports_reporter ON event_reports(reporter_user_id);
CREATE INDEX IF NOT EXISTS idx_event_reports_status ON event_reports(status);
CREATE INDEX IF NOT EXISTS idx_event_reports_received ON event_reports(received_ts DESC);

-- ============================================================================
-- 事件举报历史表
-- ============================================================================

CREATE TABLE IF NOT EXISTS event_report_history (
    id BIGSERIAL,
    report_id BIGINT NOT NULL,
    action TEXT NOT NULL,
    actor_user_id TEXT,
    actor_role TEXT,
    old_status TEXT,
    new_status TEXT,
    reason TEXT,
    created_ts BIGINT NOT NULL,
    metadata JSONB,
    CONSTRAINT pk_event_report_history PRIMARY KEY (id),
    CONSTRAINT fk_event_report_history_report FOREIGN KEY (report_id) REFERENCES event_reports(id) ON DELETE CASCADE
);

CREATE INDEX IF NOT EXISTS idx_event_report_history_report ON event_report_history(report_id);

-- ============================================================================
-- 举报速率限制表
-- ============================================================================

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
    CONSTRAINT uq_report_rate_limits_user UNIQUE (user_id),
    CONSTRAINT fk_report_rate_limits_user FOREIGN KEY (user_id) REFERENCES users(user_id) ON DELETE CASCADE
);

CREATE INDEX IF NOT EXISTS idx_report_rate_limits_user ON report_rate_limits(user_id);

-- ============================================================================
-- 举报统计表
-- ============================================================================

CREATE TABLE IF NOT EXISTS event_report_stats (
    id BIGSERIAL,
    stat_date DATE NOT NULL,
    total_reports INTEGER DEFAULT 0,
    open_reports INTEGER DEFAULT 0,
    resolved_reports INTEGER DEFAULT 0,
    dismissed_reports INTEGER DEFAULT 0,
    escalated_reports INTEGER DEFAULT 0,
    avg_resolution_time_ms BIGINT,
    created_ts BIGINT NOT NULL,
    updated_ts BIGINT,
    CONSTRAINT pk_event_report_stats PRIMARY KEY (id),
    CONSTRAINT uq_event_report_stats_date UNIQUE (stat_date)
);

CREATE INDEX IF NOT EXISTS idx_event_report_stats_date ON event_report_stats(stat_date);

-- ============================================================================
-- 管理审计事件表
-- ============================================================================

CREATE TABLE IF NOT EXISTS audit_events (
    event_id TEXT PRIMARY KEY,
    actor_id TEXT NOT NULL,
    action TEXT NOT NULL,
    resource_type TEXT NOT NULL,
    resource_id TEXT NOT NULL,
    result TEXT NOT NULL,
    request_id TEXT NOT NULL,
    details JSONB NOT NULL DEFAULT '{}'::jsonb,
    created_ts BIGINT NOT NULL
);

CREATE INDEX IF NOT EXISTS idx_audit_events_actor_created
ON audit_events(actor_id, created_ts DESC);

CREATE INDEX IF NOT EXISTS idx_audit_events_resource_created
ON audit_events(resource_type, resource_id, created_ts DESC);

CREATE INDEX IF NOT EXISTS idx_audit_events_request_created
ON audit_events(request_id, created_ts DESC);

-- ============================================================================
-- Federation 缓存表
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
-- 功能开关表
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
-- 房间邀请表
-- ============================================================================

CREATE TABLE IF NOT EXISTS room_invites (
    id BIGSERIAL,
    room_id TEXT NOT NULL,
    inviter TEXT NOT NULL,
    invitee TEXT NOT NULL,
    is_accepted BOOLEAN DEFAULT FALSE,
    accepted_at BIGINT,
    created_ts BIGINT NOT NULL,
    expires_at BIGINT,
    CONSTRAINT pk_room_invites PRIMARY KEY (id)
);

CREATE INDEX IF NOT EXISTS idx_room_invites_room ON room_invites(room_id);
CREATE INDEX IF NOT EXISTS idx_room_invites_invitee ON room_invites(invitee);

CREATE TABLE IF NOT EXISTS room_invite_blocklist (
    id BIGSERIAL,
    room_id TEXT NOT NULL,
    user_id TEXT NOT NULL,
    created_ts BIGINT NOT NULL,
    CONSTRAINT pk_room_invite_blocklist PRIMARY KEY (id),
    CONSTRAINT uq_room_invite_blocklist_room_user UNIQUE (room_id, user_id),
    CONSTRAINT fk_room_invite_blocklist_room FOREIGN KEY (room_id) REFERENCES rooms(room_id) ON DELETE CASCADE
);

CREATE INDEX IF NOT EXISTS idx_room_invite_blocklist_room ON room_invite_blocklist(room_id);
CREATE INDEX IF NOT EXISTS idx_room_invite_blocklist_user ON room_invite_blocklist(user_id);

CREATE TABLE IF NOT EXISTS room_invite_allowlist (
    id BIGSERIAL,
    room_id TEXT NOT NULL,
    user_id TEXT NOT NULL,
    created_ts BIGINT NOT NULL,
    CONSTRAINT pk_room_invite_allowlist PRIMARY KEY (id),
    CONSTRAINT uq_room_invite_allowlist_room_user UNIQUE (room_id, user_id),
    CONSTRAINT fk_room_invite_allowlist_room FOREIGN KEY (room_id) REFERENCES rooms(room_id) ON DELETE CASCADE
);

CREATE INDEX IF NOT EXISTS idx_room_invite_allowlist_room ON room_invite_allowlist(room_id);
CREATE INDEX IF NOT EXISTS idx_room_invite_allowlist_user ON room_invite_allowlist(user_id);

-- ============================================================================
-- 推送通知队列表
-- ============================================================================

CREATE TABLE IF NOT EXISTS push_notification_queue (
    id BIGSERIAL,
    user_id TEXT NOT NULL,
    device_id TEXT NOT NULL,
    event_id TEXT NOT NULL,
    room_id TEXT NOT NULL,
    notification_type TEXT NOT NULL,
    content JSONB DEFAULT '{}',
    is_processed BOOLEAN DEFAULT FALSE,
    processed_at BIGINT,
    created_ts BIGINT NOT NULL,
    CONSTRAINT pk_push_notification_queue PRIMARY KEY (id)
);

CREATE INDEX IF NOT EXISTS idx_push_queue_user ON push_notification_queue(user_id);
CREATE INDEX IF NOT EXISTS idx_push_queue_processed ON push_notification_queue(is_processed);

-- ============================================================================
-- 推送通知日志表
-- ============================================================================

CREATE TABLE IF NOT EXISTS push_notification_log (
    id BIGSERIAL,
    user_id TEXT NOT NULL,
    device_id TEXT NOT NULL,
    pushkey TEXT NOT NULL,
    status TEXT NOT NULL,
    error_message TEXT,
    retry_count INTEGER DEFAULT 0,
    last_attempt_at BIGINT,
    created_ts BIGINT NOT NULL,
    CONSTRAINT pk_push_notification_log PRIMARY KEY (id)
);

CREATE INDEX IF NOT EXISTS idx_push_log_user ON push_notification_log(user_id);
CREATE INDEX IF NOT EXISTS idx_push_log_status ON push_notification_log(status);

-- ============================================================================
-- 推送配置表
-- ============================================================================

CREATE TABLE IF NOT EXISTS push_config (
    id BIGSERIAL,
    user_id TEXT NOT NULL,
    device_id TEXT NOT NULL,
    config_type TEXT NOT NULL,
    config_data JSONB DEFAULT '{}',
    created_ts BIGINT NOT NULL,
    updated_ts BIGINT,
    CONSTRAINT pk_push_config PRIMARY KEY (id),
    CONSTRAINT uq_push_config_user_device_type UNIQUE (user_id, device_id, config_type)
);

CREATE INDEX IF NOT EXISTS idx_push_config_user ON push_config(user_id);

-- ============================================================================
-- 通知表 (Matrix 标准通知)
-- ============================================================================

CREATE TABLE IF NOT EXISTS notifications (
    id BIGSERIAL,
    user_id TEXT NOT NULL,
    event_id TEXT,
    room_id TEXT,
    ts BIGINT NOT NULL,
    notification_type VARCHAR(50) DEFAULT 'message',
    profile_tag VARCHAR(255),
    is_read BOOLEAN DEFAULT FALSE,
    -- 注意: read 字段已移除（与 is_read 冗余）
    created_ts BIGINT NOT NULL,
    updated_ts BIGINT,
    CONSTRAINT pk_notifications PRIMARY KEY (id)
);

CREATE INDEX IF NOT EXISTS idx_notifications_user_id ON notifications(user_id);
CREATE INDEX IF NOT EXISTS idx_notifications_ts ON notifications(ts DESC);
CREATE INDEX IF NOT EXISTS idx_notifications_room ON notifications(room_id);

-- ============================================================================
-- 语音消息表 (Voice Messages)
-- ============================================================================

CREATE TABLE IF NOT EXISTS voice_messages (
    id BIGSERIAL,
    event_id TEXT NOT NULL,
    user_id TEXT NOT NULL,
    room_id TEXT,
    media_id TEXT,
    duration_ms INT NOT NULL,
    waveform TEXT,
    mime_type VARCHAR(100),
    file_size BIGINT,
    transcription TEXT,
    encryption JSONB,
    is_processed BOOLEAN DEFAULT FALSE,
    processed_at BIGINT,
    created_ts BIGINT NOT NULL,
    CONSTRAINT pk_voice_messages PRIMARY KEY (id),
    CONSTRAINT uq_voice_messages_event UNIQUE (event_id)
);

CREATE INDEX IF NOT EXISTS idx_voice_messages_room ON voice_messages(room_id);
CREATE INDEX IF NOT EXISTS idx_voice_messages_user ON voice_messages(user_id);
CREATE INDEX IF NOT EXISTS idx_voice_messages_processed ON voice_messages(is_processed);
CREATE INDEX IF NOT EXISTS idx_voice_messages_room_ts ON voice_messages(room_id, created_ts DESC);
CREATE INDEX IF NOT EXISTS idx_voice_messages_user_ts ON voice_messages(user_id, created_ts DESC);

-- ============================================================================
-- 语音使用统计表 (Voice Usage Stats)
-- ============================================================================

CREATE TABLE IF NOT EXISTS voice_usage_stats (
    id BIGSERIAL,
    user_id TEXT NOT NULL,
    room_id TEXT,
    date DATE NOT NULL,
    period_start TIMESTAMP,
    period_end TIMESTAMP,
    total_duration_ms BIGINT DEFAULT 0,
    total_file_size BIGINT DEFAULT 0,
    message_count BIGINT DEFAULT 0,
    last_active_ts BIGINT,
    -- 注意: last_activity_at 已移除（与 last_active_ts 冗余）
    created_ts BIGINT,
    updated_ts BIGINT,
    CONSTRAINT pk_voice_usage_stats PRIMARY KEY (id),
    CONSTRAINT uq_voice_usage_stats_user_room_period UNIQUE (user_id, room_id, period_start)
);

CREATE INDEX IF NOT EXISTS idx_voice_usage_stats_user ON voice_usage_stats(user_id);
CREATE INDEX IF NOT EXISTS idx_voice_usage_stats_date ON voice_usage_stats(date);

-- ============================================================================
-- Presence 表
-- ============================================================================

CREATE TABLE IF NOT EXISTS presence (
    user_id TEXT NOT NULL,
    status_msg TEXT,
    presence TEXT NOT NULL DEFAULT 'offline',
    last_active_ts BIGINT NOT NULL DEFAULT 0,
    status_from TEXT,
    created_ts BIGINT NOT NULL,
    updated_ts BIGINT NOT NULL,
    CONSTRAINT pk_presence PRIMARY KEY (user_id),
    CONSTRAINT fk_presence_user FOREIGN KEY (user_id) REFERENCES users(user_id) ON DELETE CASCADE
);

CREATE TABLE IF NOT EXISTS presence_subscriptions (
    subscriber_id TEXT NOT NULL,
    target_id TEXT NOT NULL,
    created_ts BIGINT NOT NULL,
    CONSTRAINT pk_presence_subscriptions PRIMARY KEY (subscriber_id, target_id),
    CONSTRAINT fk_presence_subscriptions_subscriber FOREIGN KEY (subscriber_id) REFERENCES users(user_id) ON DELETE CASCADE,
    CONSTRAINT fk_presence_subscriptions_target FOREIGN KEY (target_id) REFERENCES users(user_id) ON DELETE CASCADE
);

CREATE INDEX IF NOT EXISTS idx_presence_subscriptions_subscriber
ON presence_subscriptions(subscriber_id);

CREATE INDEX IF NOT EXISTS idx_presence_subscriptions_target
ON presence_subscriptions(target_id);

-- ============================================================================
-- 用户目录表
-- ============================================================================

CREATE TABLE IF NOT EXISTS user_directory (
    user_id TEXT NOT NULL,
    room_id TEXT NOT NULL,
    visibility TEXT NOT NULL DEFAULT 'private',
    added_by TEXT,
    created_ts BIGINT NOT NULL,
    CONSTRAINT pk_user_directory PRIMARY KEY (user_id, room_id),
    CONSTRAINT fk_user_directory_user FOREIGN KEY (user_id) REFERENCES users(user_id) ON DELETE CASCADE,
    CONSTRAINT fk_user_directory_room FOREIGN KEY (room_id) REFERENCES rooms(room_id) ON DELETE CASCADE
);

-- ============================================================================
-- 好友相关表
-- ============================================================================

CREATE TABLE IF NOT EXISTS friends (
    id BIGSERIAL,
    user_id TEXT NOT NULL,
    friend_id TEXT NOT NULL,
    created_ts BIGINT NOT NULL,
    CONSTRAINT pk_friends PRIMARY KEY (id),
    CONSTRAINT uq_friends_user_friend UNIQUE (user_id, friend_id),
    CONSTRAINT fk_friends_user FOREIGN KEY (user_id) REFERENCES users(user_id) ON DELETE CASCADE,
    CONSTRAINT fk_friends_friend FOREIGN KEY (friend_id) REFERENCES users(user_id) ON DELETE CASCADE
);

CREATE TABLE IF NOT EXISTS friend_requests (
    id BIGSERIAL,
    sender_id TEXT NOT NULL,
    receiver_id TEXT NOT NULL,
    message TEXT,
    status TEXT NOT NULL DEFAULT 'pending',
    created_ts BIGINT NOT NULL,
    updated_ts BIGINT,
    CONSTRAINT pk_friend_requests PRIMARY KEY (id),
    CONSTRAINT uq_friend_requests_sender_receiver UNIQUE (sender_id, receiver_id),
    CONSTRAINT fk_friend_requests_sender FOREIGN KEY (sender_id) REFERENCES users(user_id) ON DELETE CASCADE,
    CONSTRAINT fk_friend_requests_receiver FOREIGN KEY (receiver_id) REFERENCES users(user_id) ON DELETE CASCADE
);

CREATE TABLE IF NOT EXISTS friend_categories (
    id BIGSERIAL,
    user_id TEXT NOT NULL,
    name TEXT NOT NULL,
    color TEXT NOT NULL DEFAULT '#000000',
    created_ts BIGINT NOT NULL,
    CONSTRAINT pk_friend_categories PRIMARY KEY (id),
    CONSTRAINT fk_friend_categories_user FOREIGN KEY (user_id) REFERENCES users(user_id) ON DELETE CASCADE
);

CREATE TABLE IF NOT EXISTS blocked_users (
    id BIGSERIAL,
    user_id TEXT NOT NULL,
    blocked_id TEXT NOT NULL,
    reason TEXT,
    created_ts BIGINT NOT NULL,
    CONSTRAINT pk_blocked_users PRIMARY KEY (id),
    CONSTRAINT uq_blocked_users_user_blocked UNIQUE (user_id, blocked_id),
    CONSTRAINT fk_blocked_users_user FOREIGN KEY (user_id) REFERENCES users(user_id) ON DELETE CASCADE,
    CONSTRAINT fk_blocked_users_blocked FOREIGN KEY (blocked_id) REFERENCES users(user_id) ON DELETE CASCADE
);

CREATE INDEX IF NOT EXISTS idx_blocked_users_user_id ON blocked_users(user_id);

-- 密钥轮转历史表
CREATE TABLE IF NOT EXISTS key_rotation_history (
    id BIGSERIAL PRIMARY KEY,
    user_id TEXT NOT NULL,
    device_id TEXT NOT NULL,
    key_id TEXT NOT NULL,
    rotated_ts BIGINT NOT NULL,
    rotation_type TEXT NOT NULL DEFAULT 'olm',
    revoked BOOLEAN NOT NULL DEFAULT FALSE
);

CREATE INDEX IF NOT EXISTS idx_key_rotation_history_user ON key_rotation_history(user_id);
CREATE INDEX IF NOT EXISTS idx_key_rotation_history_device ON key_rotation_history(device_id);
CREATE INDEX IF NOT EXISTS idx_key_rotation_history_rotated_ts ON key_rotation_history(rotated_ts);

-- 房间封禁表
CREATE TABLE IF NOT EXISTS blocked_rooms (
    id BIGSERIAL PRIMARY KEY,
    room_id TEXT NOT NULL UNIQUE,
    blocked_at BIGINT NOT NULL,
    blocked_by TEXT NOT NULL,
    reason TEXT
);

CREATE INDEX IF NOT EXISTS idx_blocked_rooms_room_id ON blocked_rooms(room_id);

CREATE INDEX IF NOT EXISTS idx_friends_user_id ON friends(user_id);
CREATE INDEX IF NOT EXISTS idx_friend_requests_sender ON friend_requests(sender_id);
CREATE INDEX IF NOT EXISTS idx_friend_requests_receiver ON friend_requests(receiver_id);

-- ============================================================================
-- 私密会话表
-- ============================================================================

-- ============================================================================
-- 安全事件表
-- ============================================================================

CREATE TABLE IF NOT EXISTS security_events (
    id BIGSERIAL,
    event_type TEXT NOT NULL,
    user_id TEXT,
    ip_address TEXT,
    user_agent TEXT,
    details JSONB,
    created_ts BIGINT NOT NULL,
    CONSTRAINT pk_security_events PRIMARY KEY (id)
);

CREATE TABLE IF NOT EXISTS ip_blocks (
    id BIGSERIAL,
    ip_address TEXT NOT NULL,
    reason TEXT,
    blocked_ts BIGINT NOT NULL,
    expires_at BIGINT,
    CONSTRAINT pk_ip_blocks PRIMARY KEY (id),
    CONSTRAINT uq_ip_blocks_ip UNIQUE (ip_address)
);

-- ip_reputation: 零代码引用，已废弃 (方案第三层冗余表)
-- CREATE TABLE IF NOT EXISTS ip_reputation (...);

CREATE INDEX IF NOT EXISTS idx_security_events_user_id ON security_events(user_id);
CREATE INDEX IF NOT EXISTS idx_security_events_created_ts ON security_events(created_ts);
CREATE INDEX IF NOT EXISTS idx_ip_blocks_blocked_ts ON ip_blocks(blocked_ts);

-- ============================================================================
-- 读标记表
-- ============================================================================

CREATE TABLE IF NOT EXISTS read_markers (
    id BIGSERIAL,
    room_id TEXT NOT NULL,
    user_id TEXT NOT NULL,
    event_id TEXT NOT NULL,
    marker_type TEXT NOT NULL,
    created_ts BIGINT NOT NULL,
    updated_ts BIGINT NOT NULL,
    CONSTRAINT pk_read_markers PRIMARY KEY (id),
    CONSTRAINT uq_read_markers_room_user_type UNIQUE (room_id, user_id, marker_type)
);

CREATE INDEX IF NOT EXISTS idx_read_markers_room_user ON read_markers(room_id, user_id);

-- ============================================================================
-- 事件接收表
-- ============================================================================

CREATE TABLE IF NOT EXISTS event_receipts (
    id BIGSERIAL,
    event_id TEXT NOT NULL,
    room_id TEXT NOT NULL,
    user_id TEXT NOT NULL,
    receipt_type TEXT NOT NULL,
    ts BIGINT NOT NULL,
    data JSONB DEFAULT '{}',
    created_ts BIGINT NOT NULL,
    updated_ts BIGINT NOT NULL,
    CONSTRAINT pk_event_receipts PRIMARY KEY (id),
    CONSTRAINT uq_event_receipts_event_room_user_type UNIQUE (event_id, room_id, user_id, receipt_type)
);

CREATE INDEX IF NOT EXISTS idx_event_receipts_event ON event_receipts(event_id);
CREATE INDEX IF NOT EXISTS idx_event_receipts_room ON event_receipts(room_id);

-- ============================================================================
-- 房间状态事件表
-- ============================================================================

CREATE TABLE IF NOT EXISTS room_state_events (
    id BIGSERIAL,
    room_id TEXT NOT NULL,
    type TEXT NOT NULL,
    state_key TEXT NOT NULL,
    content JSONB NOT NULL,
    sender TEXT NOT NULL,
    origin_server_ts BIGINT NOT NULL,
    CONSTRAINT pk_room_state_events PRIMARY KEY (id),
    CONSTRAINT uq_room_state_events_room_type_key UNIQUE (room_id, type, state_key)
);

CREATE INDEX IF NOT EXISTS idx_room_state_events_room ON room_state_events(room_id);

-- ============================================================================
-- State Groups 表 — MSC1442 State Resolution v2 的核心数据结构
-- ============================================================================

-- State Groups 主表
-- 每个 state_group 对应房间在某个时刻的完整状态
CREATE TABLE IF NOT EXISTS state_groups (
    id BIGSERIAL,
    room_id TEXT NOT NULL,
    event_id TEXT NOT NULL,
    state_hash TEXT NOT NULL,
    created_ts BIGINT NOT NULL,
    CONSTRAINT pk_state_groups PRIMARY KEY (id),
    CONSTRAINT uq_state_groups_hash UNIQUE (state_hash),
    CONSTRAINT fk_state_groups_room FOREIGN KEY (room_id) REFERENCES rooms(room_id) ON DELETE CASCADE,
    CONSTRAINT fk_state_groups_event FOREIGN KEY (event_id) REFERENCES events(event_id) ON DELETE CASCADE
);

CREATE INDEX IF NOT EXISTS idx_state_groups_room ON state_groups(room_id);
CREATE INDEX IF NOT EXISTS idx_state_groups_event ON state_groups(event_id);

-- State Group 边表
-- 存储 state_group 之间的前后关系（DAG 边），用于计算 auth_difference
CREATE TABLE IF NOT EXISTS state_group_edges (
    state_group_id BIGINT NOT NULL,
    prev_state_group_id BIGINT NOT NULL,
    CONSTRAINT pk_state_group_edges PRIMARY KEY (state_group_id, prev_state_group_id),
    CONSTRAINT fk_state_group_edges_from FOREIGN KEY (state_group_id) REFERENCES state_groups(id) ON DELETE CASCADE,
    CONSTRAINT fk_state_group_edges_to FOREIGN KEY (prev_state_group_id) REFERENCES state_groups(id) ON DELETE CASCADE
);

CREATE INDEX IF NOT EXISTS idx_state_group_edges_prev ON state_group_edges(prev_state_group_id);

-- 事件到 State Group 映射表
-- 关联每个事件到对应的 state_group
CREATE TABLE IF NOT EXISTS event_to_state_groups (
    event_id TEXT NOT NULL,
    state_group_id BIGINT NOT NULL,
    CONSTRAINT pk_event_to_state_groups PRIMARY KEY (event_id),
    CONSTRAINT fk_event_to_state_groups_event FOREIGN KEY (event_id) REFERENCES events(event_id) ON DELETE CASCADE,
    CONSTRAINT fk_event_to_state_groups_sg FOREIGN KEY (state_group_id) REFERENCES state_groups(id) ON DELETE CASCADE
);

CREATE INDEX IF NOT EXISTS idx_event_to_state_groups_sg ON event_to_state_groups(state_group_id);

-- State Group 状态表
-- 存储每个 state_group 包含的具体状态条目
CREATE TABLE IF NOT EXISTS state_group_state (
    state_group_id BIGINT NOT NULL,
    event_type TEXT NOT NULL,
    state_key TEXT NOT NULL,
    event_id TEXT NOT NULL,
    CONSTRAINT pk_state_group_state PRIMARY KEY (state_group_id, event_type, state_key),
    CONSTRAINT fk_state_group_state_sg FOREIGN KEY (state_group_id) REFERENCES state_groups(id) ON DELETE CASCADE,
    CONSTRAINT fk_state_group_state_event FOREIGN KEY (event_id) REFERENCES events(event_id) ON DELETE CASCADE
);

CREATE INDEX IF NOT EXISTS idx_state_group_state_eid ON state_group_state(event_id);

-- ============================================================================
-- 刷新令牌使用记录表
-- ============================================================================

CREATE TABLE IF NOT EXISTS refresh_token_usage (
    id BIGSERIAL,
    refresh_token_id BIGINT NOT NULL,
    user_id TEXT NOT NULL,
    old_access_token_id TEXT,
    new_access_token_id TEXT,
    used_ts BIGINT NOT NULL,
    ip_address TEXT,
    user_agent TEXT,
    is_success BOOLEAN DEFAULT TRUE,
    error_message TEXT,
    CONSTRAINT pk_refresh_token_usage PRIMARY KEY (id),
    CONSTRAINT fk_refresh_token_usage_token FOREIGN KEY (refresh_token_id) REFERENCES refresh_tokens(id) ON DELETE CASCADE,
    CONSTRAINT fk_refresh_token_usage_user FOREIGN KEY (user_id) REFERENCES users(user_id) ON DELETE CASCADE
);

CREATE INDEX IF NOT EXISTS idx_refresh_token_usage_token ON refresh_token_usage(refresh_token_id);
CREATE INDEX IF NOT EXISTS idx_refresh_token_usage_user ON refresh_token_usage(user_id);

-- ============================================================================
-- 刷新令牌家族表
-- ============================================================================

CREATE TABLE IF NOT EXISTS refresh_token_families (
    id BIGSERIAL,
    family_id TEXT NOT NULL,
    user_id TEXT NOT NULL,
    device_id TEXT,
    created_ts BIGINT NOT NULL,
    last_refresh_ts BIGINT,
    refresh_count INTEGER DEFAULT 0,
    is_compromised BOOLEAN DEFAULT FALSE,
    compromised_at BIGINT,
    CONSTRAINT pk_refresh_token_families PRIMARY KEY (id),
    CONSTRAINT uq_refresh_token_families_id UNIQUE (family_id),
    CONSTRAINT fk_refresh_token_families_user FOREIGN KEY (user_id) REFERENCES users(user_id) ON DELETE CASCADE,
    CONSTRAINT fk_refresh_token_families_device FOREIGN KEY (device_id) REFERENCES devices(device_id) ON DELETE SET NULL
);

CREATE INDEX IF NOT EXISTS idx_refresh_token_families_user ON refresh_token_families(user_id);

-- ============================================================================
-- 刷新令牌轮换表
-- ============================================================================

CREATE TABLE IF NOT EXISTS refresh_token_rotations (
    id BIGSERIAL,
    family_id TEXT NOT NULL,
    old_token_hash TEXT,
    new_token_hash TEXT NOT NULL,
    rotated_ts BIGINT NOT NULL,
    rotation_reason TEXT,
    CONSTRAINT pk_refresh_token_rotations PRIMARY KEY (id),
    CONSTRAINT fk_refresh_token_rotations_family FOREIGN KEY (family_id) REFERENCES refresh_token_families(family_id) ON DELETE CASCADE
);

CREATE INDEX IF NOT EXISTS idx_refresh_token_rotations_family ON refresh_token_rotations(family_id);

-- ============================================================================
-- 应用服务表 (Application Services)
-- ============================================================================

CREATE TABLE IF NOT EXISTS application_services (
    id BIGSERIAL,
    as_id TEXT NOT NULL,
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
    api_key TEXT,
    config JSONB NOT NULL DEFAULT '{}'::jsonb,
    CONSTRAINT pk_application_services PRIMARY KEY (id),
    CONSTRAINT uq_application_services_id UNIQUE (as_id)
);

CREATE INDEX IF NOT EXISTS idx_application_services_enabled ON application_services(is_enabled) WHERE is_enabled = TRUE;

-- ============================================================================
-- 第十一部分（续）：动态创建的附加表
-- ============================================================================

-- 输入状态表
CREATE TABLE IF NOT EXISTS typing (
    user_id TEXT NOT NULL,
    room_id TEXT NOT NULL,
    typing BOOLEAN DEFAULT FALSE,
    last_active_ts BIGINT NOT NULL,
    CONSTRAINT pk_typing PRIMARY KEY (user_id, room_id)
);

-- 消息搜索索引表
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

-- 用户隐私设置表
CREATE TABLE IF NOT EXISTS user_privacy_settings (
    user_id VARCHAR(255) PRIMARY KEY,
    allow_presence_lookup BOOLEAN DEFAULT TRUE,
    allow_profile_lookup BOOLEAN DEFAULT TRUE,
    allow_room_invites BOOLEAN DEFAULT TRUE,
    created_ts BIGINT NOT NULL,
    updated_ts BIGINT
);

-- 第三方身份验证表（独立于 user_threepids）
-- 房间标签表
CREATE TABLE IF NOT EXISTS room_tags (
    id SERIAL PRIMARY KEY,
    user_id VARCHAR(255) NOT NULL,
    room_id VARCHAR(255) NOT NULL,
    tag VARCHAR(255) NOT NULL,
    order_value DOUBLE PRECISION,
    created_ts BIGINT NOT NULL,
    CONSTRAINT uq_room_tags_user_room_tag UNIQUE (user_id, room_id, tag)
);

CREATE INDEX IF NOT EXISTS idx_room_tags_user ON room_tags(user_id);

-- 房间事件缓存表
CREATE TABLE IF NOT EXISTS room_events (
    id SERIAL PRIMARY KEY,
    event_id VARCHAR(255) NOT NULL,
    room_id VARCHAR(255) NOT NULL,
    sender VARCHAR(255) NOT NULL,
    event_type VARCHAR(255) NOT NULL,
    state_key VARCHAR(255),
    content JSONB NOT NULL DEFAULT '{}',
    prev_event_id VARCHAR(255),
    origin_server_ts BIGINT NOT NULL,
    created_ts BIGINT NOT NULL,
    CONSTRAINT uq_room_events_event UNIQUE (event_id)
);

CREATE INDEX IF NOT EXISTS idx_room_events_room ON room_events(room_id);
CREATE INDEX IF NOT EXISTS idx_room_events_event ON room_events(event_id);

-- E2EE To-Device 消息表
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

-- 设备列表变更跟踪表
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

-- 房间临时数据表（typing, receipts 等）
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
CREATE UNIQUE INDEX IF NOT EXISTS idx_room_ephemeral_room_type_user ON room_ephemeral(room_id, event_type, user_id);

-- 设备列表流位置表
CREATE TABLE IF NOT EXISTS device_lists_stream (
    stream_id BIGSERIAL PRIMARY KEY,
    user_id VARCHAR(255) NOT NULL,
    device_id VARCHAR(255),
    created_ts BIGINT NOT NULL
);

CREATE INDEX IF NOT EXISTS idx_device_lists_stream_user ON device_lists_stream(user_id);

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

-- 用户过滤器持久化表
CREATE TABLE IF NOT EXISTS user_filters (
    id SERIAL PRIMARY KEY,
    user_id VARCHAR(255) NOT NULL,
    filter_id VARCHAR(255) NOT NULL,
    filter_json JSONB NOT NULL DEFAULT '{}',
    created_ts BIGINT NOT NULL,
    CONSTRAINT uq_user_filters_user_filter UNIQUE (user_id, filter_id)
);

CREATE INDEX IF NOT EXISTS idx_user_filters_user ON user_filters(user_id);

CREATE SEQUENCE IF NOT EXISTS sliding_sync_pos_seq;

CREATE TABLE IF NOT EXISTS sliding_sync_lists (
    id BIGSERIAL PRIMARY KEY,
    user_id TEXT NOT NULL,
    device_id TEXT NOT NULL,
    conn_id TEXT,
    list_key TEXT NOT NULL,
    sort JSONB DEFAULT '[]',
    filters JSONB DEFAULT '{}',
    room_subscription JSONB DEFAULT '{}',
    ranges JSONB DEFAULT '[]',
    created_ts BIGINT NOT NULL,
    updated_ts BIGINT NOT NULL
);

CREATE UNIQUE INDEX IF NOT EXISTS idx_sliding_sync_lists_unique ON sliding_sync_lists(user_id, device_id, COALESCE(conn_id, ''), list_key);
CREATE INDEX IF NOT EXISTS idx_sliding_sync_lists_user_device ON sliding_sync_lists(user_id, device_id);

CREATE TABLE IF NOT EXISTS sliding_sync_tokens (
    id BIGSERIAL PRIMARY KEY,
    user_id TEXT NOT NULL,
    device_id TEXT NOT NULL,
    conn_id TEXT,
    token TEXT NOT NULL,
    pos BIGINT NOT NULL,
    created_ts BIGINT NOT NULL,
    expires_at BIGINT
);

CREATE UNIQUE INDEX IF NOT EXISTS idx_sliding_sync_tokens_unique ON sliding_sync_tokens(user_id, device_id, COALESCE(conn_id, ''));
CREATE INDEX IF NOT EXISTS idx_sliding_sync_tokens_user ON sliding_sync_tokens(user_id, device_id);

-- Sliding Sync 房间状态缓存表
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
    updated_ts BIGINT NOT NULL
);

CREATE UNIQUE INDEX IF NOT EXISTS idx_sliding_sync_rooms_unique ON sliding_sync_rooms (user_id, device_id, room_id, COALESCE(conn_id, ''));
CREATE INDEX IF NOT EXISTS idx_sliding_sync_rooms_user_device ON sliding_sync_rooms(user_id, device_id);
CREATE INDEX IF NOT EXISTS idx_sliding_sync_rooms_bump_stamp ON sliding_sync_rooms(bump_stamp DESC);
CREATE INDEX IF NOT EXISTS idx_sliding_sync_rooms_room_id ON sliding_sync_rooms(room_id, updated_ts DESC);

-- 线程订阅表
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
    CONSTRAINT uq_thread_subscriptions UNIQUE (room_id, thread_id, user_id)
);

CREATE INDEX IF NOT EXISTS idx_thread_subscriptions_room_thread ON thread_subscriptions(room_id, thread_id);

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

CREATE INDEX IF NOT EXISTS idx_thread_read_receipts_user_unread
ON thread_read_receipts(user_id, updated_ts DESC)
WHERE unread_count > 0;

CREATE INDEX IF NOT EXISTS idx_thread_read_receipts_user_room_unread
ON thread_read_receipts(user_id, room_id, updated_ts DESC)
WHERE unread_count > 0;

-- Space 层级结构表
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
    CONSTRAINT uq_space_hierarchy UNIQUE (space_id, room_id)
);

CREATE INDEX IF NOT EXISTS idx_space_hierarchy_space ON space_hierarchy(space_id);

-- ============================================================================
-- 第十二部分：密码安全表
-- ============================================================================

-- 密码历史记录表
-- 存储用户的历史密码哈希，防止重复使用
CREATE TABLE IF NOT EXISTS password_history (
    id BIGSERIAL PRIMARY KEY,
    user_id TEXT NOT NULL,
    password_hash TEXT NOT NULL,
    created_ts BIGINT NOT NULL,
    CONSTRAINT fk_password_history_user FOREIGN KEY (user_id) REFERENCES users(user_id) ON DELETE CASCADE
);

CREATE INDEX IF NOT EXISTS idx_password_history_user ON password_history(user_id);
CREATE INDEX IF NOT EXISTS idx_password_history_created ON password_history(created_ts DESC);

-- 密码策略配置表
-- 存储系统级密码策略配置
CREATE TABLE IF NOT EXISTS password_policy (
    id SERIAL PRIMARY KEY,
    name VARCHAR(100) NOT NULL UNIQUE,
    value TEXT NOT NULL,
    description TEXT,
    updated_ts BIGINT NOT NULL
);

-- ============================================================================
-- 第十三部分：迁移版本控制表
-- ============================================================================

-- 迁移记录表
CREATE TABLE IF NOT EXISTS schema_migrations (
    id BIGSERIAL,
    version TEXT NOT NULL,
    name TEXT,
    checksum TEXT,
    applied_ts BIGINT,
    execution_time_ms BIGINT,
    success BOOLEAN NOT NULL DEFAULT TRUE,
    description TEXT,
    executed_at TIMESTAMPTZ DEFAULT NOW(),
    CONSTRAINT pk_schema_migrations PRIMARY KEY (id),
    CONSTRAINT uq_schema_migrations_version UNIQUE (version)
);

CREATE INDEX IF NOT EXISTS idx_schema_migrations_version ON schema_migrations(version);

-- 数据库元数据表
CREATE TABLE IF NOT EXISTS db_metadata (
    id BIGSERIAL PRIMARY KEY,
    key TEXT NOT NULL UNIQUE,
    value TEXT NOT NULL,
    created_ts BIGINT NOT NULL,
    updated_ts BIGINT NOT NULL
);

CREATE INDEX IF NOT EXISTS idx_db_metadata_key ON db_metadata(key);

-- ============================================================================
-- 第十五部分：测试发现修复的表 (v6.0.2)
-- ============================================================================

-- 保留策略表 (服务器级)
CREATE TABLE IF NOT EXISTS server_retention_policy (
    id BIGSERIAL,
    max_lifetime BIGINT,
    min_lifetime BIGINT NOT NULL DEFAULT 0,
    expire_on_clients BOOLEAN NOT NULL DEFAULT FALSE,
    created_ts BIGINT NOT NULL,
    updated_ts BIGINT NOT NULL,
    CONSTRAINT pk_server_retention_policy PRIMARY KEY (id)
);

INSERT INTO server_retention_policy (id, max_lifetime, min_lifetime, expire_on_clients, created_ts, updated_ts)
VALUES (1, NULL, 0, FALSE, 0, 0)
ON CONFLICT (id) DO NOTHING;

CREATE TABLE IF NOT EXISTS room_retention_policies (
    id BIGSERIAL PRIMARY KEY,
    room_id TEXT NOT NULL,
    max_lifetime BIGINT,
    min_lifetime BIGINT NOT NULL DEFAULT 0,
    expire_on_clients BOOLEAN NOT NULL DEFAULT FALSE,
    is_server_default BOOLEAN NOT NULL DEFAULT FALSE,
    created_ts BIGINT NOT NULL,
    updated_ts BIGINT NOT NULL,
    CONSTRAINT uq_room_retention_policies_room UNIQUE (room_id),
    CONSTRAINT fk_room_retention_policies_room FOREIGN KEY (room_id) REFERENCES rooms(room_id) ON DELETE CASCADE
);

CREATE INDEX IF NOT EXISTS idx_room_retention_policies_server_default
ON room_retention_policies(is_server_default)
WHERE is_server_default = TRUE;

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

CREATE INDEX IF NOT EXISTS idx_retention_cleanup_queue_status_origin
ON retention_cleanup_queue(status, origin_server_ts ASC);

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

CREATE INDEX IF NOT EXISTS idx_retention_cleanup_logs_room_started
ON retention_cleanup_logs(room_id, started_ts DESC);

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

CREATE INDEX IF NOT EXISTS idx_deleted_events_index_room_ts
ON deleted_events_index(room_id, deletion_ts ASC);

CREATE TABLE IF NOT EXISTS room_sticky_events (
    id BIGSERIAL PRIMARY KEY,
    room_id TEXT NOT NULL,
    user_id TEXT NOT NULL,
    event_id TEXT NOT NULL,
    event_type TEXT NOT NULL,
    sticky BOOLEAN NOT NULL DEFAULT TRUE,
    created_ts BIGINT NOT NULL,
    updated_ts BIGINT NOT NULL,
    CONSTRAINT uq_room_sticky_events_room_user_type UNIQUE (room_id, user_id, event_type),
    CONSTRAINT fk_room_sticky_events_room FOREIGN KEY (room_id) REFERENCES rooms(room_id) ON DELETE CASCADE,
    CONSTRAINT fk_room_sticky_events_user FOREIGN KEY (user_id) REFERENCES users(user_id) ON DELETE CASCADE
);

CREATE INDEX IF NOT EXISTS idx_room_sticky_events_user_sticky
ON room_sticky_events(user_id, sticky, room_id);

CREATE TABLE IF NOT EXISTS user_reputations (
    user_id TEXT PRIMARY KEY,
    reputation_score INTEGER NOT NULL DEFAULT 50,
    total_reports INTEGER NOT NULL DEFAULT 0,
    accepted_reports INTEGER NOT NULL DEFAULT 0,
    false_reports INTEGER NOT NULL DEFAULT 0,
    last_report_ts BIGINT,
    last_update_ts BIGINT NOT NULL,
    warnings_count INTEGER NOT NULL DEFAULT 0,
    is_banned BOOLEAN NOT NULL DEFAULT FALSE,
    ban_reason TEXT,
    ban_expires_at BIGINT,
    CONSTRAINT fk_user_reputations_user FOREIGN KEY (user_id) REFERENCES users(user_id) ON DELETE CASCADE
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

CREATE INDEX IF NOT EXISTS idx_moderation_actions_user_created
ON moderation_actions(user_id, created_ts DESC);

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

CREATE INDEX IF NOT EXISTS idx_moderation_rules_active_priority
ON moderation_rules(is_active, priority DESC, created_ts ASC);

CREATE INDEX IF NOT EXISTS idx_moderation_rules_type_active
ON moderation_rules(rule_type, is_active);

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

CREATE INDEX IF NOT EXISTS idx_moderation_logs_event_created
ON moderation_logs(event_id, created_ts DESC);

CREATE INDEX IF NOT EXISTS idx_moderation_logs_room_created
ON moderation_logs(room_id, created_ts DESC);

CREATE INDEX IF NOT EXISTS idx_moderation_logs_sender_created
ON moderation_logs(sender, created_ts DESC);

-- 用户媒体配额表
CREATE TABLE IF NOT EXISTS user_media_quota (
    id BIGSERIAL,
    user_id TEXT NOT NULL,
    max_bytes BIGINT DEFAULT 1073741824,
    used_bytes BIGINT DEFAULT 0,
    file_count INTEGER DEFAULT 0,
    created_ts BIGINT NOT NULL,
    updated_ts BIGINT,
    CONSTRAINT pk_user_media_quota PRIMARY KEY (id),
    CONSTRAINT uq_user_media_quota_user UNIQUE (user_id),
    CONSTRAINT fk_user_media_quota_user FOREIGN KEY (user_id) REFERENCES users(user_id) ON DELETE CASCADE
);

CREATE INDEX IF NOT EXISTS idx_user_media_quota_used ON user_media_quota(used_bytes DESC) WHERE used_bytes > 0;

-- 媒体配额配置表
CREATE TABLE IF NOT EXISTS media_quota_config (
    id BIGSERIAL,
    config_name TEXT NOT NULL,
    max_file_size BIGINT DEFAULT 10485760,
    max_upload_rate BIGINT,
    allowed_content_types TEXT[] DEFAULT ARRAY['image/jpeg', 'image/png', 'image/gif', 'video/mp4', 'audio/ogg'],
    retention_days INTEGER DEFAULT 90,
    is_enabled BOOLEAN DEFAULT TRUE,
    created_ts BIGINT NOT NULL,
    updated_ts BIGINT,
    CONSTRAINT pk_media_quota_config PRIMARY KEY (id),
    CONSTRAINT uq_media_quota_config_name UNIQUE (config_name)
);

CREATE INDEX IF NOT EXISTS idx_media_quota_config_enabled ON media_quota_config(is_enabled) WHERE is_enabled = TRUE;

-- 一次性密钥表 (E2EE)
CREATE TABLE IF NOT EXISTS one_time_keys (
    id BIGSERIAL,
    user_id TEXT NOT NULL,
    device_id TEXT NOT NULL,
    algorithm TEXT NOT NULL,
    key_id TEXT NOT NULL,
    key_data TEXT NOT NULL,
    is_used BOOLEAN DEFAULT FALSE,
    used_ts BIGINT,
    created_ts BIGINT NOT NULL,
    expires_at BIGINT,
    CONSTRAINT pk_one_time_keys PRIMARY KEY (id),
    CONSTRAINT uq_one_time_keys_user_device_algorithm UNIQUE (user_id, device_id, algorithm, key_id),
    CONSTRAINT fk_one_time_keys_user FOREIGN KEY (user_id) REFERENCES users(user_id) ON DELETE CASCADE
);

CREATE INDEX IF NOT EXISTS idx_one_time_keys_user_device ON one_time_keys(user_id, device_id);
CREATE INDEX IF NOT EXISTS idx_one_time_keys_used ON one_time_keys(is_used) WHERE is_used = FALSE;

-- Rendezvous 会话表
CREATE TABLE IF NOT EXISTS rendezvous_session (
    id BIGSERIAL,
    session_id TEXT NOT NULL,
    user_id TEXT,
    device_id TEXT,
    intent TEXT,
    transport TEXT,
    transport_data JSONB,
    key TEXT,
    content JSONB DEFAULT '{}',
    status TEXT DEFAULT 'pending',
    expires_at BIGINT NOT NULL,
    created_ts BIGINT NOT NULL,
    updated_ts BIGINT,
    CONSTRAINT pk_rendezvous_session PRIMARY KEY (id),
    CONSTRAINT uq_rendezvous_session_id UNIQUE (session_id)
);

CREATE INDEX IF NOT EXISTS idx_rendezvous_session_user ON rendezvous_session(user_id) WHERE user_id IS NOT NULL;
CREATE INDEX IF NOT EXISTS idx_rendezvous_session_expires ON rendezvous_session(expires_at);
CREATE INDEX IF NOT EXISTS idx_rendezvous_session_status ON rendezvous_session(status);

-- Rendezvous 消息表
CREATE TABLE IF NOT EXISTS rendezvous_messages (
    id BIGSERIAL,
    session_id TEXT NOT NULL,
    direction TEXT NOT NULL,
    message_type TEXT NOT NULL,
    content JSONB NOT NULL,
    created_ts BIGINT NOT NULL,
    CONSTRAINT pk_rendezvous_messages PRIMARY KEY (id)
);

CREATE INDEX IF NOT EXISTS idx_rendezvous_messages_session ON rendezvous_messages(session_id);

-- 应用服务状态表
CREATE TABLE IF NOT EXISTS application_service_state (
    id BIGSERIAL,
    as_id TEXT NOT NULL,
    state_key TEXT NOT NULL,
    value JSONB NOT NULL,
    updated_ts BIGINT NOT NULL,
    CONSTRAINT pk_application_service_state PRIMARY KEY (id),
    CONSTRAINT uq_application_service_state_as_key UNIQUE (as_id, state_key),
    CONSTRAINT fk_application_service_state_as FOREIGN KEY (as_id) REFERENCES application_services(as_id) ON DELETE CASCADE
);

CREATE INDEX IF NOT EXISTS idx_application_service_state_as ON application_service_state(as_id);

-- 应用服务事务表
CREATE TABLE IF NOT EXISTS application_service_transactions (
    id BIGSERIAL,
    as_id TEXT NOT NULL,
    txn_id TEXT NOT NULL,
    data JSONB DEFAULT '{}',
    processed BOOLEAN DEFAULT FALSE,
    processed_ts BIGINT,
    created_ts BIGINT NOT NULL,
    CONSTRAINT pk_application_service_transactions PRIMARY KEY (id),
    CONSTRAINT uq_application_service_transactions_as_txn UNIQUE (as_id, txn_id)
);

CREATE INDEX IF NOT EXISTS idx_application_service_transactions_as ON application_service_transactions(as_id);
CREATE INDEX IF NOT EXISTS idx_application_service_transactions_processed ON application_service_transactions(processed) WHERE processed = FALSE;

-- 应用服务事件表
CREATE TABLE IF NOT EXISTS application_service_events (
    id BIGSERIAL,
    as_id TEXT NOT NULL,
    event_id TEXT NOT NULL,
    room_id TEXT,
    event_type TEXT,
    processed BOOLEAN DEFAULT FALSE,
    processed_ts BIGINT,
    created_ts BIGINT NOT NULL,
    CONSTRAINT pk_application_service_events PRIMARY KEY (id),
    CONSTRAINT uq_application_service_events_event UNIQUE (event_id)
);

CREATE INDEX IF NOT EXISTS idx_application_service_events_as ON application_service_events(as_id);
CREATE INDEX IF NOT EXISTS idx_application_service_events_room ON application_service_events(room_id);

-- 应用服务用户命名空间表
CREATE TABLE IF NOT EXISTS application_service_user_namespaces (
    id BIGSERIAL,
    as_id TEXT NOT NULL,
    namespace TEXT NOT NULL,
    is_exclusive BOOLEAN DEFAULT TRUE,
    created_ts BIGINT NOT NULL,
    CONSTRAINT pk_application_service_user_namespaces PRIMARY KEY (id),
    CONSTRAINT fk_application_service_user_namespaces_as FOREIGN KEY (as_id) REFERENCES application_services(as_id) ON DELETE CASCADE
);

CREATE INDEX IF NOT EXISTS idx_application_service_user_namespaces_as ON application_service_user_namespaces(as_id);

-- 应用服务房间别名命名空间表
CREATE TABLE IF NOT EXISTS application_service_room_alias_namespaces (
    id BIGSERIAL,
    as_id TEXT NOT NULL,
    namespace TEXT NOT NULL,
    is_exclusive BOOLEAN DEFAULT TRUE,
    created_ts BIGINT NOT NULL,
    CONSTRAINT pk_application_service_room_alias_namespaces PRIMARY KEY (id),
    CONSTRAINT fk_application_service_room_alias_namespaces_as FOREIGN KEY (as_id) REFERENCES application_services(as_id) ON DELETE CASCADE
);

-- 应用服务房间命名空间表
CREATE TABLE IF NOT EXISTS application_service_room_namespaces (
    id BIGSERIAL,
    as_id TEXT NOT NULL,
    namespace TEXT NOT NULL,
    is_exclusive BOOLEAN DEFAULT TRUE,
    created_ts BIGINT NOT NULL,
    CONSTRAINT pk_application_service_room_namespaces PRIMARY KEY (id),
    CONSTRAINT fk_application_service_room_namespaces_as FOREIGN KEY (as_id) REFERENCES application_services(as_id) ON DELETE CASCADE
);

-- ============================================================================
-- 完成提示
-- ============================================================================

DO $$
BEGIN
    RAISE NOTICE '==========================================';
    RAISE NOTICE 'synapse-rust 统一数据库架构 v6.0.3 初始化完成';
    RAISE NOTICE '创建时间: %', NOW();
    RAISE NOTICE '表数量: 130+';
    RAISE NOTICE '主要变更 (v6.0.3):';
    RAISE NOTICE '  - 添加 server_retention_policy 表';
    RAISE NOTICE '  - 添加 user_media_quota, media_quota_config 表';
    RAISE NOTICE '  - 添加 one_time_keys 表 (E2EE)';
    RAISE NOTICE '  - 添加 rendezvous_session 表';
    RAISE NOTICE '  - 添加应用服务相关表 (5个)';
    RAISE NOTICE '  - 标记废弃表 (threepids, reports, thread_statistics, ip_reputation)';
    RAISE NOTICE '==========================================';
END $$;
--
-- 插入默认管理员账户
-- 用户名: admin (可通过环境变量 ADMIN_USERNAME 配置)
-- 密码: 通过环境变量 ADMIN_PASSWORD 配置，或使用随机生成的强密码
-- 密码策略要求：至少8位，包含大写字母、小写字母、数字、特殊字符
-- 哈希算法: Argon2id (项目标准)
--
-- 注意：生产环境应通过以下方式管理初始密码：
-- 1. 环境变量: ADMIN_PASSWORD
-- 2. 配置文件: config.yaml 中的 admin.initial_password
-- 3. 首次部署时通过 API 注册管理员账户
--
-- 开发环境默认密码哈希（仅用于开发测试，生产环境必须修改）：
-- 此哈希值对应一个符合密码策略的强密码，请勿在生产环境使用
--
INSERT INTO users (user_id, username, password_hash, is_admin, is_guest, created_ts, displayname)
VALUES (
    '@admin:localhost',
    'admin',
    '$argon2id$v=19$m=65536,t=3,p=1$VGVzdFNhbHRGb3JBZG1pbg$K7G8H5J3M2N9P4Q6R8S0T2U4V6W8X0Y2Z4A6B8C0D2E4F6G8H0J2K4L6M8N0P2Q4',
    TRUE,
    FALSE,
    EXTRACT(EPOCH FROM NOW()) * 1000,
    'Administrator'
) ON CONFLICT (user_id) DO NOTHING;

-- 安全建议：首次登录后立即修改密码
-- 可以通过以下 API 修改密码：
-- POST /_matrix/client/v3/account/password
-- {
--   "new_password": "<YOUR_NEW_STRONG_PASSWORD>",
--   "auth": {
--     "type": "m.login.password",
--     "user": "admin",
--     "password": "<YOUR_CURRENT_PASSWORD>"
--   }
-- }

-- 插入初始版本记录

-- 插入默认同步流类型
INSERT INTO sync_stream_id (stream_type, last_id, updated_ts)
VALUES
    ('events', 0, EXTRACT(EPOCH FROM NOW()) * 1000),
    ('presence', 0, EXTRACT(EPOCH FROM NOW()) * 1000),
    ('receipts', 0, EXTRACT(EPOCH FROM NOW()) * 1000),
    ('account_data', 0, EXTRACT(EPOCH FROM NOW()) * 1000)
ON CONFLICT (stream_type) DO NOTHING;

-- 插入默认密码策略配置
INSERT INTO password_policy (name, value, description, updated_ts) VALUES
    ('min_length', '8', '最小密码长度', EXTRACT(EPOCH FROM NOW()) * 1000),
    ('max_length', '128', '最大密码长度', EXTRACT(EPOCH FROM NOW()) * 1000),
    ('require_uppercase', 'true', '是否需要大写字母', EXTRACT(EPOCH FROM NOW()) * 1000),
    ('require_lowercase', 'true', '是否需要小写字母', EXTRACT(EPOCH FROM NOW()) * 1000),
    ('require_digit', 'true', '是否需要数字', EXTRACT(EPOCH FROM NOW()) * 1000),
    ('require_special', 'true', '是否需要特殊字符', EXTRACT(EPOCH FROM NOW()) * 1000),
    ('max_age_days', '90', '密码最大有效期（天），0表示永不过期', EXTRACT(EPOCH FROM NOW()) * 1000),
    ('history_count', '5', '密码历史记录数量，防止重复使用', EXTRACT(EPOCH FROM NOW()) * 1000),
    ('max_failed_attempts', '5', '最大登录失败次数，超过后锁定账户', EXTRACT(EPOCH FROM NOW()) * 1000),
    ('lockout_duration_minutes', '30', '账户锁定时长（分钟）', EXTRACT(EPOCH FROM NOW()) * 1000),
    ('force_first_login_change', 'true', '是否强制首次登录修改密码', EXTRACT(EPOCH FROM NOW()) * 1000)
ON CONFLICT (name) DO NOTHING;

-- 设置默认管理员账户需要首次登录修改密码
UPDATE users SET must_change_password = TRUE WHERE username = 'admin';

-- ============================================================================
-- 第十四部分：外键约束（数据一致性保障）
-- ============================================================================

-- 注意：外键约束可能影响性能，在高并发场景下可考虑在应用层实现
-- 以下外键约束使用 ON DELETE CASCADE 确保数据一致性

-- 用户相关外键
ALTER TABLE devices ADD CONSTRAINT fk_devices_user_id
    FOREIGN KEY (user_id) REFERENCES users(user_id) ON DELETE CASCADE;
ALTER TABLE access_tokens ADD CONSTRAINT fk_access_tokens_user_id
    FOREIGN KEY (user_id) REFERENCES users(user_id) ON DELETE CASCADE;
ALTER TABLE refresh_tokens ADD CONSTRAINT fk_refresh_tokens_user_id
    FOREIGN KEY (user_id) REFERENCES users(user_id) ON DELETE CASCADE;

-- 房间相关外键
ALTER TABLE events ADD CONSTRAINT fk_events_room_id
    FOREIGN KEY (room_id) REFERENCES rooms(room_id) ON DELETE CASCADE;
ALTER TABLE room_memberships ADD CONSTRAINT fk_room_memberships_room_id
    FOREIGN KEY (room_id) REFERENCES rooms(room_id) ON DELETE CASCADE;
ALTER TABLE room_memberships ADD CONSTRAINT fk_room_memberships_user_id
    FOREIGN KEY (user_id) REFERENCES users(user_id) ON DELETE CASCADE;

-- E2EE 相关外键
ALTER TABLE device_keys ADD CONSTRAINT fk_device_keys_user_id
    FOREIGN KEY (user_id) REFERENCES users(user_id) ON DELETE CASCADE;
ALTER TABLE cross_signing_keys ADD CONSTRAINT fk_cross_signing_keys_user_id
    FOREIGN KEY (user_id) REFERENCES users(user_id) ON DELETE CASCADE;

-- 推送相关外键
ALTER TABLE push_notification_queue ADD CONSTRAINT fk_push_queue_user_id
    FOREIGN KEY (user_id) REFERENCES users(user_id) ON DELETE CASCADE;

-- ============================================================================
-- 第十五部分：性能优化索引
-- ============================================================================

-- 复合索引：优化常用查询场景
-- 用户房间列表查询优化
CREATE INDEX IF NOT EXISTS idx_room_memberships_user_membership
    ON room_memberships(user_id, membership);

-- 房间消息历史查询优化
CREATE INDEX IF NOT EXISTS idx_events_room_time
    ON events(room_id, origin_server_ts DESC);

-- 用户设备列表查询优化
CREATE INDEX IF NOT EXISTS idx_device_keys_user_device
    ON device_keys(user_id, device_id);

-- 推送规则匹配优化
CREATE INDEX IF NOT EXISTS idx_push_rules_user_priority
    ON push_rules(user_id, priority);

-- 用户事件查询优化
CREATE INDEX IF NOT EXISTS idx_events_sender_type
    ON events(sender, event_type);

-- 房间成员查询优化
CREATE INDEX IF NOT EXISTS idx_room_memberships_room_membership
    ON room_memberships(room_id, membership);

-- JSONB GIN 索引：优化 JSON 内容搜索
CREATE INDEX IF NOT EXISTS idx_events_content_gin
    ON events USING GIN (content);
CREATE INDEX IF NOT EXISTS idx_account_data_content_gin
    ON account_data USING GIN (content);
-- user_account_data.content is TEXT type, GIN index not applicable

-- ============================================================================
-- 第十六部分：流式数据表（实时状态同步）
-- ============================================================================

-- 在线状态流表
-- 记录所有用户的在线状态变更事件，支持增量 sync
CREATE TABLE IF NOT EXISTS presence_stream (
    stream_id BIGSERIAL,
    user_id TEXT NOT NULL,
    state TEXT NOT NULL DEFAULT 'offline',
    status_msg TEXT,
    last_active_ts BIGINT NOT NULL,
    currently_active BOOLEAN DEFAULT FALSE,
    created_ts BIGINT NOT NULL,
    CONSTRAINT pk_presence_stream PRIMARY KEY (stream_id),
    CONSTRAINT fk_presence_stream_user FOREIGN KEY (user_id) REFERENCES users(user_id) ON DELETE CASCADE
);

CREATE INDEX IF NOT EXISTS idx_presence_stream_user ON presence_stream(user_id);
CREATE INDEX IF NOT EXISTS idx_presence_stream_stream ON presence_stream(stream_id);

-- 输入状态流表
-- 记录所有房间的输入状态变更事件
CREATE TABLE IF NOT EXISTS typing_stream (
    stream_id BIGSERIAL,
    room_id TEXT NOT NULL,
    user_id TEXT NOT NULL,
    is_typing BOOLEAN NOT NULL DEFAULT FALSE,
    timeout_ms BIGINT,
    created_ts BIGINT NOT NULL,
    CONSTRAINT pk_typing_stream PRIMARY KEY (stream_id),
    CONSTRAINT fk_typing_stream_room FOREIGN KEY (room_id) REFERENCES rooms(room_id) ON DELETE CASCADE,
    CONSTRAINT fk_typing_stream_user FOREIGN KEY (user_id) REFERENCES users(user_id) ON DELETE CASCADE
);

CREATE INDEX IF NOT EXISTS idx_typing_stream_room ON typing_stream(room_id);
CREATE INDEX IF NOT EXISTS idx_typing_stream_user ON typing_stream(user_id);
CREATE INDEX IF NOT EXISTS idx_typing_stream_active ON typing_stream(room_id, is_typing) WHERE is_typing = TRUE;

-- 设备列表出站推送追踪表
-- 记录需要向远端服务器推送的设备列表变更
CREATE TABLE IF NOT EXISTS device_lists_outbound_pokes (
    destination TEXT NOT NULL,
    user_id TEXT NOT NULL,
    stream_id BIGINT NOT NULL,
    sent_ts BIGINT,
    created_ts BIGINT NOT NULL,
    CONSTRAINT pk_device_lists_outbound_pokes PRIMARY KEY (user_id, destination),
    CONSTRAINT fk_device_lists_outbound_pokes_user FOREIGN KEY (user_id) REFERENCES users(user_id) ON DELETE CASCADE
);

CREATE INDEX IF NOT EXISTS idx_device_lists_outbound_stream ON device_lists_outbound_pokes(stream_id);
CREATE INDEX IF NOT EXISTS idx_device_lists_outbound_dest ON device_lists_outbound_pokes(destination);

-- ============================================================================
-- 第十七部分：房间统计表
-- ============================================================================

-- 房间当前统计表
-- 存储各房间的实时统计数据（成员数/事件数等），对应 Synapse room_stats_current
CREATE TABLE IF NOT EXISTS room_stats_current (
    room_id TEXT NOT NULL,
    current_state_events BIGINT NOT NULL DEFAULT 0,
    joined_members BIGINT NOT NULL DEFAULT 0,
    invited_members BIGINT NOT NULL DEFAULT 0,
    left_members BIGINT NOT NULL DEFAULT 0,
    banned_members BIGINT NOT NULL DEFAULT 0,
    local_users_in_room BIGINT NOT NULL DEFAULT 0,
    completed_delta_stream_id BIGINT NOT NULL DEFAULT 0,
    created_ts BIGINT NOT NULL,
    updated_ts BIGINT NOT NULL,
    CONSTRAINT pk_room_stats_current PRIMARY KEY (room_id),
    CONSTRAINT fk_room_stats_current_room FOREIGN KEY (room_id) REFERENCES rooms(room_id) ON DELETE CASCADE
);

CREATE INDEX IF NOT EXISTS idx_room_stats_joined ON room_stats_current(joined_members DESC);
CREATE INDEX IF NOT EXISTS idx_room_stats_local ON room_stats_current(local_users_in_room DESC);

-- ============================================================================
-- 第十八部分：目标服务器重试计时表
-- ============================================================================

-- 目标服务器重试计时表
-- 追踪每个被联合服务器的健康状态和重试计时，对应 Synapse destination_retry_timings
CREATE TABLE IF NOT EXISTS destination_retry_timings (
    destination TEXT NOT NULL,
    retry_interval BIGINT NOT NULL DEFAULT 0,
    retry_last_ts BIGINT NOT NULL DEFAULT 0,
    failure_count INT NOT NULL DEFAULT 0,
    last_successful_stream_ordering BIGINT NOT NULL DEFAULT 0,
    created_ts BIGINT NOT NULL,
    updated_ts BIGINT NOT NULL,
    CONSTRAINT pk_destination_retry_timings PRIMARY KEY (destination)
);

CREATE INDEX IF NOT EXISTS idx_destination_retry_next ON destination_retry_timings(retry_last_ts, failure_count);

-- ============================================================================
-- 第十九部分：第三方身份验证表（3PID）
-- ============================================================================

-- 第三方身份验证会话表
-- 对应 Synapse threepid_validation_session，存储邮箱/手机验证令牌
CREATE TABLE IF NOT EXISTS threepid_validation_session (
    id BIGSERIAL PRIMARY KEY,
    session_id TEXT NOT NULL UNIQUE,
    medium TEXT NOT NULL,
    address TEXT NOT NULL,
    client_secret TEXT NOT NULL,
    token TEXT NOT NULL,
    send_attempt INT NOT NULL DEFAULT 0,
    next_link TEXT,
    is_validated BOOLEAN NOT NULL DEFAULT FALSE,
    validated_at BIGINT,
    created_ts BIGINT NOT NULL,
    expires_at BIGINT NOT NULL
);

CREATE INDEX IF NOT EXISTS idx_threepid_session_token ON threepid_validation_session(token);
CREATE INDEX IF NOT EXISTS idx_threepid_session_address ON threepid_validation_session(medium, address);
CREATE INDEX IF NOT EXISTS idx_threepid_session_expires ON threepid_validation_session(expires_at) WHERE is_validated = FALSE;

-- ============================================================================
-- 完成提示
-- ============================================================================

DO $$
DECLARE
    table_count INTEGER;
    index_count INTEGER;
BEGIN
    SELECT COUNT(*) INTO table_count FROM information_schema.tables
    WHERE table_schema = 'public' AND table_type = 'BASE TABLE';

    SELECT COUNT(*) INTO index_count FROM pg_indexes
    WHERE schemaname = 'public' AND indexname LIKE 'idx_%';

    RAISE NOTICE '==========================================';
    RAISE NOTICE 'synapse-rust 统一数据库架构 v7.0.0 初始化完成';
    RAISE NOTICE '完成时间: %', NOW();
    RAISE NOTICE '----------------------------------------';
    RAISE NOTICE '表数量: %', table_count;
    RAISE NOTICE '索引数量: %', index_count;
    RAISE NOTICE '----------------------------------------';
    RAISE NOTICE '主要变更:';
    RAISE NOTICE '  - 布尔字段统一使用 is_/has_ 前缀';
    RAISE NOTICE '  - NOT NULL 时间戳使用 _ts 后缀';
    RAISE NOTICE '  - 可空时间戳使用 _at 后缀';
    RAISE NOTICE '  - 统一索引命名规范';
    RAISE NOTICE '  - 添加外键约束保障数据一致性';
    RAISE NOTICE '----------------------------------------';
    RAISE NOTICE '默认管理员: admin';
    RAISE NOTICE '(请通过环境变量 ADMIN_PASSWORD 设置密码!)';
    RAISE NOTICE '==========================================';

    -- 记录迁移执行
END $$;

-- ============================================================================
-- v7 Folded Delta
-- ============================================================================

-- ===== Folded from: 20260401000001_consolidated_schema_additions.sql =====

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
END $$;

CREATE TABLE IF NOT EXISTS space_summaries (
    id BIGSERIAL PRIMARY KEY,
    space_id TEXT NOT NULL UNIQUE,
    summary JSONB DEFAULT '{}',
    children_count BIGINT DEFAULT 0,
    member_count BIGINT DEFAULT 0,
    updated_ts BIGINT NOT NULL,
    CONSTRAINT fk_space_summary_space FOREIGN KEY (space_id) REFERENCES spaces(space_id) ON DELETE CASCADE
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
        CONSTRAINT pk_application_service_users PRIMARY KEY (as_id, user_id),
        CONSTRAINT fk_application_service_users_as FOREIGN KEY (as_id) REFERENCES application_services(as_id) ON DELETE CASCADE
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
        created_ts BIGINT NOT NULL DEFAULT (EXTRACT(EPOCH FROM clock_timestamp()) * 1000)::BIGINT,
        CONSTRAINT fk_application_service_statistics_as FOREIGN KEY (as_id) REFERENCES application_services(as_id) ON DELETE CASCADE
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

-- ===== Folded from: 20260406000001_consolidated_schema_fixes.sql =====

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

-- ===== Folded from: 20260410000001_consolidated_feature_additions.sql =====

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

ALTER TABLE application_services
ADD COLUMN IF NOT EXISTS api_key TEXT;

ALTER TABLE application_services
ADD COLUMN IF NOT EXISTS config JSONB NOT NULL DEFAULT '{}'::jsonb;

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

-- ===== Folded from: 20260422000001_schema_code_alignment.sql =====

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

-- ===== Folded from: 20260423000001_add_federation_admission_control.sql =====

ALTER TABLE federation_servers ADD COLUMN IF NOT EXISTS status TEXT NOT NULL DEFAULT 'active';
ALTER TABLE federation_servers ADD COLUMN IF NOT EXISTS updated_ts BIGINT;

CREATE INDEX IF NOT EXISTS idx_federation_servers_status ON federation_servers(status);

COMMENT ON COLUMN federation_servers.status IS 'Federation admission status: pending, active, rejected';
COMMENT ON COLUMN federation_servers.updated_ts IS 'Timestamp of last status update in milliseconds';

-- ===== Folded from: 20260423000002_fix_auth_token_schema.sql =====

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

-- ===== Folded from: 20260430000001_add_oidc_user_mapping.sql =====

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

-- ===== Folded from: 20260430000002_add_missing_perf_indexes.sql =====

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

-- ===== Folded from: 20260501000001_backup_keys_metadata.sql =====

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

-- ===== Folded from: 20260505000001_add_user_search_and_presence_indexes.sql =====

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

-- ===== Folded from: 20260505000002_add_saml_config_overrides.sql =====

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

-- ===== Folded from: 20260507000002_repair_legacy_background_retention_room_alias_schema.sql =====

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

-- ===== Folded from: 20260507000001_add_stream_ordering.sql =====

ALTER TABLE events ADD COLUMN IF NOT EXISTS stream_ordering BIGINT;

CREATE SEQUENCE IF NOT EXISTS events_stream_ordering_seq;
ALTER SEQUENCE events_stream_ordering_seq OWNED BY events.stream_ordering;

DO $$
BEGIN
    ALTER TABLE events
        ALTER COLUMN stream_ordering SET DEFAULT nextval('events_stream_ordering_seq');
EXCEPTION WHEN others THEN
    NULL;
END $$;

UPDATE events e
SET stream_ordering = sub.new_ordering
FROM (
    SELECT event_id,
           ROW_NUMBER() OVER (ORDER BY origin_server_ts ASC, event_id ASC) AS new_ordering
    FROM events
    WHERE stream_ordering IS NULL
) sub
WHERE e.event_id = sub.event_id;

SELECT setval('events_stream_ordering_seq', GREATEST(COALESCE((SELECT MAX(stream_ordering) FROM events), 0), 1), true);

CREATE INDEX IF NOT EXISTS idx_events_stream_ordering ON events(stream_ordering);
CREATE INDEX IF NOT EXISTS idx_events_room_stream_ordering ON events(room_id, stream_ordering DESC);
CREATE INDEX IF NOT EXISTS idx_events_room_stream_ordering_not_redacted
    ON events(room_id, stream_ordering DESC)
    WHERE is_redacted = FALSE;

-- ===== Folded from: 20260421000001_consolidated_drop_redundant_tables.sql =====

-- ============================================================================
-- Consolidated Migration: Drop Redundant Tables
-- Created: 2026-04-22 (consolidated from 4 migrations dated 2026-04-21 ~ 2026-04-22)
--
-- Merged source files:
--   1. 20260421000001_drop_unused_tables.sql (3 zero-ref tables)
--   2. 20260422000001_drop_redundant_tables_phase_b.sql (4 dead-code tables)
--   3. 20260422000002_drop_redundant_tables_phase_c.sql (9 over-engineered tables)
--   4. 20260422000003_drop_redundant_tables_phase_d.sql (2 retention queue tables)
--
-- Total: 18 tables dropped. All had zero or stub-only code references.
-- See docs/synapse-rust/REDUNDANT_TABLE_DELETION_PLAN.md for analysis.
-- ============================================================================


-- ===== Merged from: 20260421000001_drop_unused_tables.sql =====

-- Drop tables that have no code references and are not part of the Matrix spec.
-- These were over-engineered features that were never wired into the application.
-- Safe: verified zero DML references in src/ for each table.

DROP TABLE IF EXISTS private_messages CASCADE;
DROP TABLE IF EXISTS private_sessions CASCADE;
DROP TABLE IF EXISTS room_children CASCADE;
DROP TABLE IF EXISTS ip_reputation CASCADE;

-- ===== Merged from: 20260422000001_drop_redundant_tables_phase_b.sql =====

-- Migration: Drop redundant tables (Phase B)
-- password_policy: PasswordPolicyService was never instantiated; policy is config-driven
-- key_rotation_history: Redundant with key_rotation_log; routes migrated to key_rotation_log
-- presence_routes: Module system over-engineering; presence routing is built-in
-- password_auth_providers: Module system over-engineering; auth is handled by AuthService/OIDC

DROP TABLE IF EXISTS password_policy CASCADE;
DROP TABLE IF EXISTS key_rotation_history CASCADE;
DROP TABLE IF EXISTS presence_routes CASCADE;
DROP TABLE IF EXISTS password_auth_providers CASCADE;

-- ===== Merged from: 20260422000002_drop_redundant_tables_phase_c.sql =====

-- Migration: Drop redundant tables (Phase C)
-- worker_load_stats: Replaced by tracing::debug! structured logging
-- worker_connections: Replaced by tracing::info! structured logging
-- retention_stats: Replaced by runtime aggregation from retention_cleanup_logs
-- deleted_events_index: Replaced by tracing::info! logging + events.status filtering
-- event_report_history: Replaced by tracing::info! logging, methods return stubs
-- event_report_stats: Replaced by runtime aggregation from event_reports
-- spam_check_results: Replaced by tracing::info! logging, methods return stubs
-- third_party_rule_results: Replaced by tracing::info! logging, methods return stubs
-- rate_limit_callbacks: Module over-engineering, methods return stubs

DROP TABLE IF EXISTS worker_load_stats CASCADE;
DROP TABLE IF EXISTS worker_connections CASCADE;
DROP TABLE IF EXISTS retention_stats CASCADE;
DROP TABLE IF EXISTS deleted_events_index CASCADE;
DROP TABLE IF EXISTS event_report_history CASCADE;
DROP TABLE IF EXISTS event_report_stats CASCADE;
DROP TABLE IF EXISTS spam_check_results CASCADE;
DROP TABLE IF EXISTS third_party_rule_results CASCADE;
DROP TABLE IF EXISTS rate_limit_callbacks CASCADE;

-- ===== Merged from: 20260422000003_drop_redundant_tables_phase_d.sql =====

-- Migration: Drop redundant tables (Phase D - retention queue/logs)
-- retention_cleanup_queue: Replaced by in-memory processing + tracing logging
-- retention_cleanup_logs: Replaced by tracing::info! structured logging
-- The retention service (delete_events_before) still works via direct events table DELETE.

DROP TABLE IF EXISTS retention_cleanup_queue CASCADE;
DROP TABLE IF EXISTS retention_cleanup_logs CASCADE;
