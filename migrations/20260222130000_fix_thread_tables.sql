-- Add missing columns to thread_roots table
ALTER TABLE thread_roots 
ADD COLUMN IF NOT EXISTS root_event_id VARCHAR(255),
ADD COLUMN IF NOT EXISTS sender VARCHAR(255),
ADD COLUMN IF NOT EXISTS content JSONB DEFAULT '{}',
ADD COLUMN IF NOT EXISTS origin_server_ts BIGINT,
ADD COLUMN IF NOT EXISTS last_reply_event_id VARCHAR(255),
ADD COLUMN IF NOT EXISTS last_reply_sender VARCHAR(255),
ADD COLUMN IF NOT EXISTS is_frozen BOOLEAN DEFAULT FALSE,
ADD COLUMN IF NOT EXISTS updated_ts BIGINT;

-- Migrate existing data
UPDATE thread_roots 
SET root_event_id = event_id,
    sender = creator,
    origin_server_ts = created_ts,
    updated_ts = created_ts
WHERE root_event_id IS NULL;

-- Make columns NOT NULL after migration
ALTER TABLE thread_roots 
ALTER COLUMN root_event_id SET NOT NULL,
ALTER COLUMN sender SET NOT NULL,
ALTER COLUMN origin_server_ts SET NOT NULL;

-- Create thread_replies table if not exists
CREATE TABLE IF NOT EXISTS thread_replies (
    id BIGSERIAL PRIMARY KEY,
    room_id VARCHAR(255) NOT NULL,
    thread_id VARCHAR(255) NOT NULL,
    event_id VARCHAR(255) NOT NULL UNIQUE,
    root_event_id VARCHAR(255) NOT NULL,
    sender VARCHAR(255) NOT NULL,
    in_reply_to_event_id VARCHAR(255),
    content JSONB DEFAULT '{}',
    origin_server_ts BIGINT NOT NULL,
    is_edited BOOLEAN DEFAULT FALSE,
    is_redacted BOOLEAN DEFAULT FALSE,
    created_ts BIGINT NOT NULL
);

CREATE INDEX IF NOT EXISTS idx_thread_replies_room ON thread_replies(room_id);
CREATE INDEX IF NOT EXISTS idx_thread_replies_thread ON thread_replies(thread_id);
CREATE INDEX IF NOT EXISTS idx_thread_replies_sender ON thread_replies(sender);

-- Create thread_subscriptions table if not exists
CREATE TABLE IF NOT EXISTS thread_subscriptions (
    id BIGSERIAL PRIMARY KEY,
    room_id VARCHAR(255) NOT NULL,
    thread_id VARCHAR(255) NOT NULL,
    user_id VARCHAR(255) NOT NULL,
    notification_level VARCHAR(50) NOT NULL DEFAULT 'all',
    is_muted BOOLEAN DEFAULT FALSE,
    subscribed_ts BIGINT NOT NULL,
    updated_ts BIGINT NOT NULL,
    UNIQUE(room_id, thread_id, user_id)
);

-- Create thread_read_receipts table if not exists
CREATE TABLE IF NOT EXISTS thread_read_receipts (
    id BIGSERIAL PRIMARY KEY,
    room_id VARCHAR(255) NOT NULL,
    thread_id VARCHAR(255) NOT NULL,
    user_id VARCHAR(255) NOT NULL,
    last_read_event_id VARCHAR(255),
    last_read_ts BIGINT NOT NULL,
    unread_count INTEGER DEFAULT 0,
    updated_ts BIGINT NOT NULL,
    UNIQUE(room_id, thread_id, user_id)
);
