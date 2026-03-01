-- ============================================================================
-- Synapse-Rust Unified Database Schema
-- Version: 2.0.0
-- Created: 2026-03-02
-- Description: Optimized PostgreSQL schema following pg-aiguide best practices
-- ============================================================================

-- Enable required extensions
CREATE EXTENSION IF NOT EXISTS pg_trgm;  -- For trigram similarity searches
CREATE EXTENSION IF NOT EXISTS pgcrypto;  -- For cryptographic functions

-- ============================================================================
-- SECTION 1: Core User Tables
-- ============================================================================

CREATE TABLE IF NOT EXISTS users (
    user_id TEXT NOT NULL PRIMARY KEY,
    username TEXT NOT NULL UNIQUE,
    password_hash TEXT,
    email TEXT,
    displayname TEXT,
    avatar_url TEXT,
    is_admin BOOLEAN DEFAULT FALSE,
    is_deactivated BOOLEAN DEFAULT FALSE,
    is_guest BOOLEAN DEFAULT FALSE,
    shadow_banned BOOLEAN DEFAULT FALSE,
    user_type TEXT,
    appservice_id TEXT,
    consent_version TEXT,
    creation_ts BIGINT NOT NULL,
    updated_ts BIGINT,
    last_seen_ts BIGINT,
    invalid_update_ts BIGINT,
    generation BIGINT NOT NULL DEFAULT 1,
    migration_state TEXT
);

-- User indexes
CREATE INDEX IF NOT EXISTS idx_users_username ON users(username);
CREATE INDEX IF NOT EXISTS idx_users_email ON users(email) WHERE email IS NOT NULL;
CREATE INDEX IF NOT EXISTS idx_users_creation_ts ON users(creation_ts DESC);
CREATE INDEX IF NOT EXISTS idx_users_deactivated ON users(is_deactivated) WHERE is_deactivated = TRUE;
CREATE INDEX IF NOT EXISTS idx_users_username_trgm ON users USING gin(username gin_trgm_ops);
CREATE INDEX IF NOT EXISTS idx_users_displayname_trgm ON users USING gin(COALESCE(displayname, '') gin_trgm_ops);

-- ============================================================================
-- SECTION 2: Authentication Tables
-- ============================================================================

CREATE TABLE IF NOT EXISTS devices (
    device_id TEXT NOT NULL PRIMARY KEY,
    user_id TEXT NOT NULL,
    display_name TEXT,
    last_seen_ts BIGINT NOT NULL DEFAULT 0,
    last_seen_ip TEXT,
    first_seen_ts BIGINT DEFAULT 0,
    created_ts BIGINT NOT NULL,
    ignored_user_list TEXT,
    appservice_id TEXT,
    CONSTRAINT fk_devices_user FOREIGN KEY (user_id) REFERENCES users(user_id) ON DELETE CASCADE
);

CREATE INDEX IF NOT EXISTS idx_devices_user ON devices(user_id);
CREATE INDEX IF NOT EXISTS idx_devices_last_seen ON devices(last_seen_ts DESC);

CREATE TABLE IF NOT EXISTS access_tokens (
    id BIGSERIAL PRIMARY KEY,
    token TEXT NOT NULL UNIQUE,
    user_id TEXT NOT NULL,
    device_id TEXT,
    expires_ts BIGINT,
    created_ts BIGINT NOT NULL,
    invalidated_ts BIGINT,
    expired_ts BIGINT,
    CONSTRAINT fk_access_tokens_user FOREIGN KEY (user_id) REFERENCES users(user_id) ON DELETE CASCADE,
    CONSTRAINT fk_access_tokens_device FOREIGN KEY (device_id) REFERENCES devices(device_id) ON DELETE CASCADE
);

CREATE INDEX IF NOT EXISTS idx_access_tokens_user ON access_tokens(user_id);
CREATE INDEX IF NOT EXISTS idx_access_tokens_expires ON access_tokens(expires_ts) WHERE expires_ts IS NOT NULL;

CREATE TABLE IF NOT EXISTS refresh_tokens (
    id BIGSERIAL PRIMARY KEY,
    token TEXT NOT NULL UNIQUE,
    user_id TEXT NOT NULL,
    device_id TEXT,
    expires_ts BIGINT,
    created_ts BIGINT NOT NULL,
    invalidated_ts BIGINT,
    expired_ts BIGINT,
    family_id TEXT,
    rotation_count INTEGER DEFAULT 0,
    CONSTRAINT fk_refresh_tokens_user FOREIGN KEY (user_id) REFERENCES users(user_id) ON DELETE CASCADE,
    CONSTRAINT fk_refresh_tokens_device FOREIGN KEY (device_id) REFERENCES devices(device_id) ON DELETE CASCADE
);

CREATE INDEX IF NOT EXISTS idx_refresh_tokens_user ON refresh_tokens(user_id);
CREATE INDEX IF NOT EXISTS idx_refresh_tokens_family ON refresh_tokens(family_id) WHERE family_id IS NOT NULL;

CREATE TABLE IF NOT EXISTS token_blacklist (
    id BIGSERIAL PRIMARY KEY,
    token_hash TEXT NOT NULL UNIQUE,
    token TEXT,
    token_type TEXT NOT NULL DEFAULT 'access',
    user_id TEXT,
    reason TEXT,
    revoked_at BIGINT NOT NULL,
    expires_ts BIGINT,
    CONSTRAINT fk_token_blacklist_user FOREIGN KEY (user_id) REFERENCES users(user_id) ON DELETE CASCADE
);

