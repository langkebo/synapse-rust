-- Create access_tokens table
CREATE TABLE IF NOT EXISTS access_tokens (
    id BIGSERIAL PRIMARY KEY,
    token VARCHAR(255) NOT NULL UNIQUE,
    user_id VARCHAR(255) NOT NULL,
    device_id VARCHAR(255),
    created_ts BIGINT NOT NULL,
    expires_ts BIGINT,
    invalidated_ts BIGINT,
    CONSTRAINT fk_access_tokens_user FOREIGN KEY (user_id) REFERENCES users(user_id) ON DELETE CASCADE,
    CONSTRAINT fk_access_tokens_device FOREIGN KEY (device_id) REFERENCES devices(device_id) ON DELETE CASCADE
);

CREATE INDEX IF NOT EXISTS idx_access_tokens_user ON access_tokens(user_id);
CREATE INDEX IF NOT EXISTS idx_access_tokens_token ON access_tokens(token);
CREATE INDEX IF NOT EXISTS idx_access_tokens_expiry ON access_tokens(expires_ts);

-- Create refresh_tokens table
CREATE TABLE IF NOT EXISTS refresh_tokens (
    id BIGSERIAL PRIMARY KEY,
    token VARCHAR(255) NOT NULL UNIQUE,
    user_id VARCHAR(255) NOT NULL,
    device_id VARCHAR(255),
    created_ts BIGINT NOT NULL,
    expires_ts BIGINT,
    invalidated_ts BIGINT,
    CONSTRAINT fk_refresh_tokens_user FOREIGN KEY (user_id) REFERENCES users(user_id) ON DELETE CASCADE,
    CONSTRAINT fk_refresh_tokens_device FOREIGN KEY (device_id) REFERENCES devices(device_id) ON DELETE CASCADE
);

CREATE INDEX IF NOT EXISTS idx_refresh_tokens_user ON refresh_tokens(user_id);
CREATE INDEX IF NOT EXISTS idx_refresh_tokens_token ON refresh_tokens(token);
CREATE INDEX IF NOT EXISTS idx_refresh_tokens_expiry ON refresh_tokens(expires_ts);

-- Create user_rooms table (for user's room list)
CREATE TABLE IF NOT EXISTS user_rooms (
    id BIGSERIAL PRIMARY KEY,
    user_id VARCHAR(255) NOT NULL,
    room_id VARCHAR(255) NOT NULL,
    membership VARCHAR(50) NOT NULL DEFAULT 'join',
    joined_at BIGINT NOT NULL,
    CONSTRAINT fk_user_rooms_user FOREIGN KEY (user_id) REFERENCES users(user_id) ON DELETE CASCADE,
    CONSTRAINT fk_user_rooms_room FOREIGN KEY (room_id) REFERENCES rooms(room_id) ON DELETE CASCADE,
    CONSTRAINT uk_user_rooms UNIQUE (user_id, room_id)
);

CREATE INDEX IF NOT EXISTS idx_user_rooms_user ON user_rooms(user_id);
CREATE INDEX IF NOT EXISTS idx_user_rooms_room ON user_rooms(room_id);

-- Create room_aliases table
CREATE TABLE IF NOT EXISTS room_aliases (
    id BIGSERIAL PRIMARY KEY,
    room_id VARCHAR(255) NOT NULL,
    alias VARCHAR(255) NOT NULL UNIQUE,
    created_by VARCHAR(255) NOT NULL,
    CONSTRAINT fk_room_aliases_room FOREIGN KEY (room_id) REFERENCES rooms(room_id) ON DELETE CASCADE,
    CONSTRAINT fk_room_aliases_user FOREIGN KEY (created_by) REFERENCES users(user_id) ON DELETE CASCADE
);

CREATE INDEX IF NOT EXISTS idx_room_aliases_room ON room_aliases(room_id);
CREATE INDEX IF NOT EXISTS idx_room_aliases_alias ON room_aliases(alias);

-- Create room_state table
CREATE TABLE IF NOT EXISTS room_state (
    id BIGSERIAL PRIMARY KEY,
    room_id VARCHAR(255) NOT NULL,
    event_id VARCHAR(255) NOT NULL,
    event_type VARCHAR(255) NOT NULL,
    state_key VARCHAR(255) NOT NULL,
    content JSONB NOT NULL DEFAULT '{}',
    created_at BIGINT NOT NULL,
    CONSTRAINT fk_room_state_room FOREIGN KEY (room_id) REFERENCES rooms(room_id) ON DELETE CASCADE,
    CONSTRAINT fk_room_state_event FOREIGN KEY (event_id) REFERENCES events(event_id) ON DELETE CASCADE,
    CONSTRAINT uk_room_state UNIQUE (room_id, event_type, state_key)
);

CREATE INDEX IF NOT EXISTS idx_room_state_room ON room_state(room_id);
CREATE INDEX IF NOT EXISTS idx_room_state_event ON room_state(event_id);
CREATE INDEX IF NOT EXISTS idx_room_state_type ON room_state(event_type);
