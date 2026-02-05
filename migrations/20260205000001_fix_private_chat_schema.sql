-- Fix private chat schema mismatch
-- Version: 20260205000001

DROP TABLE IF EXISTS private_messages CASCADE;
DROP TABLE IF EXISTS private_sessions CASCADE;

CREATE TABLE private_sessions (
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

CREATE INDEX idx_private_sessions_user1 ON private_sessions(user_id_1);
CREATE INDEX idx_private_sessions_user2 ON private_sessions(user_id_2);
CREATE INDEX idx_private_sessions_created ON private_sessions(created_ts);

CREATE TABLE private_messages (
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

CREATE INDEX idx_private_messages_session ON private_messages(session_id);
CREATE INDEX idx_private_messages_session_ts ON private_messages(session_id, created_ts DESC);
CREATE INDEX idx_private_messages_session_read ON private_messages(session_id, created_ts DESC, read_by_receiver);
CREATE INDEX idx_private_messages_sender ON private_messages(sender_id);
CREATE INDEX idx_private_messages_created ON private_messages(created_ts);
