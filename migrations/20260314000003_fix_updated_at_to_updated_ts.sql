-- Migration: Fix updated_at to updated_ts for field naming consistency
-- Date: 2026-03-13
-- Description: Rename all updated_at columns to updated_ts to comply with field naming standards
-- Reference: DATABASE_FIELD_STANDARDS.md

-- This migration ensures all tables use updated_ts instead of updated_at
-- According to the standard: _ts suffix for NOT NULL timestamps, _at for nullable timestamps
-- updated_ts is typically nullable (can be NULL until first update)

DO $$
DECLARE
    t RECORD;
BEGIN
    -- Rename updated_at to updated_ts in all tables that have it
    FOR t IN 
        SELECT DISTINCT table_name 
        FROM information_schema.columns 
        WHERE column_name = 'updated_at' 
        AND table_schema = 'public'
    LOOP
        EXECUTE format('ALTER TABLE %I RENAME COLUMN updated_at TO updated_ts', t.table_name);
        RAISE NOTICE 'Renamed updated_at to updated_ts in table %', t.table_name;
    END LOOP;
END $$;

-- Update specific tables that need special handling

-- Table: sync_stream_id (change last_updated_ts to updated_ts if exists)
DO $$
BEGIN
    IF EXISTS (
        SELECT 1 FROM information_schema.columns 
        WHERE table_name = 'sync_stream_id' AND column_name = 'last_updated_ts'
    ) THEN
        ALTER TABLE sync_stream_id RENAME COLUMN last_updated_ts TO updated_ts;
    END IF;
END $$;

-- Table: password_policy (ensure updated_ts exists)
DO $$
BEGIN
    IF EXISTS (
        SELECT 1 FROM information_schema.columns 
        WHERE table_name = 'password_policy' AND column_name = 'updated_at'
    ) THEN
        ALTER TABLE password_policy RENAME COLUMN updated_at TO updated_ts;
    END IF;
END $$;

-- Verify the changes
DO $$
DECLARE
    remaining_count INTEGER;
BEGIN
    SELECT COUNT(*) INTO remaining_count
    FROM information_schema.columns 
    WHERE column_name = 'updated_at' 
    AND table_schema = 'public';
    
    IF remaining_count > 0 THEN
        RAISE WARNING 'There are still % tables with updated_at column', remaining_count;
    ELSE
        RAISE NOTICE 'All updated_at columns have been renamed to updated_ts';
    END IF;
END $$;

-- Create indexes for commonly queried updated_ts fields
CREATE INDEX IF NOT EXISTS idx_users_updated_ts ON users(updated_ts) WHERE updated_ts IS NOT NULL;
CREATE INDEX IF NOT EXISTS idx_devices_updated_ts ON devices(updated_ts) WHERE updated_ts IS NOT NULL;
CREATE INDEX IF NOT EXISTS idx_rooms_updated_ts ON rooms(updated_ts) WHERE updated_ts IS NOT NULL;
CREATE INDEX IF NOT EXISTS idx_room_memberships_updated_ts ON room_memberships(updated_ts) WHERE updated_ts IS NOT NULL;
CREATE INDEX IF NOT EXISTS idx_events_updated_ts ON events(updated_ts) WHERE updated_ts IS NOT NULL;

-- Update field standards documentation reference
-- Note: This migration aligns with DATABASE_FIELD_STANDARDS.md
-- - updated_ts: BIGINT NULLABLE - Millisecond timestamp for last update
-- - created_ts: BIGINT NOT NULL - Millisecond timestamp for creation
