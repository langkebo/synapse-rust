-- Add missing tables for synapse-rust

-- Create backup_keys table (for E2EE key backups)
CREATE TABLE IF NOT EXISTS backup_keys (
    id BIGSERIAL PRIMARY KEY,
    backup_id BIGINT NOT NULL,
    room_id VARCHAR(255) NOT NULL,
    session_id VARCHAR(255) NOT NULL,
    first_message_index BIGINT NOT NULL DEFAULT 0,
    forwarded_count BIGINT NOT NULL DEFAULT 0,
    is_verified BOOLEAN NOT NULL DEFAULT FALSE,
    session_data TEXT NOT NULL,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    CONSTRAINT fk_backup_keys_backup FOREIGN KEY (backup_id) REFERENCES key_backups(id) ON DELETE CASCADE,
    CONSTRAINT uk_backup_keys UNIQUE (backup_id, room_id, session_id)
);

CREATE INDEX IF NOT EXISTS idx_backup_keys_backup ON backup_keys(backup_id);
CREATE INDEX IF NOT EXISTS idx_backup_keys_room ON backup_keys(room_id);
CREATE INDEX IF NOT EXISTS idx_backup_keys_session ON backup_keys(session_id);

-- Create typing table (for typing indicators)
CREATE TABLE IF NOT EXISTS typing (
    id BIGSERIAL PRIMARY KEY,
    room_id VARCHAR(255) NOT NULL,
    user_id VARCHAR(255) NOT NULL,
    typing BOOLEAN NOT NULL DEFAULT TRUE,
    last_updated TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    CONSTRAINT fk_typing_room FOREIGN KEY (room_id) REFERENCES rooms(room_id) ON DELETE CASCADE,
    CONSTRAINT fk_typing_user FOREIGN KEY (user_id) REFERENCES users(user_id) ON DELETE CASCADE,
    CONSTRAINT uk_typing UNIQUE (room_id, user_id)
);

CREATE INDEX IF NOT EXISTS idx_typing_room ON typing(room_id);
CREATE INDEX IF NOT EXISTS idx_typing_user ON typing(user_id);

-- Add missing columns to friend_requests table if they don't exist
DO $$
BEGIN
    IF NOT EXISTS (
        SELECT 1 FROM information_schema.columns
        WHERE table_name = 'friend_requests' AND column_name = 'created_ts'
    ) THEN
        ALTER TABLE friend_requests ADD COLUMN created_ts BIGINT NOT NULL DEFAULT EXTRACT(EPOCH FROM NOW())::BIGINT;
    END IF;

    IF NOT EXISTS (
        SELECT 1 FROM information_schema.columns
        WHERE table_name = 'friend_requests' AND column_name = 'updated_ts'
    ) THEN
        ALTER TABLE friend_requests ADD COLUMN updated_ts BIGINT;
    END IF;
END $$;

-- Add missing columns to blocked_users table if they don't exist
DO $$
BEGIN
    IF NOT EXISTS (
        SELECT 1 FROM information_schema.columns
        WHERE table_name = 'blocked_users' AND column_name = 'created_ts'
    ) THEN
        ALTER TABLE blocked_users ADD COLUMN created_ts BIGINT NOT NULL DEFAULT EXTRACT(EPOCH FROM NOW())::BIGINT;
    END IF;
END $$;

-- Add missing columns to private_sessions table if they don't exist
DO $$
BEGIN
    IF NOT EXISTS (
        SELECT 1 FROM information_schema.columns
        WHERE table_name = 'private_sessions' AND column_name = 'updated_ts'
    ) THEN
        ALTER TABLE private_sessions ADD COLUMN updated_ts BIGINT;
    END IF;

    IF NOT EXISTS (
        SELECT 1 FROM information_schema.columns
        WHERE table_name = 'private_sessions' AND column_name = 'unread_count'
    ) THEN
        ALTER TABLE private_sessions ADD COLUMN unread_count INT NOT NULL DEFAULT 0;
    END IF;

    IF NOT EXISTS (
        SELECT 1 FROM information_schema.columns
        WHERE table_name = 'private_sessions' AND column_name = 'encrypted_content'
    ) THEN
        ALTER TABLE private_sessions ADD COLUMN encrypted_content TEXT;
    END IF;
END $$;

