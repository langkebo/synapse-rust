-- ============================================================================
-- Migration: 20260302000001_fix_space_tables.sql
-- Created: 2026-03-02
-- Purpose: Fix missing columns and tables for Space functionality
-- ============================================================================

-- ============================================================================
-- 1. Fix space_members table - add updated_ts column
-- ============================================================================

DO $$ 
BEGIN
    IF NOT EXISTS (SELECT 1 FROM information_schema.columns 
                   WHERE table_name = 'space_members' AND column_name = 'updated_ts') THEN
        ALTER TABLE space_members ADD COLUMN updated_ts BIGINT;
        RAISE NOTICE 'Added updated_ts column to space_members';
    END IF;
END $$;

-- ============================================================================
-- 2. Create space_events table if not exists
-- ============================================================================

CREATE TABLE IF NOT EXISTS space_events (
    id BIGSERIAL PRIMARY KEY,
    event_id VARCHAR(255) UNIQUE NOT NULL,
    space_id VARCHAR(255) NOT NULL,
    event_type VARCHAR(255) NOT NULL,
    sender VARCHAR(255) NOT NULL,
    content JSONB,
    state_key VARCHAR(255),
    origin_server_ts BIGINT NOT NULL,
    processed_ts BIGINT
);

CREATE INDEX IF NOT EXISTS idx_space_events_space ON space_events(space_id);
CREATE INDEX IF NOT EXISTS idx_space_events_type ON space_events(event_type);
CREATE INDEX IF NOT EXISTS idx_space_events_sender ON space_events(sender);

-- ============================================================================
-- 3. Fix space_summaries table - add missing columns
-- ============================================================================

DO $$ 
BEGIN
    IF NOT EXISTS (SELECT 1 FROM information_schema.columns 
                   WHERE table_name = 'space_summaries' AND column_name = 'summary') THEN
        ALTER TABLE space_summaries ADD COLUMN summary JSONB;
        RAISE NOTICE 'Added summary column to space_summaries';
    END IF;
END $$;

DO $$ 
BEGIN
    IF NOT EXISTS (SELECT 1 FROM information_schema.columns 
                   WHERE table_name = 'space_summaries' AND column_name = 'children_count') THEN
        ALTER TABLE space_summaries ADD COLUMN children_count BIGINT DEFAULT 0;
        RAISE NOTICE 'Added children_count column to space_summaries';
    END IF;
END $$;

DO $$ 
BEGIN
    IF NOT EXISTS (SELECT 1 FROM information_schema.columns 
                   WHERE table_name = 'space_summaries' AND column_name = 'member_count') THEN
        ALTER TABLE space_summaries ADD COLUMN member_count BIGINT DEFAULT 0;
        RAISE NOTICE 'Added member_count column to space_summaries';
    END IF;
END $$;

-- ============================================================================
-- 4. Fix space_children table - ensure via_servers is TEXT[]
-- ============================================================================

-- Check if via_servers column exists and is JSONB, convert to TEXT[] if needed
DO $$ 
BEGIN
    -- Check if column exists and is JSONB type
    IF EXISTS (
        SELECT 1 FROM information_schema.columns 
        WHERE table_name = 'space_children' 
        AND column_name = 'via_servers' 
        AND data_type = 'jsonb'
    ) THEN
        -- Create a new column with TEXT[] type
        ALTER TABLE space_children ADD COLUMN via_servers_text TEXT[];
        
        -- Copy data (convert JSONB to TEXT[])
        UPDATE space_children 
        SET via_servers_text = ARRAY(
            SELECT jsonb_array_elements_text(via_servers)
        )
        WHERE via_servers IS NOT NULL;
        
        -- Drop old column
        ALTER TABLE space_children DROP COLUMN via_servers;
        
        -- Rename new column
        ALTER TABLE space_children RENAME COLUMN via_servers_text TO via_servers;
        
        RAISE NOTICE 'Converted via_servers from JSONB to TEXT[]';
    END IF;
END $$;

-- Add via_servers column if not exists (as TEXT[])
DO $$ 
BEGIN
    IF NOT EXISTS (SELECT 1 FROM information_schema.columns 
                   WHERE table_name = 'space_children' AND column_name = 'via_servers') THEN
        ALTER TABLE space_children ADD COLUMN via_servers TEXT[];
        RAISE NOTICE 'Added via_servers column to space_children as TEXT[]';
    END IF;
END $$;

-- ============================================================================
-- 5. Ensure unique constraints exist
-- ============================================================================

-- Ensure space_summaries has unique constraint on space_id
DO $$
BEGIN
    IF NOT EXISTS (SELECT 1 FROM pg_constraint WHERE conname = 'space_summaries_space_id_key') THEN
        IF EXISTS (SELECT 1 FROM information_schema.columns WHERE table_name = 'space_summaries' AND column_name = 'space_id') THEN
            ALTER TABLE space_summaries ADD CONSTRAINT space_summaries_space_id_key UNIQUE (space_id);
            RAISE NOTICE 'Added unique constraint on space_id for space_summaries';
        END IF;
    END IF;
END $$;

-- ============================================================================
-- 6. Migration Record
-- ============================================================================

INSERT INTO migrations (name, applied_at) 
VALUES ('20260302000001_fix_space_tables', NOW())
ON CONFLICT (name) DO NOTHING;

-- ============================================================================
-- 7. Verification
-- ============================================================================

DO $$
DECLARE
    space_members_cols INTEGER;
    space_events_exists BOOLEAN;
    space_summaries_cols INTEGER;
BEGIN
    SELECT COUNT(*) INTO space_members_cols 
    FROM information_schema.columns 
    WHERE table_name = 'space_members' AND column_name = 'updated_ts';
    
    SELECT EXISTS (
        SELECT 1 FROM information_schema.tables 
        WHERE table_name = 'space_events'
    ) INTO space_events_exists;
    
    SELECT COUNT(*) INTO space_summaries_cols 
    FROM information_schema.columns 
    WHERE table_name = 'space_summaries' 
    AND column_name IN ('summary', 'children_count', 'member_count');
    
    RAISE NOTICE 'Space tables fix completed: space_members.updated_ts=%, space_events=%, space_summaries cols=%', 
                 space_members_cols, space_events_exists, space_summaries_cols;
END $$;