CREATE INDEX IF NOT EXISTS idx_token_blacklist_hash ON token_blacklist(token_hash);
CREATE INDEX IF NOT EXISTS idx_token_blacklist_user ON token_blacklist(user_id);
CREATE INDEX IF NOT EXISTS idx_token_blacklist_expires ON token_blacklist(expires_ts) WHERE expires_ts IS NOT NULL;

-- ============================================================================
-- SECTION 3: Room Tables
-- ============================================================================

CREATE TABLE IF NOT EXISTS rooms (
    room_id TEXT NOT NULL PRIMARY KEY,
    name TEXT,
    topic TEXT,
    avatar TEXT,
    creator TEXT NOT NULL,
    is_public BOOLEAN NOT NULL DEFAULT FALSE,
    is_direct BOOLEAN DEFAULT FALSE,
    federate BOOLEAN NOT NULL DEFAULT TRUE,
    version TEXT NOT NULL DEFAULT '1',
    canonical_alias TEXT,
    join_rule TEXT DEFAULT 'invite',
    history_visibility TEXT DEFAULT 'shared',
    guest_access BOOLEAN DEFAULT FALSE,
    encryption TEXT,
    member_count INTEGER DEFAULT 0,
    is_flagged BOOLEAN DEFAULT FALSE,
    is_spotlight BOOLEAN DEFAULT FALSE,
    creation_ts BIGINT NOT NULL,
    updated_ts BIGINT,
    deleted_ts BIGINT
);

CREATE INDEX IF NOT EXISTS idx_rooms_creator ON rooms(creator);
CREATE INDEX IF NOT EXISTS idx_rooms_public ON rooms(is_public) WHERE is_public = TRUE;
CREATE INDEX IF NOT EXISTS idx_rooms_member_count ON rooms(member_count DESC);
CREATE INDEX IF NOT EXISTS idx_rooms_creation_ts ON rooms(creation_ts DESC);

CREATE TABLE IF NOT EXISTS room_memberships (
    room_id TEXT NOT NULL,
    user_id TEXT NOT NULL,
    sender TEXT NOT NULL,
    membership TEXT NOT NULL,
    event_id TEXT NOT NULL,
    event_type TEXT,
    display_name TEXT,
    avatar_url TEXT,
    is_banned BOOLEAN DEFAULT FALSE,
    invite_token TEXT,
    inviter TEXT,
    updated_ts BIGINT,
    joined_ts BIGINT,
    left_ts BIGINT,
    reason TEXT,
    join_reason TEXT,
    banned_by TEXT,
    ban_reason TEXT,
    ban_ts BIGINT,
    PRIMARY KEY (room_id, user_id),
    CONSTRAINT fk_room_memberships_room FOREIGN KEY (room_id) REFERENCES rooms(room_id) ON DELETE CASCADE,
    CONSTRAINT fk_room_memberships_user FOREIGN KEY (user_id) REFERENCES users(user_id) ON DELETE CASCADE
);

CREATE INDEX IF NOT EXISTS idx_room_memberships_user ON room_memberships(user_id);
CREATE INDEX IF NOT EXISTS idx_room_memberships_membership ON room_memberships(membership);
CREATE INDEX IF NOT EXISTS idx_room_memberships_joined ON room_memberships(joined_ts DESC) WHERE joined_ts IS NOT NULL;

CREATE TABLE IF NOT EXISTS room_aliases (
    room_id TEXT NOT NULL,
    alias TEXT NOT NULL UNIQUE,
    creator TEXT NOT NULL,
    created_ts BIGINT NOT NULL,
    CONSTRAINT fk_room_aliases_room FOREIGN KEY (room_id) REFERENCES rooms(room_id) ON DELETE CASCADE
);

CREATE INDEX IF NOT EXISTS idx_room_aliases_room ON room_aliases(room_id);

CREATE TABLE IF NOT EXISTS room_invites (
    id BIGSERIAL PRIMARY KEY,
    room_id TEXT NOT NULL,
    user_id TEXT NOT NULL,
    inviter TEXT NOT NULL,
    invite_token TEXT NOT NULL UNIQUE,
    is_accepted BOOLEAN DEFAULT FALSE,
    created_ts BIGINT NOT NULL,
    accepted_ts BIGINT,
    expires_ts BIGINT,
    CONSTRAINT fk_room_invites_room FOREIGN KEY (room_id) REFERENCES rooms(room_id) ON DELETE CASCADE,
    CONSTRAINT fk_room_invites_user FOREIGN KEY (user_id) REFERENCES users(user_id) ON DELETE CASCADE
);

CREATE INDEX IF NOT EXISTS idx_room_invites_user ON room_invites(user_id);
CREATE INDEX IF NOT EXISTS idx_room_invites_token ON room_invites(invite_token);

-- ============================================================================
-- SECTION 4: Events Table
-- ============================================================================

CREATE TABLE IF NOT EXISTS events (
    event_id TEXT NOT NULL PRIMARY KEY,
    room_id TEXT NOT NULL,
    sender TEXT NOT NULL,
    type TEXT NOT NULL,
    content TEXT NOT NULL,
    state_key TEXT,
    depth BIGINT NOT NULL DEFAULT 0,
    origin_server_ts BIGINT NOT NULL,
    processed_ts BIGINT NOT NULL,
    not_before BIGINT DEFAULT 0,
    status TEXT DEFAULT NULL,
    reference_image TEXT,
    origin TEXT NOT NULL,
    redacted BOOLEAN DEFAULT FALSE,
    CONSTRAINT fk_events_room FOREIGN KEY (room_id) REFERENCES rooms(room_id) ON DELETE CASCADE,
    CONSTRAINT fk_events_sender FOREIGN KEY (sender) REFERENCES users(user_id) ON DELETE CASCADE
);

