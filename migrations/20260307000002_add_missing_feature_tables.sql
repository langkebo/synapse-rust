-- Dehydrated Devices Table (MSC3814)
CREATE TABLE IF NOT EXISTS dehydrated_devices (
    id BIGSERIAL PRIMARY KEY,
    user_id TEXT NOT NULL,
    device_id TEXT NOT NULL,
    device_data JSONB NOT NULL,
    algorithm TEXT NOT NULL,
    account JSONB,
    created_ts BIGINT NOT NULL,
    updated_ts BIGINT NOT NULL,
    expires_ts BIGINT,
    
    CONSTRAINT dehydrated_devices_user_id_fkey 
        FOREIGN KEY (user_id) REFERENCES users(user_id) ON DELETE CASCADE,
    CONSTRAINT dehydrated_devices_unique 
        UNIQUE (user_id, device_id)
);

CREATE INDEX IF NOT EXISTS idx_dehydrated_devices_user ON dehydrated_devices(user_id);
CREATE INDEX IF NOT EXISTS idx_dehydrated_devices_expires ON dehydrated_devices(expires_ts);

COMMENT ON TABLE dehydrated_devices IS 'Dehydrated devices for MSC3814: Dehydrated Devices';

-- Rendezvous Sessions Table (MSC4108)
CREATE TABLE IF NOT EXISTS rendezvous_sessions (
    id BIGSERIAL PRIMARY KEY,
    session_id TEXT NOT NULL UNIQUE,
    user_id TEXT,
    device_id TEXT,
    intent TEXT NOT NULL,
    transport TEXT NOT NULL,
    transport_data JSONB,
    key TEXT NOT NULL,
    created_ts BIGINT NOT NULL,
    expires_ts BIGINT NOT NULL,
    status TEXT NOT NULL DEFAULT 'waiting',
    
    CONSTRAINT rendezvous_sessions_user_id_fkey 
        FOREIGN KEY (user_id) REFERENCES users(user_id) ON DELETE CASCADE
);

CREATE INDEX IF NOT EXISTS idx_rendezvous_sessions_session ON rendezvous_sessions(session_id);
CREATE INDEX IF NOT EXISTS idx_rendezvous_sessions_user ON rendezvous_sessions(user_id);
CREATE INDEX IF NOT EXISTS idx_rendezvous_sessions_status ON rendezvous_sessions(status);
CREATE INDEX IF NOT EXISTS idx_rendezvous_sessions_expires ON rendezvous_sessions(expires_ts);

-- Rendezvous Messages Table
CREATE TABLE IF NOT EXISTS rendezvous_messages (
    id BIGSERIAL PRIMARY KEY,
    session_id TEXT NOT NULL,
    direction TEXT NOT NULL,
    message_type TEXT NOT NULL,
    content JSONB NOT NULL,
    created_ts BIGINT NOT NULL,
    
    CONSTRAINT rendezvous_messages_session_id_fkey 
        FOREIGN KEY (session_id) REFERENCES rendezvous_sessions(session_id) ON DELETE CASCADE
);

CREATE INDEX IF NOT EXISTS idx_rendezvous_messages_session ON rendezvous_messages(session_id);

COMMENT ON TABLE rendezvous_sessions IS 'Rendezvous sessions for MSC4108: QR Code Login';
COMMENT ON TABLE rendezvous_messages IS 'Rendezvous messages for secure channel communication';

-- Moderation Rules Table
CREATE TABLE IF NOT EXISTS moderation_rules (
    id BIGSERIAL PRIMARY KEY,
    rule_id TEXT NOT NULL UNIQUE,
    server_id TEXT,
    rule_type TEXT NOT NULL,
    pattern TEXT NOT NULL,
    action TEXT NOT NULL,
    reason TEXT,
    created_by TEXT NOT NULL,
    created_ts BIGINT NOT NULL,
    updated_ts BIGINT NOT NULL,
    is_active BOOLEAN NOT NULL DEFAULT true,
    priority INTEGER NOT NULL DEFAULT 100,
    
    CONSTRAINT moderation_rules_created_by_fkey 
        FOREIGN KEY (created_by) REFERENCES users(user_id) ON DELETE CASCADE
);

