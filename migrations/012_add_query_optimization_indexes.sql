-- Database Query Optimization Migration
-- This migration adds additional indexes to optimize high-frequency queries

-- Add index for rooms.join_rule (missing from initial schema)
CREATE INDEX IF NOT EXISTS idx_rooms_join_rule ON rooms(join_rule);

-- Add composite index for room_memberships (room_id, membership) for filtering by room and membership status
CREATE INDEX IF NOT EXISTS idx_memberships_room_membership ON room_memberships(room_id, membership);

-- Add composite index for room_memberships (user_id, membership) for filtering by user and membership status
CREATE INDEX IF NOT EXISTS idx_memberships_user_membership ON room_memberships(user_id, membership);

-- Add composite index for events (room_id, origin_server_ts) for efficient time-based queries in rooms
CREATE INDEX IF NOT EXISTS idx_events_room_ts ON events(room_id, origin_server_ts DESC);

-- Add composite index for events (room_id, type) for filtering events by type in a room
CREATE INDEX IF NOT EXISTS idx_events_room_type ON events(room_id, type);

-- Add index for room_state (event_type, state_key) for efficient state lookups
CREATE INDEX IF NOT EXISTS idx_room_state_type_key ON room_state(event_type, state_key);

-- Add index for user_rooms (user_id, membership) for efficient user room list queries
CREATE INDEX IF NOT EXISTS idx_user_rooms_user_membership ON user_rooms(user_id, membership);

-- Add index for user_rooms (room_id, membership) for efficient room member queries
CREATE INDEX IF NOT EXISTS idx_user_rooms_room_membership ON user_rooms(room_id, membership);

-- Add index for access_tokens (user_id, invalidated_ts) for finding valid tokens
CREATE INDEX IF NOT EXISTS idx_access_tokens_user_valid ON access_tokens(user_id, invalidated_ts) WHERE invalidated_ts IS NULL;

-- Add index for refresh_tokens (user_id, invalidated_ts) for finding valid refresh tokens
CREATE INDEX IF NOT EXISTS idx_refresh_tokens_user_valid ON refresh_tokens(user_id, invalidated_ts) WHERE invalidated_ts IS NULL;

-- Add index for presence (last_active_ts) for finding recently active users
CREATE INDEX IF NOT EXISTS idx_presence_last_active ON presence(last_active_ts DESC);

-- Add index for presence (presence) for filtering by presence status
CREATE INDEX IF NOT EXISTS idx_presence_status ON presence(presence);

-- Add composite index for ratelimit (user_id, endpoint, window_start) for efficient rate limit checks
CREATE INDEX IF NOT EXISTS idx_ratelimit_user_endpoint_window ON ratelimit(user_id, endpoint, window_start);

-- Add composite index for ratelimit (ip_address, endpoint, window_start) for efficient IP-based rate limit checks
CREATE INDEX IF NOT EXISTS idx_ratelimit_ip_endpoint_window ON ratelimit(ip_address, endpoint, window_start);
