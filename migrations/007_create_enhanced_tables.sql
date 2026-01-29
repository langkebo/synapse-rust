-- Create friends table
CREATE TABLE IF NOT EXISTS friends (
    id BIGSERIAL PRIMARY KEY,
    user_id VARCHAR(255) NOT NULL,
    friend_id VARCHAR(255) NOT NULL,
    category_id VARCHAR(255),
    created_ts BIGINT NOT NULL,
    CONSTRAINT fk_friends_user FOREIGN KEY (user_id) REFERENCES users(user_id) ON DELETE CASCADE,
    CONSTRAINT fk_friends_friend FOREIGN KEY (friend_id) REFERENCES users(user_id) ON DELETE CASCADE,
    CONSTRAINT uk_friends UNIQUE (user_id, friend_id)
);

CREATE INDEX IF NOT EXISTS idx_friends_user ON friends(user_id);
CREATE INDEX IF NOT EXISTS idx_friends_friend ON friends(friend_id);

-- Create friend_requests table
CREATE TABLE IF NOT EXISTS friend_requests (
    id BIGSERIAL PRIMARY KEY,
    from_user_id VARCHAR(255) NOT NULL,
    to_user_id VARCHAR(255) NOT NULL,
    message VARCHAR(512),
    status VARCHAR(50) NOT NULL DEFAULT 'pending',
    created_ts BIGINT NOT NULL,
    responded_ts BIGINT,
    CONSTRAINT fk_friend_requests_from FOREIGN KEY (from_user_id) REFERENCES users(user_id) ON DELETE CASCADE,
    CONSTRAINT fk_friend_requests_to FOREIGN KEY (to_user_id) REFERENCES users(user_id) ON DELETE CASCADE,
    CONSTRAINT uk_friend_requests UNIQUE (from_user_id, to_user_id)
);

CREATE INDEX IF NOT EXISTS idx_friend_requests_from ON friend_requests(from_user_id);
CREATE INDEX IF NOT EXISTS idx_friend_requests_to ON friend_requests(to_user_id);
CREATE INDEX IF NOT EXISTS idx_friend_requests_status ON friend_requests(status);

-- Create friend_categories table
CREATE TABLE IF NOT EXISTS friend_categories (
    id VARCHAR(255) PRIMARY KEY,
    user_id VARCHAR(255) NOT NULL,
    name VARCHAR(255) NOT NULL,
    color VARCHAR(7),
    icon VARCHAR(255),
    created_ts BIGINT NOT NULL,
    CONSTRAINT fk_categories_user FOREIGN KEY (user_id) REFERENCES users(user_id) ON DELETE CASCADE,
    CONSTRAINT uk_categories UNIQUE (user_id, name)
);

CREATE INDEX IF NOT EXISTS idx_categories_user ON friend_categories(user_id);

-- Create blocked_users table
CREATE TABLE IF NOT EXISTS blocked_users (
    id BIGSERIAL PRIMARY KEY,
    user_id VARCHAR(255) NOT NULL,
    blocked_user_id VARCHAR(255) NOT NULL,
    reason VARCHAR(512),
    created_ts BIGINT NOT NULL,
    CONSTRAINT fk_blocked_users_user FOREIGN KEY (user_id) REFERENCES users(user_id) ON DELETE CASCADE,
    CONSTRAINT fk_blocked_users_blocked FOREIGN KEY (blocked_user_id) REFERENCES users(user_id) ON DELETE CASCADE,
    CONSTRAINT uk_blocked_users UNIQUE (user_id, blocked_user_id)
);

CREATE INDEX IF NOT EXISTS idx_blocked_users_user ON blocked_users(user_id);
CREATE INDEX IF NOT EXISTS idx_blocked_users_blocked ON blocked_users(blocked_user_id);

-- Create private_sessions table
CREATE TABLE IF NOT EXISTS private_sessions (
    id VARCHAR(255) PRIMARY KEY,
    user_id VARCHAR(255) NOT NULL,
    other_user_id VARCHAR(255) NOT NULL,
    session_type VARCHAR(50) NOT NULL DEFAULT 'direct',
    encryption_key VARCHAR(255),
    created_ts BIGINT NOT NULL,
    last_activity_ts BIGINT NOT NULL,
    CONSTRAINT fk_private_sessions_user FOREIGN KEY (user_id) REFERENCES users(user_id) ON DELETE CASCADE,
    CONSTRAINT fk_private_sessions_other FOREIGN KEY (other_user_id) REFERENCES users(user_id) ON DELETE CASCADE
);

