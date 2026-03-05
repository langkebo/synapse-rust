-- Migration: Fix schema inconsistencies discovered during API testing
-- Description: Add missing columns discovered during systematic API testing
-- Version: 20260303000002
-- Date: 2026-03-03

-- ============================================
-- 1. access_tokens table - Add revoked_ts column
-- ============================================
DO $$
BEGIN
    IF NOT EXISTS (
        SELECT 1 FROM information_schema.columns 
        WHERE table_name = 'access_tokens' AND column_name = 'revoked_ts'
    ) THEN
        ALTER TABLE access_tokens ADD COLUMN revoked_ts BIGINT;
    END IF;
END $$;

-- ============================================
-- 2. token_blacklist table - Add expires_at column
-- ============================================
DO $$
BEGIN
    IF NOT EXISTS (
        SELECT 1 FROM information_schema.columns 
        WHERE table_name = 'token_blacklist' AND column_name = 'expires_at'
    ) THEN
        ALTER TABLE token_blacklist ADD COLUMN expires_at BIGINT;
    END IF;
END $$;

-- ============================================
-- 3. user_threepids table - Add validated_at and added_at columns
-- ============================================
DO $$
BEGIN
    IF NOT EXISTS (
        SELECT 1 FROM information_schema.columns 
        WHERE table_name = 'user_threepids' AND column_name = 'validated_at'
    ) THEN
        ALTER TABLE user_threepids ADD COLUMN validated_at BIGINT;
    END IF;
END $$;

DO $$
BEGIN
    IF NOT EXISTS (
        SELECT 1 FROM information_schema.columns 
        WHERE table_name = 'user_threepids' AND column_name = 'added_at'
    ) THEN
        ALTER TABLE user_threepids ADD COLUMN added_at BIGINT;
    END IF;
END $$;

-- ============================================
-- 4. users table - Add any remaining missing columns
-- ============================================
DO $$
BEGIN
    IF NOT EXISTS (
        SELECT 1 FROM information_schema.columns 
        WHERE table_name = 'users' AND column_name = 'is_deactivated'
    ) THEN
        ALTER TABLE users ADD COLUMN is_deactivated BOOLEAN DEFAULT FALSE NOT NULL;
    END IF;
END $$;

DO $$
BEGIN
    IF NOT EXISTS (
        SELECT 1 FROM information_schema.columns 
        WHERE table_name = 'users' AND column_name = 'updated_ts'
    ) THEN
        ALTER TABLE users ADD COLUMN updated_ts BIGINT;
    END IF;
END $$;

DO $$
BEGIN
    IF NOT EXISTS (
        SELECT 1 FROM information_schema.columns 
        WHERE table_name = 'users' AND column_name = 'user_type'
    ) THEN
        ALTER TABLE users ADD COLUMN user_type TEXT;
    END IF;
END $$;

DO $$
BEGIN
    IF NOT EXISTS (
        SELECT 1 FROM information_schema.columns 
        WHERE table_name = 'users' AND column_name = 'consent_version'
    ) THEN
        ALTER TABLE users ADD COLUMN consent_version TEXT;
    END IF;
END $$;

DO $$
BEGIN
    IF NOT EXISTS (
        SELECT 1 FROM information_schema.columns 
        WHERE table_name = 'users' AND column_name = 'appservice_id'
    ) THEN
        ALTER TABLE users ADD COLUMN appservice_id TEXT;
    END IF;
END $$;

DO $$
BEGIN
    IF NOT EXISTS (
        SELECT 1 FROM information_schema.columns 
        WHERE table_name = 'users' AND column_name = 'invalid_update_ts'
    ) THEN
        ALTER TABLE users ADD COLUMN invalid_update_ts BIGINT;
    END IF;
END $$;

DO $$
BEGIN
    IF NOT EXISTS (
        SELECT 1 FROM information_schema.columns 
        WHERE table_name = 'users' AND column_name = 'migration_state'
    ) THEN
        ALTER TABLE users ADD COLUMN migration_state TEXT;
    END IF;
END $$;

-- ============================================
-- 5. devices table - Add missing columns
-- ============================================
DO $$
BEGIN
    IF NOT EXISTS (
        SELECT 1 FROM information_schema.columns 
        WHERE table_name = 'devices' AND column_name = 'first_seen_ts'
    ) THEN
        ALTER TABLE devices ADD COLUMN first_seen_ts BIGINT;
    END IF;
END $$;