CREATE INDEX IF NOT EXISTS idx_events_room ON events(room_id);
CREATE INDEX IF NOT EXISTS idx_events_room_sender ON events(room_id, sender);
CREATE INDEX IF NOT EXISTS idx_events_type_ts ON events(type, origin_server_ts DESC);
CREATE INDEX IF NOT EXISTS idx_events_state_key ON events(state_key) WHERE state_key IS NOT NULL;
CREATE INDEX IF NOT EXISTS idx_events_sender ON events(sender);

-- ============================================================================
-- SECTION 5: Presence and User Status
-- ============================================================================

CREATE TABLE IF NOT EXISTS presence (
    user_id TEXT NOT NULL PRIMARY KEY,
    status_msg TEXT,
    presence TEXT NOT NULL DEFAULT 'offline',
    last_active_ts BIGINT NOT NULL DEFAULT 0,
    status_from TEXT,
    created_ts BIGINT NOT NULL,
    updated_ts BIGINT NOT NULL,
    CONSTRAINT fk_presence_user FOREIGN KEY (user_id) REFERENCES users(user_id) ON DELETE CASCADE
);

CREATE INDEX IF NOT EXISTS idx_presence_last_active ON presence(last_active_ts DESC);

CREATE TABLE IF NOT EXISTS typing (
    user_id TEXT NOT NULL,
    room_id TEXT NOT NULL,
    last_typing_ts BIGINT NOT NULL,
    CONSTRAINT typing_unique UNIQUE(user_id, room_id)
);

CREATE INDEX IF NOT EXISTS idx_typing_room ON typing(room_id);

-- ============================================================================
-- SECTION 6: E2EE Tables
-- ============================================================================

CREATE TABLE IF NOT EXISTS device_keys (
    id UUID NOT NULL PRIMARY KEY DEFAULT gen_random_uuid(),
    user_id TEXT NOT NULL,
    device_id TEXT NOT NULL,
    display_name TEXT,
    algorithm TEXT NOT NULL,
    key_id TEXT NOT NULL,
    public_key TEXT NOT NULL,
    signatures JSONB NOT NULL DEFAULT '{}',
    created_at TIMESTAMP WITH TIME ZONE NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMP WITH TIME ZONE NOT NULL DEFAULT NOW(),
    CONSTRAINT fk_device_keys_user FOREIGN KEY (user_id) REFERENCES users(user_id) ON DELETE CASCADE,
    CONSTRAINT device_keys_unique UNIQUE(user_id, device_id, key_id)
);

CREATE INDEX IF NOT EXISTS idx_device_keys_user ON device_keys(user_id);
CREATE INDEX IF NOT EXISTS idx_device_keys_device ON device_keys(device_id);
CREATE INDEX IF NOT EXISTS idx_device_keys_algorithm ON device_keys(algorithm);

CREATE TABLE IF NOT EXISTS cross_signing_keys (
    id UUID NOT NULL PRIMARY KEY DEFAULT gen_random_uuid(),
    user_id TEXT NOT NULL,
    key_type TEXT NOT NULL,
    public_key TEXT NOT NULL,
    usage TEXT[] NOT NULL,
    signatures JSONB NOT NULL DEFAULT '{}',
    created_at TIMESTAMP WITH TIME ZONE NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMP WITH TIME ZONE NOT NULL DEFAULT NOW(),
    CONSTRAINT fk_cross_signing_keys_user FOREIGN KEY (user_id) REFERENCES users(user_id) ON DELETE CASCADE,
    CONSTRAINT cross_signing_keys_unique UNIQUE(user_id, key_type)
);

CREATE INDEX IF NOT EXISTS idx_cross_signing_keys_user ON cross_signing_keys(user_id);

CREATE TABLE IF NOT EXISTS megolm_sessions (
    id UUID NOT NULL PRIMARY KEY DEFAULT gen_random_uuid(),
    session_id TEXT NOT NULL UNIQUE,
    room_id TEXT NOT NULL,
    sender_key TEXT NOT NULL,
    session_key TEXT NOT NULL,
    algorithm TEXT NOT NULL DEFAULT 'm.megolm.v1.aes-sha2',
    message_index BIGINT NOT NULL DEFAULT 0,
    created_at TIMESTAMP WITH TIME ZONE NOT NULL DEFAULT NOW(),
    last_used_at TIMESTAMP WITH TIME ZONE NOT NULL DEFAULT NOW(),
    expires_at TIMESTAMP WITH TIME ZONE
);

CREATE INDEX IF NOT EXISTS idx_megolm_sessions_room ON megolm_sessions(room_id);
CREATE INDEX IF NOT EXISTS idx_megolm_sessions_sender ON megolm_sessions(sender_key);

CREATE TABLE IF NOT EXISTS inbound_megolm_sessions (
    id UUID NOT NULL PRIMARY KEY DEFAULT gen_random_uuid(),
    session_id TEXT NOT NULL UNIQUE,
    sender_key TEXT NOT NULL,
    room_id TEXT NOT NULL,
    session_key TEXT NOT NULL,
    algorithm TEXT NOT NULL DEFAULT 'm.megolm.v1.aes-sha2',
    message_index BIGINT NOT NULL DEFAULT 0,
    created_at TIMESTAMP WITH TIME ZONE NOT NULL DEFAULT NOW(),
    expires_at TIMESTAMP WITH TIME ZONE
);

CREATE INDEX IF NOT EXISTS idx_inbound_megolm_sessions_sender ON inbound_megolm_sessions(sender_key);

