BEGIN;

DROP TABLE IF EXISTS sliding_sync_lists CASCADE;
DROP TABLE IF EXISTS sliding_sync_tokens CASCADE;
DROP TABLE IF EXISTS sliding_sync_rooms CASCADE;

CREATE TABLE sliding_sync_lists (
    id BIGSERIAL PRIMARY KEY,
    user_id TEXT NOT NULL,
    device_id TEXT NOT NULL,
    conn_id TEXT,
    list_key TEXT NOT NULL,
    sort JSONB DEFAULT '[]',
    filters JSONB DEFAULT '{}',
    room_subscription JSONB DEFAULT '{}',
    ranges JSONB DEFAULT '[]',
    created_ts BIGINT NOT NULL,
    updated_ts BIGINT NOT NULL
);

CREATE UNIQUE INDEX idx_sliding_sync_lists_unique ON sliding_sync_lists(user_id, device_id, COALESCE(conn_id, ''), list_key);
CREATE INDEX idx_sliding_sync_lists_user_device ON sliding_sync_lists(user_id, device_id);

CREATE TABLE sliding_sync_tokens (
    id BIGSERIAL PRIMARY KEY,
    user_id TEXT NOT NULL,
    device_id TEXT NOT NULL,
    conn_id TEXT,
    token TEXT NOT NULL,
    pos BIGINT NOT NULL,
    created_ts BIGINT NOT NULL,
    expires_at BIGINT
);

CREATE INDEX idx_sliding_sync_tokens_user ON sliding_sync_tokens(user_id, device_id);

CREATE TABLE sliding_sync_rooms (
    id BIGSERIAL PRIMARY KEY,
    user_id TEXT NOT NULL,
    device_id TEXT NOT NULL,
    room_id TEXT NOT NULL,
    conn_id TEXT,
    list_key TEXT,
    bump_stamp BIGINT NOT NULL,
    highlight_count INTEGER DEFAULT 0,
    notification_count INTEGER DEFAULT 0,
    is_dm BOOLEAN DEFAULT FALSE,
    is_encrypted BOOLEAN DEFAULT FALSE,
    is_tombstoned BOOLEAN DEFAULT FALSE,
    invited BOOLEAN DEFAULT FALSE,
    name TEXT,
    avatar TEXT,
    timestamp BIGINT NOT NULL,
    created_ts BIGINT NOT NULL,
    updated_ts BIGINT NOT NULL
);

CREATE UNIQUE INDEX idx_sliding_sync_rooms_unique ON sliding_sync_rooms(user_id, device_id, room_id, COALESCE(conn_id, ''));
CREATE INDEX idx_sliding_sync_rooms_user_device ON sliding_sync_rooms(user_id, device_id);
CREATE INDEX idx_sliding_sync_rooms_bump_stamp ON sliding_sync_rooms(bump_stamp DESC);

COMMIT;