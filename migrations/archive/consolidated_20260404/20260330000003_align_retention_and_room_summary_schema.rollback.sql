-- Rollback script for 20260330000003_align_retention_and_room_summary_schema.sql
-- This script reverses the schema changes made by the forward migration
-- Note: This rollback is for emergency use only. Data loss may occur.

-- Drop new tables (data will be lost)
DROP TABLE IF EXISTS room_retention_policies;
DROP TABLE IF EXISTS room_summary_members;

-- Reverse column renames in room_summaries
DO $$
BEGIN
    IF EXISTS (
        SELECT 1 FROM information_schema.columns
        WHERE table_schema = 'public' AND table_name = 'room_summaries' AND column_name = 'joined_member_count'
    ) AND NOT EXISTS (
        SELECT 1 FROM information_schema.columns
        WHERE table_schema = 'public' AND table_name = 'room_summaries' AND column_name = 'joined_members'
    ) THEN
        ALTER TABLE room_summaries RENAME COLUMN joined_member_count TO joined_members;
    END IF;

    IF EXISTS (
        SELECT 1 FROM information_schema.columns
        WHERE table_schema = 'public' AND table_name = 'room_summaries' AND column_name = 'invited_member_count'
    ) AND NOT EXISTS (
        SELECT 1 FROM information_schema.columns
        WHERE table_schema = 'public' AND table_name = 'room_summaries' AND column_name = 'invited_members'
    ) THEN
        ALTER TABLE room_summaries RENAME COLUMN invited_member_count TO invited_members;
    END IF;
END $$;

-- Reverse column additions in room_summaries (PostgreSQL cannot remove columns easily)
-- This is marked as NOT REVERSIBLE for column additions
-- Run only if you understand the data loss implications

-- Reverse server_retention_policy column additions
DO $$
BEGIN
    IF EXISTS (
        SELECT 1 FROM information_schema.columns
        WHERE table_schema = 'public' AND table_name = 'server_retention_policy' AND column_name = 'max_lifetime'
    ) THEN
        -- Cannot easily drop columns, marked as NOT REVERSIBLE
        RAISE NOTICE 'server_retention_policy column additions are NOT REVERSIBLE automatically';
    END IF;
END $$;