CREATE TABLE IF NOT EXISTS key_backups (
    id UUID NOT NULL PRIMARY KEY DEFAULT gen_random_uuid(),
    user_id TEXT NOT NULL,
    version TEXT NOT NULL,
    algorithm TEXT NOT NULL DEFAULT 'm.megolm_backup.v1',
    auth_data JSONB NOT NULL,
    encrypted_data JSONB NOT NULL,
    created_at TIMESTAMP WITH TIME ZONE NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMP WITH TIME ZONE NOT NULL DEFAULT NOW(),
    CONSTRAINT fk_key_backups_user FOREIGN KEY (user_id) REFERENCES users(user_id) ON DELETE CASCADE,
    CONSTRAINT key_backups_unique UNIQUE(user_id, version)
);

CREATE INDEX IF NOT EXISTS idx_key_backups_user ON key_backups(user_id);

CREATE TABLE IF NOT EXISTS backup_keys (
    id UUID NOT NULL PRIMARY KEY DEFAULT gen_random_uuid(),
    backup_id UUID NOT NULL,
    room_id TEXT NOT NULL,
    session_id TEXT NOT NULL,
    first_message_index BIGINT NOT NULL,
    forwarded_count BIGINT NOT NULL DEFAULT 0,
    is_verified BOOLEAN NOT NULL DEFAULT FALSE,
    session_data TEXT NOT NULL,
    created_at TIMESTAMP WITH TIME ZONE NOT NULL DEFAULT NOW(),
    CONSTRAINT fk_backup_keys_backup FOREIGN KEY (backup_id) REFERENCES key_backups(id) ON DELETE CASCADE,
    CONSTRAINT backup_keys_unique UNIQUE(backup_id, room_id, session_id)
);

CREATE INDEX IF NOT EXISTS idx_backup_keys_backup ON backup_keys(backup_id);

-- ============================================================================
-- SECTION 7: Push Notifications
-- ============================================================================

CREATE TABLE IF NOT EXISTS push_rules (
    id BIGSERIAL PRIMARY KEY,
    rule_id TEXT NOT NULL UNIQUE,
    user_id TEXT NOT NULL,
    kind TEXT NOT NULL,
    priority INTEGER NOT NULL DEFAULT 0,
    pattern TEXT,
    conditions JSONB NOT NULL DEFAULT '[]',
    actions JSONB NOT NULL DEFAULT '[]',
    is_default BOOLEAN NOT NULL DEFAULT FALSE,
    is_enabled BOOLEAN NOT NULL DEFAULT TRUE,
    created_ts BIGINT NOT NULL,
    updated_ts BIGINT,
    CONSTRAINT fk_push_rules_user FOREIGN KEY (user_id) REFERENCES users(user_id) ON DELETE CASCADE
);

CREATE INDEX IF NOT EXISTS idx_push_rules_user ON push_rules(user_id);
CREATE INDEX IF NOT EXISTS idx_push_rules_user_kind ON push_rules(user_id, kind);
CREATE INDEX IF NOT EXISTS idx_push_rules_priority ON push_rules(priority DESC);

CREATE TABLE IF NOT EXISTS pushers (
    id BIGSERIAL PRIMARY KEY,
    user_id TEXT NOT NULL,
    profile_tag TEXT NOT NULL,
    kind TEXT NOT NULL,
    app_id TEXT NOT NULL,
    app_display_name TEXT NOT NULL,
    device_display_name TEXT NOT NULL,
    profile_tag_url TEXT,
    data JSONB NOT NULL DEFAULT '{}',
    is_enabled BOOLEAN NOT NULL DEFAULT TRUE,
    created_ts BIGINT NOT NULL,
    updated_ts BIGINT,
    CONSTRAINT fk_pushers_user FOREIGN KEY (user_id) REFERENCES users(user_id) ON DELETE CASCADE
);

CREATE INDEX IF NOT EXISTS idx_pushers_user ON pushers(user_id);

-- ============================================================================
-- SECTION 8: Federation Tables
-- ============================================================================

CREATE TABLE IF NOT EXISTS federation_signing_keys (
    id BIGSERIAL PRIMARY KEY,
    server_name TEXT NOT NULL UNIQUE,
    key_id TEXT NOT NULL,
    public_key TEXT NOT NULL,
    valid_from_ts BIGINT NOT NULL,
    valid_until_ts BIGINT NOT NULL,
    created_ts BIGINT NOT NULL
);

CREATE INDEX IF NOT EXISTS idx_federation_signing_keys_valid ON federation_signing_keys(valid_until_ts) WHERE valid_until_ts > EXTRACT(EPOCH FROM NOW()) * 1000;

CREATE TABLE IF NOT EXISTS federation_blacklist (
    id BIGSERIAL PRIMARY KEY,
    server_name TEXT NOT NULL UNIQUE,
    block_type TEXT DEFAULT 'server',
    reason TEXT,
    blocked_by TEXT,
    created_ts BIGINT NOT NULL,
    updated_ts BIGINT
);

CREATE INDEX IF NOT EXISTS idx_federation_blacklist_type ON federation_blacklist(block_type);

-- ============================================================================
-- SECTION 9: Space Tables
-- ============================================================================

CREATE TABLE IF NOT EXISTS spaces (
    id BIGSERIAL PRIMARY KEY,
    space_id TEXT NOT NULL UNIQUE,
    room_id TEXT NOT NULL,
    name TEXT,
    topic TEXT,
    avatar_url TEXT,
    creator TEXT NOT NULL,
    join_rule TEXT DEFAULT 'invite',
    visibility TEXT DEFAULT 'private',
    is_public BOOLEAN DEFAULT FALSE,
    parent_space_id TEXT,
    creation_ts BIGINT NOT NULL,
    updated_ts BIGINT
);

