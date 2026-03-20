-- Migration: Fix missing database columns discovered during testing
-- Date: 2026-03-13
-- Description: 
--   - Add missing guest_access column to rooms table
--   - Add missing parent_id column to rooms table (for space hierarchy)
--   This is needed for room hierarchy endpoint

-- Add guest_access column to rooms table
ALTER TABLE rooms ADD COLUMN IF NOT EXISTS guest_access VARCHAR(50) DEFAULT 'can_join';

-- Add parent_id column for space/room hierarchy
ALTER TABLE rooms ADD COLUMN IF NOT EXISTS parent_id VARCHAR(255);

-- Create migration to track applied fixes
DO $$
BEGIN
    RAISE NOTICE 'Database fix migration applied successfully';
    
    -- List of known issues fixed:
    -- 1. room_ephemeral.expires_at -> expires_ts (previous migration)
    -- 2. sync_stream_id.stream_type NOT NULL removed (previous migration)
    -- 3. events.processed_ts exists (previous migration)
    -- 4. rooms.guest_access added (this migration)
    -- 5. rooms.parent_id added (this migration)
END $$;
