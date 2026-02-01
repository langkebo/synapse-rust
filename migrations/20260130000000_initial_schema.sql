-- synapse-rust Unified Database Schema
-- Generated: 2026-01-30
-- Purpose: Consolidate all database migrations into a single schema definition
-- This migration creates all tables, indexes, and constraints for the synapse-rust project
-- Based on the existing database structure

-- ============================================
-- SECTION 1: Core Tables - Users
-- ============================================

-- Users table: Core user information storage
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

-- Users index for efficient lookups
CREATE INDEX IF NOT EXISTS idx_users_username ON users(username);
CREATE INDEX IF NOT EXISTS idx_users_creation_ts ON users(creation_ts);
CREATE INDEX IF NOT EXISTS idx_users_deactivated ON users(deactivated) WHERE deactivated = TRUE;

-- ============================================
-- SECTION 2: Devices
-- ============================================

-- Devices table: User device information
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

-- Devices indexes for common query patterns
CREATE INDEX IF NOT EXISTS idx_devices_user ON devices(user_id);
CREATE INDEX IF NOT EXISTS idx_devices_last_seen ON devices(last_seen_ts DESC);
CREATE INDEX IF NOT EXISTS idx_devices_created_at ON devices(created_at);

-- ============================================
-- SECTION 3: Authentication Tokens
-- ============================================

-- Access tokens for user authentication
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

-- Access tokens indexes
CREATE INDEX IF NOT EXISTS idx_access_tokens_user ON access_tokens(user_id);
CREATE INDEX IF NOT EXISTS idx_access_tokens_expires ON access_tokens(expires_ts);
CREATE INDEX IF NOT EXISTS idx_access_tokens_token ON access_tokens(token);

-- Refresh tokens for token refresh mechanism
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

-- Refresh tokens indexes
CREATE INDEX IF NOT EXISTS idx_refresh_tokens_user ON refresh_tokens(user_id);
CREATE INDEX IF NOT EXISTS idx_refresh_tokens_expires ON refresh_tokens(expires_ts);
CREATE INDEX IF NOT EXISTS idx_refresh_tokens_token ON refresh_tokens(token);

-- ============================================
-- SECTION 4: Rooms and Memberships
-- ============================================

-- Rooms table: Room metadata storage
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

-- Rooms indexes
CREATE INDEX IF NOT EXISTS idx_rooms_creator ON rooms(creator);
CREATE INDEX IF NOT EXISTS idx_rooms_creation ON rooms(creation_ts);
CREATE INDEX IF NOT EXISTS idx_rooms_public ON rooms(is_public) WHERE is_public = TRUE;
CREATE INDEX IF NOT EXISTS idx_rooms_activity ON rooms(last_activity_ts DESC);
CREATE INDEX IF NOT EXISTS idx_rooms_join_rules ON rooms(join_rule);

-- Room memberships: User-room relationship tracking
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

-- Room memberships critical composite indexes
CREATE INDEX IF NOT EXISTS idx_memberships_room ON room_memberships(room_id);
CREATE INDEX IF NOT EXISTS idx_memberships_user ON room_memberships(user_id);
CREATE INDEX IF NOT EXISTS idx_memberships_room_membership ON room_memberships(room_id, membership);
CREATE INDEX IF NOT EXISTS idx_memberships_room_membership_joined ON room_memberships(room_id, membership, joined_ts DESC);
CREATE INDEX IF NOT EXISTS idx_memberships_joined_ts ON room_memberships(joined_ts);

-- Room events: Core message storage (note: table name is 'events' in existing DB)
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

-- Room events composite indexes for common query patterns
CREATE INDEX IF NOT EXISTS idx_events_room ON events(room_id);
CREATE INDEX IF NOT EXISTS idx_events_room_ts ON events(room_id, origin_server_ts DESC);
CREATE INDEX IF NOT EXISTS idx_events_room_type ON events(room_id, event_type);
CREATE INDEX IF NOT EXISTS idx_events_sender ON events(sender);
CREATE INDEX IF NOT EXISTS idx_events_ts ON events(origin_server_ts DESC);
CREATE INDEX IF NOT EXISTS idx_events_type ON events(event_type);

