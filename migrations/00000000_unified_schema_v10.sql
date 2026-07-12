-- ============================================================================
-- synapse-rust 统一数据库架构 v10.0.0
-- 创建日期: 2026-06-07
-- 最后更新: 2026-06-07
--
-- 版本历史:
--   v7.0.0 (2026-05-15): 初始 v7 基线
--   v8.0.0 (2026-06-04): 统一合并所有迁移变更，消除重复表定义
--   v9.0.0 (2026-06-07): 折入 post-v8 delta 修复
--   v10.0.0 (2026-06-07): 折入 TIMESTAMPTZ→BIGINT 统一修复
--
-- 主要变更 (v10):
--   - device_verification_request.expires_at/completed_at 改 BIGINT 毫秒
--   - key_rotation_log.rotated_at 改 BIGINT 毫秒
--   - key_rotation_state.rotated_at 改 BIGINT 毫秒
--   - megolm_key_shares.shared_at 改 BIGINT 毫秒
--   - schema_migrations.executed_at 改 BIGINT 毫秒
--
-- 主要变更 (v9):
--   - user_threepids.validated_ts -> validated_at (m-30)
--   - email_verification_tokens.expires_at/created_ts 改 BIGINT 毫秒 (m-28)
--   - burn_after_read_pending.delete_at -> delete_ts (m-29)
--   - push_device.last_used_at 改 BIGINT 毫秒 (m-27)
--   - idx_burn_pending_delete_at -> idx_burn_pending_delete_ts (跟随 m-29)
--
-- 主要变更 (v8):
--   - 移除 19 个已 DROP 的冗余表
--   - 移除 Folded Delta 中的重复表定义
--   - 所有 ALTER TABLE 变更内联到表定义
--   - voice_usage_stats 使用 20260517 版本
--   - user_privacy_settings 合并 visibility 列
--   - spam_check_results / third_party_rule_results 移除冗余列 (m-26)
--   - CAS 表使用 _at 后缀
--   - 新增 burn_after_read, key_rotation, megolm_session_keys 等表
--   - 整合所有索引、视图、外键、触发器、默认数据
--
-- 规范:
--   - NOT NULL 时间戳使用 _ts 后缀 (BIGINT 毫秒)
--   - 可空时间戳使用 _at 后缀 (BIGINT 毫秒)
--   - 布尔字段使用 is_ 前缀
--   - 所有 CREATE TABLE 使用 IF NOT EXISTS
--   - 所有 CREATE INDEX 使用 IF NOT EXISTS
-- ============================================================================
--no-transaction

SET TIME ZONE 'UTC';

-- ============================================================================
-- Extensions
-- ============================================================================

CREATE EXTENSION IF NOT EXISTS pgcrypto;
CREATE EXTENSION IF NOT EXISTS pg_trgm;

-- ============================================================================
-- Helper Functions
-- ============================================================================

CREATE OR REPLACE FUNCTION update_updated_ts_column()
RETURNS TRIGGER AS $$
BEGIN
    NEW.updated_ts = EXTRACT(EPOCH FROM NOW())::BIGINT * 1000;
    RETURN NEW;
END;
$$ LANGUAGE plpgsql;

-- ============================================================================
-- Sequences
-- ============================================================================

CREATE SEQUENCE IF NOT EXISTS sliding_sync_pos_seq;
CREATE SEQUENCE IF NOT EXISTS to_device_stream_id_seq;
CREATE SEQUENCE IF NOT EXISTS events_stream_ordering_seq;

-- ============================================================================
-- Part 1: Core User Tables
-- ============================================================================

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

CREATE TABLE IF NOT EXISTS user_threepids (
    id BIGSERIAL,
    user_id TEXT NOT NULL,
    medium TEXT NOT NULL,
    address TEXT NOT NULL,
    validated_at BIGINT,
    added_ts BIGINT NOT NULL,
    is_verified BOOLEAN DEFAULT FALSE,
    verification_token TEXT,
    verification_expires_at BIGINT,
    CONSTRAINT pk_user_threepids PRIMARY KEY (id),
    CONSTRAINT uq_user_threepids_medium_address UNIQUE (medium, address),
    CONSTRAINT fk_user_threepids_user FOREIGN KEY (user_id) REFERENCES users(user_id) ON DELETE CASCADE
);

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
    CONSTRAINT fk_access_tokens_device FOREIGN KEY (device_id) REFERENCES devices(device_id) ON DELETE SET NULL NOT VALID
);

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
    CONSTRAINT fk_refresh_tokens_device FOREIGN KEY (device_id) REFERENCES devices(device_id) ON DELETE SET NULL NOT VALID
);

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
    CONSTRAINT fk_token_blacklist_user FOREIGN KEY (user_id) REFERENCES users(user_id) ON DELETE SET NULL NOT VALID
);

CREATE TABLE IF NOT EXISTS user_privacy_settings (
    user_id VARCHAR(255) NOT NULL PRIMARY KEY,
    allow_presence_lookup BOOLEAN DEFAULT TRUE,
    allow_profile_lookup BOOLEAN DEFAULT TRUE,
    allow_room_invites BOOLEAN DEFAULT TRUE,
    profile_visibility TEXT NOT NULL DEFAULT 'public',
    avatar_visibility TEXT NOT NULL DEFAULT 'public',
    displayname_visibility TEXT NOT NULL DEFAULT 'public',
    presence_visibility TEXT NOT NULL DEFAULT 'contacts',
    room_membership_visibility TEXT NOT NULL DEFAULT 'contacts',
    id BIGSERIAL,
    created_ts BIGINT NOT NULL,
    updated_ts BIGINT
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

-- ============================================================================
-- Part 2: Room Tables
-- ============================================================================

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
    redacts TEXT,
    stream_ordering BIGINT DEFAULT nextval('events_stream_ordering_seq'),
    CONSTRAINT pk_events PRIMARY KEY (event_id),
    CONSTRAINT fk_events_room FOREIGN KEY (room_id) REFERENCES rooms(room_id) ON DELETE CASCADE
);
ALTER SEQUENCE events_stream_ordering_seq OWNED BY events.stream_ordering;

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

CREATE TABLE IF NOT EXISTS room_aliases (
    room_alias TEXT NOT NULL,
    room_id TEXT NOT NULL,
    server_name TEXT NOT NULL DEFAULT '',
    created_ts BIGINT NOT NULL,
    CONSTRAINT pk_room_aliases PRIMARY KEY (room_alias),
    CONSTRAINT fk_room_aliases_room FOREIGN KEY (room_id) REFERENCES rooms(room_id) ON DELETE CASCADE
);

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

CREATE TABLE IF NOT EXISTS room_invites (
    id BIGSERIAL,
    room_id TEXT NOT NULL,
    inviter TEXT NOT NULL,
    invitee TEXT NOT NULL,
    is_accepted BOOLEAN DEFAULT FALSE,
    accepted_at BIGINT,
    created_ts BIGINT NOT NULL,
    expires_at BIGINT,
    invite_code TEXT,
    inviter_user_id TEXT,
    invitee_email TEXT,
    invitee_user_id TEXT,
    is_used BOOLEAN NOT NULL DEFAULT FALSE,
    is_revoked BOOLEAN NOT NULL DEFAULT FALSE,
    used_ts BIGINT,
    revoked_at BIGINT,
    revoked_reason TEXT,
    signature TEXT,
    signed_version SMALLINT NOT NULL DEFAULT 0,
    CONSTRAINT pk_room_invites PRIMARY KEY (id)
);

CREATE TABLE IF NOT EXISTS room_invite_blocklist (
    id BIGSERIAL,
    room_id TEXT NOT NULL,
    user_id TEXT NOT NULL,
    created_ts BIGINT NOT NULL,
    CONSTRAINT pk_room_invite_blocklist PRIMARY KEY (id),
    CONSTRAINT uq_room_invite_blocklist_room_user UNIQUE (room_id, user_id),
    CONSTRAINT fk_room_invite_blocklist_room FOREIGN KEY (room_id) REFERENCES rooms(room_id) ON DELETE CASCADE
);

CREATE TABLE IF NOT EXISTS room_invite_allowlist (
    id BIGSERIAL,
    room_id TEXT NOT NULL,
    user_id TEXT NOT NULL,
    created_ts BIGINT NOT NULL,
    CONSTRAINT pk_room_invite_allowlist PRIMARY KEY (id),
    CONSTRAINT uq_room_invite_allowlist_room_user UNIQUE (room_id, user_id),
    CONSTRAINT fk_room_invite_allowlist_room FOREIGN KEY (room_id) REFERENCES rooms(room_id) ON DELETE CASCADE
);

CREATE TABLE IF NOT EXISTS room_tags (
    id SERIAL PRIMARY KEY,
    user_id VARCHAR(255) NOT NULL,
    room_id VARCHAR(255) NOT NULL,
    tag VARCHAR(255) NOT NULL,
    order_value DOUBLE PRECISION,
    created_ts BIGINT NOT NULL,
    CONSTRAINT uq_room_tags_user_room_tag UNIQUE (user_id, room_id, tag)
);

CREATE TABLE IF NOT EXISTS room_sticky_events (
    id BIGSERIAL PRIMARY KEY,
    room_id TEXT NOT NULL,
    user_id TEXT NOT NULL,
    event_id TEXT NOT NULL,
    event_type TEXT NOT NULL,
    is_sticky BOOLEAN NOT NULL DEFAULT TRUE,
    created_ts BIGINT NOT NULL,
    updated_ts BIGINT NOT NULL,
    CONSTRAINT uq_room_sticky_events_room_user_type UNIQUE (room_id, user_id, event_type),
    CONSTRAINT fk_room_sticky_events_room FOREIGN KEY (room_id) REFERENCES rooms(room_id) ON DELETE CASCADE,
    CONSTRAINT fk_room_sticky_events_user FOREIGN KEY (user_id) REFERENCES users(user_id) ON DELETE CASCADE
);

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

CREATE TABLE IF NOT EXISTS room_retention_policies (
    id BIGSERIAL PRIMARY KEY,
    room_id TEXT NOT NULL,
    max_lifetime BIGINT,
    min_lifetime BIGINT NOT NULL DEFAULT 0,
    is_expire_on_clients BOOLEAN NOT NULL DEFAULT FALSE,
    is_server_default BOOLEAN NOT NULL DEFAULT FALSE,
    created_ts BIGINT NOT NULL,
    updated_ts BIGINT NOT NULL,
    CONSTRAINT uq_room_retention_policies_room UNIQUE (room_id),
    CONSTRAINT fk_room_retention_policies_room FOREIGN KEY (room_id) REFERENCES rooms(room_id) ON DELETE CASCADE
);

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

CREATE TABLE IF NOT EXISTS blocked_rooms (
    id BIGSERIAL PRIMARY KEY,
    room_id TEXT NOT NULL UNIQUE,
    blocked_at BIGINT NOT NULL,
    blocked_by TEXT NOT NULL,
    reason TEXT
);

-- ============================================================================
-- Part 3: E2EE Tables
-- ============================================================================

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
    is_fallback BOOLEAN NOT NULL DEFAULT FALSE,
    display_name TEXT,
    CONSTRAINT pk_device_keys PRIMARY KEY (id),
    CONSTRAINT uq_device_keys_user_device_key UNIQUE (user_id, device_id, key_id)
);

CREATE TABLE IF NOT EXISTS cross_signing_keys (
    id BIGSERIAL,
    user_id TEXT NOT NULL,
    key_type TEXT NOT NULL,
    key_data TEXT NOT NULL,
    signatures JSONB,
    binding_token TEXT,
    binding_ts BIGINT,
    added_ts BIGINT NOT NULL,
    CONSTRAINT pk_cross_signing_keys PRIMARY KEY (id),
    CONSTRAINT uq_cross_signing_keys_user_type UNIQUE (user_id, key_type)
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

CREATE TABLE IF NOT EXISTS key_rotation_log (
    id BIGSERIAL PRIMARY KEY,
    user_id TEXT NOT NULL,
    device_id TEXT NOT NULL,
    room_id TEXT,
    rotation_type TEXT NOT NULL,
    old_key_id TEXT,
    new_key_id TEXT,
    reason TEXT,
    rotated_at BIGINT NOT NULL
);

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
    -- Phase 2 (E2EE vodozemac 双写): 标识 session_key 列 pickle 格式
    -- 'legacy' = 自研 AES-256-GCM pickle (历史数据)
    -- 'vodozemac' = vodozemac 0.9 pickle (新增)
    -- 'dual' = 同时持有两种 pickle (vodozemac_pickle 列非空 + session_key 仍为 legacy)
    pickle_format TEXT NOT NULL DEFAULT 'legacy',
    vodozemac_pickle TEXT,
    CONSTRAINT pk_megolm_sessions PRIMARY KEY (id),
    CONSTRAINT uq_megolm_sessions_session UNIQUE (session_id),
    CONSTRAINT chk_megolm_sessions_pickle_format CHECK (
        pickle_format IN ('legacy', 'vodozemac', 'dual')
    )
);

CREATE TABLE IF NOT EXISTS event_signatures (
    id UUID DEFAULT gen_random_uuid(),
    event_id TEXT NOT NULL,
    user_id TEXT NOT NULL,
    device_id TEXT NOT NULL,
    signature TEXT NOT NULL,
    key_id TEXT NOT NULL,
    algorithm TEXT NOT NULL DEFAULT 'ed25519',
    created_ts BIGINT NOT NULL,
    CONSTRAINT pk_event_signatures PRIMARY KEY (id),
    CONSTRAINT uq_event_signatures_event_user_device_key UNIQUE (event_id, user_id, device_id, key_id)
);

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

CREATE TABLE IF NOT EXISTS backup_keys (
    id BIGSERIAL,
    backup_id BIGINT NOT NULL,
    room_id TEXT NOT NULL,
    session_id TEXT NOT NULL,
    session_data JSONB NOT NULL,
    first_message_index BIGINT NOT NULL DEFAULT 0,
    forwarded_count BIGINT NOT NULL DEFAULT 0,
    is_verified BOOLEAN NOT NULL DEFAULT FALSE,
    created_ts BIGINT NOT NULL,
    CONSTRAINT pk_backup_keys PRIMARY KEY (id),
    CONSTRAINT fk_backup_keys_backup FOREIGN KEY (backup_id) REFERENCES key_backups(backup_id) ON DELETE CASCADE
);

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
    updated_ts BIGINT,
    CONSTRAINT uq_e2ee_key_requests_request UNIQUE (request_id)
);

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
    expires_at BIGINT NOT NULL,
    completed_at BIGINT,
    CONSTRAINT pk_device_verification_request PRIMARY KEY (id),
    CONSTRAINT uq_device_verification_request_token UNIQUE (request_token),
    CONSTRAINT fk_device_verification_request_user FOREIGN KEY (user_id) REFERENCES users(user_id) ON DELETE CASCADE
);

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

CREATE TABLE IF NOT EXISTS e2ee_audit_log (
    id BIGSERIAL PRIMARY KEY,
    user_id TEXT NOT NULL,
    device_id TEXT NOT NULL,
    action TEXT NOT NULL,
    event_id TEXT,
    room_id TEXT,
    details JSONB NOT NULL DEFAULT '{}',
    operation TEXT,
    key_id TEXT,
    ip_address TEXT,
    created_ts BIGINT NOT NULL
);

CREATE TABLE IF NOT EXISTS e2ee_secret_storage_keys (
    id BIGSERIAL PRIMARY KEY,
    user_id TEXT NOT NULL,
    key_name TEXT NOT NULL,
    key_id TEXT NOT NULL UNIQUE,
    algorithm TEXT NOT NULL,
    key_data BYTEA NOT NULL,
    encrypted_key TEXT,
    public_key TEXT,
    signatures JSONB,
    created_ts BIGINT NOT NULL,
    updated_ts BIGINT NOT NULL,
    is_active BOOLEAN NOT NULL DEFAULT TRUE
);

