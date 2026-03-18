-- Add missing tables for Matrix compatibility
-- Created: 2026-03-15
-- Tables: room_depth, event_auth, redactions

-- Table: room_depth
-- Tracks the maximum depth of events in each room for efficient querying
CREATE TABLE IF NOT EXISTS room_depth (
    room_id VARCHAR(255) PRIMARY KEY,
    current_depth BIGINT NOT NULL DEFAULT 0,
    max_depth BIGINT NOT NULL DEFAULT 0,
    updated_at BIGINT NOT NULL DEFAULT (EXTRACT(EPOCH FROM NOW()) * 1000)::BIGINT
);

CREATE INDEX IF NOT EXISTS idx_room_depth_room_id ON room_depth(room_id);

-- Table: event_auth
-- Stores event authorization data for federation and state resolution
CREATE TABLE IF NOT EXISTS event_auth (
    room_id VARCHAR(255) NOT NULL,
    event_id VARCHAR(255) NOT NULL,
    auth_method VARCHAR(100) NOT NULL,
    auth_data JSONB,
    created_at BIGINT NOT NULL DEFAULT (EXTRACT(EPOCH FROM NOW()) * 1000)::BIGINT,
    PRIMARY KEY (room_id, event_id, auth_method)
);

CREATE INDEX IF NOT EXISTS idx_event_auth_room_id ON event_auth(room_id);
CREATE INDEX IF NOT EXISTS idx_event_auth_event_id ON event_auth(event_id);

-- Table: redactions
-- Tracks message deletion/redaction events for compliance and sync
CREATE TABLE IF NOT EXISTS redactions (
    redacts_event_id VARCHAR(255) PRIMARY KEY,
    event_id VARCHAR(255) NOT NULL,
    reason JSONB,
    redacted_by VARCHAR(255) NOT NULL,
    redacted_at BIGINT NOT NULL DEFAULT (EXTRACT(EPOCH FROM NOW()) * 1000)::BIGINT,
    FOREIGN KEY (event_id) REFERENCES events(event_id) ON DELETE CASCADE
);

CREATE INDEX IF NOT EXISTS idx_redactions_event_id ON redactions(event_id);
CREATE INDEX IF NOT EXISTS idx_redactions_redacted_by ON redactions(redacted_by);

-- Table: room_depth (initial population from existing events)
INSERT INTO room_depth (room_id, current_depth, max_depth, updated_at)
SELECT 
    room_id,
    COALESCE(MAX(depth), 0) as current_depth,
    COALESCE(MAX(depth), 0) as max_depth,
    (EXTRACT(EPOCH FROM NOW()) * 1000)::BIGINT as updated_at
FROM events
GROUP BY room_id
ON CONFLICT (room_id) DO NOTHING;
