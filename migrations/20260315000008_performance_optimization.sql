-- Performance Optimization Migration
-- Date: 2026-03-14
-- Description: Add performance indexes for large room sync optimization

-- ============================================================================
-- P1.1: Events Table Optimization
-- ============================================================================

-- Covering index for room events time-based queries
-- This index includes commonly accessed columns to avoid table lookups
CREATE INDEX IF NOT EXISTS idx_events_room_time_covering 
ON events(room_id, origin_server_ts DESC) 
INCLUDE (event_id, type, sender, state_key);

-- Index for event type filtering
CREATE INDEX IF NOT EXISTS idx_events_room_type 
ON events(room_id, type) 
WHERE state_key IS NULL;

-- Index for redacted events (commonly filtered out)
CREATE INDEX IF NOT EXISTS idx_events_not_redacted 
ON events(room_id, origin_server_ts DESC) 
WHERE redacted = FALSE OR redacted IS NULL;

-- ============================================================================
-- P1.2: Room Memberships Optimization
-- ============================================================================

-- Composite index for membership queries
CREATE INDEX IF NOT EXISTS idx_room_memberships_room_user_status 
ON room_memberships(room_id, user_id, membership);

-- Index for user's joined rooms (most common query)
CREATE INDEX IF NOT EXISTS idx_room_memberships_user_joined 
ON room_memberships(user_id, membership) 
WHERE membership = 'join';

-- Index for membership by time (for incremental sync)
CREATE INDEX IF NOT EXISTS idx_room_memberships_joined_ts 
ON room_memberships(room_id, joined_ts DESC) 
WHERE membership = 'join';

-- ============================================================================
-- P1.3: Room Summaries Optimization
-- ============================================================================

-- Index for room summary by user
CREATE INDEX IF NOT EXISTS idx_room_summaries_user 
ON room_summaries(user_id) 
WHERE is_direct = TRUE OR is_direct IS NULL;

-- Index for last activity time
CREATE INDEX IF NOT EXISTS idx_room_summaries_last_activity 
ON room_summaries(user_id, last_activity_ts DESC);

-- ============================================================================
-- P1.4: Read Markers Optimization
-- ============================================================================

-- Index for user's read markers
CREATE INDEX IF NOT EXISTS idx_read_markers_user_room 
ON read_markers(user_id, room_id);

-- Index for fully read marker type
CREATE INDEX IF NOT EXISTS idx_read_markers_fully_read 
ON read_markers(room_id, user_id) 
WHERE marker_type = 'm.fully_read';

-- ============================================================================
-- P1.5: Presence Optimization
-- ============================================================================

-- Index for presence status queries
CREATE INDEX IF NOT EXISTS idx_presence_status 
ON presence(presence) 
WHERE presence = 'online';

-- Index for last active time
CREATE INDEX IF NOT EXISTS idx_presence_last_active 
ON presence(last_active_ts DESC) 
WHERE presence = 'online';

-- ============================================================================
-- P1.6: Sync Stream Optimization
-- ============================================================================

-- Index for sync stream by position
CREATE INDEX IF NOT EXISTS idx_sync_stream_position 
ON sync_stream(position DESC);

-- Index for sync stream by type
CREATE INDEX IF NOT EXISTS idx_sync_stream_type_position 
ON sync_stream(stream_type, position DESC);

-- ============================================================================
-- P1.7: Receipts Optimization
-- ============================================================================

-- Index for user's receipts in room
CREATE INDEX IF NOT EXISTS idx_receipts_user_room 
ON receipts(user_id, room_id);

-- Index for room receipts by time
CREATE INDEX IF NOT EXISTS idx_receipts_room_time 
ON receipts(room_id, ts DESC);

-- ============================================================================
-- P1.8: Media Upload Progress Table
-- ============================================================================

-- Table for tracking chunked uploads
CREATE TABLE IF NOT EXISTS upload_progress (
    upload_id TEXT PRIMARY KEY,
    user_id TEXT NOT NULL,
    filename TEXT,
    content_type TEXT,
    total_size BIGINT,
    uploaded_size BIGINT DEFAULT 0,
    total_chunks INTEGER DEFAULT 0,
    uploaded_chunks INTEGER DEFAULT 0,
    status TEXT DEFAULT 'pending',
    created_ts BIGINT NOT NULL DEFAULT (EXTRACT(EPOCH FROM NOW()) * 1000)::BIGINT,
    updated_ts BIGINT,
    expires_at BIGINT
);

CREATE INDEX IF NOT EXISTS idx_upload_progress_user ON upload_progress(user_id);
CREATE INDEX IF NOT EXISTS idx_upload_progress_status ON upload_progress(status) WHERE status = 'pending';
CREATE INDEX IF NOT EXISTS idx_upload_progress_expires ON upload_progress(expires_at) WHERE expires_at IS NOT NULL;

-- Table for upload chunks
CREATE TABLE IF NOT EXISTS upload_chunks (
    id BIGSERIAL PRIMARY KEY,
    upload_id TEXT NOT NULL REFERENCES upload_progress(upload_id) ON DELETE CASCADE,
    chunk_index INTEGER NOT NULL,
    chunk_data BYTEA NOT NULL,
    chunk_size BIGINT NOT NULL,
    created_ts BIGINT NOT NULL DEFAULT (EXTRACT(EPOCH FROM NOW()) * 1000)::BIGINT,
    UNIQUE(upload_id, chunk_index)
);

CREATE INDEX IF NOT EXISTS idx_upload_chunks_upload ON upload_chunks(upload_id);

-- ============================================================================
-- P1.9: E2EE Audit Log Table
-- ============================================================================

CREATE TABLE IF NOT EXISTS e2ee_audit_log (
    id BIGSERIAL PRIMARY KEY,
    user_id TEXT NOT NULL,
    device_id TEXT,
    operation TEXT NOT NULL,
    key_id TEXT,
    room_id TEXT,
    details JSONB,
    ip_address TEXT,
    created_ts BIGINT NOT NULL DEFAULT (EXTRACT(EPOCH FROM NOW()) * 1000)::BIGINT
);

CREATE INDEX IF NOT EXISTS idx_e2ee_audit_user ON e2ee_audit_log(user_id);
CREATE INDEX IF NOT EXISTS idx_e2ee_audit_operation ON e2ee_audit_log(operation);
CREATE INDEX IF NOT EXISTS idx_e2ee_audit_created ON e2ee_audit_log(created_ts DESC);

-- ============================================================================
-- Completion Notice
-- ============================================================================

DO $$
BEGIN
    RAISE NOTICE '==========================================';
    RAISE NOTICE 'Performance optimization migration completed';
    RAISE NOTICE 'Created indexes for:';
    RAISE NOTICE '  - Events table (covering, type, not_redacted)';
    RAISE NOTICE '  - Room memberships (composite, joined, time)';
    RAISE NOTICE '  - Room summaries (user, activity)';
    RAISE NOTICE '  - Read markers (user_room, fully_read)';
    RAISE NOTICE '  - Presence (status, last_active)';
    RAISE NOTICE '  - Sync stream (position, type)';
    RAISE NOTICE '  - Receipts (user_room, time)';
    RAISE NOTICE 'Created tables for:';
    RAISE NOTICE '  - upload_progress (chunked uploads)';
    RAISE NOTICE '  - upload_chunks (upload data)';
    RAISE NOTICE '  - e2ee_audit_log (security audit)';
    RAISE NOTICE '==========================================';
END $$;
