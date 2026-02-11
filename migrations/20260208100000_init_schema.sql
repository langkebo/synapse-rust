-- Users table
CREATE TABLE IF NOT EXISTS users (
    user_id TEXT NOT NULL PRIMARY KEY,
    username TEXT NOT NULL UNIQUE,
    password_hash TEXT,
    is_admin BOOLEAN DEFAULT FALSE,
    is_guest BOOLEAN DEFAULT FALSE,
    consent_version TEXT,
    appservice_id TEXT,
    creation_ts BIGINT NOT NULL,
    user_type TEXT,
    deactivated BOOLEAN DEFAULT FALSE,
    shadow_banned BOOLEAN DEFAULT FALSE,
    generation BIGINT NOT NULL,
    avatar_url TEXT,
    displayname TEXT,
    invalid_update_ts BIGINT,
    migration_state TEXT
);

-- Devices table
CREATE TABLE IF NOT EXISTS devices (
    device_id TEXT NOT NULL PRIMARY KEY,
    user_id TEXT NOT NULL,
    display_name TEXT,
    last_seen_ts TIMESTAMP WITH TIME ZONE DEFAULT NOW(),
    last_seen_ip TEXT,
    created_ts BIGINT NOT NULL,
    user_agent TEXT,
    keys JSONB,
    device_display_name TEXT,
    FOREIGN KEY (user_id) REFERENCES users(user_id) ON DELETE CASCADE
);

-- Access Tokens table
CREATE TABLE IF NOT EXISTS access_tokens (
    id BIGSERIAL PRIMARY KEY,
    token TEXT NOT NULL UNIQUE,
    user_id TEXT NOT NULL,
    device_id TEXT,
    created_ts BIGINT NOT NULL,
    expired_ts BIGINT,
    invalidated BOOLEAN DEFAULT FALSE,
    FOREIGN KEY (user_id) REFERENCES users(user_id) ON DELETE CASCADE,
    FOREIGN KEY (device_id) REFERENCES devices(device_id) ON DELETE CASCADE
);

-- Refresh Tokens table
CREATE TABLE IF NOT EXISTS refresh_tokens (
    id BIGSERIAL PRIMARY KEY,
    token TEXT NOT NULL UNIQUE,
    user_id TEXT NOT NULL,
    device_id TEXT NOT NULL,
    created_ts BIGINT NOT NULL,
    expired_ts BIGINT,
    invalidated BOOLEAN DEFAULT FALSE,
    FOREIGN KEY (user_id) REFERENCES users(user_id) ON DELETE CASCADE,
    FOREIGN KEY (device_id) REFERENCES devices(device_id) ON DELETE CASCADE
);

-- Rooms table
CREATE TABLE IF NOT EXISTS rooms (
    room_id TEXT NOT NULL PRIMARY KEY,
    is_public BOOLEAN NOT NULL DEFAULT FALSE,
    creator TEXT NOT NULL,
    creation_ts BIGINT NOT NULL,
    federate BOOLEAN NOT NULL DEFAULT TRUE,
    version TEXT NOT NULL DEFAULT '1',
    name TEXT,
    topic TEXT,
    avatar TEXT,
    canonical_alias TEXT,
    guest_access BOOLEAN DEFAULT FALSE,
    history_visibility TEXT DEFAULT 'shared',
    encryption TEXT,
    is_flaged BOOLEAN DEFAULT FALSE,
    is_spotlight BOOLEAN DEFAULT FALSE,
    deleted_ts BIGINT,
    join_rule TEXT,
    visibility TEXT
);

-- Room Memberships table
CREATE TABLE IF NOT EXISTS room_memberships (
    room_id TEXT NOT NULL,
    user_id TEXT NOT NULL,
    sender TEXT NOT NULL,
    membership TEXT NOT NULL,
    event_id TEXT NOT NULL,
    event_type TEXT NOT NULL,
    display_name TEXT,
    avatar_url TEXT,
    is_banned BOOLEAN DEFAULT FALSE,
    invite_token TEXT,
    inviter TEXT,
    updated_ts BIGINT,
    joined_ts BIGINT,
    left_ts BIGINT,
    reason TEXT,
    banned_by TEXT,
    ban_reason TEXT,
    ban_ts BIGINT,
    join_reason TEXT,
    PRIMARY KEY (room_id, user_id),
    FOREIGN KEY (room_id) REFERENCES rooms(room_id) ON DELETE CASCADE,
    FOREIGN KEY (user_id) REFERENCES users(user_id) ON DELETE CASCADE
);

