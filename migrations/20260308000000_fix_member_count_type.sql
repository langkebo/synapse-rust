-- Fix member_count type from INTEGER to BIGINT
ALTER TABLE rooms ALTER COLUMN member_count TYPE BIGINT;

-- Add missing columns for device_keys if not exists
DO $$ 
BEGIN
    IF NOT EXISTS (SELECT 1 FROM information_schema.columns WHERE table_name = 'device_keys' AND column_name = 'ts_updated_ms') THEN
        ALTER TABLE device_keys ADD COLUMN ts_updated_ms BIGINT NOT NULL DEFAULT EXTRACT(EPOCH FROM NOW()) * 1000;
    END IF;
END $$;

-- Ensure search_index table exists
CREATE TABLE IF NOT EXISTS search_index (
    id SERIAL PRIMARY KEY,
    event_id VARCHAR(255) NOT NULL,
    room_id VARCHAR(255) NOT NULL,
    user_id VARCHAR(255) NOT NULL,
    event_type VARCHAR(255) NOT NULL,
    content TEXT NOT NULL,
    created_ts BIGINT NOT NULL,
    updated_ts BIGINT,
    UNIQUE (event_id)
);

CREATE INDEX IF NOT EXISTS idx_search_index_room ON search_index(room_id);
CREATE INDEX IF NOT EXISTS idx_search_index_user ON search_index(user_id);
CREATE INDEX IF NOT EXISTS idx_search_index_type ON search_index(event_type);

-- Ensure user_directory table exists
CREATE TABLE IF NOT EXISTS user_directory (
    user_id VARCHAR(255) PRIMARY KEY,
    displayname VARCHAR(255),
    avatar_url TEXT,
    server_name VARCHAR(255),
    updated_ts BIGINT
);

CREATE INDEX IF NOT EXISTS idx_user_directory_displayname ON user_directory(displayname);
