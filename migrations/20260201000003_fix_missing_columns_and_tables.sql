-- Fix missing columns in device_keys
ALTER TABLE device_keys ADD COLUMN IF NOT EXISTS id BIGSERIAL;
ALTER TABLE device_keys ADD COLUMN IF NOT EXISTS algorithm VARCHAR(100);

-- Create missing megolm_sessions table
CREATE TABLE IF NOT EXISTS megolm_sessions (
    id UUID PRIMARY KEY,
    session_id VARCHAR(255) NOT NULL UNIQUE,
    room_id VARCHAR(255) NOT NULL,
    sender_key TEXT NOT NULL,
    session_key TEXT NOT NULL,
    algorithm VARCHAR(100) NOT NULL,
    message_index BIGINT NOT NULL DEFAULT 0,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    last_used_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    expires_at TIMESTAMPTZ
);

CREATE INDEX IF NOT EXISTS idx_megolm_sessions_room ON megolm_sessions(room_id);
CREATE INDEX IF NOT EXISTS idx_megolm_sessions_session_id ON megolm_sessions(session_id);

-- Fix voice_messages table: rename sender_id to user_id to match code expectations
DO $$
BEGIN
    IF EXISTS (
        SELECT 1
        FROM information_schema.columns
        WHERE table_name = 'voice_messages'
          AND column_name = 'sender_id'
    ) AND NOT EXISTS (
        SELECT 1
        FROM information_schema.columns
        WHERE table_name = 'voice_messages'
          AND column_name = 'user_id'
    ) THEN
        ALTER TABLE voice_messages RENAME COLUMN sender_id TO user_id;
    END IF;
END $$;

-- Fix voice_usage_stats: ensure total_duration_ms type matches code expectations (i32/INTEGER)
-- Alternatively, we could change the code to i64, but changing the DB to INTEGER is often easier 
-- if we don't expect durations > 24 days (2^31 ms).
ALTER TABLE voice_usage_stats ALTER COLUMN total_duration_ms TYPE INTEGER;
