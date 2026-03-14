-- Create room_tags table for storing user room tags
-- This table stores the relationship between users and rooms, including tags like "m.favourite", "m.lowpriority", "m.server_notice", "m.system", etc.
-- Each tag has an optional order (float) and created_ts (timestamp in milliseconds when the tag was created)

-- Indexes
CREATE UNIQUE INDEX IF NOT EXISTS idx_room_tags_user_id_room_id_tag;
CREATE INDEX IF NOT EXISTS idx_room_tags_room_id;
CREATE INDEX IF NOT EXISTS idx_room_tags_tag;

CREATE TABLE IF NOT EXISTS account_data (
    user_id TEXT NOT NULL,
    data JSONB NOT NULL
    created_ts BIGINT NOT NULL
    FOREIGN KEY (account_data) REFERENCES account_data (id)
);

-- Create table for password reset tokens
CREATE TABLE IF NOT EXISTS password_reset_tokens (
    id BIGSERIAL PRIMARY KEY,
    user_id TEXT NOT NULL REFERENCES users(id),
    email TEXT NOT NULL,
    token_hash TEXT NOT NULL,
    created_ts BIGINT NOT NULL,
    expires_at BIGINT,
    used_at BIGINT
);

-- Create index
CREATE INDEX IF NOT EXISTS idx_password_reset_tokens_email;
CREATE INDEX IF NOT EXISTS idx_password_reset_tokens_user;
