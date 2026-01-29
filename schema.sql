DROP TABLE IF EXISTS room_events, events, presence, room_memberships, rooms, refresh_tokens, access_tokens, devices, users CASCADE;

CREATE TABLE IF NOT EXISTS users (
    user_id TEXT NOT NULL PRIMARY KEY,
    username TEXT NOT NULL UNIQUE,
    password_hash TEXT,
    admin BOOLEAN DEFAULT FALSE,
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

CREATE TABLE IF NOT EXISTS devices (
    device_id TEXT NOT NULL PRIMARY KEY,
    user_id TEXT NOT NULL,
    display_name TEXT,
    last_seen_ts BIGINT NOT NULL,
    last_seen_ip TEXT,
    created_ts BIGINT NOT NULL,
    ignored_user_list TEXT,
    appservice_id TEXT,
    first_seen_ts BIGINT DEFAULT 0,
    FOREIGN KEY (user_id) REFERENCES users(user_id) ON DELETE CASCADE
);

CREATE TABLE IF NOT EXISTS access_tokens (
    id BIGSERIAL PRIMARY KEY,
    token TEXT NOT NULL UNIQUE,
    user_id TEXT NOT NULL,
    device_id TEXT,
    expires_ts BIGINT,
    created_ts BIGINT NOT NULL,
    invalidated_ts BIGINT,
    expired_ts BIGINT,
    FOREIGN KEY (user_id) REFERENCES users(user_id) ON DELETE CASCADE,
    FOREIGN KEY (device_id) REFERENCES devices(device_id) ON DELETE CASCADE
);

CREATE TABLE IF NOT EXISTS refresh_tokens (
    id BIGSERIAL PRIMARY KEY,
    token TEXT NOT NULL UNIQUE,
    user_id TEXT NOT NULL,
    device_id TEXT,
    expires_ts BIGINT,
    created_ts BIGINT NOT NULL,
    invalidated_ts BIGINT,
    expired_ts BIGINT,
    FOREIGN KEY (user_id) REFERENCES users(user_id) ON DELETE CASCADE,
    FOREIGN KEY (device_id) REFERENCES devices(device_id) ON DELETE CASCADE
);

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
    member_count INTEGER DEFAULT 0
);

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
    join_reason TEXT,
    banned_by TEXT,
    ban_reason TEXT,
    ban_ts BIGINT,
    PRIMARY KEY (room_id, user_id),
    FOREIGN KEY (room_id) REFERENCES rooms(room_id) ON DELETE CASCADE,
    FOREIGN KEY (user_id) REFERENCES users(user_id) ON DELETE CASCADE
);

CREATE TABLE IF NOT EXISTS events (
    event_id TEXT NOT NULL PRIMARY KEY,
    room_id TEXT NOT NULL,
    user_id TEXT NOT NULL,
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
    sender TEXT NOT NULL,
    redacted BOOLEAN DEFAULT FALSE,
    FOREIGN KEY (room_id) REFERENCES rooms(room_id) ON DELETE CASCADE,
    FOREIGN KEY (user_id) REFERENCES users(user_id) ON DELETE CASCADE
);

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

CREATE TABLE IF NOT EXISTS user_directory (
    user_id TEXT NOT NULL,
    room_id TEXT NOT NULL,
    visibility TEXT NOT NULL DEFAULT 'private',
    added_by TEXT,
    created_ts BIGINT NOT NULL,
    PRIMARY KEY (user_id, room_id),
    FOREIGN KEY (user_id) REFERENCES users(user_id) ON DELETE CASCADE,
    FOREIGN KEY (room_id) REFERENCES rooms(room_id) ON DELETE CASCADE
);

GRANT ALL ON ALL TABLES IN SCHEMA public TO synapse_user;
GRANT ALL ON ALL SEQUENCES IN SCHEMA public TO synapse_user;

CREATE TABLE IF NOT EXISTS device_keys (
    id UUID NOT NULL PRIMARY KEY,
    user_id TEXT NOT NULL,
    device_id TEXT NOT NULL,
    display_name TEXT,
    algorithm TEXT NOT NULL,
    key_id TEXT NOT NULL,
    public_key TEXT NOT NULL,
    signatures JSONB NOT NULL DEFAULT '{}',
    created_at TIMESTAMP WITH TIME ZONE NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMP WITH TIME ZONE NOT NULL DEFAULT NOW(),
    UNIQUE(user_id, device_id, key_id)
);