-- Room aliases: Room alias management
CREATE TABLE IF NOT EXISTS room_aliases (
    room_alias VARCHAR(255) NOT NULL PRIMARY KEY,
    alias VARCHAR(255) NOT NULL,
    room_id VARCHAR(255) NOT NULL,
    created_by VARCHAR(255) NOT NULL,
    creation_ts BIGINT NOT NULL,
    FOREIGN KEY (room_id) REFERENCES rooms(room_id) ON DELETE CASCADE,
    FOREIGN KEY (created_by) REFERENCES users(user_id) ON DELETE CASCADE
);

-- Room aliases indexes
CREATE INDEX IF NOT EXISTS idx_room_aliases_room ON room_aliases(room_id);
CREATE INDEX IF NOT EXISTS idx_room_aliases_creator ON room_aliases(created_by);

-- Room auth chains: Authorization chain tracking
CREATE TABLE IF NOT EXISTS room_auth_chains (
    room_id VARCHAR(255) NOT NULL,
    event_id VARCHAR(255) NOT NULL,
    auth_event_id VARCHAR(255) NOT NULL,
    depth BIGINT NOT NULL,
    PRIMARY KEY (room_id, event_id, auth_event_id),
    FOREIGN KEY (room_id) REFERENCES rooms(room_id) ON DELETE CASCADE
);

-- ============================================
-- SECTION 5: Social Features - Friends and Private Chats
-- ============================================

-- Friends table: Bidirectional friend relationships
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

-- Friends indexes
CREATE INDEX IF NOT EXISTS idx_friends_user ON friends(user_id);
CREATE INDEX IF NOT EXISTS idx_friends_friend ON friends(friend_id);
CREATE INDEX IF NOT EXISTS idx_friends_created ON friends(created_ts);

-- Friend requests table: Pending friend request tracking
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

-- Friend requests indexes
CREATE INDEX IF NOT EXISTS idx_friend_requests_from ON friend_requests(from_user_id);
CREATE INDEX IF NOT EXISTS idx_friend_requests_to ON friend_requests(to_user_id);
CREATE INDEX IF NOT EXISTS idx_friend_requests_status ON friend_requests(status);
CREATE INDEX IF NOT EXISTS idx_friend_requests_created ON friend_requests(created_ts DESC);

-- Private sessions: Private chat session metadata
CREATE TABLE IF NOT EXISTS private_sessions (
    id VARCHAR(255) NOT NULL PRIMARY KEY,
    user_id_1 VARCHAR(255) NOT NULL,
    user_id_2 VARCHAR(255) NOT NULL,
    session_type VARCHAR(50) DEFAULT 'direct',
    encryption_key VARCHAR(255),
    created_ts BIGINT NOT NULL,
    last_activity_ts BIGINT NOT NULL,
    updated_ts BIGINT,
    unread_count INTEGER DEFAULT 0,
    encrypted_content TEXT,
    FOREIGN KEY (user_id_1) REFERENCES users(user_id) ON DELETE CASCADE,
    FOREIGN KEY (user_id_2) REFERENCES users(user_id) ON DELETE CASCADE,
    UNIQUE (user_id_1, user_id_2)
);

-- Private sessions indexes
CREATE INDEX IF NOT EXISTS idx_private_sessions_user1 ON private_sessions(user_id_1);
CREATE INDEX IF NOT EXISTS idx_private_sessions_user2 ON private_sessions(user_id_2);
CREATE INDEX IF NOT EXISTS idx_private_sessions_created ON private_sessions(created_ts);

-- Private messages: Private chat message storage
CREATE TABLE IF NOT EXISTS private_messages (
    id BIGSERIAL PRIMARY KEY,
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
    deleted_ts BIGINT,
    is_edited BOOLEAN DEFAULT FALSE,
    unread_count INTEGER DEFAULT 0,
    FOREIGN KEY (session_id) REFERENCES private_sessions(id) ON DELETE CASCADE,
    FOREIGN KEY (sender_id) REFERENCES users(user_id) ON DELETE CASCADE
);

-- Private messages critical composite indexes
CREATE INDEX IF NOT EXISTS idx_private_messages_session ON private_messages(session_id);
CREATE INDEX IF NOT EXISTS idx_private_messages_session_ts ON private_messages(session_id, created_ts DESC);
CREATE INDEX IF NOT EXISTS idx_private_messages_session_read ON private_messages(session_id, created_ts DESC, read_by_receiver);
CREATE INDEX IF NOT EXISTS idx_private_messages_sender ON private_messages(sender_id);
CREATE INDEX IF NOT EXISTS idx_private_messages_created ON private_messages(created_ts);

