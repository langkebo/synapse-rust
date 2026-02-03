-- Add missing private chat tables
-- Version: 20260204000005
-- Purpose: Add private_sessions and private_messages tables that were missing from the database

-- 1. Create private_sessions table for private chat session metadata
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

-- 2. Create private_messages table for private chat message storage
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

-- Add table comments for documentation
COMMENT ON TABLE private_sessions IS 'Private chat session metadata between two users';
COMMENT ON TABLE private_messages IS 'Private chat messages - high write volume table';
