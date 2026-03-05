-- Migration: Add ts column to notifications table
-- Date: 2026-03-01
-- Description: Add ts column for timestamp_to_event and notifications API

-- Add ts column to notifications table if not exists
DO $$
BEGIN
    IF NOT EXISTS (
        SELECT 1 FROM information_schema.columns 
        WHERE table_name = 'notifications' AND column_name = 'ts'
    ) THEN
        ALTER TABLE notifications ADD COLUMN ts BIGINT;
        RAISE NOTICE 'Added ts column to notifications table';
    END IF;
END $$;

-- Update existing records to set ts from created_ts
UPDATE notifications SET ts = created_ts WHERE ts IS NULL;

-- Create index for ts column
CREATE INDEX IF NOT EXISTS idx_notifications_ts ON notifications(ts DESC);

-- Add comment
COMMENT ON COLUMN notifications.ts IS 'Notification timestamp for API compatibility';
