-- Final fix for all identified schema issues
-- Version: 20260202000001

CREATE EXTENSION IF NOT EXISTS pgcrypto;

-- 1. Fix device_keys table
-- Ensure id is UUID to match the Rust model
DO $$
BEGIN
    -- If id exists and is not uuid, drop it and recreate it
    IF EXISTS (
        SELECT 1 FROM information_schema.columns 
        WHERE table_name = 'device_keys' AND column_name = 'id' 
        AND data_type != 'uuid'
    ) THEN
        ALTER TABLE device_keys DROP COLUMN id;
        ALTER TABLE device_keys ADD COLUMN id UUID DEFAULT gen_random_uuid();
        ALTER TABLE device_keys ADD CONSTRAINT device_keys_id_unique UNIQUE (id);
    ELSIF NOT EXISTS (
        SELECT 1 FROM information_schema.columns 
        WHERE table_name = 'device_keys' AND column_name = 'id'
    ) THEN
        ALTER TABLE device_keys ADD COLUMN id UUID DEFAULT gen_random_uuid();
        ALTER TABLE device_keys ADD CONSTRAINT device_keys_id_unique UNIQUE (id);
    END IF;
END $$;

-- Ensure algorithm exists
ALTER TABLE device_keys ADD COLUMN IF NOT EXISTS algorithm VARCHAR(100);
-- Ensure updated_at exists
ALTER TABLE device_keys ADD COLUMN IF NOT EXISTS updated_at TIMESTAMPTZ DEFAULT NOW();
ALTER TABLE device_keys ADD COLUMN IF NOT EXISTS ts_updated_ms BIGINT DEFAULT EXTRACT(EPOCH FROM NOW()) * 1000;

-- 2. Fix voice_messages table
DO $$
BEGIN
    IF EXISTS (
        SELECT 1 FROM information_schema.columns 
        WHERE table_name = 'voice_messages' AND column_name = 'sender_id'
    ) AND NOT EXISTS (
        SELECT 1 FROM information_schema.columns 
        WHERE table_name = 'voice_messages' AND column_name = 'user_id'
    ) THEN
        ALTER TABLE voice_messages RENAME COLUMN sender_id TO user_id;
    END IF;
END $$;

-- 3. Ensure megolm_sessions table exists
CREATE TABLE IF NOT EXISTS megolm_sessions (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
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