CREATE TABLE IF NOT EXISTS e2ee_stored_secrets (
    id BIGSERIAL PRIMARY KEY,
    user_id TEXT NOT NULL,
    secret_name TEXT NOT NULL,
    secret_data BYTEA NOT NULL,
    key_key_id TEXT NOT NULL,
    encrypted_secret TEXT,
    key_id TEXT,
    created_ts BIGINT NOT NULL,
    updated_ts BIGINT NOT NULL
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

CREATE TABLE IF NOT EXISTS leak_alerts (
    id BIGSERIAL PRIMARY KEY,
    user_id TEXT NOT NULL,
    key_id TEXT NOT NULL,
    alert_type TEXT NOT NULL,
    severity TEXT NOT NULL,
    details JSONB NOT NULL DEFAULT '{}',
    created_ts BIGINT NOT NULL,
    is_acknowledged BOOLEAN NOT NULL DEFAULT FALSE,
    acknowledged_by TEXT,
    acknowledged_at BIGINT
);

-- ============================================================================
-- Part 4: Media Tables
-- ============================================================================

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

CREATE TABLE IF NOT EXISTS user_media_quota (
    id BIGSERIAL,
    user_id TEXT NOT NULL,
    max_bytes BIGINT DEFAULT 1073741824,
    used_bytes BIGINT DEFAULT 0,
    file_count INTEGER DEFAULT 0,
    quota_config_id BIGINT,
    custom_max_storage_bytes BIGINT,
    custom_max_file_size_bytes BIGINT,
    custom_max_files_count INTEGER,
    current_storage_bytes BIGINT NOT NULL DEFAULT 0,
    current_files_count INTEGER NOT NULL DEFAULT 0,
    created_ts BIGINT NOT NULL,
    updated_ts BIGINT,
    CONSTRAINT pk_user_media_quota PRIMARY KEY (id),
    CONSTRAINT uq_user_media_quota_user UNIQUE (user_id),
    CONSTRAINT fk_user_media_quota_user FOREIGN KEY (user_id) REFERENCES users(user_id) ON DELETE CASCADE
);

CREATE TABLE IF NOT EXISTS media_quota_config (
    id BIGSERIAL,
    config_name TEXT NOT NULL DEFAULT '',
    max_file_size BIGINT DEFAULT 10485760,
    max_upload_rate BIGINT,
    allowed_content_types TEXT[] DEFAULT ARRAY['image/jpeg', 'image/png', 'image/gif', 'video/mp4', 'audio/ogg'],
    retention_days INTEGER DEFAULT 90,
    is_enabled BOOLEAN DEFAULT TRUE,
    created_ts BIGINT NOT NULL DEFAULT (EXTRACT(EPOCH FROM NOW()) * 1000)::BIGINT,
    updated_ts BIGINT,
    name TEXT NOT NULL DEFAULT 'default',
    description TEXT,
    max_storage_bytes BIGINT NOT NULL DEFAULT 10737418240,
    max_file_size_bytes BIGINT NOT NULL DEFAULT 10485760,
    max_files_count INTEGER NOT NULL DEFAULT 10000,
    allowed_mime_types JSONB NOT NULL DEFAULT '[]'::jsonb,
    blocked_mime_types JSONB NOT NULL DEFAULT '[]'::jsonb,
    is_default BOOLEAN NOT NULL DEFAULT FALSE,
    CONSTRAINT pk_media_quota_config PRIMARY KEY (id),
    CONSTRAINT uq_media_quota_config_name UNIQUE (config_name)
);

CREATE TABLE IF NOT EXISTS media_usage_log (
    id BIGSERIAL PRIMARY KEY,
    user_id TEXT NOT NULL,
    media_id TEXT NOT NULL,
    file_size_bytes BIGINT NOT NULL,
    mime_type TEXT,
    operation TEXT NOT NULL,
    timestamp BIGINT NOT NULL
);

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

CREATE TABLE IF NOT EXISTS voice_usage_stats (
    id BIGSERIAL PRIMARY KEY,
    user_id TEXT NOT NULL,
    room_id TEXT,
    media_id TEXT NOT NULL,
    content_type TEXT NOT NULL,
    duration_ms INTEGER NOT NULL DEFAULT 0,
    size_bytes BIGINT NOT NULL DEFAULT 0,
    created_ts BIGINT NOT NULL
);

-- ============================================================================
-- Part 5: Auth Tables (CAS/SAML/OIDC)
-- ============================================================================

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

CREATE TABLE IF NOT EXISTS cas_services (
    id BIGSERIAL,
    service_id TEXT NOT NULL,
    name TEXT NOT NULL,
    description TEXT,
    service_url_pattern TEXT NOT NULL,
    allowed_attributes JSONB DEFAULT '[]',
    allowed_proxy_callbacks JSONB DEFAULT '[]',
    is_enabled BOOLEAN DEFAULT TRUE,
    is_require_secure BOOLEAN DEFAULT TRUE,
    is_single_logout BOOLEAN DEFAULT FALSE,
    created_ts BIGINT NOT NULL,
    updated_ts BIGINT,
    CONSTRAINT pk_cas_services PRIMARY KEY (id),
    CONSTRAINT uq_cas_services_service UNIQUE (service_id)
);

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

CREATE TABLE IF NOT EXISTS saml_config_overrides (
    config_key TEXT NOT NULL,
    config_value JSONB NOT NULL,
    updated_ts BIGINT NOT NULL,
    CONSTRAINT pk_saml_config_overrides PRIMARY KEY (config_key)
);

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

CREATE TABLE IF NOT EXISTS email_verification_tokens (
    id BIGSERIAL PRIMARY KEY,
    user_id TEXT,
    email TEXT NOT NULL,
    token TEXT NOT NULL UNIQUE,
    expires_at BIGINT NOT NULL,
    created_ts BIGINT NOT NULL,
    is_used BOOLEAN NOT NULL DEFAULT FALSE,
    session_data JSONB
);

-- ============================================================================
-- Part 6: Captcha Tables
-- ============================================================================

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
-- Part 7: Push Notification Tables
-- ============================================================================

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

CREATE TABLE IF NOT EXISTS push_rules (
    id BIGSERIAL,
    user_id TEXT NOT NULL,
    scope TEXT NOT NULL,
    rule_id TEXT NOT NULL,
    kind TEXT NOT NULL,
    priority_class INTEGER NOT NULL DEFAULT 0,
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
    last_used_at BIGINT,
    last_error TEXT,
    error_count INTEGER NOT NULL DEFAULT 0,
    metadata JSONB NOT NULL DEFAULT '{}',
    CONSTRAINT uq_push_device_user_device UNIQUE (user_id, device_id),
    CONSTRAINT fk_push_device_user FOREIGN KEY (user_id) REFERENCES users(user_id) ON DELETE CASCADE
);

CREATE TABLE IF NOT EXISTS push_notification_queue (
    id BIGSERIAL,
    user_id TEXT NOT NULL,
    device_id TEXT NOT NULL,
    event_id TEXT,
    room_id TEXT,
    notification_type TEXT,
    content JSONB DEFAULT '{}',
    is_processed BOOLEAN DEFAULT FALSE,
    processed_at BIGINT,
    created_ts BIGINT NOT NULL,
    priority INTEGER NOT NULL DEFAULT 0,
    status TEXT NOT NULL DEFAULT 'pending',
    attempts INTEGER NOT NULL DEFAULT 0,
    max_attempts INTEGER NOT NULL DEFAULT 3,
    next_attempt_at BIGINT,
    sent_at BIGINT,
    error_message TEXT,
    CONSTRAINT pk_push_notification_queue PRIMARY KEY (id)
);

CREATE TABLE IF NOT EXISTS push_notification_log (
    id BIGSERIAL,
    user_id TEXT NOT NULL,
    device_id TEXT NOT NULL,
    pushkey TEXT,
    status TEXT,
    error_message TEXT,
    retry_count INTEGER DEFAULT 0,
    last_attempt_at BIGINT,
    created_ts BIGINT NOT NULL,
    event_id TEXT,
    room_id TEXT,
    notification_type TEXT,
    push_type TEXT,
    sent_at BIGINT,
    is_success BOOLEAN,
    provider_response TEXT,
    response_time_ms INTEGER,
    metadata JSONB NOT NULL DEFAULT '{}',
    CONSTRAINT pk_push_notification_log PRIMARY KEY (id)
);

CREATE TABLE IF NOT EXISTS push_config (
    id BIGSERIAL,
    user_id TEXT NOT NULL,
    device_id TEXT NOT NULL,
    config_type TEXT NOT NULL,
    config_data JSONB DEFAULT '{}',
    config_key TEXT,
    config_value TEXT,
    created_ts BIGINT NOT NULL,
    updated_ts BIGINT,
    CONSTRAINT pk_push_config PRIMARY KEY (id),
    CONSTRAINT uq_push_config_user_device_type UNIQUE (user_id, device_id, config_type)
);

CREATE TABLE IF NOT EXISTS notifications (
    id BIGSERIAL,
    user_id TEXT NOT NULL,
    event_id TEXT,
    room_id TEXT,
    ts BIGINT NOT NULL,
    notification_type VARCHAR(50) DEFAULT 'message',
    profile_tag VARCHAR(255),
    is_read BOOLEAN DEFAULT FALSE,
    created_ts BIGINT NOT NULL,
    updated_ts BIGINT,
    CONSTRAINT pk_notifications PRIMARY KEY (id)
);

-- ============================================================================
-- Part 8: Space Tables
-- ============================================================================

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

CREATE TABLE IF NOT EXISTS space_summaries (
    id BIGSERIAL PRIMARY KEY,
    space_id TEXT NOT NULL UNIQUE,
    summary JSONB DEFAULT '{}',
    children_count BIGINT DEFAULT 0,
    member_count BIGINT DEFAULT 0,
    updated_ts BIGINT NOT NULL,
    CONSTRAINT fk_space_summary_space FOREIGN KEY (space_id) REFERENCES spaces(space_id) ON DELETE CASCADE
);

CREATE TABLE IF NOT EXISTS space_statistics (
    space_id TEXT PRIMARY KEY,
    name TEXT,
    is_public BOOLEAN NOT NULL DEFAULT FALSE,
    child_room_count BIGINT DEFAULT 0,
    member_count BIGINT DEFAULT 0,
    created_ts BIGINT NOT NULL,
    updated_ts BIGINT
);

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

-- ============================================================================
-- Part 9: Federation Tables
-- ============================================================================

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

CREATE TABLE IF NOT EXISTS federation_blacklist (
    id BIGSERIAL,
    server_name TEXT NOT NULL,
    reason TEXT,
    added_ts BIGINT NOT NULL,
    added_by TEXT,
    updated_ts BIGINT,
    block_type TEXT NOT NULL DEFAULT 'manual',
    blocked_by TEXT,
    created_ts BIGINT,
    expires_at BIGINT,
    is_enabled BOOLEAN NOT NULL DEFAULT TRUE,
    metadata JSONB NOT NULL DEFAULT '{}',
    CONSTRAINT pk_federation_blacklist PRIMARY KEY (id),
    CONSTRAINT uq_federation_blacklist_name UNIQUE (server_name)
);

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

CREATE TABLE IF NOT EXISTS federation_inbound_events (
    event_id TEXT NOT NULL,
    origin TEXT NOT NULL,
    origin_server_ts BIGINT NOT NULL,
    received_ts BIGINT NOT NULL,
    CONSTRAINT pk_federation_inbound_events PRIMARY KEY (event_id)
);

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

CREATE TABLE IF NOT EXISTS federation_cache (
    id BIGSERIAL PRIMARY KEY,
    key TEXT NOT NULL UNIQUE,
    value TEXT,
    expiry_ts BIGINT,
    created_ts BIGINT NOT NULL DEFAULT 0
);

CREATE TABLE IF NOT EXISTS event_edges (
    event_id TEXT NOT NULL,
    prev_event_id TEXT NOT NULL,
    is_state BOOLEAN NOT NULL DEFAULT FALSE,
    CONSTRAINT pk_event_edges PRIMARY KEY (event_id, prev_event_id),
    CONSTRAINT fk_event_edges_event FOREIGN KEY (event_id) REFERENCES events(event_id) ON DELETE CASCADE
);

CREATE TABLE IF NOT EXISTS event_forward_extremities (
    room_id TEXT NOT NULL,
    event_id TEXT NOT NULL,
    CONSTRAINT pk_event_forward_extremities PRIMARY KEY (room_id, event_id),
    CONSTRAINT fk_event_forward_extremities_event FOREIGN KEY (event_id) REFERENCES events(event_id) ON DELETE CASCADE,
    CONSTRAINT fk_event_forward_extremities_room FOREIGN KEY (room_id) REFERENCES rooms(room_id) ON DELETE CASCADE
);

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

CREATE TABLE IF NOT EXISTS device_lists_outbound_pokes (
    destination TEXT NOT NULL,
    user_id TEXT NOT NULL,
    stream_id BIGINT NOT NULL,
    sent_ts BIGINT,
    created_ts BIGINT NOT NULL,
    CONSTRAINT pk_device_lists_outbound_pokes PRIMARY KEY (user_id, destination),
    CONSTRAINT fk_device_lists_outbound_pokes_user FOREIGN KEY (user_id) REFERENCES users(user_id) ON DELETE CASCADE
);

-- ============================================================================
-- Part 10: Account Data Tables
-- ============================================================================

CREATE TABLE IF NOT EXISTS filters (
    id BIGSERIAL,
    user_id TEXT NOT NULL,
    filter_id TEXT NOT NULL,
    content JSONB NOT NULL DEFAULT '{}',
    created_ts BIGINT NOT NULL,
    CONSTRAINT pk_filters PRIMARY KEY (id),
    CONSTRAINT uq_filters_user_filter UNIQUE (user_id, filter_id)
);

CREATE TABLE IF NOT EXISTS user_filters (
    id SERIAL PRIMARY KEY,
    user_id VARCHAR(255) NOT NULL,
    filter_id VARCHAR(255) NOT NULL,
    filter_json JSONB NOT NULL DEFAULT '{}',
    created_ts BIGINT NOT NULL,
    CONSTRAINT uq_user_filters_user_filter UNIQUE (user_id, filter_id)
);

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

CREATE TABLE IF NOT EXISTS user_account_data (
    id BIGSERIAL,
    user_id TEXT NOT NULL,
    event_type TEXT NOT NULL,
    content TEXT NOT NULL,
    created_ts BIGINT NOT NULL,
    CONSTRAINT pk_user_account_data PRIMARY KEY (id),
    CONSTRAINT uq_user_account_data_user_type UNIQUE (user_id, event_type)
);

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

-- ============================================================================
-- Part 11: Background Task Tables
-- ============================================================================

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
    next_retry_ts BIGINT,
    CONSTRAINT pk_worker_commands PRIMARY KEY (id),
    CONSTRAINT uq_worker_commands_id UNIQUE (command_id)
);

CREATE SEQUENCE IF NOT EXISTS worker_events_stream_id_seq;
CREATE TABLE IF NOT EXISTS worker_events (
    id BIGSERIAL,
    event_id TEXT NOT NULL,
    stream_id BIGINT NOT NULL DEFAULT nextval('worker_events_stream_id_seq'),
    event_type TEXT NOT NULL,
    room_id TEXT,
    sender TEXT,
    event_data JSONB DEFAULT '{}',
    created_ts BIGINT NOT NULL,
    processed_by JSONB DEFAULT '[]',
    CONSTRAINT pk_worker_events PRIMARY KEY (id),
    CONSTRAINT uq_worker_events_id UNIQUE (event_id)
);

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
    retry_count INTEGER NOT NULL DEFAULT 0,
    max_retries INTEGER NOT NULL DEFAULT 3,
    next_retry_ts BIGINT,
    CONSTRAINT fk_worker_task_assignments_worker FOREIGN KEY (assigned_worker_id) REFERENCES workers(worker_id) ON DELETE SET NULL
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

CREATE TABLE IF NOT EXISTS sync_stream_id (
    id BIGSERIAL,
    stream_type TEXT,
    last_id BIGINT DEFAULT 0,
    updated_ts BIGINT,
    CONSTRAINT pk_sync_stream_id PRIMARY KEY (id),
    CONSTRAINT uq_sync_stream_id_type UNIQUE (stream_type)
);

-- ============================================================================
-- Part 12: Module Tables
-- ============================================================================

CREATE TABLE IF NOT EXISTS modules (
    id BIGSERIAL,
    module_name TEXT NOT NULL,
    module_type TEXT NOT NULL,
    is_enabled BOOLEAN DEFAULT TRUE,
    config JSONB DEFAULT '{}',
    priority INTEGER DEFAULT 0,
    version TEXT NOT NULL DEFAULT '1.0.0',
    last_executed_ts BIGINT,
    execution_count INTEGER NOT NULL DEFAULT 0,
    error_count INTEGER NOT NULL DEFAULT 0,
    last_error TEXT,
    created_ts BIGINT NOT NULL,
    updated_ts BIGINT NOT NULL,
    description TEXT,
    CONSTRAINT pk_modules PRIMARY KEY (id),
    CONSTRAINT uq_modules_name UNIQUE (module_name)
);

CREATE TABLE IF NOT EXISTS module_execution_logs (
    id BIGSERIAL,
    module_id BIGINT,
    execution_type TEXT NOT NULL DEFAULT 'module_execution',
    input_data JSONB,
    output_data JSONB,
    is_success BOOLEAN NOT NULL DEFAULT TRUE,
    error_message TEXT,
    execution_time_ms BIGINT,
    module_name TEXT NOT NULL DEFAULT '',
    module_type TEXT NOT NULL DEFAULT '',
    event_id TEXT,
    room_id TEXT,
    metadata JSONB,
    executed_ts BIGINT NOT NULL,
    created_ts BIGINT NOT NULL DEFAULT (EXTRACT(EPOCH FROM NOW()) * 1000)::BIGINT,
    CONSTRAINT pk_module_execution_logs PRIMARY KEY (id),
    CONSTRAINT fk_module_execution_logs_module FOREIGN KEY (module_id) REFERENCES modules(id) ON DELETE CASCADE
);

CREATE TABLE IF NOT EXISTS spam_check_results (
    id BIGSERIAL PRIMARY KEY,
    event_id TEXT NOT NULL,
    room_id TEXT NOT NULL,
    sender TEXT NOT NULL,
    event_type TEXT NOT NULL,
    content JSONB,
    result TEXT NOT NULL,
    score INTEGER NOT NULL DEFAULT 0,
    reason TEXT,
    checker_module TEXT NOT NULL,
    checked_ts BIGINT NOT NULL,
    action_taken TEXT,
    created_ts BIGINT NOT NULL
);

CREATE TABLE IF NOT EXISTS third_party_rule_results (
    id BIGSERIAL PRIMARY KEY,
    event_id TEXT,
    room_id TEXT,
    sender TEXT NOT NULL,
    event_type TEXT NOT NULL,
    rule_name TEXT NOT NULL,
    is_allowed BOOLEAN DEFAULT TRUE,
    reason TEXT,
    modified_content JSONB,
    checked_ts BIGINT NOT NULL,
    created_ts BIGINT NOT NULL
);

CREATE TABLE IF NOT EXISTS media_callbacks (
    id BIGSERIAL,
    callback_name TEXT NOT NULL,
    callback_type TEXT NOT NULL,
    is_enabled BOOLEAN DEFAULT TRUE,
    url TEXT NOT NULL,
    headers JSONB DEFAULT '{}',
    method TEXT NOT NULL DEFAULT 'POST',
    timeout_ms INTEGER NOT NULL DEFAULT 5000,
    retry_count INTEGER NOT NULL DEFAULT 3,
    updated_ts BIGINT NOT NULL,
    media_id TEXT NOT NULL DEFAULT '',
    user_id TEXT NOT NULL DEFAULT '',
    status TEXT NOT NULL DEFAULT 'pending',
    result JSONB,
    completed_ts BIGINT,
    created_ts BIGINT NOT NULL,
    CONSTRAINT pk_media_callbacks PRIMARY KEY (id),
    CONSTRAINT uq_media_callbacks_name UNIQUE (callback_name)
);

