-- Burn After Read persistence tables
-- Provides database-backed storage for self-destructing message settings,
-- pending burn events, and burn history logs.

-- Per-room burn settings for each user
CREATE TABLE IF NOT EXISTS burn_after_read_settings (
    user_id TEXT NOT NULL,
    room_id TEXT NOT NULL,
    is_enabled BOOLEAN NOT NULL DEFAULT FALSE,
    burn_after_ms BIGINT NOT NULL DEFAULT 60000,
    created_ts BIGINT NOT NULL,
    updated_ts BIGINT,
    PRIMARY KEY (user_id, room_id)
);

-- Pending burn events waiting to be processed
CREATE TABLE IF NOT EXISTS burn_after_read_pending (
    id BIGSERIAL PRIMARY KEY,
    user_id TEXT NOT NULL,
    room_id TEXT NOT NULL,
    event_id TEXT NOT NULL,
    created_ts BIGINT NOT NULL,
    delete_at BIGINT NOT NULL,
    is_processed BOOLEAN NOT NULL DEFAULT FALSE,
    UNIQUE(user_id, room_id, event_id)
);

CREATE INDEX IF NOT EXISTS idx_burn_pending_delete_at
    ON burn_after_read_pending(delete_at)
    WHERE is_processed = FALSE;

-- Record of burned events (audit log)
CREATE TABLE IF NOT EXISTS burn_after_read_log (
    id BIGSERIAL PRIMARY KEY,
    user_id TEXT NOT NULL,
    room_id TEXT NOT NULL,
    event_id TEXT NOT NULL,
    burned_ts BIGINT NOT NULL
);

CREATE INDEX IF NOT EXISTS idx_burn_log_user
    ON burn_after_read_log(user_id);

-- Per-user default burn duration
CREATE TABLE IF NOT EXISTS burn_after_read_user_defaults (
    user_id TEXT NOT NULL PRIMARY KEY,
    default_burn_ms BIGINT NOT NULL DEFAULT 60000,
    created_ts BIGINT NOT NULL,
    updated_ts BIGINT
);
