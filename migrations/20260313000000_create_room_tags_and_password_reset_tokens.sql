-- Create room_tags table for storing user room tags
-- This table stores the relationship between users and rooms, including tags like "m.favourite", "m.lowpriority", "m.server_notice", "m.system", etc.
-- Each tag has an optional order (float) and created_ts (timestamp in milliseconds when the tag was created)

-- Create room_tags table
CREATE TABLE IF NOT EXISTS room_tags (
    id BIGSERIAL PRIMARY KEY,
    user_id TEXT NOT NULL,
    room_id TEXT NOT NULL,
    tag TEXT NOT NULL,
    tag_order DOUBLE PRECISION,
    created_ts BIGINT NOT NULL DEFAULT (EXTRACT(EPOCH FROM NOW()) * 1000)::BIGINT,
    UNIQUE(user_id, room_id, tag)
);

-- Indexes for room_tags
CREATE UNIQUE INDEX IF NOT EXISTS idx_room_tags_user_id_room_id_tag ON room_tags(user_id, room_id, tag);
CREATE INDEX IF NOT EXISTS idx_room_tags_room_id ON room_tags(room_id);
CREATE INDEX IF NOT EXISTS idx_room_tags_tag ON room_tags(tag);

-- Create account_data table
CREATE TABLE IF NOT EXISTS account_data (
    id BIGSERIAL PRIMARY KEY,
    user_id TEXT NOT NULL,
    data_type TEXT NOT NULL,
    data JSONB NOT NULL,
    created_ts BIGINT NOT NULL DEFAULT (EXTRACT(EPOCH FROM NOW()) * 1000)::BIGINT,
    updated_ts BIGINT,
    UNIQUE(user_id, data_type)
);

CREATE INDEX IF NOT EXISTS idx_account_data_user ON account_data(user_id);
CREATE INDEX IF NOT EXISTS idx_account_data_type ON account_data(data_type);

-- Create table for password reset tokens
CREATE TABLE IF NOT EXISTS password_reset_tokens (
    id BIGSERIAL PRIMARY KEY,
    user_id TEXT NOT NULL,
    email TEXT NOT NULL,
    token_hash TEXT NOT NULL,
    created_ts BIGINT NOT NULL DEFAULT (EXTRACT(EPOCH FROM NOW()) * 1000)::BIGINT,
    expires_at BIGINT,
    used_at BIGINT
);

-- Create indexes for password_reset_tokens
CREATE INDEX IF NOT EXISTS idx_password_reset_tokens_email ON password_reset_tokens(email);
CREATE INDEX IF NOT EXISTS idx_password_reset_tokens_user ON password_reset_tokens(user_id);
CREATE INDEX IF NOT EXISTS idx_password_reset_tokens_token ON password_reset_tokens(token_hash);
