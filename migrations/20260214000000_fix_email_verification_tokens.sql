-- Fix email_verification_tokens table schema
-- Add session_data column and make user_id nullable

-- Make user_id nullable (for registration flow where user doesn't exist yet)
ALTER TABLE email_verification_tokens ALTER COLUMN user_id DROP NOT NULL;

-- Add session_data column for storing registration session data
ALTER TABLE email_verification_tokens ADD COLUMN IF NOT EXISTS session_data JSONB;

-- Add comment
COMMENT ON COLUMN email_verification_tokens.session_data IS 'Stores session data for registration flow';

-- Verify the fix
SELECT column_name, data_type, is_nullable 
FROM information_schema.columns 
WHERE table_name = 'email_verification_tokens' 
ORDER BY ordinal_position;