-- ============================================
-- SECTION 6: Presence and Status
-- ============================================

-- User presence: Online status tracking
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

-- Presence indexes
CREATE INDEX IF NOT EXISTS idx_presence ON presence(user_id);
CREATE INDEX IF NOT EXISTS idx_presence_status ON presence(presence);
CREATE INDEX IF NOT EXISTS idx_presence_last_active ON presence(last_active_ts DESC);

-- ============================================
-- SECTION 7: End-to-End Encryption Keys
-- ============================================

-- Device keys: User device public keys
CREATE TABLE IF NOT EXISTS device_keys (
    user_id VARCHAR(255) NOT NULL,
    device_id VARCHAR(255) NOT NULL,
    key_json JSONB NOT NULL,
    ts_added_ms BIGINT NOT NULL,
    ts_last_accessed BIGINT NOT NULL,
    verified BOOLEAN DEFAULT FALSE,
    blocked BOOLEAN DEFAULT FALSE,
    PRIMARY KEY (user_id, device_id),
    FOREIGN KEY (user_id, device_id) REFERENCES devices(device_id, user_id) ON DELETE CASCADE
);

-- Device keys indexes
CREATE INDEX IF NOT EXISTS idx_device_keys_user ON device_keys(user_id);
CREATE INDEX IF NOT EXISTS idx_device_keys_device ON device_keys(device_id);
CREATE INDEX IF NOT EXISTS idx_device_keys_verified ON device_keys(verified) WHERE verified = TRUE;
CREATE INDEX IF NOT EXISTS idx_device_keys_ts ON device_keys(ts_last_accessed);

-- One-time keys: Pre-key material for encryption
CREATE TABLE IF NOT EXISTS one_time_keys (
    user_id VARCHAR(255) NOT NULL,
    device_id VARCHAR(255) NOT NULL,
    key_id VARCHAR(255) NOT NULL,
    key_json JSONB NOT NULL,
    ts_created_ms BIGINT NOT NULL,
    exhausted BOOLEAN DEFAULT FALSE,
    signature_json TEXT,
    PRIMARY KEY (user_id, device_id, key_id),
    FOREIGN KEY (user_id, device_id) REFERENCES devices(device_id, user_id) ON DELETE CASCADE
);

-- One-time keys indexes
CREATE INDEX IF NOT EXISTS idx_one_time_keys_user ON one_time_keys(user_id);
CREATE INDEX IF NOT EXISTS idx_one_time_keys_device ON one_time_keys(device_id);
CREATE INDEX IF NOT EXISTS idx_one_time_keys_exhausted ON one_time_keys(exhausted) WHERE exhausted = FALSE;
CREATE INDEX IF NOT EXISTS idx_one_time_keys_created ON one_time_keys(ts_created_ms DESC);

-- Key backups: Encrypted key backup metadata
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

-- Key backups indexes
CREATE INDEX IF NOT EXISTS idx_key_backups_user ON key_backups(user_id);
CREATE INDEX IF NOT EXISTS idx_key_backups_version ON key_backups(version);

-- Backup keys: Individual key backup entries
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

-- Backup keys indexes
CREATE INDEX IF NOT EXISTS idx_backup_keys_user ON backup_keys(user_id);
CREATE INDEX IF NOT EXISTS idx_backup_keys_room ON backup_keys(room_id);

-- Cross-signing keys: Master/key-signing/user-signing keys
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

-- Cross-signing keys indexes
CREATE INDEX IF NOT EXISTS idx_cross_signing_user ON cross_signing_keys(user_id);
CREATE INDEX IF NOT EXISTS idx_cross_signing_type ON cross_signing_keys(key_type);

-- Signatures: Key signature storage
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

-- Signatures indexes
CREATE INDEX IF NOT EXISTS idx_signatures_user ON signatures(user_id);
CREATE INDEX IF NOT EXISTS idx_signatures_target ON signatures(target_user_id);

-- ============================================
-- SECTION 8: Event Signatures
-- ============================================

