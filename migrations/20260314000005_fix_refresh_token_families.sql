-- Migration: Fix refresh_token_families field naming
-- Date: 2026-03-13
-- Description: Rename fields to comply with naming standards

-- Rename column in refresh_token_families table
ALTER TABLE refresh_token_families RENAME COLUMN last_refresh_at TO last_refresh_ts;

-- Add missing columns if they don't exist
DO $$
BEGIN
    -- Add last_refresh_ts if it doesn't exist (for new installations)
    IF NOT EXISTS (
        SELECT 1 FROM information_schema.columns 
        WHERE table_name = 'refresh_token_families' 
        AND column_name = 'last_refresh_ts'
    ) THEN
        ALTER TABLE refresh_token_families ADD COLUMN last_refresh_ts BIGINT;
    END IF;
END $$;