DO $$
BEGIN
    IF NOT EXISTS (
        SELECT 1 FROM information_schema.columns 
        WHERE table_name = 'devices' AND column_name = 'device_key'
    ) THEN
        ALTER TABLE devices ADD COLUMN device_key JSONB;
    END IF;
END $$;

DO $$
BEGIN
    IF NOT EXISTS (
        SELECT 1 FROM information_schema.columns 
        WHERE table_name = 'devices' AND column_name = 'appservice_id'
    ) THEN
        ALTER TABLE devices ADD COLUMN appservice_id TEXT;
    END IF;
END $$;

DO $$
BEGIN
    IF NOT EXISTS (
        SELECT 1 FROM information_schema.columns 
        WHERE table_name = 'devices' AND column_name = 'ignored_user_list'
    ) THEN
        ALTER TABLE devices ADD COLUMN ignored_user_list TEXT;
    END IF;
END $$;

-- ============================================
-- 6. refresh_tokens table - Add missing columns
-- ============================================
DO $$
BEGIN
    IF NOT EXISTS (
        SELECT 1 FROM information_schema.columns 
        WHERE table_name = 'refresh_tokens' AND column_name = 'access_token_id'
    ) THEN
        ALTER TABLE refresh_tokens ADD COLUMN access_token_id TEXT;
    END IF;
END $$;

DO $$
BEGIN
    IF NOT EXISTS (
        SELECT 1 FROM information_schema.columns 
        WHERE table_name = 'refresh_tokens' AND column_name = 'scope'
    ) THEN
        ALTER TABLE refresh_tokens ADD COLUMN scope TEXT;
    END IF;
END $$;

DO $$
BEGIN
    IF NOT EXISTS (
        SELECT 1 FROM information_schema.columns 
        WHERE table_name = 'refresh_tokens' AND column_name = 'expires_at'
    ) THEN
        ALTER TABLE refresh_tokens ADD COLUMN expires_at BIGINT;
    END IF;
END $$;

DO $$
BEGIN
    IF NOT EXISTS (
        SELECT 1 FROM information_schema.columns 
        WHERE table_name = 'refresh_tokens' AND column_name = 'last_used_ts'
    ) THEN
        ALTER TABLE refresh_tokens ADD COLUMN last_used_ts BIGINT;
    END IF;
END $$;

DO $$
BEGIN
    IF NOT EXISTS (
        SELECT 1 FROM information_schema.columns 
        WHERE table_name = 'refresh_tokens' AND column_name = 'use_count'
    ) THEN
        ALTER TABLE refresh_tokens ADD COLUMN use_count INT DEFAULT 0;
    END IF;
END $$;

DO $$
BEGIN
    IF NOT EXISTS (
        SELECT 1 FROM information_schema.columns 
        WHERE table_name = 'refresh_tokens' AND column_name = 'revoked_reason'
    ) THEN
        ALTER TABLE refresh_tokens ADD COLUMN revoked_reason TEXT;
    END IF;
END $$;

DO $$
BEGIN
    IF NOT EXISTS (
        SELECT 1 FROM information_schema.columns 
        WHERE table_name = 'refresh_tokens' AND column_name = 'client_info'
    ) THEN
        ALTER TABLE refresh_tokens ADD COLUMN client_info JSONB;
    END IF;
END $$;

DO $$
BEGIN
    IF NOT EXISTS (
        SELECT 1 FROM information_schema.columns 
        WHERE table_name = 'refresh_tokens' AND column_name = 'ip_address'
    ) THEN
        ALTER TABLE refresh_tokens ADD COLUMN ip_address TEXT;
    END IF;
END $$;

DO $$
BEGIN
    IF NOT EXISTS (
        SELECT 1 FROM information_schema.columns 
        WHERE table_name = 'refresh_tokens' AND column_name = 'user_agent'
    ) THEN
        ALTER TABLE refresh_tokens ADD COLUMN user_agent TEXT;
    END IF;
END $$;

-- ============================================
-- 7. Create indexes for performance
-- ============================================
CREATE INDEX IF NOT EXISTS idx_access_tokens_revoked ON access_tokens(revoked_ts) WHERE revoked_ts IS NOT NULL;
CREATE INDEX IF NOT EXISTS idx_token_blacklist_expires ON token_blacklist(expires_at) WHERE expires_at IS NOT NULL;
CREATE INDEX IF NOT EXISTS idx_user_threepids_validated ON user_threepids(validated_at) WHERE validated_at IS NOT NULL;