CREATE INDEX IF NOT EXISTS idx_private_sessions_user ON private_sessions(user_id);
CREATE INDEX IF NOT EXISTS idx_private_sessions_other ON private_sessions(other_user_id);
CREATE INDEX IF NOT EXISTS idx_private_sessions_activity ON private_sessions(last_activity_ts DESC);

-- Create private_messages table
CREATE TABLE IF NOT EXISTS private_messages (
    id BIGSERIAL PRIMARY KEY,
    session_id VARCHAR(255) NOT NULL,
    sender_id VARCHAR(255) NOT NULL,
    message_type VARCHAR(50) NOT NULL DEFAULT 'text',
    content TEXT,
    encrypted_content TEXT,
    read_by_receiver BOOLEAN NOT NULL DEFAULT FALSE,
    created_ts BIGINT NOT NULL,
    CONSTRAINT fk_private_messages_session FOREIGN KEY (session_id) REFERENCES private_sessions(id) ON DELETE CASCADE,
    CONSTRAINT fk_private_messages_sender FOREIGN KEY (sender_id) REFERENCES users(user_id) ON DELETE CASCADE
);

CREATE INDEX IF NOT EXISTS idx_private_messages_session ON private_messages(session_id);
CREATE INDEX IF NOT EXISTS idx_private_messages_sender ON private_messages(sender_id);
CREATE INDEX IF NOT EXISTS idx_private_messages_created ON private_messages(created_ts DESC);

-- Create session_keys table
CREATE TABLE IF NOT EXISTS session_keys (
    id BIGSERIAL PRIMARY KEY,
    session_id VARCHAR(255) NOT NULL,
    key_type VARCHAR(50) NOT NULL,
    key_data TEXT NOT NULL,
    created_ts BIGINT NOT NULL,
    expires_ts BIGINT,
    CONSTRAINT fk_session_keys_session FOREIGN KEY (session_id) REFERENCES private_sessions(id) ON DELETE CASCADE
);

CREATE INDEX IF NOT EXISTS idx_session_keys_session ON session_keys(session_id);
CREATE INDEX IF NOT EXISTS idx_session_keys_type ON session_keys(key_type);

-- Create voice_messages table
CREATE TABLE IF NOT EXISTS voice_messages (
    message_id VARCHAR(255) PRIMARY KEY,
    room_id VARCHAR(255),
    user_id VARCHAR(255) NOT NULL,
    duration_ms INT NOT NULL,
    file_size BIGINT NOT NULL,
    content_type VARCHAR(100) NOT NULL,
    waveform_data JSONB,
    file_path VARCHAR(512),
    created_ts BIGINT NOT NULL,
    CONSTRAINT fk_voice_messages_user FOREIGN KEY (user_id) REFERENCES users(user_id) ON DELETE CASCADE
);

CREATE INDEX IF NOT EXISTS idx_voice_messages_room ON voice_messages(room_id);
CREATE INDEX IF NOT EXISTS idx_voice_messages_user ON voice_messages(user_id);
CREATE INDEX IF NOT EXISTS idx_voice_messages_created ON voice_messages(created_ts DESC);

-- Create voice_usage_stats table
CREATE TABLE IF NOT EXISTS voice_usage_stats (
    id BIGSERIAL PRIMARY KEY,
    user_id VARCHAR(255) NOT NULL,
    date DATE NOT NULL,
    message_count INT NOT NULL DEFAULT 0,
    total_duration_ms INT NOT NULL DEFAULT 0,
    total_file_size BIGINT NOT NULL DEFAULT 0,
    created_ts BIGINT NOT NULL,
    updated_ts BIGINT NOT NULL,
    CONSTRAINT fk_voice_stats_user FOREIGN KEY (user_id) REFERENCES users(user_id) ON DELETE CASCADE,
    CONSTRAINT uk_voice_stats UNIQUE (user_id, date)
);

CREATE INDEX IF NOT EXISTS idx_voice_stats_user ON voice_usage_stats(user_id);
CREATE INDEX IF NOT EXISTS idx_voice_stats_date ON voice_usage_stats(date DESC);
