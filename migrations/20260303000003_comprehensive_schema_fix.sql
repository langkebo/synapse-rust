-- Migration: Comprehensive schema fix for all discovered issues
-- Description: Fix all schema inconsistencies discovered during API testing
-- Version: 20260303000003
-- Date: 2026-03-03

-- ============================================
-- 1. USERS TABLE FIXES
-- ============================================

-- Rename deactivated to is_deactivated
DO $$
BEGIN
    IF EXISTS (SELECT 1 FROM information_schema.columns WHERE table_name = 'users' AND column_name = 'deactivated')
       AND NOT EXISTS (SELECT 1 FROM information_schema.columns WHERE table_name = 'users' AND column_name = 'is_deactivated')
    THEN
        ALTER TABLE users RENAME COLUMN deactivated TO is_deactivated;
    END IF;
END $$;

-- Rename shadow_banned to is_shadow_banned
DO $$
BEGIN
    IF EXISTS (SELECT 1 FROM information_schema.columns WHERE table_name = 'users' AND column_name = 'shadow_banned')
       AND NOT EXISTS (SELECT 1 FROM information_schema.columns WHERE table_name = 'users' AND column_name = 'is_shadow_banned')
    THEN
        ALTER TABLE users RENAME COLUMN shadow_banned TO is_shadow_banned;
    END IF;
END $$;

-- Add missing columns
ALTER TABLE users ADD COLUMN IF NOT EXISTS is_deactivated BOOLEAN DEFAULT FALSE;
ALTER TABLE users ADD COLUMN IF NOT EXISTS updated_ts BIGINT;
ALTER TABLE users ADD COLUMN IF NOT EXISTS user_type TEXT;
ALTER TABLE users ADD COLUMN IF NOT EXISTS consent_version TEXT;
ALTER TABLE users ADD COLUMN IF NOT EXISTS appservice_id TEXT;
ALTER TABLE users ADD COLUMN IF NOT EXISTS invalid_update_ts BIGINT;
ALTER TABLE users ADD COLUMN IF NOT EXISTS migration_state TEXT;

-- ============================================
-- 2. DEVICES TABLE FIXES
-- ============================================

ALTER TABLE devices ADD COLUMN IF NOT EXISTS first_seen_ts BIGINT;
ALTER TABLE devices ADD COLUMN IF NOT EXISTS device_key JSONB;
ALTER TABLE devices ADD COLUMN IF NOT EXISTS appservice_id TEXT;
ALTER TABLE devices ADD COLUMN IF NOT EXISTS ignored_user_list TEXT;

-- ============================================
-- 3. REFRESH_TOKENS TABLE FIXES
-- ============================================

ALTER TABLE refresh_tokens ADD COLUMN IF NOT EXISTS access_token_id TEXT;
ALTER TABLE refresh_tokens ADD COLUMN IF NOT EXISTS scope TEXT;
ALTER TABLE refresh_tokens ADD COLUMN IF NOT EXISTS expires_at BIGINT;
ALTER TABLE refresh_tokens ADD COLUMN IF NOT EXISTS last_used_ts BIGINT;
ALTER TABLE refresh_tokens ADD COLUMN IF NOT EXISTS use_count INT DEFAULT 0;
ALTER TABLE refresh_tokens ADD COLUMN IF NOT EXISTS revoked_reason TEXT;
ALTER TABLE refresh_tokens ADD COLUMN IF NOT EXISTS client_info JSONB;
ALTER TABLE refresh_tokens ADD COLUMN IF NOT EXISTS ip_address TEXT;
ALTER TABLE refresh_tokens ADD COLUMN IF NOT EXISTS user_agent TEXT;

-- ============================================
-- 4. ACCESS_TOKENS TABLE FIXES
-- ============================================

ALTER TABLE access_tokens ADD COLUMN IF NOT EXISTS revoked_ts BIGINT;

-- ============================================
-- 5. TOKEN_BLACKLIST TABLE FIXES
-- ============================================

ALTER TABLE token_blacklist ADD COLUMN IF NOT EXISTS expires_at BIGINT;

-- ============================================
-- 6. USER_THREEPIDS TABLE FIXES
-- ============================================

ALTER TABLE user_threepids ADD COLUMN IF NOT EXISTS validated_at BIGINT;
ALTER TABLE user_threepids ADD COLUMN IF NOT EXISTS added_at BIGINT;

