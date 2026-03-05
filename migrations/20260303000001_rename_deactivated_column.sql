-- Migration: Rename deactivated to is_deactivated
-- Description: Standardize boolean column naming with is_ prefix
-- Version: 20260303000001

-- Rename deactivated to is_deactivated in users table
DO $$
BEGIN
    IF EXISTS (
        SELECT 1 FROM information_schema.columns 
        WHERE table_name = 'users' AND column_name = 'deactivated'
    ) THEN
        ALTER TABLE users RENAME COLUMN deactivated TO is_deactivated;
    END IF;
END $$;

-- Rename is_shadow_banned if needed (ensure consistency)
DO $$
BEGIN
    IF EXISTS (
        SELECT 1 FROM information_schema.columns 
        WHERE table_name = 'users' AND column_name = 'shadow_banned'
    ) THEN
        ALTER TABLE users RENAME COLUMN shadow_banned TO is_shadow_banned;
    END IF;
END $$;

-- Add is_deactivated column if it doesn't exist
DO $$
BEGIN
    IF NOT EXISTS (
        SELECT 1 FROM information_schema.columns 
        WHERE table_name = 'users' AND column_name = 'is_deactivated'
    ) THEN
        ALTER TABLE users ADD COLUMN is_deactivated BOOLEAN DEFAULT FALSE;
    END IF;
END $$;

-- Update any NULL values
UPDATE users SET is_deactivated = FALSE WHERE is_deactivated IS NULL;

-- Make column NOT NULL
ALTER TABLE users ALTER COLUMN is_deactivated SET NOT NULL;

-- Add updated_ts column if it doesn't exist
DO $$
BEGIN
    IF NOT EXISTS (
        SELECT 1 FROM information_schema.columns 
        WHERE table_name = 'users' AND column_name = 'updated_ts'
    ) THEN
        ALTER TABLE users ADD COLUMN updated_ts BIGINT;
    END IF;
END $$;

-- Add user_type column if it doesn't exist
DO $$
BEGIN
    IF NOT EXISTS (
        SELECT 1 FROM information_schema.columns 
        WHERE table_name = 'users' AND column_name = 'user_type'
    ) THEN
        ALTER TABLE users ADD COLUMN user_type TEXT DEFAULT NULL;
    END IF;
END $$;

-- Add consent_version column if it doesn't exist
DO $$
BEGIN
    IF NOT EXISTS (
        SELECT 1 FROM information_schema.columns 
        WHERE table_name = 'users' AND column_name = 'consent_version'
    ) THEN
        ALTER TABLE users ADD COLUMN consent_version TEXT;
    END IF;
END $$;

-- Add appservice_id column if it doesn't exist
DO $$
BEGIN
    IF NOT EXISTS (
        SELECT 1 FROM information_schema.columns 
        WHERE table_name = 'users' AND column_name = 'appservice_id'
    ) THEN
        ALTER TABLE users ADD COLUMN appservice_id TEXT;
    END IF;
END $$;

-- Add invalid_update_ts column if it doesn't exist
DO $$
BEGIN
    IF NOT EXISTS (
        SELECT 1 FROM information_schema.columns 
        WHERE table_name = 'users' AND column_name = 'invalid_update_ts'
    ) THEN
        ALTER TABLE users ADD COLUMN invalid_update_ts BIGINT;
    END IF;
END $$;

-- Add migration_state column if it doesn't exist
DO $$
BEGIN
    IF NOT EXISTS (
        SELECT 1 FROM information_schema.columns 
        WHERE table_name = 'users' AND column_name = 'migration_state'
    ) THEN
        ALTER TABLE users ADD COLUMN migration_state TEXT;
    END IF;
END $$;

-- Add devices table missing columns
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

-- Add refresh_tokens table missing columns
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