CREATE INDEX IF NOT EXISTS idx_spaces_room ON spaces(room_id);
CREATE INDEX IF NOT EXISTS idx_spaces_creator ON spaces(creator);
CREATE INDEX IF NOT EXISTS idx_spaces_parent ON spaces(parent_space_id);

CREATE TABLE IF NOT EXISTS space_members (
    id BIGSERIAL PRIMARY KEY,
    space_id TEXT NOT NULL,
    user_id TEXT NOT NULL,
    membership TEXT DEFAULT 'join',
    joined_ts BIGINT NOT NULL,
    inviter TEXT,
    left_ts BIGINT,
    CONSTRAINT space_members_unique UNIQUE(space_id, user_id),
    CONSTRAINT fk_space_members_space FOREIGN KEY (space_id) REFERENCES spaces(space_id) ON DELETE CASCADE
);

CREATE INDEX IF NOT EXISTS idx_space_members_space ON space_members(space_id);
CREATE INDEX IF NOT EXISTS idx_space_members_user ON space_members(user_id);
CREATE INDEX IF NOT EXISTS idx_space_members_membership ON space_members(membership);

CREATE TABLE IF NOT EXISTS space_children (
    id BIGSERIAL PRIMARY KEY,
    space_id TEXT NOT NULL,
    room_id TEXT NOT NULL,
    via_servers TEXT[],
    order_str TEXT,
    suggested BOOLEAN DEFAULT FALSE,
    added_by TEXT NOT NULL,
    added_ts BIGINT NOT NULL,
    CONSTRAINT space_children_unique UNIQUE(space_id, room_id),
    CONSTRAINT fk_space_children_space FOREIGN KEY (space_id) REFERENCES spaces(space_id) ON DELETE CASCADE
);

CREATE INDEX IF NOT EXISTS idx_space_children_space ON space_children(space_id);
CREATE INDEX IF NOT EXISTS idx_space_children_room ON space_children(room_id);

-- ============================================================================
-- SECTION 10: Thread Tables
-- ============================================================================

CREATE TABLE IF NOT EXISTS thread_roots (
    id BIGSERIAL PRIMARY KEY,
    thread_id TEXT NOT NULL UNIQUE,
    room_id TEXT NOT NULL,
    root_event_id TEXT NOT NULL UNIQUE,
    creator TEXT NOT NULL,
    title TEXT,
    last_reply_ts BIGINT,
    reply_count INTEGER DEFAULT 0,
    is_locked BOOLEAN DEFAULT FALSE,
    created_ts BIGINT NOT NULL
);

CREATE INDEX IF NOT EXISTS idx_thread_roots_room ON thread_roots(room_id, last_reply_ts DESC);
CREATE INDEX IF NOT EXISTS idx_thread_roots_creator ON thread_roots(creator);

CREATE TABLE IF NOT EXISTS thread_replies (
    id BIGSERIAL PRIMARY KEY,
    event_id TEXT NOT NULL UNIQUE,
    thread_id TEXT NOT NULL,
    room_id TEXT NOT NULL,
    sender TEXT NOT NULL,
    content JSONB NOT NULL DEFAULT '{}',
    created_ts BIGINT NOT NULL
);

CREATE INDEX IF NOT EXISTS idx_thread_replies_thread ON thread_replies(thread_id, created_ts DESC);
CREATE INDEX IF NOT EXISTS idx_thread_replies_room ON thread_replies(room_id);
CREATE INDEX IF NOT EXISTS idx_thread_replies_sender ON thread_replies(sender);

-- ============================================================================
-- SECTION 11: Media Tables
-- ============================================================================

CREATE TABLE IF NOT EXISTS media_repository (
    id BIGSERIAL PRIMARY KEY,
    media_id TEXT NOT NULL UNIQUE,
    user_id TEXT NOT NULL,
    server_name TEXT NOT NULL,
    media_type TEXT NOT NULL,
    media_length BIGINT NOT NULL,
    file_path TEXT NOT NULL,
    upload_name TEXT,
    created_ts BIGINT NOT NULL,
    last_access_ts BIGINT,
    quarantine_status TEXT DEFAULT 'safe',
    CONSTRAINT fk_media_user FOREIGN KEY (user_id) REFERENCES users(user_id) ON DELETE CASCADE
);

CREATE INDEX IF NOT EXISTS idx_media_user ON media_repository(user_id);
CREATE INDEX IF NOT EXISTS idx_media_created ON media_repository(created_ts DESC);

-- ============================================================================
-- SECTION 12: Account Data Tables
-- ============================================================================

CREATE TABLE IF NOT EXISTS account_data (
    id BIGSERIAL PRIMARY KEY,
    user_id TEXT NOT NULL,
    data_type TEXT NOT NULL,
    content JSONB NOT NULL DEFAULT '{}',
    created_ts BIGINT NOT NULL,
    updated_ts BIGINT,
    CONSTRAINT account_data_unique UNIQUE(user_id, data_type),
    CONSTRAINT fk_account_data_user FOREIGN KEY (user_id) REFERENCES users(user_id) ON DELETE CASCADE
);

CREATE INDEX IF NOT EXISTS idx_account_data_user ON account_data(user_id);