-- Event signatures: Event signature verification data
CREATE TABLE IF NOT EXISTS event_signatures (
    event_id VARCHAR(255) NOT NULL,
    algorithm VARCHAR(50) NOT NULL,
    key_id VARCHAR(255) NOT NULL,
    signature TEXT NOT NULL,
    PRIMARY KEY (event_id, algorithm, key_id),
    FOREIGN KEY (event_id) REFERENCES events(event_id) ON DELETE CASCADE
);

-- Event signatures indexes
CREATE INDEX IF NOT EXISTS idx_event_signatures_event ON event_signatures(event_id);

-- ============================================
-- SECTION 9: Room Directory and User Directory
-- ============================================

-- Room directory: Public room listing
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

-- Room directory indexes
CREATE INDEX IF NOT EXISTS idx_room_directory_public ON room_directory(is_public) WHERE is_public = TRUE;
CREATE INDEX IF NOT EXISTS idx_room_directory_category ON room_directory(primary_category);
CREATE INDEX IF NOT EXISTS idx_room_directory_searchable ON room_directory(searchable) WHERE searchable = TRUE;

-- User directory: Searchable user listing
CREATE TABLE IF NOT EXISTS user_directory (
    user_id VARCHAR(255) NOT NULL PRIMARY KEY,
    displayname VARCHAR(255),
    avatar_url VARCHAR(512),
    last_active_ts BIGINT,
    searchable BOOLEAN DEFAULT TRUE,
    UNIQUE (user_id),
    FOREIGN KEY (user_id) REFERENCES users(user_id) ON DELETE CASCADE
);

-- User directory indexes
CREATE INDEX IF NOT EXISTS idx_user_directory_searchable ON user_directory(searchable) WHERE searchable = TRUE;
CREATE INDEX IF NOT EXISTS idx_user_directory_name ON user_directory(displayname);
CREATE INDEX IF NOT EXISTS idx_user_directory_active ON user_directory(last_active_ts DESC);

-- User directory search: Full-text search auxiliary table
CREATE TABLE IF NOT EXISTS user_directory_search (
    user_id VARCHAR(255) NOT NULL PRIMARY KEY,
    ts_vector TSVECTOR,
    FOREIGN KEY (user_id) REFERENCES users(user_id) ON DELETE CASCADE
);

-- User directory search indexes
CREATE INDEX IF NOT EXISTS idx_user_search_vector ON user_directory_search USING GIN(ts_vector);

-- ============================================
-- SECTION 10: Additional Tables
-- ============================================

-- Voice messages: Voice message storage
CREATE TABLE IF NOT EXISTS voice_messages (
    id BIGSERIAL PRIMARY KEY,
    message_id VARCHAR(255) NOT NULL,
    room_id VARCHAR(255) NOT NULL,
    sender_id VARCHAR(255) NOT NULL,
    file_path VARCHAR(512),
    file_name VARCHAR(255),
    file_size BIGINT,
    duration_ms BIGINT,
    waveform JSONB,
    mime_type VARCHAR(100),
    encryption JSONB,
    created_ts BIGINT NOT NULL,
    processed BOOLEAN DEFAULT FALSE,
    processed_ts BIGINT,
    FOREIGN KEY (room_id) REFERENCES rooms(room_id) ON DELETE CASCADE,
    FOREIGN KEY (sender_id) REFERENCES users(user_id) ON DELETE CASCADE
);

-- Voice messages indexes
CREATE INDEX IF NOT EXISTS idx_voice_messages_room ON voice_messages(room_id);
CREATE INDEX IF NOT EXISTS idx_voice_messages_sender ON voice_messages(sender_id);
CREATE INDEX IF NOT EXISTS idx_voice_messages_created ON voice_messages(created_ts);

-- Friend categories: User-defined friend categories
CREATE TABLE IF NOT EXISTS friend_categories (
    id BIGSERIAL PRIMARY KEY,
    user_id VARCHAR(255) NOT NULL,
    name VARCHAR(255) NOT NULL,
    color VARCHAR(20),
    icon VARCHAR(100),
    sort_order BIGINT DEFAULT 0,
    created_ts BIGINT NOT NULL,
    FOREIGN KEY (user_id) REFERENCES users(user_id) ON DELETE CASCADE,
    UNIQUE (user_id, name)
);

-- Friend categories indexes
CREATE INDEX IF NOT EXISTS idx_friend_categories_user ON friend_categories(user_id);

-- Read markers: Message read position tracking
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

