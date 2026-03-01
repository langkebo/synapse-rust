-- Fix email_verification_tokens table column name
-- The code uses 'expires_ts' but the schema defined 'expires_at'
-- This migration adds the missing column and ensures compatibility

-- Add expires_ts column if it doesn't exist
DO $$ 
BEGIN
    IF NOT EXISTS (
        SELECT 1 FROM information_schema.columns 
        WHERE table_name = 'email_verification_tokens' 
        AND column_name = 'expires_ts'
    ) THEN
        ALTER TABLE email_verification_tokens ADD COLUMN expires_ts BIGINT;
        
        -- Copy data from expires_at to expires_ts if exists
        IF EXISTS (
            SELECT 1 FROM information_schema.columns 
            WHERE table_name = 'email_verification_tokens' 
            AND column_name = 'expires_at'
        ) THEN
            UPDATE email_verification_tokens 
            SET expires_ts = EXTRACT(EPOCH FROM expires_at) 
            WHERE expires_ts IS NULL AND expires_at IS NOT NULL;
        END IF;
    END IF;
END $$;

-- Add created_ts column if it doesn't exist
DO $$ 
BEGIN
    IF NOT EXISTS (
        SELECT 1 FROM information_schema.columns 
        WHERE table_name = 'email_verification_tokens' 
        AND column_name = 'created_ts'
    ) THEN
        ALTER TABLE email_verification_tokens ADD COLUMN created_ts BIGINT DEFAULT 0;
        
        -- Copy data from created_at to created_ts if exists
        IF EXISTS (
            SELECT 1 FROM information_schema.columns 
            WHERE table_name = 'email_verification_tokens' 
            AND column_name = 'created_at'
        ) THEN
            UPDATE email_verification_tokens 
            SET created_ts = EXTRACT(EPOCH FROM created_at) 
            WHERE created_ts = 0 AND created_at IS NOT NULL;
        END IF;
    END IF;
END $$;

-- Add used column if it doesn't exist
DO $$ 
BEGIN
    IF NOT EXISTS (
        SELECT 1 FROM information_schema.columns 
        WHERE table_name = 'email_verification_tokens' 
        AND column_name = 'used'
    ) THEN
        ALTER TABLE email_verification_tokens ADD COLUMN used BOOLEAN DEFAULT FALSE;
    END IF;
END $$;

-- Add session_data column if it doesn't exist
DO $$ 
BEGIN
    IF NOT EXISTS (
        SELECT 1 FROM information_schema.columns 
        WHERE table_name = 'email_verification_tokens' 
        AND column_name = 'session_data'
    ) THEN
        ALTER TABLE email_verification_tokens ADD COLUMN session_data JSONB;
    END IF;
END $$;

-- Create indexes for performance
CREATE INDEX IF NOT EXISTS idx_email_verification_tokens_expires_ts 
ON email_verification_tokens(expires_ts);

CREATE INDEX IF NOT EXISTS idx_email_verification_tokens_created_ts 
ON email_verification_tokens(created_ts);

CREATE INDEX IF NOT EXISTS idx_email_verification_tokens_used 
ON email_verification_tokens(used);
