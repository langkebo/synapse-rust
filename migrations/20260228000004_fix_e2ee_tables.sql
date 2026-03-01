-- Fix E2EE related tables for to_device, megolm, and cross_signing

-- To Device Messages Table
CREATE TABLE IF NOT EXISTS to_device_messages (
    id SERIAL PRIMARY KEY,
    sender_user_id VARCHAR(255) NOT NULL,
    sender_device_id VARCHAR(255),
    recipient_user_id VARCHAR(255) NOT NULL,
    recipient_device_id VARCHAR(255) NOT NULL,
    message_type VARCHAR(255) NOT NULL,
    content JSONB NOT NULL,
    created_ts BIGINT NOT NULL,
    delivered BOOLEAN DEFAULT FALSE,
    delivered_ts BIGINT
);

CREATE INDEX IF NOT EXISTS idx_to_device_recipient ON to_device_messages(recipient_user_id, recipient_device_id);
CREATE INDEX IF NOT EXISTS idx_to_device_sender ON to_device_messages(sender_user_id);
CREATE INDEX IF NOT EXISTS idx_to_device_created ON to_device_messages(created_ts);

-- Megolm Sessions Table (if not exists)
CREATE TABLE IF NOT EXISTS megolm_sessions (
    id SERIAL PRIMARY KEY,
    room_id VARCHAR(255) NOT NULL,
    session_id VARCHAR(255) NOT NULL UNIQUE,
    sender_key VARCHAR(255) NOT NULL,
    session_key TEXT NOT NULL,
    forwarding_chains TEXT[],
    created_ts BIGINT NOT NULL,
    expires_ts BIGINT,
    is_outbound BOOLEAN DEFAULT FALSE
);

CREATE INDEX IF NOT EXISTS idx_megolm_sessions_room_id ON megolm_sessions(room_id);
CREATE INDEX IF NOT EXISTS idx_megolm_sessions_sender_key ON megolm_sessions(sender_key);

-- Inbound Megolm Sessions
CREATE TABLE IF NOT EXISTS inbound_megolm_sessions (
    id SERIAL PRIMARY KEY,
    session_id VARCHAR(255) NOT NULL UNIQUE,
    sender_key VARCHAR(255) NOT NULL,
    room_id VARCHAR(255) NOT NULL,
    session_data TEXT NOT NULL,
    forwarding_chains TEXT[],
    imported_ts BIGINT NOT NULL
);

CREATE INDEX IF NOT EXISTS idx_inbound_megolm_sessions_sender_key ON inbound_megolm_sessions(sender_key);

-- Cross Signing Keys Table
CREATE TABLE IF NOT EXISTS cross_signing_keys (
    id SERIAL PRIMARY KEY,
    user_id VARCHAR(255) NOT NULL,
    key_type VARCHAR(50) NOT NULL,
    key_data TEXT NOT NULL,
    signatures JSONB,
    created_ts BIGINT NOT NULL,
    updated_ts BIGINT NOT NULL,
    UNIQUE(user_id, key_type)
);

CREATE INDEX IF NOT EXISTS idx_cross_signing_keys_user_id ON cross_signing_keys(user_id);

-- Device Signing Keys
CREATE TABLE IF NOT EXISTS device_signing_keys (
    id SERIAL PRIMARY KEY,
    user_id VARCHAR(255) NOT NULL,
    device_id VARCHAR(255) NOT NULL,
    master_key TEXT,
    self_signing_key TEXT,
    user_signing_key TEXT,
    created_ts BIGINT NOT NULL,
    updated_ts BIGINT NOT NULL,
    UNIQUE(user_id, device_id)
);

CREATE INDEX IF NOT EXISTS idx_device_signing_keys_user ON device_signing_keys(user_id);