-- Read markers indexes
CREATE INDEX IF NOT EXISTS idx_read_markers_user ON read_markers(user_id);
CREATE INDEX IF NOT EXISTS idx_read_markers_room ON read_markers(room_id);

-- Room state: Room state storage
CREATE TABLE IF NOT EXISTS room_state (
    id BIGSERIAL PRIMARY KEY,
    room_id VARCHAR(255) NOT NULL,
    event_id VARCHAR(255) NOT NULL UNIQUE,
    state_key VARCHAR(255),
    room_state JSONB NOT NULL,
    created_ts BIGINT NOT NULL,
    FOREIGN KEY (room_id) REFERENCES rooms(room_id) ON DELETE CASCADE,
    FOREIGN KEY (event_id) REFERENCES events(event_id) ON DELETE CASCADE
);

-- Room state indexes
CREATE INDEX IF NOT EXISTS idx_room_state_room ON room_state(room_id);
CREATE INDEX IF NOT EXISTS idx_room_state_event ON room_state(event_id);
CREATE INDEX IF NOT EXISTS idx_room_state_key ON room_state(room_id, state_key);

-- Room account data: Per-room user data storage
CREATE TABLE IF NOT EXISTS room_account_data (
    id BIGSERIAL PRIMARY KEY,
    user_id VARCHAR(255) NOT NULL,
    room_id VARCHAR(255) NOT NULL,
    event_type VARCHAR(255) NOT NULL,
    data_type VARCHAR(255) NOT NULL,
    content JSONB NOT NULL,
    created_ts BIGINT NOT NULL,
    updated_ts BIGINT,
    FOREIGN KEY (user_id) REFERENCES users(user_id) ON DELETE CASCADE,
    FOREIGN KEY (room_id) REFERENCES rooms(room_id) ON DELETE CASCADE,
    UNIQUE (user_id, room_id, data_type)
);

-- Room account data indexes
CREATE INDEX IF NOT EXISTS idx_room_account_data_user ON room_account_data(user_id);
CREATE INDEX IF NOT EXISTS idx_room_account_data_room ON room_account_data(room_id);

-- User account data: Per-user data storage
CREATE TABLE IF NOT EXISTS user_account_data (
    id BIGSERIAL PRIMARY KEY,
    user_id VARCHAR(255) NOT NULL,
    data_type VARCHAR(255) NOT NULL,
    content JSONB NOT NULL,
    created_ts BIGINT NOT NULL,
    updated_ts BIGINT,
    FOREIGN KEY (user_id) REFERENCES users(user_id) ON DELETE CASCADE,
    UNIQUE (user_id, data_type)
);

-- User account data indexes
CREATE INDEX IF NOT EXISTS idx_user_account_data_user ON user_account_data(user_id);

-- User rooms: User-room association
CREATE TABLE IF NOT EXISTS user_rooms (
    id BIGSERIAL PRIMARY KEY,
    user_id VARCHAR(255) NOT NULL,
    room_id VARCHAR(255) NOT NULL,
    is_hidden BOOLEAN DEFAULT FALSE,
    is_pinned BOOLEAN DEFAULT FALSE,
    notification_level VARCHAR(50) DEFAULT 'mention',
    hide_notification_settings BOOLEAN DEFAULT FALSE,
    created_ts BIGINT NOT NULL,
    updated_ts BIGINT,
    FOREIGN KEY (user_id) REFERENCES users(user_id) ON DELETE CASCADE,
    FOREIGN KEY (room_id) REFERENCES rooms(room_id) ON DELETE CASCADE,
    UNIQUE (user_id, room_id)
);

-- User rooms indexes
CREATE INDEX IF NOT EXISTS idx_user_rooms_user ON user_rooms(user_id);
CREATE INDEX IF NOT EXISTS idx_user_rooms_room ON user_rooms(room_id);

-- Typing: Typing indicator tracking
CREATE TABLE IF NOT EXISTS typing (
    id BIGSERIAL PRIMARY KEY,
    room_id VARCHAR(255) NOT NULL,
    user_id VARCHAR(255) NOT NULL,
    typing BOOLEAN DEFAULT FALSE,
    last_active_ts BIGINT NOT NULL,
    timeout_ts BIGINT,
    FOREIGN KEY (room_id) REFERENCES rooms(room_id) ON DELETE CASCADE,
    FOREIGN KEY (user_id) REFERENCES users(user_id) ON DELETE CASCADE,
    UNIQUE (room_id, user_id)
);

