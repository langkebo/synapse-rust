-- Token Blacklist Table for Security Enhancement
-- This migration adds token revocation support

-- Create token_blacklist table
CREATE TABLE IF NOT EXISTS token_blacklist (
    id BIGSERIAL PRIMARY KEY,
    token_hash VARCHAR(64) NOT NULL UNIQUE,
    user_id VARCHAR(255) NOT NULL,
    revoked_at BIGINT NOT NULL,
    reason VARCHAR(255),
    created_at TIMESTAMP DEFAULT CURRENT_TIMESTAMP
);

-- Create index for fast blacklist lookups
CREATE INDEX IF NOT EXISTS idx_token_blacklist_hash ON token_blacklist(token_hash);
CREATE INDEX IF NOT EXISTS idx_token_blacklist_user ON token_blacklist(user_id);
CREATE INDEX IF NOT EXISTS idx_token_blacklist_revoked_at ON token_blacklist(revoked_at);

-- Add is_valid column to access_tokens if not exists
DO $$ 
BEGIN
    IF NOT EXISTS (SELECT 1 FROM information_schema.columns WHERE table_name = 'access_tokens' AND column_name = 'is_valid') THEN
        ALTER TABLE access_tokens ADD COLUMN is_valid BOOLEAN DEFAULT TRUE;
    END IF;
END $$;

-- Add revoked_ts column to access_tokens if not exists
DO $$ 
BEGIN
    IF NOT EXISTS (SELECT 1 FROM information_schema.columns WHERE table_name = 'access_tokens' AND column_name = 'revoked_ts') THEN
        ALTER TABLE access_tokens ADD COLUMN revoked_ts BIGINT;
    END IF;
END $$;

-- Create index for token validity checks
CREATE INDEX IF NOT EXISTS idx_access_tokens_valid ON access_tokens(is_valid);
CREATE INDEX IF NOT EXISTS idx_access_tokens_user_valid ON access_tokens(user_id, is_valid);

-- Performance Optimization Indexes
-- User table indexes
CREATE INDEX IF NOT EXISTS idx_users_name ON users(name);
CREATE INDEX IF NOT EXISTS idx_users_creation_ts ON users(creation_ts);

-- Room table indexes
CREATE INDEX IF NOT EXISTS idx_rooms_creator ON rooms(creator);
CREATE INDEX IF NOT EXISTS idx_rooms_join_rule ON rooms(join_rule);

-- Event table indexes
CREATE INDEX IF NOT EXISTS idx_events_room_type ON events(room_id, event_type);
CREATE INDEX IF NOT EXISTS idx_events_origin_ts ON events(origin_server_ts);

-- Room members indexes
CREATE INDEX IF NOT EXISTS idx_room_members_room_user ON room_members(room_id, user_id);
CREATE INDEX IF NOT EXISTS idx_room_members_membership ON room_members(membership);

-- Room aliases index
CREATE INDEX IF NOT EXISTS idx_room_aliases_alias ON room_aliases(room_alias);

-- Device indexes
CREATE INDEX IF NOT EXISTS idx_devices_user ON devices(user_id);

-- Refresh token indexes
CREATE INDEX IF NOT EXISTS idx_refresh_tokens_user ON refresh_tokens(user_id);
CREATE INDEX IF NOT EXISTS idx_refresh_tokens_hash ON refresh_tokens(token_hash);

-- Function to cleanup old blacklist entries
CREATE OR REPLACE FUNCTION cleanup_token_blacklist(max_age_seconds BIGINT)
RETURNS BIGINT AS $$
DECLARE
    deleted_count BIGINT;
    cutoff_ts BIGINT;
BEGIN
    cutoff_ts := EXTRACT(EPOCH FROM NOW()) - max_age_seconds;
    
    DELETE FROM token_blacklist WHERE revoked_at < cutoff_ts;
    
    GET DIAGNOSTICS deleted_count = ROW_COUNT;
    RETURN deleted_count;
END;
$$ LANGUAGE plpgsql;

-- Create a view for active tokens
CREATE OR REPLACE VIEW active_tokens AS
SELECT * FROM access_tokens WHERE is_valid = TRUE;

-- Create a view for user sessions
CREATE OR REPLACE VIEW user_sessions AS
SELECT 
    at.id,
    at.user_id,
    at.device_id,
    at.created_ts,
    at.expires_ts,
    d.display_name AS device_name,
    CASE WHEN at.is_valid THEN 'active' ELSE 'revoked' END AS status
FROM access_tokens at
LEFT JOIN devices d ON at.device_id = d.device_id;