-- ============================================
-- 7. ROOMS TABLE FIXES
-- ============================================

-- Remove duplicate created_ts if both exist
DO $$
BEGIN
    IF EXISTS (SELECT 1 FROM information_schema.columns WHERE table_name = 'rooms' AND column_name = 'created_ts')
       AND EXISTS (SELECT 1 FROM information_schema.columns WHERE table_name = 'rooms' AND column_name = 'created_ts')
    THEN
        ALTER TABLE rooms DROP COLUMN created_ts;
    END IF;
END $$;

-- ============================================
-- 8. EVENTS TABLE FIXES
-- ============================================

ALTER TABLE events ADD COLUMN IF NOT EXISTS processed_ts BIGINT;
ALTER TABLE events ADD COLUMN IF NOT EXISTS not_before BIGINT DEFAULT 0;
ALTER TABLE events ADD COLUMN IF NOT EXISTS status TEXT DEFAULT 'pending';
ALTER TABLE events ADD COLUMN IF NOT EXISTS reference_image TEXT;
ALTER TABLE events ADD COLUMN IF NOT EXISTS origin TEXT DEFAULT 'self';
ALTER TABLE events ADD COLUMN IF NOT EXISTS unsigned JSONB DEFAULT '{}';

-- ============================================
-- 9. PRESENCE TABLE FIXES
-- ============================================

ALTER TABLE presence ADD COLUMN IF NOT EXISTS created_ts BIGINT;

-- ============================================
-- 10. E2EE TABLES
-- ============================================

CREATE TABLE IF NOT EXISTS e2ee_key_requests (
    request_id TEXT PRIMARY KEY,
    user_id TEXT NOT NULL,
    device_id TEXT NOT NULL,
    room_id TEXT NOT NULL,
    session_id TEXT NOT NULL,
    algorithm TEXT NOT NULL,
    action TEXT NOT NULL,
    created_ts BIGINT NOT NULL,
    fulfilled BOOLEAN DEFAULT FALSE,
    fulfilled_by_device TEXT,
    fulfilled_ts BIGINT
);

CREATE TABLE IF NOT EXISTS e2ee_secret_storage_keys (
    key_id TEXT NOT NULL,
    user_id TEXT NOT NULL,
    algorithm TEXT NOT NULL,
    encrypted_key TEXT NOT NULL,
    public_key TEXT,
    signatures JSONB,
    created_ts BIGINT NOT NULL,
    PRIMARY KEY (user_id, key_id)
);

CREATE TABLE IF NOT EXISTS e2ee_stored_secrets (
    user_id TEXT NOT NULL,
    secret_name TEXT NOT NULL,
    encrypted_secret TEXT NOT NULL,
    key_id TEXT NOT NULL,
    PRIMARY KEY (user_id, secret_name)
);

CREATE TABLE IF NOT EXISTS e2ee_ssss (
    user_id TEXT PRIMARY KEY,
    key_data JSONB NOT NULL,
    created_ts BIGINT NOT NULL,
    updated_ts BIGINT
);

-- ============================================
-- 11. FEDERATION TABLES
-- ============================================

CREATE TABLE IF NOT EXISTS federation_signing_keys (
    key_id TEXT PRIMARY KEY,
    public_key TEXT NOT NULL,
    private_key TEXT,
    expires_at BIGINT,
    created_ts BIGINT NOT NULL DEFAULT EXTRACT(EPOCH FROM NOW()) * 1000
);

-- ============================================
-- 12. INDEXES
-- ============================================

CREATE INDEX IF NOT EXISTS idx_access_tokens_revoked ON access_tokens(revoked_ts) WHERE revoked_ts IS NOT NULL;
CREATE INDEX IF NOT EXISTS idx_token_blacklist_expires ON token_blacklist(expires_at) WHERE expires_at IS NOT NULL;
CREATE INDEX IF NOT EXISTS idx_e2ee_key_requests_user ON e2ee_key_requests(user_id);
CREATE INDEX IF NOT EXISTS idx_e2ee_key_requests_session ON e2ee_key_requests(session_id);

-- ============================================
-- 13. UPDATE STATISTICS
-- ============================================

ANALYZE users;
ANALYZE devices;
ANALYZE refresh_tokens;
ANALYZE access_tokens;
ANALYZE token_blacklist;
ANALYZE user_threepids;
ANALYZE rooms;
ANALYZE events;
ANALYZE presence;