-- Typing indexes
CREATE INDEX IF NOT EXISTS idx_typing_room ON typing(room_id);
CREATE INDEX IF NOT EXISTS idx_typing_user ON typing(user_id);

-- Room key distributions: Key distribution tracking
CREATE TABLE IF NOT EXISTS room_key_distributions (
    id BIGSERIAL PRIMARY KEY,
    room_id VARCHAR(255) NOT NULL,
    event_id VARCHAR(255) NOT NULL,
    created_ts BIGINT NOT NULL,
    content JSONB NOT NULL,
    FOREIGN KEY (room_id) REFERENCES rooms(room_id) ON DELETE CASCADE,
    FOREIGN KEY (event_id) REFERENCES events(event_id) ON DELETE CASCADE
);

-- Room key distributions indexes
CREATE INDEX IF NOT EXISTS idx_room_key_distributions_room ON room_key_distributions(room_id);
CREATE INDEX IF NOT EXISTS idx_room_key_distributions_event ON room_key_distributions(event_id);

-- Session keys: Private chat session keys
CREATE TABLE IF NOT EXISTS session_keys (
    id BIGSERIAL PRIMARY KEY,
    session_id VARCHAR(255) NOT NULL,
    sender_id VARCHAR(255) NOT NULL,
    key_data TEXT NOT NULL,
    created_ts BIGINT NOT NULL
);

-- Session keys indexes
CREATE INDEX IF NOT EXISTS idx_session_keys_session ON session_keys(session_id);
CREATE INDEX IF NOT EXISTS idx_session_keys_sender ON session_keys(sender_id);

-- IP blocks: IP blocking configuration
CREATE TABLE IF NOT EXISTS ip_blocks (
    id BIGSERIAL PRIMARY KEY,
    ip_range CIDR NOT NULL,
    ip_address INET,
    reason TEXT,
    blocked_at BIGINT NOT NULL,
    blocked_ts BIGINT NOT NULL,
    expires_at BIGINT,
    expires_ts BIGINT
);

-- IP blocks indexes
CREATE INDEX IF NOT EXISTS idx_ip_blocks_range ON ip_blocks(ip_range);

-- IP reputation: IP reputation scores
CREATE TABLE IF NOT EXISTS ip_reputation (
    id BIGSERIAL PRIMARY KEY,
    ip_address INET NOT NULL,
    score INTEGER DEFAULT 50,
    reputation_score INTEGER DEFAULT 50,
    threat_level VARCHAR(50) DEFAULT 'none',
    last_seen_at BIGINT,
    updated_at BIGINT,
    details TEXT,
    last_updated_ts BIGINT NOT NULL,
    created_ts BIGINT NOT NULL,
    UNIQUE (ip_address)
);

-- IP reputation indexes
CREATE INDEX IF NOT EXISTS idx_ip_reputation_ip ON ip_reputation(ip_address);
CREATE INDEX IF NOT EXISTS idx_ip_reputation_score ON ip_reputation(score);

-- Key changes: Key change tracking
CREATE TABLE IF NOT EXISTS key_changes (
    id BIGSERIAL PRIMARY KEY,
    user_id VARCHAR(255) NOT NULL,
    device_id VARCHAR(255) NOT NULL,
    change_type VARCHAR(50) NOT NULL,
    changed_ts BIGINT NOT NULL,
    content JSONB,
    FOREIGN KEY (user_id) REFERENCES users(user_id) ON DELETE CASCADE
);

-- Key changes indexes
CREATE INDEX IF NOT EXISTS idx_key_changes_user ON key_changes(user_id);
CREATE INDEX IF NOT EXISTS idx_key_changes_device ON key_changes(device_id);
CREATE INDEX IF NOT EXISTS idx_key_changes_ts ON key_changes(changed_ts DESC);

-- Security events: Security event logging
CREATE TABLE IF NOT EXISTS security_events (
    id BIGSERIAL PRIMARY KEY,
    event_type VARCHAR(255) NOT NULL,
    user_id VARCHAR(255),
    ip_address INET,
    user_agent TEXT,
    details TEXT,
    created_at BIGINT NOT NULL,
    severity VARCHAR(50) DEFAULT 'warning',
    description TEXT,
    created_ts BIGINT NOT NULL,
    resolved BOOLEAN DEFAULT FALSE,
    resolved_ts BIGINT,
    resolved_by VARCHAR(255)
);