-- Room Events table
CREATE TABLE IF NOT EXISTS room_events (
    event_id TEXT NOT NULL,
    room_id TEXT NOT NULL,
    user_id TEXT NOT NULL,
    event_type TEXT NOT NULL,
    content TEXT NOT NULL,
    state_key TEXT,
    depth BIGINT NOT NULL DEFAULT 0,
    origin_server_ts BIGINT NOT NULL,
    processed_ts BIGINT NOT NULL,
    not_before BIGINT DEFAULT 0,
    status TEXT DEFAULT NULL,
    reference_image TEXT,
    origin TEXT NOT NULL,
    sender TEXT NOT NULL,
    unsigned TEXT,
    redacted BOOLEAN DEFAULT FALSE,
    PRIMARY KEY (event_id),
    FOREIGN KEY (room_id) REFERENCES rooms(room_id) ON DELETE CASCADE,
    FOREIGN KEY (user_id) REFERENCES users(user_id) ON DELETE CASCADE
);

-- Presence table
CREATE TABLE IF NOT EXISTS presence (
    user_id TEXT NOT NULL PRIMARY KEY,
    status_msg TEXT,
    presence TEXT NOT NULL DEFAULT 'offline',
    last_active_ts BIGINT NOT NULL DEFAULT 0,
    status_from TEXT,
    created_ts BIGINT NOT NULL,
    updated_ts BIGINT NOT NULL,
    FOREIGN KEY (user_id) REFERENCES users(user_id) ON DELETE CASCADE
);

-- User Directory table
CREATE TABLE IF NOT EXISTS user_directory (
    user_id TEXT NOT NULL PRIMARY KEY,
    room_id TEXT NOT NULL,
    visibility TEXT NOT NULL DEFAULT 'private',
    added_by TEXT,
    created_ts BIGINT NOT NULL,
    PRIMARY KEY (user_id, room_id),
    FOREIGN KEY (user_id) REFERENCES users(user_id) ON DELETE CASCADE,
    FOREIGN KEY (room_id) REFERENCES rooms(room_id) ON DELETE CASCADE
);

-- Push Rules table
CREATE TABLE IF NOT EXISTS push_rules (
    id BIGSERIAL PRIMARY KEY,
    user_id TEXT NOT NULL,
    rule_id TEXT NOT NULL,
    priority_class INTEGER NOT NULL DEFAULT 0,
    priority INTEGER NOT NULL DEFAULT 0,
    conditions TEXT,
    actions TEXT,
    is_default_rule BOOLEAN DEFAULT FALSE,
    is_enabled BOOLEAN DEFAULT TRUE,
    is_user_created BOOLEAN DEFAULT FALSE,
    created_ts BIGINT NOT NULL,
    FOREIGN KEY (user_id) REFERENCES users(user_id) ON DELETE CASCADE
);

-- Push Rules User Sent Rules table
CREATE TABLE IF NOT EXISTS push_rules_user_sent_rules (
    id BIGSERIAL PRIMARY KEY,
    user_id TEXT NOT NULL,
    rule_id TEXT NOT NULL,
    enable BOOLEAN DEFAULT TRUE,
    FOREIGN KEY (user_id) REFERENCES users(user_id) ON DELETE CASCADE
);

-- Receipts table
CREATE TABLE IF NOT EXISTS receipts (
    sender TEXT NOT NULL,
    sent_to TEXT NOT NULL,
    room_id TEXT NOT NULL,
    event_id TEXT NOT NULL,
    sent_ts BIGINT NOT NULL,
    receipt_type TEXT NOT NULL,
    PRIMARY KEY (sent_to, sender, room_id),
    FOREIGN KEY (sender) REFERENCES users(user_id) ON DELETE CASCADE,
    FOREIGN KEY (sent_to) REFERENCES users(user_id) ON DELETE CASCADE,
    FOREIGN KEY (room_id) REFERENCES rooms(room_id) ON DELETE CASCADE
);