-- ============================================================================
-- Part 13: Additional Tables (burn_after_read, key_rotation, etc.)
-- ============================================================================

CREATE TABLE IF NOT EXISTS burn_after_read_settings (
    user_id TEXT NOT NULL,
    room_id TEXT NOT NULL,
    is_enabled BOOLEAN NOT NULL DEFAULT FALSE,
    burn_after_ms BIGINT NOT NULL DEFAULT 60000,
    created_ts BIGINT NOT NULL,
    updated_ts BIGINT,
    PRIMARY KEY (user_id, room_id)
);

CREATE TABLE IF NOT EXISTS burn_after_read_pending (
    id BIGSERIAL PRIMARY KEY,
    user_id TEXT NOT NULL,
    room_id TEXT NOT NULL,
    event_id TEXT NOT NULL,
    created_ts BIGINT NOT NULL,
    delete_ts BIGINT NOT NULL,
    is_processed BOOLEAN NOT NULL DEFAULT FALSE,
    UNIQUE(user_id, room_id, event_id)
);

CREATE TABLE IF NOT EXISTS burn_after_read_log (
    id BIGSERIAL PRIMARY KEY,
    user_id TEXT NOT NULL,
    room_id TEXT NOT NULL,
    event_id TEXT NOT NULL,
    burned_ts BIGINT NOT NULL
);

CREATE TABLE IF NOT EXISTS burn_after_read_user_defaults (
    user_id TEXT NOT NULL PRIMARY KEY,
    default_burn_ms BIGINT NOT NULL DEFAULT 60000,
    created_ts BIGINT NOT NULL,
    updated_ts BIGINT
);

CREATE TABLE IF NOT EXISTS key_rotation_pending (
    room_id TEXT NOT NULL,
    reason TEXT NOT NULL,
    triggered_by_user_id TEXT NOT NULL,
    created_ts BIGINT NOT NULL,
    PRIMARY KEY (room_id, triggered_by_user_id)
);

CREATE TABLE IF NOT EXISTS key_rotation_state (
    user_id TEXT NOT NULL,
    room_id TEXT NOT NULL,
    is_rotated BOOLEAN NOT NULL DEFAULT FALSE,
    rotated_at BIGINT,
    PRIMARY KEY (user_id, room_id)
);

CREATE TABLE IF NOT EXISTS megolm_key_shares (
    room_id TEXT NOT NULL,
    session_id TEXT NOT NULL,
    share_reason TEXT NOT NULL,
    shared_at BIGINT NOT NULL DEFAULT (EXTRACT(EPOCH FROM NOW())::BIGINT * 1000),
    PRIMARY KEY (room_id, session_id)
);

CREATE TABLE IF NOT EXISTS key_rotation_config (
    key TEXT PRIMARY KEY,
    value TEXT NOT NULL
);

CREATE TABLE IF NOT EXISTS megolm_session_keys (
    id UUID DEFAULT gen_random_uuid(),
    user_id TEXT NOT NULL,
    session_id TEXT NOT NULL,
    encrypted_key TEXT NOT NULL,
    created_ts BIGINT NOT NULL,
    expires_at BIGINT,
    CONSTRAINT pk_megolm_session_keys PRIMARY KEY (id),
    CONSTRAINT uq_megolm_session_keys UNIQUE (user_id, session_id)
);

-- ============================================================================
-- Part 14: Security Tables
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

CREATE TABLE IF NOT EXISTS rate_limits (
    user_id TEXT PRIMARY KEY,
    messages_per_second DOUBLE PRECISION,
    burst_count INTEGER,
    updated_ts BIGINT NOT NULL DEFAULT (EXTRACT(EPOCH FROM clock_timestamp()) * 1000)::BIGINT,
    CONSTRAINT fk_rate_limits_user FOREIGN KEY (user_id) REFERENCES users(user_id) ON DELETE CASCADE
);

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
    CONSTRAINT fk_report_rate_limits_user FOREIGN KEY (user_id) REFERENCES users(user_id) ON DELETE CASCADE NOT VALID
);

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

-- ============================================================================
-- Part 15: State Group Tables
-- ============================================================================

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

CREATE TABLE IF NOT EXISTS state_group_edges (
    state_group_id BIGINT NOT NULL,
    prev_state_group_id BIGINT NOT NULL,
    CONSTRAINT pk_state_group_edges PRIMARY KEY (state_group_id, prev_state_group_id),
    CONSTRAINT fk_state_group_edges_from FOREIGN KEY (state_group_id) REFERENCES state_groups(id) ON DELETE CASCADE,
    CONSTRAINT fk_state_group_edges_to FOREIGN KEY (prev_state_group_id) REFERENCES state_groups(id) ON DELETE CASCADE
);

CREATE TABLE IF NOT EXISTS event_to_state_groups (
    event_id TEXT NOT NULL,
    state_group_id BIGINT NOT NULL,
    CONSTRAINT pk_event_to_state_groups PRIMARY KEY (event_id),
    CONSTRAINT fk_event_to_state_groups_event FOREIGN KEY (event_id) REFERENCES events(event_id) ON DELETE CASCADE,
    CONSTRAINT fk_event_to_state_groups_sg FOREIGN KEY (state_group_id) REFERENCES state_groups(id) ON DELETE CASCADE
);

CREATE TABLE IF NOT EXISTS state_group_state (
    state_group_id BIGINT NOT NULL,
    event_type TEXT NOT NULL,
    state_key TEXT NOT NULL,
    event_id TEXT NOT NULL,
    CONSTRAINT pk_state_group_state PRIMARY KEY (state_group_id, event_type, state_key),
    CONSTRAINT fk_state_group_state_sg FOREIGN KEY (state_group_id) REFERENCES state_groups(id) ON DELETE CASCADE,
    CONSTRAINT fk_state_group_state_event FOREIGN KEY (event_id) REFERENCES events(event_id) ON DELETE CASCADE
);

-- ============================================================================
-- Part 16: Token Tables
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
    CONSTRAINT fk_refresh_token_usage_token FOREIGN KEY (refresh_token_id) REFERENCES refresh_tokens(id) ON DELETE CASCADE NOT VALID,
    CONSTRAINT fk_refresh_token_usage_user FOREIGN KEY (user_id) REFERENCES users(user_id) ON DELETE CASCADE NOT VALID
);

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
    CONSTRAINT fk_refresh_token_families_user FOREIGN KEY (user_id) REFERENCES users(user_id) ON DELETE CASCADE NOT VALID,
    CONSTRAINT fk_refresh_token_families_device FOREIGN KEY (device_id) REFERENCES devices(device_id) ON DELETE SET NULL NOT VALID
);

CREATE TABLE IF NOT EXISTS refresh_token_rotations (
    id BIGSERIAL,
    family_id TEXT NOT NULL,
    old_token_hash TEXT,
    new_token_hash TEXT NOT NULL,
    rotated_ts BIGINT NOT NULL,
    rotation_reason TEXT,
    CONSTRAINT pk_refresh_token_rotations PRIMARY KEY (id),
    CONSTRAINT fk_refresh_token_rotations_family FOREIGN KEY (family_id) REFERENCES refresh_token_families(family_id) ON DELETE CASCADE NOT VALID
);

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
    created_by TEXT,
    allowed_email_domains TEXT[],
    allowed_user_ids TEXT[],
    auto_join_rooms TEXT[],
    display_name TEXT,
    email TEXT,
    CONSTRAINT pk_registration_tokens PRIMARY KEY (id),
    CONSTRAINT uq_registration_tokens_token UNIQUE (token)
);

CREATE TABLE IF NOT EXISTS registration_token_usage (
    id BIGSERIAL,
    token_id BIGINT,
    user_id TEXT NOT NULL,
    used_ts BIGINT NOT NULL,
    token TEXT,
    username TEXT,
    email TEXT,
    ip_address TEXT,
    user_agent TEXT,
    is_success BOOLEAN NOT NULL DEFAULT TRUE,
    error_message TEXT,
    CONSTRAINT pk_registration_token_usage PRIMARY KEY (id),
    CONSTRAINT fk_registration_token_usage_token FOREIGN KEY (token_id) REFERENCES registration_tokens(id) ON DELETE CASCADE,
    CONSTRAINT fk_registration_token_usage_user FOREIGN KEY (user_id) REFERENCES users(user_id) ON DELETE CASCADE NOT VALID
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

-- ============================================================================
-- Part 17: Application Service Tables
-- ============================================================================

CREATE TABLE IF NOT EXISTS application_services (
    id BIGSERIAL,
    as_id TEXT NOT NULL,
    url TEXT NOT NULL,
    as_token TEXT NOT NULL,
    hs_token TEXT NOT NULL,
    sender_localpart TEXT NOT NULL,
    is_enabled BOOLEAN DEFAULT FALSE,
    is_rate_limited BOOLEAN DEFAULT TRUE,
    protocols TEXT[] DEFAULT '{}',
    namespaces JSONB DEFAULT '{}',
    api_key TEXT,
    config JSONB NOT NULL DEFAULT '{}'::jsonb,
    created_ts BIGINT NOT NULL,
    updated_ts BIGINT,
    description TEXT,
    CONSTRAINT pk_application_services PRIMARY KEY (id),
    CONSTRAINT uq_application_services_id UNIQUE (as_id)
);

CREATE TABLE IF NOT EXISTS application_service_state (
    id BIGSERIAL,
    as_id TEXT NOT NULL,
    state_key TEXT NOT NULL,
    value JSONB NOT NULL,
    state_value TEXT,
    updated_ts BIGINT NOT NULL,
    CONSTRAINT pk_application_service_state PRIMARY KEY (id),
    CONSTRAINT uq_application_service_state_as_key UNIQUE (as_id, state_key),
    CONSTRAINT fk_application_service_state_as FOREIGN KEY (as_id) REFERENCES application_services(as_id) ON DELETE CASCADE
);

CREATE TABLE IF NOT EXISTS application_service_transactions (
    id BIGSERIAL,
    as_id TEXT NOT NULL,
    txn_id TEXT NOT NULL,
    data JSONB DEFAULT '{}',
    is_processed BOOLEAN DEFAULT FALSE,
    processed_ts BIGINT,
    created_ts BIGINT NOT NULL,
    transaction_id TEXT,
    events JSONB,
    sent_ts BIGINT,
    completed_ts BIGINT,
    retry_count INTEGER NOT NULL DEFAULT 0,
    last_error TEXT,
    CONSTRAINT pk_application_service_transactions PRIMARY KEY (id),
    CONSTRAINT uq_application_service_transactions_as_txn UNIQUE (as_id, txn_id)
);

CREATE TABLE IF NOT EXISTS application_service_events (
    id BIGSERIAL,
    as_id TEXT NOT NULL,
    event_id TEXT NOT NULL,
    room_id TEXT,
    event_type TEXT,
    is_processed BOOLEAN DEFAULT FALSE,
    processed_ts BIGINT,
    created_ts BIGINT NOT NULL,
    CONSTRAINT pk_application_service_events PRIMARY KEY (id),
    CONSTRAINT uq_application_service_events_event UNIQUE (event_id)
);

CREATE TABLE IF NOT EXISTS application_service_user_namespaces (
    id BIGSERIAL,
    as_id TEXT NOT NULL,
    namespace TEXT NOT NULL,
    is_exclusive BOOLEAN DEFAULT TRUE,
    created_ts BIGINT NOT NULL,
    CONSTRAINT pk_application_service_user_namespaces PRIMARY KEY (id),
    CONSTRAINT fk_application_service_user_namespaces_as FOREIGN KEY (as_id) REFERENCES application_services(as_id) ON DELETE CASCADE
);

CREATE TABLE IF NOT EXISTS application_service_room_alias_namespaces (
    id BIGSERIAL,
    as_id TEXT NOT NULL,
    namespace TEXT NOT NULL,
    is_exclusive BOOLEAN DEFAULT TRUE,
    created_ts BIGINT NOT NULL,
    CONSTRAINT pk_application_service_room_alias_namespaces PRIMARY KEY (id),
    CONSTRAINT fk_application_service_room_alias_namespaces_as FOREIGN KEY (as_id) REFERENCES application_services(as_id) ON DELETE CASCADE
);

CREATE TABLE IF NOT EXISTS application_service_room_namespaces (
    id BIGSERIAL,
    as_id TEXT NOT NULL,
    namespace TEXT NOT NULL,
    is_exclusive BOOLEAN DEFAULT TRUE,
    created_ts BIGINT NOT NULL,
    CONSTRAINT pk_application_service_room_namespaces PRIMARY KEY (id),
    CONSTRAINT fk_application_service_room_namespaces_as FOREIGN KEY (as_id) REFERENCES application_services(as_id) ON DELETE CASCADE
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
    is_rate_limited BOOLEAN NOT NULL DEFAULT TRUE,
    virtual_user_count BIGINT NOT NULL DEFAULT 0,
    pending_event_count BIGINT NOT NULL DEFAULT 0,
    pending_transaction_count BIGINT NOT NULL DEFAULT 0,
    last_seen_ts BIGINT,
    created_ts BIGINT NOT NULL DEFAULT (EXTRACT(EPOCH FROM clock_timestamp()) * 1000)::BIGINT,
    CONSTRAINT fk_application_service_statistics_as FOREIGN KEY (as_id) REFERENCES application_services(as_id) ON DELETE CASCADE
);

-- ============================================================================
-- Part 18: Sliding Sync Tables
-- ============================================================================

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
    is_invited BOOLEAN DEFAULT FALSE,
    name TEXT,
    avatar TEXT,
    timestamp BIGINT DEFAULT 0,
    created_ts BIGINT NOT NULL,
    updated_ts BIGINT NOT NULL
);

-- ============================================================================
-- Part 19: Thread Tables
-- ============================================================================

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
-- Part 20: Misc Tables
-- ============================================================================

CREATE TABLE IF NOT EXISTS typing (
    user_id TEXT NOT NULL,
    room_id TEXT NOT NULL,
    is_typing BOOLEAN DEFAULT FALSE,
    last_active_ts BIGINT NOT NULL,
    CONSTRAINT pk_typing PRIMARY KEY (user_id, room_id)
);

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

CREATE TABLE IF NOT EXISTS to_device_transactions (
    id BIGSERIAL PRIMARY KEY,
    transaction_id TEXT,
    message_id TEXT,
    sender_user_id TEXT NOT NULL,
    sender_device_id TEXT NOT NULL,
    created_ts BIGINT NOT NULL,
    CONSTRAINT uq_to_device_transactions_txn UNIQUE (transaction_id, sender_user_id, sender_device_id)
);

CREATE TABLE IF NOT EXISTS device_lists_changes (
    id SERIAL PRIMARY KEY,
    user_id VARCHAR(255) NOT NULL,
    device_id VARCHAR(255),
    change_type VARCHAR(50) NOT NULL,
    stream_id BIGINT NOT NULL,
    created_ts BIGINT NOT NULL
);

CREATE TABLE IF NOT EXISTS device_lists_stream (
    stream_id BIGSERIAL PRIMARY KEY,
    user_id VARCHAR(255) NOT NULL,
    device_id VARCHAR(255),
    created_ts BIGINT NOT NULL
);

CREATE TABLE IF NOT EXISTS room_ephemeral (
    id SERIAL PRIMARY KEY,
    room_id VARCHAR(255) NOT NULL,
    event_type VARCHAR(255) NOT NULL,
    user_id VARCHAR(255) NOT NULL,
    content JSONB NOT NULL DEFAULT '{}',
    stream_id BIGINT NOT NULL,
    created_ts BIGINT NOT NULL,
    expires_at BIGINT,
    CONSTRAINT uq_room_ephemeral_room_event_user UNIQUE (room_id, event_type, user_id)
);

CREATE TABLE IF NOT EXISTS lazy_loaded_members (
    user_id TEXT NOT NULL,
    device_id TEXT NOT NULL,
    room_id TEXT NOT NULL,
    member_user_id TEXT NOT NULL,
    created_ts BIGINT NOT NULL,
    updated_ts BIGINT NOT NULL,
    CONSTRAINT pk_lazy_loaded_members PRIMARY KEY (user_id, device_id, room_id, member_user_id)
);

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

CREATE TABLE IF NOT EXISTS password_history (
    id BIGSERIAL PRIMARY KEY,
    user_id TEXT NOT NULL,
    password_hash TEXT NOT NULL,
    created_ts BIGINT NOT NULL,
    CONSTRAINT fk_password_history_user FOREIGN KEY (user_id) REFERENCES users(user_id) ON DELETE CASCADE
);

CREATE TABLE IF NOT EXISTS server_retention_policy (
    id BIGSERIAL,
    max_lifetime BIGINT,
    min_lifetime BIGINT NOT NULL DEFAULT 0,
    is_expire_on_clients BOOLEAN NOT NULL DEFAULT FALSE,
    created_ts BIGINT NOT NULL,
    updated_ts BIGINT NOT NULL,
    CONSTRAINT pk_server_retention_policy PRIMARY KEY (id)
);

CREATE TABLE IF NOT EXISTS schema_migrations (
    id BIGSERIAL,
    version TEXT NOT NULL,
    name TEXT,
    checksum TEXT,
    applied_ts BIGINT,
    execution_time_ms BIGINT,
    is_success BOOLEAN NOT NULL DEFAULT TRUE,
    description TEXT,
    executed_at BIGINT DEFAULT (EXTRACT(EPOCH FROM NOW())::BIGINT * 1000),
    CONSTRAINT pk_schema_migrations PRIMARY KEY (id),
    CONSTRAINT uq_schema_migrations_version UNIQUE (version)
);