-- Security events indexes
CREATE INDEX IF NOT EXISTS idx_security_events_user ON security_events(user_id);
CREATE INDEX IF NOT EXISTS idx_security_events_type ON security_events(event_type);
CREATE INDEX IF NOT EXISTS idx_security_events_severity ON security_events(severity);
CREATE INDEX IF NOT EXISTS idx_security_events_created ON security_events(created_ts DESC);

-- Blocked users: User blocking relationships
CREATE TABLE IF NOT EXISTS blocked_users (
    user_id VARCHAR(255) NOT NULL,
    blocked_user_id VARCHAR(255) NOT NULL,
    reason TEXT,
    created_ts BIGINT NOT NULL,
    PRIMARY KEY (user_id, blocked_user_id),
    FOREIGN KEY (user_id) REFERENCES users(user_id) ON DELETE CASCADE,
    FOREIGN KEY (blocked_user_id) REFERENCES users(user_id) ON DELETE CASCADE
);

-- Blocked users indexes
CREATE INDEX IF NOT EXISTS idx_blocked_users_user ON blocked_users(user_id);
CREATE INDEX IF NOT EXISTS idx_blocked_users_blocked_user ON blocked_users(blocked_user_id);

-- Rate limiting: Rate limit configuration
CREATE TABLE IF NOT EXISTS ratelimit (
    id BIGSERIAL PRIMARY KEY,
    user_id VARCHAR(255),
    room_id VARCHAR(255),
    action VARCHAR(100) NOT NULL,
    rate_limit_type VARCHAR(50) DEFAULT 'user',
    limit_count BIGINT DEFAULT 10,
    limit_window BIGINT DEFAULT 1000,
    current_count BIGINT DEFAULT 0,
    window_start BIGINT NOT NULL,
    created_ts BIGINT NOT NULL,
    updated_ts BIGINT,
    FOREIGN KEY (user_id) REFERENCES users(user_id) ON DELETE CASCADE,
    FOREIGN KEY (room_id) REFERENCES rooms(room_id) ON DELETE CASCADE
);

-- Rate limiting indexes
CREATE INDEX IF NOT EXISTS idx_ratelimit_user ON ratelimit(user_id);
CREATE INDEX IF NOT EXISTS idx_ratelimit_room ON ratelimit(room_id);
CREATE INDEX IF NOT EXISTS idx_ratelimit_action ON ratelimit(action);
CREATE INDEX IF NOT EXISTS idx_ratelimit_window ON ratelimit(window_start);

-- Database metadata: Application metadata storage
CREATE TABLE IF NOT EXISTS db_metadata (
    id BIGSERIAL PRIMARY KEY,
    key VARCHAR(255) NOT NULL UNIQUE,
    value TEXT NOT NULL,
    created_ts BIGINT NOT NULL,
    updated_ts BIGINT
);

-- Database metadata indexes
CREATE INDEX IF NOT EXISTS idx_db_metadata_key ON db_metadata(key);

-- Voice usage stats: Voice usage statistics
CREATE TABLE IF NOT EXISTS voice_usage_stats (
    id BIGSERIAL PRIMARY KEY,
    user_id VARCHAR(255) NOT NULL,
    room_id VARCHAR(255) NOT NULL,
    total_duration_ms BIGINT DEFAULT 0,
    total_file_size BIGINT DEFAULT 0,
    message_count BIGINT DEFAULT 0,
    last_activity_ts BIGINT NOT NULL,
    last_active_ts BIGINT NOT NULL,
    date DATE NOT NULL,
    period_start DATE NOT NULL,
    period_end DATE NOT NULL,
    created_ts BIGINT NOT NULL,
    updated_ts BIGINT,
    FOREIGN KEY (user_id) REFERENCES users(user_id) ON DELETE CASCADE,
    FOREIGN KEY (room_id) REFERENCES rooms(room_id) ON DELETE CASCADE,
    UNIQUE (user_id, room_id, period_start)
);

