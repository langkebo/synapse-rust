-- Add blocked_rooms table for admin room blocking functionality
-- Migration: 20260221000000_add_blocked_rooms.sql

CREATE TABLE IF NOT EXISTS blocked_rooms (
    room_id TEXT PRIMARY KEY,
    blocked_at BIGINT NOT NULL,
    blocked_by TEXT NOT NULL DEFAULT 'admin',
    reason TEXT,
    created_at BIGINT NOT NULL DEFAULT EXTRACT(EPOCH FROM NOW()) * 1000
);

-- Add index for faster lookups
CREATE INDEX IF NOT EXISTS idx_blocked_rooms_blocked_at ON blocked_rooms(blocked_at);

-- Add comment
COMMENT ON TABLE blocked_rooms IS 'Stores blocked/banned rooms for admin functionality';
