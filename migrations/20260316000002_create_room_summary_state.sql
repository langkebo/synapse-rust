-- Migration: Create room_summary_state table for MSC3245 support
-- Date: 2026-03-15
-- Description: Add room_summary_state table required for room state summary queries

-- Create room_summary_state table
CREATE TABLE IF NOT EXISTS room_summary_state (
    id BIGSERIAL PRIMARY KEY,
    room_id VARCHAR(255) NOT NULL,
    event_type VARCHAR(255) NOT NULL,
    state_key TEXT,
    event_id VARCHAR(255) NOT NULL,
    content JSONB DEFAULT '{}'::jsonb,
    updated_ts BIGINT DEFAULT EXTRACT(EPOCH FROM NOW())::BIGINT * 1000,
    created_ts BIGINT DEFAULT EXTRACT(EPOCH FROM NOW())::BIGINT * 1000,
    UNIQUE(room_id, event_type, state_key)
);

-- Create indexes for efficient lookups
CREATE INDEX IF NOT EXISTS idx_room_summary_state_room ON room_summary_state(room_id);
CREATE INDEX IF NOT EXISTS idx_room_summary_state_type_key ON room_summary_state(event_type, state_key);
CREATE INDEX IF NOT EXISTS idx_room_summary_state_updated ON room_summary_state(updated_ts);

DO $$
BEGIN
    RAISE NOTICE 'Room summary state table created successfully';
END $$;
