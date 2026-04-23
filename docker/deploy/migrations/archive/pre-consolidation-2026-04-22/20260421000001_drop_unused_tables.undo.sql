-- Undo: recreate dropped tables (for rollback only)

CREATE TABLE IF NOT EXISTS private_sessions (
    id VARCHAR(255) NOT NULL,
    user_id_1 VARCHAR(255) NOT NULL,
    user_id_2 VARCHAR(255) NOT NULL,
    session_type VARCHAR(50) DEFAULT 'direct',
    encryption_key VARCHAR(255),
    created_ts BIGINT NOT NULL,
    last_activity_ts BIGINT NOT NULL,
    updated_ts BIGINT,
    unread_count INTEGER DEFAULT 0,
    encrypted_content TEXT,
    CONSTRAINT pk_private_sessions PRIMARY KEY (id),
    CONSTRAINT uq_private_sessions_users UNIQUE (user_id_1, user_id_2)
);

CREATE TABLE IF NOT EXISTS private_messages (
    id BIGSERIAL,
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
    deleted_at BIGINT,
    is_edited BOOLEAN DEFAULT FALSE,
    unread_count INTEGER DEFAULT 0,
    CONSTRAINT pk_private_messages PRIMARY KEY (id)
);

CREATE TABLE IF NOT EXISTS room_children (
    id BIGSERIAL PRIMARY KEY,
    parent_room_id TEXT NOT NULL,
    child_room_id TEXT NOT NULL,
    state_key TEXT,
    content JSONB NOT NULL DEFAULT '{}',
    suggested BOOLEAN NOT NULL DEFAULT FALSE,
    created_ts BIGINT NOT NULL DEFAULT 0,
    updated_ts BIGINT
);
