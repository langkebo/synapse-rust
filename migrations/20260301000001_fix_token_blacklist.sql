-- ============================================================================
-- Fix token_blacklist table schema
-- ============================================================================
-- Description: Ensures token_blacklist table has all required columns
-- ============================================================================

-- Add token column if not exists
DO $$
BEGIN
    IF NOT EXISTS (
        SELECT 1 FROM information_schema.columns 
        WHERE table_name = 'token_blacklist' AND column_name = 'token'
    ) THEN
        ALTER TABLE token_blacklist ADD COLUMN token TEXT;
    END IF;
END $$;

-- Ensure token_type column exists with default
DO $$
BEGIN
    IF NOT EXISTS (
        SELECT 1 FROM information_schema.columns 
        WHERE table_name = 'token_blacklist' AND column_name = 'token_type'
    ) THEN
        ALTER TABLE token_blacklist ADD COLUMN token_type TEXT DEFAULT 'access';
    END IF;
END $$;

-- Update existing NULL values
UPDATE token_blacklist SET token_type = 'access' WHERE token_type IS NULL;

-- Ensure revoked_at column exists
DO $$
BEGIN
    IF NOT EXISTS (
        SELECT 1 FROM information_schema.columns 
        WHERE table_name = 'token_blacklist' AND column_name = 'revoked_at'
    ) THEN
        ALTER TABLE token_blacklist ADD COLUMN revoked_at BIGINT;
        UPDATE token_blacklist SET revoked_at = revoked_ts WHERE revoked_at IS NULL;
    END IF;
END $$;

-- Ensure expires_ts column exists
DO $$
BEGIN
    IF NOT EXISTS (
        SELECT 1 FROM information_schema.columns 
        WHERE table_name = 'token_blacklist' AND column_name = 'expires_ts'
    ) THEN
        ALTER TABLE token_blacklist ADD COLUMN expires_ts BIGINT;
        UPDATE token_blacklist SET expires_ts = expires_at WHERE expires_ts IS NULL;
    END IF;
END $$;

-- Create missing indexes if not exist
CREATE INDEX IF NOT EXISTS idx_token_blacklist_hash ON token_blacklist(token_hash);
CREATE INDEX IF NOT EXISTS idx_token_blacklist_user ON token_blacklist(user_id);
CREATE INDEX IF NOT EXISTS idx_token_blacklist_expires ON token_blacklist(expires_ts) WHERE expires_ts IS NOT NULL;