-- Voice usage stats indexes
CREATE INDEX IF NOT EXISTS idx_voice_usage_user ON voice_usage_stats(user_id);
CREATE INDEX IF NOT EXISTS idx_voice_usage_room ON voice_usage_stats(room_id);
CREATE INDEX IF NOT EXISTS idx_voice_usage_period ON voice_usage_stats(period_start);

-- ============================================
-- SECTION 11: Tracking and Monitoring Tables
-- ============================================

-- Initialization tracking: Track initialization completion
CREATE TABLE IF NOT EXISTS initial_sync_ (
    id VARCHAR(255) NOT NULL,
    room_id VARCHAR(255),
    user_id VARCHAR(255),
    stream_id BIGINT NOT NULL,
    instance_name VARCHAR(255) NOT NULL,
    PRIMARY KEY (id, instance_name),
    FOREIGN KEY (room_id) REFERENCES rooms(room_id) ON DELETE CASCADE,
    FOREIGN KEY (user_id) REFERENCES users(user_id) ON DELETE CASCADE
);

-- ============================================
-- SECTION 12: Helper Functions and Views
-- ============================================

-- View: Active user sessions
CREATE OR REPLACE VIEW active_sessions AS
SELECT 
    u.user_id,
    u.username,
    COUNT(at.id) as active_session_count,
    MAX(at.last_used_ts) as last_activity
FROM users u
LEFT JOIN access_tokens at ON u.user_id = at.user_id 
    AND at.expires_ts > EXTRACT(EPOCH FROM NOW())::BIGINT * 1000
GROUP BY u.user_id, u.username;

-- View: Room statistics
CREATE OR REPLACE VIEW room_stats AS
SELECT 
    r.room_id,
    r.name,
    r.is_public,
    r.encryption,
    r.creation_ts,
    r.member_count,
    COUNT(DISTINCT rm.user_id) as current_member_count,
    COUNT(e.event_id) as message_count
FROM rooms r
LEFT JOIN room_memberships rm ON r.room_id = rm.room_id
LEFT JOIN events e ON r.room_id = e.room_id 
    AND e.event_type NOT IN ('m.room.member', 'm.room.aliases', 'm.room.canonical_alias')
GROUP BY r.room_id, r.name, r.is_public, r.encryption, r.creation_ts, r.member_count;

-- ============================================
-- SECTION 13: Comments and Documentation
-- ============================================

-- Table comments for better database documentation
COMMENT ON TABLE users IS 'Core user information storage following Matrix protocol user ID format @username:servername';
COMMENT ON TABLE devices IS 'User device information including device keys for end-to-end encryption';
COMMENT ON TABLE access_tokens IS 'OAuth2-style access tokens for user authentication';
COMMENT ON TABLE refresh_tokens IS 'Refresh tokens for access token renewal';
COMMENT ON TABLE rooms IS 'Room metadata following Matrix protocol room ID format !roomid:servername';
COMMENT ON TABLE room_memberships IS 'User membership tracking in Matrix rooms with various membership types';
COMMENT ON TABLE events IS 'Room message events storage - highest volume table in the system';
COMMENT ON TABLE room_aliases IS 'Room alias to room ID mapping for friendly room access';
COMMENT ON TABLE friends IS 'Bidirectional friend relationships between users';
COMMENT ON TABLE friend_requests IS 'Pending friend requests tracking';
COMMENT ON TABLE private_sessions IS 'Private chat session metadata between two users';
COMMENT ON TABLE private_messages IS 'Private chat messages - high write volume table';
COMMENT ON TABLE presence IS 'User online status and presence information';
COMMENT ON TABLE device_keys IS 'Device public keys for end-to-end encryption verification';
COMMENT ON TABLE one_time_keys IS 'Pre-key material for encrypted message exchange';
COMMENT ON TABLE key_backups IS 'Encrypted key backup metadata for cross-device key recovery';
COMMENT ON TABLE backup_keys IS 'Individual encrypted key backup entries';
COMMENT ON TABLE cross_signing_keys IS 'Master, user-signing, and device-signing keys';
COMMENT ON TABLE signatures IS 'Key signatures for verification chain';
COMMENT ON TABLE event_signatures IS 'Event signatures for message verification';
COMMENT ON TABLE room_directory IS 'Public room directory for room discovery';
COMMENT ON TABLE user_directory IS 'Searchable user directory';
COMMENT ON TABLE initial_sync_ IS 'Stream synchronization tracking';
