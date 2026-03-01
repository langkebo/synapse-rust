-- Fix email_verification_tokens table column types
-- The code uses BIGINT for timestamps but schema defined TIMESTAMP WITH TIME ZONE

-- First, drop the incorrectly typed columns if they exist
ALTER TABLE email_verification_tokens DROP COLUMN IF EXISTS expires_ts;
ALTER TABLE email_verification_tokens DROP COLUMN IF EXISTS created_ts;

-- Add columns with correct types (BIGINT for Unix timestamps)
ALTER TABLE email_verification_tokens ADD COLUMN expires_ts BIGINT;
ALTER TABLE email_verification_tokens ADD COLUMN created_ts BIGINT DEFAULT 0;

-- Migrate data from timestamp columns to bigint columns
UPDATE email_verification_tokens 
SET expires_ts = EXTRACT(EPOCH FROM expires_at)::BIGINT 
WHERE expires_at IS NOT NULL;

UPDATE email_verification_tokens 
SET created_ts = EXTRACT(EPOCH FROM created_at)::BIGINT 
WHERE created_at IS NOT NULL;

-- Add other missing columns
ALTER TABLE email_verification_tokens ADD COLUMN IF NOT EXISTS used BOOLEAN DEFAULT FALSE;
ALTER TABLE email_verification_tokens ADD COLUMN IF NOT EXISTS session_data JSONB;

-- Create indexes for performance
CREATE INDEX IF NOT EXISTS idx_email_verification_tokens_expires_ts 
ON email_verification_tokens(expires_ts);

CREATE INDEX IF NOT EXISTS idx_email_verification_tokens_created_ts 
ON email_verification_tokens(created_ts);

CREATE INDEX IF NOT EXISTS idx_email_verification_tokens_used 
ON email_verification_tokens(used);