-- Add missing columns to private_messages table if they don't exist
DO $$
BEGIN
    IF NOT EXISTS (
        SELECT 1 FROM information_schema.columns
        WHERE table_name = 'private_messages' AND column_name = 'encrypted_content'
    ) THEN
        ALTER TABLE private_messages ADD COLUMN encrypted_content TEXT;
    END IF;

    IF NOT EXISTS (
        SELECT 1 FROM information_schema.columns
        WHERE table_name = 'private_messages' AND column_name = 'read_by_receiver'
    ) THEN
        ALTER TABLE private_messages ADD COLUMN read_by_receiver BOOLEAN NOT NULL DEFAULT FALSE;
    END IF;
END $$;

-- Add missing columns to rooms table if they don't exist
DO $$
BEGIN
    IF NOT EXISTS (
        SELECT 1 FROM information_schema.columns
        WHERE table_name = 'rooms' AND column_name = 'visibility'
    ) THEN
        ALTER TABLE rooms ADD COLUMN visibility VARCHAR(50) DEFAULT 'public';
    END IF;
END $$;

-- Add missing columns to voice_messages table if they don't exist
DO $$
BEGIN
    IF NOT EXISTS (
        SELECT 1 FROM information_schema.columns
        WHERE table_name = 'voice_messages' AND column_name = 'session_id'
    ) THEN
        ALTER TABLE voice_messages ADD COLUMN session_id VARCHAR(255);
    END IF;
END $$;

-- Add missing columns to voice_usage_stats table if they don't exist
DO $$
BEGIN
    IF NOT EXISTS (
        SELECT 1 FROM information_schema.columns
        WHERE table_name = 'voice_usage_stats' AND column_name = 'updated_ts'
    ) THEN
        ALTER TABLE voice_usage_stats ADD COLUMN updated_ts BIGINT NOT NULL DEFAULT EXTRACT(EPOCH FROM NOW())::BIGINT;
    END IF;
END $$;

-- Create missing tables if they don't exist
CREATE TABLE IF NOT EXISTS room_account_data (
    id BIGSERIAL PRIMARY KEY,
    room_id VARCHAR(255) NOT NULL,
    user_id VARCHAR(255) NOT NULL,
    event_type VARCHAR(255) NOT NULL,
    content JSONB NOT NULL DEFAULT '{}',
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    CONSTRAINT fk_room_account_data_room FOREIGN KEY (room_id) REFERENCES rooms(room_id) ON DELETE CASCADE,
    CONSTRAINT fk_room_account_data_user FOREIGN KEY (user_id) REFERENCES users(user_id) ON DELETE CASCADE,
    CONSTRAINT uk_room_account_data UNIQUE (room_id, user_id, event_type)
);

CREATE TABLE IF NOT EXISTS user_account_data (
    id BIGSERIAL PRIMARY KEY,
    user_id VARCHAR(255) NOT NULL,
    event_type VARCHAR(255) NOT NULL,
    content JSONB NOT NULL DEFAULT '{}',
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    CONSTRAINT fk_user_account_data_user FOREIGN KEY (user_id) REFERENCES users(user_id) ON DELETE CASCADE,
    CONSTRAINT uk_user_account_data UNIQUE (user_id, event_type)
);

CREATE TABLE IF NOT EXISTS read_markers (
    id BIGSERIAL PRIMARY KEY,
    room_id VARCHAR(255) NOT NULL,
    user_id VARCHAR(255) NOT NULL,
    event_id VARCHAR(255) NOT NULL,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    CONSTRAINT fk_read_markers_room FOREIGN KEY (room_id) REFERENCES rooms(room_id) ON DELETE CASCADE,
    CONSTRAINT fk_read_markers_user FOREIGN KEY (user_id) REFERENCES users(user_id) ON DELETE CASCADE,
    CONSTRAINT uk_read_markers UNIQUE (room_id, user_id)
);

CREATE INDEX IF NOT EXISTS idx_room_account_data_room ON room_account_data(room_id);
CREATE INDEX IF NOT EXISTS idx_room_account_data_user ON room_account_data(user_id);
CREATE INDEX IF NOT EXISTS idx_user_account_data_user ON user_account_data(user_id);
CREATE INDEX IF NOT EXISTS idx_read_markers_room ON read_markers(room_id);
CREATE INDEX IF NOT EXISTS idx_read_markers_user ON read_markers(user_id);

\echo '✅ Missing tables and columns added successfully!'
