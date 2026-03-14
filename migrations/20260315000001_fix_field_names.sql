-- Migration: Fix database field naming inconsistencies
-- Date: 2026-03-13
-- Description: Fix field names to match code expectations
--   - room_ephemeral: expires_at → expires_ts
--   - events: ensure processed_ts exists
--   - sync_stream_id: ensure stream_type is nullable

-- Fix room_ephemeral table (code uses expires_ts)
ALTER TABLE room_ephemeral RENAME COLUMN expires_at TO expires_ts;

-- Ensure events table has processed_ts column
ALTER TABLE events ADD COLUMN IF NOT EXISTS processed_ts BIGINT;

-- Ensure sync_stream_id.stream_type allows NULL (for DEFAULT VALUES inserts)
ALTER TABLE sync_stream_id ALTER COLUMN stream_type DROP NOT NULL;

-- Clean up duplicate processed_at column if exists (keep processed_ts)
-- Only run this if both columns exist
DO $$
BEGIN
    IF EXISTS (
        SELECT 1 FROM information_schema.columns 
        WHERE table_name = 'events' AND column_name = 'processed_at'
    ) AND EXISTS (
        SELECT 1 FROM information_schema.columns 
        WHERE table_name = 'events' AND column_name = 'processed_ts'
    ) THEN
        -- Copy data from processed_at to processed_ts if needed
        UPDATE events SET processed_ts = processed_at 
        WHERE processed_ts IS NULL AND processed_at IS NOT NULL;
        
        -- Drop the old column
        ALTER TABLE events DROP COLUMN IF EXISTS processed_at;
    END IF;
END $$;

-- Verify the changes
DO $$
BEGIN
    RAISE NOTICE 'Migration verification:';
END $$;