CREATE TABLE IF NOT EXISTS room_account_data (
    id BIGSERIAL PRIMARY KEY,
    user_id TEXT NOT NULL,
    room_id TEXT NOT NULL,
    data_type TEXT NOT NULL,
    content JSONB NOT NULL DEFAULT '{}',
    created_ts BIGINT NOT NULL,
    updated_ts BIGINT,
    CONSTRAINT room_account_data_unique UNIQUE(user_id, room_id, data_type),
    CONSTRAINT fk_room_account_data_user FOREIGN KEY (user_id) REFERENCES users(user_id) ON DELETE CASCADE
);

CREATE INDEX IF NOT EXISTS idx_room_account_data_user ON room_account_data(user_id);
CREATE INDEX IF NOT EXISTS idx_room_account_data_room ON room_account_data(room_id);

-- ============================================================================
-- SECTION 13: Voice Message Tables
-- ============================================================================

CREATE TABLE IF NOT EXISTS voice_messages (
    id BIGSERIAL PRIMARY KEY,
    event_id TEXT NOT NULL UNIQUE,
    room_id TEXT NOT NULL,
    user_id TEXT NOT NULL,
    media_id TEXT NOT NULL,
    duration_ms BIGINT NOT NULL,
    waveform JSONB,
    file_size BIGINT,
    mime_type TEXT DEFAULT 'audio/ogg',
    is_processed BOOLEAN DEFAULT FALSE,
    created_ts BIGINT NOT NULL
);

CREATE INDEX IF NOT EXISTS idx_voice_messages_room ON voice_messages(room_id, created_ts DESC);
CREATE INDEX IF NOT EXISTS idx_voice_messages_user ON voice_messages(user_id, created_ts DESC);
CREATE INDEX IF NOT EXISTS idx_voice_messages_pending ON voice_messages(created_ts) WHERE is_processed = FALSE;

-- ============================================================================
-- SECTION 14: Worker Tables
-- ============================================================================

CREATE TABLE IF NOT EXISTS workers (
    id BIGSERIAL PRIMARY KEY,
    worker_id TEXT NOT NULL UNIQUE,
    worker_type TEXT NOT NULL,
    status TEXT DEFAULT 'starting',
    last_heartbeat_ts BIGINT,
    started_ts BIGINT NOT NULL,
    stopped_ts BIGINT,
    config JSONB DEFAULT '{}'
);

CREATE INDEX IF NOT EXISTS idx_workers_id ON workers(worker_id);
CREATE INDEX IF NOT EXISTS idx_workers_status ON workers(status);
CREATE INDEX IF NOT EXISTS idx_workers_type ON workers(worker_type);

CREATE TABLE IF NOT EXISTS worker_commands (
    id BIGSERIAL PRIMARY KEY,
    worker_id TEXT NOT NULL,
    target_worker_id TEXT,
    command_id TEXT,
    command_type TEXT NOT NULL,
    command_data JSONB DEFAULT '{}',
    status TEXT DEFAULT 'pending',
    created_ts BIGINT NOT NULL,
    processed_ts BIGINT,
    error_message TEXT,
    retry_count INTEGER DEFAULT 0
);

CREATE INDEX IF NOT EXISTS idx_worker_commands_worker ON worker_commands(worker_id);
CREATE INDEX IF NOT EXISTS idx_worker_commands_target ON worker_commands(target_worker_id);
CREATE INDEX IF NOT EXISTS idx_worker_commands_status ON worker_commands(status) WHERE status = 'pending';

CREATE TABLE IF NOT EXISTS worker_tasks (
    id BIGSERIAL PRIMARY KEY,
    task_type TEXT NOT NULL,
    task_data JSONB DEFAULT '{}',
    priority INTEGER DEFAULT 0,
    status TEXT DEFAULT 'pending',
    assigned_worker_id TEXT,
    result JSONB,
    error_message TEXT,
    created_ts BIGINT NOT NULL,
    started_ts BIGINT,
    completed_ts BIGINT,
    retry_count INTEGER DEFAULT 0
);

CREATE INDEX IF NOT EXISTS idx_worker_tasks_status ON worker_tasks(status) WHERE status = 'pending';
CREATE INDEX IF NOT EXISTS idx_worker_tasks_priority ON worker_tasks(priority DESC);
CREATE INDEX IF NOT EXISTS idx_worker_tasks_assigned ON worker_tasks(assigned_worker_id);

-- ============================================================================
-- SECTION 15: Application Service Tables
-- ============================================================================

CREATE TABLE IF NOT EXISTS application_services (
    id BIGSERIAL PRIMARY KEY,
    as_id TEXT NOT NULL UNIQUE,
    url TEXT NOT NULL,
    as_token TEXT NOT NULL,
    hs_token TEXT NOT NULL,
    sender TEXT NOT NULL,
    name TEXT,
    description TEXT,
    rate_limited BOOLEAN DEFAULT FALSE,
    protocols JSONB DEFAULT '[]',
    namespaces JSONB DEFAULT '{}',
    is_enabled BOOLEAN DEFAULT TRUE,
    created_ts BIGINT NOT NULL,
    updated_ts BIGINT,
    last_seen_ts BIGINT
);

CREATE TABLE IF NOT EXISTS application_service_state (
    id BIGSERIAL PRIMARY KEY,
    as_id TEXT NOT NULL,
    state_key TEXT NOT NULL,
    state_value TEXT NOT NULL,
    updated_ts BIGINT NOT NULL,
    CONSTRAINT app_service_state_unique UNIQUE(as_id, state_key)
);

CREATE INDEX IF NOT EXISTS idx_app_service_state_as_id ON application_service_state(as_id);

CREATE TABLE IF NOT EXISTS application_service_users (
    id BIGSERIAL PRIMARY KEY,
    as_id TEXT NOT NULL,
    user_id TEXT NOT NULL,
    displayname TEXT,
    avatar_url TEXT,
    created_ts BIGINT NOT NULL,
    CONSTRAINT app_service_users_unique UNIQUE(as_id, user_id)
);

