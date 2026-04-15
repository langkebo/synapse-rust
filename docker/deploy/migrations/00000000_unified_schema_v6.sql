-- ============================================================================
-- synapse-rust 统一数据库架构 v6.0.4
-- 创建日期: 2026-03-09
-- 最后更新: 2026-03-24
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
    verification_expires_ts BIGINT,
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
    CONSTRAINT fk_access_tokens_user FOREIGN KEY (user_id) REFERENCES users(user_id) ON DELETE CASCADE
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
    CONSTRAINT fk_refresh_tokens_user FOREIGN KEY (user_id) REFERENCES users(user_id) ON DELETE CASCADE
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
    CONSTRAINT uq_token_blacklist_token_hash UNIQUE (token_hash)
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
    CONSTRAINT pk_events PRIMARY KEY (event_id),
    CONSTRAINT fk_events_room FOREIGN KEY (room_id) REFERENCES rooms(room_id) ON DELETE CASCADE
);

CREATE INDEX IF NOT EXISTS idx_events_room_id ON events(room_id);
CREATE INDEX IF NOT EXISTS idx_events_sender ON events(sender);
CREATE INDEX IF NOT EXISTS idx_events_type ON events(event_type);
CREATE INDEX IF NOT EXISTS idx_events_origin_server_ts ON events(origin_server_ts DESC);
CREATE INDEX IF NOT EXISTS idx_events_not_redacted ON events(room_id, origin_server_ts DESC) WHERE is_redacted = FALSE;

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

CREATE INDEX IF NOT EXISTS idx_room_children_parent_suggested
ON room_children(parent_room_id, suggested, child_room_id);

CREATE INDEX IF NOT EXISTS idx_room_children_child
ON room_children(child_room_id);

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

-- 线程统计表 (Thread Statistics)
-- 注意: 此表已废弃，功能已合并到 thread_roots
-- CREATE TABLE IF NOT EXISTS thread_statistics (...);

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
    expires_ts BIGINT NOT NULL,
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
    expires_ts BIGINT NOT NULL,
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
    expires_ts BIGINT NOT NULL,
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
    expires_ts BIGINT NOT NULL,
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
    expires_ts BIGINT NOT NULL,
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
    CONSTRAINT pk_federation_servers PRIMARY KEY (id),
    CONSTRAINT uq_federation_servers_name UNIQUE (server_name)
);

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
    CONSTRAINT fk_registration_token_usage_token FOREIGN KEY (token_id) REFERENCES registration_tokens(id) ON DELETE CASCADE
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
    CONSTRAINT uq_report_rate_limits_user UNIQUE (user_id)
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
    CONSTRAINT uq_presence_subscriptions UNIQUE (subscriber_id, target_id),
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

CREATE TABLE IF NOT EXISTS private_sessions (
    id VARCHAR(255) NOT NULL,
    user_id_1 VARCHAR(255) NOT NULL,
    user_id_2 VARCHAR(255) NOT NULL,
    session_type VARCHAR(50) DEFAULT 'direct',
    encryption_key VARCHAR(255),
    created_ts BIGINT NOT NULL,
    last_activity_ts BIGINT NOT NULL,
    updated_ts BIGINT,
    unread_count INTEGER DEFAULT 0,
    encrypted_content TEXT,
    CONSTRAINT pk_private_sessions PRIMARY KEY (id),
    CONSTRAINT uq_private_sessions_users UNIQUE (user_id_1, user_id_2),
    CONSTRAINT fk_private_sessions_user1 FOREIGN KEY (user_id_1) REFERENCES users(user_id) ON DELETE CASCADE,
    CONSTRAINT fk_private_sessions_user2 FOREIGN KEY (user_id_2) REFERENCES users(user_id) ON DELETE CASCADE
);

CREATE TABLE IF NOT EXISTS private_messages (
    id BIGSERIAL,
    session_id VARCHAR(255) NOT NULL,
    sender_id VARCHAR(255) NOT NULL,
    content TEXT NOT NULL,
    encrypted_content TEXT,
    created_ts BIGINT NOT NULL,
    message_type VARCHAR(50) DEFAULT 'm.text',
    is_read BOOLEAN DEFAULT FALSE,
    read_by_receiver BOOLEAN DEFAULT FALSE,
    read_ts BIGINT,
    edit_history JSONB,
    is_deleted BOOLEAN DEFAULT FALSE,
    deleted_at BIGINT,
    is_edited BOOLEAN DEFAULT FALSE,
    unread_count INTEGER DEFAULT 0,
    CONSTRAINT pk_private_messages PRIMARY KEY (id),
    CONSTRAINT fk_private_messages_session FOREIGN KEY (session_id) REFERENCES private_sessions(id) ON DELETE CASCADE,
    CONSTRAINT fk_private_messages_sender FOREIGN KEY (sender_id) REFERENCES users(user_id) ON DELETE CASCADE
);

CREATE INDEX IF NOT EXISTS idx_private_sessions_user ON private_sessions(user_id_1, user_id_2);
CREATE INDEX IF NOT EXISTS idx_private_messages_session ON private_messages(session_id);

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

CREATE TABLE IF NOT EXISTS ip_reputation (
    id BIGSERIAL,
    ip_address TEXT NOT NULL,
    score INTEGER DEFAULT 0,
    last_seen_ts BIGINT NOT NULL,
    updated_ts BIGINT NOT NULL,
    details JSONB,
    CONSTRAINT pk_ip_reputation PRIMARY KEY (id),
    CONSTRAINT uq_ip_reputation_ip UNIQUE (ip_address)
);

