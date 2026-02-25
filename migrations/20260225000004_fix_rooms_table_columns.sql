-- Add missing columns to rooms table
-- This migration fixes schema inconsistencies between code and database

DO $$
BEGIN
    -- Add name column if not exists
    IF NOT EXISTS (SELECT 1 FROM information_schema.columns WHERE table_name = 'rooms' AND column_name = 'name') THEN
        ALTER TABLE rooms ADD COLUMN name VARCHAR(255);
        RAISE NOTICE 'Added name column to rooms table';
    END IF;

    -- Add topic column if not exists
    IF NOT EXISTS (SELECT 1 FROM information_schema.columns WHERE table_name = 'rooms' AND column_name = 'topic') THEN
        ALTER TABLE rooms ADD COLUMN topic TEXT;
        RAISE NOTICE 'Added topic column to rooms table';
    END IF;

    -- Add avatar_url column if not exists
    IF NOT EXISTS (SELECT 1 FROM information_schema.columns WHERE table_name = 'rooms' AND column_name = 'avatar_url') THEN
        ALTER TABLE rooms ADD COLUMN avatar_url TEXT;
        RAISE NOTICE 'Added avatar_url column to rooms table';
    END IF;

    -- Add canonical_alias column if not exists
    IF NOT EXISTS (SELECT 1 FROM information_schema.columns WHERE table_name = 'rooms' AND column_name = 'canonical_alias') THEN
        ALTER TABLE rooms ADD COLUMN canonical_alias VARCHAR(255);
        RAISE NOTICE 'Added canonical_alias column to rooms table';
    END IF;

    -- Add member_count column if not exists
    IF NOT EXISTS (SELECT 1 FROM information_schema.columns WHERE table_name = 'rooms' AND column_name = 'member_count') THEN
        ALTER TABLE rooms ADD COLUMN member_count BIGINT DEFAULT 0;
        RAISE NOTICE 'Added member_count column to rooms table';
    END IF;

    -- Add history_visibility column if not exists
    IF NOT EXISTS (SELECT 1 FROM information_schema.columns WHERE table_name = 'rooms' AND column_name = 'history_visibility') THEN
        ALTER TABLE rooms ADD COLUMN history_visibility VARCHAR(50) DEFAULT 'joined';
        RAISE NOTICE 'Added history_visibility column to rooms table';
    END IF;

    -- Add encryption column if not exists
    IF NOT EXISTS (SELECT 1 FROM information_schema.columns WHERE table_name = 'rooms' AND column_name = 'encryption') THEN
        ALTER TABLE rooms ADD COLUMN encryption VARCHAR(50);
        RAISE NOTICE 'Added encryption column to rooms table';
    END IF;

    -- Add last_activity_ts column if not exists
    IF NOT EXISTS (SELECT 1 FROM information_schema.columns WHERE table_name = 'rooms' AND column_name = 'last_activity_ts') THEN
        ALTER TABLE rooms ADD COLUMN last_activity_ts BIGINT;
        RAISE NOTICE 'Added last_activity_ts column to rooms table';
    END IF;

    -- Rename created_ts to creation_ts if needed
    IF EXISTS (SELECT 1 FROM information_schema.columns WHERE table_name = 'rooms' AND column_name = 'created_ts') THEN
        ALTER TABLE rooms RENAME COLUMN created_ts TO creation_ts;
        RAISE NOTICE 'Renamed created_ts to creation_ts';
    END IF;

    RAISE NOTICE 'Rooms table schema updated successfully';
END $$;

-- Create indexes for the new columns
CREATE INDEX IF NOT EXISTS idx_rooms_name ON rooms(name);
CREATE INDEX IF NOT EXISTS idx_rooms_member_count ON rooms(member_count);