CREATE INDEX IF NOT EXISTS idx_app_service_users_as_id ON application_service_users(as_id);

-- ============================================================================
-- SECTION 16: Retention Tables
-- ============================================================================

CREATE TABLE IF NOT EXISTS room_retention_policies (
    id BIGSERIAL PRIMARY KEY,
    room_id TEXT NOT NULL UNIQUE,
    max_lifetime BIGINT,
    min_lifetime BIGINT DEFAULT 0,
    expire_on_clients BOOLEAN DEFAULT FALSE,
    is_server_default BOOLEAN DEFAULT FALSE,
    created_ts BIGINT NOT NULL,
    updated_ts BIGINT NOT NULL
);

CREATE TABLE IF NOT EXISTS server_retention_policy (
    id BIGSERIAL PRIMARY KEY,
    max_lifetime BIGINT,
    min_lifetime BIGINT DEFAULT 0,
    expire_on_clients BOOLEAN DEFAULT FALSE,
    created_ts BIGINT NOT NULL,
    updated_ts BIGINT NOT NULL
);

-- Insert default server retention policy
INSERT INTO server_retention_policy (min_lifetime, expire_on_clients, created_ts, updated_ts)
SELECT 0, FALSE, EXTRACT(EPOCH FROM NOW())::BIGINT * 1000, EXTRACT(EPOCH FROM NOW())::BIGINT * 1000
WHERE NOT EXISTS (SELECT 1 FROM server_retention_policy LIMIT 1);

-- ============================================================================
-- SECTION 17: Utility Tables
-- ============================================================================

CREATE TABLE IF NOT EXISTS migrations (
    id SERIAL PRIMARY KEY,
    name TEXT NOT NULL UNIQUE,
    applied_at TIMESTAMP DEFAULT NOW()
);

CREATE TABLE IF NOT EXISTS filters (
    id BIGSERIAL PRIMARY KEY,
    user_id TEXT NOT NULL,
    filter_id TEXT NOT NULL UNIQUE,
    filter_json JSONB NOT NULL,
    created_ts BIGINT NOT NULL,
    CONSTRAINT fk_filters_user FOREIGN KEY (user_id) REFERENCES users(user_id) ON DELETE CASCADE
);

CREATE INDEX IF NOT EXISTS idx_filters_user ON filters(user_id);

CREATE TABLE IF NOT EXISTS openid_tokens (
    id BIGSERIAL PRIMARY KEY,
    user_id TEXT NOT NULL,
    token TEXT NOT NULL UNIQUE,
    application_id TEXT NOT NULL,
    expires_ts BIGINT NOT NULL,
    created_ts BIGINT NOT NULL,
    CONSTRAINT fk_openid_tokens_user FOREIGN KEY (user_id) REFERENCES users(user_id) ON DELETE CASCADE
);

CREATE INDEX IF NOT EXISTS idx_openid_tokens_user ON openid_tokens(user_id);

-- ============================================================================
-- SECTION 18: Email Verification
-- ============================================================================

CREATE TABLE IF NOT EXISTS email_verification_tokens (
    id BIGSERIAL PRIMARY KEY,
    user_id TEXT NOT NULL,
    email TEXT NOT NULL,
    token TEXT NOT NULL UNIQUE,
    purpose TEXT NOT NULL DEFAULT 'registration',
    expires_ts BIGINT NOT NULL,
    used_ts BIGINT,
    created_ts BIGINT NOT NULL,
    CONSTRAINT fk_email_verification_user FOREIGN KEY (user_id) REFERENCES users(user_id) ON DELETE CASCADE
);

CREATE INDEX IF NOT EXISTS idx_email_verification_user ON email_verification_tokens(user_id);
CREATE INDEX IF NOT EXISTS idx_email_verification_token ON email_verification_tokens(token);

-- ============================================================================
-- SECTION 19: Friends System
-- ============================================================================

CREATE TABLE IF NOT EXISTS friends (
    id BIGSERIAL PRIMARY KEY,
    user_id TEXT NOT NULL,
    friend_id TEXT NOT NULL,
    category_id BIGINT,
    created_ts BIGINT NOT NULL,
    CONSTRAINT friends_unique UNIQUE(user_id, friend_id),
    CONSTRAINT fk_friends_user FOREIGN KEY (user_id) REFERENCES users(user_id) ON DELETE CASCADE,
    CONSTRAINT fk_friends_friend FOREIGN KEY (friend_id) REFERENCES users(user_id) ON DELETE CASCADE
);

CREATE INDEX IF NOT EXISTS idx_friends_user ON friends(user_id);

CREATE TABLE IF NOT EXISTS friend_requests (
    id BIGSERIAL PRIMARY KEY,
    sender_id TEXT NOT NULL,
    receiver_id TEXT NOT NULL,
    message TEXT,
    status TEXT NOT NULL DEFAULT 'pending',
    created_ts BIGINT NOT NULL,
    updated_ts BIGINT,
    CONSTRAINT friend_requests_unique UNIQUE(sender_id, receiver_id),
    CONSTRAINT fk_friend_requests_sender FOREIGN KEY (sender_id) REFERENCES users(user_id) ON DELETE CASCADE,
    CONSTRAINT fk_friend_requests_receiver FOREIGN KEY (receiver_id) REFERENCES users(user_id) ON DELETE CASCADE
);

CREATE INDEX IF NOT EXISTS idx_friend_requests_sender ON friend_requests(sender_id);
CREATE INDEX IF NOT EXISTS idx_friend_requests_receiver ON friend_requests(receiver_id);