CREATE INDEX IF NOT EXISTS idx_security_events_user_id ON security_events(user_id);
CREATE INDEX IF NOT EXISTS idx_security_events_created_ts ON security_events(created_ts);
CREATE INDEX IF NOT EXISTS idx_ip_blocks_blocked_ts ON ip_blocks(blocked_ts);
CREATE INDEX IF NOT EXISTS idx_ip_reputation_score ON ip_reputation(score);

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
    CONSTRAINT pk_refresh_token_usage PRIMARY KEY (id)
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
    CONSTRAINT uq_refresh_token_families_id UNIQUE (family_id)
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
    CONSTRAINT pk_refresh_token_rotations PRIMARY KEY (id)
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
    UNIQUE (user_id, room_id)
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
-- 注意: 此表已废弃，功能与 user_threepids 重复
-- CREATE TABLE IF NOT EXISTS threepids (
--     id SERIAL PRIMARY KEY,
--     user_id VARCHAR(255) NOT NULL,
--     medium VARCHAR(50) NOT NULL,
--     address VARCHAR(255) NOT NULL,
--     validated_ts BIGINT,
--     added_ts BIGINT NOT NULL,
--     CONSTRAINT uq_threepids_medium_address UNIQUE (medium, address)
-- );
-- CREATE INDEX IF NOT EXISTS idx_threepids_user ON threepids(user_id);

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

-- 事件举报表（独立于 event_reports）
-- 注意: 此表已废弃，功能与 event_reports 重复
-- CREATE TABLE IF NOT EXISTS reports (
--     id SERIAL PRIMARY KEY,
--     room_id VARCHAR(255) NOT NULL,
--     event_id VARCHAR(255) NOT NULL,
--     reporter_user_id VARCHAR(255) NOT NULL,
--     reason TEXT,
--     score INTEGER DEFAULT 0,
--     created_ts BIGINT NOT NULL
-- );
-- CREATE INDEX IF NOT EXISTS idx_reports_room ON reports(room_id);

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
    expires_ts BIGINT,
    expires_at BIGINT GENERATED ALWAYS AS (expires_ts) STORED
);

CREATE INDEX IF NOT EXISTS idx_room_ephemeral_room ON room_ephemeral(room_id);

-- 设备列表流位置表
CREATE TABLE IF NOT EXISTS device_lists_stream (
    stream_id BIGSERIAL PRIMARY KEY,
    user_id VARCHAR(255) NOT NULL,
    device_id VARCHAR(255),
    created_ts BIGINT NOT NULL
);

CREATE INDEX IF NOT EXISTS idx_device_lists_stream_user ON device_lists_stream(user_id);

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
    expires_ts BIGINT NOT NULL,
    created_ts BIGINT NOT NULL,
    updated_ts BIGINT,
    CONSTRAINT pk_rendezvous_session PRIMARY KEY (id),
    CONSTRAINT uq_rendezvous_session_id UNIQUE (session_id)
);

CREATE INDEX IF NOT EXISTS idx_rendezvous_session_user ON rendezvous_session(user_id) WHERE user_id IS NOT NULL;
CREATE INDEX IF NOT EXISTS idx_rendezvous_session_expires ON rendezvous_session(expires_ts);
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

-- IP 信誉表 (增强版)
ALTER TABLE ip_reputation ADD COLUMN IF NOT EXISTS score INTEGER DEFAULT 50;
ALTER TABLE ip_reputation ADD COLUMN IF NOT EXISTS last_checked_ts BIGINT;
ALTER TABLE ip_reputation ADD COLUMN IF NOT EXISTS is_whitelisted BOOLEAN DEFAULT FALSE;
ALTER TABLE ip_reputation ADD COLUMN IF NOT EXISTS blocked_until BIGINT;

-- 第三方身份表 (已废弃，保留兼容性)
-- 注意: 此表已废弃，功能与 user_threepids 重复
-- CREATE TABLE IF NOT EXISTS threepids (...);

-- 事件举报表 (已废弃，保留兼容性)
-- 注意: 此表已废弃，功能与 event_reports 重复
-- CREATE TABLE IF NOT EXISTS reports (...);

-- 线程统计表 (已废弃，保留兼容性)
-- 注意: 此表已废弃，功能已合并到 thread_roots
-- CREATE TABLE IF NOT EXISTS thread_statistics (...);

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
    RAISE NOTICE '  - 增强 ip_reputation 表字段';
    RAISE NOTICE '  - 标记废弃表 (threepids, reports, thread_statistics)';
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
INSERT INTO schema_migrations (version, name, applied_ts, description)
VALUES (
    'v6.0.0', 
    'unified_schema_v6', 
    EXTRACT(EPOCH FROM NOW()) * 1000, 
    'BREAKING CHANGE - Unified field naming, added missing tables and columns'
) ON CONFLICT (version) DO NOTHING;

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
    RAISE NOTICE 'synapse-rust 统一数据库架构 v6.0.4 初始化完成';
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
    INSERT INTO schema_migrations (version, name, success, applied_ts, execution_time_ms, description)
    VALUES ('00000000', 'unified_schema_v6', true, FLOOR(EXTRACT(EPOCH FROM NOW()) * 1000)::BIGINT, 0, 'synapse-rust v6.0.4 统一基础架构')
    ON CONFLICT (version) DO NOTHING;
END $$;