CREATE INDEX IF NOT EXISTS idx_moderation_rules_rule ON moderation_rules(rule_id);
CREATE INDEX IF NOT EXISTS idx_moderation_rules_type ON moderation_rules(rule_type);
CREATE INDEX IF NOT EXISTS idx_moderation_rules_active ON moderation_rules(is_active);
CREATE INDEX IF NOT EXISTS idx_moderation_rules_priority ON moderation_rules(priority DESC);

-- Moderation Logs Table
CREATE TABLE IF NOT EXISTS moderation_logs (
    id BIGSERIAL PRIMARY KEY,
    rule_id TEXT NOT NULL,
    event_id TEXT NOT NULL,
    room_id TEXT NOT NULL,
    sender TEXT NOT NULL,
    content_hash TEXT NOT NULL,
    action_taken TEXT NOT NULL,
    confidence REAL NOT NULL,
    created_ts BIGINT NOT NULL,
    
    CONSTRAINT moderation_logs_rule_id_fkey 
        FOREIGN KEY (rule_id) REFERENCES moderation_rules(rule_id) ON DELETE CASCADE,
    CONSTRAINT moderation_logs_sender_fkey 
        FOREIGN KEY (sender) REFERENCES users(user_id) ON DELETE CASCADE
);

CREATE INDEX IF NOT EXISTS idx_moderation_logs_event ON moderation_logs(event_id);
CREATE INDEX IF NOT EXISTS idx_moderation_logs_room ON moderation_logs(room_id);
CREATE INDEX IF NOT EXISTS idx_moderation_logs_sender ON moderation_logs(sender);
CREATE INDEX IF NOT EXISTS idx_moderation_logs_created ON moderation_logs(created_ts);

COMMENT ON TABLE moderation_rules IS 'Moderation rules for content filtering';
COMMENT ON TABLE moderation_logs IS 'Moderation action logs for audit trail';

-- Livekit Integration Tables
CREATE TABLE IF NOT EXISTS livekit_rooms (
    id BIGSERIAL PRIMARY KEY,
    room_id TEXT NOT NULL UNIQUE,
    livekit_sid TEXT NOT NULL,
    matrix_room_id TEXT,
    created_by TEXT NOT NULL,
    created_ts BIGINT NOT NULL,
    expires_ts BIGINT,
    status TEXT NOT NULL DEFAULT 'active',
    metadata JSONB,
    
    CONSTRAINT livekit_rooms_created_by_fkey 
        FOREIGN KEY (created_by) REFERENCES users(user_id) ON DELETE CASCADE
);

CREATE INDEX IF NOT EXISTS idx_livekit_rooms_room ON livekit_rooms(room_id);
CREATE INDEX IF NOT EXISTS idx_livekit_rooms_matrix_room ON livekit_rooms(matrix_room_id);
CREATE INDEX IF NOT EXISTS idx_livekit_rooms_status ON livekit_rooms(status);

CREATE TABLE IF NOT EXISTS livekit_participants (
    id BIGSERIAL PRIMARY KEY,
    livekit_room_id BIGINT NOT NULL,
    user_id TEXT NOT NULL,
    identity TEXT NOT NULL,
    joined_ts BIGINT NOT NULL,
    left_ts BIGINT,
    status TEXT NOT NULL DEFAULT 'joined',
    
    CONSTRAINT livekit_participants_room_fkey 
        FOREIGN KEY (livekit_room_id) REFERENCES livekit_rooms(id) ON DELETE CASCADE,
    CONSTRAINT livekit_participants_user_fkey 
        FOREIGN KEY (user_id) REFERENCES users(user_id) ON DELETE CASCADE
);

CREATE INDEX IF NOT EXISTS idx_livekit_participants_room ON livekit_participants(livekit_room_id);
CREATE INDEX IF NOT EXISTS idx_livekit_participants_user ON livekit_participants(user_id);

COMMENT ON TABLE livekit_rooms IS 'Livekit room mappings for MatrixRTC';
COMMENT ON TABLE livekit_participants IS 'Livekit participant tracking';
