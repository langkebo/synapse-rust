-- Fix room directory and alias related tables
-- This migration ensures all required tables and columns exist

-- Ensure room_directory table exists with correct structure
CREATE TABLE IF NOT EXISTS room_directory (
    room_id VARCHAR(255) PRIMARY KEY,
    is_public BOOLEAN DEFAULT true,
    name VARCHAR(255),
    topic VARCHAR(512),
    avatar_url VARCHAR(512),
    canonical_alias VARCHAR(255),
    member_count BIGINT DEFAULT 0,
    primary_category VARCHAR(100),
    searchable BOOLEAN DEFAULT true
);

-- Ensure room_aliases table exists with correct structure
CREATE TABLE IF NOT EXISTS room_aliases (
    room_alias VARCHAR(255) PRIMARY KEY,
    room_id VARCHAR(255) NOT NULL,
    server_name VARCHAR(255) NOT NULL,
    created_ts BIGINT,
    updated_ts BIGINT
);

-- Ensure read_markers table exists
CREATE TABLE IF NOT EXISTS read_markers (
    id SERIAL PRIMARY KEY,
    room_id VARCHAR(255) NOT NULL,
    user_id VARCHAR(255) NOT NULL,
    event_id VARCHAR(255) NOT NULL,
    marker_type VARCHAR(50) DEFAULT 'm.read',
    created_ts BIGINT,
    updated_ts BIGINT,
    UNIQUE(room_id, user_id, marker_type)
);

-- Create indexes for better performance
CREATE INDEX IF NOT EXISTS idx_room_directory_public ON room_directory(is_public);
CREATE INDEX IF NOT EXISTS idx_room_aliases_room_id ON room_aliases(room_id);
CREATE INDEX IF NOT EXISTS idx_read_markers_room_user ON read_markers(room_id, user_id);
CREATE INDEX IF NOT EXISTS idx_read_markers_user ON read_markers(user_id);

-- Add missing columns to room_aliases if they don't exist
DO $$ 
BEGIN
    IF NOT EXISTS (
        SELECT 1 FROM information_schema.columns 
        WHERE table_name = 'room_aliases' 
        AND column_name = 'created_by'
    ) THEN
        ALTER TABLE room_aliases ADD COLUMN created_by VARCHAR(255);
    END IF;
END $$;

-- Add alias column if missing (for compatibility)
DO $$ 
BEGIN
    IF NOT EXISTS (
        SELECT 1 FROM information_schema.columns 
        WHERE table_name = 'room_aliases' 
        AND column_name = 'alias'
    ) THEN
        ALTER TABLE room_aliases ADD COLUMN alias VARCHAR(255);
    END IF;
END $$;