-- Pusher Throttle table
CREATE TABLE IF NOT EXISTS pusher_throttle (
    pusher TEXT NOT NULL PRIMARY KEY,
    last_sent_ts BIGINT NOT NULL,
    throttle_ms INTEGER NOT NULL DEFAULT 0
);

-- Pushers table
CREATE TABLE IF NOT EXISTS pushers (
    id BIGSERIAL PRIMARY KEY,
    user_id TEXT NOT NULL,
    access_token TEXT NOT NULL,
    profile_tag TEXT,
    kind TEXT NOT NULL,
    app_id TEXT NOT NULL,
    app_display_name TEXT,
    device_name TEXT,
    pushkey TEXT NOT NULL,
    ts BIGINT NOT NULL,
    language TEXT,
    data TEXT,
    expiry_ts BIGINT,
    FOREIGN KEY (user_id) REFERENCES users(user_id) ON DELETE CASCADE
);

-- Ratelimit Shard table
CREATE TABLE IF NOT EXISTS ratelimit_shard (
    user_id TEXT NOT NULL PRIMARY KEY,
    shard_id INTEGER NOT NULL
);

-- User Filters table
CREATE TABLE IF NOT EXISTS user_filters (
    user_id TEXT NOT NULL,
    filter_id BIGINT NOT NULL,
    filter_definition TEXT NOT NULL,
    PRIMARY KEY (user_id, filter_id),
    FOREIGN KEY (user_id) REFERENCES users(user_id) ON DELETE CASCADE
);

-- User IPs table
CREATE TABLE IF NOT EXISTS user_ips (
    user_id TEXT NOT NULL,
    access_token TEXT NOT NULL,
    ip TEXT NOT NULL,
    user_agent TEXT,
    device_id TEXT NOT NULL,
    last_seen BIGINT NOT NULL,
    first_seen BIGINT NOT NULL DEFAULT 0
);

-- Current State Events table
CREATE TABLE IF NOT EXISTS current_state_events (
    room_id TEXT NOT NULL,
    type TEXT NOT NULL,
    state_key TEXT NOT NULL,
    event_id TEXT NOT NULL,
    membership TEXT,
    depth BIGINT NOT NULL,
    stream_ordering BIGINT,
    PRIMARY KEY (room_id, type, state_key),
    FOREIGN KEY (room_id) REFERENCES rooms(room_id) ON DELETE CASCADE
);

-- Email Verification Tokens table
CREATE TABLE IF NOT EXISTS email_verification_tokens (
    id BIGSERIAL PRIMARY KEY,
    email TEXT NOT NULL,
    token TEXT NOT NULL,
    expires_at BIGINT NOT NULL,
    created_ts BIGINT NOT NULL,
    used BOOLEAN DEFAULT FALSE,
    session_data JSONB
);

-- Friends table
CREATE TABLE IF NOT EXISTS friends (
    user_id TEXT NOT NULL,
    friend_id TEXT NOT NULL,
    created_ts BIGINT NOT NULL,
    note TEXT,
    is_favorite BOOLEAN DEFAULT FALSE,
    PRIMARY KEY (user_id, friend_id),
    FOREIGN KEY (user_id) REFERENCES users(user_id) ON DELETE CASCADE,
    FOREIGN KEY (friend_id) REFERENCES users(user_id) ON DELETE CASCADE
);

-- Friend Requests table
CREATE TABLE IF NOT EXISTS friend_requests (
    id BIGSERIAL PRIMARY KEY,
    from_user_id TEXT NOT NULL,
    to_user_id TEXT NOT NULL,
    created_ts BIGINT NOT NULL,
    updated_ts BIGINT,
    status TEXT DEFAULT 'pending',
    message TEXT,
    hide BOOLEAN DEFAULT FALSE,
    FOREIGN KEY (from_user_id) REFERENCES users(user_id) ON DELETE CASCADE,
    FOREIGN KEY (to_user_id) REFERENCES users(user_id) ON DELETE CASCADE
);

