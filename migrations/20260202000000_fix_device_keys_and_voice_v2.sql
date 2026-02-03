-- Fix missing columns for E2EE and Voice messages
-- Version: 20260202000000

-- device_keys missing display_name
ALTER TABLE device_keys ADD COLUMN IF NOT EXISTS display_name VARCHAR(255);

-- voice_messages missing session_id
ALTER TABLE voice_messages ADD COLUMN IF NOT EXISTS session_id VARCHAR(255);

-- Ensure voice_messages.sender_id is user_id (handled in previous migration but double check)
DO $$
BEGIN
    IF EXISTS (
        SELECT 1 FROM information_schema.columns 
        WHERE table_name = 'voice_messages' AND column_name = 'sender_id'
    ) AND NOT EXISTS (
        SELECT 1 FROM information_schema.columns 
        WHERE table_name = 'voice_messages' AND column_name = 'user_id'
    ) THEN
        ALTER TABLE voice_messages RENAME COLUMN sender_id TO user_id;
    END IF;
END $$;
