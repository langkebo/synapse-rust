-- ====================================================================
-- Performance Optimization Migration (Fixed Version)
-- ====================================================================
-- This migration adds critical indexes to improve query performance
-- across the synapse-rust application.
--
-- NOTE: This version fixes references to non-existent columns/tables
-- ====================================================================

-- ====================================================================
-- USERS TABLE INDEXES
-- ====================================================================

CREATE INDEX IF NOT EXISTS idx_users_active ON users(deactivated)
    WHERE deactivated = FALSE;

CREATE INDEX IF NOT EXISTS idx_users_username_creation ON users(username, creation_ts DESC);

CREATE INDEX IF NOT EXISTS idx_users_admin ON users(is_admin)
    WHERE is_admin = TRUE;

CREATE INDEX IF NOT EXISTS idx_users_guest ON users(is_guest)
    WHERE is_guest = TRUE;

-- ====================================================================
-- DEVICES TABLE INDEXES
-- ====================================================================

CREATE INDEX IF NOT EXISTS idx_devices_user_last_seen ON devices(user_id, last_seen_ts DESC);

CREATE INDEX IF NOT EXISTS idx_devices_display_name ON devices(user_id, display_name)
    WHERE display_name IS NOT NULL;

-- ====================================================================
-- ACCESS TOKENS INDEXES
-- ====================================================================
-- NOTE: invalidated column does not exist in current schema, using available columns

CREATE INDEX IF NOT EXISTS idx_access_tokens_token ON access_tokens(token);

CREATE INDEX IF NOT EXISTS idx_access_tokens_user ON access_tokens(user_id);

CREATE INDEX IF NOT EXISTS idx_access_tokens_expires ON access_tokens(expires_ts);

-- ====================================================================
-- REFRESH TOKENS INDEXES
-- ====================================================================
-- NOTE: invalidated column does not exist in current schema, using available columns

CREATE INDEX IF NOT EXISTS idx_refresh_tokens_token ON refresh_tokens(token);

CREATE INDEX IF NOT EXISTS idx_refresh_tokens_user_device ON refresh_tokens(user_id, device_id);

CREATE INDEX IF NOT EXISTS idx_refresh_tokens_expires ON refresh_tokens(expires_ts);

-- ====================================================================
-- ROOMS TABLE INDEXES
-- ====================================================================

CREATE INDEX IF NOT EXISTS idx_rooms_public ON rooms(is_public, creation_ts DESC)
    WHERE is_public = TRUE;

CREATE INDEX IF NOT EXISTS idx_rooms_creator ON rooms(creator, creation_ts DESC);

-- ====================================================================
-- ROOM MEMBERSHIPS INDEXES
-- ====================================================================

CREATE INDEX IF NOT EXISTS idx_room_memberships_room_joined ON room_memberships(room_id, membership)
    WHERE membership = 'join';

CREATE INDEX IF NOT EXISTS idx_room_memberships_user_joined ON room_memberships(user_id, membership, joined_ts DESC)
    WHERE membership = 'join';

CREATE INDEX IF NOT EXISTS idx_room_memberships_invites ON room_memberships(user_id, membership, updated_ts DESC)
    WHERE membership = 'invite';

CREATE INDEX IF NOT EXISTS idx_room_memberships_bans ON room_memberships(room_id, membership, banned_by)
    WHERE membership = 'ban';

CREATE INDEX IF NOT EXISTS idx_room_memberships_state ON room_memberships(room_id, user_id, membership, event_id);

-- ====================================================================
-- EVENTS TABLE INDEXES
-- ====================================================================

CREATE INDEX IF NOT EXISTS idx_events_room_origin ON events(room_id, origin_server_ts DESC)
    WHERE redacted = FALSE OR redacted IS NULL;

CREATE INDEX IF NOT EXISTS idx_events_event_id ON events(event_id)
    WHERE redacted = FALSE OR redacted IS NULL;

CREATE INDEX IF NOT EXISTS idx_events_room_type ON events(room_id, event_type)
    WHERE redacted = FALSE OR redacted IS NULL;

CREATE INDEX IF NOT EXISTS idx_events_room_sender ON events(room_id, sender, origin_server_ts DESC)
    WHERE redacted = FALSE OR redacted IS NULL;

-- ====================================================================
-- FRIENDS TABLE INDEXES
-- ====================================================================

CREATE INDEX IF NOT EXISTS idx_friends_user_created ON friends(user_id, created_ts DESC);

CREATE INDEX IF NOT EXISTS idx_friends_friend ON friends(friend_id);

-- ====================================================================
-- FRIEND REQUESTS INDEXES
-- ====================================================================

CREATE INDEX IF NOT EXISTS idx_friend_requests_pending ON friend_requests(to_user_id, status, created_ts DESC)
    WHERE status = 'pending';

CREATE INDEX IF NOT EXISTS idx_friend_requests_sent ON friend_requests(from_user_id, status, created_ts DESC);

CREATE INDEX IF NOT EXISTS idx_friend_requests_status ON friend_requests(status, created_ts DESC);

-- ====================================================================
-- BLOCKED USERS INDEXES
-- ====================================================================

CREATE INDEX IF NOT EXISTS idx_blocked_users_blocker ON blocked_users(user_id, created_ts DESC);

CREATE INDEX IF NOT EXISTS idx_blocked_users_blocked ON blocked_users(blocked_user_id);

-- ====================================================================
-- DEVICE KEYS INDEXES (E2EE)
-- ====================================================================

CREATE INDEX IF NOT EXISTS idx_device_keys_user ON device_keys(user_id, device_id);

CREATE INDEX IF NOT EXISTS idx_device_keys_algorithm ON device_keys(user_id, algorithm)
    WHERE display_name IS NOT NULL;

-- ====================================================================
-- VOICE MESSAGES INDEXES
-- ====================================================================
-- NOTE: sender_id column does not exist, using user_id instead

CREATE INDEX IF NOT EXISTS idx_voice_messages_room ON voice_messages(room_id, created_ts DESC);

CREATE INDEX IF NOT EXISTS idx_voice_messages_user ON voice_messages(user_id, created_ts DESC);

-- ====================================================================
-- IP BLOCKS INDEXES
-- ====================================================================

CREATE INDEX IF NOT EXISTS idx_ip_blocks_ip ON ip_blocks(ip_address);

-- ====================================================================
-- SECURITY EVENTS INDEXES
-- ====================================================================

CREATE INDEX IF NOT EXISTS idx_security_events_recent ON security_events(event_type, created_at DESC);

CREATE INDEX IF NOT EXISTS idx_security_events_user ON security_events(user_id, created_at DESC)
    WHERE user_id IS NOT NULL;

CREATE INDEX IF NOT EXISTS idx_security_events_ip ON security_events(ip_address, created_at DESC)
    WHERE ip_address IS NOT NULL;

-- ====================================================================
-- STATISTICS UPDATE
-- ====================================================================

ANALYZE users;
ANALYZE devices;
ANALYZE access_tokens;
ANALYZE refresh_tokens;
ANALYZE rooms;
ANALYZE room_memberships;
ANALYZE events;
ANALYZE friends;
ANALYZE friend_requests;
ANALYZE blocked_users;
ANALYZE device_keys;
ANALYZE voice_messages;
ANALYZE ip_blocks;
ANALYZE security_events;