-- Blocked Users table
CREATE TABLE IF NOT EXISTS blocked_users (
    user_id TEXT NOT NULL,
    blocked_user_id TEXT NOT NULL,
    reason TEXT,
    created_ts BIGINT NOT NULL,
    PRIMARY KEY (user_id, blocked_user_id),
    FOREIGN KEY (user_id) REFERENCES users(user_id) ON DELETE CASCADE,
    FOREIGN KEY (blocked_user_id) REFERENCES users(user_id) ON DELETE CASCADE
);

-- Friend Categories table
CREATE TABLE IF NOT EXISTS friend_categories (
    id BIGSERIAL PRIMARY KEY,
    user_id TEXT NOT NULL,
    name TEXT NOT NULL,
    color TEXT,
    icon TEXT,
    sort_order BIGINT DEFAULT 0,
    created_ts BIGINT NOT NULL,
    FOREIGN KEY (user_id) REFERENCES users(user_id) ON DELETE CASCADE,
    UNIQUE (user_id, name)
);

-- Security Events table
CREATE TABLE IF NOT EXISTS security_events (
    id BIGSERIAL PRIMARY KEY,
    event_type TEXT NOT NULL,
    severity TEXT NOT NULL DEFAULT 'info',
    user_id TEXT,
    ip_address TEXT,
    user_agent TEXT,
    details TEXT,
    created_at BIGINT NOT NULL,
    resolved BOOLEAN DEFAULT FALSE,
    resolved_by TEXT,
    resolved_ts BIGINT
);

-- IP Blocks table
CREATE TABLE IF NOT EXISTS ip_blocks (
    id BIGSERIAL PRIMARY KEY,
    ip_address TEXT NOT NULL,
    reason TEXT,
    blocked_by TEXT NOT NULL,
    blocked_at BIGINT NOT NULL,
    expires_at BIGINT,
    is_active BOOLEAN DEFAULT TRUE,
    FOREIGN KEY (blocked_by) REFERENCES users(user_id) ON DELETE CASCADE
);

-- Event Reports table
CREATE TABLE IF NOT EXISTS event_reports (
    id BIGSERIAL PRIMARY KEY,
    event_id TEXT NOT NULL,
    room_id TEXT NOT NULL,
    user_id TEXT NOT NULL,
    reporter_user_id TEXT NOT NULL,
    reason TEXT,
    score INTEGER DEFAULT -100,
    created_ts BIGINT NOT NULL,
    updated_ts BIGINT NOT NULL,
    FOREIGN KEY (event_id) REFERENCES events(event_id) ON DELETE CASCADE,
    FOREIGN KEY (user_id) REFERENCES users(user_id) ON DELETE CASCADE,
    FOREIGN KEY (reporter_user_id) REFERENCES users(user_id) ON DELETE CASCADE
);

-- IP Reputation table
CREATE TABLE IF NOT EXISTS ip_reputation (
    ip_address TEXT NOT NULL PRIMARY KEY,
    score INTEGER NOT NULL DEFAULT 50,
    threat_level TEXT DEFAULT 'medium',
    last_seen_at BIGINT NOT NULL,
    updated_at BIGINT NOT NULL,
    report_count INTEGER DEFAULT 0,
    whitelist BOOLEAN DEFAULT FALSE,
    details JSONB
);

-- Database Performance Stats table
CREATE TABLE IF NOT EXISTS database_performance_stats (
    id BIGSERIAL PRIMARY KEY,
    metric_type TEXT NOT NULL,
    metric_name TEXT NOT NULL,
    metric_value DOUBLE PRECISION NOT NULL,
    collected_at BIGINT NOT NULL,
    metadata JSONB
);

-- Database Health History table
CREATE TABLE IF NOT EXISTS database_health_history (
    id BIGSERIAL PRIMARY KEY,
    check_type TEXT NOT NULL,
    status TEXT NOT NULL,
    details JSONB,
    checked_at BIGINT NOT NULL
);

-- Typing table
CREATE TABLE IF NOT EXISTS typing (
    user_id TEXT NOT NULL,
    room_id TEXT NOT NULL,
    typing BOOLEAN DEFAULT FALSE,
    last_active_ts BIGINT NOT NULL,
    UNIQUE (user_id, room_id),
    FOREIGN KEY (user_id) REFERENCES users(user_id) ON DELETE CASCADE,
    FOREIGN KEY (room_id) REFERENCES rooms(room_id) ON DELETE CASCADE
);
