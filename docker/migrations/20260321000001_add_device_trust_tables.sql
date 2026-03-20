-- E2EE Phase 1: Device Trust and Verification Tables
-- Migration: 20260321000001_add_device_trust_tables.sql
-- Description: Add device trust status and verification request tables for new device verification flow
-- Database: PostgreSQL

-- =====================================================
-- Table: device_trust_status
-- Purpose: Track device trust levels (verified, unverified, blocked)
-- =====================================================
CREATE TABLE IF NOT EXISTS device_trust_status (
    id BIGSERIAL PRIMARY KEY,
    user_id VARCHAR(255) NOT NULL,
    device_id VARCHAR(255) NOT NULL,
    trust_level VARCHAR(50) NOT NULL DEFAULT 'unverified' CHECK (trust_level IN ('verified', 'unverified', 'blocked')),
    verified_by_device_id VARCHAR(255),
    verified_at TIMESTAMP WITH TIME ZONE,
    created_at TIMESTAMP WITH TIME ZONE DEFAULT CURRENT_TIMESTAMP,
    updated_at TIMESTAMP WITH TIME ZONE DEFAULT CURRENT_TIMESTAMP,
    UNIQUE (user_id, device_id)
);

CREATE INDEX IF NOT EXISTS idx_device_trust_user_trust ON device_trust_status(user_id, trust_level);
CREATE INDEX IF NOT EXISTS idx_device_trust_level ON device_trust_status(trust_level);

-- =====================================================
-- Table: device_verification_request
-- Purpose: Track new device verification requests
-- =====================================================
CREATE TABLE IF NOT EXISTS device_verification_request (
    id BIGSERIAL PRIMARY KEY,
    user_id VARCHAR(255) NOT NULL,
    new_device_id VARCHAR(255) NOT NULL,
    requesting_device_id VARCHAR(255),
    verification_method VARCHAR(50) NOT NULL CHECK (verification_method IN ('sas', 'qr', 'emoji')),
    status VARCHAR(50) NOT NULL DEFAULT 'pending' CHECK (status IN ('pending', 'approved', 'rejected', 'expired')),
    request_token VARCHAR(255) NOT NULL UNIQUE,
    commitment VARCHAR(255),
    pubkey VARCHAR(255),
    created_at TIMESTAMP WITH TIME ZONE DEFAULT CURRENT_TIMESTAMP,
    expires_at TIMESTAMP WITH TIME ZONE NOT NULL,
    completed_at TIMESTAMP WITH TIME ZONE,
    UNIQUE (user_id, new_device_id)
);

CREATE INDEX IF NOT EXISTS idx_verification_user_device ON device_verification_request(user_id, new_device_id);
CREATE INDEX IF NOT EXISTS idx_verification_status ON device_verification_request(status);
CREATE INDEX IF NOT EXISTS idx_verification_expires ON device_verification_request(expires_at);

-- =====================================================
-- Table: key_rotation_log
-- Purpose: Track key rotation history for audit
-- =====================================================
CREATE TABLE IF NOT EXISTS key_rotation_log (
    id BIGSERIAL PRIMARY KEY,
    user_id VARCHAR(255) NOT NULL,
    device_id VARCHAR(255) NOT NULL,
    room_id VARCHAR(255),
    rotation_type VARCHAR(50) NOT NULL CHECK (rotation_type IN ('olm', 'megolm', 'cross_signing')),
    old_key_id VARCHAR(255),
    new_key_id VARCHAR(255),
    reason VARCHAR(255),
    rotated_at TIMESTAMP WITH TIME ZONE DEFAULT CURRENT_TIMESTAMP
);

CREATE INDEX IF NOT EXISTS idx_key_rotation_user_room ON key_rotation_log(user_id, room_id);
CREATE INDEX IF NOT EXISTS idx_key_rotation_at ON key_rotation_log(rotated_at);
CREATE INDEX IF NOT EXISTS idx_key_rotation_type ON key_rotation_log(rotation_type);

-- =====================================================
-- Table: e2ee_security_events
-- Purpose: Track security-related events for audit
-- =====================================================
CREATE TABLE IF NOT EXISTS e2ee_security_events (
    id BIGSERIAL PRIMARY KEY,
    user_id VARCHAR(255) NOT NULL,
    device_id VARCHAR(255),
    event_type VARCHAR(100) NOT NULL,
    event_data JSONB,
    ip_address VARCHAR(45),
    user_agent VARCHAR(512),
    created_at TIMESTAMP WITH TIME ZONE DEFAULT CURRENT_TIMESTAMP
);

CREATE INDEX IF NOT EXISTS idx_e2ee_security_user_events ON e2ee_security_events(user_id, created_at);
CREATE INDEX IF NOT EXISTS idx_e2ee_security_type ON e2ee_security_events(event_type);
CREATE INDEX IF NOT EXISTS idx_e2ee_security_created ON e2ee_security_events(created_at);

-- =====================================================
-- Table: cross_signing_trust
-- Purpose: Track cross-signing trust relationships
-- =====================================================
CREATE TABLE IF NOT EXISTS cross_signing_trust (
    id BIGSERIAL PRIMARY KEY,
    user_id VARCHAR(255) NOT NULL,
    target_user_id VARCHAR(255) NOT NULL,
    master_key_id VARCHAR(255),
    is_trusted BOOLEAN DEFAULT FALSE,
    trusted_at TIMESTAMP WITH TIME ZONE,
    created_at TIMESTAMP WITH TIME ZONE DEFAULT CURRENT_TIMESTAMP,
    updated_at TIMESTAMP WITH TIME ZONE DEFAULT CURRENT_TIMESTAMP,
    UNIQUE (user_id, target_user_id)
);

CREATE INDEX IF NOT EXISTS idx_cross_signing_trust_target ON cross_signing_trust(target_user_id);

-- Insert migration record
INSERT INTO schema_migrations (version, description, applied_at)
VALUES ('20260321000001', 'Add device trust and verification tables for E2EE Phase 1', CURRENT_TIMESTAMP)
ON CONFLICT (version) DO UPDATE SET applied_at = CURRENT_TIMESTAMP;
