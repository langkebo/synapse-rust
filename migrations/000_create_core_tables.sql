-- Create users table
CREATE TABLE IF NOT EXISTS users (
    user_id VARCHAR(255) PRIMARY KEY,
    username VARCHAR(255) NOT NULL UNIQUE,
    password_hash VARCHAR(255),
    displayname VARCHAR(255),
    avatar_url VARCHAR(512),
    admin BOOLEAN NOT NULL DEFAULT FALSE,
    deactivated BOOLEAN NOT NULL DEFAULT FALSE,
    is_guest BOOLEAN NOT NULL DEFAULT FALSE,
    consent_version VARCHAR(255),
    appservice_id VARCHAR(255),
    user_type VARCHAR(50),
    shadow_banned BOOLEAN NOT NULL DEFAULT FALSE,
    generation BIGINT NOT NULL DEFAULT 0,
    invalid_update_ts BIGINT,
    migration_state VARCHAR(50),
    creation_ts BIGINT NOT NULL
);

CREATE INDEX IF NOT EXISTS idx_users_username ON users(username);
CREATE INDEX IF NOT EXISTS idx_users_creation_ts ON users(creation_ts DESC);

-- Create devices table
CREATE TABLE IF NOT EXISTS devices (
    device_id VARCHAR(255) PRIMARY KEY,
    user_id VARCHAR(255) NOT NULL,
    display_name VARCHAR(255),
    device_key JSONB,
    last_seen_ip VARCHAR(50),
    last_seen_ts BIGINT,
    created_at BIGINT NOT NULL,
    CONSTRAINT fk_devices_user FOREIGN KEY (user_id) REFERENCES users(user_id) ON DELETE CASCADE
);

CREATE INDEX IF NOT EXISTS idx_devices_user ON devices(user_id);
CREATE INDEX IF NOT EXISTS idx_devices_last_seen ON devices(last_seen_ts);

-- Create rooms table
CREATE TABLE IF NOT EXISTS rooms (
    room_id VARCHAR(255) PRIMARY KEY,
    creator VARCHAR(255) NOT NULL,
    is_public BOOLEAN NOT NULL DEFAULT FALSE,
    name VARCHAR(255),
    topic VARCHAR(512),
    avatar_url VARCHAR(512),
    join_rules VARCHAR(50) NOT NULL DEFAULT 'invite',
    history_visibility VARCHAR(50) NOT NULL DEFAULT 'shared',
    encryption VARCHAR(100),
    creation_ts BIGINT NOT NULL,
    last_activity_ts BIGINT NOT NULL
);

CREATE INDEX IF NOT EXISTS idx_rooms_creator ON rooms(creator);
CREATE INDEX IF NOT EXISTS idx_rooms_activity ON rooms(last_activity_ts DESC);

-- Create room_memberships table
CREATE TABLE IF NOT EXISTS room_memberships (
    id BIGSERIAL PRIMARY KEY,
    room_id VARCHAR(255) NOT NULL,
    user_id VARCHAR(255) NOT NULL,
    sender VARCHAR(255) NOT NULL,
    membership VARCHAR(50) NOT NULL DEFAULT 'join',
    event_id VARCHAR(255) NOT NULL,
    displayname VARCHAR(255),
    avatar_url VARCHAR(512),
    is_hidden BOOLEAN NOT NULL DEFAULT FALSE,
    is_banned BOOLEAN NOT NULL DEFAULT FALSE,
    ban_reason VARCHAR(512),
    power_level BIGINT NOT NULL DEFAULT 0,
    joined_at BIGINT,
    left_at BIGINT,
    CONSTRAINT fk_memberships_room FOREIGN KEY (room_id) REFERENCES rooms(room_id) ON DELETE CASCADE,
    CONSTRAINT fk_memberships_user FOREIGN KEY (user_id) REFERENCES users(user_id) ON DELETE CASCADE,
    CONSTRAINT fk_memberships_sender FOREIGN KEY (sender) REFERENCES users(user_id) ON DELETE CASCADE,
    CONSTRAINT uk_membership UNIQUE (room_id, user_id)
);

CREATE INDEX IF NOT EXISTS idx_memberships_room ON room_memberships(room_id);
CREATE INDEX IF NOT EXISTS idx_memberships_user ON room_memberships(user_id);
CREATE INDEX IF NOT EXISTS idx_memberships_membership ON room_memberships(membership);

-- Create events table
CREATE TABLE IF NOT EXISTS events (
    event_id VARCHAR(255) PRIMARY KEY,
    room_id VARCHAR(255) NOT NULL,
    type VARCHAR(255) NOT NULL,
    content JSONB NOT NULL DEFAULT '{}',
    sender VARCHAR(255) NOT NULL,
    unsigned JSONB NOT NULL DEFAULT '{}',
    redacted BOOLEAN NOT NULL DEFAULT FALSE,
    origin_server_ts BIGINT NOT NULL,
    CONSTRAINT fk_events_room FOREIGN KEY (room_id) REFERENCES rooms(room_id) ON DELETE CASCADE,
    CONSTRAINT fk_events_sender FOREIGN KEY (sender) REFERENCES users(user_id) ON DELETE CASCADE
);

CREATE INDEX IF NOT EXISTS idx_events_room ON events(room_id);
CREATE INDEX IF NOT EXISTS idx_events_sender ON events(sender);
CREATE INDEX IF NOT EXISTS idx_events_type ON events(type);
CREATE INDEX IF NOT EXISTS idx_events_ts ON events(origin_server_ts DESC);

-- Create presence table
CREATE TABLE IF NOT EXISTS presence (
    id BIGSERIAL PRIMARY KEY,
    user_id VARCHAR(255) NOT NULL,
    presence VARCHAR(50) NOT NULL DEFAULT 'offline',
    status_msg VARCHAR(255),
    last_active_ts BIGINT NOT NULL,
    CONSTRAINT fk_presence_user FOREIGN KEY (user_id) REFERENCES users(user_id) ON DELETE CASCADE,
    CONSTRAINT uk_presence_user UNIQUE (user_id)
);

CREATE INDEX IF NOT EXISTS idx_presence ON presence(user_id);

-- Create ratelimit table
CREATE TABLE IF NOT EXISTS ratelimit (
    id BIGSERIAL PRIMARY KEY,
    user_id VARCHAR(255),
    ip_address VARCHAR(50),
    endpoint VARCHAR(255) NOT NULL,
    request_count BIGINT NOT NULL DEFAULT 0,
    window_start BIGINT NOT NULL,
    window_size BIGINT NOT NULL
);

CREATE INDEX IF NOT EXISTS idx_ratelimit_user ON ratelimit(user_id);
CREATE INDEX IF NOT EXISTS idx_ratelimit_ip ON ratelimit(ip_address);
CREATE INDEX IF NOT EXISTS idx_ratelimit_endpoint ON ratelimit(endpoint);
