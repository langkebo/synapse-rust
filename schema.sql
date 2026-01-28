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