-- ============================================================================
-- SECTION 20: CAS/SAML Authentication
-- ============================================================================

CREATE TABLE IF NOT EXISTS cas_tickets (
    id BIGSERIAL PRIMARY KEY,
    ticket_id TEXT UNIQUE,
    service TEXT NOT NULL,
    user_id TEXT,
    created_at BIGINT,
    expires_at BIGINT,
    consumed BOOLEAN DEFAULT FALSE
);

CREATE INDEX IF NOT EXISTS idx_cas_tickets_ticket ON cas_tickets(ticket_id);

CREATE TABLE IF NOT EXISTS cas_services (
    id BIGSERIAL PRIMARY KEY,
    service_url TEXT NOT NULL UNIQUE,
    name TEXT,
    description TEXT,
    created_at BIGINT,
    is_enabled BOOLEAN DEFAULT TRUE
);

CREATE TABLE IF NOT EXISTS cas_proxy_tickets (
    id BIGSERIAL PRIMARY KEY,
    ticket_id TEXT NOT NULL UNIQUE,
    proxy_ticket TEXT NOT NULL,
    target_service TEXT NOT NULL,
    created_ts BIGINT NOT NULL,
    expires_ts BIGINT
);

CREATE INDEX IF NOT EXISTS idx_cas_proxy_tickets_ticket ON cas_proxy_tickets(ticket_id);

-- ============================================================================
-- SECTION 21: Room Summary Tables
-- ============================================================================

CREATE TABLE IF NOT EXISTS room_summaries (
    id BIGSERIAL PRIMARY KEY,
    room_id TEXT NOT NULL UNIQUE,
    room_type TEXT,
    name TEXT,
    topic TEXT,
    avatar_url TEXT,
    canonical_alias TEXT,
    join_rules TEXT DEFAULT 'invite',
    history_visibility TEXT DEFAULT 'shared',
    guest_access TEXT DEFAULT 'forbidden',
    is_direct BOOLEAN DEFAULT FALSE,
    is_space BOOLEAN DEFAULT FALSE,
    is_encrypted BOOLEAN DEFAULT FALSE,
    member_count INTEGER DEFAULT 0,
    joined_member_count INTEGER DEFAULT 0,
    invited_member_count INTEGER DEFAULT 0,
    hero_users JSONB DEFAULT '[]',
    last_event_id TEXT,
    last_event_ts BIGINT,
    last_message_ts BIGINT,
    unread_notifications INTEGER DEFAULT 0,
    unread_highlight INTEGER DEFAULT 0,
    updated_ts BIGINT NOT NULL,
    created_ts BIGINT NOT NULL
);

CREATE INDEX IF NOT EXISTS idx_room_summaries_last_event_ts ON room_summaries(last_event_ts DESC NULLS LAST);

CREATE TABLE IF NOT EXISTS room_summary_members (
    id BIGSERIAL PRIMARY KEY,
    room_id TEXT NOT NULL,
    user_id TEXT NOT NULL,
    display_name TEXT,
    avatar_url TEXT,
    membership TEXT DEFAULT 'join',
    is_hero BOOLEAN DEFAULT FALSE,
    last_active_ts BIGINT,
    updated_ts BIGINT NOT NULL,
    created_ts BIGINT NOT NULL,
    CONSTRAINT room_summary_members_unique UNIQUE(room_id, user_id)
);

CREATE INDEX IF NOT EXISTS idx_room_summary_members_hero ON room_summary_members(room_id, is_hero) WHERE is_hero = TRUE;

-- ============================================================================
-- SECTION 22: Statistics Views
-- ============================================================================

CREATE OR REPLACE VIEW v_active_users AS
SELECT 
    u.user_id,
    u.username,
    u.displayname,
    u.email,
    u.creation_ts,
    COUNT(DISTINCT d.device_id) as device_count,
    MAX(d.last_seen_ts) as last_active_ts
FROM users u
LEFT JOIN devices d ON d.user_id = u.user_id
WHERE u.is_deactivated = FALSE OR u.is_deactivated IS NULL
GROUP BY u.user_id, u.username, u.displayname, u.email, u.creation_ts;

CREATE OR REPLACE VIEW v_room_statistics AS
SELECT 
    r.room_id,
    r.name,
    r.is_public,
    r.creation_ts,
    COUNT(rm.user_id) FILTER (WHERE rm.membership = 'join') as joined_members,
    COUNT(rm.user_id) FILTER (WHERE rm.membership = 'invite') as invited_members,
    COUNT(rm.user_id) FILTER (WHERE rm.membership = 'leave') as left_members,
    COUNT(e.event_id) as total_events
FROM rooms r
LEFT JOIN room_memberships rm ON rm.room_id = r.room_id
LEFT JOIN events e ON e.room_id = r.room_id
GROUP BY r.room_id, r.name, r.is_public, r.creation_ts;

-- ============================================================================
-- SECTION 23: Grant Permissions
-- ============================================================================

GRANT ALL ON ALL TABLES IN SCHEMA public TO synapse;
GRANT ALL ON ALL SEQUENCES IN SCHEMA public TO synapse;
GRANT ALL ON ALL FUNCTIONS IN SCHEMA public TO synapse;

-- ============================================================================
-- SECTION 24: Record Schema Version
-- ============================================================================

INSERT INTO migrations (name, applied_at) 
VALUES ('00000000_unified_schema_v3', NOW())
ON CONFLICT (name) DO NOTHING;

-- Log completion
DO $$
BEGIN
    RAISE NOTICE 'Unified schema v3 created successfully';
END $$;
