-- Migration: Ensure voice_usage_stats table exists
-- Created: 2026-02-28
-- Purpose: Fix VOICE-001 - Create voice_usage_stats table for voice message statistics

-- Create voice_usage_stats table if not exists
CREATE TABLE IF NOT EXISTS voice_usage_stats (
    id BIGSERIAL PRIMARY KEY,
    user_id VARCHAR(255) NOT NULL,
    room_id VARCHAR(255),
    date DATE NOT NULL,
    period_start TIMESTAMP,
    period_end TIMESTAMP,
    total_duration_ms BIGINT DEFAULT 0,
    total_file_size BIGINT DEFAULT 0,
    message_count BIGINT DEFAULT 0,
    last_activity_ts BIGINT,
    last_active_ts BIGINT,
    created_ts BIGINT,
    updated_ts BIGINT
);

-- Create unique index for user_id, room_id, period_start combination
CREATE UNIQUE INDEX IF NOT EXISTS idx_voice_usage_stats_unique 
    ON voice_usage_stats(user_id, room_id, period_start);

-- Create indexes for common queries
CREATE INDEX IF NOT EXISTS idx_voice_usage_stats_user 
    ON voice_usage_stats(user_id);
CREATE INDEX IF NOT EXISTS idx_voice_usage_stats_date 
    ON voice_usage_stats(date);
CREATE INDEX IF NOT EXISTS idx_voice_usage_stats_room 
    ON voice_usage_stats(room_id);

-- Create room_parents table if not exists (for room hierarchy)
CREATE TABLE IF NOT EXISTS room_parents (
    id BIGSERIAL PRIMARY KEY,
    room_id VARCHAR(255) NOT NULL,
    parent_id VARCHAR(255) NOT NULL,
    created_ts BIGINT NOT NULL,
    CONSTRAINT room_parents_unique UNIQUE(room_id, parent_id)
);

CREATE INDEX IF NOT EXISTS idx_room_parents_room 
    ON room_parents(room_id);
CREATE INDEX IF NOT EXISTS idx_room_parents_parent 
    ON room_parents(parent_id);

-- Insert migration record
INSERT INTO migrations (name, applied_at) 
VALUES ('20260228000007_ensure_voice_tables', NOW())
ON CONFLICT (name) DO NOTHING;
