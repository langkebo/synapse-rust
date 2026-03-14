-- Migration: Add missing columns to room_summaries table
-- Date: 2026-03-13
-- Description: Add missing columns and tables that are required by the room summary service

-- Add missing columns to room_summaries
ALTER TABLE room_summaries ADD COLUMN IF NOT EXISTS id BIGINT;
ALTER TABLE room_summaries ADD COLUMN IF NOT EXISTS room_type VARCHAR(50);
ALTER TABLE room_summaries ADD COLUMN IF NOT EXISTS avatar_url TEXT;
ALTER TABLE room_summaries ADD COLUMN IF NOT EXISTS join_rules VARCHAR(50) DEFAULT 'public';
ALTER TABLE room_summaries ADD COLUMN IF NOT EXISTS history_visibility VARCHAR(50) DEFAULT 'shared';
ALTER TABLE room_summaries ADD COLUMN IF NOT EXISTS guest_access VARCHAR(50) DEFAULT 'can_join';
ALTER TABLE room_summaries ADD COLUMN IF NOT EXISTS is_direct BOOLEAN DEFAULT false;
ALTER TABLE room_summaries ADD COLUMN IF NOT EXISTS is_space BOOLEAN DEFAULT false;
ALTER TABLE room_summaries ADD COLUMN IF NOT EXISTS is_encrypted BOOLEAN DEFAULT false;
ALTER TABLE room_summaries ADD COLUMN IF NOT EXISTS last_event_id TEXT;
ALTER TABLE room_summaries ADD COLUMN IF NOT EXISTS last_event_ts BIGINT;
ALTER TABLE room_summaries ADD COLUMN IF NOT EXISTS last_message_ts BIGINT;
ALTER TABLE room_summaries ADD COLUMN IF NOT EXISTS unread_notifications BIGINT DEFAULT 0;
ALTER TABLE room_summaries ADD COLUMN IF NOT EXISTS unread_highlight BIGINT DEFAULT 0;
ALTER TABLE room_summaries ADD COLUMN IF NOT EXISTS created_ts BIGINT DEFAULT EXTRACT(EPOCH FROM NOW())::BIGINT * 1000;
ALTER TABLE room_summaries ADD COLUMN IF NOT EXISTS joined_member_count BIGINT DEFAULT 0;
ALTER TABLE room_summaries ADD COLUMN IF NOT EXISTS invited_member_count BIGINT DEFAULT 0;
ALTER TABLE room_summaries ALTER COLUMN hero_users SET DEFAULT '[]'::jsonb;

-- Ensure primary key is properly set
DO $$
BEGIN
    IF NOT EXISTS (SELECT 1 FROM information_schema.table_constraints 
                   WHERE constraint_name = 'pk_room_summaries' AND table_name = 'room_summaries') THEN
        ALTER TABLE room_summaries ADD CONSTRAINT pk_room_summaries PRIMARY KEY (room_id);
    END IF;
END $$;

-- Create room_summary_members table
CREATE TABLE IF NOT EXISTS room_summary_members (
    id BIGSERIAL PRIMARY KEY,
    room_id VARCHAR(255) NOT NULL,
    user_id VARCHAR(255) NOT NULL,
    display_name TEXT,
    avatar_url TEXT,
    membership VARCHAR(50) DEFAULT 'join',
    is_hero BOOLEAN DEFAULT false,
    last_active_ts BIGINT,
    updated_ts BIGINT DEFAULT EXTRACT(EPOCH FROM NOW())::BIGINT * 1000,
    created_ts BIGINT DEFAULT EXTRACT(EPOCH FROM NOW())::BIGINT * 1000
);

CREATE INDEX IF NOT EXISTS idx_room_summary_members_room ON room_summary_members(room_id);
CREATE UNIQUE INDEX IF NOT EXISTS idx_room_summary_members_user_room ON room_summary_members(user_id, room_id);

DO $$
BEGIN
    RAISE NOTICE 'Room summaries migration completed successfully';
END $$;
