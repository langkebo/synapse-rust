-- MatrixRTC Session Persistence Tables
-- This migration adds support for MSC3401 (Native Group VoIP Signaling)
-- and MSC3758 (MatrixRTC Session Persistence)

-- MatrixRTC Sessions Table
CREATE TABLE IF NOT EXISTS matrixrtc_sessions (
    id BIGSERIAL PRIMARY KEY,
    room_id TEXT NOT NULL,
    session_id TEXT NOT NULL,
    application TEXT NOT NULL DEFAULT 'm.call',
    call_id TEXT,
    creator TEXT NOT NULL,
    created_ts BIGINT NOT NULL,
    updated_ts BIGINT NOT NULL,
    is_active BOOLEAN NOT NULL DEFAULT true,
    config JSONB NOT NULL DEFAULT '{}'::jsonb,
    
    CONSTRAINT matrixrtc_sessions_room_id_fkey 
        FOREIGN KEY (room_id) REFERENCES rooms(room_id) ON DELETE CASCADE,
    CONSTRAINT matrixrtc_sessions_creator_fkey 
        FOREIGN KEY (creator) REFERENCES users(user_id) ON DELETE CASCADE,
    CONSTRAINT matrixrtc_sessions_unique 
        UNIQUE (room_id, session_id)
);

-- MatrixRTC Memberships Table
CREATE TABLE IF NOT EXISTS matrixrtc_memberships (
    id BIGSERIAL PRIMARY KEY,
    room_id TEXT NOT NULL,
    session_id TEXT NOT NULL,
    user_id TEXT NOT NULL,
    device_id TEXT NOT NULL,
    membership_id TEXT NOT NULL,
    application TEXT NOT NULL DEFAULT 'm.call',
    call_id TEXT,
    created_ts BIGINT NOT NULL,
    updated_ts BIGINT NOT NULL,
    expires_ts BIGINT,
    foci_active TEXT,
    foci_preferred JSONB,
    application_data JSONB,
    is_active BOOLEAN NOT NULL DEFAULT true,
    
    CONSTRAINT matrixrtc_memberships_room_id_fkey 
        FOREIGN KEY (room_id) REFERENCES rooms(room_id) ON DELETE CASCADE,
    CONSTRAINT matrixrtc_memberships_user_id_fkey 
        FOREIGN KEY (user_id) REFERENCES users(user_id) ON DELETE CASCADE,
    CONSTRAINT matrixrtc_memberships_unique 
        UNIQUE (room_id, session_id, user_id, device_id)
);

-- MatrixRTC Encryption Keys Table
CREATE TABLE IF NOT EXISTS matrixrtc_encryption_keys (
    id BIGSERIAL PRIMARY KEY,
    room_id TEXT NOT NULL,
    session_id TEXT NOT NULL,
    key_index INTEGER NOT NULL,
    key TEXT NOT NULL,
    created_ts BIGINT NOT NULL,
    expires_ts BIGINT,
    sender_user_id TEXT NOT NULL,
    sender_device_id TEXT NOT NULL,
    
    CONSTRAINT matrixrtc_encryption_keys_room_id_fkey 
        FOREIGN KEY (room_id) REFERENCES rooms(room_id) ON DELETE CASCADE,
    CONSTRAINT matrixrtc_encryption_keys_sender_user_id_fkey 
        FOREIGN KEY (sender_user_id) REFERENCES users(user_id) ON DELETE CASCADE,
    CONSTRAINT matrixrtc_encryption_keys_unique 
        UNIQUE (room_id, session_id, key_index)
);

-- Indexes for efficient queries
CREATE INDEX IF NOT EXISTS idx_matrixrtc_sessions_room_id ON matrixrtc_sessions(room_id);
CREATE INDEX IF NOT EXISTS idx_matrixrtc_sessions_is_active ON matrixrtc_sessions(is_active);
CREATE INDEX IF NOT EXISTS idx_matrixrtc_sessions_room_active ON matrixrtc_sessions(room_id, is_active);

CREATE INDEX IF NOT EXISTS idx_matrixrtc_memberships_room_session ON matrixrtc_memberships(room_id, session_id);
CREATE INDEX IF NOT EXISTS idx_matrixrtc_memberships_user ON matrixrtc_memberships(user_id);
CREATE INDEX IF NOT EXISTS idx_matrixrtc_memberships_active ON matrixrtc_memberships(is_active);
CREATE INDEX IF NOT EXISTS idx_matrixrtc_memberships_expires ON matrixrtc_memberships(expires_ts) WHERE is_active = true;

CREATE INDEX IF NOT EXISTS idx_matrixrtc_encryption_keys_room_session ON matrixrtc_encryption_keys(room_id, session_id);
CREATE INDEX IF NOT EXISTS idx_matrixrtc_encryption_keys_expires ON matrixrtc_encryption_keys(expires_ts);

-- Comments for documentation
COMMENT ON TABLE matrixrtc_sessions IS 'MatrixRTC session persistence for native group VoIP signaling (MSC3401)';
COMMENT ON TABLE matrixrtc_memberships IS 'MatrixRTC session memberships with device-level tracking';
COMMENT ON TABLE matrixrtc_encryption_keys IS 'Encryption keys for MatrixRTC sessions (E2EE for calls)';

COMMENT ON COLUMN matrixrtc_sessions.application IS 'Application type: m.call (default), or custom application';
COMMENT ON COLUMN matrixrtc_sessions.call_id IS 'Optional call identifier for grouping related sessions';
COMMENT ON COLUMN matrixrtc_sessions.config IS 'Session configuration (capabilities, settings)';

COMMENT ON COLUMN matrixrtc_memberships.membership_id IS 'Unique identifier for this membership';
COMMENT ON COLUMN matrixrtc_memberships.expires_ts IS 'Membership expiration timestamp (auto-leave after timeout)';
COMMENT ON COLUMN matrixrtc_memberships.foci_active IS 'Active focus (livekit, native-webrtc, etc.)';
COMMENT ON COLUMN matrixrtc_memberships.foci_preferred IS 'Preferred focus configurations';
COMMENT ON COLUMN matrixrtc_memberships.application_data IS 'Application-specific membership data';

COMMENT ON COLUMN matrixrtc_encryption_keys.key_index IS 'Key index for key rotation';
COMMENT ON COLUMN matrixrtc_encryption_keys.expires_ts IS 'Key expiration for forward secrecy';
