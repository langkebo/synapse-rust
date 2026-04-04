-- Rollback script for 20260330000001_add_thread_replies_and_receipts.sql
-- This script reverses the schema changes made by the forward migration
-- Note: This rollback is for emergency use only. Data loss may occur.

-- Drop new tables (data will be lost)
DROP TABLE IF EXISTS thread_read_receipts;
DROP TABLE IF EXISTS thread_replies;

-- Reverse column/constraint/index renames in thread_roots
DO $$
BEGIN
    -- Rename columns back
    IF EXISTS (
        SELECT 1 FROM information_schema.columns
        WHERE table_schema = 'public' AND table_name = 'thread_roots' AND column_name = 'root_event_id'
    ) AND NOT EXISTS (
        SELECT 1 FROM information_schema.columns
        WHERE table_schema = 'public' AND table_name = 'thread_roots' AND column_name = 'event_id'
    ) THEN
        ALTER TABLE thread_roots RENAME COLUMN root_event_id TO event_id;
    END IF;

    -- Rename constraints back
    IF EXISTS (
        SELECT 1 FROM pg_constraint
        WHERE conname = 'uq_thread_roots_room_root_event'
    ) AND NOT EXISTS (
        SELECT 1 FROM pg_constraint
        WHERE conname = 'uq_thread_roots_room_event'
    ) THEN
        ALTER TABLE thread_roots
        RENAME CONSTRAINT uq_thread_roots_room_root_event TO uq_thread_roots_room_event;
    END IF;

    -- Rename indexes back
    IF EXISTS (
        SELECT 1 FROM pg_indexes
        WHERE schemaname = 'public' AND indexname = 'idx_thread_roots_root_event'
    ) AND NOT EXISTS (
        SELECT 1 FROM pg_indexes
        WHERE schemaname = 'public' AND indexname = 'idx_thread_roots_event'
    ) THEN
        ALTER INDEX idx_thread_roots_root_event RENAME TO idx_thread_roots_event;
    END IF;
END $$;