CREATE TABLE IF NOT EXISTS cross_signing_keys (
    id UUID NOT NULL PRIMARY KEY,
    user_id TEXT NOT NULL,
    key_type TEXT NOT NULL,
    public_key TEXT NOT NULL,
    usage TEXT[] NOT NULL,
    signatures JSONB NOT NULL DEFAULT '{}',
    created_at TIMESTAMP WITH TIME ZONE NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMP WITH TIME ZONE NOT NULL DEFAULT NOW(),
    UNIQUE(user_id, key_type)
);

CREATE TABLE IF NOT EXISTS megolm_sessions (
    id UUID NOT NULL PRIMARY KEY,
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

CREATE TABLE IF NOT EXISTS inbound_megolm_sessions (
    id UUID NOT NULL PRIMARY KEY,
    session_id TEXT NOT NULL UNIQUE,
    sender_key TEXT NOT NULL,
    room_id TEXT NOT NULL,
    session_key TEXT NOT NULL,
    algorithm TEXT NOT NULL DEFAULT 'm.megolm.v1.aes-sha2',
    message_index BIGINT NOT NULL DEFAULT 0,
    created_at TIMESTAMP WITH TIME ZONE NOT NULL DEFAULT NOW(),
    expires_at TIMESTAMP WITH TIME ZONE
);

CREATE TABLE IF NOT EXISTS key_backups (
    id UUID NOT NULL PRIMARY KEY,
    user_id TEXT NOT NULL,
    version TEXT NOT NULL,
    algorithm TEXT NOT NULL DEFAULT 'm.megolm_backup.v1',
    auth_data JSONB NOT NULL,
    encrypted_data JSONB NOT NULL,
    created_at TIMESTAMP WITH TIME ZONE NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMP WITH TIME ZONE NOT NULL DEFAULT NOW(),
    UNIQUE(user_id, version)
);

CREATE TABLE IF NOT EXISTS backup_keys (
    id UUID NOT NULL PRIMARY KEY,
    backup_id UUID NOT NULL,
    room_id TEXT NOT NULL,
    session_id TEXT NOT NULL,
    first_message_index BIGINT NOT NULL,
    forwarded_count BIGINT NOT NULL DEFAULT 0,
    is_verified BOOLEAN NOT NULL DEFAULT FALSE,
    session_data TEXT NOT NULL,
    created_at TIMESTAMP WITH TIME ZONE NOT NULL DEFAULT NOW(),
    FOREIGN KEY (backup_id) REFERENCES key_backups(id) ON DELETE CASCADE,
    UNIQUE(backup_id, room_id, session_id)
);

CREATE INDEX IF NOT EXISTS idx_device_keys_user_id ON device_keys(user_id);
CREATE INDEX IF NOT EXISTS idx_device_keys_device_id ON device_keys(device_id);
CREATE INDEX IF NOT EXISTS idx_cross_signing_keys_user_id ON cross_signing_keys(user_id);
CREATE INDEX IF NOT EXISTS idx_megolm_sessions_room_id ON megolm_sessions(room_id);
CREATE INDEX IF NOT EXISTS idx_megolm_sessions_sender_key ON megolm_sessions(sender_key);
CREATE INDEX IF NOT EXISTS idx_inbound_megolm_sessions_sender_key ON inbound_megolm_sessions(sender_key);
CREATE INDEX IF NOT EXISTS idx_key_backups_user_id ON key_backups(user_id);
CREATE INDEX IF NOT EXISTS idx_backup_keys_backup_id ON backup_keys(backup_id);

CREATE TABLE IF NOT EXISTS friends (
    id BIGSERIAL PRIMARY KEY,
    user_id TEXT NOT NULL,
    friend_id TEXT NOT NULL,
    created_ts BIGINT NOT NULL,
    UNIQUE(user_id, friend_id),
    FOREIGN KEY (user_id) REFERENCES users(user_id) ON DELETE CASCADE,
    FOREIGN KEY (friend_id) REFERENCES users(user_id) ON DELETE CASCADE
);

CREATE TABLE IF NOT EXISTS friend_requests (
    id BIGSERIAL PRIMARY KEY,
    sender_id TEXT NOT NULL,
    receiver_id TEXT NOT NULL,
    message TEXT,
    status TEXT NOT NULL DEFAULT 'pending',
    created_ts BIGINT NOT NULL,
    updated_ts BIGINT,
    UNIQUE(sender_id, receiver_id),
    FOREIGN KEY (sender_id) REFERENCES users(user_id) ON DELETE CASCADE,
    FOREIGN KEY (receiver_id) REFERENCES users(user_id) ON DELETE CASCADE
);

CREATE TABLE IF NOT EXISTS friend_categories (
    id BIGSERIAL PRIMARY KEY,
    user_id TEXT NOT NULL,
    name TEXT NOT NULL,
    color TEXT NOT NULL DEFAULT '#000000',
    created_ts BIGINT NOT NULL,
    FOREIGN KEY (user_id) REFERENCES users(user_id) ON DELETE CASCADE
);

CREATE TABLE IF NOT EXISTS blocked_users (
    id BIGSERIAL PRIMARY KEY,
    user_id TEXT NOT NULL,
    blocked_id TEXT NOT NULL,
    reason TEXT,
    created_ts BIGINT NOT NULL,
    UNIQUE(user_id, blocked_id),
    FOREIGN KEY (user_id) REFERENCES users(user_id) ON DELETE CASCADE,
    FOREIGN KEY (blocked_id) REFERENCES users(user_id) ON DELETE CASCADE
);

CREATE TABLE IF NOT EXISTS private_sessions (
    id BIGSERIAL PRIMARY KEY,
    user_id_1 TEXT NOT NULL,
    user_id_2 TEXT NOT NULL,
    last_message TEXT,
    unread_count INTEGER DEFAULT 0,
    created_ts BIGINT NOT NULL,
    updated_ts BIGINT,
    FOREIGN KEY (user_id_1) REFERENCES users(user_id) ON DELETE CASCADE,
    FOREIGN KEY (user_id_2) REFERENCES users(user_id) ON DELETE CASCADE,
    UNIQUE(user_id_1, user_id_2)
);

CREATE TABLE IF NOT EXISTS private_messages (
    id BIGSERIAL PRIMARY KEY,
    session_id BIGINT NOT NULL,
    sender_id TEXT NOT NULL,
    content TEXT NOT NULL,
    encrypted_content TEXT,
    message_type TEXT DEFAULT 'text',
    is_read BOOLEAN DEFAULT FALSE,
    read_by_receiver BOOLEAN DEFAULT FALSE,
    created_ts BIGINT NOT NULL,
    FOREIGN KEY (session_id) REFERENCES private_sessions(id) ON DELETE CASCADE,
    FOREIGN KEY (sender_id) REFERENCES users(user_id) ON DELETE CASCADE
);

CREATE TABLE IF NOT EXISTS voice_messages (
    id BIGSERIAL PRIMARY KEY,
    message_id TEXT NOT NULL UNIQUE,
    user_id TEXT NOT NULL,
    room_id TEXT,
    session_id BIGINT,
    file_path TEXT NOT NULL,
    content_type TEXT NOT NULL,
    duration_ms INTEGER NOT NULL,
    file_size BIGINT NOT NULL,
    waveform_data TEXT,
    transcribe_text TEXT,
    created_ts BIGINT NOT NULL,
    FOREIGN KEY (user_id) REFERENCES users(user_id) ON DELETE CASCADE,
    FOREIGN KEY (session_id) REFERENCES private_sessions(id) ON DELETE SET NULL
);

CREATE TABLE IF NOT EXISTS voice_usage_stats (
    id BIGSERIAL PRIMARY KEY,
    user_id TEXT NOT NULL,
    room_id TEXT,
    total_duration_ms BIGINT DEFAULT 0,
    total_count INTEGER DEFAULT 0,
    last_used_ts BIGINT,
    FOREIGN KEY (user_id) REFERENCES users(user_id) ON DELETE CASCADE
);

CREATE INDEX IF NOT EXISTS idx_friends_user_id ON friends(user_id);
CREATE INDEX IF NOT EXISTS idx_friend_requests_sender ON friend_requests(sender_id);
CREATE INDEX IF NOT EXISTS idx_friend_requests_receiver ON friend_requests(receiver_id);
CREATE INDEX IF NOT EXISTS idx_blocked_users_user_id ON blocked_users(user_id);
CREATE INDEX IF NOT EXISTS idx_private_sessions_user ON private_sessions(user_id_1, user_id_2);
CREATE INDEX IF NOT EXISTS idx_private_messages_session ON private_messages(session_id);
CREATE INDEX IF NOT EXISTS idx_voice_messages_user_id ON voice_messages(user_id);
CREATE INDEX IF NOT EXISTS idx_voice_messages_room_id ON voice_messages(room_id);
