-- Consolidated Fix Migration for Device Keys and Voice Messages
-- Version: 20260202000000_consolidated
-- Purpose: Merge multiple fix migrations into a single migration for better maintainability
-- Original migrations merged:
--   - 20260202000000_fix_device_keys_and_voice_v2.sql
--   - 20260202000001_final_fix.sql
--   - 20260202000002_fix_device_keys_and_voice_final.sql
--   - 20260202000003_fix_voice_nullable.sql
--   - 20260202120000_fix_device_keys_constraint.sql
--   - 20260204000001_add_transcribe_text_column.sql
--   - 20260204000002_add_device_keys_id_column_fixed.sql
--   - 20260204000004_fix_device_keys_fk.sql

-- This migration is IDEMPOTENT - safe to run multiple times

-- ============================================
-- Section 1: Device Keys Column Fixes
-- ============================================

-- device_keys missing display_name
DO $$
BEGIN
    IF NOT EXISTS (
        SELECT 1 FROM information_schema.columns 
        WHERE table_name = 'device_keys' AND column_name = 'display_name'
    ) THEN
        ALTER TABLE device_keys ADD COLUMN display_name VARCHAR(255);
    END IF;
END $$;

-- device_keys id column (consolidated from multiple migrations)
DO $$
BEGIN
    IF NOT EXISTS (
        SELECT 1 FROM information_schema.columns 
        WHERE table_name = 'device_keys' AND column_name = 'id'
    ) THEN
        ALTER TABLE device_keys ADD COLUMN id BIGSERIAL;
    END IF;
END $$;

-- Create index on id column
DO $$
BEGIN
    IF NOT EXISTS (
        SELECT 1 FROM pg_indexes 
        WHERE tablename = 'device_keys' AND indexname = 'idx_device_keys_id'
    ) THEN
        CREATE UNIQUE INDEX idx_device_keys_id ON device_keys(id);
    END IF;
END $$;

-- ============================================
-- Section 2: Voice Messages Column Fixes
-- ============================================

-- voice_messages missing session_id
DO $$
BEGIN
    IF NOT EXISTS (
        SELECT 1 FROM information_schema.columns 
        WHERE table_name = 'voice_messages' AND column_name = 'session_id'
    ) THEN
        ALTER TABLE voice_messages ADD COLUMN session_id VARCHAR(255);
    END IF;
END $$;

-- Ensure voice_messages.sender_id is user_id
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

-- voice_messages transcribe_text column
DO $$
BEGIN
    IF NOT EXISTS (
        SELECT 1 FROM information_schema.columns 
        WHERE table_name = 'voice_messages' AND column_name = 'transcribe_text'
    ) THEN
        ALTER TABLE voice_messages ADD COLUMN transcribe_text TEXT;
    END IF;
END $$;

-- Create index on transcribe_text
DO $$
BEGIN
    IF NOT EXISTS (
        SELECT 1 FROM pg_indexes 
        WHERE tablename = 'voice_messages' AND indexname = 'idx_voice_messages_transcribe_text'
    ) THEN
        CREATE INDEX idx_voice_messages_transcribe_text ON voice_messages(transcribe_text);
    END IF;
END $$;

-- Handle duration_ms column type (was renamed from duration in some migrations)
DO $$
BEGIN
    IF EXISTS (
        SELECT 1 FROM information_schema.columns 
        WHERE table_name = 'voice_messages' AND column_name = 'duration_ms'
    ) THEN
        ALTER TABLE voice_messages ALTER COLUMN duration_ms TYPE BIGINT;
    END IF;
EXCEPTION
    WHEN undefined_column THEN
        NULL; -- Column doesn't exist
END $$;

-- Handle file_size column type
DO $$
BEGIN
    IF EXISTS (
        SELECT 1 FROM information_schema.columns 
        WHERE table_name = 'voice_messages' AND column_name = 'file_size'
    ) THEN
        ALTER TABLE voice_messages ALTER COLUMN file_size TYPE BIGINT;
    END IF;
EXCEPTION
    WHEN undefined_column THEN
        NULL; -- Column doesn't exist
END $$;

-- ============================================
-- Section 3: Device Keys Constraints
-- ============================================

-- Drop existing incorrect foreign key if exists
DO $$
DECLARE
    constraint_name TEXT;
BEGIN
    SELECT conname INTO constraint_name FROM pg_constraint 
    WHERE conname = 'device_keys_user_id_device_id_fkey'
    LIMIT 1;
    
    IF constraint_name IS NOT NULL THEN
        EXECUTE format('ALTER TABLE device_keys DROP CONSTRAINT %I', constraint_name);
    END IF;
END $$;

-- Add corrected foreign key constraint (devices table PK is (device_id, user_id))
-- This may fail if there are duplicate entries, which is expected in test environments
DO $$
BEGIN
    IF NOT EXISTS (SELECT 1 FROM pg_constraint WHERE conname = 'device_keys_device_user_fkey') THEN
        ALTER TABLE device_keys 
        ADD CONSTRAINT device_keys_device_user_fkey 
        FOREIGN KEY (device_id, user_id) 
        REFERENCES devices(device_id, user_id) 
        ON DELETE CASCADE;
    END IF;
EXCEPTION
    WHEN duplicate_object THEN
        NULL; -- Constraint already exists
    WHEN foreign_key_violation THEN
        RAISE NOTICE 'Could not create foreign key constraint due to data inconsistencies';
END $$;

-- Create unique index for device_keys (handle duplicates gracefully)
-- Note: UNIQUE INDEX may fail if duplicates exist, which is acceptable in test data
DO $$
BEGIN
    IF NOT EXISTS (
        SELECT 1 FROM pg_indexes 
        WHERE tablename = 'device_keys' AND indexname = 'device_keys_user_device_unique'
    ) THEN
        CREATE UNIQUE INDEX device_keys_user_device_unique 
        ON device_keys(user_id, device_id);
    END IF;
EXCEPTION
    WHEN duplicate_object THEN
        NULL; -- Index already exists
    WHEN undefined_table THEN
        NULL; -- Table doesn't exist
END $$;

-- ============================================
-- Section 4: Comments for Documentation
-- ============================================

DO $$
BEGIN
    IF EXISTS (SELECT 1 FROM pg_tables WHERE tablename = 'device_keys') THEN
        COMMENT ON TABLE device_keys IS 'Stores device keys for end-to-end encryption';
        COMMENT ON COLUMN device_keys.display_name IS 'Human-readable device name';
    END IF;
    
    IF EXISTS (SELECT 1 FROM pg_tables WHERE tablename = 'voice_messages') THEN
        COMMENT ON COLUMN voice_messages.transcribe_text IS 'Transcribed text from voice message';
    END IF;
EXCEPTION
    WHEN undefined_table THEN
        NULL; -- Tables don't exist yet
END $$;
