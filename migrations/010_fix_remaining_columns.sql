-- Fix remaining column issues

-- Add missing columns to voice_messages table
DO $$
BEGIN
    IF NOT EXISTS (
        SELECT 1 FROM information_schema.columns
        WHERE table_name = 'voice_messages' AND column_name = 'created_ts'
    ) THEN
        ALTER TABLE voice_messages ADD COLUMN created_ts BIGINT NOT NULL DEFAULT EXTRACT(EPOCH FROM NOW())::BIGINT;
    END IF;
END $$;

-- Add missing columns to private_sessions table
DO $$
BEGIN
    IF NOT EXISTS (
        SELECT 1 FROM information_schema.columns
        WHERE table_name = 'private_sessions' AND column_name = 'user_id_2'
    ) THEN
        ALTER TABLE private_sessions ADD COLUMN user_id_2 VARCHAR(255);
    END IF;
END $$;

\echo '✅ Remaining column issues fixed!'