-- ============================================
-- 8. Rename deprecated columns if they exist
-- ============================================
DO $$
BEGIN
    IF EXISTS (
        SELECT 1 FROM information_schema.columns 
        WHERE table_name = 'users' AND column_name = 'deactivated'
    ) AND NOT EXISTS (
        SELECT 1 FROM information_schema.columns 
        WHERE table_name = 'users' AND column_name = 'is_deactivated'
    ) THEN
        ALTER TABLE users RENAME COLUMN deactivated TO is_deactivated;
    END IF;
END $$;

DO $$
BEGIN
    IF EXISTS (
        SELECT 1 FROM information_schema.columns 
        WHERE table_name = 'users' AND column_name = 'shadow_banned'
    ) AND NOT EXISTS (
        SELECT 1 FROM information_schema.columns 
        WHERE table_name = 'users' AND column_name = 'is_shadow_banned'
    ) THEN
        ALTER TABLE users RENAME COLUMN shadow_banned TO is_shadow_banned;
    END IF;
END $$;

-- ============================================
-- 9. Update statistics
-- ============================================
ANALYZE users;
ANALYZE devices;
ANALYZE refresh_tokens;
ANALYZE access_tokens;
ANALYZE token_blacklist;
ANALYZE user_threepids;

-- ============================================
-- 10. Fix presence table - ensure created_ts has default
-- ============================================
DO $$
BEGIN
    IF EXISTS (
        SELECT 1 FROM information_schema.columns 
        WHERE table_name = 'presence' AND column_name = 'created_ts'
    ) THEN
        -- Set default for existing NULL values
        UPDATE presence SET created_ts = EXTRACT(EPOCH FROM NOW()) * 1000 WHERE created_ts IS NULL;
    END IF;
END $$;

-- ============================================
-- 11. Fix rooms table - remove duplicate created_ts column
-- ============================================
DO $$
BEGIN
    IF EXISTS (
        SELECT 1 FROM information_schema.columns 
        WHERE table_name = 'rooms' AND column_name = 'created_ts'
    ) AND EXISTS (
        SELECT 1 FROM information_schema.columns 
        WHERE table_name = 'rooms' AND column_name = 'created_ts'
    ) THEN
        ALTER TABLE rooms DROP COLUMN created_ts;
    END IF;
END $$;

-- ============================================
-- 12. Add missing columns to events table
-- ============================================
DO $$
BEGIN
    IF NOT EXISTS (
        SELECT 1 FROM information_schema.columns 
        WHERE table_name = 'events' AND column_name = 'depth'
    ) THEN
        ALTER TABLE events ADD COLUMN depth BIGINT DEFAULT 0;
    END IF;
END $$;

DO $$
BEGIN
    IF NOT EXISTS (
        SELECT 1 FROM information_schema.columns 
        WHERE table_name = 'events' AND column_name = 'processed_ts'
    ) THEN
        ALTER TABLE events ADD COLUMN processed_ts BIGINT;
    END IF;
END $$;

DO $$
BEGIN
    IF NOT EXISTS (
        SELECT 1 FROM information_schema.columns 
        WHERE table_name = 'events' AND column_name = 'not_before'
    ) THEN
        ALTER TABLE events ADD COLUMN not_before BIGINT DEFAULT 0;
    END IF;
END $$;

DO $$
BEGIN
    IF NOT EXISTS (
        SELECT 1 FROM information_schema.columns 
        WHERE table_name = 'events' AND column_name = 'status'
    ) THEN
        ALTER TABLE events ADD COLUMN status TEXT DEFAULT 'pending';
    END IF;
END $$;

DO $$
BEGIN
    IF NOT EXISTS (
        SELECT 1 FROM information_schema.columns 
        WHERE table_name = 'events' AND column_name = 'reference_image'
    ) THEN
        ALTER TABLE events ADD COLUMN reference_image TEXT;
    END IF;
END $$;

DO $$
BEGIN
    IF NOT EXISTS (
        SELECT 1 FROM information_schema.columns 
        WHERE table_name = 'events' AND column_name = 'origin'
    ) THEN
        ALTER TABLE events ADD COLUMN origin TEXT DEFAULT 'self';
    END IF;
END $$;

DO $$
BEGIN
    IF NOT EXISTS (
        SELECT 1 FROM information_schema.columns 
        WHERE table_name = 'events' AND column_name = 'unsigned'
    ) THEN
        ALTER TABLE events ADD COLUMN unsigned JSONB DEFAULT '{}';
    END IF;
END $$;
