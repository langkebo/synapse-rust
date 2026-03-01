-- Fix E2EE related tables for megolm sessions

-- Drop and recreate megolm_sessions with correct structure
DROP TABLE IF EXISTS megolm_sessions CASCADE;

CREATE TABLE megolm_sessions (
    id SERIAL PRIMARY KEY,
    session_id VARCHAR(255) NOT NULL UNIQUE,
    room_id VARCHAR(255) NOT NULL,
    sender_key VARCHAR(255) NOT NULL,
    session_key TEXT NOT NULL,
    algorithm VARCHAR(100) DEFAULT 'm.megolm.v1.aes-sha2',
    message_index INTEGER DEFAULT 0,
    created_at BIGINT NOT NULL,
    last_used_at BIGINT,
    expires_at BIGINT,
    is_outbound BOOLEAN DEFAULT FALSE
);

CREATE INDEX idx_megolm_sessions_room_id ON megolm_sessions(room_id);
CREATE INDEX idx_megolm_sessions_sender_key ON megolm_sessions(sender_key);
CREATE INDEX idx_megolm_sessions_session_id ON megolm_sessions(session_id);

-- Ensure inbound_megolm_sessions has correct structure
DROP TABLE IF EXISTS inbound_megolm_sessions CASCADE;

CREATE TABLE inbound_megolm_sessions (
    id SERIAL PRIMARY KEY,
    session_id VARCHAR(255) NOT NULL UNIQUE,
    sender_key VARCHAR(255) NOT NULL,
    room_id VARCHAR(255) NOT NULL,
    session_data TEXT NOT NULL,
    forwarding_chains TEXT[],
    imported_at BIGINT NOT NULL
);

CREATE INDEX idx_inbound_megolm_sessions_sender_key ON inbound_megolm_sessions(sender_key);
CREATE INDEX idx_inbound_megolm_sessions_room_id ON inbound_megolm_sessions(room_id);