CREATE TABLE IF NOT EXISTS db_metadata (
    id BIGSERIAL PRIMARY KEY,
    key TEXT NOT NULL UNIQUE,
    value TEXT NOT NULL,
    created_ts BIGINT NOT NULL,
    updated_ts BIGINT NOT NULL
);

CREATE TABLE IF NOT EXISTS migration_audit (
    id BIGSERIAL PRIMARY KEY,
    version VARCHAR(50) NOT NULL,
    description TEXT,
    duration_ms BIGINT NOT NULL,
    rows_affected BIGINT DEFAULT 0,
    executed_by VARCHAR(100) NOT NULL DEFAULT CURRENT_USER,
    executed_at BIGINT NOT NULL DEFAULT (EXTRACT(EPOCH FROM NOW())::BIGINT * 1000),
    status VARCHAR(20) NOT NULL DEFAULT 'SUCCESS',
    error_message TEXT,
    checksum VARCHAR(64),
    migration_file VARCHAR(255),
    created_ts BIGINT NOT NULL DEFAULT (EXTRACT(EPOCH FROM NOW()) * 1000)::BIGINT
);

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

CREATE TABLE IF NOT EXISTS rendezvous_messages (
    id BIGSERIAL,
    session_id TEXT NOT NULL,
    direction TEXT NOT NULL,
    message_type TEXT NOT NULL,
    content JSONB NOT NULL,
    created_ts BIGINT NOT NULL,
    CONSTRAINT pk_rendezvous_messages PRIMARY KEY (id)
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

CREATE TABLE IF NOT EXISTS user_notification_settings (
    user_id TEXT PRIMARY KEY,
    is_enabled BOOLEAN NOT NULL DEFAULT TRUE,
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

-- OpenClaw / AI Integration Tables
CREATE TABLE IF NOT EXISTS openclaw_connections (
    id BIGSERIAL PRIMARY KEY,
    user_id TEXT NOT NULL,
    name TEXT NOT NULL,
    provider TEXT NOT NULL,
    base_url TEXT NOT NULL,
    encrypted_api_key TEXT,
    config JSONB DEFAULT '{}',
    is_default BOOLEAN DEFAULT FALSE,
    is_active BOOLEAN DEFAULT TRUE,
    created_ts BIGINT NOT NULL,
    updated_ts BIGINT NOT NULL,
    UNIQUE(user_id, name)
);

CREATE TABLE IF NOT EXISTS ai_conversations (
    id BIGSERIAL PRIMARY KEY,
    user_id TEXT NOT NULL,
    connection_id BIGINT REFERENCES openclaw_connections(id) ON DELETE SET NULL,
    title TEXT,
    model_id TEXT,
    system_prompt TEXT,
    temperature REAL DEFAULT 0.7,
    max_tokens INTEGER DEFAULT 4096,
    is_pinned BOOLEAN DEFAULT FALSE,
    metadata JSONB DEFAULT '{}',
    created_ts BIGINT NOT NULL,
    updated_ts BIGINT NOT NULL
);

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
    is_public BOOLEAN DEFAULT FALSE,
    metadata JSONB DEFAULT '{}',
    created_ts BIGINT NOT NULL,
    updated_ts BIGINT NOT NULL
);

CREATE TABLE IF NOT EXISTS ai_connections (
    id VARCHAR(36) PRIMARY KEY,
    user_id VARCHAR(255) NOT NULL,
    provider VARCHAR(50) NOT NULL,
    config JSONB,
    is_active BOOLEAN DEFAULT TRUE,
    created_ts BIGINT NOT NULL,
    updated_ts BIGINT
);

-- Beacon / Call / MatrixRTC Tables
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

CREATE TABLE IF NOT EXISTS call_candidates (
    id BIGSERIAL PRIMARY KEY,
    call_id TEXT NOT NULL,
    room_id TEXT NOT NULL,
    sender_id TEXT NOT NULL,
    candidate JSONB NOT NULL,
    created_ts BIGINT NOT NULL
);

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

CREATE UNIQUE INDEX IF NOT EXISTS idx_matrixrtc_sessions_room_session ON matrixrtc_sessions(room_id, session_id);
CREATE UNIQUE INDEX IF NOT EXISTS idx_matrixrtc_memberships_room_session_user_device ON matrixrtc_memberships(room_id, session_id, user_id, device_id);
CREATE UNIQUE INDEX IF NOT EXISTS idx_matrixrtc_encryption_keys_room_session_idx ON matrixrtc_encryption_keys(room_id, session_id, key_index);

-- ============================================================================
-- Indexes (Consolidated)
-- ============================================================================

-- Users
CREATE INDEX IF NOT EXISTS idx_users_email ON users(email);
CREATE INDEX IF NOT EXISTS idx_users_is_admin ON users(is_admin);
CREATE INDEX IF NOT EXISTS idx_users_created_ts ON users(created_ts DESC);
CREATE INDEX IF NOT EXISTS idx_users_must_change_password ON users(must_change_password) WHERE must_change_password = TRUE;
CREATE INDEX IF NOT EXISTS idx_users_password_expires ON users(password_expires_at) WHERE password_expires_at IS NOT NULL;
CREATE INDEX IF NOT EXISTS idx_users_locked ON users(locked_until) WHERE locked_until IS NOT NULL;
CREATE INDEX IF NOT EXISTS idx_users_lower_username ON users(LOWER(username));
CREATE INDEX IF NOT EXISTS idx_users_lower_displayname ON users(LOWER(COALESCE(displayname, '')));
CREATE INDEX IF NOT EXISTS idx_users_lower_email ON users(LOWER(COALESCE(email, '')));
CREATE INDEX IF NOT EXISTS idx_users_username_trgm ON users USING GIN (username gin_trgm_ops);
CREATE INDEX IF NOT EXISTS idx_users_displayname_trgm ON users USING GIN (displayname gin_trgm_ops);
CREATE INDEX IF NOT EXISTS idx_users_user_id_trgm ON users USING GIN (user_id gin_trgm_ops);
CREATE INDEX IF NOT EXISTS idx_users_email_trgm ON users USING GIN (email gin_trgm_ops);

-- User threepids
CREATE INDEX IF NOT EXISTS idx_user_threepids_user ON user_threepids(user_id);
CREATE INDEX IF NOT EXISTS idx_user_threepids_medium_address ON user_threepids(medium, address);

-- Devices
CREATE INDEX IF NOT EXISTS idx_devices_user_id ON devices(user_id);
CREATE INDEX IF NOT EXISTS idx_devices_last_seen ON devices(last_seen_ts DESC);

-- Access tokens
CREATE INDEX IF NOT EXISTS idx_access_tokens_user_id ON access_tokens(user_id);
CREATE INDEX IF NOT EXISTS idx_access_tokens_valid ON access_tokens(is_revoked) WHERE is_revoked = FALSE;
CREATE INDEX IF NOT EXISTS idx_access_tokens_token_hash ON access_tokens(token_hash);
CREATE INDEX IF NOT EXISTS idx_access_tokens_user_revoked ON access_tokens(user_id, is_revoked) WHERE is_revoked = FALSE;
CREATE INDEX IF NOT EXISTS idx_access_tokens_device_id ON access_tokens(device_id) WHERE device_id IS NOT NULL;
CREATE INDEX IF NOT EXISTS idx_access_tokens_expires ON access_tokens(expires_at) WHERE is_revoked = FALSE;

-- Refresh tokens
CREATE INDEX IF NOT EXISTS idx_refresh_tokens_user_id ON refresh_tokens(user_id);
CREATE INDEX IF NOT EXISTS idx_refresh_tokens_revoked ON refresh_tokens(is_revoked) WHERE is_revoked = FALSE;
CREATE INDEX IF NOT EXISTS idx_refresh_tokens_device_id ON refresh_tokens(device_id) WHERE device_id IS NOT NULL;

-- Token blacklist
CREATE INDEX IF NOT EXISTS idx_token_blacklist_hash ON token_blacklist(token_hash);
CREATE INDEX IF NOT EXISTS idx_token_blacklist_user_id ON token_blacklist(user_id) WHERE user_id IS NOT NULL;

-- Rooms
CREATE INDEX IF NOT EXISTS idx_rooms_creator ON rooms(creator) WHERE creator IS NOT NULL;
CREATE INDEX IF NOT EXISTS idx_rooms_is_public ON rooms(is_public) WHERE is_public = TRUE;
CREATE INDEX IF NOT EXISTS idx_rooms_last_activity ON rooms(last_activity_ts DESC) WHERE last_activity_ts IS NOT NULL;
CREATE INDEX IF NOT EXISTS idx_rooms_name_trgm ON rooms USING GIN (name gin_trgm_ops);
CREATE INDEX IF NOT EXISTS idx_rooms_canonical_alias_trgm ON rooms USING GIN (canonical_alias gin_trgm_ops);

-- Room memberships
CREATE INDEX IF NOT EXISTS idx_room_memberships_room ON room_memberships(room_id);
CREATE INDEX IF NOT EXISTS idx_room_memberships_user ON room_memberships(user_id);
CREATE INDEX IF NOT EXISTS idx_room_memberships_membership ON room_memberships(membership);
CREATE INDEX IF NOT EXISTS idx_room_memberships_user_membership ON room_memberships(user_id, membership);
CREATE INDEX IF NOT EXISTS idx_room_memberships_room_membership ON room_memberships(room_id, membership);
CREATE INDEX IF NOT EXISTS idx_room_memberships_joined ON room_memberships(user_id, room_id) WHERE membership = 'join';
CREATE INDEX IF NOT EXISTS idx_room_memberships_user_status ON room_memberships(user_id, membership, joined_ts DESC);
CREATE INDEX IF NOT EXISTS idx_room_memberships_room_status ON room_memberships(room_id, membership);
CREATE INDEX IF NOT EXISTS idx_room_memberships_room_user ON room_memberships(room_id, user_id);
CREATE INDEX IF NOT EXISTS idx_memberships_user_room ON room_memberships(user_id, room_id);

-- Events
CREATE INDEX IF NOT EXISTS idx_events_room_id ON events(room_id);
CREATE INDEX IF NOT EXISTS idx_events_sender ON events(sender);
CREATE INDEX IF NOT EXISTS idx_events_type ON events(event_type);
CREATE INDEX IF NOT EXISTS idx_events_origin_server_ts ON events(origin_server_ts DESC);
CREATE INDEX IF NOT EXISTS idx_events_not_redacted ON events(room_id, origin_server_ts DESC) WHERE is_redacted = FALSE;
CREATE INDEX IF NOT EXISTS idx_events_room_time ON events(room_id, origin_server_ts DESC);
CREATE INDEX IF NOT EXISTS idx_events_sender_type ON events(sender, event_type);
CREATE INDEX IF NOT EXISTS idx_events_content_gin ON events USING GIN (content jsonb_path_ops);
CREATE INDEX IF NOT EXISTS idx_events_type_state ON events(room_id, event_type, state_key) WHERE event_type LIKE 'm.room.%';
CREATE INDEX IF NOT EXISTS idx_events_sender_time ON events(sender, origin_server_ts DESC);
CREATE INDEX IF NOT EXISTS idx_events_stream_ordering ON events(stream_ordering);
CREATE INDEX IF NOT EXISTS idx_events_room_stream_ordering ON events(room_id, stream_ordering DESC);
CREATE INDEX IF NOT EXISTS idx_events_room_stream_ordering_not_redacted ON events(room_id, stream_ordering DESC) WHERE is_redacted = FALSE;
CREATE INDEX IF NOT EXISTS idx_events_sync_covering ON events(room_id, stream_ordering DESC) INCLUDE (event_id, sender, event_type, content, origin_server_ts);
CREATE INDEX IF NOT EXISTS idx_events_friend_room ON events(sender, room_id, origin_server_ts DESC) WHERE event_type = 'm.room.create' AND content->>'type' = 'm.friends';
CREATE INDEX IF NOT EXISTS idx_events_friend_list ON events(room_id, origin_server_ts DESC) WHERE event_type = 'm.friends.list' AND state_key = '';
CREATE INDEX IF NOT EXISTS idx_events_redacts ON events(redacts) WHERE redacts IS NOT NULL;

-- Event relations
CREATE UNIQUE INDEX IF NOT EXISTS idx_event_relations_unique ON event_relations(event_id, relation_type, sender);
CREATE INDEX IF NOT EXISTS idx_event_relations_room_event ON event_relations(room_id, relates_to_event_id, relation_type);
CREATE INDEX IF NOT EXISTS idx_event_relations_sender ON event_relations(sender, relation_type);
CREATE INDEX IF NOT EXISTS idx_event_relations_origin_ts ON event_relations(room_id, origin_server_ts DESC);

-- Room summaries
CREATE INDEX IF NOT EXISTS idx_room_summaries_last_event_ts ON room_summaries(last_event_ts DESC);
CREATE INDEX IF NOT EXISTS idx_room_summaries_space ON room_summaries(is_space) WHERE is_space = TRUE;
CREATE INDEX IF NOT EXISTS idx_room_summaries_room_id ON room_summaries(room_id);

-- Room summary members
CREATE INDEX IF NOT EXISTS idx_room_summary_members_user_membership_room ON room_summary_members(user_id, membership, room_id);
CREATE INDEX IF NOT EXISTS idx_room_summary_members_room_membership_hero_active ON room_summary_members(room_id, membership, is_hero DESC, last_active_ts DESC);
CREATE INDEX IF NOT EXISTS idx_room_summary_members_room_hero_user ON room_summary_members(room_id, is_hero DESC, user_id);
CREATE INDEX IF NOT EXISTS idx_room_summary_members_room_user ON room_summary_members(room_id, user_id);

-- Room summary state
CREATE INDEX IF NOT EXISTS idx_room_summary_state_room ON room_summary_state(room_id);

-- Room summary update queue
CREATE INDEX IF NOT EXISTS idx_room_summary_update_queue_status_priority_created ON room_summary_update_queue(status, priority DESC, created_ts ASC);

-- Room directory
CREATE INDEX IF NOT EXISTS idx_room_directory_public ON room_directory(is_public) WHERE is_public = TRUE;

-- Room aliases
CREATE INDEX IF NOT EXISTS idx_room_aliases_room_id ON room_aliases(room_id);
CREATE UNIQUE INDEX IF NOT EXISTS idx_room_aliases_room_alias ON room_aliases(room_alias);

-- Room invites
CREATE INDEX IF NOT EXISTS idx_room_invites_room ON room_invites(room_id);
CREATE INDEX IF NOT EXISTS idx_room_invites_invitee ON room_invites(invitee);
CREATE UNIQUE INDEX IF NOT EXISTS uq_room_invites_invite_code ON room_invites(invite_code) WHERE invite_code IS NOT NULL;

-- Room invite blocklist/allowlist
CREATE INDEX IF NOT EXISTS idx_room_invite_blocklist_room ON room_invite_blocklist(room_id);
CREATE INDEX IF NOT EXISTS idx_room_invite_blocklist_user ON room_invite_blocklist(user_id);
CREATE INDEX IF NOT EXISTS idx_room_invite_blocklist_room_user ON room_invite_blocklist(room_id, user_id);
CREATE INDEX IF NOT EXISTS idx_room_invite_allowlist_room ON room_invite_allowlist(room_id);
CREATE INDEX IF NOT EXISTS idx_room_invite_allowlist_user ON room_invite_allowlist(user_id);
CREATE INDEX IF NOT EXISTS idx_room_invite_allowlist_room_user ON room_invite_allowlist(room_id, user_id);

-- Room tags
CREATE INDEX IF NOT EXISTS idx_room_tags_user ON room_tags(user_id);
CREATE INDEX IF NOT EXISTS idx_room_tags_user_room ON room_tags(user_id, room_id);

-- Room sticky events
CREATE INDEX IF NOT EXISTS idx_room_sticky_events_user_sticky ON room_sticky_events(user_id, is_sticky, room_id);

-- Room parents
CREATE INDEX IF NOT EXISTS idx_room_parents_room ON room_parents(room_id);
CREATE INDEX IF NOT EXISTS idx_room_parents_parent ON room_parents(parent_room_id);

-- Room retention policies
CREATE INDEX IF NOT EXISTS idx_room_retention_policies_server_default ON room_retention_policies(is_server_default) WHERE is_server_default = TRUE;

-- Room stats current
CREATE INDEX IF NOT EXISTS idx_room_stats_joined ON room_stats_current(joined_members DESC);
CREATE INDEX IF NOT EXISTS idx_room_stats_local ON room_stats_current(local_users_in_room DESC);

-- Device keys
CREATE INDEX IF NOT EXISTS idx_device_keys_user_device ON device_keys(user_id, device_id);
CREATE INDEX IF NOT EXISTS idx_device_keys_fallback ON device_keys(user_id, device_id) WHERE is_fallback = TRUE;

-- Cross signing keys
CREATE INDEX IF NOT EXISTS idx_cross_signing_keys_user ON cross_signing_keys(user_id);

-- Device trust status
CREATE INDEX IF NOT EXISTS idx_device_trust_status_user_level ON device_trust_status(user_id, trust_level);

-- Cross signing trust
CREATE INDEX IF NOT EXISTS idx_cross_signing_trust_user_trusted ON cross_signing_trust(user_id, is_trusted);

-- Key signatures
CREATE INDEX IF NOT EXISTS idx_key_signatures_target ON key_signatures(target_user_id, target_key_id);

-- Key rotation log
CREATE INDEX IF NOT EXISTS idx_key_rotation_log_user_rotated ON key_rotation_log(user_id, rotated_at DESC);

-- E2EE security events
CREATE INDEX IF NOT EXISTS idx_e2ee_security_events_user_created ON e2ee_security_events(user_id, created_ts DESC);

-- Verification requests
CREATE INDEX IF NOT EXISTS idx_verification_requests_to_user_state ON verification_requests(to_user, state, updated_ts DESC);

-- Megolm sessions
CREATE INDEX IF NOT EXISTS idx_megolm_sessions_room ON megolm_sessions(room_id);
CREATE INDEX IF NOT EXISTS idx_megolm_sessions_session ON megolm_sessions(session_id);
-- Phase 2: 支持按 pickle_format 过滤的懒迁移查询（只查 'legacy' 存量）
CREATE INDEX IF NOT EXISTS idx_megolm_sessions_pickle_format ON megolm_sessions(pickle_format) WHERE pickle_format = 'legacy';

-- Event signatures
CREATE INDEX IF NOT EXISTS idx_event_signatures_event ON event_signatures(event_id);

-- Key backups
CREATE INDEX IF NOT EXISTS idx_key_backups_user ON key_backups(user_id);

-- Backup keys
CREATE INDEX IF NOT EXISTS idx_backup_keys_backup ON backup_keys(backup_id);
CREATE INDEX IF NOT EXISTS idx_backup_keys_room ON backup_keys(room_id);

-- Olm accounts
CREATE INDEX IF NOT EXISTS idx_olm_accounts_user ON olm_accounts(user_id);
CREATE INDEX IF NOT EXISTS idx_olm_accounts_device ON olm_accounts(device_id);

-- Olm sessions
CREATE INDEX IF NOT EXISTS idx_olm_sessions_user_device ON olm_sessions(user_id, device_id);
CREATE INDEX IF NOT EXISTS idx_olm_sessions_sender_key ON olm_sessions(sender_key);
CREATE INDEX IF NOT EXISTS idx_olm_sessions_expires ON olm_sessions(expires_at) WHERE expires_at IS NOT NULL;

-- E2EE key requests
CREATE INDEX IF NOT EXISTS idx_e2ee_key_requests_user ON e2ee_key_requests(user_id);
CREATE INDEX IF NOT EXISTS idx_e2ee_key_requests_session ON e2ee_key_requests(session_id);
CREATE INDEX IF NOT EXISTS idx_e2ee_key_requests_pending ON e2ee_key_requests(is_fulfilled) WHERE is_fulfilled = FALSE;

-- Device verification request
CREATE INDEX IF NOT EXISTS idx_device_verification_request_user_device_pending ON device_verification_request(user_id, new_device_id) WHERE status = 'pending';
CREATE INDEX IF NOT EXISTS idx_device_verification_request_expires_pending ON device_verification_request(expires_at) WHERE status = 'pending';

-- One time keys
CREATE INDEX IF NOT EXISTS idx_one_time_keys_user_device ON one_time_keys(user_id, device_id);
CREATE INDEX IF NOT EXISTS idx_one_time_keys_used ON one_time_keys(is_used) WHERE is_used = FALSE;

-- Dehydrated devices
CREATE INDEX IF NOT EXISTS idx_dehydrated_devices_user ON dehydrated_devices(user_id);
CREATE INDEX IF NOT EXISTS idx_dehydrated_devices_expires ON dehydrated_devices(expires_at) WHERE expires_at IS NOT NULL;

-- E2EE audit log
CREATE INDEX IF NOT EXISTS idx_e2ee_audit_log_user ON e2ee_audit_log(user_id);
CREATE INDEX IF NOT EXISTS idx_e2ee_audit_log_created ON e2ee_audit_log(created_ts DESC);
CREATE INDEX IF NOT EXISTS idx_e2ee_audit_log_action ON e2ee_audit_log(action);
CREATE INDEX IF NOT EXISTS idx_e2ee_audit_log_user_created ON e2ee_audit_log(user_id, created_ts DESC);

-- E2EE secret storage keys
CREATE INDEX IF NOT EXISTS idx_e2ee_secret_storage_keys_user ON e2ee_secret_storage_keys(user_id);
CREATE INDEX IF NOT EXISTS idx_e2ee_secret_storage_keys_key_id ON e2ee_secret_storage_keys(key_id);

-- E2EE stored secrets
CREATE UNIQUE INDEX IF NOT EXISTS idx_e2ee_stored_secrets_user_name ON e2ee_stored_secrets(user_id, secret_name);
CREATE INDEX IF NOT EXISTS idx_e2ee_stored_secrets_key ON e2ee_stored_secrets(key_key_id);

-- Leak alerts
CREATE INDEX IF NOT EXISTS idx_leak_alerts_user ON leak_alerts(user_id);
CREATE INDEX IF NOT EXISTS idx_leak_alerts_created ON leak_alerts(created_ts DESC);
CREATE INDEX IF NOT EXISTS idx_leak_alerts_acknowledged ON leak_alerts(is_acknowledged) WHERE is_acknowledged = FALSE;

-- Media
CREATE INDEX IF NOT EXISTS idx_media_uploader ON media_metadata(uploader_user_id);
CREATE INDEX IF NOT EXISTS idx_media_server ON media_metadata(server_name);
CREATE INDEX IF NOT EXISTS idx_thumbnails_media ON thumbnails(media_id);
CREATE INDEX IF NOT EXISTS idx_user_media_quota_used ON user_media_quota(used_bytes DESC) WHERE used_bytes > 0;
CREATE INDEX IF NOT EXISTS idx_media_quota_config_enabled ON media_quota_config(is_enabled) WHERE is_enabled = TRUE;
CREATE INDEX IF NOT EXISTS idx_media_usage_log_user ON media_usage_log(user_id);
CREATE INDEX IF NOT EXISTS idx_media_usage_log_timestamp ON media_usage_log(timestamp);
CREATE INDEX IF NOT EXISTS idx_media_quota_alerts_user ON media_quota_alerts(user_id) WHERE is_read = FALSE;
CREATE INDEX IF NOT EXISTS idx_upload_progress_expires ON upload_progress(expires_at ASC);
CREATE INDEX IF NOT EXISTS idx_upload_progress_user_created_active ON upload_progress(user_id, created_ts DESC) WHERE status <> 'finalized';
CREATE INDEX IF NOT EXISTS idx_upload_chunks_upload_order ON upload_chunks(upload_id, chunk_index ASC);

-- Voice messages
CREATE INDEX IF NOT EXISTS idx_voice_messages_room ON voice_messages(room_id);
CREATE INDEX IF NOT EXISTS idx_voice_messages_user ON voice_messages(user_id);
CREATE INDEX IF NOT EXISTS idx_voice_messages_processed ON voice_messages(is_processed);
CREATE INDEX IF NOT EXISTS idx_voice_messages_room_ts ON voice_messages(room_id, created_ts DESC);
CREATE INDEX IF NOT EXISTS idx_voice_messages_user_ts ON voice_messages(user_id, created_ts DESC);

-- Voice usage stats
CREATE INDEX IF NOT EXISTS idx_voice_usage_stats_user ON voice_usage_stats(user_id, created_ts DESC);
CREATE INDEX IF NOT EXISTS idx_voice_usage_stats_room ON voice_usage_stats(room_id, created_ts DESC);

-- CAS
CREATE INDEX IF NOT EXISTS idx_cas_tickets_user ON cas_tickets(user_id);

-- SAML
CREATE INDEX IF NOT EXISTS idx_saml_sessions_user ON saml_sessions(user_id);

-- Captcha
CREATE INDEX IF NOT EXISTS idx_captcha_target ON registration_captcha(target);
CREATE INDEX IF NOT EXISTS idx_captcha_status ON registration_captcha(status);
CREATE INDEX IF NOT EXISTS idx_captcha_send_target ON captcha_send_log(target);

-- Push devices
CREATE INDEX IF NOT EXISTS idx_push_devices_user ON push_devices(user_id);
CREATE INDEX IF NOT EXISTS idx_push_device_user_enabled ON push_device(user_id) WHERE is_enabled = TRUE;

-- Push rules
CREATE INDEX IF NOT EXISTS idx_push_rules_user ON push_rules(user_id);
CREATE INDEX IF NOT EXISTS idx_push_rules_user_priority ON push_rules(user_id, priority);

-- Pushers
CREATE INDEX IF NOT EXISTS idx_pushers_user ON pushers(user_id);
CREATE INDEX IF NOT EXISTS idx_pushers_enabled ON pushers(is_enabled) WHERE is_enabled = TRUE;

-- Push notification queue
CREATE INDEX IF NOT EXISTS idx_push_queue_user ON push_notification_queue(user_id);
CREATE INDEX IF NOT EXISTS idx_push_queue_processed ON push_notification_queue(is_processed);

-- Push notification log
CREATE INDEX IF NOT EXISTS idx_push_log_user ON push_notification_log(user_id);
CREATE INDEX IF NOT EXISTS idx_push_log_status ON push_notification_log(status);

-- Push config
CREATE INDEX IF NOT EXISTS idx_push_config_user ON push_config(user_id);

-- Notifications
CREATE INDEX IF NOT EXISTS idx_notifications_user_id ON notifications(user_id);
CREATE INDEX IF NOT EXISTS idx_notifications_ts ON notifications(ts DESC);
CREATE INDEX IF NOT EXISTS idx_notifications_room ON notifications(room_id);

-- Spaces
CREATE INDEX IF NOT EXISTS idx_space_children_space ON space_children(space_id);
CREATE INDEX IF NOT EXISTS idx_space_children_room ON space_children(room_id);
CREATE INDEX IF NOT EXISTS idx_spaces_creator ON spaces(creator);
CREATE INDEX IF NOT EXISTS idx_spaces_public ON spaces(is_public) WHERE is_public = TRUE;
CREATE INDEX IF NOT EXISTS idx_spaces_parent ON spaces(parent_space_id) WHERE parent_space_id IS NOT NULL;
CREATE INDEX IF NOT EXISTS idx_space_members_space ON space_members(space_id);
CREATE INDEX IF NOT EXISTS idx_space_members_user ON space_members(user_id);
CREATE INDEX IF NOT EXISTS idx_space_members_membership ON space_members(membership);
CREATE INDEX IF NOT EXISTS idx_space_summary_space ON space_summaries(space_id);
CREATE INDEX IF NOT EXISTS idx_space_statistics_member_count ON space_statistics(member_count DESC);
CREATE INDEX IF NOT EXISTS idx_space_events_space ON space_events(space_id);
CREATE INDEX IF NOT EXISTS idx_space_events_space_type_ts ON space_events(space_id, event_type, origin_server_ts DESC);
CREATE INDEX IF NOT EXISTS idx_space_events_space_ts ON space_events(space_id, origin_server_ts DESC);
CREATE INDEX IF NOT EXISTS idx_space_hierarchy_space ON space_hierarchy(space_id);
CREATE INDEX IF NOT EXISTS idx_spaces_name_trgm ON spaces USING GIN (name gin_trgm_ops);
CREATE INDEX IF NOT EXISTS idx_spaces_topic_trgm ON spaces USING GIN (topic gin_trgm_ops);

-- Federation
CREATE INDEX IF NOT EXISTS idx_federation_servers_status ON federation_servers(status);
CREATE INDEX IF NOT EXISTS idx_federation_blacklist_server ON federation_blacklist(server_name);
CREATE INDEX IF NOT EXISTS idx_federation_blacklist_config_enabled ON federation_blacklist_config(is_enabled) WHERE is_enabled = TRUE;
CREATE INDEX IF NOT EXISTS idx_federation_blacklist_log_server ON federation_blacklist_log(server_name);
CREATE INDEX IF NOT EXISTS idx_federation_blacklist_log_performed ON federation_blacklist_log(performed_ts DESC);
CREATE INDEX IF NOT EXISTS idx_federation_blacklist_rule_enabled ON federation_blacklist_rule(is_enabled) WHERE is_enabled = TRUE;
CREATE INDEX IF NOT EXISTS idx_federation_blacklist_rule_priority ON federation_blacklist_rule(priority DESC);
CREATE INDEX IF NOT EXISTS idx_federation_queue_destination ON federation_queue(destination);
CREATE INDEX IF NOT EXISTS idx_federation_queue_status ON federation_queue(status);
CREATE INDEX IF NOT EXISTS idx_federation_queue_pending ON federation_queue(destination, created_ts) WHERE status = 'pending';
CREATE INDEX IF NOT EXISTS idx_federation_queue_dest_status ON federation_queue(destination, status, created_ts);
CREATE INDEX IF NOT EXISTS idx_federation_inbound_events_origin ON federation_inbound_events(origin);
CREATE INDEX IF NOT EXISTS idx_federation_inbound_events_received ON federation_inbound_events(received_ts DESC);
CREATE INDEX IF NOT EXISTS idx_federation_signing_keys_server_created ON federation_signing_keys(server_name, created_ts DESC);
CREATE INDEX IF NOT EXISTS idx_federation_signing_keys_key_id ON federation_signing_keys(key_id);
CREATE INDEX IF NOT EXISTS idx_federation_access_stats_server ON federation_access_stats(server_name);
CREATE INDEX IF NOT EXISTS idx_federation_cache_key ON federation_cache(key);
CREATE INDEX IF NOT EXISTS idx_federation_cache_expiry ON federation_cache(expiry_ts);
CREATE INDEX IF NOT EXISTS idx_event_edges_prev ON event_edges(prev_event_id);
CREATE INDEX IF NOT EXISTS idx_event_forward_extremities_room ON event_forward_extremities(room_id);
CREATE INDEX IF NOT EXISTS idx_destination_retry_next ON destination_retry_timings(retry_last_ts, failure_count);
CREATE INDEX IF NOT EXISTS idx_device_lists_outbound_stream ON device_lists_outbound_pokes(stream_id);
CREATE INDEX IF NOT EXISTS idx_device_lists_outbound_dest ON device_lists_outbound_pokes(destination);
CREATE INDEX IF NOT EXISTS idx_device_lists_outbound_pokes_user ON device_lists_outbound_pokes(user_id);

-- Filters
CREATE INDEX IF NOT EXISTS idx_filters_user ON filters(user_id);
CREATE INDEX IF NOT EXISTS idx_filters_filter_id ON filters(filter_id);
CREATE INDEX IF NOT EXISTS idx_user_filters_user_id ON user_filters(user_id);

-- OpenID tokens
CREATE INDEX IF NOT EXISTS idx_openid_tokens_user ON openid_tokens(user_id);

-- Account data
CREATE INDEX IF NOT EXISTS idx_account_data_user ON account_data(user_id);
CREATE INDEX IF NOT EXISTS idx_account_data_user_type ON account_data(user_id, data_type);
CREATE INDEX IF NOT EXISTS idx_account_data_content_gin ON account_data USING GIN (content);

-- Threepid validation session
CREATE INDEX IF NOT EXISTS idx_threepid_session_token ON threepid_validation_session(token);
CREATE INDEX IF NOT EXISTS idx_threepid_session_address ON threepid_validation_session(medium, address);
CREATE INDEX IF NOT EXISTS idx_threepid_session_expires ON threepid_validation_session(expires_at) WHERE is_validated = FALSE;

-- Background updates
CREATE INDEX IF NOT EXISTS idx_background_updates_status ON background_updates(status);
CREATE INDEX IF NOT EXISTS idx_background_updates_running ON background_updates(is_running) WHERE is_running = TRUE;
CREATE INDEX IF NOT EXISTS idx_background_updates_running_job ON background_updates(job_name, started_ts) WHERE status = 'running';
CREATE INDEX IF NOT EXISTS idx_background_updates_pending ON background_updates(status, job_type, created_ts) WHERE status IN ('pending', 'scheduled');
CREATE INDEX IF NOT EXISTS idx_background_update_locks_expires ON background_update_locks(expires_at);
CREATE INDEX IF NOT EXISTS idx_background_update_history_job_start ON background_update_history(job_name, execution_start_ts DESC);
CREATE INDEX IF NOT EXISTS idx_background_update_stats_created ON background_update_stats(created_ts DESC);

-- Workers
CREATE INDEX IF NOT EXISTS idx_workers_type ON workers(worker_type);
CREATE INDEX IF NOT EXISTS idx_workers_status ON workers(status);
CREATE INDEX IF NOT EXISTS idx_workers_heartbeat ON workers(last_heartbeat_ts) WHERE last_heartbeat_ts IS NOT NULL;
CREATE INDEX IF NOT EXISTS idx_worker_commands_target ON worker_commands(target_worker_id);
CREATE INDEX IF NOT EXISTS idx_worker_commands_status ON worker_commands(status);
CREATE INDEX IF NOT EXISTS idx_worker_events_stream ON worker_events(stream_id);
CREATE INDEX IF NOT EXISTS idx_worker_events_type ON worker_events(event_type);
CREATE INDEX IF NOT EXISTS idx_worker_task_assignments_status_priority_created ON worker_task_assignments(status, priority DESC, created_ts ASC);
CREATE INDEX IF NOT EXISTS idx_worker_task_assignments_worker_status ON worker_task_assignments(assigned_worker_id, status);

-- Modules
CREATE INDEX IF NOT EXISTS idx_modules_enabled ON modules(is_enabled);
CREATE INDEX IF NOT EXISTS idx_modules_type_enabled_priority ON modules(module_type, is_enabled, priority);
CREATE INDEX IF NOT EXISTS idx_module_logs_module ON module_execution_logs(module_id);
CREATE INDEX IF NOT EXISTS idx_module_logs_created ON module_execution_logs(created_ts);
CREATE INDEX IF NOT EXISTS idx_module_execution_logs_module_name_executed ON module_execution_logs(module_name, executed_ts DESC);

-- Spam check results
CREATE INDEX IF NOT EXISTS idx_spam_results_event ON spam_check_results(event_id);
CREATE INDEX IF NOT EXISTS idx_spam_results_room ON spam_check_results(room_id);
CREATE INDEX IF NOT EXISTS idx_spam_results_sender_checked ON spam_check_results(sender, checked_ts DESC);

-- Third party rule results
CREATE INDEX IF NOT EXISTS idx_third_party_results_event_checked ON third_party_rule_results(event_id, checked_ts DESC);

-- Media callbacks
CREATE INDEX IF NOT EXISTS idx_media_callbacks_type_enabled ON media_callbacks(callback_type, is_enabled);

-- Burn after read
CREATE INDEX IF NOT EXISTS idx_burn_pending_delete_ts ON burn_after_read_pending(delete_ts) WHERE is_processed = FALSE;
CREATE INDEX IF NOT EXISTS idx_burn_log_user ON burn_after_read_log(user_id);

-- Key rotation
CREATE INDEX IF NOT EXISTS idx_key_rotation_pending_room ON key_rotation_pending(room_id);
CREATE INDEX IF NOT EXISTS idx_key_rotation_state_user ON key_rotation_state(user_id);
CREATE INDEX IF NOT EXISTS idx_megolm_key_shares_room ON megolm_key_shares(room_id);

-- Megolm session keys
CREATE INDEX IF NOT EXISTS idx_megolm_session_keys_lookup ON megolm_session_keys(user_id, session_id);
CREATE INDEX IF NOT EXISTS idx_megolm_session_keys_expiry ON megolm_session_keys(expires_at) WHERE expires_at IS NOT NULL;

-- Security
CREATE INDEX IF NOT EXISTS idx_security_events_user_id ON security_events(user_id);
CREATE INDEX IF NOT EXISTS idx_security_events_created_ts ON security_events(created_ts);
CREATE INDEX IF NOT EXISTS idx_ip_blocks_blocked_ts ON ip_blocks(blocked_ts);

-- Event reports
CREATE INDEX IF NOT EXISTS idx_event_reports_event ON event_reports(event_id);
CREATE INDEX IF NOT EXISTS idx_event_reports_room ON event_reports(room_id);
CREATE INDEX IF NOT EXISTS idx_event_reports_reporter ON event_reports(reporter_user_id);
CREATE INDEX IF NOT EXISTS idx_event_reports_status ON event_reports(status);
CREATE INDEX IF NOT EXISTS idx_event_reports_received ON event_reports(received_ts DESC);

-- Report rate limits
CREATE INDEX IF NOT EXISTS idx_report_rate_limits_user ON report_rate_limits(user_id);

-- Audit events
CREATE INDEX IF NOT EXISTS idx_audit_events_actor_created ON audit_events(actor_id, created_ts DESC);
CREATE INDEX IF NOT EXISTS idx_audit_events_resource_created ON audit_events(resource_type, resource_id, created_ts DESC);
CREATE INDEX IF NOT EXISTS idx_audit_events_request_created ON audit_events(request_id, created_ts DESC);

-- Feature flags
CREATE INDEX IF NOT EXISTS idx_feature_flags_scope_status ON feature_flags(target_scope, status, updated_ts DESC);
CREATE INDEX IF NOT EXISTS idx_feature_flags_expires_at ON feature_flags(expires_at) WHERE expires_at IS NOT NULL;
CREATE INDEX IF NOT EXISTS idx_feature_flag_targets_lookup ON feature_flag_targets(flag_key, subject_type, subject_id);

-- State groups
CREATE INDEX IF NOT EXISTS idx_state_groups_room ON state_groups(room_id);
CREATE INDEX IF NOT EXISTS idx_state_groups_event ON state_groups(event_id);
CREATE INDEX IF NOT EXISTS idx_state_group_edges_prev ON state_group_edges(prev_state_group_id);
CREATE INDEX IF NOT EXISTS idx_event_to_state_groups_sg ON event_to_state_groups(state_group_id);
CREATE INDEX IF NOT EXISTS idx_event_to_state_groups_event_id ON event_to_state_groups(event_id);
CREATE INDEX IF NOT EXISTS idx_state_group_state_eid ON state_group_state(event_id);
CREATE INDEX IF NOT EXISTS idx_state_group_state_group_type_key ON state_group_state(state_group_id, event_type, state_key);

-- Refresh token usage
CREATE INDEX IF NOT EXISTS idx_refresh_token_usage_token ON refresh_token_usage(refresh_token_id);
CREATE INDEX IF NOT EXISTS idx_refresh_token_usage_user ON refresh_token_usage(user_id);

-- Refresh token families
CREATE INDEX IF NOT EXISTS idx_refresh_token_families_user ON refresh_token_families(user_id);
CREATE INDEX IF NOT EXISTS idx_refresh_token_families_device ON refresh_token_families(device_id) WHERE device_id IS NOT NULL;

-- Refresh token rotations
CREATE INDEX IF NOT EXISTS idx_refresh_token_rotations_family ON refresh_token_rotations(family_id);

-- Registration tokens
CREATE INDEX IF NOT EXISTS idx_registration_tokens_type ON registration_tokens(token_type);
CREATE INDEX IF NOT EXISTS idx_registration_tokens_expires ON registration_tokens(expires_at) WHERE expires_at IS NOT NULL;
CREATE INDEX IF NOT EXISTS idx_registration_tokens_enabled ON registration_tokens(is_enabled) WHERE is_enabled = TRUE;
CREATE INDEX IF NOT EXISTS idx_reg_token_usage_token ON registration_token_usage(token_id);
CREATE INDEX IF NOT EXISTS idx_reg_token_usage_user ON registration_token_usage(user_id);
CREATE INDEX IF NOT EXISTS idx_registration_token_batches_created ON registration_token_batches(created_ts DESC);
CREATE INDEX IF NOT EXISTS idx_registration_token_batches_enabled_created ON registration_token_batches(created_ts DESC) WHERE is_enabled = TRUE;

-- Application services
CREATE INDEX IF NOT EXISTS idx_application_services_enabled ON application_services(is_enabled) WHERE is_enabled = TRUE;
CREATE INDEX IF NOT EXISTS idx_application_service_state_as ON application_service_state(as_id);
CREATE INDEX IF NOT EXISTS idx_application_service_transactions_as ON application_service_transactions(as_id);
CREATE INDEX IF NOT EXISTS idx_application_service_transactions_processed ON application_service_transactions(is_processed) WHERE is_processed = FALSE;
CREATE INDEX IF NOT EXISTS idx_application_service_events_as ON application_service_events(as_id);
CREATE INDEX IF NOT EXISTS idx_application_service_events_room ON application_service_events(room_id);
CREATE INDEX IF NOT EXISTS idx_application_service_user_namespaces_as ON application_service_user_namespaces(as_id);
CREATE INDEX IF NOT EXISTS idx_application_service_room_alias_namespaces_as ON application_service_room_alias_namespaces(as_id);
CREATE INDEX IF NOT EXISTS idx_application_service_room_namespaces_as ON application_service_room_namespaces(as_id);
CREATE INDEX IF NOT EXISTS idx_application_service_users_as ON application_service_users(as_id);
CREATE INDEX IF NOT EXISTS idx_secure_key_backups_user_created ON secure_key_backups(user_id, created_ts DESC);
CREATE INDEX IF NOT EXISTS idx_secure_backup_session_keys_backup ON secure_backup_session_keys(user_id, backup_id);

-- Sliding sync
CREATE UNIQUE INDEX IF NOT EXISTS idx_sliding_sync_lists_unique ON sliding_sync_lists(user_id, device_id, COALESCE(conn_id, ''), list_key);
CREATE INDEX IF NOT EXISTS idx_sliding_sync_lists_user_device ON sliding_sync_lists(user_id, device_id);
CREATE UNIQUE INDEX IF NOT EXISTS idx_sliding_sync_tokens_unique ON sliding_sync_tokens(user_id, device_id, COALESCE(conn_id, ''));
CREATE INDEX IF NOT EXISTS idx_sliding_sync_tokens_user ON sliding_sync_tokens(user_id, device_id);
CREATE UNIQUE INDEX IF NOT EXISTS idx_sliding_sync_rooms_unique ON sliding_sync_rooms(user_id, device_id, room_id, COALESCE(conn_id, ''));
CREATE INDEX IF NOT EXISTS idx_sliding_sync_rooms_user_device ON sliding_sync_rooms(user_id, device_id);
CREATE INDEX IF NOT EXISTS idx_sliding_sync_rooms_bump_stamp ON sliding_sync_rooms(bump_stamp DESC)
    WHERE bump_stamp IS NOT NULL;

-- Thread tables
CREATE INDEX IF NOT EXISTS idx_thread_roots_room ON thread_roots(room_id);
CREATE INDEX IF NOT EXISTS idx_thread_roots_root_event ON thread_roots(root_event_id);
CREATE INDEX IF NOT EXISTS idx_thread_replies_root ON thread_replies(root_event_id);
CREATE INDEX IF NOT EXISTS idx_thread_replies_room ON thread_replies(room_id);
CREATE INDEX IF NOT EXISTS idx_thread_replies_content_trgm ON thread_replies USING GIN ((content->>'body') gin_trgm_ops);
CREATE INDEX IF NOT EXISTS idx_thread_relations_relates_to ON event_relations(relates_to_event_id);
CREATE INDEX IF NOT EXISTS idx_thread_relations_type ON event_relations(relation_type);
CREATE INDEX IF NOT EXISTS idx_thread_subscriptions_user ON thread_subscriptions(user_id);
CREATE INDEX IF NOT EXISTS idx_thread_read_receipts_user_room ON thread_read_receipts(user_id, room_id);

-- Room invite signatures
CREATE UNIQUE INDEX IF NOT EXISTS uq_room_invites_invite_code ON room_invites(invite_code)
    WHERE invite_code IS NOT NULL;

-- OIDC user mapping
CREATE INDEX IF NOT EXISTS idx_oidc_user_mapping_user ON oidc_user_mapping(user_id);

-- SAML config overrides
CREATE INDEX IF NOT EXISTS idx_saml_config_overrides_updated_ts ON saml_config_overrides(updated_ts DESC);

-- Key rotation config
-- Registration captcha
CREATE INDEX IF NOT EXISTS idx_registration_captcha_captcha_id ON registration_captcha(captcha_id);

-- Cross signing keys
CREATE INDEX IF NOT EXISTS idx_cross_signing_keys_user ON cross_signing_keys(user_id);

-- Device lists stream
CREATE INDEX IF NOT EXISTS idx_device_lists_stream_stream_id ON device_lists_stream(stream_id);

-- E2EE audit log
CREATE INDEX IF NOT EXISTS idx_e2ee_audit_log_user_created ON e2ee_audit_log(user_id, created_ts DESC);

-- Room invite blocklist/allowlist
CREATE INDEX IF NOT EXISTS idx_room_invite_blocklist_room_user ON room_invite_blocklist(room_id, user_id);
CREATE INDEX IF NOT EXISTS idx_room_invite_allowlist_room_user ON room_invite_allowlist(room_id, user_id);

-- Presence
CREATE INDEX IF NOT EXISTS idx_presence_last_active_ts ON presence(last_active_ts) WHERE last_active_ts IS NOT NULL;

-- Room summaries (regular table indexes)
CREATE INDEX IF NOT EXISTS idx_room_summaries_room_id ON room_summaries(room_id);
CREATE INDEX IF NOT EXISTS idx_room_summary_members_room_user ON room_summary_members(room_id, user_id);

-- Room tags
CREATE INDEX IF NOT EXISTS idx_room_tags_user_room ON room_tags(user_id, room_id);

-- User threepids
CREATE INDEX IF NOT EXISTS idx_user_threepids_medium_address ON user_threepids(medium, address);

-- Room memberships additional
CREATE INDEX IF NOT EXISTS idx_room_memberships_room_user ON room_memberships(room_id, user_id);

-- Room retention policies
CREATE INDEX IF NOT EXISTS idx_room_retention_policies_server_default ON room_retention_policies(is_server_default) WHERE is_server_default = TRUE;

-- Room aliases
CREATE UNIQUE INDEX IF NOT EXISTS idx_room_aliases_room_alias ON room_aliases(room_alias);
CREATE INDEX IF NOT EXISTS idx_room_aliases_room_id ON room_aliases(room_id);

-- Friend list performance
CREATE INDEX IF NOT EXISTS idx_events_friend_room ON events(sender, room_id, origin_server_ts DESC)
    WHERE event_type = 'm.room.create' AND content->>'type' = 'm.friends';
CREATE INDEX IF NOT EXISTS idx_events_friend_list ON events(room_id, origin_server_ts DESC)
    WHERE event_type = 'm.friends.list' AND state_key = '';
CREATE INDEX IF NOT EXISTS idx_friend_requests_receiver_status ON friend_requests(receiver_id, status, created_ts DESC);
CREATE INDEX IF NOT EXISTS idx_friend_requests_sender_status ON friend_requests(sender_id, status, created_ts DESC);

-- pg_trgm search indexes
CREATE INDEX IF NOT EXISTS idx_users_email_trgm ON users USING GIN (email gin_trgm_ops);
CREATE INDEX IF NOT EXISTS idx_users_user_id_trgm ON users USING GIN (user_id gin_trgm_ops);
CREATE INDEX IF NOT EXISTS idx_rooms_name_trgm ON rooms USING GIN (name gin_trgm_ops);
CREATE INDEX IF NOT EXISTS idx_rooms_canonical_alias_trgm ON rooms USING GIN (canonical_alias gin_trgm_ops);
CREATE INDEX IF NOT EXISTS idx_search_index_content_trgm ON search_index USING GIN (content gin_trgm_ops);

-- Auth token schema
CREATE UNIQUE INDEX IF NOT EXISTS uq_access_tokens_token_hash ON access_tokens(token_hash);
CREATE UNIQUE INDEX IF NOT EXISTS uq_refresh_tokens_token_hash ON refresh_tokens(token_hash);
CREATE UNIQUE INDEX IF NOT EXISTS uq_token_blacklist_token_hash ON token_blacklist(token_hash);
CREATE INDEX IF NOT EXISTS idx_access_tokens_token_hash ON access_tokens(token_hash);
CREATE INDEX IF NOT EXISTS idx_access_tokens_valid ON access_tokens(is_revoked) WHERE is_revoked = FALSE;
CREATE INDEX IF NOT EXISTS idx_refresh_tokens_revoked ON refresh_tokens(is_revoked);
CREATE INDEX IF NOT EXISTS idx_token_blacklist_hash ON token_blacklist(token_hash);
CREATE INDEX IF NOT EXISTS idx_access_tokens_device_id ON access_tokens(device_id) WHERE device_id IS NOT NULL;
CREATE INDEX IF NOT EXISTS idx_refresh_tokens_device_id ON refresh_tokens(device_id) WHERE device_id IS NOT NULL;
CREATE INDEX IF NOT EXISTS idx_token_blacklist_user_id ON token_blacklist(user_id) WHERE user_id IS NOT NULL;

-- Users additional
CREATE INDEX IF NOT EXISTS idx_users_lower_email ON users(LOWER(COALESCE(email, '')));
CREATE INDEX IF NOT EXISTS idx_users_created_ts ON users(created_ts DESC);

-- Media quota
CREATE INDEX IF NOT EXISTS idx_media_quota_alerts_user ON media_quota_alerts(user_id);
CREATE INDEX IF NOT EXISTS idx_media_quota_alerts_is_read ON media_quota_alerts(is_read) WHERE is_read = FALSE;

-- Dehydrated devices
CREATE INDEX IF NOT EXISTS idx_dehydrated_devices_user ON dehydrated_devices(user_id);

-- Delayed events
CREATE INDEX IF NOT EXISTS idx_delayed_events_room ON delayed_events(room_id);

-- Rendezvous sessions
CREATE INDEX IF NOT EXISTS idx_rendezvous_session_expires ON rendezvous_session(expires_at) WHERE expires_at IS NOT NULL;

-- QR login
CREATE INDEX IF NOT EXISTS idx_qr_login_transactions_expires ON qr_login_transactions(expires_at) WHERE expires_at IS NOT NULL;

-- Widgets
CREATE INDEX IF NOT EXISTS idx_widgets_room ON widgets(room_id);

-- Server notifications
CREATE INDEX IF NOT EXISTS idx_server_notifications_target_users ON server_notifications USING GIN (target_user_ids jsonb_ops);

-- Beacon
CREATE INDEX IF NOT EXISTS idx_beacon_info_room ON beacon_info(room_id);

-- Call/MATRIXRTC
CREATE INDEX IF NOT EXISTS idx_call_sessions_room ON call_sessions(room_id);

-- Migration audit
CREATE INDEX IF NOT EXISTS idx_migration_audit_version ON migration_audit(version);
CREATE INDEX IF NOT EXISTS idx_migration_audit_executed_at ON migration_audit(executed_at);
CREATE INDEX IF NOT EXISTS idx_migration_audit_status ON migration_audit(status);

-- Replication positions
CREATE INDEX IF NOT EXISTS idx_replication_positions_worker ON replication_positions(worker_id);

-- Stream tables
CREATE INDEX IF NOT EXISTS idx_presence_stream_user ON presence_stream(user_id);
CREATE INDEX IF NOT EXISTS idx_presence_stream_stream ON presence_stream(stream_id);
CREATE INDEX IF NOT EXISTS idx_typing_stream_room ON typing_stream(room_id);
CREATE INDEX IF NOT EXISTS idx_typing_stream_user ON typing_stream(user_id);
CREATE INDEX IF NOT EXISTS idx_typing_stream_active ON typing_stream(room_id, is_typing) WHERE is_typing = TRUE;

-- Room stats
CREATE INDEX IF NOT EXISTS idx_room_stats_joined ON room_stats_current(joined_members DESC);
CREATE INDEX IF NOT EXISTS idx_room_stats_local ON room_stats_current(local_users_in_room DESC);

-- Threepid validation
CREATE INDEX IF NOT EXISTS idx_threepid_session_token_v8 ON threepid_validation_session(token);
CREATE INDEX IF NOT EXISTS idx_threepid_session_address_v8 ON threepid_validation_session(medium, address);
CREATE INDEX IF NOT EXISTS idx_threepid_session_expires_v8 ON threepid_validation_session(expires_at) WHERE is_validated = FALSE;

-- ============================================================================
-- Views
-- ============================================================================

-- Active workers view
CREATE OR REPLACE VIEW active_workers AS
SELECT id, worker_id, worker_name, worker_type, host, port, status,
       last_heartbeat_ts, started_ts, stopped_ts, config, metadata, version, is_enabled
FROM workers
WHERE status = 'running' OR status = 'starting';

-- Worker type statistics view
-- Note: worker_load_stats and worker_connections tables were dropped in v8;
-- avg_cpu_usage and total_connections are always NULL/0.
CREATE OR REPLACE VIEW worker_type_statistics AS
SELECT
    w.worker_type,
    COUNT(*)::BIGINT AS total_count,
    COUNT(*) FILTER (WHERE w.status = 'running')::BIGINT AS running_count,
    COUNT(*) FILTER (WHERE w.status = 'starting')::BIGINT AS starting_count,
    COUNT(*) FILTER (WHERE w.status = 'stopping')::BIGINT AS stopping_count,
    COUNT(*) FILTER (WHERE w.status = 'stopped')::BIGINT AS stopped_count,
    NULL::DOUBLE PRECISION AS avg_cpu_usage,
    NULL::DOUBLE PRECISION AS avg_memory_usage,
    0::BIGINT AS total_connections
FROM workers w
GROUP BY w.worker_type;

-- ============================================================================
-- Materialized Views
-- ============================================================================

-- rooms_summaries: caches per-room member counts, latest activity, name/topic/avatar
CREATE MATERIALIZED VIEW IF NOT EXISTS rooms_summaries_mv AS
SELECT
    r.room_id,
    r.creator,
    r.room_version,
    r.join_rules,
    r.is_public,
    r.history_visibility,
    r.created_ts,
    r.last_activity_ts,
    COALESCE(member_count.cnt, 0)                       AS joined_members,
    COALESCE(member_count.cnt, 0) +
        COALESCE(invite_count.cnt, 0)                   AS total_members,
    COALESCE(invite_count.cnt, 0)                       AS invited_members,
    latest_event.event_id                               AS last_event_id,
    latest_event.event_type                             AS last_event_type,
    latest_event.sender                                 AS last_event_sender,
    latest_event.origin_server_ts                       AS last_event_ts,
    room_name.content->>'name'                          AS room_name,
    room_topic.content->>'topic'                        AS room_topic,
    room_avatar.content->>'url'                         AS room_avatar_url,
    room_canonical_alias.content->>'alias'              AS canonical_alias
FROM rooms r
LEFT JOIN LATERAL (
    SELECT COUNT(*) AS cnt
    FROM room_memberships
    WHERE room_id = r.room_id AND membership = 'join'
) member_count ON true
LEFT JOIN LATERAL (
    SELECT COUNT(*) AS cnt
    FROM room_memberships
    WHERE room_id = r.room_id AND membership = 'invite'
) invite_count ON true
LEFT JOIN LATERAL (
    SELECT event_id, event_type, sender, origin_server_ts
    FROM events
    WHERE room_id = r.room_id
      AND is_redacted = false
    ORDER BY origin_server_ts DESC
    LIMIT 1
) latest_event ON true
LEFT JOIN LATERAL (
    SELECT content
    FROM events
    WHERE room_id = r.room_id
      AND event_type = 'm.room.name'
      AND is_redacted = false
    ORDER BY origin_server_ts DESC
    LIMIT 1
) room_name ON true
LEFT JOIN LATERAL (
    SELECT content
    FROM events
    WHERE room_id = r.room_id
      AND event_type = 'm.room.topic'
      AND is_redacted = false
    ORDER BY origin_server_ts DESC
    LIMIT 1
) room_topic ON true
LEFT JOIN LATERAL (
    SELECT content
    FROM events
    WHERE room_id = r.room_id
      AND event_type = 'm.room.avatar'
      AND is_redacted = false
    ORDER BY origin_server_ts DESC
    LIMIT 1
) room_avatar ON true
LEFT JOIN LATERAL (
    SELECT content
    FROM events
    WHERE room_id = r.room_id
      AND event_type = 'm.room.canonical_alias'
      AND is_redacted = false
    ORDER BY origin_server_ts DESC
    LIMIT 1
) room_canonical_alias ON true;

-- Materialized view indexes
CREATE UNIQUE INDEX IF NOT EXISTS idx_rooms_summaries_mv_room_id
    ON rooms_summaries_mv(room_id);

CREATE INDEX IF NOT EXISTS idx_rooms_summaries_mv_public_activity
    ON rooms_summaries_mv(is_public, joined_members DESC, last_activity_ts DESC)
    WHERE is_public = true;

CREATE INDEX IF NOT EXISTS idx_rooms_summaries_mv_creator
    ON rooms_summaries_mv(creator, created_ts DESC);

CREATE INDEX IF NOT EXISTS idx_rooms_summaries_mv_members
    ON rooms_summaries_mv(joined_members DESC, last_activity_ts DESC);

-- pg_cron refresh schedule (only if pg_cron extension is available)
DO $$
BEGIN
    IF EXISTS (SELECT 1 FROM pg_extension WHERE extname = 'pg_cron') THEN
        PERFORM cron.schedule(
            'refresh-rooms-summaries',
            '*/5 * * * *',
            'REFRESH MATERIALIZED VIEW CONCURRENTLY rooms_summaries_mv'
        );
    END IF;
END $$;

-- public_room_directory: filters rooms_summaries_mv for public rooms
CREATE MATERIALIZED VIEW IF NOT EXISTS public_room_directory AS
SELECT
    room_id,
    room_name,
    room_topic,
    room_avatar_url,
    canonical_alias,
    joined_members,
    total_members,
    last_event_ts,
    room_version,
    join_rules,
    history_visibility,
    created_ts
FROM rooms_summaries_mv
WHERE is_public = true
  AND join_rules != 'knock';

CREATE UNIQUE INDEX IF NOT EXISTS idx_public_room_directory_room_id
    ON public_room_directory(room_id);

CREATE INDEX IF NOT EXISTS idx_public_room_directory_members
    ON public_room_directory(joined_members DESC, last_event_ts DESC);

DO $$
BEGIN
    IF EXISTS (SELECT 1 FROM pg_extension WHERE extname = 'pg_cron') THEN
        PERFORM cron.schedule(
            'refresh-public-room-directory',
            '*/5 * * * *',
            'REFRESH MATERIALIZED VIEW CONCURRENTLY public_room_directory'
        );
    END IF;
END $$;

-- ============================================================================
-- Foreign Keys
-- ============================================================================

DO $$
BEGIN
    IF NOT EXISTS (SELECT 1 FROM pg_constraint WHERE conname = 'fk_devices_user_id' AND conrelid = 'devices'::regclass) THEN
        ALTER TABLE devices ADD CONSTRAINT fk_devices_user_id
            FOREIGN KEY (user_id) REFERENCES users(user_id) ON DELETE CASCADE;
    END IF;

    IF NOT EXISTS (SELECT 1 FROM pg_constraint WHERE conname = 'fk_access_tokens_user_id' AND conrelid = 'access_tokens'::regclass) THEN
        ALTER TABLE access_tokens ADD CONSTRAINT fk_access_tokens_user_id
            FOREIGN KEY (user_id) REFERENCES users(user_id) ON DELETE CASCADE;
    END IF;

    IF NOT EXISTS (SELECT 1 FROM pg_constraint WHERE conname = 'fk_access_tokens_device' AND conrelid = 'access_tokens'::regclass) THEN
        ALTER TABLE access_tokens ADD CONSTRAINT fk_access_tokens_device
            FOREIGN KEY (device_id) REFERENCES devices(device_id) ON DELETE SET NULL NOT VALID;
    END IF;

    IF NOT EXISTS (SELECT 1 FROM pg_constraint WHERE conname = 'fk_refresh_tokens_user_id' AND conrelid = 'refresh_tokens'::regclass) THEN
        ALTER TABLE refresh_tokens ADD CONSTRAINT fk_refresh_tokens_user_id
            FOREIGN KEY (user_id) REFERENCES users(user_id) ON DELETE CASCADE;
    END IF;

    IF NOT EXISTS (SELECT 1 FROM pg_constraint WHERE conname = 'fk_refresh_tokens_device' AND conrelid = 'refresh_tokens'::regclass) THEN
        ALTER TABLE refresh_tokens ADD CONSTRAINT fk_refresh_tokens_device
            FOREIGN KEY (device_id) REFERENCES devices(device_id) ON DELETE SET NULL NOT VALID;
    END IF;

    IF NOT EXISTS (SELECT 1 FROM pg_constraint WHERE conname = 'fk_events_room_id' AND conrelid = 'events'::regclass) THEN
        ALTER TABLE events ADD CONSTRAINT fk_events_room_id
            FOREIGN KEY (room_id) REFERENCES rooms(room_id) ON DELETE CASCADE;
    END IF;

    IF NOT EXISTS (SELECT 1 FROM pg_constraint WHERE conname = 'fk_room_memberships_room_id' AND conrelid = 'room_memberships'::regclass) THEN
        ALTER TABLE room_memberships ADD CONSTRAINT fk_room_memberships_room_id
            FOREIGN KEY (room_id) REFERENCES rooms(room_id) ON DELETE CASCADE;
    END IF;

    IF NOT EXISTS (SELECT 1 FROM pg_constraint WHERE conname = 'fk_room_memberships_user_id' AND conrelid = 'room_memberships'::regclass) THEN
        ALTER TABLE room_memberships ADD CONSTRAINT fk_room_memberships_user_id
            FOREIGN KEY (user_id) REFERENCES users(user_id) ON DELETE CASCADE;
    END IF;

    IF NOT EXISTS (SELECT 1 FROM pg_constraint WHERE conname = 'fk_device_keys_user_id' AND conrelid = 'device_keys'::regclass) THEN
        ALTER TABLE device_keys ADD CONSTRAINT fk_device_keys_user_id
            FOREIGN KEY (user_id) REFERENCES users(user_id) ON DELETE CASCADE;
    END IF;

    IF NOT EXISTS (SELECT 1 FROM pg_constraint WHERE conname = 'fk_cross_signing_keys_user_id' AND conrelid = 'cross_signing_keys'::regclass) THEN
        ALTER TABLE cross_signing_keys ADD CONSTRAINT fk_cross_signing_keys_user_id
            FOREIGN KEY (user_id) REFERENCES users(user_id) ON DELETE CASCADE;
    END IF;

    IF NOT EXISTS (SELECT 1 FROM pg_constraint WHERE conname = 'fk_push_queue_user_id' AND conrelid = 'push_notification_queue'::regclass) THEN
        ALTER TABLE push_notification_queue ADD CONSTRAINT fk_push_queue_user_id
            FOREIGN KEY (user_id) REFERENCES users(user_id) ON DELETE CASCADE;
    END IF;

    IF NOT EXISTS (SELECT 1 FROM pg_constraint WHERE conname = 'fk_token_blacklist_user' AND conrelid = 'token_blacklist'::regclass) THEN
        ALTER TABLE token_blacklist ADD CONSTRAINT fk_token_blacklist_user
            FOREIGN KEY (user_id) REFERENCES users(user_id) ON DELETE SET NULL NOT VALID;
    END IF;

    IF NOT EXISTS (SELECT 1 FROM pg_constraint WHERE conname = 'fk_registration_token_usage_user' AND conrelid = 'registration_token_usage'::regclass) THEN
        ALTER TABLE registration_token_usage ADD CONSTRAINT fk_registration_token_usage_user
            FOREIGN KEY (user_id) REFERENCES users(user_id) ON DELETE CASCADE NOT VALID;
    END IF;

    IF NOT EXISTS (SELECT 1 FROM pg_constraint WHERE conname = 'fk_report_rate_limits_user' AND conrelid = 'report_rate_limits'::regclass) THEN
        ALTER TABLE report_rate_limits ADD CONSTRAINT fk_report_rate_limits_user
            FOREIGN KEY (user_id) REFERENCES users(user_id) ON DELETE CASCADE NOT VALID;
    END IF;

    IF NOT EXISTS (SELECT 1 FROM pg_constraint WHERE conname = 'fk_refresh_token_usage_token' AND conrelid = 'refresh_token_usage'::regclass) THEN
        ALTER TABLE refresh_token_usage ADD CONSTRAINT fk_refresh_token_usage_token
            FOREIGN KEY (refresh_token_id) REFERENCES refresh_tokens(id) ON DELETE CASCADE NOT VALID;
    END IF;

    IF NOT EXISTS (SELECT 1 FROM pg_constraint WHERE conname = 'fk_refresh_token_usage_user' AND conrelid = 'refresh_token_usage'::regclass) THEN
        ALTER TABLE refresh_token_usage ADD CONSTRAINT fk_refresh_token_usage_user
            FOREIGN KEY (user_id) REFERENCES users(user_id) ON DELETE CASCADE NOT VALID;
    END IF;

    IF NOT EXISTS (SELECT 1 FROM pg_constraint WHERE conname = 'fk_refresh_token_families_user' AND conrelid = 'refresh_token_families'::regclass) THEN
        ALTER TABLE refresh_token_families ADD CONSTRAINT fk_refresh_token_families_user
            FOREIGN KEY (user_id) REFERENCES users(user_id) ON DELETE CASCADE NOT VALID;
    END IF;

    IF NOT EXISTS (SELECT 1 FROM pg_constraint WHERE conname = 'fk_refresh_token_families_device' AND conrelid = 'refresh_token_families'::regclass) THEN
        ALTER TABLE refresh_token_families ADD CONSTRAINT fk_refresh_token_families_device
            FOREIGN KEY (device_id) REFERENCES devices(device_id) ON DELETE SET NULL NOT VALID;
    END IF;

    IF NOT EXISTS (SELECT 1 FROM pg_constraint WHERE conname = 'fk_refresh_token_rotations_family' AND conrelid = 'refresh_token_rotations'::regclass) THEN
        ALTER TABLE refresh_token_rotations ADD CONSTRAINT fk_refresh_token_rotations_family
            FOREIGN KEY (family_id) REFERENCES refresh_token_families(family_id) ON DELETE CASCADE NOT VALID;
    END IF;

    IF NOT EXISTS (SELECT 1 FROM pg_constraint WHERE conname = 'fk_module_execution_logs_module' AND conrelid = 'module_execution_logs'::regclass) THEN
        ALTER TABLE module_execution_logs ADD CONSTRAINT fk_module_execution_logs_module
            FOREIGN KEY (module_id) REFERENCES modules(id) ON DELETE CASCADE;
    END IF;

    IF NOT EXISTS (SELECT 1 FROM pg_constraint WHERE conname = 'fk_room_retention_policies_room' AND conrelid = 'room_retention_policies'::regclass) THEN
        ALTER TABLE room_retention_policies ADD CONSTRAINT fk_room_retention_policies_room
            FOREIGN KEY (room_id) REFERENCES rooms(room_id) ON DELETE CASCADE;
    END IF;

    IF NOT EXISTS (SELECT 1 FROM pg_constraint WHERE conname = 'fk_replication_positions_worker' AND conrelid = 'replication_positions'::regclass) THEN
        ALTER TABLE replication_positions ADD CONSTRAINT fk_replication_positions_worker
            FOREIGN KEY (worker_id) REFERENCES workers(worker_id) ON DELETE CASCADE;
    END IF;

    IF NOT EXISTS (SELECT 1 FROM pg_constraint WHERE conname = 'fk_presence_stream_user' AND conrelid = 'presence_stream'::regclass) THEN
        ALTER TABLE presence_stream ADD CONSTRAINT fk_presence_stream_user
            FOREIGN KEY (user_id) REFERENCES users(user_id) ON DELETE CASCADE;
    END IF;

    IF NOT EXISTS (SELECT 1 FROM pg_constraint WHERE conname = 'fk_typing_stream_room' AND conrelid = 'typing_stream'::regclass) THEN
        ALTER TABLE typing_stream ADD CONSTRAINT fk_typing_stream_room
            FOREIGN KEY (room_id) REFERENCES rooms(room_id) ON DELETE CASCADE;
    END IF;

    IF NOT EXISTS (SELECT 1 FROM pg_constraint WHERE conname = 'fk_typing_stream_user' AND conrelid = 'typing_stream'::regclass) THEN
        ALTER TABLE typing_stream ADD CONSTRAINT fk_typing_stream_user
            FOREIGN KEY (user_id) REFERENCES users(user_id) ON DELETE CASCADE;
    END IF;

    IF NOT EXISTS (SELECT 1 FROM pg_constraint WHERE conname = 'fk_device_lists_outbound_pokes_user' AND conrelid = 'device_lists_outbound_pokes'::regclass) THEN
        ALTER TABLE device_lists_outbound_pokes ADD CONSTRAINT fk_device_lists_outbound_pokes_user
            FOREIGN KEY (user_id) REFERENCES users(user_id) ON DELETE CASCADE;
    END IF;

    IF NOT EXISTS (SELECT 1 FROM pg_constraint WHERE conname = 'fk_room_stats_current_room' AND conrelid = 'room_stats_current'::regclass) THEN
        ALTER TABLE room_stats_current ADD CONSTRAINT fk_room_stats_current_room
            FOREIGN KEY (room_id) REFERENCES rooms(room_id) ON DELETE CASCADE;
    END IF;
END $$;

-- ============================================================================
-- Primary Keys (missing from original schema)
-- ============================================================================

-- typing composite PK
DO $$
BEGIN
    IF NOT EXISTS (
        SELECT 1 FROM information_schema.table_constraints
        WHERE table_schema = 'public' AND table_name = 'typing' AND constraint_name = 'pk_typing'
    ) THEN
        ALTER TABLE typing ADD CONSTRAINT pk_typing PRIMARY KEY (user_id, room_id);
    END IF;
END $$;

-- presence_subscriptions composite PK
DO $$
BEGIN
    IF NOT EXISTS (
        SELECT 1 FROM information_schema.table_constraints
        WHERE table_schema = 'public' AND table_name = 'presence_subscriptions' AND constraint_name = 'pk_presence_subscriptions'
    ) THEN
        ALTER TABLE presence_subscriptions ADD CONSTRAINT pk_presence_subscriptions PRIMARY KEY (subscriber_id, target_id);
    END IF;
END $$;

-- ============================================================================
-- Triggers
-- ============================================================================

-- Auto-update updated_ts for openclaw_connections
DO $$ BEGIN
IF NOT EXISTS (SELECT 1 FROM pg_trigger WHERE tgname = 'update_openclaw_connections_updated_ts') THEN
    CREATE TRIGGER update_openclaw_connections_updated_ts
        BEFORE UPDATE ON openclaw_connections
        FOR EACH ROW
        EXECUTE FUNCTION update_updated_ts_column();
END IF;
END $$;

-- Auto-update updated_ts for ai_conversations
DO $$ BEGIN
IF NOT EXISTS (SELECT 1 FROM pg_trigger WHERE tgname = 'update_ai_conversations_updated_ts') THEN
    CREATE TRIGGER update_ai_conversations_updated_ts
        BEFORE UPDATE ON ai_conversations
        FOR EACH ROW
        EXECUTE FUNCTION update_updated_ts_column();
END IF;
END $$;

-- Auto-update updated_ts for ai_chat_roles
DO $$ BEGIN
IF NOT EXISTS (SELECT 1 FROM pg_trigger WHERE tgname = 'update_ai_chat_roles_updated_ts') THEN
    CREATE TRIGGER update_ai_chat_roles_updated_ts
        BEFORE UPDATE ON ai_chat_roles
        FOR EACH ROW
        EXECUTE FUNCTION update_updated_ts_column();
END IF;
END $$;

-- ============================================================================
-- Default Data
-- ============================================================================

-- Default admin user (development only - change password in production!)
INSERT INTO users (user_id, username, password_hash, is_admin, is_guest, created_ts, displayname)
VALUES (
    '@admin:localhost',
    'admin',
    '$argon2id$v=19$m=65536,t=3,p=1$VGVzdFNhbHRGb3JBZG1pbg$K7G8H5J3M2N9P4Q6R8S0T2U4V6W8X0Y2Z4A6B8C0D2E4F6G8H0J2K4L6M8N0P2Q4',
    TRUE,
    FALSE,
    (EXTRACT(EPOCH FROM NOW()) * 1000)::BIGINT,
    'Administrator'
) ON CONFLICT (user_id) DO NOTHING;

-- Default sync stream types
INSERT INTO sync_stream_id (stream_type, last_id, updated_ts)
VALUES
    ('events', 0, (EXTRACT(EPOCH FROM NOW()) * 1000)::BIGINT),
    ('presence', 0, (EXTRACT(EPOCH FROM NOW()) * 1000)::BIGINT),
    ('receipts', 0, (EXTRACT(EPOCH FROM NOW()) * 1000)::BIGINT),
    ('account_data', 0, (EXTRACT(EPOCH FROM NOW()) * 1000)::BIGINT)
ON CONFLICT (stream_type) DO NOTHING;

-- Default server retention policy
INSERT INTO server_retention_policy (id, max_lifetime, min_lifetime, is_expire_on_clients, created_ts, updated_ts)
VALUES (1, NULL, 0, FALSE, 0, 0)
ON CONFLICT (id) DO NOTHING;

-- Default media quota (10TB storage, 1GB max file, 1M files, 80% alert threshold)
INSERT INTO server_media_quota (
    id, max_storage_bytes, max_file_size_bytes, max_files_count,
    current_storage_bytes, current_files_count, alert_threshold_percent, updated_ts
)
SELECT
    1, 10995116277760, 1073741824, 1000000, 0, 0, 80,
    (EXTRACT(EPOCH FROM NOW())::BIGINT * 1000)
WHERE NOT EXISTS (SELECT 1 FROM server_media_quota WHERE id = 1);

-- Force admin to change password on first login
UPDATE users SET must_change_password = TRUE WHERE username = 'admin';

-- ============================================================================
-- Comments
-- ============================================================================

-- Migration audit
COMMENT ON TABLE migration_audit IS 'Records metrics for each database migration execution, used for performance monitoring and troubleshooting';
COMMENT ON COLUMN migration_audit.duration_ms IS 'Migration execution time in milliseconds';
COMMENT ON COLUMN migration_audit.rows_affected IS 'Number of rows affected';
COMMENT ON COLUMN migration_audit.status IS 'Execution status: SUCCESS, FAILED, ROLLED_BACK';
COMMENT ON COLUMN migration_audit.checksum IS 'SHA-256 checksum of the migration script';
COMMENT ON COLUMN migration_audit.migration_file IS 'Migration script filename';

-- Event relations
COMMENT ON TABLE event_relations IS 'Stores Matrix event relations (annotations, references, replacements, threads)';
COMMENT ON COLUMN event_relations.event_id IS 'The event that is relating to another event';
COMMENT ON COLUMN event_relations.relates_to_event_id IS 'The event_id being related to';
COMMENT ON COLUMN event_relations.relation_type IS 'Relation type: m.annotation (reactions), m.reference, m.replace (edits), m.thread';

-- OpenClaw connections
COMMENT ON TABLE openclaw_connections IS 'OpenClaw connection configuration table';
COMMENT ON COLUMN openclaw_connections.user_id IS 'User ID';
COMMENT ON COLUMN openclaw_connections.name IS 'Connection name';
COMMENT ON COLUMN openclaw_connections.provider IS 'Provider: openai, anthropic, ollama, openclaw, custom';
COMMENT ON COLUMN openclaw_connections.base_url IS 'API endpoint URL';
COMMENT ON COLUMN openclaw_connections.encrypted_api_key IS 'Encrypted API key';
COMMENT ON COLUMN openclaw_connections.config IS 'Other configuration (temperature, maxTokens, etc.)';
COMMENT ON COLUMN openclaw_connections.is_default IS 'Whether this is the default connection';
COMMENT ON COLUMN openclaw_connections.is_active IS 'Whether the connection is active';

-- AI conversations
COMMENT ON TABLE ai_conversations IS 'AI conversation records table';
COMMENT ON COLUMN ai_conversations.user_id IS 'User ID';
COMMENT ON COLUMN ai_conversations.connection_id IS 'Associated OpenClaw connection';
COMMENT ON COLUMN ai_conversations.title IS 'Conversation title';
COMMENT ON COLUMN ai_conversations.model_id IS 'Model ID used';
COMMENT ON COLUMN ai_conversations.system_prompt IS 'System prompt';
COMMENT ON COLUMN ai_conversations.temperature IS 'Temperature parameter';
COMMENT ON COLUMN ai_conversations.max_tokens IS 'Maximum token count';
COMMENT ON COLUMN ai_conversations.is_pinned IS 'Whether pinned';
COMMENT ON COLUMN ai_conversations.metadata IS 'Other metadata';

-- AI messages
COMMENT ON TABLE ai_messages IS 'AI message records table';
COMMENT ON COLUMN ai_messages.conversation_id IS 'Associated conversation ID';
COMMENT ON COLUMN ai_messages.role IS 'Message role: user, assistant, system, tool';
COMMENT ON COLUMN ai_messages.content IS 'Message content';
COMMENT ON COLUMN ai_messages.token_count IS 'Token count';
COMMENT ON COLUMN ai_messages.tool_calls IS 'Function calling tool call records';
COMMENT ON COLUMN ai_messages.tool_call_id IS 'Tool call ID (for correlating tool responses)';
COMMENT ON COLUMN ai_messages.metadata IS 'Other metadata';

-- AI generations
COMMENT ON TABLE ai_generations IS 'AI generation records table (image/video/audio)';
COMMENT ON COLUMN ai_generations.user_id IS 'User ID';
COMMENT ON COLUMN ai_generations.conversation_id IS 'Associated conversation ID';
COMMENT ON COLUMN ai_generations.type IS 'Generation type: image, video, audio';
COMMENT ON COLUMN ai_generations.prompt IS 'Prompt';
COMMENT ON COLUMN ai_generations.result_url IS 'Result URL';
COMMENT ON COLUMN ai_generations.result_mxc IS 'Matrix MXC URL';
COMMENT ON COLUMN ai_generations.status IS 'Status: pending, processing, completed, failed';
COMMENT ON COLUMN ai_generations.error_message IS 'Error message';
COMMENT ON COLUMN ai_generations.metadata IS 'Other metadata (dimensions, duration, etc.)';

-- AI chat roles
COMMENT ON TABLE ai_chat_roles IS 'AI chat roles table';
COMMENT ON COLUMN ai_chat_roles.user_id IS 'User ID';
COMMENT ON COLUMN ai_chat_roles.name IS 'Role name';
COMMENT ON COLUMN ai_chat_roles.description IS 'Role description';
COMMENT ON COLUMN ai_chat_roles.system_message IS 'System prompt';
COMMENT ON COLUMN ai_chat_roles.model_id IS 'Default model ID';
COMMENT ON COLUMN ai_chat_roles.avatar_url IS 'Avatar URL';
COMMENT ON COLUMN ai_chat_roles.category IS 'Category';
COMMENT ON COLUMN ai_chat_roles.temperature IS 'Default temperature parameter';
COMMENT ON COLUMN ai_chat_roles.max_tokens IS 'Default maximum token count';
COMMENT ON COLUMN ai_chat_roles.is_public IS 'Whether public';

-- Federation servers
COMMENT ON COLUMN federation_servers.status IS 'Federation admission status: pending, active, rejected';
COMMENT ON COLUMN federation_servers.updated_ts IS 'Timestamp of last status update in milliseconds';

-- Room invites (Sprint 4 signature binding)
COMMENT ON COLUMN room_invites.signature IS 'HMAC-SHA256(secret, "v1|" || invite_code || "|" || room_id || "|" || inviter_user_id || "|" || expires_at || "|" || created_ts), hex-encoded. NULL = legacy token issued before Sprint 4.';
COMMENT ON COLUMN room_invites.signed_version IS '0 = legacy (no signature), 1 = HMAC-SHA256 binding to (room, inviter, exp, created_ts).';

-- Cross signing keys (Sprint 5 HMAC binding)
COMMENT ON COLUMN cross_signing_keys.binding_token IS 'HMAC-SHA256(secret, "v1|" || user_id || "|" || device_id || "|" || key_type || "|" || added_ts), hex-encoded. NULL for pre-Sprint 5 rows.';
COMMENT ON COLUMN cross_signing_keys.binding_ts IS 'Server-side timestamp at which the binding was computed (epoch ms).';

-- ============================================================================
-- Completion Notice
-- ============================================================================

-- ============================================================================
-- URL Preview Cache (MSC4452)
-- ============================================================================

CREATE TABLE IF NOT EXISTS url_preview_cache (
    url TEXT NOT NULL PRIMARY KEY,
    title TEXT,
    description TEXT,
    og_title TEXT,
    og_image TEXT,
    og_image_width INTEGER,
    og_image_height INTEGER,
    og_site_name TEXT,
    og_type TEXT,
    created_ts BIGINT NOT NULL,
    expires_ts BIGINT NOT NULL
);

CREATE INDEX IF NOT EXISTS idx_url_preview_cache_expires ON url_preview_cache(expires_ts);

-- ============================================================================
-- Finalization notice
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
    RAISE NOTICE 'synapse-rust Unified Database Schema v8.0.0 initialized';
    RAISE NOTICE 'Completed at: %', NOW();
    RAISE NOTICE '----------------------------------------';
    RAISE NOTICE 'Table count: %', table_count;
    RAISE NOTICE 'Index count: %', index_count;
    RAISE NOTICE '----------------------------------------';
    RAISE NOTICE 'Key changes from v7:';
    RAISE NOTICE '  - Removed 19 dropped redundant tables';
    RAISE NOTICE '  - Removed duplicate table definitions';
    RAISE NOTICE '  - Inlined all ALTER TABLE changes';
    RAISE NOTICE '  - voice_usage_stats uses 20260517 version';
    RAISE NOTICE '  - user_privacy_settings merged visibility columns';
    RAISE NOTICE '  - spam_check_results/third_party_rule_results removed redundant columns (m-26)';
    RAISE NOTICE '  - CAS tables use _at suffix convention';
    RAISE NOTICE '  - Added burn_after_read, key_rotation, megolm_session_keys tables';
    RAISE NOTICE '  - Consolidated all indexes, views, FKs, triggers, defaults';
    RAISE NOTICE '  - Boolean fields use is_ prefix';
    RAISE NOTICE '  - NOT NULL timestamps use _ts suffix';
    RAISE NOTICE '  - Nullable timestamps use _at suffix';
    RAISE NOTICE '----------------------------------------';
    RAISE NOTICE 'Default admin: admin';
    RAISE NOTICE '(Set password via ADMIN_PASSWORD env var!)';
    RAISE NOTICE '==========================================';
END $$;

-- ============================================================================
-- OIDC 动态客户端注册 (RFC7591 / MSC3861)
-- ============================================================================
CREATE TABLE IF NOT EXISTS oauth_clients (
    client_id TEXT NOT NULL PRIMARY KEY,
    client_secret TEXT NOT NULL,
    client_name TEXT,
    redirect_uris JSONB NOT NULL DEFAULT '[]',
    grant_types JSONB NOT NULL DEFAULT '["authorization_code"]',
    response_types JSONB NOT NULL DEFAULT '["code"]',
    scope TEXT NOT NULL DEFAULT 'openid profile email',
    created_ts BIGINT NOT NULL,
    is_confidential BOOLEAN NOT NULL DEFAULT TRUE
);

CREATE INDEX IF NOT EXISTS idx_oauth_clients_created ON oauth_clients(created_ts DESC);

-- ============================================================================
-- OIDC 会话持久化 (MSC3861)
-- ============================================================================

-- OIDC 授权会话（PKCE state + 授权码）
CREATE TABLE IF NOT EXISTS oidc_auth_sessions (
    id BIGSERIAL PRIMARY KEY,
    session_key TEXT NOT NULL UNIQUE,
    session_type TEXT NOT NULL,          -- 'pkce_state' 或 'auth_code'
    client_id TEXT NOT NULL DEFAULT '',
    redirect_uri TEXT NOT NULL DEFAULT '',
    scope TEXT NOT NULL DEFAULT '',
    state TEXT NOT NULL DEFAULT '',
    nonce TEXT,
    code_verifier TEXT,
    code_challenge TEXT,
    code_challenge_method TEXT,
    user_id TEXT,
    consent_given BOOLEAN NOT NULL DEFAULT FALSE,
    created_ts BIGINT NOT NULL,
    expires_at BIGINT NOT NULL
);

CREATE INDEX IF NOT EXISTS idx_oidc_auth_sessions_expires ON oidc_auth_sessions(expires_at);
CREATE INDEX IF NOT EXISTS idx_oidc_auth_sessions_key ON oidc_auth_sessions(session_key);

-- OIDC Refresh Token（内置 Provider）
CREATE TABLE IF NOT EXISTS oidc_refresh_tokens (
    id BIGSERIAL PRIMARY KEY,
    token_hash TEXT NOT NULL UNIQUE,
    user_id TEXT NOT NULL,
    client_id TEXT NOT NULL,
    scope TEXT NOT NULL DEFAULT '',
    created_ts BIGINT NOT NULL,
    expires_at BIGINT,
    is_revoked BOOLEAN NOT NULL DEFAULT FALSE,
    revoked_at BIGINT
);

CREATE INDEX IF NOT EXISTS idx_oidc_refresh_tokens_user ON oidc_refresh_tokens(user_id, is_revoked);

-- OIDC 同意会话（MSC3861）
CREATE TABLE IF NOT EXISTS oidc_consent_sessions (
    id BIGSERIAL PRIMARY KEY,
    session_id TEXT NOT NULL UNIQUE,
    client_id TEXT NOT NULL,
    client_name TEXT,
    redirect_uri TEXT NOT NULL,
    scope TEXT NOT NULL DEFAULT '',
    state TEXT NOT NULL DEFAULT '',
    nonce TEXT,
    code_challenge TEXT,
    user_id TEXT NOT NULL,
    created_ts BIGINT NOT NULL,
    expires_at BIGINT NOT NULL
);

CREATE INDEX IF NOT EXISTS idx_oidc_consent_sessions_expires ON oidc_consent_sessions(expires_at);

-- ============================================================================
-- MAS 用户锁定状态同步 (Synapse v1.151.0 / #24)
-- ============================================================================
CREATE TABLE IF NOT EXISTS user_locks (
    id BIGSERIAL PRIMARY KEY,
    user_id TEXT NOT NULL,
    reason TEXT,
    locked_by TEXT NOT NULL,
    created_ts BIGINT NOT NULL,
    unlocked_ts BIGINT,
    is_active BOOLEAN NOT NULL DEFAULT TRUE,
    FOREIGN KEY (user_id) REFERENCES users(user_id) ON DELETE CASCADE
);

CREATE UNIQUE INDEX IF NOT EXISTS idx_user_locks_user_active ON user_locks(user_id, is_active) WHERE is_active = TRUE;
CREATE INDEX IF NOT EXISTS idx_user_locks_active ON user_locks(is_active, created_ts DESC) WHERE is_active = TRUE;

-- ============================================================================
-- Quarantined Media Changes Stream (#25)
-- ============================================================================

CREATE TABLE IF NOT EXISTS quarantined_media_changes (
    stream_id BIGSERIAL PRIMARY KEY,
    media_id TEXT NOT NULL,
    server_name TEXT NOT NULL,
    change_type TEXT NOT NULL,  -- 'quarantine' or 'unquarantine'
    changed_by TEXT NOT NULL,
    created_ts BIGINT NOT NULL
);

CREATE INDEX IF NOT EXISTS idx_quarantined_media_changes_stream ON quarantined_media_changes(stream_id DESC);
CREATE INDEX IF NOT EXISTS idx_quarantined_media_changes_media ON quarantined_media_changes(media_id, server_name);
